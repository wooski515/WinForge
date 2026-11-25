use eframe::egui::{self, RichText, Ui};

use crate::app::{JobKind, OutputKind, Section, Tab, WinForgeApp, EDITIONS, KEYBOARDS, LANGUAGES};
use crate::i18n::Lang;
use crate::sys::probe;
use crate::theme::*;
use crate::widgets::{self, GridItem};

/// Минимальный размер флешки для установки Windows 11. Флешка «на 8 ГБ» — это
/// 8·10⁹ байт, то есть около 7,45 ГБ в счёте Windows, поэтому порог чуть ниже 8.
const MIN_USB_GB: f64 = 7.2;

impl WinForgeApp {
    fn load_usb_list(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(probe::usb_disks());
        });
        self.tools.usb_rx = Some(rx);
    }

    /// Выбор флешки: только съёмные USB-диски, с предупреждением о стирании.
    fn usb_picker(&mut self, ui: &mut Ui) {
        let l = self.lang;
        if self.tools.usb_list.is_none() && self.tools.usb_rx.is_none() {
            self.load_usb_list();
        }
        if let Some(rx) = &self.tools.usb_rx {
            match rx.try_recv() {
                Ok(list) => {
                    // Выбранная флешка исчезла (вынули) — сбрасываем выбор.
                    if let Some(sel) = &self.image.usb {
                        if !list.iter().any(|d| d.number == sel.number && d.serial == sel.serial && d.size == sel.size) {
                            self.image.usb = None;
                        }
                    }
                    self.tools.usb_list = Some(list);
                    self.tools.usb_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint(),
                Err(_) => self.tools.usb_rx = None,
            }
        }

        let list = self.tools.usb_list.clone().unwrap_or_default();
        let loading = self.tools.usb_rx.is_some();
        let mut refresh = false;
        let mut picked: Option<probe::UsbDisk> = None;

        if list.is_empty() {
            ui.label(
                RichText::new(if loading {
                    l.tr("Шукаємо флешки…", "Looking for USB drives…")
                } else {
                    l.tr(
                        "Флешок не знайдено. Встав флешку від 8 ГБ і натисни «Оновити».",
                        "No USB drives found. Plug in an 8 GB+ drive and press Refresh.",
                    )
                })
                .size(13.0)
                .color(MUTED),
            );
        } else {
            let ids: Vec<String> = list.iter().map(|d| format!("usb-{}", d.number)).collect();
            let titles: Vec<String> = list
                .iter()
                .map(|d| {
                    let letters = if d.letters.is_empty() { String::new() } else { format!("{}  ", d.letters) };
                    format!("{letters}{}", d.name)
                })
                .collect();
            let subtitles: Vec<String> = list
                .iter()
                .map(|d| {
                    let gb = d.size_gb();
                    match (l, gb < MIN_USB_GB) {
                        (Lang::Uk, false) => format!("{gb:.1} ГБ · диск {}", d.number),
                        (Lang::En, false) => format!("{gb:.1} GB · disk {}", d.number),
                        (Lang::Uk, true) => format!("{gb:.1} ГБ — замало, потрібно від 8 ГБ"),
                        (Lang::En, true) => format!("{gb:.1} GB — too small, needs 8 GB+"),
                    }
                })
                .collect();
            let selected = self.image.usb.as_ref().map(|u| u.number);
            let items = list.iter().enumerate().map(|(i, d)| GridItem {
                id: &ids[i],
                title: &titles[i],
                subtitle: &subtitles[i],
                selected: selected == Some(d.number),
                risk: None,
                hover: None,
            });
            for id in widgets::card_grid(ui, 220.0, items) {
                if let Some(d) = list.iter().find(|d| format!("usb-{}", d.number) == id) {
                    if d.size_gb() >= MIN_USB_GB {
                        picked = Some(d.clone());
                    }
                }
            }
        }

        ui.horizontal(|ui| {
            if ui.add_enabled(!loading, egui::Button::new(l.tr("↻ Оновити", "↻ Refresh"))).clicked() {
                refresh = true;
            }
            if loading {
                ui.spinner();
            }
        });
        if let Some(sel) = &self.image.usb {
            let text = match l {
                Lang::Uk => format!(
                    "⚠ Усі дані на «{}» ({:.1} ГБ) буде стерто. Перед записом ще раз спитаю.",
                    sel.name,
                    sel.size_gb()
                ),
                Lang::En => format!(
                    "⚠ All data on \"{}\" ({:.1} GB) will be erased. You'll be asked again before writing.",
                    sel.name,
                    sel.size_gb()
                ),
            };
            ui.label(RichText::new(text).size(12.5).color(DANGER));
        }

        if let Some(d) = picked {
            self.image.usb = Some(d);
        }
        if refresh {
            self.tools.usb_list = None;
            self.load_usb_list();
        }
    }

    fn plan_lines(&self) -> Vec<String> {
        let l = self.lang;
        let img = &self.image;
        let user = if img.username.trim().is_empty() { "User" } else { img.username.trim() };
        let account = if img.local_account {
            l.tr("локальний обліковий запис", "local account")
        } else {
            l.tr("обліковий запис Microsoft", "Microsoft account")
        };

        let mut lines = vec![
            match self.iso_file_name() {
                Some(name) => format!("{}: {name}", l.tr("Вихідний образ", "Source image")),
                None => l.tr("Вихідний образ: не вибрано", "Source image: not selected").into(),
            },
            format!(
                "{}, {}: {}",
                l.pick(EDITIONS[img.edition]),
                l.tr("мова", "language"),
                l.pick(LANGUAGES[img.language])
            ),
            format!("{}: {}", l.tr("Розкладки", "Keyboard"), l.pick(KEYBOARDS[img.keyboard])),
            match l {
                Lang::Uk => format!("Користувач «{user}», {account}"),
                Lang::En => format!("User \"{user}\", {account}"),
            },
        ];
        if img.skip_oobe {
            lines.push(l.tr("Пропуск екранів початкового налаштування", "Skip first-run setup screens").into());
        }
        if img.bypass_tpm {
            lines.push(l.tr("Обхід перевірки TPM / Secure Boot / ОЗП", "Bypass TPM / Secure Boot / RAM checks").into());
        }
        lines.push(format!(
            "{}: {}",
            l.tr("Видалити вбудованих застосунків", "Built-in apps to remove"),
            self.count_enabled(Section::Removal).0
        ));
        lines.push(format!(
            "{}: {}",
            l.tr("Твіки приватності", "Privacy tweaks"),
            self.count_enabled(Section::Privacy).0
        ));
        lines.push(format!(
            "{}: {}",
            l.tr("Оптимізації системи", "System optimizations"),
            self.count_enabled(Section::System).0
        ));
        if !img.drivers_path.trim().is_empty() {
            lines.push(format!(
                "{} {}",
                l.tr("Вбудувати драйвери з", "Include drivers from"),
                img.drivers_path.trim()
            ));
        }
        if img.export_drivers {
            lines.push(l.tr("Вбудувати драйвери цього ПК", "Include this PC's drivers").into());
        }
        if !img.wifi_ssid.trim().is_empty() {
            lines.push(format!("{}: {}", l.tr("Підключитися до Wi-Fi", "Join Wi-Fi"), img.wifi_ssid.trim()));
        }
        if img.compact_os {
            lines.push(l.tr("Стиснути систему (CompactOS)", "Compress the system (CompactOS)").into());
        }
        if img.guest_tools {
            lines.push(l.tr("Доповнення для віртуальної машини", "Virtual machine guest tools").into());
        }
        if crate::sys::activation::normalize_key(&img.product_key).is_some() {
            lines.push(l.tr("Встановити свій ключ продукту", "Install your product key").into());
        }
        if let Some((host, _)) = crate::sys::activation::parse_kms_host(&img.kms_host) {
            lines.push(format!("{}: {host}", l.tr("KMS-сервер організації", "Organization KMS server")));
        }
        let (custom, _) = self.selected_custom();
        let mut apps_line = format!(
            "{}: {}",
            l.tr("Встановити програм", "Apps to install"),
            self.selected_apps.len()
        );
        if custom > 0 {
            apps_line.push_str(&match l {
                Lang::Uk => format!(" (з них власних: {custom})"),
                Lang::En => format!(" ({custom} of them your own)"),
            });
        }
        lines.push(apps_line);
        lines.push(format!(
            "{}: {}",
            l.tr("Папка програм", "Apps folder"),
            self.install.folder
        ));
        lines.push(match (self.image.output, &self.image.usb) {
            (OutputKind::Usb, Some(u)) => format!(
                "{}: {} ({:.1} {})",
                l.tr("Записати на флешку", "Write to USB drive"),
                u.name,
                u.size_gb(),
                l.tr("ГБ", "GB")
            ),
            (OutputKind::Usb, None) => l.tr("Флешку ще не вибрано", "No USB drive selected yet").into(),
            (OutputKind::Iso, _) => format!("{}: {}", l.tr("ISO-файл", "ISO file"), self.image.output_path.trim()),
        });
        lines
    }

    pub fn ui_build(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "🔨",
            l.tr("Збірка", "Build"),
            l.tr(
                "Перевір підсумковий план і запусти збірку образу.",
                "Review the final plan and build the image.",
            ),
        );

        let mut go_to: Option<Tab> = None;
        if self.iso_file_name().is_none() {
            widgets::banner(
                ui,
                "⚠",
                l.tr(
                    "Вихідний ISO не вибрано (вкладка «Образ Windows»). Файли збірки підготувати можна, але запустити збірку — ні.",
                    "No source ISO selected (Windows image tab). You can prepare the build files, but not run the build.",
                ),
                MODERATE,
            );
            ui.add_space(14.0);
        }

        let stats = [
            (self.selected_apps.len(), l.tr("програм", "apps"), Tab::Apps, ACCENT),
            (
                self.count_enabled(Section::Privacy).0,
                l.tr("твіків приватності", "privacy tweaks"),
                Tab::Privacy,
                SAFE,
            ),
            (
                self.count_enabled(Section::System).0,
                l.tr("оптимізацій", "optimizations"),
                Tab::System,
                MODERATE,
            ),
            (
                self.count_enabled(Section::Removal).0,
                l.tr("застосунків до видалення", "apps to remove"),
                Tab::Removal,
                DANGER,
            ),
        ];
        ui.columns(stats.len(), |cols| {
            for (col, (value, label, tab, color)) in cols.iter_mut().zip(stats) {
                if widgets::stat_tile(col, value, label, color).clicked() {
                    go_to = Some(tab);
                }
            }
        });
        ui.add_space(16.0);

        let plan = self.plan_lines();
        let mut start: Option<JobKind> = None;

        ui.columns(2, |cols| {
            widgets::card(&mut cols[0], |ui| {
                widgets::section_title(ui, l.tr("Підсумковий план", "Final plan"));
                ui.add_space(4.0);
                for line in &plan {
                    ui.label(RichText::new(format!("✔   {line}")).color(TEXT));
                }
            });

            widgets::card(&mut cols[1], |ui| {
                widgets::section_title(ui, l.tr("Куди записати", "Where to write"));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.image.output, OutputKind::Usb, l.tr("🔌  Флешка", "🔌  USB drive"));
                    ui.selectable_value(
                        &mut self.image.output,
                        OutputKind::Iso,
                        l.tr("💿  ISO-файл (для віртуалки)", "💿  ISO file (for a VM)"),
                    );
                });
                ui.add_space(6.0);
                let mut browse = false;
                if self.image.output == OutputKind::Usb {
                    self.usb_picker(ui);
                } else {
                    ui.horizontal(|ui| {
                        let width = ui.available_width() - 110.0;
                        ui.add_sized(
                            [width, 28.0],
                            egui::TextEdit::singleline(&mut self.image.output_path),
                        );
                        if ui.button(l.tr("📂 Огляд…", "📂 Browse…")).clicked() {
                            browse = true;
                        }
                    });
                    ui.label(
                        RichText::new(l.tr(
                            "Завантажувальний ISO (BIOS + UEFI) збирається вбудованими засобами Windows.",
                            "The bootable ISO (BIOS + UEFI) is built with tools that come with Windows.",
                        ))
                        .size(12.5)
                        .color(MUTED),
                    );
                }
                if browse {
                    if let Some(file) = rfd::FileDialog::new()
                        .set_title(l.tr("Куди зберегти ISO", "Where to save the ISO"))
                        .set_file_name("WinForge_Windows.iso")
                        .add_filter("ISO", &["iso"])
                        .save_file()
                    {
                        self.image.output_path = file.display().to_string();
                    }
                }

                // Робоча папка: розпакований образ займає ~40 ГБ.
                ui.add_space(10.0);
                ui.label(
                    RichText::new(l.tr("Робоча папка збірки", "Build work folder"))
                        .strong()
                        .color(TEXT),
                );
                let mut browse_work = false;
                ui.horizontal(|ui| {
                    let width = ui.available_width() - 110.0;
                    ui.add_sized([width, 28.0], egui::TextEdit::singleline(&mut self.image.work_dir));
                    if ui.button(l.tr("📂 Огляд…", "📂 Browse…")).clicked() {
                        browse_work = true;
                    }
                });
                let need = crate::sys::image::BUILD_SPACE_GB;
                let letter: String = self.image.work_dir.trim().chars().take(2).collect::<String>().to_uppercase();
                let free = self.tools.drives.iter().find(|d| d.letter == letter).map(|d| d.free_gb);
                let (text, color) = match free {
                    Some(gb) if gb >= need => (
                        match l {
                            Lang::Uk => format!("Вільно на {letter} {gb} ГБ — вистачить (потрібно ~{need} ГБ)"),
                            Lang::En => format!("{gb} GB free on {letter} — enough (needs ~{need} GB)"),
                        },
                        MUTED,
                    ),
                    Some(gb) => (
                        match l {
                            Lang::Uk => format!("⚠ Вільно на {letter} лише {gb} ГБ, а потрібно ~{need} ГБ — вибери інший диск"),
                            Lang::En => format!("⚠ Only {gb} GB free on {letter}, needs ~{need} GB — pick another drive"),
                        },
                        DANGER,
                    ),
                    None => (
                        match l {
                            Lang::Uk => format!("Потрібно ~{need} ГБ вільного місця"),
                            Lang::En => format!("Needs ~{need} GB of free space"),
                        },
                        MUTED,
                    ),
                };
                ui.label(RichText::new(text).size(12.5).color(color));
                if browse_work {
                    if let Some(dir) = rfd::FileDialog::new()
                        .set_title(l.tr("Робоча папка збірки", "Build work folder"))
                        .pick_folder()
                    {
                        self.image.work_dir = dir.display().to_string();
                    }
                }
                ui.add_space(16.0);
                let build_label = match self.image.output {
                    OutputKind::Usb => l.tr("🔌   Підготувати запис на флешку", "🔌   Prepare the USB drive"),
                    OutputKind::Iso => l.tr("💿   Підготувати збірку ISO", "💿   Prepare the ISO build"),
                };
                if widgets::big_button(ui, build_label, ACCENT).clicked() {
                    start = Some(JobKind::BuildIso);
                }
            });
        });

        if let Some(tab) = go_to {
            self.tab = tab;
        }
        if let Some(kind) = start {
            self.start_job(ui.ctx(), kind, true);
        }
    }

    /// «Застосувати» (режим «Ця Windows»): те, что отмечено на шагах, — к этому ПК.
    pub fn ui_apply(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "⚡",
            l.tr("Застосувати до цієї Windows", "Apply to this Windows"),
            l.tr(
                "Програми, твіки й видалення з попередніх кроків — для системи, яка вже стоїть.",
                "Apps, tweaks and removals from the previous steps — for the system that is already installed.",
            ),
        );
        if !crate::sys::is_elevated() {
            widgets::banner(
                ui,
                "⚠",
                l.tr(
                    "WinForge запущено без прав адміністратора: системні твіки застосувати не вийде. Перезапуск — на кроці перевірки.",
                    "WinForge runs without administrator rights: system tweaks can't be applied. You can restart it at the review step.",
                ),
                MODERATE,
            );
            ui.add_space(14.0);
        }

        self.reapply_card(ui);

        let mut go_to: Option<Tab> = None;
        let stats = [
            (self.selected_apps.len(), l.tr("програм", "apps"), Tab::Apps, ACCENT),
            (self.count_enabled(Section::Privacy).0, l.tr("твіків приватності", "privacy tweaks"), Tab::Privacy, SAFE),
            (self.count_enabled(Section::System).0, l.tr("оптимізацій", "optimizations"), Tab::System, MODERATE),
            (self.count_enabled(Section::Removal).0, l.tr("до видалення", "to remove"), Tab::Removal, DANGER),
        ];
        ui.columns(stats.len(), |cols| {
            for (col, (value, label, tab, color)) in cols.iter_mut().zip(stats) {
                if widgets::stat_tile(col, value, label, color).clicked() {
                    go_to = Some(tab);
                }
            }
        });
        ui.add_space(16.0);

        let mut start = false;
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("Як це працює", "How it works"));
            ui.add_space(4.0);
            for line in [
                l.tr(
                    "1. Спершу WinForge лише перевірить систему й покаже, що саме зміниться.",
                    "1. First WinForge only checks the system and shows exactly what will change.",
                ),
                l.tr(
                    "2. Перед змінами створить точку відновлення Windows (можна вимкнути).",
                    "2. Before any change it creates a Windows restore point (you can turn this off).",
                ),
                l.tr(
                    "3. Кожна зміна записується в журнал — її можна відкотити на вкладці «Відкат змін».",
                    "3. Every change goes to a journal — undo it on the Undo changes tab.",
                ),
            ] {
                ui.label(RichText::new(line).color(TEXT));
            }
            ui.add_space(10.0);
            if widgets::big_button(ui, l.tr("🔍   Перевірити й застосувати", "🔍   Review and apply"), ACCENT).clicked() {
                start = true;
            }
        });

        ui.add_space(14.0);
        let mut update_apps = false;
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("🔄 Оновити всі програми", "🔄 Update all apps"));
            ui.label(
                RichText::new(l.tr(
                    "Усі програми, які знає winget, оновляться до нових версій. Відкриті програми краще закрити. Програми з Microsoft Store оновлює сам Store.",
                    "Every app winget knows about is updated to its latest version. Close open apps first. Microsoft Store apps are updated by the Store itself.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(6.0);
            if widgets::secondary_button(ui, l.tr("🔄 Оновити програми", "🔄 Update apps")).clicked() {
                update_apps = true;
            }
        });

        if let Some(tab) = go_to {
            self.tab = tab;
        }
        if start {
            self.start_job(ui.ctx(), JobKind::ApplyLive, true);
        }
        if update_apps {
            // Без предпросмотра: показывать нечего, а откат обновлений невозможен.
            self.start_job(ui.ctx(), JobKind::UpdateApps, false);
        }
    }

    /// Карточка «Windows оновилася»: после смены сборки предлагает снова выбрать
    /// применённое раньше и посмотреть в предпросмотре, что вернулось.
    pub fn reapply_card(&mut self, ui: &mut Ui) {
        if !self.reapply_due {
            return;
        }
        let known = self.applied_known_ids().len();
        if known == 0 {
            return;
        }
        let l = self.lang;
        let mut check = false;
        let mut dismiss = false;
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("🔁 Windows оновилася", "🔁 Windows has been updated"));
            ui.label(
                RichText::new(match l {
                    crate::i18n::Lang::Uk => format!(
                        "Збірка {} → {}. Великі оновлення інколи повертають рекламу, телеметрію й видалені програми. WinForge вибере лише застосоване раніше (пунктів: {}; решту вибору буде знято), і перевірить, що з них знову діє, а що ні, — нічого не застосує без тебе.",
                        self.applied.build, self.windows_build, known
                    ),
                    crate::i18n::Lang::En => format!(
                        "Build {} → {}. Big updates sometimes bring back ads, telemetry and removed apps. WinForge will select only what you applied before ({} items; the rest of the selection is cleared) and check which of them still work; nothing is applied without you.",
                        self.applied.build, self.windows_build, known
                    ),
                })
                .size(13.0)
                .color(TEXT),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if widgets::primary_button(ui, l.tr("🔍 Перевірити, що повернулося", "🔍 Check what came back")).clicked() {
                    check = true;
                }
                if widgets::secondary_button(ui, l.tr("Не нагадувати", "Don't remind me")).clicked() {
                    dismiss = true;
                }
            });
        });
        ui.add_space(14.0);
        if check {
            self.reselect_applied();
            // Выбор уже точный: советы по железу не должны снять из него пункты молча.
            self.hw_checked = true;
            self.set_mode(crate::app::Mode::Live);
            self.tab = Tab::Apply;
            self.start_job(ui.ctx(), JobKind::ApplyLive, true);
        }
        if dismiss {
            self.dismiss_reapply();
        }
    }
}
