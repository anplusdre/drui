// Slots: a frame inside a component whose content instances may change. The main component sets how many layers a slot takes
// (`slot_min`, `slot_max`, 0 = no limit) and which components it suggests (`slot_pref`); an instance adds layers to it from there.
use egui::{Pos2, Vec2};

use crate::fig::{self, Grid, Num, Prefix};
use crate::glyphs::Glyph;
use crate::model::*;
use crate::App;

impl App {
    // the layers an instance put into slot `slot` itself (what comes from the main component is not counted)
    pub fn slot_content(&self, slot: u64) -> Vec<u64> {
        let mine = self.index_of(slot).and_then(|i| self.shapes[i].src);
        let from_main: Vec<u64> = mine.map(|m| self.shapes.iter().filter(|s| s.parent == Some(m)).map(|s| s.id).collect()).unwrap_or_default();
        self.shapes.iter().filter(|s| s.parent == Some(slot) && s.src.is_none_or(|m| !from_main.contains(&m))).map(|s| s.id).collect()
    }

    // can slot `slot` take one more layer?
    pub fn slot_has_room(&self, slot: u64) -> bool {
        let Some(i) = self.index_of(slot) else { return false };
        let max = self.shapes[i].slot_max;
        max == 0 || self.slot_content(slot).len() < max as usize
    }

    // puts an instance of component `main` into slot `slot`; false when the slot is full
    pub fn slot_add(&mut self, slot: u64, main: u64) -> bool {
        if !self.slot_has_room(slot) {
            self.status = "the slot is full".into();
            return false;
        }
        match self.instantiate(main, Some(slot), Pos2::ZERO) {
            Some(id) => {
                self.sel = vec![id];
                true
            }
            None => false,
        }
    }

    // Slot section of the Design panel: limits and preferred instances on the main component's slot, an Add menu on an instance's
    pub(crate) fn slot_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        if !self.shapes[i].slot || self.shapes[i].kind != Kind::Frame {
            return;
        }
        let id = self.shapes[i].id;
        let in_instance = self.shapes[i].src.is_some();
        let comps = self.components();
        fig::header(ui, "Slot", false, &[]);
        if !in_instance {
            fig::labels(ui, g, true, "Min layers", "Max layers");
            let r = fig::controls(ui, 0.0);
            let num = Num { min: 0.0, max: 99.0, decimals: 0, speed: 0.1, suffix: "" };
            let s = &mut self.shapes[i];
            let (mut lo, mut hi) = (s.slot_min as f32, s.slot_max as f32);
            if fig::num_field(ui, g.col1(r), ui.id().with(("slot-min", id)), Prefix::None, &mut lo, num, "Minimum layers") {
                s.slot_min = lo as u32;
            }
            if fig::num_field(ui, g.col2(r), ui.id().with(("slot-max", id)), Prefix::None, &mut hi, num, "Maximum layers (0 = no limit)") {
                s.slot_max = hi as u32;
            }
            if s.slot_max > 0 && s.slot_min > s.slot_max {
                s.slot_max = s.slot_min;
            }
            fig::labels(ui, g, false, "Preferred instances", "");
            let mut remove = None;
            for (k, &pid) in self.shapes[i].slot_pref.iter().enumerate() {
                let name = comps.iter().find(|c| c.0 == pid).map_or("(deleted component)", |c| c.1.as_str());
                let r = fig::controls(ui, if k == 0 { 0.0 } else { 4.0 });
                fig::input_bg(ui, g.both(r), false);
                fig::text(ui, Pos2::new(g.left + 8.0, r.center().y), egui::Align2::LEFT_CENTER, name, fig::reg(11.0), crate::TEXT());
                if fig::icon_button(ui, g.icon(r), Glyph::Minus, "Remove preferred instance", false).clicked() {
                    remove = Some(k);
                }
            }
            if let Some(k) = remove {
                self.shapes[i].slot_pref.remove(k);
            }
            let free: Vec<(Option<u64>, &str)> = std::iter::once((None, "Add preferred instance…")).chain(comps.iter().filter(|c| !self.shapes[i].slot_pref.contains(&c.0)).map(|c| (Some(c.0), c.1.as_str()))).collect();
            let mut pick: Option<u64> = None;
            let r = fig::controls(ui, 4.0);
            if fig::dropdown(ui, g.both(r), Some(Glyph::Plus), &mut pick, &free, "Add preferred instance") {
                if let Some(p) = pick {
                    self.shapes[i].slot_pref.push(p);
                }
            }
        } else {
            let s = &self.shapes[i];
            let (lo, hi, n) = (s.slot_min, s.slot_max, self.slot_content(id).len() as u32);
            let limits = match (lo, hi) {
                (0, 0) => format!("{n} layers"),
                (a, 0) => format!("{n} layers, at least {a}"),
                (0, b) => format!("{n} of at most {b} layers"),
                (a, b) => format!("{n} layers, {a} to {b}"),
            };
            let r = fig::controls(ui, 4.0);
            let warn = n < lo;
            fig::text(ui, Pos2::new(g.left, r.center().y), egui::Align2::LEFT_CENTER, &limits, fig::reg(11.0), if warn { crate::BRAND } else { crate::TEXT_SECONDARY() });
            let pool: Vec<(u64, String)> = {
                let pref: Vec<(u64, String)> = comps.iter().filter(|c| self.shapes[i].slot_pref.contains(&c.0)).cloned().collect();
                if pref.is_empty() { comps.clone() } else { pref }
            };
            let items: Vec<(Option<u64>, &str)> = std::iter::once((None, "Add layer…")).chain(pool.iter().map(|c| (Some(c.0), c.1.as_str()))).collect();
            let mut pick: Option<u64> = None;
            let r = fig::controls(ui, 4.0);
            if self.slot_has_room(id) && fig::dropdown(ui, g.both(r), Some(Glyph::Plus), &mut pick, &items, "Add layer to slot") {
                if let Some(p) = pick {
                    self.slot_add(id, p);
                }
            }
        }
        let _ = Vec2::ZERO;
        fig::end(ui);
    }
}
