# drui

A vector UI design tool written in Rust with [egui](https://github.com/emilk/egui). Native desktop app (Windows first), no web view.

> **Work in progress.** Most of the editor works and is covered by tests, but the app is still being finished. The checklist below says what is done and what is not.

Frames, auto layout, components and variants, vectors and booleans, prototypes, motion, export. Fonts are [Inter](https://rsms.me/inter/) (OFL), icons are [Lucide](https://lucide.dev) (ISC).

## Build and run

```
cargo run --release                 # opens design.json in the working directory (created on first save)
cargo run --release -- file.json    # open a file
cargo run -- --gallery              # the design tokens and every shared widget (see docs/DESIGN.md)
cargo test                          # logic tests plus headless runs of the whole UI
```

Needs a recent stable Rust toolchain (edition 2024). Windows 10/11 is the platform it is developed and tested on; the rest is plain egui/wgpu and should port, but is not tested elsewhere.

## Status

Legend: ✅ done and covered by a test, 🟡 works with limits, ⬜ not built yet.

### Canvas and editing
- ✅ Frames, groups, nesting, multi-select, marquee, reparenting by drag
- ✅ Rotate / resize / scale handles, snapping, rulers, guides, align / distribute / flip
- ✅ Copy, paste, duplicate, context menu, undo and redo (including pages)
- ✅ Rectangle, ellipse, polygon, star, arc, line, corner radius
- ✅ Pen tool, node editing, flatten, boolean operations, masks (vector, alpha, luminance)
- ✅ Text editing, auto-size, text styles, mixed styles inside one text layer
- ✅ Pencil and brushes (pressure, taper, calligraphy, scatter, stretch brushes)
- 🟡 Eyedropper picks from layers, not from any screen pixel (egui cannot sample the screen)
- ⬜ Text on a path, Scale tool, Slice and Section

### Styling
- ✅ Multiple fills and strokes; solid, linear / radial / angular / diamond gradients, image and video fills
- ✅ Stroke position, dashes, joins, caps, arrowheads, per-side widths, variable-width profiles
- ✅ Drop and inner shadow, layer blur, background blur, glass effect
- ✅ Opacity, blend modes, colour, text and effect styles, selection colors
- ✅ Variables: colour / number / string / boolean, collections, modes, aliases
- 🟡 Glass effect is an approximation
- ⬜ Pattern and shader paints in the colour picker, gradient stop editing in the picker (stops are editable on the canvas)

### Layout
- ✅ Constraints, layout guides (grid, columns, rows)
- ✅ Auto layout: flow, wrap, grid, alignment, padding, gap, Hug / Fixed / Fill, min and max sizes, absolute children
- ⬜ Alignment "More actions" menu, "Show in exports", ready-for-dev toggle

### Components
- ✅ Components, instances, swap, detach, overrides
- ✅ Variants: component sets, variant properties, component properties (boolean, text, instance swap)

### Document
- ✅ Pages, versions, branches, comments with threads
- ✅ Layers panel: tree, drag, rename, search
- ✅ JSON files with migration of older files
- ✅ Export PNG / JPG / SVG / PDF, per-layer export settings, batch export
- ✅ Import SVG
- 🟡 Comments: no attachments, per-message edit / delete or pin dragging

### Prototype and motion
- ✅ Links, triggers (click, hover, press, drag, delay, key), actions (navigate, back, link, overlay, scroll, set variable, change variant)
- ✅ Transitions: dissolve, slide, smart animate; easing curves and springs
- ✅ Present mode and a one-file HTML export of the prototype
- ✅ Motion timeline: keyframes for position, size, rotation, opacity, radius, fill, scale, arc; per-key easing; zoomable
- 🟡 Pixel preview only stores the chosen density
- ⬜ Faster blend compositing (cache per view or repaint only the dirty area)

### Dev mode
- ✅ Read-only inspect: measurements, colors, typography, CSS properties, redlines, padding / gap overlay, asset export

### Shell
- ✅ Rail, menus, toolbar, zoom menu, resizable panels, help sheet, command palette, keyboard shortcuts
- ✅ Shared widget set with a gallery (`drui --gallery`, [docs/DESIGN.md](docs/DESIGN.md))
- ⬜ Verify every panel's measurements against the reference with a pixel diff
- ⬜ Linux and macOS builds are untested

## Not in this repository

These parts of the app are kept out of the public source:

- accounts and subscriptions
- live co-editing between computers
- code export to front-end frameworks
- the local scripting API and plugins

## License

[PolyForm Noncommercial License 1.0.0](LICENSE). You may use, study and change the source for any non-commercial purpose; commercial use and selling it are not allowed. This is *source-available*, not OSI "open source".

drui is an independent project. It is not affiliated with, endorsed by or sponsored by any other company or product. Product and company names mentioned in the source or the docs belong to their owners and are used only to describe compatibility or a point of comparison.
