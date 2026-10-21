// Draw's non-destructive transforms: a layer drawn again and again along a step (linear) or round a pivot (radial).
// The copies exist only when drawing and exporting; "Apply transforms" turns them into real layers.
use egui::Vec2;

use crate::fig::{self, Grid, Num, Prefix};
use crate::glyphs::Glyph;
use crate::model::*;
use crate::App;

impl Repeat {
    pub fn linear() -> Self {
        Repeat { kind: 0, count: 5, step: Vec2::new(40.0, 0.0), angle: 30.0, pivot: Vec2::ZERO, spin: true }
    }
}

impl Shape {
    // the layer with its repeat applied: itself first, then each copy with `repeat` cleared
    pub fn repeat_copies(&self) -> Vec<Shape> {
        let Some(r) = self.repeat.as_ref().filter(|_| !self.kind.is_container()) else { return vec![self.clone()] };
        let mut base = self.clone();
        base.repeat = None;
        let center = self.pos + self.size / 2.0;
        let pivot = center + r.pivot;
        (0..r.count.clamp(1, 200))
            .map(|k| {
                let mut s = base.clone();
                let k = k as f32;
                if r.kind == 0 {
                    s.pos += r.step * k;
                } else {
                    let a = (r.angle * k).to_radians();
                    let (sn, cs) = a.sin_cos();
                    let d = center - pivot;
                    let c = pivot + Vec2::new(d.x * cs - d.y * sn, d.x * sn + d.y * cs);
                    s.pos = c - self.size / 2.0;
                    if r.spin {
                        s.rotation += r.angle * k;
                    }
                }
                s
            })
            .collect()
    }
}

impl App {
    // Apply transforms to selection: the copies become layers of their own
    pub fn apply_repeat(&mut self) {
        let mut made = vec![];
        for id in self.sel_roots() {
            let Some(i) = self.index_of(id).filter(|&i| self.shapes[i].repeat.is_some()) else { continue };
            let copies = self.shapes[i].repeat_copies();
            self.shapes[i].repeat = None;
            let (parent, name) = (self.shapes[i].parent, self.shapes[i].name.clone());
            made.push(id);
            self.shapes[i].pos = copies[0].pos;
            self.shapes[i].rotation = copies[0].rotation;
            for c in copies.into_iter().skip(1) {
                let nid = self.new_id();
                let mut s = c;
                s.id = nid;
                s.parent = parent;
                s.name = name.clone();
                self.shapes.insert(i + 1, s);
                made.push(nid);
            }
        }
        if !made.is_empty() {
            self.sel = made;
        }
    }

    pub(crate) fn repeat_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        if self.shapes[i].kind.is_container() {
            return;
        }
        let id = self.shapes[i].id;
        let has = self.shapes[i].repeat.is_some();
        let (_, b) = fig::header(ui, "Repeat", !has, &[(if has { Glyph::Minus } else { Glyph::Plus }, if has { "Remove repeat" } else { "Repeat this layer" }, false)]);
        if b[0].clicked() {
            self.shapes[i].repeat = if has { None } else { Some(Repeat::linear()) };
            return;
        }
        let Some(r) = self.shapes[i].repeat.as_mut() else { return };
        let mut radial = r.kind == 1;
        let row = fig::controls(ui, 0.0);
        if let Some(k) = fig::text_segments(ui, g.full(row), &[("Linear", !radial, true), ("Radial", radial, true)]) {
            r.kind = k as u8;
            radial = k == 1;
        }
        fig::labels(ui, g, false, "Count", if radial { "Angle" } else { "Step X" });
        let row = fig::controls(ui, 0.0);
        let mut n = r.count as f32;
        if fig::num_field(ui, g.col1(row), ui.id().with(("rep-n", id)), Prefix::None, &mut n, Num { min: 1.0, max: 200.0, decimals: 0, ..Num::default() }, "Repeat count") {
            r.count = n as u32;
        }
        if radial {
            fig::num_field(ui, g.col2(row), ui.id().with(("rep-a", id)), Prefix::Icon(Glyph::RotateCw), &mut r.angle, Num { decimals: 1, suffix: "°", ..Num::default() }, "Angle between copies");
            fig::labels(ui, g, false, "Pivot X", "Pivot Y");
            let row = fig::controls(ui, 0.0);
            fig::num_field(ui, g.col1(row), ui.id().with(("rep-px", id)), Prefix::Text("X"), &mut r.pivot.x, Num { decimals: 1, ..Num::default() }, "Pivot X from the layer's center");
            fig::num_field(ui, g.col2(row), ui.id().with(("rep-py", id)), Prefix::Text("Y"), &mut r.pivot.y, Num { decimals: 1, ..Num::default() }, "Pivot Y from the layer's center");
            let row = fig::controls(ui, 4.0);
            fig::checkbox(ui, g.full(row), &mut r.spin, "Rotate copies");
        } else {
            fig::num_field(ui, g.col2(row), ui.id().with(("rep-sx", id)), Prefix::Text("X"), &mut r.step.x, Num { decimals: 1, ..Num::default() }, "Step X");
            fig::labels(ui, g, false, "Step Y", "");
            let row = fig::controls(ui, 0.0);
            fig::num_field(ui, g.col1(row), ui.id().with(("rep-sy", id)), Prefix::Text("Y"), &mut r.step.y, Num { decimals: 1, ..Num::default() }, "Step Y");
        }
        let row = fig::controls(ui, 8.0);
        if fig::button(ui, g.full(row), "Apply transforms to selection").clicked() {
            self.apply_repeat();
        }
    }
}

// ---- layer brushes: "Create brush" keeps the selected layers, a stroke with the Layer brush scatters copies of them

impl App {
    pub fn create_brush_from_selection(&mut self) {
        let shapes = self.selection_subtrees();
        if shapes.is_empty() {
            self.status = "select the layers to make a brush from".into();
            return;
        }
        self.custom_brush = Some(shapes);
        self.brush = crate::vector::Brush::Layer;
        self.set_tool(crate::Tool::Pencil);
        self.status = "brush created: draw with the pencil (stroke width sets the size)".into();
    }

    pub fn finish_layer_brush(&mut self, raw: &[egui::Pos2]) {
        use egui::{Pos2, Rect};
        let Some(src) = self.custom_brush.clone() else {
            self.status = "Layer brush: select layers and run Create brush first".into();
            return;
        };
        let is_root = |s: &Shape| s.parent.is_none_or(|p| !src.iter().any(|x| x.id == p));
        let Some(bb) = src.iter().filter(|s| is_root(s)).map(|s| Rect::from_min_size(s.pos, s.size)).reduce(|a, b| a.union(b)) else { return };
        let single_leaf = src.iter().filter(|s| is_root(s)).count() == 1 && src.iter().all(|s| !s.kind.is_container());
        let longest = bb.width().max(bb.height()).max(1.0);
        let scale = if single_leaf { (self.brush_w * 4.0 / longest).clamp(0.05, 10.0) } else { 1.0 };
        let stamp = longest * scale;
        let parent = self.frame_at(raw[0], &[]);
        let o = self.origin_of(parent);
        // a small deterministic hash, so the same stroke always lays the same copies
        let rand = |k: u32| {
            let mut x = k.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
            x ^= x >> 15;
            x = x.wrapping_mul(0x2C1B_3C6D);
            x ^= x >> 12;
            (x % 10_000) as f32 / 10_000.0
        };
        let mut points: Vec<Pos2> = vec![];
        let (mut next, mut along) = (0.0f32, 0.0f32);
        for w in raw.windows(2) {
            let (seg, len) = (w[1] - w[0], (w[1] - w[0]).length());
            while len > 1e-6 && next <= along + len {
                points.push(w[0] + seg * ((next - along) / len));
                next += stamp * (0.8 + 0.4 * rand(points.len() as u32 + 7));
            }
            along += len;
        }
        let mut roots = vec![];
        for (k, p) in points.into_iter().enumerate() {
            let (mut copies, _) = self.remap(&src, Some(parent), egui::Vec2::ZERO);
            let jitter = if single_leaf { 0.8 + 0.4 * rand(k as u32 * 5 + 1) } else { 1.0 };
            for c in copies.iter_mut().filter(|c| c.parent == parent) {
                c.pos = p - o + (c.pos - bb.center()) * scale * jitter;
                c.size *= scale * jitter;
                if single_leaf {
                    c.rotation += (rand(k as u32 * 5 + 2) - 0.5) * 40.0;
                }
                roots.push(c.id);
            }
            self.shapes.extend(copies);
        }
        if roots.is_empty() {
            return;
        }
        self.sel = roots;
        self.group();
        if let Some(i) = self.single() {
            self.shapes[i].name = format!("Brush {}", self.shapes.len());
        }
    }
}
