// Figma UI3 panel primitives, measured from the desktop app (REA screenshots at 150%, values in CSS px):
// 24px controls on #383838 with 5px corners, 11px values, 9.5px field labels, 11px semibold section titles,
// 40px section headers, 16px left / 8px right padding, a two-column grid plus a 24px icon column, 1px #444
// dividers. Panels lay rows out top to bottom on the full panel width and place controls by these numbers.
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Response, Sense, Stroke, Vec2};

use crate::glyphs::{paint_glyph, paint_glyph_sized, Glyph};
use crate::{TEXT, TEXT_SECONDARY, TEXT_TERTIARY};

#[allow(non_snake_case)]
pub fn INPUT() -> Color32 {
    crate::theme::FIELD()
}
#[allow(non_snake_case)]
pub fn INPUT_HOVER() -> Color32 {
    crate::theme::FIELD_HOVER()
}
#[allow(non_snake_case)]
pub fn RULE() -> Color32 {
    crate::theme::LINE()
}
#[allow(non_snake_case)]
pub fn SELECTED() -> Color32 {
    crate::theme::SELECTED_BG() // selected layer row, rail item, toggled icon
}
#[allow(non_snake_case)]
pub fn SELECTED_INK() -> Color32 {
    crate::theme::SELECTED_FG() // icon on SELECTED(), active constraint, links
}
pub const SHARE: Color32 = Color32::from_rgb(0x0c, 0x8c, 0xe9);
pub const COMPONENT_INK: Color32 = Color32::from_rgb(0xd1, 0xa8, 0xff);
// Figma's main menu and its submenus (measured 2026-10-18, CSS px): #1e1e1e, no border, 8px padding all round, rows 24px
// tall on a #0c8ce9 highlight inset 8px, label 8px inside the highlight, a 17px gap (8, a 1px #353535 line, 8) between groups
#[allow(non_snake_case)]
pub fn MENU_BG() -> Color32 {
    crate::theme::MENU_SURFACE()
}
#[allow(non_snake_case)]
pub fn MENU_RULE() -> Color32 {
    crate::theme::MENU_LINE()
}
pub const MENU_HOT: Color32 = SHARE;
pub const MENU_PAD: i8 = 8;
pub const MENU_GROUP_GAP: f32 = 17.0;

pub const ROW: f32 = 24.0;
pub const HEADER: f32 = 40.0;
pub const RADIUS: u8 = 5;

// set once the theme's fonts are in place (they take effect the frame after `set_fonts`); until then, and in
// tests that never install them, semibold text falls back to the regular face
pub static FONTS_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn semi(size: f32) -> FontId {
    if FONTS_READY.load(std::sync::atomic::Ordering::Relaxed) {
        FontId::new(size, FontFamily::Name("semibold".into()))
    } else {
        FontId::proportional(size)
    }
}

pub fn reg(size: f32) -> FontId {
    FontId::proportional(size)
}

// the property grid of a panel: two columns, then a 24px icon column
#[derive(Clone, Copy)]
pub struct Grid {
    pub left: f32,
    pub top: f32, // the panel's top (moves with scrolling), so a row is identified by its place in the content
    pub cw: f32, // column width
    pub c2: f32, // second column x
    pub c3: f32, // icon column x
    pub right: f32,
    pub edge: f32, // where a full-width control ends: the same 16px from the panel's edge as `left`
}

type RowKey = (i32, i32);

// which rows used the icon column, and which asked for a second column, in the frame being drawn; `wide` are the rows that asked
// for one and had no icon last frame: their field reaches the right edge instead of leaving the icon column empty
#[derive(Default)]
struct RowState {
    icons: std::collections::HashSet<RowKey>,
    asked: std::collections::HashSet<RowKey>,
    wide: std::collections::HashSet<RowKey>,
}

thread_local! {
    static ROWS: std::cell::RefCell<RowState> = std::cell::RefCell::new(RowState::default());
}

// called at the start of every frame: closes the books of the last one; true when the set of widened rows changed (draw again)
pub fn rows_frame() -> bool {
    ROWS.with(|r| {
        let mut r = r.borrow_mut();
        let wide: std::collections::HashSet<RowKey> = r.asked.difference(&r.icons).copied().collect();
        let changed = wide != r.wide;
        r.wide = wide;
        r.icons.clear();
        r.asked.clear();
        changed
    })
}

impl Grid {
    pub fn of(panel: Rect) -> Grid {
        let cw = ((panel.width() - 64.0) / 2.0).max(40.0);
        let left = panel.left() + 16.0;
        Grid { left, top: panel.top(), cw, c2: left + cw + 8.0, c3: panel.right() - 32.0, right: panel.right() - 8.0, edge: panel.right() - 16.0 }
    }
    fn key(&self, row: Rect) -> RowKey {
        ((row.top() - self.top).round() as i32, self.left.round() as i32)
    }
    // a row that has a second column and no icon: the column runs to the right edge
    fn reaches_right(&self, row: Rect) -> bool {
        let k = self.key(row);
        ROWS.with(|r| {
            let mut r = r.borrow_mut();
            r.asked.insert(k);
            r.wide.contains(&k)
        })
    }
    pub fn col1(&self, row: Rect) -> Rect {
        Rect::from_min_size(Pos2::new(self.left, row.top()), Vec2::new(self.cw, ROW))
    }
    pub fn col2(&self, row: Rect) -> Rect {
        let w = if self.reaches_right(row) { self.edge - self.c2 } else { self.cw };
        Rect::from_min_size(Pos2::new(self.c2, row.top()), Vec2::new(w, ROW))
    }
    pub fn both(&self, row: Rect) -> Rect {
        let right = if self.reaches_right(row) { self.edge } else { self.c2 + self.cw };
        Rect::from_min_max(Pos2::new(self.left, row.top()), Pos2::new(right, row.top() + ROW))
    }
    pub fn icon(&self, row: Rect) -> Rect {
        let k = self.key(row);
        ROWS.with(|r| r.borrow_mut().icons.insert(k));
        Rect::from_min_size(Pos2::new(self.c3, row.top()), Vec2::splat(ROW))
    }
    pub fn full(&self, row: Rect) -> Rect {
        Rect::from_min_max(Pos2::new(self.left, row.top()), Pos2::new(self.edge, row.top() + ROW))
    }
}

// a full-width row of `h` at the cursor
pub fn row(ui: &mut egui::Ui, h: f32) -> Rect {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
    r
}

// a 1px divider across the panel
pub fn rule(ui: &mut egui::Ui) {
    let r = row(ui, 1.0);
    ui.painter().rect_filled(r, 0.0, RULE());
}

// a section title row (40px). `dim` greys the title, as Figma does for empty sections (Stroke, Effects).
// Actions are 24px icon buttons from the right edge, 28px apart; returns the index of a clicked one.
pub fn header(ui: &mut egui::Ui, title: &str, dim: bool, actions: &[(Glyph, &str, bool)]) -> (Response, Vec<Response>) {
    // a section with content and buttons in its title row is 2px taller than an empty one (measured)
    let r = row(ui, if actions.is_empty() || dim { HEADER } else { HEADER + 2.0 });
    let resp = ui.interact(r, ui.id().with(("section", title)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, title));
    ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.top() + 20.0 + TEXT_LIFT * 11.0), Align2::LEFT_CENTER, title, semi(11.0), if dim { TEXT_SECONDARY() } else { TEXT() });
    let buttons = actions
        .iter()
        .enumerate()
        .map(|(k, (g, tip, on))| {
            let b = Rect::from_min_size(Pos2::new(r.right() - 32.0 - 28.0 * (actions.len() - 1 - k) as f32, r.top() + 8.0), Vec2::splat(ROW));
            icon_button(ui, b, *g, tip, *on)
        })
        .collect();
    (resp, buttons)
}

// a plain Figma button: 24px, #383838, centred 11px label
pub fn button(ui: &mut egui::Ui, r: Rect, label: &str) -> Response {
    let resp = ui.interact(r, ui.id().with(("button", label, r.min.y as i32)), Sense::click());
    input_bg(ui, r, resp.hovered());
    ptext(&ui.painter().with_clip_rect(r), r.center(), Align2::CENTER_CENTER, label, reg(11.0), TEXT());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp
}

// the end of a section: 16px, then the divider
pub fn end(ui: &mut egui::Ui) {
    row(ui, 16.0);
    rule(ui);
}

static LABELS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

// View > Property labels: whether the captions over the fields are drawn (their rows stay, so nothing moves)
pub fn set_labels(on: bool) {
    LABELS.store(on, std::sync::atomic::Ordering::Relaxed);
}

// field labels over the two columns, 9.5px secondary, centred 11px above the controls below. The first
// label row under a header is 20px, later ones 24px.
pub fn labels(ui: &mut egui::Ui, g: &Grid, first: bool, a: &str, b: &str) {
    let r = row(ui, if first { 20.0 } else { 24.0 });
    if !LABELS.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let y = r.bottom() - if first { 13.0 } else { 9.0 };
    for (x, t) in [(g.left, a), (g.c2, b)] {
        if !t.is_empty() {
            ptext(ui.painter(), Pos2::new(x, y), Align2::LEFT_CENTER, t, reg(10.0), TEXT_SECONDARY());
        }
    }
}

// a row of controls (24px) after `gap`
pub fn controls(ui: &mut egui::Ui, gap: f32) -> Rect {
    if gap > 0.0 {
        row(ui, gap);
    }
    row(ui, ROW)
}

pub fn input_bg_r(ui: &egui::Ui, r: Rect, radius: u8) {
    ui.painter().rect_filled(r, radius, INPUT());
}

pub fn input_bg(ui: &egui::Ui, r: Rect, hovered: bool) {
    ui.painter().rect_filled(r, RADIUS, if hovered { INPUT_HOVER() } else { INPUT() });
}

// a flat 24px icon button; `on` is the toggled look
pub fn icon_button(ui: &mut egui::Ui, r: Rect, g: Glyph, tip: &str, on: bool) -> Response {
    let resp = ui.interact(r, ui.id().with(("icon", tip, r.min.x as i32, r.min.y as i32)), Sense::click());
    let bg = if on {
        SELECTED()
    } else if resp.hovered() {
        INPUT()
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(r, RADIUS, bg);
    paint_glyph(ui.painter(), r.center(), g, if on { SELECTED_INK() } else { TEXT() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, tip));
    resp.tip(tip)
}

// an icon button that opens a menu, as Figma draws them: the icon and a chevron-down side by side, `MENU_BTN_W` wide
pub const MENU_BTN_W: f32 = 46.0;

pub fn menu_icon_button(ui: &mut egui::Ui, r: Rect, g: Glyph, tip: &str, on: bool) -> Response {
    let r = Rect::from_min_size(r.min, Vec2::new(MENU_BTN_W, r.height()));
    let resp = ui.interact(r, ui.id().with(("menu-icon", tip, r.min.x as i32, r.min.y as i32)), Sense::click());
    let bg = if on {
        SELECTED()
    } else if resp.hovered() {
        INPUT()
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(r, RADIUS, bg);
    let ink = if on { SELECTED_INK() } else { TEXT() };
    paint_glyph(ui.painter(), Pos2::new(r.left() + 13.0, r.center().y), g, ink);
    paint_glyph_sized(ui.painter(), Pos2::new(r.right() - 11.0, r.center().y), Glyph::ChevronDown, ink, 10.0);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, tip));
    resp.tip(tip)
}

// x of the left edge of each button of a header row, laid out from the right edge (menu buttons are wider)
pub fn button_xs(right: f32, menu: &[bool]) -> Vec<f32> {
    let mut x = right;
    let mut out = vec![0.0; menu.len()];
    for k in (0..menu.len()).rev() {
        x -= if menu[k] { MENU_BTN_W } else { ROW };
        out[k] = x;
        x -= 4.0;
    }
    out
}

// what sits at the left of a number field
#[derive(Clone, Copy)]
pub enum Prefix<'a> {
    None,
    Text(&'a str),
    Icon(Glyph),
}

#[derive(Clone, Copy)]
pub struct Num {
    pub speed: f32,
    pub min: f32,
    pub max: f32,
    pub decimals: usize,
    pub suffix: &'static str,
}

impl Default for Num {
    fn default() -> Self {
        Num { speed: 1.0, min: f32::NEG_INFINITY, max: f32::INFINITY, decimals: 2, suffix: "" }
    }
}

pub fn fmt_num(v: f32, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, v);
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_owned() } else { s };
    if s == "-0" { "0".into() } else { s }
}

// Figma's number field: prefix, value; drag sideways to scrub, click to type. `id` must be stable.
// Exposed to accessibility as a named value that also accepts SetValue.
pub fn num_field(ui: &mut egui::Ui, r: Rect, id: egui::Id, prefix: Prefix, v: &mut f32, n: Num, name: &str) -> bool {
    let before = *v;
    let editing: Option<String> = ui.data(|d| d.get_temp(id));
    let resp = ui.interact(r, id, Sense::click_and_drag());
    input_bg(ui, r, resp.hovered() && editing.is_none());
    let p = ui.painter();
    let value_x = match prefix {
        Prefix::None => r.left() + 8.0,
        Prefix::Text(t) => {
            ptext(p, Pos2::new(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, t, reg(11.0), TEXT()); // Figma's X / Y / W / H are white
            r.left() + 24.0
        }
        Prefix::Icon(g) => {
            paint_glyph_sized(p, Pos2::new(r.left() + 12.0, r.center().y), g, TEXT_SECONDARY(), 13.0);
            r.left() + 24.0
        }
    };
    let value_rect = Rect::from_min_max(Pos2::new(value_x - 1.0, r.top()), Pos2::new(r.right() - 4.0, r.bottom()));
    match editing {
        Some(mut text) => {
            let edit_id = id.with("edit");
            let out = put(ui, value_rect, egui::TextEdit::singleline(&mut text).id(edit_id).frame(egui::Frame::NONE).font(reg(11.0)).margin(egui::Margin::ZERO).vertical_align(egui::Align::Center).text_color(TEXT()));
            if !out.has_focus() && !out.lost_focus() {
                out.request_focus();
            }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if out.lost_focus() || enter || esc {
                if !esc {
                    let digits: String = text.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-')).collect();
                    if let Ok(x) = digits.parse::<f32>() {
                        *v = x.clamp(n.min, n.max);
                    }
                }
                ui.data_mut(|d| d.remove::<String>(id));
            } else {
                ui.data_mut(|d| d.insert_temp(id, text));
            }
        }
        None => {
            if resp.dragged() {
                let dx = resp.drag_delta().x;
                if dx != 0.0 {
                    *v = (*v + dx * n.speed).clamp(n.min, n.max);
                }
            } else if resp.clicked() {
                ui.data_mut(|d| d.insert_temp(id, fmt_num(*v, n.decimals)));
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            let shown = format!("{}{}", fmt_num(*v, n.decimals), n.suffix);
            ptext(&ui.painter().with_clip_rect(value_rect), Pos2::new(value_x, r.center().y), Align2::LEFT_CENTER, shown, reg(11.0), TEXT());
        }
    }
    // values set from outside (UI Automation, screen readers)
    let set = ui.input(|i| {
        i.accesskit_action_requests(id, egui::accesskit::Action::SetValue).find_map(|q| match &q.data {
            Some(egui::accesskit::ActionData::NumericValue(x)) => Some(*x as f32),
            Some(egui::accesskit::ActionData::Value(s)) => s.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-')).collect::<String>().parse().ok(),
            _ => None,
        })
    });
    if let Some(x) = set {
        *v = x.clamp(n.min, n.max);
    }
    let value = *v as f64;
    resp.widget_info(|| {
        let mut info = egui::WidgetInfo::drag_value(true, value);
        info.label = Some(name.to_owned());
        info
    });
    // like egui's DragValue: a settable value, also as text, so UI Automation's Value pattern works
    let shown = format!("{}{}", fmt_num(*v, n.decimals), n.suffix);
    ui.ctx().accesskit_node_builder(id, |b| {
        use egui::accesskit::Action;
        if n.min.is_finite() {
            b.set_min_numeric_value(n.min as f64);
        }
        if n.max.is_finite() {
            b.set_max_numeric_value(n.max as f64);
        }
        b.set_numeric_value(value);
        b.set_value(shown);
        b.add_action(Action::SetValue);
    });
    *v != before
}

// a dropdown field: optional icon, the current label, a chevron; a menu of `items` on click
pub fn dropdown<T: PartialEq + Copy>(ui: &mut egui::Ui, r: Rect, icon: Option<Glyph>, cur: &mut T, items: &[(T, &str)], name: &str) -> bool {
    dropdown_styled(ui, r, icon, cur, items, name, false)
}

// the dropdown with Figma's outlined look (the stroke Position field): no fill, a 1px #444 border
pub fn dropdown_outlined<T: PartialEq + Copy>(ui: &mut egui::Ui, r: Rect, icon: Option<Glyph>, cur: &mut T, items: &[(T, &str)], name: &str) -> bool {
    dropdown_styled(ui, r, icon, cur, items, name, true)
}

fn dropdown_styled<T: PartialEq + Copy>(ui: &mut egui::Ui, r: Rect, icon: Option<Glyph>, cur: &mut T, items: &[(T, &str)], name: &str, outlined: bool) -> bool {
    dropdown_shown(ui, r, icon, cur, items, name, outlined, None)
}

// the dropdown whose box shows `text` instead of the current item's name (a summary such as "5 columns")
#[allow(clippy::too_many_arguments)]
pub fn dropdown_shown<T: PartialEq + Copy>(ui: &mut egui::Ui, r: Rect, icon: Option<Glyph>, cur: &mut T, items: &[(T, &str)], name: &str, outlined: bool, text: Option<&str>) -> bool {
    let id = ui.id().with(("dropdown", name, r.min.x as i32));
    let resp = ui.interact(r, id, Sense::click());
    if outlined {
        if resp.hovered() {
            ui.painter().rect_filled(r, RADIUS, INPUT_HOVER().gamma_multiply(0.5));
        }
        ui.painter().rect_stroke(r, RADIUS, Stroke::new(1.0, RULE()), egui::StrokeKind::Inside);
    } else {
        input_bg(ui, r, resp.hovered());
    }
    let label = text.unwrap_or_else(|| items.iter().find(|(v, _)| v == cur).map_or("", |(_, l)| *l));
    let p = ui.painter();
    let tx = match icon {
        Some(g) => {
            paint_glyph_sized(p, Pos2::new(r.left() + 20.3, r.center().y), g, TEXT_SECONDARY(), 13.0);
            r.left() + 37.3 // Figma measures 33.3; 4px more air between the icon and the label, asked for by the user
        }
        None => r.left() + 8.0,
    };
    ptext(&p.with_clip_rect(Rect::from_min_max(r.min, Pos2::new(r.right() - 22.0, r.bottom()))), Pos2::new(tx, r.center().y), Align2::LEFT_CENTER, label, reg(11.0), TEXT());
    paint_glyph_sized(p, Pos2::new(r.right() - 13.0, r.center().y), Glyph::ChevronDown, TEXT(), 9.5);
    let shown = label.to_owned();
    resp.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, name);
        info.current_text_value = Some(shown.clone());
        info
    });
    let mut changed = false;
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(r.width());
        for (v, l) in items {
            if crate::glyphs::menu_option(ui, v == cur, l) {
                changed = *cur != *v;
                *cur = *v;
                ui.close();
            }
        }
    });
    changed
}

// a segmented group of icon buttons filling `r`; returns the clicked index
pub fn segments(ui: &mut egui::Ui, r: Rect, items: &[(Glyph, &str, bool)]) -> Option<usize> {
    let tips: Vec<(&str, bool)> = items.iter().map(|(_, t, on)| (*t, *on)).collect();
    segments_custom(ui, r, &tips, |p, c, k, ink| paint_glyph(p, c, items[k].0, ink))
}

// a segmented group filling `r` whose cells are drawn by `paint(painter, centre, index, ink)`; returns the clicked index
pub fn segments_custom(ui: &mut egui::Ui, r: Rect, items: &[(&str, bool)], paint: impl Fn(&egui::Painter, Pos2, usize, Color32)) -> Option<usize> {
    ui.painter().rect_filled(r, RADIUS, INPUT());
    let w = r.width() / items.len() as f32;
    let mut hit = None;
    for (k, (tip, on)) in items.iter().enumerate() {
        let cell = Rect::from_min_size(Pos2::new(r.left() + w * k as f32, r.top()), Vec2::new(w, r.height()));
        let resp = ui.interact(cell, ui.id().with(("seg", tip, r.min.x as i32, r.min.y as i32)), Sense::click());
        let corners = CornerRadius { nw: if k == 0 { RADIUS } else { 0 }, sw: if k == 0 { RADIUS } else { 0 }, ne: if k + 1 == items.len() { RADIUS } else { 0 }, se: if k + 1 == items.len() { RADIUS } else { 0 } };
        if *on {
            // measured in Figma: the selected cell is the panel colour with a 1px #444 border, icon stays white
            ui.painter().rect_filled(cell, corners, crate::BG());
            ui.painter().rect_stroke(cell, corners, Stroke::new(1.0, RULE()), egui::StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(cell, corners, INPUT_HOVER());
        }
        paint(ui.painter(), cell.center(), k, TEXT());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, *on, *tip));
        if resp.tip(*tip).clicked() {
            hit = Some(k);
        }
    }
    hit
}

// a segmented control of words ("Basic", "Dynamic", "Brush"); cells that are not `enabled` are greyed and inert
pub fn text_segments(ui: &mut egui::Ui, r: Rect, items: &[(&str, bool, bool)]) -> Option<usize> {
    ui.painter().rect_filled(r, RADIUS, INPUT());
    let w = r.width() / items.len() as f32;
    let mut hit = None;
    for (k, (label, on, enabled)) in items.iter().enumerate() {
        let cell = Rect::from_min_size(Pos2::new(r.left() + w * k as f32, r.top()), Vec2::new(w, r.height()));
        let resp = ui.interact(cell, ui.id().with(("tseg", label, r.min.x as i32, r.min.y as i32)), if *enabled { Sense::click() } else { Sense::hover() });
        let corners = CornerRadius { nw: if k == 0 { RADIUS } else { 0 }, sw: if k == 0 { RADIUS } else { 0 }, ne: if k + 1 == items.len() { RADIUS } else { 0 }, se: if k + 1 == items.len() { RADIUS } else { 0 } };
        if *on {
            ui.painter().rect_filled(cell, corners, crate::BG());
            ui.painter().rect_stroke(cell, corners, Stroke::new(1.0, RULE()), egui::StrokeKind::Inside);
        }
        ptext(ui.painter(), cell.center(), Align2::CENTER_CENTER, *label, reg(11.0), if *enabled { if *on { TEXT() } else { TEXT_SECONDARY() } } else { TEXT_TERTIARY() });
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, *enabled, *on, *label));
        if *enabled && resp.clicked() {
            hit = Some(k);
        }
    }
    hit
}

// a 14px color swatch that opens a color picker
pub fn swatch(ui: &mut egui::Ui, center: Pos2, c: &mut Color32, name: &str) -> bool {
    let r = Rect::from_center_size(center, Vec2::splat(14.0));
    let id = ui.id().with(("swatch", name));
    let resp = ui.interact(r, id, Sense::click());
    // a checkerboard under translucent colors
    if c.a() < 255 {
        ui.painter().rect_filled(r, 2.0, Color32::WHITE);
        for k in 0..4 {
            let q = Rect::from_min_size(r.min + Vec2::new((k % 2) as f32 * 7.0, (k / 2) as f32 * 7.0), Vec2::splat(7.0));
            if k % 3 == 0 {
                ui.painter().rect_filled(q, 0.0, Color32::from_gray(200));
            }
        }
    }
    ui.painter().rect_filled(r, 2.0, *c);
    ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_black_alpha(40)), egui::StrokeKind::Inside);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ColorButton, true, name));
    // Figma's picker, to the left of the panel this swatch sits in
    crate::picker::popup(&resp, ui.max_rect().left(), |ui| {
        let mut tab = crate::picker::Tab::Custom;
        if crate::picker::header(ui, &mut tab, false, None).1 {
            ui.close();
        }
        rule(ui);
        crate::picker::body(ui, c, id)
    })
    .unwrap_or(false)
}

// hex text for a color, editable on click
// a typed colour as 0xRRGGBB: six digits, or the short forms Figma expands (fa0 -> ffaa00, ab -> ababab, f -> ffffff)
pub fn parse_hex(text: &str) -> Option<u32> {
    let h = text.trim().trim_start_matches('#');
    if h.is_empty() || h.len() > 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let full: String = match h.len() {
        1 => h.repeat(6),
        2 => h.repeat(3),
        3 => h.chars().flat_map(|c| [c, c]).collect(),
        6 => h.to_owned(),
        _ => return None,
    };
    u32::from_str_radix(&full, 16).ok()
}

pub fn hex_text(ui: &mut egui::Ui, pos: Pos2, width: f32, c: &mut Color32, id: egui::Id) -> bool {
    let r = Rect::from_min_size(Pos2::new(pos.x - 1.0, pos.y - 10.0), Vec2::new(width, 20.0));
    let editing: Option<String> = ui.data(|d| d.get_temp(id));
    match editing {
        Some(mut text) => {
            let out = put(ui, r, egui::TextEdit::singleline(&mut text).id(id.with("edit")).frame(egui::Frame::NONE).font(reg(11.0)).margin(egui::Margin::ZERO).vertical_align(egui::Align::Center).text_color(TEXT()));
            if !out.has_focus() && !out.lost_focus() {
                out.request_focus();
            }
            let done = out.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));
            let mut changed = false;
            if done {
                if let Some(v) = parse_hex(&text) {
                    *c = Color32::from_rgba_unmultiplied((v >> 16) as u8, (v >> 8) as u8, v as u8, c.a());
                    changed = true;
                }
                ui.data_mut(|d| d.remove::<String>(id));
            } else {
                ui.data_mut(|d| d.insert_temp(id, text));
            }
            changed
        }
        None => {
            let hex = format!("{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
            let resp = ui.interact(r, id, Sense::click());
            ptext(ui.painter(), Pos2::new(pos.x, pos.y), Align2::LEFT_CENTER, &hex, reg(11.0), TEXT());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, format!("Color {hex}")));
            if resp.clicked() {
                ui.data_mut(|d| d.insert_temp(id, hex));
            }
            false
        }
    }
}

// a Figma checkbox: 12px box, label 11px
pub fn checkbox(ui: &mut egui::Ui, r: Rect, on: &mut bool, label: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(("check", label)), Sense::click());
    // measured in Figma 126.9.13: a 16px box on the panel's left padding, #383838 with a 1px #606060 border, a white tick,
    // the label 24px after the box's left edge
    let b = Rect::from_min_size(Pos2::new(r.left(), r.center().y - 8.0), Vec2::splat(16.0));
    let border = crate::theme::CHECK_BORDER();
    if *on {
        ui.painter().rect_filled(b, 3.0, INPUT());
        ui.painter().rect_stroke(b, 3.0, Stroke::new(1.0, border), egui::StrokeKind::Inside);
        let p = |x: f32, y: f32| b.min + Vec2::new(x, y);
        ui.painter().line(vec![p(4.0, 8.4), p(6.8, 11.0), p(12.0, 5.0)], Stroke::new(1.5, Color32::WHITE));
    } else {
        ui.painter().rect_stroke(b, 3.0, Stroke::new(1.0, if resp.hovered() { TEXT_SECONDARY() } else { border }), egui::StrokeKind::Inside);
    }
    ptext(ui.painter(), Pos2::new(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, label, reg(11.0), TEXT());
    let state = *on;
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, state, label));
    if resp.clicked() {
        *on = !*on;
        true
    } else {
        false
    }
}

// the checkbox of Figma's popovers (measured in the Export settings popover): a 16px box that fills with the share blue and a
// white tick when on, #383838 with a 1px #606060 border when off; the label 24px after the box's left edge
pub fn checkbox_blue(ui: &mut egui::Ui, r: Rect, on: &mut bool, label: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(("check-blue", label)), Sense::click());
    let b = Rect::from_min_size(Pos2::new(r.left(), r.center().y - 8.0), Vec2::splat(16.0));
    if *on {
        ui.painter().rect_filled(b, 3.0, SHARE);
        let p = |x: f32, y: f32| b.min + Vec2::new(x, y);
        ui.painter().line(vec![p(4.0, 8.4), p(6.8, 11.0), p(12.0, 5.0)], Stroke::new(1.5, Color32::WHITE));
    } else {
        ui.painter().rect_filled(b, 3.0, INPUT());
        ui.painter().rect_stroke(b, 3.0, Stroke::new(1.0, if resp.hovered() { TEXT_SECONDARY() } else { crate::theme::CHECK_BORDER() }), egui::StrokeKind::Inside);
    }
    ptext(ui.painter(), Pos2::new(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, label, reg(11.0), TEXT());
    let state = *on;
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, state, label));
    if resp.clicked() {
        *on = !*on;
        true
    } else {
        false
    }
}

// text centred on a line is centred on its glyphs: egui centres the line box and Inter's cap height is
// centred in it (hhea ascent 1984, descent 494, cap 1490 of 2048), so no lift is needed. Measured in the
// running app (X field digits, page row, section titles, layer names): the ink centre is within a pixel of
// the container's. Kept as one constant so every centred label can be nudged together.
pub const TEXT_LIFT: f32 = 0.0;
pub fn ptext(p: &egui::Painter, pos: Pos2, align: Align2, s: impl ToString, font: FontId, color: Color32) -> Rect {
    let dy = if align.y() == egui::Align::Center { -TEXT_LIFT * font.size } else { 0.0 };
    p.text(pos + Vec2::new(0.0, dy), align, s.to_string(), font, color)
}

pub fn text(ui: &egui::Ui, pos: Pos2, align: Align2, s: &str, font: FontId, color: Color32) {
    ptext(ui.painter(), pos, align, s, font, color);
}

// a tab label in a 24px pill, like Design / Prototype
pub fn tab(ui: &mut egui::Ui, pos: Pos2, label: &str, selected: bool) -> (Response, f32) {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), semi(11.0), if selected { TEXT() } else { TEXT_SECONDARY() });
    let r = Rect::from_min_size(pos, Vec2::new(galley.size().x + 16.0, ROW));
    let resp = ui.interact(r, ui.id().with(("tab", label)), Sense::click());
    if selected {
        ui.painter().rect_filled(r, RADIUS, INPUT());
    } else if resp.hovered() {
        ui.painter().rect_filled(r, RADIUS, crate::theme::WELL());
    }
    ui.painter().galley(r.center() - galley.size() / 2.0 - Vec2::new(0.0, TEXT_LIFT * 11.0), galley, TEXT());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label));
    (resp, r.width())
}

// a text field on #383838 without egui's frame; accessible by name and settable from outside
pub fn text_input(ui: &mut egui::Ui, r: Rect, text: &mut String, name: &str, hint: &str) -> Response {
    input_bg(ui, r, false);
    text_edit(ui, r, text, name, hint)
}

// the editable text of a field whose background is already painted: it fills the whole box (so a click
// anywhere in it edits) with the text centred vertically
pub fn text_edit(ui: &mut egui::Ui, r: Rect, text: &mut String, name: &str, hint: &str) -> Response {
    let inner = Rect::from_min_max(Pos2::new(r.left() + 6.0, r.top()), Pos2::new(r.right() - 4.0, r.bottom()));
    let mut resp = put(ui, inner, egui::TextEdit::singleline(text).frame(egui::Frame::NONE).font(reg(11.0)).hint_text(hint).margin(egui::Margin::symmetric(2, 0)).vertical_align(egui::Align::Center).min_size(inner.size()));
    let set = ui.input(|i| {
        i.accesskit_action_requests(resp.id, egui::accesskit::Action::SetValue).find_map(|q| match &q.data {
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

// an outlined full-width button (Figma's "Show prototype settings"): 24px, 1px #444, 16px from both edges
pub fn outline_button(ui: &mut egui::Ui, gap: f32, label: &str) -> Response {
    let row_r = controls(ui, gap);
    let r = Rect::from_min_max(Pos2::new(row_r.left() + 16.0, row_r.top()), Pos2::new(row_r.right() - 16.0, row_r.bottom()));
    let resp = ui.interact(r, ui.id().with(("outline", label)), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, RADIUS, INPUT());
    }
    ui.painter().rect_stroke(r, RADIUS, Stroke::new(1.0, RULE()), egui::StrokeKind::Inside);
    ptext(ui.painter(), r.center(), Align2::CENTER_CENTER, label, reg(11.0), TEXT());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp
}

// a label and a value on one 24px row (Dev Mode's property list)
pub fn value_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let r = controls(ui, 0.0);
    ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, label, reg(11.0), TEXT_SECONDARY());
    let resp = ui.interact(Rect::from_min_max(Pos2::new(r.left() + 100.0, r.top()), r.max), ui.id().with(("value-row", label)), Sense::click());
    ptext(&ui.painter().with_clip_rect(r), Pos2::new(r.left() + 100.0, r.center().y), Align2::LEFT_CENTER, value, reg(11.0), if resp.hovered() { SELECTED_INK() } else { TEXT() });
    let (l, v) = (label.to_owned(), value.to_owned());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("{l}: {v}")));
    // like Dev Mode: click a value to copy it
    if resp.clicked() {
        ui.ctx().copy_text(value.to_owned());
    }
}

// Figma's info note: an icon at 16, then 11px text wrapped from x 48 to 24px from the right
pub fn note(ui: &mut egui::Ui, g: Glyph, text: &str) {
    let w = ui.available_width();
    let galley = ui.painter().layout(text.to_owned(), reg(11.0), TEXT(), (w - 48.0 - 24.0).max(40.0));
    let r = row(ui, galley.size().y + 8.0);
    paint_glyph(ui.painter(), Pos2::new(r.left() + 24.0, r.top() + 4.0 + 8.0), g, TEXT());
    ui.painter().galley(Pos2::new(r.left() + 48.0, r.top() + 4.0 - 1.4), galley, TEXT());
    let t = text.to_owned();
    ui.interact(r, ui.id().with(("note", text.len())), Sense::hover()).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, t.clone()));
}

// a code block: line numbers in a gutter, the code as selectable monospace text, on #1e1e1e with a 1px border
pub fn code_block(ui: &mut egui::Ui, code: &str, name: &str) {
    let font = FontId::monospace(11.0);
    let line_h = ui.fonts_mut(|f| f.row_height(&font));
    let lines = code.lines().count().max(1);
    let gap = row(ui, 4.0);
    let r = row(ui, lines as f32 * line_h + 16.0);
    let bx = Rect::from_min_max(Pos2::new(r.left() + 16.0, r.top()), Pos2::new(r.right() - 8.0, r.bottom()));
    let _ = gap;
    ui.painter().rect(bx, RADIUS, crate::theme::SUNKEN(), Stroke::new(1.0, RULE()), egui::StrokeKind::Inside);
    for k in 0..lines {
        ui.painter().text(Pos2::new(bx.left() + 22.0, bx.top() + 8.0 + k as f32 * line_h), Align2::RIGHT_TOP, (k + 1).to_string(), font.clone(), TEXT_TERTIARY());
    }
    let mut text = code.to_owned();
    let inner = Rect::from_min_max(Pos2::new(bx.left() + 30.0, bx.top() + 8.0), Pos2::new(bx.right() - 6.0, bx.bottom() - 4.0));
    let edit = put(ui, inner, egui::TextEdit::multiline(&mut text).frame(egui::Frame::NONE).font(font).margin(egui::Margin::ZERO).desired_width(inner.width()).text_color(TEXT()));
    edit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, name));
}

// a hover tooltip with 8px of air on each side (egui's own tooltip frame takes the menu margin, which has no side padding here)
pub trait Tip {
    fn tip(self, text: impl Into<String>) -> Response;
}

impl Tip for Response {
    fn tip(self, text: impl Into<String>) -> Response {
        let text: String = text.into();
        self.on_hover_ui(|ui| {
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 0)).show(ui, |ui| {
                ui.label(egui::RichText::new(text).font(reg(11.0)).color(TEXT()));
            });
        })
    }
}

// places a widget in `r` like `Ui::put`, but leaves the layout cursor where it was: `put` moves it to the bottom of `r`, which
// pulled everything below up whenever the field's text box was shorter than its row (a focused field shrank the panel)
pub fn put(ui: &mut egui::Ui, r: Rect, w: impl egui::Widget) -> Response {
    // the same child `Ui::put` makes (centred and justified in `r`), without the cursor move
    ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::centered_and_justified(egui::Direction::TopDown))).add(w)
}

// A slider with its value, as in Figma's Glass settings: a track and a round knob in the left 80px, the number in a box after it.
// Measured in Figma 126.9.13 (REA + a screenshot): the control is 136 x 24, the track runs 10px in from each end of the slider part and
// is 4px thick, the knob is 13px, the value box is 55px wide.
pub fn slider_field(ui: &mut egui::Ui, r: Rect, id: egui::Id, v: &mut f32, min: f32, max: f32, name: &str) -> bool {
    const SLIDER_W: f32 = 80.0;
    let sl = Rect::from_min_size(r.min, Vec2::new(SLIDER_W, r.height()));
    let bx = Rect::from_min_max(Pos2::new(r.left() + SLIDER_W + 1.0, r.top()), r.max);
    input_bg(ui, sl, false);
    let resp = ui.interact(sl, id.with("slider"), Sense::click_and_drag());
    let track = Rect::from_min_max(Pos2::new(sl.left() + 10.0, sl.center().y - 2.0), Pos2::new(sl.right() - 10.0, sl.center().y + 2.0));
    let mut changed = false;
    if resp.dragged() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let t = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let nv = (min + t * (max - min)).round();
            if nv != *v {
                *v = nv;
                changed = true;
            }
        }
    }
    let t = ((*v - min) / (max - min)).clamp(0.0, 1.0);
    let knob = Pos2::new(track.left() + t * track.width(), track.center().y);
    ui.painter().rect_filled(track, 2.0, Color32::from_gray(0x4a));
    ui.painter().rect_filled(Rect::from_min_max(track.min, Pos2::new(knob.x, track.max.y)), 2.0, crate::BRAND);
    ui.painter().circle_filled(knob + Vec2::new(0.0, 0.5), 6.5, Color32::from_black_alpha(80));
    ui.painter().circle_filled(knob, 6.5, Color32::WHITE);
    resp.widget_info(|| {
        let mut info = egui::WidgetInfo::slider(true, *v as f64, name);
        info.current_text_value = Some(format!("{}", *v as i32));
        info
    });
    changed |= num_field(ui, bx, id.with("value"), Prefix::None, v, Num { min, max, decimals: 0, speed: 1.0, suffix: "" }, name);
    changed
}

// Figma's Light control: a square with a glass shape in the middle and a light that is dragged round it. The angle is in degrees,
// clockwise from the top (-45 puts the light at the top left).
pub fn light_dial(ui: &mut egui::Ui, r: Rect, id: egui::Id, angle: &mut f32, name: &str) -> bool {
    input_bg(ui, r, false);
    let c = r.center();
    let resp = ui.interact(r, id, Sense::click_and_drag());
    let mut changed = false;
    if resp.dragged() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let d = p - c;
            if d.length() > 2.0 {
                let a = d.x.atan2(-d.y).to_degrees().round();
                if a != *angle {
                    *angle = a;
                    changed = true;
                }
            }
        }
    }
    // the glass, lit from the side the light is on
    let glass = Rect::from_center_size(c + Vec2::new(0.0, 3.0), Vec2::new(26.0, 30.0));
    ui.painter().rect_filled(glass, 8.0, Color32::from_gray(0x3a));
    let a = angle.to_radians();
    let dir = Vec2::new(a.sin(), -a.cos());
    ui.painter().rect_stroke(glass, 8.0, Stroke::new(1.5, Color32::from_white_alpha(70)), egui::StrokeKind::Inside);
    let light = c + dir * 21.0;
    ui.painter().line_segment([light - dir * 6.0, c + dir * 14.0], Stroke::new(1.0, crate::BRAND.gamma_multiply(0.6)));
    ui.painter().circle_filled(light, 5.0, crate::BRAND);
    resp.widget_info(|| {
        let mut info = egui::WidgetInfo::slider(true, *angle as f64, name);
        info.current_text_value = Some(format!("{}°", *angle as i32));
        info
    });
    changed
}
