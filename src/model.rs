// Document model: shapes and everything a shape can carry, plus the file format.
// Every Shape field has a serde default, so files written by older versions keep loading (see `migrate`).
use std::collections::HashSet;

use egui::{Color32, Pos2, Vec2};
use serde::{Deserialize, Serialize};

pub const GREY_100: Color32 = Color32::from_rgb(0xd9, 0xd9, 0xd9);

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Kind {
    Frame,
    Group, // container with no look of its own; its box always hugs its children (see refit_groups)
    Rect,
    Ellipse, // also arcs and rings: see Shape::arc / Shape::inner
    Line,
    Text,
    Polygon,
    Star,
    Path,
}

impl Kind {
    pub fn is_container(self) -> bool {
        matches!(self, Kind::Frame | Kind::Group)
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum Constraint {
    #[default]
    Min, // left / top
    Max,
    Both,
    Center,
    Scale,
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween, // main axis only
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum TextFit {
    Fixed,
    #[default]
    AutoWidth,
    AutoHeight,
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Darken,
    Multiply,
    ColorBurn,
    Lighten,
    Screen,
    ColorDodge,
    Overlay,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    PlusDarker,  // linear burn: the colours add up and 1 is taken away
    PlusLighter, // linear dodge: the colours add up
    PassThrough, // groups and frames: the layers inside blend with what is below the group (what they do anyway)
}

impl BlendMode {
    pub const ALL: [BlendMode; 19] = [
        BlendMode::PassThrough,
        BlendMode::Normal,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::ColorBurn,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::ColorDodge,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
        BlendMode::PlusDarker,
        BlendMode::PlusLighter,
    ];

    // does it leave the layer as it is drawn (no blending to do)?
    pub fn is_normal(self) -> bool {
        matches!(self, BlendMode::Normal | BlendMode::PassThrough)
    }

    // the CSS / SVG `mix-blend-mode` keyword
    pub fn css(self) -> &'static str {
        match self {
            BlendMode::Normal => "normal",
            BlendMode::Darken => "darken",
            BlendMode::Multiply => "multiply",
            BlendMode::ColorBurn => "color-burn",
            BlendMode::Lighten => "lighten",
            BlendMode::Screen => "screen",
            BlendMode::ColorDodge => "color-dodge",
            BlendMode::Overlay => "overlay",
            BlendMode::SoftLight => "soft-light",
            BlendMode::HardLight => "hard-light",
            BlendMode::Difference => "difference",
            BlendMode::Exclusion => "exclusion",
            BlendMode::Hue => "hue",
            BlendMode::Saturation => "saturation",
            BlendMode::Color => "color",
            BlendMode::Luminosity => "luminosity",
            BlendMode::PlusDarker => "plus-darker",
            BlendMode::PlusLighter => "plus-lighter",
            BlendMode::PassThrough => "normal",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Darken => "Darken",
            BlendMode::Multiply => "Multiply",
            BlendMode::ColorBurn => "Color burn",
            BlendMode::Lighten => "Lighten",
            BlendMode::Screen => "Screen",
            BlendMode::ColorDodge => "Color dodge",
            BlendMode::Overlay => "Overlay",
            BlendMode::SoftLight => "Soft light",
            BlendMode::HardLight => "Hard light",
            BlendMode::Difference => "Difference",
            BlendMode::Exclusion => "Exclusion",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::Color => "Color",
            BlendMode::Luminosity => "Luminosity",
            BlendMode::PlusDarker => "Plus darker",
            BlendMode::PlusLighter => "Plus lighter",
            BlendMode::PassThrough => "Pass through",
        }
    }
}

// how present mode moves from a frame to the one a link points at
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum Transition {
    #[default]
    Instant,
    Dissolve,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    SmartAnimate, // layers with the same name in both frames move, resize and fade between them
}

impl Transition {
    pub const ALL: [(Transition, &'static str); 7] = [
        (Transition::Instant, "Instant"),
        (Transition::Dissolve, "Dissolve"),
        (Transition::SmartAnimate, "Smart animate"),
        (Transition::SlideLeft, "Slide left"),
        (Transition::SlideRight, "Slide right"),
        (Transition::SlideUp, "Slide up"),
        (Transition::SlideDown, "Slide down"),
    ];
}

// what starts an interaction in present mode
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum Trigger {
    #[default]
    Click,
    AfterDelay, // `Shape::delay` seconds after the frame appears
    MouseEnter,
    MouseLeave,
    Drag,     // a drag that starts on the layer
    MouseDown, // the pointer goes down on the layer
    MouseUp,   // the pointer goes up over the layer
    KeyPress,  // `Interaction::key`
    WhileHover, // Change to only: the instance shows the variant while the pointer is over the layer, then goes back
    WhilePress, // Change to only: the same while the pointer is down on the layer
    VideoTime, // the layer's video reaches `delay` seconds
    VideoEnd,  // the layer's video reaches its end (it loops after)
}

impl Trigger {
    // Figma's order
    pub const ALL: [(Trigger, &'static str); 12] = [
        (Trigger::Click, "On click"),
        (Trigger::Drag, "On drag"),
        (Trigger::WhileHover, "While hovering"),
        (Trigger::WhilePress, "While pressing"),
        (Trigger::KeyPress, "Key / gamepad"),
        (Trigger::MouseEnter, "Mouse enter"),
        (Trigger::MouseLeave, "Mouse leave"),
        (Trigger::MouseDown, "Mouse down"),
        (Trigger::MouseUp, "Mouse up"),
        (Trigger::AfterDelay, "After delay"),
        (Trigger::VideoTime, "When video hits a time"),
        (Trigger::VideoEnd, "When video ends"),
    ];
}

// what an interaction does
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum Action {
    #[default]
    Navigate,
    Back,
    OpenLink, // `Shape::url`
    OpenOverlay,  // shows the linked frame on top of the presented one
    CloseOverlay,
    SwapOverlay,  // replaces the topmost overlay with the linked frame
    ScrollTo,     // scrolls the presented frame to the linked layer
    SetVariable,  // `Interaction::var` = `Interaction::value`
    ChangeTo,     // an instance changes to the linked variant
    Video,        // plays, pauses or toggles the video of the linked layer (`IxOpts::video_op`)
    SetMode,      // switches a variable collection to a mode for the presented page (`IxOpts::coll`, `IxOpts::mode`)
}

impl Action {
    pub const ALL: [(Action, &'static str); 11] = [
        (Action::Navigate, "Navigate to"),
        (Action::Back, "Back"),
        (Action::OpenOverlay, "Open overlay"),
        (Action::SwapOverlay, "Swap overlay"),
        (Action::CloseOverlay, "Close overlay"),
        (Action::ScrollTo, "Scroll to"),
        (Action::SetVariable, "Set variable"),
        (Action::ChangeTo, "Change to"),
        (Action::Video, "Play / pause video"),
        (Action::SetMode, "Set variable mode"),
        (Action::OpenLink, "Open link"),
    ];
    // does the action need a frame or layer to point at?
    pub fn needs_target(self) -> bool {
        matches!(self, Action::Navigate | Action::OpenOverlay | Action::SwapOverlay | Action::ScrollTo | Action::ChangeTo | Action::Video)
    }
    // does it animate a change of frame?
    pub fn animates(self) -> bool {
        matches!(self, Action::Navigate | Action::Back | Action::OpenOverlay | Action::SwapOverlay)
    }
}

// the smaller options of an interaction, kept together so a layer's first interaction can carry them too
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct IxOpts {
    pub drag_dir: u8,           // On drag: 0 any direction, 1 left, 2 right, 3 up, 4 down
    pub video_op: u8,           // Play / pause video: 0 toggle, 1 play, 2 pause
    pub keep_scroll: bool,      // Navigate / Back: the frame keeps the scroll position it had (default: starts at the top)
    pub reset_components: bool, // Navigate / Back: interactive components go back to their first variant
    pub reset_video: bool,      // Navigate / Back: videos start again (default: they carry on)
    pub coll: u64,              // Set variable mode: the collection
    pub mode: u64,              // and the mode it is switched to
}

// one interaction of a layer: when it runs and what it does. The layer's own `link`, `trigger`, `action`... fields are its first
// one; `Shape::more` holds the others.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Interaction {
    pub trigger: Trigger,
    pub action: Action,
    pub link: Option<u64>,
    pub transition: Transition,
    pub easing: Easing,
    pub duration: f32,
    pub delay: f32,
    pub url: String,
    pub key: String,
    pub var: Option<u64>,
    pub value: String,
    pub overlay_pos: u8,       // 0 centre, 1..=3 top left / centre / right, 4..=6 bottom, 7 manual (`overlay_off`)
    pub overlay_off: Vec2,     // manual position: the overlay's top-left from the presented frame's top-left
    pub overlay_dim: bool,     // the page behind an overlay is dimmed
    pub close_outside: bool,   // Open overlay: a click outside closes it
    pub condition: String,     // runs only when this expression is true (empty = always); see expr.rs
    pub opts: IxOpts,
}

impl Default for Interaction {
    fn default() -> Self {
        Self { trigger: Trigger::Click, action: Action::Navigate, link: None, transition: Transition::Instant, easing: Easing::default(), duration: 0.3, delay: 0.0, url: String::new(), key: String::new(), var: None, value: String::new(), overlay_pos: 0, overlay_off: Vec2::ZERO, overlay_dim: true, close_outside: true, condition: String::new(), opts: IxOpts::default() }
    }
}

// a frame's Scroll behavior > Overflow in present mode
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum Overflow {
    #[default]
    None,
    Horizontal,
    Vertical,
    Both,
}

impl Overflow {
    pub const ALL: [(Overflow, &'static str); 4] = [(Overflow::None, "No scrolling"), (Overflow::Horizontal, "Horizontal scrolling"), (Overflow::Vertical, "Vertical scrolling"), (Overflow::Both, "Horizontal & vertical scrolling")];
}

// Prototype settings > Device: a bezel drawn around the presented frame
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum Device {
    #[default]
    None,
    Phone,
    Tablet,
    Laptop,
}

impl Device {
    pub const ALL: [(Device, &'static str); 4] = [(Device::None, "No device"), (Device::Phone, "Phone"), (Device::Tablet, "Tablet"), (Device::Laptop, "Laptop")];
}

// the speed curve of a prototype transition
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum Easing {
    Linear,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
    EaseInBack,
    EaseOutBack,
    EaseInOutBack,
    Bouncy, // a spring that overshoots and settles
    SpringGentle, // Figma's spring presets: stiffness / damping 100 / 15, 300 / 20, 600 / 15, 80 / 20 (mass 1)
    SpringQuick,
    SpringBouncy,
    SpringSlow,
}

impl Easing {
    pub const ALL: [(Easing, &'static str); 12] = [
        (Easing::Linear, "Linear"),
        (Easing::EaseIn, "Ease in"),
        (Easing::EaseOut, "Ease out"),
        (Easing::EaseInOut, "Ease in and out"),
        (Easing::EaseInBack, "Ease in back"),
        (Easing::EaseOutBack, "Ease out back"),
        (Easing::EaseInOutBack, "Ease in and out back"),
        (Easing::Bouncy, "Bouncy"),
        (Easing::SpringGentle, "Gentle spring"),
        (Easing::SpringQuick, "Quick spring"),
        (Easing::SpringBouncy, "Bouncy spring"),
        (Easing::SpringSlow, "Slow spring"),
    ];

    // (stiffness, damping) of a spring preset
    pub fn spring(self) -> Option<(f32, f32)> {
        match self {
            Easing::SpringGentle => Some((100.0, 15.0)),
            Easing::SpringQuick => Some((300.0, 20.0)),
            Easing::SpringBouncy => Some((600.0, 15.0)),
            Easing::SpringSlow => Some((80.0, 20.0)),
            _ => None,
        }
    }

    // progress at linear time `t` (0..1); 0 and 1 map to themselves, the back curves and Bouncy overshoot in between
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        const C1: f32 = 1.70158;
        const C2: f32 = C1 * 1.525;
        const C3: f32 = C1 + 1.0;
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t * t,
            Easing::EaseOut => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOut => if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 },
            Easing::EaseInBack => C3 * t * t * t - C1 * t * t,
            Easing::EaseOutBack => 1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2),
            Easing::EaseInOutBack => {
                if t < 0.5 {
                    (2.0 * t).powi(2) * ((C2 + 1.0) * 2.0 * t - C2) / 2.0
                } else {
                    ((2.0 * t - 2.0).powi(2) * ((C2 + 1.0) * (t * 2.0 - 2.0) + C2) + 2.0) / 2.0
                }
            }
            Easing::Bouncy if t >= 1.0 => 1.0,
            Easing::Bouncy => 1.0 - (-6.0 * t).exp() * (t * 14.0).cos(),
            // a damped spring from 0 to 1, run until it has settled (e^-6) by the end of the transition
            _ if t >= 1.0 => 1.0,
            spring => {
                let (k, c) = spring.spring().unwrap_or((100.0, 15.0));
                let (w, z) = (k.sqrt(), c / (2.0 * k.sqrt()));
                let tau = t * 6.0 / (z * w);
                let decay = (-z * w * tau).exp();
                if z < 1.0 {
                    let wd = w * (1.0 - z * z).sqrt();
                    1.0 - decay * ((wd * tau).cos() + z * w / wd * (wd * tau).sin())
                } else if z > 1.0 {
                    let wd = w * (z * z - 1.0).sqrt();
                    1.0 - decay * ((wd * tau).cosh() + z * w / wd * (wd * tau).sinh())
                } else {
                    1.0 - decay * (1.0 + w * tau)
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum ExportFormat {
    #[default]
    Png,
    Jpg,
    Svg,
    Pdf,
    Lottie, // the page's animation as Lottie JSON
    DotLottie, // the same in a dotLottie (.lottie) container
    Webp,
    Gif,  // the Motion timeline as an animated GIF
    Mp4,  // the Motion timeline as H.264 video (ffmpeg)
    Webm, // the Motion timeline as VP9 video (ffmpeg)
}

impl ExportFormat {
    pub fn ext(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpg => "jpg",
            ExportFormat::Svg => "svg",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Lottie => "json",
            ExportFormat::DotLottie => "lottie",
            ExportFormat::Webp => "webp",
            ExportFormat::Gif => "gif",
            ExportFormat::Mp4 => "mp4",
            ExportFormat::Webm => "webm",
        }
    }

    pub const ALL: [(ExportFormat, &'static str); 10] = [
        (ExportFormat::Png, "PNG"),
        (ExportFormat::Jpg, "JPG"),
        (ExportFormat::Svg, "SVG"),
        (ExportFormat::Pdf, "PDF"),
        (ExportFormat::Webp, "WebP"),
        (ExportFormat::Gif, "GIF"),
        (ExportFormat::Mp4, "MP4"),
        (ExportFormat::Webm, "WebM"),
        (ExportFormat::Lottie, "Lottie"),
        (ExportFormat::DotLottie, "dotLottie"),
    ];

    // a picture format with a scale
    pub fn has_scale(self) -> bool {
        matches!(self, ExportFormat::Png | ExportFormat::Jpg | ExportFormat::Webp | ExportFormat::Gif | ExportFormat::Mp4 | ExportFormat::Webm)
    }

    // the name the file gets: Lottie files are ".lottie.json"
    pub fn file_ext(self) -> &'static str {
        if self == ExportFormat::Lottie { "lottie.json" } else { self.ext() }
    }
}

// one file a layer exports to, kept on the layer like Figma's export settings
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportSetting {
    pub format: ExportFormat,
    pub scale: f32,     // PNG only
    pub suffix: String, // added to the layer name: "Icon@2x.png"
    pub ignore_overlap: bool, // Figma's "Ignore overlapping layers" (kept; an export draws every layer either way)
    pub include_id: bool,     // SVG: every layer's group gets an id from its name ("Include "id" attribute")
    pub dim: u8,              // what `scale` is: 0 a multiplier (2x), 1 a width in px (512w), 2 a height in px (512h)
    pub profile: u8,          // colour profile: 0 same as the file, 1 sRGB, 2 Display P3
    pub quality: u8,          // JPG: 0 low, 1 medium, 2 high
    pub basic_resample: bool, // images are scaled with nearest neighbour (hard edges, pixel art) instead of smoothly
    pub outline_text: bool,   // SVG: text becomes shapes
}

impl Default for ExportSetting {
    fn default() -> Self {
        Self { format: ExportFormat::Png, scale: 1.0, suffix: String::new(), ignore_overlap: true, include_id: false, dim: 0, profile: 0, quality: 2, basic_resample: false, outline_text: false }
    }
}

impl ExportSetting {
    pub fn file_name(&self, layer: &str) -> String {
        let clean: String = layer.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c }).collect();
        format!("{clean}{}.{}", self.suffix, self.format.file_ext())
    }
}

// a property the Motion timeline can animate
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Prop {
    X,
    Y,
    Width,
    Height,
    Rotation,
    Opacity,
    Radius, // all four corners
    Fill,   // the first fill, when it is a solid color (rgba 0..255)
    Scale,  // the layer's size is multiplied by this about its centre (1 = as it is); not a field of the layer
    Arc,    // bends the X / Y path between keyframes into an arc: its height as a share of the travel (0 = straight)
}

impl Prop {
    pub const ALL: [(Prop, &'static str); 10] = [
        (Prop::X, "X"),
        (Prop::Y, "Y"),
        (Prop::Width, "Width"),
        (Prop::Height, "Height"),
        (Prop::Rotation, "Rotation"),
        (Prop::Opacity, "Opacity"),
        (Prop::Radius, "Corner radius"),
        (Prop::Fill, "Fill"),
        (Prop::Scale, "Scale"),
        (Prop::Arc, "Arc"),
    ];

    pub fn label(self) -> &'static str {
        Prop::ALL.iter().find(|p| p.0 == self).map_or("", |p| p.1)
    }

    // Scale and Arc exist only as tracks: the layer has no field for them
    pub fn is_virtual(self) -> bool {
        matches!(self, Prop::Scale | Prop::Arc)
    }

    // what a virtual property is when it has no keyframe yet
    pub fn rest(self) -> f32 {
        if self == Prop::Scale { 1.0 } else { 0.0 }
    }
}

// a value at a time on a track; `ease` shapes the way to the next keyframe
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Key {
    pub t: f32, // milliseconds from the start of the page's timeline
    pub v: [f32; 4],
    #[serde(default)]
    pub ease: Easing,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Track {
    pub prop: Prop,
    pub keys: Vec<Key>, // sorted by time
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum PaintKind {
    Solid(Color32),
    Linear { a: Color32, b: Color32, angle: f32 }, // degrees, 0 = left to right; the two-colour gradient older files have
    Gradient(Gradient),
    Image(u64), // Doc::images id, placed by the paint's `image` settings
    Pattern(PatternFill), // a layer repeated as tiles
    Shader(ShaderFill),   // a procedural fill
}

// a Pattern paint: the layer `source` repeated with `space` between the tiles, scaled, turned and shifted
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PatternFill {
    pub source: Option<u64>,
    pub scale: f32,
    pub space: Vec2, // gap between tiles, in the source's own size
    pub rotation: f32,
    pub offset: Vec2,
    pub tile: u8,  // 0 rectangular, 1 hexagonal with rows shifted by half a tile, 2 hexagonal with columns shifted
    pub align: u8, // where the pattern starts in the layer: 0..9 over its 3 x 3 points (top left first)
}

impl Default for PatternFill {
    fn default() -> Self {
        Self { source: None, scale: 1.0, space: Vec2::ZERO, rotation: 0.0, offset: Vec2::ZERO, tile: 0, align: 0 }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum ShaderKind {
    #[default]
    Plasma,
    Waves,
    Noise,
    Checker,
    Stripes,
    Grid,
    Dots, // round dots on a grid: the halftone look
}

impl ShaderKind {
    pub const ALL: [(ShaderKind, &'static str); 7] = [(ShaderKind::Dots, "Dots"), (ShaderKind::Plasma, "Plasma"), (ShaderKind::Waves, "Waves"), (ShaderKind::Noise, "Noise"), (ShaderKind::Checker, "Checker"), (ShaderKind::Stripes, "Stripes"), (ShaderKind::Grid, "Grid")];
}

// a shader effect of a layer (Effects > Shader): a procedural pattern drawn over the layer, see shader_fx.rs
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ShaderEffect {
    pub fill: ShaderFill, // the pattern, its two colours, scale and rotation
    pub visible: bool,
    pub opacity: f32,
    pub blend: BlendMode,
    pub softness: f32,   // blur of the pattern, px
    pub clip_alpha: bool, // keep it inside the layer's shape; off it covers the layer's box
}

impl Default for ShaderEffect {
    fn default() -> Self {
        Self::new(ShaderKind::Dots)
    }
}

impl ShaderEffect {
    pub fn new(kind: ShaderKind) -> Self {
        // the patterns of tiles and dots are drawn on a clear background so the layer shows through; the noise ones fill it, so they multiply
        let noisy = matches!(kind, ShaderKind::Plasma | ShaderKind::Waves | ShaderKind::Noise);
        let fill = ShaderFill { kind, a: if noisy { Color32::from_rgb(0xff, 0xff, 0xff) } else { Color32::TRANSPARENT }, b: if noisy { Color32::from_rgb(0x20, 0x20, 0x40) } else { Color32::BLACK }, scale: 1.0, angle: 0.0 };
        Self { fill, visible: true, opacity: 1.0, blend: if noisy { BlendMode::Multiply } else { BlendMode::Normal }, softness: 0.0, clip_alpha: true }
    }
}

// a Shader paint: a pattern made by the renderer between two colours
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ShaderFill {
    pub kind: ShaderKind,
    pub a: Color32,
    pub b: Color32,
    pub scale: f32,
    pub angle: f32,
}

impl Default for ShaderFill {
    fn default() -> Self {
        Self { kind: ShaderKind::Plasma, a: Color32::from_rgb(0x1a, 0x1a, 0x2e), b: Color32::from_rgb(0xe9, 0x4f, 0x8a), scale: 1.0, angle: 0.0 }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum GradientKind {
    #[default]
    Linear,
    Radial,
    Angular,
    Diamond,
}

impl GradientKind {
    pub const ALL: [(GradientKind, &'static str); 4] = [(GradientKind::Linear, "Linear"), (GradientKind::Radial, "Radial"), (GradientKind::Angular, "Angular"), (GradientKind::Diamond, "Diamond")];
}

// a colour at a position (0..1) along the gradient; the colour carries its own alpha
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct GradStop {
    pub pos: f32,
    pub color: Color32,
}

// Figma's four gradients. Linear runs along `angle` (0 = left to right) through the box; Radial, Angular and Diamond are
// centred at `center` (0..1 of the box); `scale` stretches Radial and Diamond relative to half the box.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Gradient {
    pub kind: GradientKind,
    pub stops: Vec<GradStop>, // sorted by position, at least two
    pub angle: f32,
    pub center: Pos2,
    pub scale: Vec2,
    pub length: f32,     // Linear: how long the gradient line is, 1 = from edge to edge of the box along `angle`
    pub selected: usize, // the stop the picker edits
}

impl Default for Gradient {
    fn default() -> Self {
        Gradient::between(Color32::BLACK, Color32::from_gray(0x66))
    }
}

impl Gradient {
    pub fn between(a: Color32, b: Color32) -> Self {
        Self { kind: GradientKind::Linear, stops: vec![GradStop { pos: 0.0, color: a }, GradStop { pos: 1.0, color: b }], angle: 0.0, center: Pos2::new(0.5, 0.5), scale: Vec2::new(1.0, 1.0), length: 1.0, selected: 0 }
    }

    pub fn sort(&mut self) {
        let sel = self.stops.get(self.selected).copied();
        self.stops.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        if let Some(sel) = sel {
            self.selected = self.stops.iter().position(|s| *s == sel).unwrap_or(0);
        }
    }

    // the colour at `t` (0..1): linear between neighbouring stops
    pub fn at(&self, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        let Some(first) = self.stops.first() else { return Color32::TRANSPARENT };
        if t <= first.pos {
            return first.color;
        }
        for w in self.stops.windows(2) {
            if t <= w[1].pos {
                let span = (w[1].pos - w[0].pos).max(1e-6);
                let k = (t - w[0].pos) / span;
                let (a, b) = (w[0].color.to_array().map(|v| v as f32), w[1].color.to_array().map(|v| v as f32));
                let mix = |i: usize| a[i] + (b[i] - a[i]) * k;
                return Color32::from_rgba_premultiplied(mix(0) as u8, mix(1) as u8, mix(2) as u8, mix(3) as u8);
            }
        }
        self.stops.last().map_or(Color32::TRANSPARENT, |s| s.color)
    }

    // the gradient parameter (0..1 across the gradient) at point `p` of a box, `p` normalised 0..1 in both axes and
    // `aspect` = width / height
    pub fn param(&self, p: Pos2, aspect: f32) -> f32 {
        // work in units where the box is `aspect` wide and 1 tall
        let (dx, dy) = ((p.x - self.center.x) * aspect, p.y - self.center.y);
        match self.kind {
            GradientKind::Linear => {
                // the line runs through `center` along `angle`; length 1 reaches the box's edges
                let (s, c) = self.angle.to_radians().sin_cos();
                let full = (aspect * c.abs() + s.abs()) * self.length.max(1e-3);
                0.5 + (dx * c + dy * s) / full.max(1e-6)
            }
            GradientKind::Radial => {
                let (rx, ry) = (0.5 * aspect * self.scale.x.max(1e-3), 0.5 * self.scale.y.max(1e-3));
                (dx / rx).hypot(dy / ry)
            }
            GradientKind::Diamond => {
                let (rx, ry) = (0.5 * aspect * self.scale.x.max(1e-3), 0.5 * self.scale.y.max(1e-3));
                dx.abs() / rx + dy.abs() / ry
            }
            GradientKind::Angular => {
                let a = (dy.atan2(dx) - self.angle.to_radians()).rem_euclid(std::f32::consts::TAU);
                a / std::f32::consts::TAU
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum ImageMode {
    #[default]
    Fill,
    Fit,
    Crop,
    Tile,
}

impl ImageMode {
    pub const ALL: [(ImageMode, &'static str); 4] = [(ImageMode::Fill, "Fill"), (ImageMode::Fit, "Fit"), (ImageMode::Crop, "Crop"), (ImageMode::Tile, "Tile")];
}

// how an image paint sits in its shape (Figma's image fill): scale mode, a quarter-turn rotation, a tile scale, and the
// image adjustments (each -1..1, 0 = untouched)
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageFill {
    pub mode: ImageMode,
    pub rotation: u16, // 0, 90, 180 or 270
    pub scale: f32,    // Tile: the tile's size relative to the image's own
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub temperature: f32,
    pub tint: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub crop: [f32; 4], // Crop: the part of the image that shows (left, top, right, bottom as 0..1 of the image)
}

impl Default for ImageFill {
    fn default() -> Self {
        Self { mode: ImageMode::Fill, rotation: 0, scale: 0.5, exposure: 0.0, contrast: 0.0, saturation: 0.0, temperature: 0.0, tint: 0.0, highlights: 0.0, shadows: 0.0, crop: [0.0, 0.0, 1.0, 1.0] }
    }
}

impl ImageFill {
    pub fn adjusted(&self) -> bool {
        [self.exposure, self.contrast, self.saturation, self.temperature, self.tint, self.highlights, self.shadows].iter().any(|v| v.abs() > 1e-4)
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Paint {
    pub kind: PaintKind,
    pub visible: bool,
    pub opacity: f32,
    pub style: Option<u64>, // Solid only: follow this ColorStyle instead of the stored color
    pub var: Option<u64>,   // Solid only: follow this colour variable (its value in the layer's modes) instead
    pub image: ImageFill,   // Image only
    pub video: Option<u64>, // Image only: this fill is a video (Doc::videos); its image is the poster frame. Some(0) = chosen, none picked yet
    pub blend: BlendMode,   // how this fill or stroke mixes with what is under it
}

impl Default for Paint {
    fn default() -> Self {
        Paint::solid(GREY_100)
    }
}

impl Paint {
    pub fn solid(c: Color32) -> Self {
        Self { kind: PaintKind::Solid(c), visible: true, opacity: 1.0, style: None, var: None, image: ImageFill::default(), video: None, blend: BlendMode::Normal }
    }
}

// the old single drop shadow; files that have one are converted to an `Effect` when they load
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Shadow {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub color: Color32,
}

impl Default for Shadow {
    fn default() -> Self {
        Self { dx: 0.0, dy: 4.0, blur: 8.0, color: Color32::from_black_alpha(64) }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum EffectKind {
    #[default]
    DropShadow,
    InnerShadow,
    LayerBlur,
    BackgroundBlur,
    Noise,
    Texture,
    Glass,
}

impl EffectKind {
    pub const ALL: [(EffectKind, &'static str); 7] = [
        (EffectKind::InnerShadow, "Inner shadow"),
        (EffectKind::DropShadow, "Drop shadow"),
        (EffectKind::LayerBlur, "Layer blur"),
        (EffectKind::BackgroundBlur, "Background blur"),
        (EffectKind::Noise, "Noise"),
        (EffectKind::Texture, "Texture"),
        (EffectKind::Glass, "Glass"),
    ];
    pub fn is_blur(self) -> bool {
        matches!(self, EffectKind::LayerBlur | EffectKind::BackgroundBlur)
    }
    pub fn label(self) -> &'static str {
        Self::ALL.iter().find(|(k, _)| *k == self).map_or("", |(_, l)| l)
    }
    pub fn is_shadow(self) -> bool {
        matches!(self, EffectKind::DropShadow | EffectKind::InnerShadow)
    }
}

// one effect of a layer (Figma's Effects section). Shadows use offset, blur, spread and colour; blurs use `blur`
// (a blur of 10 is a Gaussian with a standard deviation of 5, as in Figma).
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Effect {
    pub kind: EffectKind,
    pub visible: bool,
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color32,
    pub show_behind: bool, // Drop shadow: also show it under a see-through layer; Texture: clip to the shape
    // Noise: mode 0 mono (colour), 1 duo (colour and colour2), 2 multi (random colours); size in px, density in %; opacity is the colour's alpha
    // Texture: size and radius (the grain is softened by the radius)
    pub mode: u8,
    pub color2: Color32,
    pub size: f32,
    pub density: f32,
    pub radius: f32,
    // Layer blur: a progressive blur goes from `start_blur` at `from` % to `blur` at `to` % along `angle` degrees (0 = top to bottom)
    pub progressive: bool,
    pub start_blur: f32,
    pub from: f32,
    pub to: f32,
    pub angle: f32,
    // variables that drive the colour and the blur
    pub var_color: Option<u64>,
    pub var_blur: Option<u64>,
    pub splay: f32, // Glass: how far the bend spreads towards the middle (0..100)
    #[serde(default)]
    pub var_pos: [Option<u64>; 3], // number variables that drive the offset X, offset Y and spread
    pub blend: BlendMode, // shadows: how the shadow mixes with what is under it
}

impl Effect {
    pub fn new(kind: EffectKind) -> Self {
        // Figma's defaults: a shadow at 0, 4 with blur 4 in 25% black; a blur of 4
        let noise = kind == EffectKind::Noise;
        let glass = kind == EffectKind::Glass;
        Self {
            kind,
            visible: true,
            dx: 0.0,
            dy: if kind.is_shadow() { 4.0 } else { 0.0 },
            blur: if glass { 16.0 } else { 4.0 },
            spread: if glass { 80.0 } else { 0.0 },
            color: if noise { Color32::from_black_alpha(25) } else { Color32::from_black_alpha(64) },
            show_behind: kind == EffectKind::Texture, // Texture clips to the shape unless told otherwise
            mode: 0,
            color2: Color32::from_white_alpha(25),
            size: if noise { 2.0 } else if glass { 50.0 } else { 4.0 },
            density: if glass { 80.0 } else { 50.0 },
            radius: if glass { 20.0 } else { 1.0 },
            progressive: false,
            start_blur: 0.0,
            from: 0.0,
            to: 100.0,
            angle: if glass { -45.0 } else { 0.0 },
            var_color: None,
            var_blur: None,
            var_pos: [None; 3],
            splay: 0.0,
            blend: BlendMode::Normal,
        }
    }
}

impl Default for Effect {
    fn default() -> Self {
        Effect::new(EffectKind::DropShadow)
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum StrokeAlign {
    Inside,
    #[default]
    Center,
    Outside,
}

impl StrokeAlign {
    pub const ALL: [(StrokeAlign, &'static str); 3] = [(StrokeAlign::Inside, "Inside"), (StrokeAlign::Center, "Center"), (StrokeAlign::Outside, "Outside")];
}

// the end of an open stroke
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum StrokeCap {
    #[default]
    None,
    Round,
    Square,
    ArrowLines,
    TriangleArrow,
    ReverseTriangle,
    CircleArrow,
    DiamondArrow,
}

impl StrokeCap {
    pub const ALL: [(StrokeCap, &'static str); 8] = [
        (StrokeCap::None, "None"),
        (StrokeCap::ArrowLines, "Line arrow"),
        (StrokeCap::TriangleArrow, "Triangle arrow"),
        (StrokeCap::ReverseTriangle, "Reversed triangle"),
        (StrokeCap::CircleArrow, "Circle arrow"),
        (StrokeCap::DiamondArrow, "Diamond arrow"),
        (StrokeCap::Round, "Round"),
        (StrokeCap::Square, "Square"),
    ];
    // drawn as a head at the end point rather than as a line cap
    pub fn is_head(self) -> bool {
        matches!(self, StrokeCap::ArrowLines | StrokeCap::TriangleArrow | StrokeCap::ReverseTriangle | StrokeCap::CircleArrow | StrokeCap::DiamondArrow)
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum StrokeJoin {
    #[default]
    Miter,
    Bevel,
    Round,
}

impl StrokeJoin {
    pub const ALL: [(StrokeJoin, &'static str); 3] = [(StrokeJoin::Miter, "Miter"), (StrokeJoin::Bevel, "Bevel"), (StrokeJoin::Round, "Round")];
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoLayout {
    pub stroke_in_layout: bool, // the frame's own stroke counts as padding
    pub vertical: bool,
    pub gap: f32,
    pub pad: [f32; 4], // left, top, right, bottom
    pub main: Align,
    pub cross: Align,
    pub hug_x: bool, // the frame's own width / height follow its content
    pub hug_y: bool,
    pub wrap: bool,   // children flow onto further lines when the frame is full
    pub columns: u32, // > 0: lay children out as a grid with this many columns
    pub first_on_top: bool, // canvas stacking: the first child is drawn above the later ones
    pub baseline: bool,     // horizontal flow: children line up on their text baselines
    pub col_sizes: Vec<f32>, // grid: fixed width of each column (0 or missing = as wide as its content)
    pub row_sizes: Vec<f32>, // grid: fixed height of each row
}

impl Default for AutoLayout {
    fn default() -> Self {
        Self { stroke_in_layout: false, vertical: false, gap: 8.0, pad: [8.0; 4], main: Align::Start, cross: Align::Start, hug_x: true, hug_y: true, wrap: false, columns: 0, first_on_top: false, baseline: false, col_sizes: vec![], row_sizes: vec![] }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum GridKind {
    #[default]
    Grid,
    Columns,
    Rows,
}

// where columns (rows) sit: Figma's Type: Stretch fills the frame between the margins, the others use the fixed width
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum GridAlign {
    #[default]
    Stretch,
    Start, // Left / Top
    End,   // Right / Bottom
    Center,
}

fn yes() -> bool {
    true
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct LayoutGrid {
    pub kind: GridKind,
    pub count: u32,   // Columns / Rows
    pub size: f32,    // Grid cell size
    pub gutter: f32,  // Columns / Rows
    pub margin: f32,  // Columns / Rows
    #[serde(default)]
    pub align: GridAlign, // Columns / Rows
    #[serde(default = "default_cell")]
    pub cell: f32, // Columns / Rows, when not stretched: the width (height) of one
    #[serde(default = "yes")]
    pub visible: bool, // the eye in the Layout guide section
    pub color: Color32,
    #[serde(default)]
    pub var_color: Option<u64>, // a colour variable that drives the guide's colour
    #[serde(default)]
    pub var_num: [Option<u64>; 2], // number variables that drive the grid size and the gutter
}

impl LayoutGrid {
    pub fn new(kind: GridKind) -> Self {
        // Figma's defaults: 5 columns or rows, 10px grid, a 20px gutter, red at 10%
        Self { kind, count: 5, size: 10.0, gutter: 20.0, margin: 0.0, align: GridAlign::Stretch, cell: 64.0, visible: true, color: Color32::from_rgba_unmultiplied(255, 0, 0, 26), var_color: None, var_num: [None; 2] }
    }
}

fn default_cell() -> f32 {
    64.0
}

// one anchor of a bezier contour; handles are offsets from the anchor (zero = sharp)
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct PathPt {
    pub p: Pos2,
    pub hin: Vec2,
    pub hout: Vec2,
    #[serde(default)]
    pub radius: f32, // a sharp corner is rounded to this radius (px); curves ignore it
}

impl PathPt {
    pub fn corner(p: Pos2) -> Self {
        Self { p, hin: Vec2::ZERO, hout: Vec2::ZERO, radius: 0.0 }
    }
}

// Path points live in the unit square of the shape's box, so resizing a path is just changing `size`
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Contour {
    pub pts: Vec<PathPt>,
    pub closed: bool,
    pub hole: bool, // belongs to the previous non-hole contour (boolean results)
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Shape {
    pub id: u64,
    pub parent: Option<u64>, // always a Frame or Group; Vec order = z-order among siblings
    pub name: String,
    pub kind: Kind,
    pub text: String,
    pub pos: Pos2,  // relative to the parent's origin (world position for root shapes)
    pub size: Vec2, // Line: end = pos + size, so it may be negative
    pub rotation: f32, // degrees, about the center; leaf shapes only
    pub radius: [f32; 4], // Rect / Frame corners: top-left, top-right, bottom-right, bottom-left
    pub opacity: f32,
    pub blend: BlendMode,
    pub fills: Vec<Paint>,   // bottom to top
    pub strokes: Vec<Paint>, // bottom to top
    pub stroke_w: f32,
    pub width_points: Vec<(f32, f32)>, // Width profile Custom (5): where along the line (0..1) and how wide (0..1 of the weight), by position
    pub width_profile: u8, // an open path's stroke: 0 the same width all along, 1 taper (thick to thin), 2 reverse taper, 3 lens (thin, thick, thin), 4 pinch (thick, thin, thick), 5 custom (`width_points`)
    pub dash: bool,
    pub dash_cap: u8, // the ends of each dash: 0 flat, 1 round, 2 square
    pub dynamic: Option<Dynamic>, // Stroke > Dynamic: the path is drawn with a wiggle
    pub dash_pattern: Vec<f32>, // [dash, gap, ...] in px; empty = solid, or the default pattern when `dash` is set
    pub stroke_align: StrokeAlign,
    pub stroke_cap: [StrokeCap; 2], // start, end of an open path
    pub stroke_join: StrokeJoin,
    pub miter_angle: f32,           // degrees; sharper corners than this are bevelled
    pub stroke_sides: Option<[f32; 4]>, // individual widths: top, right, bottom, left (rectangles and frames)
    pub shadow: Option<Shadow>,     // only read from old files: becomes an entry of `effects`
    pub effects: Vec<Effect>,       // bottom to top, as listed in the Effects section
    pub shaders: Vec<ShaderEffect>, // shader effects, drawn over the fills (Effects > Shader)
    pub modes: Vec<(u64, u64)>,     // variable modes chosen on this layer: (collection id, mode id); layers inside inherit them
    pub bindings: Vec<(BindProp, u64)>, // properties driven by a variable id
    pub visible: bool,
    pub locked: bool,
    pub clip: bool, // Frame: clip children to its bounds
    pub mask: bool, // masks the siblings above it
    pub corner_smooth: f32, // Corner smoothing 0..1 (Figma's squircle): rectangles and frames
    pub mask_kind: u8, // what of the mask counts: 0 its outline (Vector), 1 its transparency (Alpha), 2 its brightness (Luminance)
    pub lock_aspect: bool,
    // Polygon / Star
    pub sides: u32,
    pub inner: f32, // Star inner radius ratio; Ellipse ring hole ratio
    pub arc: [f32; 2], // Ellipse: start angle, sweep (degrees)
    pub path: Vec<Contour>,
    // Text
    pub font_size: f32,
    pub bold: bool,    // Semi Bold instead of Regular (Font style)
    pub tracking: f32, // letter spacing in % of the font size
    pub valign: u8,    // vertical alignment of the text in a taller box: 0 top, 1 middle, 2 bottom
    pub wrap_style: u8, // Type settings > Wrap style: 0 Auto, 1 Balance (even lines, up to 6), 2 Pretty (no lone word on the last line)
    pub opsz: f32,     // Variable tab: the optical size axis of Inter (14..32); 0 = the font's own default
    pub vtrim: u8,     // vertical trim: 0 standard (the full line box), 1 cap height to baseline
    pub mono: bool,
    pub align: TextAlign,
    pub line_h: f32, // multiplier, 0 = font default
    pub fit: TextFit,
    pub family: String, // an installed font family; empty = Inter (or JetBrains Mono with `mono`)
    pub weight: u16,    // 100..=900; 0 = Regular, or Semi Bold with `bold`
    pub italic: bool,
    pub case: u8,       // 0 as typed, 1 UPPER, 2 lower, 3 Title, 4 Small caps
    pub underline: bool,
    pub strike: bool,
    pub list: u8,       // 0 none, 1 bulleted, 2 numbered
    pub para_spacing: f32, // px between paragraphs
    pub para_indent: f32,  // px of the first line of each paragraph
    pub max_lines: u32,    // 0 = no limit; more lines are cut with an ellipsis
    pub script: u8,        // 0 normal, 1 superscript, 2 subscript
    pub text_dir: u8,      // 0 automatic, 1 left to right, 2 right to left
    pub ul_style: u8,      // underline: 0 solid, 1 dotted, 2 wavy
    pub ul_thick: f32,     // underline thickness in px (0 = the font's)
    pub ul_offset: f32,    // underline distance below the baseline in px (0 = the font's)
    pub ul_skip: bool,     // underline skips the descenders
    pub ul_color: Option<Color32>, // underline colour (None = the text's)
    pub hang: bool,        // hanging punctuation and lists: the marker or opening quote sits outside the text box
    pub list_spacing: f32, // px between the items of a list
    pub axes: Vec<(String, f32)>, // values of an installed variable font's axes (tag, value)
    pub features: String,  // further OpenType features (e.g. "tnum, ss01"): kept and given to the code, not drawn
    pub runs: Vec<TextRun>, // mixed styles inside the text
    pub stretch: Option<StretchBrush>, // a path drawn with a stretch brush: an image stretched along it
    pub text_anim: TextAnim, // how the text appears over the timeline
    #[serde(skip)]
    pub text_t: Option<f32>, // the playhead, while `evaluate` shows the page at a time (not saved)
    // layout
    pub layout: Option<AutoLayout>, // Frame: lay children out automatically
    pub fill_x: bool,               // inside an auto-layout frame: stretch to fill
    pub fill_y: bool,
    pub span: [u32; 2], // inside a grid: how many columns and rows the cell covers
    pub abs: bool,                  // inside an auto-layout frame: ignore the layout (absolute position, uses constraints)
    pub min_size: Vec2,             // auto layout: lower limit of width / height when hugging or filling; 0 = none
    pub max_size: Vec2,             // upper limit; 0 = none
    pub ch: Constraint, // how this shape reacts when its parent frame is resized
    pub cv: Constraint,
    pub grids: Vec<LayoutGrid>,
    // reuse
    pub text_path: Option<u64>,               // a text layer that runs along this path layer's outline
    pub bool_op: Option<u8>,                  // a group that shows its layers combined: 0 union, 1 subtract, 2 intersect, 3 exclude
    pub dev_done: bool,                       // marked completed in Dev Mode
    pub code_connect: String,                 // a component's snippet for developers, with {Property} placeholders
    pub note_of: Option<u64>,                 // a note: the layer it is about (it follows that layer)
    pub note_side: u8,                        // ... and the side of it the note sits on: 0 right, 1 left, 2 top, 3 bottom
    pub no_bg_export: bool,                   // a frame: its fills are left out of exports (Fill > Show in exports unchecked)
    pub desc: String,                         // a main component's description, shown on its instances and in the code
    pub doc_url: String,                      // a link to the component's documentation
    pub ready: bool,                          // marked "ready for dev" (the </> button in the header)
    pub section: bool,                        // a frame drawn as a Section: a titled container that does not clip
    pub slice: bool,                          // a rectangle that only marks a region to export
    pub lib: Option<(u64, u64)>,              // inserted from a library: its id and the layer's id in the library file
    pub slot: bool,                           // a frame inside a component whose content instances may change
    pub slot_min: u32,                        // a slot takes at least this many layers
    pub slot_max: u32,                        // and at most this many (0 = no limit)
    pub slot_pref: Vec<u64>,                  // the components the slot suggests
    pub component_set: bool,                  // a frame holding variant components (its children), drawn with a dashed border
    pub variant: Vec<(String, String)>,       // a variant component: property name -> value, e.g. Size = Large
    pub cprops: Vec<CompProp>,                // properties defined here (a component, or a component set for all its variants)
    pub cprop_link: Option<u64>,              // inside a component: the property (id) that drives this layer
    pub cvals: Vec<(u64, CompPropValue)>,     // an instance's chosen property values
    pub component: bool,    // main component
    pub instance: bool,     // root of an instance
    pub src: Option<u64>,   // inside an instance: the shape this one mirrors
    pub swap: Option<u64>,  // a nested instance swapped to another component: that component. `src` stays the nested instance of the outer main
    // prototype
    pub effect_style: Option<u64>, // follows this EffectStyle (its effects replace the layer's)
    pub text_style: Option<u64>,   // follows this TextStyle
    pub grid_style: Option<u64>,   // follows this GridStyle (layout guides)
    pub link: Option<u64>, // click navigates to this frame in present mode
    pub transition: Transition,
    pub easing: Easing,
    pub duration: f32, // seconds
    pub trigger: Trigger,
    pub action: Action,
    pub delay: f32,   // seconds, for the After delay trigger
    pub url: String,  // for the Open link action
    pub key: String,  // Key press trigger: a key name such as Enter, ArrowLeft or A
    pub var: Option<u64>, // Set variable action
    pub value: String,
    pub overlay_pos: u8,
    pub overlay_off: Vec2,
    pub overlay_dim: bool,
    pub close_outside: bool,
    pub condition: String,     // the first interaction runs only when this is true
    pub more: Vec<Interaction>, // further interactions of this layer
    pub fixed: bool,  // inside a scrolling frame: stays put while the others scroll
    pub opts: IxOpts, // the first interaction's smaller options
    pub code: Option<String>, // a code layer: this frame shows live HTML / CSS / script (in exports and the React code), not its children
    pub repeat: Option<Repeat>, // drawn again and again (Draw's non-destructive transforms)
    pub flow: Option<String>, // a frame that starts a flow: its name
    pub overflow: Overflow,
    // motion: keyframed properties (see anim.rs); the plain fields above are the values without animation
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<Track>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<ExportSetting>,
    // legacy (before multiple paints); only read, converted by `migrate`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Color32>,
}

impl Shape {
    // the share of the stroke weight at `t` (0..1) along an open path: its Width profile, or its own points when it is Custom
    pub fn width_at(&self, t: f32) -> f32 {
        if self.width_profile == 5 { crate::geom::points_width(&self.width_points, t) } else { crate::geom::profile_width(self.width_profile, t) }
    }

    // vertical trim of a text layer: the world-unit space cut above the first line's cap height and below the last baseline
    // (Inter metrics: ascent .969, descent .241, cap height .727 of the size, line box 1.21)
    pub fn trim_px(&self) -> (f32, f32) {
        if self.kind != Kind::Text || self.vtrim == 0 {
            return (0.0, 0.0);
        }
        let size = self.base_style().size;
        let half_leading = if self.line_h > 0.0 { (size * self.line_h - size * 1.21) / 2.0 } else { 0.0 };
        (half_leading + size * (0.969 - 0.727), half_leading + size * 0.241)
    }
    // the variable driving `prop`, if any
    // `v` limited by this layer's min / max on one axis (0 = no limit)
    pub fn limit(&self, axis: usize, v: f32) -> f32 {
        let (lo, hi) = if axis == 0 { (self.min_size.x, self.max_size.x) } else { (self.min_size.y, self.max_size.y) };
        let v = if hi > 0.0 { v.min(hi) } else { v };
        v.max(lo)
    }

    pub fn bound(&self, prop: BindProp) -> Option<u64> {
        self.bindings.iter().find(|(p, _)| *p == prop).map(|(_, v)| *v)
    }

    // drives `prop` from variable `var`, or (None) stops doing so
    pub fn bind(&mut self, prop: BindProp, var: Option<u64>) {
        self.bindings.retain(|(p, _)| *p != prop);
        if let Some(v) = var {
            self.bindings.push((prop, v));
        }
    }
}

impl Shape {
    // every interaction of the layer: the first one (when it has a link) and the further ones
    pub fn interactions(&self) -> Vec<Interaction> {
        let mut v = vec![];
        if self.link.is_some() {
            v.push(self.primary_interaction());
        }
        v.extend(self.more.iter().cloned());
        v
    }

    pub fn primary_interaction(&self) -> Interaction {
        Interaction {
            trigger: self.trigger,
            action: self.action,
            link: self.link,
            transition: self.transition,
            easing: self.easing,
            duration: self.duration,
            delay: self.delay,
            url: self.url.clone(),
            key: self.key.clone(),
            var: self.var,
            value: self.value.clone(),
            overlay_pos: self.overlay_pos,
            overlay_off: self.overlay_off,
            overlay_dim: self.overlay_dim,
            close_outside: self.close_outside,
            condition: self.condition.clone(),
            opts: self.opts,
        }
    }

    pub fn set_primary_interaction(&mut self, ix: Interaction) {
        (self.trigger, self.action, self.link, self.transition, self.easing, self.duration, self.delay) = (ix.trigger, ix.action, ix.link, ix.transition, ix.easing, ix.duration, ix.delay);
        (self.url, self.key, self.var, self.value, self.overlay_pos, self.close_outside, self.condition) = (ix.url, ix.key, ix.var, ix.value, ix.overlay_pos, ix.close_outside, ix.condition);
        (self.overlay_off, self.overlay_dim, self.opts) = (ix.overlay_off, ix.overlay_dim, ix.opts);
    }
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            id: 0,
            parent: None,
            name: String::new(),
            kind: Kind::Rect,
            text: String::new(),
            pos: Pos2::ZERO,
            size: Vec2::ZERO,
            rotation: 0.0,
            radius: [0.0; 4],
            opacity: 1.0,
            blend: BlendMode::Normal,
            fills: vec![Paint::solid(GREY_100)],
            strokes: vec![],
            width_profile: 0,
            width_points: vec![],
            stroke_w: 1.0,
            dash: false,
            dash_cap: 0,
            dynamic: None,
            dash_pattern: vec![],
            stroke_align: StrokeAlign::Center,
            stroke_cap: [StrokeCap::None; 2],
            stroke_join: StrokeJoin::Miter,
            miter_angle: 28.96,
            stroke_sides: None,
            shadow: None,
            effects: vec![],
            shaders: vec![],
            modes: vec![],
            bindings: vec![],
            visible: true,
            locked: false,
            clip: false,
            mask: false,
            corner_smooth: 0.0,
            mask_kind: 0,
            lock_aspect: false,
            sides: 5,
            inner: 0.0,
            arc: [0.0, 360.0],
            path: vec![],
            font_size: 16.0,
            bold: false,
            tracking: 0.0,
            valign: 0,
            vtrim: 0,
            opsz: 0.0,
            wrap_style: 0,
            mono: false,
            align: TextAlign::Left,
            line_h: 0.0,
            fit: TextFit::AutoWidth,
            layout: None,
            fill_x: false,
            fill_y: false,
            abs: false,
            span: [1, 1],
            min_size: Vec2::ZERO,
            max_size: Vec2::ZERO,
            ch: Constraint::Min,
            cv: Constraint::Min,
            grids: vec![],
            text_path: None,
            bool_op: None,
            ready: false,
            dev_done: false,
            code_connect: String::new(),
            note_of: None,
            note_side: 0,
            no_bg_export: false,
            desc: String::new(),
            doc_url: String::new(),
            section: false,
            slice: false,
            component_set: false,
            slot: false,
            slot_min: 0,
            slot_max: 0,
            slot_pref: vec![],
            lib: None,
            variant: vec![],
            cprops: vec![],
            cprop_link: None,
            cvals: vec![],
            component: false,
            instance: false,
            src: None,
            swap: None,
            family: String::new(),
            weight: 0,
            italic: false,
            case: 0,
            underline: false,
            strike: false,
            list: 0,
            para_spacing: 0.0,
            para_indent: 0.0,
            max_lines: 0,
            script: 0,
            text_dir: 0,
            ul_style: 0,
            ul_thick: 0.0,
            ul_offset: 0.0,
            ul_skip: false,
            ul_color: None,
            hang: false,
            list_spacing: 0.0,
            axes: vec![],
            features: String::new(),
            runs: vec![],
            stretch: None,
            text_anim: TextAnim::default(),
            text_t: None,
            effect_style: None,
            text_style: None,
            grid_style: None,
            link: None,
            key: String::new(),
            var: None,
            value: String::new(),
            overlay_pos: 0,
            overlay_off: Vec2::ZERO,
            overlay_dim: true,
            close_outside: true,
            condition: String::new(),
            more: vec![],
            fixed: false,
            repeat: None,
            opts: IxOpts::default(),
            code: None,
            transition: Transition::Instant,
            easing: Easing::EaseOut,
            duration: 0.3,
            trigger: Trigger::Click,
            action: Action::Navigate,
            delay: 0.8,
            url: String::new(),
            flow: None,
            overflow: Overflow::None,
            tracks: vec![],
            exports: vec![],
            fill: None,
            stroke: None,
        }
    }
}

impl Shape {
    pub fn new(kind: Kind, id: u64, n: usize, pos: Pos2, parent: Option<u64>) -> Self {
        let mut s = Shape { id, parent, name: format!("{kind:?} {n}"), kind, pos, ..Shape::default() };
        match kind {
            // Figma defaults: frames white, other shapes #d9d9d9 with no stroke
            Kind::Frame => {
                s.fills = vec![Paint::solid(Color32::WHITE)];
                s.clip = true;
            }
            Kind::Group => s.fills.clear(),
            Kind::Line => {
                s.fills.clear();
                s.strokes = vec![Paint::solid(GREY_100)];
                s.stroke_w = 2.0;
            }
            Kind::Text => {
                s.text = "Text".into();
                s.size = Vec2::new(40.0, 20.0);
            }
            Kind::Polygon => s.sides = 3,
            Kind::Star => {
                s.sides = 5;
                s.inner = 0.4;
            }
            _ => {}
        }
        s
    }
}

// ---- document-level data

// ---- component properties (Figma's boolean, text and instance-swap properties) ----

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum CompPropKind {
    #[default]
    Bool,
    Text,
    Swap,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum CompPropValue {
    Bool(bool),
    Text(String),
    Swap(u64), // the component an instance-swap property shows
}

// a property of a component (or of a component set: then all its variants share it). A layer inside the component linked to it
// (`Shape::cprop_link`) takes its value from each instance: a boolean shows / hides the layer, a text sets its text, an instance
// swap picks the component of a nested instance.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CompProp {
    pub id: u64,
    pub name: String,
    pub kind: CompPropKind,
    pub default: CompPropValue,
}

// ---- variables (Figma's: collections of named values with one value per mode) ----

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum VarKind {
    #[default]
    Color,
    Number,
    Text,
    Bool,
}

impl VarKind {
    pub const ALL: [(VarKind, &'static str); 4] = [(VarKind::Color, "Color"), (VarKind::Number, "Number"), (VarKind::Text, "String"), (VarKind::Bool, "Boolean")];
    pub fn label(self) -> &'static str {
        Self::ALL.iter().find(|(k, _)| *k == self).map_or("", |(_, l)| l)
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum VarValue {
    Color(Color32),
    Number(f32),
    Text(String),
    Bool(bool),
    Alias(u64), // the value of another variable (of the same type) in the layer's modes
}

impl VarValue {
    pub fn default_for(kind: VarKind) -> Self {
        match kind {
            VarKind::Color => VarValue::Color(Color32::from_rgb(0x80, 0x80, 0x80)),
            VarKind::Number => VarValue::Number(0.0),
            VarKind::Text => VarValue::Text(String::new()),
            VarKind::Bool => VarValue::Bool(true),
        }
    }
}

// where a variable may be applied; a variable with no scopes can go anywhere of its type
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum VarScope {
    Fill,
    Stroke,
    Effect,
    Radius,
    Size,
    Spacing,
    StrokeWidth,
    Opacity,
    FontSize,
    Visibility,
    TextContent,
}

impl VarScope {
    pub const NUMBER: [(VarScope, &'static str); 7] = [(VarScope::Radius, "Corner radius"), (VarScope::Size, "Width and height"), (VarScope::Spacing, "Gap and padding"), (VarScope::StrokeWidth, "Stroke width"), (VarScope::Opacity, "Opacity"), (VarScope::FontSize, "Font size"), (VarScope::Effect, "Effect blur")];
    pub const COLOR: [(VarScope, &'static str); 3] = [(VarScope::Fill, "Fill"), (VarScope::Stroke, "Stroke"), (VarScope::Effect, "Effects and guides")];
    pub fn choices(kind: VarKind) -> &'static [(VarScope, &'static str)] {
        match kind {
            VarKind::Color => &Self::COLOR,
            VarKind::Number => &Self::NUMBER,
            VarKind::Text => &[(VarScope::TextContent, "Text content")],
            VarKind::Bool => &[(VarScope::Visibility, "Show / hide")],
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct VarMode {
    pub id: u64,
    pub name: String,
}

// a variable: one value per mode of its collection (same order as `Collection::modes`)
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Variable {
    pub id: u64,
    pub name: String, // "Group/name" puts it in a group
    pub kind: VarKind,
    pub values: Vec<VarValue>,
    #[serde(default)]
    pub scopes: Vec<VarScope>, // empty = anywhere of its type
    #[serde(default)]
    pub syntax: [String; 3], // code syntax as in Dev Mode: web (`--color-primary`), Android, iOS; empty = made from the name
    #[serde(default)]
    pub hidden: bool, // hide from publishing: a published library leaves it out
}

impl Variable {
    pub fn allows(&self, scope: VarScope) -> bool {
        self.scopes.is_empty() || self.scopes.contains(&scope)
    }
    // the group in the name ("Brand/primary" is in "Brand"), and the name without it
    pub fn group(&self) -> (&str, &str) {
        match self.name.rsplit_once('/') {
            Some((g, n)) => (g.trim(), n.trim()),
            None => ("", self.name.as_str()),
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Collection {
    pub id: u64,
    pub name: String,
    pub modes: Vec<VarMode>, // at least one; the first is the default
    pub vars: Vec<Variable>,
    // an extended collection shows the variables of the collection it extends and may override their values per mode
    #[serde(default)]
    pub extends: Option<u64>,
    #[serde(default)]
    pub overrides: Vec<(u64, u64, VarValue)>, // variable id, mode id, value
}

// the properties of a layer a variable can drive
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BindProp {
    Opacity, // a number 0..100
    Radius,
    StrokeWeight,
    Width,
    Height,
    Gap,
    PadX, // left and right padding
    PadY, // top and bottom padding
    FontSize,
    Visible, // a boolean
    Text,    // a string: the text of a text layer
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ColorStyle {
    pub id: u64,
    pub name: String,
    pub color: Color32,
    #[serde(default)]
    pub description: String, // shown when the pointer rests on the style
}

// a stretch brush stroke: the image is laid along the path, as wide as `width`, and bent with it
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StretchBrush {
    pub image: u64, // Doc::images id
    pub width: f32,
}

// Stroke > Dynamic: the stroke of a path is drawn through points spread along it and moved sideways at random
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Dynamic {
    pub frequency: f32, // wiggles per 100 px of path
    pub wiggle: f32,    // how far a point may move, px
    pub smoothen: f32,  // 0..100: how much the corners are rounded off
}

impl Default for Dynamic {
    fn default() -> Self {
        Dynamic { frequency: 10.0, wiggle: 2.0, smoothen: 50.0 }
    }
}

// Draw's linear / radial repeat of a layer
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Repeat {
    pub kind: u8,    // 0 linear, 1 radial
    pub count: u32,  // copies including the layer itself
    pub step: Vec2,  // linear: the move from one copy to the next
    pub angle: f32,  // radial: degrees from one copy to the next
    pub pivot: Vec2, // radial: the center of the turn, from the layer's center
    pub spin: bool,  // radial: the copies turn with it
}

// a text layer's animation on the timeline: its characters, words or lines appear one after the other
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TextAnim {
    pub unit: u8,     // 0 none, 1 character, 2 word, 3 line
    pub kind: u8,     // 0 fade in, 1 typewriter
    pub start: f32,   // ms: when the first unit begins
    pub per_unit: f32, // ms between one unit and the next
    pub duration: f32, // ms a unit takes to fade in
}

// a stretch of a text layer (character positions, `start..end`) with its own style; unset fields follow the layer
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TextRun {
    pub start: usize,
    pub end: usize,
    pub bold: Option<bool>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub size: Option<f32>,
    pub color: Option<Color32>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub family: Option<String>,
    pub url: Option<String>,
}

// shared effects, text settings and layout guides that layers follow, like colour styles
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct EffectStyle {
    pub id: u64,
    pub name: String,
    pub effects: Vec<Effect>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TextStyle {
    pub id: u64,
    pub name: String,
    pub font_size: f32,
    pub bold: bool,
    pub mono: bool,
    pub tracking: f32,
    pub line_h: f32,
    pub align: TextAlign,
    pub valign: u8,
    #[serde(default)]
    pub extra: TextExtra,
}

// the text settings beyond size, weight flag, line height and alignment that a text style carries
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TextExtra {
    pub family: String,
    pub weight: u16,
    pub italic: bool,
    pub case: u8,
    pub underline: bool,
    pub strike: bool,
    pub list: u8,
    pub para_spacing: f32,
    pub para_indent: f32,
    pub script: u8,
}

impl TextExtra {
    pub fn of(s: &Shape) -> Self {
        Self { family: s.family.clone(), weight: s.weight, italic: s.italic, case: s.case, underline: s.underline, strike: s.strike, list: s.list, para_spacing: s.para_spacing, para_indent: s.para_indent, script: s.script }
    }
    pub fn apply(&self, s: &mut Shape) {
        (s.family, s.weight, s.italic, s.case, s.underline, s.strike, s.list, s.para_spacing, s.para_indent, s.script) = (self.family.clone(), self.weight, self.italic, self.case, self.underline, self.strike, self.list, self.para_spacing, self.para_indent, self.script);
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GridStyle {
    pub id: u64,
    pub name: String,
    pub grids: Vec<LayoutGrid>,
}

// an animation style: a named easing (and, when it has one, the time a keyframe takes to reach the next) to put on keyframes
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AnimStyle {
    pub id: u64,
    pub name: String,
    pub ease: Easing,
    pub duration: f32, // ms; 0 = keep the time between the keyframes
}

#[derive(Clone, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtStyles {
    pub effects: Vec<EffectStyle>,
    pub texts: Vec<TextStyle>,
    pub grids: Vec<GridStyle>,
    pub anims: Vec<AnimStyle>,
    pub descriptions: Vec<(u64, String)>, // what each style is for (shown when the pointer rests on it)
}

impl ExtStyles {
    pub fn description(&self, id: u64) -> &str {
        self.descriptions.iter().find(|d| d.0 == id).map_or("", |d| d.1.as_str())
    }

    pub fn set_description(&mut self, id: u64, text: String) {
        self.descriptions.retain(|d| d.0 != id);
        if !text.is_empty() {
            self.descriptions.push((id, text));
        }
    }

    pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.effects.iter().map(|s| s.id).chain(self.texts.iter().map(|s| s.id)).chain(self.grids.iter().map(|s| s.id)).chain(self.anims.iter().map(|s| s.id))
    }
}

impl TextStyle {
    pub fn from_shape(id: u64, name: String, s: &Shape) -> Self {
        Self { id, name, font_size: s.font_size, bold: s.bold, mono: s.mono, tracking: s.tracking, line_h: s.line_h, align: s.align, valign: s.valign, extra: TextExtra::of(s) }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Guide {
    pub vertical: bool,
    pub pos: f32,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Page {
    pub name: String,
    pub shapes: Vec<Shape>,
    pub guides: Vec<Guide>,
    pub background: Color32, // its alpha is the page's opacity (Page > opacity)
    pub bg_visible: bool,    // the eye in the Page section: off shows no page colour at all
    pub anim_ms: f32, // length of the Motion timeline
    pub anim_loop: bool,
    pub proto_device: Device,
    pub proto_bg: Color32, // Prototype settings > Background color: around the presented frame
    pub proto_scale: u8,   // Prototype settings > Content scaling: 0 fit, 1 fit width, 2 fill screen, 3 100%
    pub audio: Vec<AudioClip>, // sounds on the Motion timeline
    pub annotations: Vec<Annotation>, // Dev Mode notes pinned to layers
    pub measures: Vec<(u64, u64)>,    // Dev Mode distances kept on the canvas: the two layers
}

// a note for the developer, pinned to a layer in Dev Mode
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Annotation {
    pub id: u64,
    pub target: u64,
    pub text: String,
    pub label: String,
}

// a sound on a page's timeline: the file (base64, kept in the document), when it starts, how loud
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioClip {
    pub id: u64,
    pub name: String,
    pub data: String,
    pub start: f32, // ms
    pub gain: f32,
}

impl Default for Page {
    fn default() -> Self {
        Self { name: "Page 1".into(), shapes: vec![], guides: vec![], background: crate::theme::SUNKEN(), bg_visible: true, anim_ms: 2000.0, anim_loop: true, proto_device: Device::None, proto_bg: Color32::BLACK, proto_scale: 0, audio: vec![], annotations: vec![], measures: vec![] }
    }
}

// a branch of the file: its pages, the pages as they were when it was made (what a merge compares against), and which branch it
// was made from. The pages of the branch being worked on are the document's own (those stored here are from when it was left).
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Branch {
    pub name: String,
    pub pages: Vec<Page>,
    pub base: Vec<Page>,
    pub from: Option<usize>,
    #[serde(default)]
    pub review: Review,
}

// a review of a branch before it is merged: asked of someone, then approved or sent back with a note
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Review {
    pub state: u8,       // 0 none asked, 1 requested, 2 approved, 3 changes requested
    pub reviewer: String, // who was asked
    pub by: String,       // who answered
    pub note: String,     // what they wrote
    pub secs: u64,        // unix time of the last step
}

// a library this document has added: where its file is, which version came in, and how its ids became this document's
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct LibRef {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub version: u64,
    pub map: Vec<(u64, u64)>, // library id -> id in this document, for styles, variables, collections and modes
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Version {
    pub name: String,
    pub secs: u64, // unix time
    pub shapes: Vec<Shape>, // the page that was current when saved
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub author: String,
    pub secs: u64, // unix time
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<u64>, // pictures (Doc::images ids)
    #[serde(default)]
    pub edited: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub id: u64,
    pub page: usize,
    pub pos: Pos2,
    pub text: String,
    pub resolved: bool,
    #[serde(default)]
    pub author: String, // empty in files from before comments had authors
    #[serde(default)]
    pub secs: u64, // unix time it was written
    #[serde(default)]
    pub replies: Vec<Reply>,
    #[serde(default)]
    pub attachments: Vec<u64>, // pictures on the first message
    #[serde(default)]
    pub edited: bool,
    #[serde(default)]
    pub reactions: Vec<Reaction>, // emoji people put on its messages
}

// an emoji on one message of a comment (0 is the comment's own text, then the replies), with who put it there
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    pub msg: usize,
    pub emoji: String,
    pub who: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub data: String, // base64 of the original file
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Doc {
    pub pages: Vec<Page>,
    pub current: usize,
    pub styles: Vec<ColorStyle>,
    pub ext_styles: ExtStyles,
    pub variables: Vec<Collection>,
    pub versions: Vec<Version>,
    pub comments: Vec<Comment>,
    pub images: Vec<Asset>,
    pub videos: Vec<Asset>, // the files behind Video fills (base64)
    pub next_id: u64,
    pub libraries: Vec<LibRef>,
    pub branches: Vec<Branch>, // empty until the first branch is made: the file is then just "main"
    pub cur_branch: usize,
    pub lib_id: u64,      // the identity of this file as a library (0 until it is published)
    pub lib_version: u64,
    pub color_profile: u8, // 0 sRGB, 1 Display P3
    pub visits: Vec<(String, u64)>, // the viewer history: who joined a live session, and when
    pub lib_description: String, // what changed in the next publish of this file as a library
}

// a bare array of shapes is what older versions saved
#[derive(Deserialize)]
#[serde(untagged)]
pub enum File {
    Old(Vec<Shape>),
    Doc(Doc),
}

// makes older files usable: legacy colors become paints, unique ids, names, parents that point at containers
pub fn migrate(mut shapes: Vec<Shape>) -> Vec<Shape> {
    let mut seen = HashSet::new();
    let mut next = shapes.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    for (n, s) in shapes.iter_mut().enumerate() {
        if s.id == 0 || !seen.insert(s.id) {
            s.id = next;
            next += 1;
            seen.insert(s.id);
        }
        if s.name.is_empty() {
            s.name = format!("{:?} {}", s.kind, n + 1);
        }
        if let Some(sh) = s.shadow.take() {
            // the old single drop shadow becomes the first effect (same offset, blur and colour)
            s.effects.insert(0, Effect { dx: sh.dx, dy: sh.dy, blur: sh.blur, color: sh.color, ..Effect::new(EffectKind::DropShadow) });
        }
        if s.fill.is_some() || s.stroke.is_some() {
            let (fill, stroke) = (s.fill.take(), s.stroke.take());
            s.fills = if s.kind == Kind::Line { vec![] } else { fill.map(Paint::solid).into_iter().collect() };
            s.strokes = if s.stroke_w > 0.0 { stroke.map(Paint::solid).into_iter().collect() } else { vec![] };
            if s.kind == Kind::Line {
                s.strokes = vec![Paint::solid(stroke.unwrap_or(GREY_100))];
            }
            if s.kind == Kind::Text {
                // text used to be sized by its box height and coloured by `fill`
                s.font_size = (s.size.y * 0.75).max(1.0);
                s.fit = TextFit::Fixed;
            }
        }
    }
    let containers: HashSet<u64> = shapes.iter().filter(|s| s.kind.is_container()).map(|s| s.id).collect();
    for s in &mut shapes {
        if s.parent.is_some_and(|p| !containers.contains(&p)) {
            s.parent = None;
        }
    }
    shapes
}

impl BindProp {
    // where in the interface this property's variables are applied
    pub fn scope(self) -> VarScope {
        match self {
            BindProp::Opacity => VarScope::Opacity,
            BindProp::Radius => VarScope::Radius,
            BindProp::StrokeWeight => VarScope::StrokeWidth,
            BindProp::Width | BindProp::Height => VarScope::Size,
            BindProp::Gap | BindProp::PadX | BindProp::PadY => VarScope::Spacing,
            BindProp::FontSize => VarScope::FontSize,
            BindProp::Visible => VarScope::Visibility,
            BindProp::Text => VarScope::TextContent,
        }
    }
}
