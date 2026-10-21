# A readable diagram format for Antimony models — draft specification

**Status: draft for discussion, version 0.4, 2026-10-21. Nothing here is
implemented.** 0.2: the file type and COMBINE archives settled (§3). 0.3: made
consistent with the app and with the minimal Render profile
(`docs/render-design.md` §11): bold and italic names added; the reaction line
pattern and the compartment name colour removed; the §14 mapping corrected. 0.4:
corner points on reactant and product lines (`leg … via`, §9), as the app now has
them (SPECIFICATION.md §5.6.5). Working name: *Antimony diagram* (file extension `.antd`, open
question §15).

## Contents

1. Purpose and scope
2. Design principles
3. File structure
4. Lexical rules
5. Values
6. Styles
7. Species drawings
8. Compartment boxes and free text
9. Reactions
10. Modifier lines
11. What may be left out
12. Writing the file back (canonical form)
13. Errors
14. Mapping to the app and to SBML Layout and Render
15. Open questions
16. Grammar
17. Examples

---

## 1. Purpose and scope

A plain-text file that holds a biochemical model **and** how it is drawn, which
people can read, write by hand, review in a diff and keep in version control,
and which tools can read back exactly.

- The model is **Antimony**, unchanged. Anything an Antimony tool reads, it still
  reads.
- The drawing is a separate `diagram` section that refers to the model by id:
  where each species, reaction and line is, and how it looks.
- Everything the PathwayDesigner app can draw today can be written (§14), and
  everything written maps onto SBML Layout and Render, so the format is a
  readable way into the standards rather than a rival to them.

Not in scope: simulation settings, units of display, animation, several
diagrams of one model (open question §15).

## 2. Design principles

1. **Biology once, drawing separately.** The diagram never restates the
   biology: reactants, products, rate laws, reversibility, and whether a
   modifier activates or inhibits all live in the model. The diagram only says
   where things are and how they look. Remove the diagram section and the model
   is unchanged.
2. **Looks have names.** Repeated looks (metabolite, cofactor, enzyme, flux) are
   named styles, defined once and given by class. An item's own settings
   override its class only where they differ. A theme is a set of definitions
   for the same names.
3. **Only what differs is written.** Defaults, automatic Bézier handles,
   inherited looks and positions left to automatic layout are all left out. A
   short file is a readable file.
4. **One statement per line, in a fixed order.** Moving one species changes one
   line, so a diff shows what changed.
5. **A species has one id; its drawings are numbered.** `ATP` is the species;
   `ATP#2` and `ATP#3` are further drawings of it (aliases), which exist only in
   the diagram. A rate law can never name a drawing.
6. **Lines are routes.** Every line says which drawings it joins, its path
   (handles, corner points) and its look, in that order.
7. **Readable both ways.** A person may write five lines and leave the rest to
   automatic layout; a tool writes the complete file in canonical form (§12).
   Reading the canonical form back gives the same diagram.

## 3. File structure

```
<Antimony model>          // model … end, as today
diagram <model name> v1   // the drawing of that model
  <statements>
end
```

- The `diagram` section follows the model it draws and names it. A file holds
  one model and at most one diagram (§15).
- `v1` is the format version. A reader that meets a newer version reads what it
  understands and warns (§13).
- **Its own file type** (decided). A file of this format holds the Antimony
  model followed by its diagram section, and has its own extension (§15), not
  `.ant`: Antimony tools aren't expected to read it whole. A reader takes
  everything before `diagram` as Antimony, so the model part can always be
  handed to any Antimony tool on its own.
- **One file is the normal form.** The model and its drawing travel together.
- **Split, in a COMBINE archive.** The two parts may also be stored as two files:
  the model as plain Antimony (`.ant`) and the diagram section alone in a file of
  this type, which then starts with `diagram <model name> v1`. In a COMBINE
  archive (OMEX), the manifest lists both, and the diagram file names the model
  it draws by that model's name. A reader given a diagram file alone looks for
  the model in the same archive. Joining the two gives the one-file form, and
  splitting it gives the two files; nothing is lost either way.

## 4. Lexical rules

- As Antimony: `//` and `/* … */` comments, double-quoted strings, numbers as in
  Antimony (`1`, `0.5`, `1e-3`, `-20`), identifiers as Antimony ids.
- **One statement per line.** A statement may continue onto further lines only
  inside `{ … }` or after a trailing comma. A `;` may end a statement but is
  never needed.
- **Words are not reserved.** `at`, `size`, `style`, `via` and the other words
  of this format are recognised by position, so a species may be called `at`
  (as in Antimony, which reserves no words).
- A **drawing reference** is an id, optionally followed by `#` and a number from
  2: `ATP`, `ATP#2`. There is no space around `#`.
- A **class** is a style name after a dot, with no space: `.cofactor`. An item
  may have several: `.cofactor.highlight` (later ones win, §6.3).

## 5. Values

| Value | Written | Notes |
|---|---|---|
| Number | `24`, `1.5`, `-20` | |
| Point | `(x, y)` | World units, y pointing **down**, as on screen. |
| Offset | `(dx, dy)` | From an item's centre or a reaction's junction. |
| Size | `w x h`, or one number for a circle | `80 x 36`, `24` |
| Colour | `#RGB`, `#RRGGBB`, `#RRGGBBAA`, `none` | `none` = transparent (no fill). |
| Border | `<colour> <width>` | `#8D6E00 1.5`; `none` for no border. |
| Line | `<colour> <width> [dashed\|dotted\|solid]` | The pattern only for modifier lines (§6.2). |
| Flag | the word alone | `bold`, `locked`, `straight`. |

Coordinates are absolute. Relative placement ("`G6P below Glc by 120`") is
tempting for hand-writing but makes round trips fragile; it is an open question
for input only (§15).

## 6. Styles

### 6.1 Defining a style

```
style metabolite { shape circle; size 24; fill #FFD54F; border #8D6E00 1.5; label outside }
style flux reaction { line #37474F 2.5 }
```

`style <name> [<kind>] { <property> <value>; … }`. The kind is `species`
(the default), `reaction`, `compartment`, `text` or `modifier`. A style
gives only the properties it names.

### 6.2 Properties

**Species**

| Property | Values | Default |
|---|---|---|
| `shape` | `roundedRect`, `rectangle`, `ellipse`, `circle`, `hexagon` | `roundedRect` |
| `size` | `w x h` or a diameter | `80 x 36` |
| `fill` | colour, `none` | `#EEF6FF` |
| `border` | border, `none` | `#4A7FCB 1.5` |
| `borderStyle` | `automatic`, `solid`, `dashed` | `automatic` (dashed for drawings `#2` …) |
| `label` | `inside`, `outside`, `outside (dx, dy)` | `inside` |
| `labelColour` | colour | `#1A1A1A` |
| `fontSize` | number | 12 |
| `labelBold`, `labelItalic` | flags | off |

**Reactions**

| Property | Values | Default |
|---|---|---|
| `line` | colour and width | `#444444 1.5` |
| `junction` | colour, `hidden` | `#FF8800` |
| `name` | `shown`, `shown (dx, dy)`, `hidden` | `hidden` |
| `nameSize`, `nameColour` | number, colour | 12, `#1A1A1A` |
| `nameBold`, `nameItalic` | flags | off |

**Compartments**

| Property | Values | Default |
|---|---|---|
| `fill` | colour, `none` | `#F0F6EA` |
| `border` | border, `none` | `#7FA36B 2` |
| `fontSize` | number | 12 |

The compartment's name is drawn in its border colour when the border colour is
set, otherwise in `#3D5C2B`; it has no colour of its own.

**Text**

| Property | Values | Default |
|---|---|---|
| `size` | number | 14 |
| `font` | `sans`, `serif` | `sans` |
| `bold`, `italic` | flags | off |
| `align` | `left`, `center`, `right` | `left` |
| `colour` | colour | `#1A1A1A` |
| `wrap` | width | no wrapping |

**Modifier lines** (§10)

| Property | Values | Default |
|---|---|---|
| `line` | line | `#007777 1.5 dashed` |
| `symbol` | size | the kind's (dot 8, circle 10, arrow 12, bar 16) |
| `distance` | number | the kind's (6, 5, 8, 8) |

The defaults are the app's (SPECIFICATION.md §5, §8). A property not listed is
an error in a style, but kept (§13).

**How each look is drawn.** The format names looks; it doesn't draw them. What
a reader must draw for each is defined in SPECIFICATION.md, so that two
readers draw a file the same way: species and their shapes (§5.3, §5.10),
null nodes (§5.4), reaction lines, arrowheads and the gaps at their ends
(§5.5, §5.6), modifier lines and their symbols, sizes and distances (§5.7),
junctions (§5.8), names outside a node (§5.11), reaction names (§5.12), stems
(§5.13), compartment boxes (§7.9) and text (§7.8). Written as SBML, the same
looks become Render as `docs/render-design.md` §3 describes.

### 6.3 Precedence

For each property, the first of these that gives it wins:

1. the item's own `{ … }` settings;
2. its classes, the last one first;
3. the diagram's `theme` (§6.4);
4. the built-in defaults.

This is the app's current behaviour, written down: a saved style is a class, a
theme is §6.4, and an item's own look is its `{ … }`.

### 6.4 Themes and the background

```
theme escher
background #FFFFFF
```

`theme` names a built-in set of style definitions (`classic`, `escher`); a
style the file defines with the same name replaces the theme's. Only the first
`theme` statement counts.

## 7. Species drawings

```
Glc   at (200, 120) .metabolite
ATP   at (320, 160) .cofactor
ATP#2 at (320, 400) .cofactor            // a second drawing of ATP
HK    at (480, 180) .enzyme { label outside (0, -28) }
X     .metabolite                         // no position: laid out automatically
Y     at (0, 0) locked
```

`<drawing> [at <point>] [<classes>] [locked] [{ <overrides> }]`.

- The first drawing of a species is the species' own id; further drawings are
  `#2`, `#3`, … The numbers need not be consecutive; a writer renumbers them from
  2 in order (§12).
- A species of the model with no line here is drawn once, laid out
  automatically. A drawing of an id the model doesn't have is an error (§13).
- **Null nodes** (sources and sinks) are drawn for reactions with an empty side
  and placed by the reaction's statement (§9): `source (x, y)`, `sink (x, y)`.

## 8. Compartment boxes and free text

```
box cytosol at (60, 40) size 520 x 600
box cell hidden
text "Upper glycolysis" at (80, 72) { size 22; bold }
text "After Kholodenko (2000)." at (60, 74) { size 13; italic; colour #546E7A; wrap 560 }
```

- `box <compartment> at <point> size <size> [hidden] [<classes>] [{ … }]`:
  `at` is the box's top-left corner. `hidden` keeps the compartment in the model
  but draws no box.
- `text <string> at <point> [<classes>] [{ … }]`: `at` is the top-left corner of
  the text. A text has no id in the file; a writer gives them in reading order.

## 9. Reactions

```
hexokinase at (200, 180) .flux straight
pfk        at (200, 420) .flux straight uses ATP#2, ADP#2
J1         at (360, 158) .phosphorylation curved smooth name (-20, -29)
CS         at (520, 260) stem (-40, 0) (40, 0)
src_R1     at (100, 0) source (40, 0)
```

`<reaction> [at <point>] [<shape>] [stem <offset> <offset> [free]] [uses …]
[source <point>] [sink <point>] [name [<offset>]] [<classes>] [{ … }]`.

- `at` is the junction.
- **Shape:** `straight` (the default), `linear` (one straight line, for one
  reactant and one product), or `curved` (Bézier), optionally `smooth`.
- **Stem** (SPECIFICATION.md §5.13): where the reactants' lines end and the
  products' lines start, from the junction. A stem through the junction is
  kept straight unless `free`.
- **`uses`** lists drawings other than a species' first: here pfk's ATP line goes
  to `ATP#2`. A species not listed is drawn to its first drawing.
- **`name`** shows the reaction's id, at an offset from the junction or placed
  clear of its lines when none is given.
- **Legs with moved handles or corner points** get a line of their own:

  ```
  leg Glc -> hexokinase curve (210, 140) (205, 165)
  leg hexokinase -> G6P curve (195, 200) (200, 220)
  leg ATP -> hexokinase via (100, 200) (100, 100)
  ```

  `leg <drawing> -> <reaction>` for a reactant, `leg <reaction> -> <drawing>`
  for a product, then `via` and the line's corner points, in order from the
  species whichever way the line runs (SPECIFICATION.md §5.6.5), and `curve` and
  the two handles of its end piece in the order of SPECIFICATION.md §5.6.1; one
  of them or both. Legs left out have automatic handles and no corner points.

## 10. Modifier lines

```
line MAPK_PP to J1 via (1160, 650) (1160, 112) (360, 116) { line #C80000 3 solid; symbol 30 }
line MKKK_P to J3
line E#2 to R5 kind catalyst
```

`line <drawing> to <reaction> [via <point> …] [kind <kind>] [<classes>] [{ … }]`.

- **The kind is the model's.** `X -| J1` (inhibitor) and `X -o J1` (activator)
  are Antimony and stay in the model section. Only a kind Antimony has no syntax
  for is written here: `kind catalyst`. `kind modifier` is the default.
- `via` lists the line's corner points in order from the modifier.
- The look is the modifier properties of §6.2, from a `modifier` style or the
  line's own `{ … }`.
- A modifier the model has (from its rate law or an interaction) but the diagram
  doesn't mention is drawn as a plain straight line from its first drawing.

## 11. What may be left out

| Left out | Means |
|---|---|
| A species' position | Laid out automatically, with everything else that has none |
| A reaction's position | Its junction between its species |
| A reaction's shape | `straight` |
| A leg | Automatic handles, no corner points |
| A modifier line | A plain straight line |
| Any property | The class's, theme's or built-in value (§6.3) |
| The whole diagram section | The model laid out automatically |

## 12. Writing the file back (canonical form)

A writer produces one canonical form, so that two tools writing the same diagram
write the same text:

1. `theme`, then `background`.
2. Styles: species, reaction, compartment, text, modifier; each kind by name.
3. Boxes, in the model's compartment order.
4. Texts, in reading order (top to bottom, then left to right).
5. Species drawings, in the model's species order, each species' drawings
   together (`ATP`, `ATP#2`, …).
6. Reactions, in the model's order; their `leg` lines just after each.
7. Modifier lines, by reaction, then by modifier.

Numbers are written in the shortest form that reads back the same, always with
`.` as the decimal point. Nothing equal to its inherited value is written.

## 13. Errors

The model section is read as Antimony, with its own errors. In the diagram
section, **nothing is fatal**: the model always loads, and each problem is a
warning with a line and column.

- An id the model doesn't have: the statement is skipped.
- A property not known for its kind: skipped, but **kept** and written back, so a
  newer file passes through an older tool unharmed.
- A value that doesn't read: that property is skipped.
- A newer `v` number: read as far as understood, with one warning.

## 14. Mapping to the app and to SBML Layout and Render

| This format | PathwayDesigner JSON | SBML Layout / Render |
|---|---|---|
| Species drawing | `species[]` (an alias for `#2` …) | `speciesGlyph` (several per species for aliases) |
| `at`, `size` | `x`, `y`, `w`, `h` | the glyph's bounding box |
| Style, class | saved style, `styleName` | the `name` of the Render style of the items that follow it (one named style, `idList` of their glyphs) |
| Item's `{ … }` | the item's style | part of the item's look, written as the Render style of its glyph; an item with several classes or its own settings gets an unnamed style with its combined look |
| `box` | compartment `x`, `y`, `w`, `h` | `compartmentGlyph` |
| `text` | `texts[]` | `textGlyph` |
| Reaction `at` | `jx`, `jy` | the reaction glyph's position |
| `stem` | `stem` | the reaction glyph's curve |
| `leg … curve` | participant handles | `speciesReferenceGlyph` curve (cubic Bézier) |
| `leg … via` | participant `points` | that curve's further segments (line segments) |
| `line … via` | modifier `points` | modifier's reference glyph curve (line segments) |
| `kind catalyst` | modifier `kind` | SBO term on the modifier reference |
| Modifier line look | modifier look keys | Render style on that reference glyph |
| `theme` | `theme` | not carried by Render: the profile writes what is drawn, not the look of items added later |
| `background` | `background` | the render information's background colour |

The Render written is PathwayDesigner's minimal Render profile
(`docs/render-design.md`). Things SBML Layout and Render can't hold (Linear
reactions, smooth junctions, names outside nodes, automatic border dashes, hidden
compartments, text wrapping) go in PathwayDesigner's existing SBML notes, as now.

## 15. Open questions

1. *Settled:* its own file type, model and diagram in one file normally, split
   in a COMBINE archive (§3). Still open: the archive details (the manifest's
   format identifier for this type, and how the diagram file points at its
   model when the model's name isn't unique in the archive).
2. **The name and extension.** *Antimony diagram* and `.antd` are placeholders.
3. **Relative placement on input** (`below Glc by 120`, `right of`, aligned
   rows): convenient by hand, but must be written back as absolute points.
4. **Several diagrams of one model** (`diagram main`, `diagram overview`).
5. **Modules.** Antimony models can be built from modules; how a diagram refers
   into them (`M1.S1`).
6. **Style inheritance** (`style cofactor : metabolite { … }`): useful, but each
   level makes a file harder to read.
7. **Ids for texts**, so a tool can refer to one.

## 16. Grammar

EBNF over tokens (§4). `NL` is a line break.

```
file        = model-section { NL } [ diagram ] ;
diagram     = "diagram" id "v" number NL { statement NL } "end" ;
statement   = theme | background | style | box | text | species
            | reaction | leg | mline | (* empty *) ;

theme       = "theme" id ;
background  = "background" colour ;
style       = "style" id [ kind ] block ;
kind        = "species" | "reaction" | "compartment" | "text" | "modifier" ;
block       = "{" [ property { ";" property } [ ";" ] ] "}" ;
property    = id value { value } | id ;              (* `bold` alone is a flag *)

box         = "box" id [ "at" point "size" size ] [ "hidden" ] { class } [ block ] ;
text        = "text" string "at" point { class } [ block ] ;
species     = drawing [ "at" point ] { class } [ "locked" ] [ block ] ;
reaction    = id [ "at" point ] [ rshape ] [ stem ] [ uses ] [ ends ]
              [ name ] { class } [ block ] ;
rshape      = "straight" | "linear" | "curved" [ "smooth" ] ;
stem        = "stem" offset offset [ "free" ] ;
uses        = "uses" drawing { "," drawing } ;
ends        = [ "source" point ] [ "sink" point ] ;
name        = "name" [ offset ] ;
leg         = "leg" ( drawing "->" id | id "->" drawing )
              ( "via" point { point } [ "curve" point point ] | "curve" point point ) ;
mline       = "line" drawing "to" id [ "via" point { point } ]
              [ "kind" id ] { class } [ block ] ;

drawing     = id [ "#" number ] ;
class       = "." id ;
point       = "(" number "," number ")" ;
offset      = point ;
size        = number [ "x" number ] ;
colour      = "#" hexdigits | "none" ;
value       = number | colour | point | id | string ;
```

## 17. Examples

### 17.1 Upper glycolysis (the README's first screenshot)

```
model upper_glycolysis
  compartment cytosol = 1
  species $Glc in cytosol, G6P in cytosol, F6P in cytosol, FBP in cytosol
  species DHAP in cytosol, GAP in cytosol
  species $ATP in cytosol, $ADP in cytosol
  species $HK in cytosol, $PGI in cytosol, $PFK in cytosol, $ALD in cytosol

  hexokinase: Glc + ATP -> G6P + ADP; HK*k1*Glc*ATP
  isomerase: G6P -> F6P; PGI*k2*G6P
  phosphofructokinase: F6P + ATP -> FBP + ADP; PFK*k3*F6P*ATP
  aldolase: FBP -> DHAP + GAP; ALD*k4*FBP

  Glc = 1; ATP = 1; ADP = 1
  HK = 0.1; PGI = 0.1; PFK = 0.1; ALD = 0.1
  k1 = 0.1; k2 = 0.1; k3 = 0.1; k4 = 0.1
end

diagram upper_glycolysis v1
  style metabolite { shape circle; size 24; fill #FFD54F; border #8D6E00 1.5; label outside }
  style cofactor   { shape circle; size 16; fill #E0E0E0; border #757575 1; label outside }
  style enzyme     { fill #E3F2FD; border #1565C0 1.5 }
  style flux reaction { line #37474F 2.5 }

  box cytosol at (60, 40) size 520 x 600
  text "Upper glycolysis" at (80, 72) { size 22; bold }

  Glc   at (200, 120) .metabolite
  G6P   at (200, 240) .metabolite
  F6P   at (200, 360) .metabolite
  FBP   at (200, 480) .metabolite
  DHAP  at (140, 600) .metabolite
  GAP   at (260, 600) .metabolite
  ATP   at (320, 160) .cofactor
  ATP#2 at (320, 400) .cofactor
  ADP   at (320, 200) .cofactor
  ADP#2 at (320, 440) .cofactor
  HK    at (480, 180) .enzyme
  PGI   at (480, 300) .enzyme
  PFK   at (480, 420) .enzyme
  ALD   at (480, 540) .enzyme

  hexokinase          at (200, 180) .flux
  isomerase           at (200, 300) .flux
  phosphofructokinase at (200, 420) .flux uses ATP#2, ADP#2
  aldolase            at (200, 540) .flux
end
```

About 30 lines of diagram, against 665 lines of JSON for the same drawing.

### 17.2 The MAPK feedback (the Gallery's showcase, abridged)

```
model mapk_cascade
  // … the ten reactions and their rate laws, as in the Gallery model …
  MAPK_PP -| J1        // the feedback: biology, so it is here
end

diagram mapk_cascade v1
  style inactive { fill #ECEFF1; border #607D8B 1.5; size 92 x 36 }
  style active   { fill #FFCCBC; border #D84315 2; size 92 x 36 }
  style phosphorylation reaction   { line #D84315 2.5; name shown; nameColour #BF360C }
  style dephosphorylation reaction { line #607D8B 2; name shown }

  MKKK    at (220, 210) .inactive
  MKKK_P  at (500, 210) .active
  // … the other species …
  MAPK_PP at (1060, 650) .active

  J1 at (360, 158) curved .phosphorylation name (-20, -29)
  // … the other reactions …

  line MAPK_PP to J1 via (1160, 650) (1160, 112) (360, 116) { line #C80000 3 solid; symbol 30 }
end
```
