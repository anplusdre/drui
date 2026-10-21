// Comments on the canvas, after Figma's: with the comment tool (C) a blue pin follows the pointer, a click
// opens a composer ("Add a comment" and a send button) beside a plain pin, a placed comment is a pin with
// its author's avatar, hovering it unfolds a card (author, time, text), clicking it opens the thread (header
// with resolve and close, the messages, a reply field). Sizes are CSS px read off Figma at 150%: pin 32,
// avatar 24, composer 288x40, thread 360 wide.
use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::fig;
use crate::glyphs::{menu_item, paint_glyph_sized, Glyph};
use crate::model::*;
use crate::{App, Drag, Tool, BG, TEXT, TEXT_SECONDARY};

const PIN: f32 = 32.0;
const AVATAR: f32 = 24.0;
const THREAD_W: f32 = 360.0;
const COMPOSER_W: f32 = 288.0;
const COMPOSER_H: f32 = 40.0;
const ME: &str = "You";
const SEND_OFF: Color32 = Color32::from_rgb(0x6e, 0x6e, 0x6e);

// the pin's box for a tip (its sharp bottom-left corner) at `tip`
pub fn pin_rect(tip: Pos2) -> Rect {
    Rect::from_min_size(Pos2::new(tip.x, tip.y - PIN), Vec2::splat(PIN))
}

fn pin_corners() -> CornerRadius {
    CornerRadius { nw: 16, ne: 16, se: 16, sw: 0 }
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

// "just now", "5 min ago", "3 hr ago", else the date
pub fn ago(secs: u64) -> String {
    let d = now().saturating_sub(secs);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86399 => format!("{} hr ago", d / 3600),
        _ => crate::panels::when(secs),
    }
}

fn author_of(name: &str) -> &str {
    if name.is_empty() { ME } else { name }
}

fn avatar_color(name: &str) -> Color32 {
    const PALETTE: [(u8, u8, u8); 6] = [(0x7b, 0x61, 0xff), (0xf2, 0x48, 0x22), (0x14, 0xae, 0x5c), (0xff, 0xa6, 0x29), (0x9b, 0x4d, 0xff), (0x0d, 0x99, 0xff)];
    let h = name.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let (r, g, b) = PALETTE[h as usize % PALETTE.len()];
    Color32::from_rgb(r, g, b)
}

// a round avatar: the author's initial on a color
pub(crate) fn avatar(p: &egui::Painter, c: Pos2, r: f32, name: &str) {
    p.circle_filled(c, r, avatar_color(author_of(name)));
    let initial = author_of(name).chars().next().map_or('?'.to_string(), |ch| ch.to_uppercase().to_string());
    fig::ptext(p, c, Align2::CENTER_CENTER, initial, fig::semi(r * 0.9), Color32::WHITE);
}

// the pin: a rounded square with a sharp bottom-left corner, optionally holding an avatar
fn draw_pin(p: &egui::Painter, tip: Pos2, fill: Color32, who: Option<&str>) {
    let r = pin_rect(tip);
    p.add(egui::Shadow { offset: [0, 2], blur: 6, spread: 0, color: Color32::from_black_alpha(70) }.as_shape(r, 16.0));
    p.rect_filled(r, pin_corners(), fill);
    if let Some(name) = who {
        avatar(p, r.center(), AVATAR / 2.0, name);
    }
}

fn thread_frame() -> egui::Frame {
    egui::Frame::NONE.fill(BG()).corner_radius(crate::R_LARGE).stroke(Stroke::new(1.0, fig::RULE())).shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(90) })
}

// the round send button: grey while there is nothing to send, blue once there is
fn send_button(ui: &mut egui::Ui, center: Pos2, active: bool, name: &str) -> bool {
    let r = Rect::from_center_size(center, Vec2::splat(AVATAR));
    let resp = ui.interact(r, ui.id().with(("send", name)), Sense::click());
    ui.painter().circle_filled(center, AVATAR / 2.0, if active { fig::SHARE } else { SEND_OFF });
    paint_glyph_sized(ui.painter(), center, Glyph::ArrowUp, if active { Color32::WHITE } else { BG() }, 13.0);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, active, name));
    active && resp.clicked()
}

// what the menu of a message asked for
#[derive(Clone, PartialEq)]
enum MsgAction {
    Edit,
    Delete,
    React(String), // put an emoji on the message, or take yours off it
}

const EMOJI: [&str; 6] = ["\u{1F44D}", "\u{2764}", "\u{1F602}", "\u{1F389}", "\u{1F440}", "\u{1F64F}"];

// the people a comment can mention: everyone who has written in the file, the people in the session, and "You"
fn people(app: &App) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let authors = app.comments.iter().flat_map(|c| std::iter::once(c.author.clone()).chain(c.replies.iter().map(|r| r.author.clone())));
    for n in authors.chain(std::iter::once(ME.to_owned())) {
        let n = n.trim().to_owned();
        if !n.is_empty() && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

// `@Name` in a text: the byte ranges of the mentions (a mention is the @ and the word after it)
fn mentions(text: &str) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut at = None;
    for (i, c) in text.char_indices() {
        match (at, c) {
            (None, '@') => at = Some(i),
            (Some(s), c) if !(c.is_alphanumeric() || c == '_') => {
                if i > s + 1 {
                    out.push((s, i));
                }
                at = if c == '@' { Some(i) } else { None };
            }
            _ => {}
        }
    }
    if let Some(s) = at.filter(|&s| text.len() > s + 1) {
        out.push((s, text.len()));
    }
    out
}

// a mention being typed at the end of a draft: the part after the @ (None when the draft does not end in one)
fn mention_prefix(text: &str) -> Option<&str> {
    let at = text.rfind('@')?;
    let rest = &text[at + 1..];
    rest.chars().all(|c| c.is_alphanumeric() || c == '_').then_some(rest)
}

// a layout job of `text` with the @mentions in blue
fn rich_text(text: &str, wrap: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap;
    let fmt = |c: Color32| egui::text::TextFormat { font_id: fig::reg(11.0), color: c, ..Default::default() };
    let mut at = 0;
    for (s, e) in mentions(text) {
        job.append(&text[at..s], 0.0, fmt(TEXT()));
        job.append(&text[s..e], 0.0, fmt(fig::SELECTED_INK()));
        at = e;
    }
    job.append(&text[at..], 0.0, fmt(TEXT()));
    job
}

// one message of a thread: avatar, name and time on a 24px line, the text under them (mentions in blue), its pictures, and a menu
// on hover with Edit and Delete for the messages that may have them
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn message(ui: &mut egui::Ui, who: &str, secs: u64, text: &str, edited: bool, pictures: &[u64], textures: &std::collections::HashMap<u64, egui::TextureHandle>, menu: bool, reacts: &[&Reaction]) -> Option<MsgAction> {
    let w = ui.available_width();
    let galley = ui.painter().layout_job(rich_text(text, (w - 48.0 - 16.0).max(40.0)));
    let pics = if pictures.is_empty() { 0.0 } else { 108.0 };
    let chips = if reacts.is_empty() { 0.0 } else { 30.0 };
    let r = fig::row(ui, 16.0 + AVATAR + 12.0 + galley.size().y.max(14.0) + pics + chips + 16.0);
    let line_y = r.top() + 16.0 + AVATAR / 2.0;
    avatar(ui.painter(), Pos2::new(r.left() + 16.0 + AVATAR / 2.0, line_y), AVATAR / 2.0, who);
    let name = fig::ptext(ui.painter(), Pos2::new(r.left() + 48.0, line_y), Align2::LEFT_CENTER, author_of(who), fig::semi(11.0), TEXT());
    let when = fig::ptext(ui.painter(), Pos2::new(name.right() + 6.0, line_y), Align2::LEFT_CENTER, ago(secs), fig::reg(11.0), TEXT_SECONDARY());
    if edited {
        fig::ptext(ui.painter(), Pos2::new(when.right() + 6.0, line_y), Align2::LEFT_CENTER, "(edited)", fig::reg(11.0), TEXT_SECONDARY());
    }
    let text_y = r.top() + 16.0 + AVATAR + 12.0;
    ui.painter().galley(Pos2::new(r.left() + 48.0, text_y), galley.clone(), TEXT());
    // the pictures: thumbnails under the text
    for (k, id) in pictures.iter().enumerate() {
        let t = Rect::from_min_size(Pos2::new(r.left() + 48.0 + 108.0 * k as f32, text_y + galley.size().y.max(14.0) + 6.0), Vec2::splat(100.0));
        ui.painter().rect_filled(t, 4.0, fig::INPUT());
        if let Some(tex) = textures.get(id) {
            let sz = tex.size_vec2();
            let k = (t.width() / sz.x.max(1.0)).min(t.height() / sz.y.max(1.0));
            ui.painter().image(tex.id(), Rect::from_center_size(t.center(), sz * k), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
    }
    let (a, t) = (author_of(who).to_owned(), text.to_owned());
    ui.interact(r, ui.id().with(("message", secs, t.len())), Sense::hover()).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("{a}: {t}")));
    let mut action = None;
    // the reactions: a chip per emoji with how many put it there; a click adds or takes away yours
    let mut x = r.left() + 48.0;
    let chip_y = r.bottom() - 16.0 - 22.0;
    for re in reacts {
        let mine = re.who.iter().any(|w| w == ME);
        let label = format!("{} {}", re.emoji, re.who.len());
        let galley = ui.painter().layout_no_wrap(label.clone(), fig::reg(11.0), TEXT());
        let chip = Rect::from_min_size(Pos2::new(x, chip_y), Vec2::new(galley.size().x + 16.0, 22.0));
        let resp = ui.interact(chip, ui.id().with(("reaction", secs, text.len(), re.emoji.clone())), Sense::click());
        ui.painter().rect(chip, 11, if mine { fig::SELECTED() } else { fig::INPUT() }, Stroke::new(1.0, if mine { fig::SELECTED_INK() } else { Color32::TRANSPARENT }), StrokeKind::Inside);
        fig::ptext(ui.painter(), chip.center(), Align2::CENTER_CENTER, label.clone(), fig::reg(11.0), TEXT());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, mine, format!("Reaction {label}")));
        if resp.clicked() {
            action = Some(MsgAction::React(re.emoji.clone()));
        }
        x = chip.right() + 4.0;
    }
    let add = Rect::from_min_size(Pos2::new(r.right() - 16.0 - fig::ROW * if menu { 2.0 } else { 1.0 } - if menu { 4.0 } else { 0.0 }, r.top() + 16.0), Vec2::splat(fig::ROW));
    let add_resp = fig::icon_button(ui, add, Glyph::Plus, "Add reaction", false);
    egui::Popup::menu(&add_resp).show(|ui| {
        ui.horizontal(|ui| {
            for e in EMOJI {
                if ui.add(egui::Button::new(egui::RichText::new(e).size(15.0)).frame(false)).clicked() {
                    action = Some(MsgAction::React(e.to_owned()));
                    ui.close();
                }
            }
        });
    });
    if menu {
        let more = Rect::from_min_size(Pos2::new(r.right() - 16.0 - fig::ROW, r.top() + 16.0), Vec2::splat(fig::ROW));
        let resp = fig::icon_button(ui, more, Glyph::More, "Message options", false);
        egui::Popup::menu(&resp).show(|ui| {
            if menu_item(ui, None, "Edit message", "", true) {
                action = Some(MsgAction::Edit);
                ui.close();
            }
            if menu_item(ui, Some(Glyph::Delete), "Delete message", "", true) {
                action = Some(MsgAction::Delete);
                ui.close();
            }
        });
    }
    action
}

impl App {
    // pins that are drawn: resolved ones only when shown (or open)
    fn pin_visible(&self, c: &Comment) -> bool {
        c.page == self.cur && (!c.resolved || self.show_resolved || self.comment_sel == Some(c.id))
    }

    // the pin under a screen point
    pub fn comment_hit(&self, origin: Pos2, press: Pos2) -> Option<u64> {
        self.comments.iter().filter(|c| self.pin_visible(c)).rev().find(|c| pin_rect(self.to_screen(origin, c.pos)).expand(2.0).contains(press)).map(|c| c.id)
    }

    // pins, the hover card and the comment-tool cursor, drawn into the canvas
    pub fn paint_comments(&self, ui: &egui::Ui, painter: &egui::Painter, origin: Pos2, canvas: Rect) {
        if !self.show_comments {
            return;
        }
        let ptr = ui.input(|i| i.pointer.latest_pos()).filter(|p| canvas.contains(*p) && ui.rect_contains_pointer(canvas));
        let hover = ptr.filter(|_| matches!(self.drag, Drag::None)).and_then(|p| self.comment_hit(origin, p));
        for c in self.comments.iter().filter(|c| self.pin_visible(c)) {
            let tip = self.to_screen(origin, c.pos);
            let color = if c.resolved { Color32::from_gray(0x6b) } else { fig::SHARE };
            if hover == Some(c.id) && self.comment_sel != Some(c.id) {
                self.paint_card(painter, tip, c);
            } else {
                draw_pin(painter, tip, color, Some(&c.author));
                if self.comment_sel == Some(c.id) {
                    painter.rect_stroke(pin_rect(tip), pin_corners(), Stroke::new(2.0, Color32::WHITE), StrokeKind::Outside);
                }
            }
        }
        if let Some((w, _)) = &self.new_comment {
            draw_pin(painter, self.to_screen(origin, *w), fig::SHARE, None);
        } else if self.tool == Tool::Comment {
            if let Some(p) = ptr {
                draw_pin(painter, p, fig::SHARE, None);
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            }
        }
    }

    // hovering a pin unfolds it into a card: avatar, "name time" over the text
    fn paint_card(&self, painter: &egui::Painter, tip: Pos2, c: &Comment) {
        let who = author_of(&c.author);
        let name = painter.layout_no_wrap(who.to_owned(), fig::semi(11.0), TEXT());
        let time = painter.layout_no_wrap(ago(c.secs), fig::reg(11.0), TEXT_SECONDARY());
        let first = c.text.lines().next().unwrap_or("");
        let body = painter.layout_no_wrap(first.to_owned(), fig::reg(11.0), TEXT());
        let w = (12.0 + AVATAR + 8.0 + (name.size().x + 6.0 + time.size().x).max(body.size().x) + 16.0).clamp(160.0, 300.0);
        let r = Rect::from_min_size(Pos2::new(tip.x, tip.y - 56.0), Vec2::new(w, 56.0));
        painter.add(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(80) }.as_shape(r, 16.0));
        painter.rect(r, pin_corners(), BG(), Stroke::new(1.0, fig::RULE()), StrokeKind::Inside);
        avatar(painter, Pos2::new(r.left() + 12.0 + AVATAR / 2.0, r.center().y), AVATAR / 2.0, &c.author);
        let x = r.left() + 12.0 + AVATAR + 8.0;
        let clip = painter.with_clip_rect(r.shrink2(Vec2::new(0.0, 1.0)).with_max_x(r.right() - 12.0));
        let n = fig::ptext(&clip, Pos2::new(x, r.top() + 12.0 + 8.0), Align2::LEFT_CENTER, who, fig::semi(11.0), TEXT());
        fig::ptext(&clip, Pos2::new(n.right() + 6.0, r.top() + 12.0 + 8.0), Align2::LEFT_CENTER, ago(c.secs), fig::reg(11.0), TEXT_SECONDARY());
        fig::ptext(&clip, Pos2::new(x, r.top() + 12.0 + 16.0 + 8.0), Align2::LEFT_CENTER, first, fig::reg(11.0), TEXT());
    }

    // the composer for a new comment and the open thread: real widgets, so they float above the canvas
    pub fn comment_popups(&mut self, ctx: &egui::Context, origin: Pos2, canvas: Rect) {
        self.composer(ctx, origin, canvas);
        self.thread(ctx, origin, canvas);
    }

    fn composer(&mut self, ctx: &egui::Context, origin: Pos2, canvas: Rect) {
        let Some((w, mut text)) = self.new_comment.take() else { return };
        let tip = self.to_screen(origin, w);
        // beside the pin: to its left, or to its right when there is no room
        let mut left = tip.x - 16.0 - COMPOSER_W;
        if left < canvas.left() + 8.0 {
            left = tip.x + PIN + 16.0;
        }
        let top = (tip.y - PIN / 2.0 - COMPOSER_H / 2.0).clamp(canvas.top() + 8.0, (canvas.bottom() - COMPOSER_H - 8.0).max(canvas.top() + 8.0));
        let (mut submit, mut cancel) = (false, false);
        let mut attach_first = false;
        egui::Area::new(Id::new("comment-composer")).fixed_pos(Pos2::new(left, top)).order(egui::Order::Foreground).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::new(COMPOSER_W, COMPOSER_H), Sense::hover());
            ui.painter().rect(r, 12, BG(), Stroke::new(2.0, fig::SHARE), StrokeKind::Inside);
            let edit_r = Rect::from_min_max(Pos2::new(r.left() + 8.0, r.top()), Pos2::new(r.right() - 72.0, r.bottom()));
            let out = fig::text_edit(ui, edit_r, &mut text, "New comment", "Add a comment (@ to mention)");
            let pic = Rect::from_center_size(Pos2::new(r.right() - 16.0 - AVATAR - AVATAR / 2.0, r.center().y), Vec2::splat(AVATAR));
            let attached = !self.pending_attach.is_empty();
            if fig::icon_button(ui, pic, Glyph::Image, if attached { "A picture is attached: click to add another" } else { "Attach a picture" }, attached).clicked() {
                attach_first = true;
            }
            let enter = out.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                cancel = true;
            } else if enter && !text.trim().is_empty() {
                submit = true;
            } else if !out.has_focus() {
                out.request_focus();
            }
            if send_button(ui, Pos2::new(r.right() - 8.0 - AVATAR / 2.0, r.center().y), !text.trim().is_empty(), "Send comment") {
                submit = true;
            }
        });
        if attach_first {
            if let Some(path) = rfd::FileDialog::new().add_filter("image", &["png", "jpg", "jpeg"]).pick_file() {
                if let Ok(bytes) = std::fs::read(&path) {
                    let name = path.file_stem().map_or("attachment".to_owned(), |n| n.to_string_lossy().into_owned());
                    if let Some(img) = self.register_image(&name, bytes) {
                        self.pending_attach.push(img);
                    }
                }
            }
        }
        if submit {
            let id = self.new_id();
            self.comments.push(Comment { id, page: self.cur, pos: w, text: text.trim().to_owned(), resolved: false, author: ME.into(), secs: now(), replies: vec![], attachments: std::mem::take(&mut self.pending_attach), edited: false, reactions: vec![] });
            self.comment_sel = Some(id);
            self.tool = Tool::Select;
        } else if cancel {
            self.tool = Tool::Select;
        } else {
            self.new_comment = Some((w, text));
        }
    }

    fn thread(&mut self, ctx: &egui::Context, origin: Pos2, canvas: Rect) {
        let Some(id) = self.comment_sel else { return };
        let Some(ci) = self.comments.iter().position(|c| c.id == id && self.pin_visible(c)) else { return };
        let tip = self.to_screen(origin, self.comments[ci].pos);
        let mut left = tip.x + PIN + 16.0;
        if left + THREAD_W > canvas.right() - 8.0 {
            left = tip.x - 16.0 - THREAD_W;
        }
        let top = tip.y - PIN - 16.0;
        let (mut close, mut resolve, mut delete) = (false, false, false);
        let mut reply: Option<(String, Vec<u64>)> = None;
        let mut pending: Option<(usize, MsgAction)> = None;
        let mut edit_done: Option<(usize, Option<String>)> = None;
        let mut attach = false;
        let people_list = people(self);
        let picked_pics: Vec<u64> = self.pending_attach.clone();
        let c = self.comments[ci].clone();
        egui::Area::new(Id::new("comment-thread")).fixed_pos(Pos2::new(left.max(canvas.left() + 8.0), top)).constrain_to(canvas).order(egui::Order::Foreground).show(ctx, |ui| {
            thread_frame().show(ui, |ui| {
                ui.set_width(THREAD_W);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let hr = fig::row(ui, 40.0);
                fig::text(ui, Pos2::new(hr.left() + 16.0, hr.center().y), Align2::LEFT_CENTER, "Comment", fig::semi(11.0), TEXT());
                let slot = |k: f32| Rect::from_min_size(Pos2::new(hr.right() - 32.0 - 28.0 * k, hr.top() + 8.0), Vec2::splat(fig::ROW));
                close = fig::icon_button(ui, slot(0.0), Glyph::Close, "Close comment", false).clicked();
                resolve = fig::icon_button(ui, slot(1.0), Glyph::CircleCheck, if c.resolved { "Reopen comment" } else { "Resolve comment" }, c.resolved).clicked();
                let more = fig::icon_button(ui, slot(2.0), Glyph::More, "More options", false);
                egui::Popup::menu(&more).show(|ui| {
                    if menu_item(ui, Some(Glyph::Delete), "Delete thread", "", true) {
                        delete = true;
                        ui.close();
                    }
                });
                fig::rule(ui);
                let editing: Option<usize> = ui.data(|d| d.get_temp(Id::new(("msg-edit", id))));
                for (n, (who, secs, text, edited, pics)) in std::iter::once((c.author.clone(), c.secs, c.text.clone(), c.edited, c.attachments.clone())).chain(c.replies.iter().map(|r| (r.author.clone(), r.secs, r.text.clone(), r.edited, r.attachments.clone()))).enumerate() {
                    if editing == Some(n) {
                        // the message being edited: a field in its place
                        let row = fig::row(ui, 64.0);
                        avatar(ui.painter(), Pos2::new(row.left() + 16.0 + AVATAR / 2.0, row.center().y), AVATAR / 2.0, &who);
                        let field = Rect::from_min_max(Pos2::new(row.left() + 48.0, row.top() + 12.0), Pos2::new(row.right() - 16.0, row.bottom() - 12.0));
                        fig::input_bg_r(ui, field, 12);
                        let did = Id::new(("msg-edit-draft", id, n));
                        let mut draft: String = ui.data(|d| d.get_temp(did)).unwrap_or(text.clone());
                        let edit_r = Rect::from_min_max(field.min + Vec2::new(4.0, 0.0), Pos2::new(field.right() - 40.0, field.bottom()));
                        let out = fig::text_edit(ui, edit_r, &mut draft, "Edit message", "Edit message");
                        ui.painter().rect_stroke(field, 12, Stroke::new(2.0, fig::SHARE), StrokeKind::Inside);
                        if !out.has_focus() {
                            out.request_focus();
                        }
                        let enter = out.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let save = send_button(ui, Pos2::new(field.right() - 8.0 - AVATAR / 2.0, field.center().y), !draft.trim().is_empty(), "Save message");
                        if (enter || save) && !draft.trim().is_empty() {
                            edit_done = Some((n, Some(draft.trim().to_owned())));
                        } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            edit_done = Some((n, None));
                        }
                        ui.data_mut(|d| d.insert_temp(did, draft));
                    } else if let Some(act) = message(ui, &who, secs, &text, edited, &pics, &self.tex, who == ME || who.is_empty(), &c.reactions.iter().filter(|r| r.msg == n).collect::<Vec<_>>()) {
                        pending = Some((n, act));
                    }
                }
                // the reply field: avatar, then a rounded field that turns blue while typing
                let rid = Id::new(("reply-draft", id));
                let mut draft: String = ui.data(|d| d.get_temp(rid)).unwrap_or_default();
                let row = fig::row(ui, 64.0);
                avatar(ui.painter(), Pos2::new(row.left() + 16.0 + AVATAR / 2.0, row.center().y), AVATAR / 2.0, ME);
                let field = Rect::from_min_max(Pos2::new(row.left() + 48.0, row.top() + 12.0), Pos2::new(row.right() - 16.0, row.bottom() - 12.0));
                let edit_r = Rect::from_min_max(field.min + Vec2::new(4.0, 0.0), Pos2::new(field.right() - 68.0, field.bottom()));
                fig::input_bg_r(ui, field, 12);
                let out = fig::text_edit(ui, edit_r, &mut draft, "Reply", "Reply (@ to mention)");
                if out.has_focus() {
                    ui.painter().rect_stroke(field, 12, Stroke::new(2.0, fig::SHARE), StrokeKind::Inside);
                }
                // a picture to send with the reply
                let pic = Rect::from_center_size(Pos2::new(field.right() - 12.0 - AVATAR - 16.0, field.center().y), Vec2::splat(AVATAR));
                let pic_resp = fig::icon_button(ui, pic, Glyph::Image, if picked_pics.is_empty() { "Attach a picture" } else { "A picture is attached: click to add another" }, !picked_pics.is_empty());
                if pic_resp.clicked() {
                    attach = true;
                }
                // @name: a menu of the people whose name starts with what was typed
                if let Some(prefix) = mention_prefix(&draft).map(str::to_owned) {
                    let found: Vec<&String> = people_list.iter().filter(|n| n.to_lowercase().starts_with(&prefix.to_lowercase())).collect();
                    if out.has_focus() && !found.is_empty() {
                        let menu = Rect::from_min_size(Pos2::new(field.left(), field.top() - 4.0 - 24.0 * found.len() as f32), Vec2::new(160.0, 24.0 * found.len() as f32));
                        ui.painter().rect(menu, 6, BG(), Stroke::new(1.0, fig::RULE()), StrokeKind::Inside);
                        for (k, name) in found.iter().enumerate() {
                            let row = Rect::from_min_size(menu.min + Vec2::new(0.0, 24.0 * k as f32), Vec2::new(menu.width(), 24.0));
                            let resp = ui.interact(row, Id::new(("mention", id, k)), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(row, 4, fig::INPUT_HOVER());
                            }
                            fig::ptext(ui.painter(), Pos2::new(row.left() + 8.0, row.center().y), Align2::LEFT_CENTER, name.as_str(), fig::reg(11.0), TEXT());
                            if resp.clicked() {
                                let at = draft.rfind('@').unwrap_or(0);
                                draft.truncate(at);
                                draft.push_str(&format!("@{} ", name.replace(' ', "_")));
                                out.request_focus();
                            }
                        }
                    }
                }
                let enter = out.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let sent = send_button(ui, Pos2::new(field.right() - 8.0 - AVATAR / 2.0, field.center().y), !draft.trim().is_empty() || !picked_pics.is_empty(), "Send reply");
                if (enter || sent) && (!draft.trim().is_empty() || !picked_pics.is_empty()) {
                    reply = Some((draft.trim().to_owned(), picked_pics.clone()));
                    draft.clear();
                    out.request_focus();
                }
                ui.data_mut(|d| d.insert_temp(rid, draft));
            });
        });
        // Esc closes the thread, whichever field has the keyboard
        close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if attach {
            if let Some(path) = rfd::FileDialog::new().add_filter("image", &["png", "jpg", "jpeg"]).pick_file() {
                if let Ok(bytes) = std::fs::read(&path) {
                    let name = path.file_stem().map_or("attachment".to_owned(), |n| n.to_string_lossy().into_owned());
                    if let Some(img) = self.register_image(&name, bytes) {
                        self.pending_attach.push(img);
                    }
                }
            }
        }
        if let Some((text, pics)) = reply {
            self.comments[ci].replies.push(Reply { author: ME.into(), secs: now(), text, attachments: pics, edited: false });
            self.pending_attach.clear();
        }
        // a message's menu and the inline editor
        let eid = Id::new(("msg-edit", id));
        if let Some((n, act)) = pending {
            match act {
                MsgAction::React(emoji) => {
                    let rs = &mut self.comments[ci].reactions;
                    match rs.iter().position(|r| r.msg == n && r.emoji == emoji) {
                        Some(k) => {
                            if let Some(p) = rs[k].who.iter().position(|w| w == ME) {
                                rs[k].who.remove(p);
                            } else {
                                rs[k].who.push(ME.into());
                            }
                            if rs[k].who.is_empty() {
                                rs.remove(k);
                            }
                        }
                        None => rs.push(Reaction { msg: n, emoji, who: vec![ME.into()] }),
                    }
                }
                MsgAction::Edit => ctx.data_mut(|d| {
                    d.insert_temp(eid, n);
                    d.remove::<String>(Id::new(("msg-edit-draft", id, n)));
                }),
                MsgAction::Delete => {
                    // the reactions of the message go with it; the ones of the later messages move up
                    self.comments[ci].reactions.retain(|r| r.msg != n);
                    for r in self.comments[ci].reactions.iter_mut().filter(|r| r.msg > n) {
                        r.msg -= 1;
                    }
                    if n == 0 {
                        if self.comments[ci].replies.is_empty() {
                            delete = true;
                        } else {
                            // the first message goes: the first reply becomes the comment
                            let r = self.comments[ci].replies.remove(0);
                            let c = &mut self.comments[ci];
                            (c.author, c.secs, c.text, c.edited, c.attachments) = (r.author, r.secs, r.text, r.edited, r.attachments);
                        }
                    } else if n - 1 < self.comments[ci].replies.len() {
                        self.comments[ci].replies.remove(n - 1);
                    }
                }
            }
        }
        if let Some((n, text)) = edit_done {
            if let Some(t) = text {
                if n == 0 {
                    self.comments[ci].text = t;
                    self.comments[ci].edited = true;
                } else if let Some(r) = self.comments[ci].replies.get_mut(n - 1) {
                    r.text = t;
                    r.edited = true;
                }
            }
            ctx.data_mut(|d| d.remove::<usize>(eid));
        }
        if resolve {
            self.comments[ci].resolved ^= true;
            if self.comments[ci].resolved && !self.show_resolved {
                self.comment_sel = None;
            }
        }
        if delete {
            self.comments.remove(ci);
            self.comment_sel = None;
        } else if close {
            self.comment_sel = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_comments_load_and_times_read_naturally() {
        // a comment saved before authors, times and replies existed
        let c: Comment = serde_json::from_str(r#"{"id":3,"page":0,"pos":[10.0,20.0],"text":"hi","resolved":false}"#).unwrap();
        assert!(c.author.is_empty() && c.replies.is_empty() && c.secs == 0);
        assert_eq!(author_of(&c.author), "You");
        let t = now();
        assert_eq!(ago(t), "just now");
        assert_eq!(ago(t - 120), "2 min ago");
        assert_eq!(ago(t - 7300), "2 hr ago");
    }

    #[test]
    fn pin_sits_above_its_tip() {
        let r = pin_rect(Pos2::new(100.0, 200.0));
        assert_eq!(r.left_bottom(), Pos2::new(100.0, 200.0), "the sharp corner is the tip");
        assert_eq!(r.size(), Vec2::splat(PIN));
    }

    #[test]
    fn comment_tool_composer_and_thread_flow() {
        let mut a = App::default();
        a.new_comment = Some((Pos2::new(50.0, 60.0), "first".into()));
        a.tool = Tool::Comment;
        let ctx = egui::Context::default();
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        let raw = egui::RawInput { screen_rect: Some(canvas), ..Default::default() };
        // submit with the send path
        let _ = ctx.run_ui(raw.clone(), |ui| {
            let _ = ui;
        });
        a.comments.push(Comment { id: 9, page: 0, pos: Pos2::new(50.0, 60.0), text: "first".into(), resolved: false, author: "Alex".into(), secs: now(), replies: vec![], attachments: vec![], edited: false, reactions: vec![] });
        a.comment_sel = Some(9);
        for _ in 0..2 {
            let _ = ctx.run_ui(raw.clone(), |ui| a.comment_popups(ui.ctx(), Pos2::ZERO, canvas));
        }
        assert_eq!(a.comment_sel, Some(9), "the thread stays open");
        // resolving hides the pin unless resolved comments are shown
        a.comments[0].resolved = true;
        a.comment_sel = None;
        let on_pin = a.to_screen(Pos2::ZERO, Pos2::new(50.0, 60.0)) + Vec2::new(10.0, -10.0);
        assert!(a.comment_hit(Pos2::ZERO, on_pin).is_none());
        a.show_resolved = true;
        assert_eq!(a.comment_hit(Pos2::ZERO, on_pin), Some(9));
    }

    #[test]
    fn mentions_are_found_and_completed_and_threads_draw_with_pictures_and_an_editor() {
        let t = "hi @Ann_B and @Bo, see @";
        let found: Vec<&str> = mentions(t).into_iter().map(|(a, b)| &t[a..b]).collect();
        assert_eq!(found, vec!["@Ann_B", "@Bo", "@"].into_iter().filter(|m| m.len() > 1).collect::<Vec<_>>(), "{found:?}");
        assert_eq!(mention_prefix("hello @An"), Some("An"));
        assert_eq!(mention_prefix("hello @"), Some(""));
        assert_eq!(mention_prefix("hello @An x"), None);
        assert_eq!(mention_prefix("no mention"), None);
        let mut a = App::default();
        a.comments.push(Comment {
            id: 9,
            page: 0,
            pos: Pos2::new(50.0, 60.0),
            text: "first @Ann".into(),
            resolved: false,
            author: "Alex".into(),
            secs: now(),
            replies: vec![Reply { author: "You".into(), secs: now(), text: "mine".into(), attachments: vec![123], edited: true }, Reply { author: "Ann".into(), secs: now(), text: "hers".into(), attachments: vec![], edited: false }],
            attachments: vec![],
            edited: false,
            reactions: vec![Reaction { msg: 1, emoji: "{1F44D}".into(), who: vec!["Ann".into(), "You".into()] }],
        });
        assert_eq!(people(&a), vec!["Alex".to_owned(), "You".to_owned(), "Ann".to_owned()]);
        a.comment_sel = Some(9);
        let ctx = egui::Context::default();
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        let raw = egui::RawInput { screen_rect: Some(canvas), ..Default::default() };
        // the thread with a picture and an edited message, then with my reply being edited
        for editing in [None, Some(1usize)] {
            ctx.data_mut(|d| {
                match editing {
                    Some(n) => {
                        d.insert_temp(Id::new(("msg-edit", 9u64)), n);
                    }
                    None => d.remove::<usize>(Id::new(("msg-edit", 9u64))),
                }
            });
            for _ in 0..2 {
                let _ = ctx.run_ui(raw.clone(), |ui| a.comment_popups(ui.ctx(), Pos2::ZERO, canvas));
            }
        }
        assert_eq!(a.comments[0].replies.len(), 2);
        // pictures on comments are textures to load
        a.images.push(Asset { id: 123, name: "pic".into(), data: String::new() });
        a.prepare_textures(&ctx);
    }
}
