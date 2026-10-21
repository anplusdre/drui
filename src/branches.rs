// Branches of a file: a copy of the document to work on apart from the main one, which can be merged back (a three-way merge of each
// page against the state the branch started from, see merge.rs). The file keeps every branch; the branch being worked on is the
// document on the screen.
use egui::{Align2, Pos2, Rect, Vec2};

use crate::fig;
use crate::model::*;
use crate::{App, TEXT, TEXT_SECONDARY};

impl App {
    // the document as one branch keeps it: the pages with the current page's layers and guides put in
    fn working_pages(&mut self) -> Vec<Page> {
        self.pages[self.cur].shapes = self.shapes.clone();
        self.pages[self.cur].guides = self.guides.clone();
        self.pages.clone()
    }

    // a file has "main" from the start, without it being listed in the file
    fn ensure_main(&mut self) {
        if self.branches.is_empty() {
            self.branches.push(Branch { name: "main".into(), pages: vec![], base: vec![], from: None, review: Review::default() });
            self.cur_branch = 0;
        }
    }

    // makes a branch of the document as it is and goes to it
    pub fn create_branch(&mut self, name: &str) {
        self.ensure_main();
        let name = if name.trim().is_empty() { format!("Branch {}", self.branches.len()) } else { name.trim().to_owned() };
        let pages = self.working_pages();
        let from = self.cur_branch;
        self.branches[from].pages = pages.clone();
        self.branches.push(Branch { name, pages: pages.clone(), base: pages, from: Some(from), review: Review::default() });
        let k = self.branches.len() - 1;
        self.switch_branch(k);
    }

    // saves the branch being worked on and opens another
    pub fn switch_branch(&mut self, k: usize) {
        if k >= self.branches.len() || k == self.cur_branch {
            return;
        }
        let pages = self.working_pages();
        let from = self.cur_branch;
        self.branches[from].pages = pages;
        let target = self.branches[k].pages.clone();
        if target.is_empty() {
            return;
        }
        self.cur_branch = k;
        self.cur = self.cur.min(target.len() - 1);
        self.shapes = target[self.cur].shapes.clone();
        self.guides = target[self.cur].guides.clone();
        self.pages = target;
        self.sel.clear();
        self.undo.clear();
        self.redo.clear();
        self.tex.clear();
        self.status = format!("branch {}", self.branches[k].name);
    }

    // ask `who` to review branch `k` before it is merged
    pub fn request_review(&mut self, k: usize, who: &str) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        if let Some(b) = self.branches.get_mut(k).filter(|b| b.from.is_some()) {
            b.review = Review { state: 1, reviewer: who.trim().to_owned(), by: String::new(), note: String::new(), secs };
            self.status = format!("review of {} requested{}", b.name, if who.trim().is_empty() { String::new() } else { format!(" from {}", who.trim()) });
        }
    }

    // the answer to a review request: approve it, or send it back with a note
    pub fn answer_review(&mut self, k: usize, approve: bool, note: &str) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let me = self.user.clone();
        if let Some(b) = self.branches.get_mut(k).filter(|b| b.review.state == 1) {
            b.review.state = if approve { 2 } else { 3 };
            b.review.by = me;
            b.review.note = note.trim().to_owned();
            b.review.secs = secs;
        }
    }

    // a review is not required to merge (as in Figma), but a branch with one still waiting or sent back says so when it is merged
    pub fn can_merge(&self, k: usize) -> bool {
        self.branches.get(k).is_some_and(|b| b.from.is_some())
    }

    // merges branch `k` into the one it came from; the notes of what clashed. The branch goes on from the merged state.
    pub fn merge_branch(&mut self, k: usize) -> Vec<String> {
        let Some(parent) = self.branches.get(k).and_then(|b| b.from) else { return vec![] };
        if !self.can_merge(k) {
            return vec![];
        }
        let unreviewed = matches!(self.branches[k].review.state, 1 | 3);
        let pages = self.working_pages();
        let cur = self.cur_branch;
        self.branches[cur].pages = pages;
        let (base, mine, theirs) = (self.branches[k].base.clone(), self.branches[parent].pages.clone(), self.branches[k].pages.clone());
        let mut notes = vec![];
        let mut merged_pages: Vec<Page> = vec![];
        for (n, t) in theirs.iter().enumerate() {
            match mine.get(n) {
                Some(m) => {
                    let b = base.get(n).map_or(&[][..], |p| &p.shapes[..]);
                    let (shapes, mut note) = crate::merge::merge_shapes(b, &m.shapes, &t.shapes);
                    notes.append(&mut note);
                    merged_pages.push(Page { shapes, ..m.clone() });
                }
                None => merged_pages.push(t.clone()), // a page the branch added
            }
        }
        // a page only the parent has stays
        merged_pages.extend(mine.iter().skip(theirs.len()).cloned());
        self.branches[parent].pages = merged_pages.clone();
        self.branches[k].base = merged_pages.clone();
        // the branch carries on from the merged document
        self.branches[k].pages = merged_pages.clone();
        if self.cur_branch == parent || self.cur_branch == k {
            self.pages = merged_pages;
            self.cur = self.cur.min(self.pages.len() - 1);
            self.shapes = self.pages[self.cur].shapes.clone();
            self.guides = self.pages[self.cur].guides.clone();
            self.prune_sel();
        }
        self.status = format!("merged {} into {}{}{}", self.branches[k].name, self.branches[parent].name, if notes.is_empty() { String::new() } else { format!(" ({} conflicts, yours kept)", notes.len()) }, if unreviewed { ", without an approved review" } else { "" });
        notes
    }

    pub fn delete_branch(&mut self, k: usize) {
        if k == 0 || k == self.cur_branch || k >= self.branches.len() {
            return;
        }
        self.branches.remove(k);
        for b in &mut self.branches {
            b.from = b.from.filter(|&f| f != k).map(|f| if f > k { f - 1 } else { f });
        }
        if self.cur_branch > k {
            self.cur_branch -= 1;
        }
    }

    // the Branches window (File menu)
    pub fn branches_window(&mut self, ctx: &egui::Context) {
        if !self.branches_open {
            return;
        }
        let screen = ctx.content_rect();
        let rows: f32 = if self.branches.is_empty() { 32.0 } else { self.branches.iter().map(|b| if b.from.is_some() { 60.0 } else { 32.0 }).sum() };
        let size = Vec2::new(420.0, 130.0 + rows);
        let at = Pos2::new(screen.center().x - size.x / 2.0, screen.min.y + 80.0);
        let (mut close, mut create, mut switch, mut merge, mut delete) = (false, false, None, None, None);
        let (mut ask, mut answer): (Option<usize>, Option<(usize, bool)>) = (None, None);
        let rid = egui::Id::new("branch-reviewer");
        let mut reviewer: String = ctx.data(|d| d.get_temp(rid)).unwrap_or_default();
        let nid = egui::Id::new("branch-name");
        let mut name: String = ctx.data(|d| d.get_temp(nid)).unwrap_or_default();
        egui::Area::new(egui::Id::new("branches-window")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            let card = Rect::from_min_size(at, size);
            ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().add(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: egui::Color32::from_black_alpha(110) }.as_shape(card, crate::R_LARGE));
            ui.painter().rect(card, crate::R_LARGE, crate::BG(), egui::Stroke::new(1.0, fig::RULE()), egui::StrokeKind::Inside);
            fig::ptext(ui.painter(), Pos2::new(card.left() + 16.0, card.top() + 20.0), Align2::LEFT_CENTER, "Branches", fig::semi(11.0), TEXT());
            if fig::icon_button(ui, Rect::from_min_size(Pos2::new(card.right() - 32.0, card.top() + 8.0), Vec2::splat(fig::ROW)), crate::glyphs::Glyph::Close, "Close branches", false).clicked() {
                close = true;
            }
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(card.left(), card.top() + 40.0), Vec2::new(card.width(), 1.0)), 0.0, fig::RULE());
            let list: Vec<(String, Option<usize>)> = if self.branches.is_empty() { vec![("main".into(), None)] } else { self.branches.iter().map(|b| (b.name.clone(), b.from)).collect() };
            let mut top = card.top() + 48.0;
            for (k, (bname, from)) in list.iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(card.left() + 8.0, top), Vec2::new(card.width() - 16.0, 28.0));
                top += if from.is_some() { 60.0 } else { 32.0 };
                let here = k == self.cur_branch;
                if here {
                    ui.painter().rect_filled(r, fig::RADIUS, fig::SELECTED());
                }
                fig::ptext(ui.painter(), Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, bname, fig::reg(11.0), if here { fig::SELECTED_INK() } else { TEXT() });
                if let Some(f) = from {
                    fig::ptext(ui.painter(), Pos2::new(r.left() + 110.0, r.center().y), Align2::LEFT_CENTER, format!("from {}", list[*f].0), fig::reg(11.0), TEXT_SECONDARY());
                }
                let mut x = r.right() - 4.0;
                if k > 0 && !here {
                    x -= 24.0;
                    if fig::icon_button(ui, Rect::from_min_size(Pos2::new(x, r.top() + 2.0), Vec2::splat(fig::ROW)), crate::glyphs::Glyph::Minus, "Delete the branch", false).clicked() {
                        delete = Some(k);
                    }
                }
                if from.is_some() {
                    x -= 56.0;
                    let ok = self.can_merge(k);
                    let merge_btn = fig::button(ui, Rect::from_min_size(Pos2::new(x, r.top() + 2.0), Vec2::new(52.0, fig::ROW)), "Merge");
                    if merge_btn.clicked() {
                        merge = Some(k);
                    }
                    let _ = ok;
                    // the review line: asked, waiting, approved or sent back
                    let rv = self.branches.get(k).map(|b| b.review.clone()).unwrap_or_default();
                    let line = Rect::from_min_size(Pos2::new(r.left(), r.bottom() + 2.0), Vec2::new(r.width(), 26.0));
                    let (text, ink) = match rv.state {
                        1 => (format!("Review requested{}", if rv.reviewer.is_empty() { String::new() } else { format!(" from {}", rv.reviewer) }), TEXT_SECONDARY()),
                        2 => (format!("Approved by {}{}", rv.by, if rv.note.is_empty() { String::new() } else { format!(": {}", rv.note) }), fig::SELECTED_INK()),
                        3 => (format!("{} asked for changes{}", rv.by, if rv.note.is_empty() { String::new() } else { format!(": {}", rv.note) }), crate::GUIDE),
                        _ => ("No review asked".to_owned(), TEXT_SECONDARY()),
                    };
                    fig::ptext(&ui.painter().with_clip_rect(Rect::from_min_max(line.min, Pos2::new(line.right() - 150.0, line.bottom()))), Pos2::new(line.left() + 10.0, line.center().y), Align2::LEFT_CENTER, text, fig::reg(11.0), ink);
                    if rv.state == 1 {
                        if fig::button(ui, Rect::from_min_size(Pos2::new(line.right() - 148.0, line.top()), Vec2::new(70.0, fig::ROW)), "Approve").clicked() {
                            answer = Some((k, true));
                        }
                        if fig::button(ui, Rect::from_min_size(Pos2::new(line.right() - 74.0, line.top()), Vec2::new(70.0, fig::ROW)), "Changes").clicked() {
                            answer = Some((k, false));
                        }
                    } else if fig::button(ui, Rect::from_min_size(Pos2::new(line.right() - 148.0, line.top()), Vec2::new(144.0, fig::ROW)), if rv.state == 0 { "Request review" } else { "Request again" }).clicked() {
                        ask = Some(k);
                    }
                }
                if !here {
                    x -= 60.0;
                    if fig::button(ui, Rect::from_min_size(Pos2::new(x, r.top() + 2.0), Vec2::new(56.0, fig::ROW)), "Open").clicked() {
                        switch = Some(k);
                    }
                }
            }
            let y = card.bottom() - 44.0;
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(card.left(), y - 4.0), Vec2::new(card.width(), 1.0)), 0.0, fig::RULE());
            fig::text_input(ui, Rect::from_min_size(Pos2::new(card.left() + 16.0, y + 4.0), Vec2::new(140.0, fig::ROW)), &mut name, "New branch name", "Branch name");
            if fig::button(ui, Rect::from_min_size(Pos2::new(card.left() + 164.0, y + 4.0), Vec2::new(100.0, fig::ROW)), "Create branch").clicked() {
                create = true;
            }
            fig::text_input(ui, Rect::from_min_size(Pos2::new(card.left() + 276.0, y + 4.0), Vec2::new(128.0, fig::ROW)), &mut reviewer, "Reviewer name", "Reviewer (for requests)");
        });
        ctx.data_mut(|d| d.insert_temp(nid, name.clone()));
        ctx.data_mut(|d| d.insert_temp(rid, reviewer.clone()));
        if let Some(k) = ask {
            self.request_review(k, &reviewer);
        }
        if let Some((k, approve)) = answer {
            self.answer_review(k, approve, "");
        }
        if create {
            self.create_branch(&name);
            ctx.data_mut(|d| d.insert_temp(nid, String::new()));
        } else if let Some(k) = switch {
            self.switch_branch(k);
        } else if let Some(k) = merge {
            self.merge_branch(k);
        } else if let Some(k) = delete {
            self.delete_branch(k);
        }
        if close {
            self.branches_open = false;
        }
    }
}
