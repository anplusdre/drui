// On-canvas padding and gap handles of a selected auto-layout frame, as in Figma: the padding strips on the four inner edges and the
// strips between neighbours can be dragged to change the padding and the gap. Alt moves the opposite side too, Shift all four.
// ponytail: the look (colour, pill size, hover rule) is not measured against Figma yet; the behaviour is from memory of Figma.
use egui::{Align2, CursorIcon, Painter, Pos2, Rect, Stroke, Vec2};

use crate::model::*;
use crate::{App, Drag, Mode, Tool, LAYOUT_INK};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LayoutHandle {
    Pad(usize), // 0 left, 1 top, 2 right, 3 bottom
    Gap,
}

// what a layout-handle drag remembers: the frame, the pointer where it began, and the padding and gap at that moment
#[derive(Clone, Copy)]
pub struct LayoutDrag {
    pub h: LayoutHandle,
    pub frame: u64,
    pub from: Pos2,
    pub pad: [f32; 4],
    pub gap: f32,
}

const MIN_HIT: f32 = 8.0; // screen px: thin strips are still grabbable
const PILL: Vec2 = Vec2::new(16.0, 4.0);

impl App {
    // the one selected, unrotated auto-layout frame the handles belong to
    fn layout_frame(&self) -> Option<usize> {
        let i = self.single()?;
        let s = &self.shapes[i];
        (s.kind == Kind::Frame && s.layout.is_some() && s.rotation == 0.0 && !s.locked && self.chain(i).iter().all(|&k| self.shapes[k].visible)).then_some(i)
    }

    // the strips in world space with the handle each one drags
    fn layout_bands(&self, i: usize) -> Vec<(LayoutHandle, Rect)> {
        let (Some(lay), base) = (self.shapes[i].layout.as_ref(), self.world_rect(i).min) else { return vec![] };
        let sz = self.shapes[i].size;
        let p = lay.pad;
        let at = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(base + Vec2::new(x0, y0), base + Vec2::new(x1.max(x0), y1.max(y0)));
        let mut out = vec![
            (LayoutHandle::Pad(0), at(0.0, p[1], p[0], sz.y - p[3])),
            (LayoutHandle::Pad(1), at(p[0], 0.0, sz.x - p[2], p[1])),
            (LayoutHandle::Pad(2), at(sz.x - p[2], p[1], sz.x, sz.y - p[3])),
            (LayoutHandle::Pad(3), at(p[0], sz.y - p[3], sz.x - p[2], sz.y)),
        ];
        if lay.columns == 0 && !lay.wrap && lay.main != Align::SpaceBetween {
            let id = self.shapes[i].id;
            let mut kids: Vec<&Shape> = self.shapes.iter().filter(|s| s.parent == Some(id) && s.kind != Kind::Line && !s.abs).collect();
            let m = if lay.vertical { 1 } else { 0 };
            kids.sort_by(|a, b| (if m == 0 { a.pos.x } else { a.pos.y }).total_cmp(&(if m == 0 { b.pos.x } else { b.pos.y })));
            for w in kids.windows(2) {
                let (a, b) = (w[0], w[1]);
                out.push((
                    LayoutHandle::Gap,
                    if lay.vertical { at(p[0], a.pos.y + a.size.y, sz.x - p[2], b.pos.y) } else { at(a.pos.x + a.size.x, p[1], b.pos.x, sz.y - p[3]) },
                ));
            }
        }
        out
    }

    // the strip under a screen point; a strip thinner than MIN_HIT counts as MIN_HIT wide
    fn layout_band_at(&self, i: usize, origin: Pos2, p: Pos2) -> Option<(LayoutHandle, Rect)> {
        let w = self.to_world(origin, p);
        let grow = MIN_HIT / self.zoom;
        self.layout_bands(i).into_iter().find(|(_, r)| {
            let (dx, dy) = (((grow - r.width()) / 2.0).max(0.0), ((grow - r.height()) / 2.0).max(0.0));
            r.expand2(Vec2::new(dx, dy)).contains(w)
        })
    }

    pub(crate) fn layout_handle_at(&self, origin: Pos2, press: Pos2) -> Option<Drag> {
        let i = self.layout_frame().filter(|_| self.mode != Mode::Dev)?;
        let (h, _) = self.layout_band_at(i, origin, press)?;
        let lay = self.shapes[i].layout.as_ref()?;
        Some(Drag::Layout(LayoutDrag { h, frame: self.shapes[i].id, from: self.to_world(origin, press), pad: lay.pad, gap: lay.gap }))
    }

    pub(crate) fn layout_drag(&mut self, d: LayoutDrag, pointer: Pos2, shift: bool, alt: bool) {
        let Some(i) = self.index_of(d.frame) else { return };
        let delta = pointer - d.from;
        let Some(lay) = self.shapes[i].layout.clone() else { return };
        let vertical = lay.vertical;
        let s = &mut self.shapes[i];
        let l = s.layout.as_mut().unwrap();
        match d.h {
            LayoutHandle::Pad(side) => {
                // dragging into the frame grows the padding: right and bottom move against the axis
                let (along, sign) = (if side % 2 == 0 { delta.x } else { delta.y }, if side < 2 { 1.0 } else { -1.0 });
                let v = (d.pad[side] + sign * along).round().max(0.0);
                let sides: Vec<usize> = if shift { vec![0, 1, 2, 3] } else if alt { vec![side, (side + 2) % 4] } else { vec![side] };
                for k in sides {
                    l.pad[k] = v;
                }
                s.bind(BindProp::PadX, None);
                s.bind(BindProp::PadY, None);
            }
            LayoutHandle::Gap => {
                l.gap = (d.gap + if vertical { delta.y } else { delta.x }).round().max(0.0);
                s.bind(BindProp::Gap, None);
            }
        }
    }

    // a double-click on an edge handle of the selected layer, as in Figma: Hug on that axis for a layout frame or a text box,
    // Alt for Fill container on a layer inside an auto-layout frame. True when it did something.
    pub(crate) fn edge_double_click(&mut self, origin: Pos2, p: Pos2, alt: bool) -> bool {
        use crate::panels::{sizing_set, Sizing};
        let Some(i) = self.single() else { return false };
        let Some(&((hx, hy), _)) = self.single_handle_pts(i).iter().find(|((hx, hy), pt)| (*hx == 0) != (*hy == 0) && (self.to_screen(origin, *pt) - p).length() <= crate::HANDLE) else { return false };
        let horizontal = hx != 0;
        let _ = hy;
        let in_flow = self.shapes[i].parent.and_then(|pid| self.index_of(pid)).is_some_and(|pi| self.shapes[pi].layout.is_some() && self.shapes[pi].kind == Kind::Frame);
        let s = &mut self.shapes[i];
        let can_hug = s.kind == Kind::Text || (s.kind == Kind::Frame && s.layout.is_some());
        let z = if alt && in_flow {
            Sizing::Fill
        } else if can_hug && !alt {
            Sizing::Hug
        } else {
            return false;
        };
        sizing_set(s, horizontal, z);
        true
    }

    pub(crate) fn layout_cursor(&self, origin: Pos2, hover: Pos2) -> Option<CursorIcon> {
        let i = self.layout_frame().filter(|_| self.mode != Mode::Dev && self.tool == Tool::Select)?;
        let (h, _) = self.layout_band_at(i, origin, hover)?;
        let vertical = self.shapes[i].layout.as_ref()?.vertical;
        Some(match h {
            LayoutHandle::Pad(side) if side % 2 == 0 => CursorIcon::ResizeHorizontal,
            LayoutHandle::Pad(_) => CursorIcon::ResizeVertical,
            LayoutHandle::Gap if vertical => CursorIcon::ResizeVertical,
            LayoutHandle::Gap => CursorIcon::ResizeHorizontal,
        })
    }

    // the strips are shown while the selected frame is hovered (or Alt is held, or one is being dragged); the strip under the pointer is lit
    pub(crate) fn draw_layout_handles(&self, painter: &Painter, origin: Pos2, hover: Option<Pos2>, alt: bool) {
        let dragging = if let Drag::Layout(d) = self.drag { Some(d) } else { None };
        let Some(i) = self.layout_frame().filter(|_| self.mode != Mode::Dev) else { return };
        let over = hover.is_some_and(|h| self.world_rect(i).contains(self.to_world(origin, h)));
        if !(over || alt || dragging.is_some()) {
            return;
        }
        let lit = hover.and_then(|h| self.layout_band_at(i, origin, h)).map(|(_, r)| r);
        for (h, r) in self.layout_bands(i) {
            let sr = self.screen_rect(origin, r);
            let on = dragging.map_or(lit == Some(r), |d| d.h == h);
            let (fill, ink) = if on { (LAYOUT_INK.gamma_multiply(0.3), LAYOUT_INK) } else { (LAYOUT_INK.gamma_multiply(0.08), LAYOUT_INK.gamma_multiply(0.7)) };
            painter.rect_filled(sr, 0.0, fill);
            let (w, hgt) = if sr.width() >= sr.height() { (PILL.x, PILL.y) } else { (PILL.y, PILL.x) };
            painter.rect(Rect::from_center_size(sr.center(), Vec2::new(w, hgt)), 2.0, ink, Stroke::NONE, egui::StrokeKind::Inside);
        }
        if let (Some(d), Some(lay)) = (dragging, self.shapes[i].layout.as_ref()) {
            let v = match d.h {
                LayoutHandle::Pad(side) => lay.pad[side],
                LayoutHandle::Gap => lay.gap,
            };
            if let Some(h) = hover {
                crate::fig::ptext(painter, h + Vec2::new(14.0, -14.0), Align2::LEFT_CENTER, &format!("{}", v), crate::fig::reg(11.0), LAYOUT_INK);
            }
        }
    }
}

// what a crop-handle drag remembers: the layer, which paint, which side (0 left, 1 top, 2 right, 3 bottom), the crop and the pointer at the start
#[derive(Clone, Copy)]
pub struct CropDrag {
    pub shape: u64,
    pub paint: usize,
    pub side: usize,
    pub start: [f32; 4],
    pub from: Pos2,
}

impl App {
    // the selected layer's first visible image fill that is in Crop mode: (layer index, paint index)
    fn crop_target(&self) -> Option<(usize, usize)> {
        let i = self.single().filter(|&i| !self.shapes[i].locked && self.shapes[i].rotation == 0.0)?;
        let k = self.shapes[i].fills.iter().position(|p| p.visible && matches!(p.kind, PaintKind::Image(_)) && p.image.mode == ImageMode::Crop)?;
        Some((i, k))
    }

    // the four edge handles in screen space: a pill just inside the middle of each side
    pub(crate) fn crop_handles(&self, origin: Pos2, i: usize) -> [Pos2; 4] {
        let r = self.screen_rect(origin, self.world_rect(i));
        let inset = 10.0f32.min(r.width() / 4.0).min(r.height() / 4.0);
        [Pos2::new(r.left() + inset, r.center().y), Pos2::new(r.center().x, r.top() + inset), Pos2::new(r.right() - inset, r.center().y), Pos2::new(r.center().x, r.bottom() - inset)]
    }

    pub(crate) fn crop_handle_at(&self, origin: Pos2, press: Pos2) -> Option<Drag> {
        if self.mode == Mode::Dev {
            return None;
        }
        let (i, k) = self.crop_target()?;
        let side = self.crop_handles(origin, i).iter().position(|h| (*h - press).length() <= 9.0)?;
        Some(Drag::Crop(CropDrag { shape: self.shapes[i].id, paint: k, side, start: self.shapes[i].fills[k].image.crop, from: self.to_world(origin, press) }))
    }

    pub(crate) fn crop_drag(&mut self, d: CropDrag, pointer: Pos2) {
        let Some(i) = self.index_of(d.shape) else { return };
        let size = self.shapes[i].size.max(Vec2::splat(1.0));
        let delta = pointer - d.from;
        let Some(p) = self.shapes[i].fills.get_mut(d.paint) else { return };
        let mut c = d.start;
        let (w, h) = (c[2] - c[0], c[3] - c[1]);
        // dragging an edge inward moves that side of the visible part inward by the same share of the box
        match d.side {
            0 => c[0] = (c[0] + delta.x / size.x * w).clamp(0.0, c[2] - 0.02),
            1 => c[1] = (c[1] + delta.y / size.y * h).clamp(0.0, c[3] - 0.02),
            2 => c[2] = (c[2] + delta.x / size.x * w).clamp(c[0] + 0.02, 1.0),
            _ => c[3] = (c[3] + delta.y / size.y * h).clamp(c[1] + 0.02, 1.0),
        }
        p.image.crop = c;
    }

    pub(crate) fn draw_crop_handles(&self, painter: &Painter, origin: Pos2) {
        let Some((i, _)) = self.crop_target().filter(|_| self.mode != Mode::Dev) else { return };
        for (k, h) in self.crop_handles(origin, i).into_iter().enumerate() {
            let size = if k % 2 == 0 { Vec2::new(4.0, 16.0) } else { Vec2::new(16.0, 4.0) };
            painter.rect(Rect::from_center_size(h, size), 2.0, egui::Color32::WHITE, Stroke::new(1.0, crate::BRAND), egui::StrokeKind::Inside);
        }
    }
}
