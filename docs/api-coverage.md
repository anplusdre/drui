# REST API coverage

What a script or Claude Code can do through the local REST API (`drui --api`, see `agent.md` for how to use it), against what the app can do.
The routes are listed by `GET /` and explained in `GET /guide`.

## Covered

| App function | Route |
|---|---|
| Layers: create, edit, delete, read, outline, batch patch; every layer property (fills, strokes, effects, auto layout, constraints, text runs, prototype interactions, dev status, ...) | `/layers`, `/patch`, `/outline` |
| Selection | `/selection` |
| Every Actions palette command (group, ungroup, frame, auto layout, component, detach, mask, flatten, booleans, flip, z-order, tidy up, select all/inverse/matching, hide, lock, copy/paste properties, duplicate, resize to fit, text on path, outline stroke, simplify, zoom, view toggles, ...) | `GET /actions`, `POST /actions` |
| Offset path, rename, find and replace | `POST /actions` with `value`, `pattern`, `from`/`to` |
| Align, distribute | `/align`, `/distribute` |
| Copy, paste, duplicate page | `/copy`, `/paste`, `/pages/{i}/duplicate` |
| Undo, redo | `/undo`, `/redo` |
| Pages: add, open, delete when empty | `/pages` |
| Save, open, import `.fig` and `.sketch` | `/save`, `/open`, `/import` |
| Export a layer or the page as SVG, PNG, PDF | `/export` |
| Pictures; component instances | `/images`, `/instances` |
| Variables (collections, modes, values) | `/variables` |
| Styles: color, effect, text, grid, animation (add and read) | `/styles` |
| Comments, replies, resolve | `/comments` |
| Notes beside a layer; edit copies of designs | `/notes`, `/claude_edit` |
| Version history: save, list, restore | `/versions` |
| Branches: create, open, ask for review, merge when approved, delete when unchanged | `/branches` |
| Libraries: add, update, publish | `/libraries` |
| The whole document as JSON | `GET /doc` |
| Preferences and view state: panel tab, snapping, rulers, pixel grid and preview, guides, zoom, mode, color profile, nudge, mirror handles, labels | `GET/PUT /view` |
| Motion timeline: playhead, length, loop, play | `GET/PUT /motion` |
| Cut, eraser, shape builder, paint, driven by coordinates | `POST /tools/{cut,erase,builder,paint}` |
| Present and leave it | `POST/DELETE /present` |
| Edit or delete an existing style | `PATCH/DELETE /styles/{kind}/{id}` (needs `confirm`) |
| Hand on or withdraw a branch review | `PUT /branches/{i}/review` |

## Left out on purpose

- **Changing or deleting an existing variable collection, page with layers, or branch with changes**: it would change what the person made. Styles can be edited with `confirm`. `PUT /variables` replaces the variables and is the one exception that predates this rule; use it with care.
- **Approving or sending back a review**: only the person can.
- **Restoring a version, opening or importing over unsaved work**: allowed only with an explicit `confirm` / `discard` in the request.
- **Tools that follow the pointer** (pen, shape builder, cut, eraser, paint, eyedropper), **Present**, **Quick insert** and **file dialogs**: they need the person at the keyboard. Their results are reachable another way: a patch, `/import`, `/images`.
- The person's **system clipboard**: `/copy` and `/paste` use the app's own.

## Not covered

- The pen and pencil by drag: draw a path with a patch (`kind: Path`) instead.
- Clicking through a running presentation (the prototype's interactions run only for the person at the keyboard).
- Panel layout that is not a setting: panel widths, scroll positions, the pan of the canvas (zoom is covered).
- Native file dialogs: `/open`, `/import`, `/images` and `/libraries` take a path.
- Per-layer export settings and slices as routes of their own (they are layer fields, so a patch sets them).
- The shader effect library browser.

`cargo test` has one test per group of routes (`rest_api_*` in `src/tests.rs`).
