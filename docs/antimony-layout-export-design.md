# Writing the layout as Antimony — design

**Status: decided 2026-10-22 (the author agreed D1–D6); nothing implemented yet.**

## 1. Why

PathwayDesigner reads Antimony 3's layout statements (`S1.position = {…}`,
`J0.S1.b1 = {…}`, `S1.color = …`; `docs/antimony-layout-design.md`), but its
Antimony export still writes the model alone (S §11.3). So a diagram that goes
out as Antimony comes back, in PathwayDesigner or libAntimony, laid out from
scratch. This step writes the drawing too, so that:

- PathwayDesigner reads its own Antimony back as the same drawing, as it does
  its SBML (the round trip of §8);
- libAntimony, Tellurium and anything built on them (SBMLNetwork, the Antimony
  panel in other tools) draw the model as PathwayDesigner drew it, and can turn
  it into SBML Layout and Render.

It is also the first half of the author's plan for the two layout forms: users
may use either Antimony's statements or the readable diagram format
(`docs/diagram-format.md`), with a conversion from ours to Antimony's (§9).

## 2. What is written

A section at the end of the model, before its `end`, introduced as the other
sections are (S §11.3):

```
  // Layout
  model.layout = on
  model.autolayout.maxNumConnectedEdges = 1000

  species.size = {80, 36}
  species.color = "#EEF6FF"
  species.linecolor = "#4A7FCB"
  …
  S1.position = {60, 102}
  S1.position.J2 = {60, 302}
  J0.position = {200, 110}
  J0.S1.species_end = {140, 120}
  J0.S1.b1 = {185, 120}
  J0.S1.b2 = {155, 120}
  …
```

`maxNumConnectedEdges` is raised so that libAntimony's automatic aliasing adds no
drawings to a layout that places every species (libAntimony finding 5); it is
left out when some species have no place (none do in a PathwayDesigner diagram).

## 3. Where the numbers come from

**The SBML export's layout, written as statements.** Reading treats a line's
statements as an SBML Layout curve written out (`antimony_layout.rs`); writing
does the inverse. `sbml::export` already works out everything a reader needs:
each glyph's box, each species reference glyph's curve cut at the node's border
and split into segments at corner points, the reaction glyph's box and stem, and
the Render style of each glyph. So the writer asks it for the `sbml_io` layout
(a new `pub(crate)` function beside `export`, returning the document it builds
now) and translates glyphs into statements. Nothing is computed twice, and the
Antimony drawing is the SBML drawing by construction.

Ids: SBML export turns an id that isn't a valid SId into one (`sid`, D-13). The
Antimony writer already writes the model's ids as they are; the layout section
maps each glyph back to its item through the export's id table, so the two
sections name the same things.

## 4. Positions and sizes

| Item | Written |
|---|---|
| Species (primary) | `S.position` = top-left corner of its box, `S.size` (the default 60 × 36 is libAntimony's, not ours, so the size is always written, or once as `species.size` when most share it) |
| Alias | `S.position.R1.R2` and `S.size.R1.R2`, the reactions whose lines go to it, in the model's order |
| Null node | `J.--.position`, `J.--.size` (default 30 × 30 is libAntimony's; ours is 28 × 28) |
| Reaction | `J.position` = the junction less half of the default 20 × 20 box, so the box's centre is the junction; no size (a size of 0 is dropped by libAntimony's writer and read as 20 × 20 — finding 4) |
| Compartment | `C.position`, `C.size` from its box; a hidden compartment and the default compartment are not written |
| Linear reaction | its junction, on the line, as for any reaction (its lines are below) |

Known loss: an alias that serves several reactions is attached by libAntimony
3.2.1 to the first one only (finding 1); PathwayDesigner reads it as written.

**As built (step 1, 2026-10-22):** `pd-core/src/antimony_layout_out.rs`, and
`antimony::export_with_layout` (the model, then a `// Layout` section);
`antimony::export` still writes the model alone. Positions come from the model
itself (the same boxes SBML export writes); the SBML layout is used from step 2,
for the lines. `layout_report --antimony-layout` exports every Gallery model so
and reads it back, beside the SBML round trip; what positions lose:

- a hidden compartment's box (Antimony can't say hidden; SBML loses it too);
- an alias no line goes to (Antimony names an alias by its reactions; three in
  the Millard model);
- a Linear reaction's stored junction: its drawn one is written, and comes back.

Two fixes to reading came out of it: an item whose id is a kind's name (the
Millard model's compartment `compartment`) takes `compartment.position` and
`compartment.size`, which no kind has; and the automatic-layout warning is
given only when the text places nothing, since our own export sets
`maxNumConnectedEdges`. libAntimony 3.2.1 reads the exports of the MAPK cascade,
the TCA cycle and the Millard model without error (the last in 6 s, where its
own automatic layout took over two minutes).

## 5. Lines

Every line is written, straight ones included, so that libAntimony draws ours
and doesn't make its own. Each is written **from the reaction to the species**,
the way libAntimony's own layouts run, so the names mean what the reference says
(finding 8):

- One segment: `species_end` (where the line meets the node's border, from the
  SBML curve), `b1` and `b2`. `rxn_end` is left out when the line starts at the
  junction (libAntimony reads that right); it is written when it doesn't (a stem,
  or a modifier line, which ends short of the junction).
- **`species_end` is always written**: libAntimony reads a line with
  `rxn_end` and no `species_end` as ending at (0, 0) (finding 2).
- **A straight segment** is written with `b1` and `b2` a third and two thirds
  along it, never `{0, 0}`, which libAntimony's writer uses for "none" and which
  can't be told from a real point (finding 4).
- **Corner points:** one segment per piece, numbered from the reaction
  (`seg1` at the reaction, the last at the species, SBML order), each with both
  its ends written, so that either order a reader assumes gives the same path
  (finding 10). Needs libAntimony 3.1.3 or later.
- **Modifier lines:** `J.M.…`, with `rxn_end` where the line meets the
  reaction, and segments for corner points as above. Straight, as drawn.
- **Null nodes' lines:** `J.--.species_end`, `b1`, `b2`.
- **Stems** (S §5.13) can't be said: libAntimony has one point per reaction. The
  lines are written to the stem's ends, so the drawing is the same, but the stem
  comes back as lines meeting at the junction. One note says so.
- **Smooth junctions** come back drawn the same (their handles are written), but
  not marked smooth.

**As built (step 2, 2026-10-22).** Each line comes from SBML export's own
curve (`sbml::export::leg_segments`, taken out of SBML export, whose output is
byte for byte unchanged), turned to run from the reaction. Changes to the plan,
from trying libAntimony:

- **Both ends are always written.** In the Millard model libAntimony started a
  line without `rxn_end` far from its reaction ((10342.65, 5015.28) for PGI's),
  though smaller models read right.
- **Base points are always written.** libAntimony draws a segment with ends but
  no base points as a curve towards the origin, and without any line
  statements draws its own slightly curved lines.
- **An alias's size is always written:** libAntimony doesn't give an alias
  `species.size`.

And to reading:

- a segment with base points at a third and two thirds is straight, so a
  straight reaction comes back straight (any other base points are a curve's,
  even on the line: a curved reaction's automatic handles often are);
- a missing end is the junction or the species' border point, whichever is
  farther from the given end (it was measured to the centre, which went wrong
  when the junction is close), and a point on the border counts as on it;
- **stems are read back**: when every reactant's line starts at one point and
  every product's at another, those are the stem's ends.

Result (`layout_report --antimony-layout`): positions, sizes, curves, corner
points, modifier routes and stems come back over the Gallery, curves within 0.034
(two decimals); lost are the smooth-junction flag (49 reactions; drawn the same),
Linear (29; drawn as two straight lines meeting at the junction) and the losses
of step 1. libAntimony 3.2.1 reads the exports and draws every line as
PathwayDesigner's SBML does (MAPK cascade, upper glycolysis, the Millard model:
same segment ends, to rounding), except a second line to one species (8 in the
Millard model), which Antimony can't name. libAntimony's `loadAntimonyFile`
fails on the Millard export (79 KB, "Failure reading input stream") that
`loadAntimonyString` reads.

## 6. Looks

**The look as drawn, not only what was changed.** libAntimony fills what a text
leaves out with SBMLNetwork's defaults (white junctions, 8-point reaction names,
its own colours), which aren't PathwayDesigner's. So the writer states the look
every item is drawn with:

- **By kind first:** the look most species share is written once as `species.…`,
  likewise `reaction.…` and `compartment.…`; then each item only where it
  differs. A diagram in one style is a handful of lines.
- **Mapping** (the inverse of reading, `antimony_looks.rs`):

| PathwayDesigner | Written |
|---|---|
| Species fill, border colour and width | `color`, `linecolor`, `linewidth` (a boundary species' border width multiplied back by its factor) |
| Name colour, size, bold, italic | `fontcolor`, `fontsize`, `fontstyle` |
| Shape | `rectangle` (also for the rounded rectangle, which libAntimony doesn't have), `ellipse`, `circle`, `hexagon` |
| An alias's look | `S.color.R1.R2` and the like, as libAntimony writes it |
| Reaction line colour and width | `linecolor`, `linewidth` |
| Junction colour | `color` (a hidden junction can't be said; written in the line colour) |
| Modifier line colour and width | `J.M.linecolor`, `J.M.linewidth` |
| Compartment fill, border | `color`, `linecolor`, `linewidth` |
| Background | `model.layout.background` |

- **Colours** are written as a name from libAntimony's table when one matches
  exactly (as its writer does: `"#C77434"` is `ochre`), else as `"#RRGGBB"` or
  `"#RRGGBBAA"`. `orchid` is never written as a name (finding 6).
- **Not written** (no statement exists): names outside nodes and where they are,
  shown reaction names, dashed or dotted lines and borders, arrowhead and
  modifier symbol sizes, free text, the theme (its look is written out instead),
  saved styles. One note lists what was left out, as SBML export's notes do.

**As built (step 3, 2026-10-23).** Every look is the one the renderer draws
(`draw::species_colours`, `draw::line_colour`, `draw::label_size` and the like,
the functions SBML Render export uses). Changes to the plan:

- **The rounded rectangle**, which libAntimony has no name for, is written as
  nothing (not as `rectangle`, which reads back square-cornered); and a kind's
  value is only written where every item can say its own (otherwise a rounded
  rectangle among circles would inherit `species.shape = circle`).
- **No border** is written `linewidth = 0`, and read so.
- **A hidden junction** is written `color = transparent`, and read as hidden; a
  Linear reaction comes back Straight with its junction hidden, drawn the same.
- **A compartment's name size** is written (`fontsize`) and read.
- **The default background and a modifier line's default width** come back as
  the defaults.

Result (`layout_report --antimony-layout`): every look field comes back as it
does through SBML (values set by hand to their default come back as the default
in both), apart from what Antimony can't say: dashes, names outside nodes,
saved styles and an item's link to one, modifier symbols. libAntimony 3.2.1
draws the looks written (MAPK cascade, TCA cycle, upper glycolysis in the Escher
theme, repressilator), apart from: `reaction.fontsize` (by kind) not applied to
reaction names, though `J1.fontsize` is; aliases' looks where its alias fault
moves an alias's lines (finding 1); and hexagons, which PathwayDesigner's SBML
import doesn't recognise as SBMLNetwork draws them.

## 7. In the app

- **File ▸ Export Antimony** writes the layout section (D1).
- **The Antimony panel's Get Antimony** writes the model alone, as now, with a
  *With layout* checkbox beside it: the panel is for reading and editing the
  model, and a layout section of a few hundred lines would bury it (D1).
- Text with a layout section loaded back in the panel draws as written, as it
  does today.

**As built (step 4).** File ▸ Export Antimony writes the layout and then shows
what the statements couldn't say, as notes; the panel has the *With layout*
checkbox. The check script's third route reads the export with libAntimony
(`loadAntimonyString`, since `loadAntimonyFile` fails on large files, finding
16) and compares its SBML with ours at the level of the geometry. It found that
libAntimony rejects ids it reserves (`model model`, `compartment compartment`
in the yeast glycolysis model), which PathwayDesigner's reader accepts; the
export now writes such ids with `_` added (SPECIFICATION §11.3), and all 16
Gallery models agree, apart from the known findings and losses. libAntimony
leaves a parse message in `getLastError()` for every hex colour though the
load succeeds (finding 20); the script goes by the load's result.

## 8. Testing

- **Round trip through PathwayDesigner:** every Gallery model exported as
  Antimony and read back gives the same drawing: positions, sizes, handles and
  corner points to the precision written (two decimals, D4), looks equal (what
  isn't written, §5–§6, aside). Added to `layout_report` as a third mode, beside its SBML
  and model-only Antimony modes, so it prints what survives field by field.
- **libAntimony as the reader:** `scripts/antimony_layout_check.py` gains a
  third route: PathwayDesigner's Antimony export read by libAntimony into SBML,
  compared with PathwayDesigner's own SBML export of the same model. Expected
  differences: the libAntimony findings (aliases for several reactions,
  segments before 3.1.3), and the losses of §5–§6.
- **Unit tests** for each row of §4–§6, and that the model part of the text is
  unchanged; deliberate breakages as usual.

## 9. Later

- **The readable diagram format.** Once it is implemented, a conversion from it
  to these statements is this writer applied to the diagram it describes; the
  reverse is reading (already done) followed by the format's writer.
- **Writing only what differs from PathwayDesigner's defaults**, for a short
  text meant for PathwayDesigner alone (an option, if wanted).

## 10. Steps

1. The SBML export hands out its layout; positions and sizes (§4); the round-trip
   mode of `layout_report`.
2. Lines (§5).
3. Looks (§6).
4. The app (§7), the libAntimony route of the check script, SPECIFICATION §11.3,
   SPEC-Rust §8.2, README, and the paper's sentence ("does not yet write them").

## 11. Decisions (the author agreed all six, 2026-10-22)

- **D1.** File ▸ Export Antimony always writes the layout; the panel's Get
  Antimony writes the model alone unless *With layout* is ticked. (Alternatives:
  a preference for both; or always both.)
- **D2.** Every line is written, straight ones too, so libAntimony draws ours.
  (Alternative: only curved lines and corner points, a shorter text that
  libAntimony would draw with its own straight-or-curved choice.)
- **D3.** The full look is written (by kind, then differences), so libAntimony
  draws PathwayDesigner's colours. (Alternative: only looks changed from
  PathwayDesigner's defaults; shorter, but libAntimony would draw its own.)
- **D4.** Numbers rounded to two decimals, as libAntimony writes them. (Exact
  numbers make a long text; PathwayDesigner reads two decimals within its
  tolerance.)
- **D5.** Colour names when they match libAntimony's table exactly, else hex.
- **D6.** The Escher theme is written as its looks, not as
  `model.layout.style = escher`, since SBMLNetwork's Escher style isn't ours.
