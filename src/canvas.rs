// The canvas: view navigation, tools, selection handles, overlays, text editing, present mode.
use egui::{Align2, Color32, CursorIcon, FontId, Painter, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::geom::{BoolOp, flatten};
use crate::model::*;
use crate::{App, BG, BRAND, Drag, GUIDE, HANDLE, Mode, TEXT_SECONDARY, Tool};

const RULER: f32 = 18.0;
const DIRS: [(i8, i8); 8] = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)];

// what the selection handles act on
enum Handles {
    None,
    Line(usize),
    Single(usize),
    Multi(Rect),
}

impl App {
    fn handles(&self) -> Handles {
        let roots = self.sel_roots();
        match roots[..] {
            [] => Handles::None,
            [id] => match self.index_of(id) {
                // no handles while typing: a stray press on one would resize the box and fix its size
                Some(i) if self.editing_text == Some(self.shapes[i].id) => Handles::None,
                Some(i) if self.shapes[i].locked || !self.chain(i).iter().all(|&k| self.shapes[k].visible) => Handles::None,
                Some(i) if self.shapes[i].kind == Kind::Line => Handles::Line(i),
                Some(i) if self.shapes[i].kind == Kind::Group => Handles::Multi(self.world_rect(i)),
                Some(i) => Handles::Single(i),
                None => Handles::None,
            },
            _ => self.sel_bounds().map_or(Handles::None, Handles::Multi),
        }
    }

    // world position of each of the 8 handles of a single shape (rotation applied)
    pub(crate) fn single_handle_pts(&self, i: usize) -> Vec<((i8, i8), Pos2)> {
        let s = &self.shapes[i];
        let (o, r) = (self.origin_of(s.parent), s.rect());
        DIRS.iter()
            .map(|&h| {
                let local = Pos2::new(r.center().x + h.0 as f32 * r.width() / 2.0, r.center().y + h.1 as f32 * r.height() / 2.0);
                (h, s.xf(local) + o)
            })
            .collect()
    }

    fn bounds_handle_pts(b: Rect) -> Vec<((i8, i8), Pos2)> {
        DIRS.iter().map(|&h| (h, Pos2::new(b.center().x + h.0 as f32 * b.width() / 2.0, b.center().y + h.1 as f32 * b.height() / 2.0))).collect()
    }

    // which handle (if any) sits under the screen point, as the drag it starts
    fn handle_drag_at(&mut self, origin: Pos2, press: Pos2) -> Option<Drag> {
        let near = |a: Pos2, r: f32| (a - press).length() <= r;
        match self.handles() {
            Handles::None => None,
            Handles::Line(i) => {
                let s = &self.shapes[i];
                let o = self.origin_of(s.parent);
                let (a, b) = (s.pos + o, s.pos + s.size + o);
                if near(self.to_screen(origin, a), HANDLE) {
                    Some(Drag::Corners(b))
                } else if near(self.to_screen(origin, b), HANDLE) {
                    Some(Drag::Corners(a))
                } else {
                    None
                }
            }
            Handles::Single(i) => {
                let pts = self.single_handle_pts(i);
                if let Some(&(h, _)) = pts.iter().find(|(_, p)| near(self.to_screen(origin, *p), HANDLE * 0.8)) {
                    if self.scale_tool {
                        self.begin_scale();
                        return Some(Drag::Scale { h, bounds: self.world_rect(i) });
                    }
                    return Some(Drag::Resize { h, rect: self.shapes[i].rect() });
                }
                let s = &self.shapes[i];
                if s.kind == Kind::Group {
                    return None;
                }
                // just outside a corner: rotate
                let w = self.to_world(origin, press);
                let inside = s.rect().expand(2.0 / self.zoom).contains(s.unxf(w - self.origin_of(s.parent)));
                let corner = pts.iter().filter(|(h, _)| h.0 != 0 && h.1 != 0).any(|(_, p)| near(self.to_screen(origin, *p), HANDLE + 14.0));
                if corner && !inside {
                    let c = s.center() + self.origin_of(s.parent);
                    let a = (w - c).angle();
                    return Some(Drag::Rotate { center: c, start: a, base: s.rotation });
                }
                None
            }
            Handles::Multi(b) => {
                let hit = Self::bounds_handle_pts(b).into_iter().find(|(_, p)| near(self.to_screen(origin, *p), HANDLE * 0.8));
                hit.map(|(h, _)| {
                    self.begin_scale();
                    Drag::Scale { h, bounds: b }
                })
            }
        }
    }

    fn cursor_for(h: (i8, i8)) -> CursorIcon {
        match h {
            (0, _) => CursorIcon::ResizeVertical,
            (_, 0) => CursorIcon::ResizeHorizontal,
            (a, b) if a == b => CursorIcon::ResizeNwSe,
            _ => CursorIcon::ResizeNeSw,
        }
    }

    fn node_hit(&self, origin: Pos2, press: Pos2) -> Option<(usize, usize, u8)> {
        let i = self.single().filter(|&i| self.shapes[i].kind == Kind::Path)?;
        let pts = self.node_points(i);
        let sp = |p: Pos2| self.to_screen(origin, p);
        if let Some((c, k)) = self.node_sel {
            if let Some(q) = pts.get(c).and_then(|c| c.pts.get(k)) {
                if q.hin != Vec2::ZERO && (sp(q.p + q.hin) - press).length() <= 6.0 {
                    return Some((c, k, 1));
                }
                if q.hout != Vec2::ZERO && (sp(q.p + q.hout) - press).length() <= 6.0 {
                    return Some((c, k, 2));
                }
            }
        }
        for (c, contour) in pts.iter().enumerate() {
            for (k, q) in contour.pts.iter().enumerate() {
                if (sp(q.p) - press).length() <= 7.0 {
                    return Some((c, k, 0));
                }
            }
        }
        None
    }

    pub fn exit_present(&mut self) {
        self.present_inline = false;
        self.stop_video_playback();
        self.present_anim = None;
        self.present_overlays.clear();
        self.audio_stop();
        self.page_modes.clear();
        if let Some(v) = self.present_vars.take() {
            self.variables = v;
        }
        if let Some(v) = self.present_shapes.take() {
            self.shapes = v;
            self.prune_sel();
        }
        if self.present.take().is_some() {
            (self.pan, self.zoom) = self.saved_view;
        }
    }

    pub fn start_present(&mut self, frame: u64) {
        if self.present.is_none() {
            self.saved_view = (self.pan, self.zoom);
        }
        if self.present_vars.is_none() {
            self.present_vars = Some(self.variables.clone());
        }
        if self.present_shapes.is_none() {
            self.present_shapes = Some(self.shapes.clone());
        }
        self.present = Some(frame);
        self.present_t0 = None;
        self.present_stack.clear();
        self.video_ctl.clear();
        self.video_last.clear();
        self.present_scrolls.clear();
        self.present_overlays.clear();
        self.present_scroll = Vec2::ZERO;
        self.present_fired = false;
        self.present_hover = None;
        self.present_while.clear();
        self.page_modes.clear();
        self.sel.clear();
    }

    // the most the presented frame's content can be scrolled: its children's bounds beyond the frame's own size
    pub(crate) fn present_scroll_range(&self, i: usize) -> Vec2 {
        let id = self.shapes[i].id;
        let size = self.shapes[i].size;
        let mut far = size;
        for s in self.shapes.iter().filter(|s| s.parent == Some(id) && s.visible) {
            let b = s.aabb();
            far = far.max(b.max.to_vec2());
        }
        (far - size).max(Vec2::ZERO)
    }

    fn canvas_present(&mut self, ui: &mut egui::Ui, resp: &egui::Response, painter: &Painter, origin: Pos2) {
        let Some(i) = self.present.and_then(|id| self.index_of(id)) else {
            self.exit_present();
            return;
        };
        let r = self.world_rect(i);
        let avail = resp.rect.size() - Vec2::splat(48.0);
        let fit = (avail.x / r.width().max(1.0)).min(avail.y / r.height().max(1.0));
        // Z cycles the content scaling: fit, fit width, fill screen, 100%
        if ui.input(|inp| inp.key_pressed(egui::Key::Z) && inp.modifiers.is_none()) {
            if let Some(pg) = self.pages.get_mut(self.cur) {
                pg.proto_scale = (pg.proto_scale + 1) % 4;
            }
        }
        let scale = self.pages.get(self.cur).map_or(0, |pg| pg.proto_scale);
        self.zoom = match scale {
            1 => avail.x / r.width().max(1.0),
            2 => (resp.rect.size().x / r.width().max(1.0)).max(resp.rect.size().y / r.height().max(1.0)),
            3 => 1.0,
            _ => fit,
        }
        .clamp(0.05, 8.0);
        self.pan = resp.rect.size() / 2.0 - r.center().to_vec2() * self.zoom;
        if scale == 1 && r.height() * self.zoom > resp.rect.size().y {
            self.pan.y = 24.0 - r.min.y * self.zoom; // taller than the screen: starts at the top
        }
        let now = ui.input(|inp| inp.time);
        if self.present_t0.is_none() && self.has_motion() {
            self.audio_play(0.0); // the sound starts with the frame's animation
        }
        self.present_t0.get_or_insert(now);
        let video_t = (now - self.present_t0.unwrap_or(now)) as f32;
        self.update_video_textures(ui.ctx(), video_t);
        // keyframe animations run from when the frame appeared (the page is swapped back below)
        let base = self.has_motion().then(|| {
            let t0 = *self.present_t0.get_or_insert(now);
            let (t, running) = crate::anim::play_time(0.0, ((now - t0) * 1000.0) as f32, self.anim_len(), self.pages.get(self.cur).is_some_and(|p| p.anim_loop));
            if running {
                ui.ctx().request_repaint();
            }
            let shown = crate::anim::evaluate(&self.shapes, t);
            std::mem::replace(&mut self.shapes, shown)
        });
        // Prototype settings: the colour around the frame and a device bezel
        let (bg, device) = self.pages.get(self.cur).map_or((Color32::BLACK, Device::None), |pg| (pg.proto_bg, pg.proto_device));
        painter.rect_filled(resp.rect, 0.0, bg);
        let screen = self.screen_rect(origin, r);
        let bezel = match device {
            Device::None => 0.0,
            Device::Phone => 14.0,
            Device::Tablet => 22.0,
            Device::Laptop => 10.0,
        };
        if bezel > 0.0 {
            let outer = screen.expand(bezel);
            painter.rect_filled(outer, if device == Device::Phone { 34.0 } else { 16.0 }, Color32::from_gray(0x2a));
            painter.rect_stroke(outer, if device == Device::Phone { 34.0 } else { 16.0 }, Stroke::new(1.0, Color32::from_gray(0x55)), StrokeKind::Inside);
        }
        // Scroll behavior > Overflow: the wheel scrolls the frame's children, within their bounds
        let overflow = self.shapes[i].overflow;
        if overflow != Overflow::None {
            let wheel = wheel_delta(ui);
            let range = self.present_scroll_range(i);
            let (h, v) = (matches!(overflow, Overflow::Horizontal | Overflow::Both), matches!(overflow, Overflow::Vertical | Overflow::Both));
            let d = Vec2::new(if h { -wheel.x } else { 0.0 }, if v { -wheel.y } else { 0.0 }) / self.zoom;
            self.present_scroll = (self.present_scroll + d).clamp(Vec2::ZERO, range);
        } else {
            self.present_scroll = Vec2::ZERO;
        }
        let unscrolled = (self.present_scroll != Vec2::ZERO).then(|| {
            let before = self.shapes.clone();
            let id = self.shapes[i].id;
            let by = self.present_scroll;
            for c in self.shapes.iter_mut().filter(|c| c.parent == Some(id) && !c.fixed) {
                c.pos -= by;
            }
            before
        });
        let s = self.shapes[i].clone();
        let p = painter.with_clip_rect(screen.intersect(resp.rect));
        let anim = self.present_anim.and_then(|a| Some((a, self.index_of(a.from)?)));
        match anim {
            Some((a, fi)) if now < a.t0 + a.dur as f64 => {
                let t = ((now - a.t0) / a.dur as f64) as f32;
                if a.tr == Transition::SmartAnimate {
                    // draw the new frame with its layers tweened from their namesakes in the old one
                    let tweened = smart_tween(&self.shapes, a.from, s.id, a.ease.at(t));
                    let saved = std::mem::replace(&mut self.shapes, tweened);
                    let mid = self.shapes[i].clone();
                    self.draw_shape(&p, origin, &mid, self.origin_of(mid.parent), 1.0);
                    self.shapes = saved;
                } else {
                    let f = transition_frame(a.tr, a.ease, t, screen.size());
                    // the old frame is drawn where the new one is, so both share the presented area
                    let from = self.shapes[fi].clone();
                    let shift = r.min - self.world_rect(fi).min;
                    self.draw_shape(&p, origin, &from, self.origin_of(from.parent) + shift + f.from_offset / self.zoom, f.from_alpha);
                    self.draw_shape(&p, origin, &s, self.origin_of(s.parent) + f.to_offset / self.zoom, f.to_alpha);
                }
                ui.ctx().request_repaint();
            }
            _ => {
                self.present_anim = None;
                self.draw_shape(&p, origin, &s, self.origin_of(s.parent), 1.0);
            }
        }
        // overlays: the page behind is dimmed, each overlay frame is drawn where it was placed
        self.prune_overlays(now);
        let overlays = self.overlay_list();
        // an overlay comes in with its animation (Dissolve fades, the slides move it in from an edge); the dimming fades with it.
        // Closing plays the same animation backwards.
        let progress = |o: &crate::present::Overlay| {
            if o.dur > 0.0 && o.tr != Transition::Instant {
                match o.closing {
                    Some(c) => 1.0 - ((now - c) / o.dur as f64).clamp(0.0, 1.0) as f32,
                    None => ((now - o.t0) / o.dur as f64).clamp(0.0, 1.0) as f32,
                }
            } else {
                1.0
            }
        };
        if overlays.iter().any(|o| o.dim) {
            let k = overlays.iter().filter(|o| o.dim).map(|o| progress(o)).fold(0.0, f32::max);
            painter.rect_filled(screen.intersect(resp.rect), 0.0, Color32::from_black_alpha((90.0 * k) as u8));
        }
        for o in overlays {
            if let Some(oi) = self.index_of(o.frame) {
                let ov = self.shapes[oi].clone();
                let q = painter.with_clip_rect(resp.rect);
                let t = progress(&o);
                let (offset, alpha) = if t < 1.0 {
                    let tr = if o.tr == Transition::SmartAnimate { Transition::Dissolve } else { o.tr };
                    let f = transition_frame(tr, o.ease, t, screen.size());
                    ui.ctx().request_repaint();
                    (f.to_offset / self.zoom, f.to_alpha)
                } else {
                    (Vec2::ZERO, 1.0)
                };
                self.draw_shape(&q, origin, &ov, self.origin_of(ov.parent) + o.shift + offset, alpha);
            }
        }
        if let Some(before) = unscrolled {
            self.shapes = before;
        }
        if let Some(base) = base {
            self.shapes = base;
        }
        painter.text(resp.rect.left_top() + Vec2::new(12.0, 10.0), Align2::LEFT_TOP, "Presenting. Click linked shapes to navigate, Esc to exit.", FontId::proportional(11.0), TEXT_SECONDARY());
        // the comments of the file are visible while presenting (when comments are shown), as in Figma's prototype view
        self.paint_comments(ui, &painter, origin, resp.rect);
        let hover = ui.input(|inp| inp.pointer.hover_pos()).filter(|h| resp.rect.contains(*h)).map(|h| self.to_world(origin, h));
        self.present_hover_at(hover, now);
        self.present_video_events(now);
        if let Some(wait) = self.present_delays(now) {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f32(wait.clamp(0.01, 1.0)));
        }
        if let Some(url) = self.open_url.take() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
        // pointer and key triggers
        if let (true, Some(p)) = (resp.drag_started(), ui.input(|inp| inp.pointer.press_origin())) {
            self.drag_origin = Some(self.to_world(origin, p));
        }
        // On drag runs once the pointer has gone far enough to have a direction
        if let (Some(from), true, Some(p)) = (self.drag_origin, resp.dragged(), ui.input(|inp| inp.pointer.latest_pos())) {
            let d = self.to_world(origin, p) - from;
            if d.length() * self.zoom > 12.0 {
                self.present_dir = if d.x.abs() > d.y.abs() { if d.x < 0.0 { 1 } else { 2 } } else if d.y < 0.0 { 3 } else { 4 };
                self.present_pointer(from, Trigger::Drag, now);
                self.present_dir = 0;
                self.drag_origin = None;
            }
        }
        if !resp.dragged() {
            self.drag_origin = None;
        }
        let (down, up) = ui.input(|inp| (inp.pointer.primary_pressed(), inp.pointer.primary_released()));
        if let Some(p) = ui.input(|inp| inp.pointer.interact_pos()).filter(|p| resp.rect.contains(*p)) {
            let w = self.to_world(origin, p);
            if down {
                self.present_pointer(w, Trigger::MouseDown, now);
                self.present_press(w);
            }
            if up {
                self.present_pointer(w, Trigger::MouseUp, now);
            }
        }
        if up {
            self.present_release();
        }
        let keys: Vec<String> = ui.input(|inp| inp.events.iter().filter_map(|e| if let egui::Event::Key { key, pressed: true, repeat: false, .. } = e { Some(key.name().to_owned()) } else { None }).collect());
        for k in keys.into_iter().filter(|k| k != "Escape") {
            self.present_key(&k, now);
        }
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let w = self.to_world(origin, p);
                self.present_click(w, now);
            }
        }
    }

    // View > Outlines: the fills are covered with the page colour and every layer is drawn as a 1px wireframe
    fn draw_outlines(&self, painter: &Painter, rect: Rect, origin: Pos2) {
        painter.rect_filled(rect, 0.0, self.page_bg());
        let ink = Color32::from_gray(0x80);
        for (i, s) in self.shapes.iter().enumerate() {
            if !s.visible && !self.outline_hidden {
                continue;
            }
            self.outline_shape(painter, origin, i, Stroke::new(1.0, ink));
            if self.outline_bounds {
                let r = self.screen_rect(origin, self.world_rect(i));
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, BRAND.gamma_multiply(0.6)), StrokeKind::Middle);
            }
        }
    }

    // View > Layout guides: the Layout guide section's grids, columns and rows drawn over their frame
    fn draw_layout_guides(&self, painter: &Painter, origin: Pos2) {
        if !self.layout_guides {
            return;
        }
        for (i, s) in self.shapes.iter().enumerate().filter(|(_, s)| s.visible && !s.grids.is_empty()) {
            let w = self.world_rect(i);
            let clip = painter.with_clip_rect(self.screen_rect(origin, w).intersect(painter.clip_rect()));
            for g in s.grids.iter().filter(|g| g.visible) {
                let mut bar = |r: Rect| clip.rect_filled(self.screen_rect(origin, r), 0.0, g.color);
                match g.kind {
                    GridKind::Grid => {
                        let step = g.size.max(1.0);
                        let (mut x, mut y) = (w.left(), w.top());
                        while x < w.right() {
                            bar(Rect::from_min_max(Pos2::new(x, w.top()), Pos2::new(x + 1.0 / self.zoom.min(1.0), w.bottom())));
                            x += step;
                        }
                        while y < w.bottom() {
                            bar(Rect::from_min_max(Pos2::new(w.left(), y), Pos2::new(w.right(), y + 1.0 / self.zoom.min(1.0))));
                            y += step;
                        }
                    }
                    GridKind::Columns | GridKind::Rows => {
                        let cols = g.kind == GridKind::Columns;
                        let n = g.count.max(1) as f32;
                        let len = if cols { w.width() } else { w.height() };
                        // Stretch fills the frame between the margins; the others use the fixed size, packed to one side or centred
                        let (cell, first) = if g.align == GridAlign::Stretch {
                            (((len - 2.0 * g.margin - (n - 1.0) * g.gutter) / n).max(0.0), g.margin)
                        } else {
                            let total = n * g.cell + (n - 1.0) * g.gutter;
                            (g.cell, match g.align {
                                GridAlign::Start => g.margin,
                                GridAlign::End => len - g.margin - total,
                                _ => (len - total) / 2.0,
                            })
                        };
                        for k in 0..g.count {
                            let a = (if cols { w.left() } else { w.top() }) + first + k as f32 * (cell + g.gutter);
                            bar(if cols { Rect::from_min_max(Pos2::new(a, w.top()), Pos2::new(a + cell, w.bottom())) } else { Rect::from_min_max(Pos2::new(w.left(), a), Pos2::new(w.right(), a + cell)) });
                        }
                    }
                }
            }
        }
    }

    // Figma's pixel grid: 1px lines over the artwork once a pixel is at least 8 screen px wide (zoom 800%)
    fn draw_pixel_grid(&self, painter: &Painter, rect: Rect, origin: Pos2) {
        if !self.pixel_grid || self.zoom < 8.0 {
            return;
        }
        let col = Color32::from_rgba_unmultiplied(128, 128, 128, 40);
        let st = Stroke::new(1.0, col);
        let tl = self.to_world(origin, rect.min);
        let br = self.to_world(origin, rect.max);
        for x in (tl.x.floor() as i32)..=(br.x.ceil() as i32) {
            let sx = self.to_screen(origin, Pos2::new(x as f32, 0.0)).x.round() + 0.5;
            painter.line_segment([Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())], st);
        }
        for y in (tl.y.floor() as i32)..=(br.y.ceil() as i32) {
            let sy = self.to_screen(origin, Pos2::new(0.0, y as f32)).y.round() + 0.5;
            painter.line_segment([Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)], st);
        }
    }

    fn draw_rulers(&self, painter: &Painter, rect: Rect) {
        let font = FontId::proportional(9.0);
        let (mut step, mut k) = (1.0f32, 0);
        while step * self.zoom < 60.0 {
            step *= [2.0, 2.5, 2.0][k % 3];
            k += 1;
        }
        let top = Rect::from_min_max(rect.min + Vec2::new(RULER, 0.0), Pos2::new(rect.max.x, rect.min.y + RULER));
        let left = Rect::from_min_max(rect.min + Vec2::new(0.0, RULER), Pos2::new(rect.min.x + RULER, rect.max.y));
        painter.rect_filled(top, 0.0, BG());
        painter.rect_filled(left, 0.0, BG());
        painter.rect_filled(Rect::from_min_size(rect.min, Vec2::splat(RULER)), 0.0, BG());
        let tick = Stroke::new(1.0, TEXT_SECONDARY());
        let (w0, w1) = (self.to_world(rect.min, rect.min + Vec2::new(RULER, RULER)), self.to_world(rect.min, rect.max));
        let mut x = (w0.x / step).floor() * step;
        while x <= w1.x {
            let sx = rect.min.x + self.pan.x + x * self.zoom;
            if sx >= rect.min.x + RULER {
                painter.line_segment([Pos2::new(sx, rect.min.y + 10.0), Pos2::new(sx, rect.min.y + RULER)], tick);
                painter.text(Pos2::new(sx + 2.0, rect.min.y + 1.0), Align2::LEFT_TOP, format!("{x:.0}"), font.clone(), TEXT_SECONDARY());
            }
            x += step;
        }
        let mut y = (w0.y / step).floor() * step;
        while y <= w1.y {
            let sy = rect.min.y + self.pan.y + y * self.zoom;
            if sy >= rect.min.y + RULER {
                painter.line_segment([Pos2::new(rect.min.x + 10.0, sy), Pos2::new(rect.min.x + RULER, sy)], tick);
                let galley = painter.layout_no_wrap(format!("{y:.0}"), font.clone(), TEXT_SECONDARY());
                let pivot = Pos2::new(rect.min.x + 3.0, sy + 3.0 + galley.size().x);
                painter.add(egui::epaint::TextShape::new(pivot, galley, TEXT_SECONDARY()).with_angle(-std::f32::consts::FRAC_PI_2));
            }
            y += step;
        }
    }

    // blend modes can't be done by egui's painter, so the page is rendered by resvg (half size while dragging)
    // and shown as one image; frame names are still drawn on top
    fn draw_composited(&mut self, ctx: &egui::Context, painter: &Painter, rect: Rect) {
        let dragging = ctx.input(|i| i.pointer.any_down());
        let k = ctx.pixels_per_point() * if dragging { 0.5 } else { 1.0 };
        let px = [(rect.width() * k).round().max(1.0) as u32, (rect.height() * k).round().max(1.0) as u32];
        let background = self.page_bg();
        let fresh = self.blend_cache.as_ref().is_some_and(|c| c.shapes == self.shapes && c.pan == self.pan && c.zoom == self.zoom && c.px == px && c.background == background);
        if !fresh {
            let view = Rect::from_min_size(self.to_world(rect.min, rect.min), rect.size() / self.zoom);
            let svg = self.svg_scene(ctx, view, Vec2::new(px[0] as f32, px[1] as f32), background);
            if let Some(img) = crate::export::rasterize_scene(&svg, px[0], px[1]) {
                let tex = ctx.load_texture("composite", img, egui::TextureOptions::LINEAR);
                self.blend_cache = Some(crate::BlendCache { shapes: self.shapes.clone(), pan: self.pan, zoom: self.zoom, px, background, tex });
            }
        }
        if let Some(c) = &self.blend_cache {
            painter.image(c.tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        for s in self.shapes.iter().filter(|s| s.kind == Kind::Frame && s.parent.is_none() && s.visible) {
            let r = self.screen_rect(rect.min, s.rect());
            painter.text(r.left_top() - Vec2::new(0.0, 4.0), Align2::LEFT_BOTTOM, &s.name, FontId::proportional(11.0), TEXT_SECONDARY());
        }
    }

    fn outline_shape(&self, painter: &Painter, origin: Pos2, i: usize, stroke: Stroke) {
        let s = &self.shapes[i];
        let o = self.origin_of(s.parent);
        if s.kind == Kind::Line || s.kind == Kind::Group {
            painter.rect_stroke(self.screen_rect(origin, self.world_rect(i)), 0.0, stroke, StrokeKind::Outside);
        } else {
            let c: Vec<Pos2> = s.corners().iter().map(|&p| self.to_screen(origin, p + o)).collect();
            painter.add(egui::Shape::closed_line(c, stroke));
        }
    }

    // the single selected top-level frame, and the screen centres of its two "+" buttons (right, below)
    fn quick_add_pts(&self, origin: Pos2) -> Option<(u64, [Pos2; 2])> {
        if self.tool != Tool::Select || self.mode == Mode::Dev || self.sel.len() != 1 {
            return None;
        }
        let i = self.index_of(self.sel[0]).filter(|&i| self.shapes[i].kind == Kind::Frame && self.shapes[i].parent.is_none() && !self.shapes[i].locked)?;
        let r = self.screen_rect(origin, self.world_rect(i));
        Some((self.shapes[i].id, [Pos2::new(r.right() + 18.0, r.center().y), Pos2::new(r.center().x, r.bottom() + 40.0)]))
    }

    fn quick_add_hit(&self, origin: Pos2, p: Pos2) -> Option<bool> {
        let (_, pts) = self.quick_add_pts(origin)?;
        pts.iter().position(|c| c.distance(p) <= 10.0).map(|k| k == 0)
    }

    fn draw_quick_add(&self, painter: &Painter, origin: Pos2) {
        let Some((_, pts)) = self.quick_add_pts(origin) else { return };
        for c in pts {
            painter.circle(c, 9.0, Color32::WHITE, Stroke::new(1.0, BRAND));
            painter.line_segment([c - Vec2::new(4.0, 0.0), c + Vec2::new(4.0, 0.0)], Stroke::new(1.5, BRAND));
            painter.line_segment([c - Vec2::new(0.0, 4.0), c + Vec2::new(0.0, 4.0)], Stroke::new(1.5, BRAND));
        }
    }

    // a copy of the selected frame 100 px to its right or below, selected
    pub fn quick_add(&mut self, right: bool) {
        let Some(i) = self.sel.first().and_then(|&id| self.index_of(id)) else { return };
        let d = if right { Vec2::new(self.shapes[i].size.x + 100.0, 0.0) } else { Vec2::new(0.0, self.shapes[i].size.y + 100.0) };
        let shapes = self.selection_subtrees();
        self.insert_copies(shapes, d);
    }

    fn draw_selection(&self, painter: &Painter, origin: Pos2) {
        let line = Stroke::new(1.0, BRAND);
        let dev = self.mode == Mode::Dev;
        let handle = |p: Pos2, small: bool| {
            if dev {
                return;
            }
            let h = Rect::from_center_size(p, Vec2::splat(if small { HANDLE * 0.75 } else { HANDLE }));
            painter.rect(h, 0.0, Color32::WHITE, line, StrokeKind::Inside);
        };
        for id in self.sel_roots() {
            let Some(i) = self.index_of(id).filter(|&i| self.chain(i).iter().all(|&k| self.shapes[k].visible)) else { continue };
            let s = &self.shapes[i];
            let o = self.origin_of(s.parent);
            if s.kind == Kind::Line || s.kind == Kind::Group {
                painter.rect_stroke(self.screen_rect(origin, self.world_rect(i)), 0.0, line, StrokeKind::Outside);
            } else {
                let c: Vec<Pos2> = s.corners().iter().map(|&p| self.to_screen(origin, p + o)).collect();
                painter.add(egui::Shape::closed_line(c, line));
            }
        }
        match self.handles() {
            Handles::Line(i) => {
                let s = &self.shapes[i];
                let o = self.origin_of(s.parent);
                handle(self.to_screen(origin, s.pos + o), false);
                handle(self.to_screen(origin, s.pos + s.size + o), false);
            }
            Handles::Single(i) if !self.node_edit => {
                for (h, p) in self.single_handle_pts(i) {
                    handle(self.to_screen(origin, p), h.0 == 0 || h.1 == 0);
                }
                self.size_badge(painter, origin, self.world_rect(i), self.shapes[i].size);
            }
            Handles::Multi(b) => {
                painter.rect_stroke(self.screen_rect(origin, b), 0.0, line, StrokeKind::Outside);
                for (h, p) in Self::bounds_handle_pts(b) {
                    handle(self.to_screen(origin, p), h.0 == 0 || h.1 == 0);
                }
                self.size_badge(painter, origin, b, b.size());
            }
            _ => {}
        }
    }

    fn size_badge(&self, painter: &Painter, origin: Pos2, world: Rect, size: Vec2) {
        let r = self.screen_rect(origin, world);
        let galley = painter.layout_no_wrap(format!("{} × {}", size.x.round(), size.y.round()), FontId::proportional(10.0), Color32::WHITE);
        let at = Pos2::new(r.center().x - galley.size().x / 2.0, r.bottom() + 8.0);
        painter.rect_filled(Rect::from_min_size(at - Vec2::new(5.0, 2.0), galley.size() + Vec2::new(10.0, 4.0)), 3.0, BRAND);
        painter.galley(at, galley, Color32::WHITE);
    }

    fn draw_nodes(&self, painter: &Painter, origin: Pos2) {
        let Some(i) = self.single().filter(|&i| self.node_edit && self.shapes[i].kind == Kind::Path) else { return };
        let sp = |p: Pos2| self.to_screen(origin, p);
        for (c, contour) in self.node_points(i).iter().enumerate() {
            for (k, q) in contour.pts.iter().enumerate() {
                let selected = self.node_sel == Some((c, k));
                if selected {
                    for (h, on) in [(q.hin, true), (q.hout, true)] {
                        if h != Vec2::ZERO && on {
                            painter.line_segment([sp(q.p), sp(q.p + h)], Stroke::new(1.0, BRAND));
                            painter.circle(sp(q.p + h), 3.5, Color32::WHITE, Stroke::new(1.0, BRAND));
                        }
                    }
                }
                let r = Rect::from_center_size(sp(q.p), Vec2::splat(7.0));
                painter.rect(r, 0.0, if selected { BRAND } else { Color32::WHITE }, Stroke::new(1.0, BRAND), StrokeKind::Inside);
            }
        }
    }

    fn draw_pen(&self, painter: &Painter, origin: Pos2, pointer: Option<Pos2>) {
        if self.pen.is_empty() {
            return;
        }
        let mut pts = self.pen.clone();
        let st = Stroke::new(1.5, BRAND);
        let contour = Contour { pts: self.pen.clone(), closed: false, hole: false };
        let line: Vec<Pos2> = flatten(&contour, self.zoom).into_iter().map(|p| self.to_screen(origin, p)).collect();
        if line.len() >= 2 {
            painter.add(egui::Shape::line(line, st));
        }
        if let (Some(p), Some(last)) = (pointer, pts.last_mut()) {
            painter.line_segment([self.to_screen(origin, last.p), p], Stroke::new(1.0, BRAND.gamma_multiply(0.6)));
        }
        if let (Some(p), Some(first)) = (pointer, self.pen.first()) {
            let f = self.to_screen(origin, first.p);
            if self.pen.len() >= 2 && (p - f).length() <= 8.0 {
                painter.circle_stroke(f, 8.0, Stroke::new(2.0, BRAND));
            }
        }
        for q in &self.pen {
            painter.rect(Rect::from_center_size(self.to_screen(origin, q.p), Vec2::splat(6.0)), 0.0, Color32::WHITE, st, StrokeKind::Inside);
            for h in [q.hin, q.hout] {
                if h != Vec2::ZERO {
                    painter.circle(self.to_screen(origin, q.p + h), 3.0, Color32::WHITE, st);
                }
            }
        }
    }

    // keeps a text box in sync with what the TextEdit shows while it's being edited
    fn text_editor(&mut self, ctx: &egui::Context, origin: Pos2) {
        let Some(id) = self.editing_text else { return };
        let Some(i) = self.index_of(id) else {
            self.editing_text = None;
            return;
        };
        let s = self.shapes[i].clone();
        let o = self.origin_of(s.parent);
        let top_left = self.to_screen(origin, s.pos + o);
        let mut text = s.text.clone();
        let font = FontId::new(s.font_size * self.zoom, if s.mono { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional });
        let color = self.text_color(&s);
        let mut done = false;
        let mut sel = None;
        let zoom = self.zoom;
        egui::Area::new(egui::Id::new("text_edit")).fixed_pos(top_left).order(egui::Order::Foreground).show(ctx, |ui| {
            let width = if s.fit == TextFit::AutoWidth { (s.size.x * self.zoom).max(24.0) + 64.0 } else { s.size.x * self.zoom };
            let auto_width = s.fit == TextFit::AutoWidth;
            // the editor draws the text with its mixed styles (no case, list markers or spacing: those would move the caret)
            let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                let mut tmp = s.clone();
                tmp.text = buf.as_str().to_owned();
                // an auto-width box never wraps while typing, whatever room the popup area offers
                let wrap = if auto_width { f32::INFINITY } else { wrap_width / zoom };
                ui.painter().layout_job(crate::textrun::text_job(ui.painter(), &tmp, zoom, color, wrap, true))
            };
            // the area remembers its first (tiny) size and would offer only that much room: give the editor its own rect
            let room = Rect::from_min_size(ui.next_widget_position(), Vec2::new(width, f32::INFINITY));
            let out = ui.scope_builder(egui::UiBuilder::new().max_rect(room), |ui| egui::TextEdit::multiline(&mut text)
                .font(font)
                .text_color(color)
                .frame(egui::Frame::NONE)
                .desired_rows(1)
                .desired_width(width)
                .margin(Vec2::ZERO)
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(ui)).inner;
            let r = out.response;
            if let Some(cr) = out.cursor_range {
                sel = Some((cr.primary.index.0.min(cr.secondary.index.0), cr.primary.index.0.max(cr.secondary.index.0)));
            }
            if !r.has_focus() {
                r.request_focus();
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                done = true;
            }
        });
        if text != s.text {
            self.shapes[i].text = text.clone();
            self.shapes[i].shift_runs(&s.text, &text);
        }
        self.text_sel = sel.filter(|(a, b)| a != b).map(|(a, b)| (id, a, b));
        // a click on the canvas that isn't the editor ends editing (the panels keep it, so they can style the selection)
        let view = self.view;
        let clicked_away = ctx.input(|i| i.pointer.any_pressed()) && ctx.input(|i| i.pointer.press_origin()).is_some_and(|p| {
            let w = if s.fit == TextFit::AutoWidth { s.size.x * self.zoom + 16.0 } else { s.size.x * self.zoom };
            view.contains(p) && !Rect::from_min_size(top_left, Vec2::new(w, s.size.y * self.zoom + 4.0)).contains(p)
        });
        if done || clicked_away {
            self.editing_text = None;
            if self.shapes[i].text.is_empty() {
                self.remove_subtree(id);
            }
        }
    }

    // the right-click menu: Figma's rows (icon, name, shortcut) hugging the widest one; submenus are flat groups between rules
    pub(crate) fn canvas_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        use crate::glyphs::{Glyph, menu_item, menu_sep, menu_submenu};
        let has = !self.sel.is_empty();
        let many = self.sel.len() > 1;
        if menu_item(ui, Some(Glyph::Copy), "Copy", "Ctrl+C", has) {
            self.copy(ctx);
            ui.close();
        }
        if menu_item(ui, None, "Copy as SVG", "", has) {
            self.copy_as_svg(ctx);
            ui.close();
        }
        if menu_item(ui, None, "Copy as CSS", "", has) {
            self.copy_as_css(ctx);
            ui.close();
        }
        if menu_item(ui, None, "Copy as PNG", "Ctrl+Shift+C", has) {
            self.copy_as_png(ctx);
            ui.close();
        }
        if menu_item(ui, None, "Paste", "Ctrl+V", !self.clipboard.is_empty()) {
            self.paste_internal();
            ui.close();
        }
        if menu_item(ui, None, "Paste into selection", "Ctrl+Shift+V", !self.clipboard.is_empty() && self.single().is_some_and(|i| self.shapes[i].kind.is_container())) {
            self.paste_into_selection();
            ui.close();
        }
        if menu_item(ui, None, "Copy properties", "Ctrl+Alt+C", self.single().is_some()) {
            self.copy_properties();
            ui.close();
        }
        if menu_item(ui, None, "Paste properties", "Ctrl+Alt+V", has && self.props_clip.is_some()) {
            self.paste_properties();
            ui.close();
        }
        if menu_item(ui, None, "Duplicate", "Ctrl+D", has) {
            self.duplicate();
            ui.close();
        }
        if menu_item(ui, None, "Select matching layers", "Ctrl+Alt+A", has) {
            self.select_matching();
            ui.close();
        }
        menu_sep(ui);
        menu_submenu(ui, Some(Glyph::Forward), "Arrange", |ui| {
            if menu_item(ui, Some(Glyph::Forward), "Bring forward", "Ctrl+]", has) {
                self.sel_roots().iter().rev().for_each(|&id| self.move_z(id, true));
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Back), "Send backward", "Ctrl+[", has) {
                self.sel_roots().iter().for_each(|&id| self.move_z(id, false));
                ui.close();
            }
        });
        menu_sep(ui);
        if menu_item(ui, Some(Glyph::Group), "Group selection", "Ctrl+G", has) {
            self.group();
            ui.close();
        }
        if menu_item(ui, Some(Glyph::Ungroup), "Ungroup", "Ctrl+Shift+G", has) {
            self.ungroup();
            ui.close();
        }
        if menu_item(ui, Some(Glyph::Frame), "Frame selection", "Ctrl+Alt+G", has) {
            self.frame_selection();
            ui.close();
        }
        if menu_item(ui, Some(Glyph::AutoLayout), "Add auto layout", "Shift+A", has) {
            self.toggle_auto_layout();
            ui.close();
        }
        if menu_item(ui, Some(Glyph::Component), "Create component", "Ctrl+Alt+K", has) {
            self.make_component();
            ui.close();
        }
        if menu_item(ui, Some(Glyph::Mask), "Use as mask", "Ctrl+Alt+M", has) {
            self.toggle_mask();
            ui.close();
        }
        if menu_item(ui, Some(Glyph::Flatten), "Flatten", "Ctrl+E", has) {
            self.flatten();
            ui.close();
        }
        let on_path = self.sel.iter().filter_map(|&id| self.index_of(id)).any(|i| self.shapes[i].text_path.is_some());
        if on_path {
            if menu_item(ui, Some(Glyph::Type), "Detach text from path", "", true) {
                self.detach_text_path();
                ui.close();
            }
        } else if menu_item(ui, Some(Glyph::Type), "Text on path", "", many) {
            self.text_on_path();
            ui.close();
        }
        menu_sep(ui);
        menu_submenu(ui, Some(Glyph::Union), "Boolean groups", |ui| {
            for (g, label, op) in [(Glyph::Union, "Union selection", BoolOp::Union), (Glyph::Subtract, "Subtract selection", BoolOp::Subtract), (Glyph::Intersect, "Intersect selection", BoolOp::Intersect), (Glyph::Exclude, "Exclude selection", BoolOp::Exclude)] {
                if menu_item(ui, Some(g), label, "", many) {
                    self.boolean(op);
                    ui.close();
                }
            }
        });
        menu_submenu(ui, Some(Glyph::FlipH), "Flip", |ui| {
            if menu_item(ui, Some(Glyph::FlipH), "Flip horizontal", "Shift+H", has) {
                self.flip(true);
                ui.close();
            }
            if menu_item(ui, Some(Glyph::FlipV), "Flip vertical", "Shift+V", has) {
                self.flip(false);
                ui.close();
            }
        });
        menu_sep(ui);
        let hidden = has && self.sel.iter().filter_map(|&id| self.index_of(id)).all(|i| !self.shapes[i].visible);
        let locked = has && self.sel.iter().filter_map(|&id| self.index_of(id)).all(|i| self.shapes[i].locked);
        if menu_item(ui, Some(if hidden { Glyph::Eye } else { Glyph::EyeOff }), if hidden { "Show" } else { "Hide" }, "Ctrl+Shift+H", has) {
            self.toggle_hidden_selection();
            ui.close();
        }
        if menu_item(ui, Some(if locked { Glyph::LockOpen } else { Glyph::Lock }), if locked { "Unlock" } else { "Lock" }, "Ctrl+Shift+L", has) {
            self.toggle_locked_selection();
            ui.close();
        }
        menu_sep(ui);
        if menu_item(ui, Some(Glyph::Delete), "Delete", "Del", has) {
            for id in self.sel.clone() {
                self.remove_subtree(id);
            }
            ui.close();
        }
    }

    pub fn canvas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let origin = resp.rect.min;
        self.view = resp.rect;
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Canvas"));
        painter.rect_filled(resp.rect, 0.0, self.page_bg());
        self.prepare_textures(&ctx);
        if crate::textrun::ensure_fonts(&ctx, &self.shapes) {
            ctx.request_repaint();
        }
        self.snap_lines.clear();
        self.fit_texts(&painter);
        self.settle();

        if self.present.is_some() {
            self.canvas_present(ui, &resp, &painter, origin);
            return;
        }
        if self.node_edit && !self.single().is_some_and(|i| self.shapes[i].kind == Kind::Path) {
            self.node_edit = false;
        }

        // pan (scroll / middle drag) and zoom (ctrl+scroll / pinch, around the cursor)
        if resp.hovered() {
            let scroll = wheel_delta(ui);
            let (zoom_d, hover) = ui.input(|i| (i.zoom_delta(), i.pointer.hover_pos()));
            self.pan += scroll;
            if let (true, Some(h)) = (zoom_d != 1.0, hover) {
                let w = self.to_world(origin, h);
                self.zoom = (self.zoom * zoom_d).clamp(0.05, 64.0);
                self.pan = h - origin - w.to_vec2() * self.zoom;
            }
        }
        // holding Space turns any tool into the Hand tool (not while a text field has the keyboard)
        let space_pan = self.editing_text.is_none() && !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_down(egui::Key::Space));
        if (self.tool == Tool::Hand || space_pan) && resp.dragged_by(PointerButton::Primary) {
            self.pan += resp.drag_delta();
        }
        if self.tool == Tool::Hand || (space_pan && resp.hovered()) {
            ui.ctx().set_cursor_icon(if resp.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab });
        }
        if resp.dragged_by(PointerButton::Middle) {
            self.pan += resp.drag_delta();
        }

        let (shift, hover) = ui.input(|i| (i.modifiers.shift, i.pointer.hover_pos()));
        let ruler = if self.rulers { RULER } else { 0.0 };
        let in_top = |p: Pos2| p.y < origin.y + ruler && p.x >= origin.x + ruler;
        let in_left = |p: Pos2| p.x < origin.x + ruler && p.y >= origin.y + ruler;

        // cursor feedback
        if let Some(h) = hover.filter(|_| self.tool == Tool::Select && matches!(self.drag, Drag::None) && self.mode != Mode::Dev) {
            if let Handles::Single(i) = self.handles() {
                if let Some((hd, _)) = self.single_handle_pts(i).into_iter().find(|(_, p)| (self.to_screen(origin, *p) - h).length() <= HANDLE) {
                    ui.ctx().set_cursor_icon(Self::cursor_for(hd));
                }
            } else if let Handles::Multi(b) = self.handles() {
                if let Some((hd, _)) = Self::bounds_handle_pts(b).into_iter().find(|(_, p)| (self.to_screen(origin, *p) - h).length() <= HANDLE) {
                    ui.ctx().set_cursor_icon(Self::cursor_for(hd));
                }
            }
        }
        if let Some(c) = hover.filter(|_| matches!(self.drag, Drag::None)).and_then(|h| self.layout_cursor(origin, h)) {
            ui.ctx().set_cursor_icon(c);
        }
        if matches!(self.drag, Drag::Layout(_)) {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }
        if matches!(self.tool, Tool::Draw(_) | Tool::Pen | Tool::Pencil | Tool::Comment) {
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        }

        self.sync_grad_edit(&ctx);

        // primary button: select / move / resize / rotate / draw / pen / marquee / guides
        let press = ui.input(|i| i.pointer.press_origin());
        let pointer_world = resp.interact_pointer_pos().or(hover).map(|p| self.to_world(origin, p));
        let aiming = crate::picker::eyedropper_active(&ctx);
        if aiming {
            self.drag = Drag::None;
        }
        // the "+" beside a selected top-level frame duplicates it to the right or below
        let quick = resp.clicked() && !aiming && resp.interact_pointer_pos().and_then(|p| self.quick_add_hit(origin, p)).map(|right| self.quick_add(right)).is_some();
        if resp.drag_started_by(PointerButton::Primary) && !aiming {
            if let Some(press) = press {
                if self.editing_text.is_some() {
                    self.editing_text = None;
                }
                let w = self.to_world(origin, press);
                // a press anywhere but on a gradient handle ends gradient editing
                let grad = self.grad_hit(origin, press);
                if grad.is_none() {
                    self.grad_edit = None;
                }
                self.drag = match if space_pan { Tool::Hand } else { self.tool } {
                    Tool::Comment | Tool::Hand => Drag::None,
                    Tool::Pen => {
                        // pressing on the first point closes the path on release; dragging from there pulls its handles
                        // out first, so the closing corner can be round
                        if self.pen_hits_start(w) {
                            Drag::PenClose
                        } else {
                            self.pen_click(w);
                            Drag::PenHandle
                        }
                    }
                    Tool::Draw(kind) => {
                        self.add(kind, w);
                        if kind == Kind::Line && self.arrow_tool {
                            if let Some(s) = self.shapes.last_mut() {
                                s.stroke_cap[1] = StrokeCap::ArrowLines;
                            }
                        }
                        if kind == Kind::Frame && self.section_tool {
                            let n = self.shapes.iter().filter(|s| s.section).count() + 1;
                            if let Some(s) = self.shapes.last_mut() {
                                s.section = true;
                                s.clip = false;
                                s.name = format!("Section {n}");
                                s.fills = vec![Paint::solid(Color32::WHITE)];
                                s.strokes = vec![Paint::solid(Color32::from_gray(0xe6))];
                                s.stroke_w = 2.0;
                                s.stroke_align = StrokeAlign::Inside;
                                s.radius = [12.0; 4];
                            }
                        }
                        if kind == Kind::Rect && self.slice_tool {
                            let n = self.shapes.iter().filter(|s| s.slice).count() + 1;
                            if let Some(s) = self.shapes.last_mut() {
                                s.slice = true;
                                s.name = format!("Slice {n}");
                                s.fills.clear();
                            }
                        }
                        Drag::Corners(w)
                    }
                    Tool::Pencil => {
                        self.pencil = vec![w];
                        self.pencil_force = vec![pen_force(ui.ctx()).unwrap_or(0.5)];
                        Drag::Pencil
                    }
                    // Dev mode is read-only: a press selects (or starts a marquee), nothing moves
                    Tool::Select if self.mode == Mode::Dev => {
                        if !shift {
                            self.sel.clear();
                        }
                        match self.hit_test(w).map(|id| self.select_unit(id)) {
                            Some(id) => {
                                self.sel.push(id);
                                Drag::None
                            }
                            None => {
                                self.base_sel = self.sel.clone();
                                Drag::Marquee(w)
                            }
                        }
                    }
                    Tool::Select => {
                        let guide = self.guides.iter().position(|g| {
                            let d = if g.vertical { (self.to_screen(origin, Pos2::new(g.pos, 0.0)).x - press.x).abs() } else { (self.to_screen(origin, Pos2::new(0.0, g.pos)).y - press.y).abs() };
                            d <= 4.0
                        });
                        if self.rulers && (in_top(press) || in_left(press)) {
                            self.guides.push(Guide { vertical: in_left(press), pos: if in_left(press) { w.x } else { w.y } });
                            Drag::Guide(self.guides.len() - 1)
                        } else if let Some(g) = guide {
                            Drag::Guide(g)
                        } else if let Some((c, k, part)) = self.node_hit(origin, press).filter(|_| self.node_edit) {
                            let alt = ui.input(|i| i.modifiers.alt);
                            if let (Some(from), 0) = (self.node_connect.take(), part) {
                                self.node_connect_to(from, (c, k));
                                self.node_sel = Some((c, k));
                                Drag::None
                            } else if { self.node_sel = Some((c, k)); alt && part == 0 } {
                                // Alt+drag from a vertex starts a branch that shares it
                                match self.node_add_branch(c, k) {
                                    Some(n) => Drag::Node { c: n, i: 1, part: 0 },
                                    None => Drag::Node { c, i: k, part },
                                }
                            } else if self.bend_tool && part == 0 {
                                self.bend_moved = false;
                                Drag::Bend { c, i: k, from: w }
                            } else {
                                Drag::Node { c, i: k, part }
                            }
                        } else if let Some((code, idx)) = grad {
                            if code == crate::gradient_edit::G_STOP {
                                self.grad_click(origin, press);
                            }
                            Drag::Gradient(code, idx)
                        } else if let Some(d) = self.connect_node_at(origin, press) {
                            d
                        } else if let Some(d) = self.smart_handle_at(origin, press, ui.input(|i| i.modifiers.command)) {
                            d
                        } else if let Some(d) = self.radius_handle_at(origin, press) {
                            d
                        } else if let Some(d) = self.handle_drag_at(origin, press) {
                            d
                        } else if let Some(d) = self.layout_handle_at(origin, press) {
                            d
                        } else if let Some(d) = self.crop_handle_at(origin, press) {
                            d
                        } else if self.vtool.is_some() {
                            self.vtool_pts = vec![w];
                            if matches!(self.vtool, Some(crate::vtools::VTool::Builder | crate::vtools::VTool::Paint)) {
                                self.builder_touch(w, ui.input(|i| i.modifiers.alt));
                            }
                            Drag::VTool
                        } else if self.lasso_tool {
                            if !shift {
                                self.sel.clear();
                            }
                            self.base_sel = self.sel.clone();
                            self.lasso = vec![w];
                            Drag::Lasso
                        } else if let Some(id) = self.comment_hit(origin, press) {
                            self.comment_sel = Some(id);
                            Drag::None
                        } else if let Some(id) = self.hit_test(w).map(|id| self.select_unit(id)) {
                            self.comment_sel = None;
                            if !self.sel.contains(&id) {
                                if !shift {
                                    self.sel.clear();
                                }
                                self.sel.push(id);
                            }
                            Drag::Move { raw: Vec2::ZERO, bounds: self.sel_bounds().unwrap_or(Rect::NOTHING) }
                        } else {
                            if !shift {
                                self.sel.clear();
                            }
                            self.base_sel = self.sel.clone();
                            Drag::Marquee(w)
                        }
                    }
                };
            }
        } else if resp.clicked() && !aiming && !quick {
            if let Some(p) = resp.interact_pointer_pos() {
                let w = self.to_world(origin, p);
                match self.tool {
                    Tool::Select if self.eyedropper => {
                        if let Some(c) = self.color_at(w) {
                            self.apply_picked(c, &ctx);
                        }
                        self.eyedropper = false;
                    }
                    // Dev Mode: Alt+click another layer keeps the distance from the selection to it on the canvas
                    Tool::Select if self.mode == Mode::Dev && ui.input(|i| i.modifiers.alt) && self.sel.len() == 1 && self.hit_test(w).map(|id| self.select_unit(id)).is_some_and(|id| !self.sel.contains(&id)) => {
                        if let Some(id) = self.hit_test(w).map(|id| self.select_unit(id)) {
                            self.pin_measurement(id);
                        }
                    }
                    Tool::Select => {
                        let edge = resp.double_clicked() && self.mode != Mode::Dev && self.edge_double_click(origin, p, ui.input(|i| i.modifiers.alt));
                        if !edge && !self.grad_click(origin, p) {
                            self.grad_edit = None;
                            self.click_select(origin, p, w, shift, resp.double_clicked() && self.mode != Mode::Dev);
                        }
                    }
                    Tool::Draw(Kind::Text) => {
                        self.add(Kind::Text, w);
                        self.editing_text = self.sel.first().copied();
                        if let Some(i) = self.single() {
                            self.shapes[i].text.clear();
                        }
                        self.tool = Tool::Select;
                    }
                    Tool::Draw(_) | Tool::Pencil | Tool::Hand => {}
                    Tool::Pen => {
                        if resp.double_clicked() || resp.triple_clicked() {
                            // the first click of the pair already placed the last point
                            self.finish_pen();
                        } else {
                            self.pen_click(w);
                        }
                    }
                    Tool::Comment => {
                        // a click on a pin opens it; anywhere else (re)starts the composer there
                        if let Some(id) = self.comment_hit(origin, resp.interact_pointer_pos().unwrap_or_default()) {
                            self.comment_sel = Some(id);
                            self.new_comment = None;
                            self.tool = Tool::Select;
                        } else {
                            self.comment_sel = None;
                            let text = self.new_comment.take().map(|(_, t)| t).unwrap_or_default();
                            self.new_comment = Some((w, text));
                        }
                    }
                }
            }
        }
        if resp.dragged_by(PointerButton::Primary) {
            self.drag_update(origin, &resp, pointer_world, shift);
        }
        if resp.drag_stopped() {
            // holding Space while moving keeps the layers in the frame they came from (Figma)
            let keep_parent = ui.input(|i| i.key_down(egui::Key::Space));
            self.drag_finish(origin, hover, keep_parent);
        }

        // right-click: pick what's under the pointer, then offer the commands
        if resp.secondary_clicked() {
            if let Some(w) = hover.map(|p| self.to_world(origin, p)) {
                if let Some(id) = self.hit_test(w).map(|id| self.select_unit(id)) {
                    if !self.sel.contains(&id) {
                        self.sel = vec![id];
                    }
                }
            }
        }
        if self.mode != Mode::Dev {
            resp.context_menu(|ui| self.canvas_menu(ui, &ctx));
        }

        // draw
        let painter = painter.with_clip_rect(resp.rect);
        // blend modes and masks of any shape other than a plain box (whose bounding-box clip is exact) need the page rendered as SVG
        if self.shapes.iter().any(|s| s.visible && (s.shaders.iter().any(|x| x.visible) || !s.blend.is_normal() || s.fills.iter().chain(&s.strokes).any(|p| p.visible && !p.blend.is_normal()) || s.effects.iter().any(|e| e.visible && !e.blend.is_normal()) || (s.kind.is_container() && s.effects.iter().any(|e| e.visible && e.kind == EffectKind::LayerBlur)) || (s.mask && !(matches!(s.kind, Kind::Rect | Kind::Frame) && s.rotation == 0.0 && s.radius.iter().all(|&r| r == 0.0))))) {
            self.draw_composited(&ctx, &painter, resp.rect);
        } else {
            self.draw_level(&painter, origin, None, Vec2::ZERO, 1.0);
        }
        if self.outlines {
            self.draw_outlines(&painter, resp.rect, origin);
        }
        self.draw_layout_guides(&painter, origin);
        self.draw_pixel_grid(&painter, resp.rect, origin);
        // hover outline, like Figma: shows what a click would pick
        if self.tool == Tool::Select && matches!(self.drag, Drag::None) && !self.node_edit && resp.hovered() {
            let over_ruler = hover.is_some_and(|p| self.rulers && (in_top(p) || in_left(p)));
            if let (Some(h), false) = (hover, over_ruler) {
                if let Some(i) = self.hit_test(self.to_world(origin, h)).map(|id| self.select_unit(id)).and_then(|id| self.index_of(id)) {
                    if !self.sel.contains(&self.shapes[i].id) {
                        self.outline_shape(&painter, origin, i, Stroke::new(1.5, BRAND.gamma_multiply(0.85)));
                    }
                }
            }
        }
        for g in &self.guides {
            let (a, b) = if g.vertical {
                let x = self.to_screen(origin, Pos2::new(g.pos, 0.0)).x;
                (Pos2::new(x, resp.rect.top()), Pos2::new(x, resp.rect.bottom()))
            } else {
                let y = self.to_screen(origin, Pos2::new(0.0, g.pos)).y;
                (Pos2::new(resp.rect.left(), y), Pos2::new(resp.rect.right(), y))
            };
            painter.line_segment([a, b], Stroke::new(1.0, GUIDE.gamma_multiply(0.7)));
        }
        self.draw_selection(&painter, origin);
        self.draw_vtool(&painter, origin, hover);
        self.draw_quick_add(&painter, origin);
        self.draw_radius_handles(&painter, origin);
        self.draw_smart_selection(&painter, origin, hover);
        if let Some(r) = self.kbox {
            painter.rect(self.screen_rect(origin, r), 0.0, BRAND.gamma_multiply(0.12), Stroke::new(1.0, BRAND), StrokeKind::Inside);
        }
        self.draw_layout_handles(&painter, origin, hover, ui.input(|i| i.modifiers.alt));
        self.draw_crop_handles(&painter, origin);
        self.draw_gradient_handles(&painter, origin);
        self.draw_nodes(&painter, origin);
        self.draw_links(&painter, origin);
        self.draw_connect_node(&painter, origin, hover);
        self.draw_pen(&painter, origin, hover);
        if matches!(self.drag, Drag::Pencil) && self.pencil.len() >= 2 {
            use crate::vector::{brush_outline, brush_widths, scatter, Brush};
            if self.brush == Brush::Scatter {
                for c in scatter(&self.pencil, &self.pencil_force, self.brush_w) {
                    let (min, max) = c.pts.iter().fold((c.pts[0].p, c.pts[0].p), |(a, b), q| (a.min(q.p), b.max(q.p)));
                    painter.circle_filled(self.to_screen(origin, min + (max - min) / 2.0), (max.x - min.x) / 2.0 * self.zoom, GREY_100);
                }
            } else if self.brush != Brush::Pencil {
                let widths = brush_widths(self.brush, &self.pencil, &self.pencil_force, self.nib);
                let outline = brush_outline(&self.pencil, &widths, self.brush_w);
                let pts: Vec<Pos2> = outline.iter().map(|&p| self.to_screen(origin, p)).collect();
                painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, GREY_100)));
            } else {
                let line: Vec<Pos2> = self.pencil.iter().map(|&p| self.to_screen(origin, p)).collect();
                painter.add(egui::Shape::line(line, Stroke::new((self.brush_w * self.zoom).max(1.0), GREY_100)));
            }
        }
        // Dev Mode always shows distances to the hovered layer; in Design mode holding Alt does, as in Figma
        let alt = ui.input(|i| i.modifiers.alt);
        if (self.mode == Mode::Dev || alt) && matches!(self.drag, Drag::None) {
            let over = hover.filter(|&h| resp.rect.contains(h)).and_then(|h| self.hit_test(self.to_world(origin, h)));
            if self.mode == Mode::Dev {
                self.draw_spacing(&painter, origin, over);
            }
            // pointing at nothing new (or the selection itself) with Alt shows the selection's distances to its parent
            let target = over.map(|id| self.select_unit(id)).filter(|id| !self.sel.contains(id)).or_else(|| if self.mode == Mode::Dev { None } else { self.single().and_then(|i| self.shapes[i].parent) });
            self.draw_redlines(&painter, origin, target);
        }
        if self.mode == Mode::Dev {
            self.draw_annotations(&painter, origin);
            self.draw_pinned_measurements(&painter, origin);
        }
        self.net_cursor = hover.filter(|&p| resp.rect.contains(p)).map(|p| self.to_world(origin, p));
        for g in &self.snap_lines {
            let (a, b) = if g.vertical {
                let x = self.to_screen(origin, Pos2::new(g.pos, 0.0)).x;
                (Pos2::new(x, resp.rect.top()), Pos2::new(x, resp.rect.bottom()))
            } else {
                let y = self.to_screen(origin, Pos2::new(0.0, g.pos)).y;
                (Pos2::new(resp.rect.left(), y), Pos2::new(resp.rect.right(), y))
            };
            painter.line_segment([a, b], Stroke::new(1.0, GUIDE));
        }
        if matches!(self.drag, Drag::Lasso) && self.lasso.len() > 1 {
            let pts: Vec<Pos2> = self.lasso.iter().map(|&p| self.to_screen(origin, p)).collect();
            painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, BRAND)));
        }
        if let (Drag::Marquee(start), Some(p)) = (self.drag, ui.input(|i| i.pointer.latest_pos())) {
            let r = Rect::from_two_pos(self.to_screen(origin, start), p);
            painter.rect(r, 0.0, BRAND.gamma_multiply(0.12), Stroke::new(1.0, BRAND), StrokeKind::Inside);
        }
        self.paint_comments(ui, &painter, origin, resp.rect);
        if self.rulers {
            self.draw_rulers(&painter, resp.rect);
        }
        self.text_editor(&ctx, origin);
        self.comment_popups(&ctx, origin, resp.rect);
    }

    // a click (no drag) with the select tool
    fn click_select(&mut self, origin: Pos2, p: Pos2, w: Pos2, shift: bool, double: bool) {
        if self.node_edit {
            if let Some((c, k, 0)) = self.node_hit(origin, p) {
                self.node_sel = Some((c, k));
                if double {
                    self.node_toggle_smooth(c, k);
                }
                return;
            }
            if double && self.node_insert_at(w) {
                return;
            }
        }
        if let Some(id) = self.comment_hit(origin, p) {
            self.comment_sel = Some(id);
            return;
        }
        self.comment_sel = None;
        // a click picks the outermost group; a double-click reaches inside it
        // a top-level frame is hit by its title only while pressing (so a drag from its body is a marquee), but a click
        // on its empty body selects it too
        let hit = self.hit_test(w).or_else(|| self.frame_at(w, &[])).map(|id| if double { id } else { self.select_unit(id) });
        match (hit, shift) {
            (Some(id), true) => match self.sel.iter().position(|&x| x == id) {
                Some(k) => {
                    self.sel.remove(k);
                }
                None => self.sel.push(id),
            },
            (Some(id), false) => {
                self.sel = vec![id];
                if double {
                    if let Some(i) = self.index_of(id) {
                        match self.shapes[i].kind {
                            Kind::Text => self.editing_text = Some(id),
                            Kind::Path => self.start_node_edit(i),
                            _ => {}
                        }
                    }
                }
            }
            (None, false) => self.sel.clear(),
            (None, true) => {}
        }
    }

    fn drag_update(&mut self, origin: Pos2, resp: &egui::Response, pointer: Option<Pos2>, shift: bool) {
        match self.drag {
            Drag::Pencil => {
                // one point per screen pixel of travel is plenty; finish_pencil simplifies anyway
                if let (Some(p), Some(&last)) = (pointer, self.pencil.last()) {
                    let step = (p - last).length() * self.zoom;
                    if step >= 1.0 {
                        // a pen reports its pressure; a mouse gets a pretend one: faster strokes are thinner
                        let prev = self.pencil_force.last().copied().unwrap_or(0.5);
                        let force = pen_force(&resp.ctx).or_else(|| crate::pen::latest(150).filter(|p| p.pressure > 0.0).map(|p| p.pressure.clamp(0.05, 1.0))).unwrap_or_else(|| 0.7 * prev + 0.3 * (1.1 - step / 40.0).clamp(0.2, 1.0));
                        // a pen that leans turns the Calligraphy nib toward the direction it leans
                        if let Some(pen) = crate::pen::latest(150).filter(|p| p.lean() > 8.0) {
                            if self.brush == crate::vector::Brush::Calligraphy {
                                self.nib = pen.azimuth();
                            }
                        }
                        self.pencil.push(p);
                        self.pencil_force.push(force);
                    }
                }
            }
            Drag::Move { raw, bounds } => {
                let mut raw = raw + resp.drag_delta() / self.zoom;
                if shift {
                    if raw.x.abs() > raw.y.abs() { raw.y = 0.0 } else { raw.x = 0.0 }
                }
                let roots = self.sel_roots();
                let off = self.snap_rect(bounds.translate(raw), &roots);
                let target = bounds.translate(raw + off);
                if let Some(cur) = self.sel_bounds() {
                    let d = target.min - cur.min;
                    for id in roots {
                        if let Some(i) = self.index_of(id).filter(|&i| !self.shapes[i].locked) {
                            self.shapes[i].pos += d;
                        }
                    }
                }
                self.drag = Drag::Move { raw, bounds };
            }
            Drag::Corners(fixed) => {
                if let (Some(i), Some(w)) = (self.single(), pointer) {
                    let ids = [self.shapes[i].id];
                    let mut w = self.snap_pos(w, &ids);
                    let is_line = self.shapes[i].kind == Kind::Line;
                    if shift {
                        let d = w - fixed;
                        w = if is_line {
                            // snap the angle to 45 degree steps
                            let a = (d.angle() / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
                            fixed + Vec2::angled(a) * d.length()
                        } else {
                            let m = d.x.abs().max(d.y.abs());
                            fixed + Vec2::new(m * d.x.signum(), m * d.y.signum())
                        };
                    }
                    let o = self.origin_of(self.shapes[i].parent);
                    self.shapes[i].set_corners(fixed - o, w - o);
                }
            }
            Drag::Resize { h, rect } => {
                if let (Some(i), Some(w)) = (self.single(), pointer) {
                    let ids = [self.shapes[i].id];
                    let w = self.snap_pos(w, &ids);
                    self.do_resize(i, h, rect, w, shift);
                }
            }
            Drag::Scale { h, bounds } => {
                if let Some(w) = pointer {
                    self.do_scale(h, bounds, w, shift);
                }
            }
            Drag::Gradient(code, idx) => {
                if let Some(w) = pointer {
                    self.grad_drag(code, idx, w);
                }
            }
            Drag::Connect(_) => ui_repaint(resp),
            Drag::Layout(d) => {
                if let Some(w) = pointer {
                    let alt = resp.ctx.input(|i| i.modifiers.alt);
                    self.layout_drag(d, w, shift, alt);
                }
            }
            Drag::Rotate { center, start, base } => {
                if let (Some(i), Some(w)) = (self.single(), pointer) {
                    let mut deg = base + ((w - center).angle() - start).to_degrees();
                    if shift {
                        deg = (deg / 15.0).round() * 15.0;
                    }
                    self.set_rotation(i, deg);
                }
            }
            Drag::Marquee(start) => {
                if let Some(w) = pointer {
                    let mut sel = self.base_sel.clone();
                    for id in self.marquee_hits(Rect::from_two_pos(start, w)) {
                        if !sel.contains(&id) {
                            sel.push(id);
                        }
                    }
                    self.sel = sel;
                }
            }
            Drag::Crop(d) => {
                if let Some(w) = pointer {
                    self.crop_drag(d, w);
                }
            }
            Drag::Smart { vertical, start, from } => {
                if let Some(w) = pointer {
                    self.smart_gap_drag(vertical, start, from, w);
                }
            }
            Drag::SmartMove { vertical, id, grab } => {
                if let Some(w) = pointer {
                    self.smart_move_drag(vertical, id, grab, w);
                }
            }
            Drag::Radius { k } => {
                if let Some(w) = pointer {
                    self.radius_drag(k, resp.ctx.input(|i| i.modifiers.alt), w);
                }
            }
            Drag::VTool => {
                if let Some(w) = pointer {
                    match self.vtool {
                        Some(crate::vtools::VTool::Builder) => self.builder_touch(w, resp.ctx.input(|i| i.modifiers.alt)),
                        Some(crate::vtools::VTool::Cut) => self.vtool_pts.truncate(1),
                        _ => {}
                    }
                    if matches!(self.vtool, Some(crate::vtools::VTool::Cut | crate::vtools::VTool::Eraser)) {
                        self.vtool_pts.push(w);
                    }
                }
            }
            Drag::Lasso => {
                if let Some(w) = pointer {
                    if self.lasso.last().is_none_or(|l| (*l - w).length() * self.zoom > 2.0) {
                        self.lasso.push(w);
                    }
                    let mut sel = self.base_sel.clone();
                    for id in self.lasso_hits(&self.lasso.clone()) {
                        if !sel.contains(&id) {
                            sel.push(id);
                        }
                    }
                    self.sel = sel;
                }
            }
            Drag::Guide(g) => {
                if let (Some(w), Some(guide)) = (pointer, self.guides.get_mut(g)) {
                    guide.pos = if guide.vertical { w.x } else { w.y };
                }
            }
            Drag::PenHandle => {
                if let Some(w) = pointer {
                    self.pen_drag(w);
                }
            }
            Drag::PenClose => {
                if let Some(w) = pointer {
                    self.pen_close_drag(w);
                }
            }
            Drag::Bend { c, i, from } => {
                if let Some(w) = pointer {
                    if (w - from).length() * self.zoom > 3.0 {
                        self.bend_moved = true;
                    }
                    if self.bend_moved {
                        self.node_bend(c, i, w);
                    }
                }
            }
            Drag::Node { c, i, part } => {
                if let Some(w) = pointer {
                    let w = if part == 0 { self.snap_pos(w, &[]) } else { w };
                    self.node_move(c, i, part, w);
                }
            }
            Drag::None => {}
        }
        let _ = origin;
    }

    fn drag_finish(&mut self, origin: Pos2, hover: Option<Pos2>, keep_parent: bool) {
        let drag = self.drag;
        self.drag = Drag::None;
        match drag {
            Drag::Bend { c, i, .. } => {
                // a click that did not drag straightens a curved vertex
                if !self.bend_moved && self.node_is_curved(c, i) {
                    self.node_straighten(c, i);
                }
            }
            Drag::Connect(id) => {
                if let Some(p) = hover {
                    self.finish_connect(id, self.to_world(origin, p));
                }
            }
            Drag::Move { .. } => {
                // dropping moved shapes onto a frame (or empty canvas) reparents them
                if let Some(p) = hover.filter(|_| !keep_parent) {
                    let ids = self.sel_roots();
                    let target = self.frame_at(self.to_world(origin, p), &ids);
                    for id in ids {
                        // moving inside the frame the shape already lives in (even via a group) changes nothing
                        if self.container_frame(id) != target {
                            self.reparent(id, target);
                        }
                    }
                }
            }
            Drag::SmartMove { vertical, .. } => self.smart_move_finish(vertical),
            Drag::VTool => {
                let pts = std::mem::take(&mut self.vtool_pts);
                match self.vtool {
                    Some(crate::vtools::VTool::Cut) if pts.len() > 1 => self.cut_along(pts[0], *pts.last().unwrap()),
                    Some(crate::vtools::VTool::Eraser) => self.erase_along(&pts),
                    _ => {}
                }
            }
            Drag::Guide(g) => {
                // dragged back onto a ruler: delete it
                if let (Some(p), Some(_)) = (hover, self.guides.get(g)) {
                    let r = self.view;
                    if p.y < r.top() + RULER || p.x < r.left() + RULER {
                        self.guides.remove(g);
                    }
                }
            }
            Drag::PenHandle => return, // the pen tool stays active
            Drag::PenClose => {
                self.finish_pen_closed();
                return;
            }
            Drag::Pencil => {
                self.finish_pencil();
                return; // the pencil stays active
            }
            Drag::Corners(_) => {
                // a plain click with a draw tool makes a default-sized shape; text boxes dragged out wrap
                if let (Tool::Draw(kind), Some(i)) = (self.tool, self.single()) {
                    let s = &mut self.shapes[i];
                    if kind != Kind::Line && kind != Kind::Text && s.size.length() < 2.0 {
                        s.size = Vec2::splat(100.0);
                    }
                    if kind == Kind::Line && s.size.length() < 2.0 {
                        s.size = Vec2::new(100.0, 0.0);
                    }
                    if kind == Kind::Text {
                        s.text.clear();
                        // only a real drag makes a wrapping box; a click (even a shaky one) makes one that grows with what is typed
                        s.fit = if s.size.x * self.zoom > 12.0 { TextFit::AutoHeight } else { TextFit::AutoWidth };
                        self.editing_text = Some(s.id);
                    }
                }
            }
            _ => {}
        }
        if !matches!(self.tool, Tool::Pen) {
            self.tool = Tool::Select;
        }
    }
}

// where the outgoing and incoming frames are, `t` of the way (0..1, linear time) through a transition;
// offsets are in screen pixels relative to the presented area of size `size`
#[derive(Debug, PartialEq)]
pub struct TransitionFrame {
    pub from_offset: Vec2,
    pub from_alpha: f32,
    pub to_offset: Vec2,
    pub to_alpha: f32,
}

pub fn transition_frame(tr: Transition, ease: Easing, t: f32, size: Vec2) -> TransitionFrame {
    let e = ease.at(t);
    let fade = e.clamp(0.0, 1.0);
    // direction the content moves in
    let dir = match tr {
        Transition::SlideLeft => Vec2::new(-size.x, 0.0),
        Transition::SlideRight => Vec2::new(size.x, 0.0),
        Transition::SlideUp => Vec2::new(0.0, -size.y),
        Transition::SlideDown => Vec2::new(0.0, size.y),
        Transition::Instant | Transition::Dissolve | Transition::SmartAnimate => Vec2::ZERO,
    };
    match tr {
        Transition::Instant => TransitionFrame { from_offset: Vec2::ZERO, from_alpha: 0.0, to_offset: Vec2::ZERO, to_alpha: 1.0 },
        // Smart Animate is drawn by `smart_tween`; on its own it is a dissolve
        Transition::Dissolve | Transition::SmartAnimate => TransitionFrame { from_offset: Vec2::ZERO, from_alpha: 1.0 - fade, to_offset: Vec2::ZERO, to_alpha: fade },
        // push: the new frame comes in from the opposite side while the old one leaves
        _ => TransitionFrame { from_offset: dir * e, from_alpha: 1.0, to_offset: dir * (e - 1.0), to_alpha: 1.0 },
    }
}

// every layer under `root` with its name path below the root (names of its ancestors, then its own)
fn name_paths(shapes: &[Shape], root: u64) -> Vec<(usize, Vec<&str>)> {
    let mut out = vec![];
    let mut stack: Vec<(u64, Vec<&str>)> = vec![(root, vec![])];
    while let Some((p, path)) = stack.pop() {
        for (k, s) in shapes.iter().enumerate().filter(|(_, s)| s.parent == Some(p)) {
            let mut mine = path.clone();
            mine.push(s.name.as_str());
            if s.kind.is_container() {
                stack.push((s.id, mine.clone()));
            }
            out.push((k, mine));
        }
    }
    out
}

/// Wheel smoothing state, kept in egui's temp memory between frames.
#[derive(Clone, Default)]
struct WheelQueue {
    live: Vec2,       // finger-phase deltas not yet applied
    glide: Vec2,      // inertia flush not yet applied (first easing stage)
    glide2: Vec2,     // the flush after the first stage (second stage, so the glide eases in)
    vel: Vec2,        // applied finger speed, px/s
    coasted: Vec2,    // moved while waiting for the flush; taken off it when it lands
    last_small: f64,  // time of the last finger event
}

/// Scroll delta for this frame. Windows delivers a touchpad swipe as a ramp of small line events
/// while the fingers are down, then (after a ~300 ms pause) the whole inertia as ONE huge event.
/// Applied as is, or eased briefly, that reads as a stop followed by a lurch. So small events are
/// queued and paid out almost at once, the view keeps coasting from the finger speed the moment the
/// fingers stop, and the flush is paid out as a glide that eases in, minus what was already coasted.
/// Everything is paid by frame time, so the motion is even whatever the frame rate.
fn wheel_delta(ui: &egui::Ui) -> Vec2 {
    const LIVE_TAU: f32 = 0.02; // seconds for the live queue to drain by ~63%
    const GLIDE_TAU: f32 = 0.15; // the same for each of the two easing stages of an inertia flush
    const COAST_TAU: f32 = 0.15; // speed decay while coasting through the driver's pause
    const COAST_MAX: f64 = 0.6; // give up coasting if no flush came within this many seconds
    const FINGER_GAP: f64 = 0.025; // no finger event for this long: the fingers have stopped
    const FLUSH: f32 = 4.0; // lines in one event that make it an inertia flush, not a finger
    let id = egui::Id::new("wheel_delta_pending");
    let line = ui.ctx().options(|o| o.input_options.line_scroll_speed);
    let mut q = ui.ctx().data(|d| d.get_temp::<WheelQueue>(id)).unwrap_or_default();
    let (dt, now, zooming, shift) = ui.input(|i| {
        let (mut small, mut big) = (Vec2::ZERO, Vec2::ZERO);
        for e in &i.events {
            if let egui::Event::MouseWheel { unit, delta, .. } = e {
                let lines = match unit {
                    egui::MouseWheelUnit::Point => *delta / line,
                    egui::MouseWheelUnit::Line => *delta,
                    egui::MouseWheelUnit::Page => *delta * i.content_rect().height() / line,
                };
                if lines.length() >= FLUSH { big += lines * line } else { small += lines * line }
            }
        }
        if small != Vec2::ZERO {
            q.live += small;
            q.last_small = i.time;
            q.coasted = Vec2::ZERO; // a new gesture
        }
        if big != Vec2::ZERO {
            // what is left of the flush after the part already coasted (never reversing it)
            let rest = |b: f32, c: f32| if b * c > 0.0 { if b.abs() > c.abs() { b - c } else { 0.0 } } else { b };
            q.glide += Vec2::new(rest(big.x, q.coasted.x), rest(big.y, q.coasted.y));
            q.coasted = Vec2::ZERO;
            q.vel = Vec2::ZERO;
        }
        (i.unstable_dt.clamp(0.001, 0.05), i.time, i.modifiers.command || i.modifiers.ctrl, i.modifiers.shift)
    });
    if zooming { // ctrl+wheel zooms
        q = WheelQueue::default();
    }
    let pay = |v: &mut Vec2, tau: f32| {
        let out = if v.length() < 0.5 { *v } else { *v * (1.0 - (-dt / tau).exp()) };
        *v -= out;
        out
    };
    let live_out = pay(&mut q.live, LIVE_TAU);
    let mut out = live_out;
    let gliding = q.glide != Vec2::ZERO || q.glide2 != Vec2::ZERO;
    if now - q.last_small < FINGER_GAP {
        let a = 1.0 - (-dt / 0.03).exp();
        q.vel = q.vel * (1.0 - a) + live_out / dt * a;
    } else if !gliding && q.vel.length() > 20.0 && now - q.last_small < COAST_MAX {
        // supply what the live queue's tail does not, so the speed carries on unbroken
        let extra = |need: f32, got: f32, v: f32| if (need - got) * v > 0.0 { need - got } else { 0.0 };
        let c = Vec2::new(extra(q.vel.x * dt, live_out.x, q.vel.x), extra(q.vel.y * dt, live_out.y, q.vel.y));
        out += c;
        q.coasted += c;
        q.vel *= (-dt / COAST_TAU).exp();
    } else if gliding {
        q.vel = Vec2::ZERO;
    }
    let stage = pay(&mut q.glide, GLIDE_TAU);
    q.glide2 += stage;
    out += pay(&mut q.glide2, GLIDE_TAU);
    if shift {
        out = Vec2::new(out.x + out.y, 0.0);
    }
    let busy = q.live != Vec2::ZERO || q.glide != Vec2::ZERO || q.glide2 != Vec2::ZERO || q.vel.length() > 20.0;
    ui.ctx().data_mut(|d| d.insert_temp(id, q));
    if busy {
        ui.ctx().request_repaint();
    }
    out
}


fn mix(a: f32, b: f32, e: f32) -> f32 {
    a + (b - a) * e
}

fn mix_color(a: Color32, b: Color32, e: f32) -> Color32 {
    let e = e.clamp(0.0, 1.0);
    let (a, b) = (a.to_srgba_unmultiplied(), b.to_srgba_unmultiplied());
    let c: Vec<u8> = (0..4).map(|k| mix(a[k] as f32, b[k] as f32, e).round() as u8).collect();
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

// `to` (a fill of the incoming layer) `e` of the way from `from`: colors, gradients (a solid counts as a
// gradient of one color) and paint opacity blend; images and other pairs stay as they are
fn mix_paint(from: &Paint, to: &mut Paint, e: f32) {
    let ends = |k: &PaintKind| match k {
        PaintKind::Solid(c) => Some((*c, *c, None)),
        PaintKind::Linear { a, b, angle } => Some((*a, *b, Some(*angle))),
        PaintKind::Gradient(g) if g.kind == GradientKind::Linear && g.stops.len() == 2 => Some((g.stops[0].color, g.stops[1].color, Some(g.angle))),
        PaintKind::Gradient(_) | PaintKind::Image(_) | PaintKind::Pattern(_) | PaintKind::Shader(_) => None,
    };
    let (Some((a0, b0, ang0)), Some((a1, b1, ang1))) = (ends(&from.kind), ends(&to.kind)) else { return };
    to.opacity = mix(from.opacity, to.opacity, e);
    if let (None, None) = (ang0, ang1) {
        to.kind = PaintKind::Solid(mix_color(a0, a1, e));
        return;
    }
    let (g0, g1) = (ang0.or(ang1).unwrap_or(0.0), ang1.or(ang0).unwrap_or(0.0));
    to.kind = PaintKind::Gradient(Gradient { angle: mix(g0, g1, e), ..Gradient::between(mix_color(a0, a1, e), mix_color(b0, b1, e)) });
}

// Smart Animate: the page with the layers of frame `to` `e` of the way (eased; may overshoot) from their
// namesakes in frame `from` (same name path). Matched layers tween position, size, rotation, corner radius,
// opacity and solid color; layers only in `to` fade in; layers only in `from` are moved into `to` and fade out.
// Indices are unchanged, so callers can keep using theirs.
pub fn smart_tween(shapes: &[Shape], from: u64, to: u64, e: f32) -> Vec<Shape> {
    let mut out = shapes.to_vec();
    let fade = e.clamp(0.0, 1.0);
    let src = name_paths(shapes, from);
    let mut used = vec![false; src.len()];
    // id in `from` -> its counterpart in `to`, roots included
    let mut pair: std::collections::HashMap<u64, u64> = [(from, to)].into();
    for (k, path) in name_paths(shapes, to) {
        let Some(j) = (0..src.len()).find(|&j| !used[j] && src[j].1 == path) else {
            out[k].opacity *= fade;
            continue;
        };
        used[j] = true;
        let (a, b) = (&shapes[src[j].0], &shapes[k]);
        pair.insert(a.id, b.id);
        let t = &mut out[k];
        t.pos = Pos2::new(mix(a.pos.x, b.pos.x, e), mix(a.pos.y, b.pos.y, e));
        t.size = Vec2::new(mix(a.size.x, b.size.x, e), mix(a.size.y, b.size.y, e));
        if b.kind != Kind::Line {
            t.size = t.size.max(Vec2::ZERO);
        }
        t.rotation = mix(a.rotation, b.rotation, e);
        t.opacity = mix(a.opacity, b.opacity, fade);
        t.font_size = mix(a.font_size, b.font_size, e).max(1.0);
        for c in 0..4 {
            t.radius[c] = mix(a.radius[c], b.radius[c], e).max(0.0);
        }
        for (tp, ap) in t.fills.iter_mut().zip(&a.fills) {
            mix_paint(ap, tp, fade);
        }
        if let (Kind::Path, Kind::Path) = (a.kind, b.kind) {
            if let Some(m) = crate::vector::morph(&a.path, &b.path, e) {
                t.path = m;
            }
        }
        for c in 0..2 {
            t.arc[c] = mix(a.arc[c], b.arc[c], e);
        }
        t.inner = mix(a.inner, b.inner, e).clamp(0.0, 1.0);
    }
    for (j, (k, _)) in src.iter().enumerate() {
        if used[j] {
            continue;
        }
        // an unmatched layer whose parent has a counterpart goes along with it (its own children follow it)
        if let Some(&p) = shapes[*k].parent.and_then(|p| pair.get(&p)) {
            out[*k].parent = Some(p);
            out[*k].opacity *= 1.0 - fade;
        }
    }
    out
}

// the pressure of a pen on the canvas this frame, when the device reports one
fn pen_force(ctx: &egui::Context) -> Option<f32> {
    ctx.input(|i| i.events.iter().rev().find_map(|e| match e {
        egui::Event::Touch { force: Some(f), .. } => Some(f.clamp(0.05, 1.0)),
        _ => None,
    }))
}

// the line of a connection being dragged follows the pointer: repaint every frame
fn ui_repaint(resp: &egui::Response) {
    resp.ctx.request_repaint();
}
