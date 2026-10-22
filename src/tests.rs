use egui::{Pos2, Rect, Vec2};

use crate::geom::BoolOp;
use crate::model::*;
use crate::App;

fn rect_at(a: &mut App, x: f32, y: f32) -> u64 {
    a.add(Kind::Rect, Pos2::new(x, y));
    let i = a.shapes.len() - 1;
    a.shapes[i].size = Vec2::new(10.0, 10.0);
    a.shapes[i].id
}

fn frame(a: &mut App, x: f32, y: f32, w: f32, h: f32) -> u64 {
    a.add(Kind::Frame, Pos2::new(x, y));
    let i = a.shapes.len() - 1;
    a.shapes[i].size = Vec2::new(w, h);
    a.shapes[i].id
}

#[test]
fn nesting() {
    let mut a = App::default();
    a.add(Kind::Frame, Pos2::new(100.0, 100.0)); // id 1
    a.shapes[0].size = Vec2::new(200.0, 200.0);
    a.add(Kind::Rect, Pos2::new(150.0, 150.0)); // id 2, drawn inside the frame
    a.shapes[1].size = Vec2::new(300.0, 20.0); // sticks out past the frame's right edge

    // children are parented to the frame and stored relative to it
    assert_eq!(a.shapes[1].parent, Some(1));
    assert_eq!(a.shapes[1].pos, Pos2::new(50.0, 50.0));
    assert_eq!(a.world_rect(1).min, Pos2::new(150.0, 150.0));

    // the part of the child clipped by the frame can't be clicked
    assert_eq!(a.hit_test(Pos2::new(200.0, 160.0)), Some(2));
    assert_eq!(a.hit_test(Pos2::new(350.0, 160.0)), None);

    // reparenting keeps the on-screen position
    a.reparent(2, None);
    assert_eq!(a.shapes.last().unwrap().parent, None);
    assert_eq!(a.shapes.last().unwrap().pos, Pos2::new(150.0, 150.0));
    a.reparent(2, Some(1));
    assert_eq!(a.shapes.last().unwrap().pos, Pos2::new(50.0, 50.0));

    // a frame can't be dropped into itself, and deleting it takes its children along
    assert_eq!(a.frame_at(Pos2::new(200.0, 200.0), &[1]), None);
    a.remove_subtree(1);
    assert!(a.shapes.is_empty());
}

// a root frame is picked by its title strip, not its body; nested shapes still by body
#[test]
fn root_frame_title_pick() {
    let mut a = App::default();
    frame(&mut a, 100.0, 100.0, 200.0, 200.0);
    assert_eq!(a.hit_test(Pos2::new(200.0, 200.0)), None);
    assert_eq!(a.hit_test(Pos2::new(110.0, 95.0)), Some(1));
}

#[test]
fn groups_and_marquee() {
    let mut a = App::default();
    let (r1, r2, r3) = (rect_at(&mut a, 0.0, 0.0), rect_at(&mut a, 50.0, 50.0), rect_at(&mut a, 500.0, 500.0));

    // marquee picks what it touches, nothing else
    let mut hits = a.marquee_hits(Rect::from_min_max(Pos2::new(-5.0, -5.0), Pos2::new(70.0, 70.0)));
    hits.sort();
    assert_eq!(hits, vec![r1, r2]);

    // group: box hugs the children, they keep their world position
    a.sel = vec![r1, r2];
    a.group();
    a.settle();
    let g = a.sel[0];
    let gi = a.index_of(g).unwrap();
    assert_eq!(a.world_rect(gi), Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(60.0, 60.0)));
    assert_eq!(a.world_rect(a.index_of(r2).unwrap()).min, Pos2::new(50.0, 50.0));

    // clicking a child picks the group, double-click's raw hit is the child
    assert_eq!(a.hit_test(Pos2::new(5.0, 5.0)), Some(r1));
    assert_eq!(a.select_unit(r1), g);
    // a marquee selects the group, not its children
    assert_eq!(a.marquee_hits(Rect::from_min_max(Pos2::new(-5.0, -5.0), Pos2::new(70.0, 70.0))), vec![g]);

    // moving a child out of origin grows the box and shifts the group origin, world positions hold
    let c = a.index_of(r1).unwrap();
    a.shapes[c].pos.x -= 20.0;
    a.settle();
    let gi = a.index_of(g).unwrap();
    assert_eq!(a.world_rect(gi).min, Pos2::new(-20.0, 0.0));
    assert_eq!(a.world_rect(a.index_of(r2).unwrap()).min, Pos2::new(50.0, 50.0));

    // ungroup restores plain root shapes at the same place; r3 is untouched
    a.sel = vec![g];
    a.ungroup();
    assert!(a.shapes.iter().all(|s| s.parent.is_none() && s.kind != Kind::Group));
    assert_eq!(a.sel, vec![r1, r2]);
    assert_eq!(a.world_rect(a.index_of(r1).unwrap()).min, Pos2::new(-20.0, 0.0));
    assert_eq!(a.index_of(r3), Some(2));

    // a group that loses all its children disappears
    a.sel = vec![r1, r2];
    a.group();
    a.remove_subtree(r1);
    a.remove_subtree(r2);
    a.settle();
    assert_eq!(a.shapes.len(), 1);
}

#[test]
fn layer_drag_moves() {
    let mut a = App::default();
    frame(&mut a, 100.0, 100.0, 200.0, 200.0); // id 1
    let (r2, r3) = (rect_at(&mut a, 500.0, 500.0), rect_at(&mut a, 600.0, 600.0)); // root shapes

    // drop r2 into the frame: parent changes, screen position holds
    a.move_to(r2, Some(1), None);
    let i = a.index_of(r2).unwrap();
    assert_eq!(a.shapes[i].parent, Some(1));
    assert_eq!(a.world_rect(i).min, Pos2::new(500.0, 500.0));

    // above r2 (higher z) among its siblings, then below the frame at root level
    a.move_to(r3, Some(1), Some((r2, true)));
    let sibs: Vec<u64> = a.shapes.iter().filter(|s| s.parent == Some(1)).map(|s| s.id).collect();
    assert_eq!(sibs, vec![r2, r3]);
    a.move_to(r3, None, Some((1, false)));
    assert_eq!(a.shapes.iter().position(|s| s.id == r3), Some(0));

    // a frame can't be dropped into its own child
    a.move_to(1, Some(r2), None);
    assert_eq!(a.shapes[a.index_of(1).unwrap()].parent, None);
}

#[test]
fn old_files_load() {
    // the first file format: no ids, parents, names or paints, plain fill/stroke colors
    let old = r#"[{"kind":"Rect","text":"","pos":{"x":3.0,"y":4.0},"size":{"x":10.0,"y":20.0},"fill":[255,0,0,255],"stroke":[0,0,0,255],"stroke_w":2.0}]"#;
    let shapes: Vec<Shape> = serde_json::from_str(old).unwrap();
    let new = migrate(shapes);
    let s = &new[0];
    assert_ne!(s.id, 0);
    assert!(s.visible && !s.locked && s.parent.is_none() && !s.name.is_empty());
    assert_eq!(s.fills.len(), 1);
    assert_eq!(s.strokes.len(), 1);
    assert_eq!(s.stroke_w, 2.0);
}

#[test]
fn rotation_and_resize() {
    let mut a = App::default();
    let r = rect_at(&mut a, 0.0, 0.0);
    let i = a.index_of(r).unwrap();
    a.shapes[i].size = Vec2::new(100.0, 20.0);
    a.shapes[i].rotation = 90.0;
    // a quarter turn about the center swaps the bounding box dimensions
    let b = a.shapes[i].aabb();
    assert!((b.width() - 20.0).abs() < 1e-3 && (b.height() - 100.0).abs() < 1e-3);
    // a click on the rotated body hits, one on the unrotated body's tip doesn't
    assert!(a.shapes[i].hit(Pos2::new(50.0, 55.0), 0.0));
    assert!(!a.shapes[i].hit(Pos2::new(5.0, 10.0), 0.0));
    // dragging the right edge handle (along the shape's own x axis, i.e. down on screen) keeps the opposite edge fixed
    let rect = a.shapes[i].rect();
    let fixed_before = a.shapes[i].xf(rect.left_center());
    a.do_resize(i, (1, 0), rect, Pos2::new(50.0, 160.0), false);
    let s = &a.shapes[i];
    let fixed_after = s.xf(s.rect().left_center());
    assert!((fixed_before - fixed_after).length() < 1e-3);
    assert!((s.size.x - 200.0).abs() < 1e-2);
}

#[test]
fn constraints_follow_frame_resize() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let right = rect_at(&mut a, 80.0, 10.0);
    let both = rect_at(&mut a, 10.0, 40.0);
    let center = rect_at(&mut a, 45.0, 70.0);
    for (id, c) in [(right, Constraint::Max), (both, Constraint::Both), (center, Constraint::Center)] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].ch = c;
    }
    a.settle(); // records the frame size
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].size.x = 200.0;
    a.settle();
    let g = |id| a.shapes[a.index_of(id).unwrap()].clone();
    assert_eq!(g(right).pos.x, 180.0);
    assert_eq!(g(both).size.x, 110.0);
    assert_eq!(g(center).pos.x, 95.0);
}

#[test]
fn auto_layout_lays_out_and_hugs() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 500.0, 500.0);
    let (r1, r2) = (rect_at(&mut a, 300.0, 300.0), rect_at(&mut a, 400.0, 100.0));
    for id in [r1, r2] {
        // both live inside the frame
        assert_eq!(a.shapes[a.index_of(id).unwrap()].parent, Some(f));
    }
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].layout = Some(AutoLayout { gap: 6.0, pad: [4.0; 4], ..AutoLayout::default() });
    a.settle();
    let (s1, s2) = (a.shapes[a.index_of(r1).unwrap()].clone(), a.shapes[a.index_of(r2).unwrap()].clone());
    assert_eq!((s1.pos.x, s1.pos.y), (4.0, 4.0));
    assert_eq!(s2.pos.x, 4.0 + 10.0 + 6.0);
    // hugging: 4 + 10 + 6 + 10 + 4 wide, 4 + 10 + 4 tall
    let f = &a.shapes[a.index_of(f).unwrap()];
    assert_eq!((f.size.x, f.size.y), (34.0, 18.0));
}

#[test]
fn boolean_ops() {
    let mut a = App::default();
    let r1 = rect_at(&mut a, 0.0, 0.0);
    let r2 = rect_at(&mut a, 5.0, 0.0);
    a.sel = vec![r1, r2];
    a.boolean(BoolOp::Union);
    // a live boolean group: the two rectangles stay inside it, it shows (and exports as) their union
    assert_eq!(a.shapes.len(), 3);
    let g = a.single().map(|i| a.shapes[i].clone()).expect("the group is selected");
    assert!(g.kind == Kind::Group && g.bool_op == Some(0) && g.name == "Union");
    let p = a.bool_proxy(&g).expect("a union");
    assert_eq!(p.kind, Kind::Path);
    assert!((p.size.x - 15.0).abs() < 1e-3 && (p.size.y - 10.0).abs() < 1e-3);
    // the operation can change later
    a.boolean(BoolOp::Subtract);
    let g = a.single().map(|i| a.shapes[i].clone()).unwrap();
    assert_eq!((g.bool_op, g.name.as_str()), (Some(1), "Subtract"));
    assert!((a.bool_proxy(&g).unwrap().size.x - 5.0).abs() < 1e-3);
    // flatten turns it into the one vector
    a.flatten();
    assert_eq!(a.shapes.len(), 1);
    assert_eq!(a.shapes[0].kind, Kind::Path);
    assert!((a.shapes[0].size.x - 5.0).abs() < 1e-3);
    a.sel = vec![a.shapes[0].id];
    a.flip(true);
    assert_eq!(a.shapes.len(), 1);
}

#[test]
fn instances_follow_main_but_keep_overrides() {
    let mut a = App::default();
    let main = frame(&mut a, 0.0, 0.0, 100.0, 50.0);
    let child = rect_at(&mut a, 10.0, 10.0);
    a.sel = vec![main];
    a.make_component();
    let inst = a.instantiate(main, None, Pos2::new(300.0, 0.0)).unwrap();
    let ic = a.shapes.iter().find(|s| s.src == Some(child)).unwrap().id;
    assert!(a.shapes[a.index_of(inst).unwrap()].instance);

    // a change in the main flows to the instance
    let before = a.shapes.clone();
    let ci = a.index_of(child).unwrap();
    a.shapes[ci].fills = vec![Paint::solid(egui::Color32::RED)];
    a.sync_instances(&before);
    assert_eq!(a.shapes[a.index_of(ic).unwrap()].fills, vec![Paint::solid(egui::Color32::RED)]);

    // an override on the instance survives the next change in the main
    let ii = a.index_of(ic).unwrap();
    a.shapes[ii].fills = vec![Paint::solid(egui::Color32::BLUE)];
    let before = a.shapes.clone();
    a.shapes[ci].fills = vec![Paint::solid(egui::Color32::GREEN)];
    a.sync_instances(&before);
    assert_eq!(a.shapes[a.index_of(ic).unwrap()].fills, vec![Paint::solid(egui::Color32::BLUE)]);

    // structure follows too: a new child in the main appears in the instance
    let before = a.shapes.clone();
    a.add(Kind::Ellipse, Pos2::new(5.0, 5.0));
    a.sync_instances(&before);
    assert_eq!(a.shapes.iter().filter(|s| s.parent == Some(inst)).count(), 2);
}

#[test]
fn copy_paste_and_align() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let r = rect_at(&mut a, 10.0, 10.0);
    a.sel = vec![f];
    a.duplicate();
    // the copy of the frame brings its child, with fresh ids and the right parent
    assert_eq!(a.shapes.len(), 4);
    let copy = a.sel[0];
    assert_ne!(copy, f);
    assert_eq!(a.shapes.iter().filter(|s| s.parent == Some(copy)).count(), 1);
    // align a lone shape to its parent frame
    a.sel = vec![r];
    a.align(crate::ops::AlignOp::Right);
    assert_eq!(a.shapes[a.index_of(r).unwrap()].pos.x, 90.0);
}

#[test]
fn save_and_load_round_trip() {
    let mut a = App::default();
    rect_at(&mut a, 1.0, 2.0);
    a.add_page();
    rect_at(&mut a, 3.0, 4.0);
    let path = std::env::temp_dir().join("drui_test_roundtrip.json");
    a.save_to(path.clone());
    let mut b = App::default();
    b.load_from(path.clone());
    assert_eq!(b.pages.len(), 2);
    assert_eq!(b.cur, 1);
    assert_eq!(b.shapes.len(), 1);
    b.switch_page(0);
    assert_eq!(b.shapes[0].pos, Pos2::new(1.0, 2.0));
    let _ = std::fs::remove_file(path);
}

#[test]
fn pen_paths_and_nodes() {
    let mut a = App::default();
    a.pen_click(Pos2::new(0.0, 0.0));
    a.pen_click(Pos2::new(100.0, 0.0));
    a.pen_click(Pos2::new(100.0, 100.0));
    a.pen_click(Pos2::new(1.0, 1.0)); // within 8 screen px of the first point: closes
    assert_eq!(a.shapes.len(), 1);
    let s = &a.shapes[0];
    assert_eq!(s.kind, Kind::Path);
    assert!(s.path[0].closed && s.path[0].pts.len() == 3);
    assert_eq!((s.size.x, s.size.y), (100.0, 100.0));
    // inside the triangle hits, the empty corner doesn't
    assert!(s.hit(Pos2::new(80.0, 20.0), 0.0));
    assert!(!s.hit(Pos2::new(10.0, 90.0), 0.0));

    // a new node on the hypotenuse; moving one anchor refits the box
    a.sel = vec![a.shapes[0].id];
    assert!(a.node_insert_at(Pos2::new(50.0, 50.0)));
    assert_eq!(a.shapes[0].path[0].pts.len(), 4);
    a.node_move(0, 0, 0, Pos2::new(-50.0, 0.0));
    assert_eq!(a.shapes[0].pos.x, -50.0);
}

#[test]
fn export_renders_png_and_pdf() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    frame(&mut a, 0.0, 0.0, 120.0, 80.0);
    rect_at(&mut a, 10.0, 10.0);
    a.add(Kind::Star, Pos2::new(40.0, 20.0));
    let i = a.shapes.len() - 1;
    a.shapes[i].size = Vec2::new(30.0, 30.0);
    a.shapes[i].fills = vec![Paint { kind: PaintKind::Linear { a: egui::Color32::RED, b: egui::Color32::BLUE, angle: 45.0 }, ..Paint::default() }];
    a.shapes[i].effects = vec![Effect::new(EffectKind::DropShadow)];
    a.add(Kind::Text, Pos2::new(5.0, 60.0));
    let t = a.shapes.len() - 1;
    a.shapes[t].text = "Hello\nworld".into();
    let roots: Vec<u64> = a.shapes.iter().filter(|s| s.parent.is_none()).map(|s| s.id).collect();
    let mut out = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        out = a.svg_for(ui.ctx(), &roots);
    });
    let (svg, bounds) = out.unwrap();
    assert!(svg.contains("<text") && svg.contains("linearGradient") && svg.contains("feGaussianBlur") && svg.contains("<mask"));
    assert_eq!((bounds.width(), bounds.height()), (120.0, 80.0));
    let png = crate::export::render_png(&svg, 2.0).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let pdf = crate::export::render_pdf(&svg).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

// ---- headless canvas harness: real pointer events through `App::canvas`

struct Harness {
    ctx: egui::Context,
    t: f64,
}

impl Harness {
    fn new() -> Self {
        Self { ctx: egui::Context::default(), t: 0.0 }
    }

    fn frame(&mut self, app: &mut App, events: Vec<egui::Event>) {
        self.t += 0.1;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
            time: Some(self.t),
            events,
            ..Default::default()
        };
        let _ = self.ctx.run_ui(raw, |ui| {
            // Motion mode edits the page at the playhead, as App::ui does
            let base = app.shapes.clone();
            let shown = if app.mode == crate::Mode::Motion { app.motion_begin() } else { None };
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| app.canvas(ui));
            if let Some(shown) = shown {
                app.motion_end(&base, &shown);
            }
        });
    }

    // a frame that runs the keyboard shortcuts as well as the canvas
    fn keys(&mut self, app: &mut App, events: Vec<egui::Event>) {
        self.t += 0.1;
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))), time: Some(self.t), events, ..Default::default() };
        let _ = self.ctx.run_ui(raw, |ui| {
            app.shortcuts(ui.ctx());
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| app.canvas(ui));
        });
    }

    fn key(k: egui::Key) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    fn mv(&mut self, app: &mut App, p: Pos2) {
        self.frame(app, vec![egui::Event::PointerMoved(p)]);
    }

    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
    }

    // press and release without moving
    fn click(&mut self, app: &mut App, p: Pos2) {
        self.mv(app, p);
        self.frame(app, vec![Self::button(p, true)]);
        self.frame(app, vec![Self::button(p, false)]);
        self.frame(app, vec![]);
    }

    // press, move to `to` in a few steps, release
    fn drag(&mut self, app: &mut App, from: Pos2, to: Pos2) {
        self.mv(app, from);
        self.frame(app, vec![Self::button(from, true)]);
        for k in 1..=4 {
            let p = from + (to - from) * (k as f32 / 4.0);
            self.frame(app, vec![egui::Event::PointerMoved(p)]);
        }
        self.frame(app, vec![Self::button(to, false)]);
        self.frame(app, vec![]);
    }
}

// world (0,0) is at screen (40,40) with the default pan
fn scr(x: f32, y: f32) -> Pos2 {
    Pos2::new(40.0 + x, 40.0 + y)
}

#[test]
fn pen_tool_through_the_canvas() {
    let mut h = Harness::new();
    let mut a = App::default();
    h.frame(&mut a, vec![]); // lays out the canvas
    a.rulers = false;
    a.snap = false;

    // click, click, click, then click on the first point: a closed triangle
    a.set_tool(crate::Tool::Pen);
    for (x, y) in [(100.0, 100.0), (300.0, 100.0), (300.0, 300.0)] {
        h.click(&mut a, scr(x, y));
    }
    assert_eq!(a.pen.len(), 3, "three points placed");
    assert!(a.shapes.is_empty(), "still drawing");
    h.click(&mut a, scr(101.0, 101.0));
    assert!(a.pen.is_empty());
    assert_eq!(a.shapes.len(), 1);
    let s = &a.shapes[0];
    assert_eq!(s.kind, Kind::Path);
    assert!(s.path[0].closed);
    assert_eq!(s.path[0].pts.len(), 3);
    assert!((s.size.x - 200.0).abs() < 1.0 && (s.size.y - 200.0).abs() < 1.0, "size {:?}", s.size);
    assert_eq!(a.tool, crate::Tool::Select, "tool resets after a finished path");

    // an open path with a dragged-out smooth point; Enter-style finish
    a.shapes.clear();
    a.sel.clear();
    a.set_tool(crate::Tool::Pen);
    h.click(&mut a, scr(100.0, 400.0));
    h.drag(&mut a, scr(250.0, 400.0), scr(300.0, 340.0)); // second point with handles
    assert_eq!(a.pen.len(), 2);
    let q = a.pen[1];
    assert!(q.hout.length() > 10.0 && (q.hin + q.hout).length() < 1e-3, "symmetric handles, got {:?}/{:?}", q.hin, q.hout);
    h.click(&mut a, scr(400.0, 400.0));
    assert_eq!(a.pen.len(), 3);
    a.finish_pen();
    assert_eq!(a.shapes.len(), 1);
    let s = &a.shapes[0];
    assert!(!s.path[0].closed);
    assert!(s.strokes.len() == 1 && s.fills.is_empty(), "open paths get a stroke, no fill");
    // the curve bulges up past the straight line between the anchors (world y 340..400)
    assert!(s.pos.y < 399.0, "box covers the curve, pos {:?}", s.pos);

    // double-click ends the path without leaving a duplicate point
    a.shapes.clear();
    a.set_tool(crate::Tool::Pen);
    h.click(&mut a, scr(100.0, 500.0));
    h.click(&mut a, scr(200.0, 500.0));
    h.t += 1.0; // let the earlier clicks age out of egui's multi-click window
    h.mv(&mut a, scr(300.0, 520.0));
    h.frame(&mut a, vec![Harness::button(scr(300.0, 520.0), true)]);
    h.frame(&mut a, vec![Harness::button(scr(300.0, 520.0), false)]);
    h.frame(&mut a, vec![Harness::button(scr(300.0, 520.0), true)]);
    h.frame(&mut a, vec![Harness::button(scr(300.0, 520.0), false)]);
    h.frame(&mut a, vec![]);
    assert_eq!(a.shapes.len(), 1, "double-click finished the path (pen has {} pts, tool {:?})", a.pen.len(), a.tool);
    assert_eq!(a.shapes[0].path[0].pts.len(), 3);

    // node editing: drag an anchor and the box follows
    let id = a.shapes[0].id;
    a.sel = vec![id];
    a.start_node_edit(0);
    let first = a.node_points(0)[0].pts[0].p;
    h.drag(&mut a, scr(first.x, first.y), scr(first.x - 60.0, first.y - 30.0));
    let moved = a.node_points(0)[0].pts[0].p;
    assert!((moved.x - (first.x - 60.0)).abs() < 2.0 && (moved.y - (first.y - 30.0)).abs() < 2.0, "{first:?} -> {moved:?}");
}


#[test]
fn every_icon_rasterizes() {
    for (g, name) in crate::glyphs::all_glyphs() {
        // Mask is the one icon painted by hand instead of from an SVG
        let Some((file, svg)) = crate::glyphs::svg_of(g) else {
            assert!(matches!(g, crate::glyphs::Glyph::Mask), "no svg for {name}");
            continue;
        };
        let img = crate::glyphs::rasterize(svg).unwrap_or_else(|| panic!("{file} failed to render"));
        assert!(img.pixels.iter().any(|p| p.a() > 0), "{file} rendered empty");
    }
}

// the component gallery (`--gallery`) draws every token and shared widget without panicking
#[test]
fn component_gallery_renders() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    for _ in 0..2 {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 1200.0))), ..Default::default() };
        let _ = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| a.gallery(ui));
        });
    }
}

// runs the whole HUD (panels, toolbar, canvas) for each kind of selection: nothing may panic
#[test]
fn hud_renders_for_every_selection() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 300.0, 200.0);
    for k in [Kind::Rect, Kind::Ellipse, Kind::Polygon, Kind::Star, Kind::Line, Kind::Text] {
        a.add(k, Pos2::new(20.0, 20.0));
        let i = a.shapes.len() - 1;
        a.shapes[i].size = Vec2::new(40.0, 30.0);
    }
    a.sel = vec![f];
    a.make_component();
    a.instantiate_next_to(f);
    a.pen_click(Pos2::new(5.0, 5.0));
    a.pen_click(Pos2::new(50.0, 5.0));
    a.finish_pen();
    a.ext.effects.push(EffectStyle { id: 900, name: "Soft".into(), effects: vec![Effect::new(EffectKind::DropShadow)] });
    a.ext.texts.push(TextStyle::from_shape(901, "Body".into(), &Shape::default()));
    a.ext.grids.push(GridStyle { id: 902, name: "Cols".into(), grids: vec![LayoutGrid::new(GridKind::Columns)] });
    for s in a.shapes.iter_mut() {
        s.effect_style = Some(900);
        s.grid_style = Some(902);
        if s.kind == Kind::Text {
            s.text_style = Some(901);
        }
    }
    a.status = "hello".into();
    a.show_help = true;
    let ids: Vec<u64> = a.shapes.iter().map(|s| s.id).collect();
    let mut selections: Vec<Vec<u64>> = vec![vec![]];
    selections.extend(ids.iter().map(|&i| vec![i]));
    selections.push(ids.clone());
    for sel in selections {
        a.sel = sel;
        for tab in [crate::Tab::Design, crate::Tab::Prototype] {
            a.tab = tab;
            for left in [crate::LeftTab::Layers, crate::LeftTab::Assets, crate::LeftTab::History, crate::LeftTab::Comments] {
                a.left_tab = left;
                for _ in 0..2 {
                    let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))), ..Default::default() };
                    let _ = ctx.run_ui(raw, |ui| {
                        egui::Panel::left("rail").show(ui, |ui| a.rail(ui));
                        egui::Panel::left("l").show(ui, |ui| a.left_panel(ui));
                        egui::Panel::right("r").show(ui, |ui| a.right_panel(ui));
                        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| a.canvas(ui));
                        a.toolbar(ui.ctx());
                    });
                }
            }
        }
    }
}

#[test]
fn auto_layout_wrap_and_grid() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 100.0, 300.0);
    for k in 0..5 {
        rect_at(&mut a, 5.0 + k as f32, 5.0);
    }
    let fi = a.index_of(f).unwrap();
    for id in a.shapes.iter().filter(|s| s.parent == Some(f)).map(|s| s.id).collect::<Vec<_>>() {
        let i = a.index_of(id).unwrap();
        a.shapes[i].size = Vec2::new(40.0, 10.0);
    }
    // wrap: 40+8+40 = 88 fits in 100-0 padding, a third would not: three lines of 2, 2, 1
    a.shapes[fi].layout = Some(AutoLayout { gap: 8.0, pad: [0.0; 4], hug_x: false, hug_y: true, wrap: true, ..AutoLayout::default() });
    a.settle();
    let kids: Vec<(f32, f32)> = a.shapes.iter().filter(|s| s.parent == Some(f)).map(|s| (s.pos.x, s.pos.y)).collect();
    assert_eq!(kids, vec![(0.0, 0.0), (48.0, 0.0), (0.0, 18.0), (48.0, 18.0), (0.0, 36.0)]);
    assert_eq!(a.shapes[fi].size.y, 46.0, "hugs the three lines");

    // grid: 2 columns of 40 with an 8 gap, hugging both ways
    a.shapes[fi].layout = Some(AutoLayout { gap: 8.0, pad: [0.0; 4], columns: 2, ..AutoLayout::default() });
    a.settle();
    let kids: Vec<(f32, f32)> = a.shapes.iter().filter(|s| s.parent == Some(f)).map(|s| (s.pos.x, s.pos.y)).collect();
    assert_eq!(kids[1], (48.0, 0.0));
    assert_eq!(kids[4], (0.0, 36.0));
    assert_eq!((a.shapes[fi].size.x, a.shapes[fi].size.y), (88.0, 46.0));
}

#[test]
fn blend_modes_reach_the_svg_and_composite() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    rect_at(&mut a, 10.0, 10.0);
    let i = a.shapes.len() - 1;
    a.shapes[i].blend = BlendMode::Multiply;
    a.shapes[i].fills = vec![Paint::solid(egui::Color32::RED)];
    let roots = vec![a.shapes[0].id];
    let mut out = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| out = Some((a.svg_for(ui.ctx(), &roots), a.svg_scene(ui.ctx(), Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)), Vec2::splat(100.0), egui::Color32::WHITE))));
    let (svg, scene) = out.unwrap();
    assert!(svg.unwrap().0.contains("mix-blend-mode:multiply"));
    let img = crate::export::rasterize_scene(&scene, 100, 100).expect("scene renders");
    // red multiplied onto the white frame stays red; the page background (white) shows elsewhere
    assert_eq!(img.pixels[15 * 100 + 15].r(), 255);
    assert_eq!(img.pixels[15 * 100 + 15].g(), 0);
}

#[test]
fn selection_colors_recolor_everything_inside() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let r = rect_at(&mut a, 10.0, 10.0);
    let r2 = rect_at(&mut a, 30.0, 10.0);
    for id in [r, r2] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].fills = vec![Paint::solid(egui::Color32::RED)];
    }
    a.sel = vec![f];
    let colors = a.selection_colors();
    assert!(colors.contains(&egui::Color32::RED) && colors.contains(&egui::Color32::WHITE));
    a.recolor(egui::Color32::RED, egui::Color32::GREEN);
    assert!(a.shapes.iter().filter(|s| s.kind == Kind::Rect).all(|s| s.fills == vec![Paint::solid(egui::Color32::GREEN)]));
}


#[test]
fn present_transitions() {
    use crate::canvas::transition_frame;
    let size = Vec2::new(400.0, 300.0);
    // push left: the new frame enters from the right, the old one leaves to the left
    let start = transition_frame(Transition::SlideLeft, Easing::EaseOut, 0.0, size);
    assert_eq!((start.from_offset, start.to_offset), (Vec2::ZERO, Vec2::new(400.0, 0.0)));
    let end = transition_frame(Transition::SlideLeft, Easing::EaseOut, 1.0, size);
    assert_eq!((end.from_offset, end.to_offset), (Vec2::new(-400.0, 0.0), Vec2::ZERO));
    let up = transition_frame(Transition::SlideUp, Easing::EaseOut, 0.0, size);
    assert_eq!(up.to_offset, Vec2::new(0.0, 300.0));
    // eased: past halfway in distance at half time, and monotonic
    let mid = transition_frame(Transition::SlideRight, Easing::EaseOut, 0.5, size);
    assert!(mid.from_offset.x > 200.0 && mid.from_offset.x < 400.0);
    assert!(transition_frame(Transition::SlideRight, Easing::EaseOut, 0.6, size).from_offset.x > mid.from_offset.x);
    // dissolve cross-fades in place; out-of-range time is clamped
    let d = transition_frame(Transition::Dissolve, Easing::EaseOut, 0.5, size);
    assert!((d.from_alpha + d.to_alpha - 1.0).abs() < 1e-6 && d.to_offset == Vec2::ZERO);
    assert_eq!(transition_frame(Transition::Dissolve, Easing::EaseOut, 7.0, size).to_alpha, 1.0);

    // clicking a linked button inside frame A animates to B with the button's transition
    let mut a = App::default();
    let fa = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let fb = frame(&mut a, 300.0, 0.0, 100.0, 100.0);
    let btn = rect_at(&mut a, 10.0, 10.0);
    let i = a.index_of(btn).unwrap();
    assert_eq!(a.shapes[i].parent, Some(fa));
    a.shapes[i].link = Some(fb);
    a.shapes[i].transition = Transition::SlideLeft;
    a.shapes[i].duration = 0.4;
    a.start_present(fa);
    a.present_click(Pos2::new(15.0, 15.0), 2.0);
    assert_eq!(a.present, Some(fb));
    let anim = a.present_anim.unwrap();
    assert_eq!((anim.from, anim.tr, anim.dur, anim.t0), (fa, Transition::SlideLeft, 0.4, 2.0));
    // Instant links switch without an animation
    a.present = Some(fa);
    let i = a.index_of(btn).unwrap();
    a.shapes[i].transition = Transition::Instant;
    a.present_click(Pos2::new(15.0, 15.0), 3.0);
    assert!(a.present_anim.is_none() && a.present == Some(fb));
}

#[test]
fn dev_mode_css_and_redlines() {
    use crate::inspect::{redlines, shape_css_in};
    let mut a = App::default();
    let f = frame(&mut a, 80.0, 80.0, 400.0, 300.0);
    let r = rect_at(&mut a, 120.0, 120.0);
    let i = a.index_of(r).unwrap();
    {
        let s = &mut a.shapes[i];
        s.size = Vec2::new(120.0, 80.0);
        s.fills = vec![Paint::solid(egui::Color32::from_rgb(0x0d, 0x99, 0xff))];
        s.strokes = vec![Paint::solid(egui::Color32::BLACK)];
        s.stroke_w = 2.0;
        s.radius = [8.0; 4];
        s.opacity = 0.5;
        s.rotation = 15.0;
        s.blend = BlendMode::Multiply;
    }
    assert_eq!(a.shapes[i].parent, Some(f));
    let css = a.css_of(i);
    for want in [
        "left: 120px;", "top: 120px;", "width: 120px;", "height: 80px;", "background: #0D99FF;", "border: 2px solid #000000;",
        "border-radius: 8px;", "opacity: 0.5;", "mix-blend-mode: multiply;", "transform: rotate(15deg);",
    ] {
        assert!(css.contains(want), "missing {want} in\n{css}");
    }
    // stacked paints: top layer first, solids as flat gradients; a gradient's angle turns into CSS degrees
    let mut s = a.shapes[i].clone();
    s.fills.push(Paint { kind: PaintKind::Linear { a: egui::Color32::WHITE, b: egui::Color32::BLACK, angle: 0.0 }, visible: true, opacity: 1.0, style: None, var: None, image: Default::default(), video: None, blend: BlendMode::Normal });
    let css = shape_css_in(&s, Pos2::ZERO, &[], None);
    assert!(css.contains("background: linear-gradient(90deg, #FFFFFF, #000000), linear-gradient(#0D99FF, #0D99FF);"), "{css}");

    // redlines: gap to a shape on the right, insets inside a container
    let sel = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
    let gap = redlines(sel, Rect::from_min_size(Pos2::new(30.0, 2.0), Vec2::new(10.0, 4.0)));
    assert_eq!(gap.len(), 1);
    assert_eq!(gap[0].2, 20.0);
    assert_eq!(gap[0].0.y, 4.0, "drawn at the middle of the vertical overlap");
    let ins = redlines(sel, Rect::from_min_size(Pos2::new(-5.0, -3.0), Vec2::new(30.0, 20.0)));
    let mut d: Vec<f32> = ins.iter().map(|l| l.2).collect();
    d.sort_by(f32::total_cmp);
    assert_eq!(d, vec![3.0, 5.0, 7.0, 15.0]);
}

#[test]
fn pencil_and_dev_mode_through_the_canvas() {
    use crate::vector::{rdp, smooth};
    // a straight run collapses to its ends; a corner survives
    let line: Vec<Pos2> = (0..20).map(|k| Pos2::new(k as f32, 0.01 * (k % 2) as f32)).collect();
    assert_eq!(rdp(&line, 0.5).len(), 2);
    let corner = [Pos2::new(0.0, 0.0), Pos2::new(5.0, 0.0), Pos2::new(10.0, 0.0), Pos2::new(10.0, 5.0), Pos2::new(10.0, 10.0)];
    assert_eq!(rdp(&corner, 0.5), vec![Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0), Pos2::new(10.0, 10.0)]);
    let sm = smooth(&[Pos2::new(0.0, 0.0), Pos2::new(6.0, 6.0), Pos2::new(12.0, 0.0)]);
    assert_eq!(sm[1].hout, Vec2::new(2.0, 0.0));
    assert_eq!(sm[1].hin, -sm[1].hout);

    let mut h = Harness::new();
    let mut a = App::default();
    h.frame(&mut a, vec![]);
    a.rulers = false;
    a.snap = false;
    a.set_mode(crate::Mode::Draw);
    assert_eq!(a.tool, crate::Tool::Pencil);
    // an arc-ish stroke
    h.mv(&mut a, scr(100.0, 100.0));
    h.frame(&mut a, vec![Harness::button(scr(100.0, 100.0), true)]);
    for k in 1..=12 {
        let t = k as f32 / 12.0;
        h.frame(&mut a, vec![egui::Event::PointerMoved(scr(100.0 + 200.0 * t, 100.0 + 120.0 * (t * std::f32::consts::PI).sin()))]);
    }
    h.frame(&mut a, vec![Harness::button(scr(300.0, 100.0), false)]);
    h.frame(&mut a, vec![]);
    assert_eq!(a.shapes.len(), 1, "one path per stroke");
    let s = &a.shapes[0];
    assert_eq!(s.kind, Kind::Path);
    assert!(!s.path[0].closed && s.path[0].pts.len() >= 3, "{} points", s.path[0].pts.len());
    assert!(s.strokes.len() == 1 && s.fills.is_empty());
    assert_eq!(a.tool, crate::Tool::Pencil, "the pencil stays active");

    // Dev mode: dragging a shape selects it but moves nothing; delete does nothing
    let id = a.shapes[0].id;
    let before = a.shapes[0].pos;
    a.set_mode(crate::Mode::Dev);
    a.sel.clear();
    h.drag(&mut a, scr(200.0, 220.0), scr(260.0, 260.0));
    assert_eq!(a.shapes[0].pos, before);
    assert_eq!(a.sel, vec![id], "the press selected the stroke");
    h.keys(&mut a, vec![Harness::key(egui::Key::Delete), Harness::key(egui::Key::ArrowRight), Harness::key(egui::Key::R)]);
    assert_eq!(a.shapes.len(), 1, "Delete is ignored");
    assert_eq!(a.shapes[0].pos, before, "nudging is ignored");
    assert_eq!(a.tool, crate::Tool::Select, "tool keys are ignored");
    // the same keys work again in Design mode
    a.set_mode(crate::Mode::Design);
    a.sel = vec![id];
    h.keys(&mut a, vec![Harness::key(egui::Key::Delete)]);
    assert!(a.shapes.is_empty());
}

#[test]
fn dev_mode_auto_layout_css_spacing() {
    use crate::inspect::spacing;
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 300.0, 100.0);
    let r1 = rect_at(&mut a, 10.0, 10.0);
    let r2 = rect_at(&mut a, 60.0, 10.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].layout = Some(AutoLayout { gap: 12.0, pad: [16.0, 8.0, 16.0, 8.0], main: Align::Center, cross: Align::Center, hug_x: false, hug_y: true, ..AutoLayout::default() });
    for id in [r1, r2] {
        let i = a.index_of(id).unwrap();
        assert_eq!(a.shapes[i].parent, Some(f));
        a.shapes[i].size = Vec2::new(40.0, 30.0);
    }
    let i2 = a.index_of(r2).unwrap();
    a.shapes[i2].fill_x = true;
    a.settle();
    let css = a.css_of(fi);
    for want in ["display: flex;", "flex-direction: row;", "padding: 8px 16px;", "justify-content: center;", "align-items: center;", "gap: 12px;", "width: 300px;"] {
        assert!(css.contains(want), "missing {want} in\n{css}");
    }
    assert!(!css.contains("height:"), "a hugging height is left to the layout:\n{css}");
    // children are flex items, not absolutely placed; Fill grows
    let kid = a.css_of(a.index_of(r1).unwrap());
    assert!(kid.contains("flex-shrink: 0;") && !kid.contains("position: absolute") && kid.contains("width: 40px;"), "{kid}");
    let grow = a.css_of(a.index_of(r2).unwrap());
    assert!(grow.contains("flex: 1 0 0;") && !grow.contains("width:"), "{grow}");
    // grid and wrap
    a.shapes[fi].layout.as_mut().unwrap().columns = 3;
    let grid = a.css_of(fi);
    assert!(grid.contains("display: grid;") && grid.contains("grid-template-columns: repeat(3, minmax(0, 1fr));"), "{grid}");
    let lay = a.shapes[fi].layout.as_mut().unwrap();
    lay.columns = 0;
    lay.wrap = true;
    lay.main = Align::SpaceBetween;
    let wrap = a.css_of(fi);
    assert!(wrap.contains("flex-wrap: wrap;") && wrap.contains("justify-content: space-between;") && !wrap.contains("gap:"), "{wrap}");

    // spacing overlay: four padding bands and the gap between two children
    let kids = [Rect::from_min_size(Pos2::new(16.0, 8.0), Vec2::new(40.0, 30.0)), Rect::from_min_size(Pos2::new(68.0, 8.0), Vec2::new(40.0, 30.0))];
    let sp = spacing(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 46.0)), [16.0, 8.0, 16.0, 8.0], &kids);
    let vals: Vec<f32> = sp.iter().map(|z| z.1).collect();
    assert_eq!(vals, vec![16.0, 8.0, 16.0, 8.0, 12.0]);
    assert_eq!(sp[4].0, Rect::from_min_max(Pos2::new(56.0, 8.0), Pos2::new(68.0, 38.0)));
    // a grid gets gaps on both axes
    let cells = [(0.0, 0.0), (50.0, 0.0), (0.0, 40.0), (50.0, 40.0)].map(|(x, y)| Rect::from_min_size(Pos2::new(x, y), Vec2::new(40.0, 30.0)));
    let g = spacing(Rect::from_min_size(Pos2::ZERO, Vec2::new(90.0, 70.0)), [0.0; 4], &cells);
    assert_eq!(g.len(), 4, "{g:?}");
    assert!(g.iter().all(|z| z.1 == 10.0));
}

#[test]
fn easing_and_smart_animate() {
    use crate::canvas::smart_tween;
    for (e, _) in Easing::ALL {
        assert!(e.at(0.0).abs() < 1e-4 && (e.at(1.0) - 1.0).abs() < 1e-4, "{e:?} must start at 0 and end at 1");
    }
    assert_eq!(Easing::Linear.at(0.25), 0.25);
    assert!(Easing::EaseIn.at(0.5) < 0.5 && Easing::EaseOut.at(0.5) > 0.5);
    assert!((Easing::EaseInOut.at(0.5) - 0.5).abs() < 1e-6);
    assert!(Easing::EaseOutBack.at(0.8) > 1.0, "back overshoots");
    assert!(Easing::EaseInBack.at(0.2) < 0.0, "back pulls back first");
    assert!((0..100).any(|k| Easing::Bouncy.at(k as f32 / 100.0) > 1.0), "a spring overshoots");
    // Figma's spring presets: Bouncy spring overshoots most, Slow spring (overdamped) never does, all settle by the end
    let peak = |e: Easing| (0..=100).map(|k| e.at(k as f32 / 100.0)).fold(0.0f32, f32::max);
    assert!(peak(Easing::SpringBouncy) > peak(Easing::SpringGentle) && peak(Easing::SpringGentle) > 1.0, "{} {}", peak(Easing::SpringBouncy), peak(Easing::SpringGentle));
    assert!(peak(Easing::SpringSlow) <= 1.0001, "an overdamped spring does not overshoot");
    assert!((Easing::SpringQuick.at(0.97) - 1.0).abs() < 0.02, "settled before the end");

    // two frames with a "Card" that moves and grows, a "Badge" only in A and a "Tag" only in B
    let mut a = App::default();
    let fa = frame(&mut a, 0.0, 0.0, 200.0, 200.0);
    let fb = frame(&mut a, 400.0, 0.0, 200.0, 200.0);
    let card_a = rect_at(&mut a, 10.0, 10.0);
    let badge = rect_at(&mut a, 100.0, 150.0);
    let card_b = rect_at(&mut a, 500.0, 100.0);
    let tag = rect_at(&mut a, 420.0, 150.0);
    let named = |a: &mut App, id: u64, name: &str, size: f32, color: egui::Color32| {
        let i = a.index_of(id).unwrap();
        a.shapes[i].name = name.into();
        a.shapes[i].size = Vec2::splat(size);
        a.shapes[i].fills = vec![Paint::solid(color)];
    };
    named(&mut a, card_a, "Card", 20.0, egui::Color32::from_rgb(0, 0, 0));
    named(&mut a, card_b, "Card", 60.0, egui::Color32::from_rgb(200, 100, 0));
    named(&mut a, badge, "Badge", 10.0, egui::Color32::WHITE);
    named(&mut a, tag, "Tag", 10.0, egui::Color32::WHITE);
    let ib = a.index_of(card_b).unwrap();
    assert_eq!((a.shapes[ib].parent, a.shapes[ib].pos), (Some(fb), Pos2::new(100.0, 100.0)));

    let half = smart_tween(&a.shapes, fa, fb, 0.5);
    let c = &half[ib];
    assert_eq!((c.pos, c.size), (Pos2::new(55.0, 55.0), Vec2::splat(40.0)), "halfway between the two cards");
    assert_eq!(c.fills[0].kind, PaintKind::Solid(egui::Color32::from_rgb(100, 50, 0)));
    let it = a.index_of(tag).unwrap();
    assert_eq!(half[it].opacity, 0.5, "new layers fade in");
    let ig = a.index_of(badge).unwrap();
    assert_eq!((half[ig].parent, half[ig].opacity), (Some(fb), 0.5), "old layers move along and fade out");
    // the ends are the frames themselves; the original page is untouched
    let end = smart_tween(&a.shapes, fa, fb, 1.0);
    assert_eq!((end[ib].pos, end[ib].size, end[ig].opacity), (Pos2::new(100.0, 100.0), Vec2::splat(60.0), 0.0));
    assert_eq!(smart_tween(&a.shapes, fa, fb, 0.0)[ib].pos, Pos2::new(10.0, 10.0));
    assert_eq!(a.shapes[ig].parent, Some(fa));

    // a Smart Animate link carries its easing into the animation
    let i = a.index_of(card_a).unwrap();
    a.shapes[i].link = Some(fb);
    a.shapes[i].transition = Transition::SmartAnimate;
    a.shapes[i].easing = Easing::EaseInOutBack;
    a.start_present(fa);
    a.present_click(Pos2::new(15.0, 15.0), 1.0);
    let anim = a.present_anim.unwrap();
    assert_eq!((a.present, anim.tr, anim.ease), (Some(fb), Transition::SmartAnimate, Easing::EaseInOutBack));
    // older files without an easing load with the default
    let s: Shape = serde_json::from_str(r#"{"id": 1, "transition": "Dissolve"}"#).unwrap();
    assert_eq!(s.easing, Easing::EaseOut);
}

#[test]
fn brush_width_and_pressure() {
    use crate::vector::brush_outline;
    // a straight stroke: the outline is as wide as the width times the pressure at each point
    let pts: Vec<Pos2> = (0..=4).map(|k| Pos2::new(k as f32 * 10.0, 0.0)).collect();
    let out = brush_outline(&pts, &[1.0, 1.0, 0.5, 1.0, 1.0], 8.0);
    assert_eq!(out.len(), 5 + 5 + 5 + 5, "two sides and two round caps");
    assert_eq!((out[0], out[2]), (Pos2::new(0.0, 4.0), Pos2::new(20.0, 2.0)));
    assert!(out.iter().all(|p| p.y.abs() <= 4.0 + 1e-4 && p.x >= -4.0 - 1e-4 && p.x <= 44.0 + 1e-4));
    assert!(out.iter().any(|p| p.x > 43.9), "the end cap reaches past the last point");

    let mut h = Harness::new();
    let mut a = App::default();
    h.frame(&mut a, vec![]);
    a.rulers = false;
    a.snap = false;
    a.set_mode(crate::Mode::Draw);
    let stroke = |h: &mut Harness, a: &mut App, y: f32| {
        h.mv(a, scr(100.0, y));
        h.frame(a, vec![Harness::button(scr(100.0, y), true)]);
        for k in 1..=12 {
            let t = k as f32 / 12.0;
            h.frame(a, vec![egui::Event::PointerMoved(scr(100.0 + 200.0 * t, y + 40.0 * (t * std::f32::consts::PI).sin()))]);
        }
        h.frame(a, vec![Harness::button(scr(300.0, y), false)]);
        h.frame(a, vec![]);
    };
    // the width chosen in the Draw toolbar is used for new strokes
    a.brush_w = 8.0;
    stroke(&mut h, &mut a, 100.0);
    assert_eq!((a.shapes.len(), a.shapes[0].stroke_w), (1, 8.0));
    // with pressure, a stroke is a filled outline (a mouse stands in with its speed)
    a.brush = crate::vector::Brush::Pressure;
    stroke(&mut h, &mut a, 250.0);
    assert_eq!(a.shapes.len(), 2);
    let b = &a.shapes[1];
    assert!(b.name.starts_with("Pressure") && b.path[0].closed && b.strokes.is_empty() && !b.fills.is_empty(), "{b:?}");
    assert!(b.size.y >= 40.0 && b.size.y <= 40.0 + 8.0 + 1.0, "about as tall as the stroke plus its width: {}", b.size.y);
    assert_eq!(a.tool, crate::Tool::Pencil);
}

#[test]
fn motion_keyframes() {
    use crate::anim::{commit, evaluate, play_time, value_at};
    let k = |t: f32, x: f32, ease: Easing| Key { t, v: [x, 0.0, 0.0, 0.0], ease };
    let keys = vec![k(0.0, 100.0, Easing::Linear), k(1000.0, 300.0, Easing::EaseIn), k(2000.0, 0.0, Easing::Linear)];
    assert_eq!(value_at(&keys, -5.0).unwrap()[0], 100.0, "held before the first keyframe");
    assert_eq!(value_at(&keys, 500.0).unwrap()[0], 200.0, "linear from the first");
    assert!(value_at(&keys, 1500.0).unwrap()[0] > 150.0, "ease in starts slowly on the way down");
    assert_eq!(value_at(&keys, 9000.0).unwrap()[0], 0.0, "held after the last");
    assert_eq!(play_time(1500.0, 700.0, 2000.0, true), (200.0, true));
    assert_eq!(play_time(1500.0, 700.0, 2000.0, false), (2000.0, false));

    // a rectangle whose X is animated 100 -> 300 over the first second
    let mut a = App::default();
    let id = rect_at(&mut a, 100.0, 100.0);
    let i = a.index_of(id).unwrap();
    a.shapes[i].size = Vec2::new(40.0, 40.0);
    a.shapes[i].tracks.push(Track { prop: Prop::X, keys: vec![k(0.0, 100.0, Easing::Linear), k(1000.0, 300.0, Easing::Linear)] });
    let shown = evaluate(&a.shapes, 1000.0);
    assert_eq!(shown[i].pos.x, 300.0);

    // edits at the playhead: an animated value becomes a keyframe, the base stays; others are plain edits
    let mut after = shown.clone();
    after[i].pos.x = 320.0;
    after[i].pos.y = 150.0;
    let out = commit(&a.shapes, &shown, after.clone(), 1000.0, false);
    assert_eq!((out[i].pos.x, out[i].pos.y), (100.0, 150.0));
    assert_eq!(out[i].tracks[0].keys[1].v[0], 320.0);
    assert_eq!(out[i].tracks.len(), 1);
    // with auto-keyframing an edit of an unanimated value starts a track (old value at 0, new at the playhead)
    let out = commit(&a.shapes, &shown, after, 1000.0, true);
    let y = out[i].tracks.iter().find(|t| t.prop == Prop::Y).unwrap();
    assert_eq!(y.keys.iter().map(|k| (k.t, k.v[0])).collect::<Vec<_>>(), vec![(0.0, 100.0), (1000.0, 150.0)]);
    assert_eq!(out[i].pos.y, 100.0, "the base keeps the old value");

    // through the canvas: in Motion mode at 1 s the rectangle is drawn at x 300, and dragging it moves the keyframe
    let mut h = Harness::new();
    h.frame(&mut a, vec![]);
    a.rulers = false;
    a.snap = false;
    a.set_mode(crate::Mode::Motion);
    a.anim_time = 1000.0;
    h.drag(&mut a, scr(320.0, 120.0), scr(370.0, 120.0));
    let i = a.index_of(id).unwrap();
    assert_eq!(a.sel, vec![id], "the press found the shape where it is shown");
    assert_eq!(a.shapes[i].pos.x, 100.0, "the base position is unchanged");
    assert_eq!(a.shapes[i].tracks[0].keys[1].v[0], 350.0, "the keyframe at the playhead moved");
    assert_eq!(evaluate(&a.shapes, 500.0)[i].pos.x, 225.0);

    // fill color and opacity animate too; tracks survive a save
    let i = a.index_of(id).unwrap();
    a.shapes[i].fills = vec![Paint::solid(egui::Color32::BLACK)];
    a.shapes[i].tracks.push(Track { prop: Prop::Fill, keys: vec![Key { t: 0.0, v: [0.0, 0.0, 0.0, 255.0], ease: Easing::Linear }, Key { t: 1000.0, v: [200.0, 100.0, 0.0, 255.0], ease: Easing::Linear }] });
    assert_eq!(evaluate(&a.shapes, 500.0)[i].fills[0].kind, PaintKind::Solid(egui::Color32::from_rgb(100, 50, 0)));
    let json = serde_json::to_string(&a.shapes[i]).unwrap();
    let back: Shape = serde_json::from_str(&json).unwrap();
    assert_eq!(back.tracks, a.shapes[i].tracks);
    assert!(!serde_json::to_string(&Shape::default()).unwrap().contains("tracks"), "no tracks, nothing written");
}

#[test]
fn smart_animate_morphs_paths_and_gradients() {
    use crate::canvas::smart_tween;
    use crate::vector::morph;
    let pt = |x: f32, y: f32| PathPt::corner(Pos2::new(x, y));
    let tri = Contour { pts: vec![pt(0.5, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)], closed: true, hole: false };
    let quad = Contour { pts: vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)], closed: true, hole: false };
    // the triangle gets a point (on an edge, so it still looks the same) to match the square
    let start = morph(&[tri.clone()], &[quad.clone()], 0.0).unwrap();
    assert_eq!(start[0].pts.len(), 4);
    let end = morph(&[tri], &[quad.clone()], 1.0).unwrap();
    assert_eq!(end[0].pts.iter().map(|p| p.p).collect::<Vec<_>>(), quad.pts.iter().map(|p| p.p).collect::<Vec<_>>());
    assert!(morph(&[quad.clone()], &[quad.clone(), quad.clone()], 0.5).is_none(), "different contour counts do not morph");

    // two frames with a "Blob" path and a gradient "Bg"
    let mut a = App::default();
    let fa = frame(&mut a, 0.0, 0.0, 200.0, 200.0);
    let fb = frame(&mut a, 400.0, 0.0, 200.0, 200.0);
    let mk = |a: &mut App, x: f32, name: &str, path: Vec<Contour>| {
        let id = rect_at(a, x, 10.0);
        let i = a.index_of(id).unwrap();
        let s = &mut a.shapes[i];
        s.name = name.into();
        s.kind = Kind::Path;
        s.size = Vec2::splat(50.0);
        s.path = path;
        id
    };
    let tri = Contour { pts: vec![pt(0.5, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)], closed: true, hole: false };
    mk(&mut a, 10.0, "Blob", vec![tri]);
    let blob_b = mk(&mut a, 410.0, "Blob", vec![quad]);
    let bg_a = rect_at(&mut a, 10.0, 100.0);
    let bg_b = rect_at(&mut a, 410.0, 100.0);
    for (id, paint) in [
        (bg_a, Paint::solid(egui::Color32::from_rgb(0, 0, 0))),
        (bg_b, Paint { kind: PaintKind::Linear { a: egui::Color32::from_rgb(200, 0, 0), b: egui::Color32::from_rgb(0, 0, 200), angle: 90.0 }, ..Paint::default() }),
    ] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].name = "Bg".into();
        a.shapes[i].fills = vec![paint];
    }
    let half = smart_tween(&a.shapes, fa, fb, 0.5);
    let ib = a.index_of(blob_b).unwrap();
    assert_eq!(half[ib].path[0].pts.len(), 4);
    assert_eq!(half[ib].path[0].pts[0].p, Pos2::new(0.25, 0.0), "halfway between the apex and the corner");
    let ig = a.index_of(bg_b).unwrap();
    assert_eq!(half[ig].fills[0].kind, PaintKind::Gradient(Gradient { angle: 90.0, ..Gradient::between(egui::Color32::from_rgb(100, 0, 0), egui::Color32::from_rgb(0, 0, 100)) }), "a solid blends into a gradient");
}


#[test]
fn brush_presets() {
    use crate::vector::{brush_widths, scatter, Brush};
    let pts: Vec<Pos2> = (0..=20).map(|k| Pos2::new(k as f32 * 5.0, 0.0)).collect();
    let force = vec![1.0; pts.len()];
    let taper = brush_widths(Brush::Taper, &pts, &force, 0.0);
    assert!(taper[0] < 0.1 && taper[20] < 0.1 && taper[10] == 1.0, "{taper:?}");
    // a horizontal stroke with a horizontal nib is thin; with an upright nib it is full width
    assert!(brush_widths(Brush::Calligraphy, &pts, &force, 0.0)[10] <= 0.15 + 1e-6);
    assert!((brush_widths(Brush::Calligraphy, &pts, &force, 90.0)[10] - 1.0).abs() < 1e-5);
    let dots = scatter(&pts, &force, 8.0);
    assert!(dots.len() >= 10 && dots.iter().all(|c| c.closed && c.pts.len() == 4), "{} dots", dots.len());
    assert_eq!(scatter(&pts, &force, 8.0), dots, "the same stroke scatters the same way");

    // through the canvas: a Taper stroke and a Scatter stroke
    let mut h = Harness::new();
    let mut a = App::default();
    h.frame(&mut a, vec![]);
    a.rulers = false;
    a.snap = false;
    a.set_mode(crate::Mode::Draw);
    a.brush_w = 10.0;
    for (n, b) in [Brush::Taper, Brush::Scatter, Brush::Calligraphy].into_iter().enumerate() {
        a.brush = b;
        let y = 100.0 + 120.0 * n as f32;
        h.mv(&mut a, scr(100.0, y));
        h.frame(&mut a, vec![Harness::button(scr(100.0, y), true)]);
        for k in 1..=12 {
            let t = k as f32 / 12.0;
            h.frame(&mut a, vec![egui::Event::PointerMoved(scr(100.0 + 200.0 * t, y + 40.0 * (t * std::f32::consts::PI).sin()))]);
        }
        h.frame(&mut a, vec![Harness::button(scr(300.0, y), false)]);
        h.frame(&mut a, vec![]);
    }
    assert_eq!(a.shapes.len(), 3);
    assert!(a.shapes[0].name.starts_with("Taper") && a.shapes[0].path.len() == 1 && a.shapes[0].path[0].closed);
    assert!(a.shapes[1].name.starts_with("Scatter") && a.shapes[1].path.len() > 10, "{} dots", a.shapes[1].path.len());
    assert!(a.shapes[2].name.starts_with("Calligraphy") && !a.shapes[2].fills.is_empty() && a.shapes[2].strokes.is_empty());
}


// "Resize to fit": the frame takes the bounds of its children and they stay where they were on the canvas
#[test]
fn resize_to_fit_wraps_the_children() {
    let mut a = App::default();
    let f = frame(&mut a, 100.0, 100.0, 400.0, 300.0);
    let r = rect_at(&mut a, 130.0, 140.0);
    let ri = a.index_of(r).unwrap();
    a.shapes[ri].parent = Some(f);
    a.shapes[ri].pos = Pos2::new(30.0, 40.0);
    a.shapes[ri].size = Vec2::new(50.0, 20.0);
    a.sel = vec![f];
    a.resize_to_fit();
    let fi = a.index_of(f).unwrap();
    let ri = a.index_of(r).unwrap();
    assert_eq!(a.shapes[fi].pos, Pos2::new(130.0, 140.0));
    assert_eq!(a.shapes[fi].size, Vec2::new(50.0, 20.0));
    assert_eq!(a.shapes[ri].pos, Pos2::new(0.0, 0.0));
}

// a frame rotates with its content as one piece: a child turns about the frame's centre, and turning back restores it
#[test]
fn rotating_a_frame_turns_its_children() {
    let mut a = App::default();
    let f = frame(&mut a, 100.0, 100.0, 200.0, 100.0);
    let r = rect_at(&mut a, 100.0, 100.0);
    let ri = a.index_of(r).unwrap();
    a.shapes[ri].parent = Some(f);
    a.shapes[ri].pos = Pos2::new(10.0, 20.0);
    a.shapes[ri].size = Vec2::new(20.0, 10.0);
    let fi = a.index_of(f).unwrap();
    a.set_rotation(fi, 90.0);
    let (fi, ri) = (a.index_of(f).unwrap(), a.index_of(r).unwrap());
    assert_eq!(a.shapes[fi].rotation, 90.0);
    assert_eq!(a.shapes[ri].rotation, 90.0);
    // the frame's world bounds are now 100 wide and 200 tall around the same centre (200, 150)
    let bb = a.world_rect(fi);
    assert!((bb.width() - 100.0).abs() < 1e-3 && (bb.height() - 200.0).abs() < 1e-3 && (bb.center() - Pos2::new(200.0, 150.0)).length() < 1e-3);
    // the child's centre (120, 125) relative to the frame centre (200, 150) is (-80, -25); turned 90 deg it is (25, -80)
    let c = a.world_rect(ri).center();
    assert!((c - Pos2::new(225.0, 70.0)).length() < 1e-3, "{c:?}");
    a.set_rotation(fi, 0.0);
    let (fi, ri) = (a.index_of(f).unwrap(), a.index_of(r).unwrap());
    assert!((a.shapes[ri].pos - Pos2::new(10.0, 20.0)).length() < 1e-3);
    assert!(a.shapes[ri].rotation.abs() < 1e-3 && a.shapes[fi].rotation == 0.0);
}


// Shift+A reads the layout from where the children are; anything that is not a plain frame is wrapped; Ctrl+Alt+Shift+A removes it
#[test]
fn shift_a_adds_auto_layout_like_figma() {
    let mut a = App::default();
    let mut h = Harness::new();
    let f = frame(&mut a, 0.0, 0.0, 200.0, 60.0);
    for x in [10.0, 50.0, 90.0] {
        let r = rect_at(&mut a, x, 10.0);
        let ri = a.index_of(r).unwrap();
        a.shapes[ri].parent = Some(f);
        a.shapes[ri].pos = Pos2::new(x, 10.0);
        a.shapes[ri].size = Vec2::new(20.0, 20.0);
    }
    a.sel = vec![f];
    let shift_a = |mods: egui::Modifiers| egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: mods };
    h.keys(&mut a, vec![shift_a(egui::Modifiers::SHIFT)]);
    let l = a.shapes[a.index_of(f).unwrap()].layout.clone().expect("auto layout added");
    assert!(!l.vertical);
    assert_eq!(l.gap, 20.0);
    assert_eq!(l.pad, [10.0, 10.0, 90.0, 30.0]);
    // a plain rectangle is wrapped in a new auto-layout frame
    let r = rect_at(&mut a, 300.0, 300.0);
    a.sel = vec![r];
    let before = a.shapes.len();
    h.keys(&mut a, vec![shift_a(egui::Modifiers::SHIFT)]);
    assert_eq!(a.shapes.len(), before + 1);
    let wrapper = a.single().map(|i| a.shapes[i].clone()).unwrap();
    assert!(wrapper.kind == Kind::Frame && wrapper.layout.is_some());
    assert_eq!(a.shapes[a.index_of(r).unwrap()].parent, Some(wrapper.id));
    // and Ctrl+Alt+Shift+A takes it off again
    h.keys(&mut a, vec![shift_a(egui::Modifiers::SHIFT | egui::Modifiers::COMMAND | egui::Modifiers::ALT)]);
    assert!(a.shapes[a.index_of(wrapper.id).unwrap()].layout.is_none());
}

// strokes with alignment / dashes / arrows, effects, the four gradients and image modes all reach the SVG (and so the export and
// the canvas sprites), and the old single shadow becomes an effect when a file loads
#[test]
fn svg_covers_strokes_effects_gradients_and_images() {
    use crate::model::*;
    let ctx = egui::Context::default();
    let mut a = App::default();
    let solid = |c: egui::Color32| vec![Paint::solid(c)];
    let mut svg_of = |a: &mut App, s: Shape| -> String {
        a.shapes = vec![s];
        let roots = vec![a.shapes[0].id];
        let mut out = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| out = a.svg_for(ui.ctx(), &roots));
        out.unwrap().0
    };
    let base = |k: Kind| Shape { id: 1, kind: k, size: Vec2::new(100.0, 60.0), ..Shape::default() };
    // stroke alignment: inside clips, outside masks, centre does neither; dashes and joins are attributes
    let mut s = base(Kind::Rect);
    s.strokes = solid(egui::Color32::BLACK);
    s.stroke_w = 4.0;
    s.stroke_align = StrokeAlign::Inside;
    s.dash_pattern = vec![6.0, 3.0];
    s.stroke_join = StrokeJoin::Round;
    let svg = svg_of(&mut a, s.clone());
    assert!(svg.contains("clip-path") && svg.contains("stroke-dasharray=\"6.00 3.00\"") && svg.contains("stroke-linejoin=\"round\"") && svg.contains("stroke-width=\"8\""), "{svg}");
    s.stroke_align = StrokeAlign::Outside;
    assert!(svg_of(&mut a, s.clone()).contains("<mask"));
    s.stroke_align = StrokeAlign::Center;
    let svg = svg_of(&mut a, s.clone());
    assert!(!svg.contains("<mask") && svg.contains("stroke-width=\"4\""));
    // individual sides make a ring
    s.stroke_sides = Some([1.0, 2.0, 3.0, 4.0]);
    assert!(svg_of(&mut a, s.clone()).contains("fill-rule=\"evenodd\""));
    // arrow heads on an open line
    let mut l = base(Kind::Line);
    l.fills.clear();
    l.strokes = solid(egui::Color32::BLACK);
    l.stroke_cap = [StrokeCap::ArrowLines, StrokeCap::TriangleArrow];
    let svg = svg_of(&mut a, l);
    assert_eq!(svg.matches("<path").count(), 3, "the line and two arrow heads: {svg}");
    let mut l2 = base(Kind::Line);
    l2.fills.clear();
    l2.strokes = solid(egui::Color32::BLACK);
    for cap in [StrokeCap::ReverseTriangle, StrokeCap::CircleArrow, StrokeCap::DiamondArrow] {
        l2.stroke_cap = [cap, StrokeCap::None];
        assert_eq!(svg_of(&mut a, l2.clone()).matches("<path").count(), 2, "{cap:?} draws a head");
    }
    // effects: spread uses a morphology filter, an inner shadow clips to the layer, a blur wraps the layer
    let mut e = base(Kind::Rect);
    e.effects = vec![
        Effect { spread: 3.0, ..Effect::new(EffectKind::DropShadow) },
        Effect::new(EffectKind::InnerShadow),
        Effect::new(EffectKind::LayerBlur),
    ];
    let svg = svg_of(&mut a, e.clone());
    assert!(svg.contains("feMorphology") && svg.contains("operator=\"in\"") && svg.contains("feGaussianBlur") && svg.contains("<mask"), "{svg}");
    assert!(e.needs_sprite() && !base(Kind::Rect).needs_sprite());
    // gradients
    let grad = |kind: GradientKind| {
        let mut g = Gradient::between(egui::Color32::WHITE, egui::Color32::BLACK);
        g.kind = kind;
        g.stops.insert(1, GradStop { pos: 0.5, color: egui::Color32::RED });
        let mut s = base(Kind::Rect);
        s.fills = vec![Paint { kind: PaintKind::Gradient(g), ..Paint::default() }];
        s
    };
    assert!(svg_of(&mut a, grad(GradientKind::Radial)).contains("radialGradient"));
    assert_eq!(svg_of(&mut a, grad(GradientKind::Diamond)).matches("<linearGradient").count(), 4);
    assert!(svg_of(&mut a, grad(GradientKind::Angular)).matches("<path").count() >= 360);
    assert!(!grad(GradientKind::Linear).needs_sprite() == false, "three stops need a sprite");
    // gradient maths: the ends and the middle of a three-stop gradient
    let g = match grad(GradientKind::Linear).fills[0].kind.clone() { PaintKind::Gradient(g) => g, _ => unreachable!() };
    assert_eq!(g.at(0.0), egui::Color32::WHITE);
    assert_eq!(g.at(0.5), egui::Color32::RED);
    assert_eq!(g.at(1.0), egui::Color32::BLACK);
    // images: fill covers, fit contains, tile repeats; adjustments become a filter
    let png = {
        let mut buf = std::io::Cursor::new(vec![]);
        image::RgbaImage::from_pixel(8, 4, image::Rgba([255, 0, 0, 255])).write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    };
    let id = a.register_image("red", png).unwrap();
    let img = |mode: ImageMode, adj: f32| {
        let mut s = base(Kind::Rect);
        s.fills = vec![Paint { kind: PaintKind::Image(id), image: ImageFill { mode, exposure: adj, rotation: 90, ..ImageFill::default() }, ..Paint::default() }];
        s
    };
    let svg = svg_of(&mut a, img(ImageMode::Fill, 0.0));
    assert!(svg.contains("xMidYMid slice") && svg.contains("rotate(90"));
    assert!(svg_of(&mut a, img(ImageMode::Fit, 0.0)).contains("xMidYMid meet"));
    assert!(svg_of(&mut a, img(ImageMode::Tile, 0.0)).contains("<pattern"));
    assert!(svg_of(&mut a, img(ImageMode::Fill, 0.5)).contains("feColorMatrix"));
    assert!(img(ImageMode::Fill, 0.0).needs_sprite());
    // a layer's sprite rasterises: the pixels under the fill are opaque
    let mut r = base(Kind::Rect);
    r.fills = solid(egui::Color32::from_rgb(255, 0, 0));
    r.effects = vec![Effect::new(EffectKind::InnerShadow)];
    let region = r.aabb().expand(r.paint_pad() + 2.0);
    let image = a.sprite_image(&ctx, &r, region, 120, 80).unwrap();
    assert!(image.pixels.iter().any(|p| p.a() == 255));
    // an old file's single shadow becomes the first effect
    let json = r#"[{"id":1,"name":"A","kind":"Rect","size":{"x":10.0,"y":10.0},"shadow":{"dx":1.0,"dy":2.0,"blur":3.0,"color":[0,0,0,255]}}]"#;
    let shapes = migrate(serde_json::from_str::<Vec<Shape>>(json).unwrap());
    assert_eq!(shapes[0].effects.len(), 1);
    assert!(shapes[0].shadow.is_none() && shapes[0].effects[0].dy == 2.0 && shapes[0].effects[0].kind == EffectKind::DropShadow);
}

// whole frames: keyboard edits are undoable, undo / redo are not edits themselves, and a button that kept focus after a click
// does not swallow the shortcuts (only a text field does)
#[test]
fn keyboard_edits_undo_and_shortcuts_survive_focus() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let r1 = rect_at(&mut a, 10.0, 10.0);
    let r2 = rect_at(&mut a, 60.0, 10.0);
    a.sel = vec![r1, r2];
    let mut t = 0.0;
    let key = |k: egui::Key, mods: egui::Modifiers| egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: mods };
    let mut run = |a: &mut App, events: Vec<egui::Event>, focus_a_button: bool| {
        t += 0.1;
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))), time: Some(t), events, ..Default::default() };
        let _ = ctx.run_ui(raw, |ui| {
            if focus_a_button {
                // a panel button that was clicked earlier and still has focus
                let id = egui::Id::new("clicked-button");
                let resp = ui.interact(Rect::from_min_size(Pos2::new(900.0, 700.0), Vec2::splat(20.0)), id, egui::Sense::click());
                resp.request_focus();
            }
            a.run_frame(ui);
        });
    };
    run(&mut a, vec![], true);
    // Shift+A wraps the two shapes in an auto-layout frame, even though a button has focus
    let shift = egui::Modifiers::SHIFT;
    run(&mut a, vec![key(egui::Key::A, shift)], true);
    assert_eq!(a.shapes.len(), 3, "a frame was added");
    let frame_id = a.single().map(|i| a.shapes[i].id).expect("the new frame is selected");
    assert!(a.shapes[a.index_of(frame_id).unwrap()].layout.is_some());
    // undo takes it away, redo brings it back; neither adds undo steps of its own
    let steps = a.undo.len();
    assert!(steps >= 1, "the keyboard edit is an undo step");
    run(&mut a, vec![key(egui::Key::Z, egui::Modifiers::COMMAND)], false);
    assert_eq!(a.shapes.len(), 2, "undone");
    assert_eq!(a.undo.len(), steps - 1);
    run(&mut a, vec![], false);
    assert_eq!(a.undo.len(), steps - 1, "undo did not record itself");
    run(&mut a, vec![key(egui::Key::Z, egui::Modifiers::COMMAND | shift)], false);
    assert_eq!(a.shapes.len(), 3, "redone");
    assert_eq!(a.undo.len(), steps);
    // deleting by key is undoable too
    a.sel = vec![frame_id];
    run(&mut a, vec![key(egui::Key::Delete, egui::Modifiers::NONE)], false);
    let after_delete = a.shapes.len();
    assert!(after_delete < 3);
    run(&mut a, vec![key(egui::Key::Z, egui::Modifiers::COMMAND)], false);
    assert_eq!(a.shapes.len(), 3);
}

// background blur: the backdrop is the layers behind, blurred and cut to the layer's outline
#[test]
fn background_blur_draws_a_blurred_backdrop() {
    use crate::model::*;
    let ctx = egui::Context::default();
    let mut a = App::default();
    // a red square and a white square side by side, and a glass layer over their seam
    let red = rect_at(&mut a, 0.0, 0.0);
    let white = rect_at(&mut a, 50.0, 0.0);
    for (id, c) in [(red, egui::Color32::from_rgb(255, 0, 0)), (white, egui::Color32::WHITE)] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].size = Vec2::new(50.0, 50.0);
        a.shapes[i].fills = vec![Paint::solid(c)];
    }
    let glass = rect_at(&mut a, 25.0, 10.0);
    let gi = a.index_of(glass).unwrap();
    a.shapes[gi].size = Vec2::new(50.0, 30.0);
    a.shapes[gi].fills = vec![];
    a.shapes[gi].effects = vec![Effect { blur: 16.0, ..Effect::new(EffectKind::BackgroundBlur) }];
    let order = a.draw_order();
    let earlier: Vec<usize> = order.iter().copied().take_while(|&k| k != gi).collect();
    assert_eq!(earlier.len(), 2);
    let region = a.shapes[gi].aabb().expand(8.0 * 3.0 + 2.0);
    let (pw, ph) = (region.width() as u32 * 2, region.height() as u32 * 2);
    let img = a.backdrop_image(&ctx, &earlier, &a.shapes[gi], Vec2::ZERO, region, 8.0, pw, ph, None).unwrap();
    // pixel (x, y) of the image for a world point
    let at = |wx: f32, wy: f32| {
        let (px, py) = (((wx - region.min.x) * 2.0) as usize, ((wy - region.min.y) * 2.0) as usize);
        img.pixels[py * pw as usize + px]
    };
    // outside the layer nothing is drawn; at the seam (x = 50) the colours have mixed: neither pure red nor pure white
    assert_eq!(at(10.0, 25.0).a(), 0, "outside the glass");
    let seam = at(50.0, 25.0);
    assert!(seam.a() == 255 && seam.g() > 40 && seam.g() < 215, "the seam is blurred: {seam:?}");
    // far from the seam inside the layer the colour is still (almost) the original
    assert!(at(30.0, 25.0).g() < 90);
}

// gradient handles: the handles of a gradient being edited move its line, centre, radii and stops
#[test]
fn gradient_handles_edit_the_gradient() {
    use crate::gradient_edit::*;
    use crate::model::*;
    let mut a = App::default();
    let r = rect_at(&mut a, 100.0, 100.0);
    let i = a.index_of(r).unwrap();
    a.shapes[i].size = Vec2::new(200.0, 100.0);
    let mut g = Gradient::between(egui::Color32::WHITE, egui::Color32::BLACK);
    g.stops.insert(1, GradStop { pos: 0.5, color: egui::Color32::RED });
    a.shapes[i].fills = vec![Paint { kind: PaintKind::Gradient(g), ..Paint::default() }];
    a.sel = vec![r];
    a.grad_edit = Some((r, false, 0));
    let grad = |a: &App| match &a.shapes[a.index_of(r).unwrap()].fills[0].kind {
        PaintKind::Gradient(g) => g.clone(),
        _ => unreachable!(),
    };
    // linear: the line runs edge to edge along the horizontal; drag its end to the bottom right
    let g0 = grad(&a);
    assert_eq!((g0.angle, g0.length), (0.0, 1.0));
    a.grad_drag(G_END, 0, Pos2::new(300.0, 200.0));
    let g1 = grad(&a);
    assert!((g1.angle - 14.04).abs() < 0.1, "the line turned toward the corner: {}", g1.angle);
    // the start stayed where it was (100, 150): the centre is halfway to (300, 200)
    assert!((g1.center.x - 0.5).abs() < 1e-3 && (g1.center.y - 0.75).abs() < 1e-3, "{:?}", g1.center);
    // a stop dragged along the line: the middle stop moves to a quarter of the way and stays selected
    a.grad_drag(G_STOP, 1, Pos2::new(150.0, 162.5));
    let g2 = grad(&a);
    assert!(g2.stops[1].pos < 0.4 && g2.selected == 1 && g2.stops.len() == 3);
    // radial: the radius handles scale the ellipse, the centre handle moves it
    a.shapes[i].fills = vec![Paint { kind: PaintKind::Gradient(Gradient { kind: GradientKind::Radial, ..Gradient::default() }), ..Paint::default() }];
    a.grad_drag(G_EDGE_X, 0, Pos2::new(250.0, 150.0));
    a.grad_drag(G_EDGE_Y, 0, Pos2::new(200.0, 175.0));
    let g3 = grad(&a);
    assert!((g3.scale.x - 0.5).abs() < 1e-3 && (g3.scale.y - 0.5).abs() < 1e-3, "{:?}", g3.scale);
    a.grad_drag(G_CENTER, 0, Pos2::new(100.0, 100.0));
    assert_eq!(grad(&a).center, Pos2::new(0.0, 0.0));
    // handles are only there while a gradient is being edited
    a.grad_edit = None;
    a.grad_drag(G_CENTER, 0, Pos2::new(300.0, 200.0));
    assert_eq!(grad(&a).center, Pos2::new(0.0, 0.0));
}

// the first stop sits on the centre of a radial gradient (the start of a linear one), so dragging it moves that handle
#[test]
fn end_stops_drag_the_handle_under_them() {
    use crate::gradient_edit::*;
    use crate::model::*;
    let mut a = App::default();
    let r = rect_at(&mut a, 0.0, 0.0);
    let i = a.index_of(r).unwrap();
    a.shapes[i].size = Vec2::new(100.0, 100.0);
    a.shapes[i].fills = vec![Paint { kind: PaintKind::Gradient(Gradient { kind: GradientKind::Radial, ..Gradient::default() }), ..Paint::default() }];
    a.sel = vec![r];
    a.grad_edit = Some((r, false, 0));
    a.grad_drag(G_STOP, 0, Pos2::new(25.0, 75.0));
    let g = match &a.shapes[i].fills[0].kind { PaintKind::Gradient(g) => g.clone(), _ => unreachable!() };
    assert_eq!((g.center, g.stops[0].pos), (Pos2::new(0.25, 0.75), 0.0));
}

// export and the page composite include background blurs: the layers behind the glass are blurred inside it
#[test]
fn export_includes_background_blur() {
    use crate::model::*;
    let ctx = egui::Context::default();
    let mut a = App::default();
    let red = rect_at(&mut a, 0.0, 0.0);
    let white = rect_at(&mut a, 50.0, 0.0);
    for (id, c) in [(red, egui::Color32::from_rgb(255, 0, 0)), (white, egui::Color32::WHITE)] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].size = Vec2::new(50.0, 50.0);
        a.shapes[i].fills = vec![Paint::solid(c)];
    }
    let glass = rect_at(&mut a, 25.0, 10.0);
    let gi = a.index_of(glass).unwrap();
    a.shapes[gi].size = Vec2::new(50.0, 30.0);
    a.shapes[gi].fills = vec![];
    a.shapes[gi].effects = vec![Effect { blur: 16.0, ..Effect::new(EffectKind::BackgroundBlur) }];
    let roots: Vec<u64> = a.shapes.iter().map(|s| s.id).collect();
    let mut out = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| out = a.svg_for(ui.ctx(), &roots));
    let (svg, bounds) = out.unwrap();
    assert!(svg.contains("edgeMode=\"duplicate\""), "the blurred backdrop is in the export");
    // rasterised at 1:1 the middle of the glass (the seam at x = 50) is a mix of red and white, the squares outside it are untouched
    let img = crate::export::rasterize_scene(&svg, bounds.width() as u32, bounds.height() as u32).unwrap();
    let at = |x: f32, y: f32| img.pixels[(y - bounds.min.y) as usize * img.size[0] + (x - bounds.min.x) as usize];
    let seam = at(50.0, 25.0);
    assert!(seam.g() > 40 && seam.g() < 215, "{seam:?}");
    assert_eq!(at(10.0, 40.0), egui::Color32::from_rgb(255, 0, 0));
    // exporting only the glass has nothing behind it: no backdrop
    let mut only = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| only = a.svg_for(ui.ctx(), &[glass]));
    assert!(!only.unwrap().0.contains("edgeMode"));
}

// variables: a bound colour or number takes the variable's value in the layer's mode; a frame can choose another mode and the
// layers inside inherit it; deleting a variable unbinds; variables survive saving; the window renders
#[test]
fn variables_resolve_modes_and_bindings() {
    use crate::model::*;
    let mut a = App::default();
    let (light, dark) = (a.new_id(), a.new_id());
    let (bg, size, label) = (a.new_id(), a.new_id(), a.new_id());
    let coll = a.new_id();
    a.variables = vec![Collection {
        id: coll,
        name: "Theme".into(),
        modes: vec![VarMode { id: light, name: "Light".into() }, VarMode { id: dark, name: "Dark".into() }],
        vars: vec![
            Variable { id: bg, name: "bg".into(), kind: VarKind::Color, values: vec![VarValue::Color(egui::Color32::WHITE), VarValue::Color(egui::Color32::BLACK)], ..Variable::default() },
            Variable { id: size, name: "radius".into(), kind: VarKind::Number, values: vec![VarValue::Number(4.0), VarValue::Number(16.0)], ..Variable::default() },
            Variable { id: label, name: "title".into(), kind: VarKind::Text, values: vec![VarValue::Text("Hello".into()), VarValue::Text("Hallo".into())], ..Variable::default() },
        ],
        ..Collection::default()
    }];
    let f = frame(&mut a, 0.0, 0.0, 200.0, 100.0);
    let r = rect_at(&mut a, 10.0, 10.0);
    let ri = a.index_of(r).unwrap();
    a.shapes[ri].parent = Some(f);
    a.shapes[ri].fills[0].var = Some(bg);
    a.shapes[ri].bind(BindProp::Radius, Some(size));
    a.add(Kind::Text, Pos2::new(5.0, 5.0));
    let ti = a.shapes.len() - 1;
    a.shapes[ti].parent = None; // outside the frame
    a.shapes[ti].bind(BindProp::Text, Some(label));
    a.settle();
    let (ri, ti) = (a.index_of(r).unwrap(), ti);
    // the first mode applies by default
    assert_eq!(a.shapes[ri].fills[0].kind, PaintKind::Solid(egui::Color32::WHITE));
    assert_eq!(a.shapes[ri].radius, [4.0; 4]);
    assert_eq!(a.shapes[ti].text, "Hello");
    // the frame chooses Dark: the rectangle inside follows, the text outside does not
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].modes = vec![(coll, dark)];
    a.settle();
    assert_eq!(a.shapes[ri].fills[0].kind, PaintKind::Solid(egui::Color32::BLACK));
    assert_eq!(a.shapes[ri].radius, [16.0; 4]);
    assert_eq!(a.shapes[ti].text, "Hello");
    // editing the variable moves the layer; a mode that goes away is dropped from the frame
    a.variables[0].vars[0].values[1] = VarValue::Color(egui::Color32::from_rgb(10, 20, 30));
    a.settle();
    assert_eq!(a.shapes[ri].fills[0].kind, PaintKind::Solid(egui::Color32::from_rgb(10, 20, 30)));
    // save and load keep the variables and the bindings
    let json = serde_json::to_string(&a.doc()).unwrap();
    let doc: Doc = serde_json::from_str(&json).unwrap();
    assert_eq!(doc.variables, a.variables);
    assert!(doc.pages[0].shapes.iter().any(|s| s.bound(BindProp::Radius) == Some(size)));
    // the window renders, with a collection and its variables
    let ctx = egui::Context::default();
    a.vars_open = true;
    for _ in 0..3 {
        let _ = ctx.run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))), ..Default::default() }, |ui| a.variables_window(ui.ctx()));
    }
    assert!(a.vars_open);
}

// variants: combining components makes a set; an instance changes variant through a property value; component properties
// (boolean, text, instance swap) set layers inside instances and have defaults on the main component
#[test]
fn variants_and_component_properties() {
    use crate::model::*;
    let mut a = App::default();
    // two button components of different sizes, each with a label (text) and an icon (rectangle)
    let mut make = |a: &mut App, x: f32, w: f32, text: &str| -> (u64, u64, u64) {
        let f = frame(a, x, 0.0, w, 40.0);
        let icon = rect_at(a, x, 0.0);
        let ii = a.index_of(icon).unwrap();
        a.shapes[ii].parent = Some(f);
        a.shapes[ii].pos = Pos2::new(4.0, 4.0);
        a.add(Kind::Text, Pos2::new(x + 20.0, 10.0));
        let ti = a.shapes.len() - 1;
        a.shapes[ti].parent = Some(f);
        a.shapes[ti].pos = Pos2::new(20.0, 10.0);
        a.shapes[ti].text = text.into();
        let tid = a.shapes[ti].id;
        a.sel = vec![f];
        a.make_component();
        (f, icon, tid)
    };
    let (small, small_icon, small_text) = make(&mut a, 0.0, 80.0, "Go");
    let (large, large_icon, large_text) = make(&mut a, 200.0, 160.0, "Go");
    a.sel = vec![small, large];
    a.combine_as_variants();
    let set = a.single().unwrap();
    assert!(a.shapes[set].component_set);
    assert_eq!(a.variants_of(set).len(), 2);
    // property values and Figma-style names; the first property is renamed and the values set
    a.rename_variant_property(set, "Property 1", "Size");
    let (si, li) = (a.index_of(small).unwrap(), a.index_of(large).unwrap());
    a.set_variant_value(si, "Size", "Small");
    a.set_variant_value(li, "Size", "Large");
    assert_eq!(a.shapes[si].name, "Size=Small");
    assert_eq!(a.variant_properties(set), vec![("Size".to_owned(), vec!["Small".to_owned(), "Large".to_owned()])]);
    // a set is offered once, as its first variant
    let list = a.components();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].0, small);
    // an instance of Small becomes an instance of Large when its Size changes
    let inst = a.instantiate(small, None, Pos2::new(0.0, 300.0)).unwrap();
    assert_eq!(a.shapes[a.index_of(inst).unwrap()].size.x, 80.0);
    a.pick_variant(inst, "Size", "Large");
    let ii = a.index_of(a.shapes.iter().find(|s| s.instance && s.src == Some(large)).unwrap().id).unwrap();
    assert_eq!(a.shapes[ii].size.x, 160.0);
    let inst = a.shapes[ii].id;
    // properties: a boolean on the icon and a text on the label, defined on the set (shared by both variants)
    let before = a.shapes.clone(); // what the app's frame loop hands to sync_instances
    let ti = a.index_of(small_icon).unwrap();
    let bool_prop = a.create_property(ti, CompPropKind::Bool).unwrap();
    let txt = a.index_of(small_text).unwrap();
    // the labels share a name, so a property made from one links the other variant's label too (same name path)
    let (lt, st) = (a.index_of(large_text).unwrap(), a.index_of(small_text).unwrap());
    a.shapes[lt].name = "Label".into();
    a.shapes[st].name = "Label".into();
    let text_prop = a.create_property(txt, CompPropKind::Text).unwrap();
    assert_eq!(a.shapes[lt].cprop_link, Some(text_prop), "linked across the variants by name");
    // the same properties drive the other variant's layers
    let (l_icon, l_text) = (a.index_of(large_icon).unwrap(), a.index_of(large_text).unwrap());
    a.shapes[l_icon].cprop_link = Some(bool_prop);
    a.shapes[l_text].cprop_link = Some(text_prop);
    assert_eq!(a.shapes[a.index_of(set_id(&a)).unwrap()].cprops.len(), 2);
    a.sync_instances(&before); // the instance's layers learn which property drives them
    a.settle();
    let kids = |a: &App, inst: u64| -> (usize, usize) {
        let kids: Vec<usize> = (0..a.shapes.len()).filter(|&k| a.shapes[k].parent == Some(inst)).collect();
        let icon = kids.iter().copied().find(|&k| a.shapes[k].kind == Kind::Rect).unwrap();
        let label = kids.iter().copied().find(|&k| a.shapes[k].kind == Kind::Text).unwrap();
        (icon, label)
    };
    let (icon, label) = kids(&a, inst);
    assert!(a.shapes[icon].visible && a.shapes[label].text == "Go", "defaults match the main component");
    // the instance hides the icon and changes the text
    let ii = a.index_of(inst).unwrap();
    a.set_instance_property(ii, bool_prop, CompPropValue::Bool(false));
    a.set_instance_property(ii, text_prop, CompPropValue::Text("Buy".into()));
    a.settle();
    let (icon, label) = kids(&a, inst);
    assert!(!a.shapes[icon].visible && a.shapes[label].text == "Buy");
    // switching to the other variant keeps the chosen values
    a.pick_variant(inst, "Size", "Small");
    let inst2 = a.shapes.iter().find(|s| s.instance && s.src == Some(small)).unwrap().id;
    a.settle();
    let (icon, label) = kids(&a, inst2);
    assert!(!a.shapes[icon].visible && a.shapes[label].text == "Buy", "values survive a variant change");
    // a new default reaches instances that never set the property (a second instance), not the one that did
    let other = a.instantiate(small, None, Pos2::new(0.0, 400.0)).unwrap();
    let set_i = a.index_of(set_id(&a)).unwrap();
    a.shapes[set_i].cprops.iter_mut().find(|p| p.id == text_prop).unwrap().default = CompPropValue::Text("Hi".into());
    a.settle();
    assert_eq!(a.shapes[kids(&a, other).1].text, "Hi");
    assert_eq!(a.shapes[kids(&a, inst2).1].text, "Buy");
    // deleting a property frees its layers
    a.delete_property(set_i, bool_prop);
    assert!(a.shapes.iter().all(|s| s.cprop_link != Some(bool_prop)));
}

fn set_id(a: &App) -> u64 {
    a.shapes.iter().find(|s| s.component_set).unwrap().id
}

#[test]
fn auto_layout_ignore_and_limits() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 300.0, 100.0);
    let r1 = rect_at(&mut a, 10.0, 10.0);
    let r2 = rect_at(&mut a, 60.0, 10.0);
    let r3 = rect_at(&mut a, 120.0, 40.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].layout = Some(AutoLayout { gap: 10.0, pad: [10.0; 4], hug_x: false, hug_y: false, ..AutoLayout::default() });
    for id in [r1, r2, r3] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].size = Vec2::new(40.0, 30.0);
    }
    // r1 stays put, r2 fills but is capped at 100, r3 sits after it
    let (i1, i2, i3) = (a.index_of(r1).unwrap(), a.index_of(r2).unwrap(), a.index_of(r3).unwrap());
    a.shapes[i1].abs = true;
    a.shapes[i1].pos = Pos2::new(200.0, 50.0);
    a.shapes[i2].fill_x = true;
    a.shapes[i2].max_size.x = 100.0;
    a.settle();
    assert_eq!(a.shapes[i1].pos, Pos2::new(200.0, 50.0), "ignoring layers keep their position");
    assert_eq!(a.shapes[i2].pos.x, 10.0, "the first layer in the flow starts at the padding");
    assert_eq!(a.shapes[i2].size.x, 100.0, "Fill respects the maximum width");
    assert_eq!(a.shapes[i3].pos.x, 120.0, "the next layer follows the capped one");

    // an ignoring layer follows its constraints when the frame is resized
    a.shapes[i1].ch = Constraint::Max;
    a.shapes[fi].size.x = 400.0;
    a.settle();
    assert_eq!(a.shapes[i1].pos.x, 300.0, "right constraint moves with the frame");

    // a hugging frame is at least its minimum width
    a.shapes[fi].layout.as_mut().unwrap().hug_x = true;
    a.shapes[fi].min_size.x = 500.0;
    a.settle();
    assert_eq!(a.shapes[fi].size.x, 500.0);
}

#[test]
fn auto_layout_canvas_stacking() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 300.0, 100.0);
    let r1 = rect_at(&mut a, 10.0, 10.0);
    let r2 = rect_at(&mut a, 20.0, 10.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].layout = Some(AutoLayout::default());
    let pos = |a: &App, id: u64| a.draw_order().iter().position(|&i| a.shapes[i].id == id).unwrap();
    assert!(pos(&a, r1) < pos(&a, r2), "last child on top by default");
    a.shapes[fi].layout.as_mut().unwrap().first_on_top = true;
    assert!(pos(&a, r1) > pos(&a, r2), "first child on top with canvas stacking");
}

#[test]
fn prototype_triggers_back_scroll_and_flow() {
    let mut a = App::default();
    let fa = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let fb = frame(&mut a, 300.0, 0.0, 100.0, 100.0);
    let btn = rect_at(&mut a, 10.0, 10.0);
    let i = a.index_of(btn).unwrap();
    a.shapes[i].link = Some(fb);
    a.start_present(fa);
    // Mouse enter does not react to a click; a click does not fire it
    a.shapes[i].trigger = Trigger::MouseEnter;
    a.present_click(Pos2::new(15.0, 15.0), 1.0);
    assert_eq!(a.present, Some(fa));
    a.present_hover_at(Some(Pos2::new(15.0, 15.0)), 1.0);
    assert_eq!(a.present, Some(fb));
    // Back returns to the frame we came from
    let back = rect_at(&mut a, 310.0, 10.0);
    let k = a.index_of(back).unwrap();
    a.shapes[k].link = Some(fa);
    a.shapes[k].action = Action::Back;
    a.present_click(Pos2::new(315.0, 15.0), 2.0);
    assert_eq!(a.present, Some(fa));
    // After delay fires once the delay has passed
    a.shapes[i].trigger = Trigger::AfterDelay;
    a.shapes[i].delay = 0.5;
    a.present_t0 = Some(10.0);
    assert!(a.present_delays(10.2).is_some_and(|w| (w - 0.3).abs() < 1e-3));
    assert_eq!(a.present, Some(fa));
    a.present_delays(10.6);
    assert_eq!(a.present, Some(fb));
    // scroll range is the children's overflow beyond the frame
    let tall = frame(&mut a, 600.0, 0.0, 100.0, 100.0);
    let r = rect_at(&mut a, 610.0, 10.0);
    let ri = a.index_of(r).unwrap();
    a.shapes[ri].size = Vec2::new(10.0, 160.0);
    let ti = a.index_of(tall).unwrap();
    assert!(a.present_scroll_range(ti).y > 0.0);
    // a flow start is a plain label on a top-level frame
    a.shapes[ti].flow = Some("Flow 1".into());
    a.sel = vec![];
    assert_eq!(a.present_target(), Some(tall));
}

#[test]
fn arrow_tool_draws_a_line_with_an_arrowhead() {
    let mut h = Harness::new();
    let mut a = App::default();
    h.frame(&mut a, vec![]);
    a.rulers = false;
    a.snap = false;

    // Shift+L picks the line tool with the arrowhead on
    let shift_l = egui::Event::Key { key: egui::Key::L, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::SHIFT };
    h.keys(&mut a, vec![shift_l]);
    assert_eq!(a.tool, crate::Tool::Draw(Kind::Line));
    assert!(a.arrow_tool);
    h.drag(&mut a, scr(100.0, 100.0), scr(300.0, 100.0));
    assert_eq!(a.shapes.len(), 1);
    assert_eq!(a.shapes[0].kind, Kind::Line);
    assert_eq!(a.shapes[0].stroke_cap[1], StrokeCap::ArrowLines);
    assert_eq!(a.shapes[0].stroke_cap[0], StrokeCap::None);

    // a plain line afterwards has no arrowhead
    a.set_tool(crate::Tool::Draw(Kind::Line));
    assert!(!a.arrow_tool, "any other tool turns the arrowhead off");
    h.drag(&mut a, scr(100.0, 200.0), scr(300.0, 200.0));
    assert_eq!(a.shapes.len(), 2);
    assert_eq!(a.shapes[1].stroke_cap[1], StrokeCap::None);
}

#[test]
fn undo_takes_back_pen_points_first() {
    let mut h = Harness::new();
    let mut a = App::default();
    h.frame(&mut a, vec![]);
    a.rulers = false;
    a.snap = false;
    let f = frame(&mut a, 600.0, 600.0, 50.0, 50.0);
    a.sel = vec![f];
    let snap = a.snap();
    a.undo.push(snap); // an earlier edit that undo must not touch while a path is being drawn
    a.set_tool(crate::Tool::Pen);
    h.click(&mut a, scr(100.0, 100.0));
    h.click(&mut a, scr(200.0, 100.0));
    assert_eq!(a.pen.len(), 2);
    a.do_undo(true);
    assert_eq!(a.pen.len(), 1, "the last point goes");
    assert_eq!(a.undo.len(), 1, "the earlier edit is still on the stack");
}

#[test]
fn pen_press_on_the_first_point_can_drag_the_closing_corner_round() {
    let mut h = Harness::new();
    let mut a = App::default();
    h.frame(&mut a, vec![]);
    a.rulers = false;
    a.snap = false;
    a.set_tool(crate::Tool::Pen);
    for (x, y) in [(100.0, 100.0), (300.0, 100.0), (300.0, 300.0)] {
        h.click(&mut a, scr(x, y));
    }
    h.drag(&mut a, scr(101.0, 101.0), scr(60.0, 160.0));
    assert!(a.pen.is_empty());
    assert_eq!(a.shapes.len(), 1);
    assert!(a.shapes[0].path[0].closed);
    let first = a.shapes[0].path[0].pts[0];
    assert!(first.hout.length() > 0.1 && (first.hin + first.hout).length() < 1e-3, "round closing corner, got {:?}/{:?}", first.hin, first.hout);
}

#[test]
fn clicking_a_frames_body_selects_it_and_duplicates_count_up() {
    let mut h = Harness::new();
    let mut a = App::default();
    h.frame(&mut a, vec![]);
    a.rulers = false;
    let f = frame(&mut a, 100.0, 100.0, 300.0, 300.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].name = "Frame 1".into();
    h.click(&mut a, scr(250.0, 250.0));
    assert_eq!(a.sel, vec![f], "a click on the empty body selects the frame");
    a.duplicate();
    a.duplicate();
    let names: Vec<_> = a.shapes.iter().map(|s| s.name.clone()).collect();
    assert_eq!(names, ["Frame 1", "Frame 2", "Frame 3"]);
}

fn key_with(k: egui::Key, m: egui::Modifiers) -> egui::Event {
    egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m }
}

#[test]
fn section_slice_and_scale_tools() {
    let mut h = Harness::new();
    let mut a = App::default();
    h.frame(&mut a, vec![]);
    a.rulers = false;
    a.snap = false;
    // Shift+S draws a Section: a frame that does not clip and is named like one
    h.keys(&mut a, vec![key_with(egui::Key::S, egui::Modifiers::SHIFT)]);
    h.drag(&mut a, scr(100.0, 100.0), scr(400.0, 300.0));
    assert!(a.shapes[0].section && a.shapes[0].kind == Kind::Frame && !a.shapes[0].clip);
    assert_eq!(a.shapes[0].name, "Section 1");
    // S draws a Slice, which exports what lies under it
    let r = rect_at(&mut a, 120.0, 120.0);
    let ri = a.index_of(r).unwrap();
    a.shapes[ri].size = Vec2::new(50.0, 50.0);
    h.keys(&mut a, vec![Harness::key(egui::Key::S)]);
    h.drag(&mut a, scr(110.0, 110.0), scr(210.0, 210.0));
    let sl = a.shapes.iter().find(|s| s.slice).expect("a slice").clone();
    assert_eq!(sl.name, "Slice 1");
    let (svg, b) = a.svg_for(&egui::Context::default(), &[sl.id]).expect("slice exports");
    assert_eq!(b, a.world_rect(a.index_of(sl.id).unwrap()));
    assert!(svg.contains("<rect") || svg.contains("<path"), "the layers under the slice are drawn: {svg}");
    // K: scaling a frame scales its children, stroke and text with it
    a.shapes.clear();
    let f = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let c = rect_at(&mut a, 10.0, 10.0);
    let ci = a.index_of(c).unwrap();
    a.shapes[ci].parent = Some(f);
    a.shapes[ci].pos = Pos2::new(10.0, 10.0);
    a.shapes[ci].size = Vec2::new(20.0, 20.0);
    a.shapes[ci].stroke_w = 2.0;
    a.sel = vec![f];
    a.scale_tool = true;
    a.begin_scale();
    a.do_scale((1, 1), Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)), Pos2::new(200.0, 200.0), false);
    let (fi, ci) = (a.index_of(f).unwrap(), a.index_of(c).unwrap());
    assert_eq!(a.shapes[fi].size, Vec2::splat(200.0));
    assert_eq!(a.shapes[ci].pos, Pos2::new(20.0, 20.0));
    assert_eq!(a.shapes[ci].size, Vec2::new(40.0, 40.0));
    assert_eq!(a.shapes[ci].stroke_w, 4.0);
}

#[test]
fn figma_layer_shortcuts() {
    let mut h = Harness::new();
    let mut a = App::default();
    let r1 = rect_at(&mut a, 0.0, 0.0);
    let r2 = rect_at(&mut a, 200.0, 0.0);
    let r3 = rect_at(&mut a, 400.0, 0.0);
    let (none, shift, cs) = (egui::Modifiers::NONE, egui::Modifiers::SHIFT, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
    a.sel = vec![r1];
    h.keys(&mut a, vec![key_with(egui::Key::Tab, none)]);
    assert_eq!(a.sel, vec![r2], "Tab: next layer");
    h.keys(&mut a, vec![key_with(egui::Key::Tab, shift)]);
    assert_eq!(a.sel, vec![r1], "Shift+Tab: previous layer");
    h.keys(&mut a, vec![key_with(egui::Key::A, cs)]);
    assert_eq!(a.sel, vec![r2, r3], "Ctrl+Shift+A: select inverse");
    h.keys(&mut a, vec![key_with(egui::Key::H, cs)]);
    assert!(!a.shapes[a.index_of(r2).unwrap()].visible && !a.shapes[a.index_of(r3).unwrap()].visible, "Ctrl+Shift+H hides");
    h.keys(&mut a, vec![key_with(egui::Key::H, cs)]);
    assert!(a.shapes[a.index_of(r2).unwrap()].visible, "and shows again");
    h.keys(&mut a, vec![key_with(egui::Key::L, cs)]);
    assert!(a.shapes[a.index_of(r2).unwrap()].locked, "Ctrl+Shift+L locks");
    // copy / paste properties
    let i1 = a.index_of(r1).unwrap();
    a.shapes[i1].opacity = 0.5;
    a.sel = vec![r1];
    let ca = egui::Modifiers::COMMAND | egui::Modifiers::ALT;
    h.keys(&mut a, vec![key_with(egui::Key::C, ca)]);
    a.sel = vec![r3];
    h.keys(&mut a, vec![key_with(egui::Key::V, ca)]);
    assert_eq!(a.shapes[a.index_of(r3).unwrap()].opacity, 0.5);
    // frame selection wraps in a plain frame, no auto layout
    a.sel = vec![r1];
    h.keys(&mut a, vec![key_with(egui::Key::G, ca)]);
    let f = a.single().map(|i| a.shapes[i].clone()).unwrap();
    assert!(f.kind == Kind::Frame && f.layout.is_none());
    assert_eq!(a.shapes[a.index_of(r1).unwrap()].parent, Some(f.id));
    // Shift+Enter selects the parent
    a.sel = vec![r1];
    h.keys(&mut a, vec![key_with(egui::Key::Enter, shift)]);
    assert_eq!(a.sel, vec![f.id]);
}

// several layers selected: the Design panel draws position, dimensions, appearance, fill, stroke and effects without
// touching the layers (the sections edit a stand-in layer that is removed again within the frame)
#[test]
fn multi_selection_panel_draws_and_leaves_shapes_alone() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let r1 = rect_at(&mut a, 10.0, 10.0);
    let r2 = rect_at(&mut a, 60.0, 10.0);
    a.sel = vec![r1, r2];
    let before = a.shapes.clone();
    for k in 0..3 {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))), time: Some(0.1 * (k + 1) as f64), ..Default::default() };
        let _ = ctx.run_ui(raw, |ui| a.run_frame(ui));
    }
    assert_eq!(a.shapes, before);
    // the box around both: moving it moves both, resizing it scales both
    a.nudge(5.0, 0.0);
    assert_eq!(a.shapes[0].pos.x, before[0].pos.x + 5.0);
    let bb = a.sel_bounds().unwrap();
    a.begin_scale();
    a.do_scale((1, 1), bb, Pos2::new(bb.min.x + bb.width() * 2.0, bb.max.y), false);
    let nb = a.sel_bounds().unwrap();
    assert!((nb.width() - bb.width() * 2.0).abs() < 0.01 && (nb.height() - bb.height()).abs() < 0.01);
}

#[test]
fn outline_stroke_and_offset_path() {
    use crate::model::*;
    let mut a = App::default();
    // a stroke-only line of width 10 becomes a filled shape 10 thick
    let l = a.new_id();
    let mut s = Shape::new(Kind::Line, l, 1, Pos2::new(0.0, 0.0), None);
    s.size = Vec2::new(100.0, 0.0);
    s.stroke_w = 10.0;
    s.strokes = vec![Paint::solid(egui::Color32::RED)];
    a.shapes.push(s);
    a.sel = vec![l];
    a.outline_stroke();
    let i = a.index_of(l).unwrap();
    assert_eq!(a.shapes[i].kind, Kind::Path);
    assert!(a.shapes[i].strokes.is_empty() && a.shapes[i].fills.len() == 1);
    assert!((a.shapes[i].size.y - 10.0).abs() < 0.5 && (a.shapes[i].size.x - 100.0).abs() < 0.5, "size {:?}", a.shapes[i].size);
    // a rectangle with a fill and an inside stroke keeps its fill and gets the outline as a new layer
    let r = rect_at(&mut a, 200.0, 200.0);
    let ri = a.index_of(r).unwrap();
    a.shapes[ri].size = Vec2::new(100.0, 100.0);
    a.shapes[ri].stroke_w = 10.0;
    a.shapes[ri].stroke_align = StrokeAlign::Inside;
    a.shapes[ri].strokes = vec![Paint::solid(egui::Color32::BLUE)];
    a.sel = vec![r];
    let n = a.shapes.len();
    a.outline_stroke();
    assert_eq!(a.shapes.len(), n + 1);
    let o = &a.shapes[a.single().unwrap()];
    assert!(o.size.x <= 100.5 && o.size.x >= 99.5, "an inside stroke stays inside: {:?}", o.size);
    assert!(a.shapes[a.index_of(r).unwrap()].strokes.is_empty());
    // offset path grows and shrinks
    let r2 = rect_at(&mut a, 500.0, 500.0);
    let r2i = a.index_of(r2).unwrap();
    a.shapes[r2i].size = Vec2::new(100.0, 100.0);
    a.sel = vec![r2];
    a.offset_selection(10.0);
    let g = a.index_of(r2).unwrap();
    assert!((a.shapes[g].size.x - 120.0).abs() < 1.0, "grown: {:?}", a.shapes[g].size);
    a.offset_selection(-20.0);
    let g = a.index_of(r2).unwrap();
    assert!((a.shapes[g].size.x - 80.0).abs() < 1.0, "shrunk: {:?}", a.shapes[g].size);
}

#[test]
fn tidy_up_eyedropper_and_actions() {
    use crate::model::*;
    let mut a = App::default();
    // three squares in a rough row with uneven gaps and heights become an even row
    let ids: Vec<u64> = [(0.0, 3.0), (60.0, 0.0), (150.0, 5.0)].iter().map(|&(x, y)| rect_at(&mut a, x, y)).collect();
    for &id in &ids {
        let i = a.index_of(id).unwrap();
        a.shapes[i].size = Vec2::new(40.0, 40.0);
    }
    a.sel = ids.clone();
    a.tidy_up();
    let xs: Vec<f32> = ids.iter().map(|&id| a.shapes[a.index_of(id).unwrap()].pos.x).collect();
    let gaps = [xs[1] - xs[0] - 40.0, xs[2] - xs[1] - 40.0];
    assert!((gaps[0] - gaps[1]).abs() < 0.01, "even gaps: {gaps:?}");
    assert_eq!(xs[0], 0.0, "the corner of the selection stays");
    // the eyedropper takes the color of the layer under the point
    let r = rect_at(&mut a, 500.0, 500.0);
    let ri = a.index_of(r).unwrap();
    a.shapes[ri].size = Vec2::new(50.0, 50.0);
    a.shapes[ri].fills = vec![Paint::solid(egui::Color32::from_rgb(1, 2, 3))];
    assert_eq!(a.color_at(Pos2::new(520.0, 520.0)), Some(egui::Color32::from_rgb(1, 2, 3)));
    a.sel = vec![ids[0]];
    a.apply_picked(egui::Color32::from_rgb(1, 2, 3), &egui::Context::default());
    assert_eq!(a.shapes[a.index_of(ids[0]).unwrap()].fills.last().map(|p| p.kind.clone()), Some(PaintKind::Solid(egui::Color32::from_rgb(1, 2, 3))));
    // the Actions palette opens and a typed command runs
    a.open_actions();
    assert!(a.actions_open);
}

#[test]
fn text_on_a_path_draws_and_exports_along_it() {
    use crate::model::*;
    let ctx = egui::Context::default();
    let mut a = App::default();
    let l = a.new_id();
    let mut line = Shape::new(Kind::Line, l, 1, Pos2::new(0.0, 100.0), None);
    line.size = Vec2::new(200.0, 0.0);
    a.shapes.push(line);
    let t = a.new_id();
    let mut text = Shape::new(Kind::Text, t, 2, Pos2::new(0.0, 0.0), None);
    text.text = "Hello".into();
    a.shapes.push(text);
    a.sel = vec![t, l];
    a.text_on_path();
    assert_eq!(a.shapes[a.index_of(t).unwrap()].text_path, Some(l));
    let (svg, _) = a.svg_for(&ctx, &[t]).expect("svg");
    assert!(svg.contains("<textPath") && svg.contains("M0.00 100.00 L200.00 100.00"), "{svg}");
    let (p, dir) = crate::geom::along(&[Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0), Pos2::new(10.0, 10.0)], 15.0).unwrap();
    assert_eq!((p, dir), (Pos2::new(10.0, 5.0), Vec2::new(0.0, 1.0)));
    a.detach_text_path();
    assert_eq!(a.shapes[a.index_of(t).unwrap()].text_path, None);
}

#[test]
fn pages_move_duplicate_and_keep_their_comments() {
    let mut a = App::default();
    a.pages = ["A", "B", "C"].iter().map(|n| Page { name: (*n).into(), ..Page::default() }).collect();
    a.cur = 0;
    let id = a.new_id();
    a.shapes = vec![Shape::new(Kind::Rect, id, 1, Pos2::ZERO, None)];
    // drag A to the end: B C A, and the open page is still the one that holds the rectangle
    a.move_page(0, 3);
    let names: Vec<_> = a.pages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["B", "C", "A"]);
    assert_eq!(a.cur, 2);
    assert_eq!(a.pages[a.cur].name, "A");
    assert_eq!(a.pages[a.cur].shapes.len(), 1, "the open page's shapes went with it");
    // dropping a page where it already is changes nothing
    a.move_page(1, 1);
    a.move_page(1, 2);
    assert_eq!(a.pages.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["B", "C", "A"]);
    // duplicate B: B, B copy, C, A, and the copy is open
    a.duplicate_page(0);
    let names: Vec<_> = a.pages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["B", "B copy", "C", "A"]);
    assert_eq!(a.cur, 1);
    // the page that held the rectangle is now at 3
    assert_eq!(a.pages[3].shapes.len(), 1);
}

#[test]
fn text_style_letter_spacing_and_vertical_alignment_reach_the_layout_and_the_svg() {
    use crate::model::*;
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(Default::default(), |_| {}); // fonts exist after the first pass
    crate::widgets::apply_theme(&ctx);
    let _ = ctx.run_ui(Default::default(), |_| {});
    let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), Rect::EVERYTHING);
    let mut t = Shape::new(Kind::Text, 1, 1, Pos2::ZERO, None);
    t.text = "Hello".into();
    let plain = crate::render::text_galley(&painter, &t, 1.0, egui::Color32::WHITE, f32::INFINITY).size().x;
    t.tracking = 10.0;
    let spaced = crate::render::text_galley(&painter, &t, 1.0, egui::Color32::WHITE, f32::INFINITY).size().x;
    assert!(spaced > plain + 2.0, "letter spacing widens the text: {plain} -> {spaced}");
    // Semi Bold and a bottom-aligned text in a tall box reach the SVG
    t.tracking = 0.0;
    t.bold = true;
    t.fit = TextFit::Fixed;
    t.size = Vec2::new(100.0, 100.0);
    t.valign = 2;
    let mut a = App::default();
    a.shapes = vec![t];
    let (svg, _) = a.svg_for(&ctx, &[1]).unwrap();
    assert!(svg.contains("font-weight=\"600\""), "{svg}");
    let y: f32 = svg.split(" y=\"").nth(1).and_then(|s| s.split('"').next()).and_then(|s| s.parse().ok()).unwrap();
    assert!(y > 70.0, "bottom aligned: baseline at {y}");
}

#[test]
fn layout_handles_drag_padding_and_gap() {
    use crate::layout_handles::{LayoutDrag, LayoutHandle};
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let r1 = rect_at(&mut a, 0.0, 0.0);
    let r2 = rect_at(&mut a, 0.0, 0.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].layout = Some(AutoLayout::default()); // 8 gap, 8 padding, hugging, horizontal
    for id in [r1, r2] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].size = Vec2::new(40.0, 30.0);
    }
    a.settle();
    a.sel = vec![f];
    let w0 = a.shapes[fi].size.x;
    let drag = |h, lay: &AutoLayout| LayoutDrag { h, frame: f, from: Pos2::ZERO, pad: lay.pad, gap: lay.gap };
    let lay = a.shapes[fi].layout.clone().unwrap();

    // the left padding strip sits inside the left edge; a press there starts the drag (view at zoom 1, no pan)
    a.zoom = 1.0;
    a.pan = Vec2::ZERO;
    let strip = a.world_rect(fi).min + Vec2::new(3.0, 20.0);
    assert!(matches!(a.layout_handle_at(Pos2::ZERO, strip), Some(crate::Drag::Layout(d)) if d.h == LayoutHandle::Pad(0)));
    // between the two children: the gap
    let mid = a.world_rect(fi).min + Vec2::new(8.0 + 40.0 + 4.0, 20.0);
    assert!(matches!(a.layout_handle_at(Pos2::ZERO, mid), Some(crate::Drag::Layout(d)) if d.h == LayoutHandle::Gap));

    a.layout_drag(drag(LayoutHandle::Pad(0), &lay), Pos2::new(5.0, 0.0), false, false);
    assert_eq!(a.shapes[fi].layout.as_ref().unwrap().pad, [13.0, 8.0, 8.0, 8.0], "dragging in grows the padding");
    a.layout_drag(drag(LayoutHandle::Pad(2), &lay), Pos2::new(-4.0, 0.0), false, true);
    assert_eq!(a.shapes[fi].layout.as_ref().unwrap().pad, [12.0, 8.0, 12.0, 8.0], "Alt moves the opposite side too");
    a.layout_drag(drag(LayoutHandle::Pad(3), &lay), Pos2::new(0.0, 99.0), false, false);
    a.layout_drag(drag(LayoutHandle::Pad(1), &lay), Pos2::new(0.0, -50.0), true, false);
    assert_eq!(a.shapes[fi].layout.as_ref().unwrap().pad, [0.0; 4].map(|_| 0.0), "Shift sets all four, never below 0");
    a.layout_drag(drag(LayoutHandle::Gap, &lay), Pos2::new(4.0, 0.0), false, false);
    assert_eq!(a.shapes[fi].layout.as_ref().unwrap().gap, 12.0);
    a.settle();
    assert_eq!(a.shapes[fi].size.x, 40.0 + 40.0 + 12.0, "a hugging frame follows the new padding and gap (was {w0})");
}

#[test]
fn effect_text_and_grid_styles_propagate_and_survive_delete() {
    let mut a = App::default();
    let r1 = rect_at(&mut a, 0.0, 0.0);
    let r2 = rect_at(&mut a, 50.0, 0.0);
    a.add(Kind::Text, Pos2::new(0.0, 50.0));
    let t = a.shapes.last().unwrap().id;
    a.ext.effects.push(EffectStyle { id: 900, name: "Soft".into(), effects: vec![Effect::new(EffectKind::DropShadow)] });
    a.ext.texts.push(TextStyle { id: 901, name: "Body".into(), font_size: 20.0, bold: true, mono: false, tracking: 5.0, line_h: 1.4, align: TextAlign::Center, valign: 1, extra: TextExtra::default() });
    a.ext.grids.push(GridStyle { id: 902, name: "Cols".into(), grids: vec![LayoutGrid::new(GridKind::Columns)] });
    let (i1, i2, it) = (a.index_of(r1).unwrap(), a.index_of(r2).unwrap(), a.index_of(t).unwrap());
    a.shapes[i1].effect_style = Some(900);
    a.shapes[i2].effect_style = Some(900);
    a.shapes[i1].grid_style = Some(902);
    a.shapes[it].text_style = Some(901);
    a.settle();
    assert_eq!(a.shapes[i1].effects.len(), 1);
    assert_eq!(a.shapes[i2].effects.len(), 1);
    assert_eq!(a.shapes[i1].grids[0].kind, GridKind::Columns);
    assert_eq!((a.shapes[it].font_size, a.shapes[it].bold, a.shapes[it].align, a.shapes[it].tracking), (20.0, true, TextAlign::Center, 5.0));
    // editing the style reaches every layer that follows it
    a.ext.effects[0].effects[0].blur = 30.0;
    a.ext.texts[0].font_size = 33.0;
    a.settle();
    assert_eq!(a.shapes[i2].effects[0].blur, 30.0);
    assert_eq!(a.shapes[it].font_size, 33.0);
    // the styles are saved with the document
    let doc = a.doc();
    let back: Doc = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!((back.ext_styles.effects.len(), back.ext_styles.texts.len(), back.ext_styles.grids.len()), (1, 1, 1));
    // deleting a style lets the layers keep what they had
    a.ext.effects.clear();
    a.settle();
    assert_eq!(a.shapes[i1].effect_style, None);
    assert_eq!(a.shapes[i1].effects[0].blur, 30.0);
}

#[test]
fn noise_texture_and_progressive_blur_reach_the_svg_and_rasterize() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let mut svg_of = |a: &mut App, effects: Vec<Effect>| -> String {
        a.shapes = vec![Shape { id: 1, kind: Kind::Rect, size: Vec2::new(100.0, 60.0), effects, ..Shape::default() }];
        let roots = vec![1];
        let mut out = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| out = a.svg_for(ui.ctx(), &roots));
        out.unwrap().0
    };
    let noise = |mode: u8| Effect { mode, ..Effect::new(EffectKind::Noise) };
    for mode in 0..3u8 {
        let svg = svg_of(&mut a, vec![noise(mode)]);
        assert!(svg.contains("feTurbulence"), "noise mode {mode}: {svg}");
        assert!(crate::export::render_png(&svg, 1.0).is_ok(), "noise mode {mode} rasterizes");
    }
    assert_eq!(svg_of(&mut a, vec![noise(1)]).matches("feTurbulence").count(), 2, "duo draws two passes");
    let tex = svg_of(&mut a, vec![Effect::new(EffectKind::Texture)]);
    assert!(tex.contains("feTurbulence") && tex.contains("SourceAlpha"), "{tex}");
    assert!(crate::export::render_png(&tex, 1.0).is_ok());
    let prog = svg_of(&mut a, vec![Effect { progressive: true, start_blur: 0.0, blur: 20.0, ..Effect::new(EffectKind::LayerBlur) }]);
    assert_eq!(prog.matches("<use").count(), 10, "{prog}");
    assert!(crate::export::render_png(&prog, 1.0).is_ok());
    // a uniform blur keeps its single filter and needs no <use>
    let uni = svg_of(&mut a, vec![Effect::new(EffectKind::LayerBlur)]);
    assert!(!uni.contains("<use") && uni.contains("feGaussianBlur"));
    assert!(Shape { effects: vec![noise(0)], ..Shape::default() }.needs_sprite());
}

#[test]
fn prototype_overlays_scroll_to_variables_keys_and_several_interactions() {
    let mut a = App::default();
    let home = frame(&mut a, 0.0, 0.0, 200.0, 400.0);
    let modal = frame(&mut a, 500.0, 0.0, 100.0, 80.0);
    let btn = rect_at(&mut a, 10.0, 10.0);
    let close = rect_at(&mut a, 510.0, 10.0);
    let (hi, mi, bi, ci) = (a.index_of(home).unwrap(), a.index_of(modal).unwrap(), a.index_of(btn).unwrap(), a.index_of(close).unwrap());
    a.shapes[mi].size = Vec2::new(100.0, 80.0);
    for i in [bi, ci] {
        a.shapes[i].size = Vec2::new(20.0, 20.0);
    }
    a.shapes[bi].parent = Some(home);
    a.shapes[ci].parent = Some(modal);
    a.shapes[ci].pos = Pos2::new(10.0, 10.0);
    // the button opens the modal at the bottom right; the modal's own button closes it
    a.shapes[bi].link = Some(modal);
    a.shapes[bi].action = Action::OpenOverlay;
    a.shapes[bi].overlay_pos = 6;
    a.shapes[ci].link = Some(modal);
    a.shapes[ci].action = Action::CloseOverlay;
    a.start_present(home);
    let w = a.world_rect(bi).center();
    a.present_click(w, 1.0);
    assert_eq!(a.present_overlays.len(), 1);
    assert_eq!(a.present_overlays[0].shift, Vec2::new(100.0 - 500.0, 320.0), "bottom right of the 200 x 400 frame");
    // an open overlay blocks the page behind it (when it does not close on an outside click)
    a.present_overlays[0].outside = false;
    a.present_click(w, 1.1);
    assert_eq!(a.present_overlays.len(), 1);
    // a click outside closes it (close_outside is on by default)
    a.present_overlays[0].outside = true;
    a.present_click(Pos2::new(5.0, 5.0), 1.2);
    assert!(a.present_overlays.is_empty());
    // open again and close with the modal's button, which lives at the shifted place
    a.present_click(w, 1.3);
    let inside = a.world_rect(ci).center() + a.present_overlays[0].shift;
    a.present_click(inside, 1.4);
    assert!(a.present_overlays.is_empty());

    // Set variable changes the value while presenting and exit puts it back
    a.exit_present();
    a.variables.push(Collection { id: 70, name: "Vars".into(), modes: vec![VarMode { id: 71, name: "Mode 1".into() }], vars: vec![Variable { id: 72, name: "count".into(), kind: VarKind::Number, values: vec![VarValue::Number(1.0)], ..Variable::default() }], ..Collection::default() });
    a.shapes[bi].action = Action::SetVariable;
    a.shapes[bi].var = Some(72);
    a.shapes[bi].value = "5".into();
    a.start_present(home);
    a.present_click(w, 2.0);
    assert_eq!(a.variables[0].vars[0].values[0], VarValue::Number(5.0));
    a.exit_present();
    assert_eq!(a.variables[0].vars[0].values[0], VarValue::Number(1.0), "variables are restored");

    // a key press runs its interaction; a second interaction on the same layer runs on its own trigger
    a.shapes[bi].action = Action::Navigate;
    a.shapes[bi].link = Some(modal);
    a.shapes[bi].more.push(Interaction { trigger: Trigger::KeyPress, key: "enter".into(), action: Action::Navigate, link: Some(modal), ..Interaction::default() });
    a.start_present(home);
    a.present_key("Enter", 3.0);
    assert_eq!(a.present, Some(modal));
    // scroll to a layer: the presented frame scrolls to it
    a.exit_present();
    a.shapes[hi].overflow = Overflow::Vertical;
    let far = rect_at(&mut a, 0.0, 600.0);
    let fi = a.index_of(far).unwrap();
    a.shapes[fi].parent = Some(home);
    a.shapes[fi].size = Vec2::new(20.0, 20.0);
    a.shapes[bi].more.clear();
    a.shapes[bi].action = Action::ScrollTo;
    a.shapes[bi].link = Some(far);
    a.start_present(home);
    a.present_click(w, 4.0);
    let hi = a.index_of(home).unwrap();
    assert_eq!(a.present_scroll.y, a.present_scroll_range(hi).y.min(600.0), "scrolled as far as the content allows");
}


#[test]
fn rich_text_runs_case_lists_and_export() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    a.add(Kind::Text, Pos2::new(0.0, 0.0));
    let i = a.shapes.len() - 1;
    a.shapes[i].text = "hello world\nsecond".into();
    a.shapes[i].fit = TextFit::AutoWidth;
    // case: upper, lower, title; the characters shown change, the text stays
    let shown = |s: &Shape| s.display_chars().iter().map(|c| c.0).collect::<String>();
    a.shapes[i].case = 1;
    assert_eq!(shown(&a.shapes[i]), "HELLO WORLD\nSECOND");
    a.shapes[i].case = 3;
    assert_eq!(shown(&a.shapes[i]), "Hello World\nSecond");
    a.shapes[i].case = 0;
    // lists: bullets and numbers at every paragraph start
    a.shapes[i].list = 2;
    assert_eq!(shown(&a.shapes[i]), "1.\u{2002}hello world\n2.\u{2002}second");
    a.shapes[i].list = 1;
    assert!(shown(&a.shapes[i]).starts_with("\u{2022}\u{2002}hello"));
    a.shapes[i].list = 0;
    // paragraph spacing adds an empty line; indent adds an invisible first-line space
    a.shapes[i].para_spacing = 10.0;
    assert_eq!(a.shapes[i].display_chars().iter().filter(|c| c.1.spacer).count(), 1);
    a.shapes[i].para_spacing = 0.0;
    // runs: a bold stretch; typing before it moves it; typing inside it extends it
    a.shapes[i].style_range(0, 5, |r| r.weight = Some(700));
    assert_eq!(a.shapes[i].runs.len(), 1);
    assert_eq!(a.shapes[i].char_style(2).weight, 700);
    assert_eq!(a.shapes[i].char_style(6).weight, 400);
    let old = a.shapes[i].text.clone();
    a.shapes[i].text = format!(">>{old}");
    let new = a.shapes[i].text.clone();
    a.shapes[i].shift_runs(&old, &new);
    assert_eq!((a.shapes[i].runs[0].start, a.shapes[i].runs[0].end), (2, 7), "typing before the run moves it");
    let old = a.shapes[i].text.clone();
    a.shapes[i].text = old.replacen("hello", "heXllo", 1);
    let new = a.shapes[i].text.clone();
    a.shapes[i].shift_runs(&old, &new);
    assert_eq!((a.shapes[i].runs[0].start, a.shapes[i].runs[0].end), (2, 8), "typing inside the run extends it");
    // styling the same range twice merges, and equal neighbours join
    a.shapes[i].style_range(2, 8, |r| r.weight = Some(700));
    a.shapes[i].style_range(8, 10, |r| r.weight = Some(700));
    assert_eq!(a.shapes[i].runs.len(), 1);
    assert_eq!((a.shapes[i].runs[0].start, a.shapes[i].runs[0].end), (2, 10));
    // export: mixed styles write one text piece per style, decoration and links are kept
    a.shapes[i].style_range(12, 15, |r| {
        r.underline = Some(true);
        r.url = Some("https://example.com".into());
    });
    a.shapes[i].underline = false;
    let id = a.shapes[i].id;
    let mut svg = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| svg = a.svg_for(ui.ctx(), &[id]));
    let svg = svg.unwrap().0;
    assert!(svg.contains("font-weight=\"700\"") && svg.contains("text-decoration=\"underline\"") && svg.contains("href=\"https://example.com\""), "{svg}");
    assert!(crate::export::render_png(&svg, 1.0).is_ok());
    // a plain layer still takes the simple path
    let mut plain = Shape { id: 9, kind: Kind::Text, text: "plain".into(), size: Vec2::new(40.0, 16.0), ..Shape::default() };
    assert!(!plain.rich());
    plain.italic = true;
    assert!(plain.rich());
}

#[test]
fn text_links_open_in_present_mode() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 200.0, 100.0);
    a.add(Kind::Text, Pos2::new(10.0, 10.0));
    let i = a.shapes.len() - 1;
    a.shapes[i].parent = Some(f);
    a.shapes[i].text = "see docs now".into();
    a.shapes[i].size = Vec2::new(120.0, 20.0);
    a.shapes[i].style_range(4, 8, |r| r.url = Some("https://example.com/docs".into()));
    a.start_present(f);
    a.present_click(Pos2::new(10.0 + 120.0 * 6.0 / 12.0, 20.0), 1.0);
    assert_eq!(a.open_url.take().as_deref(), Some("https://example.com/docs"));
    a.present_click(Pos2::new(150.0, 90.0), 1.1);
    assert!(a.open_url.is_none(), "a click away from the text opens nothing");
}

#[test]
fn double_click_an_edge_hugs_or_fills() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 300.0, 200.0);
    let c = frame(&mut a, 20.0, 20.0, 100.0, 80.0);
    let (fi, ci) = (a.index_of(f).unwrap(), a.index_of(c).unwrap());
    a.shapes[ci].parent = Some(f);
    a.shapes[fi].layout = Some(AutoLayout { hug_x: false, hug_y: false, ..AutoLayout::default() });
    a.shapes[ci].layout = Some(AutoLayout { hug_x: false, hug_y: false, ..AutoLayout::default() });
    a.zoom = 1.0;
    a.pan = Vec2::ZERO;
    a.sel = vec![c];
    a.settle();
    let r = a.world_rect(ci);
    // the right edge's middle handle: plain double-click hugs the width, Alt fills it
    let right = Pos2::new(r.max.x, r.center().y);
    assert!(a.edge_double_click(Pos2::ZERO, right, false));
    assert!(a.shapes[ci].layout.as_ref().unwrap().hug_x);
    let r = a.world_rect(ci);
    let bottom = Pos2::new(r.center().x, r.max.y);
    assert!(a.edge_double_click(Pos2::ZERO, bottom, true));
    assert!(a.shapes[ci].fill_y);
    // a corner or empty space does nothing
    assert!(!a.edge_double_click(Pos2::ZERO, Pos2::new(r.max.x, r.max.y), false));
    assert!(!a.edge_double_click(Pos2::ZERO, Pos2::new(500.0, 500.0), false));
}

#[test]
fn auto_layout_aligns_text_baselines() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 300.0, 100.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].layout = Some(AutoLayout { pad: [0.0; 4], gap: 0.0, ..AutoLayout::default() });
    for (size, px) in [(10.0, 20.0), (40.0, 50.0)] {
        a.add(Kind::Text, Pos2::new(px, 0.0));
        let t = a.shapes.len() - 1;
        a.shapes[t].parent = Some(f);
        a.shapes[t].font_size = size;
        a.shapes[t].size = Vec2::new(30.0, size * 1.2);
    }
    let (small, big) = (a.shapes.len() - 2, a.shapes.len() - 1);
    a.settle();
    assert_eq!(a.shapes[small].pos.y, 0.0, "top aligned to begin with");
    a.shapes[fi].layout.as_mut().unwrap().baseline = true;
    a.settle();
    // the baselines (80% of the size below the top) meet: the small text sits 24 lower
    assert!((a.shapes[small].pos.y - 24.0).abs() < 1e-3 && a.shapes[big].pos.y == 0.0, "{} {}", a.shapes[small].pos.y, a.shapes[big].pos.y);
    // a vertical flow ignores the setting
    a.shapes[fi].layout.as_mut().unwrap().vertical = true;
    a.settle();
    assert_eq!(a.shapes[small].pos.x, 0.0);
}

#[test]
fn grid_cells_span_and_columns_have_fixed_sizes() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 300.0, 300.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].layout = Some(AutoLayout { columns: 3, gap: 10.0, pad: [0.0; 4], hug_x: true, hug_y: true, ..AutoLayout::default() });
    let mut ids = vec![];
    for _ in 0..5 {
        let id = rect_at(&mut a, 0.0, 0.0);
        let i = a.index_of(id).unwrap();
        a.shapes[i].parent = Some(f);
        a.shapes[i].size = Vec2::new(40.0, 30.0);
        ids.push(i);
    }
    // the first cell covers two columns, the second two rows: 1 1 2 / 3 3 2? placed in order into free cells
    a.shapes[ids[0]].span = [2, 1];
    a.shapes[ids[0]].size.x = 120.0;
    a.shapes[ids[1]].span = [1, 2];
    a.settle();
    // columns: 120 over two columns means 55 each; the third column is 40 wide
    assert_eq!(a.shapes[ids[0]].pos, Pos2::new(0.0, 0.0));
    assert_eq!(a.shapes[ids[1]].pos.x, 55.0 + 10.0 + 55.0 + 10.0, "the second cell sits in the third column");
    assert_eq!(a.shapes[ids[2]].pos, Pos2::new(0.0, 40.0), "the next free cell is below the wide one");
    assert_eq!(a.shapes[ids[3]].pos, Pos2::new(65.0, 40.0));
    // fixed sizes win over the content
    a.shapes[fi].layout.as_mut().unwrap().col_sizes = vec![100.0, 0.0, 70.0];
    a.settle();
    assert_eq!(a.shapes[ids[1]].pos.x, 100.0 + 10.0 + 55.0 + 10.0, "column 0 is fixed at 100, column 1 keeps the width the spanning cell gave it");
}

#[test]
fn vertical_auto_layout_wraps_into_columns() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 200.0, 100.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].layout = Some(AutoLayout { vertical: true, wrap: true, gap: 10.0, pad: [0.0; 4], hug_x: true, hug_y: false, ..AutoLayout::default() });
    let mut ids = vec![];
    for _ in 0..5 {
        let id = rect_at(&mut a, 0.0, 0.0);
        let i = a.index_of(id).unwrap();
        a.shapes[i].parent = Some(f);
        a.shapes[i].size = Vec2::new(30.0, 40.0);
        ids.push(i);
    }
    a.settle();
    // 40 + 10 + 40 = 90 fits in 100, a third would not: two per column
    let pos: Vec<(f32, f32)> = ids.iter().map(|&i| (a.shapes[i].pos.x, a.shapes[i].pos.y)).collect();
    assert_eq!(pos, vec![(0.0, 0.0), (0.0, 50.0), (40.0, 0.0), (40.0, 50.0), (80.0, 0.0)]);
    assert_eq!(a.shapes[fi].size.x, 110.0, "hugging width covers three columns");
}

#[test]
fn variables_aliases_scopes_groups_extensions_and_effect_bindings() {
    use egui::Color32;
    let mut a = App::default();
    let (light, dark, brand, accent, radius, base, ext, dark2) = (a.new_id(), a.new_id(), a.new_id(), a.new_id(), a.new_id(), a.new_id(), a.new_id(), a.new_id());
    let blur = a.new_id();
    a.variables = vec![Collection {
        id: base,
        name: "Base".into(),
        modes: vec![VarMode { id: light, name: "Light".into() }],
        vars: vec![
            Variable { id: brand, name: "Brand/primary".into(), kind: VarKind::Color, values: vec![VarValue::Color(Color32::from_rgb(10, 20, 30))], scopes: vec![VarScope::Fill, VarScope::Effect], ..Variable::default() },
            // an alias follows another variable
            Variable { id: accent, name: "Brand/accent".into(), kind: VarKind::Color, values: vec![VarValue::Alias(brand)], ..Variable::default() },
            Variable { id: radius, name: "radius".into(), kind: VarKind::Number, values: vec![VarValue::Number(6.0)], scopes: vec![VarScope::Radius], ..Variable::default() },
            Variable { id: blur, name: "soft".into(), kind: VarKind::Number, values: vec![VarValue::Number(12.0)], scopes: vec![VarScope::Effect], ..Variable::default() },
        ],
        ..Collection::default()
    }];
    // an extension with its own mode that overrides the brand colour
    a.variables.push(Collection { id: ext, name: "Dark theme".into(), modes: vec![VarMode { id: dark, name: "Dark".into() }, VarMode { id: dark2, name: "Darker".into() }], extends: Some(base), overrides: vec![(brand, dark, VarValue::Color(Color32::from_rgb(200, 210, 220)))], ..Collection::default() });
    let f = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let r = rect_at(&mut a, 10.0, 10.0);
    let (fi, ri) = (a.index_of(f).unwrap(), a.index_of(r).unwrap());
    a.shapes[ri].parent = Some(f);
    a.shapes[ri].fills[0].var = Some(accent);
    a.shapes[ri].effects = vec![Effect { var_color: Some(brand), var_blur: Some(blur), ..Effect::new(EffectKind::DropShadow) }];
    a.shapes[ri].grids = vec![LayoutGrid { var_color: Some(accent), ..LayoutGrid::new(GridKind::Grid) }];
    a.settle();
    assert_eq!(a.shapes[ri].fills[0].kind, PaintKind::Solid(Color32::from_rgb(10, 20, 30)), "the alias follows the colour it points to");
    assert_eq!(a.shapes[ri].effects[0].color, Color32::from_rgb(10, 20, 30));
    assert_eq!(a.shapes[ri].effects[0].blur, 12.0);
    assert_eq!(a.shapes[ri].grids[0].color, Color32::from_rgb(10, 20, 30));
    // the frame chooses the extension's first mode: the alias reaches the override through the colour it points to
    a.shapes[fi].modes = vec![(ext, dark)];
    a.settle();
    assert_eq!(a.shapes[ri].fills[0].kind, PaintKind::Solid(Color32::from_rgb(200, 210, 220)), "the extension's override wins");
    // a mode without an override falls back to the parent's value
    a.shapes[fi].modes = vec![(ext, dark2)];
    a.settle();
    assert_eq!(a.shapes[ri].fills[0].kind, PaintKind::Solid(Color32::from_rgb(10, 20, 30)));
    // a circle of aliases ends instead of looping
    a.variables[0].vars[0].values = vec![VarValue::Alias(accent)];
    assert_eq!(a.variable_value(brand, ri), None);
    // groups come from the name; scopes limit where a variable is offered
    let v = &a.variables[0].vars[1];
    assert_eq!(v.group(), ("Brand", "accent"));
    assert!(a.variables[0].vars[2].allows(VarScope::Radius) && !a.variables[0].vars[2].allows(VarScope::Opacity));
    assert!(a.variables[0].vars[1].allows(VarScope::Fill), "no scopes means everywhere");
    // the window draws a plain and an extended collection (with an alias, a scope, a group and an override)
    a.variables[0].vars[0].values = vec![VarValue::Color(Color32::from_rgb(10, 20, 30))];
    a.vars_open = true;
    let wctx = egui::Context::default();
    for k in 0..2 {
        a.var_coll = k;
        for _ in 0..2 {
            let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))), ..Default::default() };
            let _ = wctx.run_ui(raw, |ui| a.variables_window(ui.ctx()));
        }
    }
    assert_eq!(a.variables.len(), 2);
    // deleting a variable turns the aliases of it into plain values
    a.variables[0].vars[0].values = vec![VarValue::Color(Color32::BLACK)];
    a.unbind_everywhere_for_test(brand);
    assert_eq!(a.variables[1].overrides.len(), 0);
    assert!(matches!(a.variables[0].vars[1].values[0], VarValue::Color(_)));
}

#[test]
fn variant_change_keeps_overrides_slots_and_interactive_components_change_variants() {
    let mut a = App::default();
    let mut make = |a: &mut App, x: f32, w: f32| -> (u64, u64, u64) {
        let f = frame(a, x, 0.0, w, 40.0);
        a.add(Kind::Text, Pos2::new(x + 20.0, 10.0));
        let ti = a.shapes.len() - 1;
        a.shapes[ti].parent = Some(f);
        a.shapes[ti].pos = Pos2::new(20.0, 10.0);
        a.shapes[ti].text = "Go".into();
        a.shapes[ti].name = "Label".into();
        let tid = a.shapes[ti].id;
        // a slot frame with a default dot in it
        let slot = frame(a, x, 0.0, 30.0, 30.0);
        let si = a.index_of(slot).unwrap();
        a.shapes[si].parent = Some(f);
        a.shapes[si].pos = Pos2::new(0.0, 5.0);
        a.shapes[si].name = "Slot".into();
        a.shapes[si].slot = true;
        let dot = rect_at(a, x, 0.0);
        let di = a.index_of(dot).unwrap();
        a.shapes[di].parent = Some(slot);
        a.shapes[di].name = "Default".into();
        a.sel = vec![f];
        a.make_component();
        (f, tid, slot)
    };
    let (off, off_text, _) = make(&mut a, 0.0, 80.0);
    let (on, _, _) = make(&mut a, 200.0, 120.0);
    a.sel = vec![off, on];
    a.combine_as_variants();
    let set = a.single().unwrap();
    a.rename_variant_property(set, "Property 1", "State");
    let (oi, ni) = (a.index_of(off).unwrap(), a.index_of(on).unwrap());
    a.set_variant_value(oi, "State", "Off");
    a.set_variant_value(ni, "State", "On");

    let inst = a.instantiate(off, None, Pos2::new(0.0, 300.0)).unwrap();
    // the instance changes its label and puts its own layer into the slot
    let find = |a: &App, name: &str, under: u64| a.shapes.iter().position(|s| s.name == name && a.is_inside(s.id, under)).unwrap();
    let li = find(&a, "Label", inst);
    a.shapes[li].text = "Pay now".into();
    let slot_i = find(&a, "Slot", inst);
    let slot_id = a.shapes[slot_i].id;
    a.add(Kind::Rect, Pos2::new(300.0, 305.0));
    let own = a.shapes.last().unwrap().id;
    let oi2 = a.index_of(own).unwrap();
    a.shapes[oi2].parent = Some(slot_id);
    a.shapes[oi2].name = "Mine".into();
    a.shapes[oi2].src = None;
    // editing the main component's slot content leaves the instance's content alone (a slot is the instance's own)
    let dot_main = a.shapes.iter().position(|s| s.name == "Default" && a.is_inside(s.id, off)).unwrap();
    a.shapes[dot_main].size = Vec2::new(7.0, 7.0);
    let before = a.shapes.clone();
    a.shapes[dot_main].size = Vec2::new(9.0, 9.0);
    a.sync_instances(&before);
    assert!(a.shapes.iter().any(|s| s.name == "Mine" && s.parent == Some(slot_id)));

    // changing the variant keeps the label override and the slot's own content
    a.pick_variant(inst, "State", "On");
    let new_inst = a.shapes.iter().find(|s| s.instance && s.src == Some(on)).unwrap().id;
    let li = find(&a, "Label", new_inst);
    assert_eq!(a.shapes[li].text, "Pay now", "the overridden text survives the variant change");
    let slot_new = a.shapes[find(&a, "Slot", new_inst)].id;
    assert!(a.shapes.iter().any(|s| s.name == "Mine" && s.parent == Some(slot_new)), "the slot's own layer moved to the new variant's slot");
    assert!(!a.shapes.iter().any(|s| s.name == "Default" && s.parent == Some(slot_new)), "and the default content made room for it");
    // an untouched layer follows the new variant (its width came from the main)
    assert_eq!(a.shapes[a.index_of(new_inst).unwrap()].size.x, 120.0);

    // interactive components: a click on a layer of the instance changes it to another variant
    let btn = find(&a, "Label", new_inst);
    a.shapes[btn].link = Some(off);
    a.shapes[btn].action = Action::ChangeTo;
    let page = frame(&mut a, 1000.0, 0.0, 400.0, 400.0);
    let ii = a.index_of(new_inst).unwrap();
    a.shapes[ii].parent = Some(page);
    a.shapes[ii].pos = Pos2::new(10.0, 10.0);
    a.settle();
    a.start_present(page);
    let click = a.world_rect(find(&a, "Label", new_inst)).center();
    a.present_click(click, 1.0);
    assert!(a.shapes.iter().any(|s| s.instance && s.src == Some(off) && a.is_inside(s.id, page)), "the instance is now the Off variant");
    a.exit_present();
    assert!(a.shapes.iter().any(|s| s.instance && s.src == Some(on) && a.is_inside(s.id, page)), "leaving present mode puts the page back");
    let _ = off_text;
}

#[test]
fn libraries_publish_add_insert_and_update() {
    use egui::Color32;
    let path = std::env::temp_dir().join(format!("drui-test-{}.drlib", std::process::id()));
    // the library file: a button component that uses a colour style and a colour variable
    let mut lib = App::default();
    lib.styles.push(ColorStyle { id: 500, name: "Brand".into(), color: Color32::from_rgb(255, 0, 0), description: String::new() });
    lib.next_id = 600;
    let var = lib.new_id();
    lib.variables = vec![Collection { id: lib.new_id(), name: "Theme".into(), modes: vec![VarMode { id: lib.new_id(), name: "Mode 1".into() }], vars: vec![Variable { id: var, name: "bg".into(), kind: VarKind::Color, values: vec![VarValue::Color(Color32::BLUE)], ..Variable::default() }], ..Collection::default() }];
    let f = frame(&mut lib, 0.0, 0.0, 120.0, 40.0);
    lib.add(Kind::Text, Pos2::new(10.0, 10.0));
    let ti = lib.shapes.len() - 1;
    lib.shapes[ti].parent = Some(f);
    lib.shapes[ti].text = "Buy".into();
    lib.shapes[ti].fills = vec![Paint { style: Some(500), ..Paint::solid(egui::Color32::RED) }];
    let fi = lib.index_of(f).unwrap();
    lib.shapes[fi].fills = vec![Paint { var: Some(var), ..Paint::solid(Color32::BLUE) }];
    lib.sel = vec![f];
    lib.make_component();
    lib.publish_library_to(path.clone());
    assert_eq!(lib.lib_version, 1);

    // another file adds it: the style and variable come in under new ids, the component is inserted on use
    let mut app = App::default();
    app.next_id = 5000;
    app.add_library_from(path.clone());
    assert_eq!(app.libs.len(), 1);
    assert_eq!(app.styles.len(), 1);
    assert_ne!(app.styles[0].id, 500, "the style has an id of this file");
    assert_eq!(app.variables.len(), 1);
    let root = app.library_file(app.libs[0].id).unwrap().components[0].root;
    let lib_id = app.libs[0].id;
    app.insert_library_component(lib_id, root);
    let main = app.shapes.iter().find(|s| s.component && s.lib.is_some()).unwrap().id;
    let inst = app.shapes.iter().find(|s| s.instance && s.src == Some(main)).unwrap().id;
    let label = app.shapes.iter().position(|s| s.kind == Kind::Text && app.is_inside(s.id, inst)).unwrap();
    assert_eq!(app.shapes[label].text, "Buy");
    assert_eq!(app.shapes[label].fills[0].style, Some(app.styles[0].id), "the component uses this file's copy of the style");
    let inst_i = app.index_of(inst).unwrap();
    assert_eq!(app.shapes[inst_i].fills[0].var, Some(app.variables[0].vars[0].id));
    // inserting again reuses the inserted component
    app.insert_library_component(lib_id, root);
    assert_eq!(app.shapes.iter().filter(|s| s.component && s.lib.is_some()).count(), 1);

    // the library changes and is published again: Update brings the new text into the copy and its instance
    lib.shapes[ti].text = "Buy now".into();
    lib.publish_library_to(path.clone());
    assert!(!app.library_has_update(lib_id), "the file has not been read again yet");
    app.refresh_library(lib_id);
    assert!(app.library_has_update(lib_id));
    app.update_from_library(lib_id);
    let label = app.shapes.iter().position(|s| s.kind == Kind::Text && app.is_inside(s.id, inst)).unwrap();
    assert_eq!(app.shapes[label].text, "Buy now");
    assert!(!app.library_has_update(lib_id));
    // a file never adds itself
    app.lib_id = lib_id;
    app.add_library_from(path.clone());
    assert_eq!(app.libs.len(), 1);
    let _ = std::fs::remove_file(path);
}

#[test]
fn masks_clip_to_the_real_outline() {
    use egui::Color32;
    let ctx = egui::Context::default();
    let mut a = App::default();
    // an ellipse mask under a red square: the corners of the square's box are outside the ellipse
    a.add(Kind::Ellipse, Pos2::new(0.0, 0.0));
    let e = a.shapes.len() - 1;
    a.shapes[e].size = Vec2::new(100.0, 100.0);
    a.shapes[e].mask = true;
    a.add(Kind::Rect, Pos2::new(0.0, 0.0));
    let r = a.shapes.len() - 1;
    a.shapes[r].size = Vec2::new(100.0, 100.0);
    a.shapes[r].fills = vec![Paint::solid(egui::Color32::RED)];
    a.shapes[r].strokes.clear();
    let mut svg = None;
    let view = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| svg = Some(a.svg_scene(ui.ctx(), view, Vec2::splat(100.0), Color32::WHITE)));
    let svg = svg.unwrap();
    assert!(svg.contains("<mask") && svg.contains("mask=\"url(#mk"), "{svg}");
    let img = crate::export::rasterize_scene(&svg, 100, 100).unwrap();
    let at = |x: usize, y: usize| img.pixels[y * 100 + x];
    assert!(at(50, 50).r() > 200 && at(50, 50).g() < 60, "the middle shows the square");
    assert!(at(3, 3).g() > 200, "the corner is outside the mask: the white background shows");
}

#[test]
fn pasting_svg_makes_a_frame_of_paths() {
    fn f_id(a: &App) -> u64 {
        a.shapes.iter().find(|s| s.kind == Kind::Frame).unwrap().id
    }
    use egui::Color32;
    let mut a = App::default();
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80" viewBox="0 0 120 80">
        <g transform="translate(10 10)"><rect width="40" height="30" rx="4" fill="#ff0000"/></g>
        <circle cx="90" cy="40" r="20" fill="none" stroke="#0000ff" stroke-width="3" stroke-dasharray="4 2"/>
        <path d="M5 70 Q 30 50 55 70" stroke="#00aa00" fill="none" stroke-width="2" stroke-linecap="round"/>
        <linearGradient id="g"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient>
        <rect x="60" y="5" width="20" height="20" fill="url(#g)"/></svg>"##;
    a.paste(svg);
    let f = a.single().unwrap();
    assert_eq!(a.shapes[f].kind, Kind::Frame);
    assert_eq!(a.shapes[f].size, Vec2::new(120.0, 80.0));
    let kids: Vec<&Shape> = a.shapes.iter().filter(|s| s.parent == Some(a.shapes[f].id)).collect();
    assert_eq!(kids.len(), 4, "one path layer per drawn shape");
    assert!(kids.iter().all(|s| s.kind == Kind::Path));
    // the translated rectangle sits where the group put it, red
    let red = kids.iter().find(|s| matches!(s.fills.first().map(|p| &p.kind), Some(PaintKind::Solid(c)) if *c == Color32::from_rgb(255, 0, 0))).unwrap();
    assert!((red.pos.x - 10.0).abs() < 0.5 && (red.pos.y - 10.0).abs() < 0.5 && (red.size.x - 40.0).abs() < 0.5, "{:?} {:?}", red.pos, red.size);
    // strokes keep colour, width, dashes and caps
    let blue = kids.iter().find(|s| s.stroke_w == 3.0).unwrap();
    assert_eq!(blue.dash_pattern, vec![4.0, 2.0]);
    assert!(blue.fills.is_empty());
    let green = kids.iter().find(|s| s.stroke_w == 2.0).unwrap();
    assert_eq!(green.stroke_cap, [StrokeCap::Round; 2]);
    assert!(kids.iter().any(|s| matches!(s.fills.first().map(|p| &p.kind), Some(PaintKind::Gradient(g)) if g.stops.len() == 2)));
    // copying as SVG gives text that pastes back
    a.sel = vec![f_id(&a)];
    let ctx = egui::Context::default();
    let mut copied = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        a.copy_as_svg(ui.ctx());
        copied = ui.ctx().output(|o| o.commands.iter().find_map(|c| if let egui::OutputCommand::CopyText(t) = c { Some(t.clone()) } else { None }));
    });
    let text = copied.expect("the SVG went to the clipboard");
    assert!(text.trim_start().starts_with("<svg"));
    let before = a.shapes.len();
    a.paste(&text);
    assert!(a.shapes.len() > before, "the copied SVG pastes back as layers");
    // plain text is still a text layer
    a.paste("hello");
    assert_eq!(a.shapes[a.single().unwrap()].kind, Kind::Text);
}

#[test]
fn lasso_selects_what_it_encloses() {
    let mut a = App::default();
    let r1 = rect_at(&mut a, 10.0, 10.0);
    let r2 = rect_at(&mut a, 100.0, 10.0);
    let r3 = rect_at(&mut a, 60.0, 80.0);
    // a triangle round the first and the third rectangle, away from the second
    let tri = [Pos2::new(0.0, 0.0), Pos2::new(40.0, 0.0), Pos2::new(90.0, 100.0), Pos2::new(0.0, 100.0)];
    let hits = a.lasso_hits(&tri);
    assert!(hits.contains(&r1) && hits.contains(&r3) && !hits.contains(&r2), "{hits:?}");
    assert!(a.lasso_hits(&tri[..2]).is_empty(), "two points enclose nothing");
    // dragging through the canvas with the lasso tool selects them
    let mut h = Harness::new();
    a.lasso_tool = true;
    a.rulers = false;
    a.snap = false;
    h.frame(&mut a, vec![]);
    let path = [Pos2::new(0.0, 0.0), Pos2::new(40.0, 0.0), Pos2::new(90.0, 100.0), Pos2::new(0.0, 100.0), Pos2::new(1.0, 1.0)];
    h.mv(&mut a, scr(path[0].x, path[0].y));
    h.frame(&mut a, vec![Harness::button(scr(path[0].x, path[0].y), true)]);
    for p in &path[1..] {
        h.frame(&mut a, vec![egui::Event::PointerMoved(scr(p.x, p.y))]);
    }
    h.frame(&mut a, vec![Harness::button(scr(1.0, 1.0), false)]);
    h.frame(&mut a, vec![]);
    assert!(a.sel.contains(&r1) && a.sel.contains(&r3) && !a.sel.contains(&r2), "{:?}", a.sel);
}

#[test]
fn pattern_and_shader_fills_render() {
    use egui::Color32;
    let ctx = egui::Context::default();
    let mut a = App::default();
    // the tile: a 10 x 10 red square far away from the target
    a.add(Kind::Rect, Pos2::new(500.0, 500.0));
    let t = a.shapes.len() - 1;
    a.shapes[t].size = Vec2::splat(10.0);
    a.shapes[t].fills = vec![Paint::solid(egui::Color32::RED)];
    a.shapes[t].strokes.clear();
    let tile = a.shapes[t].id;
    // the target: 100 x 100 with a pattern of it, 10 apart
    a.add(Kind::Rect, Pos2::new(0.0, 0.0));
    let r = a.shapes.len() - 1;
    a.shapes[r].size = Vec2::splat(100.0);
    a.shapes[r].strokes.clear();
    a.shapes[r].fills = vec![Paint { kind: PaintKind::Pattern(PatternFill { source: Some(tile), space: Vec2::splat(10.0), ..PatternFill::default() }), ..Paint::solid(Color32::BLACK) }];
    let rid = a.shapes[r].id;
    let render = |a: &App| -> egui::ColorImage {
        let mut svg = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| svg = a.svg_for(ui.ctx(), &[rid]));
        let (svg, _) = svg.unwrap();
        assert!(svg.contains("<pattern"), "{svg}");
        crate::export::rasterize_scene(&svg, 100, 100).unwrap()
    };
    assert!(a.shapes[r].needs_sprite());
    let img = render(&a);
    let at = |x: usize, y: usize| img.pixels[y * 100 + x];
    assert!(at(5, 5).r() > 200 && at(5, 5).g() < 60, "the first tile is the red square: {:?}", at(5, 5));
    assert!(at(15, 5).a() < 20, "the gap between tiles is empty: {:?}", at(15, 5));
    assert!(at(25, 5).r() > 200, "the next tile starts 20 px on: {:?}", at(25, 5));
    // shaders: every kind renders, and the noise ones are not flat
    for kind in ShaderKind::ALL.map(|k| k.0) {
        a.shapes[r].fills = vec![Paint { kind: PaintKind::Shader(ShaderFill { kind, a: Color32::BLACK, b: Color32::WHITE, ..ShaderFill::default() }), ..Paint::solid(Color32::BLACK) }];
        let img = render(&a);
        let distinct = img.pixels.iter().map(|p| p.r()).collect::<std::collections::HashSet<_>>().len();
        assert!(distinct > 2, "{kind:?} is not flat ({distinct} grey levels)");
    }
    // both survive a save and load
    let doc = a.doc();
    let back: Doc = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert!(matches!(back.pages[0].shapes.iter().find(|s| s.id == rid).unwrap().fills[0].kind, PaintKind::Shader(_)));
}

#[test]
fn image_crop_shows_the_cropped_part_and_has_edge_handles() {
    use egui::Color32;
    let ctx = egui::Context::default();
    let mut a = App::default();
    // an 8 x 4 picture: red on the left half, blue on the right
    let png = {
        let mut buf = std::io::Cursor::new(vec![]);
        let img = image::RgbaImage::from_fn(8, 4, |x, _| if x < 4 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) });
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    };
    let id = a.register_image("halves", png).unwrap();
    a.add(Kind::Rect, Pos2::new(0.0, 0.0));
    let r = a.shapes.len() - 1;
    a.shapes[r].size = Vec2::new(80.0, 40.0);
    a.shapes[r].strokes.clear();
    a.shapes[r].fills = vec![Paint { kind: PaintKind::Image(id), image: ImageFill { mode: ImageMode::Crop, crop: [0.5, 0.0, 1.0, 1.0], ..ImageFill::default() }, ..Paint::default() }];
    let rid = a.shapes[r].id;
    let render = |a: &App| -> egui::ColorImage {
        let mut svg = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| svg = a.svg_for(ui.ctx(), &[rid]));
        crate::export::rasterize_scene(&svg.unwrap().0, 80, 40).unwrap()
    };
    // cropped to the right half: all of the layer is blue
    let img = render(&a);
    assert!(img.pixels[10 * 80 + 12].b() > 150 && img.pixels[10 * 80 + 12].r() < 80, "{:?}", img.pixels[10 * 80 + 12]);
    assert!(img.pixels[10 * 80 + 75].b() > 150);
    // the whole picture again: red on the left, blue on the right
    a.shapes[r].fills[0].image.crop = [0.0, 0.0, 1.0, 1.0];
    let img = render(&a);
    assert!(img.pixels[10 * 80 + 12].r() > 150 && img.pixels[10 * 80 + 75].b() > 150);
    let _ = egui::Color32::RED;
    // the left edge handle crops the image from the left as it is dragged in
    a.sel = vec![rid];
    a.zoom = 1.0;
    a.pan = Vec2::ZERO;
    let o = Pos2::ZERO;
    let handle = a.crop_handles(o, r)[0];
    let Some(crate::Drag::Crop(d)) = a.crop_handle_at(o, handle) else { panic!("a press on the handle starts a crop drag") };
    a.crop_drag(d, d.from + Vec2::new(40.0, 0.0));
    assert!((a.shapes[r].fills[0].image.crop[0] - 0.5).abs() < 1e-4, "half the box in = half the picture: {:?}", a.shapes[r].fills[0].image.crop);
    // a layer that is not in Crop mode has no handles
    a.shapes[r].fills[0].image.mode = ImageMode::Fill;
    assert!(a.crop_handle_at(o, handle).is_none());
}

#[test]
fn paint_and_shadow_blends_frame_blur_with_children_and_glass() {
    use egui::Color32;
    let ctx = egui::Context::default();
    let mut a = App::default();
    let mut svg_of = |a: &App, id: u64| -> String {
        let mut svg = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| svg = a.svg_for(ui.ctx(), &[id]));
        svg.unwrap().0
    };
    a.add(Kind::Rect, Pos2::new(0.0, 0.0));
    let r = a.shapes.len() - 1;
    a.shapes[r].size = Vec2::new(60.0, 40.0);
    a.shapes[r].fills = vec![Paint { blend: BlendMode::Multiply, ..Paint::solid(egui::Color32::RED) }, Paint::solid(Color32::BLUE)];
    a.shapes[r].effects = vec![Effect { blend: BlendMode::Screen, ..Effect::new(EffectKind::DropShadow) }];
    let rid = a.shapes[r].id;
    let svg = svg_of(&a, rid);
    assert!(svg.contains("mix-blend-mode:multiply") && svg.contains("mix-blend-mode:screen"), "{svg}");
    assert_eq!(svg.matches("mix-blend-mode").count(), 2, "only the paints that ask for it blend");
    assert!(crate::export::render_png(&svg, 1.0).is_ok());
    // a layer blur on a frame is one filter round the frame and its children
    a.shapes.clear();
    let f = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let k = rect_at(&mut a, 10.0, 10.0);
    let (fi, ki) = (a.index_of(f).unwrap(), a.index_of(k).unwrap());
    a.shapes[ki].parent = Some(f);
    a.shapes[fi].effects = vec![Effect::new(EffectKind::LayerBlur)];
    let svg = svg_of(&a, f);
    assert_eq!(svg.matches("feGaussianBlur").count(), 1, "{svg}");
    let blur_at = svg.find("<g filter=").expect("a blur group");
    let kid_at = svg.rfind("<path").unwrap();
    let close_at = svg.rfind("</g>").unwrap();
    assert!(blur_at < kid_at && kid_at < close_at, "the child is inside the blurred group: {svg}");
    // glass: the tint and the light edge are drawn, and it needs a sprite
    a.shapes[fi].effects = vec![Effect::new(EffectKind::Glass)];
    assert!(a.shapes[fi].needs_sprite() || a.shapes[fi].kind == Kind::Frame);
    let svg = svg_of(&a, f);
    assert!(svg.contains("linearGradient") && svg.contains("stroke=\"url(#g"), "{svg}");
    assert!(crate::export::render_png(&svg, 1.0).is_ok());
    // a layer's CSS gets the glass in its own terms
    let css = a.css_of(fi);
    assert!(css.contains("backdrop-filter: blur(8px);") && css.contains("rgba(255, 255, 255,"), "{css}");
}

#[test]
fn motion_scale_and_arc_tracks() {
    use crate::anim::{commit, evaluate};
    let mut a = App::default();
    let r = rect_at(&mut a, 0.0, 0.0);
    let i = a.index_of(r).unwrap();
    a.shapes[i].size = Vec2::new(20.0, 10.0);
    let key = |t: f32, v: f32| Key { t, v: [v, 0.0, 0.0, 0.0], ease: Easing::default() };
    // a straight move from x 0 to 100 in a second, scaled from 100% to 200%
    a.shapes[i].tracks = vec![
        Track { prop: Prop::X, keys: vec![key(0.0, 0.0), key(1000.0, 100.0)] },
        Track { prop: Prop::Scale, keys: vec![key(0.0, 1.0), key(1000.0, 2.0)] },
    ];
    let mid = evaluate(&a.shapes, 500.0);
    let m = &mid[i];
    let ease = Easing::default().at(0.5);
    assert!((m.size.x - 20.0 * (1.0 + ease)).abs() < 1e-2, "scaled about the middle: {:?}", m.size);
    // the centre follows the X track: pos.x + size.x / 2 = 100 * ease + 10 (the centre of the unscaled rectangle) 
    let c = m.pos.x + m.size.x / 2.0;
    assert!((c - (100.0 * ease + 10.0)).abs() < 1e-2, "{c}");
    // the page's own values are untouched by looking at the page at a time
    assert_eq!(a.shapes[i].size, Vec2::new(20.0, 10.0));
    // an Arc bends the move: halfway the layer is off the straight line, by 25% of the distance at a bend of 100%
    a.shapes[i].tracks = vec![Track { prop: Prop::X, keys: vec![key(0.0, 0.0), key(1000.0, 100.0)] }, Track { prop: Prop::Y, keys: vec![key(0.0, 0.0), key(1000.0, 0.0)] }, Track { prop: Prop::Arc, keys: vec![key(0.0, 1.0), key(1000.0, 1.0)] }];
    let at = |t: f32| evaluate(&a.shapes, t)[i].pos;
    let (start, end) = (at(0.0), at(1000.0));
    assert!(start.y.abs() < 1e-3 && end.y.abs() < 1e-3, "the ends stay on the line: {start:?} {end:?}");
    let half = at(500.0);
    let f = half.x / 100.0;
    assert!((half.y.abs() - 100.0 * 4.0 * f * (1.0 - f)).abs() < 0.5, "a parabola of height 100 over a 100 long move: {half:?}");
    assert!(half.y.abs() > 20.0);
    // viewing and committing a frame without an edit leaves the page's size alone
    a.shapes[i].tracks = vec![Track { prop: Prop::Scale, keys: vec![key(0.0, 3.0)] }];
    let base = a.shapes.clone();
    let shown = evaluate(&base, 0.0);
    assert_eq!(shown[i].size, Vec2::new(60.0, 30.0));
    let after = commit(&base, &shown, shown.clone(), 0.0, false);
    assert_eq!(after[i].size, Vec2::new(20.0, 10.0), "the scale is not baked into the layer");
    assert_eq!(after[i].pos, base[i].pos);
    // the panel field writes a keyframe at the playhead
    assert!(Prop::Scale.is_virtual() && Prop::Arc.is_virtual() && !Prop::X.is_virtual());
}

#[test]
fn lottie_export_has_layers_keyframes_and_valid_json() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 200.0, 100.0);
    a.add(Kind::Rect, Pos2::new(10.0, 10.0));
    let r = a.shapes.len() - 1;
    a.shapes[r].parent = Some(f);
    a.shapes[r].size = Vec2::new(40.0, 20.0);
    a.shapes[r].radius = [4.0; 4];
    a.shapes[r].fills = vec![Paint::solid(egui::Color32::from_rgb(255, 0, 0))];
    a.add(Kind::Ellipse, Pos2::new(100.0, 40.0));
    let e = a.shapes.len() - 1;
    a.shapes[e].parent = Some(f);
    a.shapes[e].size = Vec2::splat(30.0);
    a.add(Kind::Text, Pos2::new(20.0, 70.0));
    let t = a.shapes.len() - 1;
    a.shapes[t].parent = Some(f);
    a.shapes[t].text = "Hi".into();
    a.shapes[t].size = Vec2::new(30.0, 20.0);
    a.shapes[t].fit = TextFit::Fixed;
    // the rectangle moves and fades over a second
    let key = |t: f32, v: f32| Key { t, v: [v, 0.0, 0.0, 0.0], ease: Easing::default() };
    a.shapes[r].tracks = vec![Track { prop: Prop::X, keys: vec![key(0.0, 10.0), key(1000.0, 110.0)] }, Track { prop: Prop::Opacity, keys: vec![key(0.0, 1.0), key(1000.0, 0.0)] }];
    a.pages[a.cur].anim_ms = 1000.0;
    a.settle();
    let mut out = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| out = a.lottie_json(ui.ctx(), &[f]));
    let json = out.expect("a Lottie file");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["fr"], 30.0);
    assert_eq!(v["op"], 30);
    assert_eq!((v["w"].as_f64().unwrap(), v["h"].as_f64().unwrap()), (200.0, 100.0));
    let layers = v["layers"].as_array().unwrap();
    // the frame, the rectangle, the ellipse and the text outlines: four layers, the top one first
    assert_eq!(layers.len(), 4, "{json}");
    let names: Vec<&str> = layers.iter().map(|l| l["nm"].as_str().unwrap()).collect();
    assert_eq!(names.last().copied(), Some(a.shapes[a.index_of(f).unwrap()].name.as_str()), "the frame is the bottom layer: {names:?}");
    let rect = layers.iter().find(|l| l["nm"] == a.shapes[r].name.as_str()).unwrap();
    // the position and the opacity are keyframes, one per frame; the rotation never changes
    assert_eq!(rect["ks"]["p"]["a"], 1);
    assert_eq!(rect["ks"]["p"]["k"].as_array().unwrap().len(), 30);
    assert_eq!(rect["ks"]["o"]["a"], 1);
    assert_eq!(rect["ks"]["r"]["a"], 0);
    let first = &rect["ks"]["p"]["k"][0]["s"];
    assert_eq!((first[0].as_f64().unwrap(), first[1].as_f64().unwrap()), (30.0, 20.0), "the centre of the rectangle at the start");
    let items = rect["shapes"][0]["it"].as_array().unwrap();
    assert!(items.iter().any(|i| i["ty"] == "rc") && items.iter().any(|i| i["ty"] == "fl"), "{items:?}");
    let text = layers.iter().find(|l| l["nm"] == a.shapes[t].name.as_str()).unwrap();
    assert!(text["shapes"][0]["it"].as_array().unwrap().iter().any(|i| i["ty"] == "sh"), "the text became outlines");
    if let Ok(dir) = std::env::var("LOTTIE_OUT") {
        std::fs::write(format!("{dir}/check.json"), &json).unwrap();
    }
    let mut none = Some(String::new());
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| none = a.lottie_json(ui.ctx(), &[]));
    assert!(none.is_none());
}

#[test]
fn text_animation_reveals_units_and_animation_styles_are_saved() {
    use crate::anim::evaluate;
    let mut a = App::default();
    a.add(Kind::Text, Pos2::new(0.0, 0.0));
    let i = a.shapes.len() - 1;
    a.shapes[i].text = "ab cd\nef".into();
    // by character, a fade of 100 ms each, one starting every 100 ms after 0
    a.shapes[i].text_anim = TextAnim { unit: 1, kind: 0, start: 0.0, per_unit: 100.0, duration: 100.0 };
    let fades = |a: &App, t: f32| -> Vec<u8> {
        let ev = evaluate(&a.shapes, t);
        ev[i].display_chars().iter().filter(|c| c.0 != '\n').map(|c| c.1.fade).collect()
    };
    assert_eq!(fades(&a, 0.0), vec![0; 7], "nothing shows at the start");
    assert_eq!(fades(&a, 150.0)[..2], [255, 128], "the first is in, the second half-way");
    assert_eq!(fades(&a, 10_000.0), vec![255; 7]);
    assert!(a.shapes[i].display_chars().iter().all(|c| c.1.fade == 255), "the page itself shows everything");
    // by word: both letters of a word share a fade
    a.shapes[i].text_anim = TextAnim { unit: 2, kind: 1, start: 0.0, per_unit: 500.0, duration: 0.0 };
    let w = fades(&a, 600.0);
    assert_eq!(w, vec![255, 255, 255, 255, 255, 0, 0], "typewriter: the first two words are in, the third not yet ({w:?})");
    // by line
    a.shapes[i].text_anim = TextAnim { unit: 3, kind: 1, start: 0.0, per_unit: 1000.0, duration: 0.0 };
    assert_eq!(fades(&a, 500.0), vec![255, 255, 255, 255, 255, 0, 0]);
    assert!(a.has_motion(), "an animated text counts as motion");
    // animation styles are saved with the file and published with a library
    a.ext.anims.push(AnimStyle { id: 77, name: "Snappy".into(), ease: Easing::EaseOutBack, duration: 250.0 });
    let doc = a.doc();
    let back: Doc = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(back.ext_styles.anims.len(), 1);
    assert_eq!(a.build_library().ext.anims[0].name, "Snappy");
}

#[test]
fn audio_clips_keep_a_waveform_and_survive_save_and_load() {
    let mut a = App::default();
    // one second of a 440 Hz tone, 16-bit mono at 8 kHz, as a WAV file
    let wav = {
        let rate = 8000u32;
        let pcm: Vec<u8> = (0..rate).flat_map(|n| ((((n as f32) * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5 * i16::MAX as f32) as i16).to_le_bytes()).collect();
        let mut v = b"RIFF".to_vec();
        v.extend((36 + pcm.len() as u32).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend([1, 0, 1, 0]);
        v.extend(rate.to_le_bytes());
        v.extend((rate * 2).to_le_bytes());
        v.extend([2, 0, 16, 0]);
        v.extend(b"data");
        v.extend((pcm.len() as u32).to_le_bytes());
        v.extend(pcm);
        v
    };
    a.pages[0].anim_ms = 500.0;
    assert!(a.add_audio_bytes("tone", &wav));
    assert!(!a.add_audio_bytes("broken", b"not a sound"), "a file that is not audio is refused");
    let clip = a.pages[0].audio[0].clone();
    assert_eq!(a.pages[0].audio.len(), 1);
    let (peaks, ms) = a.clip_peaks(clip.id).unwrap().clone();
    assert!((ms - 1000.0).abs() < 5.0, "one second long: {ms}");
    assert_eq!(peaks.len(), crate::audio::PEAKS);
    assert!(peaks.iter().all(|p| (0.3..=0.6).contains(p)), "a steady tone at half volume: {:?}", &peaks[..4]);
    assert!(a.pages[0].anim_ms >= 1000.0, "the timeline grew to hold the sound");
    // the sound is saved with the page
    let doc = a.doc();
    let back: Doc = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(back.pages[0].audio.len(), 1);
    assert_eq!(back.pages[0].audio[0].name, "tone");
    // the waveform is read again from the saved data
    a.audio_peaks.clear();
    assert!(a.clip_peaks(clip.id).is_some());
    // the timeline draws the sound's row, and the Add audio / Style controls
    a.audio_stop();
    let r = rect_at(&mut a, 0.0, 0.0);
    let i = a.index_of(r).unwrap();
    a.shapes[i].tracks = vec![Track { prop: Prop::X, keys: vec![Key { t: 0.0, v: [0.0; 4], ease: Easing::default() }, Key { t: 500.0, v: [50.0, 0.0, 0.0, 0.0], ease: Easing::default() }] }];
    a.sel = vec![r];
    a.key_sel = Some((r, Prop::X, 0.0));
    let ctx = egui::Context::default();
    for _ in 0..2 {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 300.0))), ..Default::default() };
        let _ = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| a.timeline(ui));
        });
    }
    assert_eq!(a.pages[0].audio.len(), 1);
}

#[test]
fn dotlottie_is_a_valid_zip_with_a_manifest() {
    assert_eq!(crate::lottie::crc32_for_test(b"123456789"), 0xCBF4_3926, "the standard CRC-32 check value");
    let zip = crate::lottie::zip_stored(&[("manifest.json", b"{}"), ("animations/animation.json", b"{\"v\":1}")]);
    assert_eq!(&zip[..4], b"PK\x03\x04");
    let end = zip.len() - 22;
    assert_eq!(&zip[end..end + 4], b"PK\x05\x06");
    assert_eq!(u16::from_le_bytes([zip[end + 10], zip[end + 11]]), 2, "two entries");
    if let Ok(dir) = std::env::var("ZIP_OUT") {
        std::fs::write(format!("{dir}/check.zip"), &zip).unwrap();
    }
}


#[test]
fn prototype_html_has_screens_hot_spots_and_valid_data() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let home = frame(&mut a, 0.0, 0.0, 200.0, 300.0);
    let detail = frame(&mut a, 400.0, 0.0, 200.0, 300.0);
    let btn = rect_at(&mut a, 20.0, 40.0);
    let bi = a.index_of(btn).unwrap();
    a.shapes[bi].parent = Some(home);
    a.shapes[bi].size = Vec2::new(80.0, 30.0);
    a.shapes[bi].link = Some(detail);
    a.shapes[bi].transition = Transition::SlideLeft;
    a.shapes[bi].duration = 0.4;
    a.shapes[bi].more.push(Interaction { trigger: Trigger::KeyPress, key: "Enter".into(), action: Action::OpenLink, url: "https://example.com".into(), link: Some(detail), ..Interaction::default() });
    let mut html = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| html = a.prototype_html(ui.ctx(), Some(home)));
    let html = html.unwrap();
    assert!(html.contains(&format!("id=\"s{home}\"")) && html.contains(&format!("id=\"s{detail}\"")), "one screen per frame");
    assert!(html.contains("<svg"), "the screens are SVG");
    let a_ix = html.split("const D = ").nth(1).unwrap().split(";\ndocument.getElementById('stage')").next().unwrap();
    let d: serde_json::Value = serde_json::from_str(a_ix).expect("the data is JSON");
    assert_eq!(d["start"], home);
    let spots = d["frames"][home.to_string()]["spots"].as_array().unwrap();
    assert_eq!(spots.len(), 1);
    assert_eq!((spots[0]["x"].as_f64().unwrap(), spots[0]["y"].as_f64().unwrap(), spots[0]["w"].as_f64().unwrap()), (20.0, 40.0, 80.0), "relative to its frame");
    let ix = spots[0]["ix"].as_array().unwrap();
    assert_eq!((ix[0]["on"].as_str(), ix[0]["do"].as_str(), ix[0]["tr"].as_str(), ix[0]["ms"].as_f64()), (Some("click"), Some("nav"), Some("left"), Some(400.0)));
    assert_eq!((ix[1]["on"].as_str(), ix[1]["do"].as_str(), ix[1]["url"].as_str()), (Some("key"), Some("link"), Some("https://example.com")));
    assert!(ix[0]["ease"].as_str().unwrap().starts_with("linear(0.0000, "), "an easing becomes a CSS linear() curve");
    // variables: Set variable and conditions reach the player
    let vid = a.new_id();
    a.variables.push(Collection { id: 900, name: "Game".into(), modes: vec![VarMode { id: 901, name: "Mode 1".into() }], vars: vec![Variable { id: vid, name: "count".into(), kind: VarKind::Number, values: vec![VarValue::Number(2.0)], ..Variable::default() }], ..Collection::default() });
    a.shapes[bi].more.push(Interaction { trigger: Trigger::Click, action: Action::SetVariable, var: Some(vid), value: "count + 1".into(), condition: "count < 5".into(), ..Interaction::default() });
    let mut html2 = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| html2 = a.prototype_html(ui.ctx(), Some(home)));
    let html2 = html2.unwrap();
    let d2: serde_json::Value = serde_json::from_str(html2.split("const D = ").nth(1).unwrap().split(";
document.getElementById('stage')").next().unwrap()).unwrap();
    assert_eq!(d2["vars"]["count"], 2.0);
    assert_eq!(d2["vars"]["Game / count"], 2.0);
    let set = d2["frames"][home.to_string()]["spots"][0]["ix"].as_array().unwrap().iter().find(|i| i["do"] == "set").expect("a set action");
    assert_eq!((set["var"].as_str(), set["expr"].as_str(), set["if"].as_str()), (Some("count"), Some("count + 1"), Some("count < 5")));
    // the script is valid JavaScript (checked with node when it is installed)
    if let Some(script) = html2.split("<script>").nth(1).and_then(|r| r.split("</script>").next()) {
        let path = std::env::temp_dir().join(format!("drui-player-{}.js", std::process::id()));
        std::fs::write(&path, script).unwrap();
        if let Ok(out) = std::process::Command::new("node").arg("--check").arg(&path).output() {
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        }
        let _ = std::fs::remove_file(path);
    }
    if let Ok(dir) = std::env::var("HTML_OUT") {
        std::fs::write(format!("{dir}/proto.html"), &html).unwrap();
    }
    // no frame, no prototype
    a.shapes.clear();
    let mut none = Some(String::new());
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| none = a.prototype_html(ui.ctx(), None));
    assert!(none.is_none());
}

#[test]
fn vertex_radius_branches_and_the_bend_tool() {
    let mut a = App::default();
    let id = a.new_id();
    let mut s = Shape::new(Kind::Path, id, 1, Pos2::ZERO, None);
    let corner = |x: f32, y: f32| PathPt::corner(Pos2::new(x, y));
    s.set_path_world(&[Contour { pts: vec![corner(0.0, 0.0), corner(100.0, 0.0), corner(100.0, 100.0), corner(0.0, 100.0)], closed: true, hole: false }]);
    a.shapes.push(s);
    a.sel = vec![id];
    a.node_edit = true;
    // a radius on the vertex at (100, 0) rounds it: the outline no longer passes through the corner
    let near = |pts: &[Pos2], p: Pos2| pts.iter().any(|q| (*q - p).length() < 0.5);
    let sharp = a.shapes[0].polys(1.0).remove(0).pts;
    assert!(near(&sharp, Pos2::new(100.0, 0.0)));
    a.node_set_radius(0, 1, 20.0);
    assert_eq!(a.node_radius(0, 1), Some(20.0));
    let round = a.shapes[0].polys(1.0).remove(0).pts;
    assert!(!near(&round, Pos2::new(100.0, 0.0)), "the corner is cut");
    assert!(near(&round, Pos2::new(80.0, 0.0)) && near(&round, Pos2::new(100.0, 20.0)), "the cut starts one radius away on both sides");
    // the arc stays inside the square and bulges toward the corner: its middle is about 20 * (1 - 0.707) from the corner's diagonal
    let mid = round.iter().min_by(|p, q| (p.x - p.y.abs() * 0.0 - 100.0).abs().partial_cmp(&(q.x - 100.0).abs()).unwrap()).copied().unwrap();
    assert!(mid.x <= 100.0 + 1e-3 && mid.y >= -1e-3);
    // a radius bigger than the segments allow is limited to half of them
    a.node_set_radius(0, 1, 500.0);
    let capped = a.shapes[0].polys(1.0).remove(0).pts;
    assert!(capped.iter().all(|p| p.x <= 100.0 + 1e-3 && p.y >= -1e-3 && p.x >= -1e-3 && p.y <= 100.0 + 1e-3));
    a.node_set_radius(0, 1, 0.0);
    // a branch starts on the vertex and the two share it: moving the vertex takes the branch along
    let n = a.node_add_branch(0, 2).expect("a branch");
    assert_eq!(a.shapes[0].path.len(), 2);
    assert_eq!(n, 1);
    let w = a.node_points(0);
    assert!((w[1].pts[0].p - w[0].pts[2].p).length() < 1e-3, "the branch leaves from the vertex");
    a.node_move(0, 2, 0, Pos2::new(150.0, 120.0));
    let w = a.node_points(0);
    assert!((w[0].pts[2].p - Pos2::new(150.0, 120.0)).length() < 1e-2);
    assert!((w[1].pts[0].p - w[0].pts[2].p).length() < 1e-2, "the branch moved with the vertex");
    assert!((w[1].pts[1].p - Pos2::new(124.0, 100.0)).length() < 1e-2, "its other end stayed where it was: {:?}", w[1].pts[1].p);
    // bend: pull handles out of a corner, a click makes it sharp again
    assert!(!a.node_is_curved(0, 0));
    a.node_bend(0, 0, Pos2::new(30.0, 0.0));
    assert!(a.node_is_curved(0, 0));
    let w = a.node_points(0);
    assert!((w[0].pts[0].hout + w[0].pts[0].hin).length() < 1e-3, "the handles are symmetric");
    a.node_straighten(0, 0);
    assert!(!a.node_is_curved(0, 0));
    // two vertices can be joined by a new edge
    let before = a.shapes[0].path.len();
    assert!(a.node_connect_to((0, 0), (0, 2)));
    assert_eq!(a.shapes[0].path.len(), before + 1);
    assert!(!a.node_connect_to((0, 1), (0, 1)), "a vertex is not joined to itself");
    // the radius is saved
    a.node_set_radius(0, 1, 12.0);
    let doc = a.doc();
    let back: Doc = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(back.pages[0].shapes[0].path[0].pts[1].radius, 12.0);
}

#[test]
fn simplify_vector_thins_a_hand_drawn_path_and_keeps_its_ends() {
    let mut a = App::default();
    let id = a.new_id();
    let mut s = Shape::new(Kind::Path, id, 1, Pos2::ZERO, None);
    // 60 points along a gentle arc with a little jitter, as a mouse draws it
    let pts: Vec<PathPt> = (0..60).map(|k| {
        let x = k as f32 * 5.0;
        PathPt::corner(Pos2::new(x, 40.0 * (x / 300.0 * std::f32::consts::PI).sin() + if k % 2 == 0 { 0.3 } else { -0.3 }))
    }).collect();
    s.set_path_world(&[Contour { pts, closed: false, hole: false }]);
    a.shapes.push(s);
    a.sel = vec![id];
    let (first, last) = { let c = &a.shapes[0].path_world()[0]; (c.pts[0].p, c.pts[c.pts.len() - 1].p) };
    let removed = a.simplify_vector(2.0);
    let c = &a.shapes[0].path_world()[0];
    assert!(removed > 40 && c.pts.len() < 20, "{} points left", c.pts.len());
    assert!((c.pts[0].p - first).length() < 0.5 && (c.pts[c.pts.len() - 1].p - last).length() < 0.5, "the ends stay");
    assert!(c.pts.iter().any(|p| p.hout != Vec2::ZERO), "the result is curved");
    // the outline is still close to the drawing: every flattened point is within a few px of the original arc
    for p in crate::geom::flatten(c, 4.0) {
        let want = 40.0 * (p.x / 300.0 * std::f32::consts::PI).sin();
        assert!((p.y - want).abs() < 4.0, "{p:?} vs {want}");
    }
    // nothing selected, nothing to do; a non-path is left alone
    a.sel.clear();
    assert_eq!(a.simplify_vector(2.0), 0);
}

#[test]
fn pen_reports_are_kept_for_a_moment() {
    // a stylus leaning to the right (tilt x 40, y 0) at 60% pressure
    crate::pen::report(40.0, 0.0, 0.6);
    let p = crate::pen::latest(1000).expect("a fresh report");
    assert!((p.azimuth() - 0.0).abs() < 1e-3 && (p.lean() - 40.0).abs() < 1e-3 && (p.pressure - 0.6).abs() < 1e-6);
    crate::pen::report(0.0, 30.0, 0.2);
    assert!((crate::pen::latest(1000).unwrap().azimuth() - 90.0).abs() < 1e-3, "leaning toward the user");
    std::thread::sleep(std::time::Duration::from_millis(30));
    assert!(crate::pen::latest(5).is_none(), "an old report is no longer the pen's state");
}

#[test]
fn stretch_brush_lays_the_picture_along_the_stroke() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    // an 8 x 4 picture: red left half, blue right half
    let png = {
        let mut buf = std::io::Cursor::new(vec![]);
        image::RgbaImage::from_fn(8, 4, |x, _| if x < 4 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) }).write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    };
    let img = a.register_image("ribbon", png).unwrap();
    // a stroke along a quarter turn: from (10, 20) to (90, 60), bent at (90, 20)
    let id = a.new_id();
    let mut s = Shape::new(Kind::Path, id, 1, Pos2::ZERO, None);
    let corner = |x: f32, y: f32| PathPt::corner(Pos2::new(x, y));
    s.set_path_world(&[Contour { pts: vec![corner(10.0, 20.0), corner(90.0, 20.0), corner(90.0, 70.0)], closed: false, hole: false }]);
    s.fills.clear();
    s.strokes.clear();
    s.stroke_w = 0.0;
    s.stretch = Some(StretchBrush { image: img, width: 16.0 });
    a.shapes.push(s);
    assert!(a.shapes[0].needs_sprite());
    let mut svg = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| svg = a.svg_for(ui.ctx(), &[id]));
    let (svg, area) = svg.unwrap();
    let (w, h) = (area.width().ceil() as u32 * 2, area.height().ceil() as u32 * 2);
    let img = crate::export::rasterize_scene(&svg, w, h).unwrap();
    let at = |x: f32, y: f32| img.pixels[(((y - area.min.y) * 2.0) as usize) * w as usize + (((x - area.min.x) * 2.0) as usize)];
    assert!(at(25.0, 20.0).r() > 150 && at(25.0, 20.0).b() < 100, "near the start the picture is red: {:?}", at(25.0, 20.0));
    assert!(at(85.0, 20.0).b() > 150 && at(85.0, 20.0).r() < 100, "later it is blue: {:?}", at(85.0, 20.0));
    assert!(at(90.0, 55.0).b() > 150, "after the bend the picture follows the stroke downwards: {:?}", at(90.0, 55.0));
    assert!(at(50.0, 45.0).a() < 10, "away from the stroke there is nothing: {:?}", at(50.0, 45.0));
    // the stroke saves and loads with its picture
    let doc = a.doc();
    let back: Doc = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(back.pages[0].shapes[0].stretch.as_ref().map(|b| b.width), Some(16.0));
}

#[test]
fn expressions_and_conditional_interactions() {
    use crate::expr::{eval, Val};
    let vars = |n: &str| match n {
        "count" => Some(Val::Num(3.0)),
        "done" => Some(Val::Bool(false)),
        "name" => Some(Val::Str("Ann".into())),
        _ => None,
    };
    let e = |s: &str| eval(s, &vars);
    assert_eq!(e("1 + 2 * 3"), Ok(Val::Num(7.0)));
    assert_eq!(e("(1 + 2) * 3 - 4 / 2 % 5"), Ok(Val::Num(7.0)));
    assert_eq!(e("count >= 3 && !done"), Ok(Val::Bool(true)));
    assert_eq!(e("count < 3 || done"), Ok(Val::Bool(false)));
    assert_eq!(e("name == 'Ann'"), Ok(Val::Bool(true)));
    assert_eq!(e("\"Hi \" + name"), Ok(Val::Str("Hi Ann".into())));
    assert_eq!(e("-count + 10"), Ok(Val::Num(7.0)));
    assert!(e("1 / 0").is_err() && e("1 +").is_err() && e("missing > 1").is_err() && e("(1 + 2").is_err() && e("2 3").is_err());
    assert_eq!(eval("`Game / count` + 1", &|n| if n == "Game / count" { Some(Val::Num(4.0)) } else { None }), Ok(Val::Num(5.0)), "a name with spaces goes in backticks");
    assert_eq!(Val::Num(5.0).text(), "5");
    assert_eq!(Val::Num(2.5).text(), "2.5");

    // a counter: each click adds one; the second button only goes to the next screen once the count is 2 or more
    let mut a = App::default();
    let home = frame(&mut a, 0.0, 0.0, 200.0, 200.0);
    let next = frame(&mut a, 400.0, 0.0, 200.0, 200.0);
    let plus = rect_at(&mut a, 10.0, 10.0);
    let go = rect_at(&mut a, 100.0, 10.0);
    let (pi, gi) = (a.index_of(plus).unwrap(), a.index_of(go).unwrap());
    for (i, w) in [(pi, 40.0), (gi, 60.0)] {
        a.shapes[i].parent = Some(home);
        a.shapes[i].size = Vec2::new(w, 30.0);
    }
    a.variables = vec![Collection { id: 70, name: "Game".into(), modes: vec![VarMode { id: 71, name: "Mode 1".into() }], vars: vec![Variable { id: 72, name: "count".into(), kind: VarKind::Number, values: vec![VarValue::Number(0.0)], ..Variable::default() }], ..Collection::default() }];
    a.shapes[pi].link = Some(next);
    a.shapes[pi].action = Action::SetVariable;
    a.shapes[pi].var = Some(72);
    a.shapes[pi].value = "count + 1".into();
    a.shapes[gi].link = Some(next);
    a.shapes[gi].condition = "count >= 2".into();
    a.start_present(home);
    let (cp, cg) = (a.world_rect(pi).center(), a.world_rect(gi).center());
    a.present_click(cg, 1.0);
    assert_eq!(a.present, Some(home), "the condition is false at the start");
    a.present_click(cp, 1.1);
    a.present_click(cg, 1.2);
    assert_eq!(a.present, Some(home), "one is not enough");
    assert_eq!(a.variables[0].vars[0].values[0], VarValue::Number(1.0));
    a.present_click(cp, 1.3);
    assert_eq!(a.variables[0].vars[0].values[0], VarValue::Number(2.0), "an expression writes into the variable");
    a.present_click(cg, 1.4);
    assert_eq!(a.present, Some(next), "now the condition holds");
    a.exit_present();
    assert_eq!(a.variables[0].vars[0].values[0], VarValue::Number(0.0), "the variables go back when presenting ends");
}

#[test]
fn suggest_auto_layout_reads_stacks_and_grids() {
    use crate::suggest::suggest;
    let r = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
    // three boxes one above the other, 12 apart, sharing a left edge
    let col = [r(10.0, 10.0, 80.0, 30.0), r(10.0, 52.0, 80.0, 30.0), r(10.0, 94.0, 80.0, 30.0)];
    let s = suggest(&col);
    assert!(s[0].vertical && s[0].columns == 0 && s[0].score > 0.9, "{s:?}");
    assert!(s[0].label.contains("gap 12"), "{}", s[0].label);
    // a row of boxes, evenly spaced
    let row = [r(0.0, 10.0, 40.0, 40.0), r(60.0, 10.0, 40.0, 40.0), r(120.0, 10.0, 40.0, 40.0)];
    assert!(!suggest(&row)[0].vertical);
    // a 2 x 2 grid is a grid
    let grid = [r(0.0, 0.0, 40.0, 40.0), r(60.0, 0.0, 40.0, 40.0), r(0.0, 60.0, 40.0, 40.0), r(60.0, 60.0, 40.0, 40.0)];
    let g = suggest(&grid);
    assert!(g.iter().any(|s| s.columns == 2), "{g:?}");
    // boxes lying on top of each other fit nothing; one box has nothing to arrange
    assert!(suggest(&[r(0.0, 0.0, 50.0, 50.0), r(10.0, 10.0, 50.0, 50.0)]).is_empty());
    assert!(suggest(&[r(0.0, 0.0, 50.0, 50.0)]).is_empty());

    // applying a suggestion: the frame gets that layout, read from where the children are
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 120.0, 140.0);
    let fi = a.index_of(f).unwrap();
    let mut ids = vec![];
    for b in col {
        let id = rect_at(&mut a, b.min.x, b.min.y);
        let i = a.index_of(id).unwrap();
        a.shapes[i].parent = Some(f);
        a.shapes[i].size = b.size();
        ids.push(id);
    }
    a.sel = vec![f];
    a.open_suggestions();
    let (target, list) = a.suggestions.clone().expect("suggestions for a frame of stacked boxes");
    assert_eq!(target, f);
    a.apply_suggestion(target, &list[0]);
    let lay = a.shapes[fi].layout.clone().expect("auto layout is on");
    assert!(lay.vertical && lay.gap == 12.0 && lay.pad[0] == 10.0 && lay.pad[1] == 10.0, "{lay:?}");
    // a grid suggestion makes a grid
    let g2 = frame_with_grid(&mut a);
    a.sel = vec![g2];
    a.open_suggestions();
    let (gf, gl) = a.suggestions.clone().unwrap();
    let grid_sug = gl.iter().find(|s| s.columns == 2).expect("a grid suggestion").clone();
    a.apply_suggestion(gf, &grid_sug);
    assert_eq!(a.shapes[a.index_of(gf).unwrap()].layout.as_ref().unwrap().columns, 2);
    // a frame that has auto layout already gets nothing
    a.sel = vec![f];
    a.suggestions = None;
    a.open_suggestions();
    assert!(a.suggestions.is_none());

    fn frame_with_grid(a: &mut App) -> u64 {
        let f = frame(a, 400.0, 0.0, 120.0, 120.0);
        for (x, y) in [(10.0, 10.0), (70.0, 10.0), (10.0, 70.0), (70.0, 70.0)] {
            let id = rect_at(a, 400.0 + x, y);
            let i = a.index_of(id).unwrap();
            a.shapes[i].parent = Some(f);
            a.shapes[i].pos = Pos2::new(x, y);
            a.shapes[i].size = Vec2::splat(40.0);
        }
        f
    }
}

#[test]
fn undo_puts_back_the_selection_guides_styles_variables_and_comments() {
    let mut a = App::default();
    let r1 = rect_at(&mut a, 0.0, 0.0);
    let r2 = rect_at(&mut a, 50.0, 0.0);
    a.sel = vec![r1, r2];
    let before = a.snap();
    // an edit that touches more than the layers
    a.sel = vec![r2];
    a.guides.push(Guide { vertical: true, pos: 10.0 });
    a.styles.push(ColorStyle { id: 500, name: "Brand".into(), color: egui::Color32::RED, description: String::new() });
    a.ext.effects.push(EffectStyle { id: 501, name: "Soft".into(), effects: vec![Effect::new(EffectKind::DropShadow)] });
    a.variables.push(Collection { id: 502, name: "Theme".into(), modes: vec![VarMode { id: 503, name: "Mode 1".into() }], ..Collection::default() });
    a.comments.push(Comment { id: 504, page: 0, pos: Pos2::ZERO, text: "hi".into(), resolved: false, author: "You".into(), secs: 1, replies: vec![], attachments: vec![], edited: false, reactions: vec![] });
    a.shapes[0].name = "renamed".into();
    assert!(a.snap_rest_for_test() != before_rest_of(&before), "the non-layer parts differ now");
    a.undo.push(before);
    // undo takes all of it back, the selection included, and redo brings it forward again
    a.do_undo(true);
    assert_eq!(a.sel, vec![r1, r2], "the selection is as it was");
    assert!(a.guides.is_empty() && a.styles.is_empty() && a.ext.effects.is_empty() && a.variables.is_empty() && a.comments.is_empty());
    assert_ne!(a.shapes[0].name, "renamed");
    a.do_undo(false);
    assert_eq!(a.sel, vec![r2]);
    assert_eq!((a.guides.len(), a.styles.len(), a.ext.effects.len(), a.variables.len(), a.comments.len()), (1, 1, 1, 1, 1));
    assert_eq!(a.shapes[0].name, "renamed");
    // a layer that is gone by the time of the undo drops out of the restored selection
    let before = a.snap();
    a.remove_subtree(r1);
    a.undo.push(before);
    a.sel = vec![r2];
    a.do_undo(true);
    assert!(a.index_of(r1).is_some());

    fn before_rest_of(s: &crate::Snap) -> crate::Snap {
        s.rest()
    }
}


#[test]
fn branches_split_and_merge_back() {
    let mut a = App::default();
    let r1 = rect_at(&mut a, 0.0, 0.0);
    let r2 = rect_at(&mut a, 100.0, 0.0);
    a.create_branch("Try");
    assert_eq!(a.branches.len(), 2);
    assert_eq!(a.cur_branch, 1);
    // on the branch: move r1, add a layer
    let i = a.index_of(r1).unwrap();
    a.shapes[i].pos.x = 30.0;
    let added = rect_at(&mut a, 200.0, 0.0);
    // back on main: fade r2, move r1 differently on y
    a.switch_branch(0);
    assert!(a.index_of(added).is_none(), "main does not have the branch's layer");
    assert_eq!(a.shapes[a.index_of(r1).unwrap()].pos.x, 0.0);
    let j = a.index_of(r2).unwrap();
    a.shapes[j].opacity = 0.5;
    // the branch is merged into main
    let notes = a.merge_branch(1);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(a.shapes[a.index_of(r1).unwrap()].pos.x, 30.0, "the branch's move arrives");
    assert!(a.index_of(added).is_some(), "the branch's new layer arrives");
    assert_eq!(a.shapes[a.index_of(r2).unwrap()].opacity, 0.5, "main's own change stays");
    // the file keeps the branches
    let d = a.doc();
    assert_eq!(d.branches.len(), 2);
    a.delete_branch(1);
    assert_eq!(a.branches.len(), 1);
}


// one HTTP request to the REST API from a client thread: (status, body bytes)
fn http(port: u16, token: Option<&str>, host: &str, method: &str, path: &str, body: &str) -> (u16, Vec<u8>) {
    use std::io::{Read, Write};
    let mut c = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n{auth}Content-Length: {}\r\n\r\n{body}", body.len());
    c.write_all(req.as_bytes()).unwrap();
    let mut out = Vec::new();
    c.read_to_end(&mut out).unwrap();
    let split = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let code = String::from_utf8_lossy(&out[..split]).split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, out[split + 4..].to_vec())
}

#[test]
fn rest_api_edits_comments_export_and_plugins() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    a.ctx = Some(ctx.clone());
    a.toggle_api(&ctx, 0);
    let (port, token) = (a.api.as_ref().unwrap().port, a.api.as_ref().unwrap().token.clone());
    let host = format!("127.0.0.1:{port}");

    // the client runs on its own thread; the app answers between its frames, as `run_frame` does
    let (h2, t2) = (host.clone(), token.clone());
    let client = std::thread::spawn(move || {
        let t = Some(t2.as_str());
        let json = |b: &[u8]| serde_json::from_slice::<serde_json::Value>(b).unwrap();
        let mut seen = vec![];
        seen.push(http(port, None, &h2, "GET", "/layers", "").0); // no token
        seen.push(http(port, Some("wrong"), &h2, "GET", "/layers", "").0); // wrong token
        seen.push(http(port, t, "evil.example", "GET", "/layers", "").0); // DNS-rebinding style Host
        let (c, b) = http(port, t, &h2, "POST", "/layers", r#"{"name":"Box","kind":"Rect","size":[40,20]}"#);
        assert_eq!(c, 200);
        let id = json(&b)["id"].as_u64().unwrap();
        let (_, b) = http(port, t, &h2, "GET", &format!("/layers/{id}"), "");
        assert_eq!(json(&b)["name"], "Box");
        assert_eq!(http(port, t, &h2, "PATCH", &format!("/layers/{id}"), r#"{"name":"Renamed","opacity":0.5}"#).0, 200);
        let (_, b) = http(port, t, &h2, "GET", "/layers", "");
        assert_eq!(json(&b).as_array().unwrap().len(), 1);
        assert_eq!(json(&b)[0]["name"], "Renamed");
        // a frame with a child made in one patch, the child pointing at the frame by ref
        let patch = r#"{"add":[{"ref":"f","kind":"Frame","name":"F","size":[100,100]},{"parent_ref":"f","kind":"Rect","name":"In","size":[10,10]}]}"#;
        assert_eq!(http(port, t, &h2, "POST", "/patch", patch).0, 200);
        assert_eq!(http(port, t, &h2, "POST", "/patch", r#"{"add":[{"parent_ref":"nope"}]}"#).0, 400);
        assert_eq!(http(port, t, &h2, "PUT", "/selection", &format!("{{\"ids\":[{id},99999]}}")).0, 200);
        let (_, b) = http(port, t, &h2, "GET", "/selection", "");
        assert_eq!(json(&b)["ids"], serde_json::json!([id]));
        let (c, png) = http(port, t, &h2, "GET", &format!("/export/{id}?format=png&scale=2"), "");
        assert_eq!((c, &png[1..4]), (200, &b"PNG"[..]));
        let (c, b) = http(port, t, &h2, "POST", "/comments", r#"{"text":"hello","x":5,"y":6}"#);
        assert_eq!(c, 200);
        let cid = json(&b)["id"].as_u64().unwrap();
        assert_eq!(http(port, t, &h2, "PATCH", &format!("/comments/{cid}"), r#"{"resolved":true,"reply":"ok"}"#).0, 200);
        let (_, b) = http(port, t, &h2, "GET", "/comments", "");
        assert_eq!((json(&b)[0]["resolved"].clone(), json(&b)[0]["replies"].as_array().unwrap().len()), (serde_json::json!(true), 1));
        assert_eq!(http(port, t, &h2, "DELETE", &format!("/layers/{id}"), "").0, 200);
        assert_eq!(http(port, t, &h2, "DELETE", &format!("/layers/{id}"), "").0, 404);
        assert_eq!(http(port, t, &h2, "GET", "/nothing", "").0, 404);
        seen
    });
    while !client.is_finished() {
        a.poll_api(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(client.join().unwrap(), vec![401, 401, 403]);
    // what the client built is in the document: the frame and its child, the box is gone
    assert!(a.shapes.iter().any(|s| s.name == "In" && s.parent.is_some()));
    assert!(!a.shapes.iter().any(|s| s.name == "Renamed"));

}

#[test]
fn rest_api_edits_a_claude_edit_copy_and_leaves_notes() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    a.ctx = Some(ctx.clone());
    // an existing design the agent did not make: a frame with a rect and a label
    let fid = a.new_id();
    let mut f = Shape::new(Kind::Frame, fid, 1, egui::pos2(100.0, 50.0), None);
    f.name = "Login".into();
    f.size = egui::vec2(300.0, 200.0);
    let rid = a.new_id();
    let mut r = Shape::new(Kind::Rect, rid, 2, egui::pos2(10.0, 10.0), Some(fid));
    r.name = "Button".into();
    r.size = egui::vec2(100.0, 40.0);
    a.shapes.push(f);
    a.shapes.push(r);
    a.toggle_api(&ctx, 0);
    let (port, token) = (a.api.as_ref().unwrap().port, a.api.as_ref().unwrap().token.clone());
    let host = format!("127.0.0.1:{port}");

    let client = std::thread::spawn(move || {
        let t = Some(token.as_str());
        let json = |b: &[u8]| serde_json::from_slice::<serde_json::Value>(b).unwrap();
        // the original is protected, in all three ways to write
        assert_eq!(http(port, t, &host, "PATCH", &format!("/layers/{rid}"), r##"{"fill":"#ff0000"}"##).0, 403);
        assert_eq!(http(port, t, &host, "POST", "/patch", &format!(r#"{{"set":[{{"id":{rid},"w":5}}]}}"#)).0, 403);
        assert_eq!(http(port, t, &host, "DELETE", &format!("/layers/{rid}"), "").0, 403);
        let (c, b) = http(port, t, &host, "POST", "/claude_edit", &format!(r#"{{"id":{fid}}}"#));
        assert_eq!(c, 200);
        let v = json(&b);
        let (cid, crid) = (v["id"].as_u64().unwrap(), v["ids"][rid.to_string()].as_u64().unwrap());
        assert_eq!((v["name"].as_str().unwrap(), v["existing"].as_bool().unwrap()), ("Login claude_edit", false));
        assert_eq!(v["outline"]["children"][0]["name"], "Button");
        // asking again returns the same copy
        let (_, b) = http(port, t, &host, "POST", "/claude_edit", &format!(r#"{{"id":{fid}}}"#));
        assert_eq!((json(&b)["id"].as_u64().unwrap(), json(&b)["existing"].as_bool().unwrap()), (cid, true));
        // the copy can be edited, with the shorthands
        assert_eq!(http(port, t, &host, "PATCH", &format!("/layers/{crid}"), r##"{"fill":"#ff0000","w":120,"radius":6}"##).0, 200);
        // a note beside the changed layer, and a second one that must not sit on the first
        let (c, b) = http(port, t, &host, "POST", "/notes", &format!(r#"{{"text":"Wider so the label fits.","near":{crid},"side":"right"}}"#));
        assert_eq!(c, 200);
        let n1 = json(&b);
        let (_, b) = http(port, t, &host, "POST", "/notes", &format!(r#"{{"text":"Red follows the brand.","near":{crid},"side":"right"}}"#));
        let n2 = json(&b);
        assert!(n2["y"].as_f64().unwrap() > n1["y"].as_f64().unwrap() + 20.0, "notes stack: {n1} {n2}");
        assert_eq!(http(port, t, &host, "POST", "/notes", r#"{"text":""}"#).0, 400);
        // a note can be removed again, and the outline is compact
        assert_eq!(http(port, t, &host, "DELETE", &format!("/layers/{}", n2["id"]), "").0, 200);
        let (_, b) = http(port, t, &host, "GET", "/outline?depth=0", "");
        let o = json(&b);
        let editable: Vec<(String, bool)> = o.as_array().unwrap().iter().map(|n| (n["name"].as_str().unwrap().to_string(), n["editable"].as_bool().unwrap())).collect();
        assert!(editable.contains(&("Login".into(), false)) && editable.contains(&("Login claude_edit".into(), true)), "{editable:?}");
        assert_eq!(http(port, t, &host, "GET", "/guide", "").0, 200);
        (cid, crid)
    });
    while !client.is_finished() {
        a.poll_api(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let (cid, crid) = client.join().unwrap();
    // the original is untouched, the copy carries the edit and sits beside it
    let orig = &a.shapes[a.index_of(rid).unwrap()];
    assert_eq!((orig.size.x, orig.fills.len()), (100.0, 1));
    let copy = &a.shapes[a.index_of(crid).unwrap()];
    assert_eq!((copy.size.x, copy.radius[0], copy.parent), (120.0, 6.0, Some(cid)));
    assert!(matches!(copy.fills[0].kind, PaintKind::Solid(c) if c == egui::Color32::RED));
    assert!(a.shapes[a.index_of(cid).unwrap()].pos.x >= 100.0 + 300.0, "placed right of the original");
    assert_eq!(a.shapes.iter().filter(|s| s.name.starts_with("claude_note")).count(), 1);
}

#[test]
fn the_api_rate_limit_lets_a_burst_through_and_then_refuses() {
    let mut b = crate::api::Bucket { tokens: 60.0, at: std::time::Instant::now() };
    let allowed = (0..200).filter(|_| b.take()).count();
    assert!((60..=62).contains(&allowed), "a burst of 60, then the refill rate: {allowed}");
    b.at -= std::time::Duration::from_secs(1);
    assert!(b.take(), "a second later requests are served again");
}

#[test]
fn rest_api_runs_actions_pages_undo_files() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    a.ctx = Some(ctx.clone());
    // a layer of the person's own, and an unsaved edit
    let own = a.new_id();
    let mut f = Shape::new(Kind::Frame, own, 1, egui::pos2(0.0, 0.0), None);
    f.name = "Mine".into();
    a.shapes.push(f);
    a.undo.push(a.snap());
    a.toggle_api(&ctx, 0);
    let (port, token) = (a.api.as_ref().unwrap().port, a.api.as_ref().unwrap().token.clone());
    let host = format!("127.0.0.1:{port}");

    let client = std::thread::spawn(move || {
        let t = Some(token.as_str());
        let json = |b: &[u8]| serde_json::from_slice::<serde_json::Value>(b).unwrap();
        let (c, b) = http(port, t, &host, "GET", "/actions", "");
        let names: Vec<String> = json(&b).as_array().unwrap().iter().map(|n| n["name"].as_str().unwrap().to_string()).collect();
        assert!(c == 200 && names.contains(&"Group selection".to_string()) && !names.contains(&"Present".to_string()) && !names.iter().any(|n| n.starts_with("Import")), "{names:?}");
        // two layers of the agent's own: align them, group them
        let mut ids = vec![];
        for x in [0, 50] {
            let (_, b) = http(port, t, &host, "POST", "/layers", &format!(r#"{{"kind":"Rect","x":{x},"y":0,"w":10,"h":10}}"#));
            ids.push(json(&b)["id"].as_u64().unwrap());
        }
        let both = format!("[{},{}]", ids[0], ids[1]);
        assert_eq!(http(port, t, &host, "POST", "/align", &format!(r#"{{"op":"right","ids":{both}}}"#)).0, 200);
        assert_eq!(http(port, t, &host, "POST", "/align", r#"{"op":"sideways"}"#).0, 400);
        assert_eq!(http(port, t, &host, "POST", "/actions", &format!(r#"{{"name":"Group selection","ids":{both}}}"#)).0, 200);
        assert_eq!(http(port, t, &host, "POST", "/actions", r#"{"name":"Group selection","ids":[]}"#).0, 400, "needs a selection");
        assert_eq!(http(port, t, &host, "POST", "/actions", r#"{"name":"Present"}"#).0, 400, "not for scripts");
        // the person's own layer is protected from actions and arrangement too
        assert_eq!(http(port, t, &host, "POST", "/actions", &format!(r#"{{"name":"Flip horizontal","ids":[{own}]}}"#)).0, 403);
        assert_eq!(http(port, t, &host, "POST", "/align", &format!(r#"{{"op":"left","ids":[{own}]}}"#)).0, 403);
        // pages: add and name one, go back, delete the empty one; a page with layers stays
        assert_eq!(http(port, t, &host, "POST", "/pages", r#"{"name":"Second"}"#).0, 200);
        assert_eq!(http(port, t, &host, "PUT", "/pages/0", "").0, 200);
        assert_eq!(http(port, t, &host, "DELETE", "/pages/0", "").0, 403);
        assert_eq!(http(port, t, &host, "DELETE", "/pages/1", "").0, 200);
        assert_eq!(http(port, t, &host, "DELETE", "/pages/7", "").0, 404);
        // files: no overwriting of unsaved work, no saving over a non-drui file
        assert_eq!(http(port, t, &host, "POST", "/open", r#"{"path":"x.json"}"#).0, 409);
        assert_eq!(http(port, t, &host, "POST", "/import", r#"{"path":"x.png","discard":true}"#).0, 400);
        assert_eq!(http(port, t, &host, "POST", "/open", r#"{"path":"does-not-exist.json","discard":true}"#).0, 400, "a failed open is not a 200");
        let (c, _) = http(port, t, &host, "POST", "/actions", r#"{"name":"Push overrides to main component","ids":[]}"#);
        assert_eq!(c, 400, "not for scripts");
        assert_eq!(http(port, t, &host, "POST", "/save", r#"{"path":"x.exe"}"#).0, 400);
        assert_eq!(http(port, t, &host, "POST", "/undo", "").0, 200);
        ids
    });
    while !client.is_finished() {
        a.poll_api(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let ids = client.join().unwrap();
    let (x0, x1) = (a.world_rect(a.index_of(ids[0]).unwrap()).max.x, a.world_rect(a.index_of(ids[1]).unwrap()).max.x);
    assert!(a.shapes.iter().any(|s| s.kind == Kind::Group), "grouped");
    assert!((x0 - x1).abs() < 0.01, "aligned right: {x0} {x1}");
    assert_eq!(a.pages.len(), 2, "the undo brought the deleted page back");
}

#[test]
fn rest_api_styles_versions_branches_and_libraries() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    a.ctx = Some(ctx.clone());
    a.toggle_api(&ctx, 0);
    let (port, token) = (a.api.as_ref().unwrap().port, a.api.as_ref().unwrap().token.clone());
    let host = format!("127.0.0.1:{port}");
    let client = std::thread::spawn(move || {
        let t = Some(token.as_str());
        let json = |b: &[u8]| serde_json::from_slice::<serde_json::Value>(b).unwrap();
        // styles can be added, listed, and nothing else
        let (c, b) = http(port, t, &host, "POST", "/styles/color", r##"{"name":"Brand","color":[10,132,255,255]}"##);
        assert_eq!(c, 200, "{}", String::from_utf8_lossy(&b));
        assert_eq!(http(port, t, &host, "POST", "/styles/color", r#"{"name":""}"#).0, 400);
        assert_eq!(http(port, t, &host, "POST", "/styles/nope", r#"{"name":"x"}"#).0, 400);
        let (_, b) = http(port, t, &host, "GET", "/styles", "");
        assert_eq!(json(&b)["color"][0]["name"], "Brand");
        // versions: save, list, restore only when confirmed
        http(port, t, &host, "POST", "/layers", r#"{"kind":"Rect","name":"A"}"#);
        assert_eq!(http(port, t, &host, "POST", "/versions", r#"{"name":"v1"}"#).0, 200);
        http(port, t, &host, "POST", "/layers", r#"{"kind":"Rect","name":"B"}"#);
        assert_eq!(http(port, t, &host, "POST", "/versions/0/restore", "{}").0, 403);
        assert_eq!(http(port, t, &host, "POST", "/versions/0/restore", r#"{"confirm":true}"#).0, 200);
        assert_eq!(http(port, t, &host, "POST", "/versions/5/restore", r#"{"confirm":true}"#).0, 404);
        let (_, b) = http(port, t, &host, "GET", "/versions", "");
        assert_eq!(json(&b)[0]["name"], "v1");
        // branches: make one, it cannot merge without an approved review, and an unchanged one can go
        assert_eq!(http(port, t, &host, "POST", "/branches", r#"{"name":"try"}"#).0, 200);
        assert_eq!(http(port, t, &host, "POST", "/branches/1/merge", "").0, 403);
        assert_eq!(http(port, t, &host, "POST", "/branches/1/review", r#"{"reviewer":"sam"}"#).0, 200);
        assert_eq!(http(port, t, &host, "POST", "/branches/1/merge", "").0, 403, "asked is not approved");
        assert_eq!(http(port, t, &host, "DELETE", "/branches/1", "").0, 403, "it is open");
        assert_eq!(http(port, t, &host, "PUT", "/branches/0", "").0, 200);
        assert_eq!(http(port, t, &host, "DELETE", "/branches/1", "").0, 200);
        assert_eq!(http(port, t, &host, "DELETE", "/branches/0", "").0, 403);
        // a picture, a component and its instance, a copy, a duplicated page
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        let (c, b) = http(port, t, &host, "POST", "/images", &format!(r#"{{"data":"{png}","name":"dot.png","x":5,"y":5}}"#));
        assert_eq!(c, 200, "{}", String::from_utf8_lossy(&b));
        assert!(json(&b)["layer"].is_u64());
        assert_eq!(http(port, t, &host, "POST", "/images", r#"{"data":"AAAA"}"#).0, 400, "not a picture");
        let (_, b) = http(port, t, &host, "POST", "/layers", r#"{"kind":"Frame","name":"Btn","component":true,"w":50,"h":20}"#);
        let main = json(&b)["id"].as_u64().unwrap();
        let (c, b) = http(port, t, &host, "POST", "/instances", &format!(r#"{{"main":{main},"x":100,"y":0}}"#));
        assert_eq!(c, 200, "{}", String::from_utf8_lossy(&b));
        assert_eq!(http(port, t, &host, "POST", "/instances", r#"{"main":99999}"#).0, 400);
        let (_, b) = http(port, t, &host, "POST", "/copy", &format!(r#"{{"ids":[{main}]}}"#));
        assert_eq!(json(&b)["layers"], 1);
        let (_, b) = http(port, t, &host, "POST", "/paste", "");
        assert_eq!(json(&b)["ids"].as_array().unwrap().len(), 1);
        assert_eq!(http(port, t, &host, "POST", "/pages/0/duplicate", "").0, 200);
        assert_eq!(http(port, t, &host, "POST", "/pages/9/duplicate", "").0, 404);
        // libraries
        assert_eq!(http(port, t, &host, "GET", "/libraries", "").0, 200);
        assert_eq!(http(port, t, &host, "POST", "/libraries", r#"{"path":"x.exe"}"#).0, 400);
        assert_eq!(http(port, t, &host, "POST", "/libraries/9/update", "").0, 404);
    });
    while !client.is_finished() {
        a.poll_api(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    client.join().unwrap();
    assert_eq!(a.shapes.iter().filter(|s| s.name == "A").count(), 1);
    assert!(!a.shapes.iter().any(|s| s.name == "B"), "restored to v1");
}

#[test]
fn rest_api_view_motion_tools_and_style_edits() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    a.ctx = Some(ctx.clone());
    let own = a.new_id();
    let mut r = Shape::new(Kind::Rect, own, 1, egui::pos2(0.0, 0.0), None);
    r.size = egui::vec2(50.0, 50.0);
    a.shapes.push(r);
    a.toggle_api(&ctx, 0);
    let (port, token) = (a.api.as_ref().unwrap().port, a.api.as_ref().unwrap().token.clone());
    let host = format!("127.0.0.1:{port}");
    let client = std::thread::spawn(move || {
        let t = Some(token.as_str());
        let json = |b: &[u8]| serde_json::from_slice::<serde_json::Value>(b).unwrap();
        let rect = |x: i32| {
            let (_, b) = http(port, t, &host, "POST", "/layers", &format!(r#"{{"kind":"Rect","x":{x},"y":0,"w":100,"h":100}}"#));
            json(&b)["id"].as_u64().unwrap()
        };
        // view and preferences
        assert_eq!(http(port, t, &host, "PUT", "/view", r#"{"rulers":false,"left_tab":"history","zoom":2,"guides":[{"vertical":true,"pos":100}],"nudge_big":20}"#).0, 200);
        let (_, b) = http(port, t, &host, "GET", "/view", "");
        let v = json(&b);
        assert_eq!((v["rulers"].clone(), v["left_tab"].clone(), v["nudge_big"].clone(), v["guides"][0]["pos"].clone()), (false.into(), "history".into(), 20.0.into(), 100.0.into()));
        assert_eq!(http(port, t, &host, "PUT", "/view", r#"{"nonsense":1}"#).0, 400);
        assert_eq!(http(port, t, &host, "PUT", "/view", r#"{"rulers":"false"}"#).0, 400, "wrong type");
        assert_eq!(http(port, t, &host, "PUT", "/view", r#"{"mode":""}"#).0, 400);
        assert_eq!(http(port, t, &host, "PUT", "/view", r#"{"left_tab":"sideways"}"#).0, 400);
        // the Motion timeline
        let (c, b) = http(port, t, &host, "PUT", "/motion", r#"{"length":3000,"loop":true,"time":9000}"#);
        assert_eq!(c, 200);
        assert_eq!((json(&b)["length"].as_f64(), json(&b)["time"].as_f64(), json(&b)["loop"].as_bool()), (Some(3000.0), Some(3000.0), Some(true)));
        // styles: editing and deleting need the person's say-so
        let (_, b) = http(port, t, &host, "POST", "/styles/color", r##"{"name":"Brand","color":[10,132,255,255]}"##);
        let sid = json(&b)["id"].as_u64().unwrap();
        assert_eq!(http(port, t, &host, "PATCH", &format!("/styles/color/{sid}"), r#"{"name":"Renamed"}"#).0, 403);
        assert_eq!(http(port, t, &host, "PATCH", &format!("/styles/color/{sid}"), r#"{"name":"Renamed","confirm":true}"#).0, 200);
        let (_, b) = http(port, t, &host, "GET", "/styles", "");
        assert_eq!(json(&b)["color"][0]["name"], "Renamed");
        assert_eq!(http(port, t, &host, "DELETE", &format!("/styles/color/{sid}"), r#"{"confirm":true}"#).0, 200);
        assert_eq!(http(port, t, &host, "DELETE", &format!("/styles/color/{sid}"), r#"{"confirm":true}"#).0, 404);
        // branch review: handed on or withdrawn, never answered
        http(port, t, &host, "POST", "/branches", r#"{"name":"try"}"#);
        assert_eq!(http(port, t, &host, "PUT", "/branches/1/review", r#"{"state":"approved"}"#).0, 403);
        assert_eq!(http(port, t, &host, "PUT", "/branches/1/review", r#"{"state":"requested","reviewer":"sam"}"#).0, 200);
        assert_eq!(http(port, t, &host, "PUT", "/branches/1/review", r#"{"state":"withdrawn"}"#).0, 200);
        // the pointer tools by coordinates
        let (r1, r2) = (rect(200), rect(250));
        let both = format!("[{r1},{r2}]");
        assert_eq!(http(port, t, &host, "POST", "/tools/cut", &format!(r#"{{"ids":[{r1}],"from":[250,-10],"to":[250,110]}}"#)).0, 200);
        assert_eq!(http(port, t, &host, "POST", "/tools/cut", r#"{"from":[0,0],"to":[1,1]}"#).0, 400, "names its layers");
        assert_eq!(http(port, t, &host, "POST", "/tools/cut", &format!(r#"{{"ids":[{own}],"from":[25,-10],"to":[25,60]}}"#)).0, 403);
        let (e1, e2) = (rect(500), rect(550));
        assert_eq!(http(port, t, &host, "POST", "/tools/builder", &format!(r#"{{"ids":[{e1},{e2}],"merge":[[525,50]],"delete":[[510,50]]}}"#)).0, 200);
        let w = rect(800);
        assert_eq!(http(port, t, &host, "POST", "/tools/erase", &format!(r#"{{"ids":[{w}],"points":[[850,-20],[850,120]]}}"#)).0, 200);
        assert_eq!(http(port, t, &host, "POST", "/tools/lasso", &format!(r#"{{"ids":[{w}]}}"#)).0, 400);
        let _ = both;
        // Present
        let (_, b) = http(port, t, &host, "POST", "/layers", r#"{"kind":"Frame","name":"Home","w":300,"h":200}"#);
        let f = json(&b)["id"].as_u64().unwrap();
        assert_eq!(http(port, t, &host, "POST", "/present", &format!(r#"{{"id":{f}}}"#)).0, 200);
        assert_eq!(http(port, t, &host, "POST", "/present", r#"{"id":99999}"#).0, 404);
        assert_eq!(http(port, t, &host, "DELETE", "/present", "").0, 200);
    });
    while !client.is_finished() {
        a.poll_api(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    client.join().unwrap();
    assert!(!a.rulers && a.present.is_none());
    let paths = a.shapes.iter().filter(|s| s.kind == Kind::Path).count();
    assert!(paths >= 4, "the cut made two paths and the builder at least two: {paths}");
    assert!(a.shapes.iter().any(|s| s.id == own && s.size.x == 50.0), "the person's layer is untouched");
}



#[test]
fn variables_drive_effect_offsets_and_layout_guide_sizes() {
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].effects.push(Effect::new(EffectKind::DropShadow));
    a.shapes[fi].grids.push(LayoutGrid::new(GridKind::Columns));
    a.shapes[fi].grids.push(LayoutGrid::new(GridKind::Grid));
    let (dx, dy, sp, gut, size) = (a.new_id(), a.new_id(), a.new_id(), a.new_id(), a.new_id());
    let num = |id: u64, name: &str, v: f32, w: f32| Variable { id, name: name.into(), kind: VarKind::Number, values: vec![VarValue::Number(v), VarValue::Number(w)], ..Variable::default() };
    a.variables.push(Collection {
        id: 500,
        name: "Elevation".into(),
        modes: vec![VarMode { id: 501, name: "Low".into() }, VarMode { id: 502, name: "High".into() }],
        vars: vec![num(dx, "dx", 1.0, 3.0), num(dy, "dy", 2.0, 8.0), num(sp, "spread", 0.0, 4.0), num(gut, "gutter", 16.0, 32.0), num(size, "cell", 8.0, 24.0)],
        ..Collection::default()
    });
    a.shapes[fi].effects[0].var_pos = [Some(dx), Some(dy), Some(sp)];
    a.shapes[fi].grids[0].var_num = [None, Some(gut)];
    a.shapes[fi].grids[1].var_num = [Some(size), None];
    a.settle();
    let s = &a.shapes[fi];
    assert_eq!((s.effects[0].dx, s.effects[0].dy, s.effects[0].spread), (1.0, 2.0, 0.0));
    assert_eq!((s.grids[0].gutter, s.grids[1].size), (16.0, 8.0));
    // choosing the second mode on the frame restyles the shadow and the guides
    a.shapes[fi].modes.push((500, 502));
    a.settle();
    let s = &a.shapes[fi];
    assert_eq!((s.effects[0].dx, s.effects[0].dy, s.effects[0].spread), (3.0, 8.0, 4.0));
    assert_eq!((s.grids[0].gutter, s.grids[1].size), (32.0, 24.0));
}

#[test]
fn web_player_carries_change_to_as_a_picture_of_the_variant() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let v1 = frame(&mut a, 0.0, 0.0, 60.0, 30.0);
    let btn = rect_at(&mut a, 5.0, 5.0);
    let v2 = frame(&mut a, 100.0, 0.0, 60.0, 30.0);
    a.sel = vec![v1, v2];
    a.make_component();
    a.sel = vec![v1, v2];
    a.combine_as_variants();
    let bi = a.index_of(btn).unwrap();
    a.shapes[bi].trigger = Trigger::WhileHover;
    a.shapes[bi].action = Action::ChangeTo;
    a.shapes[bi].link = Some(v2);
    let screen = frame(&mut a, 0.0, 400.0, 300.0, 200.0);
    a.instantiate(v1, Some(screen), Pos2::new(20.0, 20.0)).unwrap();
    let mut html = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| html = a.prototype_html(ui.ctx(), Some(screen)));
    let html = html.unwrap();
    let d: serde_json::Value = serde_json::from_str(html.split("const D = ").nth(1).unwrap().split(";
document.getElementById('stage')").next().unwrap()).unwrap();
    let ix = d["frames"][screen.to_string()]["spots"].as_array().unwrap().iter().flat_map(|s| s["ix"].as_array().unwrap().clone()).find(|i| i["do"] == "variant").expect("a variant action");
    assert_eq!((ix["on"].as_str(), ix["hold"].as_bool(), ix["bx"].as_f64(), ix["by"].as_f64()), (Some("enter"), Some(true), Some(20.0), Some(20.0)));
    assert!(ix["svg"].as_str().unwrap().contains("<svg"));
}


// a "Button" component set (Size = Small / Large, a Label text property and a Show Icon boolean) and a "Home" screen that uses it
fn button_set_and_screens(a: &mut App) -> (usize, u64, u64) {
    let mut make = |a: &mut App, x: f32, w: f32| -> (u64, u64, u64) {
        let f = frame(a, x, 0.0, w, 40.0);
        let icon = rect_at(a, x, 0.0);
        let ii = a.index_of(icon).unwrap();
        a.shapes[ii].parent = Some(f);
        a.shapes[ii].pos = Pos2::new(4.0, 4.0);
        a.shapes[ii].name = "Icon".into();
        a.add(Kind::Text, Pos2::new(x + 20.0, 10.0));
        let ti = a.shapes.len() - 1;
        a.shapes[ti].parent = Some(f);
        a.shapes[ti].pos = Pos2::new(20.0, 10.0);
        a.shapes[ti].text = "Go".into();
        a.shapes[ti].name = "Label".into();
        let tid = a.shapes[ti].id;
        a.sel = vec![f];
        a.make_component();
        (f, icon, tid)
    };
    let (small, small_icon, small_text) = make(a, 0.0, 80.0);
    let (large, _, _) = make(a, 200.0, 160.0);
    a.sel = vec![small, large];
    a.combine_as_variants();
    let set = a.single().unwrap();
    a.shapes[set].name = "Button".into();
    a.rename_variant_property(set, "Property 1", "Size");
    let (si, li) = (a.index_of(small).unwrap(), a.index_of(large).unwrap());
    a.set_variant_value(si, "Size", "Small");
    a.set_variant_value(li, "Size", "Large");
    a.create_property(a.index_of(small_icon).unwrap(), CompPropKind::Bool).unwrap();
    a.create_property(a.index_of(small_text).unwrap(), CompPropKind::Text).unwrap();
    // two screens; the first has an instance of the large button that opens the second
    let home = frame(a, 0.0, 400.0, 300.0, 200.0);
    let detail = frame(a, 400.0, 400.0, 300.0, 200.0);
    let hi = a.index_of(home).unwrap();
    a.shapes[hi].name = "Home".into();
    let di = a.index_of(detail).unwrap();
    a.shapes[di].name = "Detail".into();
    let inst = a.instantiate(large, Some(home), Pos2::new(20.0, 20.0)).unwrap();
    let ii = a.index_of(inst).unwrap();
    a.shapes[ii].link = Some(detail);
    a.settle();
    (set, home, detail)
}



#[test]
fn fill_show_in_exports_leaves_a_frames_background_out_of_an_export() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 100.0, 100.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].fills = vec![Paint::solid(egui::Color32::from_rgb(1, 2, 3))];
    let r = rect_at(&mut a, 10.0, 10.0);
    let ri = a.index_of(r).unwrap();
    a.shapes[ri].parent = Some(f);
    a.shapes[ri].fills = vec![Paint::solid(egui::Color32::from_rgb(9, 8, 7))];
    let svg_of = |a: &App| a.svg_for(&ctx, &[f]).unwrap().0;
    let with = svg_of(&a);
    assert!(with.contains("rgb(1,2,3)") && with.contains("rgb(9,8,7)"));
    a.shapes[fi].no_bg_export = true;
    let without = svg_of(&a);
    assert!(!without.contains("rgb(1,2,3)"), "the frame's fill is left out");
    assert!(without.contains("rgb(9,8,7)"), "what is inside stays");
}

#[test]
fn the_connection_node_links_a_layer_to_the_frame_it_is_dropped_on() {
    let mut a = App::default();
    let home = frame(&mut a, 0.0, 0.0, 200.0, 300.0);
    let detail = frame(&mut a, 400.0, 0.0, 200.0, 300.0);
    let btn = rect_at(&mut a, 20.0, 40.0);
    let bi = a.index_of(btn).unwrap();
    a.shapes[bi].parent = Some(home);
    a.sel = vec![btn];
    // no node outside the Prototype tab
    assert!(a.connect_node(Pos2::ZERO).is_none());
    a.tab = crate::Tab::Prototype;
    let (id, node) = a.connect_node(Pos2::ZERO).expect("a node on the selected layer");
    assert_eq!(id, btn);
    assert!(a.connect_node_at(Pos2::ZERO, node).is_some_and(|d| matches!(d, crate::Drag::Connect(b) if b == btn)));
    assert!(a.connect_node_at(Pos2::ZERO, node + Vec2::new(40.0, 0.0)).is_none());
    // dropping on its own screen does nothing, on another frame it links
    a.finish_connect(btn, Pos2::new(100.0, 100.0));
    assert_eq!(a.shapes[bi].link, None);
    a.finish_connect(btn, Pos2::new(500.0, 100.0));
    assert_eq!((a.shapes[bi].link, a.shapes[bi].action, a.shapes[bi].trigger), (Some(detail), Action::Navigate, Trigger::Click));
    assert_eq!(a.connect_target(Pos2::new(900.0, 900.0), btn), None, "empty canvas");
}

#[test]
fn shader_effects_draw_a_pattern_over_the_layer() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let r = rect_at(&mut a, 0.0, 0.0);
    let ri = a.index_of(r).unwrap();
    a.shapes[ri].size = Vec2::new(100.0, 60.0);
    a.shapes[ri].fills = vec![Paint::solid(egui::Color32::from_rgb(10, 20, 30))];
    let svg_of = |a: &App| a.svg_for(&ctx, &[r]).unwrap().0;
    assert!(!svg_of(&a).contains("<circle"), "no shader yet");
    // dots: a pattern of circles over the fill, clipped to the layer's own path
    a.shapes[ri].shaders.push(ShaderEffect::new(ShaderKind::Dots));
    let svg = svg_of(&a);
    assert!(svg.contains("<pattern") && svg.contains("<circle"), "{svg}");
    assert!(svg.matches("fill=\"url(#").count() >= 1);
    // softness blurs it, a hidden shader draws nothing, Clip to alpha off covers the box
    a.shapes[ri].shaders[0].softness = 20.0;
    assert!(svg_of(&a).contains("feGaussianBlur"));
    a.shapes[ri].shaders[0].softness = 0.0;
    a.shapes[ri].shaders[0].clip_alpha = false;
    assert!(svg_of(&a).contains("H100.00V60.00"), "the box");
    a.shapes[ri].shaders[0].visible = false;
    assert!(!svg_of(&a).contains("<circle"));
    // every preset renders, and a shader effect survives a save
    for (k, _) in ShaderKind::ALL {
        a.shapes[ri].shaders = vec![ShaderEffect::new(k)];
        assert!(svg_of(&a).contains("<pattern"), "{k:?}");
    }
    let json = serde_json::to_string(&a.shapes[ri]).unwrap();
    let back: Shape = serde_json::from_str(&json).unwrap();
    assert_eq!(back.shaders, a.shapes[ri].shaders);
}

// runs the document sync as the frame loop does: until the instances have caught up with every change made by `edit`
fn edit_and_sync(a: &mut App, edit: impl FnOnce(&mut App)) {
    let before = a.shapes.clone();
    edit(a);
    for _ in 0..4 {
        let b = a.shapes.clone();
        a.sync_instances(&before);
        if a.shapes == b {
            break;
        }
    }
}

#[test]
fn nested_instances_follow_override_and_reset() {
    let mut a = App::default();
    let red = Paint::solid(egui::Color32::RED);
    let blue = Paint::solid(egui::Color32::BLUE);
    // A and A2: small components with one rect each
    let ma = frame(&mut a, 0.0, 0.0, 20.0, 20.0);
    let ra = rect_at(&mut a, 2.0, 2.0);
    a.sel = vec![ma];
    a.make_component();
    let ma2 = frame(&mut a, 100.0, 0.0, 20.0, 20.0);
    let ra2 = rect_at(&mut a, 102.0, 2.0);
    a.sel = vec![ma2];
    a.make_component();
    let i = a.index_of(ra2).unwrap();
    a.shapes[i].fills = vec![blue.clone()];
    // B: a component with an instance of A inside, and a second one, B2, the same
    let mb = frame(&mut a, 200.0, 0.0, 80.0, 40.0);
    a.sel = vec![mb];
    a.make_component();
    let nested_in_b = a.instantiate(ma, Some(mb), Pos2::new(10.0, 10.0)).unwrap();
    let mb2 = frame(&mut a, 400.0, 0.0, 80.0, 40.0);
    a.sel = vec![mb2];
    a.make_component();
    a.instantiate(ma, Some(mb2), Pos2::new(30.0, 5.0)).unwrap();
    let b1 = a.instantiate(mb, None, Pos2::new(0.0, 300.0)).unwrap();
    let fill_of = |a: &App, id: u64| a.shapes[a.index_of(id).unwrap()].fills.clone();
    let nested = |a: &App| a.shapes.iter().find(|s| s.parent == Some(b1) && s.instance).map(|s| s.id);

    // 1. the instance of A inside the instance of B is still an instance: it can be swapped and has properties
    let n1 = nested(&a).expect("a nested instance root keeps its instance flag");
    assert_eq!(a.shapes[a.index_of(n1).unwrap()].pos, Pos2::new(10.0, 10.0));

    // 2. a change in A flows through B's nested instance into B's instance
    edit_and_sync(&mut a, |a| {
        let k = a.index_of(ra).unwrap();
        a.shapes[k].fills = vec![red.clone()];
    });
    let leaf = |a: &App, root: u64| a.shapes.iter().find(|s| s.parent == Some(root)).map(|s| s.id).unwrap();
    assert_eq!(fill_of(&a, leaf(&a, n1)), vec![red.clone()]);

    // 3. the nested instance's place belongs to B: moving it in B moves it in every instance of B
    edit_and_sync(&mut a, |a| {
        let k = a.index_of(nested_in_b).unwrap();
        a.shapes[k].pos = Pos2::new(25.0, 12.0);
    });
    assert_eq!(a.shapes[a.index_of(n1).unwrap()].pos, Pos2::new(25.0, 12.0));

    // 4. swapping the nested instance in the instance of B shows A2's look, keeps the place, and leaves B's own nested instance alone
    a.swap_instance(n1, ma2);
    let n1 = nested(&a).unwrap();
    assert_eq!(a.shapes[a.index_of(n1).unwrap()].pos, Pos2::new(25.0, 12.0));
    assert_eq!(fill_of(&a, leaf(&a, n1)), vec![blue.clone()]);
    assert_eq!(fill_of(&a, leaf(&a, nested_in_b)), vec![red.clone()]);

    // 5. the swap survives a change in B (a new child in B) ...
    edit_and_sync(&mut a, |a| {
        let k = a.index_of(mb).unwrap();
        a.shapes[k].fills = vec![red.clone()];
    });
    let n1 = nested(&a).unwrap();
    assert_eq!(fill_of(&a, leaf(&a, n1)), vec![blue.clone()], "the nested swap is an override and stays");

    // 6. ... and an override inside the nested instance (its rect's colour) survives swapping the outer instance to another component
    let k = a.index_of(leaf(&a, n1)).unwrap();
    a.shapes[k].fills = vec![Paint::solid(egui::Color32::GREEN)];
    a.swap_instance(b1, mb2);
    let b1 = a.shapes.iter().find(|s| s.instance && s.src == Some(mb2)).unwrap().id;
    let n = a.shapes.iter().find(|s| s.parent == Some(b1) && s.instance).expect("the new instance has its nested instance");
    assert_eq!(a.shapes[a.index_of(leaf(&a, n.id)).unwrap()].fills, vec![Paint::solid(egui::Color32::GREEN)]);

    // 7. Reset all overrides puts the instance back as its main has it: the nested instance is A again, the colours the main's
    a.reset_overrides(b1);
    let b1 = a.shapes.iter().find(|s| s.instance && s.src == Some(mb2)).unwrap().id;
    let n = a.shapes.iter().find(|s| s.parent == Some(b1) && s.instance).unwrap().id;
    assert_eq!(fill_of(&a, leaf(&a, n)), vec![red.clone()]);
}

#[test]
fn duplicating_a_component_makes_an_instance_and_properties_can_be_created() {
    let mut a = App::default();
    let c = frame(&mut a, 0.0, 0.0, 120.0, 60.0);
    rect_at(&mut a, 5.0, 5.0);
    a.sel = vec![c];
    a.make_component();

    // like Figma: Ctrl+D on a main component gives an instance of it, the component stays the only main
    a.duplicate();
    let dup = a.sel[0];
    let di = a.index_of(dup).unwrap();
    assert!(a.shapes[di].instance && !a.shapes[di].component, "the copy is an instance");
    assert_eq!(a.shapes[di].src, Some(c));
    assert_eq!(a.shapes.iter().filter(|s| s.component).count(), 1);
    // a plain frame is still copied as a frame
    let f = frame(&mut a, 500.0, 0.0, 50.0, 50.0);
    a.sel = vec![f];
    a.duplicate();
    assert!(!a.shapes[a.index_of(a.sel[0]).unwrap()].instance);

    // Create property > Slot: a slot frame inside the component, which the instance gets too
    let ci = a.index_of(c).unwrap();
    a.add_slot(ci);
    let slot = a.shapes.iter().find(|s| s.parent == Some(c) && s.slot).expect("a slot in the component");
    assert_eq!(slot.name, "Slot");

    // Create property > Variant on a lone component: it becomes a set of two variants with a first property
    a.create_variant_property(a.index_of(c).unwrap());
    let set = a.shapes.iter().position(|s| s.component_set).expect("a component set");
    assert_eq!(a.variants_of(set).len(), 2);
    assert_eq!(a.variant_properties(set).len(), 1);
    // ... and on the set it adds another property
    a.create_variant_property(set);
    assert_eq!(a.variant_properties(set).len(), 2);

    // two components selected together combine as variants
    let x = frame(&mut a, 900.0, 0.0, 40.0, 40.0);
    let y = frame(&mut a, 1000.0, 0.0, 40.0, 40.0);
    a.sel = vec![x, y];
    a.make_component();
    a.sel = vec![x, y];
    a.combine_as_variants();
    assert_eq!(a.shapes.iter().filter(|s| s.component_set).count(), 2);
}

#[test]
fn while_hovering_and_while_pressing_change_the_variant_and_go_back() {
    let mut a = App::default();
    // a component set with two variants: Default (has a button layer) and Active
    let v1 = frame(&mut a, 0.0, 0.0, 60.0, 30.0);
    let btn = rect_at(&mut a, 5.0, 5.0);
    let v2 = frame(&mut a, 100.0, 0.0, 60.0, 30.0);
    a.sel = vec![v1, v2];
    a.make_component();
    a.sel = vec![v1, v2];
    a.combine_as_variants();
    let bi = a.index_of(btn).unwrap();
    a.shapes[bi].trigger = Trigger::WhileHover;
    a.shapes[bi].action = Action::ChangeTo;
    a.shapes[bi].link = Some(v2);
    // a screen with an instance of Default in it
    let screen = frame(&mut a, 0.0, 400.0, 300.0, 200.0);
    let inst = a.instantiate(v1, Some(screen), Pos2::new(20.0, 20.0)).unwrap();
    let shows = |a: &App| {
        let i = a.shapes.iter().position(|s| s.instance && s.parent == Some(screen)).unwrap();
        a.shapes[a.main_of(i).unwrap()].id
    };
    assert_eq!(shows(&a), v1);
    a.start_present(screen);
    let over = Pos2::new(20.0 + 5.0 + 5.0, 400.0 + 20.0 + 5.0 + 5.0); // inside the button
    let away = Pos2::new(250.0, 580.0);

    // hovering the button shows Active; the pointer staying over it keeps it; leaving goes back
    a.present_hover_at(Some(over), 1.0);
    assert_eq!(shows(&a), v2, "While hovering changed the instance");
    a.present_hover_at(Some(over), 1.1);
    assert_eq!(shows(&a), v2);
    a.present_hover_at(Some(away), 1.2);
    assert_eq!(shows(&a), v1, "and it went back when the pointer left");
    assert!(a.present_while.is_empty());
    let _ = inst;

    // While pressing: the same on the pointer going down and up
    let bi = a.shapes.iter().position(|s| s.parent.is_some_and(|p| a.index_of(p).is_some_and(|k| a.shapes[k].instance)) && s.trigger == Trigger::WhileHover).unwrap();
    a.shapes[bi].trigger = Trigger::WhilePress;
    a.present_press(over);
    assert_eq!(shows(&a), v2, "While pressing changed the instance");
    a.present_release();
    assert_eq!(shows(&a), v1, "and it went back on release");
}

#[test]
fn overlays_open_at_a_manual_position_with_their_animation() {
    let mut a = App::default();
    let base = frame(&mut a, 100.0, 100.0, 300.0, 200.0);
    let btn = rect_at(&mut a, 110.0, 110.0);
    let ovl = frame(&mut a, 1000.0, 0.0, 80.0, 40.0);
    let bi = a.index_of(btn).unwrap();
    a.shapes[bi].action = Action::OpenOverlay;
    a.shapes[bi].link = Some(ovl);
    a.shapes[bi].overlay_pos = 7;
    a.shapes[bi].overlay_off = Vec2::new(30.0, 40.0);
    a.shapes[bi].overlay_dim = false;
    a.shapes[bi].transition = Transition::SlideLeft;
    a.shapes[bi].duration = 0.4;
    a.start_present(base);
    a.present_click(Pos2::new(115.0, 115.0), 5.0);
    let o = a.present_overlays[0];
    // the overlay's top-left lands 30, 40 inside the presented frame
    let at = a.world_rect(a.index_of(ovl).unwrap()).min + o.shift;
    assert_eq!(at, Pos2::new(130.0, 140.0));
    assert!(!o.dim);
    assert_eq!((o.tr, o.dur, o.t0), (Transition::SlideLeft, 0.4, 5.0));
    // the primary interaction carries the new fields to and from the interaction list
    let ix = a.shapes[bi].primary_interaction();
    assert_eq!((ix.overlay_pos, ix.overlay_off, ix.overlay_dim), (7, Vec2::new(30.0, 40.0), false));

    // closing plays the animation backwards: a click outside starts it, the overlay stays until it has run, and it blocks nothing meanwhile
    a.present_click(Pos2::new(390.0, 290.0), 6.0);
    assert_eq!(a.present_overlays.len(), 1, "still on screen while it leaves");
    assert_eq!(a.present_overlays[0].closing, Some(6.0));
    assert!(a.top_overlay().is_none(), "a closing overlay no longer takes clicks");
    a.prune_overlays(6.2);
    assert_eq!(a.present_overlays.len(), 1, "half way out");
    a.prune_overlays(6.5);
    assert!(a.present_overlays.is_empty(), "gone once the animation has run");
    // without an animation it is gone at once
    a.shapes[bi].transition = Transition::Instant;
    a.present_click(Pos2::new(115.0, 115.0), 7.0);
    a.present_click(Pos2::new(390.0, 290.0), 7.1);
    assert!(a.present_overlays.is_empty());
}

#[test]
fn files_browser_projects_recents_and_file_operations() {
    let dir = std::env::temp_dir().join(format!("drui-files-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    unsafe { std::env::set_var("DRUI_FILES", &dir) };
    let mut a = App::default();

    // a project, and files in it and in Drafts
    assert_eq!(a.files_new_project("Website: v2?"), Some("Website- v2-".to_string()), "unsafe characters are replaced");
    assert_eq!(crate::files::projects(), vec!["Website- v2-".to_string()]);
    let p1 = a.files_new_file("Website- v2-", "Home page").unwrap();
    let p2 = a.files_new_file("", "Sketch").unwrap();
    let p3 = a.files_new_file("", "Sketch").unwrap();
    assert_ne!(p2, p3, "a taken name gets a number");
    assert_eq!(a.file, p3, "a new file is opened");
    assert_eq!(a.pages.len(), 1);

    // listing, search and Recents (the browser notes recents itself; the app does not in tests)
    assert_eq!(crate::files::list("Website- v2-", "").len(), 1);
    assert_eq!(crate::files::list("", "").len(), 2);
    assert_eq!(crate::files::list("", "sket").len(), 2);
    assert!(crate::files::list("", "home").is_empty());
    crate::files::note_recent(&p1);
    crate::files::note_recent(&p2);
    crate::files::note_recent(&p1);
    let recent = crate::files::list(crate::files::RECENTS, "");
    assert_eq!(recent.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["Home page", "Sketch", "Sketch 2"], "newest first, no repeats (new files count as recent too)");

    // duplicate, rename, delete
    let copy = a.files_duplicate(&p1).unwrap();
    assert_eq!(copy.file_stem().unwrap(), "Home page copy");
    let renamed = a.files_rename(&copy, "Landing").unwrap();
    assert!(renamed.is_file() && !copy.exists());
    assert!(a.files_rename(&renamed, "Home page").is_none(), "no overwriting another file");
    assert!(a.files_delete(&renamed));
    assert_eq!(crate::files::list("Website- v2-", "").len(), 1);
    assert_eq!(crate::files::ago(Some(std::time::SystemTime::now())), "just now");
    assert_eq!(crate::files::ago(Some(std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 86400))), "3 days ago");

    // the window draws
    let ctx = egui::Context::default();
    a.files.open = true;
    a.files.project = crate::files::RECENTS.into();
    for _ in 0..3 {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))), ..Default::default() };
        let _ = ctx.run_ui(raw, |ui| a.files_window(ui.ctx()));
    }
    assert!(a.files.open);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn typing_lines_into_a_text_box_grows_it() {
    let mut h = Harness::new();
    let mut a = App::default();
    h.frame(&mut a, vec![]);
    a.rulers = false;
    a.set_tool(crate::Tool::Draw(Kind::Text));
    h.click(&mut a, scr(100.0, 100.0));
    assert!(a.editing_text.is_some(), "a click with the text tool starts editing");
    h.frame(&mut a, vec![egui::Event::Text("asdasdasdasdasdasdadasdasdasdasdasdasdasdasdasasd".into())]);
    h.frame(&mut a, vec![]);
    eprintln!("after long line {:?}", a.shapes[0].size);
    for line in ["asda", "sdas", "dasd"] {
        h.frame(&mut a, vec![egui::Event::Text(line.into())]);
        h.frame(&mut a, vec![Harness::key(egui::Key::Enter)]);
        h.frame(&mut a, vec![]);
    }
    let s = &a.shapes[0];
    eprintln!("text {:?} size {:?} fit {:?}", s.text, s.size, s.fit);
    assert!(s.size.y > 40.0, "height follows the lines: {:?}", s.size);
}

#[test]
fn typing_in_the_full_window_grows_the_text_box() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let mut t = 0.0;
    let mut frame = |a: &mut App, events: Vec<egui::Event>| {
        t += 0.1;
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))), time: Some(t), events, ..Default::default() };
        let _ = ctx.run_ui(raw, |ui| a.run_frame(ui));
    };
    frame(&mut a, vec![]);
    a.add(Kind::Text, Pos2::new(100.0, 100.0));
    let id = a.sel[0];
    a.editing_text = Some(id);
    a.shapes[0].text.clear();
    frame(&mut a, vec![]);
    for ch in "asdasdasdasdasdasdasd".chars() {
        frame(&mut a, vec![egui::Event::Text(ch.to_string())]);
    }
    for line in ["asda", "sdas", "dasd"] {
        frame(&mut a, vec![Harness::key(egui::Key::Enter)]);
        frame(&mut a, vec![egui::Event::Text(line.into())]);
        frame(&mut a, vec![]);
    }
    let s = &a.shapes[0];
    eprintln!("FULL text {:?} size {:?} fit {:?} editing {:?}", s.text, s.size, s.fit, a.editing_text);
    assert!(s.size.y > 60.0, "height follows the lines: {:?}", s.size);
}

#[test]
fn short_hex_colours_expand_like_figma() {
    use crate::fig::parse_hex;
    assert_eq!(parse_hex("fa0"), Some(0xffaa00));
    assert_eq!(parse_hex("#F00"), Some(0xff0000));
    assert_eq!(parse_hex("ab"), Some(0xababab));
    assert_eq!(parse_hex("7"), Some(0x777777));
    assert_eq!(parse_hex("123456"), Some(0x123456));
    assert_eq!(parse_hex("1234"), None);
    assert_eq!(parse_hex("xyz"), None);
    assert_eq!(parse_hex(""), None);
}

#[test]
fn page_hex_field_keeps_its_height_while_editing() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let mut t = 0.0;
    let mut frame = |a: &mut App, events: Vec<egui::Event>| -> Vec<f32> {
        t += 0.1;
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))), time: Some(t), events, ..Default::default() };
        let out = ctx.run_ui(raw, |ui| a.run_frame(ui));
        let mut v = vec![];
        for cs in &out.shapes {
            if let egui::Shape::Rect(r) = &cs.shape {
                if r.fill == crate::theme::LINE() && r.rect.left() > 1190.0 && r.rect.height() <= 1.5 {
                    v.push(r.rect.top());
                }
            }
        }
        v
    };
    frame(&mut a, vec![]);
    let idle = frame(&mut a, vec![]);
    let p = Pos2::new(1260.0, 137.0);
    frame(&mut a, vec![egui::Event::PointerMoved(p)]);
    frame(&mut a, vec![Harness::button(p, true)]);
    frame(&mut a, vec![Harness::button(p, false)]);
    frame(&mut a, vec![]);
    let active = frame(&mut a, vec![]);
    assert_eq!(idle, active, "the panel keeps its layout while a field is being edited");
}



#[test]
fn notes_follow_their_layer_and_never_overlap() {
    let mut a = App::default();
    let f = frame(&mut a, 100.0, 100.0, 200.0, 100.0);
    let fi = a.index_of(f).unwrap();
    let long = "A long note that wraps over several lines and so ends up taller than the guess the API made when it placed the note. ".repeat(3);
    let n1 = a.add_note(&serde_json::json!({ "text": long, "near": f })).unwrap()["id"].as_u64().unwrap();
    let n2 = a.add_note(&serde_json::json!({ "text": "Second", "near": f })).unwrap()["id"].as_u64().unwrap();
    let ctx = egui::Context::default();
    let settle = |a: &mut App| {
        for _ in 0..3 {
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| a.fit_texts(ui.painter()));
            a.settle();
        }
    };
    let rect = |a: &App, id: u64| { let s = &a.shapes[a.index_of(id).unwrap()]; egui::Rect::from_min_size(s.pos, s.size) };
    settle(&mut a);
    // laid out: the first note has grown to its text and the second is still clear of it
    assert!(rect(&a, n1).height() > 60.0, "the long note grew: {:?}", rect(&a, n1));
    assert!(!rect(&a, n1).intersects(rect(&a, n2)), "{:?} {:?}", rect(&a, n1), rect(&a, n2));
    assert_eq!(rect(&a, n1).min.x, 100.0 + 200.0 + 32.0);
    // settling again changes nothing
    let before = (rect(&a, n1), rect(&a, n2));
    settle(&mut a);
    assert_eq!((rect(&a, n1), rect(&a, n2)), before);
    // the layer moves, the notes go with it
    a.shapes[fi].pos = egui::pos2(500.0, 300.0);
    settle(&mut a);
    assert_eq!(rect(&a, n1).min, egui::pos2(500.0 + 200.0 + 32.0, 300.0));
    assert!(!rect(&a, n1).intersects(rect(&a, n2)));
    // the layer is deleted: the notes stay where they are
    let at = rect(&a, n1).min;
    a.shapes.remove(fi);
    settle(&mut a);
    assert_eq!(rect(&a, n1).min, at);
}


// writes a document with a vector path, a frame, a text layer, a main component and an instance to $DRUI_PATHDOC for checking
// panels and menus in the real app. Does nothing unless it is set.
#[test]
fn dump_path_doc_for_menu_check() {
    let Ok(file) = std::env::var("DRUI_PATHDOC") else { return };
    let mut a = App::default();
    let r = rect_at(&mut a, 40.0, 40.0);
    a.sel = vec![r];
    a.flatten();
    let k = a.shapes.len() - 1;
    a.shapes[k].name = "A path".into();
    a.shapes[k].exports = vec![ExportSetting { format: ExportFormat::Svg, ..ExportSetting::default() }];
    let f = frame(&mut a, 100.0, 40.0, 80.0, 60.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].name = "A frame".into();
    a.shapes[fi].grids = vec![LayoutGrid::new(GridKind::Columns), LayoutGrid::new(GridKind::Grid)];
    a.add(Kind::Text, Pos2::new(200.0, 40.0));
    let t = a.shapes.len() - 1;
    a.shapes[t].name = "A text".into();
    a.shapes[t].text = "Hello".into();
    let c = rect_at(&mut a, 300.0, 40.0);
    a.sel = vec![c];
    a.make_component();
    let cid = a.sel[0];
    let ci = a.index_of(cid).unwrap();
    a.shapes[ci].name = "A component".into();
    let inst = a.instantiate(cid, None, Pos2::new(400.0, 40.0)).unwrap();
    let n = a.index_of(inst).unwrap();
    a.shapes[n].name = "An instance".into();
    a.save_to(std::path::PathBuf::from(file));
}

#[test]
fn svg_export_can_carry_layer_ids() {
    let mut a = App::default();
    let r1 = rect_at(&mut a, 0.0, 0.0);
    let r2 = rect_at(&mut a, 20.0, 0.0);
    for (id, name) in [(r1, "Hero card"), (r2, "Hero card")] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].name = name.into();
    }
    let ctx = egui::Context::default();
    let (plain, _) = a.svg_for(&ctx, &[r1, r2]).unwrap();
    assert!(!plain.contains(" id=\"Hero"));
    let (with, _) = a.svg_for_ids(&ctx, &[r1, r2], true).unwrap();
    // spaces become underscores and a repeated name gets a number
    assert!(with.contains("id=\"Hero_card\"") && with.contains("id=\"Hero_card_1\""), "{with}");
}

#[test]
fn vertical_trim_cuts_the_box_to_cap_height_and_baseline() {
    let mut a = App::default();
    a.add(Kind::Text, Pos2::new(0.0, 0.0));
    let id = a.shapes.last().unwrap().id;
    let i = a.index_of(id).unwrap();
    a.shapes[i].text = "Hello".into();
    a.shapes[i].fit = TextFit::AutoWidth;
    let ctx = egui::Context::default();
    let height = |a: &mut App| {
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| a.fit_texts(ui.painter()));
        a.shapes[a.index_of(id).unwrap()].size.y
    };
    let standard = height(&mut a);
    let i = a.index_of(id).unwrap();
    a.shapes[i].vtrim = 1;
    let (top, bottom) = a.shapes[i].trim_px();
    assert!(top > 0.0 && bottom > 0.0);
    let trimmed = height(&mut a);
    assert!((standard - trimmed - top - bottom).abs() <= 1.0, "{standard} {trimmed} {top} {bottom}");
}

#[test]
fn prototype_html_scrolls_overflow_and_smart_animates() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let home = frame(&mut a, 0.0, 0.0, 200.0, 300.0);
    let detail = frame(&mut a, 400.0, 0.0, 200.0, 300.0);
    let btn = rect_at(&mut a, 20.0, 40.0);
    let far = rect_at(&mut a, 20.0, 500.0);
    for (id, size) in [(btn, Vec2::new(80.0, 30.0)), (far, Vec2::new(80.0, 100.0))] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].parent = Some(home);
        a.shapes[i].size = size;
    }
    let bi = a.index_of(btn).unwrap();
    a.shapes[bi].action = Action::ScrollTo;
    a.shapes[bi].link = Some(far);
    a.shapes[bi].more.push(Interaction { trigger: Trigger::Click, action: Action::Navigate, link: Some(detail), transition: Transition::SmartAnimate, ..Interaction::default() });
    let mut html = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| html = a.prototype_html(ui.ctx(), Some(home)));
    let html = html.unwrap();
    let d: serde_json::Value = serde_json::from_str(html.split("const D = ").nth(1).unwrap().split(";\ndocument.getElementById('stage')").next().unwrap()).unwrap();
    let f = &d["frames"][home.to_string()];
    assert!(f["sr"][1].as_f64().unwrap() > 200.0, "the content beyond the frame can be scrolled: {f}");
    assert_eq!(d["frames"][detail.to_string()]["sr"][1].as_f64(), Some(0.0));
    let ix = f["spots"][0]["ix"].as_array().unwrap();
    let scroll = ix.iter().find(|i| i["do"] == "scroll").expect("a scroll action");
    assert_eq!((scroll["sx"].as_f64(), scroll["sy"].as_f64()), (Some(20.0), Some(500.0)));
    assert_eq!(ix.iter().find(|i| i["do"] == "nav").unwrap()["tr"], "smart");
    assert!(html.contains("class=\"sc\""), "a scrollable screen has a content layer");
    // ids are scoped per screen: two screens must not share a clip or a layer id
    assert!(html.contains(&format!("id=\"s{home}_")) && html.contains(&format!("id=\"s{detail}_")));
    if let Some(script) = html.split("<script>").nth(1).and_then(|r| r.split("</script>").next()) {
        let path = std::env::temp_dir().join(format!("drui-player2-{}.js", std::process::id()));
        std::fs::write(&path, script).unwrap();
        if let Ok(out) = std::process::Command::new("node").arg("--check").arg(&path).output() {
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        }
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn timeline_shows_frames_and_zooms() {
    assert_eq!(crate::anim::fmt_time(1000.0, true), "30 f");
    assert_eq!(crate::anim::fmt_time(1234.4, false), "1234 ms");
    let mut a = App::default();
    a.add(Kind::Rect, Pos2::ZERO);
    let id = a.shapes.last().unwrap().id;
    let i = a.index_of(id).unwrap();
    let key = |t: f32, x: f32, ease| Key { t, v: [x, 0.0, 0.0, 0.0], ease };
    a.shapes[i].tracks = vec![Track { prop: Prop::X, keys: vec![key(0.0, 0.0, Easing::EaseInOut), key(1000.0, 50.0, Easing::SpringBouncy), key(2000.0, 0.0, Easing::Linear)] }];
    a.sel = vec![id];
    a.anim_frames = true;
    a.tl_view = (4.0, 500.0);
    let ctx = egui::Context::default();
    for _ in 0..2 {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 300.0))), ..Default::default() };
        let _ = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| a.timeline(ui));
        });
    }
    let (z, start) = a.tl_view;
    assert_eq!(z, 4.0);
    assert!((0.0..=1500.0).contains(&start), "the window stays inside the timeline: {start}");
    // zoomed out past the whole length: back to showing everything
    a.tl_view = (1.0, 900.0);
    let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 300.0))), ..Default::default() };
    let _ = ctx.run_ui(raw, |ui| {
        egui::CentralPanel::default().show(ui, |ui| a.timeline(ui));
    });
    assert_eq!(a.tl_view, (1.0, 0.0));
}

#[test]
fn a_mask_clips_to_its_real_outline_on_the_canvas_raster() {
    let mut a = App::default();
    a.add(Kind::Ellipse, Pos2::ZERO);
    let mask = a.shapes.last().unwrap().id;
    a.add(Kind::Rect, Pos2::new(-50.0, -50.0));
    let body = a.shapes.last().unwrap().id;
    let (mi, bi) = (a.index_of(mask).unwrap(), a.index_of(body).unwrap());
    a.shapes[mi].size = Vec2::new(100.0, 100.0);
    a.shapes[mi].mask = true;
    a.shapes[bi].size = Vec2::new(200.0, 200.0);
    a.shapes[bi].fills = vec![Paint::solid(egui::Color32::RED)];
    a.shapes[bi].strokes.clear();
    let run: Vec<Shape> = vec![a.shapes[mi].clone(), a.shapes[bi].clone()];
    let region = Rect::from_min_size(Pos2::new(-2.0, -2.0), Vec2::splat(104.0));
    let img = a.masked_run_image(&egui::Context::default(), &run, Vec2::ZERO, region, 104, 104).expect("a picture");
    let at = |x: usize, y: usize| img.pixels[y * 104 + x];
    assert_eq!(at(52, 52), egui::Color32::RED, "the middle of the circle shows the layer");
    assert_eq!(at(5, 5).a(), 0, "the corner of the mask's box is outside the circle, so it is hidden");
    assert_eq!(at(98, 98).a(), 0);
    // a plain square-cornered rectangle mask keeps the fast box clip; a round one needs the picture
    assert!(a.is_box_mask(&{ let mut r = a.shapes[bi].clone(); r.kind = Kind::Rect; r.rotation = 0.0; r.radius = [0.0; 4]; r }));
    assert!(!a.is_box_mask(&a.shapes[mi]), "an ellipse is not a box");
}

#[test]
fn mask_types_cut_by_outline_transparency_or_brightness() {
    let build = |kind: u8, mask_fill: egui::Color32| {
        let mut a = App::default();
        a.add(Kind::Rect, Pos2::ZERO);
        let mask = a.shapes.last().unwrap().id;
        a.add(Kind::Rect, Pos2::ZERO);
        let body = a.shapes.last().unwrap().id;
        let (mi, bi) = (a.index_of(mask).unwrap(), a.index_of(body).unwrap());
        // the mask is a 100 x 100 box; its fill is the colour under test, at half opacity
        a.shapes[mi].size = Vec2::new(100.0, 100.0);
        a.shapes[mi].mask = true;
        a.shapes[mi].mask_kind = kind;
        a.shapes[mi].strokes.clear();
        a.shapes[mi].fills = vec![Paint::solid(mask_fill)];
        a.shapes[bi].size = Vec2::new(100.0, 100.0);
        a.shapes[bi].strokes.clear();
        a.shapes[bi].fills = vec![Paint::solid(egui::Color32::RED)];
        let run = vec![a.shapes[mi].clone(), a.shapes[bi].clone()];
        let img = a.masked_run_image(&egui::Context::default(), &run, Vec2::ZERO, Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)), 100, 100).expect("raster");
        img.pixels[50 * 100 + 50]
    };
    let white = egui::Color32::WHITE;
    let black = egui::Color32::BLACK;
    let clear = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 0);
    // Vector: the outline decides, the mask's own paint does not matter
    assert!(build(0, black).a() > 250 && build(0, clear).a() > 250);
    // Alpha: an invisible mask hides everything, a solid one shows it, whatever the colour
    assert!(build(1, black).a() > 250);
    assert_eq!(build(1, clear).a(), 0);
    // Luminance: white shows, black hides
    assert!(build(2, white).a() > 250);
    assert!(build(2, black).a() < 5);
}

#[test]
fn the_page_background_has_an_opacity_and_an_eye() {
    let mut a = App::default();
    let solid = a.pages[0].background;
    assert_eq!(a.page_bg(), solid);
    a.pages[0].bg_visible = false;
    assert_eq!(a.page_bg(), egui::Color32::TRANSPARENT);
    a.pages[0].bg_visible = true;
    a.pages[0].background = egui::Color32::from_rgba_unmultiplied(40, 80, 120, 128);
    assert_eq!(a.page_bg().to_srgba_unmultiplied()[3], 128);
    // saved and read back; a file from before the eye existed shows the page colour
    let doc = a.doc();
    let text = serde_json::to_string(&doc).unwrap();
    let back: Doc = serde_json::from_str(&text).unwrap();
    assert!(back.pages[0].bg_visible && back.pages[0].background == a.pages[0].background);
    let old = text.replace("\"bg_visible\":true,", "");
    assert!(serde_json::from_str::<Doc>(&old).unwrap().pages[0].bg_visible);
}

#[test]
fn page_changes_and_page_look_are_undoable() {
    let mut a = App::default();
    a.add(Kind::Rect, Pos2::ZERO);
    a.pages[0].name = "First".into();
    a.add_page();
    assert_eq!((a.pages.len(), a.cur), (2, 1));
    a.add(Kind::Ellipse, Pos2::ZERO);
    a.shapes.last_mut().unwrap().name = "on page two".into();
    // undo the page add: back to one page, with its layer
    a.do_undo(true);
    assert_eq!((a.pages.len(), a.cur), (1, 0), "the added page is gone");
    assert_eq!(a.shapes.len(), 1);
    // redo brings the page back (with what it had)
    a.do_undo(false);
    assert_eq!(a.pages.len(), 2);
    // deleting a page is undone with its layers
    a.switch_page(1);
    a.pages[1].name = "Two".into();
    let layers = a.shapes.len();
    a.delete_page(1);
    assert_eq!(a.pages.len(), 1);
    a.do_undo(true);
    assert_eq!(a.pages.len(), 2);
    assert_eq!(a.pages[1].shapes.len().max(a.shapes.len()), layers.max(1));
    assert_eq!(a.pages[1].name, "Two");
    // moving a page and its undo
    a.move_page(0, 2);
    assert_eq!(a.pages[1].name, "First");
    a.do_undo(true);
    assert_eq!(a.pages[0].name, "First");
    // renaming is one step, and dropped when nothing changed
    let steps = a.undo.len();
    a.start_rename_page(0);
    a.end_rename_page(0);
    assert_eq!(a.undo.len(), steps, "an unchanged name leaves no step");
    a.start_rename_page(0);
    a.pages[0].name = "Renamed".into();
    a.end_rename_page(0);
    assert_eq!(a.undo.len(), steps + 1);
    a.do_undo(true);
    assert_eq!(a.pages[0].name, "First");
    // an edit on one page, a visit to another, then undo returns to the edited page
    a.switch_page(0);
    let before = a.shapes.len();
    a.add(Kind::Rect, Pos2::new(5.0, 5.0));
    a.undo.push(crate::Snap { ..a.snap() }); // (the frame loop does this for a gesture; here the state after is enough to test the page)
    a.switch_page(1);
    assert_eq!(a.cur, 1);
    assert!(!a.undo.is_empty(), "switching pages keeps the history");
    let _ = before;
}

#[test]
fn prototype_html_keeps_fixed_children_out_of_the_scrolling_layer() {
    let ctx = egui::Context::default();
    let mut a = App::default();
    let home = frame(&mut a, 0.0, 0.0, 200.0, 300.0);
    let header = rect_at(&mut a, 0.0, 0.0);
    let far = rect_at(&mut a, 20.0, 500.0);
    for (id, size, name) in [(header, Vec2::new(200.0, 40.0), "Header"), (far, Vec2::new(80.0, 100.0), "Footer")] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].parent = Some(home);
        a.shapes[i].size = size;
        a.shapes[i].name = name.into();
    }
    let hi = a.index_of(header).unwrap();
    a.shapes[hi].fixed = true;
    a.shapes[hi].link = Some(home);
    let mut html = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| html = a.prototype_html(ui.ctx(), Some(home)));
    let html = html.unwrap();
    assert!(html.contains("class=\"fx\""), "the fixed children are a layer of their own");
    assert!(html.contains(&format!("id=\"s{home}f_Header\"")), "the fixed layer has the header");
    assert!(!html.contains(&format!("id=\"s{home}_Header\"")), "and the scrolling layer does not");
    assert!(html.contains(&format!("id=\"s{home}_Footer\"")));
    let d: serde_json::Value = serde_json::from_str(html.split("const D = ").nth(1).unwrap().split(";\ndocument.getElementById('stage')").next().unwrap()).unwrap();
    assert_eq!(d["frames"][home.to_string()]["spots"][0]["fx"], true, "a fixed layer's hot spot does not scroll");
}

#[test]
fn glass_refraction_bends_the_backdrop_near_the_edge_and_dispersion_parts_the_colours() {
    let render = |refraction: f32, dispersion: f32| {
        let mut a = App::default();
        a.add(Kind::Rect, Pos2::ZERO);
        let bg = a.shapes.last().unwrap().id;
        a.add(Kind::Rect, Pos2::new(100.0, 100.0));
        let glass = a.shapes.last().unwrap().id;
        let (bi, gi) = (a.index_of(bg).unwrap(), a.index_of(glass).unwrap());
        a.shapes[bi].size = Vec2::new(300.0, 300.0);
        a.shapes[bi].strokes.clear();
        let mut ramp = Paint::solid(egui::Color32::BLACK);
        ramp.kind = PaintKind::Linear { a: egui::Color32::BLACK, b: egui::Color32::WHITE, angle: 0.0 };
        a.shapes[bi].fills = vec![ramp];
        a.shapes[gi].size = Vec2::new(100.0, 100.0);
        a.shapes[gi].strokes.clear();
        a.shapes[gi].fills.clear();
        let mut e = Effect::new(EffectKind::Glass);
        (e.blur, e.spread, e.radius, e.size, e.density) = (0.0, refraction, 40.0, dispersion, 0.0);
        a.shapes[gi].effects = vec![e];
        let ctx = egui::Context::default();
        let (svg, r) = a.svg_for(&ctx, &[bg, glass]).expect("svg");
        crate::export::rasterize_scene(&svg, r.width().ceil() as u32, r.height().ceil() as u32).expect("raster")
    };
    let (flat, bent, split) = (render(0.0, 0.0), render(100.0, 0.0), render(100.0, 100.0));
    let px = |img: &egui::ColorImage, x: usize, y: usize| img.pixels[y * img.width() + x];
    if std::env::var("DBG_GLASS").is_ok() { for x in (96..206).step_by(6) { eprintln!("x{x} flat {} bent {} split {:?}", px(&flat, x, 150).r(), px(&bent, x, 150).r(), px(&split, x, 150)); } }
    // the middle of the glass is untouched, the rim is not
    assert_eq!(px(&flat, 150, 150), px(&bent, 150, 150), "no bending in the middle of the glass");
    let rim = (px(&flat, 103, 150).r() as i32 - px(&bent, 103, 150).r() as i32).abs();
    assert!(rim > 3, "the backdrop is shifted at the left rim: {rim}");
    // outside the glass nothing changes
    assert_eq!(px(&flat, 50, 50), px(&bent, 50, 50));
    // dispersion: red, green and blue of a grey ramp no longer agree at the rim
    let p = px(&split, 103, 150);
    assert!(p.r() != p.b(), "the channels part: {p:?}");
    let q = px(&bent, 103, 150);
    assert_eq!(q.r(), q.b(), "without dispersion a grey ramp stays grey");
}

#[test]
fn the_canvas_backdrop_of_glass_is_bent_at_its_edge() {
    use crate::model::*;
    let ctx = egui::Context::default();
    let mut a = App::default();
    // a red square and a white one side by side, and a 60x40 glass layer sitting over the seam
    let red = rect_at(&mut a, 0.0, 0.0);
    let white = rect_at(&mut a, 34.0, 0.0);
    for (id, c) in [(red, egui::Color32::from_rgb(255, 0, 0)), (white, egui::Color32::WHITE)] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].size = Vec2::new(if id == red { 34.0 } else { 100.0 }, 100.0);
        a.shapes[i].strokes.clear();
        a.shapes[i].fills = vec![Paint::solid(c)];
    }
    let glass = rect_at(&mut a, 30.0, 20.0);
    let gi = a.index_of(glass).unwrap();
    a.shapes[gi].size = Vec2::new(40.0, 40.0);
    a.shapes[gi].fills = vec![];
    a.shapes[gi].strokes.clear();
    let mut e = Effect::new(EffectKind::Glass);
    (e.blur, e.spread, e.radius, e.size) = (0.0, 100.0, 60.0, 0.0);
    a.shapes[gi].effects = vec![e];
    let order = a.draw_order();
    let earlier: Vec<usize> = order.iter().copied().take_while(|&k| k != gi).collect();
    let reach = crate::export::glass_reach(&e, a.shapes[gi].aabb());
    assert!(reach > 5.0, "the backdrop reaches past the box: {reach}");
    let region = a.shapes[gi].aabb().expand(2.0 + reach);
    let (pw, ph) = (region.width() as u32 * 2, region.height() as u32 * 2);
    let flat = a.backdrop_image(&ctx, &earlier, &a.shapes[gi], Vec2::ZERO, region, 0.01, pw, ph, None).unwrap();
    let bent = a.backdrop_image(&ctx, &earlier, &a.shapes[gi], Vec2::ZERO, region, 0.01, pw, ph, Some(&e)).unwrap();
    let at = |img: &egui::ColorImage, wx: f32, wy: f32| img.pixels[((wy - region.min.y) * 2.0) as usize * pw as usize + ((wx - region.min.x) * 2.0) as usize];
    // the seam (x = 34) is 4px inside the glass's left rim (x = 30): the bend takes pixels from further out, so just right of
    // the seam a column that is white unbent shows red
    let (f, b) = (at(&flat, 36.0, 40.0), at(&bent, 36.0, 40.0));
    assert_eq!(f.g(), 255, "unbent: white right of the seam");
    assert!(b.g() < 200, "bent: pulled from the red outside the rim: {b:?}");
    // the middle is untouched
    assert_eq!(at(&flat, 55.0, 40.0), at(&bent, 55.0, 40.0));
}

#[test]
fn width_profiles_shape_an_open_strokes_outline() {
    use crate::geom::{profile_width, variable_width_outline};
    let line = [Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0)];
    let width_at = |out: &[Pos2], x: f32| {
        // the outline's vertical extent where its x is closest to `x`
        let near: Vec<&Pos2> = out.iter().filter(|p| (p.x - x).abs() < 1.5).collect();
        near.iter().map(|p| p.y).fold(f32::MIN, f32::max) - near.iter().map(|p| p.y).fold(f32::MAX, f32::min)
    };
    let taper = variable_width_outline(&line, 10.0, &|t| profile_width(1, t)).unwrap();
    assert!((width_at(&taper, 1.0) - 10.0).abs() < 1.0, "thick at the start");
    assert!(width_at(&taper, 99.0) < 1.0, "thin at the end");
    let lens = variable_width_outline(&line, 10.0, &|t| profile_width(3, t)).unwrap();
    assert!(width_at(&lens, 50.0) > 9.0 && width_at(&lens, 1.0) < 1.0 && width_at(&lens, 99.0) < 1.0);
    let pinch = variable_width_outline(&line, 10.0, &|t| profile_width(4, t)).unwrap();
    assert!(width_at(&pinch, 50.0) < 3.0 && width_at(&pinch, 1.0) > 9.0);
    assert_eq!((profile_width(0, 0.3), profile_width(2, 0.25)), (1.0, 0.25));
    assert!(variable_width_outline(&[Pos2::ZERO], 4.0, &|_| 1.0).is_none() && variable_width_outline(&[Pos2::ZERO, Pos2::ZERO], 4.0, &|_| 1.0).is_none());
    // on a layer: the SVG draws a filled outline instead of a stroke, and the canvas draws it from the picture
    let mut a = App::default();
    a.add(Kind::Line, Pos2::ZERO);
    let id = a.shapes.last().unwrap().id;
    let i = a.index_of(id).unwrap();
    a.shapes[i].size = Vec2::new(100.0, 0.0);
    a.shapes[i].stroke_w = 10.0;
    let uniform = a.svg_for(&egui::Context::default(), &[id]).unwrap().0;
    assert!(uniform.contains("stroke-width=\"10\""));
    a.shapes[i].width_profile = 1;
    assert!(a.shapes[i].needs_sprite());
    let tapered = a.svg_for(&egui::Context::default(), &[id]).unwrap().0;
    assert!(!tapered.contains("stroke-width=\"10\"") && tapered.contains("Z\" fill="), "{tapered}");
    let img = crate::export::rasterize_scene(&tapered, 194, 94).unwrap();
    let col_alpha = |x: usize| (0..img.height()).filter(|&y| img.pixels[y * img.width() + x].a() > 128).count();
    assert!(col_alpha(47 + 5) > col_alpha(47 + 90) + 4, "thicker at the start: {} vs {}", col_alpha(47 + 5), col_alpha(47 + 90));
}

#[test]
fn the_variable_tab_sets_inters_weight_and_optical_size() {
    let mut a = App::default();
    a.add(Kind::Text, Pos2::ZERO);
    let id = a.shapes.last().unwrap().id;
    let i = a.index_of(id).unwrap();
    a.shapes[i].text = "Hello".into();
    a.shapes[i].fit = TextFit::AutoWidth;
    let ctx = egui::Context::default();
    // the axes the layout job carries (the test context has no Inter loaded, so widths cannot show them)
    let coords = |a: &App| {
        let mut c = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let job = crate::textrun::text_job(ui.painter(), &a.shapes[a.index_of(id).unwrap()], 1.0, egui::Color32::WHITE, f32::INFINITY, false);
            c = Some(job.sections[0].format.coords.clone());
        });
        c.unwrap()
    };
    let plain = coords(&a);
    a.shapes[i].weight = 900;
    let heavy = coords(&a);
    assert!(heavy != plain, "a weight sets the wght axis");
    a.shapes[i].weight = 0;
    a.shapes[i].opsz = 32.0;
    let display = coords(&a);
    assert!(display != plain && display != heavy, "the optical size sets the opsz axis");
    a.shapes[i].opsz = 99.0;
    assert!(coords(&a) == display, "the axis is kept within Inter's 14 to 32");
    // saved with the layer, and in the SVG
    a.shapes[i].opsz = 32.0;
    let svg = a.svg_for(&ctx, &[id]).unwrap().0;
    assert!(svg.contains("font-variation-settings:'opsz' 32"), "{svg}");
    let back: Shape = serde_json::from_str(&serde_json::to_string(&a.shapes[i]).unwrap()).unwrap();
    assert_eq!(back.opsz, 32.0);
}

#[test]
fn wrap_style_balances_lines_and_avoids_a_lone_last_word() {
    let mut a = App::default();
    a.add(Kind::Text, Pos2::ZERO);
    let id = a.shapes.last().unwrap().id;
    let i = a.index_of(id).unwrap();
    a.shapes[i].text = "Make the headline read evenly across the lines it needs".into();
    a.shapes[i].fit = TextFit::AutoHeight;
    a.shapes[i].font_size = 16.0;
    let ctx = egui::Context::default();
    // (rows, words in the last row, widest row, narrowest row) at a box width and wrap style
    let lines = |a: &mut App, w: f32, style: u8| {
        let i = a.index_of(id).unwrap();
        a.shapes[i].size.x = w;
        a.shapes[i].wrap_style = style;
        let mut out = (0, 0, 0.0f32, f32::MAX);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let s = &a.shapes[i];
            let g = crate::render::text_galley(ui.painter(), s, 1.0, egui::Color32::WHITE, crate::render::wrap_of(s));
            let widths: Vec<f32> = g.rows.iter().map(|r| r.rect().width()).collect();
            out = (g.rows.len(), g.rows.last().map_or(0, |r| r.row.text().split_whitespace().count()), widths.iter().copied().fold(0.0, f32::max), widths.iter().copied().fold(f32::MAX, f32::min));
        });
        out
    };
    let (mut balanced_better, mut pretty_fixed) = (false, false);
    for w in (100..400).step_by(7) {
        let w = w as f32;
        let auto = lines(&mut a, w, 0);
        let bal = lines(&mut a, w, 1);
        let pretty = lines(&mut a, w, 2);
        assert_eq!(bal.0, auto.0, "balance keeps the number of lines at {w}");
        assert!(bal.2 <= auto.2 + 0.01, "no row gets wider at {w}");
        if auto.0 >= 2 && auto.0 <= 6 && (bal.2 - bal.3) + 1.0 < (auto.2 - auto.3) {
            balanced_better = true;
        }
        assert!(pretty.0 <= auto.0, "pretty never adds a line at {w}");
        if auto.0 >= 2 && auto.1 == 1 && pretty.1 >= 2 {
            pretty_fixed = true;
        }
    }
    assert!(balanced_better, "balance evened out the lines somewhere");
    assert!(pretty_fixed, "pretty gave a lone last word company somewhere");
    // one line is left alone
    assert_eq!(lines(&mut a, 2000.0, 1), lines(&mut a, 2000.0, 0));
}

#[test]
fn glass_splay_spreads_the_bend_towards_the_middle() {
    use crate::model::*;
    let ctx = egui::Context::default();
    let make = |splay: f32| {
        let mut a = App::default();
        let red = rect_at(&mut a, 0.0, 0.0);
        let white = rect_at(&mut a, 38.0, 0.0);
        for (id, c, w) in [(red, egui::Color32::from_rgb(255, 0, 0), 38.0), (white, egui::Color32::WHITE, 100.0)] {
            let i = a.index_of(id).unwrap();
            a.shapes[i].size = Vec2::new(w, 100.0);
            a.shapes[i].strokes.clear();
            a.shapes[i].fills = vec![Paint::solid(c)];
        }
        let glass = rect_at(&mut a, 30.0, 20.0);
        let gi = a.index_of(glass).unwrap();
        a.shapes[gi].size = Vec2::new(40.0, 40.0);
        a.shapes[gi].fills = vec![];
        a.shapes[gi].strokes.clear();
        let mut e = Effect::new(EffectKind::Glass);
        (e.blur, e.spread, e.radius, e.size, e.splay) = (0.0, 100.0, 60.0, 0.0, splay);
        a.shapes[gi].effects = vec![e];
        let order = a.draw_order();
        let earlier: Vec<usize> = order.iter().copied().take_while(|&k| k != gi).collect();
        let region = a.shapes[gi].aabb().expand(2.0 + crate::export::glass_reach(&e, a.shapes[gi].aabb()));
        let (pw, ph) = (region.width() as u32 * 2, region.height() as u32 * 2);
        let img = a.backdrop_image(&ctx, &earlier, &a.shapes[gi], Vec2::ZERO, region, 0.01, pw, ph, Some(&e)).unwrap();
        // the pixel 10 px inside the left rim, mid height
        img.pixels[((40.0 - region.min.y) * 2.0) as usize * pw as usize + ((40.0 - region.min.x) * 2.0) as usize]
    };
    let (tight, spread) = (make(0.0), make(100.0));
    assert_eq!(tight.g(), 255, "without splay the bend has died away 10px in: {tight:?}");
    assert!(spread.g() < 200, "with splay it still reaches there: {spread:?}");
}

#[test]
fn a_custom_width_profile_follows_its_points() {
    use crate::geom::points_width;
    let pts = [(0.0, 1.0), (0.5, 0.2), (1.0, 0.6)];
    assert_eq!(points_width(&pts, 0.0), 1.0);
    assert!((points_width(&pts, 0.25) - 0.6).abs() < 1e-5, "straight between the points");
    assert!((points_width(&pts, 0.5) - 0.2).abs() < 1e-5);
    assert!((points_width(&pts, 1.0) - 0.6).abs() < 1e-5);
    assert_eq!(points_width(&[], 0.3), 1.0, "no points: the full width");
    // unsorted points are read in order, and a point before the first holds flat
    assert!((points_width(&[(1.0, 0.0), (0.0, 1.0)], 0.5) - 0.5).abs() < 1e-5);
    let mut a = App::default();
    a.add(Kind::Line, Pos2::ZERO);
    let i = a.shapes.len() - 1;
    a.shapes[i].size = Vec2::new(100.0, 0.0);
    a.shapes[i].stroke_w = 10.0;
    a.shapes[i].width_profile = 5;
    a.shapes[i].width_points = vec![(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)];
    assert_eq!(a.shapes[i].width_at(0.5), 0.0);
    assert_eq!(a.shapes[i].width_at(0.0), 1.0);
    let id = a.shapes[i].id;
    let svg = a.svg_for(&egui::Context::default(), &[id]).unwrap().0;
    assert!(!svg.contains("stroke-width=\"10\"") && svg.contains("Z\" fill="), "a custom profile is drawn as an outline");
    // saved with the layer
    let back: Shape = serde_json::from_str(&serde_json::to_string(&a.shapes[i]).unwrap()).unwrap();
    assert_eq!(back.width_points.len(), 3);
}

#[test]
fn a_video_fill_has_a_poster_saves_with_its_file_and_plays_frames() {
    let Some(ffmpeg) = crate::video::ffmpeg().cloned() else { return eprintln!("ffmpeg not found: skipped") };
    // a one second test video (colour bars that move)
    let dir = std::env::temp_dir().join(format!("drui-test-video-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("bars.mp4");
    let ok = std::process::Command::new(&ffmpeg).args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=12", "-t", "1", "-pix_fmt", "yuv420p"]).arg(&file).status().unwrap().success();
    assert!(ok, "ffmpeg made the test video");
    let bytes = std::fs::read(&file).unwrap();
    let mut a = App::default();
    a.add(Kind::Rect, Pos2::ZERO);
    let i = a.shapes.len() - 1;
    a.shapes[i].fills = vec![Paint::solid(egui::Color32::GRAY)];
    assert!(a.attach_video(i, false, 0, "bars", "mp4", bytes), "{}", a.status);
    let (poster, vid) = match (&a.shapes[i].fills[0].kind, a.shapes[i].fills[0].video) {
        (PaintKind::Image(p), Some(v)) => (*p, v),
        other => panic!("not a video fill: {other:?}"),
    };
    let asset = a.images.iter().find(|x| x.id == poster).expect("the poster is an image asset");
    let png = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &asset.data).unwrap();
    let img = image::load_from_memory(&png).unwrap();
    assert_eq!((img.width(), img.height()), (160, 90), "the poster is the video's first frame");
    // saved with the file and read back
    let text = serde_json::to_string(&a.doc()).unwrap();
    let back: Doc = serde_json::from_str(&text).unwrap();
    assert_eq!(back.videos.len(), 1);
    assert_eq!(back.videos[0].id, vid);
    assert_eq!(back.pages[0].shapes[0].fills[0].video, Some(vid));
    // an old file without any video reads as before
    let old: Doc = serde_json::from_str(&text.replace("\"videos\"", "\"videos_gone\"")).unwrap();
    assert!(old.videos.is_empty());
    // the frames are cut in the background; then the layer's texture follows the time and loops
    let ctx = egui::Context::default();
    for _ in 0..100 {
        a.update_video_textures(&ctx, 0.0);
        if a.clips.get(&vid).is_some_and(|c| c.frames() >= 12) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let clip = a.clips.get(&vid).expect("a clip");
    assert!(clip.frames() >= 12, "12 frames for one second: {}", clip.frames());
    a.update_video_textures(&ctx, 0.0);
    assert!(a.tex.contains_key(&poster), "the layer shows a frame");
    let first = a.clips[&vid].last.as_ref().unwrap().0;
    a.update_video_textures(&ctx, 0.5);
    let mid = a.clips[&vid].last.as_ref().unwrap().0;
    assert_ne!(first, mid, "another time, another frame");
    a.update_video_textures(&ctx, 1.0);
    assert_eq!(a.clips[&vid].last.as_ref().unwrap().0, first, "a one second video starts over after a second");
    a.stop_video_playback();
    assert!(a.clips.is_empty() && !a.tex.contains_key(&poster), "the poster shows again and the frames are gone");
    let _ = std::fs::remove_dir_all(dir);
    // without ffmpeg there is nothing to attach
}

// structure of a real .fig file (set FIG_FILE): node types and the fields they carry. Prints counts only, no content.
#[test]
#[ignore]
fn probe_fig_structure() {
    let Ok(path) = std::env::var("FIG_FILE") else { return };
    let f = crate::figfile::read(&std::fs::read(path).unwrap()).unwrap();
    eprintln!("version {} images {}", f.version, f.images.len());
    let nodes = f.message.arr("nodeChanges");
    eprintln!("nodeChanges {} blobs {}", nodes.len(), f.message.arr("blobs").len());
    let mut by: std::collections::BTreeMap<String, (usize, std::collections::BTreeSet<String>)> = Default::default();
    for n in nodes {
        let t = n.s("type").unwrap_or("?").to_owned();
        let e = by.entry(t).or_default();
        e.0 += 1;
        if let crate::kiwi::Val::Obj(m) = n {
            e.1.extend(m.keys().cloned());
        }
    }
    for (t, (n, keys)) in &by {
        eprintln!("{t}: {n}  fields: {}", keys.iter().cloned().collect::<Vec<_>>().join(","));
    }
}

fn val_json(v: &crate::kiwi::Val, depth: usize) -> String {
    use crate::kiwi::Val;
    match v {
        Val::Bool(b) => b.to_string(),
        Val::Int(i) => i.to_string(),
        Val::Float(f) => format!("{:.3}", f),
        Val::Str(s) => format!("{:?}", s.chars().take(24).collect::<String>()),
        Val::Enum(s) => s.clone(),
        Val::Bytes(b) => format!("<{} bytes>", b.len()),
        Val::Arr(a) if depth > 3 => format!("[{} items]", a.len()),
        Val::Arr(a) => format!("[{}]", a.iter().take(3).map(|x| val_json(x, depth + 1)).collect::<Vec<_>>().join(", ")),
        Val::Obj(_) if depth > 3 => "{..}".into(),
        Val::Obj(m) => format!("{{{}}}", m.iter().map(|(k, x)| format!("{k}: {}", val_json(x, depth + 1))).collect::<Vec<_>>().join(", ")),
    }
}

#[test]
#[ignore]
fn probe_fig_values() {
    let Ok(path) = std::env::var("FIG_FILE") else { return };
    let f = crate::figfile::read(&std::fs::read(path).unwrap()).unwrap();
    let want = std::env::var("FIG_KEYS").unwrap_or_default();
    let want: Vec<&str> = want.split(',').filter(|s| !s.is_empty()).collect();
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    for n in f.message.arr("nodeChanges") {
        let t = n.s("type").unwrap_or("?").to_owned();
        let c = seen.entry(t.clone()).or_default();
        *c += 1;
        if *c > 1 || !["FRAME", "TEXT", "VECTOR", "ROUNDED_RECTANGLE", "INSTANCE", "SYMBOL", "CANVAS", "LINE", "ELLIPSE", "BOOLEAN_OPERATION"].contains(&t.as_str()) {
            continue;
        }
        if let crate::kiwi::Val::Obj(m) = n {
            let parts: Vec<String> = m.iter().filter(|(k, _)| want.is_empty() || want.contains(&k.as_str())).map(|(k, v)| format!("{k}: {}", val_json(v, 1))).collect();
            eprintln!("## {t}\n{}\n", parts.join("\n"));
        }
    }
}

#[test]
#[ignore]
fn probe_fig_blobs() {
    let Ok(path) = std::env::var("FIG_FILE") else { return };
    let f = crate::figfile::read(&std::fs::read(path).unwrap()).unwrap();
    for k in [0usize, 25, 40, 41, 229, 24] {
        if let Some(b) = f.message.arr("blobs").get(k).and_then(|b| b.bytes("bytes")) {
            eprintln!("blob {k}: {} bytes: {}", b.len(), b.iter().take(70).map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" "));
        }
    }
}

#[test]
#[ignore]
fn import_real_fig_file() {
    let Ok(path) = std::env::var("FIG_FILE") else { return };
    let t = std::time::Instant::now();
    let (doc, summary) = App::doc_from_fig(&std::fs::read(path).unwrap()).unwrap();
    eprintln!("{} in {:?}", summary.text(), t.elapsed());
    let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
    let (mut bad, mut orphans, mut comps, mut insts, mut empty_paths) = (0, 0, 0, 0, 0);
    for p in &doc.pages {
        let ids: std::collections::HashSet<u64> = p.shapes.iter().map(|s| s.id).collect();
        for s in &p.shapes {
            *kinds.entry(format!("{:?}", s.kind)).or_default() += 1;
            if !(s.pos.x.is_finite() && s.pos.y.is_finite() && s.size.x.is_finite() && s.size.y.is_finite() && s.rotation.is_finite()) {
                bad += 1;
                eprintln!("bad: {:?} {:?} pos {:?} size {:?} rot {} parent {:?} src {:?}", s.name, s.kind, s.pos, s.size, s.rotation, s.parent, s.src);
            }
            if s.parent.is_some_and(|q| !ids.contains(&q)) {
                orphans += 1;
            }
            comps += s.component as usize;
            insts += s.instance as usize;
            if s.kind == Kind::Path && s.path.is_empty() {
                empty_paths += 1;
            }
        }
        eprintln!("page {:?}: {} layers", p.name, p.shapes.len());
    }
    eprintln!("kinds {kinds:?}\ncomponents {comps} instances {insts} bad {bad} orphans {orphans} empty_paths {empty_paths} images {}", doc.images.len());
    assert_eq!((bad, orphans), (0, 0));
}

// renders the first top-level frames of a page of a real .fig to PNGs (FIG_FILE, FIG_PAGE, FIG_OUT)
#[test]
#[ignore]
fn render_imported_frames() {
    let (Ok(path), Ok(out)) = (std::env::var("FIG_FILE"), std::env::var("FIG_OUT")) else { return };
    let want = std::env::var("FIG_PAGE").unwrap_or_default();
    let (doc, _) = App::doc_from_fig(&std::fs::read(path).unwrap()).unwrap();
    let mut a = App::default();
    a.open_doc(doc, std::path::PathBuf::from("x.json"));
    let page = a.pages.iter().position(|p| p.name.trim() == want.trim()).unwrap_or(0);
    a.switch_page(page);
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {}); // fonts exist after the first run
    let mut roots: Vec<u64> = a.shapes.iter().filter(|s| s.parent.is_none() && s.kind == Kind::Frame && s.size.x > 200.0).map(|s| s.id).collect();
    roots.truncate(std::env::var("FIG_N").ok().and_then(|n| n.parse().ok()).unwrap_or(3));
    for (k, id) in roots.iter().enumerate() {
        let (svg, r) = a.svg_for(&ctx, &[*id]).unwrap();
        let scale = (1400.0 / r.width().max(r.height())).min(1.0);
        let (w, h) = ((r.width() * scale).ceil() as u32, (r.height() * scale).ceil() as u32);
        let img = crate::export::rasterize_scene(&svg, w.max(1), h.max(1)).unwrap();
        let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
        for p in &img.pixels {
            let [r_, g, b, al] = p.to_srgba_unmultiplied();
            // over white, so a transparent background is readable
            let f = |c: u8| ((c as u32 * al as u32 + 255 * (255 - al as u32)) / 255) as u8;
            rgba.extend([f(r_), f(g), f(b), 255]);
        }
        image::RgbaImage::from_raw(w, h, rgba).unwrap().save(format!("{out}/frame{k}.png")).unwrap();
        eprintln!("frame{k}: {:?} {}x{} ({} layers)", a.shapes[a.index_of(*id).unwrap()].name, r.width(), r.height(), a.shapes.iter().filter(|s| a.is_inside(s.id, *id)).count());
    }
}

#[test]
#[ignore]
fn probe_imported_point() {
    let Ok(path) = std::env::var("FIG_FILE") else { return };
    let (doc, _) = App::doc_from_fig(&std::fs::read(path).unwrap()).unwrap();
    let mut a = App::default();
    a.open_doc(doc, std::path::PathBuf::from("x.json"));
    let page = a.pages.iter().position(|p| p.name.trim() == std::env::var("FIG_PAGE").unwrap_or_default().trim()).unwrap_or(0);
    a.switch_page(page);
    let root = a.shapes.iter().find(|s| s.parent.is_none() && s.kind == Kind::Frame && s.size.x > 200.0).unwrap().id;
    let ri = a.index_of(root).unwrap();
    let o = a.shapes[ri].pos;
    let (px, py): (f32, f32) = (std::env::var("FIG_X").unwrap().parse().unwrap(), std::env::var("FIG_Y").unwrap().parse().unwrap());
    let p = Pos2::new(o.x + px, o.y + py);
    for s in &a.shapes {
        if !a.is_inside(s.id, root) {
            continue;
        }
        let i = a.index_of(s.id).unwrap();
        let r = a.world_rect(i);
        if r.contains(p) && r.width() < 60.0 {
            eprintln!("{:?} {:?} size {:?} fills {} {:?} strokes {} w {} path {} src {:?} comp {} inst {} parent {:?}", s.name, s.kind, s.size, s.fills.len(), s.fills.first().map(|f| format!("{:?}", f.kind).chars().take(40).collect::<String>()), s.strokes.len(), s.stroke_w, s.path.len(), s.src, s.component, s.instance, s.parent);
        }
    }
}

// builds a small .fig (a ZIP holding canvas.fig: "fig-kiwi", a version, a deflated Kiwi schema and a deflated message) with the
// field names Figma's own schema uses, so the importer is tested without a real Figma file
fn tiny_fig() -> Vec<u8> {
    use crate::kiwi::writer::*;
    // definitions: (name, kind 0 enum / 1 struct / 2 message, [(field, type, array, value)]); native types: bool !0, byte !1,
    // int !2, uint !3, float !4, string !5
    type Def = (&'static str, u8, Vec<(&'static str, i32, bool, u32)>);
    let floats = |names: &[&'static str]| -> Vec<(&'static str, i32, bool, u32)> { names.iter().enumerate().map(|(i, n)| (*n, !4, false, i as u32 + 1)).collect() };
    let defs: Vec<Def> = vec![
        ("NodeType", 0, [("DOCUMENT", 1), ("CANVAS", 2), ("FRAME", 3), ("ROUNDED_RECTANGLE", 4), ("TEXT", 5), ("SYMBOL", 6), ("INSTANCE", 7)].iter().map(|(a, v)| (*a, 0, false, *v)).collect()), // 0
        ("PaintType", 0, vec![("SOLID", 0, false, 1)]),                                                                                                                                    // 1
        ("GUID", 1, vec![("sessionID", !3, false, 1), ("localID", !3, false, 2)]),                                                                                                          // 2
        ("ParentIndex", 1, vec![("guid", 2, false, 1), ("position", !5, false, 2)]),                                                                                                        // 3
        ("Vector", 1, floats(&["x", "y"])),                                                                                                                                                 // 4
        ("Matrix", 1, floats(&["m00", "m01", "m02", "m10", "m11", "m12"])),                                                                                                                 // 5
        ("Color", 1, floats(&["r", "g", "b", "a"])),                                                                                                                                        // 6
        ("Paint", 2, vec![("type", 1, false, 1), ("color", 6, false, 2), ("opacity", !4, false, 3), ("visible", !0, false, 4)]),                                                             // 7
        ("TextData", 2, vec![("characters", !5, false, 1)]),                                                                                                                                // 8
        ("SymbolData", 2, vec![("symbolID", 2, false, 1)]),                                                                                                                                 // 9
        ("NodeChange", 2, vec![("guid", 2, false, 1), ("parentIndex", 3, false, 2), ("type", 0, false, 3), ("name", !5, false, 4), ("size", 4, false, 5), ("transform", 5, false, 6), ("fillPaints", 7, true, 7), ("cornerRadius", !4, false, 8), ("strokeWeight", !4, false, 9), ("textData", 8, false, 10), ("fontSize", !4, false, 11), ("symbolData", 9, false, 12), ("frameMaskDisabled", !0, false, 13)]), // 10
        ("Message", 2, vec![("nodeChanges", 10, true, 1)]),                                                                                                                              // 11
    ];
    let mut schema = vec![];
    var_u(defs.len() as u64, &mut schema);
    for (name, kind, fields) in &defs {
        string(name, &mut schema);
        schema.push(*kind);
        var_u(fields.len() as u64, &mut schema);
        for (f, ty, arr, val) in fields {
            string(f, &mut schema);
            var_i(*ty as i64, &mut schema);
            schema.push(*arr as u8);
            var_u(*val as u64, &mut schema);
        }
    }
    let guid = |s: u64, l: u64| {
        let mut b = vec![];
        var_u(s, &mut b);
        var_u(l, &mut b);
        b
    };
    let matrix = |x: f32, y: f32| {
        let mut b = vec![];
        for v in [1.0, 0.0, x, 0.0, 1.0, y] {
            var_f(v, &mut b);
        }
        b
    };
    let paint = |r: f32, g: f32, b_: f32| {
        let mut b = vec![];
        var_u(1, &mut b); // type
        var_u(1, &mut b); // SOLID
        var_u(2, &mut b); // color
        for v in [r, g, b_, 1.0] {
            var_f(v, &mut b);
        }
        var_u(4, &mut b); // visible
        b.push(1);
        var_u(0, &mut b);
        b
    };
    // one node message: guid, parent and position, type, name, size, place, an optional fill, and any extra fields
    let node = |g: (u64, u64), parent: (u64, u64), position: &str, ty: u64, name: &str, size: (f32, f32), at: (f32, f32), fill: Option<(f32, f32, f32)>, extra: &dyn Fn(&mut Vec<u8>)| {
        let mut b = vec![];
        var_u(1, &mut b);
        b.extend(guid(g.0, g.1));
        var_u(2, &mut b);
        b.extend(guid(parent.0, parent.1));
        string(position, &mut b);
        var_u(3, &mut b);
        var_u(ty, &mut b);
        var_u(4, &mut b);
        string(name, &mut b);
        var_u(5, &mut b);
        var_f(size.0, &mut b);
        var_f(size.1, &mut b);
        var_u(6, &mut b);
        b.extend(matrix(at.0, at.1));
        if let Some((r, g_, b_)) = fill {
            var_u(7, &mut b);
            var_u(1, &mut b);
            b.extend(paint(r, g_, b_));
        }
        extra(&mut b);
        var_u(0, &mut b);
        b
    };
    let none: &dyn Fn(&mut Vec<u8>) = &|_| {};
    let radius: &dyn Fn(&mut Vec<u8>) = &|b| {
        var_u(8, b);
        var_f(8.0, b);
    };
    let text: &dyn Fn(&mut Vec<u8>) = &|b| {
        var_u(10, b);
        var_u(1, b);
        string("Hello", b);
        var_u(0, b);
        var_u(11, b);
        var_f(18.0, b);
    };
    let instance_of: &dyn Fn(&mut Vec<u8>) = &|b| {
        // symbolData { symbolID = (0, 20) }
        var_u(12, b);
        var_u(1, b);
        var_u(0, b);
        var_u(20, b);
        var_u(0, b);
    };
    let nodes: Vec<Vec<u8>> = vec![
        node((0, 0), (0, 0), "!", 1, "Document", (0.0, 0.0), (0.0, 0.0), None, none),
        node((0, 1), (0, 0), "!", 2, "Page 1", (0.0, 0.0), (0.0, 0.0), None, none),
        node((0, 10), (0, 1), "!", 3, "Card", (200.0, 100.0), (10.0, 20.0), Some((1.0, 0.0, 0.0)), radius),
        node((0, 11), (0, 10), "!", 4, "Box", (50.0, 30.0), (5.0, 6.0), Some((0.0, 1.0, 0.0)), none),
        node((0, 12), (0, 10), "\"", 5, "Label", (80.0, 20.0), (60.0, 40.0), Some((0.0, 0.0, 1.0)), text),
        // a component and an instance of it, on the same page
        node((0, 20), (0, 1), "\"", 6, "Btn", (80.0, 24.0), (300.0, 20.0), Some((0.5, 0.5, 0.5)), none),
        node((0, 21), (0, 20), "!", 4, "Face", (80.0, 24.0), (0.0, 0.0), Some((0.2, 0.4, 0.6)), none),
        node((0, 30), (0, 1), "#", 7, "Btn copy", (80.0, 24.0), (300.0, 80.0), None, instance_of),
    ];
    let mut data = vec![];
    var_u(1, &mut data);
    var_u(nodes.len() as u64, &mut data);
    for nd in &nodes {
        data.extend_from_slice(nd);
    }
    var_u(0, &mut data);
    let mut canvas = b"fig-kiwi".to_vec();
    canvas.extend_from_slice(&15u32.to_le_bytes());
    for chunk in [&schema, &data] {
        let z = miniz_oxide::deflate::compress_to_vec(chunk, 6);
        canvas.extend_from_slice(&(z.len() as u32).to_le_bytes());
        canvas.extend_from_slice(&z);
    }
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(vec![]));
    zip.start_file("canvas.fig", zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
    std::io::Write::write_all(&mut zip, &canvas).unwrap();
    zip.finish().unwrap().into_inner()
}

#[test]
fn a_fig_file_imports_pages_layers_fills_text_and_linked_instances() {
    let (doc, summary) = App::doc_from_fig(&tiny_fig()).expect("the file reads");
    assert_eq!((doc.pages.len(), doc.pages[0].name.as_str()), (1, "Page 1"));
    let shapes = &doc.pages[0].shapes;
    let by = |name: &str| shapes.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no layer {name}: {:?}", shapes.iter().map(|s| &s.name).collect::<Vec<_>>()));
    let card = by("Card");
    assert_eq!((card.kind, card.pos, card.size, card.radius[0], card.parent), (Kind::Frame, Pos2::new(10.0, 20.0), Vec2::new(200.0, 100.0), 8.0, None));
    assert_eq!(card.fills[0].kind, PaintKind::Solid(egui::Color32::from_rgb(255, 0, 0)));
    // children sit in their parent, in the order of their position strings
    let (bx, label) = (by("Box"), by("Label"));
    assert_eq!((bx.parent, bx.kind, bx.pos), (Some(card.id), Kind::Rect, Pos2::new(5.0, 6.0)));
    assert_eq!((label.kind, label.text.as_str(), label.font_size), (Kind::Text, "Hello", 18.0));
    let order: Vec<&str> = shapes.iter().filter(|s| s.parent == Some(card.id)).map(|s| s.name.as_str()).collect();
    assert_eq!(order, ["Box", "Label"], "back to front");
    // the component is a component; the instance is linked to it and holds its own copy of what is inside
    let (btn, inst) = (by("Btn"), shapes.iter().find(|s| s.instance).expect("an instance"));
    assert!(btn.component && !btn.instance);
    assert_eq!((inst.pos, inst.src), (Pos2::new(300.0, 80.0), Some(btn.id)));
    assert!(shapes.iter().any(|s| s.parent == Some(inst.id) && s.name == "Face" && s.src.is_some()), "the instance holds a mirror of Face");
    assert_eq!(summary.instances, 1);
    // it opens as a document, and the page draws
    let mut a = App::default();
    a.open_doc(doc, std::path::PathBuf::from("x.json"));
    assert!(a.shapes.len() >= 7);
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
    let id = a.shapes.iter().find(|s| s.name == "Card").unwrap().id;
    let svg = a.svg_for(&ctx, &[id]).unwrap().0;
    assert!(svg.contains("rgb(255,0,0)") && svg.contains("Hello"), "{svg}");
    // not a Figma file, or cut short
    assert!(App::doc_from_fig(b"hello").is_err());
    assert!(App::doc_from_fig(&tiny_fig()[..40]).is_err());
}

#[test]
#[ignore]
fn probe_fig_features() {
    let Ok(path) = std::env::var("FIG_FILE") else { return };
    let f = crate::figfile::read(&std::fs::read(path).unwrap()).unwrap();
    let (mut flips, mut protos, mut mixed, mut shown) = (0, 0, 0, 0);
    let mut paint_types: std::collections::BTreeMap<String, usize> = Default::default();
    for n in f.message.arr("nodeChanges") {
        if let Some(t) = n.get("transform") {
            let det = t.f("m00").unwrap_or(1.0) * t.f("m11").unwrap_or(1.0) - t.f("m01").unwrap_or(0.0) * t.f("m10").unwrap_or(0.0);
            if det < 0.0 {
                flips += 1;
            }
        }
        for p in n.arr("fillPaints").iter().chain(n.arr("strokePaints")) {
            *paint_types.entry(p.s("type").unwrap_or("?").to_owned()).or_default() += 1;
        }
        if !n.arr("prototypeInteractions").is_empty() {
            protos += 1;
            if shown < 3 {
                shown += 1;
                eprintln!("PROTO {}", val_json(&n.arr("prototypeInteractions")[0], 0));
            }
        }
        if let Some(td) = n.get("textData") {
            if !td.arr("characterStyleIDs").is_empty() && td.arr("characterStyleIDs").iter().any(|v| *v != td.arr("characterStyleIDs")[0]) {
                mixed += 1;
                if mixed <= 2 {
                    eprintln!("MIXED text keys: {:?}", if let crate::kiwi::Val::Obj(m) = td { m.keys().collect::<Vec<_>>() } else { vec![] });
                    eprintln!("MIXED table {}", val_json(&crate::kiwi::Val::Arr(td.arr("styleOverrideTable").to_vec()), 0));
                    eprintln!("MIXED ids {}", val_json(&crate::kiwi::Val::Arr(td.arr("characterStyleIDs").iter().take(30).cloned().collect()), 0));
                }
            }
        }
    }
    eprintln!("flipped {flips} nodes with interactions {protos} mixed-style texts {mixed} paint types {paint_types:?}");
}

// hand-made kiwi values for the importer's pieces
fn kv(pairs: Vec<(&str, crate::kiwi::Val)>) -> crate::kiwi::Val {
    crate::kiwi::Val::Obj(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn kf(v: f64) -> crate::kiwi::Val {
    crate::kiwi::Val::Float(v)
}

fn kn(v: i64) -> crate::kiwi::Val {
    crate::kiwi::Val::Int(v)
}

fn ke(v: &str) -> crate::kiwi::Val {
    crate::kiwi::Val::Enum(v.to_owned())
}

fn matrix(m: [f64; 6]) -> crate::kiwi::Val {
    kv(vec![("m00", kf(m[0])), ("m01", kf(m[1])), ("m02", kf(m[2])), ("m10", kf(m[3])), ("m11", kf(m[4])), ("m12", kf(m[5]))])
}

#[test]
fn a_mirrored_matrix_becomes_a_turn_and_a_mirror_of_the_contents() {
    let mut a = App::default();
    a.add(Kind::Path, Pos2::ZERO);
    let id = a.shapes.last().unwrap().id;
    let i = a.index_of(id).unwrap();
    a.shapes[i].size = Vec2::new(100.0, 50.0);
    a.shapes[i].path = vec![Contour { pts: vec![PathPt::corner(Pos2::new(0.2, 0.4)), PathPt::corner(Pos2::new(0.9, 0.7))], closed: false, hole: false }];
    // x -> 120 - x, y -> y + 30: a horizontal mirror, the box's middle lands at (70, 55)
    let mut s = a.shapes[i].clone();
    let flipped = crate::figimport::place_for_test(&mut s, &matrix([-1.0, 0.0, 120.0, 0.0, 1.0, 30.0]));
    assert!(flipped);
    assert_eq!((s.pos, s.rotation), (Pos2::new(20.0, 30.0), 0.0));
    a.shapes[i] = s;
    a.flip_inside(id, true);
    let s = &a.shapes[i];
    assert!((s.path[0].pts[0].p.x - 0.8).abs() < 1e-6 && (s.path[0].pts[1].p.x - 0.1).abs() < 1e-6, "the points are mirrored about the middle");
    assert_eq!(s.path[0].pts[0].p.y, 0.4, "and only in x");
    assert!(s.rotation.abs() < 1e-3);
    // a vertical mirror is a mirror and a half turn
    let mut v = a.shapes[i].clone();
    assert!(crate::figimport::place_for_test(&mut v, &matrix([1.0, 0.0, 0.0, 0.0, -1.0, 50.0])));
    assert!((v.rotation.abs() - 180.0).abs() < 1e-3, "{}", v.rotation);
    // an ordinary turn is not a mirror
    let mut r = a.shapes[i].clone();
    assert!(!crate::figimport::place_for_test(&mut r, &matrix([0.0, -1.0, 10.0, 1.0, 0.0, 10.0])));
    assert!((r.rotation - 90.0).abs() < 1e-3);
}

#[test]
fn mixed_text_styles_become_runs_that_keep_only_what_differs() {
    let file = crate::figfile::FigFile { message: kv(vec![]), images: Default::default(), version: 0 };
    let mut imp = crate::figimport::importer_for_test(&file);
    let mut s = Shape::new(Kind::Text, 1, 1, Pos2::ZERO, None);
    s.text = "hello world".into();
    s.font_size = 16.0;
    let td = kv(vec![
        ("characters", crate::kiwi::Val::Str("hello world".into())),
        // "hello " is the layer's own style (0); "wor" is style 7 (bigger, underlined, a link); "ld" is style 8 (italic)
        ("characterStyleIDs", crate::kiwi::Val::Arr([0, 0, 0, 0, 0, 0, 7, 7, 7, 8, 8].iter().map(|&i| kn(i)).collect())),
        (
            "styleOverrideTable",
            crate::kiwi::Val::Arr(vec![
                kv(vec![("styleID", kn(7)), ("fontSize", kf(30.0)), ("textDecoration", ke("UNDERLINE")), ("hyperlink", kv(vec![("url", crate::kiwi::Val::Str("https://example.com".into()))]))]),
                kv(vec![("styleID", kn(8)), ("fontName", kv(vec![("family", crate::kiwi::Val::Str("Inter".into())), ("style", crate::kiwi::Val::Str("Italic".into()))]))]),
            ]),
        ),
    ]);
    let runs = crate::figimport::text_runs_for_test(&mut imp, &s, &td);
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!((runs[0].start, runs[0].end, runs[0].size, runs[0].underline, runs[0].url.as_deref()), (6, 9, Some(30.0), Some(true), Some("https://example.com")));
    assert_eq!((runs[1].start, runs[1].end, runs[1].italic, runs[1].size), (9, 11, Some(true), None));
}

#[test]
fn prototype_interactions_map_triggers_actions_transitions_and_targets() {
    let action = |nav: &str, conn: &str, tr: &str| {
        kv(vec![("connectionType", ke(conn)), ("navigationType", ke(nav)), ("transitionType", ke(tr)), ("transitionDuration", kf(0.4)), ("easingType", ke("OUT_CUBIC")), ("transitionNodeID", kv(vec![("sessionID", kn(0)), ("localID", kn(9))]))])
    };
    let node = kv(vec![(
        "prototypeInteractions",
        crate::kiwi::Val::Arr(vec![
            kv(vec![("event", kv(vec![("interactionType", ke("ON_CLICK"))])), ("actions", crate::kiwi::Val::Arr(vec![action("NAVIGATE", "INTERNAL_NODE", "DISSOLVE")]))]),
            kv(vec![("event", kv(vec![("interactionType", ke("ON_HOVER"))])), ("actions", crate::kiwi::Val::Arr(vec![action("SWAP_STATE", "INTERNAL_NODE", "DISSOLVE")]))]),
            kv(vec![("event", kv(vec![("interactionType", ke("AFTER_TIMEOUT")), ("transitionTimeout", kf(2.5))])), ("actions", crate::kiwi::Val::Arr(vec![action("OVERLAY", "INTERNAL_NODE", "MOVE_IN")]))]),
            kv(vec![("event", kv(vec![("interactionType", ke("ON_CLICK"))])), ("actions", crate::kiwi::Val::Arr(vec![action("NAVIGATE", "BACK", "INSTANT")]))]),
            kv(vec![("event", kv(vec![("interactionType", ke("ON_CLICK"))])), ("actions", crate::kiwi::Val::Arr(vec![action("NAVIGATE", "NONE", "INSTANT")]))]),
            kv(vec![("isDeleted", crate::kiwi::Val::Bool(true)), ("event", kv(vec![("interactionType", ke("ON_CLICK"))])), ("actions", crate::kiwi::Val::Arr(vec![action("NAVIGATE", "INTERNAL_NODE", "INSTANT")]))]),
        ]),
    )]);
    let ixs = crate::figimport::interactions_for_test(&node);
    assert_eq!(ixs.len(), 4, "the none-connection and the deleted one are left out");
    assert_eq!((ixs[0].0.trigger, ixs[0].0.action, ixs[0].0.transition, ixs[0].0.duration, ixs[0].1), (Trigger::Click, Action::Navigate, Transition::Dissolve, 0.4, Some((0, 9))));
    assert_eq!((ixs[1].0.trigger, ixs[1].0.action), (Trigger::WhileHover, Action::ChangeTo));
    assert_eq!((ixs[2].0.trigger, ixs[2].0.action, ixs[2].0.delay, ixs[2].0.transition), (Trigger::AfterDelay, Action::OpenOverlay, 2.5, Transition::SlideLeft));
    assert_eq!(ixs[3].0.action, Action::Back);
}

#[test]
fn variables_import_with_their_modes_aliases_and_types() {
    use crate::kiwi::Val;
    let g = |s: i64, l: i64| kv(vec![("sessionID", kn(s)), ("localID", kn(l))]);
    let wrap = |s: i64, l: i64| kv(vec![("guid", g(s, l))]);
    let col = |r: f64| kv(vec![("r", kf(r)), ("g", kf(0.0)), ("b", kf(0.0)), ("a", kf(1.0))]);
    let entry = |mode: i64, value: Val| kv(vec![("modeID", g(0, mode)), ("variableData", kv(vec![("value", value)]))]);
    let set = kv(vec![
        ("guid", g(0, 50)),
        ("type", ke("VARIABLE_SET")),
        ("name", Val::Str("Colors".into())),
        ("variableSetModes", Val::Arr(vec![kv(vec![("id", g(0, 60)), ("name", Val::Str("Light".into()))]), kv(vec![("id", g(0, 61)), ("name", Val::Str("Dark".into()))])])),
    ]);
    let variable = |local: i64, name: &str, ty: &str, entries: Vec<Val>| {
        kv(vec![("guid", g(0, local)), ("type", ke("VARIABLE")), ("name", Val::Str(name.into())), ("variableResolvedType", ke(ty)), ("variableSetID", wrap(0, 50)), ("variableDataValues", kv(vec![("entries", Val::Arr(entries))]))])
    };
    let nodes = vec![
        set,
        variable(51, "brand", "COLOR", vec![entry(60, kv(vec![("colorValue", col(1.0))])), entry(61, kv(vec![("colorValue", col(0.5))]))]),
        // an alias of the first, a number and a boolean
        variable(52, "link", "COLOR", vec![entry(60, kv(vec![("alias", wrap(0, 51))])), entry(61, kv(vec![("alias", wrap(0, 51))]))]),
        variable(53, "radius", "FLOAT", vec![entry(60, kv(vec![("floatValue", kf(12.0))]))]),
        variable(54, "dense", "BOOLEAN", vec![entry(60, kv(vec![("boolValue", Val::Bool(true))]))]),
        // a kind drui has no variable for is skipped
        variable(55, "odd", "MYSTERY", vec![]),
    ];
    let file = crate::figfile::FigFile { message: kv(vec![("nodeChanges", Val::Arr(nodes))]), images: Default::default(), version: 0 };
    let mut app = App::default();
    let (cols, n) = crate::figimport::variables_for_test(&file, &mut app);
    assert_eq!((cols.len(), n), (1, 4));
    let c = &cols[0];
    assert_eq!((c.name.as_str(), c.modes.iter().map(|m| m.name.as_str()).collect::<Vec<_>>()), ("Colors", vec!["Light", "Dark"]));
    let by = |name: &str| c.vars.iter().find(|v| v.name == name).unwrap_or_else(|| panic!("no variable {name}"));
    let brand = by("brand");
    assert_eq!((brand.kind, brand.values[0].clone(), brand.values[1].clone()), (VarKind::Color, VarValue::Color(egui::Color32::from_rgb(255, 0, 0)), VarValue::Color(egui::Color32::from_rgb(128, 0, 0))));
    assert_eq!(by("link").values[0], VarValue::Alias(brand.id), "an alias points at the other variable");
    assert_eq!((by("radius").kind, by("radius").values[0].clone(), by("dense").values[0].clone()), (VarKind::Number, VarValue::Number(12.0), VarValue::Bool(true)));
    assert_eq!(by("radius").values.len(), 2, "one value per mode, the missing one filled in");
}

#[test]
#[ignore]
fn probe_fig_actions() {
    let Ok(path) = std::env::var("FIG_FILE") else { return };
    let f = crate::figfile::read(&std::fs::read(path).unwrap()).unwrap();
    let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
    let mut action_keys: std::collections::BTreeSet<String> = Default::default();
    let mut overlay: std::collections::BTreeMap<String, usize> = Default::default();
    let mut shown = 0;
    for n in f.message.arr("nodeChanges") {
        for k in ["overlayPositionType", "overlayBackgroundInteraction", "overlayBackgroundAppearance", "overlayRelativePosition"] {
            if let Some(v) = n.get(k) {
                *overlay.entry(format!("{k}={}", val_json(v, 2))).or_default() += 1;
            }
        }
        for pi in n.arr("prototypeInteractions") {
            if let crate::kiwi::Val::Obj(m) = pi {
                for k in m.keys() { action_keys.insert(format!("interaction.{k}")); }
            }
            if let Some(e) = pi.get("event") {
                if let crate::kiwi::Val::Obj(m) = e { for k in m.keys() { action_keys.insert(format!("event.{k}")); } }
            }
            for a in pi.arr("actions") {
                if let crate::kiwi::Val::Obj(m) = a { for k in m.keys() { action_keys.insert(format!("action.{k}")); } }
                *kinds.entry(format!("{} / {} / {}", a.s("connectionType").unwrap_or("?"), a.s("navigationType").unwrap_or("?"), a.s("transitionType").unwrap_or("?"))).or_default() += 1;
                if shown < 2 && a.s("connectionType") != Some("INTERNAL_NODE") { shown += 1; eprintln!("ODD {}", val_json(a, 0)); }
            }
        }
    }
    eprintln!("action kinds {kinds:?}\nkeys {action_keys:?}\noverlay {overlay:?}");
}

#[test]
#[ignore]
fn probe_fig_overlay() {
    let Ok(path) = std::env::var("FIG_FILE") else { return };
    let f = crate::figfile::read(&std::fs::read(path).unwrap()).unwrap();
    let nodes = f.message.arr("nodeChanges");
    let guid = |v: &crate::kiwi::Val| (v.i("sessionID").unwrap_or(-1), v.i("localID").unwrap_or(-1));
    for n in nodes {
        for pi in n.arr("prototypeInteractions") {
            for a in pi.arr("actions") {
                if a.s("navigationType") == Some("OVERLAY") {
                    eprintln!("ACTION {}", val_json(a, 0));
                    eprintln!("SOURCE {} size {} transform {}", n.s("name").unwrap_or("?"), val_json(n.get("size").unwrap(), 0), val_json(n.get("transform").unwrap(), 0));
                    let t = a.get("transitionNodeID").map(guid).unwrap();
                    if let Some(target) = nodes.iter().find(|m| m.get("guid").map(guid) == Some(t)) {
                        eprintln!("TARGET {} type {} size {} overlayPos {:?} bgInteraction {:?} bgAppearance {:?}", target.s("name").unwrap_or("?"), target.s("type").unwrap_or("?"), val_json(target.get("size").unwrap(), 0), target.s("overlayPositionType"), target.s("overlayBackgroundInteraction"), target.get("overlayBackgroundAppearance").map(|v| val_json(v, 0)));
                    }
                }
            }
        }
    }
}

#[test]
fn overlays_variable_actions_conditions_patterns_and_videos_import() {
    use crate::kiwi::Val;
    let g = |s: i64, l: i64| kv(vec![("sessionID", kn(s)), ("localID", kn(l))]);
    let wrap = |l: i64| kv(vec![("guid", g(0, l))]);
    let arr = |v: Vec<Val>| Val::Arr(v);
    let node = |l: i64, parent: i64, pos: &str, ty: &str, name: &str, size: (f64, f64), at: (f64, f64), extra: Vec<(&str, Val)>| {
        let mut f = vec![
            ("guid", g(0, l)),
            ("parentIndex", kv(vec![("guid", g(0, parent)), ("position", Val::Str(pos.into()))])),
            ("type", ke(ty)),
            ("name", Val::Str(name.into())),
            ("size", kv(vec![("x", kf(size.0)), ("y", kf(size.1))])),
            ("transform", matrix([1.0, 0.0, at.0, 0.0, 1.0, at.1])),
        ];
        f.extend(extra);
        kv(f)
    };
    let alias = |l: i64| kv(vec![("value", kv(vec![("alias", wrap(l))]))]);
    let num = |x: f64| kv(vec![("value", kv(vec![("floatValue", kf(x))]))]);
    let expr = |f: &str, args: Vec<Val>| kv(vec![("value", kv(vec![("expressionValue", kv(vec![("expressionFunction", ke(f)), ("expressionArguments", arr(args))]))]))]);
    let click = || kv(vec![("interactionType", ke("ON_CLICK"))]);
    let action = |conn: &str, nav: &str, extra: Vec<(&str, Val)>| {
        let mut f = vec![("connectionType", ke(conn)), ("navigationType", ke(nav)), ("transitionType", ke("INSTANT_TRANSITION")), ("transitionNodeID", g(0, 20))];
        f.extend(extra);
        kv(f)
    };
    let interactions = arr(vec![
        // opens the menu as an overlay, 50px under the button
        kv(vec![("event", click()), ("actions", arr(vec![action("INTERNAL_NODE", "OVERLAY", vec![("overlayRelativePosition", kv(vec![("x", kf(0.0)), ("y", kf(50.0))]))])]))]),
        // count = count + 1
        kv(vec![("event", click()), ("actions", arr(vec![kv(vec![("connectionType", ke("SET_VARIABLE")), ("targetVariableID", wrap(70)), ("targetVariableValue", expr("ADDITION", vec![alias(70), num(1.0)]))])]))]),
        // if count < 5 go to the menu, else go back
        kv(vec![
            ("event", click()),
            (
                "actions",
                arr(vec![kv(vec![
                    ("connectionType", ke("CONDITIONAL")),
                    ("conditionalActions", arr(vec![
                        kv(vec![("condition", expr("LESS_THAN", vec![alias(70), num(5.0)])), ("actions", arr(vec![action("INTERNAL_NODE", "NAVIGATE", vec![])]))]),
                        kv(vec![("actions", arr(vec![action("BACK", "NAVIGATE", vec![])]))]),
                    ])),
                ])]),
            ),
        ]),
    ]);
    let pattern = kv(vec![("type", ke("PATTERN")), ("patternSourceNodeID", g(0, 12)), ("patternSpacing", kv(vec![("x", kf(15.0)), ("y", kf(15.0))])), ("patternScalingFactor", kf(2.0)), ("visible", Val::Bool(true)), ("opacity", kf(1.0))]);
    let video = kv(vec![("type", ke("VIDEO")), ("video", kv(vec![("hash", Val::Str("aaaa".into()))])), ("image", kv(vec![("hash", Val::Str("bbbb".into()))])), ("visible", Val::Bool(true)), ("opacity", kf(1.0))]);
    let nodes = vec![
        node(0, 0, "!", "DOCUMENT", "Document", (0.0, 0.0), (0.0, 0.0), vec![]),
        node(1, 0, "!", "CANVAS", "Page", (0.0, 0.0), (0.0, 0.0), vec![]),
        node(10, 1, "!", "FRAME", "Screen", (400.0, 800.0), (0.0, 0.0), vec![]),
        node(11, 10, "!", "ROUNDED_RECTANGLE", "Button", (80.0, 40.0), (20.0, 100.0), vec![("prototypeInteractions", interactions)]),
        node(12, 10, "\"", "ROUNDED_RECTANGLE", "Tile", (30.0, 30.0), (10.0, 10.0), vec![]),
        node(13, 10, "#", "ROUNDED_RECTANGLE", "Patterned", (200.0, 100.0), (150.0, 10.0), vec![("fillPaints", arr(vec![pattern]))]),
        node(14, 10, "$", "ROUNDED_RECTANGLE", "Clip", (160.0, 90.0), (150.0, 200.0), vec![("fillPaints", arr(vec![video]))]),
        node(20, 1, "\"", "FRAME", "Menu", (120.0, 200.0), (500.0, 0.0), vec![("overlayPositionType", ke("MANUAL")), ("overlayBackgroundInteraction", ke("CLOSE_ON_CLICK_OUTSIDE"))]),
        node(50, 0, "\"", "VARIABLE_SET", "Game", (0.0, 0.0), (0.0, 0.0), vec![("variableSetModes", arr(vec![kv(vec![("id", g(0, 60)), ("name", Val::Str("Mode 1".into()))])]))]),
        node(70, 50, "!", "VARIABLE", "count", (0.0, 0.0), (0.0, 0.0), vec![
            ("variableResolvedType", ke("FLOAT")),
            ("variableSetID", wrap(50)),
            ("variableDataValues", kv(vec![("entries", arr(vec![kv(vec![("modeID", g(0, 60)), ("variableData", num(0.0))])]))])),
        ]),
    ];
    // a one-pixel PNG for the poster, and some bytes standing for the video
    let mut png = std::io::Cursor::new(vec![]);
    image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let mut images: std::collections::HashMap<String, Vec<u8>> = Default::default();
    images.insert("aaaa".into(), b"not really a video".to_vec());
    images.insert("bbbb".into(), png.into_inner());
    let file = crate::figfile::FigFile { message: kv(vec![("nodeChanges", Val::Arr(nodes))]), images, version: 0 };
    let (doc, summary) = App::doc_from_file(&file).expect("imports");
    let shapes = &doc.pages[0].shapes;
    let by = |name: &str| shapes.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no {name}"));
    let (button, menu, tile) = (by("Button"), by("Menu"), by("Tile"));
    let count = doc.variables[0].vars[0].id;
    let all = button.interactions();
    assert_eq!(all.len(), 4, "{all:#?}");
    // the overlay: the menu's own settings, and a manual position measured from the presented frame (button at 20,100 plus 0,50)
    let overlay = &all[0];
    assert_eq!((overlay.action, overlay.link, overlay.overlay_pos, overlay.close_outside, overlay.overlay_dim), (Action::OpenOverlay, Some(menu.id), 7, true, false));
    assert_eq!(overlay.overlay_off, Vec2::new(20.0, 150.0));
    // set variable: count = count + 1
    let set = &all[1];
    assert_eq!((set.action, set.var, set.value.as_str()), (Action::SetVariable, Some(count), "(`Game / count` + 1)"));
    // the if / else if: the first runs when count < 5, the other only when it does not
    assert_eq!((all[2].action, all[2].link, all[2].condition.as_str()), (Action::Navigate, Some(menu.id), "(`Game / count` < 5)"));
    assert_eq!((all[3].action, all[3].condition.as_str()), (Action::Back, "!((`Game / count` < 5))"));
    // the pattern repeats the Tile layer, spaced by its own size
    let Some(PaintKind::Pattern(pf)) = by("Patterned").fills.first().map(|p| p.kind.clone()) else { panic!("not a pattern") };
    assert_eq!((pf.source, pf.scale, pf.space), (Some(tile.id), 2.0, Vec2::new(0.5, 0.5)));
    // the video fill: an image fill with the poster, and the file kept in the document
    let clip = by("Clip");
    let (PaintKind::Image(poster), Some(vid)) = (clip.fills[0].kind.clone(), clip.fills[0].video) else { panic!("not a video fill") };
    assert!(doc.images.iter().any(|a| a.id == poster) && doc.videos.iter().any(|a| a.id == vid));
    assert_eq!((summary.patterns, summary.videos, summary.variables), (1, 1, 1));
}

#[test]
fn bulk_rename_and_find_replace() {
    use crate::model::*;
    let mut a = App::default();
    let ids: Vec<u64> = (0..3).map(|k| rect_at(&mut a, k as f32 * 60.0, 0.0)).collect();
    a.sel = ids.clone();
    a.rename_selection("Card #");
    // the top row of the layers panel (the last shape) is number 1
    let name = |a: &App, id: u64| a.shapes[a.index_of(id).unwrap()].name.clone();
    assert_eq!((name(&a, ids[2]), name(&a, ids[0])), ("Card 1".to_string(), "Card 3".to_string()));
    a.sel = vec![ids[1]];
    a.rename_selection("*!");
    assert_eq!(name(&a, ids[1]), "Card 2!");
    // find and replace touches text layers only, and honours the selection
    let t1 = rect_at(&mut a, 0.0, 100.0);
    let t2 = rect_at(&mut a, 0.0, 200.0);
    for (id, text) in [(t1, "Hello world"), (t2, "world peace")] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].kind = Kind::Text;
        a.shapes[i].text = text.into();
    }
    a.sel = vec![t1];
    a.replace_text("world", "there");
    assert_eq!(a.shapes[a.index_of(t1).unwrap()].text, "Hello there");
    assert_eq!(a.shapes[a.index_of(t2).unwrap()].text, "world peace");
    a.sel.clear();
    a.replace_text("world", "all");
    assert_eq!(a.shapes[a.index_of(t2).unwrap()].text, "all peace");
}

#[test]
fn quick_add_copies_frame_beside() {
    use crate::model::*;
    let mut a = App::default();
    let f = rect_at(&mut a, 10.0, 20.0);
    let i = a.index_of(f).unwrap();
    a.shapes[i].kind = Kind::Frame;
    a.shapes[i].size = Vec2::new(200.0, 100.0);
    a.sel = vec![f];
    a.quick_add(true);
    let c = a.index_of(a.sel[0]).unwrap();
    assert_ne!(a.sel[0], f);
    assert_eq!(a.shapes[c].pos, Pos2::new(310.0, 20.0).to_vec2().to_pos2());
    a.quick_add(false);
    let d = a.index_of(a.sel[0]).unwrap();
    assert_eq!(a.shapes[d].pos.y, a.shapes[c].pos.y + 200.0);
}

#[test]
fn handle_mirror_modes() {
    use crate::model::*;
    let mut a = App::default();
    a.pen = vec![PathPt::corner(Pos2::new(0.0, 0.0)), PathPt { p: Pos2::new(100.0, 0.0), hin: Vec2::new(-20.0, 0.0), hout: Vec2::new(30.0, 0.0), radius: 0.0 }, PathPt::corner(Pos2::new(200.0, 0.0))];
    a.finish_pen();
    let id = a.shapes.last().unwrap().id;
    a.sel = vec![id];
    for (mode, want) in [(0u8, Vec2::new(30.0, 0.0)), (1, Vec2::new(18.0, 24.0)), (2, Vec2::new(3.0, 4.0))] {
        a.handle_mirror = mode;
        let i = a.index_of(id).unwrap();
        let mut v = a.shapes[i].path_world();
        v[0].pts[1].hin = Vec2::new(-20.0, 0.0);
        v[0].pts[1].hout = Vec2::new(30.0, 0.0);
        a.shapes[i].set_path_world(&v);
        let node = a.shapes[i].path_world()[0].pts[1].p;
        a.node_move(0, 1, 1, node + Vec2::new(-3.0, -4.0));
        let out = a.shapes[a.index_of(id).unwrap()].path_world()[0].pts[1].hout;
        assert!((out - want).length() < 0.01, "mode {mode}: {out:?}");
    }
}

#[test]
fn region_tools_cut_erase_build_and_split() {
    use crate::model::*;
    use crate::vtools::VTool;
    let mut a = App::default();
    let mk = |a: &mut App, x: f32| {
        let id = rect_at(a, x, 0.0);
        let i = a.index_of(id).unwrap();
        a.shapes[i].size = Vec2::new(100.0, 100.0);
        id
    };
    let (r1, r2) = (mk(&mut a, 0.0), mk(&mut a, 60.0));
    // two overlapping squares make three regions
    a.sel = vec![r1, r2];
    a.set_vtool(Some(VTool::Builder));
    assert_eq!(a.builder.as_ref().map(|b| b.faces.len()), Some(3));
    // merge the overlap and the left piece, drop the right piece
    a.builder_touch(Pos2::new(80.0, 50.0), false);
    a.builder_touch(Pos2::new(20.0, 50.0), false);
    a.builder_touch(Pos2::new(140.0, 50.0), true);
    a.finish_builder();
    a.vtool = None;
    assert_eq!(a.sel.len(), 1, "one merged shape");
    let w = a.shapes[a.index_of(a.sel[0]).unwrap()].size;
    assert!((w.x - 100.0).abs() < 0.5, "merged region is the left square: {w:?}");
    // cut it in two along x = 50
    a.cut_along(Pos2::new(50.0, -10.0), Pos2::new(50.0, 110.0));
    assert_eq!(a.sel.len(), 2);
    let widths: Vec<f32> = a.sel.iter().map(|&id| a.shapes[a.index_of(id).unwrap()].size.x).collect();
    assert!(widths.iter().all(|w| (w - 50.0).abs() < 0.5), "{widths:?}");
    // the eraser takes a bite out of one half
    let target = a.sel[0];
    a.sel = vec![target];
    let before = a.shapes[a.index_of(target).unwrap()].size.x;
    let mid = a.shapes[a.index_of(target).unwrap()].pos;
    a.erase_along(&[Pos2::new(mid.x - 5.0, 50.0), Pos2::new(mid.x + 20.0, 50.0)]);
    assert!(a.shapes[a.index_of(target).unwrap()].size.x <= before + 0.1);
    // split vector: a path of two outlines becomes two paths
    let id = a.new_id();
    let mut p = Shape::new(Kind::Path, id, a.shapes.len() + 1, Pos2::ZERO, None);
    p.set_path_world(&[
        Contour { pts: vec![PathPt::corner(Pos2::new(0.0, 300.0)), PathPt::corner(Pos2::new(20.0, 300.0)), PathPt::corner(Pos2::new(20.0, 320.0))], closed: true, hole: false },
        Contour { pts: vec![PathPt::corner(Pos2::new(100.0, 300.0)), PathPt::corner(Pos2::new(120.0, 300.0)), PathPt::corner(Pos2::new(120.0, 320.0))], closed: true, hole: false },
    ]);
    a.shapes.push(p);
    a.sel = vec![id];
    a.split_vector();
    assert_eq!(a.sel.len(), 2);
}

#[test]
fn outline_text_makes_a_path() {
    use crate::model::*;
    let mut a = App::default();
    let id = rect_at(&mut a, 10.0, 10.0);
    let i = a.index_of(id).unwrap();
    a.shapes[i].kind = Kind::Text;
    a.shapes[i].text = "Hi".into();
    a.shapes[i].font_size = 32.0;
    a.shapes[i].size = Vec2::new(60.0, 40.0);
    a.sel = vec![id];
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
    a.ctx = Some(ctx);
    a.flatten();
    let s = &a.shapes[a.index_of(id).unwrap()];
    assert_eq!(s.kind, Kind::Path);
    assert!(s.path.len() >= 2 && s.size.x > 5.0, "{} contours, {:?}", s.path.len(), s.size);
}

#[test]
fn layer_brush_scatters_copies() {
    use crate::model::*;
    let mut a = App::default();
    let id = rect_at(&mut a, 0.0, 0.0);
    let i = a.index_of(id).unwrap();
    a.shapes[i].size = Vec2::new(10.0, 10.0);
    a.sel = vec![id];
    a.create_brush_from_selection();
    assert_eq!(a.brush, crate::vector::Brush::Layer);
    let before = a.shapes.len();
    a.brush_w = 2.0;
    a.finish_layer_brush(&[Pos2::new(100.0, 100.0), Pos2::new(200.0, 100.0)]);
    let group = a.shapes.iter().find(|s| s.kind == Kind::Group).expect("a group of copies");
    let n = a.shapes.iter().filter(|s| s.parent == Some(group.id)).count();
    assert!(n >= 8 && a.shapes.len() == before + n + 1, "{n} copies");
}

#[test]
fn repeat_copies_and_apply() {
    use crate::model::*;
    let mut a = App::default();
    let id = rect_at(&mut a, 0.0, 0.0);
    let i = a.index_of(id).unwrap();
    a.shapes[i].size = Vec2::new(10.0, 10.0);
    a.shapes[i].repeat = Some(Repeat { kind: 0, count: 3, step: Vec2::new(20.0, 5.0), ..Repeat::linear() });
    let c = a.shapes[i].repeat_copies();
    assert_eq!((c.len(), c[2].pos), (3, Pos2::new(40.0, 10.0)));
    // radial: a quarter turn round a pivot 10 px to the right of the center
    a.shapes[i].repeat = Some(Repeat { kind: 1, count: 2, angle: 90.0, pivot: Vec2::new(10.0, 0.0), spin: true, ..Repeat::linear() });
    let c = a.shapes[i].repeat_copies();
    assert!((c[1].pos - Pos2::new(10.0, -10.0)).length() < 0.01, "{:?}", c[1].pos);
    assert_eq!(c[1].rotation, 90.0);
    a.sel = vec![id];
    a.apply_repeat();
    assert_eq!(a.sel.len(), 2);
    assert!(a.shapes.iter().all(|s| s.repeat.is_none()));
}

#[test]
fn video_actions_and_drag_direction() {
    use crate::model::*;
    let mut a = App::default();
    let id = rect_at(&mut a, 0.0, 0.0);
    let i = a.index_of(id).unwrap();
    a.shapes[i].fills[0].video = Some(7);
    let ix = Interaction { trigger: Trigger::Click, action: Action::Video, link: Some(id), ..Interaction::default() };
    a.present_fire(i, &ix, 10.0);
    assert!(!a.video_ctl[&7].playing, "toggle pauses a playing video");
    let play = Interaction { opts: IxOpts { video_op: 1, ..IxOpts::default() }, ..ix.clone() };
    a.present_fire(i, &play, 12.0);
    assert!(a.video_ctl[&7].playing);
    assert_eq!(a.video_pos(7, 13.0), a.video_ctl[&7].base + 1.0, "plays on from the paused place");
    // On drag with a direction runs only for that direction
    let drag = Interaction { trigger: Trigger::Drag, action: Action::Back, link: Some(id), opts: IxOpts { drag_dir: 2, ..IxOpts::default() }, ..Interaction::default() };
    a.shapes[i].set_primary_interaction(drag);
    a.shapes[i].size = Vec2::new(50.0, 50.0);
    a.present_dir = 1;
    assert!(a.present_targets_at(Pos2::new(10.0, 10.0), Trigger::Drag).is_none());
    a.present_dir = 2;
    assert!(a.present_targets_at(Pos2::new(10.0, 10.0), Trigger::Drag).is_some());
}

#[test]
fn quick_insert_puts_shapes_at_view_centre() {
    use crate::model::*;
    let mut a = App::default();
    a.view = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
    a.open_quick();
    a.quick_query = "ell".into();
    let items = a.quick_items();
    assert_eq!(items.len(), 1);
    a.quick_insert(items[0].1);
    let s = &a.shapes[a.index_of(a.sel[0]).unwrap()];
    assert_eq!(s.kind, Kind::Ellipse);
    assert_eq!(s.pos + s.size / 2.0, a.to_world(a.view.min, a.view.center()).to_vec2().to_pos2());
}

#[test]
fn spell_checker_flags_a_misspelling() {
    if !crate::spell::available() {
        return;
    }
    let m = crate::spell::misspelled("The quikc brown fox");
    if m.is_empty() {
        eprintln!("no spell checker language installed here; skipped");
        return;
    }
    assert_eq!(m, vec![(4, 5)]);
}

#[test]
fn text_direction_axes_hanging_and_list_spacing() {
    use crate::model::*;
    use crate::textrun::{bidi_visual, family_axes};
    // Hebrew letters come out reversed, the Latin stays in order
    let base = Shape::default().base_style();
    let chars: Vec<(char, crate::textrun::CharStyle)> = "ab \u{5d0}\u{5d1}\u{5d2}".chars().map(|c| (c, base.clone())).collect();
    let shown: String = bidi_visual(chars, 0).into_iter().map(|c| c.0).collect();
    assert_eq!(shown, "ab \u{5d2}\u{5d1}\u{5d0}");
    let mut s = Shape::default();
    s.kind = Kind::Text;
    s.text = "\u{5d0}".into();
    assert!(s.is_rtl());
    s.text_dir = 1;
    assert!(!s.is_rtl());
    // the variable Inter has two axes
    let axes = crate::textrun::fvar_axes_for_test(include_bytes!("../assets/fonts/Inter.ttf"));
    assert_eq!(axes.iter().map(|a| a.0).collect::<Vec<_>>(), vec![*b"opsz", *b"wght"]);
    let _ = family_axes("no such family");
    // hanging: a bullet list and a leading quote hang, only when asked and left aligned
    s.text = "\u{201c}Quote".into();
    assert!(s.hang_lead().is_none());
    s.hang = true;
    assert_eq!(s.hang_lead().as_deref(), Some("\u{201c}"));
    s.list = 1;
    assert!(s.hang_lead().unwrap().starts_with('\u{2022}'));
    s.align = TextAlign::Center;
    assert!(s.hang_lead().is_none());
    // list spacing adds an empty line between the items
    let mut l = Shape::default();
    l.kind = Kind::Text;
    l.text = "one\ntwo".into();
    l.list = 1;
    l.list_spacing = 6.0;
    assert_eq!(l.display_chars().iter().filter(|c| c.1.spacer).count(), 1);
}

#[test]
fn custom_underline_marks() {
    use crate::model::*;
    use crate::textrun::Ul;
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
    let painter = egui::Painter::new(ctx, egui::LayerId::background(), Rect::EVERYTHING);
    let mut s = Shape::default();
    s.kind = Kind::Text;
    s.text = "Hello gy world".into();
    s.font_size = 20.0;
    s.underline = true;
    s.size = Vec2::new(200.0, 30.0);
    let galley = crate::render::text_galley(&painter, &s, 1.0, egui::Color32::WHITE, 200.0);
    let marks = |s: &Shape| crate::textrun::underline_marks(s, &galley, 1.0).marks;
    assert_eq!(marks(&s).len(), 1, "one solid line");
    s.ul_skip = true;
    assert!(marks(&s).len() >= 2, "ink skipped around the g and y");
    s.ul_skip = false;
    s.ul_style = 1;
    assert!(marks(&s).iter().all(|m| matches!(m, Ul::Dot(_))) && marks(&s).len() > 5);
    s.ul_style = 2;
    assert!(matches!(&marks(&s)[0], Ul::Line(p) if p.len() > 3));
}

#[test]
fn component_extras_push_restore_multi_edit_and_slots() {
    use crate::model::*;
    let mut a = App::default();
    // create several components at once
    let fs: Vec<u64> = (0..3).map(|k| frame(&mut a, k as f32 * 300.0, 0.0, 100.0, 100.0)).collect();
    a.sel = fs.clone();
    a.make_component();
    assert!(fs.iter().all(|&f| a.shapes[a.index_of(f).unwrap()].component), "all three become components");
    // push overrides: the instance's colour goes to the main and to the other instance
    let main = fs[0];
    let i1 = a.instantiate(main, None, Pos2::new(0.0, 500.0)).unwrap();
    let i2 = a.instantiate(main, None, Pos2::new(300.0, 500.0)).unwrap();
    let red = Paint::solid(egui::Color32::RED);
    let k = a.index_of(i1).unwrap();
    a.shapes[k].fills = vec![red.clone()];
    a.sel = vec![i1];
    a.push_overrides();
    assert_eq!(a.shapes[a.index_of(main).unwrap()].fills, vec![red.clone()]);
    assert_eq!(a.shapes[a.index_of(i2).unwrap()].fills, vec![red.clone()], "the other instance follows");
    // restore a deleted main component: the instances point at the new one again
    a.remove_subtree(main);
    assert!(a.main_is_missing(a.index_of(i1).unwrap()));
    a.sel = vec![i1];
    a.restore_main();
    let restored = a.index_of(main).expect("the main is back under its old id");
    assert!(a.shapes[restored].component && !a.shapes[restored].instance);
    assert!(!a.main_is_missing(a.index_of(i1).unwrap()));
    // slots: limits stop an instance adding more layers
    let slot = frame(&mut a, 0.0, 900.0, 200.0, 200.0);
    let si = a.index_of(slot).unwrap();
    a.shapes[si].slot = true;
    a.shapes[si].slot_max = 1;
    assert!(a.slot_add(slot, fs[1]));
    assert!(!a.slot_add(slot, fs[1]), "the slot takes one layer at most");
    assert_eq!(a.slot_content(slot).len(), 1);
}

#[test]
fn multi_edit_variants_copies_a_change_to_the_others() {
    use crate::model::*;
    let mut a = App::default();
    let mut kids = vec![];
    let mut comps = vec![];
    for k in 0..2 {
        let f = frame(&mut a, k as f32 * 300.0, 0.0, 100.0, 100.0);
        let child = rect_at(&mut a, 10.0, 10.0);
        a.reparent(child, Some(f));
        let ci = a.index_of(child).unwrap();
        a.shapes[ci].name = "Dot".into();
        kids.push(child);
        comps.push(f);
    }
    a.sel = comps.clone();
    a.make_component();
    a.sel = comps.clone();
    a.combine_as_variants();
    let set = a.shapes.iter().position(|s| s.component_set).expect("a set");
    a.sel = vec![a.shapes[set].id];
    a.toggle_multi_variants();
    assert!(a.multi_variants.is_some());
    let before = a.shapes.clone();
    let ci = a.index_of(kids[0]).unwrap();
    a.shapes[ci].opacity = 0.5;
    a.sync_variant_edit(&before);
    assert_eq!(a.shapes[a.index_of(kids[1]).unwrap()].opacity, 0.5, "the other variant's layer changed too");
}

#[test]
fn variable_duplicate_eyedropper_and_hidden() {
    use crate::model::*;
    let mut a = App::default();
    a.add_color_variable(egui::Color32::from_rgb(1, 2, 3));
    assert_eq!(a.variables[0].vars.len(), 1);
    assert_eq!(a.variables[0].vars[0].values[0], VarValue::Color(egui::Color32::from_rgb(1, 2, 3)));
    a.duplicate_variable(0, 0);
    assert_eq!(a.variables[0].vars.len(), 2);
    assert_eq!(a.variables[0].vars[1].name, "color 1 copy");
    assert_ne!(a.variables[0].vars[0].id, a.variables[0].vars[1].id);
    a.variables[0].vars[1].hidden = true;
    let lib = a.build_library();
    assert_eq!(lib.variables[0].vars.len(), 1, "the hidden variable is not published");
    // the eyedropper pick makes a variable when asked to
    a.eyedrop_var = true;
    a.apply_picked(egui::Color32::from_rgb(9, 9, 9), &egui::Context::default());
    assert_eq!(a.variables[0].vars.len(), 3);
    assert!(!a.eyedrop_var);
}

#[test]
fn corner_smoothing_makes_a_squircle() {
    use crate::geom::rounded_rect_smooth;
    let r = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
    let area = |pts: &[Pos2]| (0..pts.len()).map(|i| pts[i].x * pts[(i + 1) % pts.len()].y - pts[(i + 1) % pts.len()].x * pts[i].y).sum::<f32>().abs() / 2.0;
    let round = rounded_rect_smooth(r, [20.0; 4], 0.0, 1.0);
    let smooth = rounded_rect_smooth(r, [20.0; 4], 0.6, 1.0);
    assert!(area(&smooth) < area(&round), "smoothing reaches further along the edges: {} vs {}", area(&smooth), area(&round));
    assert!(smooth.iter().all(|p| r.expand(0.01).contains(*p)), "stays inside the rectangle");
    // the curve starts on the edge, 1.6 radii from the corner
    assert!(smooth.iter().any(|p| (p.x - 32.0).abs() < 0.01 && p.y.abs() < 0.01), "starts 32 px from the top left corner... {:?}", &smooth[..3]);
    // an all-zero radius stays a plain rectangle
    assert_eq!(rounded_rect_smooth(r, [0.0; 4], 0.6, 1.0).len(), 4);
    // clockwise and closed: the polygon winds the same way as the plain one
    let wind = |pts: &[Pos2]| (0..pts.len()).map(|i| pts[i].x * pts[(i + 1) % pts.len()].y - pts[(i + 1) % pts.len()].x * pts[i].y).sum::<f32>().signum();
    assert_eq!(wind(&smooth), wind(&round));
}

#[test]
fn dynamic_stroke_dash_cap_and_stroke_in_layout() {
    use crate::geom::{dynamic_polys, Poly};
    use crate::model::*;
    // a dynamic stroke has more points than the line it follows, moves them sideways, and keeps its ends
    let line = vec![Poly { pts: vec![Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0)], closed: false, hole: false }];
    let w = dynamic_polys(&line, &Dynamic { frequency: 10.0, wiggle: 4.0, smoothen: 0.0 });
    assert!(w[0].pts.len() >= 20, "{} points", w[0].pts.len());
    assert!(w[0].pts.iter().any(|p| p.y.abs() > 0.5) && w[0].pts.iter().all(|p| p.y.abs() <= 4.0));
    assert_eq!((w[0].pts[0], *w[0].pts.last().unwrap()), (Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0)));
    assert_eq!(dynamic_polys(&line, &Dynamic { frequency: 10.0, wiggle: 4.0, smoothen: 0.0 })[0].pts, w[0].pts, "the same wiggle every time");
    let smooth = dynamic_polys(&line, &Dynamic { frequency: 10.0, wiggle: 4.0, smoothen: 100.0 });
    assert!(smooth[0].pts.len() > w[0].pts.len());
    // a dashed stroke with a round dash cap is written with a round linecap
    let mut a = App::default();
    let id = rect_at(&mut a, 0.0, 0.0);
    let i = a.index_of(id).unwrap();
    a.shapes[i].strokes = vec![Paint::solid(egui::Color32::BLACK)];
    a.shapes[i].stroke_w = 2.0;
    a.shapes[i].dash = true;
    a.shapes[i].dash_cap = 1;
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
    let (svg, _) = a.svg_for(&ctx, &[id]).unwrap();
    assert!(svg.contains(r#"stroke-linecap="round""#) && svg.contains("stroke-dasharray"), "{svg}");
    // include stroke in layout: the frame's inside stroke pushes its children in
    let f = frame(&mut a, 500.0, 0.0, 200.0, 100.0);
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].layout = Some(AutoLayout { pad: [0.0; 4], hug_x: false, hug_y: false, ..AutoLayout::default() });
    a.shapes[fi].strokes = vec![Paint::solid(egui::Color32::BLACK)];
    a.shapes[fi].stroke_w = 6.0;
    a.shapes[fi].stroke_align = StrokeAlign::Inside;
    let kid = rect_at(&mut a, 0.0, 0.0);
    a.reparent(kid, Some(f));
    a.settle();
    let before = a.shapes[a.index_of(kid).unwrap()].pos;
    let fi = a.index_of(f).unwrap();
    a.shapes[fi].layout.as_mut().unwrap().stroke_in_layout = true;
    a.settle();
    let after = a.shapes[a.index_of(kid).unwrap()].pos;
    assert_eq!(after - before, Vec2::splat(6.0));
}

#[test]
fn hexagonal_pattern_tiles_and_alignment() {
    use crate::model::*;
    let mut a = App::default();
    let tile = rect_at(&mut a, 0.0, 1000.0);
    let target = rect_at(&mut a, 0.0, 0.0);
    let ti = a.index_of(target).unwrap();
    a.shapes[ti].size = Vec2::new(100.0, 100.0);
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
    let svg_with = |a: &mut App, tile_kind: u8, align: u8| {
        a.shapes[ti].fills = vec![Paint { kind: PaintKind::Pattern(PatternFill { source: Some(tile), tile: tile_kind, align, ..PatternFill::default() }), ..Paint::solid(egui::Color32::BLACK) }];
        a.svg_for(&ctx, &[target]).unwrap().0
    };
    let plain = svg_with(&mut a, 0, 0);
    let hex = svg_with(&mut a, 1, 0);
    assert!(hex.matches("scale(1)").count() >= plain.matches("scale(1)").count() + 2, "the cell holds three copies of the tile");
    let centred = svg_with(&mut a, 0, 4);
    assert!(centred.contains("x=\"45.00\"") && centred.contains("y=\"45.00\""), "starts a tile's half-size from the centre: {centred}");
}

static PROFILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn display_p3_profile_is_embedded_in_pngs() {
    let _lock = PROFILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let profile = crate::profile::display_p3_profile();
    assert_eq!(u32::from_be_bytes(profile[0..4].try_into().unwrap()) as usize, profile.len(), "the header carries the profile's size");
    assert_eq!(&profile[36..40], b"acsp");
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="#f00"/></svg>"##;
    let png = crate::export::render_png_in(svg, 1.0, true).unwrap();
    assert!(png.windows(4).any(|w| w == b"iCCP"), "the PNG names its profile");
    let img = image::load_from_memory(&png).expect("a PNG with an iCCP chunk still decodes");
    assert_eq!((img.width(), img.height()), (4, 4));
    assert!(!crate::export::render_png_in(svg, 1.0, false).unwrap().windows(4).any(|w| w == b"iCCP"), "sRGB files carry none");
    // the CSS speaks display-p3
    let css = crate::inspect::css_color_in(egui::Color32::from_rgb(255, 0, 0), 1.0, true);
    assert_eq!(css, "color(display-p3 1.000 0.000 0.000)");
}

#[test]
fn corner_handles_set_one_or_all_radii() {
    use crate::model::*;
    let mut a = App::default();
    let id = rect_at(&mut a, 0.0, 0.0);
    let i = a.index_of(id).unwrap();
    a.shapes[i].size = Vec2::new(200.0, 100.0);
    a.sel = vec![id];
    a.radius_drag(0, false, Pos2::new(20.0, 20.0));
    assert_eq!(a.shapes[i].radius, [20.0; 4]);
    a.radius_drag(1, true, Pos2::new(190.0, 10.0));
    assert_eq!(a.shapes[i].radius, [20.0, 10.0, 20.0, 20.0], "Alt moves one corner");
    a.radius_drag(2, false, Pos2::new(300.0, 200.0));
    assert_eq!(a.shapes[i].radius, [0.0; 4], "never below zero");
    a.radius_drag(3, false, Pos2::new(100.0, 0.0));
    assert_eq!(a.shapes[i].radius, [50.0; 4], "at most half the shorter side");
}

#[test]
fn webp_gif_and_multipage_pdf_exports() {
    use crate::model::*;
    let mut a = App::default();
    let f1 = frame(&mut a, 0.0, 0.0, 40.0, 30.0);
    let f2 = frame(&mut a, 100.0, 0.0, 60.0, 20.0);
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
    // WebP decodes back to the picture's size
    let webp = a.export_extra(&ctx, &[f1], ExportFormat::Webp, 2.0).unwrap().unwrap();
    let img = image::load_from_memory_with_format(&webp, image::ImageFormat::WebP).expect("a WebP");
    assert_eq!((img.width(), img.height()), (80, 60));
    // an animated GIF has several frames and the GIF signature
    let gif = a.export_extra(&ctx, &[f1], ExportFormat::Gif, 1.0).unwrap().unwrap();
    assert_eq!(&gif[..6], b"GIF89a");
    // a PDF of two frames has two pages
    let svgs: Vec<String> = [f1, f2].iter().map(|&f| a.svg_for(&ctx, &[f]).unwrap().0).collect();
    let pdf = crate::anim_export::pdf_pages(&svgs).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.starts_with("%PDF") && text.contains("/Count 2"), "two pages");
}

#[test]
fn mp4_and_webm_exports_through_ffmpeg() {
    use crate::model::*;
    if crate::video::ffmpeg().is_none() {
        eprintln!("no ffmpeg here; skipped");
        return;
    }
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 41.0, 31.0);
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
    let mp4 = a.export_extra(&ctx, &[f], ExportFormat::Mp4, 1.0).unwrap().unwrap();
    assert_eq!(&mp4[4..8], b"ftyp", "an MP4 container");
    let webm = a.export_extra(&ctx, &[f], ExportFormat::Webm, 1.0).unwrap().unwrap();
    assert_eq!(&webm[..4], &[0x1a, 0x45, 0xdf, 0xa3], "a WebM (Matroska) container");
}

#[test]
fn sketch_file_imports_layers_symbols_and_styles() {
    use crate::model::*;
    use std::io::Write;
    let doc = serde_json::json!({ "pages": [{ "_ref": "pages/p1" }] });
    let rect = serde_json::json!({
        "_class": "rectangle", "name": "Card", "frame": { "x": 10, "y": 20, "width": 100, "height": 50 }, "rotation": 15,
        "points": [{ "cornerRadius": 8 }, { "cornerRadius": 8 }, { "cornerRadius": 8 }, { "cornerRadius": 8 }],
        "style": {
            "fills": [{ "isEnabled": true, "fillType": 1, "color": { "red": 1, "green": 0, "blue": 0, "alpha": 1 },
                "gradient": { "gradientType": 0, "from": "{0, 0.5}", "to": "{1, 0.5}", "stops": [{ "position": 0, "color": { "red": 1, "green": 0, "blue": 0, "alpha": 1 } }, { "position": 1, "color": { "red": 0, "green": 0, "blue": 1, "alpha": 1 } }] } }],
            "borders": [{ "isEnabled": true, "color": { "red": 0, "green": 0, "blue": 0, "alpha": 1 }, "thickness": 3, "position": 1 }],
            "shadows": [{ "isEnabled": true, "offsetX": 2, "offsetY": 4, "blurRadius": 6, "spread": 0, "color": { "red": 0, "green": 0, "blue": 0, "alpha": 0.5 } }],
            "contextSettings": { "opacity": 0.5, "blendMode": 2 }
        }
    });
    let text = serde_json::json!({ "_class": "text", "name": "Title", "frame": { "x": 0, "y": 0, "width": 80, "height": 20 },
        "attributedString": { "string": "Hello", "attributes": [{ "attributes": { "MSAttributedStringFontAttribute": { "attributes": { "name": "Helvetica-Bold", "size": 18 } }, "MSAttributedStringColorAttribute": { "red": 0, "green": 1, "blue": 0, "alpha": 1 } } }] } });
    let path = serde_json::json!({ "_class": "shapePath", "name": "Curve", "frame": { "x": 0, "y": 0, "width": 40, "height": 40 }, "isClosed": false,
        "points": [{ "point": "{0, 0}", "hasCurveFrom": true, "curveFrom": "{0.5, 0}" }, { "point": "{1, 1}", "hasCurveTo": true, "curveTo": "{1, 0.5}" }] });
    let master = serde_json::json!({ "_class": "symbolMaster", "name": "Button", "symbolID": "S1", "frame": { "x": 300, "y": 0, "width": 60, "height": 20 }, "layers": [rect.clone()] });
    let inst = serde_json::json!({ "_class": "symbolInstance", "name": "Btn", "symbolID": "S1", "frame": { "x": 400, "y": 0, "width": 70, "height": 25 } });
    let page = serde_json::json!({ "name": "Page A", "layers": [
        { "_class": "artboard", "name": "Screen", "frame": { "x": 0, "y": 0, "width": 375, "height": 800 }, "hasBackgroundColor": true, "backgroundColor": { "red": 1, "green": 1, "blue": 1, "alpha": 1 }, "layers": [rect, text, path, inst] },
        master, { "_class": "slice", "name": "s", "frame": { "x": 0, "y": 0, "width": 1, "height": 1 } }
    ] });
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("document.json", o).unwrap();
        z.write_all(doc.to_string().as_bytes()).unwrap();
        z.start_file("pages/p1.json", o).unwrap();
        z.write_all(page.to_string().as_bytes()).unwrap();
        z.finish().unwrap();
    }
    let (d, summary) = App::doc_from_sketch(&buf.into_inner()).expect("imports");
    assert_eq!((d.pages.len(), summary.symbols), (1, 1));
    assert_eq!(summary.skipped.get("slice"), Some(&1));
    let shapes = &d.pages[0].shapes;
    let by = |n: &str| shapes.iter().find(|s| s.name == n).unwrap_or_else(|| panic!("no {n}"));
    let card = by("Card");
    assert_eq!((card.kind, card.radius, card.rotation, card.opacity, card.blend), (Kind::Rect, [8.0; 4], -15.0, 0.5, BlendMode::Multiply));
    assert!(matches!(&card.fills[0].kind, PaintKind::Gradient(g) if g.stops.len() == 2 && g.angle.abs() < 1e-3));
    assert_eq!((card.stroke_w, card.stroke_align, card.effects.len()), (3.0, StrokeAlign::Inside, 1));
    let t = by("Title");
    assert_eq!((t.text.as_str(), t.font_size, t.weight, t.family.as_str()), ("Hello", 18.0, 700, "Helvetica"));
    let curve = by("Curve");
    assert_eq!(curve.kind, Kind::Path);
    assert!(curve.path[0].pts[0].hout != Vec2::ZERO && curve.path[0].pts[1].hin != Vec2::ZERO, "the handles came along");
    let screen = by("Screen");
    assert_eq!((screen.kind, screen.clip), (Kind::Frame, true));
    let master = by("Button");
    assert!(master.component);
    let inst = shapes.iter().find(|s| s.instance && s.src == Some(master.id)).expect("the instance of the symbol");
    assert_eq!(inst.size, Vec2::new(70.0, 25.0));
    assert_eq!(inst.parent, Some(screen.id), "inside the artboard it was in");
}


#[test]
fn branch_review_is_asked_answered_and_not_required() {
    let mut a = App::default();
    rect_at(&mut a, 0.0, 0.0);
    a.create_branch("Feature");
    let k = a.cur_branch;
    a.request_review(k, "Ann");
    assert_eq!((a.branches[k].review.state, a.branches[k].review.reviewer.as_str()), (1, "Ann"));
    a.user = "Ann".into();
    a.answer_review(k, false, "move it left");
    assert_eq!((a.branches[k].review.state, a.branches[k].review.note.as_str()), (3, "move it left"));
    a.request_review(k, "Ann");
    a.answer_review(k, true, "");
    assert_eq!((a.branches[k].review.state, a.branches[k].review.by.as_str()), (2, "Ann"));
    // a review is not required: a branch merges with one still waiting, and says so
    a.request_review(k, "Ann");
    a.merge_branch(k);
    assert!(a.status.starts_with("merged") && a.status.contains("without an approved review"), "{}", a.status);
}

#[test]
fn smart_selection_respaces_and_reorders_a_row() {
    use crate::model::*;
    use crate::Drag;
    let mut a = App::default();
    let ids: Vec<u64> = [0.0, 30.0, 60.0].iter().map(|&x| rect_at(&mut a, x, 0.0)).collect();
    a.sel = ids.clone();
    let row = a.smart_row().expect("a row");
    assert!(!row.vertical && row.ids == ids);
    let x = |a: &App, id: u64| a.shapes[a.index_of(id).unwrap()].pos.x;
    // a gap handle: every gap follows the pointer
    a.zoom = 1.0;
    a.pan = Vec2::ZERO;
    let Some(Drag::Smart { vertical, start, from }) = a.smart_handle_at(Pos2::ZERO, Pos2::new(20.0, 5.0), false) else { panic!("no handle under the pointer") };
    assert_eq!(start, 20.0);
    a.smart_gap_drag(vertical, start, from, Pos2::new(30.0, 5.0));
    assert_eq!(ids.iter().map(|&i| x(&a, i)).collect::<Vec<_>>(), vec![0.0, 40.0, 80.0], "gaps of 30");
    a.smart_gap_drag(vertical, start, from, Pos2::new(20.0, 5.0));
    assert_eq!(ids.iter().map(|&i| x(&a, i)).collect::<Vec<_>>(), vec![0.0, 30.0, 60.0]);
    // Ctrl-drag the first layer to the far end: the order changes and the gaps stay
    let Some(Drag::SmartMove { vertical, id, grab }) = a.smart_handle_at(Pos2::ZERO, Pos2::new(5.0, 5.0), true) else { panic!("no layer under the pointer") };
    a.smart_move_drag(vertical, id, grab, Pos2::new(75.0, 5.0));
    a.smart_move_finish(vertical);
    assert_eq!(ids.iter().map(|&i| x(&a, i)).collect::<Vec<_>>(), vec![60.0, 0.0, 30.0], "the first is last now; the others moved up");
}

#[test]
fn keyboard_creates_shapes_and_grows_a_selection_box() {
    use crate::model::*;
    let mut a = App::default();
    a.view = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
    a.net_cursor = Some(Pos2::new(300.0, 200.0));
    a.set_tool(crate::Tool::Draw(Kind::Ellipse));
    a.enter_pressed();
    let s = &a.shapes[a.index_of(a.sel[0]).unwrap()];
    assert_eq!((s.kind, s.size, s.pos), (Kind::Ellipse, Vec2::splat(100.0), Pos2::new(250.0, 150.0)));
    assert_eq!(a.tool, crate::Tool::Select);
    // a box grown with the keyboard selects what it reaches
    let near = rect_at(&mut a, 330.0, 190.0);
    let far = rect_at(&mut a, 600.0, 190.0);
    a.net_cursor = Some(Pos2::new(300.0, 200.0));
    a.kbox = None;
    a.kbox_grow(Vec2::new(1.0, 0.0), 50.0);
    assert!(a.sel.contains(&near) && !a.sel.contains(&far));
    a.kbox_grow(Vec2::new(1.0, 0.0), 300.0);
    assert!(a.sel.contains(&far));
    a.escape();
    assert!(a.kbox.is_none());
}

#[test]
fn export_settings_size_profile_jpg_and_outlined_text() {
    let _lock = PROFILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use crate::model::*;
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 100.0, 50.0);
    let t = rect_at(&mut a, 5.0, 5.0);
    let ti = a.index_of(t).unwrap();
    a.shapes[ti].kind = Kind::Text;
    a.shapes[ti].text = "Hi".into();
    a.shapes[ti].size = Vec2::new(40.0, 20.0);
    a.reparent(t, Some(f));
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
    a.ctx = Some(ctx.clone());
    let (svg, _) = a.svg_for(&ctx, &[f]).unwrap();
    // 512w: the layer comes out 512 px wide, whatever its size
    let w512 = ExportSetting { dim: 1, scale: 512.0, ..ExportSetting::default() };
    assert_eq!(w512.effective_scale(Vec2::new(100.0, 50.0)), 5.12);
    let png = a.export_one(&ctx, f, &w512, &svg).unwrap();
    assert_eq!(image::load_from_memory(&png).unwrap().width(), 512);
    // a Display P3 export carries the profile, whatever the file says
    let p3 = ExportSetting { profile: 2, ..ExportSetting::default() };
    assert!(a.export_one(&ctx, f, &p3, &svg).unwrap().windows(4).any(|w| w == b"iCCP"));
    // JPG decodes
    let jpg = a.export_one(&ctx, f, &ExportSetting { format: ExportFormat::Jpg, quality: 0, ..ExportSetting::default() }, &svg).unwrap();
    assert_eq!(&jpg[..2], &[0xff, 0xd8]);
    assert_eq!(image::load_from_memory(&jpg).unwrap().width(), 100);
    // outline text: no <text> left
    assert!(svg.contains("<text"));
    let outlined = a.export_one(&ctx, f, &ExportSetting { format: ExportFormat::Svg, outline_text: true, ..ExportSetting::default() }, &svg).unwrap();
    let outlined = String::from_utf8(outlined).unwrap();
    assert!(!outlined.contains("<text") && outlined.contains("<path"), "{outlined}");
}

#[test]
fn set_variable_mode_switches_the_page() {
    use crate::model::*;
    let mut a = App::default();
    let id = rect_at(&mut a, 0.0, 0.0);
    let i = a.index_of(id).unwrap();
    a.variables.push(Collection {
        id: 1,
        name: "Theme".into(),
        modes: vec![VarMode { id: 10, name: "Light".into() }, VarMode { id: 11, name: "Dark".into() }],
        vars: vec![Variable { id: 5, name: "n".into(), kind: VarKind::Number, values: vec![VarValue::Number(1.0), VarValue::Number(2.0)], ..Variable::default() }],
        ..Collection::default()
    });
    assert_eq!(a.variable_value(5, i), Some(VarValue::Number(1.0)));
    let ix = Interaction { action: Action::SetMode, opts: IxOpts { coll: 1, mode: 11, ..IxOpts::default() }, ..Interaction::default() };
    a.present_fire(i, &ix, 0.0);
    assert_eq!(a.variable_value(5, i), Some(VarValue::Number(2.0)), "the page is in Dark now");
    // a layer that chose its own mode keeps it
    a.shapes[i].modes = vec![(1, 10)];
    assert_eq!(a.variable_value(5, i), Some(VarValue::Number(1.0)));
}

#[test]
fn library_publish_hides_dotted_components_and_carries_a_note() {
    let mut a = App::default();
    let f1 = frame(&mut a, 0.0, 0.0, 50.0, 50.0);
    let f2 = frame(&mut a, 100.0, 0.0, 50.0, 50.0);
    for (id, name) in [(f1, "Button"), (f2, ".internal")] {
        let i = a.index_of(id).unwrap();
        a.shapes[i].name = name.into();
    }
    a.sel = vec![f1, f2];
    a.make_component();
    a.lib_description = "new button".into();
    let lib = a.build_library();
    assert_eq!(lib.components.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Button"]);
    assert_eq!(lib.description, "new button");
}

// writes a document that shows the newer features, for a look in the running app (cargo test make_demo_doc -- --ignored)
#[test]
#[ignore]
fn make_demo_doc() {
    use crate::model::*;
    let mut a = App::default();
    let f = frame(&mut a, 0.0, 0.0, 700.0, 400.0);
    let r = rect_at(&mut a, 30.0, 30.0);
    let ri = a.index_of(r).unwrap();
    a.shapes[ri].size = Vec2::new(160.0, 160.0);
    a.shapes[ri].radius = [48.0; 4];
    a.shapes[ri].corner_smooth = 0.6;
    a.shapes[ri].fills = vec![Paint::solid(egui::Color32::from_rgb(0x0d, 0x99, 0xff))];
    a.reparent(r, Some(f));
    let r2 = rect_at(&mut a, 210.0, 30.0);
    let r2i = a.index_of(r2).unwrap();
    a.shapes[r2i].size = Vec2::new(160.0, 160.0);
    a.shapes[r2i].radius = [48.0; 4];
    a.shapes[r2i].fills = vec![Paint::solid(egui::Color32::from_rgb(0x0d, 0x99, 0xff))];
    a.reparent(r2, Some(f));
    let t = rect_at(&mut a, 30.0, 220.0);
    let ti = a.index_of(t).unwrap();
    a.shapes[ti].kind = Kind::Text;
    a.shapes[ti].text = "Wavy underline, skip ink: gypsy".into();
    a.shapes[ti].font_size = 24.0;
    a.shapes[ti].size = Vec2::new(400.0, 40.0);
    a.shapes[ti].underline = true;
    a.shapes[ti].ul_style = 2;
    a.shapes[ti].ul_skip = true;
    a.shapes[ti].fills = vec![Paint::solid(egui::Color32::BLACK)];
    a.reparent(t, Some(f));
    let c = frame(&mut a, 400.0, 30.0, 260.0, 160.0);
    let ci = a.index_of(c).unwrap();
    a.shapes[ci].code = Some("<button>Hello</button>".into());
    a.reparent(c, Some(f));
    let line = rect_at(&mut a, 30.0, 300.0);
    let li = a.index_of(line).unwrap();
    a.shapes[li].size = Vec2::new(300.0, 40.0);
    a.shapes[li].repeat = Some(Repeat { kind: 0, count: 4, step: Vec2::new(90.0, 0.0), ..Repeat::linear() });
    a.shapes[li].size = Vec2::new(60.0, 40.0);
    a.reparent(line, Some(f));
    a.save_to(std::env::temp_dir().join("drui-demo.json"));
}
