// The Stroke and Effects sections of the Design tab beyond the paint rows, as Figma 126.9.13 has them (measured, CSS px):
// - Stroke: under the paints a row of "Position" (Inside / Center / Outside, outlined, x 16) and "Weight" (x 100) with two
//   buttons at the right: advanced stroke settings (a popover) and individual strokes (a width per side).
// - Effects: each effect is one 24px row: a pill with its icon and name (blue while its popover is open), an eye and a minus. The
//   popover has the type dropdown in a 40px header, rows of 32px (label x 16, fields x 88..224) and, for a drop shadow, a
//   "Show behind transparent areas" checkbox under a divider. "+" opens a menu of the effect types.
use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Stroke, Vec2};

use crate::fig::{self, Grid, Num, Prefix};
use crate::glyphs::{menu_item, paint_glyph_sized, Glyph};
use crate::model::*;
use crate::picker;
use crate::{TEXT, TEXT_SECONDARY, TEXT_TERTIARY};

fn n(decimals: usize) -> Num {
    Num { decimals, ..Num::default() }
}

fn effect_glyph(k: EffectKind) -> Glyph {
    match k {
        EffectKind::DropShadow => Glyph::ShadowDrop,
        EffectKind::InnerShadow => Glyph::ShadowInner,
        EffectKind::LayerBlur => Glyph::BlurLayer,
        EffectKind::BackgroundBlur => Glyph::BlurBackground,
        EffectKind::Noise => Glyph::Sparkles,
        EffectKind::Texture => Glyph::Grid,
        EffectKind::Glass => Glyph::BlurBackground,
    }
}

// a label for a popover row: 11px secondary, 16px from the edge, centred on the row
pub(crate) fn row_label(ui: &egui::Ui, r: Rect, text: &str) {
    fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, text, fig::reg(11.0), TEXT_SECONDARY());
}

// the 40px header of a popover with `title` (or the content drawn by the caller) and the close button; true when closed
pub(crate) fn popover_header(ui: &mut egui::Ui, title: &str) -> bool {
    let r = fig::row(ui, 40.0);
    fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, title, fig::semi(11.0), TEXT());
    let close = fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 32.0, r.top() + 8.0), Vec2::splat(fig::ROW)), Glyph::Close, "Close", false).clicked();
    fig::rule(ui);
    close
}

// ---- stroke ----

#[derive(Clone, Copy, PartialEq)]
enum DashStyle {
    Solid,
    Dash,
    Custom,
}

// a corner as a join icon: the shape of the join between two strokes
fn join_icon(p: &egui::Painter, c: Pos2, join: StrokeJoin, ink: Color32) {
    let st = Stroke::new(1.5, ink);
    let (l, t, r) = (c.x - 6.0, c.y - 6.0, c.x + 6.0);
    let b = c.y + 6.0;
    let pts: Vec<Pos2> = match join {
        StrokeJoin::Miter => vec![Pos2::new(l, b), Pos2::new(l, t), Pos2::new(r, t)],
        StrokeJoin::Bevel => vec![Pos2::new(l, b), Pos2::new(l, t + 4.0), Pos2::new(l + 4.0, t), Pos2::new(r, t)],
        StrokeJoin::Round => {
            let mut v = vec![Pos2::new(l, b), Pos2::new(l, t + 6.0)];
            for k in 0..=6 {
                let a = std::f32::consts::FRAC_PI_2 * k as f32 / 6.0;
                v.push(Pos2::new(l + 6.0 - 6.0 * a.cos(), t + 6.0 - 6.0 * a.sin()));
            }
            v.push(Pos2::new(r, t));
            v
        }
    };
    p.add(egui::Shape::line(pts, st));
}

// the advanced stroke settings popover (Basic): style, join, miter angle, and for open paths the end caps
fn stroke_settings(ui: &mut egui::Ui, s: &mut Shape) {
    if popover_header(ui, "Stroke settings") {
        ui.close();
    }
    fig::row(ui, 12.0);
    // Figma 126.9.13 (REA + a screenshot of a frame's popover): Basic / Dynamic / Brush are offered for vector paths only; a shape goes
    // straight to Style and Join
    if matches!(s.kind, Kind::Path | Kind::Line) {
        let r = fig::row(ui, fig::ROW);
        let dynamic = s.dynamic.is_some();
        if let Some(k) = fig::text_segments(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top()), Vec2::new(208.0, fig::ROW)), &[("Basic", !dynamic, true), ("Dynamic", dynamic, true), ("Brush", false, false)]) {
            s.dynamic = if k == 1 { Some(s.dynamic.clone().unwrap_or_default()) } else { None };
        }
        fig::row(ui, 8.0);
        if let Some(dy) = &mut s.dynamic {
            let id = ui.id().with("dynamic-stroke");
            let field = |r: Rect| Rect::from_min_size(Pos2::new(r.left() + 96.0, r.top() + 4.0), Vec2::new(128.0, fig::ROW));
            for (k, label, v, max, suffix) in [(0, "Frequency", &mut dy.frequency, 100.0, ""), (1, "Wiggle", &mut dy.wiggle, 50.0, " px"), (2, "Smoothen", &mut dy.smoothen, 100.0, "%")] {
                let r = fig::row(ui, 32.0);
                row_label(ui, r, label);
                fig::num_field(ui, field(r), id.with(k), Prefix::None, v, Num { min: 0.0, max, decimals: 1, speed: 0.2, suffix }, label);
            }
        }
    }
    // style: solid, the default dash, or a dash and gap of your own
    let mut style = if s.dash_pattern.iter().any(|v| *v > 0.0) {
        DashStyle::Custom
    } else if s.dash {
        DashStyle::Dash
    } else {
        DashStyle::Solid
    };
    let r = fig::row(ui, 32.0);
    row_label(ui, r, "Style");
    let field = |r: Rect| Rect::from_min_size(Pos2::new(r.left() + 96.0, r.top() + 4.0), Vec2::new(128.0, fig::ROW));
    if fig::dropdown_outlined(ui, field(r), None, &mut style, &[(DashStyle::Solid, "Solid"), (DashStyle::Dash, "Dash"), (DashStyle::Custom, "Custom")], "Stroke style") {
        match style {
            DashStyle::Solid => {
                s.dash = false;
                s.dash_pattern.clear();
            }
            DashStyle::Dash => {
                s.dash = true;
                s.dash_pattern.clear();
            }
            DashStyle::Custom => {
                s.dash = true;
                s.dash_pattern = vec![(s.stroke_w * 4.0).max(1.0), (s.stroke_w * 3.0).max(1.0)];
            }
        }
    }
    if style == DashStyle::Custom {
        let mut dash = s.dash_pattern.first().copied().unwrap_or(4.0);
        let mut gap = s.dash_pattern.get(1).copied().unwrap_or(dash);
        let id = ui.id().with("dash-fields");
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Dash");
        let mut changed = fig::num_field(ui, field(r), id.with("d"), Prefix::None, &mut dash, Num { min: 0.1, max: 1000.0, decimals: 1, speed: 0.1, suffix: "" }, "Dash length");
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Gap");
        changed |= fig::num_field(ui, field(r), id.with("g"), Prefix::None, &mut gap, Num { min: 0.1, max: 1000.0, decimals: 1, speed: 0.1, suffix: "" }, "Dash gap");
        if changed {
            s.dash_pattern = vec![dash.max(0.1), gap.max(0.1)];
        }
    }
    if style != DashStyle::Solid {
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Dash cap");
        fig::dropdown_outlined(ui, field(r), None, &mut s.dash_cap, &[(0, "None"), (1, "Round"), (2, "Square")], "Dash cap");
    }
    // the end caps of an open path
    let open_path = s.kind == Kind::Line || (s.kind == Kind::Path && s.path.iter().any(|c| !c.closed));
    if open_path {
        for (k, name) in [(0, "Start point"), (1, "End point")] {
            let r = fig::row(ui, 32.0);
            row_label(ui, r, name);
            let mut cap = s.stroke_cap[k];
            if fig::dropdown_outlined(ui, field(r), None, &mut cap, &StrokeCap::ALL, name) {
                s.stroke_cap[k] = cap;
            }
        }
    }
    let r = fig::row(ui, 32.0);
    row_label(ui, r, "Join");
    let items: Vec<(&str, bool)> = StrokeJoin::ALL.iter().map(|(j, l)| (*l, *j == s.stroke_join)).collect();
    if let Some(k) = fig::segments_custom(ui, field(r), &items, |p, c, k, ink| join_icon(p, c, StrokeJoin::ALL[k].0, ink)) {
        s.stroke_join = StrokeJoin::ALL[k].0;
    }
    if s.stroke_join == StrokeJoin::Miter {
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Miter angle");
        let mut deg = s.miter_angle;
        if fig::num_field(ui, field(r), ui.id().with("miter"), Prefix::Icon(Glyph::RotateCw), &mut deg, Num { min: 1.0, max: 180.0, decimals: 2, speed: 0.2, suffix: "°" }, "Miter angle") {
            s.miter_angle = deg.clamp(1.0, 180.0);
        }
    }
    fig::row(ui, 12.0);
}

// what the Stroke section shows under its paints: position and weight, the two buttons, and per-side widths
pub fn stroke_controls(ui: &mut egui::Ui, g: &Grid, s: &mut Shape, key: u64, vars: &[Collection]) {
    let closed = matches!(s.kind, Kind::Rect | Kind::Frame | Kind::Ellipse | Kind::Polygon | Kind::Star) || (s.kind == Kind::Path && s.path.iter().all(|c| c.closed));
    // the labels: "Position" and "Weight", 24px above the controls
    let avail = (g.right - 60.0) - g.left - 8.0;
    let dd_w = (avail * 0.51).round();
    let wt_x = g.left + dd_w + 8.0;
    let lr = fig::row(ui, 24.0);
    for (x, t) in [(g.left, "Position"), (wt_x, "Weight")] {
        fig::ptext(ui.painter(), Pos2::new(x, lr.bottom() - 9.0), Align2::LEFT_CENTER, t, fig::reg(10.0), TEXT_SECONDARY());
    }
    let r = fig::controls(ui, 0.0);
    let mut align = if closed { s.stroke_align } else { StrokeAlign::Center };
    let items: &[(StrokeAlign, &str)] = if closed { &StrokeAlign::ALL } else { &[(StrokeAlign::Center, "Center")] };
    if fig::dropdown_outlined(ui, Rect::from_min_size(Pos2::new(g.left, r.top()), Vec2::new(dd_w, fig::ROW)), None, &mut align, items, "Stroke align") && closed {
        s.stroke_align = align;
    }
    let wt = Rect::from_min_size(Pos2::new(wt_x, r.top()), Vec2::new(avail - dd_w, fig::ROW));
    fig::num_field(ui, wt, ui.id().with(("stroke-w", key)), Prefix::Icon(Glyph::Rows), &mut s.stroke_w, Num { min: 0.0, max: 1000.0, decimals: 1, speed: 0.1, suffix: "" }, "Stroke weight");
    if let Some(c) = crate::variables::var_field(ui, wt, ui.id().with(("bind", "stroke-w", key)), s.bound(BindProp::StrokeWeight), vars, VarKind::Number, VarScope::StrokeWidth) {
        s.bind(BindProp::StrokeWeight, c);
    }
    let adv = Rect::from_min_size(Pos2::new(g.right - 52.0, r.top()), Vec2::splat(fig::ROW));
    let open_id = ui.id().with(("stroke-settings-open", key));
    let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let resp = fig::icon_button(ui, adv, Glyph::Sliders, "Advanced stroke settings", open);
    let shown = picker::popup(&resp, ui.max_rect().left(), |ui| stroke_settings(ui, s));
    ui.data_mut(|d| d.insert_temp(open_id, shown.is_some()));
    if !closed {
        // a line or an open path: its two end points (arrowheads and caps) right under the weight
        let lr = fig::row(ui, 24.0);
        for (x, t) in [(g.left, "Start point"), (wt_x, "End point")] {
            fig::ptext(ui.painter(), Pos2::new(x, lr.bottom() - 9.0), Align2::LEFT_CENTER, t, fig::reg(10.0), TEXT_SECONDARY());
        }
        let r2 = fig::controls(ui, 0.0);
        for (k, x, name) in [(0, g.left, "Start point"), (1, wt_x, "End point")] {
            fig::dropdown_outlined(ui, Rect::from_min_size(Pos2::new(x, r2.top()), Vec2::new(dd_w, fig::ROW)), None, &mut s.stroke_cap[k], &StrokeCap::ALL, name);
        }
        // how the width changes along the line
        let lr = fig::row(ui, 24.0);
        fig::ptext(ui.painter(), Pos2::new(g.left, lr.bottom() - 9.0), Align2::LEFT_CENTER, "Width profile", fig::reg(10.0), TEXT_SECONDARY());
        let r3 = fig::controls(ui, 0.0);
        fig::dropdown_outlined(ui, Rect::from_min_size(Pos2::new(g.left, r3.top()), Vec2::new(avail, fig::ROW)), None, &mut s.width_profile, &[(0, "Uniform"), (1, "Taper"), (2, "Reverse taper"), (3, "Lens"), (4, "Pinch"), (5, "Custom")], "Width profile");
        if s.width_profile == 5 {
            // a custom profile starts flat, then its points are the user's
            if s.width_points.len() < 2 {
                s.width_points = (0..5).map(|k| (k as f32 / 4.0, 1.0)).collect();
            }
            let r4 = fig::row(ui, 72.0);
            width_profile_editor(ui, Rect::from_min_size(Pos2::new(g.left, r4.top() + 4.0), Vec2::new(avail, 64.0)), &mut s.width_points, key);
        }
    }
    if matches!(s.kind, Kind::Rect | Kind::Frame) {
        let on = s.stroke_sides.is_some();
        if fig::icon_button(ui, g.icon(r), Glyph::Rect, "Individual strokes", on).clicked() {
            s.stroke_sides = if on { None } else { Some([s.stroke_w; 4]) };
        }
        if let Some(sides) = &mut s.stroke_sides {
            // top and right, then bottom and left
            for (row, names) in [("top", "right"), ("bottom", "left")].into_iter().enumerate() {
                let r = fig::controls(ui, 8.0);
                for (col, (name, idx)) in [(names.0, if row == 0 { 0 } else { 2 }), (names.1, if row == 0 { 1 } else { 3 })].into_iter().enumerate() {
                    let f = if col == 0 { g.col1(r) } else { g.col2(r) };
                    let prefix = match name {
                        "top" => "T",
                        "right" => "R",
                        "bottom" => "B",
                        _ => "L",
                    };
                    fig::num_field(ui, f, ui.id().with(("side", key, idx)), Prefix::Text(prefix), &mut sides[idx], Num { min: 0.0, max: 1000.0, decimals: 1, speed: 0.1, suffix: "" }, &format!("Stroke {name} weight"));
                }
            }
        }
    }
}

// ---- effects ----

// "+" or an empty title: the menu of effect types; the chosen one
pub fn effect_menu(resp: &Response) -> Option<EffectKind> {
    egui::Popup::menu(resp)
        .show(|ui| {
            let mut pick = None;
            for (k, label) in EffectKind::ALL {
                if menu_item(ui, Some(effect_glyph(k)), label, "", true) {
                    pick = Some(k);
                    ui.close();
                }
            }
            // Figma's list ends with Shader, which opens the shader library instead of adding an effect
            if menu_item(ui, Some(Glyph::Waves), "Shader", "", true) {
                ui.data_mut(|d| d.insert_temp(egui::Id::new("shader-library-request"), true));
                ui.close();
            }
            pick
        })
        .and_then(|r| r.inner)
}

// an effect's settings popover
pub(crate) fn effect_settings(ui: &mut egui::Ui, e: &mut Effect, key: egui::Id, vars: &[Collection]) {
    // the header: the type as a flat dropdown, then close
    let r = fig::row(ui, 40.0);
    let items: Vec<(EffectKind, &str)> = EffectKind::ALL.to_vec();
    let mut kind = e.kind;
    let dd = Rect::from_min_size(Pos2::new(r.left() + 8.0, r.top() + 8.0), Vec2::new(150.0, fig::ROW));
    let resp = ui.interact(dd, key.with("kind"), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(dd, fig::RADIUS, fig::INPUT());
    }
    paint_glyph_sized(ui.painter(), Pos2::new(dd.left() + 14.0, dd.center().y), effect_glyph(e.kind), TEXT(), 14.0);
    let t = fig::ptext(ui.painter(), Pos2::new(dd.left() + 30.0, dd.center().y), Align2::LEFT_CENTER, e.kind.label(), fig::reg(11.0), TEXT());
    paint_glyph_sized(ui.painter(), Pos2::new(t.right() + 10.0, dd.center().y), Glyph::ChevronDown, TEXT(), 9.5);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Effect type"));
    egui::Popup::menu(&resp).show(|ui| {
        for (k, label) in &items {
            if menu_item(ui, Some(effect_glyph(*k)), label, "", true) {
                kind = *k;
                ui.close();
            }
        }
    });
    if kind != e.kind {
        let was_shadow = e.kind.is_shadow();
        e.kind = kind;
        if kind.is_shadow() != was_shadow {
            *e = Effect { kind, visible: e.visible, ..Effect::new(kind) };
        }
    }
    if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 32.0, r.top() + 8.0), Vec2::splat(fig::ROW)), Glyph::Close, "Close", false).clicked() {
        ui.close();
    }
    // Figma 126.9.13 (REA + a screenshot): a shadow's blend mode is the droplet button left of the close button, not a row
    if e.kind.is_shadow() {
        let b = fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 56.0, r.top() + 8.0), Vec2::splat(fig::ROW)), Glyph::Droplet, "Blend mode", e.blend != BlendMode::Normal);
        egui::Popup::menu(&b).show(|ui| {
            for (mode, label) in BlendMode::ALL.iter().map(|b| (*b, b.label())) {
                if crate::glyphs::menu_option(ui, e.blend == mode, label) {
                    e.blend = mode;
                    ui.close();
                }
            }
        });
    }
    fig::rule(ui);
    fig::row(ui, 12.0);
    let field = |r: Rect| Rect::from_min_size(Pos2::new(r.left() + 88.0, r.top() + 4.0), Vec2::new(136.0, fig::ROW));
    if e.kind.is_shadow() {
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Position");
        fig::num_field(ui, field(r), key.with("x"), Prefix::Text("X"), &mut e.dx, n(0), "Effect X");
        if let Some(c) = crate::variables::var_field(ui, field(r), key.with("bind-x"), e.var_pos[0], vars, VarKind::Number, VarScope::Effect) {
            e.var_pos[0] = c;
        }
        let r = fig::row(ui, 32.0);
        fig::num_field(ui, field(r), key.with("y"), Prefix::Text("Y"), &mut e.dy, n(0), "Effect Y");
        if let Some(c) = crate::variables::var_field(ui, field(r), key.with("bind-y"), e.var_pos[1], vars, VarKind::Number, VarScope::Effect) {
            e.var_pos[1] = c;
        }
    }
    if e.kind == EffectKind::Glass {
        // Figma 126.9.13: Light (a dial, the angle and the intensity), a rule, then a slider each for Refraction, Depth, Dispersion,
        // Frost and Splay. Frost is a percentage here; the blur behind the glass is 16px at 100%.
        let r = fig::row(ui, 77.0);
        fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.top() + 16.0), Align2::LEFT_CENTER, "Light", fig::reg(11.0), TEXT_SECONDARY());
        fig::light_dial(ui, Rect::from_min_size(Pos2::new(r.left() + 88.0, r.top() + 3.0), Vec2::splat(56.0)), key.with("dial"), &mut e.angle, "Glass light direction");
        let fx = r.left() + 152.0;
        fig::num_field(ui, Rect::from_min_size(Pos2::new(fx, r.top() + 5.0), Vec2::new(73.0, fig::ROW)), key.with("glass-angle"), Prefix::None, &mut e.angle, Num { min: -360.0, max: 360.0, decimals: 0, speed: 1.0, suffix: "°" }, "Glass light angle");
        fig::num_field(ui, Rect::from_min_size(Pos2::new(fx, r.top() + 35.0), Vec2::new(73.0, fig::ROW)), key.with("glass-intensity"), Prefix::None, &mut e.density, Num { min: 0.0, max: 100.0, decimals: 0, speed: 1.0, suffix: "%" }, "Glass light intensity");
        fig::rule(ui);
        fig::row(ui, 12.0);
        let mut frost = (e.blur / 0.16).round();
        let rows: [(&str, &str, usize); 5] = [("Refraction", "Glass refraction", 0), ("Depth", "Glass depth", 1), ("Dispersion", "Glass dispersion", 2), ("Frost", "Glass frost", 3), ("Splay", "Glass splay", 4)];
        for (label, name, k) in rows {
            let r = fig::row(ui, 32.0);
            row_label(ui, r, label);
            let v = match k {
                0 => &mut e.spread,
                1 => &mut e.radius,
                2 => &mut e.size,
                3 => &mut frost,
                _ => &mut e.splay,
            };
            fig::slider_field(ui, field(r), key.with(("glass", k)), v, 0.0, 100.0, name);
        }
        e.blur = frost * 0.16;
    } else if e.kind == EffectKind::LayerBlur {
        // Figma: a full-width Uniform / Progressive control without a label
        let r = fig::row(ui, 32.0);
        if let Some(k) = fig::text_segments(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top() + 4.0), Vec2::new(208.0, fig::ROW)), &[("Uniform", !e.progressive, true), ("Progressive", e.progressive, true)]) {
            e.progressive = k == 1;
        }
    }
    if e.kind == EffectKind::Glass {
    } else if matches!(e.kind, EffectKind::Noise | EffectKind::Texture) {
        if e.kind == EffectKind::Noise {
            let r = fig::row(ui, 32.0);
            row_label(ui, r, "Noise type");
            fig::dropdown_outlined(ui, field(r), None, &mut e.mode, &[(0, "Mono"), (1, "Duo"), (2, "Multi")], "Noise type");
        }
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Size");
        fig::num_field(ui, field(r), key.with("nsize"), Prefix::Icon(Glyph::Grid), &mut e.size, Num { min: 0.5, max: 200.0, decimals: 1, speed: 0.1, suffix: "" }, "Grain size");
        if e.kind == EffectKind::Noise {
            let r = fig::row(ui, 32.0);
            row_label(ui, r, "Density");
            fig::num_field(ui, field(r), key.with("density"), Prefix::Icon(Glyph::Sparkles), &mut e.density, Num { min: 0.0, max: 100.0, decimals: 0, speed: 1.0, suffix: "%" }, "Noise density");
        } else {
            let r = fig::row(ui, 32.0);
            row_label(ui, r, "Radius");
            fig::num_field(ui, field(r), key.with("tradius"), Prefix::Icon(Glyph::Droplet), &mut e.radius, Num { min: 0.0, max: 100.0, decimals: 1, speed: 0.1, suffix: "" }, "Texture radius");
        }
    } else if e.kind == EffectKind::LayerBlur && e.progressive {
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Start");
        fig::num_field(ui, field(r), key.with("sblur"), Prefix::Icon(Glyph::Grid), &mut e.start_blur, Num { min: 0.0, max: 500.0, decimals: 0, ..Num::default() }, "Progressive blur at the start");
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "End");
        fig::num_field(ui, field(r), key.with("blur"), Prefix::Icon(Glyph::Grid), &mut e.blur, Num { min: 0.0, max: 500.0, decimals: 0, ..Num::default() }, "Progressive blur at the end");
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Range");
        fig::num_field(ui, Rect::from_min_size(Pos2::new(r.left() + 88.0, r.top() + 4.0), Vec2::new(66.0, fig::ROW)), key.with("from"), Prefix::Text("A"), &mut e.from, Num { min: 0.0, max: 100.0, decimals: 0, speed: 1.0, suffix: "" }, "Progressive blur starts at (%)");
        fig::num_field(ui, Rect::from_min_size(Pos2::new(r.left() + 158.0, r.top() + 4.0), Vec2::new(66.0, fig::ROW)), key.with("to"), Prefix::Text("B"), &mut e.to, Num { min: 0.0, max: 100.0, decimals: 0, speed: 1.0, suffix: "" }, "Progressive blur ends at (%)");
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Angle");
        fig::num_field(ui, field(r), key.with("angle"), Prefix::Icon(Glyph::Cursor), &mut e.angle, Num { min: -360.0, max: 360.0, decimals: 0, speed: 1.0, suffix: "°" }, "Progressive blur angle");
    } else {
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Blur");
        fig::num_field(ui, field(r), key.with("blur"), Prefix::Icon(Glyph::Grip), &mut e.blur, Num { min: 0.0, max: 500.0, decimals: 0, ..Num::default() }, "Effect blur");
        if let Some(c) = crate::variables::var_field(ui, field(r), key.with("bind-blur"), e.var_blur, vars, VarKind::Number, VarScope::Effect) {
            e.var_blur = c;
        }
    }
    if e.kind.is_shadow() || e.kind == EffectKind::Noise {
        if e.kind == EffectKind::Noise && e.mode == 1 {
            let r = fig::row(ui, 32.0);
            row_label(ui, r, "Color 2");
            let f = field(r);
            fig::input_bg(ui, f, false);
            fig::swatch(ui, Pos2::new(f.left() + 14.0, f.center().y), &mut e.color2, "Second noise color");
        }
    }
    if e.kind.is_shadow() || (e.kind == EffectKind::Noise && e.mode != 2) {
        if e.kind.is_shadow() {
            let r = fig::row(ui, 32.0);
            row_label(ui, r, "Spread");
            fig::num_field(ui, field(r), key.with("spread"), Prefix::Icon(Glyph::Sun), &mut e.spread, n(0), "Effect spread");
            if let Some(c) = crate::variables::var_field(ui, field(r), key.with("bind-spread"), e.var_pos[2], vars, VarKind::Number, VarScope::Effect) {
                e.var_pos[2] = c;
            }
        }
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Color");
        // swatch, hex and opacity in one field, like a paint row
        let f = field(r);
        fig::input_bg(ui, f, false);
        let mut c = e.color;
        fig::swatch(ui, Pos2::new(f.left() + 14.0, f.center().y), &mut c, "Effect color");
        // the hex is of the colour without its opacity (a Color32 holds it premultiplied)
        let [cr, cg, cb, ca] = c.to_srgba_unmultiplied();
        let mut rgb = Color32::from_rgb(cr, cg, cb);
        if fig::hex_text(ui, Pos2::new(f.left() + 28.0, f.center().y), f.width() - 28.0 - 48.0, &mut rgb, key.with("hex")) {
            c = Color32::from_rgba_unmultiplied(rgb.r(), rgb.g(), rgb.b(), ca);
        }
        let op = Rect::from_min_max(Pos2::new(f.right() - 48.0, f.top()), f.max);
        ui.painter().rect_filled(Rect::from_min_size(op.min, Vec2::new(1.0, op.height())), 0.0, crate::BG());
        let mut alpha = (c.a() as f32 / 255.0 * 100.0).round();
        if fig::num_field(ui, op, key.with("alpha"), Prefix::None, &mut alpha, Num { min: 0.0, max: 100.0, decimals: 0, suffix: "", speed: 1.0 }, "Effect opacity") {
            let [r, g, b, _] = c.to_srgba_unmultiplied();
            c = Color32::from_rgba_unmultiplied(r, g, b, (alpha / 100.0 * 255.0).round() as u8);
        }
        fig::text(ui, Pos2::new(op.right() - 8.0, op.center().y), Align2::RIGHT_CENTER, "%", fig::reg(11.0), TEXT());
        e.color = c;
        if let Some(v) = crate::variables::var_field(ui, f, key.with("bind-color"), e.var_color, vars, VarKind::Color, VarScope::Effect) {
            e.var_color = v;
        }
    }
    if e.kind == EffectKind::DropShadow {
        fig::row(ui, 12.0);
        fig::rule(ui);
        let r = fig::row(ui, 48.0);
        let cb = Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top() + 12.0), Vec2::new(208.0, fig::ROW));
        fig::checkbox(ui, cb, &mut e.show_behind, "Show behind transparent areas");
    } else if e.kind == EffectKind::Texture {
        fig::row(ui, 12.0);
        fig::rule(ui);
        let r = fig::row(ui, 48.0);
        let cb = Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top() + 12.0), Vec2::new(208.0, fig::ROW));
        fig::checkbox(ui, cb, &mut e.show_behind, "Clip to shape");
    } else {
        fig::row(ui, 12.0);
    }
}

// the move asked for in an effect's menu this frame, if it is for the list `owner` (a layer or style id): (index, step)
pub fn take_effect_move(ui: &egui::Ui, owner: u64) -> Option<(usize, i32)> {
    let id = egui::Id::new("effect-move");
    let (o, k, d): (u64, usize, i32) = ui.data(|data| data.get_temp(id))?;
    (o == owner).then(|| {
        ui.data_mut(|data| data.remove::<(u64, usize, i32)>(id));
        (k, d)
    })
}

// one effect of the layer: the pill (icon, name; blue while its settings are open), the eye and the minus. True when removed.
pub fn effect_row(ui: &mut egui::Ui, g: &Grid, r: Rect, e: &mut Effect, salt: (&str, usize, u64), vars: &[Collection]) -> bool {
    let id = ui.id().with(("effect", salt.0, salt.1, salt.2));
    let pill = Rect::from_min_max(Pos2::new(g.left, r.top()), Pos2::new(g.right - 60.0, r.bottom()));
    let resp = ui.interact(pill, id, Sense::click());
    let open_id = id.with("open");
    let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let (fill, ink) = if open { (fig::SELECTED(), fig::SELECTED_INK()) } else { (if resp.hovered() { fig::INPUT_HOVER() } else { fig::INPUT() }, if e.visible { TEXT() } else { TEXT_TERTIARY() }) };
    ui.painter().rect_filled(pill, fig::RADIUS, fill);
    paint_glyph_sized(ui.painter(), Pos2::new(pill.left() + 14.0, pill.center().y), effect_glyph(e.kind), ink, 14.0);
    fig::ptext(&ui.painter().with_clip_rect(pill), Pos2::new(pill.left() + 30.0, pill.center().y), Align2::LEFT_CENTER, e.kind.label(), fig::reg(11.0), ink);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, e.kind.label()));
    let shown = picker::popup(&resp, ui.max_rect().left(), |ui| effect_settings(ui, e, id, vars));
    ui.data_mut(|d| d.insert_temp(open_id, shown.is_some()));
    // the right-click menu: copy the effect's settings to another effect, and put it up or down the list
    let clip_id = egui::Id::new("effect-clipboard");
    resp.context_menu(|ui| {
        if crate::glyphs::menu_item(ui, None, "Copy settings", "", true) {
            let copy = *e;
            ui.data_mut(|d| d.insert_temp(clip_id, copy));
            ui.close();
        }
        let have: Option<Effect> = ui.data(|d| d.get_temp(clip_id));
        if crate::glyphs::menu_item(ui, None, "Paste settings", "", have.is_some()) {
            if let Some(c) = have {
                *e = c;
            }
            ui.close();
        }
        crate::glyphs::menu_sep(ui);
        // the list shows the last effect on top, so "up" is toward the end
        for (label, d) in [("Move up", 1i32), ("Move down", -1)] {
            if crate::glyphs::menu_item(ui, None, label, "", true) {
                ui.data_mut(|data| data.insert_temp(egui::Id::new("effect-move"), (salt.2, salt.1, d)));
                ui.close();
            }
        }
    });
    let eye = Rect::from_center_size(Pos2::new(g.right - 40.0, r.center().y), Vec2::splat(fig::ROW));
    if fig::icon_button(ui, eye, if e.visible { Glyph::Eye } else { Glyph::EyeOff }, if e.visible { "Hide effect" } else { "Show effect" }, false).clicked() {
        e.visible ^= true;
    }
    fig::icon_button(ui, g.icon(r), Glyph::Minus, "Remove effect", false).clicked()
}

// the Custom width profile as a picture of the stroke seen from the side, start at the left and end at the right, with a handle
// on each point: drag a handle to move it (the two ends only up and down), click the empty space to add a point, double-click
// a point to remove it
fn width_profile_editor(ui: &mut egui::Ui, r: Rect, points: &mut Vec<(f32, f32)>, key: u64) {
    let id = ui.id().with(("width-profile", key));
    let resp = ui.interact(r, id, Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Width profile editor"));
    fig::input_bg(ui, r, false);
    let inner = r.shrink2(Vec2::new(10.0, 8.0));
    let cy = inner.center().y;
    let pos = |t: f32, w: f32| Pos2::new(inner.left() + t * inner.width(), cy - w * inner.height() / 2.0);
    let drag_id = id.with("dragging");
    let nearest = |p: Pos2, pts: &[(f32, f32)]| pts.iter().enumerate().map(|(k, &(t, w))| (k, pos(t, w).distance(p))).filter(|(_, d)| *d <= 9.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(k, _)| k);
    if resp.drag_started() {
        let hit = resp.interact_pointer_pos().and_then(|p| nearest(p, points));
        ui.data_mut(|d| d.insert_temp(drag_id, hit.unwrap_or(usize::MAX)));
    }
    if resp.dragged() {
        let which = ui.data(|d| d.get_temp::<usize>(drag_id)).unwrap_or(usize::MAX);
        if let (Some(p), true) = (resp.interact_pointer_pos(), which < points.len()) {
            let last = points.len() - 1;
            let w = ((cy - p.y) * 2.0 / inner.height()).clamp(0.0, 1.0);
            let t = if which == 0 || which == last { points[which].0 } else { ((p.x - inner.left()) / inner.width()).clamp(0.02, 0.98) };
            points[which] = (t, w);
        }
    }
    if resp.double_clicked() {
        if let Some(k) = resp.interact_pointer_pos().and_then(|p| nearest(p, points)) {
            if points.len() > 2 && k != 0 && k != points.len() - 1 {
                points.remove(k);
            }
        }
    } else if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos().filter(|p| nearest(*p, points).is_none()) {
            let t = ((p.x - inner.left()) / inner.width()).clamp(0.02, 0.98);
            points.push((t, ((cy - p.y).abs() * 2.0 / inner.height()).clamp(0.0, 1.0)));
        }
    }
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    // the stroke seen from the side: a strip mirrored about the middle line
    let n = 48;
    let mut mesh = egui::Mesh::default();
    for k in 0..=n {
        let p = pos(k as f32 / n as f32, crate::geom::points_width(points, k as f32 / n as f32));
        mesh.colored_vertex(p, fig::SELECTED());
        mesh.colored_vertex(Pos2::new(p.x, 2.0 * cy - p.y), fig::SELECTED());
        if k > 0 {
            let i = 2 * k as u32;
            mesh.add_triangle(i - 2, i - 1, i);
            mesh.add_triangle(i - 1, i + 1, i);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    for sign in [1.0, -1.0] {
        let edge: Vec<Pos2> = (0..=n).map(|k| { let p = pos(k as f32 / n as f32, crate::geom::points_width(points, k as f32 / n as f32)); Pos2::new(p.x, cy + sign * (p.y - cy)) }).collect();
        ui.painter().add(egui::Shape::line(edge, Stroke::new(1.0, fig::SELECTED_INK())));
    }
    ui.painter().line_segment([Pos2::new(inner.left(), cy), Pos2::new(inner.right(), cy)], Stroke::new(1.0, fig::RULE()));
    for &(t, w) in points.iter() {
        let c = pos(t, w);
        ui.painter().circle_filled(c, 4.0, Color32::WHITE);
        ui.painter().circle_stroke(c, 4.0, Stroke::new(1.0, fig::SELECTED_INK()));
    }
}
