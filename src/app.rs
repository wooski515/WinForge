//! Состояние приложения, боковое меню, нижняя панель и диалоги.
//! Содержимое вкладок — в модуле `ui`.

use std::collections::HashSet;

use eframe::egui::{self, vec2, Align, Align2, Color32, FontId, Layout, Margin, RichText, Stroke};

use crate::catalog::{Action, Catalogs, Risk, TweakCatalog};
use crate::i18n::{self, Lang};
use crate::run;
use crate::sys;
use crate::theme::{self, *};
use crate::tools::{CustomApp, CustomKind, InstallSettings, ToolsState};
use crate::widgets;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    // Сборка своей Windows
    Image,
    Apps,
    Privacy,
    System,
    Removal,
    Build,
    // Уже установленная Windows
    Apply,
    Cleanup,
    Registry,
    Startup,
    Network,
    Hosts,
    Integrator,
    Activation,
    DiskMonitor,
    EnvVars,
    Hardware,
    Backups,
}

/// Режим программы: собрать новую Windows или настроить уже установленную.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Build,
    Live,
}

impl Tab {
    /// Шаги мастера сборки (по ним ходят кнопки «Назад» / «Далее»).
    pub const BUILD_STEPS: [Tab; 7] = [
        Tab::Home,
        Tab::Image,
        Tab::Apps,
        Tab::Privacy,
        Tab::System,
        Tab::Removal,
        Tab::Build,
    ];

    /// Шаги для уже установленной системы: те же отметки, но применяются к этому ПК.
    pub const LIVE_STEPS: [Tab; 6] = [
        Tab::Home,
        Tab::Apps,
        Tab::Privacy,
        Tab::System,
        Tab::Removal,
        Tab::Apply,
    ];

    /// Инструменты для уже установленной системы.
    pub const TOOLS: [Tab; 11] = [
        Tab::Cleanup,
        Tab::Registry,
        Tab::Startup,
        Tab::Network,
        Tab::Hosts,
        Tab::Integrator,
        Tab::Activation,
        Tab::DiskMonitor,
        Tab::EnvVars,
        Tab::Hardware,
        Tab::Backups,
    ];

    pub fn steps(mode: Mode) -> &'static [Tab] {
        match mode {
            Mode::Build => &Tab::BUILD_STEPS,
            Mode::Live => &Tab::LIVE_STEPS,
        }
    }

    /// Есть ли вкладка в меню этого режима.
    pub fn in_mode(self, mode: Mode) -> bool {
        Tab::steps(mode).contains(&self) || (mode == Mode::Live && Tab::TOOLS.contains(&self))
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tab::Home => "🏠",
            Tab::Image => "💿",
            Tab::Apps => "📦",
            Tab::Privacy => "🛡",
            Tab::System => "⚙",
            Tab::Removal => "🗑",
            Tab::Build => "🔨",
            Tab::Apply => "⚡",
            Tab::Cleanup => "✨",
            Tab::Registry => "🔧",
            Tab::Startup => "🚀",
            Tab::Network => "📶",
            Tab::Hosts => "📝",
            Tab::Integrator => "🖱",
            Tab::Activation => "🔑",
            Tab::DiskMonitor => "📈",
            Tab::EnvVars => "📁",
            Tab::Hardware => "🖥",
            Tab::Backups => "↺",
        }
    }

    pub fn title(self, lang: Lang) -> &'static str {
        match self {
            Tab::Home => lang.tr("Головна", "Home"),
            Tab::Image => lang.tr("Образ Windows", "Windows image"),
            Tab::Apps => lang.tr("Програми", "Apps"),
            Tab::Privacy => lang.tr("Приватність", "Privacy"),
            Tab::System => lang.tr("Оптимізація", "Optimization"),
            Tab::Removal => lang.tr("Видалення", "Removal"),
            Tab::Build => lang.tr("Збірка", "Build"),
            Tab::Apply => lang.tr("Застосувати", "Apply"),
            Tab::Cleanup => lang.tr("Очищення", "Cleanup"),
            Tab::Registry => lang.tr("Реєстр", "Registry"),
            Tab::Startup => lang.tr("Автозапуск", "Startup"),
            Tab::Network => lang.tr("Мережа", "Network"),
            Tab::Hosts => lang.tr("Файл hosts", "Hosts file"),
            Tab::Integrator => lang.tr("Контекстне меню", "Context menu"),
            Tab::Activation => lang.tr("Активація", "Activation"),
            Tab::DiskMonitor => lang.tr("Монітор дисків", "Disk monitor"),
            Tab::EnvVars => lang.tr("Змінні PATH", "PATH variables"),
            Tab::Hardware => lang.tr("Обладнання", "Hardware"),
            Tab::Backups => lang.tr("Відкат змін", "Undo changes"),
        }
    }

    /// Уникальный номер вкладки (для отдельной прокрутки каждой).
    fn index(self) -> usize {
        self as usize
    }

    fn step_index(self, mode: Mode) -> Option<usize> {
        Tab::steps(mode).iter().position(|t| *t == self)
    }
}

/// Разделы с переключателями.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Privacy,
    System,
    Removal,
    Cleanup,
    Registry,
    Integrator,
}

/// Пара строк (украинский, английский).
pub type Pair = (&'static str, &'static str);

pub const EDITIONS: &[Pair] = &[
    ("Windows 11 Pro", "Windows 11 Pro"),
    ("Windows 11 Home", "Windows 11 Home"),
    ("Windows 11 Pro для робочих станцій", "Windows 11 Pro for Workstations"),
    ("Windows 11 для освітніх закладів", "Windows 11 Education"),
    ("Windows 10 Pro", "Windows 10 Pro"),
    ("Windows 10 Home", "Windows 10 Home"),
];
/// Язык самой устанавливаемой Windows.
pub const LANGUAGES: &[Pair] = &[
    ("Українська", "Ukrainian"),
    ("Англійська (США)", "English (US)"),
];
pub const KEYBOARDS: &[Pair] = &[
    (
        "Українська + Російська + Англійська (США)",
        "Ukrainian + Russian + English (US)",
    ),
    ("Українська + Англійська (США)", "Ukrainian + English (US)"),
    ("Англійська (США)", "English (US)"),
];
pub const TIMEZONES: &[Pair] = &[
    ("(UTC+02:00) Київ", "(UTC+02:00) Kyiv"),
    ("(UTC+01:00) Варшава, Берлін", "(UTC+01:00) Warsaw, Berlin"),
    ("(UTC+00:00) Лондон", "(UTC+00:00) London"),
    ("(UTC+05:00) Астана", "(UTC+05:00) Astana"),
    ("(UTC+00:00) Всесвітній координований час", "(UTC+00:00) Coordinated Universal Time"),
];
/// Коды раскладок клавиатуры для autounattend (InputLocale), в порядке KEYBOARDS.
pub const KEYBOARD_LOCALES: &[&str] = &[
    "0422:00000422;0419:00000419;0409:00000409",
    "0422:00000422;0409:00000409",
    "0409:00000409",
];
/// Идентификаторы часовых поясов Windows, в порядке TIMEZONES.
pub const TIMEZONE_IDS: &[&str] = &[
    "FLE Standard Time",
    "Central European Standard Time",
    "GMT Standard Time",
    "Central Asia Standard Time",
    "UTC",
];

/// Переводит список пар в строки выбранного языка.
pub fn localized(list: &[Pair], lang: Lang) -> Vec<&'static str> {
    list.iter().map(|p| lang.pick(*p)).collect()
}

pub struct ImageSettings {
    pub iso_path: String,
    pub edition: usize,
    pub language: usize,
    pub keyboard: usize,
    pub timezone: usize,
    pub username: String,
    pub password: String,
    pub computer_name: String,
    pub local_account: bool,
    pub auto_login: bool,
    pub skip_oobe: bool,
    pub bypass_tpm: bool,
    pub no_network: bool,
    pub cleanup: bool,
    pub drivers_path: String,
    /// Сеть Wi-Fi, к которой система подключится сама (пусто — не настраивать).
    pub wifi_ssid: String,
    /// Пароль Wi-Fi (в профиль WinForge не сохраняется).
    pub wifi_password: String,
    /// Поставить гостевые дополнения VirtualBox / VMware с подключённого диска.
    pub guest_tools: bool,
    /// Сжать систему (CompactOS).
    pub compact_os: bool,
    /// Свой ключ продукта (в профиль не сохраняется).
    pub product_key: String,
    /// KMS-сервер организации (пусто — не задавать).
    pub kms_host: String,
    pub output_path: String,
    /// Где build.ps1 распаковывает образ (нужно ~40 ГБ). Скрипт работает только
    /// в своей подпапке `WinForge-work` внутри неё.
    pub work_dir: String,
    /// Куда записать результат: ISO-файл или флешка.
    pub output: OutputKind,
    /// Выбранная флешка (для OutputKind::Usb).
    pub usb: Option<crate::sys::probe::UsbDisk>,
}

/// Результат сборки.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    /// Загрузочная флешка — для настоящей установки.
    Usb,
    /// ISO-файл — для виртуалки или записи другой программой.
    Iso,
}

impl Default for ImageSettings {
    fn default() -> Self {
        Self {
            iso_path: String::new(),
            edition: 0,
            language: 0,
            keyboard: 0,
            timezone: 0,
            username: "User".into(),
            password: String::new(),
            computer_name: "MY-PC".into(),
            local_account: true,
            auto_login: false,
            skip_oobe: true,
            bypass_tpm: false,
            no_network: true,
            cleanup: true,
            drivers_path: String::new(),
            wifi_ssid: String::new(),
            wifi_password: String::new(),
            guest_tools: false,
            compact_os: false,
            product_key: String::new(),
            kms_host: String::new(),
            output_path: format!(r"{}\WinForge_Windows.iso", default_build_base()),
            work_dir: default_build_base(),
            output: OutputKind::Usb,
            usb: None,
        }
    }
}

/// Папка для сборки по умолчанию: на диске с наибольшим свободным местом,
/// по возможности не на системном (там обычно тесно).
fn default_build_base() -> String {
    let drives = crate::tools::list_drives();
    let best = drives
        .iter()
        .filter(|d| !d.system)
        .max_by_key(|d| d.free_gb)
        .filter(|d| d.free_gb >= crate::sys::image::BUILD_SPACE_GB)
        .or_else(|| drives.iter().max_by_key(|d| d.free_gb));
    match best {
        Some(d) => format!(r"{}\WinForge", d.letter),
        None => r"C:\WinForge".into(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    BuildIso,
    ApplyLive,
    Clean,
    RegistryFix,
    ContextMenu,
}

impl JobKind {
    /// Меняет ли задача систему настолько, что перед ней стоит сделать точку восстановления.
    pub fn makes_restore_point(self) -> bool {
        matches!(self, JobKind::ApplyLive | JobKind::RegistryFix | JobKind::ContextMenu)
    }
}

/// Результат генерации файлов сборки образа.
pub struct BuildResult {
    pub ok: bool,
    /// Папка, куда записаны файлы.
    pub dir: std::path::PathBuf,
    pub lines: Vec<String>,
}

/// Идущая сборка: build.ps1 работает скрыто от имени администратора,
/// WinForge читает его файл прогресса и показывает шаги и проценты.
pub struct BuildRun {
    pub process: sys::AdminProcess,
    pub progress_file: std::path::PathBuf,
    pub cancel_file: std::path::PathBuf,
    pub to_usb: bool,
    pub started: std::time::Instant,
    pub progress: sys::image::BuildProgress,
    pub last_read: Option<std::time::Instant>,
    /// Код выхода, если процесс завершился.
    pub exit: Option<u32>,
    pub cancel_requested: bool,
}

impl BuildRun {
    /// Перечитывает файл прогресса (не чаще 3 раз в секунду) и проверяет процесс.
    fn refresh(&mut self) {
        if self.progress.finished() || self.exit.is_some() {
            return;
        }
        if self.last_read.is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(300)) {
            return;
        }
        self.last_read = Some(std::time::Instant::now());
        // Код выхода — до чтения файла: последние строки скрипт пишет перед выходом.
        let exit = self.process.exit_code();
        if let Ok(text) = std::fs::read_to_string(&self.progress_file) {
            self.progress = sys::image::BuildProgress::parse(&text);
        }
        if exit.is_some() {
            self.exit = exit;
        }
    }

    /// Причина, если процесс закончился, ничего не сообщив (окно убили, сбой PowerShell).
    fn silent_exit(&self) -> Option<u32> {
        match self.exit {
            Some(code) if !self.progress.finished() => Some(code),
            _ => None,
        }
    }

    fn running(&self) -> bool {
        !self.progress.finished() && self.exit.is_none()
    }
}

/// Текущая операция: фоновое выполнение, готовые файлы сборки или сама сборка.
pub enum JobState {
    Run { kind: JobKind, runner: run::Runner },
    Build(BuildResult),
    Building(BuildRun),
}

impl JobKind {
    /// Нужны ли права администратора (изменения в HKLM / системных службах).
    pub fn needs_admin(self) -> bool {
        matches!(self, JobKind::ApplyLive | JobKind::RegistryFix)
    }
}

pub struct WinForgeApp {
    pub lang: Lang,
    pub tab: Tab,
    /// Собрать новую Windows или настроить эту.
    pub mode: Mode,
    pub cat: Catalogs,
    pub catalog_errors: Vec<String>,
    pub selected_apps: HashSet<String>,
    /// Включённые твики и отмеченные пункты всех разделов (по id).
    pub enabled: HashSet<String>,
    pub search: String,
    pub image: ImageSettings,
    /// Куда ставить программы и свои программы.
    pub install: InstallSettings,
    pub tools: ToolsState,
    pub profile_name: String,
    /// id опасного пункта, ожидающего подтверждения.
    pub confirm: Option<String>,
    pub job: Option<JobState>,
    /// Что введено в поле «YES» перед записью флешки.
    usb_confirm: String,
    toast: Option<(String, f64)>,
    /// Короткие фоновые задачи: по завершении их текст показывается уведомлением.
    bg_tasks: Vec<std::sync::mpsc::Receiver<String>>,
}

impl WinForgeApp {
    pub fn new() -> Self {
        let (cat, catalog_errors) = Catalogs::load();
        let mut app = Self {
            lang: Lang::Uk,
            tab: Tab::Home,
            mode: Mode::Build,
            cat,
            catalog_errors,
            selected_apps: HashSet::new(),
            enabled: HashSet::new(),
            search: String::new(),
            image: ImageSettings::default(),
            install: InstallSettings::default(),
            tools: ToolsState::default(),
            profile_name: "my-profile".into(),
            confirm: None,
            job: None,
            usb_confirm: String::new(),
            toast: None,
            bg_tasks: Vec::new(),
        };
        app.apply_preset("base");
        // Реестр и контекстное меню стартуют пустыми: это разовые действия по желанию.
        for section in [Section::Privacy, Section::System, Section::Removal, Section::Cleanup] {
            app.set_recommended(section);
        }
        app
    }

    /// Короткая запись для перевода: `self.tr("укр", "eng")`.
    pub fn tr<'a>(&self, uk: &'a str, en: &'a str) -> &'a str {
        self.lang.tr(uk, en)
    }

    pub fn notify(&mut self, ctx: &egui::Context, message: impl Into<String>) {
        let until = ctx.input(|i| i.time) + 2.8;
        self.toast = Some((message.into(), until));
    }

    /// Запускает короткую задачу в фоне; её результат покажется уведомлением.
    pub fn spawn_task(&mut self, task: impl FnOnce() -> String + Send + 'static) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(task());
        });
        self.bg_tasks.push(rx);
    }

    /// Забирает результаты завершившихся фоновых задач.
    fn poll_tasks(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        self.bg_tasks.retain(|rx| match rx.try_recv() {
            Ok(msg) => {
                done.push(msg);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        if let Some(msg) = done.pop() {
            self.notify(ctx, msg);
        }
        if !self.bg_tasks.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    /// Каталог твиков раздела (для «Удаления» — None, у него свой формат).
    pub fn section_catalog(&self, section: Section) -> Option<&TweakCatalog> {
        match section {
            Section::Privacy => Some(&self.cat.privacy),
            Section::System => Some(&self.cat.system),
            Section::Cleanup => Some(&self.cat.cleaner),
            Section::Registry => Some(&self.cat.registry),
            Section::Integrator => Some(&self.cat.integrator),
            Section::Removal => None,
        }
    }

    /// Все пункты раздела: (id, рекомендуется ли).
    pub fn section_items(&self, section: Section) -> Vec<(String, bool)> {
        match self.section_catalog(section) {
            Some(catalog) => catalog
                .group
                .iter()
                .flat_map(|g| g.tweak.iter().map(|t| (t.id.clone(), t.recommended)))
                .collect(),
            None => self
                .cat
                .bloat
                .app
                .iter()
                .map(|a| (a.id.clone(), a.recommended))
                .collect(),
        }
    }

    pub fn set_recommended(&mut self, section: Section) {
        for (id, recommended) in self.section_items(section) {
            if recommended {
                self.enabled.insert(id);
            } else {
                self.enabled.remove(&id);
            }
        }
    }

    pub fn clear_section(&mut self, section: Section) {
        for (id, _) in self.section_items(section) {
            self.enabled.remove(&id);
        }
    }

    /// (включено, всего)
    pub fn count_enabled(&self, section: Section) -> (usize, usize) {
        let items = self.section_items(section);
        let on = items.iter().filter(|(id, _)| self.enabled.contains(id)).count();
        (on, items.len())
    }

    /// Включённые id раздела в порядке каталога.
    pub fn enabled_in(&self, section: Section) -> Vec<String> {
        self.section_items(section)
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| self.enabled.contains(id))
            .collect()
    }

    /// Название, риск и предупреждение пункта на текущем языке.
    pub fn find_item(&self, id: &str) -> Option<(String, Risk, String)> {
        let l = self.lang;
        for catalog in self.cat.tweak_catalogs() {
            for group in &catalog.group {
                if let Some(t) = group.tweak.iter().find(|t| t.id == id) {
                    let warning = t.warning.as_ref().map(|w| w.get(l).to_owned()).unwrap_or_default();
                    return Some((t.name.get(l).to_owned(), t.risk, warning));
                }
            }
        }
        self.cat.bloat.app.iter().find(|a| a.id == id).map(|a| {
            let warning = a.warning.as_ref().map(|w| w.get(l).to_owned()).unwrap_or_default();
            (a.name.get(l).to_owned(), a.risk, warning)
        })
    }

    /// Клик по переключателю/карточке. Опасные пункты сначала спрашивают подтверждение.
    pub fn click_item(&mut self, id: &str) {
        if self.enabled.remove(id) {
            return;
        }
        if let Some((_, risk, _)) = self.find_item(id) {
            if risk == Risk::Danger {
                self.confirm = Some(id.to_owned());
            } else {
                self.enabled.insert(id.to_owned());
            }
        }
    }

    pub fn toggle_app(&mut self, id: &str) {
        if !self.selected_apps.remove(id) {
            self.selected_apps.insert(id.to_owned());
        }
    }

    /// Добавляет программы пресета к уже выбранным. Возвращает, сколько добавилось.
    pub fn apply_preset(&mut self, preset: &str) -> usize {
        let ids: Vec<String> = self
            .cat
            .apps
            .category
            .iter()
            .flat_map(|c| c.app.iter())
            .filter(|a| a.presets.iter().any(|p| p == preset))
            .map(|a| a.id.clone())
            .collect();
        let before = self.selected_apps.len();
        self.selected_apps.extend(ids);
        self.selected_apps.len() - before
    }

    fn tab_badge(&self, tab: Tab) -> Option<usize> {
        match tab {
            Tab::Apps => Some(self.selected_apps.len()),
            Tab::Privacy => Some(self.count_enabled(Section::Privacy).0),
            Tab::System => Some(self.count_enabled(Section::System).0),
            Tab::Removal => Some(self.count_enabled(Section::Removal).0),
            Tab::Cleanup => Some(self.count_enabled(Section::Cleanup).0),
            Tab::Registry => Some(self.count_enabled(Section::Registry).0),
            Tab::Integrator => Some(self.count_enabled(Section::Integrator).0),
            _ => None,
        }
    }

    pub fn iso_file_name(&self) -> Option<String> {
        let path = self.image.iso_path.trim();
        if path.is_empty() {
            return None;
        }
        Some(
            std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_owned()),
        )
    }

    /// Запускает операцию. `preview = true` — только чтение состояния.
    pub fn start_job(&mut self, _ctx: &egui::Context, kind: JobKind, preview: bool) {
        if kind == JobKind::BuildIso {
            self.usb_confirm.clear();
            self.job = Some(JobState::Build(self.generate_build()));
            return;
        }
        let mut work = self.build_work(kind);
        // Перед настоящими изменениями — точка восстановления Windows.
        if !preview && self.tools.restore_point && kind.makes_restore_point() {
            if let run::Work::Apply { units, .. } = &mut work {
                if !units.is_empty() {
                    units.insert(0, self.restore_point_unit());
                }
            }
        }
        self.job = Some(JobState::Run {
            kind,
            runner: run::Runner::start(preview, work),
        });
    }

    /// Точка восстановления. Windows создаёт не больше одной в сутки: если сегодня
    /// уже была, Checkpoint-Computer молча пропускает — откатиться можно к ней.
    fn restore_point_unit(&self) -> run::Unit {
        run::Unit {
            label: self.tr("Точка відновлення Windows", "Windows restore point").into(),
            actions: vec![Action {
                run: Some(
                    "$ErrorActionPreference = 'Stop'; try { Checkpoint-Computer -Description 'WinForge' -RestorePointType MODIFY_SETTINGS -WarningAction SilentlyContinue }                      catch { throw \"Restore point not created (is System Protection on for drive C:?): $($_.Exception.Message)\" }"
                        .into(),
                ),
                shell: Some("powershell".into()),
                when: Some("live".into()),
                ..Default::default()
            }],
        }
    }

    fn build_work(&self, kind: JobKind) -> run::Work {
        match kind {
            JobKind::Clean => run::Work::Clean {
                units: self.clean_units(),
            },
            JobKind::RegistryFix => {
                let mut units = self.tweak_units(Section::Registry);
                if self.tools.restart_explorer {
                    units.push(run::Unit {
                        label: self.tr("Перезапуск Провідника", "Restart Explorer").into(),
                        actions: vec![Action {
                            run: Some("taskkill /f /im explorer.exe & start explorer.exe".into()),
                            shell: Some("cmd".into()),
                            when: Some("live".into()),
                            ..Default::default()
                        }],
                    });
                }
                run::Work::Apply {
                    label: self.tr("Виправлення реєстру", "Registry fixes").into(),
                    units,
                }
            }
            JobKind::ContextMenu => run::Work::Apply {
                label: self.tr("Контекстне меню", "Context menu").into(),
                units: self.integrator_units(),
            },
            JobKind::ApplyLive => {
                // Сначала программы: твики могут остановить службы (BITS и т.п.),
                // нужные winget для скачивания.
                let mut units = self.app_install_units();
                units.extend(self.removal_units());
                units.extend(self.tweak_units(Section::Privacy));
                units.extend(self.tweak_units(Section::System));
                run::Work::Apply {
                    label: self.tr("Застосування до системи", "Applying to system").into(),
                    units,
                }
            }
            JobKind::BuildIso => run::Work::Apply {
                label: String::new(),
                units: Vec::new(),
            },
        }
    }

    /// Единицы работы для раздела твиков (только пункты с действиями).
    fn tweak_units(&self, section: Section) -> Vec<run::Unit> {
        let l = self.lang;
        let Some(catalog) = self.section_catalog(section) else {
            return Vec::new();
        };
        catalog
            .group
            .iter()
            .flat_map(|g| g.tweak.iter())
            .filter(|t| self.enabled.contains(&t.id) && !t.actions.is_empty())
            .map(|t| run::Unit {
                label: t.name.get(l).to_owned(),
                actions: t.actions.clone(),
            })
            .collect()
    }

    fn removal_units(&self) -> Vec<run::Unit> {
        let l = self.lang;
        self.cat
            .bloat
            .app
            .iter()
            .filter(|a| self.enabled.contains(&a.id) && !a.actions.is_empty())
            .map(|a| run::Unit {
                label: a.name.get(l).to_owned(),
                actions: a.actions.clone(),
            })
            .collect()
    }

    /// Пункты контекстного меню + команды Win+R (как записи App Paths).
    fn integrator_units(&self) -> Vec<run::Unit> {
        let mut units = self.tweak_units(Section::Integrator);
        for (alias, target) in &self.tools.run_commands {
            let alias = alias.trim();
            let target = target.trim();
            if alias.is_empty() || target.is_empty() {
                continue;
            }
            units.push(run::Unit {
                label: format!("Win+R: {alias}"),
                actions: vec![Action {
                    reg: Some(format!(
                        r"HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths\{alias}.exe"
                    )),
                    name: Some(String::new()),
                    sz: Some(target.to_owned()),
                    ..Default::default()
                }],
            });
        }
        units
    }

    /// Установка выбранных программ (winget-команды и свои установщики).
    fn app_install_units(&self) -> Vec<run::Unit> {
        let l = self.lang;
        let folder = self.install.folder.clone();
        let mut units = Vec::new();
        for cat in &self.cat.apps.category {
            for a in &cat.app {
                if self.selected_apps.contains(&a.id) && !a.winget.is_empty() {
                    units.push(run::Unit {
                        label: a.name.get(l).to_owned(),
                        actions: vec![Action {
                            // --location — папка самой программы: у каждой своя подпапка.
                            // Не все установщики её понимают (VC++, Telegram) — тогда
                            // повтор в папку по умолчанию.
                            run: Some(match sys::image::app_install_dir(&folder, a.name.get(Lang::En)) {
                                Some(dir) => format!(
                                    "winget install --id {id} -e --silent --accept-package-agreements --accept-source-agreements --location \"{dir}\" \
                                     || winget install --id {id} -e --silent --accept-package-agreements --accept-source-agreements",
                                    id = a.winget
                                ),
                                None => format!(
                                    "winget install --id {} -e --silent --accept-package-agreements --accept-source-agreements",
                                    a.winget
                                ),
                            }),
                            shell: Some("cmd".into()),
                            when: Some("live".into()),
                            ..Default::default()
                        }],
                    });
                }
            }
        }
        for c in &self.install.custom {
            if !self.selected_apps.contains(&c.id) {
                continue;
            }
            let cmd = match c.kind {
                CustomKind::Winget => format!(
                    "winget install --id {} --silent --accept-package-agreements --accept-source-agreements",
                    c.value
                ),
                // .msi — через msiexec: сам по себе он не исполняемый файл.
                CustomKind::Installer if c.value.trim().to_lowercase().ends_with(".msi") => {
                    format!("msiexec /i \"{}\" {}", c.value.trim(), c.args)
                }
                CustomKind::Installer => format!("\"{}\" {}", c.value.trim(), c.args),
            };
            units.push(run::Unit {
                label: c.name.clone(),
                actions: vec![Action {
                    run: Some(cmd),
                    shell: Some("cmd".into()),
                    when: Some("live".into()),
                    ..Default::default()
                }],
            });
        }
        units
    }

    fn clean_units(&self) -> Vec<run::CleanUnit> {
        let l = self.lang;
        let mut out = Vec::new();
        for g in &self.cat.cleaner.group {
            for t in &g.tweak {
                if self.enabled.contains(&t.id) && (!t.paths.is_empty() || t.recycle) {
                    out.push(run::CleanUnit {
                        label: format!("{} — {}", g.name.get(l), t.name.get(l)),
                        paths: t.paths.clone(),
                        recycle: t.recycle,
                        stop_services: t.stop_services.clone(),
                    });
                }
            }
        }
        out
    }

    /// Генерирует файлы сборки образа в папке данных: autounattend.xml,
    /// first-logon.cmd, setup-apps.ps1 и build.ps1.
    fn generate_build(&self) -> BuildResult {
        let l = self.lang;
        let dir = sys::data_dir().join("build");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return BuildResult {
                ok: false,
                dir,
                lines: vec![format!("{}: {e}", l.tr("Не вдалося створити папку", "Could not create folder"))],
            };
        }

        // Действия выбранных твиков (приватность, система) и удаления, применимые к образу.
        let mut actions: Vec<&Action> = Vec::new();
        for section in [Section::Privacy, Section::System] {
            if let Some(cat) = self.section_catalog(section) {
                for t in cat.group.iter().flat_map(|g| g.tweak.iter()) {
                    if self.enabled.contains(&t.id) {
                        actions.extend(t.actions.iter());
                    }
                }
            }
        }
        for a in self.cat.bloat.app.iter().filter(|a| self.enabled.contains(&a.id)) {
            actions.extend(a.actions.iter());
        }
        actions.retain(|a| sys::engine::applies_image(a));

        let reg_lines: Vec<String> = actions.iter().filter_map(|a| sys::image::setup_reg_line(a)).collect();
        let feature_lines: Vec<String> =
            actions.iter().filter_map(|a| sys::image::setup_feature_line(a)).collect();
        let mut logon_lines: Vec<String> = actions.iter().filter_map(|a| sys::image::first_logon_line(a)).collect();
        let appx: Vec<String> = actions.iter().filter_map(|a| a.appx.clone()).collect();

        let folder = self.install.folder.as_str();
        let winget_ids: Vec<sys::image::WingetInstall> = self
            .cat
            .apps
            .category
            .iter()
            .flat_map(|c| c.app.iter())
            .filter(|a| self.selected_apps.contains(&a.id) && !a.winget.is_empty())
            .map(|a| sys::image::WingetInstall {
                id: a.winget.clone(),
                dir: sys::image::app_install_dir(folder, a.name.get(Lang::En)),
            })
            .chain(
                self.install
                    .custom
                    .iter()
                    .filter(|c| self.selected_apps.contains(&c.id) && c.kind == CustomKind::Winget)
                    .map(|c| sys::image::WingetInstall {
                        id: c.value.clone(),
                        dir: sys::image::app_install_dir(folder, &c.name),
                    }),
            )
            .collect();
        // Свои установщики: файлы с этого ПК едут на носителе в WinForge\Installers,
        // в новой системе их путь — C:\WinForge\Installers\<имя>. Ссылки качаются при входе.
        let chosen: Vec<&CustomApp> = self
            .install
            .custom
            .iter()
            .filter(|c| self.selected_apps.contains(&c.id) && c.kind == CustomKind::Installer)
            .collect();
        let local: Vec<&str> = chosen
            .iter()
            .filter(|c| !sys::installer::is_url(&c.value))
            .map(|c| c.value.trim())
            .collect();
        let staged = sys::installer::staged_names(&local);
        let installers: Vec<(String, String)> =
            local.iter().zip(&staged).map(|(from, name)| (from.to_string(), name.clone())).collect();
        let mut staged_iter = staged.iter();
        let custom: Vec<sys::image::CustomInstall> = chosen
            .iter()
            .map(|c| sys::image::CustomInstall {
                name: c.name.clone(),
                value: if sys::installer::is_url(&c.value) {
                    c.value.trim().to_owned()
                } else {
                    format!(
                        "{}\\Installers\\{}",
                        sys::image::SETUP_DIR,
                        staged_iter.next().map(String::as_str).unwrap_or("setup.exe")
                    )
                },
                args: c.args.clone(),
            })
            .collect();

        let build_ps1 = sys::image::build_script(&sys::image::BuildInput {
            source_iso: self.image.iso_path.trim(),
            output_iso: self.image.output_path.trim(),
            edition: EDITIONS[self.image.edition].1,
            drivers: &self.image.drivers_path,
            work_dir: self.image.work_dir.trim(),
            cleanup: self.image.cleanup,
            installers: &installers,
            usb: match (self.image.output, &self.image.usb) {
                (OutputKind::Usb, Some(u)) => Some(sys::image::UsbTarget {
                    number: u.number,
                    serial: &u.serial,
                    size: u.size,
                }),
                _ => None,
            },
        });

        // Скрипты установки вшиваются в autounattend.xml; копии рядом — чтобы их можно было прочитать.
        let ssid = self.image.wifi_ssid.trim();
        let product_key = sys::activation::normalize_key(&self.image.product_key);
        let specialize = sys::image::specialize_script_with(
            &appx,
            &reg_lines,
            &feature_lines,
            &sys::image::SpecializeOptions {
                copy_installers: !installers.is_empty(),
                wifi: !ssid.is_empty(),
                compact_os: self.image.compact_os,
                guest_tools: self.image.guest_tools,
                product_key: product_key.is_some(),
                kms: sys::activation::parse_kms_host(&self.image.kms_host),
            },
        );
        if !ssid.is_empty() || product_key.is_some() {
            // Установщик переписывает копии файла ответов в конце установки — чистим ещё раз.
            logon_lines.push(sys::image::secrets_scrub_logon_line());
        }
        let first_logon = sys::image::first_logon_script(&logon_lines);
        let setup_apps = sys::image::setup_apps_script(&winget_ids, &custom, self.install.delete_installers);
        let mut embedded = vec![
            (format!("{}\\specialize.ps1", sys::image::SETUP_DIR), specialize.clone()),
            (format!("{}\\first-logon.cmd", sys::image::SETUP_DIR), first_logon.clone()),
            (format!("{}\\setup-apps.ps1", sys::image::SETUP_DIR), setup_apps.clone()),
        ];
        if !ssid.is_empty() {
            embedded.push((
                sys::image::WIFI_PROFILE.to_owned(),
                sys::image::wifi_profile_xml(ssid, &self.image.wifi_password),
            ));
        }
        if let Some(key) = &product_key {
            embedded.push((sys::image::PRODUCT_KEY_FILE.to_owned(), key.clone()));
        }

        let files: [(&str, String); 5] = [
            ("autounattend.xml", self.make_unattend(embedded)),
            ("specialize.ps1", specialize),
            ("first-logon.cmd", first_logon),
            ("setup-apps.ps1", setup_apps),
            ("build.ps1", build_ps1),
        ];
        let mut lines = Vec::new();
        let mut ok = true;
        for (name, content) in files {
            let path = dir.join(name);
            // PowerShell 5 читает скрипты без BOM в кодировке ANSI — добавляем BOM.
            let bytes = if name.ends_with(".ps1") {
                let mut b = vec![0xEF, 0xBB, 0xBF];
                b.extend_from_slice(content.as_bytes());
                b
            } else {
                content.into_bytes()
            };
            match std::fs::write(&path, bytes) {
                Ok(()) => lines.push(format!("✔  {name}")),
                Err(e) => {
                    ok = false;
                    lines.push(format!("✖  {name}: {e}"));
                }
            }
        }
        lines.push(String::new());
        lines.push(match l {
            Lang::Uk => format!(
                "Реєстр: {}, компоненти: {}, при першому вході: {}, застосунків до видалення: {}, програм: {}",
                reg_lines.len(),
                feature_lines.len(),
                logon_lines.len(),
                appx.len(),
                winget_ids.len() + custom.len()
            ),
            Lang::En => format!(
                "Registry: {}, features: {}, at first sign-in: {}, apps to remove: {}, apps to install: {}",
                reg_lines.len(),
                feature_lines.len(),
                logon_lines.len(),
                appx.len(),
                winget_ids.len() + custom.len()
            ),
        });
        BuildResult { ok, dir, lines }
    }

    /// autounattend.xml из настроек образа; `files` вшиваются в него и
    /// распаковываются установщиком в новой системе.
    fn make_unattend(&self, files: Vec<(String, String)>) -> String {
        let img = &self.image;
        let ui_lang = if img.language == 0 { "uk-UA" } else { "en-US" };
        let locales = KEYBOARD_LOCALES[img.keyboard.min(KEYBOARD_LOCALES.len() - 1)];
        let tz = TIMEZONE_IDS[img.timezone.min(TIMEZONE_IDS.len() - 1)];
        let username = match img.username.trim() {
            "" => "User",
            name => name,
        };
        let computer_name = match img.computer_name.trim() {
            "" => "MY-PC",
            name => name,
        };
        sys::image::Unattend {
            ui_language: ui_lang.into(),
            input_locales: locales.into(),
            timezone: tz.into(),
            username: username.into(),
            password: img.password.clone(),
            computer_name: computer_name.into(),
            local_account: img.local_account,
            auto_login: img.auto_login,
            bypass_tpm: img.bypass_tpm,
            no_network: img.no_network,
            files,
        }
        .xml()
    }

    /// Сколько МБ освободит очистка по результатам анализа.
    pub fn analysis_total(&self) -> u64 {
        self.tools
            .analysis
            .as_ref()
            .map(|items| items.iter().map(|(_, mb)| mb).sum())
            .unwrap_or(0)
    }

    /// Сколько отмеченных своих программ (всего, из них со своим установщиком).
    pub fn selected_custom(&self) -> (usize, usize) {
        let selected: Vec<_> = self
            .install
            .custom
            .iter()
            .filter(|a| self.selected_apps.contains(&a.id))
            .collect();
        let installers = selected.iter().filter(|a| a.kind == CustomKind::Installer).count();
        (selected.len(), installers)
    }

    // -----------------------------------------------------------------------
    // Боковое меню и нижняя панель
    // -----------------------------------------------------------------------

    /// Переключает режим; вкладка, которой нет в новом режиме, сменяется на «Головну».
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if !self.tab.in_mode(mode) {
            self.tab = Tab::Home;
        }
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav")
            .resizable(false)
            .exact_width(236.0)
            .frame(
                egui::Frame::new()
                    .fill(SIDEBAR)
                    .inner_margin(Margin::symmetric(12, 18)),
            )
            .show(ctx, |ui| {
                widgets::logo(ui);
                ui.add_space(14.0);
                // Режим: собрать новую Windows или настроить ту, что стоит.
                let mut mode = self.mode;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    let width = ui.available_width();
                    for (m, text, hint) in [
                        (
                            Mode::Build,
                            self.tr("💿  Зібрати нову Windows", "💿  Build a new Windows"),
                            self.tr("Свій образ: флешка або ISO", "Your own image: USB drive or ISO"),
                        ),
                        (
                            Mode::Live,
                            self.tr("💻  Налаштувати цю Windows", "💻  Tune this Windows"),
                            self.tr(
                                "Твіки, програми та інструменти для вже встановленої системи",
                                "Tweaks, apps and tools for the system that is already installed",
                            ),
                        ),
                    ] {
                        let selected = mode == m;
                        let button = egui::Button::new(
                            RichText::new(text).size(13.0).color(if selected { Color32::WHITE } else { TEXT }),
                        )
                        .fill(if selected { ACCENT } else { Color32::TRANSPARENT })
                        .min_size(vec2(width, 30.0));
                        if ui.add(button).on_hover_text(hint).clicked() {
                            mode = m;
                        }
                    }
                });
                if mode != self.mode {
                    self.set_mode(mode);
                }
                ui.add_space(8.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let steps = Tab::steps(self.mode);
                        let sections: Vec<(Option<&str>, &[Tab])> = match self.mode {
                            Mode::Build => vec![
                                (None, &steps[..1]),
                                (Some(self.tr("КРОКИ ЗБІРКИ", "BUILD STEPS")), &steps[1..]),
                            ],
                            Mode::Live => vec![
                                (None, &steps[..1]),
                                (Some(self.tr("НАЛАШТУВАННЯ", "SETTINGS")), &steps[1..]),
                                (Some(self.tr("ІНСТРУМЕНТИ", "TOOLS")), &Tab::TOOLS[..]),
                            ],
                        };
                        for (label, tabs) in sections {
                            if let Some(label) = label {
                                ui.add_space(12.0);
                                ui.label(RichText::new(format!("   {label}")).size(11.0).color(MUTED));
                                ui.add_space(4.0);
                            }
                            for &tab in tabs {
                                let badge = self.tab_badge(tab);
                                if widgets::nav_item(ui, tab.icon(), tab.title(self.lang), self.tab == tab, badge)
                                    .clicked()
                                {
                                    self.tab = tab;
                                }
                            }
                        }
                        ui.add_space(8.0);
                    });
            });
    }

    fn bottom_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom")
            .frame(
                egui::Frame::new()
                    .fill(SIDEBAR)
                    .inner_margin(Margin::symmetric(20, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Переключатель языка интерфейса
                    ui.label(RichText::new("🌐").color(MUTED))
                        .on_hover_text(self.tr("Мова інтерфейсу", "Interface language"));
                    for lang in Lang::ALL {
                        let selected = self.lang == lang;
                        let text = RichText::new(lang.name()).color(if selected { Color32::WHITE } else { TEXT });
                        if ui.selectable_label(selected, text).clicked() {
                            self.lang = lang;
                        }
                    }
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    ui.label(RichText::new(self.tr("Профіль", "Profile")).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.profile_name).desired_width(130.0));
                    if ui.button(self.tr("💾 Зберегти", "💾 Save")).clicked() {
                        if let Some(msg) = self.save_profile_dialog() {
                            self.notify(ui.ctx(), msg);
                        }
                    }
                    if ui.button(self.tr("📂 Відкрити", "📂 Open")).clicked() {
                        if let Some(msg) = self.open_profile_dialog() {
                            self.notify(ui.ctx(), msg);
                        }
                    }

                    // «Назад» / «Далее» — только на шагах сборки.
                    let steps = Tab::steps(self.mode);
                    let Some(idx) = self.tab.step_index(self.mode) else {
                        return;
                    };
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(next) = steps.get(idx + 1) {
                            let label = format!("{}: {}  →", self.tr("Далі", "Next"), next.title(self.lang));
                            if widgets::primary_button(ui, &label).clicked() {
                                self.tab = *next;
                            }
                        }
                        if idx > 0 && widgets::secondary_button(ui, self.tr("←  Назад", "←  Back")).clicked() {
                            self.tab = steps[idx - 1];
                        }
                    });
                });
            });
    }

    // -----------------------------------------------------------------------
    // Диалоги и уведомления
    // -----------------------------------------------------------------------

    fn confirm_modal(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm.clone() else {
            return;
        };
        let Some((name, _, warning)) = self.find_item(&id) else {
            self.confirm = None;
            return;
        };
        let l = self.lang;

        let mut decision: Option<bool> = None;
        let modal = egui::Modal::new(egui::Id::new("confirm")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(
                RichText::new(l.tr("⚠  Небезпечне налаштування", "⚠  Risky setting"))
                    .font(FontId::new(20.0, theme::bold()))
                    .color(DANGER),
            );
            ui.add_space(6.0);
            ui.label(RichText::new(&name).strong().color(TEXT));
            ui.label(RichText::new(&warning).color(MUTED));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let accept = egui::Button::new(
                    RichText::new(l.tr("Все одно продовжити", "Continue anyway")).color(Color32::WHITE),
                )
                .fill(DANGER)
                .min_size(vec2(0.0, 32.0));
                if ui.add(accept).clicked() {
                    decision = Some(true);
                }
                if widgets::secondary_button(ui, l.tr("Скасувати", "Cancel")).clicked() {
                    decision = Some(false);
                }
            });
        });
        if decision.is_none() && modal.should_close() {
            decision = Some(false);
        }

        match decision {
            Some(true) => {
                self.enabled.insert(id);
                self.confirm = None;
            }
            Some(false) => self.confirm = None,
            None => {}
        }
    }

    fn job_modal(&mut self, ctx: &egui::Context) {
        enum Decision {
            None,
            Close,
            Apply(JobKind),
            Elevate,
            OpenFolder(std::path::PathBuf),
            RunBuild(std::path::PathBuf, bool),
            CancelBuild,
        }
        if let Some(JobState::Building(run)) = &mut self.job {
            run.refresh();
        }
        let Some(job) = &self.job else {
            return;
        };
        let l = self.lang;
        let mut decision = Decision::None;

        match job {
            JobState::Build(result) => {
                let ok = result.ok;
                let lines = result.lines.clone();
                let dir = result.dir.clone();
                let source = self.image.iso_path.trim().to_owned();
                let source_ok = !source.is_empty() && std::path::Path::new(&source).is_file();
                let to_usb = self.image.output == OutputKind::Usb;
                let usb = self.image.usb.clone();
                egui::Modal::new(egui::Id::new("job")).show(ctx, |ui| {
                    ui.set_width(620.0);
                    ui.label(
                        RichText::new(l.tr("💿  Файли збірки образу", "💿  Image build files"))
                            .font(FontId::new(20.0, theme::bold()))
                            .color(TEXT),
                    );
                    ui.label(
                        RichText::new(l.tr(
                            "WinForge підготував файли. Саму збірку (10–20 хв) виконує build.ps1 від імені адміністратора. Твіки застосує інсталятор Windows під час встановлення.",
                            "WinForge prepared the files. build.ps1 does the build itself (10–20 min) as administrator. Windows Setup applies the tweaks during installation.",
                        ))
                        .size(12.5)
                        .color(MUTED),
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new(dir.display().to_string()).monospace().size(12.0).color(ACCENT));
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for line in &lines {
                            let color = if line.starts_with('✖') { DANGER } else { TEXT };
                            ui.label(RichText::new(line).size(13.0).color(color));
                        }
                    });
                    ui.add_space(10.0);
                    if !ok {
                        ui.label(RichText::new(l.tr("Є помилки запису.", "Some files failed to write.")).color(DANGER));
                        ui.add_space(4.0);
                    }
                    if !source_ok {
                        ui.label(
                            RichText::new(l.tr(
                                "Щоб запустити збірку, вибери вихідний ISO на вкладці «Образ Windows».",
                                "To run the build, pick a source ISO on the Windows image tab.",
                            ))
                            .size(12.5)
                            .color(MODERATE),
                        );
                        ui.add_space(4.0);
                    }
                    match (to_usb, &usb) {
                        (true, None) => {
                            ui.label(
                                RichText::new(l.tr(
                                    "Флешку не вибрано: вибери її на вкладці «Збірка» і підготуй файли ще раз.",
                                    "No USB drive selected: pick one on the Build tab and prepare the files again.",
                                ))
                                .size(12.5)
                                .color(MODERATE),
                            );
                            ui.add_space(4.0);
                        }
                        (true, Some(u)) => {
                            let text = match l {
                                Lang::Uk => format!(
                                    "⚠ Флешку «{}» ({:.1} ГБ) буде повністю стерто. Щоб підтвердити, введи YES:",
                                    u.name,
                                    u.size_gb()
                                ),
                                Lang::En => format!(
                                    "⚠ The USB drive \"{}\" ({:.1} GB) will be fully erased. To confirm, type YES:",
                                    u.name,
                                    u.size_gb()
                                ),
                            };
                            ui.label(RichText::new(text).size(12.5).color(DANGER));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.usb_confirm)
                                    .hint_text("YES")
                                    .desired_width(120.0),
                            );
                            ui.add_space(4.0);
                        }
                        _ => {}
                    }
                    ui.horizontal(|ui| {
                        if widgets::secondary_button(ui, l.tr("Закрити", "Close")).clicked() {
                            decision = Decision::Close;
                        }
                        if widgets::secondary_button(ui, l.tr("📂 Відкрити папку", "📂 Open folder")).clicked() {
                            decision = Decision::OpenFolder(dir.clone());
                        }
                        let run_label = if to_usb {
                            l.tr("▶ Зібрати й записати на флешку", "▶ Build and write to USB")
                        } else {
                            l.tr("▶ Запустити збірку", "▶ Run the build")
                        };
                        let confirmed = !to_usb || (usb.is_some() && self.usb_confirm.trim() == "YES");
                        let run = ui.add_enabled(
                            ok && source_ok && confirmed,
                            egui::Button::new(RichText::new(run_label).color(Color32::WHITE))
                                .fill(if to_usb { DANGER } else { ACCENT })
                                .min_size(vec2(0.0, 32.0)),
                        );
                        if run.clicked() {
                            decision = Decision::RunBuild(dir.join("build.ps1"), to_usb);
                        }
                    });
                });
            }
            JobState::Building(run) => {
                let p = &run.progress;
                let running = run.running();
                let silent_exit = run.silent_exit();
                let elapsed = run.started.elapsed().as_secs();
                let title = if run.to_usb {
                    l.tr("💾  Запис флешки", "💾  Writing the USB drive")
                } else {
                    l.tr("💿  Збірка ISO", "💿  Building the ISO")
                };
                let labels = [
                    l.tr("Копіювання файлів інсталяції", "Copying installation files"),
                    l.tr("Вибір редакції Windows", "Selecting the Windows edition"),
                    l.tr("Файл відповідей з твіками", "Answer file with the tweaks"),
                    if run.to_usb {
                        l.tr("Запис на флешку", "Writing to the USB drive")
                    } else {
                        l.tr("Запис ISO-файлу", "Writing the ISO file")
                    },
                ];
                egui::Modal::new(egui::Id::new("job")).show(ctx, |ui| {
                    ui.set_width(560.0);
                    ui.label(RichText::new(title).font(FontId::new(20.0, theme::bold())).color(TEXT));
                    let time = format!("{}:{:02}", elapsed / 60, elapsed % 60);
                    let sub = if running {
                        match l {
                            Lang::Uk => format!("Іде збірка · {time}. Можна користуватися іншими програмами."),
                            Lang::En => format!("Building · {time}. You can use other programs meanwhile."),
                        }
                    } else {
                        match l {
                            Lang::Uk => format!("Тривалість: {time}"),
                            Lang::En => format!("Duration: {time}"),
                        }
                    };
                    ui.label(RichText::new(sub).size(12.5).color(MUTED));
                    ui.add_space(10.0);
                    let failed = p.error.is_some() || silent_exit.is_some();
                    ui.add(
                        egui::ProgressBar::new(p.fraction())
                            .show_percentage()
                            .fill(if failed { DANGER } else if p.done.is_some() { SAFE } else { ACCENT })
                            .animate(running),
                    );
                    ui.add_space(8.0);
                    for (i, label) in labels.iter().enumerate() {
                        let n = i as u32 + 1;
                        let (icon, color, extra) = if p.done.is_some() || n < p.step {
                            ("✔", SAFE, String::new())
                        } else if n == p.step && failed {
                            ("✖", DANGER, String::new())
                        } else if n == p.step {
                            let extra = match p.pct {
                                Some(pct) => format!(" — {pct}%"),
                                None => " …".to_owned(),
                            };
                            ("●", ACCENT, extra)
                        } else {
                            ("○", MUTED, String::new())
                        };
                        ui.label(RichText::new(format!("{icon}   {label}{extra}")).color(color));
                    }
                    for w in &p.warnings {
                        ui.add(egui::Label::new(RichText::new(format!("⚠ {w}")).size(12.5).color(MODERATE)).wrap());
                    }
                    ui.add_space(10.0);
                    if let Some(done) = &p.done {
                        let text = match l {
                            Lang::Uk => format!("Готово: {done}"),
                            Lang::En => format!("Done: {done}"),
                        };
                        ui.add(egui::Label::new(RichText::new(text).color(SAFE).strong()).wrap());
                    } else if let Some(err) = &p.error {
                        let text = if run.cancel_requested {
                            l.tr("Збірку скасовано.", "The build was cancelled.").to_owned()
                        } else {
                            match l {
                                Lang::Uk => format!("Помилка: {err}"),
                                Lang::En => format!("Error: {err}"),
                            }
                        };
                        ui.add(egui::Label::new(RichText::new(text).color(DANGER)).wrap());
                    } else if let Some(code) = silent_exit {
                        let text = match l {
                            Lang::Uk => format!("Збірка зупинилася без повідомлення (код {code}). Подивись журнал."),
                            Lang::En => format!("The build stopped without a message (code {code}). See the log."),
                        };
                        ui.add(egui::Label::new(RichText::new(text).color(DANGER)).wrap());
                    } else if run.cancel_requested {
                        ui.label(
                            RichText::new(l.tr(
                                "Зупиняємо… Вибір редакції не переривається — дочекайся кінця кроку.",
                                "Stopping… Selecting the edition can't be interrupted — wait for the step to end.",
                            ))
                            .size(12.5)
                            .color(MODERATE),
                        );
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if running {
                            let cancel = ui.add_enabled(
                                !run.cancel_requested,
                                egui::Button::new(l.tr("Скасувати", "Cancel")).min_size(vec2(0.0, 32.0)),
                            );
                            if cancel.clicked() {
                                decision = Decision::CancelBuild;
                            }
                        } else {
                            if widgets::secondary_button(ui, l.tr("Закрити", "Close")).clicked() {
                                decision = Decision::Close;
                            }
                            if p.done.is_some() {
                                let folder = if run.to_usb {
                                    None
                                } else {
                                    std::path::Path::new(self.image.output_path.trim()).parent().map(|d| d.to_path_buf())
                                };
                                if let Some(folder) = folder {
                                    if widgets::primary_button(ui, l.tr("📂 Відкрити папку з ISO", "📂 Open the ISO folder")).clicked() {
                                        decision = Decision::OpenFolder(folder);
                                    }
                                }
                            }
                        }
                        let log = p.log.clone().map(std::path::PathBuf::from).filter(|f| f.is_file());
                        if let Some(log) = log {
                            if widgets::secondary_button(ui, l.tr("📄 Журнал", "📄 Log")).clicked() {
                                decision = Decision::OpenFolder(log);
                            }
                        }
                    });
                });
                if running {
                    ctx.request_repaint_after(std::time::Duration::from_millis(300));
                }
            }
            JobState::Run { kind, runner } => {
                let kind = *kind;
                let snap = runner.snapshot();
                let preview = snap.preview;
                let finished = snap.finished;
                let needs_admin = kind.needs_admin();
                let elevated = sys::is_elevated();

                let title = match kind {
                    JobKind::ApplyLive => l.tr("⚡  Застосування до цієї системи", "⚡  Applying to this system"),
                    JobKind::Clean => l.tr("✨  Очищення диска", "✨  Disk cleanup"),
                    JobKind::RegistryFix => l.tr("🔧  Виправлення реєстру", "🔧  Registry fixes"),
                    JobKind::ContextMenu => l.tr("🖱  Контекстне меню", "🖱  Context menu"),
                    JobKind::BuildIso => "",
                };
                let subtitle = if preview {
                    l.tr(
                        "Попередній перегляд: показано, що зміниться. Нічого не змінено.",
                        "Preview: this is what will change. Nothing has been changed.",
                    )
                } else {
                    l.tr(
                        "Застосування змін. Зберігається резервна копія для відкату.",
                        "Applying changes. A backup is saved so you can roll back.",
                    )
                };

                egui::Modal::new(egui::Id::new("job")).show(ctx, |ui| {
                    ui.set_width(560.0);
                    ui.label(RichText::new(title).font(FontId::new(20.0, theme::bold())).color(TEXT));
                    ui.label(
                        RichText::new(subtitle)
                            .size(12.5)
                            .color(if preview { MODERATE } else { ACCENT }),
                    );
                    ui.add_space(10.0);
                    ui.add(
                        egui::ProgressBar::new(snap.fraction())
                            .show_percentage()
                            .fill(ACCENT)
                            .animate(!finished),
                    );
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                        for step in &snap.steps {
                            let (icon, color, extra) = match &step.outcome {
                                run::Outcome::Wait => ("○", MUTED, String::new()),
                                run::Outcome::Active => ("●", ACCENT, String::new()),
                                run::Outcome::Good(s) => ("✔", SAFE, s.clone()),
                                run::Outcome::Warn(s) => ("●", MODERATE, s.clone()),
                                run::Outcome::Bad(s) => ("✖", DANGER, s.clone()),
                            };
                            let text = if extra.is_empty() {
                                format!("{icon}   {}", step.label)
                            } else {
                                format!("{icon}   {} — {extra}", step.label)
                            };
                            ui.add(egui::Label::new(RichText::new(text).color(color)).wrap());
                        }
                        if snap.steps.is_empty() {
                            ui.label(
                                RichText::new(l.tr("Немає пунктів для застосування.", "Nothing selected to apply."))
                                    .color(MUTED),
                            );
                        }
                    });
                    ui.add_space(12.0);

                    if finished {
                        ui.label(RichText::new(&snap.summary).color(SAFE).strong());
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if widgets::secondary_button(ui, l.tr("Закрити", "Close")).clicked() {
                                decision = Decision::Close;
                            }
                            // В предпросмотре предлагаем реально применить.
                            if preview && !snap.steps.is_empty() {
                                if needs_admin && !elevated {
                                    if widgets::primary_button(
                                        ui,
                                        l.tr("🛡 Перезапустити як адміністратор", "🛡 Restart as administrator"),
                                    )
                                    .clicked()
                                    {
                                        decision = Decision::Elevate;
                                    }
                                } else {
                                    let label = match kind {
                                        JobKind::Clean => l.tr("✨ Очистити насправді", "✨ Clean for real"),
                                        _ => l.tr("⚡ Застосувати насправді", "⚡ Apply for real"),
                                    };
                                    if widgets::primary_button(ui, label).clicked() {
                                        decision = Decision::Apply(kind);
                                    }
                                }
                            }
                        });
                        if preview && kind.makes_restore_point() && !snap.steps.is_empty() {
                            ui.checkbox(
                                &mut self.tools.restore_point,
                                l.tr(
                                    "Спершу створити точку відновлення Windows",
                                    "Create a Windows restore point first",
                                ),
                            )
                            .on_hover_text(l.tr(
                                "Якщо щось піде не так — «Відновлення системи» поверне все як було",
                                "If anything goes wrong, System Restore brings everything back",
                            ));
                        }
                        if preview && needs_admin && !elevated {
                            ui.label(
                                RichText::new(l.tr(
                                    "Зміни системного реєстру потребують прав адміністратора.",
                                    "System registry changes require administrator rights.",
                                ))
                                .size(12.0)
                                .color(MUTED),
                            );
                        }
                    } else if preview {
                        if widgets::secondary_button(ui, l.tr("Скасувати", "Cancel")).clicked() {
                            decision = Decision::Close;
                        }
                    } else {
                        ui.label(
                            RichText::new(l.tr("Застосування…", "Applying…")).color(MUTED),
                        );
                    }
                });

                if !finished {
                    ctx.request_repaint_after(std::time::Duration::from_millis(120));
                }
            }
        }

        match decision {
            Decision::None => {}
            Decision::Close => {
                self.tools.analysis = None;
                self.job = None;
            }
            Decision::Apply(kind) => {
                self.start_job(ctx, kind, false);
            }
            Decision::Elevate => {
                if sys::relaunch_as_admin() {
                    self.notify(ctx, self.tr("Запуск від адміністратора…", "Starting as administrator…"));
                } else {
                    self.notify(ctx, self.tr("Не вдалося запустити від адміністратора", "Could not start as administrator"));
                }
                self.job = None;
            }
            Decision::OpenFolder(dir) => {
                sys::open_folder(&dir);
            }
            Decision::RunBuild(script, to_usb) => {
                let dir = script.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                let progress_file = dir.join("progress.txt");
                let cancel_file = dir.join("cancel.flag");
                let _ = std::fs::remove_file(&progress_file);
                let _ = std::fs::remove_file(&cancel_file);
                // YES уже введено в WinForge: окну сборки спрашивать нечего.
                let mut args = format!(
                    "-ProgressFile \"{}\" -CancelFile \"{}\"",
                    progress_file.display(),
                    cancel_file.display()
                );
                if to_usb {
                    args.push_str(" -Confirmed");
                }
                match sys::run_script_hidden_as_admin(&script, &args) {
                    Some(process) => {
                        self.usb_confirm.clear();
                        self.job = Some(JobState::Building(BuildRun {
                            process,
                            progress_file,
                            cancel_file,
                            to_usb,
                            started: std::time::Instant::now(),
                            progress: Default::default(),
                            last_read: None,
                            exit: None,
                            cancel_requested: false,
                        }));
                    }
                    None => self.notify(ctx, self.tr("Запуск скасовано", "Launch cancelled")),
                }
            }
            Decision::CancelBuild => {
                if let Some(JobState::Building(run)) = &mut self.job {
                    if std::fs::write(&run.cancel_file, b"cancel").is_ok() {
                        run.cancel_requested = true;
                    }
                }
            }
        }
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if matches!(&self.toast, Some((_, until)) if now > *until) {
            self.toast = None;
        }
        let Some((message, _)) = &self.toast else {
            return;
        };

        egui::Area::new(egui::Id::new("toast"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-24.0, -76.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(36, 41, 52))
                    .stroke(Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.6)))
                    .corner_radius(10.0)
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.label(RichText::new(format!("✔  {message}")).color(TEXT));
                    });
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

impl eframe::App for WinForgeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Виджеты читают язык из контекста egui.
        i18n::set_current(ctx, self.lang);
        self.poll_tasks(ctx);

        self.sidebar(ctx);
        self.bottom_bar(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin {
                left: 32,
                right: 20,
                top: 24,
                bottom: 0,
            }))
            .show(ctx, |ui| {
                let prefix = self.tr(
                    "Помилка в каталозі, використано вбудовану версію",
                    "Catalog error, using the built-in version",
                );
                for error in &self.catalog_errors {
                    widgets::banner(ui, "⚠", &format!("{prefix} — {error}"), DANGER);
                    ui.add_space(8.0);
                }
                egui::ScrollArea::vertical()
                    .id_salt(self.tab.index())
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(2.0);
                        match self.tab {
                            Tab::Home => self.ui_home(ui),
                            Tab::Image => self.ui_image(ui),
                            Tab::Apps => self.ui_apps(ui),
                            Tab::Privacy => self.ui_privacy(ui),
                            Tab::System => self.ui_system(ui),
                            Tab::Removal => self.ui_removal(ui),
                            Tab::Build => self.ui_build(ui),
                            Tab::Apply => self.ui_apply(ui),
                            Tab::Activation => self.ui_activation(ui),
                            Tab::Cleanup => self.ui_cleanup(ui),
                            Tab::Registry => self.ui_registry(ui),
                            Tab::Startup => self.ui_startup(ui),
                            Tab::Network => self.ui_network(ui),
                            Tab::Hosts => self.ui_hosts(ui),
                            Tab::Integrator => self.ui_integrator(ui),
                            Tab::DiskMonitor => self.ui_disk_monitor(ui),
                            Tab::EnvVars => self.ui_env_vars(ui),
                            Tab::Hardware => self.ui_hardware(ui),
                            Tab::Backups => self.ui_backups(ui),
                        }
                        ui.add_space(24.0);
                    });
            });

        self.confirm_modal(ctx);
        self.job_modal(ctx);
        self.toast_ui(ctx);
    }
}
