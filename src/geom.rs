// Geometry of shapes: outlines, rotation, hit testing, triangulation and boolean operations.
use std::f32::consts::TAU;

use egui::{Pos2, Rect, Vec2};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};

use crate::model::{Contour, Kind, PathPt, Shape, StrokeAlign, StrokeCap, StrokeJoin};

// a flattened outline in the shape's parent space
#[derive(Clone, Debug)]
pub struct Poly {
    pub pts: Vec<Pos2>,
    pub closed: bool,
    pub hole: bool,
}

// the share of the stroke weight at `t` (0 start .. 1 end) along an open path, for a width profile (Shape::width_profile)
pub fn profile_width(profile: u8, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match profile {
        1 => 1.0 - t,
        2 => t,
        3 => (std::f32::consts::PI * t).sin(),
        4 => 1.0 - 0.8 * (std::f32::consts::PI * t).sin(),
        _ => 1.0,
    }
}

// the share of the weight at `t` along a line for a custom profile: straight lines between the points, held flat past the ends
pub fn points_width(points: &[(f32, f32)], t: f32) -> f32 {
    let mut p: Vec<(f32, f32)> = points.to_vec();
    p.sort_by(|a, b| a.0.total_cmp(&b.0));
    match p.as_slice() {
        [] => 1.0,
        [only] => only.1,
        _ => {
            if t <= p[0].0 {
                return p[0].1;
            }
            for w in p.windows(2) {
                if t <= w[1].0 {
                    let span = (w[1].0 - w[0].0).max(1e-6);
                    return w[0].1 + (w[1].1 - w[0].1) * (t - w[0].0) / span;
                }
            }
            p[p.len() - 1].1
        }
    }
}

// the outline of an open polyline stroked with a width that follows `profile`: the line is resampled along its length, each
// sample is pushed out to both sides by half the width there (corners are mitred, at most twice the half width), and the
// result is the left side followed by the right side backwards. None for a line too short to draw.
pub fn variable_width_outline(pts: &[Pos2], width: f32, profile: &dyn Fn(f32) -> f32) -> Option<Vec<Pos2>> {
    let mut cum = vec![0.0f32];
    for w in pts.windows(2) {
        cum.push(cum.last().copied().unwrap_or(0.0) + (w[1] - w[0]).length());
    }
    let total = *cum.last()?;
    if pts.len() < 2 || total < 0.01 {
        return None;
    }
    let n = ((total / 2.0).ceil() as usize).clamp(8, 400);
    // the point and the direction of travel at arc length `at`
    let at = |len: f32| -> (Pos2, Vec2) {
        let i = cum.partition_point(|&c| c <= len).clamp(1, pts.len() - 1);
        let (a, b) = (pts[i - 1], pts[i]);
        let seg = (cum[i] - cum[i - 1]).max(1e-6);
        let dir = (b - a) / seg;
        (a + dir * (len - cum[i - 1]).clamp(0.0, seg), dir)
    };
    let samples: Vec<(Pos2, Vec2)> = (0..=n).map(|k| at(total * k as f32 / n as f32)).collect();
    let (mut left, mut right) = (Vec::with_capacity(n + 1), Vec::with_capacity(n + 1));
    for (k, &(p, dir)) in samples.iter().enumerate() {
        // the direction there is the average of the way in and the way out
        let prev = if k > 0 { samples[k - 1].1 } else { dir };
        let next = if k < n { samples[k + 1].1 } else { dir };
        let mut t = prev + next;
        if t.length() < 1e-4 {
            t = dir;
        }
        let t = t.normalized();
        let normal = Vec2::new(-t.y, t.x);
        // a corner makes the offset longer along the normal: 1 / cos(half the turn), capped
        let miter = (1.0 / t.dot(dir).abs().max(0.5)).min(2.0);
        let half = width * profile(k as f32 / n as f32) / 2.0 * miter;
        left.push(p + normal * half);
        right.push(p - normal * half);
    }
    left.extend(right.into_iter().rev());
    Some(left)
}

pub fn rot_vec(v: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

fn seg_count(r: f32, zoom: f32) -> usize {
    (((r * zoom).abs().sqrt() * 4.0) as usize).clamp(16, 180)
}

fn arc_pts(c: Pos2, rx: f32, ry: f32, a0: f32, a1: f32, n: usize, out: &mut Vec<Pos2>) {
    for k in 0..=n {
        let a = a0 + (a1 - a0) * k as f32 / n as f32;
        out.push(c + Vec2::new(rx * a.cos(), ry * a.sin()));
    }
}

// One corner with Figma's corner smoothing, as a polyline from the incoming edge to the outgoing one (clockwise). The curve is the
// continuous-curvature one iOS uses: a bezier that eases into a shorter circular arc and out again, after the construction in
// "Desperately seeking squircles" (the same numbers figma-squircle uses). `smooth` is 0..1; 0 is a plain round corner.
// `corner` is the corner point, `k` which one (0 top left, 1 top right, 2 bottom right, 3 bottom left), `budget` how far along an
// edge the corner may reach (half the shorter side).
fn smooth_corner(corner: Pos2, k: usize, radius: f32, smooth: f32, budget: f32, zoom: f32, out: &mut Vec<Pos2>) {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
    let mut xi = smooth.clamp(0.0, 1.0);
    let mut p = (1.0 + xi) * radius;
    // not enough room: the smoothing is reduced, as Figma does
    xi = xi.min(budget / radius - 1.0).max(0.0);
    p = p.min(budget);
    let arc_measure = FRAC_PI_2 * (1.0 - xi);
    let arc_len = (arc_measure / 2.0).sin() * radius * 2f32.sqrt();
    let alpha = (FRAC_PI_2 - arc_measure) / 2.0;
    let p3p4 = radius * (alpha / 2.0).tan();
    let beta = FRAC_PI_4 * xi;
    let c = p3p4 * beta.cos();
    let d = c * beta.tan();
    let b = (p - arc_len - c - d) / 3.0;
    let a = 2.0 * b;
    // canonical top-right corner: offsets from the corner point, the edge runs in from the left, the corner turns down
    let s = Vec2::new(-p, 0.0);
    let bez = |p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, n: usize, out: &mut Vec<Vec2>| {
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            out.push(p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t));
        }
    };
    let n = seg_count(radius, zoom) / 6 + 3;
    let mut v: Vec<Vec2> = vec![s];
    let p3 = s + Vec2::new(a + b + c, d);
    bez(s, s + Vec2::new(a, 0.0), s + Vec2::new(a + b, 0.0), p3, n, &mut v);
    // the circular part between the two curves
    let e1 = p3 + Vec2::new(arc_len, arc_len);
    if xi < 0.999 {
        let mid = (p3 + e1) / 2.0;
        let h = (radius * radius - (arc_len * 2f32.sqrt() / 2.0).powi(2)).max(0.0).sqrt();
        let centre = mid + Vec2::new(-1.0, 1.0).normalized() * h;
        let (a0, a1) = ((p3 - centre).angle(), (e1 - centre).angle());
        let mut pts = vec![];
        arc_pts(centre.to_pos2(), radius, radius, a0, a1, n, &mut pts);
        v.extend(pts.into_iter().skip(1).map(|q| q.to_vec2()));
    } else {
        v.push(e1);
    }
    bez(e1, e1 + Vec2::new(d, c), e1 + Vec2::new(d, b + c), e1 + Vec2::new(d, a + b + c), n, &mut v);
    // turned to the corner it is for: a quarter turn per step clockwise from the top right one
    for q in v {
        let rot = match k {
            0 => Vec2::new(q.y, -q.x),
            1 => q,
            2 => Vec2::new(-q.y, q.x),
            _ => Vec2::new(-q.x, -q.y),
        };
        out.push(corner + rot);
    }
}

fn rounded_rect(r: Rect, rad: [f32; 4], zoom: f32) -> Vec<Pos2> {
    rounded_rect_smooth(r, rad, 0.0, zoom)
}

pub fn rounded_rect_smooth(r: Rect, rad: [f32; 4], smooth: f32, zoom: f32) -> Vec<Pos2> {
    let m = (r.width().min(r.height()) / 2.0).max(0.0);
    let rad = rad.map(|x| x.clamp(0.0, m));
    if rad.iter().all(|&x| x == 0.0) {
        return vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
    }
    if smooth > 0.0 {
        let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
        let mut out = vec![];
        for k in 0..4 {
            if rad[k] == 0.0 {
                out.push(corners[k]);
            } else {
                smooth_corner(corners[k], k, rad[k], smooth, m, zoom, &mut out);
            }
        }
        return out;
    }
    let centers = [
        Pos2::new(r.min.x + rad[0], r.min.y + rad[0]),
        Pos2::new(r.max.x - rad[1], r.min.y + rad[1]),
        Pos2::new(r.max.x - rad[2], r.max.y - rad[2]),
        Pos2::new(r.min.x + rad[3], r.max.y - rad[3]),
    ];
    let mut out = vec![];
    for k in 0..4 {
        let a0 = std::f32::consts::PI * (1.0 + k as f32 * 0.5);
        if rad[k] == 0.0 {
            out.push(centers[k]);
        } else {
            arc_pts(centers[k], rad[k], rad[k], a0, a0 + TAU / 4.0, seg_count(rad[k], zoom) / 4 + 2, &mut out);
        }
    }
    out
}

// `c` with every sharp vertex that has a radius replaced by an arc: two points on the neighbouring segments joined by a curve
pub fn round_contour(c: &Contour) -> Contour {
    if c.pts.iter().all(|q| q.radius <= 0.0) {
        return c.clone();
    }
    let n = c.pts.len();
    let mut out: Vec<PathPt> = vec![];
    for k in 0..n {
        let q = c.pts[k];
        let edge = !c.closed && (k == 0 || k + 1 == n);
        if q.radius <= 0.0 || edge || q.hin != Vec2::ZERO || q.hout != Vec2::ZERO {
            out.push(PathPt { radius: 0.0, ..q });
            continue;
        }
        let (a, b) = (c.pts[(k + n - 1) % n].p, c.pts[(k + 1) % n].p);
        let (to_a, to_b) = (a - q.p, b - q.p);
        let (la, lb) = (to_a.length(), to_b.length());
        if la < 1e-4 || lb < 1e-4 {
            out.push(PathPt { radius: 0.0, ..q });
            continue;
        }
        let (u, v) = (to_a / la, to_b / lb);
        let theta = u.dot(v).clamp(-1.0, 1.0).acos(); // the angle between the two segments
        if theta < 1e-3 || theta > std::f32::consts::PI - 1e-3 {
            out.push(PathPt { radius: 0.0, ..q });
            continue;
        }
        // the cut on each side, never more than half a segment so that neighbouring corners do not overlap
        let d = (q.radius / (theta / 2.0).tan()).min(la / 2.0).min(lb / 2.0);
        let r = d * (theta / 2.0).tan();
        let sweep = std::f32::consts::PI - theta;
        let h = 4.0 / 3.0 * (sweep / 4.0).tan() * r;
        out.push(PathPt { p: q.p + u * d, hin: Vec2::ZERO, hout: -u * h, radius: 0.0 });
        out.push(PathPt { p: q.p + v * d, hin: -v * h, hout: Vec2::ZERO, radius: 0.0 });
    }
    Contour { pts: out, ..c.clone() }
}

// flattens one contour whose points are absolute (any consistent space)
pub fn flatten(c: &Contour, zoom: f32) -> Vec<Pos2> {
    let n = c.pts.len();
    let mut out = vec![];
    let segs = if c.closed { n } else { n.saturating_sub(1) };
    for i in 0..segs {
        let (a, b) = (c.pts[i], c.pts[(i + 1) % n]);
        out.push(a.p);
        if a.hout != Vec2::ZERO || b.hin != Vec2::ZERO {
            let (c0, c1) = (a.p + a.hout, b.p + b.hin);
            let len = (c0 - a.p).length() + (c1 - c0).length() + (b.p - c1).length();
            let steps = (((len * zoom).abs().sqrt() * 1.5) as usize).clamp(6, 64);
            for k in 1..steps {
                let t = k as f32 / steps as f32;
                let u = 1.0 - t;
                let p = a.p.to_vec2() * (u * u * u) + c0.to_vec2() * (3.0 * u * u * t) + c1.to_vec2() * (3.0 * u * t * t) + b.p.to_vec2() * (t * t * t);
                out.push(p.to_pos2());
            }
        }
    }
    if !c.closed || n == 1 {
        if let Some(l) = c.pts.last() {
            out.push(l.p);
        }
    }
    out
}

impl Shape {
    pub fn rect(&self) -> Rect {
        Rect::from_two_pos(self.pos, self.pos + self.size)
    }

    // radians; groups (their box hugs the children) and lines have no rotation of their own. A frame rotates
    // its own box; its descendants are turned along with it when the rotation is set (`App::set_rotation`).
    pub fn rot(&self) -> f32 {
        if self.kind == Kind::Group || self.kind == Kind::Line { 0.0 } else { self.rotation.to_radians() }
    }

    pub fn center(&self) -> Pos2 {
        self.pos + self.size / 2.0
    }

    // unrotated parent-space point -> where this shape's rotation puts it
    pub fn xf(&self, p: Pos2) -> Pos2 {
        let c = self.center();
        c + rot_vec(p - c, self.rot())
    }

    pub fn unxf(&self, p: Pos2) -> Pos2 {
        let c = self.center();
        c + rot_vec(p - c, -self.rot())
    }

    // top-left, top-right, bottom-right, bottom-left (rotated)
    pub fn corners(&self) -> [Pos2; 4] {
        let r = self.rect();
        [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].map(|p| self.xf(p))
    }

    // axis-aligned bounds of the rotated shape, in the parent's space
    pub fn aabb(&self) -> Rect {
        if self.rot() == 0.0 {
            return self.rect();
        }
        let c = self.corners();
        c[1..].iter().fold(Rect::from_min_max(c[0], c[0]), |r, &p| r.union(Rect::from_min_max(p, p)))
    }

    pub fn set_corners(&mut self, a: Pos2, b: Pos2) {
        if self.kind == Kind::Line {
            self.pos = a;
            self.size = b - a;
        } else {
            let r = Rect::from_two_pos(a, b);
            self.pos = r.min;
            self.size = r.size();
        }
    }

    // path anchors in the parent's space, rotation not applied
    pub fn path_abs(&self) -> Vec<Contour> {
        self.path
            .iter()
            .map(|c| Contour {
                pts: c
                    .pts
                    .iter()
                    .map(|q| PathPt {
                        p: self.pos + q.p.to_vec2() * self.size,
                        hin: q.hin * self.size,
                        hout: q.hout * self.size,
                        radius: q.radius,
                    })
                    .collect(),
                ..c.clone()
            })
            .collect()
    }

    // path anchors in the parent's space with the shape's rotation applied
    pub fn path_world(&self) -> Vec<Contour> {
        let a = self.rot();
        let mut v = self.path_abs();
        for c in &mut v {
            for q in &mut c.pts {
                q.p = self.xf(q.p);
                q.hin = rot_vec(q.hin, a);
                q.hout = rot_vec(q.hout, a);
            }
        }
        v
    }

    // replaces the path with contours given in the parent's space; the box is refitted, rotation resets
    pub fn set_path_world(&mut self, contours: &[Contour]) {
        let mut bb: Option<Rect> = None;
        for c in contours {
            for p in flatten(c, 4.0) {
                let r = Rect::from_min_max(p, p);
                bb = Some(bb.map_or(r, |b| b.union(r)));
            }
        }
        let bb = bb.unwrap_or(Rect::from_min_size(self.pos, Vec2::ZERO));
        let div = Vec2::new(if bb.width() > 1e-6 { bb.width() } else { 1.0 }, if bb.height() > 1e-6 { bb.height() } else { 1.0 });
        self.pos = bb.min;
        self.size = bb.size();
        self.rotation = 0.0;
        self.path = contours
            .iter()
            .map(|c| Contour {
                pts: c
                    .pts
                    .iter()
                    .map(|q| PathPt { p: ((q.p - bb.min) / div).to_pos2(), hin: q.hin / div, hout: q.hout / div, radius: q.radius })
                    .collect(),
                ..c.clone()
            })
            .collect();
    }

    // does the shape have an interior (as opposed to being just a stroke)?
    pub fn has_area(&self) -> bool {
        match self.kind {
            Kind::Line | Kind::Group => false,
            Kind::Path => self.path.iter().any(|c| c.closed || self.fills.iter().any(|f| f.visible)),
            _ => true,
        }
    }

    // outlines in the parent's space, rotation applied. `zoom` only picks how finely curves are cut.
    pub fn polys(&self, zoom: f32) -> Vec<Poly> {
        let r = self.rect();
        let closed = |pts: Vec<Pos2>| Poly { pts, closed: true, hole: false };
        let local: Vec<Poly> = match self.kind {
            Kind::Group => vec![],
            Kind::Line => vec![Poly { pts: vec![self.pos, self.pos + self.size], closed: false, hole: false }],
            Kind::Frame | Kind::Rect => vec![closed(rounded_rect_smooth(r, self.radius, self.corner_smooth, zoom))],
            Kind::Text => vec![closed(rounded_rect(r, [0.0; 4], zoom))],
            Kind::Ellipse => {
                let (c, rx, ry) = (r.center(), r.width() / 2.0, r.height() / 2.0);
                let n = seg_count(rx.max(ry), zoom);
                let (a0, sweep) = (self.arc[0].to_radians(), self.arc[1].clamp(-360.0, 360.0).to_radians());
                let ring = self.inner.clamp(0.0, 0.99);
                if sweep.abs() >= TAU - 1e-3 {
                    let mut o = vec![];
                    arc_pts(c, rx, ry, a0, a0 + TAU, n, &mut o);
                    o.pop();
                    let mut v = vec![closed(o)];
                    if ring > 0.0 {
                        let mut i = vec![];
                        arc_pts(c, rx * ring, ry * ring, a0, a0 + TAU, n, &mut i);
                        i.pop();
                        v.push(Poly { pts: i, closed: true, hole: true });
                    }
                    v
                } else {
                    let m = ((n as f32 * sweep.abs() / TAU) as usize).max(2);
                    let mut o = vec![];
                    arc_pts(c, rx, ry, a0, a0 + sweep, m, &mut o);
                    if ring > 0.0 {
                        let mut i = vec![];
                        arc_pts(c, rx * ring, ry * ring, a0, a0 + sweep, m, &mut i);
                        i.reverse();
                        o.extend(i);
                    } else {
                        o.push(c);
                    }
                    vec![closed(o)]
                }
            }
            Kind::Polygon | Kind::Star => {
                let (c, rx, ry) = (r.center(), r.width() / 2.0, r.height() / 2.0);
                let n = self.sides.clamp(3, 64) as usize;
                let star = self.kind == Kind::Star;
                let total = if star { n * 2 } else { n };
                let pts = (0..total)
                    .map(|k| {
                        let a = -TAU / 4.0 + TAU * k as f32 / total as f32;
                        let f = if star && k % 2 == 1 { self.inner.clamp(0.05, 1.0) } else { 1.0 };
                        c + Vec2::new(rx * f * a.cos(), ry * f * a.sin())
                    })
                    .collect();
                vec![closed(pts)]
            }
            Kind::Path => self
                .path_abs()
                .iter()
                .map(|c| Poly { pts: flatten(&round_contour(c), zoom), closed: c.closed, hole: c.hole })
                .collect(),
        };
        if self.rot() == 0.0 {
            return local;
        }
        local.into_iter().map(|mut p| {
            p.pts.iter_mut().for_each(|q| *q = self.xf(*q));
            p
        }).collect()
    }

    // `p` is in this shape's parent space
    pub fn hit(&self, p: Pos2, tol: f32) -> bool {
        match self.kind {
            Kind::Group => false,
            Kind::Frame | Kind::Rect | Kind::Text => self.rect().expand(tol).contains(self.unxf(p)),
            _ => {
                let polys = self.polys(1.0);
                let reach = tol + if self.strokes.iter().any(|s| s.visible) { self.stroke_w / 2.0 } else { 0.0 };
                let filled = self.has_area() && self.fills.iter().any(|f| f.visible) || self.kind != Kind::Path && self.kind != Kind::Line;
                if filled && inside_even_odd(&polys, p) {
                    return true;
                }
                polys.iter().any(|poly| near_edges(&poly.pts, poly.closed, p, reach))
            }
        }
    }
}

pub fn inside_even_odd(polys: &[Poly], p: Pos2) -> bool {
    let mut inside = false;
    for poly in polys.iter().filter(|q| q.closed) {
        let n = poly.pts.len();
        for i in 0..n {
            let (a, b) = (poly.pts[i], poly.pts[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                inside = !inside;
            }
        }
    }
    inside
}

fn near_edges(pts: &[Pos2], closed: bool, p: Pos2, reach: f32) -> bool {
    let n = pts.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    (0..segs).any(|i| {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let ab = b - a;
        let t = if ab.length_sq() == 0.0 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
        (p - (a + ab * t)).length() <= reach
    })
}

pub fn is_convex(pts: &[Pos2]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    for i in 0..n {
        let (a, b, c) = (pts[i], pts[(i + 1) % n], pts[(i + 2) % n]);
        let cr = (b - a).x * (c - b).y - (b - a).y * (c - b).x;
        if cr.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cr.signum();
        } else if cr.signum() != sign {
            return false;
        }
    }
    true
}

// triangles for the closed polys (a hole belongs to the poly before it); returns vertices + indices
pub fn triangulate(polys: &[Poly]) -> (Vec<Pos2>, Vec<u32>) {
    let (mut verts, mut idx) = (vec![], vec![]);
    let mut i = 0;
    while i < polys.len() {
        if !polys[i].closed || polys[i].hole {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < polys.len() && polys[j].hole {
            j += 1;
        }
        let (mut data, mut holes, mut pts) = (vec![], vec![], vec![]);
        for (k, poly) in polys[i..j].iter().enumerate() {
            if k > 0 {
                holes.push(pts.len());
            }
            for p in &poly.pts {
                data.extend([p.x as f64, p.y as f64]);
                pts.push(*p);
            }
        }
        if let Ok(tris) = earcutr::earcut(&data, &holes, 2) {
            let base = verts.len() as u32;
            idx.extend(tris.into_iter().map(|t| base + t as u32));
            verts.extend(pts);
        }
        i = j;
    }
    (verts, idx)
}

// ---- boolean operations

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

fn to_contours(polys: &[Poly]) -> Vec<Vec<[f64; 2]>> {
    polys.iter().filter(|p| p.closed && p.pts.len() >= 3).map(|p| p.pts.iter().map(|q| [q.x as f64, q.y as f64]).collect()).collect()
}

// operands bottom to top; Subtract removes every later operand from the first
// The path of a dynamic stroke: points every `100 / frequency` px along each outline, each moved across the path by up to `wiggle`
// (the same stroke always wiggles the same way), then rounded off with as many corner-cutting passes as `smoothen` asks for.
pub fn dynamic_polys(polys: &[Poly], d: &crate::model::Dynamic) -> Vec<Poly> {
    let rand = |k: u32| {
        let mut x = k.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
        x ^= x >> 15;
        x = x.wrapping_mul(0x2C1B_3C6D);
        x ^= x >> 12;
        (x % 20_000) as f32 / 10_000.0 - 1.0
    };
    let step = (100.0 / d.frequency.clamp(0.5, 200.0)).max(1.0);
    let passes = (d.smoothen.clamp(0.0, 100.0) / 25.0).round() as usize;
    let mut seed = 0u32;
    polys
        .iter()
        .map(|p| {
            let mut pts: Vec<Pos2> = vec![];
            let n = p.pts.len();
            let segs = if p.closed { n } else { n.saturating_sub(1) };
            let mut carry = 0.0;
            for i in 0..segs {
                let (a, b) = (p.pts[i], p.pts[(i + 1) % n]);
                let len = (b - a).length();
                if len < 1e-4 {
                    continue;
                }
                let dir = (b - a) / len;
                let normal = Vec2::new(-dir.y, dir.x);
                let mut t = carry;
                while t < len {
                    seed += 1;
                    let ends = !p.closed && i == 0 && t == 0.0;
                    let off = if ends { 0.0 } else { rand(seed) * d.wiggle };
                    pts.push(a + dir * t + normal * off);
                    t += step;
                }
                carry = t - len;
            }
            if !p.closed {
                if let Some(&last) = p.pts.last() {
                    pts.push(last);
                }
            }
            for _ in 0..passes {
                let m = pts.len();
                if m < 3 {
                    break;
                }
                let mut next = vec![];
                let range = if p.closed { m } else { m - 1 };
                if !p.closed {
                    next.push(pts[0]);
                }
                for i in 0..range {
                    let (a, b) = (pts[i], pts[(i + 1) % m]);
                    next.push(a + (b - a) * 0.25);
                    next.push(a + (b - a) * 0.75);
                }
                if !p.closed {
                    next.push(pts[m - 1]);
                }
                pts = next;
            }
            Poly { pts, closed: p.closed, hole: p.hole }
        })
        .collect()
}

pub fn bool_op(op: BoolOp, operands: &[Vec<Poly>]) -> Vec<Contour> {
    let Some((first, rest)) = operands.split_first() else { return vec![] };
    let rule = match op {
        BoolOp::Union => OverlayRule::Union,
        BoolOp::Subtract => OverlayRule::Difference,
        BoolOp::Intersect => OverlayRule::Intersect,
        BoolOp::Exclude => OverlayRule::Xor,
    };
    let mut acc: Vec<Vec<[f64; 2]>> = to_contours(first);
    for o in rest {
        let clip = to_contours(o);
        let shapes: Vec<Vec<Vec<[f64; 2]>>> = acc.overlay(&clip, rule, FillRule::EvenOdd);
        acc = shapes.into_iter().flatten().collect();
    }
    // re-run once to get the outer/hole structure of the final result
    let shapes: Vec<Vec<Vec<[f64; 2]>>> = acc.overlay(&Vec::<Vec<[f64; 2]>>::new(), OverlayRule::Union, FillRule::EvenOdd);
    let mut out = vec![];
    for shape in shapes {
        for (k, c) in shape.into_iter().enumerate() {
            out.push(Contour { pts: c.into_iter().map(|q| PathPt::corner(Pos2::new(q[0] as f32, q[1] as f32))).collect(), closed: true, hole: k > 0 });
        }
    }
    out
}

// i_overlay's result (shapes of an outer contour followed by its holes) as contours
fn shapes_to_contours(shapes: Vec<Vec<Vec<[f64; 2]>>>) -> Vec<Contour> {
    let mut out = vec![];
    for shape in shapes {
        for (k, c) in shape.into_iter().enumerate() {
            out.push(Contour { pts: c.into_iter().map(|q| PathPt::corner(Pos2::new(q[0] as f32, q[1] as f32))).collect(), closed: true, hole: k > 0 });
        }
    }
    out
}

// Figma's "Outline stroke": the area a stroke of `width` paints along `polys`, as filled contours. Inside / outside strokes of
// closed outlines are the centered stroke of twice the width cut to the inside / outside of the outline.
pub fn outline_stroke(polys: &[Poly], width: f32, join: StrokeJoin, miter_angle: f32, caps: [StrokeCap; 2], align: StrokeAlign) -> Vec<Contour> {
    let cap = |c: StrokeCap| match c {
        StrokeCap::Round => LineCap::Round(0.1),
        StrokeCap::Square => LineCap::Square,
        _ => LineCap::Butt,
    };
    let lj = match join {
        StrokeJoin::Miter => LineJoin::Miter(miter_angle.to_radians().clamp(0.1, 3.0) as f64),
        StrokeJoin::Bevel => LineJoin::Bevel,
        StrokeJoin::Round => LineJoin::Round(0.2),
    };
    let closed_fill = polys.iter().any(|p| p.closed) && align != StrokeAlign::Center;
    let w = if closed_fill { width * 2.0 } else { width } as f64;
    let mut acc: Vec<Vec<[f64; 2]>> = vec![];
    for p in polys.iter().filter(|p| p.pts.len() >= 2) {
        let path: Vec<[f64; 2]> = p.pts.iter().map(|q| [q.x as f64, q.y as f64]).collect();
        let style = StrokeStyle::<[f64; 2]>::new(w).line_join(lj.clone()).start_cap(cap(caps[0])).end_cap(cap(caps[1]));
        let shapes = path.stroke(style, p.closed);
        let next: Vec<Vec<[f64; 2]>> = shapes.into_iter().flatten().collect();
        let merged: Vec<Vec<Vec<[f64; 2]>>> = acc.overlay(&next, OverlayRule::Union, FillRule::NonZero);
        acc = merged.into_iter().flatten().collect();
    }
    if closed_fill {
        let fill = to_contours(polys);
        let rule = if align == StrokeAlign::Inside { OverlayRule::Intersect } else { OverlayRule::Difference };
        let cut: Vec<Vec<Vec<[f64; 2]>>> = acc.overlay(&fill, rule, FillRule::EvenOdd);
        acc = cut.into_iter().flatten().collect();
    }
    let shapes: Vec<Vec<Vec<[f64; 2]>>> = acc.overlay(&Vec::<Vec<[f64; 2]>>::new(), OverlayRule::Union, FillRule::EvenOdd);
    shapes_to_contours(shapes)
}

// Figma Draw's "Offset path": the closed outlines grown by `d` (shrunk when negative), corners kept sharp up to a limit
pub fn offset_path(polys: &[Poly], d: f32) -> Vec<Contour> {
    let contours = to_contours(polys);
    if contours.is_empty() {
        return vec![];
    }
    let style = OutlineStyle::<f64>::new(d as f64).line_join(LineJoin::Miter(0.3));
    shapes_to_contours(contours.outline(&style))
}

// a point `dist` along a polyline with the direction (unit) there; past the end it stays on the last segment
pub fn along(pts: &[Pos2], dist: f32) -> Option<(Pos2, Vec2)> {
    let mut left = dist.max(0.0);
    let mut last = None;
    for w in pts.windows(2) {
        let (d, len) = (w[1] - w[0], (w[1] - w[0]).length());
        if len < 1e-4 {
            continue;
        }
        last = Some((w[0], d / len, len));
        if left <= len {
            return Some((w[0] + d / len * left, d / len));
        }
        left -= len;
    }
    last.map(|(a, dir, len)| (a + dir * len + dir * left, dir))
}
