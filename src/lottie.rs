// Lottie export: the page's animation as a Lottie JSON (the format of LottieFiles and the After Effects exporter). The page is
// evaluated at every frame (so tracks, easing, arcs and scale are all in the samples) and each layer becomes a shape layer:
// rectangles, ellipses and paths with their first fill and stroke, positions, rotation, opacity, size, radius and colour as
// keyframes. Text is flattened to outlines at the first frame (Lottie needs embedded fonts), images are skipped.
use std::collections::HashMap;

use egui::{Pos2, Rect};
use serde_json::{json, Value};

use crate::model::*;
use crate::App;

const FPS: f32 = 30.0;

// a property over the frames as a Lottie value: static when it never changes, else one keyframe per frame
fn animated<const N: usize>(frames: &[[f32; N]]) -> Value {
    let first = frames[0];
    if frames.iter().all(|f| f.iter().zip(&first).all(|(a, b)| (a - b).abs() < 1e-4)) {
        return json!({ "a": 0, "k": if N == 1 { json!(first[0]) } else { json!(first.to_vec()) } });
    }
    let keys: Vec<Value> = frames
        .iter()
        .enumerate()
        .map(|(t, v)| {
            let mut k = json!({ "t": t, "s": v.to_vec() });
            if t + 1 < frames.len() {
                k["i"] = json!({ "x": [1.0], "y": [1.0] });
                k["o"] = json!({ "x": [0.0], "y": [0.0] });
            }
            k
        })
        .collect();
    json!({ "a": 1, "k": keys })
}

fn rgba(c: egui::Color32) -> [f32; 4] {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0]
}

fn world_pos(ev: &[Shape], index: &HashMap<u64, usize>, i: usize) -> Pos2 {
    let mut p = ev[i].pos;
    let mut parent = ev[i].parent;
    while let Some(pid) = parent {
        let Some(&pi) = index.get(&pid) else { break };
        p += ev[pi].pos.to_vec2();
        parent = ev[pi].parent;
    }
    p
}

// a contour as a Lottie bezier shape (vertices, and the tangents in and out as offsets)
fn bezier(c: &Contour, origin: Pos2) -> Value {
    let pt = |p: Pos2| vec![p.x - origin.x, p.y - origin.y];
    json!({
        "c": c.closed,
        "v": c.pts.iter().map(|q| pt(q.p)).collect::<Vec<_>>(),
        "i": c.pts.iter().map(|q| vec![q.hin.x, q.hin.y]).collect::<Vec<_>>(),
        "o": c.pts.iter().map(|q| vec![q.hout.x, q.hout.y]).collect::<Vec<_>>(),
    })
}

impl App {
    pub fn lottie_json(&self, ctx: &egui::Context, roots: &[u64]) -> Option<String> {
        let rects: Vec<Rect> = roots.iter().filter_map(|&r| self.index_of(r)).map(|i| self.world_rect(i)).collect();
        let area = rects.into_iter().reduce(|a, b| a.union(b))?;
        let (len_ms, looping) = (self.anim_len(), self.pages.get(self.cur).is_some_and(|p| p.anim_loop));
        let n_frames = ((len_ms / 1000.0 * FPS).ceil() as usize).max(2);
        // the page at each frame
        let frames: Vec<Vec<Shape>> = (0..n_frames).map(|f| crate::anim::evaluate(&self.shapes, f as f32 / FPS * 1000.0)).collect();
        let index: HashMap<u64, usize> = self.shapes.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        let mut layers: Vec<Value> = vec![];
        for i in self.draw_order() {
            let s = &self.shapes[i];
            if !roots.iter().any(|&r| self.is_inside(s.id, r)) || s.slice || s.mask || !s.visible || self.chain(i).iter().any(|&c| !self.shapes[c].visible) {
                continue;
            }
            if matches!(s.kind, Kind::Group) {
                continue; // a group draws only through its layers
            }
            let sample = |f: &dyn Fn(&Shape, Pos2) -> Vec<f32>| -> Vec<Vec<f32>> { frames.iter().map(|ev| f(&ev[i], world_pos(ev, &index, i) - area.min.to_vec2())).collect() };
            let to1 = |v: Vec<Vec<f32>>| -> Vec<[f32; 1]> { v.into_iter().map(|x| [x[0]]).collect() };
            let to2 = |v: Vec<Vec<f32>>| -> Vec<[f32; 2]> { v.into_iter().map(|x| [x[0], x[1]]).collect() };
            // geometry
            let flat = !matches!(s.kind, Kind::Rect | Kind::Frame | Kind::Ellipse);
            let (geom, origin_at_center): (Vec<Value>, bool) = match s.kind {
                Kind::Rect | Kind::Frame => {
                    let size = to2(sample(&|sh, _| vec![sh.size.x, sh.size.y]));
                    let radius = to1(sample(&|sh, _| vec![sh.radius[0]]));
                    (vec![json!({ "ty": "rc", "nm": "Rectangle", "p": { "a": 0, "k": [0, 0] }, "s": animated(&size), "r": animated(&radius) })], true)
                }
                Kind::Ellipse if s.arc[1].abs() >= 359.0 || s.arc[1] == 0.0 => {
                    let size = to2(sample(&|sh, _| vec![sh.size.x, sh.size.y]));
                    (vec![json!({ "ty": "el", "nm": "Ellipse", "p": { "a": 0, "k": [0, 0] }, "s": animated(&size) })], true)
                }
                Kind::Text => {
                    // flattened at the first frame: the outlines of an SVG made from the layer
                    let Some((svg, _)) = self.svg_for(ctx, &[s.id]) else { continue };
                    let opt = resvg::usvg::Options { fontdb: crate::export::fonts(), ..resvg::usvg::Options::default() };
                    let Ok(tree) = resvg::usvg::Tree::from_str(&svg, &opt) else { continue };
                    let mut items = vec![];
                    collect_paths(tree.root(), &mut items);
                    (items, false)
                }
                _ => {
                    // other shapes as the outline they draw now, around the layer's centre
                    let centre = s.center();
                    let items: Vec<Value> = s.polys(8.0).iter().map(|p| {
                        let pts: Vec<Pos2> = p.pts.iter().map(|q| Pos2::new(q.x - centre.x, q.y - centre.y)).collect();
                        let c = Contour { pts: pts.into_iter().map(PathPt::corner).collect(), closed: p.closed, hole: false };
                        json!({ "ty": "sh", "nm": "Path", "ks": { "a": 0, "k": bezier(&c, Pos2::ZERO) } })
                    }).collect();
                    (items, true)
                }
            };
            if geom.is_empty() {
                continue;
            }
            let _ = flat;
            // paints
            let mut items = geom;
            if let Some(f) = s.fills.iter().rev().find(|f| f.visible).filter(|_| s.has_area() || s.kind == Kind::Text) {
                items.push(paint_item(f, "fl", s.fills.iter().rposition(|x| std::ptr::eq(x, f)).unwrap_or(0)));
            }
            if s.stroke_w > 0.0 && s.kind != Kind::Text {
                if let Some(st) = s.strokes.iter().rev().find(|p| p.visible) {
                    let mut item = paint_item(st, "st", 0);
                    item["w"] = json!({ "a": 0, "k": s.stroke_w });
                    item["lc"] = json!(match s.stroke_cap[0] { StrokeCap::Round => 2, StrokeCap::Square => 3, _ => 1 });
                    item["lj"] = json!(match s.stroke_join { StrokeJoin::Round => 2, StrokeJoin::Bevel => 3, StrokeJoin::Miter => 1 });
                    items.push(item);
                }
            }
            // the colours of a solid fill are keyframed too
            if let Some(Value::Object(fill)) = items.iter_mut().find(|v| v["ty"] == "fl") {
                if matches!(s.fills.iter().rev().find(|f| f.visible).map(|f| &f.kind), Some(PaintKind::Solid(_))) {
                    let cols = frames.iter().map(|ev| ev[i].fills.iter().rev().find(|f| f.visible).map_or([0.0; 4], |f| if let PaintKind::Solid(c) = f.kind { rgba(c) } else { [0.0; 4] })).collect::<Vec<[f32; 4]>>();
                    fill.insert("c".into(), animated(&cols.iter().map(|c| [c[0], c[1], c[2], 1.0]).collect::<Vec<_>>()));
                }
            }
            items.push(json!({ "ty": "tr", "p": { "a": 0, "k": [0, 0] }, "a": { "a": 0, "k": [0, 0] }, "s": { "a": 0, "k": [100, 100] }, "r": { "a": 0, "k": 0 }, "o": { "a": 0, "k": 100 } }));
            // the layer's own transform
            let pos = if origin_at_center {
                to2(sample(&|sh, wp| { let c = wp + sh.size / 2.0; vec![c.x, c.y] }))
            } else {
                to2(sample(&|_, wp| vec![wp.x, wp.y]))
            };
            let rot = to1(sample(&|sh, _| vec![if flat { 0.0 } else { sh.rotation }]));
            let op = to1(sample(&|sh, _| vec![sh.opacity * 100.0]));
            layers.push(json!({
                "ddd": 0, "ind": layers.len() + 1, "ty": 4, "nm": s.name, "sr": 1, "ao": 0, "ip": 0, "op": n_frames, "st": 0,
                "ks": { "o": animated(&op), "r": animated(&rot), "p": animated(&pos), "a": { "a": 0, "k": [0, 0, 0] }, "s": { "a": 0, "k": [100, 100, 100] } },
                "shapes": [json!({ "ty": "gr", "nm": s.name, "it": items })],
            }));
        }
        if layers.is_empty() {
            return None;
        }
        // Lottie draws the first layer on top
        layers.reverse();
        for (n, l) in layers.iter_mut().enumerate() {
            l["ind"] = json!(n + 1);
        }
        let doc = json!({
            "v": "5.7.0", "fr": FPS, "ip": 0, "op": n_frames, "w": area.width().round(), "h": area.height().round(), "ddd": 0,
            "nm": roots.first().and_then(|&r| self.index_of(r)).map_or("drui".to_owned(), |i| self.shapes[i].name.clone()),
            "assets": [], "layers": layers, "meta": { "g": "drui", "loop": looping },
        });
        serde_json::to_string(&doc).ok()
    }
}

// the first (non-flattened) fill or stroke of a paint as a Lottie item: solid, or a linear / radial gradient
fn paint_item(p: &Paint, ty: &str, _n: usize) -> Value {
    let k = p.opacity * 100.0;
    match &p.kind {
        PaintKind::Gradient(g) if matches!(g.kind, GradientKind::Linear | GradientKind::Radial) => {
            let (lin, a) = (g.kind == GradientKind::Linear, g.angle.to_radians());
            // positions are in the layer's space around its centre; the box size is not known here, so a unit-box gradient
            // scaled by 100 is used
            let (start, end) = if lin { ([-50.0 * a.cos(), -50.0 * a.sin()], [50.0 * a.cos(), 50.0 * a.sin()]) } else { ([0.0, 0.0], [50.0, 0.0]) };
            let mut flat = vec![];
            for s in &g.stops {
                let [r, gg, b, _] = rgba(s.color);
                flat.extend([s.pos, r, gg, b]);
            }
            json!({ "ty": if ty == "fl" { "gf" } else { "gs" }, "t": if lin { 1 } else { 2 }, "s": { "a": 0, "k": start }, "e": { "a": 0, "k": end }, "g": { "p": g.stops.len(), "k": { "a": 0, "k": flat } }, "o": { "a": 0, "k": k }, "r": 1 })
        }
        _ => {
            let c = match &p.kind {
                PaintKind::Solid(c) => *c,
                PaintKind::Linear { a, .. } => *a,
                PaintKind::Gradient(g) => g.stops.first().map_or(egui::Color32::GRAY, |s| s.color),
                PaintKind::Shader(s) => s.a,
                _ => egui::Color32::GRAY,
            };
            let [r, g, b, a] = rgba(c);
            json!({ "ty": ty, "c": { "a": 0, "k": [r, g, b, 1] }, "o": { "a": 0, "k": k * a }, "r": 1 })
        }
    }
}

// the paths of a usvg tree as Lottie shape items (with their fill and stroke), in the tree's own coordinates
fn collect_paths(group: &resvg::usvg::Group, out: &mut Vec<Value>) {
    use resvg::usvg;
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect_paths(g, out),
            usvg::Node::Text(t) => collect_paths(t.flattened(), out),
            usvg::Node::Path(p) => {
                for c in crate::svg_import::contours(p) {
                    out.push(json!({ "ty": "sh", "nm": "Path", "ks": { "a": 0, "k": bezier(&c, Pos2::ZERO) } }));
                }
            }
            _ => {}
        }
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
pub fn crc32_for_test(data: &[u8]) -> u32 {
    crc32(data)
}

// a zip file holding `files` uncompressed (the dotLottie container needs nothing more)
pub fn zip_stored(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out: Vec<u8> = vec![];
    let mut central: Vec<u8> = vec![];
    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let head = |v: &mut Vec<u8>, central: bool| {
            v.extend(if central { [0x50, 0x4b, 0x01, 0x02] } else { [0x50, 0x4b, 0x03, 0x04] });
            if central {
                v.extend(20u16.to_le_bytes()); // made by
            }
            v.extend(20u16.to_le_bytes()); // needed to extract
            v.extend(0u16.to_le_bytes()); // flags
            v.extend(0u16.to_le_bytes()); // stored
            v.extend(0u16.to_le_bytes()); // time
            v.extend(0x21u16.to_le_bytes()); // date: 1980-01-01
            v.extend(crc.to_le_bytes());
            v.extend((data.len() as u32).to_le_bytes());
            v.extend((data.len() as u32).to_le_bytes());
            v.extend((name.len() as u16).to_le_bytes());
            v.extend(0u16.to_le_bytes()); // extra
            if central {
                v.extend([0u8; 6]); // comment length, disk number, internal attributes
                v.extend(0u32.to_le_bytes()); // external attributes
                v.extend(offset.to_le_bytes());
            }
            v.extend(name.as_bytes());
        };
        head(&mut out, false);
        out.extend(*data);
        head(&mut central, true);
    }
    let (start, size) = (out.len() as u32, central.len() as u32);
    out.extend(central);
    out.extend([0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend(size.to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

impl App {
    // the animation as a dotLottie container: a manifest and the Lottie JSON as the one animation
    pub fn dotlottie(&self, ctx: &egui::Context, roots: &[u64]) -> Option<Vec<u8>> {
        let json = self.lottie_json(ctx, roots)?;
        let manifest = r#"{"version":"1","generator":"drui","animations":[{"id":"animation"}]}"#;
        Some(zip_stored(&[("manifest.json", manifest.as_bytes()), ("animations/animation.json", json.as_bytes())]))
    }
}
