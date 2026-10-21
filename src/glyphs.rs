// Small painted glyphs for action buttons (egui's default font lacks most symbols), plus tiny widgets that use them.
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::{BG, BG_PRESSED, BRAND, R_SMALL, TEXT, TEXT_SECONDARY, TEXT_TERTIARY};

#[derive(Clone, Copy, PartialEq)]
pub enum Glyph {
    AlignLeft,
    AlignHCenter,
    AlignRight,
    AlignTop,
    AlignVMiddle,
    AlignBottom,
    DistH,
    DistV,
    FlipH,
    FlipV,
    Group,
    Ungroup,
    Union,
    Subtract,
    Intersect,
    Exclude,
    Flatten,
    Mask,
    AutoLayout,
    Forward,
    Back,
    Delete,
    Component,
    Present,
    Help,
    Plus,
    Minus,
    Move,
    Hand,
    Frame,
    Rect,
    Ellipse,
    Polygon,
    Star,
    Line,
    Pen,
    Type,
    Comment,
    Vector,
    Eye,
    EyeOff,
    Lock,
    LockOpen,
    ChevronRight,
    ChevronDown,
    Undo,
    Redo,
    Open,
    Save,
    SaveAll,
    ImagePlus,
    Library,
    Clock,
    MessageSquare,
    Search,
    File,
    Image,
    CircleCheck,
    Download,
    Sparkles,
    Grid,
    Contrast,
    RotateCw,
    Radius,
    Crosshair,
    RotateCcw,
    Link,
    Pencil,
    Layers,
    Menu,
    CollapseAll,
    Code,
    Styles,
    Scan,
    Section,
    Slice,
    Instance,
    Shrink,
    Check,
    Info,
    Globe,
    Waves,
    Swap,
    ShadowDrop,
    ShadowInner,
    BlurLayer,
    BlurBackground,
    Ratio,
    Rows,
    Columns,
    Droplet,
    Copy,
    PanelLeft,
    Close,
    Pipette,
    Gradient,
    More,
    ArrowUp,
    BookOpen,
    Sliders,
    Underline,
    Strikethrough,
    BulletList,
    NumberList,
    PanelTop,
    PanelRight,
    PanelBottom,
    Wrap,
    Cursor,
    Keyframe,
    TextLeft,
    TextCenter,
    TextRight,
    ToTop,
    ToMiddle,
    ToBottom,
    AutoWidth,
    AutoHeight,
    Variable,
    Grip,
    Sun,
}

// Icons are Lucide (ISC license, see assets/icons/LICENSE), embedded and rasterized once with resvg.
// A glyph without an SVG falls back to the hand-painted drawing below.
macro_rules! svg {
    ($n:literal) => {
        Some(($n, include_str!(concat!("../assets/icons/", $n, ".svg"))))
    };
}

pub fn svg_of(g: Glyph) -> Option<(&'static str, &'static str)> {
    use Glyph::*;
    match g {
        AlignLeft => svg!("align-start-vertical"),
        AlignHCenter => svg!("align-center-vertical"),
        AlignRight => svg!("align-end-vertical"),
        AlignTop => svg!("align-start-horizontal"),
        AlignVMiddle => svg!("align-center-horizontal"),
        AlignBottom => svg!("align-end-horizontal"),
        DistH => svg!("align-horizontal-distribute-center"),
        DistV => svg!("align-vertical-distribute-center"),
        FlipH => svg!("arrow-left-right"),
        FlipV => svg!("arrow-up-down"),
        Group => svg!("group"),
        Ungroup => svg!("ungroup"),
        Union => svg!("squares-unite"),
        Subtract => svg!("squares-subtract"),
        Intersect => svg!("squares-intersect"),
        Exclude => svg!("squares-exclude"),
        Flatten | Vector => svg!("spline"),
        Mask => None,
        AutoLayout => svg!("columns-3"),
        Forward => svg!("bring-to-front"),
        Back => svg!("send-to-back"),
        Delete => svg!("trash"),
        Component => svg!("component"),
        Present => svg!("play"),
        Help => svg!("circle-question-mark"),
        Plus => svg!("plus"),
        Minus => svg!("minus"),
        Move => svg!("mouse-pointer-2"),
        Hand => svg!("hand"),
        Frame => svg!("frame"),
        Rect => svg!("square"),
        Ellipse => svg!("circle"),
        Polygon => svg!("triangle"),
        Star => svg!("star"),
        Line => svg!("slash"),
        Pen => svg!("pen-tool"),
        Type => svg!("type"),
        Comment => svg!("message-circle"),
        Eye => svg!("eye"),
        EyeOff => svg!("eye-off"),
        Lock => svg!("lock"),
        LockOpen => svg!("lock-open"),
        ChevronRight => svg!("chevron-right"),
        ChevronDown => svg!("chevron-down"),
        Undo => svg!("undo-2"),
        Redo => svg!("redo-2"),
        Open => svg!("folder-open"),
        Save => svg!("save"),
        SaveAll => svg!("save-all"),
        ImagePlus => svg!("image-plus"),
        Library => svg!("library"),
        Clock => svg!("clock"),
        MessageSquare => svg!("message-square"),
        Search => svg!("search"),
        File => svg!("file"),
        Image => svg!("image"),
        CircleCheck => svg!("circle-check"),
        Download => svg!("download"),
        Sparkles => svg!("sparkles"),
        Grid => svg!("grid-3x3"),
        Contrast => svg!("contrast"),
        RotateCw => svg!("rotate-cw"),
        Radius => svg!("square-round-corner"),
        Crosshair => svg!("crosshair"),
        RotateCcw => svg!("rotate-ccw"),
        Link => svg!("link"),
        Pencil => svg!("pencil"),
        Layers => svg!("layers"),
        Menu => svg!("menu"),
        CollapseAll => svg!("list-collapse"),
        Code => svg!("code"),
        Styles => svg!("layout-grid"),
        Scan => svg!("scan"),
        Section => svg!("layout-template"),
        Instance => svg!("diamond"),
        Slice => svg!("scissors"),
        Shrink => svg!("shrink"),
        Check => svg!("check"),
        Info => svg!("info"),
        Globe => svg!("globe"),
        Waves => svg!("waves"),
        Swap => svg!("arrow-left-right"),
        ShadowDrop => svg!("shadow-drop"),
        ShadowInner => svg!("shadow-inner"),
        BlurLayer => svg!("blur-layer"),
        BlurBackground => svg!("blur-background"),
        Ratio => svg!("ratio"),
        TextLeft => svg!("text-align-start"),
        TextCenter => svg!("text-align-center"),
        TextRight => svg!("text-align-end"),
        ToTop => svg!("arrow-up-to-line"),
        ToMiddle => svg!("fold-vertical"),
        ToBottom => svg!("arrow-down-to-line"),
        AutoWidth => svg!("arrow-right-to-line"),
        AutoHeight => svg!("arrow-down-to-line"),
        Variable => svg!("variable"),
        Grip => svg!("grip"),
        Sun => svg!("sun"),
        Rows => svg!("rows-3"),
        Columns => svg!("columns-3"),
        Droplet => svg!("droplet"),
        Copy => svg!("copy"),
        PanelLeft => svg!("panel-left"),
        Close => svg!("x"),
        Pipette => svg!("pipette"),
        Gradient => svg!("blend"),
        More => svg!("ellipsis"),
        ArrowUp => svg!("arrow-up"),
        BookOpen => svg!("book-open"),
        Sliders => svg!("sliders-horizontal"),
        Underline => svg!("underline"),
        Strikethrough => svg!("strikethrough"),
        BulletList => svg!("list"),
        NumberList => svg!("list-ordered"),
        PanelTop => svg!("panel-top"),
        PanelRight => svg!("panel-right"),
        PanelBottom => svg!("panel-bottom"),
        Wrap => svg!("corner-down-left"),
        Cursor => svg!("mouse-pointer-click"),
        Keyframe => svg!("diamond"),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn rasterize(svg: &str) -> Option<egui::ColorImage> {
    rasterize_px(svg, 72, 2.0)
}

// the 24 unit icon at exactly `px` device pixels, its lines `stroke` units wide
pub fn rasterize_px(svg: &str, px: u32, stroke: f32) -> Option<egui::ColorImage> {
    use resvg::{tiny_skia, usvg};
    let svg = svg.replace("currentColor", "#ffffff").replace("stroke-width=\"2\"", &format!("stroke-width=\"{stroke}\""));
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let mut pm = tiny_skia::Pixmap::new(px.max(1), px.max(1))?;
    let k = px as f32 / 24.0;
    resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pm.as_mut());
    Some(egui::ColorImage::from_rgba_premultiplied([px.max(1) as usize; 2], pm.data()))
}

// one texture per icon and device size, so icons are drawn 1:1 and stay crisp. Figma's icons have
// 1px lines whatever their size, so the stroke is set to one logical pixel at that size.
fn icon_texture(ctx: &egui::Context, name: &'static str, svg: &str, px: u32) -> Option<egui::TextureId> {
    let key = egui::Id::new(("lucide", name, px));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return Some(t.id());
    }
    let ppp = ctx.pixels_per_point();
    let stroke = (24.0 * ppp / px as f32).clamp(0.75, 3.0);
    let tex = ctx.load_texture(format!("icon-{name}-{px}"), rasterize_px(svg, px, stroke)?, egui::TextureOptions::LINEAR);
    let id = tex.id();
    ctx.data_mut(|d| d.insert_temp(key, tex));
    Some(id)
}

pub fn paint_glyph(p: &egui::Painter, c: Pos2, g: Glyph, color: Color32) {
    paint_glyph_sized(p, c, g, color, 16.0);
}

pub fn paint_glyph_sized(p: &egui::Painter, c: Pos2, g: Glyph, color: Color32, size: f32) {
    let ppp = p.ctx().pixels_per_point();
    let px = (size * ppp).round().max(1.0) as u32;
    if let Some(tid) = svg_of(g).and_then(|(name, svg)| icon_texture(p.ctx(), name, svg, px)) {
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        // on the device pixel grid, at its own pixel size
        let side = px as f32 / ppp;
        let min = Pos2::new(((c.x - side / 2.0) * ppp).round() / ppp, ((c.y - side / 2.0) * ppp).round() / ppp);
        p.image(tid, Rect::from_min_size(min, Vec2::splat(side)), uv, color);
        return;
    }
    let st = Stroke::new(1.3, color);
    let at = |x: f32, y: f32| c + Vec2::new(x, y);
    let bar = |x0: f32, y0: f32, x1: f32, y1: f32| p.rect_filled(Rect::from_min_max(at(x0, y0), at(x1, y1)), 1.0, color);
    let outline = |x0: f32, y0: f32, x1: f32, y1: f32| p.rect_stroke(Rect::from_min_max(at(x0, y0), at(x1, y1)), 1.0, st, StrokeKind::Middle);
    match g {
        Glyph::AlignLeft => {
            p.line_segment([at(-6.0, -7.0), at(-6.0, 7.0)], st);
            bar(-4.0, -5.0, 6.0, -1.5);
            bar(-4.0, 1.5, 2.0, 5.0);
        }
        Glyph::AlignHCenter => {
            p.line_segment([at(0.0, -7.0), at(0.0, 7.0)], st);
            bar(-6.0, -5.0, 6.0, -1.5);
            bar(-3.0, 1.5, 3.0, 5.0);
        }
        Glyph::AlignRight => {
            p.line_segment([at(6.0, -7.0), at(6.0, 7.0)], st);
            bar(-6.0, -5.0, 4.0, -1.5);
            bar(-2.0, 1.5, 4.0, 5.0);
        }
        Glyph::AlignTop => {
            p.line_segment([at(-7.0, -6.0), at(7.0, -6.0)], st);
            bar(-5.0, -4.0, -1.5, 6.0);
            bar(1.5, -4.0, 5.0, 2.0);
        }
        Glyph::AlignVMiddle => {
            p.line_segment([at(-7.0, 0.0), at(7.0, 0.0)], st);
            bar(-5.0, -6.0, -1.5, 6.0);
            bar(1.5, -3.0, 5.0, 3.0);
        }
        Glyph::AlignBottom => {
            p.line_segment([at(-7.0, 6.0), at(7.0, 6.0)], st);
            bar(-5.0, -6.0, -1.5, 4.0);
            bar(1.5, 0.0, 5.0, 4.0);
        }
        Glyph::DistH => {
            p.line_segment([at(-6.5, -6.0), at(-6.5, 6.0)], st);
            p.line_segment([at(6.5, -6.0), at(6.5, 6.0)], st);
            bar(-2.0, -4.0, 2.0, 4.0);
        }
        Glyph::DistV => {
            p.line_segment([at(-6.0, -6.5), at(6.0, -6.5)], st);
            p.line_segment([at(-6.0, 6.5), at(6.0, 6.5)], st);
            bar(-4.0, -2.0, 4.0, 2.0);
        }
        Glyph::FlipH => {
            p.line_segment([at(0.0, -7.0), at(0.0, 7.0)], st);
            p.add(egui::Shape::closed_line(vec![at(-2.0, -5.0), at(-7.0, 5.0), at(-2.0, 5.0)], st));
            p.add(egui::Shape::convex_polygon(vec![at(2.0, -5.0), at(7.0, 5.0), at(2.0, 5.0)], color, Stroke::NONE));
        }
        Glyph::FlipV => {
            p.line_segment([at(-7.0, 0.0), at(7.0, 0.0)], st);
            p.add(egui::Shape::closed_line(vec![at(-5.0, -2.0), at(5.0, -7.0), at(5.0, -2.0)], st));
            p.add(egui::Shape::convex_polygon(vec![at(-5.0, 2.0), at(5.0, 7.0), at(5.0, 2.0)], color, Stroke::NONE));
        }
        Glyph::Group => {
            for (a, b) in [
                ((-7.0, -7.0), (-3.0, -7.0)),
                ((3.0, -7.0), (7.0, -7.0)),
                ((7.0, -7.0), (7.0, -3.0)),
                ((7.0, 3.0), (7.0, 7.0)),
                ((7.0, 7.0), (3.0, 7.0)),
                ((-3.0, 7.0), (-7.0, 7.0)),
                ((-7.0, 7.0), (-7.0, 3.0)),
                ((-7.0, -3.0), (-7.0, -7.0)),
            ] {
                p.line_segment([at(a.0, a.1), at(b.0, b.1)], st);
            }
            outline(-4.0, -4.0, 0.0, 0.0);
            outline(1.0, 1.0, 5.0, 5.0);
        }
        Glyph::Ungroup => {
            outline(-7.0, -7.0, -1.0, -1.0);
            outline(1.0, 1.0, 7.0, 7.0);
        }
        Glyph::Union => {
            bar(-6.0, -6.0, 2.0, 2.0);
            bar(-2.0, -2.0, 6.0, 6.0);
        }
        Glyph::Subtract => {
            bar(-6.0, -6.0, 2.0, 2.0);
            p.rect_filled(Rect::from_min_max(at(-2.0, -2.0), at(2.0, 2.0)), 0.0, BG());
            outline(-2.0, -2.0, 6.0, 6.0);
        }
        Glyph::Intersect => {
            outline(-6.0, -6.0, 2.0, 2.0);
            outline(-2.0, -2.0, 6.0, 6.0);
            bar(-2.0, -2.0, 2.0, 2.0);
        }
        Glyph::Exclude => {
            bar(-6.0, -6.0, 2.0, 2.0);
            bar(-2.0, -2.0, 6.0, 6.0);
            p.rect_filled(Rect::from_min_max(at(-2.0, -2.0), at(2.0, 2.0)), 0.0, BG());
        }
        Glyph::Flatten => {
            p.line_segment([at(-6.0, 5.0), at(-1.0, -4.0)], st);
            p.line_segment([at(-1.0, -4.0), at(6.0, 3.0)], st);
            for q in [at(-6.0, 5.0), at(-1.0, -4.0), at(6.0, 3.0)] {
                p.rect_filled(Rect::from_center_size(q, Vec2::splat(3.5)), 0.0, color);
            }
        }
        Glyph::Mask => {
            outline(-6.5, -6.5, 6.5, 6.5);
            p.circle_filled(at(0.0, 0.0), 3.5, color);
        }
        Glyph::AutoLayout => {
            outline(-7.0, -6.0, 7.0, 6.0);
            bar(-4.5, -3.0, -2.0, 3.0);
            bar(-1.0, -3.0, 1.5, 3.0);
            bar(2.5, -3.0, 5.0, 3.0);
        }
        Glyph::Forward => {
            outline(-6.0, -1.0, 1.0, 6.0);
            bar(-2.0, -6.0, 6.0, 2.0);
        }
        Glyph::Back => {
            bar(-6.0, -1.0, 1.0, 6.0);
            outline(-2.0, -6.0, 6.0, 2.0);
        }
        Glyph::Delete => {
            p.line_segment([at(-6.0, -4.0), at(6.0, -4.0)], st);
            p.line_segment([at(-2.0, -6.5), at(2.0, -6.5)], st);
            outline(-4.5, -3.0, 4.5, 6.5);
            p.line_segment([at(-1.5, -1.0), at(-1.5, 4.5)], Stroke::new(1.0, color));
            p.line_segment([at(1.5, -1.0), at(1.5, 4.5)], Stroke::new(1.0, color));
        }
        Glyph::Component => {
            p.add(egui::Shape::closed_line(vec![at(0.0, -7.0), at(7.0, 0.0), at(0.0, 7.0), at(-7.0, 0.0)], st));
            p.add(egui::Shape::convex_polygon(vec![at(0.0, -3.0), at(3.0, 0.0), at(0.0, 3.0), at(-3.0, 0.0)], color, Stroke::NONE));
        }
        Glyph::Present => {
            p.add(egui::Shape::convex_polygon(vec![at(-4.0, -6.0), at(6.0, 0.0), at(-4.0, 6.0)], color, Stroke::NONE));
        }
        Glyph::Help => {
            crate::fig::ptext(p, c, Align2::CENTER_CENTER, "?", FontId::proportional(14.0), color);
        }
        Glyph::Plus => {
            p.line_segment([at(-5.0, 0.0), at(5.0, 0.0)], Stroke::new(1.6, color));
            p.line_segment([at(0.0, -5.0), at(0.0, 5.0)], Stroke::new(1.6, color));
        }
        Glyph::Minus => {
            p.line_segment([at(-5.0, 0.0), at(5.0, 0.0)], Stroke::new(1.6, color));
        }
        _ => {}
    }
}

// a small decorative icon in a row (14px cell)
pub fn row_icon(ui: &mut egui::Ui, g: Glyph, color: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
    paint_glyph_sized(ui.painter(), rect.center(), g, color, 14.0);
    resp
}

// a thin vertical divider between button groups
pub fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(7.0, 24.0), Sense::hover());
    ui.painter().line_segment([rect.center_top() + Vec2::new(0.0, 3.0), rect.center_bottom() - Vec2::new(0.0, 3.0)], Stroke::new(1.0, BG_PRESSED()));
}

// a menu row: optional icon, label, shortcut hint on the right; true when clicked
// a menu whose content depends on what it is about (the same button, another layer) names that, so its width memory is not
// shared with the last one: call it first thing in the menu
pub fn menu_salt(ui: &egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new("menu-salt"), egui::Id::new(salt)));
}

// the width of the widest item of the menu `ui` is drawing: the larger of last frame's widest and this
// frame's widest so far, so a menu settles on its content width and shrinks when the content does
fn menu_width(ui: &egui::Ui, item: f32) -> f32 {
    let salt = ui.data(|d| d.get_temp::<egui::Id>(egui::Id::new("menu-salt")));
    let id = ui.id().with("menu-width").with(salt);
    let pass = ui.ctx().cumulative_pass_nr();
    let (seen, mut cur, mut prev) = ui.data(|d| d.get_temp::<(u64, f32, f32)>(id)).unwrap_or((pass, 0.0, 0.0));
    if seen != pass {
        // a menu that was closed for a while (a gap of passes) starts over: its last width belongs to other content
        prev = if pass > seen + 2 { 0.0 } else { cur };
        cur = 0.0;
    }
    cur = cur.max(item);
    ui.data_mut(|d| d.insert_temp(id, (pass, cur, prev)));
    cur.max(prev)
}

// Figma's menu rows (measured in the zoom menu, Figma 126.9.13, CSS px): 24px tall, 11px text starting 32px from the
// menu's left edge, an icon or check mark centred at 20px, the shortcut 16px from the right edge in secondary ink.
// Separators are 1px #383838 with 7px above and below.
pub const MENU_ROW: f32 = 24.0;

// where a menu row's label starts: Figma measures 32px; 6px more keeps the label clear of its icon
const LABEL_X: f32 = 38.0;

// the style of Figma's main menu and its cascading submenus: a #1e1e1e card with 8px padding whose rows are inset
// highlights (see `fig::MENU_BG()`); `menu_row` and `menu_sep` lay out differently inside one
pub fn fig_menu_style(s: &mut egui::Style) {
    egui::containers::menu::menu_style(s);
    s.visuals.window_fill = crate::fig::MENU_BG();
    s.visuals.window_stroke = Stroke::NONE;
    let p = crate::fig::MENU_PAD;
    s.spacing.menu_margin = egui::Margin::same(p);
    s.spacing.item_spacing = Vec2::ZERO;
    s.spacing.menu_spacing = crate::fig::MENU_GROUP_GAP; // read by `menu_sep`
    s.spacing.icon_width = 0.0; // no icons in rows: `menu_row` reads this
    s.spacing.menu_width = 0.0; // the minimum width of a row: a menu sets its own (`ui.spacing_mut().menu_width`)
}

// the same card for a menu whose rows have icons (Figma's "More actions": icon centred 19px from the highlight's edge, label
// at 35px, 24px of room after the longest label, groups 15px apart)
pub fn fig_icon_menu_style(s: &mut egui::Style) {
    fig_menu_style(s);
    s.spacing.menu_spacing = 15.0;
    s.spacing.icon_width = 16.0;
}

// the least width of a row: an inset menu names it (`spacing.menu_width`); egui's own default would pad the other menus out
fn row_min(ui: &egui::Ui) -> f32 {
    if inset_menu(ui) {
        ui.spacing().menu_width
    } else {
        ui.min_size().x
    }
}

fn inset_menu(ui: &egui::Ui) -> bool {
    ui.spacing().menu_margin.left > 0
}

fn menu_row(ui: &mut egui::Ui, g: Option<Glyph>, check: Option<bool>, label: &str, shortcut: &str, enabled: bool, chevron: bool) -> bool {
    // always drawn: a disabled row shows greyed, it must not be skipped
    let resp = menu_row_resp(ui, g, check, label, shortcut, enabled, chevron);
    enabled && resp.clicked()
}

fn menu_row_resp(ui: &mut egui::Ui, g: Option<Glyph>, check: Option<bool>, label: &str, shortcut: &str, enabled: bool, chevron: bool) -> egui::Response {
    // menus hug their widest item, as Figma's do: every item takes the width of the widest one seen last
    // frame in this menu (or this frame so far), never the popup's whole available width
    let font = crate::fig::reg(11.0);
    let label_w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), TEXT()).size().x);
    let inset = inset_menu(ui);
    // inset menus (Figma's main menu): label 8px inside the highlight, shortcut 8px from its right edge, 6px between them
    let icons = inset && ui.spacing().icon_width > 0.0;
    let (label_x, right, gap) = if icons { (35.0, 8.0, 20.0) } else if inset { (if check.is_some() { 24.0 } else { 8.0 }, 8.0, 6.0) } else { (LABEL_X, 16.0, 20.0) };
    let short_w = if shortcut.is_empty() { 0.0 } else { gap + ui.fonts_mut(|f| f.layout_no_wrap(shortcut.to_owned(), font.clone(), TEXT()).size().x) };
    let w = menu_width(ui, label_x + label_w + short_w + if icons { if shortcut.is_empty() { 28.0 } else { 8.0 } } else if inset { 8.0 } else { 16.0 }).max(ui.min_rect().width()).max(row_min(ui));
    if inset {
        // a popup remembers its last width and a justified layout stretches rows to it, so a menu would never shrink
        ui.set_max_width(w);
    }
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, MENU_ROW), if enabled { Sense::click() } else { Sense::hover() });
    let hot = enabled && resp.hovered();
    if hot {
        ui.painter().rect_filled(rect, R_SMALL as f32 + 1.0, if inset { crate::fig::MENU_HOT } else { BRAND });
    }
    let (ink, dim) = if hot {
        (Color32::WHITE, Color32::WHITE)
    } else if enabled {
        (TEXT(), TEXT_SECONDARY())
    } else {
        (TEXT_TERTIARY(), TEXT_TERTIARY())
    };
    let p = ui.painter();
    // inset menus have no icons; a check item keeps a 16px column for its tick (position not measured)
    let icon = if check == Some(true) { Some(Glyph::Check) } else if inset && !icons { None } else { g };
    if let Some(g) = icon {
        paint_glyph(p, rect.left_center() + Vec2::new(if icons { 19.3 } else if inset { 12.0 } else { 20.0 }, 0.0), g, ink);
    }
    crate::fig::ptext(p, rect.left_center() + Vec2::new(label_x, 0.0), Align2::LEFT_CENTER, label, font.clone(), ink);
    if chevron {
        paint_glyph_sized(p, rect.right_center() - Vec2::new(if inset { 11.3 } else { 16.0 }, 0.0), Glyph::ChevronRight, ink, 9.5);
    } else if !shortcut.is_empty() {
        crate::fig::ptext(p, rect.right_center() - Vec2::new(right, 0.0), Align2::RIGHT_CENTER, shortcut, font, dim);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    resp
}

// a row with a chevron that opens a submenu to its right on hover, as Figma's cascading menus do
pub fn menu_submenu<R>(ui: &mut egui::Ui, g: Option<Glyph>, label: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) {
    let resp = menu_row_resp(ui, g, None, label, "", true, true);
    egui::containers::menu::SubMenu::new().show(ui, &resp, add_contents);
}

// an option of a dropdown list (measured in Figma 126.9.13, font style menu): 24px rows, a check at 11px for the current one,
// the label at 24px, the current or hovered row filled with the brand blue
pub fn menu_option(ui: &mut egui::Ui, on: bool, label: &str) -> bool {
    let font = crate::fig::reg(11.0);
    let label_w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), TEXT()).size().x);
    let w = menu_width(ui, 24.0 + label_w + 16.0).max(ui.min_rect().width());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, MENU_ROW), Sense::click());
    if on || resp.hovered() {
        ui.painter().rect_filled(rect, R_SMALL as f32 + 1.0, BRAND);
    }
    if on {
        paint_glyph_sized(ui.painter(), rect.left_center() + Vec2::new(11.0, 0.0), Glyph::Check, Color32::WHITE, 9.5);
    }
    crate::fig::ptext(ui.painter(), rect.left_center() + Vec2::new(24.0, 0.0), Align2::LEFT_CENTER, label, font, if on || resp.hovered() { Color32::WHITE } else { TEXT() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, label));
    resp.clicked()
}

pub fn menu_item(ui: &mut egui::Ui, g: Option<Glyph>, label: &str, shortcut: &str, enabled: bool) -> bool {
    menu_row(ui, g, None, label, shortcut, enabled, false)
}

// a row of a toolbar tool menu (measured, Figma 126.9.13 "Move tools"): check at 19px, tool icon at 43px, name at 59px,
// shortcut 16px from the right
pub fn menu_item_checked(ui: &mut egui::Ui, g: Option<Glyph>, on: bool, label: &str, shortcut: &str) -> bool {
    let font = crate::fig::reg(11.0);
    let label_w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), TEXT()).size().x);
    let short_w = if shortcut.is_empty() { 0.0 } else { 24.0 + ui.fonts_mut(|f| f.layout_no_wrap(shortcut.to_owned(), font.clone(), TEXT()).size().x) };
    let w = menu_width(ui, 59.0 + label_w + short_w + 16.0).max(ui.min_rect().width());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, MENU_ROW), Sense::click());
    let hot = resp.hovered();
    if hot {
        ui.painter().rect_filled(rect, R_SMALL as f32 + 1.0, BRAND);
    }
    let (ink, dim) = if hot { (Color32::WHITE, Color32::WHITE) } else { (TEXT(), TEXT_SECONDARY()) };
    let p = ui.painter();
    if on {
        paint_glyph(p, rect.left_center() + Vec2::new(19.0, 0.0), Glyph::Check, ink);
    }
    if let Some(g) = g {
        paint_glyph(p, rect.left_center() + Vec2::new(43.0, 0.0), g, ink);
    }
    crate::fig::ptext(p, rect.left_center() + Vec2::new(59.0, 0.0), Align2::LEFT_CENTER, label, font.clone(), ink);
    if !shortcut.is_empty() {
        crate::fig::ptext(p, rect.right_center() - Vec2::new(16.0, 0.0), Align2::RIGHT_CENTER, shortcut, font, dim);
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, label));
    resp.clicked()
}

// a row with a check mark in front when `on`
pub fn menu_check(ui: &mut egui::Ui, on: bool, label: &str, shortcut: &str) -> bool {
    let resp = menu_row(ui, None, Some(on), label, shortcut, true, false);
    resp
}

// the Actions row at the top of Figma's main menu: a search icon at 16px, the label at 32px, the shortcut at the right
pub fn menu_search_row(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    let font = crate::fig::reg(11.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.min_rect().width().max(row_min(ui)), MENU_ROW), Sense::click());
    let hot = resp.hovered();
    if hot {
        ui.painter().rect_filled(rect, R_SMALL as f32 + 1.0, crate::fig::MENU_HOT);
    }
    let (ink, dim) = if hot { (Color32::WHITE, Color32::WHITE) } else { (TEXT(), TEXT_SECONDARY()) };
    paint_glyph(ui.painter(), rect.left_center() + Vec2::new(16.0, 0.0), Glyph::Search, ink);
    crate::fig::ptext(ui.painter(), rect.left_center() + Vec2::new(32.0, 0.0), Align2::LEFT_CENTER, label, font.clone(), ink);
    crate::fig::ptext(ui.painter(), rect.right_center() - Vec2::new(8.0, 0.0), Align2::RIGHT_CENTER, shortcut, font, dim);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp.clicked()
}

// a row that opens a submenu (a chevron at the right)
#[allow(dead_code)]
pub fn menu_sub(ui: &mut egui::Ui, label: &str) -> bool {
    menu_row(ui, None, None, label, "", true, true)
}

// the divider between groups of rows
pub fn menu_sep(ui: &mut egui::Ui) {
    if inset_menu(ui) {
        // 8px, a 1px line across the whole card (through its padding), 8px
        let pad = ui.spacing().menu_margin.left as f32;
        let gap = ui.spacing().menu_spacing;
        let (r, _) = ui.allocate_exact_size(Vec2::new(menu_width(ui, 0.0).max(ui.min_rect().width()).max(row_min(ui)), gap), Sense::hover());
        ui.painter().rect_filled(Rect::from_min_size(Pos2::new(r.left() - pad, r.top() + ((gap - 1.0) / 2.0).floor()), Vec2::new(r.width() + 2.0 * pad, 1.0)), 0.0, crate::fig::MENU_RULE());
        return;
    }
    let (r, _) = ui.allocate_exact_size(Vec2::new(menu_width(ui, 0.0).max(ui.min_rect().width()), 15.0), Sense::hover());
    ui.painter().rect_filled(Rect::from_min_size(Pos2::new(r.left(), r.top() + 7.0), Vec2::new(r.width(), 1.0)), 0.0, Color32::from_gray(0x38));
}

// every glyph with its name, for the component gallery (generated from the enum: keep in step with it)
pub fn all_glyphs() -> Vec<(Glyph, &'static str)> {
    vec![
        (Glyph::AlignLeft, "AlignLeft"),
        (Glyph::AlignHCenter, "AlignHCenter"),
        (Glyph::AlignRight, "AlignRight"),
        (Glyph::AlignTop, "AlignTop"),
        (Glyph::AlignVMiddle, "AlignVMiddle"),
        (Glyph::AlignBottom, "AlignBottom"),
        (Glyph::DistH, "DistH"),
        (Glyph::DistV, "DistV"),
        (Glyph::FlipH, "FlipH"),
        (Glyph::FlipV, "FlipV"),
        (Glyph::Group, "Group"),
        (Glyph::Ungroup, "Ungroup"),
        (Glyph::Union, "Union"),
        (Glyph::Subtract, "Subtract"),
        (Glyph::Intersect, "Intersect"),
        (Glyph::Exclude, "Exclude"),
        (Glyph::Flatten, "Flatten"),
        (Glyph::Mask, "Mask"),
        (Glyph::AutoLayout, "AutoLayout"),
        (Glyph::Forward, "Forward"),
        (Glyph::Back, "Back"),
        (Glyph::Delete, "Delete"),
        (Glyph::Component, "Component"),
        (Glyph::Present, "Present"),
        (Glyph::Help, "Help"),
        (Glyph::Plus, "Plus"),
        (Glyph::Minus, "Minus"),
        (Glyph::Move, "Move"),
        (Glyph::Hand, "Hand"),
        (Glyph::Frame, "Frame"),
        (Glyph::Rect, "Rect"),
        (Glyph::Ellipse, "Ellipse"),
        (Glyph::Polygon, "Polygon"),
        (Glyph::Star, "Star"),
        (Glyph::Line, "Line"),
        (Glyph::Pen, "Pen"),
        (Glyph::Type, "Type"),
        (Glyph::Comment, "Comment"),
        (Glyph::Vector, "Vector"),
        (Glyph::Eye, "Eye"),
        (Glyph::EyeOff, "EyeOff"),
        (Glyph::Lock, "Lock"),
        (Glyph::LockOpen, "LockOpen"),
        (Glyph::ChevronRight, "ChevronRight"),
        (Glyph::ChevronDown, "ChevronDown"),
        (Glyph::Undo, "Undo"),
        (Glyph::Redo, "Redo"),
        (Glyph::Open, "Open"),
        (Glyph::Save, "Save"),
        (Glyph::SaveAll, "SaveAll"),
        (Glyph::ImagePlus, "ImagePlus"),
        (Glyph::Library, "Library"),
        (Glyph::Clock, "Clock"),
        (Glyph::MessageSquare, "MessageSquare"),
        (Glyph::Search, "Search"),
        (Glyph::File, "File"),
        (Glyph::Image, "Image"),
        (Glyph::CircleCheck, "CircleCheck"),
        (Glyph::Download, "Download"),
        (Glyph::Sparkles, "Sparkles"),
        (Glyph::Grid, "Grid"),
        (Glyph::Contrast, "Contrast"),
        (Glyph::RotateCw, "RotateCw"),
        (Glyph::Radius, "Radius"),
        (Glyph::Crosshair, "Crosshair"),
        (Glyph::TextLeft, "TextLeft"),
        (Glyph::TextCenter, "TextCenter"),
        (Glyph::TextRight, "TextRight"),
        (Glyph::ToTop, "ToTop"),
        (Glyph::ToMiddle, "ToMiddle"),
        (Glyph::ToBottom, "ToBottom"),
        (Glyph::AutoWidth, "AutoWidth"),
        (Glyph::AutoHeight, "AutoHeight"),
        (Glyph::Variable, "Variable"),
        (Glyph::Grip, "Grip"),
        (Glyph::Sun, "Sun"),
        (Glyph::RotateCcw, "RotateCcw"),
        (Glyph::Link, "Link"),
        (Glyph::Pencil, "Pencil"),
        (Glyph::Layers, "Layers"),
        (Glyph::Menu, "Menu"),
        (Glyph::CollapseAll, "CollapseAll"),
        (Glyph::Code, "Code"),
        (Glyph::Styles, "Styles"),
        (Glyph::Scan, "Scan"),
        (Glyph::Section, "Section"),
        (Glyph::Instance, "Instance"),
        (Glyph::Slice, "Slice"),
        (Glyph::Shrink, "Shrink"),
        (Glyph::Check, "Check"),
        (Glyph::Info, "Info"),
        (Glyph::Globe, "Globe"),
        (Glyph::Waves, "Waves"),
        (Glyph::Swap, "Swap"),
        (Glyph::ShadowDrop, "ShadowDrop"),
        (Glyph::ShadowInner, "ShadowInner"),
        (Glyph::BlurLayer, "BlurLayer"),
        (Glyph::BlurBackground, "BlurBackground"),
        (Glyph::Ratio, "Ratio"),
        (Glyph::Rows, "Rows"),
        (Glyph::Columns, "Columns"),
        (Glyph::Droplet, "Droplet"),
        (Glyph::Copy, "Copy"),
        (Glyph::PanelLeft, "PanelLeft"),
        (Glyph::Close, "Close"),
        (Glyph::Pipette, "Pipette"),
        (Glyph::Gradient, "Gradient"),
        (Glyph::More, "More"),
        (Glyph::ArrowUp, "ArrowUp"),
        (Glyph::BookOpen, "BookOpen"),
        (Glyph::Sliders, "Sliders"),
        (Glyph::Underline, "Underline"),
        (Glyph::Strikethrough, "Strikethrough"),
        (Glyph::BulletList, "BulletList"),
        (Glyph::NumberList, "NumberList"),
        (Glyph::PanelTop, "PanelTop"),
        (Glyph::PanelRight, "PanelRight"),
        (Glyph::PanelBottom, "PanelBottom"),
        (Glyph::Wrap, "Wrap"),
        (Glyph::Cursor, "Cursor"),
        (Glyph::Keyframe, "Keyframe"),
    ]
}

// a row that opens a submenu, indented like `menu_item` rows (text 32px from the menu's edge, as in Figma)
pub fn menu_button<R>(ui: &mut egui::Ui, label: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<Option<R>> {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::new(32.0, 4.0);
        ui.menu_button(label, add_contents)
    })
    .inner
}
