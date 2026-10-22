#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")] // no console window behind a release build
#![allow(dead_code)] // some helpers are only used by parts that are not in this tree
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use eframe::egui::{self, Color32, Key, Modifiers, Pos2, Rect, Vec2};

mod actions;
mod kbd;
mod smart_sel;
mod code_layer;
mod sketchimport;
mod anim_export;
mod corner_handles;
mod profile;
mod comp_more;
mod slots;
mod spell;
mod quick;
mod repeat;
mod vtools;
mod figfile;
mod figimport;
mod kiwi;
mod video;
mod anim;
mod api;
mod api_more;
mod branches;
mod audio;
mod canvas;
mod comments;
mod comp;
mod connect;
mod devmode;
mod export;
mod files;
mod expr;
mod fig;
mod gallery;
mod geom;
mod gradient_edit;
mod glyphs;
mod inspect;
mod layout;
mod layout_handles;
mod library;
mod lottie;
mod merge;
mod model;
mod ops;
mod pen;
mod panels;
mod picker;
mod present;
mod presets;
mod proto_html;
mod props;
mod render;
mod shader_fx;
mod strokefx;
mod suggest;
mod svg_import;
mod textrun;
mod variables;
mod variants;
mod variants_ui;
mod vector;
#[cfg(test)]
mod tests;
mod theme;
mod widgets;

use model::*;

// ponytail: default save path; "Save as" / "Open" use a native file dialog
const FILE: &str = "design.json";
const HANDLE: f32 = 8.0;

// Figma dark-theme tokens, read from the editor's own CSS (ramp-grey-*, ramp-blue-500, radius-*, text ramps).
#[allow(non_snake_case)]
fn BG() -> Color32 {
    theme::SURFACE() // panels, toolbar
}
#[allow(non_snake_case)]
fn BG_SECONDARY() -> Color32 {
    theme::FIELD() // hover, inputs
}
#[allow(non_snake_case)]
fn BG_PRESSED() -> Color32 {
    theme::LINE() // pressed, borders
}
#[allow(non_snake_case)]
fn BG_MENU() -> Color32 {
    theme::SUNKEN() // menus, canvas
}
const BRAND: Color32 = Color32::from_rgb(0x0d, 0x99, 0xff); // blue-500
const LAYOUT_INK: Color32 = Color32::from_rgb(0xf2, 0x48, 0xc4); // padding / gap handles (placeholder, not measured yet)
const GUIDE: Color32 = Color32::from_rgb(0xf2, 0x48, 0x22); // snap lines and guides
// ponytail: approximation (brand blue at 30% over BG()); Figma's pale-blue-800 was not read from the CSS
const SELECTED_ROW: Color32 = Color32::from_rgb(0x39, 0x43, 0x60); // Figma's selected row (measured)
#[allow(non_snake_case)]
fn TEXT() -> Color32 {
    theme::TEXT()
}
#[allow(non_snake_case)]
fn TEXT_SECONDARY() -> Color32 {
    theme::TEXT_SECONDARY()
}
#[allow(non_snake_case)]
fn TEXT_TERTIARY() -> Color32 {
    theme::TEXT_TERTIARY()
}
const R_SMALL: u8 = 2;
const R_MEDIUM: u8 = 5;
const R_LARGE: u8 = 18; // cards: the toolbar pill, popovers, dialogs, comment cards (Figma measures 13; rounder by request)
const R_MENU: u8 = 12; // menus and dropdown lists

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tool {
    Select,
    Draw(Kind),
    Pen,
    Pencil,
    Hand, // drag to pan
    Comment,
}

// Figma's toolbelt modes: what the toolbar offers and how the canvas and right panel behave
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Design,
    Draw,   // freehand: pencil and pen up front
    Motion, // prototyping: Prototype panel, every link arrow shown
    Dev,    // read-only: select and inspect, CSS, redlines
}

impl Mode {
    const ALL: [(Mode, &'static str, &'static str); 4] = [(Mode::Design, "Design", ""), (Mode::Draw, "Draw", ""), (Mode::Motion, "Motion", ""), (Mode::Dev, "Dev Mode", "Shift+D")];
}

#[derive(Clone, Copy)]
enum Drag {
    None,
    Move { raw: Vec2, bounds: Rect }, // pointer travel so far, selection bounds when the drag began
    Corners(Pos2),                    // draw tools and line ends: fixed world corner, the other follows the pointer
    Resize { h: (i8, i8), rect: Rect }, // one shape, handle direction, its unrotated rect when the drag began
    Scale { h: (i8, i8), bounds: Rect }, // groups and multi-selections; originals in App::scale_orig
    Rotate { center: Pos2, start: f32, base: f32 },
    Marquee(Pos2), // world point where the box started
    Lasso,         // a free-hand outline in App::lasso
    VTool,         // a drag of the Cut or Eraser tool (points in App::vtool_pts)
    Radius { k: usize }, // a corner radius handle (Alt: that corner only)
    Smart { vertical: bool, start: f32, from: f32 }, // Smart Selection: a gap handle between layers of a row
    SmartMove { vertical: bool, id: u64, grab: f32 }, // Smart Selection: a layer dragged along its row (Ctrl)
    Guide(usize),
    PenHandle, // dragging the handles out of the point just placed
    PenClose,  // pressed on the path's first point: release closes it, dragging first shapes the closing corner
    Node { c: usize, i: usize, part: u8 }, // path editing: part 0 anchor, 1 handle in, 2 handle out
    Bend { c: usize, i: usize, from: Pos2 }, // path editing with the Bend tool: pulling handles out of a vertex (a click straightens it)
    Pencil,                               // freehand stroke; points in App::pencil
    Gradient(u8, usize),                  // a gradient handle (gradient_edit.rs): which, and the stop for a stop
    Crop(layout_handles::CropDrag),       // an edge handle of an image fill in Crop mode
    Layout(layout_handles::LayoutDrag),   // a padding or gap strip of the selected auto-layout frame
    Connect(u64),                         // the Prototype tab's connection node of this layer, being dragged to a frame
}

// what one undo step puts back: the layers and the selection, and with them the guides, styles, variables and comments
#[derive(Clone, PartialEq)]
pub(crate) struct Snap {
    shapes: Vec<Shape>,
    sel: Vec<u64>,
    guides: Vec<Guide>,
    styles: Vec<ColorStyle>,
    ext: ExtStyles,
    variables: Vec<Collection>,
    comments: Vec<Comment>,
    page: usize,                // the page these layers belong to: undoing an edit goes back to it
    page_look: (Color32, bool), // the page's colour (with its opacity) and eye
    pages: Option<Vec<Page>>,   // set by page add / delete / move / duplicate / rename: every page, to put them all back
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Design,
    Prototype,
    Motion, // Motion mode: keyframes of the selection
}

#[derive(Clone, Copy, PartialEq)]
enum LeftTab {
    Layers,
    Assets,
    History,
    Comments,
}

// the canvas as rendered by resvg, reused until the document or view changes
struct BlendCache {
    shapes: Vec<Shape>,
    pan: Vec2,
    zoom: f32,
    px: [u32; 2],
    background: Color32,
    tex: egui::TextureHandle,
}

// a prototype transition in progress: `from` is the frame being left
#[derive(Clone, Copy)]
struct PresentAnim {
    from: u64,
    t0: f64,
    tr: Transition,
    ease: Easing,
    dur: f32,
}

struct App {
    shapes: Vec<Shape>, // the current page
    pages: Vec<Page>,   // every page; the current one's `shapes` is only refreshed on save / switch
    cur: usize,
    guides: Vec<Guide>, // the current page's
    styles: Vec<ColorStyle>,
    ext: ExtStyles, // effect, text and grid styles
    variables: Vec<Collection>, // Figma's variables (variables.rs)
    vars_open: bool,            // the Variables window
    var_coll: usize,            // the collection it shows
    versions: Vec<Version>,
    branches: Vec<Branch>, // see branches.rs
    cur_branch: usize,
    branches_open: bool,
    files: files::FilesUi, // the files browser window
    pending_attach: Vec<u64>, // pictures chosen for the comment being written
    suggestions: Option<(u64, Vec<suggest::Suggestion>)>, // Suggest auto layout: the frame and what it could become
    audio_peaks: std::collections::HashMap<u64, (Vec<f32>, f32)>, // a clip's waveform and length
    audio_out: Option<(rodio::MixerDeviceSink, Vec<rodio::Player>)>, // the sound that is playing
    libs: Vec<LibRef>,                                  // libraries this file has added
    lib_id: u64,                                        // this file's identity as a library (0 until published)
    lib_version: u64,
    lib_files: std::collections::HashMap<u64, library::Library>, // the added libraries' files, read when needed
    comments: Vec<Comment>,
    images: Vec<Asset>,
    videos: Vec<Asset>,                      // the video files behind Video fills (video.rs)
    video_ctl: HashMap<u64, video::VideoCtl>, // videos an interaction paused or restarted
    video_last: HashMap<u64, f32>,           // each playing video's position at the last check (for its time / end triggers)
    present_scrolls: HashMap<u64, Vec2>,     // the scroll each frame had when it was left
    present_dir: u8,                         // the direction of the drag being run through On drag interactions (1 left, 2 right, 3 up, 4 down)
    drag_origin: Option<Pos2>,               // where a drag in present mode began, until it has gone far enough to have a direction
    clips: HashMap<u64, video::Clip>,        // videos being played in present mode, cut into frames
    tex: HashMap<u64, egui::TextureHandle>,
    sprites: std::cell::RefCell<HashMap<u64, (egui::TextureHandle, u64)>>, // rasters of layers with effects and the like (render.rs)
    sel: Vec<u64>,
    base_sel: Vec<u64>, // selection a shift-marquee adds to
    row_drag: Option<u64>, // layers-panel row being dragged
    col_paint: Option<(bool, bool)>, // a drag down the eye (true) or lock (false) column: the state it sets on every row it crosses
    next_id: u64,
    collapsed: HashSet<u64>, // layers-panel state only: not saved, not undoable
    tool: Tool,
    drag: Drag,
    pan: Vec2,
    zoom: f32,
    undo: Vec<Snap>,
    saved_undo: usize, // how many undo steps there were when the file was last saved or opened: more or fewer means unsaved edits
    redo: Vec<Snap>,
    pending: Option<Snap>, // pre-gesture snapshot, committed once the pointer is released
    grad_edit: Option<(u64, bool, usize)>, // the gradient paint whose handles are on the canvas: layer id, is a stroke, paint index
    undid: bool,                 // an undo / redo ran this frame (not to be recorded as an edit)
    status: String,
    themed: bool,
    file: PathBuf,
    clipboard: Vec<Shape>,
    search_open: bool, // the left panel shows the search view (the magnifier next to Pages)
    search_query: String,
    search_focused: bool,
    page_drag: Option<usize>, // a page row being dragged to a new place
    props_clip: Option<Shape>, // Ctrl+Alt+C: the layer whose look Ctrl+Alt+V pastes
    sizes: HashMap<u64, Vec2>, // last seen frame sizes, to apply constraints when they change
    scale_orig: Vec<Shape>,    // shapes as they were when a Scale drag began
    snap: bool,
    rulers: bool,
    snap_lines: Vec<Guide>, // smart guides to draw this frame
    pen: Vec<PathPt>,       // path being drawn, world coordinates
    pencil: Vec<Pos2>,      // freehand stroke being drawn, world coordinates
    pencil_force: Vec<f32>, // pressure (0..1) at each pencil point
    brush_w: f32,           // Draw toolbar: stroke width of new pencil strokes
    brush: vector::Brush,   // Draw toolbar: what a pencil stroke becomes
    nib: f32,               // Calligraphy nib angle, degrees (pens do not report tilt through winit)
    mode: Mode,
    node_edit: bool,
    node_sel: Option<(usize, usize)>,
    editing_text: Option<u64>,
    text_sel: Option<(u64, usize, usize)>, // the layer being edited and the characters selected in it
    renaming: Option<u64>,
    layer_anchor: Option<u64>, // the row a Shift-click in the layers panel selects a range from
    nudge_small: f32,      // Preferences > Nudge amount: arrow keys
    nudge_big: f32,        // and Shift+arrow keys
    vtool: Option<vtools::VTool>, // Shape builder / Cut / Eraser / Paint, used with the Select tool
    vtool_pts: Vec<Pos2>,         // the line of Cut or the path of Eraser, world coordinates
    builder: Option<vtools::Builder>, // the regions Shape builder and Paint work on
    handle_mirror: u8,     // Preferences > Mirror handles: 0 none, 1 angle (when the node is smooth), 2 angle and length
    renaming_page: Option<usize>,
    present: Option<u64>,
    present_anim: Option<PresentAnim>,
    present_stack: Vec<u64>, // frames visited, for the Back action
    present_overlays: Vec<present::Overlay>, // overlays open on top of the presented frame, bottom first
    present_shapes: Option<Vec<Shape>>,      // the page as it was when presenting began (interactive components change it), put back at the end
    present_vars: Option<Vec<Collection>>,   // the variables as they were when presenting began, put back when it ends
    present_scroll: Vec2,    // Scroll behavior: how far the presented frame's content has been scrolled
    present_while: Vec<present::WhileState>, // While hovering / pressing changes in force
    present_hover: Option<usize>, // the shape with a Mouse enter / leave interaction under the pointer
    present_fired: bool,     // the After delay interactions of this frame have run
    open_url: Option<String>,
    present_t0: Option<f64>, // when the presented frame appeared: its keyframe animation runs from there
    anim_time: f32,                    // Motion timeline playhead, ms
    anim_frames: bool,                 // the timeline shows frames (anim::FPS) instead of milliseconds
    tl_view: (f32, f32),               // timeline zoom (1 = the whole length) and the time at its left edge, ms
    anim_play: Option<(f64, f32)>,     // playing since (input time, playhead then)
    auto_key: bool,                    // edits of values without a track create one
    key_sel: Option<(u64, Prop, f32)>, // selected keyframe: shape, property, time
    saved_view: (Vec2, f32),
    tab: Tab,
    left_tab: LeftTab,
    layer_filter: String,
    export_scale: f32,
    export_fmt: export::Format,
    version_name: String,
    comment_sel: Option<u64>,                // the comment whose thread is open
    new_comment: Option<(Pos2, String)>,     // the composer: where the comment goes, and what is typed
    gallery: bool, // `--gallery`: show the component gallery instead of the editor
    show_resolved: bool,                     // pins and list rows of resolved comments
    paste_n: f32,
    view: Rect, // where the canvas is on screen
    show_help: bool,
    left_min: bool, // the left panel is minimised to a floating bar
    blend_cache: Option<BlendCache>,
    last_shape: Kind, // the shape tool the toolbar button offers
    shown_status: String, // the toast currently on screen
    status_t: f64,
    api: Option<api::Api>, // the local REST API (--api, or Actions > Toggle REST API)
    api_owned: HashSet<u64>, // top-level layers made through the API: the agent may edit them without a claude_edit copy
    api_want: Option<u16>, // start it on the next frame (needs the egui context); 0 = any free port
    ctx: Option<egui::Context>,
    net_cursor: Option<Pos2>, // this peer's pointer in world space, sent to the others
    user: String, // the name other people see in a live session
    zoom_text: String, // the zoom menu's field
    zoom_fresh: bool,  // the zoom menu was just opened: focus and select its field
    show_comments: bool, // comment pins on the canvas (View > Comments)
    section_tool: bool,   // the frame tool draws a Section (Frame tools > Section)
    slice_tool: bool,     // the rectangle drawn is a Slice
    presets_open: HashSet<String>, // open groups of the Frame tool's presets
    var_tick: Option<u64>,        // the variable whose tick was just clicked
    eyedrop_var: bool,            // the next eyedropper pick makes a colour variable
    var_sel: Vec<u64>,            // variables ticked in the Variables window for bulk edits
    multi_variants: Option<u64>, // Q: the component set whose variants are edited together
    color_profile: u8,    // File > Color profile: 0 sRGB, 1 Display P3
    visits: Vec<(String, u64)>, // viewer history: who joined a live session and when (unix seconds)
    following: Option<u64>,     // the peer whose view this one follows (spotlight)
    spotlight_on: bool,         // this person asked the others to follow them
    chat_open: bool,            // cursor chat: the message field is open
    chat_text: String,
    chat_send: Option<String>,  // typed, to go out with the next poll
    my_chat: Option<(String, f64)>, // the message shown beside this person's own cursor
    history_open: bool,
    page_modes: Vec<(u64, u64)>, // Set variable mode in present mode: the mode each collection has for the whole page
    lib_description: String, // the note that goes out with the next library publish
    smart_ids: std::cell::RefCell<Vec<u64>>, // the row Smart Selection is working on, in order
    smart_anchor: std::cell::Cell<(f32, f32)>, // where the row starts along its axis, and its gap
    kbox: Option<Rect>,   // the selection box the keyboard grows (Ctrl+Alt+arrows)
    light_ui: bool,       // Preferences > Light interface
    light_applied: bool,  // what the egui styles were last built for
    spell: bool,          // Preferences > Check spelling: red wavy lines under misspelled words
    quick_open: bool,     // Quick insert (Shift+I)
    quick_query: String,
    quick_sel: usize,
    quick_focused: bool,
    hide_ui: bool,        // Ctrl+\: panels and toolbar out of the way
    prop_labels: bool,    // View > Property labels: the small captions over the Design panel's fields
    present_inline: bool, // Shift+Space: the prototype runs inside the editor, panels kept
    actions_open: bool,   // the Actions palette (Ctrl+/ or the toolbar button)
    actions_query: String,
    actions_sel: usize,
    actions_focused: bool,
    eyedropper: bool,     // Eyedropper (I): the next click on the canvas picks a color
    node_connect: Option<(usize, usize)>, // vector editing: Connect was pressed on this vertex; the next vertex clicked joins it
    pen_listening: bool,  // the pen's own reports (tilt) are being read from the window
    custom_brush: Option<Vec<Shape>>, // the layers Create brush kept: the Layer brush scatters copies of them
    stretch_image: Option<u64>, // the picture the Stretch brush lays along its stroke
    bend_tool: bool,      // Pen tools > Bend: in vector editing, dragging a vertex bends it, a click straightens it
    bend_moved: bool,     // the Bend drag in progress has left its start
    lasso_tool: bool,     // Move tools > Lasso: drag a free-hand outline round the layers to select
    lasso: Vec<Pos2>,     // the outline being drawn, world coordinates
    scale_tool: bool,     // Scale tool (K): resize handles scale children, strokes and text with the layer
    arrow_tool: bool,     // the line tool draws an arrow (Shape tools > Arrow)
    proto_settings: bool, // the Prototype tab shows Prototype settings
    snap_pixels: bool,   // View > Snap to pixel grid: moves and resizes land on whole pixels
    layout_guides: bool, // View > Layout guides: draw the layers' layout grids
    labels: bool,        // View > Additional labels
    pixel_preview: u8,   // 0 disabled, 1 = 1x, 2 = 2x (the choice is kept; the canvas is not rasterised at pixel density yet)
    outlines: bool,      // View > Outlines > Show outlines: wireframe view
    outline_hidden: bool,
    outline_bounds: bool,
    pixel_grid: bool,    // a 1px grid when zoomed in far enough
    share_open: bool,    // the Share dialog
    share_addr: bool,    // its "Join by address" row is open
}

impl Default for App {
    fn default() -> Self {
        Self {
            shapes: vec![],
            pages: vec![Page::default()],
            cur: 0,
            guides: vec![],
            styles: vec![],
            ext: ExtStyles::default(),
            variables: vec![],
            vars_open: false,
            var_coll: 0,
            versions: vec![],
            branches: vec![],
            cur_branch: 0,
            branches_open: false,
            files: Default::default(),
            pending_attach: vec![],
            suggestions: None,
            audio_peaks: Default::default(),
            audio_out: None,
            libs: vec![],
            lib_id: 0,
            lib_version: 0,
            lib_files: Default::default(),
            comments: vec![],
            images: vec![],
            videos: vec![],
            video_ctl: HashMap::new(),
            video_last: HashMap::new(),
            present_scrolls: HashMap::new(),
            present_dir: 0,
            drag_origin: None,
            clips: HashMap::new(),
            tex: HashMap::new(),
            sprites: Default::default(),
            sel: vec![],
            base_sel: vec![],
            row_drag: None,
            col_paint: None,
            next_id: 1,
            collapsed: HashSet::new(),
            tool: Tool::Select,
            drag: Drag::None,
            pan: Vec2::new(40.0, 40.0),
            zoom: 1.0,
            undo: vec![],
            redo: vec![],
            pending: None,
            grad_edit: None,
            undid: false,
            status: String::new(),
            themed: false,
            saved_undo: 0,
            file: PathBuf::from(FILE),
            clipboard: vec![],
            search_open: false,
            search_query: String::new(),
            search_focused: false,
            page_drag: None,
            props_clip: None,
            sizes: HashMap::new(),
            scale_orig: vec![],
            snap: true,
            rulers: true,
            snap_lines: vec![],
            pen: vec![],
            pencil: vec![],
            pencil_force: vec![],
            brush_w: 2.0,
            brush: vector::Brush::Pencil,
            nib: 35.0,
            mode: Mode::Design,
            node_edit: false,
            node_sel: None,
            editing_text: None,
            text_sel: None,
            renaming: None,
            layer_anchor: None,
            nudge_small: 1.0,
            nudge_big: 10.0,
            vtool: None,
            vtool_pts: vec![],
            builder: None,
            handle_mirror: 1,
            renaming_page: None,
            present: None,
            present_anim: None,
            present_stack: vec![],
            present_overlays: vec![],
            present_vars: None,
            present_shapes: None,
            present_scroll: Vec2::ZERO,
            present_while: vec![],
            present_hover: None,
            present_fired: false,
            open_url: None,
            present_t0: None,
            anim_time: 0.0,
            anim_frames: false,
            tl_view: (1.0, 0.0),
            anim_play: None,
            auto_key: false,
            key_sel: None,
            saved_view: (Vec2::ZERO, 1.0),
            tab: Tab::Design,
            left_tab: LeftTab::Layers,
            layer_filter: String::new(),
            export_scale: 1.0,
            export_fmt: export::Format::Png,
            version_name: String::new(),
            comment_sel: None,
            new_comment: None,
            show_resolved: false,
            gallery: false,
            paste_n: 0.0,
            view: Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 600.0)),
            show_help: false,
            left_min: false,
            blend_cache: None,
            last_shape: Kind::Rect,
            shown_status: String::new(),
            status_t: 0.0,
            api: None,
            api_owned: HashSet::new(),
            api_want: None,
            ctx: None,
            net_cursor: None,
            user: std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "guest".into()),
            zoom_text: String::new(),
            zoom_fresh: false,
            show_comments: true,
            pixel_grid: true,
            section_tool: false,
            slice_tool: false,
            presets_open: HashSet::from(["Phone".to_owned()]),
            var_tick: None,
            eyedrop_var: false,
            var_sel: vec![],
            multi_variants: None,
            color_profile: 0,
            visits: vec![],
            following: None,
            spotlight_on: false,
            chat_open: false,
            chat_text: String::new(),
            chat_send: None,
            my_chat: None,
            history_open: false,
            page_modes: vec![],
            lib_description: String::new(),
            smart_ids: Default::default(),
            smart_anchor: Default::default(),
            kbox: None,
            light_ui: false,
            light_applied: false,
            spell: true,
            quick_open: false,
            quick_query: String::new(),
            quick_sel: 0,
            quick_focused: false,
            hide_ui: false,
            prop_labels: true,
            present_inline: false,
            actions_open: false,
            actions_query: String::new(),
            actions_sel: 0,
            actions_focused: false,
            eyedropper: false,
            scale_tool: false,
            lasso_tool: false,
            bend_tool: false,
            custom_brush: None,
            stretch_image: None,
            pen_listening: false,
            node_connect: None,
            bend_moved: false,
            lasso: vec![],
            arrow_tool: false,
            proto_settings: false,
            snap_pixels: true,
            layout_guides: false,
            labels: true,
            pixel_preview: 0,
            outlines: false,
            outline_hidden: true,
            outline_bounds: false,
            share_open: false,
            share_addr: false,
        }
    }
}

// ponytail: the tree helpers below scan the flat Vec (O(n) per lookup, O(n^2) per frame);
// index by id / keep child lists if documents reach thousands of layers
impl App {
    fn to_world(&self, origin: Pos2, p: Pos2) -> Pos2 {
        ((p - origin - self.pan) / self.zoom).to_pos2()
    }

    fn to_screen(&self, origin: Pos2, p: Pos2) -> Pos2 {
        origin + self.pan + p.to_vec2() * self.zoom
    }

    fn screen_rect(&self, origin: Pos2, r: Rect) -> Rect {
        Rect::from_min_max(self.to_screen(origin, r.min), self.to_screen(origin, r.max))
    }

    fn index_of(&self, id: u64) -> Option<usize> {
        self.shapes.iter().position(|s| s.id == id)
    }

    // the selected shape, when exactly one is selected
    fn single(&self) -> Option<usize> {
        match self.sel[..] {
            [id] => self.index_of(id),
            _ => None,
        }
    }

    // selected shapes that have no selected ancestor, in z (Vec) order: what move/group/arrange act on
    fn sel_roots(&self) -> Vec<u64> {
        (0..self.shapes.len())
            .filter(|&i| self.sel.contains(&self.shapes[i].id))
            .filter(|&i| self.chain(i)[1..].iter().all(|&k| !self.sel.contains(&self.shapes[k].id)))
            .map(|i| self.shapes[i].id)
            .collect()
    }

    fn prune_sel(&mut self) {
        self.sel = self.sel.iter().copied().filter(|&id| self.index_of(id).is_some()).collect();
    }

    fn sel_bounds(&self) -> Option<Rect> {
        self.sel_roots().iter().filter_map(|&id| self.index_of(id)).map(|i| self.world_rect(i)).reduce(Rect::union)
    }

    // the shape at `i` followed by its ancestors, nearest first
    fn chain(&self, i: usize) -> Vec<usize> {
        let mut out = vec![i];
        let mut cur = self.shapes[i].parent;
        while let Some(i) = cur.and_then(|id| self.index_of(id)) {
            if out.len() > self.shapes.len() {
                break; // malformed file with a parent cycle
            }
            out.push(i);
            cur = self.shapes[i].parent;
        }
        out
    }

    // world-space origin of children of `parent`
    fn origin_of(&self, parent: Option<u64>) -> Vec2 {
        match parent.and_then(|id| self.index_of(id)) {
            Some(i) => self.chain(i).iter().map(|&k| self.shapes[k].pos.to_vec2()).fold(Vec2::ZERO, |a, b| a + b),
            None => Vec2::ZERO,
        }
    }

    // world-space bounds of the (rotated) shape
    fn world_rect(&self, i: usize) -> Rect {
        self.shapes[i].aabb().translate(self.origin_of(self.shapes[i].parent))
    }

    // `id` is `ancestor` or one of its descendants
    fn is_inside(&self, id: u64, ancestor: u64) -> bool {
        self.index_of(id).is_some_and(|i| self.chain(i).iter().any(|&k| self.shapes[k].id == ancestor))
    }

    // depth-first, parents before children, siblings in z-order: last = topmost
    fn draw_order(&self) -> Vec<usize> {
        fn walk(app: &App, parent: Option<u64>, out: &mut Vec<usize>) {
            let mut kids: Vec<usize> = (0..app.shapes.len()).filter(|&i| app.shapes[i].parent == parent).collect();
            // canvas stacking: an auto-layout frame can draw its first child on top
            if parent.and_then(|p| app.index_of(p)).is_some_and(|p| app.shapes[p].layout.as_ref().is_some_and(|l| l.first_on_top)) {
                kids.reverse();
            }
            for i in kids {
                out.push(i);
                walk(app, Some(app.shapes[i].id), out);
            }
        }
        let mut out = vec![];
        walk(self, None, &mut out);
        out
    }

    // can the pointer at world point `p` reach shape `i`? (visible, unlocked, inside every clipping ancestor)
    fn reachable(&self, i: usize, p: Pos2) -> bool {
        let chain = self.chain(i);
        chain.iter().all(|&k| self.shapes[k].visible && !self.shapes[k].locked)
            && chain[1..].iter().all(|&k| !self.shapes[k].clip || self.world_rect(k).contains(p))
    }

    // Figma-style "title" strip above a root frame; the only part of a root frame that picks it
    // ponytail: width is estimated from the name length (text isn't measured here); measure with a galley if it drifts
    fn title_rect(&self, s: &Shape) -> Rect {
        let w = (s.name.chars().count() as f32 * 6.0 + 6.0).max(24.0);
        Rect::from_min_size(s.pos - Vec2::new(0.0, 18.0 / self.zoom), Vec2::new(w, 18.0) / self.zoom)
    }

    // deepest shape under world point `p`
    fn hit_test(&self, p: Pos2) -> Option<u64> {
        let tol = 4.0 / self.zoom;
        self.draw_order().into_iter().rev().find_map(|i| {
            let s = &self.shapes[i];
            let hit = if s.kind == Kind::Frame && s.parent.is_none() {
                self.title_rect(s).contains(p)
            } else {
                s.hit(p - self.origin_of(s.parent), tol)
            };
            (hit && !s.mask && self.reachable(i, p)).then_some(s.id)
        })
    }

    // what a click on `id` selects: the outermost group around it (clicks pick groups, double-click picks inside)
    fn select_unit(&self, id: u64) -> u64 {
        let Some(i) = self.index_of(id) else { return id };
        let mut best = id;
        for &k in &self.chain(i)[1..] {
            if self.shapes[k].kind != Kind::Group {
                break;
            }
            best = self.shapes[k].id;
        }
        best
    }

    // nearest Frame ancestor (groups are transparent), None = the canvas
    fn container_frame(&self, id: u64) -> Option<u64> {
        let i = self.index_of(id)?;
        self.chain(i)[1..].iter().map(|&k| &self.shapes[k]).find(|s| s.kind == Kind::Frame).map(|s| s.id)
    }

    // deepest frame under `p`, ignoring `exclude` and everything inside them
    fn frame_at(&self, p: Pos2, exclude: &[u64]) -> Option<u64> {
        self.draw_order().into_iter().rev().find_map(|i| {
            let s = &self.shapes[i];
            let ok = s.kind == Kind::Frame
                && self.world_rect(i).contains(p)
                && self.reachable(i, p)
                && !exclude.iter().any(|&ex| self.is_inside(s.id, ex));
            ok.then_some(s.id)
        })
    }

    // shapes a marquee over world rect `r` selects: whole groups, never their children;
    // frames only when fully enclosed, so a box drawn inside a frame picks its contents instead
    fn marquee_hits(&self, r: Rect) -> Vec<u64> {
        let hits: Vec<usize> = self
            .draw_order()
            .into_iter()
            .filter(|&i| {
                let chain = self.chain(i);
                let wr = self.world_rect(i);
                chain.iter().all(|&k| self.shapes[k].visible && !self.shapes[k].locked)
                    && chain[1..].iter().all(|&k| self.shapes[k].kind != Kind::Group)
                    && if self.shapes[i].kind == Kind::Frame { r.contains_rect(wr) } else { r.intersects(wr) }
            })
            .collect();
        hits.iter()
            .filter(|&&i| self.chain(i)[1..].iter().all(|k| !hits.contains(k)))
            .map(|&i| self.shapes[i].id)
            .collect()
    }

    // the layers a free-hand outline selects: those it covers, judged by their box corners and centre (a frame must be wholly inside)
    fn lasso_hits(&self, poly: &[Pos2]) -> Vec<u64> {
        let inside = |p: Pos2| {
            let mut c = false;
            for k in 0..poly.len() {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                    c = !c;
                }
            }
            c
        };
        if poly.len() < 3 {
            return vec![];
        }
        let hits: Vec<usize> = self
            .draw_order()
            .into_iter()
            .filter(|&i| {
                let chain = self.chain(i);
                let wr = self.world_rect(i);
                let corners = [wr.left_top(), wr.right_top(), wr.right_bottom(), wr.left_bottom()];
                chain.iter().all(|&k| self.shapes[k].visible && !self.shapes[k].locked)
                    && chain[1..].iter().all(|&k| self.shapes[k].kind != Kind::Group)
                    && if self.shapes[i].kind == Kind::Frame { corners.iter().all(|&c| inside(c)) } else { inside(wr.center()) || corners.iter().any(|&c| inside(c)) || poly.iter().any(|&p| wr.contains(p)) }
            })
            .collect();
        hits.iter().filter(|&&i| self.chain(i)[1..].iter().all(|k| !hits.contains(k))).map(|&i| self.shapes[i].id).collect()
    }

    fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id - 1
    }

    fn add(&mut self, kind: Kind, world: Pos2) {
        let parent = self.frame_at(world, &[]);
        let pos = world - self.origin_of(parent);
        let id = self.new_id();
        self.shapes.push(Shape::new(kind, id, self.shapes.len() + 1, pos, parent));
        self.sel = vec![id];
    }

    // moves `id` under `parent` (None = canvas) and into z-slot `anchor` = (sibling, above it?);
    // no anchor = top of the new parent. The shape stays where it is on screen.
    fn move_to(&mut self, id: u64, parent: Option<u64>, anchor: Option<(u64, bool)>) {
        if parent.is_some_and(|p| self.is_inside(p, id)) || anchor.is_some_and(|(a, _)| self.is_inside(a, id)) {
            return; // can't drop a shape into itself
        }
        let Some(i) = self.index_of(id) else { return };
        let world = self.shapes[i].pos + self.origin_of(self.shapes[i].parent);
        let mut s = self.shapes.remove(i);
        s.parent = parent;
        s.pos = world - self.origin_of(parent);
        let at = anchor
            .and_then(|(a, above)| self.index_of(a).map(|k| if above { k + 1 } else { k }))
            .unwrap_or(self.shapes.len());
        self.shapes.insert(at, s);
    }

    fn reparent(&mut self, id: u64, new_parent: Option<u64>) {
        if self.index_of(id).is_some_and(|i| self.shapes[i].parent != new_parent) {
            self.move_to(id, new_parent, None);
        }
    }

    // wraps the selection in a group (placed where the topmost selected shape was)
    fn group(&mut self) {
        let roots = self.sel_roots();
        let (Some(top), Some(bb)) = (roots.last().and_then(|&id| self.index_of(id)), self.sel_bounds()) else { return };
        let parent = self.shapes[top].parent;
        let id = self.new_id();
        let mut g = Shape::new(Kind::Group, id, self.shapes.len() + 1, bb.min - self.origin_of(parent), parent);
        g.size = bb.size();
        self.shapes.insert(top + 1, g);
        for rid in roots {
            // reparent without reordering: only the group's own z-slot matters to its siblings
            let i = self.index_of(rid).unwrap();
            let world = self.shapes[i].pos + self.origin_of(self.shapes[i].parent);
            self.shapes[i].parent = Some(id);
            self.shapes[i].pos = world - self.origin_of(Some(id));
        }
        self.sel = vec![id];
    }

    // replaces every selected group by its children, in the group's z-slot
    fn ungroup(&mut self) {
        let mut out = vec![];
        for id in self.sel.clone() {
            let Some(g) = self.index_of(id).filter(|&g| self.shapes[g].kind == Kind::Group) else {
                out.push(id);
                continue;
            };
            let (gp, gpos) = (self.shapes[g].parent, self.shapes[g].pos.to_vec2());
            let kids: Vec<Shape> = self
                .shapes
                .iter()
                .filter(|s| s.parent == Some(id))
                .cloned()
                .map(|mut s| {
                    s.parent = gp;
                    s.pos += gpos;
                    s
                })
                .collect();
            out.extend(kids.iter().map(|k| k.id));
            self.shapes.retain(|s| s.parent != Some(id));
            let g = self.index_of(id).unwrap();
            self.shapes.splice(g..=g, kids);
        }
        let mut seen = HashSet::new();
        out.retain(|&id| self.index_of(id).is_some() && seen.insert(id));
        self.sel = out;
    }

    fn remove_subtree(&mut self, id: u64) {
        let doomed: Vec<u64> = self.shapes.iter().filter(|s| self.is_inside(s.id, id)).map(|s| s.id).collect();
        self.shapes.retain(|s| !doomed.contains(&s.id));
        self.sel.retain(|id| !doomed.contains(id));
        if self.editing_text.is_some_and(|t| doomed.contains(&t)) {
            self.editing_text = None;
        }
    }

    // swap with the next/previous sibling in z-order
    fn move_z(&mut self, id: u64, up: bool) {
        let Some(i) = self.index_of(id) else { return };
        let parent = self.shapes[i].parent;
        let sibs: Vec<usize> = (0..self.shapes.len()).filter(|&k| self.shapes[k].parent == parent).collect();
        let Some(k) = sibs.iter().position(|&x| x == i) else { return };
        let other = if up { sibs.get(k + 1) } else { k.checked_sub(1).and_then(|k| sibs.get(k)) };
        if let Some(&j) = other.filter(|&&j| !self.sel.contains(&self.shapes[j].id)) {
            self.shapes.swap(i, j);
        }
    }

    // the document as an undo step keeps it
    pub(crate) fn snap(&self) -> Snap {
        Snap { shapes: self.shapes.clone(), sel: self.sel.clone(), guides: self.guides.clone(), styles: self.styles.clone(), ext: self.ext.clone(), variables: self.variables.clone(), comments: self.comments.clone(), page: self.cur, page_look: self.page_look(), pages: None }
    }

    fn page_look(&self) -> (Color32, bool) {
        self.pages.get(self.cur).map_or((Color32::TRANSPARENT, true), |p| (p.background, p.bg_visible))
    }

    // every page, the open one with its layers as they are now
    fn pages_state(&self) -> Vec<Page> {
        let mut pages = self.pages.clone();
        if let Some(p) = pages.get_mut(self.cur) {
            p.shapes = self.shapes.clone();
            p.guides = self.guides.clone();
        }
        pages
    }

    // an undo step for a change to the pages themselves (add, delete, move, duplicate, rename): taken before the change
    pub(crate) fn record_pages_undo(&mut self) {
        let mut s = self.snap();
        s.pages = Some(self.pages_state());
        self.undo.push(s);
        self.redo.clear();
        self.pending = None;
    }

    // the parts of a snapshot other than the layers and the selection (the layers are compared on their own, a changed selection is
    // not an edit)
    fn snap_rest(&self) -> Snap {
        Snap { shapes: vec![], sel: vec![], guides: self.guides.clone(), styles: self.styles.clone(), ext: self.ext.clone(), variables: self.variables.clone(), comments: self.comments.clone(), page: self.cur, page_look: self.page_look(), pages: None }
    }

    fn do_undo(&mut self, back: bool) {
        // while a pen path is being drawn, undo takes back its last point (as Figma does), not an earlier edit
        if back && !self.pen.is_empty() {
            self.pen.pop();
            return;
        }
        self.undid = true;
        let mut now = self.snap();
        let state = self.pages_state();
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        if let Some(prev) = from.pop() {
            // a step that changed the pages is redone from the pages as they are now
            if prev.pages.is_some() {
                now.pages = Some(state);
            }
            to.push(now);
            // back to the page the step belongs to (every page if the step changed them)
            self.store_current_page();
            if let Some(pages) = prev.pages {
                self.pages = pages;
            }
            let page = prev.page.min(self.pages.len().saturating_sub(1));
            self.cur = page;
            if let Some(p) = self.pages.get_mut(page) {
                p.background = prev.page_look.0;
                p.bg_visible = prev.page_look.1;
            }
            self.shapes = prev.shapes;
            self.guides = prev.guides;
            self.styles = prev.styles;
            self.ext = prev.ext;
            self.variables = prev.variables;
            self.comments = prev.comments;
            // the selection comes back as it was, minus layers that no longer exist
            self.sel = prev.sel;
            self.prune_sel();
            self.sizes.clear();
            self.node_edit = false;
        }
    }

    // ---- files, pages

    fn doc(&mut self) -> Doc {
        self.pages[self.cur].shapes = self.shapes.clone();
        self.pages[self.cur].guides = self.guides.clone();
        if let Some(b) = self.branches.get_mut(self.cur_branch) {
            b.pages = self.pages.clone();
        }
        Doc {
            pages: self.pages.clone(),
            current: self.cur,
            styles: self.styles.clone(),
            ext_styles: self.ext.clone(),
            variables: self.variables.clone(),
            versions: self.versions.clone(),
            libraries: self.libs.clone(),
            branches: self.branches.clone(),
            cur_branch: self.cur_branch,
            lib_id: self.lib_id,
            lib_version: self.lib_version,
            color_profile: self.color_profile,
            visits: self.visits.clone(),
            lib_description: self.lib_description.clone(),
            comments: self.comments.clone(),
            images: self.images.clone(),
            videos: self.videos.clone(),
            next_id: self.next_id,
        }
    }

    fn save_to(&mut self, path: PathBuf) {
        let doc = self.doc();
        self.status = match serde_json::to_string_pretty(&doc).map(|s| std::fs::write(&path, s)) {
            Ok(Ok(())) => {
                if !cfg!(test) {
                    files::note_recent(&path);
                }
                self.saved_undo = self.undo.len();
                self.file = path.clone();
                format!("saved {}", path.display())
            }
            Ok(Err(e)) => format!("save failed: {e}"),
            Err(e) => format!("save failed: {e}"),
        };
    }

    fn load_from(&mut self, path: PathBuf) {
        let parsed = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<File>(&s).map_err(|e| e.to_string()));
        let mut doc = match parsed {
            Ok(File::Old(shapes)) => Doc { pages: vec![Page { shapes, ..Page::default() }], ..Doc::default() },
            Ok(File::Doc(d)) => d,
            Err(e) => {
                self.status = format!("load failed: {e}");
                return;
            }
        };
        self.open_doc(doc, path);
    }

    // makes `doc` the open document: the file path it came from (or will be saved to) is `path`
    pub(crate) fn open_doc(&mut self, mut doc: Doc, path: PathBuf) {
        if doc.pages.is_empty() {
            doc.pages.push(Page::default());
        }
        for p in &mut doc.pages {
            p.shapes = migrate(std::mem::take(&mut p.shapes));
        }
        let top = doc
            .pages
            .iter()
            .flat_map(|p| p.shapes.iter().map(|s| s.id))
            .chain(doc.images.iter().map(|a| a.id))
            .chain(doc.styles.iter().map(|s| s.id))
            .chain(doc.ext_styles.ids())
            .chain(doc.variables.iter().flat_map(|c| std::iter::once(c.id).chain(c.modes.iter().map(|m| m.id)).chain(c.vars.iter().map(|v| v.id))))
            .chain(doc.comments.iter().map(|c| c.id))
            .max()
            .unwrap_or(0);
        self.next_id = doc.next_id.max(top + 1);
        self.cur = doc.current.min(doc.pages.len() - 1);
        self.shapes = doc.pages[self.cur].shapes.clone();
        self.guides = doc.pages[self.cur].guides.clone();
        self.pages = doc.pages;
        self.styles = doc.styles;
        self.ext = doc.ext_styles;
        self.variables = doc.variables;
        self.versions = doc.versions;
        self.libs = doc.libraries;
        self.branches = doc.branches;
        self.cur_branch = doc.cur_branch.min(self.branches.len().saturating_sub(1));
        self.lib_id = doc.lib_id;
        self.lib_version = doc.lib_version;
        self.color_profile = doc.color_profile;
        self.visits = doc.visits;
        self.lib_description = doc.lib_description;
        self.lib_files.clear();
        self.comments = doc.comments;
        self.images = doc.images;
        self.videos = doc.videos;
        self.clips.clear();
        self.tex.clear();
        self.reset_session();
        self.status = format!("loaded {}", path.display());
        if !cfg!(test) {
            files::note_recent(&path);
        }
        self.file = path;
    }

    // forget everything that refers to the previous document state
    // a new document: nothing of the old one is undoable
    fn reset_session(&mut self) {
        self.undo.clear();
        self.saved_undo = 0;
        self.redo.clear();
        self.reset_view();
    }

    // moving to another page, or after a page is deleted: the view starts afresh but the history stays
    fn reset_view(&mut self) {
        self.sel.clear();
        self.collapsed.clear();
        self.pending = None;
        self.sizes.clear();
        self.node_edit = false;
        self.editing_text = None;
        self.present = None;
        self.pen.clear();
    }

    fn switch_page(&mut self, i: usize) {
        if i == self.cur || i >= self.pages.len() {
            return;
        }
        self.pages[self.cur].shapes = std::mem::take(&mut self.shapes);
        self.pages[self.cur].guides = std::mem::take(&mut self.guides);
        self.cur = i;
        self.shapes = self.pages[i].shapes.clone();
        self.guides = self.pages[i].guides.clone();
        self.reset_view();
    }

    // the current page's shapes and guides go back into its `pages` entry (they live in `shapes` / `guides` while it is open)
    fn store_current_page(&mut self) {
        self.pages[self.cur].shapes = self.shapes.clone();
        self.pages[self.cur].guides = self.guides.clone();
    }

    // moves page `from` to the slot `to` (0..=len, counted before the move); the open page and the comments follow their pages
    fn move_page(&mut self, from: usize, to: usize) {
        if from >= self.pages.len() || to > self.pages.len() || to == from || to == from + 1 {
            return;
        }
        self.record_pages_undo();
        self.store_current_page();
        let page = self.pages.remove(from);
        let ins = if to > from { to - 1 } else { to };
        self.pages.insert(ins, page);
        let map = |i: usize| -> usize {
            if i == from {
                return ins;
            }
            let j = if i > from { i - 1 } else { i };
            if j >= ins { j + 1 } else { j }
        };
        self.cur = map(self.cur);
        for c in &mut self.comments {
            c.page = map(c.page);
        }
        self.renaming_page = None;
    }

    // a copy of page `i` right after it, which becomes the open page
    fn duplicate_page(&mut self, i: usize) {
        if i >= self.pages.len() {
            return;
        }
        self.record_pages_undo();
        self.store_current_page();
        let mut copy = self.pages[i].clone();
        copy.name = format!("{} copy", copy.name);
        self.pages.insert(i + 1, copy);
        if self.cur > i {
            self.cur += 1;
        }
        for c in &mut self.comments {
            if c.page > i {
                c.page += 1;
            }
        }
        self.switch_page(i + 1);
    }

    // the editor of a page's name opens: that is one undo step (taken now, dropped again if the name ends up unchanged)
    pub(crate) fn start_rename_page(&mut self, i: usize) {
        if self.renaming_page != Some(i) {
            self.record_pages_undo();
        }
        self.renaming_page = Some(i);
    }

    pub(crate) fn end_rename_page(&mut self, i: usize) {
        self.renaming_page = None;
        let unchanged = self.undo.last().and_then(|s| s.pages.as_ref()).and_then(|p| p.get(i)).is_some_and(|p| p.name == self.pages[i].name);
        if unchanged && self.undo.last().is_some_and(|s| s.pages.is_some()) {
            self.undo.pop();
        }
    }

    fn add_page(&mut self) {
        self.record_pages_undo();
        let n = self.pages.len() + 1;
        self.pages.push(Page { name: format!("Page {n}"), ..Page::default() });
        self.switch_page(n - 1);
    }

    fn delete_page(&mut self, i: usize) {
        if self.pages.len() < 2 || i >= self.pages.len() {
            return;
        }
        self.record_pages_undo();
        self.pages[self.cur].shapes = std::mem::take(&mut self.shapes);
        self.pages[self.cur].guides = std::mem::take(&mut self.guides);
        let keep = if i == self.cur { if i == 0 { 1 } else { 0 } } else { self.cur };
        self.pages.remove(i);
        self.cur = if keep > i { keep - 1 } else { keep };
        self.shapes = self.pages[self.cur].shapes.clone();
        self.guides = self.pages[self.cur].guides.clone();
        self.comments.retain(|c| c.page != i);
        for c in &mut self.comments {
            if c.page > i {
                c.page -= 1;
            }
        }
        self.reset_view();
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() {
            return;
        }
        if self.mode == Mode::Dev {
            if ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy))) {
                self.copy(ctx);
            }
            return;
        }
        for ev in ctx.input(|i| i.events.clone()) {
            match ev {
                egui::Event::Copy => self.copy(ctx),
                egui::Event::Cut => {
                    self.copy(ctx);
                    for id in self.sel_roots() {
                        self.remove_subtree(id);
                    }
                }
                egui::Event::Paste(s) => self.paste(&s),
                _ => {}
            }
        }
        for f in ctx.input(|i| i.raw.dropped_files.clone()) {
            let bytes = f.bytes.as_ref().map(|b| b.to_vec()).or_else(|| f.path.as_ref().and_then(|p| std::fs::read(p).ok()));
            let name = f.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or(f.name.clone());
            // a Figma file dropped on the window is imported; anything else is an image
            if let Some(path) = f.path.as_ref().filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fig"))) {
                self.import_fig(path.clone());
                continue;
            }
            if let Some(path) = f.path.as_ref().filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sketch"))) {
                self.import_sketch(path.clone());
                continue;
            }
            if let Some(bytes) = bytes {
                self.add_image(&name, bytes, None);
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        // only typing into a text field takes the keys: a button or row that kept focus after a click must not
        if ctx.text_edit_focused() {
            return self.undo_keys(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::D)) {
            self.set_mode(if self.mode == Mode::Dev { Mode::Design } else { Mode::Dev });
        }
        self.view_keys(ctx);
        if self.mode == Mode::Dev {
            // read-only: looking around is all there is
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.escape();
            }
            return;
        }
        self.undo_keys(ctx);
        self.edit_keys(ctx);
    }

    fn undo_keys(&mut self, ctx: &egui::Context) {
        if self.mode == Mode::Dev {
            return;
        }
        let redo = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y));
        if redo {
            self.do_undo(false);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z)) {
            self.do_undo(true);
        }
    }

    // zoom, rulers and help: fine in every mode
    fn view_keys(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let key = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if ctx.input(|i| i.key_pressed(Key::Questionmark)) {
            self.show_help ^= true;
        }
        if key(Modifiers::SHIFT, Key::R) {
            self.rulers ^= true;
        }
        if key(Modifiers::SHIFT, Key::G) {
            self.layout_guides ^= true;
        }
        if key(Modifiers::COMMAND | Modifiers::SHIFT, Key::O) {
            self.outlines ^= true;
        }
        if key(Modifiers::SHIFT, Key::Quote) {
            self.pixel_grid ^= true;
        }
        if key(Modifiers::SHIFT, Key::Backslash) {
            self.left_min ^= true;
        }
        // text shortcuts (not while typing): size, weight, letter spacing, line height
        if self.editing_text.is_none() {
            let size = Modifiers::COMMAND | Modifiers::SHIFT;
            let weight = Modifiers::COMMAND | Modifiers::ALT;
            for (k, d) in [(Key::Period, 1.0f32), (Key::Comma, -1.0)] {
                if key(size, k) {
                    self.adjust_text(|s| s.font_size = (s.font_size + d).max(1.0));
                }
                if key(weight, k) {
                    self.adjust_text(|s| {
                        s.weight = ((s.base_weight() as i32 + (d as i32) * 100).clamp(100, 900)) as u16;
                        s.bold = false;
                    });
                }
            }
            for (m, step) in [(Modifiers::ALT, 1.0f32), (Modifiers::ALT | Modifiers::SHIFT, 10.0)] {
                if key(m, Key::ArrowRight) {
                    self.adjust_text(|s| s.tracking += step);
                }
                if key(m, Key::ArrowLeft) {
                    self.adjust_text(|s| s.tracking -= step);
                }
                if key(m, Key::ArrowUp) {
                    self.adjust_text(|s| s.line_h = ((if s.line_h > 0.0 { s.line_h } else { 1.2 }) + step * 0.05).max(0.1));
                }
                if key(m, Key::ArrowDown) {
                    self.adjust_text(|s| s.line_h = ((if s.line_h > 0.0 { s.line_h } else { 1.2 }) - step * 0.05).max(0.1));
                }
            }
        }
        if key(Modifiers::NONE, Key::Q) && self.editing_text.is_none() {
            self.toggle_multi_variants();
        }
        if key(Modifiers::COMMAND, Key::Backslash) {
            self.hide_ui ^= true;
        }
        if key(Modifiers::SHIFT, Key::I) {
            self.open_quick();
        }
        if key(Modifiers::SHIFT, Key::Space) {
            if self.present.is_some() {
                self.exit_present();
                self.present_inline = false;
            } else if let Some(f) = self.present_target() {
                self.start_present(f);
                self.present_inline = true;
            }
        }
        if key(Modifiers::SHIFT, Key::Num0) {
            self.zoom_at_center(1.0);
        }
        if key(Modifiers::SHIFT, Key::Num1) {
            self.zoom_fit(false);
        }
        if key(Modifiers::SHIFT, Key::Num2) {
            self.zoom_fit(true);
        }
        if key(cmd, Key::Equals) || key(cmd, Key::Plus) {
            self.zoom_at_center(self.zoom * 1.25);
        }
        if key(cmd, Key::Minus) {
            self.zoom_at_center(self.zoom / 1.25);
        }
        if key(cmd, Key::Slash) {
            if self.actions_open {
                self.actions_open = false;
            } else {
                self.open_actions();
            }
        }
    }

    fn edit_keys(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let (ca, cs) = (Modifiers::COMMAND | Modifiers::ALT, Modifiers::COMMAND | Modifiers::SHIFT);
        let key = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        // more specific modifier sets first: consume_key ignores an extra Shift
        // egui matches a pattern when the pressed keys hold at least its modifiers (extra Shift and Alt are fine), so the
        // most specific shortcut of a letter comes first
        if key(ca, Key::G) {
            self.frame_selection();
        } else if key(cs, Key::G) {
            self.ungroup();
        } else if key(cmd, Key::G) {
            self.group();
        }
        if key(cs, Key::K) {
            self.combine_as_variants();
        } else if key(ca, Key::K) {
            self.make_component();
        }
        if key(ca, Key::B) {
            self.detach_instances();
        }
        if key(ca, Key::M) {
            self.toggle_mask();
        }
        if key(ca, Key::U) {
            self.boolean(geom::BoolOp::Union);
        }
        if key(ca, Key::S) {
            self.boolean(geom::BoolOp::Subtract);
        }
        if key(ca, Key::I) {
            self.boolean(geom::BoolOp::Intersect);
        }
        if key(ca, Key::X) {
            self.boolean(geom::BoolOp::Exclude);
        }
        // bring to front / send to back: Ctrl+Shift+] and Ctrl+Shift+[ (tried before the one-step moves, which Shift also matches)
        if key(cs, Key::CloseBracket) {
            for id in self.sel_roots() {
                while self.index_of(id).is_some_and(|i| self.shapes[i..].iter().any(|s| s.parent == self.shapes[i].parent && !self.sel.contains(&s.id))) {
                    let before = self.shapes.clone();
                    self.move_z(id, true);
                    if self.shapes == before {
                        break;
                    }
                }
            }
        } else if key(cs, Key::OpenBracket) {
            for id in self.sel_roots().into_iter().rev() {
                while self.index_of(id).is_some_and(|i| self.shapes[..i].iter().any(|s| s.parent == self.shapes[i].parent && !self.sel.contains(&s.id))) {
                    let before = self.shapes.clone();
                    self.move_z(id, false);
                    if self.shapes == before {
                        break;
                    }
                }
            }
        } else if key(cmd, Key::CloseBracket) {
            self.sel_roots().iter().rev().for_each(|&id| self.move_z(id, true));
        }
        if key(cmd, Key::OpenBracket) {
            self.sel_roots().iter().for_each(|&id| self.move_z(id, false));
        }
        if key(cmd, Key::S) {
            self.save_to(self.file.clone());
        }
        if key(cmd, Key::E) {
            self.flatten();
        }
        // Figma: Outline stroke is Ctrl+Alt+O (Ctrl+Shift+O shows outlines)
        if key(ca, Key::O) {
            self.outline_stroke();
        }
        if key(cmd, Key::D) {
            self.duplicate();
        }
        // Figma's other layer shortcuts (their Shift / Alt variants first: consume_key ignores an extra Shift)
        let (shift, alt) = (Modifiers::SHIFT, Modifiers::ALT);
        if key(cs, Key::H) {
            self.toggle_hidden_selection();
        }
        if key(cs, Key::L) {
            self.toggle_locked_selection();
        }
        if key(cs, Key::C) {
            self.copy_as_png(ctx);
        }
        if key(ca, Key::C) {
            self.copy_properties();
        }
        if key(ca, Key::T) {
            self.tidy_up();
        }
        if key(ca, Key::V) {
            self.paste_properties();
        } else if key(cs, Key::V) {
            self.paste_into_selection();
        }
        if key(cmd, Key::R) || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) {
            if let Some(i) = self.single() {
                self.renaming = Some(self.shapes[i].id);
                self.left_min = false;
            }
        }
        if key(shift, Key::Tab) {
            self.select_sibling(false);
        } else if key(Modifiers::NONE, Key::Tab) {
            self.select_sibling(true);
        }
        if key(shift, Key::Enter) {
            self.select_parent();
        }
        if key(shift, Key::N) {
            self.go_to_frame(false);
        } else if key(Modifiers::NONE, Key::N) {
            self.go_to_frame(true);
        }
        // align with Alt+letter, as Figma
        for (k, op) in [(Key::A, ops::AlignOp::Left), (Key::D, ops::AlignOp::Right), (Key::W, ops::AlignOp::Top), (Key::S, ops::AlignOp::Bottom), (Key::H, ops::AlignOp::HCenter), (Key::V, ops::AlignOp::VCenter)] {
            if key(alt, k) {
                self.align(op);
            }
        }
        // Ctrl+Alt+Shift+A must be tried before Ctrl+Alt+A, which egui also matches when Shift is down
        if key(Modifiers::SHIFT | Modifiers::COMMAND | Modifiers::ALT, Key::A) {
            self.remove_auto_layout_selection();
        } else if key(ca, Key::A) {
            self.select_matching();
        } else if key(cs, Key::A) {
            self.select_inverse();
        } else if key(cmd, Key::A) {
            self.select_all();
        }
        if key(Modifiers::SHIFT, Key::H) {
            self.flip(true);
        }
        if key(Modifiers::SHIFT, Key::V) {
            self.flip(false);
        }
        if key(Modifiers::SHIFT, Key::A) {
            self.add_auto_layout_selection();
        }
        if key(Modifiers::SHIFT, Key::L) {
            self.set_tool(Tool::Draw(Kind::Line));
            self.arrow_tool = true;
        }
        if key(Modifiers::SHIFT, Key::B) {
            self.set_vtool(Some(vtools::VTool::Builder));
        }
        if key(Modifiers::SHIFT, Key::E) {
            self.set_vtool(Some(vtools::VTool::Eraser));
        }
        if key(Modifiers::SHIFT, Key::P) {
            self.set_tool(Tool::Pencil);
        }
        if key(Modifiers::SHIFT, Key::S) {
            self.set_tool(Tool::Draw(Kind::Frame));
            self.section_tool = true;
        }
        // Ctrl+Alt+arrows: a selection box grown from the keyboard
        for (k, dx, dy) in [(Key::ArrowLeft, -1.0, 0.0), (Key::ArrowRight, 1.0, 0.0), (Key::ArrowUp, 0.0, -1.0), (Key::ArrowDown, 0.0, 1.0)] {
            for (m, step) in [(Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT, 10.0), (Modifiers::COMMAND | Modifiers::ALT, 50.0)] {
                if key(m, k) {
                    self.kbox_grow(Vec2::new(dx, dy), step);
                }
            }
        }
        for (k, dx, dy) in [(Key::ArrowLeft, -1.0, 0.0), (Key::ArrowRight, 1.0, 0.0), (Key::ArrowUp, 0.0, -1.0), (Key::ArrowDown, 0.0, 1.0)] {
            for (m, step) in [(Modifiers::SHIFT, self.nudge_big), (Modifiers::NONE, self.nudge_small)] {
                if key(m, k) {
                    self.nudge(dx * step, dy * step);
                }
            }
        }
        // plain keys: the modifier-less tool letters
        let plain = |k| ctx.input(|i| i.key_pressed(k) && i.modifiers.is_none());
        for (k, t) in [
            (Key::V, Tool::Select),
            (Key::H, Tool::Hand),
            (Key::F, Tool::Draw(Kind::Frame)),
            (Key::A, Tool::Draw(Kind::Frame)), // Figma: frame is A as well as F
            (Key::R, Tool::Draw(Kind::Rect)),
            (Key::O, Tool::Draw(Kind::Ellipse)),
            (Key::L, Tool::Draw(Kind::Line)),
            (Key::T, Tool::Draw(Kind::Text)),
            (Key::P, Tool::Pen),
            (Key::C, Tool::Comment),
            (Key::K, Tool::Select),
            (Key::I, Tool::Select),
            (Key::S, Tool::Draw(Kind::Rect)),
            (Key::X, Tool::Select),
            (Key::B, Tool::Pencil), // the Brush tool: the pencil with a brush other than the plain pencil
        ] {
            if plain(k) {
                self.set_tool(t);
                match k {
                    Key::K => self.scale_tool = true,
                    Key::I => self.start_eyedropper(),
                    Key::S => self.slice_tool = true,
                    Key::X => self.set_vtool(Some(vtools::VTool::Cut)),
                    Key::B if self.brush == vector::Brush::Pencil => self.brush = vector::Brush::Taper,
                    _ => {}
                }
            }
        }
        if plain(Key::Enter) {
            self.enter_pressed();
        }
        if plain(Key::Delete) || plain(Key::Backspace) {
            if self.node_edit {
                self.delete_node();
            } else {
                for id in self.sel.clone() {
                    self.remove_subtree(id);
                }
            }
        }
        if plain(Key::Escape) {
            self.escape();
        }
    }

    fn set_tool(&mut self, t: Tool) {
        if let Tool::Draw(k) = t {
            if matches!(k, Kind::Rect | Kind::Line | Kind::Ellipse | Kind::Polygon | Kind::Star) {
                self.last_shape = k;
            }
        }
        self.finish_pen();
        self.arrow_tool = false;
        self.section_tool = false;
        self.slice_tool = false;
        self.scale_tool = false;
        self.lasso_tool = false;
        self.finish_builder();
        self.vtool = None;
        self.vtool_pts.clear();
        self.bend_tool = false;
        self.node_connect = None;
        self.eyedropper = false;
        self.tool = t;
        self.node_edit = false;
    }

    fn set_mode(&mut self, m: Mode) {
        self.set_tool(Tool::Select);
        self.mode = m;
        self.editing_text = None;
        self.renaming = None;
        match m {
            Mode::Design | Mode::Dev => self.tab = Tab::Design,
            Mode::Motion => self.tab = Tab::Motion,
            Mode::Draw => self.tool = Tool::Pencil,
        }
    }

    fn escape(&mut self) {
        if self.kbox.take().is_some() {
        } else if self.vtool.is_some() {
            self.builder = None;
            self.vtool = None;
            self.vtool_pts.clear();
        } else if self.grad_edit.take().is_some() {
        } else if self.new_comment.take().is_some() {
            self.tool = Tool::Select;
        } else if self.comment_sel.take().is_some() {
        } else if self.present.is_some() {
            self.exit_present();
        } else if !self.pen.is_empty() {
            self.finish_pen();
        } else if self.node_edit {
            self.node_edit = false;
        } else if self.tool != Tool::Select {
            self.tool = Tool::Select;
        } else {
            self.sel.clear();
        }
    }

    fn enter_pressed(&mut self) {
        if let (Tool::Draw(kind), true) = (self.tool, self.vtool.is_none() && self.pen.is_empty()) {
            self.create_by_keyboard(kind);
        } else if self.vtool.is_some() {
            self.finish_builder();
            self.vtool = None;
        } else if !self.pen.is_empty() {
            self.finish_pen();
        } else if let Some(i) = self.single() {
            match self.shapes[i].kind {
                Kind::Path => {
                    if self.node_edit {
                        self.node_edit = false;
                    } else {
                        self.start_node_edit(i);
                    }
                }
                Kind::Text => self.editing_text = Some(self.shapes[i].id),
                Kind::Group | Kind::Frame => {
                    // Enter dives into a container, like Figma
                    let id = self.shapes[i].id;
                    let kids: Vec<u64> = self.shapes.iter().filter(|s| s.parent == Some(id)).map(|s| s.id).collect();
                    if !kids.is_empty() {
                        self.sel = kids;
                    }
                }
                _ => self.flatten_then_edit(),
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        theme::set_light(self.light_ui);
        profile::set_p3(self.color_profile == 1);
        if self.light_ui != self.light_applied {
            widgets::apply_theme(ui.ctx());
            self.light_applied = self.light_ui;
        }
        let ctx = ui.ctx().clone();
        if !self.pen_listening {
            // read the pen's tilt from the window's own messages (Windows); once
            self.pen_listening = true;
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(RawWindowHandle::Win32(h)) = frame.window_handle().map(|h| h.as_raw()) {
                pen::install(h.hwnd.get());
            }
        }
        if self.themed {
            fig::FONTS_READY.store(true, std::sync::atomic::Ordering::Relaxed);
        } else {
            widgets::apply_theme(&ctx);
            self.themed = true;
        }
        self.ctx = Some(ctx.clone());
        if let Some(port) = self.api_want.take() {
            self.toggle_api(&ctx, port);
        }
        self.run_frame(ui);
    }
}

impl App {
    // one whole frame: events, shortcuts, panels, canvas and the undo bookkeeping (a method of its own so tests can run it)
    pub(crate) fn run_frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if fig::rows_frame() {
            ctx.request_repaint(); // property rows without an icon just learned to reach the panel's right edge
        }
        if self.gallery {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(BG_MENU())).show(ui, |ui| self.gallery(ui));
            return;
        }
        let before = self.shapes.clone(); // before keyboard edits too, so they are undoable
        let before_sel = self.sel.clone();
        let before_rest = self.snap_rest();
        let before_page = self.cur;
        self.handle_events(&ctx);
        self.shortcuts(&ctx);
        self.poll_api(&ctx);
        // an undo or redo just replaced the shapes: that is not a new edit
        let undid = std::mem::take(&mut self.undid);
        let before = if undid { self.shapes.clone() } else { before };
        let (before_sel, before_rest) = if undid { (self.sel.clone(), self.snap_rest()) } else { (before_sel, before_rest) };
        // Motion mode shows and edits the page at the playhead; `anim::commit` folds the edits back below
        let motion = self.mode == Mode::Motion && self.present.is_none();
        if motion {
            self.tick_motion(ctx.input(|i| i.time));
            if self.anim_play.is_some() {
                ctx.request_repaint();
            }
        }
        let shown = if motion { self.motion_begin() } else { None };

        // the color picker's "On this page" swatches: the page's solid colors, first use first
        let mut colors: Vec<Color32> = vec![];
        for p in self.shapes.iter().flat_map(|s| s.fills.iter().chain(s.strokes.iter())) {
            if let PaintKind::Solid(c) = p.kind {
                let c = Color32::from_rgb(c.r(), c.g(), c.b());
                if !colors.contains(&c) && colors.len() < 45 {
                    colors.push(c);
                }
            }
        }
        ctx.data_mut(|d| d.insert_temp(picker::page_colors_id(), colors));
        let chrome = (self.present.is_none() || self.present_inline) && !self.hide_ui;
        if chrome {
            // Figma: a 58px rail and 240px panels on #2c2c2c, 1px #444 between them
            let bare = egui::Frame::NONE.fill(BG());
            egui::Panel::left("rail").resizable(false).exact_size(58.0).frame(bare).show(ui, |ui| self.rail(ui));
            if !self.left_min {
                egui::Panel::left("layers").resizable(true).default_size(240.0).size_range(240.0..=480.0).frame(bare).show(ui, |ui| self.left_panel(ui));
            }
            egui::Panel::right("props").resizable(true).default_size(240.0).size_range(240.0..=480.0).frame(bare).show(ui, |ui| self.right_panel(ui));
            if self.mode == Mode::Motion {
                egui::Panel::bottom("timeline").resizable(true).default_size(170.0).size_range(90.0..=420.0).show(ui, |ui| self.timeline(ui));
            }
        }
        let canvas = egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.canvas(ui)).response.rect;
        fig::set_labels(self.prop_labels);
        if chrome {
            self.toolbar(&ctx);
            self.actions_palette(&ctx);
            self.quick_palette(&ctx);
            if self.left_min {
                self.left_pill(&ctx, canvas);
            }
        }
        self.variables_window(&ctx);
        self.suggestions_window(&ctx);
        self.branches_window(&ctx);
        self.files_window(&ctx);
        if !ctx.input(|i| i.pointer.any_down()) {
            self.row_drag = None;
        }
        if let Some(shown) = shown {
            self.motion_end(&before, &shown);
        }
        self.settle();
        if self.shapes != before {
            self.sync_instances(&before);
            self.sync_variant_edit(&before);
            self.settle();
            ctx.request_repaint();
        }
        let remote = false;
        let before = if remote { self.shapes.clone() } else { before };
        let before_rest = if remote { self.snap_rest() } else { before_rest };

        // one undo step per gesture: remember the state before the first change,
        // commit it when the pointer is released
        // ponytail: full-document snapshots, switch to diffs if documents get large
        // (a change of page is not an edit of either page: moving between pages, and page add / delete, record their own steps)
        if self.pending.is_none() && self.cur == before_page && (self.shapes != before || self.snap_rest() != before_rest) {
            let Snap { guides, styles, ext, variables, comments, page, page_look, .. } = before_rest;
            self.pending = Some(Snap { shapes: before, sel: before_sel, guides, styles, ext, variables, comments, page, page_look, pages: None });
        }
        if !ctx.input(|i| i.pointer.any_down()) {
            if let Some(p) = self.pending.take() {
                self.undo.push(p);
                self.redo.clear();
            }
        }
    }
}

impl Snap {
    // the same without the layers and the selection
    pub(crate) fn rest(&self) -> Snap {
        Snap { shapes: vec![], sel: vec![], ..self.clone() }
    }
}

#[cfg(test)]
impl App {
    pub(crate) fn snap_rest_for_test(&self) -> Snap {
        self.snap_rest()
    }
}

fn main() -> eframe::Result {
    // `drui [file.json] [--select id,id] [--mode design|draw|motion|dev] [--left file|assets|history|comments] [--gallery] [--background]`
    let args: Vec<String> = std::env::args().skip(1).collect();
    eframe::run_native(
        "drui",
        eframe::NativeOptions {
            viewport: {
                let v = egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]).with_min_inner_size([960.0, 600.0]).with_title("drui");
                // `--background`: for automation. The window opens far off-screen, not in the taskbar and without taking focus,
                // so driving the app (UI Automation, window captures) never disturbs whoever is using the machine
                if std::env::args().any(|a| a == "--background") { v.with_position([-20000.0, 0.0]).with_active(false).with_taskbar(false) } else { v }
            },
            ..Default::default()
        },
        Box::new(move |_| {
            let mut app = App::default();
            if let Some(path) = args.iter().find(|a| !a.starts_with("--") && args.iter().position(|x| x == *a).is_some_and(|i| i == 0 || !["--select", "--mode", "--left", "--api"].contains(&args[i - 1].as_str()))) {
                app.load_from(PathBuf::from(path));
            }
            if let Some(ids) = args.iter().position(|a| a == "--select").and_then(|i| args.get(i + 1)) {
                app.sel = ids.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            }
            let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.to_lowercase());
            app.gallery = args.iter().any(|a| a == "--gallery");
            app.light_ui = args.iter().any(|a| a == "--light");
            app.files.open = args.iter().any(|a| a == "--files"); // the files browser open from the start
            // `--api` or `--api 7440`: serve the local REST API (port 0 / omitted: any free port)
            if let Some(i) = args.iter().position(|a| a == "--api") {
                app.api_want = Some(args.get(i + 1).and_then(|p| p.parse().ok()).unwrap_or(0));
            }
            if let Some(m) = flag("--mode") {
                if let Some((mode, ..)) = Mode::ALL.iter().find(|(_, label, _)| label.to_lowercase().starts_with(&m)) {
                    app.set_mode(*mode);
                }
            }
            if let Some(t) = flag("--left") {
                app.left_tab = match t.as_str() {
                    "assets" => LeftTab::Assets,
                    "history" => LeftTab::History,
                    "comments" => LeftTab::Comments,
                    _ => LeftTab::Layers,
                };
            }
            Ok(Box::new(app))
        }),
    )
}

