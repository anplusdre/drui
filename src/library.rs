// Libraries: a file (.drlib) that publishes a document's components, styles and variables for other documents. A document that adds
// the library gets its styles and variables (with new ids) and can insert its components: the first insert copies the component
// into the page as a main component that remembers where it came from (`Shape::lib`), instances are ordinary instances of that
// copy. When the library file is published again, "Update" brings the copies (and so their instances) up to date.
use std::collections::HashMap;
use std::path::PathBuf;

use egui::Vec2;
use serde::{Deserialize, Serialize};

use crate::model::*;
use crate::App;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LibComponent {
    pub name: String,
    pub root: u64,  // the component, or the set frame of a component set
    pub first: u64, // what an instance is made of: the component, or the set's first variant
    pub shapes: Vec<Shape>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub id: u64,
    pub name: String,
    pub version: u64,
    pub description: String, // what the publisher said changed
    pub components: Vec<LibComponent>,
    pub styles: Vec<ColorStyle>,
    pub ext: ExtStyles,
    pub variables: Vec<Collection>,
}

fn tr(map: &HashMap<u64, u64>, id: u64) -> u64 {
    map.get(&id).copied().unwrap_or(id)
}

// the ids a library shape uses for styles, variables and modes, turned into this document's
fn translate(s: &mut Shape, map: &HashMap<u64, u64>) {
    for p in s.fills.iter_mut().chain(s.strokes.iter_mut()) {
        p.style = p.style.map(|v| tr(map, v));
        p.var = p.var.map(|v| tr(map, v));
    }
    for b in s.bindings.iter_mut() {
        b.1 = tr(map, b.1);
    }
    for e in s.effects.iter_mut() {
        e.var_color = e.var_color.map(|v| tr(map, v));
        e.var_blur = e.var_blur.map(|v| tr(map, v));
        e.var_pos = e.var_pos.map(|v| v.map(|v| tr(map, v)));
    }
    for g in s.grids.iter_mut() {
        g.var_color = g.var_color.map(|v| tr(map, v));
        g.var_num = g.var_num.map(|v| v.map(|v| tr(map, v)));
    }
    for m in s.modes.iter_mut() {
        *m = (tr(map, m.0), tr(map, m.1));
    }
    s.effect_style = s.effect_style.map(|v| tr(map, v));
    s.text_style = s.text_style.map(|v| tr(map, v));
    s.grid_style = s.grid_style.map(|v| tr(map, v));
    s.var = s.var.map(|v| tr(map, v));
    for ix in s.more.iter_mut() {
        ix.var = ix.var.map(|v| tr(map, v));
    }
}

fn read_library(path: &str) -> Result<Library, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str::<Library>(&text).map_err(|e| e.to_string())
}

impl App {
    // everything this document offers as a library
    pub fn build_library(&self) -> Library {
        let mut components = vec![];
        for (first, name) in self.components() {
            // a component named with a leading . or _ is hidden from publishing, as in Figma
            if name.starts_with('.') || name.starts_with('_') {
                continue;
            }
            let Some(fi) = self.index_of(first) else { continue };
            let root = self.set_of(fi).map_or(first, |s| self.shapes[s].id);
            let shapes: Vec<Shape> = self.shapes.iter().filter(|s| self.is_inside(s.id, root)).cloned().collect();
            components.push(LibComponent { name, root, first, shapes });
        }
        let name = self.file.file_stem().map_or("Library".to_owned(), |n| n.to_string_lossy().into_owned());
        // variables marked Hide from publishing stay in this file
        let mut variables = self.variables.clone();
        for c in &mut variables {
            c.vars.retain(|v| !v.hidden);
        }
        Library { id: self.lib_id, name, version: self.lib_version, description: self.lib_description.clone(), components, styles: self.styles.clone(), ext: self.ext.clone(), variables }
    }

    // "Publish library": writes the .drlib file; the version counts up with every publish
    pub fn publish_library(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("drui library", &["drlib"]).set_file_name(format!("{}.drlib", self.file.file_stem().map_or("library".into(), |n| n.to_string_lossy().into_owned()))).save_file() else { return };
        self.publish_library_to(path);
    }

    pub fn publish_library_to(&mut self, path: PathBuf) {
        if self.lib_id == 0 {
            // a library keeps its identity across publishes: made from the clock, the id counter keeps ids of one file apart
            self.lib_id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64 >> 8);
        }
        self.lib_version += 1;
        let lib = self.build_library();
        self.status = match serde_json::to_string(&lib).map(|s| std::fs::write(&path, s)) {
            Ok(Ok(())) => format!("published {} components as {} (version {})", lib.components.len(), path.display(), lib.version),
            _ => "publishing the library failed".into(),
        };
    }

    // "Add library": the styles and variables come into the document with new ids; the components are inserted when used
    pub fn add_library(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("drui library", &["drlib"]).pick_file() else { return };
        self.add_library_from(path);
    }

    pub fn add_library_from(&mut self, path: PathBuf) {
        let path_s = path.to_string_lossy().into_owned();
        let lib = match read_library(&path_s) {
            Ok(l) => l,
            Err(e) => {
                self.status = format!("could not read the library: {e}");
                return;
            }
        };
        if lib.id == 0 || lib.id == self.lib_id || self.libs.iter().any(|l| l.id == lib.id) {
            self.status = "that library is already in this file".into();
            return;
        }
        let mut map: HashMap<u64, u64> = HashMap::new();
        let ids = lib.styles.iter().map(|s| s.id).chain(lib.ext.ids()).chain(lib.variables.iter().flat_map(|c| std::iter::once(c.id).chain(c.modes.iter().map(|m| m.id)).chain(c.vars.iter().map(|v| v.id)))).collect::<Vec<u64>>();
        for id in ids {
            map.insert(id, self.new_id());
        }
        for mut s in lib.styles.iter().cloned() {
            s.id = tr(&map, s.id);
            self.styles.push(s);
        }
        for mut s in lib.ext.effects.iter().cloned() {
            s.id = tr(&map, s.id);
            for e in &mut s.effects {
                e.var_color = e.var_color.map(|v| tr(&map, v));
                e.var_blur = e.var_blur.map(|v| tr(&map, v));
                e.var_pos = e.var_pos.map(|v| v.map(|v| tr(&map, v)));
            }
            self.ext.effects.push(s);
        }
        for mut s in lib.ext.texts.iter().cloned() {
            s.id = tr(&map, s.id);
            self.ext.texts.push(s);
        }
        for mut s in lib.ext.anims.iter().cloned() {
            s.id = tr(&map, s.id);
            self.ext.anims.push(s);
        }
        for mut s in lib.ext.grids.iter().cloned() {
            s.id = tr(&map, s.id);
            for g in &mut s.grids {
                g.var_color = g.var_color.map(|v| tr(&map, v));
                g.var_num = g.var_num.map(|v| v.map(|v| tr(&map, v)));
            }
            self.ext.grids.push(s);
        }
        for mut c in lib.variables.iter().cloned() {
            c.id = tr(&map, c.id);
            c.extends = c.extends.map(|e| tr(&map, e));
            for m in &mut c.modes {
                m.id = tr(&map, m.id);
            }
            let fix = |v: &mut VarValue| {
                if let VarValue::Alias(a) = v {
                    *a = tr(&map, *a);
                }
            };
            for v in &mut c.vars {
                v.id = tr(&map, v.id);
                v.values.iter_mut().for_each(fix);
            }
            for o in &mut c.overrides {
                o.0 = tr(&map, o.0);
                o.1 = tr(&map, o.1);
                fix(&mut o.2);
            }
            self.variables.push(c);
        }
        self.libs.push(LibRef { id: lib.id, name: lib.name.clone(), path: path_s, version: lib.version, map: map.into_iter().collect() });
        self.lib_files.insert(lib.id, lib);
        self.status = "library added".into();
    }

    // the library's file, read once and kept (Check for updates reads it again)
    pub fn library_file(&mut self, id: u64) -> Option<&Library> {
        if !self.lib_files.contains_key(&id) {
            let path = self.libs.iter().find(|l| l.id == id)?.path.clone();
            let lib = read_library(&path).ok()?;
            self.lib_files.insert(id, lib);
        }
        self.lib_files.get(&id)
    }

    pub fn refresh_library(&mut self, id: u64) {
        self.lib_files.remove(&id);
        let _ = self.library_file(id);
    }

    fn id_map(&self, lib: u64) -> HashMap<u64, u64> {
        self.libs.iter().find(|l| l.id == lib).map(|l| l.map.iter().copied().collect()).unwrap_or_default()
    }

    // inserts the component of a library into this page (once) and returns the local component an instance is made of
    pub fn import_library_component(&mut self, lib_id: u64, root: u64) -> Option<u64> {
        let comp = self.library_file(lib_id)?.components.iter().find(|c| c.root == root)?.clone();
        if let Some(s) = self.shapes.iter().find(|s| s.lib == Some((lib_id, comp.first))) {
            return Some(s.id);
        }
        let map = self.id_map(lib_id);
        let right = self.shapes.iter().filter(|s| s.parent.is_none()).map(|s| s.pos.x + s.size.x).fold(0.0f32, f32::max);
        let (mut copies, ids) = self.remap(&comp.shapes, Some(None), Vec2::ZERO);
        let back: HashMap<u64, u64> = ids.iter().map(|(&o, &n)| (n, o)).collect();
        let left = copies.iter().filter(|c| c.parent.is_none()).map(|c| c.pos.x).fold(f32::MAX, f32::min);
        for c in &mut copies {
            c.lib = back.get(&c.id).map(|&o| (lib_id, o));
            translate(c, &map);
            if c.parent.is_none() {
                c.pos.x += right + 80.0 - left;
            }
        }
        self.shapes.extend(copies);
        ids.get(&comp.first).copied()
    }

    // an instance of a library component in the middle of the view
    pub fn insert_library_component(&mut self, lib_id: u64, root: u64) {
        if let Some(local) = self.import_library_component(lib_id, root) {
            self.instantiate_in_view(local);
        }
    }

    pub fn library_has_update(&self, lib_id: u64) -> bool {
        match (self.libs.iter().find(|l| l.id == lib_id), self.lib_files.get(&lib_id)) {
            (Some(r), Some(f)) => f.version > r.version,
            _ => false,
        }
    }

    // brings the inserted components of a library up to the published version; instances follow through the usual sync
    pub fn update_from_library(&mut self, lib_id: u64) {
        let Some(lib) = self.library_file(lib_id).cloned() else { return };
        let map = self.id_map(lib_id);
        let before = self.shapes.clone();
        let mut known: Vec<u64> = vec![];
        for comp in &lib.components {
            if !self.shapes.iter().any(|s| s.lib == Some((lib_id, comp.first))) {
                continue; // never inserted: nothing to update
            }
            for ls in &comp.shapes {
                known.push(ls.id);
                let mut t = ls.clone();
                translate(&mut t, &map);
                match self.shapes.iter().position(|s| s.lib == Some((lib_id, ls.id))) {
                    Some(k) => {
                        let root = self.shapes[k].parent.is_none() || ls.id == comp.root;
                        crate::comp::copy_fields(&mut self.shapes[k], &t, root);
                    }
                    None => {
                        // a layer the library gained: a copy under the layer that matches its parent
                        let Some(parent) = ls.parent.and_then(|p| self.shapes.iter().find(|s| s.lib == Some((lib_id, p))).map(|s| s.id)) else { continue };
                        let id = self.new_id();
                        t.id = id;
                        t.parent = Some(parent);
                        t.lib = Some((lib_id, ls.id));
                        t.src = None;
                        self.shapes.push(t);
                    }
                }
            }
        }
        // layers the library no longer has
        let gone: Vec<u64> = self.shapes.iter().filter(|s| s.lib.is_some_and(|(l, o)| l == lib_id && !known.contains(&o))).map(|s| s.id).collect();
        for id in gone {
            if self.index_of(id).is_some() {
                self.remove_subtree(id);
            }
        }
        if let Some(r) = self.libs.iter_mut().find(|l| l.id == lib_id) {
            r.version = lib.version;
        }
        self.sync_instances(&before);
        self.status = "library updated".into();
    }

    pub fn remove_library(&mut self, lib_id: u64) {
        self.libs.retain(|l| l.id != lib_id);
        self.lib_files.remove(&lib_id);
    }
}
