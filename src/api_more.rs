// More of the REST API (api.rs has the layers and the rest): styles, version history, branches and libraries.
//
// The same rule as everywhere in the API: nothing here may change or destroy what the person made. Styles can only be added (a changed
// style restyles every layer that follows it), a version is restored or a branch merged only when the person has said so, and a branch
// is deleted only while it has no changes.
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::App;
use crate::api::{Answer, Req, Resp};
use crate::model::*;

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl App {
    fn view_json(&self) -> Value {
        json!({
            "mode": crate::Mode::ALL.iter().find(|m| m.0 == self.mode).map(|m| m.1),
            "zoom": self.zoom,
            "left_tab": match self.left_tab { crate::LeftTab::Layers => "layers", crate::LeftTab::Assets => "assets", crate::LeftTab::History => "history", crate::LeftTab::Comments => "comments" },
            "left_min": self.left_min, "hide_ui": self.hide_ui, "light_ui": self.light_ui,
            "snap": self.snap, "snap_pixels": self.snap_pixels, "pixel_grid": self.pixel_grid, "rulers": self.rulers, "layout_guides": self.layout_guides,
            "prop_labels": self.prop_labels, "labels": self.labels, "spell": self.spell, "outlines": self.outlines,
            "outline_hidden": self.outline_hidden, "outline_bounds": self.outline_bounds,
            "pixel_preview": self.pixel_preview, "color_profile": if self.color_profile == 1 { "p3" } else { "srgb" },
            "nudge_small": self.nudge_small, "nudge_big": self.nudge_big, "handle_mirror": self.handle_mirror,
            "show_comments": self.show_comments, "show_resolved": self.show_resolved,
            "guides": self.guides.iter().map(|g| json!({ "vertical": g.vertical, "pos": g.pos })).collect::<Vec<_>>(),
        })
    }

    // applies the fields that are present; a bad value stops it before anything is applied
    fn set_view(&mut self, v: &Value) -> Result<(), String> {
        let obj = v.as_object().ok_or("expected a JSON object")?;
        let known = ["mode", "zoom", "left_tab", "left_min", "hide_ui", "light_ui", "snap", "snap_pixels", "pixel_grid", "rulers", "layout_guides", "prop_labels", "labels", "spell", "outlines", "outline_hidden", "outline_bounds", "pixel_preview", "color_profile", "nudge_small", "nudge_big", "handle_mirror", "show_comments", "show_resolved", "guides"];
        if let Some(k) = obj.keys().find(|k| !known.contains(&k.as_str())) {
            return Err(format!("unknown setting {k:?}: GET /view lists them"));
        }
        let mode = match v["mode"].as_str() {
            Some(m) => Some(crate::Mode::ALL.iter().find(|x| x.1.to_lowercase().starts_with(&m.to_lowercase())).ok_or("mode is design, draw, motion or dev")?.0),
            None => None,
        };
        let tab = match v["left_tab"].as_str() {
            Some("layers") => Some(crate::LeftTab::Layers),
            Some("assets") => Some(crate::LeftTab::Assets),
            Some("history") => Some(crate::LeftTab::History),
            Some("comments") => Some(crate::LeftTab::Comments),
            Some(_) => return Err("left_tab is layers, assets, history or comments".into()),
            None => None,
        };
        let profile = match v["color_profile"].as_str() {
            Some("srgb") => Some(0),
            Some("p3") => Some(1),
            Some(_) => return Err("color_profile is srgb or p3".into()),
            None => None,
        };
        let guides = match v.get("guides") {
            Some(g) => Some(serde_json::from_value::<Vec<Guide>>(g.clone()).map_err(|e| format!("guides are [{{\"vertical\": true, \"pos\": 100}}]: {e}"))?),
            None => None,
        };
        let pv = match v["pixel_preview"].as_u64() {
            Some(n @ 0..=2) => Some(n as u8),
            Some(_) => return Err("pixel_preview is 0, 1 or 2".into()),
            None => None,
        };
        let mirror = match v["handle_mirror"].as_u64() {
            Some(n @ 0..=2) => Some(n as u8),
            Some(_) => return Err("handle_mirror is 0, 1 or 2".into()),
            None => None,
        };
        let flag = |k: &str, to: &mut bool| {
            if let Some(b) = v[k].as_bool() {
                *to = b;
            }
        };
        flag("left_min", &mut self.left_min);
        flag("hide_ui", &mut self.hide_ui);
        flag("light_ui", &mut self.light_ui);
        flag("snap", &mut self.snap);
        flag("snap_pixels", &mut self.snap_pixels);
        flag("pixel_grid", &mut self.pixel_grid);
        flag("rulers", &mut self.rulers);
        flag("layout_guides", &mut self.layout_guides);
        flag("prop_labels", &mut self.prop_labels);
        flag("labels", &mut self.labels);
        flag("spell", &mut self.spell);
        flag("outlines", &mut self.outlines);
        flag("outline_hidden", &mut self.outline_hidden);
        flag("outline_bounds", &mut self.outline_bounds);
        flag("show_comments", &mut self.show_comments);
        flag("show_resolved", &mut self.show_resolved);
        if let Some(n) = v["nudge_small"].as_f64().filter(|n| *n > 0.0) {
            self.nudge_small = n as f32;
        }
        if let Some(n) = v["nudge_big"].as_f64().filter(|n| *n > 0.0) {
            self.nudge_big = n as f32;
        }
        self.pixel_preview = pv.unwrap_or(self.pixel_preview);
        self.handle_mirror = mirror.unwrap_or(self.handle_mirror);
        if let Some(p) = profile {
            self.color_profile = p;
        }
        if let Some(t) = tab {
            self.left_tab = t;
        }
        if let Some(g) = guides {
            self.guides = g;
        }
        if let Some(z) = v["zoom"].as_f64() {
            self.zoom_at_center(z as f32);
        }
        if let Some(m) = mode {
            self.set_mode(m);
        }
        Ok(())
    }

    fn motion_json(&self) -> Value {
        json!({
            "time": self.anim_time, "length": self.anim_len(), "loop": self.pages[self.cur].anim_loop, "playing": self.anim_play.is_some(),
            "mode": crate::Mode::ALL.iter().find(|m| m.0 == self.mode).map(|m| m.1),
            "note": "the page shows the playhead's frame in Motion mode (PUT /view {\"mode\": \"motion\"}); edits there become keyframes",
        })
    }

    // the routes of this file; None for a path that is not one of them
    pub(crate) fn api_more(&mut self, r: &Req, seg: &[&str]) -> Option<Answer> {
        let body = || -> Result<Value, Resp> { serde_json::from_slice(&r.body).map_err(|e| Resp::err(400, format!("body is not JSON: {e}"))) };
        let index = |s: &str, len: usize| -> Result<usize, Resp> {
            let i = s.parse::<usize>().map_err(|_| Resp::err(400, "indexes are numbers"))?;
            if i < len { Ok(i) } else { Err(Resp::err(404, "no such entry")) }
        };
        let confirmed = |v: &Value| v["confirm"].as_bool() == Some(true);
        Some((|| match (r.method.as_str(), seg) {
            // ---- images, instances, copies of pages and layers
            ("POST", ["images"]) => {
                use base64::Engine;
                let v = body()?;
                let bytes = match (v["path"].as_str(), v["data"].as_str()) {
                    (Some(p), _) => std::fs::read(p).map_err(|e| Resp::err(400, format!("{p}: {e}")))?,
                    (_, Some(d)) => base64::engine::general_purpose::STANDARD.decode(d).map_err(|e| Resp::err(400, format!("data is base64: {e}")))?,
                    _ => return Err(Resp::err(400, "expected {\"path\": \"x.png\"} or {\"data\": \"<base64>\", \"name\": \"x.png\"}")),
                };
                let name = v["name"].as_str().map(String::from).or_else(|| v["path"].as_str().and_then(|p| std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()))).unwrap_or_else(|| "image".into());
                if v["place"].as_bool() == Some(false) {
                    let id = self.register_image(&name, bytes).ok_or_else(|| Resp::err(400, self.status.clone()))?;
                    return Ok(Resp::json(200, json!({ "asset": id })));
                }
                let at = v["x"].as_f64().zip(v["y"].as_f64()).map(|(x, y)| egui::Pos2::new(x as f32, y as f32));
                let first = self.next_id;
                self.add_image(&name, bytes, at);
                if self.next_id == first {
                    return Err(Resp::err(400, self.status.clone()));
                }
                // top-level layers made here are the agent's own, like those of POST /layers
                let made: Vec<u64> = self.shapes.iter().filter(|s| s.id >= first && s.parent.is_none()).map(|s| s.id).collect();
                self.api_owned.extend(made.iter().copied());
                Ok(Resp::json(200, json!({ "layer": made.last(), "asset": first })))
            }
            ("POST", ["instances"]) => {
                let v = body()?;
                let main = v["main"].as_u64().ok_or_else(|| Resp::err(400, "expected {\"main\": <component id>}"))?;
                let parent = v["parent"].as_u64();
                if let Some(p) = parent {
                    self.guard(p).map_err(|e| Resp::err(403, e))?;
                }
                let pos = egui::Pos2::new(v["x"].as_f64().unwrap_or(0.0) as f32, v["y"].as_f64().unwrap_or(0.0) as f32);
                let id = self.instantiate(main, parent, pos).ok_or_else(|| Resp::err(400, "main is not a component, or the parent is inside it"))?;
                if parent.is_none() {
                    self.api_owned.insert(id);
                }
                Ok(Resp::json(200, json!({ "id": id })))
            }
            ("POST", ["pages", i, "duplicate"]) => {
                let i = index(i, self.pages.len())?;
                self.duplicate_page(i);
                Ok(Resp::json(200, json!({ "current": self.cur, "pages": self.pages.len() })))
            }
            ("POST", ["copy"]) => {
                if let Some(ids) = body().ok().and_then(|v| v["ids"].as_array().cloned()) {
                    self.sel = ids.iter().filter_map(Value::as_u64).filter(|&id| self.index_of(id).is_some()).collect();
                }
                // the app's own clipboard only: the person's system clipboard is theirs
                self.clipboard = self.selection_subtrees();
                self.paste_n = 0.0;
                Ok(Resp::json(200, json!({ "layers": self.clipboard.len() })))
            }
            ("POST", ["paste"]) => {
                let first = self.next_id;
                self.paste_internal();
                let made: Vec<u64> = self.shapes.iter().filter(|s| s.id >= first && s.parent.is_none()).map(|s| s.id).collect();
                self.api_owned.extend(made.iter().copied());
                Ok(Resp::json(200, json!({ "ids": made })))
            }
            // ---- the pointer tools, driven by coordinates instead of a drag (canvas coordinates; `ids` are the layers they work on)
            ("POST", ["tools", tool]) => {
                let v = body()?;
                let pt = |p: &Value| -> Result<egui::Pos2, Resp> {
                    match (p[0].as_f64(), p[1].as_f64()) {
                        (Some(x), Some(y)) => Ok(egui::Pos2::new(x as f32, y as f32)),
                        _ => Err(Resp::err(400, "a point is [x, y]")),
                    }
                };
                let pts = |k: &str| -> Result<Vec<egui::Pos2>, Resp> { v[k].as_array().map(|a| a.iter().map(&pt).collect()).unwrap_or(Ok(vec![])) };
                // in the app the tools fall back to every layer on the page when nothing is selected; a script must name its layers
                let ids: Vec<u64> = v["ids"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
                if ids.is_empty() || ids.iter().any(|&id| self.index_of(id).is_none()) {
                    return Err(Resp::err(400, "send \"ids\": the layers to work on"));
                }
                for &id in &ids {
                    self.guard(id).map_err(|e| Resp::err(403, e))?;
                }
                self.sel = ids;
                let first = self.next_id;
                match *tool {
                    "cut" => {
                        let (a, b) = (pt(&v["from"])?, pt(&v["to"])?);
                        self.cut_along(a, b);
                    }
                    "erase" => {
                        let path = pts("points")?;
                        if path.len() < 2 {
                            return Err(Resp::err(400, "erase needs \"points\": [[x, y], ...], at least two"));
                        }
                        self.erase_along(&path);
                    }
                    "builder" | "paint" => {
                        let (merge, delete) = (pts("merge")?, pts("delete")?);
                        self.vtool = Some(if *tool == "builder" { crate::vtools::VTool::Builder } else { crate::vtools::VTool::Paint });
                        self.start_builder();
                        if self.builder.is_none() {
                            let why = self.status.clone();
                            self.vtool = None;
                            return Err(Resp::err(400, why));
                        }
                        // a point inside a region picks it: `merge` joins regions (Shape builder) or fills them (Paint), `delete` removes them
                        for p in merge {
                            self.builder_touch(p, false);
                        }
                        for p in delete.into_iter().filter(|_| *tool == "builder") {
                            self.builder_touch(p, true);
                        }
                        self.finish_builder();
                        self.builder = None;
                        self.vtool = None;
                    }
                    _ => return Err(Resp::err(400, "tool is cut, erase, builder or paint")),
                }
                let made: Vec<u64> = self.shapes.iter().filter(|s| s.id >= first && s.parent.is_none()).map(|s| s.id).collect();
                self.api_owned.extend(made);
                Ok(Resp::json(200, json!({ "status": self.status, "selection": self.sel })))
            }
            // ---- Present: the person's canvas shows the prototype until DELETE /present
            ("POST", ["present"]) => {
                let id = body()?["id"].as_u64().ok_or_else(|| Resp::err(400, "expected {\"id\": <frame id>}"))?;
                self.index_of(id).filter(|&i| self.shapes[i].kind == Kind::Frame).ok_or_else(|| Resp::err(404, "no such frame"))?;
                self.start_present(id);
                Ok(Resp::json(200, json!({ "presenting": id })))
            }
            ("DELETE", ["present"]) => {
                self.exit_present();
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            // ---- view and preferences
            ("GET", ["view"]) => Ok(Resp::json(200, self.view_json())),
            ("PUT", ["view"]) => {
                let v = body()?;
                self.set_view(&v).map_err(|e| Resp::err(400, e))?;
                Ok(Resp::json(200, self.view_json()))
            }
            // ---- the Motion timeline
            ("GET", ["motion"]) => Ok(Resp::json(200, self.motion_json())),
            ("PUT", ["motion"]) => {
                let v = body()?;
                if let Some(ms) = v["length"].as_f64() {
                    self.pages[self.cur].anim_ms = (ms as f32).clamp(100.0, 600_000.0);
                }
                if let Some(on) = v["loop"].as_bool() {
                    self.pages[self.cur].anim_loop = on;
                }
                if let Some(t) = v["time"].as_f64() {
                    self.anim_play = None;
                    self.anim_time = (t as f32).clamp(0.0, self.anim_len());
                }
                if let Some(play) = v["play"].as_bool() {
                    // playing also plays the page's sound layers, if it has any
                    let now = self.ctx.as_ref().map_or(0.0, |c| c.input(|i| i.time));
                    if play != self.anim_play.is_some() {
                        self.toggle_play(now);
                    }
                }
                Ok(Resp::json(200, self.motion_json()))
            }
            // ---- styles
            ("PATCH", ["styles", kind, id]) | ("DELETE", ["styles", kind, id]) => {
                let id = id.parse::<u64>().map_err(|_| Resp::err(400, "ids are numbers"))?;
                let v = body().unwrap_or(Value::Null);
                // a changed or deleted style restyles every layer that follows it, the person's designs included
                if !confirmed(&v) {
                    return Err(Resp::err(403, "this restyles every layer that follows the style: send {\"confirm\": true} only when the person asked for it"));
                }
                let merge = |old: &mut Value| {
                    for (k, val) in v.as_object().into_iter().flatten().filter(|(k, _)| !matches!(k.as_str(), "id" | "confirm")) {
                        old[k] = val.clone();
                    }
                };
                macro_rules! edit_list {
                    ($list:expr) => {{
                        let k = $list.iter().position(|s| s.id == id).ok_or_else(|| Resp::err(404, "no such style"))?;
                        if r.method == "DELETE" {
                            $list.remove(k);
                        } else {
                            let mut j = serde_json::to_value(&$list[k]).map_err(|e| Resp::err(500, e))?;
                            merge(&mut j);
                            $list[k] = serde_json::from_value(j).map_err(|e| Resp::err(400, format!("not a {} style: {e}", kind)))?;
                        }
                    }};
                }
                match *kind {
                    "color" if r.method == "DELETE" => {
                        let k = self.styles.iter().position(|s| s.id == id).ok_or_else(|| Resp::err(404, "no such style"))?;
                        self.delete_style(k); // the layers keep the color the style had
                    }
                    "color" => edit_list!(self.styles),
                    "effects" => edit_list!(self.ext.effects),
                    "texts" => edit_list!(self.ext.texts),
                    "grids" => edit_list!(self.ext.grids),
                    "anims" => edit_list!(self.ext.anims),
                    _ => return Err(Resp::err(400, "kind is color, effects, texts, grids or anims")),
                }
                self.apply_ext_styles();
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            ("GET", ["styles"]) => Ok(Resp::json(
                200,
                json!({
                    "color": self.styles, "effects": self.ext.effects, "texts": self.ext.texts, "grids": self.ext.grids, "anims": self.ext.anims,
                    "usage": "layers follow a style through their fills / strokes ('style'), 'effect_style', 'text_style' or 'grid_style'",
                }),
            )),
            ("POST", ["styles", kind]) => {
                let mut v = body()?;
                if v["name"].as_str().is_none_or(|n| n.trim().is_empty()) {
                    return Err(Resp::err(400, "a style needs a name"));
                }
                let id = self.new_id();
                v["id"] = json!(id);
                let bad = |e: serde_json::Error| Resp::err(400, format!("not a {kind} style: {e}"));
                match *kind {
                    "color" => self.styles.push(serde_json::from_value(v).map_err(bad)?),
                    "effects" => self.ext.effects.push(serde_json::from_value(v).map_err(bad)?),
                    "texts" => self.ext.texts.push(serde_json::from_value(v).map_err(bad)?),
                    "grids" => self.ext.grids.push(serde_json::from_value(v).map_err(bad)?),
                    "anims" => self.ext.anims.push(serde_json::from_value(v).map_err(bad)?),
                    _ => return Err(Resp::err(400, "kind is color, effects, texts, grids or anims")),
                }
                Ok(Resp::json(200, json!({ "id": id })))
            }
            // ---- version history
            ("GET", ["versions"]) => Ok(Resp::json(200, Value::Array(self.versions.iter().enumerate().map(|(i, v)| json!({ "index": i, "name": v.name, "secs": v.secs, "layers": v.shapes.len() })).collect()))),
            ("POST", ["versions"]) => {
                let name = body().ok().and_then(|v| v["name"].as_str().map(|n| n.trim().to_string())).filter(|n| !n.is_empty());
                let name = name.unwrap_or_else(|| format!("Version {}", self.versions.len() + 1));
                self.versions.push(Version { name, secs: now(), shapes: self.shapes.clone() });
                Ok(Resp::json(200, json!({ "index": self.versions.len() - 1 })))
            }
            ("POST", ["versions", i, "restore"]) => {
                let i = index(i, self.versions.len())?;
                if !confirmed(&body()?) {
                    return Err(Resp::err(403, "restoring replaces the open page with the saved one: send {\"confirm\": true} only when the person asked for it"));
                }
                let snap = self.snap();
                self.undo.push(snap);
                self.redo.clear();
                self.shapes = self.versions[i].shapes.clone();
                self.sel.clear();
                self.sizes.clear();
                self.status = format!("restored {}", self.versions[i].name);
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            // ---- branches
            ("GET", ["branches"]) => Ok(Resp::json(
                200,
                Value::Array(self.branches.iter().enumerate().map(|(i, b)| json!({ "index": i, "name": b.name, "from": b.from, "current": i == self.cur_branch, "review": b.review.state })).collect()),
            )),
            ("POST", ["branches"]) => {
                let name = body().ok().and_then(|v| v["name"].as_str().map(String::from)).unwrap_or_default();
                self.create_branch(&name);
                Ok(Resp::json(200, json!({ "index": self.cur_branch, "name": self.branches[self.cur_branch].name })))
            }
            ("PUT", ["branches", i]) => {
                let i = index(i, self.branches.len())?;
                self.switch_branch(i);
                Ok(Resp::json(200, json!({ "current": self.cur_branch })))
            }
            ("POST", ["branches", i, "review"]) => {
                let i = index(i, self.branches.len())?;
                let who = body()?["reviewer"].as_str().filter(|w| !w.trim().is_empty()).ok_or_else(|| Resp::err(400, "expected {\"reviewer\": \"name\"}"))?.to_string();
                // asking is the agent's to do; approving is the person's alone
                self.branches[i].review = Review { state: 1, reviewer: who, secs: now(), ..Review::default() };
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            ("PUT", ["branches", i, "review"]) => {
                let i = index(i, self.branches.len())?;
                let v = body()?;
                // the agent may hand the request to someone else or withdraw it; an answer (approve, send back) is the person's
                match v["state"].as_str() {
                    Some("withdrawn") => self.branches[i].review = Review::default(),
                    Some("requested") => {
                        let who = v["reviewer"].as_str().unwrap_or(&self.branches[i].review.reviewer).to_string();
                        self.request_review(i, &who);
                    }
                    _ => return Err(Resp::err(403, "state is \"requested\" (with a reviewer) or \"withdrawn\"; approving and sending back are the person's")),
                }
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            ("POST", ["branches", i, "merge"]) => {
                let i = index(i, self.branches.len())?;
                if !self.can_merge(i) {
                    return Err(Resp::err(400, "main has nothing to merge into"));
                }
                if self.branches[i].review.state != 2 {
                    return Err(Resp::err(403, "the branch needs an approved review, and only the person can approve it in the Branches window"));
                }
                let notes = self.merge_branch(i);
                Ok(Resp::json(200, json!({ "conflicts": notes })))
            }
            ("DELETE", ["branches", i]) => {
                let i = index(i, self.branches.len())?;
                let b = &self.branches[i];
                let same = |a: &[Page], b: &[Page]| serde_json::to_value(a).ok() == serde_json::to_value(b).ok();
                if i == 0 || i == self.cur_branch || !same(&b.pages, &b.base) {
                    return Err(Resp::err(403, "only a branch that is not open, not main and has no changes can be deleted through the API"));
                }
                self.delete_branch(i);
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            // ---- libraries
            ("GET", ["libraries"]) => Ok(Resp::json(
                200,
                json!({
                    "added": self.libs.iter().map(|l| json!({ "id": l.id, "name": l.name, "path": l.path, "version": l.version, "update": self.library_has_update(l.id) })).collect::<Vec<_>>(),
                    "this_file": { "id": self.lib_id, "version": self.lib_version },
                }),
            )),
            ("POST", ["libraries"]) | ("POST", ["libraries", "publish"]) => {
                let path = PathBuf::from(body()?["path"].as_str().ok_or_else(|| Resp::err(400, "expected {\"path\": \"x.drlib\"}"))?);
                if path.extension().and_then(|e| e.to_str()) != Some("drlib") {
                    return Err(Resp::err(400, "libraries are .drlib files"));
                }
                if seg.len() == 2 { self.publish_library_to(path) } else { self.add_library_from(path) }
                Ok(Resp::json(200, json!({ "status": self.status })))
            }
            ("POST", ["libraries", id, "update"]) => {
                let id = id.parse::<u64>().map_err(|_| Resp::err(400, "ids are numbers"))?;
                if !self.libs.iter().any(|l| l.id == id) {
                    return Err(Resp::err(404, "no such library"));
                }
                self.update_from_library(id);
                Ok(Resp::json(200, json!({ "status": self.status })))
            }
            _ => Err(Resp::err(0, "")),
        })())
        .filter(|res| !matches!(res, Err(e) if e.code == 0))
    }
}
