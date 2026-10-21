# drui

drui is a UI design tool I'm building in Rust. You draw frames, set up auto layout, make components and variants, wire up prototypes and export your work. It runs as a native desktop app (no browser, no Electron) and is built on [egui](https://github.com/emilk/egui).

It started as "can I build a design tool that feels fast and works the way I want?" and it has grown into something that is already useful every day. It is still unfinished, and the checklist below says plainly what works and what doesn't.

Fonts are [Inter](https://rsms.me/inter/) (OFL) and icons are [Lucide](https://lucide.dev) (ISC).

## The plan

My goal is to finish every feature on the checklist below by the **end of October 2026**. Anything not done yet is an open [issue](https://github.com/anplusdre/drui/issues) in the [October 2026 milestone](https://github.com/anplusdre/drui/milestone/1), so you can follow what is left, how far along it is, and what is being worked on next.

## Try it

You need a recent stable Rust toolchain (edition 2024). I develop and test on Windows 10/11. The rest is plain egui/wgpu, so other systems may work, but I haven't tried them.

```
cargo run --release                 # opens design.json in the current folder (created on first save)
cargo run --release -- file.json    # open a file
cargo run -- --gallery              # every colour, size and shared widget on one screen
cargo test                          # logic tests plus headless runs of the whole UI
```

If you want to read the code, start with [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). If you want to change how something looks, read [docs/DESIGN.md](docs/DESIGN.md) first.

## What works today

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
- ✅ Import `.fig` files (pages, layers, fills, strokes, effects, auto layout, text, components, prototype links, variables)
- 🟡 `.fig` import: library components from other files, FigJam-only objects and plugin data are not carried over
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

## What is not in this repository

Some parts of drui are not published here. I plan to sell them, so they stay private:

- **Collaboration.** Live co-editing: several people in one file, with cursors, cursor chat and shared edits.
- **Claude CLI integration.** A local API that lets Claude Code read, debug and edit your design while you watch. It works on copies, never on your originals, and leaves notes saying why it changed things. Plugins and scripting go with it.
- **Export to frontend.** Turn designs into code: React and Next.js apps, HTML and CSS, Tailwind, Kotlin (Jetpack Compose), Swift (SwiftUI), Flutter, and design tokens from your variables. This also includes the Code tab in Dev Mode. (The CSS properties and measurements in Dev Mode, and the one-file HTML export of a prototype, stay free.)
- **Accounts.** The sign-in that switches those features on.

None of this is free. If you want to use it, or you want to talk about licensing it for your team, email me at [anplusdre@gmail.com](mailto:anplusdre@gmail.com), or find me on GitHub as [@anplusdre](https://github.com/anplusdre).

Everything else, which is the whole editor, the prototype player, motion, `.fig` import and PNG/SVG/PDF export, is in this repository.

## License

This code is under the [PolyForm Noncommercial License 1.0.0](LICENSE). In plain words: you can read it, run it, learn from it and change it for personal and other non-commercial use. You can't use it commercially or sell it. That makes it *source-available*, not "open source" in the strict sense. If you need to use it commercially, email me at the address above.

drui is an independent project. It is not affiliated with, endorsed by or sponsored by Figma, Inc. or any other company or product. Figma is a trademark of Figma, Inc. Product and company names that appear in the source or docs belong to their owners and are used only to say what drui can open or to give a point of comparison.
