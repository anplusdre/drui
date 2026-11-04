// Agent mode: a button in the mode strip turns the canvas into a place to talk to Claude Code. Click a layer and a chat box opens
// beside it (styled like a comment) and the composer at the bottom of the left panel creates or changes; a message runs the user's own `claude -p` with a plugin made of docs/agent.md and tools/dr.py,
// which edits a `claude_edit` copy through the REST API. While it runs the user can only pan and zoom. When it is done a marker is
// left beside the copy, like a comment pin; each box is a session of its own and can be written to again (`--resume`).
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard};

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, Id, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::api::EDIT_SUFFIX;
use crate::comments::{avatar, draw_pin, pin_rect, send_button_with, thread_frame, thread_scroll};
use crate::fig::{self, Tip};
use crate::glyphs::{paint_glyph_sized, Glyph};
use crate::{App, BG, TEXT, TEXT_SECONDARY, TEXT_TERTIARY};

const WIDTH: f32 = 320.0; // the chat box; long text wraps and the box grows downward
const AVATAR: f32 = 24.0;
const PAD: f32 = 16.0; // the air round everything in the box, the same as the header's side padding
const CLAUDE_ROW: f32 = 14.5; // the row under the chatbox and its list of mentions (Claude desktop measured 15 wide-ish; Inter at 14.5 matches the widths)
const COMPOSER: f32 = 190.0; // the room the composer at the bottom of the panel takes from the conversation above it
const NOTCH_H: f32 = 36.0;
const NOTCH_X: f32 = 24.0; // the notch's close button
const CHAT: f32 = 14.0; // the size of what is written in the box and of Claude's status line
const INSTALL: &str = "npm install -g @anthropic-ai/claude-code";
// what the headless run may do without asking: call the REST API through the helper and write its request bodies
// which model and how hard it thinks, chosen from the request: a small tweak gets the small fast model, an open-ended brief (a whole
// page, a dashboard, an app) is researched online first on the bigger one with more effort, everything else the bigger one at medium
#[derive(Clone, Copy)]
struct Plan {
    model: &'static str,
    effort: &'static str,
    research: bool,
    turns: &'static str,
    label: &'static str,
}

const SIMPLE: Plan = Plan { model: "claude-haiku-5-5", effort: "medium", research: false, turns: "40", label: "Haiku 5.5 · medium effort" };
const NORMAL: Plan = Plan { model: "claude-sonnet-5-5", effort: "medium", research: false, turns: "40", label: "Sonnet 5.5 · medium effort" };
const DEEP: Plan = Plan { model: "claude-sonnet-5-5", effort: "high", research: true, turns: "80", label: "Sonnet 5.5 · high effort · researching" };

fn plan_for(text: &str, has_target: bool) -> Plan {
    let t = text.to_lowercase();
    let toks: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let word = |list: &[&str]| toks.iter().any(|w| list.contains(w));
    let phrase = |list: &[&str]| list.iter().any(|p| t.contains(p));
    // a whole screen or product, or a request to look things up: open-ended
    let big = word(&["page", "pages", "screen", "screens", "dashboard", "app", "website", "landing", "onboarding", "flow", "portfolio", "checkout", "system"]);
    let brief = phrase(&["help me", "help build", "help create", "build a", "build an", "design a", "design an", "create a", "create an", "make a", "make an", "redesign", "from scratch"]);
    let look_up = phrase(&["research", "inspiration", "best practice", "competitor", "look up", "trending"]);
    if look_up || (big && brief) || toks.len() > 40 {
        return DEEP;
    }
    // a short change to something named or selected
    let tweak = word(&[
        "color", "colors", "colour", "pink", "red", "orange", "yellow", "green", "blue", "purple", "black", "white", "gray", "grey", "bigger", "smaller", "larger", "radius", "round", "rounded", "corner", "corners", "font", "bold", "italic", "size", "width", "height", "padding", "gap", "margin", "opacity", "shadow", "border", "stroke", "fill", "align", "center", "rename", "move", "hide", "show", "delete", "remove", "duplicate", "change", "set", "swap", "increase", "decrease", "text",
    ]);
    if toks.len() <= 12 && tweak && (has_target || toks.len() <= 8) {
        return SIMPLE;
    }
    NORMAL
}

// how a designer's words map to properties, and to check before saying done
const SYSTEM: &str = "You are working inside drui, a design tool, for a designer. Read colors the way a designer means them: a color named on its own (make the button pink, a blue card) is the layer's background, its fill, and never its text; the text color changes only when they say text color, font color, label color or the color of the words; a border or outline color is the stroke. A button, card or similar thing is the frame, so change that frame's fill (and its children only when asked). The user knows you as drui: if you name yourself, say drui. You can run only `python dr.py ...`, exactly that as the whole command from the current directory: no cd, no &&, no pipes, no other programs and no scripts of your own. Keep patches small with the compact forms in the guide (children, templates and use, repeat and each, and for vector shapes and icons d or svg) instead of listing layers one by one. For a big design write the JSON request bodies straight into files in the current directory with the Write tool (several moderate files, not one huge one) and apply each with `python dr.py POST /patch @file.json`. Nobody can approve anything while you work, so never stop to ask: if something is refused, find another way within these rules. A component, a button, an input or a family of them is always ONE component set with variant properties (Style, Size, State, Icon and the rest the guide lists), never separate components: read the guide's Components and variants section before building one, and cover every axis a designer expects, not only the variants already drawn. After a change, read the layer back (GET /layers/<id>) and make sure the property really changed before you say it is done; if the API refused, say so plainly.";
const ALLOWED: &str = "Bash(python dr.py:*),Bash(python3 dr.py:*),Bash(py dr.py:*),Read,Write";
const RESEARCH_TOOLS: &str = ",WebSearch,WebFetch"; // an open-ended brief may look things up

#[derive(Clone, Copy, PartialEq)]
enum Problem {
    NotInstalled,
    NotLoggedIn,
    NoApi, // drui's own REST API could not start; `Agent::api_error` says why
}

enum Ev {
    Status(String),
    Done { session: Option<String>, text: String },
    Failed(String),
    Missing(Problem),
}

struct Msg {
    mine: bool,
    text: String,
}

// one chat box: the layers it is about, the copy Claude made of them, its messages and Claude's own session id
struct Session {
    id: u64,
    layers: Vec<u64>,
    title: String,
    copy: Option<u64>, // the claude_edit copy Claude made (Duplicate mode)
    made: Option<u64>, // the first top-level layer a run added (a general request): the marker goes there
    answered: bool,    // a run has finished: the session has a marker
    anchor: Pos2,      // world position a new box hangs from
    msgs: Vec<Msg>,
    claude: Option<String>,
    text: String,
    focus: bool,
    refs: Vec<Mention>, // what the bottom chatbox's message mentioned
}

struct Run {
    session: u64,
    rx: Receiver<Ev>,
    child: Arc<Mutex<Proc>>,
    status: String,
    since: f64,
    unlocked: Vec<u64>, // original frames opened to the API for an Edit run; locked again when it ends
    before: HashSet<u64>, // the top-level layers there were when it started
    inplace: bool,        // it was an Edit request
    reply: bool,          // Claude was asked to write a reply
}

#[derive(Default)]
pub struct Agent {
    pub on: bool,
    sessions: Vec<Session>,
    open: Option<u64>,
    last_sel: Vec<u64>,
    run: Option<Run>,
    problem: Option<Problem>,
    api_error: String, // why the REST API did not start
    next: u64,
    inplace: bool, // Edit (change the layers where they are) instead of Duplicate
    no_reply: bool, // Claude writes no reply
    alert: Option<f64>, // when the user last tried to do something while Claude was working
    view: Option<u64>,  // the conversation shown in the left panel (the bottom chatbox writes to it)
    draft: String,      // the bottom chatbox's text
    last_draft: String,
    mentions: Vec<Mention>, // what @ named in the draft
    pop_n: usize,       // rows in the list of mentions (0 = closed)
    pop_sel: usize,
    pop_off: bool,      // Esc closed the list until the text changes
}

// what a run's thread has sent: status lines update `status`, the first other event ends the run. A thread that died without a word
// (a panic) ends it too: else the run would stay for good and the shortcuts stay blocked with it.
fn drain(rx: &Receiver<Ev>, status: &mut String) -> Option<Ev> {
    loop {
        match rx.try_recv() {
            Ok(Ev::Status(s)) => *status = s,
            Ok(ev) => return Some(ev),
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => return Some(Ev::Failed("drui stopped before it answered".into())),
        }
    }
}

impl Agent {
    pub fn busy(&self) -> bool {
        self.run.is_some()
    }
}

// ---- the claude process

// the process of a run, shared by the thread that starts it and the UI thread that may stop it. `stopped` is set under the same lock the
// child is stored under, so a Stop that comes before the process exists is not lost (it would run on, editing the design, unseen)
#[derive(Default)]
struct Proc {
    child: Option<Child>,
    stopped: bool,
}

impl Proc {
    // the tree is killed on a thread of its own: taskkill takes a moment and this is called from the UI thread
    fn stop(&mut self) {
        self.stopped = true;
        if let Some(mut c) = self.child.take() {
            std::thread::spawn(move || {
                kill_tree(&mut c);
                let _ = c.wait();
            });
        }
    }
}

fn proc_lock(p: &Mutex<Proc>) -> MutexGuard<'_, Proc> {
    p.lock().unwrap_or_else(|e| e.into_inner())
}

type Pipes = (Option<ChildStdin>, Option<ChildStdout>, Option<ChildStderr>);

// starts the process unless Stop already came (None); the check and storing the child happen under one lock
fn launch(slot: &Mutex<Proc>, spawn: impl FnOnce() -> std::io::Result<Child>) -> Option<std::io::Result<Pipes>> {
    let mut p = proc_lock(slot);
    if p.stopped {
        return None;
    }
    Some(spawn().map(|mut c| {
        let pipes = (c.stdin.take(), c.stdout.take(), c.stderr.take());
        p.child = Some(c);
        pipes
    }))
}

#[cfg(windows)]
fn quiet(c: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    c.creation_flags(0x0800_0000) // no console window
}

#[cfg(not(windows))]
fn quiet(c: &mut Command) -> &mut Command {
    c
}

fn find_claude() -> Option<&'static str> {
    ["claude", "claude.cmd"].into_iter().find(|n| quiet(Command::new(n).arg("--version")).output().is_ok_and(|o| o.status.success()))
}

// `claude auth status` prints {"loggedIn": true, ...}; when it cannot be read the run itself finds out
fn logged_in(bin: &str) -> bool {
    let Ok(out) = quiet(Command::new(bin).args(["auth", "status"])).output() else { return true };
    serde_json::from_slice::<serde_json::Value>(&out.stdout).ok().and_then(|v| v["loggedIn"].as_bool()).unwrap_or(true)
}

// not rewritten when it already says this: another drui window running the same build may be reading it, and a write is a moment of an empty file
// ponytail: two different builds still overwrite each other's copy; a folder per build if that ever shows
fn put(path: &Path, text: &str) -> std::io::Result<()> {
    if std::fs::read(path).is_ok_and(|old| old == text.as_bytes()) {
        return Ok(());
    }
    std::fs::write(path, text)
}

// the plugin Claude Code loads: the drui skill (docs/agent.md), the UI/UX skill (docs/ux.md) and the REST helper, written fresh for every run
fn plugin_dir() -> std::io::Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir); // not the install folder: it may be read-only
    let dir = base.join("drui").join("claude-plugin");
    std::fs::create_dir_all(dir.join(".claude-plugin"))?;
    std::fs::create_dir_all(dir.join("skills").join("drui"))?;
    put(&dir.join(".claude-plugin").join("plugin.json"), r#"{"name": "drui", "version": "1.0.0", "description": "Edit a design in drui through its local REST API"}"#)?;
    let guide = without_notes(&include_str!("../docs/agent.md").replace("python tools/dr.py", "python dr.py").replace("tools/dr.py", "dr.py"));
    let skill = format!("---\nname: drui\ndescription: Edit a design in drui (the design tool) through its local REST API: read the outline and patch layers. Use whenever asked to change a drui design or layer.\n---\n\n{guide}");
    put(&dir.join("skills").join("drui").join("SKILL.md"), &skill)?;
    let ux = format!("---\nname: drui-ux\ndescription: UI/UX rules for designing a screen or page in drui: content hierarchy, consistency, readability, color, cards, tables and the laws of UX. Use whenever creating or changing a screen, page or layout.\n---\n\n{}", include_str!("../docs/ux.md"));
    std::fs::create_dir_all(dir.join("skills").join("drui-ux"))?;
    put(&dir.join("skills").join("drui-ux").join("SKILL.md"), &ux)?;
    put(&dir.join("dr.py"), include_str!("../tools/dr.py"))?;
    Ok(dir)
}

// agent mode does not leave notes: the guide without its Notes section and the lines that ask for them
fn without_notes(guide: &str) -> String {
    let guide = guide.replace(", `python dr.py POST /notes '{\"text\": \"...\"}'`", "");
    let mut out = String::new();
    let mut skip = false;
    for line in guide.lines() {
        if line.starts_with("## ") {
            skip = line.starts_with("## Notes");
        }
        if !skip && !line.starts_with("3. **Leave notes**") && !line.starts_with("4. Add notes") {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = quiet(Command::new("taskkill").args(["/PID", &child.id().to_string(), "/T", "/F"])).output();
    }
    let _ = child.kill();
}

// a short line for what a tool call does, shown under the flashing text
fn describe(tool: &str, input: &serde_json::Value) -> String {
    match tool {
        "Write" => return "Preparing the layout".into(),
        "WebSearch" => return "Searching the web".into(),
        "WebFetch" => return "Reading a web page".into(),
        _ => {}
    }
    let cmd = input["command"].as_str().unwrap_or("");
    let steps = [("claude_edit", "Copying the frame"), ("/patch", "Applying changes"), ("/export", "Looking at the result"), ("/outline", "Reading the design"), ("/layers", "Reading a layer")];
    steps.iter().find(|(k, _)| cmd.contains(k)).map_or_else(|| format!("Running {tool}"), |(_, v)| (*v).to_string())
}

// the headless `claude -p`; the API's address goes in its environment (dr.py prefers it to the shared file in the temp folder, which the
// latest drui window to start its API overwrites)
fn claude_cmd(bin: &str, dir: &Path, plan: Plan, resume: Option<&str>, (url, token): &(String, String)) -> Command {
    let mut cmd = Command::new(bin);
    let allowed = if plan.research { format!("{ALLOWED}{RESEARCH_TOOLS}") } else { ALLOWED.to_string() };
    cmd.current_dir(dir).args(["-p", "--output-format", "stream-json", "--verbose", "--permission-mode", "acceptEdits", "--model", plan.model, "--effort", plan.effort, "--max-turns", plan.turns, "--allowedTools", &allowed, "--append-system-prompt", SYSTEM, "--plugin-dir"]).arg(dir);
    cmd.env("DRUI_API_URL", url).env("DRUI_API_TOKEN", token);
    if let Some(id) = resume {
        cmd.args(["--resume", id]);
    }
    cmd
}

fn run_claude(ctx: egui::Context, prompt: String, resume: Option<String>, plan: Plan, api: (String, String), slot: Arc<Mutex<Proc>>, tx: Sender<Ev>) {
    let send = |ev: Ev| {
        let _ = tx.send(ev);
        ctx.request_repaint();
    };
    let Some(bin) = find_claude() else { return send(Ev::Missing(Problem::NotInstalled)) };
    if !logged_in(bin) {
        return send(Ev::Missing(Problem::NotLoggedIn));
    }
    let dir = match plugin_dir() {
        Ok(d) => d,
        Err(e) => return send(Ev::Failed(format!("could not write the plugin: {e}"))),
    };
    let mut cmd = claude_cmd(bin, &dir, plan, resume.as_deref(), &api);
    let (stdin, stdout, stderr) = match launch(&slot, || quiet(cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())).spawn()) {
        None => return, // Stop came first
        Some(Err(e)) => return send(Ev::Failed(format!("could not start claude: {e}"))),
        Some(Ok(pipes)) => pipes,
    };
    // the prompt goes through stdin: command lines mangle newlines and quotes on Windows
    if let Some(mut stdin) = stdin {
        let _ = stdin.write_all(prompt.as_bytes());
    }
    let errors = Arc::new(Mutex::new(String::new()));
    if let Some(mut e) = stderr {
        let errors = errors.clone();
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = std::io::Read::read_to_string(&mut e, &mut s);
            *errors.lock().unwrap() = s;
        });
    }
    let mut finished = false;
    for line in BufReader::new(stdout.unwrap()).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        match v["type"].as_str() {
            Some("assistant") => {
                for block in v["message"]["content"].as_array().into_iter().flatten() {
                    if block["type"] == "tool_use" {
                        send(Ev::Status(describe(block["name"].as_str().unwrap_or("a tool"), &block["input"])));
                    }
                }
            }
            Some("result") => {
                finished = true;
                let text = v["result"].as_str().unwrap_or("").trim().to_string();
                let low = text.to_lowercase();
                if v["is_error"].as_bool().unwrap_or(false) {
                    send(if low.contains("/login") || low.contains("invalid api key") || low.contains("authenticat") { Ev::Missing(Problem::NotLoggedIn) } else { Ev::Failed(text) });
                } else {
                    send(Ev::Done { session: v["session_id"].as_str().map(str::to_string), text });
                }
            }
            _ => {}
        }
    }
    let child = proc_lock(&slot).child.take();
    if let Some(mut c) = child {
        let _ = c.wait();
    }
    if !finished {
        let e = errors.lock().unwrap().trim().to_string();
        send(Ev::Failed(if e.is_empty() { "drui stopped before it answered".into() } else { e }));
    }
}

// ---- drawing helpers

// the dark, faintly see-through fill of agent mode's cards and chatbox (egui cannot blur what is behind it)
fn glass() -> Color32 {
    crate::BG_MENU().gamma_multiply(0.84)
}

// a menu above the chatbox: solid, so the box under it does not show through
fn popup_frame() -> egui::Frame {
    egui::Frame::NONE.fill(crate::BG_MENU()).stroke(Stroke::new(1.0, fig::RULE())).corner_radius(12).shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(110) })
}

// a floating card in that style
fn glass_frame(radius: u8) -> egui::Frame {
    egui::Frame::NONE.fill(glass()).stroke(Stroke::new(1.0, fig::RULE())).corner_radius(radius).shadow(egui::Shadow { offset: [0, 2], blur: 12, spread: 0, color: Color32::from_black_alpha(80) })
}

// the flashing text of Claude Code's own spinner: a bright band sweeping over the letters
fn shimmer(p: &egui::Painter, pos: Pos2, text: &str, t: f64, size: f32) -> Rect {
    let n = text.chars().count() as f64;
    let sweep = (t * 9.0) % (n + 10.0) - 5.0;
    let mut job = LayoutJob::default();
    for (i, c) in text.chars().enumerate() {
        let k = (1.0 - (i as f64 - sweep).abs() / 5.0).max(0.0) as f32;
        let color = fig::AGENT.gamma_multiply(0.45 + 0.55 * k);
        job.append(&c.to_string(), 0.0, TextFormat { font_id: fig::reg(size), color, ..Default::default() });
    }
    let galley = p.layout_job(job);
    let r = Rect::from_min_size(Pos2::new(pos.x, pos.y - galley.size().y / 2.0), galley.size());
    p.galley(r.min, galley, TEXT());
    r
}

// the label hangs from the top border like a phone's notch: solid, rounded at the bottom, flaring into the border at the top, with
// room at its right end for a close button. Returns the notch's body.
fn notch(p: &egui::Painter, top: Pos2, label: &str) -> Rect {
    const R: u8 = 16;
    const FLARE: f32 = 14.0;
    let text = p.layout_no_wrap(label.to_owned(), fig::semi(14.0), fig::AGENT_INK).size().x;
    let w = 20.0 + text + 12.0 + NOTCH_X + 10.0;
    let body = Rect::from_min_size(Pos2::new(top.x - w / 2.0, top.y), Vec2::new(w, NOTCH_H));
    p.rect_filled(body, egui::CornerRadius { nw: 0, ne: 0, sw: R, se: R }, fig::AGENT);
    for (corner, dir) in [(body.left_top(), -1.0_f32), (body.right_top(), 1.0)] {
        // the corner between the border and the notch's side, with a concave curve taken out: a fan of triangles from the corner
        let c = Pos2::new(corner.x + dir * FLARE, corner.y + FLARE);
        let arc: Vec<Pos2> = (0..=10).map(|k| k as f32 / 10.0 * -std::f32::consts::FRAC_PI_2).map(|a| Pos2::new(c.x - dir * FLARE * a.cos(), c.y + FLARE * a.sin())).collect();
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(corner, fig::AGENT);
        arc.iter().for_each(|q| mesh.colored_vertex(*q, fig::AGENT));
        (0..arc.len() as u32 - 1).for_each(|k| mesh.add_triangle(0, 1 + k, 2 + k));
        p.add(egui::Shape::mesh(mesh));
    }
    fig::ptext(p, Pos2::new(body.left() + 20.0, body.center().y), Align2::LEFT_CENTER, label, fig::semi(14.0), fig::AGENT_INK);
    body
}

fn claude_face(p: &egui::Painter, c: Pos2) {
    p.circle_filled(c, AVATAR / 2.0, fig::AGENT);
    paint_glyph_sized(p, c, Glyph::MessageSquare, Color32::WHITE, 13.0);
}

// a kind of thing the bottom chatbox can mention with @
#[derive(Clone, Copy, PartialEq)]
enum What {
    Layer,
    Component,
    Variable,
}

#[derive(Clone, PartialEq)]
struct Mention {
    label: String,
    what: What,
    id: u64,
}

// the messages of a conversation, then the flashing status of a running one with its Stop button; the width is the ui's.
// Returns whether Stop was pressed.
fn paint_messages(ui: &mut egui::Ui, msgs: &[(bool, String)], running: Option<&(String, f64)>) -> bool {
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    let w = ui.available_width();
    for (mine, body) in msgs {
        let galley = ui.painter().layout(body.clone(), fig::reg(CHAT), TEXT(), w - PAD - AVATAR - 8.0 - PAD);
        let r = fig::row(ui, PAD + 16.0 + galley.size().y + PAD);
        let c = Pos2::new(r.left() + PAD + AVATAR / 2.0, r.top() + PAD + AVATAR / 2.0);
        if *mine {
            avatar(ui.painter(), c, AVATAR / 2.0, "");
        } else {
            claude_face(ui.painter(), c);
        }
        let x = r.left() + PAD + AVATAR + 8.0;
        fig::ptext(ui.painter(), Pos2::new(x, r.top() + PAD + 8.0), Align2::LEFT_CENTER, if *mine { "You" } else { "drui" }, fig::semi(11.0), TEXT());
        // a label, not a painted galley, so the words can be selected and copied
        let text = Rect::from_min_size(Pos2::new(x, r.top() + PAD + 16.0), galley.size());
        ui.put(text, egui::Label::new(egui::RichText::new(body.as_str()).font(fig::reg(CHAT)).color(TEXT())).selectable(true));
    }
    let Some((status, t)) = running else { return false };
    let r = fig::row(ui, 68.0);
    claude_face(ui.painter(), Pos2::new(r.left() + PAD + AVATAR / 2.0, r.top() + PAD + AVATAR / 2.0));
    let verbs = ["Thinking", "Pondering", "Designing", "Tweaking", "Polishing"];
    let label = format!("{}…", verbs[(*t / 2.5) as usize % verbs.len()]);
    let x = r.left() + PAD + AVATAR + 8.0;
    shimmer(ui.painter(), Pos2::new(x, r.top() + PAD + 8.0), &label, *t, CHAT);
    if !status.is_empty() {
        fig::ptext(ui.painter(), Pos2::new(x, r.top() + PAD + 28.0), Align2::LEFT_CENTER, status, fig::reg(10.0), TEXT_TERTIARY());
    }
    fig::button(ui, Rect::from_min_size(Pos2::new(r.right() - PAD - 52.0, r.top() + PAD), Vec2::new(52.0, fig::ROW)), "Stop").clicked()
}

// a value set from outside (UI Automation, screen readers) for the text field `id`
fn value_set_from_outside(ui: &egui::Ui, id: Id) -> Option<String> {
    ui.input(|i| {
        i.accesskit_action_requests(id, egui::accesskit::Action::SetValue).find_map(|q| match &q.data {
            Some(egui::accesskit::ActionData::Value(v)) => Some(v.to_string()),
            _ => None,
        })
    })
}

// the "@query" being typed just before the cursor: where the "@" is (in chars) and the text after it
fn mention_query(text: &str, cursor: usize) -> Option<(usize, String)> {
    let before: Vec<char> = text.chars().take(cursor).collect();
    let at = before.iter().rposition(|&c| c == '@')?;
    let q: String = before[at + 1..].iter().collect();
    let alone = at == 0 || before[at - 1].is_whitespace();
    (alone && !q.contains('\n') && q.chars().count() <= 24).then_some((at, q))
}

impl App {
    pub(crate) fn toggle_agent(&mut self) {
        self.agent.on ^= true;
        if self.agent.on {
            self.set_tool(crate::Tool::Select);
            self.agent.last_sel = self.sel.clone();
        } else {
            self.agent.open = None;
            self.agent.view = None;
            self.agent.pop_n = 0;
            self.agent_prune();
        }
    }

    // a draft nobody wrote in is not a session
    fn agent_prune(&mut self) {
        let open = self.agent.open;
        self.agent.sessions.retain(|s| !s.msgs.is_empty() || open == Some(s.id));
    }

    fn top_of(&self, id: u64) -> Option<usize> {
        let i = self.index_of(id)?;
        self.chain(i).last().copied().or(Some(i))
    }

    // the layers the user clicked: a new draft beside them (or the session whose copy they clicked into)
    fn agent_open_for(&mut self, sel: &[u64]) {
        let top = sel.first().and_then(|&id| self.top_of(id)).map(|i| self.shapes[i].id);
        if let Some(s) = top.and_then(|t| self.agent.sessions.iter().find(|s| s.copy == Some(t))) {
            self.agent.open = Some(s.id);
            self.agent_prune();
            return;
        }
        let rects: Vec<Rect> = sel.iter().filter_map(|&id| self.index_of(id)).map(|i| self.world_rect(i)).collect();
        let Some(first) = rects.first().copied() else { return };
        let bounds = rects.iter().fold(first, |a, r| a.union(*r));
        let title = match sel {
            [id] => self.index_of(*id).map_or(String::new(), |i| self.shapes[i].name.clone()),
            _ => format!("{} layers", sel.len()),
        };
        let id = self.agent_new_id();
        self.agent.open = Some(id);
        self.agent_prune();
        self.agent.sessions.push(Session { id, layers: sel.to_vec(), title, copy: None, made: None, answered: false, anchor: bounds.left_bottom(), msgs: vec![], claude: None, text: String::new(), focus: true, refs: vec![] });
    }

    fn agent_new_id(&mut self) -> u64 {
        self.agent.next += 1;
        self.agent.next
    }

    // a marker sits on the copy's top right, else on what Claude made, else on the layer itself (an Edit run)
    fn marker_world(&self, s: &Session) -> Pos2 {
        let on = s.copy.or(s.made).or(s.layers.first().copied()).and_then(|e| self.index_of(e));
        on.map_or(s.anchor, |i| self.world_rect(i).right_top())
    }

    // the whole of agent mode for a frame: the colored outline and notch, the markers, the left panel, the boxes, the block while Claude works
    pub(crate) fn agent_ui(&mut self, ctx: &egui::Context, canvas: Rect) {
        self.agent_poll(ctx);
        if !self.agent.on {
            return;
        }
        let now = ctx.input(|i| i.time);
        let busy = self.agent.busy();
        // Esc ends agent mode, unless it is closing the list of mentions
        let esc = !busy && self.agent.pop_n == 0 && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        // a click on a layer opens a box for it
        if self.sel != self.agent.last_sel {
            self.agent.last_sel = self.sel.clone();
            if self.sel.is_empty() {
                if let Some(id) = self.agent.open.filter(|id| self.agent.sessions.iter().any(|s| s.id == *id && s.msgs.is_empty())) {
                    self.agent.sessions.retain(|s| s.id != id);
                    self.agent.open = None;
                }
            } else if !busy {
                let sel = self.sel.clone();
                self.agent_open_for(&sel);
            }
        }
        let area = canvas;
        let layer = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new("agent-frame")));
        layer.rect_stroke(area.shrink(1.0), 0.0, Stroke::new(2.0, fig::AGENT), StrokeKind::Inside);
        let body = notch(&layer, Pos2::new(area.center().x, area.top() + 2.0), "Agent Mode");
        // the notch's close button: a translucent circle at its right end (under the block while Claude works)
        let dot = Rect::from_center_size(Pos2::new(body.right() - 10.0 - NOTCH_X / 2.0, body.center().y), Vec2::splat(NOTCH_X));
        let (mut close, mut hot) = (false, false);
        egui::Area::new(Id::new("agent-close")).fixed_pos(dot.min).order(Order::Foreground).show(ctx, |ui| {
            let (_, resp) = ui.allocate_exact_size(dot.size(), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Close agent mode"));
            hot = resp.hovered();
            close = resp.tip("Close agent mode (Esc)").clicked();
        });
        // painted with the notch (this layer is above the button's own), so it shows
        layer.circle_filled(dot.center(), NOTCH_X / 2.0, Color32::from_black_alpha(if hot { 90 } else { 40 }));
        paint_glyph_sized(&layer, dot.center(), Glyph::Close, fig::AGENT_INK, 14.0);
        self.agent_alert(ctx, area, now);
        self.agent_markers(ctx, canvas);
        if busy {
            self.agent_block(ctx);
        }
        self.agent_panel(ctx, canvas, now);
        self.agent_box(ctx, canvas, now);
        self.agent_input(ctx, canvas, now);
        self.agent_modal(ctx);
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
        if esc || close {
            self.toggle_agent();
        }
    }

    // the markers of sessions that have an answer: a pin like a comment's, on the copy's top right
    fn agent_markers(&mut self, ctx: &egui::Context, canvas: Rect) {
        // the agent panel covers the canvas's left edge; a pin under it would float over it
        let left = self.agent_panel_rect(canvas);
        let mut toggle = None;
        for s in self.agent.sessions.iter().filter(|s| s.answered) {
            let tip = self.to_screen(canvas.min, self.marker_world(s));
            let r = pin_rect(tip);
            if !canvas.contains_rect(r) || r.intersects(left) {
                continue;
            }
            let open = self.agent.open == Some(s.id);
            egui::Area::new(Id::new(("agent-pin", s.id))).fixed_pos(r.min).order(Order::Foreground).show(ctx, |ui| {
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
                draw_pin(ui.painter(), r.left_bottom(), fig::AGENT, None);
                paint_glyph_sized(ui.painter(), r.center(), Glyph::MessageSquare, Color32::WHITE, 16.0);
                if open || resp.hovered() {
                    ui.painter().rect_stroke(r, egui::CornerRadius { nw: 16, ne: 16, se: 16, sw: 0 }, Stroke::new(2.0, Color32::WHITE), StrokeKind::Outside);
                }
                let first = s.msgs.first().map_or(String::new(), |m| m.text.clone());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("drui session: {first}")));
                if resp.tip(first).clicked() {
                    toggle = Some(s.id);
                }
            });
        }
        if let Some(id) = toggle {
            self.agent.open = if self.agent.open == Some(id) { None } else { Some(id) };
            self.agent_prune();
        }
    }

    // the answer to a click or a key while Claude works: a short alert under the notch
    fn agent_alert(&mut self, ctx: &egui::Context, area: Rect, now: f64) {
        let Some(t) = self.agent.alert else { return };
        let age = now - t;
        if age > 2.5 {
            self.agent.alert = None;
            return;
        }
        let k = ((2.5 - age) / 0.5).clamp(0.0, 1.0) as f32;
        egui::Area::new(Id::new("agent-alert")).pivot(Align2::CENTER_TOP).fixed_pos(area.center_top() + Vec2::new(0.0, 2.0 + NOTCH_H + 8.0)).order(Order::Tooltip).interactable(false).show(ctx, |ui| {
            egui::Frame::new().fill(BG().gamma_multiply(k)).stroke(Stroke::new(1.0, fig::AGENT.gamma_multiply(k))).corner_radius(crate::R_LARGE).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
                ui.label(egui::RichText::new("Not allowed while drui is working. You can still pan and zoom.").font(fig::reg(11.0)).color(TEXT().gamma_multiply(k)));
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
    }

    // while Claude edits nothing may be touched: this covers the window and only pans and zooms the canvas
    fn agent_block(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let origin = self.view.min;
        egui::Area::new(Id::new("agent-block")).fixed_pos(screen.min).order(Order::Foreground).show(ctx, |ui| {
            let (_, resp) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
            ctx.set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
            if resp.dragged() {
                self.pan += resp.drag_delta();
            }
            // anything but looking around is answered with an alert
            let typed = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::Text(_) | egui::Event::Paste(_))));
            if resp.clicked() || resp.secondary_clicked() || resp.double_clicked() || typed {
                self.agent.alert = Some(ui.input(|i| i.time));
            }
            if resp.hovered() {
                self.pan += crate::canvas::wheel_delta(ui);
                let (zoom, hover) = ui.input(|i| (i.zoom_delta(), i.pointer.hover_pos()));
                if let (true, Some(h)) = (zoom != 1.0 && zoom.is_finite() && zoom > 0.0, hover) {
                    let w = self.to_world(origin, h);
                    self.zoom = (self.zoom * zoom).clamp(0.05, 64.0);
                    self.pan = h - origin - w.to_vec2() * self.zoom;
                }
            }
        });
    }

    // where the agent cards float: the left panel's place and width, 20px in from the canvas edges
    fn agent_panel_rect(&self, canvas: Rect) -> Rect {
        let inset = 20.0;
        Rect::from_min_size(Pos2::new(canvas.left() + inset, canvas.top() + inset), Vec2::new(self.left_w, (canvas.height() - inset - 20.0).max(120.0)))
    }

    // the left panel of agent mode: one card per session over a dark backdrop; a card opens its conversation here
    fn agent_panel(&mut self, ctx: &egui::Context, canvas: Rect, now: f64) {
        let r = self.agent_panel_rect(canvas);
        let view = self.agent.view.and_then(|id| self.agent.sessions.iter().find(|s| s.id == id)).map(|s| s.id);
        // what the panel draws, copied out so the drawing does not hold the app
        let cards: Vec<(u64, String, String, bool)> = self
            .agent
            .sessions
            .iter()
            .rev()
            .filter(|s| !s.msgs.is_empty())
            .map(|s| {
                let running = self.agent.run.as_ref().is_some_and(|r| r.session == s.id);
                let state = if running { "Working…" } else if s.answered { "Done" } else { "No answer yet" };
                (s.id, s.msgs.iter().find(|m| m.mine).map_or(s.title.clone(), |m| m.text.replace('\n', " ")), format!("{} · {state}", s.title), running)
            })
            .collect();
        let convo = view.and_then(|id| self.agent.sessions.iter().find(|s| s.id == id)).map(|s| (s.id, s.title.clone(), s.msgs.iter().map(|m| (m.mine, m.text.clone())).collect::<Vec<_>>()));
        let running = view.and_then(|id| self.agent.run.as_ref().filter(|r| r.session == id)).map(|r| (r.status.clone(), now - r.since));
        let busy = self.agent.busy();
        let (mut back, mut stop, mut delete, mut pick) = (false, false, None::<u64>, None::<u64>);
        egui::Area::new(Id::new("agent-panel")).fixed_pos(r.min).order(Order::Middle).fade_in(false).show(ctx, |ui| {
            // the area would offer only the room it had last frame, so a growing conversation got cut off: give it the whole height
            let room = Rect::from_min_size(ui.next_widget_position(), Vec2::new(r.width(), r.height()));
            ui.scope_builder(egui::UiBuilder::new().max_rect(room), |ui| {
            ui.set_width(r.width());
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            if let Some((id, title, msgs)) = &convo {
                // the open session: one floating card, as tall as its conversation up to the screen
                glass_frame(crate::R_LARGE).show(ui, |ui| {
                    ui.set_width(r.width() - 2.0);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let hr = fig::row(ui, 44.0);
                    let at = |x: f32| Rect::from_min_size(Pos2::new(x, hr.top() + 10.0), Vec2::splat(fig::ROW));
                    back = fig::icon_button(ui, at(hr.left() + 8.0), Glyph::ChevronLeft, "Back to the sessions", false).clicked();
                    if fig::icon_button(ui, at(hr.right() - 32.0), Glyph::Delete, "Delete this session", false).clicked() {
                        delete = Some(*id);
                    }
                    let head = if title.is_empty() { "drui".to_string() } else { format!("drui · {title}") };
                    fig::ptext(&ui.painter().with_clip_rect(hr.shrink2(Vec2::new(44.0, 0.0))), Pos2::new(hr.left() + 44.0, hr.center().y), Align2::LEFT_CENTER, head, fig::semi(11.0), TEXT());
                    fig::rule(ui);
                    let out = egui::ScrollArea::vertical().max_height((r.height() - COMPOSER - 48.0).max(80.0)).auto_shrink([false, true]).stick_to_bottom(true).show(ui, |ui| stop = paint_messages(ui, msgs, running.as_ref()));
                    // the area is sized from the last frame's content: draw once more when the content changed
                    let (key, h) = (Id::new("agent-convo-height"), out.content_size.y);
                    if ui.data(|d| d.get_temp::<f32>(key)) != Some(h) {
                        ui.data_mut(|d| d.insert_temp(key, h));
                        ui.ctx().request_repaint();
                    }
                });
            } else {
                // the sessions: one floating card each, 12px apart, straight on the canvas
                egui::ScrollArea::vertical().max_height((r.height() - COMPOSER).max(80.0)).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    for (id, title, sub, running) in &cards {
                        let card = fig::row(ui, 64.0);
                        let resp = ui.interact(card, Id::new(("agent-card", *id)), Sense::click());
                        ui.painter().add(egui::Shadow { offset: [0, 2], blur: 12, spread: 0, color: Color32::from_black_alpha(80) }.as_shape(card, crate::R_LARGE as f32));
                        ui.painter().rect(card, crate::R_LARGE, if resp.hovered() { fig::INPUT() } else { glass() }, Stroke::new(1.0, fig::RULE()), StrokeKind::Inside);
                        let clip = ui.painter().with_clip_rect(Rect::from_min_max(card.min, Pos2::new(card.right() - 44.0, card.bottom())));
                        fig::ptext(&clip, Pos2::new(card.left() + PAD, card.top() + PAD + 7.0), Align2::LEFT_CENTER, title, fig::semi(11.0), TEXT());
                        if *running {
                            shimmer(&clip, Pos2::new(card.left() + PAD, card.top() + PAD + 25.0), sub, now, 10.0);
                        } else {
                            fig::ptext(&clip, Pos2::new(card.left() + PAD, card.top() + PAD + 25.0), Align2::LEFT_CENTER, sub, fig::reg(10.0), TEXT_SECONDARY());
                        }
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Open session: {title}")));
                        let gone = fig::icon_button(ui, Rect::from_min_size(Pos2::new(card.right() - 8.0 - fig::ROW, card.center().y - fig::ROW / 2.0), Vec2::splat(fig::ROW)), Glyph::Delete, "Delete session", false).clicked();
                        if gone {
                            delete = Some(*id);
                        } else if resp.clicked() {
                            pick = Some(*id);
                        }
                        fig::row(ui, 12.0);
                    }
                });
            }
            });
        });
        if let Some(id) = delete.filter(|_| !busy) {
            self.agent.sessions.retain(|s| s.id != id);
            for slot in [&mut self.agent.open, &mut self.agent.view] {
                if *slot == Some(id) {
                    *slot = None;
                }
            }
        } else if let Some(id) = pick {
            self.agent.view = Some(id);
        } else if back {
            self.agent.view = None;
        } else if stop {
            if let Some(id) = view {
                self.agent_stop(id);
            }
        }
    }

    // the box beside a layer (the per-layer chat): its messages, the flashing status while it runs, the field to write in
    fn agent_box(&mut self, ctx: &egui::Context, canvas: Rect, now: f64) {
        let Some(id) = self.agent.open else { return };
        let Some(s) = self.agent.sessions.iter().find(|s| s.id == id) else { return };
        // a new box hangs under the layers; an answered one sits beside its marker
        let tip = self.to_screen(canvas.min, if s.answered { self.marker_world(s) } else { s.anchor });
        let pos = if s.answered { Pos2::new(tip.x + 40.0, tip.y - 32.0) } else { Pos2::new(tip.x, tip.y + 8.0) };
        let (title, msgs, focus) = (s.title.clone(), s.msgs.iter().map(|m| (m.mine, m.text.clone())).collect::<Vec<_>>(), s.focus);
        let mut text = s.text.clone();
        let (mut inplace, mut reply) = (self.agent.inplace, !self.agent.no_reply);
        let running = self.agent.run.as_ref().filter(|r| r.session == id).map(|r| (r.status.clone(), now - r.since));
        let busy = self.agent.busy();
        let (mut close, mut stop, mut send) = (false, false, false);
        egui::Area::new(Id::new("agent-box")).fixed_pos(pos).constrain_to(canvas).order(Order::Tooltip).show(ctx, |ui| {
            thread_frame().show(ui, |ui| {
                ui.set_width(WIDTH);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let hr = fig::row(ui, 40.0);
                let head = if title.is_empty() { "drui".to_string() } else { format!("drui · {title}") };
                fig::ptext(&ui.painter().with_clip_rect(hr.with_max_x(hr.right() - 40.0)), Pos2::new(hr.left() + 16.0, hr.center().y), Align2::LEFT_CENTER, head, fig::semi(11.0), TEXT());
                close = !busy && fig::icon_button(ui, Rect::from_min_size(Pos2::new(hr.right() - 32.0, hr.top() + 8.0), Vec2::splat(fig::ROW)), Glyph::Close, "Close", false).clicked();
                fig::rule(ui);
                stop = thread_scroll(ui, ("agent-box-scroll", id), canvas.height(), |ui| paint_messages(ui, &msgs, running.as_ref()));
                if running.is_none() {
                    egui::Frame::NONE.inner_margin(egui::Margin::same(PAD as i8)).show(ui, |ui| {
                        let tid = Id::new(("agent-input", id));
                        // Enter sends, Shift+Enter starts a new line
                        if ui.memory(|m| m.has_focus(tid)) && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                            send = !text.trim().is_empty();
                        }
                        let boxed = egui::Frame::NONE.fill(fig::INPUT()).corner_radius(12).inner_margin(egui::Margin { left: 12, right: 44, top: 10, bottom: 10 }).show(ui, |ui| {
                            ui.set_width(WIDTH - 2.0 * PAD - 12.0 - 44.0);
                            let hint = if msgs.is_empty() { "Ask drui to change this" } else { "Ask for another change" };
                            let out = ui.add(egui::TextEdit::multiline(&mut text).id(tid).frame(egui::Frame::NONE).font(fig::reg(CHAT)).hint_text(hint).desired_rows(1).desired_width(f32::INFINITY).margin(egui::Margin::ZERO).text_color(TEXT()));
                            out.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Message to drui"));
                            if let Some(v) = value_set_from_outside(ui, out.id) {
                                text = v;
                            }
                            if focus {
                                out.request_focus();
                            }
                        });
                        let r = boxed.response.rect;
                        ui.painter().rect_stroke(r, 12, Stroke::new(1.0, fig::AGENT), StrokeKind::Inside);
                        // centred on a one-line field, on the last line of a tall one
                        let y = if r.height() < 60.0 { r.center().y } else { r.bottom() - 8.0 - AVATAR / 2.0 };
                        if send_button_with(ui, Pos2::new(r.right() - 8.0 - AVATAR / 2.0, y), !text.trim().is_empty(), "Send to drui", fig::AGENT) {
                            send = true;
                        }
                        // what a request does: change the layers where they are, or in a copy; and whether Claude writes back
                        fig::row(ui, PAD);
                        let row = fig::row(ui, fig::ROW);
                        if let Some(k) = fig::text_segments(ui, Rect::from_min_size(row.min, Vec2::new(144.0, fig::ROW)), &[("Edit", inplace, true), ("Duplicate", !inplace, true)]) {
                            inplace = k == 0;
                        }
                        fig::checkbox(ui, Rect::from_min_size(Pos2::new(row.right() - 100.0, row.top()), Vec2::new(100.0, fig::ROW)), &mut reply, "Write reply");
                    });
                }
            });
        });
        if let Some(s) = self.agent.sessions.iter_mut().find(|s| s.id == id) {
            s.text = text;
            s.focus = false;
        }
        (self.agent.inplace, self.agent.no_reply) = (inplace, !reply);
        if close {
            self.agent.open = None;
            self.agent_prune();
        } else if stop {
            self.agent_stop(id);
        } else if send {
            self.agent_send(ctx, id);
        }
    }

    // what @ can name: layers, components and variables whose name holds the query
    fn mention_items(&self, q: &str) -> Vec<Mention> {
        let q = q.trim().to_lowercase();
        let ok = |n: &str| q.is_empty() || n.to_lowercase().contains(&q);
        let mut out: Vec<Mention> = vec![];
        let layer = |s: &crate::model::Shape, what: What| Mention { label: s.name.clone(), what, id: s.id };
        out.extend(self.shapes.iter().filter(|s| !s.component && !s.name.is_empty() && ok(&s.name)).take(5).map(|s| layer(s, What::Layer)));
        out.extend(self.shapes.iter().filter(|s| s.component && ok(&s.name)).take(3).map(|s| layer(s, What::Component)));
        out.extend(self.variables.iter().flat_map(|c| c.vars.iter()).filter(|v| ok(&v.name)).take(3).map(|v| Mention { label: v.name.clone(), what: What::Variable, id: v.id }));
        out
    }

    // the composer at the bottom of the left panel, laid out like Claude's desktop composer (measured on its Chat tab at 144 dpi, CSS px: radius 12,
    // a 20px "+" centred 21.3 in and 21.7 above the bottom, a 32px round send button) in the colors of the agent panel, with @ to name layers,
    // components and variables. What it does follows from the context line under the field: it continues the conversation open in the panel; with
    // none open and a layer selected it changes that layer (Edit or Duplicate); with nothing selected it creates something new.
    fn agent_input(&mut self, ctx: &egui::Context, canvas: Rect, now: f64) {
        let panel = self.agent_panel_rect(canvas);
        let busy = self.agent.busy();
        let w = panel.width();
        // what a send does: the context line, whether it has the Edit / Duplicate switch, and the field's hint
        let sel_name = match self.sel.as_slice() {
            [] => String::new(),
            [id] => self.index_of(*id).map_or(String::new(), |i| self.shapes[i].name.clone()),
            l => format!("{} layers", l.len()),
        };
        let (context, edits, hint) = match self.agent.view.and_then(|v| self.agent.sessions.iter().find(|s| s.id == v)) {
            Some(s) if s.layers.is_empty() => (format!("Continuing: {}", s.title), false, "Ask for another change…"),
            Some(s) => (format!("Changing: {}", s.title), true, "Ask for another change…"),
            None if !self.sel.is_empty() => (format!("Changing: {sel_name}"), true, "Ask drui to change this…"),
            None => ("New design".to_string(), false, "Ask drui to create something…"),
        };
        let mut inplace = self.agent.inplace;
        let mut draft = std::mem::take(&mut self.agent.draft);
        let mentions = self.agent.mentions.clone();
        let (mut send, mut stop, mut plus) = (false, false, false);
        let mut picked: Option<(usize, usize, Mention)> = None;
        let tid = Id::new("agent-chatbox");
        // with the list of mentions open the arrows and Enter drive it instead of the text
        let (mut step, mut take) = (0_i32, false);
        if self.agent.pop_n > 0 && ctx.memory(|m| m.has_focus(tid)) {
            ctx.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    step = 1;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    step = -1;
                }
                take = i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    self.agent.pop_n = 0;
                    self.agent.pop_off = true;
                }
            });
        } else if ctx.memory(|m| m.has_focus(tid)) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            send = !draft.trim().is_empty();
        }
        let sel_row = (self.agent.pop_sel as i32 + step).max(0) as usize;
        let mut popup_n = 0;
        egui::Area::new(Id::new("agent-input")).pivot(Align2::LEFT_BOTTOM).fixed_pos(panel.left_bottom()).order(Order::Tooltip).show(ctx, |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                let t = buf.as_str();
                let mut spans: Vec<(usize, usize)> = vec![];
                for m in &mentions {
                    let tag = format!("@{}", m.label);
                    let mut from = 0;
                    while let Some(i) = t[from..].find(&tag) {
                        spans.push((from + i, from + i + tag.len()));
                        from += i + tag.len();
                    }
                }
                spans.sort();
                let mut job = LayoutJob::default();
                job.wrap.max_width = wrap;
                let fmt = |c: Color32| TextFormat { font_id: fig::reg(CHAT), color: c, line_height: Some(24.0), ..Default::default() };
                let mut at = 0;
                for (a, b) in spans {
                    if a < at {
                        continue;
                    }
                    job.append(&t[at..a], 0.0, fmt(TEXT()));
                    job.append(&t[a..b], 0.0, fmt(fig::AGENT));
                    at = b;
                }
                job.append(&t[at..], 0.0, fmt(TEXT()));
                ui.painter().layout_job(job)
            };
            let boxed = glass_frame(12).inner_margin(egui::Margin { left: 16, right: 16, top: 10, bottom: 0 }).show(ui, |ui| {
                ui.set_width(w - 2.0 - 32.0);
                let region = ui.scope(|ui| {
                    ui.set_min_height(48.0);
                    // egui's caret is as tall as the 24px line: it is drawn below at the height of the text instead
                    ui.visuals_mut().text_cursor.stroke = Stroke::NONE;
                    egui::TextEdit::multiline(&mut draft).id(tid).frame(egui::Frame::NONE).font(fig::reg(CHAT)).desired_rows(1).desired_width(f32::INFINITY).margin(egui::Margin::ZERO).interactive(!busy).layouter(&mut layouter).show(ui)
                });
                let out = region.inner;
                if ui.interact(region.response.rect, Id::new("agent-chatbox-region"), Sense::click()).clicked() && !busy {
                    out.response.request_focus();
                }
                out.response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Message to drui"));
                if let Some(v) = value_set_from_outside(ui, out.response.id) {
                    draft = v;
                }
                if out.response.has_focus() && !busy {
                    if let Some(cr) = out.cursor_range.filter(|_| (ui.input(|i| i.time) / 0.5) as i64 % 2 == 0) {
                        let c = out.galley.pos_from_cursor(cr.primary);
                        let h = ui.fonts_mut(|f| f.row_height(&fig::reg(CHAT)));
                        let (x, y) = (out.galley_pos.x + c.left(), out.galley_pos.y + c.center().y);
                        ui.painter().line_segment([Pos2::new(x, y - h / 2.0), Pos2::new(x, y + h / 2.0)], Stroke::new(1.5, TEXT()));
                    }
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                }
                if draft.is_empty() {
                    fig::ptext(ui.painter(), Pos2::new(out.response.rect.left(), out.response.rect.top() + 12.0), Align2::LEFT_CENTER, hint, fig::reg(CHAT), TEXT_TERTIARY());
                }
                // the "@query" before the cursor drives the list of mentions
                let cursor = out.cursor_range.map_or(draft.chars().count(), |c| c.primary.index.0);
                let query = mention_query(&draft, cursor).filter(|_| out.response.has_focus() && !self.agent.pop_off);
                let items = query.as_ref().map(|(_, q)| self.mention_items(q)).unwrap_or_default();
                popup_n = items.len();
                let row = fig::row(ui, 42.8);
                let c = row.center().y + 0.3;
                // + : mention something
                let plus_r = Rect::from_center_size(Pos2::new(row.left() - 16.0 + 21.3 - 1.0, c), Vec2::splat(32.0));
                let pr = ui.interact(plus_r, Id::new("agent-plus"), Sense::click());
                if pr.hovered() {
                    ui.painter().rect_filled(plus_r, 8, Color32::from_white_alpha(14));
                }
                paint_glyph_sized(ui.painter(), plus_r.center(), Glyph::Plus, TEXT(), 20.0);
                pr.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, !busy, "Mention a layer, component or variable"));
                plus = pr.tip("Mention a layer, component or variable (@)").clicked() && !busy;
                // send, or stop while Claude works
                let sc = Pos2::new(row.right() + 16.0 - 8.0 - 16.0 - 1.0, c);
                let sr = Rect::from_center_size(sc, Vec2::splat(32.0));
                let sresp = ui.interact(sr, Id::new("agent-send"), Sense::click());
                let ready = busy || !draft.trim().is_empty();
                ui.painter().circle_filled(sc, 16.0, if ready { fig::AGENT } else { fig::AGENT.gamma_multiply(0.4) });
                if busy {
                    ui.painter().rect_filled(Rect::from_center_size(sc, Vec2::splat(10.0)), 2, fig::AGENT_INK);
                } else {
                    paint_glyph_sized(ui.painter(), sc, Glyph::ArrowUp, fig::AGENT_INK, 18.0);
                }
                sresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ready, if busy { "Stop drui" } else { "Send to drui" }));
                if sresp.clicked() {
                    if busy {
                        stop = true;
                    } else if ready {
                        send = true;
                    }
                }
                // what a send does, and for a layer how: change it where it is, or in a copy
                let cr = fig::row(ui, fig::ROW);
                let clip = ui.painter().with_clip_rect(cr);
                fig::ptext(&clip, Pos2::new(cr.left(), cr.center().y), Align2::LEFT_CENTER, &context, fig::reg(12.0), TEXT_SECONDARY());
                if edits {
                    let sw = fig::row(ui, fig::ROW);
                    if let Some(k) = fig::text_segments(ui, Rect::from_min_size(sw.min, Vec2::new(144.0, fig::ROW)), &[("Edit", inplace, !busy), ("Duplicate", !inplace, !busy)]) {
                        inplace = k == 0;
                    }
                }
                fig::row(ui, 12.0);
                (out, query, items)
            });
            let (out, query, items) = boxed.inner;
            let r = boxed.response.rect;
            // the list of mentions, above the box
            if let Some((at, _)) = query.filter(|_| !items.is_empty()) {
                let sel = sel_row.min(items.len() - 1);
                if take {
                    picked = Some((at, out.cursor_range.map_or(draft.chars().count(), |c| c.primary.index.0), items[sel].clone()));
                }
                egui::Area::new(Id::new("agent-mentions")).pivot(Align2::LEFT_BOTTOM).fixed_pos(Pos2::new(r.left(), r.top() - 8.0)).order(Order::Tooltip).show(ui.ctx(), |ui| {
                    popup_frame().inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                        ui.set_width(340.0);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for (k, m) in items.iter().enumerate() {
                            let row = fig::row(ui, 32.0);
                            let resp = ui.interact(row, Id::new(("agent-mention", k)), Sense::click());
                            if k == sel || resp.hovered() {
                                ui.painter().rect_filled(row, 8, Color32::from_white_alpha(if k == sel { 22 } else { 12 }));
                            }
                            let (g, kind) = match m.what {
                                What::Layer => (Glyph::Frame, "Layer"),
                                What::Component => (Glyph::Component, "Component"),
                                What::Variable => (Glyph::Sliders, "Variable"),
                            };
                            paint_glyph_sized(ui.painter(), Pos2::new(row.left() + 16.0, row.center().y), g, TEXT_SECONDARY(), 14.0);
                            fig::ptext(&ui.painter().with_clip_rect(row.with_max_x(row.right() - 90.0)), Pos2::new(row.left() + 32.0, row.center().y), Align2::LEFT_CENTER, &m.label, fig::reg(CLAUDE_ROW), TEXT());
                            fig::ptext(ui.painter(), Pos2::new(row.right() - 12.0, row.center().y), Align2::RIGHT_CENTER, kind, fig::reg(12.0), TEXT_SECONDARY());
                            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Mention {} {}", kind, m.label)));
                            if resp.clicked() {
                                picked = Some((at, out.cursor_range.map_or(draft.chars().count(), |c| c.primary.index.0), m.clone()));
                            }
                        }
                    });
                });
            }
        });
        let _ = now;
        // after the frame: arrows move the highlight, a pick replaces "@query" with "@Name " and puts the cursor behind it
        self.agent.pop_n = popup_n;
        self.agent.pop_sel = if popup_n == 0 { 0 } else { sel_row.min(popup_n - 1) };
        let typed_since = draft != self.agent.last_draft;
        if typed_since {
            self.agent.pop_off = false;
        }
        if let Some((at, cursor, m)) = picked {
            let chars: Vec<char> = draft.chars().collect();
            let tag = format!("@{} ", m.label);
            let rebuilt: String = chars[..at.min(chars.len())].iter().collect::<String>() + &tag + &chars[cursor.min(chars.len())..].iter().collect::<String>();
            let end = at + tag.chars().count();
            draft = rebuilt;
            if !self.agent.mentions.contains(&m) {
                self.agent.mentions.push(m);
            }
            let mut st = egui::TextEdit::load_state(ctx, tid).unwrap_or_default();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(end))));
            st.store(ctx, tid);
            ctx.memory_mut(|m| m.request_focus(tid));
        }
        if plus {
            if !draft.is_empty() && !draft.ends_with(char::is_whitespace) {
                draft.push(' ');
            }
            draft.push('@');
            let mut st = egui::TextEdit::load_state(ctx, tid).unwrap_or_default();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(draft.chars().count()))));
            st.store(ctx, tid);
            ctx.memory_mut(|m| m.request_focus(tid));
            self.agent.pop_off = false;
        }
        self.agent.last_draft = draft.clone();
        self.agent.inplace = inplace;
        if stop {
            if let Some(run) = &self.agent.run {
                let id = run.session;
                self.agent_stop(id);
            }
            self.agent.draft = draft;
        } else if send && !busy {
            self.agent.draft = draft;
            self.agent_send_chatbox(ctx);
        } else {
            self.agent.draft = draft;
        }
    }

    // the composer's text goes to the conversation open in the panel; else to the box beside the selected layers (the draft nobody has written in),
    // else it starts one: about the selection, or a general request to create something when nothing is selected. @-named things are context only.
    fn agent_send_chatbox(&mut self, ctx: &egui::Context) {
        let text = std::mem::take(&mut self.agent.draft);
        let mentions: Vec<Mention> = std::mem::take(&mut self.agent.mentions).into_iter().filter(|m| text.contains(&format!("@{}", m.label))).collect();
        if text.trim().is_empty() {
            return;
        }
        let exists = |id: &u64| self.agent.sessions.iter().any(|s| s.id == *id);
        let draft_box = self.agent.open.filter(|id| self.agent.sessions.iter().any(|s| s.id == *id && s.msgs.is_empty() && !s.layers.is_empty()) && !self.sel.is_empty());
        let id = match self.agent.view.filter(exists).or(draft_box) {
            Some(id) => id,
            None => {
                let layers = self.sel.clone();
                let title = match layers.as_slice() {
                    [] => text.split_whitespace().take(5).collect::<Vec<_>>().join(" "),
                    [id] => self.index_of(*id).map_or(String::new(), |i| self.shapes[i].name.clone()),
                    l => format!("{} layers", l.len()),
                };
                let id = self.agent_new_id();
                let anchor = self.view.center();
                let anchor = self.to_world(self.view.min, anchor);
                self.agent.sessions.push(Session { id, layers, title, copy: None, made: None, answered: false, anchor, msgs: vec![], claude: None, text: String::new(), focus: false, refs: vec![] });
                id
            }
        };
        self.agent.view = Some(id);
        if let Some(s) = self.agent.sessions.iter_mut().find(|s| s.id == id) {
            s.text = text;
            s.refs = mentions;
        }
        self.agent_send(ctx, id);
    }

    // starts the REST API unless it runs; false when it cannot, and the modal says why (and can try again)
    fn agent_connect(&mut self, ctx: &egui::Context, port: u16) -> bool {
        if self.api.is_none() {
            self.toggle_api(ctx, port);
        }
        if self.api.is_some() {
            if self.agent.problem == Some(Problem::NoApi) {
                self.agent.problem = None;
            }
            return true;
        }
        self.agent.api_error = self.status.strip_prefix("REST API failed to start: ").unwrap_or(&self.status).to_string();
        self.agent.problem = Some(Problem::NoApi);
        false
    }

    // the message was not sent: it goes back into the chatbox it came from (a session's own box still holds it)
    fn agent_keep_message(&mut self, id: u64) {
        if let Some(s) = self.agent.sessions.iter_mut().find(|s| s.id == id) {
            if self.agent.view == Some(id) || s.layers.is_empty() {
                self.agent.draft = std::mem::take(&mut s.text);
                self.agent.mentions = std::mem::take(&mut s.refs);
            }
        }
    }

    fn agent_stop(&mut self, id: u64) {
        if let Some(run) = self.agent.run.take() {
            proc_lock(&run.child).stop();
            run.unlocked.iter().for_each(|t| {
                self.api_owned.remove(t);
            });
        }
        if let Some(s) = self.agent.sessions.iter_mut().find(|s| s.id == id) {
            s.msgs.push(Msg { mine: false, text: "Stopped. Whatever it had changed so far is in the design; Ctrl+Z undoes it step by step.".into() });
        }
    }

    // sends a session's text: the first message tells Claude what is selected and mentioned, a later one resumes the same Claude session
    fn agent_send(&mut self, ctx: &egui::Context, id: u64) {
        if self.agent.busy() {
            return;
        }
        if !self.agent_connect(ctx, 0) {
            self.agent_keep_message(id);
            return;
        }
        let Some(api) = &self.api else { return };
        let api = (format!("http://127.0.0.1:{}", api.port), api.token.clone());
        let Some(k) = self.agent.sessions.iter().position(|s| s.id == id) else { return };
        let text = std::mem::take(&mut self.agent.sessions[k].text).trim().to_string();
        if text.is_empty() {
            return;
        }
        let refs = std::mem::take(&mut self.agent.sessions[k].refs);
        let s = &self.agent.sessions[k];
        let layers: Vec<String> = s.layers.iter().take(8).filter_map(|&l| self.index_of(l)).map(|i| format!("#{} \"{}\" ({:?})", self.shapes[i].id, self.shapes[i].name, self.shapes[i].kind)).collect();
        let mut tops: Vec<u64> = s.layers.iter().filter_map(|&l| self.top_of(l)).map(|i| self.shapes[i].id).collect();
        tops.dedup();
        let general = s.layers.is_empty();
        // Edit / Duplicate and Write reply are about changing selected layers: a general request always writes its reply and builds new things
        let (inplace, reply) = (self.agent.inplace && !general, !self.agent.no_reply || general);
        let plan = plan_for(&text, !general || !refs.is_empty());
        let how = if inplace {
            "Change the design right where it is: the original frames are unlocked for this request, so do NOT call POST /claude_edit and ignore the skill's rule about duplicating first."
        } else if let Some(c) = s.copy {
            &format!("Keep editing the claude_edit copy (layer #{c}) you made before; change only the copy.")
        } else if general {
            "Anything you create is yours to edit. If you change an existing frame, first duplicate it with POST /claude_edit and change only the copy."
        } else {
            "Duplicate the top-level frame with POST /claude_edit and change only the copy."
        };
        let ending = if !reply { "When you are done, write no reply at all: answer with just a period." } else { "When you are done, reply in one to three plain sentences about what you changed." };
        let named: Vec<String> = refs
            .iter()
            .map(|m| match m.what {
                What::Layer => format!("layer #{} \"{}\"", m.id, m.label),
                What::Component => format!("component #{} \"{}\"", m.id, m.label),
                What::Variable => format!("variable #{} \"{}\" (GET /variables has it)", m.id, m.label),
            })
            .collect();
        let mentioned = if named.is_empty() { String::new() } else { format!(" The user mentioned with @: {}.", named.join(", ")) };
        let scope = if general { "This is a general request to create something new, not to edit a selection; look at the design with GET /outline if you need to.".to_string() } else { format!("They selected: {}. The top-level frame holding the selection is {}.", layers.join(", "), tops.first().and_then(|&t| self.index_of(t)).map(|i| format!("#{} \"{}\"", self.shapes[i].id, self.shapes[i].name)).unwrap_or_default()) };
        let research = if plan.research { " This request is open-ended: before you design anything, research online with WebSearch and WebFetch (a few focused searches on how good examples of this kind of design are laid out, the usual sections, patterns and sizes), decide on a plan, then build it in the design through the API. Make sensible choices instead of asking questions." } else { "" };
        let prompt = if s.claude.is_some() {
            format!("Another request from the same user about the same design. {scope}{mentioned} {how} Do not add notes.{research}\n\n{text}\n\n{ending}")
        } else {
            format!("You are editing a design in drui, the design tool the user has open. {scope}{mentioned}\n\nTheir request:\n{text}\n\nThe drui REST API is already running. Read skills/drui/SKILL.md in the current directory first and use it: call the API with `python dr.py`. When you create or change a screen, page or layout, also read skills/drui-ux/SKILL.md and follow it. {how} Do not add notes.{research} {ending}")
        };
        // an Edit run may change the original frames: open them to the API until it ends (all of them for a general request)
        let unlock: Vec<u64> = if !inplace {
            vec![]
        } else if general {
            self.shapes.iter().filter(|s| s.parent.is_none()).map(|s| s.id).collect()
        } else {
            tops
        };
        let unlocked: Vec<u64> = unlock.into_iter().filter(|t| self.api_owned.insert(*t)).collect();
        let before: HashSet<u64> = self.shapes.iter().filter(|s| s.parent.is_none()).map(|s| s.id).collect();
        let resume = s.claude.clone();
        self.agent.sessions[k].msgs.push(Msg { mine: true, text });
        let (tx, rx) = channel();
        let slot = Arc::<Mutex<Proc>>::default();
        let (c, sl) = (ctx.clone(), slot.clone());
        std::thread::spawn(move || run_claude(c, prompt, resume, plan, api, sl, tx));
        self.agent.run = Some(Run { session: id, rx, child: slot, status: plan.label.to_string(), since: ctx.input(|i| i.time), unlocked, before, inplace, reply });
    }

    fn agent_poll(&mut self, ctx: &egui::Context) {
        let Some(run) = self.agent.run.as_mut() else { return };
        let Some(ev) = drain(&run.rx, &mut run.status) else { return };
        let run = self.agent.run.take();
        let id = run.as_ref().map_or(0, |r| r.session);
        for t in run.iter().flat_map(|r| &r.unlocked) {
            self.api_owned.remove(t);
        }
        let Some(k) = self.agent.sessions.iter().position(|s| s.id == id) else { return };
        match ev {
            Ev::Done { session, text } => {
                let s = &mut self.agent.sessions[k];
                s.claude = session.or(s.claude.take());
                s.answered = true;
                if run.as_ref().is_none_or(|r| r.reply) {
                    let s = &mut self.agent.sessions[k];
                    s.msgs.push(Msg { mine: false, text: if text.is_empty() { "Done.".into() } else { text } });
                }
                self.agent_found_copy(k, run.as_ref().map(|r| &r.before), run.as_ref().is_some_and(|r| r.inplace));
            }
            Ev::Failed(e) => self.agent.sessions[k].msgs.push(Msg { mine: false, text: format!("drui could not finish: {e}") }),
            Ev::Missing(p) => {
                // the message goes back into the field, to be sent once Claude Code works
                let to_chatbox = self.agent.view == Some(id) || self.agent.sessions[k].layers.is_empty();
                if let Some(m) = self.agent.sessions[k].msgs.pop().filter(|m| m.mine) {
                    if to_chatbox {
                        self.agent.draft = m.text;
                    } else {
                        self.agent.sessions[k].text = m.text;
                    }
                }
                self.agent.problem = Some(p);
            }
            Ev::Status(_) => {}
        }
        ctx.request_repaint();
    }

    // after a run: the copy of the top-level frame (Duplicate), or the first new top-level layer, is where the marker goes
    fn agent_found_copy(&mut self, k: usize, before: Option<&HashSet<u64>>, inplace: bool) {
        if !inplace {
            if let Some(top) = self.agent.sessions[k].layers.first().and_then(|&l| self.top_of(l)) {
                let name = format!("{} {EDIT_SUFFIX}", self.shapes[top].name);
                if let Some(i) = self.shapes.iter().position(|s| s.parent.is_none() && s.name.trim_end() == name) {
                    let copy = self.shapes[i].id;
                    self.agent.sessions[k].copy = Some(copy);
                    self.sel = vec![copy];
                    self.agent.last_sel = self.sel.clone();
                    return;
                }
            }
        }
        let made = before.and_then(|b| self.shapes.iter().find(|s| s.parent.is_none() && !b.contains(&s.id)).map(|s| s.id));
        if let Some(m) = made {
            self.agent.sessions[k].made = Some(m);
            // what Claude built is selected and brought into view (the cards and the chatbox float over the canvas's edges)
            self.sel = vec![m];
            self.agent.last_sel = self.sel.clone();
            self.zoom_fit(true);
        }
    }

    // Claude Code is missing or not signed in: what to do about it
    fn agent_modal(&mut self, ctx: &egui::Context) {
        let Some(problem) = self.agent.problem else { return };
        let (title, body, command) = match problem {
            Problem::NotInstalled => ("Claude Code is not installed", "Agent mode runs your own Claude Code on this computer. Install it, sign in, then send your message again.".to_string(), Some(INSTALL)),
            Problem::NotLoggedIn => ("Claude Code is not signed in", "Claude Code is installed but you are not signed in. Open a terminal, run claude and sign in (type /login), then send your message again.".to_string(), Some("claude")),
            Problem::NoApi => (
                "drui could not start its API",
                format!("Agent mode edits your design through drui's local REST API, and it could not start: {}. A firewall or security program may be blocking drui from listening on 127.0.0.1. Allow it, try again, then send your message again.", self.agent.api_error),
                None,
            ),
        };
        let (mut done, mut retry) = (false, false);
        egui::Area::new(Id::new("agent-modal")).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).order(Order::Tooltip).show(ctx, |ui| {
            thread_frame().show(ui, |ui| {
                ui.set_width(360.0);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let hr = fig::row(ui, 40.0);
                fig::ptext(ui.painter(), Pos2::new(hr.left() + 16.0, hr.center().y), Align2::LEFT_CENTER, title, fig::semi(11.0), TEXT());
                done |= fig::icon_button(ui, Rect::from_min_size(Pos2::new(hr.right() - 32.0, hr.top() + 8.0), Vec2::splat(fig::ROW)), Glyph::Close, "Close", false).clicked();
                fig::rule(ui);
                let galley = ui.painter().layout(body, fig::reg(11.0), TEXT_SECONDARY(), 328.0);
                let r = fig::row(ui, galley.size().y + 2.0 * PAD);
                ui.painter().galley(Pos2::new(r.left() + 16.0, r.top() + PAD), galley, TEXT_SECONDARY());
                if let Some(command) = command {
                    let r = fig::row(ui, 24.0);
                    let code = Rect::from_min_max(Pos2::new(r.left() + 16.0, r.top()), Pos2::new(r.right() - 16.0, r.bottom()));
                    ui.painter().rect(code, fig::RADIUS, crate::theme::SUNKEN(), Stroke::new(1.0, fig::RULE()), StrokeKind::Inside);
                    fig::ptext(ui.painter(), Pos2::new(code.left() + 8.0, code.center().y), Align2::LEFT_CENTER, command, egui::FontId::monospace(11.0), TEXT());
                }
                let r = fig::row(ui, 24.0 + 2.0 * PAD);
                if fig::button(ui, Rect::from_min_size(Pos2::new(r.right() - 16.0 - 60.0, r.top() + PAD), Vec2::new(60.0, fig::ROW)), "Close").clicked() {
                    done = true;
                }
                let (label, w) = if command.is_some() { ("Copy command", 100.0) } else { ("Try again", 80.0) };
                if fig::button(ui, Rect::from_min_size(Pos2::new(r.right() - 16.0 - 60.0 - 8.0 - w, r.top() + PAD), Vec2::new(w, fig::ROW)), label).clicked() {
                    match command {
                        Some(c) => ui.ctx().copy_text(c.to_owned()),
                        None => retry = true,
                    }
                }
            });
        });
        if retry {
            self.agent_connect(ctx, 0); // closes the modal when it works, else shows the new reason
        }
        if done {
            self.agent.problem = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;

    fn frame(ctx: &egui::Context, a: &mut App, t: f64) {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))), time: Some(t), ..Default::default() };
        let _ = ctx.run_ui(raw, |ui| a.run_frame(ui));
    }

    // the model and effort follow the request: a tweak is small and quick, a vague brief is researched on the bigger model
    #[test]
    fn the_model_follows_how_big_the_request_is() {
        let p = |t: &str, target: bool| plan_for(t, target).label;
        assert_eq!(p("make the button pink", true), SIMPLE.label);
        assert_eq!(p("make the card radius bigger", true), SIMPLE.label);
        assert_eq!(p("rename it to Pay now", true), SIMPLE.label);
        assert_eq!(p("add a subtle gradient and make it feel more premium", true), NORMAL.label, "no plain property to change: the normal model");
        assert_eq!(p("create new button", false), NORMAL.label);
        assert_eq!(p("Help create input field", false), NORMAL.label);
        assert_eq!(p("help build a mobile app page dashboard", false), DEEP.label);
        assert_eq!(p("redesign the checkout page", true), DEEP.label);
        assert_eq!(p("find some inspiration for the cards", true), DEEP.label);
        assert!(DEEP.research && !SIMPLE.research && !NORMAL.research);
    }

    #[test]
    fn the_mention_query_is_the_at_word_before_the_cursor() {
        assert_eq!(mention_query("make @but", 9), Some((5, "but".into())));
        assert_eq!(mention_query("make @", 6), Some((5, String::new())));
        assert_eq!(mention_query("a@b", 3), None, "an @ inside a word is not a mention");
        assert_eq!(mention_query("make @but now", 5), None, "the cursor is before the @");
    }

    // Stop before the process exists must keep it from ever starting; Stop after takes the child out of the slot
    #[test]
    fn a_stop_before_the_process_starts_is_not_lost() {
        let slot = Mutex::new(Proc::default());
        proc_lock(&slot).stop();
        let started = launch(&slot, || panic!("the process must not start after Stop"));
        assert!(started.is_none());

        let slot = Mutex::new(Proc::default());
        #[cfg(windows)]
        let (prog, args) = ("cmd", ["/c", "ping -n 30 127.0.0.1"]);
        #[cfg(not(windows))]
        let (prog, args) = ("sleep", ["30", ""]);
        let started = launch(&slot, || quiet(Command::new(prog).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())).spawn());
        assert!(started.is_some_and(|r| r.is_ok_and(|p| p.0.is_some() && p.1.is_some() && p.2.is_some())));
        assert!(proc_lock(&slot).child.is_some());
        proc_lock(&slot).stop();
        let p = proc_lock(&slot);
        assert!(p.stopped && p.child.is_none(), "stop took the child to kill it");
    }

    // a worker thread that dies without a word ends the run instead of leaving it busy for good
    #[test]
    fn a_dead_worker_ends_the_run() {
        let mut status = String::new();
        let (tx, rx) = channel();
        assert!(drain(&rx, &mut status).is_none(), "still running");
        tx.send(Ev::Status("Reading the design".into())).unwrap();
        assert!(drain(&rx, &mut status).is_none() && status == "Reading the design");
        drop(tx);
        assert!(matches!(drain(&rx, &mut status), Some(Ev::Failed(_))));
        let (tx, rx) = channel();
        tx.send(Ev::Done { session: None, text: "ok".into() }).unwrap();
        drop(tx);
        assert!(matches!(drain(&rx, &mut status), Some(Ev::Done { .. })), "an answer sent before the thread ended still arrives");
    }

    // the API cannot start (its port is taken): the modal opens with the reason, the message goes back to the chatbox, Try again connects
    #[test]
    fn a_failed_api_start_opens_the_modal_and_keeps_the_message() {
        let ctx = egui::Context::default();
        let mut a = App::default();
        let taken = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = taken.local_addr().unwrap().port();
        assert!(!a.agent_connect(&ctx, port));
        assert!(a.agent.problem == Some(Problem::NoApi) && !a.agent.api_error.is_empty() && a.api.is_none());

        a.agent.sessions.push(Session { id: 1, layers: vec![], title: String::new(), copy: None, made: None, answered: false, anchor: Pos2::ZERO, msgs: vec![], claude: None, text: "make a card".into(), focus: false, refs: vec![] });
        a.agent_keep_message(1);
        assert_eq!(a.agent.draft, "make a card");

        drop(taken);
        assert!(a.agent_connect(&ctx, port), "Try again works once the port is free");
        assert!(a.agent.problem.is_none() && a.api.is_some());
    }

    // each run is told its own drui's address, whatever the shared file in the temp folder says
    #[test]
    fn the_run_gets_its_own_api_address() {
        let cmd = claude_cmd("claude", Path::new("."), SIMPLE, Some("abc"), &("http://127.0.0.1:1234".into(), "tok".into()));
        let env = |k: &str| cmd.get_envs().find(|(n, _)| *n == k).and_then(|(_, v)| v).map(|v| v.to_string_lossy().into_owned());
        assert_eq!((env("DRUI_API_URL"), env("DRUI_API_TOKEN")), (Some("http://127.0.0.1:1234".into()), Some("tok".into())));
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.windows(2).any(|w| w == ["--resume", "abc"]));
    }

    // a click on a layer opens its box; the bottom chatbox, the panel's cards and conversation, the marker, the modal and the block draw
    #[test]
    fn agent_mode_draws_its_box_panel_chatbox_and_markers() {
        let ctx = egui::Context::default();
        let mut a = App::default();
        a.add(Kind::Rect, Pos2::new(200.0, 200.0));
        let id = a.shapes[0].id;
        a.sel.clear();
        a.toggle_agent();
        frame(&ctx, &mut a, 0.1);
        assert!(a.agent.open.is_none(), "nothing is selected yet");
        a.sel = vec![id];
        frame(&ctx, &mut a, 0.2);
        let open = a.agent.open.expect("the click opens a box");
        // an answered session shows as a marker; its conversation opens in the panel; a run blocks the window and flashes
        a.agent.sessions[0].msgs = vec![Msg { mine: true, text: "Make it blue".into() }, Msg { mine: false, text: "Done. I made it blue.".into() }];
        a.agent.sessions[0].answered = true;
        a.agent.problem = Some(Problem::NotLoggedIn);
        frame(&ctx, &mut a, 0.3);
        a.agent.problem = Some(Problem::NoApi);
        a.agent.api_error = "Address already in use".into();
        frame(&ctx, &mut a, 0.35);
        a.agent.problem = Some(Problem::NotInstalled);
        a.agent.view = Some(open);
        a.agent.draft = "make @".into();
        let (_tx, rx) = channel();
        a.agent.run = Some(Run { session: open, rx, child: Arc::default(), status: "Applying changes".into(), since: 0.0, unlocked: vec![], before: HashSet::new(), inplace: false, reply: true });
        frame(&ctx, &mut a, 0.4);
        assert!(a.agent.busy());
        a.agent.run = None;
        a.agent.view = None;
        frame(&ctx, &mut a, 0.45);
        a.sel = vec![];
        frame(&ctx, &mut a, 0.5);
        // Esc ends agent mode
        a.toggle_agent();
        assert!(!a.agent.on && a.agent.open.is_none());
        assert_eq!(a.agent.sessions.len(), 1, "the answered session is kept, an empty draft is not");
    }
}
