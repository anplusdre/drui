// More about components: push an instance's overrides to its main component, restore a main component that was deleted, edit all
// variants of a set at once (Q), and swap the components of one library for another's.
use std::collections::HashMap;

use egui::Vec2;

use crate::model::*;
use crate::App;

impl App {
    // the instance that holds layer `i`: the nearest one up the chain
    fn instance_root(&self, i: usize) -> Option<usize> {
        self.chain(i).into_iter().find(|&k| self.shapes[k].instance)
    }

    // Push overrides to main component: what the selected instance changed on its layers is written into the component, and so
    // into every other instance (which follow the component unless they changed it themselves)
    pub fn push_overrides(&mut self) {
        let before = self.shapes.clone();
        let mut pushed = 0;
        for id in self.sel_roots() {
            let Some(root) = self.index_of(id).and_then(|i| if self.shapes[i].instance { Some(i) } else { self.instance_root(i) }) else { continue };
            let rid = self.shapes[root].id;
            let layers: Vec<usize> = self.shapes.iter().enumerate().filter(|(_, s)| s.src.is_some() && self.is_inside(s.id, rid)).map(|(k, _)| k).collect();
            for k in layers {
                let is_root = self.shapes[k].id == rid;
                let Some(m) = self.shapes[k].src.and_then(|s| self.index_of(s)) else { continue };
                if m == k {
                    continue;
                }
                let from = self.shapes[k].clone();
                let to = &mut self.shapes[m];
                macro_rules! push {
                    ($($f:ident),*) => { $( if to.$f != from.$f { to.$f = from.$f.clone(); pushed += 1; } )* };
                }
                push!(text, radius, opacity, fills, strokes, stroke_w, effects, visible, font_size, mono, align, line_h, bold, weight, italic, family, runs);
                if !is_root {
                    push!(rotation);
                }
            }
        }
        if pushed == 0 {
            self.status = "no overrides to push".into();
            return;
        }
        self.sync_instances(&before);
        self.status = format!("pushed {pushed} override{} to the main component", if pushed == 1 { "" } else { "s" });
    }

    // does the selected instance have lost its main component (it was deleted)?
    pub fn main_is_missing(&self, i: usize) -> bool {
        let s = &self.shapes[i];
        s.instance && s.swap.is_none() && s.src.is_some_and(|m| self.index_of(m).is_none())
    }

    // Restore main component: a component is made again from the instance, with the ids its instances remember, so every instance
    // of the deleted one points at it once more
    pub fn restore_main(&mut self) {
        let Some(i) = self.sel_roots().into_iter().filter_map(|id| self.index_of(id)).find(|&i| self.main_is_missing(i)) else {
            self.status = "select an instance whose main component was deleted".into();
            return;
        };
        let root_id = self.shapes[i].id;
        let main_id = self.shapes[i].src.unwrap_or(0);
        let subtree: Vec<Shape> = self.shapes.iter().filter(|s| self.is_inside(s.id, root_id)).cloned().collect();
        // a layer takes the id its `src` names; a layer the main never had (an instance's own slot content) is left out
        let mut ids: HashMap<u64, u64> = HashMap::new();
        for s in &subtree {
            if let Some(src) = s.src.filter(|&m| self.index_of(m).is_none()) {
                ids.insert(s.id, src);
            }
        }
        let beside = self.shapes[i].pos + Vec2::new(self.shapes[i].size.x + 100.0, 0.0);
        let parent = self.shapes[i].parent;
        let mut made = vec![];
        for s in &subtree {
            let Some(&nid) = ids.get(&s.id) else { continue };
            let mut c = s.clone();
            c.id = nid;
            if s.id == root_id {
                c.parent = parent;
                c.pos = beside;
                c.component = true;
                c.instance = false;
                c.name = format!("{} (restored)", s.name);
            } else {
                c.parent = s.parent.and_then(|p| ids.get(&p).copied());
                c.component = false;
            }
            c.src = None;
            c.swap = None;
            if c.parent.is_none() && s.id != root_id {
                continue;
            }
            made.push(c);
        }
        if made.is_empty() {
            return;
        }
        // nested instances inside keep pointing at their own mains
        for c in &mut made {
            let was_nested = subtree.iter().find(|s| ids.get(&s.id) == Some(&c.id)).is_some_and(|s| s.instance && s.id != root_id);
            c.instance = was_nested;
            if was_nested {
                c.src = subtree.iter().find(|s| ids.get(&s.id) == Some(&c.id)).and_then(|s| s.src);
            }
        }
        let _ = main_id;
        self.shapes.extend(made);
        self.sel = vec![ids[&root_id]];
        self.status = "main component restored".into();
    }

    // ---- multi-edit variants (Q)

    // Q: edits to one variant of the selected set are made to all of them
    pub fn toggle_multi_variants(&mut self) {
        if self.multi_variants.is_some() {
            self.multi_variants = None;
            self.status = "multi-edit variants off".into();
            return;
        }
        let set = self.sel.first().and_then(|&id| self.index_of(id)).and_then(|i| if self.shapes[i].component_set { Some(i) } else { self.set_of(i).or_else(|| self.chain(i).into_iter().find_map(|k| self.shapes[k].component_set.then_some(k))) });
        match set {
            Some(s) => {
                self.multi_variants = Some(self.shapes[s].id);
                self.status = "multi-edit variants: changes go to every variant (Q to stop)".into();
            }
            None => self.status = "select a component set or one of its variants first".into(),
        }
    }

    // after an edit: what changed in a layer of one variant is copied to the layer at the same name path in the others
    pub fn sync_variant_edit(&mut self, before: &[Shape]) {
        let Some(set) = self.multi_variants.and_then(|id| self.index_of(id)) else { return };
        let prev: HashMap<u64, &Shape> = before.iter().map(|s| (s.id, s)).collect();
        let variants = self.variants_of(set);
        // (variant, path, layer) triples of every layer that changed
        let mut edits: Vec<(usize, Vec<String>, Shape, Shape)> = vec![];
        for &v in &variants {
            let vid = self.shapes[v].id;
            for (path, k) in self.name_paths(vid) {
                if let Some(p) = prev.get(&self.shapes[k].id) {
                    if **p != self.shapes[k] {
                        edits.push((v, path, (*p).clone(), self.shapes[k].clone()));
                    }
                }
            }
        }
        if edits.is_empty() {
            return;
        }
        // only one variant is being edited at a time: copy its edit to the others
        for (from, path, old, new) in edits {
            for &v in variants.iter().filter(|&&v| v != from) {
                let vid = self.shapes[v].id;
                let Some(&(_, k)) = self.name_paths(vid).iter().find(|(p, _)| *p == path) else { continue };
                let root = path.is_empty();
                let c = &mut self.shapes[k];
                macro_rules! follow {
                    ($($f:ident),*) => { $( if new.$f != old.$f && c.$f == old.$f { c.$f = new.$f.clone(); } )* };
                }
                follow!(text, rotation, radius, opacity, fills, strokes, stroke_w, stroke_align, stroke_cap, stroke_join, dash, effects, visible, font_size, mono, align, line_h, bold, weight, italic, family, runs, layout, fill_x, fill_y, clip, blend);
                if !root {
                    follow!(size);
                }
            }
        }
    }

    // ---- swap libraries

    // every instance of a component from library `from` becomes an instance of the component with the same name in library `to`
    // (which must be added to the file); returns how many instances were swapped
    pub fn swap_library(&mut self, from: u64, to: u64) -> usize {
        let Some(target) = self.library_file(to).cloned() else { return 0 };
        let mut swapped = 0;
        let olds: Vec<(u64, String)> = self.shapes.iter().filter(|s| s.component && s.lib.is_some_and(|(l, _)| l == from)).map(|s| (s.id, s.name.clone())).collect();
        for (old, name) in olds {
            let Some(comp) = target.components.iter().find(|c| c.name == name) else { continue };
            let Some(new_main) = self.import_library_component(to, comp.root) else { continue };
            let insts: Vec<u64> = self.shapes.iter().filter(|s| s.instance && s.src == Some(old) && s.swap.is_none()).map(|s| s.id).collect();
            for inst in insts {
                if self.swap_instance(inst, new_main).is_some() {
                    swapped += 1;
                }
            }
        }
        self.status = format!("swapped {swapped} instance{}", if swapped == 1 { "" } else { "s" });
        swapped
    }
}
