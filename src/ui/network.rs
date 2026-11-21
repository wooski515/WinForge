//! Вкладка «Сеть»: текущий и новый DNS, очистка кэша DNS и настоящий пинг.

use eframe::egui::{self, Margin, RichText, Ui};

use crate::app::WinForgeApp;
use crate::i18n::Lang;
use crate::sys::{self, probe};
use crate::theme::*;
use crate::tools::{PingRun, DNS_PROVIDERS, PING_COUNT, PING_PRESETS};
use crate::widgets::{self, GridItem};

impl WinForgeApp {
    fn load_current_dns(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(probe::current_dns());
        });
        self.tools.dns_rx = Some(rx);
    }

    fn start_ping(&mut self, host: String) {
        let (tx, rx) = std::sync::mpsc::channel();
        let target = host.clone();
        std::thread::spawn(move || {
            for _ in 0..PING_COUNT {
                let started = std::time::Instant::now();
                if tx.send(probe::ping_once(&target)).is_err() {
                    return;
                }
                // Не чаще раза в секунду, как обычный ping.
                if let Some(rest) = std::time::Duration::from_secs(1).checked_sub(started.elapsed()) {
                    std::thread::sleep(rest);
                }
            }
        });
        self.tools.ping = Some(PingRun {
            host,
            results: Vec::new(),
            rx,
        });
    }

    /// Адреса DNS выбранного варианта (пусто — автоматически).
    fn chosen_dns_servers(&self) -> Vec<String> {
        let custom_index = DNS_PROVIDERS.len() - 1;
        match self.tools.dns_choice {
            0 => Vec::new(),
            i if i == custom_index => self
                .tools
                .custom_dns
                .iter()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect(),
            i => DNS_PROVIDERS[i]
                .servers
                .split('·')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }

    pub fn ui_network(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "📶",
            l.tr("Мережа", "Network"),
            l.tr(
                "DNS-сервер, очищення кешу DNS і перевірка пінгу.",
                "DNS server, DNS cache flush and ping checks.",
            ),
        );

        // Приём фоновых результатов.
        if self.tools.dns_current.is_none() && self.tools.dns_rx.is_none() {
            self.load_current_dns();
        }
        if let Some(rx) = &self.tools.dns_rx {
            match rx.try_recv() {
                Ok(list) => {
                    self.tools.dns_current = Some(list);
                    self.tools.dns_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint(),
                Err(_) => self.tools.dns_rx = None,
            }
        }
        if let Some(run) = &mut self.tools.ping {
            while let Ok(result) = run.rx.try_recv() {
                run.results.push(result);
            }
            if run.results.len() < PING_COUNT {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        }

        let custom_index = DNS_PROVIDERS.len() - 1;
        let mut chosen: Option<usize> = None;
        let mut apply_dns = false;
        let mut flush = false;
        let elevated = sys::is_elevated();

        // ---------- DNS ----------
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("DNS-сервер", "DNS server"));
            ui.label(
                RichText::new(l.tr(
                    "Від DNS залежить, як швидко відкриваються сайти. Деякі ще й блокують рекламу чи шкідливі сайти.",
                    "DNS affects how fast sites open. Some also block ads or malicious sites.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(4.0);
            let current = match &self.tools.dns_current {
                Some(list) if list.is_empty() => l.tr("невідомо", "unknown").to_owned(),
                Some(list) => list.join(" · "),
                None => l.tr("визначаємо…", "detecting…").to_owned(),
            };
            ui.label(
                RichText::new(format!("{}: {current}", l.tr("Зараз", "Current")))
                    .monospace()
                    .size(12.5)
                    .color(ACCENT),
            );
            ui.add_space(8.0);

            let ids: Vec<String> = (0..DNS_PROVIDERS.len()).map(|i| format!("dns-{i}")).collect();
            let current_choice = self.tools.dns_choice;
            let items = DNS_PROVIDERS.iter().enumerate().map(|(i, p)| GridItem {
                id: &ids[i],
                title: l.pick(p.name),
                subtitle: p.servers,
                selected: current_choice == i,
                risk: None,
                hover: Some(Box::new(move |ui: &mut Ui| {
                    ui.label(RichText::new(l.pick(p.name)).strong().color(TEXT));
                    ui.label(RichText::new(l.pick(p.note)).color(MUTED));
                })),
            });
            for id in widgets::card_grid(ui, 200.0, items) {
                chosen = id.strip_prefix("dns-").and_then(|n| n.parse().ok());
            }

            if self.tools.dns_choice == custom_index {
                ui.add_space(4.0);
                egui::Grid::new("custom_dns")
                    .num_columns(2)
                    .spacing([18.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new(l.tr("Основний", "Primary")).color(MUTED));
                        ui.add_sized(
                            [220.0, 26.0],
                            egui::TextEdit::singleline(&mut self.tools.custom_dns[0]).hint_text("1.2.3.4"),
                        );
                        ui.end_row();
                        ui.label(RichText::new(l.tr("Додатковий", "Secondary")).color(MUTED));
                        ui.add_sized(
                            [220.0, 26.0],
                            egui::TextEdit::singleline(&mut self.tools.custom_dns[1]).hint_text("5.6.7.8"),
                        );
                        ui.end_row();
                    });
            }

            let doh = DNS_PROVIDERS[self.tools.dns_choice].doh;
            ui.add_space(6.0);
            ui.add_enabled_ui(!doh.is_empty(), |ui| {
                ui.checkbox(
                    &mut self.tools.dns_doh,
                    l.tr("Шифрувати DNS (DNS-over-HTTPS)", "Encrypt DNS (DNS-over-HTTPS)"),
                )
                .on_hover_text(l.tr(
                    "Провайдер не бачить, які сайти ти відкриваєш. Якщо шифрування недоступне, Windows сама перейде на звичайний DNS.",
                    "Your ISP can't see which sites you open. If encryption is unavailable, Windows falls back to plain DNS.",
                ))
                .on_disabled_hover_text(l.tr(
                    "Для цього варіанта шифрування недоступне",
                    "Encryption is not available for this option",
                ));
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if widgets::primary_button(ui, l.tr("✔ Застосувати DNS", "✔ Apply DNS")).clicked() {
                    apply_dns = true;
                }
                if widgets::secondary_button(ui, l.tr("♻ Очистити кеш DNS", "♻ Flush DNS cache"))
                    .on_hover_text("ipconfig /flushdns")
                    .clicked()
                {
                    flush = true;
                }
                if !elevated {
                    ui.label(
                        RichText::new(l.tr(
                            "Зміна DNS потребує прав адміністратора",
                            "Changing DNS requires administrator rights",
                        ))
                        .size(12.0)
                        .color(MODERATE),
                    );
                }
            });
        });
        ui.add_space(14.0);

        // ---------- Пинг ----------
        let mut start_ping: Option<String> = None;
        let mut shodan = false;
        let pinging = self
            .tools
            .ping
            .as_ref()
            .map(|r| r.results.len() < PING_COUNT)
            .unwrap_or(false);

        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("Пінг", "Ping"));
            ui.label(
                RichText::new(l.tr(
                    "Перевір затримку до сервера або сайту.",
                    "Check the latency to a server or website.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_sized(
                    [260.0, 28.0],
                    egui::TextEdit::singleline(&mut self.tools.ping_host)
                        .hint_text(l.tr("IP-адреса або сайт", "IP address or website")),
                );
                let ping_button = egui::Button::new(
                    RichText::new(l.tr("▶ Пінг", "▶ Ping")).color(egui::Color32::WHITE),
                )
                .fill(ACCENT)
                .min_size(egui::vec2(0.0, 30.0));
                if ui.add_enabled(!pinging, ping_button).clicked() {
                    start_ping = Some(self.tools.ping_host.trim().to_owned());
                }
                if widgets::secondary_button(ui, "🔍 Shodan")
                    .on_hover_text(l.tr(
                        "Відкрити відомості про цю IP на shodan.io у браузері",
                        "Open details about this IP on shodan.io in your browser",
                    ))
                    .clicked()
                {
                    shodan = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(l.tr("Швидко:", "Quick:")).color(MUTED));
                for preset in PING_PRESETS {
                    if ui.add_enabled(!pinging, egui::Button::new(*preset).small()).clicked() {
                        start_ping = Some((*preset).to_owned());
                    }
                }
            });

            if let Some(run) = &self.tools.ping {
                ui.add_space(8.0);
                egui::Frame::new()
                    .fill(INPUT_BG)
                    .corner_radius(8.0)
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 3.0;
                        mono(ui, &format!("{} {}:", l.tr("Обмін пакетами з", "Pinging"), run.host), MUTED);
                        for result in &run.results {
                            match result {
                                Some(ms) => {
                                    let line = match l {
                                        Lang::Uk => format!("Відповідь від {}: час={ms} мс", run.host),
                                        Lang::En => format!("Reply from {}: time={ms} ms", run.host),
                                    };
                                    mono(ui, &line, TEXT);
                                }
                                None => mono(ui, l.tr("Немає відповіді", "No reply"), DANGER),
                            }
                        }
                        if run.results.len() < PING_COUNT {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                mono(ui, "…", MUTED);
                            });
                        } else {
                            let times: Vec<u32> = run.results.iter().flatten().copied().collect();
                            let lost = PING_COUNT - times.len();
                            let percent = lost * 100 / PING_COUNT;
                            let summary = if times.is_empty() {
                                match l {
                                    Lang::Uk => format!("Надіслано {PING_COUNT}, втрачено {lost} (100%)"),
                                    Lang::En => format!("Sent {PING_COUNT}, lost {lost} (100%)"),
                                }
                            } else {
                                let min = times.iter().min().copied().unwrap_or(0);
                                let max = times.iter().max().copied().unwrap_or(0);
                                let avg = times.iter().sum::<u32>() / times.len() as u32;
                                match l {
                                    Lang::Uk => format!(
                                        "Надіслано {PING_COUNT}, отримано {}, втрачено {lost} ({percent}%) · мін {min} / сер {avg} / макс {max} мс",
                                        times.len()
                                    ),
                                    Lang::En => format!(
                                        "Sent {PING_COUNT}, received {}, lost {lost} ({percent}%) · min {min} / avg {avg} / max {max} ms",
                                        times.len()
                                    ),
                                }
                            };
                            ui.add_space(4.0);
                            mono(ui, &summary, if lost == 0 { SAFE } else { MODERATE });
                        }
                    });
            }
        });

        // ---------- Действия ----------
        if let Some(i) = chosen {
            self.tools.dns_choice = i;
        }
        if apply_dns {
            let servers = self.chosen_dns_servers();
            let name = l.pick(DNS_PROVIDERS[self.tools.dns_choice].name).to_owned();
            if self.tools.dns_choice == custom_index && servers.is_empty() {
                self.notify(ui.ctx(), l.tr("Введи хоча б одну адресу DNS", "Enter at least one DNS address"));
            } else if !elevated {
                self.notify(
                    ui.ctx(),
                    l.tr(
                        "Перезапусти WinForge від імені адміністратора, щоб змінити DNS",
                        "Restart WinForge as administrator to change DNS",
                    ),
                );
            } else {
                let doh = DNS_PROVIDERS[self.tools.dns_choice].doh;
                let doh = (self.tools.dns_doh && !doh.is_empty()).then_some(doh);
                self.spawn_task(move || match probe::set_dns(&servers, doh) {
                    Ok(()) => match l {
                        Lang::Uk => format!("DNS змінено: {name}"),
                        Lang::En => format!("DNS changed: {name}"),
                    },
                    Err(e) => format!("{}: {e}", l.tr("Не вдалося змінити DNS", "Could not change DNS")),
                });
                // Обновить «Зараз» после применения.
                self.tools.dns_current = None;
            }
        }
        if flush {
            self.spawn_task(move || match probe::flush_dns() {
                Ok(()) => l.tr("Кеш DNS очищено", "DNS cache flushed").to_owned(),
                Err(e) => format!("{}: {e}", l.tr("Помилка", "Error")),
            });
        }
        if let Some(host) = start_ping {
            if !probe::valid_host(&host) {
                self.notify(ui.ctx(), l.tr("Введи IP-адресу або сайт без пробілів", "Enter an IP address or website without spaces"));
            } else {
                self.tools.ping_host = host.clone();
                self.start_ping(host);
            }
        }
        if shodan {
            let host = self.tools.ping_host.trim().to_owned();
            if host.parse::<std::net::IpAddr>().is_ok() {
                sys::open_url(&format!("https://www.shodan.io/host/{host}"));
            } else {
                self.notify(ui.ctx(), l.tr("Shodan шукає за IP-адресою", "Shodan looks up IP addresses"));
            }
        }
    }
}

fn mono(ui: &mut Ui, text: &str, color: eframe::egui::Color32) {
    ui.label(RichText::new(text).monospace().size(13.0).color(color));
}
