// Vector work: the pen tool, node editing, flattening shapes to paths, boolean operations.
use egui::{Pos2, Vec2};

use crate::geom::{BoolOp, Poly, bool_op, flatten};
use crate::model::*;
use crate::{App, Tool};

fn lerp(a: Pos2, b: Pos2, t: f32) -> Pos2 {
    a + (b - a) * t
}

impl App {
    // ---- pen tool

    // a click with the pen: add a corner point, or close the path on its first point
    pub fn pen_click(&mut self, w: Pos2) {
        if self.pen.len() >= 2 && (w - self.pen[0].p).length() * self.zoom <= 8.0 {
            self.finish_path(true);
        } else {
            let w = self.snap_pos(w, &[]);
            self.pen.push(PathPt::corner(w));
        }
    }

    // dragging after placing a point pulls out smooth, symmetric handles
    pub fn pen_drag(&mut self, w: Pos2) {
        if let Some(last) = self.pen.last_mut() {
            last.hout = w - last.p;
            last.hin = -last.hout;
        }
    }

    pub fn pen_hits_start(&self, w: Pos2) -> bool {
        self.pen.len() >= 2 && (w - self.pen[0].p).length() * self.zoom <= 8.0
    }

    // dragging from the first point pulls the handles of the closing corner (smooth, symmetric)
    pub fn pen_close_drag(&mut self, w: Pos2) {
        if let Some(first) = self.pen.first_mut() {
            if (w - first.p).length() * self.zoom > 3.0 {
                first.hout = w - first.p;
                first.hin = -first.hout;
            }
        }
    }

    pub fn finish_pen_closed(&mut self) {
        if !self.pen.is_empty() {
            self.finish_path(true);
        }
    }

    pub fn finish_pen(&mut self) {
        if !self.pen.is_empty() {
            self.finish_path(false);
        }
    }

    fn finish_path(&mut self, closed: bool) {
        let pts = std::mem::take(&mut self.pen);
        if pts.len() < 2 {
            return;
        }
        self.finish_contours(vec![Contour { pts, closed, hole: false }], closed);
    }

    // a new path layer from contours in world coordinates; open ones get a stroke, closed ones a fill
    fn finish_contours(&mut self, world: Vec<Contour>, closed: bool) {
        let Some(first) = world.first().and_then(|c| c.pts.first()) else { return };
        let parent = self.frame_at(first.p, &[]);
        let o = self.origin_of(parent);
        let id = self.new_id();
        let mut s = Shape::new(Kind::Path, id, self.shapes.len() + 1, Pos2::ZERO, parent);
        s.name = format!("Vector {}", self.shapes.len() + 1);
        if !closed {
            s.fills.clear();
            s.strokes = vec![Paint::solid(GREY_100)];
            s.stroke_w = 2.0;
        }
        let local: Vec<Contour> = world.into_iter().map(|c| Contour { pts: c.pts.iter().map(|q| PathPt { p: q.p - o, ..*q }).collect(), ..c }).collect();
        s.set_path_world(&local);
        self.shapes.push(s);
        self.sel = vec![id];
        self.tool = Tool::Select;
    }

    // ---- node editing

    pub fn start_node_edit(&mut self, i: usize) {
        if self.shapes[i].kind != Kind::Path {
            return;
        }
        let baked = self.shapes[i].path_world(); // makes the shape unrotated
        self.shapes[i].set_path_world(&baked);
        self.node_edit = true;
        self.node_sel = None;
    }

    // turns the selected plain shape into a path and edits its nodes
    pub fn flatten_then_edit(&mut self) {
        self.flatten();
        if let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Path) {
            self.start_node_edit(i);
        }
    }

    // anchors and handles of the path being edited, in world coordinates
    pub fn node_points(&self, i: usize) -> Vec<Contour> {
        let o = self.origin_of(self.shapes[i].parent);
        let mut v = self.shapes[i].path_world();
        for c in &mut v {
            for q in &mut c.pts {
                q.p += o;
            }
        }
        v
    }

    pub fn node_move(&mut self, c: usize, k: usize, part: u8, world: Pos2) {
        let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Path) else { return };
        let o = self.origin_of(self.shapes[i].parent);
        let mut v = self.shapes[i].path_world();
        let local = world - o;
        let Some(old_p) = v.get(c).and_then(|c| c.pts.get(k)).map(|q| q.p) else { return };
        if part == 0 {
            // the vertices of other contours that sat on this one (where a branch leaves) move with it
            let moved = local - old_p;
            for (ci, cc) in v.iter_mut().enumerate() {
                for (pi, pt) in cc.pts.iter_mut().enumerate() {
                    if (ci, pi) != (c, k) && (pt.p - old_p).length() < 0.01 {
                        pt.p += moved;
                    }
                }
            }
            v[c].pts[k].p = local;
            self.shapes[i].set_path_world(&v);
            return;
        }
        let mirror = self.handle_mirror;
        let q = &mut v[c].pts[k];
        match part {
            1 => {
                let follow = mirror == 2 || (mirror == 1 && collinear(q.hin, q.hout));
                let other = q.hout;
                q.hin = local - q.p;
                if follow && other != Vec2::ZERO && q.hin != Vec2::ZERO {
                    q.hout = -q.hin.normalized() * if mirror == 2 { q.hin.length() } else { other.length() };
                }
            }
            _ => {
                let follow = mirror == 2 || (mirror == 1 && collinear(q.hin, q.hout));
                let other = q.hin;
                q.hout = local - q.p;
                if follow && other != Vec2::ZERO && q.hout != Vec2::ZERO {
                    q.hin = -q.hout.normalized() * if mirror == 2 { q.hout.length() } else { other.length() };
                }
            }
        }
        self.shapes[i].set_path_world(&v);
    }

    // a branch: a new open contour that starts on vertex `k` of contour `c` and has one more point, to be dragged out (the new point
    // is (returned contour, 1)); the two contours share the vertex and it moves them both
    pub fn node_add_branch(&mut self, c: usize, k: usize) -> Option<usize> {
        let i = self.single().filter(|&i| self.shapes[i].kind == Kind::Path)?;
        let mut v = self.shapes[i].path_world();
        let at = v.get(c)?.pts.get(k)?.p;
        v.push(Contour { pts: vec![PathPt::corner(at), PathPt::corner(at + Vec2::new(24.0, 0.0))], closed: false, hole: false });
        let n = v.len() - 1;
        self.shapes[i].set_path_world(&v);
        self.node_sel = Some((n, 1));
        Some(n)
    }

    // Simplify vector: every selected path is flattened, thinned with Douglas-Peucker to within `tolerance` px and fitted with smooth
    // curves again, so a hand-drawn path with many points becomes one with few
    pub fn simplify_vector(&mut self, tolerance: f32) -> usize {
        let mut removed = 0;
        for id in self.sel_roots() {
            let Some(i) = self.index_of(id).filter(|&i| self.shapes[i].kind == Kind::Path) else { continue };
            let mut v = self.shapes[i].path_world();
            for c in v.iter_mut() {
                let before = c.pts.len();
                let mut pts = rdp(&flatten(c, 8.0), tolerance);
                if c.closed && pts.len() > 2 && (pts[0] - pts[pts.len() - 1]).length() < 1e-3 {
                    pts.pop();
                }
                if pts.len() >= 2 && pts.len() < before * 2 {
                    c.pts = smooth(&pts);
                    removed += before.saturating_sub(c.pts.len());
                }
            }
            self.shapes[i].set_path_world(&v);
        }
        removed
    }

    // a new edge between two vertices (an open contour of two points that share them), so a path can form a network
    pub fn node_connect_to(&mut self, from: (usize, usize), to: (usize, usize)) -> bool {
        let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Path) else { return false };
        if from == to {
            return false;
        }
        let mut v = self.shapes[i].path_world();
        let (Some(a), Some(b)) = (v.get(from.0).and_then(|c| c.pts.get(from.1)).map(|q| q.p), v.get(to.0).and_then(|c| c.pts.get(to.1)).map(|q| q.p)) else { return false };
        v.push(Contour { pts: vec![PathPt::corner(a), PathPt::corner(b)], closed: false, hole: false });
        self.shapes[i].set_path_world(&v);
        true
    }

    // the corner radius of one vertex (0 = sharp)
    pub fn node_set_radius(&mut self, c: usize, k: usize, radius: f32) {
        let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Path) else { return };
        let Some(q) = self.shapes[i].path.get_mut(c).and_then(|c| c.pts.get_mut(k)) else { return };
        q.radius = radius.max(0.0);
    }

    pub fn node_radius(&self, c: usize, k: usize) -> Option<f32> {
        let i = self.single().filter(|&i| self.shapes[i].kind == Kind::Path)?;
        self.shapes[i].path.get(c)?.pts.get(k).map(|q| q.radius)
    }

    // the Bend tool dragged over a vertex: its handles are pulled out of it symmetrically toward the pointer
    pub fn node_bend(&mut self, c: usize, k: usize, world: Pos2) {
        let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Path) else { return };
        let o = self.origin_of(self.shapes[i].parent);
        let mut v = self.shapes[i].path_world();
        let Some(q) = v.get_mut(c).and_then(|c| c.pts.get_mut(k)) else { return };
        let d = world - o - q.p;
        q.hout = d;
        q.hin = -d;
        q.radius = 0.0;
        self.shapes[i].set_path_world(&v);
    }

    // a click of the Bend tool on a curved vertex makes it a sharp corner again
    pub fn node_straighten(&mut self, c: usize, k: usize) {
        let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Path) else { return };
        let mut v = self.shapes[i].path_world();
        let Some(q) = v.get_mut(c).and_then(|c| c.pts.get_mut(k)) else { return };
        q.hin = Vec2::ZERO;
        q.hout = Vec2::ZERO;
        self.shapes[i].set_path_world(&v);
    }

    pub fn node_is_curved(&self, c: usize, k: usize) -> bool {
        self.node_sel.is_some() && self.single().filter(|&i| self.shapes[i].kind == Kind::Path).and_then(|i| self.shapes[i].path.get(c)?.pts.get(k)).is_some_and(|q| q.hin != Vec2::ZERO || q.hout != Vec2::ZERO)
    }

    // sharp <-> smooth for one anchor
    pub fn node_toggle_smooth(&mut self, c: usize, k: usize) {
        let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Path) else { return };
        let mut v = self.shapes[i].path_world();
        let Some(contour) = v.get_mut(c) else { return };
        let n = contour.pts.len();
        let (prev, next) = (
            if k > 0 || contour.closed { Some(contour.pts[(k + n - 1) % n].p) } else { None },
            if k + 1 < n || contour.closed { Some(contour.pts[(k + 1) % n].p) } else { None },
        );
        let q = &mut contour.pts[k];
        if q.hin == Vec2::ZERO && q.hout == Vec2::ZERO {
            let dir = match (prev, next) {
                (Some(a), Some(b)) => b - a,
                (None, Some(b)) => b - q.p,
                (Some(a), None) => q.p - a,
                _ => Vec2::ZERO,
            };
            let len = [prev, next].iter().flatten().map(|p| (*p - q.p).length()).fold(f32::MAX, f32::min) / 3.0;
            if dir != Vec2::ZERO && len.is_finite() {
                q.hout = dir.normalized() * len;
                q.hin = -q.hout;
            }
        } else {
            q.hin = Vec2::ZERO;
            q.hout = Vec2::ZERO;
        }
        self.shapes[i].set_path_world(&v);
    }

    pub fn delete_node(&mut self) {
        let (Some((c, k)), Some(i)) = (self.node_sel, self.single()) else { return };
        if self.shapes[i].kind != Kind::Path {
            return;
        }
        let mut v = self.shapes[i].path_world();
        if c < v.len() && k < v[c].pts.len() {
            v[c].pts.remove(k);
            if v[c].pts.len() < 2 {
                v.remove(c);
            }
        }
        self.node_sel = None;
        if v.is_empty() {
            let id = self.shapes[i].id;
            self.remove_subtree(id);
            self.node_edit = false;
        } else {
            self.shapes[i].set_path_world(&v);
        }
    }

    // inserts an anchor on the segment nearest to world point `w`, keeping the curve's shape
    pub fn node_insert_at(&mut self, w: Pos2) -> bool {
        let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Path) else { return false };
        let o = self.origin_of(self.shapes[i].parent);
        let p = w - o;
        let mut v = self.shapes[i].path_world();
        let mut best: Option<(f32, usize, usize, f32)> = None;
        for (ci, c) in v.iter().enumerate() {
            let n = c.pts.len();
            let segs = if c.closed { n } else { n.saturating_sub(1) };
            for k in 0..segs {
                let (a, b) = (c.pts[k], c.pts[(k + 1) % n]);
                for step in 1..32 {
                    let t = step as f32 / 32.0;
                    let d = (bezier(a, b, t) - p).length();
                    if best.is_none_or(|(bd, ..)| d < bd) {
                        best = Some((d, ci, k, t));
                    }
                }
            }
        }
        let Some((d, ci, k, t)) = best else { return false };
        if d * self.zoom > 10.0 {
            return false;
        }
        split_segment(&mut v[ci], k, t);
        self.node_sel = Some((ci, k + 1));
        self.shapes[i].set_path_world(&v);
        true
    }

    // ---- flatten

    // the shape's outline as editable contours in its parent's space; None for text and containers
    fn path_contours(&self, i: usize) -> Option<Vec<Contour>> {
        let s = &self.shapes[i];
        match s.kind {
            Kind::Path => Some(s.path_world()),
            Kind::Text | Kind::Group | Kind::Frame => None,
            Kind::Line => Some(vec![Contour { pts: vec![PathPt::corner(s.pos), PathPt::corner(s.pos + s.size)], closed: false, hole: false }]),
            _ => Some(s.polys(4.0).into_iter().map(|p| Contour { pts: p.pts.into_iter().map(PathPt::corner).collect(), closed: p.closed, hole: p.hole }).collect()),
        }
    }

    pub fn flatten(&mut self) {
        for id in self.sel_roots() {
            let Some(i) = self.index_of(id) else { continue };
            if self.shapes[i].kind == Kind::Text {
                let keep = std::mem::replace(&mut self.sel, vec![id]);
                self.text_to_vector();
                self.sel = keep;
                continue;
            }
            // a boolean group becomes the one vector it shows
            if self.shapes[i].bool_op.is_some() {
                if let Some(mut p) = self.bool_proxy(&self.shapes[i].clone()) {
                    p.name = self.shapes[i].name.clone();
                    let doomed: Vec<u64> = self.shapes.iter().filter(|s| s.id != id && self.is_inside(s.id, id)).map(|s| s.id).collect();
                    self.shapes[i] = p;
                    self.shapes.retain(|s| !doomed.contains(&s.id));
                }
                continue;
            }
            if let Some(c) = self.path_contours(i) {
                let s = &mut self.shapes[i];
                s.set_path_world(&c);
                s.kind = Kind::Path;
                s.radius = [0.0; 4];
            }
        }
    }

    // Ctrl+Shift+O: the stroke becomes a filled shape. A layer that also has a fill keeps it and gets the outline as a new
    // layer above it; a layer with only a stroke turns into the outline itself.
    pub fn outline_stroke(&mut self) {
        let mut made = vec![];
        for id in self.sel_roots() {
            let Some(i) = self.index_of(id) else { continue };
            let s = self.shapes[i].clone();
            if s.kind.is_container() || s.kind == Kind::Text || s.strokes.is_empty() || s.stroke_w <= 0.0 {
                continue;
            }
            let contours = crate::geom::outline_stroke(&s.polys(4.0), s.stroke_w, s.stroke_join, s.miter_angle, s.stroke_cap, s.stroke_align);
            if contours.is_empty() {
                continue;
            }
            let strokes = s.strokes.clone();
            if s.fills.iter().any(|f| f.visible) && s.kind != Kind::Line {
                let nid = self.new_id();
                let mut o = Shape::new(Kind::Path, nid, self.shapes.len() + 1, Pos2::ZERO, s.parent);
                o.name = format!("{} stroke", s.name);
                o.fills = strokes;
                o.strokes.clear();
                o.stroke_w = 0.0;
                o.set_path_world(&contours);
                self.shapes[i].strokes.clear();
                self.shapes.insert(i + 1, o);
                made.push(nid);
            } else {
                let t = &mut self.shapes[i];
                t.set_path_world(&contours);
                t.kind = Kind::Path;
                t.radius = [0.0; 4];
                t.fills = strokes;
                t.strokes.clear();
                t.stroke_w = 0.0;
                t.stroke_cap = [StrokeCap::None; 2];
                made.push(id);
            }
        }
        if !made.is_empty() {
            self.sel = made;
        }
    }

    // Offset path: closed outlines grown by `d` px (shrunk when negative), turned into paths
    pub fn offset_selection(&mut self, d: f32) {
        for id in self.sel_roots() {
            let Some(i) = self.index_of(id) else { continue };
            let s = &self.shapes[i];
            if s.kind.is_container() || s.kind == Kind::Text || s.kind == Kind::Line {
                continue;
            }
            let contours = crate::geom::offset_path(&s.polys(4.0), d);
            if contours.is_empty() {
                continue;
            }
            let t = &mut self.shapes[i];
            t.set_path_world(&contours);
            t.kind = Kind::Path;
            t.radius = [0.0; 4];
        }
    }

    // ---- boolean operations

    // world-space outline of a shape or, for a container, the union of everything inside it
    fn operand_polys(&self, id: u64) -> Vec<Poly> {
        if self.index_of(id).is_some_and(|g| self.shapes[g].bool_op.is_some()) {
            return self.bool_result_world(id).into_iter().map(|c| Poly { pts: flatten(&c, 1.0), closed: true, hole: c.hole }).collect();
        }
        let leaves: Vec<Vec<Poly>> = self
            .shapes
            .iter()
            .enumerate()
            .filter(|(_, s)| self.is_inside(s.id, id) && !s.kind.is_container() && s.has_area() && s.visible)
            .map(|(_, s)| {
                let o = self.origin_of(s.parent);
                s.polys(4.0)
                    .into_iter()
                    .map(|mut p| {
                        p.pts.iter_mut().for_each(|q| *q += o);
                        p
                    })
                    .collect()
            })
            .collect();
        if leaves.len() <= 1 {
            return leaves.into_iter().next().unwrap_or_default();
        }
        bool_op(BoolOp::Union, &leaves).into_iter().map(|c| Poly { pts: flatten(&c, 1.0), closed: true, hole: c.hole }).collect()
    }

    // the operation a boolean group applies
    pub fn group_op(s: &Shape) -> Option<BoolOp> {
        s.bool_op.map(|k| [BoolOp::Union, BoolOp::Subtract, BoolOp::Intersect, BoolOp::Exclude][k.min(3) as usize])
    }

    // what a boolean group shows: its layers combined, in world space (the layers stay in the group, bottom to top)
    pub fn bool_result_world(&self, gid: u64) -> Vec<Contour> {
        let Some(g) = self.index_of(gid) else { return vec![] };
        let Some(op) = Self::group_op(&self.shapes[g]) else { return vec![] };
        let operands: Vec<Vec<Poly>> = self.shapes.iter().filter(|s| s.parent == Some(gid) && s.visible).map(|s| self.operand_polys(s.id)).collect();
        bool_op(op, &operands)
    }

    // a boolean group as the plain path it draws and exports as, in its parent's space
    pub fn bool_proxy(&self, g: &Shape) -> Option<Shape> {
        let mut contours = self.bool_result_world(g.id);
        if contours.is_empty() {
            return None;
        }
        let o = self.origin_of(g.parent);
        for c in &mut contours {
            c.pts.iter_mut().for_each(|q| q.p -= o);
        }
        let mut p = g.clone();
        p.kind = Kind::Path;
        p.bool_op = None;
        p.rotation = 0.0;
        p.set_path_world(&contours);
        Some(p)
    }

    // Figma's boolean groups: the selection becomes a group that shows the combined shape and keeps its layers, so the
    // operation can change and the layers can still be edited. A selected boolean group just takes the new operation.
    pub fn boolean(&mut self, op: BoolOp) {
        let kind = match op {
            BoolOp::Union => 0,
            BoolOp::Subtract => 1,
            BoolOp::Intersect => 2,
            BoolOp::Exclude => 3,
        };
        let name = ["Union", "Subtract", "Intersect", "Exclude"][kind as usize];
        if let Some(g) = self.single().filter(|&i| self.shapes[i].bool_op.is_some()) {
            self.shapes[g].bool_op = Some(kind);
            self.shapes[g].name = name.into();
            return;
        }
        let roots = self.sel_roots();
        if roots.len() < 2 {
            return;
        }
        let first = self.index_of(roots[0]).unwrap();
        let (fills, strokes, stroke_w, opacity) = (self.shapes[first].fills.clone(), self.shapes[first].strokes.clone(), self.shapes[first].stroke_w, self.shapes[first].opacity);
        self.group();
        let Some(g) = self.single().filter(|&i| self.shapes[i].kind == Kind::Group) else { return };
        let s = &mut self.shapes[g];
        s.bool_op = Some(kind);
        s.name = name.into();
        s.fills = fills;
        s.strokes = strokes;
        s.stroke_w = stroke_w;
        s.opacity = opacity;
        if self.bool_result_world(self.shapes[g].id).is_empty() {
            self.status = "boolean result is empty".into();
        }
    }
}

fn collinear(a: Vec2, b: Vec2) -> bool {
    a != Vec2::ZERO && b != Vec2::ZERO && (a.x * b.y - a.y * b.x).abs() < 1e-3 * a.length() * b.length() && a.dot(b) < 0.0
}

fn bezier(a: PathPt, b: PathPt, t: f32) -> Pos2 {
    let (p0, p1, p2, p3) = (a.p, a.p + a.hout, b.p + b.hin, b.p);
    let u = 1.0 - t;
    (p0.to_vec2() * (u * u * u) + p1.to_vec2() * (3.0 * u * u * t) + p2.to_vec2() * (3.0 * u * t * t) + p3.to_vec2() * (t * t * t)).to_pos2()
}

// ---- pencil: freehand strokes become smooth open paths

// splits the bezier segment after point `k` at `t` (de Casteljau): the outline does not change
pub fn split_segment(c: &mut Contour, k: usize, t: f32) {
    let n = c.pts.len();
    let (a, b) = (c.pts[k], c.pts[(k + 1) % n]);
    let (p0, p1, p2, p3) = (a.p, a.p + a.hout, b.p + b.hin, b.p);
    let (q0, q1, q2) = (lerp(p0, p1, t), lerp(p1, p2, t), lerp(p2, p3, t));
    let (r0, r1) = (lerp(q0, q1, t), lerp(q1, q2, t));
    let m = lerp(r0, r1, t);
    c.pts[k].hout = q0 - p0;
    c.pts[(k + 1) % n].hin = q2 - p3;
    c.pts.insert(k + 1, PathPt { p: m, hin: r0 - m, hout: r1 - m, radius: 0.0 });
}

// `c` with points added (longest segments split in half first) until it has `n`
fn with_points(c: &Contour, n: usize) -> Contour {
    let mut c = c.clone();
    while c.pts.len() < n && c.pts.len() >= 2 {
        let segs = if c.closed { c.pts.len() } else { c.pts.len() - 1 };
        let len = |k: usize| (c.pts[(k + 1) % c.pts.len()].p - c.pts[k].p).length();
        let k = (0..segs).max_by(|&x, &y| len(x).total_cmp(&len(y))).unwrap_or(0);
        split_segment(&mut c, k, 0.5);
    }
    c
}

// a path `e` of the way from contours `a` to `b` (both in the unit box): point counts are evened out by
// splitting segments, which keeps each outline, then points and handles are blended.
// None when the two have a different number of contours.
pub fn morph(a: &[Contour], b: &[Contour], e: f32) -> Option<Vec<Contour>> {
    if a.len() != b.len() {
        return None;
    }
    let mix = |x: Vec2, y: Vec2| x + (y - x) * e;
    Some(
        a.iter()
            .zip(b)
            .map(|(ca, cb)| {
                let n = ca.pts.len().max(cb.pts.len());
                let (ca, cb) = (with_points(ca, n), with_points(cb, n));
                let pts = ca.pts.iter().zip(&cb.pts).map(|(p, q)| PathPt { p: (mix(p.p.to_vec2(), q.p.to_vec2())).to_pos2(), hin: mix(p.hin, q.hin), hout: mix(p.hout, q.hout), radius: 0.0 }).collect();
                Contour { pts, closed: cb.closed, hole: cb.hole }
            })
            .collect(),
    )
}

// Ramer-Douglas-Peucker: drops points closer than `eps` to the line through their neighbours
pub fn rdp(pts: &[Pos2], eps: f32) -> Vec<Pos2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let ab = b - a;
    let dist = |p: Pos2| {
        let len = ab.length();
        if len < 1e-6 { (p - a).length() } else { (ab.x * (p - a).y - ab.y * (p - a).x).abs() / len }
    };
    let (k, d) = pts[1..pts.len() - 1].iter().enumerate().map(|(k, &p)| (k + 1, dist(p))).fold((0, -1.0f32), |m, x| if x.1 > m.1 { x } else { m });
    if d <= eps {
        return vec![a, b];
    }
    let mut left = rdp(&pts[..=k], eps);
    left.pop(); // pts[k] starts the right half
    left.extend(rdp(&pts[k..], eps));
    left
}

// Catmull-Rom through the points, as cubic bezier handles (ends use their single neighbour)
pub fn smooth(pts: &[Pos2]) -> Vec<PathPt> {
    (0..pts.len())
        .map(|i| {
            let prev = pts[i.saturating_sub(1)];
            let next = pts[(i + 1).min(pts.len() - 1)];
            let hout = (next - prev) / 6.0;
            PathPt { p: pts[i], hin: -hout, hout, radius: 0.0 }
        })
        .collect()
}

// Draw toolbar brushes. Every one but Pencil makes a filled shape
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Brush {
    Pencil,      // an even stroke
    Pressure,    // width follows pen pressure (speed with a mouse)
    Taper,       // thin at both ends, full width in between
    Calligraphy, // a flat nib held at an angle: thin along the nib, wide across it
    Scatter,     // a trail of dots of varying size: a texture brush
    Stretch,     // an image stretched along the stroke
    Layer,       // copies of layers made into a brush (Create brush) scattered along the stroke
}

impl Brush {
    pub const ALL: [(Brush, &'static str); 7] = [
        (Brush::Pencil, "Pencil"),
        (Brush::Pressure, "Pressure"),
        (Brush::Taper, "Taper"),
        (Brush::Calligraphy, "Calligraphy"),
        (Brush::Scatter, "Scatter"),
        (Brush::Stretch, "Stretch"),
        (Brush::Layer, "Layer brush"),
    ];

    pub fn label(self) -> &'static str {
        Brush::ALL.iter().find(|b| b.0 == self).map_or("", |b| b.1)
    }
}

// how wide the stroke is at each point, as a fraction of the brush width
pub fn brush_widths(brush: Brush, pts: &[Pos2], force: &[f32], nib_deg: f32) -> Vec<f32> {
    let n = pts.len();
    let mut along = vec![0.0f32; n];
    for i in 1..n {
        along[i] = along[i - 1] + (pts[i] - pts[i - 1]).length();
    }
    let total = along.last().copied().unwrap_or(0.0).max(1e-3);
    let nib = Vec2::angled(nib_deg.to_radians());
    (0..n)
        .map(|i| match brush {
            Brush::Pencil | Brush::Scatter | Brush::Stretch | Brush::Layer => 1.0,
            Brush::Pressure => force.get(i).copied().unwrap_or(1.0),
            // ramps over the first and last 30% of the stroke
            Brush::Taper => {
                let ramp = (total * 0.3).max(1e-3);
                (along[i] / ramp).min((total - along[i]) / ramp).clamp(0.0, 1.0).sqrt().max(0.08)
            }
            Brush::Calligraphy => {
                let d = pts[(i + 1).min(n - 1)] - pts[i.saturating_sub(1)];
                let d = if d.length() > 1e-6 { d.normalized() } else { Vec2::X };
                // wide when the stroke crosses the nib, thin when it runs along it
                (d.x * nib.y - d.y * nib.x).abs().max(0.15)
            }
        })
        .collect()
}

// a closed circle of four bezier arcs
fn circle(c: Pos2, r: f32) -> Contour {
    const K: f32 = 0.552_284_8;
    let pts = [Vec2::new(r, 0.0), Vec2::new(0.0, r), Vec2::new(-r, 0.0), Vec2::new(0.0, -r)]
        .iter()
        .map(|&v| {
            let h = Vec2::new(-v.y, v.x) * K;
            PathPt { p: c + v, hin: -h, hout: h, radius: 0.0 }
        })
        .collect();
    Contour { pts, closed: true, hole: false }
}

// the Scatter brush: dots every so often along the stroke, jittered across it, sized by pressure
pub fn scatter(pts: &[Pos2], force: &[f32], w: f32) -> Vec<Contour> {
    // a small deterministic hash, so the same stroke always gives the same dots
    let rand = |k: u32| {
        let mut x = k.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
        x ^= x >> 15;
        x = x.wrapping_mul(0x2C1B_3C6D);
        x ^= x >> 12;
        (x % 10_000) as f32 / 10_000.0
    };
    let step = (w * 0.7).max(1.0);
    let mut out = vec![];
    let mut next = 0.0;
    let mut along = 0.0;
    for i in 1..pts.len() {
        let seg = pts[i] - pts[i - 1];
        let len = seg.length();
        while next <= along + len && len > 1e-6 {
            let k = out.len() as u32;
            let p = pts[i - 1] + seg * ((next - along) / len);
            let across = Vec2::new(-seg.y, seg.x) / len * ((rand(k * 3) - 0.5) * w);
            let r = w / 2.0 * (0.35 + 0.65 * rand(k * 3 + 1)) * force.get(i).copied().unwrap_or(1.0).max(0.3);
            out.push(circle(p + across, r));
            next += step * (0.6 + 0.8 * rand(k * 3 + 2));
        }
        along += len;
    }
    out
}

// the outline of a brush stroke along `pts` whose width at each point is `w` times its pressure:
// the left side, a round cap, the right side back, a round cap (a closed polygon, not repeated at the end)
pub fn brush_outline(pts: &[Pos2], force: &[f32], w: f32) -> Vec<Pos2> {
    let n = pts.len();
    if n < 2 {
        return pts.to_vec();
    }
    let half = |i: usize| (w * force.get(i).copied().unwrap_or(1.0)).max(0.05) / 2.0;
    let normal = |i: usize| {
        let d = pts[(i + 1).min(n - 1)] - pts[i.saturating_sub(1)];
        let d = if d.length() > 1e-6 { d.normalized() } else { Vec2::X };
        Vec2::new(-d.y, d.x)
    };
    let mut out: Vec<Pos2> = (0..n).map(|i| pts[i] + normal(i) * half(i)).collect();
    // a half circle around an end, from the side `n` points to, through the direction `ahead`
    let cap = |out: &mut Vec<Pos2>, c: Pos2, nrm: Vec2, r: f32| {
        let ahead = Vec2::new(nrm.y, -nrm.x);
        for k in 1..6 {
            let a = std::f32::consts::PI * k as f32 / 6.0;
            out.push(c + nrm * (r * a.cos()) + ahead * (r * a.sin()));
        }
    };
    cap(&mut out, pts[n - 1], normal(n - 1), half(n - 1));
    out.extend((0..n).rev().map(|i| pts[i] - normal(i) * half(i)));
    cap(&mut out, pts[0], -normal(0), half(0));
    out
}

impl App {
    pub fn finish_pencil(&mut self) {
        let raw = std::mem::take(&mut self.pencil);
        let force = std::mem::take(&mut self.pencil_force);
        let pts = rdp(&raw, 1.5 / self.zoom);
        if pts.len() < 2 || (pts[pts.len() - 1] - pts[0]).length() * self.zoom < 2.0 && pts.len() == 2 {
            return; // a click, not a stroke
        }
        if self.brush == Brush::Layer {
            self.finish_layer_brush(&raw);
        } else if self.brush == Brush::Scatter {
            let dots = scatter(&raw, &force, self.brush_w);
            self.finish_contours(dots, true);
            if let Some(i) = self.single() {
                self.shapes[i].name = format!("Scatter {}", self.shapes.len());
            }
        } else if self.brush == Brush::Stretch {
            // the picture goes along the smoothed stroke; the layer itself has no fill or stroke
            let Some(image) = self.stretch_image.filter(|id| self.images.iter().any(|a| a.id == *id)).or_else(|| self.images.first().map(|a| a.id)) else {
                self.status = "Stretch brush: add a picture to stretch (Upload in the Brush menu)".into();
                return;
            };
            self.pen = smooth(&pts);
            self.finish_path(false);
            if let Some(i) = self.single() {
                self.shapes[i].name = format!("Stretch {}", self.shapes.len());
                self.shapes[i].fills.clear();
                self.shapes[i].strokes.clear();
                self.shapes[i].stroke_w = 0.0;
                self.shapes[i].stretch = Some(StretchBrush { image, width: self.brush_w.max(4.0) * 2.0 });
            }
        } else if self.brush != Brush::Pencil {
            // a filled outline; simplified after it is built so the width changes survive
            let widths = brush_widths(self.brush, &raw, &force, self.nib);
            let outline = rdp(&brush_outline(&raw, &widths, self.brush_w), 0.5 / self.zoom);
            self.pen = smooth(&outline);
            self.finish_path(true);
            if let Some(i) = self.single() {
                self.shapes[i].name = format!("{} {}", self.brush.label(), self.shapes.len());
            }
        } else {
            self.pen = smooth(&pts);
            self.finish_path(false);
            if let Some(i) = self.single() {
                self.shapes[i].name = format!("Pencil {}", self.shapes.len());
                self.shapes[i].stroke_w = self.brush_w;
            }
        }
        self.tool = Tool::Pencil;
    }
}
