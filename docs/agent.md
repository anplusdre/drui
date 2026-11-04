# Working in drui as an agent

You are editing a design in drui through its local REST API. Read this once, then work.

## Connect

Start drui with `drui --api` (or Actions > Toggle REST API). The URL and token are in `%TEMP%\drui-api.json`
(`{"url": "...", "token": "...", "pid": ...}`). Send `Authorization: Bearer <token>` on every request.
`tools/dr.py` does this for you (it uses `DRUI_API_URL` and `DRUI_API_TOKEN` when they are set, as agent mode sets them, so two drui windows never mix up; in PowerShell pass bodies as `@body.json`): `python tools/dr.py GET /outline`, `python tools/dr.py POST /notes '{"text": "..."}'`.
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

## Compact patches and vectors

A screen is rarely worth listing layer by layer. These forms are expanded into the flat `add` list before the patch is checked and applied, so they
change nothing else (the same guard, one undo step). Use them whenever there are repeated or nested layers.

- **`children`**: nest a layer's children inside it instead of `ref` / `parent_ref`: `{"kind": "Frame", "name": "Card", "children": [{"kind": "Text", "text": "Title"}]}`.
- **`templates` and `use`**: put a layer you repeat in `templates` (next to `add`) and place it with `use`; the entry's own fields win over the template's, and
  its `children` replace the template's.
- **`repeat` and `each`**: `"repeat": {"cols": 4, "dx": 90, "dy": 100}` with `"each": [{...}, {...}]` makes one copy per item of `each`, the first copy at the entry's
  `x` / `y` and the next ones `dx` along and `dy` down (`cols` copies a row; leave `dx` / `dy` out inside an auto-layout frame, which places them itself).
  Without `each`, `"repeat": 8` (or `{"n": 8, ...}`) makes eight plain copies.
- **Variables**: each item of `each` is an object whose keys fill `{key}` in every string of the copy and of its whole subtree (a string that is only `{key}` keeps
  the value's type, so `"fill": "{color}"` and `"radius": "{r}"` work). `{i}` (from 0) and `{n}` (from 1) are the copy's own number, in its own fields only.
  A repeated layer's `ref` becomes `ref_0`, `ref_1`, ...

```json
{"templates": {"tile": {"kind": "Frame", "name": "{label}", "w": 72, "h": 84, "layout": {"vertical": true, "gap": 6},
                         "children": [{"kind": "Ellipse", "w": 48, "h": 48, "fill": "{color}"}, {"kind": "Text", "text": "{label}", "font_size": 12}]}},
 "add": [{"ref": "screen", "kind": "Frame", "name": "Home", "w": 390, "h": 844, "children": [
          {"use": "tile", "x": 16, "y": 120, "repeat": {"cols": 4, "dx": 88, "dy": 100},
           "each": [{"label": "Ride", "color": "#00aa13"}, {"label": "Car", "color": "#ee2737"}, {"label": "Food", "color": "#ffb300"}, {"label": "Send", "color": "#1976d2"}]}]}]}
```

**Vectors.** Draw a shape or an icon as SVG, which is the form you know best:

- `{"kind": "Path", "name": "Heart", "x": 300, "y": 300, "d": "M20 10C20 0 0 0 0 10C0 20 20 28 20 36C20 28 40 20 40 10C40 0 20 0 20 10Z", "fill": "#ff4d6d"}`:
  `d` is SVG path data (`M L H V C S Q T A Z`, absolute or relative, arcs included), drawn from the layer's `x`, `y`; the layer is sized to the drawing. Give it a `fill`,
  or `"fill": null` with a `stroke` and `stroke_w` for a line icon.
- `{"svg": "<svg xmlns=...>...</svg>", "name": "Icon", "x": 340, "y": 20}`: a whole SVG as a frame of vector layers (paths, rectangles, circles, lines and polygons are drawn,
  with their solid and gradient fills and strokes; text, pictures and filters are not).
- Edit a vector afterwards like any layer (`fill`, `stroke`, `x`, `y`, `w`, `h`); to change its shape send a new `d` on a new layer and remove the old one.

## Components and variants

"Make a button component", "build the full set", "turn these into components" mean **one component set with variant properties**, never a pile of
separate components named `button/Primary/Hover`. Frames named like that in the design are a hint: each `/` part is a property
(`Style`, `State`) and each distinct value is one of its values.

- A **variant** is a Frame with `"component": true` and `"variant": [["Style", "Primary"], ["Size", "Medium"], ["State", "Default"]]` (a list of
  `[property, value]` pairs, the same properties and order in every variant). Its children are what it looks like.
- The **set** is a Frame with `"component_set": true` that has the variants as its children (`children`, or `parent_ref`). drui names every variant after
  its values and lays them out in a grid inside the set, so give the variants a size (a fixed `w`/`h`, or auto layout that hugs its content) and no `x`/`y`.
- A **component property** on a layer inside the variants: `"property": "Text"` on a text layer (every instance can change its words),
  `"Bool"` on a layer that instances can hide, `"Swap"` on a nested instance. Put it on that layer in every variant; drui joins them into one property.
- `each` may be an **object of lists**, which makes every combination: `"each": {"style": [...], "size": [...]}`. A list item that is an object adds all its keys
  as variables (`{"size": "Small", "h": 32, "px": 12, "fs": 13}`); a bare value fills `{style}`. Use it for the axes that look the same way
  (Size x Icon) and keep a table with one object per colour combination (`look`: style + state + fill + stroke + text colour), which crossed with the sizes gives every variant.
- Instances: `POST /instances {"main": <variant id>, "parent": <frame id>, "x": 0, "y": 0}`. Do not build a second copy of a component by hand.

**Complete means every axis a designer expects**, not just the ones already drawn. For a button: **Style** (Primary, Secondary, Outline, Ghost, Danger, Neutral,
Warning, Positive), **Size** (Small 32, Medium 40, Large 48), **State** (Default, Hover, Pressed, Focused, Disabled) and **Icon** (None, Left, Right, Only), with a
`Text` property on the label. Take the colours of the styles that exist from the layers (`GET /layers/<id>`) and derive the new ones the same way: Hover about 8% darker,
Pressed about 16% darker, Disabled a washed-out version of Default, Warning amber, Positive green, Neutral grey. Other things (inputs, cards, chips, toggles, tabs) get
the axes that fit them in the same way (Type, Size, State, Selected, Icon...).

Icon variants differ in their children, so write one template per value (`btn_none`, `btn_left`, `btn_right`, `btn_only`; the icon is a small Lucide-style `Path` with
`"fill": null`, a `stroke` and `stroke_w`), and one entry per template with `each` for the rest. Hidden layers still take room in auto layout: leave a layer out instead of hiding it.

```json
{"templates": {
   "btn_none": {"kind": "Frame", "component": true, "name": "Button", "radius": "{r}", "h": "{h}", "fill": "{fill}", "stroke": "{stroke}", "stroke_w": 1,
                "variant": [["Style", "{style}"], ["Size", "{size}"], ["State", "{state}"], ["Icon", "None"]],
                "layout": {"pad": [16, 0, 16, 0], "gap": 8, "main": "Center", "cross": "Center", "hug_y": false},
                "children": [{"kind": "Text", "name": "Label", "text": "Button", "font_size": "{fs}", "fill": "{ink}", "property": "Text"}]},
   "btn_left": {"use": "btn_none", "variant": [["Style", "{style}"], ["Size", "{size}"], ["State", "{state}"], ["Icon", "Left"]],
                "children": [{"kind": "Path", "name": "Icon", "d": "M5 12h14M12 5v14", "fill": null, "stroke": "{ink}", "stroke_w": 2},
                             {"kind": "Text", "name": "Label", "text": "Button", "font_size": "{fs}", "fill": "{ink}", "property": "Text"}]}},
 "add": [{"kind": "Frame", "name": "Button", "component_set": true, "x": 0, "y": 0, "children": [
   {"use": "btn_none", "each": {"look": [{"style": "Primary", "state": "Default", "fill": "#4560ff", "stroke": null, "ink": "#ffffff"},
                                         {"style": "Primary", "state": "Hover", "fill": "#3a50e0", "stroke": null, "ink": "#ffffff"}],
                                "size": [{"size": "Small", "h": 32, "r": 8, "fs": 13}, {"size": "Medium", "h": 40, "r": 10, "fs": 14}]}},
   {"use": "btn_left", "each": {"look": ["... the same list ..."], "size": ["... the same list ..."]}}]}]}
```

After it, read `GET /outline?root=<set id>` (a set's variants are listed with their `variant` values), export the set and look at it, and check that every
combination you meant is there. Do this in a few patches (one per Icon value, or per Style) when the list is long; the set keeps its id, so later patches use `"parent": <set id>`.

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
- **View and preferences.** `GET /view` lists them, `PUT /view {"snap": true, "zoom": 2, "left_tab": "history", "mode": "motion", "guides": [{"vertical": true, "pos": 100}], ...}` sets any of them.
  They change what the person sees (`hide_ui`, `left_min`, `right_min`, `light_ui`, `mode` too): put them back when you are done.
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
