//! Вкладка «Контекстное меню»: пункты правого клика и свои команды для Win+R.

use eframe::egui::{self, RichText, Ui};

use crate::app::{JobKind, Section, WinForgeApp};
use crate::theme::*;
use crate::widgets;

impl WinForgeApp {
    pub fn ui_integrator(&mut self, ui: &mut Ui) {
        let l = self.lang;
        self.tweak_page(
            ui,
            Section::Integrator,
            "🖱",
            l.tr("Контекстне меню", "Context menu"),
            l.tr(
                "Корисні пункти в меню правого кліку та власні команди для вікна «Виконати» (Win+R).",
                "Handy right-click menu items and your own commands for the Run dialog (Win+R).",
            ),
        );

        let mut remove: Option<usize> = None;
        let mut add = false;

        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("Власні команди «Виконати»", "Custom Run commands"));
            ui.label(
                RichText::new(l.tr(
                    "Набери коротке слово в Win+R — і відкриється потрібна програма чи папка.",
                    "Type a short word in Win+R to open a program or folder.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(8.0);
            egui::Grid::new("run_commands")
                .num_columns(3)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(l.tr("Команда", "Command")).color(MUTED));
                    ui.label(RichText::new(l.tr("Що відкривати", "What to open")).color(MUTED));
                    ui.label("");
                    ui.end_row();

                    for (i, (alias, target)) in self.tools.run_commands.iter_mut().enumerate() {
                        // Точный размер: иначе в Grid поля сжимаются до ширины колонки.
                        ui.add_sized([140.0, 26.0], egui::TextEdit::singleline(alias));
                        ui.add_sized([420.0, 26.0], egui::TextEdit::singleline(target));
                        if ui
                            .small_button("✖")
                            .on_hover_text(l.tr("Видалити", "Remove"))
                            .clicked()
                        {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }

                    ui.add_sized(
                        [140.0, 26.0],
                        egui::TextEdit::singleline(&mut self.tools.new_alias).hint_text(l.tr("напр. ps", "e.g. ps")),
                    );
                    ui.add_sized(
                        [420.0, 26.0],
                        egui::TextEdit::singleline(&mut self.tools.new_target).hint_text("powershell.exe"),
                    );
                    if ui.button(l.tr("➕ Додати", "➕ Add")).clicked() {
                        add = true;
                    }
                    ui.end_row();
                });
        });
        ui.add_space(14.0);

        let apply = widgets::big_button(
            ui,
            l.tr("🖱   Застосувати до контекстного меню", "🖱   Apply to the context menu"),
            ACCENT,
        )
        .clicked();

        if let Some(i) = remove {
            self.tools.run_commands.remove(i);
        }
        if add {
            let alias = self.tools.new_alias.trim().to_owned();
            let target = self.tools.new_target.trim().to_owned();
            if alias.is_empty() || target.is_empty() {
                self.notify(
                    ui.ctx(),
                    l.tr("Заповни обидва поля: команду і що відкривати", "Fill in both the command and what to open"),
                );
            } else {
                self.tools.run_commands.push((alias, target));
                self.tools.new_alias.clear();
                self.tools.new_target.clear();
            }
        }
        if apply {
            self.start_job(ui.ctx(), JobKind::ContextMenu, true);
        }
    }
}
