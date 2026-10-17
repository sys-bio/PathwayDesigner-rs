# PathwayDesigner — Functional Specification and Milestone Plan

| | |
|---|---|
| **Document type** | Product specification (technology-neutral) with an implementation milestone plan |
| **Reference implementation** | PathwayDesigner FMX/Delphi, version 0.38 (build 2026-05-12), commit `4011e37` |
| **Audience** | Anyone building their own implementation of PathwayDesigner, in any language or UI toolkit |
| **Related** | `docs/SPEC-Rust.md`: how to build this specification in Rust with egui, for desktop and browser |
| **Status** | Written from the reference source code. Where the reference code deviates from the intended behaviour, this document specifies the *intended* behaviour and lists the deviation in §15 |

### How to read this document

- **MUST / SHOULD / MAY** have their RFC 2119 meanings. MUST items are required for
  behavioural parity with the reference application. SHOULD items are strongly
  recommended. MAY items are optional.
- All lengths are in **world units** ("world px"), the coordinate space in which the
  model is stored. At zoom 1.0, one world unit is one screen pixel. Hit-test
  tolerances are the exception and are in **screen px** (stated each time).
- The y-axis points **down**, as in screen coordinates. "Perpendicular" of a vector
  `(x, y)` always means `(-y, x)`.
- Colours are written `#RRGGBB` (opaque) or `#RRGGBBAA`.
- Numbered constants (gaps, radii, fractions) are the reference values. A
  re-implementation SHOULD use the same values so that files look the same in both
  applications.

---

## Contents

1. Product overview
2. Glossary
3. Domain model
4. Identifiers
5. Visual notation and rendering
6. Interaction model
7. Editing commands
8. Properties, styling and kinetic laws
9. Undo / redo
10. Automatic layout
11. File formats and interchange
12. Preferences
13. User interface layout
14. Non-functional requirements
15. Known deviations in the reference implementation
16. Milestone plan
17. Appendices — constants, palette, geometry, conformance checklist, reference implementation notes

---

## 1. Product overview

PathwayDesigner is a desktop editor for drawing **biochemical reaction networks**.
A network is made of *species* (molecules, drawn as boxes) and *reactions*
(hyperedges joining reactant species to product species through a central
*junction* point). Each reaction can also have *modifiers*: species that affect its
rate without being used up.

The user can:

- draw and edit a network directly on a zoomable, pannable canvas;
- import and export networks as **Antimony** text, and type Antimony into a side
  panel to see the network drawn;
- lay networks out automatically with a force-directed algorithm;
- give reaction edges straight lines, a single line, or cubic Bézier curves, with
  fine control over the curve handles;
- style species and reactions (colours, line widths);
- save everything, including layout and styling, in a native **JSON** format;
- export **SBML Level 3** with the **SBML Layout** package, and export the drawing as
  **PNG**, **PDF** or to a printer.

### 1.1 Goals

1. A clean, readable network diagram that is quick to draw and edit.
2. Lossless storage of both biochemistry and layout in the native file.
3. Round-trip with the text-based modelling world (Antimony, SBML).

### 1.2 Non-goals

- Simulation, parameter fitting or any numerical analysis.
- SBGN compliance. The notation is deliberately simple (boxes and junction-centred
  arcs).
- Several documents open at once (one diagram per window).

### 1.3 Target platforms

The reference ships for Windows 64-bit, and its project is also configured for
macOS 64-bit. A re-implementation SHOULD run on Windows and macOS. It MAY run on
Linux or in a browser.

---

## 2. Glossary

| Term | Meaning |
|---|---|
| **Species** | A chemical entity. Drawn as a rounded rectangle labelled with its id. |
| **Primary species** | A species node that carries biochemical data (initial value, boundary flag, compartment). |
| **Alias** | An extra node that draws a primary species a second time somewhere else on the canvas. It has no biochemical data of its own and is never exported as a separate species. |
| **Null node** | A synthetic source or sink species, used when one side of a reaction is empty (`-> A`, `A ->`). Drawn as a small grey circle with a cross. |
| **Boundary species** | A species whose amount is fixed externally (Antimony `$S`). Drawn with a thicker border. |
| **Reaction** | A transformation from reactants to products, with an optional rate law. |
| **Participant** | One reactant, product or modifier entry of a reaction. It points at a species node and has a stoichiometry. Reactant and product participants also store Bézier handles. |
| **Modifier** | A species that appears in a reaction's rate law but is neither a reactant nor a product (for example an enzyme or an inhibitor). |
| **Junction** | The point where a reaction's legs meet. It has a position stored in the model. |
| **Leg** | The edge between one participant species and the junction. |
| **Arity** | Reactant and product counts: UniUni (1→1), BiUni (2→1), UniBi (1→2), BiBi (2→2). |
| **Render mode** | How a reaction's legs are drawn: Straight, Linear or Bézier (§5.5). |
| **Smooth junction** | A Bézier option that keeps all junction-side handles on one line, so the curve passes smoothly through the junction. |
| **Kinetic law / rate law** | An infix maths expression giving the reaction rate, for example `k1*A*B`. |
| **World / screen coordinates** | Model space and canvas pixel space. `screen = world × zoom + scrollOffset`. |

---

## 3. Domain model

The domain model MUST NOT depend on the UI toolkit. It stores no pixels apart from
world coordinates.

### 3.1 Model (document root)

| Field | Type | Notes |
|---|---|---|
| `modelName` | string | From Antimony `model <name>`. Empty for a new diagram. |
| `compartments` | list of Compartment | Drawn as boxes (§7.9). |
| `parameters` | list of Parameter | `variable = expression` |
| `assignmentRules` | list of AssignmentRule | `variable := expression` |
| `species` | ordered list of SpeciesNode | List order is the **z-order**: later items are drawn on top and are hit first. |
| `reactions` | ordered list of Reaction | |
| *id counter* | integer | Next numeric suffix for generated ids (§4). Not persisted, recomputed on load. |

### 3.2 SpeciesNode

| Field | Type | Default | Notes |
|---|---|---|---|
| `id` | string | generated | Biochemical identifier, also the label of a primary node. |
| `center` | point | — | World coordinates of the centre. |
| `width`, `height` | number | 80 × 36 | Null nodes are 28 × 28. Can be changed (§7.10). |
| `shape` | NodeShape | rounded rectangle | What it is drawn as (§5.10). Ignored for null nodes. Added in the Rust version. |
| `borderDash` | Automatic, Solid or Dashed | Automatic | Whether the border is dashed (§5.3). Ignored for null nodes. Added in the Rust version. |
| `initialValue` | number | 0 | Initial concentration. |
| `isBoundary` | bool | false | |
| `isConstant` | bool | false | |
| `compartment` | string | "" | Compartment id. Empty means the default compartment. |
| `aliasOf` | reference to SpeciesNode or none | none | When set, the node is an alias. It always points at a **primary**, never at another alias. |
| `isNullNode` | bool | false | |
| `locked` | bool | false | Locked nodes are not moved by auto-layout. |
| `selected` | bool | false | Transient. Not persisted. |
| `style` | VisualStyle | default | §3.6 |

Rules:

- An alias's biochemical fields are ignored. Everything biochemical comes from its
  primary.
- An alias SHOULD display its **primary's** id as its label (see §15, D-1).
- Null nodes have no biochemical meaning. They MUST NOT appear in Antimony export
  and SHOULD NOT appear in SBML export (§15, D-9).

### 3.3 Participant

| Field | Type | Notes |
|---|---|---|
| `species` | reference to SpeciesNode | Does not own the node. May point at an alias; the leg is then drawn to the alias. |
| `stoichiometry` | number | Default 1. Not used for modifiers. |
| `ctrl1`, `ctrl2` | point | Bézier handles in world coordinates (§5.6.1 explains which is which). |
| `ctrlPtsSet` | bool | `false` means "compute the handles automatically at draw time". `true` means "the handles are stored". |

### 3.4 Reaction

| Field | Type | Default | Notes |
|---|---|---|---|
| `id` | string | generated | |
| `junctionPos` | point | — | |
| `reactants`, `products` | ordered lists of Participant | | Order matters: it sets the fan index (§5.6.2). |
| `modifiers` | ordered list of Participant | | Only `species` is meaningful. |
| `kineticLaw` | string | "" | Infix expression (§8.3). |
| `isReversible` | bool | false | |
| `isLinear` | bool | false | Render mode flag |
| `isBezier` | bool | false | Render mode flag |
| `isJunctionSmooth` | bool | false | Only meaningful when `isBezier`. |
| `selected` | bool | false | Transient |
| `style` | VisualStyle | default | |

**Invariant:** `isLinear` and `isBezier` are never both true. Every command that
sets one MUST clear the other.

### 3.5 Compartment, Parameter, AssignmentRule

- **Compartment**: `id`, `size` (the volume; default 1), `dimensions` (default 3),
  `style`, the box it is drawn as, and whether that box is hidden (§7.9). The id
  `defaultCompartment` is special: it is never shown as a "real" compartment, and
  never drawn, even with a box (renaming a drawn compartment to it removes its box
  from the diagram).
- **Parameter**: `variable`, `expression` (a string, usually a number such as `0.1`).
- **AssignmentRule**: `variable`, `expression` (string).

### 3.6 VisualStyle

Every species, reaction and compartment has an optional style override.

| Field | Applies to | "Use default" sentinel |
|---|---|---|
| `hasCustomStyle` | all | `false` means ignore the whole record |
| `fillColor` | species, compartment | colour value 0 (`#00000000`) |
| `borderColor` | species, compartment | 0 |
| `borderWidth` | species, compartment | 0 |
| `labelColor` | species | 0 |
| `fontSize` | species | 0 |
| `lineColor` | reaction | 0 |
| `lineWidth` | reaction | 0 |
| `junctionColor` | reaction | 0 |
| `junctionVisible` | reaction | default **true** |

The sentinels MUST be applied **per field**. A custom style that sets only the fill
colour still uses the default border colour and width. This is what keeps old files
looking exactly as they did.

`junctionVisible` is a display toggle. Changing it does not mark the style as
custom. It MUST still be persisted (see §15, D-8).

### 3.7 Model operations

The model layer MUST provide:

- `addSpecies(x, y, w=80, h=36)` → new node with a generated id (§4).
- `addAlias(primary, x, y)`: if `primary` is itself an alias, use its primary
  instead. The new alias copies the primary's width and height.
- `addReaction(jx, jy)` → new reaction with a generated id.
- Find species or reaction by id (exact, case-sensitive). Find parameter or
  compartment by name (the reference compares case-insensitively).
- `deleteSpecies(node)`: removes the node. If the node is a primary, it also removes
  all of its aliases. It then removes **every reaction** in which any removed node
  is a reactant or product. It MUST also remove the removed nodes from every
  reaction's modifier list (§15, D-6). It returns the ids of the deleted reactions.
- `deleteReaction(reaction)`.
- `renameSpecies(oldId, newId)`: renames the node and rewrites every kinetic law so
  that whole-word occurrences of `oldId` become `newId`. A whole word is a match not
  next to a letter, digit or `_`. Matching SHOULD be case-sensitive (§15, D-12).
- `clear()`, `clearSelection()`, `selectedSpecies()`, `selectedReactions()`.
- `hasNonDefaultCompartments()`: true if any compartment id is not
  `defaultCompartment`, compared case-insensitively.
- Serialise to and from native JSON (§11.1).

---

## 4. Identifiers

- Ids for new species and reactions come from **one shared counter**. A species gets
  `S<n>` and a reaction gets `R<n>`, then the counter goes up by one. For example:
  S1, S2, R3, S4.
- After any load, import or undo restore, the counter is set to
  `1 + max(numeric suffix)` over all species and reaction ids. The numeric suffix is
  everything after the first character, read as an integer. Ids whose suffix is not
  an integer are ignored.
- User-entered and imported ids are kept **exactly**, including letter case. The
  application MUST NEVER rename an identifier silently.
- Renaming SHOULD reject an empty id, an id that is already used by another species,
  reaction, parameter or compartment, and an id that is not a valid SBML SId
  (`[A-Za-z_][A-Za-z0-9_]*`). (The reference does not check, §15, D-3.)
- Null nodes are named `_source_<reactionId>` and `_sink_<reactionId>`.

---

## 5. Visual notation and rendering

### 5.1 Coordinates and view state

- The view keeps `zoom` (default 1.0, range 0.1–8.0) and `scrollOffset` (a point in
  screen px).
- `screen = world × zoom + scrollOffset` and `world = (screen − scrollOffset) / zoom`.
- Every world-space length (stroke widths, radii, font sizes, gaps) is multiplied by
  `zoom` when drawn.
- Geometry helpers (§17.3) MUST be pure functions that do not care whether they get
  world or screen coordinates.

### 5.2 Draw order

Each frame MUST be drawn in this order:

1. Background (`#F8F9FA`)
2. Reaction legs and arrowheads
3. Modifier arcs
4. Species nodes, which hide any part of a leg that falls inside them
5. Selection halos
6. Junction handles
7. Bézier control-point handles (for selected Bézier reactions only)
8. In-progress reaction feedback (add-reaction mode only)
9. Rubber-band rectangle
10. Hover tooltip

### 5.3 Species nodes

- **Shape:** an axis-aligned rounded rectangle centred on `center`, size
  `width × height`, corner radius **8**.
- **Fill / border:**
  - Normal node: fill `#EEF6FF`, border `#4A7FCB`, border width 1.5.
  - Alias node, when the alias indicator is on (§7.6): fill `#F5F0FF`, border
    `#7A6FC8`, and the border is **dashed** (5 on, 4 off).
  - A custom style overrides the colours field by field. An alias keeps its dashed
    border even when it has a custom style.
  - **Border style** *(added in the Rust version)*: each species (primary or alias;
    null nodes aside) has one of **Automatic** (the rule above: dashed for an alias
    while the alias indicator is on, solid otherwise; the default), **Solid** or
    **Dashed** (5 on, 4 off). Solid and Dashed apply whatever the alias indicator
    says, so an alias can be styled, say as a coloured circle, without the dashes,
    and a primary can be dashed. Set with the **Border style** list in the Species
    group (§13.3), for every selected species, one undo step. A new alias starts
    as Automatic, so dashed, even if its primary is Solid. A primary and its aliases
    can then look the same; the model tree still marks aliases, and *Go to Primary*
    still finds the primary. Saved like the shape (§5.10): JSON version 5, and the
    SBML species glyph's note.
- **Boundary species:** the border stroke width is multiplied by **2.4**.
- **Label:** the id (for an alias, the primary's id), centred horizontally. It is
  centred vertically on the font's cap height. Default size 12, colour `#1A1A1A`.
  Custom `fontSize` and `labelColor` are used when set.
- **Selection:** a selected node is shown **only** by a red (`#FF0000`) dashed halo
  (10 on, 5 off). The halo is a rounded rectangle 6 units outside the node, stroke
  width 2, corner radius 14. Selection MUST NOT change the node's fill or border.
- **Auto-width:** after a rename, the node widens to `textWidth(label size) + 2×10`
  (the label size is 12 unless the species has its own font size; the reference
  always used 12) if
  that is larger than its current width. It never shrinks. All aliases of the same
  primary widen the same way.

### 5.4 Null nodes

- A circle of radius `width/2` (14 for a 28 × 28 node). Fill `#B0B0B0`, border
  `#505050`, border width 1.5.
- An × cross in `#505050`, stroke 1.8, round caps. Each arm reaches 55 % of the
  radius from the centre.
- No label. Custom styles are ignored.
- A selected null node has a **circular** halo of radius `width/2 + 6`.
- When the preference **Show null species** is off, null nodes **and every leg that
  touches one** are not drawn.

### 5.5 Reaction render modes

| Mode | Flags | Drawing |
|---|---|---|
| **Straight** | neither flag | Each reactant leg is a straight segment from the node's border to the junction. Each product leg is a straight segment from the junction to the node's border, with an arrowhead. |
| **Linear** | `isLinear` (UniUni only) | One straight line from reactant to product with one arrowhead. The junction handle is hidden. The *effective* junction is the stored junction projected onto the reactant–product line, with the projection parameter clamped to [0.1, 0.9] and, further, to at least 10 units outside both nodes' borders, so modifier arcs never end inside a node. If the nodes are too close for that, it is halfway between their borders; if the two centres are less than 1 unit apart, it is their midpoint. When a drag or an alignment moves one end of a Linear reaction, the stored junction is moved to keep its place along the line. (The reference only clamps to [0.1, 0.9] and leaves the stored junction where it is, so a modifier's dot can slide into a node.) |
| **Bézier** | `isBezier` | Each leg is one cubic Bézier curve (§5.6). |

If `isLinear` is set on a reaction that is not UniUni, it MUST be ignored and the
reaction drawn as Straight.

**Line style:** colour `#444444` and width 1.5 by default, with round caps. A custom
`lineColor` or `lineWidth` overrides these. A **selected** reaction is drawn with
**1.5 × its line width**, in the **selected reaction colour**: `#1144CC` by default,
set in Preferences (§12). Its arrowheads and modifier arcs take the same colour.
(The reference keeps the line's own colour, which makes a selected reaction hard
to see; changed at the user's request.)

**Gaps and arrowheads (world units):**

| Constant | Value | Meaning |
|---|---|---|
| reactant gap | 6 | A reactant leg starts this far outside the node border, measured along the leg direction. |
| product line gap | 6 | A product line ends this far before the node border. |
| arrow inset | 3 | The arrowhead *tip* sits this far before the node border. This is set separately from the line gap so the tip can almost touch the node while the line end stays hidden under the arrowhead. |
| arrow length | 12 | Length of the arrowhead triangle along the direction of travel |
| arrow half-base | 5 | Half the width of the arrowhead base |

The arrowhead is a filled isosceles triangle in the line colour, with
`tip = T`, `base1/2 = T − d·12 ± perp(d)·5`, where `d` is the unit direction of
travel.

**Border intersection for straight lines:** use the axis-aligned rectangle (ignore
the rounded corners). Starting at the centre `C`, step towards an external point `P`
along the unit vector `u`. The border point is `C + u·min(halfW/|uₓ|, halfH/|u_y|)`.

### 5.6 Bézier legs

#### 5.6.1 Handle convention

This convention MUST be used everywhere (drawing, dragging, layout, smoothing,
export):

| Leg | Conceptual curve | `ctrl1` | `ctrl2` |
|---|---|---|---|
| Reactant | `P0` = species **centre** → `P3` = junction | outer handle (species side) | inner handle (junction side) |
| Product | `P0` = junction → `P3` = species **centre** | inner handle (junction side) | outer handle (species side) |

Legs are stored **centre to centre** and cut at the node border only when drawn.
No border point is ever stored, so moving or resizing a node never requires handles
to be recomputed.

#### 5.6.2 Automatic handles (`ctrlPtsSet = false`)

For a leg from `A` to `B` (as in §5.6.1), with `L = |B − A|`, `d = (B − A)/L`,
`n = perp(d)`, and `i` = the index of this leg among the reaction's legs of the
**same role**, of which there are `N`:

```
offset = (N <= 1) ? 0 : (i − (N − 1)/2) × 50
reactant:  ctrl1 = A + d·0.35L              ctrl2 = B − d·0.35L + n·offset
product:   ctrl1 = A + d·0.35L + n·offset   ctrl2 = B − d·0.35L
if L < 1:  ctrl1 = A, ctrl2 = B
```

The fan offset is applied only at the junction end. Legs therefore spread apart at
the junction but leave each node cleanly.

#### 5.6.3 Clipping and drawing

1. Find where the conceptual curve crosses the node rectangle by **bisection** on
   `t ∈ [0, 1]`, 24 steps, using an inside-rectangle test. Clamp the result to
   [0.001, 0.999].
2. **Reactant:** split with de Casteljau at `t` and keep the right half
   `[t..1]` = `(R0, R1, R2, R3)`. Move the start forward by 6 (the reactant gap)
   along the unit tangent `(R1 − R0)`. Draw a cubic from that start through
   `R1, R2` to the junction.
3. **Product:** split at `t` and keep the left half `[0..t]` = `(L0, L1, L2, L3)`.
   Let `d = unit(L3 − L2)`, the tangent at the border.
   - The line ends at `L3 − d·6` and is drawn as a cubic through `L1, L2`.
   - The arrowhead tip is at `L3 − d·3` and points along `d`.
   - The arrowhead therefore always follows the tangent of the curve.

#### 5.6.4 Control-handle display

For each **selected** Bézier reaction, every leg shows two handles: circles of
radius 4, white fill, border `#888800` width 1. A dashed grey (`#AAAAAA`, 8/4) line
joins each handle to its anchor: the species centre for an outer handle, the
junction for an inner handle. When the reaction has a smooth junction, the inner
handles are outlined in teal (`#009999`, width 1.5).

### 5.7 Modifier arcs

For each modifier of a reaction:

- A **dashed** line (6 on, 4 off) of width 1.5. It starts 6 units inside the
  modifier node's border, on the side facing the junction. It ends at a filled dot of
  radius **4**, whose centre is **10** units from the junction in the direction of
  the modifier.
- Colour teal `#007777`, or the selected reaction colour (§12) while the reaction
  is selected.
- No arrowhead. The arc is drawn before the species so that its start is hidden
  under the modifier node.
- If the modifier's centre is less than 1 unit from the junction, skip it.

### 5.8 Junction handle

- A circle of radius 4 at the effective junction. Fill `#FF8800`, or the custom
  `junctionColor`. Border `#A05000`, width 1.
- When the reaction is selected: radius **6.4** and border `#0033AA`.
- Not drawn for Linear reactions, or when `junctionVisible = false`. In both cases
  the junction can still be hit and dragged.

### 5.9 Transient overlays

- **Rubber band:** a rectangle in screen px. Fill `#0066CC` at 20 % alpha, border
  `#0066CC` at 1 px.
- **Pending reaction (add-reaction mode):**
  - Species already picked as reactants get a green (`#00AA44`) rounded ring, 4
    units outside the node, width 2.5.
  - Species already picked as products get a red (`#CC3300`) ring.
  - A dashed guide line (`#888888`) runs from the last picked species to the cursor.
  - A dot of radius 5 px at the cursor is green while reactants are still needed and
    red otherwise.
- **Tooltip:** a pale-yellow (`#FFFDE7`) rounded box with a `#888866` border and
  11 px text, drawn at cursor + (14, 14) screen px. For a species it shows
  `id: initialValue` (up to 4 decimals). For a reaction it shows `id` or
  `id: kineticLaw`. The tooltip SHOULD be kept inside the canvas.

---

### 5.10 Species shapes

*Added in the Rust version; the reference drew every species as a rounded
rectangle.* Each species (primary or alias; null nodes aside) is drawn as one of:

| Shape | Drawn as |
|---|---|
| **Rounded rectangle** | The box with corners rounded by 8 (§5.3). The default. |
| **Rectangle** | The box with square corners. |
| **Ellipse** | The ellipse that fills the box. |
| **Circle** | An ellipse whose width and height are kept equal (below). |
| **Hexagon** | Points at the middle of the left and right sides and flat top and bottom, filling the box; the slanted edges start a quarter of the width in from each side. |

- **Fill, border, alias dashes and label** are drawn as for the rounded rectangle,
  on the shape's outline. The selection halo and its corner handles (§7.10) stay
  the box around it.
- **Lines end on the shape.** A reaction leg, a modifier arc and a Bézier leg stop
  where they cross the outline that is drawn (for a rounded rectangle, the box, as
  in §5.5), so arrowheads touch an ellipse or a hexagon, not the box around it.
  Clicking still uses the box (§6.2).
- **Choosing a shape:** the **Shape** list in the Species group (§13.3), for every
  selected species, one undo step. A species made a circle takes the larger of its
  width and height both ways, so its name still fits.
- **A circle stays round:** resizing it by a corner always keeps its proportions,
  Shift or not; either size field sets both; Fit to Name sets both to the width.
- A new alias takes its primary's shape, as it takes its size; each can then be
  changed on its own.
- **Files.** JSON version 5 (SPEC-Rust §4.2), written only when some species has a
  shape other than the rounded rectangle, so other models are still written as
  version 3 or 4. In SBML, PathwayDesigner's note on the species glyph
  (SPEC-Rust §8.3). Antimony has no layout.

## 6. Interaction model

### 6.1 Modes

The canvas works as a state machine:

| State | Entered by | Left by |
|---|---|---|
| **Select** (default) | Select tool, Esc, a cancelled add-reaction | — |
| **AddSpecies** | Add Species tool | Select tool or Esc |
| **AddReaction(nR, nP)** | an arity tool | Select tool, Esc, or clicking empty canvas |
| **DraggingNodes** | mouse-down on a species in Select mode | mouse-up |
| **DraggingJunction** | mouse-down on a junction | mouse-up |
| **DraggingCtrlPt** | mouse-down on a Bézier handle | mouse-up |
| **RubberBand** | mouse-down on empty canvas | mouse-up |

Entering AddSpecies or AddReaction clears the selection.

### 6.2 Hit testing (Select mode, left button)

Candidates are tried in this order, and the **first** hit wins:

1. **Bézier handle** of a *selected* Bézier reaction, within **8 screen px**.
   After it, in the Rust version: a selected text's wrap handle (§7.8), a selected
   compartment's corner (§7.9), then a **selected species' corner** (§7.10), each
   within 8 screen px.
2. **Junction** of any reaction (at its effective position), within **8 screen px**.
3. **Species**: the **topmost** node (last in list order) whose rectangle contains the
   point.
4. **Reaction leg**, within **6 screen px**:
   - Straight leg: distance to the segment.
   - Linear leg: distance to the drawn segment.
   - Bézier leg: sample the curve at 20 points and ignore samples inside the node.
5. **Empty canvas.**

### 6.3 Selection rules

| Action | Effect |
|---|---|
| Click an unselected species | Select only that species, then start dragging it. |
| Click a selected species | Keep the whole selection and drag all of it. |
| Shift + click a species | Toggle it in or out of the selection. No drag. |
| Click a junction | Clear the selection (unless Shift is held), select the reaction, start dragging the junction. |
| Click a leg | Select only that reaction. Shift toggles instead. No drag. |
| Click empty canvas | Clear the selection (unless Shift is held) and start a rubber band. |
| Release rubber band | Add every species whose rectangle **intersects** the band, and every reaction whose stored junction lies **inside** the band. |
| Ctrl/Cmd + A | Select all species and reactions. |
| Esc | Cancel the current mode and clear the selection. |

Every change to the selection MUST raise a *selection-changed* notification. The
notification carries the selected species and reactions so the inspector and other
UI can update.

### 6.4 Dragging

**Nodes.** Every selected species moves by the same world-space delta, measured from
the mouse-down point. In addition:

- A reaction whose reactants and products are **all** in the moved set (modifiers are
  not counted) has its junction moved by the same delta.
- Stored Bézier handles move with the end they belong to. A species-side handle
  moves if its species moved. A junction-side handle moves if its junction moved.
- A smooth-junction reaction whose junction moved has its handles recomputed
  (§7.4.4).
- Positions MUST be computed as *original position + current delta*, never by adding
  small increments, so rounding errors don't build up.
- **Locked nodes MUST NOT move.** A locked species stays where it is even when it is
  selected with others; the rest of the selection moves around it. It does not count
  as "moved" for the junction and handle rules above. (The reference lets locked
  nodes be dragged.)

**Junction.** The junction follows the cursor. If the reaction has a smooth junction,
every **stored** inner handle moves by the same delta, which keeps the inner handles
in line.

**Bézier handle.**

1. When the handle is grabbed, if the participant's handles are automatic, save
   **both** automatic positions first (set `ctrlPtsSet = true`). The other handle
   then never jumps to (0, 0).
2. While dragging, the handle follows the cursor.
3. If the handle is an **inner** handle and the reaction has a smooth junction,
   apply the smoothing rule after each move:
   - Let `D = unit(dragged − J)`.
   - Every other inner handle keeps its distance `a` from `J`. Same-role handles move
     to `J + D·a`. Opposite-role handles move to `J − D·a`.
   - A participant with automatic handles is given stored handles first.
   - If `a` is less than 0.5, use the dragged handle's distance instead.

### 6.5 Creating species and reactions

**AddSpecies.** Each left click adds a species (80 × 36) centred at the click
point. The tool stays active. Each species is one undo step.

**AddReaction(nR, nP).** The four tools are UniUni (1,1), BiUni (2,1), UniBi (1,2)
and BiBi (2,2).

- Each click on a species adds it as the next **reactant** until `nR` have been
  picked, then as the next **product**.
- A species that has already been picked for this reaction is ignored. (So the
  drawing tools cannot create `A + A` or `A -> A`. Use Antimony for those.)
- Clicking empty canvas cancels and returns to Select mode.
- When all `nR + nP` species have been picked, the reaction is created:
  - **Junction** = midpoint of the reactant centroid and the product centroid.
  - **Stoichiometry** = 1 for every participant.
  - **Rate law** = mass action: `k<N>*R1*R2…`, where `<N>` is the numeric part of the
    new reaction's id (so reaction `R7` uses `k7`). If no parameter `k<N>` exists,
    one is added with value `0.1`.
  - **Render mode:** if the *default Bézier* option is on, use Bézier. Otherwise use
    Linear for UniUni and Straight for everything else. If the *default smooth
    junction* option is also on and the reaction is Bézier, give it a smooth
    junction and compute its handles immediately. The reference turns **both
    options on** at startup.
  - The reaction is one undo step. The tool stays active for the next reaction of
    the same arity.

### 6.6 Other gestures

| Gesture | Effect |
|---|---|
| Double-click a species (Select mode) | Rename (§7.1). On an alias, this renames the primary. |
| Right-click | Context menu for the item under the cursor (§13.4). Hit order: species, then junction or leg. Nothing happens on empty canvas. |
| Ctrl/Cmd + mouse wheel | Zoom by a factor of 1.15 per notch, clamped to [0.1, 8]. The world point under the cursor MUST stay under the cursor. |
| Scrollbars | Pan. The reference uses a fixed 0–8000 range and sets `scrollOffset = −scrollbarValue`. A re-implementation SHOULD size the scroll range from the content bounds. |
| Plain mouse wheel | SHOULD pan vertically (Shift + wheel pans horizontally). The reference does nothing. |
| Hover | After the pointer has been still for **600 ms** over a species, junction or leg, show the tooltip. Moving to a different item hides it, and so does leaving the canvas. |
| Delete key (Select mode) | Delete the selection (§7.2). |
| Pointer move | The status bar MAY show the pointer coordinates. |

---

## 7. Editing commands

Every command in this section changes the model and MUST be undoable (§9) unless it
says otherwise.

### 7.1 Rename species

Prompt for a new id, pre-filled with the current one. On OK, with a different,
valid, unused id:

- rename the primary (§3.7), which also rewrites kinetic laws;
- auto-widen the node and its aliases (§5.3);
- record one undo step.

Null nodes cannot be renamed.

### 7.2 Delete

Delete all selected items:

1. Delete each selected species, with the cascade from §3.7.
2. Delete any selected reactions that still exist.

The whole deletion is one undo step. The context-menu delete does the same for a
single item.

### 7.3 Aliases

- **Create alias** (context menu, primary non-null species): add an alias offset by
  (+24, +24) from the primary and select only the new alias.
- **Go to primary** (context menu, alias): select only the primary and scroll so the
  primary is **centred** in the viewport. (The reference puts it at the top-left
  corner instead, §15, D-15.)
- **Delete an alias**: removes the alias and any reaction that uses the alias as a
  reactant or product. The primary is not affected.
- **Delete a primary**: also removes all of its aliases (§3.7).
- **Split into Aliases** (context menu, a primary non-null species drawn by two or
  more reactions; added in the Rust version, not in the reference): gives each
  reaction that draws the species its own drawing of it, in one undo step. The
  primary keeps the reaction whose junction is nearest to it. Every other reaction
  (reactant, product or modifier) gets a new alias, placed on the line from its
  junction towards the primary, at most 90 units from the junction (half way if
  the primary is nearer than 180), so long lines to a busy node become short ones.
  Lines already drawn to an alias stay with it. Reactant and product legs moved to
  an alias go back to automatic handles, and smooth junctions are recomputed. The
  new aliases are selected, so they can be dragged as a group. The biochemistry
  does not change.
- **Split Off Selected Reactions** (context menu, a species or alias, when a
  reaction it is drawn in is selected too; added in the Rust version): as Split into
  Aliases, but only for the selected reactions that draw the right-clicked node. Each
  gets its own new alias, already joined to it and placed the same way; every other
  reaction stays with the node. To use it, select the species, add the reaction with
  Shift-click (on the canvas; Ctrl/Cmd-click in the model tree), and right-click the
  species (a right-click on something selected keeps the selection). Otherwise the item is shown disabled. Split off from an
  alias, the new alias is of the same primary, and the old one stays, drawing
  whatever is left. One undo step; the new aliases are selected; the biochemistry
  does not change.
- **Merge into Primary** (context menu, an alias; added in the Rust version): that
  alias alone goes back into its primary, as below; other aliases stay. One undo
  step.
- **Merge Aliases** (context menu, a primary species that has aliases, or
  **Merge All Aliases into Primary** on any of its aliases; added in the Rust version):
  the reverse of Split into Aliases, in one undo step. Every line drawn to an alias
  of the species, reactant, product or modifier, is drawn to the primary again, and
  the aliases are removed. Reactant and product legs moved back go to automatic
  handles, smooth junctions are recomputed, and a reaction that had the species as a
  modifier twice (once through an alias) has it once. Only the primary is selected
  afterwards. The biochemistry does not change.
- **Merge Selected Aliases into This** (context menu, a species or alias, when other
  aliases of the same species are selected too; added in the Rust version): the
  selected aliases are merged into the node that was right-clicked, as Merge Aliases
  merges them into the primary. The right-clicked node stays where it is; aliases
  that are not selected, and selected nodes of other species, are left alone. On the
  primary it merges only the selected aliases. The primary itself is never merged
  away. Select with click and Shift-click, then right-click the one to keep.
  Otherwise the item is shown disabled. One undo step.

### 7.4 Reaction shape commands

Each command applies to the currently selected reactions.

#### 7.4.1 Toggle Linear (UniUni only)

For each selected UniUni reaction, flip `isLinear`.

- Turning it **on** clears `isBezier`.
- Turning it **off** moves the junction to the midpoint of the two centres, so the
  reaction becomes Straight.

Reactions that are not UniUni are left unchanged.

#### 7.4.2 Set Bézier

For every selected reaction, set `isBezier = true` and `isLinear = false`.

#### 7.4.3 Set Straight

For every selected reaction, clear both flags. For UniUni reactions, also move the
junction to the midpoint of the two centres. The reference has this command in its
code but not in its UI. A re-implementation SHOULD show it.

#### 7.4.4 Toggle Smooth Junction (Bézier reactions only)

The toolbar button shows pressed when there are selected Bézier reactions and all
of them are smooth. If they are all smooth, a click turns `isJunctionSmooth` off
for all of them. Otherwise it turns it **on** for all of them, including any that
were already smooth. (The reference flips each reaction on its own. The button
showed no state, and new reactions start smooth, so the first click seemed to do
nothing: it turned smoothing off, which leaves the curves as they are. Changed at
the user's request.) When it turns **on**, compute and store the smooth handles
immediately:

```
CentR = mean of reactant centres  (or J if there are none)
CentP = mean of product centres   (or CentR if there are none)
axis  = unit(CentP − CentR)   (use (1, 0) if |CentP − CentR| < 0.5)
For each reactant S (leg length L = |J − S|):
    ctrl1 = S + 0.35·(J − S)          (outer)
    ctrl2 = J − axis·0.35·L           (inner)
For each product S (L = |S − J|):
    ctrl1 = J + axis·0.35·L           (inner)
    ctrl2 = S + 0.35·(J − S)          (outer)
If L < 1, reset that participant to automatic handles.
```

**Looped species** (the same species is both a reactant and a product, for example
`A -> A + B`). For each such pair, let `p = perp(unit(J − A))`:

- reactant `A`: `ctrl1 += 30·p`
- product `A`: `ctrl2 −= 30·p`
- then **every** reactant inner handle is set to `J + 25·p` and **every** product
  inner handle to `J − 25·p`.

Without this adjustment the two arcs to `A` would lie on top of each other, and the
`J → B` leg would become a straight line.

The junction is not moved and the render-mode flags are not changed.

#### 7.4.5 Make Nice (also "Adjust Reaction" on the context menu)

For each target reaction:

1. Turn on Bézier mode.
2. Move the junction to `(CentR + CentP)/2`.
3. If the junction is not smooth, reset every participant to automatic handles. If it
   is smooth, compute the smooth handles as in §7.4.4.

### 7.5 Align and distribute

These commands work on the selected species and on the junctions of the selected
reactions. Alignment needs at least 2 such items. Distribution needs at least 3.
Otherwise the command does nothing. A species counts as its rectangle and a junction
as a point. The bounding box below is the box around all of them. A Linear
reaction's junction is left out, because it stays on its line. (The reference aligns
species only. Junctions were added at the user's request; to align only species,
select only species, for example with Edit ▸ *Select All Species*.)

| Command | New position |
|---|---|
| Align Left | each node's left edge = the bounding box's left edge |
| Align Center | each node's centre x = the bounding box's centre x |
| Align Right | each node's right edge = the bounding box's right edge |
| Align Top / Middle / Bottom | the same, on the y-axis |
| Distribute Horizontally | sort by centre x, keep the first and last fixed, space the centres evenly between them |
| Distribute Vertically | the same, on the y-axis |

**Locked species do not move.** They still count towards the bounding box, so other
nodes can be aligned to a locked one. When distributing, locked nodes keep their
places and the others are spaced as usual.

The changes are then passed on to the reactions:

- **If all reactants and products of a reaction moved:**
  - The junction moves by the mean of the participants' movements.
  - Stored handles move: species-side handles by their own species' movement,
    junction-side handles by the junction's movement.
  - Smooth reactions have their handles recomputed.
- **If only some participants moved:** the junction stays where it is, and the moved
  participants of a Bézier reaction are reset to automatic handles.
- **A selected junction** then goes to its own new position, moving as a junction
  drag does (a smooth junction's inner handles go with it).

#### 7.5.1 Arrange in Circle

*Not in the reference; added at the user's request.* It is in the Alignment
submenu and the Arrange menu, after the distribute commands. It works on the
selected species that aren't locked, and needs at least 3 of them; locked
species are left out. It is one undo step.

**The circle.** Its centre is the centre of the species. Its radius is the larger
of their mean distance from the centre and the radius at which neighbours are
the largest box diagonal plus 140 apart, centre to centre. The species go evenly
round it.

**The order round the circle.** A *link* is a reaction with exactly one reactant
and one product among these species, and they are different. If the links join
every species into one chain or one loop, the order follows the chain or loop.
Otherwise the order is the species' order round the centre now, so placing them
roughly first sets the order. The order runs clockwise or anticlockwise, and
starts at the angle, that moves the species least from their angles now.

**Reactions between neighbours.** A link between neighbours on the circle is
drawn as an arc of the circle. It becomes a Bézier reaction with stored handles and
no smooth junction. Its junction goes on the circle halfway between the two
species. Each leg is a cubic Bézier along half the arc. Its handles lie on the
circle's tangents at its ends, and are (4/3)·tan(θ/4)·R long, for sweep θ and
radius R. This is the usual cubic approximation of a circular arc, and it is out
by less than 0.03 % of R for the arcs used here. The two legs meet at the junction
along the same tangent, so the reaction reads as one piece of the circle. A
second link between the same neighbours takes the mirror image of the arc in the
line between them, so it bulges inwards. Any further links get automatic
handles.

**Other species of an arc reaction** (as in uni-bi, bi-uni and bi-bi reactions)
join at the junction. A species that belongs to just one arc reaction this way,
and isn't locked, is moved to just outside the circle beside the junction. The
reactants go before the junction and the products after it, in the direction of
travel. A species shared by several arc reactions, such as a cofactor, stays
where it is. The leg of each such species has its junction-side handle on the
arc's tangent, so it merges into the arc, and its own handle pointing square to
the circle. Both handles are 0.35 of the leg's length.

**Other reactions.** Species movements are passed on to other reactions as in
§7.5. A reaction that isn't an arc, and all of whose species moved, gets its
junction at the centre of its species and automatic handles (smooth handles
recomputed). Without this, the mean movement could put the junction anywhere.

### 7.6 View toggles (not undoable; only Modifier lines is remembered)

- **Alias indicator** on/off. Default **on**. Turns the purple fill and dashed border
  of aliases on or off.
- **Modifier lines** on/off (View menu). Default **on**. Hides or shows the dashed
  arcs from modifiers to their reactions, and the dots at their ends, for every
  reaction at once. The modifiers stay in the model. It is remembered between
  sessions, like a preference. *Not in the reference; added at the user's
  request.*

Exported pictures (SVG, PNG, PDF, print) follow these settings.

**Zoom In and Zoom Out** are in the View menu and in the status bar (§13.5). Each
zooms by two wheel notches (×1.15², within [0.1, 8]) about the centre of the
canvas. Without them, zooming needs the mouse wheel or a pinch, which users may not
know about. *Not in the reference; added at the user's request.*

### 7.7 New / demo / random network

- **New** asks for confirmation, then clears the model, resets the counter and
  scroll position, and **clears the undo history**.
- At startup the reference shows a small demo network (A + B → C + D, E → F, and an
  alias of ATP). A re-implementation MAY start empty.
- **Random network** (a test tool) MUST be undoable or clear the history:
  - Make 10 species on a 5-column grid, with initial values in [1, 5].
  - Make a backbone chain S1→S2→…→S10 of Linear UniUni reactions.
  - Make 3 more reactions (12 in total). Each has a 20 % chance of being BiUni and a
    20 % chance of being UniBi.
  - Give every reaction a mass-action law `k<i>*…` and a matching parameter
    `k<i> = 0.1`.

### 7.8 Free text

*Not in the reference; added at the user's request.* Free text puts notes, titles
and paragraphs on the diagram. It has no part in the model's biochemistry.

**A text** has an id `T<n>` from the shared counter (§4), the top-left corner of its
first line, its words, a typeface, a size, bold and italic, an alignment, a
colour, and a wrap width.

- **Words** may hold line breaks, which start new lines.
- **Wrap width** 0 means lines break only at line breaks. Otherwise the text wraps
  between words, so no line is wider than the wrap width. A word wider than the
  wrap width gets a line of its own. The narrowest wrap width is 20.
- **Typefaces:** *Sans* (Noto Sans, the label font) or *Serif* (Noto Serif, a
  Times-like face), each in regular, bold, italic and bold italic. These eight
  bundled fonts are the only ones, so text looks and measures the same on every
  platform, in the browser, and in SVG, PNG and PDF (SPEC-Rust §5.4).
- **Size** is the font size in world units, at least 1. New text is 14.
- **Alignment:** *Left* (the default), *Center* or *Right*: how each line lines up
  in the text's width (below). A line wider than the width starts at the left
  edge.
- **Colour** defaults to the label colour.

The style applies to the whole text; there is no mixed styling within one text.

**Layout.** The text's width is its wrap width, or else its widest line. Each
line starts at the left edge, is centred in the width, or ends at its right edge,
as the alignment says. Lines are one font height apart (ascent − descent of the
face). The text's **box** is its width (the wrap
width, or else its widest line, but at least one em) by its lines' height, plus 4
on every side. The box is what selects the text, what the rubber band meets, what
alignment lines up, and what the exported picture's bounds include (§11.6).

**Drawing.** Texts are drawn after everything else, in their order, so they are on
top. A selected text has the selection halo's dashed outline around its box, and a
square **wrap handle** (the Bézier handle's size and colours) in the middle of its
box's right edge. Neither is drawn in exported pictures.

**Editing.**
- The **Text** tool (left palette, after Species): a click places new text, with
  its top-left corner at the click, words "Text", selected. The tool goes back to
  Select, and the keyboard goes to the words field of the right panel's Text group
  with the words selected, so typing replaces them.
- **Hit testing (§6.2):** after a selected Bézier reaction's handles come the wrap
  handles of selected texts, then texts (topmost first), then junctions, species
  and legs. Text is drawn on top, so it is hit first.
- Click, Shift+click, the rubber band, **Select All** (Ctrl/Cmd+A), Delete, arrow-key
  nudges and dragging work on texts as on species. Select All Species and Select
  All Reactions leave texts out.
- **Dragging the wrap handle** sets the wrap width to the text's width when the drag
  began plus the distance moved, never less than the narrowest wrap width. One undo
  step, "Wrap Text".
- **Double-click** a text: the keyboard goes to its words field.
- **Right-click** a text: Edit Text · Delete Text · Alignment ▸ …
- **Align and distribute (§7.5)** treat a text like a species: by its box.
- Arrange in Circle and auto-layout leave texts where they are.

**Undo.** Placing a text and then typing its words is one undo step. Later typing
in the words field is one step for each run of typing, until something else
happens. Every style change is one step.

**Files.** JSON version 4 (SPEC-Rust §4.2). In SBML, a text glyph with the words,
with PathwayDesigner's note for the look (SPEC-Rust §8.3). Antimony has no layout,
so it doesn't keep texts.

### 7.9 Compartments

*Drawing them was planned (F-2); the details below were settled with the user.*
Every compartment except the default one is drawn as a box: its id `C<n>` (or any
valid id it is given), its volume, its style, and the box (centre and size).

**Drawing.** First of all, before the reactions, largest box first so nested ones
are on top. A rectangle with corners rounded by 14 (less for a very small box),
filled and outlined: by default a pale green fill (`#F0F6EA`), a green border
(`#7FA36B`) 2 wide. The fill, border colour and border width can each be set. The
id is written inside the top-left corner, 8 in from each side, in 12-point label
font, dark green (`#3D5C2B`), or in the border colour if that was set. A selected
compartment has the selection halo's dashed outline and a square handle at each
corner. Neither is drawn in exported pictures.

**Which compartment a species is in.** A species belongs to the **innermost** box
its centre is in (the smallest one, so boxes can be nested), or to the default
compartment if it is in none. Aliases and null nodes have no compartment of their
own. Two rules keep the drawing and the model in step:
- **When the user moves things,** each species moved for itself takes the
  compartment it ends up in: after a drag, a nudge, an align or distribute, Arrange
  in Circle, and when a species is added. Drawing a compartment takes in the
  species inside it. Resizing one updates the species inside its old and new box,
  and its own species. Species carried by a dragged compartment keep theirs.
- **When the app moves things,** species keep their compartment and the boxes
  follow. Auto-layout refits every box around its species and around the boxes
  nested in it (as they were nested at the start), innermost first, with 30 of space
  all round, at every step. A compartment that has no box (an older file, an
  Antimony or SBML import without one) gets one fitted around its species when the
  model is opened or imported, or a 300 × 200 box to the right of the drawing if it
  has no species.

A model with no drawn compartments never has its species' compartments changed.

**Editing.**
- The **Compartment** tool (left palette, after Text): a drag draws the box; a
  click makes a 300 × 200 box centred on it. The new compartment is selected and the
  tool goes back to Select. The smallest box is 40 × 40.
- **Hit testing (§6.2):** a selected compartment's corner handles come after the
  wrap handles of texts. Its border (within the leg tolerance) or its name selects it,
  after every other target, innermost box first. A click anywhere else inside it
  reaches what is there, or starts a rubber band.
- **Dragging a compartment** moves everything inside its box: species (unless
  locked), reaction junctions (except a Linear reaction's, which stays on its line),
  texts, and the compartments nested in it. Reactions follow as in a node drag
  (§6.4): a moved junction takes its junction-side handles with it, and a smooth
  junction is recomputed, even when some of its species are outside the box.
- **Dragging a corner** resizes the box; the opposite corner stays.
- The rubber band selects a compartment only when it encloses the whole box. Select
  All selects compartments too.
- **Delete** removes the compartment; its species stay, in the default compartment.
- **Hiding the box.** The Compartment group's **Show box** tick box hides a
  compartment's box without removing anything: the compartment, its id (which rate
  laws may use), its volume and its species stay. A hidden box is not drawn, in the
  window or in exported pictures, and is as if it had no box: it can't be clicked
  or resized, Select All and the rubber band leave it out, it carries nothing when
  something else is dragged, and it takes in no species. A species in it keeps it
  when moved, unless it lands in a box that is shown. The box keeps its place, so
  ticking Show box again puts it back where it was. The left panel's model tree
  lists every compartment, a hidden one as `C1 (hidden)` and the default one as
  `defaultCompartment (not drawn)`, so a hidden one can still be selected. The
  default compartment has no box to show, so Show box is off for it.
- **Right-click:** Delete Compartment (keeps its species).
- The right panel's **Compartment** group (§13.3): Id, Volume, Fill, Border, Border
  width. A new id must be a valid SId not used by anything else (changing only its
  letter case is allowed). Renaming updates the species in it and whole-word uses in
  rate laws, parameters and rules.
- The tooltip over a compartment's border or name is `id: volume v`.

**Files.** JSON version 4 (SPEC-Rust §4.2). In SBML, a compartment glyph with the
box, and PathwayDesigner's note for a custom style (SPEC-Rust §8.3). Antimony has
no layout: imported compartments get fitted boxes.

### 7.10 Resizing species

*Added in the Rust version; the reference drew every species 80 × 36.* Every
species has its own width and height (§3.2), which files have always stored; they
can now be changed. A null node keeps its size.

- **Corner handles.** Every selected species (null nodes aside) has a square handle
  at each corner of its selection halo. Dragging one resizes **every selected
  species by the same amount**, each about **its own centre**: each side moves by
  the pointer's movement, so the width changes by twice the horizontal movement and
  the height by twice the vertical one. Reactions stay attached, since the centres
  don't move. Sizes from a drag are whole units.
- **Shift keeps proportions.** With Shift held (pressing or letting go of it during
  the drag counts at once), each species is scaled by the factor the dragged one
  grows by, the larger of its width and height factors, so each keeps its own
  proportions.
- No species gets smaller than **10** either way. **Esc** during the drag puts the
  sizes back. A drag is one undo step, *Resize Species*.
- **Width** and **Height** fields in the Species group (§13.3) set every selected
  species' width or height, applied when the field loses focus; they show the first
  selected species' size. One undo step each.
- **Fit to Name** (a button in the Species group) makes each selected species as
  wide as its name, at its label size, plus **10** each side, rounded up to a
  whole unit; the height stays. One
  undo step.
- The cursor over a handle is a diagonal resize arrow, as for compartments.
- **Files.** Sizes are saved as they always were (`w`, `h` in JSON, in every
  version, SPEC-Rust §4.2; the glyph's box in SBML), so a resized model is still
  written in the version it would otherwise be.

---

## 8. Properties, styling and kinetic laws

### 8.1 Inspector panel

The inspector is driven entirely by the selection-changed notification. While it is
filling in its own controls, it MUST ignore their change events, or it will end up
editing the model by mistake.

**Species group.** Enabled when at least one species is selected. Single-value
fields are enabled only when exactly one species is selected.

| Control | Applies to | Behaviour |
|---|---|---|
| Fill colour swatch | all selected species | Opens the colour picker (§8.2). Sets `fillColor` and `hasCustomStyle`. |
| Border colour swatch | all selected species | Sets `borderColor`. |
| Border width (number) | single species | Applied when the field loses focus. Sets `borderWidth` and `hasCustomStyle`. |
| Width, Height (numbers) | all selected species | Applied when the field loses focus (§7.10). |
| Fit to Name (button) | all selected species | §7.10. |
| Shape (list) | all selected species | §5.10. Shows "Mixed" when they differ. |
| Border style (list) | all selected species | Automatic, Solid or Dashed (§5.3). Shows "Mixed" when they differ. |
| Label colour swatch | all selected species | Opens the colour picker. Sets `labelColor` and `hasCustomStyle`. Added in the Rust version. |
| Font size (number) | all selected species | The label's size, 0 = the default 12. Applied when the field loses focus. Sets `fontSize` and `hasCustomStyle`. Added in the Rust version. |
| Concentration (number) | single species | `initialValue`, applied when the field loses focus |
| Boundary species (checkbox) | selected species | `isBoundary`, applied at once |

**Reaction group.** Enabled when at least one reaction is selected.

| Control | Applies to | Behaviour |
|---|---|---|
| Line colour swatch | all selected reactions | Sets `lineColor` and `hasCustomStyle`. |
| Line width (number) | single reaction | Sets `lineWidth` and `hasCustomStyle`. |
| Show junction (checkbox) | all selected reactions | Sets `junctionVisible`. The box is ticked only if **every** selected reaction shows its junction. |
| Kinetic law (text) | single reaction | Applied when the field loses focus. Then runs modifier sync (§8.4). |

**Displayed values.** Each swatch shows the **effective** colour, which is the
custom colour or else the default, with alias colours used for aliases. When several
items are selected, the swatches show the first item's colours. When nothing is
selected, the numeric fields are cleared.

Every inspector edit SHOULD be undoable. In the reference only the colour swatches
are (§15, D-4).

### 8.2 Colour picker

A popup appears below the swatch that opened it. It contains:

- **40 palette swatches** in 8 columns × 5 rows (Appendix 17.2). The palette includes
  every default diagram colour, so a user can always set an item back to its default
  look.
- A preview of the current colour with its `#RRGGBB` value.
- A hex input. Pressing Enter or **Apply** with a valid `#RRGGBB` value applies it.
  An invalid value is shown in red.

Choosing a colour closes the popup and applies it to every selected item of the
matching kind. Each time the picker is opened and a colour is chosen, that counts as
**one** undo step: save the styles before the picker opens, and push a single
style-edit command when the colour is applied.

### 8.3 Kinetic-law expressions

Kinetic laws, parameter expressions and assignment-rule expressions are stored as
strings. Wherever they need to be understood (modifier detection, MathML export),
they are parsed with this grammar:

```
expr       := or
or         := and ( ('||' | 'or') and )*
and        := equality ( ('&&' | 'and') equality )*
equality   := comparison ( ('==' | '!=') comparison )*
comparison := additive ( ('<' | '<=' | '>' | '>=') additive )*
additive   := term ( ('+' | '-') term )*
term       := power ( ('*' | '/') power )*
power      := unary ( '^' unary )*          // right-associative
unary      := ('-' | '!' | 'not') unary | primary
primary    := NUMBER | IDENT | IDENT '(' [expr (',' expr)*] ')' | '(' expr ')'
NUMBER     := digits ['.' digits] [('e'|'E') ['+'|'-'] digits]
IDENT      := [A-Za-z_][A-Za-z0-9_]*
```

Known functions: `sin cos tan asin acos atan sinh cosh tanh exp ln log log10 sqrt
abs floor ceil ceiling pow min max`. Other function names are passed through
unchanged.

A `-` is always the subtraction or negation operator, even when a digit follows it
directly. `k1-2` MUST parse as `k1 − 2`. A number MAY also start with a decimal
point (`.5`).

In the reference, only part of this grammar works (§15, D-20):
- `&&`, `||`, `and`, `or` and `not` are not recognised. Only `!` works.
- A `-` directly before a digit is read as a negative number, so `k1-2` fails.

### 8.4 Modifier synchronisation

When a reaction's kinetic law is edited:

1. If the law is empty, remove all modifiers.
2. If the law cannot be parsed, leave the modifiers unchanged.
3. Otherwise, collect the set of identifiers in the law, then:
   - remove every modifier whose species id is no longer in the set;
   - for every identifier that matches a species id and is not one of the reaction's
     reactants or products, add that species as a modifier (no duplicates).

This keeps the diagram matching the rate law: typing `Vm*S/(Km+S)*E` adds a
modifier arc from `E`.

### 8.5 Copy Style and Paste Style

*Added in the Rust version.* The look of a species, a reaction, a compartment or a
text can be copied to others of the same kind, as a format painter does.

- **Copy Style** (the context menu of the item, or the **Edit** menu with exactly
  one item selected, null nodes aside; Ctrl+Alt+C, Cmd+Option+C on a Mac) remembers
  how that item looks. It is not an edit, so it has no undo step; the status bar
  says what was copied. What is copied depends on the kind:

  | Kind | Copied | Not copied |
  |---|---|---|
  | Species or alias | fill, border colour, border width, border style (§5.3), label colour, font size, shape (§5.10) | size, place |
  | Reaction (its junction or a leg) | line colour, line width, junction colour, whether the junction is shown | its mode (straight, Bézier, linear) and handles, which are geometry |
  | Compartment | fill, border colour, border width | its box, whether the box is shown (§7.9) |
  | Text | typeface, bold, italic, size, colour, alignment | its words, wrap width, place |

- **Paste Style** (the same menus; Ctrl+Alt+V, Cmd+Option+V) gives that look to every
  selected item **of the kind that was copied**, in one undo step; selected items of
  other kinds are left alone, so it can be used on a mixed selection. A species keeps
  its own width and height, since species need different widths for different names
  (use Fit to Name, §7.10, after); one made a circle takes its larger side both ways.
  A null node is left alone. It is shown disabled until a style has been copied and
  something of that kind is selected.
- Ctrl+Alt rather than Ctrl+Shift: Chrome keeps Ctrl+Shift+C for its developer
  tools, so a web page can't use it.
- The copied look lasts until the next Copy Style or the end of the session; it
  isn't saved. Named, saved styles are planned (§16.2).

---

## 9. Undo / redo

### 9.1 Behaviour

- Undo (Ctrl/Cmd+Z) and Redo (Ctrl/Cmd+Shift+Z, and SHOULD also accept Ctrl+Y) work
  through a linear history with a maximum depth of **100**. When the history is
  full, the oldest entry is dropped.
- Any new command clears the redo history.
- **Open file** and **New** clear the history.
- After an undo or redo, the selection is cleared, any drag in progress is
  cancelled, and the selection-changed notification is raised.
- Every command has a short description (for example "Undo Move"). The Edit menu
  SHOULD show it.
- A drag that ends where it started MUST NOT add an undo step.

### 9.2 Required command types

| Command | What it stores | Used for |
|---|---|---|
| **Snapshot** | The full model serialised before and after, plus the id counter before and after | Any change to the structure or to several kinds of data: add or delete species and reactions, rename, alias, mode toggles, smooth, make-nice, align, auto-layout, import, drags that also move stored handles or smooth junctions |
| **MoveNodes** | species id → position, and reaction id → junction position, before and after | A plain node drag |
| **MoveJunction** | reaction id, old and new position | A plain junction drag |
| **DragCtrlPt** | reaction id, role, participant index, old and new `(ctrl1, ctrl2, ctrlPtsSet)` | Dragging a handle on a reaction without a smooth junction |
| **StyleEdit** | id → VisualStyle, before and after, for species and reactions | Colour and width edits |

**Rule:** a lightweight command (anything except Snapshot) MUST look up model
objects **by id** at undo/redo time. It must never keep direct object references,
because restoring a snapshot rebuilds every object. If a drag will also change
anything beyond the positions the lightweight command stores (stored handles that
follow a node, or the inner handles of a smooth junction), take a Snapshot when the
drag **starts** instead.

---

## 10. Automatic layout

### 10.1 Force-directed layout (ForceAtlas2-style, for hypergraphs)

The layout is invoked from the toolbar. It is one undo step.

**Nodes.** Every species node and every reaction **junction** is a layout node.

- Species mass = 1 + the number of reactant or product entries that use it.
- Junction mass = 1 + number of reactants + number of products.
- Locked species are fixed. Aliases of a locked primary are also fixed.

**Forces**, computed each iteration:

| Force | Formula | Value used by the app |
|---|---|---|
| Repulsion between every pair of layout nodes (O(n²)) | `Kr·mᵢ·mⱼ / d²` along the line joining them, with `d ≥ 1` | `Kr = 8000` |
| Spring between each **unique** species and its reaction's junction | `Ka·d` | `Ka = 0.02` |
| Gravity towards the centroid of the starting positions | `Kg·m·(centre − pos)` | `Kg = 0.01` |

**Spring deduplication.** A species that is both a reactant and a product of one
reaction (`A → A + B`) gets **one** spring, not two. With two springs the junction is
pulled inside `A`.

**Step limit.** Each node moves by its total force, capped at the current `speed`.
`speed` starts at `max(SpeedStart, 0.05 × diagonal of the starting bounding box)`
with `SpeedStart = 20`, and is multiplied by `0.98` after every iteration. The app
runs **800** iterations.

**Aliases** follow their primary rigidly. After each iteration, set
`alias = original alias position + total movement of the primary so far`.

**Re-centring.** When the iterations are done, move everything so the centroid is
back at its starting position. Then write the positions back to the model.

**Options** (two checkboxes next to the toolbar button):

- **Randomize:** before the layout, put every unlocked species at a random position in
  [100, 500)², put each junction at the centroid of its participants, and reset every
  participant to automatic handles.
- **Deckard:** after the layout, run the Bézier handle pass (§10.2). Then
  re-centre once more, moving the stored handles too.

After the layout, every Bézier reaction with a smooth junction has its handles
recomputed (§7.4.4). (Note: when *Deckard* is off, stored handles of reactions
without a smooth junction are left where they were, §15, D-11.)

### 10.2 Bézier handle pass (Deckard & Bergmann, 2006)

For every Bézier reaction:

1. **Junction.**
   - If any species is both a reactant and a product (a "looped" species `A`), put
     `J` at the centroid of the **unique** participant species. For `A → A + B`
     that is `(A + B)/2`.
   - Otherwise put `J` at `(CentR + CentP)/2`.
2. **Substrate guide** `G` = mean of (all reactant centres, plus `J`).
3. If the reaction is looped: `G = J + 50·perp(unit(J − A))`.
   Otherwise, if it is UniUni: `G = J + |J − G|·unit(Reactant − Product)`.
4. Pull back: `G = J + 25·unit(G − J)`, so the inner handle is always 25 units from
   `J`.
5. Reactants: `ctrl2 = G`, `ctrl1 = S + 0.35·(J − S)`.
   Products: `ctrl1 = 2J − G` (the reflection of `G` through `J`),
   `ctrl2 = S + 0.35·(J − S)`.
   Set `ctrlPtsSet = true` for all of them.
6. **Repeated species on one side** (for example `2A` written as `A + A`): rotate the
   two outer-handle vectors (handle − species centre) by +20° and −20°, and make each
   10 units longer.
7. **Looped species:** with `p = perp(unit(J − A))`, do reactant `A`'s
   `ctrl1 += 30·p` and product `A`'s `ctrl2 −= 30·p`.

---

## 11. File formats and interchange

### 11.1 Native JSON (`.json`), format version 3

This is the only lossless format. Write it as UTF-8 with 2-space indentation.

```json
{
  "version": 3,
  "modelName": "",
  "compartments":    [ { "id": "c1", "size": 1.0, "dimensions": 3, "style": { } } ],
  "parameters":      [ { "variable": "k1", "expression": "0.1" } ],
  "assignmentRules": [ { "variable": "Vt", "expression": "Vm*E" } ],
  "species": [
    { "id": "S1", "x": 120.0, "y": 80.0, "w": 80.0, "h": 36.0,
      "initialValue": 1.0, "isBoundary": false, "isConstant": false,
      "compartment": "c1",          // optional — omitted when ""
      "aliasOf": "S0",              // optional — present only on aliases
      "isNullNode": true,           // optional — present only when true
      "style": { } }                // optional — present only when hasCustomStyle
  ],
  "reactions": [
    { "id": "R1", "jx": 200.0, "jy": 80.0,
      "kineticLaw": "k1*S1", "isReversible": false,
      "isLinear": false, "isBezier": true, "isJunctionSmooth": false,
      "reactants": [ { "id": "S1", "stoichiometry": 1.0,
                       "ctrl1x": 0, "ctrl1y": 0, "ctrl2x": 0, "ctrl2y": 0,
                       "ctrlSet": true } ],      // ctrl* keys only when ctrlPtsSet
      "products":  [ { "id": "S2", "stoichiometry": 1.0 } ],
      "modifiers": [ { "id": "E" } ],
      "style": { } }
  ]
}
```

**Style object keys:** `fillColor`, `borderColor`, `labelColor`, `lineColor`,
`junctionColor`, `borderWidth`, `lineWidth`, `fontSize`, `junctionVisible`. A
missing key means 0 (or `true` for `junctionVisible`).

**Colours** are strings in the form `#RRGGBBAA`. Readers MUST also accept `#RRGGBB`
(treated as opaque). Readers MUST also accept legacy files in which a colour is a
signed 32-bit integer in `0xAARRGGBB` form.

**Version history.** A reader MUST accept versions 1–3. A missing `version` means 1.

| Version | Differences |
|---|---|
| 1 | `reactants` and `products` are arrays of species-id **strings**. Stoichiometry is 1. |
| 2 | Participant objects with stoichiometry. Biochemical fields, compartments and parameters. |
| 3 | Optional `style` objects. Hex colours. |

`modifiers` may be missing in any version. That means no modifiers.

**Load procedure.**

1. Clear the model.
2. Load compartments, parameters and rules.
3. Create all species.
4. In a second pass, resolve `aliasOf` by id.
5. Create the reactions. Participants refer to species by id. A participant whose
   species is missing is skipped.
6. Recompute the id counter.

Loading clears the undo history and the selection, and returns to Select mode.

**Save.** Write the model and its view-independent data only. Selection, zoom and
scroll are not saved.

**Round-trip:** load followed by save MUST produce semantically identical JSON.

### 11.2 Antimony import

The reference parser handles a **subset** of Antimony. A re-implementation MUST
accept at least the following, and SHOULD use a complete Antimony parser
(for example libAntimony) where one is available.

**Lexical rules.**

- `//` and `#` start a comment that runs to the end of the line. A `#` followed by
  six hex digits is a colour, not a comment.
- Statements end with `;` or a newline. The `;` is optional.

**Statements.**

| Form | Meaning |
|---|---|
| `model <name>` … `end` | Optional wrapper. Sets `modelName`. Without it the name is `anonymous`. |
| `compartment c1 [= size] [, c2 …]` | Declare compartments. |
| `species [$]S [in C] [= value] [, …]`, also `var …` | Declare species. `$` means boundary. |
| `$S = value` | Declare a boundary species with an initial value. |
| `[id:] [n[*]][$]A + … -> [n[*]][$]B + … [; rateExpr]` | A reaction. Either side may be empty. |
| `x = expr` | If `x` is a species, set its initial value (must be a number). If `x` is a compartment, set its size. Otherwise `x` is a parameter. |
| `x := expr` | Assignment rule |

**Arrow.** `->` is the reaction arrow. `=>` MUST also be accepted, as a plain
synonym for `->`.

**Implicit declarations.**

- A species used in a reaction but never declared is created with initial value 0.
- An identifier used in a rate law that is neither a species nor assigned anywhere
  becomes a parameter with value `0`.

**Mapping to the diagram.**

1. Clear the model and set `modelName`.
2. Copy the compartments.
3. Copy every assignment whose variable is not a species as a parameter.
4. Copy the assignment rules.
5. Create one node (80 × 36) for every species, at a random position in
   [0, 600)². Ids keep their declared spelling. If a reaction refers to a species
   with different letter case, it resolves to the declared spelling, matched
   case-insensitively (the reference does this). Copy `isBoundary`, `isConstant`,
   `compartment` and `initialValue`.
6. For each reaction:
   - Create it with the declared id, or `R<k>` if it has none.
   - Copy the participants with their stoichiometry, and the rate
     law. The rate law is stored as the parser prints it back: fully parenthesised,
     with the outermost parentheses removed.
   - Put the junction at `(CentR + CentP)/2`.
   - **Catalytic conversion** (when the preference is on, §12): any species that
     appears on **both** sides with **equal** stoichiometry is removed from both
     sides and becomes a **modifier**. So `A -> A + B; k*A` becomes `-> B; k*A`
     with `A` as a modifier, and so does `A -> A + B` with no rate law. (The
     reference only makes `A` a modifier through the rate law, so without one `A`
     ends up connected to nothing.)
   - **Modifiers:** every species identifier in the rate law that is not a
     participant becomes a modifier. If it has no node yet, create one. After
     conversion, this is how `A` above becomes a modifier.
   - **Null nodes:** if the reactant side is now empty, add a null node
     `_source_<rid>` (28 × 28) as the reactant. If the product side is empty, add
     `_sink_<rid>` as the product.
   - The render mode is Straight.
7. Clear the selection and set the scroll offset to (30, 30). The import is one undo
   step.

A re-implementation SHOULD run auto-layout (§10) after an import, or offer to. The
reference leaves the random positions in place.

**Errors.** A parse error MUST be reported to the user with a message, and the
current model MUST be left unchanged.

### 11.3 Antimony export

Output is plain text, one statement per line, indented by two spaces, in this order.
Sections are separated by blank lines and introduced by `// Section` comments.

1. `compartment C = <size>;` for each compartment other than `defaultCompartment`.
2. `species [$]S [in C];` for each **primary, non-null** species.
3. `<rid>: [n ]A + [n ]B -> [n ]C; <law>;` for each reaction:
   - Null-node participants are left out, so a reaction with a null source is
     written `R1:  -> B; k*A;`.
   - A stoichiometry other than 1 is written in front of the species name.
   - Boundary species get a `$` prefix.
   - An empty rate law is written as `v`.
   - The arrow is always `->`.
4. `k = <expression>;` for each parameter.
5. `S = <initialValue>;` for each primary, non-null species.
6. `x := <expression>;` for each assignment rule.

Aliases are never exported. Numbers MUST be formatted with a `.` decimal separator
whatever the system locale is. A re-implementation SHOULD wrap the output in
`model <name>` … `end`.

**Round-trip.** Importing an exported file MUST give back the same biochemistry
(species, reactions, stoichiometry, rate laws, parameters, initial values). Layout
is not preserved.

### 11.4 Live Antimony panel

A multi-line text editor beside the canvas, with two buttons:

- **Load Antimony** imports the panel's text (§11.2) and redraws. The import is one
  undo step. Parse errors are reported.
- **Get Antimony** replaces the panel's text with the export of the current model
  (§11.3).

The application remembers the last imported Antimony text, so it can re-import it
when the catalytic-conversion preference changes (§12).

### 11.5 SBML export (Level 3 Version 1 + Layout)

File ▸ *Save to SBML* writes a UTF-8 `.xml` file. A re-implementation MAY use
libSBML. The reference writes the XML itself.

**Document element.**

```xml
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1"
      xmlns:layout="http://www.sbml.org/sbml/level3/version1/layout/version1"
      layout:required="false">
  <model id="<SId from modelName, or 'model'>" name="<modelName>">
```

**Core content.**

| Element | Content |
|---|---|
| compartments | Add `defaultCompartment` (size 1, 3-D, constant) if it is missing, then every model compartment (`constant="true"`). |
| species | One per **primary** species. Null nodes SHOULD be left out. Attributes: `compartment` (defaults to `defaultCompartment`), `initialConcentration`, `hasOnlySubstanceUnits="false"`, `boundaryCondition`, `constant`. |
| parameters | One per parameter whose expression is a number, with `constant="true"`. Parameters with non-numeric expressions SHOULD become an `initialAssignment`. |
| reactions | `reversible`, `fast="false"`. Reactants and products as `speciesReference` elements with `stoichiometry` and `constant="true"`. Modifiers SHOULD be written as `modifierSpeciesReference` (§15, D-10). |
| kineticLaw | The rate law converted to Content MathML (table below). If the law is empty or cannot be parsed, leave out the `<kineticLaw>` element entirely. |
| rules | Assignment rules SHOULD be written as `assignmentRule` elements. The reference leaves them out. |

**Converting the rate law to MathML.**

| Expression part | MathML |
|---|---|
| numbers | `<cn>` |
| identifiers | `<ci>` |
| `+ - * / ^` | `plus minus times divide power` |
| unary minus | `<minus/>` with one argument |
| `< <= > >= == !=` | `lt leq gt geq eq neq` |
| `&& \|\| !` | `and or not` |
| `asin acos atan` | `arcsin arccos arctan` |
| `log`, `log10` | `<log/>` with `<logbase><cn>10</cn></logbase>` |
| `sqrt` | `<root/>` with `<degree><cn>2</cn></degree>` |
| `ceil`, `ceiling` | `<ceiling/>` |
| `pow` | `<power/>` |
| other functions | the element with the same name |

**Layout content** goes inside `<model>`:

```xml
<layout:listOfLayouts xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <layout:layout layout:id="layout1">
    <layout:dimensions layout:width="maxX+80" layout:height="maxY+80"/>
```

- **One `speciesGlyph` per node, primary *and* alias.** `id="sg_<nodeId>"`.
  `species` = the node's own id, or its primary's id for an alias. The bounding box
  is the node's rectangle.
- **One `reactionGlyph` per reaction.** `id="rg_<rid>"`, with a zero-size bounding
  box at the junction.
- **One `speciesReferenceGlyph` per reactant and product.** Modifiers SHOULD also get
  one, with `role="modifier"`.
  - Reactants: `id="srg_<rid>_sub_<sid>"`, `role="substrate"`.
  - Products: `id="srg_<rid>_prod_<sid>"`, `role="product"`.
  - Each points at the glyph of the node actually used, which may be an alias.
  - Each has a curve with one segment:
    - Reactant: from the node's **border point** towards the junction, to the
      junction.
    - Product: from the junction to the node's border point.
    - Bézier reactions use `xsi:type="CubicBezier"` with `basePoint1 = ctrl1` and
      `basePoint2 = ctrl2`, using the stored or automatic handles. Because of the
      convention in §5.6.1, no reversal is needed.
    - Other reactions use `xsi:type="LineSegment"`.
  - Ids MUST be unique. If one species appears twice on the same side, add a suffix
    (§15, D-13).
- **One `textGlyph` per node.** `id="tg_<nodeId>"`, `graphicalObject = sg_<nodeId>`,
  `originOfText` = the species id. The bounding box is the node's rectangle.

All numbers MUST be written with a `.` decimal separator whatever the locale.

**SBML import** is not part of the reference release (the code for it is disabled).
See milestone F-1.

### 11.6 Graphics output

All three outputs draw the **content bounds**: the union of all species rectangles
and junction points, plus a 60-unit margin, or 800 × 600 if the model is empty.
Each output saves the view's zoom and scroll first, draws with its own transform,
and restores them afterwards.

| Output | Behaviour |
|---|---|
| **PNG** | Scale = DPI/96, default 150 DPI. The image size is the content bounds times the scale. |
| **PDF** | Vector output on one page, US Letter portrait by default. A4 and landscape SHOULD be offered. The content is scaled to fit the page and centred. The Rust version also offers **Drawing size**, a page exactly the size of the drawing. |
| **Print** | The content is scaled to fit the printer page and centred. A print dialog SHOULD be shown (the reference prints straight to the default printer). |

The drawing used for output MUST be identical to the screen drawing, minus
everything that depends on what the user is doing at the moment: selection halos,
the larger size and line width of selected items, Bézier handles, the tooltip, the
rubber band and the pending-reaction feedback. The background is the canvas colour
(`#F8F9FA`). (The reference draws all of these, §15, D-21.)

---

## 12. Preferences

Preferences are kept in a small JSON file in the user's settings folder:

- Windows: `%APPDATA%\PathwayDesigner\settings.json`
- macOS: `~/Library/Application Support/PathwayDesigner/settings.json`

The file is read at startup, and written when the preferences dialog is closed with
OK and when the application exits.

| Preference | Default | Effect |
|---|---|---|
| **Convert catalytic reactions** | true | During Antimony import, `A -> A + B` becomes `-> B` with `A` as a modifier (§11.2). If this setting changes and an Antimony source has been imported, that source is imported **again** so the change takes effect. (The user SHOULD be warned that this throws away edits made since the import.) |
| **Show null species** | true | Draw null nodes and the legs that touch them (§5.4). |
| **Selected reaction colour** | `#1144CC` | The colour a selected reaction's lines, arrowheads and modifier arcs are drawn in (§5.5), with a Reset button. Added in the Rust version. Never used in exports. |

The preferences dialog is a modal window with one tab, holding two checkboxes, the
selected reaction colour and OK/Cancel buttons.

The saved preference values MUST be applied when the application starts (§15, D-2).

---

## 13. User interface layout

The application has one window, titled "Biochemical Network Editor" (followed by the
file name when a file is open).

### 13.1 Main menu

| Menu | Items |
|---|---|
| **File** | New · Open… · Save… · *(separator)* · Import Antimony… · Export Antimony… · *(separator)* · Import SBML… · Save to SBML… · *(separator)* · Export ▸ {SVG…, PNG…, PDF…} · Print… · Quit |
| **Edit** | Undo · Redo · *(separator)* · Select All · Select All Species · Select All Reactions · *(separator)* · Copy Style · Paste Style (§8.5) · *(separator)* · Preferences… |
| **Help** | About (shows the version and build date) |

File dialogs: Open and Save use `*.json`. Antimony uses `*.ant` and `*.txt`. SBML
uses `*.xml`. Export offers `diagram.png` and `diagram.pdf` and asks before
overwriting a file.

A re-implementation SHOULD add: **Save** that remembers the current file (with Save
As… for a new name), a modified-document indicator, and a prompt to save unsaved
changes on New, Open and Quit. The reference has none of these.

### 13.2 Toolbars

- **Top toolbar:** New · Open · Save · Toggle Alias indicator · Auto-layout · Linear
  UniUni · Random network · Set Bézier · Smooth junction · Make Nice, then the
  checkboxes **Deckard** and **Randomize** (both off by default), which apply to
  auto-layout.
- **Left tool palette** (modes): Select · Add Species · Add Text (§7.8) · Add
  Compartment (§7.9) · Add BiBi · Add UniBi · Add BiUni · Add UniUni.
- Each tool button MUST have a tooltip. The current mode SHOULD be highlighted.

### 13.3 Right panel

Two tabs at the top: **Properties** and **Antimony**. *(Before 0.1.7 the panel
showed every group at once, most of them disabled, with the Antimony text between
the Species and Reaction groups; there was no room left for more.)*

- **Properties** shows the groups for what is selected, and only those, in this
  order: the **Species** group, the **Reaction** group (§8.1), the **Compartment**
  group (§7.9: Id, Volume, Fill, Border, Border width, for one selected compartment;
  the colours, and Show box, for every selected one), and the **Text** group (§7.8).
  With nothing selected it shows a one-line hint instead.
- **Antimony** has the Antimony text with its *Load Antimony* and *Get Antimony*
  buttons, and its errors below. The panel switches to this tab when a file opened
  or imported as Antimony has errors, so that they can be seen.

The Text group has the words of the one
selected text (applied as they are typed), then Typeface, Size, Style (Bold,
Italic), Align (Left, Center, Right), Colour and Wrap width. All but the words and
the wrap width apply to every selected text.

### 13.4 Context menu

Which items appear depends on what was right-clicked:

| Target | Items |
|---|---|
| Primary species | Create Alias · Split into Aliases (one per reaction) · Split Off Selected Reactions · Merge Aliases · Merge Selected Aliases into This · Rename · Delete Species · Copy Style · Paste Style · Alignment ▸ … |
| Null node | Remove Null Node · Alignment ▸ … |
| Alias | Rename (renames the primary) · Go to Primary · Split Off Selected Reactions · Merge Selected Aliases into This · Merge into Primary · Merge All Aliases into Primary · Delete Alias · Copy Style · Paste Style · Alignment ▸ … |
| Reaction (junction or leg) | Delete Reaction · Adjust Reaction (Make Nice) · Copy Style · Paste Style |
| Free text (§7.8) | Edit Text · Delete Text · Copy Style · Paste Style · Alignment ▸ … |
| Compartment (§7.9, its border or name) | Delete Compartment (keeps its species) · Copy Style · Paste Style |

The *Alignment ▸* submenu contains Align Top / Middle / Bottom, Align Left / Center
/ Right, and Distribute Horizontally / Vertically.

A **Lock/Unlock** item MUST toggle `locked` on the selected species, and locked
nodes MUST look different. Locking pins a node in place: no drag, alignment or
layout moves it (§6.4, §7.5, §10.1), which lets the user auto-layout part of a
network while the rest stays fixed. The reference shows this item but it does
nothing.

### 13.5 Status bar

The status bar MAY show the pointer coordinates. (It used to warn that compartments
weren't drawn; they are now, §7.9.)

At its right end is a zoom control: **− 95% +**. The buttons zoom out and in as
the View menu's Zoom Out and Zoom In do (§7.6). Clicking the percentage zooms to
fit. Small controls about the view, rather than the diagram, belong here and in
the View menu, which keeps the toolbar for the reaction commands.

---

## 14. Non-functional requirements

| Area | Requirement |
|---|---|
| **Architecture** | Keep these layers separate: (1) domain model, with no UI types; (2) pure geometry; (3) undo, knowing only the model; (4) layout, knowing only the model; (5) import/export, knowing only the model; (6) the view, which holds view state, draws onto a canvas it is given, and runs the interaction state machine (it owns no widgets); (7) the UI shell. Requirements §11.6 and §16 depend on the view being able to draw onto any canvas: screen, bitmap, PDF or printer. |
| **Performance** | Panning, zooming and dragging SHOULD feel smooth (about 60 fps) with up to about 200 species and 300 reactions. Auto-layout is O(n²) per iteration, so it SHOULD show progress or run in the background for large models. |
| **Numerics and locale** | Every file format MUST write and read `.` as the decimal separator, whatever the user's locale. |
| **Encoding** | All files are written as UTF-8. A byte-order mark MUST be accepted on read. |
| **Robustness** | A malformed file MUST give an error message and leave the current model unchanged. Missing optional JSON keys get their default values. |
| **Accessibility** | Every tool MUST have a text label or tooltip. Keyboard shortcuts for Undo, Redo, Select All, Delete and Escape are required. |
| **Licence** | The reference is MIT-licensed (© 2026 Herbert M. Sauro, NIBIB EB028887). Any third-party SBML or Antimony libraries used must have compatible licences. |

---

## 15. Known deviations in the reference implementation (v0.38)

These are places where the reference code does **not** do what this specification
says. They are listed so a new implementation does not copy them by mistake. Each
one is also a candidate for fixing in the reference.

| # | Area | Reference behaviour | Specified behaviour |
|---|---|---|---|
| D-1 | Alias label | An alias shows its **own** generated id (for example `S7`). | Show the primary's id (§3.2). |
| D-2 | Catalytic preference | The Antimony importer starts with conversion **off**, whatever the saved preference says. The preference only takes effect after the Preferences dialog is closed with OK. | Apply the saved preference at startup (§12). |
| D-3 | Rename | No check for uniqueness or validity. Double-click rename does not update kinetic laws. Context-menu rename does not update laws either, because it renames the node before calling the law update, and it is not undoable. | Check the new id, rewrite the laws, make it undoable (§7.1). |
| D-4 | Undo coverage | These are **not** undoable: inspector numeric and checkbox edits, the rate-law edit, the context-menu Delete, context-menu Rename and Random network. | Everything that changes the model is undoable. |
| D-5 | Plain wheel and scrolling | Scrollbars always cover 0–8000. A plain mouse wheel does nothing. | Scroll range from the content bounds. The wheel pans (§6.6). |
| D-6 | Delete cascade | A deleted species stays in other reactions' **modifier** lists, leaving references to a deleted object. | Remove it from modifier lists too (§3.7). |
| D-8 | Junction visibility | `junctionVisible = false` is only saved if the reaction also has some other custom style. Otherwise it is lost on save. | Always save it (§3.6). |
| D-9 | SBML null nodes | Null nodes are exported as real SBML species. | Leave them out. Express the empty side as an empty reactant or product list (§11.5). |
| D-10 | SBML modifiers and rules | Modifiers and assignment rules are not exported. Non-numeric parameters are dropped. | Export them (§11.5). |
| D-11 | Stale handles after layout | When *Deckard* is off, stored handles of reactions without a smooth junction stay at their old absolute positions while the nodes move. | Either reset them to automatic handles or always run the handle pass. |
| D-12 | Whole-word rename | Kinetic-law rewriting ignores letter case, so renaming `a` also changes `A`. | Match case-sensitively. |
| D-13 | SBML glyph ids | `srg_<rid>_sub_<sid>` is duplicated when one species appears twice on the same side. The model id becomes `id` when the model name is empty. | Make ids unique. Use `model` as the fallback model id. |
| D-14 | Antimony export locale | Numbers are formatted with the system locale. | Always use `.` (§14). |
| D-15 | Go to primary | Scrolls so the primary is at the top-left corner of the viewport. | Centre the primary (§7.3). |
| D-16 | Lock / Unlock | The context-menu item does nothing. Locking is only honoured by auto-layout, and only through the JSON field. | Make it work (§13.4). |
| D-17 | Fan index comment | Code comments say the fan index counts the legs on a *species node*. The code actually counts the legs of the *same role on the reaction*. | Follow the code (§5.6.2). |
| D-18 | Version string | `package.json` says 0.3.0 while the app says 0.38. | Use one source for the version. |
| D-19 | Antimony `model` wrapper | `model <name>` is accepted, but the closing `end` is read as an ordinary identifier and reported as an error. `model name()` with parentheses is not supported. | Accept the standard wrapper (§11.2). |
| D-20 | Expression lexer | The lexer never produces `&&` or `\|\|` (a single `&` or `\|` is an unknown token). The parser does not recognise `and`, `or` or `not`. A `-` directly before a digit is read as part of a negative number, so `k1-2` is a parse error. A number cannot start with `.`. | Accept the full grammar in §8.3. |
| D-21 | Output drawing | PNG, PDF and print use the full screen drawing. Selection halos, Bézier handles and, if visible, the tooltip all appear in the output. | Leave out everything that depends on the current selection or mouse position (§11.6). |

---

## 16. Milestone plan

The plan builds a complete, independent implementation in **13 milestones**. Every
milestone ends with something that can be demonstrated and with acceptance criteria
that can be tested. Each milestone builds on the earlier ones.

The reference repository includes sample files in `Models/` (`*.json`, `*.ant`).
These MUST be used as conformance fixtures from M1 onwards.

### M0 — Foundations

**Scope:**

- Choose the technology: language, UI toolkit, and a 2-D canvas with anti-aliasing,
  dashed strokes, cubic curves, text measurement and PDF output.
- Set up the project skeleton, CI and the unit-test framework.
- Write the pure geometry library: vector maths, point-to-segment distance,
  rectangle-border intersection, the arrowhead triangle, world↔screen transforms,
  Bézier evaluation, de Casteljau splitting, and the bisection that finds where a
  curve crosses a rectangle.

**Acceptance:**

- Unit tests cover every geometry function, including the degenerate cases: a
  zero-length vector, a point equal to the centre, a zero-length segment.
- The geometry module imports nothing from the UI toolkit.

### M1 — Domain model and native file format

**Scope:**

- All entities in §3, with the id rules in §4.
- The model operations, including the delete cascade (with modifier clean-up) and
  rename with kinetic-law rewriting.
- A JSON v3 writer and a v1/v2/v3 reader (§11.1), including the legacy integer
  colours.

**Acceptance:**

- Every `Models/*.json` file loads. Saving and reloading gives the same model
  (compare field by field).
- A v1 file and a v2 file load correctly.
- Unit tests cover the cascade, rename, the id counter after load, and alias
  resolution.

### M2 — Static rendering and the view

**Scope:**

- A canvas with zoom and pan.
- The full drawing pipeline from §5 for **Straight** and **Linear** reactions: species
  nodes, aliases, boundary borders, null nodes, arrowheads with their gaps and insets,
  modifier arcs, junction handles and labels.
- The view can draw onto any canvas it is given.

**Acceptance:**

- The sample files look the same as in the reference v0.38 (compare screenshots side
  by side).
- Ctrl+wheel zoom keeps the point under the cursor fixed.
- Zooming to 8× and 0.1× works without artefacts.

### M3 — Selection and direct manipulation

**Scope:**

- Hit testing in priority order (§6.2).
- Click, Shift-click and rubber-band selection.
- Dragging species groups, with junctions following.
- Dragging junctions, including the sliding behaviour of Linear reactions.
- Delete, Esc and Select-All.
- The selection-changed notification.
- An undo framework with the MoveNodes, MoveJunction and Snapshot commands.

**Acceptance:**

- Every rule in §6.3 has a scripted or manual test.
- Dragging a fully selected reaction moves its junction. Dragging a partly selected
  one does not.
- Undo and redo of moves and deletes work. A drag that ends where it started adds
  nothing to the history.

### M4 — Creating and editing structure

**Scope:**

- The AddSpecies tool.
- AddReaction tools for all four arities, with the pending-reaction feedback, the
  default mass-action law and parameter, and the default render mode.
- Rename, with its checks.
- Aliases: create, go to primary, delete, alias indicator toggle.
- The context menu (§13.4).
- New.
- Undo for all of these.

**Acceptance:**

- A three-reaction network can be built using only the mouse.
- `k<N>` parameters appear with value 0.1.
- Renaming a species updates every kinetic law that uses it.
- Deleting a primary removes its aliases and the reactions that use them.
- Every action can be undone and redone at least 20 steps deep.

### M5 — Bézier curves

**Scope:**

- Automatic handles with fan offsets (§5.6.2).
- Clipping at node borders, with arrowheads that follow the curve's tangent.
- Display and dragging of handles, with the handles saved on first grab.
- Smooth junctions: computing the handles, and the in-line rule while dragging.
- Make Nice, Set Bézier, Toggle Linear and Set Straight.
- Stored handles that move with node and junction drags.
- The DragCtrlPt command, and Snapshot escalation where §9.2 requires it.

**Acceptance:**

- For BiBi, UniBi and `A → A + B` reactions, the curves do not overlap, and each
  arrowhead lies along its curve at the node border.
- Moving a node never makes a stored handle jump.
- Every handle operation can be undone.

### M6 — Layout tools

**Scope:**

- Align and Distribute (§7.5), including how they affect junctions and handles.
- The force-directed auto-layout (§10.1), with locking and the Randomize option.
- The Deckard handle pass (§10.2).
- The Lock/Unlock command, with a visual cue for locked nodes.

**Acceptance:**

- Laying out a 10-species, 12-reaction random network gives a drawing with no
  overlapping nodes. Junctions sit between their participants, and running the
  layout twice does not make the drawing drift.
- Locked nodes do not move.
- The two arcs of `A → A + B` stay visibly apart.
- The random network generator (§7.7) exists as a test tool.

### M7 — Kinetic laws, properties and styling

**Scope:**

- The expression parser (§8.3).
- Modifier synchronisation (§8.4).
- The inspector panel (§8.1), with its feedback-loop guard.
- The colour picker (§8.2).
- The StyleEdit command.
- Hover tooltips.

**Acceptance:**

- Typing `Vm*S/(Km+S)*E` adds a modifier arc from `E`. Removing `E` from the law
  removes the arc.
- Styles are saved to JSON and come back exactly.
- Every inspector edit can be undone.
- A species with only its fill colour customised still shows the default border.

### M8 — Antimony interchange

**Scope:**

- Antimony import (§11.2): parsing, the mapping to the diagram, case resolution,
  catalytic conversion, null nodes, modifier detection, and error reporting.
- Antimony export (§11.3).
- The live Antimony panel (§11.4).
- The Preferences dialog and its storage (§12), including re-importing when the
  catalytic setting changes.

**Acceptance:**

- Every `Models/*.ant` file imports.
- Exporting then re-importing keeps the biochemistry.
- `A -> A + B; k*A` gives `-> B` with modifier `A` when conversion is on, and keeps
  the stoichiometry when it is off.
- `-> A` and `A ->` produce null nodes, which hide and show with the preference.
- A file with a syntax error shows an error and leaves the diagram unchanged.

### M9 — SBML export with Layout

**Scope:**

- The SBML L3V1 core: compartments, species, parameters, reactions, modifiers and
  rules.
- MathML conversion.
- The Layout package with glyphs for aliases and border-anchored curves (§11.5).

**Acceptance:**

- The output passes the SBML online validator (or libSBML `checkConsistency`) with
  no errors.
- The output opens in a layout-aware tool (for example SBMLLayout or COPASI) with
  the positions preserved.
- Number formatting does not depend on the locale (test with a locale that uses a
  comma as the decimal separator).

### M10 — Graphics output

**Scope:** PNG export (choice of DPI), PDF export (page size and orientation), and
printing (§11.6).

**Acceptance:**

- PNG and PDF match the screen drawing without the overlays.
- The PDF is vector (its text can be selected).
- Content is centred and scaled to fit the page.

### M11 — Document management and polish

**Scope:**

- Remember the current file, with Save and Save As.
- Track unsaved changes and ask before New, Open and Quit.
- Show the window title, the About box and the status-bar messages (§13.5).
- Keyboard shortcuts.
- Error handling for every file operation.

**Acceptance:**

- Closing with unsaved changes always asks first.
- A malformed file never crashes the application or loses the current model.

### M12 — Packaging and release (parity with reference v0.38)

**Scope:**

- Installers or packages for Windows and macOS.
- A user README.
- Conformance testing against the checklist in Appendix 17.4.

**Acceptance:** every MUST in this document passes. Every deviation from the
reference is either listed in §15 or fixed.

### 16.1 How the milestones map to reference releases

| Reference version | Covered by |
|---|---|
| 0.1.0: editing, zoom and pan, aliases, straight and basic Bézier, Antimony subset, JSON, auto-layout | M1–M6, M8 |
| 0.1.1: undo/redo, better Bézier | M3–M5 |
| 0.1.2: SBML export with Layout, experimental colours | M9, M7 |
| 0.2: align and distribute, better layout, live Antimony panel, hover tooltips | M6, M8, M7 |
| 0.22: printing, PNG and PDF | M10 |
| 0.30: property editing, junction visibility, colour picker, hex colours | M7 |
| 0.32: null species, preferences, catalytic conversion | M8 |
| 0.37: modifiers, editable rate law, rate-law ⇄ diagram sync | M7, M8 |
| 0.38: show-null preference, better Deckard handles for `A → A + B`, separate arrowhead inset | M5, M6, M8 |

### 16.2 Future milestones (after parity)

These come from the README's "Not yet supported" list and from gaps in the code.

| # | Milestone | Notes |
|---|---|---|
| F-1 | **SBML import with Layout import** | Positions come from `speciesGlyph`/`reactionGlyph` when present. Otherwise run auto-layout. Use `initialConcentration`, falling back to `initialAmount`. Import modifiers and rules. |
| F-2 | **Compartment drawing** | A compartment is drawn as a rounded box containing its species, with its own style. This replaces the status-bar warning. **Done: §7.9.** |
| F-3 | **Font size and label colour controls** | The model already stores `fontSize` and `labelColor`. Only the UI is missing. **Done: §8.1**, with Copy Style and Paste Style (§8.5). |
| F-4 | **Node shapes** | Rectangle, ellipse, circle and polygon. Border intersection and hit testing must handle each shape. **Done: §5.10** (rectangle, ellipse, circle, hexagon; hit testing keeps the box), with resizing (§7.10). |
| F-5 | **Label positioning** | Inside, above, below, left or right of the node. |
| F-6 | **Style themes** | For example an Escher-like theme: global style presets on top of the per-item styles. First, **saved styles**: a look from Copy Style (§8.5) saved under a name and applied from a list, kept with the app and exportable to a file. The styles a model uses must also be saved in the model file, so that someone given the model can apply and change them; the nodes themselves always carry their own colours, so the model looks the same either way. Diagram-wide themes (new defaults for everything without a custom style) after that. |
| F-7 | **Arrange in a circle** | Put the selected nodes on a circle. |
| F-8 | **Human-readable layout format** | Convert between the JSON format and a text format for layout ("Alcuin"). The reference lexer already has keywords for it (`layout`, `at`, `size`, `style`, `bezier`, `cp`, …). |
| F-9 | **Several documents** | Tabs or several windows. |
| F-10 | **Full Antimony** | Events, functions, modules and units, through a complete parser. |

---

## 17. Appendices

### 17.1 Constants (world units unless marked)

| Name | Value | | Name | Value |
|---|---|---|---|---|
| Default species size | 80 × 36 | | Null node size | 28 × 28 |
| Node corner radius | 8 | | Label font size | 12 |
| Label padding (auto-width) | 10 per side | | Default line / border width | 1.5 |
| Boundary border factor | × 2.4 | | Selected line width factor | × 1.5 |
| Junction radius (selected) | 4 (6.4) | | Modifier dot radius / offset | 4 / 10 |
| Reactant gap / product line gap | 6 / 6 | | Arrow tip inset | 3 |
| Arrow length / half-base | 12 / 5 | | Selection halo outset / width | 6 / 2 |
| Pending ring outset / width | 4 / 2.5 | | Alias offset on creation | (24, 24) |
| Handle radius | 4 | | Handle hit tolerance | 8 screen px |
| Junction hit tolerance | 8 screen px | | Leg hit tolerance | 6 screen px |
| Bézier hit samples | 20 | | Border-crossing bisection steps | 24 |
| Handle fraction along leg | 0.35 | | Fan spread | 50 |
| Smooth loop: outer fan / inner offset | 30 / 25 | | Deckard: loop guide / pull-back | 50 / 25 |
| Deckard duplicate fan | ±20°, +10 | | Zoom step / range | ×1.15 / [0.1, 8] |
| Tooltip delay | 600 ms | | Undo depth | 100 |
| Layout: Kr / Ka / Kg | 8000 / 0.02 / 0.01 | | Layout: start speed / decay / iterations | max(20, 5 % of diagonal) / 0.98 / 800 |
| Content margin for export | 60 | | PNG default DPI | 150 |
| Free text: default size | 14 | | Free text: box padding / narrowest wrap | 4 / 20 |
| Compartment: border width / corner radius | 2 / 14 | | Compartment: default box / smallest | 300 × 200 / 40 |
| Compartment: name inset / size | 8 / 12 | | Compartment: fitting space | 30 |

### 17.2 Palette

**Diagram defaults:**

| Use | Colour |
|---|---|
| Background | `#F8F9FA` |
| Species fill / border | `#EEF6FF` / `#4A7FCB` |
| Alias fill / border | `#F5F0FF` / `#7A6FC8` |
| Label | `#1A1A1A` |
| Reaction line | `#444444` |
| Selected reaction accent (modifier arcs) | `#1144CC` |
| Junction fill / border / selected border | `#FF8800` / `#A05000` / `#0033AA` |
| Modifier | `#007777` |
| Null node fill / border | `#B0B0B0` / `#505050` |
| Selection halo | `#FF0000` |
| Pending reactant / product ring | `#00AA44` / `#CC3300` |
| Guide line | `#888888` |
| Rubber band | `#0066CC` (20 % fill) |
| Handle fill / border / smooth inner | `#FFFFFF` / `#888800` / `#009999` |
| Handle guide | `#AAAAAA` |
| Tooltip fill / border / text | `#FFFDE7` / `#888866` / `#333322` |

**Colour-picker swatches** (8 per row):

1. Neutrals: `FFFFFF F0F0F0 D0D0D0 B0B0B0 808080 505050 282828 000000`
2. Blues: `EEF6FF CCE0FF 9DBFFF 4A7FCB 2255CC 1144CC 003399 0099CC`
3. Purples: `F5F0FF E8DEFF D4C8FF B39DDB 9575CD 7A6FC8 553399 330066`
4. Warm: `00AA44 33CC66 88CC00 FFCC00 FF8800 A05000 CC3300 FF4444`
5. Diagram defaults and tints: `F8F9FA 1A1A1A 444444 888888 FFF9C4 FFE0B2 E8F5E9 FFE8E8`

### 17.3 Geometry function contracts

| Function | Contract |
|---|---|
| `normalize(v)` | Unit vector, or (0, 0) if `|v| < 1e-7`. |
| `perp(v)` | `(-v.y, v.x)`. Not normalised. |
| `pointToSegmentDist(p, a, b)` | Distance to the closest point on the segment, with the projection clamped to [0, 1]. Distance to `a` when `a == b`. |
| `rectBoundaryIntersect(c, hw, hh, ext)` | The point on the border of the axis-aligned rectangle where the ray from `c` towards `ext` leaves it. Returns `c` if `ext == c`. |
| `filledArrowhead(tip, dir, len=12, hb=5)` | Triangle `{tip, tip−d·len ± perp(d)·hb}`. |
| `productLineTip(c, hw, hh, junction, gap)` | Border point (towards the junction) moved back by `gap` towards the junction. |
| `worldToScreen` / `screenToWorld` | `s = w·z + o`, and `w = (s − o)/z` (treat `z = 0` as 1). |

### 17.4 Conformance checklist (summary)

- [ ] Every `Models/*.json` file loads and saves back unchanged.
- [ ] Every `Models/*.ant` file imports. Exporting and re-importing keeps the biochemistry.
- [ ] Render modes: Straight, Linear (sliding junction, hidden handle), Bézier (clipped, tangent arrowheads).
- [ ] Aliases: purple and dashed with the indicator on, primary's label, deleting a primary deletes its aliases, one SBML glyph per alias.
- [ ] Null nodes: created on import, can be hidden by the preference, left out of Antimony export.
- [ ] Modifiers: detected on import, synced when the rate law is edited, drawn as dashed arcs ending in a dot.
- [ ] Every model change can be undone and redone. Open and New clear the history.
- [ ] Hit-test priority and selection rules from §6.2–6.3.
- [ ] Align and distribute with junction and handle updates.
- [ ] Auto-layout: dedup springs, locked nodes stay put, no drift when run repeatedly, Deckard pass.
- [ ] SBML passes validation. Layout curves start and end on node borders.
- [ ] PNG at the chosen DPI. Vector PDF fitted to the page.
- [ ] Locale-independent number formatting in every output.
- [ ] Preferences saved, reloaded and applied at startup.

### 17.5 Reference implementation notes (Delphi)

This appendix is for anyone working on the Delphi code itself. A
re-implementation does not need it.

#### 17.5.1 Toolchain and build

| | |
|---|---|
| Language and framework | Delphi 13 (RAD Studio / BDS 37.0) with FireMonkey (FMX) |
| Drawing | Skia4Delphi. The canvas is a `TSkPaintBox`, and the view draws onto an `ISkCanvas`. Normal FMX controls do not use Skia (`GlobalUseSkia := False`). |
| Platforms | Win64 (released). macOS 64-bit is configured (`Entitlement.TemplateOSX.xml`, `info.plist.TemplateOSX.xml`) but not released routinely. |
| Version | `APP_VERSION` and `BUILD_DATE` in `Src/uAppVersion.pas` |

Build from the command line:

```
cmd /c '"C:\Program Files (x86)\Embarcadero\Studio\37.0\bin\rsvars.bat" && msbuild PathwayDesigner_FMX_Project.dproj /t:Build /p:Config=Debug /p:Platform=Win64'
```

The output goes to `Win64\Debug\`. SBML export is written in pure Delphi and does
not need libSBML.

#### 17.5.2 Source units

| Unit | Responsibility | Spec sections |
|---|---|---|
| `Src/uBioModel.pas` | Domain model, id generation, JSON read/write. Uses no FMX or Skia (only `System.UITypes` for colours). | §3, §4, §11.1 |
| `Src/uGeometry.pas` | Pure 2-D geometry. Knows nothing about the UI or the model. | §17.3 |
| `Src/uUndoManager.pas` | Undo commands and history. Knows only the model. | §9 |
| `Src/uAutoLayout.pas` | Force-directed layout and the Deckard Bézier handle pass. Knows only the model. | §10 |
| `Src/uDiagramView.pas` | View state, the Skia drawing passes, hit testing, the interaction state machine, and editing commands (align, Bézier tools, rename, alias). Draws onto a canvas it is given and owns no controls. | §5–§7, §8.4 |
| `Src/uAntimonyBridge.pas` | Converts between the Antimony model and the diagram model. Also holds the catalytic-conversion switch. | §11.2, §11.3 |
| `AntimonyLibrary/` | The Antimony lexer, statement parser, expression parser and syntax tree (`uAntimonyLexer`, `uAntimonyParser`, `uAntimonyExpressionParser`, `uExpressionNode`, `uAntimonyModelType`, `uAntimony`). The expression parser is also used for MathML export and modifier sync. | §8.3, §11.2 |
| `Src/uSBMLExport.pas` | The SBML L3V1 writer, including MathML and the Layout package. This is the code that runs. | §11.5 |
| `Src/uSBMLBridge.pas` | Hands SBML export to `uSBMLExport`. Its libSBML-based import and export code is disabled. | §11.5 |
| `Src/uRandomNetwork.pas` | Random network generator for testing | §7.7 |
| `Src/uColorPicker.pas` | The colour-picker popup, built entirely in code | §8.2 |
| `Src/ufMain.pas` / `.fmx` | Main window: menus, toolbars, inspector, scrollbars, file dialogs, PNG/PDF/print, the hover timer | §8.1, §11.6, §13 |
| `ufPreferences.pas` / `.fmx`, `uPreferencesObject.pas` | The Preferences dialog and its settings file | §12 |
| `Src/uFontHandling.pas` | macOS only: sets the default UI font to the system font at 11 pt | — |

Which unit may use which:

- `ufMain` → `uDiagramView` → {`uAutoLayout`, `uUndoManager`, `uGeometry`, `uAntimonyBridge`, `uSBMLBridge`/`uSBMLExport`} → `uBioModel`
- `uAntimonyBridge` and `uSBMLExport` also use `AntimonyLibrary/`.
- Nothing below `ufMain` creates or changes FMX controls.

#### 17.5.3 Design invariants

Any change to the code, and any re-implementation, must keep these rules. Each one
is described in full in the section given.

1. **Geometry is pure.** Geometry functions don't care whether they get world or
   screen coordinates. The caller decides (§5.1, §17.3).
2. **The model knows nothing about pixels, and the view owns no controls**
   (§14).
3. **Undo commands find model objects by id, never by direct reference**, so they
   still work after a snapshot is restored (§9.2).
4. **`ctrlPtsSet` alone says whether handles are user-placed or automatic.**
   Automatic handles are computed when drawing and never saved (§3.3, §5.6.2).
5. **Bézier legs are stored centre to centre and cut at the node border when
   drawn.** No border point is stored (§5.6.1).
6. **The three render modes exclude each other.** Setting one clears the others
   (§3.4).
7. **Aliases are visual only.** Every export skips them as species, but SBML Layout
   gives each alias its own glyph (§3.2, §11.5).
8. **Identifiers are never renamed silently**, including their letter case (§4).
9. **Style defaults are per field, using sentinels.** `hasCustomStyle = false`, or a
   0 colour, width or font size, means "use the default". This keeps older files
   looking exactly as they did (§3.6).

#### 17.5.4 Status of the Antimony parser

The parser in `AntimonyLibrary/` is a **proof of concept**. It was written to show
that an Antimony parser in Delphi is possible. A fuller Delphi Antimony parser
exists separately and is meant to replace it.

That replacement should clear up the "subset of Antimony" limitation and deviations
D-19 and D-20. It should also remove the need for the importer's case-insensitive
id workaround (`CanonicalId` in `uAntimonyBridge.pas`). The rule behind that
workaround, that ids are never renamed (invariant 8), must still hold afterwards.
