# Wrapping a species' name inside its node

2026-11-01. Design; W1–W7 settled with the author, W4 changed: no name
spills out of its node. Built: SPECIFICATION §5.3 and SPEC-Rust §5.4 describe
what was made.

A species' name is drawn on one line, centred in its node. A name wider than
the node spills out of it. Reactome's maps have small nodes with long labels
and draw those labels on two or three lines: `aldolase tetramer` in a node
81 × 41, `GAPDH tetramers` in 69 × 41, `pyruvate kinase tetramer` in
119 × 36. Opened in PathwayDesigner, they overflow their nodes.

## What the user sees

- A name inside its node that is wider than the room the node gives is broken
  at spaces onto as many lines as it needs, each line centred, the lines
  together centred in the node. `aldolase tetramer` reads as `aldolase` over
  `tetramer`.
- A name that fits is drawn on one line, as now, so diagrams whose names fit
  look exactly as before.
- The node keeps its size and the name its font size: wrapping changes only
  where the words go.
- On screen, in exported pictures (PNG, SVG, PDF) and when printing, the same
  lines.

## Decisions

- **W1. The room:** the node's width less `NAME_H_MARGIN` (4) each side.
  (First `FIT_MARGIN`, 10, the margin Fit to Name leaves; but with W4 that drew
  smaller many names that sit inside their nodes today, such as `ATP` in a
  node 37 wide, in the Gallery's yeast and E. coli models. With 4, a name that
  is inside its node now is drawn as before.) A name Fit to Name has fitted
  never wraps. The same room for every shape. An ellipse, circle or hexagon
  has less room near its top and bottom, but its middle, where one or two
  lines go, is as wide as the node.
- **W2. Where lines break:** at spaces only, filling each line with as many
  words as fit. A single word wider than the room is not broken; it overflows,
  as a long name does now. *Recommended*; breaking inside words
  (`Glucose-6-` / `phosphate`) can come later if wanted.
- **W3. Line spacing:** that of free text (S §7.8), the font's own.
- **W4. No name spills out of its node** (the author: "I don't want to see
  any text overflowing the node"). When the lines are too many for the node's
  height, or a word is wider than the room, the name is drawn smaller, by half
  a point at a time, until it fits: wrapped again at each size, in the room's
  width and the node's height less `NAME_V_MARGIN` (2) above and below. It is
  never drawn smaller than `MIN_NAME_SIZE` (6); a node too small for that is
  the one case left, and Fit to Name, or a larger node, is the cure. The size
  set for the name is kept: only the drawing is smaller.
- **W5. What is not wrapped:** names drawn outside the node (S §5.11), which
  have no box to fit; reaction names; compartment names; free text, which has
  its own wrapping.
- **W6. Fit to Name:** unchanged: it widens the node to fit the name on one
  line.
- **W7. Files:** nothing new is saved. Wrapping follows from the node's size,
  the name and its font, so native files, SBML and the diagram format are
  unchanged. In SBML Layout the name's text glyph already has the node's box,
  so a tool that wraps text to its box can do the same.

## Steps

1. **Core.** Breaking a name into lines for a node (`pd-core::render`), and
   drawing them, which gives the screen, PNG, SVG, PDF and printing at once.
   Tests, broken on purpose; the Reactome glycolysis map checked.
2. **Docs.** SPECIFICATION §5.3 (the Rust version note), SPEC-Rust, the
   README if worth a line. Shown in the browser.
