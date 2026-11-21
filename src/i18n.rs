//! Языки интерфейса: украинский и английский.
//!
//! Текущий язык хранится в памяти egui, поэтому любой виджет может узнать его
//! через `i18n::current(ui.ctx())`, не передавая параметр по цепочке.

use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    #[default]
    Uk,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Uk, Lang::En];

    /// Выбирает строку для текущего языка.
    pub fn tr<'a>(self, uk: &'a str, en: &'a str) -> &'a str {
        match self {
            Lang::Uk => uk,
            Lang::En => en,
        }
    }

    /// Выбирает строку из пары (uk, en).
    pub fn pick(self, pair: (&'static str, &'static str)) -> &'static str {
        self.tr(pair.0, pair.1)
    }

    /// Название языка (на самом этом языке).
    pub fn name(self) -> &'static str {
        match self {
            Lang::Uk => "Українська",
            Lang::En => "English",
        }
    }
}

fn storage_id() -> egui::Id {
    egui::Id::new("winforge-ui-language")
}

pub fn set_current(ctx: &egui::Context, lang: Lang) {
    ctx.data_mut(|d| d.insert_temp(storage_id(), lang));
}

pub fn current(ctx: &egui::Context) -> Lang {
    ctx.data(|d| d.get_temp::<Lang>(storage_id()))
        .unwrap_or_default()
}
