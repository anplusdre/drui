// Paste SVG: SVG text on the clipboard becomes a frame with one path layer per drawn path (text arrives as outlines, as usvg
// flattens it). Fills and strokes keep their solid colours and gradients; clips, masks and filters of the source are not kept.
use egui::{Color32, Pos2, Vec2};
use resvg::usvg;

use crate::model::*;
use crate::App;

fn color(c: usvg::Color, opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.red, c.green, c.blue, (opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
}

// a usvg paint as a drui paint; None for patterns, which have no counterpart here
fn paint(p: &usvg::Paint, opacity: f32, bounds: egui::Rect) -> Option<Paint> {
    let stops = |g: &usvg::BaseGradient| -> Vec<GradStop> { g.stops().iter().map(|s| GradStop { pos: s.offset().get(), color: color(s.color(), s.opacity().get()) }).collect() };
    match p {
        usvg::Paint::Color(c) => Some(Paint { opacity, ..Paint::solid(color(*c, 1.0)) }),
        usvg::Paint::LinearGradient(g) => {
            let (dx, dy) = (g.x2() - g.x1(), g.y2() - g.y1());
            let mut grad = Gradient::between(Color32::BLACK, Color32::WHITE);
            grad.stops = stops(g);
            grad.angle = dy.atan2(dx).to_degrees();
            let _ = bounds;
            (grad.stops.len() >= 2).then(|| Paint { kind: PaintKind::Gradient(grad), opacity, ..Paint::solid(Color32::BLACK) })
        }
        usvg::Paint::RadialGradient(g) => {
            let mut grad = Gradient::between(Color32::BLACK, Color32::WHITE);
            grad.kind = GradientKind::Radial;
            grad.stops = stops(g);
            (grad.stops.len() >= 2).then(|| Paint { kind: PaintKind::Gradient(grad), opacity, ..Paint::solid(Color32::BLACK) })
        }
        usvg::Paint::Pattern(_) => None,
    }
}

// the contours of a path in the SVG's own coordinates (the path's transforms applied)
pub(crate) fn contours(path: &usvg::Path) -> Vec<Contour> {
    use usvg::tiny_skia_path::PathSegment;
    let ts = path.abs_transform();
    let map = |p: usvg::tiny_skia_path::Point| {
        let mut q = [p];
        ts.map_points(&mut q);
        Pos2::new(q[0].x, q[0].y)
    };
    let mut out: Vec<Contour> = vec![];
    let mut cur: Option<Contour> = None;
    let finish = |c: Option<Contour>, out: &mut Vec<Contour>| {
        if let Some(mut c) = c {
            // a path that comes back to its start closes on its first point
            if c.closed && c.pts.len() > 1 && (c.pts[0].p - c.pts[c.pts.len() - 1].p).length() < 1e-3 {
                let last = c.pts.pop().unwrap();
                c.pts[0].hin = last.hin;
            }
            if c.pts.len() >= 2 {
                out.push(c);
            }
        }
    };
    for seg in path.data().segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                finish(cur.take(), &mut out);
                cur = Some(Contour { pts: vec![PathPt::corner(map(p))], closed: false, hole: false });
            }
            PathSegment::LineTo(p) => {
                if let Some(c) = cur.as_mut() {
                    c.pts.push(PathPt::corner(map(p)));
                }
            }
            PathSegment::QuadTo(c1, p) => {
                if let Some(c) = cur.as_mut() {
                    // a quadratic curve as the cubic with the same shape
                    let (p0, q1, p1) = (c.pts.last().map_or(Pos2::ZERO, |l| l.p), map(c1), map(p));
                    let (a, b) = (p0 + (q1 - p0) * (2.0 / 3.0), p1 + (q1 - p1) * (2.0 / 3.0));
                    if let Some(l) = c.pts.last_mut() {
                        l.hout = a - p0;
                    }
                    c.pts.push(PathPt { p: p1, hin: b - p1, hout: Vec2::ZERO, radius: 0.0 });
                }
            }
            PathSegment::CubicTo(c1, c2, p) => {
                if let Some(c) = cur.as_mut() {
                    let (p0, a, b, p1) = (c.pts.last().map_or(Pos2::ZERO, |l| l.p), map(c1), map(c2), map(p));
                    if let Some(l) = c.pts.last_mut() {
                        l.hout = a - p0;
                    }
                    c.pts.push(PathPt { p: p1, hin: b - p1, hout: Vec2::ZERO, radius: 0.0 });
                }
            }
            PathSegment::Close => {
                if let Some(c) = cur.as_mut() {
                    c.closed = true;
                }
                finish(cur.take(), &mut out);
            }
        }
    }
    finish(cur.take(), &mut out);
    out
}

impl App {
    fn svg_paths(&mut self, group: &usvg::Group, frame: u64, out: &mut Vec<Shape>) {
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => self.svg_paths(g, frame, out),
                usvg::Node::Text(t) => self.svg_paths(t.flattened(), frame, out),
                usvg::Node::Path(p) if p.is_visible() => {
                    let cs = contours(p);
                    if cs.is_empty() {
                        continue;
                    }
                    let id = self.new_id();
                    let mut s = Shape::new(Kind::Path, id, self.shapes.len() + out.len() + 1, Pos2::ZERO, Some(frame));
                    s.name = format!("Vector {}", out.len() + 1);
                    s.fills.clear();
                    s.strokes.clear();
                    let bounds = egui::Rect::from_min_max(Pos2::new(p.bounding_box().left(), p.bounding_box().top()), Pos2::new(p.bounding_box().right(), p.bounding_box().bottom()));
                    if let Some(f) = p.fill() {
                        s.fills = paint(f.paint(), f.opacity().get(), bounds).into_iter().collect();
                    }
                    if let Some(st) = p.stroke() {
                        s.strokes = paint(st.paint(), st.opacity().get(), bounds).into_iter().collect();
                        s.stroke_w = st.width().get();
                        s.stroke_cap = [
                            match st.linecap() {
                                usvg::LineCap::Round => StrokeCap::Round,
                                usvg::LineCap::Square => StrokeCap::Square,
                                usvg::LineCap::Butt => StrokeCap::None,
                            };
                            2
                        ];
                        s.stroke_join = match st.linejoin() {
                            usvg::LineJoin::Round => StrokeJoin::Round,
                            usvg::LineJoin::Bevel => StrokeJoin::Bevel,
                            _ => StrokeJoin::Miter,
                        };
                        if let Some(d) = st.dasharray() {
                            s.dash_pattern = d.to_vec();
                        }
                    }
                    if s.fills.is_empty() && s.strokes.is_empty() {
                        continue;
                    }
                    s.set_path_world(&cs);
                    out.push(s);
                }
                _ => {}
            }
        }
    }

    // pastes SVG source as a frame of path layers in the middle of the view; false when it is not SVG
    pub fn import_svg(&mut self, text: &str) -> bool {
        let t = text.trim_start();
        if !(t.starts_with("<svg") || (t.starts_with("<?xml") && t.contains("<svg"))) {
            return false;
        }
        let opt = usvg::Options { fontdb: crate::export::fonts(), ..usvg::Options::default() };
        let Ok(tree) = usvg::Tree::from_str(text, &opt) else {
            self.status = "that SVG could not be read".into();
            return true;
        };
        let size = Vec2::new(tree.size().width(), tree.size().height());
        let at = self.to_world(self.view.min, self.view.center()) - size / 2.0;
        let parent = self.frame_at(at + size / 2.0, &[]);
        let fid = self.new_id();
        let mut frame = Shape::new(Kind::Frame, fid, self.shapes.len() + 1, at - self.origin_of(parent), parent);
        frame.name = "SVG".into();
        frame.size = size;
        frame.fills.clear();
        frame.clip = false;
        let mut paths = vec![];
        self.svg_paths(tree.root(), fid, &mut paths);
        if paths.is_empty() {
            self.status = "that SVG has nothing to draw".into();
            return true;
        }
        self.shapes.push(frame);
        self.shapes.extend(paths);
        self.sel = vec![fid];
        self.tool = crate::Tool::Select;
        true
    }
}
