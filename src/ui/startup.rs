//! Вкладка «Автозапуск»: реальные записи из реестра и папок автозагрузки.
//! Выключение — через StartupApproved, как в Диспетчере задач (обратимо).

use eframe::egui::{Align, Layout, RichText, Ui};

use crate::app::WinForgeApp;
use crate::sys::{self, probe};
use crate::theme::*;
use crate::widgets;

impl WinForgeApp {
    fn reload_startup(&mut self) {
        self.tools.startup_real = Some(probe::read_startup());
    }

    pub fn ui_startup(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "🚀",
            l.tr("Автозапуск", "Startup"),
            l.tr(
                "Програми, які стартують разом з Windows. Менше автозапуску — швидше вмикається комп'ютер.",
                "Programs that start with Windows. Less startup clutter means a faster boot.",
            ),
        );
        if self.tools.startup_real.is_none() {
            self.reload_startup();
        }
        let items = self.tools.startup_real.clone().unwrap_or_default();
        let enabled_count = items.iter().filter(|e| e.enabled).count();
        let elevated = sys::is_elevated();

        let mut refresh = false;
        ui.horizontal(|ui| {
            if widgets::secondary_button(ui, l.tr("↻ Оновити", "↻ Refresh")).clicked() {
                refresh = true;
            }
            ui.label(
                RichText::new(l.tr(
                    "Вимкнення нічого не видаляє — запис можна ввімкнути назад.",
                    "Disabling deletes nothing — you can turn an entry back on.",
                ))
                .size(12.5)
                .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "{}: {enabled_count} {} {}",
                        l.tr("Запускається", "Starting"),
                        l.tr("з", "of"),
                        items.len()
                    ))
                    .strong()
                    .color(TEXT),
                );
            });
        });
        ui.add_space(14.0);

        if items.is_empty() {
            widgets::card(ui, |ui| {
                ui.label(RichText::new(l.tr("Автозапуск порожній.", "Nothing starts automatically.")).color(MUTED));
            });
        }

        let mut toggled: Option<probe::StartupItem> = None;
        for loc in probe::StartupLoc::ALL {
            let entries: Vec<&probe::StartupItem> = items.iter().filter(|e| e.loc == loc).collect();
            if entries.is_empty() {
                continue;
            }
            widgets::card(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::section_title(ui, l.pick(loc.title()));
                    if loc.needs_admin() && !elevated {
                        widgets::pill(ui, l.tr("потрібен адміністратор", "admin needed"), MODERATE);
                    }
                });
                ui.label(RichText::new(loc.source()).monospace().size(12.0).color(MUTED));
                ui.add_space(6.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let count = entries.len();
                for (i, entry) in entries.iter().enumerate() {
                    let response = widgets::setting_row_badge(
                        ui,
                        &entry.name,
                        &entry.command,
                        entry.enabled,
                        None,
                        i + 1 < count,
                    )
                    .on_hover_ui(|ui| {
                        ui.label(RichText::new(&entry.name).strong().color(TEXT));
                        ui.add_space(4.0);
                        ui.label(RichText::new(&entry.command).monospace().size(12.0).color(TEXT));
                    });
                    if response.clicked() {
                        toggled = Some((*entry).clone());
                    }
                }
            });
            ui.add_space(14.0);
        }

        if let Some(item) = toggled {
            let want = !item.enabled;
            match probe::set_startup_enabled(&item, want) {
                Ok(()) => {
                    let msg = match (l, want) {
                        (crate::i18n::Lang::Uk, true) => format!("Увімкнено: {}", item.name),
                        (crate::i18n::Lang::Uk, false) => format!("Вимкнено: {}", item.name),
                        (crate::i18n::Lang::En, true) => format!("Enabled: {}", item.name),
                        (crate::i18n::Lang::En, false) => format!("Disabled: {}", item.name),
                    };
                    self.notify(ui.ctx(), msg);
                }
                Err(_) if item.loc.needs_admin() && !elevated => {
                    self.notify(
                        ui.ctx(),
                        l.tr(
                            "Для цього запису потрібні права адміністратора",
                            "This entry needs administrator rights",
                        ),
                    );
                }
                Err(e) => self.notify(ui.ctx(), format!("{}: {e}", l.tr("Помилка", "Error"))),
            }
            refresh = true;
        }
        if refresh {
            self.reload_startup();
        }
    }
}
