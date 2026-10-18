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
| Graphics export | One drawing pass with an SVG writer and a PDF writer. **resvg** turns the SVG into PNG | Vector PDF and SVG on every platform, all in pure Rust (§9). |
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
      antimony                  S §11.2, §11.3 — the bridge: maps the antimony crate's model
                                onto the diagram (import), and writes the diagram as Antimony
                                (export)
      sbml/                     S §11.5 — L3V1 core + Layout writer, MathML; reader (F-1)
      text/                     text width from the bundled font (§5.4)
      view/                     view state, hit testing, the interaction state machine,
                                editing commands (S §5–§7)
      render/                   the Renderer trait and the drawing passes (S §5.2),
                                plus SvgRenderer (§9)
    antimony/                   the whole Antimony language: text to a model of it, with
                                diagnostics, and back; SBML output behind a feature. No
                                dependencies and nothing about PathwayDesigner (§8.2)
    pd-export/                  graphics output: SVG to PNG (resvg), and pd-core's SVG and PDF. No egui. Builds for wasm32 too
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

**Version 4: free text (S §7.8).** A top-level `texts` array, after `reactions`,
left out when empty. A document without texts is still written as version 3. Each
text is an object; every key but `id` may be missing and takes its default, and the
writer leaves out keys at their default (except `size`, `x`, `y` and `text`):

| Key | Default | Meaning |
|---|---|---|
| `id` | (required) | `T<n>`; unique among all ids |
| `x`, `y` | 0 | the top-left corner of the first line |
| `text` | `""` | the words; `\n` starts a new line |
| `size` | 14 | font size; below 1 reads as 14 |
| `typeface` | `"sans"` | `"sans"` or `"serif"`; anything else reads as sans |
| `bold`, `italic` | false | |
| `align` | `"left"` | `"left"`, `"center"` or `"right"`; anything else reads as left |
| `color` | label colour | `#RRGGBBAA` |
| `wrapWidth` | 0 | 0: no wrapping |

`Models/text_1.json` is the version 4 sample file.

**Version 5: species shapes (S §5.10) and border styles (S §5.3).** A species may
have `shape`: one of `"rectangle"`, `"ellipse"`, `"circle"` or `"hexagon"`; left
out for the rounded rectangle. It may have `borderDash`: `"solid"` or `"dashed"`;
left out for automatic. A document in which any species has either is version 5; otherwise it
is written as version 4 or 3 as before, so a model that doesn't use shapes can
still be opened by the Delphi app or an older Rust version. A shape this version
doesn't know is read as the rounded rectangle, and a border style it doesn't
know as automatic, each with a warning. An older Rust
version opening a version 5 file sees that it is newer and makes Save behave as
Save As (§4).

**Version 9: background colour (S §8.8).** A top-level `"background"`, after
`theme`, as `#RRGGBBAA`; left out for the default. A document with its own
background is version 9.

**Version 8: stems (S §5.13).** A reaction may have `stem`, after `labelDy`:
`[rdx, rdy, pdx, pdy]`, where its reactants' lines end and its products' lines
start, each from the junction. Left out for a reaction without one; anything but
four numbers is left out, with a warning. A document in which a reaction has a stem
is version 8; otherwise it is written as before.

**Version 7: reaction names (S §5.12).** A reaction may have `labelDx` and
`labelDy`, after `styleName`: where its name's centre is drawn, from its junction.
Both are written for a reaction showing its name and both left out otherwise; a
reaction with only one, or a value that isn't a number, doesn't show its name, with
a warning. A document in which a reaction shows its name is version 7; otherwise
it is written as before. A list of styles on its own stays version 6, as nothing in
a style needs 7. `Models/reaction_label_1.json` is the sample file.

**Version 6: names outside the node (S §5.11).** A species may have `labelDx`
and `labelDy`: where its name's centre is drawn, from the node's centre. Both are
written for a name outside the node and both left out for one inside. A species
with only one of them, or with a value that isn't a number, has its name drawn
inside, with a warning. A document in which any species has its name outside is
version 6; otherwise it is written as version 5, 4 or 3 as before.
`Models/label_1.json` is the sample file.

**Version 6: no border (S §3.6).** A species or compartment may have
`"noBorder": true`: it is drawn with no border. It is an item key, beside `style`,
because inside the style object `borderWidth` 0 means "the default" to every
reader. Left out otherwise; a document in which a species or compartment has it is
version 6. A saved style of those kinds carries it the same way.

**Version 6: themes (S §8.7).** A top-level `"theme": "escher"` after `styles`,
left out for none (Classic). A theme this version doesn't know is read as none,
with a warning. A document with a theme is version 6.

**Version 6: saved styles (S §8.6).** A species, reaction, compartment or text may
have `styleName`: the saved style it follows. A top-level `styles` array, after
`texts`, left out when empty, holds the styles some item follows (a style nothing
follows is not written). Each is an object with `name`, `kind` (`"species"`,
`"reaction"`, `"compartment"` or `"text"`) and the look, in the keys an item of that
kind uses, each left out at its default:

| Kind | Keys |
|---|---|
| species | `style` (the style object, only when custom), `shape`, `borderDash`, `labelOutside` (true for a name outside the node), and then `w` and `h`, the size |
| reaction | `style` (written as on a reaction, so a hidden junction is kept) |
| compartment | `style` |
| text | `size`, `typeface`, `bold`, `italic`, `align`, `color` |

A list of styles on its own (My Styles, kept under the preference key
`pathwayDesigner.styles`, and the file Export My Styles… writes) is
`{"version": 6, "styles": [...]}`, each style as above. A diagram has the same
`styles` key, so Import Styles… reads either. Looks are read into a standard form
(S §8.6).

Keys that don't belong to the kind are ignored. A style of a kind this version
doesn't know, with no name, with a name its kind already has, or with
`labelOutside` but no size, is skipped with a warning. An item following a style
the file doesn't have is unlinked with a warning; one that doesn't look like its
style is unlinked quietly. A document in which an item follows a style is version
6.

**Version 4: drawn compartments (S §7.9).** Each entry of `compartments` may have
`x`, `y`, `w` and `h`: the box's centre and size, as for a species. All four are
written for a drawn compartment and left out otherwise. A compartment missing any
of them, or with a width or height below 1, is not drawn yet, and gets a box fitted
around its species when opened. A document with a drawn compartment is version 4.
`Models/compartment_1.json` is its sample file. Opening a version 3 file that has
compartments fits boxes, so saving it writes version 4: drawing compartments needs
their boxes. A compartment whose box is hidden (S §7.9) has `"hidden": true`, and
keeps its `x`, `y`, `w` and `h`; a shown one leaves `hidden` out.

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

- Bundle **Noto Sans Regular** (SIL Open Font Licence) in `assets/fonts/` and
  use it for diagram labels in egui, SVG, PNG and PDF. Free text (S §7.8) may
  also use Noto Sans Bold, Italic and Bold Italic, and Noto Serif in the same
  four faces: eight files, all Noto 2.015 from the same project, under the same
  licence. In egui each face is a font family of its own holding only that
  font, so egui draws exactly what was measured. SVG and PDF draw glyph
  outlines from the font data, so no font needs to be installed.
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
| Cursor | An open hand over a species or junction, and a closed hand while dragging one. A pointing hand over a leg. A **crosshair** over a Bézier handle and while dragging it: a hand would cover the small handle. A crosshair in the add tools, and an open hand while Space is held. |

**Keyboard shortcuts.** Use `Context::input_mut(|i| i.consume_shortcut(..))` so a
text field that has the keyboard focus gets its normal editing keys first.
Shortcuts that affect the diagram only apply while the canvas has focus or no text
field is being edited. The file shortcuts (New, Open, Save, Save As) are the
exception: they mean nothing to a text field, so they work while typing too. The
field gives up the keyboard first, which applies what was typed, and the command
runs on the next frame. (Fixed 2026-10-12: Ctrl+S right after typing text in the
Text group did nothing, so the text was never saved.)

| Keys | Action |
|---|---|
| Delete / Backspace | Delete the selection |
| Esc | Cancel |
| Cmd+A | Select all |
| Arrow keys | Nudge the selection 1 unit; with Shift, 10 units. While snapping to the grid (S §7.6), to the next grid line (five with Shift); Alt keeps the 1 and 10 units |
| Cmd+Z | Undo |
| Cmd+Shift+Z, Ctrl+Y | Redo |
| Cmd+S / Cmd+Shift+S | Save / Save As |
| Cmd+O | Open |
| Cmd+N | New |

**Nudging** (added at the user's request; the reference has none). The arrow
keys move the selected species and the junctions of the selected reactions, as a
drag by the same amount would: locked species stay, a junction follows when all
its species move, and stored handles follow their end. A run of nudges with the
same selection is **one undo step**: the history's last snapshot is amended
(`UndoHistory::amend_last`) instead of a step being added per key press. Any
other change, an undo, or a different selection starts a new step. While a
widget has the keyboard focus (a text field, or a row of the model tree, which
uses ↑/↓ itself), the arrow keys are that widget's.

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
Arrange in Circle (S §7.5.1, `view/circle.rs`) is the ninth command,
`Arrange::Circle`, after a separator.

### 6.4 Browser default actions

In the browser, a key or pointer event reaches egui *and* the page, and eframe
cancels the browser's default action for only a few shortcuts (Ctrl/Cmd+S, O, P
and comma, and Space). Anything else egui uses as a shortcut also does what the
browser would do with it. The page must leave the browser nothing harmful to do.

**The bug this caused (fixed 2026-10-10).** Ctrl/Cmd+A selected everything in
the diagram, and the browser also ran its own Select All, which selected the
canvas. Clicking the canvas doesn't clear a page selection. The next press on a
spot inside that selection, followed by a move, started the browser's native
drag-and-drop. During one, the page gets no `mousemove` and no `mouseup`, so:
- the species stayed where it was while the button was held;
- egui never saw the release, so after the button was let go the species jumped
  to the pointer and followed it, as if still dragged;
- the next click gave egui its release and ended the drag.

Which species failed depended on where the browser's selection lay. That made it
look as though it depended on a species' colour or on Bézier reactions.

**The rule (in `index.html`):**
- nothing on the page is selectable: `user-select: none` on `html`, `body` and
  the canvas, and `-webkit-user-drag: none` on the canvas;
- the browser's `dragstart` is cancelled on the whole document.

This doesn't affect dragging inside the app, which egui does from raw pointer
events. egui's text fields still work, because they use eframe's own `<input>`.
Files dropped onto the page from outside are unaffected too: `dragstart` only
fires for drags that start inside the page.

When adding a shortcut, check what the browser does with it by default. If that
default is harmful, cancel it, or make sure the page gives it nothing to act on.

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

### 8.2 Antimony (the `antimony` crate and `pd-core::antimony`)

**Two parts.**

- **`antimony`** (package `antimony-rs`, `crates/antimony`) is a crate of its own,
  written to be published and used without PathwayDesigner: a port of a complete
  Antimony implementation, checked against it case by case. It reads all of the
  language into a model of it (species, reactions, rules, events, functions, units,
  annotations, …) with diagnostics that carry a line and column, and writes Antimony
  and SBML. Its design is in `crates/antimony/SPEC.md`. It knows nothing about
  PathwayDesigner and has no dependencies; pd-core does not turn on its `sbml`
  feature, since pd-core has its own SBML bridge (§8.3).
- **`pd-core::antimony`** is the bridge. It turns the crate's model into a diagram
  `Model`, and writes a diagram as Antimony text, and holds everything specific to
  PathwayDesigner: case resolution, implied parameters, the catalytic rewrite,
  modifiers from the rate law, null nodes, random placement, warnings for what a
  diagram cannot hold, parsing expressions with `pd-core::expr`, and the layout of
  the text it writes (S §11.3).

Until A9 of the crate's plan (2026-10-15) the reader was a small crate of its own,
`pd-antimony`, which understood only the statements a diagram holds. The bridge's
tests did not change in the swap, but for one input: a `model` block with no `end`,
which that reader accepted and Antimony does not.

**Import.** The rules are S §11.2, over the whole language:

- **Statements:** anything Antimony allows reads without an error. What a diagram
  holds is used: `model name … end`, compartments, species (with `$`, `in` and
  values), reactions with an optional `id:` and stoichiometry, `->` and `=>` (the
  same thing in a diagram), `x = expr`, `x := expr`, and `const`/`var`, which are
  read through. A `$` on a name that is not otherwise a species (`$B = 4`) makes a
  boundary species, as it always has here.
- **Comments and separators:** `//`, `#` and `/* … */` comments. Statements end
  with `;` or a new line.
- **What the import does:** create parameters that are used but never assigned;
  resolve ids typed in a different case to the declared spelling; apply the catalytic
  rewrite when the preference is on; detect modifiers; create null nodes; place nodes
  at random positions.
- **Errors** come back as a list of `{line, column, message}`. None of them stop the
  app. Show them next to the text in the Antimony panel (§11.1). On any error the
  current model MUST stay unchanged.
- **What a diagram cannot hold** (functions, units, events, rate rules, algebraic
  rules, constraints, objectives, interactions, other models in the file, names,
  notes and annotations, units on symbols) gives a **warning** saying it was skipped,
  not a failure (S §16.2 F-10). The crate's own warnings are passed on too, except
  that a name used without a value is not one: here it is a parameter or species
  at 0, by design.
- **What still gives an error:** text that is not Antimony, a species' value or a
  compartment's size that is an expression rather than a number, two reactions
  with one name, and a rate law that `pd-core::expr` cannot read (it has no `%` or
  `? :`).

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
  rate laws through `pd-core::sbml::math` and `to_infix`, and assignment rules.
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

**Details settled by the implementation (R9).**
- **Round trip.** Every `Models/*.json` exports and imports back with the same
  biochemistry and the same drawing: node boxes, alias ids, junctions, every
  leg's curve, and every reaction's mode and smooth junction.
- **PathwayDesigner's note** (`crates/pd-core/src/sbml/shape.rs`). Layout records
  only geometry, so it can't say that a reaction is Linear (one straight line, no
  junction) or that a Bézier junction is smooth. The export adds a note to the
  reaction glyph's annotation, only where it is needed:
  `<pd:reaction xmlns:pd="https://github.com/sys-bio/PathwayDesigner-rs/ns/sbml-layout/1" mode="linear"/>`,
  or `smoothJunction="true"`. The import reads it back; a file without it imports
  as before. Other tools skip annotations they don't know, and libSBML's
  `checkConsistency()` accepts the files. (Until 2026-10-12 Linear came back as
  Straight, and smooth junctions as plain ones.)
- **Species references get ids** (`sr_<rid>_sub_<sid>`, `…_prod_…`, `…_mod_…`,
  made unique), and each species-reference glyph names its reference. Import
  matches a glyph to its participant by that id. Without one it uses the glyph's
  role and species, as in files from the Delphi app.
- **Null nodes keep their place.** A null node is not a species (D-9), but it gets
  a species glyph that names no species, and its leg's glyph points at that. On
  import, such a glyph places the null node and gives its leg's handles.
- **Curves drawn the other way round.** The handles are taken as stored, unless a
  curve runs opposite to this app's convention: from the junction to the species
  for a reactant, or from the species to the junction for a product. Other tools
  may write curves that way (the Layout test files in `crates/sbml-io/tests/data`
  do), and taking those as stored would cross the handles, so they are swapped
  to match.
- **A reaction glyph without a bounding box** (libSBML leaves it out when there
  is a curve) puts the junction at the middle of its curve.
- **An all-zero bounding box means no position.** libSBML writes position (0, 0)
  and size 0 × 0 for a glyph that was never given one. Such a species glyph
  doesn't place its species, and such a reaction glyph gives no junction: the
  junction then goes halfway between the reaction's species. A layout in which
  no species glyph has a position counts as no layout, so the model is laid out
  automatically.
- **Local parameters** become model parameters. A name already taken becomes
  `<reaction>_<name>`, and the rate law is rewritten to match.
- **What a diagram can't hold** gives a warning: function definitions, events,
  rate and algebraic rules, constraints, and initial assignments to anything but
  parameters. MathML the expression language can't say (`infinity`, `notanumber`,
  MathML outside SBML's subset) leaves that expression out, with a warning. `pi`,
  `true`, `false` and Avogadro become numbers, `exponentiale` becomes `exp(1)`,
  `log` with another base becomes `ln(x)/ln(b)`, and `piecewise`, `delay` and
  `rateOf` become calls of those names.
- **Export of `max` and `min`.** These exist in MathML only from Level 3
  Version 2. In the Level 3 Version 1 export they are written as nested
  `piecewise`.
- **Names used but never defined.** If an expression uses a name the model
  doesn't define, the export declares it as a parameter with value 0, as
  Antimony import does (S §11.2), and says so. Without this the file would be
  invalid.
- **A file with no layout** is laid out automatically when it is opened or
  imported. Every species would otherwise be at a random place. A file whose
  layout leaves some species out says how many were placed at random.
- **Menus.** File ▸ *Import SBML…* replaces the diagram as one undo step, like
  *Import Antimony…*. Both name the document after the imported file (`model.xml`
  becomes `model.json`) and forget the previous file, so the status bar and title
  show the new name and Save asks where to save, instead of writing the imported
  model over the previous file. *Open…* also opens SBML, as a new document. *Save to
  SBML…* writes Level 3 Version 1 with the layout. Problems and notes about the
  file are shown in the status bar once it is written.

**Checking.** Without libSBML there is no complete SBML validator in the app.
Instead:
1. `pd-core::sbml` checks the structure (ids exist and are unique, references
   resolve) and shows any problems when the file is written;
2. CI exports every `Models/*.json` and checks it with **python-libsbml**'s
   `checkConsistency()` (`crates/pd-core/examples/export_sbml.rs`,
   `tests/scripts/check_sbml.py`). All pass with no errors; the warnings are
   about units, which these models don't declare. python-libsbml is a
   development-only dependency and is never shipped (§12).

**Species shapes (S §5.10) and border styles (S §5.3)** go in a note on the
species glyph, written only when one is not the default:
`<pd:species xmlns:pd="…/ns/sbml-layout/1" shape="ellipse" border="solid" noBorder="true"
label="outside"/>`, with only the attributes that are set. Import gives each node
its glyph's shape and border style (an alias has its own glyph, so its own). SBML
Layout has neither, so other tools draw a box with a solid border. A name outside
the node (S §5.11) also gives the species' text glyph the name's own box rather
than the node's, so other tools draw it in the same place; import reads a name as
outside when the note says so or when the text glyph's centre is outside the
species glyph's box.

**Compartments (S §7.9)** are written as compartment glyphs with their box. A custom
style goes in a note on the glyph: `<pd:compartment xmlns:pd="…/ns/sbml-layout/1"
fill="#F0F6EAFF" border="#7FA36BFF" borderWidth="3"/>`, with only the fields that
are set; a hidden box (S §7.9) is still written as a glyph, with `hidden="true"` in
the note. Import gives each compartment the box of its glyph and the note's style,
and hides the box if the note says so. Compartments without a glyph are fitted
around their species.

**Free text (S §7.8)** is written as a text glyph with a `text` attribute and its
box (the text's layout width and height), and no graphical object or origin of
text. SBML Layout has no fonts, so the glyph's annotation holds PathwayDesigner's
note, next to the reaction note:
`<pd:text xmlns:pd="…/ns/sbml-layout/1" size="14" typeface="serif" bold="true"
italic="true" align="center" color="#CC0000FF" wrapWidth="120"/>`, with only `size` always present.
Import makes free text from every text glyph that has a `text` and labels nothing,
at its box's top-left corner, with the note's look if it has one and the defaults
otherwise. Text glyphs that label a species are not free text.

**Stems (S §5.13)** are written as the reaction glyph's own `curve`: two line
segments, from the reactants' end of the stem to the junction and on to the
products' end; each species reference glyph's curve ends (or starts) at its side's
end. A reaction without a stem has no glyph curve. Reading, a reaction glyph with a
curve gets a stem from the curve's first point to its last, turned round if the
first is nearer the products and the last nearer the reactants (as another tool may
write it), unless the reaction is Linear or has a smooth junction, or both ends are
at the junction. The junction is still the glyph's box (or, without one, halfway
along the curve).

**Reaction names (S §5.12)** are written as a text glyph on the reaction's glyph,
`graphicalObject` its id and `originOfText` the reaction, in the name's own box, so
other tools show the name where it is drawn. Import shows a reaction's name when a
text glyph has its reaction glyph as `graphicalObject` (with or without
`originOfText`), drawn at the glyph's centre. The name's size and colour are not
carried, as SBML Layout has no fonts; they come back as the defaults.

### 8.4 Behaviour specific to PathwayDesigner

The catalytic rewrite, modifiers from the rate law, and adding null nodes are
separate passes (`pd-core::passes`), applied to each reaction after any import,
Antimony or SBML. They are not hidden inside one parser. Each can therefore be
tested on its own, and they behave the same for both formats.

### 8.5 Licences

Every dependency MUST be under a licence compatible with the project's MIT licence
(MIT, Apache-2.0, BSD, Zlib, or OFL for the font). Check this in CI with
`cargo deny check licenses`. The About box and a `THIRD-PARTY` file list the
dependencies and their licences. `scripts/third_party.py` generates
`THIRD-PARTY.html` from `cargo metadata`: pd-app's run-time dependencies for the
browser and the desktop, each crate's own licence files (identical texts listed
once), standard texts with authors and repository for crates that ship no licence
file, and the Noto Sans licence. The web app ships the page next to itself and the
About box links to it. CI fails if it is out of date (`--check`). (`cargo about`
was the plan; a small script needs no tool to install.)

---

## 9. Graphics output

| Output | How |
|---|---|
| **SVG** | `SvgRenderer` draws the content bounds (S §11.6) without anything that depends on the current selection or mouse position (D-21), and writes every label as **outlines** (glyph paths), so the file looks the same in any viewer or editor, on any machine, whether or not the font is installed. Do not refer to the font by name. The reference has no SVG export, so this is new. |
| **PNG** | SVG → `usvg` → `resvg` → `tiny-skia`, scaled to the chosen DPI (default 150, so a scale of 150/96) |
| **PDF** | `PdfRenderer` (`pd-core::render::pdf`) draws through the same `Renderer` calls as the screen and the SVG, labels as glyph outlines, and writes the PDF itself: one page, paths, fills, strokes and transparency, no fonts. Page Letter (the default) or A4, portrait or landscape, with the drawing scaled to fit inside a half-inch margin and centred (S §11.6); or **Drawing size**, a page exactly the drawing at 96 world units to the inch, for putting into papers |
| **Print** | No portable printing API exists in Rust. *Print…* asks for the page as for PDF, then opens the PDF where it can be printed. **Web:** in a new tab, where the browser's PDF viewer prints; if the browser blocks the tab, a message suggests allowing pop-ups or using *Export ▸ PDF…*. **Desktop:** written to a temporary file and opened with the system viewer (the `open` crate) |

All of these build for wasm32. On the web the result is offered as a download (§10.1).

**Why not svg2pdf (R10).** svg2pdf was the plan, but its current release (0.13) is
built on an older usvg (0.45) than the resvg the PNG path uses (0.48), so both
would be compiled into the web app. The drawing needs only six primitives
(rounded rectangle, circle, polyline, cubic, polygon, outlined text), so a direct
PDF writer is about 300 lines with no dependencies, and it shares the drawing
code with the screen and the SVG.

**Checking the PDF.** Unit tests check the file's structure (the
cross-reference table, stream lengths, page sizes, fitting). CI writes every
`Models/*.json` as PDF and PNG (`crates/pd-export/examples/export_models.rs`), and
`crates/pd-export/tests/scripts/compare_pdf.py` opens each PDF with pypdf and has
**pypdfium2** (the PDF engine of Chrome) draw it: after a one-pixel blur, which
evens out the engines' different anti-aliasing, it must match resvg's PNG to
within 48 levels out of 255. All models match to within 24; leaving out the
labels makes every model fail (119 or more).

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

A file that has been read (from a dialog or dropped) is opened on the **next
frame**: the frame it arrives shows the busy cursor and *Opening <name>…* in the
status bar, since converting a big file blocks the browser's only thread until it
is done.

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
  (`convertCatalyticReactions`, `showNullSpecies`), with `selectedReactionColour`
  and View ▸ Modifier lines (`showModifiers`), View ▸ Grid (`showGrid`) and View ▸
  Snap to grid (`snapToGrid`), S §7.6, which are kept the same way.
- My Styles (S §8.6) are kept under `styles`, as a style list (§4.2). They live in
  the view beside the model (`DiagramView::my_styles`), so opening a file keeps
  them, and are saved whenever eframe saves (every few seconds and on exit).
- Store UI-only settings (panel sizes, the alias-indicator toggle, layout options,
  recent files on desktop) under a separate key.
- Apply the preferences **when the app starts** (fixes D-2).
- On the web, add a `pathwayDesigner.` prefix to every key. The `github.io` origin is
  shared by every project page of the account (§14).

Sharing a settings file with the Delphi app is not needed.

---

## 11. User interface

Same layout as S §13: a single window, menu bar, toolbars, canvas, right-hand panel
(the Properties and Antimony tabs, S §13.3) and status bar.

| S §13 element | egui |
|---|---|
| Main menu (S §13.1), with Save / Save As (S §13.1 SHOULD) | Menu bar in a `TopBottomPanel`. The undo and redo items show what they will undo or redo (S §9.1) |
| Top toolbar and left tool palette (S §13.2) | Image buttons with SVG icons (`egui_extras` image loaders; convert `Icons/*.png` to SVG). The current mode is shown selected, and buttons are enabled or disabled from `selection()`. Every button has a hover text |
| Right panel | A `SidePanel` the user can resize, with two tabs (S §13.3). Properties shows only the inspector groups for the selection. The Antimony tab uses a multi-line `TextEdit` with error markers in the margin (§8.2) |
| Context menu (S §13.4) | `Response::context_menu` for the target that was hit. Lock/Unlock works (D-16) |
| Colour picker (S §8.2) | A popup next to the swatch: an 8 × 5 grid of swatches, a preview and a `#RRGGBB` text field. Take the style snapshot when it **opens**, and record one StyleEdit undo step when a colour is applied |
| Status bar | Counts, the file-access mode on the web, and the zoom control at the right (S §13.5) |
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
- The canvas can't be read by assistive tools. Add a **model tree panel**: species,
  reactions and compartments as lists that can be navigated with the keyboard, and
  whose selection is shared with the canvas. (Compartments were added so that one
  whose box is hidden can still be selected, S §7.9.) It also helps everyone find their way around large networks.
  Selecting in the tree never moves the diagram: stepping through the list with
  the arrow keys would otherwise scroll it at every step, and the selected item
  would be hard to follow. (Earlier builds centred the clicked item.)
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
| Web | `crates/pd-app/tests/web_smoke.py` (CI job `web`; Pages deploys only if it passes): the production build served from the **sub-path** (§14.2) in headless Chromium, which draws the app through WebGL. Checks that it starts without errors, the service worker takes over, it starts again with the network off, the new-version banner appears for a newer published build, and the licences page is there. **Still to add:** open a sample, draw, save (download), and Ctrl/Cmd+A, a click on empty canvas, then a drag (§6.4) |

**Native tests can't see the browser.** `pd-app`'s whole-app test drives the
real app through egui with made-up input (New, add species, Ctrl+A, clear,
drag). It guards the wiring from egui through the canvas to the view, but the
Select All bug of §6.4 passed it, because the cause was the browser's default
action. When a bug shows only in the browser, get the exact steps from the
person who saw it before building native reproductions, and suspect what the
browser itself does with the input.

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

**As built (D1).** `cargo-packager` is configured in `crates/pd-app/Cargo.toml`
(`[package.metadata.packager]`): product name, identifier
`org.sys-bio.pathwaydesigner`, the icons in `assets/icon` (drawn as `icon.svg` and
rendered to PNG and ICO by `cargo run -p pd-export --example make_icons`), the
licence, and `THIRD-PARTY.html` as a resource. `.github/workflows/release.yml`
builds the packages when a `v*` tag is pushed (or by hand from the Actions tab):
an NSIS installer on Windows, a `.app` (zipped) and `.dmg` on macOS (Apple
silicon), and a `.deb` and AppImage on Linux, attached to a **draft** release to
check before publishing. The packages are **not signed**, by decision: Windows
SmartScreen and macOS Gatekeeper warn on first start (Windows: *More info ▸ Run
anyway*; macOS: right-click the app and choose *Open*). This replaces the plan
above of signed installers and a notarised macOS app.

### 14.2 Web: GitHub Pages and its base path

- **Build:** `trunk build --release --public-url ./`: relative paths, so the same
  files work at `https://<user>.github.io/<repo>/` and from any folder of any other
  server.
- **Copying the site:** CI writes the built files, with a readme
  (`crates/pd-app/web/README.md`), to the **`gh-pages` branch** after every
  successful push to `main`, as one commit that replaces the last, so old builds
  don't pile up in the repository. Anyone can copy that branch to their own server
  and open `index.html`, as with MakeSBML; nothing needs building. Each release
  also carries the same files as `PathwayDesigner-web-<version>.zip`. (Until
  2026-10-11 the site was only uploaded to Pages from CI, so the built files
  existed nowhere anyone could copy them.)
- **Base path:** a project's Pages site is served from `https://<user>.github.io/<repo>/`,
  not from the domain root. With `--public-url ./` every path is relative, so there
  is no base path to set. Never write an absolute path to an app file anywhere.
  Always test the production build **served from a sub-path** on your own machine
  (`web_smoke.py` does): serving it from `/` hides exactly the bugs that leave the
  page blank on Pages.
- **What GitHub Pages provides:** HTTPS (needed for the File System Access API and
  service workers) and automatic compression of the `.wasm` file.
- **What it doesn't provide:**
  - No custom headers, so no cross-origin isolation, so no WebAssembly threads. That
    is acceptable (§7.2, invariant 12).
  - No server-side redirects, so the app uses one page and no paths. Any state wanted
    in a link goes in the `#` part of the URL.
  - Limits: about 1 GB per repository, 100 MB per file, and about 100 GB of transfer a
    month.
- **Offline use:** a service worker (`crates/pd-app/sw.js`) caches the app. It is
  registered relative to the base path, so its scope is the base path. Trunk names
  the `.js` and `.wasm` files after a hash of their contents, so a file with a hashed
  name never changes and is served from the cache once fetched. The page itself is
  fetched from the network first, so a reload always gets the newest version when
  online, and the cached one offline. Each fresh page prunes the cached hashed files
  it no longer uses. (This replaces the plan of a cache named after the build: the
  service worker never needs to know the build.)
- **Zoom to fit at startup:** a browser tab can give the canvas its final size well
  after the first frames, so a fit done once came out tiny, in the top left corner.
  After a zoom to fit (startup, Open, import), the fit is done again whenever the
  canvas changes size, until the user first presses a button, scrolls, zooms or
  types.
- **"New version available":** the page compares the build it runs (the hash in its
  `pathway-designer-<hash>.js`) with the one the published page names: 10 seconds
  after loading, every 30 minutes, and when the tab comes back into view. If they
  differ, a banner offers **Reload**. Pages' own caching headers can't be changed,
  so the page has to ask.

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

### 14.6 Visitor counter

The published web app shows a counter from freevisitorcounters.com at the bottom
of the left panel. index.html holds the counter's code. It runs only when the page
is served from `sys-bio.github.io`, so copies hosted elsewhere don't show it or
count their visits. Adding `?counter` to the address shows it anywhere, for
testing. The desktop app has no counter and never contacts the service.

- The counter's scripts must run while the page loads, because one of them uses
  `document.write`. So index.html writes them in with `document.write` too, and
  only on the published site.
- The service's other script checks that its sponsor link is on the page and
  visible when the page finishes loading, then empties the link. If the link is
  missing or hidden, the counter shows an error instead. Don't remove or hide it.
- When the counter is shown, index.html marks the page with `data-counter`. The
  app then stops the model tree 72 points short of the panel's bottom. Each frame
  it puts the counter's box over that gap (`web::place_counter`, changing the
  page only when the gap moves).
- sw.js caches only the app's own files, so the counter is never cached and is
  simply missing when offline.

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
| **R10** | M10 Graphics output | SVG, PNG via resvg, vector PDF, printing in the browser (§9) |
| **R11** | M11 Documents and polish | Web file adapters (§10.1), unsaved-change tracking and prompts, autosave and recovery (§10.2), panic handling in the browser (§14.4), AccessKit, the model tree panel |
| **R12** | M12 Web release | The Pages deployment and service worker (§14.2), the `THIRD-PARTY` file, and the S §17.4 checklist passing in the browser (`docs/release-checklist.md`) |

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
- **Packaging (§14.1):** installers or packages for Windows, macOS and Linux,
  built in CI on tags (unsigned, by decision).
**Status of D1 (2026-10-10):**
- *Done:* native file dialogs, Save to the remembered path, drag-and-drop with the
  path (R11); a file named on the command line opens at startup (as when a file
  is opened with the app); **File ▸ Open Recent** (eight files, kept in eframe's
  storage, a missing file is taken off the list); the unsaved-changes prompt on
  closing the window (R11); autosave recovery (R11: on the desktop eframe writes
  its storage to disk at every autosave, every 10 seconds, so after a crash the
  changes up to the last autosave are offered back at the next start, and a
  crash dialog says so); preferences in eframe's storage file in the app's data
  folder; printing through the system PDF viewer (R10); Cmd shortcuts on macOS
  (`Modifiers::COMMAND`); the window icon; packaging (§14.1); the **macOS menu
  bar** (`crates/pd-app/src/macos_menu.rs`, with the `muda` crate): the
  application menu (About PathwayDesigner, Settings… Cmd+,, Services, Hide, Hide
  Others, Show All, Quit PathwayDesigner Cmd+Q) and a Window menu (Minimize, Zoom,
  Full Screen). Quit is the app's own item, so it asks about unsaved changes as
  closing the window does (the system's Quit ends the app at once). There is no
  native Edit menu: its key equivalents would take Cmd+C, Cmd+V, Cmd+Z and Cmd+A
  from egui. On macOS the in-window menus leave out Preferences, Quit and the Help
  menu, which the application menu has. CI lints and tests the app on a Mac
  runner (job `macos`).
- *Decided against:* signing and notarisation (the packages are unsigned).
- *Not yet:* the acceptance checks below on macOS and Linux (the macOS menu
  bar has only been built in CI, not tried on a Mac).
- **Acceptance:**
  - the S §17.4 checklist passes in the desktop build on all three operating systems;
  - the desktop budgets in §5.6 are met;
  - a file saved by the desktop build opens unchanged in the browser build, and the
    other way round;
  - a v3 file opens unchanged in the desktop build and in the Delphi app, both ways
    round.

### 15.2 After matching the reference

In order of value. These follow S §16.2.

1. **Draw compartments** (F-2). **Done (S §7.9).** The model and its styles already exist and are saved.
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
