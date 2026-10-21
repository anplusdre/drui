// Keyboard-only work: Enter with a drawing tool picked makes a shape where the pointer rests (or in the middle of the view), and
// Ctrl+Alt+arrows grow a selection box from there, selecting what it touches (Shift: bigger steps), until Escape.
use egui::{Pos2, Rect, Vec2};

use crate::model::*;
use crate::App;

impl App {
    // where keyboard-made things go: the pointer's last place on the canvas, else the middle of the view
    fn kbd_point(&self) -> Pos2 {
        self.net_cursor.unwrap_or_else(|| self.to_world(self.view.min, self.view.center()))
    }

    // Enter with a drawing tool: a shape of the usual size, centred on the point, selected; the tool goes back to Move
    pub fn create_by_keyboard(&mut self, kind: Kind) {
        let at = self.kbd_point();
        let size = match kind {
            Kind::Text => Vec2::new(100.0, 24.0),
            Kind::Line => Vec2::new(100.0, 0.0),
            Kind::Frame => Vec2::new(200.0, 200.0),
            _ => Vec2::splat(100.0),
        };
        self.add(kind, at - size / 2.0);
        if let Some(s) = self.shapes.last_mut() {
            s.size = size;
            if kind == Kind::Text {
                s.text = "Text".into();
            }
        }
        let id = self.sel.first().copied();
        self.set_tool(crate::Tool::Select);
        if kind == Kind::Text {
            self.editing_text = id; // typing replaces it
        }
        self.sel = id.into_iter().collect();
    }

    // Ctrl+Alt+arrow: the keyboard selection box grows one step towards `dir` (a box of nothing starts at the point)
    pub fn kbox_grow(&mut self, dir: Vec2, step: f32) {
        let start = self.kbd_point();
        let mut r = self.kbox.unwrap_or_else(|| Rect::from_min_max(start, start));
        if dir.x > 0.0 {
            r.max.x += step;
        } else if dir.x < 0.0 {
            r.min.x -= step;
        }
        if dir.y > 0.0 {
            r.max.y += step;
        } else if dir.y < 0.0 {
            r.min.y -= step;
        }
        self.kbox = Some(r);
        self.sel = self.marquee_hits(r);
        self.status = format!("{} layer{} in the box (Esc to leave it)", self.sel.len(), if self.sel.len() == 1 { "" } else { "s" });
    }
}
