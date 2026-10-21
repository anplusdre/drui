// The design system's palette: every surface grey and the text ramp live here and nowhere else, so restyling the app
// is a change to this file. main.rs and fig.rs re-export these under the names the panels use.
// The dark palette is Figma UI3 dark, one step darker by request (Figma: #2c2c2c / #383838 / #444444 / #1e1e1e); the light one is
// Figma's light UI (white panels, #f5f5f5 fields, #e6e6e6 borders, a #e5e5e5 canvas, near-black text).
// The colours are functions because the interface can be switched between the two while the app runs.
#![allow(non_snake_case)]
use std::sync::atomic::{AtomicBool, Ordering};

use egui::Color32;

static LIGHT: AtomicBool = AtomicBool::new(false);

pub fn set_light(on: bool) {
    LIGHT.store(on, Ordering::Relaxed);
}

pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

const fn grey(v: u8) -> Color32 {
    Color32::from_rgb(v, v, v)
}

fn pick(dark: Color32, light: Color32) -> Color32 {
    if is_light() { light } else { dark }
}

// surfaces
pub fn SURFACE() -> Color32 {
    pick(grey(0x22), grey(0xff)) // panels, toolbar, popovers (Figma dark #2c2c2c)
}
pub fn FIELD() -> Color32 {
    pick(grey(0x2c), grey(0xf5)) // inputs, hover fill, pills (Figma #383838)
}
pub fn FIELD_HOVER() -> Color32 {
    pick(grey(0x34), grey(0xeb)) // hovered input (Figma #404040)
}
pub fn LINE() -> Color32 {
    pick(grey(0x38), grey(0xe0)) // dividers, borders, pressed (Figma #444444)
}
pub fn SUNKEN() -> Color32 {
    pick(grey(0x16), grey(0xe5)) // canvas, menus, code blocks (Figma #1e1e1e)
}
pub fn WELL() -> Color32 {
    pick(grey(0x28), grey(0xee)) // recessed chips and wells (Figma #333333)
}
pub fn CHECK_BORDER() -> Color32 {
    pick(grey(0x58), grey(0xb3)) // unchecked checkbox outline (Figma #606060)
}

// text: the colour at a fixed alpha, so it follows whatever it sits on
pub fn TEXT() -> Color32 {
    pick(Color32::WHITE, grey(0x1e))
}
pub fn TEXT_SECONDARY() -> Color32 {
    pick(Color32::from_rgba_premultiplied(179, 179, 179, 179), Color32::from_rgba_premultiplied(0, 0, 0, 140)) // 70% / 55%
}
pub fn TEXT_TERTIARY() -> Color32 {
    pick(Color32::from_rgba_premultiplied(102, 102, 102, 102), Color32::from_rgba_premultiplied(0, 0, 0, 90)) // 40% / 35%
}

// the blue that marks a selected row, and the ink on it
pub fn SELECTED_BG() -> Color32 {
    pick(Color32::from_rgb(0x39, 0x43, 0x60), Color32::from_rgb(0xd9, 0xe8, 0xff))
}
pub fn SELECTED_FG() -> Color32 {
    pick(Color32::from_rgb(0x7c, 0xc4, 0xf8), Color32::from_rgb(0x0d, 0x6e, 0xd1))
}
pub fn MENU_SURFACE() -> Color32 {
    pick(Color32::from_rgb(0x1e, 0x1e, 0x1e), grey(0xff))
}
pub fn MENU_LINE() -> Color32 {
    pick(Color32::from_rgb(0x35, 0x35, 0x35), grey(0xe0))
}
