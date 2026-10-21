// Present mode: the interactions of a prototype. Triggers (click, drag, mouse down / up / enter / leave, key press, after delay) run
// actions (navigate, back, open link, open / close / swap overlay, scroll to, set variable) on the presented frame.
use egui::{Pos2, Rect, Vec2};

use crate::model::*;
use crate::{App, PresentAnim};

// an overlay open on top of the presented frame; `shift` moves the overlay frame from where it sits on the page to where it shows
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Overlay {
    pub frame: u64,
    pub shift: Vec2,
    pub outside: bool, // a click outside closes it
    pub dim: bool,     // the page behind is dimmed
    pub t0: f64,       // when it opened, for its animation
    pub tr: Transition,
    pub ease: Easing,
    pub dur: f32,
    pub closing: Option<f64>, // when it began to close: the animation runs backwards, then it is dropped
}

// a While hovering / While pressing change that is in force: the instance `inst` shows another variant until the pointer leaves
// `area` (the layer that has the interaction, where it was) or lets go; then it is swapped back to `back`
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WhileState {
    pub press: bool,
    pub area: Rect,
    pub inst: u64,
    pub back: u64,
}

impl App {
    // the shape under `w` (or its nearest ancestor) that has an interaction with this trigger, with those interactions
    pub(crate) fn present_targets_at(&self, w: Pos2, trigger: Trigger) -> Option<(usize, Vec<Interaction>)> {
        let hit = self.draw_order().into_iter().rev().find(|&i| {
            let s = &self.shapes[i];
            !s.kind.is_container() && s.visible && s.hit(w - self.origin_of(s.parent), 0.0) || s.link.is_some() && s.visible && s.rect().contains(w - self.origin_of(s.parent))
        })?;
        self.chain(hit).into_iter().find_map(|k| {
            let v: Vec<Interaction> = self.shapes[k].interactions().into_iter().filter(|ix| ix.trigger == trigger && (trigger != Trigger::Drag || ix.opts.drag_dir == 0 || ix.opts.drag_dir == self.present_dir)).collect();
            (!v.is_empty()).then_some((k, v))
        })
    }

    // a variable's current value for an expression, found by name ("count" or "Collection / count"), as seen from layer `at`
    fn expr_lookup(&self, at: usize, name: &str) -> Option<crate::expr::Val> {
        use crate::expr::Val;
        for c in &self.variables {
            for v in &c.vars {
                if v.name == name || format!("{} / {}", c.name, v.name) == name || format!("{}/{}", c.name, v.name) == name {
                    return Some(match self.variable_value(v.id, at)? {
                        VarValue::Number(n) => Val::Num(n as f64),
                        VarValue::Bool(b) => Val::Bool(b),
                        VarValue::Text(t) => Val::Str(t),
                        VarValue::Color(c) => Val::Str(format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())),
                        VarValue::Alias(_) => return None,
                    });
                }
            }
        }
        None
    }

    fn condition_holds(&self, k: usize, ix: &Interaction) -> bool {
        ix.condition.trim().is_empty() || crate::expr::eval(&ix.condition, &|n| self.expr_lookup(k, n)).map(|v| v.truthy()) == Ok(true)
    }

    // While hovering / pressing: the instance around layer `k` changes to the linked variant and is put back by `revert_while`
    fn apply_while(&mut self, k: usize, ix: &Interaction, press: bool) {
        if ix.action != Action::ChangeTo || !self.condition_holds(k, ix) {
            return;
        }
        let Some(target) = ix.link.filter(|&t| self.index_of(t).is_some()) else { return };
        let Some(inst) = self.chain(k).into_iter().find(|&c| self.shapes[c].instance) else { return };
        let area = self.world_rect(k);
        if self.present_while.iter().any(|s| s.press == press && s.area == area) {
            return;
        }
        let (Some(back), inst_id) = (self.main_of(inst).map(|m| self.shapes[m].id), self.shapes[inst].id) else { return };
        if let Some(new) = self.swap_instance(inst_id, target) {
            self.present_while.push(WhileState { press, area, inst: new, back });
        }
    }

    fn revert_while(&mut self, s: WhileState) {
        self.present_while.retain(|x| *x != s);
        if self.index_of(s.inst).is_some() {
            self.swap_instance(s.inst, s.back);
        }
    }

    // the pointer went down at `w`: While pressing changes start
    pub(crate) fn present_press(&mut self, w: Pos2) {
        if self.top_overlay().is_some() {
            return;
        }
        if let Some((k, ixs)) = self.present_targets_at(w, Trigger::WhilePress) {
            for ix in ixs {
                self.apply_while(k, &ix, true);
            }
        }
    }

    // the pointer went up anywhere: While pressing changes go back
    pub(crate) fn present_release(&mut self) {
        for s in self.present_while.clone().into_iter().filter(|s| s.press) {
            self.revert_while(s);
        }
    }

    // run one interaction of shape `k` (when its condition holds)
    pub(crate) fn present_fire(&mut self, k: usize, ix: &Interaction, now: f64) {
        if !self.condition_holds(k, ix) {
            return;
        }
        match ix.action {
            Action::Navigate | Action::Back => {
                // what the screen we leave keeps: its scroll, the variants of interactive components, the videos' places
                if let Some(from) = self.present {
                    self.present_scrolls.insert(from, self.present_scroll);
                }
                if ix.opts.reset_components {
                    if let Some(base) = self.present_shapes.clone() {
                        self.shapes = base;
                    }
                }
                if ix.opts.reset_video {
                    self.video_ctl.clear();
                    self.video_last.clear();
                } else {
                    self.freeze_videos(now);
                }
                let target = if ix.action == Action::Back { self.present_stack.pop() } else { ix.link };
                let Some(t) = target.filter(|&t| self.index_of(t).is_some()) else { return };
                if let (Action::Navigate, Some(from)) = (ix.action, self.present) {
                    self.present_stack.push(from);
                }
                self.present_anim = match (self.present, ix.transition) {
                    (_, Transition::Instant) => None,
                    (Some(from), _) if from != t && ix.duration > 0.0 => Some(PresentAnim { from, t0: now, tr: ix.transition, ease: ix.easing, dur: ix.duration }),
                    _ => None,
                };
                self.present = Some(t);
                self.present_t0 = None;
                self.present_scroll = if ix.opts.keep_scroll { self.present_scrolls.get(&t).copied().unwrap_or(Vec2::ZERO) } else { Vec2::ZERO };
                self.present_fired = false;
                self.present_hover = None;
                self.present_overlays.clear();
            }
            Action::SetMode => {
                if ix.opts.coll != 0 {
                    self.page_modes.retain(|(c, _)| *c != ix.opts.coll);
                    self.page_modes.push((ix.opts.coll, ix.opts.mode));
                }
            }
            Action::Video => {
                let Some(vid) = ix.link.and_then(|t| self.index_of(t)).and_then(|t| self.shapes[t].fills.iter().find_map(|p| p.video)) else { return };
                let pos = self.video_pos(vid, now);
                let playing = self.video_ctl.get(&vid).is_none_or(|c| c.playing);
                let playing = match ix.opts.video_op {
                    1 => true,
                    2 => false,
                    _ => !playing,
                };
                self.video_ctl.insert(vid, crate::video::VideoCtl { playing, base: pos, since: now });
            }
            Action::OpenLink => {
                if !ix.url.trim().is_empty() {
                    self.open_url = Some(ix.url.clone());
                }
            }
            Action::OpenOverlay | Action::SwapOverlay => {
                let Some(t) = ix.link.and_then(|t| self.index_of(t)) else { return };
                let Some(base) = self.present.and_then(|p| self.index_of(p)) else { return };
                if ix.action == Action::SwapOverlay {
                    if let Some(n) = self.top_overlay() {
                        self.close_overlay_at(n, now);
                    }
                }
                let (screen, ov) = (self.world_rect(base), self.world_rect(t));
                // 7 = manual: the offset is from the presented frame's top-left
                let at = if ix.overlay_pos == 7 { screen.min + ix.overlay_off } else { overlay_origin(screen, ov.size(), ix.overlay_pos) };
                let shift = at - ov.min;
                self.present_overlays.push(Overlay { frame: self.shapes[t].id, shift, outside: ix.close_outside, dim: ix.overlay_dim, t0: now, tr: ix.transition, ease: ix.easing, dur: ix.duration, closing: None });
            }
            Action::CloseOverlay => {
                // an interaction inside an overlay closes that overlay, any other the topmost one
                let inside = self.present_overlays.iter().rposition(|o| self.index_of(o.frame).is_some_and(|f| f == k || self.chain(k).contains(&f)));
                if let Some(n) = inside.filter(|&n| self.present_overlays[n].closing.is_none()).or_else(|| self.top_overlay()) {
                    self.close_overlay_at(n, now);
                }
            }
            Action::ScrollTo => {
                let (Some(t), Some(f)) = (ix.link.and_then(|t| self.index_of(t)), self.present.and_then(|p| self.index_of(p))) else { return };
                let range = self.present_scroll_range(f);
                let by = self.world_rect(t).min - self.world_rect(f).min;
                self.present_scroll = by.clamp(Vec2::ZERO, range);
            }
            Action::ChangeTo => {
                // the instance this interaction sits in changes to the linked variant (interactive components)
                let Some(target) = ix.link else { return };
                let Some(inst) = self.chain(k).into_iter().map(|c| &self.shapes[c]).find(|s| s.instance).map(|s| s.id) else { return };
                self.swap_instance(inst, target);
            }
            Action::SetVariable => {
                let Some(id) = ix.var else { return };
                // the value may be an expression over the variables ("count + 1"); text that is not one is the value itself
                let text = crate::expr::eval(&ix.value, &|n| self.expr_lookup(k, n)).map_or_else(|_| ix.value.clone(), |v| v.text());
                for c in self.variables.iter_mut() {
                    if let Some(v) = c.vars.iter_mut().find(|v| v.id == id) {
                        if let Some(val) = parse_var_value(v.kind, &text) {
                            v.values.iter_mut().for_each(|x| *x = val.clone());
                        }
                    }
                }
            }
        }
    }

    fn present_fire_all(&mut self, k: usize, ixs: Vec<Interaction>, now: f64) {
        for ix in ixs {
            self.present_fire(k, &ix, now);
        }
    }

    // a click or press in present mode: the topmost overlay takes it (a click outside may close it); otherwise the frame does
    pub(crate) fn present_pointer(&mut self, w: Pos2, trigger: Trigger, now: f64) {
        if let Some(top) = self.top_overlay().map(|n| self.present_overlays[n]) {
            let inside = self.index_of(top.frame).is_some_and(|f| self.world_rect(f).translate(top.shift).contains(w));
            if inside {
                if let Some((k, ixs)) = self.present_targets_at(w - top.shift, trigger) {
                    self.present_fire_all(k, ixs, now);
                }
            } else if trigger == Trigger::Click && top.outside {
                if let Some(n) = self.top_overlay() {
                    self.close_overlay_at(n, now);
                }
            }
            return; // an open overlay blocks what is behind it
        }
        if trigger == Trigger::Click {
            if let Some(url) = self.text_link_at(w) {
                self.open_url = Some(url);
                return;
            }
        }
        if let Some((k, ixs)) = self.present_targets_at(w, trigger) {
            self.present_fire_all(k, ixs, now);
        }
    }

    // the link of the text under `w`: the run under the pointer, judged by how far along the text box it is (the layout is not
    // measured here, so a line that wraps picks the nearest run by position along its width)
    fn text_link_at(&self, w: Pos2) -> Option<String> {
        let i = self.draw_order().into_iter().rev().find(|&i| {
            let s = &self.shapes[i];
            s.kind == Kind::Text && s.visible && s.runs.iter().any(|r| r.url.is_some()) && self.world_rect(i).contains(w)
        })?;
        let s = &self.shapes[i];
        let r = self.world_rect(i);
        let n = s.text.chars().count().max(1);
        let at = (((w.x - r.min.x) / r.width().max(1.0)) * n as f32) as usize;
        s.runs.iter().filter(|r| r.url.is_some()).min_by_key(|run| if (run.start..run.end).contains(&at) { 0 } else { at.abs_diff(run.start).min(at.abs_diff(run.end)) + 1 }).and_then(|r| r.url.clone())
    }

    // click in present mode: follow the On click interactions of the shape under the pointer (or its nearest ancestor with any)
    pub(crate) fn present_click(&mut self, w: Pos2, now: f64) {
        self.present_pointer(w, Trigger::Click, now);
    }

    // pointer moves in present mode: Mouse enter / leave interactions
    pub(crate) fn present_hover_at(&mut self, w: Option<Pos2>, now: f64) {
        let w = w.filter(|_| self.top_overlay().is_none());
        // While hovering: the changes end when the pointer leaves the layer, and start when it is over a layer that has one
        for s in self.present_while.clone().into_iter().filter(|s| !s.press) {
            if !w.is_some_and(|w| s.area.contains(w)) {
                self.revert_while(s);
            }
        }
        if let Some((k, ixs)) = w.and_then(|w| self.present_targets_at(w, Trigger::WhileHover)) {
            for ix in ixs {
                self.apply_while(k, &ix, false);
            }
        }
        let enter = w.and_then(|w| self.present_targets_at(w, Trigger::MouseEnter));
        let leave_over = w.and_then(|w| self.present_targets_at(w, Trigger::MouseLeave));
        let now_over = enter.as_ref().map(|e| e.0).or(leave_over.as_ref().map(|e| e.0));
        let was = self.present_hover;
        if now_over == was {
            return;
        }
        self.present_hover = now_over;
        let leaves: Vec<Interaction> = was.and_then(|k| self.shapes.get(k)).map(|s| s.interactions().into_iter().filter(|ix| ix.trigger == Trigger::MouseLeave).collect()).unwrap_or_default();
        if let (Some(k), false) = (was, leaves.is_empty()) {
            self.present_fire_all(k, leaves, now);
        }
        // a Mouse enter fires on arriving, also when the pointer comes straight from another layer
        if let Some((k, ixs)) = enter.filter(|e| Some(e.0) == now_over) {
            self.present_fire_all(k, ixs, now);
        }
    }

    // After delay interactions inside the presented frame, once the delay has passed. Returns the next one's wait in seconds.
    pub(crate) fn present_delays(&mut self, now: f64) -> Option<f32> {
        let frame = self.present.and_then(|id| self.index_of(id))?;
        let t0 = self.present_t0?;
        let mut due: Vec<(usize, Interaction)> = vec![];
        for k in (0..self.shapes.len()).filter(|&k| self.shapes[k].visible && (k == frame || self.chain(k).contains(&frame))) {
            due.extend(self.shapes[k].interactions().into_iter().filter(|ix| ix.trigger == Trigger::AfterDelay).map(|ix| (k, ix)));
        }
        if self.present_fired || due.is_empty() {
            return None;
        }
        let elapsed = (now - t0) as f32;
        match due.iter().filter(|(_, ix)| ix.delay <= elapsed).min_by(|a, b| a.1.delay.total_cmp(&b.1.delay)).cloned() {
            Some((k, ix)) => {
                self.present_fired = true;
                self.present_fire(k, &ix, now);
                None
            }
            None => due.iter().map(|(_, ix)| ix.delay - elapsed).reduce(f32::min),
        }
    }

    // a key went down in present mode: the Key press interactions of the presented frame and its open overlays with that key
    pub(crate) fn present_key(&mut self, key: &str, now: f64) {
        let mut frames: Vec<usize> = self.present.and_then(|p| self.index_of(p)).into_iter().collect();
        frames.extend(self.present_overlays.iter().filter(|o| o.closing.is_none()).filter_map(|o| self.index_of(o.frame)));
        let mut hits: Vec<(usize, Interaction)> = vec![];
        for k in (0..self.shapes.len()).filter(|&k| self.shapes[k].visible && frames.iter().any(|&f| f == k || self.chain(k).contains(&f))) {
            hits.extend(self.shapes[k].interactions().into_iter().filter(|ix| ix.trigger == Trigger::KeyPress && ix.key.eq_ignore_ascii_case(key.trim())).map(|ix| (k, ix)));
        }
        for (k, ix) in hits {
            self.present_fire(k, &ix, now);
        }
    }

    // where video `vid` is, in seconds from its start (it loops, so this keeps growing)
    pub(crate) fn video_pos(&self, vid: u64, now: f64) -> f32 {
        match self.video_ctl.get(&vid) {
            Some(c) if c.playing => c.base + (now - c.since) as f32,
            Some(c) => c.base,
            None => (now - self.present_t0.unwrap_or(now)) as f32,
        }
    }

    // every video keeps playing from where it is although the clock that started it is about to restart
    fn freeze_videos(&mut self, now: f64) {
        for (_, vid) in self.video_fills() {
            if !self.video_ctl.contains_key(&vid) {
                let pos = self.video_pos(vid, now);
                self.video_ctl.insert(vid, crate::video::VideoCtl { playing: true, base: pos, since: now });
            }
        }
    }

    // When video hits a time / When video ends: fired as each playing video passes the moment (it loops, so the end comes again)
    pub(crate) fn present_video_events(&mut self, now: f64) {
        let Some(frame) = self.present.and_then(|id| self.index_of(id)) else { return };
        let videos: Vec<(usize, u64)> = (0..self.shapes.len())
            .filter(|&k| self.shapes[k].visible && (k == frame || self.chain(k).contains(&frame)))
            .filter_map(|k| self.shapes[k].fills.iter().find_map(|p| p.video).map(|v| (k, v)))
            .collect();
        for (k, vid) in videos {
            let Some(len) = self.clips.get(&vid).map(|c| c.seconds).filter(|&l| l > 0.05) else { continue };
            let pos = self.video_pos(vid, now);
            let Some(last) = self.video_last.insert(vid, pos) else { continue };
            if pos <= last {
                continue; // paused, or restarted
            }
            let ended = (pos / len).floor() > (last / len).floor();
            let (a, b) = (last % len, pos % len);
            let ixs = self.shapes[k].interactions();
            for ix in ixs {
                let hit = match ix.trigger {
                    Trigger::VideoEnd => ended,
                    Trigger::VideoTime => {
                        let t = ix.delay.clamp(0.0, len);
                        if ended { t > a || t <= b } else { t > a && t <= b }
                    }
                    _ => false,
                };
                if hit {
                    self.present_fire(k, &ix, now);
                }
            }
        }
    }

    // the topmost overlay that is not on its way out
    pub(crate) fn top_overlay(&self) -> Option<usize> {
        self.present_overlays.iter().rposition(|o| o.closing.is_none())
    }

    // the overlay at `n` goes: its animation runs backwards when it has one, otherwise it is gone at once
    fn close_overlay_at(&mut self, n: usize, now: f64) {
        let o = self.present_overlays[n];
        if o.closing.is_some() {
            return;
        }
        if o.tr != Transition::Instant && o.dur > 0.0 {
            self.present_overlays[n].closing = Some(now);
        } else {
            self.present_overlays.remove(n);
        }
    }

    // drops the overlays whose closing animation has run
    pub(crate) fn prune_overlays(&mut self, now: f64) {
        self.present_overlays.retain(|o| o.closing.is_none_or(|c| now - c < o.dur as f64));
    }

    // the overlays with the frame ids and where they sit on screen, bottom first (for drawing)
    pub(crate) fn overlay_list(&self) -> Vec<Overlay> {
        self.present_overlays.iter().copied().filter(|o| self.index_of(o.frame).is_some()).collect()
    }
}

// where the top-left of an overlay of `size` goes inside `screen`: 0 centre, 1..=3 top left / centre / right, 4..=6 bottom
fn overlay_origin(screen: Rect, size: Vec2, pos: u8) -> Pos2 {
    let x = match pos {
        1 | 4 => screen.min.x,
        2 | 5 => screen.center().x - size.x / 2.0,
        3 | 6 => screen.max.x - size.x,
        _ => screen.center().x - size.x / 2.0,
    };
    let y = match pos {
        1..=3 => screen.min.y,
        4..=6 => screen.max.y - size.y,
        _ => screen.center().y - size.y / 2.0,
    };
    Pos2::new(x, y)
}

// text typed in the Set variable action's value field, read as the variable's own kind
fn parse_var_value(kind: VarKind, text: &str) -> Option<VarValue> {
    let t = text.trim();
    match kind {
        VarKind::Number => t.parse().ok().map(VarValue::Number),
        VarKind::Bool => match t.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(VarValue::Bool(true)),
            "false" | "0" | "no" | "off" => Some(VarValue::Bool(false)),
            _ => None,
        },
        VarKind::Text => Some(VarValue::Text(text.to_owned())),
        VarKind::Color => {
            let h = t.trim_start_matches('#');
            if h.len() < 6 {
                let v = crate::fig::parse_hex(h)?;
                return Some(VarValue::Color(egui::Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)));
            }
            let v = u32::from_str_radix(h, 16).ok()?;
            match h.len() {
                6 => Some(VarValue::Color(egui::Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))),
                8 => Some(VarValue::Color(egui::Color32::from_rgba_unmultiplied((v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8))),
                _ => None,
            }
        }
    }
}
