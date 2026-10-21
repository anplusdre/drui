// Painting shapes onto the egui canvas.
use std::sync::Arc;

use egui::epaint::{Mesh, TextShape, Vertex, WHITE_UV};
use egui::{Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Stroke, Vec2};

use crate::geom::{Poly, is_convex, triangulate};
use crate::model::{EffectKind, GridKind, Kind, Paint, PaintKind, Shape, TextAlign, TextFit};
use crate::{App, BRAND, TEXT_SECONDARY};

const COMPONENT: Color32 = Color32::from_rgb(0x97, 0x47, 0xff);

// the galley for a text shape at the given zoom (1.0 = world units)
// red wavy lines under the misspelled words of a laid-out text starting at screen point `at`
fn squiggles(painter: &Painter, at: Pos2, galley: &egui::Galley, text: &str) {
    use egui::epaint::text::cursor::CCursor;
    for (start, len) in crate::spell::misspelled(text) {
        let a = galley.pos_from_cursor(CCursor::new(start));
        let b = galley.pos_from_cursor(CCursor::new(start + len));
        if (a.max.y - b.max.y).abs() > 1.0 || b.min.x <= a.min.x {
            continue; // a word that wraps over two lines
        }
        let y = at.y + a.max.y;
        let pts: Vec<Pos2> = (0..)
            .map(|k| at.x + a.min.x + k as f32 * 2.0)
            .take_while(|&x| x < at.x + b.min.x)
            .enumerate()
            .map(|(k, x)| Pos2::new(x, y + if k % 2 == 0 { 0.0 } else { 1.5 }))
            .collect();
        if pts.len() > 1 {
            painter.add(egui::Shape::line(pts, Stroke::new(1.0, Color32::from_rgb(0xf2, 0x48, 0x22))));
        }
    }
}

pub fn text_galley(painter: &Painter, s: &Shape, zoom: f32, color: Color32, wrap_width: f32) -> Arc<egui::Galley> {
    let width = wrapped_width(painter, s, zoom, color, wrap_width);
    painter.layout_job(crate::textrun::text_job(painter, s, zoom, color, width, false))
}

// Wrap style: the width the lines are broken at. Balance: the narrowest width that keeps the same number of lines (so they come
// out about equally long), for up to 6 lines. Pretty: a slightly narrower width when the last line would be a single word.
fn wrapped_width(painter: &Painter, s: &Shape, zoom: f32, color: Color32, w: f32) -> f32 {
    if s.wrap_style == 0 || !w.is_finite() || s.fit == TextFit::AutoWidth {
        return w;
    }
    let lay = |w: f32| painter.layout_job(crate::textrun::text_job(painter, s, zoom, color, w, false));
    let first = lay(w);
    let lines = first.rows.len();
    if lines < 2 {
        return w;
    }
    if s.wrap_style == 1 {
        if lines > 6 {
            return w;
        }
        let (mut lo, mut hi) = (w * 0.25, w);
        for _ in 0..12 {
            let mid = (lo + hi) / 2.0;
            if lay(mid).rows.len() <= lines { hi = mid } else { lo = mid }
        }
        return (hi + 0.5).min(w);
    }
    let words = |g: &egui::Galley| g.rows.last().map_or(2, |r| r.row.text().split_whitespace().count());
    if words(&first) >= 2 {
        return w;
    }
    for k in 1..=20 {
        let try_w = w * (1.0 - 0.015 * k as f32);
        let g = lay(try_w);
        if g.rows.len() == lines && words(&g) >= 2 {
            return try_w;
        }
        if g.rows.len() > lines {
            break;
        }
    }
    w
}

pub fn wrap_of(s: &Shape) -> f32 {
    if s.fit == TextFit::AutoWidth { f32::INFINITY } else { s.size.x.max(1.0) }
}

impl App {
    // the page colour as it shows: its own opacity applied, nothing at all when the eye is off
    pub fn page_bg(&self) -> Color32 {
        let p = &self.pages[self.cur];
        if p.bg_visible { p.background } else { Color32::TRANSPARENT }
    }

    // the color a solid paint currently shows (follows its style when it has one)
    pub fn paint_color(&self, p: &Paint) -> Color32 {
        if let Some(st) = p.style.and_then(|id| self.styles.iter().find(|s| s.id == id)) {
            return st.color;
        }
        match p.kind {
            PaintKind::Solid(c) => c,
            PaintKind::Linear { a, .. } => a,
            PaintKind::Gradient(ref g) => g.stops.first().map_or(Color32::GRAY, |s| s.color),
            PaintKind::Image(_) | PaintKind::Pattern(_) => Color32::GRAY,
            PaintKind::Shader(ref sf) => sf.a,
        }
    }

    // first visible fill: used for text color
    pub fn text_color(&self, s: &Shape) -> Color32 {
        s.fills.iter().rev().find(|f| f.visible).map(|f| self.paint_color(f)).unwrap_or(Color32::BLACK)
    }

    fn screen_polys(&self, origin: Pos2, s: &Shape, offset: Vec2) -> Vec<Poly> {
        s.polys(self.zoom)
            .into_iter()
            .map(|mut p| {
                p.pts.iter_mut().for_each(|q| *q = self.to_screen(origin, *q + offset));
                p
            })
            .collect()
    }

    fn fill_polys(&self, painter: &Painter, polys: &[Poly], paint: &Paint, bounds: Rect, alpha: f32) {
        let k = alpha * paint.opacity;
        let solid = matches!(paint.kind, PaintKind::Solid(_));
        if solid {
            let col = self.paint_color(paint).gamma_multiply(k);
            let closed: Vec<&Poly> = polys.iter().filter(|p| p.closed).collect();
            if let [only] = closed[..] {
                if is_convex(&only.pts) {
                    painter.add(egui::Shape::convex_polygon(only.pts.clone(), col, Stroke::NONE));
                    return;
                }
            }
        }
        let (verts, idx) = triangulate(polys);
        if idx.is_empty() {
            return;
        }
        let tex = match paint.kind {
            PaintKind::Image(id) => self.tex.get(&id).map(|t| t.id()),
            _ => None,
        };
        let mut mesh = tex.map_or_else(Mesh::default, Mesh::with_texture);
        let size = bounds.size().max(Vec2::splat(1e-3));
        for p in &verts {
            let (color, uv) = match &paint.kind {
                PaintKind::Solid(_) => (self.paint_color(paint).gamma_multiply(k), WHITE_UV),
                PaintKind::Linear { a, b, angle } => {
                    let (s, c) = angle.to_radians().sin_cos();
                    let half = (size.x * c.abs() + size.y * s.abs()) / 2.0;
                    let t = (0.5 + (*p - bounds.center()).dot(Vec2::new(c, s)) / (2.0 * half.max(1e-3))).clamp(0.0, 1.0);
                    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
                    (Color32::from_rgba_premultiplied(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()), lerp(a.a(), b.a())).gamma_multiply(k), WHITE_UV)
                }
                PaintKind::Gradient(g) => {
                    let n = Pos2::new((p.x - bounds.min.x) / size.x, (p.y - bounds.min.y) / size.y);
                    (g.at(g.param(n, size.x / size.y)).gamma_multiply(k), WHITE_UV)
                }
                PaintKind::Image(id) => {
                    let mut uv = ((*p - bounds.min) / size).to_pos2();
                    // a playing video fills its layer the way Fill does: scaled to cover, the overflow cropped evenly
                    if let Some(t) = paint.video.and_then(|_| self.tex.get(id)) {
                        let (box_aspect, tex_aspect) = (size.x / size.y, t.aspect_ratio());
                        if tex_aspect > box_aspect {
                            uv.x = 0.5 + (uv.x - 0.5) * box_aspect / tex_aspect;
                        } else {
                            uv.y = 0.5 + (uv.y - 0.5) * tex_aspect / box_aspect;
                        }
                    }
                    (Color32::WHITE.gamma_multiply(k), uv)
                }
                // sprites draw these; the mesh path only sees them through a fallback colour
                PaintKind::Pattern(_) | PaintKind::Shader(_) => (self.paint_color(paint).gamma_multiply(k), WHITE_UV),
            };
            mesh.vertices.push(Vertex { pos: *p, uv, color });
        }
        mesh.indices = idx;
        painter.add(egui::Shape::mesh(mesh));
    }

    fn stroke_polys(&self, painter: &Painter, polys: &[Poly], s: &Shape, alpha: f32) {
        let w = s.stroke_w * self.zoom;
        if w <= 0.0 {
            return;
        }
        for paint in s.strokes.iter().filter(|p| p.visible) {
            let st = Stroke::new(w, self.paint_color(paint).gamma_multiply(alpha * paint.opacity));
            for p in polys {
                let mut pts = p.pts.clone();
                if p.closed {
                    pts.push(pts[0]);
                }
                if s.dash {
                    painter.extend(egui::Shape::dashed_line(&pts, st, (w * 4.0).max(4.0), (w * 3.0).max(3.0)));
                } else {
                    painter.add(egui::Shape::line(pts, st));
                }
            }
        }
    }

    // a background blur: what lies behind the layer, blurred and cut to its outline, drawn under the layer's own paint. The raster is
    // cached by what is behind (every layer before this one), the layer, the view scale and the page colour.
    fn draw_backdrop(&self, painter: &Painter, origin: Pos2, s: &Shape, offset: Vec2, sigma: f32, alpha: f32, glass: Option<&crate::model::Effect>) {
        let Some(idx) = self.index_of(s.id) else { return };
        let order = self.draw_order();
        let Some(pos) = order.iter().position(|&k| k == idx) else { return };
        let earlier = &order[..pos];
        let ctx = painter.ctx();
        let pass = ctx.cumulative_pass_nr();
        let region = s.aabb().translate(offset).expand(sigma * 3.0 + 2.0 + glass.map_or(0.0, |g| crate::export::glass_reach(g, s.aabb())));
        let want = (self.zoom * ctx.pixels_per_point()).max(0.02);
        let mut scale = 2f32.powf((want.log2() * 2.0).round() / 2.0);
        scale = scale.min(2048.0 / region.width().max(region.height()).max(1.0));
        let (pw, ph) = ((region.width() * scale).ceil().max(1.0) as u32, (region.height() * scale).ceil().max(1.0) as u32);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            "backdrop".hash(&mut h);
            for &k in earlier {
                serde_json::to_string(&self.shapes[k]).unwrap_or_default().hash(&mut h);
            }
            serde_json::to_string(s).unwrap_or_default().hash(&mut h);
            (offset.x.to_bits(), offset.y.to_bits(), pw, ph, sigma.to_bits()).hash(&mut h);
            self.page_bg().to_array().hash(&mut h);
            h.finish()
        };
        let tex = {
            let mut cache = self.sprites.borrow_mut();
            match cache.get_mut(&key) {
                Some((t, used)) => {
                    *used = pass;
                    Some(t.clone())
                }
                None => {
                    let img = self.backdrop_image(ctx, earlier, s, offset, region, sigma, pw, ph, glass);
                    let t = img.map(|i| ctx.load_texture("backdrop", i, egui::TextureOptions::LINEAR));
                    if let Some(t) = &t {
                        cache.insert(key, (t.clone(), pass));
                    }
                    t
                }
            }
        };
        let Some(tex) = tex else { return };
        painter.image(tex.id(), self.screen_rect(origin, region), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE.gamma_multiply(alpha));
    }

    // a layer that needs more than the mesh painter (effects, aligned or capped strokes, richer paints, images) is drawn from
    // a raster of its own SVG: the same drawing the export makes. The raster is cached per look and zoom, never per position.
    fn draw_sprite(&self, painter: &Painter, origin: Pos2, s: &Shape, offset: Vec2, alpha: f32) {
        let ctx = painter.ctx();
        let pass = ctx.cumulative_pass_nr();
        let mut local = s.clone();
        local.pos = Pos2::ZERO;
        local.id = 0;
        local.parent = None;
        local.opacity = 1.0;
        local.blend = crate::model::BlendMode::Normal;
        local.name.clear();
        local.link = None;
        local.tracks.clear();
        let region = local.aabb().expand(local.paint_pad() + 2.0);
        // raster resolution: the zoom in half-octave steps, never beyond 4096 px
        let want = (self.zoom * ctx.pixels_per_point()).max(0.02);
        let mut scale = 2f32.powf((want.log2() * 2.0).round() / 2.0);
        scale = scale.min(4096.0 / region.width().max(region.height()).max(1.0));
        let (pw, ph) = ((region.width() * scale).ceil().max(1.0) as u32, (region.height() * scale).ceil().max(1.0) as u32);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&local).unwrap_or_default().hash(&mut h);
            serde_json::to_string(&self.styles).unwrap_or_default().hash(&mut h);
            (pw, ph).hash(&mut h);
            h.finish()
        };
        let tex = {
            let mut cache = self.sprites.borrow_mut();
            if cache.len() > 160 {
                cache.retain(|_, (_, used)| pass.saturating_sub(*used) < 4);
            }
            match cache.get_mut(&key) {
                Some((t, used)) => {
                    *used = pass;
                    Some(t.clone())
                }
                None => {
                    let img = self.sprite_image(ctx, &local, region, pw, ph);
                    let t = img.map(|i| ctx.load_texture("sprite", i, egui::TextureOptions::LINEAR));
                    if let Some(t) = &t {
                        cache.insert(key, (t.clone(), pass));
                    }
                    t
                }
            }
        };
        let Some(tex) = tex else { return };
        let dest = self.screen_rect(origin, region.translate(s.pos.to_vec2() + offset));
        painter.image(tex.id(), dest, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE.gamma_multiply(alpha));
    }

    fn draw_grids(&self, painter: &Painter, origin: Pos2, s: &Shape, offset: Vec2) {
        let r = self.screen_rect(origin, s.rect().translate(offset));
        let painter = painter.with_clip_rect(r.intersect(painter.clip_rect()));
        for g in &s.grids {
            match g.kind {
                GridKind::Grid => {
                    let step = (g.size * self.zoom).max(2.0);
                    let mut x = r.left();
                    while x <= r.right() {
                        painter.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(1.0, g.color));
                        x += step;
                    }
                    let mut y = r.top();
                    while y <= r.bottom() {
                        painter.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], Stroke::new(1.0, g.color));
                        y += step;
                    }
                }
                GridKind::Columns | GridKind::Rows => {
                    let cols = g.kind == GridKind::Columns;
                    let total = if cols { s.size.x } else { s.size.y };
                    let n = g.count.max(1) as f32;
                    let each = (total - 2.0 * g.margin - g.gutter * (n - 1.0)) / n;
                    for k in 0..g.count {
                        let a = g.margin + k as f32 * (each + g.gutter);
                        let band = if cols {
                            Rect::from_min_size(s.pos + Vec2::new(a, 0.0), Vec2::new(each, s.size.y))
                        } else {
                            Rect::from_min_size(s.pos + Vec2::new(0.0, a), Vec2::new(s.size.x, each))
                        };
                        painter.rect_filled(self.screen_rect(origin, band.translate(offset)), 0.0, g.color);
                    }
                }
            }
        }
    }

    fn draw_text(&self, painter: &Painter, origin: Pos2, s: &Shape, offset: Vec2, alpha: f32) {
        if self.editing_text == Some(s.id) {
            return; // the live TextEdit shows it
        }
        // hovering a font in the font picker shows it on the layer
        let ctx = painter.ctx();
        if let Some((sid, fam, mono, frame)) = ctx.data(|d| d.get_temp::<(u64, String, bool, u64)>(egui::Id::new("font-preview"))) {
            if sid == s.id && frame + 1 >= ctx.cumulative_frame_nr() {
                let mut t = s.clone();
                (t.id, t.family, t.mono) = (u64::MAX, fam, mono);
                return self.draw_text(painter, origin, &t, offset, alpha);
            }
        }
        let col = self.text_color(s).gamma_multiply(alpha);
        if let Some(pts) = self.text_path_world(s) {
            // text on a path: every character stands on the outline, turned along it
            let font = FontId::new(s.font_size * self.zoom, if s.mono { FontFamily::Monospace } else { FontFamily::Proportional });
            let mut dist = 0.0;
            for ch in s.text.chars().filter(|c| *c != '\n') {
                let galley = painter.layout_no_wrap(ch.to_string(), font.clone(), col);
                let adv = galley.size().x / self.zoom;
                if let Some((at, dir)) = crate::geom::along(&pts, dist + adv / 2.0) {
                    let (tangent, normal) = (dir, Vec2::new(-dir.y, dir.x));
                    let top_left = at + offset - tangent * (adv / 2.0) - normal * (s.font_size * 0.8);
                    painter.add(TextShape::new(self.to_screen(origin, top_left), galley, col).with_angle(dir.y.atan2(dir.x)));
                }
                dist += adv;
            }
            return;
        }
        let galley = text_galley(painter, s, self.zoom, col, wrap_of(s));
        let x = match s.align {
            TextAlign::Left => s.pos.x - s.hang_shift(painter),
            TextAlign::Center => s.pos.x + s.size.x / 2.0,
            TextAlign::Right => s.pos.x + s.size.x,
        };
        // a box taller than its text puts the text at the top, middle or bottom
        let (trim_top, trim_bottom) = s.trim_px();
        let extra = ((s.size.y - galley.size().y / self.zoom + trim_top + trim_bottom).max(0.0)) * [0.0, 0.5, 1.0][s.valign.min(2) as usize] - trim_top;
        let anchor = s.xf(Pos2::new(x, s.pos.y + extra)) + offset;
        let at = self.to_screen(origin, anchor);
        if self.spell && s.rot() == 0.0 && s.list == 0 && s.runs.is_empty() && !s.text.is_empty() {
            squiggles(painter, at, &galley, &s.text);
        }
        if s.custom_underline() {
            let ul = crate::textrun::underline_marks(s, &galley, self.zoom);
            let ink = ul.color.map_or(col, |c| c.gamma_multiply(alpha));
            let (sn, cs) = s.rot().sin_cos();
            let place = |p: Pos2| at + Vec2::new(p.x * cs - p.y * sn, p.x * sn + p.y * cs);
            for m in ul.marks {
                match m {
                    crate::textrun::Ul::Line(pts) => {
                        painter.add(egui::Shape::line(pts.into_iter().map(place).collect(), Stroke::new(ul.width, ink)));
                    }
                    crate::textrun::Ul::Dot(p) => {
                        painter.circle_filled(place(p), ul.width / 2.0, ink);
                    }
                }
            }
        }
        let shape = TextShape::new(at, galley, col).with_angle(s.rot());
        painter.add(shape);
    }

    // draws the children of `parent`; `offset` is the parent's world origin
    pub fn draw_level(&self, painter: &Painter, origin: Pos2, parent: Option<u64>, offset: Vec2, alpha: f32) {
        let kids: Vec<usize> = (0..self.shapes.len()).filter(|&i| self.shapes[i].parent == parent && self.shapes[i].visible).collect();
        let Some(first) = kids.iter().position(|&i| self.shapes[i].mask) else {
            for &i in &kids {
                self.draw_shape(painter, origin, &self.shapes[i], offset, alpha);
            }
            return;
        };
        for &i in &kids[..first] {
            self.draw_shape(painter, origin, &self.shapes[i], offset, alpha);
        }
        // from the first mask up: a mask that is a plain box clips to its rect; any other outline needs a picture of the run
        let run = &kids[first..];
        if run.iter().all(|&i| !self.shapes[i].mask || self.is_box_mask(&self.shapes[i])) {
            self.draw_box_masked(painter, origin, run, offset, alpha);
        } else {
            self.draw_masked_run(painter, origin, run, offset, alpha);
        }
    }

    // a mask whose outline is its own box: an unrotated rectangle with square corners
    pub(crate) fn is_box_mask(&self, s: &Shape) -> bool {
        s.mask_kind == 0 && s.kind == Kind::Rect && s.rotation == 0.0 && s.radius.iter().all(|&r| r == 0.0)
    }

    fn draw_box_masked(&self, painter: &Painter, origin: Pos2, run: &[usize], offset: Vec2, alpha: f32) {
        let mut mask: Option<Rect> = None;
        for s in run.iter().map(|&i| &self.shapes[i]) {
            if s.mask {
                let r = self.screen_rect(origin, s.aabb().translate(offset));
                mask = Some(mask.map_or(r, |m| m.intersect(r)));
                continue;
            }
            match mask {
                Some(m) => self.draw_shape(&painter.with_clip_rect(m.intersect(painter.clip_rect())), origin, s, offset, alpha),
                None => self.draw_shape(painter, origin, s, offset, alpha),
            }
        }
    }

    // the masked siblings drawn through the SVG renderer, which clips to the mask's real outline (the export does the same);
    // the raster is cached per look and zoom like a layer's sprite
    fn draw_masked_run(&self, painter: &Painter, origin: Pos2, run: &[usize], offset: Vec2, alpha: f32) {
        let ctx = painter.ctx();
        let shapes: Vec<Shape> = run.iter().map(|&i| self.shapes[i].clone()).collect();
        // what can show: the masked layers' paint, inside every mask's box
        let mut shown: Option<Rect> = None;
        let mut within: Option<Rect> = None;
        for s in &shapes {
            let r = s.aabb().expand(s.paint_pad() + 2.0);
            if s.mask {
                within = Some(within.map_or(r, |w| w.intersect(r)));
            } else {
                shown = Some(shown.map_or(r, |u| u.union(r)));
            }
        }
        let (Some(shown), Some(within)) = (shown, within) else { return };
        let region = shown.intersect(within).translate(offset);
        if region.width() < 0.5 || region.height() < 0.5 {
            return;
        }
        let pass = ctx.cumulative_pass_nr();
        let want = (self.zoom * ctx.pixels_per_point()).max(0.02);
        let mut scale = 2f32.powf((want.log2() * 2.0).round() / 2.0);
        scale = scale.min(4096.0 / region.width().max(region.height()).max(1.0));
        let (pw, ph) = ((region.width() * scale).ceil().max(1.0) as u32, (region.height() * scale).ceil().max(1.0) as u32);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&shapes).unwrap_or_default().hash(&mut h);
            serde_json::to_string(&self.styles).unwrap_or_default().hash(&mut h);
            ((offset.x * 100.0) as i64, (offset.y * 100.0) as i64, pw, ph).hash(&mut h);
            h.finish()
        };
        let tex = {
            let mut cache = self.sprites.borrow_mut();
            if cache.len() > 160 {
                cache.retain(|_, (_, used)| pass.saturating_sub(*used) < 4);
            }
            match cache.get_mut(&key) {
                Some((t, used)) => {
                    *used = pass;
                    Some(t.clone())
                }
                None => {
                    let t = self.masked_run_image(ctx, &shapes, offset, region, pw, ph).map(|i| ctx.load_texture("masked", i, egui::TextureOptions::LINEAR));
                    if let Some(t) = &t {
                        cache.insert(key, (t.clone(), pass));
                    }
                    t
                }
            }
        };
        let Some(tex) = tex else { return };
        painter.image(tex.id(), self.screen_rect(origin, region), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE.gamma_multiply(alpha));
    }

    pub fn draw_shape(&self, painter: &Painter, origin: Pos2, s: &Shape, offset: Vec2, alpha: f32) {
        if s.repeat.is_some() {
            for c in s.repeat_copies() {
                self.draw_shape(painter, origin, &c, offset, alpha);
            }
            return;
        }
        if s.kind == Kind::Group && s.bool_op.is_some() {
            // a boolean group draws its combined shape; its layers are only edited, not drawn
            if let Some(p) = self.bool_proxy(s) {
                self.draw_shape(painter, origin, &p, offset, alpha);
            }
            return;
        }
        if s.slice {
            // a slice only marks a region: a dashed green outline, never part of an export
            let r = self.screen_rect(origin, s.rect().translate(offset));
            let green = Color32::from_rgb(0x14, 0xae, 0x5c);
            let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, green), 6.0, 4.0));
            painter.text(r.left_top() - Vec2::new(0.0, 4.0), Align2::LEFT_BOTTOM, &s.name, FontId::proportional(11.0), green);
            return;
        }
        // off-screen shapes cost tessellation and text layout every frame: skip them (margin covers strokes, effects, labels)
        let cullable = match s.kind {
            Kind::Rect | Kind::Ellipse | Kind::Line | Kind::Polygon | Kind::Star | Kind::Text => true,
            Kind::Frame => s.clip,
            Kind::Group | Kind::Path => false,
        };
        if cullable {
            let reach = s.effects.iter().filter(|e| e.visible).map(|e| e.dx.abs().max(e.dy.abs()) + e.blur + e.spread.abs()).fold(0.0, f32::max);
            let margin = (s.stroke_w + reach + if s.kind == Kind::Text { s.font_size * 2.0 } else { 0.0 }) * self.zoom + 24.0;
            if !self.screen_rect(origin, s.aabb().translate(offset)).expand(margin).intersects(painter.clip_rect()) {
                return;
            }
        }
        let alpha = alpha * s.opacity;
        if s.kind == Kind::Text {
            // text keeps a simple copy for its drop shadows
            for e in s.effects.iter().filter(|e| e.visible && e.kind == EffectKind::DropShadow) {
                let moved = Shape { pos: s.pos + Vec2::new(e.dx, e.dy), fills: vec![Paint::solid(e.color)], effects: vec![], ..s.clone() };
                self.draw_text(painter, origin, &moved, offset, alpha * e.color.a() as f32 / 255.0);
            }
            self.draw_text(painter, origin, s, offset, alpha);
            return;
        }
        // a background blur, or glass (the same blurred backdrop, bent near the edge by its Refraction)
        if let Some(e) = s.effects.iter().find(|e| e.visible && matches!(e.kind, EffectKind::BackgroundBlur | EffectKind::Glass)).filter(|_| s.has_area()) {
            self.draw_backdrop(painter, origin, s, offset, e.blur / 2.0, alpha, (e.kind == EffectKind::Glass).then_some(e));
        }
        if s.needs_sprite() && !self.video_playing(s) {
            self.draw_sprite(painter, origin, s, offset, alpha);
        } else if s.kind != Kind::Group {
            let polys = self.screen_polys(origin, s, offset);
            let bounds = self.screen_rect(origin, s.aabb().translate(offset));
            for f in s.fills.iter().filter(|f| f.visible) {
                if s.has_area() {
                    self.fill_polys(painter, &polys, f, bounds, alpha);
                }
            }
            self.stroke_polys(painter, &polys, s, alpha);
        }
        if s.kind.is_container() {
            let r = self.screen_rect(origin, if s.rot() == 0.0 { s.rect() } else { s.aabb() }.translate(offset)); // a rotated frame clips to its bounds
            if s.kind == Kind::Frame && s.parent.is_none() {
                let color = if s.component { COMPONENT } else { TEXT_SECONDARY() };
                let label = s.name.clone();
                painter.text(r.left_top() - Vec2::new(0.0, 4.0), Align2::LEFT_BOTTOM, label, FontId::proportional(11.0), color);
            }
            if let Some(code) = &s.code {
                // a code layer: the source in a monospace preview with a </> badge (the browser runs it; here it is shown as written)
                let inner = painter.with_clip_rect(r.intersect(painter.clip_rect()));
                let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, COMPONENT), 6.0, 4.0));
                let font = FontId::monospace((11.0 * self.zoom).clamp(6.0, 16.0));
                let ink = Color32::from_gray(0x5a);
                let galley = inner.layout(code.clone(), font, ink, (r.width() - 16.0).max(20.0));
                inner.galley(r.left_top() + Vec2::new(8.0, 22.0), galley, ink);
                let badge = Rect::from_min_size(r.left_top() + Vec2::new(6.0, 4.0), Vec2::new(38.0, 14.0));
                inner.rect_filled(badge, 3.0, COMPONENT);
                inner.text(badge.center(), Align2::CENTER_CENTER, "</>", FontId::monospace(10.0), Color32::BLACK);
            }
            if s.component_set {
                // Figma draws a component set with a dashed purple border
                let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, COMPONENT), 6.0, 4.0));
            }
            let clip = if s.kind == Kind::Frame && s.clip { r.intersect(painter.clip_rect()) } else { painter.clip_rect() };
            let inner = painter.with_clip_rect(clip);
            self.draw_level(&inner, origin, Some(s.id), offset + s.pos.to_vec2(), alpha);
            if s.kind == Kind::Frame && !s.grids.is_empty() {
                self.draw_grids(painter, origin, s, offset);
            }
        }
    }

    // prototype arrow from the selected shape to its link target
    pub fn draw_links(&self, painter: &Painter, origin: Pos2) {
        // a flow starting point is marked with a play badge and the flow's name above its frame
        for (i, s) in self.shapes.iter().enumerate().filter(|(_, s)| s.visible && s.flow.is_some()) {
            let r = self.screen_rect(origin, self.world_rect(i));
            let name = s.flow.as_deref().unwrap_or("");
            let font = egui::FontId::proportional(11.0);
            let w = painter.layout_no_wrap(name.to_owned(), font.clone(), egui::Color32::WHITE).size().x;
            let pill = Rect::from_min_size(Pos2::new(r.left(), r.top() - 28.0), Vec2::new(w + 30.0, 20.0));
            painter.rect_filled(pill, 10.0, Color32::from_rgb(0x0c, 0x8c, 0xe9));
            painter.add(egui::Shape::convex_polygon(vec![Pos2::new(pill.left() + 8.0, pill.center().y - 4.0), Pos2::new(pill.left() + 8.0, pill.center().y + 4.0), Pos2::new(pill.left() + 14.0, pill.center().y)], Color32::WHITE, Stroke::NONE));
            crate::fig::ptext(painter, Pos2::new(pill.left() + 20.0, pill.center().y), egui::Align2::LEFT_CENTER, name, font, Color32::WHITE);
        }
        // Motion mode shows the whole flow, otherwise only the selection's links
        let ids: Vec<u64> = if self.mode == crate::Mode::Motion { self.shapes.iter().filter(|s| s.link.is_some()).map(|s| s.id).collect() } else { self.sel.clone() };
        for &id in &ids {
            let Some(i) = self.index_of(id) else { continue };
            let Some(t) = self.shapes[i].link.and_then(|t| self.index_of(t)) else { continue };
            let a = self.screen_rect(origin, self.world_rect(i)).center();
            let b = self.screen_rect(origin, self.world_rect(t)).center();
            painter.line_segment([a, b], Stroke::new(1.5, BRAND));
            painter.circle_filled(b, 4.0, BRAND);
        }
    }
}
