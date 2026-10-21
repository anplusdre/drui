// Corner radius handles on the canvas: a dot inside each corner of the selected rectangle or frame. Dragging one sets the radius of
// all four corners; with Alt held, of that corner only. The handle sits as far in from its corner as the radius (never closer than
// 12 screen px, so a square corner can be picked up).
use egui::{Painter, Pos2, Stroke, Vec2};

use crate::model::*;
use crate::{App, Drag, BRAND};

const MIN_SCREEN: f32 = 12.0; // closest a handle sits to its corner, screen px
const SIZE: f32 = 80.0; // a shape smaller than this (screen px) shows no handles

impl App {
    // the selected layer that has corner handles, and the world position of each handle (top left, top right, bottom right, bottom left)
    pub(crate) fn radius_handles(&self) -> Option<(usize, [Pos2; 4])> {
        let i = self.single().filter(|&i| matches!(self.shapes[i].kind, Kind::Rect | Kind::Frame) && !self.shapes[i].section && !self.shapes[i].locked)?;
        if self.node_edit || self.mode == crate::Mode::Dev || self.scale_tool || self.vtool.is_some() {
            return None;
        }
        let s = &self.shapes[i];
        if s.size.x.min(s.size.y) * self.zoom < SIZE {
            return None;
        }
        let o = self.origin_of(s.parent);
        let r = s.rect();
        let max = r.width().min(r.height()) / 2.0;
        let corners = [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.right_bottom(), -1.0, -1.0), (r.left_bottom(), 1.0, -1.0)];
        let mut out = [Pos2::ZERO; 4];
        for (k, (c, sx, sy)) in corners.into_iter().enumerate() {
            let inset = s.radius[k].max(MIN_SCREEN / self.zoom).min(max);
            out[k] = s.xf(c + Vec2::new(sx * inset, sy * inset)) + o;
        }
        Some((i, out))
    }

    pub(crate) fn radius_handle_at(&self, origin: Pos2, press: Pos2) -> Option<Drag> {
        let (_, pts) = self.radius_handles()?;
        pts.iter().position(|&p| (self.to_screen(origin, p) - press).length() <= 6.0).map(|k| Drag::Radius { k })
    }

    // the pointer (world) drags handle `k`: the radius is how far it is from the corner, measured along both sides
    pub(crate) fn radius_drag(&mut self, k: usize, single: bool, world: Pos2) {
        let Some(i) = self.single() else { return };
        let o = self.origin_of(self.shapes[i].parent);
        let s = &self.shapes[i];
        let p = s.unxf(world - o);
        let r = s.rect();
        let (c, sx, sy) = [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.right_bottom(), -1.0, -1.0), (r.left_bottom(), 1.0, -1.0)][k];
        let max = r.width().min(r.height()) / 2.0;
        let v = (((p.x - c.x) * sx + (p.y - c.y) * sy) / 2.0).clamp(0.0, max).round();
        let s = &mut self.shapes[i];
        if single {
            s.radius[k] = v;
        } else {
            s.radius = [v; 4];
        }
        self.status = format!("corner radius {v}");
    }

    pub(crate) fn draw_radius_handles(&self, painter: &Painter, origin: Pos2) {
        let Some((_, pts)) = self.radius_handles() else { return };
        for p in pts {
            let c = self.to_screen(origin, p);
            painter.circle(c, 3.5, egui::Color32::WHITE, Stroke::new(1.0, BRAND));
        }
    }
}
