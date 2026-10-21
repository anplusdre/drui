// The files browser (File > Browse files...): Figma's home screen, kept on this computer. Files live in one folder (`drui files` in
// the user's home, or $DRUI_FILES): the files directly in it are Drafts, each folder is a project. Recents remembers the files
// opened or saved last. There are no teams, accounts or cloud storage.
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::{Align2, Pos2, Rect, Vec2};

use crate::fig;
use crate::glyphs::Glyph;
use crate::model::*;
use crate::{App, TEXT, TEXT_SECONDARY};

pub const RECENTS: &str = "@recents";
const MAX_RECENT: usize = 20;

#[derive(Default)]
pub struct FilesUi {
    pub open: bool,
    pub project: String, // "" Drafts, RECENTS, or a project's folder name
    pub query: String,
    pub name: String,    // the field for new files and projects, and the new name for Rename
    pub confirm: Option<PathBuf>, // the file whose Delete was pressed once
}

pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub modified: Option<SystemTime>,
}

pub fn root() -> PathBuf {
    let base = std::env::var_os("DRUI_FILES").map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join("drui files")
    });
    let _ = std::fs::create_dir_all(&base);
    base
}

// a name that is safe as a file or folder name
pub fn clean_name(name: &str) -> String {
    let n: String = name.trim().chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '-' } else { c }).collect();
    n.trim_matches('.').trim().to_string()
}

pub fn projects() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root()).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().into_string().ok()).filter(|n| !n.starts_with('.')).collect();
    v.sort_by_key(|n| n.to_lowercase());
    v
}

fn folder_of(project: &str) -> PathBuf {
    if project.is_empty() { root() } else { root().join(project) }
}

fn recent_file() -> PathBuf {
    root().join(".recent")
}

pub fn recents() -> Vec<PathBuf> {
    std::fs::read_to_string(recent_file()).unwrap_or_default().lines().map(PathBuf::from).filter(|p| p.is_file()).collect()
}

pub fn note_recent(path: &Path) {
    let Ok(path) = path.canonicalize() else { return };
    let mut list: Vec<PathBuf> = recents().into_iter().filter(|p| p.canonicalize().ok().as_ref() != Some(&path)).collect();
    list.insert(0, path);
    list.truncate(MAX_RECENT);
    let text: Vec<String> = list.iter().map(|p| p.display().to_string()).collect();
    let _ = std::fs::write(recent_file(), text.join("\n"));
}

fn row(path: PathBuf) -> Row {
    let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    Row { path, name, modified }
}

// the files of a project (newest first), or the recents; `query` keeps those whose name contains it
pub fn list(project: &str, query: &str) -> Vec<Row> {
    let mut rows: Vec<Row> = if project == RECENTS {
        recents().into_iter().map(row).collect()
    } else {
        let mut v: Vec<Row> = std::fs::read_dir(folder_of(project)).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json")).map(row).collect();
        v.sort_by(|a, b| b.modified.cmp(&a.modified));
        v
    };
    let q = query.trim().to_lowercase();
    rows.retain(|r| q.is_empty() || r.name.to_lowercase().contains(&q));
    rows
}

// "just now", "5 minutes ago", "3 days ago"...
pub fn ago(t: Option<SystemTime>) -> String {
    let Some(secs) = t.and_then(|t| SystemTime::now().duration_since(t).ok()).map(|d| d.as_secs()) else { return String::new() };
    let plural = |n: u64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => plural(secs / 60, "minute"),
        3600..=86399 => plural(secs / 3600, "hour"),
        86400..=604799 => plural(secs / 86400, "day"),
        604800..=2591999 => plural(secs / 604800, "week"),
        _ => plural(secs / 2592000, "month"),
    }
}

// a path in `dir` for `name` that is not taken: "name.json", "name 2.json"...
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let mut p = dir.join(format!("{name}.json"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{name} {n}.json"));
        n += 1;
    }
    p
}

impl App {
    // a new empty file in `project` (named), opened
    pub fn files_new_file(&mut self, project: &str, name: &str) -> Option<PathBuf> {
        let name = clean_name(name);
        let name = if name.is_empty() { "Untitled".to_string() } else { name };
        let path = free_path(&folder_of(project), &name);
        let doc = Doc { pages: vec![Page { name: "Page 1".into(), ..Page::default() }], ..Doc::default() };
        match serde_json::to_string_pretty(&doc).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.load_from(path.clone());
                note_recent(&path);
                Some(path)
            }
            Err(e) => {
                self.status = format!("could not create the file: {e}");
                None
            }
        }
    }

    pub fn files_new_project(&mut self, name: &str) -> Option<String> {
        let name = clean_name(name);
        if name.is_empty() || name.starts_with('@') {
            return None;
        }
        match std::fs::create_dir_all(root().join(&name)) {
            Ok(()) => Some(name),
            Err(e) => {
                self.status = format!("could not create the project: {e}");
                None
            }
        }
    }

    pub fn files_duplicate(&mut self, path: &Path) -> Option<PathBuf> {
        let dir = path.parent()?;
        let stem = path.file_stem()?.to_string_lossy().into_owned();
        let to = free_path(dir, &format!("{stem} copy"));
        std::fs::copy(path, &to).ok()?;
        Some(to)
    }

    pub fn files_rename(&mut self, path: &Path, name: &str) -> Option<PathBuf> {
        let name = clean_name(name);
        if name.is_empty() {
            return None;
        }
        let to = path.parent()?.join(format!("{name}.json"));
        if to.exists() {
            self.status = format!("there is already a file called {name}");
            return None;
        }
        std::fs::rename(path, &to).ok()?;
        let recent: Vec<PathBuf> = recents().into_iter().map(|p| if p == path.canonicalize().unwrap_or_default() { to.canonicalize().unwrap_or(to.clone()) } else { p }).collect();
        let _ = std::fs::write(recent_file(), recent.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n"));
        if self.file == path {
            self.file = to.clone();
        }
        Some(to)
    }

    pub fn files_delete(&mut self, path: &Path) -> bool {
        let ok = std::fs::remove_file(path).is_ok();
        let gone = path.canonicalize().ok();
        let _ = gone; // the file no longer exists: Recents drops it by itself (it lists only files that exist)
        ok
    }

    // the window (File > Browse files...)
    pub fn files_window(&mut self, ctx: &egui::Context) {
        if !self.files.open {
            return;
        }
        let screen = ctx.content_rect();
        let size = Vec2::new(720.0, 460.0);
        let at = Pos2::new(screen.center().x - size.x / 2.0, screen.min.y + 60.0);
        let side = 176.0;
        let projects = projects();
        let mut ui_state = std::mem::take(&mut self.files);
        let rows = list(&ui_state.project, &ui_state.query);
        let current = self.file.canonicalize().ok();
        // what the window asks for, done after drawing
        enum Do {
            Open(PathBuf),
            NewFile,
            NewProject,
            Duplicate(PathBuf),
            Rename(PathBuf),
            Delete(PathBuf),
        }
        let mut todo: Option<Do> = None;
        egui::Area::new(egui::Id::new("files-window")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            let card = Rect::from_min_size(at, size);
            ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().add(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: egui::Color32::from_black_alpha(110) }.as_shape(card, crate::R_LARGE));
            ui.painter().rect(card, crate::R_LARGE, crate::BG(), egui::Stroke::new(1.0, fig::RULE()), egui::StrokeKind::Inside);
            fig::ptext(ui.painter(), Pos2::new(card.left() + 16.0, card.top() + 20.0), Align2::LEFT_CENTER, "Files", fig::semi(11.0), TEXT());
            if fig::icon_button(ui, Rect::from_min_size(Pos2::new(card.right() - 32.0, card.top() + 8.0), Vec2::splat(fig::ROW)), Glyph::Close, "Close files", false).clicked() {
                ui_state.open = false;
            }
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(card.left(), card.top() + 40.0), Vec2::new(card.width(), 1.0)), 0.0, fig::RULE());
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(card.left() + side, card.top() + 41.0), Vec2::new(1.0, card.height() - 41.0)), 0.0, fig::RULE());

            // the places: Recents, Drafts and the projects
            let mut places: Vec<(String, String)> = vec![(RECENTS.into(), "Recents".into()), (String::new(), "Drafts".into())];
            places.extend(projects.iter().map(|p| (p.clone(), p.clone())));
            for (k, (key, label)) in places.iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(card.left() + 8.0, card.top() + 48.0 + 28.0 * k as f32), Vec2::new(side - 16.0, 24.0));
                if r.bottom() > card.bottom() - 48.0 {
                    break;
                }
                let on = ui_state.project == *key;
                let resp = ui.interact(r, ui.id().with(("place", k)), egui::Sense::click());
                if on {
                    ui.painter().rect_filled(r, fig::RADIUS, fig::SELECTED());
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, fig::RADIUS, fig::INPUT());
                }
                paint_place_icon(ui, Pos2::new(r.left() + 12.0, r.center().y), key);
                fig::ptext(&ui.painter().with_clip_rect(r), Pos2::new(r.left() + 26.0, r.center().y), Align2::LEFT_CENTER, label, fig::reg(11.0), if on { fig::SELECTED_INK() } else { TEXT() });
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
                if resp.clicked() {
                    ui_state.project = key.clone();
                    ui_state.confirm = None;
                }
            }

            // search at the top of the list
            let right = card.left() + side + 16.0;
            fig::text_input(ui, Rect::from_min_size(Pos2::new(right, card.top() + 52.0), Vec2::new(card.right() - right - 16.0, fig::ROW)), &mut ui_state.query, "Search files", "Search files");

            // the files
            let top = card.top() + 88.0;
            if rows.is_empty() {
                let msg = if ui_state.query.trim().is_empty() { if ui_state.project == RECENTS { "No recent files yet" } else { "No files here yet" } } else { "No file has that name" };
                fig::ptext(ui.painter(), Pos2::new(right, top + 16.0), Align2::LEFT_CENTER, msg, fig::reg(11.0), TEXT_SECONDARY());
            }
            for (k, r) in rows.iter().enumerate() {
                let y = top + 32.0 * k as f32;
                if y + 28.0 > card.bottom() - 52.0 {
                    break;
                }
                let line = Rect::from_min_size(Pos2::new(right - 8.0, y), Vec2::new(card.right() - right, 28.0));
                let here = current.as_ref().is_some_and(|c| r.path.canonicalize().ok().as_ref() == Some(c));
                let resp = ui.interact(line, ui.id().with(("file", k)), egui::Sense::click());
                if here {
                    ui.painter().rect_filled(line, fig::RADIUS, fig::SELECTED());
                } else if resp.hovered() {
                    ui.painter().rect_filled(line, fig::RADIUS, fig::INPUT());
                }
                let name_font = fig::reg(11.0);
                fig::ptext(&ui.painter().with_clip_rect(Rect::from_min_size(line.min, Vec2::new(250.0, 28.0))), Pos2::new(line.left() + 10.0, line.center().y), Align2::LEFT_CENTER, &r.name, name_font, if here { fig::SELECTED_INK() } else { TEXT() });
                fig::ptext(ui.painter(), Pos2::new(line.left() + 270.0, line.center().y), Align2::LEFT_CENTER, ago(r.modified), fig::reg(11.0), TEXT_SECONDARY());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &format!("Open {}", r.name)));
                if resp.clicked() {
                    todo = Some(Do::Open(r.path.clone()));
                }
                let mut x = line.right() - 4.0;
                let sure = ui_state.confirm.as_ref() == Some(&r.path);
                x -= if sure { 76.0 } else { 24.0 };
                let del = Rect::from_min_size(Pos2::new(x, line.top() + 2.0), Vec2::new(if sure { 72.0 } else { fig::ROW }, fig::ROW));
                if sure {
                    if fig::button(ui, del, "Delete it").clicked() {
                        todo = Some(Do::Delete(r.path.clone()));
                    }
                } else if fig::icon_button(ui, del, Glyph::Minus, "Delete the file", false).clicked() {
                    ui_state.confirm = Some(r.path.clone());
                }
                x -= 64.0;
                if fig::button(ui, Rect::from_min_size(Pos2::new(x, line.top() + 2.0), Vec2::new(60.0, fig::ROW)), "Rename").clicked() {
                    todo = Some(Do::Rename(r.path.clone()));
                }
                x -= 76.0;
                if fig::button(ui, Rect::from_min_size(Pos2::new(x, line.top() + 2.0), Vec2::new(72.0, fig::ROW)), "Duplicate").clicked() {
                    todo = Some(Do::Duplicate(r.path.clone()));
                }
            }

            // new file / new project / the new name for Rename, along the bottom
            let y = card.bottom() - 44.0;
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(card.left() + side + 1.0, y - 4.0), Vec2::new(card.width() - side - 1.0, 1.0)), 0.0, fig::RULE());
            fig::text_input(ui, Rect::from_min_size(Pos2::new(right, y + 4.0), Vec2::new(200.0, fig::ROW)), &mut ui_state.name, "Name", "Name");
            if fig::button(ui, Rect::from_min_size(Pos2::new(right + 208.0, y + 4.0), Vec2::new(76.0, fig::ROW)), "New file").clicked() {
                todo = Some(Do::NewFile);
            }
            if fig::button(ui, Rect::from_min_size(Pos2::new(right + 292.0, y + 4.0), Vec2::new(92.0, fig::ROW)), "New project").clicked() {
                todo = Some(Do::NewProject);
            }
        });
        match todo {
            Some(Do::Open(p)) => {
                self.load_from(p.clone());
                note_recent(&p);
                ui_state.open = false;
            }
            Some(Do::NewFile) => {
                let project = if ui_state.project == RECENTS { String::new() } else { ui_state.project.clone() };
                if self.files_new_file(&project, &ui_state.name).is_some() {
                    ui_state.name.clear();
                    ui_state.open = false;
                }
            }
            Some(Do::NewProject) => {
                if let Some(p) = self.files_new_project(&ui_state.name) {
                    ui_state.project = p;
                    ui_state.name.clear();
                }
            }
            Some(Do::Duplicate(p)) => {
                self.files_duplicate(&p);
            }
            Some(Do::Rename(p)) => {
                if self.files_rename(&p, &ui_state.name).is_some() {
                    ui_state.name.clear();
                }
            }
            Some(Do::Delete(p)) => {
                self.files_delete(&p);
                ui_state.confirm = None;
            }
            None => {}
        }
        self.files = ui_state;
    }
}

fn paint_place_icon(ui: &egui::Ui, at: Pos2, key: &str) {
    let g = if key == RECENTS { Glyph::Clock } else if key.is_empty() { Glyph::File } else { Glyph::Frame };
    crate::glyphs::paint_glyph_sized(ui.painter(), at, g, TEXT_SECONDARY(), 13.0);
}
