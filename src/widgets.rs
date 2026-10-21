// Theme, small widgets and hand-painted icons.
use crate::fig::Tip as _;
use egui::{Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};

use crate::glyphs::{Glyph, paint_glyph_sized};
use crate::model::Kind;
use crate::{BG, BG_MENU, BG_PRESSED, BG_SECONDARY, BRAND, R_MEDIUM, TEXT, TEXT_TERTIARY};

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Move,
    Pen,
    Pencil,
    Hand,
    Comment,
    Shape(Kind),
    Glyph(Glyph),
}

pub fn apply_theme(ctx: &egui::Context) {
    // Inter (OFL, see assets/fonts/OFL.txt) is Figma's UI font
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("Inter".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Inter.ttf"))));
    if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        f.insert(0, "Inter".into());
    }
    // a static SemiBold instance (weight 600) of the same font: egui cannot pick a weight from the variable one
    fonts.font_data.insert("InterSemiBold".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-SemiBold.ttf"))));
    let fallback = fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.insert(egui::FontFamily::Name("semibold".into()), std::iter::once("InterSemiBold".to_owned()).chain(fallback).collect());
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|s| {
        // egui's debug builds outline a widget in red for a frame whenever the widget at a rect changes id (a label turning
        // into a text field, a menu opening in place): normal for this UI, so the warning is off
        s.debug.warn_if_rect_changes_id = false;
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(8.0, 4.0);
        s.spacing.interact_size.y = 24.0;
        for (style, size) in [
            (egui::TextStyle::Body, 11.0),
            (egui::TextStyle::Button, 11.0),
            (egui::TextStyle::Small, 9.5),
            (egui::TextStyle::Heading, 13.0),
            (egui::TextStyle::Monospace, 11.0),
        ] {
            let family = s.text_styles[&style].family.clone();
            s.text_styles.insert(style, FontId::new(size, family));
        }
        // Figma's menus run their rows edge to edge: no side padding, 8px above the first row and 6px below the last (measured)
        s.spacing.menu_margin = egui::Margin { left: 0, right: 0, top: 8, bottom: 6 };
    });
    let mut v = if crate::theme::is_light() { egui::Visuals::light() } else { egui::Visuals::dark() };
    v.panel_fill = BG();
    v.window_fill = BG_MENU();
    v.extreme_bg_color = BG_SECONDARY(); // text inputs
    v.faint_bg_color = BG_SECONDARY();
    v.window_stroke = Stroke::new(1.0, BG_PRESSED());
    // selected text: white on the brand blue (an assumption from Figma's inputs, not measured yet)
    v.selection.bg_fill = BRAND;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.window_corner_radius = egui::CornerRadius::same(crate::R_LARGE);
    v.menu_corner_radius = egui::CornerRadius::same(crate::R_MENU);
    let r = egui::CornerRadius::same(R_MEDIUM);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BG();
    w.noninteractive.bg_stroke = Stroke::new(1.0, BG_PRESSED());
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT());
    w.inactive.bg_fill = BG_SECONDARY();
    w.inactive.weak_bg_fill = Color32::TRANSPARENT; // buttons are flat until hovered
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT());
    w.hovered.bg_fill = BG_SECONDARY();
    w.hovered.weak_bg_fill = BG_SECONDARY();
    w.hovered.bg_stroke = Stroke::NONE;
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT());
    w.active.bg_fill = BG_PRESSED();
    w.active.weak_bg_fill = BG_PRESSED();
    w.active.bg_stroke = Stroke::NONE;
    w.active.fg_stroke = Stroke::new(1.0, TEXT());
    w.open.weak_bg_fill = BG_PRESSED();
    for wv in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        wv.corner_radius = r;
        wv.expansion = 0.0;
    }
    ctx.set_visuals_of(egui::Theme::Dark, v);
}

#[allow(dead_code)]
pub fn small_button(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add(egui::Button::new(egui::RichText::new(text).size(11.0)).small()).clicked()
}

pub fn paint_icon(p: &egui::Painter, c: Pos2, icon: Icon, color: Color32) {
    let g = match icon {
        Icon::Move => Glyph::Move,
        Icon::Pen => Glyph::Pen,
        Icon::Pencil => Glyph::Pencil,
        Icon::Hand => Glyph::Hand,
        Icon::Comment => Glyph::Comment,
        Icon::Glyph(g) => g,
        Icon::Shape(k) => match k {
            Kind::Frame => Glyph::Frame,
            Kind::Group => Glyph::Group,
            Kind::Rect => Glyph::Rect,
            Kind::Ellipse => Glyph::Ellipse,
            Kind::Line => Glyph::Line,
            Kind::Text => Glyph::Type,
            Kind::Polygon => Glyph::Polygon,
            Kind::Star => Glyph::Star,
            Kind::Path => Glyph::Vector,
        },
    };
    paint_glyph_sized(p, c, g, color, 16.0);
}


pub fn paint_eye(p: &egui::Painter, c: Pos2, color: Color32, hidden: bool) {
    paint_glyph_sized(p, c, if hidden { Glyph::EyeOff } else { Glyph::Eye }, color, 15.0);
}

pub fn paint_lock(p: &egui::Painter, c: Pos2, color: Color32, locked: bool) {
    paint_glyph_sized(p, c, if locked { Glyph::Lock } else { Glyph::LockOpen }, color, 15.0);
}

pub fn tool_button(ui: &mut egui::Ui, selected: bool, icon: Icon, tip: &str, key: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
    let bg = if selected {
        BRAND
    } else if resp.hovered() {
        BG_SECONDARY()
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, R_MEDIUM, bg);
    paint_icon(ui.painter(), rect.center() - Vec2::new(0.0, 1.0), icon, TEXT());
    if !key.is_empty() {
        ui.painter().text(rect.right_bottom() - Vec2::new(4.0, 2.0), Align2::RIGHT_BOTTOM, key, FontId::proportional(8.5), if selected { TEXT() } else { TEXT_TERTIARY() });
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, tip));
    resp.tip(tip)
}

// a DragValue with an accessible name that also takes a number set from outside (UI Automation's
// Value / RangeValue patterns), which egui's DragValue ignores; `make` configures the DragValue
pub fn number(ui: &mut egui::Ui, v: &mut f32, name: &str, make: impl FnOnce(&mut f32) -> egui::DragValue<'_>) -> egui::Response {
    let mut resp = ui.add(make(v));
    let set = ui.input(|i| {
        i.accesskit_action_requests(resp.id, egui::accesskit::Action::SetValue).find_map(|r| match &r.data {
            Some(egui::accesskit::ActionData::NumericValue(x)) => Some(*x as f32),
            Some(egui::accesskit::ActionData::Value(s)) => {
                let digits: String = s.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-')).collect();
                digits.parse().ok()
            }
            _ => None,
        })
    });
    if let Some(x) = set {
        *v = x;
        resp.mark_changed();
        // leave text editing, or the field keeps showing what it had before
        ui.memory_mut(|m| m.surrender_focus(resp.id));
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, name));
    resp
}

// a one-line text field with an accessible name; it also takes values set from outside
// (UI Automation's Value pattern, screen readers), which egui's TextEdit ignores on its own
#[allow(dead_code)]
pub fn text_field(ui: &mut egui::Ui, text: &mut String, name: &str, hint: &str, width: f32) -> egui::Response {
    let mut resp = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(width));
    let set = ui.input(|i| {
        i.accesskit_action_requests(resp.id, egui::accesskit::Action::SetValue).find_map(|r| match &r.data {
            Some(egui::accesskit::ActionData::Value(v)) => Some(v.to_string()),
            _ => None,
        })
    });
    if let Some(v) = set {
        *text = v;
        resp.mark_changed();
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, name));
    resp
}
