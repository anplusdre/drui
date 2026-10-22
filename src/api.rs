// The local REST API: an HTTP server on 127.0.0.1 with a bearer token, so scripts and Claude Code can read and edit the open file.
//
// Worker threads only parse HTTP; every request goes to the app over a channel and is answered once per frame by
// `App::poll_api`, inside the frame's undo window, so an edit made through the API is one undo step like any other.
// Only loopback connections and `Host: 127.0.0.1` / `localhost` are served, and every request needs the token
// (`Authorization: Bearer <token>`, or `?token=`). A browser page cannot send that header without a CORS preflight, and no CORS
// headers are ever sent, so web pages cannot reach it.
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use egui::{Color32, Pos2, Rect, Vec2};
use serde_json::{Value, json};

use crate::model::*;
use crate::App;

const MAX_BODY: usize = 64 << 20;
// An agent in a runaway loop must not be able to flood the app: at most RATE requests a second (a burst of BURST), MAX_CONNS at once.
const RATE: f64 = 30.0;
const BURST: f64 = 60.0;
const MAX_CONNS: usize = 32;

const PRO_ROUTES: [&str; 0] = [];

// the agent's instructions (docs/agent.md), with the paid parts' routes where they are built in
fn guide() -> String {
    #[allow(unused_mut)]
    let mut g = include_str!("../docs/agent.md").to_string();
    g
}

// the routes `GET /` lists
fn routes() -> Vec<&'static str> {
    [
        "GET /guide", "GET /outline?root=&depth=", "POST /claude_edit {id}", "POST /notes {text, near, side}", "GET /doc", "GET /pages", "GET /layers", "GET|PATCH|DELETE /layers/{id}", "POST /layers", "POST /patch",
        "GET|PUT /selection", "GET|PUT /variables", "GET|POST /comments", "PATCH|DELETE /comments/{id}",
        "GET /export/{id|page}?format=svg|png|pdf&scale=2",
        "POST /undo", "POST /redo", "POST /save {path?}", "POST /open {path, discard?}", "POST /import {path, discard?}", "POST /pages {name?}", "PUT|DELETE /pages/{i}",
        "POST /images {path|data}", "POST /instances {main, parent?, x, y}", "POST /pages/{i}/duplicate", "POST /copy {ids?}", "POST /paste", "POST /tools/{cut|erase|builder|paint}", "POST|DELETE /present", "GET|PUT /view", "GET|PUT /motion {time, length, loop, play}", "PATCH|DELETE /styles/{kind}/{id} {confirm}", "PUT /branches/{i}/review", "GET /styles", "POST /styles/{color|effects|texts|grids|anims}", "GET|POST /versions", "POST /versions/{i}/restore {confirm}", "GET|POST /branches", "PUT|DELETE /branches/{i}", "POST /branches/{i}/review {reviewer}", "POST /branches/{i}/merge",
        "GET /libraries", "POST /libraries {path}", "POST /libraries/publish {path}", "POST /libraries/{id}/update",
        "GET /actions", "POST /actions {name, ids?}", "POST /align {op, ids?}", "POST /distribute {horizontal, ids?}",
    ]
    .into_iter()
    .chain(PRO_ROUTES)
    .collect()
}

// a token bucket shared by every connection
pub(crate) struct Bucket {
    pub(crate) tokens: f64,
    pub(crate) at: std::time::Instant,
}

impl Bucket {
    pub(crate) fn take(&mut self) -> bool {
        let now = std::time::Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.at).as_secs_f64() * RATE).min(BURST);
        self.at = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

pub struct Req {
    pub method: String,
    pub path: Vec<String>, // the path split at '/', percent-decoding left to the callers (ids and names only)
    pub query: HashMap<String, String>,
    pub body: Vec<u8>,
    reply: Sender<Resp>,
}

pub struct Resp {
    pub code: u16,
    pub ctype: &'static str,
    pub body: Vec<u8>,
}

impl Resp {
    pub fn json(code: u16, v: Value) -> Resp {
        Resp { code, ctype: "application/json", body: v.to_string().into_bytes() }
    }
    pub fn err(code: u16, msg: impl std::fmt::Display) -> Resp {
        Resp::json(code, json!({ "error": msg.to_string() }))
    }
}

// (a named type: tools/publish.py cuts gated items at the first top-level comma, so a gated signature must not have one)
pub(crate) type Answer = Result<Resp, Resp>;

pub struct Api {
    pub port: u16,
    pub token: String,
    rx: Receiver<Req>,
}

// where a note of size `size` sits beside `r`: 32px off the chosen side, level with its top (or left) edge
fn note_anchor(r: Rect, side: &str, size: Vec2) -> Option<Pos2> {
    Some(match side {
        "right" => Pos2::new(r.max.x + 32.0, r.min.y),
        "left" => Pos2::new(r.min.x - 32.0 - size.x, r.min.y),
        "top" => Pos2::new(r.min.x, r.min.y - 32.0 - size.y),
        "bottom" => Pos2::new(r.min.x, r.max.y + 32.0),
        _ => return None,
    })
}

// `r` moved past the notes in `taken`: down beside a layer's left / right, right above or below it
fn slide_past(mut r: Rect, taken: &[Rect], side: &str) -> Rect {
    for _ in 0..200 {
        let Some(o) = taken.iter().find(|o| o.expand(8.0).intersects(r)) else { break };
        r = if matches!(side, "top" | "bottom") { r.translate(Vec2::new(o.max.x + 12.0 - r.min.x, 0.0)) } else { r.translate(Vec2::new(0.0, o.max.y + 12.0 - r.min.y)) };
    }
    r
}

impl App {
    // Notes that are about a layer stay beside it as it moves, and never overlap each other, whatever height their text ended up with.
    // Idempotent: a note that is where it should be is not touched.
    pub fn place_notes(&mut self) {
        let free: Vec<Rect> = self.shapes.iter().filter(|s| s.parent.is_none() && s.name.starts_with(NOTE_NAME) && s.note_of.is_none()).map(|s| Rect::from_min_size(s.pos, s.size)).collect();
        let mut linked: Vec<usize> = (0..self.shapes.len()).filter(|&i| self.shapes[i].parent.is_none() && self.shapes[i].name.starts_with(NOTE_NAME) && self.shapes[i].note_of.is_some()).collect();
        if linked.is_empty() {
            return;
        }
        linked.sort_by_key(|&i| self.shapes[i].id);
        let mut taken = free;
        for i in linked {
            let (size, side) = (self.shapes[i].size, ["right", "left", "top", "bottom"][self.shapes[i].note_side.min(3) as usize]);
            let Some(t) = self.shapes[i].note_of.and_then(|id| self.index_of(id)) else {
                // the layer is gone: the note stays where it is
                taken.push(Rect::from_min_size(self.shapes[i].pos, size));
                continue;
            };
            let at = note_anchor(self.world_rect(t), side, size).unwrap_or(self.shapes[i].pos);
            let r = slide_past(Rect::from_min_size(at, size), &taken, side);
            if (r.min - self.shapes[i].pos).abs().max_elem() > 0.5 {
                self.shapes[i].pos = r.min;
            }
            taken.push(r);
        }
    }
}

// the reply of an export: the text files, and the pictures (vector and image layers) as base64
fn bundle_json((files, assets): (Vec<(String, String)>, Vec<(String, Vec<u8>)>)) -> Value {
    use base64::Engine;
    let files: serde_json::Map<String, Value> = files.into_iter().map(|(n, c)| (n, json!(c))).collect();
    let assets: serde_json::Map<String, Value> = assets.into_iter().map(|(n, b)| (n, json!(base64::engine::general_purpose::STANDARD.encode(b)))).collect();
    json!({ "files": files, "assets": assets })
}

fn random_token() -> String {
    // RandomState is seeded from the operating system, so each hasher gives 64 fresh bits
    let bits = || std::collections::hash_map::RandomState::new().build_hasher().finish();
    format!("{:016x}{:016x}", bits(), bits())
}

// binds 127.0.0.1:port (0 = any free port) and serves until the program ends
pub fn start(ctx: egui::Context, port: u16) -> Result<Api, String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let token = random_token();
    let (tx, rx) = channel::<Req>();
    let t = token.clone();
    let bucket = Arc::new(Mutex::new(Bucket { tokens: BURST, at: std::time::Instant::now() }));
    let live = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            if live.load(Ordering::SeqCst) >= MAX_CONNS {
                continue; // dropped: the client sees a closed connection
            }
            live.fetch_add(1, Ordering::SeqCst);
            let (tx, ctx, t, bucket, live) = (tx.clone(), ctx.clone(), t.clone(), bucket.clone(), live.clone());
            std::thread::spawn(move || {
                serve(conn, port, &t, tx, ctx, &bucket);
                live.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    Ok(Api { port, token, rx })
}

fn serve(mut conn: TcpStream, port: u16, token: &str, tx: Sender<Req>, ctx: egui::Context, bucket: &Mutex<Bucket>) {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(10)));
    let resp = match read_request(&mut conn, port, token) {
        Ok(_) if !bucket.lock().map(|mut b| b.take()).unwrap_or(false) => Resp::err(429, format!("too many requests: at most {RATE} a second")),
        Ok((method, target, body)) => {
            let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
            let query: HashMap<String, String> = query.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
            let path = path.split('/').filter(|s| !s.is_empty()).map(String::from).collect();
            let (reply, wait) = channel();
            if tx.send(Req { method, path, query, body, reply }).is_ok() {
                ctx.request_repaint();
                wait.recv_timeout(Duration::from_secs(15)).unwrap_or_else(|_| Resp::err(504, "the app did not answer in time"))
            } else {
                Resp::err(503, "the app is closing")
            }
        }
        Err(r) => r,
    };
    let reason = match resp.code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        429 => "Too Many Requests",
        _ => "Error",
    };
    let head = format!("HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", resp.code, reason, resp.ctype, resp.body.len());
    let _ = conn.write_all(head.as_bytes());
    let _ = conn.write_all(&resp.body);
}

// reads one request; Err carries the answer to send instead (bad request, wrong token, wrong host)
fn read_request(conn: &mut TcpStream, port: u16, token: &str) -> Result<(String, String, Vec<u8>), Resp> {
    if !conn.peer_addr().map(|a| a.ip().is_loopback()).unwrap_or(false) {
        return Err(Resp::err(403, "loopback only"));
    }
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        if buf.len() > 64 << 10 {
            return Err(Resp::err(400, "headers too large"));
        }
        match conn.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(Resp::err(400, "incomplete request")),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    let mut lines = head.lines();
    let mut first = lines.next().unwrap_or("").split_whitespace();
    let (method, target) = (first.next().unwrap_or("").to_uppercase(), first.next().unwrap_or("").to_string());
    let headers: HashMap<String, String> = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string())).collect();
    let host = headers.get("host").map(|h| h.to_lowercase()).unwrap_or_default();
    if host != format!("127.0.0.1:{port}") && host != format!("localhost:{port}") {
        return Err(Resp::err(403, "bad Host header"));
    }
    let query_token = target.split_once('?').and_then(|(_, q)| q.split('&').find_map(|kv| kv.strip_prefix("token=")));
    let given = headers.get("authorization").and_then(|a| a.strip_prefix("Bearer ")).or(query_token);
    if given != Some(token) {
        return Err(Resp::err(401, "missing or wrong token"));
    }
    let len: usize = headers.get("content-length").and_then(|l| l.parse().ok()).unwrap_or(0);
    if len > MAX_BODY {
        return Err(Resp::err(400, "body too large"));
    }
    let mut body = buf[end + 4..].to_vec();
    while body.len() < len {
        match conn.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(Resp::err(400, "incomplete body")),
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(len);
    Ok((method, target, body))
}

// layers the agent edits live under a top-level frame whose name ends with this (a duplicate of the original), see docs/agent.md
pub const EDIT_SUFFIX: &str = "claude_edit";
// explanations the agent leaves next to the design
pub const NOTE_NAME: &str = "claude_note";

fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim().trim_start_matches('#');
    let h: String = if h.len() == 3 || h.len() == 4 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    let v = u32::from_str_radix(&h, 16).ok()?;
    match h.len() {
        6 => Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)),
        8 => Some(Color32::from_rgba_unmultiplied((v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8)),
        _ => None,
    }
}

fn hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
}

// "#rrggbb" (or null for none) as the JSON of a paint list
fn paints_json(v: &Value) -> Result<Value, String> {
    match v {
        Value::Null => Ok(json!([])),
        Value::String(h) => {
            let c = parse_hex(h).ok_or_else(|| format!("{h:?} is not a colour like #1a2b3c"))?;
            Ok(json!([serde_json::to_value(Paint::solid(c)).map_err(|e| e.to_string())?]))
        }
        _ => Err("fill / stroke is a hex string such as \"#1a2b3c\", or null; use fills / strokes for gradients".into()),
    }
}

// shallow merge of `patch` into a shape (nested values are replaced whole), keeping what must not change.
// Shorthands: x, y, w, h, radius (one number), fill and stroke ("#hex" or null); the file's own field names work as well.
fn merge_shape(s: &Shape, patch: &Value) -> Result<Shape, String> {
    let mut v = serde_json::to_value(s).map_err(|e| e.to_string())?;
    let (Some(obj), Some(p)) = (v.as_object_mut(), patch.as_object()) else { return Err("a layer is a JSON object".into()) };
    for (k, val) in p {
        match k.as_str() {
            "id" | "ref" | "parent_ref" => {}
            "x" | "y" => obj["pos"][k.as_str()] = val.clone(),
            "w" => obj["size"]["x"] = val.clone(),
            "h" => obj["size"]["y"] = val.clone(),
            "radius" if val.is_number() => {
                obj.insert(k.clone(), json!([val, val, val, val]));
            }
            "fill" => {
                obj.insert("fills".into(), paints_json(val)?);
            }
            "stroke" => {
                obj.insert("strokes".into(), paints_json(val)?);
            }
            _ => {
                obj.insert(k.clone(), val.clone());
            }
        }
    }
    serde_json::from_value(v).map_err(|e| e.to_string())
}

impl App {
    // answers the requests that arrived since the last frame (called once per frame, inside the undo window)
    pub(crate) fn poll_api(&mut self, ctx: &egui::Context) {
        let Some(api) = &self.api else { return };
        let reqs: Vec<Req> = api.rx.try_iter().collect();
        for r in reqs {
            let resp = self.api_handle(ctx, &r);
            let _ = r.reply.send(resp);
        }
    }

    // the one place that edits the document on behalf of scripts: the API's POST /patch (and the plugins' results, in the paid build)
    pub(crate) fn apply_patch(&mut self, p: &Value) -> Result<String, String> {
        let mut report = Vec::new();
        let mut refs: HashMap<String, u64> = HashMap::new();
        let ids = |v: &Value| -> Vec<u64> { v.as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default() };
        for id in ids(&p["remove"]) {
            if self.index_of(id).is_some() {
                self.remove_subtree(id);
                report.push(format!("removed {id}"));
            }
        }
        for patch in p["set"].as_array().into_iter().flatten() {
            let id = patch["id"].as_u64().ok_or("set: every entry needs an id")?;
            let i = self.index_of(id).ok_or_else(|| format!("set: no layer {id}"))?;
            self.shapes[i] = merge_shape(&self.shapes[i], patch)?;
        }
        let mut added = Vec::new();
        for new in p["add"].as_array().into_iter().flatten() {
            let mut s = merge_shape(&Shape::default(), new)?;
            s.id = self.new_id();
            if let Some(r) = new["ref"].as_str() {
                refs.insert(r.to_string(), s.id);
            }
            if let Some(r) = new["parent_ref"].as_str() {
                s.parent = Some(*refs.get(r).ok_or_else(|| format!("add: parent_ref {r} is not an earlier ref"))?);
            }
            if s.parent.is_some_and(|pid| self.index_of(pid).is_none()) {
                return Err(format!("add: parent of {} does not exist", s.name));
            }
            if s.name.is_empty() {
                s.name = format!("{:?}", s.kind);
            }
            added.push(s.id);
            self.shapes.push(s);
        }
        if !added.is_empty() {
            report.push(format!("added {}", added.len()));
        }
        if p["select"].is_array() {
            let sel = ids(&p["select"]);
            self.sel = sel.into_iter().filter(|&id| self.index_of(id).is_some()).collect();
        } else if !added.is_empty() {
            self.sel = added;
        }
        self.prune_sel();
        self.sizes.clear();
        if let Some(s) = p["status"].as_str() {
            self.status = s.to_string();
        } else if !report.is_empty() {
            self.status = report.join(", ");
        }
        Ok(report.join(", "))
    }

    fn api_handle(&mut self, ctx: &egui::Context, r: &Req) -> Resp {
        let seg: Vec<&str> = r.path.iter().map(String::as_str).collect();
        let body = || -> Result<Value, Resp> { serde_json::from_slice(&r.body).map_err(|e| Resp::err(400, format!("body is not JSON: {e}"))) };
        let id_of = |s: &str| s.parse::<u64>().map_err(|_| Resp::err(400, "ids are numbers"));
        let res: Result<Resp, Resp> = (|| match (r.method.as_str(), seg.as_slice()) {
            ("GET", []) => Ok(Resp::json(
                200,
                json!({
                    "app": "drui", "file": self.file.display().to_string(), "page": self.cur,
                    "pages": self.pages.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                    "guide": "GET /guide: how to work here, read it first",
                    "routes": routes(),
                }),
            )),
            ("GET", ["doc"]) => Ok(Resp::json(200, serde_json::to_value(self.doc()).map_err(|e| Resp::err(500, e))?)),
            ("GET", ["pages"]) => Ok(Resp::json(
                200,
                Value::Array(self.pages.iter().enumerate().map(|(i, p)| json!({ "index": i, "name": p.name, "current": i == self.cur, "layers": if i == self.cur { self.shapes.len() } else { p.shapes.len() } })).collect()),
            )),
            ("GET", ["guide"]) => Ok(Resp { code: 200, ctype: "text/markdown; charset=utf-8", body: guide().into_bytes() }),
            ("POST", ["undo"]) | ("POST", ["redo"]) => {
                self.do_undo(seg[0] == "undo");
                Ok(Resp::json(200, json!({ "undo_steps": self.undo.len(), "redo_steps": self.redo.len() })))
            }
            ("POST", ["save"]) => {
                let path = body().ok().and_then(|v| v["path"].as_str().map(PathBuf::from)).unwrap_or_else(|| self.file.clone());
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    return Err(Resp::err(400, "drui files are .json"));
                }
                self.save_to(path);
                Ok(Resp::json(200, json!({ "status": self.status, "file": self.file.display().to_string() })))
            }
            ("POST", ["open"]) | ("POST", ["import"]) => {
                let v = body()?;
                let path = PathBuf::from(v["path"].as_str().ok_or_else(|| Resp::err(400, "expected {\"path\": \"file\"}"))?);
                // replacing the open document would throw away what the person has not saved
                if !self.undo.is_empty() && v["discard"].as_bool() != Some(true) {
                    return Err(Resp::err(409, "the open document has unsaved edits: POST /save first, or send \"discard\": true"));
                }
                match (seg[0], path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref()) {
                    ("import", Some("fig")) => self.import_fig(path),
                    ("import", Some("sketch")) => self.import_sketch(path),
                    ("import", _) => return Err(Resp::err(400, "import takes a .fig or .sketch file; /open takes a drui file")),
                    _ => self.load_from(path),
                }
                Ok(Resp::json(200, json!({ "status": self.status, "file": self.file.display().to_string() })))
            }
            ("POST", ["pages"]) => {
                self.add_page();
                if let Some(n) = body().ok().and_then(|v| v["name"].as_str().map(String::from)) {
                    self.pages[self.cur].name = n; // only the page just made is named: the person's own pages keep their names
                }
                Ok(Resp::json(200, json!({ "index": self.cur })))
            }
            ("PUT", ["pages", i]) => {
                let i = id_of(i)? as usize;
                if i >= self.pages.len() {
                    return Err(Resp::err(404, "no such page"));
                }
                self.switch_page(i);
                Ok(Resp::json(200, json!({ "ok": true, "current": self.cur })))
            }
            ("DELETE", ["pages", i]) => {
                let i = id_of(i)? as usize;
                if i >= self.pages.len() {
                    return Err(Resp::err(404, "no such page"));
                }
                let empty = if i == self.cur { self.shapes.is_empty() } else { self.pages[i].shapes.is_empty() };
                if !empty {
                    return Err(Resp::err(403, "only an empty page can be deleted through the API"));
                }
                if self.pages.len() < 2 {
                    return Err(Resp::err(400, "the last page cannot be deleted"));
                }
                self.delete_page(i);
                Ok(Resp::json(200, json!({ "ok": true, "current": self.cur })))
            }
            ("GET", ["actions"]) => Ok(Resp::json(200, Value::Array(crate::actions::api_commands()))),
            ("POST", ["actions"]) => {
                let v = body()?;
                if let Some(ids) = v["ids"].as_array() {
                    self.sel = ids.iter().filter_map(Value::as_u64).filter(|&id| self.index_of(id).is_some()).collect();
                }
                let name = v["name"].as_str().ok_or_else(|| Resp::err(400, "expected {\"name\": \"Group selection\"}; GET /actions lists them"))?;
                // like every other write, an action that works on the selection stays out of the person's own designs
                if crate::actions::command_needs(name).unwrap_or(1) > 0 {
                    self.guard_selection().map_err(|e| Resp::err(403, e))?;
                }
                // the three actions that take text or a number, as the Actions palette does ("offset path 8", "rename Button #", "replace a with b")
                match name.to_lowercase().as_str() {
                    "offset path" => {
                        let d = v["value"].as_f64().ok_or_else(|| Resp::err(400, "offset path needs \"value\": px"))?;
                        self.offset_selection(d as f32);
                    }
                    "rename" => {
                        let p = v["pattern"].as_str().ok_or_else(|| Resp::err(400, "rename needs \"pattern\", # = number, * = old name"))?;
                        self.rename_selection(p);
                    }
                    "replace" => {
                        let (f, t) = (v["from"].as_str().filter(|f| !f.is_empty()), v["to"].as_str());
                        let (Some(f), Some(t)) = (f, t) else { return Err(Resp::err(400, "replace needs \"from\" and \"to\"")) };
                        if self.sel.is_empty() {
                            return Err(Resp::err(400, "replace works on the selection: send \"ids\"")); // nothing selected means the whole page in the app
                        }
                        self.replace_text(f, t);
                    }
                    _ => self.run_api_command(name).map_err(|e| Resp::err(400, e))?,
                }
                Ok(Resp::json(200, json!({ "status": self.status, "selection": self.sel })))
            }
            ("POST", ["align"]) => {
                let v = body()?;
                let op = match v["op"].as_str() {
                    Some("left") => crate::ops::AlignOp::Left,
                    Some("hcenter") => crate::ops::AlignOp::HCenter,
                    Some("right") => crate::ops::AlignOp::Right,
                    Some("top") => crate::ops::AlignOp::Top,
                    Some("vcenter") => crate::ops::AlignOp::VCenter,
                    Some("bottom") => crate::ops::AlignOp::Bottom,
                    _ => return Err(Resp::err(400, "op is left, hcenter, right, top, vcenter or bottom")),
                };
                if let Some(ids) = v["ids"].as_array() {
                    self.sel = ids.iter().filter_map(Value::as_u64).filter(|&id| self.index_of(id).is_some()).collect();
                }
                self.guard_selection().map_err(|e| Resp::err(403, e))?;
                self.align(op);
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            ("POST", ["distribute"]) => {
                let v = body()?;
                if let Some(ids) = v["ids"].as_array() {
                    self.sel = ids.iter().filter_map(Value::as_u64).filter(|&id| self.index_of(id).is_some()).collect();
                }
                self.guard_selection().map_err(|e| Resp::err(403, e))?;
                self.distribute(v["horizontal"].as_bool().unwrap_or(true));
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            ("GET", ["outline"]) => {
                let depth = r.query.get("depth").and_then(|d| d.parse::<usize>().ok()).unwrap_or(usize::MAX);
                let roots: Vec<u64> = match r.query.get("root") {
                    Some(id) => vec![id_of(id)?],
                    None => self.shapes.iter().filter(|s| s.parent.is_none()).map(|s| s.id).collect(),
                };
                let kids = self.child_map();
                Ok(Resp::json(200, Value::Array(roots.iter().filter_map(|&id| self.outline_node(id, &kids, depth)).collect())))
            }
            ("POST", ["claude_edit"]) => {
                let id = body()?["id"].as_u64().ok_or_else(|| Resp::err(400, "expected {\"id\": <layer id>}"))?;
                Ok(Resp::json(200, self.claude_edit_copy(id).map_err(|e| Resp::err(400, e))?))
            }
            ("POST", ["notes"]) => Ok(Resp::json(200, self.add_note(&body()?).map_err(|e| Resp::err(400, e))?)),
            ("GET", ["layers"]) => Ok(Resp::json(200, serde_json::to_value(&self.shapes).map_err(|e| Resp::err(500, e))?)),
            ("GET", ["layers", id]) => {
                let i = self.index_of(id_of(id)?).ok_or_else(|| Resp::err(404, "no such layer"))?;
                Ok(Resp::json(200, serde_json::to_value(&self.shapes[i]).map_err(|e| Resp::err(500, e))?))
            }
            ("POST", ["layers"]) => {
                let v = body()?;
                let out = self.api_patch(&json!({ "add": [v] })).map_err(|e| if e.starts_with(GUARD_MSG) { Resp::err(403, e) } else { Resp::err(400, e) })?;
                let id = self.shapes.last().map(|s| s.id);
                Ok(Resp::json(200, json!({ "id": id, "result": out })))
            }
            ("PATCH", ["layers", id]) => {
                let mut v = body()?;
                v["id"] = json!(id_of(id)?);
                self.api_patch(&json!({ "set": [v] })).map_err(|e| if e.starts_with(GUARD_MSG) { Resp::err(403, e) } else { Resp::err(400, e) })?;
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            ("DELETE", ["layers", id]) => {
                let id = id_of(id)?;
                self.index_of(id).ok_or_else(|| Resp::err(404, "no such layer"))?;
                self.guard(id).map_err(|e| Resp::err(403, e))?;
                self.remove_subtree(id);
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            ("POST", ["patch"]) => {
                let out = self.api_patch(&body()?).map_err(|e| if e.starts_with(GUARD_MSG) { Resp::err(403, e) } else { Resp::err(400, e) })?;
                Ok(Resp::json(200, json!({ "ok": true, "result": out })))
            }
            ("GET", ["selection"]) => Ok(Resp::json(200, json!({ "ids": self.sel }))),
            ("PUT", ["selection"]) => {
                let ids = body()?["ids"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect::<Vec<_>>()).ok_or_else(|| Resp::err(400, "expected {\"ids\": [..]}"))?;
                self.sel = ids.into_iter().filter(|&id| self.index_of(id).is_some()).collect();
                Ok(Resp::json(200, json!({ "ids": self.sel })))
            }
            ("GET", ["variables"]) => Ok(Resp::json(200, serde_json::to_value(&self.variables).map_err(|e| Resp::err(500, e))?)),
            ("PUT", ["variables"]) => {
                self.variables = serde_json::from_value(body()?).map_err(|e| Resp::err(400, e))?;
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            ("GET", ["comments"]) => Ok(Resp::json(200, serde_json::to_value(&self.comments).map_err(|e| Resp::err(500, e))?)),
            ("POST", ["comments"]) => {
                let v = body()?;
                let text = v["text"].as_str().ok_or_else(|| Resp::err(400, "a comment needs text"))?.to_string();
                let pos = Pos2::new(v["x"].as_f64().unwrap_or(0.0) as f32, v["y"].as_f64().unwrap_or(0.0) as f32);
                let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                let id = self.new_id();
                let page = v["page"].as_u64().map(|p| p as usize).unwrap_or(self.cur);
                self.comments.push(Comment { id, page, pos, text, resolved: false, author: v["author"].as_str().unwrap_or(&self.user).to_string(), secs, replies: vec![], attachments: vec![], edited: false, reactions: vec![] });
                Ok(Resp::json(200, json!({ "id": id })))
            }
            ("PATCH", ["comments", id]) => {
                let (id, v) = (id_of(id)?, body()?);
                let c = self.comments.iter_mut().find(|c| c.id == id).ok_or_else(|| Resp::err(404, "no such comment"))?;
                if let Some(t) = v["text"].as_str() {
                    c.text = t.to_string();
                    c.edited = true;
                }
                if let Some(b) = v["resolved"].as_bool() {
                    c.resolved = b;
                }
                if let Some(t) = v["reply"].as_str() {
                    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                    c.replies.push(Reply { author: v["author"].as_str().unwrap_or(&self.user).to_string(), secs, text: t.to_string(), attachments: vec![], edited: false });
                }
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            ("DELETE", ["comments", id]) => {
                let id = id_of(id)?;
                let n = self.comments.len();
                self.comments.retain(|c| c.id != id);
                if self.comments.len() == n {
                    return Err(Resp::err(404, "no such comment"));
                }
                Ok(Resp::json(200, json!({ "ok": true })))
            }
            ("GET", ["export", what]) => {
                let roots: Vec<u64> = if *what == "page" { self.shapes.iter().filter(|s| s.parent.is_none()).map(|s| s.id).collect() } else { vec![id_of(what)?] };
                let (svg, _) = self.svg_for(ctx, &roots).ok_or_else(|| Resp::err(404, "nothing to export"))?;
                let scale = r.query.get("scale").and_then(|s| s.parse::<f32>().ok()).unwrap_or(1.0).clamp(0.1, 16.0);
                match r.query.get("format").map(String::as_str).unwrap_or("svg") {
                    "svg" => Ok(Resp { code: 200, ctype: "image/svg+xml", body: svg.into_bytes() }),
                    "png" => Ok(Resp { code: 200, ctype: "image/png", body: crate::export::render_png(&svg, scale).map_err(|e| Resp::err(500, e))? }),
                    "pdf" => Ok(Resp { code: 200, ctype: "application/pdf", body: crate::export::render_pdf(&svg).map_err(|e| Resp::err(500, e))? }),
                    f => Err(Resp::err(400, format!("unknown format {f}"))),
                }
            }
            _ => match self.api_more(r, &seg) {
                Some(res) => res,
                None => self.api_extra(r, &seg),
            },
        })();
        // an edit leaves the layout to be redone and the size cache stale
        if matches!(r.method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE") {
            self.sizes.clear();
        }
        res.unwrap_or_else(|e| e)
    }
}

impl App {

    fn api_extra(&mut self, r: &Req, _seg: &[&str]) -> Answer {
        Err(Resp::err(404, format!("no route for {} /{}", r.method, r.path.join("/"))))
    }
}

impl App {
    // starts the REST API (or stops it when it runs) and tells the user where it is
    pub(crate) fn toggle_api(&mut self, ctx: &egui::Context, port: u16) {
        let info = std::env::temp_dir().join("drui-api.json");
        if self.api.take().is_some() {
            // the listener thread keeps its socket until exit, but without the receiver every request is answered 503
            let _ = std::fs::remove_file(info);
            self.status = "REST API stopped".into();
            return;
        }
        match start(ctx.clone(), port) {
            Ok(api) => {
                let url = format!("http://127.0.0.1:{}", api.port);
                let _ = std::fs::write(&info, json!({ "url": url, "token": api.token, "pid": std::process::id() }).to_string());
                self.status = format!("REST API on {url} (token in {})", info.display());
                self.api = Some(api);
            }
            Err(e) => self.status = format!("REST API failed to start: {e}"),
        }
    }
}

// the message every refusal starts with, so the routes can answer 403 instead of 400
const GUARD_MSG: &str = "layer ";

impl App {
    fn guard_selection(&self) -> Result<(), String> {
        self.sel.iter().try_for_each(|&id| self.guard(id))
    }

    // top-level ancestor of the layer at `i`
    fn root_idx(&self, i: usize) -> usize {
        *self.chain(i).last().unwrap_or(&i)
    }

    // the agent may change a layer only when it sits in a claude_edit copy, a note, or a design it made itself
    pub(crate) fn guard(&self, id: u64) -> Result<(), String> {
        if std::env::var_os("DRUI_API_UNGUARDED").is_some() {
            return Ok(());
        }
        let Some(i) = self.index_of(id) else { return Ok(()) }; // the caller reports a missing layer
        let root = &self.shapes[self.root_idx(i)];
        if root.name.trim_end().ends_with(EDIT_SUFFIX) || root.name.starts_with(NOTE_NAME) || self.api_owned.contains(&root.id) {
            return Ok(());
        }
        Err(format!(
            "{GUARD_MSG}{id} (\"{}\") belongs to the original design \"{}\". Do not edit it: POST /claude_edit {{\"id\": {}}} makes a copy named \"{} {EDIT_SUFFIX}\" and you edit that",
            self.shapes[i].name, root.name, root.id, root.name
        ))
    }

    // apply_patch for API clients: refuses edits outside a claude_edit copy, and remembers the top-level layers it creates
    fn api_patch(&mut self, p: &Value) -> Result<String, String> {
        let set = p["set"].as_array().into_iter().flatten().filter_map(|e| e["id"].as_u64());
        let remove = p["remove"].as_array().into_iter().flatten().filter_map(Value::as_u64);
        for id in set.chain(remove) {
            self.guard(id)?;
        }
        for pid in p["add"].as_array().into_iter().flatten().filter_map(|e| e["parent"].as_u64()) {
            self.guard(pid)?;
        }
        let first = self.next_id;
        let out = self.apply_patch(p)?;
        let made: Vec<u64> = self.shapes.iter().filter(|s| s.id >= first && s.parent.is_none()).map(|s| s.id).collect();
        self.api_owned.extend(made);
        Ok(out)
    }

    fn child_map(&self) -> HashMap<Option<u64>, Vec<usize>> {
        let mut m: HashMap<Option<u64>, Vec<usize>> = HashMap::new();
        for (i, s) in self.shapes.iter().enumerate() {
            m.entry(s.parent).or_default().push(i);
        }
        m
    }

    // one layer and its children as compact JSON: what an agent needs to see a design without the full layer records
    fn outline_node(&self, id: u64, kids: &HashMap<Option<u64>, Vec<usize>>, depth: usize) -> Option<Value> {
        let i = self.index_of(id)?;
        let s = &self.shapes[i];
        let mut n = json!({ "id": s.id, "name": s.name, "kind": format!("{:?}", s.kind), "x": s.pos.x, "y": s.pos.y, "w": s.size.x, "h": s.size.y });
        let o = n.as_object_mut()?;
        if s.parent.is_none() {
            o.insert("editable".into(), json!(self.guard(id).is_ok()));
        }
        let solid = |p: &Paint| if let PaintKind::Solid(c) = p.kind { Some(hex(c)) } else { None };
        if let Some(p) = s.fills.iter().find(|p| p.visible) {
            o.insert("fill".into(), json!(solid(p).unwrap_or_else(|| "gradient/image".into())));
        }
        if let Some(p) = s.strokes.iter().find(|p| p.visible).filter(|_| s.stroke_w > 0.0) {
            o.insert("stroke".into(), json!(solid(p).unwrap_or_else(|| "gradient/image".into())));
            o.insert("stroke_w".into(), json!(s.stroke_w));
        }
        if s.kind == Kind::Text {
            o.insert("text".into(), json!(s.text));
            o.insert("font_size".into(), json!(s.font_size));
        }
        if let Some(l) = &s.layout {
            o.insert("layout".into(), json!(if l.columns > 0 { "grid" } else if l.vertical { "vertical" } else { "horizontal" }));
        }
        for (flag, on) in [("component", s.component), ("instance", s.instance), ("hidden", !s.visible)] {
            if on {
                o.insert(flag.into(), json!(true));
            }
        }
        let children = kids.get(&Some(id)).map(|v| v.as_slice()).unwrap_or(&[]);
        if depth == 0 {
            if !children.is_empty() {
                o.insert("children_count".into(), json!(children.len()));
            }
        } else if !children.is_empty() {
            let list: Vec<Value> = children.iter().filter_map(|&k| self.outline_node(self.shapes[k].id, kids, depth - 1)).collect();
            o.insert("children".into(), Value::Array(list));
        }
        Some(n)
    }

    // POST /claude_edit: a copy of the layer named "<name> claude_edit", placed beside the original (or the copy that already exists)
    fn claude_edit_copy(&mut self, id: u64) -> Result<Value, String> {
        let i = self.index_of(id).ok_or("no such layer")?;
        let orig = self.shapes[i].clone();
        if orig.name.trim_end().ends_with(EDIT_SUFFIX) {
            return Err(format!("layer {id} is already a {EDIT_SUFFIX} copy; edit it directly"));
        }
        let name = format!("{} {EDIT_SUFFIX}", orig.name);
        let kids = self.child_map();
        if let Some(e) = self.shapes.iter().find(|s| s.parent == orig.parent && s.name == name) {
            let eid = e.id;
            return Ok(json!({ "id": eid, "name": name, "existing": true, "outline": self.outline_node(eid, &kids, usize::MAX) }));
        }
        let subtree: Vec<Shape> = self.shapes.iter().filter(|s| self.is_inside(s.id, id)).cloned().collect();
        let mut d = Vec2::splat(10.0);
        if orig.parent.is_none() {
            // beside the original, past every other top-level layer in the way
            let mut r = self.world_rect(i).translate(Vec2::new(orig.size.x.abs() + 80.0, 0.0));
            let others: Vec<Rect> = (0..self.shapes.len()).filter(|&k| self.shapes[k].parent.is_none() && k != i).map(|k| self.world_rect(k)).collect();
            for _ in 0..100 {
                match others.iter().find(|o| o.intersects(r)) {
                    Some(o) => r = r.translate(Vec2::new(o.max.x + 80.0 - r.min.x, 0.0)),
                    None => break,
                }
            }
            d = r.min - self.world_rect(i).min;
        }
        let (copies, map) = self.remap(&subtree, Some(orig.parent), d);
        let new_id = map[&id];
        self.shapes.extend(copies);
        let ni = self.index_of(new_id).ok_or("copy failed")?;
        self.shapes[ni].name = name.clone();
        self.sel = vec![new_id];
        self.sizes.clear();
        let kids = self.child_map();
        Ok(json!({
            "id": new_id, "name": name, "existing": false,
            "ids": map.iter().map(|(a, b)| (a.to_string(), json!(b))).collect::<serde_json::Map<_, _>>(),
            "outline": self.outline_node(new_id, &kids, usize::MAX),
        }))
    }

    // POST /notes: a sticky note frame on the canvas, beside `near` (a layer id) or at x / y, that never overlaps another note
    pub(crate) fn add_note(&mut self, v: &Value) -> Result<Value, String> {
        let text = v["text"].as_str().filter(|t| !t.trim().is_empty()).ok_or("a note needs text")?.to_string();
        let w = v["width"].as_f64().unwrap_or(220.0).clamp(80.0, 800.0) as f32;
        let lines: f32 = text.split('\n').map(|l| (l.chars().count() as f32 * 7.0 / (w - 24.0)).ceil().max(1.0)).sum();
        let h = lines * 16.0 + 24.0; // an estimate; the frame hugs its text once laid out
        let fill = match v["color"].as_str() {
            Some(c) => parse_hex(c).ok_or_else(|| format!("{c:?} is not a colour like #ffe08a"))?,
            None => Color32::from_rgb(0xff, 0xe0, 0x8a),
        };
        let mut target_name = String::new();
        let mut target_id = None;
        let side = v["side"].as_str().unwrap_or("right");
        let mut at = Pos2::new(v["x"].as_f64().unwrap_or(0.0) as f32, v["y"].as_f64().unwrap_or(0.0) as f32);
        if let Some(near) = v["near"].as_u64() {
            let i = self.index_of(near).ok_or("near: no such layer")?;
            let r = self.world_rect(i);
            target_name = self.shapes[i].name.clone();
            target_id = Some(near);
            at = note_anchor(r, side, Vec2::new(w, h)).ok_or_else(|| format!("side is right, left, top or bottom, not {side}"))?;
        } else if v["x"].is_null() {
            let right = self.shapes.iter().filter(|s| s.parent.is_none()).map(|s| s.pos.x + s.size.x).fold(0.0, f32::max);
            at.x = right + 40.0;
        }
        // slide past notes already there: down beside a layer's left / right, right above or below it
        let taken: Vec<Rect> = self.shapes.iter().filter(|s| s.parent.is_none() && s.name.starts_with(NOTE_NAME)).map(|s| Rect::from_min_size(s.pos, s.size)).collect();
        let r = slide_past(Rect::from_min_size(at, Vec2::new(w, h)), &taken, side);
        let fid = self.new_id();
        let n = self.shapes.len() + 1;
        let mut f = Shape::new(Kind::Frame, fid, n, r.min, None);
        f.name = if target_name.is_empty() { NOTE_NAME.into() } else { format!("{NOTE_NAME}: {target_name}") };
        f.size = Vec2::new(w, h);
        f.fills = vec![Paint::solid(fill)];
        f.radius = [8.0; 4];
        f.note_of = target_id;
        f.note_side = match side {
            "left" => 1,
            "top" => 2,
            "bottom" => 3,
            _ => 0,
        };
        f.clip = false;
        f.layout = Some(AutoLayout { vertical: true, gap: 6.0, pad: [12.0; 4], hug_x: false, hug_y: true, ..AutoLayout::default() });
        let tid = self.new_id();
        let mut t = Shape::new(Kind::Text, tid, 1, Pos2::ZERO, Some(fid));
        t.name = "note text".into();
        t.text = text;
        t.font_size = 13.0;
        t.fit = TextFit::AutoHeight;
        t.size = Vec2::new(w - 24.0, 16.0);
        t.fill_x = true;
        t.fills = vec![Paint::solid(Color32::from_rgb(0x1e, 0x1e, 0x1e))];
        self.shapes.push(f);
        self.shapes.push(t);
        self.api_owned.insert(fid);
        self.sizes.clear();
        Ok(json!({ "id": fid, "x": r.min.x, "y": r.min.y, "w": w }))
    }
}
