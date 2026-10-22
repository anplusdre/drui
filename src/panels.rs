// The side panels and the toolbar.
use crate::fig::Tip as _;
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};

use crate::fig;
use crate::geom::BoolOp;
use crate::model::*;
use crate::ops::AlignOp;
use crate::glyphs::{Glyph, divider, menu_item, paint_glyph, paint_glyph_sized, row_icon};
use crate::widgets::{Icon, paint_eye, paint_lock, tool_button};
use crate::{App, BG, BG_PRESSED, BG_SECONDARY, BRAND, LeftTab, Mode, R_LARGE, R_MEDIUM, R_SMALL, Tab, TEXT, TEXT_SECONDARY, TEXT_TERTIARY, Tool};

enum RowAct {
    Select(u64),
    Visible(u64),
    Locked(u64),
    Collapse(u64),
    Rename(u64),
}

// where a dragged layer row would land, relative to the row under the pointer (visual: Above = higher z)
#[derive(Clone, Copy)]
enum Drop {
    Above(u64),
    Below(u64),
    Into(u64),
}

pub(crate) enum Act {
    Group,
    Flatten,
    Mask,
    Bool(BoolOp),
    Component,
    PushOverrides,
    RestoreMain,
    Detach,
    Swap(u64),
    GoMain(u64),
    Instance(u64),
    Align(AlignOp),
    Distribute(bool),
    Flip(bool),
    AutoLayout,
    AutoLayoutGrid,
    ResizeToFit,
    Export,
    NewStyle(Color32),
}

impl App {
    // deletes color style `k`; shapes that used it keep the color it had
    pub(crate) fn delete_style(&mut self, k: usize) {
        let (id, color) = (self.styles[k].id, self.styles[k].color);
        for s in self.shapes.iter_mut() {
            for p in s.fills.iter_mut().chain(s.strokes.iter_mut()).filter(|p| p.style == Some(id)) {
                p.style = None;
                p.kind = PaintKind::Solid(color);
            }
        }
        self.styles.remove(k);
    }
}

// the layers list icon of a layer: its kind, or what it is on top of that (a component or instance, a section, a slice, a
// boolean group, an auto layout frame showing its direction)
fn layer_glyph(s: &Shape) -> Glyph {
    if let Some(k) = s.bool_op {
        return [Glyph::Union, Glyph::Subtract, Glyph::Intersect, Glyph::Exclude][k.min(3) as usize];
    }
    if s.component || s.component_set {
        return Glyph::Component;
    }
    if s.instance {
        return Glyph::Instance;
    }
    if s.section {
        return Glyph::Section;
    }
    if s.slice {
        return Glyph::Slice;
    }
    match &s.layout {
        Some(l) if s.kind == Kind::Frame => {
            if l.vertical {
                Glyph::Rows
            } else {
                Glyph::AutoLayout
            }
        }
        _ => kind_glyph(s.kind),
    }
}

fn kind_glyph(k: Kind) -> Glyph {
    match k {
        Kind::Frame => Glyph::Frame,
        Kind::Group => Glyph::Group,
        Kind::Rect => Glyph::Rect,
        Kind::Ellipse => Glyph::Ellipse,
        Kind::Line => Glyph::Line,
        Kind::Text => Glyph::Type,
        Kind::Polygon => Glyph::Polygon,
        Kind::Star => Glyph::Star,
        Kind::Path => Glyph::Vector,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Sizing {
    Fixed,
    Hug,
    Fill,
}

pub(crate) fn sizing_get(s: &Shape, horizontal: bool) -> Sizing {
    if if horizontal { s.fill_x } else { s.fill_y } {
        return Sizing::Fill;
    }
    let hug = match s.kind {
        Kind::Text => {
            if horizontal { s.fit == TextFit::AutoWidth } else { s.fit != TextFit::Fixed }
        }
        Kind::Frame => s.layout.as_ref().is_some_and(|l| if horizontal { l.hug_x } else { l.hug_y }),
        _ => false,
    };
    if hug { Sizing::Hug } else { Sizing::Fixed }
}

pub(crate) fn sizing_set(s: &mut Shape, horizontal: bool, z: Sizing) {
    if horizontal {
        s.fill_x = z == Sizing::Fill;
    } else {
        s.fill_y = z == Sizing::Fill;
    }
    match s.kind {
        Kind::Text => {
            // a text box that hugs its width must hug its height too
            let (hw, hh) = (sizing_get(s, true) == Sizing::Hug, sizing_get(s, false) == Sizing::Hug);
            let (hw, hh) = if horizontal { (z == Sizing::Hug, hh) } else { (hw, z == Sizing::Hug) };
            s.fit = match (hw, hh) {
                (true, _) => TextFit::AutoWidth,
                (false, true) => TextFit::AutoHeight,
                (false, false) => TextFit::Fixed,
            };
        }
        Kind::Frame => {
            if let Some(l) = &mut s.layout {
                if horizontal {
                    l.hug_x = z == Sizing::Hug;
                } else {
                    l.hug_y = z == Sizing::Hug;
                }
            }
        }
        _ => {}
    }
}

// a short wavy sample of what a brush draws, inside `r`
fn brush_sample(p: &egui::Painter, r: Rect, brush: crate::vector::Brush, nib: f32) {
    use crate::vector::{brush_outline, brush_widths, scatter, Brush};
    let pts: Vec<Pos2> = (0..=24).map(|k| k as f32 / 24.0).map(|t| Pos2::new(r.min.x + t * r.width(), r.center().y - (t * std::f32::consts::TAU).sin() * r.height() * 0.3)).collect();
    // a stand-in for pressure: heavier in the middle
    let force: Vec<f32> = (0..pts.len()).map(|k| 0.35 + 0.65 * (k as f32 / 24.0 * std::f32::consts::PI).sin()).collect();
    let w = (r.height() * 0.35).max(2.0);
    match brush {
        Brush::Pencil => {
            p.add(egui::Shape::line(pts, Stroke::new(1.5, TEXT())));
        }
        Brush::Stretch => {
            // a ribbon: the picture stretched along the sample
            p.add(egui::Shape::line(pts, Stroke::new(w * 1.4, egui::Color32::from_rgb(0x9a, 0x7b, 0xff))));
        }
        Brush::Layer => {
            for (k, q) in pts.iter().enumerate().step_by(4) {
                p.circle_stroke(*q, w * (0.6 + 0.2 * (k % 3) as f32), Stroke::new(1.2, TEXT()));
            }
        }
        Brush::Scatter => {
            for c in scatter(&pts, &force, w) {
                let (min, max) = c.pts.iter().fold((c.pts[0].p, c.pts[0].p), |(a, b), q| (a.min(q.p), b.max(q.p)));
                p.circle_filled(min + (max - min) / 2.0, (max.x - min.x) / 2.0, TEXT());
            }
        }
        b => {
            let widths = brush_widths(b, &pts, &force, nib);
            let outline = brush_outline(&pts, &widths, w);
            // the outline is not convex: fill it as one triangle strip between the two sides
            // (a single mesh, so no anti-aliased seams between the pieces)
            let n = pts.len();
            let right = |k: usize| outline[n + 5 + (n - 1 - k)]; // the right side runs backwards after the end cap
            let mut mesh = egui::Mesh::default();
            for i in 0..n {
                mesh.colored_vertex(outline[i], TEXT());
                mesh.colored_vertex(right(i), TEXT());
            }
            for i in 0..n as u32 - 1 {
                let k = i * 2;
                mesh.add_triangle(k, k + 1, k + 2);
                mesh.add_triangle(k + 1, k + 3, k + 2);
            }
            p.add(mesh);
        }
    }
}

// an easing curve in a 24px box (time across, progress up)
fn ease_box(ui: &mut egui::Ui, r: Rect, ease: Easing) {
    let resp = ui.interact(r, ui.id().with("ease-curve"), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, fig::RADIUS, fig::INPUT());
    let plot = r.shrink(6.0);
    let pts: Vec<Pos2> = (0..=24).map(|k| k as f32 / 24.0).map(|t| Pos2::new(plot.min.x + t * plot.width(), plot.max.y - ease.at(t) * plot.height())).collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.0, fig::SELECTED_INK())));
    let name = Easing::ALL.iter().find(|e| e.0 == ease).map_or("", |e| e.1);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("Easing curve: {name}")));
}

// a 32px rail button with an optional 9px label under it
fn rail_button(ui: &mut egui::Ui, b: Rect, g: Glyph, label: Option<&str>, selected: bool, name: &str) -> egui::Response {
    let resp = ui.interact(b, ui.id().with(("rail", name)), Sense::click());
    let bg = if selected {
        fig::SELECTED()
    } else if resp.hovered() {
        fig::INPUT()
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(b, fig::RADIUS, bg);
    paint_glyph_sized(ui.painter(), b.center(), g, if selected { fig::SELECTED_INK() } else { TEXT() }, 16.0);
    if let Some(l) = label {
        fig::ptext(ui.painter(), Pos2::new(b.center().x, b.bottom() + 8.7), Align2::CENTER_CENTER, l, fig::reg(9.0), TEXT());
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, name));
    resp
}

// a left-panel section title, like Pages and Layers: 44px, 11px semibold at 16, 24px icon buttons from
// 8px off the right edge, 28px apart
fn left_header(ui: &mut egui::Ui, title: &str, actions: &[(Glyph, &str, bool)]) -> Vec<egui::Response> {
    let r = fig::row(ui, 44.0);
    fig::text(ui, Pos2::new(r.left() + 16.0, r.top() + 22.0 + fig::TEXT_LIFT * 11.0), Align2::LEFT_CENTER, title, fig::semi(11.0), TEXT());
    actions
        .iter()
        .enumerate()
        .map(|(k, (g, tip, on))| {
            let b = Rect::from_min_size(Pos2::new(r.right() - 32.0 - 28.0 * (actions.len() - 1 - k) as f32, r.top() + 10.0), Vec2::splat(24.0));
            fig::icon_button(ui, b, *g, tip, *on)
        })
        .collect()
}

// a 24px search field inset 16px / 8px: a search icon in a 20px cap, then the text
fn search_field(ui: &mut egui::Ui, text: &mut String, name: &str, hint: &str) -> egui::Response {
    let row = fig::row(ui, 24.0);
    let f = Rect::from_min_max(Pos2::new(row.left() + 16.0, row.top()), Pos2::new(row.right() - 8.0, row.bottom()));
    fig::input_bg(ui, f, false);
    paint_glyph_sized(ui.painter(), Pos2::new(f.left() + 12.0, f.center().y), Glyph::Search, TEXT_SECONDARY(), 13.0);
    fig::text_edit(ui, f.with_min_x(f.left() + 18.0), text, name, hint)
}

// a group title in an Assets list, as Figma's "All libraries" / "UI kits": 32px, 11px semibold at 16, an
// optional action at the right; a click folds it (remembered). Returns (open, action clicked).
fn fold_head(ui: &mut egui::Ui, title: &str, action: Option<(Glyph, &str)>) -> (bool, bool) {
    let id = ui.make_persistent_id(("fold", title));
    let mut open = ui.data_mut(|d| *d.get_persisted_mut_or_insert_with(id, || true));
    let r = fig::row(ui, 32.0);
    let resp = ui.interact(r, id.with("head"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, title));
    fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, title, fig::semi(11.0), TEXT());
    let acted = action.is_some_and(|(g, tip)| fig::icon_button(ui, row_button(r, 0), g, tip, false).clicked());
    if resp.clicked() && !acted {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    (open, acted)
}

// a 32px list row with a 24px highlight inset 8px each side; returns the row, its response and whether
// the pointer is over it (hover actions stay visible while the pointer is on them)
fn list_row(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, name: &str, selected: bool) -> (Rect, egui::Response, bool) {
    let r = fig::row(ui, 32.0);
    let hl = Rect::from_min_max(Pos2::new(r.left() + 8.0, r.top() + 4.0), Pos2::new(r.right() - 8.0, r.bottom() - 4.0));
    let resp = ui.interact(hl, ui.id().with(salt), Sense::click());
    let hot = ui.rect_contains_pointer(hl);
    if selected {
        ui.painter().rect_filled(hl, fig::RADIUS, fig::SELECTED());
    } else if hot {
        ui.painter().rect_filled(hl, fig::RADIUS, fig::INPUT());
    }
    let n = name.to_owned();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, n.clone()));
    (r, resp, hot)
}

// the settings popover of one export setting, as Figma's "Export" popover (measured 2026-10-18): a 40px header, then 32px rows
// with the label at 16 and a 100px field at 124 (Suffix, Color profile), then two checkboxes whose box starts at 12
pub(crate) fn export_popover(ui: &mut egui::Ui, set: &mut ExportSetting) {
    if crate::strokefx::popover_header(ui, "Export") {
        ui.close();
    }
    fig::row(ui, 6.0);
    let field = |r: Rect| Rect::from_min_size(Pos2::new(r.left() + 124.0, r.top() + 4.0), Vec2::new(100.0, fig::ROW));
    let r = fig::row(ui, 32.0);
    crate::strokefx::row_label(ui, r, "Suffix");
    fig::text_input(ui, field(r), &mut set.suffix, "Suffix", "None");
    let r = fig::row(ui, 32.0);
    crate::strokefx::row_label(ui, r, "Color profile");
    fig::dropdown(ui, field(r), None, &mut set.profile, &[(0u8, "Same as file"), (1, "sRGB"), (2, "Display P3")], "Color profile");
    if matches!(set.format, ExportFormat::Png | ExportFormat::Jpg | ExportFormat::Pdf) {
        let r = fig::row(ui, 32.0);
        crate::strokefx::row_label(ui, r, "Image resampling");
        let mut detailed = !set.basic_resample;
        if fig::dropdown(ui, field(r), None, &mut detailed, &[(true, "Detailed"), (false, "Basic")], "Image resampling") {
            set.basic_resample = !detailed;
        }
    }
    if set.format == ExportFormat::Jpg {
        let r = fig::row(ui, 32.0);
        crate::strokefx::row_label(ui, r, "Image quality");
        fig::dropdown(ui, field(r), None, &mut set.quality, &[(0u8, "Low"), (1, "Medium"), (2, "High")], "Image quality");
    }
    let r = fig::row(ui, 32.0);
    fig::checkbox_blue(ui, Rect::from_min_size(Pos2::new(r.left() + 12.0, r.top()), Vec2::new(r.width() - 24.0, 32.0)), &mut set.ignore_overlap, "Ignore overlapping layers");
    if set.format == ExportFormat::Svg {
        let r = fig::row(ui, 32.0);
        fig::checkbox_blue(ui, Rect::from_min_size(Pos2::new(r.left() + 12.0, r.top()), Vec2::new(r.width() - 24.0, 32.0)), &mut set.include_id, "Include \"id\" attribute");
        let r = fig::row(ui, 32.0);
        fig::checkbox_blue(ui, Rect::from_min_size(Pos2::new(r.left() + 12.0, r.top()), Vec2::new(r.width() - 24.0, 32.0)), &mut set.outline_text, "Outline text");
    }
    fig::row(ui, 8.0);
}

// the `slot`th 24px button from the right of a 32px row (0 = rightmost, 8px in)
fn row_button(r: Rect, slot: usize) -> Rect {
    Rect::from_min_size(Pos2::new(r.right() - 32.0 - 28.0 * slot as f32, r.top() + (r.height() - 24.0) / 2.0), Vec2::splat(24.0))
}

// a grey line of text in an empty list, at the name column
fn empty_row(ui: &mut egui::Ui, text: &str) {
    let r = fig::row(ui, 24.0);
    fig::ptext(&ui.painter().with_clip_rect(r), Pos2::new(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, text, fig::reg(11.0), TEXT_TERTIARY());
}

// "Oct 12, 14:03" (UTC) for a unix time
pub(crate) fn when(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    // civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let t = secs % 86400;
    format!("{} {}, {:02}:{:02} UTC", MONTHS[(m - 1) as usize], d, t / 3600, t / 60 % 60)
}

impl App {
    // ---- left panel

    // ---- left rail, main menu, file header

    // the rail on the far left (Figma: 58px; 32px buttons 56px apart under the logo slot, which holds the menu)
    pub fn rail(&mut self, ui: &mut egui::Ui) {
        let r = ui.max_rect();
        let cx = r.left() + 29.0;
        let menu = rail_button(ui, Rect::from_center_size(Pos2::new(cx, r.top() + 24.0), Vec2::splat(32.0)), Glyph::Menu, None, false, "Menu");
        egui::Popup::menu(&menu).style(crate::glyphs::fig_menu_style).show(|ui| self.main_menu(ui));
        ui.painter().rect_filled(Rect::from_center_size(Pos2::new(cx, r.top() + 48.5), Vec2::new(24.0, 1.0)), 0.0, fig::RULE());
        // Figma's rail: File and Assets. Version history is under File, comments under View.
        for (k, (t, g, label)) in [(LeftTab::Layers, Glyph::File, "File"), (LeftTab::Assets, Glyph::Library, "Assets")].into_iter().enumerate() {
            let b = Rect::from_min_size(Pos2::new(r.left() + 13.0, r.top() + 60.0 + 56.0 * k as f32), Vec2::splat(32.0));
            if rail_button(ui, b, g, Some(label), self.left_tab == t, label).clicked() {
                self.left_tab = t;
            }
        }
        // Variables: a window over the canvas, not a left-panel tab
        let vb = Rect::from_min_size(Pos2::new(r.left() + 13.0, r.top() + 60.0 + 56.0 * 2.0), Vec2::splat(32.0));
        if rail_button(ui, vb, Glyph::Sliders, Some("Variables"), self.vars_open, "Variables").clicked() {
            self.vars_open ^= true;
        }
        let help = Rect::from_center_size(Pos2::new(cx, r.bottom() - 32.0), Vec2::splat(32.0));
        if rail_button(ui, help, Glyph::Help, None, self.show_help, "Help").clicked() {
            self.show_help ^= true;
        }
    }

    // Figma's zoom menu (measured, Figma 126.9.13): an editable zoom value, then zoom steps, then what the canvas shows
    pub(crate) fn zoom_items(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let (r, _) = ui.allocate_exact_size(Vec2::new(225.0, 30.0), Sense::hover());
        let field = Rect::from_min_size(Pos2::new(r.left() + 24.0, r.top()), Vec2::new(r.width() - 48.0, 30.0));
        if self.zoom_fresh {
            self.zoom_text = format!("{:.0}%", self.zoom * 100.0);
        }
        ui.painter().rect_stroke(field, fig::RADIUS, Stroke::new(2.0, fig::SHARE), StrokeKind::Inside);
        let resp = fig::text_edit(ui, field, &mut self.zoom_text, "Zoom", "");
        if self.zoom_fresh {
            resp.request_focus();
            let mut st = egui::TextEdit::load_state(ui.ctx(), resp.id).unwrap_or_default();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(self.zoom_text.chars().count()))));
            st.store(ui.ctx(), resp.id);
            self.zoom_fresh = false;
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Ok(v) = self.zoom_text.trim().trim_end_matches('%').trim().parse::<f32>() {
                self.zoom_at_center(v / 100.0);
            }
            ui.close();
        }
        ui.add_space(3.0);
        crate::glyphs::menu_sep(ui);
        let steps: [(&str, &str, Option<f32>); 6] = [("Zoom in", "Ctrl++", None), ("Zoom out", "Ctrl+–", None), ("Zoom to fit", "Shift+1", None), ("Zoom to 50%", "", Some(0.5)), ("Zoom to 100%", "Ctrl+0", Some(1.0)), ("Zoom to 200%", "", Some(2.0))];
        for (k, (label, key, z)) in steps.iter().enumerate() {
            if menu_item(ui, None, label, key, true) {
                match (k, z) {
                    (0, _) => self.zoom_at_center(self.zoom * 1.25),
                    (1, _) => self.zoom_at_center(self.zoom * 0.8),
                    (2, _) => self.zoom_fit(false),
                    (_, Some(z)) => self.zoom_at_center(*z),
                    _ => {}
                }
                ui.close();
            }
            if k == 2 && !self.sel.is_empty() && menu_item(ui, None, "Zoom to selection", "Shift+2", true) {
                self.zoom_fit(true);
                ui.close();
            }
        }
        // order, shortcuts and submenus as in Figma 126.9.13 (measured with REA)
        crate::glyphs::menu_sep(ui);
        crate::glyphs::menu_submenu(ui, None, "Pixel preview", |ui| {
            for (v, label) in [(0u8, "Disabled"), (1, "1x"), (2, "2x")] {
                if crate::glyphs::menu_check(ui, self.pixel_preview == v, label, "") {
                    self.pixel_preview = v;
                    ui.close();
                }
            }
        });
        if crate::glyphs::menu_check(ui, self.pixel_grid, "Pixel grid", "Shift+'") {
            self.pixel_grid ^= true;
        }
        if crate::glyphs::menu_check(ui, self.snap_pixels, "Snap to pixel grid", "Ctrl+Shift+'") {
            self.snap_pixels ^= true;
        }
        if crate::glyphs::menu_check(ui, self.layout_guides, "Layout guides", "Shift+G") {
            self.layout_guides ^= true;
        }
        if crate::glyphs::menu_check(ui, self.rulers, "Rulers", "Shift+R") {
            self.rulers ^= true;
        }
        crate::glyphs::menu_submenu(ui, None, "Outlines", |ui| {
            if crate::glyphs::menu_check(ui, self.outlines, "Show outlines", "Ctrl+Shift+O") {
                self.outlines ^= true;
            }
            crate::glyphs::menu_sep(ui);
            if crate::glyphs::menu_check(ui, self.outline_hidden, "Include hidden layers", "") {
                self.outline_hidden ^= true;
            }
            if crate::glyphs::menu_check(ui, self.outline_bounds, "Include object bounds", "") {
                self.outline_bounds ^= true;
            }
        });
        if crate::glyphs::menu_check(ui, self.labels, "Additional labels", "") {
            self.labels ^= true;
        }
        crate::glyphs::menu_sep(ui);
        if crate::glyphs::menu_check(ui, self.show_comments, "Comments", "Shift+C") {
            self.show_comments ^= true;
        }
    }

    fn view_items(&mut self, ui: &mut egui::Ui) {
        let (rulers, snap) = (self.rulers, self.snap);
        if menu_item(ui, Some(if rulers { Glyph::Eye } else { Glyph::EyeOff }), "Rulers and guides", "Shift+R", true) {
            self.rulers ^= true;
            ui.close();
        }
        if menu_item(ui, Some(if snap { Glyph::Eye } else { Glyph::EyeOff }), "Snap to objects", "", true) {
            self.snap ^= true;
            ui.close();
        }
        crate::glyphs::menu_sep(ui);
        for (label, key, z) in [("Zoom in", "Ctrl +", Some(1.25)), ("Zoom out", "Ctrl -", Some(0.8)), ("Zoom to 100%", "Shift+0", None)] {
            if menu_item(ui, None, label, key, true) {
                match z {
                    Some(f) => self.zoom_at_center(self.zoom * f),
                    None => self.zoom_at_center(1.0),
                }
                ui.close();
            }
        }
        if menu_item(ui, None, "Zoom to fit", "Shift+1", true) {
            self.zoom_fit(false);
            ui.close();
        }
        if menu_item(ui, None, "Zoom to selection", "Shift+2", !self.sel.is_empty()) {
            self.zoom_fit(true);
            ui.close();
        }
    }

    fn main_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Figma's main menu is 210px wide (194 of rows) and groups its rows: Actions, the menus, Plugins and Preferences, Help
        ui.spacing_mut().menu_width = 194.0;
        if crate::glyphs::menu_search_row(ui, "Actions…", "Ctrl+/") {
            self.open_actions();
            ui.close();
        }
        crate::glyphs::menu_sep(ui);
        crate::glyphs::menu_submenu(ui, None, "File", |ui| self.file_items(ui, &ctx));
        self.menu_rest(ui, &ctx);
    }

    // Open, Save, Save as, Place image, Export
    fn file_items(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        {
            if menu_item(ui, None, "Browse files…", "", true) {
                self.files.open = true;
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Open), "Open…", "", true) {
                if let Some(p) = rfd::FileDialog::new().add_filter("drui", &["json"]).pick_file() {
                    self.load_from(p);
                }
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Save), "Save", "Ctrl+S", true) {
                self.save_to(self.file.clone());
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Open), "Import Figma file (.fig)…", "", true) {
                if let Some(p) = rfd::FileDialog::new().add_filter("Figma file", &["fig"]).pick_file() {
                    self.import_fig(p);
                }
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Open), "Import Sketch file (.sketch)…", "", true) {
                if let Some(p) = rfd::FileDialog::new().add_filter("Sketch file", &["sketch"]).pick_file() {
                    self.import_sketch(p);
                }
                ui.close();
            }
            if menu_item(ui, Some(Glyph::SaveAll), "Save as…", "", true) {
                if let Some(p) = rfd::FileDialog::new().add_filter("drui", &["json"]).set_file_name("design.json").save_file() {
                    self.save_to(p);
                }
                ui.close();
            }
            crate::glyphs::menu_sep(ui);
            if menu_item(ui, Some(Glyph::ImagePlus), "Place image…", "", true) {
                if let Some(p) = rfd::FileDialog::new().add_filter("image", &["png", "jpg", "jpeg"]).pick_file() {
                    if let Ok(bytes) = std::fs::read(&p) {
                        let name = p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        self.add_image(&name, bytes, None);
                    }
                }
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Download), "Export…", "", true) {
                self.export(ctx);
                ui.close();
            }
            crate::glyphs::menu_sep(ui);
            if menu_item(ui, Some(Glyph::Clock), "Show version history", "", true) {
                self.left_tab = LeftTab::History;
                ui.close();
            }
            if menu_item(ui, None, "Branches…", "", true) {
                self.branches_open = true;
                ui.close();
            }
        }
    }

    fn menu_rest(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let ctx = ctx.clone();
        let has = !self.sel.is_empty();
        crate::glyphs::menu_submenu(ui, None, "Edit", |ui| {
            if menu_item(ui, Some(Glyph::Undo), "Undo", "Ctrl+Z", !self.undo.is_empty()) {
                self.do_undo(true);
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Redo), "Redo", "Ctrl+Shift+Z", !self.redo.is_empty()) {
                self.do_undo(false);
                ui.close();
            }
            crate::glyphs::menu_sep(ui);
            if menu_item(ui, None, "Copy", "Ctrl+C", has) {
                self.copy(&ctx);
                ui.close();
            }
            if menu_item(ui, None, "Copy as SVG", "", has) {
                self.copy_as_svg(&ctx);
                ui.close();
            }
            if menu_item(ui, None, "Copy as CSS", "", has) {
                self.copy_as_css(&ctx);
                ui.close();
            }
            if menu_item(ui, None, "Copy as PNG", "Ctrl+Shift+C", has) {
                self.copy_as_png(&ctx);
                ui.close();
            }
            if menu_item(ui, None, "Paste", "Ctrl+V", !self.clipboard.is_empty()) {
                self.paste_internal();
                ui.close();
            }
            if menu_item(ui, None, "Duplicate", "Ctrl+D", has) {
                self.duplicate();
                ui.close();
            }
            if menu_item(ui, None, "Select all", "Ctrl+A", true) {
                self.select_all();
                ui.close();
            }
            if menu_item(ui, None, "Select matching layers", "Ctrl+Alt+A", has) {
                self.select_matching();
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Delete), "Delete", "Del", has) {
                for id in self.sel.clone() {
                    self.remove_subtree(id);
                }
                ui.close();
            }
        });
        crate::glyphs::menu_submenu(ui, None, "View", |ui| {
            self.view_items(ui);
            crate::glyphs::menu_sep(ui);
            if menu_item(ui, Some(Glyph::MessageSquare), if self.left_tab == LeftTab::Comments { "Hide comments panel" } else { "Show comments panel" }, "", true) {
                self.left_tab = if self.left_tab == LeftTab::Comments { LeftTab::Layers } else { LeftTab::Comments };
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Help), "Keyboard shortcuts", "?", true) {
                self.show_help = true;
                ui.close();
            }
        });
        crate::glyphs::menu_submenu(ui, None, "Object", |ui| {
            for (g, label, key, which) in [
                (Glyph::Group, "Group selection", "Ctrl+G", 0),
                (Glyph::Ungroup, "Ungroup", "Ctrl+Shift+G", 1),
                (Glyph::Flatten, "Flatten", "Ctrl+E", 2),
                (Glyph::Scan, "Outline stroke", "Ctrl+Alt+O", 9),
                (Glyph::Mask, "Use as mask", "Ctrl+Alt+M", 3),
                (Glyph::Component, "Create component", "Ctrl+Alt+K", 4),
                (Glyph::Component, "Detach instance", "Ctrl+Alt+B", 5),
                (Glyph::Component, "Combine as variants", "Ctrl+Shift+K", 8),
                (Glyph::AutoLayout, "Add auto layout", "Shift+A", 6),
                (Glyph::AutoLayout, "Remove auto layout", "Ctrl+Alt+Shift+A", 7),
            ] {
                if menu_item(ui, Some(g), label, key, has) {
                    match which {
                        0 => self.group(),
                        1 => self.ungroup(),
                        2 => self.flatten(),
                        3 => self.toggle_mask(),
                        4 => self.make_component(),
                        5 => self.detach_instances(),
                        8 => self.combine_as_variants(),
                        9 => self.outline_stroke(),
                        6 => self.add_auto_layout_selection(),
                        _ => self.remove_auto_layout_selection(),
                    }
                    ui.close();
                }
            }
            crate::glyphs::menu_sep(ui);
            crate::glyphs::menu_submenu(ui, None, "Boolean groups", |ui| {
                for (g, label, op) in [(Glyph::Union, "Union", BoolOp::Union), (Glyph::Subtract, "Subtract", BoolOp::Subtract), (Glyph::Intersect, "Intersect", BoolOp::Intersect), (Glyph::Exclude, "Exclude", BoolOp::Exclude)] {
                    if menu_item(ui, Some(g), label, "", self.sel.len() > 1) {
                        self.boolean(op);
                        ui.close();
                    }
                }
            });
        });
        crate::glyphs::menu_submenu(ui, None, "Arrange", |ui| {
            let roots = self.sel_roots();
            if menu_item(ui, Some(Glyph::Forward), "Bring forward", "", has) {
                roots.iter().rev().for_each(|&id| self.move_z(id, true));
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Back), "Send backward", "", has) {
                roots.iter().for_each(|&id| self.move_z(id, false));
                ui.close();
            }
            crate::glyphs::menu_sep(ui);
            if menu_item(ui, Some(Glyph::FlipH), "Flip horizontal", "Shift+H", has) {
                self.flip(true);
                ui.close();
            }
            if menu_item(ui, Some(Glyph::FlipV), "Flip vertical", "Shift+V", has) {
                self.flip(false);
                ui.close();
            }
        });
        crate::glyphs::menu_sep(ui);
        if menu_item(ui, None, if self.api.is_some() { "Stop REST API" } else { "Start REST API" }, "", true) {
            self.toggle_api(&ctx, 0);
            ui.close();
        }
        crate::glyphs::menu_submenu(ui, None, "Color profile", |ui| {
            for (v, label) in [(0u8, "sRGB"), (1, "Display P3")] {
                if crate::glyphs::menu_check(ui, self.color_profile == v, label, "") {
                    self.color_profile = v;
                }
            }
        });
        crate::glyphs::menu_submenu(ui, None, "Preferences", |ui| {
            for (label, big) in [("Small nudge", false), ("Big nudge", true)] {
                crate::glyphs::menu_submenu(ui, None, label, |ui| {
                    for v in if big { [10.0, 20.0, 50.0] } else { [1.0, 2.0, 5.0] } {
                        let cur = if big { &mut self.nudge_big } else { &mut self.nudge_small };
                        if crate::glyphs::menu_check(ui, *cur == v, &format!("{v} px"), "") {
                            *cur = v;
                        }
                    }
                });
            }
            crate::glyphs::menu_submenu(ui, None, "Mirror handles", |ui| {
                for (v, label) in [(0, "No mirroring"), (1, "Mirror angle"), (2, "Mirror angle and length")] {
                    if crate::glyphs::menu_check(ui, self.handle_mirror == v, label, "") {
                        self.handle_mirror = v;
                    }
                }
            });
            crate::glyphs::menu_sep(ui);
            for (on, label, key, which) in [
                (self.snap, "Snap to objects", "", 0),
                (self.snap_pixels, "Snap to pixel grid", "Ctrl+Shift+'", 1),
                (self.pixel_grid, "Show pixel grid", "Shift+'", 2),
                (self.rulers, "Show rulers", "Shift+R", 3),
                (self.layout_guides, "Show layout guides", "Shift+G", 4),
                (self.prop_labels, "Show property labels", "", 5),
                (self.spell, "Check spelling", "", 7),
                (self.light_ui, "Light interface", "", 8),
                (self.hide_ui, "Hide interface", "Ctrl+\\", 6),
            ] {
                if crate::glyphs::menu_check(ui, on, label, key) {
                    match which {
                        0 => self.snap ^= true,
                        1 => self.snap_pixels ^= true,
                        2 => self.pixel_grid ^= true,
                        3 => self.rulers ^= true,
                        4 => self.layout_guides ^= true,
                        5 => self.prop_labels ^= true,
                        7 => self.spell ^= true,
                        8 => self.light_ui ^= true,
                        _ => self.hide_ui ^= true,
                    }
                }
            }
        });
        crate::glyphs::menu_sep(ui);
        let target = self.present_target();
        if menu_item(ui, Some(Glyph::Present), "Present", "", target.is_some()) {
            if let Some(f) = target {
                self.start_present(f);
            }
            ui.close();
        }
    }

    // the file name (13px semibold, a chevron for the file menu), the page and layer count, Save; 64px
    fn file_header(&mut self, ui: &mut egui::Ui) {
        let title = self.file.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let r = fig::row(ui, 64.0);
        let galley = ui.painter().layout_no_wrap(title, fig::semi(13.0), TEXT());
        let tpos = Pos2::new(r.left() + 16.0, r.top() + 24.0 - galley.size().y / 2.0 + 0.3);
        let hit = Rect::from_min_size(tpos - Vec2::new(6.0, 4.0), galley.size() + Vec2::new(34.0, 8.0));
        let resp = ui.interact(hit, ui.id().with("file-menu"), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(hit, fig::RADIUS, fig::INPUT());
        }
        let w = galley.size().x;
        ui.painter().galley(tpos, galley, TEXT());
        paint_glyph_sized(ui.painter(), Pos2::new(tpos.x + w + 14.0, r.top() + 24.0), Glyph::ChevronDown, TEXT(), 9.5);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "File menu"));
        let ctx = ui.ctx().clone();
        egui::Popup::menu(&resp).style(crate::glyphs::fig_menu_style).show(|ui| {
            self.file_items(ui, &ctx);
        });
        // where the file lives: "Drafts" until it is saved, then its folder; a badge says it is a local file
        let place = if self.file.exists() { self.file.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Files".into()) } else { "Drafts".into() };
        let place_galley = ui.painter().layout_no_wrap(place.clone(), fig::reg(11.0), TEXT_SECONDARY());
        let place_pos = Pos2::new(r.left() + 16.0, r.top() + 49.0);
        fig::text(ui, place_pos, Align2::LEFT_CENTER, &place, fig::reg(11.0), TEXT_SECONDARY());
        let chip_galley = ui.painter().layout_no_wrap("Local".into(), fig::reg(11.0), BRAND);
        let chip = Rect::from_min_size(Pos2::new(place_pos.x + place_galley.size().x + 8.0, place_pos.y - 9.0), Vec2::new(chip_galley.size().x + 12.0, 18.0));
        ui.painter().rect_filled(chip, fig::RADIUS, BRAND.gamma_multiply(0.2));
        fig::text(ui, chip.center(), Align2::CENTER_CENTER, "Local", fig::reg(11.0), BRAND);
        if fig::icon_button(ui, Rect::from_center_size(Pos2::new(r.right() - 24.0, r.top() + 24.0), Vec2::splat(24.0)), Glyph::Save, "Save (Ctrl+S)", false).clicked() {
            self.save_to(self.file.clone());
        }
        if fig::icon_button(ui, Rect::from_center_size(Pos2::new(r.right() - 52.0, r.top() + 24.0), Vec2::splat(24.0)), Glyph::PanelLeft, "Minimize left panel (Shift+\\)", false).clicked() {
            self.left_min = true;
        }
        fig::rule(ui);
    }

    pub fn left_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        if self.left_tab != LeftTab::Assets {
            self.file_header(ui);
        }
        match self.left_tab {
            LeftTab::Layers if self.search_open => self.search_view(ui),
            LeftTab::Layers => self.layers(ui),
            LeftTab::Assets => self.assets_tab(ui),
            LeftTab::History => self.history_tab(ui),
            LeftTab::Comments => self.comments_tab(ui),
        }
    }

    // Pages: a 44px header with Find and Add, 24px rows 8px apart inset 8px, then a divider. A page row opens a menu on a right
    // click (rename, duplicate, delete) and can be dragged to a new place in the list.
    fn pages_section(&mut self, ui: &mut egui::Ui) {
        let r = fig::row(ui, 44.0);
        fig::text(ui, Pos2::new(r.left() + 16.0, r.top() + 22.0 + fig::TEXT_LIFT * 11.0), Align2::LEFT_CENTER, "Pages", fig::semi(11.0), TEXT());
        let plus = Rect::from_min_size(Pos2::new(r.right() - 32.0, r.top() + 10.0), Vec2::splat(24.0));
        if fig::icon_button(ui, plus.translate(Vec2::new(-28.0, 0.0)), Glyph::Search, "Search", false).clicked() {
            self.search_open = true;
            self.search_query.clear();
            self.search_focused = false;
        }
        let add_page = fig::icon_button(ui, plus, Glyph::Plus, "Add page", false).clicked();
        let (mut switch, mut delete, mut duplicate) = (None, None, None);
        let n = self.pages.len();
        let mut rects: Vec<Rect> = vec![];
        for i in 0..n {
            let row = fig::row(ui, 24.0);
            let rect = Rect::from_min_max(Pos2::new(row.left() + 8.0, row.top()), Pos2::new(row.right() - 8.0, row.bottom()));
            rects.push(rect);
            let resp = ui.interact(rect, ui.id().with(("page", i)), Sense::click_and_drag());
            let cur = i == self.cur;
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, cur, format!("Page {}", self.pages[i].name)));
            if cur {
                ui.painter().rect_filled(rect, fig::RADIUS, fig::INPUT());
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, fig::RADIUS, crate::theme::WELL());
            }
            let del = Rect::from_center_size(rect.right_center() - Vec2::new(14.0, 0.0), Vec2::splat(18.0));
            if self.renaming_page == Some(i) {
                let edit = Rect::from_min_max(rect.min + Vec2::new(4.0, 2.0), Pos2::new(del.left() - 2.0, rect.max.y - 2.0));
                let eid = egui::Id::new(("page-rename", i));
                let r = fig::put(ui, edit, egui::TextEdit::singleline(&mut self.pages[i].name).id(eid).font(fig::reg(11.0)).vertical_align(egui::Align::Center));
                // focus once when the editor opens: forcing it every frame kept a click elsewhere from leaving the field
                let flag = eid.with("opened");
                let started = ui.data(|d| d.get_temp::<bool>(flag)) != Some(true);
                if started {
                    ui.data_mut(|d| d.insert_temp(flag, true));
                    r.request_focus();
                }
                if (!started && !r.has_focus()) || r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    ui.data_mut(|d| d.remove::<bool>(flag));
                    self.end_rename_page(i);
                }
            } else {
                let font = if cur { fig::semi(11.0) } else { fig::reg(11.0) };
                fig::text(ui, Pos2::new(row.left() + 16.0, rect.center().y), Align2::LEFT_CENTER, &self.pages[i].name, font, if self.page_drag == Some(i) { TEXT_SECONDARY() } else { TEXT() });
            }
            if n > 1 && resp.hovered() && self.page_drag.is_none() {
                paint_glyph(ui.painter(), del.center(), Glyph::Delete, TEXT_SECONDARY());
            }
            if resp.drag_started() {
                self.page_drag = Some(i);
            }
            if resp.double_clicked() {
                self.start_rename_page(i);
            } else if resp.clicked() {
                if n > 1 && resp.interact_pointer_pos().is_some_and(|p| del.contains(p)) {
                    delete = Some(i);
                } else {
                    switch = Some(i);
                }
            }
            let mut rename = false;
            resp.context_menu(|ui| {
                if menu_item(ui, Some(Glyph::Pencil), "Rename", "Double-click", true) {
                    rename = true;
                    ui.close();
                }
                if menu_item(ui, Some(Glyph::Copy), "Duplicate page", "", true) {
                    duplicate = Some(i);
                    ui.close();
                }
                crate::glyphs::menu_sep(ui);
                if menu_item(ui, Some(Glyph::Delete), "Delete page", "", n > 1) {
                    delete = Some(i);
                    ui.close();
                }
            });
            if rename {
                self.start_rename_page(i);
            }
            if i + 1 < n {
                fig::row(ui, 8.0);
            }
        }
        // dragging a row: a line shows where it lands; letting go moves it
        let mut moved = None;
        if let Some(from) = self.page_drag {
            match ui.input(|i| i.pointer.latest_pos()) {
                Some(p) if ui.input(|i| i.pointer.any_down()) => {
                    let slot = rects.iter().position(|r| p.y < r.center().y).unwrap_or(n);
                    let (y, r0) = if slot < n { (rects[slot].top() - 4.0, rects[slot]) } else { (rects[n - 1].bottom() + 4.0, rects[n - 1]) };
                    ui.painter().line_segment([Pos2::new(r0.left(), y), Pos2::new(r0.right(), y)], Stroke::new(2.0, BRAND));
                    ui.ctx().request_repaint();
                }
                Some(p) => {
                    moved = Some((from, rects.iter().position(|r| p.y < r.center().y).unwrap_or(n)));
                    self.page_drag = None;
                }
                None => self.page_drag = None,
            }
        }
        fig::row(ui, 12.0);
        fig::rule(ui);
        if let Some((from, to)) = moved {
            self.move_page(from, to);
        } else if let Some(i) = delete {
            self.delete_page(i);
        } else if let Some(i) = duplicate {
            self.duplicate_page(i);
        } else if let Some(i) = switch {
            self.switch_page(i);
        }
        if add_page {
            self.add_page();
        }
    }

    // the search view that replaces the Pages and Layers lists: a field, then the pages and the layers whose names match
    fn search_view(&mut self, ui: &mut egui::Ui) {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.search_open = false;
            return;
        }
        let r = fig::row(ui, 44.0);
        let field = Rect::from_min_max(Pos2::new(r.left() + 16.0, r.top() + 10.0), Pos2::new(r.right() - 44.0, r.top() + 34.0));
        fig::input_bg(ui, field, false);
        paint_glyph_sized(ui.painter(), Pos2::new(field.left() + 12.0, field.center().y), Glyph::Search, TEXT_SECONDARY(), 13.0);
        let edit = fig::text_edit(ui, field.with_min_x(field.left() + 18.0), &mut self.search_query, "Search", "Search layers and pages");
        if !self.search_focused {
            edit.request_focus();
            self.search_focused = true;
        }
        if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 36.0, r.top() + 10.0), Vec2::splat(24.0)), Glyph::Close, "Close search (Esc)", false).clicked() {
            self.search_open = false;
        }
        fig::rule(ui);
        let q = self.search_query.trim().to_lowercase();
        if q.is_empty() {
            ui.add_space(14.0);
            ui.vertical_centered(|ui| {
                row_icon(ui, Glyph::Search, TEXT_TERTIARY());
                ui.label(RichText::new("Search layers and pages").color(TEXT_SECONDARY()));
            });
            return;
        }
        let pages: Vec<usize> = (0..self.pages.len()).filter(|&i| self.pages[i].name.to_lowercase().contains(&q)).collect();
        let layers: Vec<usize> = (0..self.shapes.len()).rev().filter(|&i| self.shapes[i].name.to_lowercase().contains(&q)).collect();
        let (mut go_page, mut go_layer) = (None, None);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if pages.is_empty() && layers.is_empty() {
                ui.add_space(14.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("No results").color(TEXT_SECONDARY())));
            }
            if !pages.is_empty() {
                let h = fig::row(ui, 32.0);
                fig::ptext(ui.painter(), Pos2::new(h.left() + 16.0, h.center().y), Align2::LEFT_CENTER, "Pages", fig::semi(11.0), TEXT());
                for &i in &pages {
                    let (r, resp, _) = list_row(ui, ("search-page", i), &format!("Page {}", self.pages[i].name), i == self.cur);
                    paint_glyph_sized(ui.painter(), Pos2::new(r.left() + 24.0, r.center().y), Glyph::File, TEXT_SECONDARY(), 13.0);
                    fig::ptext(&ui.painter().with_clip_rect(r), Pos2::new(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, &self.pages[i].name, fig::reg(11.0), TEXT());
                    if resp.clicked() {
                        go_page = Some(i);
                    }
                }
            }
            if !layers.is_empty() {
                let h = fig::row(ui, 32.0);
                fig::ptext(ui.painter(), Pos2::new(h.left() + 16.0, h.center().y), Align2::LEFT_CENTER, "Layers", fig::semi(11.0), TEXT());
                for &i in &layers {
                    let s = &self.shapes[i];
                    let (r, resp, _) = list_row(ui, ("search-layer", s.id), &format!("Layer {}", s.name), self.sel.contains(&s.id));
                    paint_glyph_sized(ui.painter(), Pos2::new(r.left() + 24.0, r.center().y), layer_glyph(s), TEXT_SECONDARY(), 13.0);
                    let parent = s.parent.and_then(|p| self.index_of(p)).map(|p| self.shapes[p].name.clone()).unwrap_or_default();
                    let clip = r.with_max_x(r.right() - 8.0);
                    let p = ui.painter().with_clip_rect(clip);
                    let t = fig::ptext(&p, Pos2::new(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, &s.name, fig::reg(11.0), TEXT());
                    if !parent.is_empty() && t.right() + 12.0 < r.right() - 24.0 {
                        fig::ptext(&p, Pos2::new(r.right() - 16.0, r.center().y), Align2::RIGHT_CENTER, &parent, fig::reg(11.0), TEXT_TERTIARY());
                    }
                    if resp.clicked() {
                        go_layer = Some(s.id);
                    }
                }
            }
        });
        if let Some(i) = go_page {
            self.switch_page(i);
            self.search_open = false;
        }
        if let Some(id) = go_layer {
            self.sel = vec![id];
            self.zoom_fit(true);
            self.search_open = false;
        }
    }

    // layer rows top-first; children of collapsed containers are skipped
    fn layer_rows(&self, parent: Option<u64>, depth: usize, out: &mut Vec<(usize, usize)>) {
        for i in (0..self.shapes.len()).rev().filter(|&i| self.shapes[i].parent == parent) {
            out.push((i, depth));
            let s = &self.shapes[i];
            if s.kind.is_container() && !self.collapsed.contains(&s.id) {
                self.layer_rows(Some(s.id), depth + 1, out);
            }
        }
    }

    fn apply_drop(&mut self, id: u64, drop: Drop) {
        // dragging a selected row drags the whole selection
        let ids = if self.sel.contains(&id) { self.sel_roots() } else { vec![id] };
        let parent_of = |app: &App, t: u64| app.index_of(t).and_then(|i| app.shapes[i].parent);
        let (parent, mut anchor) = match drop {
            Drop::Into(t) => (Some(t), None),
            Drop::Above(t) => (parent_of(self, t), Some((t, true))),
            Drop::Below(t) => (parent_of(self, t), Some((t, false))),
        };
        for id in ids {
            self.move_to(id, parent, anchor);
            if anchor.is_some_and(|(_, above)| above) {
                anchor = Some((id, true)); // keep the moved rows in their original order
            }
        }
    }

    fn layers(&mut self, ui: &mut egui::Ui) {
        self.pages_section(ui);
        // Layers: a 44px header with Collapse; Find (in the Pages header) adds a search field under it
        let r = fig::row(ui, 44.0);
        fig::text(ui, Pos2::new(r.left() + 16.0, r.top() + 22.0 + fig::TEXT_LIFT * 11.0), Align2::LEFT_CENTER, "Layers", fig::semi(11.0), TEXT());
        if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 32.0, r.top() + 10.0), Vec2::splat(24.0)), Glyph::CollapseAll, "Collapse layers", false).clicked() {
            self.collapsed = self.shapes.iter().filter(|s| s.kind.is_container()).map(|s| s.id).collect();
        }
        let finding = ui.data(|d| d.get_temp::<bool>(egui::Id::new("find-layers"))).unwrap_or(false) || !self.layer_filter.is_empty();
        if finding {
            search_field(ui, &mut self.layer_filter, "Search layers", "Find layers");
            fig::row(ui, 4.0);
        }
        let filter = self.layer_filter.to_lowercase();
        let mut rows = vec![];
        if filter.is_empty() {
            self.layer_rows(None, 0, &mut rows);
        } else {
            rows = (0..self.shapes.len()).rev().filter(|&i| self.shapes[i].name.to_lowercase().contains(&filter)).map(|i| (i, 0)).collect();
        }
        let (range_mod, toggle_mod) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
        let mut act = None;
        let mut drag_start = None;
        let mut dropped = false;
        let mut rects: Vec<(u64, Rect, usize, bool)> = vec![];
        let mut row_resps: Vec<(u64, egui::Response)> = vec![];
        let mut rename_done = false;
        let mut rename_text: Option<(u64, String)> = None;
        let renaming = self.renaming;
        let can_drag = filter.is_empty();
        // a drag that starts on the eye or lock sets that state on every row it passes over (Figma)
        let mut col_paint = self.col_paint;
        let mut painted: Vec<u64> = vec![];
        let (pointer_pos, pointer_down, press_origin) = ui.input(|i| (i.pointer.latest_pos(), i.pointer.any_down(), i.pointer.press_origin()));
        if !pointer_down {
            col_paint = None;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if rows.is_empty() {
                ui.add_space(14.0);
                ui.vertical_centered(|ui| {
                    row_icon(ui, Glyph::Layers, TEXT_TERTIARY());
                    if filter.is_empty() {
                        ui.label(RichText::new("No layers yet").color(TEXT_SECONDARY()));
                        ui.label(RichText::new("Press R, O or F and drag on the canvas,\nor drop an image file here").color(TEXT_TERTIARY()).size(11.0));
                    } else {
                        ui.label(RichText::new("No layers match").color(TEXT_SECONDARY()));
                    }
                });
            }
            for &(i, depth) in &rows {
                let s = &self.shapes[i];
                // Figma: 32px rows, a 24px highlight inset 8px on the left and flush right, 24px per level;
                // chevron at 20, icon at 36, name at 51 (11px)
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click_and_drag());
                rects.push((s.id, rect, depth, s.kind.is_container()));
                row_resps.push((s.id, resp.clone()));
                let selected = self.sel.contains(&s.id);
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, format!("Layer {}", s.name)));
                let dx = depth as f32 * 24.0;
                let arrow = Rect::from_center_size(Pos2::new(rect.left() + 20.0 + dx, rect.center().y), Vec2::splat(16.0));
                let eye = Rect::from_center_size(Pos2::new(rect.right() - 20.0, rect.center().y), Vec2::splat(20.0));
                let lock = eye.translate(Vec2::new(-24.0, 0.0));
                let hl = Rect::from_min_max(Pos2::new(rect.left() + 8.0, rect.top() + 4.0), Pos2::new(rect.right(), rect.bottom() - 4.0));
                let corners = egui::CornerRadius { nw: fig::RADIUS, sw: fig::RADIUS, ne: 0, se: 0 };
                let p = &ui.painter().clone();
                if selected {
                    p.rect_filled(hl, corners, fig::SELECTED());
                } else if resp.hovered() {
                    p.rect_filled(hl, corners, fig::INPUT());
                }
                let special = s.component || s.instance || s.src.is_some();
                let ink = if !s.visible {
                    TEXT_TERTIARY()
                } else if special {
                    fig::COMPONENT_INK
                } else {
                    TEXT()
                };
                if s.kind.is_container() {
                    let g = if self.collapsed.contains(&s.id) { Glyph::ChevronRight } else { Glyph::ChevronDown };
                    paint_glyph_sized(p, arrow.center(), g, TEXT_TERTIARY(), 9.5);
                }
                let icon_ink = if special { fig::COMPONENT_INK.gamma_multiply(0.6) } else if selected { TEXT() } else { TEXT_TERTIARY() };
                // like Figma, a frame with auto layout shows its direction instead of the plain frame icon
                let glyph = layer_glyph(s);
                paint_glyph_sized(p, Pos2::new(rect.left() + 36.0 + dx, rect.center().y), glyph, icon_ink, 13.0);
                let name_pos = Pos2::new(rect.left() + 51.3 + dx, rect.center().y);
                if renaming == Some(s.id) {
                    let mut text = s.name.clone();
                    let edit_rect = Rect::from_min_max(Pos2::new(name_pos.x - 2.0, rect.min.y + 2.0), Pos2::new(lock.left() - 2.0, rect.max.y - 2.0));
                    // an explicit id: the row and its editor share a rect, and equal auto ids make egui flash its red clash border
                    let eid = egui::Id::new(("layer-rename", s.id));
                    let r = fig::put(ui, edit_rect, egui::TextEdit::singleline(&mut text).id(eid).font(fig::reg(11.0)).vertical_align(egui::Align::Center));
                    // focus once, when the editor opens: forcing it every frame kept a click elsewhere from leaving the field
                    let started = ui.data(|d| d.get_temp::<u64>(eid)) != Some(s.id);
                    if started {
                        ui.data_mut(|d| d.insert_temp(eid, s.id));
                        r.request_focus();
                        if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), eid) {
                            st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()))));
                            st.store(ui.ctx(), eid);
                        }
                    }
                    if !started && !r.has_focus() || r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        ui.data_mut(|d| d.remove::<u64>(eid));
                        rename_done = true;
                    }
                    rename_text = Some((s.id, text));
                } else {
                    let name_clip = Rect::from_min_max(rect.min, Pos2::new(lock.left() - 2.0, rect.max.y));
                    fig::ptext(&p.with_clip_rect(name_clip), name_pos, Align2::LEFT_CENTER, &s.name, fig::reg(11.0), ink);
                }
                if resp.hovered() || !s.visible {
                    paint_eye(p, eye.center(), TEXT_SECONDARY(), !s.visible);
                }
                if resp.hovered() || s.locked {
                    paint_lock(p, lock.center(), TEXT_SECONDARY(), s.locked);
                }
                if resp.drag_started() && press_origin.is_some_and(|o| eye.contains(o) || lock.contains(o)) {
                    let on_eye = press_origin.is_some_and(|o| eye.contains(o));
                    col_paint = Some((on_eye, if on_eye { s.visible ^ true } else { s.locked ^ true }));
                } else if can_drag && col_paint.is_none() && resp.drag_started() {
                    drag_start = Some(s.id);
                }
                if col_paint.is_some() && pointer_pos.is_some_and(|p| rect.contains(p)) {
                    painted.push(s.id);
                }
                dropped |= resp.drag_stopped();
                if resp.secondary_clicked() && !selected {
                    act = Some(RowAct::Select(s.id)); // a right click picks the row, as on the canvas
                } else if resp.double_clicked() {
                    act = Some(RowAct::Rename(s.id));
                } else if resp.clicked() {
                    // no pointer position: keyboard or accessibility activation, which selects
                    act = Some(match resp.interact_pointer_pos() {
                        Some(pos) if eye.contains(pos) => RowAct::Visible(s.id),
                        Some(pos) if lock.contains(pos) => RowAct::Locked(s.id),
                        Some(pos) if s.kind.is_container() && arrow.contains(pos) => RowAct::Collapse(s.id),
                        _ => RowAct::Select(s.id),
                    });
                }
            }
            // drag a row to reorder / reparent: top quarter = above, bottom quarter = below, middle of a container = into
            if drag_start.is_some() {
                self.row_drag = drag_start;
            }
            let Some(dragging) = self.row_drag else { return };
            let target = ui.input(|i| i.pointer.latest_pos()).and_then(|p| {
                let &(id, r, depth, container) = rects.iter().find(|(_, r, ..)| r.contains(p))?;
                let f = (p.y - r.top()) / r.height();
                let d = if f < 0.25 || (!container && f < 0.5) {
                    Drop::Above(id)
                } else if f > 0.75 || !container {
                    Drop::Below(id)
                } else {
                    Drop::Into(id)
                };
                Some((d, r, depth))
            });
            if let Some((d, r, depth)) = target {
                let x = r.left() + 8.0 + depth as f32 * 24.0;
                let line = Stroke::new(2.0, BRAND);
                match d {
                    Drop::Above(_) => {
                        ui.painter().line_segment([Pos2::new(x, r.top()), Pos2::new(r.right(), r.top())], line);
                    }
                    Drop::Below(_) => {
                        ui.painter().line_segment([Pos2::new(x, r.bottom()), Pos2::new(r.right(), r.bottom())], line);
                    }
                    Drop::Into(_) => {
                        ui.painter().rect_stroke(r, R_SMALL, line, StrokeKind::Inside);
                    }
                }
            }
            if dropped {
                self.row_drag = None;
                if let Some((d, ..)) = target {
                    self.apply_drop(dragging, d);
                }
            }
        });
        self.col_paint = col_paint;
        if let Some((eye, value)) = col_paint {
            for id in painted {
                if let Some(i) = self.index_of(id) {
                    if eye { self.shapes[i].visible = value } else { self.shapes[i].locked = value }
                }
            }
        }
        if let Some((id, text)) = rename_text {
            if let Some(i) = self.index_of(id) {
                self.shapes[i].name = text;
            }
        }
        if rename_done {
            self.renaming = None;
        }
        match act {
            Some(RowAct::Select(id)) => {
                // Shift: every row between the anchor and this one; Ctrl: add or remove this row
                let anchor = self.layer_anchor.filter(|a| range_mod && rows.iter().any(|&(i, _)| self.shapes[i].id == *a));
                if let Some(a) = anchor {
                    let at = |t: u64| rows.iter().position(|&(i, _)| self.shapes[i].id == t);
                    if let (Some(x), Some(y)) = (at(a), at(id)) {
                        self.sel = rows[x.min(y)..=x.max(y)].iter().map(|&(i, _)| &self.shapes[i]).filter(|s| !s.locked).map(|s| s.id).collect();
                    }
                } else {
                    if !toggle_mod && !range_mod {
                        self.sel.clear();
                    }
                    match self.sel.iter().position(|&x| x == id) {
                        Some(k) if toggle_mod || range_mod => {
                            self.sel.remove(k);
                        }
                        Some(_) => {}
                        None => self.sel.push(id),
                    }
                    self.layer_anchor = Some(id);
                }
            }
            Some(RowAct::Rename(id)) => self.renaming = Some(id),
            Some(RowAct::Visible(id)) => {
                if let Some(i) = self.index_of(id) {
                    self.shapes[i].visible ^= true;
                }
            }
            Some(RowAct::Locked(id)) => {
                if let Some(i) = self.index_of(id) {
                    self.shapes[i].locked ^= true;
                }
            }
            Some(RowAct::Collapse(id)) => {
                let collapse = !self.collapsed.contains(&id);
                // Alt+click: the layer and everything inside it
                let all = ui.input(|i| i.modifiers.alt);
                let ids: Vec<u64> = if all {
                    self.shapes.iter().filter(|s| s.kind.is_container() && (s.id == id || self.is_inside(s.id, id))).map(|s| s.id).collect()
                } else {
                    vec![id]
                };
                for k in ids {
                    if collapse { self.collapsed.insert(k); } else { self.collapsed.remove(&k); }
                }
            }
            None => {}
        }
        // a right click on a row: Figma's layer menu is the canvas menu with Rename in front
        let ctx = ui.ctx().clone();
        for (id, resp) in row_resps {
            egui::Popup::context_menu(&resp).show(|ui| {
                if menu_item(ui, Some(Glyph::Pencil), "Rename", "Double-click", true) {
                    self.renaming = Some(id);
                    ui.close();
                }
                crate::glyphs::menu_sep(ui);
                self.canvas_menu(ui, &ctx);
            });
        }
    }

    // Assets: a 44px header, a search field, then Components / Color styles / Images, each a foldable group
    // of 32px rows laid out like layer rows (icon at 36, name at 51)
    fn assets_tab(&mut self, ui: &mut egui::Ui) {
        // Figma: a 48px header (13px semibold, a book button), the search field 13px under it with a filter
        // button beside it, then the lists 4px under the field
        let head = fig::row(ui, 48.0);
        fig::ptext(ui.painter(), Pos2::new(head.left() + 16.0, head.center().y), Align2::LEFT_CENTER, "Assets", fig::semi(13.0), TEXT());
        if fig::icon_button(ui, Rect::from_center_size(Pos2::new(head.right() - 24.0, head.center().y), Vec2::splat(24.0)), Glyph::BookOpen, "Add a library", false).clicked() {
            self.add_library();
        }
        fig::row(ui, 13.0);
        let fid = egui::Id::new("asset-filter");
        let mut filter: String = ui.data(|d| d.get_temp(fid)).unwrap_or_default();
        let row = fig::row(ui, 24.0);
        let field = Rect::from_min_max(Pos2::new(row.left() + 16.0, row.top()), Pos2::new(row.right() - 44.0, row.bottom()));
        fig::input_bg(ui, field, false);
        paint_glyph_sized(ui.painter(), Pos2::new(field.left() + 12.0, field.center().y), Glyph::Search, TEXT(), 13.0);
        fig::text_edit(ui, field.with_min_x(field.left() + 18.0), &mut filter, "Search assets", "Search assets");
        fig::icon_button(ui, Rect::from_min_size(Pos2::new(row.right() - 36.0, row.top()), Vec2::splat(24.0)), Glyph::Sliders, "Filter assets", false);
        ui.data_mut(|d| d.insert_temp(fid, filter.clone()));
        fig::row(ui, 4.0);
        let filter = filter.to_lowercase();
        let hit = |name: &str| filter.is_empty() || name.to_lowercase().contains(&filter);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // components
            let comps: Vec<(u64, String)> = self.components().into_iter().filter(|(_, n)| hit(n)).collect();
            let (open, create) = fold_head(ui, "Components", Some((Glyph::Plus, "Create component from selection")));
            if create {
                self.make_component();
            }
            if open {
                if comps.is_empty() {
                    empty_row(ui, if filter.is_empty() { "Select a frame and press Ctrl+Alt+K" } else { "No components match" });
                }
                for (id, name) in comps {
                    let (r, resp, hot) = list_row(ui, ("asset-comp", id), &format!("Component {name}"), false);
                    paint_glyph_sized(ui.painter(), Pos2::new(r.left() + 36.0, r.center().y), Glyph::Component, fig::COMPONENT_INK, 13.0);
                    fig::ptext(&ui.painter().with_clip_rect(r.with_max_x(r.right() - 64.0)), Pos2::new(r.left() + 51.3, r.center().y), Align2::LEFT_CENTER, &name, fig::reg(11.0), TEXT());
                    let go = hot && fig::icon_button(ui, row_button(r, 1), Glyph::Crosshair, "Go to the main component", false).clicked();
                    let insert = hot && fig::icon_button(ui, row_button(r, 0), Glyph::Plus, "Insert an instance in the view", false).clicked();
                    if insert {
                        self.instantiate_in_view(id);
                    } else if go || resp.clicked() {
                        self.sel = vec![id];
                        self.zoom_fit(true);
                    }
                }
                fig::row(ui, 8.0);
            }
            fig::rule(ui);
            // libraries: publish this file, add a file, and insert the components of the added ones
            let (open, add) = fold_head(ui, "Libraries", Some((Glyph::Plus, "Add a library file")));
            if add {
                self.add_library();
            }
            if open {
                let r = fig::row(ui, 32.0);
                let r2 = fig::row(ui, 32.0);
                fig::text_input(ui, Rect::from_min_size(Pos2::new(r2.left() + 16.0, r2.top() + 4.0), Vec2::new(r2.width() - 32.0, 24.0)), &mut self.lib_description, "What changed in this publish", "What changed (sent with the publish)");
                if fig::button(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top() + 4.0), Vec2::new(r.width() - 32.0, 24.0)), "Publish this file as a library").clicked() {
                    self.publish_library();
                }
                if self.libs.is_empty() {
                    empty_row(ui, "Add a .drlib file to use its components and styles");
                }
                for lib in self.libs.clone() {
                    let r = fig::row(ui, 32.0);
                    fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, format!("{}  v{}", lib.name, lib.version), fig::semi(11.0), TEXT_SECONDARY());
                    // what the publisher said changed in the newest version
                    if let Some(note) = self.lib_files.get(&lib.id).map(|f| f.description.clone()).filter(|d| !d.is_empty()) {
                        ui.interact(r, ui.id().with(("lib-note", lib.id)), Sense::hover()).on_hover_text(note);
                    }
                    let known = self.library_file(lib.id).map(|f| (f.version, f.components.iter().map(|c| (c.root, c.name.clone())).collect::<Vec<_>>()));
                    if self.library_has_update(lib.id) && fig::button(ui, Rect::from_min_size(Pos2::new(r.right() - 140.0, r.top() + 4.0), Vec2::new(72.0, 24.0)), "Update").clicked() {
                        self.update_from_library(lib.id);
                    }
                    if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 64.0, r.top() + 4.0), Vec2::splat(24.0)), Glyph::RotateCcw, "Check the library file for a new version", false).clicked() {
                        self.refresh_library(lib.id);
                    }
                    let swap = fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 92.0, r.top() + 4.0), Vec2::splat(24.0)), Glyph::Swap, "Swap this library's components for another library's", false);
                    let others: Vec<(u64, String)> = self.libs.iter().filter(|l| l.id != lib.id).map(|l| (l.id, l.name.clone())).collect();
                    let mut to = None;
                    egui::Popup::menu(&swap).show(|ui| {
                        if others.is_empty() {
                            ui.label("Add another library first");
                        }
                        for (oid, oname) in &others {
                            if menu_item(ui, None, &format!("Swap to {oname}"), "", true) {
                                to = Some(*oid);
                                ui.close();
                            }
                        }
                    });
                    if let Some(oid) = to {
                        self.swap_library(lib.id, oid);
                    }
                    if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 36.0, r.top() + 4.0), Vec2::splat(24.0)), Glyph::Minus, "Remove the library (inserted components stay)", false).clicked() {
                        self.remove_library(lib.id);
                    }
                    let Some((_, comps)) = known else {
                        empty_row(ui, "The library file could not be read");
                        continue;
                    };
                    for (root, name) in comps.into_iter().filter(|(_, n)| hit(n)) {
                        let (r, resp, hot) = list_row(ui, ("asset-lib", lib.id, root), &format!("Library component {name}"), false);
                        paint_glyph_sized(ui.painter(), Pos2::new(r.left() + 36.0, r.center().y), Glyph::Component, fig::COMPONENT_INK, 13.0);
                        fig::ptext(&ui.painter().with_clip_rect(r.with_max_x(r.right() - 40.0)), Pos2::new(r.left() + 51.3, r.center().y), Align2::LEFT_CENTER, &name, fig::reg(11.0), TEXT());
                        let insert = hot && fig::icon_button(ui, row_button(r, 0), Glyph::Plus, "Insert an instance in the view", false).clicked();
                        if insert || resp.double_clicked() {
                            self.insert_library_component(lib.id, root);
                        }
                    }
                }
                fig::row(ui, 8.0);
            }
            fig::rule(ui);
            // color styles: swatch, name (double-click to rename), hex; delete on hover
            let (open, add) = fold_head(ui, "Color styles", Some((Glyph::Plus, "New color style")));
            if add {
                let id = self.new_id();
                self.styles.push(ColorStyle { id, name: format!("Style {}", self.styles.len() + 1), color: Color32::from_rgb(0x0d, 0x99, 0xff), description: String::new() });
                ui.data_mut(|d| d.insert_temp(egui::Id::new("style-rename"), id));
            }
            if open {
                let rename_id = egui::Id::new("style-rename");
                let renaming: Option<u64> = ui.data(|d| d.get_temp(rename_id));
                let mut remove = None;
                if self.styles.is_empty() {
                    empty_row(ui, "Press + to add a color style");
                }
                for (k, st) in self.styles.iter_mut().enumerate().filter(|(_, st)| hit(&st.name)) {
                    let (r, resp, hot) = list_row(ui, ("asset-style", st.id), &format!("Color style {}", st.name), false);
                    if !st.description.is_empty() && hot {
                        resp.clone().on_hover_text(st.description.clone());
                    }
                    fig::swatch(ui, Pos2::new(r.left() + 36.0, r.center().y), &mut st.color, &format!("Color of {}", st.name));
                    let name_r = Rect::from_min_max(Pos2::new(r.left() + 47.0, r.top() + 4.0), Pos2::new(r.right() - 72.0, r.bottom() - 4.0));
                    if renaming == Some(st.id) {
                        let edit = fig::text_input(ui, name_r, &mut st.name, "Style name", "Style name");
                        if !edit.has_focus() && !edit.lost_focus() {
                            edit.request_focus();
                        }
                        if edit.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            ui.data_mut(|d| d.remove::<u64>(rename_id));
                        }
                    } else {
                        fig::ptext(&ui.painter().with_clip_rect(name_r), Pos2::new(r.left() + 51.3, r.center().y), Align2::LEFT_CENTER, &st.name, fig::reg(11.0), TEXT());
                        if resp.double_clicked() {
                            ui.data_mut(|d| d.insert_temp(rename_id, st.id));
                        }
                    }
                    if hot {
                        if fig::icon_button(ui, row_button(r, 0), Glyph::Delete, "Delete style", false).clicked() {
                            remove = Some(k);
                        }
                    } else {
                        let hex = format!("{:02X}{:02X}{:02X}", st.color.r(), st.color.g(), st.color.b());
                        fig::ptext(ui.painter(), Pos2::new(r.right() - 16.0, r.center().y), Align2::RIGHT_CENTER, hex, fig::reg(11.0), TEXT_SECONDARY());
                    }
                }
                if let Some(k) = remove {
                    self.delete_style(k);
                }
                fig::row(ui, 8.0);
            }
            fig::rule(ui);
            // images
            let images: Vec<(u64, String)> = self.images.iter().filter(|a| hit(&a.name)).map(|a| (a.id, a.name.clone())).collect();
            let (open, _) = fold_head(ui, "Images", None);
            if open {
                if images.is_empty() {
                    empty_row(ui, "Drop an image file onto the canvas");
                }
                for (id, name) in images {
                    let (r, ..) = list_row(ui, ("asset-image", id), &format!("Image {name}"), false);
                    paint_glyph_sized(ui.painter(), Pos2::new(r.left() + 36.0, r.center().y), Glyph::Image, TEXT_SECONDARY(), 13.0);
                    fig::ptext(&ui.painter().with_clip_rect(r.with_max_x(r.right() - 8.0)), Pos2::new(r.left() + 51.3, r.center().y), Align2::LEFT_CENTER, &name, fig::reg(11.0), TEXT());
                }
                fig::row(ui, 8.0);
            }
            fig::rule(ui);
        });
    }

    // Version history: a 44px header with Save, a name field, then the current version and the saved ones
    // newest first on a timeline (a dot at 24, the name at 40, the time and size under it); restore and
    // delete on hover
    fn history_tab(&mut self, ui: &mut egui::Ui) {
        let head = left_header(ui, "Version history", &[(Glyph::Plus, "Save version", false), (Glyph::Close, "Close version history", false)]);
        if head[1].clicked() {
            self.left_tab = LeftTab::Layers;
        }
        let save = head[0].clicked();
        let row = fig::row(ui, 24.0);
        let field = Rect::from_min_max(Pos2::new(row.left() + 16.0, row.top()), Pos2::new(row.right() - 8.0 - 48.0 - 8.0, row.bottom()));
        let edit = fig::text_input(ui, field, &mut self.version_name, "Version name", "Version name");
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let button = fig::button(ui, Rect::from_min_max(Pos2::new(field.right() + 8.0, row.top()), Pos2::new(row.right() - 8.0, row.bottom())), "Save").clicked();
        if save || enter || button {
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let name = if self.version_name.trim().is_empty() { format!("Version {}", self.versions.len() + 1) } else { std::mem::take(&mut self.version_name) };
            self.versions.push(Version { name, secs, shapes: self.shapes.clone() });
        }
        fig::row(ui, 8.0);
        fig::rule(ui);
        let mut restore = None;
        let mut delete = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            fig::row(ui, 8.0);
            let n = self.versions.len();
            let entry = |ui: &mut egui::Ui, title: &str, sub: &str, current: bool, last: bool| -> (Rect, egui::Response, bool) {
                let r = fig::row(ui, 48.0);
                let hl = Rect::from_min_max(Pos2::new(r.left() + 8.0, r.top() + 2.0), Pos2::new(r.right() - 8.0, r.bottom() - 2.0));
                let resp = ui.interact(hl, ui.id().with(("version", title, r.top() as i32)), Sense::click());
                let hot = ui.rect_contains_pointer(hl);
                if hot {
                    ui.painter().rect_filled(hl, fig::RADIUS, fig::INPUT());
                }
                let dot = Pos2::new(r.left() + 24.0, r.top() + 17.0);
                if !last {
                    ui.painter().line_segment([dot + Vec2::new(0.0, 5.0), Pos2::new(dot.x, r.bottom() + 12.0)], Stroke::new(1.0, fig::RULE()));
                }
                if current {
                    ui.painter().circle_filled(dot, 4.0, fig::SHARE);
                } else {
                    ui.painter().circle_stroke(dot, 3.5, Stroke::new(1.0, TEXT_SECONDARY()));
                }
                let clip = ui.painter().with_clip_rect(hl.with_max_x(hl.right() - if hot && !current { 60.0 } else { 8.0 }));
                fig::ptext(&clip, Pos2::new(r.left() + 40.0, dot.y), Align2::LEFT_CENTER, title, if current { fig::semi(11.0) } else { fig::reg(11.0) }, TEXT());
                fig::ptext(&clip, Pos2::new(r.left() + 40.0, r.top() + 33.0), Align2::LEFT_CENTER, sub, fig::reg(11.0), TEXT_SECONDARY());
                let (t, s) = (title.to_owned(), sub.to_owned());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("{t}, {s}")));
                (r, resp, hot)
            };
            let layers = |k: usize| format!("{k} layer{}", if k == 1 { "" } else { "s" });
            entry(ui, "Current version", &layers(self.shapes.len()), true, n == 0);
            for k in (0..n).rev() {
                let v = &self.versions[k];
                let sub = format!("{}  ·  {}", when(v.secs), layers(v.shapes.len()));
                let (r, _, hot) = entry(ui, &v.name.clone(), &sub, false, k == 0);
                if hot {
                    let b = |slot: f32| Rect::from_min_size(Pos2::new(r.right() - 40.0 - 28.0 * slot, r.top() + 5.0), Vec2::splat(24.0));
                    if fig::icon_button(ui, b(0.0), Glyph::Delete, "Delete this version", false).clicked() {
                        delete = Some(k);
                    }
                    if fig::icon_button(ui, b(1.0), Glyph::RotateCcw, "Restore this version", false).clicked() {
                        restore = Some(k);
                    }
                }
            }
        });
        if let Some(k) = restore {
            // restoring is itself undoable
            let snap = self.snap();
            self.undo.push(snap);
            self.redo.clear();
            self.shapes = self.versions[k].shapes.clone();
            self.sel.clear();
            self.sizes.clear();
            self.status = format!("restored {}", self.versions[k].name);
        }
        if let Some(k) = delete {
            self.versions.remove(k);
        }
    }

    // Comments: a 44px header with Show resolved, then one block per comment on this page: a numbered
    // avatar at 28, "#n" at 48 with resolve / delete on hover, the text under it (editable in place)
    fn comments_tab(&mut self, ui: &mut egui::Ui) {
        let show_resolved = self.show_resolved;
        let head = left_header(ui, "Comments", &[(Glyph::CircleCheck, "Show resolved comments", show_resolved), (Glyph::Close, "Close comments", false)]);
        if head[0].clicked() {
            self.show_resolved = !show_resolved;
        }
        if head[1].clicked() {
            self.left_tab = LeftTab::Layers;
        }
        fig::rule(ui);
        let mut delete = None;
        let mut goto = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let cur = self.cur;
            let sel = self.comment_sel;
            let mut shown = 0;
            for (n, c) in self.comments.iter_mut().filter(|c| c.page == cur).enumerate() {
                if c.resolved && !show_resolved {
                    continue;
                }
                shown += 1;
                let bg = ui.painter().add(egui::Shape::Noop);
                let top = fig::row(ui, 32.0);
                let block_top = top.top();
                let avatar = Pos2::new(top.left() + 28.0, top.top() + 18.0);
                ui.painter().circle_filled(avatar, 12.0, if c.resolved { fig::INPUT() } else { fig::SHARE });
                fig::ptext(ui.painter(), avatar, Align2::CENTER_CENTER, (n + 1).to_string(), fig::semi(11.0), Color32::WHITE);
                let title = Rect::from_min_max(Pos2::new(top.left() + 8.0, top.top()), Pos2::new(top.right() - 68.0, top.bottom()));
                let head = ui.interact(title, ui.id().with(("comment", c.id)), Sense::click());
                head.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Go to comment {}", n + 1)));
                fig::ptext(ui.painter(), Pos2::new(top.left() + 48.0, avatar.y), Align2::LEFT_CENTER, format!("#{}", n + 1), fig::semi(11.0), TEXT());
                if c.resolved {
                    fig::ptext(ui.painter(), Pos2::new(top.left() + 72.0, avatar.y), Align2::LEFT_CENTER, "Resolved", fig::reg(11.0), TEXT_SECONDARY());
                }
                if head.clicked() {
                    goto = Some(c.id);
                }
                // the text, wrapped from 48 to 16px from the right
                let w = ui.available_width();
                let galley_h = ui.painter().layout(if c.text.is_empty() { "W".into() } else { c.text.clone() }, fig::reg(11.0), TEXT(), (w - 48.0 - 16.0 - 8.0).max(40.0)).size().y;
                let body = fig::row(ui, galley_h + 12.0);
                let field = Rect::from_min_max(Pos2::new(body.left() + 44.0, body.top()), Pos2::new(body.right() - 16.0, body.bottom()));
                if ui.rect_contains_pointer(field) {
                    fig::input_bg(ui, field, false);
                }
                let edit = fig::put(ui, field.shrink2(Vec2::new(4.0, 6.0)), egui::TextEdit::multiline(&mut c.text).id(egui::Id::new("comment-text")).frame(egui::Frame::NONE).font(fig::reg(11.0)).margin(egui::Margin::ZERO).desired_rows(1).hint_text("Write a comment").text_color(TEXT()));
                edit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, format!("Comment {}", n + 1)));
                let end = fig::row(ui, 12.0);
                let block = Rect::from_min_max(Pos2::new(end.left(), block_top), end.max);
                if sel == Some(c.id) {
                    ui.painter().set(bg, egui::Shape::rect_filled(block, 0.0, fig::SELECTED()));
                }
                if ui.rect_contains_pointer(block) {
                    let b = |slot: f32| Rect::from_min_size(Pos2::new(block.right() - 32.0 - 28.0 * slot, block_top + 6.0), Vec2::splat(24.0));
                    if fig::icon_button(ui, b(0.0), Glyph::Delete, "Delete comment", false).clicked() {
                        delete = Some(c.id);
                    }
                    let tip = if c.resolved { "Resolved: click to reopen" } else { "Mark as resolved" };
                    if fig::icon_button(ui, b(1.0), Glyph::CircleCheck, tip, c.resolved).clicked() {
                        c.resolved ^= true;
                    }
                }
                fig::rule(ui);
            }
            if shown == 0 {
                fig::row(ui, 12.0);
                fig::note(ui, Glyph::MessageSquare, "Press C, then click the canvas to leave a comment.");
            }
        });
        if let Some(id) = delete {
            self.comments.retain(|c| c.id != id);
        }
        if let Some(id) = goto {
            self.comment_sel = Some(id);
            if let Some(c) = self.comments.iter().find(|c| c.id == id) {
                self.pan = self.view.size() / 2.0 - c.pos.to_vec2() * self.zoom;
            }
        }
    }

    // the minimised left panel: a floating 40px bar over the canvas's top-left corner with the file name and
    // the button that brings the panel back
    pub fn left_pill(&mut self, ctx: &egui::Context, canvas: Rect) {
        let title = self.file.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let inset = if self.rulers { 18.0 + 8.0 } else { 8.0 };
        egui::Area::new(egui::Id::new("left-pill")).fixed_pos(canvas.min + Vec2::splat(inset)).order(egui::Order::Foreground).show(ctx, |ui| {
            let galley = ui.painter().layout_no_wrap(title, fig::semi(13.0), TEXT());
            let (r, _) = ui.allocate_exact_size(Vec2::new(16.0 + galley.size().x + 12.0 + 24.0 + 8.0, 40.0), Sense::hover());
            ui.painter().add(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(80) }.as_shape(r, 13.0));
            ui.painter().rect(r, 13.0, BG(), Stroke::new(1.0, fig::RULE()), StrokeKind::Inside);
            fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, galley.text(), fig::semi(13.0), TEXT());
            let b = Rect::from_min_size(Pos2::new(r.right() - 32.0, r.top() + 8.0), Vec2::splat(24.0));
            if fig::icon_button(ui, b, Glyph::PanelLeft, "Expand left panel (Shift+\\)", false).clicked() {
                self.left_min = false;
            }
        });
    }

    // ---- right panel

    // Figma's right panel: a 48px bar (people, Present, Share), 32px of tabs with the zoom menu, a divider,
    // then the tab's sections
    pub fn right_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let top = fig::row(ui, 48.0);
        let share = Rect::from_min_size(Pos2::new(top.right(), top.top() + 8.0), Vec2::new(0.0, 32.0)); // no Share button: Present sits at the edge
        let target = self.present_target();
        let pr = Rect::from_min_size(Pos2::new(share.left() - 8.0 - 44.0, share.top()), Vec2::new(44.0, 32.0));
        let present = ui.interact(pr, ui.id().with("present"), if target.is_some() { Sense::click() } else { Sense::hover() });
        if present.hovered() && target.is_some() {
            ui.painter().rect_filled(pr, fig::RADIUS, fig::INPUT());
        }
        let ink = if target.is_some() { TEXT() } else { TEXT_TERTIARY() };
        paint_glyph_sized(ui.painter(), Pos2::new(pr.left() + 16.0, pr.center().y), Glyph::Present, ink, 19.0);
        paint_glyph_sized(ui.painter(), Pos2::new(pr.left() + 36.0, pr.center().y), Glyph::ChevronDown, ink, 9.5);
        present.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, target.is_some(), "Present"));
        if present.clicked() {
            if let Some(f) = target {
                self.start_present(f);
            }
        }
        let people = Rect::from_min_max(Pos2::new(top.left() + 16.0, top.top() + 13.0), Pos2::new(pr.left() - 8.0, top.bottom() - 13.0));
        // a child that takes no room in the panel's layout
        let mut people_ui = ui.new_child(egui::UiBuilder::new().max_rect(people).layout(egui::Layout::left_to_right(egui::Align::Center)));

        // tabs
        let tabs = fig::row(ui, 32.0);
        let mut x = tabs.left() + 8.0;
        let mut tab = |ui: &mut egui::Ui, label: &str, on: bool| {
            let (resp, w) = fig::tab(ui, Pos2::new(x, tabs.top()), label, on);
            x += w + 6.0;
            resp.clicked()
        };
        if self.mode == Mode::Dev {
            tab(ui, "Inspect", true);
        } else {
            // Motion mode trades the Design tab for Motion, like Figma
            if (self.tab == Tab::Motion) != (self.mode == Mode::Motion) && self.tab != Tab::Prototype {
                self.tab = if self.mode == Mode::Motion { Tab::Motion } else { Tab::Design };
            }
            let (first, first_tab) = if self.mode == Mode::Motion { ("Motion", Tab::Motion) } else { ("Design", Tab::Design) };
            if tab(ui, first, self.tab == first_tab) {
                self.tab = first_tab;
            }
            if tab(ui, "Prototype", self.tab == Tab::Prototype) {
                self.tab = Tab::Prototype;
            }
        }
        let zoom = format!("{:.0}%", self.zoom * 100.0);
        let zr = Rect::from_min_max(Pos2::new(tabs.right() - 76.0, tabs.top()), Pos2::new(tabs.right() - 8.0, tabs.top() + 24.0));
        let zresp = ui.interact(zr, ui.id().with("zoom-menu"), Sense::click());
        // Figma: the button takes the selected look (#394360, blue text) while its menu is open
        let open_id = ui.id().with("zoom-menu-open");
        let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
        if open {
            ui.painter().rect_filled(zr, fig::RADIUS, fig::SELECTED());
        } else if zresp.hovered() {
            ui.painter().rect_filled(zr, fig::RADIUS, fig::INPUT());
        }
        let ink = if open { fig::SELECTED_INK() } else { TEXT() };
        fig::text(ui, Pos2::new(tabs.right() - 28.7, zr.center().y), Align2::RIGHT_CENTER, &zoom, fig::reg(11.0), ink);
        paint_glyph_sized(ui.painter(), Pos2::new(tabs.right() - 20.0, zr.center().y), Glyph::ChevronDown, ink, 9.5);
        zresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, zoom.clone()));
        if zresp.clicked() && !open {
            self.zoom_fresh = true;
        }
        let shown = egui::Popup::menu(&zresp).show(|ui| {
            self.zoom_items(ui);
        });
        ui.data_mut(|d| d.insert_temp(open_id, shown.is_some()));
        fig::rule(ui);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.mode == Mode::Dev {
                self.inspect_panel(ui);
                return;
            }
            match self.tab {
                Tab::Design => {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    self.design_tab(ui);
                }
                Tab::Prototype => {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                    self.proto_tab(ui);
                }
                Tab::Motion => {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                    self.motion_tab(ui);
                }
            }
        });
    }



    fn export_section(&mut self, ui: &mut egui::Ui, act: &mut Option<Act>) {
        if let Some(i) = self.single() {
            return self.export_settings(ui, i);
        }
        let g = fig::Grid::of(ui.max_rect());
        fig::header(ui, "Export", false, &[]);
        let r = fig::controls(ui, 4.0);
        fig::dropdown(ui, g.col1(r), None, &mut self.export_fmt, &ExportFormat::ALL, "Export format");
        if self.export_fmt.has_scale() {
            fig::dropdown(ui, g.col2(r), None, &mut self.export_scale, &[(0.5, "0.5x"), (1.0, "1x"), (2.0, "2x"), (3.0, "3x"), (4.0, "4x")], "Export scale");
        }
        let r = fig::controls(ui, 8.0);
        if fig::button(ui, g.full(r), if self.sel.is_empty() { "Export page" } else { "Export selection" }).clicked() {
            *act = Some(Act::Export);
        }
        fig::end(ui);
    }

    // a layer's saved export settings, like Figma's Export section: scale, suffix, format, remove; then the button
    pub(crate) fn export_settings(&mut self, ui: &mut egui::Ui, i: usize) {
        let g = fig::Grid::of(ui.max_rect());
        let has = !self.shapes[i].exports.is_empty();
        let (head, b) = fig::header(ui, "Export", !has, &[(Glyph::Plus, "Add export settings", false)]);
        if b[0].clicked() || (!has && head.clicked()) {
            // like Figma: the first is 1x, then 2x, 3x ... with a matching suffix
            let n = self.shapes[i].exports.len();
            let suffix = if n == 0 { String::new() } else { format!("@{}x", n + 1) };
            self.shapes[i].exports.push(ExportSetting { format: ExportFormat::Png, scale: (n + 1) as f32, suffix, ..ExportSetting::default() });
        }
        let mut remove = None;
        for k in 0..self.shapes[i].exports.len() {
            // Figma 126.9.13 (measured 2026-10-18): scale 75px, file type 75px, "advanced export settings" 24px and remove 24px,
            // 8, 8 and 4px apart, from the panel's 16px padding
            let r = fig::controls(ui, if k == 0 { 4.0 } else { 8.0 });
            let at = |x: f32, w: f32| Rect::from_min_size(Pos2::new(g.left + x, r.top()), Vec2::new(w, fig::ROW));
            let (scale_r, type_r, adv_r, rm_r) = (at(0.0, 75.0), at(82.0, 75.0), at(164.7, 24.0), at(192.7, 24.0));
            let set = &mut self.shapes[i].exports[k];
            if set.format.has_scale() {
                let mut cur = (set.dim, set.scale);
                if fig::dropdown(ui, scale_r, None, &mut cur, &[((0, 0.5), "0.5x"), ((0, 0.75), "0.75x"), ((0, 1.0), "1x"), ((0, 1.5), "1.5x"), ((0, 2.0), "2x"), ((0, 3.0), "3x"), ((0, 4.0), "4x"), ((1, 512.0), "512w"), ((2, 512.0), "512h")], &format!("Export {} scale", k + 1)) {
                    (set.dim, set.scale) = cur;
                }
            } else {
                // only a PNG has a scale: Figma shows the field dimmed
                fig::input_bg(ui, scale_r, false);
                fig::text(ui, Pos2::new(scale_r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, "1x", fig::reg(11.0), TEXT_TERTIARY());
                paint_glyph_sized(ui.painter(), Pos2::new(scale_r.right() - 12.0, r.center().y), Glyph::ChevronDown, TEXT_TERTIARY(), 9.5);
            }
            fig::dropdown(ui, type_r, None, &mut set.format, &ExportFormat::ALL, &format!("Export {} format", k + 1));
            let open_id = ui.id().with(("export-adv-open", k));
            let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
            let adv = fig::icon_button(ui, adv_r, Glyph::More, "Advanced export settings", open);
            let shown = crate::picker::popup(&adv, ui.max_rect().left(), |ui| export_popover(ui, set));
            ui.data_mut(|d| d.insert_temp(open_id, shown.is_some()));
            if fig::icon_button(ui, rm_r, Glyph::Minus, "Remove export setting", false).clicked() {
                remove = Some(k);
            }
        }
        if let Some(k) = remove {
            self.shapes[i].exports.remove(k);
        }
        if has {
            let r = fig::controls(ui, 8.0);
            let n = self.shapes[i].exports.len();
            let label = if n == 1 { format!("Export {}", self.shapes[i].name) } else { format!("Export {} ({} files)", self.shapes[i].name, n) };
            if fig::button(ui, g.full(r), &label).clicked() {
                self.export_saved(&ui.ctx().clone());
            }
            fig::end(ui);
        } else {
            fig::rule(ui);
        }
    }

    fn design_tab(&mut self, ui: &mut egui::Ui) {
        let mut act: Option<Act> = None;
        if self.tool == Tool::Draw(Kind::Frame) && !self.section_tool {
            // like Figma, the Frame tool swaps the panel for the device presets
            self.frame_presets_panel(ui);
        } else if self.sel.is_empty() {
            self.page_props(ui, &mut act);
            self.export_section(ui, &mut act);
        } else if let Some(i) = self.single() {
            self.single_props(ui, i, &mut act);
        } else {
            self.multi_props(ui, &mut act);
            self.selection_colors_section(ui);
            self.export_section(ui, &mut act);
        }
        let roots = self.sel_roots();
        match act {
            None => {}
            Some(Act::Group) => self.group(),
            Some(Act::Flatten) => self.flatten(),
            Some(Act::Mask) => self.toggle_mask(),
            Some(Act::Bool(op)) => self.boolean(op),
            Some(Act::Component) => self.make_component(),
            Some(Act::Detach) => self.detach_instances(),
            Some(Act::PushOverrides) => self.push_overrides(),
            Some(Act::RestoreMain) => self.restore_main(),
            Some(Act::Swap(m)) => {
                if let Some(&id) = roots.first() {
                    self.swap_instance(id, m);
                }
            }
            Some(Act::GoMain(m)) => {
                self.sel = vec![m];
                self.zoom_fit(true);
            }
            Some(Act::Instance(m)) => self.instantiate_next_to(m),
            Some(Act::Align(op)) => self.align(op),
            Some(Act::Distribute(h)) => self.distribute(h),
            Some(Act::Flip(h)) => self.flip(h),
            Some(Act::AutoLayout) => self.toggle_auto_layout(),
            Some(Act::ResizeToFit) => self.resize_to_fit(),
            Some(Act::AutoLayoutGrid) => {
                self.toggle_auto_layout();
                if let Some(l) = self.single().and_then(|i| self.shapes[i].layout.as_mut()) {
                    l.columns = 2;
                }
            }
            Some(Act::Export) => self.export(&ui.ctx().clone()),
            Some(Act::NewStyle(c)) => {
                let id = self.new_id();
                self.styles.push(ColorStyle { id, name: format!("Style {}", self.styles.len() + 1), color: c, description: String::new() });
            }
        }
    }

    // the Prototype tab, like Figma's: Interactions (trigger, destination, animation, easing), the connections
    // in the page, the Present button, and Figma's notes on making and running a prototype
    fn proto_tab(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let g = fig::Grid::of(ui.max_rect());
        let frames: Vec<(u64, String)> = self.shapes.iter().filter(|s| s.kind == Kind::Frame).map(|s| (s.id, s.name.clone())).collect();
        let single = self.single();

        // Prototype settings replace the tab's content (Figma's "Show prototype settings")
        if self.proto_settings {
            let (_, b) = fig::header(ui, "Prototype settings", false, &[(Glyph::Close, "Hide prototype settings", false)]);
            if b[0].clicked() {
                self.proto_settings = false;
            }
            let pg = &mut self.pages[self.cur];
            let r = fig::controls(ui, 0.0);
            fig::dropdown_outlined(ui, g.full(r), None, &mut pg.proto_device, &Device::ALL, "Device");
            let r = fig::controls(ui, 8.0);
            let scales = [(0u8, "Content scaling: fit"), (1, "Content scaling: fit width"), (2, "Content scaling: fill screen"), (3, "Content scaling: 100%")];
            fig::dropdown_outlined(ui, g.full(r), None, &mut pg.proto_scale, &scales, "Content scaling");
            let r = fig::controls(ui, 8.0);
            fig::input_bg(ui, g.full(r), false);
            fig::swatch(ui, Pos2::new(g.left + 12.0, r.center().y), &mut pg.proto_bg, "Background color");
            let id = ui.id().with("proto-bg-hex");
            fig::hex_text(ui, Pos2::new(g.left + 28.0, r.center().y), g.edge - g.left - 36.0, &mut pg.proto_bg, id);
            fig::end(ui);
            self.proto_notes(ui);
            return;
        }

        // ---- Flow starting point: a top-level frame that Present can start from
        if let Some(i) = single.filter(|&i| self.shapes[i].kind == Kind::Frame && self.shapes[i].parent.is_none()) {
            let has = self.shapes[i].flow.is_some();
            let actions: &[(Glyph, &str, bool)] = if has { &[(Glyph::Minus, "Remove flow starting point", false)] } else { &[(Glyph::Plus, "Add flow starting point", false)] };
            let (_, b) = fig::header(ui, "Flow starting point", !has, actions);
            if b.first().is_some_and(|b| b.clicked()) {
                let n = self.shapes.iter().filter(|s| s.flow.is_some()).count() + 1;
                self.shapes[i].flow = if has { None } else { Some(format!("Flow {n}")) };
            }
            if let Some(name) = self.shapes[i].flow.as_mut() {
                let r = fig::controls(ui, 2.0);
                fig::text_input(ui, g.full(r), name, "Flow name", "Flow name");
                fig::end(ui);
            } else {
                fig::rule(ui);
            }
        } else {
            fig::header(ui, "Flow starting point", true, &[]);
            fig::rule(ui);
        }

        // ---- Interactions
        let link = single.and_then(|i| self.shapes[i].link);
        let can_add = single.is_some_and(|i| frames.iter().any(|f| f.0 != self.shapes[i].id));
        let actions: &[(Glyph, &str, bool)] = if can_add { &[(Glyph::Plus, "Add interaction", false)] } else { &[] };
        let (_, b) = fig::header(ui, "Interactions", link.is_none(), actions);
        if let (Some(i), true) = (single, b.first().is_some_and(|b| b.clicked())) {
            let me = self.shapes[i].id;
            let first = frames.iter().find(|f| f.0 != me).map(|f| f.0);
            if link.is_none() {
                self.shapes[i].link = first;
            } else {
                self.shapes[i].more.push(Interaction { link: first, ..Interaction::default() });
            }
        }
        match (single, link) {
            (Some(i), Some(_)) => {
                let me = self.shapes[i].id;
                let layers: Vec<(u64, String)> = self.shapes.iter().filter(|s| s.parent.is_some() && s.id != me).map(|s| (s.id, s.name.clone())).collect();
                let vars: Vec<(Option<u64>, String)> = self.variables.iter().flat_map(|c| c.vars.iter().map(move |v| (Some(v.id), format!("{} / {}", c.name, v.name)))).collect();
                // every variant of every component set, for the Change to action
                let variants: Vec<(u64, String)> = (0..self.shapes.len())
                    .filter(|&k| self.shapes[k].component_set)
                    .flat_map(|k| self.variants_of(k).into_iter().map(move |v| (k, v)))
                    .map(|(k, v)| (self.shapes[v].id, format!("{} / {}", self.shapes[k].name, self.shapes[v].name)))
                    .collect();
                let n = 1 + self.shapes[i].more.len();
                let mut remove: Option<usize> = None;
                for idx in 0..n {
                    let mut ix = if idx == 0 { self.shapes[i].primary_interaction() } else { self.shapes[i].more[idx - 1].clone() };
                    let colls: Vec<(u64, String, Vec<(u64, String)>)> = self.variables.iter().map(|c| (c.id, c.name.clone(), c.modes.iter().map(|m| (m.id, m.name.clone())).collect())).collect();
                    ui.data_mut(|d| d.insert_temp(egui::Id::new("ix-collections"), colls));
                    if interaction_editor(ui, &g, &mut ix, &frames, &layers, &variants, &vars, me, idx) {
                        remove = Some(idx);
                    }
                    if idx == 0 {
                        self.shapes[i].set_primary_interaction(ix);
                    } else {
                        self.shapes[i].more[idx - 1] = ix;
                    }
                }
                if let Some(idx) = remove {
                    let s = &mut self.shapes[i];
                    if idx > 0 {
                        s.more.remove(idx - 1);
                    } else if !s.more.is_empty() {
                        let next = s.more.remove(0);
                        s.set_primary_interaction(next);
                    } else {
                        s.link = None;
                        s.set_primary_interaction(Interaction { link: None, ..Interaction::default() });
                    }
                }
                fig::end(ui);
            }
            (None, _) => {
                fig::note(ui, Glyph::Link, "Select a layer to add an interaction.");
                fig::row(ui, 8.0);
                fig::rule(ui);
            }
            _ => fig::rule(ui),
        }

        // ---- Connections in the page
        let links: Vec<(u64, Kind, String, String)> = self
            .shapes
            .iter()
            .filter_map(|s| {
                let to = s.link.and_then(|l| frames.iter().find(|f| f.0 == l))?;
                Some((s.id, s.kind, s.name.clone(), to.1.clone()))
            })
            .collect();
        // Figma has no such section: it appears only once the page has connections
        if !links.is_empty() {
            fig::header(ui, "Connections", false, &[]);
        }
        for (k, (id, kind, name, to)) in links.iter().enumerate() {
            let r = fig::controls(ui, if k == 0 { 2.0 } else { 8.0 });
            let f = Rect::from_min_max(Pos2::new(g.left, r.top()), Pos2::new(g.edge, r.bottom()));
            let resp = ui.interact(f, ui.id().with(("connection", *id)), Sense::click());
            fig::input_bg(ui, f, resp.hovered());
            paint_glyph_sized(ui.painter(), Pos2::new(f.left() + 12.0, r.center().y), kind_glyph(*kind), TEXT_SECONDARY(), 13.0);
            fig::ptext(&ui.painter().with_clip_rect(f.shrink(4.0)), Pos2::new(f.left() + 24.0, r.center().y), Align2::LEFT_CENTER, format!("{name}  →  {to}"), fig::reg(11.0), TEXT());
            let label = format!("{name} to {to}");
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
            if resp.clicked() {
                self.sel = vec![*id];
            }
        }
        if !links.is_empty() {
            fig::end(ui);
        }

        // ---- Scroll behavior (frames)
        if let Some(i) = single.filter(|&i| self.shapes[i].kind == Kind::Frame) {
            let r = fig::row(ui, 40.0);
            fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.top() + 20.0), Align2::LEFT_CENTER, "Scroll behavior", fig::semi(11.0), TEXT());
            let r = fig::controls(ui, 0.0);
            fig::ptext(ui.painter(), Pos2::new(g.left, r.center().y), Align2::LEFT_CENTER, "Overflow", fig::reg(11.0), TEXT_SECONDARY());
            let dd = Rect::from_min_max(Pos2::new(g.left + 67.0, r.top()), Pos2::new(g.edge, r.bottom()));
            fig::dropdown_outlined(ui, dd, None, &mut self.shapes[i].overflow, &Overflow::ALL, "Overflow");
            fig::end(ui);
        }
        // a layer in a scrolling frame can stay where it is while the rest scrolls
        if let Some(i) = single.filter(|&i| self.shapes[i].parent.and_then(|p| self.index_of(p)).is_some_and(|p| self.shapes[p].kind == Kind::Frame && self.shapes[p].overflow != Overflow::None)) {
            let r = fig::row(ui, 40.0);
            fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.top() + 20.0), Align2::LEFT_CENTER, "Scroll behavior", fig::semi(11.0), TEXT());
            let r = fig::controls(ui, 0.0);
            fig::checkbox(ui, g.both(r), &mut self.shapes[i].fixed, "Fixed position when scrolling");
            fig::end(ui);
        }

        // ---- Present
        let target = self.present_target();
        if fig::outline_button(ui, 16.0, "Present").clicked() {
            if let Some(f) = target {
                self.start_present(f);
            }
        }
        fig::row(ui, 8.0);
        if fig::outline_button(ui, 0.0, "Open in browser").clicked() {
            let ctx = ui.ctx().clone();
            self.open_prototype_in_browser(&ctx);
        }
        fig::row(ui, 8.0);
        if fig::outline_button(ui, 0.0, "Export as web page").clicked() {
            let ctx = ui.ctx().clone();
            self.export_prototype_html(&ctx);
        }
        fig::row(ui, 8.0);
        if fig::outline_button(ui, 0.0, "Show prototype settings").clicked() {
            self.proto_settings = true;
        }
        fig::row(ui, 16.0);
        fig::rule(ui);
        self.proto_notes(ui);
    }

    // Figma's two dismissable notes at the bottom of the Prototype tab
    fn proto_notes(&mut self, ui: &mut egui::Ui) {
        for (title, g, text) in [
            ("Creating a connection", Glyph::Link, "Select a layer in a frame, add an interaction and pick the frame it navigates to."),
            ("Running your prototype", Glyph::Present, "Use the play button at the top of this panel to play your prototype. Without connections it presents your frames."),
        ] {
            // each note has a close button and stays closed once dismissed
            let id = egui::Id::new(("dismissed-note", title));
            if ui.data_mut(|d| d.get_persisted::<bool>(id)).unwrap_or(false) {
                continue;
            }
            let (_, b) = fig::header(ui, title, false, &[(Glyph::Close, "Dismiss", false)]);
            if b[0].clicked() {
                ui.data_mut(|d| d.insert_persisted(id, true));
                continue;
            }
            fig::note(ui, g, text);
            fig::row(ui, 16.0);
        }
    }

    // ---- HUD: toolbar, zoom, hints, toasts, help

    // the root frame to present: the one holding the selection, else the first
    pub(crate) fn present_target(&self) -> Option<u64> {
        let held = self
            .sel
            .first()
            .and_then(|&id| self.index_of(id))
            .and_then(|i| self.chain(i).last().copied())
            .map(|k| self.shapes[k].id)
            .filter(|&id| self.index_of(id).is_some_and(|i| self.shapes[i].kind == Kind::Frame));
        held.or_else(|| self.shapes.iter().find(|s| s.flow.is_some()).map(|s| s.id)).or_else(|| self.shapes.iter().find(|s| s.kind == Kind::Frame && s.parent.is_none()).map(|s| s.id))
    }

    // what the active tool or mode expects from the user next
    fn hint(&self) -> Option<&'static str> {
        if self.editing_text.is_some() {
            return Some("Type to edit  ·  Esc or click outside to finish");
        }
        if self.node_edit {
            return Some("Drag nodes and handles  ·  double-click a node to smooth it  ·  double-click a segment to add a node  ·  Del removes  ·  Esc exits");
        }
        match self.tool {
            Tool::Select if self.mode == Mode::Dev => Some("Dev Mode: select to inspect  ·  hover another layer for distances  ·  Shift+D to leave"),
            Tool::Select => None,
            Tool::Hand => Some("Drag to pan  ·  V to go back to Move"),
            Tool::Pen if self.pen.is_empty() => Some("Click to start a path  ·  drag to pull out a curve"),
            Tool::Pen => Some("Click to add points  ·  click the first point to close  ·  Enter or double-click to finish"),
            Tool::Draw(Kind::Text) => Some("Click to type  ·  drag for a fixed-width text box"),
            Tool::Draw(Kind::Line) => Some("Drag to draw a line  ·  hold Shift to snap the angle"),
            Tool::Draw(_) => Some("Drag to draw  ·  hold Shift to keep it square"),
            Tool::Pencil => Some("Drag to draw freehand  ·  V to stop"),
            Tool::Comment => Some("Click anywhere to leave a comment"),
        }
    }

    fn mode_glyph(m: Mode) -> Glyph {
        match m {
            Mode::Design => Glyph::Cursor,
            Mode::Draw => Glyph::Pencil,
            Mode::Motion => Glyph::Keyframe,
            Mode::Dev => Glyph::Code,
        }
    }

    // Figma's toolbelt modes: Draw, Design, Motion and Dev Mode as one strip of icons at the right end of the
    // toolbar, the active one dark with a blue icon
    fn mode_strip(&mut self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(4.0 * 30.0 + 4.0, 36.0), Sense::hover());
        ui.painter().rect_filled(rect, 8, fig::INPUT());
        for (k, (m, label, key)) in [Mode::Draw, Mode::Design, Mode::Motion, Mode::Dev].into_iter().filter_map(|m| Mode::ALL.iter().find(|a| a.0 == m).copied()).enumerate() {
            let b = Rect::from_min_size(rect.min + Vec2::new(2.0 + 30.0 * k as f32, 2.0), Vec2::new(30.0, 32.0));
            let resp = ui.interact(b, ui.id().with(("mode", label)), Sense::click());
            let on = self.mode == m;
            if on {
                ui.painter().rect_filled(b, 6, crate::theme::SUNKEN());
            } else if resp.hovered() {
                ui.painter().rect_filled(b, 6, crate::theme::FIELD_HOVER());
            }
            paint_glyph_sized(ui.painter(), b.center(), Self::mode_glyph(m), if on { fig::SELECTED_INK() } else { TEXT() }, 16.0);
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, label));
            let tip = if key.is_empty() { label.to_owned() } else { format!("{label} ({key})") };
            if resp.tip(tip).clicked() {
                self.set_mode(m);
            }
        }
    }

    // Draw toolbar: stroke width (also applied to a selected pencil stroke) and the pressure switch
    fn brush_controls(&mut self, ui: &mut egui::Ui) {
        const WIDTHS: [f32; 7] = [1.0, 2.0, 4.0, 8.0, 12.0, 24.0, 48.0];
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(46.0, 34.0), Sense::click());
        ui.painter().rect_filled(rect, R_MEDIUM as f32, if resp.hovered() { BG_SECONDARY() } else { Color32::TRANSPARENT });
        let dot = |p: &egui::Painter, c: Pos2, w: f32| p.circle_filled(c, (w / 2.0).clamp(1.0, 7.0), TEXT());
        dot(ui.painter(), rect.left_center() + Vec2::new(12.0, 0.0), self.brush_w);
        fig::ptext(ui.painter(), rect.left_center() + Vec2::new(22.0, 0.0), Align2::LEFT_CENTER, format!("{}", self.brush_w), FontId::proportional(11.0), TEXT());
        paint_glyph_sized(ui.painter(), rect.right_center() - Vec2::new(10.0, 0.0), Glyph::ChevronDown, TEXT(), 10.0);
        let label = format!("Stroke width: {}", self.brush_w);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
        let resp = resp.tip("Stroke width");
        let mut pick = None;
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(150.0);
            ui.set_max_width(200.0);
            for w in WIDTHS {
                if menu_item(ui, None, &format!("{w} px"), "", w != self.brush_w) {
                    pick = Some(w);
                    ui.close();
                }
            }
        });
        if let Some(w) = pick {
            self.brush_w = w;
            // restyle the selected stroke too, like Figma's Draw toolbar
            for &id in &self.sel {
                if let Some(i) = self.index_of(id).filter(|&i| self.shapes[i].kind == Kind::Path && !self.shapes[i].strokes.is_empty()) {
                    self.shapes[i].stroke_w = w;
                }
            }
        }
        // the brush: a menu of presets, each with a sample stroke
        use crate::vector::Brush;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(96.0, 34.0), Sense::click());
        ui.painter().rect_filled(rect, R_MEDIUM as f32, if resp.hovered() { BG_SECONDARY() } else { Color32::TRANSPARENT });
        brush_sample(ui.painter(), Rect::from_min_size(rect.min + Vec2::new(6.0, 9.0), Vec2::new(26.0, 16.0)), self.brush, self.nib);
        fig::ptext(ui.painter(), rect.left_center() + Vec2::new(36.0, 0.0), Align2::LEFT_CENTER, self.brush.label(), FontId::proportional(11.0), TEXT());
        paint_glyph_sized(ui.painter(), rect.right_center() - Vec2::new(10.0, 0.0), Glyph::ChevronDown, TEXT(), 10.0);
        let label = format!("Brush: {}", self.brush.label());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
        let resp = resp.tip("Brush");
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(200.0);
            ui.set_max_width(220.0);
            for (b, name) in Brush::ALL {
                let (r, item) = ui.allocate_exact_size(Vec2::new(200.0, 30.0), Sense::click());
                if item.hovered() || b == self.brush {
                    ui.painter().rect_filled(r, R_SMALL as f32 + 1.0, if item.hovered() { BRAND } else { BG_SECONDARY() });
                }
                brush_sample(ui.painter(), Rect::from_min_size(r.min + Vec2::new(8.0, 7.0), Vec2::new(60.0, 16.0)), b, self.nib);
                fig::ptext(ui.painter(), r.left_center() + Vec2::new(80.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(12.0), TEXT());
                item.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, b == self.brush, name));
                if item.clicked() {
                    self.brush = b;
                    ui.close();
                }
            }
            if self.brush == Brush::Stretch {
                ui.label("Picture");
                let items: Vec<(u64, String)> = self.images.iter().map(|a| (a.id, a.name.clone())).collect();
                if items.is_empty() {
                    ui.label("No pictures yet");
                }
                for (id, name) in items {
                    if crate::glyphs::menu_item_checked(ui, Some(Glyph::Image), self.stretch_image == Some(id), &name, "") {
                        self.stretch_image = Some(id);
                    }
                }
                if menu_item(ui, Some(Glyph::Plus), "Upload a picture…", "", true) {
                    if let Some(path) = rfd::FileDialog::new().add_filter("image", &["png", "jpg", "jpeg"]).pick_file() {
                        if let Ok(bytes) = std::fs::read(&path) {
                            let name = path.file_stem().map_or("brush".to_owned(), |n| n.to_string_lossy().into_owned());
                            self.stretch_image = self.register_image(&name, bytes);
                        }
                    }
                    ui.close();
                }
            }
            if self.brush == Brush::Calligraphy {
                ui.horizontal(|ui| {
                    ui.label("Nib angle");
                    crate::widgets::number(ui, &mut self.nib, "Nib angle", |x| egui::DragValue::new(x).range(0.0..=180.0).suffix("°"));
                });
            }
        });
    }

    // the toolbar after Move, outside Design mode
    fn mode_tools(&mut self, ui: &mut egui::Ui) {
        if self.mode == Mode::Draw {
            if tool_button(ui, self.tool == Tool::Pencil, Icon::Pencil, "Pencil (Shift+P)", "⇧P").clicked() {
                self.set_tool(Tool::Pencil);
            }
            if tool_button(ui, self.tool == Tool::Pen, Icon::Pen, "Pen (P)", "").clicked() {
                self.set_tool(Tool::Pen);
            }
            divider(ui);
            self.brush_controls(ui);
            divider(ui);
        }
        if tool_button(ui, self.tool == Tool::Comment, Icon::Comment, "Comment (C)", "C").clicked() {
            self.set_tool(Tool::Comment);
        }
    }

    // the small chevron beside a toolbar tool: a menu of the tools that share the button, the current one checked
    fn tool_caret(&mut self, ui: &mut egui::Ui, name: &str, items: &[(Tool, Glyph, &str, &str)]) {
        let (rect, caret) = ui.allocate_exact_size(Vec2::new(14.0, 34.0), Sense::click());
        if caret.hovered() {
            ui.painter().rect_filled(rect, R_SMALL as f32 + 1.0, BG_SECONDARY());
        }
        paint_glyph_sized(ui.painter(), rect.center(), Glyph::ChevronDown, TEXT(), 11.0);
        caret.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        egui::Popup::menu(&caret).show(|ui| {
            for (t, g, label, key) in items {
                if crate::glyphs::menu_item_checked(ui, Some(*g), self.tool == *t && !(*t == Tool::Select && (self.scale_tool || self.lasso_tool)), label, key) {
                    self.set_tool(*t);
                    ui.close();
                }
            }
            // the Move tools menu also holds Scale (K): handles scale children, strokes and text along
            if name == "Move tools" && crate::glyphs::menu_item_checked(ui, Some(Glyph::Ratio), self.tool == Tool::Select && self.scale_tool, "Scale", "K") {
                self.set_tool(Tool::Select);
                self.scale_tool = true;
                ui.close();
            }
            // the Creation tools menu also holds Bend: in vector editing, drag a vertex to pull curves out of it
            if name == "Creation tools" && crate::glyphs::menu_item_checked(ui, Some(Glyph::Waves), self.bend_tool, "Bend", "") {
                self.bend_tool = !self.bend_tool;
                if self.bend_tool && !self.node_edit {
                    self.status = "Bend: select a vector and press Enter to edit its points, then drag a vertex".into();
                }
                ui.close();
            }
            if name == "Creation tools" {
                for (t, key) in [(crate::vtools::VTool::Builder, "Shift+B"), (crate::vtools::VTool::Cut, "X"), (crate::vtools::VTool::Eraser, "Shift+E"), (crate::vtools::VTool::Paint, "")] {
                    if crate::glyphs::menu_item_checked(ui, Some(Glyph::Pencil), self.vtool == Some(t), t.label(), key) {
                        self.set_vtool(Some(t));
                        ui.close();
                    }
                }
            }
            if name == "Move tools" && crate::glyphs::menu_item_checked(ui, Some(Glyph::Cursor), self.tool == Tool::Select && self.lasso_tool, "Lasso select", "") {
                self.set_tool(Tool::Select);
                self.lasso_tool = true;
                ui.close();
            }
        });
    }

    pub fn toolbar(&mut self, ctx: &egui::Context) {
        const SHAPES: [(Kind, &str, &str); 5] = [
            (Kind::Rect, "Rectangle", "R"),
            (Kind::Line, "Line", "L"),
            (Kind::Ellipse, "Ellipse", "O"),
            (Kind::Polygon, "Polygon", ""),
            (Kind::Star, "Star", ""),
        ];
        let pill = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Frame::new()
                .fill(BG())
                .stroke(Stroke::new(1.0, BG_PRESSED()))
                .corner_radius(R_LARGE)
                .inner_margin(6)
                .shadow(egui::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(90) })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        add(ui);
                    })
                });
        };

        // above whatever sits under the canvas (the Motion timeline)
        let lift = (ctx.content_rect().max.y - self.view.max.y).max(0.0);
        egui::Area::new(egui::Id::new("toolbar")).anchor(Align2::CENTER_BOTTOM, [0.0, -16.0 - lift]).order(egui::Order::Foreground).show(ctx, |ui| {
            pill(ui, &mut |ui| {
                // the move tool's menu (Figma's "Move tools"): Move and Hand tool, each with its shortcut
                let move_on = matches!(self.tool, Tool::Select | Tool::Hand);
                let (move_icon, move_name) = if self.tool == Tool::Hand { (Icon::Hand, "Hand tool (H)") } else { (Icon::Move, "Move (V)") };
                if tool_button(ui, move_on, move_icon, move_name, "").clicked() {
                    self.set_tool(if self.tool == Tool::Hand { Tool::Hand } else { Tool::Select });
                }
                self.tool_caret(ui, "Move tools", &[(Tool::Select, Glyph::Move, "Move", "V"), (Tool::Hand, Glyph::Hand, "Hand tool", "H")]);
                if self.mode != Mode::Design {
                    self.mode_tools(ui);
                } else {
                let frame_on = self.tool == Tool::Draw(Kind::Frame) || (self.tool == Tool::Draw(Kind::Rect) && self.slice_tool);
                if tool_button(ui, frame_on, Icon::Shape(Kind::Frame), "Frame (F)", "").clicked() {
                    self.set_tool(Tool::Draw(Kind::Frame));
                }
                let (rect, caret) = ui.allocate_exact_size(Vec2::new(14.0, 34.0), Sense::click());
                if caret.hovered() {
                    ui.painter().rect_filled(rect, R_SMALL as f32 + 1.0, BG_SECONDARY());
                }
                paint_glyph_sized(ui.painter(), rect.center(), Glyph::ChevronDown, TEXT(), 11.0);
                caret.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Frame tools"));
                egui::Popup::menu(&caret).show(|ui| {
                    // Figma's Frame tools menu: Frame, Section, Slice
                    let on = self.tool == Tool::Draw(Kind::Frame) && !self.section_tool;
                    if crate::glyphs::menu_item_checked(ui, Some(Glyph::Frame), on, "Frame", "F or A") {
                        self.set_tool(Tool::Draw(Kind::Frame));
                        ui.close();
                    }
                    if crate::glyphs::menu_item_checked(ui, Some(Glyph::Section), self.tool == Tool::Draw(Kind::Frame) && self.section_tool, "Section", "Shift+S") {
                        self.set_tool(Tool::Draw(Kind::Frame));
                        self.section_tool = true;
                        ui.close();
                    }
                    if crate::glyphs::menu_item_checked(ui, Some(Glyph::Slice), self.tool == Tool::Draw(Kind::Rect) && self.slice_tool, "Slice", "S") {
                        self.set_tool(Tool::Draw(Kind::Rect));
                        self.slice_tool = true;
                        ui.close();
                    }
                });
                // shapes share one button that remembers the last one used, with a menu for the rest
                let on = matches!(self.tool, Tool::Draw(k) if SHAPES.iter().any(|s| s.0 == k));
                let (name, _) = SHAPES.iter().find(|s| s.0 == self.last_shape).map_or(("Rectangle", "R"), |s| (s.1, s.2));
                if tool_button(ui, on, Icon::Shape(self.last_shape), name, "").clicked() {
                    self.set_tool(Tool::Draw(self.last_shape));
                }
                let (rect, caret) = ui.allocate_exact_size(Vec2::new(14.0, 34.0), Sense::click());
                if caret.hovered() {
                    ui.painter().rect_filled(rect, R_SMALL as f32 + 1.0, BG_SECONDARY());
                }
                paint_glyph_sized(ui.painter(), rect.center(), Glyph::ChevronDown, TEXT(), 11.0);
                egui::Popup::menu(&caret).show(|ui| {
                    // Figma's Shape tools menu: check, icon, name, shortcut; Arrow is a line with an arrowhead
                    for (k, label, shortcut) in SHAPES {
                        let on = self.tool == Tool::Draw(k) && !(k == Kind::Line && self.arrow_tool);
                        if crate::glyphs::menu_item_checked(ui, Some(kind_glyph(k)), on, label, shortcut) {
                            self.set_tool(Tool::Draw(k));
                            ui.close();
                        }
                        if k == Kind::Line && crate::glyphs::menu_item_checked(ui, Some(Glyph::ArrowUp), self.tool == Tool::Draw(Kind::Line) && self.arrow_tool, "Arrow", "Shift+L") {
                            self.set_tool(Tool::Draw(Kind::Line));
                            self.arrow_tool = true;
                            ui.close();
                        }
                    }
                    ui.separator();
                    if menu_item(ui, Some(Glyph::ImagePlus), "Place image…", "", true) {
                        if let Some(p) = rfd::FileDialog::new().add_filter("image", &["png", "jpg", "jpeg"]).pick_file() {
                            if let Ok(bytes) = std::fs::read(&p) {
                                let name = p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                self.add_image(&name, bytes, None);
                            }
                        }
                        ui.close();
                    }
                });
                divider(ui);
                let pen_on = matches!(self.tool, Tool::Pen | Tool::Pencil);
                let (pen_icon, pen_name) = if self.tool == Tool::Pencil { (Icon::Pencil, "Pencil (Shift+P)") } else { (Icon::Pen, "Pen (P)") };
                if tool_button(ui, pen_on, pen_icon, pen_name, "").clicked() {
                    self.set_tool(if self.tool == Tool::Pencil { Tool::Pencil } else { Tool::Pen });
                }
                self.tool_caret(ui, "Creation tools", &[(Tool::Pen, Glyph::Pen, "Pen", "P"), (Tool::Pencil, Glyph::Pencil, "Pencil", "Shift+P")]);
                if tool_button(ui, self.tool == Tool::Draw(Kind::Text), Icon::Shape(Kind::Text), "Text (T)", "").clicked() {
                    self.set_tool(Tool::Draw(Kind::Text));
                }
                if tool_button(ui, self.tool == Tool::Comment, Icon::Comment, "Comment (C)", "").clicked() {
                    self.set_tool(Tool::Comment);
                }
                if tool_button(ui, self.actions_open, Icon::Glyph(Glyph::Grid), "Actions (Ctrl+/)", "").clicked() {
                    if self.actions_open {
                        self.actions_open = false;
                    } else {
                        self.open_actions();
                    }
                }
                }
                divider(ui);
                self.mode_strip(ui);
            });
        });

        // the help button, bottom right of the canvas
        egui::Area::new(egui::Id::new("help")).pivot(Align2::RIGHT_BOTTOM).fixed_pos(self.view.right_bottom() - Vec2::splat(16.0)).order(egui::Order::Foreground).show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
            let fill = if self.show_help || resp.hovered() { BG_SECONDARY() } else { BG() };
            ui.painter().circle(rect.center(), 16.0, fill, Stroke::new(1.0, BG_PRESSED()));
            paint_glyph_sized(ui.painter(), rect.center(), Glyph::Help, TEXT(), 16.0);
            if resp.tip("Keyboard shortcuts (?)").clicked() {
                self.show_help ^= true;
            }
        });

        // what to do next, for the active tool
        if let Some(h) = self.hint() {
            let ruler = if self.rulers { 18.0 } else { 0.0 };
            egui::Area::new(egui::Id::new("hint")).pivot(Align2::CENTER_TOP).fixed_pos(self.view.center_top() + Vec2::new(0.0, ruler + 10.0)).order(egui::Order::Foreground).interactable(false).show(ctx, |ui| {
                egui::Frame::new().fill(BG().gamma_multiply(0.92)).stroke(Stroke::new(1.0, BG_PRESSED())).corner_radius(R_LARGE).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
                    ui.label(RichText::new(h).color(TEXT_SECONDARY()).size(11.5));
                });
            });
        }

        // status messages fade out on their own
        let now = ctx.input(|i| i.time);
        if self.status != self.shown_status {
            self.shown_status = self.status.clone();
            self.status_t = now;
        }
        let age = now - self.status_t;
        if !self.shown_status.is_empty() && age < 4.0 {
            let a = ((4.0 - age) / 0.6).clamp(0.0, 1.0) as f32;
            egui::Area::new(egui::Id::new("toast")).anchor(Align2::CENTER_BOTTOM, [0.0, -76.0]).order(egui::Order::Tooltip).interactable(false).show(ctx, |ui| {
                egui::Frame::new().fill(BG().gamma_multiply(a)).stroke(Stroke::new(1.0, BG_PRESSED().gamma_multiply(a))).corner_radius(R_LARGE).inner_margin(egui::Margin::symmetric(14, 7)).show(ui, |ui| {
                    ui.label(RichText::new(&self.shown_status).color(TEXT().gamma_multiply(a)));
                });
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }

        let mut open = self.show_help;
        egui::Window::new("Keyboard shortcuts").open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            let rows: [(&str, &str); 35] = [
                ("V  F  R  O  L  T  P  C", "Move, Frame, Rectangle, Ellipse, Line, Text, Pen, Comment"),
                ("Shift+P", "Pencil (freehand)"),
                ("Shift+D", "Dev Mode: inspect, CSS, distances (read-only)"),
                ("Enter", "Edit text or path, dive into a group or frame"),
                ("Esc", "Cancel, leave a mode, deselect"),
                ("Del", "Delete"),
                ("Ctrl+C / V / D", "Copy, paste, duplicate"),
                ("Ctrl+A", "Select all"),
                ("Ctrl+Alt+A", "Select matching layers"),
                ("Ctrl+Z / Ctrl+Shift+Z", "Undo, redo"),
                ("Ctrl+G / Ctrl+Shift+G", "Group, ungroup"),
                ("Ctrl+E", "Flatten to a path"),
                ("Ctrl+Alt+K / B", "Create component, detach instance"),
                ("Ctrl+Alt+M", "Use as mask"),
                ("Ctrl+Alt+U S I X", "Union, subtract, intersect, exclude"),
                ("Shift+A", "Add auto layout (wraps anything that is not a plain frame)"),
                ("Ctrl+Alt+Shift+A", "Remove auto layout"),
                ("Ctrl+Shift+K", "Combine components as variants"),
                ("Shift+H / V", "Flip horizontally, vertically"),
                ("Arrows", "Nudge 1 px (Shift: 10 px)"),
                ("Shift+R", "Rulers"),
                ("Shift+0 / 1 / 2", "Zoom to 100%, fit, selection"),
                ("Ctrl +  /  Ctrl -", "Zoom in, out"),
                ("Scroll, middle-drag", "Pan"),
                ("Ctrl+scroll", "Zoom"),
                ("Shift+click", "Add to or remove from selection"),
                ("Shift+drag", "Constrain axis, proportions or angle"),
                ("Double-click", "Edit text or path, select inside a group"),
                ("Right-click", "Context menu"),
                ("Drag from a ruler", "Create a guide (drag it back to delete)"),
                ("Drag a layer row", "Reorder or move into a frame or group"),
                ("Drop an image file", "Place it on the canvas"),
                ("Pen: click-drag", "Pull out a smooth point"),
                ("Pen: Enter", "Finish an open path"),
                ("?", "This sheet"),
            ];
            egui::Grid::new("keys").num_columns(2).spacing([18.0, 5.0]).show(ui, |ui| {
                for (k, d) in rows {
                    ui.label(RichText::new(k).strong());
                    ui.label(RichText::new(d).color(TEXT_SECONDARY()));
                    ui.end_row();
                }
            });
        });
        self.show_help = open;
    }
}

// the address other machines on the LAN can reach a listener bound to `bound` at (0.0.0.0 is replaced by this machine's LAN IP)
pub(crate) fn lan_address(bound: &str) -> String {
    let port = bound.rsplit(':').next().unwrap_or("7878");
    // a connected UDP socket reveals the outgoing interface; nothing is sent
    let ip = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("192.168.0.1:9").and_then(|_| s.local_addr()))
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into());
    if bound.starts_with("0.0.0.0") { format!("{ip}:{port}") } else { bound.to_owned() }
}

// one interaction in the Prototype tab: its summary row, trigger and action, then the fields that action needs. True when removed.
#[allow(clippy::too_many_arguments)]
fn interaction_editor(ui: &mut egui::Ui, g: &fig::Grid, ix: &mut Interaction, frames: &[(u64, String)], layers: &[(u64, String)], variants: &[(u64, String)], vars: &[(Option<u64>, String)], me: u64, n: usize) -> bool {
    let mut removed = false;
    let r = fig::controls(ui, if n == 0 { 2.0 } else { 12.0 });
    let f = g.both(r);
    fig::input_bg(ui, f, false);
    paint_glyph_sized(ui.painter(), Pos2::new(f.left() + 12.0, r.center().y), Glyph::Link, TEXT_SECONDARY(), 13.0);
    let trig = Trigger::ALL.iter().find(|t| t.0 == ix.trigger).map_or("On click", |t| t.1);
    fig::text(ui, Pos2::new(f.left() + 24.0, r.center().y), Align2::LEFT_CENTER, trig, fig::reg(11.0), TEXT());
    let pool = match ix.action {
        Action::ScrollTo | Action::Video => layers,
        Action::ChangeTo => variants,
        _ => frames,
    };
    let to = ix.link.and_then(|l| pool.iter().find(|f| f.0 == l)).map_or(String::new(), |f| format!("→  {}", f.1));
    let act = Action::ALL.iter().find(|a| a.0 == ix.action).map_or("", |a| a.1);
    fig::ptext(&ui.painter().with_clip_rect(f), Pos2::new(f.left() + 96.0, r.center().y), Align2::LEFT_CENTER, if to.is_empty() { act.to_owned() } else { to }, fig::reg(11.0), TEXT_SECONDARY());
    if fig::icon_button(ui, g.icon(r), Glyph::Minus, "Remove interaction", false).clicked() {
        removed = true;
    }
    let salt = (me, n);
    fig::labels(ui, g, false, "Trigger", "Action");
    let r = fig::controls(ui, 0.0);
    fig::dropdown(ui, g.col1(r), None, &mut ix.trigger, &Trigger::ALL, &format!("Trigger {}", n + 1));
    fig::dropdown(ui, g.col2(r), None, &mut ix.action, &Action::ALL, &format!("Action {}", n + 1));
    // a condition: the interaction runs only when it is true, e.g. score >= 3 && !done
    fig::labels(ui, g, false, "Only if", "");
    let r = fig::controls(ui, 0.0);
    fig::text_input(ui, g.both(r), &mut ix.condition, "Interaction condition", "always (e.g. count > 2)");
    match ix.trigger {
        Trigger::AfterDelay => {
            fig::labels(ui, g, false, "Delay", "");
            let r = fig::controls(ui, 0.0);
            let mut ms = ix.delay * 1000.0;
            if fig::num_field(ui, g.col1(r), ui.id().with(("delay", salt)), fig::Prefix::Icon(Glyph::Clock), &mut ms, fig::Num { min: 0.0, max: 60000.0, decimals: 0, speed: 10.0, suffix: " ms" }, "Delay") {
                ix.delay = ms / 1000.0;
            }
        }
        Trigger::Drag => {
            fig::labels(ui, g, false, "Direction", "");
            let r = fig::controls(ui, 0.0);
            let items = [(0u8, "Any direction"), (1, "Left"), (2, "Right"), (3, "Up"), (4, "Down")];
            fig::dropdown(ui, g.both(r), None, &mut ix.opts.drag_dir, &items, "Drag direction");
        }
        Trigger::VideoTime => {
            fig::labels(ui, g, false, "Time", "");
            let r = fig::controls(ui, 0.0);
            fig::num_field(ui, g.col1(r), ui.id().with(("video-time", salt)), fig::Prefix::Icon(Glyph::Clock), &mut ix.delay, fig::Num { min: 0.0, max: 36000.0, decimals: 1, speed: 0.1, suffix: " s" }, "Video time");
        }
        Trigger::KeyPress => {
            fig::labels(ui, g, false, "Key", "");
            let r = fig::controls(ui, 0.0);
            fig::text_input(ui, g.both(r), &mut ix.key, "Key", "Enter, ArrowLeft, A");
        }
        _ => {}
    }
    match ix.action {
        Action::OpenLink => {
            fig::labels(ui, g, false, "URL", "");
            let r = fig::controls(ui, 0.0);
            fig::text_input(ui, g.both(r), &mut ix.url, "URL", "https://");
        }
        Action::SetMode => {
            let colls: Vec<(u64, String, Vec<(u64, String)>)> = ui.data(|d| d.get_temp(egui::Id::new("ix-collections"))).unwrap_or_default();
            fig::labels(ui, g, false, "Collection", "Mode");
            let r = fig::controls(ui, 0.0);
            if colls.is_empty() {
                fig::text(ui, Pos2::new(g.left + 8.0, r.center().y), Align2::LEFT_CENTER, "No variables yet", fig::reg(11.0), TEXT_TERTIARY());
            } else {
                let items: Vec<(u64, &str)> = colls.iter().map(|c| (c.0, c.1.as_str())).collect();
                if ix.opts.coll == 0 || !colls.iter().any(|c| c.0 == ix.opts.coll) {
                    ix.opts.coll = colls[0].0;
                }
                if fig::dropdown(ui, g.col1(r), None, &mut ix.opts.coll, &items, "Variable collection") {
                    ix.opts.mode = 0;
                }
                let modes: Vec<(u64, &str)> = colls.iter().find(|c| c.0 == ix.opts.coll).map(|c| c.2.iter().map(|m| (m.0, m.1.as_str())).collect()).unwrap_or_default();
                if !modes.iter().any(|m| m.0 == ix.opts.mode) {
                    ix.opts.mode = modes.first().map_or(0, |m| m.0);
                }
                fig::dropdown(ui, g.col2(r), None, &mut ix.opts.mode, &modes, "Variable mode");
            }
        }
        Action::Video => {
            fig::labels(ui, g, false, "Do", "");
            let r = fig::controls(ui, 0.0);
            fig::dropdown(ui, g.both(r), None, &mut ix.opts.video_op, &[(0u8, "Toggle play / pause"), (1, "Play"), (2, "Pause")], "Video action");
        }
        Action::SetVariable => {
            fig::labels(ui, g, false, "Variable", "Value");
            let r = fig::controls(ui, 0.0);
            if vars.is_empty() {
                fig::text(ui, Pos2::new(g.left + 8.0, r.center().y), Align2::LEFT_CENTER, "No variables yet", fig::reg(11.0), TEXT_TERTIARY());
            } else {
                let items: Vec<(Option<u64>, &str)> = vars.iter().map(|(id, name)| (*id, name.as_str())).collect();
                fig::dropdown(ui, g.col1(r), None, &mut ix.var, &items, "Variable");
                fig::text_input(ui, g.col2(r), &mut ix.value, "Variable value", "Value or count + 1");
            }
        }
        _ => {}
    }
    if ix.action.needs_target() {
        let (label, items): (&str, Vec<(Option<u64>, &str)>) = match ix.action {
            Action::ScrollTo => ("Scroll to", layers.iter().map(|f| (Some(f.0), f.1.as_str())).collect()),
            Action::Video => ("Video layer", layers.iter().map(|f| (Some(f.0), f.1.as_str())).collect()),
            Action::ChangeTo => ("Change to", variants.iter().map(|f| (Some(f.0), f.1.as_str())).collect()),
            _ => ("Destination", frames.iter().filter(|f| f.0 != me).map(|f| (Some(f.0), f.1.as_str())).collect()),
        };
        fig::labels(ui, g, false, label, "");
        let r = fig::controls(ui, 0.0);
        fig::dropdown(ui, g.both(r), Some(match ix.action {
            Action::ScrollTo | Action::Video => Glyph::Link,
            Action::ChangeTo => Glyph::Component,
            _ => Glyph::Frame,
        }), &mut ix.link, &items, label);
    }
    if matches!(ix.action, Action::OpenOverlay | Action::SwapOverlay) {
        fig::labels(ui, g, false, "Position", "");
        let r = fig::controls(ui, 0.0);
        let items = [(0u8, "Center"), (1, "Top left"), (2, "Top center"), (3, "Top right"), (4, "Bottom left"), (5, "Bottom center"), (6, "Bottom right"), (7, "Manual")];
        fig::dropdown(ui, g.both(r), None, &mut ix.overlay_pos, &items, "Overlay position");
        if ix.overlay_pos == 7 {
            // the overlay's top-left, measured from the top-left of the frame it opens over
            let r = fig::controls(ui, 8.0);
            let (mut x, mut y) = (ix.overlay_off.x, ix.overlay_off.y);
            let num = fig::Num { min: -10000.0, max: 10000.0, decimals: 0, speed: 1.0, suffix: "" };
            let cx = fig::num_field(ui, g.col1(r), ui.id().with(("overlay-x", salt)), fig::Prefix::Text("X"), &mut x, num, "Overlay X");
            let cy = fig::num_field(ui, g.col2(r), ui.id().with(("overlay-y", salt)), fig::Prefix::Text("Y"), &mut y, num, "Overlay Y");
            if cx || cy {
                ix.overlay_off = Vec2::new(x, y);
            }
        }
        let r = fig::controls(ui, 8.0);
        fig::checkbox(ui, g.col1(r), &mut ix.close_outside, "Close when clicking outside");
        let r = fig::controls(ui, 8.0);
        fig::checkbox(ui, g.col1(r), &mut ix.overlay_dim, "Add background behind overlay");
    }
    if matches!(ix.action, Action::Navigate | Action::Back) {
        let r = fig::controls(ui, 8.0);
        fig::checkbox(ui, g.col1(r), &mut ix.opts.keep_scroll, "Keep scroll position");
        let r = fig::controls(ui, 8.0);
        fig::checkbox(ui, g.col1(r), &mut ix.opts.reset_components, "Reset component state");
        let r = fig::controls(ui, 8.0);
        fig::checkbox(ui, g.col1(r), &mut ix.opts.reset_video, "Reset video state");
    }
    if ix.action.animates() {
        fig::labels(ui, g, false, "Animation", "Duration");
        let r = fig::controls(ui, 0.0);
        fig::dropdown(ui, g.col1(r), None, &mut ix.transition, &Transition::ALL, &format!("Animation {}", n + 1));
        let mut ms = ix.duration * 1000.0;
        if fig::num_field(ui, g.col2(r), ui.id().with(("duration", salt)), fig::Prefix::Icon(Glyph::Clock), &mut ms, fig::Num { min: 50.0, max: 5000.0, decimals: 0, speed: 5.0, suffix: " ms" }, "Duration") {
            ix.duration = ms / 1000.0;
        }
        if ix.transition != Transition::Instant {
            fig::labels(ui, g, false, "Easing", "");
            let r = fig::controls(ui, 0.0);
            fig::dropdown(ui, g.both(r), None, &mut ix.easing, &Easing::ALL, &format!("Easing {}", n + 1));
            ease_box(ui, g.icon(r), ix.easing);
            if ix.transition == Transition::SmartAnimate {
                fig::row(ui, 8.0);
                fig::note(ui, Glyph::Sparkles, "Layers with the same name in both frames animate between them.");
            }
        }
    }
    removed
}
