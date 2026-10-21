// The component gallery: `drui --gallery` shows the design tokens and every shared panel widget (fig.rs,
// picker.rs, glyphs.rs) as they render, in a 240px panel like the real ones. It is the visual half of
// docs/DESIGN.md: copy what you see here, do not restyle it. Add a widget to fig.rs, add it here.
use crate::fig::Tip as _;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};

use crate::fig::{self, Grid, Num, Prefix};
use crate::glyphs::{menu_item, paint_glyph_sized, Glyph};
use crate::{App, BG, BG_MENU, BG_PRESSED, BG_SECONDARY, BRAND, GUIDE, R_LARGE, R_MEDIUM, R_SMALL, SELECTED_ROW, TEXT, TEXT_SECONDARY, TEXT_TERTIARY};

// widget values that live only while the gallery is open
#[derive(Clone)]
struct Demo {
    w: f32,
    h: f32,
    opacity: f32,
    sizing: u8,
    flow: usize,
    wrap: bool,
    check: bool,
    color: Color32,
    text: String,
    tab: bool,
}

impl Default for Demo {
    fn default() -> Self {
        Demo { w: 200.0, h: 120.0, opacity: 100.0, sizing: 0, flow: 1, wrap: false, check: true, color: Color32::from_rgb(0xef, 0xc7, 0x5e), text: String::new(), tab: true }
    }
}

fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

impl App {
    pub fn gallery(&mut self, ui: &mut egui::Ui) {
        let mut demo: Demo = ui.data(|d| d.get_temp(egui::Id::new("gallery-demo"))).unwrap_or_default();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.horizontal_top(|ui| {
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.set_width(420.0);
                    ui.add_space(24.0);
                    tokens(ui);
                });
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.add_space(24.0);
                    ui.label(egui::RichText::new("Panel components (240px, as in the side panels)").color(TEXT_SECONDARY()).size(11.0));
                    ui.add_space(8.0);
                    ui.allocate_ui(Vec2::new(240.0, 10.0), |ui| {
                        egui::Frame::NONE.fill(BG()).stroke(Stroke::new(1.0, fig::RULE())).show(ui, |ui| {
                            ui.set_width(240.0);
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            panel_specimens(ui, &mut demo);
                        });
                    });
                    ui.add_space(24.0);
                });
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.add_space(24.0);
                    ui.label(egui::RichText::new("Effect settings popovers (Glass, Drop shadow, Layer blur)").color(TEXT_SECONDARY()).size(11.0));
                    ui.add_space(8.0);
                    for (k, kind) in [crate::model::EffectKind::Glass, crate::model::EffectKind::DropShadow, crate::model::EffectKind::LayerBlur].into_iter().enumerate() {
                        let id = egui::Id::new(("gallery-fx", k));
                        let mut fx: crate::model::Effect = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| crate::model::Effect::new(kind));
                        ui.allocate_ui(Vec2::new(240.0, 10.0), |ui| {
                            egui::Frame::NONE.fill(BG_MENU()).stroke(Stroke::new(1.0, fig::RULE())).corner_radius(R_LARGE).show(ui, |ui| {
                                ui.set_width(240.0);
                                ui.spacing_mut().item_spacing = Vec2::ZERO;
                                crate::strokefx::effect_settings(ui, &mut fx, id.with("key"), &[]);
                            });
                        });
                        ui.add_space(12.0);
                        ui.data_mut(|d| d.insert_temp(id, fx));
                    }
                    let gid = egui::Id::new("gallery-grid");
                    let mut lg: crate::model::LayoutGrid = ui.data(|d| d.get_temp(gid)).unwrap_or_else(|| crate::model::LayoutGrid::new(crate::model::GridKind::Columns));
                    ui.allocate_ui(Vec2::new(240.0, 10.0), |ui| {
                        egui::Frame::NONE.fill(BG_MENU()).stroke(Stroke::new(1.0, fig::RULE())).corner_radius(R_LARGE).show(ui, |ui| {
                            ui.set_width(240.0);
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            crate::props::grid_popover(ui, &mut lg, gid.with("key"), &[]);
                        });
                    });
                    ui.data_mut(|d| d.insert_temp(gid, lg));
                    ui.add_space(12.0);
                    let xid = egui::Id::new("gallery-export");
                    let mut xs: crate::model::ExportSetting = ui.data(|d| d.get_temp(xid)).unwrap_or(crate::model::ExportSetting { format: crate::model::ExportFormat::Svg, ..Default::default() });
                    ui.allocate_ui(Vec2::new(240.0, 10.0), |ui| {
                        egui::Frame::NONE.fill(BG_MENU()).stroke(Stroke::new(1.0, fig::RULE())).corner_radius(R_LARGE).show(ui, |ui| {
                            ui.set_width(240.0);
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            crate::panels::export_popover(ui, &mut xs);
                        });
                    });
                    ui.data_mut(|d| d.insert_temp(xid, xs));
                    ui.add_space(12.0);
                    let sid = egui::Id::new("gallery-shader");
                    let mut sh: crate::model::ShaderEffect = ui.data(|d| d.get_temp(sid)).unwrap_or_default();
                    ui.allocate_ui(Vec2::new(240.0, 10.0), |ui| {
                        egui::Frame::NONE.fill(BG_MENU()).stroke(Stroke::new(1.0, fig::RULE())).corner_radius(R_LARGE).show(ui, |ui| {
                            ui.set_width(240.0);
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            crate::shader_fx::settings(ui, &mut sh, sid.with("key"));
                        });
                    });
                    ui.data_mut(|d| d.insert_temp(sid, sh));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new("Main menu (inset rows, Actions row, group gaps)").color(TEXT_SECONDARY()).size(11.0));
                    ui.add_space(8.0);
                    ui.allocate_ui(Vec2::new(210.0, 10.0), |ui| {
                        crate::glyphs::fig_menu_style(ui.style_mut());
                        egui::Frame::menu(ui.style()).show(ui, |ui| {
                            ui.spacing_mut().menu_width = 194.0;
                            crate::glyphs::menu_search_row(ui, "Actions…", "Ctrl+/");
                            crate::glyphs::menu_sep(ui);
                            crate::glyphs::menu_sub(ui, "File");
                            crate::glyphs::menu_sub(ui, "Edit");
                            crate::glyphs::menu_sep(ui);
                            crate::glyphs::menu_check(ui, true, "Snap to objects", "");
                            crate::glyphs::menu_item(ui, None, "Save", "Ctrl+S", true);
                        });
                    });
                });
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.add_space(24.0);
                    ui.label(egui::RichText::new("Icons (Lucide, rasterized per size on the pixel grid)").color(TEXT_SECONDARY()).size(11.0));
                    ui.add_space(8.0);
                    icons(ui);
                });
            });
        });
        ui.data_mut(|d| d.insert_temp(egui::Id::new("gallery-demo"), demo));
        let _ = &self.shapes;
    }
}

fn caption(ui: &mut egui::Ui, text: &str) {
    ui.add_space(20.0);
    ui.label(egui::RichText::new(text).color(TEXT_SECONDARY()).size(11.0));
    ui.add_space(8.0);
}

fn tokens(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("drui design system").color(TEXT()).size(18.0).strong());
    ui.label(egui::RichText::new("Dark theme. Tokens live in main.rs and fig.rs; see docs/DESIGN.md.").color(TEXT_SECONDARY()).size(11.0));
    caption(ui, "Color");
    let colors: [(&str, Color32, &str); 15] = [
        ("BG", BG(), "panels, toolbar, popovers"),
        ("BG_MENU", BG_MENU(), "canvas, menus"),
        ("INPUT / BG_SECONDARY", BG_SECONDARY(), "fields, hover, pills"),
        ("INPUT_HOVER", fig::INPUT_HOVER(), "field hover"),
        ("RULE / BG_PRESSED", BG_PRESSED(), "dividers, borders, pressed"),
        ("SELECTED", fig::SELECTED(), "selected row, toggled icon"),
        ("SELECTED_INK", fig::SELECTED_INK(), "icon on SELECTED, links"),
        ("BRAND", BRAND, "active tool, focus"),
        ("SHARE", fig::SHARE, "Share, pins, send, checkbox"),
        ("GUIDE", GUIDE, "guides, snap lines"),
        ("COMPONENT_INK", fig::COMPONENT_INK, "components, instances"),
        ("TEXT", TEXT(), "primary text, icons"),
        ("TEXT_SECONDARY", TEXT_SECONDARY(), "labels (white 70%)"),
        ("TEXT_TERTIARY", TEXT_TERTIARY(), "disabled, hints (white 40%)"),
        ("SELECTED_ROW", SELECTED_ROW, "same as SELECTED"),
    ];
    for (name, c, usage) in colors {
        let (r, _) = ui.allocate_exact_size(Vec2::new(420.0, 28.0), Sense::hover());
        let sw = Rect::from_min_size(r.min + Vec2::new(0.0, 2.0), Vec2::new(40.0, 24.0));
        ui.painter().rect_filled(sw, R_MEDIUM, c);
        ui.painter().rect_stroke(sw, R_MEDIUM, Stroke::new(1.0, Color32::from_white_alpha(20)), egui::StrokeKind::Inside);
        fig::ptext(ui.painter(), Pos2::new(r.left() + 52.0, r.center().y), Align2::LEFT_CENTER, name, fig::semi(11.0), TEXT());
        fig::ptext(ui.painter(), Pos2::new(r.left() + 190.0, r.center().y), Align2::LEFT_CENTER, hex(c), fig::reg(11.0), TEXT_SECONDARY());
        fig::ptext(ui.painter(), Pos2::new(r.left() + 250.0, r.center().y), Align2::LEFT_CENTER, usage, fig::reg(11.0), TEXT_SECONDARY());
    }
    caption(ui, "Type: Inter, 11px values and names; semibold titles and tabs; 13px semibold for names; 10px field labels");
    for (sample, font) in [("Section title: 11 semibold", fig::semi(11.0)), ("Value or name: 11 regular", fig::reg(11.0)), ("Layer or file name: 13 semibold", fig::semi(13.0)), ("Field label: 10 regular, secondary", fig::reg(10.0))] {
        let (r, _) = ui.allocate_exact_size(Vec2::new(420.0, 24.0), Sense::hover());
        fig::ptext(ui.painter(), Pos2::new(r.left(), r.center().y), Align2::LEFT_CENTER, sample, font, if sample.starts_with("Field") { TEXT_SECONDARY() } else { TEXT() });
    }
    caption(ui, "Radius: 2 small, 5 controls (fig::RADIUS), 13 popovers and the toolbar");
    let (r, _) = ui.allocate_exact_size(Vec2::new(420.0, 40.0), Sense::hover());
    for (k, (rad, label)) in [(R_SMALL, "2"), (R_MEDIUM, "5"), (R_LARGE, "13")].into_iter().enumerate() {
        let b = Rect::from_min_size(r.min + Vec2::new(k as f32 * 72.0, 0.0), Vec2::new(60.0, 40.0));
        ui.painter().rect_filled(b, rad, BG_SECONDARY());
        fig::ptext(ui.painter(), b.center(), Align2::CENTER_CENTER, label, fig::reg(11.0), TEXT());
    }
    caption(ui, "Spacing: rows 24, section header 40, 16px left and 8px right padding, two columns (W - 64) / 2 plus a 24px icon column, 16px before each divider");
    let (r, _) = ui.allocate_exact_size(Vec2::new(420.0, 56.0), Sense::hover());
    for (k, (w, label)) in [(24.0, "ROW 24"), (40.0, "HEADER 40"), (16.0, "16"), (8.0, "8")].into_iter().enumerate() {
        let b = Rect::from_min_size(r.min + Vec2::new(k as f32 * 100.0, 0.0), Vec2::new(w, w.min(40.0)));
        ui.painter().rect_filled(b, 2, BRAND.gamma_multiply(0.5));
        fig::ptext(ui.painter(), Pos2::new(b.left(), b.bottom() + 10.0), Align2::LEFT_CENTER, label, fig::reg(10.0), TEXT_SECONDARY());
    }
}

fn panel_specimens(ui: &mut egui::Ui, d: &mut Demo) {
    let g = Grid::of(ui.max_rect());
    // tabs
    let tabs = fig::row(ui, 32.0);
    let (a, wa) = fig::tab(ui, Pos2::new(tabs.left() + 8.0, tabs.top() + 4.0), "Design", d.tab);
    let (b, _) = fig::tab(ui, Pos2::new(tabs.left() + 8.0 + wa, tabs.top() + 4.0), "Prototype", !d.tab);
    if a.clicked() {
        d.tab = true;
    }
    if b.clicked() {
        d.tab = false;
    }
    fig::rule(ui);
    // a section with header actions
    let (_, acts) = fig::header(ui, "Section title", false, &[(Glyph::Plus, "Add", false), (Glyph::Eye, "Toggled icon button", true)]);
    let _ = acts;
    fig::labels(ui, &g, true, "Number fields", "");
    let r = fig::controls(ui, 0.0);
    fig::num_field(ui, g.col1(r), ui.id().with("g-w"), Prefix::Text("W"), &mut d.w, Num { decimals: 2, ..Num::default() }, "Width");
    fig::num_field(ui, g.col2(r), ui.id().with("g-h"), Prefix::Text("H"), &mut d.h, Num { decimals: 2, ..Num::default() }, "Height");
    if fig::icon_button(ui, g.icon(r), Glyph::Ratio, "Lock aspect ratio", false).clicked() {}
    fig::labels(ui, &g, false, "Prefix icon, suffix", "Dropdown");
    let r = fig::controls(ui, 0.0);
    fig::num_field(ui, g.col1(r), ui.id().with("g-o"), Prefix::Icon(Glyph::Contrast), &mut d.opacity, Num { min: 0.0, max: 100.0, decimals: 0, suffix: "%", speed: 1.0 }, "Opacity");
    let mut sizing = d.sizing;
    fig::dropdown(ui, g.col2(r), Some(Glyph::Frame), &mut sizing, &[(0u8, "Fixed"), (1, "Hug"), (2, "Fill")], "Sizing");
    d.sizing = sizing;
    fig::labels(ui, &g, false, "Segments and icon button", "");
    let r = fig::controls(ui, 0.0);
    if let Some(k) = fig::segments(ui, g.both(r), &[(Glyph::Frame, "Freeform", d.flow == 0), (Glyph::Rows, "Vertical", d.flow == 1), (Glyph::AutoLayout, "Horizontal", d.flow == 2), (Glyph::Grid, "Grid", d.flow == 3)]) {
        d.flow = k;
    }
    if fig::icon_button(ui, g.icon(r), Glyph::Wrap, "Wrap", d.wrap).clicked() {
        d.wrap ^= true;
    }
    let r = fig::controls(ui, 8.0);
    fig::checkbox(ui, g.col1(r), &mut d.check, "Checkbox");
    let r = fig::controls(ui, 8.0);
    fig::input_bg(ui, g.both(r), false);
    fig::swatch(ui, Pos2::new(g.left + 12.0, r.center().y), &mut d.color, "Swatch (opens the colour picker)");
    fig::text(ui, Pos2::new(g.left + 24.0, r.center().y), Align2::LEFT_CENTER, &hex(d.color).to_uppercase()[1..], fig::reg(11.0), TEXT());
    let r = fig::controls(ui, 8.0);
    fig::text_input(ui, g.both(r), &mut d.text, "Text input", "Text input");
    let r = fig::controls(ui, 8.0);
    fig::button(ui, g.col1(r), "Button");
    fig::end(ui);
    // list rows, notes, values, code
    fig::header(ui, "Grey title: empty section", true, &[(Glyph::Plus, "Add", false)]);
    fig::rule(ui);
    fig::value_row(ui, "Value row", "click to copy");
    fig::row(ui, 8.0);
    fig::outline_button(ui, 0.0, "Outline button");
    fig::row(ui, 8.0);
    fig::note(ui, Glyph::Sparkles, "A note: icon at 16, 11px text wrapped to the panel.");
    fig::row(ui, 8.0);
    fig::code_block(ui, "display: flex;\ngap: 10px;", "Code");
    fig::row(ui, 8.0);
    let r = fig::row(ui, 32.0);
    let menu = ui.interact(Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top() + 4.0), Vec2::new(120.0, 24.0)), ui.id().with("g-menu"), Sense::click());
    fig::input_bg(ui, menu.rect, menu.hovered());
    fig::ptext(ui.painter(), menu.rect.center(), Align2::CENTER_CENTER, "Open a menu", fig::reg(11.0), TEXT());
    egui::Popup::menu(&menu).show(|ui| {
        menu_item(ui, Some(Glyph::Copy), "Menu item", "Ctrl+C", true);
        menu_item(ui, Some(Glyph::Delete), "Another item", "", true);
        menu_item(ui, None, "Disabled", "", false);
    });
    fig::row(ui, 8.0);
}

fn icons(ui: &mut egui::Ui) {
    let all: Vec<(Glyph, &str)> = crate::glyphs::all_glyphs();
    ui.set_width(360.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(4.0);
        for (g, name) in all {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
            ui.painter().rect_filled(r, R_MEDIUM, BG_SECONDARY());
            paint_glyph_sized(ui.painter(), r.center(), g, TEXT(), 16.0);
            resp.tip(name);
        }
    });
}
