// Components and instances.
//
// An instance is a copy of a component's subtree whose shapes remember what they mirror (`Shape::src`).
// When a shape in the main component changes, every mirrored copy that still has the old value follows;
// a value the user changed on the instance is an override and is left alone.
use std::collections::HashMap;

use egui::{Pos2, Vec2};

use crate::model::*;
use crate::App;

fn sync_fields(c: &mut Shape, p: &Shape, m: &Shape, root: bool) {
    macro_rules! sync {
        ($($f:ident),*) => { $( if m.$f != p.$f && c.$f == p.$f { c.$f = m.$f.clone(); } )* };
    }
    sync!(
        kind, text, rotation, radius, opacity, fills, strokes, stroke_w, width_profile, width_points, dash, dash_cap, dynamic, dash_pattern, stroke_align, stroke_cap, stroke_join, miter_angle, stroke_sides, effects, modes, bindings, cprop_link, visible, locked, clip, mask,
        lock_aspect, sides, inner, arc, path, font_size, mono, align, line_h, fit, layout, fill_x, fill_y, abs, min_size, max_size, ch, cv, grids, size,
        slot, slot_min, slot_max, slot_pref, link, trigger, action, delay, url, key, var, value, overlay_pos, overlay_off, overlay_dim, close_outside, condition, more, transition, easing, duration, corner_smooth
    );
    if !root {
        sync!(pos, name);
    }
}

// the fields of `new` that the old instance layer `old` had changed from its main `old_main` (an override) take the old value, when
// `new` still shows what the new main has there
fn keep_overrides(new: &mut Shape, old: &Shape, old_main: &Shape, new_main: &Shape) {
    macro_rules! keep {
        ($($f:ident),*) => { $( if old.$f != old_main.$f && new.$f == new_main.$f { new.$f = old.$f.clone(); } )* };
    }
    keep!(text, rotation, radius, opacity, fills, strokes, stroke_w, effects, visible, font_size, mono, align, line_h, bold, weight, italic, family, runs);
}

fn kept_children(kept: &[(Vec<String>, Shape, Option<Shape>)], path: &[String]) -> Vec<Shape> {
    kept.iter().filter(|(p, _, _)| p.len() == path.len() + 1 && p.starts_with(path)).map(|(_, s, _)| s.clone()).collect()
}

// copies the fields a component's own layer carries from `from` (a library's version of it) into `to`; the root keeps its place
pub fn copy_fields(to: &mut Shape, from: &Shape, root: bool) {
    macro_rules! copy {
        ($($f:ident),*) => { $( to.$f = from.$f.clone(); )* };
    }
    copy!(
        kind, text, rotation, radius, opacity, fills, strokes, stroke_w, width_profile, width_points, dash, dash_cap, dynamic, dash_pattern, stroke_align, stroke_cap, stroke_join, miter_angle, stroke_sides, effects, modes, bindings, cprop_link, visible, locked, clip, mask, mask_kind, lock_aspect, sides, inner,
        arc, path, font_size, mono, align, line_h, fit, layout, fill_x, fill_y, abs, min_size, max_size, ch, cv, grids, size, slot, slot_min, slot_max, slot_pref, link, trigger, action, delay, url, key, var, value, overlay_pos, overlay_off, overlay_dim, close_outside, condition, more, transition, easing, duration,
        bold, weight, italic, case, underline, strike, list, para_spacing, para_indent, max_lines, script, features, runs, family, tracking, valign, vtrim, opsz, wrap_style, effect_style, text_style, grid_style, cprops, variant, component_set
    );
    if !root {
        copy!(pos, name);
    }
}

impl App {
    // what can be instantiated: standalone components, and each component set once (as its first variant)
    pub fn components(&self) -> Vec<(u64, String)> {
        let mut out = vec![];
        for (i, s) in self.shapes.iter().enumerate() {
            if s.component_set {
                if let Some(&first) = self.variants_of(i).first() {
                    out.push((self.shapes[first].id, s.name.clone()));
                }
            } else if s.component && self.set_of(i).is_none() {
                out.push((s.id, s.name.clone()));
            }
        }
        out
    }

    // a frame exactly around shape `id`, which moves inside it
    fn wrap_in_frame(&mut self, id: u64) -> Option<u64> {
        let i = self.index_of(id)?;
        let parent = self.shapes[i].parent;
        let r = self.shapes[i].aabb();
        let fid = self.new_id();
        let mut f = Shape::new(Kind::Frame, fid, self.shapes.len() + 1, r.min, parent);
        f.size = r.size();
        f.fills.clear();
        f.clip = false;
        self.shapes.insert(i + 1, f);
        let i = self.index_of(id)?;
        self.shapes[i].parent = Some(fid);
        self.shapes[i].pos -= r.min.to_vec2();
        Some(fid)
    }

    pub fn make_component(&mut self) {
        let mut out = vec![];
        for id in self.sel_roots() {
            let Some(i) = self.index_of(id) else { continue };
            let target = match self.shapes[i].kind {
                Kind::Frame => id,
                Kind::Group => {
                    self.shapes[i].kind = Kind::Frame;
                    self.shapes[i].clip = false;
                    self.shapes[i].fills.clear();
                    id
                }
                _ => match self.wrap_in_frame(id) {
                    Some(f) => f,
                    None => continue,
                },
            };
            if let Some(t) = self.index_of(target) {
                self.shapes[t].component = true;
                self.shapes[t].instance = false;
                self.shapes[t].src = None;
                out.push(target);
            }
        }
        if !out.is_empty() {
            self.sel = out;
        }
    }

    // places a new instance of component `main` under `parent` at `pos` (parent space)
    pub fn instantiate(&mut self, main: u64, parent: Option<u64>, pos: Pos2) -> Option<u64> {
        self.index_of(main).filter(|&i| self.shapes[i].kind == Kind::Frame)?;
        if parent.is_some_and(|p| self.is_inside(p, main)) {
            return None; // an instance can't live inside its own component
        }
        let subtree: Vec<Shape> = self.shapes.iter().filter(|s| self.is_inside(s.id, main)).cloned().collect();
        let (mut copies, ids) = self.remap(&subtree, Some(parent), Vec2::ZERO);
        let back: HashMap<u64, u64> = ids.iter().map(|(&old, &new)| (new, old)).collect();
        for c in &mut copies {
            c.src = back.get(&c.id).copied();
            c.component = false;
            c.component_set = false;
            c.variant.clear();
            c.cprops.clear();
            // the copy of a nested instance (an instance inside the component) is an instance too
            c.instance = c.src == Some(main) || c.instance;
            c.swap = None;
        }
        let root = ids[&main];
        // an instance of a variant is named after its component set, like Figma
        let set_name = self.index_of(main).and_then(|m| self.set_of(m)).map(|s| self.shapes[s].name.clone());
        if let Some(r) = copies.iter_mut().find(|c| c.id == root) {
            r.pos = pos;
            if let Some(n) = set_name {
                r.name = n;
            }
        }
        self.shapes.extend(copies);
        self.sel = vec![root];
        Some(root)
    }

    // an instance dropped next to its component
    pub fn instantiate_next_to(&mut self, main: u64) {
        let Some(i) = self.index_of(main) else { return };
        // beside the component, or beside its whole set (never inside it)
        let a = self.set_of(i).unwrap_or(i);
        let (parent, pos) = (self.shapes[a].parent, self.shapes[a].pos + Vec2::new(self.shapes[a].size.x + 40.0, 0.0));
        self.instantiate(main, parent, pos);
    }

    // an instance in the middle of the view
    pub fn instantiate_in_view(&mut self, main: u64) {
        let Some(i) = self.index_of(main) else { return };
        let c = self.to_world(self.view.min, self.view.center());
        let parent = self.frame_at(c, &[main]);
        let pos = c - self.shapes[i].size / 2.0 - self.origin_of(parent);
        self.instantiate(main, parent, pos);
    }

    pub fn detach_instances(&mut self) {
        for id in self.sel_roots() {
            let ids: Vec<u64> = self.shapes.iter().filter(|s| self.is_inside(s.id, id)).map(|s| s.id).collect();
            for s in self.shapes.iter_mut().filter(|s| ids.contains(&s.id)) {
                s.src = None;
                s.instance = false;
            }
        }
    }

    // the layers of the subtree of `root` by their name path from the root (siblings of one name are told apart by order)
    pub(crate) fn name_paths(&self, root: u64) -> Vec<(Vec<String>, usize)> {
        fn walk(app: &App, id: u64, path: Vec<String>, out: &mut Vec<(Vec<String>, usize)>) {
            let Some(i) = app.index_of(id) else { return };
            out.push((path.clone(), i));
            let mut seen: HashMap<String, usize> = HashMap::new();
            for k in (0..app.shapes.len()).filter(|&k| app.shapes[k].parent == Some(id)) {
                let n = seen.entry(app.shapes[k].name.clone()).or_insert(0);
                let mut p = path.clone();
                p.push(format!("{}#{}", app.shapes[k].name, n));
                *n += 1;
                walk(app, app.shapes[k].id, p, out);
            }
        }
        let mut out = vec![];
        walk(self, root, vec![], &mut out);
        out
    }

    // the main component an instance shows: what it was swapped to, else what its `src` leads to (a nested instance mirrors an
    // instance of the outer main, which leads on to the component)
    pub fn main_of(&self, i: usize) -> Option<usize> {
        let s = &self.shapes[i];
        if !s.instance {
            return None;
        }
        if let Some(m) = s.swap.and_then(|m| self.index_of(m)) {
            return Some(m);
        }
        let mut k = self.index_of(s.src?)?;
        for _ in 0..16 {
            if self.shapes[k].component {
                return Some(k);
            }
            if let Some(m) = self.shapes[k].swap.and_then(|m| self.index_of(m)) {
                return Some(m);
            }
            k = self.index_of(self.shapes[k].src?)?;
        }
        None
    }

    // the nested instance `id` goes back to what the outer main has there: a fresh copy of that nested instance, its swap and overrides gone
    fn mirror_nested(&mut self, id: u64) -> Option<u64> {
        let i = self.index_of(id)?;
        let outer = self.shapes[i].src.filter(|&o| self.index_of(o).is_some())?;
        let (parent, was_selected) = (self.shapes[i].parent, self.sel.contains(&id));
        self.remove_subtree(id);
        let sub: Vec<Shape> = self.shapes.iter().filter(|s| self.is_inside(s.id, outer)).cloned().collect();
        let (mut copies, ids) = self.remap(&sub, Some(parent), Vec2::ZERO);
        let back: HashMap<u64, u64> = ids.iter().map(|(&old, &new)| (new, old)).collect();
        for c in &mut copies {
            c.src = back.get(&c.id).copied();
            c.component = false;
            c.component_set = false;
            c.variant.clear();
            c.cprops.clear();
            c.swap = None;
        }
        let root = ids[&outer];
        self.shapes.extend(copies);
        if was_selected {
            self.sel = vec![root];
        }
        Some(root)
    }

    // Reset all overrides: the instance becomes what its main component says again (its place and name stay)
    pub fn reset_overrides(&mut self, id: u64) {
        let Some(i) = self.index_of(id).filter(|&i| self.shapes[i].instance) else { return };
        let nested = self.shapes[i].src.and_then(|s| self.index_of(s)).is_some_and(|k| !self.shapes[k].component);
        if nested {
            self.mirror_nested(id);
        } else if let Some(m) = self.main_of(i) {
            let main = self.shapes[m].id;
            self.swap_inner(id, main, false);
        }
    }

    // returns the id of the new instance (a swap makes a fresh copy)
    pub fn swap_instance(&mut self, id: u64, new_main: u64) -> Option<u64> {
        self.swap_inner(id, new_main, true)
    }

    // `id` becomes an instance of `new_main`; with `keep`, what was overridden in the old instance (a changed text, colour, size...)
    // is kept on the layers of the new one that have the same name path, as in Figma. A nested instance swapped to another
    // component remembers the swap (`Shape::swap`) and keeps its place in the outer main.
    fn swap_inner(&mut self, id: u64, new_main: u64, keep: bool) -> Option<u64> {
        let i = self.index_of(id).filter(|&i| self.shapes[i].instance)?;
        let outer = self.shapes[i].src.filter(|&s| self.index_of(s).is_some_and(|k| !self.shapes[k].component));
        if let Some(o) = outer {
            // swapped back to what the outer main has there: that is no override at all
            if self.index_of(o).and_then(|k| self.main_of(k)).is_some_and(|k| self.shapes[k].id == new_main) {
                return self.mirror_nested(id);
            }
        }
        let (parent, pos, name) = (self.shapes[i].parent, self.shapes[i].pos, self.shapes[i].name.clone());
        let (cvals, link) = (if keep { self.shapes[i].cvals.clone() } else { vec![] }, self.shapes[i].cprop_link);
        let (modes, bindings) = (self.shapes[i].modes.clone(), self.shapes[i].bindings.clone());
        // every layer of the old instance next to what it mirrored, by path
        let kept: Vec<(Vec<String>, Shape, Option<Shape>)> = self
            .name_paths(id)
            .into_iter()
            .filter(|_| keep)
            .map(|(path, k)| (path, self.shapes[k].clone(), self.shapes[k].src.and_then(|s| self.index_of(s)).map(|s| self.shapes[s].clone())))
            .collect();
        // what the instance put into its slots itself
        let slot_content: Vec<(Vec<String>, Vec<Shape>)> = self
            .name_paths(id)
            .into_iter()
            .filter(|(_, k)| keep && self.shapes[*k].slot)
            .map(|(path, k)| {
                let sid = self.shapes[k].id;
                let own: Vec<u64> = self.shapes.iter().filter(|s| s.parent == Some(sid) && s.src.is_none()).map(|s| s.id).collect();
                (path, self.shapes.iter().filter(|s| own.iter().any(|&o| self.is_inside(s.id, o))).cloned().collect())
            })
            .filter(|(_, v): &(Vec<String>, Vec<Shape>)| !v.is_empty())
            .collect();
        let slot = i;
        self.remove_subtree(id);
        let created = self.instantiate(new_main, parent, pos);
        if let Some(new) = created {
            if let Some(n) = self.index_of(new) {
                self.shapes[n].name = name;
                self.shapes[n].cvals = cvals;
                self.shapes[n].cprop_link = link;
                self.shapes[n].modes = modes;
                self.shapes[n].bindings = bindings;
                if let Some(o) = outer {
                    self.shapes[n].src = Some(o);
                    self.shapes[n].swap = Some(new_main);
                }
                let s = self.shapes.remove(n);
                let at = slot.min(self.shapes.len());
                self.shapes.insert(at, s);
            }
            // nested instances that were swapped inside the old one are swapped the same way in the new one
            let mut fresh = self.name_paths(new);
            let swaps: Vec<(u64, u64)> = kept
                .iter()
                .filter(|(p, o, _)| !p.is_empty() && o.instance && o.swap.is_some())
                .filter_map(|(p, o, _)| fresh.iter().find(|(fp, _)| fp == p).map(|&(_, k)| (k, o.swap.unwrap())))
                .filter(|&(k, _)| self.shapes[k].instance)
                .map(|(k, c)| (self.shapes[k].id, c))
                .collect();
            for (nid, comp) in swaps {
                self.swap_instance(nid, comp);
            }
            if keep {
                fresh = self.name_paths(new);
            }
            for (path, old, main) in kept.iter().filter(|(p, _, m)| !p.is_empty() && m.is_some()) {
                let (Some(&(_, k)), Some(main)) = (fresh.iter().find(|(p, _)| p == path), main) else { continue };
                let main_now = self.shapes[k].src.and_then(|s| self.index_of(s)).map(|s| self.shapes[s].clone());
                if let Some(main_now) = main_now {
                    keep_overrides(&mut self.shapes[k], old, main, &main_now);
                }
            }
            // content a slot got in the old instance stays with the slot: the new main's default content makes room for it
            for (path, subtree) in &slot_content {
                let Some(&(_, k)) = fresh.iter().find(|(p, _)| p == path) else { continue };
                if !self.shapes[k].slot {
                    continue;
                }
                let sid = self.shapes[k].id;
                let defaults: Vec<u64> = self.shapes.iter().filter(|s| s.parent == Some(sid) && s.src.is_some()).map(|s| s.id).collect();
                for d in defaults {
                    self.remove_subtree(d);
                }
                let (copies, _) = self.remap(subtree, Some(Some(sid)), Vec2::ZERO);
                self.shapes.extend(copies);
            }
        }
        created
    }

    // `before` is the document as it was when the frame began
    pub fn sync_instances(&mut self, before: &[Shape]) {
        if !self.shapes.iter().any(|s| s.src.is_some()) {
            return;
        }
        let prev: HashMap<u64, &Shape> = before.iter().map(|s| (s.id, s)).collect();
        for ci in 0..self.shapes.len() {
            let Some(src) = self.shapes[ci].src else { continue };
            let (Some(mi), Some(p)) = (self.index_of(src), prev.get(&src)) else { continue };
            if self.shapes[mi] == **p {
                continue;
            }
            let m = self.shapes[mi].clone();
            // only an instance of a main component has a place of its own; a nested instance's belongs to the outer main
            let root = self.shapes[ci].instance && m.component;
            sync_fields(&mut self.shapes[ci], p, &m, root);
        }
        let roots: Vec<(u64, u64)> = self.shapes.iter().filter(|s| s.instance).filter_map(|s| s.swap.or(s.src).map(|m| (s.id, m))).collect();
        for (inst, main) in roots {
            self.sync_children(main, inst);
        }
    }

    // adds / removes instance children so they match the main's children, then recurses
    fn sync_children(&mut self, main: u64, inst: u64) {
        // a slot's content belongs to each instance: what the main has inside is only where it starts
        if self.index_of(main).is_some_and(|m| self.shapes[m].slot) {
            return;
        }
        let kids = |app: &App, p: u64| -> Vec<(u64, Option<u64>)> {
            app.shapes.iter().filter(|s| s.parent == Some(p)).map(|s| (s.id, s.src)).collect()
        };
        let m_kids: Vec<u64> = kids(self, main).into_iter().map(|k| k.0).collect();
        for (id, src) in kids(self, inst) {
            if src.is_some_and(|s| !m_kids.contains(&s)) {
                self.remove_subtree(id);
            }
        }
        let i_kids = kids(self, inst);
        for &mk in &m_kids {
            if i_kids.iter().any(|k| k.1 == Some(mk)) || self.is_inside(inst, mk) {
                continue;
            }
            let sub: Vec<Shape> = self.shapes.iter().filter(|s| self.is_inside(s.id, mk)).cloned().collect();
            let (mut copies, ids) = self.remap(&sub, Some(Some(inst)), Vec2::ZERO);
            let back: HashMap<u64, u64> = ids.iter().map(|(&o, &n)| (n, o)).collect();
            for c in &mut copies {
                c.src = back.get(&c.id).copied();
                c.component = false;
                c.component_set = false;
                c.variant.clear();
                c.cprops.clear();
                c.swap = None;
            }
            self.shapes.extend(copies);
        }
        for (id, src) in kids(self, inst) {
            if let Some(mk) = src.filter(|s| m_kids.contains(s)) {
                // a nested instance swapped here mirrors its swapped component, not the outer main's nested instance
                let mirror = self.index_of(id).and_then(|k| self.shapes[k].swap).unwrap_or(mk);
                self.sync_children(mirror, id);
            }
        }
    }
}
