# PathwayDesigner

A drawing tool for biochemical reaction networks. Draw species and reactions,
shape the reactions with straight lines or Bézier curves, give them rate laws,
lay the network out automatically, and exchange it with the rest of the systems
biology world as Antimony or SBML with layout.

It is written in Rust with [egui](https://github.com/emilk/egui), and runs in the
browser. A desktop version will follow from the same code.

**Try it:** https://sys-bio.github.io/PathwayDesigner-rs/

![PathwayDesigner in the browser](docs/screenshot.png)

This is a re-implementation of the Delphi PathwayDesigner. It does everything the
Delphi version does, and some more (see [Status](#status)).

## Features

**Drawing and editing**
- Species, and reactions of any shape: A → B, A + B → C, A → B + C, A + B → C + D,
  with stoichiometry.
- Three ways to draw a reaction: **Straight** (legs meet at a junction), **Linear**
  (one straight line, for A → B), and **Bézier** curves with handles you can drag,
  optionally with a smooth junction.
- **Aliases** (a second drawing of the same species), **modifiers** (drawn as
  dashed arcs, found from the rate law), and **null nodes** for sources and sinks.
- An inspector for colours, line widths, initial values, boundary species and
  rate laws, with errors in a rate law shown as you type.
- A model tree listing every species and reaction, usable from the keyboard.
- Undo and redo for every change.

**Layout**
- Force-directed **auto-layout**, animated, with an optional pass that shapes the
  Bézier curves. Locked nodes stay where they are.
- **Align and distribute** species and junctions, and nudge the selection with the
  arrow keys.
- A random network generator, for trying things out.

**Files**
- PathwayDesigner's own JSON format, compatible with the Delphi version's files.
- **Antimony**: import and export, with a live Antimony panel next to the drawing.
- **SBML** Level 2 and Level 3: import and export, including the **Layout**
  package, so positions, aliases and curves survive the round trip.
- **Graphics**: SVG, PNG at any resolution, and vector PDF (Letter, A4 or the
  drawing's own size), and printing.

**In the browser**
- Works offline once it has been opened. Saves straight back to the file you
  opened (in Chromium-based browsers), autosaves, and offers your changes back if
  the tab was closed or the app crashed.
- Says when a new version has been published.

## Host it yourself

The `gh-pages` branch of this repository is the finished website. Copy its files
into any folder of your web server and open `index.html`, as with
[MakeSBML](https://github.com/sys-bio/makesbml): there is nothing to build or
install. Download it with Code ▸ Download ZIP (with the `gh-pages` branch
selected), or

```
git clone --branch gh-pages --depth 1 https://github.com/sys-bio/PathwayDesigner-rs.git pathwaydesigner
```

Each [release](https://github.com/sys-bio/PathwayDesigner-rs/releases) also has the
same files as `PathwayDesigner-web-<version>.zip`, next to the desktop installers
for Windows, macOS and Linux.

## Getting started

Pick a tool on the left: **Species** adds a species where you click, and the
reaction tools (A → B and the others) add a reaction when you click its species in
turn. **Select** is for selecting, dragging and editing. Esc goes back to Select.

The toolbar shapes the selected reactions (Linear, Bézier, Straight, smooth
junction, Make Nice) and runs the auto-layout. Right-click a species or a reaction
for more: aliases, renaming, locking, alignment.

| Mouse | |
|---|---|
| Click, Shift-click | Select; add to or take from the selection |
| Drag on empty canvas | Select everything in a rectangle |
| Drag a species, junction or handle | Move it |
| Double-click a species | Rename it |
| Right-click | The menu for what is under the pointer |
| Wheel, or two-finger scroll | Pan (Shift: sideways) |
| Ctrl + wheel, or pinch | Zoom, keeping the point under the pointer still |
| Middle-button drag, or Space + drag | Pan |

| Keys (Cmd on macOS) | |
|---|---|
| Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y | Undo, redo |
| Ctrl+A | Select all (Edit also has Select All Species and Select All Reactions) |
| Delete, Backspace | Delete the selection |
| Arrow keys, Shift + arrow keys | Nudge the selection 1 unit, 10 units |
| Esc | Cancel, back to the Select tool |
| Ctrl+S, Ctrl+Shift+S | Save, Save As |
| Ctrl+O, Ctrl+N | Open, New (in a browser, Ctrl+N opens a new browser window: use File ▸ New) |

## File formats

| Format | Extension | Read | Write |
|---|---|---|---|
| PathwayDesigner diagram | `.json` | yes | yes |
| Antimony | `.ant`, `.txt` | yes | yes |
| SBML Level 2 and 3, with Layout | `.xml` | yes | Level 3 Version 1 |
| SVG, PNG, PDF | `.svg`, `.png`, `.pdf` | | yes |

`Models/` holds example models in each format. File ▸ Open sample opens the JSON
ones directly, even in the browser.

## Project layout

| Crate | What it is |
|---|---|
| [`pd-core`](crates/pd-core) | The model, geometry, drawing, interaction, undo, layout, and the JSON, Antimony and SBML bridges. No user interface or platform code, so all of it is tested without a window. |
| [`pd-app`](crates/pd-app) | The egui application, for the browser and the desktop. |
| [`pd-export`](crates/pd-export) | Graphics output: SVG, PNG (through resvg) and PDF. |
| [`pd-antimony`](crates/pd-antimony) | Reads and writes Antimony text. |
| [`sbml-io`](crates/sbml-io) | Reads and writes SBML as plain Rust data. Written to be reusable on its own: Level 2 and 3 core plus Layout, pure Rust, with everything it doesn't understand kept for a lossless round trip. See its [README](crates/sbml-io/README.md). |

## Building and testing

You need [Rust](https://rustup.rs/), the `wasm32-unknown-unknown` target
(`rustup target add wasm32-unknown-unknown`) and [Trunk](https://trunkrs.dev/).

```
cargo test --workspace                # the tests
cargo run -p pd-app                   # a desktop window
trunk serve                           # the browser version, at http://127.0.0.1:8080/
trunk build --release --public-url ./   # the web release, into dist/
```

On Windows, run Trunk from PowerShell, or from Git Bash with `MSYS_NO_PATHCONV=1`,
or Git Bash turns the `--public-url` path into a Windows path.

Desktop packages are made with [cargo-packager](https://github.com/crabnebula-dev/cargo-packager)
(`cargo install cargo-packager --locked`):

```
cargo packager --release -p pd-app --formats nsis           # Windows installer
cargo packager --release -p pd-app --formats app,dmg        # macOS
cargo packager --release -p pd-app --formats deb,appimage   # Linux
```

The packages land in `target/packages/`. Pushing a version tag (`v0.1.0`, …) has
CI build all of them and attach them to a draft GitHub release. They are not
signed, so on first start Windows asks for *More info ▸ Run anyway*, and macOS for
a right-click on the app and *Open*.

Every push runs CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)):
formatting, Clippy for the desktop and for WebAssembly, licence checks, and the
tests; then
- **SBML:** every Level 2 and Level 3 model of the
  [SBML Test Suite](https://github.com/sbmlteam/sbml-test-suite) is read and written
  back by `sbml-io`, and python-libsbml must find each one unchanged; every example
  model is exported and checked with libsbml's `checkConsistency()`;
- **graphics:** every example model's PDF is drawn by pdfium and compared with its
  PNG;
- **web:** the release build is served from a sub-path, as GitHub Pages serves it,
  and checked in headless Chromium: it starts, works offline, and shows the
  new-version notice.

Pushes to `main` that pass are published to GitHub Pages, and the built website
is written to the `gh-pages` branch.

## Status

The browser version is complete: milestones R0 to R12 of [SPEC-Rust.md](SPEC-Rust.md)
are done. The desktop release (milestone D1) is next. The conformance checklist and
its evidence are in [docs/release-checklist.md](docs/release-checklist.md).

Beyond the Delphi version, this one adds SBML import, SVG export, a drawing-sized
PDF page, the model tree, animated auto-layout, keyboard nudging, aligning
junctions, offline use and autosave, and fixes the defects listed in
SPECIFICATION.md §15.

## Documents

- [SPECIFICATION.md](SPECIFICATION.md): what the application does, written from
  the Delphi version, with every change made since recorded in it.
- [SPEC-Rust.md](SPEC-Rust.md): how it is built in Rust, the decisions taken, and
  the milestone plan.
- [docs/release-checklist.md](docs/release-checklist.md): the release checklist.

## Licence

MIT, see [LICENSE](LICENSE). The bundled Noto Sans font is under the SIL Open
Font Licence ([assets/fonts/NotoSans-OFL.txt](assets/fonts/NotoSans-OFL.txt)).
The licences of the open-source crates PathwayDesigner is built from are in
[THIRD-PARTY.html](THIRD-PARTY.html), which the app also links to from Help ▸ About.
