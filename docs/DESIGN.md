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
| `fig::AGENT` | `#d97757` | agent mode: canvas border, mode button, markers, send button |
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
- Panel padding: 16 left, 8 right. Columns: `Grid::of(panel)` gives `cw = (W - 64) / 2`, `c2 = left + cw + 8`, `c3 = right - 32` (24px icon column). Panels are 300 wide by default (resizable 240 to 480).
- **Give things air.** Inside a card, popover, dialog or chat box the padding is 16px on every side (the same as the panel's left padding), and groups inside it (a field and the options under it, a message and the next) are 16px apart. 8px is for controls that belong together (a field and its own button, a label and its value). Do not go under 12px between a field and the edge or divider above it; when a layout looks tight, it is: open it up rather than argue for the smaller number. Agent mode's box and modal use `agent.rs::PAD` (16).
- Vertical rhythm: header 40 → first label row 20 (later 24) → control row 24 → `fig::end` = 16 + 1px divider.

## 2b. Fields while focused

A focused text field gets a 1px `BRAND` border inside its box and a 3px outline of `BRAND` at 8% around it (`fig::focus_ring`, drawn by `text_edit` and the number field's editor; `text_edit_in` puts the ring round a whole field when only its text part is passed, or turns it off for fields with their own focus look).

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
| `dropdown(ui, rect, icon, &mut T, items, name)` | field + chevron + menu (`glyphs::menu_option`: 24px rows inset 6px from the card, label at 8, the current or hovered row filled white with black text, no check) |
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
- **Dropdown options**: `glyphs::menu_option(ui, on, label)` for a list of values (weight, case, sizes): 24px rows, the label at 8, the current row filled white (`fig::MENU_HOT`) with black text (`MENU_HOT_INK`), no check. `menu_item` is for command menus with icons and shortcuts (its label starts at 38); `menu_item_checked` is for a menu of tools (icon at 19, name at 35, the current tool filled like the hovered row). Every highlighted menu row is white with black text, never blue, and the rows of a plain menu sit `fig::MENU_INSET` (6px) in from the card's edges, 6px above the first and below the last. A check mark is only for on/off toggles (`menu_check`).
- **Type settings popover** (`props.rs::type_settings`): `picker::popup` placement, a 40px tab row (`fig::tab`), then 32px rows built with a local `line(label, width)` that right-aligns a 24px control at the 16px padding. Reuse the pattern for other "settings" popovers.
- **Font picker** (`props.rs::family_picker`): a "Fonts" popover with a close button, a focused search field with a magnifier and 28px rows (current one filled, with a check), scrolled with `show_rows`.
- **Menus**: `glyphs::menu_item(ui, icon, label, shortcut, enabled)` inside `egui::Popup::menu(&resp).show(..)`. Items size to the widest item; do not call `set_min_width` with a big number.
- **Colour picker**: `picker::popup(&resp, panel_left, |ui| ..)`, `picker::header`, `picker::types`, `picker::body` (square, hue, opacity, Hex/RGB/HSB, "On this page" swatches). Pass the page colours via `picker::page_colors_id()`. Use `fig::swatch` instead of building your own.
- **Comments**: `comments.rs` (pin, hover card, composer, thread). Pin 32, avatar 24, composer 288x40, thread 360 wide.
- **Icons**: `glyphs::Glyph` (Lucide SVGs in `assets/icons`, rasterized per size). 16px in 24px buttons, 13px in fields/rows, 9.5px chevrons. Add an icon = add the SVG, a `Glyph` variant and its `svg_of` line; the gallery and the icon test pick it up from `all_glyphs()` (regenerate that list when you add a variant).
- **Left-panel list pieces** (`panels.rs`, private for now): `left_header` (44px title row, 24px actions), `search_field`, `fold_head`, `list_row` (32px row, 24px highlight inset 8px), `row_button`, `empty_row`.

## 5. Panel anatomy

- **Menu header** (a docked 40px bar under the tab bar, 1px rule below, `panels.rs::menu_header`): the Menu icon, then the icon-and-label buttons File (opens the file menu), Assets and Variables (each opens a window over the canvas, one at a time, Esc closes), Help at the right end. Version history is under File, the comments panel under View.
- **Tab bar** (40, `tabs.rs`): a Home button (the home page lists the recent files), then one square, full-height tab per open file with no gap, 12px regular text and a 14px close icon, the open one on `BG`; the menu header is docked under it, the left and right panels float over the canvas (card with `R_LARGE`, 8px from the canvas edges, `Shift+\` / `Alt+\` hide them).
- **Left panel** (300): file header 64px (13px semibold name, a subline with where the file lives, "Drafts" until saved, and a "Local" badge, minimize, save); the Canvas list (the pages) has Find, New folder and Add in its header; a folder row (chevron, folder icon, name) folds on a click, a canvas row (file icon, name) sits at the top level or 16px in under a folder; a canvas can be dragged to reorder, dropped on a folder row to file it there, or dropped under the list (or "Move out of folder" in its menu) to take it out; both have a right-click menu and rename on a double click; the magnifier opens a search view over the whole panel; a layer row's right-click menu is the canvas menu with Rename in front. Picking a layer on the canvas opens its canvas's layers too. A click on a canvas opens it: the panel then shows that canvas's layers under a 44px header (Back chevron, the canvas name, Find, Collapse), and Back returns to the list. Layers are always inside a canvas; the list and the layers are two views of the same panel, not two sections. Assets is a window (menu header), with its own 48px header.
- **Right panel** (300): 48px top bar (Present, Share), 32px tabs + zoom, then sections (Position, Layout / Auto layout, Appearance, Fill, Stroke, Effects, Export).
- **Toolbar** (bottom center, `R_LARGE`, 6px padding): 34px tool buttons without letter badges (12px corners: concentric with the card, `R_LARGE` minus the 6px padding), then a divider and the 5-icon mode strip (Draw, Design, Motion, Dev Mode, and Agent mode in `fig::AGENT`) at the right end.
- **Agent mode** (`agent.rs`): every panel, the bottom toolbar and the Motion timeline are hidden; only the tab bar stays. A 2px `fig::AGENT` outline runs round the canvas with a solid "Agent Mode" notch hanging from its top edge (36 tall, 14px semibold white `fig::AGENT_INK` text, rounded at the bottom and flared into the outline) and a translucent round close button at its right end; Esc closes agent mode too (first it closes the list of mentions). While Claude runs, an invisible full-window layer blocks everything but pan and zoom, and a click or key shows a short "Not allowed" alert under the notch.
  - **Per-layer box**: clicking a layer opens a comment-styled box beside it (`comments::thread_frame`, 320 wide, 14px text, 16px `PAD` all round): header, messages, the flashing status and Stop while it runs, its messages in a scroll area (`comments::thread_scroll`: at least 96 tall, at most half the canvas and 480, newest in view, the same as a comment thread), a 1px `fig::AGENT` field whose send button (`send_button_with`) is centered on it, then 16px down an Edit / Duplicate switch and a "Write reply" checkbox. An answered session leaves a pin (`comments::draw_pin`, outlined in white on hover).
  - **Session cards** (down the left edge, 20px in from the canvas edges, `left_w` wide): no panel behind them, just one floating card per session straight on the canvas (64px, `R_LARGE`, the agent "glass" fill: `BG_MENU` at 84% so the canvas shows through faintly, a 1px rule and a soft shadow; first message as the title, "layer · state" under it, a delete button), 12px apart. A card opens its conversation as one floating card of the same style (back and delete in its header, read-only with Stop).
  - **Composer** (`agent_input`): pinned to the bottom of the left panel, the panel's width (the conversation or the session cards above it leave `COMPOSER` px free). The layout of the composer of Claude's desktop app (measured on its Chat tab at 144 dpi): radius 12, text 14px Inter with 24px lines, a 20px "+" centered 21.3 in and 21.7 above the bottom (it inserts an @), a coral round 32px send button (a stop square while Claude works), in the session cards' glass colors. Under the field a context line says what a send does: "Continuing: <title>" (a conversation is open in the panel), "Changing: <layer>" (none open, a layer is selected) or "New design" (nothing selected); a change also shows the Edit / Duplicate switch. drui always writes its reply. The one rule: what you click decides the job (empty canvas = create, a layer = change it; the comment-style box beside a layer is the quick way to the same thing and shares the conversation). Its list of @ mentions (layers, components, variables; arrows and Enter pick, Esc closes) is our own `Order::Tooltip` area with a solid `BG_MENU` fill (`popup_frame`), never `egui::Popup`: a stock popup draws on a layer below the box and the box shows through it. The text caret is drawn by hand at the height of the text, because egui's is as tall as the 24px line. A picked name shows in `fig::AGENT` and is told to Claude with its id, as context only.
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
