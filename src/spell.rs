// Spell checking, with the operating system's own checker (Windows' spell checking API, the one Edge and Office use). Elsewhere
// nothing is reported. Results are kept per text, so a layer is checked once however often it is drawn.
use std::collections::HashMap;
use std::sync::Mutex;

// a misspelled stretch of a text: where it starts and how long it is, in characters
pub type Miss = (usize, usize);

static CACHE: Mutex<Option<HashMap<String, Vec<Miss>>>> = Mutex::new(None);

pub fn available() -> bool {
    cfg!(windows)
}

// the misspelled words of `text`
pub fn misspelled(text: &str) -> Vec<Miss> {
    if text.trim().is_empty() {
        return vec![];
    }
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(v) = cache.get(text) {
        return v.clone();
    }
    if cache.len() > 2000 {
        cache.clear();
    }
    let v = imp::check(text);
    cache.insert(text.to_owned(), v.clone());
    v
}

// replacements the checker offers for `word`, best first
pub fn suggestions(word: &str) -> Vec<String> {
    imp::suggest(word)
}

// stop reporting `word` (this session) and forget what was cached
pub fn ignore(word: &str) {
    imp::ignore(word);
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

#[cfg(windows)]
mod imp {
    use super::Miss;
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Globalization::{ISpellChecker, ISpellCheckerFactory, SpellCheckerFactory};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};

    thread_local! {
        static CHECKER: Option<ISpellChecker> = make();
    }

    fn make() -> Option<ISpellChecker> {
        unsafe {
            // already initialised by the window system on this thread (a different mode) is fine
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let factory: ISpellCheckerFactory = CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            factory.CreateSpellChecker(&HSTRING::from("en-US")).ok()
        }
    }

    pub fn check(text: &str) -> Vec<Miss> {
        let utf16: Vec<u16> = text.encode_utf16().collect();
        // UTF-16 offsets to character offsets
        let char_at = |u: usize| text.char_indices().scan(0usize, |n, (_, c)| { let at = *n; *n += c.len_utf16(); Some(at) }).take_while(|&at| at < u).count();
        CHECKER.with(|c| {
            let Some(c) = c else { return vec![] };
            let mut out = vec![];
            unsafe {
                let Ok(errors) = c.Check(&HSTRING::from_wide(&utf16)) else { return out };
                loop {
                    let mut e = None;
                    if errors.Next(&mut e).is_err() {
                        break;
                    }
                    let Some(e) = e else { break };
                    let (Ok(start), Ok(len)) = (e.StartIndex(), e.Length()) else { continue };
                    let (a, b) = (char_at(start as usize), char_at((start + len) as usize));
                    out.push((a, b.saturating_sub(a)));
                }
            }
            out
        })
    }

    pub fn suggest(word: &str) -> Vec<String> {
        CHECKER.with(|c| {
            let Some(c) = c else { return vec![] };
            let mut out = vec![];
            unsafe {
                let Ok(list) = c.Suggest(&HSTRING::from(word)) else { return out };
                loop {
                    let mut item = [PWSTR::null()];
                    let mut got = 0u32;
                    if list.Next(&mut item, Some(&mut got)).is_err() || got == 0 {
                        break;
                    }
                    if let Ok(s) = item[0].to_string() {
                        out.push(s);
                    }
                    if out.len() >= 5 {
                        break;
                    }
                }
            }
            out
        })
    }

    pub fn ignore(word: &str) {
        CHECKER.with(|c| {
            if let Some(c) = c {
                let _ = unsafe { c.Ignore(&HSTRING::from(word)) };
            }
        });
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Miss;
    pub fn check(_: &str) -> Vec<Miss> {
        vec![]
    }
    pub fn suggest(_: &str) -> Vec<String> {
        vec![]
    }
    pub fn ignore(_: &str) {}
}

use crate::fig::{self, Grid};
use crate::model::Kind;
use crate::App;

impl App {
    // the Design panel lists the misspelled words of the selected text layer, each with a fix and Ignore
    pub(crate) fn spelling_section(&mut self, ui: &mut egui::Ui, g: &Grid, i: usize) {
        if !self.spell || !available() || self.shapes[i].kind != Kind::Text {
            return;
        }
        let text = self.shapes[i].text.clone();
        let misses = misspelled(&text);
        if misses.is_empty() {
            return;
        }
        fig::header(ui, "Spelling", false, &[]);
        let chars: Vec<char> = text.chars().collect();
        let mut fix: Option<(usize, usize, String)> = None;
        let mut skip: Option<String> = None;
        for (start, len) in misses.into_iter().take(6) {
            let word: String = chars[start..(start + len).min(chars.len())].iter().collect();
            let r = fig::controls(ui, 4.0);
            fig::text(ui, egui::Pos2::new(g.left, r.center().y), egui::Align2::LEFT_CENTER, &word, fig::reg(11.0), crate::TEXT());
            let half = (g.cw - 4.0) / 2.0;
            let ignore = egui::Rect::from_min_size(egui::Pos2::new(g.right - half, r.top()), egui::Vec2::new(half, fig::ROW));
            if fig::button(ui, ignore, "Ignore").clicked() {
                skip = Some(word.clone());
            }
            if let Some(best) = suggestions(&word).into_iter().next() {
                let b = egui::Rect::from_min_size(egui::Pos2::new(g.right - half * 2.0 - 4.0, r.top()), egui::Vec2::new(half, fig::ROW));
                if fig::button(ui, b, &best).clicked() {
                    fix = Some((start, len, best));
                }
            }
        }
        if let Some((start, len, with)) = fix {
            let mut c: Vec<char> = chars;
            c.splice(start..(start + len).min(c.len()), with.chars());
            self.shapes[i].text = c.into_iter().collect();
        }
        if let Some(w) = skip {
            ignore(&w);
        }
        fig::end(ui);
    }
}
