// Dev Mode: the read-only Inspect panel (measurements, colors, typography, CSS) and redlines on the canvas.
use std::fmt::Write;

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};

use crate::glyphs::Glyph;
use crate::model::*;
use crate::{App, GUIDE, TEXT, TEXT_SECONDARY};

// 12, 12.5, 0.33: no trailing zeros
fn num(v: f32) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

// `alpha` multiplies the color's own alpha (paint opacity)
pub fn hex(c: Color32, alpha: f32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let a = (a as f32 * alpha).round() as u8;
    if a == 255 { format!("#{r:02X}{g:02X}{b:02X}") } else { format!("#{r:02X}{g:02X}{b:02X}{a:02X}") }
}

pub fn rgba(c: Color32, alpha: f32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    format!("rgba({r}, {g}, {b}, {})", num(a as f32 / 255.0 * alpha))
}

// a CSS color: hex when opaque, rgba otherwise
pub(crate) fn css_color(c: Color32, alpha: f32) -> String {
    css_color_in(c, alpha, crate::profile::is_p3())
}

pub(crate) fn css_color_in(c: Color32, alpha: f32, p3: bool) -> String {
    let a = c.to_srgba_unmultiplied()[3] as f32 * alpha;
    if p3 {
        let [r, g, b, _] = c.to_srgba_unmultiplied();
        let f = |v: u8| format!("{:.3}", v as f32 / 255.0);
        return if a >= 254.5 { format!("color(display-p3 {} {} {})", f(r), f(g), f(b)) } else { format!("color(display-p3 {} {} {} / {:.2})", f(r), f(g), f(b), a / 255.0) };
    }
    if a >= 254.5 { hex(c, 1.0) } else { rgba(c, alpha) }
}

// one paint as a CSS background layer; solids become flat gradients so they can stack
fn css_stops(g: &Gradient, opacity: f32) -> String {
    g.stops.iter().map(|s| format!("{} {}%", css_color(s.color, opacity), num((s.pos * 100.0 * 10.0).round() / 10.0))).collect::<Vec<_>>().join(", ")
}

fn css_layer(p: &Paint, images: &[Asset], stacked: bool) -> String {
    match p.kind.clone() {
        PaintKind::Gradient(g) => {
            let (cx, cy) = (num(g.center.x * 100.0), num(g.center.y * 100.0));
            match g.kind {
                // drui: 0 degrees = left to right; CSS: 90deg = to right
                GradientKind::Linear => format!("linear-gradient({}deg, {})", num((g.angle + 90.0).rem_euclid(360.0)), css_stops(&g, p.opacity)),
                GradientKind::Radial => format!("radial-gradient(ellipse {}% {}% at {cx}% {cy}%, {})", num(50.0 * g.scale.x), num(50.0 * g.scale.y), css_stops(&g, p.opacity)),
                GradientKind::Angular => format!("conic-gradient(from {}deg at {cx}% {cy}%, {})", num((g.angle + 90.0).rem_euclid(360.0)), css_stops(&g, p.opacity)),
                // CSS has no diamond gradient; the closest is a radial one
                GradientKind::Diamond => format!("radial-gradient(closest-side at {cx}% {cy}%, {})", css_stops(&g, p.opacity)),
            }
        }
        PaintKind::Solid(c) if stacked => format!("linear-gradient({0}, {0})", css_color(c, p.opacity)),
        PaintKind::Solid(c) => css_color(c, p.opacity),
        // drui: 0 degrees = left to right; CSS: 90deg = to right
        PaintKind::Linear { a, b, angle } => format!("linear-gradient({}deg, {}, {})", num((angle + 90.0).rem_euclid(360.0)), css_color(a, p.opacity), css_color(b, p.opacity)),
        PaintKind::Image(id) => {
            let name = images.iter().find(|a| a.id == id).map_or("image".to_owned(), |a| a.name.clone());
            format!("url(\"{name}.png\") center / 100% 100% no-repeat")
        }
        // a pattern is a repeating background of the exported tile; a shader has no CSS, its two colours stand for it
        PaintKind::Pattern(pf) => format!("url(\"pattern-{}.png\") 0 0 / {}% auto repeat", pf.source.unwrap_or(0), num(pf.scale * 100.0)),
        PaintKind::Shader(sf) => format!("linear-gradient({}deg, {}, {})", num((sf.angle + 90.0).rem_euclid(360.0)), css_color(sf.a, p.opacity), css_color(sf.b, p.opacity)),
    }
}

// 8px, or 8px 16px, or all four sides (CSS order: top right bottom left) from drui's left, top, right, bottom
fn css_box(pad: [f32; 4]) -> String {
    let [l, t, r, b] = pad.map(|v| format!("{}px", num(v)));
    if l == r && t == b {
        if l == t { l } else { format!("{t} {l}") }
    } else {
        format!("{t} {r} {b} {l}")
    }
}

fn css_align(a: Align) -> &'static str {
    match a {
        Align::Start => "flex-start",
        Align::Center => "center",
        Align::End => "flex-end",
        Align::SpaceBetween => "space-between",
    }
}

// CSS for shape `s` whose unrotated top-left is at world point `at`;
// inside an auto-layout parent `flow` the shape is a flex/grid item instead of absolutely placed
pub fn shape_css_in(s: &Shape, at: Pos2, images: &[Asset], flow: Option<&AutoLayout>) -> String {
    let mut out = String::new();
    let mut line = |k: &str, v: String| {
        let _ = writeln!(out, "{k}: {v};");
    };
    let (w, h) = (s.size.x.abs(), s.size.y.abs());
    let at = Pos2::new(at.x + s.size.x.min(0.0), at.y + s.size.y.min(0.0)); // lines drawn up or left
    let lay = s.layout.as_ref().filter(|_| s.kind == Kind::Frame);
    match flow {
        Some(p) => {
            // fill on the main axis grows, fill on the cross axis stretches
            let (main_fill, cross_fill) = if p.vertical { (s.fill_y, s.fill_x) } else { (s.fill_x, s.fill_y) };
            if main_fill && p.columns == 0 {
                line("flex", "1 0 0".into());
            } else {
                line("flex-shrink", "0".into());
            }
            if cross_fill || p.columns > 0 && (s.fill_x || s.fill_y) {
                line("align-self", "stretch".into());
            }
        }
        None => {
            line("position", "absolute".into());
            line("left", format!("{}px", num(at.x)));
            line("top", format!("{}px", num(at.y)));
        }
    }
    // hugging and filling sizes come from the layout, so only fixed ones are written
    let filled = |fill: bool| fill && flow.is_some();
    if !lay.is_some_and(|l| l.hug_x) && !filled(s.fill_x) {
        line("width", format!("{}px", num(w)));
    }
    if !lay.is_some_and(|l| l.hug_y) && !filled(s.fill_y) {
        line("height", format!("{}px", num(h)));
    }
    if let Some(l) = lay {
        if l.columns > 0 {
            line("display", "grid".into());
            line("grid-template-columns", format!("repeat({}, minmax(0, 1fr))", l.columns));
        } else {
            line("display", "flex".into());
            line("flex-direction", if l.vertical { "column" } else { "row" }.into());
            if l.wrap {
                line("flex-wrap", "wrap".into());
            }
        }
        if l.pad.iter().any(|&v| v != 0.0) {
            line("padding", css_box(l.pad));
        }
        if l.columns == 0 {
            line("justify-content", css_align(l.main).into());
        }
        let cross = if l.cross == Align::SpaceBetween { Align::Start } else { l.cross };
        line("align-items", css_align(cross).into());
        if l.main != Align::SpaceBetween || l.columns > 0 {
            line("gap", format!("{}px", num(l.gap)));
        }
    }
    let fills: Vec<&Paint> = s.fills.iter().filter(|f| f.visible).collect();
    if s.kind == Kind::Text {
        if let Some(&PaintKind::Solid(c)) = fills.last().map(|f| &f.kind) {
            line("color", css_color(c, fills.last().map_or(1.0, |f| f.opacity)));
        }
        line("font-family", if !s.family.is_empty() { format!("\"{}\", sans-serif", s.family) } else if s.mono { "\"JetBrains Mono\", monospace".into() } else { "Inter, sans-serif".into() });
        line("font-size", format!("{}px", num(s.font_size)));
        if s.base_weight() != 400 {
            line("font-weight", s.base_weight().to_string());
        }
        if s.italic {
            line("font-style", "italic".into());
        }
        match s.case {
            1 => line("text-transform", "uppercase".into()),
            2 => line("text-transform", "lowercase".into()),
            3 => line("text-transform", "capitalize".into()),
            4 => line("font-variant", "small-caps".into()),
            _ => {}
        }
        match (s.underline, s.strike) {
            (true, true) => line("text-decoration", "underline line-through".into()),
            (true, false) => line("text-decoration", "underline".into()),
            (false, true) => line("text-decoration", "line-through".into()),
            _ => {}
        }
        if s.tracking != 0.0 {
            line("letter-spacing", format!("{}px", num(s.font_size * s.tracking / 100.0)));
        }
        if s.para_indent > 0.0 {
            line("text-indent", format!("{}px", num(s.para_indent)));
        }
        if s.script != 0 {
            line("vertical-align", if s.script == 1 { "super" } else { "sub" }.into());
        }
        if !s.features.trim().is_empty() {
            let f: Vec<String> = s.features.split(',').map(|t| format!("\"{}\"", t.trim())).filter(|t| t.len() > 2).collect();
            line("font-feature-settings", f.join(", "));
        }
        if s.wrap_style != 0 {
            line("text-wrap", if s.wrap_style == 1 { "balance" } else { "pretty" }.into());
        }
        if s.vtrim == 1 {
            line("text-box-trim", "trim-both".into());
            line("text-box-edge", "cap alphabetic".into());
        }
        if s.max_lines > 0 {
            line("display", "-webkit-box".into());
            line("-webkit-line-clamp", s.max_lines.to_string());
            line("-webkit-box-orient", "vertical".into());
            line("overflow", "hidden".into());
        }
        if s.line_h > 0.0 {
            line("line-height", num(s.line_h));
        }
        let align = match s.align {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
        };
        line("text-align", align.into());
    } else if !fills.is_empty() && s.kind != Kind::Line && s.kind != Kind::Group {
        // CSS lists the top layer first; drui stores bottom to top
        let stacked = fills.len() > 1;
        let layers: Vec<String> = fills.iter().rev().map(|f| css_layer(f, images, stacked)).collect();
        line("background", layers.join(", "));
    }
    if let Some(st) = s.strokes.iter().find(|p| p.visible) {
        if s.stroke_w > 0.0 {
            let c = match &st.kind {
                PaintKind::Solid(c) | PaintKind::Linear { a: c, .. } => *c,
                PaintKind::Gradient(g) => g.stops.first().map_or(Color32::GRAY, |s| s.color),
                PaintKind::Image(_) | PaintKind::Pattern(_) => Color32::GRAY,
                PaintKind::Shader(sf) => sf.a,
            };
            line("border", format!("{}px {} {}", num(s.stroke_w), if s.dash { "dashed" } else { "solid" }, css_color(c, st.opacity)));
        }
    }
    match s.kind {
        Kind::Ellipse => line("border-radius", "50%".into()),
        Kind::Rect | Kind::Frame if s.radius.iter().any(|&r| r > 0.0) => {
            let r = s.radius;
            let v = if r.iter().all(|&x| x == r[0]) { format!("{}px", num(r[0])) } else { r.iter().map(|&x| format!("{}px", num(x))).collect::<Vec<_>>().join(" ") };
            line("border-radius", v);
        }
        _ => {}
    }
    if s.kind == Kind::Frame && s.clip {
        line("overflow", "hidden".into());
    }
    if s.opacity < 1.0 {
        line("opacity", num(s.opacity));
    }
    if !s.blend.is_normal() {
        line("mix-blend-mode", s.blend.css().into());
    }
    if s.rotation != 0.0 {
        line("transform", format!("rotate({}deg)", num(s.rotation)));
    }
    // shadows share one box-shadow list (text-shadow has no spread or inset); blurs are filters
    let shadows: Vec<String> = s
        .effects
        .iter()
        .filter(|e| e.visible && e.kind.is_shadow() && (s.kind != Kind::Text || e.kind == EffectKind::DropShadow))
        .map(|e| {
            let inset = if e.kind == EffectKind::InnerShadow { "inset " } else { "" };
            if s.kind == Kind::Text {
                format!("{}px {}px {}px {}", num(e.dx), num(e.dy), num(e.blur), css_color(e.color, 1.0))
            } else {
                format!("{inset}{}px {}px {}px {}px {}", num(e.dx), num(e.dy), num(e.blur), num(e.spread), css_color(e.color, 1.0))
            }
        })
        .collect();
    if !shadows.is_empty() {
        line(if s.kind == Kind::Text { "text-shadow" } else { "box-shadow" }, shadows.join(", "));
    }
    if let Some(e) = s.effects.iter().find(|e| e.visible && e.kind == EffectKind::LayerBlur) {
        line("filter", format!("blur({}px)", num(e.blur / 2.0)));
    }
    if let Some(e) = s.effects.iter().find(|e| e.visible && e.kind == EffectKind::BackgroundBlur) {
        line("backdrop-filter", format!("blur({}px)", num(e.blur / 2.0)));
    }
    // glass: the frosted backdrop, a faint white tint and a light edge (CSS has no refraction)
    if let Some(e) = s.effects.iter().find(|e| e.visible && e.kind == EffectKind::Glass) {
        line("backdrop-filter", format!("blur({}px)", num(e.blur / 2.0)));
        line("background-color", format!("rgba(255, 255, 255, {})", num(0.12 * e.density / 50.0)));
        line("box-shadow", format!("inset 0 0 {}px rgba(255, 255, 255, {})", num(e.radius), num(0.5 * e.density / 50.0)));
    }
    out
}

// padding / gap overlays and the box model
const SPACING: Color32 = Color32::from_rgb(0xf2, 0x48, 0x22);

// the Inspect box model (Figma Dev Mode's "Layer properties"): padding on four sides around the content
// size, 16px in from the left, 8px from the right
fn box_model(ui: &mut egui::Ui, pad: [f32; 4], content: Vec2) {
    let row = crate::fig::row(ui, 104.0);
    let r = Rect::from_min_max(Pos2::new(row.left() + 16.0, row.top() + 4.0), Pos2::new(row.right() - 8.0, row.bottom() - 4.0));
    let resp = ui.interact(r, ui.id().with("box-model"), egui::Sense::hover());
    let p = ui.painter();
    p.rect(r, 5.0, SPACING.gamma_multiply(0.10), Stroke::new(1.0, SPACING.gamma_multiply(0.55)), egui::StrokeKind::Inside);
    crate::fig::ptext(p, r.left_top() + Vec2::new(8.0, 10.0), egui::Align2::LEFT_CENTER, "Padding", crate::fig::reg(10.0), TEXT_SECONDARY());
    let inner = r.shrink2(Vec2::new(44.0, 28.0));
    p.rect(inner, 3.0, crate::fig::INPUT(), Stroke::new(1.0, crate::fig::RULE()), egui::StrokeKind::Inside);
    let font = crate::fig::reg(11.0);
    let size = format!("{} × {}", num(content.x.max(0.0)), num(content.y.max(0.0)));
    crate::fig::ptext(p, inner.center(), egui::Align2::CENTER_CENTER, &size, font.clone(), TEXT());
    let [l, t, rt, b] = pad;
    let c = r.center();
    crate::fig::ptext(p, Pos2::new((r.min.x + inner.min.x) / 2.0, c.y), egui::Align2::CENTER_CENTER, num(l), font.clone(), TEXT());
    crate::fig::ptext(p, Pos2::new(c.x, (r.min.y + inner.min.y) / 2.0), egui::Align2::CENTER_CENTER, num(t), font.clone(), TEXT());
    crate::fig::ptext(p, Pos2::new((r.max.x + inner.max.x) / 2.0, c.y), egui::Align2::CENTER_CENTER, num(rt), font.clone(), TEXT());
    crate::fig::ptext(p, Pos2::new(c.x, (r.max.y + inner.max.y) / 2.0), egui::Align2::CENTER_CENTER, num(b), font, TEXT());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("Box model: padding {}, content {size}", css_box(pad))));
}

// distance markers between the selection `a` and the hovered shape `b` (world rects):
// insets when one contains the other, otherwise the gap on each axis where they don't overlap
pub fn redlines(a: Rect, b: Rect) -> Vec<(Pos2, Pos2, f32)> {
    let mut out = vec![];
    let (inner, outer) = if b.contains_rect(a) {
        (a, b)
    } else if a.contains_rect(b) {
        (b, a)
    } else {
        // separated horizontally: one segment at the middle of their vertical overlap (or of `a`)
        let mid_y = if a.max.y > b.min.y && b.max.y > a.min.y { (a.min.y.max(b.min.y) + a.max.y.min(b.max.y)) / 2.0 } else { a.center().y };
        let mid_x = if a.max.x > b.min.x && b.max.x > a.min.x { (a.min.x.max(b.min.x) + a.max.x.min(b.max.x)) / 2.0 } else { a.center().x };
        if a.max.x <= b.min.x {
            out.push((Pos2::new(a.max.x, mid_y), Pos2::new(b.min.x, mid_y), b.min.x - a.max.x));
        } else if b.max.x <= a.min.x {
            out.push((Pos2::new(b.max.x, mid_y), Pos2::new(a.min.x, mid_y), a.min.x - b.max.x));
        }
        if a.max.y <= b.min.y {
            out.push((Pos2::new(mid_x, a.max.y), Pos2::new(mid_x, b.min.y), b.min.y - a.max.y));
        } else if b.max.y <= a.min.y {
            out.push((Pos2::new(mid_x, b.max.y), Pos2::new(mid_x, a.min.y), a.min.y - b.max.y));
        }
        return out;
    };
    let c = inner.center();
    for (p, q) in [
        (Pos2::new(outer.min.x, c.y), Pos2::new(inner.min.x, c.y)),
        (Pos2::new(inner.max.x, c.y), Pos2::new(outer.max.x, c.y)),
        (Pos2::new(c.x, outer.min.y), Pos2::new(c.x, inner.min.y)),
        (Pos2::new(c.x, inner.max.y), Pos2::new(c.x, outer.max.y)),
    ] {
        let d = (q - p).length();
        if d > 0.01 {
            out.push((p, q, d));
        }
    }
    out
}

// the spacing an auto-layout frame shows in Dev Mode (world rects with their size):
// its padding on each side, and the gap from each child to its nearest neighbour right of / below it
pub fn spacing(frame: Rect, pad: [f32; 4], kids: &[Rect]) -> Vec<(Rect, f32)> {
    let mut out = vec![];
    let [l, t, r, b] = pad;
    for (zone, v) in [
        (Rect::from_min_max(frame.min, Pos2::new(frame.min.x + l, frame.max.y)), l),
        (Rect::from_min_max(frame.min, Pos2::new(frame.max.x, frame.min.y + t)), t),
        (Rect::from_min_max(Pos2::new(frame.max.x - r, frame.min.y), frame.max), r),
        (Rect::from_min_max(Pos2::new(frame.min.x, frame.max.y - b), frame.max), b),
    ] {
        if v > 0.01 {
            out.push((zone, v));
        }
    }
    // the shared range of two spans, if they share some length
    let shared = |a0: f32, a1: f32, b0: f32, b1: f32| Some((a0.max(b0), a1.min(b1))).filter(|(lo, hi)| lo < hi);
    for a in kids {
        let right = kids.iter().filter(|k| k.min.x >= a.max.x + 0.01 && shared(a.min.y, a.max.y, k.min.y, k.max.y).is_some()).min_by(|p, q| p.min.x.total_cmp(&q.min.x));
        if let Some(k) = right {
            let (y0, y1) = shared(a.min.y, a.max.y, k.min.y, k.max.y).unwrap();
            out.push((Rect::from_min_max(Pos2::new(a.max.x, y0), Pos2::new(k.min.x, y1)), k.min.x - a.max.x));
        }
        let below = kids.iter().filter(|k| k.min.y >= a.max.y + 0.01 && shared(a.min.x, a.max.x, k.min.x, k.max.x).is_some()).min_by(|p, q| p.min.y.total_cmp(&q.min.y));
        if let Some(k) = below {
            let (x0, x1) = shared(a.min.x, a.max.x, k.min.x, k.max.x).unwrap();
            out.push((Rect::from_min_max(Pos2::new(x0, a.max.y), Pos2::new(x1, k.min.y)), k.min.y - a.max.y));
        }
    }
    out
}

impl App {
    // unrotated top-left of shape `i` in world space
    fn world_pos(&self, i: usize) -> Pos2 {
        self.shapes[i].pos + self.origin_of(self.shapes[i].parent)
    }

    // the auto layout of shape `i`'s parent, if that places it (lines are left where they are)
    pub(crate) fn flow_of(&self, i: usize) -> Option<&AutoLayout> {
        let p = self.shapes[i].parent.and_then(|p| self.index_of(p))?;
        self.shapes[p].layout.as_ref().filter(|_| self.shapes[p].kind == Kind::Frame && self.shapes[i].kind != Kind::Line)
    }


    // the visible children of layer `i` in z-order (bottom first)
    pub(crate) fn visible_children(&self, i: usize) -> Vec<usize> {
        let id = self.shapes[i].id;
        (0..self.shapes.len()).filter(|&k| self.shapes[k].parent == Some(id) && self.shapes[k].visible).collect()
    }



    // the CSS of layer `i`; inside the code of a parent, `nested` positions it from the parent's corner as nested HTML needs
    pub(crate) fn css_nested(&self, i: usize, nested: bool) -> String {
        let at = if nested { self.shapes[i].pos } else { self.world_pos(i) };
        shape_css_in(&self.shapes[i], at, &self.images, self.flow_of(i))
    }

    pub fn css_of(&self, i: usize) -> String {
        shape_css_in(&self.shapes[i], self.world_pos(i), &self.images, self.flow_of(i))
    }

    // padding and gaps of an auto-layout frame: the hovered one, else the selected one
    pub fn draw_spacing(&self, painter: &Painter, origin: Pos2, hover: Option<u64>) {
        let is_flow = |i: &usize| self.shapes[*i].kind == Kind::Frame && self.shapes[*i].layout.is_some();
        let Some(fi) = hover.and_then(|id| self.index_of(id)).filter(is_flow).or_else(|| self.single().filter(is_flow)) else { return };
        let Some(lay) = self.shapes[fi].layout.as_ref() else { return };
        let fid = self.shapes[fi].id;
        let kids: Vec<Rect> = (0..self.shapes.len()).filter(|&k| self.shapes[k].parent == Some(fid) && self.shapes[k].kind != Kind::Line && self.shapes[k].visible).map(|k| self.world_rect(k)).collect();
        for (zone, v) in spacing(self.world_rect(fi), lay.pad, &kids) {
            let r = self.screen_rect(origin, zone);
            painter.rect_filled(r, 0.0, SPACING.gamma_multiply(0.16));
            if r.width().min(r.height()) >= 1.0 {
                let galley = painter.layout_no_wrap(num(v), FontId::proportional(10.0), Color32::WHITE);
                let pill = Rect::from_center_size(r.center(), galley.size() + Vec2::new(6.0, 2.0));
                painter.rect_filled(pill, 2.0, SPACING);
                crate::fig::ptext(painter, pill.center(), egui::Align2::CENTER_CENTER, num(v), FontId::proportional(10.0), Color32::WHITE);
            }
        }
    }

    // red distance lines from the selection to the shape under the pointer
    pub fn draw_redlines(&self, painter: &Painter, origin: Pos2, hover: Option<u64>) {
        let (Some(a), Some(hi)) = (self.sel_bounds(), hover.and_then(|id| self.index_of(id))) else { return };
        if self.sel.contains(&self.shapes[hi].id) {
            return;
        }
        let b = self.world_rect(hi);
        painter.rect_stroke(self.screen_rect(origin, b), 0.0, Stroke::new(1.0, GUIDE), egui::StrokeKind::Outside);
        for (p, q, d) in redlines(a, b) {
            let (sp, sq) = (self.to_screen(origin, p), self.to_screen(origin, q));
            painter.line_segment([sp, sq], Stroke::new(1.0, GUIDE));
            let cap = if (sq - sp).x.abs() > (sq - sp).y.abs() { Vec2::new(0.0, 4.0) } else { Vec2::new(4.0, 0.0) };
            for e in [sp, sq] {
                painter.line_segment([e - cap, e + cap], Stroke::new(1.0, GUIDE));
            }
            let galley = painter.layout_no_wrap(num(d), FontId::proportional(10.5), Color32::WHITE);
            let mid = sp + (sq - sp) / 2.0;
            let r = Rect::from_center_size(mid, galley.size() + Vec2::new(8.0, 4.0));
            painter.rect_filled(r, 3.0, GUIDE);
            crate::fig::ptext(painter, r.center(), egui::Align2::CENTER_CENTER, num(d), FontId::proportional(10.5), Color32::WHITE);
        }
    }

    // Dev Mode's Inspect tab, laid out like Figma's: the layer, List / Code with a language, then sections of
    // label / value rows (click a value to copy it), colors, the box model, typography and assets
    pub fn inspect_panel(&mut self, ui: &mut egui::Ui) {
        use crate::fig;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let g = fig::Grid::of(ui.max_rect());
        let Some(i) = self.single() else {
            fig::row(ui, 8.0);
            fig::note(ui, Glyph::Code, if self.sel.is_empty() { "Select a layer to inspect it." } else { "Select one layer to inspect it." });
            if self.sel.is_empty() {
                fig::row(ui, 8.0);
                fig::rule(ui);
                self.dev_overview(ui, &g);
            }
            return;
        };
        let s = self.shapes[i].clone();
        let at = self.world_pos(i);
        // the layer: name and kind
        let r = fig::row(ui, 48.0);
        let name_end = fig::ptext(&ui.painter().with_clip_rect(r), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, &s.name, fig::semi(13.0), TEXT()).right();
        fig::ptext(ui.painter(), Pos2::new(name_end + 8.0, r.center().y), Align2::LEFT_CENTER, format!("{:?}", s.kind), fig::reg(11.0), TEXT_SECONDARY());
        fig::rule(ui);

        // the variables that drive this layer, as Figma's Dev Mode lists them
        let var_name = |id: u64| self.variables.iter().find_map(|c| c.vars.iter().find(|v| v.id == id).map(|v| format!("{} / {}", c.name, v.name)));
        let mut bound: Vec<(String, String)> = s.bindings.iter().filter_map(|(p, id)| Some((format!("{p:?}"), var_name(*id)?))).collect();
        for (what, paints) in [("Fill", &s.fills), ("Stroke", &s.strokes)] {
            bound.extend(paints.iter().filter_map(|p| Some((what.to_owned(), var_name(p.var?)?))));
        }
        if !bound.is_empty() {
            fig::header(ui, "Variables", false, &[]);
            for (what, name) in &bound {
                fig::value_row(ui, what, name);
            }
            fig::end(ui);
        }
        fig::header(ui, "Size & position", false, &[]);
        fig::value_row(ui, "Width", &format!("{}px", num(s.size.x.abs())));
        fig::value_row(ui, "Height", &format!("{}px", num(s.size.y.abs())));
        fig::value_row(ui, "Left", &format!("{}px", num(at.x)));
        fig::value_row(ui, "Top", &format!("{}px", num(at.y)));
        if s.rotation != 0.0 {
            fig::value_row(ui, "Rotation", &format!("{}°", num(s.rotation)));
        }
        if s.opacity < 1.0 {
            fig::value_row(ui, "Opacity", &format!("{}%", num(s.opacity * 100.0)));
        }
        fig::end(ui);

        let paints: Vec<(&str, &Paint)> = s.fills.iter().filter(|p| p.visible).map(|p| ("Fill", p)).chain(s.strokes.iter().filter(|p| p.visible).map(|p| ("Stroke", p))).collect();
        if !paints.is_empty() {
            fig::header(ui, "Colors", false, &[]);
            for (what, p) in paints {
                let colors: Vec<Color32> = match &p.kind {
                    PaintKind::Solid(c) => vec![*c],
                    PaintKind::Linear { a, b, .. } => vec![*a, *b],
                    PaintKind::Gradient(g) => g.stops.iter().map(|s| s.color).collect(),
                    PaintKind::Image(_) | PaintKind::Pattern(_) => vec![],
                    PaintKind::Shader(sf) => vec![sf.a, sf.b],
                };
                if colors.is_empty() {
                    fig::value_row(ui, what, "image");
                }
                for c in colors {
                    // swatch, hex (click to copy), rgba
                    let r = fig::controls(ui, 0.0);
                    let f = Rect::from_min_max(Pos2::new(g.left, r.top()), Pos2::new(g.edge, r.bottom()));
                    let resp = ui.interact(f, ui.id().with(("color", what, hex(c, p.opacity))), egui::Sense::click());
                    if resp.hovered() {
                        fig::input_bg(ui, f, false);
                    }
                    let sw = Rect::from_center_size(Pos2::new(f.left() + 12.0, r.center().y), Vec2::splat(14.0));
                    ui.painter().rect(sw, 2.0, c, Stroke::new(1.0, Color32::from_black_alpha(50)), egui::StrokeKind::Inside);
                    let h = hex(c, p.opacity);
                    let end = fig::ptext(ui.painter(), Pos2::new(f.left() + 24.0, r.center().y), Align2::LEFT_CENTER, &h, fig::reg(11.0), TEXT()).right();
                    fig::ptext(&ui.painter().with_clip_rect(f), Pos2::new(end + 8.0, r.center().y), Align2::LEFT_CENTER, rgba(c, p.opacity), fig::reg(11.0), TEXT_SECONDARY());
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{what} {h}")));
                    if resp.clicked() {
                        ui.ctx().copy_text(h.clone());
                        self.status = format!("{h} copied");
                    }
                }
            }
            fig::end(ui);
        }
        if let Some(l) = s.layout.as_ref().filter(|_| s.kind == Kind::Frame) {
            fig::header(ui, "Auto layout", false, &[]);
            box_model(ui, l.pad, Vec2::new(s.size.x - l.pad[0] - l.pad[2], s.size.y - l.pad[1] - l.pad[3]));
            let flow = if l.columns > 0 { format!("Grid, {} columns", l.columns) } else if l.vertical { "Vertical".into() } else { "Horizontal".into() };
            fig::value_row(ui, "Direction", &if l.wrap && l.columns == 0 { format!("{flow}, wrap") } else { flow });
            fig::value_row(ui, "Gap", &if l.main == Align::SpaceBetween && l.columns == 0 { "Auto (space between)".into() } else { format!("{}px", num(l.gap)) });
            fig::value_row(ui, "Padding", &css_box(l.pad));
            fig::value_row(ui, "Align", &format!("{}, {}", css_align(l.main), css_align(l.cross)));
            let sizing = |hug: bool| if hug { "Hug" } else { "Fixed" };
            fig::value_row(ui, "Sizing", &format!("{} × {}", sizing(l.hug_x), sizing(l.hug_y)));
            fig::end(ui);
        }
        if s.kind == Kind::Text {
            fig::header(ui, "Typography", false, &[]);
            fig::value_row(ui, "Font", if s.mono { "JetBrains Mono" } else { "Inter" });
            fig::value_row(ui, "Size", &format!("{}px", num(s.font_size)));
            if s.line_h > 0.0 {
                fig::value_row(ui, "Line height", &num(s.line_h));
            }
            fig::value_row(ui, "Align", &format!("{:?}", s.align));
            fig::end(ui);
        }
        self.inspect_assets(ui, i);
        self.dev_layer_sections(ui, &g, i);
    }

    // the layer's assets: its saved export settings, or a one-off export
    fn inspect_assets(&mut self, ui: &mut egui::Ui, i: usize) {
        use crate::fig;
        let g = fig::Grid::of(ui.max_rect());
        let s = self.shapes[i].clone();
        let (_, b) = fig::header(ui, "Assets", false, &[(Glyph::Copy, "Copy SVG", false)]);
        if b[0].clicked() {
            if let Some((svg, _)) = self.svg_for(ui.ctx(), &[s.id]) {
                ui.ctx().copy_text(svg);
                self.status = "SVG copied".into();
            }
        }
        if !s.exports.is_empty() {
            for set in &s.exports {
                let what = if set.format == ExportFormat::Png { format!("{}x PNG", num(set.scale)) } else { set.format.ext().to_uppercase() };
                fig::value_row(ui, &what, &set.file_name(&s.name));
            }
            let n = s.exports.len();
            let r = fig::controls(ui, 8.0);
            if fig::button(ui, g.full(r), &format!("Export {n} asset{}", if n == 1 { "" } else { "s" })).clicked() {
                self.export_saved(&ui.ctx().clone());
            }
        } else {
            let r = fig::controls(ui, 2.0);
            fig::dropdown(ui, g.col1(r), None, &mut self.export_fmt, &[(ExportFormat::Png, "PNG"), (ExportFormat::Svg, "SVG"), (ExportFormat::Pdf, "PDF")], "Asset format");
            if self.export_fmt == ExportFormat::Png {
                fig::dropdown(ui, g.col2(r), None, &mut self.export_scale, &[(1.0, "1x"), (2.0, "2x"), (3.0, "3x")], "Asset scale");
            }
            let r = fig::controls(ui, 8.0);
            if fig::button(ui, g.full(r), &format!("Export {}", s.name)).clicked() {
                self.export(&ui.ctx().clone());
            }
        }
        fig::end(ui);
    }
}
