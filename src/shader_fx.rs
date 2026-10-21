// Shader effects, as Figma 126.9.13 has them (observed with REA, a screenshot of the library and of Halftone's settings): under the
// effects, a "Shader effects" list of rows (icon, name, eye, minus); "Add effect > Shader" opens a library window left of the panel
// (search, Created by you, a grid of presets with a picture and a name); a row opens its own settings popover (a type dropdown in the
// header, rows of controls, "Clip to alpha").
//
// Figma's presets are GPU shaders (halftone, refraction, particles ...) and its "Create new" asks an AI. drui's are its own procedural
// patterns, the ones of Shader fills (`ShaderKind`) plus Dots, drawn over the layer by the exporter and the canvas as an SVG pattern:
// Scale, Rotation, Softness (a blur), Opacity, Blend and the two colours are the controls; "Clip to alpha" keeps the pattern inside
// the layer's shape, off it covers the layer's box. There is no AI and no user-made shader.
use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Vec2};

use crate::fig::{self, Grid};
use crate::glyphs::{paint_glyph_sized, Glyph};
use crate::model::*;
use crate::picker;
use crate::{BG_MENU, R_LARGE, TEXT, TEXT_SECONDARY, TEXT_TERTIARY};

pub fn kind_label(k: ShaderKind) -> &'static str {
    ShaderKind::ALL.iter().find(|(x, _)| *x == k).map_or("", |(_, l)| l)
}

fn kind_glyph(k: ShaderKind) -> Glyph {
    match k {
        ShaderKind::Waves => Glyph::Waves,
        ShaderKind::Grid | ShaderKind::Checker => Glyph::Grid,
        ShaderKind::Stripes => Glyph::Rows,
        _ => Glyph::Grip,
    }
}

// a small picture of a preset for the library
fn thumb(p: &egui::Painter, r: Rect, k: ShaderKind) {
    p.rect_filled(r, fig::RADIUS, Color32::from_gray(0x2a));
    let c = Color32::from_rgb(0x9a, 0xb4, 0xff);
    let clip = p.with_clip_rect(r);
    match k {
        ShaderKind::Dots => {
            let mut y = r.top() + 8.0;
            while y < r.bottom() {
                let mut x = r.left() + 8.0 + if ((y - r.top()) / 14.0) as i32 % 2 == 0 { 0.0 } else { 7.0 };
                while x < r.right() {
                    let d = ((x - r.center().x).powi(2) + (y - r.center().y).powi(2)).sqrt() / r.width();
                    clip.circle_filled(Pos2::new(x, y), (5.0 - d * 7.0).max(0.8), c);
                    x += 14.0;
                }
                y += 14.0;
            }
        }
        ShaderKind::Stripes => {
            let mut x = r.left() - r.height();
            while x < r.right() {
                clip.line_segment([Pos2::new(x, r.bottom()), Pos2::new(x + r.height(), r.top())], Stroke::new(5.0, c));
                x += 12.0;
            }
        }
        ShaderKind::Checker => {
            for iy in 0..6 {
                for ix in 0..8 {
                    if (ix + iy) % 2 == 0 {
                        clip.rect_filled(Rect::from_min_size(r.min + Vec2::new(ix as f32 * 16.0, iy as f32 * 16.0), Vec2::splat(16.0)), 0.0, c);
                    }
                }
            }
        }
        ShaderKind::Grid => {
            let mut x = r.left();
            while x < r.right() {
                clip.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(1.0, c));
                x += 16.0;
            }
            let mut y = r.top();
            while y < r.bottom() {
                clip.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], Stroke::new(1.0, c));
                y += 16.0;
            }
        }
        ShaderKind::Waves => {
            for row in 0..5 {
                let y0 = r.top() + 12.0 + row as f32 * 16.0;
                let pts: Vec<Pos2> = (0..=40).map(|i| Pos2::new(r.left() + i as f32 * r.width() / 40.0, y0 + (i as f32 * 0.5 + row as f32).sin() * 5.0)).collect();
                clip.add(egui::Shape::line(pts, Stroke::new(2.0, c)));
            }
        }
        ShaderKind::Plasma | ShaderKind::Noise => {
            let mut seed = if k == ShaderKind::Plasma { 7u32 } else { 19 };
            for iy in 0..12 {
                for ix in 0..16 {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let v = ((seed >> 24) as f32 / 255.0 * if k == ShaderKind::Plasma { 0.6 } else { 1.0 }) + if k == ShaderKind::Plasma { (ix as f32 * 0.4).sin() * 0.2 + 0.2 } else { 0.0 };
                    clip.rect_filled(Rect::from_min_size(r.min + Vec2::new(ix as f32 * 8.0, iy as f32 * 8.0), Vec2::splat(8.0)), 0.0, Color32::from_rgb((60.0 + v * 120.0) as u8, (80.0 + v * 100.0) as u8, (150.0 + v * 90.0) as u8));
                }
            }
        }
    }
}

// one shader of the layer in the Effects section: the pill (blue while its settings are open), the eye and the minus; true when removed
pub fn shader_row(ui: &mut egui::Ui, g: &Grid, r: Rect, sh: &mut ShaderEffect, salt: (usize, u64)) -> bool {
    let id = ui.id().with(("shader-effect", salt.0, salt.1));
    let pill = Rect::from_min_max(Pos2::new(g.left, r.top()), Pos2::new(g.right - 60.0, r.bottom()));
    let resp = ui.interact(pill, id, Sense::click());
    let open_id = id.with("open");
    let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let (fill, ink) = if open { (fig::SELECTED(), fig::SELECTED_INK()) } else { (if resp.hovered() { fig::INPUT_HOVER() } else { fig::INPUT() }, if sh.visible { TEXT() } else { TEXT_TERTIARY() }) };
    ui.painter().rect_filled(pill, fig::RADIUS, fill);
    let name = kind_label(sh.fill.kind);
    paint_glyph_sized(ui.painter(), Pos2::new(pill.left() + 14.0, pill.center().y), kind_glyph(sh.fill.kind), ink, 14.0);
    fig::ptext(&ui.painter().with_clip_rect(pill), Pos2::new(pill.left() + 30.0, pill.center().y), Align2::LEFT_CENTER, name, fig::reg(11.0), ink);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    let shown = picker::popup(&resp, ui.max_rect().left(), |ui| settings(ui, sh, id));
    ui.data_mut(|d| d.insert_temp(open_id, shown.is_some()));
    let eye = Rect::from_center_size(Pos2::new(g.right - 40.0, r.center().y), Vec2::splat(fig::ROW));
    if fig::icon_button(ui, eye, if sh.visible { Glyph::Eye } else { Glyph::EyeOff }, if sh.visible { "Hide shader effect" } else { "Show shader effect" }, false).clicked() {
        sh.visible ^= true;
    }
    fig::icon_button(ui, g.icon(r), Glyph::Minus, "Remove shader effect", false).clicked()
}

// a shader's settings popover: the pattern as a flat dropdown in the header, then the controls
pub(crate) fn settings(ui: &mut egui::Ui, sh: &mut ShaderEffect, key: Id) {
    let r = fig::row(ui, 40.0);
    let dd = Rect::from_min_size(Pos2::new(r.left() + 8.0, r.top() + 8.0), Vec2::new(150.0, fig::ROW));
    let resp = ui.interact(dd, key.with("kind"), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(dd, fig::RADIUS, fig::INPUT());
    }
    paint_glyph_sized(ui.painter(), Pos2::new(dd.left() + 14.0, dd.center().y), kind_glyph(sh.fill.kind), TEXT(), 14.0);
    let t = fig::ptext(ui.painter(), Pos2::new(dd.left() + 30.0, dd.center().y), Align2::LEFT_CENTER, kind_label(sh.fill.kind), fig::reg(11.0), TEXT());
    paint_glyph_sized(ui.painter(), Pos2::new(t.right() + 10.0, dd.center().y), Glyph::ChevronDown, TEXT(), 9.5);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Shader pattern"));
    egui::Popup::menu(&resp).show(|ui| {
        for (k, label) in ShaderKind::ALL {
            if crate::glyphs::menu_item(ui, Some(kind_glyph(k)), label, "", true) {
                sh.fill.kind = k;
                ui.close();
            }
        }
    });
    if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 32.0, r.top() + 8.0), Vec2::splat(fig::ROW)), Glyph::Close, "Close", false).clicked() {
        ui.close();
    }
    fig::rule(ui);
    fig::row(ui, 12.0);
    let field = |r: Rect| Rect::from_min_size(Pos2::new(r.left() + 88.0, r.top() + 4.0), Vec2::new(136.0, fig::ROW));
    let label = |ui: &egui::Ui, r: Rect, t: &str| fig::ptext(ui.painter(), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, t, fig::reg(11.0), TEXT_SECONDARY());
    for (name, a_color) in [("Color", true), ("Background", false)] {
        let r = fig::row(ui, 32.0);
        label(ui, r, name);
        let f = field(r);
        fig::input_bg(ui, f, false);
        let c = if a_color { &mut sh.fill.b } else { &mut sh.fill.a };
        fig::swatch(ui, Pos2::new(f.left() + 14.0, f.center().y), c, &format!("Shader {name}"));
        fig::ptext(ui.painter(), Pos2::new(f.left() + 32.0, f.center().y), Align2::LEFT_CENTER, format!("{:02X}{:02X}{:02X}  {}%", c.r(), c.g(), c.b(), (c.a() as f32 / 255.0 * 100.0).round()), fig::reg(11.0), TEXT());
    }
    let mut scale = (sh.fill.scale * 20.0).round();
    let mut soft = sh.softness;
    let mut opacity = (sh.opacity * 100.0).round();
    let rows: [(&str, &str, f32, f32); 4] = [("Scale", "Shader scale", 1.0, 100.0), ("Softness", "Shader softness", 0.0, 100.0), ("Rotation", "Shader rotation", -180.0, 180.0), ("Opacity", "Shader opacity", 0.0, 100.0)];
    let mut rot = sh.fill.angle;
    for (k, (lab, name, lo, hi)) in rows.into_iter().enumerate() {
        let r = fig::row(ui, 32.0);
        label(ui, r, lab);
        let v = match k {
            0 => &mut scale,
            1 => &mut soft,
            2 => &mut rot,
            _ => &mut opacity,
        };
        fig::slider_field(ui, field(r), key.with(("shader-row", k)), v, lo, hi, name);
    }
    sh.fill.scale = scale / 20.0;
    sh.softness = soft;
    sh.fill.angle = rot;
    sh.opacity = opacity / 100.0;
    let r = fig::row(ui, 32.0);
    label(ui, r, "Blend");
    let items: Vec<(BlendMode, &str)> = BlendMode::ALL.iter().map(|b| (*b, b.label())).collect();
    fig::dropdown_outlined(ui, field(r), None, &mut sh.blend, &items, "Shader blend mode");
    let r = fig::row(ui, 32.0);
    label(ui, r, "Clip to alpha");
    fig::checkbox(ui, Rect::from_min_size(Pos2::new(r.left() + 88.0, r.top() + 4.0), Vec2::new(136.0, fig::ROW)), &mut sh.clip_alpha, "Clip to alpha");
    fig::row(ui, 12.0);
}

// the library window ("Shader effects", Beta in Figma): left of the panel, search and a grid of presets. The kind picked, if any.
pub fn library(ctx: &egui::Context, panel_left: f32, open: &mut bool) -> Option<ShaderKind> {
    if !*open {
        return None;
    }
    let mut picked = None;
    let search_id = Id::new("shader-library-search");
    let mut query: String = ctx.data(|d| d.get_temp(search_id)).unwrap_or_default();
    let pos = Pos2::new(panel_left - 8.0 - 360.0, 80.0);
    egui::Area::new(Id::new("shader-library")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
        egui::Frame::NONE.fill(BG_MENU()).stroke(Stroke::new(1.0, fig::RULE())).corner_radius(R_LARGE).show(ui, |ui| {
            ui.set_width(360.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let r = fig::row(ui, 48.0);
            fig::ptext(ui.painter(), Pos2::new(r.left() + 20.0, r.center().y), Align2::LEFT_CENTER, "Shader effects", fig::semi(13.0), TEXT());
            if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.right() - 40.0, r.top() + 12.0), Vec2::splat(fig::ROW)), Glyph::Close, "Close shader effects", false).clicked() {
                *open = false;
            }
            let r = fig::row(ui, 44.0);
            fig::text_input(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top() + 4.0), Vec2::new(r.width() - 32.0, 32.0)), &mut query, "Search shader effects", "Search");
            fig::rule(ui);
            let r = fig::row(ui, 36.0);
            fig::ptext(ui.painter(), Pos2::new(r.left() + 20.0, r.center().y), Align2::LEFT_CENTER, "By drui", fig::reg(11.0), TEXT_SECONDARY());
            let q = query.to_lowercase();
            let list: Vec<(ShaderKind, &str)> = ShaderKind::ALL.iter().filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q)).copied().collect();
            for (row, chunk) in list.chunks(2).enumerate() {
                let r = fig::row(ui, 128.0);
                for (col, (k, name)) in chunk.iter().enumerate() {
                    let card = Rect::from_min_size(Pos2::new(r.left() + 16.0 + col as f32 * 168.0, r.top()), Vec2::new(160.0, 120.0));
                    let resp = ui.interact(card, Id::new(("shader-card", row, col)), Sense::click());
                    thumb(&ui.painter().with_clip_rect(card), Rect::from_min_size(card.min, Vec2::new(160.0, 96.0)), *k);
                    if resp.hovered() {
                        ui.painter().rect_stroke(card, fig::RADIUS, Stroke::new(1.0, crate::BRAND), egui::StrokeKind::Inside);
                    }
                    fig::ptext(ui.painter(), Pos2::new(card.left() + 4.0, card.bottom() - 10.0), Align2::LEFT_CENTER, *name, fig::reg(11.0), TEXT());
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *name));
                    if resp.clicked() {
                        picked = Some(*k);
                    }
                }
            }
            if list.is_empty() {
                let r = fig::row(ui, 48.0);
                fig::ptext(ui.painter(), Pos2::new(r.left() + 20.0, r.center().y), Align2::LEFT_CENTER, "Nothing matches.", fig::reg(11.0), TEXT_TERTIARY());
            }
            fig::row(ui, 8.0);
        });
    });
    ctx.data_mut(|d| d.insert_temp(search_id, query));
    if picked.is_some() {
        *open = false;
    }
    picked
}
