// Export to SVG, PNG and PDF. Everything goes through an SVG document built from the shapes;
// resvg rasterizes it and svg2pdf turns it into a page.
use std::fmt::Write;

use base64::Engine;
use egui::{Color32, Pos2, Rect, Vec2};

use crate::geom::Poly;
use crate::model::*;
use crate::render::{text_galley, wrap_of};
use crate::App;

pub use crate::model::ExportFormat as Format;

fn col(c: Color32) -> (String, f32) {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    (format!("rgb({r},{g},{b})"), a as f32 / 255.0)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn path_d(polys: &[Poly], off: Vec2) -> String {
    let mut d = String::new();
    for p in polys.iter().filter(|p| p.pts.len() >= 2) {
        for (k, q) in p.pts.iter().enumerate() {
            let _ = write!(d, "{}{:.2} {:.2} ", if k == 0 { "M" } else { "L" }, q.x + off.x, q.y + off.y);
        }
        if p.closed {
            d.push('Z');
        }
    }
    d
}


// the image adjustments as a chain of filter primitives (each value -1..1, 0 = untouched)
fn adjust_filter(f: &ImageFill) -> String {
    let mut out = String::new();
    if f.exposure.abs() > 1e-4 {
        let k = 2f32.powf(f.exposure * 1.5);
        let _ = write!(out, r#"<feColorMatrix type="matrix" values="{k:.4} 0 0 0 0  0 {k:.4} 0 0 0  0 0 {k:.4} 0 0  0 0 0 1 0"/>"#);
    }
    if f.contrast.abs() > 1e-4 {
        let slope = 1.0 + f.contrast * if f.contrast > 0.0 { 1.5 } else { 1.0 };
        let icpt = 0.5 * (1.0 - slope);
        let _ = write!(out, r#"<feComponentTransfer><feFuncR type="linear" slope="{slope:.4}" intercept="{icpt:.4}"/><feFuncG type="linear" slope="{slope:.4}" intercept="{icpt:.4}"/><feFuncB type="linear" slope="{slope:.4}" intercept="{icpt:.4}"/></feComponentTransfer>"#);
    }
    if f.saturation.abs() > 1e-4 {
        let _ = write!(out, r#"<feColorMatrix type="saturate" values="{:.4}"/>"#, (1.0 + f.saturation).max(0.0));
    }
    if f.temperature.abs() > 1e-4 || f.tint.abs() > 1e-4 {
        let (t, g) = (f.temperature * 0.2, f.tint * 0.12);
        let _ = write!(out, r#"<feColorMatrix type="matrix" values="1 0 0 0 {:.4}  0 1 0 0 {:.4}  0 0 1 0 {:.4}  0 0 0 1 0"/>"#, t + g * 0.5, -g, -t + g * 0.5);
    }
    if f.highlights.abs() > 1e-4 || f.shadows.abs() > 1e-4 {
        let (lo, hi) = (0.25 + f.shadows * 0.12, 0.75 + f.highlights * 0.12);
        let table = format!("0 {lo:.4} 0.5 {hi:.4} 1");
        let _ = write!(out, r#"<feComponentTransfer><feFuncR type="table" tableValues="{table}"/><feFuncG type="table" tableValues="{table}"/><feFuncB type="table" tableValues="{table}"/></feComponentTransfer>"#);
    }
    out
}

// the pixel size of a base64 image, read from its header
fn image_size(data: &str) -> Option<(f32, f32)> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).ok()?;
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()?;
    Some((w as f32, h as f32))
}

struct Svg<'a> {
    app: &'a App,
    painter: egui::Painter,
    defs: String,
    body: String,
    n: usize,
    leaf: bool, // draw a container's own paint only, not its children (a sprite of one layer)
    // for background blurs: the layers being drawn in z order (what a blur can see behind it), and the colour under all of them
    // (the page colour on the canvas, none in an export). None: no background blur is drawn.
    scope: Option<Vec<usize>>,
    // (frame, fixed): of that frame's own children draw only the ones that are (not) fixed in place while it scrolls
    fixed_split: Option<(u64, bool)>,
    base: Option<Color32>,
    depth: u8, // how many patterns deep the tile being drawn is
    skip_blur: bool, // the layer's own blur is applied around it and its children, not inside its paint
    ids: bool,       // every layer's group gets an id from its name (Export > "Include id attribute")
    names: std::collections::HashMap<String, u32>, // the ids given so far, to keep them unique
}

// a rounded rectangle as path data (corner radii: top-left, top-right, bottom-right, bottom-left)
fn rrect_d(r: Rect, rad: [f32; 4]) -> String {
    let m = (r.width().min(r.height()) / 2.0).max(0.0);
    let [tl, tr, br, bl] = rad.map(|v| v.clamp(0.0, m));
    format!(
        "M{:.2} {:.2}H{:.2}A{tr:.2} {tr:.2} 0 0 1 {:.2} {:.2}V{:.2}A{br:.2} {br:.2} 0 0 1 {:.2} {:.2}H{:.2}A{bl:.2} {bl:.2} 0 0 1 {:.2} {:.2}V{:.2}A{tl:.2} {tl:.2} 0 0 1 {:.2} {:.2}Z",
        r.min.x + tl, r.min.y,
        r.max.x - tr, r.max.x, r.min.y + tr,
        r.max.y - br, r.max.x - br, r.max.y,
        r.min.x + bl, r.min.x, r.max.y - bl,
        r.min.y + tl, r.min.x + tl, r.min.y
    )
}

// an arrow head at `tip` pointing along `dir` (unit), sized from the stroke weight
// the head of an end cap at `tip`; the shapes that are filled are `cap.is_filled_head()`
fn head_d(cap: StrokeCap, tip: egui::Pos2, dir: Vec2, w: f32) -> String {
    let len = (w * 4.0).max(8.0);
    let side = Vec2::new(-dir.y, dir.x);
    match cap {
        StrokeCap::ReverseTriangle => {
            // the base sits on the end point and the tip points back along the line
            let (a, b, t) = (tip + side * len * 0.55, tip - side * len * 0.55, tip - dir * len);
            format!("M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}Z", a.x, a.y, b.x, b.y, t.x, t.y)
        }
        StrokeCap::CircleArrow => {
            let (r, c) = (len * 0.4, tip - dir * len * 0.4);
            format!("M{:.2} {:.2}a{r:.2} {r:.2} 0 1 0 {:.2} 0a{r:.2} {r:.2} 0 1 0 {:.2} 0Z", c.x - r, c.y, r * 2.0, -r * 2.0)
        }
        StrokeCap::DiamondArrow => {
            let (c, h) = (tip - dir * len * 0.5, len * 0.5);
            let (a, b, d) = (tip, c + side * h * 0.8, c - side * h * 0.8);
            let back = tip - dir * len;
            format!("M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}Z", a.x, a.y, b.x, b.y, back.x, back.y, d.x, d.y)
        }
        c => arrow_d(tip, dir, w, c == StrokeCap::TriangleArrow),
    }
}

fn arrow_d(tip: egui::Pos2, dir: Vec2, w: f32, filled: bool) -> String {
    let len = (w * 4.0).max(8.0);
    let side = Vec2::new(-dir.y, dir.x);
    let (a, b) = (tip - dir * len + side * len * 0.55, tip - dir * len - side * len * 0.55);
    if filled {
        format!("M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}Z", tip.x, tip.y, a.x, a.y, b.x, b.y)
    } else {
        format!("M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}", a.x, a.y, tip.x, tip.y, b.x, b.y)
    }
}

impl Shape {
    // the dash pattern of the stroke in px (`dash` alone means Figma's default of 4 and 3 line widths)
    pub fn dash_array(&self) -> Option<Vec<f32>> {
        if self.dash_pattern.iter().any(|v| *v > 0.0) {
            Some(self.dash_pattern.clone())
        } else if self.dash {
            Some(vec![self.stroke_w * 4.0, self.stroke_w * 3.0])
        } else {
            None
        }
    }

    // does this layer need more than egui's mesh painter can do: effects, aligned / capped / joined / dashed-by-pattern /
    // per-side strokes, gradients other than a plain two-stop linear one, and every image?
    pub fn needs_sprite(&self) -> bool {
        if self.kind == Kind::Text || self.kind == Kind::Group {
            return false;
        }
        let visible_stroke = self.stroke_w > 0.0 && self.strokes.iter().any(|p| p.visible);
        self.stretch.is_some()
            || self.effects.iter().any(|e| e.visible)
            || (visible_stroke
                && (self.stroke_align != StrokeAlign::Center
                    || self.width_profile != 0
                    || self.stroke_cap != [StrokeCap::None; 2]
                    || self.stroke_join != StrokeJoin::Miter
                    || !self.dash_pattern.is_empty()
                    || self.dynamic.is_some()
                    || (self.dash && self.dash_cap != 0)
                    || self.stroke_sides.is_some()
                    || self.strokes.iter().any(|p| p.visible && !matches!(p.kind, PaintKind::Solid(_)))))
            || self.fills.iter().any(|f| {
                f.visible
                    && match &f.kind {
                        PaintKind::Image(_) | PaintKind::Pattern(_) | PaintKind::Shader(_) => true,
                        PaintKind::Gradient(g) => !(g.kind == GradientKind::Linear && g.stops.len() == 2),
                        _ => false,
                    }
            })
    }

    // how far the painted result reaches beyond the layer's bounds: stroke, shadows, blur and arrow heads
    pub fn paint_pad(&self) -> f32 {
        let mut pad = 0.0f32;
        if self.stroke_w > 0.0 && self.strokes.iter().any(|p| p.visible) {
            let join = if self.stroke_join == StrokeJoin::Miter { 4.0 } else { 1.0 };
            pad = pad.max(self.stroke_w * join + 2.0).max(if self.stroke_cap.iter().any(|c| c.is_head()) { (self.stroke_w * 4.0).max(8.0) + 4.0 } else { 0.0 });
        }
        if let Some(sb) = &self.stretch {
            pad = pad.max(sb.width);
        }
        for e in self.effects.iter().filter(|e| e.visible) {
            let reach = if e.kind.is_shadow() { e.dx.abs().max(e.dy.abs()) + e.spread.abs() + e.blur * 1.6 } else if e.kind.is_blur() { e.blur.max(e.start_blur) * 1.6 } else { 0.0 };
            pad = pad.max(reach + 2.0);
        }
        pad
    }
}

impl Svg<'_> {
    fn id(&mut self, prefix: &str) -> String {
        self.n += 1;
        format!("{prefix}{}", self.n)
    }

    fn stops(g: &Gradient, opacity: f32) -> String {
        let mut out = String::new();
        for s in &g.stops {
            let (c, a) = col(s.color);
            let _ = write!(out, r#"<stop offset="{:.4}" stop-color="{c}" stop-opacity="{:.4}"/>"#, s.pos.clamp(0.0, 1.0), a * opacity);
        }
        out
    }

    // a paint as an SVG paint server (id) plus opacity, or None when the paint was drawn straight into the body (those that
    // cannot be a paint server: angular and diamond gradients, images). `body_ok` is false where a paint server is the only way
    // (strokes), then those paints fall back to a close paint server.
    fn paint_attr(&mut self, paint: &Paint, bounds: Rect, d: &str, body_ok: bool) -> Option<(String, f32)> {
        match &paint.kind {
            PaintKind::Solid(_) => {
                let (c, a) = col(self.app.paint_color(paint));
                Some((c, a * paint.opacity))
            }
            PaintKind::Linear { a, b, angle } => {
                let g = Gradient { angle: *angle, ..Gradient::between(*a, *b) };
                self.gradient_attr(&g, paint.opacity, bounds, d, body_ok)
            }
            PaintKind::Gradient(g) => self.gradient_attr(g, paint.opacity, bounds, d, body_ok),
            PaintKind::Image(img) => {
                if !body_ok {
                    return Some(("rgb(128,128,128)".into(), paint.opacity));
                }
                self.image(*img, &paint.image, paint.opacity, bounds, d);
                None
            }
            PaintKind::Pattern(pf) => self.pattern_attr(pf, paint.opacity, bounds),
            PaintKind::Shader(sf) => Some((self.shader_attr(sf, bounds), paint.opacity)),
        }
    }

    // a Pattern paint: a pattern element whose tile is the source layer drawn once (with a gap), scaled, turned and shifted
    fn pattern_attr(&mut self, pf: &PatternFill, opacity: f32, bounds: Rect) -> Option<(String, f32)> {
        let Some(src) = pf.source.and_then(|s| self.app.index_of(s)) else { return Some(("rgb(128,128,128)".into(), opacity)) };
        if self.depth > 2 {
            return Some(("rgb(128,128,128)".into(), opacity)); // a layer that is patterned with itself stops here
        }
        let sh = self.app.shapes[src].clone();
        let r = sh.aabb();
        let k = pf.scale.max(0.01);
        let (tw, th) = ((r.width() + pf.space.x) * k, (r.height() + pf.space.y) * k);
        // the source, drawn with its top-left corner at the tile's origin
        let saved = std::mem::take(&mut self.body);
        let saved_leaf = std::mem::replace(&mut self.leaf, false);
        self.depth += 1;
        self.shape(&sh, Vec2::ZERO - r.min.to_vec2());
        self.depth -= 1;
        self.leaf = saved_leaf;
        let tile = std::mem::replace(&mut self.body, saved);
        let id = self.id("pt");
        let c = bounds.center();
        // hexagonal tiles: every other row (column) sits half a tile along, so the cell holds two of them
        let (cell_w, cell_h, tile) = match pf.tile {
            1 => (tw, th * 2.0, format!(r#"<g transform="scale({k})">{tile}</g><g transform="translate({:.2} {:.2}) scale({k})">{tile}</g><g transform="translate({:.2} {:.2}) scale({k})">{tile}</g>"#, tw / 2.0, th, -tw / 2.0, th)),
            2 => (tw * 2.0, th, format!(r#"<g transform="scale({k})">{tile}</g><g transform="translate({:.2} {:.2}) scale({k})">{tile}</g><g transform="translate({:.2} {:.2}) scale({k})">{tile}</g>"#, tw, th / 2.0, tw, -th / 2.0)),
            _ => (tw, th, format!(r#"<g transform="scale({k})">{tile}</g>"#)),
        };
        // where the pattern starts: one of the nine points of the layer's box
        let (col, row) = ((pf.align % 9 % 3) as usize, (pf.align % 9 / 3) as usize);
        let ox = [bounds.min.x, c.x - tw / 2.0, bounds.max.x - tw][col];
        let oy = [bounds.min.y, c.y - th / 2.0, bounds.max.y - th][row];
        let _ = write!(
            self.defs,
            r#"<pattern id="{id}" patternUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{cell_w:.2}" height="{cell_h:.2}" patternTransform="rotate({:.2} {:.2} {:.2})">{tile}</pattern>"#,
            ox + pf.offset.x, oy + pf.offset.y, pf.rotation, c.x, c.y
        );
        Some((format!("url(#{id})"), opacity))
    }

    // a Shader paint as a pattern covering the layer: noise turned into the two colours, or tiles of them
    fn shader_attr(&mut self, sf: &ShaderFill, bounds: Rect) -> String {
        let id = self.id("sh");
        let (w, h) = (bounds.width().max(1.0), bounds.height().max(1.0));
        let k = sf.scale.max(0.05);
        let ((ar, ag, ab), (br, bg, bb)) = ((sf.a.r() as f32 / 255.0, sf.a.g() as f32 / 255.0, sf.a.b() as f32 / 255.0), (sf.b.r() as f32 / 255.0, sf.b.g() as f32 / 255.0, sf.b.b() as f32 / 255.0));
        let ((ca, aa), cb) = (col(sf.a), col(sf.b).0);
        let turn = format!(r#"patternTransform="rotate({:.2} {:.2} {:.2})""#, sf.angle, bounds.center().x, bounds.center().y);
        match sf.kind {
            ShaderKind::Plasma | ShaderKind::Waves | ShaderKind::Noise => {
                let freq = match sf.kind {
                    ShaderKind::Plasma => format!("{:.5}", 0.012 / k),
                    ShaderKind::Waves => format!("{:.5} {:.5}", 0.004 / k, 0.03 / k),
                    _ => format!("{:.5}", 0.8 / k),
                };
                // the noise's red channel (about 0.25 to 0.75) stretched to 0..1, then mapped from colour a to colour b
                let (gain, base) = (2.0, -0.5);
                let row = |a: f32, b: f32| format!("{:.4} 0 0 0 {:.4}", (b - a) * gain, a + (b - a) * base);
                let f = self.id("f");
                let _ = write!(
                    self.defs,
                    r#"<filter id="{f}" filterUnits="userSpaceOnUse" x="0" y="0" width="{w:.2}" height="{h:.2}" color-interpolation-filters="sRGB"><feTurbulence type="fractalNoise" baseFrequency="{freq}" numOctaves="3" seed="4"/><feColorMatrix type="matrix" values="{} {} {} 0 0 0 0 1"/></filter>"#,
                    row(ar, br), row(ag, bg), row(ab, bb)
                );
                let _ = write!(self.defs, r##"<pattern id="{id}" patternUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{w:.2}" height="{h:.2}"><rect x="0" y="0" width="{w:.2}" height="{h:.2}" fill="#000" filter="url(#{f})"/></pattern>"##, bounds.min.x, bounds.min.y);
            }
            ShaderKind::Checker => {
                let s = 24.0 * k;
                let _ = write!(self.defs, r#"<pattern id="{id}" patternUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" {turn}><rect width="{:.2}" height="{:.2}" fill="{ca}" fill-opacity="{aa}"/><rect width="{s:.2}" height="{s:.2}" fill="{cb}"/><rect x="{s:.2}" y="{s:.2}" width="{s:.2}" height="{s:.2}" fill="{cb}"/></pattern>"#, bounds.min.x, bounds.min.y, s * 2.0, s * 2.0, s * 2.0, s * 2.0);
            }
            ShaderKind::Stripes => {
                let s = 12.0 * k;
                let _ = write!(self.defs, r#"<pattern id="{id}" patternUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" {turn}><rect width="{:.2}" height="{:.2}" fill="{ca}" fill-opacity="{aa}"/><rect width="{s:.2}" height="{:.2}" fill="{cb}"/></pattern>"#, bounds.min.x, bounds.min.y, s * 2.0, s * 2.0, s * 2.0, s * 2.0, s * 2.0);
            }
            ShaderKind::Dots => {
                let s = 16.0 * k;
                let _ = write!(self.defs, r#"<pattern id="{id}" patternUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{s:.2}" height="{s:.2}" {turn}><rect width="{s:.2}" height="{s:.2}" fill="{ca}" fill-opacity="{aa}"/><circle cx="{:.2}" cy="{:.2}" r="{:.2}" fill="{cb}"/></pattern>"#, bounds.min.x, bounds.min.y, s / 2.0, s / 2.0, s * 0.3);
            }
            ShaderKind::Grid => {
                let s = 24.0 * k;
                let _ = write!(self.defs, r#"<pattern id="{id}" patternUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{s:.2}" height="{s:.2}" {turn}><rect width="{s:.2}" height="{s:.2}" fill="{ca}" fill-opacity="{aa}"/><path d="M0 0H{s:.2}M0 0V{s:.2}" stroke="{cb}" stroke-width="{:.2}" fill="none"/></pattern>"#, bounds.min.x, bounds.min.y, (1.5 * k).max(0.5));
            }
        }
        format!("url(#{id})")
    }

    fn gradient_attr(&mut self, g: &Gradient, opacity: f32, bounds: Rect, d: &str, body_ok: bool) -> Option<(String, f32)> {
        let id = self.id("g");
        let (w, h) = (bounds.width().max(1e-3), bounds.height().max(1e-3));
        let c = bounds.min + Vec2::new(g.center.x * w, g.center.y * h);
        match g.kind {
            GradientKind::Linear => {
                let (s, co) = g.angle.to_radians().sin_cos();
                let half = (w * co.abs() + h * s.abs()) / 2.0 * g.length.max(1e-3);
                let (ctr, dx, dy) = (c, co * half, s * half);
                let _ = write!(
                    self.defs,
                    r#"<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}">{}</linearGradient>"#,
                    ctr.x - dx, ctr.y - dy, ctr.x + dx, ctr.y + dy, Self::stops(g, 1.0)
                );
                Some((format!("url(#{id})"), opacity))
            }
            GradientKind::Radial => {
                let (rx, ry) = (w / 2.0 * g.scale.x.max(1e-3), h / 2.0 * g.scale.y.max(1e-3));
                let _ = write!(
                    self.defs,
                    r#"<radialGradient id="{id}" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="1" gradientTransform="translate({:.2} {:.2}) scale({:.2} {:.2})">{}</radialGradient>"#,
                    c.x, c.y, rx, ry, Self::stops(g, 1.0)
                );
                Some((format!("url(#{id})"), opacity))
            }
            GradientKind::Diamond | GradientKind::Angular if !body_ok => {
                let last = g.stops.last().map_or(Color32::GRAY, |s| s.color);
                let (cc, a) = col(last);
                Some((cc, a * opacity))
            }
            GradientKind::Diamond => {
                // four quadrants, each a linear gradient along the diagonal that reaches 1 on the diamond's edge
                let (rx, ry) = (w / 2.0 * g.scale.x.max(1e-3), h / 2.0 * g.scale.y.max(1e-3));
                let clip = self.id("c");
                let _ = write!(self.defs, r#"<clipPath id="{clip}"><path d="{d}" clip-rule="evenodd"/></clipPath>"#);
                let _ = write!(self.body, r#"<g clip-path="url(#{clip})" opacity="{opacity}">"#);
                for (sx, sy) in [(1.0f32, 1.0f32), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                    // t = (|dx|/rx + |dy|/ry); along (sx/rx, sy/ry) the gradient vector's end (t = 1) is v / |v|^2
                    let v = Vec2::new(sx / rx, sy / ry);
                    let end = c + v / v.length_sq();
                    let gid = self.id("g");
                    let _ = write!(
                        self.defs,
                        r#"<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}">{}</linearGradient>"#,
                        c.x, c.y, end.x, end.y, Self::stops(g, 1.0)
                    );
                    let q = Rect::from_two_pos(c, Pos2::new(if sx > 0.0 { bounds.max.x + 1.0 } else { bounds.min.x - 1.0 }, if sy > 0.0 { bounds.max.y + 1.0 } else { bounds.min.y - 1.0 }));
                    let _ = write!(self.body, r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="url(#{gid})"/>"#, q.min.x, q.min.y, q.width(), q.height());
                }
                self.body.push_str("</g>");
                None
            }
            GradientKind::Angular => {
                // many thin wedges from the centre, each in the colour at its angle
                let clip = self.id("c");
                let _ = write!(self.defs, r#"<clipPath id="{clip}"><path d="{d}" clip-rule="evenodd"/></clipPath>"#);
                let _ = write!(self.body, r#"<g clip-path="url(#{clip})" opacity="{opacity}"><g shape-rendering="crispEdges">"#);
                let reach = (w + h) * 2.0 + 10.0;
                let n = 360;
                for k in 0..n {
                    let (a0, a1) = (k as f32 / n as f32, (k as f32 + 1.0) / n as f32);
                    let (c0, c1) = ((a0 * std::f32::consts::TAU) + g.angle.to_radians(), (a1 * std::f32::consts::TAU) + g.angle.to_radians() + 0.012);
                    let (p0, p1) = (c + Vec2::new(c0.cos(), c0.sin()) * reach, c + Vec2::new(c1.cos(), c1.sin()) * reach);
                    let (cc, aa) = col(g.at((a0 + a1) / 2.0));
                    let _ = write!(self.body, r#"<path d="M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}Z" fill="{cc}" fill-opacity="{aa}"/>"#, c.x, c.y, p0.x, p0.y, p1.x, p1.y);
                }
                self.body.push_str("</g></g>");
                None
            }
        }
    }

    // an image paint clipped to the shape: Fill covers the box, Fit shows all of it, Tile repeats it; quarter turns and the
    // adjustments (a colour-matrix filter) apply
    fn image(&mut self, img: u64, fill: &ImageFill, opacity: f32, bounds: Rect, d: &str) {
        let Some(asset) = self.app.images.iter().find(|a| a.id == img) else { return };
        let mime = match base64::engine::general_purpose::STANDARD.decode(asset.data.get(..8).unwrap_or("")).ok().as_deref() {
            Some([0xff, 0xd8, ..]) => "image/jpeg",
            _ => "image/png",
        };
        let clip = self.id("c");
        let _ = write!(self.defs, r#"<clipPath id="{clip}"><path d="{d}" clip-rule="evenodd"/></clipPath>"#);
        let mut attrs = String::new();
        if fill.adjusted() {
            let f = self.id("a");
            let _ = write!(self.defs, r#"<filter id="{f}" color-interpolation-filters="sRGB">{}</filter>"#, adjust_filter(fill));
            let _ = write!(attrs, r#" filter="url(#{f})""#);
        }
        let c = bounds.center();
        let turned = fill.rotation % 180 != 0;
        let (bw, bh) = if turned { (bounds.height(), bounds.width()) } else { (bounds.width(), bounds.height()) };
        let rot = if fill.rotation % 360 != 0 { format!(r#" transform="rotate({} {:.2} {:.2})""#, fill.rotation, c.x, c.y) } else { String::new() };
        let (x, y) = (c.x - bw / 2.0, c.y - bh / 2.0);
        let data = format!("data:{mime};base64,{}", asset.data);
        if fill.mode == ImageMode::Tile {
            // the tile is the image at `scale` of its own size
            let (iw, ih) = image_size(&asset.data).unwrap_or((bw, bh));
            let (tw, th) = ((iw * fill.scale.max(0.01)).max(1.0), (ih * fill.scale.max(0.01)).max(1.0));
            let pat = self.id("p");
            let _ = write!(
                self.defs,
                r#"<pattern id="{pat}" patternUnits="userSpaceOnUse" x="{x:.2}" y="{y:.2}" width="{tw:.2}" height="{th:.2}"><image width="{tw:.2}" height="{th:.2}" preserveAspectRatio="none" href="{data}"{attrs}/></pattern>"#
            );
            let _ = write!(self.body, r#"<g clip-path="url(#{clip})" opacity="{opacity}"><rect x="{x:.2}" y="{y:.2}" width="{bw:.2}" height="{bh:.2}" fill="url(#{pat})"{rot}/></g>"#);
        } else if fill.mode == ImageMode::Crop {
            // only the cropped part of the image shows, stretched over the layer
            let [cx0, cy0, cx1, cy1] = fill.crop;
            let (iw, ih) = image_size(&asset.data).unwrap_or((bw, bh));
            let _ = iw;
            let _ = ih;
            let (cw, ch) = ((cx1 - cx0).max(0.01), (cy1 - cy0).max(0.01));
            let (full_w, full_h) = (bw / cw, bh / ch);
            let (ix, iy) = (x - cx0 * full_w, y - cy0 * full_h);
            let _ = write!(
                self.body,
                r#"<g clip-path="url(#{clip})" opacity="{opacity}"><image x="{ix:.2}" y="{iy:.2}" width="{full_w:.2}" height="{full_h:.2}" preserveAspectRatio="none" href="{data}"{rot}{attrs}/></g>"#
            );
        } else {
            let fit = if fill.mode == ImageMode::Fit { "xMidYMid meet" } else { "xMidYMid slice" };
            let _ = write!(
                self.body,
                r#"<g clip-path="url(#{clip})" opacity="{opacity}"><image x="{x:.2}" y="{y:.2}" width="{bw:.2}" height="{bh:.2}" preserveAspectRatio="{fit}" href="{data}"{rot}{attrs}/></g>"#
            );
        }
    }

    // a background blur: what is behind the layer (the layers drawn before it, and the base colour), blurred and cut to the layer's
    // outline `d`, drawn under the layer's own paint. Only where the builder knows the z order (`scope`).
    fn backdrop(&mut self, s: &Shape, d: &str, bounds: Rect, sigma: f32, glass: Option<&Effect>) {
        let Some(scope) = self.scope.clone() else { return };
        let Some(idx) = self.app.index_of(s.id) else { return };
        let Some(pos) = scope.iter().position(|&k| k == idx) else { return };
        if pos == 0 && self.base.is_none() {
            return; // nothing behind it in this export
        }
        // glass bends what is behind it near the edge: how far (Refraction, as a share of the Depth band) and by how much the
        // colours part (Dispersion)
        let bend = glass.map(|g| glass_bend(g, bounds));
        let reach = bend.map_or(0.0, |(_, max, ..)| max);
        let region = bounds.expand(sigma * 3.0 + 2.0 + reach);
        let (clip, blur) = (self.id("c"), self.id("f"));
        let _ = write!(self.defs, r#"<clipPath id="{clip}"><path d="{d}" clip-rule="evenodd"/></clipPath>"#);
        let _ = write!(
            self.defs,
            r#"<filter id="{blur}" filterUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="{sigma:.3}" edgeMode="duplicate"/></filter>"#,
            region.min.x, region.min.y, region.width(), region.height()
        );
        // with refraction the backdrop is drawn to pixels, shifted there, and put in as a picture (still blurred by the filter)
        if let Some((depth_px, max, disperse, splay)) = bend.filter(|&(_, max, ..)| max >= 0.25) {
            let k = (2.0f32).min(2048.0 / region.width().max(region.height()).max(1.0));
            let (w, h) = ((region.width() * k).ceil().max(4.0) as u32, (region.height() * k).ceil().max(4.0) as u32);
            let (saved_defs, saved_body) = (std::mem::take(&mut self.defs), std::mem::take(&mut self.body));
            self.backdrop_content(&scope, pos, region);
            let (content_defs, content) = (std::mem::replace(&mut self.defs, saved_defs), std::mem::replace(&mut self.body, saved_body));
            let doc = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{} {} {} {}"><defs>{content_defs}</defs>{content}</svg>"#,
                region.min.x, region.min.y, region.width(), region.height()
            );
            if let Some(uri) = bend_backdrop(&doc, d, region, (w, h), k, depth_px, max, disperse, splay) {
                let _ = write!(
                    self.body,
                    r#"<g clip-path="url(#{clip})"><g filter="url(#{blur})"><image x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" preserveAspectRatio="none" href="{uri}"/></g></g>"#,
                    region.min.x, region.min.y, region.width(), region.height()
                );
                return;
            }
        }
        let _ = write!(self.body, r#"<g clip-path="url(#{clip})"><g filter="url(#{blur})">"#);
        self.backdrop_content(&scope, pos, region);
        self.body.push_str("</g></g>");
    }

    // what is behind the layer at `pos` in `scope`: the base colour, then every earlier layer's own paint (no blur inside the blur)
    fn backdrop_content(&mut self, scope: &[usize], pos: usize, region: Rect) {
        if let Some(base) = self.base {
            let (c, a) = col(base);
            let _ = write!(self.body, r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{c}" fill-opacity="{a}"/>"#, region.min.x, region.min.y, region.width(), region.height());
        }
        let was_leaf = std::mem::replace(&mut self.leaf, true);
        let saved = self.scope.take();
        for &k in &scope[..pos] {
            let other = self.app.shapes[k].clone();
            if other.mask || self.app.chain(k).iter().any(|&c| !self.app.shapes[c].visible) {
                continue;
            }
            self.shape(&other, self.app.origin_of(other.parent));
        }
        self.scope = saved;
        self.leaf = was_leaf;
    }

    // the layer's own paint: effects, fills, strokes (not its children)
    // a stretch brush stroke: the picture cut into thin slices along its width, each slice placed on the piece of the path it belongs
    // to (a transform per slice, so the picture bends with the stroke)
    fn stretch_stroke(&mut self, s: &Shape, sb: &StretchBrush, o: Vec2) {
        let Some(asset) = self.app.images.iter().find(|a| a.id == sb.image) else { return };
        let Some((iw, ih)) = image_size(&asset.data) else { return };
        let mime = match base64::engine::general_purpose::STANDARD.decode(asset.data.get(..8).unwrap_or("")).ok().as_deref() {
            Some([0xff, 0xd8, ..]) => "image/jpeg",
            _ => "image/png",
        };
        let Some(line) = s.polys(2.0).into_iter().find(|p| p.pts.len() >= 2) else { return };
        let pts: Vec<egui::Pos2> = line.pts.iter().map(|p| *p + o).collect();
        let mut along = vec![0.0f32];
        for w in pts.windows(2) {
            along.push(along.last().unwrap() + (w[1] - w[0]).length());
        }
        let total = *along.last().unwrap();
        if total < 1e-3 {
            return;
        }
        // the point and direction at a distance along the stroke
        let at = |d: f32| -> (egui::Pos2, Vec2) {
            let d = d.clamp(0.0, total);
            let k = along.partition_point(|&a| a < d).clamp(1, pts.len() - 1);
            let (a, b) = (pts[k - 1], pts[k]);
            let seg = (along[k] - along[k - 1]).max(1e-6);
            let t = (d - along[k - 1]) / seg;
            (a + (b - a) * t, (b - a) / (b - a).length().max(1e-6))
        };
        let img = self.id("si");
        let _ = write!(self.defs, r#"<image id="{img}" width="{iw}" height="{ih}" preserveAspectRatio="none" href="data:{mime};base64,{}"/>"#, asset.data);
        let n = ((total / 3.0).ceil() as usize).clamp(2, 600);
        for i in 0..n {
            let (u0, u1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
            let (p0, d0) = at(u0 * total);
            let (p1, _) = at(u1 * total);
            let len = (p1 - p0).length().max(1e-4);
            // the slice runs along the direction of its own piece; the image's x maps to the distance, its y across the stroke
            let dir = (p1 - p0) / len;
            let _ = d0;
            let nrm = Vec2::new(-dir.y, dir.x);
            let sx = len / ((u1 - u0) * iw);
            let sy = sb.width / ih;
            let origin = p0 - dir * (u0 * iw * sx) - nrm * (sb.width / 2.0);
            let clip = self.id("sc");
            let _ = write!(self.defs, r#"<clipPath id="{clip}"><rect x="{:.3}" y="0" width="{:.3}" height="{ih}"/></clipPath>"#, u0 * iw - 0.6, (u1 - u0) * iw + 1.2);
            let _ = write!(
                self.body,
                r##"<g transform="matrix({:.5} {:.5} {:.5} {:.5} {:.3} {:.3})"><g clip-path="url(#{clip})"><use href="#{img}"/></g></g>"##,
                dir.x * sx, dir.y * sx, nrm.x * sy, nrm.y * sy, origin.x, origin.y
            );
        }
    }

    fn paint_shape(&mut self, s: &Shape, o: Vec2) {
        if let Some(sb) = &s.stretch {
            self.stretch_stroke(s, sb, o);
            return;
        }
        let polys = s.polys(8.0);
        let d = path_d(&polys, o);
        let bounds = s.aabb().translate(o);
        let effects: Vec<Effect> = s.effects.iter().copied().filter(|e| e.visible).collect();
        let area = s.has_area();
        // the region effects may touch
        let pad = s.paint_pad() + 4.0;
        let region = bounds.expand(pad);
        let filter_box = |r: Rect| format!(r#"filterUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}""#, r.min.x, r.min.y, r.width(), r.height());
        // a background blur is the lowest thing of the layer
        if let Some(e) = effects.iter().find(|e| e.kind == EffectKind::BackgroundBlur).filter(|_| area) {
            self.backdrop(s, &d, bounds, e.blur / 2.0, None);
        }
        // glass: the blurred backdrop, a faint white tint, and a light edge that fades away from the light
        for e in effects.iter().filter(|e| e.kind == EffectKind::Glass && area) {
            self.backdrop(s, &d, bounds, e.blur / 2.0, Some(e));
            let (clip, grad) = (self.id("c"), self.id("g"));
            let _ = write!(self.defs, r#"<clipPath id="{clip}"><path d="{d}" clip-rule="evenodd"/></clipPath>"#);
            let a = e.angle.to_radians();
            let (c, half) = (bounds.center(), bounds.width().max(bounds.height()) / 2.0);
            let dir = Vec2::new(a.sin(), -a.cos());
            let k = (e.density / 100.0).clamp(0.0, 1.0);
            let _ = write!(
                self.defs,
                r##"<linearGradient id="{grad}" gradientUnits="userSpaceOnUse" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"><stop offset="0" stop-color="#fff" stop-opacity="{:.3}"/><stop offset="0.5" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#fff" stop-opacity="{:.3}"/></linearGradient>"##,
                c.x + dir.x * half, c.y + dir.y * half, c.x - dir.x * half, c.y - dir.y * half, 0.9 * k, 0.3 * k
            );
            let _ = write!(self.body, r##"<path d="{d}" fill="#fff" fill-opacity="{:.3}" fill-rule="evenodd"/>"##, 0.1 * k);
            let _ = write!(self.body, r##"<g clip-path="url(#{clip})"><path d="{d}" fill="none" stroke="url(#{grad})" stroke-width="{:.2}" stroke-linejoin="round"/></g>"##, (e.radius * 0.25).max(1.0) * 2.0);
        }
        // drop shadows go under everything
        for e in effects.iter().filter(|e| e.kind == EffectKind::DropShadow && area) {
            let f = self.id("f");
            let (c, a) = col(e.color);
            let morph = if e.spread.abs() > 1e-3 { format!(r#"<feMorphology in="SourceAlpha" operator="{}" radius="{:.2}" result="m"/>"#, if e.spread > 0.0 { "dilate" } else { "erode" }, e.spread.abs()) } else { String::new() };
            let src = if morph.is_empty() { "SourceAlpha" } else { "m" };
            let _ = write!(
                self.defs,
                r##"<filter id="{f}" {} color-interpolation-filters="sRGB">{morph}<feGaussianBlur in="{src}" stdDeviation="{:.3}" result="b"/><feOffset in="b" dx="{}" dy="{}" result="o"/><feFlood flood-color="{c}" flood-opacity="{a}" result="c"/><feComposite in="c" in2="o" operator="in"/></filter>"##,
                filter_box(region), e.blur / 2.0, e.dx, e.dy
            );
            let shadow_at = self.body.len();
            if e.show_behind {
                let _ = write!(self.body, r##"<path d="{d}" fill="#000" fill-rule="evenodd" filter="url(#{f})"/>"##);
            } else {
                // a shadow does not show through the layer itself
                let m = self.id("m");
                let _ = write!(
                    self.defs,
                    r##"<mask id="{m}" maskUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="#fff"/><path d="{d}" fill="#000" fill-rule="evenodd"/></mask>"##,
                    region.min.x, region.min.y, region.width(), region.height(), region.min.x, region.min.y, region.width(), region.height()
                );
                let _ = write!(self.body, r##"<g mask="url(#{m})"><path d="{d}" fill="#000" fill-rule="evenodd" filter="url(#{f})"/></g>"##);
            }
            self.blend_since(shadow_at, e.blend);
        }
        // a layer blur wraps what follows; a progressive one is built after the layer is painted
        let progressive = effects.iter().find(|e| e.kind == EffectKind::LayerBlur && e.progressive).copied();
        let content_start = self.body.len();
        let blur = effects.iter().find(|e| e.kind == EffectKind::LayerBlur && !e.progressive && !self.skip_blur).map(|e| e.blur);
        if let Some(b) = blur {
            let f = self.id("f");
            let _ = write!(self.defs, r#"<filter id="{f}" {} color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="{:.3}"/></filter>"#, filter_box(region.expand(b * 1.6)), b / 2.0);
            let _ = write!(self.body, r#"<g filter="url(#{f})">"#);
        }
        if area {
            // Fill > Show in exports: a frame's own fills stay on the canvas but are left out of an export
            let hide_bg = s.no_bg_export && s.kind == Kind::Frame && self.base.is_none();
            for f in s.fills.iter().filter(|f| f.visible && !hide_bg) {
                let at = self.body.len();
                if let Some((c, a)) = self.paint_attr(f, bounds, &d, true) {
                    let _ = write!(self.body, r#"<path d="{d}" fill="{c}" fill-opacity="{a}" fill-rule="evenodd"/>"#);
                }
                self.blend_since(at, f.blend);
            }
            // shader effects: a procedural pattern over the fills, clipped to the shape or covering its box, blurred by Softness
            for sh in s.shaders.iter().filter(|x| x.visible) {
                let paint = self.shader_attr(&sh.fill, bounds);
                let at = self.body.len();
                let rect_d = format!("M{:.2} {:.2}H{:.2}V{:.2}H{:.2}Z", bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y, bounds.min.x);
                let shape_d = if sh.clip_alpha { d.to_string() } else { rect_d };
                let filt = if sh.softness > 0.0 {
                    let f = self.id("f");
                    let _ = write!(self.defs, r#"<filter id="{f}" {} color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="{:.3}"/></filter>"#, filter_box(region.expand(sh.softness * 0.5)), sh.softness / 10.0);
                    format!(r#" filter="url(#{f})""#)
                } else {
                    String::new()
                };
                let _ = write!(self.body, r#"<path d="{shape_d}" fill="{paint}" fill-opacity="{:.3}" fill-rule="evenodd"{filt}/>"#, sh.opacity);
                self.blend_since(at, sh.blend);
            }
            for e in effects.iter().filter(|e| e.kind == EffectKind::InnerShadow) {
                let f = self.id("f");
                let (c, a) = col(e.color);
                let morph = if e.spread.abs() > 1e-3 { format!(r#"<feMorphology in="inv" operator="dilate" radius="{:.2}" result="inv"/>"#, e.spread.max(0.0)) } else { String::new() };
                let _ = write!(
                    self.defs,
                    r##"<filter id="{f}" {} color-interpolation-filters="sRGB"><feComponentTransfer in="SourceAlpha" result="inv"><feFuncA type="table" tableValues="1 0"/></feComponentTransfer>{morph}<feGaussianBlur in="inv" stdDeviation="{:.3}" result="b"/><feOffset in="b" dx="{}" dy="{}" result="o"/><feFlood flood-color="{c}" flood-opacity="{a}" result="c"/><feComposite in="c" in2="o" operator="in" result="sh"/><feComposite in="sh" in2="SourceAlpha" operator="in"/></filter>"##,
                    filter_box(region), e.blur / 2.0, e.dx, e.dy
                );
                let _ = write!(self.body, r##"<path d="{d}" fill="#000" fill-rule="evenodd" filter="url(#{f})"/>"##);
            }
        }
        if area {
            for e in effects.iter().filter(|e| matches!(e.kind, EffectKind::Noise | EffectKind::Texture)) {
                self.grain(e, &d, region, bounds);
            }
        }
        if s.stroke_w > 0.0 {
            for st in s.strokes.iter().filter(|p| p.visible) {
                let at = self.body.len();
                self.stroke(s, st, &polys, &d, bounds, o);
                self.blend_since(at, st.blend);
            }
        }
        if blur.is_some() {
            self.body.push_str("</g>");
        }
        if let Some(e) = progressive {
            self.progressive_blur(&e, content_start, region, bounds);
        }
    }

    // what was written to the body since `at` blends with what is under it in `mode` (a plain group when the mode is Normal)
    fn blend_since(&mut self, at: usize, mode: BlendMode) {
        if mode.is_normal() || at >= self.body.len() {
            return;
        }
        let seg = self.body.split_off(at);
        let _ = write!(self.body, r#"<g style="mix-blend-mode:{}">{seg}</g>"#, mode.css());
    }

    // Noise and Texture: a turbulence pattern drawn over the layer's fill (Texture can spill over its bounds when not clipped)
    fn grain(&mut self, e: &Effect, d: &str, region: Rect, bounds: Rect) {
        let freq = 1.0 / e.size.max(0.5);
        let seed = 7;
        let r = region;
        let filter_box = format!(r#"filterUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}""#, r.min.x, r.min.y, r.width(), r.height());
        if e.kind == EffectKind::Texture {
            let f = self.id("f");
            let blur = if e.radius > 0.05 { format!(r#"<feGaussianBlur stdDeviation="{:.2}"/>"#, e.radius / 2.0) } else { String::new() };
            let clip = if e.show_behind { r#"<feComposite in2="SourceAlpha" operator="in"/>"# } else { "" };
            // grey grain at 30% so it lightens and darkens what is below
            let _ = write!(
                self.defs,
                r##"<filter id="{f}" {filter_box} color-interpolation-filters="sRGB"><feTurbulence type="fractalNoise" baseFrequency="{freq:.4}" numOctaves="2" seed="{seed}"/><feColorMatrix type="matrix" values="1.6 0 0 0 -0.3 1.6 0 0 0 -0.3 1.6 0 0 0 -0.3 0 0 0 0 0.35"/>{blur}{clip}</filter>"##
            );
            if e.show_behind {
                let _ = write!(self.body, r##"<path d="{d}" fill="#000" fill-rule="evenodd" filter="url(#{f})"/>"##);
            } else {
                let _ = write!(self.body, r##"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="#000" filter="url(#{f})"/>"##, bounds.min.x, bounds.min.y, bounds.width(), bounds.height());
            }
            return;
        }
        // Noise: dots where the turbulence is above a threshold set by the density; multi uses the noise's own colours
        let density = (e.density / 100.0).clamp(0.0, 1.0);
        let passes: Vec<(String, f32)> = match e.mode {
            1 => vec![(format!("{}", 1), density / 2.0), (format!("{}", 2), density / 2.0)],
            _ => vec![(format!("{}", 1), density)],
        };
        for (n, (which, dens)) in passes.iter().enumerate() {
            let f = self.id("f");
            let c = if e.mode == 1 && which == "2" { e.color2 } else { e.color };
            let [cr, cg, cb, ca] = c.to_srgba_unmultiplied();
            let (cr, cg, cb) = (cr as f32 / 255.0, cg as f32 / 255.0, cb as f32 / 255.0);
            // threshold on the red channel; duo's second pass looks at the dark end of the same noise
            let (k, t) = if n == 0 { (60.0, -60.0 * (1.0 - dens)) } else { (-60.0, 60.0 * dens) };
            let tint = if e.mode == 2 { "1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 0 1".to_owned() } else { format!("0 0 0 0 {cr:.3} 0 0 0 0 {cg:.3} 0 0 0 0 {cb:.3} 0 0 0 0 1") };
            let alpha = ca as f32 / 255.0;
            let _ = write!(
                self.defs,
                r##"<filter id="{f}" {filter_box} color-interpolation-filters="sRGB"><feTurbulence type="fractalNoise" baseFrequency="{freq:.4}" numOctaves="1" seed="{}" result="n"/><feColorMatrix in="n" type="matrix" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 {k:.1} 0 0 0 {t:.2}" result="m"/><feColorMatrix in="n" type="matrix" values="{tint}" result="col"/><feComposite in="col" in2="m" operator="in" result="dots"/><feComponentTransfer in="dots" result="d2"><feFuncA type="linear" slope="{alpha:.3}"/></feComponentTransfer><feComposite in="d2" in2="SourceAlpha" operator="in"/></filter>"##,
                seed + n as u32
            );
            let _ = write!(self.body, r##"<path d="{d}" fill="#000" fill-rule="evenodd" filter="url(#{f})"/>"##);
        }
    }

    // a progressive layer blur: the painted layer is repeated at growing blur levels, each shown through a soft band of a gradient
    // that runs from `from` % to `to` % of the layer along the angle
    fn progressive_blur(&mut self, e: &Effect, start: usize, region: Rect, bounds: Rect) {
        const LEVELS: usize = 10;
        let content = self.body.split_off(start);
        let cid = self.id("pc");
        let _ = write!(self.body, r#"<defs><g id="{cid}">{content}</g></defs>"#);
        let a = e.angle.to_radians();
        let dir = Vec2::new(-a.sin(), a.cos()); // angle 0: down
        let c = bounds.center();
        let half = (bounds.width() * dir.x.abs() + bounds.height() * dir.y.abs()) / 2.0;
        let (p0, p1) = (c - dir * half, c + dir * half);
        let (t0, t1) = ((e.from / 100.0).clamp(0.0, 1.0), (e.to / 100.0).clamp(0.0, 1.0));
        let (lo, hi) = (e.start_blur.min(e.blur), e.start_blur.max(e.blur));
        let _ = (lo, hi);
        for i in 0..LEVELS {
            let f = i as f32 / (LEVELS - 1) as f32;
            let sigma = (e.start_blur + (e.blur - e.start_blur) * f) / 2.0;
            // the band of this level along the gradient: full before the first and after the last level
            let pos = |j: f32| t0 + (t1 - t0) * j / (LEVELS - 1) as f32;
            let mut stops = String::new();
            let prev = if i == 0 { None } else { Some(pos(i as f32 - 1.0)) };
            let next = if i == LEVELS - 1 { None } else { Some(pos(i as f32 + 1.0)) };
            let mid = pos(i as f32);
            let mut push = |o: f32, v: f32| {
                let _ = write!(stops, r##"<stop offset="{:.4}" stop-color="#fff" stop-opacity="{v}"/>"##, o.clamp(0.0, 1.0));
            };
            match (prev, next) {
                (None, Some(n)) => {
                    push(0.0, 1.0);
                    push(mid, 1.0);
                    push(n, 0.0);
                    push(1.0, 0.0);
                }
                (Some(p), None) => {
                    push(0.0, 0.0);
                    push(p, 0.0);
                    push(mid, 1.0);
                    push(1.0, 1.0);
                }
                (Some(p), Some(n)) => {
                    push(0.0, 0.0);
                    push(p, 0.0);
                    push(mid, 1.0);
                    push(n, 0.0);
                    push(1.0, 0.0);
                }
                (None, None) => push(0.0, 1.0),
            }
            // offsets must not decrease for an SVG gradient, so reversed ranges (from > to) are flipped by swapping the end points
            let (g0, g1) = if t1 >= t0 { (p0, p1) } else { (p1, p0) };
            let gid = self.id("pg");
            let mid_id = self.id("pm");
            let _ = write!(
                self.defs,
                r##"<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}">{stops}</linearGradient><mask id="{mid_id}" maskUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="url(#{gid})"/></mask>"##,
                g0.x, g0.y, g1.x, g1.y, region.min.x, region.min.y, region.width(), region.height(), region.min.x, region.min.y, region.width(), region.height()
            );
            let filter = if sigma > 0.01 {
                let fid = self.id("f");
                let _ = write!(self.defs, r#"<filter id="{fid}" filterUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="{sigma:.3}"/></filter>"#, region.min.x - sigma * 3.0, region.min.y - sigma * 3.0, region.width() + sigma * 6.0, region.height() + sigma * 6.0);
                format!(r#" filter="url(#{fid})""#)
            } else {
                String::new()
            };
            let _ = write!(self.body, r##"<g mask="url(#{mid_id})"><g{filter}><use href="#{cid}"/></g></g>"##);
        }
    }

    // one stroke paint of the layer, with its alignment, caps, joins, dashes and (rectangles) per-side widths
    fn stroke(&mut self, s: &Shape, paint: &Paint, polys: &[Poly], d: &str, bounds: Rect, o: Vec2) {
        let w = s.stroke_w;
        // Stroke > Dynamic: the stroke follows a wiggling copy of the outline
        let wiggled = s.dynamic.as_ref().map(|dy| crate::geom::dynamic_polys(polys, dy));
        let wiggled_d = wiggled.as_ref().map(|p| path_d(p, o));
        let polys = wiggled.as_deref().unwrap_or(polys);
        let d = wiggled_d.as_deref().unwrap_or(d);
        // per-side widths: a ring between the outer box and the box moved in by each side's width
        if let (Some(sides), true) = (s.stroke_sides, matches!(s.kind, Kind::Rect | Kind::Frame)) {
            let [t, r, b, l] = sides;
            let rect = s.rect().translate(o);
            let (outer, inner) = match s.stroke_align {
                StrokeAlign::Inside => (rect, Rect::from_min_max(rect.min + Vec2::new(l, t), rect.max - Vec2::new(r, b))),
                StrokeAlign::Outside => (Rect::from_min_max(rect.min - Vec2::new(l, t), rect.max + Vec2::new(r, b)), rect),
                StrokeAlign::Center => (Rect::from_min_max(rect.min - Vec2::new(l, t) / 2.0, rect.max + Vec2::new(r, b) / 2.0), Rect::from_min_max(rect.min + Vec2::new(l, t) / 2.0, rect.max - Vec2::new(r, b) / 2.0)),
            };
            if inner.width() > 0.0 && inner.height() > 0.0 {
                let grow = if s.stroke_align == StrokeAlign::Outside { t.max(r).max(b).max(l) } else { 0.0 };
                let shrink = if s.stroke_align == StrokeAlign::Inside { t.max(r).max(b).max(l) } else { 0.0 };
                let out_r = s.radius.map(|v| v + grow);
                let in_r = s.radius.map(|v| (v - shrink).max(0.0));
                let ring = format!("{}{}", rrect_d(outer, out_r), rrect_d(inner, in_r));
                let c = s.center() + o;
                let rot = if s.rot() != 0.0 { format!(r#" transform="rotate({} {:.2} {:.2})""#, s.rotation, c.x, c.y) } else { String::new() };
                if let Some((fill, a)) = self.paint_attr(paint, bounds, &ring, false) {
                    let _ = write!(self.body, r#"<path d="{ring}" fill="{fill}" fill-opacity="{a}" fill-rule="evenodd"{rot}/>"#);
                }
            }
            return;
        }
        // a width profile: each open line becomes a filled outline whose width follows the profile
        if s.width_profile != 0 && polys.iter().any(|p| !p.closed) {
            if let Some((fill, a)) = self.paint_attr(paint, bounds, d, false) {
                for p in polys.iter().filter(|p| !p.closed) {
                    if let Some(out) = crate::geom::variable_width_outline(&p.pts, w, &|t| s.width_at(t)) {
                        let mut od = out.iter().enumerate().map(|(k, q)| format!("{}{:.2} {:.2}", if k == 0 { "M" } else { "L" }, q.x + o.x, q.y + o.y)).collect::<Vec<_>>().join(" ");
                        od.push('Z');
                        let _ = write!(self.body, r#"<path d="{od}" fill="{fill}" fill-opacity="{a}"/>"#);
                    }
                }
            }
            return;
        }
        let Some((c, a)) = self.paint_attr(paint, bounds, d, false) else { return };
        let closed = !polys.is_empty() && polys.iter().all(|p| p.closed) && s.has_area();
        let align = if closed { s.stroke_align } else { StrokeAlign::Center };
        let width = if align == StrokeAlign::Center { w } else { w * 2.0 };
        let dash = s.dash_array().map(|v| format!(r#" stroke-dasharray="{}""#, v.iter().map(|x| format!("{x:.2}")).collect::<Vec<_>>().join(" "))).unwrap_or_default();
        let join = match s.stroke_join {
            StrokeJoin::Miter => "miter",
            StrokeJoin::Bevel => "bevel",
            StrokeJoin::Round => "round",
        };
        let limit = 1.0 / (s.miter_angle.clamp(1.0, 90.0).to_radians() / 2.0).sin();
        let cap = s.stroke_cap.iter().find_map(|c| match c {
            StrokeCap::Round => Some("round"),
            StrokeCap::Square => Some("square"),
            _ => None,
        });
        // dashes have ends of their own (Stroke > Dash cap), also on a closed outline
        let cap = if s.dash_array().is_some() && s.dash_cap != 0 {
            format!(r#" stroke-linecap="{}""#, if s.dash_cap == 1 { "round" } else { "square" })
        } else if closed {
            String::new()
        } else {
            format!(r#" stroke-linecap="{}""#, cap.unwrap_or("butt"))
        };
        let mut group = String::new();
        let mut close = "";
        match align {
            StrokeAlign::Inside => {
                let id = self.id("c");
                let _ = write!(self.defs, r#"<clipPath id="{id}"><path d="{d}" clip-rule="evenodd"/></clipPath>"#);
                let _ = write!(group, r#"<g clip-path="url(#{id})">"#);
                close = "</g>";
            }
            StrokeAlign::Outside => {
                let id = self.id("m");
                let r = bounds.expand(w * 4.0 + 4.0);
                let _ = write!(
                    self.defs,
                    r##"<mask id="{id}" maskUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="#fff"/><path d="{d}" fill="#000" fill-rule="evenodd"/></mask>"##,
                    r.min.x, r.min.y, r.width(), r.height(), r.min.x, r.min.y, r.width(), r.height()
                );
                let _ = write!(group, r#"<g mask="url(#{id})">"#);
                close = "</g>";
            }
            StrokeAlign::Center => {}
        }
        self.body.push_str(&group);
        let _ = write!(
            self.body,
            r#"<path d="{d}" fill="none" stroke="{c}" stroke-opacity="{a}" stroke-width="{width}" stroke-linejoin="{join}" stroke-miterlimit="{limit:.3}"{cap}{dash}/>"#
        );
        self.body.push_str(close);
        // arrow heads at the ends of open paths
        if !closed {
            for poly in polys.iter().filter(|p| !p.closed && p.pts.len() >= 2) {
                for (k, cap) in s.stroke_cap.iter().enumerate() {
                    if !cap.is_head() {
                        continue;
                    }
                    let filled = *cap != StrokeCap::ArrowLines;
                    let n = poly.pts.len();
                    let (tip, from) = if k == 0 { (poly.pts[0], poly.pts[1]) } else { (poly.pts[n - 1], poly.pts[n - 2]) };
                    let dir = (tip - from).normalized();
                    if !dir.x.is_finite() {
                        continue;
                    }
                    let head = head_d(*cap, tip + o, dir, w);
                    if filled {
                        let _ = write!(self.body, r#"<path d="{head}" fill="{c}" fill-opacity="{a}" stroke="{c}" stroke-opacity="{a}" stroke-width="{w}" stroke-linejoin="round"/>"#);
                    } else {
                        let _ = write!(self.body, r#"<path d="{head}" fill="none" stroke="{c}" stroke-opacity="{a}" stroke-width="{w}" stroke-linejoin="miter" stroke-linecap="butt"/>"#);
                    }
                }
            }
        }
    }

    fn shape(&mut self, s: &Shape, offset: Vec2) {
        if !s.visible || s.slice {
            return;
        }
        if s.repeat.is_some() {
            for c in s.repeat_copies() {
                self.shape(&c, offset);
            }
            return;
        }
        if s.kind == Kind::Group && s.bool_op.is_some() {
            if let Some(p) = self.app.bool_proxy(s) {
                self.shape(&p, offset);
            }
            return;
        }
        let o = offset;
        let mut group = String::new();
        if s.opacity < 1.0 {
            let _ = write!(group, r#" opacity="{}""#, s.opacity);
        }
        if !s.blend.is_normal() {
            let _ = write!(group, r#" style="mix-blend-mode:{}""#, s.blend.css());
        }
        if s.kind == Kind::Text {
            // text keeps a plain drop shadow (the first one) as a filter
            if let Some(e) = s.effects.iter().find(|e| e.visible && e.kind == EffectKind::DropShadow) {
                let id = self.id("f");
                let (c, a) = col(e.color);
                let _ = write!(
                    self.defs,
                    r#"<filter id="{id}" x="-50%" y="-50%" width="200%" height="200%"><feDropShadow dx="{}" dy="{}" stdDeviation="{}" flood-color="{c}" flood-opacity="{a}"/></filter>"#,
                    e.dx, e.dy, e.blur / 2.0
                );
                let _ = write!(group, r#" filter="url(#{id})""#);
            }
        }
        if self.ids {
            // Figma: the layer name with spaces as underscores, a number after a repeat
            let base: String = s.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
            let base = if base.chars().next().is_some_and(|c| c.is_ascii_digit()) { format!("_{base}") } else { base };
            let n = self.names.entry(base.clone()).or_insert(0);
            let id = if *n == 0 { base } else { format!("{base}_{n}") };
            *n += 1;
            let _ = write!(group, r#" id="{id}""#);
        }
        let _ = write!(self.body, "<g{group}>");
        // a layer blur on a frame or group covers what is inside it too: one filter around the layer and its children
        let outer_blur = s.effects.iter().find(|e| e.visible && e.kind == EffectKind::LayerBlur && !e.progressive).filter(|_| s.kind.is_container() && !self.leaf).map(|e| e.blur);
        if let Some(b) = outer_blur {
            let f = self.id("f");
            let _ = write!(self.defs, r#"<filter id="{f}" x="-50%" y="-50%" width="200%" height="200%" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="{:.3}"/></filter>"#, b / 2.0);
            let _ = write!(self.body, r#"<g filter="url(#{f})">"#);
            self.skip_blur = true;
        }
        if s.kind == Kind::Text {
            self.text(s, o);
        } else if s.kind != Kind::Group {
            self.paint_shape(s, o);
        }
        self.skip_blur = false;
        if let Some(code) = s.code.as_ref().filter(|_| s.kind == Kind::Frame) {
            // a code layer: the code runs in an iframe inside a foreignObject (srcdoc keeps it well-formed whatever it holds)
            let r = s.rect().translate(o);
            let _ = write!(
                self.body,
                r#"<foreignObject x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"><iframe xmlns="http://www.w3.org/1999/xhtml" style="border:0;width:100%;height:100%" sandbox="allow-scripts" srcdoc="{}"></iframe></foreignObject>"#,
                r.min.x,
                r.min.y,
                r.width(),
                r.height(),
                escape(code).replace('"', "&quot;")
            );
        }
        if s.kind.is_container() && !self.leaf {
            let inner = o + s.pos.to_vec2();
            if s.kind == Kind::Frame && s.clip {
                let id = self.id("k");
                let r = s.rect().translate(o);
                let _ = write!(self.defs, r#"<clipPath id="{id}"><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"/></clipPath>"#, r.min.x, r.min.y, r.width(), r.height());
                let _ = write!(self.body, r#"<g clip-path="url(#{id})">"#);
                self.children(Some(s.id), inner);
                self.body.push_str("</g>");
            } else {
                self.children(Some(s.id), inner);
            }
        }
        if outer_blur.is_some() {
            self.body.push_str("</g>");
        }
        self.body.push_str("</g>");
    }

    fn children(&mut self, parent: Option<u64>, offset: Vec2) {
        let kids: Vec<Shape> = self.app.shapes.iter().filter(|s| s.parent == parent).cloned().collect();
        self.children_of(&kids, offset);
    }

    // a mask layer hides what its siblings above it show outside its own shape (the real outline, not its box)
    fn children_of(&mut self, kids: &[Shape], offset: Vec2) {
        let mut masks: Vec<String> = vec![];
        let mut hide_rest = false; // a mask with nothing drawn in it hides everything above it
        for k in kids {
            if let Some((frame, fixed)) = self.fixed_split {
                if k.parent == Some(frame) && k.fixed != fixed {
                    continue;
                }
            }
            if k.mask {
                let id = self.id("mk");
                if k.mask_kind == 0 {
                    // Vector: the outline of the layer, whatever its paint
                    let d = path_d(&k.polys(8.0), offset);
                    let _ = write!(
                        self.defs,
                        r##"<mask id="{id}" maskUnits="userSpaceOnUse" x="-100000" y="-100000" width="200000" height="200000"><path d="{d}" fill="#fff" fill-rule="evenodd"/></mask>"##
                    );
                } else {
                    // Alpha / Luminance: the layer as drawn, cut by its transparency or by its brightness
                    let saved = std::mem::take(&mut self.body);
                    let mut shown = k.clone();
                    shown.mask = false;
                    self.shape(&shown, offset);
                    let inner = std::mem::replace(&mut self.body, saved);
                    hide_rest |= !["<path", "<rect", "<ellipse", "<circle", "<text", "<image", "<use", "<polygon", "<line"].iter().any(|t| inner.contains(t));
                    let ty = if k.mask_kind == 1 { "alpha" } else { "luminance" };
                    let _ = write!(self.defs, r#"<mask id="{id}" maskUnits="userSpaceOnUse" x="-100000" y="-100000" width="200000" height="200000" mask-type="{ty}">{inner}</mask>"#);
                }
                masks.push(id);
                continue;
            }
            if hide_rest {
                continue;
            }
            for id in &masks {
                let _ = write!(self.body, r#"<g mask="url(#{id})">"#);
            }
            self.shape(k, offset);
            for _ in &masks {
                self.body.push_str("</g>");
            }
        }
    }

    fn text(&mut self, s: &Shape, o: Vec2) {
        let (color, a) = col(self.app.text_color(s));
        if let Some(pts) = self.app.text_path_world(s) {
            // text on a path: an SVG textPath along the same outline
            let id = self.id("tp");
            let d: String = pts.iter().enumerate().map(|(k, p)| format!("{}{:.2} {:.2}", if k == 0 { "M" } else { "L" }, p.x, p.y)).collect::<Vec<_>>().join(" ");
            let _ = write!(self.defs, r#"<path id="{id}" d="{d}"/>"#);
            let family = if s.mono { "monospace" } else { "Inter, sans-serif" };
            let _ = write!(
                self.body,
                r##"<text font-family="{family}" font-size="{}" fill="{color}" fill-opacity="{a}" xml:space="preserve"><textPath href="#{id}">{}</textPath></text>"##,
                s.font_size, escape(&s.text)
            );
            return;
        }
        let galley = text_galley(&self.painter, s, 1.0, Color32::WHITE, wrap_of(s));
        let (x, anchor) = match s.align {
            TextAlign::Left => (s.pos.x - s.hang_shift(&self.painter), "start"),
            TextAlign::Center => (s.pos.x + s.size.x / 2.0, "middle"),
            TextAlign::Right => (s.pos.x + s.size.x, "end"),
        };
        let c = s.center() + o;
        let rot = if s.rotation != 0.0 { format!(r#" transform="rotate({} {:.2} {:.2})""#, s.rotation, c.x, c.y) } else { String::new() };
        let family = if s.mono { "monospace" } else { "Inter, sans-serif" };
        let weight = s.base_weight().to_string();
        let variation = if s.opsz > 0.0 { format!(r#" style="font-variation-settings:'opsz' {:.0}""#, s.opsz) } else { String::new() };
        let spacing = s.font_size * s.tracking / 100.0;
        let (trim_top, trim_bottom) = s.trim_px();
        let extra = ((s.size.y - galley.size().y + trim_top + trim_bottom).max(0.0)) * [0.0, 0.5, 1.0][s.valign.min(2) as usize] - trim_top;
        if s.rich() {
            // mixed styles, case, lists, spacing...: every row is written in pieces of one style, each at the x the layout gave it
            let chars = s.display_chars();
            let mut ci = 0;
            for row in &galley.rows {
                let mut pieces: Vec<(crate::textrun::CharStyle, f32, f32, String)> = vec![];
                for g in &row.row.glyphs {
                    let st = chars.get(ci).or(chars.last()).map(|c| c.1.clone());
                    ci += 1;
                    let Some(st) = st.filter(|st| !st.indent && !st.spacer) else { continue };
                    match pieces.last_mut() {
                        Some(p) if p.0 == st => p.3.push(g.chr),
                        _ => pieces.push((st, g.pos.x, g.pos.y, g.chr.to_string())),
                    }
                }
                if row.ends_with_newline {
                    ci += 1;
                }
                for (st, gx, gy, text) in pieces {
                    if text.trim().is_empty() && !st.underline && !st.strike {
                        continue;
                    }
                    let mut size = st.size;
                    if st.small {
                        size *= 0.8;
                    }
                    if st.script != 0 {
                        size *= 0.6;
                    }
                    let fam = if !st.family.is_empty() { format!("{}, sans-serif", escape(&st.family)) } else { family.to_owned() };
                    let (c, ca) = st.color.map_or((color.clone(), a), col);
                    let ca = ca * st.fade as f32 / 255.0;
                    let deco = match (st.underline && !s.custom_underline(), st.strike) {
                        (true, true) => r#" text-decoration="underline line-through""#,
                        (true, false) => r#" text-decoration="underline""#,
                        (false, true) => r#" text-decoration="line-through""#,
                        _ => "",
                    };
                    let style = if st.italic { r#" font-style="italic""# } else { "" };
                    let (px, py) = (x + row.pos.x + gx + o.x, s.pos.y + extra + row.pos.y + gy + o.y);
                    let t = format!(
                        r#"<text x="{px:.2}" y="{py:.2}" font-family="{fam}" font-weight="{}"{style}{deco} letter-spacing="{}" font-size="{size}" fill="{c}" fill-opacity="{ca}" xml:space="preserve"{rot}>{}</text>"#,
                        st.weight,
                        st.size * s.tracking / 100.0,
                        escape(&text)
                    );
                    match &st.url {
                        Some(u) => {
                            let _ = write!(self.body, r#"<a href="{}">{t}</a>"#, escape(u));
                        }
                        None => self.body.push_str(&t),
                    }
                }
            }
            if s.custom_underline() {
                // underline style, thickness, offset, skip ink: drawn as lines and dots, as on the canvas
                let ul = crate::textrun::underline_marks(s, &galley, 1.0);
                let ink = ul.color.map_or_else(|| (color.clone(), a), col);
                let at = |p: Pos2| (x + o.x + p.x, s.pos.y + extra + o.y + p.y);
                for m in ul.marks {
                    match m {
                        crate::textrun::Ul::Line(pts) => {
                            let d: Vec<String> = pts.into_iter().map(|p| { let (px, py) = at(p); format!("{px:.2},{py:.2}") }).collect();
                            let _ = write!(self.body, r#"<polyline points="{}" fill="none" stroke="{}" stroke-opacity="{}" stroke-width="{:.2}"{rot}/>"#, d.join(" "), ink.0, ink.1, ul.width);
                        }
                        crate::textrun::Ul::Dot(p) => {
                            let (px, py) = at(p);
                            let _ = write!(self.body, r#"<circle cx="{px:.2}" cy="{py:.2}" r="{:.2}" fill="{}" fill-opacity="{}"{rot}/>"#, ul.width / 2.0, ink.0, ink.1);
                        }
                    }
                }
            }
            return;
        }
        for row in &galley.rows {
            let text = row.row.text();
            if text.is_empty() {
                continue;
            }
            // ponytail: baseline estimated at 80% of the font size below the row's top
            let y = s.pos.y + extra + row.pos.y + s.font_size * 0.8 + o.y;
            let _ = write!(
                self.body,
                r#"<text x="{:.2}" y="{:.2}" font-family="{family}" font-weight="{weight}"{variation} letter-spacing="{spacing}" font-size="{}" fill="{color}" fill-opacity="{a}" text-anchor="{anchor}" xml:space="preserve"{rot}>{}</text>"#,
                x + o.x, y, s.font_size, escape(&text)
            );
        }
    }
}

impl App {
    // the SVG for these root shapes (a subtree each), and the world rect it covers
    pub fn svg_for(&self, ctx: &egui::Context, roots: &[u64]) -> Option<(String, Rect)> {
        self.svg_for_ids(ctx, roots, false)
    }

    // `ids`: give every layer's group an id from its name
    pub fn svg_for_ids(&self, ctx: &egui::Context, roots: &[u64], ids: bool) -> Option<(String, Rect)> {
        self.svg_inner(ctx, roots, ids, false)
    }

    // a frame with the content that lies beyond its edges (what present mode scrolls): the frame does not clip and the
    // rect covers the frame plus its scroll range
    pub fn svg_scrollable(&self, ctx: &egui::Context, frame: u64, ids: bool) -> Option<(String, Rect)> {
        self.svg_inner(ctx, &[frame], ids, true)
    }

    // the children of `frame` that stay in place while it scrolls, as one picture the size of the frame (None when it has none)
    pub fn svg_fixed_layer(&self, ctx: &egui::Context, frame: u64, ids: bool) -> Option<String> {
        let fi = self.index_of(frame)?;
        let kids: Vec<Shape> = self.shapes.iter().filter(|s| s.parent == Some(frame) && s.fixed).cloned().collect();
        if kids.is_empty() {
            return None;
        }
        let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), Rect::EVERYTHING);
        let mut svg = Svg { app: self, painter, defs: String::new(), body: String::new(), n: 0, leaf: false, scope: None, base: None, depth: 0, skip_blur: false, ids, names: Default::default(), fixed_split: None };
        svg.children_of(&kids, self.origin_of(Some(frame)));
        let b = self.world_rect(fi);
        Some(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{} {} {w} {h}"><defs>{}</defs>{}</svg>"#,
            b.min.x, b.min.y, svg.defs, svg.body, w = b.width().max(1.0), h = b.height().max(1.0)
        ))
    }

    fn svg_inner(&self, ctx: &egui::Context, roots: &[u64], ids: bool, unclip: bool) -> Option<(String, Rect)> {
        // a slice exports whatever lies under its rectangle: every top-level layer, cropped to the slice
        let slice = roots.iter().filter_map(|&id| self.index_of(id)).find(|&i| self.shapes[i].slice).map(|i| self.world_rect(i));
        let all: Vec<u64>;
        let roots = if slice.is_some() {
            all = self.shapes.iter().filter(|s| s.parent.is_none() && !s.slice).map(|s| s.id).collect();
            &all[..]
        } else {
            roots
        };
        let mut bounds: Option<Rect> = None;
        for &id in roots {
            let i = self.index_of(id)?;
            let pad = if self.shapes[i].kind == Kind::Frame && self.shapes[i].effects.iter().all(|e| !e.visible) { 0.0 } else { self.shapes[i].stroke_w / 2.0 + self.shapes[i].paint_pad() };
            let mut r = self.world_rect(i).expand(pad);
            if unclip {
                r.max += self.present_scroll_range(i);
            }
            bounds = Some(bounds.map_or(r, |b| b.union(r)));
        }
        let b = slice.or(bounds)?;
        let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), Rect::EVERYTHING);
        let scope: Vec<usize> = self.draw_order().into_iter().filter(|&k| roots.iter().any(|&r| self.is_inside(self.shapes[k].id, r))).collect();
        let mut svg = Svg { app: self, painter, defs: String::new(), body: String::new(), n: 0, leaf: false, scope: Some(scope), base: None, depth: 0, skip_blur: false, ids, names: Default::default(), fixed_split: if unclip { roots.first().map(|&r| (r, false)) } else { None } };
        for &id in roots {
            let i = self.index_of(id)?;
            // shapes are drawn in the space of their parent chain: offset by the parent's origin
            let mut s = self.shapes[i].clone();
            if unclip {
                s.clip = false;
            }
            svg.shape(&s, self.origin_of(s.parent));
        }
        let doc = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{x} {y} {w} {h}"><defs>{}</defs>{}</svg>"#,
            svg.defs, svg.body, x = b.min.x, y = b.min.y, w = b.width().max(1.0), h = b.height().max(1.0)
        );
        Some((doc, b))
    }

    // the raster of one layer on its own (see `draw_sprite`): `local` has its position at the origin, `region` is the world
    // rect it covers, `pw` x `ph` the pixels
    pub fn sprite_image(&self, ctx: &egui::Context, local: &Shape, region: Rect, pw: u32, ph: u32) -> Option<egui::ColorImage> {
        let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), Rect::EVERYTHING);
        let mut svg = Svg { app: self, painter, defs: String::new(), body: String::new(), n: 0, leaf: true, scope: None, base: None, depth: 0, skip_blur: false, ids: false, names: Default::default(), fixed_split: None };
        svg.shape(local, Vec2::ZERO);
        let doc = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{pw}" height="{ph}" viewBox="{} {} {} {}"><defs>{}</defs>{}</svg>"#,
            region.min.x, region.min.y, region.width(), region.height(), svg.defs, svg.body
        );
        rasterize_scene(&doc, pw, ph)
    }

    // a run of siblings that starts at a mask layer (the mask, then what it masks), drawn as one picture: what the canvas shows
    // for a mask that is not a plain box. `offset` is the parent's world origin, `region` the world rect, `pw` x `ph` pixels.
    pub fn masked_run_image(&self, ctx: &egui::Context, kids: &[Shape], offset: Vec2, region: Rect, pw: u32, ph: u32) -> Option<egui::ColorImage> {
        let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), Rect::EVERYTHING);
        let mut svg = Svg { app: self, painter, defs: String::new(), body: String::new(), n: 0, leaf: false, scope: None, base: None, depth: 0, skip_blur: false, ids: false, names: Default::default(), fixed_split: None };
        svg.children_of(kids, offset);
        let doc = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{pw}" height="{ph}" viewBox="{} {} {} {}"><defs>{}</defs>{}</svg>"#,
            region.min.x, region.min.y, region.width(), region.height(), svg.defs, svg.body
        );
        rasterize_scene(&doc, pw, ph)
    }

    // Layers the code exports cannot express as markup (vector paths, polygons, stars, arcs, image fills) are exported as a picture
    // of the layer on its own: its SVG and the rect it covers relative to the layer's position (shadows and strokes reach past the
    // box). A container's picture is its own paint only; its children are exported on top as usual.
    pub fn picture_of(&self, i: usize) -> Option<(String, Rect)> {
        let s = &self.shapes[i];
        let image = s.fills.iter().chain(s.strokes.iter()).any(|p| p.visible && matches!(p.kind, PaintKind::Image(_)));
        let vector = matches!(s.kind, Kind::Path | Kind::Polygon | Kind::Star | Kind::Line) || (s.kind == Kind::Ellipse && (s.arc[1].abs() < 359.9 || s.inner > 0.0));
        if !(image || vector) || s.kind == Kind::Text {
            return None;
        }
        let mut local = s.clone();
        local.pos = Pos2::ZERO;
        local.id = 0;
        local.parent = None;
        local.opacity = 1.0;
        local.blend = BlendMode::Normal;
        local.name.clear();
        local.link = None;
        local.tracks.clear();
        let region = local.aabb().expand(local.paint_pad() + 2.0);
        let ctx = egui::Context::default();
        let painter = egui::Painter::new(ctx, egui::LayerId::background(), Rect::EVERYTHING);
        let mut svg = Svg { app: self, painter, defs: String::new(), body: String::new(), n: 0, leaf: true, scope: None, base: None, depth: 0, skip_blur: false, ids: false, names: Default::default(), fixed_split: None };
        svg.shape(&local, Vec2::ZERO);
        let doc = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{} {} {w} {h}"><defs>{}</defs>{}</svg>"#,
            region.min.x, region.min.y, svg.defs, svg.body, w = region.width().max(1.0), h = region.height().max(1.0)
        );
        Some((doc, region))
    }

    // what is behind a layer with a background blur: the page colour and every layer painted before it (`earlier`, in draw
    // order), blurred, and cut to the layer's outline. `region` is the world rect drawn, `sigma` the blur in world units.
    pub fn backdrop_image(&self, ctx: &egui::Context, earlier: &[usize], target: &Shape, offset: Vec2, region: Rect, sigma: f32, pw: u32, ph: u32, glass: Option<&Effect>) -> Option<egui::ColorImage> {
        let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), Rect::EVERYTHING);
        let mut svg = Svg { app: self, painter, defs: String::new(), body: String::new(), n: 0, leaf: true, scope: None, base: None, depth: 0, skip_blur: false, ids: false, names: Default::default(), fixed_split: None };
        let d = path_d(&target.polys(8.0), offset);
        let (clip, blur) = (svg.id("c"), svg.id("f"));
        let _ = write!(svg.defs, r#"<clipPath id="{clip}"><path d="{d}" clip-rule="evenodd"/></clipPath>"#);
        let _ = write!(
            svg.defs,
            r#"<filter id="{blur}" filterUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="{sigma:.3}" edgeMode="duplicate"/></filter>"#,
            region.min.x, region.min.y, region.width(), region.height()
        );
        let (bg, a) = col(self.page_bg());
        let backdrop_rect = format!(r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{bg}" fill-opacity="{a}"/>"#, region.min.x, region.min.y, region.width(), region.height());
        for &k in earlier {
            let s = self.shapes[k].clone();
            if s.mask || self.chain(k).iter().any(|&c| !self.shapes[c].visible) {
                continue;
            }
            svg.shape(&s, self.origin_of(s.parent));
        }
        let content = std::mem::take(&mut svg.body);
        // refracting glass: the backdrop is drawn, bent near the layer's edge, and put back as a picture under the same blur
        let bent = glass.map(|g| glass_bend(g, target.aabb())).filter(|&(_, max, ..)| max >= 0.25).and_then(|(depth, max, disperse, splay)| {
            let scene = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{pw}" height="{ph}" viewBox="{} {} {} {}"><defs>{}</defs>{backdrop_rect}{content}</svg>"#,
                region.min.x, region.min.y, region.width(), region.height(), svg.defs
            );
            bend_backdrop(&scene, &d, region, (pw, ph), pw as f32 / region.width().max(1.0), depth, max, disperse, splay)
        });
        let inner = match bent {
            Some(uri) => format!(r#"<image x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" preserveAspectRatio="none" href="{uri}"/>"#, region.min.x, region.min.y, region.width(), region.height()),
            None => format!("{backdrop_rect}{content}"),
        };
        let _ = write!(svg.body, r#"<g clip-path="url(#{clip})"><g filter="url(#{blur})">{inner}</g></g>"#);
        let doc = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{pw}" height="{ph}" viewBox="{} {} {} {}"><defs>{}</defs>{}</svg>"#,
            region.min.x, region.min.y, region.width(), region.height(), svg.defs, svg.body
        );
        rasterize_scene(&doc, pw, ph)
    }

    // the whole page as an SVG sized `px` pixels, showing world rect `view` over `background`:
    // what the canvas shows when blend modes need real compositing
    pub fn svg_scene(&self, ctx: &egui::Context, view: Rect, px: Vec2, background: Color32) -> String {
        let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), Rect::EVERYTHING);
        let mut svg = Svg { app: self, painter, defs: String::new(), body: String::new(), n: 0, leaf: false, scope: Some(self.draw_order()), base: Some(background), depth: 0, skip_blur: false, ids: false, names: Default::default(), fixed_split: None };
        svg.children(None, Vec2::ZERO);
        let (c, a) = col(background);
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}"><defs>{}</defs><rect x="{}" y="{}" width="{}" height="{}" fill="{c}" fill-opacity="{a}"/>{}</svg>"#,
            px.x, px.y, view.min.x, view.min.y, view.width(), view.height(), svg.defs, view.min.x, view.min.y, view.width(), view.height(), svg.body
        )
    }

    // one export setting of layer `id` as file content: its scale (a multiplier or a size), colour profile, resampling and format options
    pub fn export_one(&mut self, ctx: &egui::Context, id: u64, set: &ExportSetting, svg: &str) -> Result<Vec<u8>, String> {
        let size = self.index_of(id).map_or(Vec2::splat(1.0), |i| self.world_rect(i).size());
        let scale = set.effective_scale(size);
        let p3 = match set.profile {
            1 => false,
            2 => true,
            _ => crate::profile::is_p3(),
        };
        let svg = if set.basic_resample { svg.replace("<image ", "<image image-rendering=\"optimizeSpeed\" ") } else { svg.to_owned() };
        let out = match set.format {
            Format::Svg => {
                let text = if set.include_id { self.svg_for_ids(ctx, &[id], true).map(|(s, _)| s).unwrap_or_else(|| svg.clone()) } else { svg.clone() };
                Ok((if set.outline_text { outline_text(&text) } else { text }).into_bytes())
            }
            Format::Png => render_png_in(&svg, scale, p3),
            Format::Jpg => render_png_in(&svg, scale, false).and_then(|png| encode_jpg(&png, set.quality)),
            Format::Pdf => render_pdf(&svg),
            Format::Lottie => self.lottie_json(ctx, &[id]).map(String::into_bytes).ok_or_else(|| "nothing in the layer can be a Lottie layer".to_string()),
            Format::DotLottie => self.dotlottie(ctx, &[id]).ok_or_else(|| "nothing in the layer can be a Lottie layer".to_string()),
            Format::Webp | Format::Gif | Format::Mp4 | Format::Webm => self.export_extra(ctx, &[id], set.format, scale).unwrap_or_else(|| Err("unknown format".into())),
        };
        out
    }

    // writes every export setting of these layers into `dir`; the paths written
    pub fn export_assets(&mut self, ctx: &egui::Context, ids: &[u64], dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
        let mut written = vec![];
        for &id in ids {
            let Some(i) = self.index_of(id) else { continue };
            let Some((svg, _)) = self.svg_for(ctx, &[id]) else { continue };
            for set in self.shapes[i].exports.clone() {
                let path = dir.join(set.file_name(&self.shapes[i].name));
                let bytes = self.export_one(ctx, id, &set, &svg)?;
                std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
                written.push(path);
            }
        }
        Ok(written)
    }

    // the selection's saved export settings, into a folder the user picks
    pub fn export_saved(&mut self, ctx: &egui::Context) {
        let ids: Vec<u64> = self.sel.iter().copied().filter(|&id| self.index_of(id).is_some_and(|i| !self.shapes[i].exports.is_empty())).collect();
        let Some(dir) = rfd::FileDialog::new().set_title("Export to folder").pick_folder() else { return };
        self.status = match self.export_assets(ctx, &ids, &dir) {
            Ok(w) => format!("exported {} file{} to {}", w.len(), if w.len() == 1 { "" } else { "s" }, dir.display()),
            Err(e) => format!("export failed: {e}"),
        };
    }

    pub fn export(&mut self, ctx: &egui::Context) {
        let mut roots = self.sel_roots();
        if roots.is_empty() {
            roots = self.shapes.iter().filter(|s| s.parent.is_none()).map(|s| s.id).collect();
        }
        let Some((svg, _)) = self.svg_for(ctx, &roots) else {
            self.status = "nothing to export".into();
            return;
        };
        let fmt = self.export_fmt;
        let name = roots.first().and_then(|&r| self.index_of(r)).map(|i| self.shapes[i].name.clone()).unwrap_or_else(|| "export".into());
        let Some(path) = rfd::FileDialog::new().add_filter(fmt.ext(), &[fmt.ext()]).set_file_name(format!("{name}.{}", fmt.file_ext())).save_file() else { return };
        let result: Result<(), String> = match fmt {
            Format::DotLottie => match self.dotlottie(ctx, &roots) {
                Some(bytes) => std::fs::write(&path, bytes).map_err(|e| e.to_string()),
                None => Err("nothing in the selection can be a Lottie layer".into()),
            },
            Format::Lottie => match self.lottie_json(ctx, &roots) {
                Some(json) => std::fs::write(&path, json).map_err(|e| e.to_string()),
                None => Err("nothing in the selection can be a Lottie layer".into()),
            },
            Format::Svg => std::fs::write(&path, &svg).map_err(|e| e.to_string()),
            Format::Png => render_png(&svg, self.export_scale).and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string())),
            Format::Jpg => render_png(&svg, self.export_scale).and_then(|png| encode_jpg(&png, 2)).and_then(|jpg| std::fs::write(&path, jpg).map_err(|e| e.to_string())),
            // several frames selected: one page each
            Format::Pdf if roots.len() > 1 => {
                let svgs: Vec<String> = roots.iter().filter_map(|&r| self.svg_for(ctx, &[r])).map(|s| s.0).collect();
                crate::anim_export::pdf_pages(&svgs).and_then(|pdf| std::fs::write(&path, pdf).map_err(|e| e.to_string()))
            }
            Format::Pdf => render_pdf(&svg).and_then(|pdf| std::fs::write(&path, pdf).map_err(|e| e.to_string())),
            Format::Webp | Format::Gif | Format::Mp4 | Format::Webm => match self.export_extra(ctx, &roots, fmt, self.export_scale) {
                Some(r) => r.and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string())),
                None => Err("unknown format".into()),
            },
        };
        self.status = match result {
            Ok(()) => format!("exported {}", path.display()),
            Err(e) => format!("export failed: {e}"),
        };
    }
}

// system fonts take a while to scan, so every render shares one database
pub(crate) fn fonts() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    font_store().lock().unwrap().0.clone()
}

fn scan_fonts() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    // the font the canvas draws with, so an export lays out like the canvas
    db.load_font_data(include_bytes!("../assets/fonts/Inter.ttf").to_vec());
    db.load_font_data(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").to_vec());
    std::sync::Arc::new(db)
}

// the database, and when the font folders last changed (what it was built from)
fn font_store() -> &'static std::sync::Mutex<(std::sync::Arc<resvg::usvg::fontdb::Database>, Option<std::time::SystemTime>)> {
    static STORE: std::sync::OnceLock<std::sync::Mutex<(std::sync::Arc<resvg::usvg::fontdb::Database>, Option<std::time::SystemTime>)>> = std::sync::OnceLock::new();
    STORE.get_or_init(|| std::sync::Mutex::new((scan_fonts(), font_folders_stamp())))
}

// the newest change time of the folders fonts are installed in: a font installed while drui runs changes it
fn font_folders_stamp() -> Option<std::time::SystemTime> {
    let mut dirs: Vec<std::path::PathBuf> = vec![];
    if cfg!(windows) {
        if let Some(w) = std::env::var_os("WINDIR") {
            dirs.push(std::path::PathBuf::from(w).join("Fonts"));
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(std::path::PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
        }
    } else {
        dirs.extend(["/usr/share/fonts", "/usr/local/share/fonts", "/Library/Fonts", "/System/Library/Fonts"].map(std::path::PathBuf::from));
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(std::path::PathBuf::from(&h).join(".fonts"));
            dirs.push(std::path::PathBuf::from(&h).join(".local/share/fonts"));
        }
    }
    dirs.iter().filter_map(|d| std::fs::metadata(d).and_then(|m| m.modified()).ok()).max()
}

// looks at the font folders again; when a font was added or removed since the last scan, scans them. True when it did.
pub(crate) fn rescan_fonts_if_changed() -> bool {
    let stamp = font_folders_stamp();
    {
        let g = font_store().lock().unwrap();
        if g.1 == stamp {
            return false;
        }
    }
    let db = scan_fonts();
    *font_store().lock().unwrap() = (db, stamp);
    true
}

// rasterizes a scene SVG to an egui image of its own pixel size
// the numbers of a glass layer's refraction: how deep the bent band is (px), how far it pulls at the edge (px) and the colour split
fn glass_bend(g: &Effect, bounds: Rect) -> (f32, f32, f32, f32) {
    let depth_px = (g.radius / 100.0 * 0.5 * bounds.width().min(bounds.height())).clamp(1.0, 120.0);
    (depth_px, g.spread / 100.0 * depth_px, g.size / 100.0 * 0.5, g.splay / 100.0)
}

// how far past the layer's box a refracting backdrop reaches
pub fn glass_reach(g: &Effect, bounds: Rect) -> f32 {
    glass_bend(g, bounds).1
}

// glass refraction, done on pixels: `scene` is the SVG of what is behind the glass, drawn at `k` pixels per unit into `size`.
// Every pixel within `depth_px` of the glass outline is taken from further out (a lens: the edge shows what lies beyond it), by up
// to `max_px` at the edge and falling off with the square of the distance (more gently with Splay); `disperse` splits that by colour (red is taken from
// further out than blue). Returns the result as a PNG data URI.
fn bend_backdrop(scene: &str, d: &str, region: Rect, size: (u32, u32), k: f32, depth_px: f32, max_px: f32, disperse: f32, splay: f32) -> Option<String> {
    use base64::Engine;
    let (w, h) = (size.0 as usize, size.1 as usize);
    let back = rasterize_scene(scene, size.0, size.1)?;
    let mask_doc = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{} {} {} {}"><path d="{d}" fill="#fff" fill-rule="evenodd"/></svg>"##,
        region.min.x, region.min.y, region.width(), region.height()
    );
    let mask = rasterize_scene(&mask_doc, size.0, size.1)?;
    // distance to the nearest pixel outside the shape: a 3-4 chamfer, in 3rds of a pixel
    let mut dist: Vec<i32> = mask.pixels.iter().map(|p| if p.a() > 127 { i32::MAX / 2 } else { 0 }).collect();
    let at = |x: isize, y: isize| -> Option<usize> { (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h).then(|| y as usize * w + x as usize) };
    for y in 0..h as isize {
        for x in 0..w as isize {
            let i = at(x, y)?;
            for (dx, dy, c) in [(-1, 0, 3), (-1, -1, 4), (0, -1, 3), (1, -1, 4)] {
                if let Some(j) = at(x + dx, y + dy) {
                    dist[i] = dist[i].min(dist[j] + c);
                }
            }
        }
    }
    for y in (0..h as isize).rev() {
        for x in (0..w as isize).rev() {
            let i = at(x, y)?;
            for (dx, dy, c) in [(1, 0, 3), (1, 1, 4), (0, 1, 3), (-1, 1, 4)] {
                if let Some(j) = at(x + dx, y + dy) {
                    dist[i] = dist[i].min(dist[j] + c);
                }
            }
        }
    }
    let get = |x: isize, y: isize| at(x.clamp(0, w as isize - 1), y.clamp(0, h as isize - 1)).map_or(0.0, |i| dist[i] as f32 / 3.0);
    let (depth, max) = ((depth_px * k).max(1.0), max_px * k);
    // a bilinear sample of one channel of the backdrop
    let sample = |fx: f32, fy: f32, c: usize| -> f32 {
        let (fx, fy) = (fx.clamp(0.0, w as f32 - 1.001), fy.clamp(0.0, h as f32 - 1.001));
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let p = |x: usize, y: usize| back.pixels[y * w + x].to_srgba_unmultiplied()[c] as f32;
        (p(x0, y0) * (1.0 - tx) + p(x0 + 1, y0) * tx) * (1.0 - ty) + (p(x0, y0 + 1) * (1.0 - tx) + p(x0 + 1, y0 + 1) * tx) * ty
    };
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let i = y as usize * w + x as usize;
            let here = dist[i] as f32 / 3.0;
            let (mut ox, mut oy, mut mag) = (0.0, 0.0, 0.0);
            if here > 0.0 && here < depth {
                // the distance grows inwards, so its gradient is the inward normal; the pixel is taken from outwards of itself
                let (gx, gy) = (get(x + 1, y) - get(x - 1, y), get(x, y + 1) - get(x, y - 1));
                let len = (gx * gx + gy * gy).sqrt();
                if len > 1e-3 {
                    let t = 1.0 - here / depth;
                    // Splay spreads the bend inwards: the fall-off from the edge is a power of 2 at no splay, nearly linear at full splay
                    (ox, oy, mag) = (-gx / len, -gy / len, t.powf(2.0 - 1.5 * splay.clamp(0.0, 1.0)) * max);
                }
            }
            // red looks furthest out, blue the least (a lens bends blue light more at the same angle, so it ends up nearer)
            let scales = [1.0 + disperse, 1.0, 1.0 - disperse];
            for (c, sc) in scales.iter().enumerate() {
                out[i * 4 + c] = sample(x as f32 + ox * mag * sc, y as f32 + oy * mag * sc, c).round().clamp(0.0, 255.0) as u8;
            }
            out[i * 4 + 3] = sample(x as f32 + ox * mag, y as f32 + oy * mag, 3).round().clamp(0.0, 255.0) as u8;
        }
    }
    let img = image::RgbaImage::from_raw(w as u32, h as u32, out)?;
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png.into_inner())))
}

// the blend modes the rasteriser has no name for: browsers know `plus-lighter` and `plus-darker`; here they are drawn as the
// closest it has (Screen and Multiply)
fn raster_blend(svg: &str) -> String {
    if svg.contains("plus-") { svg.replace("mix-blend-mode:plus-lighter", "mix-blend-mode:screen").replace("mix-blend-mode:plus-darker", "mix-blend-mode:multiply") } else { svg.to_owned() }
}

pub fn rasterize_scene(svg: &str, w: u32, h: u32) -> Option<egui::ColorImage> {
    use resvg::{tiny_skia, usvg};
    let svg = &raster_blend(svg);
    let opt = usvg::Options { fontdb: fonts(), ..usvg::Options::default() };
    let tree = usvg::Tree::from_str(svg, &opt).ok()?;
    let mut pm = tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    let k = w as f32 / tree.size().width().max(1.0);
    resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pm.as_mut());
    Some(egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], pm.data()))
}

// JPG has no transparency: the picture goes on white
pub fn encode_jpg(png: &[u8], quality: u8) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(png).map_err(|e| e.to_string())?.to_rgba8();
    let mut flat = image::RgbaImage::from_pixel(img.width(), img.height(), image::Rgba([255, 255, 255, 255]));
    image::imageops::overlay(&mut flat, &img, 0, 0);
    let rgb = image::DynamicImage::ImageRgba8(flat).to_rgb8();
    let q = [60, 80, 92][quality.min(2) as usize];
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q).encode(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())?;
    Ok(out)
}

// Outline text: the SVG written again with its text as shapes (the rasteriser's own layout of the glyphs)
pub fn outline_text(svg: &str) -> String {
    use resvg::usvg;
    let opt = usvg::Options { fontdb: fonts(), ..usvg::Options::default() };
    match usvg::Tree::from_str(svg, &opt) {
        Ok(tree) => tree.to_string(&usvg::WriteOptions::default()),
        Err(_) => svg.to_owned(),
    }
}

impl ExportSetting {
    // the scale to render a layer of `size` at: 2x is 2; 512w is whatever makes it 512 px wide
    pub fn effective_scale(&self, size: Vec2) -> f32 {
        match self.dim {
            1 => self.scale / size.x.max(1.0),
            2 => self.scale / size.y.max(1.0),
            _ => self.scale,
        }
    }
}

pub fn render_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    render_png_in(svg, scale, crate::profile::is_p3())
}

// `p3`: the PNG carries the Display P3 profile
pub fn render_png_in(svg: &str, scale: f32, p3: bool) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};
    let svg = &raster_blend(svg);
    let opt = usvg::Options { fontdb: fonts(), ..usvg::Options::default() };
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let size = tree.size();
    let (w, h) = ((size.width() * scale).ceil().max(1.0) as u32, (size.height() * scale).ceil().max(1.0) as u32);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("export is too large")?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.encode_png().map(|png| crate::profile::tag_png_in(png, p3)).map_err(|e| e.to_string())
}

pub fn render_pdf(svg: &str) -> Result<Vec<u8>, String> {
    use svg2pdf::usvg;
    let svg = &raster_blend(svg);
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    opt.fontdb_mut().load_font_data(include_bytes!("../assets/fonts/Inter.ttf").to_vec());
    opt.fontdb_mut().load_font_data(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").to_vec());
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), svg2pdf::PageOptions::default()).map_err(|e| e.to_string())
}
