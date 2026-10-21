// Editing commands: clipboard, selection, view, alignment, flipping, images.
use std::collections::HashMap;

use base64::Engine;
use egui::{Pos2, Rect, Vec2};

use crate::model::*;
use crate::App;

#[derive(Clone, Copy, PartialEq)]
pub enum AlignOp {
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
}

impl App {
    // ---- colors

    // solid, unstyled colors in the selection and everything inside it, without repeats
    pub fn selection_colors(&self) -> Vec<egui::Color32> {
        let roots = self.sel_roots();
        let mut out: Vec<egui::Color32> = vec![];
        for s in self.shapes.iter().filter(|s| roots.iter().any(|&r| self.is_inside(s.id, r))) {
            for p in s.fills.iter().chain(s.strokes.iter()) {
                if let (PaintKind::Solid(c), None) = (&p.kind, p.style) {
                    if !out.contains(c) && out.len() < 24 {
                        out.push(*c);
                    }
                }
            }
        }
        out
    }

    pub fn recolor(&mut self, from: egui::Color32, to: egui::Color32) {
        let roots = self.sel_roots();
        let ids: Vec<u64> = self.shapes.iter().filter(|s| roots.iter().any(|&r| self.is_inside(s.id, r))).map(|s| s.id).collect();
        for s in self.shapes.iter_mut().filter(|s| ids.contains(&s.id)) {
            for p in s.fills.iter_mut().chain(s.strokes.iter_mut()) {
                if p.style.is_none() && p.kind == PaintKind::Solid(from) {
                    p.kind = PaintKind::Solid(to);
                }
            }
        }
    }

    // ---- clipboard

    // the selection with all descendants, in z order
    pub(crate) fn selection_subtrees(&self) -> Vec<Shape> {
        let roots = self.sel_roots();
        self.shapes.iter().filter(|s| roots.iter().any(|&r| self.is_inside(s.id, r))).cloned().collect()
    }

    // the selection as SVG source on the clipboard (it pastes back as a frame of paths, or into other tools)
    pub fn copy_as_svg(&mut self, ctx: &egui::Context) {
        let ids = self.sel_roots();
        match self.svg_for(ctx, &ids) {
            Some((svg, _)) => {
                ctx.copy_text(svg);
                self.status = "copied as SVG".into();
            }
            None => self.status = "nothing selected to copy as SVG".into(),
        }
    }

    // the selection's CSS (one block per layer, as Dev Mode shows it) on the clipboard
    pub fn copy_as_css(&mut self, ctx: &egui::Context) {
        let blocks: Vec<String> = self.sel_roots().iter().filter_map(|&id| self.index_of(id)).map(|i| format!("/* {} */
{}", self.shapes[i].name, self.css_of(i))).collect();
        if blocks.is_empty() {
            self.status = "nothing selected to copy as CSS".into();
            return;
        }
        ctx.copy_text(blocks.join("

"));
        self.status = "copied as CSS".into();
    }

    // the selection rendered at 2x as an image on the clipboard
    pub fn copy_as_png(&mut self, ctx: &egui::Context) {
        let ids = self.sel_roots();
        let img = self.svg_for(ctx, &ids).and_then(|(svg, _)| crate::export::render_png(&svg, 2.0).ok()).and_then(|png| image::load_from_memory(&png).ok());
        match img {
            Some(img) => {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()));
                self.status = "copied as PNG".into();
            }
            None => self.status = "nothing selected to copy as PNG".into(),
        }
    }

    pub fn copy(&mut self, ctx: &egui::Context) {
        let shapes = self.selection_subtrees();
        if shapes.is_empty() {
            return;
        }
        // the system clipboard carries it too, so it pastes across windows and runs
        if let Ok(json) = serde_json::to_string(&shapes) {
            ctx.copy_text(format!("drui:{json}"));
        }
        self.clipboard = shapes;
        self.paste_n = 0.0;
    }

    pub fn paste(&mut self, text: &str) {
        let shapes = text.strip_prefix("drui:").and_then(|j| serde_json::from_str::<Vec<Shape>>(j).ok());
        match shapes {
            Some(shapes) => {
                self.paste_n += 1.0;
                self.insert_copies(shapes, Vec2::splat(10.0 * self.paste_n));
            }
            None if self.import_svg(text) => {}
            None if !text.trim().is_empty() => {
                // plain text becomes a text layer in the middle of the view
                let at = self.to_world(self.view.min, self.view.center());
                self.add(Kind::Text, at);
                if let Some(i) = self.single() {
                    self.shapes[i].text = text.to_owned();
                }
            }
            None => {}
        }
    }

    // paste what was last copied inside this app (the context menu has no clipboard event to read)
    pub fn paste_internal(&mut self) {
        if !self.clipboard.is_empty() {
            self.paste_n += 1.0;
            let shapes = self.clipboard.clone();
            self.insert_copies(shapes, Vec2::splat(10.0 * self.paste_n));
        }
    }

    pub fn duplicate(&mut self) {
        // like Figma, a duplicated main component is an instance of it (a variant of a set is copied as a variant)
        let roots = self.sel_roots();
        let is_main = |a: &App, id: u64| a.index_of(id).is_some_and(|i| a.shapes[i].component && !a.shapes[i].component_set && a.set_of(i).is_none());
        let mains: Vec<u64> = roots.iter().copied().filter(|&id| is_main(self, id)).collect();
        if mains.is_empty() {
            let shapes = self.selection_subtrees();
            self.insert_copies(shapes, Vec2::splat(10.0));
            return;
        }
        let mut out = vec![];
        for id in mains {
            let Some(i) = self.index_of(id) else { continue };
            let (parent, pos) = (self.shapes[i].parent, self.shapes[i].pos + Vec2::splat(10.0));
            out.extend(self.instantiate(id, parent, pos));
        }
        let others: Vec<u64> = roots.into_iter().filter(|&id| !is_main(self, id)).collect();
        if !others.is_empty() {
            let keep = std::mem::replace(&mut self.sel, others);
            let shapes = self.selection_subtrees();
            self.sel = keep;
            self.insert_copies(shapes, Vec2::splat(10.0));
            out.extend(self.sel.clone());
        }
        self.sel = out;
    }

    // fresh ids for `shapes` (a set closed under "parent", except for its roots).
    // `root_parent` re-homes the roots; roots otherwise keep their parent when it still exists.
    pub fn remap(&mut self, shapes: &[Shape], root_parent: Option<Option<u64>>, d: Vec2) -> (Vec<Shape>, HashMap<u64, u64>) {
        let ids: HashMap<u64, u64> = shapes.iter().map(|s| (s.id, self.new_id())).collect();
        let out = shapes
            .iter()
            .map(|s| {
                let mut c = s.clone();
                c.id = ids[&s.id];
                match s.parent.and_then(|p| ids.get(&p)) {
                    Some(&p) => c.parent = Some(p),
                    None => {
                        c.parent = match root_parent {
                            Some(p) => p,
                            None => s.parent.filter(|&p| self.index_of(p).is_some()),
                        };
                        c.pos += d;
                    }
                }
                c.src = c.src.map(|x| ids.get(&x).copied().unwrap_or(x));
                c.swap = c.swap.map(|x| ids.get(&x).copied().unwrap_or(x));
                c
            })
            .collect();
        (out, ids)
    }

    pub(crate) fn insert_copies(&mut self, shapes: Vec<Shape>, d: Vec2) {
        if shapes.is_empty() {
            return;
        }
        let (copies, _) = self.remap(&shapes, None, d);
        let roots: Vec<u64> = copies.iter().filter(|c| c.parent.is_none_or(|p| !copies.iter().any(|x| x.id == p))).map(|c| c.id).collect();
        self.shapes.extend(copies);
        // like Figma, a copy of "Frame 3" is "Frame 4": the number goes on past the highest one in use
        for &id in &roots {
            if let Some(i) = self.index_of(id) {
                let name = self.numbered_name(&self.shapes[i].name.clone());
                self.shapes[i].name = name;
            }
        }
        self.sel = roots;
    }

    // `name` with its trailing number moved past every layer that shares the stem; names without a number stay as they are
    fn numbered_name(&self, name: &str) -> String {
        let Some((stem, n)) = name.rsplit_once(' ').and_then(|(stem, n)| Some((stem, n.parse::<u32>().ok()?))) else { return name.to_owned() };
        let used = |s: &Shape| s.name.strip_prefix(stem).and_then(|r| r.strip_prefix(' ')).and_then(|n| n.parse::<u32>().ok());
        let top = self.shapes.iter().filter_map(used).max().unwrap_or(n);
        format!("{stem} {}", top + 1)
    }

    // ---- text on a path

    // the outline a text layer runs along, in world space (the first outline of the path layer; closed ones come back to the start)
    pub fn text_path_world(&self, s: &Shape) -> Option<Vec<Pos2>> {
        let pi = self.index_of(s.text_path?)?;
        let p = &self.shapes[pi];
        let o = self.origin_of(p.parent);
        let poly = p.polys(4.0).into_iter().find(|q| !q.hole && q.pts.len() >= 2)?;
        let mut pts: Vec<Pos2> = poly.pts.iter().map(|&q| q + o).collect();
        if poly.closed {
            pts.push(pts[0]);
        }
        Some(pts)
    }

    // Text on path: with one text layer and one other layer selected, the text follows the other layer's outline
    pub fn text_on_path(&mut self) {
        let sel: Vec<usize> = self.sel.iter().filter_map(|&id| self.index_of(id)).collect();
        let text = sel.iter().copied().find(|&i| self.shapes[i].kind == Kind::Text);
        let path = sel.iter().copied().find(|&i| !self.shapes[i].kind.is_container() && self.shapes[i].kind != Kind::Text);
        if let (Some(t), Some(p)) = (text, path) {
            self.shapes[t].text_path = Some(self.shapes[p].id);
            self.shapes[t].fit = TextFit::AutoWidth;
            self.status = "Text on path".into();
        } else {
            self.status = "Select a text layer and a path to put the text on".into();
        }
    }

    pub fn detach_text_path(&mut self) {
        for id in self.sel.clone() {
            if let Some(i) = self.index_of(id) {
                self.shapes[i].text_path = None;
            }
        }
    }

    // ---- selection

    // Ctrl+Shift+A: every sibling of the selection that is not selected
    pub fn select_inverse(&mut self) {
        let Some(parent) = self.sel.first().and_then(|&id| self.index_of(id)).map(|i| self.shapes[i].parent) else { return };
        self.sel = self.shapes.iter().filter(|s| s.parent == parent && s.visible && !s.locked && !self.sel.contains(&s.id)).map(|s| s.id).collect();
    }

    // Tab / Shift+Tab: the next / previous layer inside the same parent (nothing selected: the first top-level layer)
    pub fn select_sibling(&mut self, forward: bool) {
        let parent = self.sel.last().and_then(|&id| self.index_of(id)).map(|i| self.shapes[i].parent).unwrap_or(None);
        let sibs: Vec<u64> = self.shapes.iter().filter(|s| s.parent == parent && s.visible && !s.locked).map(|s| s.id).collect();
        if sibs.is_empty() {
            return;
        }
        let at = self.sel.last().and_then(|id| sibs.iter().position(|s| s == id));
        let k = match (at, forward) {
            (Some(k), true) => (k + 1) % sibs.len(),
            (Some(k), false) => (k + sibs.len() - 1) % sibs.len(),
            (None, true) => 0,
            (None, false) => sibs.len() - 1,
        };
        self.sel = vec![sibs[k]];
    }

    // Shift+Enter: the parent of the selection
    pub fn select_parent(&mut self) {
        let Some(p) = self.single().and_then(|i| self.shapes[i].parent) else { return };
        self.sel = vec![p];
    }

    // N / Shift+N: zoom to the next / previous top-level frame
    pub fn go_to_frame(&mut self, forward: bool) {
        let frames: Vec<u64> = self.shapes.iter().filter(|s| s.kind == Kind::Frame && s.parent.is_none()).map(|s| s.id).collect();
        if frames.is_empty() {
            return;
        }
        let at = self.sel.first().and_then(|id| frames.iter().position(|f| f == id));
        let k = match (at, forward) {
            (Some(k), true) => (k + 1) % frames.len(),
            (Some(k), false) => (k + frames.len() - 1) % frames.len(),
            (None, true) => 0,
            (None, false) => frames.len() - 1,
        };
        self.sel = vec![frames[k]];
        self.zoom_fit(true);
    }

    // Ctrl+Shift+H / Ctrl+Shift+L: hide / lock the selection (all of it hidden or locked already: show / unlock)
    pub fn toggle_hidden_selection(&mut self) {
        let hide = self.sel.iter().filter_map(|&id| self.index_of(id)).any(|i| self.shapes[i].visible);
        for id in self.sel.clone() {
            if let Some(i) = self.index_of(id) {
                self.shapes[i].visible = !hide;
            }
        }
    }

    pub fn toggle_locked_selection(&mut self) {
        let lock = self.sel.iter().filter_map(|&id| self.index_of(id)).any(|i| !self.shapes[i].locked);
        for id in self.sel.clone() {
            if let Some(i) = self.index_of(id) {
                self.shapes[i].locked = lock;
            }
        }
    }

    // Ctrl+Alt+C / Ctrl+Alt+V: copy the look (fills, strokes, effects, opacity, blend, corners) of one layer onto others
    pub fn copy_properties(&mut self) {
        if let Some(i) = self.single() {
            self.props_clip = Some(self.shapes[i].clone());
            self.status = "Copied properties".into();
        }
    }

    pub fn paste_properties(&mut self) {
        let Some(from) = self.props_clip.clone() else { return };
        for id in self.sel.clone() {
            if let Some(i) = self.index_of(id) {
                let s = &mut self.shapes[i];
                s.fills = from.fills.clone();
                s.strokes = from.strokes.clone();
                s.stroke_w = from.stroke_w;
                s.dash = from.dash;
                s.dash_pattern = from.dash_pattern.clone();
                s.stroke_align = from.stroke_align;
                s.stroke_cap = from.stroke_cap;
                s.stroke_join = from.stroke_join;
                s.effects = from.effects.clone();
                s.opacity = from.opacity;
                s.blend = from.blend;
                if s.kind == from.kind {
                    s.radius = from.radius;
                }
            }
        }
    }

    // Ctrl+Shift+V: paste into the selected frame or group, at its top-left corner
    pub fn paste_into_selection(&mut self) {
        let Some(target) = self.single().filter(|&i| self.shapes[i].kind.is_container()).map(|i| self.shapes[i].id) else { return };
        if self.clipboard.is_empty() {
            return;
        }
        let shapes = self.clipboard.clone();
        self.insert_copies(shapes, Vec2::ZERO);
        for id in self.sel.clone() {
            self.reparent(id, Some(target));
        }
    }

    pub fn select_all(&mut self) {
        let parent = self.sel.first().and_then(|&id| self.index_of(id)).and_then(|i| self.shapes[i].parent);
        self.sel = self.shapes.iter().filter(|s| s.parent == parent && s.visible && !s.locked).map(|s| s.id).collect();
    }

    // Figma's "Select matching layers": every visible, unlocked layer on the page that looks like a selected
    // one (same kind, paints, stroke, shadow, opacity, blend, corners and, for text, type), wherever it sits
    pub fn select_matching(&mut self) {
        let look = |s: &Shape| {
            let text = (s.kind == Kind::Text).then(|| (s.font_size.to_bits(), s.mono, s.line_h.to_bits()));
            (s.kind, s.fills.clone(), s.strokes.clone(), s.stroke_w.to_bits(), (s.dash, s.stroke_align, s.stroke_join, s.stroke_cap), s.effects.clone(), s.opacity.to_bits(), s.blend, s.radius.map(f32::to_bits), text)
        };
        let wanted: Vec<_> = self.sel.iter().filter_map(|&id| self.index_of(id)).map(|i| look(&self.shapes[i])).collect();
        if wanted.is_empty() {
            return;
        }
        self.sel = self.shapes.iter().filter(|s| s.visible && !s.locked && wanted.contains(&look(s))).map(|s| s.id).collect();
        self.status = format!("{} matching layer{}", self.sel.len(), if self.sel.len() == 1 { "" } else { "s" });
    }

    // Figma's text shortcuts: `f` changes every selected text layer (and the text inside selected frames)
    pub fn adjust_text(&mut self, f: impl Fn(&mut Shape)) {
        let roots = self.sel_roots();
        let mut n = 0;
        for s in self.shapes.iter_mut().filter(|s| s.kind == Kind::Text && !s.locked) {
            if roots.contains(&s.id) {
                f(s);
                n += 1;
            }
        }
        if n == 0 {
            self.status = "select a text layer first".into();
        }
    }

    pub fn nudge(&mut self, dx: f32, dy: f32) {
        for id in self.sel_roots() {
            if let Some(i) = self.index_of(id).filter(|&i| !self.shapes[i].locked) {
                self.shapes[i].pos += Vec2::new(dx, dy);
            }
        }
    }

    // ---- view

    pub fn zoom_at_center(&mut self, z: f32) {
        let c = self.view.center();
        let w = self.to_world(self.view.min, c);
        self.zoom = z.clamp(0.05, 64.0);
        self.pan = (c - self.view.min) - w.to_vec2() * self.zoom;
    }

    pub fn zoom_fit(&mut self, selection: bool) {
        let rect = if selection && !self.sel.is_empty() {
            self.sel_bounds()
        } else {
            (0..self.shapes.len()).filter(|&i| self.shapes[i].parent.is_none()).map(|i| self.world_rect(i)).reduce(Rect::union)
        };
        let Some(r) = rect else { return };
        let avail = self.view.size() - Vec2::splat(120.0);
        self.zoom = (avail.x / r.width().max(1.0)).min(avail.y / r.height().max(1.0)).clamp(0.05, 8.0);
        self.pan = self.view.size() / 2.0 - r.center().to_vec2() * self.zoom;
    }

    // ---- arranging

    pub fn align(&mut self, op: AlignOp) {
        let roots: Vec<usize> = self.sel_roots().iter().filter_map(|&id| self.index_of(id)).collect();
        let reference = if roots.len() == 1 {
            // a single shape aligns to its parent frame
            match self.shapes[roots[0]].parent.and_then(|p| self.index_of(p)) {
                Some(p) => self.shapes[p].rect().translate(self.origin_of(self.shapes[p].parent)),
                None => return,
            }
        } else {
            match self.sel_bounds() {
                Some(b) => b,
                None => return,
            }
        };
        for i in roots {
            let r = self.world_rect(i);
            let d = match op {
                AlignOp::Left => Vec2::new(reference.min.x - r.min.x, 0.0),
                AlignOp::HCenter => Vec2::new(reference.center().x - r.center().x, 0.0),
                AlignOp::Right => Vec2::new(reference.max.x - r.max.x, 0.0),
                AlignOp::Top => Vec2::new(0.0, reference.min.y - r.min.y),
                AlignOp::VCenter => Vec2::new(0.0, reference.center().y - r.center().y),
                AlignOp::Bottom => Vec2::new(0.0, reference.max.y - r.max.y),
            };
            if !self.shapes[i].locked {
                self.shapes[i].pos += d;
            }
        }
    }

    // equal gaps between three or more shapes, keeping the outermost two where they are
    pub fn distribute(&mut self, horizontal: bool) {
        let mut roots: Vec<(usize, Rect)> = self.sel_roots().iter().filter_map(|&id| self.index_of(id)).map(|i| (i, self.world_rect(i))).collect();
        if roots.len() < 3 {
            return;
        }
        let (lo, hi): (fn(&Rect) -> f32, fn(&Rect) -> f32) = if horizontal { (|r: &Rect| r.min.x, |r: &Rect| r.max.x) } else { (|r: &Rect| r.min.y, |r: &Rect| r.max.y) };
        roots.sort_by(|a, b| lo(&a.1).total_cmp(&lo(&b.1)));
        let span = hi(&roots.iter().map(|r| r.1).reduce(Rect::union).unwrap()) - lo(&roots[0].1);
        let sizes: f32 = roots.iter().map(|(_, r)| hi(r) - lo(r)).sum();
        let gap = (span - sizes) / (roots.len() - 1) as f32;
        let mut cursor = lo(&roots[0].1);
        for (i, r) in roots {
            let d = cursor - lo(&r);
            self.shapes[i].pos += if horizontal { Vec2::new(d, 0.0) } else { Vec2::new(0.0, d) };
            cursor += hi(&r) - lo(&r) + gap;
        }
    }

    // mirrors the selection about its own center; containers mirror their contents too
    pub fn flip(&mut self, horizontal: bool) {
        let Some(b) = self.sel_bounds() else { return };
        let m = if horizontal { b.center().x } else { b.center().y };
        for id in self.sel_roots() {
            let Some(i) = self.index_of(id) else { continue };
            let o = self.origin_of(self.shapes[i].parent);
            let s = &mut self.shapes[i];
            let mir = |v: f32| 2.0 * m - v;
            if s.kind == Kind::Line {
                let (a, e) = (s.pos + o, s.pos + s.size + o);
                let (a, e) = if horizontal { (Pos2::new(mir(a.x), a.y), Pos2::new(mir(e.x), e.y)) } else { (Pos2::new(a.x, mir(a.y)), Pos2::new(e.x, mir(e.y))) };
                s.pos = a - o;
                s.size = e - a;
            } else {
                let r = s.aabb().translate(o);
                let d = if horizontal { Vec2::new(mir(r.max.x) - r.min.x, 0.0) } else { Vec2::new(0.0, mir(r.max.y) - r.min.y) };
                s.pos += d;
            }
            self.flip_inside(id, horizontal);
        }
    }

    // flips a shape's own content in place (path points, rotation) and, for containers, its children
    pub(crate) fn flip_inside(&mut self, id: u64, horizontal: bool) {
        let Some(i) = self.index_of(id) else { return };
        let size = self.shapes[i].size;
        let s = &mut self.shapes[i];
        s.rotation = -s.rotation;
        for c in &mut s.path {
            for q in &mut c.pts {
                if horizontal {
                    q.p.x = 1.0 - q.p.x;
                    q.hin.x = -q.hin.x;
                    q.hout.x = -q.hout.x;
                } else {
                    q.p.y = 1.0 - q.p.y;
                    q.hin.y = -q.hin.y;
                    q.hout.y = -q.hout.y;
                }
            }
        }
        if s.kind == Kind::Ellipse {
            let [a, sw] = s.arc;
            s.arc = if horizontal { [180.0 - a - sw, sw] } else { [-a - sw, sw] };
        }
        let kids: Vec<usize> = (0..self.shapes.len()).filter(|&k| self.shapes[k].parent == Some(id)).collect();
        for k in kids {
            let c = &mut self.shapes[k];
            if c.kind == Kind::Line {
                if horizontal {
                    c.pos.x = size.x - c.pos.x;
                    c.size.x = -c.size.x;
                } else {
                    c.pos.y = size.y - c.pos.y;
                    c.size.y = -c.size.y;
                }
            } else if horizontal {
                c.pos.x = size.x - c.pos.x - c.size.x;
            } else {
                c.pos.y = size.y - c.pos.y - c.size.y;
            }
            let cid = c.id;
            self.flip_inside(cid, horizontal);
        }
    }

    pub fn toggle_mask(&mut self) {
        let on = !self.single().is_some_and(|i| self.shapes[i].mask);
        for id in self.sel_roots() {
            if let Some(i) = self.index_of(id).filter(|&i| !self.shapes[i].kind.is_container()) {
                self.shapes[i].mask = on;
            }
        }
    }

    // ---- images

    // adds the file as an asset and a rectangle filled with it
    pub fn add_image(&mut self, name: &str, bytes: Vec<u8>, at: Option<Pos2>) {
        let Ok(img) = image::load_from_memory(&bytes) else {
            self.status = format!("can't read image {name}");
            return;
        };
        let (w, h) = (img.width() as f32, img.height() as f32);
        let k = (480.0 / w.max(h)).min(1.0);
        let id = self.new_id();
        self.images.push(Asset { id, name: name.to_owned(), data: base64::engine::general_purpose::STANDARD.encode(&bytes) });
        let world = at.unwrap_or_else(|| self.to_world(self.view.min, self.view.center()) - Vec2::new(w * k, h * k) / 2.0);
        self.add(Kind::Rect, world);
        if let Some(i) = self.single() {
            let s = &mut self.shapes[i];
            s.name = name.to_owned();
            s.size = Vec2::new(w * k, h * k);
            s.fills = vec![Paint { kind: PaintKind::Image(id), ..Paint::default() }];
        }
    }

    // adds an image file as an asset without placing it; its id
    pub fn register_image(&mut self, name: &str, bytes: Vec<u8>) -> Option<u64> {
        if image::load_from_memory(&bytes).is_err() {
            self.status = format!("can't read image {name}");
            return None;
        }
        let id = self.new_id();
        self.images.push(Asset { id, name: name.to_owned(), data: base64::engine::general_purpose::STANDARD.encode(&bytes) });
        Some(id)
    }

    // the picker's Video tab: pick a video file, keep it in the document, and make its first frame the paint's image
    pub fn pick_video_for_paint(&mut self, shape: usize, stroke: bool, k: usize) {
        let Some(path) = rfd::FileDialog::new().add_filter("video", &["mp4", "mov", "webm", "mkv", "m4v", "avi", "gif"]).pick_file() else { return };
        let Ok(bytes) = std::fs::read(&path) else { return };
        let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "video".into());
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        self.attach_video(shape, stroke, k, &name, &ext, bytes);
    }

    // the part of `pick_video_for_paint` after the file is read
    pub fn attach_video(&mut self, shape: usize, stroke: bool, k: usize, name: &str, ext: &str, bytes: Vec<u8>) -> bool {
        let (png, _seconds) = match crate::video::poster(&bytes, ext) {
            Ok(p) => p,
            Err(e) => {
                self.status = e;
                return false;
            }
        };
        let Some(poster) = self.register_image(name, png) else { return false };
        let vid = self.new_id();
        self.videos.push(Asset { id: vid, name: name.to_owned(), data: base64::engine::general_purpose::STANDARD.encode(&bytes) });
        let list = if stroke { &mut self.shapes[shape].strokes } else { &mut self.shapes[shape].fills };
        if let Some(p) = list.get_mut(k) {
            p.kind = PaintKind::Image(poster);
            p.video = Some(vid);
        }
        self.status = format!("video {name} added: it plays when you present");
        true
    }

    // "Upload from computer" in the image tab of the picker: pick a file and make it the image of that paint
    pub fn pick_image_for_paint(&mut self, shape: usize, stroke: bool, k: usize) {
        let is_video = (if stroke { &self.shapes[shape].strokes } else { &self.shapes[shape].fills }).get(k).is_some_and(|p| p.video.is_some());
        if is_video {
            return self.pick_video_for_paint(shape, stroke, k);
        }
        let Some(path) = rfd::FileDialog::new().add_filter("image", &["png", "jpg", "jpeg"]).pick_file() else { return };
        let Ok(bytes) = std::fs::read(&path) else { return };
        let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
        let Some(id) = self.register_image(&name, bytes) else { return };
        let list = if stroke { &mut self.shapes[shape].strokes } else { &mut self.shapes[shape].fills };
        if let Some(p) = list.get_mut(k) {
            p.kind = PaintKind::Image(id);
        }
    }

    // (poster image, video) of every visible video fill on the page
    pub(crate) fn video_fills(&self) -> Vec<(u64, u64)> {
        let mut v: Vec<(u64, u64)> = vec![];
        for p in self.shapes.iter().flat_map(|s| s.fills.iter().chain(s.strokes.iter())).filter(|p| p.visible) {
            if let (PaintKind::Image(poster), Some(vid)) = (&p.kind, p.video) {
                if vid != 0 && !v.contains(&(*poster, vid)) {
                    v.push((*poster, vid));
                }
            }
        }
        v
    }

    // present mode: the video fills show the frame at `t` seconds (looping). The layer's texture is swapped for the frame; the
    // clip is cut into frames by ffmpeg in the background, and the poster shows until its first frame is there.
    pub fn update_video_textures(&mut self, ctx: &egui::Context, t: f32) {
        let fills = self.video_fills();
        if fills.is_empty() || crate::video::ffmpeg().is_none() {
            return;
        }
        for (poster, vid) in fills {
            if !self.clips.contains_key(&vid) {
                let Some(bytes) = self.videos.iter().find(|a| a.id == vid).and_then(|a| base64::engine::general_purpose::STANDARD.decode(&a.data).ok()) else { continue };
                let seconds = crate::video::poster(&bytes, "mp4").map_or(0.0, |p| p.1);
                if let Some(c) = crate::video::Clip::start(vid, &bytes, seconds) {
                    self.clips.insert(vid, c);
                }
            }
            let tv = if self.video_ctl.contains_key(&vid) { self.video_pos(vid, ctx.input(|i| i.time)) } else { t };
            let Some(clip) = self.clips.get_mut(&vid) else { continue };
            if let Some((k, path)) = clip.frame_at(tv) {
                if clip.last.as_ref().map(|l| l.0) != Some(k) {
                    if let Some(img) = std::fs::read(&path).ok().and_then(|b| image::load_from_memory(&b).ok()).map(|i| i.to_rgba8()) {
                        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                        clip.last = Some((k, ctx.load_texture(format!("video{vid}"), ci, egui::TextureOptions::LINEAR)));
                    }
                }
                if let Some((_, tex)) = &clip.last {
                    self.tex.insert(poster, tex.clone());
                }
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
    }

    // leaving present mode: the posters show again, and the frames on disk go
    pub fn stop_video_playback(&mut self) {
        if self.clips.is_empty() {
            return;
        }
        for (poster, _) in self.video_fills() {
            self.tex.remove(&poster);
        }
        self.clips.clear();
    }

    // does `s` show a playing video right now (the mesh path draws it, so a new frame is a new texture, not a new picture)?
    pub fn video_playing(&self, s: &Shape) -> bool {
        self.present.is_some() && !self.clips.is_empty() && s.fills.iter().any(|p| p.visible && p.video.is_some_and(|v| self.clips.contains_key(&v)))
    }

    // decode any image asset a fill refers to into a GPU texture, once
    pub fn prepare_textures(&mut self, ctx: &egui::Context) {
        let wanted: Vec<u64> = self
            .shapes
            .iter()
            .flat_map(|s| s.fills.iter().chain(s.strokes.iter()))
            .filter_map(|p| if let PaintKind::Image(id) = p.kind { Some(id) } else { None })
            .chain(self.comments.iter().flat_map(|c| c.attachments.iter().copied().chain(c.replies.iter().flat_map(|r| r.attachments.iter().copied()))))
            .filter(|id| !self.tex.contains_key(id))
            .collect();
        for id in wanted {
            let Some(a) = self.images.iter().find(|a| a.id == id) else { continue };
            let img = base64::engine::general_purpose::STANDARD
                .decode(&a.data)
                .ok()
                .and_then(|b| image::load_from_memory(&b).ok())
                .map(|i| i.to_rgba8());
            let tex = match img {
                Some(i) => {
                    let ci = egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw());
                    ctx.load_texture(format!("img{id}"), ci, egui::TextureOptions::LINEAR)
                }
                None => ctx.load_texture(format!("img{id}"), egui::ColorImage::filled([1, 1], egui::Color32::GRAY), egui::TextureOptions::LINEAR),
            };
            self.tex.insert(id, tex);
        }
    }
}
