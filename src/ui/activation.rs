//! Вкладка «Активація»: свой ключ продукту и KMS-сервер своей организации.

use eframe::egui::{self, RichText, Ui};

use crate::app::WinForgeApp;
use crate::sys::{self, activation};
use crate::theme::*;
use crate::widgets;

impl WinForgeApp {
    fn load_activation_status(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(activation::status());
        });
        self.tools.activation_rx = Some(rx);
    }

    fn start_activation(&mut self, key: Option<String>, kms: Option<(String, Option<u16>)>) {
        let l = self.lang;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = activation::activate(key.as_deref(), kms.as_ref().map(|(h, p)| (h.as_str(), *p)));
            let message = match &result {
                Ok(Some(st)) if st.activated() => (true, l.tr("Windows активовано", "Windows is activated").to_owned()),
                Ok(_) => (
                    false,
                    l.tr(
                        "Ключ прийнято, але Windows ще не активована. Перевір підключення й спробуй пізніше.",
                        "The key was accepted, but Windows is not activated yet. Check the connection and try again later.",
                    )
                    .to_owned(),
                ),
                Err(e) => (false, activation::explain(e, l)),
            };
            let _ = tx.send(message);
        });
        self.tools.activation_job = Some(rx);
    }

    pub fn ui_activation(&mut self, ui: &mut Ui) {
        let l = self.lang;
        widgets::page_header(
            ui,
            "🔑",
            l.tr("Активація Windows", "Windows activation"),
            l.tr(
                "Твій ключ продукту або KMS-сервер твоєї організації.",
                "Your own product key or your organization's KMS server.",
            ),
        );

        // Фоновые результаты.
        if self.tools.activation_status.is_none() && self.tools.activation_rx.is_none() {
            self.load_activation_status();
        }
        if let Some(rx) = &self.tools.activation_rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.tools.activation_status = Some(result);
                    self.tools.activation_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint(),
                Err(_) => self.tools.activation_rx = None,
            }
        }
        let mut finished: Option<(bool, String)> = None;
        if let Some(rx) = &self.tools.activation_job {
            match rx.try_recv() {
                Ok(result) => finished = Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint(),
                Err(_) => finished = Some((false, String::new())),
            }
        }
        if let Some((ok, message)) = finished {
            self.tools.activation_job = None;
            // Состояние после попытки; ключ после успеха не держим в памяти.
            self.tools.activation_status = None;
            if ok {
                self.tools.activation_key.clear();
            }
            if !message.is_empty() {
                self.notify(ui.ctx(), message);
            }
        }

        let elevated = sys::is_elevated();
        let busy = self.tools.activation_job.is_some();
        if !elevated {
            widgets::banner(
                ui,
                "⚠",
                l.tr(
                    "Встановити ключ можна лише з правами адміністратора. Перезапусти WinForge від імені адміністратора.",
                    "Installing a key requires administrator rights. Restart WinForge as administrator.",
                ),
                MODERATE,
            );
            ui.add_space(14.0);
        }

        // ---------- Состояние ----------
        let mut refresh = false;
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::section_title(ui, l.tr("Стан", "Status"));
                if ui.small_button(l.tr("↻ Оновити", "↻ Refresh")).clicked() {
                    refresh = true;
                }
            });
            ui.add_space(4.0);
            match &self.tools.activation_status {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(l.tr("Перевіряємо…", "Checking…")).color(MUTED));
                    });
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(activation::explain(e, l)).color(DANGER));
                }
                Some(Ok(None)) => {
                    ui.label(RichText::new(l.tr("Ключ продукту не встановлено.", "No product key is installed.")).color(MODERATE));
                }
                Some(Ok(Some(st))) => {
                    let color = if st.activated() { SAFE } else { MODERATE };
                    ui.label(RichText::new(st.text(l)).strong().size(16.0).color(color));
                    ui.label(RichText::new(&st.name).color(TEXT));
                    ui.label(
                        RichText::new(format!("{}: …-{}", l.tr("Ключ", "Key"), st.partial_key))
                            .monospace()
                            .color(MUTED),
                    );
                    if st.grace_minutes > 0 {
                        let days = st.grace_minutes / 60 / 24;
                        let text = match l {
                            crate::i18n::Lang::Uk => format!("Залишилось днів: {days}"),
                            crate::i18n::Lang::En => format!("Days left: {days}"),
                        };
                        ui.label(RichText::new(text).color(MUTED));
                    }
                    if !st.kms.is_empty() {
                        ui.label(RichText::new(format!("KMS: {}", st.kms)).monospace().color(MUTED));
                    }
                }
            }
        });
        ui.add_space(14.0);

        // ---------- Ключ ----------
        let mut install_key = false;
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("Свій ключ продукту", "Your product key"));
            ui.label(
                RichText::new(l.tr(
                    "Ключ має підходити до твоєї редакції (Home, Pro…). WinForge його ніде не зберігає.",
                    "The key must match your edition (Home, Pro…). WinForge doesn't store it anywhere.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(6.0);
            ui.add_sized(
                [360.0, 28.0],
                egui::TextEdit::singleline(&mut self.tools.activation_key)
                    .hint_text("XXXXX-XXXXX-XXXXX-XXXXX-XXXXX")
                    .font(egui::TextStyle::Monospace),
            );
            ui.add_space(8.0);
            let label = l.tr("🔑 Встановити ключ і активувати", "🔑 Install the key and activate");
            if ui.add_enabled(elevated && !busy, egui::Button::new(label)).clicked() {
                install_key = true;
            }
        });
        ui.add_space(14.0);

        // ---------- KMS ----------
        let mut use_kms = false;
        widgets::card(ui, |ui| {
            widgets::section_title(ui, l.tr("KMS-сервер організації", "Organization KMS server"));
            ui.label(
                RichText::new(l.tr(
                    "Для ПК у компанії чи навчальному закладі, де є власний KMS-сервер. Адресу дасть адміністратор мережі. Якщо в поле вище вписано ключ, його буде встановлено перед активацією.",
                    "For PCs at a company or school that runs its own KMS server. Your network admin will give you the address. If a key is entered above, it is installed before activating.",
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(6.0);
            ui.add_sized(
                [360.0, 28.0],
                egui::TextEdit::singleline(&mut self.tools.kms_host).hint_text("kms.company.local:1688"),
            );
            ui.add_space(8.0);
            let label = l.tr("🏢 Активувати через KMS", "🏢 Activate via KMS");
            if ui.add_enabled(elevated && !busy, egui::Button::new(label)).clicked() {
                use_kms = true;
            }
            if busy {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(l.tr("Активація…", "Activating…")).color(MUTED));
                });
            }
        });

        // ---------- Действия ----------
        if refresh {
            self.tools.activation_status = None;
        }
        let key_text = self.tools.activation_key.trim().to_owned();
        let key = activation::normalize_key(&key_text);
        if install_key {
            match key {
                Some(key) => self.start_activation(Some(key), None),
                None => self.notify(
                    ui.ctx(),
                    l.tr("Ключ має вигляд XXXXX-XXXXX-XXXXX-XXXXX-XXXXX", "A key looks like XXXXX-XXXXX-XXXXX-XXXXX-XXXXX"),
                ),
            }
        } else if use_kms {
            match activation::parse_kms_host(&self.tools.kms_host) {
                None => self.notify(
                    ui.ctx(),
                    l.tr("Вкажи адресу KMS-сервера, наприклад kms.company.local", "Enter the KMS server address, e.g. kms.company.local"),
                ),
                Some(_) if !key_text.is_empty() && key.is_none() => self.notify(
                    ui.ctx(),
                    l.tr("Ключ має вигляд XXXXX-XXXXX-XXXXX-XXXXX-XXXXX", "A key looks like XXXXX-XXXXX-XXXXX-XXXXX-XXXXX"),
                ),
                Some(host) => self.start_activation(key, Some(host)),
            }
        }
    }
}
