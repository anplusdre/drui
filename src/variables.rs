// Variables, like Figma's: collections of named values (colour, number, string, boolean) with one value per mode. A layer is
// bound to a variable through a colour paint (`Paint::var`) or a property (`Shape::bindings`); `resolve_variables` (run by
// `settle`) writes the variable's value, in the layer's modes, into the bound field, so everything else (rendering, layout,
// export, Dev Mode) just sees ordinary values. A frame can choose a mode per collection (`Shape::modes`); layers inside inherit
// it, otherwise a collection's first mode applies.
//
// The Variables window (rail button) edits collections, modes and variables; `var_field` is the small "apply variable"
// control next to a number field; the colour picker's Libraries tab lists colour variables.
use egui::{Align2, Color32, Id, Order, Pos2, Rect, Sense, Stroke, Vec2};

use crate::fig::{self, Num, Prefix};
use crate::glyphs::{menu_item, paint_glyph_sized, Glyph};
use crate::model::*;
use crate::{App, BG, R_LARGE, TEXT, TEXT_SECONDARY, TEXT_TERTIARY};

impl App {
    fn find_var(&self, var: u64) -> Option<(&Collection, &Variable)> {
        self.variables.iter().find_map(|c| c.vars.iter().find(|v| v.id == var).map(|v| (c, v)))
    }

    // the index of the mode of `coll` that applies to layer `at`: the nearest ancestor-or-self that chose one, else the first
    fn mode_index(&self, coll: &Collection, at: usize) -> usize {
        self.chain(at)
            .iter()
            .find_map(|&k| self.shapes[k].modes.iter().find(|(c, _)| *c == coll.id).map(|(_, m)| *m))
            // layers that chose nothing follow the page's mode (Set variable mode)
            .or_else(|| self.page_modes.iter().find(|(c, _)| *c == coll.id).map(|(_, m)| *m))
            .and_then(|m| coll.modes.iter().position(|x| x.id == m))
            .unwrap_or(0)
    }

    // the value of variable `var` for layer `at`
    pub fn variable_value(&self, var: u64, at: usize) -> Option<VarValue> {
        let mut cur = var;
        for _ in 0..16 {
            match self.own_variable_value(cur, at)? {
                VarValue::Alias(next) => cur = next,
                v => return Some(v),
            }
        }
        None // aliases that go round in a circle
    }

    // the value of a variable in the layer's modes before aliases are followed; an extension of its collection that the layer has a
    // mode for may override it
    fn own_variable_value(&self, var: u64, at: usize) -> Option<VarValue> {
        let (c, v) = self.find_var(var)?;
        for e in self.variables.iter().filter(|e| e.extends == Some(c.id)) {
            if self.chain(at).iter().any(|&k| self.shapes[k].modes.iter().any(|(cid, _)| *cid == e.id)) {
                let idx = self.mode_index(e, at);
                let mode_id = e.modes.get(idx)?.id;
                if let Some((_, _, val)) = e.overrides.iter().find(|(vid, m, _)| *vid == var && *m == mode_id) {
                    return Some(val.clone());
                }
                return v.values.get(idx).or(v.values.first()).cloned();
            }
        }
        v.values.get(self.mode_index(c, at)).or(v.values.first()).cloned()
    }

    // writes every bound variable's value into its field; true if anything changed (so `settle` runs again)
    pub fn resolve_variables(&mut self) -> bool {
        if self.variables.is_empty() {
            return false;
        }
        let mut changed = false;
        for i in 0..self.shapes.len() {
            let s = &self.shapes[i];
            if s.bindings.is_empty() && s.fills.iter().chain(&s.strokes).all(|p| p.var.is_none()) && s.effects.iter().all(|e| e.var_color.is_none() && e.var_blur.is_none() && e.var_pos.iter().all(Option::is_none)) && s.grids.iter().all(|g| g.var_color.is_none() && g.var_num.iter().all(Option::is_none)) {
                continue;
            }
            // effects and layout guides: their colour and blur follow variables
            let fx: Vec<(usize, Option<Color32>, Option<f32>)> = s
                .effects
                .iter()
                .enumerate()
                .map(|(k, e)| {
                    let c = e.var_color.and_then(|v| self.variable_value(v, i)).and_then(|v| if let VarValue::Color(c) = v { Some(c) } else { None });
                    let b = e.var_blur.and_then(|v| self.variable_value(v, i)).and_then(|v| if let VarValue::Number(n) = v { Some(n.max(0.0)) } else { None });
                    (k, c, b)
                })
                .collect();
            let num_of = |v: Option<u64>| v.and_then(|v| self.variable_value(v, i)).and_then(|v| if let VarValue::Number(n) = v { Some(n) } else { None });
            let pos: Vec<(usize, [Option<f32>; 3])> = s.effects.iter().enumerate().map(|(k, e)| (k, e.var_pos.map(num_of))).collect();
            let gn: Vec<(usize, [Option<f32>; 2])> = s.grids.iter().enumerate().map(|(k, g)| (k, g.var_num.map(num_of))).collect();
            let gc: Vec<(usize, Option<Color32>)> = s.grids.iter().enumerate().map(|(k, g)| (k, g.var_color.and_then(|v| self.variable_value(v, i)).and_then(|v| if let VarValue::Color(c) = v { Some(c) } else { None }))).collect();
            {
                let s = &mut self.shapes[i];
                for (k, c, b) in fx {
                    if let Some(c) = c.filter(|c| s.effects[k].color != *c) {
                        s.effects[k].color = c;
                        changed = true;
                    }
                    if let Some(b) = b.filter(|b| (s.effects[k].blur - *b).abs() > 1e-4) {
                        s.effects[k].blur = b;
                        changed = true;
                    }
                }
                for (k, [dx, dy, spread]) in pos {
                    let e = &mut s.effects[k];
                    for (v, slot) in [(dx, &mut e.dx), (dy, &mut e.dy), (spread, &mut e.spread)] {
                        if let Some(v) = v.filter(|v| (*slot - *v).abs() > 1e-4) {
                            *slot = v;
                            changed = true;
                        }
                    }
                }
                for (k, [size, gutter]) in gn {
                    let g = &mut s.grids[k];
                    for (v, slot, min) in [(size, &mut g.size, 2.0), (gutter, &mut g.gutter, 0.0)] {
                        if let Some(v) = v.map(|v| v.max(min)).filter(|v| (*slot - *v).abs() > 1e-4) {
                            *slot = v;
                            changed = true;
                        }
                    }
                }
                for (k, c) in gc {
                    if let Some(c) = c.filter(|c| s.grids[k].color != *c) {
                        s.grids[k].color = c;
                        changed = true;
                    }
                }
            }
            let s = &self.shapes[i];
            let paints: Vec<(bool, usize, Color32)> = s
                .fills
                .iter()
                .enumerate()
                .map(|(k, p)| (false, k, p))
                .chain(s.strokes.iter().enumerate().map(|(k, p)| (true, k, p)))
                .filter_map(|(stroke, k, p)| match self.variable_value(p.var?, i)? {
                    VarValue::Color(c) => Some((stroke, k, c)),
                    _ => None,
                })
                .collect();
            let values: Vec<(BindProp, VarValue)> = s.bindings.iter().filter_map(|(p, v)| self.variable_value(*v, i).map(|val| (*p, val))).collect();
            let s = &mut self.shapes[i];
            for (stroke, k, c) in paints {
                let p = if stroke { &mut s.strokes[k] } else { &mut s.fills[k] };
                if p.kind != PaintKind::Solid(c) {
                    p.kind = PaintKind::Solid(c);
                    changed = true;
                }
            }
            let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
            for (prop, val) in values {
                match (prop, val) {
                    (BindProp::Opacity, VarValue::Number(v)) => {
                        let o = (v / 100.0).clamp(0.0, 1.0);
                        if !near(s.opacity, o) {
                            s.opacity = o;
                            changed = true;
                        }
                    }
                    (BindProp::Radius, VarValue::Number(v)) => {
                        let r = v.max(0.0);
                        if s.radius.iter().any(|&x| !near(x, r)) {
                            s.radius = [r; 4];
                            changed = true;
                        }
                    }
                    (BindProp::StrokeWeight, VarValue::Number(v)) => {
                        if !near(s.stroke_w, v.max(0.0)) {
                            s.stroke_w = v.max(0.0);
                            changed = true;
                        }
                    }
                    (BindProp::Width, VarValue::Number(v)) => {
                        if !near(s.size.x, v.max(0.0)) {
                            s.size.x = v.max(0.0);
                            changed = true;
                        }
                    }
                    (BindProp::Height, VarValue::Number(v)) => {
                        if !near(s.size.y, v.max(0.0)) {
                            s.size.y = v.max(0.0);
                            changed = true;
                        }
                    }
                    (BindProp::FontSize, VarValue::Number(v)) => {
                        if !near(s.font_size, v.max(1.0)) {
                            s.font_size = v.max(1.0);
                            changed = true;
                        }
                    }
                    (BindProp::Gap, VarValue::Number(v)) => {
                        if let Some(l) = s.layout.as_mut().filter(|l| !near(l.gap, v.max(0.0))) {
                            l.gap = v.max(0.0);
                            changed = true;
                        }
                    }
                    (BindProp::PadX, VarValue::Number(v)) => {
                        if let Some(l) = s.layout.as_mut().filter(|l| !near(l.pad[0], v.max(0.0)) || !near(l.pad[2], v.max(0.0))) {
                            l.pad[0] = v.max(0.0);
                            l.pad[2] = v.max(0.0);
                            changed = true;
                        }
                    }
                    (BindProp::PadY, VarValue::Number(v)) => {
                        if let Some(l) = s.layout.as_mut().filter(|l| !near(l.pad[1], v.max(0.0)) || !near(l.pad[3], v.max(0.0))) {
                            l.pad[1] = v.max(0.0);
                            l.pad[3] = v.max(0.0);
                            changed = true;
                        }
                    }
                    (BindProp::Visible, VarValue::Bool(b)) => {
                        if s.visible != b {
                            s.visible = b;
                            changed = true;
                        }
                    }
                    (BindProp::Text, VarValue::Text(t)) => {
                        if s.text != t {
                            s.text = t;
                            changed = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        changed
    }

    // binds (or, with None, unbinds) property `prop` of layer `i`
    pub fn bind_property(&mut self, i: usize, prop: BindProp, var: Option<u64>) {
        let s = &mut self.shapes[i];
        s.bindings.retain(|(p, _)| *p != prop);
        if let Some(v) = var {
            s.bindings.push((prop, v));
        }
    }

    pub fn bound_var(&self, i: usize, prop: BindProp) -> Option<u64> {
        self.shapes[i].bindings.iter().find(|(p, _)| *p == prop).map(|(_, v)| *v)
    }

    // ---- the Variables window ----

    pub fn variables_window(&mut self, ctx: &egui::Context) {
        if !self.vars_open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !ctx.text_edit_focused() {
            self.vars_open = false;
            return;
        }
        let screen = ctx.content_rect();
        let size = Vec2::new((screen.width() - 120.0).clamp(480.0, 880.0), (screen.height() - 140.0).clamp(300.0, 540.0));
        let origin = Pos2::new(screen.center().x - size.x / 2.0, screen.bottom() - size.y - 72.0).max(screen.min + Vec2::splat(8.0));
        if self.variables.is_empty() {
            // Figma starts a file with one collection
            let (cid, mid) = (self.new_id(), self.new_id());
            self.variables.push(Collection { id: cid, name: "Collection 1".into(), modes: vec![VarMode { id: mid, name: "Mode 1".into() }], ..Collection::default() });
        }
        self.var_coll = self.var_coll.min(self.variables.len() - 1);
        let mut close = false;
        let mut ops: Vec<Op> = vec![];
        let new_ids = [self.new_id(), self.new_id(), self.new_id()];
        egui::Area::new(Id::new("variables-window")).order(Order::Foreground).fixed_pos(origin).show(ctx, |ui| {
            let card = Rect::from_min_size(origin, size);
            ui.allocate_exact_size(size, Sense::hover());
            ui.painter().add(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(110) }.as_shape(card, R_LARGE));
            ui.painter().rect(card, R_LARGE, BG(), Stroke::new(1.0, fig::RULE()), egui::StrokeKind::Inside);
            // header
            fig::ptext(ui.painter(), Pos2::new(card.left() + 16.0, card.top() + 20.0), Align2::LEFT_CENTER, "Variables", fig::semi(11.0), TEXT());
            if fig::icon_button(ui, Rect::from_min_size(Pos2::new(card.right() - 32.0, card.top() + 8.0), Vec2::splat(fig::ROW)), Glyph::Close, "Close variables", false).clicked() {
                close = true;
            }
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(card.left(), card.top() + 40.0), Vec2::new(card.width(), 1.0)), 0.0, fig::RULE());
            // the collections
            let side = Rect::from_min_size(Pos2::new(card.left(), card.top() + 41.0), Vec2::new(200.0, card.height() - 41.0));
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(side.right(), side.top()), Vec2::new(1.0, side.height())), 0.0, fig::RULE());
            fig::ptext(ui.painter(), Pos2::new(side.left() + 16.0, side.top() + 20.0), Align2::LEFT_CENTER, "Collections", fig::semi(11.0), TEXT_SECONDARY());
            if fig::icon_button(ui, Rect::from_min_size(Pos2::new(side.right() - 32.0, side.top() + 8.0), Vec2::splat(fig::ROW)), Glyph::Plus, "Create collection", false).clicked() {
                ops.push(Op::AddCollection);
            }
            for (k, c) in self.variables.iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(side.left() + 8.0, side.top() + 40.0 + 32.0 * k as f32), Vec2::new(side.width() - 16.0, 28.0));
                let resp = ui.interact(r, Id::new(("collection", c.id)), Sense::click());
                if self.var_coll == k {
                    ui.painter().rect_filled(r, fig::RADIUS, fig::SELECTED());
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, fig::RADIUS, fig::INPUT());
                }
                fig::ptext(&ui.painter().with_clip_rect(r), Pos2::new(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, &c.name, fig::reg(11.0), if self.var_coll == k { fig::SELECTED_INK() } else { TEXT() });
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, self.var_coll == k, format!("Collection {}", c.name)));
                if resp.clicked() {
                    ops.push(Op::Select(k));
                }
            }
            // the table of the selected collection
            let main = Rect::from_min_max(Pos2::new(side.right() + 1.0, side.top()), card.max);
            let ci = self.var_coll;
            // the collection this one extends, and every variable that a value could be an alias of
            let parent: Option<Collection> = self.variables[ci].extends.and_then(|p| self.variables.iter().find(|c| c.id == p).cloned());
            let pool: Vec<(u64, String, VarKind)> = self.variables.iter().flat_map(|c| c.vars.iter().map(move |v| (v.id, format!("{} / {}", c.name, v.name), v.kind))).collect();
            let mut dummy = Variable::default();
            let coll = &mut self.variables[ci];
            // top bar: the collection's name, create variable
            let bar = Rect::from_min_size(main.min, Vec2::new(main.width(), 40.0));
            fig::text_input(ui, Rect::from_min_size(Pos2::new(bar.left() + 16.0, bar.top() + 8.0), Vec2::new(200.0, fig::ROW)), &mut coll.name, "Collection name", "Collection");
            let add = fig::button(ui, Rect::from_min_size(Pos2::new(bar.right() - 160.0, bar.top() + 8.0), Vec2::new(144.0, fig::ROW)), "+ Create variable");
            if parent.is_none() {
                egui::Popup::menu(&add).show(|ui| {
                    for (k, label) in VarKind::ALL {
                        if menu_item(ui, None, label, "", true) {
                            ops.push(Op::AddVar(k));
                            ui.close();
                        }
                    }
                    crate::glyphs::menu_sep(ui);
                    if menu_item(ui, Some(Glyph::Pipette), "Color from eyedropper", "", true) {
                        ops.push(Op::Eyedrop);
                        ui.close();
                    }
                });
                let ticked = coll.vars.iter().filter(|v| self.var_sel.contains(&v.id)).count();
                if ticked > 0 {
                    let bulk = fig::button(ui, Rect::from_min_size(Pos2::new(bar.right() - 480.0, bar.top() + 8.0), Vec2::new(152.0, fig::ROW)), &format!("Edit {ticked} selected"));
                    egui::Popup::menu(&bulk).show(|ui| {
                        for (k, label) in ["Duplicate", "Hide from publishing", "Show in publishing", "Allow everywhere", "Delete", "Clear selection"].into_iter().enumerate() {
                            if menu_item(ui, None, label, "", true) {
                                ops.push(Op::Bulk(k as u8));
                                ui.close();
                            }
                        }
                    });
                }
                let ext = fig::button(ui, Rect::from_min_size(Pos2::new(bar.right() - 320.0, bar.top() + 8.0), Vec2::new(152.0, fig::ROW)), "Extend collection");
                if ext.clicked() {
                    ops.push(Op::Extend);
                }
            } else if add.clicked() {
                // an extended collection has no variables of its own
            }
            if let Some(p) = &parent {
                fig::ptext(ui.painter(), Pos2::new(bar.left() + 224.0, bar.center().y), Align2::LEFT_CENTER, format!("extends {}", p.name), fig::reg(11.0), TEXT_TERTIARY());
            }
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(main.left(), bar.bottom()), Vec2::new(main.width(), 1.0)), 0.0, fig::RULE());
            // the header row: Name, one column per mode, +
            let (name_w, col_w) = (220.0, 168.0);
            let head = Rect::from_min_size(Pos2::new(main.left(), bar.bottom() + 1.0), Vec2::new(main.width(), 32.0));
            fig::ptext(ui.painter(), Pos2::new(head.left() + 16.0, head.center().y), Align2::LEFT_CENTER, "Name", fig::semi(11.0), TEXT_SECONDARY());
            let n_modes = coll.modes.len();
            for (m, mode) in coll.modes.iter_mut().enumerate() {
                let x = head.left() + 16.0 + name_w + col_w * m as f32;
                let f = Rect::from_min_size(Pos2::new(x, head.top() + 4.0), Vec2::new(col_w - 36.0, fig::ROW));
                fig::text_input(ui, f, &mut mode.name, &format!("Mode {} name", m + 1), "Mode");
                if n_modes > 1 && fig::icon_button(ui, Rect::from_min_size(Pos2::new(f.right() + 4.0, f.top()), Vec2::splat(fig::ROW)), Glyph::Minus, "Remove mode", false).clicked() {
                    ops.push(Op::DelMode(m));
                }
            }
            let plus_x = head.left() + 16.0 + name_w + col_w * n_modes as f32;
            if plus_x + 24.0 < main.right() && fig::icon_button(ui, Rect::from_min_size(Pos2::new(plus_x, head.top() + 4.0), Vec2::splat(fig::ROW)), Glyph::Plus, "Create mode", false).clicked() {
                ops.push(Op::AddMode);
            }
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(main.left(), head.bottom()), Vec2::new(main.width(), 1.0)), 0.0, fig::RULE());
            // the rows
            let rows = Rect::from_min_max(Pos2::new(main.left(), head.bottom() + 1.0), main.max);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rows));
            let extended = parent.is_some();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let list: Vec<Variable> = match &parent {
                    Some(p) => p.vars.clone(),
                    None => coll.vars.clone(),
                };
                if list.is_empty() {
                    let r = fig::row(ui, 64.0);
                    let msg = if extended { "The collection this one extends has no variables yet." } else { "No variables yet. Create one with + Create variable." };
                    fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, msg, fig::reg(11.0), TEXT_TERTIARY());
                }
                let mut last_group = String::new();
                for (vi, listed) in list.iter().enumerate() {
                    // a group of variables ("Brand/primary" is in Brand) gets a title row
                    let group = listed.group().0.to_owned();
                    if group != last_group {
                        if !group.is_empty() {
                            let g = fig::row(ui, 28.0);
                            fig::ptext(ui.painter(), Pos2::new(g.left() + 16.0, g.center().y), Align2::LEFT_CENTER, &group, fig::semi(11.0), TEXT_SECONDARY());
                        }
                        last_group = group;
                    }
                    let r = fig::row(ui, 40.0);
                    let y = r.top() + 8.0;
                    // an extended collection shows the parent's variables with its own overrides on top; the plain one edits its own
                    if extended {
                        dummy = listed.clone();
                    }
                    let var: &mut Variable = if extended { &mut dummy } else { &mut coll.vars[vi] };
                    let (vid, kind) = (listed.id, listed.kind);
                    if !extended {
                        // a tick to pick the variable for a bulk edit
                        let tb = Rect::from_center_size(Pos2::new(r.left() + 8.0, r.center().y), Vec2::splat(12.0));
                        let tresp = ui.interact(tb, Id::new(("var-tick", vid)), Sense::click());
                        let on = self.var_sel.contains(&vid);
                        ui.painter().rect(tb, 2.0, if on { crate::BRAND } else { Color32::TRANSPARENT }, Stroke::new(1.0, if on { crate::BRAND } else { TEXT_SECONDARY() }), egui::StrokeKind::Inside);
                        if on {
                            paint_glyph_sized(ui.painter(), tb.center(), Glyph::Check, Color32::WHITE, 9.0);
                        }
                        tresp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, format!("Select {}", listed.name)));
                        if tresp.clicked() {
                            ops.push(Op::Bulk(if on { 10 } else { 11 }));
                            self.var_tick = Some(vid);
                        }
                    }
                    if extended {
                        fig::ptext(&ui.painter().with_clip_rect(Rect::from_min_size(Pos2::new(r.left() + 16.0, y), Vec2::new(name_w - 40.0, fig::ROW))), Pos2::new(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, &listed.name, fig::reg(11.0), TEXT());
                    } else {
                        fig::text_input(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, y), Vec2::new(name_w - 40.0, fig::ROW)), &mut var.name, &format!("Variable {} name", vi + 1), "Name");
                    }
                    let kind_icon = match kind {
                        VarKind::Color => Glyph::Droplet,
                        VarKind::Number => Glyph::Ratio,
                        VarKind::Text => Glyph::Type,
                        VarKind::Bool => Glyph::CircleCheck,
                    };
                    paint_glyph_sized(ui.painter(), Pos2::new(r.left() + 16.0 + name_w - 22.0, r.center().y), kind_icon, TEXT_SECONDARY(), 13.0);
                    for m in 0..n_modes {
                        let x = r.left() + 16.0 + name_w + col_w * m as f32;
                        let f = Rect::from_min_size(Pos2::new(x, y), Vec2::new(col_w - 36.0, fig::ROW));
                        let label = format!("{} value, mode {}", listed.name, m + 1);
                        if extended {
                            let mode_id = coll.modes[m].id;
                            let own = coll.overrides.iter().position(|(v, md, _)| *v == vid && *md == mode_id);
                            let eff = own.map(|k| coll.overrides[k].2.clone()).or_else(|| listed.values.get(m).or(listed.values.first()).cloned()).unwrap_or_else(|| VarValue::default_for(kind));
                            let mut cur = eff.clone();
                            value_cell(ui, f, Id::new(("var-value", vid, m, "ext")), &label, kind, &mut cur, &pool, vid);
                            if cur != eff {
                                match own {
                                    Some(k) => coll.overrides[k].2 = cur,
                                    None => coll.overrides.push((vid, mode_id, cur)),
                                }
                            }
                            // a value of its own is marked, and can go back to the parent's
                            if let Some(k) = own {
                                if fig::icon_button(ui, Rect::from_min_size(Pos2::new(f.right() + 4.0, f.top()), Vec2::splat(fig::ROW)), Glyph::RotateCcw, "Reset to the parent's value", true).clicked() {
                                    coll.overrides.remove(k);
                                }
                            }
                        } else if let Some(val) = var.values.get_mut(m) {
                            value_cell(ui, f, Id::new(("var-value", vid, m)), &label, kind, val, &pool, vid);
                        }
                    }
                    if !extended {
                        // where the variable may be applied
                        let sb = Rect::from_min_size(Pos2::new(r.right() - 60.0, y), Vec2::splat(fig::ROW));
                        let scoped = !var.scopes.is_empty();
                        let sresp = fig::icon_button(ui, sb, Glyph::Sliders, if scoped { "Scopes: limited" } else { "Scopes: everywhere" }, scoped);
                        egui::Popup::menu(&sresp).show(|ui| {
                            for (sc, name) in VarScope::choices(kind) {
                                let on = var.scopes.contains(sc);
                                if crate::glyphs::menu_check(ui, on, name, "") {
                                    if on {
                                        var.scopes.retain(|x| x != sc);
                                    } else {
                                        var.scopes.push(*sc);
                                    }
                                }
                            }
                            if scoped {
                                crate::glyphs::menu_sep(ui);
                                if menu_item(ui, None, "Allow everywhere", "", true) {
                                    var.scopes.clear();
                                    ui.close();
                                }
                            }
                        });
                        // how the token is named in code (Dev Mode's code syntax); empty = made from the name
                        let named = var.syntax.iter().any(|s| !s.is_empty());
                        let cb = Rect::from_min_size(Pos2::new(r.right() - 88.0, y), Vec2::splat(fig::ROW));
                        let cresp = fig::icon_button(ui, cb, Glyph::Code, if named { "Code syntax: set" } else { "Code syntax" }, named);
                        egui::Popup::menu(&cresp).show(|ui| {
                            ui.set_min_width(264.0);
                            for (k, label) in ["Web", "Android", "iOS"].into_iter().enumerate() {
                                let row = fig::row(ui, fig::ROW + 4.0);
                                fig::ptext(ui.painter(), Pos2::new(row.left() + 12.0, row.center().y), Align2::LEFT_CENTER, label, fig::reg(11.0), TEXT_SECONDARY());
                                let hint = ["--color-primary", "colorPrimary", "colorPrimary"][k];
                                fig::text_input(ui, Rect::from_min_size(Pos2::new(row.left() + 76.0, row.top() + 2.0), Vec2::new(row.width() - 88.0, fig::ROW)), &mut var.syntax[k], &format!("{label} code syntax"), hint);
                            }
                        });
                        let mb = Rect::from_min_size(Pos2::new(r.right() - 116.0, y), Vec2::splat(fig::ROW));
                        let mresp = fig::icon_button(ui, mb, Glyph::More, "More actions", var.hidden);
                        egui::Popup::menu(&mresp).show(|ui| {
                            if menu_item(ui, Some(Glyph::Copy), "Duplicate", "", true) {
                                ops.push(Op::Dup(vi));
                                ui.close();
                            }
                            if crate::glyphs::menu_check(ui, var.hidden, "Hide from publishing", "") {
                                var.hidden ^= true;
                            }
                        });
                        if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 32.0, y), Vec2::splat(fig::ROW)), Glyph::Minus, "Delete variable", false).clicked() {
                            ops.push(Op::DelVar(vi));
                        }
                    }
                    ui.painter().rect_filled(Rect::from_min_size(Pos2::new(r.left(), r.bottom() - 1.0), Vec2::new(r.width(), 1.0)), 0.0, Color32::from_gray(0x36));
                }
            });
        });
        // apply what was asked
        for op in ops {
            let ci = self.var_coll;
            match op {
                Op::Eyedrop => {
                    self.eyedrop_var = true;
                    self.vars_open = false;
                    self.start_eyedropper();
                    self.status = "Eyedropper: click a layer to make a color variable from it".into();
                }
                Op::Dup(k) => self.duplicate_variable(ci, k),
                Op::Bulk(what) => {
                    let ids: Vec<u64> = self.var_sel.clone();
                    match what {
                        10 | 11 => {
                            if let Some(v) = self.var_tick.take() {
                                if what == 10 { self.var_sel.retain(|&x| x != v) } else { self.var_sel.push(v) }
                            }
                        }
                        0 => {
                            for id in ids {
                                if let Some(k) = self.variables[ci].vars.iter().position(|v| v.id == id) {
                                    self.duplicate_variable(ci, k);
                                }
                            }
                        }
                        1 | 2 | 3 => {
                            for v in self.variables[ci].vars.iter_mut().filter(|v| ids.contains(&v.id)) {
                                match what {
                                    1 => v.hidden = true,
                                    2 => v.hidden = false,
                                    _ => v.scopes.clear(),
                                }
                            }
                        }
                        4 => {
                            for id in ids {
                                if let Some(k) = self.variables[ci].vars.iter().position(|v| v.id == id) {
                                    self.variables[ci].vars.remove(k);
                                    self.unbind_everywhere(id);
                                }
                            }
                            self.var_sel.clear();
                        }
                        _ => self.var_sel.clear(),
                    }
                }
                Op::Select(k) => self.var_coll = k,
                Op::AddCollection => {
                    let n = self.variables.len() + 1;
                    self.variables.push(Collection { id: new_ids[0], name: format!("Collection {n}"), modes: vec![VarMode { id: new_ids[1], name: "Mode 1".into() }], ..Collection::default() });
                    self.var_coll = self.variables.len() - 1;
                }
                Op::AddVar(kind) => {
                    let c = &mut self.variables[ci];
                    let n = c.vars.iter().filter(|v| v.kind == kind).count() + 1;
                    let name = match kind {
                        VarKind::Color => format!("color {n}"),
                        VarKind::Number => format!("number {n}"),
                        VarKind::Text => format!("string {n}"),
                        VarKind::Bool => format!("boolean {n}"),
                    };
                    c.vars.push(Variable { id: new_ids[0], name, kind, values: vec![VarValue::default_for(kind); c.modes.len()], ..Variable::default() });
                }
                Op::DelVar(k) => {
                    let id = self.variables[ci].vars.remove(k).id;
                    self.unbind_everywhere(id);
                }
                Op::Extend => {
                    let p = self.variables[ci].clone();
                    let mut modes: Vec<VarMode> = p.modes.iter().map(|m| VarMode { id: 0, name: m.name.clone() }).collect();
                    for m in modes.iter_mut() {
                        m.id = self.new_id();
                    }
                    self.variables.push(Collection { id: new_ids[1], name: format!("{} extended", p.name), modes, extends: Some(p.id), ..Collection::default() });
                    self.var_coll = self.variables.len() - 1;
                }
                Op::AddMode => {
                    let c = &mut self.variables[ci];
                    let n = c.modes.len() + 1;
                    c.modes.push(VarMode { id: new_ids[0], name: format!("Mode {n}") });
                    for v in &mut c.vars {
                        let copy = v.values.first().cloned().unwrap_or_else(|| VarValue::default_for(v.kind));
                        v.values.push(copy);
                    }
                }
                Op::DelMode(m) => {
                    let c = &mut self.variables[ci];
                    if c.modes.len() > 1 {
                        let gone = c.modes.remove(m).id;
                        for v in &mut c.vars {
                            v.values.remove(m.min(v.values.len() - 1));
                        }
                        for s in &mut self.shapes {
                            s.modes.retain(|(_, mode)| *mode != gone);
                        }
                    }
                }
            }
        }
        if close {
            self.vars_open = false;
        }
    }

    // a copy of variable `k` of collection `ci`, right after it (name + " copy")
    pub fn duplicate_variable(&mut self, ci: usize, k: usize) {
        let id = self.new_id();
        let Some(c) = self.variables.get_mut(ci) else { return };
        let Some(v) = c.vars.get(k).cloned() else { return };
        let copy = Variable { id, name: format!("{} copy", v.name), ..v };
        c.vars.insert(k + 1, copy);
    }

    // Create variable > Color from eyedropper: a colour variable with the picked colour in every mode
    pub fn add_color_variable(&mut self, color: egui::Color32) {
        let id = self.new_id();
        if self.variables.is_empty() {
            let (cid, mid) = (self.new_id(), self.new_id());
            self.variables.push(Collection { id: cid, name: "Collection 1".into(), modes: vec![VarMode { id: mid, name: "Mode 1".into() }], ..Collection::default() });
        }
        let ci = self.var_coll.min(self.variables.len() - 1);
        let c = &mut self.variables[ci];
        let n = c.vars.iter().filter(|v| v.kind == VarKind::Color).count() + 1;
        c.vars.push(Variable { id, name: format!("color {n}"), kind: VarKind::Color, values: vec![VarValue::Color(color); c.modes.len()], ..Variable::default() });
        self.vars_open = true;
        self.status = format!("variable color {n} made from #{:02X}{:02X}{:02X}", color.r(), color.g(), color.b());
    }

    #[cfg(test)]
    pub fn unbind_everywhere_for_test(&mut self, var: u64) {
        self.unbind_everywhere(var);
    }

    // a deleted variable leaves the layers that used it with the value they last had
    fn unbind_everywhere(&mut self, var: u64) {
        for c in &mut self.variables {
            c.overrides.retain(|(v, _, _)| *v != var);
            for v in &mut c.vars {
                for val in v.values.iter_mut().chain(c.overrides.iter_mut().map(|o| &mut o.2)) {
                    if *val == VarValue::Alias(var) {
                        *val = VarValue::default_for(v.kind);
                    }
                }
            }
        }
        for s in &mut self.shapes {
            s.bindings.retain(|(_, v)| *v != var);
            for p in s.fills.iter_mut().chain(s.strokes.iter_mut()) {
                if p.var == Some(var) {
                    p.var = None;
                }
            }
        }
    }
}

enum Op {
    Dup(usize),
    Eyedrop,
    Bulk(u8),
    Extend,
    Select(usize),
    AddCollection,
    AddVar(VarKind),
    DelVar(usize),
    AddMode,
    DelMode(usize),
}

// the small "apply variable" control of a number field, as Figma has: an icon at the field's right edge that opens the list of
// number variables; a bound field shows the variable's name instead of the value. Returns Some(choice) when the user chose a
// variable (Some(Some(id))) or detached it (Some(None)).
pub fn var_field(ui: &mut egui::Ui, r: Rect, salt: Id, bound: Option<u64>, collections: &[Collection], kind: VarKind, scope: VarScope) -> Option<Option<u64>> {
    let name = bound.and_then(|id| collections.iter().find_map(|c| c.vars.iter().find(|v| v.id == id))).map(|v| v.name.clone());
    let mut out = None;
    if let Some(name) = &name {
        // the chip covers the field: the value is the variable's
        let resp = ui.interact(r, salt.with("chip"), Sense::click());
        ui.painter().rect_filled(r, fig::RADIUS, fig::SELECTED());
        paint_glyph_sized(ui.painter(), Pos2::new(r.left() + 12.0, r.center().y), Glyph::Sliders, fig::SELECTED_INK(), 13.0);
        fig::ptext(&ui.painter().with_clip_rect(Rect::from_min_max(r.min, Pos2::new(r.right() - 22.0, r.bottom()))), Pos2::new(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, name, fig::reg(11.0), fig::SELECTED_INK());
        paint_glyph_sized(ui.painter(), Pos2::new(r.right() - 12.0, r.center().y), Glyph::ChevronDown, fig::SELECTED_INK(), 9.5);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Variable {name}")));
        out = var_menu(&resp, bound, collections, kind, scope, true);
    } else if collections.iter().any(|c| c.vars.iter().any(|v| v.kind == kind && v.allows(scope))) || ui.rect_contains_pointer(r) {
        // the apply control shows while the pointer is over the field
        let b = Rect::from_center_size(Pos2::new(r.right() - 12.0, r.center().y), Vec2::splat(18.0));
        if ui.rect_contains_pointer(r) {
            let resp = ui.interact(b, salt.with("apply"), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(b, fig::RADIUS, fig::INPUT_HOVER());
            }
            paint_glyph_sized(ui.painter(), b.center(), Glyph::Sliders, TEXT_SECONDARY(), 12.0);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Apply variable"));
            out = var_menu(&resp, None, collections, kind, scope, false);
        }
    }
    out
}

// a submenu row ("Visibility", "Text") that lists variables of `kind`: Some(Some(id)) chosen, Some(None) detached
pub fn var_submenu(ui: &mut egui::Ui, label: &str, bound: Option<u64>, collections: &[Collection], kind: VarKind, scope: VarScope) -> Option<Option<u64>> {
    let mut out = None;
    crate::glyphs::menu_button(ui, label, |ui| {
        let mut any = false;
        for c in collections {
            for v in c.vars.iter().filter(|v| v.kind == kind && v.allows(scope)) {
                any = true;
                if crate::glyphs::menu_check(ui, bound == Some(v.id), &v.name, "") {
                    out = Some(Some(v.id));
                    ui.close();
                }
            }
        }
        if !any {
            let (r, _) = ui.allocate_exact_size(Vec2::new(190.0, 28.0), Sense::hover());
            fig::ptext(ui.painter(), Pos2::new(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, "No variables of this type yet", fig::reg(11.0), TEXT_TERTIARY());
        }
        if bound.is_some() {
            crate::glyphs::menu_sep(ui);
            if menu_item(ui, None, "Detach variable", "", true) {
                out = Some(None);
                ui.close();
            }
        }
    });
    out
}

// the list of variables of `kind`, grouped by collection; the chosen one, or None-in-Some to detach
pub fn var_menu(resp: &egui::Response, bound: Option<u64>, collections: &[Collection], kind: VarKind, scope: VarScope, can_detach: bool) -> Option<Option<u64>> {
    egui::Popup::menu(resp)
        .show(|ui| {
            let mut pick = None;
            let mut any = false;
            for c in collections {
                let vars: Vec<&Variable> = c.vars.iter().filter(|v| v.kind == kind && v.allows(scope)).collect();
                if vars.is_empty() {
                    continue;
                }
                any = true;
                if collections.len() > 1 {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.min_rect().width().max(140.0), 22.0), Sense::hover());
                    fig::ptext(ui.painter(), Pos2::new(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, &c.name, fig::semi(10.0), TEXT_TERTIARY());
                }
                for v in vars {
                    if crate::glyphs::menu_check(ui, bound == Some(v.id), &v.name, "") {
                        pick = Some(Some(v.id));
                        ui.close();
                    }
                }
            }
            if !any {
                let (r, _) = ui.allocate_exact_size(Vec2::new(200.0, 28.0), Sense::hover());
                fig::ptext(ui.painter(), Pos2::new(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, "No variables of this type yet", fig::reg(11.0), TEXT_TERTIARY());
            }
            if can_detach {
                crate::glyphs::menu_sep(ui);
                if menu_item(ui, None, "Detach variable", "", true) {
                    pick = Some(None);
                    ui.close();
                }
            }
            pick
        })
        .and_then(|r| r.inner)
}

// one value cell of the Variables table: the typed control (colour, number, string, boolean), or the chip of the variable it is an
// alias of; the link button to the right makes it an alias of another variable of the same type, or lets it go again
fn value_cell(ui: &mut egui::Ui, f: Rect, salt: Id, label: &str, kind: VarKind, val: &mut VarValue, pool: &[(u64, String, VarKind)], own: u64) {
    let link = Rect::from_min_size(Pos2::new(f.right() + 4.0, f.top()), Vec2::splat(fig::ROW));
    if let VarValue::Alias(target) = val {
        let name = pool.iter().find(|p| p.0 == *target).map_or("(missing)".to_owned(), |p| p.1.clone());
        ui.painter().rect_filled(f, fig::RADIUS, fig::SELECTED());
        paint_glyph_sized(ui.painter(), Pos2::new(f.left() + 12.0, f.center().y), Glyph::Link, fig::SELECTED_INK(), 13.0);
        fig::ptext(&ui.painter().with_clip_rect(Rect::from_min_max(f.min, Pos2::new(f.right() - 4.0, f.bottom()))), Pos2::new(f.left() + 24.0, f.center().y), Align2::LEFT_CENTER, &name, fig::reg(11.0), fig::SELECTED_INK());
        if fig::icon_button(ui, link, Glyph::Minus, "Detach alias", true).clicked() {
            *val = VarValue::default_for(kind);
        }
        return;
    }
    match val {
        VarValue::Color(c) => {
            fig::input_bg(ui, f, false);
            fig::swatch(ui, Pos2::new(f.left() + 14.0, f.center().y), c, label);
            let mut rgb = *c;
            if fig::hex_text(ui, Pos2::new(f.left() + 28.0, f.center().y), f.width() - 32.0, &mut rgb, salt.with("hex")) {
                *c = Color32::from_rgba_unmultiplied(rgb.r(), rgb.g(), rgb.b(), c.a());
            }
        }
        VarValue::Number(v) => {
            fig::num_field(ui, f, salt, Prefix::None, v, Num { decimals: 2, ..Num::default() }, label);
        }
        VarValue::Text(t) => {
            fig::text_input(ui, f, t, label, "Value");
        }
        VarValue::Bool(b) => {
            let mut on = *b;
            if fig::dropdown(ui, f, None, &mut on, &[(true, "True"), (false, "False")], label) {
                *b = on;
            }
        }
        VarValue::Alias(_) => {}
    }
    let btn = fig::icon_button(ui, link, Glyph::Link, "Alias to a variable", false);
    egui::Popup::menu(&btn).show(|ui| {
        let mut any = false;
        for (id, name, k) in pool.iter().filter(|p| p.2 == kind && p.0 != own) {
            any = true;
            if menu_item(ui, None, name, "", true) {
                *val = VarValue::Alias(*id);
                ui.close();
            }
            let _ = k;
        }
        if !any {
            menu_item(ui, None, "No other variables of this type", "", false);
        }
    });
}

// the value of `v` in mode `m` of `c`, aliases followed (the target's same-numbered mode, else its last)
pub(crate) fn value(all: &[Collection], c: &Collection, v: &Variable, m: usize) -> Option<VarValue> {
    value_at(all, c, v, m, 0)
}

fn value_at(all: &[Collection], c: &Collection, v: &Variable, m: usize, depth: usize) -> Option<VarValue> {
    let own = c.modes.get(m).and_then(|mode| c.overrides.iter().find(|(id, md, _)| *id == v.id && *md == mode.id)).map(|o| o.2.clone());
    match own.or_else(|| v.values.get(m).or(v.values.first()).cloned())? {
        VarValue::Alias(t) if depth < 16 => {
            let (tc, tv) = all.iter().filter(|c| c.extends.is_none()).find_map(|c| c.vars.iter().find(|x| x.id == t).map(|x| (c, x)))?;
            value_at(all, tc, tv, m.min(tc.modes.len().saturating_sub(1)), depth + 1)
        }
        VarValue::Alias(_) => None, // aliases that go round in a circle
        other => Some(other),
    }
}
