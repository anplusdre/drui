// File > Import Figma file (.fig): the nodes of a .fig (figfile.rs / kiwi.rs) become drui layers.
//
// A .fig holds a flat list of `NodeChange`s, each with a guid and a `parentIndex` (parent guid + a position string that orders the
// siblings). Pages are the CANVAS nodes under the DOCUMENT. Carried over: frames, groups, rectangles, ellipses (arcs, rings), lines,
// stars, polygons, vectors (paths from the geometry blobs), boolean operations, text (one style per layer), solid / gradient /
// image fills, strokes (weight, alignment, caps, joins, dashes, individual sides), shadows and blurs, opacity, blend modes, masks,
// constraints, auto layout, and components with their instances (an instance on the same page stays linked to its component,
// with its overrides applied; one whose component is on another page is a copy).
// Not carried over (counted in the summary): prototype interactions, variables and modes, mixed text styles inside one layer,
// pattern / video fills, flipped layers (the flip is dropped), plugin data, FigJam content.
use std::collections::{BTreeMap, HashMap, HashSet};

use base64::Engine;
use egui::{Color32, Pos2, Vec2};

use crate::figfile::{self, FigFile};
use crate::kiwi::Val;
use crate::model::*;
use crate::App;

type Guid = (i64, i64);

fn guid_of(v: &Val) -> Option<Guid> {
    Some((v.i("sessionID")?, v.i("localID")?))
}

// a guid, or something that wraps one ({guid: {...}}): variable references are written both ways
fn guid_any(v: &Val) -> Option<Guid> {
    guid_of(v).or_else(|| v.get("guid").and_then(guid_of))
}

// the first of several names a field goes by (the schema has changed names over the years)
fn first_of<'v>(v: &'v Val, names: &[&str]) -> Option<&'v Val> {
    names.iter().find_map(|k| v.get(k))
}

fn first_guid(v: &Val, names: &[&str]) -> Option<Guid> {
    first_of(v, names).and_then(guid_any)
}

fn color(v: &Val, opacity: f64) -> Color32 {
    let c = |k: &str| (v.f(k).unwrap_or(0.0).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(c("r"), c("g"), c("b"), (v.f("a").unwrap_or(1.0) * opacity).clamp(0.0, 1.0).mul_add(255.0, 0.0).round() as u8)
}

#[derive(Default)]
pub struct Summary {
    pub pages: usize,
    pub layers: usize,
    pub instances: usize,
    pub copies: usize, // instances whose component is elsewhere, imported as plain copies
    pub images: usize,
    pub interactions: usize,
    pub variables: usize,
    pub patterns: usize,
    pub videos: usize,
    pub fonts_missing: Vec<String>, // font families the file uses that are not installed here
    pub skipped: BTreeMap<String, usize>,
}

impl Summary {
    pub fn text(&self) -> String {
        let mut s = format!("imported {} page{}, {} layers", self.pages, if self.pages == 1 { "" } else { "s" }, self.layers);
        if self.instances > 0 {
            s += &format!(", {} instances", self.instances);
        }
        if self.copies > 0 {
            s += &format!(" ({} as copies)", self.copies);
        }
        if self.interactions > 0 {
            s += &format!(", {} prototype interactions", self.interactions);
        }
        if self.variables > 0 {
            s += &format!(", {} variables", self.variables);
        }
        if self.patterns + self.videos > 0 {
            s += &format!(", {} pattern and {} video fills", self.patterns, self.videos);
        }
        if !self.fonts_missing.is_empty() {
            s += &format!("; fonts not installed (install them and reopen the font list): {}", self.fonts_missing.join(", "));
        }
        if !self.skipped.is_empty() {
            s += &format!("; left out: {}", self.skipped.iter().map(|(k, v)| format!("{v} {}", k.to_lowercase().replace('_', " "))).collect::<Vec<_>>().join(", "));
        }
        s
    }
}

pub(crate) struct Importer<'a> {
    file: &'a FigFile,
    blobs: &'a [Val],
    nodes: HashMap<Guid, &'a Val>,
    kids: HashMap<Guid, Vec<Guid>>,
    parent_of: HashMap<Guid, Guid>,
    built: HashMap<Guid, u64>, // figma node -> drui layer, on the page being imported
    page_nodes: HashSet<Guid>, // the nodes of that page
    busy: HashSet<Guid>,
    image_ids: HashMap<String, u64>,
    images: Vec<Asset>,
    videos: Vec<Asset>,
    summary: Summary,
    count: usize,
    pending_ix: Vec<(u64, &'a Val)>, // layers whose node has prototype interactions, resolved once the page is built
    var_ids: HashMap<Guid, u64>,     // figma variable -> drui variable
    fonts: std::collections::BTreeSet<String>,
    flipped: bool, // the last transform `apply` read mirrors
    cur_shape: u64, // the layer `apply` is filling in (paints that need a layer made later are noted against it)
    pending_patterns: Vec<(u64, PatternSpec)>,
    var_names: HashMap<Guid, String>, // figma variable -> the name an expression uses for it ("Collection / name")
}

// a pattern fill's source layer is a node that may not be built yet: the rest is kept until it is
struct PatternSpec {
    source: Guid,
    scale: f32,
    spacing: Vec2,
    rotation: f32,
}

fn norm(v: Vec2, size: Vec2) -> Vec2 {
    Vec2::new(v.x / size.x.max(1e-3), v.y / size.y.max(1e-3))
}

impl<'a> Importer<'a> {
    fn new(file: &'a FigFile) -> Self {
        let blobs = match file.message.get("blobs") {
            Some(Val::Arr(a)) => a.as_slice(),
            _ => &[],
        };
        let mut nodes = HashMap::new();
        let mut kids: HashMap<Guid, Vec<(String, Guid)>> = HashMap::new();
        let mut parent_of = HashMap::new();
        for n in file.message.arr("nodeChanges") {
            let Some(g) = n.get("guid").and_then(guid_of) else { continue };
            nodes.insert(g, n);
            if let Some(pi) = n.get("parentIndex") {
                if let Some(p) = pi.get("guid").and_then(guid_of) {
                    kids.entry(p).or_default().push((pi.s("position").unwrap_or("").to_owned(), g));
                    parent_of.insert(g, p);
                }
            }
        }
        // siblings in z-order: the position strings are compared as plain text, lowest first = furthest back
        let kids = kids
            .into_iter()
            .map(|(p, mut v)| {
                v.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()).then(a.1.cmp(&b.1)));
                (p, v.into_iter().map(|(_, g)| g).collect())
            })
            .collect();
        Importer { file, blobs, nodes, kids, parent_of, built: HashMap::new(), page_nodes: HashSet::new(), busy: HashSet::new(), image_ids: HashMap::new(), images: vec![], videos: vec![], summary: Summary::default(), count: 0, pending_ix: vec![], var_ids: HashMap::new(), fonts: Default::default(), flipped: false, cur_shape: 0, pending_patterns: vec![], var_names: HashMap::new() }
    }

    fn kids_of(&self, g: Guid) -> Vec<Guid> {
        self.kids.get(&g).cloned().unwrap_or_default()
    }

    fn ty(&self, g: Guid) -> &str {
        self.nodes.get(&g).and_then(|n| n.s("type")).unwrap_or("")
    }

    fn collect_page(&mut self, canvas: Guid) {
        self.page_nodes.clear();
        let mut stack = vec![canvas];
        while let Some(g) = stack.pop() {
            self.page_nodes.insert(g);
            stack.extend(self.kids_of(g));
        }
    }

    // ---- paints

    fn image_asset(&mut self, app: &mut App, hash: &[u8]) -> Option<u64> {
        let key: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        if let Some(&id) = self.image_ids.get(&key) {
            return Some(id);
        }
        let bytes = self.file.images.get(&key)?;
        image::load_from_memory(bytes).ok()?; // only formats drui can draw
        let id = app.new_id();
        self.images.push(Asset { id, name: format!("image {}", self.images.len() + 1), data: base64::engine::general_purpose::STANDARD.encode(bytes) });
        self.image_ids.insert(key, id);
        self.summary.images += 1;
        Some(id)
    }

    fn paint(&mut self, app: &mut App, p: &Val, size: Vec2) -> Option<Paint> {
        let opacity = p.f("opacity").unwrap_or(1.0) as f32;
        let mut paint = Paint::solid(Color32::TRANSPARENT);
        paint.opacity = opacity;
        paint.visible = p.b("visible").unwrap_or(true);
        paint.blend = blend(p.s("blendMode"));
        match p.s("type")? {
            "SOLID" => {
                paint.kind = PaintKind::Solid(color(p.get("color")?, 1.0));
                // bound to a colour variable
                paint.var = p.get("colorVar").and_then(|v| v.get("value")).and_then(|v| v.get("alias")).and_then(guid_any).and_then(|g| self.var_ids.get(&g)).copied();
            }
            t @ ("GRADIENT_LINEAR" | "GRADIENT_RADIAL" | "GRADIENT_ANGULAR" | "GRADIENT_DIAMOND") => {
                let stops: Vec<GradStop> = p.arr("stops").iter().filter_map(|s| Some(GradStop { pos: s.f("position")? as f32, color: color(s.get("color")?, 1.0) })).collect();
                if stops.len() < 2 {
                    return None;
                }
                let kind = match t {
                    "GRADIENT_LINEAR" => GradientKind::Linear,
                    "GRADIENT_RADIAL" => GradientKind::Radial,
                    "GRADIENT_ANGULAR" => GradientKind::Angular,
                    _ => GradientKind::Diamond,
                };
                let mut g = Gradient { kind, stops, ..Gradient::default() };
                if let Some(m) = p.get("transform") {
                    gradient_from_transform(&mut g, m, size);
                }
                paint.kind = PaintKind::Gradient(g);
            }
            "IMAGE" => {
                let hash = p.get("image")?.bytes("hash").map(<[u8]>::to_vec).or_else(|| p.get("image").and_then(|i| i.arr("hash").iter().map(|b| if let Val::Int(i) = b { Some(*i as u8) } else { None }).collect()))?;
                let id = self.image_asset(app, &hash)?;
                paint.kind = PaintKind::Image(id);
                paint.image.mode = match p.s("imageScaleMode") {
                    Some("FIT") => ImageMode::Fit,
                    Some("TILE") => ImageMode::Tile,
                    _ => ImageMode::Fill,
                };
                if let (ImageMode::Tile, Some(sc)) = (paint.image.mode, p.f("scale")) {
                    paint.image.scale = sc as f32;
                }
            }
            "PATTERN" => {
                // the layer that is repeated, found later (it may not exist yet); how it is scaled, spaced and turned is read now
                let source = first_guid(p, &["patternSourceNodeID", "patternSourceNodeId", "sourceNodeID", "sourceNodeId", "patternSource"])?;
                let spacing = first_of(p, &["patternSpacing", "spacing"]).map_or(Vec2::ZERO, |v| Vec2::new(v.f("x").unwrap_or(0.0) as f32, v.f("y").unwrap_or(0.0) as f32));
                let scale = ["patternScalingFactor", "scalingFactor", "scale"].iter().find_map(|k| p.f(k)).unwrap_or(1.0) as f32;
                let rotation = p.f("rotation").unwrap_or(0.0) as f32;
                self.pending_patterns.push((self.cur_shape, PatternSpec { source, scale, spacing, rotation }));
                paint.kind = PaintKind::Pattern(PatternFill::default());
            }
            "VIDEO" => {
                let (poster, video) = self.video_paint(app, p)?;
                paint.kind = PaintKind::Image(poster);
                paint.video = Some(video);
                paint.image.mode = ImageMode::Fill;
                self.summary.videos += 1;
            }
            other => {
                *self.summary.skipped.entry(format!("{other} fills")).or_default() += 1;
                return None;
            }
        }
        Some(paint)
    }

    // a video fill: the video file (kept in the document) and a poster frame (the paint's own image when it has one drui can read,
    // otherwise the first frame, cut by ffmpeg when it is there)
    fn video_paint(&mut self, app: &mut App, p: &Val) -> Option<(u64, u64)> {
        let hash_of = |v: &Val| -> Option<String> {
            v.bytes("hash").map(|b| b.iter().map(|x| format!("{x:02x}")).collect()).or_else(|| v.s("hash").map(str::to_lowercase))
        };
        let video_hash = first_of(p, &["video", "videoData"]).and_then(hash_of).or_else(|| p.s("videoHash").map(str::to_lowercase))?;
        let bytes = self.file.images.get(&video_hash)?.clone();
        let poster_hash = p.get("image").and_then(hash_of).or_else(|| p.get("imageThumbnail").and_then(hash_of));
        let poster = poster_hash.and_then(|h| self.file.images.get(&h).cloned()).filter(|b| image::load_from_memory(b).is_ok()).or_else(|| crate::video::poster(&bytes, "mp4").ok().map(|(png, _)| png))?;
        let poster_id = app.new_id();
        self.images.push(Asset { id: poster_id, name: "video poster".into(), data: base64::engine::general_purpose::STANDARD.encode(&poster) });
        let vid = app.new_id();
        self.videos.push(Asset { id: vid, name: format!("video {}", self.videos.len() + 1), data: base64::engine::general_purpose::STANDARD.encode(&bytes) });
        Some((poster_id, vid))
    }

    fn paints(&mut self, app: &mut App, list: &[Val], size: Vec2) -> Vec<Paint> {
        list.iter().filter_map(|p| self.paint(app, p, size)).collect()
    }

    // ---- geometry

    // commands of a fill-geometry blob: 0 close, 1 move to, 2 line to, 3 quadratic, 4 cubic (little-endian f32 pairs)
    fn contours(&self, blob: usize, size: Vec2) -> Vec<Contour> {
        let Some(b) = self.blobs.get(blob).and_then(|b| b.bytes("bytes")) else { return vec![] };
        let f = |at: usize| b.get(at..at + 4).map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]));
        let pt = |at: usize| Some(Pos2::new(f(at)?, f(at + 4)?));
        let n = |p: Pos2| Pos2::new(p.x / size.x.max(1e-3), p.y / size.y.max(1e-3));
        let mut out: Vec<Contour> = vec![];
        let mut cur: Option<Contour> = None;
        let mut at = 0;
        let finish = |c: Option<Contour>, out: &mut Vec<Contour>| {
            if let Some(mut c) = c {
                // a closed path that ends where it began: the last point is the first one
                if c.closed && c.pts.len() > 2 && (c.pts[0].p - c.pts[c.pts.len() - 1].p).length() < 1e-4 {
                    let last = c.pts.pop().unwrap();
                    c.pts[0].hin = last.hin;
                }
                if c.pts.len() >= 2 {
                    out.push(c);
                }
            }
        };
        while at < b.len() {
            let cmd = b[at];
            at += 1;
            match cmd {
                0 => {
                    if let Some(c) = cur.as_mut() {
                        c.closed = true;
                    }
                    finish(cur.take(), &mut out);
                }
                1 => {
                    finish(cur.take(), &mut out);
                    let Some(p) = pt(at) else { break };
                    at += 8;
                    cur = Some(Contour { pts: vec![PathPt::corner(n(p))], closed: false, hole: false });
                }
                2 => {
                    let Some(p) = pt(at) else { break };
                    at += 8;
                    if let Some(c) = cur.as_mut() {
                        c.pts.push(PathPt::corner(n(p)));
                    }
                }
                3 | 4 => {
                    let k = if cmd == 3 { 2 } else { 3 };
                    let Some(ps): Option<Vec<Pos2>> = (0..k).map(|i| pt(at + 8 * i)).collect() else { break };
                    at += 8 * k;
                    if let Some(c) = cur.as_mut() {
                        let prev = c.pts.last().map_or(Pos2::ZERO, |q| q.p);
                        let (c1, c2, end) = if cmd == 4 {
                            (n(ps[0]), n(ps[1]), n(ps[2]))
                        } else {
                            // a quadratic as the cubic with the same curve
                            let (q, e) = (n(ps[0]), n(ps[1]));
                            (prev + (q - prev) * (2.0 / 3.0), e + (q - e) * (2.0 / 3.0), e)
                        };
                        if let Some(last) = c.pts.last_mut() {
                            last.hout = c1 - prev;
                        }
                        c.pts.push(PathPt { p: end, hin: c2 - end, hout: Vec2::ZERO, radius: 0.0 });
                    }
                }
                _ => break, // an unknown command: what came before is kept
            }
        }
        finish(cur.take(), &mut out);
        out
    }

    // a vector network blob: u32 vertices, u32 segments, u32 regions, then vertices (style, x, y) and segments (style, start,
    // tangent start x y, end, tangent end x y). The centre lines of open paths come from here (the fill geometry has none).
    fn network(&self, blob: usize, size: Vec2) -> Vec<Contour> {
        let Some(b) = self.blobs.get(blob).and_then(|b| b.bytes("bytes")) else { return vec![] };
        let u = |at: usize| b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize);
        let f = |at: usize| b.get(at..at + 4).map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]));
        let (Some(nv), Some(ns)) = (u(0), u(4)) else { return vec![] };
        if 12 + nv * 12 + ns * 28 > b.len() {
            return vec![];
        }
        let verts: Vec<Pos2> = (0..nv).map(|i| Pos2::new(f(12 + i * 12 + 4).unwrap_or(0.0), f(12 + i * 12 + 8).unwrap_or(0.0))).collect();
        // (start, end, tangent start, tangent end)
        let segs: Vec<(usize, usize, Vec2, Vec2)> = (0..ns)
            .map(|i| {
                let at = 12 + nv * 12 + i * 28;
                (u(at + 4).unwrap_or(0), u(at + 16).unwrap_or(0), Vec2::new(f(at + 8).unwrap_or(0.0), f(at + 12).unwrap_or(0.0)), Vec2::new(f(at + 20).unwrap_or(0.0), f(at + 24).unwrap_or(0.0)))
            })
            .filter(|s| s.0 < nv && s.1 < nv)
            .collect();
        // chain the segments into paths: follow each from where it ends, starting from vertices nothing else leads to
        let mut used = vec![false; segs.len()];
        let mut out = vec![];
        let mut order: Vec<usize> = (0..segs.len()).collect();
        let ends: HashSet<usize> = segs.iter().map(|s| s.1).collect();
        order.sort_by_key(|&i| ends.contains(&segs[i].0));
        for start in order {
            if used[start] {
                continue;
            }
            let mut pts = vec![PathPt::corner(Pos2::ZERO); 0];
            let mut seg = start;
            let first = segs[seg].0;
            let mut closed = false;
            loop {
                used[seg] = true;
                let (a, e, ta, te) = segs[seg];
                if pts.is_empty() {
                    pts.push(PathPt::corner(verts[a]));
                }
                if let Some(last) = pts.last_mut() {
                    last.hout = ta;
                }
                pts.push(PathPt { p: verts[e], hin: te, hout: Vec2::ZERO, radius: 0.0 });
                if e == first && pts.len() > 2 {
                    closed = true;
                    break;
                }
                match (0..segs.len()).find(|&k| !used[k] && segs[k].0 == e) {
                    Some(k) => seg = k,
                    None => break,
                }
            }
            if closed {
                let last = pts.pop().unwrap();
                pts[0].hin = last.hin;
            }
            let pts = pts.into_iter().map(|mut q| {
                q.p = Pos2::new(q.p.x / size.x.max(1e-3), q.p.y / size.y.max(1e-3));
                q.hin = norm(q.hin, size);
                q.hout = norm(q.hout, size);
                q
            });
            let pts: Vec<PathPt> = pts.collect();
            if pts.len() >= 2 {
                out.push(Contour { pts, closed, hole: false });
            }
        }
        out
    }

    // ---- one layer's properties

    // sets the fields `n` carries on `s`: a whole node, or an override of one (which names only what changed)
    fn apply(&mut self, app: &mut App, s: &mut Shape, n: &Val) {
        self.cur_shape = s.id;
        // (a file can carry non-numbers in derived data: those are ignored)
        if let Some(sz) = n.get("size") {
            let (x, y) = (sz.f("x").unwrap_or(0.0) as f32, sz.f("y").unwrap_or(0.0) as f32);
            if x.is_finite() && y.is_finite() {
                s.size = Vec2::new(x, y);
            }
        }
        if let Some(t) = n.get("transform") {
            if ["m00", "m01", "m02", "m10", "m11", "m12"].iter().all(|k| t.f(k).is_none_or(f64::is_finite)) {
                self.flipped = place(s, t);
            }
        }
        if let Some(name) = n.s("name") {
            s.name = name.to_owned();
        }
        if let Some(v) = n.b("visible") {
            s.visible = v;
        }
        if let Some(v) = n.b("locked") {
            s.locked = v;
        }
        if let Some(o) = n.f("opacity") {
            s.opacity = o as f32;
        }
        if n.get("blendMode").is_some() {
            s.blend = blend(n.s("blendMode"));
        }
        if let Some(m) = n.b("mask") {
            s.mask = m;
            s.mask_kind = match n.s("maskType") {
                Some("ALPHA") => 1,
                Some("LUMINANCE") => 2,
                _ => 0,
            };
        }
        if let Some(fm) = n.b("frameMaskDisabled") {
            if s.kind == Kind::Frame {
                s.clip = !fm;
            }
        }
        // corners
        if let Some(r) = n.f("cornerRadius") {
            s.radius = [r as f32; 4];
        }
        if n.b("rectangleCornerRadiiIndependent") == Some(true) || n.get("rectangleTopLeftCornerRadius").is_some() {
            let r = |k: &str, d: f32| n.f(k).map_or(d, |v| v as f32);
            let base = n.f("cornerRadius").unwrap_or(0.0) as f32;
            s.radius = [r("rectangleTopLeftCornerRadius", base), r("rectangleTopRightCornerRadius", base), r("rectangleBottomRightCornerRadius", base), r("rectangleBottomLeftCornerRadius", base)];
        }
        if let Some(k) = n.f("cornerSmoothing") {
            s.corner_smooth = (k as f32).clamp(0.0, 1.0);
        }
        // paints
        if n.get("fillPaints").is_some() {
            s.fills = self.paints(app, n.arr("fillPaints"), s.size);
        }
        if n.get("strokePaints").is_some() {
            s.strokes = self.paints(app, n.arr("strokePaints"), s.size);
        }
        if let Some(w) = n.f("strokeWeight") {
            s.stroke_w = w as f32;
        }
        if n.b("borderStrokeWeightsIndependent") == Some(true) {
            let w = |k: &str| n.f(k).unwrap_or(0.0) as f32;
            s.stroke_sides = Some([w("borderTopWeight"), w("borderRightWeight"), w("borderBottomWeight"), w("borderLeftWeight")]);
        }
        match n.s("strokeAlign") {
            Some("INSIDE") => s.stroke_align = StrokeAlign::Inside,
            Some("OUTSIDE") => s.stroke_align = StrokeAlign::Outside,
            Some("CENTER") => s.stroke_align = StrokeAlign::Center,
            _ => {}
        }
        if let Some(j) = n.s("strokeJoin") {
            s.stroke_join = match j {
                "BEVEL" => StrokeJoin::Bevel,
                "ROUND" => StrokeJoin::Round,
                _ => StrokeJoin::Miter,
            };
        }
        if let Some(c) = n.s("strokeCap") {
            let cap = match c {
                "ROUND" => StrokeCap::Round,
                "SQUARE" => StrokeCap::Square,
                "ARROW_LINES" => StrokeCap::ArrowLines,
                "ARROW_EQUILATERAL" => StrokeCap::TriangleArrow,
                _ => StrokeCap::None,
            };
            s.stroke_cap = [cap, cap];
        }
        if n.get("dashPattern").is_some() {
            s.dash_pattern = n.arr("dashPattern").iter().filter_map(|v| if let Val::Float(f) = v { Some(*f as f32) } else if let Val::Int(i) = v { Some(*i as f32) } else { None }).collect();
            s.dash = !s.dash_pattern.is_empty();
        }
        // effects
        if n.get("effects").is_some() {
            s.effects = n.arr("effects").iter().filter_map(effect).collect();
        }
        // constraints
        if let Some(c) = n.s("horizontalConstraint") {
            s.ch = constraint(c);
        }
        if let Some(c) = n.s("verticalConstraint") {
            s.cv = constraint(c);
        }
        // auto layout, on the frame itself and on its children
        if let Some(mode) = n.s("stackMode") {
            if mode == "NONE" {
                s.layout = None;
            } else if s.kind == Kind::Frame {
                let vertical = mode == "VERTICAL";
                let pad = |k: &str| n.f(k).unwrap_or(0.0) as f32;
                let align = |a: Option<&str>| match a {
                    Some("CENTER") => Align::Center,
                    Some("MAX") => Align::End,
                    Some("SPACE_BETWEEN") => Align::SpaceBetween,
                    _ => Align::Start,
                };
                let hug = |sizing: Option<&str>| sizing.is_some_and(|x| x.starts_with("RESIZE_TO_FIT"));
                let (primary_hug, counter_hug) = (hug(n.s("stackPrimarySizing")), hug(n.s("stackCounterSizing")));
                s.layout = Some(AutoLayout {
                    vertical,
                    gap: pad("stackSpacing"),
                    pad: [pad("stackHorizontalPadding"), pad("stackVerticalPadding"), pad("stackPaddingRight"), pad("stackPaddingBottom")],
                    main: align(n.s("stackPrimaryAlignItems")),
                    cross: match n.s("stackCounterAlignItems") {
                        Some("CENTER") => Align::Center,
                        Some("MAX") => Align::End,
                        _ => Align::Start,
                    },
                    hug_x: if vertical { counter_hug } else { primary_hug },
                    hug_y: if vertical { primary_hug } else { counter_hug },
                    wrap: n.s("stackWrap") == Some("WRAP"),
                    baseline: n.s("stackCounterAlignItems") == Some("BASELINE"),
                    ..AutoLayout::default()
                });
            }
        }
        if n.get("stackChildPrimaryGrow").is_some() || n.get("stackChildAlignSelf").is_some() {
            // the direction is the parent's; it is applied when the layer is placed (see `place_in_layout`)
            s.fill_x = n.f("stackChildPrimaryGrow").unwrap_or(0.0) > 0.0;
            s.fill_y = n.s("stackChildAlignSelf") == Some("STRETCH");
        }
        if n.s("stackPositioning") == Some("ABSOLUTE") {
            s.abs = true;
        }
        // text
        if let Some(td) = n.get("textData") {
            if let Some(c) = td.s("characters") {
                s.text = c.to_owned();
            }
        }
        if let Some(fs) = n.f("fontSize") {
            s.font_size = fs as f32;
        }
        if let Some(fnm) = n.get("fontName") {
            let family = fnm.s("family").unwrap_or("Inter");
            if family != "Inter" {
                s.family = family.to_owned();
                self.fonts.insert(family.to_owned());
            }
            let style = fnm.s("style").unwrap_or("Regular").to_lowercase();
            s.italic = style.contains("italic");
            s.weight = weight_of(&style);
            s.bold = false;
        }
        if let Some(a) = n.s("textAlignHorizontal") {
            s.align = match a {
                "CENTER" => TextAlign::Center,
                "RIGHT" => TextAlign::Right,
                _ => TextAlign::Left,
            };
        }
        if let Some(a) = n.s("textAlignVertical") {
            s.valign = match a {
                "CENTER" => 1,
                "BOTTOM" => 2,
                _ => 0,
            };
        }
        if let Some(lh) = n.get("lineHeight") {
            let v = lh.f("value").unwrap_or(0.0) as f32;
            s.line_h = match lh.s("units") {
                Some("PIXELS") if s.font_size > 0.0 => v / s.font_size,
                Some("PERCENT") => v / 100.0,
                Some("RAW") => v,
                _ => 0.0,
            };
        }
        if let Some(ls) = n.get("letterSpacing") {
            let v = ls.f("value").unwrap_or(0.0) as f32;
            s.tracking = if ls.s("units") == Some("PIXELS") && s.font_size > 0.0 { v / s.font_size * 100.0 } else { v };
        }
        if let Some(c) = n.s("textCase") {
            s.case = match c {
                "UPPER" => 1,
                "LOWER" => 2,
                "TITLE" => 3,
                "SMALL_CAPS" | "SMALL_CAPS_FORCED" => 4,
                _ => 0,
            };
        }
        if let Some(p) = n.f("paragraphSpacing") {
            s.para_spacing = p as f32;
        }
        if let Some(a) = n.s("textAutoResize") {
            s.fit = match a {
                "WIDTH_AND_HEIGHT" => TextFit::AutoWidth,
                "HEIGHT" => TextFit::AutoHeight,
                _ => TextFit::Fixed,
            };
        }
        // arcs and rings
        if let Some(arc) = n.get("arcData") {
            let (a0, a1) = (arc.f("startingAngle").unwrap_or(0.0).to_degrees() as f32, arc.f("endingAngle").unwrap_or(std::f64::consts::TAU).to_degrees() as f32);
            s.arc = [a0, a1 - a0];
            s.inner = arc.f("innerRadius").unwrap_or(0.0) as f32;
        }
        if let Some(c) = n.f("count") {
            if matches!(s.kind, Kind::Star | Kind::Polygon) {
                s.sides = c as u32;
            }
        }
        if let Some(r) = n.f("starInnerScale") {
            if s.kind == Kind::Star {
                s.inner = r as f32;
            }
        }
        // text with more than one style: each stretch with its own style id becomes a run
        if let Some(td) = n.get("textData") {
            if !td.arr("characterStyleIDs").is_empty() {
                s.runs = self.text_runs(s, td);
            }
        }
        // vector geometry: the outline of what is filled, or the centre line of an open path
        if s.kind == Kind::Path && (n.get("fillGeometry").is_some() || n.get("vectorData").is_some()) {
            let mut contours: Vec<Contour> = vec![];
            for g in n.arr("fillGeometry") {
                if let Some(b) = g.i("commandsBlob") {
                    contours.extend(self.contours(b as usize, s.size));
                }
            }
            let net = n.get("vectorData").and_then(|v| v.i("vectorNetworkBlob")).map(|b| self.network(b as usize, s.size)).unwrap_or_default();
            // a path that is only stroked has no fill outline: draw its centre line
            if contours.is_empty() || net.iter().any(|c| !c.closed) {
                if !net.is_empty() {
                    contours = net;
                }
            }
            if !contours.is_empty() {
                s.path = contours;
            }
        }
    }

    // the runs of a text layer: `characterStyleIDs` has one style id per character (0 = the layer's own style) and
    // `styleOverrideTable` the styles by id. Only what differs from the layer's own style is kept.
    fn text_runs(&mut self, s: &Shape, td: &Val) -> Vec<TextRun> {
        let ids: Vec<i64> = td.arr("characterStyleIDs").iter().map(|v| if let Val::Int(i) = v { *i } else { 0 }).collect();
        let chars: Vec<char> = s.text.chars().collect();
        // one id per UTF-16 unit when the text has characters outside the basic plane
        let utf16_to_char: Vec<usize> = if ids.len() == chars.len() { (0..chars.len()).collect() } else { chars.iter().enumerate().flat_map(|(k, c)| std::iter::repeat_n(k, c.len_utf16())).collect() };
        let table: HashMap<i64, &Val> = td.arr("styleOverrideTable").iter().filter_map(|e| Some((e.i("styleID")?, e))).collect();
        let at = |k: usize| -> i64 { ids.get(k).copied().unwrap_or(0) };
        let total = if ids.len() == chars.len() { chars.len() } else { utf16_to_char.len().min(ids.len()) };
        let mut runs: Vec<TextRun> = vec![];
        let mut k = 0;
        while k < total {
            let id = at(k);
            let mut e = k;
            while e < total && at(e) == id {
                e += 1;
            }
            if id != 0 {
                if let Some(st) = table.get(&id) {
                    let (start, end) = (utf16_to_char[k], utf16_to_char.get(e).copied().unwrap_or(chars.len()));
                    let mut r = TextRun { start, end, ..TextRun::default() };
                    if let Some(sz) = st.f("fontSize").map(|v| v as f32).filter(|v| (v - s.font_size).abs() > 0.01) {
                        r.size = Some(sz);
                    }
                    if let Some(f) = st.get("fontName") {
                        let style = f.s("style").unwrap_or("Regular").to_lowercase();
                        let (w, it) = (weight_of(&style), style.contains("italic"));
                        if w != s.base_weight() && !(w == 0 && s.base_weight() == 400) {
                            r.weight = Some(if w == 0 { 400 } else { w });
                        }
                        if it != s.italic {
                            r.italic = Some(it);
                        }
                        let fam = f.s("family").unwrap_or("Inter");
                        let own = if s.family.is_empty() { "Inter" } else { s.family.as_str() };
                        if fam != own {
                            r.family = Some(if fam == "Inter" { String::new() } else { fam.to_owned() });
                            if fam != "Inter" {
                                self.fonts.insert(fam.to_owned());
                            }
                        }
                    }
                    if let Some(c) = st.arr("fillPaints").iter().find(|p| p.s("type") == Some("SOLID") && p.b("visible") != Some(false)).and_then(|p| Some(color(p.get("color")?, p.f("opacity").unwrap_or(1.0)))) {
                        r.color = Some(c);
                    }
                    match st.s("textDecoration") {
                        Some("UNDERLINE") => r.underline = Some(true),
                        Some("STRIKETHROUGH") => r.strike = Some(true),
                        _ => {}
                    }
                    if let Some(u) = st.get("hyperlink").and_then(|h| h.s("url")).filter(|u| !u.is_empty()) {
                        r.url = Some(u.to_owned());
                    }
                    let plain = TextRun { start, end, ..TextRun::default() };
                    if r != plain {
                        // neighbours with the same style are one run
                        match runs.last_mut() {
                            Some(last) if last.end == r.start && TextRun { start: 0, end: 0, ..last.clone() } == TextRun { start: 0, end: 0, ..r.clone() } => last.end = r.end,
                            _ => runs.push(r),
                        }
                    }
                }
            }
            k = e;
        }
        runs
    }

    // ---- variables
    //
    // VARIABLE_SET nodes are collections (with their modes), VARIABLE nodes the variables with one value per mode. Built from how
    // Figma writes them; every lookup is optional, so a file that writes them differently simply imports without them.
    fn import_variables(&mut self, app: &mut App) -> Vec<Collection> {
        let mut sorted: Vec<(Guid, &Val)> = self.nodes.iter().map(|(g, n)| (*g, *n)).collect();
        sorted.sort_by_key(|(g, _)| *g);
        let mut out: Vec<Collection> = vec![];
        let mut set_at: HashMap<Guid, usize> = HashMap::new();
        let mut mode_ids: HashMap<Guid, Vec<Guid>> = HashMap::new(); // a set's modes, in order
        for (g, n) in sorted.iter().filter(|(_, n)| n.s("type") == Some("VARIABLE_SET")) {
            let mut modes: Vec<VarMode> = vec![];
            let mut mids: Vec<Guid> = vec![];
            for m in n.arr("variableSetModes") {
                if let Some(mg) = m.get("id").and_then(guid_any) {
                    mids.push(mg);
                    modes.push(VarMode { id: app.new_id(), name: m.s("name").unwrap_or("Mode").to_owned() });
                }
            }
            if modes.is_empty() {
                modes.push(VarMode { id: app.new_id(), name: "Mode 1".into() });
            }
            set_at.insert(*g, out.len());
            mode_ids.insert(*g, mids);
            out.push(Collection { id: app.new_id(), name: n.s("name").unwrap_or("Variables").to_owned(), modes, vars: vec![], extends: None, overrides: vec![] });
        }
        // every variable gets its id first, so that aliases can point at variables made later
        let vars: Vec<(Guid, &Val)> = sorted.iter().filter(|(_, n)| n.s("type") == Some("VARIABLE")).copied().collect();
        for (g, _) in &vars {
            let id = app.new_id();
            self.var_ids.insert(*g, id);
        }
        for (g, n) in vars {
            let Some(set) = n.get("variableSetID").and_then(guid_any) else { continue };
            let Some(&ci) = set_at.get(&set) else { continue };
            let kind = match n.s("variableResolvedType") {
                Some("BOOLEAN") => VarKind::Bool,
                Some("FLOAT") => VarKind::Number,
                Some("STRING") => VarKind::Text,
                Some("COLOR") => VarKind::Color,
                _ => continue,
            };
            let entries: Vec<&Val> = n.get("variableDataValues").map(|d| d.arr("entries").iter().collect()).unwrap_or_default();
            let mids = mode_ids.get(&set).cloned().unwrap_or_default();
            let default = match kind {
                VarKind::Bool => VarValue::Bool(false),
                VarKind::Number => VarValue::Number(0.0),
                VarKind::Text => VarValue::Text(String::new()),
                VarKind::Color => VarValue::Color(Color32::BLACK),
            };
            let values: Vec<VarValue> = (0..out[ci].modes.len())
                .map(|k| {
                    let entry = entries.iter().find(|e| mids.get(k).is_some_and(|m| e.get("modeID").and_then(guid_any) == Some(*m)));
                    let v = entry.and_then(|e| e.get("variableData")).and_then(|d| d.get("value"));
                    let Some(v) = v else { return default.clone() };
                    if let Some(target) = v.get("alias").and_then(guid_any).and_then(|a| self.var_ids.get(&a)) {
                        return VarValue::Alias(*target);
                    }
                    match kind {
                        VarKind::Bool => v.b("boolValue").map_or(default.clone(), VarValue::Bool),
                        VarKind::Number => v.f("floatValue").map_or(default.clone(), |x| VarValue::Number(x as f32)),
                        VarKind::Text => v.s("textValue").map_or(default.clone(), |t| VarValue::Text(t.to_owned())),
                        VarKind::Color => v.get("colorValue").map_or(default.clone(), |c| VarValue::Color(color(c, 1.0))),
                    }
                })
                .collect();
            let id = self.var_ids[&g];
            self.var_names.insert(g, format!("{} / {}", out[ci].name, n.s("name").unwrap_or("variable")));
            out[ci].vars.push(Variable { id, name: n.s("name").unwrap_or("variable").to_owned(), kind, values, scopes: vec![], syntax: Default::default(), hidden: false });
            self.summary.variables += 1;
        }
        // a variable that did not become one must not stay referenced
        let kept: HashSet<u64> = out.iter().flat_map(|c| c.vars.iter().map(|v| v.id)).collect();
        self.var_ids.retain(|_, id| kept.contains(id));
        out
    }

    // a variable value as drui's expression language writes it (expr.rs): constants, variables by name, and the operators it has
    fn expr_of(&self, vd: &Val) -> Option<String> {
        let v = vd.get("value").unwrap_or(vd);
        if let Some(g) = v.get("alias").and_then(guid_any) {
            return self.var_names.get(&g).map(|n| format!("`{n}`"));
        }
        if let Some(b) = v.b("boolValue") {
            return Some(b.to_string());
        }
        if let Some(x) = v.f("floatValue") {
            return Some(format!("{x}"));
        }
        if let Some(t) = v.s("textValue") {
            return Some(format!("\"{}\"", t.replace('"', "'")));
        }
        let e = v.get("expressionValue")?;
        let args: Vec<String> = e.arr("expressionArguments").iter().map(|a| self.expr_of(a)).collect::<Option<_>>()?;
        let bin = |op: &str| args.get(0).zip(args.get(1)).map(|(a, b)| format!("({a} {op} {b})"));
        match e.s("expressionFunction")? {
            "ADDITION" => bin("+"),
            "SUBTRACTION" => bin("-"),
            "MULTIPLY" => bin("*"),
            "DIVIDE" => bin("/"),
            "EQUALS" => bin("=="),
            "NOT_EQUAL" => bin("!="),
            "LESS_THAN" => bin("<"),
            "LESS_THAN_OR_EQUAL" => bin("<="),
            "GREATER_THAN" => bin(">"),
            "GREATER_THAN_OR_EQUAL" => bin(">="),
            "AND" => bin("&&"),
            "OR" => bin("||"),
            "NOT" => args.first().map(|a| format!("!({a})")),
            "NEGATE" => args.first().map(|a| format!("-({a})")),
            "IS_TRUTHY" => args.first().map(|a| format!("({a})")),
            _ => None,
        }
    }

    // ---- prototype interactions

    // the interactions of node `n`: (what it does, the figma frame it leads to, an overlay's offset from the layer)
    fn interactions_of(&self, n: &Val) -> Vec<(Interaction, Option<Guid>, Option<Vec2>)> {
        let mut out = vec![];
        for pi in n.arr("prototypeInteractions") {
            if pi.b("isDeleted") == Some(true) {
                continue;
            }
            let ev = pi.get("event");
            let kind = ev.and_then(|e| e.s("interactionType")).unwrap_or("ON_CLICK");
            // an `if / else if / else` becomes actions with conditions: each runs when its own condition holds and no earlier one did
            let mut actions: Vec<(&Val, Option<String>)> = vec![];
            let mut earlier: Vec<String> = vec![];
            for a in pi.arr("actions") {
                let blocks = ["conditionalActions", "conditionalBlocks", "conditions"].iter().map(|k| a.arr(k)).find(|b| !b.is_empty());
                match (a.s("connectionType"), blocks) {
                    (Some("CONDITIONAL"), Some(blocks)) | (_, Some(blocks)) => {
                        for b in blocks {
                            let cond = first_of(b, &["condition", "conditionExpression"]).and_then(|c| self.expr_of(c));
                            let mut guard: Vec<String> = earlier.iter().map(|e| format!("!({e})")).collect();
                            if let Some(c) = &cond {
                                guard.push(c.clone());
                                earlier.push(c.clone());
                            }
                            let guard = (!guard.is_empty()).then(|| guard.join(" && "));
                            for inner in b.arr("actions") {
                                actions.push((inner, guard.clone()));
                            }
                        }
                    }
                    _ => actions.push((a, None)),
                }
            }
            for (a, condition) in actions {
                let conn = a.s("connectionType").unwrap_or("NONE");
                let nav = a.s("navigationType").unwrap_or("NAVIGATE");
                let set_var = conn == "SET_VARIABLE" || nav == "SET_VARIABLE" || first_of(a, &["targetVariableID", "targetVariableId"]).is_some();
                let action = match (conn, nav) {
                    _ if set_var => Action::SetVariable,
                    ("NONE", _) => continue,
                    ("BACK", _) => Action::Back,
                    ("CLOSE", _) => Action::CloseOverlay,
                    ("URL", _) => Action::OpenLink,
                    (_, "OVERLAY") => Action::OpenOverlay,
                    (_, "SWAP") => Action::SwapOverlay,
                    (_, "SCROLL_TO") => Action::ScrollTo,
                    (_, "CHANGE_TO") | (_, "SWAP_STATE") => Action::ChangeTo,
                    _ => Action::Navigate,
                };
                let trigger = match (kind, action) {
                    ("ON_HOVER", Action::ChangeTo) => Trigger::WhileHover,
                    ("ON_PRESS", Action::ChangeTo) => Trigger::WhilePress,
                    ("ON_HOVER", _) | ("MOUSE_ENTER", _) => Trigger::MouseEnter,
                    ("MOUSE_LEAVE", _) => Trigger::MouseLeave,
                    ("ON_PRESS", _) | ("MOUSE_DOWN", _) => Trigger::MouseDown,
                    ("MOUSE_UP", _) => Trigger::MouseUp,
                    ("ON_DRAG", _) => Trigger::Drag,
                    ("AFTER_TIMEOUT", _) => Trigger::AfterDelay,
                    ("ON_KEYBOARD_GAMEPAD", _) => Trigger::KeyPress,
                    _ => Trigger::Click,
                };
                let dir = a.s("transitionDirection").unwrap_or("");
                let transition = match a.s("transitionType").unwrap_or("INSTANT") {
                    "DISSOLVE" => Transition::Dissolve,
                    "SMART_ANIMATE" => Transition::SmartAnimate,
                    "MOVE_IN" | "MOVE_OUT" | "PUSH" | "SLIDE_IN" | "SLIDE_OUT" => match dir {
                        "LEFT" => Transition::SlideRight,
                        "TOP" => Transition::SlideDown,
                        "BOTTOM" => Transition::SlideUp,
                        _ => Transition::SlideLeft,
                    },
                    _ => Transition::Instant,
                };
                let e = a.s("easingType").unwrap_or("").to_lowercase();
                let easing = if e.contains("bouncy") {
                    Easing::SpringBouncy
                } else if e.contains("gentle") {
                    Easing::SpringGentle
                } else if e.contains("quick") {
                    Easing::SpringQuick
                } else if e.contains("slow") {
                    Easing::SpringSlow
                } else if e.contains("linear") {
                    Easing::Linear
                } else if e.contains("back") {
                    if e.contains("in_and_out") || e.contains("inout") { Easing::EaseInOutBack } else if e.starts_with("in") || e.contains("ease_in_back") { Easing::EaseInBack } else { Easing::EaseOutBack }
                } else if e.contains("in_and_out") || e.contains("inout") || e.contains("in_out") {
                    Easing::EaseInOut
                } else if e.starts_with("in") || e.contains("ease_in") {
                    Easing::EaseIn
                } else {
                    Easing::EaseOut
                };
                let mut ix = Interaction {
                    trigger,
                    action,
                    transition,
                    easing,
                    duration: a.f("transitionDuration").unwrap_or(0.3) as f32,
                    delay: ev.and_then(|e| e.f("transitionTimeout")).unwrap_or(0.0) as f32,
                    url: a.s("connectionURL").unwrap_or("").to_owned(),
                    condition: condition.clone().unwrap_or_default(),
                    ..Interaction::default()
                };
                if action == Action::SetVariable {
                    // the variable it sets and the expression it sets it to; one that did not import is left out
                    let target = first_guid(a, &["targetVariableID", "targetVariableId", "variableID", "variableId"]).and_then(|g| self.var_ids.get(&g)).copied();
                    let value = first_of(a, &["targetVariableValue", "variableValue", "value"]).and_then(|v| self.expr_of(v));
                    let (Some(var), Some(value)) = (target, value) else { continue };
                    ix.var = Some(var);
                    ix.value = value;
                }
                let rel = a.get("overlayRelativePosition").map(|v| Vec2::new(v.f("x").unwrap_or(0.0) as f32, v.f("y").unwrap_or(0.0) as f32));
                out.push((ix, a.get("transitionNodeID").and_then(guid_of), rel));
            }
        }
        out
    }

    // once a page is built: interactions get the layers they lead to (a target on another page, or one that was not imported, is dropped),
    // and an instance's layers take their component's interactions
    fn finish_page(&mut self, app: &mut App) {
        for (id, n) in std::mem::take(&mut self.pending_ix) {
            let Some(i) = app.index_of(id) else { continue };
            let mut list: Vec<Interaction> = vec![];
            for (mut ix, target, rel) in self.interactions_of(n) {
                if ix.action.needs_target() {
                    match target.and_then(|g| self.built.get(&g)) {
                        Some(&t) => ix.link = Some(t),
                        None => continue,
                    }
                }
                if matches!(ix.action, Action::OpenOverlay | Action::SwapOverlay) {
                    if let Some(t) = target.and_then(|g| self.nodes.get(&g)) {
                        // where the overlay sits, and what the page behind it does, are settings of the overlay frame
                        let (pos, close, dim) = overlay_settings(t);
                        ix.overlay_pos = pos;
                        ix.close_outside = close;
                        ix.overlay_dim = dim;
                        if pos == 7 {
                            // a manual position is given from the triggering layer: drui measures from the presented frame
                            let (layer, root) = (app.world_rect(i).min, app.chain(i).last().map_or(app.world_rect(i).min, |&r| app.world_rect(r).min));
                            ix.overlay_off = (layer - root) + rel.unwrap_or(Vec2::ZERO);
                        }
                    }
                }
                list.push(ix);
            }
            self.summary.interactions += list.len();
            let s = &mut app.shapes[i];
            s.more.clear();
            s.link = None;
            for ix in list {
                if s.link.is_none() && ix.link.is_some() {
                    s.set_primary_interaction(ix);
                } else {
                    s.more.push(ix);
                }
            }
        }
        // pattern fills now know the layer they repeat
        for (id, spec) in std::mem::take(&mut self.pending_patterns) {
            let (Some(i), Some(&src)) = (app.index_of(id), self.built.get(&spec.source)) else {
                *self.summary.skipped.entry("pattern fills (the repeated layer is not on this page)".into()).or_default() += 1;
                continue;
            };
            let src_size = app.index_of(src).map_or(Vec2::splat(1.0), |k| app.shapes[k].size.max(Vec2::splat(1.0)));
            let s = &mut app.shapes[i];
            if let Some(PaintKind::Pattern(pf)) = s.fills.iter_mut().chain(s.strokes.iter_mut()).map(|p| &mut p.kind).find(|k| matches!(k, PaintKind::Pattern(pf) if pf.source.is_none())) {
                *pf = PatternFill { source: Some(src), scale: spec.scale, space: Vec2::new(spec.spacing.x / src_size.x, spec.spacing.y / src_size.y), rotation: spec.rotation, offset: Vec2::ZERO, ..PatternFill::default() };
                self.summary.patterns += 1;
            }
        }
        // mirrors of a component's layers carry its interactions
        let copies: Vec<(usize, u64)> = app.shapes.iter().enumerate().filter(|(_, s)| s.src.is_some() && s.interactions().is_empty()).map(|(i, s)| (i, s.src.unwrap_or(0))).collect();
        for (i, src) in copies {
            if let Some(si) = app.index_of(src) {
                if !app.shapes[si].interactions().is_empty() {
                    let from = app.shapes[si].clone();
                    let s = &mut app.shapes[i];
                    s.link = from.link;
                    if from.link.is_some() {
                        s.set_primary_interaction(from.primary_interaction());
                    }
                    s.more = from.more.clone();
                }
            }
        }
    }

    // ---- building layers

    fn new_shape(&mut self, app: &mut App, kind: Kind, parent: Option<u64>) -> Shape {
        let id = app.new_id();
        self.count += 1;
        let mut s = Shape::new(kind, id, self.count, Pos2::ZERO, parent);
        s.fills.clear();
        s.strokes.clear();
        s.effects.clear();
        s.stroke_w = 0.0;
        s.clip = false;
        s
    }

    // the layer for node `g` under `parent` (and, for containers, everything inside it); None for what drui has no layer for
    fn build(&mut self, app: &mut App, g: Guid, parent: Option<u64>) -> Option<u64> {
        if let Some(&id) = self.built.get(&g) {
            // made earlier because an instance needed it: only its children may be missing
            self.build_children(app, g, id);
            return Some(id);
        }
        if !self.busy.insert(g) {
            return None;
        }
        let r = self.build_new(app, g, parent);
        self.busy.remove(&g);
        r
    }

    fn build_children(&mut self, app: &mut App, g: Guid, id: u64) {
        for k in self.kids_of(g) {
            self.build(app, k, Some(id));
        }
    }

    fn build_new(&mut self, app: &mut App, g: Guid, parent: Option<u64>) -> Option<u64> {
        let n = *self.nodes.get(&g)?;
        let ty = n.s("type").unwrap_or("");
        let rotated = n.get("transform").is_some_and(|t| (t.f("m10").unwrap_or(0.0)).abs() > 1e-6 || t.f("m00").unwrap_or(1.0) * t.f("m11").unwrap_or(1.0) - t.f("m01").unwrap_or(0.0) * t.f("m10").unwrap_or(0.0) < 0.0);
        let kind = match ty {
            "FRAME" | "SYMBOL" | "SECTION" | "INSTANCE" => Kind::Frame,
            "GROUP" | "BOOLEAN_OPERATION" if rotated => Kind::Frame, // a group cannot be turned; a frame can
            "GROUP" | "BOOLEAN_OPERATION" => Kind::Group,
            "RECTANGLE" | "ROUNDED_RECTANGLE" | "SLICE" => Kind::Rect,
            "ELLIPSE" => Kind::Ellipse,
            "LINE" => Kind::Line,
            "STAR" => Kind::Star,
            "REGULAR_POLYGON" => Kind::Polygon,
            "VECTOR" => Kind::Path,
            "TEXT" => Kind::Text,
            other => {
                if !matches!(other, "DOCUMENT" | "CANVAS") {
                    *self.summary.skipped.entry(other.to_owned()).or_default() += 1;
                }
                return None;
            }
        };
        if ty == "INSTANCE" {
            return self.build_instance(app, g, parent);
        }
        let mut s = self.new_shape(app, kind, parent);
        self.flipped = false;
        self.apply(app, &mut s, n);
        let flipped = self.flipped;
        // a group of variants is a component set; each variant is named "Property=Value, Property=Value"
        if ty == "FRAME" && n.b("isStateGroup") == Some(true) {
            s.component_set = true;
            s.clip = false;
        }
        if ty == "SYMBOL" && self.parent_of.get(&g).and_then(|p| self.nodes.get(p)).is_some_and(|p| p.b("isStateGroup") == Some(true)) {
            s.variant = s
                .name
                .split(',')
                .filter_map(|part| part.split_once('=').map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned())))
                .collect();
        }
        match ty {
            "SYMBOL" => s.component = true,
            "SECTION" => s.section = true,
            "SLICE" => s.slice = true,
            "BOOLEAN_OPERATION" => {
                s.bool_op = Some(match n.s("booleanOperation") {
                    Some("SUBTRACT") => 1,
                    Some("INTERSECT") => 2,
                    Some("XOR") | Some("EXCLUDE") => 3,
                    _ => 0,
                })
            }
            "ELLIPSE" if n.get("arcData").is_none() => s.arc = [0.0, 360.0],
            _ => {}
        }
        if kind == Kind::Line {
            // a line's box is its own ends: the start and the vector to the end
            if let Some(t) = n.get("transform") {
                let (w, m00, m01, m02, m10, m11, m12) = (s.size.x, t.f("m00").unwrap_or(1.0), t.f("m01").unwrap_or(0.0), t.f("m02").unwrap_or(0.0), t.f("m10").unwrap_or(0.0), t.f("m11").unwrap_or(1.0), t.f("m12").unwrap_or(0.0));
                let _ = (m01, m11);
                s.pos = Pos2::new(m02 as f32, m12 as f32);
                s.size = Vec2::new((m00 * w as f64) as f32, (m10 * w as f64) as f32);
                s.rotation = 0.0;
            }
        }
        let id = s.id;
        self.summary.layers += 1;
        app.shapes.push(s);
        self.built.insert(g, id);
        if !n.arr("prototypeInteractions").is_empty() {
            self.pending_ix.push((id, n));
        }
        if kind.is_container() {
            self.build_children(app, g, id);
        }
        if flipped {
            app.flip_inside(id, true);
        }
        Some(id)
    }

    // an instance: a linked instance of its component when that is on this page, otherwise a copy of the component's contents
    fn build_instance(&mut self, app: &mut App, g: Guid, parent: Option<u64>) -> Option<u64> {
        let n = *self.nodes.get(&g)?;
        let symbol = n.get("symbolData").and_then(|d| d.get("symbolID")).and_then(guid_of)?;
        let overrides: Vec<&Val> = n.get("symbolData").map(|d| d.arr("symbolOverrides").iter().collect()).unwrap_or_default();
        let derived: Vec<&Val> = n.arr("derivedSymbolData").iter().collect();
        self.summary.instances += 1;
        if !self.nodes.contains_key(&symbol) {
            // the component lives in another file (a library): there is nothing to copy
            *self.summary.skipped.entry("library components".into()).or_default() += 1;
        }
        if self.page_nodes.contains(&symbol) && self.ty(symbol) == "SYMBOL" {
            // make sure the component (and what it is inside) exists, then instantiate it
            self.ensure(app, symbol);
            if let Some(&main) = self.built.get(&symbol) {
                if let Some(root) = app.instantiate(main, parent, Pos2::ZERO) {
                    let i = app.index_of(root)?;
                    let mut s = app.shapes[i].clone();
                    self.flipped = false;
                    self.apply(app, &mut s, n);
                    let flipped = self.flipped;
                    s.component = false;
                    s.instance = true;
                    app.shapes[i] = s;
                    // sizes and places of the layers inside (an instance that was resized or laid out), then the overrides
                    for o in derived.iter().chain(overrides.iter()) {
                        self.apply_override(app, root, symbol, o);
                    }
                    self.built.insert(g, root);
                    self.summary.layers += 1;
                    if !n.arr("prototypeInteractions").is_empty() {
                        self.pending_ix.push((root, n));
                    }
                    if flipped {
                        app.flip_inside(root, true);
                    }
                    return Some(root);
                }
            }
        }
        // a copy: a frame with the component's layers inside, the overrides applied as they are built
        self.summary.copies += 1;
        let mut s = self.new_shape(app, Kind::Frame, parent);
        self.apply(app, &mut s, n);
        let id = s.id;
        app.shapes.push(s);
        self.built.insert(g, id);
        self.summary.layers += 1;
        let by_last: HashMap<Guid, Vec<&Val>> = derived.iter().chain(overrides.iter()).fold(HashMap::new(), |mut m, o| {
            if let Some(last) = o.get("guidPath").map(|p| p.arr("guids")).and_then(|g| g.last()).and_then(guid_of) {
                m.entry(last).or_default().push(*o);
            }
            m
        });
        self.build_copy_children(app, symbol, id, &by_last, 0);
        Some(id)
    }

    fn build_copy_children(&mut self, app: &mut App, from: Guid, parent: u64, ov: &HashMap<Guid, Vec<&Val>>, depth: usize) {
        if depth > 24 {
            return;
        }
        for k in self.kids_of(from) {
            let Some(&n) = self.nodes.get(&k) else { continue };
            let ty = n.s("type").unwrap_or("");
            if ty == "INSTANCE" {
                // an instance inside the component: the same again, one level down
                if let Some(sym) = n.get("symbolData").and_then(|d| d.get("symbolID")).and_then(guid_of) {
                    let mut s = self.new_shape(app, Kind::Frame, Some(parent));
                    self.apply(app, &mut s, n);
                    let id = s.id;
                    app.shapes.push(s);
                    self.summary.layers += 1;
                    self.build_copy_children(app, sym, id, ov, depth + 1);
                }
                continue;
            }
            let kind = match ty {
                "FRAME" | "SYMBOL" | "SECTION" => Kind::Frame,
                "GROUP" | "BOOLEAN_OPERATION" => Kind::Group,
                "RECTANGLE" | "ROUNDED_RECTANGLE" => Kind::Rect,
                "ELLIPSE" => Kind::Ellipse,
                "LINE" => Kind::Line,
                "STAR" => Kind::Star,
                "REGULAR_POLYGON" => Kind::Polygon,
                "VECTOR" => Kind::Path,
                "TEXT" => Kind::Text,
                _ => continue,
            };
            let mut s = self.new_shape(app, kind, Some(parent));
            self.apply(app, &mut s, n);
            for o in ov.get(&k).into_iter().flatten() {
                self.apply(app, &mut s, o);
            }
            let id = s.id;
            self.summary.layers += 1;
            app.shapes.push(s);
            if kind.is_container() {
                self.build_copy_children(app, k, id, ov, depth + 1);
            }
        }
    }

    // `o` (an override or derived-data entry) names a layer inside the component by a path of guids; set what it carries on the
    // matching layer of the instance `root`
    fn apply_override(&mut self, app: &mut App, root: u64, symbol: Guid, o: &Val) {
        let path: Vec<Guid> = o.get("guidPath").map(|p| p.arr("guids")).unwrap_or_default().iter().filter_map(guid_of).collect();
        if path.is_empty() {
            return;
        }
        let mut within: Vec<u64> = vec![root];
        for g in &path {
            if *g == symbol {
                continue;
            }
            let Some(&target) = self.built.get(g) else { return };
            let pool: Vec<u64> = within.clone();
            let found: Vec<u64> = app
                .shapes
                .iter()
                .filter(|s| pool.iter().any(|&p| p != s.id && app.is_inside(s.id, p)) && origin_chain(app, s.id).contains(&target))
                .map(|s| s.id)
                .collect();
            if found.is_empty() {
                return;
            }
            within = found;
        }
        if within == vec![root] {
            return;
        }
        for id in within {
            if let Some(i) = app.index_of(id) {
                let mut s = app.shapes[i].clone();
                self.apply(app, &mut s, o);
                app.shapes[i] = s;
            }
        }
    }

    // builds `g` and what it sits in (so that the shape exists to be instantiated), without its siblings
    fn ensure(&mut self, app: &mut App, g: Guid) {
        if self.built.contains_key(&g) || self.busy.contains(&g) {
            return;
        }
        let mut chain = vec![g];
        let mut at = g;
        while let Some(&p) = self.parent_of.get(&at) {
            if self.built.contains_key(&p) || self.ty(p) == "CANVAS" || self.ty(p) == "DOCUMENT" {
                break;
            }
            chain.push(p);
            at = p;
        }
        // outermost first; ancestors are made without their children (the page traversal fills those in later)
        let top_parent = chain.last().and_then(|c| self.parent_of.get(c)).and_then(|p| self.built.get(p)).copied();
        let mut parent_id = top_parent;
        for (k, &c) in chain.iter().rev().enumerate() {
            let last = k + 1 == chain.len();
            if let Some(&id) = self.built.get(&c) {
                parent_id = Some(id);
                continue;
            }
            if last {
                parent_id = self.build(app, c, parent_id);
            } else if let Some(&n) = self.nodes.get(&c) {
                let kind = if self.ty(c) == "GROUP" { Kind::Group } else { Kind::Frame };
                let mut s = self.new_shape(app, kind, parent_id);
                self.apply(app, &mut s, n);
                if self.ty(c) == "SYMBOL" {
                    s.component = true;
                }
                let id = s.id;
                self.summary.layers += 1;
                app.shapes.push(s);
                self.built.insert(c, id);
                parent_id = Some(id);
            }
        }
    }
}

// an overlay frame's own settings: (position 0 centre, 1..=3 top, 4..=6 bottom, 7 manual; a click outside closes it; the page behind is dimmed)
fn overlay_settings(frame: &Val) -> (u8, bool, bool) {
    let pos = match frame.s("overlayPositionType") {
        Some("TOP_LEFT") => 1,
        Some("TOP_CENTER") => 2,
        Some("TOP_RIGHT") => 3,
        Some("BOTTOM_LEFT") => 4,
        Some("BOTTOM_CENTER") => 5,
        Some("BOTTOM_RIGHT") => 6,
        Some("MANUAL") => 7,
        _ => 0,
    };
    let close = frame.s("overlayBackgroundInteraction") == Some("CLOSE_ON_CLICK_OUTSIDE");
    // a background colour with some opacity dims; none at all does not
    let dim = frame.get("overlayBackgroundAppearance").is_some_and(|a| a.get("backgroundColor").is_some_and(|c| c.f("a").unwrap_or(1.0) > 0.0));
    (pos, close, dim)
}

// the shapes `id` was copied from, nearest first (an instance's layer mirrors the component's, which may itself be a copy)
fn origin_chain(app: &App, id: u64) -> Vec<u64> {
    let mut out = vec![];
    let mut at = app.index_of(id).and_then(|i| app.shapes[i].src);
    while let Some(s) = at {
        if out.contains(&s) {
            break;
        }
        out.push(s);
        at = app.index_of(s).and_then(|i| app.shapes[i].src);
    }
    out
}

// a node's own matrix (to its parent) to drui's position and turn: the box's centre is where the matrix sends the middle of the
// layer, and the turn is about that centre
fn place(s: &mut Shape, t: &Val) -> bool {
    let (m00, m01, m02, m10, m11, m12) = (t.f("m00").unwrap_or(1.0), t.f("m01").unwrap_or(0.0), t.f("m02").unwrap_or(0.0), t.f("m10").unwrap_or(0.0), t.f("m11").unwrap_or(1.0), t.f("m12").unwrap_or(0.0));
    let (w, h) = (s.size.x as f64, s.size.y as f64);
    let center = (m00 * w / 2.0 + m01 * h / 2.0 + m02, m10 * w / 2.0 + m11 * h / 2.0 + m12);
    // a matrix that mirrors is a mirror of the layer's own contents (about its own middle) followed by a turn: the turn is found
    // from the mirrored x axis, and the mirror is applied once the layer's contents exist (`App::flip_inside`, which also negates
    // the turn, hence the sign here)
    let flipped = m00 * m11 - m01 * m10 < 0.0;
    let turn = if flipped { (-m10).atan2(-m00) } else { m10.atan2(m00) };
    s.rotation = if s.kind == Kind::Group { 0.0 } else if flipped { -turn.to_degrees() as f32 } else { turn.to_degrees() as f32 };
    if s.rotation.abs() < 1e-3 {
        s.rotation = 0.0;
    }
    s.pos = Pos2::new((center.0 - w / 2.0) as f32, (center.1 - h / 2.0) as f32);
    flipped && !matches!(s.kind, Kind::Group | Kind::Line)
}

fn blend(name: Option<&str>) -> BlendMode {
    match name.unwrap_or("NORMAL") {
        "DARKEN" => BlendMode::Darken,
        "MULTIPLY" => BlendMode::Multiply,
        "LINEAR_BURN" | "PLUS_DARKER" => BlendMode::PlusDarker,
        "COLOR_BURN" => BlendMode::ColorBurn,
        "LIGHTEN" => BlendMode::Lighten,
        "SCREEN" => BlendMode::Screen,
        "LINEAR_DODGE" | "PLUS_LIGHTER" => BlendMode::PlusLighter,
        "PASS_THROUGH" => BlendMode::PassThrough,
        "COLOR_DODGE" => BlendMode::ColorDodge,
        "OVERLAY" => BlendMode::Overlay,
        "SOFT_LIGHT" => BlendMode::SoftLight,
        "HARD_LIGHT" => BlendMode::HardLight,
        "DIFFERENCE" => BlendMode::Difference,
        "EXCLUSION" => BlendMode::Exclusion,
        "HUE" => BlendMode::Hue,
        "SATURATION" => BlendMode::Saturation,
        "COLOR" => BlendMode::Color,
        "LUMINOSITY" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

fn constraint(c: &str) -> Constraint {
    match c {
        "MAX" => Constraint::Max,
        "STRETCH" => Constraint::Both,
        "CENTER" => Constraint::Center,
        "SCALE" => Constraint::Scale,
        _ => Constraint::Min,
    }
}

fn weight_of(style: &str) -> u16 {
    let s = style.replace(['-', ' '], "");
    for (name, w) in [("thin", 100), ("extralight", 200), ("ultralight", 200), ("light", 300), ("medium", 500), ("semibold", 600), ("demibold", 600), ("extrabold", 800), ("ultrabold", 800), ("bold", 700), ("black", 900), ("heavy", 900)] {
        if s.contains(name) {
            return w;
        }
    }
    0 // Regular
}

fn effect(e: &Val) -> Option<Effect> {
    let kind = match e.s("type")? {
        "DROP_SHADOW" => EffectKind::DropShadow,
        "INNER_SHADOW" => EffectKind::InnerShadow,
        "FOREGROUND_BLUR" => EffectKind::LayerBlur,
        "BACKGROUND_BLUR" => EffectKind::BackgroundBlur,
        _ => return None,
    };
    let mut fx = Effect::new(kind);
    fx.visible = e.b("visible").unwrap_or(true);
    fx.blur = e.f("radius").unwrap_or(0.0) as f32;
    fx.spread = e.f("spread").unwrap_or(0.0) as f32;
    if let Some(o) = e.get("offset") {
        fx.dx = o.f("x").unwrap_or(0.0) as f32;
        fx.dy = o.f("y").unwrap_or(0.0) as f32;
    }
    if let Some(c) = e.get("color") {
        fx.color = color(c, 1.0);
    }
    fx.show_behind = e.b("showShadowBehindNode").unwrap_or(false);
    fx.blend = blend(e.s("blendMode"));
    Some(fx)
}

// Figma's paint transform maps the layer's unit square to gradient space, where a linear gradient runs along x from 0 to 1 at y = 0.5
// and the others are centred at (0.5, 0.5) with radius 0.5. The inverse gives the gradient's place on the layer.
fn gradient_from_transform(g: &mut Gradient, t: &Val, size: Vec2) {
    let (a, b, c, d, e, f) = (t.f("m00").unwrap_or(1.0), t.f("m01").unwrap_or(0.0), t.f("m02").unwrap_or(0.0), t.f("m10").unwrap_or(0.0), t.f("m11").unwrap_or(1.0), t.f("m12").unwrap_or(0.0));
    let (w, h) = (size.x.max(1e-3) as f64, size.y.max(1e-3) as f64);
    let det = a * e - b * d;
    if det.abs() < 1e-9 {
        return;
    }
    // the layer-space point that goes to gradient space (u, v)
    let inv = |u: f64, v: f64| ((e * (u - c) - b * (v - f)) / det, (-d * (u - c) + a * (v - f)) / det);
    match g.kind {
        GradientKind::Linear => {
            // the gradient parameter is x' = a x + b y + c over the unit square; in pixels its gradient points along (a / w, b / h)
            let (gx, gy) = (a / w, b / h);
            let len2 = (gx * gx + gy * gy).max(1e-12);
            g.angle = gy.atan2(gx).to_degrees() as f32;
            // where the parameter is 0.5, as the nearest point of that line to the middle of the layer
            let t0 = a * 0.5 + b * 0.5 + c;
            let (sx, sy) = ((0.5 - t0) * gx / len2, (0.5 - t0) * gy / len2);
            g.center = Pos2::new((0.5 + sx / w) as f32, (0.5 + sy / h) as f32);
            let along = (w * g.angle.to_radians().cos() as f64).abs() + (h * g.angle.to_radians().sin() as f64).abs();
            g.length = ((1.0 / len2.sqrt()) / along.max(1e-6)) as f32;
        }
        _ => {
            let (cx, cy) = inv(0.5, 0.5);
            g.center = Pos2::new(cx as f32, cy as f32);
            let (rx, ry) = (inv(1.0, 0.5), inv(0.5, 1.0));
            let (ux, uy) = (rx.0 - cx, rx.1 - cy);
            let (vx, vy) = (ry.0 - cx, ry.1 - cy);
            g.scale = Vec2::new(((ux * ux + uy * uy).sqrt() * 2.0) as f32, ((vx * vx + vy * vy).sqrt() * 2.0) as f32);
            if g.kind == GradientKind::Angular {
                g.angle = uy.atan2(ux).to_degrees() as f32;
            }
        }
    }
}

impl App {
    // the document of a .fig file: one page per canvas
    pub fn doc_from_fig(bytes: &[u8]) -> Result<(Doc, Summary), String> {
        let file = figfile::read(bytes)?;
        Self::doc_from_file(&file)
    }

    pub(crate) fn doc_from_file(file: &FigFile) -> Result<(Doc, Summary), String> {
        let mut imp = Importer::new(file);
        let doc_guid = imp.nodes.iter().find(|(_, n)| n.s("type") == Some("DOCUMENT")).map(|(g, _)| *g).ok_or("this file has no document")?;
        let canvases: Vec<Guid> = imp.kids_of(doc_guid).into_iter().filter(|&c| imp.ty(c) == "CANVAS" && imp.nodes[&c].b("internalOnly") != Some(true)).collect();
        if canvases.is_empty() {
            return Err("this file has no pages".into());
        }
        let mut app = App::default();
        let variables = imp.import_variables(&mut app);
        let mut pages = vec![];
        for c in &canvases {
            imp.built.clear();
            imp.collect_page(*c);
            app.shapes.clear();
            let node = imp.nodes[c];
            for k in imp.kids_of(*c) {
                imp.build(&mut app, k, None);
            }
            imp.finish_page(&mut app);
            let mut page = Page { name: node.s("name").unwrap_or("Page").to_owned(), shapes: std::mem::take(&mut app.shapes), ..Page::default() };
            if let (Some(bg), true) = (node.get("backgroundColor"), node.b("backgroundEnabled") != Some(false)) {
                page.background = color(bg, node.f("backgroundOpacity").unwrap_or(1.0));
            }
            pages.push(page);
        }
        imp.summary.pages = pages.len();
        // fonts the file uses that are not on this computer (looked up again in case one was installed a moment ago)
        crate::textrun::rescan_fonts_if_changed();
        imp.summary.fonts_missing = imp.fonts.iter().filter(|f| crate::textrun::family_weights(f).is_empty()).cloned().collect();
        let summary = std::mem::take(&mut imp.summary);
        let images = std::mem::take(&mut imp.images);
        let videos = std::mem::take(&mut imp.videos);
        let doc = Doc { pages, images, videos, variables, next_id: app.next_id, ..Doc::default() };
        Ok((doc, summary))
    }

    // File > Import Figma file: the .fig becomes the open document (a new one: it is saved next to the .fig as `<name>.json`)
    pub fn import_fig(&mut self, path: std::path::PathBuf) {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("could not read {}: {e}", path.display());
                return;
            }
        };
        match Self::doc_from_fig(&bytes) {
            Ok((doc, summary)) => {
                let save_as = path.with_extension("json");
                self.open_doc(doc, save_as);
                self.status = summary.text();
            }
            Err(e) => self.status = format!("could not import {}: {e}", path.display()),
        }
    }
}

// pieces of the importer, for the tests
#[cfg(test)]
pub fn place_for_test(s: &mut Shape, t: &Val) -> bool {
    place(s, t)
}

#[cfg(test)]
pub fn importer_for_test(file: &FigFile) -> Importer<'_> {
    Importer::new(file)
}

#[cfg(test)]
pub fn text_runs_for_test(imp: &mut Importer, s: &Shape, td: &Val) -> Vec<TextRun> {
    imp.text_runs(s, td)
}

#[cfg(test)]
pub fn interactions_for_test(n: &Val) -> Vec<(Interaction, Option<(i64, i64)>)> {
    let file = FigFile { message: Val::Obj(Default::default()), images: Default::default(), version: 0 };
    Importer::new(&file).interactions_of(n).into_iter().map(|(i, t, _)| (i, t)).collect()
}

#[cfg(test)]
pub fn variables_for_test(file: &FigFile, app: &mut App) -> (Vec<Collection>, usize) {
    let mut imp = Importer::new(file);
    let v = imp.import_variables(app);
    (v, imp.summary.variables)
}
