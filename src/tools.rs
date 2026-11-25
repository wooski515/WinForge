//! Данные и состояние вкладок-инструментов (очистка, автозапуск, сеть, hosts,
//! контекстное меню, монитор дисков, PATH) и настройки установки программ.

use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::Receiver;

use serde::{Deserialize, Serialize};

use crate::app::Pair;
use crate::sys::probe;

// ---------------------------------------------------------------------------
// Состояние
// ---------------------------------------------------------------------------

/// Пинг в процессе: результаты приходят из фонового потока по одному.
pub struct PingRun {
    pub host: String,
    /// Время ответа в мс или None (нет ответа).
    pub results: Vec<Option<u32>>,
    pub rx: Receiver<Option<u32>>,
}

pub struct ToolsState {
    // Очистка
    /// Результат анализа: (id пункта, объём в МБ).
    pub analysis: Option<Vec<(String, u64)>>,
    /// Фоновый расчёт реальных размеров очистки.
    pub analyze_rx: Option<Receiver<Vec<(String, u64)>>>,
    // Реестр
    pub restart_explorer: bool,
    /// Создавать точку восстановления перед применением изменений.
    pub restore_point: bool,
    // Сеть
    pub dns_choice: usize,
    pub custom_dns: [String; 2],
    /// Шифровать DNS (DNS-over-HTTPS), если провайдер это умеет.
    pub dns_doh: bool,
    // Активация (ключ нигде не сохраняется)
    pub activation_key: String,
    pub kms_host: String,
    pub activation_status: Option<Result<Option<crate::sys::activation::Status>, String>>,
    pub activation_rx: Option<Receiver<Result<Option<crate::sys::activation::Status>, String>>>,
    /// Идёт активация: (успех, сообщение) по завершении.
    pub activation_job: Option<Receiver<(bool, String)>>,
    pub dns_current: Option<Vec<String>>,
    pub dns_rx: Option<Receiver<Vec<String>>>,
    pub ping_host: String,
    pub ping: Option<PingRun>,
    // hosts
    pub hosts_text: String,
    pub hosts_domain: String,
    pub hosts_ip: String,
    pub hosts_www: bool,
    pub hosts_loaded: bool,
    // Контекстное меню
    pub run_commands: Vec<(String, String)>,
    pub new_alias: String,
    pub new_target: String,
    // Монитор дисков: история чтения/записи в МБ/с
    pub disk_history: VecDeque<(f32, f32)>,
    pub disk_last_sample: f64,
    pub disk_paused: bool,
    /// Живой сбор данных дисков (создаётся при открытии вкладки).
    pub disk_sampler: Option<probe::DiskSampler>,
    // Автозапуск (загружается при открытии вкладки)
    pub startup_real: Option<Vec<probe::StartupItem>>,
    // Оборудование (загружается в фоне)
    pub hw: Option<Vec<probe::HwSectionData>>,
    pub hw_rx: Option<Receiver<Vec<probe::HwSectionData>>>,
    // Редактор PATH
    pub path_loaded: bool,
    pub path_scope: PathScope,
    pub path_user: Vec<PathEntry>,
    pub path_system: Vec<PathEntry>,
    pub path_user_saved: Vec<PathEntry>,
    pub path_system_saved: Vec<PathEntry>,
    pub path_new: String,
    /// Локальные диски (для выбора, куда ставить программы).
    pub drives: Vec<DriveInfo>,
    // Образ: редакции в выбранном ISO (путь, результат) и фоновое сканирование.
    pub iso_info: Option<(String, Result<Vec<String>, String>)>,
    pub iso_scan: Option<(String, Receiver<Result<Vec<String>, String>>)>,
    // Откат: журналы применений и идущий откат.
    pub journals: Option<Vec<(std::path::PathBuf, crate::sys::Journal)>>,
    /// Идёт откат: по завершении — (итог, ошибки).
    pub rollback_rx: Option<Receiver<(usize, Vec<String>)>>,
    /// Итог последнего отката — показывается на странице, пока её не закроют.
    pub rollback_result: Option<(usize, Vec<String>)>,
    /// Что откатывается прямо сейчас: «12/149 · Компонент …».
    pub rollback_step: std::sync::Arc<std::sync::Mutex<String>>,
    // Флешки для записи сборки (загружаются в фоне).
    pub usb_list: Option<Vec<probe::UsbDisk>>,
    pub usb_rx: Option<Receiver<Vec<probe::UsbDisk>>>,
}

impl Default for ToolsState {
    fn default() -> Self {
        Self {
            analysis: None,
            analyze_rx: None,
            restart_explorer: true,
            restore_point: true,
            dns_doh: true,
            activation_key: String::new(),
            kms_host: String::new(),
            activation_status: None,
            activation_rx: None,
            activation_job: None,
            dns_choice: 0,
            custom_dns: [String::new(), String::new()],
            dns_current: None,
            dns_rx: None,
            ping_host: "1.1.1.1".into(),
            ping: None,
            hosts_text: DEFAULT_HOSTS.into(),
            hosts_domain: String::new(),
            hosts_ip: "0.0.0.0".into(),
            hosts_www: true,
            hosts_loaded: false,
            run_commands: Vec::new(),
            new_alias: String::new(),
            new_target: String::new(),
            disk_history: VecDeque::new(),
            disk_last_sample: 0.0,
            disk_paused: false,
            disk_sampler: None,
            startup_real: None,
            hw: None,
            hw_rx: None,
            path_loaded: false,
            path_scope: PathScope::User,
            path_user: Vec::new(),
            path_system: Vec::new(),
            path_user_saved: Vec::new(),
            path_system_saved: Vec::new(),
            path_new: String::new(),
            drives: list_drives(),
            iso_info: None,
            iso_scan: None,
            journals: None,
            rollback_rx: None,
            rollback_result: None,
            rollback_step: Default::default(),
            usb_list: None,
            usb_rx: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Куда ставить программы и свои программы
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct DriveInfo {
    /// "C:"
    pub letter: String,
    /// Метка тома (может быть пустой).
    pub label: String,
    /// Здесь установлена Windows.
    pub system: bool,
    pub is_hdd: bool,
    pub free_gb: u32,
    pub total_gb: u32,
}

impl DriveInfo {
    /// Папка программ по умолчанию для этого диска.
    pub fn default_folder(&self) -> String {
        if self.system {
            std::env::var("ProgramFiles").unwrap_or_else(|_| format!(r"{}\Program Files", self.letter))
        } else {
            format!(r"{}\Programs", self.letter)
        }
    }

    pub fn download_folder(&self) -> String {
        format!(r"{}\WinForge\Downloads", self.letter)
    }
}

fn system_drive() -> String {
    std::env::var("SystemDrive")
        .unwrap_or_else(|_| "C:".into())
        .to_uppercase()
}

/// Несъёмные локальные диски; системный — первым.
pub fn list_drives() -> Vec<DriveInfo> {
    let sys_drive = system_drive();
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut drives: Vec<DriveInfo> = disks
        .list()
        .iter()
        .filter(|d| !d.is_removable())
        .filter_map(|d| {
            let mount = d.mount_point().to_string_lossy().trim_end_matches('\\').to_uppercase();
            // Только тома с буквой («C:»), без точек монтирования в папках.
            if mount.len() != 2 || !mount.ends_with(':') {
                return None;
            }
            Some(DriveInfo {
                system: mount == sys_drive,
                label: d.name().to_string_lossy().into_owned(),
                is_hdd: matches!(d.kind(), sysinfo::DiskKind::HDD),
                free_gb: (d.available_space() / 1_073_741_824) as u32,
                total_gb: (d.total_space() / 1_073_741_824) as u32,
                letter: mount,
            })
        })
        .collect();
    drives.sort_by(|a, b| b.system.cmp(&a.system).then(a.letter.cmp(&b.letter)));
    if drives.is_empty() {
        drives.push(DriveInfo {
            letter: sys_drive,
            label: String::new(),
            system: true,
            is_hdd: false,
            free_gb: 0,
            total_gb: 0,
        });
    }
    drives
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CustomKind {
    /// Пакет из winget по ID.
    Winget,
    /// Свой установщик: путь к файлу или ссылка + параметры тихой установки.
    Installer,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomApp {
    pub id: String,
    pub name: String,
    pub kind: CustomKind,
    /// ID winget или путь/ссылка на установщик.
    pub value: String,
    /// Параметры тихой установки (для своего установщика).
    #[serde(default)]
    pub args: String,
}

pub struct InstallSettings {
    /// Индекс в `ToolsState::drives`.
    pub drive: usize,
    pub folder: String,
    pub download_folder: String,
    pub delete_installers: bool,
    pub custom: Vec<CustomApp>,
    pub next_custom_id: u32,
    pub form_kind: CustomKind,
    pub form_name: String,
    pub form_value: String,
    pub form_args: String,
}

impl Default for InstallSettings {
    fn default() -> Self {
        let sys = DriveInfo {
            letter: system_drive(),
            label: String::new(),
            system: true,
            is_hdd: false,
            free_gb: 0,
            total_gb: 0,
        };
        Self {
            drive: 0,
            folder: sys.default_folder(),
            download_folder: sys.download_folder(),
            delete_installers: true,
            custom: Vec::new(),
            next_custom_id: 1,
            form_kind: CustomKind::Winget,
            form_name: String::new(),
            form_value: String::new(),
            form_args: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Монитор дисков
// ---------------------------------------------------------------------------

pub const DISK_SAMPLE_INTERVAL: f64 = 0.5;
pub const DISK_HISTORY: usize = 60;

// ---------------------------------------------------------------------------
// Редактор PATH
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PathScope {
    User,
    System,
}

#[derive(Clone, PartialEq)]
pub struct PathEntry {
    pub path: String,
    /// Папки нет на диске.
    pub missing: bool,
    /// Добавлено в этой сессии и ещё не сохранено.
    pub added: bool,
}

/// Путь для сравнения: без регистра и завершающего «\».
pub fn normalize_path(path: &str) -> String {
    path.trim().trim_end_matches(['\\', '/']).to_lowercase()
}

/// Для каждой записи: повторяет ли она одну из предыдущих.
pub fn duplicate_flags(entries: &[PathEntry]) -> Vec<bool> {
    let mut seen = HashSet::new();
    entries
        .iter()
        .map(|e| !seen.insert(normalize_path(&e.path)))
        .collect()
}

// ---------------------------------------------------------------------------
// DNS и пинг
// ---------------------------------------------------------------------------

pub struct DnsProvider {
    pub name: Pair,
    /// Адреса через «·»; "DHCP" — автоматически, "—" — свои.
    pub servers: &'static str,
    pub note: Pair,
    /// Шаблон DNS-over-HTTPS (пусто — провайдер его не поддерживает или адреса свои).
    pub doh: &'static str,
}

/// Первый пункт — «Автоматично», последний — «Свій» (адреса вводятся вручную).
pub const DNS_PROVIDERS: &[DnsProvider] = &[
    DnsProvider {
        name: ("Автоматично", "Automatic"),
        servers: "DHCP",
        note: ("DNS, який видає провайдер або роутер", "DNS from your ISP or router"),
        doh: "",
    },
    DnsProvider {
        name: ("Cloudflare", "Cloudflare"),
        servers: "1.1.1.1 · 1.0.0.1",
        note: ("Найшвидший для більшості", "Fastest for most people"),
        doh: "https://cloudflare-dns.com/dns-query",
    },
    DnsProvider {
        name: ("Cloudflare із захистом", "Cloudflare with protection"),
        servers: "1.1.1.2 · 1.0.0.2",
        note: ("Швидкий і блокує шкідливі сайти", "Fast, and blocks malicious sites"),
        doh: "https://security.cloudflare-dns.com/dns-query",
    },
    DnsProvider {
        name: ("Google", "Google"),
        servers: "8.8.8.8 · 8.8.4.4",
        note: ("Стабільний і популярний", "Stable and popular"),
        doh: "https://dns.google/dns-query",
    },
    DnsProvider {
        name: ("Quad9", "Quad9"),
        servers: "9.9.9.9 · 149.112.112.112",
        note: ("Блокує шкідливі сайти", "Blocks malicious sites"),
        doh: "https://dns.quad9.net/dns-query",
    },
    DnsProvider {
        name: ("AdGuard", "AdGuard"),
        servers: "94.140.14.14 · 94.140.15.15",
        note: ("Блокує рекламу та трекери", "Blocks ads and trackers"),
        doh: "https://dns.adguard-dns.com/dns-query",
    },
    DnsProvider {
        name: ("OpenDNS", "OpenDNS"),
        servers: "208.67.222.222 · 208.67.220.220",
        note: ("Від Cisco, з фільтрами", "By Cisco, with filters"),
        doh: "https://doh.opendns.com/dns-query",
    },
    DnsProvider {
        name: ("CleanBrowsing", "CleanBrowsing"),
        servers: "185.228.168.9 · 185.228.169.9",
        note: ("Сімейний фільтр", "Family filter"),
        doh: "https://doh.cleanbrowsing.org/doh/security-filter/",
    },
    DnsProvider {
        name: ("Свій", "Custom"),
        servers: "—",
        note: ("Власні адреси DNS-серверів", "Your own DNS server addresses"),
        doh: "",
    },
];

pub const PING_PRESETS: &[&str] = &["1.1.1.1", "8.8.8.8", "google.com", "steamcommunity.com"];
pub const PING_COUNT: usize = 4;

// ---------------------------------------------------------------------------
// hosts
// ---------------------------------------------------------------------------

/// Стандартный hosts Windows (без записей).
pub const DEFAULT_HOSTS: &str = "\
# Copyright (c) 1993-2009 Microsoft Corp.
#
# This is a sample HOSTS file used by Microsoft TCP/IP for Windows.
#
# localhost name resolution is handled within DNS itself.
#	127.0.0.1       localhost
#	::1             localhost
";

pub const TELEMETRY_DOMAINS: &[&str] = &[
    "vortex.data.microsoft.com",
    "vortex-win.data.microsoft.com",
    "settings-win.data.microsoft.com",
    "watson.telemetry.microsoft.com",
    "oca.telemetry.microsoft.com",
    "sqm.telemetry.microsoft.com",
];

pub const AD_DOMAINS: &[&str] = &[
    "doubleclick.net",
    "googleadservices.com",
    "googlesyndication.com",
    "adservice.google.com",
    "ads.yahoo.com",
];

/// Сколько в тексте hosts настоящих записей (не комментариев и не пустых строк).
pub fn hosts_entry_count(text: &str) -> usize {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .count()
}

/// Добавляет запись, если такого домена в файле ещё нет. Возвращает, добавлена ли.
pub fn hosts_add(text: &mut String, ip: &str, domain: &str) -> bool {
    let exists = text.lines().any(|line| {
        let line = line.trim();
        !line.starts_with('#') && line.split_whitespace().skip(1).any(|d| d.eq_ignore_ascii_case(domain))
    });
    if exists {
        return false;
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("{ip:<15} {domain}\n"));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(list: &[&str]) -> Vec<PathEntry> {
        list.iter()
            .map(|p| PathEntry {
                path: (*p).to_owned(),
                missing: false,
                added: false,
            })
            .collect()
    }

    #[test]
    fn hosts_add_skips_duplicates() {
        let mut text = "127.0.0.1 localhost\n".to_owned();
        assert!(hosts_add(&mut text, "0.0.0.0", "example.com"));
        assert!(!hosts_add(&mut text, "0.0.0.0", "example.com"));
        assert!(!hosts_add(&mut text, "0.0.0.0", "EXAMPLE.com"));
        assert_eq!(hosts_entry_count(&text), 2);
        assert_eq!(hosts_entry_count(DEFAULT_HOSTS), 0);
    }

    #[test]
    fn path_duplicates_ignore_case_and_trailing_slash() {
        let list = entries(&[r"C:\Tools\", r"c:\tools", r"C:\Other"]);
        assert_eq!(duplicate_flags(&list), vec![false, true, false]);
    }

    #[test]
    fn drive_folders() {
        let d = DriveInfo {
            letter: "D:".into(),
            label: String::new(),
            system: false,
            is_hdd: true,
            free_gb: 1,
            total_gb: 2,
        };
        assert_eq!(d.default_folder(), r"D:\Programs");
        assert_eq!(d.download_folder(), r"D:\WinForge\Downloads");
    }

    #[test]
    fn system_drive_listed_first() {
        let drives = list_drives();
        assert!(!drives.is_empty());
        assert!(drives[0].system);
    }
}
