//! Вкладка «Реестр»: разблокировка системных инструментов и типичные исправления.

use eframe::egui::Ui;

use crate::app::{JobKind, Section, WinForgeApp};
use crate::theme::*;
use crate::widgets;

impl WinForgeApp {
    pub fn ui_registry(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "🔧",
            l.tr("Реєстр і відновлення", "Registry and repairs"),
            l.tr(
                "Повертає заблоковані інструменти та виправляє типові проблеми Windows без перевстановлення.",
                "Unblocks system tools and fixes common Windows problems without a reinstall.",
            ),
        );
        let toolbar = self.section_toolbar(ui, Section::Registry);
        ui.add_space(14.0);

        self.card_groups(ui, Section::Registry, 240.0, None);
        self.apply_toolbar(ui, Section::Registry, toolbar);

        let count = self.count_enabled(Section::Registry).0;
        let mut run = false;
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("Застосування", "Apply"));
            ui.add_space(4.0);
            if widgets::setting_row(
                ui,
                l.tr("Перезапустити Провідник після виправлення", "Restart Explorer afterwards"),
                l.tr(
                    "Деякі зміни підхопляться лише після перезапуску",
                    "Some changes only take effect after a restart",
                ),
                self.tools.restart_explorer,
                None,
                false,
            )
            .clicked()
            {
                self.tools.restart_explorer ^= true;
            }
            ui.add_space(10.0);
            let label = format!("🔧   {} ({count})", l.tr("Виправити вибране", "Fix selected"));
            ui.add_enabled_ui(count > 0, |ui| {
                if widgets::big_button(ui, &label, ACCENT).clicked() {
                    run = true;
                }
            });
        });

        if run {
            self.start_job(ui.ctx(), JobKind::RegistryFix, true);
        }
    }
}
