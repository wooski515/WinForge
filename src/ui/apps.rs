use eframe::egui::{self, Align, FontId, Layout, RichText, Ui};

use crate::app::WinForgeApp;
use crate::catalog::AppEntry;
use crate::i18n::Lang;
use crate::theme::{self, *};
use crate::tools::{CustomApp, CustomKind};
use crate::widgets::{self, GridItem};

/// Размер текстового поля в таблицах формы.
const FIELD: [f32; 2] = [420.0, 26.0];

impl WinForgeApp {
    pub fn ui_apps(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "📦",
            l.tr("Програми", "Apps"),
            l.tr(
                "Позначені програми встановляться самі через winget під час першого входу — завжди останніх версій.",
                "Checked apps install automatically via winget on first sign-in — always the latest versions.",
            ),
        );

        self.install_location_card(ui);
        ui.add_space(14.0);
        self.custom_apps_card(ui);
        ui.add_space(18.0);

        // (ключ пресета, название)
        let mut preset: Option<(&str, &str)> = None;
        let mut clear_all = false;

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text(l.tr("🔍  Пошук програм…", "🔍  Search apps…"))
                    .desired_width(240.0),
            );
            ui.add_space(14.0);
            ui.label(RichText::new(l.tr("Пресети:", "Presets:")).color(MUTED));
            if ui.button(l.tr("📦 Базовий", "📦 Basic")).clicked() {
                preset = Some(("base", l.tr("Базовий", "Basic")));
            }
            if ui.button(l.tr("🎮 Геймер", "🎮 Gamer")).clicked() {
                preset = Some(("gamer", l.tr("Геймер", "Gamer")));
            }
            if ui.button(l.tr("💻 Розробник", "💻 Developer")).clicked() {
                preset = Some(("dev", l.tr("Розробник", "Developer")));
            }
            if ui.button(l.tr("✖ Зняти все", "✖ Clear all")).clicked() {
                clear_all = true;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{}: {}", l.tr("Вибрано", "Selected"), self.selected_apps.len()))
                        .strong()
                        .color(TEXT),
                );
            });
        });
        ui.add_space(16.0);

        let query = self.search.trim().to_lowercase();
        let matches = |a: &AppEntry| {
            query.is_empty()
                || a.name.contains_lower(&query)
                || a.description.contains_lower(&query)
                || a.winget.to_lowercase().contains(&query)
        };

        let mut toggled: Vec<String> = Vec::new();
        let mut bulk: Option<(Vec<String>, bool)> = None;
        let mut found_any = false;

        for category in &self.cat.apps.category {
            let apps: Vec<&AppEntry> = category.app.iter().filter(|a| matches(a)).collect();
            if apps.is_empty() {
                continue;
            }
            found_any = true;
            let ids: Vec<String> = apps.iter().map(|a| a.id.clone()).collect();
            let chosen = apps
                .iter()
                .filter(|a| self.selected_apps.contains(&a.id))
                .count();

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{}  {}", category.icon, category.name.get(l)))
                        .font(FontId::new(17.0, theme::bold()))
                        .color(TEXT),
                );
                ui.label(RichText::new(format!("{chosen}/{}", apps.len())).color(MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(l.tr("Зняти", "Clear")).clicked() {
                        bulk = Some((ids.clone(), false));
                    }
                    if ui.small_button(l.tr("Вибрати всі", "Select all")).clicked() {
                        bulk = Some((ids.clone(), true));
                    }
                });
            });
            ui.add_space(4.0);

            let items = apps.iter().map(|a| GridItem {
                id: &a.id,
                title: a.name.get(l),
                subtitle: a.description.get(l),
                selected: self.selected_apps.contains(&a.id),
                risk: None,
                hover: Some(Box::new(move |ui: &mut Ui| app_tooltip(ui, a, l))),
            });
            toggled.extend(widgets::card_grid(ui, 220.0, items));
            ui.add_space(18.0);
        }

        if !found_any {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(l.tr("Нічого не знайдено", "Nothing found")).size(18.0).color(MUTED));
                ui.label(
                    RichText::new(l.tr(
                        "Додай програму вище, у «Мої програми», за її ID з winget",
                        "Add it above under \"My apps\" using its winget ID",
                    ))
                    .size(13.0)
                    .color(MUTED),
                );
            });
        }

        // Применяем изменения после отрисовки (каталог больше не заимствован).
        for id in toggled {
            self.toggle_app(&id);
        }
        if let Some((ids, select)) = bulk {
            for id in ids {
                if select {
                    self.selected_apps.insert(id);
                } else {
                    self.selected_apps.remove(&id);
                }
            }
        }
        if clear_all {
            self.selected_apps.clear();
            self.notify(ui.ctx(), l.tr("Вибір програм скинуто", "App selection cleared"));
        }
        if let Some((key, name)) = preset {
            let added = self.apply_preset(key);
            let msg = match l {
                Lang::Uk => format!("Пресет «{name}»: додано програм — {added}"),
                Lang::En => format!("Preset \"{name}\": {added} apps added"),
            };
            self.notify(ui.ctx(), msg);
        }
    }

    /// Карточка «Куда устанавливать»: диск, папка программ и папка установщиков.
    fn install_location_card(&mut self, ui: &mut Ui) {
        let l = self.lang;
        let mut pick_drive: Option<usize> = None;
        let mut browse_folder = false;
        let mut browse_downloads = false;

        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("💾 Куди встановлювати програми", "💾 Where to install apps"));
            ui.label(
                RichText::new(l.tr(
                    "Обери диск — шлях підставиться сам. Деякі інсталятори (драйвери, Visual C++) завжди ставляться на системний диск.",
                    "Pick a drive and the path fills in. Some installers (drivers, Visual C++) always go to the system drive.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(8.0);

            let drives = &self.tools.drives;
            let ids: Vec<String> = drives.iter().map(|d| format!("drive-{}", d.letter)).collect();
            let titles: Vec<String> = drives
                .iter()
                .map(|d| {
                    let label = if d.system {
                        l.tr("Системний", "System").to_owned()
                    } else if d.label.trim().is_empty() {
                        l.tr("Локальний диск", "Local disk").to_owned()
                    } else {
                        d.label.clone()
                    };
                    format!("{}  {label}", d.letter)
                })
                .collect();
            let subtitles: Vec<String> = drives
                .iter()
                .map(|d| {
                    let kind = if d.is_hdd { "HDD" } else { "SSD" };
                    match l {
                        Lang::Uk => format!("{kind} · вільно {} з {} ГБ", d.free_gb, d.total_gb),
                        Lang::En => format!("{kind} · {} of {} GB free", d.free_gb, d.total_gb),
                    }
                })
                .collect();
            let current = self.install.drive.min(drives.len().saturating_sub(1));
            let items = drives.iter().enumerate().map(|(i, _)| GridItem {
                id: &ids[i],
                title: &titles[i],
                subtitle: &subtitles[i],
                selected: current == i,
                risk: None,
                hover: None,
            });
            for id in widgets::card_grid(ui, 260.0, items) {
                pick_drive = ids.iter().position(|x| *x == id);
            }

            if drives.get(current).map(|d| d.is_hdd).unwrap_or(false) {
                ui.label(
                    RichText::new(l.tr(
                        "⚠ Це HDD: програми й ігри запускатимуться повільніше, ніж з SSD.",
                        "⚠ This is a hard drive: apps and games will load slower than from an SSD.",
                    ))
                    .size(12.5)
                    .color(MODERATE),
                );
            }
            ui.add_space(8.0);

            egui::Grid::new("install_paths")
                .num_columns(3)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(l.tr("Папка програм", "Apps folder")).color(MUTED));
                    // В средней колонке Grid поле нужно задавать точным размером,
                    // иначе egui берёт ширину колонки с прошлого кадра и поле сжимается.
                    ui.add_sized(FIELD, egui::TextEdit::singleline(&mut self.install.folder));
                    if ui.button(l.tr("📂 Огляд…", "📂 Browse…")).clicked() {
                        browse_folder = true;
                    }
                    ui.end_row();

                    ui.label(RichText::new(l.tr("Інсталятори зберігати в", "Save installers to")).color(MUTED));
                    ui.add_sized(FIELD, egui::TextEdit::singleline(&mut self.install.download_folder));
                    if ui.button(l.tr("📂 Огляд…", "📂 Browse…")).clicked() {
                        browse_downloads = true;
                    }
                    ui.end_row();
                });
            ui.add_space(4.0);
            if widgets::setting_row(
                ui,
                l.tr("Видаляти інсталятори після встановлення", "Delete installers after installing"),
                l.tr("Не займатимуть місце на диску", "They won't take up disk space"),
                self.install.delete_installers,
                None,
                false,
            )
            .clicked()
            {
                self.install.delete_installers ^= true;
            }
        });

        if let Some(i) = pick_drive {
            if let Some(drive) = self.tools.drives.get(i) {
                self.install.drive = i;
                self.install.folder = drive.default_folder();
                self.install.download_folder = drive.download_folder();
            }
        }
        if browse_folder {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title(l.tr("Папка для програм", "Folder for apps"))
                .pick_folder()
            {
                self.install.folder = dir.display().to_string();
            }
        }
        if browse_downloads {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title(l.tr("Папка для інсталяторів", "Folder for installers"))
                .pick_folder()
            {
                self.install.download_folder = dir.display().to_string();
            }
        }
    }

    /// Карточка «Мои программы»: свои пакеты winget или установщики.
    fn custom_apps_card(&mut self, ui: &mut Ui) {
        let l = self.lang;
        let mut toggled: Option<String> = None;
        let mut remove: Option<usize> = None;
        let mut add = false;
        let mut browse = false;
        let mut add_folder = false;

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::section_title(ui, l.tr("⭐ Мої програми", "⭐ My apps"));
                ui.label(RichText::new(format!("{}", self.install.custom.len())).color(MUTED));
            });
            ui.label(
                RichText::new(l.tr(
                    "Додай програму, якої немає в списку: за ID з winget, свій інсталятор (.exe / .msi) або цілу папку з інсталяторами. Для образу вони запишуться на флешку/ISO і встановляться без інтернету. Усе зберігається в профілі.",
                    "Add an app that isn't listed: by its winget ID, your own installer (.exe / .msi) or a whole folder of installers. For the image they go onto the USB drive/ISO and install without internet. Everything is saved in your profile.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(8.0);

            if self.install.custom.is_empty() {
                ui.label(
                    RichText::new(l.tr("Поки порожньо.", "Nothing here yet."))
                        .italics()
                        .color(MUTED),
                );
            } else {
                let missing_args = self
                    .install
                    .custom
                    .iter()
                    .filter(|a| a.kind == CustomKind::Installer && a.args.trim().is_empty())
                    .count();
                // Своя прокрутка только для длинного списка (папка на десятки файлов).
                let rows = self.install.custom.len() as f32;
                egui::ScrollArea::vertical()
                    .id_salt("custom_apps_scroll")
                    .min_scrolled_height((rows * 37.0).min(320.0))
                    .max_height(320.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                    egui::Grid::new("custom_apps")
                        .num_columns(5)
                        .spacing([14.0, 8.0])
                        .show(ui, |ui| {
                            for (i, app) in self.install.custom.iter_mut().enumerate() {
                                let mut selected = self.selected_apps.contains(&app.id);
                                if ui.checkbox(&mut selected, "").changed() {
                                    toggled = Some(app.id.clone());
                                }
                                ui.label(RichText::new(&app.name).strong().color(TEXT));
                                match app.kind {
                                    CustomKind::Winget => {
                                        ui.label(
                                            RichText::new(format!("winget: {}", app.value))
                                                .monospace()
                                                .size(12.0)
                                                .color(MUTED),
                                        );
                                        ui.label("");
                                    }
                                    CustomKind::Installer => {
                                        // Коротко имя файла; полный путь — в подсказке.
                                        let short = std::path::Path::new(&app.value)
                                            .file_name()
                                            .filter(|_| !crate::sys::installer::is_url(&app.value))
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_else(|| app.value.clone());
                                        ui.label(RichText::new(short).monospace().size(12.0).color(MUTED))
                                            .on_hover_text(&app.value);
                                        let empty = app.args.trim().is_empty();
                                        // Точный размер: в ячейке таблицы поле иначе сжимается.
                                        ui.add_sized(
                                            [240.0, 22.0],
                                            egui::TextEdit::singleline(&mut app.args)
                                                .hint_text(l.tr("тихі ключі, напр. /S", "silent switches, e.g. /S"))
                                                .text_color(if empty { MODERATE } else { TEXT }),
                                        )
                                        .on_hover_text(l.tr(
                                            "Ключі тихої установки, щоб інсталятор не показував вікон. WinForge підбирає їх сам за типом інсталятора.",
                                            "Silent install switches so the installer shows no windows. WinForge picks them by installer type.",
                                        ));
                                    }
                                }
                                if ui
                                    .small_button("✖")
                                    .on_hover_text(l.tr("Видалити", "Remove"))
                                    .clicked()
                                {
                                    remove = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                });
                if missing_args > 0 {
                    let text = match l {
                        Lang::Uk => format!(
                            "⚠ Для {missing_args} інсталяторів тихі ключі невідомі: впиши їх, інакше інсталятор чекатиме на кліки."
                        ),
                        Lang::En => format!(
                            "⚠ Silent switches are unknown for {missing_args} installers: fill them in, or the installer will wait for clicks."
                        ),
                    };
                    ui.label(RichText::new(text).size(12.5).color(MODERATE));
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(l.tr("Додати:", "Add:")).color(MUTED));
                ui.selectable_value(&mut self.install.form_kind, CustomKind::Winget, l.tr("з winget", "from winget"));
                ui.selectable_value(
                    &mut self.install.form_kind,
                    CustomKind::Installer,
                    l.tr("свій інсталятор", "own installer"),
                );
            });
            ui.add_space(6.0);
            egui::Grid::new("custom_form")
                .num_columns(3)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(l.tr("Назва", "Name")).color(MUTED));
                    ui.add_sized(
                        FIELD,
                        egui::TextEdit::singleline(&mut self.install.form_name)
                            .hint_text(l.tr("напр. Minecraft Launcher", "e.g. Minecraft Launcher")),
                    );
                    ui.label("");
                    ui.end_row();

                    match self.install.form_kind {
                        CustomKind::Winget => {
                            ui.label(RichText::new(l.tr("ID у winget", "winget ID")).color(MUTED));
                            ui.add_sized(
                                FIELD,
                                egui::TextEdit::singleline(&mut self.install.form_value)
                                    .hint_text("Mojang.MinecraftLauncher"),
                            );
                            ui.label(
                                RichText::new(l.tr("знайти: winget search <назва>", "find it: winget search <name>"))
                                    .monospace()
                                    .size(12.0)
                                    .color(MUTED),
                            );
                            ui.end_row();
                        }
                        CustomKind::Installer => {
                            ui.label(RichText::new(l.tr("Файл або посилання", "File or link")).color(MUTED));
                            ui.add_sized(
                                FIELD,
                                egui::TextEdit::singleline(&mut self.install.form_value)
                                    .hint_text(r"D:\Installers\setup.exe  /  https://…"),
                            );
                            if ui.button(l.tr("📂 Огляд…", "📂 Browse…")).clicked() {
                                browse = true;
                            }
                            ui.end_row();

                            ui.label(RichText::new(l.tr("Тиха установка", "Silent install")).color(MUTED));
                            ui.add_sized(
                                FIELD,
                                egui::TextEdit::singleline(&mut self.install.form_args)
                                    .hint_text("/S   /quiet   /VERYSILENT"),
                            );
                            ui.label(
                                RichText::new(l.tr("параметри без вікон", "no-window switches"))
                                    .size(12.0)
                                    .color(MUTED),
                            );
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if widgets::primary_button(ui, l.tr("➕ Додати програму", "➕ Add app")).clicked() {
                    add = true;
                }
                if widgets::secondary_button(ui, l.tr("📁 Додати папку з інсталяторами", "📁 Add a folder of installers"))
                    .on_hover_text(l.tr(
                        "Знайде всі .exe та .msi у папці (і в підпапках) і сам підбере тихі ключі.",
                        "Finds every .exe and .msi in the folder (and subfolders) and picks the silent switches.",
                    ))
                    .clicked()
                {
                    add_folder = true;
                }
            });
        });

        if let Some(id) = toggled {
            self.toggle_app(&id);
        }
        if let Some(i) = remove {
            let app = self.install.custom.remove(i);
            self.selected_apps.remove(&app.id);
        }
        if browse {
            if let Some(file) = rfd::FileDialog::new()
                .set_title(l.tr("Вибери інсталятор", "Pick an installer"))
                .add_filter(l.tr("Інсталятори", "Installers"), &["exe", "msi"])
                .pick_file()
            {
                self.install.form_value = file.display().to_string();
                if self.install.form_name.trim().is_empty() {
                    if let Some(stem) = file.file_stem() {
                        self.install.form_name = stem.to_string_lossy().into_owned();
                    }
                }
            }
        }
        if add {
            let msg = self.add_custom_app();
            self.notify(ui.ctx(), msg);
        }
        if add_folder {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title(l.tr("Вибери папку з інсталяторами", "Pick a folder of installers"))
                .pick_folder()
            {
                let msg = self.add_installer_folder(&dir);
                self.notify(ui.ctx(), msg);
            }
        }
    }

    /// Добавляет все установщики из папки с подобранными тихими ключами.
    fn add_installer_folder(&mut self, dir: &std::path::Path) -> String {
        let l = self.lang;
        let found = crate::sys::installer::scan_folder(dir);
        if found.is_empty() {
            return l.tr("У папці немає .exe чи .msi", "No .exe or .msi in this folder").into();
        }
        let (mut added, mut unknown, mut skipped) = (0, 0, 0);
        for f in found {
            let path = f.path.display().to_string();
            if self.install.custom.iter().any(|c| c.value.eq_ignore_ascii_case(&path)) {
                skipped += 1;
                continue;
            }
            if f.kind == crate::sys::installer::InstallerKind::Unknown {
                unknown += 1;
            }
            let id = format!("my-{}", self.install.next_custom_id);
            self.install.next_custom_id += 1;
            self.install.custom.push(CustomApp {
                id: id.clone(),
                name: f.name,
                kind: CustomKind::Installer,
                value: path,
                args: f.kind.silent_args().to_owned(),
            });
            self.selected_apps.insert(id);
            added += 1;
        }
        let mut msg = match l {
            Lang::Uk => format!("Додано інсталяторів: {added}"),
            Lang::En => format!("Installers added: {added}"),
        };
        if unknown > 0 {
            msg.push_str(&match l {
                Lang::Uk => format!(", без відомих тихих ключів: {unknown}"),
                Lang::En => format!(", without known silent switches: {unknown}"),
            });
        }
        if skipped > 0 {
            msg.push_str(&match l {
                Lang::Uk => format!(", вже були в списку: {skipped}"),
                Lang::En => format!(", already listed: {skipped}"),
            });
        }
        msg
    }

    /// Проверяет форму и добавляет свою программу. Возвращает текст уведомления.
    fn add_custom_app(&mut self) -> String {
        let l = self.lang;
        let name = self.install.form_name.trim().to_owned();
        let value = self.install.form_value.trim().to_owned();
        let kind = self.install.form_kind;

        if value.is_empty() {
            return match kind {
                CustomKind::Winget => l.tr("Вкажи ID програми в winget", "Enter the app's winget ID"),
                CustomKind::Installer => l.tr("Вкажи файл або посилання", "Enter a file or a link"),
            }
            .into();
        }
        if kind == CustomKind::Winget {
            if value.contains(char::is_whitespace) {
                return l.tr("ID у winget пишеться без пробілів", "A winget ID has no spaces").into();
            }
            // Уже есть в каталоге? Тогда просто отмечаем её.
            let existing = self
                .cat
                .apps
                .category
                .iter()
                .flat_map(|c| c.app.iter())
                .find(|a| a.winget.eq_ignore_ascii_case(&value))
                .map(|a| (a.id.clone(), a.name.get(l).to_owned()));
            if let Some((id, existing_name)) = existing {
                self.selected_apps.insert(id);
                self.install.form_value.clear();
                self.install.form_name.clear();
                return match l {
                    Lang::Uk => format!("«{existing_name}» вже є в каталозі — відмічено"),
                    Lang::En => format!("\"{existing_name}\" is already in the catalog — checked it"),
                };
            }
        }

        let name = if name.is_empty() { value.clone() } else { name };
        let id = format!("my-{}", self.install.next_custom_id);
        self.install.next_custom_id += 1;
        self.install.custom.push(CustomApp {
            id: id.clone(),
            name: name.clone(),
            kind,
            value,
            args: self.install.form_args.trim().to_owned(),
        });
        self.selected_apps.insert(id);
        self.install.form_name.clear();
        self.install.form_value.clear();
        self.install.form_args.clear();
        match l {
            Lang::Uk => format!("Додано «{name}»"),
            Lang::En => format!("Added \"{name}\""),
        }
    }
}

fn app_tooltip(ui: &mut Ui, app: &AppEntry, l: Lang) {
    ui.label(RichText::new(app.name.get(l)).strong().color(TEXT));
    ui.label(RichText::new(app.description.get(l)).color(MUTED));
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("winget install --id {}", app.winget))
            .monospace()
            .size(12.0)
            .color(ACCENT),
    );
}
