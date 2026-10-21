// The Design panel while the Frame tool is active: Figma's device presets in collapsible groups, a click adds a frame of that size.
// Phone is read from Figma 126.9.13 (names and sizes, 32px rows, Phone open); the other groups' contents were collapsed in the
// running app and are from general knowledge: check them against Figma.
use egui::{Align2, Pos2, Sense, Vec2};

use crate::fig;
use crate::model::*;
use crate::{App, TEXT, TEXT_SECONDARY, TEXT_TERTIARY, Tool};

type Group = (&'static str, &'static [(&'static str, f32, f32)]);

pub const GROUPS: &[Group] = &[
    (
        "Phone",
        &[
            ("iPhone 17", 402.0, 874.0),
            ("iPhone 16 & 17 Pro", 402.0, 874.0),
            ("iPhone 16", 393.0, 852.0),
            ("iPhone 16 & 17 Pro Max", 440.0, 956.0),
            ("iPhone 16 Plus", 430.0, 932.0),
            ("iPhone Air", 420.0, 912.0),
            ("iPhone 14 & 15 Pro Max", 430.0, 932.0),
            ("iPhone 14 & 15 Pro", 393.0, 852.0),
            ("iPhone 13 & 14", 390.0, 844.0),
            ("iPhone 14 Plus", 428.0, 926.0),
            ("Android Compact", 412.0, 917.0),
            ("Android Medium", 700.0, 840.0),
        ],
    ),
    ("Tablet", &[("iPad mini 8.3", 744.0, 1133.0), ("iPad Pro 11\"", 834.0, 1194.0), ("iPad Pro 12.9\"", 1024.0, 1366.0), ("Surface Pro 8", 1440.0, 960.0)]),
    (
        "Desktop",
        &[("MacBook Air", 1280.0, 832.0), ("MacBook Pro 14\"", 1512.0, 982.0), ("MacBook Pro 16\"", 1728.0, 1117.0), ("Desktop", 1440.0, 1024.0), ("Wireframe", 1440.0, 1024.0), ("TV", 1280.0, 720.0)],
    ),
    ("Presentation", &[("Slide 16:9", 1920.0, 1080.0), ("Slide 4:3", 1024.0, 768.0)]),
    ("Watch", &[("Apple Watch Ultra", 410.0, 502.0), ("Apple Watch 45mm", 396.0, 484.0), ("Apple Watch 44mm", 368.0, 448.0), ("Apple Watch 41mm", 352.0, 430.0), ("Apple Watch 40mm", 324.0, 394.0)]),
    ("Paper", &[("A4", 595.0, 842.0), ("A5", 420.0, 595.0), ("A6", 298.0, 420.0), ("Letter", 612.0, 792.0), ("Tabloid", 792.0, 1224.0)]),
    (
        "Social media",
        &[("Twitter post", 1600.0, 900.0), ("Twitter header", 1500.0, 500.0), ("Facebook post", 1200.0, 630.0), ("Facebook cover", 820.0, 312.0), ("Instagram post", 1080.0, 1080.0), ("Instagram story", 1080.0, 1920.0), ("YouTube thumbnail", 1280.0, 720.0)],
    ),
    ("Figma Community", &[("Community thumbnail", 1920.0, 1080.0), ("Plugin icon", 128.0, 128.0), ("Widget cover", 1920.0, 1080.0)]),
    ("Archive", &[]),
];

impl App {
    pub fn frame_presets_panel(&mut self, ui: &mut egui::Ui) {
        fig::header(ui, "Frame", false, &[]);
        let mut add: Option<(&str, f32, f32)> = None;
        for (group, items) in GROUPS {
            let open = self.presets_open.contains(*group);
            // 32px rows; the label at 24px, the chevron at 13px (Figma 126.9.13, 48 / 1.5 device px)
            let r = fig::row(ui, 32.0);
            let resp = ui.interact(r, ui.id().with(("preset-group", *group)), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, 0.0, fig::INPUT());
            }
            // Figma draws these groups' arrows as small filled triangles (white when open, grey when closed)
            let (c, ink) = (Pos2::new(r.left() + 13.0, r.center().y), if open { TEXT() } else { TEXT_SECONDARY() });
            let tri = if open { [c + Vec2::new(-4.0, -2.5), c + Vec2::new(4.0, -2.5), c + Vec2::new(0.0, 3.0)] } else { [c + Vec2::new(-2.5, -4.0), c + Vec2::new(-2.5, 4.0), c + Vec2::new(3.0, 0.0)] };
            ui.painter().add(egui::Shape::convex_polygon(tri.to_vec(), ink, egui::Stroke::NONE));
            fig::ptext(ui.painter(), Pos2::new(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, group, fig::reg(11.0), TEXT());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, *group));
            if resp.clicked() {
                if open {
                    self.presets_open.remove(*group);
                } else {
                    self.presets_open.insert((*group).to_owned());
                }
            }
            if open {
                for (name, w, h) in items.iter() {
                    let r = fig::row(ui, 32.0);
                    let resp = ui.interact(r, ui.id().with(("preset", *group, *name)), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 0.0, fig::INPUT());
                    }
                    fig::ptext(ui.painter(), Pos2::new(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, name, fig::reg(11.0), TEXT());
                    fig::ptext(ui.painter(), Pos2::new(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, format!("{}×{}", w, h), fig::reg(11.0), TEXT_TERTIARY());
                    let label = format!("{name} {w}×{h}");
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone()));
                    if resp.clicked() {
                        add = Some((name, *w, *h));
                    }
                }
            }
            fig::rule(ui);
        }
        if let Some((name, w, h)) = add {
            self.add_preset_frame(name, Vec2::new(w, h));
        }
    }

    // a frame of `size` in the middle of the view; the tool goes back to Move
    pub fn add_preset_frame(&mut self, name: &str, size: Vec2) {
        let centre = self.to_world(self.view.min, self.view.center());
        let id = self.new_id();
        let mut f = Shape::new(Kind::Frame, id, self.shapes.len() + 1, (centre - size / 2.0).round(), None);
        f.size = size;
        f.name = name.to_owned();
        self.shapes.push(f);
        self.sel = vec![id];
        self.set_tool(Tool::Select);
    }
}
