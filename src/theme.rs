//! Цвета, шрифты и общий стиль интерфейса.

use eframe::egui::{
    self, vec2, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};

pub const BG: Color32 = Color32::from_rgb(17, 19, 24);
pub const SIDEBAR: Color32 = Color32::from_rgb(22, 25, 31);
pub const CARD: Color32 = Color32::from_rgb(27, 31, 39);
pub const CARD_HOVER: Color32 = Color32::from_rgb(34, 39, 49);
pub const INPUT_BG: Color32 = Color32::from_rgb(14, 16, 21);
pub const BORDER: Color32 = Color32::from_rgb(44, 50, 62);
pub const BORDER_HOVER: Color32 = Color32::from_rgb(70, 78, 96);
pub const DIVIDER: Color32 = Color32::from_rgb(36, 41, 51);
pub const TEXT: Color32 = Color32::from_rgb(232, 234, 240);
pub const MUTED: Color32 = Color32::from_rgb(140, 148, 166);
pub const ACCENT: Color32 = Color32::from_rgb(72, 140, 255);
pub const SAFE: Color32 = Color32::from_rgb(64, 196, 124);
pub const MODERATE: Color32 = Color32::from_rgb(236, 172, 62);
pub const DANGER: Color32 = Color32::from_rgb(236, 88, 88);

/// Полужирное семейство шрифтов для заголовков.
pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_theme(egui::Theme::Dark);

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.extreme_bg_color = INPUT_BG;
    v.faint_bg_color = CARD;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);

    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);

    let button = Color32::from_rgb(38, 43, 54);
    v.widgets.inactive.weak_bg_fill = button;
    v.widgets.inactive.bg_fill = button;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);

    let hover = Color32::from_rgb(50, 57, 72);
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, BORDER_HOVER);

    let active = Color32::from_rgb(60, 68, 86);
    v.widgets.active.weak_bg_fill = active;
    v.widgets.active.bg_fill = active;

    v.widgets.open.weak_bg_fill = hover;

    ctx.style_mut(|style| {
        style.visuals = v;
        style.text_styles = [
            (TextStyle::Heading, FontId::new(26.0, bold())),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::proportional(14.5)),
            (TextStyle::Small, FontId::proportional(12.5)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(12.0, 5.0);
        style.spacing.interact_size.y = 28.0;
    });
}

/// Подключает Segoe UI и Consolas из Windows (с кириллицей и в привычном виде).
/// Если файлов нет, остаются встроенные шрифты egui.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    if add_font(&mut fonts, "segoe", r"C:\Windows\Fonts\segoeui.ttf") {
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe".into());
    }
    if add_font(&mut fonts, "consolas", r"C:\Windows\Fonts\consola.ttf") {
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "consolas".into());
    }

    let mut bold_list = fonts.families[&FontFamily::Proportional].clone();
    if add_font(&mut fonts, "segoe-semibold", r"C:\Windows\Fonts\seguisb.ttf") {
        bold_list.insert(0, "segoe-semibold".into());
    }
    fonts.families.insert(bold(), bold_list);

    ctx.set_fonts(fonts);
}

fn add_font(fonts: &mut FontDefinitions, name: &str, path: &str) -> bool {
    match std::fs::read(path) {
        Ok(bytes) => {
            fonts
                .font_data
                .insert(name.to_owned(), FontData::from_owned(bytes).into());
            true
        }
        Err(_) => false,
    }
}
