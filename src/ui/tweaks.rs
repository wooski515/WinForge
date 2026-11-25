//! Вкладки с переключателями («Приватность», «Оптимизация», «Контекстное меню»),
//! «Удаление» и общие куски для «Очистки» и «Реестра».

use std::collections::HashMap;

use eframe::egui::{Align, FontId, Layout, RichText, Ui};

use crate::app::{Section, WinForgeApp};
use crate::catalog::Risk;
use crate::theme::{self, *};
use crate::widgets::{self, GridItem};

impl WinForgeApp {
    pub fn ui_privacy(&mut self, ui: &mut Ui) {
        let l = self.lang;
        self.tweak_page(
            ui,
            Section::Privacy,
            "🛡",
            l.tr("Приватність і телеметрія", "Privacy and telemetry"),
            l.tr(
                "Служби, політики й параметри, які надсилають твої дані до Microsoft. Вимкни зайве.",
                "Services, policies and settings that send your data to Microsoft. Turn off what you don't need.",
            ),
        );
    }

    pub fn ui_system(&mut self, ui: &mut Ui) {
        let l = self.lang;
        self.tweak_page(
            ui,
            Section::System,
            "⚙",
            l.tr("Оптимізація системи", "System optimization"),
            l.tr(
                "Продуктивність, служби, диски, інтерфейс, оновлення та безпека.",
                "Performance, services, drives, interface, updates and security.",
            ),
        );
    }

    /// Панель над списком: «рекомендованное», «сбросить», легенда и счётчик.
    /// Возвращает (нажато «рекомендованное», нажато «сбросить»).
    pub(crate) fn section_toolbar(&self, ui: &mut Ui, section: Section) -> (bool, bool) {
        let l = self.lang;
        let (on, total) = self.count_enabled(section);
        let counter = match section {
            Section::Privacy | Section::System | Section::Integrator => l.tr("Увімкнено", "Enabled"),
            Section::Removal | Section::Cleanup | Section::Registry => l.tr("Позначено", "Selected"),
        };
        let mut recommended = false;
        let mut reset = false;
        ui.horizontal(|ui| {
            if widgets::primary_button(ui, l.tr("⚡ Застосувати рекомендоване", "⚡ Apply recommended")).clicked() {
                recommended = true;
            }
            if widgets::secondary_button(ui, l.tr("↺ Скинути все", "↺ Reset all")).clicked() {
                reset = true;
            }
            ui.add_space(14.0);
            widgets::risk_legend(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{counter}: {on} {} {total}", l.tr("з", "of")))
                        .strong()
                        .color(TEXT),
                );
            });
        });
        (recommended, reset)
    }

    pub(crate) fn apply_toolbar(&mut self, ui: &Ui, section: Section, (recommended, reset): (bool, bool)) {
        let l = self.lang;
        if recommended {
            self.set_recommended(section);
            self.notify(ui.ctx(), l.tr("Увімкнено рекомендовані пункти", "Recommended items selected"));
        }
        if reset {
            self.clear_section(section);
            self.notify(ui.ctx(), l.tr("Усі пункти розділу вимкнено", "All items in this section cleared"));
        }
    }

    /// Страница со списком переключателей по группам.
    pub(crate) fn tweak_page(&mut self, ui: &mut Ui, section: Section, icon: &str, title: &str, subtitle: &str) {
        let l = self.lang;
        widgets::page_header(ui, icon, title, subtitle);
        let toolbar = self.section_toolbar(ui, section);
        ui.add_space(2.0);
        ui.label(
            RichText::new(l.tr(
                "Клік по рядку — увімкнути або вимкнути. Наведи курсор, щоб побачити, що саме зміниться.",
                "Click a row to turn it on or off. Hover to see exactly what changes.",
            ))
            .size(12.5)
            .color(MUTED),
        );
        ui.add_space(12.0);

        let Some(catalog) = self.section_catalog(section) else {
            return;
        };

        let mut clicked: Option<String> = None;
        let mut group_recommended: Option<Vec<String>> = None;
        // (все пункты группы, пункты выбранного режима)
        let mut group_preset: Option<(Vec<String>, Vec<String>)> = None;

        for (gi, group) in catalog.group.iter().enumerate() {
            let group_on = group
                .tweak
                .iter()
                .filter(|t| self.enabled.contains(&t.id))
                .count();

            widgets::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{}. {}", gi + 1, group.name.get(l)))
                            .font(FontId::new(17.0, theme::bold()))
                            .color(TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .small_button(l.tr("Рекомендовані", "Recommended"))
                            .on_hover_text(l.tr(
                                "Увімкнути рекомендовані пункти цієї групи",
                                "Turn on the recommended items in this group",
                            ))
                            .clicked()
                        {
                            group_recommended = Some(
                                group
                                    .tweak
                                    .iter()
                                    .filter(|t| t.recommended && self.hw_warning(&t.avoid).is_none())
                                    .map(|t| t.id.clone())
                                    .collect(),
                            );
                        }
                        ui.label(
                            RichText::new(format!("{group_on}/{}", group.tweak.len())).color(MUTED),
                        );
                    });
                });
                if !group.description.is_empty() {
                    ui.label(RichText::new(group.description.get(l)).size(13.0).color(MUTED));
                }
                if !group.preset.is_empty() {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        for p in &group.preset {
                            // Режим выбран, если в группе включены ровно его пункты.
                            let active = group
                                .tweak
                                .iter()
                                .all(|t| self.enabled.contains(&t.id) == p.tweaks.contains(&t.id));
                            let text = if active {
                                // Фон выбранной кнопки — ACCENT, поэтому текст белый.
                                RichText::new(format!("✔ {}", p.name.get(l)))
                                    .color(eframe::egui::Color32::WHITE)
                                    .strong()
                            } else {
                                RichText::new(p.name.get(l))
                            };
                            if ui
                                .selectable_label(active, text)
                                .on_hover_text(p.description.get(l))
                                .clicked()
                            {
                                group_preset = Some((
                                    group.tweak.iter().map(|t| t.id.clone()).collect(),
                                    p.tweaks.clone(),
                                ));
                            }
                        }
                    });
                }
                ui.add_space(6.0);
                ui.spacing_mut().item_spacing.y = 0.0;

                let count = group.tweak.len();
                for (i, tweak) in group.tweak.iter().enumerate() {
                    let on = self.enabled.contains(&tweak.id);
                    let hw = self.hw_warning(&tweak.avoid);
                    let description = match hw {
                        Some(w) => format!("⚠ {w} · {}", tweak.description.get(l)),
                        None => tweak.description.get(l).to_owned(),
                    };
                    let response = widgets::setting_row(
                        ui,
                        tweak.name.get(l),
                        &description,
                        on,
                        Some(tweak.risk),
                        i + 1 < count,
                    )
                    .on_hover_ui(|ui| {
                        widgets::item_tooltip(
                            ui,
                            tweak.name.get(l),
                            &description,
                            &tweak.details,
                            tweak.warning.as_ref().map(|w| w.get(l)),
                            tweak.recommended,
                        )
                    });
                    if response.clicked() {
                        clicked = Some(tweak.id.clone());
                    }
                }
            });
            ui.add_space(14.0);
        }

        self.apply_toolbar(ui, section, toolbar);
        if let Some(ids) = group_recommended {
            self.enabled.extend(ids);
        }
        if let Some((all, chosen)) = group_preset {
            self.select_preset(&all, &chosen);
        }
        if let Some(id) = clicked {
            self.click_item(&id);
        }
    }

    /// Группы пунктов в виде карточек с галочками (очистка, реестр).
    /// `sizes` — результаты анализа очистки: размер показывается в подписи карточки.
    pub(crate) fn card_groups(
        &mut self,
        ui: &mut Ui,
        section: Section,
        min_card_width: f32,
        sizes: Option<&HashMap<String, u64>>,
    ) {
        let l = self.lang;
        let Some(catalog) = self.section_catalog(section) else {
            return;
        };

        let mut clicked: Vec<String> = Vec::new();
        let mut bulk: Option<(Vec<String>, bool)> = None;

        for group in &catalog.group {
            let chosen = group.tweak.iter().filter(|t| self.enabled.contains(&t.id)).count();
            // «Выбрать все» не трогает опасные пункты: их включают только по одному.
            let safe_ids: Vec<String> = group
                .tweak
                .iter()
                .filter(|t| t.risk != Risk::Danger && self.hw_warning(&t.avoid).is_none())
                .map(|t| t.id.clone())
                .collect();
            let all_ids: Vec<String> = group.tweak.iter().map(|t| t.id.clone()).collect();

            ui.horizontal(|ui| {
                let icon = if group.icon.is_empty() { "•" } else { group.icon.as_str() };
                ui.label(
                    RichText::new(format!("{icon}  {}", group.name.get(l)))
                        .font(FontId::new(17.0, theme::bold()))
                        .color(TEXT),
                );
                ui.label(RichText::new(format!("{chosen}/{}", group.tweak.len())).color(MUTED));
                if let Some(sizes) = sizes {
                    let group_mb: u64 = group.tweak.iter().filter_map(|t| sizes.get(&t.id)).sum();
                    if group_mb > 0 {
                        ui.label(
                            RichText::new(format!("· {}", widgets::format_size(group_mb, l)))
                                .color(SAFE),
                        );
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(l.tr("Зняти", "Clear")).clicked() {
                        bulk = Some((all_ids.clone(), false));
                    }
                    if ui
                        .small_button(l.tr("Вибрати всі", "Select all"))
                        .on_hover_text(l.tr(
                            "Небезпечні пункти треба вмикати окремо",
                            "Risky items have to be selected one by one",
                        ))
                        .clicked()
                    {
                        bulk = Some((safe_ids.clone(), true));
                    }
                });
            });
            if !group.description.is_empty() {
                ui.label(RichText::new(group.description.get(l)).size(13.0).color(MUTED));
            }
            ui.add_space(4.0);

            let subtitles: Vec<String> = group
                .tweak
                .iter()
                .map(|t| match sizes.and_then(|s| s.get(&t.id)) {
                    Some(mb) => format!("{} · {}", widgets::format_size(*mb, l), t.description.get(l)),
                    None => t.description.get(l).to_owned(),
                })
                .collect();
            let items = group.tweak.iter().zip(&subtitles).map(|(t, subtitle)| GridItem {
                id: &t.id,
                title: t.name.get(l),
                subtitle,
                selected: self.enabled.contains(&t.id),
                risk: Some(t.risk),
                hover: Some(Box::new(move |ui: &mut Ui| {
                    widgets::item_tooltip(
                        ui,
                        t.name.get(l),
                        t.description.get(l),
                        &t.details,
                        t.warning.as_ref().map(|w| w.get(l)),
                        t.recommended,
                    );
                    ui.add_space(2.0);
                    widgets::risk_badge(ui, t.risk);
                })),
            });
            clicked.extend(widgets::card_grid(ui, min_card_width, items));
            ui.add_space(18.0);
        }

        if let Some((ids, select)) = bulk {
            for id in ids {
                if select {
                    self.enabled.insert(id);
                } else {
                    self.enabled.remove(&id);
                }
            }
        }
        for id in clicked {
            self.click_item(&id);
        }
    }

    pub fn ui_removal(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "🗑",
            l.tr("Видалення вбудованих застосунків", "Remove built-in apps"),
            match self.mode {
                crate::app::Mode::Build => l.tr(
                    "Позначені застосунки не з'являться в новій Windows: інсталятор видалить їх під час встановлення.",
                    "Checked apps won't appear in the new Windows: Setup removes them during installation.",
                ),
                crate::app::Mode::Live => l.tr(
                    "Позначені застосунки буде видалено з цієї Windows для всіх користувачів.",
                    "Checked apps are removed from this Windows for all users.",
                ),
            },
        );
        let toolbar = self.section_toolbar(ui, Section::Removal);
        ui.add_space(2.0);
        ui.label(
            RichText::new(l.tr(
                "Кольорова крапка в куті картки — видаляй обережно. Наведи курсор, щоб дізнатися більше.",
                "A colored dot in the corner means remove with care. Hover for details.",
            ))
            .size(12.5)
            .color(MUTED),
        );
        ui.add_space(12.0);

        let package_label = l.tr("Пакет", "Package");
        // Разделы из каталога; пункты с неизвестным разделом (или старый каталог
        // без разделов) показываются в конце одной сеткой.
        let groups = &self.cat.bloat.group;
        let mut sections: Vec<(Option<&crate::catalog::BloatGroup>, Vec<&crate::catalog::BloatEntry>)> = groups
            .iter()
            .map(|g| (Some(g), self.cat.bloat.app.iter().filter(|a| a.group == g.id).collect()))
            .collect();
        let rest: Vec<_> = self
            .cat
            .bloat
            .app
            .iter()
            .filter(|a| !groups.iter().any(|g| g.id == a.group))
            .collect();
        if !rest.is_empty() {
            sections.push((None, rest));
        }

        let mut clicked: Vec<String> = Vec::new();
        let mut bulk: Option<(Vec<String>, bool)> = None;
        for (group, entries) in sections {
            if entries.is_empty() {
                continue;
            }
            if let Some(g) = group {
                let chosen = entries.iter().filter(|a| self.enabled.contains(&a.id)).count();
                ui.horizontal(|ui| {
                    let icon = if g.icon.is_empty() { "•" } else { g.icon.as_str() };
                    ui.label(
                        RichText::new(format!("{icon}  {}", g.name.get(l)))
                            .font(FontId::new(17.0, theme::bold()))
                            .color(TEXT),
                    );
                    ui.label(RichText::new(format!("{chosen}/{}", entries.len())).color(MUTED));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button(l.tr("Зняти", "Clear")).clicked() {
                            bulk = Some((entries.iter().map(|a| a.id.clone()).collect(), false));
                        }
                        if ui
                            .small_button(l.tr("Рекомендовані", "Recommended"))
                            .on_hover_text(l.tr(
                                "Позначити рекомендовані пункти цього розділу",
                                "Select the recommended items in this section",
                            ))
                            .clicked()
                        {
                            bulk = Some((
                                entries.iter().filter(|a| a.recommended).map(|a| a.id.clone()).collect(),
                                true,
                            ));
                        }
                    });
                });
                if !g.description.is_empty() {
                    ui.label(RichText::new(g.description.get(l)).size(13.0).color(MUTED));
                }
                ui.add_space(4.0);
            }
            let items = entries.into_iter().map(|a| GridItem {
                id: &a.id,
                title: a.name.get(l),
                subtitle: a.package.get(l),
                selected: self.enabled.contains(&a.id),
                risk: Some(a.risk),
                hover: Some(Box::new(move |ui: &mut Ui| {
                    widgets::item_tooltip(
                        ui,
                        a.name.get(l),
                        &format!("{package_label}: {}", a.package.get(l)),
                        &[],
                        a.warning.as_ref().map(|w| w.get(l)),
                        a.recommended,
                    );
                    ui.add_space(2.0);
                    widgets::risk_badge(ui, a.risk);
                })),
            });
            clicked.extend(widgets::card_grid(ui, 240.0, items));
            ui.add_space(18.0);
        }

        if let Some((ids, select)) = bulk {
            for id in ids {
                if select {
                    self.enabled.insert(id);
                } else {
                    self.enabled.remove(&id);
                }
            }
        }
        self.apply_toolbar(ui, Section::Removal, toolbar);
        for id in clicked {
            self.click_item(&id);
        }
    }
}
