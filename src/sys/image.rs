//! Сборка своего образа Windows: генерация `autounattend.xml`, скриптов
//! установки и первого входа, скрипта сборки ISO или флешки.
//!
//! Образ не правится офлайн. Твики, удаление приложений и скрипты первого входа
//! вшиты в `autounattend.xml`: установщик Windows сам применяет их в
//! устанавливаемой системе (проход specialize, от имени SYSTEM). Так сборка не
//! зависит от того, умеет ли ПК, на котором её делают, грузить кусты реестра
//! смонтированного образа (на Windows 26H2 это ломалось с 0x80070057).
//!
//! Файлы генерируются как текст (это проверяется тестами), а тяжёлую работу
//! (экспорт wim, запись ISO или флешки) выполняет сгенерированный
//! PowerShell-скрипт с правами администратора.

use crate::catalog::{Action, ActionKind};
use crate::sys::reg::{self, Root};

/// Папка WinForge в устанавливаемой системе.
pub const SETUP_DIR: &str = r"C:\WinForge";

/// Копия файла ответов, которую установщик кладёт в систему перед specialize.
const PANTHER_UNATTEND: &str = r"C:\Windows\Panther\unattend.xml";

/// Пространство имён своего блока в autounattend.xml: установщик его пропускает.
const EXT_NS: &str = "urn:winforge:unattend";

/// Распаковывает вшитые файлы `<Extensions><File path="...">` на диск.
/// .ps1 — UTF-8 с BOM (иначе PowerShell 5 читает ANSI), .cmd — в OEM-кодировке.
/// XML превращает переводы строк в LF — возвращаем CRLF.
const EXTRACT_SCRIPT: &str = "param([xml]$Document)\n\
foreach ($file in $Document.unattend.Extensions.File) {\n\
\x20   $path = [Environment]::ExpandEnvironmentVariables($file.GetAttribute('path'))\n\
\x20   [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path))\n\
\x20   $text = ($file.InnerText.Trim() -replace \"`r?`n\", \"`r`n\") + \"`r`n\"\n\
\x20   $enc = if ($path -like '*.cmd') { [Text.Encoding]::GetEncoding([Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage) } else { New-Object Text.UTF8Encoding ($path -like '*.ps1') }\n\
\x20   [IO.File]::WriteAllText($path, $text, $enc)\n\
}";

/// Команда, которая читает файл ответов и запускает из него EXTRACT_SCRIPT.
/// Поле Path у RunSynchronousCommand — не длиннее 259 символов.
fn extract_command(answer_file: &str) -> String {
    format!(
        "powershell.exe -NoProfile -Command \"$x = [xml]::new(); $x.Load('{answer_file}'); \
         Invoke-Command -ScriptBlock ([scriptblock]::Create($x.unattend.Extensions.ExtractScript)) -ArgumentList $x\""
    )
}

/// Параметры автоответа установки.
#[derive(Debug, Clone, Default)]
pub struct Unattend {
    pub ui_language: String,   // en-US | uk-UA
    pub input_locales: String, // "0422:00000422;0419:00000419;0409:00000409"
    pub timezone: String,      // "FLE Standard Time"
    pub username: String,
    pub password: String,
    pub computer_name: String,
    pub local_account: bool,
    pub auto_login: bool,
    pub bypass_tpm: bool,
    pub no_network: bool,
    /// Файлы, вшитые в autounattend.xml: (путь в системе, содержимое).
    /// Если среди них есть `specialize.ps1`, установщик запускает его от SYSTEM.
    pub files: Vec<(String, String)>,
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl Unattend {
    /// Полный autounattend.xml.
    pub fn xml(&self) -> String {
        let ui = xml_escape(&self.ui_language);
        let locales = xml_escape(&self.input_locales);
        let tz = xml_escape(&self.timezone);
        let user = xml_escape(&self.username);
        let pass = xml_escape(&self.password);
        let pc = xml_escape(&self.computer_name);

        // windowsPE: (по желанию) обход проверок совместимости Windows 11.
        let mut pe_runsync = String::new();
        if self.bypass_tpm {
            let keys = [
                "BypassTPMCheck",
                "BypassSecureBootCheck",
                "BypassRAMCheck",
                "BypassStorageCheck",
                "BypassCPUCheck",
            ];
            for (i, k) in keys.iter().enumerate() {
                pe_runsync.push_str(&format!(
                    "                <RunSynchronousCommand wcm:action=\"add\">\n\
                     \x20                   <Order>{}</Order>\n\
                     \x20                   <Path>reg add HKLM\\SYSTEM\\Setup\\LabConfig /v {} /t REG_DWORD /d 1 /f</Path>\n\
                     \x20               </RunSynchronousCommand>\n",
                    i + 1,
                    k
                ));
            }
        }
        let setup_component = if pe_runsync.is_empty() {
            String::new()
        } else {
            format!(
                "        <component name=\"Microsoft-Windows-Setup\" processorArchitecture=\"amd64\" publicKeyToken=\"31bf3856ad364e35\" language=\"neutral\" versionScope=\"nonSxS\" xmlns:wcm=\"http://schemas.microsoft.com/WMIConfig/2002/State\">\n\
                 \x20           <RunSynchronous>\n{pe_runsync}            </RunSynchronous>\n\
                 \x20       </component>\n"
            )
        };

        // oobeSystem: учётная запись и первый вход.
        let account = if self.local_account {
            format!(
                "            <UserAccounts>\n\
                 \x20               <LocalAccounts>\n\
                 \x20                   <LocalAccount wcm:action=\"add\">\n\
                 \x20                       <Name>{user}</Name>\n\
                 \x20                       <Group>Administrators</Group>\n\
                 \x20                       <Password><Value>{pass}</Value><PlainText>true</PlainText></Password>\n\
                 \x20                   </LocalAccount>\n\
                 \x20               </LocalAccounts>\n\
                 \x20           </UserAccounts>\n"
            )
        } else {
            String::new()
        };

        let autologon = if self.auto_login && self.local_account {
            format!(
                "            <AutoLogon>\n\
                 \x20               <Username>{user}</Username>\n\
                 \x20               <Enabled>true</Enabled>\n\
                 \x20               <LogonCount>1</LogonCount>\n\
                 \x20               <Password><Value>{pass}</Value><PlainText>true</PlainText></Password>\n\
                 \x20           </AutoLogon>\n"
            )
        } else {
            String::new()
        };

        let hide_online = if self.no_network || self.local_account {
            "                <HideOnlineAccountScreens>true</HideOnlineAccountScreens>\n"
        } else {
            ""
        };

        let first_logon = "            <FirstLogonCommands>\n\
             \x20               <SynchronousCommand wcm:action=\"add\">\n\
             \x20                   <Order>1</Order>\n\
             \x20                   <CommandLine>cmd /c if exist %SystemDrive%\\WinForge\\first-logon.cmd call %SystemDrive%\\WinForge\\first-logon.cmd</CommandLine>\n\
             \x20                   <Description>WinForge tweaks</Description>\n\
             \x20               </SynchronousCommand>\n\
             \x20               <SynchronousCommand wcm:action=\"add\">\n\
             \x20                   <Order>2</Order>\n\
             \x20                   <CommandLine>cmd /c if exist %SystemDrive%\\WinForge\\setup-apps.ps1 powershell -NoProfile -ExecutionPolicy Bypass -File %SystemDrive%\\WinForge\\setup-apps.ps1</CommandLine>\n\
             \x20                   <Description>WinForge apps</Description>\n\
             \x20               </SynchronousCommand>\n\
             \x20           </FirstLogonCommands>\n";

        // specialize: распаковать вшитые файлы и применить твики от имени SYSTEM.
        let (deployment, extensions) = if self.files.is_empty() {
            (String::new(), String::new())
        } else {
            let mut cmds = vec![extract_command(PANTHER_UNATTEND)];
            if let Some((path, _)) = self.files.iter().find(|(p, _)| p.ends_with("specialize.ps1")) {
                cmds.push(format!("powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{path}\""));
            }
            let runs: String = cmds
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    format!(
                        "                <RunSynchronousCommand wcm:action=\"add\">\n\
                         \x20                   <Order>{}</Order>\n\
                         \x20                   <Path>{}</Path>\n\
                         \x20               </RunSynchronousCommand>\n",
                        i + 1,
                        xml_escape(c)
                    )
                })
                .collect();
            let deployment = format!(
                "        <component name=\"Microsoft-Windows-Deployment\" processorArchitecture=\"amd64\" publicKeyToken=\"31bf3856ad364e35\" language=\"neutral\" versionScope=\"nonSxS\" xmlns:wcm=\"http://schemas.microsoft.com/WMIConfig/2002/State\">\n\
                 \x20           <RunSynchronous>\n{runs}            </RunSynchronous>\n\
                 \x20       </component>\n"
            );
            let files: String = self
                .files
                .iter()
                .map(|(path, body)| format!("        <File path=\"{}\">{}</File>\n", xml_escape(path), xml_escape(body)))
                .collect();
            let extensions = format!(
                "    <Extensions xmlns=\"{EXT_NS}\">\n\
                 \x20       <ExtractScript>{}</ExtractScript>\n\
                 {files}    </Extensions>\n",
                xml_escape(EXTRACT_SCRIPT)
            );
            (deployment, extensions)
        };

        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<!-- Created by WinForge -->
<unattend xmlns="urn:schemas-microsoft-com:unattend">
    <settings pass="windowsPE">
        <component name="Microsoft-Windows-International-Core-WinPE" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State">
            <SetupUILanguage><UILanguage>{ui}</UILanguage></SetupUILanguage>
            <InputLocale>{locales}</InputLocale>
            <SystemLocale>{ui}</SystemLocale>
            <UILanguage>{ui}</UILanguage>
            <UserLocale>{ui}</UserLocale>
        </component>
{setup_component}    </settings>
    <settings pass="specialize">
        <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State">
            <ComputerName>{pc}</ComputerName>
            <TimeZone>{tz}</TimeZone>
        </component>
{deployment}    </settings>
    <settings pass="oobeSystem">
        <component name="Microsoft-Windows-International-Core" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State">
            <InputLocale>{locales}</InputLocale>
            <SystemLocale>{ui}</SystemLocale>
            <UILanguage>{ui}</UILanguage>
            <UserLocale>{ui}</UserLocale>
        </component>
        <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State">
            <OOBE>
                <HideEULAPage>true</HideEULAPage>
                <HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE>
{hide_online}                <ProtectYourPC>3</ProtectYourPC>
            </OOBE>
{account}{autologon}{first_logon}        </component>
    </settings>
{extensions}</unattend>
"#
        )
    }
}

// ---------------------------------------------------------------------------
// Строки reg.exe
// ---------------------------------------------------------------------------

/// Строка в одинарных кавычках PowerShell (буквальная, без подстановок).
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Строка в двойных кавычках для .cmd: `%` удваивается, `"` экранируется для reg.exe.
fn bat_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('%', "%%").replace('"', "\\\""))
}

/// (тип REG_*, данные) для reg.exe.
fn reg_type_and_data(a: &Action) -> Option<(&'static str, String)> {
    if let Some(v) = a.dword {
        Some(("REG_DWORD", v.to_string()))
    } else if let Some(v) = a.qword {
        Some(("REG_QWORD", v.to_string()))
    } else if let Some(s) = &a.sz {
        Some(("REG_SZ", s.clone()))
    } else if let Some(s) = &a.expand {
        Some(("REG_EXPAND_SZ", s.clone()))
    } else if let Some(list) = &a.multi {
        Some(("REG_MULTI_SZ", list.join("\\0")))
    } else {
        a.binary.as_ref().map(|h| {
            let hex: String = reg::parse_hex(h).iter().map(|b| format!("{b:02x}")).collect();
            ("REG_BINARY", hex)
        })
    }
}

/// Команда reg.exe для ключа `key`; `quote` задаёт экранирование оболочки.
fn reg_cmd(key: &str, a: &Action, quote: fn(&str) -> String) -> Option<String> {
    if a.delete {
        return Some(match a.name.as_deref() {
            Some("") => format!("reg delete {} /ve /f", quote(key)),
            Some(n) => format!("reg delete {} /v {} /f", quote(key), quote(n)),
            None => format!("reg delete {} /f", quote(key)),
        });
    }
    let (vtype, data) = reg_type_and_data(a)?;
    let name_flag = match a.name.as_deref() {
        Some("") | None => "/ve".to_string(),
        Some(n) => format!("/v {}", quote(n)),
    };
    // Пустые данные: без /d (PowerShell 5 теряет пустые аргументы).
    let data_part = if data.is_empty() {
        String::new()
    } else {
        format!(" /d {}", quote(&data))
    };
    Some(format!("reg add {} {name_flag} /t {vtype}{data_part} /f", quote(key)))
}

/// Ключи классов пользователя живут в UsrClass.dat, а не в NTUSER.DAT,
/// поэтому в образе их пишем при первом входе.
fn is_user_classes(root: Root, sub: &str) -> bool {
    root == Root::Hkcu && sub.to_lowercase().starts_with("software\\classes")
}

/// Куда в specialize загружается профиль по умолчанию (C:\Users\Default\NTUSER.DAT):
/// из него создаются профили всех новых пользователей.
const DEFAULT_USER_HIVE: &str = r"HKU\WF_DEFAULT";

/// Отображает путь реестра на систему, которая устанавливается (проход specialize).
/// HKLM пишется как есть, HKCU — в профиль по умолчанию.
fn map_setup(path: &str) -> Option<String> {
    let (root, sub) = reg::split(path)?;
    if is_user_classes(root, &sub) {
        return None;
    }
    Some(match root {
        Root::Hklm => format!("HKLM\\{sub}"),
        Root::Hkcu => format!("{DEFAULT_USER_HIVE}\\{sub}"),
        Root::Hkcr => format!("HKLM\\SOFTWARE\\Classes\\{sub}"),
        Root::Hku => return None,
    })
}

/// Строка specialize.ps1 для реестра или службы (None, если действие не для него).
pub fn setup_reg_line(a: &Action) -> Option<String> {
    match a.kind() {
        ActionKind::Reg => {
            let key = map_setup(a.reg.as_deref()?)?;
            reg_cmd(&key, a, ps_quote)
        }
        ActionKind::Service => {
            let svc = a.service.as_deref()?;
            let start = match a.start.as_deref().unwrap_or("disabled") {
                "auto" | "automatic" | "delayed" | "delayed-auto" => 2,
                "manual" | "demand" => 3,
                "boot" => 0,
                "system" => 1,
                _ => 4,
            };
            let key = format!("HKLM\\SYSTEM\\CurrentControlSet\\Services\\{svc}");
            // Только если служба есть в системе — иначе reg add создал бы мусорный ключ.
            Some(format!(
                "if (Test-Path {}) {{ reg add {} /v Start /t REG_DWORD /d {start} /f }}",
                ps_quote(&format!("Registry::HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Services\\{svc}")),
                ps_quote(&key)
            ))
        }
        _ => None,
    }
}

/// Строка first-logon.cmd (выполняется при первом входе пользователя).
pub fn first_logon_line(a: &Action) -> Option<String> {
    match a.kind() {
        ActionKind::Reg => {
            let path = a.reg.as_deref()?;
            let (root, sub) = reg::split(path)?;
            if is_user_classes(root, &sub) {
                reg_cmd(path, a, bat_quote)
            } else {
                None
            }
        }
        ActionKind::Task => {
            let task = a.task.as_deref()?;
            let flag = if a.enabled == Some(true) { "/Enable" } else { "/Disable" };
            Some(format!("schtasks /Change /TN {} {flag} >nul 2>&1", bat_quote(task)))
        }
        ActionKind::Run => {
            let cmd = a.run.as_deref()?;
            if a.shell.as_deref().map(|s| s.eq_ignore_ascii_case("powershell")).unwrap_or(false) {
                Some(format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}",
                    encode_ps(cmd)
                ))
            } else {
                Some(cmd.to_string())
            }
        }
        _ => None,
    }
}

/// Строка specialize.ps1 для компонента Windows.
pub fn setup_feature_line(a: &Action) -> Option<String> {
    if a.kind() == ActionKind::Capability {
        // Remove-Capability объявлена в specialize.ps1.
        return Some(format!("Remove-Capability {}", ps_quote(a.capability.as_deref()?)));
    }
    if a.kind() != ActionKind::Feature {
        return None;
    }
    let feature = a.feature.as_deref()?;
    // Включение: /All — вместе с зависимостями; $sxs (из specialize.ps1) — папка
    // sources\sxs носителя установки, чтобы .NET 3.5 ставился без интернета.
    let (verb, extra, done) = if a.enabled == Some(true) {
        ("/Enable-Feature", " /All @sxs", "enabled")
    } else {
        ("/Disable-Feature", "", "disabled")
    };
    // 3010 — «готово, нужна перезагрузка»: установщик и так перезагрузит ПК.
    Some(format!(
        "& dism.exe /English /Online {verb} {}{extra} /NoRestart /Quiet | Out-Null; if ($LASTEXITCODE -in 0, 3010) {{ Log {} }} elseif ($LASTEXITCODE -eq -2146498548) {{ Log '   feature {feature}: not present in this Windows' }} else {{ Log \"   feature {feature}: not changed (dism code $LASTEXITCODE)\" }}",
        ps_quote(&format!("/FeatureName:{feature}")),
        ps_quote(&format!("   feature {feature}: {done}"))
    ))
}

/// specialize.ps1: всё, что раньше правилось в смонтированном образе.
/// Запускается установщиком внутри новой системы от имени SYSTEM, до создания
/// пользователя, поэтому HKCU-твики идут в профиль по умолчанию.
#[cfg(test)]
pub fn specialize_script(
    appx: &[String],
    reg_lines: &[String],
    feature_lines: &[String],
    copy_installers: bool,
) -> String {
    specialize_script_with(
        appx,
        reg_lines,
        feature_lines,
        &SpecializeOptions {
            copy_installers,
            ..Default::default()
        },
    )
}

/// Дополнительные шаги specialize.ps1.
#[derive(Debug, Clone, Default)]
pub struct SpecializeOptions {
    /// Забрать свои установщики с носителя.
    pub copy_installers: bool,
    /// Добавить профиль Wi-Fi из вшитого файла WIFI_PROFILE (и сразу стереть его).
    pub wifi: bool,
    /// Сжать систему (CompactOS): Windows займёт на несколько ГБ меньше.
    pub compact_os: bool,
    /// Поставить гостевые дополнения VirtualBox / VMware, если их диск подключён.
    pub guest_tools: bool,
    /// Установить ключ продукта из вшитого файла PRODUCT_KEY_FILE (и сразу стереть его).
    pub product_key: bool,
    /// KMS-сервер организации: (адрес, порт). Адрес уже проверен.
    pub kms: Option<(String, Option<u16>)>,
    /// Поставить драйверы ПК, на котором собирали образ, из `WinForge\Drivers` носителя.
    pub this_pc_drivers: bool,
}

/// Где в новой системе лежит профиль Wi-Fi (удаляется сразу после добавления).
pub const WIFI_PROFILE: &str = r"C:\WinForge\wifi.xml";

/// Где в новой системе лежит ключ продукта (удаляется сразу после установки ключа).
pub const PRODUCT_KEY_FILE: &str = r"C:\WinForge\product-key.txt";

/// Убирает узлы с секретами (профиль Wi-Fi с паролем, ключ продукта) из копий
/// файла ответов в Panther. Установщик пишет их заново в конце установки,
/// поэтому это делается и в specialize, и при первом входе.
const SECRETS_SCRUB: &str = "foreach ($p in @('C:\\Windows\\Panther\\unattend.xml', 'C:\\Windows\\Panther\\unattend-original.xml')) { if (-not (Test-Path -LiteralPath $p)) { continue }; try { $x = [xml]::new(); $x.Load($p); $n = @($x.GetElementsByTagName('File') | Where-Object { $_.GetAttribute('path') -in @('C:\\WinForge\\wifi.xml', 'C:\\WinForge\\product-key.txt') }); foreach ($f in $n) { [void]$f.ParentNode.RemoveChild($f) }; if ($n.Count -gt 0) { $x.Save($p) } } catch { } }";

/// Строка first-logon.cmd, которая ещё раз стирает секреты из файлов ответов.
pub fn secrets_scrub_logon_line() -> String {
    format!("powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}", encode_ps(SECRETS_SCRUB))
}

/// Профиль Wi-Fi для `netsh wlan add profile` (WPA2-Personal или открытая сеть).
pub fn wifi_profile_xml(ssid: &str, password: &str) -> String {
    let name = xml_escape(ssid);
    let security = if password.is_empty() {
        "<authEncryption><authentication>open</authentication><encryption>none</encryption><useOneX>false</useOneX></authEncryption>".to_string()
    } else {
        format!(
            "<authEncryption><authentication>WPA2PSK</authentication><encryption>AES</encryption><useOneX>false</useOneX></authEncryption>\
             <sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>{}</keyMaterial></sharedKey>",
            xml_escape(password)
        )
    };
    format!(
        "<?xml version=\"1.0\"?>\r\n<WLANProfile xmlns=\"http://www.microsoft.com/networking/WLAN/profile/v1\">\
         <name>{name}</name><SSIDConfig><SSID><name>{name}</name></SSID><nonBroadcast>true</nonBroadcast></SSIDConfig>\
         <connectionType>ESS</connectionType><connectionMode>auto</connectionMode>\
         <MSM><security>{security}</security></MSM></WLANProfile>"
    )
}

pub fn specialize_script_with(
    appx: &[String],
    reg_lines: &[String],
    feature_lines: &[String],
    opts: &SpecializeOptions,
) -> String {
    let copy_installers = opts.copy_installers;
    let appx = appx.iter().map(|p| ps_quote(p)).collect::<Vec<_>>().join(", ");
    let mut s = String::new();
    s.push_str("# Created by WinForge: runs inside Windows Setup (specialize pass, as SYSTEM).\r\n");
    s.push_str("$ErrorActionPreference = 'Continue'\r\n");
    s.push_str(&format!("$log = {}\r\n", ps_quote(&format!("{SETUP_DIR}\\specialize.log"))));
    s.push_str(
        "function Log([string]$m) { Add-Content -Path $log -Value \"$(Get-Date -Format 'HH:mm:ss')  $m\" -Encoding UTF8 }\r\n\
         # The reg lines below go through this function: failures end up in the log.\r\n\
         # Deleting what is already absent is not a failure: the result is the same.\r\n\
         function reg {\r\n\
         \x20   $out = & reg.exe @args 2>&1\r\n\
         \x20   if ($LASTEXITCODE -eq 0) { return }\r\n\
         \x20   if ($args[0] -eq 'delete') {\r\n\
         \x20       $query = @('query') + @($args | Select-Object -Skip 1 | Where-Object { $_ -ne '/f' })\r\n\
         \x20       & reg.exe @query 2>&1 | Out-Null\r\n\
         \x20       if ($LASTEXITCODE -ne 0) { return }\r\n\
         \x20   }\r\n\
         \x20   Log \"   reg $($args -join ' '): $out\"\r\n\
         }\r\n\
         Log 'WinForge: specialize started'\r\n\
         \r\n",
    );
    if opts.this_pc_drivers {
        // Первым делом: Wi-Fi ниже нужен адаптер с драйвером. Читаем прямо с носителя,
        // без копирования гигабайтов на C:. Ненулевой код pnputil — не провал
        // (нужна перезагрузка, драйвер не подошёл ни к одному устройству).
        s.push_str(
            "Log 'Installing drivers of the PC the image was built on...'\r\n\
             $drv = Get-PSDrive -PSProvider FileSystem | Where-Object { $_.Root -ne \"$env:SystemDrive\\\" } | ForEach-Object { Join-Path $_.Root 'WinForge\\Drivers' } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1\r\n\
             if ($drv) {\r\n\
             \x20   $t = Get-Date\r\n\
             \x20   $out = pnputil.exe /add-driver (Join-Path $drv '*.inf') /subdirs /install 2>&1\r\n\
             \x20   Log \"   pnputil from ${drv}: code $LASTEXITCODE, $([int]((Get-Date) - $t).TotalSeconds) s; $((@($out) | Where-Object { \"$_\".Trim() } | Select-Object -Last 3) -join ' | ')\"\r\n\
             } else { Log '   WinForge\\Drivers not found on any drive: was the USB drive or disc removed?' }\r\n\
             \r\n",
        );
    }
    if copy_installers {
        // Носитель установки (флешка/диск) в specialize ещё подключён: забираем
        // свои установщики, пока он есть. Системный диск пропускаем.
        s.push_str(&format!(
            "Log 'Copying own installers from the installation media...'\r\n\
             $dst = {}\r\n",
            ps_quote(&format!("{SETUP_DIR}\\Installers"))
        ));
        s.push_str(
            "$src = Get-PSDrive -PSProvider FileSystem | Where-Object { $_.Root -ne \"$env:SystemDrive\\\" } | ForEach-Object { Join-Path $_.Root 'WinForge\\Installers' } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1\r\n\
             if ($src) {\r\n\
             \x20   New-Item -ItemType Directory -Force -Path $dst | Out-Null\r\n\
             \x20   Copy-Item -Path (Join-Path $src '*') -Destination $dst -Force\r\n\
             \x20   Log \"   from ${src}: $(@(Get-ChildItem -LiteralPath $dst).Count) files\"\r\n\
             } else { Log '   not found on any drive; setup-apps.ps1 will look again at first sign-in' }\r\n\
             \r\n",
        );
    }
    s.push_str("Log 'Removing built-in apps...'\r\n");
    s.push_str(&format!("$appx = @({appx})\r\n"));
    s.push_str(
        "if ($appx.Count -gt 0) {\r\n\
         \x20   $prov = @(Get-AppxProvisionedPackage -Online)\r\n\
         \x20   Log \"   provisioned packages: $($prov.Count)\"\r\n\
         \x20   foreach ($name in $appx) {\r\n\
         \x20       foreach ($p in @($prov | Where-Object { $_.DisplayName -like $name })) {\r\n\
         \x20           try { Remove-AppxProvisionedPackage -Online -AllUsers -PackageName $p.PackageName -ErrorAction Stop | Out-Null; Log \"   removed $($p.DisplayName)\" }\r\n\
         \x20           catch { Log \"   could not remove $($p.PackageName): $($_.Exception.Message)\" }\r\n\
         \x20       }\r\n\
         \x20   }\r\n\
         }\r\n\
         \r\n\
         Log 'Applying registry tweaks...'\r\n",
    );
    s.push_str(&format!(
        "reg load {} {}\r\n",
        ps_quote(DEFAULT_USER_HIVE),
        ps_quote(r"C:\Users\Default\NTUSER.DAT")
    ));
    for line in reg_lines {
        s.push_str(line);
        s.push_str("\r\n");
    }
    s.push_str("[gc]::Collect(); [gc]::WaitForPendingFinalizers()\r\n");
    s.push_str(&format!("reg unload {}\r\n", ps_quote(DEFAULT_USER_HIVE)));
    s.push_str(&format!("Log {}\r\n", ps_quote(&format!("   registry lines: {}", reg_lines.len()))));
    s.push_str("\r\nLog 'Windows features...'\r\n");
    s.push_str(
        "# Old Windows capabilities (WordPad, IE...): by name prefix, every version and language.\r\n\
         $caps = $null\r\n\
         function Remove-Capability([string]$name) {\r\n\
         \x20   if ($null -eq $script:caps) { $script:caps = @(Get-WindowsCapability -Online -ErrorAction SilentlyContinue) }\r\n\
         \x20   $found = @($script:caps | Where-Object { $_.Name -like \"$name~*\" -and $_.State -eq 'Installed' })\r\n\
         \x20   if ($found.Count -eq 0) { Log \"   capability ${name}: not installed\"; return }\r\n\
         \x20   foreach ($c in $found) {\r\n\
         \x20       try { Remove-WindowsCapability -Online -Name $c.Name -ErrorAction Stop | Out-Null; Log \"   capability $($c.Name): removed\" }\r\n\
         \x20       catch { Log \"   capability $($c.Name): $($_.Exception.Message)\" }\r\n\
         \x20   }\r\n\
         }\r\n\
         # Components to turn on take their files from sources\\sxs on the installation media.\r\n\
         $sxs = @(Get-PSDrive -PSProvider FileSystem | ForEach-Object { Join-Path $_.Root 'sources\\sxs' } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1 | ForEach-Object { \"/Source:$_\"; '/LimitAccess' })\r\n",
    );
    for line in feature_lines {
        s.push_str(line);
        s.push_str("\r\n");
    }
    if opts.wifi {
        // Профиль — для всех пользователей. Файл с паролем стирается сразу.
        // Копии файла ответов установщик потом переписывает заново, поэтому их
        // чистит ещё и первый вход (secrets_scrub_logon_line).
        s.push_str(&format!(
            "\r\nLog 'Wi-Fi profile...'\r\n\
             $wifi = {}\r\n\
             $out = & netsh.exe wlan add profile \"filename=$wifi\" user=all 2>&1\r\n\
             Log \"   $($out -join ' ')\"\r\n\
             Remove-Item -LiteralPath $wifi -Force -ErrorAction SilentlyContinue\r\n",
            ps_quote(WIFI_PROFILE),
        ));
    }
    if opts.product_key || opts.kms.is_some() {
        s.push_str("\r\nLog 'Activation settings...'\r\n$svc = Get-CimInstance SoftwareLicensingService\r\n");
    }
    if opts.product_key {
        // Ключ — из отдельного файла, чтобы его не было в specialize.ps1; в журнал — только хвост.
        s.push_str(&format!(
            "$keyFile = {}\r\n\
             try {{\r\n\
             \x20   $k = (Get-Content -LiteralPath $keyFile -Raw).Trim()\r\n\
             \x20   Invoke-CimMethod -InputObject $svc -MethodName InstallProductKey -Arguments @{{ ProductKey = $k }} | Out-Null\r\n\
             \x20   Log \"   product key installed: ...-$($k.Substring($k.Length - 5))\"\r\n\
             }} catch {{ Log \"   product key not installed: $($_.Exception.Message)\" }}\r\n\
             Remove-Item -LiteralPath $keyFile -Force -ErrorAction SilentlyContinue\r\n",
            ps_quote(PRODUCT_KEY_FILE)
        ));
    }
    if let Some((host, port)) = &opts.kms {
        // Активироваться Windows будет сама, когда появится связь с этим сервером.
        s.push_str(&format!(
            "try {{ Invoke-CimMethod -InputObject $svc -MethodName SetKeyManagementServiceMachine -Arguments @{{ MachineName = {} }} | Out-Null; Log {} }} catch {{ Log \"   KMS server not set: $($_.Exception.Message)\" }}\r\n",
            ps_quote(host),
            ps_quote(&format!("   KMS server: {host}"))
        ));
        if let Some(port) = port {
            s.push_str(&format!(
                "try {{ Invoke-CimMethod -InputObject $svc -MethodName SetKeyManagementServicePort -Arguments @{{ PortNumber = [uint32]{port} }} | Out-Null }} catch {{ Log \"   KMS port not set: $($_.Exception.Message)\" }}\r\n"
            ));
        }
    }
    if opts.wifi || opts.product_key {
        s.push_str(SECRETS_SCRUB);
        s.push_str("\r\n");
    }
    if opts.guest_tools {
        // Диск дополнений подключают к виртуалке вторым приводом перед установкой.
        s.push_str(
            "\r\nLog 'Guest additions...'\r\n\
             $roots = @(Get-PSDrive -PSProvider FileSystem | ForEach-Object { $_.Root })\r\n\
             $vbox = $roots | ForEach-Object { Join-Path $_ 'VBoxWindowsAdditions-amd64.exe' } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1\r\n\
             $vmw = $roots | ForEach-Object { Join-Path $_ 'setup64.exe' } | Where-Object { (Test-Path -LiteralPath $_) -and (Test-Path -LiteralPath (Join-Path (Split-Path $_) 'VMwareToolsUpgrader.exe')) } | Select-Object -First 1\r\n\
             if ($vbox) {\r\n\
             \x20   $cert = Join-Path (Split-Path $vbox) 'cert'\r\n\
             \x20   $util = Join-Path $cert 'VBoxCertUtil.exe'\r\n\
             \x20   if (Test-Path -LiteralPath $util) { Get-ChildItem -LiteralPath $cert -Filter 'vbox*.cer' | ForEach-Object { & $util add-trusted-publisher $_.FullName --root $_.FullName | Out-Null } }\r\n\
             \x20   $p = Start-Process -FilePath $vbox -ArgumentList '/S' -Wait -PassThru\r\n\
             \x20   Log \"   VirtualBox Guest Additions: exit code $($p.ExitCode)\"\r\n\
             } elseif ($vmw) {\r\n\
             \x20   $p = Start-Process -FilePath $vmw -ArgumentList '/S /v \"/qn REBOOT=R\"' -Wait -PassThru\r\n\
             \x20   Log \"   VMware Tools: exit code $($p.ExitCode)\"\r\n\
             } else { Log '   no guest additions disc found' }\r\n",
        );
    }
    if opts.compact_os {
        s.push_str(
            "\r\nLog 'CompactOS (takes a few minutes)...'\r\n\
             & compact.exe /CompactOS:always | Out-Null\r\n\
             Log \"   compact.exe exit code $LASTEXITCODE\"\r\n",
        );
    }
    s.push_str("Log 'WinForge: specialize finished'\r\n");
    s
}

/// Кодирует скрипт для `powershell -EncodedCommand` (base64 от UTF-16LE).
fn encode_ps(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64(&bytes)
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

// ---------------------------------------------------------------------------
// Скрипты
// ---------------------------------------------------------------------------

/// Описание своего установщика для скрипта.
#[derive(Debug, Clone)]
pub struct CustomInstall {
    pub name: String,
    pub value: String,
    pub args: String,
}

/// first-logon.cmd: твики, которые применяются при первом входе.
pub fn first_logon_script(lines: &[String]) -> String {
    let mut s = String::from("@echo off\r\nrem Created by WinForge: tweaks applied at first sign-in\r\n");
    for line in lines {
        s.push_str(line);
        s.push_str("\r\n");
    }
    s
}

/// Программа для установки через winget.
#[derive(Debug, Clone)]
pub struct WingetInstall {
    pub id: String,
    /// Папка самой программы (`winget --location`), None — куда ставит установщик.
    pub dir: Option<String>,
}

/// Папка программы для `winget --location`. Это папка самой программы, а не
/// родительская: с одним общим «C:\Program Files» все программы ложились бы
/// в его корень вперемешку. Поэтому каждой — своя подпапка, а для папки по
/// умолчанию `--location` не нужен вовсе: установщики сами создают подпапки.
pub fn app_install_dir(base: &str, app_name: &str) -> Option<String> {
    let base = base.trim().trim_end_matches('\\');
    let default = ["C:\\Program Files", "%ProgramFiles%", "$env:ProgramFiles"];
    if base.is_empty() || default.iter().any(|d| d.eq_ignore_ascii_case(base)) {
        return None;
    }
    let name: String = app_name.chars().filter(|c| !"<>:\"/\\|?*".contains(*c)).collect();
    let name = name.trim().trim_end_matches('.');
    if name.is_empty() {
        return None;
    }
    Some(format!("{base}\\{name}"))
}

/// setup-apps.ps1: установка программ при первом входе (winget + свои установщики).
/// `custom.value` — путь в новой системе (C:\WinForge\Installers\…) или ссылка.
pub fn setup_apps_script(apps: &[WingetInstall], custom: &[CustomInstall], delete_installers: bool) -> String {
    let mut s = String::from(
        "# Created by WinForge: installs apps at first sign-in\r\n\
         $ErrorActionPreference = 'Continue'\r\n\
         $log = \"$env:SystemDrive\\WinForge\\setup.log\"\r\n\
         function Log($m) { Add-Content -Path $log -Value \"$(Get-Date -Format o)  $m\" }\r\n\
         Log 'WinForge: app setup started'\r\n\
         # If the first sign-in is interrupted (PC turned off, power cut), the next\r\n\
         # sign-in carries on: a logon task reruns this script until it reaches the end,\r\n\
         # and every finished item is noted in setup-done.txt so it is not repeated.\r\n\
         $done = \"$env:SystemDrive\\WinForge\\setup-done.txt\"\r\n\
         $taskName = 'WinForge Setup Apps'\r\n\
         if (-not (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue)) {\r\n\
         \x20   $out = schtasks.exe /Create /TN $taskName /SC ONLOGON /RU \"$env:USERDOMAIN\\$env:USERNAME\" /IT /RL HIGHEST /F /TR \"powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Minimized -File `\"$PSCommandPath`\"\" 2>&1\r\n\
         \x20   if ($LASTEXITCODE -ne 0) { Log \"   resume task not created: $out\" }\r\n\
         }\r\n\
         function Test-Done([string]$key) { (Test-Path -LiteralPath $done) -and ((Get-Content -LiteralPath $done) -contains $key) }\r\n\
         function Set-Done([string]$key) { Add-Content -LiteralPath $done -Value $key }\r\n\
         # winget появляется не сразу после первого входа — ждём до 5 минут.\r\n\
         for ($i = 0; $i -lt 60 -and -not (Get-Command winget -ErrorAction SilentlyContinue); $i++) { Start-Sleep 5 }\r\n\r\n",
    );
    // Не все установщики умеют ставиться в выбранную папку (VC++, Telegram):
    // тогда повтор в папку по умолчанию. Итог каждой программы — в журнал.
    s.push_str(
        "# Success for winget: installed, already installed / nothing to upgrade, needs a restart.\r\n\
         $wingetOk = @(0, -1978335189, -1978335135, -1978334967)\r\n\
         function Install-App([string]$id, [string]$dir) {\r\n\
         \x20   if (Test-Done \"winget:$id\") { return }\r\n\
         \x20   Log \"winget: $id\"\r\n\
         \x20   $code = 1\r\n\
         \x20   if ($dir) {\r\n\
         \x20       $out = winget install --id $id -e --silent --no-upgrade --accept-package-agreements --accept-source-agreements --location $dir 2>&1\r\n\
         \x20       $code = $LASTEXITCODE\r\n\
         \x20       if ($code -notin $wingetOk) { Log \"   not installed into $dir (code $code), retrying in the default folder\" }\r\n\
         \x20   }\r\n\
         \x20   if (-not $dir -or $code -notin $wingetOk) {\r\n\
         \x20       $out = winget install --id $id -e --silent --no-upgrade --accept-package-agreements --accept-source-agreements 2>&1\r\n\
         \x20       $code = $LASTEXITCODE\r\n\
         \x20   }\r\n\
         \x20   # 0x8A15003F: the package index is damaged (e.g. the PC was turned off while it was\r\n\
         \x20   # downloading). Fetch it again and retry once.\r\n\
         \x20   if ($code -eq -1978335169) {\r\n\
         \x20       Log '   package index damaged, refreshing it and retrying'\r\n\
         \x20       winget source reset --force 2>&1 | Out-Null\r\n\
         \x20       winget source update 2>&1 | Out-Null\r\n\
         \x20       $out = winget install --id $id -e --silent --no-upgrade --accept-package-agreements --accept-source-agreements 2>&1\r\n\
         \x20       $code = $LASTEXITCODE\r\n\
         \x20   }\r\n\
         \x20   if ($code -eq 0) { Log '   ok' } elseif ($code -in $wingetOk) { Log '   ok (already installed)' } else { Log \"   FAILED (code $code): $((@($out) | Where-Object { \"$_\".Trim() } | Select-Object -Last 2) -join ' | ')\" }\r\n\
         \x20   Set-Done \"winget:$id\"\r\n\
         }\r\n\r\n",
    );
    for a in apps {
        match &a.dir {
            Some(dir) => s.push_str(&format!("Install-App {} {}\r\n", ps_quote(&a.id), ps_quote(dir))),
            None => s.push_str(&format!("Install-App {}\r\n", ps_quote(&a.id))),
        }
    }
    s.push_str("\r\n");
    if !custom.is_empty() {
        s.push_str(&format!("$installers = {}\r\n", ps_quote(&format!("{SETUP_DIR}\\Installers"))));
        // Свои установщики едут на флешке/диске в WinForge\Installers; при установке
        // specialize.ps1 копирует их в C:\WinForge\Installers. Если не вышло —
        // ищем на носителе ещё раз (он обычно ещё вставлен).
        s.push_str(
            "function Find-Installers {\r\n\
             \x20   if (Test-Path -LiteralPath $installers) { return }\r\n\
             \x20   foreach ($d in Get-PSDrive -PSProvider FileSystem) {\r\n\
             \x20       $src = Join-Path $d.Root 'WinForge\\Installers'\r\n\
             \x20       if ($d.Root -ne \"$env:SystemDrive\\\" -and (Test-Path -LiteralPath $src)) {\r\n\
             \x20           New-Item -ItemType Directory -Force -Path $installers | Out-Null\r\n\
             \x20           Copy-Item -Path (Join-Path $src '*') -Destination $installers -Force\r\n\
             \x20           Log \"   installers copied from $src\"\r\n\
             \x20           return\r\n\
             \x20       }\r\n\
             \x20   }\r\n\
             \x20   Log '   WinForge\\Installers not found: was the USB drive or disc removed?'\r\n\
             }\r\n\
             function Install-Custom([string]$name, [string]$file, [string]$params, [string]$download) {\r\n\
             \x20   if (Test-Done \"installer:$name\") { return }\r\n\
             \x20   Log \"installer: $name\"\r\n\
             \x20   try {\r\n\
             \x20       if ($download) { $file = Join-Path $env:TEMP $file; Invoke-WebRequest -Uri $download -OutFile $file -UseBasicParsing }\r\n\
             \x20       elseif (-not (Test-Path -LiteralPath $file)) { Find-Installers }\r\n\
             \x20       if (-not (Test-Path -LiteralPath $file)) { Log \"   FAILED: file not found: $file\"; return }\r\n\
             \x20       if ($file -like '*.msi') { $p = Start-Process msiexec.exe -ArgumentList \"/i `\"$file`\" $params\" -PassThru }\r\n\
             \x20       elseif ($params) { $p = Start-Process -FilePath $file -ArgumentList $params -PassThru }\r\n\
             \x20       else { $p = Start-Process -FilePath $file -PassThru }\r\n\
             \x20       $null = $p.Handle\r\n\
             \x20       # Установщик, который ждёт кликов, не должен держать остальные программы вечно.\r\n\
             \x20       if (-not $p.WaitForExit(1800000)) { Log '   still running after 30 min (waiting for clicks?), moving on'; return }\r\n\
             \x20       # 3010 и 1641 — установлено, нужна перезагрузка.\r\n\
             \x20       if ($p.ExitCode -in 0, 3010, 1641) { Log '   ok' } else { Log \"   FAILED (code $($p.ExitCode))\" }\r\n\
             \x20   } catch { Log \"   FAILED: $($_.Exception.Message)\" }\r\n\
             \x20   Set-Done \"installer:$name\"\r\n\
             }\r\n\r\n",
        );
    }
    for c in custom {
        let (file, download) = if crate::sys::installer::is_url(&c.value) {
            let ext = if c.value.to_lowercase().split(['?', '#']).next().unwrap_or("").ends_with(".msi") {
                "msi"
            } else {
                "exe"
            };
            let file = sanitize_file(&c.name).trim_end_matches(".exe").to_owned() + "." + ext;
            (file, c.value.trim().to_owned())
        } else {
            (c.value.clone(), String::new())
        };
        s.push_str(&format!(
            "Install-Custom {} {} {} {}\r\n",
            ps_quote(&c.name),
            ps_quote(&file),
            ps_quote(c.args.trim()),
            ps_quote(&download)
        ));
    }
    if !custom.is_empty() && delete_installers {
        s.push_str(
            "if (Test-Path -LiteralPath $installers) { Remove-Item -LiteralPath $installers -Recurse -Force -ErrorAction SilentlyContinue; Log 'installers removed' }\r\n",
        );
    }
    // Дошли до конца — продолжать при следующем входе больше не нужно.
    s.push_str(
        "schtasks.exe /Delete /TN $taskName /F 2>&1 | Out-Null\r\n\
         Log 'WinForge: app setup finished'\r\n",
    );
    s
}

/// Сколько свободного места нужно для сборки: копия носителя, экспорт
/// install.wim и готовый ISO (по ~7–8 ГБ) с запасом.
pub const BUILD_SPACE_GB: u32 = 30;

/// Всё, что нужно скрипту сборки. Твики сюда не входят: они вшиты в
/// autounattend.xml и применяются установщиком Windows.
pub struct BuildInput<'a> {
    pub source_iso: &'a str,
    pub output_iso: &'a str,
    pub edition: &'a str,
    /// Папка драйверов: копируется в `$WinPEDriver$` носителя, установщик
    /// сам ставит их и в среду установки, и в новую систему.
    pub drivers: &'a str,
    /// Выгрузить сторонние драйверы этого ПК и тоже положить в `$WinPEDriver$`
    /// (для переустановки на этом же компьютере).
    pub export_drivers: bool,
    /// Базовая папка; скрипт работает только в её подпапке `WinForge-work`.
    pub work_dir: &'a str,
    /// Удалить рабочую папку после успешной сборки ISO.
    pub cleanup: bool,
    /// Свои установщики: (файл на этом ПК, имя на носителе). Кладутся в
    /// `WinForge\Installers` флешки/ISO; оттуда их забирает установка Windows.
    pub installers: &'a [(String, String)],
    /// Записать на флешку вместо ISO. Скрипт перед стиранием сверяет диск
    /// (USB, не системный, тот же серийный номер и размер) и просит ввести YES.
    pub usb: Option<UsbTarget<'a>>,
}

/// Флешка, выбранная в WinForge.
pub struct UsbTarget<'a> {
    pub number: u32,
    pub serial: &'a str,
    pub size: u64,
}

/// Вспомогательный C#: проценты в файл прогресса, отмена из WinForge,
/// копирование носителя с процентами и запись ISO из IMAPI2.
/// Отмена — появление файла CancelFile: проверяется на каждом проценте.
const HELPERS_CS: &str = r#"if (-not ('WfProgress' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
public static class WfProgress {
    public static string ProgressFile = "";
    public static string CancelFile = "";
    public static void CheckCancel() {
        if (!string.IsNullOrEmpty(CancelFile) && File.Exists(CancelFile)) { throw new OperationCanceledException("Cancelled in WinForge."); }
    }
    // Часть шага, которую занимает операция (например, копирование — 0–40 % шага 4).
    public static int From = 0;
    public static int To = 100;
    public static void Pct(int pct) {
        CheckCancel();
        pct = From + pct * (To - From) / 100;
        if (string.IsNullOrEmpty(ProgressFile)) { return; }
        try { File.AppendAllText(ProgressFile, "PCT " + pct + "\r\n"); } catch { }
    }
}
public static class WfCopy {
    // Копирует папку целиком (кроме exclude), снимает «только чтение» с копий с диска.
    public static void Tree(string from, string to, string[] exclude) {
        string root = from.TrimEnd('\\');
        var files = new System.Collections.Generic.List<string>();
        long total = 0;
        foreach (string d in Directory.EnumerateDirectories(from, "*", SearchOption.AllDirectories)) {
            Directory.CreateDirectory(Path.Combine(to, d.Substring(root.Length).TrimStart('\\')));
        }
        foreach (string f in Directory.EnumerateFiles(from, "*", SearchOption.AllDirectories)) {
            if (Array.IndexOf(exclude, Path.GetFileName(f).ToLowerInvariant()) >= 0) { continue; }
            files.Add(f);
            total += new FileInfo(f).Length;
        }
        if (total == 0) { total = 1; }
        long done = 0;
        int last = -1;
        byte[] buf = new byte[4 << 20];
        foreach (string f in files) {
            string dst = Path.Combine(to, f.Substring(root.Length).TrimStart('\\'));
            Directory.CreateDirectory(Path.GetDirectoryName(dst));
            using (FileStream i = new FileStream(f, FileMode.Open, FileAccess.Read, FileShare.Read, 1 << 20))
            using (FileStream o = new FileStream(dst, FileMode.Create, FileAccess.Write, FileShare.None, 1 << 20)) {
                int n;
                while ((n = i.Read(buf, 0, buf.Length)) > 0) {
                    o.Write(buf, 0, n);
                    done += n;
                    int pct = (int)(done * 100 / total);
                    if (pct != last) { last = pct; WfProgress.Pct(pct); }
                }
            }
            File.SetAttributes(dst, FileAttributes.Normal);
        }
        WfProgress.Pct(100);
    }
}
public static class WfIsoWriter {
    public static void Save(object stream, string path, long totalBytes) {
        IStream s = (IStream)stream;
        byte[] buf = new byte[4 * 1024 * 1024];
        IntPtr read = Marshal.AllocHGlobal(4);
        int last = -1;
        try {
            using (FileStream fs = new FileStream(path, FileMode.Create, FileAccess.Write, FileShare.None, 1 << 20)) {
                long left = totalBytes;
                while (left > 0) {
                    int want = (int)Math.Min(buf.Length, left);
                    s.Read(buf, want, read);
                    int got = Marshal.ReadInt32(read);
                    if (got <= 0) { break; }
                    fs.Write(buf, 0, got);
                    left -= got;
                    int pct = (int)((totalBytes - left) * 100 / totalBytes);
                    if (pct != last) { last = pct; WfProgress.Pct(pct); }
                }
            }
        } finally { Marshal.FreeHGlobal(read); }
    }
}
'@
}
[WfProgress]::ProgressFile = $ProgressFile
[WfProgress]::CancelFile = $CancelFile
"#;

/// Собирает загрузочный ISO встроенным в Windows IMAPI2 (без ADK):
/// UDF 1.02 как у oscdimg -udfver102, загрузчики BIOS (etfsboot) и UEFI (efisys).
/// Проверено: грузится в UEFI с Secure Boot и в BIOS, install.wim > 4 ГБ цел.
const NEW_BOOT_ISO_FN: &str = r#"function New-BootIso([string]$Source, [string]$Output) {
    function Get-BootStream($path) {
        $s = New-Object -ComObject ADODB.Stream
        $s.Type = 1
        $s.Open()
        $s.LoadFromFile($path)
        $s
    }
    # IMAPI2 держит исходные файлы открытыми, пока живут его объекты, поэтому
    # в finally они освобождаются явно — иначе следующая сборка не может
    # удалить старую папку media.
    $made = New-Object System.Collections.ArrayList
    try {
        $image = New-Object -ComObject IMAPI2FS.MsftFileSystemImage; [void]$made.Add($image)
        $image.ChooseImageDefaultsForMediaType(0x12)
        $image.FileSystemsToCreate = 4
        $image.UDFRevision = 0x102
        $image.VolumeName = 'WINFORGE'
        $biosStream = Get-BootStream (Join-Path $Source 'boot\etfsboot.com'); [void]$made.Add($biosStream)
        $bios = New-Object -ComObject IMAPI2FS.BootOptions; [void]$made.Add($bios)
        $bios.AssignBootImage($biosStream)
        $bios.PlatformId = 0
        $bios.Emulation = 0
        $uefiStream = Get-BootStream (Join-Path $Source 'efi\microsoft\boot\efisys.bin'); [void]$made.Add($uefiStream)
        $uefi = New-Object -ComObject IMAPI2FS.BootOptions; [void]$made.Add($uefi)
        $uefi.AssignBootImage($uefiStream)
        $uefi.PlatformId = 0xEF
        $uefi.Emulation = 0
        $bootArray = [object[]]@($bios.psobject.BaseObject, $uefi.psobject.BaseObject)
        [void][System.__ComObject].InvokeMember('BootImageOptionsArray', [System.Reflection.BindingFlags]::SetProperty, $null, $image, @(, $bootArray))
        $image.Root.AddTree($Source, $false)
        $result = $image.CreateResultImage(); [void]$made.Add($result)
        $stream = $result.ImageStream; [void]$made.Add($stream)
        $total = [int64]$result.TotalBlocks * $result.BlockSize
        [WfIsoWriter]::Save($stream, $Output, $total)
    } finally {
        foreach ($s in $biosStream, $uefiStream) { if ($s) { try { $s.Close() } catch {} } }
        $made.Reverse()
        foreach ($o in $made) { try { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($o) } catch {} }
        $made = $null; $image = $null; $result = $null; $stream = $null; $bios = $null; $uefi = $null
        [gc]::Collect(); [gc]::WaitForPendingFinalizers(); [gc]::Collect()
    }
}
"#;

/// Начало build.ps1: параметры запуска из WinForge, протокол прогресса,
/// журнал и аварийная остановка.
///
/// Протокол (строки в -ProgressFile): `LOG <путь журнала>`, `STEP <n> <всего>`,
/// `PCT <0..100>` или `PCT -1` (без процентов), `WARN <текст>`, `DONE <путь>`,
/// `ERROR <текст>`. Файл -CancelFile просит остановить сборку.
const BUILD_HEAD: &str = r#"#Requires -RunAsAdministrator
param([string]$ProgressFile = '', [string]$CancelFile = '', [switch]$Confirmed)
$ErrorActionPreference = 'Stop'
$steps = 4
function Report([string]$line) {
    if ($ProgressFile) { try { [IO.File]::AppendAllText($ProgressFile, "$line`r`n") } catch {} }
}
function Warn([string]$m) { Write-Warning $m; Report "WARN $($m.Trim())" }
function Test-Cancel { if ($CancelFile -and (Test-Path -LiteralPath $CancelFile)) { throw 'Cancelled in WinForge.' } }
function Step([int]$n, [string]$text) {
    Test-Cancel
    Write-Host "$n/$steps $text"
    Report "STEP $n $steps"
}
# Весь вывод — ещё и в журнал рядом со скриптом: по нему видно, где упала сборка.
# Свой файл на каждый запуск; журнал не обязателен: если не открылся, сборка идёт.
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$logFile = Join-Path $PSScriptRoot "build-$stamp.log"
try { Start-Transcript -Path $logFile -Force | Out-Null; Report "LOG $logFile" } catch { Warn "Could not start the build log: $($_.Exception.Message)" }
# Любая аварийная остановка: причина — в WinForge, журнал закрывается.
trap {
    $e = $_.Exception
    while ($e.InnerException) { $e = $e.InnerException }
    # Отменили в WinForge — недокопированный носитель (несколько ГБ) не оставляем.
    # Удаляется только своя папка WinForge-work.
    if ($e.Message -eq 'Cancelled in WinForge.' -and $cleanup -and $work -and (Split-Path $work -Leaf) -eq 'WinForge-work') {
        try { if ($source) { Dismount-DiskImage -ImagePath $source -ErrorAction SilentlyContinue | Out-Null } } catch {}
        [gc]::Collect(); [gc]::WaitForPendingFinalizers()
        Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
    }
    Report "ERROR $($e.Message)"
    try { Stop-Transcript | Out-Null } catch {}
    break
}
"#;

/// Подготовка и шаги 1–3: остатки прошлых сборок, копия носителя, редакция, файл ответов.
const BUILD_BODY: &str = r#"$here  = $PSScriptRoot
$media = Join-Path $work 'media'
if (-not (Test-Path $source)) { throw "Source ISO not found: $source" }
if (-not (Test-Path (Join-Path $here 'autounattend.xml'))) { throw 'autounattend.xml not found next to build.ps1. Prepare the build in WinForge again.' }

# Leftovers of an interrupted build. Older WinForge versions mounted the image
# here and loaded its registry: unmount and remove only our own items.
$workFull = [IO.Path]::GetFullPath($work).TrimEnd('\')
function Test-InWork([string]$p) { $p -and [IO.Path]::GetFullPath($p).StartsWith($workFull, [StringComparison]::OrdinalIgnoreCase) }
# reg через cmd: в PowerShell 5 перенаправление stderr внешней программы
# при ErrorActionPreference=Stop превращается в аварийную остановку.
foreach ($h in 'HKLM\WF_SOFT', 'HKLM\WF_SYS', 'HKLM\WF_USER') { cmd /c "reg unload $h >nul 2>&1" }
$stale = @(Get-WindowsImage -Mounted -ErrorAction SilentlyContinue | Where-Object { Test-InWork $_.Path })
foreach ($m in $stale) {
    Write-Host "   unmounting a leftover image: $($m.Path)"
    try { Dismount-WindowsImage -Path $m.Path -Discard | Out-Null } catch { Warn "   could not unmount $($m.Path): $($_.Exception.Message)" }
}
if ($stale.Count -gt 0) { cmd /c "dism /Cleanup-Mountpoints >nul 2>&1" }
$busy = @(Get-WindowsImage -Mounted -ErrorAction SilentlyContinue | Where-Object { Test-InWork $_.Path } | ForEach-Object { [IO.Path]::GetFullPath($_.Path).TrimEnd('\') })
foreach ($d in @(Get-ChildItem $work -Directory -Filter 'mount*' -ErrorAction SilentlyContinue)) {
    if ($busy -contains $d.FullName.TrimEnd('\')) { Warn "   $($d.FullName) is still in use, it will go away after a restart"; continue }
    try { Remove-Item $d.FullName -Recurse -Force } catch { Warn "   could not remove the old $($d.FullName): $($_.Exception.Message)" }
}
foreach ($p in $media, (Join-Path $work 'install.wim')) {
    if (Test-Path $p) {
        try { Remove-Item $p -Recurse -Force }
        catch { throw "Could not remove the leftover $p ($($_.Exception.Message)). Close other WinForge builds and try again; if that does not help, restart the PC." }
    }
}
New-Item -ItemType Directory -Force -Path $media | Out-Null
# Готовый ISO может держать другая программа (чаще всего виртуалка, к которой он
# подключён как DVD). Проверяем сразу, а не после 10 минут сборки.
if ($target -eq 'iso' -and (Test-Path -LiteralPath $output)) {
    try { $probe = [IO.File]::Open($output, 'Open', 'ReadWrite', 'None'); $probe.Close() }
    catch { throw "The file $output is in use by another program (often a virtual machine that has it attached as a DVD). Close or detach it and run the build again." }
}
$root = [IO.Path]::GetPathRoot($work)
$free = (New-Object IO.DriveInfo $root).AvailableFreeSpace
if ($free -lt $needGB * 1GB) { throw "Not enough free space on ${root}: need about $needGB GB, free $([math]::Round($free / 1GB)) GB. Pick another work folder in WinForge." }

Step 1 'Copying installation media...'
$disk = Mount-DiskImage -ImagePath $source -PassThru
try {
    $drive = ($disk | Get-Volume).DriveLetter + ':\'
    [WfCopy]::Tree($drive, $media, [string[]]@())
} finally {
    Dismount-DiskImage -ImagePath $source | Out-Null
}

Step 2 'Selecting the edition...'
Report 'PCT -1'
$wim = Join-Path $media 'sources\install.wim'
$esd = Join-Path $media 'sources\install.esd'
$src = if (Test-Path $wim) { $wim } else { $esd }
$all = Get-WindowsImage -ImagePath $src
$info = $all | Where-Object { $_.ImageName -eq $edition } | Select-Object -First 1
if (-not $info) { throw "Edition '$edition' not found. Available: $(($all.ImageName) -join ', ')" }
$newWim = Join-Path $work 'install.wim'
Export-WindowsImage -SourceImagePath $src -SourceIndex $info.ImageIndex -DestinationImagePath $newWim -CompressionType max | Out-Null
Remove-Item $wim, $esd -Force -ErrorAction SilentlyContinue
Move-Item $newWim $wim -Force

Step 3 'Adding the answer file with the WinForge tweaks...'
Copy-Item (Join-Path $here 'autounattend.xml') $media -Force
"#;

/// Выгрузка драйверов этого ПК в `$WinPEDriver$\ThisPC` носителя. Сначала
/// Export-WindowsDriver, при ошибке — pnputil. Временная папка внутри `$work`.
const DRIVER_EXPORT: &str = r#"Test-Cancel
Write-Host '   + drivers of this PC (can take a few minutes)'
Report 'PCT -1'
$exp = Join-Path $work 'this-pc-drivers'
Remove-Item -LiteralPath $exp -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $exp | Out-Null
try { Export-WindowsDriver -Online -Destination $exp -ErrorAction Stop | Out-Null }
catch {
    Write-Host "   Export-WindowsDriver failed ($($_.Exception.Message)), trying pnputil"
    # stderr сливает cmd: в PowerShell 5.1 с ErrorActionPreference=Stop `2>&1` превращает его в ошибку.
    $out = cmd.exe /c "pnputil.exe /export-driver * `"$exp`" 2>&1"
    if ($LASTEXITCODE -ne 0) { Write-Host "   pnputil: $((@($out) | Select-Object -Last 2) -join ' | ')" }
}
Test-Cancel
# -File: иначе -Filter *.inf совпадает и с короткими именами папок «xxx.inf_amd64_…» (8.3).
$infs = @(Get-ChildItem -LiteralPath $exp -Recurse -Filter *.inf -File -ErrorAction SilentlyContinue).Count
if ($infs -gt 0) {
    $mb = [math]::Round(((Get-ChildItem -LiteralPath $exp -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum) / 1MB)
    Write-Host "   $infs drivers, $mb MB"
    # Установщик держит $WinPEDriver$ в маленьком диске в памяти: гигабайты драйверов
    # (видеокарта) обрывают установку ошибкой 0x80070070. Туда — только контроллеры
    # дисков (без них установщик может не увидеть диск), остальное ставит specialize.
    $pe = Join-Path $media '$WinPEDriver$\ThisPC-storage'
    $rest = Join-Path $media 'WinForge\Drivers'
    New-Item -ItemType Directory -Force -Path $rest | Out-Null
    $storage = 0
    foreach ($d in @(Get-ChildItem -LiteralPath $exp -Directory)) {
        Test-Cancel
        $inf = Get-ChildItem -LiteralPath $d.FullName -Filter *.inf -File | Select-Object -First 1
        $m = if ($inf) { Select-String -LiteralPath $inf.FullName -Pattern '^\s*Class\s*=\s*"?(\w+)' | Select-Object -First 1 } else { $null }
        $class = if ($m) { $m.Matches[0].Groups[1].Value } else { '' }
        $to = if ($class -in @('SCSIAdapter', 'HDC')) { $storage++; $pe } else { $rest }
        robocopy $d.FullName (Join-Path $to $d.Name) /E /NFL /NDL /NJH /NJS /NP | Out-Null
        if ($LASTEXITCODE -ge 8) { Warn "   driver not copied: $($d.Name)" }
    }
    # Файлы прямо в корне выгрузки (обычно каждый драйвер в своей папке) — тоже в WinForge\Drivers.
    if (@(Get-ChildItem -LiteralPath $exp -File).Count -gt 0) {
        robocopy $exp (Join-Path $rest 'loose') /NFL /NDL /NJH /NJS /NP | Out-Null
        if ($LASTEXITCODE -ge 8) { Warn '   some loose driver files were not copied' }
    }
    Write-Host "   disk controller drivers for Setup: $storage; the rest installs inside the new Windows"
} else { Warn '   drivers of this PC were not exported: the image is built without them' }
Remove-Item -LiteralPath $exp -Recurse -Force -ErrorAction SilentlyContinue
"#;

/// Шаг 4 для ISO: oscdimg из ADK, если есть, иначе встроенный IMAPI2.
const ISO_STEP: &str = r#"Step 4 'Building the bootable ISO (BIOS + UEFI)...'
New-Item -ItemType Directory -Force -Path (Split-Path $output) | Out-Null
$oscdimg = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\Assessment and Deployment Kit\Deployment Tools\amd64\Oscdimg\oscdimg.exe" -ErrorAction SilentlyContinue
if ($oscdimg) {
    Report 'PCT -1'
    $boot = "2#p0,e,b$media\boot\etfsboot.com#pEF,e,b$media\efi\microsoft\boot\efisys.bin"
    & $oscdimg.FullName -m -o -u2 -udfver102 "-bootdata:$boot" -lWINFORGE $media $output
    if ($LASTEXITCODE -ne 0) { throw "oscdimg failed with code $LASTEXITCODE" }
} else {
    Write-Host '   using the built-in Windows IMAPI2 (no Windows ADK needed)'
    New-BootIso -Source $media -Output $output
}
Write-Host "Done: $output" -ForegroundColor Green
$done = $output
$keepWork = [IO.Path]::GetFullPath($output).StartsWith([IO.Path]::GetFullPath($work), [StringComparison]::OrdinalIgnoreCase)
"#;

/// Шаг 4 для флешки. Стирает только проверенный USB-диск и только после «YES»
/// (в WinForge — перед запуском, тогда скрипт получает -Confirmed).
const USB_STEP: &str = r#"Step 4 'Writing to the USB drive...'
$disk = Get-Disk -Number $usbNumber -ErrorAction Stop
if ($disk.BusType -ne 'USB' -or $disk.IsBoot -or $disk.IsSystem) { throw "Disk $usbNumber is not a removable USB drive. Nothing was erased." }
if ($usbSerial -and ("$($disk.SerialNumber)".Trim() -ne $usbSerial)) { throw 'The USB drive was replaced (serial number differs). Nothing was erased. Pick it again in WinForge.' }
if ([math]::Abs([int64]$disk.Size - $usbSize) -gt 16MB) { throw 'The USB drive was replaced (size differs). Nothing was erased. Pick it again in WinForge.' }
$need = (Get-ChildItem $media -Recurse -File | Measure-Object Length -Sum).Sum + 64MB
if ($disk.Size -lt $need) { throw ('The USB drive is too small: it needs at least {0:N1} GB.' -f ($need / 1GB)) }
Write-Host ''
Write-Host ('   {0}  ({1:N1} GB, disk {2})' -f $disk.FriendlyName, ($disk.Size / 1GB), $disk.Number) -ForegroundColor Yellow
if (-not $Confirmed) {
    $answer = Read-Host '   ALL DATA on this USB drive will be erased. Type YES to continue'
    if ($answer -cne 'YES') { throw 'Cancelled. The USB drive was not touched.' }
}
Test-Cancel
Report 'PCT -1'
Set-Disk -Number $usbNumber -IsOffline $false -ErrorAction SilentlyContinue
Set-Disk -Number $usbNumber -IsReadOnly $false -ErrorAction SilentlyContinue
Clear-Disk -Number $usbNumber -RemoveData -RemoveOEM -Confirm:$false
Initialize-Disk -Number $usbNumber -PartitionStyle MBR -ErrorAction SilentlyContinue
# FAT32: UEFI boots from it on any PC. Format-Volume makes FAT32 only up to 32 GB.
$size = [math]::Min([int64]$disk.Size - 8MB, 32GB - 8MB)
$part = New-Partition -DiskNumber $usbNumber -Size $size -IsActive -AssignDriveLetter
Format-Volume -Partition $part -FileSystem FAT32 -NewFileSystemLabel 'WINFORGE' -Confirm:$false | Out-Null
for ($i = 0; $i -lt 20 -and -not $part.DriveLetter; $i++) { Start-Sleep -Milliseconds 500; $part = Get-Partition -DiskNumber $usbNumber -PartitionNumber $part.PartitionNumber }
if (-not $part.DriveLetter) { throw 'The USB partition got no drive letter.' }
$usb = "$($part.DriveLetter):"
Write-Host "   copying files to $usb ..."
# Шаг 4 на флешку: копирование — 0–40 %, нарезка install.wim — 40–99 %
# (иначе полоска дошла бы до 100 % и откатилась назад).
[WfProgress]::From = 0; [WfProgress]::To = 40
[WfCopy]::Tree($media, "$usb\", [string[]]@('install.wim'))
[WfProgress]::From = 0; [WfProgress]::To = 100
# FAT32 can't hold files over 4 GB: Windows Setup reads split install.swm natively.
Write-Host '   splitting install.wim into install.swm parts...'
# На флешку это самая долгая часть (несколько ГБ при 5–30 МБ/с). Отдельный процесс dism:
# Split-WindowsImage не прерывается, а процесс при отмене можно остановить сразу.
# Процентов dism не даёт — считаем их по размеру уже записанных частей.
$wimPath = Join-Path $media 'sources\install.wim'
# [int64] у обоих аргументов: иначе PowerShell 5.1 выбирает Max(int, int) и падает
# на файле больше 2 ГБ («Value was either too large or too small for an Int32»).
$wimSize = [math]::Max([int64]1, [int64](Get-Item -LiteralPath $wimPath).Length)
Report 'PCT 40'
$dism = Start-Process -FilePath "$env:SystemRoot\System32\dism.exe" -WindowStyle Hidden -PassThru -ArgumentList @(
    '/Split-Image', "/ImageFile:`"$wimPath`"", "/SWMFile:`"$usb\sources\install.swm`"", '/FileSize:3800', '/CheckIntegrity')
$null = $dism.Handle  # без этого ExitCode у Start-Process -PassThru бывает пустым
try {
    while (-not $dism.HasExited) {
        Start-Sleep -Seconds 2
        $written = (Get-ChildItem -LiteralPath "$usb\sources" -Filter 'install*.swm' -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
        Report ('PCT {0}' -f (40 + [math]::Min(59, [int](59 * [double]$written / $wimSize))))
        if ($CancelFile -and (Test-Path -LiteralPath $CancelFile)) {
            throw 'Cancelled in WinForge. The USB drive was already erased and is now incomplete: write it again before using it.'
        }
    }
    $dism.WaitForExit()
    if ($dism.ExitCode -ne 0) { throw "Could not split install.wim (dism exit code $($dism.ExitCode))." }
} finally {
    if (-not $dism.HasExited) { try { $dism.Kill() } catch { } }
}
Report 'PCT 100'
# Boot code for older BIOS PCs (UEFI doesn't need it).
& (Join-Path $media 'boot\bootsect.exe') /nt60 $usb /mbr | Out-Null
Write-Host "Done: bootable USB drive $usb" -ForegroundColor Green
$done = "$usb ($($disk.FriendlyName))"
$keepWork = $false
"#;

/// Конец build.ps1: уборка рабочей папки и итог для WinForge.
const BUILD_TAIL: &str = r#"if ($cleanup -and -not $keepWork) {
    [gc]::Collect(); [gc]::WaitForPendingFinalizers()
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path $work) { Warn "Some temporary files are still in use and stay in $work. The next build removes them." }
    else { Write-Host 'Temporary build files removed.' }
}
Report "DONE $done"
try { Stop-Transcript | Out-Null } catch {}
"#;

/// build.ps1: распаковка ISO, выбор редакции, файл ответов, ISO или флешка.
/// Образ не монтируется и не правится: всё применяет установщик Windows.
/// WinForge запускает его скрыто и показывает прогресс сам (см. BUILD_HEAD).
pub fn build_script(b: &BuildInput) -> String {
    let crlf = |t: &str| t.replace("\r\n", "\n").replace('\n', "\r\n");
    let mut s = String::new();
    s.push_str("# Created by WinForge: builds a custom Windows ISO or USB drive.\r\n");
    s.push_str(&format!(
        "# Run as administrator. Needs ~{BUILD_SPACE_GB} GB free space on the work drive.\r\n"
    ));
    s.push_str("# Tweaks, app removal and first sign-in scripts are inside autounattend.xml:\r\n");
    s.push_str("# Windows Setup applies them in the system being installed.\r\n");
    s.push_str(&crlf(BUILD_HEAD));
    s.push_str(&format!("$source  = {}\r\n", ps_quote(b.source_iso)));
    s.push_str(&format!("$output  = {}\r\n", ps_quote(b.output_iso)));
    s.push_str(&format!("$edition = {}\r\n", ps_quote(b.edition)));
    // Скрипт трогает только свою подпапку WinForge-work, что бы ни выбрал пользователь.
    s.push_str(&format!("$work    = Join-Path {} 'WinForge-work'\r\n", ps_quote(b.work_dir)));
    s.push_str(&format!("$cleanup = ${}\r\n", if b.cleanup { "true" } else { "false" }));
    s.push_str(&format!("$needGB  = {BUILD_SPACE_GB}\r\n"));
    match &b.usb {
        Some(u) => {
            s.push_str("$target    = 'usb'\r\n");
            s.push_str(&format!("$usbNumber = {}\r\n", u.number));
            s.push_str(&format!("$usbSerial = {}\r\n", ps_quote(u.serial.trim())));
            s.push_str(&format!("$usbSize   = {}\r\n", u.size));
        }
        None => s.push_str("$target    = 'iso'\r\n"),
    }
    s.push_str(&crlf(HELPERS_CS));
    s.push_str(&crlf(NEW_BOOT_ISO_FN));
    s.push_str(&crlf(BUILD_BODY));
    if !b.drivers.trim().is_empty() {
        // Установщик сам берёт драйверы из папки $WinPEDriver$ в корне носителя.
        s.push_str(&format!("$drivers = {}\r\n", ps_quote(b.drivers.trim())));
        s.push_str(&crlf(
            "if (Test-Path $drivers) {\n\
             \x20   Write-Host '   + drivers ($WinPEDriver$)'\n\
             \x20   $mb = [math]::Round(((Get-ChildItem -LiteralPath $drivers -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum) / 1MB)\n\
             \x20   if ($mb -gt 400) { Warn \"   the drivers folder is $mb MB: Setup keeps it in a small RAM disk and may stop with error 0x80070070. Keep only network and disk drivers there.\" }\n\
             \x20   robocopy $drivers (Join-Path $media '$WinPEDriver$') /E /NFL /NDL /NJH /NJS /NP | Out-Null\n\
             \x20   if ($LASTEXITCODE -ge 8) { Warn '   some drivers were not copied' }\n\
             } else { Warn \"   drivers folder not found: $drivers\" }\n",
        ));
    }
    if b.export_drivers {
        // Только сторонние драйверы (видеокарта, Wi-Fi, чипсет…): встроенные в Windows и так есть.
        // Не получилось — сборка идёт дальше без них.
        s.push_str(&crlf(DRIVER_EXPORT));
    }
    if !b.installers.is_empty() {
        // Хэш-таблицы, а не вложенные массивы: массив из одного массива PowerShell разворачивает.
        let list = b
            .installers
            .iter()
            .map(|(from, name)| format!("@{{ From = {}; Name = {} }}", ps_quote(from), ps_quote(name)))
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str(&format!("$installers = @({list})\r\n"));
        s.push_str(&crlf(
            "Write-Host \"   + own installers: $($installers.Count)\"\n\
             $instDir = Join-Path $media 'WinForge\\Installers'\n\
             New-Item -ItemType Directory -Force -Path $instDir | Out-Null\n\
             foreach ($i in $installers) {\n\
             \x20   Test-Cancel\n\
             \x20   if (Test-Path -LiteralPath $i.From) { Copy-Item -LiteralPath $i.From -Destination (Join-Path $instDir $i.Name) -Force }\n\
             \x20   else { Warn \"   installer not found, skipped: $($i.From)\" }\n\
             }\n",
        ));
    }
    s.push_str("\r\n");
    s.push_str(&crlf(if b.usb.is_some() { USB_STEP } else { ISO_STEP }));
    s.push_str(&crlf(BUILD_TAIL));
    s
}

/// Состояние сборки по файлу прогресса build.ps1 (протокол — у BUILD_HEAD).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct BuildProgress {
    /// Журнал этого запуска (build-<дата>.log).
    pub log: Option<String>,
    /// Текущий шаг, с 1; 0 — ещё не начался.
    pub step: u32,
    pub steps: u32,
    /// Проценты текущего шага; None — шаг без процентов.
    pub pct: Option<u8>,
    pub warnings: Vec<String>,
    pub done: Option<String>,
    pub error: Option<String>,
}

impl BuildProgress {
    pub fn parse(text: &str) -> Self {
        let mut p = BuildProgress::default();
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "LOG" => p.log = Some(rest.to_owned()),
                "STEP" => {
                    let mut it = rest.split_whitespace().filter_map(|n| n.parse::<u32>().ok());
                    if let (Some(n), Some(total)) = (it.next(), it.next()) {
                        p.step = n;
                        p.steps = total;
                        p.pct = Some(0);
                    }
                }
                "PCT" => {
                    p.pct = match rest.trim().parse::<i32>() {
                        Ok(v) if v >= 0 => Some(v.min(100) as u8),
                        _ => None,
                    }
                }
                "WARN" => p.warnings.push(rest.to_owned()),
                "DONE" => p.done = Some(rest.to_owned()),
                "ERROR" => p.error = Some(rest.to_owned()),
                _ => {}
            }
        }
        p
    }

    /// Общая доля выполнения 0..1 (шаги равного веса).
    pub fn fraction(&self) -> f32 {
        if self.done.is_some() {
            return 1.0;
        }
        if self.steps == 0 || self.step == 0 {
            return 0.0;
        }
        let inside = f32::from(self.pct.unwrap_or(0)) / 100.0;
        ((self.step - 1) as f32 + inside) / self.steps as f32
    }

    pub fn finished(&self) -> bool {
        self.done.is_some() || self.error.is_some()
    }
}

fn sanitize_file(name: &str) -> String {
    let base: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{base}.exe")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg_action(path: &str, name: &str) -> Action {
        Action {
            reg: Some(path.into()),
            name: Some(name.into()),
            ..Default::default()
        }
    }

    fn sample_unattend(files: Vec<(String, String)>) -> Unattend {
        Unattend {
            ui_language: "uk-UA".into(),
            input_locales: "0422:00000422;0409:00000409".into(),
            timezone: "FLE Standard Time".into(),
            username: "User".into(),
            password: "p&1".into(),
            computer_name: "PC".into(),
            local_account: true,
            auto_login: true,
            bypass_tpm: true,
            no_network: true,
            files,
        }
    }

    fn sample_build(usb: bool, drivers: &str) -> String {
        sample_build_export(usb, drivers, false)
    }

    fn sample_build_export(usb: bool, drivers: &str, export_drivers: bool) -> String {
        build_script(&BuildInput {
            source_iso: r"D:\Win11.iso",
            output_iso: if usb { "" } else { r"D:\out\my.iso" },
            edition: "Windows 11 Pro",
            drivers,
            export_drivers,
            work_dir: r"D:\WinForge",
            cleanup: true,
            installers: &[],
            usb: usb.then_some(UsbTarget {
                number: 3,
                serial: " AA12'BB ",
                size: 31_914_983_424,
            }),
        })
    }

    #[test]
    fn xml_has_sections() {
        let xml = sample_unattend(Vec::new()).xml();
        assert!(xml.starts_with("<?xml"));
        assert!(xml.contains("<ComputerName>PC</ComputerName>"));
        assert!(xml.contains("FLE Standard Time"));
        assert!(xml.contains("BypassTPMCheck"));
        assert!(xml.contains("<LocalAccount"));
        assert!(xml.contains("p&amp;1"), "пароль экранирован");
        assert!(xml.contains("first-logon.cmd"));
        assert!(xml.contains("setup-apps.ps1"));
        assert!(!xml.contains("<Extensions"), "без вшитых файлов блока нет");
        assert!(!xml.contains("Microsoft-Windows-Deployment"));
        // Каждый открытый settings закрыт.
        assert_eq!(xml.matches("<settings").count(), xml.matches("</settings>").count());
        assert_eq!(xml.matches("<component").count(), xml.matches("</component>").count());
    }

    #[test]
    fn xml_without_bypass_has_no_setup_component() {
        let u = Unattend {
            ui_language: "en-US".into(),
            ..Default::default()
        };
        assert!(!u.xml().contains("Microsoft-Windows-Setup\""));
    }

    #[test]
    fn xml_embeds_files_and_runs_specialize() {
        let xml = sample_unattend(vec![
            (format!("{SETUP_DIR}\\specialize.ps1"), "Log \"a<b & c>d\"\r\n$x = 'y'".into()),
            (format!("{SETUP_DIR}\\first-logon.cmd"), "@echo off\r\nreg add \"HKCU\\X\" /f".into()),
        ])
        .xml();
        // Свой блок — в чужом пространстве имён, после всех проходов.
        let ext = xml.find("<Extensions xmlns=\"urn:winforge:unattend\">").unwrap();
        assert!(ext > xml.rfind("</settings>").unwrap());
        assert!(xml.contains(r#"<File path="C:\WinForge\specialize.ps1">Log &quot;a&lt;b &amp; c&gt;d&quot;"#));
        assert!(xml.contains(r#"<File path="C:\WinForge\first-logon.cmd">@echo off"#));
        assert!(xml.contains("<ExtractScript>param([xml]$Document)"));
        // specialize: сначала распаковка, потом сам скрипт.
        let spec = xml.find("<settings pass=\"specialize\">").unwrap();
        let deploy = xml.find("Microsoft-Windows-Deployment").unwrap();
        let oobe = xml.find("<settings pass=\"oobeSystem\">").unwrap();
        assert!(spec < deploy && deploy < oobe);
        let extract = xml.find(r"$x.Load('C:\Windows\Panther\unattend.xml')").unwrap();
        let run = xml
            .find(r#"-ExecutionPolicy Bypass -File &quot;C:\WinForge\specialize.ps1&quot;"#)
            .unwrap();
        assert!(extract < run);
        assert_eq!(xml.matches("<component").count(), xml.matches("</component>").count());
        assert_eq!(xml.matches("<File ").count(), 2);
    }

    #[test]
    fn extract_command_fits_setup_limit() {
        // RunSynchronousCommand/Path: не длиннее 259 символов.
        let c = extract_command(PANTHER_UNATTEND);
        assert!(c.len() <= 259, "{} символов", c.len());
        assert!(!c.contains('<') && !c.contains('&'));
    }

    #[test]
    fn setup_mapping() {
        let mut a = reg_action(r"HKLM\SOFTWARE\Policies\X", "Y");
        a.dword = Some(1);
        assert_eq!(
            setup_reg_line(&a).unwrap(),
            r"reg add 'HKLM\SOFTWARE\Policies\X' /v 'Y' /t REG_DWORD /d '1' /f"
        );

        let mut a = reg_action(r"HKCU\Software\Z", "W");
        a.sz = Some("it's".into());
        assert_eq!(
            setup_reg_line(&a).unwrap(),
            r"reg add 'HKU\WF_DEFAULT\Software\Z' /v 'W' /t REG_SZ /d 'it''s' /f"
        );

        let mut a = reg_action(r"HKLM\SYSTEM\CurrentControlSet\Control\X", "V");
        a.dword = Some(0);
        assert!(setup_reg_line(&a).unwrap().contains(r"'HKLM\SYSTEM\CurrentControlSet\Control\X'"));

        let mut a = reg_action(r"HKCR\Directory\shell\X", "");
        a.sz = Some("y".into());
        assert!(setup_reg_line(&a).unwrap().contains(r"'HKLM\SOFTWARE\Classes\Directory\shell\X'"));
    }

    #[test]
    fn empty_string_has_no_data_flag() {
        let mut a = reg_action(r"HKLM\SOFTWARE\X", "");
        a.sz = Some(String::new());
        assert_eq!(setup_reg_line(&a).unwrap(), r"reg add 'HKLM\SOFTWARE\X' /ve /t REG_SZ /f");
    }

    #[test]
    fn user_classes_go_to_first_logon() {
        let mut a = reg_action(r"HKCU\Software\Classes\CLSID\{x}\InprocServer32", "");
        a.sz = Some(String::new());
        assert!(setup_reg_line(&a).is_none());
        assert_eq!(
            first_logon_line(&a).unwrap(),
            r#"reg add "HKCU\Software\Classes\CLSID\{x}\InprocServer32" /ve /t REG_SZ /f"#
        );
    }

    #[test]
    fn batch_escapes_percent() {
        let mut a = reg_action(r"HKCU\Software\Classes\X\command", "");
        a.expand = Some(r"%SystemRoot%\a.exe".into());
        assert!(first_logon_line(&a).unwrap().contains(r#""%%SystemRoot%%\a.exe""#));
    }

    #[test]
    fn service_is_guarded() {
        let a = Action {
            service: Some("DiagTrack".into()),
            start: Some("disabled".into()),
            ..Default::default()
        };
        let line = setup_reg_line(&a).unwrap();
        assert!(line.starts_with(
            "if (Test-Path 'Registry::HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Services\\DiagTrack')"
        ));
        assert!(line.contains("/v Start /t REG_DWORD /d 4 /f"));
    }

    #[test]
    fn feature_runs_online_and_logs() {
        let line = setup_feature_line(&Action {
            feature: Some("SMB1Protocol".into()),
            enabled: Some(false),
            ..Default::default()
        })
        .unwrap();
        assert!(line.starts_with("& dism.exe /English /Online /Disable-Feature '/FeatureName:SMB1Protocol' /NoRestart"));
        assert!(line.contains("-in 0, 3010"), "3010 = нужна перезагрузка, это успех");
        assert!(line.contains("Log '   feature SMB1Protocol: disabled'"));
    }

    #[test]
    fn app_setup_resumes_after_an_interrupted_first_sign_in() {
        let s = setup_apps_script(
            &[WingetInstall { id: "VideoLAN.VLC".into(), dir: None }],
            &[CustomInstall { name: "Tool".into(), value: r"C:\WinForge\Installers\t.msi".into(), args: "/qn".into() }],
            true,
        );
        // Задача входа создаётся в начале и удаляется только в самом конце.
        let create = s.find("/SC ONLOGON").unwrap();
        let delete = s.find("schtasks.exe /Delete /TN $taskName").unwrap();
        assert!(create < s.find("Install-App 'VideoLAN.VLC'").unwrap());
        assert!(delete > s.find("Install-Custom 'Tool'").unwrap());
        assert!(delete < s.find("app setup finished").unwrap());
        // Готовое не повторяется; «уже установлено» — не ошибка.
        assert!(s.contains("if (Test-Done \"winget:$id\") { return }"));
        assert!(s.contains("if (Test-Done \"installer:$name\") { return }"));
        assert!(s.contains("-1978335189") && s.contains("--no-upgrade"));
        // Повреждённый индекс winget (ПК выключили во время загрузки) — обновить и повторить.
        assert!(s.contains("if ($code -eq -1978335169)") && s.contains("winget source update"));
    }

    #[test]
    fn wifi_profile_is_valid_and_escaped() {
        let x = wifi_profile_xml("Home & <5G>", "p\"ss<");
        assert!(x.contains("<name>Home &amp; &lt;5G&gt;</name>"));
        assert!(x.contains("<keyMaterial>p&quot;ss&lt;</keyMaterial>"));
        assert!(x.contains("<authentication>WPA2PSK</authentication>"));
        assert!(x.contains("<connectionMode>auto</connectionMode>"));
        let open = wifi_profile_xml("Cafe", "");
        assert!(open.contains("<authentication>open</authentication>"));
        assert!(!open.contains("keyMaterial"));
    }

    #[test]
    fn specialize_options_add_their_steps() {
        let plain = specialize_script_with(&[], &[], &[], &SpecializeOptions::default());
        assert!(!plain.contains("netsh.exe wlan") && !plain.contains("CompactOS") && !plain.contains("Guest additions"));
        let s = specialize_script_with(
            &[],
            &[],
            &[],
            &SpecializeOptions { wifi: true, compact_os: true, guest_tools: true, ..Default::default() },
        );
        assert!(s.contains(r"$wifi = 'C:\WinForge\wifi.xml'"));
        assert!(s.contains("wlan add profile \"filename=$wifi\" user=all"));
        // Файл с паролем и его копия в ответах установщика стираются.
        assert!(s.contains("Remove-Item -LiteralPath $wifi"));
        // Обе копии файла ответов, и тот же путь, что у профиля.
        assert!(s.contains("unattend-original.xml"));
        assert!(SECRETS_SCRUB.contains(WIFI_PROFILE) && SECRETS_SCRUB.contains(PRODUCT_KEY_FILE));
        assert!(secrets_scrub_logon_line().starts_with("powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand "));
        assert!(s.contains("VBoxWindowsAdditions-amd64.exe"));
        assert!(s.contains("compact.exe /CompactOS:always"));
        assert!(s.find("compact.exe").unwrap() < s.find("specialize finished").unwrap());
    }

    #[test]
    fn product_key_never_lands_in_the_script() {
        let s = specialize_script_with(
            &[],
            &[],
            &[],
            &SpecializeOptions {
                product_key: true,
                kms: Some(("kms.company.local".into(), Some(1688))),
                ..Default::default()
            },
        );
        assert!(s.contains(r"$keyFile = 'C:\WinForge\product-key.txt'"));
        assert!(s.contains("InstallProductKey -Arguments @{ ProductKey = $k }"));
        assert!(s.contains("Remove-Item -LiteralPath $keyFile"));
        assert!(s.contains("MachineName = 'kms.company.local'"));
        assert!(s.contains("PortNumber = [uint32]1688"));
        // Ключ и его узел в копиях файла ответов стираются.
        assert!(s.contains("unattend-original.xml"));
        let none = specialize_script_with(&[], &[], &[], &SpecializeOptions::default());
        assert!(!none.contains("SoftwareLicensingService") && !none.contains("unattend-original.xml"));
    }

    #[test]
    fn enabling_a_feature_uses_media_sources() {
        let line = setup_feature_line(&Action {
            feature: Some("NetFx3".into()),
            enabled: Some(true),
            ..Default::default()
        })
        .unwrap();
        assert!(line.starts_with("& dism.exe /English /Online /Enable-Feature '/FeatureName:NetFx3' /All @sxs /NoRestart"));
        let s = specialize_script(&[], &[], &[line], false);
        let sxs = s.find("$sxs = @(").unwrap();
        assert!(sxs < s.find("/FeatureName:NetFx3").unwrap(), "$sxs задаётся до использования");
    }

    #[test]
    fn capability_removed_by_name_prefix() {
        let line = setup_feature_line(&Action {
            capability: Some("Microsoft.Windows.WordPad".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(line, "Remove-Capability 'Microsoft.Windows.WordPad'");
        let s = specialize_script(&[], &[], &[line.clone()], false);
        assert!(s.find("function Remove-Capability").unwrap() < s.find(&line).unwrap());
        // Только эта возможность и её языки/версии, а не всё, что начинается так же.
        assert!(s.contains("-like \"$name~*\""));
    }

    #[test]
    fn specialize_script_order() {
        let s = specialize_script(
            &["Microsoft.BingNews".into()],
            &[r"reg add 'HKU\WF_DEFAULT\Software\Z' /v 'W' /t REG_DWORD /d '1' /f".into()],
            &["# feature".into()],
            false,
        );
        assert!(s.contains("$appx = @('Microsoft.BingNews')"));
        assert!(s.contains("Get-AppxProvisionedPackage -Online"));
        assert!(s.contains("Remove-AppxProvisionedPackage -Online -AllUsers"));
        assert!(s.contains(r"$log = 'C:\WinForge\specialize.log'"));
        // Профиль по умолчанию загружен на время правок и выгружен после.
        let load = s.find(r"reg load 'HKU\WF_DEFAULT' 'C:\Users\Default\NTUSER.DAT'").unwrap();
        let line = s.find(r"reg add 'HKU\WF_DEFAULT\Software\Z'").unwrap();
        let unload = s.find(r"reg unload 'HKU\WF_DEFAULT'").unwrap();
        assert!(load < line && line < unload);
        assert!(s.find("# feature").unwrap() > unload);
        assert!(!s.contains("$mount"), "образ не монтируется");
        // Удаление того, чего и так нет, не считается ошибкой.
        assert!(s.contains("if ($args[0] -eq 'delete')"));
        assert_eq!(s.matches('{').count(), s.matches('}').count());
    }

    #[test]
    fn setup_apps_retries_and_logs_result() {
        let apps = [
            WingetInstall { id: "Telegram.TelegramDesktop".into(), dir: None },
            WingetInstall { id: "7zip.7zip".into(), dir: Some(r"D:\Apps\7-Zip".into()) },
        ];
        let s = setup_apps_script(&apps, &[], true);
        assert!(s.contains("Install-App 'Telegram.TelegramDesktop'\r\n"), "без --location");
        assert!(s.contains(r"Install-App '7zip.7zip' 'D:\Apps\7-Zip'"));
        let first = s.find("--location $dir").unwrap();
        let retry = s.find("retrying in the default folder").unwrap();
        assert!(first < retry, "сначала выбранная папка, потом папка по умолчанию");
        assert!(s.contains("Log '   ok'") && s.contains("FAILED (code $code)"));
        assert_eq!(s.matches('{').count(), s.matches('}').count());
    }

    #[test]
    fn own_installers_travel_on_the_media() {
        let inst = [(r"D:\Soft\setup.exe".to_string(), "setup.exe".to_string())];
        let s = build_script(&BuildInput {
            source_iso: r"D:\Win11.iso",
            output_iso: r"D:\out.iso",
            edition: "Windows 11 Pro",
            drivers: "",
            export_drivers: false,
            work_dir: r"D:\WinForge",
            cleanup: true,
            installers: &inst,
            usb: None,
        });
        assert!(s.contains(r"$installers = @(@{ From = 'D:\Soft\setup.exe'; Name = 'setup.exe' })"));
        assert!(s.contains("Join-Path $media 'WinForge\\Installers'"));
        let copy = s.find("own installers").unwrap();
        assert!(copy < s.find("Step 4 '").unwrap(), "до записи ISO");
        assert_eq!(s.matches('{').count(), s.matches('}').count());
        // Без своих установщиков блока нет.
        assert!(!sample_build(false, "").contains("own installers"));

        // При установке specialize забирает их с носителя, пропуская системный диск.
        let spec = specialize_script(&[], &[], &[], true);
        assert!(spec.contains(r"$dst = 'C:\WinForge\Installers'"));
        assert!(spec.contains("Where-Object { $_.Root -ne \"$env:SystemDrive\\\" }"));
        assert!(!specialize_script(&[], &[], &[], false).contains("Installers"));
    }

    #[test]
    fn own_installers_run_silently_and_log() {
        let s = setup_apps_script(
            &[],
            &[
                CustomInstall { name: "A".into(), value: r"C:\WinForge\Installers\a.exe".into(), args: "/S".into() },
                CustomInstall { name: "B".into(), value: r"C:\WinForge\Installers\b.msi".into(), args: "/qn".into() },
                CustomInstall { name: "My Tool".into(), value: "https://x.org/t.msi?v=2".into(), args: String::new() },
            ],
            true,
        );
        assert!(s.contains(r"Install-Custom 'A' 'C:\WinForge\Installers\a.exe' '/S' ''"));
        assert!(s.contains(r"Install-Custom 'B' 'C:\WinForge\Installers\b.msi' '/qn' ''"));
        assert!(s.contains("Install-Custom 'My Tool' 'My_Tool.msi' '' 'https://x.org/t.msi?v=2'"), "ссылка: файл с верным расширением");
        assert!(s.contains("Start-Process msiexec.exe -ArgumentList \"/i `\"$file`\" $params\""));
        assert!(s.contains("-in 0, 3010, 1641"));
        assert!(s.contains("Find-Installers"), "носитель ищется ещё раз, если specialize не скопировал");
        assert!(s.contains("Remove-Item -LiteralPath $installers -Recurse"));
        assert!(!setup_apps_script(&[], &[], true).contains("Install-Custom"));
        assert_eq!(s.matches('{').count(), s.matches('}').count());
    }

    #[test]
    fn each_app_gets_its_own_folder() {
        // Папка по умолчанию: --location не нужен, установщики сами делают подпапки.
        assert_eq!(app_install_dir(r"C:\Program Files", "Firefox"), None);
        assert_eq!(app_install_dir(r"c:\program files\", "Firefox"), None);
        assert_eq!(app_install_dir("", "Firefox"), None);
        // Своя папка: каждой программе — подпапка, а не общий корень.
        assert_eq!(app_install_dir(r"D:\Apps\", "7-Zip").as_deref(), Some(r"D:\Apps\7-Zip"));
        assert_eq!(app_install_dir(r"D:\Apps", "A/B: C?").as_deref(), Some(r"D:\Apps\AB C"));
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn build_script_has_all_stages() {
        let s = sample_build(false, "");
        // Занятый готовый ISO (подключён к виртуалке) — понятная ошибка до первого шага.
        assert!(s.find("is in use by another program").unwrap() < s.find("Step 1 ").unwrap());
        // Отмена убирает недокопированную рабочую папку (только свою WinForge-work).
        let trap = s.find("trap {").unwrap();
        let removal = s[trap..].find("Remove-Item $work -Recurse").map(|i| i + trap).unwrap();
        assert!(s[trap..removal].contains("(Split-Path $work -Leaf) -eq 'WinForge-work'"));
        // trap читает $work и $cleanup в момент ошибки: они должны быть заданы до первого шага.
        let step1 = s.find("Step 1 ").unwrap();
        assert!(s.find("$work    = ").unwrap() < step1 && s.find("$cleanup = ").unwrap() < step1);
        // ISO без ADK: встроенный IMAPI2 с UDF 1.02.
        assert!(s.contains("$target    = 'iso'"));
        assert!(s.contains("New-BootIso -Source $media -Output $output"));
        assert!(s.contains("$image.UDFRevision = 0x102"));
        // Окно сборки остаётся открытым: IMAPI2 обязан отпустить файлы media.
        assert!(s.contains("FinalReleaseComObject"));
        assert!(s.contains("if (Test-Path $work) { Warn "), "об удалении — только если папки правда нет");
        assert!(!s.contains("Clear-Disk"), "в режиме ISO диски не трогаются");
        // here-string C# закрывается с начала строки, иначе PowerShell его не поймёт.
        assert!(s.contains("\r\n'@\r\n"));
        // PowerShell 5 + Stop: «2>$null» у внешней программы = аварийная остановка.
        assert!(!s.contains("2>$null"), "stderr внешних программ перенаправляется только внутри cmd /c");
        // Образ не монтируется и не правится: на части ПК кусты образа не грузятся.
        for gone in ["Mount-WindowsImage", "reg load", "/Image:", "Remove-ProvisionedAppxPackage", "Add-WindowsDriver"] {
            assert!(!s.contains(gone), "{gone} больше не нужен");
        }
        assert!(s.contains("Copy-Item (Join-Path $here 'autounattend.xml') $media"));
        assert!(s.contains("Move-Item $newWim $wim -Force"));
        // Журнал свой на каждый запуск, не обязателен и закрывается при ошибке.
        assert!(!s.contains("'build.log'"), "общий build.log мог держать открытым прошлый запуск");
        assert!(s.contains("try { Start-Transcript -Path $logFile"));
        assert!(s.contains("Report \"ERROR $($e.Message)\""), "причина остановки — в WinForge");
        assert!(s.contains("Report \"DONE $done\""));
        // Запуск из WinForge: файл прогресса, отмена, подтверждение для флешки.
        assert!(s.contains("param([string]$ProgressFile = '', [string]$CancelFile = '', [switch]$Confirmed)"));
        assert!(s.contains("[WfCopy]::Tree($drive, $media"), "копия носителя — с процентами");
        assert!(s.contains("WfProgress.Pct(pct)"));
        // Остатки прошлых сборок убираются только внутри $work и до копирования.
        assert!(s.contains("Test-InWork $_.Path"));
        assert!(s.find("unmounting a leftover image").unwrap() < s.find("Step 1 'Copying").unwrap());
        for stage in 1..=4 {
            assert!(s.contains(&format!("Step {stage} '")), "нет шага {stage}");
        }
        assert!(s.contains("$edition = 'Windows 11 Pro'"));
        assert!(!s.contains("$WinPEDriver$"), "без папки драйверов шаг не нужен");
        // Работает только в своей подпапке, а не в выбранной пользователем папке целиком.
        assert!(s.contains(r"$work    = Join-Path 'D:\WinForge' 'WinForge-work'"));
        assert!(!s.contains("LOCALAPPDATA"), "рабочие файлы не должны идти на C: по умолчанию");
        assert!(s.contains("$cleanup = $true"));
        assert!(s.contains("$needGB  = 30"));
        // Скобки сбалансированы.
        assert_eq!(s.matches('{').count(), s.matches('}').count());
    }

    #[test]
    fn build_progress_follows_the_script() {
        let p = BuildProgress::parse("\u{feff}LOG C:\\b\\build-1.log\r\nSTEP 1 4\r\nPCT 0\r\nPCT 50\r\n");
        assert_eq!(p.log.as_deref(), Some(r"C:\b\build-1.log"));
        assert_eq!((p.step, p.steps, p.pct), (1, 4, Some(50)));
        assert!((p.fraction() - 0.125).abs() < 1e-6);
        assert!(!p.finished());

        let p = BuildProgress::parse("STEP 1 4\nPCT 100\nSTEP 2 4\nPCT -1\nWARN   drivers folder not found\n");
        assert_eq!((p.step, p.pct), (2, None), "шаг без процентов");
        assert!((p.fraction() - 0.25).abs() < 1e-6);
        assert_eq!(p.warnings, vec!["  drivers folder not found".to_string()]);

        let p = BuildProgress::parse("STEP 4 4\nPCT 99\nDONE D:\\WinForge\\x.iso\n");
        assert_eq!(p.done.as_deref(), Some(r"D:\WinForge\x.iso"));
        assert_eq!(p.fraction(), 1.0);
        assert!(p.finished());

        let p = BuildProgress::parse("STEP 2 4\nERROR Cancelled in WinForge.\n");
        assert_eq!(p.error.as_deref(), Some("Cancelled in WinForge."));
        assert!(p.finished());
        assert_eq!(BuildProgress::parse("").fraction(), 0.0);
    }

    #[test]
    fn this_pc_drivers_are_exported_before_writing() {
        assert!(!sample_build(false, "").contains("Export-WindowsDriver"), "без галочки не выгружаем");
        let s = sample_build_export(false, r"D:\Drivers", true);
        let export = s.find("Export-WindowsDriver -Online").unwrap();
        assert!(export > s.find("Step 3 ").unwrap() && export < s.find("Step 4 ").unwrap());
        assert!(s.contains("cmd.exe /c \"pnputil.exe /export-driver *"), "запасной путь, stderr — через cmd");
        // В $WinPEDriver$ — только контроллеры дисков, остальное — в WinForge\Drivers.
        assert!(s.contains(r"$pe = Join-Path $media '$WinPEDriver$\ThisPC-storage'"));
        assert!(s.contains(r"$rest = Join-Path $media 'WinForge\Drivers'"));
        assert!(s.contains("@('SCSIAdapter', 'HDC')"));
        assert!(!s.contains(r"'$WinPEDriver$\ThisPC')"), "все драйверы в $WinPEDriver$ ломают установку (0x80070070)");
        // Не вышло — предупреждение, сборка продолжается.
        assert!(s.contains("Warn '   drivers of this PC were not exported"));
        assert!(s.contains("$exp = Join-Path $work 'this-pc-drivers'"), "только внутри рабочей папки");
    }

    #[test]
    fn usb_split_reports_percent_and_can_be_cancelled() {
        let s = sample_build(true, "");
        // Копирование — первые 40 % шага, нарезка — дальше: полоска не откатывается назад.
        let copy = s.find("[WfProgress]::From = 0; [WfProgress]::To = 40").unwrap();
        let split = s.find("'/Split-Image'").unwrap();
        assert!(copy < split);
        let tail = &s[split..];
        assert!(tail.contains("Report ('PCT {0}' -f (40 +"), "проценты по размеру частей");
        // Отмена останавливает процесс dism сразу и честно говорит про флешку.
        assert!(tail.contains("$dism.Kill()") && tail.contains("now incomplete"));
        assert!(tail.contains("$dism.ExitCode -ne 0"));
        assert!(tail.contains("Report 'PCT 100'"));
        assert!(!s.contains("Split-WindowsImage -") && !s.contains("AddCommand('Split-WindowsImage')"), "не прерывается — не используем");
    }

    #[test]
    fn this_pc_drivers_install_first_in_specialize() {
        let s = specialize_script_with(
            &[],
            &[],
            &[],
            &SpecializeOptions { this_pc_drivers: true, wifi: true, copy_installers: true, ..Default::default() },
        );
        let drivers = s.find("pnputil.exe /add-driver (Join-Path $drv '*.inf') /subdirs /install").unwrap();
        assert!(drivers < s.find("Copying own installers").unwrap());
        assert!(drivers < s.find("netsh").unwrap(), "до Wi-Fi: нужен драйвер адаптера");
        assert!(s.contains("Join-Path $_.Root 'WinForge\\Drivers'"));
        assert!(!specialize_script(&[], &[], &[], false).contains("pnputil"), "без галочки не нужно");
    }

    #[test]
    fn drivers_go_to_winpedriver_folder() {
        let s = sample_build(false, r" D:\Drivers ");
        assert!(s.contains(r"$drivers = 'D:\Drivers'"));
        assert!(s.contains("robocopy $drivers (Join-Path $media '$WinPEDriver$')"));
        assert_eq!(s.matches('{').count(), s.matches('}').count());
    }

    #[test]
    fn usb_build_checks_disk_before_erasing() {
        let s = sample_build(true, "");
        assert!(s.contains("$target    = 'usb'"));
        assert!(s.contains("$usbNumber = 3"));
        assert!(s.contains("$usbSerial = 'AA12''BB'"), "серийный номер экранирован");
        assert!(s.contains("$usbSize   = 31914983424"));
        assert!(s.contains("$disk.BusType -ne 'USB'"));
        assert!(s.contains("Split-WindowsImage"));
        assert!(s.contains("bootsect.exe"));
        assert!(!s.contains("New-BootIso -Source $media"), "ISO не собирается");
        // Подтверждение и проверки — раньше любой стирающей команды.
        assert!(s.contains("if (-not $Confirmed) {"), "YES в окне — только без подтверждения в WinForge");
        assert!(s.contains("[WfCopy]::Tree($media, \"$usb\\\", [string[]]@('install.wim'))"));
        let confirm = s.find("Type YES").unwrap();
        let checks = s.find("$disk.BusType -ne 'USB'").unwrap();
        for destructive in ["Clear-Disk", "Initialize-Disk", "New-Partition", "Format-Volume"] {
            let at = s.find(destructive).unwrap();
            assert!(checks < at && confirm < at, "{destructive} до проверок/подтверждения");
        }
    }

    /// Проценты нарезки считаются в настоящем PowerShell 5.1 на install.wim больше 2 ГБ
    /// (раньше [math]::Max(1, размер) падал: «Value was either too large or too small for an Int32»).
    /// Запуск: cargo test -- --ignored
    #[test]
    #[ignore = "запускает powershell.exe"]
    fn usb_split_percent_works_for_wim_over_2gb() {
        let s = sample_build(true, "");
        let line = |start: &str| s.lines().find(|l| l.trim_start().starts_with(start)).unwrap().trim().to_owned();
        let size = line("$wimSize =");
        let pct = line("Report ('PCT {0}'");
        let dir = std::env::temp_dir().join(format!("wf-wim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Файл на 3 ГБ без записи данных: SetLength только меняет длину.
        let wim = dir.join("install.wim");
        std::fs::File::create(&wim).unwrap().set_len(3 * 1024 * 1024 * 1024).unwrap();
        let script = format!(
            "$ErrorActionPreference = 'Stop'\r\nfunction Report($l) {{ $l }}\r\n$wimPath = '{}'\r\n{size}\r\n\
             foreach ($written in @($null, [double]0, [double]3221225472, [double]9e9)) {{ {pct} }}\r\n",
            wim.display()
        );
        let ps1 = dir.join("t.ps1");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(script.as_bytes());
        std::fs::write(&ps1, bytes).unwrap();
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&ps1)
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(out.status.success(), "{text}\n{}", String::from_utf8_lossy(&out.stderr));
        let got: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        assert_eq!(got, ["PCT 40", "PCT 40", "PCT 99", "PCT 99"]);
    }

    fn ps_parse_errors(name: &str, script: &str) -> String {
        let path = std::env::temp_dir().join(format!("wf-{name}-{}.ps1", std::process::id()));
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(script.as_bytes());
        std::fs::write(&path, bytes).unwrap();
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "$e = $null; [void][System.Management.Automation.Language.Parser]::ParseFile('{}', [ref]$null, [ref]$e); $e | ForEach-Object {{ $_.Message }}; \"COUNT=$($e.Count)\"",
                    path.display()
                ),
            ])
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&path);
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Все сгенерированные скрипты разбираются настоящим парсером PowerShell.
    /// Запуск: cargo test -- --ignored
    #[test]
    #[ignore = "запускает powershell.exe"]
    fn generated_scripts_parse_in_powershell() {
        for usb in [false, true] {
            let text = ps_parse_errors(&format!("build-{usb}"), &sample_build_export(usb, r"D:\Drivers", true));
            assert!(text.contains("COUNT=0"), "build usb={usb}: {text}");
        }
        let mut svc = Action {
            service: Some("DiagTrack".into()),
            start: Some("disabled".into()),
            ..Default::default()
        };
        let mut reg_lines = vec![setup_reg_line(&svc).unwrap()];
        svc = reg_action(r"HKCU\Software\Z", "W");
        svc.sz = Some("it's".into());
        reg_lines.push(setup_reg_line(&svc).unwrap());
        let feat = vec![
            setup_feature_line(&Action {
                feature: Some("SMB1Protocol".into()),
                enabled: Some(false),
                ..Default::default()
            })
            .unwrap(),
            setup_feature_line(&Action {
                feature: Some("NetFx3".into()),
                enabled: Some(true),
                ..Default::default()
            })
            .unwrap(),
            setup_feature_line(&Action {
                capability: Some("Microsoft.Windows.WordPad".into()),
                ..Default::default()
            })
            .unwrap(),
        ];
        let spec = specialize_script_with(
            &["Microsoft.BingNews".into()],
            &reg_lines,
            &feat,
            &SpecializeOptions {
                copy_installers: true,
                wifi: true,
                compact_os: true,
                guest_tools: true,
                product_key: true,
                kms: Some(("kms.company.local".into(), Some(1688))),
                this_pc_drivers: true,
            },
        );
        let text = ps_parse_errors("specialize", &spec);
        assert!(text.contains("COUNT=0"), "specialize: {text}");
        let apps = setup_apps_script(
            &[
                WingetInstall { id: "7zip.7zip".into(), dir: Some(r"D:\Apps\7-Zip".into()) },
                WingetInstall { id: "VideoLAN.VLC".into(), dir: None },
            ],
            &[
                CustomInstall { name: "My App".into(), value: r"C:\WinForge\Installers\a.exe".into(), args: "/S".into() },
                CustomInstall { name: "Tool".into(), value: "https://example.com/tool.msi?x=1".into(), args: "/qn".into() },
            ],
            true,
        );
        let text = ps_parse_errors("apps", &apps);
        assert!(text.contains("COUNT=0"), "setup-apps: {text}");
        let text = ps_parse_errors("extract", EXTRACT_SCRIPT);
        assert!(text.contains("COUNT=0"), "extract: {text}");
        let dns = crate::sys::probe::set_dns_script(
            &["1.1.1.1".into(), "1.0.0.1".into()],
            Some("https://cloudflare-dns.com/dns-query"),
        )
        .unwrap();
        let text = ps_parse_errors("dns", &dns);
        assert!(text.contains("COUNT=0"), "dns: {text}");
        use crate::sys::activation::{activate_script, status_script};
        for (name, script) in [
            ("activation-status", status_script()),
            (
                "activation",
                activate_script(Some("ABCDE-12345-FGHIJ-67890-KLMNO"), Some(("kms.company.local", Some(1688)))),
            ),
        ] {
            let text = ps_parse_errors(name, &script);
            assert!(text.contains("COUNT=0"), "{name}: {text}");
        }
    }

    /// Вшитые файлы распаковываются той же командой, что запускает установщик.
    /// Запуск: cargo test -- --ignored
    #[test]
    #[ignore = "запускает powershell.exe"]
    fn embedded_files_extract_like_setup() {
        use std::os::windows::process::CommandExt;
        let dir = std::env::temp_dir().join(format!("wf-extract-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ps1 = "# тест\r\nWrite-Host \"a<b & c>d\" '%PATH%' `$x\r\n$y = 1";
        let cmd = "@echo off\r\nreg add \"HKCU\\X\" /v A /d \"%%TEMP%%\" /f";
        let xml = sample_unattend(vec![
            (format!("{}\\s.ps1", dir.display()), ps1.into()),
            (format!("{}\\sub\\f.cmd", dir.display()), cmd.into()),
        ])
        .xml();
        std::fs::create_dir_all(&dir).unwrap();
        let answer = dir.join("unattend.xml");
        std::fs::write(&answer, &xml).unwrap();
        let full = extract_command(&answer.display().to_string());
        let args = full.strip_prefix("powershell.exe ").unwrap();
        let out = std::process::Command::new("powershell.exe").raw_arg(args).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let got = std::fs::read(dir.join("s.ps1")).unwrap();
        let mut want = vec![0xEF, 0xBB, 0xBF];
        want.extend_from_slice(format!("{ps1}\r\n").as_bytes());
        assert_eq!(got, want, "ps1: UTF-8 с BOM, CRLF");
        let got = std::fs::read(dir.join("sub").join("f.cmd")).unwrap();
        assert_eq!(got, format!("{cmd}\r\n").into_bytes(), "cmd: без BOM, CRLF");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn task_goes_to_first_logon() {
        let a = Action {
            task: Some(r"\Microsoft\Windows\X\Y".into()),
            enabled: Some(false),
            ..Default::default()
        };
        assert_eq!(
            first_logon_line(&a).unwrap(),
            r#"schtasks /Change /TN "\Microsoft\Windows\X\Y" /Disable >nul 2>&1"#
        );
    }

    #[test]
    fn powershell_run_is_encoded() {
        let a = Action {
            run: Some("Write-Host 'hi'".into()),
            shell: Some("powershell".into()),
            ..Default::default()
        };
        let line = first_logon_line(&a).unwrap();
        assert!(line.starts_with("powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand "));
        assert!(!line.contains('\''));
    }
}

/// Скрипт установки программ для ручной проверки в виртуалке:
/// WF_DUMP_SETUP=<файл> cargo test dump_setup_apps -- --ignored
#[cfg(test)]
mod dump {
    use super::*;

    #[test]
    #[ignore = "пишет файл для ручной проверки"]
    fn dump_setup_apps() {
        let Ok(path) = std::env::var("WF_DUMP_SETUP") else { return };
        let apps: Vec<WingetInstall> = [
            "Discord.Discord", "Telegram.TelegramDesktop", "Mozilla.Firefox", "VideoLAN.VLC", "7zip.7zip",
            "voidtools.Everything", "SumatraPDF.SumatraPDF", "Microsoft.VCRedist.2015+.x64", "Microsoft.DotNet.DesktopRuntime.8",
        ]
        .iter()
        .map(|id| WingetInstall { id: (*id).into(), dir: None })
        .collect();
        let custom = [CustomInstall {
            name: "7-Zip MSI test".into(),
            value: r"C:\WinForge\Installers\7z-x64.msi".into(),
            args: "/qn /norestart".into(),
        }];
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(setup_apps_script(&apps, &custom, true).as_bytes());
        std::fs::write(path, bytes).unwrap();
    }
}
