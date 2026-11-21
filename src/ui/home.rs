use eframe::egui::{FontId, RichText, Ui};

use crate::app::{Mode, Section, Tab, WinForgeApp};
use crate::theme::{self, *};
use crate::widgets;

impl WinForgeApp {
    pub fn ui_home(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "👋",
            l.tr("Ласкаво просимо до WinForge", "Welcome to WinForge"),
            l.tr(
                "Збери свою Windows: без сміття й телеметрії, з потрібними програмами та налаштуваннями.",
                "Build your own Windows: no bloat, no telemetry, with the apps and settings you need.",
            ),
        );
        widgets::banner(
            ui,
            "ℹ",
            l.tr(
                "Перед будь-якою зміною WinForge показує, що саме зміниться, і зберігає резервну копію. Повернути все можна на вкладці «Відкат змін».",
                "Before changing anything WinForge shows exactly what will change and saves a backup. You can undo it all on the Undo changes tab.",
            ),
            ACCENT,
        );
        if !crate::sys::is_elevated() {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(l.tr(
                        "WinForge запущено без прав адміністратора: системні твіки, DNS і hosts застосувати не вийде.",
                        "WinForge is running without administrator rights: system tweaks, DNS and hosts can't be applied.",
                    ))
                    .size(13.0)
                    .color(MODERATE),
                );
                if ui
                    .button(l.tr("🛡 Перезапустити від адміністратора", "🛡 Restart as administrator"))
                    .clicked()
                    && crate::sys::relaunch_as_admin()
                {
                    ui.ctx().send_viewport_cmd(eframe::egui::ViewportCommand::Close);
                }
            });
        }
        ui.add_space(18.0);

        let mut go_to: Option<Tab> = None;
        let mut mode: Option<Mode> = None;
        let mut open_profile = false;

        ui.columns(3, |cols| {
            if widgets::action_card(
                &mut cols[0],
                "💿",
                l.tr("Створити свою збірку", "Create your own build"),
                l.tr(
                    "Візьми офіційний ISO від Microsoft, прибери зайве, додай програми й отримай готовий інсталяційний образ.",
                    "Take an official ISO from Microsoft, strip the junk, add your apps and get a ready-to-install image.",
                ),
                l.tr("Почати", "Start"),
            )
            .clicked()
            {
                mode = Some(Mode::Build);
                go_to = Some(Tab::Image);
            }
            if widgets::action_card(
                &mut cols[1],
                "⚡",
                l.tr("Оптимізувати цю систему", "Optimize this system"),
                l.tr(
                    "Застосуй ті самі твіки приватності та продуктивності до вже встановленої Windows.",
                    "Apply the same privacy and performance tweaks to the Windows you already have.",
                ),
                l.tr("Налаштувати", "Tune it"),
            )
            .clicked()
            {
                mode = Some(Mode::Live);
                go_to = Some(Tab::Privacy);
            }
            if widgets::action_card(
                &mut cols[2],
                "📂",
                l.tr("Відкрити профіль", "Open a profile"),
                l.tr(
                    "Завантаж свій торішній профіль або профіль друга — усі галочки стануть як були.",
                    "Load last year's profile or a friend's — every checkbox comes back as it was.",
                ),
                l.tr("Відкрити", "Open"),
            )
            .clicked()
            {
                open_profile = true;
            }
        });

        ui.add_space(22.0);
        widgets::section_title(ui, l.tr("Поточний профіль", "Current profile"));
        ui.add_space(4.0);
        let stats = [
            (
                self.selected_apps.len(),
                l.tr("програм буде встановлено", "apps to install"),
                Tab::Apps,
                ACCENT,
            ),
            (
                self.count_enabled(Section::Privacy).0,
                l.tr("твіків приватності", "privacy tweaks"),
                Tab::Privacy,
                SAFE,
            ),
            (
                self.count_enabled(Section::System).0,
                l.tr("оптимізацій системи", "system optimizations"),
                Tab::System,
                MODERATE,
            ),
            (
                self.count_enabled(Section::Removal).0,
                l.tr("застосунків буде видалено", "apps to remove"),
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

        ui.add_space(22.0);
        widgets::section_title(ui, l.tr("Як це працює", "How it works"));
        ui.add_space(4.0);
        let steps = [
            (
                "1",
                l.tr("Образ", "Image"),
                l.tr("Обери офіційний ISO та редакцію", "Pick an official ISO and edition"),
            ),
            (
                "2",
                l.tr("Програми", "Apps"),
                l.tr("Познач, що встановити через winget", "Choose what to install via winget"),
            ),
            (
                "3",
                l.tr("Твіки", "Tweaks"),
                l.tr("Вимкни телеметрію та зайві служби", "Turn off telemetry and extra services"),
            ),
            (
                "4",
                l.tr("Видалення", "Removal"),
                l.tr("Виріж непотрібні вбудовані застосунки", "Cut unwanted built-in apps"),
            ),
            (
                "5",
                l.tr("Збірка", "Build"),
                l.tr("Отримай ISO або завантажувальну флешку", "Get an ISO or a bootable USB drive"),
            ),
        ];
        ui.columns(steps.len(), |cols| {
            for (col, (num, title, desc)) in cols.iter_mut().zip(steps) {
                widgets::card(col, |ui| {
                    ui.label(
                        RichText::new(num)
                            .font(FontId::new(24.0, theme::bold()))
                            .color(ACCENT),
                    );
                    ui.label(RichText::new(title).strong().color(TEXT));
                    ui.label(RichText::new(desc).size(12.5).color(MUTED));
                });
            }
        });

        if let Some(mode) = mode {
            self.set_mode(mode);
        }
        if let Some(tab) = go_to {
            self.tab = tab;
        }
        if open_profile {
            if let Some(msg) = self.open_profile_dialog() {
                self.notify(ui.ctx(), msg);
            }
        }
    }
}
