//! Вкладка «Переменные среды (PATH)»: реальный PATH пользователя и системы.

use eframe::egui::{self, Align, Layout, RichText, Ui};

use crate::app::WinForgeApp;
use crate::i18n::Lang;
use crate::theme::*;
use crate::sys::{self, probe};
use crate::tools::{duplicate_flags, normalize_path, PathEntry, PathScope};
use crate::widgets;

enum PathAction {
    Up(usize),
    Down(usize),
    Remove(usize),
    Add,
    RemoveMissing,
    RemoveDuplicates,
    Revert,
    Save,
}

impl WinForgeApp {
    /// Читает реальный PATH пользователя и системы из реестра.
    fn reload_path(&mut self) {
        let load = |system: bool| -> Vec<PathEntry> {
            probe::read_path(system)
                .into_iter()
                .map(|(path, missing)| PathEntry {
                    path,
                    missing,
                    added: false,
                })
                .collect()
        };
        self.tools.path_user = load(false);
        self.tools.path_system = load(true);
        self.tools.path_user_saved = self.tools.path_user.clone();
        self.tools.path_system_saved = self.tools.path_system.clone();
        self.tools.path_loaded = true;
    }

    fn path_list(&mut self) -> &mut Vec<PathEntry> {
        match self.tools.path_scope {
            PathScope::User => &mut self.tools.path_user,
            PathScope::System => &mut self.tools.path_system,
        }
    }

    fn path_saved(&self) -> &Vec<PathEntry> {
        match self.tools.path_scope {
            PathScope::User => &self.tools.path_user_saved,
            PathScope::System => &self.tools.path_system_saved,
        }
    }

    pub fn ui_env_vars(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "📁",
            l.tr("Змінні середовища (PATH)", "Environment variables (PATH)"),
            l.tr(
                "Папки, де Windows шукає програми, коли ти пишеш команду в терміналі. Прибери мертві шляхи й додай нові.",
                "Folders Windows searches when you type a command in a terminal. Remove dead paths and add new ones.",
            ),
        );
        if !self.tools.path_loaded {
            self.reload_path();
        }
        ui.add_space(12.0);

        let scope = self.tools.path_scope;
        let entries = match scope {
            PathScope::User => &self.tools.path_user,
            PathScope::System => &self.tools.path_system,
        };
        let duplicates = duplicate_flags(entries);
        let missing_count = entries.iter().filter(|e| e.missing).count();
        let duplicate_count = duplicates.iter().filter(|d| **d).count();
        let changed = entries != self.path_saved();

        let mut action: Option<PathAction> = None;
        let mut new_scope = scope;

        // Переключатель «пользователь / система»
        ui.horizontal(|ui| {
            ui.selectable_value(&mut new_scope, PathScope::User, l.tr("👤  Користувача", "👤  User"));
            ui.selectable_value(&mut new_scope, PathScope::System, l.tr("🖥  Системні", "🖥  System"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if changed {
                    widgets::pill(ui, l.tr("Є незбережені зміни", "Unsaved changes"), MODERATE);
                }
            });
        });
        if scope == PathScope::System && !sys::is_elevated() {
            ui.label(
                RichText::new(l.tr(
                    "Зберегти системний PATH можна лише від імені адміністратора.",
                    "Saving the system PATH requires administrator rights.",
                ))
                .size(12.5)
                .color(MODERATE),
            );
        }
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            let missing_label = format!("{} ({missing_count})", l.tr("🗑 Прибрати неіснуючі", "🗑 Remove missing"));
            if ui
                .add_enabled(missing_count > 0, egui::Button::new(missing_label).min_size(egui::vec2(0.0, 34.0)))
                .clicked()
            {
                action = Some(PathAction::RemoveMissing);
            }
            let dup_label = format!("{} ({duplicate_count})", l.tr("🗑 Прибрати дублікати", "🗑 Remove duplicates"));
            if ui
                .add_enabled(duplicate_count > 0, egui::Button::new(dup_label).min_size(egui::vec2(0.0, 34.0)))
                .clicked()
            {
                action = Some(PathAction::RemoveDuplicates);
            }
            if ui
                .add_enabled(
                    changed,
                    egui::Button::new(l.tr("↺ Скасувати зміни", "↺ Revert changes")).min_size(egui::vec2(0.0, 34.0)),
                )
                .clicked()
            {
                action = Some(PathAction::Revert);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::primary_button(ui, l.tr("💾 Зберегти", "💾 Save")).clicked() {
                    action = Some(PathAction::Save);
                }
            });
        });
        ui.add_space(12.0);

        // ---------- Список ----------
        let count = entries.len();
        widgets::card(ui, |ui| {
            // Справа фиксированная колонка (пометки + кнопки), путь обрезается по ширине.
            const RIGHT: f32 = 250.0;
            ui.spacing_mut().item_spacing.y = 6.0;
            for (i, entry) in entries.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(30.0, 22.0),
                        Layout::left_to_right(Align::Center),
                        |ui| ui.label(RichText::new(format!("{}", i + 1)).color(MUTED)),
                    );
                    let color = if entry.missing { MUTED } else { TEXT };
                    let mut text = RichText::new(&entry.path).monospace().size(13.0).color(color);
                    if entry.missing {
                        text = text.strikethrough();
                    }
                    let path_width = (ui.available_width() - RIGHT).max(120.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(path_width, 22.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.add(egui::Label::new(text).truncate())
                                .on_hover_text(probe::expand(&entry.path));
                        },
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("✖").on_hover_text(l.tr("Видалити", "Remove")).clicked() {
                            action = Some(PathAction::Remove(i));
                        }
                        if ui
                            .add_enabled(i + 1 < count, egui::Button::new("↓").small())
                            .on_hover_text(l.tr("Нижче", "Down"))
                            .clicked()
                        {
                            action = Some(PathAction::Down(i));
                        }
                        if ui
                            .add_enabled(i > 0, egui::Button::new("↑").small())
                            .on_hover_text(l.tr("Вище (шукається раніше)", "Up (searched earlier)"))
                            .clicked()
                        {
                            action = Some(PathAction::Up(i));
                        }
                        ui.add_space(8.0);
                        if !entry.missing && !duplicates[i] && !entry.added {
                            widgets::pill(ui, "✔ OK", SAFE);
                        }
                        if entry.added {
                            widgets::pill(ui, l.tr("Новий", "New"), ACCENT);
                        }
                        if duplicates[i] {
                            widgets::pill(ui, l.tr("Дублікат", "Duplicate"), MODERATE).on_hover_text(l.tr(
                                "Цей шлях уже є вище у списку",
                                "This path is already listed above",
                            ));
                        }
                        if entry.missing {
                            widgets::pill(ui, l.tr("Не існує", "Missing"), DANGER).on_hover_text(l.tr(
                                "Такої папки немає — запис нічого не дає",
                                "This folder doesn't exist — the entry does nothing",
                            ));
                        }
                    });
                });
            }
            if entries.is_empty() {
                ui.label(RichText::new(l.tr("PATH порожній.", "PATH is empty.")).color(MUTED));
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.tools.path_new)
                        .hint_text(r"C:\Tools\bin")
                        .font(egui::TextStyle::Monospace)
                        .desired_width(420.0),
                );
                if widgets::primary_button(ui, l.tr("➕ Додати", "➕ Add")).clicked() {
                    action = Some(PathAction::Add);
                }
            });
        });
        ui.add_space(12.0);

        // ---------- Одной строкой ----------
        let joined = entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>().join(";");
        egui::CollapsingHeader::new(
            RichText::new(l.tr("Як це виглядає одним рядком", "How it looks as one line")).color(MUTED),
        )
        .id_salt("path_joined")
        .show(ui, |ui| {
            ui.label(RichText::new(joined).monospace().size(12.0).color(TEXT));
        });

        if new_scope != scope {
            self.tools.path_scope = new_scope;
        }
        if let Some(action) = action {
            self.apply_path_action(ui, action, l);
        }
    }

    fn apply_path_action(&mut self, ui: &Ui, action: PathAction, l: Lang) {
        let msg: Option<String> = match action {
            PathAction::Up(i) => {
                self.path_list().swap(i, i - 1);
                None
            }
            PathAction::Down(i) => {
                self.path_list().swap(i, i + 1);
                None
            }
            PathAction::Remove(i) => {
                self.path_list().remove(i);
                None
            }
            PathAction::Add => {
                let path = self.tools.path_new.trim().to_owned();
                if path.is_empty() {
                    Some(l.tr("Вкажи шлях до папки", "Enter a folder path").into())
                } else if self
                    .path_list()
                    .iter()
                    .any(|e| normalize_path(&e.path) == normalize_path(&path))
                {
                    Some(l.tr("Цей шлях уже є у списку", "This path is already in the list").into())
                } else {
                    let missing = !std::path::Path::new(&probe::expand(&path)).is_dir();
                    self.path_list().push(PathEntry {
                        path,
                        missing,
                        added: true,
                    });
                    self.tools.path_new.clear();
                    None
                }
            }
            PathAction::RemoveMissing => {
                let list = self.path_list();
                let before = list.len();
                list.retain(|e| !e.missing);
                Some(removed_message(l, before - list.len()))
            }
            PathAction::RemoveDuplicates => {
                let list = self.path_list();
                let flags = duplicate_flags(list);
                let before = list.len();
                let mut i = 0;
                list.retain(|_| {
                    let keep = !flags[i];
                    i += 1;
                    keep
                });
                Some(removed_message(l, before - list.len()))
            }
            PathAction::Revert => {
                let saved = self.path_saved().clone();
                *self.path_list() = saved;
                Some(l.tr("Зміни скасовано", "Changes reverted").into())
            }
            PathAction::Save => {
                let system = self.tools.path_scope == PathScope::System;
                let paths: Vec<String> = self.path_list().iter().map(|e| e.path.clone()).collect();
                match probe::write_path(system, &paths) {
                    Ok(()) => {
                        for entry in self.path_list().iter_mut() {
                            entry.added = false;
                        }
                        let current = self.path_list().clone();
                        match self.tools.path_scope {
                            PathScope::User => self.tools.path_user_saved = current,
                            PathScope::System => self.tools.path_system_saved = current,
                        }
                        Some(
                            l.tr(
                                "PATH збережено. Нові вікна терміналу побачать зміни",
                                "PATH saved. New terminal windows will see the changes",
                            )
                            .into(),
                        )
                    }
                    Err(_) if system && !sys::is_elevated() => Some(
                        l.tr(
                            "Потрібні права адміністратора, щоб зберегти системний PATH",
                            "Administrator rights are needed to save the system PATH",
                        )
                        .into(),
                    ),
                    Err(e) => Some(format!("{}: {e}", l.tr("Не вдалося зберегти", "Could not save"))),
                }
            }
        };
        if let Some(msg) = msg {
            self.notify(ui.ctx(), msg);
        }
    }
}

fn removed_message(l: Lang, removed: usize) -> String {
    match l {
        Lang::Uk => format!("Прибрано записів: {removed}"),
        Lang::En => format!("Entries removed: {removed}"),
    }
}
