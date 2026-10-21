// The Design tab, laid out like Figma UI3's right panel (the numbers are in fig.rs): a layer header,
// then Position, Layout, Appearance, Fill, Stroke, Effects and Export sections on the two-column grid.
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};

use crate::fig::{self, Grid, Num, Prefix};
use crate::geom::BoolOp;
use crate::glyphs::{menu_item, paint_glyph_sized, Glyph};
use crate::model::*;
use crate::ops::AlignOp;
use crate::panels::{sizing_get, sizing_set, Act, Sizing};
use crate::variables::{var_field, var_submenu};
use crate::{App, TEXT, TEXT_SECONDARY, TEXT_TERTIARY};

const CONSTRAINTS_H: [(Constraint, &str); 5] = [(Constraint::Min, "Left"), (Constraint::Max, "Right"), (Constraint::Both, "Left and right"), (Constraint::Center, "Center"), (Constraint::Scale, "Scale")];
const CONSTRAINTS_V: [(Constraint, &str); 5] = [(Constraint::Min, "Top"), (Constraint::Max, "Bottom"), (Constraint::Both, "Top and bottom"), (Constraint::Center, "Center"), (Constraint::Scale, "Scale")];

fn n(decimals: usize) -> Num {
    Num { decimals, ..Num::default() }
}

// a paint's editor in Figma's picker: Custom (the paint type, then the color, gradient stops or image)
// and Libraries (the color styles); + makes a style of the color
fn paint_picker(ui: &mut egui::Ui, p: &mut Paint, id: egui::Id, styles: &[ColorStyle], variables: &[Collection], images: &[Asset], textures: &std::collections::HashMap<u64, egui::TextureHandle>, new_style: &mut Option<Color32>, upload: &mut bool) {
    use crate::picker::{self, Tab};
    let tab_id = id.with("picker-tab");
    let mut tab = ui.data(|d| d.get_temp::<Tab>(tab_id)).unwrap_or(if p.style.is_some() { Tab::Libraries } else { Tab::Custom });
    let solid = match p.kind {
        PaintKind::Solid(c) => Some(c),
        _ => None,
    };
    let (plus, close) = picker::header(ui, &mut tab, true, solid.map(|_| "Create style"));
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    if let (true, Some(c)) = (plus, solid) {
        *new_style = Some(c);
        ui.close();
    }
    if close {
        ui.close();
    }
    if tab == Tab::Libraries {
        fig::rule(ui);
        fig::row(ui, 8.0);
        if styles.is_empty() && variables.is_empty() {
            let r = fig::row(ui, 32.0);
            fig::text(ui, Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "No color styles or variables yet", fig::reg(11.0), TEXT_SECONDARY());
        }
        for st in styles {
            let r = fig::row(ui, 32.0);
            let hl = Rect::from_min_max(Pos2::new(r.left() + 8.0, r.top() + 4.0), Pos2::new(r.right() - 8.0, r.bottom() - 4.0));
            let resp = ui.interact(hl, id.with(("style", st.id)), Sense::click());
            let on = p.style == Some(st.id);
            if on {
                ui.painter().rect_filled(hl, fig::RADIUS, fig::SELECTED());
            } else if resp.hovered() {
                ui.painter().rect_filled(hl, fig::RADIUS, fig::INPUT());
            }
            let sw = Rect::from_center_size(Pos2::new(r.left() + 24.0, r.center().y), Vec2::splat(16.0));
            ui.painter().rect_filled(sw, 2, st.color);
            ui.painter().rect_stroke(sw, 2, Stroke::new(1.0, Color32::from_white_alpha(20)), egui::StrokeKind::Inside);
            fig::text(ui, Pos2::new(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, &st.name, fig::reg(11.0), TEXT());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, format!("Color style {}", st.name)));
            if resp.clicked() {
                p.style = Some(st.id);
                p.kind = PaintKind::Solid(st.color);
            }
        }
        if p.style.is_some() {
            let r = fig::row(ui, 32.0);
            if fig::button(ui, Rect::from_min_max(Pos2::new(r.left() + 16.0, r.top() + 4.0), Pos2::new(r.right() - 16.0, r.bottom() - 4.0)), "Detach style").clicked() {
                p.style = None;
            }
        }
        // colour variables, grouped by collection: one binds the paint (its value in the layer's modes follows)
        for c in variables {
            let colors: Vec<&Variable> = c.vars.iter().filter(|v| v.kind == VarKind::Color && (v.allows(VarScope::Fill) || v.allows(VarScope::Stroke))).collect();
            if colors.is_empty() {
                continue;
            }
            let r = fig::row(ui, 32.0);
            fig::text(ui, Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, &c.name, fig::semi(11.0), TEXT_SECONDARY());
            for v in colors {
                let r = fig::row(ui, 32.0);
                let hl = Rect::from_min_max(Pos2::new(r.left() + 8.0, r.top() + 4.0), Pos2::new(r.right() - 8.0, r.bottom() - 4.0));
                let resp = ui.interact(hl, id.with(("variable", v.id)), Sense::click());
                let on = p.var == Some(v.id);
                if on {
                    ui.painter().rect_filled(hl, fig::RADIUS, fig::SELECTED());
                } else if resp.hovered() {
                    ui.painter().rect_filled(hl, fig::RADIUS, fig::INPUT());
                }
                let first = match v.values.first() {
                    Some(VarValue::Color(c)) => *c,
                    _ => Color32::GRAY,
                };
                let sw = Rect::from_center_size(Pos2::new(r.left() + 24.0, r.center().y), Vec2::splat(16.0));
                ui.painter().rect_filled(sw, 2, first);
                ui.painter().rect_stroke(sw, 2, Stroke::new(1.0, Color32::from_white_alpha(20)), egui::StrokeKind::Inside);
                fig::text(ui, Pos2::new(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, &v.name, fig::reg(11.0), TEXT());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, format!("Color variable {}", v.name)));
                if resp.clicked() {
                    p.var = Some(v.id);
                    p.style = None;
                    if let Some(VarValue::Color(c)) = v.values.first() {
                        p.kind = PaintKind::Solid(*c);
                    }
                }
            }
        }
        if p.var.is_some() {
            let r = fig::row(ui, 32.0);
            if fig::button(ui, Rect::from_min_max(Pos2::new(r.left() + 16.0, r.top() + 4.0), Pos2::new(r.right() - 16.0, r.bottom() - 4.0)), "Detach variable").clicked() {
                p.var = None;
            }
        }
        fig::row(ui, 8.0);
        return;
    }
    // older files have two-colour gradients: they become the general kind before they are edited
    if let PaintKind::Linear { a, b, angle } = p.kind {
        p.kind = PaintKind::Gradient(Gradient { angle, ..Gradient::between(a, b) });
    }
    let base = match &p.kind {
        PaintKind::Solid(c) => *c,
        PaintKind::Gradient(g) => g.stops.first().map_or(GREY_100, |s| s.color),
        _ => GREY_100,
    };
    let which = match p.kind {
        PaintKind::Solid(_) => 0,
        PaintKind::Linear { .. } | PaintKind::Gradient(_) => 1,
        PaintKind::Pattern(_) => 2,
        PaintKind::Image(_) if p.video.is_some() => 4,
        PaintKind::Image(_) => 3,
        PaintKind::Shader(_) => 5,
    };
    let (chosen, to_libraries) = crate::picker::paint_types(ui, which);
    if to_libraries {
        ui.data_mut(|d| d.insert_temp(tab_id, Tab::Libraries));
    }
    if let Some(k) = chosen {
        if k != which {
            p.style = None;
            // Image and Video are the same paint (an image, with a video behind it for Video): switching between them keeps the image
            let keep_image = matches!((which, k), (3, 4) | (4, 3));
            p.video = (k == 4).then(|| p.video.unwrap_or(0));
            if !keep_image {
            p.kind = match k {
                0 => PaintKind::Solid(base),
                1 => {
                    // Figma starts a gradient at the current colour and runs it toward a lighter (or darker) one
                    let [r, g, b, a] = base.to_srgba_unmultiplied();
                    let dark = (r as u32 + g as u32 + b as u32) < 3 * 128;
                    let end = if dark { 255.0 } else { 0.0 };
                    let mix = |v: u8| (v as f32 + (end - v as f32) * 0.4).round() as u8;
                    PaintKind::Gradient(Gradient::between(base, Color32::from_rgba_unmultiplied(mix(r), mix(g), mix(b), a)))
                }
                2 => PaintKind::Pattern(PatternFill::default()),
                5 => PaintKind::Shader(ShaderFill { a: base, ..ShaderFill::default() }),
                _ => PaintKind::Image(images.first().map_or(0, |a| a.id)),
            };
            }
        }
    }
    let popup_left = ui.max_rect().left();
    match &mut p.kind {
        PaintKind::Solid(c) => {
            if crate::picker::body(ui, c, id.with("picker")) {
                p.style = None;
                p.var = None;
            }
        }
        PaintKind::Gradient(g) => {
            crate::picker::gradient_editor(ui, g, id.with("gradient"), popup_left);
        }
        PaintKind::Image(img) => {
            crate::picker::image_editor(ui, &mut p.image, img, images, textures, id.with("image"), upload);
        }
        PaintKind::Pattern(pf) => crate::picker::pattern_editor(ui, pf, id.with("pattern")),
        PaintKind::Shader(sf) => crate::picker::shader_editor(ui, sf, id.with("shader")),
        PaintKind::Linear { .. } => {}
    }
    // how this paint mixes with what is under it: Figma 126.9.13's picker has no such row above the colour square (its square starts
    // 96px under the top), so it closes the popover
    fig::rule(ui);
    let r = fig::row(ui, fig::ROW + 16.0);
    let items: Vec<(BlendMode, &str)> = BlendMode::ALL.iter().map(|b| (*b, b.label())).collect();
    fig::text(ui, Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Blend", fig::reg(11.0), TEXT_SECONDARY());
    fig::dropdown(ui, Rect::from_min_size(Pos2::new(r.left() + 100.0, r.top() + 8.0), Vec2::new(124.0, fig::ROW)), None, &mut p.blend, &items, "Paint blend mode");
}

// one paint as a Figma row: swatch and hex (or the paint type), opacity, visibility, remove.
// Returns (remove, make a style of this color).
fn paint_row(ui: &mut egui::Ui, g: &Grid, r: Rect, p: &mut Paint, salt: (&str, usize), styles: &[ColorStyle], variables: &[Collection], images: &[Asset], textures: &std::collections::HashMap<u64, egui::TextureHandle>, upload: &mut bool) -> (bool, Option<Color32>) {
    let input = Rect::from_min_max(Pos2::new(g.left, r.top()), Pos2::new(g.right - 60.0, r.bottom()));
    let op = Rect::from_min_max(Pos2::new(input.right() - 53.0, r.top()), input.max);
    fig::input_bg(ui, input, false);
    let cy = r.center().y;
    let sw = Rect::from_center_size(Pos2::new(input.left() + 12.0, cy), Vec2::splat(14.0));
    let id = ui.id().with(("paint", salt.0, salt.1));
    let resp = ui.interact(sw.expand(3.0), id, Sense::click());
    // the swatch
    match &p.kind {
        PaintKind::Solid(c) => {
            let shown = p.style.and_then(|s| styles.iter().find(|x| x.id == s)).map_or(*c, |s| s.color);
            ui.painter().rect_filled(sw, 2.0, shown);
        }
        PaintKind::Linear { .. } | PaintKind::Gradient(_) => {
            let g = match &p.kind {
                PaintKind::Gradient(g) => g.clone(),
                PaintKind::Linear { a, b, angle } => Gradient { angle: *angle, ..Gradient::between(*a, *b) },
                _ => Gradient::default(),
            };
            // the gradient across the swatch (a linear preview whatever the kind)
            let mut mesh = egui::Mesh::default();
            let n = 8;
            for k in 0..=n {
                let t = k as f32 / n as f32;
                let c = g.at(t);
                mesh.colored_vertex(Pos2::new(sw.left() + t * sw.width(), sw.top()), c);
                mesh.colored_vertex(Pos2::new(sw.left() + t * sw.width(), sw.bottom()), c);
                if k > 0 {
                    let b = (k * 2) as u32;
                    mesh.add_triangle(b - 2, b - 1, b);
                    mesh.add_triangle(b - 1, b, b + 1);
                }
            }
            ui.painter().add(mesh);
        }
        PaintKind::Image(img) => match textures.get(img) {
            Some(t) => {
                ui.painter().image(t.id(), sw, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                ui.painter().rect_filled(sw, 2.0, Color32::from_gray(90));
                paint_glyph_sized(ui.painter(), sw.center(), Glyph::Image, TEXT(), 12.0);
            }
        },
        PaintKind::Pattern(_) => {
            ui.painter().rect_filled(sw, 2.0, Color32::from_gray(90));
            paint_glyph_sized(ui.painter(), sw.center(), Glyph::Grid, TEXT(), 12.0);
        }
        PaintKind::Shader(sf) => {
            let mut mesh = egui::Mesh::default();
            for (k, c) in [sf.a, sf.b].into_iter().enumerate() {
                let x = sw.left() + k as f32 * sw.width();
                mesh.colored_vertex(Pos2::new(x, sw.top()), c);
                mesh.colored_vertex(Pos2::new(x, sw.bottom()), c);
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(1, 2, 3);
            ui.painter().add(mesh);
        }
    }
    ui.painter().rect_stroke(sw, 2.0, Stroke::new(1.0, Color32::from_black_alpha(50)), egui::StrokeKind::Inside);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ColorButton, true, format!("{} {} swatch", salt.0, salt.1 + 1)));
    // its editor: the paint type, then a picker for it
    let mut new_style = None;
    let open = crate::picker::popup(&resp, ui.max_rect().left(), |ui| paint_picker(ui, p, id, styles, variables, images, textures, &mut new_style, upload)).is_some();
    // while a gradient's popover is open its handles are on the canvas (gradient_edit.rs)
    if open && matches!(p.kind, PaintKind::Gradient(_) | PaintKind::Linear { .. }) {
        let pass = ui.ctx().cumulative_pass_nr();
        ui.ctx().data_mut(|d| d.insert_temp(crate::gradient_edit::bridge_id(), (salt.0 == "Stroke", salt.1, pass)));
    }
    // the value: hex, style name or paint type
    let tx = Pos2::new(input.left() + 24.0, cy);
    let text_w = op.left() - tx.x - 4.0;
    match &mut p.kind {
        PaintKind::Solid(c) => match (p.style.and_then(|s| styles.iter().find(|x| x.id == s)), p.var.and_then(|v| variables.iter().find_map(|c| c.vars.iter().find(|x| x.id == v)))) {
            (Some(st), _) => fig::text(ui, tx, Align2::LEFT_CENTER, &st.name, fig::reg(11.0), TEXT()),
            (None, Some(v)) => {
                fig::ptext(&ui.painter().with_clip_rect(Rect::from_min_max(input.min, Pos2::new(op.left() - 4.0, input.bottom()))), tx, Align2::LEFT_CENTER, &v.name, fig::reg(11.0), fig::SELECTED_INK());
            }
            (None, None) => {
                fig::hex_text(ui, tx, text_w, c, id.with("hex"));
            }
        },
        PaintKind::Linear { .. } => fig::text(ui, tx, Align2::LEFT_CENTER, "Linear", fig::reg(11.0), TEXT()),
        PaintKind::Gradient(g) => fig::text(ui, tx, Align2::LEFT_CENTER, GradientKind::ALL.iter().find(|(k, _)| *k == g.kind).map_or("Gradient", |(_, l)| l), fig::reg(11.0), TEXT()),
        PaintKind::Image(img) => {
            let name = images.iter().find(|a| a.id == *img).map_or("Image", |a| a.name.as_str());
            fig::ptext(&ui.painter().with_clip_rect(Rect::from_min_max(input.min, Pos2::new(op.left() - 4.0, input.bottom()))), tx, Align2::LEFT_CENTER, name, fig::reg(11.0), TEXT());
        }
        PaintKind::Pattern(_) => fig::text(ui, tx, Align2::LEFT_CENTER, "Pattern", fig::reg(11.0), TEXT()),
        PaintKind::Shader(sf) => fig::text(ui, tx, Align2::LEFT_CENTER, ShaderKind::ALL.iter().find(|(k, _)| *k == sf.kind).map_or("Shader", |(_, l)| l), fig::reg(11.0), TEXT()),
    }
    // opacity, after a hairline
    ui.painter().rect_filled(Rect::from_min_size(op.min, Vec2::new(1.0, op.height())), 0.0, crate::BG());
    let mut pct = p.opacity * 100.0;
    if fig::num_field(ui, op, id.with("opacity"), Prefix::None, &mut pct, Num { min: 0.0, max: 100.0, decimals: 0, suffix: "", speed: 1.0 }, &format!("{} {} opacity", salt.0, salt.1 + 1)) {
        p.opacity = pct / 100.0;
    }
    fig::text(ui, Pos2::new(op.right() - 8.0, cy), Align2::RIGHT_CENTER, "%", fig::reg(11.0), TEXT());
    let eye = Rect::from_center_size(Pos2::new(g.right - 40.0, cy), Vec2::splat(fig::ROW));
    if fig::icon_button(ui, eye, if p.visible { Glyph::Eye } else { Glyph::EyeOff }, if p.visible { "Hide" } else { "Show" }, false).clicked() {
        p.visible ^= true;
    }
    let remove = fig::icon_button(ui, g.icon(r), Glyph::Minus, "Remove", false).clicked();
    (remove, new_style)
}

// Figma's constraints picker: a box in a box, a bar on each side (blue when pinned), a cross in the middle
fn constraint_box(ui: &mut egui::Ui, r: Rect, ch: &mut Constraint, cv: &mut Constraint) {
    ui.painter().rect_filled(r, fig::RADIUS, fig::INPUT());
    let inner = Rect::from_center_size(r.center(), Vec2::new((r.width() - 48.0).clamp(32.0, 84.0), 24.0));
    ui.painter().rect_stroke(inner, 3.0, Stroke::new(1.0, Color32::from_gray(0x4c)), egui::StrokeKind::Inside);
    let (l, rt) = (matches!(ch, Constraint::Min | Constraint::Both), matches!(ch, Constraint::Max | Constraint::Both));
    let (t, b) = (matches!(cv, Constraint::Min | Constraint::Both), matches!(cv, Constraint::Max | Constraint::Both));
    let c = r.center();
    let bars = [
        ("Pin left", Pos2::new(r.left() + 6.0, c.y), Pos2::new(inner.left() - 5.0, c.y), l),
        ("Pin right", Pos2::new(inner.right() + 5.0, c.y), Pos2::new(r.right() - 6.0, c.y), rt),
        ("Pin top", Pos2::new(c.x, r.top() + 5.0), Pos2::new(c.x, inner.top() - 4.0), t),
        ("Pin bottom", Pos2::new(c.x, inner.bottom() + 4.0), Pos2::new(c.x, r.bottom() - 5.0), b),
    ];
    for (k, (name, a, z, on)) in bars.into_iter().enumerate() {
        let hit = Rect::from_two_pos(a, z).expand(5.0);
        let resp = ui.interact(hit, ui.id().with(("pin", k)), Sense::click());
        ui.painter().line_segment([a, z], Stroke::new(if on { 2.0 } else { 1.0 }, if on { fig::SELECTED_INK() } else if resp.hovered() { TEXT() } else { TEXT_TERTIARY() }));
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, name));
        if resp.clicked() {
            let pick = |a: bool, z: bool| match (a, z) {
                (true, true) => Constraint::Both,
                (true, false) => Constraint::Min,
                (false, true) => Constraint::Max,
                (false, false) => Constraint::Center,
            };
            match k {
                0 => *ch = pick(!l, rt),
                1 => *ch = pick(l, !rt),
                2 => *cv = pick(!t, b),
                _ => *cv = pick(t, !b),
            }
        }
    }
    // the cross: centre lines light up for Center
    let hc = *ch == Constraint::Center;
    let vc = *cv == Constraint::Center;
    ui.painter().line_segment([c - Vec2::new(6.0, 0.0), c + Vec2::new(6.0, 0.0)], Stroke::new(1.0, if hc { fig::SELECTED_INK() } else { TEXT() }));
    ui.painter().line_segment([c - Vec2::new(0.0, 6.0), c + Vec2::new(0.0, 6.0)], Stroke::new(1.0, if vc { fig::SELECTED_INK() } else { TEXT() }));
    let resp = ui.interact(Rect::from_center_size(c, Vec2::splat(14.0)), ui.id().with("pin-center"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, hc && vc, "Center constraints"));
    if resp.clicked() {
        *ch = Constraint::Center;
        *cv = Constraint::Center;
    }
}

// the 3x3 alignment grid of an auto layout frame
fn alignment_box(ui: &mut egui::Ui, r: Rect, l: &mut AutoLayout) {
    ui.painter().rect_filled(r, fig::RADIUS, fig::INPUT());
    let pick = |a: usize| [Align::Start, Align::Center, Align::End][a];
    let idx = |a: Align| match a {
        Align::Start => Some(0),
        Align::Center => Some(1),
        Align::End => Some(2),
        Align::SpaceBetween => None,
    };
    let cell = Vec2::new(r.width() / 3.0, r.height() / 3.0);
    for row in 0..3 {
        for col in 0..3 {
            let c = Rect::from_min_size(r.min + Vec2::new(col as f32 * cell.x, row as f32 * cell.y), cell);
            let resp = ui.interact(c, ui.id().with(("align9", row, col)), Sense::click());
            let (main_i, cross_i) = if l.vertical { (row, col) } else { (col, row) };
            let on = idx(l.main).is_none_or(|m| m == main_i) && idx(l.cross) == Some(cross_i);
            let name = format!("Align {} {}", ["top", "middle", "bottom"][row], ["left", "center", "right"][col]);
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, name.clone()));
            if on {
                // a short bar in the flow direction, like Figma
                let d = if l.vertical { Vec2::new(5.0, 0.0) } else { Vec2::new(0.0, 5.0) };
                ui.painter().line_segment([c.center() - d, c.center() + d], Stroke::new(2.0, fig::SELECTED_INK()));
            } else {
                ui.painter().circle_filled(c.center(), 1.5, if resp.hovered() { TEXT() } else { TEXT_TERTIARY() });
            }
            if resp.clicked() {
                if l.main != Align::SpaceBetween {
                    l.main = pick(main_i);
                }
                l.cross = pick(cross_i);
            }
        }
    }
}

// what Figma titles the Design panel with: the layer's type, not its name
fn type_label(s: &Shape) -> &'static str {
    if s.component_set {
        return "Component set";
    }
    if s.component {
        return "Component";
    }
    if s.instance {
        return "Instance";
    }
    if s.section {
        return "Section";
    }
    if let Some(k) = s.bool_op {
        return ["Union", "Subtract", "Intersect", "Exclude"][k.min(3) as usize];
    }
    match s.kind {
        Kind::Frame => "Frame",
        Kind::Group => "Group",
        Kind::Rect => "Rectangle",
        Kind::Ellipse => "Ellipse",
        Kind::Line => "Line",
        Kind::Text => "Text",
        Kind::Polygon => "Polygon",
        Kind::Star => "Star",
        Kind::Path => "Vector path",
    }
}

impl App {
    // the layer's type and Figma's quick actions, 48px, then a divider
    fn layer_header(&mut self, ui: &mut egui::Ui, i: usize, act: &mut Option<Act>) {
        let r = fig::row(ui, 48.0);
        let (kind, mask, special, id) = {
            let s = &self.shapes[i];
            (s.kind, s.mask, s.component || s.instance, s.id)
        };
        // what the header offers per kind (Figma 126.9.13): a frame has Dev Mode, Create component and More; a section only Dev
        // Mode; a group Dev Mode, component, mask and boolean groups; plain shapes component, mask, boolean groups and Edit
        // object; a vector path the same with More
        let (is_section, is_path) = (self.shapes[i].section, kind == Kind::Path);
        let mut actions: Vec<(Glyph, &str, bool, u8)> = vec![];
        let bool_group = kind == Kind::Group && self.shapes[i].bool_op.is_some();
        if kind == Kind::Frame || (kind == Kind::Group && !bool_group) {
            actions.push((Glyph::Code, "Toggle ready for dev status", self.shapes[i].ready, 4));
        }
        let main_component = self.shapes[i].component && !self.shapes[i].component_set;
        if main_component {
            // Figma 126.9.13: the component's own name, Add variant, Component configuration and More
            actions.clear();
            actions.push((Glyph::Instance, "Add variant", false, 7));
            actions.push((Glyph::Sliders, "Component configuration", false, 8));
            actions.push((Glyph::More, "More actions", false, 5));
        } else if self.shapes[i].instance {
            // Figma 126.9.13: an instance has the main component's name as its title and only More in the header
            actions.clear();
            actions.push((Glyph::More, "More actions", false, 5));
        } else if is_section {
            actions.truncate(1);
        } else {
            if !special && !bool_group {
                actions.push((Glyph::Component, "Create component (Ctrl+Alt+K)", false, 0));
            }
            if kind == Kind::Text {
                // Figma 126.9.13: Create link, Apply variable (the text of a string variable), Create component, More actions
                let linked = self.shapes[i].runs.iter().any(|r| r.url.is_some());
                actions.insert(0, (Glyph::Variable, "Apply variable", self.shapes[i].bound(BindProp::Text).is_some(), 10));
                actions.insert(0, (Glyph::Link, "Create link (Ctrl+Shift+U)", linked, 9));
                actions.push((Glyph::More, "More actions", false, 5));
            }
            if kind != Kind::Frame && kind != Kind::Text {
                actions.push((Glyph::Mask, "Use as mask (Ctrl+Alt+M)", mask, 1));
                actions.push((Glyph::Union, "Boolean groups", false, 2));
            }
            if bool_group {
                actions.push((Glyph::Component, "Create component (Ctrl+Alt+K)", false, 0));
            }
            if kind == Kind::Frame || is_path || kind == Kind::Line {
                actions.push((Glyph::More, "More actions", false, 5));
            } else if !kind.is_container() && kind != Kind::Text {
                actions.push((Glyph::Vector, "Edit object (Enter)", false, 6));
            }
        }
        let menu_flags: Vec<bool> = actions.iter().map(|a| a.3 == 2).collect();
        let xs = fig::button_xs(r.right() - 8.0, &menu_flags);
        let name_right = xs.first().copied().unwrap_or(r.right() - 8.0) - 8.0;
        let name_rect = Rect::from_min_max(Pos2::new(r.left() + 12.0, r.top() + 12.0), Pos2::new(name_right, r.bottom() - 12.0));
        let edit_id = ui.id().with(("rename-header", id));
        let editing = ui.data(|d| d.get_temp::<bool>(edit_id)).unwrap_or(false);
        if editing {
            let out = fig::put(ui, name_rect, egui::TextEdit::singleline(&mut self.shapes[i].name).id(ui.id().with(("layer-name", id))).font(fig::semi(13.0)).margin(egui::Margin::symmetric(4, 2)).vertical_align(egui::Align::Center));
            if !out.has_focus() && !out.lost_focus() {
                out.request_focus();
            }
            if out.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ui.data_mut(|d| d.remove::<bool>(edit_id));
            }
        } else {
            let resp = ui.interact(name_rect, edit_id.with("label"), Sense::click());
            let ink = TEXT(); // Figma 126.9.13 titles components and instances in white too
            // Figma titles the panel with the layer's type ("Frame", "Rectangle", "Vector path"), the name lives in the layers list;
            // frames, groups and sections get a chevron that opens the layer's menu
            let main_name = self.shapes[i].src.and_then(|m| self.index_of(m)).map(|m| self.shapes[m].name.clone());
            let title: String = if main_component { self.shapes[i].name.clone() } else if let Some(n) = main_name.filter(|_| self.shapes[i].instance) { n } else { type_label(&self.shapes[i]).to_owned() };
            let title = title.as_str();
            let text = fig::ptext(&ui.painter().with_clip_rect(name_rect), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, title, fig::semi(13.0), ink);
            let has_chev = kind.is_container() && self.shapes[i].bool_op.is_none() && !main_component;
            let chev = if has_chev { Rect::from_center_size(Pos2::new((text.right() + 14.0).min(name_rect.right() - 8.0), r.center().y), Vec2::splat(20.0)) } else { text.expand2(Vec2::new(4.0, 6.0)) };
            let cresp = ui.interact(chev, edit_id.with("menu"), Sense::click());
            if cresp.hovered() {
                ui.painter().rect_filled(chev, fig::RADIUS, fig::INPUT());
            }
            if has_chev {
                paint_glyph_sized(ui.painter(), chev.center(), Glyph::ChevronDown, TEXT(), 9.5);
            }
            cresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Layer menu"));
            let (visible, locked) = (self.shapes[i].visible, self.shapes[i].locked);
            egui::Popup::menu(&cresp).show(|ui| {
                if menu_item(ui, None, "Rename", "Double-click", true) {
                    ui.data_mut(|d| d.insert_temp(edit_id, true));
                    ui.close();
                }
                // an instance's title lists the components it can be swapped to
                if self.shapes[i].instance {
                    if self.main_is_missing(i) {
                        if menu_item(ui, Some(Glyph::Component), "Restore main component", "", true) {
                            *act = Some(Act::RestoreMain);
                            ui.close();
                        }
                    } else if menu_item(ui, Some(Glyph::Component), "Push overrides to main component", "", true) {
                        *act = Some(Act::PushOverrides);
                        ui.close();
                    }
                    let cur = self.shapes[i].src;
                    for (cid, cname) in self.components() {
                        if crate::glyphs::menu_item_checked(ui, Some(Glyph::Component), cur == Some(cid), &cname, "") && cur != Some(cid) {
                            *act = Some(Act::Swap(cid));
                            ui.close();
                        }
                    }
                    crate::glyphs::menu_sep(ui);
                }
                // a frame's title opens Figma's "Frame Dimension Presets": a click on a size resizes the frame
                if kind == Kind::Frame && !self.shapes[i].section {
                    for (group, items) in crate::presets::GROUPS.iter().filter(|g| !g.1.is_empty()) {
                        crate::glyphs::menu_submenu(ui, None, group, |ui| {
                            for (name, w, h) in items.iter() {
                                if menu_item(ui, None, name, &format!("{w}×{h}"), true) {
                                    self.shapes[i].size = Vec2::new(*w, *h);
                                    ui.close();
                                }
                            }
                        });
                    }
                    crate::glyphs::menu_sep(ui);
                }
                // a frame and a section are the same container drawn two ways
                if kind == Kind::Frame {
                    let sec = self.shapes[i].section;
                    if crate::glyphs::menu_item_checked(ui, Some(Glyph::Frame), !sec, "Frame", "") && sec {
                        self.shapes[i].section = false;
                        self.shapes[i].clip = true;
                        ui.close();
                    }
                    if crate::glyphs::menu_item_checked(ui, Some(Glyph::Section), sec, "Section", "") && !sec {
                        self.shapes[i].section = true;
                        self.shapes[i].clip = false;
                        ui.close();
                    }
                    crate::glyphs::menu_sep(ui);
                }
                if menu_item(ui, Some(if visible { Glyph::EyeOff } else { Glyph::Eye }), if visible { "Hide" } else { "Show" }, "", true) {
                    self.shapes[i].visible = !visible;
                    ui.close();
                }
                if menu_item(ui, Some(if locked { Glyph::LockOpen } else { Glyph::Lock }), if locked { "Unlock" } else { "Lock" }, "", true) {
                    self.shapes[i].locked = !locked;
                    ui.close();
                }
                // properties of the component this layer is in: a boolean shows / hides it, a text sets a text layer's text, an
                // instance swap picks a nested instance's component
                if self.chain(i).iter().any(|&k| self.shapes[k].component) {
                    crate::glyphs::menu_sep(ui);
                    let linked = self.shapes[i].cprop_link;
                    if linked.is_some() {
                        if menu_item(ui, None, "Detach component property", "", true) {
                            self.shapes[i].cprop_link = None;
                            ui.close();
                        }
                    } else {
                        let kind = self.shapes[i].kind;
                        let inst = self.shapes[i].instance;
                        if menu_item(ui, None, "Create boolean property (visibility)", "", true) {
                            self.create_property(i, CompPropKind::Bool);
                            ui.close();
                        }
                        if kind == Kind::Text && menu_item(ui, None, "Create text property", "", true) {
                            self.create_property(i, CompPropKind::Text);
                            ui.close();
                        }
                        if inst && menu_item(ui, None, "Create instance swap property", "", true) {
                            self.create_property(i, CompPropKind::Swap);
                            ui.close();
                        }
                    }
                }
                crate::glyphs::menu_sep(ui);
                // boolean and string variables drive visibility and the text of a text layer
                if let Some(c) = var_submenu(ui, "Visibility variable", self.shapes[i].bound(BindProp::Visible), &self.variables, VarKind::Bool, VarScope::Visibility) {
                    self.shapes[i].bind(BindProp::Visible, c);
                }
                if self.shapes[i].kind == Kind::Text {
                    if let Some(c) = var_submenu(ui, "Text variable", self.shapes[i].bound(BindProp::Text), &self.variables, VarKind::Text, VarScope::TextContent) {
                        self.shapes[i].bind(BindProp::Text, c);
                    }
                }
            });
            let name = self.shapes[i].name.clone();
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("Layer name {name}")));
            if resp.double_clicked() {
                ui.data_mut(|d| d.insert_temp(edit_id, true));
            }
        }
        for (k, (g, tip, on, what)) in actions.iter().enumerate() {
            let b = Rect::from_min_size(Pos2::new(xs[k], r.top() + 12.0), Vec2::splat(fig::ROW));
            let resp = if *what == 2 { fig::menu_icon_button(ui, b, *g, tip, *on) } else { fig::icon_button(ui, b, *g, tip, *on) };
            match what {
                0 if resp.clicked() => *act = Some(Act::Component),
                1 if resp.clicked() => *act = Some(Act::Mask),
                3 if resp.clicked() => *act = Some(Act::Flatten),
                4 if resp.clicked() => self.shapes[i].ready ^= true,
                7 if resp.clicked() => {
                    // a lone component first becomes the first variant of a new component set
                    if self.set_of(i).is_none() {
                        self.sel = vec![id];
                        self.combine_as_variants();
                    }
                    if let Some(k) = self.index_of(id) {
                        self.add_variant(k);
                    }
                }
                8 => {
                    egui::Popup::menu(&resp).show(|ui| {
                        if menu_item(ui, Some(Glyph::Instance), "Create instance", "", true) {
                            *act = Some(Act::Instance(id));
                            ui.close();
                        }
                        if menu_item(ui, Some(Glyph::Plus), "Create property", "", true) {
                            self.create_property(i, CompPropKind::Bool);
                            ui.close();
                        }
                        if menu_item(ui, Some(Glyph::Ungroup), "Detach from components", "", true) {
                            self.shapes[i].component = false;
                            ui.close();
                        }
                    });
                }
                6 if resp.clicked() => self.flatten_then_edit(),
                9 => {
                    // one link for the whole text: type the address, Enter applies it (empty removes it)
                    let key = ui.id().with(("text-link", id));
                    let mut url: String = ui.data(|d| d.get_temp(key)).unwrap_or_else(|| self.shapes[i].runs.iter().find_map(|r| r.url.clone()).unwrap_or_default());
                    egui::Popup::menu(&resp).show(|ui| {
                        ui.set_min_width(240.0);
                        let row = fig::row(ui, fig::ROW + 8.0);
                        let f = fig::text_input(ui, Rect::from_min_size(Pos2::new(row.left() + 8.0, row.top() + 4.0), Vec2::new(row.width() - 16.0, fig::ROW)), &mut url, "Link address", "https://");
                        if f.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let n = self.shapes[i].text.chars().count();
                            self.shapes[i].runs.retain(|r| r.url.is_none());
                            if !url.trim().is_empty() && n > 0 {
                                self.shapes[i].runs.push(TextRun { start: 0, end: n, url: Some(url.trim().to_owned()), ..TextRun::default() });
                            }
                            ui.close();
                        }
                    });
                    ui.data_mut(|d| d.insert_temp(key, url));
                }
                10 => {
                    if let Some(c) = crate::variables::var_menu(&resp, self.shapes[i].bound(BindProp::Text), &self.variables, VarKind::Text, VarScope::TextContent, true) {
                        self.shapes[i].bind(BindProp::Text, c);
                    }
                }
                5 => {
                    egui::Popup::menu(&resp).style(crate::glyphs::fig_icon_menu_style).align(egui::RectAlign::BOTTOM_END).gap(4.0).show(|ui| {
                        crate::glyphs::menu_salt(ui, ("more-actions", id));
                        // Figma 126.9.13 (measured 2026-10-18 on a path, text, frame, main component and instance):
                        //   path:       Edit object | Offset vector, Simplify vector
                        //   others:     [Toggle ready for dev status | (instance) Create component, Detach instance, Reset instance, Reset name |]
                        //               Use as mask | Union, Subtract, Intersect, Exclude, Flatten [| (text) Edit object]
                        let inst = self.shapes[i].instance;
                        if is_path && !inst && !main_component {
                            if menu_item(ui, Some(Glyph::Vector), "Edit object", "", true) {
                                self.flatten_then_edit();
                                ui.close();
                            }
                            crate::glyphs::menu_sep(ui);
                            if menu_item(ui, Some(Glyph::Shrink), "Offset vector", "", true) {
                                // the Actions palette takes the distance: "offset path 8"
                                self.open_actions();
                                self.actions_query = "offset path ".into();
                                ui.close();
                            }
                            if menu_item(ui, Some(Glyph::Waves), "Simplify vector", "", true) {
                                let n = self.simplify_vector(2.0);
                                self.status = format!("simplified: {n} points fewer");
                                ui.close();
                            }
                            return;
                        }
                        if inst || main_component {
                            if menu_item(ui, Some(Glyph::Code), "Toggle ready for dev status", "", true) {
                                self.shapes[i].ready ^= true;
                                ui.close();
                            }
                            crate::glyphs::menu_sep(ui);
                        }
                        if inst {
                            if menu_item(ui, Some(Glyph::Component), "Create component", "Ctrl+Alt+K", true) {
                                *act = Some(Act::Component);
                                ui.close();
                            }
                            if menu_item(ui, Some(Glyph::Ungroup), "Detach instance", "Ctrl+Alt+B", true) {
                                *act = Some(Act::Detach);
                                ui.close();
                            }
                            if menu_item(ui, Some(Glyph::Undo), "Reset instance", "", true) {
                                let id = self.shapes[i].id;
                                self.reset_overrides(id);
                                ui.close();
                            }
                            // the layer takes its main component's name again
                            let main_name = self.main_of(i).map(|m| self.shapes[m].name.clone());
                            if menu_item(ui, Some(Glyph::Type), "Reset name", "", main_name.is_some()) {
                                if let Some(n) = main_name {
                                    self.shapes[i].name = n;
                                }
                                ui.close();
                            }
                            crate::glyphs::menu_sep(ui);
                        }
                        if menu_item(ui, Some(Glyph::Mask), "Use as mask", "Ctrl+Alt+M", true) {
                            *act = Some(Act::Mask);
                            ui.close();
                        }
                        crate::glyphs::menu_sep(ui);
                        for (g, label, key, op) in [(Glyph::Union, "Union", "Ctrl+Alt+U", BoolOp::Union), (Glyph::Subtract, "Subtract", "Ctrl+Alt+S", BoolOp::Subtract), (Glyph::Intersect, "Intersect", "Ctrl+Alt+I", BoolOp::Intersect), (Glyph::Exclude, "Exclude", "Ctrl+Alt+X", BoolOp::Exclude)] {
                            if menu_item(ui, Some(g), label, key, self.sel.len() > 1 || bool_group) {
                                *act = Some(Act::Bool(op));
                                ui.close();
                            }
                        }
                        if menu_item(ui, Some(Glyph::Flatten), "Flatten", "Ctrl+E", true) {
                            *act = Some(Act::Flatten);
                            ui.close();
                        }
                        if kind == Kind::Text {
                            crate::glyphs::menu_sep(ui);
                            if menu_item(ui, Some(Glyph::Vector), "Edit object", "", true) {
                                self.flatten_then_edit();
                                ui.close();
                            }
                        }
                    });
                }
                2 => {
                    egui::Popup::menu(&resp).show(|ui| {
                        for (g, label, op) in [(Glyph::Union, "Union selection", BoolOp::Union), (Glyph::Subtract, "Subtract selection", BoolOp::Subtract), (Glyph::Intersect, "Intersect selection", BoolOp::Intersect), (Glyph::Exclude, "Exclude selection", BoolOp::Exclude)] {
                            if menu_item(ui, Some(g), label, "", self.sel.len() > 1 || bool_group) {
                                *act = Some(Act::Bool(op));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if menu_item(ui, Some(Glyph::Flatten), "Flatten", "Ctrl+E", true) {
                            *act = Some(Act::Flatten);
                            ui.close();
                        }
                    });
                }
                _ => {}
            }
        }
        if self.shapes[i].instance {
            let sub = fig::row(ui, 32.0);
            fig::ptext(ui.painter(), Pos2::new(sub.left() + 16.0, sub.center().y), Align2::LEFT_CENTER, "From this file", fig::reg(11.0), TEXT_SECONDARY());
            let w = ui.painter().layout_no_wrap("From this file".into(), fig::reg(11.0), TEXT_SECONDARY()).size().x;
            let b = Rect::from_center_size(Pos2::new(sub.left() + 16.0 + w + 20.0, sub.center().y), Vec2::splat(fig::ROW));
            if fig::icon_button(ui, b, Glyph::Instance, "Go to main component", false).clicked() {
                if let Some(m) = self.shapes[i].src {
                    *act = Some(Act::GoMain(m));
                }
            }
        }
        fig::rule(ui);
    }

    pub(crate) fn single_props(&mut self, ui: &mut egui::Ui, i: usize, act: &mut Option<Act>) {
        let g = Grid::of(ui.max_rect());
        let mut upload: Option<(bool, usize)> = None; // an image paint asked for a file: (is a stroke, index)
        // the layers a Pattern paint can repeat: the top-level ones and the components
        let sources: Vec<(u64, String)> = self.shapes.iter().filter(|s| (s.parent.is_none() || s.component) && s.id != self.shapes[i].id).map(|s| (s.id, s.name.clone())).collect();
        ui.data_mut(|d| d.insert_temp(egui::Id::new("pattern-sources"), sources));
        let parent_layout = self.shapes[i].parent.and_then(|p| self.index_of(p)).map(|p| (self.shapes[p].kind == Kind::Frame, self.shapes[p].layout.is_some()));
        let kind = self.shapes[i].kind;
        let leaf = !kind.is_container() && kind != Kind::Line;
        self.layer_header(ui, i, act);

        self.variants_sections(ui, &g, i);

        // ---- Position
        fig::header(ui, "Position", false, &[]);
        fig::labels(ui, &g, true, "Alignment", "");
        let r = fig::controls(ui, 0.0);
        if let Some(k) = fig::segments(ui, g.col1(r), &[(Glyph::AlignLeft, "Align left", false), (Glyph::AlignHCenter, "Align horizontal centers", false), (Glyph::AlignRight, "Align right", false)]) {
            *act = Some(Act::Align([AlignOp::Left, AlignOp::HCenter, AlignOp::Right][k]));
        }
        if let Some(k) = fig::segments(ui, g.col2(r), &[(Glyph::AlignTop, "Align top", false), (Glyph::AlignVMiddle, "Align vertical centers", false), (Glyph::AlignBottom, "Align bottom", false)]) {
            *act = Some(Act::Align([AlignOp::Top, AlignOp::VCenter, AlignOp::Bottom][k]));
        }
        self.alignment_more(ui, &g, r, act);
        fig::labels(ui, &g, false, "Position", "");
        let r = fig::controls(ui, 0.0);
        let id = self.shapes[i].id;
        let mut new_rot = None;
        {
            let s = &mut self.shapes[i];
            fig::num_field(ui, g.col1(r), ui.id().with(("x", id)), Prefix::Text("X"), &mut s.pos.x, n(2), "X position");
            fig::num_field(ui, g.col2(r), ui.id().with(("y", id)), Prefix::Text("Y"), &mut s.pos.y, n(2), "Y position");
            // constraints apply unless auto layout places the layer
            if parent_layout == Some((true, false)) || (parent_layout == Some((true, true)) && s.abs) {
                fig::labels(ui, &g, false, "Constraints", "");
                let r1 = fig::controls(ui, 0.0);
                fig::dropdown(ui, g.col1(r1), Some(Glyph::FlipH), &mut s.ch, &CONSTRAINTS_H, "Horizontal constraints");
                let r2 = fig::controls(ui, 8.0);
                fig::dropdown(ui, g.col1(r2), Some(Glyph::FlipV), &mut s.cv, &CONSTRAINTS_V, "Vertical constraints");
                let bx = Rect::from_min_max(Pos2::new(g.c2, r1.top()), Pos2::new(g.c2 + g.cw, r2.bottom()));
                constraint_box(ui, bx, &mut s.ch, &mut s.cv);
            }
            // frames rotate too (with their content), unless auto layout places the content
            if leaf || (kind == Kind::Frame && s.layout.is_none() && !s.section) {
                fig::labels(ui, &g, false, "Rotation", "");
                let r = fig::controls(ui, 0.0);
                let before = s.rotation;
                fig::num_field(ui, g.col1(r), ui.id().with(("rot", id)), Prefix::Icon(Glyph::RotateCw), &mut s.rotation, Num { decimals: 1, suffix: "°", ..Num::default() }, "Rotation");
                if s.rotation != before {
                    new_rot = Some(s.rotation);
                    s.rotation = before;
                }
                match fig::segments(ui, g.col2(r), &[(Glyph::RotateCw, "Rotate 90° right", false), (Glyph::FlipH, "Flip horizontal", false), (Glyph::FlipV, "Flip vertical", false)]) {
                    Some(0) => new_rot = Some(s.rotation + 90.0),
                    Some(1) => *act = Some(Act::Flip(true)),
                    Some(2) => *act = Some(Act::Flip(false)),
                    _ => {}
                }
            }
        }
        if let Some(deg) = new_rot {
            self.set_rotation(i, deg);
        }
        if kind == Kind::Line {
            // a line is stored as a vector; its rotation is the angle of that vector, counter-clockwise like Figma's
            fig::labels(ui, &g, false, "Rotation", "");
            let r = fig::controls(ui, 0.0);
            let s = &mut self.shapes[i];
            let len = s.size.length();
            let before = -s.size.y.atan2(s.size.x).to_degrees();
            let mut deg = before;
            fig::num_field(ui, g.col1(r), ui.id().with(("line-rot", id)), Prefix::Icon(Glyph::RotateCw), &mut deg, Num { decimals: 2, suffix: "°", ..Num::default() }, "Rotation");
            let mut target = (deg != before).then_some(deg);
            match fig::segments(ui, g.col2(r), &[(Glyph::RotateCw, "Rotate 90° right", false), (Glyph::FlipH, "Flip horizontal", false), (Glyph::FlipV, "Flip vertical", false)]) {
                Some(0) => target = Some(before - 90.0),
                Some(1) => *act = Some(Act::Flip(true)),
                Some(2) => *act = Some(Act::Flip(false)),
                _ => {}
            }
            if let Some(d) = target {
                let c = s.pos + s.size / 2.0;
                let v = Vec2::angled(-d.to_radians()) * len;
                s.pos = c - v / 2.0;
                s.size = v;
            }
        }
        fig::end(ui);

        // ---- Layout
        if self.shapes[i].section {
            self.section_layout(ui, &g, i, act);
        } else if kind == Kind::Group {
            self.group_layout(ui, &g);
        } else if kind == Kind::Text && parent_layout != Some((true, true)) {
            self.text_layout(ui, &g, i);
        } else if kind == Kind::Line {
            self.line_layout(ui, &g, i);
        } else {
            self.layout_section(ui, &g, i, parent_layout, act);
        }

        // ---- Appearance
        self.slot_section(ui, &g, i);
        self.code_section(ui, &g, i);
        self.appearance_section(ui, &g, i);
        self.repeat_section(ui, &g, i);
        self.modes_section(ui, &g, i);

        // ---- kind-specific
        if kind == Kind::Text {
            self.typography_section(ui, &g, i);
            self.spelling_section(ui, &g, i);
        }
        if kind == Kind::Path && self.node_edit {
            self.vertex_section(ui, &g);
        }
        if kind != Kind::Group || self.shapes[i].bool_op.is_some() {
            if let Some(u) = self.paint_sections(ui, &g, i, act) {
                upload = Some(u);
            }
        } else {
            // Figma shows a group's Fill, Stroke and Effects as those of its layers; editing writes to all of them
            let gid = self.shapes[i].id;
            let leaves: Vec<u64> = self.shapes.iter().filter(|s| s.id != gid && !s.kind.is_container() && self.is_inside(s.id, gid)).map(|s| s.id).collect();
            self.look_sections(ui, &g, act, leaves, false);
        }
        if let Some((is_stroke, k)) = upload {
            self.pick_image_for_paint(i, is_stroke, k);
        }
        // Figma's order after Effects: Selection colors, Layout guide, Export
        if kind.is_container() {
            self.selection_colors_section(ui);
        }
        if kind == Kind::Frame && !self.shapes[i].section {
            self.guides_section(ui, &g, i);
        }

        // ---- Export
        self.export_settings(ui, i);
    }

    // Fill, Stroke and Effects of layer `i`; returns the paint that asked for an image file: (is a stroke, index)
    fn paint_sections(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize, act: &mut Option<Act>) -> Option<(bool, usize)> {
        let styles = self.styles.clone();
        let images = self.images.clone();
        let textures = self.tex.clone();
        let variables = self.variables.clone();
        let mut upload: Option<(bool, usize)> = None;
        let mut upload_fill = false;
        let kind = self.shapes[i].kind;
        let id = self.shapes[i].id;
        // ---- Fill
        let default = if kind == Kind::Frame { Color32::WHITE } else { GREY_100 };
        let s = &mut self.shapes[i];
        let has = !s.fills.is_empty();
        let (head, b) = fig::header(ui, "Fill", !has, if has { &[(Glyph::Styles, "Apply styles", false), (Glyph::Plus, "Add fill", false)] } else { &[(Glyph::Plus, "Add fill", false)] });
        if b.last().is_some_and(|b| b.clicked()) || (!has && head.clicked()) {
            s.fills.push(Paint::solid(default));
        }
        if has && b[0].clicked() {
            // the style menu sits on the first fill's swatch
        }
        let mut remove = None;
        let n_fills = s.fills.len();
        for k in (0..n_fills).rev() {
            let r = fig::controls(ui, if k + 1 == n_fills { 2.0 } else { 8.0 });
            let (rm, st) = paint_row(ui, g, r, &mut s.fills[k], ("Fill", k), &styles, &variables, &images, &textures, &mut upload_fill);
            if upload_fill {
                upload = Some((false, k));
                upload_fill = false;
            }
            if rm {
                remove = Some(k);
            }
            if let Some(c) = st {
                *act = Some(Act::NewStyle(c));
            }
        }
        if let Some(k) = remove {
            s.fills.remove(k);
        }
        if has && s.kind == Kind::Frame {
            let r = fig::controls(ui, 8.0);
            let mut show = !s.no_bg_export;
            if fig::checkbox(ui, g.col1(r), &mut show, "Show in exports") {
                s.no_bg_export = !show;
            }
        }
        if has { fig::end(ui) } else { fig::rule(ui) }

        // ---- Stroke
        let has = !s.strokes.is_empty();
        let (head, b) = fig::header(ui, "Stroke", !has, if has { &[(Glyph::Styles, "Apply styles", false), (Glyph::Plus, "Add stroke", false)] } else { &[(Glyph::Plus, "Add stroke", false)] });
        if b.last().is_some_and(|b| b.clicked()) || (!has && head.clicked()) {
            if s.strokes.is_empty() {
                // Figma: a first stroke sits inside closed shapes and is centred on lines and open paths
                s.stroke_align = if matches!(kind, Kind::Rect | Kind::Frame | Kind::Ellipse | Kind::Polygon | Kind::Star) { StrokeAlign::Inside } else { StrokeAlign::Center };
                if s.stroke_w <= 0.0 {
                    s.stroke_w = 1.0;
                }
            }
            s.strokes.push(Paint::solid(Color32::BLACK));
        }
        let mut remove = None;
        let n_strokes = s.strokes.len();
        for k in (0..n_strokes).rev() {
            let r = fig::controls(ui, if k + 1 == n_strokes { 2.0 } else { 8.0 });
            let (rm, st) = paint_row(ui, g, r, &mut s.strokes[k], ("Stroke", k), &styles, &variables, &images, &textures, &mut upload_fill);
            if upload_fill {
                upload = Some((true, k));
                upload_fill = false;
            }
            if rm {
                remove = Some(k);
            }
            if let Some(c) = st {
                *act = Some(Act::NewStyle(c));
            }
        }
        if let Some(k) = remove {
            s.strokes.remove(k);
        }
        if s.strokes.is_empty() {
            fig::rule(ui);
        } else {
            crate::strokefx::stroke_controls(ui, &g, s, id, &self.variables);
            fig::end(ui);
        }

        if s.section {
            return upload; // a section has no effects
        }
        // ---- Effects
        let has = !s.effects.is_empty();
        let styled = s.effect_style.and_then(|sid| self.ext.effects.iter().find(|e| e.id == sid)).map(|e| e.name.clone());
        let show_styles = has || styled.is_some() || !self.ext.effects.is_empty();
        let (head, b) = fig::header(ui, "Effects", !has && styled.is_none(), if show_styles { &[(Glyph::Styles, "Apply styles", false), (Glyph::Plus, "Add effect", false)] } else { &[(Glyph::Plus, "Add effect", false)] });
        if show_styles {
            let mut create = false;
            if let Some(sid) = style_menu(&b[0], self.ext.effects.iter().map(|e| (e.id, e.name.as_str())), has, "Create effect style", &mut create) {
                s.effect_style = Some(sid);
            }
            if create {
                let nid = self.next_id;
                self.next_id += 1;
                self.ext.effects.push(EffectStyle { id: nid, name: format!("Effect {}", self.ext.effects.len() + 1), effects: s.effects.clone() });
                s.effect_style = Some(nid);
            }
        }
        if let Some(name) = styled {
            // a layer that follows a style shows its name instead of the effects
            if style_row(ui, g, &name, "Detach effect style") {
                s.effect_style = None;
            }
            fig::end(ui);
            return upload;
        }
        // "+" and the empty section's title both open the menu of effect types
        let mut added = b.last().and_then(crate::strokefx::effect_menu);
        if !has {
            added = added.or_else(|| crate::strokefx::effect_menu(&head));
        }
        let mut drop = None;
        let n_fx = s.effects.len();
        for k in (0..n_fx).rev() {
            let r = fig::controls(ui, if k + 1 == n_fx { 2.0 } else { 8.0 });
            if crate::strokefx::effect_row(ui, g, r, &mut s.effects[k], ("fx", k, id), &self.variables) {
                drop = Some(k);
            }
        }
        if let Some(k) = drop {
            s.effects.remove(k);
        }
        if let Some((k, d)) = crate::strokefx::take_effect_move(ui, id) {
            let to = k as i32 + d;
            if k < s.effects.len() && to >= 0 && (to as usize) < s.effects.len() {
                s.effects.swap(k, to as usize);
            }
        }
        if let Some(kind) = added {
            s.effects.push(Effect::new(kind));
        }
        // Shader effects: their own list under the effects, and the library window the Add effect > Shader item opens
        let lib_id = egui::Id::new(("shader-library-open", id));
        let mut lib_open = ui.data(|d| d.get_temp::<bool>(lib_id)).unwrap_or(false);
        if ui.data_mut(|d| d.remove_temp::<bool>(egui::Id::new("shader-library-request"))).unwrap_or(false) {
            lib_open = true;
        }
        if let Some(k) = crate::shader_fx::library(ui.ctx(), ui.max_rect().left(), &mut lib_open) {
            s.shaders.push(ShaderEffect::new(k));
        }
        ui.data_mut(|d| d.insert_temp(lib_id, lib_open));
        if !s.shaders.is_empty() {
            let r = fig::row(ui, 28.0);
            fig::ptext(ui.painter(), Pos2::new(g.left, r.center().y), Align2::LEFT_CENTER, "Shader effects", fig::reg(11.0), TEXT_SECONDARY());
            let mut gone = None;
            let n_sh = s.shaders.len();
            for k in (0..n_sh).rev() {
                let r = fig::controls(ui, if k + 1 == n_sh { 2.0 } else { 8.0 });
                if crate::shader_fx::shader_row(ui, g, r, &mut s.shaders[k], (k, id)) {
                    gone = Some(k);
                }
            }
            if let Some(k) = gone {
                s.shaders.remove(k);
            }
        }
        if s.effects.is_empty() && s.shaders.is_empty() { fig::rule(ui) } else { fig::end(ui) }
        upload
    }

    fn component_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize, act: &mut Option<Act>) {
        let (id, is_main) = (self.shapes[i].id, self.shapes[i].component);
        let main = self.shapes[i].src.and_then(|m| self.index_of(m)).map(|m| (self.shapes[m].id, self.shapes[m].name.clone()));
        fig::header(ui, if is_main { "Main component" } else { "Instance" }, false, &[]);
        let r = fig::controls(ui, 4.0);
        if is_main {
            if fig::button(ui, g.both(r), "Create instance").clicked() {
                *act = Some(Act::Instance(id));
            }
            let r = fig::controls(ui, 8.0);
            fig::text_input(ui, g.both(r), &mut self.shapes[i].desc, "Component description", "Description");
            let r = fig::controls(ui, 8.0);
            fig::text_input(ui, g.both(r), &mut self.shapes[i].doc_url, "Component documentation link", "Documentation link");
        } else {
            // what the main component says about itself
            let (desc, url) = self.main_of(i).map(|m| (self.shapes[m].desc.clone(), self.shapes[m].doc_url.clone())).unwrap_or_default();
            if !desc.is_empty() {
                let r = fig::controls(ui, 4.0);
                fig::ptext(ui.painter(), Pos2::new(g.both(r).left(), r.center().y), Align2::LEFT_CENTER, &desc, fig::reg(11.0), TEXT_SECONDARY());
            }
            if !url.is_empty() {
                let r = fig::controls(ui, 4.0);
                if fig::button(ui, g.both(r), "Open documentation").clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            }
            let comps = self.components();
            let mut pick: Option<u64> = None;
            let mut items: Vec<(Option<u64>, &str)> = vec![(None, main.as_ref().map_or("Component", |m| m.1.as_str()))];
            items.extend(comps.iter().map(|(c, name)| (Some(*c), name.as_str())));
            if fig::dropdown(ui, g.both(r), Some(Glyph::Component), &mut pick, &items, "Swap instance") {
                if let Some(c) = pick {
                    *act = Some(Act::Swap(c));
                }
            }
            if fig::icon_button(ui, g.icon(r), Glyph::Ungroup, "Detach instance", false).clicked() {
                *act = Some(Act::Detach);
            }
            if let Some((m, _)) = main {
                let r = fig::controls(ui, 8.0);
                if fig::button(ui, g.both(r), "Go to main component").clicked() {
                    *act = Some(Act::GoMain(m));
                }
            }
        }
        fig::end(ui);
    }

// Hug / Fixed / Fill as the right end of a W or H field: the current word, right-aligned, opening a menu
fn inline_menu<T: PartialEq + Copy>(ui: &mut egui::Ui, r: Rect, cur: &mut T, items: &[(T, &str)], name: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(("inline-menu", name, r.min.x as i32)), Sense::click());
    let label = items.iter().find(|(v, _)| v == cur).map_or("", |(_, l)| *l);
    fig::ptext(ui.painter(), Pos2::new(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, label, fig::reg(11.0), if resp.hovered() { fig::SELECTED_INK() } else { TEXT() });
    resp.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, name);
        info.current_text_value = Some(label.to_owned());
        info
    });
    let mut changed = false;
    egui::Popup::menu(&resp).show(|ui| {
        for (v, l) in items {
            if crate::glyphs::menu_option(ui, false, l) {
                changed = *cur != *v;
                *cur = *v;
                ui.close();
            }
        }
    });
    changed
}

    fn layout_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize, parent_layout: Option<(bool, bool)>, act: &mut Option<Act>) {
        let id = self.shapes[i].id;
        let parent_cols = self.shapes[i].parent.and_then(|p| self.index_of(p)).and_then(|p| self.shapes[p].layout.as_ref()).map_or(0, |l| l.columns);
        // a frame inside a main component (not the component itself, not inside an instance) can be a slot
        let in_component = !self.shapes[i].component && self.chain(i).into_iter().skip(1).any(|k| self.shapes[k].component) && !self.chain(i).into_iter().any(|k| self.shapes[k].instance);
        let s = &mut self.shapes[i];
        let kind = s.kind;
        // Figma 126: a frame with auto layout has an "Auto layout" section (its toggle highlighted at the right):
        // Flow with the wrap button, Resizing with Hug / Fixed / Fill inside the W and H fields, Alignment and
        // Gap, Padding in two rows. Without auto layout it is the plain "Layout" section.
        let is_al = kind == Kind::Frame && s.layout.is_some();
        // Figma 126.9.13 names these two header buttons "Resize to fit" and "Toggle auto layout"
        let plain_instance = s.instance && s.layout.is_none();
        let actions: Vec<(Glyph, &str, bool)> = if kind == Kind::Frame && !plain_instance { vec![(Glyph::Shrink, "Resize to fit", false), (Glyph::AutoLayout, "Toggle auto layout", is_al)] } else { vec![] };
        let (_, head) = fig::header(ui, if is_al { "Auto layout" } else { "Layout" }, false, &actions);
        if !head.is_empty() && head[0].clicked() {
            *act = Some(Act::ResizeToFit);
        }
        if head.len() > 1 && head[1].clicked() {
            *act = Some(Act::AutoLayout);
        }
        let mut first = true;
        if kind == Kind::Frame && !plain_instance {
            // flow: none, vertical, horizontal, grid across both columns; wrap in the icon column
            let flow = match &s.layout {
                None => 0,
                Some(l) if l.columns > 0 => 3,
                Some(l) if l.vertical => 1,
                Some(_) => 2,
            };
            fig::labels(ui, g, true, "Flow", "");
            first = false;
            let r = fig::controls(ui, 0.0);
            let pick = fig::segments(ui, g.both(r), &[(Glyph::Frame, "Freeform", flow == 0), (Glyph::Rows, "Vertical", flow == 1), (Glyph::AutoLayout, "Horizontal", flow == 2), (Glyph::Grid, "Grid", flow == 3)]);
            if let Some(want) = pick.filter(|&w| w != flow) {
                match (flow, want) {
                    (_, 0) => s.layout = None,
                    (0, 3) => *act = Some(Act::AutoLayoutGrid),
                    (0, _) => *act = Some(Act::AutoLayout),
                    (_, w) => {
                        if let Some(l) = &mut s.layout {
                            l.columns = if w == 3 { l.columns.max(2) } else { 0 };
                            if w != 3 {
                                l.vertical = w == 1;
                            }
                        }
                    }
                }
            }
            if let Some(l) = s.layout.as_mut().filter(|l| l.columns == 0) {
                if fig::icon_button(ui, g.icon(r), Glyph::Wrap, "Wrap", l.wrap).clicked() {
                    l.wrap ^= true;
                }
            }
        }
        let can_hug = kind == Kind::Text || (kind == Kind::Frame && s.layout.is_some());
        let can_fill = matches!(parent_layout, Some((true, true)));
        let sizing = can_hug || can_fill;
        fig::labels(ui, g, first, if sizing { "Resizing" } else { "Dimensions" }, "");
        let r = fig::controls(ui, 0.0);
        fig::num_field(ui, g.col1(r), ui.id().with(("w", id)), Prefix::Text("W"), &mut s.size.x, n(2), "Width");
        fig::num_field(ui, g.col2(r), ui.id().with(("h", id)), Prefix::Text("H"), &mut s.size.y, n(2), "Height");
        for (prop, rr, tag) in [(BindProp::Width, g.col1(r), "w"), (BindProp::Height, g.col2(r), "h")] {
            if let Some(c) = var_field(ui, rr, ui.id().with(("bind", tag, id)), s.bound(prop), &self.variables, VarKind::Number, VarScope::Size) {
                s.bind(prop, c);
            }
        }
        if sizing {
            let mut items = vec![(Sizing::Fixed, "Fixed")];
            if can_hug {
                items.push((Sizing::Hug, "Hug"));
            }
            if can_fill {
                items.push((Sizing::Fill, "Fill"));
            }
            for (horizontal, field, name) in [(true, g.col1(r), "Horizontal resizing"), (false, g.col2(r), "Vertical resizing")] {
                let mut z = sizing_get(s, horizontal);
                let menu = Rect::from_min_max(Pos2::new(field.right() - 56.0, field.top()), field.max);
                if Self::inline_menu(ui, menu, &mut z, &items, name) {
                    sizing_set(s, horizontal, z);
                }
            }
        }
        if fig::icon_button(ui, g.icon(r), Glyph::Ratio, "Lock aspect ratio", s.lock_aspect).clicked() {
            s.lock_aspect ^= true;
        }
        // limits for a layer that hugs or fills (0 = none)
        let hugs_or_fills = [true, false].iter().any(|&h| sizing_get(s, h) != Sizing::Fixed);
        if hugs_or_fills && !s.abs {
            let lim = Num { min: 0.0, decimals: 0, ..Num::default() };
            for (label, is_max) in [("Min size", false), ("Max size", true)] {
                fig::labels(ui, g, false, label, "");
                let r = fig::controls(ui, 0.0);
                let v = if is_max { &mut s.max_size } else { &mut s.min_size };
                let (wn, hn) = if is_max { ("Maximum width", "Maximum height") } else { ("Minimum width", "Minimum height") };
                fig::num_field(ui, g.col1(r), ui.id().with(("limw", id, is_max)), Prefix::Text("W"), &mut v.x, lim, wn);
                fig::num_field(ui, g.col2(r), ui.id().with(("limh", id, is_max)), Prefix::Text("H"), &mut v.y, lim, hn);
            }
        }
        if matches!(parent_layout, Some((true, true))) {
            let r = fig::controls(ui, 8.0);
            fig::checkbox(ui, g.col1(r), &mut s.abs, "Ignore auto layout");
        }
        if in_component && kind == Kind::Frame {
            let r = fig::controls(ui, 8.0);
            fig::checkbox(ui, g.both(r), &mut s.slot, "Slot: instances can change its content");
        }
        if parent_cols > 0 && !s.abs {
            // a cell of a grid can cover more than one column and row
            fig::labels(ui, g, false, "Column span", "Row span");
            let r = fig::controls(ui, 0.0);
            let num = Num { min: 1.0, max: 24.0, decimals: 0, speed: 0.1, suffix: "" };
            for (k, f, name) in [(0, g.col1(r), "Column span"), (1, g.col2(r), "Row span")] {
                let mut v = s.span[k] as f32;
                if fig::num_field(ui, f, ui.id().with(("span", id, k)), Prefix::Icon(Glyph::Grid), &mut v, num, name) {
                    s.span[k] = v.round().max(1.0) as u32;
                }
            }
        }
        let s_bindings = s.bindings.clone();
        let mut bind_change: Option<(BindProp, Option<u64>)> = None;
        if let Some(l) = &mut s.layout {
            fig::labels(ui, g, false, "Alignment", "Gap");
            let r = fig::controls(ui, 0.0);
            let bx = Rect::from_min_size(Pos2::new(g.left, r.top()), Vec2::new(g.cw, 56.0));
            alignment_box(ui, bx, l);
            let auto = l.main == Align::SpaceBetween;
            if auto {
                fig::input_bg(ui, g.col2(r), false);
                paint_glyph_sized(ui.painter(), Pos2::new(g.c2 + 12.0, r.center().y), Glyph::DistH, TEXT_SECONDARY(), 13.0);
                fig::text(ui, Pos2::new(g.c2 + 24.0, r.center().y), Align2::LEFT_CENTER, "Auto", fig::reg(11.0), TEXT());
            } else {
                fig::num_field(ui, g.col2(r), ui.id().with(("gap", id)), Prefix::Icon(if l.vertical { Glyph::DistV } else { Glyph::DistH }), &mut l.gap, Num { min: 0.0, decimals: 1, ..Num::default() }, "Gap");
                if let Some(c) = var_field(ui, g.col2(r), ui.id().with(("bind", "gap", id)), s_bindings.iter().find(|(p, _)| *p == BindProp::Gap).map(|(_, v)| *v), &self.variables, VarKind::Number, VarScope::Spacing) {
                    bind_change = Some((BindProp::Gap, c));
                }
            }
            // the chevron in the Gap field: a number or Auto (space between)
            let chev = Rect::from_min_size(Pos2::new(g.c2 + g.cw - 24.0, r.top()), Vec2::new(24.0, fig::ROW));
            let cresp = ui.interact(chev, ui.id().with(("gap-menu", id)), Sense::click());
            paint_glyph_sized(ui.painter(), chev.center(), Glyph::ChevronDown, TEXT_SECONDARY(), 9.5);
            cresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Gap mode"));
            egui::Popup::menu(&cresp).show(|ui| {
                for (is_auto, label) in [(false, "Number"), (true, "Auto")] {
                    if menu_item(ui, None, label, "", true) {
                        l.main = if is_auto { Align::SpaceBetween } else if auto { Align::Start } else { l.main };
                        ui.close();
                    }
                }
            });
            let sid = ui.id().with(("layout-settings-open", id));
            let sopen = ui.data(|d| d.get_temp::<bool>(sid)).unwrap_or(false);
            let sresp = fig::icon_button(ui, g.icon(r), Glyph::Sliders, "Auto layout settings", sopen);
            let shown = crate::picker::popup(&sresp, ui.max_rect().left(), |ui| layout_settings(ui, l));
            ui.data_mut(|d| d.insert_temp(sid, shown.is_some()));
            if l.columns > 0 {
                let r2 = Rect::from_min_size(Pos2::new(g.c2, r.top() + 32.0), Vec2::new(g.cw, fig::ROW));
                let mut cols = l.columns as f32;
                if fig::num_field(ui, r2, ui.id().with(("cols", id)), Prefix::Icon(Glyph::Grid), &mut cols, Num { min: 1.0, max: 24.0, decimals: 0, speed: 0.1, suffix: "" }, "Columns") {
                    l.columns = cols.round().max(1.0) as u32;
                }
            }
            fig::row(ui, 32.0); // the rest of the 56px alignment box
            if l.columns > 0 {
                // fixed sizes of the columns and rows ("120, 0, 80": 0 or missing = as big as the content)
                fig::labels(ui, g, false, "Column widths", "Row heights");
                let r = fig::controls(ui, 0.0);
                for (f, sizes, name, tag) in [(g.col1(r), &mut l.col_sizes, "Column widths", "cw"), (g.col2(r), &mut l.row_sizes, "Row heights", "rh")] {
                    let key = ui.id().with((tag, id));
                    let mut txt: String = ui.data(|d| d.get_temp(key)).unwrap_or_else(|| sizes.iter().map(|v| fig_num(*v)).collect::<Vec<_>>().join(", "));
                    let resp = fig::text_input(ui, f, &mut txt, name, "auto");
                    if resp.changed() {
                        *sizes = txt.split(',').map(|t| t.trim().parse::<f32>().unwrap_or(0.0).max(0.0)).collect();
                        while sizes.last() == Some(&0.0) {
                            sizes.pop();
                        }
                        ui.data_mut(|d| d.insert_temp(key, txt));
                    } else if !resp.has_focus() {
                        ui.data_mut(|d| d.remove::<String>(key));
                    }
                }
                fig::row(ui, 8.0);
            }
            fig::labels(ui, g, false, "Padding", "");
            let r = fig::controls(ui, 0.0);
            let individual_id = ui.id().with(("individual-padding", id));
            let individual = ui.data(|d| d.get_temp::<bool>(individual_id)).unwrap_or(l.pad[0] != l.pad[2] || l.pad[1] != l.pad[3]);
            let num = Num { min: 0.0, decimals: 1, ..Num::default() };
            if individual {
                // two rows: left and top, then right and bottom
                let order = [(0, "Left padding", Glyph::PanelLeft), (1, "Top padding", Glyph::PanelTop), (2, "Right padding", Glyph::PanelRight), (3, "Bottom padding", Glyph::PanelBottom)];
                let second = fig::controls(ui, 8.0);
                for (n_in_row, (k, name, glyph)) in order.into_iter().enumerate() {
                    let row = if n_in_row < 2 { r } else { second };
                    let field = if n_in_row % 2 == 0 { g.col1(row) } else { g.col2(row) };
                    fig::num_field(ui, field, ui.id().with(("pad", id, k)), Prefix::Icon(glyph), &mut l.pad[k], num, name);
                }
            } else {
                let (mut ph, mut pv) = (l.pad[0], l.pad[1]);
                if fig::num_field(ui, g.col1(r), ui.id().with(("ph", id)), Prefix::Icon(Glyph::FlipH), &mut ph, num, "Horizontal padding") {
                    l.pad[0] = ph;
                    l.pad[2] = ph;
                }
                if fig::num_field(ui, g.col2(r), ui.id().with(("pv", id)), Prefix::Icon(Glyph::FlipV), &mut pv, num, "Vertical padding") {
                    l.pad[1] = pv;
                    l.pad[3] = pv;
                }
                for (prop, f, tag) in [(BindProp::PadX, g.col1(r), "px"), (BindProp::PadY, g.col2(r), "py")] {
                    if let Some(c) = var_field(ui, f, ui.id().with(("bind", tag, id)), s_bindings.iter().find(|(p, _)| *p == prop).map(|(_, v)| *v), &self.variables, VarKind::Number, VarScope::Spacing) {
                        bind_change = Some((prop, c));
                    }
                }
            }
            if fig::icon_button(ui, g.icon(r), Glyph::Scan, "Individual padding", individual).clicked() {
                ui.data_mut(|d| d.insert_temp(individual_id, !individual));
            }
        }
        if let Some((prop, var)) = bind_change {
            s.bind(prop, var);
        }
        if kind == Kind::Frame {
            let r = fig::controls(ui, 8.0);
            fig::checkbox(ui, g.col1(r), &mut s.clip, "Clip content");
        }
        fig::end(ui);
    }

    // a frame or group chooses the mode of each collection that has more than one; layers inside inherit it
    fn modes_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        if !self.shapes[i].kind.is_container() || !self.variables.iter().any(|c| c.modes.len() > 1) {
            return;
        }
        fig::header(ui, "Variable modes", false, &[]);
        let colls: Vec<(u64, String, Vec<(u64, String)>)> = self.variables.iter().filter(|c| c.modes.len() > 1).map(|c| (c.id, c.name.clone(), c.modes.iter().map(|m| (m.id, m.name.clone())).collect())).collect();
        for (k, (cid, name, modes)) in colls.iter().enumerate() {
            let r = fig::controls(ui, if k == 0 { 4.0 } else { 8.0 });
            fig::ptext(ui.painter(), Pos2::new(g.left, r.center().y), Align2::LEFT_CENTER, name, fig::reg(11.0), TEXT_SECONDARY());
            let mut cur = self.shapes[i].modes.iter().find(|(c, _)| c == cid).map_or(0, |(_, m)| *m);
            let mut items: Vec<(u64, &str)> = vec![(0, "Auto")];
            items.extend(modes.iter().map(|(id, n)| (*id, n.as_str())));
            if fig::dropdown(ui, g.col2(r), None, &mut cur, &items, &format!("Mode of {name}")) {
                let s = &mut self.shapes[i];
                s.modes.retain(|(c, _)| c != cid);
                if cur != 0 {
                    s.modes.push((*cid, cur));
                }
            }
        }
        fig::end(ui);
    }

    fn appearance_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        let id = self.shapes[i].id;
        let s = &mut self.shapes[i];
        let (_, b) = fig::header(ui, "Appearance", false, &[(if s.visible { Glyph::Eye } else { Glyph::EyeOff }, "Toggle visibility", !s.visible), (Glyph::Droplet, "Blend mode", s.blend != BlendMode::Normal)]);
        if b[0].clicked() {
            s.visible ^= true;
        }
        egui::Popup::menu(&b[1]).show(|ui| {
            for m in BlendMode::ALL {
                if crate::glyphs::menu_option(ui, false, m.label()) {
                    s.blend = m;
                    ui.close();
                }
            }
        });
        let corners = matches!(s.kind, Kind::Rect | Kind::Frame);
        // an ellipse shows Corner radius too, greyed out (Figma 126.9.13)
        let disabled_radius = matches!(s.kind, Kind::Ellipse | Kind::Text | Kind::Line);
        fig::labels(ui, g, true, "Opacity", if corners || disabled_radius { "Corner radius" } else { "" });
        let r = fig::controls(ui, 0.0);
        if disabled_radius {
            fig::input_bg(ui, g.col2(r), false);
            paint_glyph_sized(ui.painter(), Pos2::new(g.c2 + 16.0, r.center().y), Glyph::Radius, TEXT_TERTIARY(), 13.0);
            fig::text(ui, Pos2::new(g.c2 + 32.0, r.center().y), Align2::LEFT_CENTER, "0", fig::reg(11.0), TEXT_TERTIARY());
        }
        let mut pct = s.opacity * 100.0;
        if fig::num_field(ui, g.col1(r), ui.id().with(("opacity", id)), Prefix::Icon(Glyph::Contrast), &mut pct, Num { min: 0.0, max: 100.0, decimals: 0, suffix: "%", speed: 1.0 }, "Opacity") {
            s.opacity = pct / 100.0;
        }
        if let Some(c) = var_field(ui, g.col1(r), ui.id().with(("bind", "opacity", id)), s.bound(BindProp::Opacity), &self.variables, VarKind::Number, VarScope::Opacity) {
            s.bind(BindProp::Opacity, c);
        }
        if corners {
            let indep_id = ui.id().with(("independent-corners", id));
            let indep = ui.data(|d| d.get_temp::<bool>(indep_id)).unwrap_or(s.radius.iter().any(|&x| x != s.radius[0]));
            let mut r0 = s.radius[0];
            if !indep && fig::num_field(ui, g.col2(r), ui.id().with(("radius", id)), Prefix::Icon(Glyph::Radius), &mut r0, Num { min: 0.0, decimals: 1, ..Num::default() }, "Corner radius") {
                s.radius = [r0; 4];
            }
            if !indep {
                if let Some(c) = var_field(ui, g.col2(r), ui.id().with(("bind", "radius", id)), s.bound(BindProp::Radius), &self.variables, VarKind::Number, VarScope::Radius) {
                    s.bind(BindProp::Radius, c);
                }
            }
            if indep {
                fig::input_bg(ui, g.col2(r), false);
                fig::text(ui, Pos2::new(g.c2 + 8.0, r.center().y), Align2::LEFT_CENTER, "Mixed", fig::reg(11.0), TEXT_SECONDARY());
            }
            if fig::icon_button(ui, g.icon(r), Glyph::Scan, "Independent corners", indep).clicked() {
                ui.data_mut(|d| d.insert_temp(indep_id, !indep));
            }
            // Corner smoothing: 0% is a round corner, 60% is the iOS app icon
            if s.radius.iter().any(|&x| x > 0.0) || s.corner_smooth > 0.0 {
                fig::labels(ui, g, false, "Corner smoothing", "");
                let r = fig::controls(ui, 0.0);
                let mut pct = s.corner_smooth * 100.0;
                if fig::num_field(ui, g.col1(r), ui.id().with(("smooth", id)), Prefix::Icon(Glyph::Radius), &mut pct, Num { min: 0.0, max: 100.0, decimals: 0, suffix: "%", speed: 1.0 }, "Corner smoothing") {
                    s.corner_smooth = pct / 100.0;
                }
            }
            if indep {
                let r = fig::controls(ui, 8.0);
                let w = (g.cw * 2.0 + 8.0 - 3.0 * 4.0) / 4.0;
                for (k, name) in ["Top left radius", "Top right radius", "Bottom right radius", "Bottom left radius"].iter().enumerate() {
                    let rr = Rect::from_min_size(Pos2::new(g.left + (w + 4.0) * k as f32, r.top()), Vec2::new(w, fig::ROW));
                    fig::num_field(ui, rr, ui.id().with(("rad", id, k)), Prefix::None, &mut s.radius[k], Num { min: 0.0, decimals: 1, ..Num::default() }, name);
                }
            }
        }
        match s.kind {
            Kind::Ellipse => {
                fig::labels(ui, g, false, "Arc", "");
                let r = fig::controls(ui, 0.0);
                let w = (g.cw * 2.0 + 8.0 - 2.0 * 4.0) / 3.0;
                let cell = |k: usize| Rect::from_min_size(Pos2::new(g.left + (w + 4.0) * k as f32, r.top()), Vec2::new(w, fig::ROW));
                fig::num_field(ui, cell(0), ui.id().with(("arc0", id)), Prefix::Icon(Glyph::RotateCw), &mut s.arc[0], Num { decimals: 1, suffix: "°", ..Num::default() }, "Arc start");
                let mut sweep = s.arc[1] / 360.0 * 100.0;
                if fig::num_field(ui, cell(1), ui.id().with(("arc1", id)), Prefix::None, &mut sweep, Num { min: -100.0, max: 100.0, decimals: 1, suffix: "%", speed: 0.5 }, "Arc sweep") {
                    s.arc[1] = sweep / 100.0 * 360.0;
                }
                let mut ratio = s.inner * 100.0;
                if fig::num_field(ui, cell(2), ui.id().with(("arc2", id)), Prefix::None, &mut ratio, Num { min: 0.0, max: 99.0, decimals: 1, suffix: "%", speed: 0.5 }, "Arc ratio") {
                    s.inner = ratio / 100.0;
                }
            }
            Kind::Polygon | Kind::Star => {
                let star = s.kind == Kind::Star;
                fig::labels(ui, g, false, "Count", if star { "Ratio" } else { "" });
                let r = fig::controls(ui, 0.0);
                let mut count = s.sides as f32;
                if fig::num_field(ui, g.col1(r), ui.id().with(("count", id)), Prefix::Icon(if star { Glyph::Star } else { Glyph::Polygon }), &mut count, Num { min: 3.0, max: 64.0, decimals: 0, speed: 0.1, suffix: "" }, "Count") {
                    s.sides = count.round().clamp(3.0, 64.0) as u32;
                }
                if star {
                    let mut ratio = s.inner * 100.0;
                    if fig::num_field(ui, g.col2(r), ui.id().with(("ratio", id)), Prefix::None, &mut ratio, Num { min: 5.0, max: 100.0, decimals: 1, suffix: "%", speed: 0.5 }, "Ratio") {
                        s.inner = ratio / 100.0;
                    }
                }
            }
            _ => {}
        }
        if s.mask {
            // what of this layer's picture the layers above it are cut by
            fig::labels(ui, g, false, "Mask type", "");
            let r = fig::controls(ui, 0.0);
            let kinds = ["Vector", "Alpha", "Luminance"];
            let tips: Vec<(&str, bool)> = kinds.iter().enumerate().map(|(k, n)| (*n, s.mask_kind as usize == k)).collect();
            if let Some(k) = fig::segments_custom(ui, g.both(r), &tips, |p, c, k, ink| { fig::ptext(p, c, Align2::CENTER_CENTER, kinds[k], fig::reg(11.0), ink); }) {
                s.mask_kind = k as u8;
            }
        }
        fig::end(ui);
    }

    fn typography_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        let used: Vec<String> = self.shapes.iter().filter(|s| s.kind == Kind::Text).flat_map(|s| std::iter::once(s.family.clone()).chain(s.runs.iter().filter_map(|r| r.family.clone()))).collect();
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("fonts-in-file"), used));
        let id = self.shapes[i].id;
        let s = &mut self.shapes[i];
        let (_, b) = fig::header(ui, "Typography", false, &[(Glyph::Styles, "Apply styles", false)]);
        let mut create = false;
        if let Some(sid) = style_menu(&b[0], self.ext.texts.iter().map(|t| (t.id, t.name.as_str())), true, "Create text style", &mut create) {
            s.text_style = Some(sid);
        }
        if create {
            let nid = self.next_id;
            self.next_id += 1;
            self.ext.texts.push(TextStyle::from_shape(nid, format!("Text {}", self.ext.texts.len() + 1), s));
            s.text_style = Some(nid);
        }
        if let Some(name) = s.text_style.and_then(|sid| self.ext.texts.iter().find(|t| t.id == sid)).map(|t| t.name.clone()) {
            if style_row(ui, g, &name, "Detach text style") {
                s.text_style = None;
            }
            fig::end(ui);
            return;
        }
        let r = fig::controls(ui, 4.0);
        let fam = Rect::from_min_max(Pos2::new(g.left, r.top()), Pos2::new(g.c2 + g.cw, r.bottom()));
        family_picker(ui, fam, s);
        // the weight and the size with a menu of common sizes, as in Figma
        let r = fig::controls(ui, 8.0);
        weight_field(ui, g.col1(r), s);
        let size_field = g.col2(r);
        let menu = Rect::from_min_max(Pos2::new(size_field.right() - 26.0, size_field.top()), size_field.max);
        fig::num_field(ui, size_field.with_max_x(size_field.right() - 26.0), ui.id().with(("fs", id)), Prefix::None, &mut s.font_size, Num { min: 1.0, max: 500.0, decimals: 1, speed: 0.2, suffix: "" }, "Font size");
        let resp = ui.interact(menu, ui.id().with(("fs-menu", id)), Sense::click());
        fig::input_bg(ui, menu, resp.hovered());
        paint_glyph_sized(ui.painter(), menu.center(), Glyph::ChevronDown, TEXT(), 9.5);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Font sizes"));
        egui::Popup::menu(&resp).show(|ui| {
            for size in [8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 32.0, 36.0, 40.0, 48.0, 64.0, 80.0, 96.0, 128.0] {
                if crate::glyphs::menu_option(ui, false, &format!("{size}")) {
                    s.font_size = size;
                    ui.close();
                }
            }
        });
        fig::labels(ui, g, false, "Line height", "Letter spacing");
        let r = fig::controls(ui, 0.0);
        if s.line_h == 0.0 {
            let c1 = g.col1(r);
            let resp = ui.interact(c1, ui.id().with(("lh-auto", id)), Sense::click());
            fig::input_bg(ui, c1, resp.hovered());
            paint_glyph_sized(ui.painter(), Pos2::new(c1.left() + 12.0, r.center().y), Glyph::Type, TEXT_SECONDARY(), 13.0);
            fig::text(ui, Pos2::new(c1.left() + 28.0, r.center().y), Align2::LEFT_CENTER, "Auto", fig::reg(11.0), TEXT());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Line height: auto"));
            if resp.clicked() {
                s.line_h = 1.2;
            }
        } else {
            fig::num_field(ui, g.col1(r), ui.id().with(("lh", id)), Prefix::Icon(Glyph::Type), &mut s.line_h, Num { min: 0.0, max: 5.0, decimals: 2, speed: 0.01, suffix: "" }, "Line height");
        }
        fig::num_field(ui, g.col2(r), ui.id().with(("ls", id)), Prefix::Icon(Glyph::DistH), &mut s.tracking, Num { min: -100.0, max: 500.0, decimals: 1, speed: 0.1, suffix: "%" }, "Letter spacing");
        fig::labels(ui, g, false, "Alignment", "");
        let r = fig::controls(ui, 0.0);
        if let Some(k) = fig::segments(ui, g.col1(r), &[(Glyph::TextLeft, "Align text left", s.align == TextAlign::Left), (Glyph::TextCenter, "Align text center", s.align == TextAlign::Center), (Glyph::TextRight, "Align text right", s.align == TextAlign::Right)]) {
            s.align = [TextAlign::Left, TextAlign::Center, TextAlign::Right][k];
        }
        if let Some(k) = fig::segments(ui, g.col2(r), &[(Glyph::ToTop, "Align text top", s.valign == 0), (Glyph::ToMiddle, "Align text middle", s.valign == 1), (Glyph::ToBottom, "Align text bottom", s.valign == 2)]) {
            s.valign = k as u8;
        }
        let settings = fig::icon_button(ui, g.icon(r), Glyph::Sliders, "Type settings", false);
        self.typography_extras(ui, g, i, settings);
        fig::end(ui);
    }

    // a line's Layout: its length is the W, the H is always 0 (greyed)
    fn line_layout(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        let id = self.shapes[i].id;
        fig::header(ui, "Layout", false, &[]);
        fig::labels(ui, g, true, "Dimensions", "");
        let r = fig::controls(ui, 0.0);
        let s = &mut self.shapes[i];
        let len = s.size.length();
        let mut w = len;
        if fig::num_field(ui, g.col1(r), ui.id().with(("line-w", id)), Prefix::Text("W"), &mut w, Num { min: 0.0, decimals: 2, ..Num::default() }, "Width") {
            s.size = if len > 1e-4 { s.size / len * w } else { Vec2::new(w, 0.0) };
        }
        fig::input_bg(ui, g.col2(r), false);
        fig::text(ui, Pos2::new(g.c2 + 8.0, r.center().y), Align2::LEFT_CENTER, "H", fig::reg(11.0), TEXT_TERTIARY());
        fig::text(ui, Pos2::new(g.c2 + 26.0, r.center().y), Align2::LEFT_CENTER, "0", fig::reg(11.0), TEXT_TERTIARY());
        fig::end(ui);
    }

    // a text layer's Layout (outside auto layout): how its box follows the text, then its Dimensions; the box sizes the text
    // itself decide are shown greyed
    fn text_layout(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        let id = self.shapes[i].id;
        fig::header(ui, "Layout", false, &[]);
        fig::labels(ui, g, true, "Resizing", "");
        let r = fig::controls(ui, 0.0);
        let s = &mut self.shapes[i];
        if let Some(k) = fig::segments(ui, g.both(r), &[(Glyph::AutoWidth, "Auto width", s.fit == TextFit::AutoWidth), (Glyph::AutoHeight, "Auto height", s.fit == TextFit::AutoHeight), (Glyph::Frame, "Fixed size", s.fit == TextFit::Fixed)]) {
            s.fit = [TextFit::AutoWidth, TextFit::AutoHeight, TextFit::Fixed][k];
        }
        fig::labels(ui, g, false, "Dimensions", "");
        let r = fig::controls(ui, 0.0);
        let (w_free, h_free) = (s.fit != TextFit::AutoWidth, s.fit == TextFit::Fixed);
        for (free, field, prefix, value, tag) in [(w_free, g.col1(r), "W", s.size.x, 0), (h_free, g.col2(r), "H", s.size.y, 1)] {
            if free {
                let v = if tag == 0 { &mut s.size.x } else { &mut s.size.y };
                fig::num_field(ui, field, ui.id().with(("tw", id, tag)), Prefix::Text(prefix), v, n(2), if tag == 0 { "Width" } else { "Height" });
            } else {
                fig::input_bg(ui, field, false);
                fig::text(ui, Pos2::new(field.left() + 8.0, field.center().y), Align2::LEFT_CENTER, prefix, fig::reg(11.0), TEXT_TERTIARY());
                fig::text(ui, Pos2::new(field.left() + 26.0, field.center().y), Align2::LEFT_CENTER, &format!("{}", value.round()), fig::reg(11.0), TEXT_TERTIARY());
            }
        }
        if fig::icon_button(ui, g.icon(r), Glyph::Ratio, "Lock aspect ratio", s.lock_aspect).clicked() {
            s.lock_aspect ^= true;
        }
        fig::end(ui);
    }

    fn guides_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        let id = self.shapes[i].id;
        let s = &mut self.shapes[i];
        let has = !s.grids.is_empty();
        let styled = s.grid_style.and_then(|sid| self.ext.grids.iter().find(|e| e.id == sid)).map(|e| e.name.clone());
        let show_styles = has || styled.is_some() || !self.ext.grids.is_empty();
        let (head, b) = fig::header(ui, "Layout guide", !has && styled.is_none(), if show_styles { &[(Glyph::Styles, "Apply styles", false), (Glyph::Plus, "Add layout guide", false)] } else { &[(Glyph::Plus, "Add layout guide", false)] });
        if show_styles {
            let mut create = false;
            if let Some(sid) = style_menu(&b[0], self.ext.grids.iter().map(|e| (e.id, e.name.as_str())), has, "Create grid style", &mut create) {
                s.grid_style = Some(sid);
            }
            if create {
                let nid = self.next_id;
                self.next_id += 1;
                self.ext.grids.push(GridStyle { id: nid, name: format!("Grid {}", self.ext.grids.len() + 1), grids: s.grids.clone() });
                s.grid_style = Some(nid);
            }
        }
        if let Some(name) = styled {
            if style_row(ui, g, &name, "Detach grid style") {
                s.grid_style = None;
            }
            fig::end(ui);
            return;
        }
        if b[b.len() - 1].clicked() || (!has && head.clicked()) {
            s.grids.push(LayoutGrid::new(GridKind::Grid));
        }
        let mut remove = None;
        for (k, lg) in s.grids.iter_mut().enumerate() {
            if grid_editor(ui, g, lg, ui.id().with(("lg", id)), k, &self.variables) {
                remove = Some(k);
            }
        }
        if let Some(k) = remove {
            s.grids.remove(k);
        }
        if has { fig::end(ui) } else { fig::rule(ui) }
    }

    // several layers: their count, alignment and distribution, flips
    pub(crate) fn multi_props(&mut self, ui: &mut egui::Ui, act: &mut Option<Act>) {
        let g = Grid::of(ui.max_rect());
        let r = fig::row(ui, 48.0);
        fig::text(ui, Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, &format!("{} layers", self.sel.len()), fig::semi(13.0), TEXT());
        let buttons = [(Glyph::Group, "Group selection (Ctrl+G)", 0), (Glyph::Union, "Boolean groups", 1), (Glyph::Scan, "Flatten (Ctrl+E)", 3), (Glyph::Component, "Create component", 2)];
        let xs = fig::button_xs(r.right() - 8.0, &buttons.map(|b| b.2 == 1));
        for (k, (gl, tip, a)) in buttons.into_iter().enumerate() {
            let b = Rect::from_min_size(Pos2::new(xs[k], r.top() + 12.0), Vec2::splat(fig::ROW));
            let resp = if a == 1 { fig::menu_icon_button(ui, b, gl, tip, false) } else { fig::icon_button(ui, b, gl, tip, false) };
            match a {
                0 if resp.clicked() => *act = Some(Act::Group),
                2 if resp.clicked() => *act = Some(Act::Component),
                3 if resp.clicked() => *act = Some(Act::Flatten),
                1 => {
                    // the same menu as a single layer's: union, subtract, intersect, exclude and flatten
                    egui::Popup::menu(&resp).show(|ui| {
                        for (g, label, op) in [(Glyph::Union, "Union selection", BoolOp::Union), (Glyph::Subtract, "Subtract selection", BoolOp::Subtract), (Glyph::Intersect, "Intersect selection", BoolOp::Intersect), (Glyph::Exclude, "Exclude selection", BoolOp::Exclude)] {
                            if menu_item(ui, Some(g), label, "", true) {
                                *act = Some(Act::Bool(op));
                                ui.close();
                            }
                        }
                        crate::glyphs::menu_sep(ui);
                        if menu_item(ui, Some(Glyph::Flatten), "Flatten", "Ctrl+E", true) {
                            *act = Some(Act::Flatten);
                            ui.close();
                        }
                    });
                }
                _ => {}
            }
        }
        fig::rule(ui);
        // several main components together: the Properties "+" can combine them as variants
        let roots = self.sel_roots();
        if roots.len() > 1 && roots.iter().all(|&id| self.index_of(id).is_some_and(|i| self.shapes[i].component && !self.shapes[i].component_set && self.set_of(i).is_none())) {
            let (head, b) = fig::header(ui, "Properties", true, &[(Glyph::Plus, "Create property", false)]);
            let menu = b.first().cloned().unwrap_or(head);
            self.create_property_menu(&menu, None);
            fig::rule(ui);
        }
        fig::header(ui, "Position", false, &[]);
        fig::labels(ui, &g, true, "Alignment", "");
        let r = fig::controls(ui, 0.0);
        if let Some(k) = fig::segments(ui, g.col1(r), &[(Glyph::AlignLeft, "Align left", false), (Glyph::AlignHCenter, "Align horizontal centers", false), (Glyph::AlignRight, "Align right", false)]) {
            *act = Some(Act::Align([AlignOp::Left, AlignOp::HCenter, AlignOp::Right][k]));
        }
        if let Some(k) = fig::segments(ui, g.col2(r), &[(Glyph::AlignTop, "Align top", false), (Glyph::AlignVMiddle, "Align vertical centers", false), (Glyph::AlignBottom, "Align bottom", false)]) {
            *act = Some(Act::Align([AlignOp::Top, AlignOp::VCenter, AlignOp::Bottom][k]));
        }
        self.alignment_more(ui, &g, r, act);
        fig::labels(ui, &g, false, "Distribute", "Flip");
        let r = fig::controls(ui, 0.0);
        if let Some(k) = fig::segments(ui, g.col1(r), &[(Glyph::DistH, "Distribute horizontal spacing", false), (Glyph::DistV, "Distribute vertical spacing", false)]) {
            *act = Some(Act::Distribute(k == 0));
        }
        if let Some(k) = fig::segments(ui, g.col2(r), &[(Glyph::FlipH, "Flip horizontal", false), (Glyph::FlipV, "Flip vertical", false)]) {
            *act = Some(Act::Flip(k == 0));
        }
        // the box around the selection: moving it moves every layer, resizing it scales them together
        if let Some(bb) = self.sel_bounds() {
            let key = ui.id().with("multi-box");
            fig::labels(ui, &g, false, "Position", "");
            let r = fig::controls(ui, 0.0);
            let (mut x, mut y) = (bb.min.x, bb.min.y);
            let cx = fig::num_field(ui, g.col1(r), key.with("x"), Prefix::Text("X"), &mut x, n(2), "X position");
            let cy = fig::num_field(ui, g.col2(r), key.with("y"), Prefix::Text("Y"), &mut y, n(2), "Y position");
            if cx || cy {
                self.nudge(x - bb.min.x, y - bb.min.y);
            }
            fig::labels(ui, &g, false, "Dimensions", "");
            let r = fig::controls(ui, 0.0);
            self.dimension_fields(ui, &g, r);
        }
        fig::end(ui);
        self.text_multi(ui, &g);
        self.look_sections(ui, &g, act, self.sel.clone(), true);
    }

    // several text layers together: the Typography section of the first one, whatever is changed there is written to all of them
    fn text_multi(&mut self, ui: &mut egui::Ui, g: &Grid) {
        let targets: Vec<u64> = self.sel_roots();
        if targets.len() < 2 || !targets.iter().all(|&id| self.index_of(id).is_some_and(|i| self.shapes[i].kind == Kind::Text)) {
            return;
        }
        let Some(first) = self.index_of(targets[0]) else { return };
        let orig = self.shapes[first].clone();
        let mut proxy = orig.clone();
        proxy.id = u64::MAX - 2;
        proxy.parent = None;
        self.shapes.push(proxy);
        let pi = self.shapes.len() - 1;
        self.typography_section(ui, g, pi);
        let Some(edited) = self.shapes.pop() else { return };
        for id in targets {
            let Some(i) = self.index_of(id) else { continue };
            let s = &mut self.shapes[i];
            macro_rules! sync {
                ($($f:ident),*) => { $( if edited.$f != orig.$f { s.$f = edited.$f.clone(); } )* };
            }
            sync!(font_size, bold, tracking, valign, wrap_style, opsz, vtrim, mono, align, line_h, fit, family, weight, italic, case, underline, strike, list, para_spacing, para_indent, max_lines, script, text_dir, ul_style, ul_thick, ul_offset, ul_skip, ul_color, hang, list_spacing, axes, features, text_style);
        }
    }

    // the "more actions" button at the right of the Alignment row: distribute and tidy up
    fn alignment_more(&mut self, ui: &mut egui::Ui, g: &Grid, r: Rect, act: &mut Option<Act>) {
        let many = self.sel.len() > 1;
        // Figma 126.9.13 (REA): a lone text layer has no More actions button in its Alignment row
        if !many && self.single().is_some_and(|i| self.shapes[i].kind == Kind::Text) {
            return;
        }
        let b = fig::icon_button(ui, g.icon(r), Glyph::DistV, "More actions", false);
        egui::Popup::menu(&b).show(|ui| {
            if menu_item(ui, Some(Glyph::DistH), "Distribute horizontal spacing", "Ctrl+Alt+H", many) {
                *act = Some(Act::Distribute(true));
                ui.close();
            }
            if menu_item(ui, Some(Glyph::DistV), "Distribute vertical spacing", "Ctrl+Alt+V", many) {
                *act = Some(Act::Distribute(false));
                ui.close();
            }
            crate::glyphs::menu_sep(ui);
            if menu_item(ui, Some(Glyph::Grid), "Tidy up", "Ctrl+Alt+T", many) {
                self.tidy_up();
                ui.close();
            }
        });
    }

    // a section: Layout with Resize to fit and the Dimensions only (no flow, no clip content)
    fn section_layout(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize, act: &mut Option<Act>) {
        let id = self.shapes[i].id;
        let (_, head) = fig::header(ui, "Layout", false, &[(Glyph::Shrink, "Resize to fit", false)]);
        if head[0].clicked() {
            *act = Some(Act::ResizeToFit);
        }
        fig::labels(ui, g, true, "Dimensions", "");
        let r = fig::controls(ui, 0.0);
        let size = Num { min: 0.0, decimals: 2, ..Num::default() };
        let s = &mut self.shapes[i];
        fig::num_field(ui, g.col1(r), ui.id().with(("sec-w", id)), Prefix::Text("W"), &mut s.size.x, size, "Width");
        fig::num_field(ui, g.col2(r), ui.id().with(("sec-h", id)), Prefix::Text("H"), &mut s.size.y, size, "Height");
        fig::end(ui);
    }

    // a group: Layout with its Dimensions; resizing scales everything inside
    fn group_layout(&mut self, ui: &mut egui::Ui, g: &Grid) {
        fig::header(ui, "Layout", false, &[]);
        fig::labels(ui, g, true, "Dimensions", "");
        let r = fig::controls(ui, 0.0);
        self.dimension_fields(ui, g, r);
        fig::end(ui);
    }

    // W and H of the selection's box; typing a value scales the selected layers (and what is inside groups) together
    fn dimension_fields(&mut self, ui: &mut egui::Ui, g: &Grid, r: Rect) {
        let Some(bb) = self.sel_bounds() else { return };
        let key = ui.id().with("box-dims");
        let (mut w, mut h) = (bb.width(), bb.height());
        let size = Num { min: 0.01, decimals: 2, ..Num::default() };
        let cw = fig::num_field(ui, g.col1(r), key.with("w"), Prefix::Text("W"), &mut w, size, "Width");
        let ch = fig::num_field(ui, g.col2(r), key.with("h"), Prefix::Text("H"), &mut h, size, "Height");
        if cw || ch {
            self.begin_scale();
            self.do_scale((1, 1), bb, Pos2::new(bb.min.x + w, bb.min.y + h), false);
        }
    }

    // Appearance (when asked), Fill, Stroke and Effects for several layers, as Figma shows them: the sections of the first
    // target; whatever is changed there is written to every target (values that differ between them are not marked "Mixed" yet)
    fn look_sections(&mut self, ui: &mut egui::Ui, g: &Grid, act: &mut Option<Act>, targets: Vec<u64>, appearance: bool) {
        let Some(first) = targets.iter().filter_map(|&id| self.index_of(id)).next() else { return };
        let orig = self.shapes[first].clone();
        let mut proxy = orig.clone();
        proxy.id = u64::MAX - 1;
        proxy.parent = None;
        self.shapes.push(proxy);
        let pi = self.shapes.len() - 1;
        let paints = targets.iter().filter_map(|&id| self.index_of(id)).any(|i| self.shapes[i].kind != Kind::Group);
        if appearance {
            self.appearance_section(ui, g, pi);
        }
        if paints {
            let _ = self.paint_sections(ui, g, pi, act);
        }
        let Some(edited) = self.shapes.pop() else { return };
        for id in targets {
            let Some(i) = self.index_of(id) else { continue };
            let s = &mut self.shapes[i];
            macro_rules! sync {
                ($($f:ident),*) => { $( if edited.$f != orig.$f { s.$f = edited.$f.clone(); } )* };
            }
            sync!(visible, opacity, blend);
            if s.kind != Kind::Group {
                sync!(fills, strokes, stroke_w, dash, dash_pattern, stroke_align, stroke_cap, stroke_join, miter_angle, effects, shaders);
            }
            if matches!(s.kind, Kind::Rect | Kind::Frame) {
                sync!(radius);
            }
        }
    }

    // nothing selected: the page background and the color styles
    pub(crate) fn page_props(&mut self, ui: &mut egui::Ui, act: &mut Option<Act>) {
        let g = Grid::of(ui.max_rect());
        fig::header(ui, "Page", false, &[]);
        let r = fig::controls(ui, 4.0);
        let f = g.col1(r);
        fig::input_bg(ui, f, false);
        let cur = self.cur;
        let page = &mut self.pages[cur];
        let bg = &mut page.background;
        fig::swatch(ui, Pos2::new(f.left() + 12.0, r.center().y), bg, "Page background");
        fig::hex_text(ui, Pos2::new(f.left() + 24.0, r.center().y), f.width() - 28.0, bg, ui.id().with("page-hex"));
        // the page's opacity is its colour's alpha (egui keeps colours premultiplied, so go through the straight form)
        let [pr, pg, pb, pa] = bg.to_srgba_unmultiplied();
        let mut pct = pa as f32 / 255.0 * 100.0;
        if fig::num_field(ui, g.col2(r), ui.id().with("page-opacity"), Prefix::Icon(Glyph::Contrast), &mut pct, Num { min: 0.0, max: 100.0, decimals: 0, suffix: "%", speed: 1.0 }, "Page opacity") {
            *bg = Color32::from_rgba_unmultiplied(pr, pg, pb, (pct / 100.0 * 255.0).round() as u8);
        }
        if fig::icon_button(ui, g.icon(r), if page.bg_visible { Glyph::Eye } else { Glyph::EyeOff }, "Toggle page background", !page.bg_visible).clicked() {
            page.bg_visible ^= true;
        }
        fig::end(ui);
        let has = !self.styles.is_empty();
        let (head, b) = fig::header(ui, "Color styles", !has, &[(Glyph::Plus, "New color style", false)]);
        if b[0].clicked() || (!has && head.clicked()) {
            *act = Some(Act::NewStyle(Color32::from_rgb(0x0d, 0x99, 0xff)));
        }
        let mut remove = None;
        for (k, st) in self.styles.iter_mut().enumerate() {
            let r = fig::controls(ui, if k == 0 { 4.0 } else { 8.0 });
            let f = g.both(r);
            fig::input_bg(ui, f, false);
            fig::swatch(ui, Pos2::new(f.left() + 12.0, r.center().y), &mut st.color, &format!("Style {}", st.name));
            let edit = Rect::from_min_max(Pos2::new(f.left() + 22.0, f.top()), Pos2::new(f.right() - 4.0, f.bottom()));
            fig::put(ui, edit, egui::TextEdit::singleline(&mut st.name).id(ui.id().with(("style-name", st.id))).frame(egui::Frame::NONE).font(fig::reg(11.0)).margin(egui::Margin::symmetric(2, 0)).vertical_align(egui::Align::Center));
            // a description of the style: written here, shown when the pointer rests on the style anywhere
            let info = fig::icon_button(ui, Rect::from_min_size(g.icon(r).min - Vec2::new(28.0, 0.0), Vec2::splat(fig::ROW)), Glyph::Info, "Style description", !st.description.is_empty());
            egui::Popup::menu(&info).show(|ui| {
                ui.set_min_width(220.0);
                let row = fig::row(ui, 56.0);
                let field = Rect::from_min_max(row.min + Vec2::new(8.0, 4.0), row.max - Vec2::new(8.0, 4.0));
                fig::input_bg(ui, field, false);
                fig::put(ui, field, egui::TextEdit::multiline(&mut st.description).id(ui.id().with(("style-desc", st.id))).frame(egui::Frame::NONE).font(fig::reg(11.0)).hint_text("Describe when to use this style").desired_width(f32::INFINITY));
            });
            if !st.description.is_empty() {
                info.on_hover_text(st.description.clone());
            }
            if fig::icon_button(ui, g.icon(r), Glyph::Minus, "Delete style", false).clicked() {
                remove = Some(k);
            }
        }
        if let Some(k) = remove {
            self.delete_style(k);
        }
        if has { fig::end(ui) } else { fig::rule(ui) }
        self.ext_styles_sections(ui, &g);
    }

    // every color inside the selection, editable in one place
    pub(crate) fn selection_colors_section(&mut self, ui: &mut egui::Ui) {
        let colors = self.selection_colors();
        if colors.is_empty() {
            return;
        }
        let g = Grid::of(ui.max_rect());
        // Figma folds this section into its title row: three swatches and "+n"; a click opens the list (measured: 16px swatches, 24px apart)
        let open_id = ui.id().with("selection-colors-open");
        // Figma lists up to three colors and folds longer lists into the title row
        let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(colors.len() <= 3);
        let (head, _) = fig::header(ui, "Selection colors", false, &[]);
        if head.clicked() {
            ui.data_mut(|d| d.insert_temp(open_id, !open));
        }
        if !open {
            let r = head.rect;
            let more = colors.len().saturating_sub(3);
            let mut right = r.right() - 16.0;
            if more > 0 {
                let t = fig::ptext(ui.painter(), Pos2::new(right, r.top() + 20.0 + fig::TEXT_LIFT * 11.0), Align2::RIGHT_CENTER, format!("+{more}"), fig::reg(11.0), TEXT_SECONDARY());
                right = t.left() - 10.0;
            }
            let shown = colors.len().min(3);
            for (k, c) in colors.iter().take(3).enumerate() {
                let sw = Rect::from_center_size(Pos2::new(right - 8.0 - 24.0 * (shown - 1 - k) as f32, r.top() + 20.0), Vec2::splat(16.0));
                ui.painter().rect_filled(sw, 4.0, *c);
                ui.painter().rect_stroke(sw, 4.0, Stroke::new(1.0, Color32::from_white_alpha(20)), egui::StrokeKind::Inside);
            }
            fig::rule(ui);
            return;
        }
        let mut edit = None;
        for (k, c) in colors.iter().enumerate() {
            let r = fig::controls(ui, if k == 0 { 4.0 } else { 8.0 });
            let f = g.both(r);
            fig::input_bg(ui, f, false);
            let mut now = *c;
            let a = fig::swatch(ui, Pos2::new(f.left() + 12.0, r.center().y), &mut now, &format!("Selection color {}", k + 1));
            let b = fig::hex_text(ui, Pos2::new(f.left() + 24.0, r.center().y), f.width() - 28.0, &mut now, ui.id().with(("sel-hex", k)));
            if a || b {
                edit = Some((*c, now));
            }
        }
        if let Some((from, to)) = edit {
            self.recolor(from, to);
        }
        fig::end(ui);
    }
}

// the "Auto layout settings" popover, measured on Figma 126.9.13: a 40px header, a 120px preview, then 32px rows with the label
// 16px from the edge and a 112px dropdown at x = 112. Inside stroke, Align text baseline and Layout version are not built.
fn layout_settings(ui: &mut egui::Ui, l: &mut AutoLayout) {
    if crate::strokefx::popover_header(ui, "Auto layout settings") {
        ui.close();
    }
    // the preview: two overlapping boxes, the one drawn last is on top
    let r = fig::row(ui, 120.0);
    ui.painter().rect_filled(r, 0.0, Color32::from_gray(0x38));
    let (a, b) = (Color32::from_gray(0x80), Color32::from_gray(0xb0));
    let box_at = |x: f32| Rect::from_min_size(Pos2::new(r.center().x - 36.0 + x, r.center().y - 20.0 + x * 0.3), Vec2::splat(40.0));
    let order = if l.first_on_top { [(box_at(32.0), a), (box_at(0.0), b)] } else { [(box_at(0.0), b), (box_at(32.0), a)] };
    for (rect, c) in order {
        ui.painter().rect(rect, fig::RADIUS, c, Stroke::new(1.0, Color32::from_gray(0x38)), egui::StrokeKind::Inside);
    }
    fig::row(ui, 8.0);
    let field = |r: Rect| Rect::from_min_size(Pos2::new(r.left() + 112.0, r.top() + 4.0), Vec2::new(112.0, fig::ROW));
    let r = fig::row(ui, 32.0);
    crate::strokefx::row_label(ui, r, "Canvas stacking");
    fig::dropdown_outlined(ui, field(r), None, &mut l.first_on_top, &[(false, "Last on top"), (true, "First on top")], "Canvas stacking");
    let r = fig::row(ui, 32.0);
    fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Include stroke", fig::reg(11.0), TEXT_SECONDARY());
    fig::checkbox(ui, Rect::from_min_size(Pos2::new(r.left() + 112.0, r.top() + 4.0), Vec2::new(112.0, fig::ROW)), &mut l.stroke_in_layout, "Include stroke in layout");
    // Align text baseline: for a horizontal flow
    let r = fig::row(ui, 32.0);
    fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Align text baseline", fig::reg(11.0), if l.vertical { TEXT_TERTIARY() } else { TEXT_SECONDARY() });
    if l.vertical {
        l.baseline = false;
    }
    let cb = Rect::from_min_size(Pos2::new(r.left() + 112.0, r.top() + 4.0), Vec2::new(112.0, fig::ROW));
    ui.add_enabled_ui(!l.vertical, |ui| fig::checkbox(ui, cb, &mut l.baseline, "Align text baseline"));
    let r = fig::row(ui, 32.0);
    let auto = l.main == Align::SpaceBetween;
    fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Auto spacing", fig::reg(11.0), if auto { TEXT_SECONDARY() } else { TEXT_TERTIARY() });
    if auto {
        let mut v = 0;
        fig::dropdown_outlined(ui, field(r), None, &mut v, &[(0, "Between")], "Auto spacing");
    } else {
        let f = field(r);
        ui.painter().rect_stroke(f, fig::RADIUS, Stroke::new(1.0, fig::RULE()), egui::StrokeKind::Inside);
        fig::ptext(ui.painter(), Pos2::new(f.left() + 8.0, f.center().y), Align2::LEFT_CENTER, "Between", fig::reg(11.0), TEXT_TERTIARY());
    }
    fig::row(ui, 16.0);
}

// the "Apply styles" menu: the styles to pick from, and "Create style" from what the layer has now
fn style_menu<'a>(btn: &egui::Response, items: impl Iterator<Item = (u64, &'a str)>, can_create: bool, create_label: &str, create: &mut bool) -> Option<u64> {
    let items: Vec<(u64, String)> = items.map(|(id, n)| (id, n.to_owned())).collect();
    egui::Popup::menu(btn)
        .show(|ui| {
            let mut pick = None;
            for (id, name) in &items {
                if menu_item(ui, None, name, "", true) {
                    pick = Some(*id);
                    ui.close();
                }
            }
            if can_create && menu_item(ui, Some(Glyph::Plus), create_label, "", true) {
                *create = true;
                ui.close();
            }
            if items.is_empty() && !can_create {
                menu_item(ui, None, "No styles yet", "", false);
            }
            pick
        })
        .and_then(|r| r.inner)
}

// the row of a layer that follows a style: its name and a button that lets go of it; true when detached
fn style_row(ui: &mut egui::Ui, g: &Grid, name: &str, tip: &str) -> bool {
    let r = fig::controls(ui, 2.0);
    let f = g.both(r);
    fig::input_bg(ui, f, false);
    paint_glyph_sized(ui.painter(), Pos2::new(f.left() + 12.0, r.center().y), Glyph::Styles, TEXT_SECONDARY(), 13.0);
    fig::text(ui, Pos2::new(f.left() + 28.0, r.center().y), Align2::LEFT_CENTER, name, fig::reg(11.0), TEXT());
    fig::icon_button(ui, g.icon(r), Glyph::Minus, tip, false).clicked()
}

// one layout guide, as Figma 126.9.13 shows it (measured 2026-10-18): a 24px row of the guide's icon button (it opens the settings
// popover), a summary dropdown ("Grid 10px", "5 columns", "5 rows") that picks the type, an eye and a minus, laid out from the
// panel's 16px padding at 0, 32, 164.7 and 192.7; true when its minus was pressed
fn grid_editor(ui: &mut egui::Ui, g: &Grid, lg: &mut LayoutGrid, key: egui::Id, k: usize, vars: &[Collection]) -> bool {
    let r = fig::controls(ui, if k == 0 { 4.0 } else { 8.0 });
    let at = |x: f32, w: f32| Rect::from_min_size(Pos2::new(g.left + x, r.top()), Vec2::new(w, fig::ROW));
    let open_id = key.with(("lg-open", k));
    let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let glyph = match lg.kind {
        GridKind::Grid => Glyph::Grid,
        GridKind::Columns => Glyph::Columns,
        GridKind::Rows => Glyph::Rows,
    };
    let icon = fig::icon_button(ui, at(0.0, 24.0), glyph, "Layout guide settings", open);
    let shown = crate::picker::popup(&icon, ui.max_rect().left(), |ui| grid_popover(ui, lg, key.with(("lg-pop", k)), vars));
    ui.data_mut(|d| d.insert_temp(open_id, shown.is_some()));
    let summary = match lg.kind {
        GridKind::Grid => format!("Grid {}px", lg.size),
        GridKind::Columns => format!("{} column{}", lg.count, if lg.count == 1 { "" } else { "s" }),
        GridKind::Rows => format!("{} row{}", lg.count, if lg.count == 1 { "" } else { "s" }),
    };
    fig::dropdown_shown(ui, at(32.0, 124.7), None, &mut lg.kind, &[(GridKind::Grid, "Grid"), (GridKind::Columns, "Columns"), (GridKind::Rows, "Rows")], &format!("Layout guide {} type", k + 1), false, Some(&summary));
    if fig::icon_button(ui, at(164.7, 24.0), if lg.visible { Glyph::Eye } else { Glyph::EyeOff }, "Toggle visibility", false).clicked() {
        lg.visible ^= true;
    }
    fig::icon_button(ui, at(192.7, 24.0), Glyph::Minus, "Remove layout guide", false).clicked()
}

// the guide's settings popover: a 44px header with the type as an outlined dropdown and the close button, then 32px rows with the
// label at 16 and a 136px field at 88: Grid has Size and Color; Columns and Rows have Count, Color, Type, Width / Height, Margin
// and Gutter
pub(crate) fn grid_popover(ui: &mut egui::Ui, lg: &mut LayoutGrid, key: egui::Id, vars: &[Collection]) {
    use crate::strokefx::row_label;
    let r = fig::row(ui, 44.0);
    let name = match lg.kind {
        GridKind::Grid => "Grid",
        GridKind::Columns => "Columns",
        GridKind::Rows => "Rows",
    };
    let w = ui.painter().layout_no_wrap(name.into(), fig::reg(11.0), TEXT()).size().x + 33.0;
    fig::dropdown_outlined(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top() + 10.0), Vec2::new(w, fig::ROW)), None, &mut lg.kind, &[(GridKind::Grid, "Grid"), (GridKind::Columns, "Columns"), (GridKind::Rows, "Rows")], "Layout guide type");
    if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 32.0, r.top() + 10.0), Vec2::splat(fig::ROW)), Glyph::Close, "Close", false).clicked() {
        ui.close();
    }
    fig::rule(ui);
    fig::row(ui, 10.0);
    let field = |r: Rect| Rect::from_min_size(Pos2::new(r.left() + 88.0, r.top() + 4.0), Vec2::new(136.0, fig::ROW));
    let plain = Num { min: 0.0, decimals: 1, speed: 0.5, ..Num::default() };
    if lg.kind == GridKind::Grid {
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Size");
        fig::num_field(ui, field(r), key.with("size"), Prefix::None, &mut lg.size, Num { min: 2.0, max: 500.0, ..plain }, "Grid size");
        if let Some(v) = var_field(ui, field(r), key.with("bind-size"), lg.var_num[0], vars, VarKind::Number, VarScope::Spacing) {
            lg.var_num[0] = v;
        }
    } else {
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Count");
        let mut count = lg.count as f32;
        if fig::num_field(ui, field(r), key.with("count"), Prefix::None, &mut count, Num { min: 1.0, max: 64.0, decimals: 0, speed: 0.1, suffix: "" }, "Count") {
            lg.count = count.round().max(1.0) as u32;
        }
    }
    // the colour: swatch, hex and opacity in one field, like a paint row
    let r = fig::row(ui, 32.0);
    row_label(ui, r, "Color");
    let f = field(r);
    fig::input_bg(ui, f, false);
    let mut c = lg.color;
    fig::swatch(ui, Pos2::new(f.left() + 14.0, f.center().y), &mut c, "Layout guide color");
    // the hex is of the colour without its opacity (a Color32 holds it premultiplied)
    let [cr, cg, cb, ca] = c.to_srgba_unmultiplied();
    let mut rgb = Color32::from_rgb(cr, cg, cb);
    if fig::hex_text(ui, Pos2::new(f.left() + 28.0, f.center().y), f.width() - 28.0 - 48.0, &mut rgb, key.with("hex")) {
        c = Color32::from_rgba_unmultiplied(rgb.r(), rgb.g(), rgb.b(), ca);
    }
    let op = Rect::from_min_max(Pos2::new(f.right() - 48.0, f.top()), f.max);
    ui.painter().rect_filled(Rect::from_min_size(op.min, Vec2::new(1.0, op.height())), 0.0, crate::BG());
    let mut alpha = (c.a() as f32 / 255.0 * 100.0).round();
    if fig::num_field(ui, op, key.with("alpha"), Prefix::None, &mut alpha, Num { min: 0.0, max: 100.0, decimals: 0, suffix: "", speed: 1.0 }, "Layout guide opacity") {
        let [r, g, b, _] = c.to_srgba_unmultiplied();
        c = Color32::from_rgba_unmultiplied(r, g, b, (alpha / 100.0 * 255.0).round() as u8);
    }
    fig::text(ui, Pos2::new(op.right() - 8.0, op.center().y), Align2::RIGHT_CENTER, "%", fig::reg(11.0), TEXT());
    lg.color = c;
    if let Some(v) = var_field(ui, f, key.with("bind-color"), lg.var_color, vars, VarKind::Color, VarScope::Effect) {
        lg.var_color = v;
    }
    if lg.kind != GridKind::Grid {
        let cols = lg.kind == GridKind::Columns;
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Type");
        let items: &[(GridAlign, &str)] = if cols {
            &[(GridAlign::Start, "Left"), (GridAlign::End, "Right"), (GridAlign::Center, "Center"), (GridAlign::Stretch, "Stretch")]
        } else {
            &[(GridAlign::Start, "Top"), (GridAlign::End, "Bottom"), (GridAlign::Center, "Center"), (GridAlign::Stretch, "Stretch")]
        };
        fig::dropdown(ui, field(r), None, &mut lg.align, items, "Layout guide alignment");
        // the fixed size only applies when not stretched; Figma shows "Auto" dimmed
        let r = fig::row(ui, 32.0);
        row_label(ui, r, if cols { "Width" } else { "Height" });
        if lg.align == GridAlign::Stretch {
            fig::input_bg(ui, field(r), false);
            fig::text(ui, Pos2::new(field(r).left() + 8.0, r.center().y), Align2::LEFT_CENTER, "Auto", fig::reg(11.0), TEXT_TERTIARY());
        } else {
            fig::num_field(ui, field(r), key.with("cell"), Prefix::None, &mut lg.cell, Num { min: 1.0, ..plain }, if cols { "Column width" } else { "Row height" });
        }
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Margin");
        fig::num_field(ui, field(r), key.with("margin"), Prefix::None, &mut lg.margin, plain, "Margin");
        let r = fig::row(ui, 32.0);
        row_label(ui, r, "Gutter");
        fig::num_field(ui, field(r), key.with("gutter"), Prefix::None, &mut lg.gutter, plain, "Gutter");
        if let Some(v) = var_field(ui, field(r), key.with("bind-gutter"), lg.var_num[1], vars, VarKind::Number, VarScope::Spacing) {
            lg.var_num[1] = v;
        }
    }
    fig::row(ui, 8.0);
}

#[derive(Clone, Copy, PartialEq)]
enum StyleKind {
    Effect,
    Text,
    Grid,
}

impl App {
    // the page panel's effect, text and grid styles: a title with "+", then one row per style (name, edit, delete); the sliders
    // button opens the style's own settings, which every layer following it picks up
    pub(crate) fn ext_styles_sections(&mut self, ui: &mut egui::Ui, g: &Grid) {
        use StyleKind::*;
        for (which, title, add_tip) in [(Effect, "Effect styles", "New effect style"), (Text, "Text styles", "New text style"), (Grid, "Grid styles", "New grid style")] {
            let n = match which {
                Effect => self.ext.effects.len(),
                Text => self.ext.texts.len(),
                Grid => self.ext.grids.len(),
            };
            let (head, b) = fig::header(ui, title, n == 0, &[(Glyph::Plus, add_tip, false)]);
            if b[0].clicked() || (n == 0 && head.clicked()) {
                self.new_ext_style(which);
            }
            let mut remove: Option<usize> = None;
            for k in 0..n {
                let (sid, name) = match which {
                    Effect => (self.ext.effects[k].id, self.ext.effects[k].name.clone()),
                    Text => (self.ext.texts[k].id, self.ext.texts[k].name.clone()),
                    Grid => (self.ext.grids[k].id, self.ext.grids[k].name.clone()),
                };
                let r = fig::controls(ui, if k == 0 { 4.0 } else { 8.0 });
                let desc = self.ext.description(sid).to_owned();
                let open_id = ui.id().with(("ext-open", sid));
                let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
                let f = Rect::from_min_max(Pos2::new(g.left, r.top()), Pos2::new(g.right - 60.0, r.bottom()));
                fig::input_bg(ui, f, false);
                let glyph = match which {
                    Effect => Glyph::Droplet,
                    Text => Glyph::Type,
                    Grid => Glyph::Grid,
                };
                paint_glyph_sized(ui.painter(), Pos2::new(f.left() + 12.0, r.center().y), glyph, TEXT_SECONDARY(), 13.0);
                let edit = Rect::from_min_max(Pos2::new(f.left() + 22.0, f.top()), Pos2::new(f.right() - 4.0, f.bottom()));
                let mut text = name.clone();
                fig::put(ui, edit, egui::TextEdit::singleline(&mut text).id(ui.id().with(("ext-name", sid))).frame(egui::Frame::NONE).font(fig::reg(11.0)).margin(egui::Margin::symmetric(2, 0)).vertical_align(egui::Align::Center));
                if text != name {
                    match which {
                        Effect => self.ext.effects[k].name = text,
                        Text => self.ext.texts[k].name = text,
                        Grid => self.ext.grids[k].name = text,
                    }
                }
                let pencil = Rect::from_min_size(Pos2::new(g.right - 56.0, r.top()), Vec2::splat(fig::ROW));
                if fig::icon_button(ui, pencil, Glyph::Sliders, "Edit style", open).clicked() {
                    ui.data_mut(|d| d.insert_temp(open_id, !open));
                }
                if fig::icon_button(ui, g.icon(r), Glyph::Minus, "Delete style", false).clicked() {
                    remove = Some(k);
                }
                if !desc.is_empty() {
                    ui.interact(f, ui.id().with(("ext-desc-tip", sid)), Sense::hover()).on_hover_text(desc.clone());
                }
                if open {
                    // what the style is for
                    let dr = fig::controls(ui, 4.0);
                    let mut d = desc.clone();
                    fig::text_input(ui, g.both(dr), &mut d, "Style description", "Description (shown on hover)");
                    if d != desc {
                        self.ext.set_description(sid, d);
                    }
                    self.ext_style_editor(ui, g, which, k, sid);
                }
            }
            if let Some(k) = remove {
                match which {
                    Effect => drop(self.ext.effects.remove(k)),
                    Text => drop(self.ext.texts.remove(k)),
                    Grid => drop(self.ext.grids.remove(k)),
                }
            }
            if n == 0 { fig::rule(ui) } else { fig::end(ui) }
        }
    }

    fn new_ext_style(&mut self, which: StyleKind) {
        let id = self.next_id;
        self.next_id += 1;
        match which {
            StyleKind::Effect => self.ext.effects.push(EffectStyle { id, name: format!("Effect {}", self.ext.effects.len() + 1), effects: vec![Effect::new(EffectKind::DropShadow)] }),
            StyleKind::Text => self.ext.texts.push(TextStyle { id, name: format!("Text {}", self.ext.texts.len() + 1), font_size: 16.0, bold: false, mono: false, tracking: 0.0, line_h: 0.0, align: TextAlign::Left, valign: 0, extra: TextExtra::default() }),
            StyleKind::Grid => self.ext.grids.push(GridStyle { id, name: format!("Grid {}", self.ext.grids.len() + 1), grids: vec![LayoutGrid::new(GridKind::Grid)] }),
        }
    }

    fn ext_style_editor(&mut self, ui: &mut egui::Ui, g: &Grid, which: StyleKind, k: usize, sid: u64) {
        match which {
            StyleKind::Effect => {
                let st = &mut self.ext.effects[k];
                let mut gone = None;
                for (e, fx) in st.effects.iter_mut().enumerate() {
                    let r = fig::controls(ui, 8.0);
                    if crate::strokefx::effect_row(ui, g, r, fx, ("ext-fx", e, sid), &self.variables) {
                        gone = Some(e);
                    }
                }
                if let Some(e) = gone {
                    st.effects.remove(e);
                }
                if let Some((k, d)) = crate::strokefx::take_effect_move(ui, sid) {
                    let to = k as i32 + d;
                    if k < st.effects.len() && to >= 0 && (to as usize) < st.effects.len() {
                        st.effects.swap(k, to as usize);
                    }
                }
                let r = fig::controls(ui, 8.0);
                let resp = fig::icon_button(ui, g.icon(r), Glyph::Plus, "Add effect", false);
                if let Some(kind) = crate::strokefx::effect_menu(&resp) {
                    st.effects.push(Effect::new(kind));
                }
            }
            StyleKind::Text => {
                let st = &mut self.ext.texts[k];
                let r = fig::controls(ui, 8.0);
                fig::dropdown(ui, g.col1(r), None, &mut st.bold, &[(false, "Regular"), (true, "Semi Bold")], "Style font weight");
                fig::num_field(ui, g.col2(r), ui.id().with(("ext-fs", sid)), Prefix::None, &mut st.font_size, Num { min: 1.0, max: 500.0, decimals: 1, speed: 0.2, suffix: "" }, "Style font size");
                let r = fig::controls(ui, 8.0);
                fig::num_field(ui, g.col1(r), ui.id().with(("ext-lh", sid)), Prefix::Icon(Glyph::Type), &mut st.line_h, Num { min: 0.0, max: 5.0, decimals: 2, speed: 0.01, suffix: "" }, "Style line height (0 = auto)");
                fig::num_field(ui, g.col2(r), ui.id().with(("ext-ls", sid)), Prefix::Icon(Glyph::DistH), &mut st.tracking, Num { min: -100.0, max: 500.0, decimals: 1, speed: 0.1, suffix: "%" }, "Style letter spacing");
                let r = fig::controls(ui, 8.0);
                fig::dropdown(ui, g.col1(r), None, &mut st.mono, &[(false, "Inter"), (true, "JetBrains Mono")], "Style font family");
                let weights = [(0u16, "Default weight"), (100, "Thin"), (200, "Extra Light"), (300, "Light"), (400, "Regular"), (500, "Medium"), (600, "Semi Bold"), (700, "Bold"), (800, "Extra Bold"), (900, "Black")];
                fig::dropdown(ui, g.col2(r), None, &mut st.extra.weight, &weights, "Style font weight");
                let r = fig::controls(ui, 8.0);
                fig::dropdown(ui, g.col1(r), None, &mut st.extra.case, &[(0, "Original case"), (1, "Upper case"), (2, "Lower case"), (3, "Title case"), (4, "Small caps")], "Style text case");
                fig::checkbox(ui, g.col2(r), &mut st.extra.italic, "Italic");
                let r = fig::controls(ui, 8.0);
                fig::checkbox(ui, g.col1(r), &mut st.extra.underline, "Underline");
                fig::checkbox(ui, g.col2(r), &mut st.extra.strike, "Strikethrough");
            }
            StyleKind::Grid => {
                let st = &mut self.ext.grids[k];
                let mut gone = None;
                for (e, lg) in st.grids.iter_mut().enumerate() {
                    if grid_editor(ui, g, lg, ui.id().with(("ext-grid", sid)), e, &self.variables) {
                        gone = Some(e);
                    }
                }
                if let Some(e) = gone {
                    st.grids.remove(e);
                }
                let r = fig::controls(ui, 8.0);
                if fig::icon_button(ui, g.icon(r), Glyph::Plus, "Add layout guide", false).clicked() {
                    st.grids.push(LayoutGrid::new(GridKind::Grid));
                }
            }
        }
    }
}

const INTER_WEIGHTS: [(u16, &str); 9] = [(100, "Thin"), (200, "Extra Light"), (300, "Light"), (400, "Regular"), (500, "Medium"), (600, "Semi Bold"), (700, "Bold"), (800, "Extra Bold"), (900, "Black")];

fn weight_name(w: u16) -> String {
    INTER_WEIGHTS.iter().find(|(x, _)| *x == w).map_or_else(|| w.to_string(), |(_, n)| (*n).to_owned())
}

// Figma's font picker: a "Fonts" popover beside the panel with a search field and one 28px row per family (the current one
// highlighted with a check in the gutter). Measured in Figma 126.9.13 (the previews in each family's own face are not drawn).
fn family_picker(ui: &mut egui::Ui, r: Rect, s: &mut Shape) {
    let shape_id = s.id;
    let id = ui.id().with(("family", s.id));
    let resp = ui.interact(r, id, Sense::click());
    fig::input_bg(ui, r, resp.hovered());
    let label = if !s.family.is_empty() { s.family.clone() } else if s.mono { "JetBrains Mono".to_owned() } else { "Inter".to_owned() };
    // a font the layer names but this computer does not have (a file from elsewhere): Inter shows instead, and the name says so
    let missing = !s.family.is_empty() && crate::textrun::family_weights(&s.family).is_empty();
    let shown = if missing { format!("{label} (not installed)") } else { label };
    fig::ptext(&ui.painter().with_clip_rect(r.shrink2(Vec2::new(4.0, 0.0))), Pos2::new(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, &shown, fig::reg(11.0), if missing { TEXT_SECONDARY() } else { TEXT() });
    paint_glyph_sized(ui.painter(), Pos2::new(r.right() - 12.0, r.center().y), Glyph::ChevronDown, TEXT(), 9.5);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Font family"));
    let search_id = id.with("search");
    let mut pick: Option<(String, bool)> = None;
    crate::picker::popup(&resp, ui.max_rect().left(), |ui| {
        // fonts installed since drui started appear (the font folders are only looked at again when they changed)
        crate::textrun::rescan_fonts_if_changed();
        let head = fig::row(ui, 40.0);
        fig::ptext(ui.painter(), Pos2::new(head.left() + 16.0, head.center().y), Align2::LEFT_CENTER, "Fonts", fig::semi(11.0), TEXT());
        let close = Rect::from_min_size(Pos2::new(head.right() - 32.0, head.top() + 8.0), Vec2::splat(fig::ROW));
        if fig::icon_button(ui, close, Glyph::Close, "Close", false).clicked() {
            ui.close();
        }
        let mut q: String = ui.data(|d| d.get_temp(search_id)).unwrap_or_default();
        let sr = fig::row(ui, 24.0);
        let field = Rect::from_min_max(Pos2::new(sr.left() + 8.0, sr.top()), Pos2::new(sr.right() - 8.0, sr.bottom()));
        fig::input_bg(ui, field, false);
        paint_glyph_sized(ui.painter(), Pos2::new(field.left() + 14.0, field.center().y), Glyph::Search, TEXT_SECONDARY(), 13.0);
        let edit = fig::text_edit(ui, field.with_min_x(field.left() + 26.0), &mut q, "Search fonts", "Search fonts");
        if !edit.has_focus() && !ui.data(|d| d.get_temp::<bool>(search_id.with("focused")).unwrap_or(false)) {
            edit.request_focus();
            ui.data_mut(|d| d.insert_temp(search_id.with("focused"), true));
        }
        if edit.has_focus() {
            ui.painter().rect_stroke(field, fig::RADIUS, Stroke::new(1.0, crate::BRAND), egui::StrokeKind::Inside);
        }
        ui.data_mut(|d| d.insert_temp(search_id, q.clone()));
        fig::row(ui, 8.0);
        // filters: all, the fonts this file uses, variable fonts, the popular ones
        let filter_id = search_id.with("filter");
        let mut filter: u8 = ui.data(|d| d.get_temp(filter_id)).unwrap_or(0);
        let fr = fig::row(ui, 24.0);
        let labels = ["All", "In file", "Variable", "Popular"];
        let fw = (fr.width() - 16.0) / 4.0;
        for (k, l) in labels.iter().enumerate() {
            let b = Rect::from_min_size(Pos2::new(fr.left() + 8.0 + fw * k as f32, fr.top()), Vec2::new(fw - 2.0, fig::ROW));
            let resp = ui.interact(b, search_id.with(("filter", k)), Sense::click());
            if filter == k as u8 {
                ui.painter().rect_filled(b, fig::RADIUS, fig::SELECTED());
            } else if resp.hovered() {
                ui.painter().rect_filled(b, fig::RADIUS, fig::INPUT());
            }
            fig::ptext(ui.painter(), b.center(), Align2::CENTER_CENTER, l, fig::reg(11.0), if filter == k as u8 { fig::SELECTED_INK() } else { TEXT_SECONDARY() });
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, filter == k as u8, *l));
            if resp.clicked() {
                filter = k as u8;
            }
        }
        ui.data_mut(|d| d.insert_temp(filter_id, filter));
        fig::row(ui, 4.0);
        let in_file: Vec<String> = ui.data(|d| d.get_temp(egui::Id::new("fonts-in-file"))).unwrap_or_default();
        const POPULAR: [&str; 16] = ["Inter", "Roboto", "Open Sans", "Arial", "Helvetica", "Segoe UI", "Georgia", "Times New Roman", "Verdana", "Courier New", "Consolas", "Calibri", "Montserrat", "Lato", "Poppins", "JetBrains Mono"];
        let needle = q.to_lowercase();
        let defaults = [("Inter", false), ("JetBrains Mono", true)];
        let mut names: Vec<(String, bool)> = defaults.iter().map(|(n, m)| ((*n).to_owned(), *m)).collect();
        names.extend(crate::textrun::system_families().iter().filter(|f| !defaults.iter().any(|d| d.0 == f.as_str())).map(|f| (f.clone(), false)));
        names.retain(|(n, _)| n.to_lowercase().contains(&needle));
        match filter {
            1 => names.retain(|(n, _)| in_file.contains(n) || (n == "Inter" && in_file.iter().any(|f| f.is_empty()))),
            2 => names.retain(|(n, _)| crate::textrun::is_variable(n)),
            3 => names.retain(|(n, _)| POPULAR.contains(&n.as_str())),
            _ => {}
        }
        let current = |n: &str, m: bool| if s.family.is_empty() { (n == "Inter" && !s.mono && !m) || (n == "JetBrains Mono" && s.mono) } else { n == s.family };
        egui::ScrollArea::vertical().max_height(320.0).auto_shrink([false, true]).show_rows(ui, 28.0, names.len(), |ui, range| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for (name, mono) in &names[range] {
                let row = fig::row(ui, 28.0);
                let resp = ui.interact(row, id.with(("font", name)), Sense::click());
                let on = current(name, *mono);
                let inset = row.shrink2(Vec2::new(8.0, 0.0));
                if resp.hovered() {
                    ui.painter().rect_filled(inset, 3.0, crate::BRAND);
                } else if on {
                    ui.painter().rect_filled(inset, 3.0, fig::INPUT());
                }
                if on {
                    paint_glyph_sized(ui.painter(), Pos2::new(row.left() + 20.0, row.center().y), Glyph::Check, TEXT(), 13.0);
                }
                fig::ptext(ui.painter(), Pos2::new(row.left() + 36.0, row.center().y), Align2::LEFT_CENTER, name, fig::reg(13.0), TEXT());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, name));
                if resp.hovered() {
                    // the layer shows the font while the pointer is on it
                    let (fam, is_mono) = (if *mono || name == "Inter" { String::new() } else { name.clone() }, *mono);
                    let mut probe = Shape::default();
                    probe.family = fam.clone();
                    crate::textrun::ensure_fonts(ui.ctx(), std::slice::from_ref(&probe));
                    let frame = ui.ctx().cumulative_frame_nr();
                    ui.data_mut(|d| d.insert_temp(egui::Id::new("font-preview"), (shape_id, fam, is_mono, frame)));
                    ui.ctx().request_repaint();
                }
                if resp.clicked() {
                    pick = Some((if *mono || name == "Inter" { String::new() } else { name.clone() }, *mono));
                    ui.close();
                }
            }
        });
        fig::row(ui, 8.0);
    });
    if let Some((fam, mono)) = pick {
        s.family = fam;
        s.mono = mono;
        s.weight = 0;
        ui.data_mut(|d| {
            d.remove::<String>(search_id);
            d.remove::<bool>(search_id.with("focused"));
        });
    }
}

// the weight dropdown (Figma puts it in the first column of the size row): Inter has all nine weights; an installed family offers what it has
fn weight_field(ui: &mut egui::Ui, rect: Rect, s: &mut Shape) {
    let weights: Vec<(u16, String)> = if s.family.is_empty() && !s.mono {
        INTER_WEIGHTS.iter().map(|(w, n)| (*w, (*n).to_owned())).collect()
    } else if s.family.is_empty() {
        vec![(400, "Regular".to_owned())]
    } else {
        let w = crate::textrun::family_weights(&s.family);
        if w.is_empty() { vec![(400, "Regular".to_owned())] } else { w.into_iter().map(|w| (w, weight_name(w))).collect() }
    };
    let items: Vec<(u16, &str)> = weights.iter().map(|(w, n)| (*w, n.as_str())).collect();
    let mut w = s.base_weight();
    if !items.iter().any(|(x, _)| *x == w) {
        w = items.iter().min_by_key(|(x, _)| (*x as i32 - w as i32).abs()).map_or(400, |(x, _)| *x);
    }
    if fig::dropdown(ui, rect, None, &mut w, &items, "Font weight") {
        s.weight = if w == 400 { 0 } else { w };
        s.bold = false;
    }
    }

// Figma's "Type settings" popover (Basics and Details tabs), measured in Figma 126.9.13: 240 wide, a 40px tab row, 32px rows with
// a 24px control right-aligned at the 16px padding. the Variable tab has Inter's two axes.
fn type_settings(ui: &mut egui::Ui, s: &mut Shape, id: u64) {
    let tab_id = ui.id().with("type-settings-tab");
    let mut tab = ui.data(|d| d.get_temp::<u8>(tab_id)).unwrap_or(0);
    let head = fig::row(ui, 40.0);
    let mut x = head.left() + 8.0;
    for (k, label) in ["Basics", "Details", "Variable"].into_iter().enumerate() {
        let (resp, w) = fig::tab(ui, Pos2::new(x, head.top() + 8.0), label, tab == k as u8);
        if resp.clicked() {
            tab = k as u8;
        }
        x += w;
    }
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    fig::rule(ui);
    // a labelled row: the label at the left padding, the control's rect (24px high, `w` wide) at the right padding
    let line = |ui: &mut egui::Ui, label: &str, w: f32| -> Rect {
        let r = fig::row(ui, 32.0);
        fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, label, fig::reg(11.0), TEXT_SECONDARY());
        Rect::from_min_size(Pos2::new(r.right() - 16.0 - w, r.center().y - fig::ROW / 2.0), Vec2::new(w, fig::ROW))
    };
    let num = Num { min: 0.0, max: 500.0, decimals: 1, speed: 0.2, suffix: "" };
    if tab == 0 {
        fig::row(ui, 8.0);
        let r = line(ui, "Alignment", 72.0);
        if let Some(k) = fig::segments(ui, r, &[(Glyph::AlignLeft, "Align text left", s.align == TextAlign::Left), (Glyph::AlignHCenter, "Align text center", s.align == TextAlign::Center), (Glyph::AlignRight, "Align text right", s.align == TextAlign::Right)]) {
            s.align = [TextAlign::Left, TextAlign::Center, TextAlign::Right][k];
        }
        let r = line(ui, "Decoration", 72.0);
        let cur = if s.underline { 1 } else if s.strike { 2 } else { 0 };
        if let Some(k) = fig::segments(ui, r, &[(Glyph::Minus, "No decoration", cur == 0), (Glyph::Underline, "Underline", cur == 1), (Glyph::Strikethrough, "Strikethrough", cur == 2)]) {
            (s.underline, s.strike) = (k == 1, k == 2);
        }
        if s.underline {
            let r = line(ui, "Underline style", 96.0);
            fig::dropdown(ui, r, None, &mut s.ul_style, &[(0, "Solid"), (1, "Dotted"), (2, "Wavy")], "Underline style");
            let r = line(ui, "Underline thickness", 72.0);
            fig::num_field(ui, r, ui.id().with(("ul-thick", id)), Prefix::None, &mut s.ul_thick, Num { min: 0.0, max: 50.0, decimals: 1, speed: 0.1, suffix: "" }, "Underline thickness (0 = automatic)");
            let r = line(ui, "Underline offset", 72.0);
            fig::num_field(ui, r, ui.id().with(("ul-offset", id)), Prefix::None, &mut s.ul_offset, Num { min: -50.0, max: 50.0, decimals: 1, speed: 0.1, suffix: "" }, "Underline offset (0 = automatic)");
            let r = line(ui, "Skip ink", 24.0);
            fig::checkbox(ui, r, &mut s.ul_skip, "Skip ink");
            let r = line(ui, "Underline color", 72.0);
            fig::input_bg(ui, r, false);
            let mut c = s.ul_color.unwrap_or(Color32::BLACK);
            if fig::swatch(ui, Pos2::new(r.left() + 12.0, r.center().y), &mut c, "Underline color") {
                s.ul_color = Some(c);
            }
            if s.ul_color.is_some() && fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() + 4.0, r.top()), Vec2::splat(fig::ROW)), Glyph::Close, "Use the text color", false).clicked() {
                s.ul_color = None;
            }
        }
        let r = line(ui, "Direction", 96.0);
        fig::dropdown(ui, r, None, &mut s.text_dir, &[(0, "Automatic"), (1, "Left to right"), (2, "Right to left")], "Text direction");
        let r = line(ui, "Case", 120.0);
        let names = ["Original case", "Upper case", "Lower case", "Title case", "Small caps"];
        let marks = ["", "AG", "ag", "Ag", "Aa"];
        let tips: Vec<(&str, bool)> = names.iter().enumerate().map(|(k, n)| (*n, s.case as usize == k)).collect();
        if let Some(k) = fig::segments_custom(ui, r, &tips, |p, c, k, ink| {
            if k == 0 {
                crate::glyphs::paint_glyph(p, c, Glyph::Minus, ink);
            } else {
                fig::ptext(p, c, Align2::CENTER_CENTER, marks[k], fig::reg(10.0), ink);
            }
        }) {
            s.case = k as _;
        }
        fig::row(ui, 8.0);
        fig::rule(ui);
        fig::row(ui, 8.0);
        let r = line(ui, "List style", 72.0);
        if let Some(k) = fig::segments(ui, r, &[(Glyph::Minus, "No list", s.list == 0), (Glyph::BulletList, "Bulleted list", s.list == 1), (Glyph::NumberList, "Numbered list", s.list == 2)]) {
            s.list = k as _;
        }
        let r = line(ui, "Paragraph spacing", 72.0);
        fig::num_field(ui, r, ui.id().with(("pspace", id)), Prefix::None, &mut s.para_spacing, num, "Paragraph spacing");
        let r = line(ui, "Truncate text", 48.0);
        let on = s.max_lines > 0;
        if let Some(k) = fig::segments_custom(ui, r, &[("Do not truncate", !on), ("Truncate text", on)], |p, c, k, ink| {
            if k == 0 {
                crate::glyphs::paint_glyph(p, c, Glyph::Minus, ink);
            } else {
                fig::ptext(p, c, Align2::CENTER_CENTER, "A…", fig::reg(10.0), ink);
            }
        }) {
            s.max_lines = if k == 1 { s.max_lines.max(1) } else { 0 };
        }
        let r = line(ui, "Wrap style", 96.0);
        fig::dropdown(ui, r, None, &mut s.wrap_style, &[(0, "Auto"), (1, "Balance"), (2, "Pretty")], "Wrap style");
        let r = line(ui, "Vertical trim", 48.0);
        if let Some(k) = fig::segments_custom(ui, r, &[("Standard", s.vtrim == 0), ("Cap height to baseline", s.vtrim == 1)], |p, c, k, ink| {
            fig::ptext(p, c, Align2::CENTER_CENTER, ["Std", "Cap"][k], fig::reg(10.0), ink);
        }) {
            s.vtrim = k as u8;
        }
        if s.max_lines > 0 {
            let r = line(ui, "Max lines", 72.0);
            let mut lines = s.max_lines as f32;
            if fig::num_field(ui, r, ui.id().with(("maxlines", id)), Prefix::None, &mut lines, Num { min: 1.0, max: 100.0, decimals: 0, speed: 0.1, suffix: "" }, "Maximum lines") {
                s.max_lines = lines.round().max(1.0) as u32;
            }
        }
    } else if tab == 2 {
        // the axes of the variable font: Inter has Weight and Optical size
        fig::row(ui, 8.0);
        if s.family.is_empty() && !s.mono {
            let mut w = s.base_weight() as f32;
            let r = line(ui, "Weight", 136.0);
            if fig::slider_field(ui, r, ui.id().with(("var-wght", id)), &mut w, 100.0, 900.0, "Weight axis") {
                s.weight = (w.round() as u16).clamp(100, 900);
                s.bold = false;
            }
            let mut o = if s.opsz > 0.0 { s.opsz } else { 14.0 };
            let r = line(ui, "Optical size", 136.0);
            if fig::slider_field(ui, r, ui.id().with(("var-opsz", id)), &mut o, 14.0, 32.0, "Optical size axis") {
                s.opsz = o.round().clamp(14.0, 32.0);
            }
            let r = fig::row(ui, 32.0);
            if fig::button(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.center().y - fig::ROW / 2.0), Vec2::new(208.0, fig::ROW)), "Reset axes").clicked() {
                s.weight = 0;
                s.opsz = 0.0;
            }
        } else {
            let axes = if s.mono { vec![] } else { crate::textrun::family_axes(&s.family) };
            if axes.is_empty() {
                let r = fig::row(ui, 32.0);
                fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "This font has no variable axes.", fig::reg(11.0), TEXT_SECONDARY());
            }
            for (tag, lo, def, hi) in axes {
                let name = String::from_utf8_lossy(&tag).to_string();
                let label = match name.as_str() {
                    "wght" => "Weight",
                    "wdth" => "Width",
                    "slnt" => "Slant",
                    "ital" => "Italic",
                    "opsz" => "Optical size",
                    _ => name.as_str(),
                };
                let mut v = s.axes.iter().find(|(t, _)| *t == name).map_or(def, |a| a.1);
                let r = line(ui, label, 136.0);
                if fig::slider_field(ui, r, ui.id().with(("axis", id, tag)), &mut v, lo, hi, label) {
                    s.axes.retain(|(t, _)| *t != name);
                    s.axes.push((name.clone(), v));
                }
            }
            if !s.axes.is_empty() {
                let r = fig::row(ui, 32.0);
                if fig::button(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.center().y - fig::ROW / 2.0), Vec2::new(208.0, fig::ROW)), "Reset axes").clicked() {
                    s.axes.clear();
                }
            }
        }
    } else {
        fig::row(ui, 8.0);
        let r = fig::row(ui, 32.0);
        fig::checkbox(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.center().y - fig::ROW / 2.0), Vec2::new(120.0, fig::ROW)), &mut s.italic, "Italic");
        let r = line(ui, "Position", 96.0);
        fig::dropdown(ui, r, None, &mut s.script, &[(0, "Normal"), (1, "Superscript"), (2, "Subscript")], "Text position");
        let r = line(ui, "List spacing", 72.0);
        fig::num_field(ui, r, ui.id().with(("lspace", id)), Prefix::None, &mut s.list_spacing, num, "List spacing");
        let r = line(ui, "Hanging", 24.0);
        fig::checkbox(ui, r, &mut s.hang, "Hanging punctuation and lists");
        let r = line(ui, "Paragraph indent", 72.0);
        fig::num_field(ui, r, ui.id().with(("pindent", id)), Prefix::None, &mut s.para_indent, num, "Paragraph indent");
        let r = line(ui, "OpenType features", 96.0);
        fig::text_input(ui, r, &mut s.features, "OpenType features", "tnum, ss01");
    }
    fig::row(ui, 8.0);
}

impl App {
    // Typography below the alignment row: weight and style, case, list, decoration, spacing, truncation, features, and the style of
    // the characters selected in the text editor
    fn typography_extras(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize, settings: egui::Response) {
        let id = self.shapes[i].id;
        let sel = self.text_sel.filter(|t| t.0 == id);
        let s = &mut self.shapes[i];
        crate::picker::popup(&settings, ui.max_rect().left(), |ui| type_settings(ui, s, id));
        // the characters selected in the text editor get their own style
        if let Some((_, a, b)) = sel {
            fig::labels(ui, g, false, "Selected text", "");
            let base = s.char_style(a);
            let r = fig::controls(ui, 0.0);
            let quarter = |k: usize| Rect::from_min_size(Pos2::new(g.left + k as f32 * (g.edge - g.left) / 4.0, r.top()), Vec2::new((g.edge - g.left) / 4.0 - 4.0, fig::ROW));
            if fig::button(ui, quarter(0), "Bold").clicked() {
                let on = base.weight >= 600;
                s.style_range(a, b, |r| r.weight = Some(if on { 400 } else { 700 }));
            }
            if fig::button(ui, quarter(1), "Italic").clicked() {
                let on = base.italic;
                s.style_range(a, b, |r| r.italic = Some(!on));
            }
            if fig::button(ui, quarter(2), "Underline").clicked() {
                let on = base.underline;
                s.style_range(a, b, |r| r.underline = Some(!on));
            }
            if fig::button(ui, quarter(3), "Strike").clicked() {
                let on = base.strike;
                s.style_range(a, b, |r| r.strike = Some(!on));
            }
            let r = fig::controls(ui, 8.0);
            let mut size = base.size;
            if fig::num_field(ui, g.col1(r), ui.id().with(("runsize", id)), Prefix::Text("S"), &mut size, Num { min: 1.0, max: 500.0, decimals: 1, speed: 0.2, suffix: "" }, "Selected text size") {
                s.style_range(a, b, |r| r.size = Some(size));
            }
            let f = g.col2(r);
            fig::input_bg(ui, f, false);
            let mut c = base.color.unwrap_or(Color32::BLACK);
            if fig::swatch(ui, Pos2::new(f.left() + 12.0, r.center().y), &mut c, "Selected text color") {
                s.style_range(a, b, |r| r.color = Some(c));
            }
            let r = fig::controls(ui, 8.0);
            let mut url = base.url.clone().unwrap_or_default();
            let edit = fig::text_input(ui, g.both(r), &mut url, "Link", "https://");
            if edit.changed() {
                let url = url.trim().to_owned();
                s.style_range(a, b, |r| r.url = Some(url.clone()));
            }
        }
    }
}

fn fig_num(v: f32) -> String {
    let t = format!("{v:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl App {
    // the selected vertex while a vector is being edited: its corner radius, a branch, and the Bend tool
    fn vertex_section(&mut self, ui: &mut egui::Ui, g: &Grid) {
        fig::header(ui, "Vertex", false, &[]);
        let Some((c, k)) = self.node_sel else {
            fig::note(ui, Glyph::Cursor, "Select a vertex to round it, or Alt+drag from one to start a branch.");
            fig::end(ui);
            return;
        };
        fig::labels(ui, g, true, "Corner radius", "");
        let r = fig::controls(ui, 0.0);
        let mut radius = self.node_radius(c, k).unwrap_or(0.0);
        if fig::num_field(ui, g.col1(r), ui.id().with(("vertex-radius", c, k)), Prefix::Icon(Glyph::Radius), &mut radius, Num { min: 0.0, max: 1000.0, decimals: 1, speed: 0.2, suffix: "" }, "Vertex corner radius") {
            self.node_set_radius(c, k, radius);
        }
        let r = fig::controls(ui, 8.0);
        if fig::button(ui, g.col1(r), "Add branch").clicked() {
            self.node_add_branch(c, k);
        }
        fig::checkbox(ui, g.col2(r), &mut self.bend_tool, "Bend tool");
        let r = fig::controls(ui, 8.0);
        if fig::button(ui, g.col2(r), "Simplify").clicked() {
            let n = self.simplify_vector(2.0);
            self.status = format!("simplified: {n} points fewer");
            self.node_sel = None;
        }
        let waiting = self.node_connect.is_some();
        if fig::button(ui, g.col1(r), if waiting { "Click a vertex…" } else { "Connect to…" }).clicked() {
            self.node_connect = if waiting { None } else { Some((c, k)) };
        }
        fig::end(ui);
    }
}
