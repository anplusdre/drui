// Dev Mode beyond the code panel: status (ready for dev, completed), annotations pinned to layers, measurements kept on the canvas,
// Compare changes against a saved version, and Code Connect (a component's own code snippet, filled in from an instance's
// properties).
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};

use crate::fig::{self, Grid};
use crate::glyphs::{paint_glyph_sized, Glyph};
use crate::model::*;
use crate::{App, TEXT, TEXT_SECONDARY};

const NOTE: Color32 = Color32::from_rgb(0xff, 0xc4, 0x3d); // annotation pins and tags

// what differs between a saved version and the page now
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Added,
    Removed,
    Changed(Vec<String>), // the names of the properties that differ
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerChange {
    pub id: u64,
    pub name: String,
    pub change: Change,
}

// the property names in which two layers differ (their saved JSON compared field by field)
fn differing_fields(a: &Shape, b: &Shape) -> Vec<String> {
    let (Ok(serde_json::Value::Object(x)), Ok(serde_json::Value::Object(y))) = (serde_json::to_value(a), serde_json::to_value(b)) else { return vec![] };
    let mut out: Vec<String> = x.keys().chain(y.keys()).filter(|k| x.get(*k) != y.get(*k)).cloned().collect();
    out.sort();
    out.dedup();
    out
}

impl App {
    // the layers added, removed or changed since version `v` of the file was saved
    pub fn changes_since(&self, v: usize) -> Vec<LayerChange> {
        let Some(version) = self.versions.get(v) else { return vec![] };
        let mut out = vec![];
        for s in &self.shapes {
            match version.shapes.iter().find(|o| o.id == s.id) {
                None => out.push(LayerChange { id: s.id, name: s.name.clone(), change: Change::Added }),
                Some(old) if old != s => {
                    let fields = differing_fields(old, s);
                    if !fields.is_empty() {
                        out.push(LayerChange { id: s.id, name: s.name.clone(), change: Change::Changed(fields) });
                    }
                }
                Some(_) => {}
            }
        }
        for old in version.shapes.iter().filter(|o| !self.shapes.iter().any(|s| s.id == o.id)) {
            out.push(LayerChange { id: old.id, name: old.name.clone(), change: Change::Removed });
        }
        out
    }

    // the Code Connect snippet of the component an instance (or a main component) comes from, with `{Property}` replaced by the
    // instance's values: component properties by name, variant properties as `{Size}`
    pub fn code_connect_text(&self, i: usize) -> Option<String> {
        let s = &self.shapes[i];
        let main = if s.instance { self.main_of(i)? } else { i };
        let owner = self.set_of(main).unwrap_or(main);
        let template = [owner, main].into_iter().map(|k| &self.shapes[k].code_connect).find(|t| !t.trim().is_empty())?.clone();
        let mut out = template;
        for def in self.props_of_root(i) {
            let val = s.cvals.iter().find(|(p, _)| *p == def.id).map_or(&def.default, |(_, v)| v);
            let text = match val {
                CompPropValue::Bool(b) => b.to_string(),
                CompPropValue::Text(t) => t.clone(),
                CompPropValue::Swap(c) => self.index_of(*c).map_or(String::new(), |c| self.shapes[c].name.clone()),
            };
            out = out.replace(&format!("{{{}}}", def.name), &text);
        }
        for (name, value) in &self.shapes[main].variant {
            out = out.replace(&format!("{{{name}}}"), value);
        }
        Some(out)
    }

    // pins the measurement between the selection and the layer under the pointer, or unpins it when it is pinned already
    pub fn pin_measurement(&mut self, hover: u64) {
        let Some(a) = self.single().map(|i| self.shapes[i].id) else { return };
        if a == hover {
            return;
        }
        let page = &mut self.pages[self.cur];
        match page.measures.iter().position(|&(x, y)| (x, y) == (a, hover) || (y, x) == (a, hover)) {
            Some(k) => {
                page.measures.remove(k);
            }
            None => page.measures.push((a, hover)),
        }
    }

    // the Dev panel's part for the selected layer, after its List or Code content: status, annotations
    pub fn dev_layer_sections(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        let id = self.shapes[i].id;
        fig::header(ui, "Status", false, &[]);
        let r = fig::controls(ui, 2.0);
        let (ready, done) = (self.shapes[i].ready, self.shapes[i].dev_done);
        if fig::button(ui, g.col1(r), if ready { "✓ Ready for dev" } else { "Ready for dev" }).clicked() {
            self.shapes[i].ready = !ready;
            if self.shapes[i].ready {
                self.shapes[i].dev_done = false;
            }
        }
        if fig::button(ui, g.col2(r), if done { "✓ Completed" } else { "Completed" }).clicked() {
            self.shapes[i].dev_done = !done;
            if self.shapes[i].dev_done {
                self.shapes[i].ready = false;
            }
        }
        fig::end(ui);
        // annotations on this layer
        let (_, b) = fig::header(ui, "Annotations", false, &[(Glyph::Plus, "Add an annotation", false)]);
        if b[0].clicked() {
            let nid = self.new_id();
            self.pages[self.cur].annotations.push(Annotation { id: nid, target: id, text: String::new(), label: String::new() });
        }
        let mut remove = None;
        let page = self.cur;
        let mut any = false;
        for a in self.pages[page].annotations.iter_mut().filter(|a| a.target == id) {
            any = true;
            let r = fig::controls(ui, 4.0);
            let f = Rect::from_min_max(Pos2::new(g.left, r.top()), Pos2::new(g.right - 28.0, r.bottom()));
            fig::text_input(ui, f, &mut a.text, "Annotation", "Note for the developer");
            if fig::icon_button(ui, g.icon(r), Glyph::Minus, "Remove the annotation", false).clicked() {
                remove = Some(a.id);
            }
        }
        if !any {
            fig::note(ui, Glyph::Sparkles, "Press + to leave a note on this layer for the developer.");
            fig::row(ui, 8.0);
        } else {
            fig::row(ui, 8.0);
        }
        if let Some(rid) = remove {
            self.pages[page].annotations.retain(|a| a.id != rid);
        }
        fig::rule(ui);
    }

    // the Code Connect part of the Code tab: the snippet of the component (edited on the main component), filled in for an instance
    pub fn dev_code_connect(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        let is_main = self.shapes[i].component;
        if !is_main && !self.shapes[i].instance {
            return;
        }
        fig::header(ui, "Code Connect", false, &[]);
        if is_main {
            let r = fig::controls(ui, 2.0);
            let f = g.both(r);
            fig::text_input(ui, f, &mut self.shapes[i].code_connect, "Code Connect snippet", "<Button label=\"{Label}\" size=\"{Size}\" />");
            fig::note(ui, Glyph::Code, "The snippet developers see for every instance. {Property} is replaced by the instance's value of that property.");
        } else if let Some(text) = self.code_connect_text(i) {
            fig::code_block(ui, &text, "Code Connect");
        } else {
            fig::note(ui, Glyph::Code, "This component has no Code Connect snippet yet. Select its main component to write one.");
        }
        fig::end(ui);
    }

    // nothing selected in Dev Mode: what is ready for dev, the notes and the pinned measurements, and Compare changes
    pub fn dev_overview(&mut self, ui: &mut egui::Ui, g: &Grid) {
        let page = self.cur;
        // ready for dev and completed
        let (ready, done): (Vec<(u64, String)>, Vec<(u64, String)>) = {
            let r: Vec<(u64, String)> = self.shapes.iter().filter(|s| s.ready).map(|s| (s.id, s.name.clone())).collect();
            let d: Vec<(u64, String)> = self.shapes.iter().filter(|s| s.dev_done).map(|s| (s.id, s.name.clone())).collect();
            (r, d)
        };
        fig::header(ui, "Ready for dev", ready.is_empty(), &[]);
        for (id, name) in &ready {
            self.dev_list_row(ui, g, *id, name, Glyph::Code);
        }
        if ready.is_empty() {
            fig::note(ui, Glyph::Code, "Layers marked Ready for dev show up here.");
        }
        if !done.is_empty() {
            fig::row(ui, 4.0);
            fig::header(ui, "Completed", false, &[]);
            for (id, name) in &done {
                self.dev_list_row(ui, g, *id, name, Glyph::CircleCheck);
            }
        }
        fig::end(ui);
        // annotations and measurements of the page
        let notes: Vec<(u64, u64, String)> = self.pages[page].annotations.iter().map(|a| (a.id, a.target, a.text.clone())).collect();
        fig::header(ui, "Annotations", notes.is_empty(), &[]);
        for (_, target, text) in &notes {
            let name = self.index_of(*target).map_or("(removed layer)".to_owned(), |i| self.shapes[i].name.clone());
            self.dev_list_row(ui, g, *target, &format!("{name}: {text}"), Glyph::Sparkles);
        }
        fig::end(ui);
        let n = self.pages[page].measures.len();
        let (_, b) = fig::header(ui, "Measurements", n == 0, if n > 0 { &[(Glyph::Delete, "Clear the pinned measurements", false)] } else { &[] });
        if n == 0 {
            fig::note(ui, Glyph::Crosshair, "Select a layer, then Alt+click another to keep the distance between them on the canvas.");
        } else {
            fig::note(ui, Glyph::Crosshair, &format!("{n} pinned. Alt+click the same layer again to unpin."));
        }
        if b.first().is_some_and(|b| b.clicked()) {
            self.pages[page].measures.clear();
        }
        fig::end(ui);
        // compare with a saved version
        fig::header(ui, "Compare changes", self.versions.is_empty(), &[]);
        if self.versions.is_empty() {
            fig::note(ui, Glyph::Clock, "Save a version in File > Version history to compare the design with it.");
        } else {
            let cid = egui::Id::new("dev-compare");
            let mut which: usize = ui.data(|d| d.get_temp(cid)).unwrap_or(self.versions.len() - 1).min(self.versions.len() - 1);
            let r = fig::controls(ui, 4.0);
            let items: Vec<(usize, String)> = self.versions.iter().enumerate().map(|(k, v)| (k, v.name.clone())).collect();
            let list: Vec<(usize, &str)> = items.iter().map(|(k, n)| (*k, n.as_str())).collect();
            fig::dropdown(ui, g.both(r), Some(Glyph::Clock), &mut which, &list, "Compare with version");
            ui.data_mut(|d| d.insert_temp(cid, which));
            let changes = self.changes_since(which);
            if changes.is_empty() {
                fig::note(ui, Glyph::CircleCheck, "No changes since that version.");
            }
            for c in &changes {
                let (label, glyph) = match &c.change {
                    Change::Added => (format!("{} (added)", c.name), Glyph::Plus),
                    Change::Removed => (format!("{} (removed)", c.name), Glyph::Minus),
                    Change::Changed(f) => (format!("{}: {}", c.name, f.join(", ")), Glyph::Sliders),
                };
                self.dev_list_row(ui, g, c.id, &label, glyph);
            }
        }
        fig::end(ui);
    }

    // a 28px row that selects its layer
    fn dev_list_row(&mut self, ui: &mut egui::Ui, g: &Grid, id: u64, label: &str, glyph: Glyph) {
        let r = fig::row(ui, 28.0);
        let hl = Rect::from_min_max(Pos2::new(g.left - 8.0, r.top()), Pos2::new(g.right + 8.0, r.bottom()));
        let resp = ui.interact(hl, ui.id().with(("dev-row", id, label)), egui::Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(hl, fig::RADIUS, fig::INPUT());
        }
        paint_glyph_sized(ui.painter(), Pos2::new(g.left + 6.0, r.center().y), glyph, TEXT_SECONDARY(), 13.0);
        fig::ptext(&ui.painter().with_clip_rect(Rect::from_min_max(hl.min, Pos2::new(hl.right() - 4.0, hl.bottom()))), Pos2::new(g.left + 22.0, r.center().y), Align2::LEFT_CENTER, label, fig::reg(11.0), TEXT());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
        if resp.clicked() && self.index_of(id).is_some() {
            self.sel = vec![id];
        }
    }

    // annotation pins on the canvas (Dev Mode): a yellow tag at the layer's top-right corner with the number, the text beside it
    pub fn draw_annotations(&self, painter: &Painter, origin: Pos2) {
        let list = &self.pages[self.cur].annotations;
        for (n, a) in list.iter().enumerate() {
            let Some(i) = self.index_of(a.target) else { continue };
            if !self.chain(i).iter().all(|&k| self.shapes[k].visible) {
                continue;
            }
            let r = self.screen_rect(origin, self.world_rect(i));
            let pin = Pos2::new(r.right(), r.top());
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, NOTE.gamma_multiply(0.6)), egui::StrokeKind::Outside);
            painter.circle_filled(pin, 8.0, NOTE);
            fig::ptext(painter, pin, Align2::CENTER_CENTER, format!("{}", n + 1), FontId::proportional(10.0), Color32::BLACK);
            if !a.text.is_empty() {
                let galley = painter.layout_no_wrap(a.text.clone(), FontId::proportional(11.0), Color32::BLACK);
                let tag = Rect::from_min_size(pin + Vec2::new(12.0, -9.0), galley.size() + Vec2::new(10.0, 6.0));
                painter.rect_filled(tag, 3.0, NOTE);
                painter.galley(tag.min + Vec2::new(5.0, 3.0), galley, Color32::BLACK);
            }
        }
    }

    // measurements that were pinned with Alt+click: the red distance lines between the two layers, kept on the canvas
    pub fn draw_pinned_measurements(&self, painter: &Painter, origin: Pos2) {
        for &(a, b) in &self.pages[self.cur].measures {
            let (Some(ia), Some(ib)) = (self.index_of(a), self.index_of(b)) else { continue };
            for (p, q, d) in crate::inspect::redlines(self.world_rect(ia), self.world_rect(ib)) {
                let (sp, sq) = (self.to_screen(origin, p), self.to_screen(origin, q));
                painter.line_segment([sp, sq], Stroke::new(1.0, crate::GUIDE));
                let galley = painter.layout_no_wrap(format!("{d:.0}"), FontId::proportional(10.5), Color32::WHITE);
                let r = Rect::from_center_size(sp + (sq - sp) / 2.0, galley.size() + Vec2::new(8.0, 4.0));
                painter.rect_filled(r, 3.0, crate::GUIDE);
                fig::ptext(painter, r.center(), Align2::CENTER_CENTER, format!("{d:.0}"), FontId::proportional(10.5), Color32::WHITE);
            }
        }
    }
}
