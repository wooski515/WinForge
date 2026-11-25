use eframe::egui::{self, RichText, Ui};

use crate::app::{localized, WinForgeApp, EDITIONS, KEYBOARDS, LANGUAGES, TIMEZONES};
use crate::catalog::Risk;
use crate::sys::probe;
use crate::theme::*;
use crate::widgets;

impl WinForgeApp {
    pub fn ui_image(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "💿",
            l.tr("Образ Windows", "Windows image"),
            l.tr(
                "Обери офіційний ISO від Microsoft і налаштуй, як пройде встановлення.",
                "Pick an official ISO from Microsoft and set up how installation goes.",
            ),
        );

        let mut browse_iso = false;
        let mut browse_drivers = false;
        self.poll_iso_scan(ui);
        let iso_path = self.image.iso_path.trim().to_owned();
        let iso_exists = !iso_path.is_empty() && std::path::Path::new(&iso_path).is_file();
        // Новый путь — сканируем редакции в фоне.
        if iso_exists
            && self.tools.iso_info.as_ref().map(|(p, _)| p != &iso_path).unwrap_or(true)
            && self.tools.iso_scan.as_ref().map(|(p, _)| p != &iso_path).unwrap_or(true)
        {
            self.start_iso_scan(iso_path.clone());
        }
        let scanning = self.tools.iso_scan.is_some();
        let edition_name = EDITIONS[self.image.edition].1;

        // 1. Вихідний образ
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("1. Вихідний образ", "1. Source image"));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let width = ui.available_width() - 120.0;
                ui.add(
                    egui::TextEdit::singleline(&mut self.image.iso_path)
                        .hint_text(l.tr(
                            r"Наприклад: D:\ISO\Win11_25H2_x64.iso",
                            r"For example: D:\ISO\Win11_25H2_x64.iso",
                        ))
                        .desired_width(width),
                );
                if ui.button(l.tr("📂 Огляд…", "📂 Browse…")).clicked() {
                    browse_iso = true;
                }
            });
            if iso_path.is_empty() {
                ui.label(
                    RichText::new(l.tr(
                        "Образ не вибрано. Офіційний ISO: microsoft.com/software-download/windows11",
                        "No image selected. Official ISO: microsoft.com/software-download/windows11",
                    ))
                    .size(13.0)
                    .color(MUTED),
                );
            } else if !iso_exists {
                ui.label(
                    RichText::new(l.tr("⚠  Файл не знайдено", "⚠  File not found"))
                        .size(13.0)
                        .color(DANGER),
                );
            } else if scanning {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new(l.tr("Читаємо редакції в образі…", "Reading editions in the image…"))
                            .size(13.0)
                            .color(MUTED),
                    );
                });
            } else if let Some((_, result)) = &self.tools.iso_info {
                match result {
                    Ok(names) => {
                        let text = match l {
                            crate::i18n::Lang::Uk => format!("✔  Редакцій в образі: {}", names.len()),
                            crate::i18n::Lang::En => format!("✔  Editions in the image: {}", names.len()),
                        };
                        ui.label(RichText::new(text).size(13.0).color(SAFE))
                            .on_hover_text(names.join("\n"));
                        if !names.iter().any(|n| n == edition_name) {
                            let warn = match l {
                                crate::i18n::Lang::Uk => format!(
                                    "⚠  У цьому ISO немає редакції «{edition_name}». Є: {}",
                                    names.join(", ")
                                ),
                                crate::i18n::Lang::En => format!(
                                    "⚠  This ISO has no \"{edition_name}\" edition. Available: {}",
                                    names.join(", ")
                                ),
                            };
                            ui.label(RichText::new(warn).size(12.5).color(MODERATE));
                        }
                    }
                    Err(e) => {
                        ui.label(
                            RichText::new(format!("⚠  {}: {e}", l.tr("Не вдалося прочитати ISO", "Could not read the ISO")))
                                .size(13.0)
                                .color(DANGER),
                        );
                    }
                }
            }
            ui.add_space(10.0);
            egui::Grid::new("image_grid")
                .num_columns(2)
                .spacing([28.0, 10.0])
                .min_col_width(180.0)
                .show(ui, |ui| {
                    widgets::combo_row(
                        ui,
                        l.tr("Редакція", "Edition"),
                        "edition",
                        &mut self.image.edition,
                        &localized(EDITIONS, l),
                    );
                    widgets::combo_row(
                        ui,
                        l.tr("Мова системи", "System language"),
                        "language",
                        &mut self.image.language,
                        &localized(LANGUAGES, l),
                    );
                    widgets::combo_row(
                        ui,
                        l.tr("Розкладки клавіатури", "Keyboard layouts"),
                        "keyboard",
                        &mut self.image.keyboard,
                        &localized(KEYBOARDS, l),
                    );
                    widgets::combo_row(
                        ui,
                        l.tr("Часовий пояс", "Time zone"),
                        "timezone",
                        &mut self.image.timezone,
                        &localized(TIMEZONES, l),
                    );
                });
        });
        ui.add_space(14.0);

        // 2. Обліковий запис
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("2. Обліковий запис", "2. Account"));
            ui.add_space(4.0);
            egui::Grid::new("account_grid")
                .num_columns(2)
                .spacing([28.0, 10.0])
                .min_col_width(180.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(l.tr("Ім'я користувача", "User name")).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.image.username).desired_width(340.0));
                    ui.end_row();

                    ui.label(RichText::new(l.tr("Пароль", "Password")).color(MUTED));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.image.password)
                            .password(true)
                            .hint_text(l.tr("можна залишити порожнім", "can be left empty"))
                            .desired_width(340.0),
                    );
                    ui.end_row();

                    ui.label(RichText::new(l.tr("Ім'я комп'ютера", "Computer name")).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.image.computer_name).desired_width(340.0));
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            if widgets::setting_row(
                ui,
                l.tr("Локальний обліковий запис", "Local account"),
                l.tr(
                    "Без входу в обліковий запис Microsoft і без обов'язкового інтернету",
                    "No Microsoft account sign-in and no mandatory internet",
                ),
                self.image.local_account,
                None,
                true,
            )
            .clicked()
            {
                self.image.local_account ^= true;
            }
            if widgets::setting_row(
                ui,
                l.tr("Автоматичний вхід", "Automatic sign-in"),
                l.tr(
                    "Не питати пароль під час увімкнення комп'ютера",
                    "Don't ask for a password when the PC starts",
                ),
                self.image.auto_login,
                None,
                false,
            )
            .clicked()
            {
                self.image.auto_login ^= true;
            }
        });
        ui.add_space(14.0);

        // 3. Встановлення
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("3. Встановлення", "3. Installation"));
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            if widgets::setting_row(
                ui,
                l.tr(
                    "Пропустити екрани початкового налаштування (OOBE)",
                    "Skip the first-run setup screens (OOBE)",
                ),
                l.tr(
                    "Регіон, розкладка й конфіденційність візьмуться з профілю",
                    "Region, keyboard and privacy come from the profile",
                ),
                self.image.skip_oobe,
                Some(Risk::Safe),
                true,
            )
            .clicked()
            {
                self.image.skip_oobe ^= true;
            }
            if widgets::setting_row(
                ui,
                l.tr(
                    "Обійти перевірку TPM 2.0, Secure Boot та обсягу ОЗП",
                    "Bypass TPM 2.0, Secure Boot and RAM checks",
                ),
                l.tr(
                    "Дозволяє встановити Windows 11 на старі комп'ютери",
                    "Lets you install Windows 11 on older PCs",
                ),
                self.image.bypass_tpm,
                Some(Risk::Moderate),
                true,
            )
            .clicked()
            {
                self.image.bypass_tpm ^= true;
            }
            if widgets::setting_row(
                ui,
                l.tr(
                    "Не вимагати інтернет під час встановлення",
                    "Don't require internet during setup",
                ),
                l.tr(
                    "Можна ставити без мережі, драйвери й програми підтягнуться пізніше",
                    "Install offline; drivers and apps arrive later",
                ),
                self.image.no_network,
                Some(Risk::Safe),
                true,
            )
            .clicked()
            {
                self.image.no_network ^= true;
            }
            if widgets::setting_row(
                ui,
                l.tr(
                    "Стиснути систему (CompactOS)",
                    "Compress the system (CompactOS)",
                ),
                l.tr(
                    "Windows займе на кілька ГБ менше — для малих SSD. Установка триває на кілька хвилин довше",
                    "Windows takes a few GB less — for small SSDs. Setup takes a few minutes longer",
                ),
                self.image.compact_os,
                Some(Risk::Safe),
                true,
            )
            .clicked()
            {
                self.image.compact_os ^= true;
            }
            if widgets::setting_row(
                ui,
                l.tr(
                    "Встановити доповнення для віртуальної машини",
                    "Install virtual machine guest tools",
                ),
                l.tr(
                    "VirtualBox або VMware: спільний буфер обміну. Їхній диск підключи другим приводом",
                    "VirtualBox or VMware: shared clipboard. Attach their disc as a second drive",
                ),
                self.image.guest_tools,
                Some(Risk::Safe),
                true,
            )
            .clicked()
            {
                self.image.guest_tools ^= true;
            }
            if widgets::setting_row(
                ui,
                l.tr(
                    "Видалити тимчасові файли після збірки",
                    "Delete temporary files after the build",
                ),
                l.tr(
                    "Розпакований образ займає 10–15 ГБ",
                    "The unpacked image takes 10–15 GB",
                ),
                self.image.cleanup,
                Some(Risk::Safe),
                false,
            )
            .clicked()
            {
                self.image.cleanup ^= true;
            }
        });
        ui.add_space(14.0);

        // 4. Драйвери
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("4. Драйвери (необов'язково)", "4. Drivers (optional)"));
            ui.label(
                RichText::new(l.tr(
                    "Драйвери з папки буде вбудовано в образ: Wi-Fi і мережа запрацюють одразу після встановлення.",
                    "Drivers from the folder are built into the image: Wi-Fi and network work right after installation.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let width = ui.available_width() - 120.0;
                ui.add(
                    egui::TextEdit::singleline(&mut self.image.drivers_path)
                        .hint_text(l.tr(r"Наприклад: D:\Drivers", r"For example: D:\Drivers"))
                        .desired_width(width),
                );
                if ui.button(l.tr("📂 Огляд…", "📂 Browse…")).clicked() {
                    browse_drivers = true;
                }
            });
            ui.add_space(6.0);
            if widgets::setting_row(
                ui,
                l.tr("Додати драйвери цього ПК", "Add this PC's drivers"),
                l.tr(
                    "Відеокарта, Wi-Fi, чипсет — для перевстановлення на цьому ж комп'ютері. Образ більший на 0,5–4 ГБ (з драйвером відеокарти NVIDIA — до 3–4 ГБ)",
                    "Graphics, Wi-Fi, chipset: for reinstalling on this same computer. The image grows by 0.5–4 GB (up to 3–4 GB with an NVIDIA graphics driver)",
                ),
                self.image.export_drivers,
                Some(Risk::Safe),
                false,
            )
            .on_hover_text(l.tr(
                "Під час збірки WinForge вивантажить сторонні драйвери цієї системи (Export-WindowsDriver). Драйвери контролерів дисків потраплять в установник, решта встановиться всередині нової Windows на етапі налаштування. Вбудовані драйвери Windows не потрібні — вони й так є в образі.",
                "During the build WinForge exports this system's third-party drivers (Export-WindowsDriver). Disk controller drivers go into Setup; the rest install inside the new Windows during setup. Windows' own drivers are already in the image.",
            ))
            .clicked()
            {
                self.image.export_drivers ^= true;
            }
        });
        ui.add_space(14.0);

        // 5. Wi-Fi
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("5. Wi-Fi (необов'язково)", "5. Wi-Fi (optional)"));
            ui.label(
                RichText::new(l.tr(
                    "Після встановлення система сама підключиться до цієї мережі — програми почнуть ставитися одразу.",
                    "After installation the system joins this network on its own, so apps start installing right away.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(6.0);
            egui::Grid::new("wifi_grid")
                .num_columns(2)
                .spacing([18.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(l.tr("Назва мережі", "Network name")).color(MUTED));
                    ui.add_sized(
                        [260.0, 26.0],
                        egui::TextEdit::singleline(&mut self.image.wifi_ssid)
                            .hint_text(l.tr("порожньо — не налаштовувати", "empty — don't set up")),
                    );
                    ui.end_row();
                    ui.label(RichText::new(l.tr("Пароль", "Password")).color(MUTED));
                    ui.add_sized(
                        [260.0, 26.0],
                        egui::TextEdit::singleline(&mut self.image.wifi_password)
                            .password(true)
                            .hint_text(l.tr("порожньо — відкрита мережа", "empty — open network")),
                    );
                    ui.end_row();
                });
            if !self.image.wifi_ssid.trim().is_empty() && !self.image.wifi_password.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(l.tr(
                        "⚠  Пароль Wi-Fi лежатиме відкритим текстом у autounattend.xml на флешці, доки її не перезаписати. У новій системі він стирається одразу після підключення.",
                        "⚠  The Wi-Fi password sits as plain text in autounattend.xml on the USB drive until you rewrite it. In the new system it is erased right after connecting.",
                    ))
                    .size(12.5)
                    .color(MODERATE),
                );
            }
        });
        ui.add_space(14.0);

        // 6. Активация
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("6. Активація (необов'язково)", "6. Activation (optional)"));
            ui.label(
                RichText::new(l.tr(
                    "Твій ключ продукту та/або KMS-сервер твоєї організації. Windows активується сама, щойно з'явиться зв'язок.",
                    "Your product key and/or your organization's KMS server. Windows activates on its own once it is online.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(6.0);
            egui::Grid::new("activation_grid")
                .num_columns(2)
                .spacing([18.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(l.tr("Ключ продукту", "Product key")).color(MUTED));
                    ui.add_sized(
                        [320.0, 26.0],
                        egui::TextEdit::singleline(&mut self.image.product_key)
                            .password(true)
                            .hint_text("XXXXX-XXXXX-XXXXX-XXXXX-XXXXX"),
                    );
                    ui.end_row();
                    ui.label(RichText::new(l.tr("KMS-сервер", "KMS server")).color(MUTED));
                    ui.add_sized(
                        [320.0, 26.0],
                        egui::TextEdit::singleline(&mut self.image.kms_host)
                            .hint_text(l.tr("порожньо — не задавати", "empty — don't set")),
                    );
                    ui.end_row();
                });
            let key = self.image.product_key.trim();
            let host = self.image.kms_host.trim();
            ui.add_space(4.0);
            if !key.is_empty() && crate::sys::activation::normalize_key(key).is_none() {
                ui.label(
                    RichText::new(l.tr(
                        "⚠  Ключ має вигляд XXXXX-XXXXX-XXXXX-XXXXX-XXXXX — у такому вигляді його не буде додано.",
                        "⚠  A key looks like XXXXX-XXXXX-XXXXX-XXXXX-XXXXX — as typed it won't be added.",
                    ))
                    .size(12.5)
                    .color(MODERATE),
                );
            } else if !key.is_empty() {
                ui.label(
                    RichText::new(l.tr(
                        "Ключ має підходити до вибраної редакції. На флешці він лежить у autounattend.xml, а в новій системі стирається одразу після встановлення ключа. У профіль не зберігається.",
                        "The key must match the chosen edition. It sits in autounattend.xml on the USB drive and is erased in the new system right after it is installed. It is not saved to the profile.",
                    ))
                    .size(12.5)
                    .color(MUTED),
                );
            }
            if !host.is_empty() && crate::sys::activation::parse_kms_host(host).is_none() {
                ui.label(
                    RichText::new(l.tr(
                        "⚠  Адреса KMS-сервера: ім'я або IP, за потреби з портом, наприклад kms.company.local:1688.",
                        "⚠  KMS server address: a name or IP, optionally with a port, e.g. kms.company.local:1688.",
                    ))
                    .size(12.5)
                    .color(MODERATE),
                );
            }
        });

        if browse_iso {
            if let Some(file) = rfd::FileDialog::new()
                .set_title(l.tr("Вибери ISO-образ Windows", "Pick a Windows ISO image"))
                .add_filter("ISO", &["iso"])
                .pick_file()
            {
                self.image.iso_path = file.display().to_string();
            }
        }
        if browse_drivers {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title(l.tr("Папка з драйверами (.inf)", "Folder with drivers (.inf)"))
                .pick_folder()
            {
                self.image.drivers_path = dir.display().to_string();
            }
        }
    }

    fn start_iso_scan(&mut self, path: String) {
        let (tx, rx) = std::sync::mpsc::channel();
        let target = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(probe::iso_editions(&target));
        });
        self.tools.iso_scan = Some((path, rx));
    }

    fn poll_iso_scan(&mut self, ui: &Ui) {
        let Some((path, rx)) = &self.tools.iso_scan else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                self.tools.iso_info = Some((path.clone(), result));
                self.tools.iso_scan = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint(),
            Err(_) => self.tools.iso_scan = None,
        }
    }
}
