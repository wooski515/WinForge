//! Свои виджеты: переключатели, карточки, бейджи риска, пункты меню.
//! Всё рисуется вручную, чтобы интерфейс выглядел аккуратно и одинаково.

use std::sync::Arc;

use eframe::egui::{
    self, pos2,
    text::{LayoutJob, TextWrapping},
    vec2, Align2, Color32, CursorIcon, FontId, Galley, Margin, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, TextFormat, Ui, Vec2,
};

use crate::catalog::Risk;
use crate::i18n::{self, Lang};
use crate::theme::{self, *};

/// Смешивание двух цветов: t = 0 → a, t = 1 → b.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}

pub fn risk_color(risk: Risk) -> Color32 {
    match risk {
        Risk::Safe => SAFE,
        Risk::Moderate => MODERATE,
        Risk::Danger => DANGER,
    }
}

pub fn risk_label(risk: Risk, lang: Lang) -> &'static str {
    match risk {
        Risk::Safe => lang.tr("Безпечно", "Safe"),
        Risk::Moderate => lang.tr("Обережно", "Caution"),
        Risk::Danger => lang.tr("Небезпечно", "Risky"),
    }
}

/// Текст в одну строку, обрезанный многоточием, если не влезает.
fn one_line(ui: &Ui, text: &str, font: FontId, color: Color32, max_width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap = TextWrapping::truncate_at_width(max_width.max(10.0));
    ui.fonts(|f| f.layout_job(job))
}

// ---------------------------------------------------------------------------
// Заголовки, карточки, баннеры
// ---------------------------------------------------------------------------

pub fn page_header(ui: &mut Ui, icon: &str, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon).size(32.0));
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(
                RichText::new(title)
                    .font(FontId::new(26.0, theme::bold()))
                    .color(TEXT),
            );
            ui.label(RichText::new(subtitle).size(14.0).color(MUTED));
        });
    });
    ui.add_space(18.0);
}

pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(FontId::new(17.0, theme::bold()))
            .color(TEXT),
    );
}

/// Карточка на всю ширину.
pub fn card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(12.0)
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Обычная раскладка сверху вниз: без неё в ui.columns текст
            // растягивается по ширине (justify) и выглядит «в разрядку».
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add_contents)
                .inner
        })
        .inner
}

/// Цветная плашка с пояснением.
pub fn banner(ui: &mut Ui, icon: &str, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.10))
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.45)))
        .corner_radius(10.0)
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(format!("{icon}  {text}")).color(mix(TEXT, color, 0.35)));
        });
}

pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::WHITE).size(15.0))
            .fill(ACCENT)
            .min_size(vec2(0.0, 34.0)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

pub fn secondary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(text).size(15.0)).min_size(vec2(0.0, 34.0)))
        .on_hover_cursor(CursorIcon::PointingHand)
}

pub fn big_button(ui: &mut Ui, text: &str, fill: Color32) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::WHITE).size(17.0))
            .fill(fill)
            .min_size(vec2(ui.available_width(), 46.0)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Переключатель и бейдж риска
// ---------------------------------------------------------------------------

fn paint_toggle(ui: &Ui, rect: Rect, id: egui::Id, on: bool, hovered: bool) {
    let t = ui.ctx().animate_bool(id.with("toggle"), on);
    let radius = rect.height() / 2.0;
    let painter = ui.painter();
    painter.rect_filled(rect, radius, mix(Color32::from_rgb(58, 64, 78), ACCENT, t));
    if hovered {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, Color32::from_white_alpha(70)),
            StrokeKind::Outside,
        );
    }
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    painter.circle_filled(pos2(x, rect.center().y), radius - 4.0, Color32::WHITE);
}

fn pill_galley(ui: &Ui, text: &str, color: Color32) -> Arc<Galley> {
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), FontId::proportional(12.0), color))
}

fn pill_size(galley: &Galley) -> Vec2 {
    galley.size() + vec2(18.0, 6.0)
}

fn paint_pill(ui: &Ui, rect: Rect, galley: Arc<Galley>, color: Color32) {
    ui.painter().rect(
        rect,
        rect.height() / 2.0,
        color.gamma_multiply(0.14),
        Stroke::new(1.0_f32, color.gamma_multiply(0.45)),
        StrokeKind::Inside,
    );
    let pos = rect.center() - galley.size() / 2.0;
    ui.painter().galley(pos, galley, color);
}

/// Цветная «пилюля» с текстом (бейдж).
pub fn pill(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let galley = pill_galley(ui, text, color);
    let (rect, response) = ui.allocate_exact_size(pill_size(&galley), Sense::hover());
    paint_pill(ui, rect, galley, color);
    response
}

pub fn risk_badge(ui: &mut Ui, risk: Risk) -> Response {
    let lang = i18n::current(ui.ctx());
    pill(ui, risk_label(risk, lang), risk_color(risk))
}

pub fn risk_legend(ui: &mut Ui) {
    let l = i18n::current(ui.ctx());
    risk_badge(ui, Risk::Safe).on_hover_text(l.tr(
        "Нічого не ламає, можна вмикати сміливо",
        "Breaks nothing, safe to turn on",
    ));
    risk_badge(ui, Risk::Moderate).on_hover_text(l.tr(
        "Може вимкнути якусь функцію Windows",
        "May turn off a Windows feature",
    ));
    risk_badge(ui, Risk::Danger).on_hover_text(l.tr(
        "Знижує безпеку або стабільність. Попросить підтвердження",
        "Lowers security or stability. Asks for confirmation",
    ));
}

/// Строка настройки: название, описание, (бейдж риска) и переключатель справа.
/// Кликабельна вся строка.
pub fn setting_row(
    ui: &mut Ui,
    title: &str,
    description: &str,
    on: bool,
    risk: Option<Risk>,
    divider: bool,
) -> Response {
    let lang = i18n::current(ui.ctx());
    let badge = risk.map(|r| (risk_label(r, lang), risk_color(r)));
    setting_row_badge(ui, title, description, on, badge, divider)
}

/// То же, что `setting_row`, но с произвольным бейджем (текст и цвет).
pub fn setting_row_badge(
    ui: &mut Ui,
    title: &str,
    description: &str,
    on: bool,
    badge: Option<(&str, Color32)>,
    divider: bool,
) -> Response {
    let height = if description.is_empty() { 42.0 } else { 54.0 };
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter()
                .rect_filled(rect.expand2(vec2(8.0, 0.0)), 6.0, CARD_HOVER);
        }

        let toggle_rect =
            Rect::from_center_size(pos2(rect.right() - 24.0, rect.center().y), vec2(44.0, 24.0));
        paint_toggle(ui, toggle_rect, response.id, on, hovered);

        let mut text_right = toggle_rect.left() - 16.0;
        if let Some((text, color)) = badge {
            let galley = pill_galley(ui, text, color);
            let size = pill_size(&galley);
            let badge_rect =
                Rect::from_min_size(pos2(text_right - size.x, rect.center().y - size.y / 2.0), size);
            paint_pill(ui, badge_rect, galley, color);
            text_right = badge_rect.left() - 16.0;
        }

        let max_width = text_right - rect.left();
        let title_color = if on { TEXT } else { mix(TEXT, MUTED, 0.35) };
        let title_g = one_line(ui, title, FontId::proportional(15.0), title_color, max_width);
        if description.is_empty() {
            let y = rect.center().y - title_g.size().y / 2.0;
            ui.painter().galley(pos2(rect.left(), y), title_g, TEXT);
        } else {
            let desc_g = one_line(ui, description, FontId::proportional(12.5), MUTED, max_width);
            let title_h = title_g.size().y;
            let top = rect.center().y - (title_h + 2.0 + desc_g.size().y) / 2.0;
            ui.painter().galley(pos2(rect.left(), top), title_g, TEXT);
            ui.painter()
                .galley(pos2(rect.left(), top + title_h + 2.0), desc_g, MUTED);
        }

        if divider {
            ui.painter()
                .hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0_f32, DIVIDER));
        }
    }

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Всплывающая подсказка с подробностями о твике.
pub fn item_tooltip(
    ui: &mut Ui,
    name: &str,
    description: &str,
    details: &[String],
    warning: Option<&str>,
    recommended: bool,
) {
    ui.set_max_width(460.0);
    ui.label(RichText::new(name).strong().color(TEXT));
    if !description.is_empty() {
        ui.label(RichText::new(description).color(MUTED));
    }
    if recommended {
        let l = i18n::current(ui.ctx());
        ui.label(RichText::new(l.tr("★ Рекомендовано", "★ Recommended")).size(12.5).color(SAFE));
    }
    if !details.is_empty() {
        ui.add_space(4.0);
        let l = i18n::current(ui.ctx());
        ui.label(RichText::new(l.tr("Що зміниться:", "What changes:")).size(12.5).color(MUTED));
        for d in details {
            ui.label(RichText::new(format!("• {d}")).monospace().size(12.0).color(TEXT));
        }
    }
    if let Some(w) = warning {
        ui.add_space(4.0);
        ui.label(RichText::new(format!("⚠ {w}")).color(DANGER));
    }
}

// ---------------------------------------------------------------------------
// Карточки выбора, плитки, пункты меню
// ---------------------------------------------------------------------------

/// Карточка с галочкой (программы, встроенные приложения).
pub fn select_card(
    ui: &mut Ui,
    width: f32,
    title: &str,
    subtitle: &str,
    selected: bool,
    risk: Option<Risk>,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 60.0), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let t = ui.ctx().animate_bool(response.id, selected);
        let painter = ui.painter();

        let base = if hovered { CARD_HOVER } else { CARD };
        let stroke = if selected {
            Stroke::new(1.5_f32, ACCENT)
        } else if hovered {
            Stroke::new(1.0_f32, BORDER_HOVER)
        } else {
            Stroke::new(1.0_f32, BORDER)
        };
        painter.rect(rect, 10.0, mix(base, ACCENT, 0.16 * t), stroke, StrokeKind::Inside);

        // Галочка
        let check = Rect::from_center_size(pos2(rect.left() + 22.0, rect.center().y), vec2(18.0, 18.0));
        painter.rect(
            check,
            5.0,
            mix(INPUT_BG, ACCENT, t),
            Stroke::new(1.5_f32, mix(MUTED, ACCENT, t)),
            StrokeKind::Inside,
        );
        if t > 0.01 {
            let stroke = Stroke::new(2.0_f32, Color32::WHITE.gamma_multiply(t));
            let a = pos2(check.left() + 4.0, check.center().y);
            let b = pos2(check.left() + 7.5, check.bottom() - 4.5);
            let c = pos2(check.right() - 4.0, check.top() + 5.0);
            painter.line_segment([a, b], stroke);
            painter.line_segment([b, c], stroke);
        }

        // Точка риска в углу (для «осторожно» и «опасно»)
        let x = rect.left() + 42.0;
        let mut right = rect.right() - 12.0;
        if let Some(risk) = risk {
            if risk != Risk::Safe {
                painter.circle_filled(
                    pos2(rect.right() - 14.0, rect.top() + 14.0),
                    4.0,
                    risk_color(risk),
                );
                right -= 14.0;
            }
        }

        let max_width = right - x;
        let title_g = one_line(ui, title, FontId::proportional(15.0), TEXT, max_width);
        let sub_g = one_line(ui, subtitle, FontId::proportional(12.0), MUTED, max_width);
        let title_h = title_g.size().y;
        let top = rect.center().y - (title_h + 2.0 + sub_g.size().y) / 2.0;
        painter.galley(pos2(x, top), title_g, TEXT);
        painter.galley(pos2(x, top + title_h + 2.0), sub_g, MUTED);
    }

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Сетка карточек выбора. `items`: (id, заголовок, подзаголовок, выбрано, риск, подсказка).
/// Возвращает id карточек, по которым кликнули.
pub fn card_grid<'a>(
    ui: &mut Ui,
    min_card_width: f32,
    items: impl IntoIterator<Item = GridItem<'a>>,
) -> Vec<String> {
    let items: Vec<GridItem<'a>> = items.into_iter().collect();
    let gap = 10.0;
    let available = ui.available_width();
    let columns = (((available + gap) / (min_card_width + gap)).floor() as usize).max(1);
    let width = ((available - gap * (columns as f32 - 1.0)) / columns as f32).floor();

    let mut clicked = Vec::new();
    for row in items.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for item in row {
                let mut response =
                    select_card(ui, width, item.title, item.subtitle, item.selected, item.risk);
                if let Some(hover) = &item.hover {
                    response = response.on_hover_ui(|ui| hover(ui));
                }
                if response.clicked() {
                    clicked.push(item.id.to_owned());
                }
            }
        });
        ui.add_space(gap - ui.spacing().item_spacing.y);
    }
    clicked
}

pub struct GridItem<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub subtitle: &'a str,
    pub selected: bool,
    pub risk: Option<Risk>,
    pub hover: Option<Box<dyn Fn(&mut Ui) + 'a>>,
}

/// Большая кликабельная карточка-действие на главной.
pub fn action_card(ui: &mut Ui, icon: &str, title: &str, description: &str, cta: &str) -> Response {
    let width = ui.available_width();
    let pad = 18.0;
    let desc_g = ui.painter().layout(
        description.to_owned(),
        FontId::proportional(13.5),
        MUTED,
        width - pad * 2.0,
    );
    let height = (pad + 40.0 + 12.0 + 26.0 + desc_g.size().y + 16.0 + 20.0 + pad).max(200.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let painter = ui.painter();
        painter.rect(
            rect,
            12.0,
            if hovered { CARD_HOVER } else { CARD },
            Stroke::new(1.0_f32, if hovered { ACCENT.gamma_multiply(0.7) } else { BORDER }),
            StrokeKind::Inside,
        );
        let x = rect.left() + pad;
        let mut y = rect.top() + pad;

        let bubble = Rect::from_min_size(pos2(x, y), vec2(40.0, 40.0));
        painter.rect_filled(bubble, 10.0, mix(CARD, ACCENT, 0.22));
        painter.text(bubble.center(), Align2::CENTER_CENTER, icon, FontId::proportional(20.0), TEXT);
        y += 52.0;

        painter.text(pos2(x, y), Align2::LEFT_TOP, title, FontId::new(17.0, theme::bold()), TEXT);
        y += 26.0;
        painter.galley(pos2(x, y), desc_g, MUTED);

        painter.text(
            pos2(x, rect.bottom() - pad),
            Align2::LEFT_BOTTOM,
            format!("{cta}  →"),
            FontId::proportional(14.5),
            ACCENT,
        );
    }

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Плитка со счётчиком (сколько программ, твиков и т.д.).
pub fn stat_tile(ui: &mut Ui, value: usize, label: &str, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 88.0), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let painter = ui.painter();
        painter.rect(
            rect,
            12.0,
            if hovered { CARD_HOVER } else { CARD },
            Stroke::new(1.0_f32, BORDER),
            StrokeKind::Inside,
        );
        painter.rect_filled(
            Rect::from_min_size(rect.left_top() + vec2(0.0, 20.0), vec2(3.0, rect.height() - 40.0)),
            2.0,
            color,
        );
        painter.text(
            rect.left_top() + vec2(20.0, 12.0),
            Align2::LEFT_TOP,
            value.to_string(),
            FontId::new(30.0, theme::bold()),
            color,
        );
        painter.text(
            rect.left_top() + vec2(20.0, 56.0),
            Align2::LEFT_TOP,
            label,
            FontId::proportional(13.0),
            MUTED,
        );
    }

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Пункт бокового меню.
pub fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool, badge: Option<usize>) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, 8.0, mix(SIDEBAR, ACCENT, 0.20));
            painter.rect_filled(
                Rect::from_min_size(pos2(rect.left(), rect.top() + 8.0), vec2(3.0, 20.0)),
                2.0,
                ACCENT,
            );
        } else if response.hovered() {
            painter.rect_filled(rect, 8.0, CARD_HOVER);
        }

        let fg = if selected { TEXT } else { mix(MUTED, TEXT, 0.4) };
        painter.text(
            pos2(rect.left() + 16.0, rect.center().y),
            Align2::LEFT_CENTER,
            icon,
            FontId::proportional(16.0),
            if selected { ACCENT } else { fg },
        );
        painter.text(
            pos2(rect.left() + 44.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(15.0),
            fg,
        );

        if let Some(count) = badge.filter(|n| *n > 0) {
            let galley = ui.fonts(|f| {
                f.layout_no_wrap(count.to_string(), FontId::proportional(12.0), TEXT)
            });
            let size = vec2((galley.size().x + 14.0).max(24.0), 20.0);
            let pill = Rect::from_center_size(
                pos2(rect.right() - 12.0 - size.x / 2.0, rect.center().y),
                size,
            );
            painter.rect_filled(
                pill,
                10.0,
                if selected { ACCENT } else { Color32::from_rgb(44, 50, 62) },
            );
            painter.galley(pill.center() - galley.size() / 2.0, galley, TEXT);
        }
    }

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Иконка WinForge 128×128 RGBA (assets/make_icon.py): окно и логотип в меню.
pub const ICON_128: &[u8] = include_bytes!("../assets/icon_128.rgba");

/// Текстура иконки (загружается один раз и хранится в памяти egui).
fn icon_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("winforge-icon");
    if let Some(tex) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return tex;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([128, 128], ICON_128);
    let tex = ctx.load_texture("winforge-icon", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

pub fn logo(ui: &mut Ui) {
    ui.horizontal(|ui| {
        let tex = icon_texture(ui.ctx());
        ui.add(egui::Image::new(&tex).fit_to_exact_size(vec2(40.0, 40.0)));
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let l = i18n::current(ui.ctx());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("WinForge")
                        .font(FontId::new(19.0, theme::bold()))
                        .color(TEXT),
                );
                if crate::sys::is_elevated() {
                    pill(ui, l.tr("АДМІН", "ADMIN"), SAFE).on_hover_text(l.tr(
                        "Запущено з правами адміністратора: можна змінювати систему.",
                        "Running as administrator: system changes are allowed.",
                    ));
                }
            });
            ui.label(
                RichText::new(l.tr("конструктор Windows", "Windows builder"))
                    .size(12.0)
                    .color(MUTED),
            );
        });
    });
}

/// Размер в МБ → «850 МБ» / «1.4 ГБ».
pub fn format_size(mb: u64, lang: Lang) -> String {
    if mb >= 1024 {
        format!("{:.1} {}", mb as f64 / 1024.0, lang.tr("ГБ", "GB"))
    } else {
        format!("{mb} {}", lang.tr("МБ", "MB"))
    }
}

/// Карточка со строками «подпись — значение».
pub fn info_card(ui: &mut Ui, icon: &str, title: &str, rows: &[(&str, String)]) {
    card(ui, |ui| {
        section_title(ui, &format!("{icon}  {title}"));
        ui.add_space(4.0);
        egui::Grid::new(("info", title))
            .num_columns(2)
            .spacing([18.0, 6.0])
            .show(ui, |ui| {
                for (label, value) in rows {
                    ui.label(RichText::new(*label).color(MUTED));
                    ui.label(RichText::new(value).color(TEXT));
                    ui.end_row();
                }
            });
    });
}

/// Строка таблицы «подпись — выпадающий список».
pub fn combo_row(ui: &mut Ui, label: &str, id: &str, value: &mut usize, options: &[&str]) {
    ui.label(RichText::new(label).color(MUTED));
    egui::ComboBox::from_id_salt(id)
        .width(340.0)
        .selected_text(options[*value])
        .show_ui(ui, |ui| {
            for (i, option) in options.iter().enumerate() {
                ui.selectable_value(&mut *value, i, *option);
            }
        });
    ui.end_row();
}

#[cfg(test)]
mod icon_tests {
    #[test]
    fn icon_is_128_square_rgba() {
        assert_eq!(super::ICON_128.len(), 128 * 128 * 4);
    }
}
