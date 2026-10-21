# Architecture

Notes for reading and changing the code. `src/` is one crate (egui/eframe 0.35). Build and run are in the [README](../README.md).

`cargo test` runs the logic tests and a headless run of the whole UI (`tests.rs`).

## Files in src/

| file | role |
|---|---|
| `main.rs` | `App` state, tree helpers (`chain`, `origin_of`, `hit_test`, `move_to`, `group`, ...), file/page ops, keyboard shortcuts, `ui()` frame loop, `main()`/CLI args |
| `model.rs` | `Shape` (every field has a serde default), `Paint`, `AutoLayout`, `Doc`/`Page`/`File`, `BlendMode`, `Transition`, `migrate` |
| `geom.rs` | outlines (`Shape::polys`), rotation (`xf`/`unxf`/`aabb`), hit test, earcut triangulation, boolean ops (i_overlay) |
| `render.rs` | painting shapes to egui: fills (solid/gradient/image meshes), strokes, shadow, text, grids |
| `layout.rs` | `settle()`: constraints, auto layout (linear, wrap, grid), group refit, text fit, snapping, scale/resize math |
| `canvas.rs` | pointer tools, handles, pen/pencil, node edit, marquee, rulers/guides, context menu, present mode, text editor overlay, blend compositing |
| `panels.rs` | rail, main menu, left panel (pages, layers, assets, history, comments), right panel (design, prototype), toolbar, HUD (hint, toast, help) |
| `glyphs.rs` | icons (Lucide SVGs rasterized by resvg), flat section headers (`collapsing`), small widgets (`menu_item`, `tab_text`, ...) |
| `widgets.rs` | theme (Inter + Inter SemiBold, dark tokens), tool buttons |
| `gallery.rs` | `drui --gallery`: the design tokens and every shared widget rendered, the visual half of docs/DESIGN.md |
| `picker.rs` | Colour picker (square, hue, opacity, value row, swatches, popover placement) |
| `comments.rs` | canvas comments: pins, hover card, composer, thread |
| `fig.rs` | Panel primitives that follow Figma's desktop UI, with the numbers measured from screenshots: rows, headers, labels, grid, number fields, dropdowns, segments, icon buttons, swatches, glyph-centred text (`ptext`) |
| `props.rs` | the Design tab's sections (layer header, Position, Layout, Appearance, Typography, Fill, Stroke, Effects, Layout guide, Page, multi-select, Selection colors) |
| `ops.rs` | clipboard, align/distribute, flip, mask, images, selection colors |
| `vector.rs` | pen tool, node editing, flatten, booleans |
| `comp.rs` | components and instances (instance follows main unless overridden) |
| `export.rs` | SVG builder, PNG (resvg), PDF (svg2pdf), `svg_scene`/`rasterize_scene` for blend compositing |
| `inspect.rs` | Dev Mode: Inspect panel, CSS properties (`shape_css_in`), redlines, padding/gap overlay |
| `anim.rs` | Motion keyframes: evaluate/commit at the playhead, timeline panel, Motion tab |
| `tests.rs` | unit tests and a headless harness (`Harness`) that feeds real pointer events through `App::canvas` |

Key conventions:
- `Shape.pos` is relative to its parent's origin; Vec order is z-order among siblings; containers are `Frame` and `Group` (groups hug their children via `refit_groups`).
- Everything derived (sizes, layouts, group boxes) is computed in `App::settle()`, called every frame and idempotent. New derived geometry belongs there, with epsilon compares so it never jitters (jitter = endless undo entries).
- Undo = full-snapshot of `shapes` per gesture (`pending` in `ui()`).
- Every custom-painted widget must set `resp.widget_info(...)` so it has an accessible name (the REA observer and screen readers need it).
- Child modules may touch `App`'s private fields (they are descendants of the crate root).

