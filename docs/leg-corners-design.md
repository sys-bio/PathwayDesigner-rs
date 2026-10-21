# Design: corner points on reaction legs

Status: **decisions recorded** (2026-10-20): D1 (a), D2 (a), D3 as recommended.
Step 1 in progress.

## 1. Why

Escher maps drawn by programs other than Escher's editor route a reaction's lines
with right-angle bends. The MetaCarto maps of all BiGG models
(github.com/forxhunter/Awesome_visualization_Metabolic_Network) do this for about
half their reactions: in the 96 Recon3D pathway maps, 5,171 of 10,598 reactions
pass through more than the two markers Escher's editor makes. PathwayDesigner
keeps only the marker next to each metabolite, so each bent route comes out as
one straight line, and a dense map becomes a tangle of diagonals (compared side by
side in the session of 2026-10-20). Every reaction, metabolite and label is still
read; only the routes are lost.

SBML Layout can carry the same routes: a species reference glyph's curve may have
any number of segments. Today a curve of several segments from another tool is
also drawn as one piece. So the same change helps both imports.

Modifier lines already have corner points (SPECIFICATION §5.7.1): stored,
drawn, hit, edited, saved in JSON and SBML. This design gives reactant and
product legs the same, reusing that machinery.

## 2. What the maps contain

Measured on Escher's 12 published maps and the 96 Recon3D maps:

| | Published | Recon3D |
|---|---|---|
| Segments between two markers (a bend) | 0 | 23,173, **none with handles** |
| Segments at a metabolite | 6,879, all with handles | 34,144, about half with handles |
| Marker → centre segments with handles | 0 | 2,988 of 21,196 |
| Legs with no bend | all | 23,846 |
| Legs with 1, 2, 3, 4+ bends | — | 5,079, 2,833, 1,048, ~1,000 |
| Sides whose lines share only the marker next to the centre | all | 15,608 |
| Sides whose lines share a longer run | 0 | ~3,700 |

So: bends are straight pieces; only the piece at the metabolite is curved; and the
shape of a reaction is a tree from its centre out to its metabolites, where lines of
one side sometimes run together for a while before they part.

## 3. The model

**A leg's corner points.** A reactant's or product's `Participant.points` (today
used only by modifiers) holds the leg's corner points, in world coordinates, in
order **from the species** to its side's anchor (the stem's end, or the junction).
Empty: the leg is as today.

**Its pieces.** With corner points, a leg is:

1. **the end piece**, from the species' centre to the first corner point: the
   Bézier piece of today, with `ctrl1`/`ctrl2` as its handles (§5.6.1, with
   "junction" read as "first corner point"); cut at the node border, with the
   reactant gap or the arrowhead, exactly as now (§5.6.3);
2. **straight pieces** from the first corner point through the others to the
   anchor.

Without stored handles (`ctrl_pts_set` false) the end piece is straight, not fanned:
the fan offset (§5.6.2) is for lines that meet at a junction, and these don't.

Everything that draws, hits, clips or exports a leg already goes through
`bezier::leg_curve`. It keeps returning the end piece (its far end becomes the first
corner point when there is one), and a new `leg_corners` gives the straight run.
About 40 places outside the tests use `leg_curve`, `drawn_leg` or `leg_anchor`;
those that draw, hit or export also take the straight run.

**Modes.** On a Straight reaction the end piece is straight. A Linear reaction is
drawn as one line and ignores corner points, as it ignores the stem. A smooth junction smooths only legs with no corner points: a leg
with corner points meets the anchor along its last straight piece.

**The stem is unchanged** in this design: two straight pieces through the junction.
See decision D1 for runs that several legs share.

## 4. Behaviour

The rules of modifier corner points (§5.7.1), applied to legs:

- Drawn as straight pieces in the reaction's colour, width and dash. When the
  reaction is selected, each corner point is a small square, as a modifier's are.
- **Moving.** A corner point stays where it is when the species or the reaction
  moves on its own. When the species and the reaction move together (a drag or
  nudge of a selection that holds both), the leg's corner points move with them.
- **Editing.** Drag a corner point (*Move Corner Point*). Double-click a leg of a
  selected reaction to add one there (*Add Corner Point*). Right-click a corner
  point: *Remove Corner Point*. Right-click a leg: *Straighten Line* (removes its
  corner points; disabled when it has none). The current corner point and the arrow
  keys work as for modifiers. One undo step each.
- **Hitting.** Each straight piece is hit as a modifier's piece is; a click on one
  selects the reaction.
- **Handles.** The end piece's two handles are shown and dragged as now. The inner
  handle's dashed guide line runs to the first corner point instead of the junction.
- **Commands that reshape a reaction:** see decision D2.
- Copy Style, themes and saved styles leave corner points alone (a route belongs to
  one diagram), as for modifiers.

## 5. Files

- **JSON** (next version, 15): a reactant or product object may have `"points"`:
  `[x1, y1, x2, y2, …]`, as a modifier object has. Written as version 15 only when
  some leg has corner points; a file without them reads as today.
- **SBML.** A leg with corner points is written as a curve of several segments:
  the end piece as drawn (a `CubicBezier` cut at the border, as now, S §11.5),
  then a `LineSegment` per straight piece, ending at the anchor. Reading, a curve of
  several segments gives the leg corner points at the inner segment ends; the
  segment at the species keeps its handles (extended to the centre, as now); other
  Bézier segments are read as straight pieces, as modifiers' are. This also
  changes how routed curves from other tools are read: today they come in as one
  piece.
- **Render:** no change. **Antimony:** no change (no layout).
- **Diagram format** (docs/diagram-format.md §9): `leg` lines get `via <point> …`,
  as modifier `line`s have.

## 6. Escher import

The importer walks each reaction's segments as a tree from its midmarker:

- Each side's **stem end** is the marker next to the midmarker on the way to that
  side's first metabolite, as now.
- Each metabolite's **corner points** are the markers on its way to the stem end,
  in order from the metabolite (none for an Escher-editor map, so those import
  exactly as today).
- The segment at the metabolite gives the end piece's handles, as now.
- A curved marker → centre segment (2,988 in Recon3D) is drawn straight, as part of
  the stem.
- A metabolite whose way reaches the midmarker through another marker than its
  side's stem end gets the corners on its own way, then a straight piece to the
  stem end.

**Warnings.** Water and protons are left out of MetaCarto's drawings on purpose,
which today gives one warning per reaction (up to 181 for one map). This becomes
one warning per map: "h_c, h2o_c and 3 others are listed in 140 reactions but not
drawn; they were left out of them." It is a small change and goes in as its own
step.

## 7. Decisions needed

**D1. Runs that several legs share** (about 3,700 Recon3D sides).
- **(a) Repeat the run in each leg** *(recommended for now)*. The legs lie on top of
  one another along the shared run, so the drawing looks the same. Dragging a
  shared corner moves it for one leg only, and the others show where it was. No new
  concept; the stem stays as it is.
- (b) Give the stem corner points: the shared run belongs to the stem side. This
  draws and edits a shared run as one, but every stem rule in §5.13 (Straight stem,
  Straighten Stem, Add/Remove Stem, pivoting) has to say what happens to its
  corners. It could come later on top of (a).

**D2. Commands that reshape a reaction** (Set Straight, Toggle Linear, Make Nice,
auto-layout, Arrange).
- **(a) They remove a leg's corner points** *(recommended)*. These commands state a
  new shape, and auto-layout moves every node, which would leave the old corners
  stranded across the map. Undo puts them back.
- (b) They leave them, as they leave modifiers' corner points (§5.7.1). This is
  consistent with modifier lines but leaves a laid-out map full of stale routes.

**D3. Curved pieces in a route.** SBML and Escher both allow any piece to be a
Bézier; the data has curved pieces only at the metabolite (and 2,988 marker → centre
pieces). Recommended: only the end piece may be curved, as above; bending other
pieces stays a later option (as §5.7.1 already notes for modifier lines).

## 8. Steps

Each step with tests, deliberate breakages and the browser, as usual.

1. **Model, drawing and hitting.** `points` on legs; `leg_curve` and `leg_corners`;
   draw, hit, PDF, SVG and PNG; Linear and smooth junction rules; JSON version 15.
2. **Editing.** Generalize the modifier corner-point editing (`view/modifier_lines.rs`)
   to legs: drag, add, remove, Straighten Line, current point and arrow keys; moving
   with a selection; the reshaping commands (D2).
3. **SBML.** Write and read multi-segment leg curves; round-trip tests; check that
   COPASI and SBMLNetwork still draw the export.
4. **Escher import.** Corner points from the marker chains (section 6); recheck the 12
   published maps (must not change) and the 96 Recon3D maps against MetaCarto's own
   SVGs.
5. **One warning per map** for metabolites listed but not drawn.
6. **Docs.** SPECIFICATION §5.6, §5.7.1 (shared editing), §11.5, §11.7; SPEC-Rust
   JSON version 15; diagram-format `via` on legs; the paper's Escher results.
