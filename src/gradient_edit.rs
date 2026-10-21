// Gradient handles on the canvas, like Figma's: while a gradient fill (or stroke) is being edited, its geometry is drawn over the
// layer and can be dragged.
//   Linear: a start and an end point joined by a line (drag either; the angle and length follow), stops along the line.
//   Radial / Diamond: the centre, a handle on the horizontal radius and one on the vertical radius, stops along the horizontal one.
//   Angular: the centre and a handle for the start angle.
// A stop is dragged along the line (it keeps its colour, the list in the picker follows); clicking one selects it.
// The paint being edited is `App::grad_edit` = (layer id, is a stroke, paint index); the colour picker's row reports it each frame
// while its popover is open (`paint_row` -> `BRIDGE`), and it stays until the selection changes, Esc, or a click elsewhere.
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

use crate::model::*;
use crate::{App, Drag, Mode, Tool};

pub const G_START: u8 = 0;
pub const G_END: u8 = 1;
pub const G_CENTER: u8 = 2;
pub const G_EDGE_X: u8 = 3;
pub const G_EDGE_Y: u8 = 4;
pub const G_DIR: u8 = 5;
pub const G_STOP: u8 = 6;

// the picker row's report that a gradient popover is open: (is a stroke, paint index, UI pass)
pub fn bridge_id() -> egui::Id {
    egui::Id::new("grad-edit-bridge")
}

struct Handle {
    code: u8,
    idx: usize,
    at: Pos2,
    color: Option<Color32>,
    selected: bool,
}

struct Geom {
    lines: Vec<[Pos2; 2]>,
    handles: Vec<Handle>,
}

impl App {
    // the layer's index and the gradient of the paint being edited
    fn grad_paint(&self) -> Option<(usize, Gradient)> {
        let (id, stroke, k) = self.grad_edit?;
        let i = self.index_of(id)?;
        if self.single() != Some(i) {
            return None;
        }
        let list = if stroke { &self.shapes[i].strokes } else { &self.shapes[i].fills };
        match &list.get(k)?.kind {
            PaintKind::Gradient(g) => Some((i, g.clone())),
            PaintKind::Linear { a, b, angle } => Some((i, Gradient { angle: *angle, ..Gradient::between(*a, *b) })),
            _ => None,
        }
    }

    // follows the picker: a gradient popover that is open makes its paint the edited one; it ends with the selection
    pub fn sync_grad_edit(&mut self, ctx: &egui::Context) {
        if self.mode != Mode::Design {
            self.grad_edit = None;
            return;
        }
        let now = ctx.cumulative_pass_nr();
        if let Some((stroke, k, pass)) = ctx.data(|d| d.get_temp::<(bool, usize, u64)>(bridge_id())) {
            if pass + 1 >= now {
                if let Some(i) = self.single() {
                    self.grad_edit = Some((self.shapes[i].id, stroke, k));
                }
            }
        }
        if self.grad_edit.is_some() && self.grad_paint().is_none() {
            self.grad_edit = None;
        }
    }

    fn grad_geom(&self) -> Option<Geom> {
        let (i, g) = self.grad_paint()?;
        let b = self.world_rect(i);
        let (w, h) = (b.width().max(1e-3), b.height().max(1e-3));
        let ctr = b.min + Vec2::new(g.center.x * w, g.center.y * h);
        let mut geo = Geom { lines: vec![], handles: vec![] };
        let stop_handle = |k: usize, at: Pos2, g: &Gradient| Handle { code: G_STOP, idx: k, at, color: Some(g.stops[k].color), selected: g.selected == k };
        match g.kind {
            GradientKind::Linear => {
                let (s, c) = g.angle.to_radians().sin_cos();
                let half = (w * c.abs() + h * s.abs()) / 2.0 * g.length;
                let d = Vec2::new(c, s);
                let (a, e) = (ctr - d * half, ctr + d * half);
                geo.lines.push([a, e]);
                geo.handles.push(Handle { code: G_START, idx: 0, at: a, color: None, selected: false });
                geo.handles.push(Handle { code: G_END, idx: 0, at: e, color: None, selected: false });
                for k in 0..g.stops.len() {
                    geo.handles.push(stop_handle(k, a + (e - a) * g.stops[k].pos, &g));
                }
            }
            GradientKind::Radial | GradientKind::Diamond => {
                let (rx, ry) = (w / 2.0 * g.scale.x, h / 2.0 * g.scale.y);
                let (ex, ey) = (ctr + Vec2::new(rx, 0.0), ctr + Vec2::new(0.0, ry));
                geo.lines.push([ctr, ex]);
                geo.lines.push([ctr, ey]);
                geo.handles.push(Handle { code: G_CENTER, idx: 0, at: ctr, color: None, selected: false });
                geo.handles.push(Handle { code: G_EDGE_X, idx: 0, at: ex, color: None, selected: false });
                geo.handles.push(Handle { code: G_EDGE_Y, idx: 0, at: ey, color: None, selected: false });
                for k in 0..g.stops.len() {
                    geo.handles.push(stop_handle(k, ctr + Vec2::new(rx * g.stops[k].pos, 0.0), &g));
                }
            }
            GradientKind::Angular => {
                let r = 0.4 * w.min(h);
                let dir = ctr + Vec2::new(g.angle.to_radians().cos(), g.angle.to_radians().sin()) * r;
                geo.lines.push([ctr, dir]);
                geo.handles.push(Handle { code: G_CENTER, idx: 0, at: ctr, color: None, selected: false });
                geo.handles.push(Handle { code: G_DIR, idx: 0, at: dir, color: None, selected: false });
            }
        }
        Some(geo)
    }

    pub fn draw_gradient_handles(&self, painter: &Painter, origin: Pos2) {
        if self.mode != Mode::Design || self.tool != Tool::Select {
            return;
        }
        let Some(geo) = self.grad_geom() else { return };
        let to = |p: Pos2| self.to_screen(origin, p);
        for [a, b] in &geo.lines {
            painter.line_segment([to(*a), to(*b)], Stroke::new(3.0, Color32::from_black_alpha(90)));
            painter.line_segment([to(*a), to(*b)], Stroke::new(1.5, Color32::WHITE));
        }
        for h in &geo.handles {
            let at = to(h.at);
            match h.color {
                None => {
                    painter.circle_filled(at, 6.0, Color32::from_black_alpha(90));
                    painter.circle_filled(at, 5.0, Color32::WHITE);
                    painter.circle_filled(at, 2.5, crate::fig::SHARE);
                }
                Some(c) => {
                    let r = Rect::from_center_size(at, Vec2::splat(if h.selected { 16.0 } else { 14.0 }));
                    painter.rect_filled(r.expand(1.0), 4, Color32::from_black_alpha(90));
                    painter.rect_filled(r, 3, Color32::WHITE);
                    painter.rect_filled(r.shrink(2.0), 2, c.to_opaque());
                    if h.selected {
                        painter.rect_stroke(r.expand(1.0), 4, Stroke::new(2.0, crate::fig::SHARE), egui::StrokeKind::Outside);
                    }
                }
            }
        }
    }

    // the handle under a screen point: (code, index)
    pub fn grad_hit(&self, origin: Pos2, p: Pos2) -> Option<(u8, usize)> {
        if self.mode != Mode::Design || self.tool != Tool::Select {
            return None;
        }
        let geo = self.grad_geom()?;
        geo.handles.iter().rev().find(|h| (self.to_screen(origin, h.at) - p).length() <= 9.0).map(|h| (h.code, h.idx))
    }

    // a click (no drag) on a handle: a stop becomes the selected one. True when it was on a handle.
    pub fn grad_click(&mut self, origin: Pos2, p: Pos2) -> bool {
        let Some((code, idx)) = self.grad_hit(origin, p) else { return false };
        if code == G_STOP {
            self.grad_write(|g| g.selected = idx.min(g.stops.len() - 1));
        }
        true
    }

    fn grad_write(&mut self, edit: impl FnOnce(&mut Gradient)) {
        let Some((i, mut g)) = self.grad_paint() else { return };
        let Some((_, stroke, k)) = self.grad_edit else { return };
        edit(&mut g);
        let list = if stroke { &mut self.shapes[i].strokes } else { &mut self.shapes[i].fills };
        list[k].kind = PaintKind::Gradient(g);
    }

    // dragging a handle to world point `q`
    pub fn grad_drag(&mut self, mut code: u8, idx: usize, q: Pos2) {
        let Some((i, g)) = self.grad_paint() else { return };
        // a stop that sits on the end of the line or on the centre is the handle under it: dragging it moves that handle
        if code == G_STOP {
            if let Some(st) = g.stops.get(idx) {
                code = match g.kind {
                    GradientKind::Linear if st.pos <= 0.001 => G_START,
                    GradientKind::Linear if st.pos >= 0.999 => G_END,
                    GradientKind::Radial | GradientKind::Diamond if st.pos <= 0.001 => G_CENTER,
                    _ => G_STOP,
                };
            }
        }
        let b = self.world_rect(i);
        let (w, h) = (b.width().max(1e-3), b.height().max(1e-3));
        let ctr = b.min + Vec2::new(g.center.x * w, g.center.y * h);
        let norm = |p: Pos2| Pos2::new((p.x - b.min.x) / w, (p.y - b.min.y) / h);
        // the current line of a linear gradient
        let (s, c) = g.angle.to_radians().sin_cos();
        let half = (w * c.abs() + h * s.abs()) / 2.0 * g.length;
        let (a, e) = (ctr - Vec2::new(c, s) * half, ctr + Vec2::new(c, s) * half);
        let mut keep_stop = None;
        self.grad_write(|g| match code {
            G_START | G_END => {
                let (from, to) = if code == G_START { (q, e) } else { (a, q) };
                let d = to - from;
                if d.length() < 1e-3 {
                    return;
                }
                g.angle = d.y.atan2(d.x).to_degrees();
                let (sn, cs) = g.angle.to_radians().sin_cos();
                g.length = (d.length() / (w * cs.abs() + h * sn.abs()).max(1e-3)).max(0.01);
                g.center = norm(from + d / 2.0);
            }
            G_CENTER => g.center = norm(q),
            G_EDGE_X => g.scale.x = ((q.x - ctr.x).abs() / (w / 2.0)).max(0.02),
            G_EDGE_Y => g.scale.y = ((q.y - ctr.y).abs() / (h / 2.0)).max(0.02),
            G_DIR => g.angle = (q - ctr).y.atan2((q - ctr).x).to_degrees(),
            G_STOP => {
                let t = match g.kind {
                    GradientKind::Linear => {
                        let d = e - a;
                        ((q - a).dot(d) / d.length_sq().max(1e-6)).clamp(0.0, 1.0)
                    }
                    GradientKind::Radial | GradientKind::Diamond => ((q.x - ctr.x) / (w / 2.0 * g.scale.x).max(1e-3)).clamp(0.0, 1.0),
                    GradientKind::Angular => return,
                };
                if idx < g.stops.len() {
                    g.stops[idx].pos = t;
                    g.selected = idx;
                    g.sort();
                    keep_stop = Some(g.selected);
                }
            }
            _ => {}
        });
        if let Some(k) = keep_stop {
            // the list is sorted: keep dragging the same stop
            self.drag = Drag::Gradient(G_STOP, k);
        }
    }
}
