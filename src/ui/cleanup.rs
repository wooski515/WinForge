//! Вкладка «Очистка»: выбор, реальный анализ объёма и очистка.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, vec2, Align, Color32, FontId, Layout, RichText, Ui};

use crate::app::{JobKind, Section, WinForgeApp};
use crate::sys::clean;
use crate::theme::{self, *};
use crate::widgets;

impl WinForgeApp {
    /// Пары (id, пути) для отмеченных пунктов очистки.
    fn selected_clean_paths(&self) -> Vec<(String, Vec<String>)> {
        self.cat
            .cleaner
            .group
            .iter()
            .flat_map(|g| g.tweak.iter())
            .filter(|t| self.enabled.contains(&t.id))
            .map(|t| (t.id.clone(), t.paths.clone()))
            .collect()
    }

    /// Запускает фоновый расчёт реальных размеров.
    fn start_analysis(&mut self) {
        let items = self.selected_clean_paths();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result: Vec<(String, u64)> = items
                .into_iter()
                .map(|(id, paths)| {
                    let mb = clean::measure(&paths) / 1_048_576;
                    (id, mb)
                })
                .collect();
            let _ = tx.send(result);
        });
        self.tools.analysis = None;
        self.tools.analyze_rx = Some(rx);
    }

    pub fn ui_cleanup(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "✨",
            l.tr("Очищення диска", "Disk cleanup"),
            l.tr(
                "Тимчасові файли, кеш і сліди браузерів. Аналіз покаже реальний розмір.",
                "Temporary files, caches and browser traces. Analyze to see the real size.",
            ),
        );

        // Приём результата фонового анализа.
        if let Some(rx) = &self.tools.analyze_rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.tools.analysis = Some(result);
                    self.tools.analyze_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.ctx().request_repaint();
                }
                Err(_) => self.tools.analyze_rx = None,
            }
        }

        let selected = self.enabled_in(Section::Cleanup);
        let analyzing = self.tools.analyze_rx.is_some();
        let stale = match &self.tools.analysis {
            Some(items) => {
                let analyzed: HashSet<&String> = items.iter().map(|(id, _)| id).collect();
                analyzed.len() != selected.len() || selected.iter().any(|id| !analyzed.contains(id))
            }
            None => false,
        };
        let total = self.analysis_total();
        let has_result = self.tools.analysis.is_some() && !stale && !analyzing;

        let mut analyze = false;
        let mut clean = false;

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    if analyzing {
                        ui.label(
                            RichText::new(l.tr("Аналізуємо…", "Analyzing…"))
                                .font(FontId::new(22.0, theme::bold()))
                                .color(TEXT),
                        );
                        ui.add(egui::ProgressBar::new(0.5).desired_width(360.0).fill(ACCENT).animate(true));
                    } else if has_result {
                        ui.label(RichText::new(l.tr("Можна звільнити", "You can free up")).color(MUTED));
                        ui.label(
                            RichText::new(widgets::format_size(total, l))
                                .font(FontId::new(32.0, theme::bold()))
                                .color(SAFE),
                        );
                    } else if stale {
                        ui.label(
                            RichText::new(l.tr("Вибір змінився", "Selection changed"))
                                .font(FontId::new(20.0, theme::bold()))
                                .color(MODERATE),
                        );
                        ui.label(
                            RichText::new(l.tr(
                                "Запусти аналіз ще раз, щоб побачити новий розмір.",
                                "Run the analysis again to see the new size.",
                            ))
                            .color(MUTED),
                        );
                    } else {
                        ui.label(
                            RichText::new(l.tr("Ще не проаналізовано", "Not analyzed yet"))
                                .font(FontId::new(20.0, theme::bold()))
                                .color(TEXT),
                        );
                        ui.label(
                            RichText::new(l.tr(
                                "Познач, що чистити, і натисни «Аналіз».",
                                "Check what to clean and press Analyze.",
                            ))
                            .color(MUTED),
                        );
                    }
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let can_clean = !selected.is_empty() && !analyzing;
                    let clean_button = egui::Button::new(
                        RichText::new(l.tr("✨  Очистити", "✨  Clean")).color(Color32::WHITE).size(16.0),
                    )
                    .fill(ACCENT)
                    .min_size(vec2(150.0, 42.0));
                    if ui.add_enabled(can_clean, clean_button).clicked() {
                        clean = true;
                    }
                    let analyze_button =
                        egui::Button::new(RichText::new(l.tr("🔍  Аналіз", "🔍  Analyze")).size(16.0))
                            .min_size(vec2(130.0, 42.0));
                    if ui.add_enabled(!analyzing && !selected.is_empty(), analyze_button).clicked() {
                        analyze = true;
                    }
                });
            });
        });
        ui.add_space(14.0);

        let toolbar = self.section_toolbar(ui, Section::Cleanup);
        ui.add_space(14.0);

        let sizes: Option<HashMap<String, u64>> = if has_result {
            self.tools.analysis.as_ref().map(|items| items.iter().cloned().collect())
        } else {
            None
        };
        self.card_groups(ui, Section::Cleanup, 200.0, sizes.as_ref());
        self.apply_toolbar(ui, Section::Cleanup, toolbar);

        if analyze {
            self.start_analysis();
        }
        if clean {
            // Сначала окно со списком и размерами; удаление — только после «Очистити насправді».
            self.start_job(ui.ctx(), JobKind::Clean, true);
        }
    }
}
