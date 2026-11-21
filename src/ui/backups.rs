//! Вкладка «Откат изменений»: журналы применений WinForge и их откат.

use eframe::egui::{self, Align, Layout, RichText, Ui};

use crate::app::WinForgeApp;
use crate::i18n::Lang;
use crate::sys;
use crate::theme::*;
use crate::widgets;

impl WinForgeApp {
    pub fn ui_backups(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "↺",
            l.tr("Відкат змін", "Undo changes"),
            l.tr(
                "Кожне застосування WinForge зберігає, що саме змінилося. Тут це можна повернути назад.",
                "Every time WinForge applies changes it records what changed. You can put it back here.",
            ),
        );

        // Завершение идущего отката.
        if let Some(rx) = &self.tools.rollback_rx {
            match rx.try_recv() {
                Ok(msg) => {
                    self.tools.rollback_rx = None;
                    self.tools.journals = None;
                    self.notify(ui.ctx(), msg);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint(),
                Err(_) => self.tools.rollback_rx = None,
            }
        }
        if self.tools.journals.is_none() {
            self.tools.journals = Some(sys::list_journals());
        }

        let busy = self.tools.rollback_rx.is_some();
        let elevated = sys::is_elevated();
        let mut refresh = false;
        let mut open_folder = false;
        let mut rollback: Option<usize> = None;

        widgets::banner(
            ui,
            "ℹ",
            l.tr(
                "Відкат повертає реєстр, служби, завдання й компоненти. Видалені вбудовані застосунки так не повернути — їх ставлять знову з Microsoft Store.",
                "Undo restores the registry, services, tasks and features. Removed built-in apps can't come back this way — reinstall them from the Microsoft Store.",
            ),
            ACCENT,
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if widgets::secondary_button(ui, l.tr("↻ Оновити", "↻ Refresh")).clicked() {
                refresh = true;
            }
            if widgets::secondary_button(ui, l.tr("📂 Папка журналів", "📂 Journal folder")).clicked() {
                open_folder = true;
            }
            if busy {
                ui.spinner();
                ui.label(RichText::new(l.tr("Відкат…", "Undoing…")).color(MUTED));
            }
        });
        ui.add_space(12.0);

        let journals = self.tools.journals.clone().unwrap_or_default();
        if journals.is_empty() {
            widgets::card(ui, |ui| {
                ui.label(
                    RichText::new(l.tr(
                        "Поки що нічого не застосовано — відкочувати нічого.",
                        "Nothing has been applied yet, so there is nothing to undo.",
                    ))
                    .color(MUTED),
                );
            });
        }

        for (i, (_, journal)) in journals.iter().enumerate() {
            let needs_admin = journal.needs_admin();
            widgets::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&journal.label).strong().color(TEXT));
                        let info = match l {
                            Lang::Uk => format!("{} UTC · змін: {}", journal.display_time(), journal.undos.len()),
                            Lang::En => format!("{} UTC · changes: {}", journal.display_time(), journal.undos.len()),
                        };
                        ui.label(RichText::new(info).size(12.5).color(MUTED));
                        if needs_admin && !elevated {
                            ui.label(
                                RichText::new(l.tr(
                                    "Для відкату потрібні права адміністратора",
                                    "Undoing this needs administrator rights",
                                ))
                                .size(12.0)
                                .color(MODERATE),
                            );
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let enabled = !busy && (!needs_admin || elevated);
                        let button = egui::Button::new(RichText::new(l.tr("↺ Відкотити", "↺ Undo")).strong())
                            .min_size(egui::vec2(120.0, 32.0));
                        if ui.add_enabled(enabled, button).clicked() {
                            rollback = Some(i);
                        }
                    });
                });
            });
            ui.add_space(10.0);
        }

        if refresh {
            self.tools.journals = None;
        }
        if open_folder {
            let dir = sys::backup_dir();
            let _ = std::fs::create_dir_all(&dir);
            sys::open_folder(&dir);
        }
        if let Some(i) = rollback {
            let (path, journal) = journals[i].clone();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let (ok, errors) = sys::rollback(&journal);
                let msg = if errors.is_empty() {
                    sys::mark_rolled_back(&path);
                    sys::probe::broadcast_settings();
                    match l {
                        Lang::Uk => format!("Відкочено змін: {ok}"),
                        Lang::En => format!("Changes undone: {ok}"),
                    }
                } else {
                    match l {
                        Lang::Uk => format!("Відкочено: {ok}, помилок: {} ({})", errors.len(), errors[0]),
                        Lang::En => format!("Undone: {ok}, errors: {} ({})", errors.len(), errors[0]),
                    }
                };
                let _ = tx.send(msg);
            });
            self.tools.rollback_rx = Some(rx);
        }
    }
}
