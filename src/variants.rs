// Component sets (variants) and component properties, like Figma's.
//
// A component set is a frame (`Shape::component_set`) whose children are variant components; each variant carries its
// property values (`Shape::variant`, e.g. Size = Large, State = Hover). An instance shows one variant: it is an instance of that
// variant component, and choosing another value for a property swaps it to the variant with that combination.
//
// Component properties (`Shape::cprops`, on a component or on its set) are a boolean, a text or an instance swap. A layer inside the
// component linked to a property (`Shape::cprop_link`) takes its value from each instance (`Shape::cvals`), or the property's
// default; `resolve_component_props` (run by `settle`) writes those values into the instances' layers.
use egui::{Pos2, Vec2};

use crate::model::*;
use crate::App;

const GAP: f32 = 20.0;

// "Size=Large, State=Hover": how Figma names a variant
pub fn variant_name(v: &[(String, String)]) -> String {
    v.iter().map(|(k, val)| format!("{k}={val}")).collect::<Vec<_>>().join(", ")
}

impl App {
    // the component set a variant lives in
    pub fn set_of(&self, i: usize) -> Option<usize> {
        let p = self.shapes[i].parent.and_then(|p| self.index_of(p))?;
        (self.shapes[p].component_set && self.shapes[i].component).then_some(p)
    }

    pub fn variants_of(&self, set: usize) -> Vec<usize> {
        let id = self.shapes[set].id;
        (0..self.shapes.len()).filter(|&k| self.shapes[k].parent == Some(id) && self.shapes[k].component).collect()
    }

    // the properties of a set with their values, in the order they first appear
    pub fn variant_properties(&self, set: usize) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = vec![];
        for k in self.variants_of(set) {
            for (name, val) in &self.shapes[k].variant {
                match out.iter_mut().find(|(n, _)| n == name) {
                    Some((_, vals)) => {
                        if !vals.contains(val) {
                            vals.push(val.clone());
                        }
                    }
                    None => out.push((name.clone(), vec![val.clone()])),
                }
            }
        }
        out
    }

    // every variant is named after its values
    fn rename_variants(&mut self, set: usize) {
        for k in self.variants_of(set) {
            if !self.shapes[k].variant.is_empty() {
                self.shapes[k].name = variant_name(&self.shapes[k].variant);
            }
        }
    }

    // lays the variants out in rows inside the set and fits the set around them
    pub fn arrange_variants(&mut self, set: usize) {
        let kids = self.variants_of(set);
        let max_row = 900.0;
        let (mut x, mut y, mut row_h, mut width) = (GAP, GAP, 0.0f32, 0.0f32);
        for k in kids {
            let size = self.shapes[k].size;
            if x > GAP && x + size.x > max_row {
                x = GAP;
                y += row_h + GAP;
                row_h = 0.0;
            }
            self.shapes[k].pos = Pos2::new(x, y);
            x += size.x + GAP;
            row_h = row_h.max(size.y);
            width = width.max(x);
        }
        self.shapes[set].size = Vec2::new(width.max(GAP * 2.0), y + row_h + GAP);
    }

    // Ctrl+Shift+K: the selected components become the variants of a new component set (Property 1 = Variant 1, 2, ...)
    pub fn combine_as_variants(&mut self) {
        let comps: Vec<u64> = self.sel_roots().into_iter().filter(|&id| self.index_of(id).is_some_and(|i| self.shapes[i].component && self.set_of(i).is_none())).collect();
        let Some(&top) = comps.last() else { return };
        let Some(ti) = self.index_of(top) else { return };
        let Some(bb) = comps.iter().filter_map(|&c| self.index_of(c)).map(|i| self.world_rect(i)).reduce(|a, b| a.union(b)) else { return };
        let parent = self.shapes[ti].parent;
        let sid = self.new_id();
        let mut set = Shape::new(Kind::Frame, sid, self.shapes.len() + 1, bb.min - Vec2::splat(GAP) - self.origin_of(parent), parent);
        set.name = format!("Component set {}", self.shapes.iter().filter(|s| s.component_set).count() + 1);
        set.size = bb.size() + Vec2::splat(GAP * 2.0);
        set.fills.clear();
        set.clip = false;
        set.component_set = true;
        let at = ti + 1;
        self.shapes.insert(at, set);
        for (n, &c) in comps.iter().enumerate() {
            let Some(i) = self.index_of(c) else { continue };
            let world = self.shapes[i].pos + self.origin_of(self.shapes[i].parent);
            self.shapes[i].parent = Some(sid);
            self.shapes[i].pos = world - self.origin_of(Some(sid));
            if self.shapes[i].variant.is_empty() {
                self.shapes[i].variant = vec![("Property 1".into(), format!("Variant {}", n + 1))];
            }
        }
        let si = self.index_of(sid).unwrap();
        self.rename_variants(si);
        self.arrange_variants(si);
        self.sel = vec![sid];
    }

    // a copy of variant `i` next to it, with a new value for the first property
    pub fn add_variant(&mut self, i: usize) {
        let Some(set) = self.set_of(i) else { return };
        let (id, sid) = (self.shapes[i].id, self.shapes[set].id);
        let subtree: Vec<Shape> = self.shapes.iter().filter(|s| self.is_inside(s.id, id)).cloned().collect();
        let (mut copies, ids) = self.remap(&subtree, Some(Some(sid)), Vec2::ZERO);
        let n = self.variants_of(set).len() + 1;
        let root = ids[&id];
        if let Some(r) = copies.iter_mut().find(|c| c.id == root) {
            match r.variant.first_mut() {
                Some((_, val)) => *val = format!("Variant {n}"),
                None => r.variant = vec![("Property 1".into(), format!("Variant {n}"))],
            }
        }
        self.shapes.extend(copies);
        let si = self.index_of(sid).unwrap();
        self.rename_variants(si);
        self.arrange_variants(si);
        self.sel = vec![root];
    }

    // sets variant `i`'s value for `prop` (adding the property to this variant if it had none)
    pub fn set_variant_value(&mut self, i: usize, prop: &str, value: &str) {
        let v = &mut self.shapes[i].variant;
        match v.iter_mut().find(|(k, _)| k == prop) {
            Some((_, val)) => *val = value.to_owned(),
            None => v.push((prop.to_owned(), value.to_owned())),
        }
        if let Some(set) = self.set_of(i) {
            self.rename_variants(set);
        }
    }

    pub fn rename_variant_property(&mut self, set: usize, old: &str, new: &str) {
        if new.is_empty() || old == new {
            return;
        }
        for k in self.variants_of(set) {
            for (name, _) in self.shapes[k].variant.iter_mut().filter(|(n, _)| n == old) {
                *name = new.to_owned();
            }
        }
        self.rename_variants(set);
    }

    pub fn add_variant_property(&mut self, set: usize) {
        let n = self.variant_properties(set).len() + 1;
        let name = format!("Property {n}");
        for k in self.variants_of(set) {
            self.shapes[k].variant.push((name.clone(), "Default".into()));
        }
        self.rename_variants(set);
    }

    pub fn remove_variant_property(&mut self, set: usize, name: &str) {
        for k in self.variants_of(set) {
            self.shapes[k].variant.retain(|(n, _)| n != name);
        }
        self.rename_variants(set);
    }

    // an instance (root `inst`) changes one variant property: it becomes an instance of the variant that has the new value and
    // keeps its other values; if no variant has exactly that combination, the one that matches the most is used
    pub fn pick_variant(&mut self, inst: u64, prop: &str, value: &str) {
        let Some(i) = self.index_of(inst).filter(|&i| self.shapes[i].instance) else { return };
        let Some(cur) = self.main_of(i) else { return };
        let Some(set) = self.set_of(cur) else { return };
        let mut want: Vec<(String, String)> = self.shapes[cur].variant.clone();
        match want.iter_mut().find(|(k, _)| k == prop) {
            Some((_, v)) => *v = value.to_owned(),
            None => want.push((prop.to_owned(), value.to_owned())),
        }
        let score = |k: usize| want.iter().filter(|w| self.shapes[k].variant.contains(w)).count() + 100 * usize::from(self.shapes[k].variant.iter().any(|(n, v)| n == prop && v == value));
        let Some(best) = self.variants_of(set).into_iter().max_by_key(|&k| score(k)) else { return };
        let target = self.shapes[best].id;
        if self.shapes[cur].id != target {
            self.swap_instance(inst, target);
        }
    }

    // ---- component properties ----

    // the component (or set) that defines the properties of the component containing layer `i`
    fn prop_owner(&self, i: usize) -> Option<usize> {
        let comp = self.chain(i).into_iter().find(|&k| self.shapes[k].component)?;
        Some(self.set_of(comp).unwrap_or(comp))
    }

    // the property definitions that apply to an instance (or main component) root
    pub fn props_of_root(&self, i: usize) -> Vec<CompProp> {
        let main = if self.shapes[i].instance { self.main_of(i) } else { Some(i) };
        let Some(m) = main else { return vec![] };
        let owner = self.set_of(m).unwrap_or(m);
        self.shapes[owner].cprops.clone()
    }

    // Create property: a boolean for a layer's visibility, a text for a text layer's text, an instance swap for a nested instance.
    // The layer is linked to it. Returns the property id.
    pub fn create_property(&mut self, layer: usize, kind: CompPropKind) -> Option<u64> {
        let owner = self.prop_owner(layer)?;
        let id = self.new_id();
        let s = &self.shapes[layer];
        let (name, default) = match kind {
            CompPropKind::Bool => (format!("Show {}", s.name), CompPropValue::Bool(s.visible)),
            CompPropKind::Text if s.kind == Kind::Text => (s.name.clone(), CompPropValue::Text(s.text.clone())),
            CompPropKind::Swap if s.instance => (s.name.clone(), CompPropValue::Swap(self.shapes[self.main_of(layer)?].id)),
            _ => return None,
        };
        self.shapes[owner].cprops.push(CompProp { id, name, kind, default });
        self.shapes[layer].cprop_link = Some(id);
        // on a layer of a component set the property links the layers of the other variants that have the same name path
        if self.shapes[owner].component_set {
            let layer_id = self.shapes[layer].id;
            if let Some(root) = self.chain(layer).into_iter().find(|&k| self.shapes[k].component).map(|k| self.shapes[k].id) {
                let path = self.name_paths(root).into_iter().find(|(_, k)| self.shapes[*k].id == layer_id).map(|(p, _)| p);
                if let Some(path) = path {
                    for v in self.variants_of(owner) {
                        let vid = self.shapes[v].id;
                        if vid == root {
                            continue;
                        }
                        let hit = self.name_paths(vid).into_iter().find(|(p, k)| *p == path && self.shapes[*k].kind == self.shapes[layer].kind).map(|(_, k)| k);
                        if let Some(k) = hit {
                            self.shapes[k].cprop_link = Some(id);
                        }
                    }
                }
            }
        }
        Some(id)
    }

    // the "+" of the Properties section: a property of `owner` that no layer is linked to yet
    pub fn add_property(&mut self, owner: usize, kind: CompPropKind) {
        let id = self.new_id();
        let n = self.shapes[owner].cprops.len() + 1;
        let (name, default) = match kind {
            CompPropKind::Bool => (format!("Property {n}"), CompPropValue::Bool(true)),
            CompPropKind::Text => (format!("Property {n}"), CompPropValue::Text("Text".into())),
            CompPropKind::Swap => match self.components().first() {
                Some((c, _)) => (format!("Property {n}"), CompPropValue::Swap(*c)),
                None => return,
            },
        };
        self.shapes[owner].cprops.push(CompProp { id, name, kind, default });
    }

    // Create property > Variant: a variant property for the set; a lone component becomes a set of two variants first
    pub fn create_variant_property(&mut self, owner: usize) {
        if self.shapes[owner].component_set {
            self.add_variant_property(owner);
            return;
        }
        let id = self.shapes[owner].id;
        self.sel = vec![id];
        self.combine_as_variants();
        let Some(sid) = self.sel.first().copied() else { return };
        if let Some(first) = self.index_of(sid).and_then(|si| self.variants_of(si).first().copied()) {
            self.add_variant(first);
        }
        self.sel = vec![sid];
    }

    // Create property > Slot: a new empty frame inside the component (in every variant of a set), marked as a slot, so each
    // instance can put its own content there
    pub fn add_slot(&mut self, owner: usize) {
        let targets: Vec<usize> = if self.shapes[owner].component_set { self.variants_of(owner) } else { vec![owner] };
        let n = self.shapes.iter().filter(|s| s.slot).count() + 1;
        let mut last = None;
        for t in targets {
            let (tid, size) = (self.shapes[t].id, self.shapes[t].size);
            let sid = self.new_id();
            let mut f = Shape::new(Kind::Frame, sid, self.shapes.len() + 1, Pos2::new(8.0, 8.0), Some(tid));
            f.name = if n == 1 { "Slot".into() } else { format!("Slot {n}") };
            f.size = Vec2::new((size.x - 16.0).max(24.0), (size.y - 16.0).clamp(24.0, 48.0));
            f.fills.clear();
            f.clip = false;
            f.slot = true;
            self.shapes.push(f);
            last = Some(sid);
        }
        if let Some(sid) = last {
            self.sel = vec![sid];
        }
    }

    pub fn delete_property(&mut self, owner: usize, prop: u64) {
        self.shapes[owner].cprops.retain(|p| p.id != prop);
        for s in &mut self.shapes {
            if s.cprop_link == Some(prop) {
                s.cprop_link = None;
            }
            s.cvals.retain(|(p, _)| *p != prop);
        }
    }

    // writes property values into the linked layers of every instance (and the defaults into the main components)
    pub fn resolve_component_props(&mut self) -> bool {
        if !self.shapes.iter().any(|s| s.cprop_link.is_some()) {
            return false;
        }
        let mut changed = false;
        let mut swaps: Vec<(u64, u64)> = vec![];
        let roots: Vec<usize> = (0..self.shapes.len()).filter(|&i| self.shapes[i].instance || (self.shapes[i].component && !self.shapes[i].component_set)).collect();
        for root in roots {
            let defs = self.props_of_root(root);
            if defs.is_empty() {
                continue;
            }
            let rid = self.shapes[root].id;
            let linked: Vec<usize> = (0..self.shapes.len()).filter(|&k| k != root && self.shapes[k].cprop_link.is_some() && self.is_inside(self.shapes[k].id, rid)).collect();
            for k in linked {
                let Some(def) = defs.iter().find(|d| Some(d.id) == self.shapes[k].cprop_link) else { continue };
                // the nearest root decides: a layer inside a nested instance belongs to that instance's own properties
                let owner_root = self.chain(k).into_iter().skip(1).find(|&c| self.shapes[c].instance || self.shapes[c].component);
                if owner_root.is_some_and(|c| c != root) {
                    continue;
                }
                let val = self.shapes[root].cvals.iter().find(|(p, _)| *p == def.id).map_or(&def.default, |(_, v)| v).clone();
                let cur_main = self.main_of(k).map(|m| self.shapes[m].id);
                let s = &mut self.shapes[k];
                match (def.kind, val) {
                    (CompPropKind::Bool, CompPropValue::Bool(b)) => {
                        if s.visible != b {
                            s.visible = b;
                            changed = true;
                        }
                    }
                    (CompPropKind::Text, CompPropValue::Text(t)) => {
                        if s.text != t {
                            s.text = t;
                            changed = true;
                        }
                    }
                    (CompPropKind::Swap, CompPropValue::Swap(c)) => {
                        if s.instance && cur_main != Some(c) {
                            swaps.push((s.id, c));
                        }
                    }
                    _ => {}
                }
            }
        }
        for (inst, comp) in swaps {
            if self.index_of(comp).is_some_and(|c| self.shapes[c].component) {
                self.swap_instance(inst, comp);
                changed = true;
            }
        }
        changed
    }

    // sets an instance's value for a property
    pub fn set_instance_property(&mut self, inst: usize, prop: u64, value: CompPropValue) {
        let v = &mut self.shapes[inst].cvals;
        match v.iter_mut().find(|(p, _)| *p == prop) {
            Some((_, val)) => *val = value,
            None => v.push((prop, value)),
        }
    }
}
