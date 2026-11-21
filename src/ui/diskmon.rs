//! Вкладка «Монитор дисков»: реальная загрузка дисков, общий график
//! чтения/записи и процессы с наибольшим вводом-выводом.

use std::collections::VecDeque;

use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Shape, Stroke, Ui};

use crate::app::WinForgeApp;
use crate::i18n::Lang;
use crate::sys::probe::DiskSampler;
use crate::theme::*;
use crate::tools::{DISK_HISTORY, DISK_SAMPLE_INTERVAL};
use crate::widgets;

const READ_COLOR: Color32 = ACCENT;
const WRITE_COLOR: Color32 = MODERATE;

fn rate_text(mb_s: f32, l: Lang) -> String {
    format!("{mb_s:.1} {}", l.tr("МБ/с", "MB/s"))
}

impl WinForgeApp {
    /// Снимает новую точку раз в DISK_SAMPLE_INTERVAL секунд.
    fn update_disk_samples(&mut self, now: f64) {
        let state = &mut self.tools;
        let sampler = state.disk_sampler.get_or_insert_with(DiskSampler::new);
        if state.disk_paused {
            state.disk_last_sample = now;
            return;
        }
        if now - state.disk_last_sample < DISK_SAMPLE_INTERVAL {
            return;
        }
        state.disk_last_sample = now;
        sampler.sample();
        state.disk_history.push_back((sampler.total_read, sampler.total_write));
        while state.disk_history.len() > DISK_HISTORY {
            state.disk_history.pop_front();
        }
    }

    pub fn ui_disk_monitor(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "📈",
            l.tr("Монітор дисків", "Disk monitor"),
            l.tr(
                "Хто й наскільки навантажує диски прямо зараз. Зручно, коли комп'ютер «гальмує» без видимої причини.",
                "What is hitting your drives right now. Handy when the PC slows down for no obvious reason.",
            ),
        );

        let now = ui.ctx().input(|i| i.time);
        self.update_disk_samples(now);
        if !self.tools.disk_paused {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }

        let mut toggle_pause = false;
        ui.horizontal(|ui| {
            let label = if self.tools.disk_paused {
                l.tr("▶ Продовжити", "▶ Resume")
            } else {
                l.tr("⏸ Пауза", "⏸ Pause")
            };
            if widgets::secondary_button(ui, label).clicked() {
                toggle_pause = true;
            }
            ui.add_space(14.0);
            widgets::pill(ui, l.tr("■ Читання", "■ Read"), READ_COLOR);
            widgets::pill(ui, l.tr("■ Запис", "■ Write"), WRITE_COLOR);
        });
        ui.add_space(12.0);

        let Some(sampler) = &self.tools.disk_sampler else {
            return;
        };
        let history = &self.tools.disk_history;
        let (read, write) = history.back().copied().unwrap_or((0.0, 0.0));

        // ---------- Общая активность ----------
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("Загальна активність вводу-виводу", "Overall I/O activity"));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                stat(ui, l.tr("↓ Читання", "↓ Read"), &rate_text(read, l), READ_COLOR);
                ui.add_space(24.0);
                stat(ui, l.tr("↑ Запис", "↑ Write"), &rate_text(write, l), WRITE_COLOR);
            });
            ui.add_space(8.0);
            sparkline(ui, history, 120.0, l);
        });
        ui.add_space(14.0);

        // ---------- Диски ----------
        let drives = &sampler.drives;
        if !drives.is_empty() {
            let per_row = drives.len().min(3);
            for chunk in drives.chunks(per_row) {
                ui.columns(per_row, |cols| {
                    for (col, drive) in cols.iter_mut().zip(chunk) {
                        widgets::card(col, |ui| {
                            let kind = if drive.is_hdd { "HDD" } else { "SSD" };
                            widgets::section_title(ui, &format!("{}  ({kind})", drive.letter));
                            ui.add_space(6.0);
                            let used = drive.total_gb.saturating_sub(drive.free_gb);
                            let fraction = if drive.total_gb > 0 {
                                used as f32 / drive.total_gb as f32
                            } else {
                                0.0
                            };
                            let bar_text = match l {
                                Lang::Uk => format!("Зайнято {used} з {} ГБ", drive.total_gb),
                                Lang::En => format!("{used} of {} GB used", drive.total_gb),
                            };
                            ui.add(
                                egui::ProgressBar::new(fraction)
                                    .text(bar_text)
                                    .fill(if fraction > 0.85 { DANGER } else { ACCENT.gamma_multiply(0.7) }),
                            );
                            ui.label(
                                RichText::new(match l {
                                    Lang::Uk => format!("Вільно {} ГБ", drive.free_gb),
                                    Lang::En => format!("{} GB free", drive.free_gb),
                                })
                                .size(12.5)
                                .color(MUTED),
                            );
                        });
                    }
                });
                ui.add_space(10.0);
            }
        }
        ui.add_space(4.0);

        // ---------- Процессы ----------
        widgets::card(ui, |ui| {
            widgets::section_title(
                ui,
                l.tr("Процеси з найбільшим вводом-виводом", "Processes with the most I/O"),
            );
            ui.label(
                RichText::new(l.tr(
                    "Враховано весь ввід-вивід процесу (диск і частково мережа).",
                    "Counts all process I/O (disk and partly network).",
                ))
                .size(12.0)
                .color(MUTED),
            );
            ui.add_space(6.0);
            if sampler.procs.is_empty() {
                ui.label(RichText::new(l.tr("Зараз тихо.", "Quiet right now.")).color(MUTED));
                return;
            }
            egui::Grid::new("disk_processes")
                .num_columns(4)
                .spacing([22.0, 8.0])
                .show(ui, |ui| {
                    for header in [l.tr("Процес", "Process"), "PID", l.tr("Читання", "Read"), l.tr("Запис", "Write")] {
                        ui.label(RichText::new(header).color(MUTED));
                    }
                    ui.end_row();
                    for p in &sampler.procs {
                        ui.label(RichText::new(&p.name).strong().color(TEXT));
                        ui.label(RichText::new(p.pid.to_string()).color(MUTED));
                        ui.label(RichText::new(rate_text(p.read_mb, l)).color(READ_COLOR));
                        ui.label(RichText::new(rate_text(p.write_mb, l)).color(WRITE_COLOR));
                        ui.end_row();
                    }
                });
        });

        if toggle_pause {
            self.tools.disk_paused ^= true;
        }
    }
}

fn stat(ui: &mut Ui, label: &str, value: &str, color: Color32) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(RichText::new(label).size(12.0).color(MUTED));
        ui.label(RichText::new(value).size(18.0).strong().color(color));
    });
}

/// Мини-график чтения и записи.
fn sparkline(ui: &mut Ui, samples: &VecDeque<(f32, f32)>, height: f32, l: Lang) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, INPUT_BG);

    let inner = Rect::from_min_max(rect.min + vec2(6.0, 18.0), rect.max - vec2(6.0, 6.0));
    let max = samples
        .iter()
        .map(|(r, w)| r.max(*w))
        .fold(5.0_f32, f32::max)
        * 1.15;

    for k in 1..4 {
        let y = inner.top() + inner.height() * k as f32 / 4.0;
        painter.hline(inner.x_range(), y, Stroke::new(1.0_f32, DIVIDER));
    }

    if samples.len() >= 2 {
        // Точки прижаты к правому краю: график «растёт» справа налево.
        let step = inner.width() / (DISK_HISTORY - 1) as f32;
        let offset = (DISK_HISTORY - samples.len()) as f32 * step;
        for (channel, color) in [(0, WRITE_COLOR), (1, READ_COLOR)] {
            let points: Vec<_> = samples
                .iter()
                .enumerate()
                .map(|(i, (r, w))| {
                    let v = if channel == 0 { *w } else { *r };
                    pos2(
                        inner.left() + offset + i as f32 * step,
                        inner.bottom() - v / max * inner.height(),
                    )
                })
                .collect();
            painter.add(Shape::line(points, Stroke::new(1.8_f32, color)));
        }
    }

    painter.text(
        rect.left_top() + vec2(8.0, 3.0),
        Align2::LEFT_TOP,
        format!("{:.0} {}", max, l.tr("МБ/с", "MB/s")),
        FontId::proportional(11.0),
        MUTED,
    );
    painter.text(
        rect.right_top() + vec2(-8.0, 3.0),
        Align2::RIGHT_TOP,
        l.tr("останні 30 с", "last 30 s"),
        FontId::proportional(11.0),
        MUTED,
    );
}
