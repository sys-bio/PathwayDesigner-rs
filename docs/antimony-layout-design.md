# Reading Antimony's layout statements — design

**Status: decided 2026-10-22 (§11); nothing implemented yet.**

## 1. Why

Antimony 3 (libAntimony 3.0.0, June 2025; Heydarabadipour et al., PLOS Comput
Biol 2026) describes a diagram with dotted statements in the model text:

```
J0: S1 -> S2; k1*S1
model.layout = on
S1.pos = {28, 35}
J0.S1.b1 = {55, 31}
species.color = azure
```

libAntimony converts them to and from SBML Layout and Render through SBMLNetwork.
The syntax is documented in the Antimony reference, section "Layout and Render"
(https://tellurium.readthedocs.io/en/latest/antimony.html).

**Today PathwayDesigner can't read such a model at all** (found 2026-10-22):

- The `antimony` crate's lexer treats the word `layout`, when not followed by
  `{`, as the start of a `layout … end` block and swallows everything up to the
  next `end` — the rest of the file, or the model's own `end`. Initial values
  after it are lost, and inside `model … end` the model is reported unclosed.
  This block form came over from the Delphi lexer; no such block exists in
  Antimony's documentation, and none of the test models has one.
- `model.layout = on` stops the parser ("Expected an identifier, found '.'"), and
  `{28, 35}` is not a value it knows.

**Goal (the author's choice, option B):** read these statements and use them: the
drawing comes from them, as it comes from SBML Layout and Render. Writing them is
a later step (§9): the author's plan is that users may use either form — these
statements, or the readable diagram format of `docs/diagram-format.md` — and that
PathwayDesigner can convert its own format to Antimony's.

## 2. The syntax

From the reference (statements end at a line break or `;`, as elsewhere in
Antimony; `#` and `//` start comments):

| Statement | Meaning |
|---|---|
| `model.layout = on` | Turn layout on (only needed when nothing else is set; `off` is an error) |
| `model.autolayout.maxNumConnectedEdges = n` | Autolayout option: alias a species above n connections (default 3) |
| `model.layout.style = calm` | One of SBMLNetwork's 24 predefined styles (`default`, `calm`, `escher`, …) |
| `model.layout.align_top = {S1, S2}` | Align nodes (also `_bottom`, `_left`, `_right`, `_hCenter`, `_vCenter`, `_circular`) |
| `model.layout.size = {w, h}`, `.width`, `.height` | Size of the whole layout |
| `model.layout.background = c` | Background colour |
| `model.layout.<look> = v` | Default look of everything (below) |
| `species.<x> = v`, `reaction.<x> = v`, `compartment.<x> = v` | Default for a whole category |
| `X.pos = {x, y}` (also `position`), `X.x`, `X.y` | Position of a species, reaction or compartment |
| `X.size = {w, h}`, `X.width`, `X.height` | Size |
| `X.<look> = v` | An item's own look; overrides its category |
| `J0.S1.species_end`, `.rxn_end`, `.b1`, `.b2` | The line between reaction J0 and species S1: its ends and two Bézier control points |
| `J0.S1.arc2.…` | The second line to S1 (stoichiometry above one) |
| `J0.S1.seg2.…` | The second segment of a line drawn in several pieces (libAntimony 3.1.3) |
| `J0.--.position`, `.size`, `.species_end`, `.b1`, `.b2` | The null node of a reaction with an empty side, and its line |
| `S1.position.J3.J4 = {x, y}`, `S1.size.J3.J4` | An alias of S1, drawn for reactions J3 and J4 |

Looks: `color` (or `fillcolor`), `linecolor` (`strokecolor`), `linewidth`
(`strokewidth`), `fontcolor`, `font` (`serif`, `sans_serif`, `monospace` or any
name), `fontsize`, `fontstyle` (`fontweight`: `normal`, `bold`, `italic`,
`bold_italic`), `shape` (`rectangle`, `square`, `ellipse`, `circle`, `triangle`,
`diamond`, `pentagon`, `hexagon`, `octagon`). Colours are web colour names or
`"#RRGGBB"` / `"#RRGGBBAA"` in quotes.

**What libAntimony 3.0.0 actually does** (probe, 2026-10-22, python `antimony`
3.0.0, the version installed here; script kept in the session scratchpad):

- `S1.pos = {100, 200}` with size 60 × 36 gives a species glyph whose bounding box
  is at (100, 200): **the position is the top-left corner**, not the centre.
- `J0.position = {200, 210}` gives a reaction glyph box at (200, 210), 20 × 20, and
  **every arc starts at its centre**, (210, 220).
- Each arc is one cubic Bézier **from the reaction to the species**, reactants
  included: start = `rxn_end` (default the reaction's centre), end =
  `species_end` (a point on the species' border), `b1` the control point on the
  reaction's side, `b2` on the species' side.
- Its writer uses `S1.position` (not `pos`), writes `model.layout.size`, and writes
  every arc in full, with two decimals.
- 3.0.0 has faults with hand-written arcs: giving only `J0.S1.b1`/`b2` produced a
  broken first arc at (0, 0) and an extra arc; an inhibitor's arc was written
  twice (as modifier and inhibitor). 3.1.0 "improved layout/render support" and
  3.1.3 added segments.

**Checked again with libAntimony 3.2.1** (installed 2026-10-22, D7):

- Positions, sizes, the 20 × 20 reaction box and the arc direction are as above.
- Giving only `b1`/`b2` for an arc still gives a broken arc from (0, 0) when the
  species has stoichiometry above one (an extra arc is made for the second
  copy). Tests against libAntimony give every arc in full.
- **Segments are numbered from the species.** The first segment (written without
  `seg1` by libAntimony's writer) ends at `species_end`; its `rxn_end` is the first
  corner; the last segment starts at the reaction's centre. Each segment runs
  from its reaction side to its species side. The reference's example agrees;
  its prose ("the species_end side of each segment is the same as the reaction
  side of the next") has it the other way round.
- `S1.position.J2 = {…}` makes a second glyph of S1, which J2's line uses; other
  reactions use the first.
- `J0.--` is the null node of J0's empty side; libAntimony writes its look as
  `J0.--.color = white`, `J0.--.lineColor = black`, `J0.--.lineWidth = 2`: **its
  writer uses `lineColor`/`lineWidth`, the reference `linecolor`/`linewidth`**, so
  setting names are read without regard to case.
- Species without a position are placed by SBMLNetwork's autolayout; a
  `default_compartment` glyph is added around everything.

## 3. Where it goes

- **`antimony` crate:** lexes and parses the statements and keeps them, in order,
  as data: a `LayoutStatement { path: Vec<PathPart>, value: LayoutValue, span }`
  list on the module, where a path part is an id, `--`, `arcN` or `segN`, and a
  value is a number, a point `{x, y}`, an id list `{S1, S2}`, a word (`on`, `red`,
  `ellipse`) or a string. The crate stays independent of PathwayDesigner: it knows
  the grammar of these statements, not what they draw. It writes them back as
  read, so a model loaded and saved by the crate keeps them.
- **`pd-core::antimony`:** turns the statements into the drawing (§4–§6), with a
  warning for each one it can't use, and reports whether the file had a layout,
  as SBML import does (`had_layout`, `unplaced`).

**Lexer:** remove the `layout … end` and `layout { … }` block rule; `layout` becomes
an ordinary identifier (D1). **Parser:** a statement whose first identifier is
`model`, `species`, `reaction` or `compartment` followed by `.`, or `X.` where X is
a species, reaction or compartment of the model (not a submodule instance — `A.x =
3` already means a submodule's variable), is a layout statement. The second test
needs the symbol table, so the parser takes the dotted assignment as written and
the semantic pass decides which it is.

## 4. Positions and sizes

| Antimony | PathwayDesigner |
|---|---|
| Species `pos` + `size` | Centre = `pos` + size / 2; size as given |
| Species with `pos` but no `size` | Size 60 × 36, libAntimony's (D2); centre = `pos` + (30, 18) |
| Category `species.size` | Each species' size, unless its own is given |
| Reaction `pos` (+ `size`, default 20 × 20) | Junction = `pos` + size / 2 |
| Compartment `pos` + `size` | Box at `pos`, that size |
| Compartment `size` alone | Box at (0, 0), as libAntimony reads it (its writer leaves out a zero position) |
| Compartment `pos` alone | No box: fitted around its species, as a compartment without one is |
| `J0.--.position` (+ `size`, default 30 × 30, libAntimony's) | The null node, centred as for a species; a null node without a position goes beside the junction, as the import puts it |
| `S1.position.J3.J4` (+ `S1.size.J3.J4`) | An alias of S1 at that place, used by J3's and J4's lines to S1. libAntimony 3.2.1 attaches it to J3 only, against its reference (§8) |
| The default compartment | No box, as SBML import does |
| `x` alone or `y` alone | The other is 0, as the reference says |
| Species or reaction with no position | Placed as SBML import places a species with no glyph, and auto-layout offered |
| `model.layout.size` | Not used (the canvas has no size); no warning |

**As built (step 2, 2026-10-22):** `pd-core/src/antimony_layout.rs`, applied after
the import has built the model; without layout statements the import is exactly
as before. `antimony::Imported` gains `had_layout` and `unplaced`.

## 5. Lines

**As built (step 3, 2026-10-22).** A line's statements are an SBML species
reference glyph's curve written out, and are read back into one: each segment
an SBML segment, the curve read by the same code as SBML import
(`sbml::import::leg_from_curve`, `null_leg_handles`, `modifier_points`, taken
out of SBML import for this). So a reactant's or product's line gets corner
points where its segments meet and Bézier handles from the piece at the species
(or the whole curve's, carried on to the node's centre when it passes through
it); a modifier's gets corner points only, as from SBML; a null node's line its
handles. A reaction with handles on any line is made Bézier (D3). Further arcs
(`arc2`, …) are skipped with one warning per model; a bent piece away from the
species is drawn straight, with one warning.

**libAntimony's conventions** (found against libAntimony 3.2.1 with
`scripts/antimony_layout_check.py`; the reference doesn't say them):

- **The names are positions on the SBML segment, not ends of the line.**
  `rxn_end` is the segment's start and `species_end` its end, `b1` the base
  point beside its start and `b2` beside its end, whichever way the line was
  drawn. libAntimony's own layouts run from the reaction to the species, so
  there the names fit; PathwayDesigner's SBML draws a reactant's line from the
  species, so libAntimony writes that line with `rxn_end` on the species'
  border. Which end is the species' is therefore decided by where the ends
  are, as SBML import does.
- **It leaves out an end it can work out again:** the reaction's centre, and
  the point where the straight line from the species' centre to the line's
  next point leaves the species' box. A missing end is read as whichever of
  the two is farther from the other end. With both missing (a straight line,
  or a person writing only `b1` and `b2`), the line starts at the junction, as
  the reference says, unless its `b1` is nearer the species than its `b2`.
- **It writes a straight segment's base points as `{0, 0}`**: both at the
  origin means no handles.
- **It rounds to two decimals**, so a curve carried on to a node's centre is
  allowed to miss it by 0.1.
- **Segments** are listed in path order by libAntimony, and from the species in
  the reference's example; the order is told by the ends neighbouring segments
  share.
- **Its own reading of these statements is not to be relied on:** an end it
  left out comes back at (0, 0), or at another reaction's place.

## 6. Looks

Each look is resolved to what the item would show — its own setting, else its
category's (`species.`), else `model.layout.` — and written into the item's own
style, as SBML Render import does today (D4).

| Antimony | PathwayDesigner |
|---|---|
| `color` | fill |
| `linecolor`, `linewidth` | border colour and width (species, compartments); line colour and width (reactions) |
| `fontcolor`, `fontsize` | label colour, font size |
| `fontstyle` | bold, italic |
| `font` | sans or serif: `serif` and serif names → serif, everything else → sans (D5) |
| `shape` | `rectangle`, `ellipse`, `circle`, `hexagon` as they are; `square` → rectangle with equal sides; `triangle`, `diamond`, `pentagon`, `octagon` → rectangle, with a warning (D5) |
| Reaction `shape` (the centroid square) | Not drawn by PathwayDesigner; ignored |
| `model.layout.background` | Background colour |
| Colour names | The CSS colour names (148), as a table in pd-core |
| `"#RRGGBBAA"` | Colour with alpha (`Rgba` has it) |
| `model.layout.style = …` | `escher` → the Escher theme; any other: not known, warning (D6) |
| `model.layout.align_…`, `model.autolayout.…` | Instructions to SBMLNetwork's autolayout, not positions; ignored, with a warning when no positions are given either |

## 7. In the app

- **Opening a file** (`.ant`, `.txt`) with layout statements: the drawing comes
  from them, as from an SBML file with a layout; without them, as now.
- **The Antimony panel** shows the model without layout statements (as now, until
  §9). If the user types or pastes layout statements into it and applies, they
  are applied to the drawing, as if read from a file; items they don't mention
  keep their place.
- **Paper:** the Methods sentence ("PathwayDesigner does not read or write
  Antimony's layout statements") becomes "reads … but does not yet write".

## 8. Testing

- **Crate:** lexer and parser tests for every statement form in §2, including the
  reference's full example and the probe's output; `layout` as an ordinary id
  (`layout = 3`, `model.layout = on` followed by values that must not be lost,
  inside `model … end`); a submodule's `A.x = 3` is still a submodule assignment.
  The corpus checks (1788 files, BioModels) must stay as they are.
- **pd-core:** each row of §4–§6 as a test; warnings for each ignored form.
- **Against libAntimony (the oracle):** `scripts/antimony_layout_check.py`. For
  every Gallery model, two Antimony texts with layout statements: libAntimony's
  own layout of the model, and PathwayDesigner's layout as libAntimony writes it.
  Each text is read by PathwayDesigner, and by libAntimony into SBML that
  PathwayDesigner's SBML import reads; the drawings must agree (node centres and
  sizes, which drawing each line goes to, junctions, compartment boxes). The
  reference is libAntimony's reading of the text, not the SBML the text was
  written from: libAntimony's writer leaves out zero positions and some sizes,
  and can write two drawings of a species for one reaction under one name.
  SBMLNetwork's automatic aliasing (`maxNumConnectedEdges`, default 3) is turned
  off for the reference, since it adds drawings the text doesn't have.
  `crates/pd-core/examples/drawing_report.rs` prints a drawing for it.

  Step 2 result (libAntimony 3.2.1, two minutes per model): 12 Gallery models
  agree on both routes; e_coli_core, the Millard model and the Escher core map
  ran out of time in libAntimony. The thirteenth, yeast glycolysis, shows one
  known libAntimony fault: it attaches an alias such as `S1.position.J2.J3` to the first reaction
  listed only (checked on a three-reaction model), where the reference and
  PathwayDesigner attach it to all; the yeast glycolysis model shows it. Also
  out of reach: a species drawn twice for one reaction (stoichiometry 2 split
  over two drawings), since PathwayDesigner draws one line per participant.

  Step 3 (lines compared with the SBML each text was written from; positions
  still with libAntimony's reading): the same 12 models agree on both routes,
  with the lines of reactions whose junction libAntimony itself moves (a
  zero-size reaction box, read back as 20 x 20) left out of the comparison and
  counted. The yeast model's differences are the alias fault above and one
  line of a species with stoichiometry 2 drawn to two drawings.
- **Mutation:** break the top-left → centre offset, swap `b1`/`b2`, drop the
  category fallback, restore the old block rule; each must fail a test.

## 9. Later: writing them

Not in this work. When PathwayDesigner writes Antimony, it could add the layout as
these statements — positions as top-left corners, every line's `b1`/`b2`,
corner points as segments, looks as item settings — so that the drawing survives
in Antimony itself and libAntimony can turn it into SBML Layout. The author's plan
is that both forms are offered: these statements, which Antimony tools read, and
the readable diagram format, with a conversion from ours to Antimony's. What the
statements can't hold (Linear reactions, smooth junctions, stems, styles by name,
hidden compartments, text) would be lost in that direction, and the conversion
would say so.

## 10. Steps

1. Lexer and parser: remove the block rule; parse and keep the statements; the
   crate writes them back. (Fixes the lost-values fault on its own.)
2. Positions and sizes (§4): species, reactions, compartments, aliases, null
   nodes.
3. Lines (§5): handles, segments, modifiers, null-node lines.
4. Looks (§6).
5. The app (§7), docs (SPECIFICATION, SPEC-Rust, README) and the paper's Methods
   sentence.

## 11. Decisions (the author, 2026-10-22)

- **D1.** The `layout … end` / `layout { … }` block rule is removed; `layout` is an
  ordinary identifier.
- **D2.** A species with a position but no size has Antimony's 60 × 36, so that
  PathwayDesigner draws what libAntimony would.
- **D3.** A reaction with handles on any line becomes `curved`; one without stays
  `straight`.
- **D4.** Looks go into each item's own style, as SBML Render import does.
- **D5.** Triangle, diamond, pentagon and octagon become rectangles with a warning.
  Of these, only the octagon and the pentagon are worth adding as shapes later.
- **D6.** `model.layout.style = escher` maps to the Escher theme; the other 23
  SBMLNetwork styles are ignored with a warning.
- **D7.** libAntimony upgraded in place to 3.2.1 (the author: "it should be updated
  anyway"); it is the test oracle.
