// Present in a browser: the prototype as one HTML file. Every top-level frame becomes a screen (its SVG), every layer with
// interactions a hot spot over it; a small script follows the interactions: navigate with instant, dissolve and slide
// transitions, back, links, overlays (placed, dimmed, closing on an outside click), key presses, hover and delays.
// Variables live in the script: Set variable and the "Only if" conditions work (the same expressions as present mode, see expr.rs),
// but the screens are pictures, so a variable does not change what is drawn. Easing curves (springs included) are sampled into a CSS
// `linear()` curve. Change to (and While hovering / pressing) puts a picture of the other variant over the instance. Not carried
// over: animations of the page. Smart Animate matches layers of the two screens by name and moves / scales the matching ones (the
// rest fade); Scroll to and the mouse wheel scroll a frame whose content lies beyond its edges (Scroll behavior > Overflow).
use egui::Rect;
use serde_json::{json, Value};

use crate::model::*;
use crate::App;

fn trigger_name(t: Trigger) -> &'static str {
    match t {
        Trigger::Click | Trigger::MouseUp => "click",
        Trigger::Drag | Trigger::MouseDown | Trigger::WhilePress => "down",
        Trigger::MouseEnter | Trigger::WhileHover => "enter",
        Trigger::MouseLeave => "leave",
        Trigger::KeyPress => "key",
        Trigger::AfterDelay | Trigger::VideoTime | Trigger::VideoEnd => "delay",
    }
}

fn transition_name(t: Transition) -> &'static str {
    match t {
        Transition::Instant => "none",
        Transition::Dissolve => "fade",
        Transition::SmartAnimate => "smart",
        Transition::SlideLeft => "left",
        Transition::SlideRight => "right",
        Transition::SlideUp => "up",
        Transition::SlideDown => "down",
    }
}

// an easing as a CSS `linear()` curve: 40 samples of the curve, which also draws overshoots and springs
fn css_ease(e: Easing) -> String {
    let pts: Vec<String> = (0..=40).map(|k| format!("{:.4}", e.at(k as f32 / 40.0))).collect();
    format!("linear({})", pts.join(", "))
}

// `k` is the layer that has the interaction, `origin` the top-left of the screen it is on
fn action_json(app: &App, ctx: &egui::Context, k: usize, origin: egui::Pos2, ix: &Interaction) -> Option<Value> {
    if matches!(ix.trigger, Trigger::VideoTime | Trigger::VideoEnd) {
        return None; // the player shows posters, not playing video
    }
    let (kind, extra) = match ix.action {
        Action::Navigate => ("nav", json!({})),
        Action::Back => ("back", json!({})),
        Action::OpenLink => ("link", json!({ "url": ix.url })),
        Action::OpenOverlay | Action::SwapOverlay => (if ix.action == Action::OpenOverlay { "open" } else { "swap" }, json!({ "pos": ix.overlay_pos, "outside": ix.close_outside })),
        Action::CloseOverlay => ("close", json!({})),
        Action::SetVariable => {
            let name = ix.var.and_then(|id| app.variables.iter().flat_map(|c| &c.vars).find(|v| v.id == id)).map(|v| v.name.clone())?;
            ("set", json!({ "var": name, "expr": ix.value }))
        }
        Action::ChangeTo => {
            // the instance around the layer shows the picture of the linked variant until it is put back
            let inst = app.chain(k).into_iter().find(|&c| app.shapes[c].instance)?;
            let target = ix.link.filter(|&t| app.index_of(t).is_some())?;
            let (svg, _) = app.svg_for(ctx, &[target])?;
            let svg = scope_ids(&svg, &format!("v{target}_"));
            let r = app.world_rect(inst);
            ("variant", json!({ "svg": svg, "bx": r.min.x - origin.x, "by": r.min.y - origin.y, "bw": r.width(), "bh": r.height(), "hold": matches!(ix.trigger, Trigger::WhileHover | Trigger::WhilePress) }))
        }
        Action::Video | Action::SetMode => return None,
        Action::ScrollTo => {
            let r = app.world_rect(ix.link.and_then(|t| app.index_of(t))?);
            ("scroll", json!({ "sx": r.min.x - origin.x, "sy": r.min.y - origin.y }))
        }
    };
    let mut v = json!({ "on": trigger_name(ix.trigger), "do": kind, "to": ix.link, "tr": transition_name(ix.transition), "ms": (ix.duration * 1000.0).round(), "ease": css_ease(ix.easing), "key": ix.key, "delay": (ix.delay * 1000.0).round() });
    if !ix.condition.trim().is_empty() {
        v["if"] = json!(ix.condition);
    }
    if let (Value::Object(m), Value::Object(e)) = (&mut v, extra) {
        m.extend(e);
    }
    Some(v)
}

// every id of an SVG (and what points at it) gets `prefix`: several pictures share one page, and each numbers its clips and gradients from 1
fn scope_ids(svg: &str, prefix: &str) -> String {
    svg.replace("id=\"", &format!("id=\"{prefix}")).replace("url(#", &format!("url(#{prefix}")).replace("href=\"#", &format!("href=\"#{prefix}"))
}

impl App {
    pub fn prototype_html(&self, ctx: &egui::Context, start: Option<u64>) -> Option<String> {
        let frames: Vec<usize> = (0..self.shapes.len()).filter(|&i| self.shapes[i].kind == Kind::Frame && self.shapes[i].parent.is_none() && self.shapes[i].visible && !self.shapes[i].section).collect();
        if frames.is_empty() {
            return None;
        }
        let first = start.filter(|s| frames.iter().any(|&i| self.shapes[i].id == *s)).unwrap_or(self.shapes[frames[0]].id);
        let mut screens = String::new();
        let mut data = serde_json::Map::new();
        for &fi in &frames {
            let f = &self.shapes[fi];
            let range = self.present_scroll_range(fi);
            let scrolls = range != egui::Vec2::ZERO;
            let (svg, area) = if scrolls { self.svg_scrollable(ctx, f.id, true)? } else { self.svg_for_ids(ctx, &[f.id], true)? };
            let svg = scope_ids(&svg, &format!("s{}_", f.id));
            // children that stay in place while the frame scrolls are a layer of their own on top of the scrolling one
            let fixed_layer = if scrolls { self.svg_fixed_layer(ctx, f.id, true).map(|l| scope_ids(&l, &format!("s{}f_", f.id))) } else { None };
            let frame_size = self.world_rect(fi).size();
            let origin = self.world_rect(fi).min;
            let mut spots = vec![];
            for k in (0..self.shapes.len()).filter(|&k| k != fi && self.is_inside(self.shapes[k].id, f.id) && self.shapes[k].visible) {
                let ixs: Vec<Value> = self.shapes[k].interactions().iter().filter_map(|ix| action_json(self, ctx, k, origin, ix)).collect();
                if ixs.is_empty() {
                    continue;
                }
                let r: Rect = self.world_rect(k);
                // fixed itself, or inside something that is: it does not move with the scrolling content
                let fixed = self.chain(k).into_iter().take_while(|&c| c != fi).any(|c| self.shapes[c].fixed);
                spots.push(json!({ "x": r.min.x - origin.x, "y": r.min.y - origin.y, "w": r.width(), "h": r.height(), "ix": ixs, "fx": fixed }));
            }
            let (w, h) = (frame_size.x.ceil(), frame_size.y.ceil());
            let fx = fixed_layer.map(|l| format!("<div class=\"fx\">{l}</div>")).unwrap_or_default();
            let inner = if scrolls { format!("<div class=\"sc\" style=\"width:{}px;height:{}px\">{}</div>{fx}", area.width().ceil(), area.height().ceil(), svg) } else { svg };
            screens.push_str(&format!("<div class=\"screen\" id=\"s{}\" style=\"width:{w}px;height:{h}px\">{inner}</div>\n", f.id));
            data.insert(f.id.to_string(), json!({ "name": f.name, "w": w, "h": h, "sr": [range.x, range.y], "spots": spots }));
        }
        let page = self.pages.get(self.cur);
        let bg = page.map_or(egui::Color32::BLACK, |p| p.proto_bg);
        // the variables as they are now (first mode), found by name or "Collection / name" like in present mode
        let mut vars = serde_json::Map::new();
        for c in self.variables.iter().filter(|c| c.extends.is_none()) {
            for v in &c.vars {
                let val = match crate::variables::value(&self.variables, c, v, 0) {
                    Some(VarValue::Number(n)) => json!(n),
                    Some(VarValue::Bool(b)) => json!(b),
                    Some(VarValue::Text(t)) => json!(t),
                    Some(VarValue::Color(col)) => json!(format!("#{:02x}{:02x}{:02x}", col.r(), col.g(), col.b())),
                    _ => continue,
                };
                vars.insert(v.name.clone(), val.clone());
                vars.insert(format!("{} / {}", c.name, v.name), val.clone());
                vars.insert(format!("{}/{}", c.name, v.name), val);
            }
        }
        let doc = json!({ "start": first, "frames": data, "vars": vars, "bg": format!("#{:02x}{:02x}{:02x}", bg.r(), bg.g(), bg.b()) });
        let title = self.file.file_stem().map_or("Prototype".to_owned(), |n| n.to_string_lossy().into_owned());
        // the data goes inside a script: a closing tag in a layer name must not end it
        let data_js = serde_json::to_string(&doc).ok()?.replace("</", "<\\/");
        Some(PLAYER.replace("{{TITLE}}", &title.replace('<', "&lt;")).replace("{{SCREENS}}", &screens).replace("{{DATA}}", &data_js))
    }

    // "Open in browser": the prototype is written next to the temp files and opened with the system's browser
    pub fn open_prototype_in_browser(&mut self, ctx: &egui::Context) {
        let start = self.present_target();
        let Some(html) = self.prototype_html(ctx, start) else {
            self.status = "there is no frame to present".into();
            return;
        };
        let name = self.file.file_stem().map_or("prototype".to_owned(), |n| n.to_string_lossy().into_owned());
        let path = std::env::temp_dir().join(format!("drui-{name}.html"));
        match std::fs::write(&path, html) {
            Ok(()) => {
                ctx.open_url(egui::OpenUrl::new_tab(format!("file:///{}", path.to_string_lossy().replace('\\', "/"))));
                self.status = format!("opened {} in the browser", path.display());
            }
            Err(e) => self.status = format!("could not write the prototype: {e}"),
        }
    }

    pub fn export_prototype_html(&mut self, ctx: &egui::Context) {
        let start = self.present_target();
        let Some(html) = self.prototype_html(ctx, start) else {
            self.status = "there is no frame to present".into();
            return;
        };
        let Some(path) = rfd::FileDialog::new().add_filter("web page", &["html"]).set_file_name("prototype.html").save_file() else { return };
        self.status = match std::fs::write(&path, html) {
            Ok(()) => format!("exported {}", path.display()),
            Err(e) => format!("export failed: {e}"),
        };
    }
}

const PLAYER: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{{TITLE}}</title>
<style>
html,body{margin:0;height:100%;overflow:hidden;font-family:Inter,system-ui,sans-serif}
#stage{position:fixed;inset:0;display:flex;align-items:center;justify-content:center}
#view{position:relative;overflow:hidden;transform-origin:center center;box-shadow:0 8px 40px rgba(0,0,0,.5)}
.screen{position:absolute;left:0;top:0;display:none}
.screen svg{display:block}
.screen{overflow:hidden}
.sc{position:relative;will-change:transform}
.fx{position:absolute;left:0;top:0;pointer-events:none}.fx svg{display:block}
.screen.on{display:block}
.spot{position:absolute;cursor:pointer}
.variant{position:absolute;pointer-events:none;overflow:hidden}
.variant svg{display:block}
#shade{position:absolute;inset:0;background:rgba(0,0,0,.35);display:none}
.hint{position:fixed;left:12px;bottom:10px;color:#fff8;font-size:11px}
</style></head><body>
<div id="stage"><div id="view"></div></div>
<div class="hint">Click the linked layers. Esc: back to start.</div>
<div id="store" style="display:none">
{{SCREENS}}</div>
<script>
const D = {{DATA}};
document.getElementById('stage').style.background = D.bg;
const view = document.getElementById('view'), store = document.getElementById('store');
let cur = null, stack = [], overlays = [], timers = [];
const V = Object.assign({}, D.vars);
// expressions (expr.rs): numbers, strings, true/false, names, + - * / %, comparisons, && || !, parentheses
function ev(src) {
  const t = []; const re = /\s*(?:(\d*\.?\d+)|"([^"]*)"|'([^']*)'|`([^`]*)`|([A-Za-z_][\w.]*)|(==|!=|<=|>=|&&|\|\||[-+*\/%<>!()]))/y;
  let m; re.lastIndex = 0;
  while (re.lastIndex < src.length && (m = re.exec(src))) {
    if (m[1] !== undefined) t.push(['n', parseFloat(m[1])]); else if (m[2] !== undefined || m[3] !== undefined) t.push(['s', m[2] ?? m[3]]);
    else if (m[4] !== undefined || m[5] !== undefined) t.push(['v', m[4] ?? m[5]]); else t.push(['o', m[6]]);
  }
  if (re.lastIndex < src.replace(/\s+$/, '').length) throw new Error('not understood');
  let at = 0; const peek = () => t[at], eat = o => peek() && peek()[0] === 'o' && peek()[1] === o && ++at;
  const num = v => { if (typeof v === 'string') { const n = parseFloat(v); if (isNaN(n)) throw 0; return n; } return +v; };
  const truthy = v => typeof v === 'string' ? v !== '' : !!v;
  const txt = v => String(v);
  const or = () => { let l = and(); while (eat('||')) { const r = and(); l = truthy(l) || truthy(r); } return l; };
  const and = () => { let l = cmp(); while (eat('&&')) { const r = cmp(); l = truthy(l) && truthy(r); } return l; };
  const cmp = () => { const l = sum(); for (const o of ['==', '!=', '<=', '>=', '<', '>']) if (eat(o)) { const r = sum();
    const strs = typeof l === 'string' && typeof r === 'string', bools = typeof l === 'boolean' && typeof r === 'boolean';
    const a = strs || bools ? l : num(l), b = strs || bools ? r : num(r);
    return { '==': a === b, '!=': a !== b, '<=': a <= b, '>=': a >= b, '<': a < b, '>': a > b }[o]; } return l; };
  const sum = () => { let l = prod(); for (;;) { if (eat('+')) { const r = prod(); l = (typeof l === 'string' || typeof r === 'string') ? txt(l) + txt(r) : num(l) + num(r); } else if (eat('-')) l = num(l) - num(prod()); else return l; } };
  const prod = () => { let l = unary(); for (;;) { const o = ['*', '/', '%'].find(eat); if (!o) return l; const b = num(unary()); if (o !== '*' && b === 0) throw 0; l = o === '*' ? num(l) * b : o === '/' ? num(l) / b : num(l) % b; } };
  const unary = () => eat('!') ? !truthy(unary()) : eat('-') ? -num(unary()) : atom();
  const atom = () => { const k = t[at++]; if (!k) throw 0; if (k[0] === 'n' || k[0] === 's') return k[1];
    if (k[0] === 'v') { if (k[1] === 'true') return true; if (k[1] === 'false') return false; if (!(k[1] in V)) throw 0; return V[k[1]]; }
    if (k[1] === '(') { const v = or(); if (!eat(')')) throw 0; return v; } throw 0; };
  const v = or(); if (at < t.length) throw 0; return v;
}
const holds = ix => { if (!ix.if) return true; try { const v = ev(ix.if); return typeof v === 'string' ? v !== '' : !!v; } catch (e) { return false; } };
const el = id => store.querySelector('#s' + id) || view.querySelector('#s' + id);
function fit() {
  const f = D.frames[cur]; if (!f) return;
  const k = Math.min(innerWidth / f.w, innerHeight / f.h, 1.5);
  view.style.transform = 'scale(' + k + ')';
}
addEventListener('resize', fit);
function spots(id, host, dx, dy) {
  const f = D.frames[id];
  f.spots.forEach(s => {
    const d = document.createElement('div'); d.className = 'spot';
    Object.assign(d.style, { left: (s.x + dx) + 'px', top: (s.y + dy) + 'px', width: s.w + 'px', height: s.h + 'px' });
    s.ix.forEach(ix => {
      if (ix.on === 'click') d.addEventListener('click', e => { e.stopPropagation(); run(ix, id, host, dx, dy); });
      if (ix.on === 'down') d.addEventListener('mousedown', e => { e.stopPropagation(); run(ix, id, host, dx, dy); });
      if (ix.on === 'enter') d.addEventListener('mouseenter', () => run(ix, id, host, dx, dy));
      if (ix.on === 'leave') d.addEventListener('mouseleave', () => run(ix, id, host, dx, dy));
      // While hovering / pressing: the other variant goes away again when the pointer leaves or lets go
      if (ix.do === 'variant' && ix.hold) {
        d.addEventListener(ix.on === 'down' ? 'mouseup' : 'mouseleave', () => unvariant(host, ix));
        if (ix.on === 'down') d.addEventListener('mouseleave', () => unvariant(host, ix));
      }
    });
    (s.fx ? host : (host.querySelector(':scope > .sc') || host)).appendChild(d);
  });
}
function arm(id) {
  timers.forEach(clearTimeout); timers = [];
  D.frames[id].spots.forEach(s => s.ix.forEach(ix => { if (ix.on === 'delay') timers.push(setTimeout(() => run(ix, id), ix.delay)); }));
}
function show(id, tr, ms, ease) {
  const next = el(id), old = cur ? el(cur) : null;
  next.querySelectorAll('.spot').forEach(n => n.remove());
  view.style.width = D.frames[id].w + 'px'; view.style.height = D.frames[id].h + 'px';
  view.appendChild(next); next.classList.add('on');
  spots(id, next, 0, 0);
  if (old && old !== next) {
    const from = { left: 'translateX(100%)', right: 'translateX(-100%)', up: 'translateY(100%)', down: 'translateY(-100%)' }[tr];
    const to = { left: 'translateX(-100%)', right: 'translateX(100%)', up: 'translateY(-100%)', down: 'translateY(100%)' }[tr];
    const opts = { duration: ms || 300, easing: ease || 'ease-out', fill: 'forwards' };
    if (tr === 'smart') smart(next, old, ms, ease);
    else if (tr === 'fade') { next.animate([{ opacity: 0 }, { opacity: 1 }], opts); old.animate([{ opacity: 1 }, { opacity: 0 }], opts); }
    else if (from) { next.animate([{ transform: from }, { transform: 'none' }], opts); old.animate([{ transform: 'none' }, { transform: to }], opts); }
    setTimeout(() => { old.classList.remove('on'); old.querySelectorAll('[style*="visibility"]').forEach(n => n.style.visibility = ''); store.appendChild(old); }, (tr === 'none' ? 0 : (ms || 300)) + 20);
  }
  cur = id; scroll = [0, 0]; fit(); arm(id);
}
// Scroll to / the wheel: the content of a frame that is bigger than the frame moves behind its edges
let scroll = [0, 0];
function scrollTo(x, y, ms) {
  const f = D.frames[cur], sc = el(cur) && el(cur).querySelector(':scope > .sc'); if (!sc) return;
  scroll = [Math.min(Math.max(x, 0), f.sr[0]), Math.min(Math.max(y, 0), f.sr[1])];
  sc.style.transition = ms ? 'transform ' + ms + 'ms ease-out' : 'none';
  sc.style.transform = 'translate(' + -scroll[0] + 'px,' + -scroll[1] + 'px)';
}
addEventListener('wheel', e => { if (cur && !overlays.length) { e.preventDefault(); scrollTo(scroll[0] + e.deltaX, scroll[1] + e.deltaY, 0); } }, { passive: false });
// Smart Animate: layers with the same name on both screens move and scale from where they were, the others fade
function smart(next, old, ms, ease) {
  const opts = { duration: ms || 300, easing: ease || 'ease-out', fill: 'both' };
  const name = n => n.id.replace(/^s\d+_/, '');
  const inDefs = n => !!n.closest('defs');
  const oldBy = new Map(); old.querySelectorAll('svg [id]').forEach(n => { if (!inDefs(n)) oldBy.set(name(n), n); });
  const jobs = [];
  const walk = n => {
    for (const c of n.children) {
      if (c.tagName === 'defs') continue;
      if (c.id && oldBy.has(name(c))) { jobs.push(['move', c, oldBy.get(name(c))]); continue; }
      if (c.id && !c.querySelector('[id]')) { jobs.push(['fade', c]); continue; }
      if (c.tagName === 'g') walk(c); else jobs.push(['fade', c]);
    }
  };
  const svg = next.querySelector('svg'); if (!svg) return;
  walk(svg);
  // measure before anything moves
  const rects = jobs.map(j => j[0] === 'move' ? [j[1].getBoundingClientRect(), j[2].getBoundingClientRect()] : null);
  const s = view.getBoundingClientRect().width / (view.offsetWidth || 1) || 1;
  jobs.forEach((j, k) => {
    if (j[0] === 'fade') { j[1].animate([{ opacity: 0 }, { opacity: 1 }], opts); return; }
    const [to, from] = rects[k];
    const sx = to.width ? from.width / to.width : 1, sy = to.height ? from.height / to.height : 1;
    j[1].style.transformBox = 'fill-box'; j[1].style.transformOrigin = '0 0';
    j[1].animate([{ transform: 'translate(' + (from.left - to.left) / s + 'px,' + (from.top - to.top) / s + 'px) scale(' + sx + ',' + sy + ')' }, { transform: 'none' }], opts);
    j[2].style.visibility = 'hidden';
  });
  old.animate([{ opacity: 1 }, { opacity: 0 }], opts);
}
function closeOverlay(all) {
  const o = overlays.pop(); if (!o) return;
  o.remove(); if (all) closeOverlay(true);
  document.getElementById('shade') && (document.getElementById('shade').style.display = overlays.length ? 'block' : 'none');
}
function openOverlay(ix) {
  const f = D.frames[ix.to]; if (!f) return;
  const host = document.createElement('div'); host.style.cssText = 'position:absolute;inset:0';
  let shade = document.getElementById('shade');
  if (!shade) { shade = document.createElement('div'); shade.id = 'shade'; view.appendChild(shade); }
  shade.style.display = 'block';
  const sc = el(ix.to).cloneNode(true); sc.id = ''; sc.classList.add('on'); sc.style.position = 'absolute';
  const cw = D.frames[cur].w, ch = D.frames[cur].h, p = ix.pos | 0;
  const x = [1, 4].includes(p) ? 0 : [3, 6].includes(p) ? cw - f.w : (cw - f.w) / 2;
  const y = [1, 2, 3].includes(p) ? 0 : [4, 5, 6].includes(p) ? ch - f.h : (ch - f.h) / 2;
  Object.assign(sc.style, { left: x + 'px', top: y + 'px' });
  host.appendChild(sc); spots(ix.to, host, x, y);
  if (ix.outside) host.addEventListener('click', () => closeOverlay(false));
  view.appendChild(host); overlays.push(host);
}
// Change to: a picture of the other variant is laid over the instance (a second Change to on it replaces the first)
function variant(host, ix, dx, dy) {
  unvariant(host, ix);
  const v = document.createElement('div'); v.className = 'variant'; v.dataset.at = ix.bx + ',' + ix.by; v.innerHTML = ix.svg;
  Object.assign(v.style, { left: (ix.bx + dx) + 'px', top: (ix.by + dy) + 'px', width: ix.bw + 'px', height: ix.bh + 'px' });
  host.insertBefore(v, host.firstChild && host.firstChild.nextSibling);
}
function unvariant(host, ix) { host.querySelectorAll('.variant').forEach(n => { if (n.dataset.at === ix.bx + ',' + ix.by) n.remove(); }); }
function run(ix, from, host, dx, dy) {
  if (!holds(ix)) return;
  switch (ix.do) {
    case 'variant': variant(host || el(from), ix, dx || 0, dy || 0); break;
    case 'set': { const old = V[ix.var]; let v; try { v = ev(ix.expr); } catch (e) { v = ix.expr; }
      V[ix.var] = typeof old === 'number' ? (parseFloat(v) || 0) : typeof old === 'boolean' ? (v === true || v === 'true') : String(v); break; }
    case 'nav': if (D.frames[ix.to]) { stack.push(cur); closeOverlay(true); show(ix.to, ix.tr, ix.ms, ix.ease); } break;
    case 'back': { const b = stack.pop(); if (b) { closeOverlay(true); show(b, ix.tr, ix.ms, ix.ease); } break; }
    case 'link': if (ix.url) open(ix.url, '_blank'); break;
    case 'open': openOverlay(ix); break;
    case 'swap': closeOverlay(false); openOverlay(ix); break;
    case 'close': closeOverlay(false); break;
    case 'scroll': scrollTo(ix.sx, ix.sy, ix.ms || 300); break;
  }
}
addEventListener('keydown', e => {
  if (e.key === 'Escape') { stack = []; closeOverlay(true); show(D.start, 'none', 0); return; }
  D.frames[cur].spots.forEach(s => s.ix.forEach(ix => { if (ix.on === 'key' && ix.key.toLowerCase() === e.key.toLowerCase()) run(ix, cur); }));
});
show(D.start, 'none', 0);
</script></body></html>
"##;
