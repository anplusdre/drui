// File > Import Sketch file (.sketch): a .sketch is a zip of JSON (`document.json`, one `pages/<id>.json` per page) and `images/`.
// Artboards become frames, symbol masters components, symbol instances instances; groups, rectangles, ovals, polygons, stars,
// vector paths (with their curve handles), shape groups (as boolean groups), text and bitmaps are built with their fills (colour,
// gradient, image), borders, shadows, blurs, opacity and blend mode. What has no counterpart (slices, hotspots, nested symbol
// overrides, smart layout...) is counted and left out.
use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};

use egui::{Color32, Pos2, Vec2};
use serde_json::Value;

use crate::model::*;
use crate::App;

#[derive(Default)]
pub struct SketchSummary {
    pub pages: usize,
    pub layers: usize,
    pub symbols: usize,
    pub images: usize,
    pub skipped: BTreeMap<String, usize>,
}

impl SketchSummary {
    pub fn text(&self) -> String {
        let mut s = format!("imported {} page{}, {} layers", self.pages, if self.pages == 1 { "" } else { "s" }, self.layers);
        if self.symbols > 0 {
            s += &format!(", {} symbols", self.symbols);
        }
        if self.images > 0 {
            s += &format!(", {} images", self.images);
        }
        if !self.skipped.is_empty() {
            let list: Vec<String> = self.skipped.iter().map(|(k, n)| format!("{n} {k}")).collect();
            s += &format!("; left out: {}", list.join(", "));
        }
        s
    }
}

fn num(v: &Value, k: &str) -> f32 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn flag(v: &Value, k: &str, default: bool) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(default)
}

fn class(v: &Value) -> &str {
    v.get("_class").and_then(Value::as_str).unwrap_or("")
}

// {"red": 0.2, "green": ..., "alpha": 1}
fn color(v: &Value) -> Color32 {
    let c = |k: &str| (num(v, k).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(c("red"), c("green"), c("blue"), (v.get("alpha").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.0, 1.0) * 255.0).round() as u8)
}

// "{0.5, 0.25}"
fn point(v: &Value) -> Pos2 {
    let s = v.as_str().unwrap_or("{0, 0}");
    let mut it = s.trim_matches(|c| c == '{' || c == '}').split(',').map(|x| x.trim().parse::<f32>().unwrap_or(0.0));
    Pos2::new(it.next().unwrap_or(0.0), it.next().unwrap_or(0.0))
}

fn blend(n: i64) -> BlendMode {
    match n {
        1 => BlendMode::Darken,
        2 => BlendMode::Multiply,
        3 => BlendMode::ColorBurn,
        4 => BlendMode::Lighten,
        5 => BlendMode::Screen,
        6 => BlendMode::ColorDodge,
        7 => BlendMode::Overlay,
        8 => BlendMode::SoftLight,
        9 => BlendMode::HardLight,
        10 => BlendMode::Difference,
        11 => BlendMode::Exclusion,
        12 => BlendMode::Hue,
        13 => BlendMode::Saturation,
        14 => BlendMode::Color,
        15 => BlendMode::Luminosity,
        16 => BlendMode::PlusDarker,
        17 => BlendMode::PlusLighter,
        _ => BlendMode::Normal,
    }
}

struct Importer {
    zip: zip::ZipArchive<Cursor<Vec<u8>>>,
    app: App, // a scratch app: ids and a place to put the layers
    images: HashMap<String, Option<u64>>, // "images/<hash>" -> the asset
    symbols: HashMap<String, u64>,        // symbolID -> the component's id
    instances: Vec<(u64, String, Option<u64>, Pos2, Vec2, usize)>, // layer id placeholder, symbolID, parent, position, size, page
    page_of: HashMap<u64, usize>,         // top-level layer -> its page
    summary: SketchSummary,
}

impl Importer {
    fn json(&mut self, name: &str) -> Result<Value, String> {
        let mut f = self.zip.by_name(name).map_err(|_| format!("the file has no {name}"))?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| format!("{name}: {e}"))
    }

    fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        let mut f = self.zip.by_name(name).ok()?;
        let mut b = vec![];
        f.read_to_end(&mut b).ok()?;
        Some(b)
    }

    fn image(&mut self, v: &Value) -> Option<u64> {
        let r = v.get("_ref").and_then(Value::as_str)?.to_owned();
        if let Some(id) = self.images.get(&r) {
            return *id;
        }
        let bytes = self.read(&format!("{r}.png")).or_else(|| self.read(&r));
        let id = bytes.and_then(|b| self.app.register_image(r.rsplit('/').next().unwrap_or("image"), b));
        if id.is_some() {
            self.summary.images += 1;
        }
        self.images.insert(r, id);
        id
    }

    fn skip(&mut self, what: &str) {
        *self.summary.skipped.entry(what.to_owned()).or_default() += 1;
    }

    // the fills of a style
    fn fills(&mut self, style: &Value, size: Vec2) -> Vec<Paint> {
        let mut out = vec![];
        for f in style.get("fills").and_then(Value::as_array).into_iter().flatten() {
            if !flag(f, "isEnabled", true) {
                continue;
            }
            let c = f.get("color").map(color).unwrap_or(Color32::BLACK);
            let paint = match f.get("fillType").and_then(Value::as_i64).unwrap_or(0) {
                1 => {
                    let g = f.get("gradient").cloned().unwrap_or(Value::Null);
                    let stops: Vec<GradStop> = g.get("stops").and_then(Value::as_array).into_iter().flatten().map(|s| GradStop { pos: num(s, "position"), color: s.get("color").map(color).unwrap_or(Color32::BLACK) }).collect();
                    if stops.len() < 2 {
                        Paint::solid(c)
                    } else {
                        let (from, to) = (point(&g["from"]), point(&g["to"]));
                        let kind = match g.get("gradientType").and_then(Value::as_i64).unwrap_or(0) {
                            1 => GradientKind::Radial,
                            2 => GradientKind::Angular,
                            _ => GradientKind::Linear,
                        };
                        let d = Vec2::new((to.x - from.x) * size.x, (to.y - from.y) * size.y);
                        let mut gr = Gradient { kind, stops, ..Gradient::default() };
                        gr.angle = d.y.atan2(d.x).to_degrees();
                        gr.center = from;
                        Paint { kind: PaintKind::Gradient(gr), ..Paint::solid(c) }
                    }
                }
                4 => match f.get("image").and_then(|i| self.image(i)) {
                    Some(id) => {
                        let mut p = Paint { kind: PaintKind::Image(id), ..Paint::solid(c) };
                        p.image.mode = match f.get("patternFillType").and_then(Value::as_i64).unwrap_or(1) {
                            0 => ImageMode::Tile,
                            2 => ImageMode::Fit,
                            _ => ImageMode::Fill,
                        };
                        p
                    }
                    None => continue,
                },
                _ => Paint::solid(c),
            };
            let mut paint = paint;
            paint.opacity = 1.0;
            if let Some(b) = f.get("contextSettings").and_then(|c| c.get("blendMode")).and_then(Value::as_i64) {
                paint.blend = blend(b);
            }
            out.push(paint);
        }
        out
    }

    fn style(&mut self, s: &mut Shape, v: &Value) {
        let Some(style) = v.get("style") else { return };
        s.fills = self.fills(style, s.size);
        // borders
        s.strokes.clear();
        s.stroke_w = 0.0;
        for b in style.get("borders").and_then(Value::as_array).into_iter().flatten().filter(|b| flag(b, "isEnabled", true)) {
            s.strokes.push(Paint::solid(b.get("color").map(color).unwrap_or(Color32::BLACK)));
            s.stroke_w = s.stroke_w.max(num(b, "thickness"));
            s.stroke_align = match b.get("position").and_then(Value::as_i64).unwrap_or(0) {
                1 => StrokeAlign::Inside,
                2 => StrokeAlign::Outside,
                _ => StrokeAlign::Center,
            };
        }
        if let Some(bo) = style.get("borderOptions") {
            if let Some(d) = bo.get("dashPattern").and_then(Value::as_array).filter(|d| !d.is_empty()) {
                s.dash_pattern = d.iter().filter_map(Value::as_f64).map(|x| x as f32).collect();
                s.dash = true;
            }
            s.stroke_cap = [match bo.get("lineCapStyle").and_then(Value::as_i64).unwrap_or(0) {
                1 => StrokeCap::Round,
                2 => StrokeCap::Square,
                _ => StrokeCap::None,
            }; 2];
            s.stroke_join = match bo.get("lineJoinStyle").and_then(Value::as_i64).unwrap_or(0) {
                1 => StrokeJoin::Round,
                2 => StrokeJoin::Bevel,
                _ => StrokeJoin::Miter,
            };
        }
        // shadows
        for (key, kind) in [("shadows", EffectKind::DropShadow), ("innerShadows", EffectKind::InnerShadow)] {
            for sh in style.get(key).and_then(Value::as_array).into_iter().flatten().filter(|x| flag(x, "isEnabled", true)) {
                let mut e = Effect::new(kind);
                (e.dx, e.dy, e.blur, e.spread) = (num(sh, "offsetX"), num(sh, "offsetY"), num(sh, "blurRadius"), num(sh, "spread"));
                e.color = sh.get("color").map(color).unwrap_or(Color32::BLACK);
                s.effects.push(e);
            }
        }
        if let Some(bl) = style.get("blur").filter(|b| flag(b, "isEnabled", false)) {
            let mut e = Effect::new(if bl.get("type").and_then(Value::as_i64) == Some(3) { EffectKind::BackgroundBlur } else { EffectKind::LayerBlur });
            e.blur = num(bl, "radius");
            s.effects.push(e);
        }
        if let Some(cs) = style.get("contextSettings") {
            s.opacity = cs.get("opacity").and_then(Value::as_f64).unwrap_or(1.0) as f32;
            s.blend = blend(cs.get("blendMode").and_then(Value::as_i64).unwrap_or(0));
        }
    }

    fn base(&mut self, kind: Kind, v: &Value, parent: Option<u64>) -> Shape {
        let id = self.app.new_id();
        let fr = v.get("frame").cloned().unwrap_or(Value::Null);
        let mut s = Shape::new(kind, id, self.summary.layers + 1, Pos2::new(num(&fr, "x"), num(&fr, "y")), parent);
        s.size = Vec2::new(num(&fr, "width"), num(&fr, "height"));
        s.name = v.get("name").and_then(Value::as_str).unwrap_or("Layer").to_owned();
        s.visible = flag(v, "isVisible", true);
        s.locked = flag(v, "isLocked", false);
        s.rotation = -num(v, "rotation"); // Sketch turns counter-clockwise
        s.mask = flag(v, "hasClippingMask", false);
        s.fills.clear();
        s.strokes.clear();
        s.stroke_w = 0.0;
        self.summary.layers += 1;
        s
    }

    fn build(&mut self, v: &Value, parent: Option<u64>, page: usize) -> Option<u64> {
        let cls = class(v).to_owned();
        let kids = |v: &Value| v.get("layers").and_then(Value::as_array).cloned().unwrap_or_default();
        let id = match cls.as_str() {
            "artboard" | "symbolMaster" | "page" => {
                let mut s = self.base(Kind::Frame, v, parent);
                s.clip = true;
                if flag(v, "hasBackgroundColor", false) {
                    if let Some(c) = v.get("backgroundColor") {
                        s.fills = vec![Paint::solid(color(c))];
                    }
                }
                self.style(&mut s, v);
                if cls == "symbolMaster" {
                    s.component = true;
                    if let Some(sid) = v.get("symbolID").and_then(Value::as_str) {
                        self.symbols.insert(sid.to_owned(), s.id);
                    }
                    self.summary.symbols += 1;
                }
                let id = s.id;
                self.app.shapes.push(s);
                for k in kids(v) {
                    self.build(&k, Some(id), page);
                }
                id
            }
            "group" | "shapeGroup" => {
                let mut s = self.base(Kind::Group, v, parent);
                self.style(&mut s, v);
                if cls == "shapeGroup" {
                    // the operation of the second layer says how the layers combine
                    let ops: Vec<i64> = kids(v).iter().map(|k| k.get("booleanOperation").and_then(Value::as_i64).unwrap_or(-1)).collect();
                    let op = ops.iter().skip(1).copied().find(|&o| o >= 0).or_else(|| ops.first().copied().filter(|&o| o >= 0)).unwrap_or(0);
                    s.bool_op = Some(match op {
                        1 => 1,
                        2 => 2,
                        3 => 3,
                        _ => 0,
                    });
                }
                let id = s.id;
                self.app.shapes.push(s);
                for k in kids(v) {
                    self.build(&k, Some(id), page);
                }
                id
            }
            "rectangle" | "oval" | "triangle" | "star" | "polygon" => {
                let kind = match cls.as_str() {
                    "rectangle" => Kind::Rect,
                    "oval" => Kind::Ellipse,
                    "star" => Kind::Star,
                    _ => Kind::Polygon,
                };
                let mut s = self.base(kind, v, parent);
                self.style(&mut s, v);
                match cls.as_str() {
                    "rectangle" => {
                        let pts: Vec<f32> = v.get("points").and_then(Value::as_array).into_iter().flatten().map(|p| num(p, "cornerRadius")).collect();
                        s.radius = if pts.len() == 4 { [pts[0], pts[1], pts[2], pts[3]] } else { [num(v, "fixedRadius"); 4] };
                    }
                    "triangle" => s.sides = 3,
                    "star" | "polygon" => {
                        s.sides = v.get("numberOfPoints").and_then(Value::as_u64).unwrap_or(5).clamp(3, 64) as u32;
                        if cls == "star" {
                            s.inner = v.get("radius").and_then(Value::as_f64).unwrap_or(0.5) as f32;
                        }
                    }
                    _ => {}
                }
                let id = s.id;
                self.app.shapes.push(s);
                id
            }
            "shapePath" | "line" => {
                let mut s = self.base(Kind::Path, v, parent);
                self.style(&mut s, v);
                let (o, size) = (s.pos, s.size);
                let pts: Vec<PathPt> = v
                    .get("points")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|p| {
                        let at = |q: Pos2| Pos2::new(o.x + q.x * size.x, o.y + q.y * size.y);
                        let a = at(point(&p["point"]));
                        let hin = if flag(p, "hasCurveTo", false) { at(point(&p["curveTo"])) - a } else { Vec2::ZERO };
                        let hout = if flag(p, "hasCurveFrom", false) { at(point(&p["curveFrom"])) - a } else { Vec2::ZERO };
                        PathPt { p: a, hin, hout, radius: num(p, "cornerRadius") }
                    })
                    .collect();
                if pts.len() < 2 {
                    self.app.shapes.push(s);
                    return None;
                }
                let closed = flag(v, "isClosed", true);
                s.set_path_world(&[Contour { pts, closed, hole: false }]);
                s.rotation = 0.0;
                let id = s.id;
                self.app.shapes.push(s);
                id
            }
            "text" => {
                let mut s = self.base(Kind::Text, v, parent);
                self.style(&mut s, v);
                let astr = v.get("attributedString").cloned().unwrap_or(Value::Null);
                s.text = astr.get("string").and_then(Value::as_str).unwrap_or("").to_owned();
                if let Some(attrs) = astr.get("attributes").and_then(Value::as_array).and_then(|a| a.first()).and_then(|a| a.get("attributes")) {
                    if let Some(font) = attrs.get("MSAttributedStringFontAttribute").and_then(|f| f.get("attributes")) {
                        s.font_size = num(font, "size").max(1.0);
                        let ps = font.get("name").and_then(Value::as_str).unwrap_or("Inter");
                        let (fam, style) = ps.split_once('-').unwrap_or((ps, ""));
                        let low = style.to_lowercase();
                        for (k, w) in [("black", 900), ("heavy", 800), ("bold", 700), ("semibold", 600), ("medium", 500), ("light", 300), ("thin", 100)] {
                            if low.contains(k) {
                                s.weight = w;
                                break;
                            }
                        }
                        s.italic = low.contains("italic") || low.contains("oblique");
                        if !fam.is_empty() && !fam.starts_with(".") && fam != "Inter" {
                            s.family = fam.to_owned();
                        }
                    }
                    if let Some(c) = attrs.get("MSAttributedStringColorAttribute") {
                        s.fills = vec![Paint::solid(color(c))];
                    }
                    if let Some(k) = attrs.get("kerning").and_then(Value::as_f64) {
                        s.tracking = (k as f32 / s.font_size.max(1.0)) * 100.0;
                    }
                    if let Some(ps) = attrs.get("paragraphStyle") {
                        s.align = match ps.get("alignment").and_then(Value::as_i64).unwrap_or(0) {
                            1 => TextAlign::Right,
                            2 => TextAlign::Center,
                            _ => TextAlign::Left,
                        };
                        let lh = num(ps, "maximumLineHeight");
                        if lh > 0.0 {
                            s.line_h = lh / s.font_size.max(1.0);
                        }
                    }
                }
                if s.fills.is_empty() {
                    s.fills = vec![Paint::solid(Color32::BLACK)];
                }
                let id = s.id;
                self.app.shapes.push(s);
                id
            }
            "bitmap" => {
                let mut s = self.base(Kind::Rect, v, parent);
                self.style(&mut s, v);
                if let Some(img) = v.get("image").and_then(|i| self.image(i)) {
                    s.fills = vec![Paint { kind: PaintKind::Image(img), ..Paint::solid(Color32::WHITE) }];
                }
                let id = s.id;
                self.app.shapes.push(s);
                id
            }
            "symbolInstance" => {
                let fr = v.get("frame").cloned().unwrap_or(Value::Null);
                let sid = v.get("symbolID").and_then(Value::as_str).unwrap_or("").to_owned();
                self.instances.push((0, sid, parent, Pos2::new(num(&fr, "x"), num(&fr, "y")), Vec2::new(num(&fr, "width"), num(&fr, "height")), page));
                self.summary.layers += 1;
                return None;
            }
            other => {
                self.skip(if other.is_empty() { "unknown layers" } else { other });
                return None;
            }
        };
        if parent.is_none() {
            self.page_of.insert(id, page);
        }
        Some(id)
    }
}

impl App {
    pub fn doc_from_sketch(bytes: &[u8]) -> Result<(Doc, SketchSummary), String> {
        let zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).map_err(|_| "this is not a Sketch file".to_string())?;
        let mut imp = Importer { zip, app: App::default(), images: HashMap::new(), symbols: HashMap::new(), instances: vec![], page_of: HashMap::new(), summary: SketchSummary::default() };
        let doc = imp.json("document.json")?;
        let refs: Vec<String> = doc.get("pages").and_then(Value::as_array).into_iter().flatten().filter_map(|p| p.get("_ref").and_then(Value::as_str)).map(str::to_owned).collect();
        if refs.is_empty() {
            return Err("this file has no pages".into());
        }
        let mut page_info: Vec<(String, Color32)> = vec![];
        for (n, r) in refs.iter().enumerate() {
            let page = imp.json(&format!("{r}.json"))?;
            page_info.push((page.get("name").and_then(Value::as_str).unwrap_or("Page").to_owned(), page.get("backgroundColor").map(color).unwrap_or(crate::theme::SUNKEN())));
            for l in page.get("layers").and_then(Value::as_array).cloned().unwrap_or_default() {
                imp.build(&l, None, n);
            }
        }
        // symbol instances, now that every master exists
        let instances = std::mem::take(&mut imp.instances);
        for (_, sid, parent, pos, size, page) in instances {
            match imp.symbols.get(&sid).copied() {
                Some(main) => {
                    if let Some(id) = imp.app.instantiate(main, parent, pos) {
                        if let Some(i) = imp.app.index_of(id) {
                            imp.app.shapes[i].size = size;
                        }
                        if parent.is_none() {
                            imp.page_of.insert(id, page);
                        }
                    }
                }
                None => imp.skip("instances of a missing symbol"),
            }
        }
        // the layers go to their pages by the top-level layer they sit under
        let shapes = std::mem::take(&mut imp.app.shapes);
        let root_of = |s: &Shape| {
            let mut cur = s;
            while let Some(p) = cur.parent.and_then(|p| shapes.iter().find(|x| x.id == p)) {
                cur = p;
            }
            cur.id
        };
        let mut pages: Vec<Page> = page_info.into_iter().map(|(name, bg)| Page { name, background: bg, ..Page::default() }).collect();
        for s in &shapes {
            let page = imp.page_of.get(&root_of(s)).copied().unwrap_or(0).min(pages.len() - 1);
            pages[page].shapes.push(s.clone());
        }
        imp.summary.pages = pages.len();
        let images = std::mem::take(&mut imp.app.images);
        let summary = std::mem::take(&mut imp.summary);
        Ok((Doc { pages, images, next_id: imp.app.next_id, ..Doc::default() }, summary))
    }

    // File > Import Sketch file: the .sketch becomes the open document (saved next to it as `<name>.json`)
    pub fn import_sketch(&mut self, path: std::path::PathBuf) {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("could not read {}: {e}", path.display());
                return;
            }
        };
        match Self::doc_from_sketch(&bytes) {
            Ok((doc, summary)) => {
                self.open_doc(doc, path.with_extension("json"));
                self.status = summary.text();
            }
            Err(e) => self.status = format!("could not import {}: {e}", path.display()),
        }
    }
}
