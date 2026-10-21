// Smart Selection: layers selected together that sit in a row (or a column) show a handle in each gap. Dragging a handle changes
// every gap at once; holding Ctrl and dragging one of the layers moves it along the row, the others make room and keep their gaps,
// so the order changes. Works on layers of one parent, like Tidy up.
use egui::{Painter, Pos2, Rect, Stroke, Vec2};

use crate::{App, Drag, LAYOUT_INK};

pub struct Row {
    pub vertical: bool,    // a column
    pub ids: Vec<u64>,     // in order along the row
    pub rects: Vec<Rect>,  // world rects, same order
}

impl Row {
    fn gaps(&self) -> Vec<f32> {
        (0..self.ids.len() - 1).map(|k| if self.vertical { self.rects[k + 1].min.y - self.rects[k].max.y } else { self.rects[k + 1].min.x - self.rects[k].max.x }).collect()
    }

    // the handle of gap `k`: the strip between two neighbours, as long as the layers overlap each other across the row
    fn handle(&self, k: usize) -> Rect {
        let (a, b) = (self.rects[k], self.rects[k + 1]);
        if self.vertical {
            let (x0, x1) = (a.min.x.max(b.min.x), a.max.x.min(b.max.x));
            Rect::from_min_max(Pos2::new(x0, a.max.y), Pos2::new(x1, b.min.y))
        } else {
            let (y0, y1) = (a.min.y.max(b.min.y), a.max.y.min(b.max.y));
            Rect::from_min_max(Pos2::new(a.max.x, y0), Pos2::new(b.min.x, y1))
        }
    }
}

impl App {
    // the selection as a row or column, when it is one
    pub fn smart_row(&self) -> Option<Row> {
        let roots: Vec<usize> = self.sel_roots().iter().filter_map(|&id| self.index_of(id)).filter(|&i| !self.shapes[i].locked && self.shapes[i].visible).collect();
        if roots.len() < 2 || roots.iter().any(|&i| self.shapes[i].parent != self.shapes[roots[0]].parent) || self.node_edit || self.mode == crate::Mode::Dev {
            return None;
        }
        let rects: Vec<Rect> = roots.iter().map(|&i| self.world_rect(i)).collect();
        for vertical in [false, true] {
            // the layers share a band across the row, and do not overlap along it
            let (lo, hi) = rects.iter().fold((f32::MIN, f32::MAX), |(lo, hi), r| if vertical { (lo.max(r.min.x), hi.min(r.max.x)) } else { (lo.max(r.min.y), hi.min(r.max.y)) });
            if lo >= hi {
                continue;
            }
            let mut order: Vec<usize> = (0..roots.len()).collect();
            order.sort_by(|&a, &b| if vertical { rects[a].min.y.total_cmp(&rects[b].min.y) } else { rects[a].min.x.total_cmp(&rects[b].min.x) });
            let ok = order.windows(2).all(|w| if vertical { rects[w[1]].min.y >= rects[w[0]].max.y - 0.5 } else { rects[w[1]].min.x >= rects[w[0]].max.x - 0.5 });
            if ok {
                return Some(Row { vertical, ids: order.iter().map(|&k| self.shapes[roots[k]].id).collect(), rects: order.iter().map(|&k| rects[k]).collect() });
            }
        }
        None
    }

    // lays the row out in the order of `ids` from `start` along it, `gap` apart; `skip` stays where it is
    fn smart_respace(&mut self, ids: &[u64], vertical: bool, start: f32, gap: f32, skip: Option<u64>) {
        let mut at = start;
        for &id in ids {
            let Some(i) = self.index_of(id) else { continue };
            let r = self.world_rect(i);
            let (cur, size) = if vertical { (r.min.y, r.height()) } else { (r.min.x, r.width()) };
            if Some(id) != skip {
                if vertical { self.shapes[i].pos.y += at - cur } else { self.shapes[i].pos.x += at - cur }
            }
            at += size + gap;
        }
    }

    pub(crate) fn smart_handle_at(&self, origin: Pos2, press: Pos2, ctrl: bool) -> Option<Drag> {
        let row = self.smart_row()?;
        let w = self.to_world(origin, press);
        let along = |r: Rect| if row.vertical { r.min.y } else { r.min.x };
        let gaps = row.gaps();
        let remember = |gap: f32| {
            *self.smart_ids.borrow_mut() = row.ids.clone();
            self.smart_anchor.set((along(row.rects[0]), gap));
        };
        for k in 0..row.ids.len() - 1 {
            let r = row.handle(k);
            let pill = if row.vertical { Rect::from_center_size(r.center(), Vec2::new(r.width().max(14.0 / self.zoom), r.height().max(8.0 / self.zoom))) } else { Rect::from_center_size(r.center(), Vec2::new(r.width().max(8.0 / self.zoom), r.height().max(14.0 / self.zoom))) };
            if pill.expand(3.0 / self.zoom).contains(w) {
                remember(gaps[k]);
                return Some(Drag::Smart { vertical: row.vertical, start: gaps[k], from: if row.vertical { w.y } else { w.x } });
            }
        }
        // Ctrl on one of the layers: reorder
        if ctrl {
            if let Some(m) = (0..row.ids.len()).find(|&m| row.rects[m].contains(w)) {
                remember(gaps.iter().sum::<f32>() / gaps.len() as f32);
                let grab = if row.vertical { w.y - row.rects[m].min.y } else { w.x - row.rects[m].min.x };
                return Some(Drag::SmartMove { vertical: row.vertical, id: row.ids[m], grab });
            }
        }
        None
    }

    // dragging a gap handle: every gap becomes the dragged one plus how far the pointer went
    pub(crate) fn smart_gap_drag(&mut self, vertical: bool, start: f32, from: f32, w: Pos2) {
        let ids = self.smart_ids.borrow().clone();
        let now = if vertical { w.y } else { w.x };
        let gap = (start + now - from).max(0.0).round();
        let (at, _) = self.smart_anchor.get();
        self.smart_respace(&ids, vertical, at, gap, None);
        self.status = format!("gap {gap}");
    }

    // dragging layer `id` along the row: it follows the pointer, and where its centre falls among the others decides the order
    pub(crate) fn smart_move_drag(&mut self, vertical: bool, id: u64, grab: f32, w: Pos2) {
        let ids = self.smart_ids.borrow().clone();
        let Some(me) = self.index_of(id) else { return };
        let (at, gap) = self.smart_anchor.get();
        let now = if vertical { w.y } else { w.x };
        let r = self.world_rect(me);
        let size = if vertical { r.height() } else { r.width() };
        let centre = now - grab + size / 2.0;
        let others: Vec<u64> = ids.iter().copied().filter(|&i| i != id).collect();
        let place = others
            .iter()
            .filter(|&&o| {
                let r = self.index_of(o).map(|i| self.world_rect(i)).unwrap_or(Rect::NOTHING);
                (if vertical { r.center().y } else { r.center().x }) < centre
            })
            .count();
        let mut order = others;
        order.insert(place, id);
        self.smart_respace(&order, vertical, at, gap, Some(id));
        let cur = if vertical { r.min.y } else { r.min.x };
        if vertical { self.shapes[me].pos.y += (now - grab) - cur } else { self.shapes[me].pos.x += (now - grab) - cur }
        *self.smart_ids.borrow_mut() = order;
    }

    // released: the row is laid out again in its new order
    pub(crate) fn smart_move_finish(&mut self, vertical: bool) {
        let ids = self.smart_ids.borrow().clone();
        let (at, gap) = self.smart_anchor.get();
        self.smart_respace(&ids, vertical, at, gap, None);
    }

    pub(crate) fn draw_smart_selection(&self, painter: &Painter, origin: Pos2, hover: Option<Pos2>) {
        let Some(row) = self.smart_row() else { return };
        let dragging = matches!(self.drag, Drag::Smart { .. });
        let gaps = row.gaps();
        for k in 0..row.ids.len() - 1 {
            let r = self.screen_rect(origin, row.handle(k));
            let pill = Rect::from_center_size(r.center(), if row.vertical { Vec2::new(r.width().max(14.0), r.height().max(6.0)) } else { Vec2::new(r.width().max(6.0), r.height().max(14.0)) });
            let hot = dragging || hover.is_some_and(|h| pill.expand(3.0).contains(h));
            painter.rect_filled(pill, 2.0, LAYOUT_INK.gamma_multiply(if hot { 0.9 } else { 0.35 }));
            if hot {
                painter.rect_stroke(pill, 2.0, Stroke::new(1.0, LAYOUT_INK), egui::StrokeKind::Outside);
                crate::fig::ptext(painter, pill.center() - Vec2::new(0.0, if row.vertical { 0.0 } else { 14.0 }), egui::Align2::CENTER_CENTER, format!("{}", gaps[k].round()), crate::fig::reg(10.0), crate::TEXT());
            }
        }
    }
}
