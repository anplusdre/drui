// Quick insert (Shift+I): type a name, press Enter, and the shape, text or component is put at the centre of the view.
use egui::{Align2, Pos2, Rect, Vec2};

use crate::glyphs::Glyph;
use crate::model::*;
use crate::App;

// what the palette can insert: a primitive, or a main component (by id)
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Item {
    Shape(Kind),
    Section,
    Component(u64),
}

const BUILTIN: [(&str, Kind); 8] = [
    ("Frame", Kind::Frame),
    ("Rectangle", Kind::Rect),
    ("Ellipse", Kind::Ellipse),
    ("Line", Kind::Line),
    ("Polygon", Kind::Polygon),
    ("Star", Kind::Star),
    ("Text", Kind::Text),
    ("Section", Kind::Frame),
];

impl App {
    pub fn open_quick(&mut self) {
        self.quick_open = true;
        self.quick_query.clear();
        self.quick_sel = 0;
        self.quick_focused = false;
    }

    pub(crate) fn quick_items(&self) -> Vec<(String, Item)> {
        let q = self.quick_query.trim().to_lowercase();
        let mut out: Vec<(String, Item)> = BUILTIN.iter().map(|&(n, k)| (n.to_owned(), if n == "Section" { Item::Section } else { Item::Shape(k) })).collect();
        out.extend(self.shapes.iter().filter(|s| s.component && !s.component_set && s.kind == Kind::Frame).map(|s| (s.name.clone(), Item::Component(s.id))));
        out.retain(|(n, _)| q.is_empty() || n.to_lowercase().contains(&q));
        out.truncate(9);
        out
    }

    // puts `item` at the centre of the view, inside the frame there, and selects it
    pub(crate) fn quick_insert(&mut self, item: Item) {
        let c = self.to_world(self.view.min, self.view.center());
        let parent = self.frame_at(c, &[]);
        let o = self.origin_of(parent);
        let id = match item {
            Item::Component(main) => {
                let size = self.index_of(main).map_or(Vec2::splat(100.0), |i| self.shapes[i].size);
                self.instantiate(main, parent, (c - o - size / 2.0))
            }
            Item::Shape(_) | Item::Section => {
                let kind = if let Item::Shape(k) = item { k } else { Kind::Frame };
                let size = match (item, kind) {
                    (Item::Section, _) => Vec2::new(400.0, 300.0),
                    (_, Kind::Text) => Vec2::new(100.0, 24.0),
                    (_, Kind::Line) => Vec2::new(100.0, 0.0),
                    (_, Kind::Frame) => Vec2::new(200.0, 200.0),
                    _ => Vec2::splat(100.0),
                };
                let nid = self.new_id();
                let mut s = Shape::new(kind, nid, self.shapes.len() + 1, (c - o - size / 2.0), parent);
                s.size = size;
                if kind == Kind::Text {
                    s.text = "Text".into();
                }
                if item == Item::Section {
                    s.section = true;
                    s.name = "Section".into();
                }
                self.shapes.push(s);
                Some(nid)
            }
        };
        if let Some(id) = id {
            self.sel = vec![id];
        }
    }

    pub fn quick_palette(&mut self, ctx: &egui::Context) {
        if !self.quick_open {
            return;
        }
        let items = self.quick_items();
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.quick_open = false;
            return;
        }
        let n = items.len().max(1);
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
            self.quick_sel = (self.quick_sel + 1) % n;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
            self.quick_sel = (self.quick_sel + n - 1) % n;
        }
        let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let mut pick: Option<Item> = None;
        let width = 300.0;
        egui::Area::new(egui::Id::new("quick-insert")).anchor(Align2::CENTER_TOP, [0.0, 90.0]).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::new().fill(crate::BG_MENU()).stroke(egui::Stroke::new(1.0, crate::BG_PRESSED())).corner_radius(crate::R_LARGE).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.set_width(width);
                let field = Rect::from_min_size(ui.cursor().min, Vec2::new(width, 32.0));
                ui.allocate_rect(field, egui::Sense::hover());
                crate::fig::input_bg(ui, field, false);
                let before = self.quick_query.clone();
                let edit = crate::fig::text_edit(ui, field, &mut self.quick_query, "Quick insert search", "Insert…");
                if !self.quick_focused {
                    edit.request_focus();
                    self.quick_focused = true;
                }
                if self.quick_query != before {
                    self.quick_sel = 0;
                }
                ui.add_space(6.0);
                for (k, (name, item)) in items.iter().enumerate() {
                    let slot = ui.painter().add(egui::Shape::Noop);
                    let glyph = match item {
                        Item::Component(_) => Glyph::Component,
                        Item::Section => Glyph::Frame,
                        Item::Shape(Kind::Text) => Glyph::Type,
                        Item::Shape(Kind::Ellipse) => Glyph::Ellipse,
                        Item::Shape(Kind::Frame) => Glyph::Frame,
                        Item::Shape(_) => Glyph::Rect,
                    };
                    let sc = ui.scope(|ui| crate::glyphs::menu_item(ui, Some(glyph), name, "", true));
                    if k == self.quick_sel {
                        ui.painter().set(slot, egui::Shape::rect_filled(sc.response.rect, crate::R_SMALL as f32 + 1.0, crate::BG_SECONDARY()));
                    }
                    if sc.inner || (enter && k == self.quick_sel) {
                        pick = Some(*item);
                    }
                }
                if items.is_empty() {
                    ui.label(egui::RichText::new("Nothing matches").color(crate::TEXT_TERTIARY()).size(11.0));
                }
            });
        });
        if let Some(item) = pick {
            self.quick_open = false;
            self.quick_insert(item);
        }
        let _ = Pos2::ZERO;
    }
}
