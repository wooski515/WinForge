//! Чтение (и безопасная запись) реальных данных системы: переменные PATH,
//! автозапуск, файл hosts, оборудование и живой монитор дисков.

use crate::sys::engine::os;
use crate::sys::reg::{self, Root, ValueData};

// ---------------------------------------------------------------------------
// Общее
// ---------------------------------------------------------------------------

/// Раскрывает %ПЕРЕМЕННЫЕ% окружения в строке.
pub fn expand(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        rest = &rest[i + 1..];
        if let Some(j) = rest.find('%') {
            let name = &rest[..j];
            match std::env::var(name) {
                Ok(v) => out.push_str(&v),
                Err(_) => {
                    out.push('%');
                    out.push_str(name);
                    out.push('%');
                }
            }
            rest = &rest[j + 1..];
        } else {
            out.push('%');
            break;
        }
    }
    out.push_str(rest);
    out
}

fn decode_utf16(bytes: &[u8]) -> String {
    let u16s: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&u16s)
        .trim_end_matches('\0')
        .to_string()
}

// ---------------------------------------------------------------------------
// PATH
// ---------------------------------------------------------------------------

fn path_target(system: bool) -> (Root, &'static str) {
    if system {
        (
            Root::Hklm,
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        )
    } else {
        (Root::Hkcu, "Environment")
    }
}

/// Записи PATH: (путь, отсутствует ли папка на диске).
pub fn read_path(system: bool) -> Vec<(String, bool)> {
    let (root, sub) = path_target(system);
    let text = reg::read_raw(root, sub, "Path")
        .map(|r| decode_utf16(&r.bytes))
        .unwrap_or_default();
    text.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            let exists = std::path::Path::new(&expand(s)).is_dir();
            (s.to_string(), !exists)
        })
        .collect()
}

/// Записывает PATH (как REG_EXPAND_SZ) и оповещает систему.
pub fn write_path(system: bool, entries: &[String]) -> Result<(), String> {
    let (root, sub) = path_target(system);
    let joined = entries.join(";");
    reg::write(root, sub, "Path", &ValueData::Expand(joined)).map_err(|e| e.to_string())?;
    broadcast_env();
    Ok(())
}

/// Рассылает окнам WM_SETTINGCHANGE с указанной областью ("Environment", "Policy"…).
#[cfg(windows)]
fn broadcast(area: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    let text: Vec<u16> = area.encode_utf16().chain(std::iter::once(0)).collect();
    let mut res: usize = 0;
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            text.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            3000,
            &mut res,
        );
    }
}

#[cfg(not(windows))]
fn broadcast(_area: &str) {}

fn broadcast_env() {
    broadcast("Environment");
}

/// Сообщает открытым окнам, что настройки изменились (после применения твиков).
pub fn broadcast_settings() {
    broadcast("Environment");
    broadcast("Policy");
    broadcast("ImmersiveColorSet");
}

// ---------------------------------------------------------------------------
// hosts
// ---------------------------------------------------------------------------

pub fn hosts_path() -> std::path::PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    std::path::PathBuf::from(root).join(r"System32\drivers\etc\hosts")
}

pub fn read_hosts() -> std::io::Result<String> {
    let bytes = std::fs::read(hosts_path())?;
    Ok(String::from_utf8_lossy(&bytes).replace("\r\n", "\n"))
}

/// Сохраняет hosts (CRLF), предварительно копируя старый файл в папку данных.
pub fn write_hosts(text: &str) -> std::io::Result<()> {
    let path = hosts_path();
    if let Ok(old) = std::fs::read(&path) {
        let dir = crate::sys::data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("hosts.bak"), old);
    }
    let normalized = text.replace("\r\n", "\n").replace('\n', "\r\n");
    std::fs::write(path, normalized)
}

// ---------------------------------------------------------------------------
// USB-флешки (для записи сборки)
// ---------------------------------------------------------------------------

/// Съёмный USB-диск, на который можно записать установку Windows.
#[derive(Clone, Debug, PartialEq)]
pub struct UsbDisk {
    /// Номер диска Windows (Get-Disk -Number).
    pub number: u32,
    pub name: String,
    pub size: u64,
    /// Серийный номер — скрипт сверяет его перед стиранием.
    pub serial: String,
    /// Буквы томов на флешке ("E:").
    pub letters: String,
}

impl UsbDisk {
    pub fn size_gb(&self) -> f64 {
        self.size as f64 / 1_000_000_000.0
    }
}

/// Только диски на шине USB, не системные и не загрузочные. Права админа не нужны.
pub fn usb_disks() -> Vec<UsbDisk> {
    let out = os::run(
        "powershell",
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
         Get-Disk | Where-Object { $_.BusType -eq 'USB' -and -not $_.IsBoot -and -not $_.IsSystem } | ForEach-Object { \
           $l = ($_ | Get-Partition -ErrorAction SilentlyContinue | Where-Object DriveLetter | ForEach-Object { \"$($_.DriveLetter):\" }) -join ' '; \
           \"USB|$($_.Number)|$($_.Size)|$(\"$($_.SerialNumber)\".Trim())|$l|$($_.FriendlyName)\" }",
    )
    .unwrap_or_default();
    parse_usb_disks(&out)
}

fn parse_usb_disks(out: &str) -> Vec<UsbDisk> {
    out.lines()
        .filter_map(|l| l.trim().strip_prefix("USB|"))
        .filter_map(|l| {
            let mut p = l.splitn(5, '|');
            Some(UsbDisk {
                number: p.next()?.trim().parse().ok()?,
                size: p.next()?.trim().parse().ok()?,
                serial: p.next()?.trim().to_string(),
                letters: p.next()?.trim().to_string(),
                name: p.next()?.trim().to_string(),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Редакции в ISO
// ---------------------------------------------------------------------------

/// Имена образов из заголовка WIM/ESD (XML-метаданные, без DISM и без прав админа).
pub fn read_wim_names(path: &std::path::Path) -> Result<Vec<String>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0u8; 208];
    f.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[0..8] != b"MSWIM\0\0\0" {
        return Err("це не WIM/ESD".into());
    }
    let u64_at = |o: usize| u64::from_le_bytes(header[o..o + 8].try_into().unwrap());
    // rhXmlData: RESHDR_DISK_SHORT по смещению 72 (7 байт размера + флаги, смещение, исходный размер).
    let xml_size = (u64_at(72) & 0x00FF_FFFF_FFFF_FFFF) as usize;
    let xml_offset = u64_at(80);
    if xml_size == 0 || xml_size > 16 * 1024 * 1024 {
        return Err("немає XML-метаданих".into());
    }
    f.seek(SeekFrom::Start(xml_offset)).map_err(|e| e.to_string())?;
    let mut raw = vec![0u8; xml_size];
    f.read_exact(&mut raw).map_err(|e| e.to_string())?;
    Ok(wim_xml_names(&decode_utf16(&raw)))
}

/// <IMAGE …><NAME>Windows 11 Pro</NAME>… → ["Windows 11 Pro", …]
fn wim_xml_names(xml: &str) -> Vec<String> {
    let mut names = Vec::new();
    for image in xml.split("<IMAGE").skip(1) {
        let image = image.split("</IMAGE>").next().unwrap_or("");
        if let Some(start) = image.find("<NAME>") {
            let rest = &image[start + 6..];
            if let Some(end) = rest.find("</NAME>") {
                names.push(rest[..end].trim().replace("&amp;", "&"));
            }
        }
    }
    names
}

/// Монтирует ISO, читает список редакций и отмонтирует обратно.
pub fn iso_editions(iso: &str) -> Result<Vec<String>, String> {
    let quoted = format!("'{}'", iso.replace('\'', "''"));
    let out = os::run(
        "powershell",
        &format!(
            "$ErrorActionPreference = 'Stop'; $i = Get-DiskImage -ImagePath {quoted}; $m = 0; \
             if (-not $i.Attached) {{ $i = Mount-DiskImage -ImagePath {quoted} -PassThru; $m = 1 }}; \
             \"$(($i | Get-Volume).DriveLetter)|$m\""
        ),
    )?;
    let line = out.lines().map(str::trim).find(|l| l.contains('|')).unwrap_or("");
    let (letter, mounted_by_us) = line.split_once('|').ok_or("не вдалося змонтувати ISO")?;
    let letter = letter.trim();
    let result = if letter.len() == 1 {
        let wim = std::path::PathBuf::from(format!(r"{letter}:\sources\install.wim"));
        let esd = std::path::PathBuf::from(format!(r"{letter}:\sources\install.esd"));
        let image = if wim.is_file() { wim } else { esd };
        if image.is_file() {
            read_wim_names(&image)
        } else {
            Err("в ISO немає sources\\install.wim / install.esd".into())
        }
    } else {
        Err("ISO не має букви диска".into())
    };
    if mounted_by_us.trim() == "1" {
        let _ = os::run("powershell", &format!("Dismount-DiskImage -ImagePath {quoted} | Out-Null"));
    }
    result
}

// ---------------------------------------------------------------------------
// Сеть
// ---------------------------------------------------------------------------

/// Допустимое имя хоста или IP (без пробелов и символов оболочки).
pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

/// Один пинг: время ответа в мс или None (нет ответа).
#[cfg(windows)]
pub fn ping_once(host: &str) -> Option<u32> {
    use std::os::windows::process::CommandExt;
    if !valid_host(host) {
        return None;
    }
    let out = std::process::Command::new("ping")
        .args(["-n", "1", "-w", "1500", host])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    parse_ping(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(not(windows))]
pub fn ping_once(_host: &str) -> Option<u32> {
    None
}

/// Время из вывода ping на любом языке: строка с «TTL=», токен перед ним
/// («time=12ms», «час=12мс», «time<1ms») — берём число после '=' или '<'.
fn parse_ping(output: &str) -> Option<u32> {
    for line in output.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some(ttl) = tokens.iter().position(|t| t.to_uppercase().starts_with("TTL=")) else {
            continue;
        };
        for token in tokens[..ttl].iter().rev() {
            if let Some(pos) = token.rfind(['=', '<']) {
                let digits: String = token[pos + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(ms) = digits.parse() {
                    return Some(ms);
                }
            }
        }
    }
    None
}

/// Текущие DNS-серверы (IPv4) активных физических адаптеров.
pub fn current_dns() -> Vec<String> {
    let out = os::run(
        "powershell",
        "Get-NetAdapter -Physical | Where-Object Status -eq 'Up' | \
         Get-DnsClientServerAddress -AddressFamily IPv4 | Select-Object -ExpandProperty ServerAddresses",
    )
    .unwrap_or_default();
    let mut list: Vec<String> = Vec::new();
    for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if !list.iter().any(|x| x == line) {
            list.push(line.to_string());
        }
    }
    list
}

/// Ставит DNS на активные физические адаптеры. Пустой список — вернуть автоматические (DHCP).
/// Нужны права администратора.
pub fn set_dns(servers: &[String], doh_template: Option<&str>) -> Result<(), String> {
    os::run("powershell", &set_dns_script(servers, doh_template)?).map(|_| ())
}

/// Скрипт смены DNS. С шаблоном DoH адреса регистрируются как серверы
/// DNS-over-HTTPS с автоповышением: Windows 11 шифрует запросы к ним, а если
/// HTTPS недоступен — откатывается на обычный DNS, и интернет не пропадает.
pub(crate) fn set_dns_script(servers: &[String], doh_template: Option<&str>) -> Result<String, String> {
    for s in servers {
        s.parse::<std::net::IpAddr>()
            .map_err(|_| format!("не IP-адреса: {s}"))?;
    }
    let list = servers.iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join(",");
    let target = if servers.is_empty() {
        "-ResetServerAddresses".to_string()
    } else {
        format!("-ServerAddresses @({list})")
    };
    let doh = match doh_template {
        Some(t) if !servers.is_empty() && t.starts_with("https://") && !t.contains('\'') => format!(
            "if (Get-Command Add-DnsClientDohServerAddress -ErrorAction SilentlyContinue) {{              foreach ($ip in @({list})) {{              $p = @{{ ServerAddress = $ip; DohTemplate = '{t}'; AllowFallbackToUdp = $true; AutoUpgrade = $true }};              if (Get-DnsClientDohServerAddress -ServerAddress $ip -ErrorAction SilentlyContinue) {{ Set-DnsClientDohServerAddress @p | Out-Null }}              else {{ Add-DnsClientDohServerAddress @p | Out-Null }} }} }}; "
        ),
        _ => String::new(),
    };
    Ok(format!(
        "$ErrorActionPreference = 'Stop'; {doh}         Get-NetAdapter -Physical | Where-Object Status -eq 'Up' |          ForEach-Object {{ Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex {target} }};          Clear-DnsClientCache"
    ))
}

/// Очищает кэш DNS (права администратора не нужны).
pub fn flush_dns() -> Result<(), String> {
    os::run("cmd", "ipconfig /flushdns").map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_script_registers_doh_only_when_asked() {
        let ips = vec!["1.1.1.1".to_string(), "1.0.0.1".to_string()];
        let s = set_dns_script(&ips, Some("https://cloudflare-dns.com/dns-query")).unwrap();
        assert!(s.contains("DohTemplate = 'https://cloudflare-dns.com/dns-query'"));
        assert!(s.contains("AllowFallbackToUdp = $true"), "без HTTPS интернет не должен пропадать");
        assert!(s.find("Add-DnsClientDohServerAddress").unwrap() < s.find("Set-DnsClientServerAddress").unwrap());
        let plain = set_dns_script(&ips, None).unwrap();
        assert!(!plain.contains("Doh"));
        // Автоматический DNS — без DoH, даже если шаблон передан.
        assert!(!set_dns_script(&[], Some("https://x/dns-query")).unwrap().contains("Doh"));
        assert!(set_dns_script(&["1.2.3.4; calc".into()], None).is_err());
    }

    #[test]
    fn ping_parsing_any_language() {
        assert_eq!(parse_ping("Reply from 1.1.1.1: bytes=32 time=12ms TTL=57"), Some(12));
        assert_eq!(parse_ping("Reply from 10.0.0.1: bytes=32 time<1ms TTL=64"), Some(1));
        assert_eq!(parse_ping("Відповідь від 1.1.1.1: число байт=32 час=9мс TTL=57"), Some(9));
        // Кириллица из кодовой страницы OEM приходит испорченной — число всё равно находится.
        assert_eq!(parse_ping("?????? ?? 8.8.8.8: ????? ????=32 ?????=23?? TTL=117"), Some(23));
        assert_eq!(parse_ping("Request timed out."), None);
    }

    #[test]
    fn host_validation() {
        assert!(valid_host("google.com"));
        assert!(valid_host("1.1.1.1"));
        assert!(valid_host("fe80::1"));
        assert!(!valid_host("a.com & calc"));
        assert!(!valid_host(""));
    }

    #[test]
    fn wim_header_and_xml() {
        // Синтетический WIM: заголовок 208 байт + XML в UTF-16LE с BOM.
        let xml = "<WIM><IMAGE INDEX=\"1\"><NAME>Windows 11 Home</NAME></IMAGE>\
                   <IMAGE INDEX=\"2\"><DISPLAYNAME>x</DISPLAYNAME><NAME>Windows 11 Pro</NAME></IMAGE></WIM>";
        let mut xml_bytes = vec![0xFF, 0xFE];
        xml_bytes.extend(xml.encode_utf16().flat_map(|u| u.to_le_bytes()));
        let mut data = vec![0u8; 208];
        data[0..8].copy_from_slice(b"MSWIM\0\0\0");
        data[72..80].copy_from_slice(&(xml_bytes.len() as u64).to_le_bytes());
        data[80..88].copy_from_slice(&208u64.to_le_bytes());
        data.extend(&xml_bytes);

        let dir = std::env::temp_dir().join(format!("wf-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("install.wim");
        std::fs::write(&path, &data).unwrap();
        let names = read_wim_names(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(names, vec!["Windows 11 Home", "Windows 11 Pro"]);
    }

    #[test]
    fn not_a_wim() {
        let dir = std::env::temp_dir().join(format!("wf-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.wim");
        std::fs::write(&path, vec![0u8; 300]).unwrap();
        assert!(read_wim_names(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn usb_list_parsing() {
        let out = "USB|3|31914983424|AA12BB|E:|SanDisk Ultra\r\nnoise\r\nUSB|4|8004304896||F: G:|Kingston | DT\r\n";
        let list = parse_usb_disks(out);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].number, 3);
        assert_eq!(list[0].serial, "AA12BB");
        assert_eq!(list[0].letters, "E:");
        assert_eq!(list[1].name, "Kingston | DT", "имя может содержать '|'");
        assert!(list[1].serial.is_empty());
    }

    #[test]
    fn expand_keeps_unknown_vars() {
        assert_eq!(expand("%WF_SURELY_MISSING%\\x"), "%WF_SURELY_MISSING%\\x");
    }
}

// ---------------------------------------------------------------------------
// Автозапуск
// ---------------------------------------------------------------------------

/// Текст на двух языках (украинский, английский).
pub type Txt = (&'static str, &'static str);

const APPROVED_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const APPROVED_RUN32: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";
const APPROVED_FOLDER: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StartupLoc {
    UserRun,
    MachineRun,
    MachineRun32,
    UserFolder,
    CommonFolder,
}

impl StartupLoc {
    pub const ALL: [StartupLoc; 5] = [
        StartupLoc::UserRun,
        StartupLoc::MachineRun,
        StartupLoc::MachineRun32,
        StartupLoc::UserFolder,
        StartupLoc::CommonFolder,
    ];

    pub fn title(self) -> Txt {
        match self {
            StartupLoc::UserRun => ("Реєстр — поточний користувач", "Registry — current user"),
            StartupLoc::MachineRun => ("Реєстр — усі користувачі", "Registry — all users"),
            StartupLoc::MachineRun32 => ("Реєстр — усі користувачі (32-біт)", "Registry — all users (32-bit)"),
            StartupLoc::UserFolder => ("Папка автозавантаження", "Startup folder"),
            StartupLoc::CommonFolder => ("Спільна папка автозавантаження", "Common startup folder"),
        }
    }

    /// Откуда читается список (для показа пользователю).
    pub fn source(self) -> &'static str {
        match self {
            StartupLoc::UserRun => r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            StartupLoc::MachineRun => r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run",
            StartupLoc::MachineRun32 => r"HKLM\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
            StartupLoc::UserFolder => r"%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup",
            StartupLoc::CommonFolder => r"%ProgramData%\Microsoft\Windows\Start Menu\Programs\StartUp",
        }
    }

    /// Нужны ли права администратора, чтобы включить/выключить запись.
    pub fn needs_admin(self) -> bool {
        !matches!(self, StartupLoc::UserRun | StartupLoc::UserFolder)
    }

    /// Где Windows хранит отметку «включено/выключено» (как Диспетчер задач).
    fn approved(self) -> (Root, &'static str) {
        match self {
            StartupLoc::UserRun => (Root::Hkcu, APPROVED_RUN),
            StartupLoc::MachineRun => (Root::Hklm, APPROVED_RUN),
            StartupLoc::MachineRun32 => (Root::Hklm, APPROVED_RUN32),
            StartupLoc::UserFolder => (Root::Hkcu, APPROVED_FOLDER),
            StartupLoc::CommonFolder => (Root::Hklm, APPROVED_FOLDER),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StartupItem {
    /// Имя для показа.
    pub name: String,
    /// Имя значения в реестре (для папки — имя файла с расширением).
    pub value_name: String,
    pub command: String,
    pub loc: StartupLoc,
    pub enabled: bool,
}

/// Отметка в StartupApproved: нечётный первый байт — выключено.
fn approved_enabled(loc: StartupLoc, value_name: &str) -> bool {
    let (root, sub) = loc.approved();
    match reg::read_raw(root, sub, value_name) {
        Some(raw) => raw.bytes.first().map(|b| b % 2 == 0).unwrap_or(true),
        None => true,
    }
}

#[cfg(windows)]
fn read_run_values(loc: StartupLoc) -> Vec<StartupItem> {
    use winreg::enums::*;
    use winreg::RegKey;

    let (hive, sub) = match loc {
        StartupLoc::UserRun => (HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Run"),
        StartupLoc::MachineRun => (HKEY_LOCAL_MACHINE, r"Software\Microsoft\Windows\CurrentVersion\Run"),
        StartupLoc::MachineRun32 => (
            HKEY_LOCAL_MACHINE,
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
        ),
        _ => return Vec::new(),
    };
    let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(sub, KEY_READ) else {
        return Vec::new();
    };
    key.enum_values()
        .filter_map(Result::ok)
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, val)| StartupItem {
            enabled: approved_enabled(loc, &name),
            command: decode_utf16(&val.bytes),
            value_name: name.clone(),
            name,
            loc,
        })
        .collect()
}

#[cfg(not(windows))]
fn read_run_values(_loc: StartupLoc) -> Vec<StartupItem> {
    Vec::new()
}

fn read_folder(loc: StartupLoc) -> Vec<StartupItem> {
    let folder = expand(loc.source());
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            if file.eq_ignore_ascii_case("desktop.ini") {
                return None;
            }
            let path = e.path();
            Some(StartupItem {
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| file.clone()),
                enabled: approved_enabled(loc, &file),
                value_name: file,
                command: path.to_string_lossy().into_owned(),
                loc,
            })
        })
        .collect()
}

/// Все записи автозапуска из реестра и папок.
pub fn read_startup() -> Vec<StartupItem> {
    let mut out = Vec::new();
    for loc in StartupLoc::ALL {
        match loc {
            StartupLoc::UserFolder | StartupLoc::CommonFolder => out.extend(read_folder(loc)),
            _ => out.extend(read_run_values(loc)),
        }
    }
    out
}

/// Включает/выключает запись через StartupApproved — так же, как Диспетчер задач.
/// Сама команда автозапуска не удаляется, поэтому действие полностью обратимо.
pub fn set_startup_enabled(item: &StartupItem, enabled: bool) -> Result<(), String> {
    let (root, sub) = item.loc.approved();
    let mut bytes = vec![0u8; 12];
    bytes[0] = if enabled { 0x02 } else { 0x03 };
    if !enabled {
        // Время отключения (FILETIME) — Диспетчер задач показывает его.
        bytes[4..12].copy_from_slice(&filetime_now().to_le_bytes());
    }
    reg::write(root, sub, &item.value_name, &ValueData::Binary(bytes)).map_err(|e| e.to_string())
}

/// Текущее время в формате FILETIME (100-нс интервалы с 1601 года).
fn filetime_now() -> u64 {
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (now.as_secs() + EPOCH_DIFF_SECS) * 10_000_000 + u64::from(now.subsec_nanos() / 100)
}

// ---------------------------------------------------------------------------
// Оборудование
// ---------------------------------------------------------------------------

pub struct HwSectionData {
    pub icon: &'static str,
    pub title: Txt,
    /// (подпись, значение на uk, значение на en)
    pub rows: Vec<(Txt, String, String)>,
}

fn row_same(label: Txt, value: String) -> (Txt, String, String) {
    (label, value.clone(), value)
}

fn row_gb(label: Txt, bytes: u64) -> (Txt, String, String) {
    let gb = bytes as f64 / 1_073_741_824.0;
    let n = if gb >= 10.0 { format!("{gb:.0}") } else { format!("{gb:.1}") };
    (label, format!("{n} ГБ"), format!("{n} GB"))
}

/// Скрипт CIM для сведений об оборудовании: строки вида КЛЮЧ=значение.
const HW_SCRIPT: &str = r#"[Console]::OutputEncoding = [Text.Encoding]::UTF8
Get-CimInstance Win32_VideoController | ForEach-Object { "GPU=$($_.Name)|$($_.DriverVersion)" }
$b = Get-CimInstance Win32_BaseBoard; "BOARD=$($b.Manufacturer) $($b.Product)"
$bios = Get-CimInstance Win32_BIOS; "BIOS=$($bios.SMBIOSBIOSVersion)"
$os = Get-CimInstance Win32_OperatingSystem; "OS=$($os.Caption)|$($os.BuildNumber)"
Get-CimInstance Win32_NetworkAdapter -Filter 'PhysicalAdapter=True' | ForEach-Object { "NET=$($_.Name)" }
Get-CimInstance MSFT_PhysicalDisk -Namespace root\Microsoft\Windows\Storage -ErrorAction SilentlyContinue |
  ForEach-Object { "DISK=$($_.FriendlyName)|$($_.MediaType)|$($_.BusType)|$($_.Size)" }"#;

/// Собирает сведения об оборудовании (sysinfo + CIM через PowerShell). Медленно: ~1–3 с.
pub fn hardware() -> Vec<HwSectionData> {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let mut sections = Vec::new();

    // Процессор
    let brand = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
    let logical = sys.cpus().len();
    let mut cpu_rows = vec![row_same(("Модель", "Model"), brand)];
    match sys.physical_core_count() {
        Some(physical) if physical > 0 => {
            cpu_rows.push(row_same(("Ядра / потоки", "Cores / threads"), format!("{physical} / {logical}")))
        }
        _ => cpu_rows.push(row_same(("Потоки", "Threads"), logical.to_string())),
    }
    sections.push(HwSectionData {
        icon: "⚙",
        title: ("Процесор", "Processor"),
        rows: cpu_rows,
    });

    // Память
    sections.push(HwSectionData {
        icon: "📊",
        title: ("Оперативна пам'ять", "Memory (RAM)"),
        rows: vec![
            row_gb(("Обсяг", "Size"), sys.total_memory()),
            row_gb(("Вільно зараз", "Free now"), sys.available_memory()),
        ],
    });

    let cim = os::run("powershell", HW_SCRIPT).unwrap_or_default();
    let values = |key: &str| -> Vec<String> {
        cim.lines()
            .filter_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()))
            .filter(|v| !v.is_empty() && v != "|")
            .collect()
    };

    // Видеокарты
    let mut gpu_rows = Vec::new();
    for gpu in values("GPU=") {
        let (name, driver) = gpu.split_once('|').unwrap_or((gpu.as_str(), ""));
        gpu_rows.push(row_same(("Модель", "Model"), name.to_string()));
        if !driver.is_empty() {
            gpu_rows.push(row_same(("Драйвер", "Driver"), driver.to_string()));
        }
    }
    if !gpu_rows.is_empty() {
        sections.push(HwSectionData {
            icon: "🎮",
            title: ("Відеокарта", "Graphics card"),
            rows: gpu_rows,
        });
    }

    // Материнская плата
    if let Some(board) = values("BOARD=").into_iter().next() {
        let mut rows = vec![row_same(("Модель", "Model"), board)];
        if let Some(bios) = values("BIOS=").into_iter().next() {
            rows.push(row_same(("BIOS", "BIOS"), bios));
        }
        sections.push(HwSectionData {
            icon: "🔌",
            title: ("Материнська плата", "Motherboard"),
            rows,
        });
    }

    // Физические диски: модель, объём, шина и тип
    let disk_rows: Vec<_> = values("DISK=").iter().map(|d| disk_row(d)).collect();
    if !disk_rows.is_empty() {
        sections.push(HwSectionData {
            icon: "💾",
            title: ("Диски", "Drives"),
            rows: disk_rows,
        });
    }

    // Сеть
    let nets = values("NET=");
    if !nets.is_empty() {
        sections.push(HwSectionData {
            icon: "📶",
            title: ("Мережа", "Network"),
            rows: nets.into_iter().map(|n| row_same(("Адаптер", "Adapter"), n)).collect(),
        });
    }

    // Система
    if let Some(os_line) = values("OS=").into_iter().next() {
        let (caption, build) = os_line.split_once('|').unwrap_or((os_line.as_str(), ""));
        let mut rows = vec![row_same(("Версія", "Version"), caption.to_string())];
        if !build.is_empty() {
            rows.push(row_same(("Збірка", "Build"), build.to_string()));
        }
        sections.push(HwSectionData {
            icon: "🖥",
            title: ("Система", "System"),
            rows,
        });
    }

    sections
}

/// "Имя|MediaType|BusType|Size" → строка диска на двух языках.
fn disk_row(raw: &str) -> (Txt, String, String) {
    let parts: Vec<&str> = raw.split('|').collect();
    let name = parts.first().copied().unwrap_or("");
    let media = match parts.get(1).copied().unwrap_or("") {
        "3" => "HDD",
        "4" => "SSD",
        _ => "",
    };
    let bus = match parts.get(2).copied().unwrap_or("") {
        "17" => "NVMe",
        "11" => "SATA",
        "7" => "USB",
        _ => "",
    };
    let size = parts
        .get(3)
        .and_then(|s| s.parse::<u64>().ok())
        .map(|b| b as f64 / 1_000_000_000.0)
        .unwrap_or(0.0);
    let kind: Vec<&str> = [bus, media].into_iter().filter(|s| !s.is_empty()).collect();
    let suffix = if kind.is_empty() { String::new() } else { format!(", {}", kind.join(" ")) };
    let (uk, en) = if size >= 1000.0 {
        (format!("{:.1} ТБ", size / 1000.0), format!("{:.1} TB", size / 1000.0))
    } else {
        (format!("{size:.0} ГБ"), format!("{size:.0} GB"))
    };
    (("Диск", "Drive"), format!("{name} — {uk}{suffix}"), format!("{name} — {en}{suffix}"))
}

fn is_hdd(d: &sysinfo::Disk) -> bool {
    matches!(d.kind(), sysinfo::DiskKind::HDD)
}

// ---------------------------------------------------------------------------
// Живой монитор дисков
// ---------------------------------------------------------------------------

pub struct DriveLive {
    pub letter: String,
    pub total_gb: u32,
    pub free_gb: u32,
    pub is_hdd: bool,
}

pub struct ProcLive {
    pub name: String,
    pub pid: u32,
    pub read_mb: f32,
    pub write_mb: f32,
}

pub struct DiskSampler {
    sys: sysinfo::System,
    disks: sysinfo::Disks,
    last: std::time::Instant,
    pub drives: Vec<DriveLive>,
    pub procs: Vec<ProcLive>,
    pub total_read: f32,
    pub total_write: f32,
}

impl DiskSampler {
    pub fn new() -> Self {
        let mut s = Self {
            sys: sysinfo::System::new(),
            disks: sysinfo::Disks::new_with_refreshed_list(),
            last: std::time::Instant::now(),
            drives: Vec::new(),
            procs: Vec::new(),
            total_read: 0.0,
            total_write: 0.0,
        };
        s.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        s.refresh_drives();
        s
    }

    fn refresh_drives(&mut self) {
        self.disks.refresh(true);
        self.drives = self
            .disks
            .list()
            .iter()
            .map(|d| DriveLive {
                letter: d.mount_point().to_string_lossy().trim_end_matches('\\').to_string(),
                total_gb: (d.total_space() / 1_073_741_824) as u32,
                free_gb: (d.available_space() / 1_073_741_824) as u32,
                is_hdd: is_hdd(d),
            })
            .collect();
        self.drives.sort_by(|a, b| a.letter.cmp(&b.letter));
    }

    /// Пересчитывает скорости по дельтам ввода-вывода процессов.
    pub fn sample(&mut self) {
        let elapsed = self.last.elapsed().as_secs_f32().max(0.05);
        self.last = std::time::Instant::now();
        self.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        let mb = 1_048_576.0_f32;
        let mut procs: Vec<ProcLive> = self
            .sys
            .processes()
            .iter()
            .map(|(pid, p)| {
                let du = p.disk_usage();
                ProcLive {
                    name: p.name().to_string_lossy().into_owned(),
                    pid: pid.as_u32(),
                    read_mb: du.read_bytes as f32 / mb / elapsed,
                    write_mb: du.written_bytes as f32 / mb / elapsed,
                }
            })
            .collect();

        self.total_read = procs.iter().map(|p| p.read_mb).sum();
        self.total_write = procs.iter().map(|p| p.write_mb).sum();

        procs.sort_by(|a, b| {
            (b.read_mb + b.write_mb)
                .partial_cmp(&(a.read_mb + a.write_mb))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        procs.retain(|p| p.read_mb + p.write_mb > 0.01);
        procs.truncate(12);
        self.procs = procs;
        self.refresh_drives();
    }
}

impl Default for DiskSampler {
    fn default() -> Self {
        Self::new()
    }
}
