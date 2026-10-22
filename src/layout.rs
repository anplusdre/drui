// Automatic geometry: groups hugging their children, constraints, auto layout, text fitting,
// snapping and scaling.
use egui::{Painter, Pos2, Rect, Vec2};

use crate::geom::rot_vec;
use crate::model::*;
use crate::render::{text_galley, wrap_of};
use crate::App;

fn set(dst: &mut f32, v: f32) -> bool {
    if (*dst - v).abs() > 1e-3 {
        *dst = v;
        true
    } else {
        false
    }
}

fn axis_get(v: Vec2, axis: usize) -> f32 {
    if axis == 0 { v.x } else { v.y }
}

fn axis_set(v: &mut Vec2, axis: usize, x: f32) -> bool {
    set(if axis == 0 { &mut v.x } else { &mut v.y }, x)
}

fn constrain(con: Constraint, pos: &mut f32, size: &mut f32, old: f32, new: f32) {
    match con {
        Constraint::Min => {}
        Constraint::Max => *pos += new - old,
        Constraint::Both => *size += new - old,
        Constraint::Center => *pos += (new - old) / 2.0,
        Constraint::Scale => {
            if old > 1e-6 {
                let r = new / old;
                *pos *= r;
                *size *= r;
            }
        }
    }
}

impl App {
    // brings every derived size and position up to date; cheap when nothing changed
    pub fn settle(&mut self) {
        for _ in 0..3 {
            let v = self.resolve_variables() | self.resolve_component_props() | self.apply_ext_styles();
            let a = self.apply_constraints();
            let b = self.run_layouts();
            let c = self.refit_groups();
            if !(v || a || b || c) {
                break;
            }
        }
        self.place_notes();
    }

    // layers that follow an effect, text or grid style take its values; a style that no longer exists is let go
    pub fn apply_ext_styles(&mut self) -> bool {
        let mut changed = false;
        let ext = self.ext.clone();
        for s in self.shapes.iter_mut() {
            if let Some(id) = s.effect_style {
                match ext.effects.iter().find(|e| e.id == id) {
                    Some(st) if s.effects != st.effects => {
                        s.effects = st.effects.clone();
                        changed = true;
                    }
                    Some(_) => {}
                    None => s.effect_style = None,
                }
            }
            if let Some(id) = s.grid_style {
                match ext.grids.iter().find(|e| e.id == id) {
                    Some(st) if s.grids != st.grids => {
                        s.grids = st.grids.clone();
                        changed = true;
                    }
                    Some(_) => {}
                    None => s.grid_style = None,
                }
            }
            if let Some(id) = s.text_style {
                match ext.texts.iter().find(|e| e.id == id) {
                    Some(st) => {
                        let now = (s.font_size, s.bold, s.mono, s.tracking, s.line_h, s.align, s.valign);
                        if now != (st.font_size, st.bold, st.mono, st.tracking, st.line_h, st.align, st.valign) || TextExtra::of(s) != st.extra {
                            (s.font_size, s.bold, s.mono, s.tracking, s.line_h, s.align, s.valign) = (st.font_size, st.bold, st.mono, st.tracking, st.line_h, st.align, st.valign);
                            st.extra.apply(s);
                            changed = true;
                        }
                    }
                    None => s.text_style = None,
                }
            }
        }
        changed
    }

    // keeps every group's box tight around its children (moving the origin if needed, children
    // keep their world position) and drops groups that lost all their children. Deepest first.
    pub fn refit_groups(&mut self) -> bool {
        let mut groups: Vec<(usize, u64)> = (0..self.shapes.len())
            .filter(|&i| self.shapes[i].kind == Kind::Group)
            .map(|i| (self.chain(i).len(), self.shapes[i].id))
            .collect();
        groups.sort_by_key(|&(depth, _)| std::cmp::Reverse(depth));
        let mut changed = false;
        for (_, gid) in groups {
            let Some(g) = self.index_of(gid) else { continue };
            let kids: Vec<usize> = (0..self.shapes.len()).filter(|&k| self.shapes[k].parent == Some(gid)).collect();
            let Some(bb) = kids.iter().map(|&k| self.shapes[k].aabb()).reduce(Rect::union) else {
                self.shapes.remove(g);
                changed = true;
                continue;
            };
            let d = bb.min.to_vec2();
            if d.abs().max_elem() > 1e-3 || (bb.size() - self.shapes[g].size).abs().max_elem() > 1e-3 {
                for &k in &kids {
                    self.shapes[k].pos -= d;
                }
                self.shapes[g].pos += d;
                self.shapes[g].size = bb.size();
                changed = true;
            }
        }
        self.prune_sel();
        changed
    }

    // when a frame changes size, its children follow their constraints
    fn apply_constraints(&mut self) -> bool {
        let mut changed = false;
        for i in 0..self.shapes.len() {
            if self.shapes[i].kind != Kind::Frame {
                continue;
            }
            let (id, new) = (self.shapes[i].id, self.shapes[i].size);
            let Some(old) = self.sizes.insert(id, new) else { continue };
            if (new - old).abs().max_elem() < 1e-4 {
                continue;
            }
            let flow = self.shapes[i].layout.is_some();
            for k in 0..self.shapes.len() {
                // the children of an auto-layout frame are placed by the layout, unless they ignore it
                if self.shapes[k].parent != Some(id) || (flow && !self.shapes[k].abs) {
                    continue;
                }
                let c = &mut self.shapes[k];
                if c.kind != Kind::Line {
                    constrain(c.ch, &mut c.pos.x, &mut c.size.x, old.x, new.x);
                    constrain(c.cv, &mut c.pos.y, &mut c.size.y, old.y, new.y);
                    changed = true;
                }
            }
        }
        changed
    }

    // lays out the children of every auto-layout frame, innermost first
    fn run_layouts(&mut self) -> bool {
        let mut frames: Vec<(usize, u64)> = (0..self.shapes.len())
            .filter(|&i| self.shapes[i].kind == Kind::Frame && self.shapes[i].layout.is_some())
            .map(|i| (self.chain(i).len(), self.shapes[i].id))
            .collect();
        frames.sort_by_key(|&(depth, _)| std::cmp::Reverse(depth));
        let mut changed = false;
        for (_, fid) in frames {
            let Some(fi) = self.index_of(fid) else { continue };
            let Some(mut lay) = self.shapes[fi].layout.clone() else { continue };
            // Include stroke in layout: the part of the frame's own stroke that lies inside counts as padding
            if lay.stroke_in_layout && self.shapes[fi].strokes.iter().any(|p| p.visible) {
                let w = self.shapes[fi].stroke_w;
                let inside = match self.shapes[fi].stroke_align {
                    StrokeAlign::Inside => w,
                    StrokeAlign::Center => w / 2.0,
                    StrokeAlign::Outside => 0.0,
                };
                lay.pad = lay.pad.map(|p| p + inside);
            }
            let kids: Vec<usize> = (0..self.shapes.len()).filter(|&k| self.shapes[k].parent == Some(fid) && self.shapes[k].kind != Kind::Line && !self.shapes[k].abs).collect();
            if kids.is_empty() {
                continue;
            }
            if lay.columns > 0 {
                changed |= self.layout_grid(fi, &kids, &lay);
                continue;
            }
            if lay.wrap && !(if lay.vertical { lay.hug_y } else { lay.hug_x }) {
                changed |= self.layout_wrap(fi, &kids, &lay);
                continue;
            }
            let (m, c) = if lay.vertical { (1, 0) } else { (0, 1) };
            let pad_main = if lay.vertical { lay.pad[1] + lay.pad[3] } else { lay.pad[0] + lay.pad[2] };
            let pad_cross = if lay.vertical { lay.pad[0] + lay.pad[2] } else { lay.pad[1] + lay.pad[3] };
            let start_main = if lay.vertical { lay.pad[1] } else { lay.pad[0] };
            let start_cross = if lay.vertical { lay.pad[0] } else { lay.pad[1] };
            let (hug_main, hug_cross) = if lay.vertical { (lay.hug_y, lay.hug_x) } else { (lay.hug_x, lay.hug_y) };
            let fills = |s: &Shape| if lay.vertical { (s.fill_y, s.fill_x) } else { (s.fill_x, s.fill_y) };
            let frame_size = self.shapes[fi].size;

            let gaps = lay.gap * (kids.len() - 1) as f32;
            let (mut fixed, mut n_fill) = (0.0, 0);
            for &k in &kids {
                if fills(&self.shapes[k]).0 && !hug_main {
                    n_fill += 1;
                } else {
                    fixed += axis_get(self.shapes[k].size, m);
                }
            }
            let inner_main = if hug_main { fixed + gaps } else { (axis_get(frame_size, m) - pad_main).max(0.0) };
            let fill_each = if n_fill > 0 { ((inner_main - fixed - gaps) / n_fill as f32).max(0.0) } else { 0.0 };
            let free = (inner_main - fixed - n_fill as f32 * fill_each - gaps).max(0.0);
            let (lead, gap) = match lay.main {
                Align::Start => (0.0, lay.gap),
                Align::Center => (free / 2.0, lay.gap),
                Align::End => (free, lay.gap),
                Align::SpaceBetween => (0.0, if kids.len() > 1 { lay.gap + free / (kids.len() - 1) as f32 } else { lay.gap }),
            };
            let baseline_on = lay.baseline && !lay.vertical;
            let base_of = |s: &Shape| if s.kind == Kind::Text { s.font_size * 0.8 } else { s.size.y };
            let top_base = if baseline_on { kids.iter().map(|&k| base_of(&self.shapes[k])).fold(0.0, f32::max) } else { 0.0 };
            // with baselines the row is as tall as the tallest child's part below the baseline plus the lifted part above it
            let max_cross = if baseline_on {
                kids.iter().map(|&k| top_base - base_of(&self.shapes[k]) + self.shapes[k].size.y).fold(0.0, f32::max)
            } else {
                kids.iter().filter(|&&k| !fills(&self.shapes[k]).1).map(|&k| axis_get(self.shapes[k].size, c)).fold(0.0, f32::max)
            };
            let inner_cross = if hug_cross { max_cross } else { (axis_get(frame_size, c) - pad_cross).max(0.0) };

            // text baselines: every child is moved down by how far its baseline is above the lowest one
            let baseline = lay.baseline && !lay.vertical;
            let base_of = |s: &Shape| if s.kind == Kind::Text { s.font_size * 0.8 } else { s.size.y };
            let max_base = if baseline { kids.iter().map(|&k| base_of(&self.shapes[k])).fold(0.0, f32::max) } else { 0.0 };
            let mut cursor = start_main + lead;
            for &k in &kids {
                let (fill_m, fill_c) = fills(&self.shapes[k]);
                let s = &mut self.shapes[k];
                if s.kind == Kind::Text && s.fill_x && s.fit == TextFit::AutoWidth {
                    s.fit = TextFit::AutoHeight; // a stretched text box wraps instead of resizing itself
                }
                let main_size = if fill_m && !hug_main { s.limit(m, fill_each) } else { axis_get(s.size, m) };
                let cross_size = if fill_c { s.limit(c, inner_cross) } else { axis_get(s.size, c) };
                let cross_pos = start_cross
                    + if baseline {
                        max_base - base_of(s)
                    } else {
                        match lay.cross {
                            Align::Center => (inner_cross - cross_size) / 2.0,
                            Align::End => inner_cross - cross_size,
                            _ => 0.0,
                        }
                    };
                let mut pos = s.pos.to_vec2();
                changed |= axis_set(&mut pos, m, cursor);
                changed |= axis_set(&mut pos, c, cross_pos);
                s.pos = pos.to_pos2();
                if fill_m && !hug_main {
                    changed |= axis_set(&mut s.size, m, main_size);
                }
                if fill_c {
                    changed |= axis_set(&mut s.size, c, cross_size);
                }
                cursor += main_size + gap;
            }
            let f = &mut self.shapes[fi];
            if hug_main {
                let v = f.limit(m, inner_main + pad_main);
                changed |= axis_set(&mut f.size, m, v);
            }
            if hug_cross {
                let v = f.limit(c, inner_cross + pad_cross);
                changed |= axis_set(&mut f.size, c, v);
            }
        }
        changed
    }

    // children in a grid: columns as wide as their widest cell (or the fixed width set for the column), rows as tall as their tallest;
    // a cell can span several columns and rows; a frame that doesn't hug shares its spare room out between the flexible columns / rows
    fn layout_grid(&mut self, fi: usize, kids: &[usize], lay: &AutoLayout) -> bool {
        let cols = lay.columns.max(1) as usize;
        // place the children in order into the first free cell block (rows grow as needed)
        let mut taken: Vec<Vec<bool>> = vec![];
        let mut cells: Vec<(usize, usize, usize, usize)> = vec![]; // row, column, column span, row span
        for &k in kids {
            let [sc, sr] = self.shapes[k].span;
            let (cs, rs) = ((sc.max(1) as usize).min(cols), sr.max(1) as usize);
            let mut at = 0usize;
            let (r, c) = loop {
                let (r, c) = (at / cols, at % cols);
                at += 1;
                if c + cs > cols {
                    continue;
                }
                while taken.len() < r + rs {
                    taken.push(vec![false; cols]);
                }
                if (r..r + rs).all(|y| (c..c + cs).all(|x| !taken[y][x])) {
                    break (r, c);
                }
            };
            (r..r + rs).for_each(|y| (c..c + cs).for_each(|x| taken[y][x] = true));
            cells.push((r, c, cs, rs));
        }
        let rows = taken.len().max(1);
        let fixed_col = |c: usize| lay.col_sizes.get(c).copied().filter(|&w| w > 0.0);
        let fixed_row = |r: usize| lay.row_sizes.get(r).copied().filter(|&h| h > 0.0);
        let (mut col_w, mut row_h) = (vec![0f32; cols], vec![0f32; rows]);
        // single cells first, then spanning ones add what the columns they cover lack
        for pass in 0..2 {
            for (n, &k) in kids.iter().enumerate() {
                let (r, c, cs, rs) = cells[n];
                let size = self.shapes[k].size;
                if (pass == 0) != (cs == 1 && rs == 1) {
                    continue;
                }
                if pass == 0 {
                    col_w[c] = col_w[c].max(size.x);
                    row_h[r] = row_h[r].max(size.y);
                } else {
                    let (have_x, have_y) = (col_w[c..c + cs].iter().sum::<f32>() + lay.gap * (cs - 1) as f32, row_h[r..r + rs].iter().sum::<f32>() + lay.gap * (rs - 1) as f32);
                    if size.x > have_x {
                        col_w[c..c + cs].iter_mut().for_each(|w| *w += (size.x - have_x) / cs as f32);
                    }
                    if size.y > have_y {
                        row_h[r..r + rs].iter_mut().for_each(|h| *h += (size.y - have_y) / rs as f32);
                    }
                }
            }
        }
        for c in 0..cols {
            if let Some(w) = fixed_col(c) {
                col_w[c] = w;
            }
        }
        for r in 0..rows {
            if let Some(h) = fixed_row(r) {
                row_h[r] = h;
            }
        }
        let fsize = self.shapes[fi].size;
        let (gx, gy) = (lay.gap * (cols - 1) as f32, lay.gap * (rows - 1) as f32);
        let flexible_cols: Vec<usize> = (0..cols).filter(|&c| fixed_col(c).is_none()).collect();
        let flexible_rows: Vec<usize> = (0..rows).filter(|&r| fixed_row(r).is_none()).collect();
        if !lay.hug_x && !flexible_cols.is_empty() {
            let extra = ((fsize.x - lay.pad[0] - lay.pad[2] - col_w.iter().sum::<f32>() - gx) / flexible_cols.len() as f32).max(0.0);
            flexible_cols.iter().for_each(|&c| col_w[c] += extra);
        }
        if !lay.hug_y && !flexible_rows.is_empty() {
            let extra = ((fsize.y - lay.pad[1] - lay.pad[3] - row_h.iter().sum::<f32>() - gy) / flexible_rows.len() as f32).max(0.0);
            flexible_rows.iter().for_each(|&r| row_h[r] += extra);
        }
        let mut changed = false;
        for (n, &k) in kids.iter().enumerate() {
            let (r, c, cs, rs) = cells[n];
            let x0 = lay.pad[0] + col_w[..c].iter().sum::<f32>() + lay.gap * c as f32;
            let y0 = lay.pad[1] + row_h[..r].iter().sum::<f32>() + lay.gap * r as f32;
            let cell_w = col_w[c..c + cs].iter().sum::<f32>() + lay.gap * (cs - 1) as f32;
            let cell_h = row_h[r..r + rs].iter().sum::<f32>() + lay.gap * (rs - 1) as f32;
            let s = &mut self.shapes[k];
            if s.fill_x {
                changed |= set(&mut s.size.x, cell_w);
            }
            if s.fill_y {
                changed |= set(&mut s.size.y, cell_h);
            }
            let place = |a: Align, cell: f32, own: f32| match a {
                Align::Center => (cell - own) / 2.0,
                Align::End => cell - own,
                _ => 0.0,
            };
            changed |= set(&mut s.pos.x, x0 + place(lay.main, cell_w, s.size.x));
            changed |= set(&mut s.pos.y, y0 + place(lay.cross, cell_h, s.size.y));
        }
        let f = &mut self.shapes[fi];
        if lay.hug_x {
            changed |= set(&mut f.size.x, lay.pad[0] + lay.pad[2] + col_w.iter().sum::<f32>() + gx);
        }
        if lay.hug_y {
            changed |= set(&mut f.size.y, lay.pad[1] + lay.pad[3] + row_h.iter().sum::<f32>() + gy);
        }
        changed
    }

    // children flow along the main axis and continue on a new line when the frame's width (or height) runs out
    fn layout_wrap(&mut self, fi: usize, kids: &[usize], lay: &AutoLayout) -> bool {
        let (m, c) = if lay.vertical { (1, 0) } else { (0, 1) };
        let pad_main = if lay.vertical { lay.pad[1] + lay.pad[3] } else { lay.pad[0] + lay.pad[2] };
        let pad_cross = if lay.vertical { lay.pad[0] + lay.pad[2] } else { lay.pad[1] + lay.pad[3] };
        let start_main = if lay.vertical { lay.pad[1] } else { lay.pad[0] };
        let start_cross = if lay.vertical { lay.pad[0] } else { lay.pad[1] };
        let hug_cross = if lay.vertical { lay.hug_x } else { lay.hug_y };
        let avail = (axis_get(self.shapes[fi].size, m) - pad_main).max(0.0);

        let mut lines: Vec<Vec<usize>> = vec![vec![]];
        let mut used = 0.0f32;
        for &k in kids {
            let size = axis_get(self.shapes[k].size, m);
            let line = lines.last_mut().unwrap();
            if !line.is_empty() && used + lay.gap + size > avail {
                lines.push(vec![k]);
                used = size;
            } else {
                if !line.is_empty() {
                    used += lay.gap;
                }
                used += size;
                line.push(k);
            }
        }
        let mut changed = false;
        let mut cross_cursor = start_cross;
        for line in &lines {
            let sizes: Vec<(f32, f32)> = line.iter().map(|&k| (axis_get(self.shapes[k].size, m), axis_get(self.shapes[k].size, c))).collect();
            let main_total = sizes.iter().map(|s| s.0).sum::<f32>() + lay.gap * (line.len() - 1) as f32;
            let line_cross = sizes.iter().map(|s| s.1).fold(0.0, f32::max);
            let free = (avail - main_total).max(0.0);
            let (lead, gap) = match lay.main {
                Align::Start => (0.0, lay.gap),
                Align::Center => (free / 2.0, lay.gap),
                Align::End => (free, lay.gap),
                Align::SpaceBetween => (0.0, if line.len() > 1 { lay.gap + free / (line.len() - 1) as f32 } else { lay.gap }),
            };
            let mut cursor = start_main + lead;
            for (&k, &(ms, cs)) in line.iter().zip(&sizes) {
                let cpos = cross_cursor
                    + match lay.cross {
                        Align::Center => (line_cross - cs) / 2.0,
                        Align::End => line_cross - cs,
                        _ => 0.0,
                    };
                let mut pos = self.shapes[k].pos.to_vec2();
                changed |= axis_set(&mut pos, m, cursor);
                changed |= axis_set(&mut pos, c, cpos);
                self.shapes[k].pos = pos.to_pos2();
                cursor += ms + gap;
            }
            cross_cursor += line_cross + lay.gap;
        }
        if hug_cross {
            let total = cross_cursor - lay.gap + (pad_cross - start_cross);
            changed |= axis_set(&mut self.shapes[fi].size, c, total);
        }
        changed
    }

    // sets a shape's rotation in degrees. A frame turns its whole content with it: every descendant's centre moves about the
    // frame's centre and the descendant is rotated by the same angle, so the frame behaves as one rigid piece.
    pub fn set_rotation(&mut self, i: usize, deg: f32) {
        use crate::geom::rot_vec;
        let deg = (deg + 180.0).rem_euclid(360.0) - 180.0;
        let old = self.shapes[i].rotation;
        self.shapes[i].rotation = deg;
        let delta = ((deg - old + 180.0).rem_euclid(360.0) - 180.0).to_radians();
        if self.shapes[i].kind != Kind::Frame || delta.abs() < 1e-7 {
            return;
        }
        let fid = self.shapes[i].id;
        let c = (self.shapes[i].size / 2.0).to_pos2();
        // frame-local origin of each parent's children, before and after the move
        let mut origin: std::collections::HashMap<u64, (Vec2, Vec2)> = std::collections::HashMap::new();
        origin.insert(fid, (Vec2::ZERO, Vec2::ZERO));
        for k in self.draw_order() {
            let Some(&(po, pn)) = self.shapes[k].parent.and_then(|p| origin.get(&p)) else { continue };
            let s = &mut self.shapes[k];
            let old_pos = s.pos;
            if s.kind == Kind::Line {
                let (a, b) = (po + s.pos.to_vec2(), po + (s.pos + s.size).to_vec2());
                let (na, nb) = (c + rot_vec(a - c.to_vec2(), delta), c + rot_vec(b - c.to_vec2(), delta));
                s.pos = (na.to_vec2() - pn).to_pos2();
                s.size = nb - na;
            } else {
                let centre = po + old_pos.to_vec2() + s.size / 2.0;
                let moved = c.to_vec2() + rot_vec(centre - c.to_vec2(), delta);
                s.pos = (moved - pn - s.size / 2.0).to_pos2();
                if s.kind != Kind::Group {
                    s.rotation = ((s.rotation + delta.to_degrees() + 180.0).rem_euclid(360.0)) - 180.0;
                }
            }
            origin.insert(s.id, (po + old_pos.to_vec2(), pn + s.pos.to_vec2()));
        }
    }

    // Figma's "Resize to fit": the frame shrinks or grows to the bounds of its children, which keep their place on the canvas.
    // With auto layout the frame just hugs its contents.
    pub fn resize_to_fit(&mut self) {
        let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Frame) else { return };
        let id = self.shapes[i].id;
        if self.shapes[i].layout.is_some() {
            crate::panels::sizing_set(&mut self.shapes[i], true, crate::panels::Sizing::Hug);
            crate::panels::sizing_set(&mut self.shapes[i], false, crate::panels::Sizing::Hug);
            return;
        }
        let kids: Vec<usize> = (0..self.shapes.len()).filter(|&k| self.shapes[k].parent == Some(id)).collect();
        let Some(bb) = kids.iter().map(|&k| self.shapes[k].aabb()).reduce(|a, b| a.union(b)) else { return };
        for &k in &kids {
            self.shapes[k].pos -= bb.min.to_vec2();
        }
        self.shapes[i].pos += bb.min.to_vec2();
        self.shapes[i].size = bb.size();
    }

    // the "Toggle auto layout" button: a frame gets or loses auto layout; anything else is wrapped in a new auto-layout frame
    pub fn toggle_auto_layout(&mut self) {
        if let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Frame) {
            if self.shapes[i].layout.take().is_none() {
                self.add_auto_layout(i);
            }
            return;
        }
        self.wrap_in_auto_layout();
    }

    // Shift+A, as in Figma: a frame without auto layout gets it (direction, gap, padding and alignment are read from where its
    // children are); anything else (shapes, several shapes, a frame that already has auto layout) is wrapped in a new auto-layout frame
    pub fn add_auto_layout_selection(&mut self) {
        if let Some(i) = self.single().filter(|&i| self.shapes[i].kind == Kind::Frame && self.shapes[i].layout.is_none()) {
            self.add_auto_layout(i);
            return;
        }
        self.wrap_in_auto_layout();
    }

    // Ctrl+Alt+Shift+A: take auto layout off the selected frames (their children stay where they are)
    pub fn remove_auto_layout_selection(&mut self) {
        for id in self.sel.clone() {
            if let Some(i) = self.index_of(id).filter(|&i| self.shapes[i].kind == Kind::Frame) {
                self.shapes[i].layout = None;
            }
        }
    }

    // Ctrl+Alt+G: a plain frame around the selection, which keeps its place on the canvas
    pub fn frame_selection(&mut self) {
        if let Some(id) = self.wrap_selection_in_frame() {
            self.sel = vec![id];
        }
    }

    fn wrap_in_auto_layout(&mut self) {
        let Some(id) = self.wrap_selection_in_frame() else { return };
        self.sel = vec![id];
        let fi = self.index_of(id).unwrap();
        self.add_auto_layout(fi);
    }

    fn wrap_selection_in_frame(&mut self) -> Option<u64> {
        let roots = self.sel_roots();
        let (Some(top), Some(bb)) = (roots.last().and_then(|&id| self.index_of(id)), self.sel_bounds()) else { return None };
        let parent = self.shapes[top].parent;
        let id = self.new_id();
        let mut f = Shape::new(Kind::Frame, id, self.shapes.len() + 1, bb.min - self.origin_of(parent), parent);
        f.size = bb.size();
        f.fills.clear();
        self.shapes.insert(top + 1, f);
        for rid in roots {
            let i = self.index_of(rid).unwrap();
            let world = self.shapes[i].pos + self.origin_of(self.shapes[i].parent);
            self.shapes[i].parent = Some(id);
            self.shapes[i].pos = world - self.origin_of(Some(id));
        }
        Some(id)
    }

    // turns auto layout on for frame `fi`, reading the layout from its children: they run along the longer side of their bounds,
    // the gap is the average distance between neighbours, the padding is the distance from the frame to the children, and the cross
    // axis is aligned where the children already share an edge or a centre
    fn add_auto_layout(&mut self, fi: usize) {
        self.add_auto_layout_as(fi, None, 0);
    }

    // the same with the direction chosen (None: along the longer side) or a grid of `columns` columns (> 0)
    pub fn add_auto_layout_as(&mut self, fi: usize, direction: Option<bool>, columns: u32) {
        let id = self.shapes[fi].id;
        let size = self.shapes[fi].size;
        let kids: Vec<usize> = (0..self.shapes.len()).filter(|&k| self.shapes[k].parent == Some(id)).collect();
        let bb = kids.iter().map(|&k| self.shapes[k].aabb()).reduce(Rect::union);
        let vertical = direction.unwrap_or_else(|| bb.is_some_and(|b| b.height() > b.width()));
        let mut order: Vec<Shape> = kids.iter().map(|&k| self.shapes[k].clone()).collect();
        order.sort_by(|a, b| {
            if columns > 0 {
                // a grid reads row by row
                let key = |s: &Shape| ((s.aabb().min.y / 8.0).round(), s.aabb().min.x);
                let (ka, kb) = (key(a), key(b));
                return ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1));
            }
            let (ka, kb) = if vertical { (a.pos.y, b.pos.y) } else { (a.pos.x, b.pos.x) };
            ka.total_cmp(&kb)
        });
        let boxes: Vec<Rect> = order.iter().map(|s| s.aabb()).collect();
        for (&slot, s) in kids.iter().zip(order) {
            self.shapes[slot] = s;
        }
        let mut lay = AutoLayout { vertical, columns, ..AutoLayout::default() };
        if let Some(bb) = bb {
            let gaps: Vec<f32> = if columns > 0 {
                // between neighbours in the same row
                boxes.windows(2).filter(|w| (w[1].min.y - w[0].min.y).abs() < 8.0).map(|w| w[1].min.x - w[0].max.x).collect()
            } else {
                boxes.windows(2).map(|w| if vertical { w[1].min.y - w[0].max.y } else { w[1].min.x - w[0].max.x }).collect()
            };
            if !gaps.is_empty() {
                lay.gap = (gaps.iter().sum::<f32>() / gaps.len() as f32).round().max(0.0);
            }
            lay.pad = [bb.min.x, bb.min.y, size.x - bb.max.x, size.y - bb.max.y].map(|p| p.max(0.0).round());
            // the cross axis: shared start, centre or end edge (within a pixel)
            let edge = |r: &Rect, k: u8| match (vertical, k) {
                (false, 0) => r.min.y,
                (false, 1) => r.center().y,
                (false, _) => r.max.y,
                (true, 0) => r.min.x,
                (true, 1) => r.center().x,
                (true, _) => r.max.x,
            };
            let same = |k: u8| boxes.iter().all(|r| (edge(r, k) - edge(&boxes[0], k)).abs() < 1.0);
            lay.cross = if columns > 0 || boxes.len() < 2 || same(0) { Align::Start } else if same(1) { Align::Center } else if same(2) { Align::End } else { Align::Start };
        }
        self.shapes[fi].layout = Some(lay);
        self.shapes[fi].clip = false;
    }

    // text boxes that size themselves
    pub fn fit_texts(&mut self, painter: &Painter) {
        let mut upd = vec![];
        for (i, s) in self.shapes.iter().enumerate() {
            if s.kind != Kind::Text || s.fit == TextFit::Fixed {
                continue;
            }
            let g = text_galley(painter, s, 1.0, egui::Color32::WHITE, wrap_of(s));
            let want = Vec2::new(if s.fit == TextFit::AutoWidth { g.size().x.ceil() + 1.0 } else { s.size.x }, (g.size().y - s.trim_px().0 - s.trim_px().1).ceil());
            if (want - s.size).abs().max_elem() > 0.5 {
                upd.push((i, want));
            }
        }
        for (i, want) in upd {
            self.shapes[i].size = want;
        }
    }

    // ---- snapping

    fn snap_lines_for(&self, exclude: &[u64]) -> (Vec<f32>, Vec<f32>) {
        let (mut xs, mut ys) = (vec![], vec![]);
        for i in 0..self.shapes.len() {
            let s = &self.shapes[i];
            if !s.visible || exclude.iter().any(|&e| self.is_inside(s.id, e)) {
                continue;
            }
            let r = self.world_rect(i);
            xs.extend([r.min.x, r.center().x, r.max.x]);
            ys.extend([r.min.y, r.center().y, r.max.y]);
        }
        for g in &self.guides {
            if g.vertical { xs.push(g.pos) } else { ys.push(g.pos) }
        }
        (xs, ys)
    }

    // offset that moves the nearest edge/center of `r` onto a snap line, per axis; remembers what snapped
    pub fn snap_rect(&mut self, r: Rect, exclude: &[u64]) -> Vec2 {
        if !self.snap {
            return Vec2::ZERO;
        }
        let (xs, ys) = self.snap_lines_for(exclude);
        let thr = 6.0 / self.zoom;
        let best = |mine: [f32; 3], cands: &[f32]| -> Option<(f32, f32)> {
            let mut best: Option<(f32, f32)> = None;
            for m in mine {
                for &c in cands {
                    let d = c - m;
                    if d.abs() <= thr && best.is_none_or(|(b, _)| d.abs() < b.abs()) {
                        best = Some((d, c));
                    }
                }
            }
            best
        };
        let mut off = Vec2::ZERO;
        if let Some((d, line)) = best([r.min.x, r.center().x, r.max.x], &xs) {
            off.x = d;
            self.snap_lines.push(Guide { vertical: true, pos: line });
        }
        if let Some((d, line)) = best([r.min.y, r.center().y, r.max.y], &ys) {
            off.y = d;
            self.snap_lines.push(Guide { vertical: false, pos: line });
        }
        off
    }

    pub fn snap_pos(&mut self, p: Pos2, exclude: &[u64]) -> Pos2 {
        let p = p + self.snap_rect(Rect::from_min_max(p, p), exclude);
        if self.snap_pixels && self.snap_lines.is_empty() {
            return Pos2::new(p.x.round(), p.y.round());
        }
        p
    }

    // ---- scaling groups and multi-selections

    // remember the selection (and the insides of groups) so a drag can rescale from the originals.
    // A frame's own children are left alone: they follow its constraints.
    pub fn begin_scale(&mut self) {
        let mut out = vec![];
        let mut stack = self.sel_roots();
        while let Some(id) = stack.pop() {
            let Some(i) = self.index_of(id) else { continue };
            out.push(self.shapes[i].clone());
            // the Scale tool (K) takes a frame's children along; plain resizing only does for groups
            if self.shapes[i].kind == Kind::Group || (self.scale_tool && self.shapes[i].kind == Kind::Frame) {
                stack.extend(self.shapes.iter().filter(|s| s.parent == Some(id)).map(|s| s.id));
            }
        }
        self.scale_orig = out;
    }

    pub fn do_scale(&mut self, h: (i8, i8), bounds: Rect, pointer: Pos2, uniform: bool) {
        let grab = Pos2::new(if h.0 < 0 { bounds.min.x } else { bounds.max.x }, if h.1 < 0 { bounds.min.y } else { bounds.max.y });
        let anchor = Pos2::new(if h.0 < 0 { bounds.max.x } else { bounds.min.x }, if h.1 < 0 { bounds.max.y } else { bounds.min.y });
        let ratio = |p: f32, g: f32, a: f32| if (g - a).abs() < 1e-6 { 1.0 } else { ((p - a) / (g - a)).max(0.01) };
        let mut s = Vec2::new(if h.0 == 0 { 1.0 } else { ratio(pointer.x, grab.x, anchor.x) }, if h.1 == 0 { 1.0 } else { ratio(pointer.y, grab.y, anchor.y) });
        if uniform && h.0 != 0 && h.1 != 0 {
            let m = s.x.max(s.y);
            s = Vec2::splat(m);
        }
        let originals = self.scale_orig.clone();
        let roots = self.sel_roots();
        for o in &originals {
            let Some(i) = self.index_of(o.id) else { continue };
            let is_root = roots.contains(&o.id);
            let origin = self.origin_of(o.parent);
            let sh = &mut self.shapes[i];
            if is_root {
                let world = o.pos + origin;
                let nw = Pos2::new(anchor.x + (world.x - anchor.x) * s.x, anchor.y + (world.y - anchor.y) * s.y);
                sh.pos = nw - origin;
            } else {
                sh.pos = Pos2::new(o.pos.x * s.x, o.pos.y * s.y);
            }
            sh.size = o.size * s;
            sh.radius = o.radius.map(|r| r * (s.x + s.y) / 2.0);
            if o.kind == Kind::Text {
                sh.font_size = (o.font_size * (s.x + s.y) / 2.0).max(1.0);
            }
            if self.scale_tool {
                sh.stroke_w = o.stroke_w * (s.x + s.y) / 2.0;
            }
        }
    }

    // rotated-shape resize: `h` is the handle direction, `rect` the unrotated rect when the drag began
    pub fn do_resize(&mut self, i: usize, h: (i8, i8), rect: Rect, pointer_world: Pos2, uniform: bool) {
        let o = self.origin_of(self.shapes[i].parent);
        let rot = self.shapes[i].rot();
        let p = pointer_world - o;
        let c0 = rect.center();
        let pl = c0 + rot_vec(p - c0, -rot); // pointer in the shape's own (unrotated) frame
        let (mut min, mut max) = (rect.min, rect.max);
        if h.0 < 0 {
            min.x = pl.x;
        } else if h.0 > 0 {
            max.x = pl.x;
        }
        if h.1 < 0 {
            min.y = pl.y;
        } else if h.1 > 0 {
            max.y = pl.y;
        }
        let keep = uniform || self.shapes[i].lock_aspect;
        if keep && rect.width() > 1e-6 && rect.height() > 1e-6 && (h.0 != 0 || h.1 != 0) {
            let (w, hh) = ((max.x - min.x).abs(), (max.y - min.y).abs());
            let k = match (h.0 != 0, h.1 != 0) {
                (true, true) => (w / rect.width()).max(hh / rect.height()),
                (true, false) => w / rect.width(),
                _ => hh / rect.height(),
            };
            let (nw, nh) = (rect.width() * k, rect.height() * k);
            if h.0 < 0 { min.x = max.x - nw } else if h.0 > 0 { max.x = min.x + nw } else { min.x = c0.x - nw / 2.0; max.x = c0.x + nw / 2.0 }
            if h.1 < 0 { min.y = max.y - nh } else if h.1 > 0 { max.y = min.y + nh } else { min.y = c0.y - nh / 2.0; max.y = c0.y + nh / 2.0 }
        }
        let r = Rect::from_two_pos(min, max);
        // the opposite side must stay put on screen: rotate the new center about the old one
        let center = c0 + rot_vec(r.center() - c0, rot);
        let s = &mut self.shapes[i];
        s.size = r.size();
        s.pos = center - r.size() / 2.0;
        if s.kind == Kind::Text && s.fit != TextFit::Fixed {
            s.fit = if h.0 != 0 && h.1 == 0 { TextFit::AutoHeight } else { TextFit::Fixed };
        }
    }
}
