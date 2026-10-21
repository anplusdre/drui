// Rich text: mixed styles inside one text layer (runs), case, decoration, lists, paragraph spacing and indent, truncation, super and
// subscript, weights of the variable Inter and installed system fonts. `display_chars` is the one place that decides what is shown;
// the egui layout job (canvas, fitting) and the SVG export both start from it.
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use egui::epaint::text::VariationCoords;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, FontFamily, FontId, Painter, Stroke};

use crate::model::*;

// the look of one character: the layer's own settings with the runs on top
#[derive(Clone, Debug, PartialEq)]
pub struct CharStyle {
    pub weight: u16,
    pub italic: bool,
    pub size: f32,
    pub color: Option<Color32>,
    pub underline: bool,
    pub strike: bool,
    pub family: String,
    pub url: Option<String>,
    pub script: u8,    // 0 normal, 1 superscript, 2 subscript
    pub small: bool,   // a lower-case letter shown as a small capital
    pub indent: bool,  // the invisible first-line indent
    pub spacer: bool,  // the empty line that makes paragraph spacing
    pub fade: u8,      // text animation: how much of the character shows (255 = all)
}

impl Shape {
    // the weight of the layer's own text: the Weight setting, else Semi Bold or Regular from the old Font style
    pub fn base_weight(&self) -> u16 {
        if self.weight != 0 { self.weight } else if self.bold { 600 } else { 400 }
    }

    // anything that needs more than one plain section
    pub fn rich(&self) -> bool {
        !self.runs.is_empty()
            || self.italic
            || self.underline
            || self.strike
            || self.case != 0
            || self.list != 0
            || self.para_spacing > 0.0
            || self.para_indent > 0.0
            || self.script != 0
            || !self.family.is_empty()
            || self.weight != 0
            || (self.text_t.is_some() && self.text_anim.unit != 0)
    }

    pub fn base_style(&self) -> CharStyle {
        CharStyle {
            weight: self.base_weight(),
            italic: self.italic,
            size: self.font_size,
            color: None,
            underline: self.underline,
            strike: self.strike,
            family: self.family.clone(),
            url: None,
            script: self.script,
            small: false,
            indent: false,
            spacer: false,
            fade: 255,
        }
    }

    // the style of the character at `i` as typed (the layer's own with its runs)
    pub fn char_style(&self, i: usize) -> CharStyle {
        self.style_at(&self.base_style(), i)
    }

    fn style_at(&self, base: &CharStyle, i: usize) -> CharStyle {
        let mut st = base.clone();
        for r in self.runs.iter().filter(|r| r.start <= i && i < r.end) {
            if let Some(w) = r.weight {
                st.weight = w;
            } else if let Some(b) = r.bold {
                st.weight = if b { 600 } else { 400 };
            }
            if let Some(v) = r.italic {
                st.italic = v;
            }
            if let Some(v) = r.size {
                st.size = v;
            }
            if r.color.is_some() {
                st.color = r.color;
            }
            if let Some(v) = r.underline {
                st.underline = v;
            }
            if let Some(v) = r.strike {
                st.strike = v;
            }
            if let Some(f) = &r.family {
                st.family = f.clone();
            }
            if r.url.is_some() {
                st.url = r.url.clone();
                st.underline = true; // a link is underlined
            }
        }
        st
    }

    // what the layer shows, one entry per character: case applied, list markers and indent added, empty lines for paragraph spacing
    pub fn display_chars(&self) -> Vec<(char, CharStyle)> {
        let base = self.base_style();
        let mut out: Vec<(char, CharStyle)> = vec![];
        let mut para_start = true;
        let mut number = 0;
        let chars: Vec<char> = self.text.chars().collect();
        let mut prev_alnum = false;
        for (i, &c) in chars.iter().enumerate() {
            if para_start {
                if self.para_indent > 0.0 {
                    out.push((' ', CharStyle { indent: true, ..base.clone() }));
                }
                match self.list {
                    1 => out.extend("\u{2022}\u{2002}".chars().map(|m| (m, base.clone()))),
                    2 => {
                        number += 1;
                        out.extend(format!("{number}.\u{2002}").chars().map(|m| (m, base.clone())));
                    }
                    _ => {}
                }
                para_start = false;
            }
            let mut st = self.style_at(&base, i);
            let shown = match (self.case, c) {
                (_, '\n') => c,
                (1, _) => c.to_uppercase().next().unwrap_or(c),
                (2, _) => c.to_lowercase().next().unwrap_or(c),
                (3, _) => {
                    if prev_alnum { c.to_lowercase().next().unwrap_or(c) } else { c.to_uppercase().next().unwrap_or(c) }
                }
                (4, _) if c.is_lowercase() => {
                    st.small = true;
                    c.to_uppercase().next().unwrap_or(c)
                }
                _ => c,
            };
            prev_alnum = c.is_alphanumeric() || c == '\'';
            out.push((shown, st));
            if c == '\n' {
                para_start = true;
                if (self.para_spacing > 0.0 || (self.list != 0 && self.list_spacing > 0.0)) && i + 1 < chars.len() {
                    out.push(('\n', CharStyle { spacer: true, ..base.clone() }));
                }
            }
        }
        // text animation: the units (characters, words, lines) appear one after the other as the playhead passes
        if let (Some(t), unit @ 1..=3) = (self.text_t, self.text_anim.unit) {
            let a = &self.text_anim;
            let (mut idx, mut in_word) = (0usize, false);
            for (c, st) in out.iter_mut().filter(|(_, st)| !st.spacer) {
                let n = match unit {
                    1 => {
                        let n = idx;
                        if *c != '\n' {
                            idx += 1;
                        }
                        n
                    }
                    2 => {
                        if c.is_whitespace() {
                            in_word = false;
                        } else if !in_word {
                            in_word = true;
                            idx += 1;
                        }
                        idx.saturating_sub(1)
                    }
                    _ => {
                        let n = idx;
                        if *c == '\n' {
                            idx += 1;
                        }
                        n
                    }
                };
                let since = t - a.start - n as f32 * a.per_unit;
                let shown = if a.kind == 1 { f32::from(since >= 0.0) } else { (since / a.duration.max(1.0)).clamp(0.0, 1.0) };
                st.fade = (shown * 255.0).round() as u8;
            }
        }
        out
    }
}

// system fonts the layer can use: every installed family, once (the database is shared with the SVG export)
static FAMILIES: std::sync::Mutex<Option<std::sync::Arc<Vec<String>>>> = std::sync::Mutex::new(None);

// the installed font families, sorted; rebuilt after `rescan_fonts_if_changed` found a change
pub fn system_families() -> std::sync::Arc<Vec<String>> {
    let mut g = FAMILIES.lock().unwrap();
    if g.is_none() {
        let db = crate::export::fonts();
        let mut v: Vec<String> = db.faces().filter_map(|f| f.families.first().map(|(n, _)| n.clone())).collect();
        v.sort_by_key(|n| n.to_lowercase());
        v.dedup();
        *g = Some(std::sync::Arc::new(v));
    }
    g.clone().unwrap()
}

// did the font folders change (a font was installed or removed)? then the font list is read again. Cheap when nothing changed.
pub fn rescan_fonts_if_changed() -> bool {
    let changed = crate::export::rescan_fonts_if_changed();
    if changed {
        *FAMILIES.lock().unwrap() = None;
    }
    changed
}

// the weights an installed family comes in (sorted, e.g. 400, 700); empty when it is not installed
pub fn family_weights(family: &str) -> Vec<u16> {
    let db = crate::export::fonts();
    let mut w: Vec<u16> = db.faces().filter(|f| f.families.iter().any(|(n, _)| n == family)).map(|f| f.weight.0).collect();
    w.sort_unstable();
    w.dedup();
    w
}

fn registered() -> &'static Mutex<HashSet<String>> {
    static REG: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashSet::new()))
}

fn face_key(family: &str, weight: u16) -> String {
    format!("sys:{family}:{weight}")
}

// the nearest installed weight of a family
fn nearest_weight(family: &str, weight: u16) -> Option<u16> {
    family_weights(family).into_iter().min_by_key(|w| (*w as i32 - weight as i32).abs())
}

// installs the system faces the text layers use into egui (it takes effect on the next frame); returns true when it added any
pub fn ensure_fonts(ctx: &egui::Context, shapes: &[Shape]) -> bool {
    let mut want: HashSet<(String, u16)> = HashSet::new();
    for s in shapes.iter().filter(|s| s.kind == Kind::Text) {
        for fam in std::iter::once(s.family.clone()).chain(s.runs.iter().filter_map(|r| r.family.clone())).filter(|f| !f.is_empty()) {
            let weights: HashSet<u16> = std::iter::once(s.base_weight()).chain(s.runs.iter().filter_map(|r| r.weight.or(r.bold.map(|b| if b { 600 } else { 400 })))).collect();
            for w in weights {
                if let Some(n) = nearest_weight(&fam, w) {
                    want.insert((fam.clone(), n));
                }
            }
        }
    }
    let mut added = false;
    for (fam, w) in want {
        let key = face_key(&fam, w);
        let mut reg = registered().lock().unwrap();
        if reg.contains(&key) {
            continue;
        }
        let db = crate::export::fonts();
        let id = db.faces().find(|f| f.families.iter().any(|(n, _)| *n == fam) && f.weight.0 == w && f.style == resvg::usvg::fontdb::Style::Normal).or_else(|| db.faces().find(|f| f.families.iter().any(|(n, _)| *n == fam) && f.weight.0 == w)).map(|f| f.id);
        let Some(id) = id else { continue };
        let Some((data, index)) = db.with_face_data(id, |d, i| (d.to_vec(), i)) else { continue };
        let mut fd = egui::FontData::from_owned(data);
        fd.index = index;
        ctx.add_font(egui::epaint::text::FontInsert::new(&key, fd, vec![egui::epaint::text::InsertFontFamily { family: FontFamily::Name(key.clone().into()), priority: egui::epaint::text::FontPriority::Highest }]));
        reg.insert(key);
        added = true;
    }
    added
}

// egui family and variation axes for a character style
fn font_for(s: &Shape, st: &CharStyle, size: f32) -> (FontId, VariationCoords) {
    if !st.family.is_empty() {
        if let Some(w) = nearest_weight(&st.family, st.weight) {
            let key = face_key(&st.family, w);
            if registered().lock().unwrap().contains(&key) {
                let tags: Vec<([u8; 4], f32)> = s.axes.iter().filter_map(|(t, v)| Some((t.as_bytes().get(0..4)?.try_into().ok()?, *v))).collect();
                let coords = if tags.is_empty() { VariationCoords::default() } else { VariationCoords::new(tags.iter().map(|(t, v)| (t, *v))) };
                return (FontId::new(size, FontFamily::Name(key.into())), coords);
            }
        }
        // not installed (yet): the layer's default face
    }
    if s.mono {
        return (FontId::new(size, FontFamily::Monospace), VariationCoords::default());
    }
    // the old Semi Bold keeps its own static face; any other weight uses the variable Inter's weight axis
    if st.weight == 600 && s.weight == 0 && s.opsz == 0.0 && s.runs.iter().all(|r| r.weight.is_none()) {
        return (FontId::new(size, FontFamily::Name("semibold".into())), VariationCoords::default());
    }
    // the variable Inter's axes: weight, and optical size when the layer sets one (Type settings > Variable)
    let mut axes: Vec<(&[u8; 4], f32)> = vec![];
    if st.weight != 400 {
        axes.push((b"wght", st.weight as f32));
    }
    if s.opsz > 0.0 {
        axes.push((b"opsz", s.opsz.clamp(14.0, 32.0)));
    }
    let coords = if axes.is_empty() { VariationCoords::default() } else { VariationCoords::new(axes) };
    (FontId::new(size, FontFamily::Proportional), coords)
}

// the egui layout job of a text layer at `zoom` (1.0 = world units)
pub fn text_job(painter: &Painter, s: &Shape, zoom: f32, color: Color32, wrap_width: f32, raw: bool) -> LayoutJob {
    let chars = if raw { s.raw_chars() } else { s.display_chars() };
    let chars = if !raw && s.is_rtl() { bidi_visual(chars, s.text_dir) } else { chars };
    let custom_ul = s.custom_underline();
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width * zoom;
    job.halign = match s.align {
        TextAlign::Left => Align::LEFT,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::RIGHT,
    };
    if s.max_lines > 0 {
        job.wrap.max_rows = s.max_lines as usize;
        job.wrap.break_anywhere = false;
        job.wrap.overflow_character = Some('\u{2026}');
    }
    let mut k = 0;
    while k < chars.len() {
        let st = chars[k].1.clone();
        let mut text = String::new();
        while k < chars.len() && chars[k].1 == st {
            text.push(chars[k].0);
            k += 1;
        }
        let mut size = st.size;
        if st.small {
            size *= 0.8;
        }
        if st.script != 0 {
            size *= 0.6;
        }
        if st.spacer {
            size = 1.0;
        }
        let (font_id, coords) = font_for(s, &st, size * zoom);
        let c = st.color.map_or(color, |c| if color.a() == 255 { c } else { c.gamma_multiply(color.a() as f32 / 255.0) });
        let c = if st.fade == 255 { c } else { c.gamma_multiply(st.fade as f32 / 255.0) };
        let deco = |on: bool| if on { Stroke::new((st.size / 14.0).max(1.0) * zoom, c) } else { Stroke::NONE };
        let mut fmt = TextFormat {
            font_id: font_id.clone(),
            color: c,
            extra_letter_spacing: st.size * s.tracking / 100.0 * zoom,
            italics: st.italic,
            underline: deco(st.underline && (raw || !custom_ul)),
            strikethrough: deco(st.strike),
            coords,
            valign: match st.script {
                1 => Align::TOP,
                2 => Align::BOTTOM,
                _ => Align::BOTTOM,
            },
            ..TextFormat::default()
        };
        if s.line_h > 0.0 {
            fmt.line_height = Some(st.size * s.line_h * zoom);
        }
        if st.spacer {
            fmt.line_height = Some((s.para_spacing + if s.list != 0 { s.list_spacing } else { 0.0 }) * zoom);
        }
        if st.indent {
            let space = painter.fonts_mut(|f| f.glyph_width(&font_id, ' '));
            fmt.extra_letter_spacing = s.para_indent * zoom - space;
        }
        job.append(&text, 0.0, fmt);
    }
    if job.sections.is_empty() {
        // an empty layer is still one line high
        let st = s.base_style();
        let (font_id, coords) = font_for(s, &st, st.size * zoom);
        job.append("", 0.0, TextFormat { font_id, color, coords, ..TextFormat::default() });
    }
    job
}

impl Shape {
    // the layer's characters with their styles as typed (no case, markers or spacing): what the editor shows while it edits
    pub fn raw_chars(&self) -> Vec<(char, CharStyle)> {
        let base = self.base_style();
        self.text.chars().enumerate().map(|(i, c)| (c, self.style_at(&base, i))).collect()
    }

    // styles the characters `start..end`: `f` changes a run that covers exactly that range (the runs are then merged again)
    pub fn style_range(&mut self, start: usize, end: usize, f: impl Fn(&mut TextRun)) {
        let n = self.text.chars().count();
        let (start, end) = (start.min(end).min(n), start.max(end).min(n));
        if start == end {
            return;
        }
        let mut run = TextRun { start, end, ..TextRun::default() };
        f(&mut run);
        self.runs.push(run);
        self.normalize_runs();
    }

    // the runs as one run per stretch of equal style, in order, without empty or unstyled ones
    pub fn normalize_runs(&mut self) {
        let n = self.text.chars().count();
        let folded: Vec<TextRun> = (0..n)
            .map(|i| {
                let mut st = TextRun::default();
                for r in self.runs.iter().filter(|r| r.start <= i && i < r.end) {
                    st.bold = r.bold.or(st.bold);
                    st.weight = r.weight.or(st.weight);
                    st.italic = r.italic.or(st.italic);
                    st.size = r.size.or(st.size);
                    st.color = r.color.or(st.color);
                    st.underline = r.underline.or(st.underline);
                    st.strike = r.strike.or(st.strike);
                    st.family = r.family.clone().or(st.family);
                    st.url = r.url.clone().or(st.url);
                }
                st
            })
            .collect();
        let mut out: Vec<TextRun> = vec![];
        for (i, st) in folded.into_iter().enumerate() {
            let plain = TextRun { start: 0, end: 0, ..st.clone() } == TextRun::default();
            if plain {
                continue;
            }
            match out.last_mut() {
                Some(l) if l.end == i && TextRun { start: 0, end: 0, ..l.clone() } == TextRun { start: 0, end: 0, ..st.clone() } => l.end = i + 1,
                _ => out.push(TextRun { start: i, end: i + 1, ..st }),
            }
        }
        self.runs = out;
    }

    // the text changed from `old` to `new`: keep every run on the characters it covered (what was typed inside a run joins it)
    pub fn shift_runs(&mut self, old: &str, new: &str) {
        if self.runs.is_empty() {
            return;
        }
        let (o, n): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
        let prefix = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
        let max_suffix = o.len().min(n.len()) - prefix;
        let suffix = o.iter().rev().zip(n.iter().rev()).take(max_suffix).take_while(|(a, b)| a == b).count();
        let (removed_end, added) = (o.len() - suffix, n.len() - suffix - prefix);
        let delta = added as isize - (removed_end - prefix) as isize;
        let map = |p: usize| -> usize {
            if p < prefix {
                p
            } else if p >= removed_end {
                (p as isize + delta).max(0) as usize
            } else {
                p.min(prefix + added) // inside the replaced stretch
            }
        };
        for r in &mut self.runs {
            // a run that ended where the typing began takes the new characters; one that began there does not
            let (s, e) = (map(r.start), map(r.end));
            r.start = s;
            r.end = e.max(s);
        }
        self.runs.retain(|r| r.end > r.start);
        self.normalize_runs();
    }
}

// ---- direction, hanging, underline styles and font axes

use egui::{Galley, Pos2};

// the underline strokes of a laid-out text, in galley space: lines (one polyline each) and dots
pub enum Ul {
    Line(Vec<Pos2>),
    Dot(Pos2),
}

pub struct UlDraw {
    pub marks: Vec<Ul>,
    pub width: f32,             // stroke width (a dot's diameter)
    pub color: Option<Color32>, // None: the text's own colour
}

impl Shape {
    // an underline with anything but the plain look: egui's own underline is switched off and `underline_marks` draws it
    pub fn custom_underline(&self) -> bool {
        self.ul_style != 0 || self.ul_thick > 0.0 || self.ul_offset != 0.0 || self.ul_skip || self.ul_color.is_some()
    }

    // right-to-left text: set by the layer, or found in the characters
    pub fn is_rtl(&self) -> bool {
        match self.text_dir {
            1 => false,
            2 => true,
            _ => self.text.chars().any(|c| matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)),
        }
    }

    // what hangs into the margin with "hanging punctuation and lists": the list marker or a leading quote
    pub fn hang_lead(&self) -> Option<String> {
        if !self.hang || self.align != TextAlign::Left {
            return None;
        }
        match self.list {
            1 => Some("\u{2022}\u{2002}".into()),
            2 => Some("1.\u{2002}".into()),
            _ => self.text.chars().next().filter(|c| matches!(c, '"' | '\'' | '\u{201c}' | '\u{2018}' | '\u{ab}' | '(' | '\u{300c}')).map(String::from),
        }
    }

    // how far left the text moves for `hang_lead`, in world units
    pub fn hang_shift(&self, painter: &Painter) -> f32 {
        let Some(lead) = self.hang_lead() else { return 0.0 };
        painter.layout_no_wrap(lead, FontId::proportional(self.font_size), Color32::WHITE).size().x
    }
}

// the characters in the order they are drawn: right-to-left stretches reversed (the Unicode bidirectional algorithm)
pub fn bidi_visual(chars: Vec<(char, CharStyle)>, dir: u8) -> Vec<(char, CharStyle)> {
    use unicode_bidi::{BidiInfo, Level};
    let text: String = chars.iter().map(|c| c.0).collect();
    let level = match dir {
        1 => Some(Level::ltr()),
        2 => Some(Level::rtl()),
        _ => None,
    };
    let info = BidiInfo::new(&text, level);
    if !info.has_rtl() && dir != 2 {
        return chars;
    }
    // byte offset -> index in `chars`
    let starts: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    let at = |byte: usize| starts.partition_point(|&b| b < byte);
    let mut out = Vec::with_capacity(chars.len());
    for para in &info.paragraphs {
        let (levels, runs) = info.visual_runs(para, para.range.clone());
        for run in runs {
            let (a, b) = (at(run.start), at(run.end));
            if levels[run.start].is_rtl() {
                out.extend(chars[a..b].iter().rev().cloned());
            } else {
                out.extend(chars[a..b].iter().cloned());
            }
        }
    }
    out
}

fn thickness_pad(size: f32) -> f32 {
    (size / 14.0).max(1.0)
}

// Underline style, thickness, offset, skip ink and colour: the marks for every underlined stretch of every row
pub fn underline_marks(s: &Shape, galley: &Galley, zoom: f32) -> UlDraw {
    let chars = s.display_chars();
    let chars = if s.is_rtl() { bidi_visual(chars, s.text_dir) } else { chars };
    let mut marks = vec![];
    let mut ci = 0;
    let mut width = 1.0;
    for row in &galley.rows {
        let mut span: Option<(f32, f32, f32, f32)> = None; // x0, x1, baseline y, font size
        let mut cuts: Vec<(f32, f32)> = vec![];
        let flush = |span: &mut Option<(f32, f32, f32, f32)>, cuts: &mut Vec<(f32, f32)>, marks: &mut Vec<Ul>, width: &mut f32| {
            let Some((x0, x1, y, size)) = span.take() else { return };
            let thick = if s.ul_thick > 0.0 { s.ul_thick * zoom } else { (size / 14.0).max(1.0) * zoom };
            *width = thick;
            let y = y + if s.ul_offset != 0.0 { s.ul_offset * zoom } else { size * 0.12 * zoom } + thick / 2.0;
            // skip ink: the line is cut where descenders cross it
            let mut pieces = vec![(x0, x1)];
            if s.ul_skip {
                for &(c0, c1) in cuts.iter() {
                    pieces = pieces.into_iter().flat_map(|(a, b)| if c1 <= a || c0 >= b { vec![(a, b)] } else { vec![(a, c0.max(a)), (c1.min(b), b)] }).filter(|(a, b)| b - a > 0.5).collect();
                }
            }
            for (a, b) in pieces {
                match s.ul_style {
                    1 => {
                        // dots: a round dot every two widths
                        let mut x = a + thick / 2.0;
                        while x < b {
                            marks.push(Ul::Dot(Pos2::new(x, y)));
                            x += thick * 2.0;
                        }
                    }
                    2 => {
                        // wavy: a zigzag
                        let step = (thick * 2.0).max(2.0);
                        let mut pts = vec![];
                        let mut x = a;
                        let mut up = false;
                        while x < b {
                            pts.push(Pos2::new(x, y + if up { -thick } else { thick }));
                            up = !up;
                            x += step;
                        }
                        pts.push(Pos2::new(b, y + if up { -thick } else { thick }));
                        marks.push(Ul::Line(pts));
                    }
                    _ => marks.push(Ul::Line(vec![Pos2::new(a, y), Pos2::new(b, y)])),
                }
            }
            cuts.clear();
        };
        for g in &row.row.glyphs {
            let st = chars.get(ci).or(chars.last()).map(|c| c.1.clone());
            ci += 1;
            let Some(st) = st.filter(|st| !st.spacer && !st.indent) else {
                flush(&mut span, &mut cuts, &mut marks, &mut width);
                continue;
            };
            if st.underline {
                let x = row.pos.x + g.pos.x;
                let y = row.pos.y + g.pos.y;
                match &mut span {
                    Some(sp) => {
                        sp.1 = x + g.advance_width;
                        sp.2 = sp.2.max(y);
                    }
                    None => span = Some((x, x + g.advance_width, y, st.size)),
                }
                if "gjpqyQ,;()[]{}|".contains(g.chr) {
                    let pad = thickness_pad(st.size) * zoom;
                    cuts.push((x - pad, x + g.advance_width + pad));
                }
            } else {
                flush(&mut span, &mut cuts, &mut marks, &mut width);
            }
        }
        flush(&mut span, &mut cuts, &mut marks, &mut width);
        if row.ends_with_newline {
            ci += 1;
        }
    }
    UlDraw { marks, width, color: s.ul_color }
}

// the variation axes of an installed family: tag, lowest, default, highest (empty for a static font)
pub fn family_axes(family: &str) -> Vec<([u8; 4], f32, f32, f32)> {
    let db = crate::export::fonts();
    let Some(face) = db.faces().find(|f| f.families.iter().any(|(n, _)| n == family)) else { return vec![] };
    let id = face.id;
    db.with_face_data(id, |d, index| fvar_axes(d, index as usize)).unwrap_or_default()
}

// the axes in a font file's fvar table
fn fvar_axes(d: &[u8], index: usize) -> Vec<([u8; 4], f32, f32, f32)> {
    let u16_at = |o: usize| d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize);
    let u32_at = |o: usize| d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let fix = |o: usize| d.get(o..o + 4).map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f32 / 65536.0);
    // a collection holds several fonts: find this one's table directory
    let base = if d.get(0..4) == Some(b"ttcf") { u32_at(12 + 4 * index).unwrap_or(0) } else { 0 };
    let Some(n) = u16_at(base + 4) else { return vec![] };
    for t in 0..n {
        let rec = base + 12 + 16 * t;
        if d.get(rec..rec + 4) == Some(b"fvar") {
            let Some(off) = u32_at(rec + 8) else { return vec![] };
            let (Some(axes_off), Some(count), Some(size)) = (u16_at(off + 4), u16_at(off + 8), u16_at(off + 10)) else { return vec![] };
            return (0..count)
                .filter_map(|a| {
                    let o = off + axes_off + a * size;
                    let tag: [u8; 4] = d.get(o..o + 4)?.try_into().ok()?;
                    Some((tag, fix(o + 4)?, fix(o + 8)?, fix(o + 12)?))
                })
                .collect();
        }
    }
    vec![]
}

// is the family a variable font? (cached by name)
pub fn is_variable(family: &str) -> bool {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, bool>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(Default::default()));
    if let Some(&v) = cache.lock().unwrap().get(family) {
        return v;
    }
    let v = family == "Inter" || !family_axes(family).is_empty();
    cache.lock().unwrap().insert(family.to_owned(), v);
    v
}

#[cfg(test)]
pub fn fvar_axes_for_test(d: &[u8]) -> Vec<([u8; 4], f32, f32, f32)> {
    fvar_axes(d, 0)
}
