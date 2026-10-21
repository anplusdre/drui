// Prototype connections drawn on the canvas, as in Figma: with the Prototype tab open and one layer selected, a circular node sits on
// its right edge; dragging it to another frame makes the layer navigate there on click (an existing link is pointed at the new frame).
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

use crate::{App, Drag, Mode, Tab, BRAND};
use crate::model::*;

const NODE_R: f32 = 5.0;
const GAP: f32 = 12.0; // the node sits this far right of the layer's edge

impl App {
    // the layer that has the node, and where the node is on screen
    pub(crate) fn connect_node(&self, origin: Pos2) -> Option<(u64, Pos2)> {
        if self.tab != Tab::Prototype || self.mode == Mode::Dev {
            return None;
        }
        let i = self.single()?;
        let s = &self.shapes[i];
        if s.slice || !s.visible {
            return None;
        }
        let r = self.screen_rect(origin, self.world_rect(i));
        Some((s.id, Pos2::new(r.right() + GAP, r.center().y)))
    }

    // a press on the node starts a connection drag
    pub(crate) fn connect_node_at(&self, origin: Pos2, press: Pos2) -> Option<Drag> {
        let (id, c) = self.connect_node(origin)?;
        ((press - c).length() <= NODE_R + 4.0).then_some(Drag::Connect(id))
    }

    // the screen (top-level frame) under `w` that `from` does not live in
    pub(crate) fn connect_target(&self, w: Pos2, from: u64) -> Option<u64> {
        let f = self.frame_at(w, &[from])?;
        let root = *self.chain(self.index_of(f)?).last()?;
        let root = &self.shapes[root];
        (root.kind == Kind::Frame && !self.is_inside(from, root.id)).then_some(root.id)
    }

    // the drag ended over a frame: the layer navigates to it
    pub(crate) fn finish_connect(&mut self, from: u64, w: Pos2) {
        let (Some(target), Some(i)) = (self.connect_target(w, from), self.index_of(from)) else { return };
        let s = &mut self.shapes[i];
        if s.link.is_none() {
            s.action = Action::Navigate;
            s.trigger = Trigger::Click;
        }
        s.link = Some(target);
        self.status = format!("{} now opens {}", self.shapes[i].name, self.index_of(target).map_or("", |t| self.shapes[t].name.as_str()));
    }

    pub(crate) fn draw_connect_node(&self, painter: &Painter, origin: Pos2, pointer: Option<Pos2>) {
        let Some((id, c)) = self.connect_node(origin) else { return };
        if let (Drag::Connect(from), Some(p)) = (self.drag, pointer) {
            painter.line_segment([c, p], Stroke::new(1.5, BRAND));
            painter.circle_filled(p, 3.0, BRAND);
            if let Some(t) = self.connect_target(self.to_world(origin, p), from).and_then(|t| self.index_of(t)) {
                painter.rect_stroke(self.screen_rect(origin, self.world_rect(t)).expand(2.0), 2.0, Stroke::new(2.0, BRAND), egui::StrokeKind::Outside);
            }
        }
        painter.circle_filled(c, NODE_R + 2.0, Color32::WHITE);
        painter.circle_filled(c, NODE_R, BRAND);
        let _ = (id, Rect::NOTHING, Vec2::ZERO);
    }
}
