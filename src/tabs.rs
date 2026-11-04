// Open files as tabs along a header at the top of the window. The open document lives in the `App` fields as always; every other
// tab is parked: its `Doc` with its undo history and view, put back when its tab is picked.
use std::path::{Path, PathBuf};

use egui::{Align2, Pos2, Rect, Sense, Vec2};

use crate::fig::{self, Tip};
use crate::glyphs::{paint_glyph_sized, Glyph};
use crate::model::{Doc, Page};
use crate::{files, App, Mode, Snap, BG, BG_MENU, TEXT, TEXT_SECONDARY};

// Not measured against Figma's tab bar yet: a 40px bar, a Home button and square, full-height 12px tabs side by side, the open one on `BG`.
pub const TAB_BAR_H: f32 = 40.0;

pub struct Parked {
    doc: Doc,
    file: PathBuf,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    saved_undo: usize,
    zoom: f32,
    pan: Vec2,
}

fn blank() -> Doc {
    Doc { pages: vec![Page { name: "Canvas 1".into(), ..Page::default() }], ..Doc::default() }
}

impl App {
    fn tab_file(&self, k: usize) -> &Path {
        self.tabs[k].as_ref().map_or(&self.file, |p| &p.file)
    }

    pub(crate) fn tab_title(&self, k: usize) -> String {
        self.tab_file(k).file_stem().map_or("Untitled".into(), |n| n.to_string_lossy().into_owned())
    }

    // edits since the file was saved or opened
    pub(crate) fn tab_dirty(&self, k: usize) -> bool {
        match &self.tabs[k] {
            Some(p) => p.undo.len() != p.saved_undo,
            None => self.undo.len() != self.saved_undo || self.pending.is_some(),
        }
    }

    fn park(&mut self) -> Parked {
        let doc = self.doc();
        Parked { doc, file: self.file.clone(), undo: std::mem::take(&mut self.undo), redo: std::mem::take(&mut self.redo), saved_undo: self.saved_undo, zoom: self.zoom, pan: self.pan }
    }

    fn unpark(&mut self, p: Parked) {
        self.load_doc(p.doc, p.file);
        (self.undo, self.redo, self.saved_undo, self.zoom, self.pan) = (p.undo, p.redo, p.saved_undo, p.zoom, p.pan);
    }

    // Motion mode shows the page at the playhead, which must not be stored as the page
    fn tabs_blocked(&mut self) -> bool {
        let blocked = self.mode == Mode::Motion;
        if blocked {
            self.status = "leave Motion mode to change files".into();
        }
        blocked
    }

    pub(crate) fn switch_tab(&mut self, k: usize) {
        if k == self.open_tab || k >= self.tabs.len() || self.tabs_blocked() {
            return;
        }
        let Some(target) = self.tabs[k].take() else { return };
        let here = self.park();
        self.tabs[self.open_tab] = Some(here);
        self.open_tab = k;
        self.unpark(target);
    }

    // a new draft in a tab of its own
    pub(crate) fn new_tab(&mut self) {
        if self.tabs_blocked() {
            return;
        }
        let taken = |a: &App, name: &str| (0..a.tabs.len()).any(|k| a.tab_file(k).file_name().is_some_and(|f| f.to_string_lossy() == name));
        let name = (1..).map(|n| if n == 1 { "Untitled.json".to_string() } else { format!("Untitled {n}.json") }).find(|n| !taken(self, n)).unwrap_or_default();
        let here = self.park();
        self.tabs[self.open_tab] = Some(here);
        self.tabs.push(None);
        self.open_tab = self.tabs.len() - 1;
        self.load_doc(blank(), PathBuf::from(name));
        self.status.clear();
    }

    // closes tab `k`; a file with unsaved edits is saved first (a draft that was never saved stays open)
    pub(crate) fn close_tab(&mut self, k: usize) {
        if k >= self.tabs.len() || self.tabs_blocked() {
            return;
        }
        if self.tab_dirty(k) {
            self.switch_tab(k);
            if !self.file.exists() {
                self.status = format!("save {} before closing it", self.tab_title(self.open_tab));
                return;
            }
            self.save_to(self.file.clone());
        }
        let k = self.open_tab;
        if self.tabs.len() == 1 {
            self.load_doc(blank(), PathBuf::from("Untitled.json"));
            return;
        }
        let next = if k + 1 < self.tabs.len() { k + 1 } else { k - 1 };
        let Some(target) = self.tabs[next].take() else { return };
        self.tabs.remove(k);
        self.open_tab = if next > k { next - 1 } else { next };
        self.unpark(target);
    }

    // every way of opening a file lands here: it gets a tab of its own, unless it is open already (that tab is shown) or the open
    // document is an untouched blank one (which it replaces)
    pub(crate) fn open_doc(&mut self, doc: Doc, path: PathBuf) {
        if let Some(k) = (0..self.tabs.len()).find(|&k| k != self.open_tab && self.tab_file(k) == path) {
            self.switch_tab(k);
            return;
        }
        let blank_now = self.undo.is_empty() && self.pending.is_none() && self.shapes.is_empty() && self.pages.iter().all(|p| p.shapes.is_empty());
        if !blank_now && self.mode != Mode::Motion {
            let here = self.park();
            self.tabs[self.open_tab] = Some(here);
            self.tabs.push(None);
            self.open_tab = self.tabs.len() - 1;
        }
        self.load_doc(doc, path);
    }

    pub fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let bar = ui.max_rect();
        let (mut pick, mut close) = (None, None);
        // Home: a square button before the tabs, on `BG` like an open tab while the home page is shown
        let hr = Rect::from_min_size(bar.min, Vec2::splat(TAB_BAR_H));
        let home = ui.interact(hr, ui.id().with("home-tab"), Sense::click());
        if self.home {
            ui.painter().rect_filled(hr, 0.0, BG());
        } else if home.hovered() {
            ui.painter().rect_filled(hr, 0.0, fig::INPUT());
        }
        paint_glyph_sized(ui.painter(), hr.center(), Glyph::Home, if self.home { TEXT() } else { TEXT_SECONDARY() }, 16.0);
        let on = self.home;
        home.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, "Home"));
        if home.tip("Home").clicked() {
            self.home = true;
        }
        let mut x = hr.right();
        for k in 0..self.tabs.len() {
            let (title, active, dirty) = (self.tab_title(k), k == self.open_tab && !self.home, self.tab_dirty(k));
            let font = fig::reg(12.0);
            let w = (ui.painter().layout_no_wrap(title.clone(), font.clone(), TEXT()).size().x + 12.0 + 8.0 + 28.0).clamp(104.0, 220.0);
            let r = Rect::from_min_size(Pos2::new(x, bar.top()), Vec2::new(w, TAB_BAR_H));
            let resp = ui.interact(r, ui.id().with(("file-tab", k)), Sense::click());
            let x_rect = Rect::from_center_size(Pos2::new(r.right() - 18.0, r.center().y), Vec2::splat(24.0));
            let x_resp = ui.interact(x_rect, ui.id().with(("file-tab-close", k)), Sense::click());
            if active {
                ui.painter().rect_filled(r, 0.0, BG());
            } else if resp.hovered() {
                ui.painter().rect_filled(r, 0.0, fig::INPUT());
            }
            let ink = if active { TEXT() } else { TEXT_SECONDARY() };
            let clip = Rect::from_min_max(r.min, Pos2::new(x_rect.left() - 2.0, r.max.y));
            fig::ptext(&ui.painter().with_clip_rect(clip), Pos2::new(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, &title, font, ink);
            if x_resp.hovered() {
                ui.painter().rect_filled(x_rect, fig::RADIUS, fig::RULE());
            }
            if dirty && !resp.hovered() && !x_resp.hovered() {
                ui.painter().circle_filled(x_rect.center(), 3.5, TEXT_SECONDARY());
            } else if active || resp.hovered() || x_resp.hovered() {
                paint_glyph_sized(ui.painter(), x_rect.center(), Glyph::Close, ink, 14.0);
            }
            let label = if dirty { format!("{title} (unsaved)") } else { title.clone() };
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, format!("File {label}")));
            x_resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Close {title}")));
            if x_resp.clicked() || resp.middle_clicked() {
                close = Some(k);
            } else if resp.clicked() {
                pick = Some(k);
            }
            x += w;
        }
        if fig::icon_button(ui, Rect::from_min_size(Pos2::new(x + 4.0, bar.top() + 8.0), Vec2::splat(fig::ROW)), Glyph::Plus, "New file", false).clicked() {
            self.new_tab();
        }
        if let Some(k) = close {
            // closing a file from the home page leaves you there
            let home = self.home;
            self.close_tab(k);
            self.home = home;
        } else if let Some(k) = pick {
            self.home = false;
            self.switch_tab(k);
        }
    }

    // a recent file picked on the home page: its tab if it is open already, else a new one
    fn open_recent(&mut self, path: PathBuf) {
        let open = (0..self.tabs.len()).find(|&k| self.tab_file(k).canonicalize().ok().as_deref() == Some(path.as_path()));
        match open {
            Some(k) => {
                self.home = false;
                self.switch_tab(k);
            }
            None => self.load_from(path),
        }
    }

    // the home page: the recent files, newest first, in the middle of the canvas area
    pub(crate) fn home_page(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, BG());
        let w = 560.0_f32.min(full.width() - 64.0).max(240.0);
        let col = Rect::from_min_size(Pos2::new(full.center().x - w / 2.0, full.top() + 40.0), Vec2::new(w, (full.height() - 40.0).max(0.0)));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(col));
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let head = fig::row(&mut ui, 48.0);
        fig::ptext(ui.painter(), Pos2::new(head.left() + 8.0, head.center().y), Align2::LEFT_CENTER, "Recent", fig::semi(13.0), TEXT());
        let btn = |x: f32, w: f32| Rect::from_min_size(Pos2::new(head.right() - x, head.center().y - fig::ROW / 2.0), Vec2::new(w, fig::ROW));
        let open = fig::button(&mut ui, btn(8.0 + 64.0, 64.0), "Open…").clicked();
        let new = fig::button(&mut ui, btn(8.0 + 64.0 + 8.0 + 76.0, 76.0), "New file").clicked();
        fig::rule(&mut ui);
        let rows = files::list(files::RECENTS, "");
        if rows.is_empty() {
            let r = fig::row(&mut ui, 40.0);
            fig::ptext(ui.painter(), Pos2::new(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, "No recent files yet", fig::reg(11.0), crate::TEXT_TERTIARY());
        }
        let mut pick = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for (k, row) in rows.iter().enumerate() {
                let r = fig::row(ui, 32.0);
                let hl = r.shrink2(Vec2::new(0.0, 4.0));
                let resp = ui.interact(hl, ui.id().with(("recent", k)), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(hl, fig::RADIUS, fig::INPUT());
                }
                fig::ptext(&ui.painter().with_clip_rect(hl), Pos2::new(hl.left() + 8.0, r.center().y), Align2::LEFT_CENTER, &row.name, fig::reg(11.0), TEXT());
                fig::ptext(ui.painter(), Pos2::new(hl.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, files::ago(row.modified), fig::reg(11.0), TEXT_SECONDARY());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Open {}", row.name)));
                if resp.clicked() {
                    pick = Some(row.path.clone());
                }
            }
        });
        if open {
            if let Some(p) = rfd::FileDialog::new().add_filter("drui", &["json"]).pick_file() {
                self.load_from(p);
            }
        } else if new {
            self.new_tab();
        } else if let Some(p) = pick {
            self.open_recent(p);
        }
    }

    pub(crate) fn tab_bar_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("file-tabs").resizable(false).exact_size(TAB_BAR_H).frame(egui::Frame::NONE.fill(BG_MENU())).show(ui, |ui| self.tab_bar(ui));
    }
}
