# Self-regulation loops

Design for drawing a **self modifier** (a species that is a modifier of a
reaction it is also a reactant or product of) as a loop. Written 2026-10-27;
nothing is built. Decisions L1 to L5 were made by the author on 2026-10-27 (§8).

## 1. The problem

SBML lets a species be both a reactant or product of a reaction and one of its
modifiers, and models use it for real biology: ATP is a substrate of
phosphofructokinase and inhibits it; ADP is a product of hexokinase and inhibits
it; protons are moved across the membrane by the proton-driven reactions of the
Millard model and their ratio sets the rate. In the Gallery, the yeast glycolysis
model has 2 and the Millard model 8.

Today such a modifier is drawn like any other (S §5.7): a straight dashed line
from a drawing of the species to the junction, ending in a dot. Two things go
wrong:

- From the drawing that takes part in the reaction, the line lies almost on top
  of that drawing's own reactant or product line, and can't be seen.
- From another drawing of the species (as the yeast model does: PFK's modifier
  line comes from the main ATP node while it consumes a second drawing of ATP
  beside it; hexokinase's comes from an ADP drawing far down the map), it is a
  long line across the diagram, and nothing says the species regulates its own
  reaction.

Two further losses, outside the drawing: Antimony export leaves a self modifier
out (the 10 of the paper's round trip), and the diagram format's writer skips its
line because its reader would refuse it.

## 2. What is drawn

A self modifier is drawn as a **loop**: a curve that leaves the drawing of the
species that takes part in the reaction, swings out to one side of that drawing's
own line, and comes back to the junction, ending in the modifier's symbol (dot,
arrowhead, bar or circle, S §5.7.1) in the modifier's look (S §5.7.2).

- **Which drawing.** The loop starts at the drawing on the reaction's line: the
  reactant's drawing if the species is a reactant, else the product's. The
  drawing the modifier itself names is not used for drawing (**L1**).
- **Shape.** One cubic Bézier curve. With `N` the drawing's centre, `J` the
  junction, `d = |J − N|`, `u` the unit vector from `N` to `J` and `n` the normal
  on the loop's side:
  - it starts on the node's border in the direction `u` turned 45° towards `n`;
  - its handles are `N + 0.25 d u + h n` and `J − 0.25 d u + h n`;
  - it ends at the symbol, which lies on the curve's last piece as for any arc;
  - the height `h` is `0.45 d`, kept between 30 and 80, so a short leg still gets
    a loop that can be seen and a long one doesn't sweep across the map.
- **Side.** The side of the leg with fewer of the reaction's other lines leaving
  the junction within 90° of `n`; on a tie, the side to the left of the
  direction from `N` to `J`. So the loop stays clear of the reaction's own
  lines.
- **Hand placed** (§3): the loop's apex offset, if the user has set one, replaces
  the height and side.
- **Corner points** win: a self modifier with corner points (S §5.7.1) is drawn
  through them as today, not as a loop.
- Hidden modifier lines (S §7.6) hide loops too.

## 3. Editing

- A loop is part of its reaction: clicking it selects the reaction, as an arc
  does.
- While the reaction is selected, a small square sits at the loop's apex.
  Dragging it moves the apex, which sets the loop's height and side (dragged
  across the leg, it flips). One undo step, "Move Loop".
- Right-click the loop: **Flip Loop** (to the other side), **Reset Loop** (back
  to automatic), and the usual **Modifier Kind ▸**. **Add Corner Point** turns
  it into a routed arc (**L2**).
- The apex is stored on the modifier's participant as an offset from the
  middle of the leg, `loop: Option<Pt>`; `None` is automatic. It moves with the
  species and junction, being an offset.
- Moving the species or the junction redraws the loop; nothing to keep in step.

## 4. Files

- **JSON.** A modifier object may have `"loop": [dx, dy]`, the hand-placed apex
  offset; left out when automatic. A file with one is version 17 (SPEC-Rust
  §4.2). An automatic loop needs nothing: a self modifier is recognised from the
  model.
- **SBML.** The modifier's species reference glyph names the species glyph of the
  drawing on the reaction's line, and its curve is the loop as one
  `CubicBezier` segment (start, two base points, end), with the role and SBO
  term of its kind as now. Other tools then draw the loop. Reading: a modifier
  glyph whose species also takes part in the reaction and whose curve is one
  `CubicBezier` becomes a loop, its apex taken from the curve; its handles
  differing from the automatic ones by more than a unit make it hand placed. No
  PathwayDesigner annotation is needed.
- **Diagram format.** A self modifier gets a `line` statement, with the word
  `loop`, and the apex if hand placed:

  ```
  line ATP to PFK kind inhibitor loop
  line ADP to HK loop (12, -40)
  ```

  The reader accepts a `line` for a species that is a reactant or product of the
  reaction, and makes it a self modifier, when the reaction's rate law uses the
  species (**L3**). This is what brings self modifiers back through Antimony with
  its diagram, without Antimony's interaction lines. docs/diagram-format.md §10
  says so; the technical report would mention it at its next version (**L4**).
- **Plain Antimony** (without the diagram) still can't carry a self modifier.

## 5. What it changes

- The yeast and Millard models draw their 10 self modifiers as loops, with no
  change to the files themselves (the loops are automatic).
- The Antimony round trip in the paper loses nothing of these when the diagram
  is read too; the paragraph can say so.
- Nothing else is drawn differently: a modifier that isn't a reactant or product
  keeps its arc.

## 6. Tests

- A self modifier draws a loop from the drawing on the reaction's line, on the
  side clear of the reaction's other lines, with the symbol of its kind; with
  corner points, an arc; hidden with modifier lines.
- The apex drag, Flip Loop and Reset Loop, each one undo step.
- JSON, SBML and the diagram format each carry an automatic and a hand-placed
  loop and read them back the same; another tool's SBML with a curved self
  modifier reads as a loop.
- The Gallery round trips are unchanged except that the 10 self modifiers now
  come back through Antimony with its diagram.
- Breaking each rule on purpose; the yeast model shown in the browser.

## 7. Steps

1. Drawing and hitting the automatic loop (pd-core render), with tests; shown in
   the browser on the yeast and Millard models.
2. Editing: the apex handle, Flip and Reset Loop, the stored offset, JSON.
3. SBML Layout written and read.
4. The diagram format: the `loop` word, the reader accepting self modifiers,
   docs/diagram-format.md, and the specification (S §5.7.3).

## 8. Decisions (made 2026-10-27)

- **L1.** The loop is drawn from the drawing on the reaction's line, whichever
  drawing the modifier names.
- **L2.** Add Corner Point turns a loop into a routed arc.
- **L3.** The diagram reader creates a self modifier from a `line` statement when
  the rate law uses the species.
- **L4.** The technical report mentions loops at its next version.
- **L5.** Height 0.45 of the leg's length, between 30 and 80, to be judged once it
  can be seen.
