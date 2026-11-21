//! Вкладка «Файл hosts»: реальный файл, готовые наборы блокировок, сохранение с бэкапом.

use eframe::egui::{self, Align, Layout, RichText, Ui};

use crate::app::WinForgeApp;
use crate::i18n::Lang;
use crate::sys::{self, probe};
use crate::theme::*;
use crate::tools::{hosts_add, hosts_entry_count, AD_DOMAINS, DEFAULT_HOSTS, TELEMETRY_DOMAINS};
use crate::widgets;

enum HostsAction {
    BlockTelemetry,
    BlockAds,
    Reset,
    AddEntry,
    Save,
    Reload,
    OpenNotepad,
}

impl WinForgeApp {
    pub fn ui_hosts(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "📝",
            l.tr("Файл hosts", "Hosts file"),
            l.tr(
                "Блокуй сайти, рекламу й телеметрію або перенаправляй домени на інші адреси.",
                "Block sites, ads and telemetry, or point domains to other addresses.",
            ),
        );

        if !self.tools.hosts_loaded {
            self.reload_hosts();
        }
        let mut action: Option<HostsAction> = None;
        let entries = hosts_entry_count(&self.tools.hosts_text);

        ui.horizontal(|ui| {
            if widgets::secondary_button(ui, l.tr("🛡 Заблокувати телеметрію Microsoft", "🛡 Block Microsoft telemetry"))
                .clicked()
            {
                action = Some(HostsAction::BlockTelemetry);
            }
            if widgets::secondary_button(ui, l.tr("🚫 Заблокувати рекламу", "🚫 Block ads")).clicked() {
                action = Some(HostsAction::BlockAds);
            }
            if widgets::secondary_button(ui, l.tr("↺ Стандартний файл", "↺ Default file")).clicked() {
                action = Some(HostsAction::Reset);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{}: {entries}", l.tr("Записів", "Entries")))
                        .strong()
                        .color(TEXT),
                );
            });
        });
        ui.add_space(14.0);

        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("Додати запис", "Add an entry"));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(l.tr("Домен", "Domain")).color(MUTED));
                ui.add(
                    egui::TextEdit::singleline(&mut self.tools.hosts_domain)
                        .hint_text("example.com")
                        .desired_width(260.0),
                );
                ui.add_space(8.0);
                ui.label(RichText::new("IP").color(MUTED));
                ui.add(egui::TextEdit::singleline(&mut self.tools.hosts_ip).desired_width(140.0));
                ui.add_space(8.0);
                ui.checkbox(&mut self.tools.hosts_www, l.tr("Також www.", "Also www."));
                ui.add_space(8.0);
                if widgets::primary_button(ui, l.tr("➕ Додати", "➕ Add")).clicked() {
                    action = Some(HostsAction::AddEntry);
                }
            });
            ui.label(
                RichText::new(l.tr(
                    "0.0.0.0 — заблокувати домен. Інша IP-адреса — перенаправити домен туди.",
                    "0.0.0.0 blocks the domain. Any other IP redirects the domain there.",
                ))
                .size(12.5)
                .color(MUTED),
            );
        });
        ui.add_space(14.0);

        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("Вміст файлу", "File contents"));
            ui.label(
                RichText::new(probe::hosts_path().display().to_string())
                    .monospace()
                    .size(12.0)
                    .color(MUTED),
            );
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::multiline(&mut self.tools.hosts_text)
                    .code_editor()
                    .desired_rows(16)
                    .desired_width(f32::INFINITY),
            );
        });
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            if widgets::primary_button(ui, l.tr("💾 Зберегти hosts", "💾 Save hosts")).clicked() {
                action = Some(HostsAction::Save);
            }
            if widgets::secondary_button(ui, l.tr("↻ Перечитати з диска", "↻ Reload from disk")).clicked() {
                action = Some(HostsAction::Reload);
            }
            if widgets::secondary_button(ui, l.tr("📂 Відкрити в Блокноті", "📂 Open in Notepad")).clicked() {
                action = Some(HostsAction::OpenNotepad);
            }
            if !sys::is_elevated() {
                ui.label(
                    RichText::new(l.tr(
                        "Зберегти hosts можна лише від імені адміністратора",
                        "Saving hosts requires administrator rights",
                    ))
                    .size(12.0)
                    .color(MODERATE),
                );
            }
        });

        let Some(action) = action else {
            return;
        };
        let msg = match action {
            HostsAction::BlockTelemetry => {
                let added = self.add_block_list("# WinForge: Microsoft telemetry", TELEMETRY_DOMAINS);
                added_message(l, added)
            }
            HostsAction::BlockAds => {
                let added = self.add_block_list("# WinForge: ads", AD_DOMAINS);
                added_message(l, added)
            }
            HostsAction::Reset => {
                self.tools.hosts_text = DEFAULT_HOSTS.into();
                l.tr("Відновлено стандартний вміст", "Default contents restored").into()
            }
            HostsAction::AddEntry => self.add_hosts_entry(),
            HostsAction::Save => match probe::write_hosts(&self.tools.hosts_text) {
                Ok(()) => {
                    // Чтобы браузеры сразу увидели изменения.
                    let _ = probe::flush_dns();
                    l.tr(
                        "hosts збережено (стара версія — у hosts.bak у папці WinForge)",
                        "hosts saved (the old version is in hosts.bak in the WinForge folder)",
                    )
                    .into()
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => l
                    .tr(
                        "Немає доступу: запусти WinForge від імені адміністратора",
                        "Access denied: run WinForge as administrator",
                    )
                    .into(),
                Err(e) => format!("{}: {e}", l.tr("Не вдалося зберегти", "Could not save")),
            },
            HostsAction::Reload => {
                self.reload_hosts();
                l.tr("Файл перечитано з диска", "File reloaded from disk").into()
            }
            HostsAction::OpenNotepad => {
                let path = probe::hosts_path().display().to_string();
                if sys::launch("notepad.exe", &format!("\"{path}\"")) {
                    l.tr("Відкрито в Блокноті", "Opened in Notepad").into()
                } else {
                    l.tr("Не вдалося відкрити Блокнот", "Could not open Notepad").into()
                }
            }
        };
        self.notify(ui.ctx(), msg);
    }

    fn reload_hosts(&mut self) {
        match probe::read_hosts() {
            Ok(text) => self.tools.hosts_text = text,
            Err(_) => self.tools.hosts_text = DEFAULT_HOSTS.into(),
        }
        self.tools.hosts_loaded = true;
    }

    /// Добавляет набор доменов в блокировку. Возвращает, сколько записей добавлено.
    fn add_block_list(&mut self, comment: &str, domains: &[&str]) -> usize {
        let text = &mut self.tools.hosts_text;
        if !text.contains(comment) {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&format!("\n{comment}\n"));
        }
        domains.iter().filter(|d| hosts_add(text, "0.0.0.0", d)).count()
    }

    fn add_hosts_entry(&mut self) -> String {
        let l = self.lang;
        let domain = self.tools.hosts_domain.trim().to_lowercase();
        if domain.is_empty() || domain.contains(char::is_whitespace) {
            return l.tr("Введи домен без пробілів", "Enter a domain without spaces").into();
        }
        let ip = match self.tools.hosts_ip.trim() {
            "" => "0.0.0.0".to_owned(),
            ip => ip.to_owned(),
        };
        let mut added = usize::from(hosts_add(&mut self.tools.hosts_text, &ip, &domain));
        if self.tools.hosts_www && !domain.starts_with("www.") {
            added += usize::from(hosts_add(&mut self.tools.hosts_text, &ip, &format!("www.{domain}")));
        }
        self.tools.hosts_domain.clear();
        added_message(l, added)
    }
}

fn added_message(l: Lang, added: usize) -> String {
    if added == 0 {
        return l.tr("Ці записи вже є у файлі", "These entries are already in the file").into();
    }
    match l {
        Lang::Uk => format!("Додано записів: {added}"),
        Lang::En => format!("Entries added: {added}"),
    }
}
