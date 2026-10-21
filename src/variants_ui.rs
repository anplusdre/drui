// The right panel's sections for component sets, variants and component properties (the logic is in variants.rs).
//   Component set: its variant properties and an "Add variant" button.
//   Variant: its value for each property (rename a property, set a value, add / remove a property).
//   Instance: one dropdown per variant property, then the component properties (checkbox, text field or dropdown).
//   Main component: its properties with names and defaults (they are created from a layer's name menu).
use egui::{Align2, Pos2, Rect, Vec2};

use crate::fig::{self, Grid};
use crate::glyphs::Glyph;
use crate::model::*;
use crate::{App, TEXT_SECONDARY};

impl App {
    pub(crate) fn variants_sections(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        if self.shapes[i].component_set {
            self.set_section(ui, g, i);
        } else if self.shapes[i].component {
            if let Some(set) = self.set_of(i) {
                self.variant_values_section(ui, g, i, set);
            }
        } else if self.shapes[i].instance {
            self.instance_variants_section(ui, g, i);
        }
        if (self.shapes[i].component && !self.shapes[i].component_set) || self.shapes[i].instance || self.shapes[i].component_set {
            self.properties_section(ui, g, i);
        }
    }

    fn set_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        fig::header(ui, "Variants", false, &[]);
        let props = self.variant_properties(i);
        if props.is_empty() {
            let r = fig::controls(ui, 4.0);
            fig::text(ui, Pos2::new(g.left, r.center().y), Align2::LEFT_CENTER, "No variant properties yet", fig::reg(11.0), TEXT_SECONDARY());
        }
        for (k, (name, vals)) in props.iter().enumerate() {
            let r = fig::controls(ui, if k == 0 { 4.0 } else { 8.0 });
            fig::ptext(ui.painter(), Pos2::new(g.left, r.center().y), Align2::LEFT_CENTER, name, fig::reg(11.0), TEXT_SECONDARY());
            fig::ptext(ui.painter(), Pos2::new(g.c2, r.center().y), Align2::LEFT_CENTER, vals.join(", "), fig::reg(11.0), crate::TEXT());
        }
        let r = fig::controls(ui, 8.0);
        if fig::button(ui, g.both(r), "Add variant").clicked() {
            if let Some(&first) = self.variants_of(i).first() {
                self.add_variant(first);
            }
        }
        fig::end(ui);
    }

    // a variant's value for each property of its set
    fn variant_values_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize, set: usize) {
        let (_, acts) = fig::header(ui, "Variant properties", false, &[(Glyph::Plus, "Add variant property", false)]);
        if acts[0].clicked() {
            self.add_variant_property(set);
        }
        let id = self.shapes[i].id;
        let mut rename: Option<(String, String)> = None;
        let mut value: Option<(String, String)> = None;
        let mut remove: Option<String> = None;
        for (k, (name, val)) in self.shapes[i].variant.clone().into_iter().enumerate() {
            let r = fig::controls(ui, if k == 0 { 4.0 } else { 8.0 });
            let (mut n, mut v) = (name.clone(), val.clone());
            if fig::text_input(ui, g.col1(r), &mut n, &format!("Variant property {} name", k + 1), "Property").changed() && !n.is_empty() {
                rename = Some((name.clone(), n));
            }
            if fig::text_input(ui, g.col2(r), &mut v, &format!("Variant property {} value", k + 1), "Value").changed() {
                value = Some((name.clone(), v));
            }
            if fig::icon_button(ui, g.icon(r), Glyph::Minus, "Remove variant property", false).clicked() {
                remove = Some(name);
            }
        }
        if let Some((old, new)) = rename {
            self.rename_variant_property(set, &old, &new);
        }
        if let Some((prop, v)) = value {
            if let Some(i) = self.index_of(id) {
                self.set_variant_value(i, &prop, &v);
            }
        }
        if let Some(name) = remove {
            self.remove_variant_property(set, &name);
        }
        let r = fig::controls(ui, 8.0);
        if fig::button(ui, g.both(r), "Add variant").clicked() {
            if let Some(i) = self.index_of(id) {
                self.add_variant(i);
            }
        }
        fig::end(ui);
    }

    // an instance: pick a value for each variant property
    fn instance_variants_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        let Some(cur) = self.main_of(i) else { return };
        let Some(set) = self.set_of(cur) else { return };
        let props = self.variant_properties(set);
        if props.is_empty() {
            return;
        }
        fig::header(ui, "Variant", false, &[]);
        let inst = self.shapes[i].id;
        let mut pick: Option<(String, String)> = None;
        for (k, (name, vals)) in props.iter().enumerate() {
            let r = fig::controls(ui, if k == 0 { 4.0 } else { 8.0 });
            fig::ptext(ui.painter(), Pos2::new(g.left, r.center().y), Align2::LEFT_CENTER, name, fig::reg(11.0), TEXT_SECONDARY());
            let now = self.shapes[cur].variant.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap_or_default();
            // a property whose values are only True and False is a toggle, as Figma shows its Boolean variant properties
            let is_bool = vals.len() == 2 && vals.iter().all(|v| v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("false"));
            if is_bool {
                let mut on = now.eq_ignore_ascii_case("true");
                if fig::checkbox(ui, g.col2(r), &mut on, name) {
                    if let Some(v) = vals.iter().find(|v| v.eq_ignore_ascii_case(if on { "true" } else { "false" })) {
                        pick = Some((name.clone(), v.clone()));
                    }
                }
                continue;
            }
            let mut sel = vals.iter().position(|v| *v == now).unwrap_or(0);
            let items: Vec<(usize, &str)> = vals.iter().enumerate().map(|(n, v)| (n, v.as_str())).collect();
            if fig::dropdown(ui, g.col2(r), None, &mut sel, &items, &format!("Variant {name}")) {
                pick = Some((name.clone(), vals[sel].clone()));
            }
        }
        if let Some((prop, value)) = pick {
            self.pick_variant(inst, &prop, &value);
        }
        fig::end(ui);
    }

    // the "+" menu of the Properties section, as in Figma: Variant, Text, Boolean, Instance swap and Slot. With several components
    // selected (`owner` None) only Variant applies: it combines them into one component set
    pub(crate) fn create_property_menu(&mut self, menu: &egui::Response, owner: Option<usize>) {
        use crate::glyphs::menu_item;
        egui::Popup::menu(menu).show(|ui| {
            if menu_item(ui, None, "Variant", "", true) {
                match owner {
                    Some(o) => self.create_variant_property(o),
                    None => self.combine_as_variants(),
                }
                ui.close();
            }
            for (kind, label) in [(CompPropKind::Text, "Text"), (CompPropKind::Bool, "Boolean"), (CompPropKind::Swap, "Instance swap")] {
                if menu_item(ui, None, label, "", owner.is_some()) {
                    if let Some(o) = owner {
                        self.add_property(o, kind);
                    }
                    ui.close();
                }
            }
            if menu_item(ui, None, "Slot", "", owner.is_some()) {
                if let Some(o) = owner {
                    self.add_slot(o);
                }
                ui.close();
            }
        });
    }

    // component properties: an instance sets values; the main component (or set) edits names and defaults
    fn properties_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        let is_instance = self.shapes[i].instance;
        let owner = if self.shapes[i].component_set {
            Some(i)
        } else if is_instance {
            self.main_of(i).map(|m| self.set_of(m).unwrap_or(m))
        } else {
            Some(self.set_of(i).unwrap_or(i))
        };
        let Some(owner) = owner else { return };
        let defs = self.shapes[owner].cprops.clone();
        if defs.is_empty() && is_instance {
            return;
        }
        // a main component (or set) can always add a property: the "+" opens the menu of kinds
        let (head, b) = fig::header(ui, "Properties", defs.is_empty(), if is_instance { &[] } else { &[(Glyph::Plus, "Create property", false)] });
        if !is_instance {
            let menu = b.first().cloned().unwrap_or(head);
            self.create_property_menu(&menu, Some(owner));
        }
        if defs.is_empty() {
            fig::rule(ui);
            return;
        }
        let comps = self.components();
        let id = self.shapes[i].id;
        let mut set_value: Option<(u64, CompPropValue)> = None;
        let mut rename: Option<(u64, String)> = None;
        let mut delete: Option<u64> = None;
        for (k, def) in defs.iter().enumerate() {
            let r = fig::controls(ui, if k == 0 { 4.0 } else { 8.0 });
            // an instance edits its value, the main component the default
            let cur = if is_instance { self.shapes[i].cvals.iter().find(|(p, _)| *p == def.id).map_or(def.default.clone(), |(_, v)| v.clone()) } else { def.default.clone() };
            if is_instance {
                fig::ptext(&ui.painter().with_clip_rect(Rect::from_min_max(r.min, Pos2::new(g.c2 - 4.0, r.bottom()))), Pos2::new(g.left, r.center().y), Align2::LEFT_CENTER, &def.name, fig::reg(11.0), TEXT_SECONDARY());
            } else {
                let mut n = def.name.clone();
                if fig::text_input(ui, g.col1(r), &mut n, &format!("Property {} name", k + 1), "Name").changed() && !n.is_empty() {
                    rename = Some((def.id, n));
                }
            }
            let f = g.col2(r);
            match cur {
                CompPropValue::Bool(mut b) => {
                    if fig::checkbox(ui, f, &mut b, "On") {
                        set_value = Some((def.id, CompPropValue::Bool(b)));
                    }
                }
                CompPropValue::Text(mut t) => {
                    if fig::text_input(ui, f, &mut t, &format!("{} text", def.name), "Text").changed() {
                        set_value = Some((def.id, CompPropValue::Text(t)));
                    }
                }
                CompPropValue::Swap(mut c) => {
                    let items: Vec<(u64, &str)> = comps.iter().map(|(id, n)| (*id, n.as_str())).collect();
                    if fig::dropdown(ui, f, Some(Glyph::Component), &mut c, &items, &format!("{} component", def.name)) {
                        set_value = Some((def.id, CompPropValue::Swap(c)));
                    }
                }
            }
            if !is_instance && fig::icon_button(ui, g.icon(r), Glyph::Minus, "Delete property", false).clicked() {
                delete = Some(def.id);
            }
        }
        if let Some((pid, v)) = set_value {
            if is_instance {
                if let Some(i) = self.index_of(id) {
                    self.set_instance_property(i, pid, v);
                }
            } else if let Some(p) = self.shapes[owner].cprops.iter_mut().find(|p| p.id == pid) {
                p.default = v;
            }
        }
        if let Some((pid, n)) = rename {
            if let Some(p) = self.shapes[owner].cprops.iter_mut().find(|p| p.id == pid) {
                p.name = n;
            }
        }
        if let Some(pid) = delete {
            self.delete_property(owner, pid);
        }
        let _ = Vec2::ZERO;
        fig::end(ui);
    }
}
