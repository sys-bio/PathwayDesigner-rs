# PathwayDesigner in Rust + egui — Implementation Specification

| | |
|---|---|
| **Companion document** | [`SPECIFICATION.md`](./SPECIFICATION.md) is the functional specification and says *what* the application does. This document says *how* to build it in Rust with egui. References to the functional spec look like **(S §n)**. |
| **Precedence** | Where the two documents disagree on behaviour, SPECIFICATION.md wins. This document adds decisions about technology, architecture, platforms and delivery. |
| **Status** | Plan for future work. Nothing is implemented yet. |

### Decisions taken

| Decision | Choice | Consequence |
|---|---|---|
| Language and UI | **Rust**, with **egui** through **eframe** | One code base can build a browser app (WebAssembly) and a native desktop app (Windows, macOS, Linux). |
| Delivery order | **Browser first, desktop later** | Milestones R0–R12 deliver the browser version. A separate milestone D1 (§15.1) delivers the desktop version from the same code. Until then, the code is kept desktop-ready: CI builds it natively on every push (§14.5). |
| Antimony and SBML | **Pure Rust**: our own Antimony parser and SBML reader/writer | The same code runs on desktop and in the browser. No C++ toolchain, no FFI and no JavaScript libraries. In exchange, we write and maintain the parsers ourselves (§8). |
| Graphics export | One SVG writer. **resvg** turns it into PNG and **svg2pdf** turns it into PDF | Vector PDF and SVG on every platform, all in pure Rust (§9). |
| File format | **Fully compatible with native JSON v3** (S §11.1), including reading v1 and v2. **From v4 onwards the Rust app owns the format** | The Delphi app is frozen and will not move past v3. Any v3 file opens in either app. New features that need new keys go into v4, which only the Rust app reads (§4.2). |
| Back end | **None**, on any target | No accounts or server. In the browser, no user data leaves the page. |
| Web hosting | **GitHub Pages** from this repository, built with **trunk** | Static files only, served from a fixed sub-path, with no custom HTTP headers (§14). |
| Threads | **No worker threads** in shared code | Long tasks run in slices spread over several frames (§7.2). This runs the same way natively and in a browser, where WebAssembly has no threads unless the page sends cross-origin headers that GitHub Pages cannot send. |

---

## 1. Goals

1. **Behaviour matching reference v0.38** as defined by SPECIFICATION.md: the same
   drawing, editing, layout, styling and interchange. Every deviation listed in S §15
   is fixed, not copied.
2. **Browser first, with the desktop ready to follow.** The first release runs in the
   browser. A desktop version for Windows, macOS and Linux follows later from the same
   crates (milestone D1, §15.1), with no redesign. Platform-specific code is limited
   to file access, preferences storage and printing (§10). Every crate builds natively
   from the start, so the desktop version stays possible.
3. **v3 compatibility is a hard constraint.** A v3 file saved by either the Delphi app
   or the Rust app opens in the other with the same geometry, styles and meaning.
   Beyond v3 the Rust app evolves the format on its own (§4.2).
4. **No installation needed on the web.** It is a static site on GitHub Pages. Open the
   URL and draw. Deploying is a `git push` (§14).
5. **Close the cheapest of the reference's gaps:** SBML import, including layout import
   (S §16.2 F-1), and drawing compartments (F-2).

### Non-goals

Simulation. SBGN compliance. Accounts, cloud storage, links that share a model,
and collaborative editing. Tabs with several documents (S §16.2 F-9 stays future work).
Editing designed for phones first: touch MUST support pan, zoom and select, but full
editing assumes a mouse or trackpad and a keyboard.

---

## 2. Architecture

The functional spec requires the layering in S §14. In Rust it is enforced by
**crate boundaries**: a crate cannot use a dependency it doesn't declare.

```
PathwayDesigner/                (the repository root is the Cargo workspace, §14.5)
  crates/
    pd-core/                    no egui, no I/O, no platform code, builds for wasm32 and native
      geometry/                 S §17.3 — vectors, distances, border intersection, arrowheads,
                                transforms, Bézier evaluation and splitting, border-crossing search
      model/                    S §3, §4 — types, operations, id counter
      json/                     S §11.1 — read v1, v2 and v3, write v3, colour codec
      expr/                     S §8.3 — lexer, parser, syntax tree, list of identifiers
      undo/                     S §9 — commands and history
      layout/                   S §10 — force-directed layout (run in steps) and the Deckard handle pass
      antimony                  S §11.2, §11.3 — the bridge: maps pd-antimony's description
                                onto the model (import) and back (export)
      sbml/                     S §11.5 — L3V1 core + Layout writer, MathML; reader (F-1)
      text/                     text width from the bundled font (§5.4)
      view/                     view state, hit testing, the interaction state machine,
                                editing commands (S §5–§7)
      render/                   the Renderer trait and the drawing passes (S §5.2),
                                plus SvgRenderer (§9)
    pd-antimony/                Antimony text ⇄ a plain description of a model. No other
                                dependencies and nothing about PathwayDesigner, so a complete
                                Antimony crate can replace it (§8.2)
    pd-export/                  turns SVG into PNG (resvg) and PDF (svg2pdf). No egui. Builds for wasm32 too
    pd-app/                     eframe/egui shell: menus, toolbars, inspector, panels,
                                dialogs, EguiRenderer, platform adapters (§10)
  assets/fonts/                 the one bundled UI and diagram font (§5.4)
```

**Rules:**

1. `pd-core` MUST NOT depend on `egui`, `eframe`, `web-sys` or any platform crate. Its
   whole test suite runs with a plain `cargo test`, with no window and no browser.
   It also MUST NOT read the clock, because `std::time::Instant` panics in the
   browser. Timing lives in `pd-app` (§7.2).
2. Inside `pd-core`, `geometry` depends on nothing. `model` depends only on `geometry`.
   `view` and `render` are the only modules that know about the screen (zoom and
   scroll), and they never own UI widgets.
3. `pd-app` is the only crate that turns egui input into `view` events and `view`
   output into egui drawing (§2.1).
4. Every crate in the workspace MUST build for both the native target and
   `wasm32-unknown-unknown`. CI checks both on every push (§14.5).

### 2.1 How the app shell and the view connect

egui is an immediate-mode GUI: the whole interface is rebuilt every frame. The view
is still an **ordinary struct that keeps its state between frames**. It is the Rust
version of the reference's view object (S §14).

```rust
pub struct DiagramView {
    pub model: Model,
    zoom: f32,                 // S §5.1, range 0.1–8.0
    scroll: Vec2,              // screen px, in egui points
    state: InteractionState,   // S §6.1
    drag: Option<DragScratch>, // originals saved when a drag starts (S §6.4)
    undo: UndoHistory,         // S §9
    hover: Option<HoverInfo>,  // S §5.9 tooltip
    // ...
}

impl DiagramView {
    pub fn handle(&mut self, ev: &ViewEvent) -> ViewResponse;  // pointer, key, wheel
    pub fn draw(&self, r: &mut dyn Renderer, opts: &DrawOptions);
    pub fn selection(&self) -> SelectionInfo;                  // S §6.3
    // editing commands: align, set Bézier, make nice, rename, … (S §7)
}
```

- **Events in.** Each frame, `pd-app` turns egui input into a small enum of its own,
  `ViewEvent`: pointer down, move and up with the button and modifiers, wheel, pinch
  zoom, key, double-click. These events use `pd-core` types only. This lets the
  interaction state machine be tested without egui by feeding it made-up events.
- **What comes back.** `ViewResponse` says what happened: whether a repaint is needed,
  whether the selection changed, and requests such as "open the context menu for this
  target" or "ask the user for a new name". Requests that need a dialog are handled by
  the app shell, which then calls the matching `DiagramView` method.
- **Drawing.** `draw` uses the `Renderer` trait (§5.1). The app shell passes an
  `EguiRenderer` that wraps the canvas area's `egui::Painter`.
- **No state is copied into the UI.** Panels read the selection, the model and
  `selection()` again every frame. They never keep their own copy of model data. The
  inspector only keeps text that is still being typed (§11.2).

---

## 3. Domain model

This is S §3 written as Rust types. Field names, meanings and defaults (species
80 × 36, JSON version 3) are unchanged. Two things change how data is represented,
not how it behaves.

**All numbers in the model are `f64`**: positions, sizes, handles, widths, font sizes,
values. JSON numbers are `f64`, so the model can hold every value in a file exactly
and the round-trip test (§4.1) can pass. `f32` exists only on the screen side: the
drawing passes convert to `f32` screen coordinates when they call the `Renderer`
(§5.1).

```rust
pub type Id = String;

#[derive(Clone, Copy, PartialEq)]
pub struct Pt { pub x: f64, pub y: f64 }   // world coordinates

#[derive(Clone, Copy, PartialEq)]
pub struct Rgba(pub [u8; 4]);          // straight (not premultiplied) RGBA; 0 = "use the default" (S §3.6)

pub struct VisualStyle {
    pub has_custom_style: bool,
    pub fill_color: Rgba, pub border_color: Rgba, pub label_color: Rgba,
    pub border_width: f64, pub line_color: Rgba, pub line_width: f64,
    pub font_size: f64, pub junction_color: Rgba, pub junction_visible: bool,
}

pub struct SpeciesNode {
    pub id: Id, pub center: Pt, pub width: f64, pub height: f64,
    pub initial_value: f64, pub is_boundary: bool, pub is_constant: bool,
    pub compartment: String,
    pub alias_of: Option<Id>,           // an id, not a reference (§3.1)
    pub is_null_node: bool, pub locked: bool, pub selected: bool,
    pub style: VisualStyle,
}

pub struct Participant {
    pub species: Id,                    // an id, not a reference
    pub stoichiometry: f64,
    pub ctrl1: Pt, pub ctrl2: Pt, pub ctrl_pts_set: bool,
}

pub enum ReactionMode { Straight, Linear, Bezier }   // replaces isLinear/isBezier (§3.2)

pub struct Reaction {
    pub id: Id, pub junction: Pt,
    pub reactants: Vec<Participant>, pub products: Vec<Participant>,
    pub modifiers: Vec<Participant>,
    pub kinetic_law: String, pub is_reversible: bool,
    pub mode: ReactionMode, pub junction_smooth: bool,
    pub selected: bool, pub style: VisualStyle,
}
```

The style holds `Rgba`, never `egui::Color32`. `Color32` stores premultiplied alpha
and belongs to the UI layer. The app shell converts colours when it draws.

### 3.1 References are ids, not pointers

The reference uses object references for aliases and participants, and has to be
careful to make undo work by id (S §9.2). Here **everything refers to things by id**,
so that rule holds automatically:

- a restored snapshot is simply a new `Model`, and every reference in it still works;
- the borrow checker is never a problem, because there are no references that point
  from one object to another inside the model.

`Model` keeps `species_index: HashMap<Id, usize>` and
`reaction_index: HashMap<Id, usize>`. These are rebuilt after loading or a structural
change, and kept up to date on renames. Every lookup MUST go through them. The draw
loop must never search a list one by one.

Species stay in a `Vec` because **list order is z-order** (S §3.1). Deleting a
species therefore rebuilds the index, which is cheap at the target sizes (§6).

**Iterate in `Vec` order only.** The indexes are for looking things up, never for
looping over. `std::collections::HashMap` iterates in a different random order on
every run. Any loop over it in layout, export, drawing or id generation would make
results change from run to run, break seeded tests (§12) and stop bug reports from
being reproducible. If a map that is looped over is ever needed, use `IndexMap` or
`BTreeMap`.

### 3.2 One `mode` instead of two flags

The reference stores `isLinear` and `isBezier` as two flags that must never both be
true (S §3.4). The Rust model has one `ReactionMode` value, so that invalid state
cannot exist. The JSON code converts in both directions:

| JSON | `mode` |
|---|---|
| `isBezier: true` (wins if both are somehow true) | `Bezier` |
| `isLinear: true` | `Linear` |
| neither | `Straight` |

When writing, both flags are written, from `mode`. A `Linear` reaction that is not
UniUni is drawn as `Straight` (S §5.5) but keeps `Linear` in the file.

### 3.3 Changing the model

The model is plain mutable data, changed directly by the view. It is not immutable
and nothing compares old and new versions. egui redraws from it every frame. This
matches the reference and avoids copying the network on every drag frame.

---

## 4. File format — native JSON

**The rules are in S §11.1.** This section only adds what is specific to Rust.

- Use **serde** with separate *file structs* that match the JSON exactly: key names,
  which keys are left out at their default, `ctrl*` keys only when `ctrlPtsSet`, and
  modifiers that have only an `id`. Convert between these structs and the model types
  in a separate step. Do **not** put `#[derive(Serialize)]` directly on the model
  types. The file format is a contract, and it must not change because someone
  renamed a field in the model.
- **Colours:** read `#RRGGBBAA` or `#RRGGBB` with or without the `#`, and the old
  signed-integer `0xAARRGGBB` form (S §11.1). The integer form is not limited to
  old files: `Models/model9.json` says version 3 and still uses it. Always write 8
  hex digits with a `#`, in **upper case**, as the Delphi writer does
  (`#FFE0B2FF`).
- **What the Delphi writer does, and the Rust writer copies:** keys in the order of
  S §11.1; style objects with all nine keys, `junctionVisible` included; a
  `modifiers` array on every reaction, even when it is empty. The `locked` flag is
  written as `"locked": true`, and only when true. Neither S §11.1 nor any sample
  file shows its key, so this is our choice; check it against the Delphi source
  if that becomes available.
- **Versions:** read 1, 2 and 3 (and later versions as they are added, §4.2). A
  missing `version` means 1. Until v4 exists, always write 3 and never write a key
  the Delphi app doesn't know. Settings that only matter to the app (last
  zoom, panel widths, recent files) are stored in preferences (§10.3), not in the
  document.
- **Newer files (`version` > 3).** In practice these will come from a newer build of
  the Rust app, for example a v4 file opened by an older desktop build or by a
  browser tab still running a cached older version (§14.2). Open them as far as possible and ignore keys
  that aren't recognised. Then warn: "This file was made by a newer version of
  PathwayDesigner. Saving it here will lose features this version doesn't support."
  For that document, **Save behaves as Save As**, so the original file can't be
  overwritten by accident and lose the newer data without anyone noticing.
- Write the text as UTF-8 with 2-space indentation. Accept a byte-order mark when
  reading. `serde_json` writes and reads `.` as the decimal separator on every
  platform, which satisfies S §14 with no extra work.
- **Fix D-8:** always write `junctionVisible` when it is `false`, even if the reaction
  has no other custom style. That means writing a `style` object for the reaction.
  Set every colour and width in it to 0. The Delphi app treats any `style` object as a
  custom style, but for reactions it uses the default for every field that is 0, so it
  still draws the reaction with its default line and junction.

### 4.1 Round-trip tests

`Models/*.json` in the repository are the reference files. For every file, CI MUST
check:

1. Load, save, and compare the result with the original **by content**: key order and
   number formatting may differ, and numbers may differ by up to 1e-9.
2. Load, draw with `SvgRenderer` at zoom 1, turn it into PNG with resvg, and compare
   with a stored image with a small tolerance per pixel (§12). This is reliable because
   the whole path is pure Rust and uses the bundled font.

If test 1 fails for any file, the release is blocked.

### 4.2 Versions after v3

The Delphi app stays at v3. From v4 onwards the Rust app owns the format.

- **Additive only.** A new version only adds optional keys, with defaults that
  reproduce the old behaviour. It never renames, removes or changes the meaning of an
  existing key. A v4 reader therefore reads v1–v3 with no special cases.
- **Write the lowest version that fits.** When saving, use v3 unless the document
  actually uses a v4 feature (for example a non-default node shape). Most files
  therefore still open in the Delphi app.
- **Say so when compatibility is lost.** The first time a document starts needing v4,
  the status bar says "This file now needs PathwayDesigner (Rust) to open". It is
  not a dialog.
- **Write it down first.** Each new version's keys, defaults and meaning are added
  to this section before the feature is released. The round-trip tests (§4.1) get v4
  sample files.

---

## 5. Rendering

### 5.1 The `Renderer` trait

Every drawing pass in S §5.2 draws through one small trait. That way the screen,
SVG, PNG and PDF all come from the same code:

```rust
// SPt { x: f32, y: f32 } and SRect are screen-space types, separate from the model's f64 Pt (§3)
pub trait Renderer {
    fn round_rect(&mut self, r: SRect, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>);
    fn circle(&mut self, c: SPt, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>);
    fn polyline(&mut self, pts: &[SPt], stroke: Stroke);            // Stroke carries width, colour, cap, dash
    fn cubic(&mut self, p0: SPt, c1: SPt, c2: SPt, p3: SPt, stroke: Stroke);
    fn polygon(&mut self, pts: &[SPt], fill: Rgba);
    fn text(&mut self, center: SPt, s: &str, size: f32, color: Rgba); // centred on cap height (S §5.3)
}
```

There are two implementations:

- **`EguiRenderer`** (in `pd-app`) builds `egui::Shape`s on the canvas `Painter`.
- **`SvgRenderer`** (in `pd-core`) writes SVG markup (§9).

Keep the trait this small. Every method added is one more method both
implementations must get right.

All coordinates passed to the renderer are **screen coordinates in `f32`** (the
`SPt`, `SRect`, sizes and widths in this trait are screen-side `f32` types, not
the model's `f64` ones, §3). The drawing passes do their arithmetic in `f64` and
convert to `f32` at the call. They apply `screen = world × zoom + scroll` (S §5.1) themselves, point by point,
and scale stroke widths and font sizes by zoom. They do this rather than handing the
renderer a transform, so stroke widths and hit tolerances (given in screen pixels)
stay under the drawing code's control, as in the reference.

### 5.2 Notes on drawing with egui

| Requirement | How to meet it in egui |
|---|---|
| Dashed straight lines: modifier arcs, guide lines, handle guides (S §5.7, §5.9) | `Shape::dashed_line` on a polyline |
| **Dashed rounded rectangles**: alias borders, the selection halo (S §5.3) | egui has no dashed outline for rounded shapes. Build the outline as a polyline (each corner arc made of about 8 segments) and dash it with `Shape::dashed_line`. Use the same polyline in `SvgRenderer`, as `stroke-dasharray` on a `<rect rx>`. |
| Cubic Bézier legs (S §5.6.3) | `CubicBezierShape` with no fill |
| Round line ends | Draw a small filled circle at each end of a wide line. egui strokes don't support caps. Below about 2 px wide this can be skipped. |
| Arrowheads, null-node circles, rounded rectangles | `Shape::convex_polygon`, `CircleShape`, `RectShape` |
| Text | Lay out the text once with `painter.layout_no_wrap` using the bundled font (§5.4), then position it so it is centred on its cap height |
| Clipping | Clip the painter to the canvas area. Tooltips are drawn on the canvas, not as egui tooltip windows (S §5.9) |

**Screen pixels are egui points.** egui handles high-DPI screens (`pixels_per_point`).
The constants in S §17.1 that are given in screen px are given in points here.

**Repainting.** eframe repaints when there is input. The view returns
`needs_repaint`. Hover tooltips need a timed repaint, so call
`ctx.request_repaint_after(600 ms)` when the pointer stops moving (S §6.6). Layout
animation requests a repaint every frame while it runs (§7.2).

### 5.3 Constants and colours

Copy S §17.1 and S §17.2 exactly into `pd-core/src/render/constants.rs`. Do not
change them, or diagrams won't match the reference image for image. This includes
the 40-colour picker palette, which MUST keep every default diagram colour.

### 5.4 Text and the bundled font

Text width affects saved geometry: renaming a species widens its node to fit the
label (S §5.3), and that width is saved as `w`. It MUST therefore come out **the
same on every platform**:

- Bundle **one** font, **Noto Sans Regular** (SIL Open Font Licence), in
  `assets/fonts/`. Use it for diagram labels in egui, in SVG (embedded
  through font data given to `usvg`), in PNG and in PDF.
- **Measure** label width in `pd-core::text` from that font's own data, not with
  egui. Then the width is the same in a headless test, a desktop build and a browser.
  Shape the text with **harfrust**, the shaper egui 0.36 uses, so GPOS kerning is
  applied exactly as egui draws it. (`ab_glyph` only reads the old `kern` table, so
  its widths would differ from the screen.) A test in `pd-app` checks that the two
  agree to within 1 point.
- Node widths after auto-widening may still differ by a pixel or two from the Delphi
  app, which uses Skia and a system font. That is acceptable, because it is a
  difference in measurement, not in the method.

### 5.5 Bézier legs

Implement S §5.6 exactly:

- **Handle convention (S §5.6.1).** On a reactant leg, `ctrl1` is the outer handle and
  `ctrl2` the inner one. On a product leg, `ctrl1` is the inner handle and `ctrl2` the
  outer one.
- **Automatic handles (S §5.6.2).** The fan offset is applied at the junction end,
  and the leg's index counts **the legs of the same role on the reaction**. It does
  not count the legs on a species node (S §15, D-17).
- **Clipping (S §5.6.3).** A 24-step bisection on `t`, a de Casteljau split, and the
  start or end moved back along the tangent by the gap, with the arrowhead tip placed
  separately at the 3-unit inset.

No border point is ever saved.

### 5.6 Performance budget

| Measure | Target |
|---|---|
| Frame time while dragging, 200 species / 300 reactions, in Chromium | ≤ 16 ms |
| Frame time while dragging, 1 000 species / 1 200 reactions | ≤ 33 ms |
| Web cold load until usable, cached | ≤ 1.5 s |
| Auto-layout, 800 iterations, 200 nodes | the UI stays responsive throughout (§7.2). Total time ≤ 3 s in the browser |
| *Desktop (D1):* cold start until usable | ≤ 1 s |
| *Desktop (D1):* auto-layout, 800 iterations, 200 nodes | ≤ 1.5 s |

Try these techniques in this order:
1. Skip drawing passes that have nothing to draw.
2. Cache label layouts (egui already caches text layout).
3. Leave out shapes that are outside the visible area.
4. Only then, consider drawing to an off-screen texture and redrawing only the parts
   that changed. Don't start with this one.

---

## 6. Interaction

The state machine, hit-test order, selection rules, drag rules and creation rules
are all S §6 exactly. This section only covers how they map onto egui.

### 6.1 Mapping egui input

| S requirement | egui |
|---|---|
| Canvas input | `ui.allocate_rect(canvas, Sense::click_and_drag())`. While a drag started on the canvas is in progress, egui keeps sending its events to the canvas even after the pointer leaves it. That does the job of pointer capture. |
| Left and right button, double-click | `PointerButton::Primary` / `Secondary`; `Response::double_clicked()` |
| Shift, Ctrl/Cmd | `Modifiers::shift` and `Modifiers::command`. `command` is Cmd on macOS and Ctrl elsewhere, so use it for all shortcuts. |
| Ctrl/Cmd + wheel zoom, kept on the cursor (S §6.6) | `InputState::zoom_delta()`. This also covers pinch on a trackpad or touch screen. Turn its value into steps of ×1.15 per wheel notch, or use the continuous factor for pinch. Either way clamp to [0.1, 8] and keep the world point under the cursor fixed. |
| Plain wheel or two-finger scroll pans (S §6.6) | `InputState::smooth_scroll_delta` |
| Other ways to pan | **Middle-button drag** and **Space + drag**. These replace the reference's fixed scrollbars (S §15, D-5). Scrollbars are optional and, if shown, are sized from the content bounds. |
| Hover tooltip after 600 ms (S §6.6) | Record when the pointer last moved. When it has been still for 600 ms over an item, show the tooltip and request a repaint. |
| Esc cancels a drag or mode | Also cancel when the window loses focus in the middle of a drag (`InputState::focused` becomes false). This replaces the "pointer cancelled" event a browser would send. |
| Touch | One finger pans when the touch didn't start on an item. Two fingers pinch-zoom. A tap selects. |

**Keyboard shortcuts.** Use `Context::input_mut(|i| i.consume_shortcut(..))` so a
text field that has the keyboard focus gets its normal editing keys first.
Shortcuts that affect the diagram only apply while the canvas has focus or no text
field is being edited:

| Keys | Action |
|---|---|
| Delete / Backspace | Delete the selection |
| Esc | Cancel |
| Cmd+A | Select all |
| Cmd+Z | Undo |
| Cmd+Shift+Z, Ctrl+Y | Redo |
| Cmd+S / Cmd+Shift+S | Save / Save As |
| Cmd+O | Open |
| Cmd+N | New |

### 6.2 Dialogs inside an immediate-mode GUI

The reference opens blocking dialogs (rename prompt, confirmations). egui cannot
block. Instead:

- When a request arrives (for example "rename species `S3`"), open an
  `egui::Modal` (or a window) that keeps a small piece of state, such as the text
  being typed.
- On OK, call `view.rename(..)`, which records one undo step.

Every confirmation (New with unsaved changes, re-import after the catalytic setting
changes) uses the same pattern.

### 6.3 Alignment

`view.align(mode)` with the eight modes and the effect on junctions and handles
from S §7.5. They appear in the context menu, and SHOULD also appear in an
**Arrange** menu so they can be found without right-clicking.

---

## 7. Undo, redo and layout

### 7.1 Undo

The commands are those of S §9.2: Snapshot, MoveNodes, MoveJunction, DragCtrlPt and
StyleEdit. Each is a variant of an enum, not a trait object:

```rust
pub enum Command {
    Snapshot   { desc: &'static str, before: Box<Model>, after: Box<Model> },
    MoveNodes  { species: Vec<(Id, Pt, Pt)>, junctions: Vec<(Id, Pt, Pt)> },
    MoveJunction { reaction: Id, from: Pt, to: Pt },
    DragCtrlPt { reaction: Id, role: Role, index: usize, from: CtrlState, to: CtrlState },
    StyleEdit  { species: Vec<(Id, VisualStyle, VisualStyle)>, reactions: Vec<(Id, VisualStyle, VisualStyle)> },
}
```

- **Snapshots store a cloned `Model`**, not JSON text. Cloning is faster than writing
  and reading JSON, and because the model refers to everything by id (§3.1), a clone is
  a complete, valid snapshot. The id counter (S §4) belongs to the model, so it is
  restored along with it.
- The history holds at most **100** entries (S §9.1). It SHOULD also limit the total
  memory used by snapshots (64 MB is suggested) and drop the oldest entries first.
- **When to use a Snapshot instead of a lighter command:** a drag that will also move
  stored handles, or that touches a smooth junction, records a Snapshot when the drag
  **starts** (S §9.2).
- **All** model changes record undo steps, including inspector edits, deleting from the
  context menu, and generating a random network (fixes D-4).

### 7.2 Auto-layout in steps, with no threads

The force-directed layout (S §10.1) is the only operation that uses a lot of CPU.
Running it in one go would freeze the window, and a background thread isn't
available in the browser build. So **one stepping version is used on every target**:

```rust
pub struct LayoutRun { /* positions, masses, speed, iteration count, options */ }
impl LayoutRun {
    pub fn new(model: &Model, params: LayoutParams, seed: u64) -> Self;
    pub fn step(&mut self, iterations: u32) -> Progress;    // runs up to this many iterations; never reads a clock
    pub fn positions(&self) -> PositionsView<'_>;           // used to draw the animation
    pub fn finish(self, model: &mut Model);                 // writes positions back, re-centres, runs the Deckard pass if chosen
}
```

**Who keeps time.** `pd-core` counts iterations, and `pd-app` measures time. The
app shell times each `step` call with `web_time::Instant` (the `web-time` crate,
which works both natively and in the browser) and adjusts the number of iterations
for the next frame so a frame's layout work takes about **8 ms**. Start at 5
iterations, multiply or divide the count to approach 8 ms, never go below 1, and
change it by at most ×2 per frame so it doesn't swing back and forth. This keeps the
animation smooth on slow and fast machines alike, and the layout itself stays fully
deterministic: the same model, parameters and seed always give the same result,
however many frames it took.

- **Parameters** are the values the app uses, from S §10.1:
  - 800 iterations;
  - `Kr = 8000`, `Ka = 0.02`, `Kg = 0.01`;
  - starting speed `max(20, 5% of the bounding-box diagonal)`, multiplied by 0.98 after
    every iteration;
  - **one spring per unique species** for each reaction;
  - locked nodes stay fixed;
  - aliases move with their primary.

  The Deckard pass (S §10.2) runs when it is chosen. Smooth-junction handles are
  recomputed afterwards, either way.
- **Each frame,** the app calls `step(n)` with the adaptive count, draws the positions so far (an animated
  layout, something the reference doesn't have), and shows progress with a **Cancel**
  button. Cancel throws the run away and leaves the model unchanged. Finishing records
  **one Snapshot** undo step.
- **Randomness.** Functions in `pd-core` that use random numbers (Randomize, random
  placement on import, the random network) take a `seed` argument. Tests pass a fixed
  one, and the app passes one from the clock.
- **Fix D-11.** After a layout run *without* the Deckard pass, stored handles of
  reactions without a smooth junction MUST be reset to automatic handles. Otherwise they
  would be left in their old absolute positions.

Make Nice for a single reaction (S §7.4.5) runs immediately. It is quick.

---

## 8. Import and export

**Decision: pure Rust.** The Antimony parser, the expression parser, the SBML writer
and reader, and the MathML conversion are all written in `pd-core`. They have no
native dependencies and work the same on every target.

This is more code than using libSBML and libAntimony. It was chosen because:

- the alternatives each need **two implementations that can drift apart**: native
  libraries on the desktop and JavaScript libraries in the browser;
- they need C++ build tools on every desktop platform;
- they add several megabytes of downloads to the web build;
- the functional spec already says exactly what has to be supported (S §8.3, §11.2,
  §11.5), so there is a definite amount of work.

### 8.1 Expression parser (`pd-core::expr`)

- Implement the grammar in S §8.3 **in full**, with the reference's limitations fixed
  (D-20): `&&`, `||`, `and`, `or`, `not`; a `-` before a digit is always an operator;
  numbers like `.5`; `^` groups from the right.
- The parser returns a syntax tree with a span (start and end position) on every
  node, so error messages can point at the mistake.
- Provide these functions:
  - `identifiers(&Ast) -> Vec<&str>`: in order, without repeats. Used for modifier
    sync (S §8.4) and for finding implied parameters.
  - `to_infix(&Ast) -> String`: prints the expression back with as few brackets as
    needed. Rate laws read from Antimony and SBML are stored in this form.
  - `to_mathml(&Ast) -> String` and `from_mathml(..) -> Ast`: the conversion table in
    S §11.5, in both directions.
- Modifier sync (S §8.4) runs **immediately, as a normal function call**. Because no
  external library is involved, there is nothing to load and nothing to wait for.

### 8.2 Antimony (`pd-antimony` and `pd-core::antimony`)

**Two parts, so the reader can be replaced.** The plan is to move to a crate that
supports all of Antimony. To make that a contained change:

- **`pd-antimony`** is a crate of its own with no dependencies. It reads Antimony
  text into a plain `Description` (compartments, species, reactions, assignments,
  rules, with expressions kept as written text and their offsets) and writes a
  `Description` back as text. It knows nothing about PathwayDesigner.
- **`pd-core::antimony`** is the bridge. It turns a `Description` into a `Model`
  and back, and holds everything specific to PathwayDesigner: case resolution,
  implied parameters, the catalytic rewrite, modifiers from the rate law, null
  nodes, random placement, and parsing expressions with `pd-core::expr`.

To use a complete Antimony crate, remove `pd-antimony` and change the bridge to
read the new crate's output. The panel, the view and the tests don't change.

**Import.** The rules are S §11.2, implemented completely:

- **Statements:** `model name … end` (and `model name()`), `compartment`,
  `species`/`var` (with `$`, `in`, `=` and lists separated by commas), reactions with
  an optional `id:` and stoichiometry, `->` with `=>` accepted as the same thing,
  `x = expr`, and `x := expr`.
- **Comments and separators:** `//` and `#` comments. Statements end with `;` or a
  new line.
- **What the import does:** create parameters that are used but never assigned;
  resolve ids typed in a different case to the declared spelling; apply the catalytic
  rewrite when the preference is on; detect modifiers; create null nodes; place nodes
  at random positions.
- **Errors** come back as a list of `{line, column, message}`. None of them stop the
  app. Show them next to the text in the Antimony panel (§11.1). On any error the
  current model MUST stay unchanged.
- **Unsupported Antimony** (events, functions, modules, units, and so on) gives a
  **warning** saying the statement was skipped, not a hard failure. This leaves room to
  move towards full Antimony later (S §16.2 F-10).

**Export.** S §11.3 exactly. Write only `->`, and wrap the output in `model <name>` …
`end`. Numbers are formatted the same way on every platform (fixes D-14).

After importing, SHOULD offer to run auto-layout. A preference can make it run
automatically.

### 8.3 SBML (`sbml-io` and `pd-core::sbml`)

**A reusable crate.** SBML reading and writing lives in **`sbml-io`**
(`crates/sbml-io`), a crate meant for others to use too. It is licensed MIT OR
Apache-2.0, and its design is in `crates/sbml-io/README.md`:
- plain Rust data, not libSBML's class hierarchy;
- Level 2 and Level 3 core plus the Layout package;
- MathML as a tree;
- other packages, notes, annotations and unknown XML kept for a round trip.

It knows nothing about PathwayDesigner. **`pd-core::sbml`** is the bridge, the same
pattern as Antimony (§8.2): it maps between `sbml-io`'s `Document` and the
`Model`, converts between `sbml-io`'s MathML tree and `pd-core::expr`, and holds
the rules below. A level-converting or more complete SBML crate could later
replace `sbml-io` behind the same bridge.

**Export** (S §11.5).
- **Core:** compartments, species (no null nodes, D-9), numeric parameters, other
  parameters as `initialAssignment`, reactions with `modifierSpeciesReference` (D-10),
  assignment rules, and rate laws as MathML.
- **Layout:**
  - one glyph for every node, aliases included;
  - curves that start and end on node borders;
  - `basePoint1 = ctrl1` and `basePoint2 = ctrl2` on both reactant and product legs,
    with no swapping (S §11.5);
  - glyphs for modifiers;
  - unique glyph ids (D-13).

**Import** (S §16.2 F-1), of Level 2 and Level 3 files:
- **Core:** compartments, parameters, species (using `initialConcentration`, or
  `initialAmount` if it is missing or 0), reactions with stoichiometry and modifiers,
  rate laws via `from_mathml` and `to_infix`, and assignment rules.
- **Layout, when the file has it:**
  - species-glyph boxes give the position and size;
  - the reaction glyph gives the junction;
  - each species-reference curve's base points become `ctrl1` and `ctrl2` (as stored,
    no swapping), `ctrlPtsSet = true` and mode `Bezier`;
  - a second species glyph for a species that already has one becomes an **alias**;
  - only things with no glyph are placed by auto-layout.

  This means files written by the Rust app or the Delphi app load back with their
  layout.
- **Afterwards:** null nodes for empty sides, and the catalytic rewrite only when the
  preference asks for it.

**Checking.** Without libSBML there is no complete SBML validator in the app.
Instead:
1. `pd-core::sbml` checks the structure (ids exist and are unique, references
   resolve) and shows any problems when the file is written;
2. CI checks every exported test file with **python-libsbml**'s
   `checkConsistency()`. That is a development-only dependency and is never shipped
   (§12).

### 8.4 Behaviour specific to PathwayDesigner

The catalytic rewrite and adding null nodes are separate passes over the model,
applied after any import, Antimony or SBML. They are not hidden inside one parser.
Each can therefore be tested on its own, and both behave the same for both formats.

### 8.5 Licences

Every dependency MUST be under a licence compatible with the project's MIT licence
(MIT, Apache-2.0, BSD, Zlib, or OFL for the font). Check this in CI with
`cargo deny check licenses`. The About box and a `THIRD-PARTY` file list the
dependencies and their licences. These can be generated with `cargo about`.

---

## 9. Graphics output

| Output | How |
|---|---|
| **SVG** | `SvgRenderer` draws the content bounds (S §11.6) without anything that depends on the current selection or mouse position (D-21), and writes every label as **outlines** (glyph paths), so the file looks the same in any viewer or editor, on any machine, whether or not the font is installed. Do not refer to the font by name. The reference has no SVG export, so this is new. |
| **PNG** | SVG → `usvg` → `resvg` → `tiny-skia`, scaled to the chosen DPI (default 150, so a scale of 150/96) |
| **PDF** | SVG → `svg2pdf`, giving a vector PDF. Page size A4 or Letter, portrait or landscape, with the drawing scaled to fit and centred (S §11.6) |
| **Print** | No portable printing API exists in Rust. **Desktop (D1):** write the PDF to a temporary file and open it with the system viewer (the `open` crate), and print from there. **Web:** open the PDF in a new tab (the browser's PDF viewer prints), or call `window.print()` on a page that shows only the drawing |

All of these build for wasm32. On the web the result is offered as a download (§10.1).

---

## 10. Platform adapters

These are the only places where code differs by target, using `#[cfg(target_arch = "wasm32")]`.
Each sits behind a small trait in `pd-app`.

The **web** side of each adapter is built in milestones R0–R12. The **native** side is
described here so the traits are designed for both targets from the start, but it is
built in the desktop milestone D1 (§15.1). Until D1, the native side MAY be a plain
stub that compiles: for example, an open that only reads a file given on the command
line.

### 10.1 Opening and saving files

| | Native (D1) | Web |
|---|---|---|
| Open | `rfd::FileDialog` | `rfd::AsyncFileDialog`, which uses a hidden file input |
| Save | `rfd::FileDialog::save_file`, then write the file. The path is remembered, so **Save writes back to the same file** | **File System Access API** where the browser has it (Chromium): keep the file handle so Save writes back to the same file. Otherwise download a Blob, so Save means "download a new copy". The status bar says which mode is in use. |
| Drag-and-drop | `RawInput::dropped_files` (with path) | `RawInput::dropped_files` (with bytes) |

- **Formats.** Accept `.json`, `.ant`/`.txt` and `.xml`/`.sbml`. Choose by file
  extension, and if that doesn't decide it, by content: `{` means JSON, `<sbml` means
  SBML, anything else Antimony.
- **Unsaved changes.** Track whether the document has changed. Ask before New, Open and
  Quit (S §13.1). "Changed" means **the undo history is at a different position from
  where it was at the last save or load**. Selecting, hovering, zooming and scrolling
  never count, even though `selected` is stored on model objects (§3). So the user is
  never asked to save after only clicking around. Undoing back to the saved state
  makes the document unchanged again. Native: intercept closing the window with `ViewportCommand` and
  `close_requested`. Web: register `beforeunload`.

### 10.2 Autosave and recovery

- **Web:** a tab can be closed or reloaded at any moment. Autosave the model 10 s after
  the last change: to IndexedDB, or to `localStorage` through eframe's storage if the
  model is small enough. On the next load, offer to recover it if it is newer than the
  last save.
- **Native:** autosave the same way to a recovery file in the app's data folder.

Autosave never adds anything to the document format.

### 10.3 Preferences

Use eframe's built-in storage (the `persistence` feature). On the desktop it is a
file in the app's data folder. On the web it is `localStorage`.

- Store the S §12 preferences under the same names
  (`convertCatalyticReactions`, `showNullSpecies`).
- Store UI-only settings (panel sizes, the alias-indicator toggle, layout options,
  recent files on desktop) under a separate key.
- Apply the preferences **when the app starts** (fixes D-2).
- On the web, add a `pathwayDesigner.` prefix to every key. The `github.io` origin is
  shared by every project page of the account (§14).

Sharing a settings file with the Delphi app is not needed.

---

## 11. User interface

Same layout as S §13: a single window, menu bar, toolbars, canvas, right-hand panel
(Species inspector, Antimony panel, Reaction inspector) and status bar.

| S §13 element | egui |
|---|---|
| Main menu (S §13.1), with Save / Save As (S §13.1 SHOULD) | Menu bar in a `TopBottomPanel`. The undo and redo items show what they will undo or redo (S §9.1) |
| Top toolbar and left tool palette (S §13.2) | Image buttons with SVG icons (`egui_extras` image loaders; convert `Icons/*.png` to SVG). The current mode is shown selected, and buttons are enabled or disabled from `selection()`. Every button has a hover text |
| Right panel | A `SidePanel` the user can resize. The Antimony panel uses a multi-line `TextEdit` with error markers in the margin (§8.2) |
| Context menu (S §13.4) | `Response::context_menu` for the target that was hit. Lock/Unlock works (D-16) |
| Colour picker (S §8.2) | A popup next to the swatch: an 8 × 5 grid of swatches, a preview and a `#RRGGBB` text field. Take the style snapshot when it **opens**, and record one StyleEdit undo step when a colour is applied |
| Status bar | Counts, the file-access mode on the web, and the compartment warning until compartments are drawn (F-2) |
| Dialogs | `egui::Modal`. Esc closes them, and Enter confirms |

### 11.1 The Antimony panel

- **Load Antimony** imports the text. **Get Antimony** replaces the text with an export
  of the current model (S §11.4).
- Parse errors are shown next to the text and in a list below it. Clicking an error
  moves the cursor to it.
- When the catalytic setting changes, the app SHOULD warn that the last imported source
  will be re-imported and that changes made since will be lost.

### 11.2 The inspector

egui rebuilds the inspector every frame from the current selection, so the
reference's guard against editing the model by mistake while filling in controls is
**not needed**. What is needed instead:

- A number or text field keeps its *draft* text while the user types. The change is
  applied, with one undo step, when the field **loses focus** or Enter is pressed
  (`Response::lost_focus()`), as in S §8.1.
- When the selection changes while a field is being edited, the draft is applied to
  the **previously** selected item, not the new one.
- The rate-law field accepts input only when exactly one reaction is selected. Parse
  errors are shown under the field and the law is still stored, because S §8.4 allows
  a law that doesn't parse.

### 11.3 Accessibility

- Every command can be reached from the keyboard through the menus. Every tool button
  has a hover text.
- egui's AccessKit integration (the eframe `accesskit` feature) MUST be turned on, so
  menus, panels and dialogs are visible to screen readers.
- The canvas can't be read by assistive tools. Add a **model tree panel**: species and
  reactions as a list that can be navigated with the keyboard, and whose selection is
  shared with the canvas. It also helps everyone find their way around large networks.
- The default colours already meet WCAG AA contrast for labels (`#1A1A1A` on
  `#EEF6FF`). Check this again after any change to the colours.

---

## 12. Testing

| Part | How |
|---|---|
| `geometry` | Unit tests covering every branch, including the special cases: a zero-length vector, a point equal to the centre, a zero-length segment |
| `model`, `json` | Round-trip tests on the reference files (§4.1). Property tests (`proptest`): any random model survives a save and reload, and any sequence of edits gives the same JSON after undo then redo |
| `expr` | The grammar table in S §8.3, error positions, and MathML round-trips for every operator and function in the S §11.5 table |
| `antimony` | Every `Models/*.ant` file imports. Exporting then re-importing keeps the biochemistry. The catalytic rewrite with the preference on and off. `=>` gives the same result as `->`. Null nodes. Errors leave the model unchanged |
| `sbml` | Export → python-libsbml `checkConsistency()` with no errors (CI only). Export → import keeps the geometry, including Bézier handles and aliases. Importing SBML Layout written by the Delphi app |
| `layout` | Seeded random generator. For `A → A + B`: the junction is outside A's box and the two arcs are apart. Running the layout twice doesn't make the drawing drift. Locked nodes stay put |
| `view` | The interaction state machine, tested without a window using made-up `ViewEvent`s, against every rule in S §6.2–§6.5 |
| Rendering | Compare images: `SvgRenderer` → resvg → PNG, against stored images for `Models/*.json` |
| UI | `egui_kittest` for interface tests that run without a window (menus, inspector applying on focus loss, modal dialogs) |
| Web | Build with trunk and run a short Playwright test against a local server serving the **sub-path** (§14.2): load, open a sample file, draw, save (download) |

The random network generator (S §7.7) uses a seed in tests.

---

## 13. Invariants

S §17.5.3 applies in full. Two of its rules are made stronger here, and six more are
added:

1. **Geometry is pure.** No egui types and no screen state in `geometry`.
2. **The model knows nothing about pixels. The view knows nothing about egui.** The
   crate boundaries enforce this (§2).
3. **References are ids** (stronger than S §9.2; see §3.1).
4. **`ctrl_pts_set` alone decides** whether handles are user-placed or automatic.
   Automatic handles are never saved.
5. **Bézier legs are stored centre to centre and cut at the node border when drawn.**
6. **The render mode is one enum** (stronger than S §3.4; see §3.2).
7. **Aliases are visual only.** Every export skips them as species, but SBML Layout
   gives each alias its own glyph.
8. **Identifiers are never renamed silently**, including their letter case.
9. **Style defaults are per field, using sentinels.**
10. **The document format is the contract.** Every key belongs to a numbered version
    that is written down (S §11.1 for v1–v3, §4.2 here for v4 on). A file is written
    at the lowest version that can hold its content, so documents that don't use new
    features stay readable by the Delphi app.
11. **`pd-core` and `pd-export` build for wasm32 and native, with no platform code.**
12. **No shared code depends on threads.** Long tasks run in steps (§7.2).
13. **The web build is a set of static files** that works from any sub-path, with no
    server configuration (§14).
14. **The model is `f64`, and only the screen side is `f32`** (§3, §5.1).
15. **`pd-core` is deterministic.** It never reads a clock or system randomness. Seeds
    and iteration counts come in as arguments, and loops follow `Vec` order, never
    `HashMap` order (§3.1, §7.2).

---

## 14. Building, hosting and deploying

### 14.1 Desktop (milestone D1)

Until D1, the only desktop work is keeping the native build compiling and the tests
running natively in CI (§14.5). The rest of this section applies from D1.

- Build with `cargo build --release -p pd-app`.
- Package with `cargo-packager` (or `cargo-bundle`): an MSI or NSIS installer for
  Windows, a `.app`/`.dmg` for macOS (signed and notarised for release), and an
  AppImage or `.deb` for Linux.
- Rendering uses eframe's default `wgpu` back end (WebGPU in the browser, with a
  WebGL fallback), with `glow` (OpenGL) as an option if some drivers have trouble.
- Release binaries are built in CI on each platform's GitHub Actions runner.

### 14.2 Web: GitHub Pages and its base path

- **Build:** `trunk build --release --public-url /<repo>/`.
- **Base path:** a project's Pages site is served from `https://<user>.github.io/<repo>/`,
  not from the domain root. Set the base path **once**, through `--public-url`, and
  never write an absolute path to an app file anywhere. Always test the production
  build **served from a sub-path** on your own machine. Serving it from `/` hides
  exactly the bugs that leave the page blank on Pages.
- **What GitHub Pages provides:** HTTPS (needed for the File System Access API and
  service workers) and automatic compression of the `.wasm` file.
- **What it doesn't provide:**
  - No custom headers, so no cross-origin isolation, so no WebAssembly threads. That
    is acceptable (§7.2, invariant 12).
  - No server-side redirects, so the app uses one page and no paths. Any state wanted
    in a link goes in the `#` part of the URL.
  - Limits: about 1 GB per repository, 100 MB per file, and about 100 GB of transfer a
    month.
- **Offline use:** a service worker (as in the eframe template) caches the app shell
  and the `.wasm` file. It is registered relative to the base path. The cache name
  includes the build hash, and old caches are deleted. The page offers a **"new
  version available — reload"** message, because Pages' own caching headers can't be
  changed.

### 14.3 Size

- Build the release with `lto = true` and `codegen-units = 1`, then run `wasm-opt`
  (trunk does this when configured). There is no hard size budget.

### 14.4 Errors and panics

- **Native (D1):** catch panics at the top of the frame loop. Save the model to the recovery
  file and show an error dialog.
- **Web:** install `console_error_panic_hook` and show a panel saying the app stopped,
  with a button to download the recovered model from autosave.

### 14.5 CI and deployment

GitHub Actions, on every push:

1. `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo deny check`.
2. `cargo test --workspace`, natively. This includes the **round-trip tests (§4.1) and
   the image comparisons**. If they fail, the build fails.
3. `cargo check --target wasm32-unknown-unknown` for every crate.
4. The SBML check with python-libsbml (§8.3).

On `main` only, also:

5. Build with trunk using the Pages base path.
6. Run the Playwright check (§12).
7. Publish with `actions/upload-pages-artifact` and `actions/deploy-pages`.

From D1 onwards, on tags, build and attach the desktop packages for all three
operating systems.

Steps 1–3 run from R0, before any desktop release exists. Step 2 builds and runs the
code natively, and `cargo check -p pd-app` is run for the native target too. This is
what keeps the desktop version possible: if anything stops it compiling, CI fails.

**Where the code lives.** The repository root is the Cargo workspace, with the crates
under `crates/`. The Delphi source is not part of it. The reference test files
(`Models/*.json`, `*.ant`) are copied into `Models/` here before R1. Build output
(`target/`, `dist/`) is never committed.

**Building the web app on Windows.** Run trunk from PowerShell, or from Git Bash with
`MSYS_NO_PATHCONV=1`. Otherwise Git Bash rewrites `--public-url /PathwayDesigner/`
into a `C:/Program Files/Git/…` path, and the page stays on "Loading".

---

## 15. Milestones

These follow the milestone plan in S §16 (M0–M12), so progress can be compared with
the functional spec. Each milestone below lists what is **specific to Rust**. Its
acceptance tests are the S §16 tests, run **in the browser build**. In addition, the
native build MUST still compile and the `pd-core` tests MUST pass natively. The desktop
app doesn't have to be usable until D1 (§15.1).

| # | Milestone (S §16) | Rust-specific work |
|---|---|---|
| **R0** | M0 Foundations | Workspace and crate boundaries (§2). CI for native and wasm32. The bundled font with `pd-core::text` measuring text widths. `geometry`. **A test page deployed to the real Pages URL** that draws one line of text with the bundled font. This proves the base-path set-up and the font pipeline before anything depends on them |
| **R1** | M1 Model and file format | `model`, the serde file structs, v1/v2/v3 reading, the colour codec, the D-8 fix. All `Models/*.json` round-trip |
| **R2** | M2 Drawing | The `Renderer` trait, `EguiRenderer`, `SvgRenderer`, all drawing passes for Straight and Linear reactions, zoom and pan with input mapping (§6.1). The image comparison tests start here |
| **R3** | M3 Selection and dragging | `ViewEvent`, the state machine, hit testing, rubber band, dragging, delete and its cascade, the undo framework (§7.1), keyboard shortcuts (§6.1) |
| **R4** | M4 Creating and editing | Tools, adding reactions and the feedback while picking species, the rename modal (§6.2), aliases, context menu, New. Also the hover tooltip, brought forward from R7 so rate laws can be checked before the inspector exists |
| **R5** | M5 Bézier curves | Automatic handles, clipping, handles and their drags, smooth junctions, Make Nice, mode commands, outlines for dashed rounded rectangles (§5.2) |
| **R6** | M6 Layout tools | Align and distribute, `LayoutRun` in steps with animation and Cancel (§7.2), the Deckard pass, locking, the D-11 fix |
| **R7** | M7 Rate laws and styling | `expr` with the full grammar (D-20), modifier sync, the inspector applying changes on focus loss (§11.2), the colour picker |
| **R8** | M8 Antimony | The `antimony` parser and export, the Antimony panel with error markers, the transform passes (§8.4), preferences storage (§10.3) |
| **R9** | M9 SBML | Export with Layout, MathML, the python-libsbml CI check. **Also SBML import with Layout import (F-1)**, which comes almost free because the reader shares the writer's structures |
| **R10** | M10 Graphics output | SVG, PNG via resvg, vector PDF via svg2pdf, printing in the browser (§9) |
| **R11** | M11 Documents and polish | Web file adapters (§10.1), unsaved-change tracking and prompts, autosave and recovery (§10.2), panic handling in the browser (§14.4), AccessKit, the model tree panel |
| **R12** | M12 Web release | The Pages deployment and service worker (§14.2), the `THIRD-PARTY` file, and the S §17.4 checklist passing in the browser |

### 15.1 D1 — Desktop release (later)

This builds the desktop version from the same crates. Because the shared code has
been compiled and tested natively since R0, it is mostly platform work:

- **Files:** the native side of the file adapters (§10.1). Native file dialogs, Save
  writing back to the remembered path, opening files by drag-and-drop with their path,
  and a recent-files list.
- **Closing and recovery:** asking about unsaved changes when the window is closed,
  the native autosave recovery file (§10.2), and the native panic handler (§14.4).
- **Settings:** preferences stored in the app's data folder (§10.3).
- **Printing:** through the system PDF viewer (§9).
- **Desktop conventions:** on macOS, the standard app menu (About, Preferences and Quit
  in their usual places) and Cmd shortcuts.
- **Packaging (§14.1):** signed installers or packages for Windows, macOS
  (notarised) and Linux, built in CI on tags.
- **Acceptance:**
  - the S §17.4 checklist passes in the desktop build on all three operating systems;
  - the desktop budgets in §5.6 are met;
  - a file saved by the desktop build opens unchanged in the browser build, and the
    other way round;
  - a v3 file opens unchanged in the desktop build and in the Delphi app, both ways
    round.

### 15.2 After matching the reference

In order of value. These follow S §16.2.

1. **Draw compartments** (F-2). The model and its styles already exist and are saved.
   Draw compartments as a background layer, and decide which species belong to one by
   whether they sit inside it.
2. **Font size and label colour controls** (F-3). The fields exist and are already
   drawn. Only the inspector controls are missing.
3. **Node shapes and label position** (F-4, F-5). These add new keys to the file, so
   they are the first features in **JSON v4** (§4.2). Write the v4 keys down in §4.2
   before releasing them.
4. **Styles in SBML using the Render package.** Map `VisualStyle` onto SBML Render so
   colours and widths survive an SBML round-trip. Write down the mapping first. The
   Delphi app won't read it, and doesn't need to: it ignores packages it doesn't
   know.
5. **Arrange in a circle** (F-7), next to align and distribute.
6. **Fuller Antimony** (F-10): events, functions, modules, replacing the warnings from
   §8.2.
7. **Tabs or several windows** (F-9). egui's viewports make several windows practical
   on the desktop.
