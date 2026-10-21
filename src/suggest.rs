// Suggest auto layout: for a frame that has none, the layouts its children already look like: a vertical or a horizontal stack (evenly
// spaced, sharing an edge or centre) or a grid (rows and columns of cells). Each gets a score; the window lists them best first and
// a click applies one (direction, gap, padding and alignment are read from the children, as Shift+A does).
use egui::{Align2, Pos2, Rect};

use crate::fig;
use crate::model::*;
use crate::{App, TEXT, TEXT_SECONDARY};

#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub label: String,
    pub vertical: bool,
    pub columns: u32, // > 0: a grid
    pub score: f32,
}

fn mean_std(v: &[f32]) -> (f32, f32) {
    let m = v.iter().sum::<f32>() / v.len().max(1) as f32;
    (m, (v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / v.len().max(1) as f32).sqrt())
}

// how many distinct positions the values fall into (values within `tol` are the same)
fn clusters(mut v: Vec<f32>, tol: f32) -> usize {
    v.sort_by(|a, b| a.total_cmp(b));
    let mut n = 0;
    let mut last = f32::NEG_INFINITY;
    for x in v {
        if x - last > tol {
            n += 1;
        }
        last = x;
    }
    n
}

// the layouts the boxes look like, best first
pub fn suggest(boxes: &[Rect]) -> Vec<Suggestion> {
    let n = boxes.len();
    if n < 2 {
        return vec![];
    }
    let mut out = vec![];
    for vertical in [true, false] {
        let mut b = boxes.to_vec();
        let (lo, hi): (fn(&Rect) -> f32, fn(&Rect) -> f32) = if vertical { (|r| r.min.y, |r| r.max.y) } else { (|r| r.min.x, |r| r.max.x) };
        b.sort_by(|x, y| lo(x).total_cmp(&lo(y)));
        let gaps: Vec<f32> = b.windows(2).map(|w| lo(&w[1]) - hi(&w[0])).collect();
        // children that overlap along the axis are not a stack
        if gaps.iter().any(|g| *g < -1.0) {
            continue;
        }
        let (gm, gs) = mean_std(&gaps);
        let uniform = if gaps.len() < 2 { 1.0 } else { 1.0 / (1.0 + gs / (gm.abs() + 4.0)) };
        // the cross axis: the start, the centre or the end edge, whichever the children agree on best
        let edges: [fn(&Rect) -> f32; 3] = if vertical { [|r| r.min.x, |r| r.center().x, |r| r.max.x] } else { [|r| r.min.y, |r| r.center().y, |r| r.max.y] };
        let aligned = edges.iter().map(|e| 1.0 / (1.0 + mean_std(&b.iter().map(e).collect::<Vec<_>>()).1 / 8.0)).fold(0.0, f32::max);
        let score = 0.5 * uniform + 0.5 * aligned;
        out.push(Suggestion { label: format!("{} stack, gap {}", if vertical { "Vertical" } else { "Horizontal" }, gm.round().max(0.0)), vertical, columns: 0, score });
    }
    // a grid: the children's left edges fall into a few columns, their tops into a few rows, and most of the cells are filled
    let cols = clusters(boxes.iter().map(|r| r.min.x).collect(), 3.0);
    let rows = clusters(boxes.iter().map(|r| r.min.y).collect(), 3.0);
    let overlap = (0..n).any(|i| (i + 1..n).any(|j| boxes[i].shrink(1.0).intersects(boxes[j].shrink(1.0))));
    if cols >= 2 && rows >= 2 && cols * rows <= n * 2 && n <= cols * rows && !overlap {
        let fill = n as f32 / (cols * rows) as f32;
        out.push(Suggestion { label: format!("Grid, {cols} columns"), vertical: false, columns: cols as u32, score: 0.35 + 0.65 * fill });
    }
    out.retain(|s| s.score > 0.35);
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

impl App {
    // the children of frame `fi` (visible, not lines)
    fn suggest_boxes(&self, fi: usize) -> Vec<Rect> {
        let id = self.shapes[fi].id;
        self.shapes.iter().filter(|s| s.parent == Some(id) && s.visible && s.kind != Kind::Line).map(|s| s.aabb()).collect()
    }

    // opens the suggestions for the selected frame without auto layout
    pub fn open_suggestions(&mut self) {
        let Some(fi) = self.single().filter(|&i| self.shapes[i].kind == Kind::Frame && self.shapes[i].layout.is_none()) else {
            self.status = "Select a frame without auto layout to get suggestions".into();
            return;
        };
        let list = suggest(&self.suggest_boxes(fi));
        if list.is_empty() {
            self.status = "No auto layout fits these layers yet: line them up or space them evenly".into();
            self.suggestions = None;
        } else {
            self.suggestions = Some((self.shapes[fi].id, list));
        }
    }

    // puts a suggestion on its frame
    pub fn apply_suggestion(&mut self, frame: u64, s: &Suggestion) {
        let Some(fi) = self.index_of(frame) else { return };
        self.add_auto_layout_as(fi, Some(s.vertical), s.columns);
        self.sel = vec![frame];
    }

    // the floating list of suggestions (the layouts are applied from here)
    pub fn suggestions_window(&mut self, ctx: &egui::Context) {
        let Some((frame, list)) = self.suggestions.clone() else { return };
        let mut apply: Option<Suggestion> = None;
        let mut close = false;
        let at = Pos2::new(self.view.center().x - 120.0, self.view.min.y + 56.0);
        egui::Area::new(egui::Id::new("suggest-auto-layout")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            let card = Rect::from_min_size(at, egui::Vec2::new(240.0, 48.0 + 32.0 * list.len() as f32));
            ui.allocate_exact_size(card.size(), egui::Sense::hover());
            ui.painter().rect(card, crate::R_LARGE, crate::BG(), egui::Stroke::new(1.0, fig::RULE()), egui::StrokeKind::Inside);
            fig::ptext(ui.painter(), Pos2::new(card.left() + 16.0, card.top() + 20.0), Align2::LEFT_CENTER, "Suggest auto layout", fig::semi(11.0), TEXT());
            if fig::icon_button(ui, Rect::from_min_size(Pos2::new(card.right() - 32.0, card.top() + 8.0), egui::Vec2::splat(fig::ROW)), crate::glyphs::Glyph::Close, "Close suggestions", false).clicked() {
                close = true;
            }
            for (k, s) in list.iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(card.left() + 8.0, card.top() + 40.0 + 32.0 * k as f32), egui::Vec2::new(card.width() - 16.0, 28.0));
                let resp = ui.interact(r, egui::Id::new(("suggestion", k)), egui::Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, fig::RADIUS, fig::INPUT());
                }
                fig::ptext(ui.painter(), Pos2::new(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, &s.label, fig::reg(11.0), TEXT());
                fig::ptext(ui.painter(), Pos2::new(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, format!("{:.0}%", s.score * 100.0), fig::reg(11.0), TEXT_SECONDARY());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &s.label));
                if resp.clicked() {
                    apply = Some(s.clone());
                }
            }
        });
        if let Some(s) = apply {
            self.apply_suggestion(frame, &s);
            self.suggestions = None;
        } else if close {
            self.suggestions = None;
        }
    }
}
