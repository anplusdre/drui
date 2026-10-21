// Draw's region tools: Shape builder (Shift+B), Cut (X), Eraser (Shift+E) and Paint (fill a region).
// They work on the outlines of the selected layers (flattened, in their parent's space) with the boolean engine in `geom`.
// Split vector (a compound path into one path per outline) lives here too.
use egui::{Color32, Painter, Pos2, Stroke, Vec2};

use crate::geom::{self, BoolOp, Poly};
use crate::model::*;
use crate::App;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum VTool {
    Builder,
    Cut,
    Eraser,
    Paint,
}

impl VTool {
    pub fn label(self) -> &'static str {
        match self {
            VTool::Builder => "Shape builder",
            VTool::Cut => "Cut",
            VTool::Eraser => "Eraser",
            VTool::Paint => "Paint",
        }
    }
}

// the regions the selected layers cut each other into, and what the user did with them
pub struct Builder {
    pub faces: Vec<Vec<Contour>>,
    pub picked: Vec<usize>,  // merged into one shape when the tool is left
    pub deleted: Vec<usize>, // dropped
    pub sources: Vec<u64>,
    pub parent: Option<u64>,
}

fn to_polys(c: &[Contour]) -> Vec<Poly> {
    c.iter().map(|c| Poly { pts: c.pts.iter().map(|p| p.p).collect(), closed: true, hole: c.hole }).collect()
}

fn area(c: &[Contour]) -> f32 {
    c.iter()
        .map(|c| {
            let n = c.pts.len();
            let a: f32 = (0..n).map(|i| c.pts[i].p.x * c.pts[(i + 1) % n].p.y - c.pts[(i + 1) % n].p.x * c.pts[i].p.y).sum::<f32>().abs() / 2.0;
            if c.hole { -a } else { a }
        })
        .sum()
}

// the planar partition of `shapes`: every piece covered by the same set of shapes, as contours
pub fn arrangement(shapes: &[Vec<Poly>]) -> Vec<Vec<Contour>> {
    let mut faces: Vec<Vec<Contour>> = vec![];
    for s in shapes {
        let mut next: Vec<Vec<Contour>> = vec![];
        let mut rest: Vec<Poly> = s.clone();
        for f in &faces {
            let fp = to_polys(f);
            next.push(geom::bool_op(BoolOp::Intersect, &[fp.clone(), s.clone()]));
            next.push(geom::bool_op(BoolOp::Subtract, &[fp.clone(), s.clone()]));
            rest = to_polys(&geom::bool_op(BoolOp::Subtract, &[rest, fp]));
        }
        next.push(geom::bool_op(BoolOp::Union, &[rest]));
        faces = next.into_iter().filter(|f| area(f) > 0.5).collect();
    }
    faces
}

// a long thin quad on one side of the line a-b, big enough to cover any shape
fn half_plane(a: Pos2, b: Pos2, side: f32) -> Vec<Poly> {
    let d = (b - a).normalized();
    let n = Vec2::new(-d.y, d.x) * side;
    let big = 1.0e6;
    let (p, q) = (a - d * big, a + d * big);
    vec![Poly { pts: vec![p, q, q + n * big, p + n * big], closed: true, hole: false }]
}

impl App {
    pub fn set_vtool(&mut self, t: Option<VTool>) {
        self.finish_builder();
        if t.is_some() {
            self.set_tool(crate::Tool::Select);
        }
        self.vtool = t;
        self.vtool_pts.clear();
        if t == Some(VTool::Builder) || t == Some(VTool::Paint) {
            self.start_builder();
        }
        if let Some(t) = t {
            self.status = match t {
                VTool::Builder => "Shape builder: click or drag over regions to merge them, Alt to delete, Enter to finish".into(),
                VTool::Cut => "Cut: drag a line across the selected layers".into(),
                VTool::Eraser => "Eraser: drag over the selected layers (all layers under the pointer when nothing is selected)".into(),
                VTool::Paint => "Paint: click a region to fill it".into(),
            };
        }
    }

    // the shapes the tool works on: selected area layers that share one parent
    fn vtool_sources(&self) -> Vec<usize> {
        let ids = self.sel_roots();
        let mut v: Vec<usize> = ids.iter().filter_map(|&id| self.index_of(id)).filter(|&i| !self.shapes[i].kind.is_container() && self.shapes[i].kind != Kind::Text && self.shapes[i].has_area()).collect();
        if let Some(p) = v.first().map(|&i| self.shapes[i].parent) {
            v.retain(|&i| self.shapes[i].parent == p);
        }
        v
    }

    pub fn start_builder(&mut self) {
        let src = self.vtool_sources();
        if src.is_empty() || (self.vtool == Some(VTool::Builder) && src.len() < 2) {
            self.status = "select two or more overlapping layers first".into();
            self.vtool = None;
            return;
        }
        let polys: Vec<Vec<Poly>> = src.iter().map(|&i| self.shapes[i].polys(4.0)).collect();
        self.builder = Some(Builder { faces: arrangement(&polys), picked: vec![], deleted: vec![], sources: src.iter().map(|&i| self.shapes[i].id).collect(), parent: self.shapes[src[0]].parent });
    }

    fn face_at(&self, world: Pos2) -> Option<usize> {
        let b = self.builder.as_ref()?;
        let p = world - self.origin_of(b.parent);
        b.faces.iter().position(|f| geom::inside_even_odd(&to_polys(f), p))
    }

    // pointer over region(s): merge them, or with Alt delete them; Paint fills the one clicked
    pub fn builder_touch(&mut self, world: Pos2, alt: bool) {
        let Some(k) = self.face_at(world) else { return };
        if self.vtool == Some(VTool::Paint) {
            self.paint_face(k);
            return;
        }
        let Some(b) = self.builder.as_mut() else { return };
        let (add, remove) = if alt { (&mut b.deleted, &mut b.picked) } else { (&mut b.picked, &mut b.deleted) };
        remove.retain(|&x| x != k);
        if !add.contains(&k) {
            add.push(k);
        }
    }

    // Paint: the region becomes a path of its own with the fill colour, on top of its layers
    fn paint_face(&mut self, k: usize) {
        let Some(b) = self.builder.as_ref() else { return };
        let (contours, parent) = (b.faces[k].clone(), b.parent);
        let color = self.region_color();
        let nid = self.new_id();
        let mut o = Shape::new(Kind::Path, nid, self.shapes.len() + 1, Pos2::ZERO, parent);
        o.name = "Paint".into();
        o.fills = vec![Paint::solid(color)];
        o.strokes.clear();
        o.stroke_w = 0.0;
        o.set_path_world(&contours);
        self.shapes.push(o);
        self.sel = vec![nid];
    }

    fn region_color(&self) -> Color32 {
        self.sel.iter().filter_map(|&id| self.index_of(id)).find_map(|i| self.shapes[i].fills.iter().find_map(|f| if let PaintKind::Solid(c) = f.kind { Some(c) } else { None })).unwrap_or(crate::BRAND)
    }

    // leaves Shape builder: the merged regions become one path, the rest one path each, deleted regions vanish
    pub fn finish_builder(&mut self) {
        let Some(b) = self.builder.take() else { return };
        if self.vtool != Some(VTool::Builder) || (b.picked.is_empty() && b.deleted.is_empty()) {
            return;
        }
        let Some(first) = b.sources.first().and_then(|&id| self.index_of(id)) else { return };
        let (fills, strokes, stroke_w) = (self.shapes[first].fills.clone(), self.shapes[first].strokes.clone(), self.shapes[first].stroke_w);
        let mut groups: Vec<Vec<Contour>> = vec![];
        if !b.picked.is_empty() {
            let polys: Vec<Vec<Poly>> = b.picked.iter().map(|&k| to_polys(&b.faces[k])).collect();
            groups.push(geom::bool_op(BoolOp::Union, &polys));
        }
        for (k, f) in b.faces.iter().enumerate() {
            if !b.picked.contains(&k) && !b.deleted.contains(&k) {
                groups.push(f.clone());
            }
        }
        for id in &b.sources {
            self.remove_subtree(*id);
        }
        let mut made = vec![];
        for g in groups.into_iter().filter(|g| !g.is_empty()) {
            let nid = self.new_id();
            let mut o = Shape::new(Kind::Path, nid, self.shapes.len() + 1, Pos2::ZERO, b.parent);
            o.name = "Shape".into();
            o.fills = fills.clone();
            o.strokes = strokes.clone();
            o.stroke_w = stroke_w;
            o.set_path_world(&g);
            self.shapes.push(o);
            made.push(nid);
        }
        self.sel = made;
    }

    // Cut: the selected layers (or the layers the line crosses) split along the line a-b into two paths each
    pub fn cut_along(&mut self, a: Pos2, b: Pos2) {
        if (b - a).length() < 1.0 {
            return;
        }
        let mut targets = self.vtool_sources();
        if targets.is_empty() {
            targets = (0..self.shapes.len()).filter(|&i| !self.shapes[i].kind.is_container() && self.shapes[i].kind != Kind::Text && self.shapes[i].has_area() && self.shapes[i].visible && !self.shapes[i].locked).collect();
        }
        let mut made = vec![];
        for i in targets.into_iter().rev() {
            let o = self.origin_of(self.shapes[i].parent);
            let (la, lb) = ((a - o), (b - o));
            let polys = self.shapes[i].polys(4.0);
            let pieces: Vec<Vec<Contour>> = [1.0, -1.0].iter().map(|&side| geom::bool_op(BoolOp::Intersect, &[polys.clone(), half_plane(la, lb, side)])).filter(|p| area(p) > 0.5).collect();
            if pieces.len() < 2 {
                continue;
            }
            let src = self.shapes[i].clone();
            self.remove_subtree(src.id);
            for piece in pieces {
                let nid = self.new_id();
                let mut s = src.clone();
                s.id = nid;
                s.kind = Kind::Path;
                s.radius = [0.0; 4];
                s.set_path_world(&piece);
                self.shapes.push(s);
                made.push(nid);
            }
        }
        if made.is_empty() {
            self.status = "the line did not cross a layer".into();
        } else {
            self.sel = made;
        }
    }

    // Eraser: the stroke along `pts` (a brush as wide as the pencil width, at least 8 px on screen) is cut out of the layers it touches
    pub fn erase_along(&mut self, pts: &[Pos2]) {
        if pts.len() < 2 {
            return;
        }
        let width = (self.brush_w.max(8.0 / self.zoom)) * 2.0;
        let mut targets = self.vtool_sources();
        if targets.is_empty() {
            targets = (0..self.shapes.len()).filter(|&i| !self.shapes[i].kind.is_container() && self.shapes[i].kind != Kind::Text && self.shapes[i].has_area() && self.shapes[i].visible && !self.shapes[i].locked).collect();
        }
        for i in targets {
            let o = self.origin_of(self.shapes[i].parent);
            let line: Vec<Pos2> = pts.iter().map(|&p| (p - o)).collect();
            let brush = geom::outline_stroke(&[Poly { pts: line, closed: false, hole: false }], width, StrokeJoin::Round, 4.0, [StrokeCap::Round; 2], StrokeAlign::Center);
            if brush.is_empty() {
                continue;
            }
            let polys = self.shapes[i].polys(4.0);
            let out = geom::bool_op(BoolOp::Subtract, &[polys.clone(), to_polys(&brush)]);
            if (area(&out) - area(&polys.iter().map(|p| Contour { pts: p.pts.iter().map(|&q| PathPt::corner(q)).collect(), closed: true, hole: p.hole }).collect::<Vec<_>>())).abs() < 0.01 {
                continue; // untouched
            }
            let id = self.shapes[i].id;
            if out.is_empty() {
                self.remove_subtree(id);
            } else {
                let s = &mut self.shapes[i];
                s.set_path_world(&out);
                s.kind = Kind::Path;
                s.radius = [0.0; 4];
            }
        }
    }

    // Split vector: a path with several outlines becomes one path per outline (holes stay with their outline)
    pub fn split_vector(&mut self) {
        let mut made = vec![];
        for id in self.sel_roots() {
            let Some(i) = self.index_of(id).filter(|&i| self.shapes[i].kind == Kind::Path && self.shapes[i].path.len() > 1) else { continue };
            let src = self.shapes[i].clone();
            let contours = src.path_world();
            let mut parts: Vec<Vec<Contour>> = vec![];
            for c in contours {
                if c.hole && !parts.is_empty() {
                    parts.last_mut().unwrap().push(c);
                } else {
                    parts.push(vec![c]);
                }
            }
            if parts.len() < 2 {
                continue;
            }
            self.remove_subtree(id);
            for part in parts {
                let nid = self.new_id();
                let mut s = src.clone();
                s.id = nid;
                s.set_path_world(&part);
                self.shapes.push(s);
                made.push(nid);
            }
        }
        if !made.is_empty() {
            self.sel = made;
        }
    }

    // the tool's overlay on the canvas
    pub fn draw_vtool(&self, painter: &Painter, origin: Pos2, hover: Option<Pos2>) {
        let line = Stroke::new(1.5, crate::BRAND);
        if let Some(b) = &self.builder {
            let o = self.origin_of(b.parent);
            let hot = hover.and_then(|h| self.face_at(self.to_world(origin, h)));
            for (k, f) in b.faces.iter().enumerate() {
                let (fill, stroke) = if b.deleted.contains(&k) {
                    (Color32::from_rgba_unmultiplied(240, 70, 70, 90), Stroke::new(1.0, Color32::from_rgb(240, 70, 70)))
                } else if b.picked.contains(&k) {
                    (crate::BRAND.gamma_multiply(0.45), line)
                } else if hot == Some(k) {
                    (crate::BRAND.gamma_multiply(0.25), line)
                } else {
                    (Color32::TRANSPARENT, Stroke::new(1.0, crate::BRAND.gamma_multiply(0.5)))
                };
                let polys = to_polys(f);
                if fill != Color32::TRANSPARENT {
                    let (tri, idx) = geom::triangulate(&polys);
                    let mut mesh = egui::Mesh::default();
                    mesh.vertices = tri.iter().map(|&q| egui::epaint::Vertex { pos: self.to_screen(origin, q + o), uv: egui::epaint::WHITE_UV, color: fill }).collect();
                    mesh.indices = idx;
                    painter.add(egui::Shape::mesh(mesh));
                }
                for p in &polys {
                    let pts: Vec<Pos2> = p.pts.iter().map(|&q| self.to_screen(origin, q + o)).collect();
                    painter.add(egui::Shape::closed_line(pts, stroke));
                }
            }
        }
        if self.vtool_pts.len() > 1 {
            let pts: Vec<Pos2> = self.vtool_pts.iter().map(|&p| self.to_screen(origin, p)).collect();
            if self.vtool == Some(VTool::Cut) {
                painter.add(egui::Shape::dashed_line(&[pts[0], *pts.last().unwrap()], line, 6.0, 4.0));
            } else {
                painter.add(egui::Shape::line(pts, Stroke::new((self.brush_w.max(8.0 / self.zoom) * 2.0 * self.zoom).max(2.0), Color32::from_rgba_unmultiplied(240, 70, 70, 70))));
            }
        }
    }
}

// ---- text to vector paths: the export engine lays the text out and its glyph outlines become the path

fn collect_glyphs(g: &resvg::usvg::Group, out: &mut Vec<Contour>) {
    use resvg::tiny_skia::PathSegment;
    use resvg::usvg::Node;
    for n in g.children() {
        match n {
            Node::Group(g) => collect_glyphs(g, out),
            Node::Text(t) => collect_glyphs(t.flattened(), out),
            Node::Path(p) => {
                if p.fill().is_none() {
                    continue;
                }
                let ts = p.abs_transform();
                let map = |q: resvg::tiny_skia::Point| {
                    let mut q = q;
                    ts.map_point(&mut q);
                    Pos2::new(q.x, q.y)
                };
                let mut cur: Option<Contour> = None;
                let mut last = Pos2::ZERO;
                let mut push = |c: Option<Contour>, out: &mut Vec<Contour>| {
                    if let Some(c) = c.filter(|c| c.pts.len() > 1) {
                        out.push(c);
                    }
                };
                for seg in p.data().segments() {
                    match seg {
                        PathSegment::MoveTo(a) => {
                            push(cur.take(), out);
                            last = map(a);
                            cur = Some(Contour { pts: vec![PathPt::corner(last)], closed: true, hole: false });
                        }
                        PathSegment::LineTo(a) => {
                            last = map(a);
                            if let Some(c) = cur.as_mut() {
                                c.pts.push(PathPt::corner(last));
                            }
                        }
                        PathSegment::QuadTo(c1, a) => {
                            let (c1, a) = (map(c1), map(a));
                            // a quadratic as a cubic: the control points sit two thirds of the way to the control
                            let (h1, h2) = ((c1 - last) * (2.0 / 3.0), (c1 - a) * (2.0 / 3.0));
                            if let Some(c) = cur.as_mut() {
                                if let Some(l) = c.pts.last_mut() {
                                    l.hout = h1;
                                }
                                c.pts.push(PathPt { p: a, hin: h2, hout: Vec2::ZERO, radius: 0.0 });
                            }
                            last = a;
                        }
                        PathSegment::CubicTo(c1, c2, a) => {
                            let (c1, c2, a) = (map(c1), map(c2), map(a));
                            if let Some(c) = cur.as_mut() {
                                if let Some(l) = c.pts.last_mut() {
                                    l.hout = c1 - last;
                                }
                                c.pts.push(PathPt { p: a, hin: c2 - a, hout: Vec2::ZERO, radius: 0.0 });
                            }
                            last = a;
                        }
                        PathSegment::Close => {
                            // the closing point repeats the first: merge it into it
                            if let Some(c) = cur.as_mut() {
                                if c.pts.len() > 2 && (c.pts[0].p - last).length() < 0.01 {
                                    let end = c.pts.pop().unwrap();
                                    c.pts[0].hin = end.hin;
                                }
                            }
                            push(cur.take(), out);
                        }
                    }
                }
                push(cur.take(), out);
            }
            _ => {}
        }
    }
}

impl App {
    // Outline text: each selected text layer becomes a vector path of its glyphs, filled like the text was
    pub fn text_to_vector(&mut self) {
        let ctx = self.ctx.clone().unwrap_or_default();
        let mut made = vec![];
        for id in self.sel_roots() {
            let Some(i) = self.index_of(id).filter(|&i| self.shapes[i].kind == Kind::Text) else { continue };
            let Some((svg, _)) = self.svg_for(&ctx, &[id]) else { continue };
            let opt = resvg::usvg::Options { fontdb: crate::export::fonts(), ..Default::default() };
            let Ok(tree) = resvg::usvg::Tree::from_str(&svg, &opt) else { continue };
            let mut contours = vec![];
            collect_glyphs(tree.root(), &mut contours);
            if contours.is_empty() {
                continue;
            }
            // the svg is in world space; the path wants the parent's
            let o = self.origin_of(self.shapes[i].parent);
            for c in &mut contours {
                for q in &mut c.pts {
                    q.p -= o;
                }
            }
            let s = &mut self.shapes[i];
            s.set_path_world(&contours);
            s.kind = Kind::Path;
            s.text.clear();
            s.runs.clear();
            made.push(id);
        }
        if made.is_empty() {
            self.status = "select a text layer to outline".into();
        }
    }
}
