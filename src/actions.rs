// The Actions palette (Figma's Actions button / Ctrl+/): search a command by name and run it.
// Also here: Tidy up and the Eyedropper, which the palette offers.
use egui::{Align2, Pos2, Rect, Vec2};

use crate::geom::BoolOp;
use crate::glyphs::Glyph;
use crate::model::*;
use crate::{App, Mode, Tool};

type Run = fn(&mut App);

// name, shortcut, what it does. `needs` says what must be selected: 0 nothing, 1 a layer, 2 several layers
const COMMANDS: &[(&str, &str, u8, Run)] = &[
    ("Group selection", "Ctrl+G", 1, |a| a.group()),
    ("Ungroup", "Ctrl+Shift+G", 1, |a| a.ungroup()),
    ("Frame selection", "Ctrl+Alt+G", 1, |a| a.frame_selection()),
    ("Add auto layout", "Shift+A", 1, |a| a.add_auto_layout_selection()),
    ("Remove auto layout", "Ctrl+Alt+Shift+A", 1, |a| a.remove_auto_layout_selection()),
    ("Suggest auto layout", "", 1, |a| a.open_suggestions()),
    ("Tidy up", "Ctrl+Alt+T", 2, |a| a.tidy_up()),
    ("Create component", "Ctrl+Alt+K", 1, |a| a.make_component()),
    ("Push overrides to main component", "", 1, |a| a.push_overrides()),
    ("Restore main component", "", 1, |a| a.restore_main()),
    ("Multi-edit variants", "Q", 0, |a| a.toggle_multi_variants()),
    ("Detach instance", "Ctrl+Alt+B", 1, |a| a.detach_instances()),
    ("Reset all overrides", "", 1, |a| {
        for id in a.sel_roots() {
            a.reset_overrides(id);
        }
    }),
    ("Use as mask", "Ctrl+Alt+M", 1, |a| a.toggle_mask()),
    ("Flatten", "Ctrl+E", 1, |a| a.flatten()),
    ("Apply transforms to selection", "", 1, |a| a.apply_repeat()),
    ("Create brush from layer", "", 1, |a| a.create_brush_from_selection()),
    ("Create code layer", "", 0, |a| a.make_code_layer()),
    ("Outline text", "", 1, |a| a.text_to_vector()),
    ("Split vector", "", 1, |a| a.split_vector()),
    ("Shape builder", "Shift+B", 1, |a| a.set_vtool(Some(crate::vtools::VTool::Builder))),
    ("Cut tool", "X", 0, |a| a.set_vtool(Some(crate::vtools::VTool::Cut))),
    ("Eraser", "Shift+E", 0, |a| a.set_vtool(Some(crate::vtools::VTool::Eraser))),
    ("Paint tool", "", 1, |a| a.set_vtool(Some(crate::vtools::VTool::Paint))),
    ("Outline stroke", "Ctrl+Alt+O", 1, |a| a.outline_stroke()),
    ("Simplify vector", "", 1, |a| {
        let n = a.simplify_vector(2.0);
        a.status = format!("simplified: {n} points fewer");
    }),
    ("Union selection", "Ctrl+Alt+U", 2, |a| a.boolean(BoolOp::Union)),
    ("Subtract selection", "Ctrl+Alt+S", 2, |a| a.boolean(BoolOp::Subtract)),
    ("Intersect selection", "Ctrl+Alt+I", 2, |a| a.boolean(BoolOp::Intersect)),
    ("Exclude selection", "Ctrl+Alt+X", 2, |a| a.boolean(BoolOp::Exclude)),
    ("Flip horizontal", "Shift+H", 1, |a| a.flip(true)),
    ("Flip vertical", "Shift+V", 1, |a| a.flip(false)),
    ("Bring forward", "Ctrl+]", 1, |a| a.sel_roots().iter().rev().for_each(|&id| a.move_z(id, true))),
    ("Send backward", "Ctrl+[", 1, |a| a.sel_roots().iter().for_each(|&id| a.move_z(id, false))),
    ("Select all", "Ctrl+A", 0, |a| a.select_all()),
    ("Select inverse", "Ctrl+Shift+A", 1, |a| a.select_inverse()),
    ("Select matching layers", "Ctrl+Alt+A", 1, |a| a.select_matching()),
    ("Hide / show selection", "Ctrl+Shift+H", 1, |a| a.toggle_hidden_selection()),
    ("Lock / unlock selection", "Ctrl+Shift+L", 1, |a| a.toggle_locked_selection()),
    ("Copy properties", "Ctrl+Alt+C", 1, |a| a.copy_properties()),
    ("Paste properties", "Ctrl+Alt+V", 1, |a| a.paste_properties()),
    ("Duplicate", "Ctrl+D", 1, |a| a.duplicate()),
    ("Resize to fit", "", 1, |a| a.resize_to_fit()),
    ("Text on path", "", 2, |a| a.text_on_path()),
    ("Detach text from path", "", 1, |a| a.detach_text_path()),
    ("Quick insert", "Shift+I", 0, |a| a.open_quick()),
    ("Hide / show interface", "Ctrl+\\", 0, |a| a.hide_ui ^= true),
    ("Eyedropper", "I", 0, |a| a.start_eyedropper()),
    ("Zoom to fit", "Shift+1", 0, |a| a.zoom_fit(false)),
    ("Zoom to selection", "Shift+2", 0, |a| a.zoom_fit(true)),
    ("Zoom to 100%", "Shift+0", 0, |a| a.zoom_at_center(1.0)),
    ("Show / hide rulers", "Shift+R", 0, |a| a.rulers ^= true),
    ("Show / hide pixel grid", "Shift+'", 0, |a| a.pixel_grid ^= true),
    ("Show / hide outlines", "Ctrl+Shift+O", 0, |a| a.outlines ^= true),
    ("Show / hide layout guides", "Shift+G", 0, |a| a.layout_guides ^= true),
    ("Dev Mode", "Shift+D", 0, |a| a.set_mode(if a.mode == Mode::Dev { Mode::Design } else { Mode::Dev })),
    ("Import Figma file (.fig)…", "", 0, |a| {
        if let Some(p) = rfd::FileDialog::new().add_filter("Figma file", &["fig"]).pick_file() {
            a.import_fig(p);
        }
    }),
    ("Import Sketch file (.sketch)…", "", 0, |a| {
        if let Some(p) = rfd::FileDialog::new().add_filter("Sketch file", &["sketch"]).pick_file() {
            a.import_sketch(p);
        }
    }),
    ("Toggle REST API", "", 0, |a| {
        if let Some(ctx) = a.ctx.clone() {
            a.toggle_api(&ctx, 0);
        }
    }),
    ("Present", "", 0, |a| {
        if let Some(f) = a.single().map(|i| a.shapes[i].id).or_else(|| a.shapes.iter().find(|s| s.kind == Kind::Frame && s.parent.is_none()).map(|s| s.id)) {
            a.start_present(f);
        }
    }),
];

// what the palette offers that makes no sense from a script: file dialogs, tools that wait for the pointer, the app's own windows
const NOT_FOR_API: &[&str] = &[
    "Import Figma file (.fig)…", "Import Sketch file (.sketch)…", "Shape builder", "Cut tool", "Eraser", "Paint tool", "Eyedropper", "Quick insert", "Present", "Account…",
    "Toggle REST API", "Open plugins folder",
    // these write into the main component, which can be the person's design while the selected instance is in an edit copy
    "Push overrides to main component", "Restore main component",
];

// the actions a script can run (GET /actions): name, shortcut, what must be selected (0 nothing, 1 a layer, 2 several)
pub(crate) fn api_commands() -> Vec<serde_json::Value> {
    let mut v: Vec<_> = COMMANDS.iter().filter(|c| !NOT_FOR_API.contains(&c.0)).map(|c| serde_json::json!({ "name": c.0, "shortcut": c.1, "needs": c.2 })).collect();
    v.push(serde_json::json!({ "name": "Offset path", "needs": 1, "args": "value: px" }));
    v.push(serde_json::json!({ "name": "Rename", "needs": 1, "args": "pattern: # = number, * = old name" }));
    v.push(serde_json::json!({ "name": "Replace", "needs": 1, "args": "from, to: text in the selected layers" }));
    v
}

// what the named action needs selected; None for a name that is not a palette action
pub(crate) fn command_needs(name: &str) -> Option<u8> {
    COMMANDS.iter().find(|c| c.0.eq_ignore_ascii_case(name)).map(|c| c.2)
}

// "Offset path 8" / "Offset path -4": a command with a number typed after its name
fn offset_arg(query: &str) -> Option<f32> {
    let q = query.trim().to_lowercase();
    let rest = q.strip_prefix("offset path").or_else(|| q.strip_prefix("offset"))?;
    rest.trim().parse::<f32>().ok()
}

// "rename Button #": the selected layers take the name, `#` becomes 1, 2, 3 ... and `*` the old name
fn rename_arg(query: &str) -> Option<String> {
    let q = query.trim();
    let rest = q.get(..7).filter(|h| h.eq_ignore_ascii_case("rename ")).map(|_| &q[7..])?;
    Some(rest.trim().to_string()).filter(|r| !r.is_empty())
}

// "replace foo with bar": Find and Replace over the text layers of the selection (the whole page when nothing is selected)
fn replace_arg(query: &str) -> Option<(String, String)> {
    let q = query.trim();
    q.get(..8).filter(|h| h.eq_ignore_ascii_case("replace "))?;
    let (from, to) = q[8..].split_once(" with ")?;
    Some((from.trim().to_string(), to.trim().to_string())).filter(|(f, _)| !f.is_empty())
}

impl App {
    // bulk rename: the layers in the order they sit in the layers panel (top first)
    pub fn rename_selection(&mut self, pattern: &str) {
        let mut order: Vec<usize> = self.sel_roots().iter().filter_map(|&id| self.index_of(id)).collect();
        order.sort_by(|a, b| b.cmp(a));
        for (n, &i) in order.iter().enumerate() {
            let old = self.shapes[i].name.clone();
            self.shapes[i].name = pattern.replace('#', &(n + 1).to_string()).replace('*', &old);
        }
        self.status = format!("renamed {} layer{}", order.len(), if order.len() == 1 { "" } else { "s" });
    }

    // Find and Replace in text layers; a layer with mixed styles is skipped unless the length stays the same
    pub fn replace_text(&mut self, from: &str, to: &str) {
        let everywhere = self.sel.is_empty();
        let scope: Vec<u64> = self.shapes.iter().filter(|s| everywhere || self.sel.iter().any(|&r| r == s.id || self.is_inside(s.id, r))).map(|s| s.id).collect();
        let (mut layers, mut skipped) = (0, 0);
        for s in self.shapes.iter_mut().filter(|s| s.kind == Kind::Text && scope.contains(&s.id) && s.text.contains(from)) {
            if !s.runs.is_empty() && from.chars().count() != to.chars().count() {
                skipped += 1;
                continue;
            }
            s.text = s.text.replace(from, to);
            layers += 1;
        }
        let tail = if skipped > 0 { format!(", {skipped} with mixed styles skipped") } else { String::new() };
        self.status = format!("replaced in {layers} text layer{}{tail}", if layers == 1 { "" } else { "s" });
    }

    // runs a palette action by name for the REST API
    pub(crate) fn run_api_command(&mut self, name: &str) -> Result<(), String> {
        let Some(&(n, _, needs, run)) = COMMANDS.iter().find(|c| c.0.eq_ignore_ascii_case(name.trim())) else {
            return Err(format!("no action called {name:?}: GET /actions lists them"));
        };
        if NOT_FOR_API.contains(&n) {
            return Err(format!("{n} needs the person at the keyboard (a dialog or the pointer)"));
        }
        if !self.can_run(needs) {
            return Err(format!("{n} needs {} selected: send \"ids\"", if needs == 1 { "a layer" } else { "several layers" }));
        }
        run(self);
        Ok(())
    }

    pub fn open_actions(&mut self) {
        self.actions_open = true;
        self.actions_query.clear();
        self.actions_sel = 0;
    }

    fn can_run(&self, needs: u8) -> bool {
        match needs {
            0 => true,
            1 => !self.sel.is_empty(),
            _ => self.sel.len() > 1,
        }
    }

    pub fn actions_palette(&mut self, ctx: &egui::Context) {
        if !self.actions_open {
            return;
        }
        let q = self.actions_query.to_lowercase();
        let matches: Vec<usize> = COMMANDS.iter().enumerate().filter(|(_, c)| q.is_empty() || c.0.to_lowercase().contains(q.trim())).map(|(k, _)| k).take(9).collect();
        let offset = offset_arg(&q);
        let renaming = rename_arg(&q).filter(|_| !self.sel.is_empty());
        let replacing = replace_arg(&self.actions_query);
        let rows = matches.len() + usize::from(offset.is_some()) + usize::from(renaming.is_some()) + usize::from(replacing.is_some());
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.actions_open = false;
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
            self.actions_sel = (self.actions_sel + 1) % rows.max(1);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
            self.actions_sel = (self.actions_sel + rows.max(1) - 1) % rows.max(1);
        }
        let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let mut run: Option<Option<usize>> = None; // Some(None) = the offset command
        let width = 360.0;
        egui::Area::new(egui::Id::new("actions-palette")).anchor(Align2::CENTER_TOP, [0.0, 90.0]).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::new().fill(crate::BG_MENU()).stroke(egui::Stroke::new(1.0, crate::BG_PRESSED())).corner_radius(crate::R_LARGE).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.set_width(width);
                let field = Rect::from_min_size(ui.cursor().min, Vec2::new(width, 32.0));
                ui.allocate_rect(field, egui::Sense::hover());
                crate::fig::input_bg(ui, field, false);
                let before = self.actions_query.clone();
                let edit = crate::fig::text_edit(ui, field, &mut self.actions_query, "Actions search", "Search actions");
                if !self.actions_focused {
                    edit.request_focus();
                    self.actions_focused = true;
                }
                if self.actions_query != before {
                    self.actions_sel = 0;
                }
                ui.add_space(6.0);
                for (n, &k) in matches.iter().enumerate() {
                    let (name, key, needs, _) = COMMANDS[k];
                    let ok = self.can_run(needs);
                    // the keyboard-selected row is drawn behind its text (a placeholder shape filled in once the row is placed)
                    let slot = ui.painter().add(egui::Shape::Noop);
                    let sc = ui.scope(|ui| crate::glyphs::menu_item(ui, None, name, key, ok));
                    let row = sc.inner;
                    if n == self.actions_sel {
                        ui.painter().set(slot, egui::Shape::rect_filled(sc.response.rect, crate::R_SMALL as f32 + 1.0, crate::BG_SECONDARY()));
                    }
                    if row || (enter && n == self.actions_sel && ok) {
                        run = Some(Some(k));
                    }
                }
                if let Some(d) = offset {
                    let label = format!("Offset path by {d} px");
                    if crate::glyphs::menu_item(ui, Some(Glyph::Shrink), &label, "", !self.sel.is_empty()) || (enter && self.actions_sel == matches.len()) {
                        run = Some(None);
                    }
                }
                let mut at = matches.len() + usize::from(offset.is_some());
                if let Some(p) = &renaming {
                    if crate::glyphs::menu_item(ui, Some(Glyph::Pencil), &format!("Rename selected layers to \"{p}\""), "# = number, * = old name", true) || (enter && self.actions_sel == at) {
                        run = Some(Some(usize::MAX));
                    }
                    at += 1;
                }
                if let Some((f, t)) = &replacing {
                    if crate::glyphs::menu_item(ui, None, &format!("Replace \"{f}\" with \"{t}\" in text"), "", true) || (enter && self.actions_sel == at) {
                        run = Some(Some(usize::MAX - 1));
                    }
                }
                if rows == 0 {
                    ui.label(egui::RichText::new("No actions match").color(crate::TEXT_TERTIARY()).size(11.0));
                }
            });
        });
        match run {
            Some(Some(k)) => {
                self.actions_open = false;
                self.actions_focused = false;
                if k == usize::MAX {
                    if let Some(p) = renaming {
                        self.rename_selection(&p);
                    }
                } else if k == usize::MAX - 1 {
                    if let Some((f, t)) = replacing {
                        self.replace_text(&f, &t);
                    }
                } else {
                    (COMMANDS[k].3)(self);
                }
            }
            Some(None) => {
                self.actions_open = false;
                self.actions_focused = false;
                if let Some(d) = offset {
                    self.offset_selection(d);
                }
            }
            None => {}
        }
        if !self.actions_open {
            self.actions_focused = false;
        }
    }

    // ---- Tidy up (Ctrl+Alt+T): the selected layers fall into even rows and columns

    // Layers are grouped into rows by their vertical overlap; inside a row they keep their order and get one gap (the average of
    // the existing gaps, 20 px when there are none), rows get one gap too. The top-left corner of the selection stays put.
    pub fn tidy_up(&mut self) {
        let roots: Vec<usize> = self.sel_roots().iter().filter_map(|&id| self.index_of(id)).collect();
        if roots.len() < 2 {
            return;
        }
        let Some(bb) = self.sel_bounds() else { return };
        let mut items: Vec<(usize, Rect)> = roots.iter().map(|&i| (i, self.world_rect(i))).collect();
        items.sort_by(|a, b| a.1.min.y.total_cmp(&b.1.min.y));
        let mut rows: Vec<Vec<(usize, Rect)>> = vec![];
        for it in items {
            match rows.last_mut() {
                Some(row) if it.1.center().y <= row.iter().map(|r| r.1.max.y).fold(f32::MIN, f32::max) && it.1.center().y >= row.iter().map(|r| r.1.min.y).fold(f32::MAX, f32::min) => row.push(it),
                _ => rows.push(vec![it]),
            }
        }
        for row in &mut rows {
            row.sort_by(|a, b| a.1.min.x.total_cmp(&b.1.min.x));
        }
        let avg = |v: Vec<f32>| if v.is_empty() { 20.0 } else { (v.iter().sum::<f32>() / v.len() as f32).max(0.0).round() };
        let gx = avg(rows.iter().flat_map(|r| r.windows(2).map(|w| w[1].1.min.x - w[0].1.max.x)).collect());
        let row_boxes: Vec<(f32, f32)> = rows.iter().map(|r| (r.iter().map(|x| x.1.min.y).fold(f32::MAX, f32::min), r.iter().map(|x| x.1.max.y).fold(f32::MIN, f32::max))).collect();
        let gy = avg(row_boxes.windows(2).map(|w| w[1].0 - w[0].1).collect());
        let mut y = bb.min.y;
        for (row, (top, bottom)) in rows.iter().zip(row_boxes) {
            let mut x = bb.min.x;
            for &(i, r) in row {
                let to = Pos2::new(x, y + (r.min.y - top));
                self.shapes[i].pos += to - r.min;
                x += r.width() + gx;
            }
            y += bottom - top + gy;
        }
    }

    // ---- Eyedropper (I): click a layer to take its color

    pub fn start_eyedropper(&mut self) {
        self.set_tool(Tool::Select);
        self.eyedropper = true;
        self.status = "Eyedropper: click a layer to pick its color".into();
    }

    // the top visible layer under `w` and the first visible solid paint of its fill (or, without one, its stroke)
    pub fn color_at(&self, w: Pos2) -> Option<egui::Color32> {
        let id = self.hit_test(w)?;
        let s = &self.shapes[self.index_of(id)?];
        s.fills.iter().chain(s.strokes.iter()).filter(|p| p.visible).find_map(|p| match p.kind {
            PaintKind::Solid(c) => Some(c),
            _ => None,
        })
    }

    // applies a picked color: the selection's first fill takes it (a layer without a fill gets one); nothing selected: copied as hex
    pub fn apply_picked(&mut self, c: egui::Color32, ctx: &egui::Context) {
        if std::mem::take(&mut self.eyedrop_var) {
            self.add_color_variable(c);
            return;
        }
        let sel: Vec<u64> = self.sel.clone();
        let mut done = false;
        for id in sel {
            let Some(i) = self.index_of(id) else { continue };
            if self.shapes[i].kind.is_container() && self.shapes[i].kind != Kind::Frame {
                continue;
            }
            match self.shapes[i].fills.last_mut() {
                Some(p) => p.kind = PaintKind::Solid(c),
                None => self.shapes[i].fills.push(Paint::solid(c)),
            }
            done = true;
        }
        let hex = format!("{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
        if !done {
            ctx.copy_text(hex.clone());
        }
        self.status = if done { format!("Picked #{hex}") } else { format!("Copied #{hex}") };
    }
}
