# A readable diagram format for Antimony models — draft specification

**Status: draft for discussion, version 0.10, 2026-10-28. Read and written by
PathwayDesigner (docs/diagram-format-implementation-design.md).** Version 0.7 is published as a technical report (Zenodo, DOI
10.5281/zenodo.23608982). 0.2: the file type and COMBINE archives settled (§3). 0.3: made
consistent with the app and with the minimal Render profile
(`docs/render-design.md` §11): bold and italic names added; the reaction line
pattern and the compartment name color removed; the §14 mapping corrected. 0.4:
corner points on reactant and product lines (`leg … via`, §9), as the app now has
them (SPECIFICATION.md §5.6.5). 0.5: the diagram may say whether a modifier
activates or inhibits (`kind activator`, `kind inhibitor`, §10), so a model need
not use Antimony's interaction syntax. 0.6: an item is given a style with the
word `style` (`Glc at (200, 120) style metabolite`) instead of a dot before the
style's name; the style comes last on the line, before `{ … }`. 0.7 (settled with
the author): `color`, not `colour`; 17 basic color names and color constants
defined in the file (§5.1); the id shown on a species or a reaction is set by the
same property, `id`, placed by side (`id above`) or by offset
(`id offset (dx, dy)`) (§6.2); `compartment` instead of `box` (§8); what
`straight`, `curved` and `linear` mean and how curve handles are given (§9);
a reaction's id can be placed by side too (`id below`); two extensions foreseen,
a look for each line of a reaction and a curved `linear` reaction (§15).
0.8 (settled with the author): the diagram is written inside an Antimony block
comment, `/* diagram … end */`, so the file is plain Antimony with the
extension `.ant`, which every Antimony tool reads (§3, §4); `modifierLines
hidden` hides every modifier line in the diagram (§6.4).
0.9 (settled with the author while writing the format): a flag can be switched
off, `idBold off`, `smooth off`, which a writer needs where a theme turns it on
(§5); a style's name that isn't a word is quoted, `style "Escher metabolite"`,
so the theme's styles, and the app's styles with spaces in their names, can be
named (§4, §6.1); a reaction's empty sets have a size, `emptySetSize` (§6.2);
texts are written in the order they are drawn, not reading order, since a
later one covers an earlier (§8, §12); numbers are written to three decimal
places, so a position from automatic layout reads `81.031` (§12); the defaults
are the format's, not any one tool's, and a file can state them, `defaults
species { … }`, which a writer always does, so it looks the same in every reader
(§6.5).
0.10 (settled with the author): labels can show display names, the SBML term
for an item's name, which the model gives in Antimony as `S1 is "glucose"`:
`labels displayName` for the whole diagram (§6.4), `label id` or
`label displayName` for one item (§6.2).
Working name: *Antimony diagram*.

## Contents

1. Purpose and scope
2. Design principles
3. File structure
4. Lexical rules
5. Values
6. Styles
7. Species drawings
8. Compartments and free text
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
   biology: reactants, products, rate laws and reversibility all live in the
   model. The diagram says where things are and how they look, and, for a
   modifier line, which kind of line it is (activator, inhibitor, catalyst)
   when the model doesn't say (§10). Remove the diagram section and the model
   is unchanged.
2. **Looks have names.** Repeated looks (metabolite, cofactor, enzyme, flux) are
   named styles, defined once and given to items by name
   (`style metabolite`). An item's own settings override its style only
   where they differ. A theme is a set of definitions for the same names.
3. **Only what differs is written.** Defaults, automatic curve handles,
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
8. **Written for biologists.** The people who read and write these files are
   modelers, not programmers. Where a word will do, the format uses a word
   rather than punctuation that has to be learnt (`style metabolite`, not a
   symbol before the name), but it adds as few words as it can, since each new
   word is one more to remember.

## 3. File structure

```
<Antimony model>             // model … end, as today
/* diagram <model name> v1   // the drawing of that model
  <statements>
end */
```

- The `diagram` section follows the model it draws and names it. A file holds
  one model and at most one diagram (§15).
- `v1` is the format version. A reader that meets a newer version reads what it
  understands and warns (§13).
- **Inside a comment, in a `.ant` file** (decided, 0.8). The diagram section is
  written inside an Antimony block comment, from `/* diagram` to `end */`. To
  every Antimony tool the file is then plain Antimony, with the usual extension
  `.ant`: libAntimony 3.2.1, for example, reads the model and skips the
  drawing. A reader of this format looks for the block comment whose first word
  is `diagram`; other comments are ignored. If the model also has Antimony's
  own layout statements (a writer may add them for tools that draw from them),
  the diagram, which says more, is used and they are not. A tool that rewrites the Antimony
  without knowing the format drops comments, and so the drawing; reading is
  safe everywhere, rewriting only in tools that know the format.
- **One file is the normal form.** The model and its drawing travel together.
- **Split, in a COMBINE archive.** The two parts may also be stored as two files:
  the model as plain Antimony (`.ant`) and the diagram section alone, still
  inside its comment, in a second `.ant` file. In a COMBINE
  archive (OMEX), the manifest lists both, and the diagram file names the model
  it draws by that model's name. A reader given a diagram file alone looks for
  the model in the same archive. Joining the two gives the one-file form, and
  splitting it gives the two files; nothing is lost either way.

## 4. Lexical rules

- As Antimony: `//` comments, double-quoted strings, numbers as in Antimony
  (`1`, `0.5`, `1e-3`, `-20`), identifiers as Antimony ids.
- **Inside the comment.** Since the whole section is one block comment, and
  block comments don't nest, the section can't hold `/* … */` comments, and no
  string or comment in it may contain `*/`, which would end the section early.
- **One statement per line.** A statement may continue onto further lines only
  inside `{ … }` or after a trailing comma. A `;` may end a statement but is
  never needed.
- **Words are not reserved.** `at`, `size`, `style`, `via` and the other words
  of this format are recognised by position, so a species may be called `at`
  (as in Antimony, which reserves no words).
- A **drawing reference** is an id, optionally followed by `#` and a number from
  2: `ATP`, `ATP#2`. There is no space around `#`.
- A **style** is given to an item by the word `style` and the style's name,
  last on the line before any `{ … }`: `style cofactor`. An item may have
  several, separated by commas: `style cofactor, highlight` (later ones win,
  §6.3). Because the name follows `style`, a style may have any name, even
  one of the format's own words (`style curved`). A name that isn't a word,
  one with a space in it say, is written in quotes: `style "Escher metabolite"`.

## 5. Values

| Value | Written | Notes |
|---|---|---|
| Number | `24`, `1.5`, `-20` | |
| Point | `(x, y)` | World units, y pointing **down**, as on screen. |
| Offset | `(dx, dy)` | From an item's centre or a reaction's junction. |
| Size | `w x h`, or one number for a circle | `80 x 36`, `24` |
| Color | a basic name, a constant, `#RGB`, `#RRGGBB`, `#RRGGBBAA`, `none` | §5.1. `none` = transparent (no fill). |
| Border | `<color> <width>` | `#8D6E00 1.5`; `none` for no border. |
| Line | `<color> <width> [dashed\|dotted\|solid]` | The pattern only for modifier lines (§6.2). |
| Flag | the word alone, which switches it on; `off` after it switches it off | `bold`, `idBold off`, `locked`. A writer uses `off` only where the theme, or the item's style, has the flag on. |

Coordinates are absolute. Relative placement ("`G6P below Glc by 120`") is
tempting for hand-writing but makes round trips fragile; it is an open question
for input only (§15).

### 5.1 Colors

A color is written in one of three ways.

- **A basic name**, one of 17 every reader knows. They are the 16 colors HTML
  and SVG have always had, and `orange`; libAntimony's color table gives them
  the same values.

  | Name | Value | Name | Value | Name | Value |
  |---|---|---|---|---|---|
  | `black` | `#000000` | `maroon` | `#800000` | `teal` | `#008080` |
  | `white` | `#FFFFFF` | `orange` | `#FFA500` | `aqua` | `#00FFFF` |
  | `gray` | `#808080` | `yellow` | `#FFFF00` | `blue` | `#0000FF` |
  | `silver` | `#C0C0C0` | `olive` | `#808000` | `navy` | `#000080` |
  | `red` | `#FF0000` | `lime` | `#00FF00` | `purple` | `#800080` |
  | | | `green` | `#008000` | `fuchsia` | `#FF00FF` |

- **A constant** the file defines, `color <name> = <color>`, written once and
  used anywhere in the diagram:

  ```
  color metab      = #FFD54F
  color enzymeBlue = #1565C0
  style metabolite { fill metab; border enzymeBlue 1.5 }
  ```

  The definition travels with the file, so a constant means the same to every
  reader. A constant may reuse a basic name (`color blue = #1565C0`); in that
  file, `blue` is then that shade. A constant's value is a hex value, a basic
  name or `none`, not another constant. A name defined twice: the first
  definition counts, with a warning (§13).
- **A hex value**, `#RRGGBB` (or `#RGB`, or `#RRGGBBAA` with transparency), for
  any other shade.

## 6. Styles

### 6.1 Defining a style

```
style metabolite { shape circle; size 24; fill #FFD54F; border #8D6E00 1.5; id right }
style flux reaction { line #37474F 2.5 }
```

`style <name> [<kind>] { <property> <value>; … }`, the name quoted if it isn't
a word (`style "My enzymes" { … }`). The kind is `species`
(the default), `reaction`, `compartment`, `text` or `modifier`. A style
gives only the properties it names.

### 6.2 Properties

**Species**

| Property | Values | Default |
|---|---|---|
| `shape` | `roundedRect`, `rectangle`, `ellipse`, `circle`, `hexagon` | `roundedRect` |
| `size` | `w x h` or a diameter | `80 x 36` |
| `fill` | color, `none` | `#EEF6FF` |
| `border` | border, `none` | `#4A7FCB 1.5` |
| `borderStyle` | `automatic`, `solid`, `dashed` | `automatic` (dashed for drawings `#2` …) |
| `id` | `inside`, `above`, `below`, `left`, `right`, `outside`, `offset (dx, dy)` | `inside` |
| `idColor` | color | `#1A1A1A` |
| `idSize` | number | 12 |
| `idBold`, `idItalic` | flags | off |
| `label` | `id`, `displayName` | as the diagram (§6.4) |

**What a label shows.** An item's label is its id, or its display name if the
diagram says `labels displayName` (§6.4). `label id` or `label displayName`
sets it for one item, whatever the diagram says; an item with no display name
always shows its id. Each drawing of a species has its own choice, so `ATP#2`
can show the name while `ATP` shows the id. The display names themselves are
the model's: the Antimony section gives them (`S1 is "glucose"`), not the
diagram. The `id`, `idColor`, `idSize`, `idBold` and `idItalic` properties
place and draw the label, whichever it shows.

**Where a species' id goes.** It is drawn inside the node by default. Outside,
there are three ways to place it, from simplest to finest:

- **A side:** `id above`, `id below`, `id left` or `id right` puts it on that
  side of the node, centred on it, with its box 8 from the node's box (the gap
  the app leaves, SPECIFICATION.md §5.11).
- **An offset:** `id offset (dx, dy)` puts its centre at that offset from the
  centre of the node, in world units with y pointing down: `(0, -28)` centres it
  28 above the node's centre, `(52, 0)` 52 to the right. This places it
  anywhere, a corner included.
- **Left to the reader:** `id outside` alone puts it on the side clearest of the
  node's lines, by the app's rule (SPECIFICATION.md §5.11): of eight directions
  (right, left, above, below, then the four corners, in that order of
  preference), the one furthest from every line leaving the node towards a
  reaction, 8 from the node's box; a node with no lines gets it on the right.
  That side can change as the diagram is edited, so this form is for files
  written by hand and for styles, where each species then gets its own clear
  side.

In each case the id moves with its node. A tool writing the file writes the side
when the id is exactly there, and the offset otherwise (§12).

**Reactions**

| Property | Values | Default |
|---|---|---|
| `line` | color and width | `#444444 1.5` |
| `junction` | color, `hidden`, `shown` | `#FF8800` |
| `emptySetSize` | number: the size of its empty sets (sources and sinks) | 28 |
| `id` | `hidden`, `shown`, `above`, `below`, `left`, `right`, `offset (dx, dy)` | `hidden` |
| `idColor`, `idSize` | color, number | `#1A1A1A`, 12 |
| `idBold`, `idItalic` | flags | off |
| `label` | `id`, `displayName` | as the diagram (§6.4) |

A reaction's id is hidden by default. `id shown` shows it beside the junction,
placed clear of the reaction's lines (SPECIFICATION.md §5.12); `id above`,
`id below`, `id left` or `id right` shows it on that side of the junction,
centred on it, with its box 8 from the junction; `id offset (dx, dy)` shows it
with its centre at that offset from the junction.

**Compartments**

| Property | Values | Default |
|---|---|---|
| `fill` | color, `none` | `#F0F6EA` |
| `border` | border, `none` | `#7FA36B 2` |
| `idSize` | number | 12 |
| `label` | `id`, `displayName` | as the diagram (§6.4) |

The compartment's id is drawn in its border color when the border color is set,
otherwise in `#3D5C2B`; it has no color of its own.

**Text**

| Property | Values | Default |
|---|---|---|
| `size` | number | 14 |
| `font` | `sans`, `serif` | `sans` |
| `bold`, `italic` | flags | off |
| `align` | `left`, `center`, `right` | `left` |
| `color` | color | `#1A1A1A` |
| `wrap` | width | no wrapping |

**Modifier lines** (§10)

| Property | Values | Default |
|---|---|---|
| `line` | line | `#007777 1.5 dashed` |
| `symbol` | size | the kind's (dot 8, circle 10, arrow 12, bar 16) |
| `distance` | number | the kind's (6, 8, 8, 8) |

These defaults are the format's own: a reader draws a property a file doesn't
give with the value in these tables, whatever its own defaults are. (They are
also PathwayDesigner's, SPECIFICATION.md §5, §8.) A file can state them itself
(§6.5). A property not listed is an error in a style, but kept (§13).

**How each look is drawn.** The format names looks; it doesn't draw them. What
a reader must draw for each is defined in SPECIFICATION.md, so that two
readers draw a file the same way: species and their shapes (§5.3, §5.10),
null nodes (§5.4), reaction lines, arrowheads and the gaps at their ends
(§5.5, §5.6), modifier lines and their symbols, sizes and distances (§5.7),
junctions (§5.8), ids outside a node (§5.11), reaction ids (§5.12), stems
(§5.13), compartment boxes (§7.9) and text (§7.8). Written as SBML, the same
looks become Render as `docs/render-design.md` §3 describes.

### 6.3 Precedence

For each property, the first of these that gives it wins:

1. the item's own `{ … }` settings;
2. its styles, the last one first;
3. the diagram's `theme` (§6.4);
4. the file's `defaults` (§6.5);
5. the format's defaults, in the tables of §6.2.

This is the app's current behaviour, written down: a saved style is a style
given by name, a theme is §6.4, and an item's own look is its `{ … }`.

### 6.4 Themes and the background

```
theme escher
background white
modifierLines hidden
labels displayName
```

`theme` names a built-in set of style definitions (`classic`, `escher`); a
style the file defines with the same name replaces the theme's. Only the first
`theme` statement counts.
`modifierLines hidden` draws no modifier lines anywhere in the diagram (they stay
in the file, with their routes and looks, and show again without it); the
default is `modifierLines shown`.
`labels displayName` labels every species, reaction and compartment that has a
display name with that name instead of its id; the default is `labels id`. An
item's own `label` overrides it (§6.2).

### 6.5 Defaults

```
defaults species { shape roundedRect; size 80 x 36; fill #EEF6FF; border #4A7FCB 1.5; idColor #1A1A1A; idSize 12 }
defaults reaction { line #444444 1.5; junction #FF8800; idColor #1A1A1A; idSize 12 }
defaults compartment { fill #F0F6EA; border #7FA36B 2; idSize 12 }
defaults text { size 14; font sans; color #1A1A1A }
defaults modifier { line #007777 1.5 dashed }
```

`defaults <kind> { <property> <value>; … }` gives, for every item of that kind,
what a property is where nothing else gives it: the item's own settings, its
styles and the theme all come first (§6.3). It takes the properties of a style
of that kind (§6.2). A file need not say anything here; a reader then uses the
format's defaults of §6.2.

Its purpose is that a file says how it looks without relying on any reader's
own defaults: a writer always writes the five statements above, with the
background, at the top of the diagram, so a file that gives only positions is
drawn the same everywhere. They are the format's defaults, so reading them back
changes nothing. A file may also set its own, which then reach every item of the
kind: `defaults species { fill gold }` makes every species gold that doesn't
say otherwise, and a style's look starts from the file's defaults, so
`style big { size 60 }` under it is gold and big.

## 7. Species drawings

```
Glc   at (200, 120) style metabolite
ATP   at (320, 160) style cofactor
ATP#2 at (320, 400) style cofactor          // a second drawing of ATP
HK    at (480, 180) style enzyme { id above }
ALD   at (480, 540) style enzyme { id offset (36, -24) }   // up and to the right
X     style metabolite                     // no position: laid out automatically
Y     at (0, 0) locked
```

`<drawing> [at <point>] [locked] [style <name>, …] [{ <overrides> }]`.

- `at` is the centre of the node.
- The first drawing of a species is the species' own id; further drawings are
  `#2`, `#3`, … The numbers need not be consecutive; a writer renumbers them from
  2 in order (§12).
- A species of the model with no line here is drawn once, laid out
  automatically. A drawing of an id the model doesn't have is an error (§13).
- **The id** is drawn inside the node unless the `id` property places it
  outside: by side (`id above`), by offset (`id offset (dx, dy)`, from the node's
  centre to the id's), or on the clearest side (`id outside`) (§6.2).
- **Null nodes** (sources and sinks) are drawn for reactions with an empty side
  and placed by the reaction's statement (§9): `source (x, y)`, `sink (x, y)`.

## 8. Compartments and free text

```
compartment cytosol at (60, 40) size 520 x 600
compartment cell hidden
text "Upper glycolysis" at (80, 72) { size 22; bold }
text "After Kholodenko (2000)." at (60, 74) { size 13; italic; color #546E7A; wrap 560 }
```

- `compartment <compartment> at <point> size <size> [hidden] [style <name>, …]
  [{ … }]`: `at` is the box's top-left corner. `hidden` keeps the compartment in
  the model but draws no box. The word is the same as Antimony's, which is no
  clash: the diagram is a separate section.
- `text <string> at <point> [style <name>, …] [{ … }]`: `at` is the top-left corner of
  the text. A text has no id in the file; texts are drawn in the order the file
  gives them, a later one over an earlier one.

## 9. Reactions

```
hexokinase at (200, 180) curved style flux { id offset (-60, 0) }
pfk        at (200, 420) curved smooth uses ATP#2, ADP#2 style flux
J1         at (360, 158) curved style phosphorylation { id offset (-20, -29) }
CS         at (520, 260) stem (-40, 0) (40, 0)
src_R1     at (100, 0) source (40, 0)
```

`<reaction> [at <point>] [<shape>] [stem <offset> <offset> [free]] [uses …]
[source <point>] [sink <point>] [style <name>, …] [{ … }]`.

- `at` is the junction, the point where the reaction's lines meet.
- **Shape**, how the reaction's lines are drawn:
  - `straight` (the default): each line runs straight from its species to the
    junction.
  - `curved`: each line is a Bézier curve, which bends gently and meets the
    junction smoothly. `curved smooth` also keeps the curves on either side of
    the junction in line with each other, so the reaction passes through it
    without a kink; `smooth off` undoes it where the theme makes reactions
    smooth (Escher does).
  - `linear`: a reaction with one reactant and one product drawn as a single
    straight line from one to the other, the junction on it.
- **Curve handles.** Each curved line has two handles, the points that pull the
  curve into shape. By default they are automatic (SPECIFICATION.md §5.6.2):
  each sits 35 % of the way along the line from its end, and lines that leave
  the same side of a junction are fanned apart so they don't lie on top of each
  other. Nothing is written for automatic handles. A line whose handles were set
  by hand gets a `leg` statement (below).
- **Stem** (SPECIFICATION.md §5.13): where the reactants' lines end and the
  products' lines start, from the junction. A stem through the junction is
  kept straight unless `free`.
- **`uses`** lists drawings other than a species' first: here pfk's ATP line goes
  to `ATP#2`. A species not listed is drawn to its first drawing.
- **The id** is hidden unless the `id` property shows it (§6.2): `id shown`
  beside the junction clear of its lines, `id below` (or `above`, `left`,
  `right`) on that side of it, `id offset (dx, dy)` at that offset from it.
- **Legs with set handles or corner points** get a line of their own:

  ```
  leg Glc -> hexokinase curve (210, 140) (205, 165)   // first near Glc, second near the junction
  leg hexokinase -> G6P curve (195, 200) (200, 220)   // first near the junction, second near G6P
  leg ATP -> hexokinase via (100, 200) (100, 100)
  ```

  `leg <drawing> -> <reaction>` for a reactant, `leg <reaction> -> <drawing>`
  for a product. `curve` gives the two handles as absolute points, in the order
  the leg is written: from the species to the junction for a reactant, from the
  junction to the species for a product (SPECIFICATION.md §5.6.1). `via` gives
  the line's corner points, in order from the species whichever way the line
  runs (SPECIFICATION.md §5.6.5); the curved piece is then the one at the
  species. A leg may have either or both. Legs left out have automatic handles
  and no corner points.

## 10. Modifier lines

```
line MAPK_PP to J1 via (1160, 650) (1160, 112) (360, 116) kind inhibitor { line red 3 solid; symbol 30 }
line MKKK_P to J3 kind activator
line E#2 to R5 kind catalyst
```

`line <drawing> to <reaction> [via <point> …] [kind <kind>] [style <name>, …] [{ … }]`.

- **The kind** is `modifier` (the default), `activator`, `inhibitor` or
  `catalyst`. Models name a modifier only in the rate law, so the diagram is
  where its kind is said, and a writer always gives it here. (If a model being
  read states a kind itself, as some converters from SBML do, that kind is used,
  with a warning if the line disagrees, §13.)
- `via` lists the line's corner points in order from the modifier.
- The look is the modifier properties of §6.2, from a `modifier` style or the
  line's own `{ … }`.
- **A self modifier.** A line from a reactant or product of the reaction that
  its rate law uses makes that species a modifier of its own reaction as well,
  drawn as a loop (SPECIFICATION.md §8.4). Antimony has no way to say this, so
  a writer always writes a self modifier's line, even a plain one. A line from
  any other species that isn't one of the reaction's modifiers is skipped,
  with a warning (§13).
- A modifier the model has (from its rate law) but the diagram
  doesn't mention is drawn as a plain straight line from its first drawing.

## 11. What may be left out

| Left out | Means |
|---|---|
| A species' position | Laid out automatically, with everything else that has none |
| A reaction's position | Its junction between its species |
| A reaction's shape | `straight` |
| A leg | Automatic handles, no corner points |
| A modifier line | A plain straight line |
| Any property | The item's styles', the theme's or the built-in value (§6.3) |
| The whole diagram section | The model laid out automatically |

## 12. Writing the file back (canonical form)

A writer produces one canonical form, so that two tools writing the same diagram
write the same text:

1. `theme`, then `background` (always, as drawn), then `modifierLines hidden`
   if so, then `labels displayName` if so, then the five `defaults` statements
   (always).
2. Color constants, in the order they were defined.
3. Styles: species, reaction, compartment, text, modifier; each kind by name.
4. Compartments, in the model's compartment order.
5. Texts, in the order they are drawn: a later text is drawn over an earlier one.
6. Species drawings, in the model's species order, each species' drawings
   together (`ATP`, `ATP#2`, …).
7. Reactions, in the model's order; their `leg` lines just after each.
8. Modifier lines, by reaction, then by modifier.

Numbers are rounded to three decimal places, a thousandth of a unit, far below
anything drawn, and written in the shortest form that reads back as that
(`81.031`, `24`, never `81.03144262477514`), always with `.` as the decimal
point. Nothing equal to its inherited value is written. A
color is written as the file's constant for it if it has one, else as a basic
name if one matches exactly, else in hex. A species' id outside its node, or a
reaction's shown id, is written by its side (`id above`) when it is exactly
there, and by its offset otherwise; never as `id outside` or `id shown` alone. A tool that doesn't keep the file's
constants (or its comments) writes the values they stand for.

Under a theme, an item names a style only where it follows one other than the
theme gives it, and its settings are those that differ from the theme's look.
Where the diagram's own copy of one of the theme's styles differs from the
built-in one (an Escher map's, at the map's sizes; one saved with an older
version of the theme), the writer defines it under the theme's name, and it
replaces the theme's (§6.4).

## 13. Errors

The model section is read as Antimony, with its own errors. In the diagram
section, **nothing is fatal**: the model always loads, and each problem is a
warning with a line and column.

- An id the model doesn't have: the statement is skipped.
- A property not known for its kind: skipped, but **kept** and written back, so a
  newer file passes through an older tool unharmed.
- A word, point or block the reader does not understand in a statement it
  otherwise understands (such as `curve` after `linear`, or a block on a `leg`):
  that part is skipped, but kept and written back, and the rest of the statement
  is used. So a later version can extend a statement, not only add properties
  (§15, item 9).
- A value that doesn't read, including a color name that is neither basic nor
  defined: that property is skipped.
- A color constant defined twice: the first definition is used.
- A newer `v` number: read as far as understood, with one warning.
- A modifier line whose `kind` disagrees with a kind the model itself states:
  the model's kind is used.

## 14. Mapping to the app and to SBML Layout and Render

| This format | PathwayDesigner JSON | SBML Layout / Render |
|---|---|---|
| Species drawing | `species[]` (an alias for `#2` …) | `speciesGlyph` (several per species for aliases) |
| `at`, `size` | `x`, `y`, `w`, `h` | the glyph's bounding box |
| `style` definition, `style <name>` on an item | saved style, `styleName` | the `name` of the Render style of the items that follow it (one named style, `idList` of their glyphs) |
| Item's `{ … }` | the item's style | part of the item's look, written as the Render style of its glyph; an item with several styles or its own settings gets an unnamed style with its combined look |
| Color constant | not kept (its value is) | a colour definition of the render information |
| `id` outside a species, its side or offset | `labelOffset` | a text glyph of the species' id, in the box the app draws it in |
| Reaction `id shown`, `id offset` | `labelOffset` | a text glyph of the reaction's id |
| `compartment` | compartment `x`, `y`, `w`, `h` | `compartmentGlyph` |
| `text` | `texts[]` | `textGlyph` |
| Reaction `at` | `jx`, `jy` | the reaction glyph's position |
| `straight`, `curved`, `linear` | `mode` | the curves of the reaction's lines (straight segments or cubic Béziers); Linear in the notes |
| `stem` | `stem` | the reaction glyph's curve |
| `leg … curve` | participant handles | `speciesReferenceGlyph` curve (cubic Bézier) |
| `leg … via` | participant `points` | that curve's further segments (line segments) |
| `line … via` | modifier `points` | modifier's reference glyph curve (line segments) |
| `kind` | modifier `kind` | SBO term on the modifier reference |
| Modifier line look | modifier look keys | Render style on that reference glyph |
| `theme` | `theme` | not carried by Render: the profile writes what is drawn, not the look of items added later |
| `background` | `background` | the render information's background color |
| `labels displayName` | `displayNames` | not carried: a text glyph refers to its item (`originOfText`), and the reader chooses whether to show the item's name or id |
| Item's `label` | the item's `label` | not carried, as above |

The Render written is PathwayDesigner's minimal Render profile
(`docs/render-design.md`). Things SBML Layout and Render can't hold (Linear
reactions, smooth junctions, ids outside nodes, automatic border dashes, hidden
compartments, text wrapping) go in PathwayDesigner's existing SBML notes, as now.

## 15. Open questions

1. *Settled:* a `.ant` file with the diagram in a block comment, model and
   diagram in one file normally, split in a COMBINE archive (§3). Still open: the archive details (the manifest's
   format identifier for this type, and how the diagram file points at its
   model when the model's name isn't unique in the archive).
2. **The name.** *Antimony diagram* is a placeholder. (The extension is settled:
   `.ant`, §3.)
3. **Relative placement on input** (`below Glc by 120`, `right of`, aligned
   rows): convenient by hand, but must be written back as absolute points.
4. **Several diagrams of one model** (`diagram main`, `diagram overview`).
5. **Modules.** Antimony models can be built from modules; how a diagram refers
   into them (`M1.S1`).
6. **Style inheritance** (`style cofactor : metabolite { … }`): useful, but each
   level makes a file harder to read.
7. **Ids for texts**, so a tool can refer to one.
8. **A display name.** `name` is kept free for a name shown in place of the id
   (SBML's `name`), if one is added later.
9. **Foreseen extensions** (§13 lets them be added without breaking older
   readers; none is drawn by PathwayDesigner yet):
   - **A look for each line of a reaction**, as a block on its `leg`:
     `leg B -> J2 curve (279, 341) (301, 355) { line green }`.
   - **A curved `linear` reaction**: one line from reactant to product with no
     junction, bent by two handles: `J4 linear curve (368, 363) (423, 344)`.
   - **The junction's shape and size**, reaction properties beside `junction`
     (its color, or `hidden`): `junctionShape` (`circle`, the default, or
     `rectangle`) and `junctionSize`.
   - **Arrowheads**, reaction properties: `arrow` (`shown`, the default, or
     `hidden`), `arrowSize` and `arrowColor`.

   For example, `J1 at (230, 195) curved { junctionShape rectangle;
   junctionSize 12; arrow hidden }`. Like every property, these work in styles
   too.

## 16. Grammar

EBNF over tokens (§4). `NL` is a line break.

```
file        = model-section { NL } [ "/*" diagram "*/" ] ;
diagram     = "diagram" id "v" number NL { statement NL } "end" ;
statement   = theme | background | mlines | labels | defaults | colordef | style | compartment | text
            | species | reaction | leg | mline | (* empty *) ;

theme       = "theme" id ;
background  = "background" color ;
mlines      = "modifierLines" ( "shown" | "hidden" ) ;
labels      = "labels" ( "id" | "displayName" ) ;
defaults    = "defaults" kind block ;
colordef    = "color" id "=" color ;
style       = "style" name [ kind ] block ;
kind        = "species" | "reaction" | "compartment" | "text" | "modifier" ;
block       = "{" [ property { ";" property } [ ";" ] ] "}" ;
property    = id value { value } | id [ "on" | "off" ] ;   (* `bold` alone is a flag *)

compartment = "compartment" id [ "at" point "size" size ] [ "hidden" ] [ styles ] [ block ] ;
text        = "text" string "at" point [ styles ] [ block ] ;
species     = drawing [ "at" point ] [ "locked" ] [ styles ] [ block ] ;
reaction    = id [ "at" point ] { rshape } [ stem ] [ uses ] [ ends ]
              [ styles ] [ block ] ;
rshape      = "straight" | "linear" | "curved" | "smooth" [ "off" ] ;
stem        = "stem" offset offset [ "free" ] ;
uses        = "uses" drawing { "," drawing } ;
ends        = [ "source" point ] [ "sink" point ] ;
leg         = "leg" ( drawing "->" id | id "->" drawing )
              ( "via" point { point } [ "curve" point point ] | "curve" point point ) ;
mline       = "line" drawing "to" id [ "via" point { point } ]
              [ "kind" id ] [ styles ] [ block ] ;

drawing     = id [ "#" number ] ;
styles      = "style" name { "," name } ;
name        = id | string ;
point       = "(" number "," number ")" ;
offset      = point ;
size        = number [ "x" number ] ;
color       = "#" hexdigits | "none" | id ;          (* id: a basic name or a constant *)
value       = number | color | point | id | string ;
```

The `id` property's values are `inside`, `above`, `below`, `left`, `right`,
`outside`, `shown`, `hidden` or `offset` followed by a point.

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

/* diagram upper_glycolysis v1
  color metab      = #FFD54F
  color metabEdge  = #8D6E00
  color enzymeBlue = #1565C0

  style metabolite { shape circle; size 24; fill metab; border metabEdge 1.5; id right }
  style cofactor   { shape circle; size 16; fill #E0E0E0; border gray 1; id right }
  style enzyme     { fill #E3F2FD; border enzymeBlue 1.5; idColor enzymeBlue }
  style flux reaction { line #37474F 2.5 }

  compartment cytosol at (60, 40) size 520 x 600
  text "Upper glycolysis" at (80, 72) { size 22; bold }

  Glc   at (200, 120) style metabolite
  G6P   at (200, 240) style metabolite
  F6P   at (200, 360) style metabolite
  FBP   at (200, 480) style metabolite
  DHAP  at (140, 600) style metabolite
  GAP   at (260, 600) style metabolite
  ATP   at (320, 160) style cofactor
  ATP#2 at (320, 400) style cofactor
  ADP   at (320, 200) style cofactor
  ADP#2 at (320, 440) style cofactor
  HK    at (480, 180) style enzyme { id above }
  PGI   at (480, 300) style enzyme
  PFK   at (480, 420) style enzyme
  ALD   at (480, 540) style enzyme { id offset (0, 30) }

  hexokinase          at (200, 180) curved style flux { id offset (-60, 0) }
  leg Glc -> hexokinase curve (205, 140) (200, 165)
  isomerase           at (200, 300) curved style flux
  phosphofructokinase at (200, 420) curved smooth uses ATP#2, ADP#2 style flux
  aldolase            at (200, 540) curved style flux { id shown }
end */
```

About 35 lines of diagram, against 665 lines of JSON for the same drawing.

### 17.2 The MAPK feedback (the Gallery's showcase, abridged)

```
model mapk_cascade
  // … the ten reactions and their rate laws, as in the Gallery model; MAPK_PP
  // appears in J1's rate law, which is where the feedback is …
end

/* diagram mapk_cascade v1
  style inactive { fill #ECEFF1; border #607D8B 1.5; size 92 x 36 }
  style active   { fill #FFCCBC; border #D84315 2; size 92 x 36 }
  style phosphorylation reaction   { line #D84315 2.5; id shown; idColor #BF360C }
  style dephosphorylation reaction { line #607D8B 2; id shown }

  MKKK    at (220, 210) style inactive
  MKKK_P  at (500, 210) style active
  // … the other species …
  MAPK_PP at (1060, 650) style active { id below }

  J1 at (360, 158) curved style phosphorylation { id offset (-20, -29) }
  // … the other reactions …

  line MAPK_PP to J1 via (1160, 650) (1160, 112) (360, 116) kind inhibitor { line red 3 solid; symbol 30 }
end */
```
