// Motion: keyframed properties. A shape's plain fields are its values without animation; its `tracks`
// override them over the page's timeline. In Motion mode the editor shows (and edits) the page at the
// playhead: `evaluate` before the frame, `commit` after it turns edits of animated values into keyframes.
use crate::fig::Tip as _;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};

use crate::fig::{self, Num, Prefix};
use crate::glyphs::Glyph;
use crate::model::*;
use crate::{App, TEXT, TEXT_SECONDARY, TEXT_TERTIARY};

pub const FPS: f32 = 30.0; // frames per second of the timeline's frame display (the animation itself is in ms)
const SAME_T: f32 = 0.5; // keyframes closer than this (ms) are the same keyframe

// the value of `p` on `s`; None when it does not apply (Fill on a shape whose first fill is not solid)
pub fn get(s: &Shape, p: Prop) -> Option<[f32; 4]> {
    let one = |v: f32| Some([v, 0.0, 0.0, 0.0]);
    match p {
        Prop::X => one(s.pos.x),
        Prop::Y => one(s.pos.y),
        Prop::Width => one(s.size.x),
        Prop::Height => one(s.size.y),
        Prop::Rotation => one(s.rotation),
        Prop::Opacity => one(s.opacity),
        Prop::Radius if matches!(s.kind, Kind::Rect | Kind::Frame) => one(s.radius[0]),
        Prop::Radius => None,
        Prop::Fill => match s.fills.first().map(|f| &f.kind) {
            Some(PaintKind::Solid(c)) => Some(c.to_srgba_unmultiplied().map(|x| x as f32)),
            _ => None,
        },
        // virtual properties have no value on the layer: they read as their rest value
        Prop::Scale => one(1.0),
        Prop::Arc => one(0.0),
    }
}

pub fn set(s: &mut Shape, p: Prop, v: [f32; 4]) {
    match p {
        Prop::X => s.pos.x = v[0],
        Prop::Y => s.pos.y = v[0],
        Prop::Width => s.size.x = v[0],
        Prop::Height => s.size.y = v[0],
        Prop::Rotation => s.rotation = v[0],
        Prop::Opacity => s.opacity = v[0].clamp(0.0, 1.0),
        Prop::Radius => s.radius = [v[0].max(0.0); 4],
        Prop::Fill => {
            if let Some(PaintKind::Solid(c)) = s.fills.first_mut().map(|f| &mut f.kind) {
                let [r, g, b, a] = v.map(|x| x.round().clamp(0.0, 255.0) as u8);
                *c = Color32::from_rgba_unmultiplied(r, g, b, a);
            }
        }
        Prop::Scale | Prop::Arc => {} // applied by `evaluate` over the whole layer
    }
}

// a track's value at time `t`: held before the first and after the last keyframe, eased in between
// by the easing of the keyframe the segment starts at
pub fn value_at(keys: &[Key], t: f32) -> Option<[f32; 4]> {
    let first = keys.first()?;
    if t <= first.t {
        return Some(first.v);
    }
    for w in keys.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t < b.t {
            let e = a.ease.at((t - a.t) / (b.t - a.t).max(1e-3));
            return Some(std::array::from_fn(|k| a.v[k] + (b.v[k] - a.v[k]) * e));
        }
    }
    keys.last().map(|k| k.v)
}

// sets the keyframe at `t` (replacing one within SAME_T), keeping the keys sorted
pub fn upsert(track: &mut Track, t: f32, v: [f32; 4]) {
    if let Some(k) = track.keys.iter_mut().find(|k| (k.t - t).abs() < SAME_T) {
        k.v = v;
        return;
    }
    let at = track.keys.iter().position(|k| k.t > t).unwrap_or(track.keys.len());
    track.keys.insert(at, Key { t, v, ease: Easing::default() });
}

pub fn key_at(track: &Track, t: f32) -> Option<usize> {
    track.keys.iter().position(|k| (k.t - t).abs() < SAME_T)
}

// the page at time `t`
pub fn evaluate(shapes: &[Shape], t: f32) -> Vec<Shape> {
    let mut out = shapes.to_vec();
    for s in out.iter_mut().filter(|s| s.kind == Kind::Text && s.text_anim.unit != 0) {
        s.text_t = Some(t);
    }
    for (s, base) in out.iter_mut().zip(shapes).filter(|(s, _)| !s.tracks.is_empty()) {
        for k in 0..s.tracks.len() {
            if let Some(v) = value_at(&s.tracks[k].keys, t) {
                let p = s.tracks[k].prop;
                set(s, p, v);
            }
        }
        // Arc bends the straight way between two X / Y keyframes into a parabola; Scale then grows the layer about its centre
        if let Some(arc) = s.tracks.iter().find(|tr| tr.prop == Prop::Arc).and_then(|tr| value_at(&tr.keys, t)).map(|v| v[0]).filter(|a| a.abs() > 1e-4) {
            s.pos += arc_offset(&s.tracks, base.pos, s.pos, t, arc);
        }
        if let Some(k) = s.tracks.iter().find(|tr| tr.prop == Prop::Scale).and_then(|tr| value_at(&tr.keys, t)).map(|v| v[0]).filter(|k| (k - 1.0).abs() > 1e-4) {
            let c = s.pos + s.size / 2.0;
            s.size *= k.max(0.0);
            s.pos = c - s.size / 2.0;
        }
    }
    out
}

// how far an Arc moves the layer sideways at `t`: between the X / Y keyframes around `t` the layer goes from one end to the other,
// and `arc` (a share of that distance) is the height of the bend in the middle
fn arc_offset(tracks: &[Track], rest: Pos2, now: Pos2, t: f32, arc: f32) -> Vec2 {
    let mut times: Vec<f32> = tracks.iter().filter(|tr| matches!(tr.prop, Prop::X | Prop::Y)).flat_map(|tr| tr.keys.iter().map(|k| k.t)).collect();
    times.sort_by(|a, b| a.total_cmp(b));
    times.dedup_by(|a, b| (*a - *b).abs() < SAME_T);
    let (Some(t0), Some(t1)) = (times.iter().rev().find(|&&k| k <= t).copied(), times.iter().find(|&&k| k > t).copied()) else { return Vec2::ZERO };
    let at = |tt: f32| {
        let get = |p: Prop, base: f32| tracks.iter().find(|tr| tr.prop == p).and_then(|tr| value_at(&tr.keys, tt)).map_or(base, |v| v[0]);
        Pos2::new(get(Prop::X, rest.x), get(Prop::Y, rest.y))
    };
    let (p0, p1) = (at(t0), at(t1));
    let chord = p1 - p0;
    let len = chord.length();
    if len < 1e-3 {
        return Vec2::ZERO;
    }
    let f = ((now - p0).dot(chord) / (len * len)).clamp(0.0, 1.0);
    Vec2::new(-chord.y, chord.x) / len * (arc * len * 4.0 * f * (1.0 - f))
}

fn differs(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(&b).any(|(x, y)| (x - y).abs() > 1e-4)
}

// folds a frame's edits back into the document. `base` is the page without animation, `shown` what the
// frame started from (base evaluated at `t`), `after` what the frame left. An animated property that changed
// gets a keyframe at `t` and keeps its base value; with `auto_key`, so does a property that had no track yet
// (keyed at 0 with its old value as well). Everything else is an ordinary edit.
pub fn commit(base: &[Shape], shown: &[Shape], mut after: Vec<Shape>, t: f32, auto_key: bool) -> Vec<Shape> {
    for s in after.iter_mut() {
        let (Some(b), Some(sh)) = (base.iter().find(|x| x.id == s.id), shown.iter().find(|x| x.id == s.id)) else { continue };
        for (p, _) in Prop::ALL.iter().filter(|(p, _)| !p.is_virtual()).copied() {
            let (Some(now), Some(was)) = (get(s, p), get(sh, p)) else { continue };
            let changed = differs(now, was);
            let track = s.tracks.iter().position(|tr| tr.prop == p);
            match track {
                Some(k) => {
                    if changed {
                        upsert(&mut s.tracks[k], t, now);
                    }
                }
                None if changed && auto_key => {
                    let mut tr = Track { prop: p, keys: vec![] };
                    if t > SAME_T {
                        upsert(&mut tr, 0.0, was);
                    }
                    upsert(&mut tr, t, now);
                    s.tracks.push(tr);
                }
                None => continue,
            }
            if let Some(v) = get(b, p) {
                set(s, p, v);
            }
        }
        // Scale and Arc moved or resized what the frame showed: what was not edited goes back to the page's own values
        if s.tracks.iter().any(|t| t.prop.is_virtual()) {
            if s.pos == sh.pos {
                s.pos = b.pos;
            }
            if s.size == sh.size {
                s.size = b.size;
            }
        }
    }
    after
}

// the playhead time `t0 + elapsed` on a timeline of length `len`; None once a non-looping one has ended
pub fn play_time(from: f32, elapsed_ms: f32, len: f32, looping: bool) -> (f32, bool) {
    let t = from + elapsed_ms;
    if t < len {
        (t, true)
    } else if looping && len > 0.0 {
        (t % len, true)
    } else {
        (len, false)
    }
}

// a time as the timeline shows it: milliseconds or frames
pub(crate) fn fmt_time(t: f32, frames: bool) -> String {
    if frames { format!("{} f", (t * FPS / 1000.0).round()) } else { format!("{} ms", t.round()) }
}

fn diamond(p: &egui::Painter, c: Pos2, r: f32, fill: Option<Color32>, stroke: Color32) {
    let pts = vec![c + Vec2::new(0.0, -r), c + Vec2::new(r, 0.0), c + Vec2::new(0.0, r), c + Vec2::new(-r, 0.0)];
    p.add(egui::Shape::convex_polygon(pts, fill.unwrap_or(Color32::TRANSPARENT), Stroke::new(1.2, stroke)));
}

impl App {
    pub fn has_motion(&self) -> bool {
        self.shapes.iter().any(|s| !s.tracks.is_empty() || (s.kind == Kind::Text && s.text_anim.unit != 0))
    }

    pub fn anim_len(&self) -> f32 {
        self.pages.get(self.cur).map_or(2000.0, |p| p.anim_ms.max(100.0))
    }

    fn anim_loop(&self) -> bool {
        self.pages.get(self.cur).is_some_and(|p| p.anim_loop)
    }

    // shows the page at the playhead for this frame; the page as it was is the caller's `before`
    pub fn motion_begin(&mut self) -> Option<Vec<Shape>> {
        if !self.has_motion() {
            return None;
        }
        let shown = evaluate(&self.shapes, self.anim_time);
        self.shapes = shown.clone();
        Some(shown)
    }

    // folds the frame's edits back into `base` (see `commit`)
    pub fn motion_end(&mut self, base: &[Shape], shown: &[Shape]) {
        let after = std::mem::take(&mut self.shapes);
        self.shapes = commit(base, shown, after, self.anim_time, self.auto_key);
    }

    // moves the playhead while playing
    pub fn tick_motion(&mut self, now: f64) {
        let Some((t0, from)) = self.anim_play else { return };
        let (t, on) = play_time(from, ((now - t0) * 1000.0) as f32, self.anim_len(), self.anim_loop());
        // a loop that has just started again restarts the sound too
        let wrapped = t + 50.0 < self.anim_time;
        self.anim_time = t;
        if !on {
            self.anim_play = None;
            self.audio_stop();
        } else if wrapped {
            self.audio_play(t);
        }
    }

    pub fn toggle_play(&mut self, now: f64) {
        self.anim_play = match self.anim_play {
            Some(_) => None,
            None if self.anim_time >= self.anim_len() - SAME_T => Some((now, 0.0)),
            None => Some((now, self.anim_time)),
        };
        // the sound follows: it starts where the playhead is, and stops with it
        match self.anim_play {
            Some((_, from)) => self.audio_play(from),
            None => self.audio_stop(),
        }
    }

    // the diamond beside a property (Figma: "Add or remove ... keyframe"): filled when there is a keyframe at
    // the playhead, blue when the property is animated; a click adds or removes the keyframe at the playhead
    // the value of a virtual property at the playhead: its track's, or the rest value
    fn virtual_value(&self, i: usize, p: Prop) -> f32 {
        self.shapes[i].tracks.iter().find(|tr| tr.prop == p).and_then(|tr| value_at(&tr.keys, self.anim_time)).map_or(p.rest(), |v| v[0])
    }

    // a field for Scale or Arc: edits go straight into a keyframe at the playhead (the property is its keyframes)
    fn virtual_field(&mut self, ui: &mut egui::Ui, r: Rect, i: usize, p: Prop, prefix: crate::fig::Prefix, n: crate::fig::Num) {
        let id = ui.id().with(("motion", p.label(), self.shapes[i].id));
        let field = Rect::from_min_max(r.min, Pos2::new(r.right() - 22.0, r.bottom()));
        crate::fig::input_bg(ui, r, false);
        let mut x = self.virtual_value(i, p) * 100.0;
        if crate::fig::num_field(ui, field, id, prefix, &mut x, n, p.label()) {
            let (t, v) = (self.anim_time, [x / 100.0, 0.0, 0.0, 0.0]);
            let s = &mut self.shapes[i];
            match s.tracks.iter().position(|tr| tr.prop == p) {
                Some(k) => upsert(&mut s.tracks[k], t, v),
                None => {
                    let mut tr = Track { prop: p, keys: vec![] };
                    if t > SAME_T {
                        upsert(&mut tr, 0.0, [p.rest(), 0.0, 0.0, 0.0]);
                    }
                    upsert(&mut tr, t, v);
                    s.tracks.push(tr);
                }
            }
        }
        self.key_button(ui, Pos2::new(r.right() - 12.0, r.center().y), i, p);
    }

    fn key_button(&mut self, ui: &mut egui::Ui, c: Pos2, i: usize, p: Prop) {
        let r = Rect::from_center_size(c, Vec2::splat(20.0));
        let resp = ui.interact(r, ui.id().with(("key", p.label(), self.shapes[i].id)), Sense::click());
        let t = self.anim_time;
        let track = self.shapes[i].tracks.iter().position(|tr| tr.prop == p);
        let on = track.and_then(|k| key_at(&self.shapes[i].tracks[k], t)).is_some();
        if resp.hovered() {
            ui.painter().rect_filled(r, crate::fig::RADIUS, crate::fig::INPUT_HOVER());
        }
        let ink = if track.is_some() { crate::fig::SELECTED_INK() } else if resp.hovered() { TEXT() } else { TEXT_SECONDARY() };
        diamond(ui.painter(), c, 4.5, on.then_some(crate::fig::SELECTED_INK()), ink);
        let label = format!("Add or remove {} keyframe", p.label());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, &label));
        if !resp.tip(&label).clicked() {
            return;
        }
        let s = &mut self.shapes[i];
        match track {
            Some(k) if on => {
                let idx = key_at(&s.tracks[k], t).unwrap_or(0);
                s.tracks[k].keys.remove(idx);
                if s.tracks[k].keys.is_empty() {
                    s.tracks.remove(k);
                }
            }
            _ => {
                let v = if p.is_virtual() { [s.tracks.iter().find(|tr| tr.prop == p).and_then(|tr| value_at(&tr.keys, t)).map_or(p.rest(), |v| v[0]), 0.0, 0.0, 0.0] } else { let Some(v) = get(s, p) else { return }; v };
                match track {
                    Some(k) => upsert(&mut s.tracks[k], t, v),
                    None => s.tracks.push(Track { prop: p, keys: vec![Key { t, v, ease: Easing::default() }] }),
                }
            }
        }
    }

    // a property field with its keyframe diamond inside at the right, as in Figma's Motion panel
    fn key_field(&mut self, ui: &mut egui::Ui, r: Rect, i: usize, p: Prop, prefix: crate::fig::Prefix, n: crate::fig::Num) {
        let Some(mut v) = get(&self.shapes[i], p) else { return };
        let id = ui.id().with(("motion", p.label(), self.shapes[i].id));
        let field = Rect::from_min_max(r.min, Pos2::new(r.right() - 22.0, r.bottom()));
        crate::fig::input_bg(ui, r, false);
        let mut x = if p == Prop::Opacity { v[0] * 100.0 } else { v[0] };
        if crate::fig::num_field(ui, field, id, prefix, &mut x, n, p.label()) {
            v[0] = if p == Prop::Opacity { (x / 100.0).clamp(0.0, 1.0) } else { x };
            set(&mut self.shapes[i], p, v);
        }
        self.key_button(ui, Pos2::new(r.right() - 12.0, r.center().y), i, p);
    }

    // the Motion tab, like Figma's: Transform, Layout, Appearance and Fill at the playhead, each value with
    // its keyframe diamond; editing an animated value sets a keyframe
    pub fn motion_tab(&mut self, ui: &mut egui::Ui) {
        use crate::fig::{self, Num, Prefix};
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let g = fig::Grid::of(ui.max_rect());
        let Some(i) = self.single() else {
            fig::row(ui, 8.0);
            fig::note(ui, Glyph::Sparkles, "Select a layer to animate it. Press a property's diamond to add a keyframe at the playhead.");
            return;
        };
        let n = |decimals: usize| Num { decimals, ..Num::default() };
        // the layer and the playhead
        let r = fig::row(ui, 48.0);
        fig::ptext(&ui.painter().with_clip_rect(r), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, &self.shapes[i].name, fig::semi(13.0), TEXT());
        fig::ptext(ui.painter(), Pos2::new(r.right() - 16.0, r.center().y), Align2::RIGHT_CENTER, format!("{} ms", self.anim_time.round()), fig::reg(11.0), TEXT_SECONDARY());
        fig::rule(ui);

        fig::header(ui, "Transform", false, &[]);
        fig::labels(ui, &g, true, "Position", "");
        let r = fig::controls(ui, 0.0);
        self.key_field(ui, g.col1(r), i, Prop::X, Prefix::Text("X"), n(1));
        self.key_field(ui, g.col2(r), i, Prop::Y, Prefix::Text("Y"), n(1));
        fig::labels(ui, &g, false, "Rotation", "");
        let r = fig::controls(ui, 0.0);
        self.key_field(ui, g.col1(r), i, Prop::Rotation, Prefix::Icon(Glyph::RotateCw), Num { decimals: 1, suffix: "°", ..Num::default() });
        fig::labels(ui, &g, false, "Scale", "Arc");
        let r = fig::controls(ui, 0.0);
        self.virtual_field(ui, g.col1(r), i, Prop::Scale, Prefix::Icon(Glyph::Ratio), Num { min: 0.0, max: 1000.0, decimals: 0, suffix: "%", speed: 1.0 });
        self.virtual_field(ui, g.col2(r), i, Prop::Arc, Prefix::Icon(Glyph::Waves), Num { min: -500.0, max: 500.0, decimals: 0, suffix: "%", speed: 1.0 });
        fig::end(ui);

        fig::header(ui, "Layout", false, &[]);
        fig::labels(ui, &g, true, "Dimensions", "");
        let r = fig::controls(ui, 0.0);
        self.key_field(ui, g.col1(r), i, Prop::Width, Prefix::Text("W"), n(1));
        self.key_field(ui, g.col2(r), i, Prop::Height, Prefix::Text("H"), n(1));
        fig::end(ui);

        let corners = get(&self.shapes[i], Prop::Radius).is_some();
        fig::header(ui, "Appearance", false, &[]);
        fig::labels(ui, &g, true, "Opacity", if corners { "Corner radius" } else { "" });
        let r = fig::controls(ui, 0.0);
        self.key_field(ui, g.col1(r), i, Prop::Opacity, Prefix::Icon(Glyph::Contrast), Num { min: 0.0, max: 100.0, decimals: 0, suffix: "%", speed: 1.0 });
        if corners {
            self.key_field(ui, g.col2(r), i, Prop::Radius, Prefix::Icon(Glyph::Radius), Num { min: 0.0, decimals: 1, ..Num::default() });
        }
        fig::end(ui);

        if let Some(v) = get(&self.shapes[i], Prop::Fill) {
            fig::header(ui, "Fill", false, &[]);
            let r = fig::controls(ui, 2.0);
            let f = g.both(r);
            fig::input_bg(ui, f, false);
            let mut c = Color32::from_rgba_unmultiplied(v[0] as u8, v[1] as u8, v[2] as u8, v[3] as u8);
            let id = self.shapes[i].id;
            let a = fig::swatch(ui, Pos2::new(f.left() + 12.0, r.center().y), &mut c, "Fill color");
            let b = fig::hex_text(ui, Pos2::new(f.left() + 24.0, r.center().y), f.width() - 28.0, &mut c, ui.id().with(("motion-hex", id)));
            if a || b {
                set(&mut self.shapes[i], Prop::Fill, c.to_srgba_unmultiplied().map(|x| x as f32));
            }
            self.key_button(ui, g.icon(r).center(), i, Prop::Fill);
            fig::end(ui);
        }
        if self.shapes[i].kind == Kind::Text {
            let id = self.shapes[i].id;
            let a = &mut self.shapes[i].text_anim;
            fig::header(ui, "Text animation", false, &[]);
            fig::labels(ui, &g, true, "Animate by", "Effect");
            let r = fig::controls(ui, 0.0);
            fig::dropdown(ui, g.col1(r), None, &mut a.unit, &[(0, "None"), (1, "Character"), (2, "Word"), (3, "Line")], "Text animation unit");
            fig::dropdown(ui, g.col2(r), None, &mut a.kind, &[(0, "Fade in"), (1, "Typewriter")], "Text animation effect");
            if a.unit != 0 {
                let ms = Num { min: 0.0, max: 60000.0, decimals: 0, speed: 5.0, suffix: " ms" };
                fig::labels(ui, &g, false, "Start", "Between units");
                let r = fig::controls(ui, 0.0);
                fig::num_field(ui, g.col1(r), ui.id().with(("ta-start", id)), Prefix::Icon(Glyph::Clock), &mut a.start, ms, "Text animation start");
                fig::num_field(ui, g.col2(r), ui.id().with(("ta-step", id)), Prefix::Icon(Glyph::Clock), &mut a.per_unit, ms, "Time between units");
                if a.kind == 0 {
                    fig::labels(ui, &g, false, "Each unit fades in over", "");
                    let r = fig::controls(ui, 0.0);
                    fig::num_field(ui, g.col1(r), ui.id().with(("ta-dur", id)), Prefix::Icon(Glyph::Clock), &mut a.duration, ms, "Fade duration");
                }
            }
            fig::end(ui);
        }
        fig::row(ui, 8.0);
        fig::note(ui, Glyph::Sparkles, "Changing an animated value sets a keyframe at the playhead. Turn on Auto-keyframe in the timeline to animate any value you change.");
    }

    // the timeline under the canvas in Motion mode: a 40px bar of controls (fig.rs widgets), a divider,
    // then a 24px ruler over 24px rows, a 200px label column on the left
    pub fn timeline(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        let len = self.anim_len();
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let bar = fig::row(ui, 40.0);
        let mut x = bar.left() + 8.0;
        let y = bar.top() + 8.0;
        // play / pause: a 24px icon button; the pause bars are painted
        let pr = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(fig::ROW));
        let playing = self.anim_play.is_some();
        let name = if playing { "Pause" } else { "Play" };
        let resp = if playing {
            let resp = ui.interact(pr, ui.id().with("tl-play"), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(pr, fig::RADIUS, fig::INPUT());
            }
            for dx in [-2.5, 2.5] {
                ui.painter().rect_filled(Rect::from_center_size(pr.center() + Vec2::new(dx, 0.0), Vec2::new(2.0, 9.0)), 0.5, TEXT());
            }
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
            resp.tip(name)
        } else {
            fig::icon_button(ui, pr, Glyph::Present, name, false)
        };
        if resp.clicked() {
            self.toggle_play(now);
        }
        x += 24.0 + 8.0;
        fig::checkbox(ui, Rect::from_min_size(Pos2::new(x, y), Vec2::new(104.0, fig::ROW)), &mut self.auto_key, "Auto-keyframe");
        x += 104.0 + 8.0;
        // ms or frames: the fields below show and take whichever is on
        let frames = self.anim_frames;
        let (unit, suffix) = if frames { (FPS / 1000.0, " f") } else { (1.0, " ms") };
        let mut t = self.anim_time * unit;
        if fig::num_field(ui, Rect::from_min_size(Pos2::new(x, y), Vec2::new(72.0, fig::ROW)), ui.id().with("tl-time"), Prefix::None, &mut t, Num { min: 0.0, max: len * unit, decimals: 0, speed: 5.0, suffix }, "Current time") {
            self.anim_time = (t / unit).clamp(0.0, len);
            self.anim_play = None;
        }
        x += 72.0;
        fig::text(ui, Pos2::new(x + 8.0, bar.center().y), Align2::CENTER_CENTER, "/", fig::reg(11.0), TEXT_TERTIARY());
        x += 16.0;
        let cur = self.cur;
        if let Some(page) = self.pages.get_mut(cur) {
            let mut d = page.anim_ms * unit;
            if fig::num_field(ui, Rect::from_min_size(Pos2::new(x, y), Vec2::new(72.0, fig::ROW)), ui.id().with("tl-duration"), Prefix::None, &mut d, Num { min: 100.0 * unit, max: 60000.0 * unit, decimals: 0, speed: 10.0, suffix }, "Duration") {
                page.anim_ms = (d / unit).clamp(100.0, 60000.0);
            }
            x += 72.0 + 8.0;
            fig::checkbox(ui, Rect::from_min_size(Pos2::new(x, y), Vec2::new(56.0, fig::ROW)), &mut page.anim_loop, "Loop");
            x += 56.0 + 8.0;
        }
        if fig::button(ui, Rect::from_min_size(Pos2::new(x, y), Vec2::new(88.0, fig::ROW)), "Add audio").clicked() {
            self.add_audio();
        }
        x += 88.0 + 8.0;
        // time format: ms or frames (30 per second)
        if fig::button(ui, Rect::from_min_size(Pos2::new(x, y), Vec2::new(48.0, fig::ROW)), if self.anim_frames { "Frames" } else { "ms" }).clicked() {
            self.anim_frames = !self.anim_frames;
        }
        x += 48.0 + 8.0;
        // zoom the lane: Ctrl+wheel over it does the same, anchored at the pointer
        for (glyph, tip, f) in [(Glyph::Minus, "Zoom timeline out", 0.5), (Glyph::Plus, "Zoom timeline in", 2.0)] {
            if fig::icon_button(ui, Rect::from_min_size(Pos2::new(x, y), Vec2::splat(fig::ROW)), glyph, tip, false).clicked() {
                let (z, s) = self.tl_view;
                let centre = s + len / z / 2.0;
                let nz = (z * f).clamp(1.0, 32.0);
                self.tl_view = (nz, centre - len / nz / 2.0);
            }
            x += fig::ROW;
        }
        // the selected keyframe, from the right: Delete, its easing, what it is
        if let Some((id, p, t)) = self.key_sel {
            let found = self.index_of(id).and_then(|i| {
                let k = self.shapes[i].tracks.iter().position(|tr| tr.prop == p)?;
                Some((i, k, key_at(&self.shapes[i].tracks[k], t)?))
            });
            match found {
                Some((i, k, n)) => {
                    let del = Rect::from_min_size(Pos2::new(bar.right() - 32.0, y), Vec2::splat(fig::ROW));
                    if fig::icon_button(ui, del, Glyph::Delete, "Delete keyframe", false).clicked() {
                        let tr = &mut self.shapes[i].tracks[k];
                        tr.keys.remove(n);
                        if tr.keys.is_empty() {
                            self.shapes[i].tracks.remove(k);
                        }
                        self.key_sel = None;
                    } else {
                        let ease = Rect::from_min_size(Pos2::new(del.left() - 8.0 - 128.0, y), Vec2::new(128.0, fig::ROW));
                        fig::dropdown(ui, ease, None, &mut self.shapes[i].tracks[k].keys[n].ease, &Easing::ALL, "Keyframe easing");
                        // animation styles: a named easing (and time) for keyframes, kept with the file and published with libraries
                        let style = fig::button(ui, Rect::from_min_size(Pos2::new(ease.left() - 8.0 - 72.0, y), Vec2::new(72.0, fig::ROW)), "Style");
                        let mut apply: Option<AnimStyle> = None;
                        let mut save = false;
                        egui::Popup::menu(&style).show(|ui| {
                            for st in &self.ext.anims {
                                if crate::glyphs::menu_item(ui, None, &st.name, "", true) {
                                    apply = Some(st.clone());
                                    ui.close();
                                }
                            }
                            if !self.ext.anims.is_empty() {
                                crate::glyphs::menu_sep(ui);
                            }
                            if crate::glyphs::menu_item(ui, Some(Glyph::Plus), "Save as animation style", "", true) {
                                save = true;
                                ui.close();
                            }
                        });
                        let keys = &mut self.shapes[i].tracks[k].keys;
                        if let Some(st) = apply {
                            keys[n].ease = st.ease;
                            if st.duration > 0.0 && n + 1 < keys.len() {
                                // the segment takes the style's time: the keyframes after it move with the next one
                                let delta = st.duration - (keys[n + 1].t - keys[n].t);
                                keys[n + 1..].iter_mut().for_each(|key| key.t += delta);
                            }
                        }
                        if save {
                            let duration = keys.get(n + 1).map_or(0.0, |next| next.t - keys[n].t);
                            let ease = keys[n].ease;
                            let id = self.next_id;
                            self.next_id += 1;
                            self.ext.anims.push(AnimStyle { id, name: format!("Animation {}", self.ext.anims.len() + 1), ease, duration });
                        }
                        fig::text(ui, Pos2::new(ease.left() - 8.0 - 80.0, bar.center().y), Align2::RIGHT_CENTER, &format!("{} at {}", p.label(), fmt_time(t, self.anim_frames)), fig::reg(11.0), TEXT_SECONDARY());
                    }
                }
                None => self.key_sel = None,
            }
        }
        fig::rule(ui);
        // rows: every animated layer and the selection, a layer's name over its tracks
        let mut rows: Vec<(u64, Option<Prop>)> = vec![];
        for s in &self.shapes {
            if s.tracks.is_empty() && !self.sel.contains(&s.id) {
                continue;
            }
            rows.push((s.id, None));
            rows.extend(s.tracks.iter().map(|tr| (s.id, Some(tr.prop))));
        }
        const LABEL_W: f32 = 200.0;
        const ROW_H: f32 = 24.0;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let full = ui.available_rect_before_wrap();
            let n_audio = self.pages.get(self.cur).map_or(0, |p| p.audio.len());
            let height = (rows.len() as f32 + 1.0 + n_audio as f32) * ROW_H + 4.0;
            let (area, _) = ui.allocate_exact_size(Vec2::new(full.width(), height.max(full.height())), Sense::hover());
            let lane = Rect::from_min_max(Pos2::new(area.min.x + LABEL_W, area.min.y), Pos2::new(area.max.x - 12.0, area.max.y));
            // the visible window of the lane: zoomed in, it shows `span` ms from `vs`; Ctrl+wheel zooms at the pointer, a sideways scroll pans
            let (zoom_d, pan_x, over) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta.x, i.pointer.hover_pos().filter(|p| lane.contains(*p))));
            let (mut z, mut vs) = self.tl_view;
            if let Some(h) = over {
                let anchor = vs + (h.x - lane.min.x) / lane.width() * len / z;
                if zoom_d != 1.0 {
                    z = (z * zoom_d).clamp(1.0, 32.0);
                    vs = anchor - (h.x - lane.min.x) / lane.width() * len / z;
                }
                vs -= pan_x / lane.width() * len / z;
            }
            let span = len / z;
            vs = vs.clamp(0.0, (len - span).max(0.0));
            self.tl_view = (z, vs);
            let x_of = |t: f32| lane.min.x + (t - vs) / span * lane.width();
            let t_of = |x: f32| (vs + (x - lane.min.x) / lane.width() * span).clamp(0.0, len);
            let painter = ui.painter_at(area);
            let lp = painter.with_clip_rect(lane);
            // ruler: seconds and tenths, click or drag to move the playhead
            let ruler = Rect::from_min_max(lane.min, Pos2::new(lane.max.x, lane.min.y + ROW_H));
            let scrub = ui.interact(ruler, ui.id().with("scrubber"), Sense::click_and_drag());
            if let Some(p) = scrub.interact_pointer_pos().filter(|_| scrub.clicked() || scrub.dragged()) {
                self.anim_time = (t_of(p.x) / 10.0).round() * 10.0;
                self.anim_play = None;
            }
            scrub.widget_info(|| egui::WidgetInfo::slider(true, self.anim_time as f64, "Timeline scrubber"));
            // the label column's edge and the ruler's bottom
            painter.rect_filled(Rect::from_min_size(Pos2::new(lane.min.x - 8.0, area.min.y), Vec2::new(1.0, area.height())), 0.0, fig::RULE());
            painter.rect_filled(Rect::from_min_size(Pos2::new(area.min.x, ruler.max.y - 1.0), Vec2::new(area.width(), 1.0)), 0.0, fig::RULE());
            let step = if span <= 400.0 { 10.0 } else if span <= 1500.0 { 50.0 } else if span <= 3000.0 { 100.0 } else if span <= 10000.0 { 500.0 } else { 1000.0 };
            let mut t = (vs / step).floor() * step;
            while t <= (vs + span).min(len) + 0.1 {
                let x = x_of(t);
                let major = (t / 1000.0).fract() < 1e-4 || (t / 1000.0).fract() > 1.0 - 1e-4;
                lp.line_segment([Pos2::new(x, ruler.max.y - if major { 8.0 } else { 4.0 }), Pos2::new(x, ruler.max.y - 1.0)], Stroke::new(1.0, if major { TEXT_TERTIARY() } else { fig::RULE() }));
                if major {
                    let label = if self.anim_frames { format!("{}f", (t * FPS / 1000.0).round()) } else { format!("{}s", (t / 1000.0).round()) };
                    fig::ptext(&lp, Pos2::new(x + 4.0, ruler.min.y + 9.0), Align2::LEFT_CENTER, label, fig::reg(10.0), TEXT_SECONDARY());
                }
                t += step;
            }
            // sounds: a row each under the layers, the waveform from where the clip starts; drag it along the timeline
            let audio_y0 = area.min.y + ROW_H * (rows.len() as f32 + 1.0);
            let clips: Vec<(u64, String, f32)> = self.pages.get(self.cur).map(|p| p.audio.iter().map(|c| (c.id, c.name.clone(), c.start)).collect()).unwrap_or_default();
            let mut remove_clip = None;
            for (n, (cid, name, start)) in clips.iter().enumerate() {
                let row = Rect::from_min_size(Pos2::new(area.min.x, audio_y0 + n as f32 * ROW_H), Vec2::new(area.width(), ROW_H));
                painter.rect_filled(Rect::from_min_size(row.min, Vec2::new(row.width(), 1.0)), 0.0, fig::RULE());
                fig::ptext(&painter.with_clip_rect(Rect::from_min_max(row.min, Pos2::new(lane.min.x - 40.0, row.max.y))), Pos2::new(row.min.x + 16.0, row.center().y), Align2::LEFT_CENTER, format!("♪ {name}"), fig::semi(11.0), TEXT());
                if fig::icon_button(ui, Rect::from_min_size(Pos2::new(lane.min.x - 36.0, row.min.y), Vec2::splat(ROW_H)), Glyph::Minus, "Remove audio", false).clicked() {
                    remove_clip = Some(*cid);
                }
                let Some((peaks, ms)) = self.clip_peaks(*cid).cloned() else { continue };
                let wave = Rect::from_min_max(Pos2::new(x_of(*start).max(lane.min.x), row.min.y + 2.0), Pos2::new(x_of(*start + ms).min(lane.max.x), row.max.y - 2.0));
                if wave.width() < 1.0 {
                    continue;
                }
                lp.rect_filled(wave, fig::RADIUS, fig::SELECTED().gamma_multiply(0.5));
                let cols = wave.width().floor().max(1.0) as usize;
                for c in 0..cols {
                    let k = (((c as f32 / cols as f32) * peaks.len() as f32) as usize).min(peaks.len() - 1);
                    let h = (peaks[k].min(1.0) * wave.height() * 0.9).max(1.0);
                    let px = wave.min.x + c as f32 + 0.5;
                    lp.line_segment([Pos2::new(px, wave.center().y - h / 2.0), Pos2::new(px, wave.center().y + h / 2.0)], Stroke::new(1.0, fig::SELECTED_INK()));
                }
                let drag = ui.interact(wave, ui.id().with(("audio-clip", *cid)), Sense::drag());
                drag.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Audio {name}")));
                if drag.dragged() {
                    let dt = drag.drag_delta().x / lane.width() * span;
                    if let Some(c) = self.pages[self.cur].audio.iter_mut().find(|c| c.id == *cid) {
                        c.start = (c.start + dt).max(0.0);
                    }
                }
            }
            if let Some(cid) = remove_clip {
                self.audio_stop();
                self.pages[self.cur].audio.retain(|c| c.id != cid);
            }
            for (n, &(id, prop)) in rows.iter().enumerate() {
                let Some(i) = self.index_of(id) else { continue };
                let y = area.min.y + ROW_H * (n as f32 + 1.0);
                let row = Rect::from_min_size(Pos2::new(area.min.x, y), Vec2::new(area.width(), ROW_H));
                let Some(p) = prop else {
                    // a layer: a divider above it, its name in 11px semibold (highlighted when selected)
                    if n > 0 {
                        painter.rect_filled(Rect::from_min_size(row.min, Vec2::new(row.width(), 1.0)), 0.0, fig::RULE());
                    }
                    let selected = self.sel.contains(&id);
                    if selected {
                        painter.rect_filled(Rect::from_min_max(Pos2::new(row.min.x + 8.0, row.min.y + 2.0), Pos2::new(lane.min.x - 16.0, row.max.y - 2.0)), fig::RADIUS, fig::SELECTED());
                    }
                    fig::ptext(&painter.with_clip_rect(Rect::from_min_max(row.min, Pos2::new(lane.min.x - 16.0, row.max.y))), Pos2::new(row.min.x + 16.0, row.center().y), Align2::LEFT_CENTER, &self.shapes[i].name, fig::semi(11.0), TEXT());
                    continue;
                };
                fig::ptext(&painter, Pos2::new(row.min.x + 32.0, row.center().y), Align2::LEFT_CENTER, p.label(), fig::reg(11.0), TEXT_SECONDARY());
                let Some(k) = self.shapes[i].tracks.iter().position(|tr| tr.prop == p) else { continue };
                // double-click the lane: a keyframe there with the value the track has at that time
                let lane_row = Rect::from_min_max(Pos2::new(lane.min.x, row.min.y), Pos2::new(lane.max.x, row.max.y));
                let lane_resp = ui.interact(lane_row, ui.id().with(("lane", id, k)), Sense::click());
                if lane_resp.double_clicked() {
                    if let Some(pp) = lane_resp.interact_pointer_pos() {
                        let t = (t_of(pp.x) / 10.0).round() * 10.0;
                        let tr = &mut self.shapes[i].tracks[k];
                        if let Some(v) = value_at(&tr.keys, t) {
                            upsert(tr, t, v);
                            self.key_sel = Some((id, p, t));
                        }
                    }
                }
                // a line between the first and last keyframe, then the keyframes
                let keys = self.shapes[i].tracks[k].keys.clone();
                if let (Some(a), Some(b)) = (keys.first(), keys.last()) {
                    lp.line_segment([Pos2::new(x_of(a.t), row.center().y), Pos2::new(x_of(b.t), row.center().y)], Stroke::new(2.0, fig::INPUT()));
                }
                // the easing of each segment drawn between its two keyframes: progress up the row, time along it (overshoots are clipped to the row)
                for w in keys.windows(2) {
                    let (x0, x1) = (x_of(w[0].t), x_of(w[1].t));
                    if x1 < lane.min.x || x0 > lane.max.x || x1 - x0 < 6.0 {
                        continue;
                    }
                    let (top, bottom) = (row.min.y + 3.0, row.max.y - 3.0);
                    let n = (((x1 - x0) / 3.0) as usize).clamp(8, 48);
                    let pts: Vec<Pos2> = (0..=n).map(|k| {
                        let f = k as f32 / n as f32;
                        Pos2::new(x0 + (x1 - x0) * f, (bottom - (bottom - top) * w[0].ease.at(f)).clamp(row.min.y + 1.0, row.max.y - 1.0))
                    }).collect();
                    lp.add(egui::Shape::line(pts, Stroke::new(1.0, TEXT_TERTIARY())));
                }
                for (kn, key) in keys.iter().enumerate() {
                    let c = Pos2::new(x_of(key.t), row.center().y);
                    if c.x < lane.min.x - 6.0 || c.x > lane.max.x + 6.0 {
                        continue; // outside the zoomed window
                    }
                    let resp = ui.interact(Rect::from_center_size(c, Vec2::splat(12.0)), ui.id().with(("key", id, k, kn)), Sense::click_and_drag());
                    let selected = self.key_sel == Some((id, p, key.t));
                    let label = format!("Keyframe {} at {} ms", p.label(), key.t.round());
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, &label));
                    if resp.clicked() || resp.drag_started() {
                        self.key_sel = Some((id, p, key.t));
                        self.anim_time = key.t;
                        self.anim_play = None;
                    }
                    let mut moved_to = key.t;
                    if resp.dragged() {
                        if let Some(pp) = resp.interact_pointer_pos() {
                            moved_to = (t_of(pp.x) / 10.0).round() * 10.0;
                        }
                    }
                    let tr = &mut self.shapes[i].tracks[k];
                    if (moved_to - key.t).abs() >= SAME_T && key_at(tr, moved_to).is_none() {
                        if let Some(idx) = key_at(tr, key.t) {
                            tr.keys[idx].t = moved_to;
                            tr.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
                            self.key_sel = Some((id, p, moved_to));
                            self.anim_time = moved_to;
                        }
                    }
                    diamond(&lp, Pos2::new(x_of(moved_to), c.y), 5.0, Some(if selected { fig::SELECTED_INK() } else { fig::INPUT() }), if selected { TEXT() } else { TEXT_SECONDARY() });
                }
            }
            // the playhead
            let x = x_of(self.anim_time.min(len));
            lp.line_segment([Pos2::new(x, area.min.y), Pos2::new(x, area.max.y)], Stroke::new(1.0, fig::SHARE));
            lp.add(egui::Shape::convex_polygon(vec![Pos2::new(x - 5.0, area.min.y), Pos2::new(x + 5.0, area.min.y), Pos2::new(x + 5.0, area.min.y + 6.0), Pos2::new(x, area.min.y + 11.0), Pos2::new(x - 5.0, area.min.y + 6.0)], fig::SHARE, Stroke::NONE));
            if rows.is_empty() {
                fig::ptext(&painter, Pos2::new(lane.min.x + 8.0, area.min.y + ROW_H * 1.5), Align2::LEFT_CENTER, "Select a layer and press a property's diamond to add a keyframe", fig::reg(11.0), TEXT_SECONDARY());
            }
        });
    }
}
