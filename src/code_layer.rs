// Code layers: a frame that holds code instead of drawn children. The code is HTML (with CSS and script as it likes); the canvas shows
// it as written, an SVG export puts it in a sandboxed iframe inside a foreignObject (a browser runs it), and the React code of the
// layer renders it with `dangerouslySetInnerHTML`.
use egui::{Pos2, Vec2};

use crate::fig::{self, Grid};
use crate::glyphs::Glyph;
use crate::model::*;
use crate::App;

const STARTER: &str = "<div style=\"font: 14px sans-serif; padding: 16px\">\n  <button onclick=\"this.textContent = 'Clicked'\">Click me</button>\n</div>";

impl App {
    // Create code layer: the selected frame becomes one; with nothing selected a new frame is made in the middle of the view
    pub fn make_code_layer(&mut self) {
        let frames: Vec<u64> = self.sel_roots().into_iter().filter(|&id| self.index_of(id).is_some_and(|i| self.shapes[i].kind == Kind::Frame)).collect();
        if frames.is_empty() {
            let c = self.to_world(self.view.min, self.view.center());
            let parent = self.frame_at(c, &[]);
            let id = self.new_id();
            let size = Vec2::new(320.0, 200.0);
            let mut f = Shape::new(Kind::Frame, id, self.shapes.len() + 1, c - self.origin_of(parent) - size / 2.0, parent);
            f.size = size;
            f.name = "Code layer".into();
            f.code = Some(STARTER.into());
            self.shapes.push(f);
            self.sel = vec![id];
            return;
        }
        for id in frames {
            if let Some(i) = self.index_of(id) {
                self.shapes[i].code.get_or_insert_with(|| STARTER.into());
            }
        }
    }

    // the Code section of the Design panel for a frame: turn it into a code layer, edit the code
    pub(crate) fn code_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        if self.shapes[i].kind != Kind::Frame || self.shapes[i].section {
            return;
        }
        let id = self.shapes[i].id;
        let has = self.shapes[i].code.is_some();
        let (_, b) = fig::header(ui, "Code layer", !has, &[(if has { Glyph::Minus } else { Glyph::Plus }, if has { "Turn back into a frame" } else { "Make this frame a code layer" }, false)]);
        if b[0].clicked() {
            self.shapes[i].code = if has { None } else { Some(STARTER.into()) };
            return;
        }
        let Some(code) = self.shapes[i].code.as_mut() else { return };
        let row = fig::controls(ui, 4.0);
        let field = egui::Rect::from_min_size(Pos2::new(g.left, row.top()), Vec2::new(g.edge - g.left, 160.0));
        ui.allocate_rect(field, egui::Sense::hover());
        fig::input_bg(ui, field, false);
        fig::put(ui, field.shrink(4.0), egui::TextEdit::multiline(code).id(ui.id().with(("code-layer", id))).code_editor().frame(egui::Frame::NONE).font(egui::FontId::monospace(11.0)).desired_width(f32::INFINITY));
        fig::end(ui);
    }
}
