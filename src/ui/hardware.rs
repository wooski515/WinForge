//! Вкладка «Оборудование»: реальные сведения (sysinfo + CIM), загружаются в фоне.

use eframe::egui::{self, RichText, Ui};

use crate::app::WinForgeApp;
use crate::sys::probe;
use crate::theme::*;
use crate::widgets;

impl WinForgeApp {
    fn load_hardware(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(probe::hardware());
        });
        self.tools.hw = None;
        self.tools.hw_rx = Some(rx);
    }

    pub fn ui_hardware(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "🖥",
            l.tr("Обладнання", "Hardware"),
            l.tr(
                "Коротко про залізо — зручно перед перевстановленням, щоб знати, які драйвери шукати.",
                "A quick look at your hardware — handy before a reinstall to know which drivers to get.",
            ),
        );

        if self.tools.hw.is_none() && self.tools.hw_rx.is_none() {
            self.load_hardware();
        }
        if let Some(rx) = &self.tools.hw_rx {
            match rx.try_recv() {
                Ok(data) => {
                    self.tools.hw = Some(data);
                    self.tools.hw_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint(),
                Err(_) => self.tools.hw_rx = None,
            }
        }

        let mut copy = false;
        let mut refresh = false;
        let loading = self.tools.hw_rx.is_some();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!loading, egui::Button::new(l.tr("📋 Копіювати все", "📋 Copy all")).min_size(egui::vec2(0.0, 32.0)))
                .clicked()
            {
                copy = true;
            }
            if ui
                .add_enabled(!loading, egui::Button::new(l.tr("↻ Оновити", "↻ Refresh")).min_size(egui::vec2(0.0, 32.0)))
                .clicked()
            {
                refresh = true;
            }
            if loading {
                ui.spinner();
                ui.label(RichText::new(l.tr("Збираємо дані…", "Collecting data…")).color(MUTED));
            }
        });
        ui.add_space(12.0);

        if let Some(sections) = &self.tools.hw {
            ui.columns(2, |cols| {
                for (i, section) in sections.iter().enumerate() {
                    let rows: Vec<(&str, String)> = section
                        .rows
                        .iter()
                        .map(|(label, uk, en)| (l.pick(*label), l.tr(uk, en).to_owned()))
                        .collect();
                    let col = &mut cols[i % 2];
                    widgets::info_card(col, section.icon, l.pick(section.title), &rows);
                    col.add_space(14.0);
                }
            });

            if copy {
                let mut text = String::new();
                for section in sections {
                    text.push_str(l.pick(section.title));
                    text.push('\n');
                    for (label, uk, en) in &section.rows {
                        text.push_str(&format!("  {}: {}\n", l.pick(*label), l.tr(uk, en)));
                    }
                }
                ui.ctx().copy_text(text);
                self.notify(ui.ctx(), l.tr("Скопійовано в буфер обміну", "Copied to clipboard"));
            }
        }
        if refresh {
            self.load_hardware();
        }
    }
}
