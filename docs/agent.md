# Working in drui as an agent

You are editing a design in drui through its local REST API. Read this once, then work.

## Connect

Start drui with `drui --api` (or Actions > Toggle REST API). The URL and token are in `%TEMP%\drui-api.json`
(`{"url": "...", "token": "...", "pid": ...}`). Send `Authorization: Bearer <token>` on every request.
`tools/dr.py` does this for you (in PowerShell pass bodies as `@body.json`): `python tools/dr.py GET /outline`, `python tools/dr.py POST /notes '{"text": "..."}'`.
Edits are one undo step each, and the user sees them on the canvas as they happen.

## The one rule: never touch the original

When the user asks you to change a design that already exists:

1. **Duplicate it.** `POST /claude_edit {"id": <frame id>}` copies the frame and everything in it, names the copy
   `<original name> claude_edit`, and puts it beside the original. The reply has the copy's `id`, its `outline`, and `ids`
   (original layer id -> copy layer id). If a copy already exists you get it back (`"existing": true`): keep editing that one.
2. **Edit only the copy** (the frame whose name ends with `claude_edit`, and what is inside it).
   The API answers `403` for a change to any layer outside such a frame, and tells you which copy to make.
3. **Leave notes** that say why you changed things (below).

A design you create from scratch needs no copy: top-level layers you add are yours to edit for as long as drui runs.
Name them sensibly. Never delete or rename the user's own layers.

## Notes: say why

For each change that is not obvious, add a note next to what you changed:

`POST /notes {"text": "Primary button is now 48px tall so it meets the 44px touch target.", "near": <layer id>, "side": "right"}`

- `near` is the layer (usually the changed layer or the claude_edit frame); `side` is `right` (default), `left`, `top` or `bottom`.
  Without `near`, give `x` and `y` (canvas coordinates), or leave both out to put the note right of everything.
- Optional: `width` (default 220), `color` (`"#ffe08a"`).
- Notes follow the layer named by `near` when it moves, and stack so they never overlap, whatever height their text takes. Keep each to one or two sentences: what changed, and why.
- A final note on the claude_edit frame summarising the whole edit is good practice.
- To remove a note: `DELETE /layers/<note id>`.

## Reading

- `GET /outline` is the cheap way to see the design: every top-level layer with its children, one small record per layer
  (`id, name, kind, x, y, w, h`, plus `fill`, `stroke`, `text`, `font_size`, `layout`, `component`, `instance` when set).
  `?root=<id>` limits it to one subtree, `?depth=1` to one level. Top-level layers carry `editable`.
  `x, y` are relative to the parent (canvas coordinates for top-level layers), the same as in patches.
- `GET /layers/<id>` is the full record of one layer: use it when the outline lacks a property you need.
  `GET /layers` returns everything and is large; avoid it on big files.
- `GET /export/<id>?format=png&scale=2` renders a layer to look at your result. Do this after a batch of edits.
- `GET /variables`, `GET /comments`, `GET /pages`, `GET /selection` are also there.

## Writing

`POST /patch` applies several changes at once, as one undo step:

```json
{"set":    [{"id": 12, "fill": "#0a84ff", "radius": 8, "w": 160, "h": 48},
             {"id": 13, "text": "Sign in"}],
 "add":    [{"ref": "card", "kind": "Frame", "name": "Card", "x": 0, "y": 0, "w": 300, "h": 120, "fill": "#ffffff",
             "layout": {"vertical": true, "gap": 8, "pad": [16, 16, 16, 16]}},
            {"parent_ref": "card", "kind": "Text", "text": "Title", "font_size": 20}],
 "remove": [14]}
```

- `set` merges the given fields into a layer. `add` creates layers; `ref` / `parent_ref` link a new child to a parent made in the
  same patch, `parent` takes an existing layer id (it must be inside a claude_edit frame). Order: remove, set, add.
- Shorthands: `x y w h` (position and size), `radius` (one number for all corners), `fill` and `stroke` (`"#rrggbb"`,
  `"#rrggbbaa"` or `null` for none). Everything else uses the file's field names exactly as `GET /layers/<id>` shows them
  (`fills`, `strokes`, `stroke_w`, `font_size`, `opacity`, `layout`, `effects`, ...). Nested values are replaced whole.
- `kind` is one of `Frame Group Rect Ellipse Line Text Polygon Star Path`.
- Prefer auto layout (`"layout": {...}`) over hand-placed coordinates for anything that is a list, row, stack or form.
- Reuse existing components and styles instead of drawing new ones; do not detach instances.
- `PATCH /layers/<id>`, `POST /layers` and `DELETE /layers/<id>` do the same for one layer.

## Everything else the app can do

- **Actions.** `GET /actions` lists the commands of the Actions palette (Group, Create component, Union, Flip, Auto layout, Zoom to fit, ...).
  `POST /actions {"name": "Group selection", "ids": [12, 13]}` runs one on those layers (`ids` becomes the selection). Three take a value:
  `{"name": "Offset path", "value": 8}`, `{"name": "Rename", "pattern": "Button #"}`, `{"name": "Replace", "from": "a", "to": "b"}`.
  Like writes, they answer `403` for layers outside a claude_edit copy. Actions that need the person at the keyboard (file dialogs, the pointer tools) are not listed.
- **Arrange.** `POST /align {"op": "left|hcenter|right|top|vcenter|bottom", "ids": [..]}`, `POST /distribute {"horizontal": true, "ids": [..]}`.
- **Undo.** `POST /undo`, `POST /redo` (every API edit is one step; this also undoes the person's own steps, so use it for your own).
- **Pages.** `POST /pages {"name": "Login v2"}` adds a page and opens it, `PUT /pages/<i>` opens page `i`, `DELETE /pages/<i>` deletes an empty page. `GET /pages` lists them.
- **Files.** `POST /save {"path": "x.json"}` (no body: the open file), `POST /open {"path": "x.json"}`, `POST /import {"path": "x.fig"}` (also `.sketch`).
  Open and import answer `409` while the document has unsaved edits, unless the body has `"discard": true`; do not send that unless the person said so.
- **Pictures and instances.** `POST /images {"path": "x.png", "x": 0, "y": 0}` (or `{"data": "<base64>", "name": "x.png"}`) places a picture as a rectangle;
  `"place": false` only stores it and returns the asset id for a fill (`{"kind": {"Image": <asset>}}`). `POST /instances {"main": <component id>, "parent": <frame id>, "x": 0, "y": 0}` places an instance.
- **Copies.** `POST /pages/<i>/duplicate`; `POST /copy {"ids": [..]}` and `POST /paste` use the app's own clipboard, never the person's.
- **Styles.** `GET /styles` lists color, effect, text, grid and animation styles; `POST /styles/color {"name": "Brand", "color": ...}` (kinds: `color effects texts grids anims`)
  adds one; point your layers at it (`style` on a fill or stroke, `effect_style`, `text_style`, `grid_style`).
  `PATCH /styles/<kind>/<id>` and `DELETE` change or remove an existing one, which restyles every layer that follows it, so they need `"confirm": true`: only when the person asked.
- **Versions.** `GET /versions`, `POST /versions {"name": "Before the redesign"}` saves the open page: do this before a big edit.
  `POST /versions/<i>/restore {"confirm": true}` replaces the open page with that version: only when the person asked.
- **Branches.** `GET /branches`, `POST /branches {"name": ...}` makes one and opens it, `PUT /branches/<i>` opens one, `POST /branches/<i>/review {"reviewer": "name"}` asks for a review.
  `PUT /branches/<i>/review {"state": "requested", "reviewer": "name"}` hands the request on, `{"state": "withdrawn"}` takes it back.
  Merging (`POST /branches/<i>/merge`) needs an approved review, which only the person can give. `DELETE` works on a branch that is not open and has no changes.
- **Libraries.** `GET /libraries`, `POST /libraries {"path": "x.drlib"}` adds one, `POST /libraries/<id>/update`, `POST /libraries/publish {"path": "x.drlib"}`.
- **View and preferences.** `GET /view` lists them, `PUT /view {"rulers": false, "snap": true, "zoom": 2, "left_tab": "history", "mode": "motion", "guides": [{"vertical": true, "pos": 100}], ...}` sets any of them.
  They change what the person sees (`hide_ui`, `left_min`, `light_ui`, `mode` too): put them back when you are done.
- **Motion.** `GET /motion`, `PUT /motion {"time": 500, "length": 3000, "loop": true, "play": false}` moves the playhead (ms) and sets the length. In Motion mode the page shows that frame and edits become keyframes.
  `play: true` plays it, with the page's sound layers.
- **Pointer tools by coordinates** (canvas coordinates, on the layers named in `ids`; they cannot touch the person's own designs):
  `POST /tools/cut {"ids": [..], "from": [x, y], "to": [x, y]}`, `POST /tools/erase {"ids": [..], "points": [[x, y], ...]}`,
  `POST /tools/builder {"ids": [..], "merge": [[x, y]], "delete": [[x, y]]}` (Shape builder: a point inside a region picks it) and `POST /tools/paint {"ids": [..], "merge": [[x, y]]}` (fills the region).
- **Present.** `POST /present {"id": <frame>}` shows the prototype on the person's canvas, `DELETE /present` leaves it. Clicking through it is not possible from here.
- The prototype view has no routes of its own: interactions are layer fields, so a patch can set them.
- The API answers at most 30 requests a second (`429` beyond that): send one `POST /patch` for a batch of edits, not one call per layer.

## Workflow

1. `GET /outline` to find the frame. `POST /claude_edit` on it.
2. Make the changes in one or a few `POST /patch` calls.
3. `GET /export/<copy id>?format=png` and look at it. Fix what is off.
4. Add notes with `POST /notes`. Tell the user which frame to look at and why you changed what you changed.

`DRUI_API_UNGUARDED=1` in drui's environment turns the claude_edit check off. Do not ask for it.
