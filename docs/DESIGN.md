# drui design system

drui follows the look of **Figma's desktop app (dark)**; see the disclaimer in the README. This file is the contract for any UI work, by people or coding agents: use the shared tokens and widgets below, never invent sizes, colors or fonts. See it rendered with `drui --gallery` (`src/gallery.rs`).

Source of truth in code: the grey/text palette in `src/theme.rs` (re-exported by `src/main.rs` and `src/fig.rs`; restyle there), other tokens in `src/main.rs` (top) and `src/fig.rs`; widgets in `src/fig.rs`, `src/picker.rs`, `src/comments.rs`, `src/glyphs.rs`; theme in `src/widgets.rs` (`apply_theme`). All numbers are CSS px (egui points) and were measured from the Figma desktop app with REA (see CLAUDE.md).

## 1. Principles

1. **Match Figma exactly, measured, not guessed.** If a number is not here, measure it in Figma (`tools/shot.ps1`, `tools/measure.py`, `tools/ink.py`) and put it in `fig.rs` with a comment.
2. **One widget per job, shared.** Reuse `fig::*`. If a second panel needs something a panel has privately (`left_header`, `list_row`, `fold_head`, `search_field` in `panels.rs`), move it to `fig.rs` first.
3. **Flat and dark.** No gradients, no heavy shadows (only popovers/toolbar/pins), hover is a fill change, selection is `SELECTED` + `SELECTED_INK`.
4. **Hug content.** Menus and popups are as wide as their widest item; never pad them to a fixed minimum.
5. **Everything is named.** Every custom-painted widget sets `resp.widget_info(...)` so screen readers and the REA observer can find it.

## 2. Tokens

### Color (`theme.rs`)

The reference dark theme is #2c2c2c / #383838 / #444444 / #1e1e1e; drui runs one step darker by request. Dropdown menus do not show a check on the selected item (the field shows the value).


| token | value | use |
|---|---|---|
| `BG` | `#222222` | panels, toolbar, popovers |
| `BG_MENU` | `#161616` | canvas, menus, code blocks |
| `fig::INPUT` / `BG_SECONDARY` | `#2c2c2c` | fields, hover fill, pills |
| `fig::INPUT_HOVER` | `#343434` | hovered field |
| `fig::RULE` / `BG_PRESSED` | `#383838` | 1px dividers, borders, pressed |
| `fig::SELECTED` | `#394360` | selected row / rail item / toggled icon background |
| `fig::SELECTED_INK` | `#7cc4f8` | icon on `SELECTED`, links, active marks |
| `BRAND` | `#0d99ff` | active tool, focus |
| `fig::SHARE` | `#0c8ce9` | Share button, comment pins, send button, checkbox |
| `GUIDE` | `#f24822` | guides and snap lines |
| `fig::COMPONENT_INK` | `#d1a8ff` | components and instances |
| `TEXT` | `#ffffff` | primary text and icons |
| `TEXT_SECONDARY` | white 70% | labels, subtitles |
| `TEXT_TERTIARY` | white 40% | disabled, hints, empty states |

Dev Mode active uses green `#14ae5c`; avatars use a small palette (`comments.rs::avatar_color`).

### Type (Inter, `fig::reg` / `fig::semi`)

| role | font |
|---|---|
| values, names, buttons, menu items | 11 regular |
| section titles, tabs, row titles | 11 semibold (`fig::semi(11.0)`) |
| layer name, file name, Assets title | 13 semibold |
| field labels | 10 regular, `TEXT_SECONDARY` |
| small notes / helpers | 9.5 |
| code | monospace 11 |

Only these two weights exist (`Inter.ttf`, `Inter-SemiBold.ttf`). Do not use `RichText::strong()` for UI text.

### Radius and spacing

- Colours come from `src/theme.rs` only (surfaces and text ramp); the UI is one step darker than Figma on purpose. Selected text in fields is white on `BRAND` (not measured against Figma).
- Radius: `R_SMALL` 2, `fig::RADIUS` / `R_MEDIUM` 5 (controls), `R_LARGE` 18 (cards: toolbar, popovers, dialogs, comment threads, the Actions palette; Figma measures 13, rounder by request), `R_MENU` 12 (menus and dropdown lists). Comment pin: 16 with a sharp bottom-left corner.
- `fig::ROW` 24 = control height. `fig::HEADER` 40 = section title row.
- Panel padding: 16 left, 8 right. Columns: `Grid::of(panel)` gives `cw = (W - 64) / 2`, `c2 = left + cw + 8`, `c3 = right - 32` (24px icon column). Panels are 240 wide (resizable to 480), rail 58.
- Vertical rhythm: header 40 → first label row 20 (later 24) → control row 24 → `fig::end` = 16 + 1px divider.

## 2b. Fields while focused

A field must look the same focused and idle: same box, same text position, same layout around it. Edit boxes are placed with `fig::put`, never `ui.put`; every field that needs a layout test gets one like `page_hex_field_keeps_its_height_while_editing` (it compares the rule positions of a panel before and after a click).

## 3. Text must be vertically centered

Always draw centered text with `fig::ptext` / `fig::text` (they apply `fig::TEXT_LIFT`), never `Painter::text` with a `*_CENTER` align. Center on the container's center line (`rect.center().y`), not on an offset you eyeballed. Single-line inputs use `fig::text_edit` / `fig::text_input` (they fill the whole box and center the text). Verify by measuring ink vs box (`tools/ink.py`); within 1 physical pixel is the bar.

## 4. Components (`src/fig.rs`)

Panels are laid out top to bottom on the **full panel width** with `ui.spacing_mut().item_spacing = Vec2::ZERO`, then place controls by rect. Skeleton of a section:

```rust
let g = fig::Grid::of(ui.max_rect());
let (_, actions) = fig::header(ui, "Fill", false, &[(Glyph::Plus, "Add fill", false)]);
fig::labels(ui, &g, true, "Opacity", "Corner radius");        // first label row = 20px
let r = fig::controls(ui, 0.0);                                // a 24px row
fig::num_field(ui, g.col1(r), ui.id().with(("opacity", id)), Prefix::Icon(Glyph::Contrast), &mut pct, Num { max: 100.0, decimals: 0, suffix: "%", ..Num::default() }, "Opacity");
fig::icon_button(ui, g.icon(r), Glyph::Eye, "Toggle visibility", false);
fig::end(ui);                                                  // 16px + divider
```

| widget | use |
|---|---|
| `row(ui, h)`, `rule(ui)`, `controls(ui, gap)`, `end(ui)` | vertical layout primitives |
| `Grid::{col1,col2,both,icon,full}(row)` | the two-column + icon-column grid. `col2` / `both` on a row that never asks for `icon` run to `Grid::edge` (panel right - 16, the same 16px as `left`); `full` always ends at `edge`. Place an icon with `g.icon(r)`, never by hand, or a widened field will overlap it |
| `header(ui, title, dim, actions)` | 40px section title with right-aligned 24px icon buttons; `dim` greys empty sections |
| `labels(ui, g, first, a, b)` | 10px field labels over the columns |
| `num_field(ui, rect, id, Prefix, &mut f32, Num, name)` | scrub by dragging, type on click, accepts UIA SetValue; `id` must be stable |
| `dropdown(ui, rect, icon, &mut T, items, name)` | field + chevron + menu (`glyphs::menu_option`: 24px rows, check at 11, label at 24, current row filled blue) |
| `segments(ui, rect, items)` | segmented icon control; returns the clicked index |
| `icon_button(ui, rect, glyph, tip, on)` | flat 24px icon button; `on` = toggled look |
| `button`, `outline_button` | filled 24px / full-width outlined button |
| `checkbox`, `swatch`, `hex_text` | checkbox; 14px colour chip opening the picker; editable hex |
| `text_input` / `text_edit` | field with its own background / text only on a background you painted |
| `tab(ui, pos, label, selected)` | 24px pill tab |
| `value_row`, `note`, `code_block` | Dev-Mode value list, info note, numbered code |
| `ptext`, `text`, `reg`, `semi` | centered text and the two fonts |
| `put(ui, rect, widget)` | places a widget in `rect` centred, **without moving the layout cursor**. Use it instead of `Ui::put` (which moves the cursor to the bottom of `rect`: a 20px edit box in a 24px row pulled the panel up 2px while focused) |
| `Tip::tip(text)` on a `Response` | hover tooltip with 8px side padding, 11px text. Use it instead of `on_hover_text` (egui's tooltip shares the menu margin, which has no side padding) |
| `parse_hex(text)` | a typed colour as 0xRRGGBB: 6 digits or the short forms 1, 2 and 3 digits (`fa0` -> `ffaa00`); `hex_text` applies it on Enter / blur |

Other shared pieces:
- **Dropdown options**: `glyphs::menu_option(ui, on, label)` for a list of values (weight, case, sizes): 24px rows, a check at 11 when `on`, the label at 24. `menu_item` is for command menus with icons and shortcuts (its label starts at 38).
- **Type settings popover** (`props.rs::type_settings`): `picker::popup` placement, a 40px tab row (`fig::tab`), then 32px rows built with a local `line(label, width)` that right-aligns a 24px control at the 16px padding. Reuse the pattern for other "settings" popovers.
- **Font picker** (`props.rs::family_picker`): a "Fonts" popover with a close button, a focused search field with a magnifier and 28px rows (current one filled, with a check), scrolled with `show_rows`.
- **Menus**: `glyphs::menu_item(ui, icon, label, shortcut, enabled)` inside `egui::Popup::menu(&resp).show(..)`. Items size to the widest item; do not call `set_min_width` with a big number.
- **Colour picker**: `picker::popup(&resp, panel_left, |ui| ..)`, `picker::header`, `picker::types`, `picker::body` (square, hue, opacity, Hex/RGB/HSB, "On this page" swatches). Pass the page colours via `picker::page_colors_id()`. Use `fig::swatch` instead of building your own.
- **Comments**: `comments.rs` (pin, hover card, composer, thread). Pin 32, avatar 24, composer 288x40, thread 360 wide.
- **Icons**: `glyphs::Glyph` (Lucide SVGs in `assets/icons`, rasterized per size). 16px in 24px buttons, 13px in fields/rows, 9.5px chevrons. Add an icon = add the SVG, a `Glyph` variant and its `svg_of` line; the gallery and the icon test pick it up from `all_glyphs()` (regenerate that list when you add a variant).
- **Left-panel list pieces** (`panels.rs`, private for now): `left_header` (44px title row, 24px actions), `search_field`, `fold_head`, `list_row` (32px row, 24px highlight inset 8px), `row_button`, `empty_row`.

## 5. Panel anatomy

- **Rail** (58): menu at top, then File and Assets (32px buttons 56px apart, 9px labels). Version history is under File, the comments panel under View.
- **Left panel** (240): file header 64px (13px semibold name, a subline with where the file lives, "Drafts" until saved, and a "Local" badge, minimize, save); the magnifier next to Pages opens a search view over the whole panel; a page row has a right-click menu (rename, duplicate, delete) and can be dragged to reorder; a layer row's right-click menu is the canvas menu with Rename in front, Pages, Layers. Assets replaces the whole panel with its own 48px header.
- **Right panel** (240): 48px top bar (Present, Share), 32px tabs + zoom, then sections (Position, Layout / Auto layout, Appearance, Fill, Stroke, Effects, Export).
- **Toolbar** (bottom center, `R_LARGE`, 6px padding): 34px tool buttons without letter badges, then a divider and the 4-icon mode strip (Draw, Design, Motion, Dev Mode) at the right end.
- **Popovers** (picker, comment thread): `BG`, radius `R_LARGE`, 1px `RULE`, soft shadow, placed beside their trigger, flipping when out of room.

## 6. State, input and accessibility

- Everything derived (sizes, layouts) is computed in `App::settle()`, never in widgets. Undo is a full-shape snapshot per gesture.
- Keyboard: tool letters V F R O L P T C, `Shift+D` Dev Mode, `Ctrl+Alt+A` select matching, `Shift+\` minimize the left panel. Add new shortcuts to the help sheet (`panels.rs`, the `rows` array) too.
- Custom widgets: `ui.interact(rect, id, Sense)` + `resp.widget_info(...)`; clicks must not depend on `interact_pointer_pos()` (accessibility clicks have none); text fields accept UIA `SetValue` (use `fig::text_edit`, `fig::num_field`).

## 7. Checklist for new UI

1. Find the Figma equivalent and measure it (REA + `tools/measure.py`); note numbers in `fig.rs`.
2. Build it from `fig::*`; add a new shared widget to `fig.rs` (not inline in a panel) and to `gallery.rs`.
3. Text centered via `ptext`; inputs via `text_edit`; menus hug content; every widget named.
4. `cargo test` (the gallery and every panel render headless), then run it: `drui demo.json --select 1 [--mode motion|dev|draw] [--left assets|history|comments]`, capture with `tools/shot.ps1`, drive with `tools/drive.ps1`, compare with Figma.
5. Update `docs/features.md` / `docs/todo.md`.
