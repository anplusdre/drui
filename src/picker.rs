// Figma's color picker, measured from the desktop app (126.9.11, 150% DPI, values in CSS px):
// a 240px popover on #2c2c2c with 13px corners. A 40px header (a "Custom" / "Libraries" pill tab at 8,
// + and close as 24px buttons 8px from the right, 28px apart), for paints a 40px row of paint types (24px
// buttons from x 8, 28px apart) and a divider, then 16px down a 208px saturation/brightness square inset
// 16px; 12px down a 16px hue slider from x 52 to 224 with the eyedropper centred at x 28 between it and the
// 16px opacity slider 12px below; 12px down a 24px row: the format dropdown (x 16, 53 wide), the value
// field (x 80, 90 wide) and the opacity field (x 170, 54 wide, "%"); 16px, a divider, 12px, the swatch set
// dropdown (24px, outlined), 12px, then 16px swatches 24px apart, 9 to a row.
use crate::fig::Tip as _;
use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Vec2};

use crate::fig::{self, Num, Prefix};
use crate::glyphs::{paint_glyph, paint_glyph_sized, Glyph};
use crate::{BG, TEXT, TEXT_SECONDARY, TEXT_TERTIARY};

pub const W: f32 = 240.0;
const SQUARE: f32 = 208.0;
const SLIDER_X: f32 = 52.0;
const SLIDER_H: f32 = 16.0;
const CHECKER: f32 = 16.0 / 3.0; // 8 device px at 150%
const CHECK_A: Color32 = Color32::from_rgb(0x2c, 0x2c, 0x2c); // the transparency checkerboard stays grey in either theme
const CHECK_B: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x2b);

// the colors used on the current page, for the "On this page" swatches; the app stores them each frame
pub fn page_colors_id() -> Id {
    Id::new("page-colors")
}

const EYE_ID: Id = Id::NULL;

// the eyedropper of the colour picker: aiming, or waiting for the screenshot taken when the pointer went down at that point
#[derive(Clone, Copy)]
enum EyeState {
    Aim,
    Wait(Pos2),
}

// is the eyedropper aiming? (the canvas and the popover then leave clicks alone, so the click only picks)
pub fn eyedropper_active(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<EyeState>(EYE_ID)).is_some()
}

// the popover's frame
pub fn frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(BG())
        .corner_radius(crate::R_LARGE)
        .stroke(Stroke::new(1.0, Color32::from_gray(0x44)))
        .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(90) })
}

// a popup for `resp` placed like Figma's: to the left of the panel `panel_left`, top-aligned with the row, moved up when it
// would run off the bottom of the window (its height is remembered from the previous frame)
pub fn popup<R>(resp: &egui::Response, panel_left: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> Option<R> {
    let ctx = resp.ctx.clone();
    let screen = ctx.content_rect();
    let hid = resp.id.with("popover-height");
    let h = ctx.data(|d| d.get_temp::<f32>(hid)).unwrap_or(0.0);
    let y = resp.rect.top().min(screen.bottom() - h - 8.0).max(screen.top() + 8.0);
    egui::Popup::menu(resp)
        .close_behavior(if eyedropper_active(&ctx) { egui::PopupCloseBehavior::IgnoreClicks } else { egui::PopupCloseBehavior::CloseOnClickOutside })
        .anchor(egui::PopupAnchor::Position(Pos2::new(panel_left - 8.0, y)))
        .align(egui::RectAlign::LEFT_START)
        .gap(0.0)
        .frame(frame())
        .width(W)
        .show(|ui| {
            ui.set_width(W);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let out = add(ui);
            let used = ui.min_rect().height() + 2.0;
            ctx.data_mut(|d| d.insert_temp(hid, used));
            out
        })
        .map(|r| r.inner)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Custom,
    Libraries,
}

// the 40px header: the tabs (Libraries only when `libraries`), + when `plus` is given, and close.
// Returns (+ clicked, close clicked).
pub fn header(ui: &mut egui::Ui, tab: &mut Tab, libraries: bool, plus: Option<&str>) -> (bool, bool) {
    let r = fig::row(ui, 40.0);
    let mut x = r.left() + 8.0;
    let tabs: &[(Tab, &str)] = if libraries { &[(Tab::Custom, "Custom"), (Tab::Libraries, "Libraries")] } else { &[(Tab::Custom, "Custom")] };
    for &(t, label) in tabs {
        let (resp, w) = fig::tab(ui, Pos2::new(x, r.top() + 8.0), label, *tab == t);
        if resp.clicked() {
            *tab = t;
        }
        x += w;
    }
    let close_r = Rect::from_min_size(Pos2::new(r.right() - 32.0, r.top() + 8.0), Vec2::splat(fig::ROW));
    let close = fig::icon_button(ui, close_r, Glyph::Close, "Close", false).clicked();
    let add = plus.is_some_and(|tip| fig::icon_button(ui, close_r.translate(Vec2::new(-28.0, 0.0)), Glyph::Plus, tip, false).clicked());
    (add, close)
}

// the 40px row of paint types and the divider under it; returns the clicked index
pub fn types(ui: &mut egui::Ui, items: &[(Glyph, &str, bool)]) -> Option<usize> {
    let r = fig::row(ui, 40.0);
    let mut hit = None;
    for (k, (g, tip, on)) in items.iter().enumerate() {
        let b = Rect::from_min_size(Pos2::new(r.left() + 8.0 + 28.0 * k as f32, r.top() + 8.0), Vec2::splat(fig::ROW));
        if fig::icon_button(ui, b, *g, tip, *on).clicked() {
            hit = Some(k);
        }
    }
    fig::rule(ui);
    hit
}

// HSV on sRGB bytes, as Figma's HSB: h, s, v, a all 0..1
fn to_hsv(c: Color32) -> [f32; 4] {
    let [r, g, b, a] = c.to_srgba_unmultiplied().map(|x| x as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, if max == 0.0 { 0.0 } else { d / max }, max, a]
}

fn rgb_of(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let (r, g, b) = match h6 as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

fn from_hsv(hsv: [f32; 4]) -> Color32 {
    let [r, g, b] = rgb_of(hsv[0], hsv[1], hsv[2]).map(|x| (x * 255.0).round() as u8);
    Color32::from_rgba_unmultiplied(r, g, b, (hsv[3] * 255.0).round() as u8)
}

fn opaque(c: Color32) -> Color32 {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    Color32::from_rgb(r, g, b)
}

// a slider track: rounded ends, `fill(t)` across it
fn track(p: &egui::Painter, r: Rect, fill: impl Fn(f32) -> Color32, checker: bool) {
    let rad = r.height() / 2.0;
    if checker {
        let clip = p.with_clip_rect(r);
        let n = (r.width() / CHECKER).ceil() as i32;
        for i in 0..n {
            for j in 0..3 {
                let cell = Rect::from_min_size(Pos2::new(r.left() + i as f32 * CHECKER, r.top() + j as f32 * CHECKER), Vec2::splat(CHECKER));
                clip.rect_filled(cell, 0.0, if (i + j) % 2 == 0 { CHECK_A } else { CHECK_B });
            }
        }
        // round the ends off by painting the panel back over the corners
        for (cx, side) in [(r.left() + rad, -1.0), (r.right() - rad, 1.0)] {
            let corner = Rect::from_min_max(Pos2::new(if side < 0.0 { r.left() } else { cx }, r.top()), Pos2::new(if side < 0.0 { cx } else { r.right() }, r.bottom()));
            let mut mesh = egui::Mesh::default();
            let steps = 12;
            let c = Pos2::new(cx, r.center().y);
            for k in 0..=steps {
                let a = std::f32::consts::FRAC_PI_2 * (k as f32 / steps as f32 * 2.0 - 1.0);
                let rim = c + Vec2::new(side * a.cos() * rad, a.sin() * rad);
                let edge = Pos2::new(if side < 0.0 { corner.left() } else { corner.right() }, rim.y);
                mesh.colored_vertex(rim, BG());
                mesh.colored_vertex(edge, BG());
                if k > 0 {
                    let b = (k * 2) as u32;
                    mesh.add_triangle(b - 2, b - 1, b);
                    mesh.add_triangle(b - 1, b, b + 1);
                }
            }
            p.add(mesh);
        }
    }
    // the end caps are plain circles painted first (a polygon cap would feather its straight edge into a
    // visible seam), then the gradient as a strip of quads between their centres covers their inner halves
    let inner = Rect::from_min_max(Pos2::new(r.left() + rad, r.top()), Pos2::new(r.right() - rad, r.bottom()));
    p.circle_filled(Pos2::new(inner.left(), r.center().y), rad, fill(0.0));
    p.circle_filled(Pos2::new(inner.right(), r.center().y), rad, fill(1.0));
    let mut mesh = egui::Mesh::default();
    let n = 36;
    for k in 0..=n {
        let t = k as f32 / n as f32;
        let x = inner.left() + t * inner.width();
        let c = fill(t);
        mesh.colored_vertex(Pos2::new(x, r.top()), c);
        mesh.colored_vertex(Pos2::new(x, r.bottom()), c);
        if k > 0 {
            let b = (k * 2) as u32;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b, b + 1);
        }
    }
    p.add(mesh);
}

// the round thumb Figma draws on the square and the sliders: 16px, a white ring around the color
fn thumb(p: &egui::Painter, c: Pos2, color: Color32) {
    p.circle_filled(c + Vec2::new(0.0, 0.5), 8.5, Color32::from_black_alpha(60));
    p.circle_filled(c, 8.0, Color32::WHITE);
    p.circle_filled(c, 5.0, color);
}

#[derive(Clone, Copy, PartialEq)]
enum Format {
    Hex,
    Rgb,
    Hsb,
}

// everything under the header and type row: square, sliders, value fields and swatches.
// `id` must be stable for this color; true when the color changed.
pub fn body(ui: &mut egui::Ui, c: &mut Color32, id: Id) -> bool {
    let before = *c;
    // the HSV being edited: kept between frames so hue survives greys and black
    let hsv_id = id.with("hsv");
    let mut hsv = match ui.data(|d| d.get_temp::<[f32; 4]>(hsv_id)) {
        Some(h) if from_hsv(h) == *c => h,
        Some(h) => {
            let mut n = to_hsv(*c);
            if n[1] == 0.0 || n[2] == 0.0 {
                n[0] = h[0];
            }
            if n[2] == 0.0 {
                n[1] = h[1];
            }
            n
        }
        None => to_hsv(*c),
    };
    let start = hsv;

    // saturation / brightness
    fig::row(ui, 16.0);
    let row = fig::row(ui, SQUARE);
    let sq = Rect::from_min_size(Pos2::new(row.left() + 16.0, row.top()), Vec2::splat(SQUARE));
    let resp = ui.interact(sq.expand(6.0), id.with("sv"), Sense::click_and_drag());
    if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on()) {
        hsv[1] = ((p.x - sq.left()) / sq.width()).clamp(0.0, 1.0);
        hsv[2] = 1.0 - ((p.y - sq.top()) / sq.height()).clamp(0.0, 1.0);
    }
    {
        let p = ui.painter();
        let n = 16;
        let mut mesh = egui::Mesh::default();
        for j in 0..=n {
            for i in 0..=n {
                let (s, v) = (i as f32 / n as f32, 1.0 - j as f32 / n as f32);
                let [r, g, b] = rgb_of(hsv[0], s, v).map(|x| (x * 255.0).round() as u8);
                mesh.colored_vertex(Pos2::new(sq.left() + s * sq.width(), sq.top() + j as f32 / n as f32 * sq.height()), Color32::from_rgb(r, g, b));
                if i > 0 && j > 0 {
                    let k = (j * (n + 1) + i) as u32;
                    let w = (n + 1) as u32;
                    mesh.add_triangle(k - w - 1, k - w, k);
                    mesh.add_triangle(k - w - 1, k - 1, k);
                }
            }
        }
        p.add(mesh);
        p.rect_stroke(sq, 2, Stroke::new(1.0, Color32::from_black_alpha(25)), egui::StrokeKind::Inside);
        thumb(&p.with_clip_rect(sq.expand(9.0)), Pos2::new(sq.left() + hsv[1] * sq.width(), sq.top() + (1.0 - hsv[2]) * sq.height()), opaque(from_hsv(hsv)));
    }
    let (s100, v100) = ((hsv[1] * 100.0).round(), (hsv[2] * 100.0).round());
    resp.widget_info(|| egui::WidgetInfo::slider(true, s100 as f64, format!("Color picker: saturation {s100}%, brightness {v100}%")));

    // hue and opacity, the eyedropper between them on the left
    let slider = |ui: &mut egui::Ui, salt: &str| -> (Rect, egui::Response) {
        let row = fig::row(ui, SLIDER_H);
        let r = Rect::from_min_max(Pos2::new(row.left() + SLIDER_X, row.top()), Pos2::new(row.left() + W - 16.0, row.bottom()));
        let resp = ui.interact(r.expand2(Vec2::new(4.0, 2.0)), id.with(salt), Sense::click_and_drag());
        (r, resp)
    };
    let pos_t = |r: Rect, x: f32| ((x - r.left() - r.height() / 2.0) / (r.width() - r.height())).clamp(0.0, 1.0);
    let at_t = |r: Rect, t: f32| Pos2::new(r.left() + r.height() / 2.0 + t * (r.width() - r.height()), r.center().y);
    fig::row(ui, 12.0);
    let (hr, hresp) = slider(ui, "hue");
    if let Some(p) = hresp.interact_pointer_pos().filter(|_| hresp.dragged() || hresp.clicked() || hresp.is_pointer_button_down_on()) {
        hsv[0] = pos_t(hr, p.x).min(0.9999);
    }
    track(ui.painter(), hr, |t| from_hsv([t, 1.0, 1.0, 1.0]), false);
    thumb(ui.painter(), at_t(hr, hsv[0]), from_hsv([hsv[0], 1.0, 1.0, 1.0]));
    let deg = (hsv[0] * 360.0).round();
    hresp.widget_info(|| egui::WidgetInfo::slider(true, deg as f64, "Hue"));
    fig::row(ui, 12.0);
    let (or, oresp) = slider(ui, "alpha");
    if let Some(p) = oresp.interact_pointer_pos().filter(|_| oresp.dragged() || oresp.clicked() || oresp.is_pointer_button_down_on()) {
        hsv[3] = pos_t(or, p.x);
    }
    let solid = opaque(from_hsv(hsv));
    track(ui.painter(), or, |t| solid.gamma_multiply(t), true);
    thumb(ui.painter(), at_t(or, hsv[3]), from_hsv(hsv));
    let pct = (hsv[3] * 100.0).round();
    oresp.widget_info(|| egui::WidgetInfo::slider(true, pct as f64, "Opacity"));
    // the eyedropper: press it, then click anywhere in the window; the colour under the pointer is read from a screenshot of the window
    let eye = Rect::from_center_size(Pos2::new(hr.left() - SLIDER_X + 28.0, (hr.center().y + or.center().y) / 2.0), Vec2::splat(fig::ROW));
    let e = ui.interact(eye, id.with("eyedropper"), Sense::click());
    let state = ui.data(|d| d.get_temp::<EyeState>(EYE_ID));
    if state.is_some() {
        ui.painter().rect_filled(eye, fig::RADIUS, fig::SELECTED());
    } else if e.hovered() {
        ui.painter().rect_filled(eye, fig::RADIUS, fig::INPUT_HOVER());
    }
    paint_glyph(ui.painter(), eye.center(), Glyph::Pipette, if state.is_some() { fig::SELECTED_INK() } else { TEXT() });
    e.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Sample color"));
    if e.clicked() {
        ui.data_mut(|d| {
            if state.is_some() {
                d.remove::<EyeState>(EYE_ID);
            } else {
                d.insert_temp(EYE_ID, EyeState::Aim);
            }
        });
    } else if let Some(state) = state {
        let ctx = ui.ctx().clone();
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        match state {
            EyeState::Aim => {
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    ui.data_mut(|d| d.remove::<EyeState>(EYE_ID));
                } else if let Some(p) = ctx.input(|i| if i.pointer.primary_pressed() { i.pointer.interact_pos() } else { None }) {
                    ui.data_mut(|d| d.insert_temp(EYE_ID, EyeState::Wait(p)));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                }
            }
            EyeState::Wait(p) => {
                let shot = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
                if let Some(img) = shot {
                    let k = ctx.pixels_per_point();
                    let (x, y) = (((p.x * k) as usize).min(img.size[0].saturating_sub(1)), ((p.y * k) as usize).min(img.size[1].saturating_sub(1)));
                    let c = img.pixels[y * img.size[0] + x];
                    let keep_alpha = hsv[3];
                    hsv = to_hsv(c);
                    hsv[3] = keep_alpha;
                    ui.data_mut(|d| d.remove::<EyeState>(EYE_ID));
                }
            }
        }
    }

    // the value row
    fig::row(ui, 12.0);
    let row = fig::row(ui, fig::ROW);
    let fmt_id = Id::new("picker-format");
    let mut format = ui.data(|d| d.get_temp::<Format>(fmt_id)).unwrap_or(Format::Hex);
    let fr = Rect::from_min_size(Pos2::new(row.left() + 16.0, row.top()), Vec2::new(53.0, fig::ROW));
    if format_dropdown(ui, fr, &mut format) {
        ui.data_mut(|d| d.insert_temp(fmt_id, format));
    }
    let vr = Rect::from_min_size(Pos2::new(row.left() + 80.0, row.top()), Vec2::new(90.0, fig::ROW));
    let opr = Rect::from_min_size(Pos2::new(vr.right(), row.top()), Vec2::new(54.0, fig::ROW));
    let mut now = from_hsv(hsv);
    match format {
        Format::Hex => {
            ui.painter().rect_filled(vr, CornerRadius { nw: fig::RADIUS, sw: fig::RADIUS, ne: 0, se: 0 }, fig::INPUT());
            if fig::hex_text(ui, Pos2::new(vr.left() + 8.0, vr.center().y), vr.width() - 12.0, &mut now, id.with("hex")) {
                hsv = to_hsv(now);
            }
        }
        Format::Rgb | Format::Hsb => {
            let w = vr.width() / 3.0;
            let mut vals = if format == Format::Rgb { now.to_srgba_unmultiplied().map(|x| x as f32) } else { [(hsv[0] * 360.0).round(), (hsv[1] * 100.0).round(), (hsv[2] * 100.0).round(), 0.0] };
            let names = if format == Format::Rgb { ["Red", "Green", "Blue"] } else { ["Hue", "Saturation", "Brightness"] };
            let maxes = if format == Format::Rgb { [255.0; 3] } else { [360.0, 100.0, 100.0] };
            let mut changed = false;
            for k in 0..3 {
                let cell = Rect::from_min_size(Pos2::new(vr.left() + w * k as f32, vr.top()), Vec2::new(w, fig::ROW));
                changed |= fig::num_field(ui, cell, id.with(("chan", k)), Prefix::None, &mut vals[k], Num { min: 0.0, max: maxes[k], decimals: 0, speed: 1.0, suffix: "" }, names[k]);
                if k > 0 {
                    ui.painter().rect_filled(Rect::from_min_size(cell.min, Vec2::new(1.0, fig::ROW)), 0.0, BG());
                }
            }
            if changed {
                hsv = if format == Format::Rgb {
                    to_hsv(Color32::from_rgba_unmultiplied(vals[0] as u8, vals[1] as u8, vals[2] as u8, (hsv[3] * 255.0).round() as u8))
                } else {
                    [vals[0] / 360.0, vals[1] / 100.0, vals[2] / 100.0, hsv[3]]
                };
            }
        }
    }
    let mut pct = hsv[3] * 100.0;
    if fig::num_field(ui, opr, id.with("opacity"), Prefix::None, &mut pct, Num { min: 0.0, max: 100.0, decimals: 0, speed: 1.0, suffix: "" }, "Opacity percent") {
        hsv[3] = pct / 100.0;
    }
    ui.painter().rect_filled(Rect::from_min_size(opr.min, Vec2::new(1.0, fig::ROW)), 0.0, BG());
    fig::text(ui, Pos2::new(opr.right() - 8.0, opr.center().y), Align2::RIGHT_CENTER, "%", fig::reg(11.0), TEXT());

    // swatches of this page
    fig::row(ui, 16.0);
    fig::rule(ui);
    fig::row(ui, 10.0);
    let row = fig::row(ui, fig::ROW);
    let set = Rect::from_min_max(Pos2::new(row.left() + 16.0, row.top()), Pos2::new(row.left() + W - 16.0, row.bottom()));
    ui.painter().rect_stroke(set, fig::RADIUS, Stroke::new(1.0, fig::RULE()), egui::StrokeKind::Inside);
    fig::text(ui, Pos2::new(set.left() + 8.0, set.center().y), Align2::LEFT_CENTER, "On this page", fig::reg(11.0), TEXT());
    paint_glyph_sized(ui.painter(), Pos2::new(set.right() - 14.0, set.center().y), Glyph::ChevronDown, TEXT(), 9.5);
    ui.interact(set, id.with("swatch-set"), Sense::hover()).widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Color swatch set selector");
        info.current_text_value = Some("On this page".into());
        info
    });
    fig::row(ui, 8.0);
    let colors: Vec<Color32> = ui.ctx().data(|d| d.get_temp(page_colors_id())).unwrap_or_default();
    if colors.is_empty() {
        let r = fig::row(ui, fig::ROW);
        fig::text(ui, Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "No colors on this page yet", fig::reg(11.0), TEXT_SECONDARY());
    }
    for line in colors.chunks(9) {
        let r = fig::row(ui, 24.0);
        for (k, &col) in line.iter().enumerate() {
            let sw = Rect::from_min_size(Pos2::new(r.left() + 16.0 + 24.0 * k as f32, r.top()), Vec2::splat(16.0));
            let resp = ui.interact(sw, id.with(("page-color", col.to_array())), Sense::click());
            ui.painter().rect_filled(sw, 2, col);
            ui.painter().rect_stroke(sw, 2, Stroke::new(1.0, if resp.hovered() { TEXT() } else { Color32::from_white_alpha(20) }), egui::StrokeKind::Inside);
            let hex = format!("{:02X}{:02X}{:02X}", col.r(), col.g(), col.b());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Solid color hex: {hex}")));
            if resp.clicked() {
                let a = hsv[3];
                hsv = to_hsv(col);
                hsv[3] = a;
            }
        }
    }
    fig::row(ui, 8.0);

    if hsv != start {
        *c = from_hsv(hsv);
    }
    ui.data_mut(|d| d.insert_temp(hsv_id, hsv));
    *c != before
}

// the color format menu: "Hex" and a chevron, no fill until hovered
fn format_dropdown(ui: &mut egui::Ui, r: Rect, cur: &mut Format) -> bool {
    let items = [(Format::Hex, "Hex"), (Format::Rgb, "RGB"), (Format::Hsb, "HSB")];
    let resp = ui.interact(r, ui.id().with("color-format"), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, fig::RADIUS, fig::INPUT());
    }
    let label = items.iter().find(|i| i.0 == *cur).map_or("Hex", |i| i.1);
    fig::text(ui, Pos2::new(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, label, fig::reg(11.0), TEXT());
    paint_glyph_sized(ui.painter(), Pos2::new(r.left() + 39.0, r.center().y), Glyph::ChevronDown, TEXT(), 9.5);
    resp.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Color format");
        info.current_text_value = Some(label.to_owned());
        info
    });
    let mut changed = false;
    egui::Popup::menu(&resp).show(|ui| {
        for (v, l) in items {
            if crate::glyphs::menu_option(ui, false, l) {
                changed = *cur != v;
                *cur = v;
                ui.close();
            }
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips() {
        for c in [Color32::from_rgb(255, 0, 0), Color32::from_rgb(12, 140, 233), Color32::from_rgba_unmultiplied(239, 199, 94, 128), Color32::BLACK, Color32::WHITE] {
            assert_eq!(from_hsv(to_hsv(c)), c);
        }
    }
}

// ---- paint types, gradients and images (Figma 126.9.13, measured; CSS px from the popover's top-left) ----
// The type row has six paints (Solid, Gradient, Pattern, Image, Video, Shader); the last three do not exist in drui and are
// shown greyed. A gradient is: 12px, a 24px row (type dropdown at x 16 / 96 wide, flip at x 180, rotate at x 208), 28px
// for the stop pins, a 32px bar (x 16..224), 4px, a 40px "Stops" row with +, then 32px stop rows (position field x 16 / 48
// wide, colour+opacity field x 72..200, remove at x 208) and 12px. An image is: 12px, a 24px row (scale mode dropdown at
// x 16, rotate at x 208), 12px, the 208px preview (x 16) with "Upload from computer", then seven 32px slider rows (label x
// 16, a 128px track from x 100).

use crate::model::{PatternFill, ShaderFill, ShaderKind};
use crate::model::{Asset, Gradient, GradientKind, GradStop, ImageFill, ImageMode};

// the row of paint types and the divider under it; `which` is the selected index (0 solid, 1 gradient, 3 image).
// Returns the chosen index among the supported ones.
// Figma also has two buttons at the right end (apply a style, apply a variable; centred 190.5 and 218.5 from the popover's left);
// drui lists both under Libraries, so either one asks for that tab: the second value is true when one was pressed.
pub fn paint_types(ui: &mut egui::Ui, which: usize) -> (Option<usize>, bool) {
    let r = fig::row(ui, 40.0);
    let items: [(Glyph, &str, bool); 6] = [(Glyph::Rect, "Solid", true), (Glyph::Gradient, "Gradient", true), (Glyph::Grid, "Pattern", true), (Glyph::Image, "Image", true), (Glyph::Present, "Video", crate::video::ffmpeg().is_some()), (Glyph::Waves, "Shader", true)];
    let mut hit = None;
    for (k, (g, tip, enabled)) in items.iter().enumerate() {
        let b = Rect::from_min_size(Pos2::new(r.left() + 8.0 + 28.0 * k as f32, r.top() + 8.0), Vec2::splat(fig::ROW));
        if *enabled {
            if fig::icon_button(ui, b, *g, tip, which == k).clicked() {
                hit = Some(k);
            }
        } else {
            let resp = ui.interact(b, ui.id().with(("paint-type", k)), Sense::hover());
            paint_glyph(ui.painter(), b.center(), *g, TEXT_TERTIARY());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, false, format!("{tip} (not available)")));
            resp.tip(format!("{tip} paints need ffmpeg: put ffmpeg next to drui, or set DRUI_FFMPEG"));
        }
    }
    let mut libraries = false;
    for (cx, g, tip) in [(190.5, Glyph::Droplet, "Apply style"), (218.5, Glyph::Variable, "Apply variable")] {
        libraries |= fig::icon_button(ui, Rect::from_center_size(Pos2::new(r.left() + cx, r.top() + 20.0), Vec2::splat(fig::ROW)), g, tip, false).clicked();
    }
    fig::rule(ui);
    (hit, libraries)
}

// the checkerboard Figma shows under transparency
fn checker(p: &egui::Painter, r: Rect, cell: f32) {
    let clip = p.with_clip_rect(r);
    let (nx, ny) = ((r.width() / cell).ceil() as i32, (r.height() / cell).ceil() as i32);
    clip.rect_filled(r, 0.0, CHECK_B);
    for i in 0..nx {
        for j in 0..ny {
            if (i + j) % 2 == 0 {
                clip.rect_filled(Rect::from_min_size(Pos2::new(r.left() + i as f32 * cell, r.top() + j as f32 * cell), Vec2::splat(cell)), 0.0, CHECK_A);
            }
        }
    }
}

// paints the panel colour over the corners of `r` so a rectangle looks rounded
fn round_corners(p: &egui::Painter, r: Rect, rad: f32) {
    for (cx, cy, sx, sy) in [(r.left() + rad, r.top() + rad, -1.0, -1.0), (r.right() - rad, r.top() + rad, 1.0, -1.0), (r.right() - rad, r.bottom() - rad, 1.0, 1.0), (r.left() + rad, r.bottom() - rad, -1.0, 1.0)] {
        let corner = Pos2::new(cx + sx * rad, cy + sy * rad);
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(corner, BG());
        let steps = 8;
        for k in 0..=steps {
            let a = std::f32::consts::FRAC_PI_2 * k as f32 / steps as f32;
            // the arc runs from the horizontal edge to the vertical edge of this corner
            let (dx, dy) = (a.cos() * sx, a.sin() * sy);
            mesh.colored_vertex(Pos2::new(cx + dx * rad, cy + dy * rad), BG());
            if k > 0 {
                mesh.add_triangle(0, k as u32, k as u32 + 1);
            }
        }
        p.add(mesh);
    }
}

// a stop's pin above the gradient bar: a dark rounded square holding the colour, a pointer down to the bar; the selected pin
// has a blue outline
fn pin(p: &egui::Painter, x: f32, bar_top: f32, color: Color32, selected: bool) -> Rect {
    let body = Rect::from_center_size(Pos2::new(x, bar_top - 16.0), Vec2::new(22.0, 20.0));
    let tri = vec![Pos2::new(x - 5.0, body.bottom() - 1.0), Pos2::new(x + 5.0, body.bottom() - 1.0), Pos2::new(x, bar_top + 1.0)];
    let fill = Color32::from_gray(0x38);
    p.rect_filled(body, 5, fill);
    p.add(egui::Shape::convex_polygon(tri.clone(), fill, Stroke::NONE));
    if selected {
        p.rect_stroke(body, 5, Stroke::new(2.0, fig::SHARE), egui::StrokeKind::Outside);
        p.add(egui::Shape::line(vec![Pos2::new(x - 6.0, body.bottom() + 1.0), Pos2::new(x, bar_top + 2.0), Pos2::new(x + 6.0, body.bottom() + 1.0)], Stroke::new(2.0, fig::SHARE)));
    }
    let sw = Rect::from_center_size(body.center(), Vec2::splat(12.0));
    checker(&p.with_clip_rect(sw), sw, 4.0);
    p.rect_filled(sw, 2, color);
    p.rect_stroke(sw, 2, Stroke::new(1.0, Color32::from_white_alpha(30)), egui::StrokeKind::Inside);
    body.union(Rect::from_min_max(Pos2::new(x - 6.0, body.bottom()), Pos2::new(x + 6.0, bar_top + 2.0)))
}

// Figma's gradient tab: the type, flip and rotate, the bar with its stops, and the list of stops. True when it changed anything.
pub fn gradient_editor(ui: &mut egui::Ui, g: &mut Gradient, id: Id, panel_left: f32) -> bool {
    let before = g.clone();
    fig::row(ui, 12.0);
    let r = fig::row(ui, fig::ROW);
    fig::dropdown(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top()), Vec2::new(96.0, fig::ROW)), None, &mut g.kind, &GradientKind::ALL, "Gradient type");
    if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.left() + 180.0, r.top()), Vec2::splat(fig::ROW)), Glyph::Swap, "Flip gradient", false).clicked() {
        for s in &mut g.stops {
            s.pos = 1.0 - s.pos;
        }
        g.stops.reverse();
        g.selected = g.stops.len().saturating_sub(1).saturating_sub(g.selected);
    }
    if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.left() + 208.0, r.top()), Vec2::splat(fig::ROW)), Glyph::RotateCw, "Rotate gradient", false).clicked() {
        g.angle = (g.angle + 90.0).rem_euclid(360.0);
    }
    // the bar, with a pin for every stop
    fig::row(ui, 28.0);
    let row = fig::row(ui, 32.0);
    let bar = Rect::from_min_size(Pos2::new(row.left() + 16.0, row.top()), Vec2::new(208.0, 32.0));
    {
        let p = ui.painter();
        checker(p, bar, 8.0);
        // the gradient as vertical strips; the end strips follow the corner arcs
        let rad = 5.0;
        let n = 72;
        let mut mesh = egui::Mesh::default();
        for k in 0..=n {
            let t = k as f32 / n as f32;
            let x = bar.left() + t * bar.width();
            let edge = (x - bar.left()).min(bar.right() - x);
            let inset = if edge < rad { rad - (rad * rad - (rad - edge) * (rad - edge)).max(0.0).sqrt() } else { 0.0 };
            let c = g.at(t);
            mesh.colored_vertex(Pos2::new(x, bar.top() + inset), c);
            mesh.colored_vertex(Pos2::new(x, bar.bottom() - inset), c);
            if k > 0 {
                let b = (k * 2) as u32;
                mesh.add_triangle(b - 2, b - 1, b);
                mesh.add_triangle(b - 1, b, b + 1);
            }
        }
        round_corners(p, bar, rad);
        p.add(mesh);
        p.rect_stroke(bar, 5, Stroke::new(1.0, Color32::from_gray(0x45)), egui::StrokeKind::Inside);
    }
    let bar_resp = ui.interact(bar, id.with("bar"), Sense::click());
    let mut remove = None;
    let mut grab = None;
    for k in 0..g.stops.len() {
        let x = bar.left() + g.stops[k].pos.clamp(0.0, 1.0) * bar.width();
        let hit = pin(ui.painter(), x, bar.top(), g.stops[k].color, g.selected == k);
        let resp = ui.interact(hit.expand(1.0), id.with(("pin", k)), Sense::click_and_drag());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, g.selected == k, format!("Gradient stop {}", k + 1)));
        if resp.clicked() || resp.drag_started() {
            grab = Some(k);
        }
        if resp.dragged() {
            if let Some(p) = resp.interact_pointer_pos() {
                g.stops[k].pos = ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
                g.selected = k;
                // dragging a stop well away from the bar takes it off
                if (p.y - bar.top()).abs() > 80.0 && g.stops.len() > 2 {
                    remove = Some(k);
                }
            }
        }
    }
    if let Some(k) = grab {
        g.selected = k;
    }
    if bar_resp.clicked() {
        if let Some(p) = bar_resp.interact_pointer_pos() {
            let t = ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
            let color = g.at(t);
            g.stops.push(GradStop { pos: t, color });
            g.selected = g.stops.len() - 1;
            g.sort();
        }
    }
    if let Some(k) = remove {
        g.stops.remove(k);
        g.selected = g.selected.min(g.stops.len() - 1);
    }
    g.sort();
    // the list of stops
    fig::row(ui, 4.0);
    let head = fig::row(ui, 40.0);
    fig::text(ui, Pos2::new(head.left() + 16.0, head.center().y), Align2::LEFT_CENTER, "Stops", fig::semi(11.0), TEXT());
    if fig::icon_button(ui, Rect::from_center_size(Pos2::new(head.left() + 220.0, head.center().y), Vec2::splat(fig::ROW)), Glyph::Plus, "Add gradient stop", false).clicked() {
        // a new stop halfway between the selected one and the next (or the previous at the end)
        let k = g.selected.min(g.stops.len() - 1);
        let other = if k + 1 < g.stops.len() { k + 1 } else { k.saturating_sub(1) };
        let t = (g.stops[k].pos + g.stops[other].pos) / 2.0;
        let color = g.at(t);
        g.stops.push(GradStop { pos: t, color });
        g.selected = g.stops.len() - 1;
        g.sort();
    }
    let mut kill = None;
    let mut pick = None;
    for k in 0..g.stops.len() {
        let row = fig::row(ui, 32.0);
        if g.selected == k {
            ui.painter().rect_filled(row, 0.0, fig::SELECTED());
        }
        let band = ui.interact(row, id.with(("row", k)), Sense::click());
        if band.clicked() {
            pick = Some(k);
        }
        let y = row.top() + 4.0;
        let pos_f = Rect::from_min_size(Pos2::new(row.left() + 16.0, y), Vec2::new(48.0, 24.0));
        let mut pct = (g.stops[k].pos * 100.0).round();
        if fig::num_field(ui, pos_f, id.with(("pos", k)), Prefix::None, &mut pct, Num { min: 0.0, max: 100.0, decimals: 0, suffix: "%", speed: 1.0 }, &format!("Gradient stop {} position", k + 1)) {
            g.stops[k].pos = pct / 100.0;
            g.selected = k;
        }
        // colour and opacity in one field, like a paint row
        let field = Rect::from_min_size(Pos2::new(row.left() + 72.0, y), Vec2::new(128.0, 24.0));
        fig::input_bg(ui, field, false);
        let sw = Rect::from_center_size(Pos2::new(field.left() + 14.0, field.center().y), Vec2::splat(14.0));
        let resp = ui.interact(sw.expand(3.0), id.with(("stop-swatch", k)), Sense::click());
        checker(&ui.painter().with_clip_rect(sw), sw, 3.5);
        ui.painter().rect_filled(sw, 2, g.stops[k].color);
        ui.painter().rect_stroke(sw, 2, Stroke::new(1.0, Color32::from_black_alpha(50)), egui::StrokeKind::Inside);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ColorButton, true, format!("Gradient stop {} color", k + 1)));
        if resp.clicked() {
            pick = Some(k);
        }
        let mut c = g.stops[k].color;
        popup(&resp, panel_left, |ui| {
            let mut tab = Tab::Custom;
            if header(ui, &mut tab, false, None).1 {
                ui.close();
            }
            fig::rule(ui);
            body(ui, &mut c, id.with(("stop-color", k)));
        });
        g.stops[k].color = c;
        let op_w = 48.0;
        let hex_w = field.width() - 28.0 - op_w;
        let mut rgb = g.stops[k].color;
        if fig::hex_text(ui, Pos2::new(field.left() + 28.0, field.center().y), hex_w, &mut rgb, id.with(("hex", k))) {
            let a = g.stops[k].color.a();
            let [r, gr, b, _] = rgb.to_srgba_unmultiplied();
            g.stops[k].color = Color32::from_rgba_unmultiplied(r, gr, b, a);
        }
        let op = Rect::from_min_max(Pos2::new(field.right() - op_w, field.top()), field.max);
        ui.painter().rect_filled(Rect::from_min_size(op.min, Vec2::new(1.0, op.height())), 0.0, BG());
        let mut alpha = (g.stops[k].color.a() as f32 / 255.0 * 100.0).round();
        if fig::num_field(ui, op, id.with(("alpha", k)), Prefix::None, &mut alpha, Num { min: 0.0, max: 100.0, decimals: 0, suffix: "", speed: 1.0 }, &format!("Gradient stop {} opacity", k + 1)) {
            let [r, gr, b, _] = g.stops[k].color.to_srgba_unmultiplied();
            g.stops[k].color = Color32::from_rgba_unmultiplied(r, gr, b, (alpha / 100.0 * 255.0).round() as u8);
        }
        fig::text(ui, Pos2::new(op.right() - 8.0, op.center().y), Align2::RIGHT_CENTER, "%", fig::reg(11.0), TEXT());
        if g.stops.len() > 2 && fig::icon_button(ui, Rect::from_min_size(Pos2::new(row.left() + 208.0, y), Vec2::splat(fig::ROW)), Glyph::Minus, "Delete gradient stop", false).clicked() {
            kill = Some(k);
        }
    }
    if let Some(k) = pick {
        g.selected = k;
    }
    if let Some(k) = kill {
        g.stops.remove(k);
        g.selected = g.selected.min(g.stops.len() - 1);
    }
    g.sort();
    fig::row(ui, 12.0);
    *g != before
}

// Figma's slider row: a 16px track with a white thumb, -1..1 with 0 in the middle (double click resets)
fn adjust_slider(ui: &mut egui::Ui, r: Rect, v: &mut f32, id: Id, name: &str) -> bool {
    let track = Rect::from_center_size(r.center(), Vec2::new(r.width(), 16.0));
    let resp = ui.interact(track.expand2(Vec2::new(8.0, 4.0)), id, Sense::click_and_drag());
    let before = *v;
    if resp.double_clicked() {
        *v = 0.0;
    } else if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
        let t = ((p.x - (track.left() + 8.0)) / (track.width() - 16.0)).clamp(0.0, 1.0);
        *v = (t * 2.0 - 1.0 + 0.0).clamp(-1.0, 1.0);
        if v.abs() < 0.03 {
            *v = 0.0;
        }
    }
    let p = ui.painter();
    p.rect_filled(track, 8, fig::INPUT());
    p.rect_stroke(track, 8, Stroke::new(1.0, Color32::from_gray(0x4a)), egui::StrokeKind::Inside);
    let c = Pos2::new(track.left() + 8.0 + (*v + 1.0) / 2.0 * (track.width() - 16.0), track.center().y);
    p.circle_filled(c + Vec2::new(0.0, 0.5), 9.0, Color32::from_black_alpha(60));
    p.circle_filled(c, 8.0, Color32::WHITE);
    let value = (*v * 100.0).round() as f64;
    resp.widget_info(|| egui::WidgetInfo::slider(true, value, name));
    *v != before
}

// Figma's image tab. `textures` has the decoded images (for the preview); `upload` is set when the button is pressed.
pub fn image_editor(ui: &mut egui::Ui, fill: &mut ImageFill, img: &mut u64, images: &[Asset], textures: &std::collections::HashMap<u64, egui::TextureHandle>, id: Id, upload: &mut bool) -> bool {
    let before = fill.clone();
    let before_img = *img;
    fig::row(ui, 12.0);
    let r = fig::row(ui, fig::ROW);
    fig::dropdown(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top()), Vec2::new(96.0, fig::ROW)), None, &mut fill.mode, &ImageMode::ALL, "Image scale mode");
    if images.len() > 1 {
        let items: Vec<(u64, &str)> = images.iter().map(|a| (a.id, a.name.as_str())).collect();
        fig::dropdown(ui, Rect::from_min_size(Pos2::new(r.left() + 120.0, r.top()), Vec2::new(80.0, fig::ROW)), None, img, &items, "Image");
    }
    if fig::icon_button(ui, Rect::from_min_size(Pos2::new(r.left() + 208.0, r.top()), Vec2::splat(fig::ROW)), Glyph::RotateCw, "Rotate 90°", false).clicked() {
        fill.rotation = (fill.rotation + 90) % 360;
    }
    if fill.mode == ImageMode::Crop {
        // the part of the image that shows, in percent of the image; the layer has handles on the canvas too
        for (names, k0) in [(["Left", "Top"], 0usize), (["Right", "Bottom"], 2)] {
            fig::row(ui, 8.0);
            let r = fig::row(ui, fig::ROW);
            for (n, name) in names.iter().enumerate() {
                let k = k0 + n;
                let mut pct = (if k < 2 { fill.crop[k] } else { 1.0 - fill.crop[k] } * 100.0).round();
                let f = Rect::from_min_size(Pos2::new(r.left() + 16.0 + n as f32 * 108.0, r.top()), Vec2::new(100.0, fig::ROW));
                if fig::num_field(ui, f, id.with(("crop", k)), Prefix::Text(&name[..1]), &mut pct, Num { min: 0.0, max: 98.0, decimals: 0, suffix: "%", speed: 1.0 }, &format!("Crop {name}")) {
                    let v = pct / 100.0;
                    fill.crop[k] = if k < 2 { v } else { 1.0 - v };
                    fill.crop[0] = fill.crop[0].min(fill.crop[2] - 0.02);
                    fill.crop[1] = fill.crop[1].min(fill.crop[3] - 0.02);
                }
            }
        }
    }
    if fill.mode == ImageMode::Tile {
        fig::row(ui, 8.0);
        let r = fig::row(ui, fig::ROW);
        fig::text(ui, Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Tile scale", fig::reg(11.0), TEXT_SECONDARY());
        let mut pct = (fill.scale * 100.0).round();
        if fig::num_field(ui, Rect::from_min_size(Pos2::new(r.left() + 100.0, r.top()), Vec2::new(64.0, fig::ROW)), id.with("tile"), Prefix::None, &mut pct, Num { min: 1.0, max: 1000.0, decimals: 0, suffix: "%", speed: 1.0 }, "Tile scale") {
            fill.scale = (pct / 100.0).max(0.01);
        }
    }
    fig::row(ui, 12.0);
    let row = fig::row(ui, 208.0);
    let thumb_r = Rect::from_min_size(Pos2::new(row.left() + 16.0, row.top()), Vec2::splat(208.0));
    checker(ui.painter(), thumb_r, 16.0);
    if let Some(t) = textures.get(img) {
        let sz = t.size_vec2();
        let k = (thumb_r.width() / sz.x.max(1.0)).min(thumb_r.height() / sz.y.max(1.0));
        let shown = Rect::from_center_size(thumb_r.center(), sz * k);
        ui.painter().image(t.id(), shown, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    }
    round_corners(ui.painter(), thumb_r, 5.0);
    ui.painter().rect_stroke(thumb_r, 5, Stroke::new(1.0, Color32::from_black_alpha(25)), egui::StrokeKind::Inside);
    let btn = Rect::from_center_size(Pos2::new(thumb_r.center().x, if textures.contains_key(img) { thumb_r.bottom() - 24.0 } else { thumb_r.center().y }), Vec2::new(144.0, fig::ROW));
    let resp = ui.interact(btn, id.with("upload"), Sense::click());
    ui.painter().rect_filled(btn, fig::RADIUS, if resp.hovered() { fig::SHARE.gamma_multiply(0.85) } else { fig::SHARE });
    fig::ptext(ui.painter(), btn.center(), Align2::CENTER_CENTER, "Upload from computer", fig::semi(11.0), Color32::WHITE);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Upload from computer"));
    if resp.clicked() {
        *upload = true;
    }
    fig::row(ui, 8.0);
    let mut sliders: [(&str, &mut f32); 7] = [
        ("Exposure", &mut fill.exposure),
        ("Contrast", &mut fill.contrast),
        ("Saturation", &mut fill.saturation),
        ("Temperature", &mut fill.temperature),
        ("Tint", &mut fill.tint),
        ("Highlights", &mut fill.highlights),
        ("Shadows", &mut fill.shadows),
    ];
    for (name, v) in sliders.iter_mut() {
        let r = fig::row(ui, 32.0);
        fig::ptext(&ui.painter().with_clip_rect(Rect::from_min_max(r.min, Pos2::new(r.left() + 96.0, r.bottom()))), Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, *name, fig::reg(11.0), TEXT_SECONDARY());
        adjust_slider(ui, Rect::from_min_size(Pos2::new(r.left() + 100.0, r.top() + 4.0), Vec2::new(128.0, 24.0)), v, id.with(*name), &format!("Image {}", name.to_lowercase()));
    }
    fig::row(ui, 8.0);
    *fill != before || *img != before_img
}

// the Pattern editor: which layer is the tile, its scale, the gap between tiles, the angle and the shift
pub fn pattern_editor(ui: &mut egui::Ui, pf: &mut PatternFill, id: Id) {
    let sources: Vec<(u64, String)> = ui.data(|d| d.get_temp(Id::new("pattern-sources"))).unwrap_or_default();
    fig::row(ui, 12.0);
    let r = fig::row(ui, fig::ROW);
    let items: Vec<(Option<u64>, &str)> = std::iter::once((None, "Choose a layer")).chain(sources.iter().map(|(i, n)| (Some(*i), n.as_str()))).collect();
    fig::dropdown(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top()), Vec2::new(208.0, fig::ROW)), Some(Glyph::Frame), &mut pf.source, &items, "Pattern source layer");
    let row = |ui: &mut egui::Ui, label: &str| -> Rect {
        fig::row(ui, 8.0);
        let r = fig::row(ui, fig::ROW);
        fig::text(ui, Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, label, fig::reg(11.0), TEXT_SECONDARY());
        r
    };
    let field = |r: Rect, k: usize| Rect::from_min_size(Pos2::new(r.left() + 100.0 + k as f32 * 66.0, r.top()), Vec2::new(60.0, fig::ROW));
    let r = row(ui, "Scale");
    let mut pct = (pf.scale * 100.0).round();
    if fig::num_field(ui, field(r, 0), id.with("scale"), Prefix::None, &mut pct, Num { min: 1.0, max: 1000.0, decimals: 0, suffix: "%", speed: 1.0 }, "Pattern scale") {
        pf.scale = (pct / 100.0).max(0.01);
    }
    let big = Num { min: -10000.0, max: 10000.0, decimals: 0, speed: 1.0, suffix: "" };
    let r = row(ui, "Spacing");
    fig::num_field(ui, field(r, 0), id.with("sx"), Prefix::Text("X"), &mut pf.space.x, big, "Horizontal spacing");
    fig::num_field(ui, field(r, 1), id.with("sy"), Prefix::Text("Y"), &mut pf.space.y, big, "Vertical spacing");
    let r = row(ui, "Offset");
    fig::num_field(ui, field(r, 0), id.with("ox"), Prefix::Text("X"), &mut pf.offset.x, big, "Horizontal offset");
    fig::num_field(ui, field(r, 1), id.with("oy"), Prefix::Text("Y"), &mut pf.offset.y, big, "Vertical offset");
    let r = row(ui, "Rotation");
    fig::num_field(ui, field(r, 0), id.with("rot"), Prefix::None, &mut pf.rotation, Num { min: -360.0, max: 360.0, decimals: 0, speed: 1.0, suffix: "°" }, "Pattern rotation");
    let r = row(ui, "Tiles");
    fig::dropdown(ui, Rect::from_min_size(Pos2::new(r.left() + 100.0, r.top()), Vec2::new(124.0, fig::ROW)), None, &mut pf.tile, &[(0u8, "Rectangular"), (1, "Hexagonal, rows"), (2, "Hexagonal, columns")], "Tile type");
    let r = row(ui, "Align");
    let names = ["Top left", "Top center", "Top right", "Center left", "Center", "Center right", "Bottom left", "Bottom center", "Bottom right"];
    let items: Vec<(u8, &str)> = names.iter().enumerate().map(|(k, n)| (k as u8, *n)).collect();
    fig::dropdown(ui, Rect::from_min_size(Pos2::new(r.left() + 100.0, r.top()), Vec2::new(124.0, fig::ROW)), None, &mut pf.align, &items, "Pattern alignment");
    fig::row(ui, 12.0);
}

// the Shader editor: the kind, its two colours, the scale and the angle
pub fn shader_editor(ui: &mut egui::Ui, sf: &mut ShaderFill, id: Id) {
    fig::row(ui, 12.0);
    let r = fig::row(ui, fig::ROW);
    fig::dropdown(ui, Rect::from_min_size(Pos2::new(r.left() + 16.0, r.top()), Vec2::new(208.0, fig::ROW)), Some(Glyph::Waves), &mut sf.kind, &ShaderKind::ALL, "Shader");
    for (label, k) in [("Color 1", 0), ("Color 2", 1)] {
        fig::row(ui, 8.0);
        let r = fig::row(ui, fig::ROW);
        fig::text(ui, Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, label, fig::reg(11.0), TEXT_SECONDARY());
        let f = Rect::from_min_size(Pos2::new(r.left() + 100.0, r.top()), Vec2::new(124.0, fig::ROW));
        fig::input_bg(ui, f, false);
        let c = if k == 0 { &mut sf.a } else { &mut sf.b };
        fig::swatch(ui, Pos2::new(f.left() + 12.0, f.center().y), c, label);
        let mut rgb = *c;
        if fig::hex_text(ui, Pos2::new(f.left() + 26.0, f.center().y), f.width() - 30.0, &mut rgb, id.with(("hex", k))) {
            *c = Color32::from_rgba_unmultiplied(rgb.r(), rgb.g(), rgb.b(), c.a());
        }
    }
    fig::row(ui, 8.0);
    let r = fig::row(ui, fig::ROW);
    fig::text(ui, Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Scale", fig::reg(11.0), TEXT_SECONDARY());
    let mut pct = (sf.scale * 100.0).round();
    if fig::num_field(ui, Rect::from_min_size(Pos2::new(r.left() + 100.0, r.top()), Vec2::new(60.0, fig::ROW)), id.with("scale"), Prefix::None, &mut pct, Num { min: 5.0, max: 1000.0, decimals: 0, suffix: "%", speed: 1.0 }, "Shader scale") {
        sf.scale = (pct / 100.0).max(0.05);
    }
    fig::num_field(ui, Rect::from_min_size(Pos2::new(r.left() + 166.0, r.top()), Vec2::new(58.0, fig::ROW)), id.with("angle"), Prefix::None, &mut sf.angle, Num { min: -360.0, max: 360.0, decimals: 0, speed: 1.0, suffix: "°" }, "Shader angle");
    fig::row(ui, 12.0);
}
