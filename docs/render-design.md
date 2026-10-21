# Design: a minimal SBML Render profile

Status: **draft, decisions recorded** (2026-10-20). Nothing here is built yet.

Decisions (author, 2026-10-20): accept whatever the minimum is and use it; no export
option to leave Render out; read global render information; gradients as one colour;
Render before notes (section 6); saved styles as Render style names (5.5). The
profile is to be laid out in the paper as a proposed minimum for Render (section 10).

Background: `Paper/render-survey.md` (45 files from COPASI, SBMLNetwork, SBMLDiagrams
and libSBML; 2,494 styles). It found that real files use a small part of Render:
solid colours, line widths, fonts, one rectangle or ellipse per glyph, and simple
line endings. Images, transforms and dash arrays never occur, and gradients fill only
9 styles. This design defines that part as PathwayDesigner's **Render profile**: what
it writes, what it reads, and what it does with anything outside the profile.

## 1. Goals and limits

1. **Writing is complete.** Everything PathwayDesigner draws that Render can express
   is written as standard Render (Level 3 Version 1 Render package, Version 1), so
   another tool that reads Render draws the diagram in our colours, widths, shapes
   and fonts. Every file written is valid under libSBML.
2. **Reading covers the profile, and reports the rest.** A file from COPASI,
   SBMLNetwork or SBMLDiagrams opens with its look wherever the look is one
   PathwayDesigner has. Anything else is approximated or skipped, with one warning
   that names what and how many.
3. **Our own files round-trip.** Export then import gives back every look Render
   can carry, exactly, with or without PathwayDesigner's notes.
4. **No new `pd:` notes.** The existing notes stay, are still written, and are still
   read (rule of 2026-10; see section 6 for which wins).
5. **Out of scope:** drawing arbitrary Render (several primitives per glyph, curves
   inside styles, images, transforms) as given; editing Render that PathwayDesigner
   can't represent; writing global render information for other files to use;
   Render in Level 2 files on export (we write Level 3 only).

What the paper may then say: *PathwayDesigner writes its diagrams' appearance as
SBML Render, and reads the Render features that COPASI, SBMLNetwork and SBMLDiagrams
write (Table N), reporting any it cannot show.* Not "supports Render" without the table.

## 2. The profile

| Render feature | Written | Read |
|---|---|---|
| `renderInformation` in a layout (local) | one, `id="pd_render"` | yes; the first local one |
| Global render information | no | yes, when no local one applies, or when the local one names it in `referenceRenderInformation` |
| `backgroundColor` | the diagram's background | yes |
| Colour definitions, `#RRGGBB`, `#RRGGBBAA`, `none` | yes (one definition per colour used) | yes |
| Linear and radial gradients | no | as one colour (the stop at the middle, or the first) |
| Styles by `idList` | yes, one per distinct look | yes |
| Styles by `roleList`, `typeList` | no | yes (COPASI writes only these) |
| `g` attributes: `stroke`, `stroke-width`, `fill`, `stroke-dasharray`, `font-family`, `font-size`, `font-weight`, `font-style`, `text-anchor`, `vtext-anchor`, `endHead` | yes | yes, inherited from `g` to its children |
| `startHead` | no | ignored (70 styles in the survey, all SBMLNetwork's) |
| Rectangle, with `rx`/`ry` | yes | yes |
| Ellipse | yes | yes |
| Polygon of straight points | yes (hexagon, line endings) | yes |
| Polygon with cubic Béziers, `curve` in a style | no | approximated (section 5.3) |
| Several primitives in one style | null nodes only | approximated: the largest shape |
| Line endings | yes (arrowhead and the four modifier symbols) | yes, for symbol size and distance |
| Relative coordinates (`50%`, `10 + 50%`) | yes, in shapes | yes, in shapes |
| Images, transforms, text primitives in styles | no | skipped, counted in the warning |

## 3. Writing

### 3.1 Where it goes

One `render:listOfRenderInformation` with one `render:renderInformation` inside the
layout, after the glyphs; `xmlns:render` on the `sbml` element and
`render:required="false"`. Its parts, in the order Render requires: colour
definitions, line endings, styles. No gradient definitions.

### 3.2 Colours

Each distinct colour used gets one `colorDefinition`, `id="c_RRGGBBAA"` (the hex of
the colour, so ids are stable between saves) and `value="#RRGGBBAA"`, or `#RRGGBB`
when opaque. A transparent fill is written as `none`.

### 3.3 Styles: what is written

**The look written is the look drawn.** Each item's effective look (its own style
fields, else its theme's or the app's default, S §3.6, §5.3) is written, not the
"use default" sentinels, so a reader sees what we draw. Items with the same
effective look share one style, whose `idList` lists their glyphs. A style for
items that follow a saved style (S §8.6) takes that style's name as its `name`, so
saved styles travel. Every glyph PathwayDesigner writes gets a style, so no reader
falls back on its own defaults.

| Glyph | Style |
|---|---|
| Species glyph | `stroke` border colour (none with no border), `stroke-width` border width (×2.4 for a boundary species, S §5.3), `fill`, `stroke-dasharray="5,4"` when the border is drawn dashed; one shape (below) |
| Null node's glyph | the grey circle and its × (an ellipse and two curves: the one multi-primitive style we write) |
| Species name's text glyph (`tg_<node>`) | `stroke` label colour, `font-family`, `font-size`, `font-weight`, `font-style`, `text-anchor="middle"`, `vtext-anchor="middle"` |
| Reaction glyph | `stroke` line colour, `stroke-width` line width; the junction dot as an ellipse filling the glyph's box (below), none when the junction is hidden |
| Compartment name's text glyph (new, `tg_<compartment>`) | its font size and colour (S §7.9) |
| Species reference glyph, reactant | as its reaction |
| Species reference glyph, product | as its reaction, `endHead` the arrowhead (3.4) |
| Species reference glyph, modifier | the line's look (S §5.7.2): `stroke`, `stroke-width`, `stroke-dasharray` (`6,4` dashed; `0.1w,2.5w` for dotted, as short dashes, since Render has no round caps); `endHead` the kind's symbol (3.4) |
| Reaction name's text glyph | the reaction style's font size and name colour (S §5.12) |
| Compartment glyph | `stroke` border colour, `stroke-width`, `fill`; a rectangle with the corner radius we draw |
| Free text's text glyph | `stroke` colour, font family, size, weight, style; `text-anchor` from the alignment (start, middle, end) |

**The junction dot.** The reaction glyph's bounding box, now zero-sized at the
junction, becomes the dot's box (8 × 8 about the junction, S §5.8), and its style
draws a filled ellipse in the junction colour. A hidden junction, or a Linear
reaction's, gets no ellipse. This uses only features in the profile (one ellipse
in a box, as SBMLNetwork writes for reaction glyphs). **The compartment's name**
gets a text glyph in the box the app draws it in, so other tools can show it and
its look travels; it is ordinary Layout.

Shapes, in relative coordinates so they follow the glyph's box:

| PathwayDesigner | Render primitive |
|---|---|
| Rounded rectangle | `rectangle x=0 y=0 width=100% height=100% rx=8 ry=8` |
| Rectangle | the same with `rx=ry=0` |
| Ellipse, Circle | `ellipse cx=50% cy=50% rx=50% ry=50%` (a circle's box is square) |
| Hexagon | `polygon` (0%,50%) (25%,0%) (75%,0%) (100%,50%) (75%,100%) (25%,100%) |

Text colour is written as `stroke`, as COPASI and SBMLNetwork do; to be confirmed
against COPASI's drawing in R3.

### 3.4 Line endings

| Ending | Shape | Used by |
|---|---|---|
| `pd_arrow` | filled triangle, our arrowhead's length 12 and half-base 5, tip inset 3 from the node's border (S §5.5) | products |
| `pd_dot_<size>_<gap>` | filled ellipse | modifiers (plain) |
| `pd_open_arrow_<size>_<gap>` | open triangle | activators |
| `pd_bar_<size>_<gap>` | filled thin rectangle | inhibitors |
| `pd_circle_<size>_<gap>` | open ellipse | catalysts |

A modifier's curve ends at the junction (as now), so the ending's bounding box is
placed back along the line by the symbol's distance from the junction (S §5.7.2).
One ending per distinct (kind, size, distance); the colour comes from the line's
style (the ending's `g` sets no `stroke` or `fill` of its own, except white for the
open symbols' insides, to be checked in R3: Render does not define whether an
ending inherits the curve's colour, and COPASI's endings name colours explicitly).

### 3.5 Not written

The theme (what *new* items look like,
not what is drawn); the alias indicator as a rule (the dashes it draws are written).

## 4. sbml-io: a Render model

`sbml-io` gets `render.rs`, beside `layout.rs`: a typed model of Render as written
in files, for both the Level 3 package and the Level 2 annotation form
(`http://projects.eml.org/bcb/sbml/render/level2`), read and written without
interpretation. Everything in it that `sbml-io` doesn't model is kept in `Extra`,
as now, so a file read and written by `sbml-io` still loses nothing.

**Built (R1, 2026-10-20).** `Model.global_render` (from `listOfLayouts`, or its
annotation in Level 2), `Layout.render` (from the layout, or its annotation),
and `Model.layouts_meta` for `listOfLayouts` itself. Level 2 is typed too, read
from and written back into the annotations: COPASI's command-line export writes
Level 2, and modern libSBML writes Level 2 Render with the same structure as
Level 3. The older `listOfCurveSegments` form (libSBML's 2011 samples) is read
as points and written as `listOfElements`. Results:
- the survey's 45 files: before R1, libSBML found 8 rewrites identical (the
  global render information was dropped); after, 43, and all 45 are stable.
  The other two are libSBML's 2011 samples, which declare SBML's namespace
  again on an annotation inside the layout; the rewrite doesn't, and libSBML
  keeps such declarations as written. Nothing is lost;
- the SBML Test Suite: still 9,350 of 9,350;
- the new libSBML fixtures, Level 3 and Level 2: identical, and the Level 2
  file reads into exactly the same `render` data as the Level 3 one;
- fixed on the way: an inherited namespace was declared again on kept XML
  inside a Level 2 layout, so four Level 2 files changed on every rewrite; and
  an annotation on a Level 2 `listOfLayouts` was dropped;
- 7 new tests; 12 deliberate breakages, all caught.

It models the whole structure, not only the profile (render information, colour
definitions, gradients and stops, line endings, styles with their lists, groups and
the six primitives, relative-absolute values), because a data library should read
what the file says; the *profile* lives in pd-core. Gradients, images and text
primitives are kept as typed values but never interpreted. Estimate: 900–1,200
lines with tests.

### 3.6 Built (R2, 2026-10-20)

`pd-core/src/sbml/appearance.rs`; the export registers each glyph with what it
draws, and the look is resolved by the drawing's own functions (`render::draw`),
with every optional part shown as in the default view. Decided while building:

- **Layout geometry is unchanged.** Curves end where they did, so the SBML
  Layout round trip (E4) is exactly as before (its report is identical). The
  cost, accepted as part of the minimum: a product's arrowhead is drawn with
  its tip at the node's border, 3 units from where PathwayDesigner draws it;
  a modifier's line runs on to the junction, under its symbol (open symbols are
  filled with the background, so this shows only past an inhibitor's bar).
- **Line endings name their colours** (fill and stroke on the ending's group),
  one ending per kind, size, distance and colour, since Render doesn't say
  whether an ending takes its line's colour.
- **The junction dot:** the reaction glyph's box is the dot's (8 × 8 about the
  junction) when the dot is drawn, a point otherwise; import already took the
  box's centre as the junction.
- **Compartment names** get a text glyph on the compartment's glyph, in the
  box the app draws the name in; import leaves such glyphs alone.
- **Hidden modifier lines** (the diagram's setting) are written with no stroke
  and no ending.
- **Dotted lines** are written as dashes `round(w),round(2.5w)`: Render's dash
  arrays are whole numbers and it has no round caps.
- **The background** is written as a value (`#RRGGBB`), not a colour id.
- **Saved styles:** the name goes on the style of the item's own glyph only,
  not on its name's or its lines'.

Checks:
- all 16 Gallery exports valid under libSBML (Render package enabled), only
  the usual unit warnings; every glyph has exactly one style (Millard: 792
  glyphs, 17 styles);
- SBMLNetwork 0.5.11 draws the exports in PathwayDesigner's colours, shapes,
  fonts, junction dots, arrowheads and modifier symbols (MAPK cascade, upper
  glycolysis in the Escher theme). It doesn't draw dash arrays (solid modifier
  lines), and it misplaces curves in models with compartment boxes
  (compartment_1) with or without Render: a Layout matter, to look at in R3;
- 6 new tests (`tests/render_export.rs`); 19 deliberate breakages, all caught;
  1103 workspace tests pass.

## 5. Reading (pd-core)

### 5.1 Which style applies

For each glyph, as the Render spec orders it: the local render information first,
then the global one it refers to, then the first global one. Within one, a style
whose `idList` names the glyph, else one whose `roleList` has the glyph's role
(species reference glyphs) or its SBO term, else one whose `typeList` has its type
(`SPECIESGLYPH`, `REACTIONGLYPH`, `SPECIESREFERENCEGLYPH`, `TEXTGLYPH`,
`COMPARTMENTGLYPH`, `ANY`). A text glyph with no style of its own takes the text
attributes of its graphical object's style.

### 5.2 From a style to a look

- Colours: hex, colour ids, `none` (transparent). A gradient id is the colour of its
  stop nearest 50%; counted in the warning.
- Border: `stroke`, `stroke-width`; `stroke="none"` or width 0 is no border.
  `stroke-dasharray` present: Dashed, else Solid.
- Fonts: a family containing "serif" but not "sans" is Serif, else Sans;
  `font-weight="bold"`, `font-style="italic"`; an absolute `font-size` (a relative
  one is ignored, counted).
- Text colour: the text glyph's `stroke`, else its `fill`.
- Modifier lines: `stroke`, `stroke-width`; a dash array whose dashes are no longer
  than the line's width is Dotted, any other is Dashed, none is Solid (so another
  tool's solid modifier lines come in solid, as drawn there).
- Symbol size and distance: read from the line ending only when the ending's shape
  is the kind's symbol (a dot, open triangle, bar or open circle); otherwise the
  kind's defaults. The kind itself always comes from the model (SBO terms and
  roles, S §5.7.1), never from the drawing.
- Product arrowheads: not read; we always draw ours.

### 5.3 Shapes

| Shapes in the style | Species shape | Counted as |
|---|---|---|
| One rectangle filling the box, `rx`/`ry` > 0 | Rounded rectangle | exact |
| One rectangle filling the box, square | Rectangle | exact |
| One ellipse filling the box | Ellipse (Circle if the box is square and the radii equal) | exact |
| Our hexagon (points within 1%) | Hexagon | exact |
| One rectangle or ellipse inset in the box | that shape, fitted to the box | approximated |
| Any other polygon | Rectangle | approximated |
| Several shapes, or curves | the largest shape by area, as above | approximated |
| An ellipse and curves on a glyph with no species | a null node (as now) | exact |
| Nothing (lines and text only) | unchanged | exact |

### 5.4 Defaults stay defaults

Each field read is compared with the default PathwayDesigner would draw for that
item; an equal value is stored as the default, not as a custom value. So our own
files come back with the same `VisualStyle` sentinels, and a theme applied later
still works on them. (An item set by hand to exactly the default colour comes back
as default: no difference in the drawing.)

### 5.5 Saved styles

A style with a `name` whose glyphs all get the same look becomes a saved style of
that name and kind, followed by those items; other names are ignored.

Why this is safe: no tool in the survey names its styles (0 of 2,494), and readers
ignore `name`, so the names cost nothing elsewhere. A glyph has one Render style,
so an item that follows a saved style but has a change of its own is written in a
style of its own, without the name; the app already drops an item's link to a
saved style once it no longer looks like it (S §8.6), so nothing is lost. Without
this, a diagram's saved styles survive JSON but not SBML.

### 5.6 The warning

One line after import, for example: *"Render: 3 gradients shown as one colour, 12
shapes approximated, 2 images skipped."* Nothing when everything was exact.

### 5.7 Built (R4, 2026-10-20)

`pd-core/src/sbml/render_in.rs`, run at the end of SBML import. The import
records each glyph with the item it draws; the resolver follows 5.1 (id, role,
type; local, referenced global, first global), and colours read as values,
ids, or a gradient's stop nearest the middle. Decided while building:

- **The junction dot** is shown if the reaction glyph's style draws a shape,
  hidden if it draws none (a Linear reaction is left alone). Our files say it
  both ways; another tool's file without a dot is drawn without one, as there.
- **Hidden modifier lines:** if every modifier line's style has no stroke, the
  diagram's lines are turned off; those lines' own looks then come only from
  the notes.
- **Shapes and dashes** keep the note's value when it draws the same (a circle
  is an ellipse to Render; "automatic" and "solid" dashes look the same on a
  primary); otherwise Render's.
- **The background** is the first found along the chain (SBMLNetwork gives it
  on the global render information only).

Results:
- the E4 report, standard Layout and Render only (notes removed): changes per
  model fall from 262 to 34 (Upper glycolysis), 4,653 to 301 (the Escher map),
  720 to 181 (Millard). What remains is what only the notes say (Linear
  reactions, which then lose their junction dot too; smooth junctions; solid
  against automatic dashes), the theme (§3.5), and two representation
  differences that draw the same: a width set by hand to its default value
  comes back as the default, and an alias's boundary border (the app draws an
  alias's border from the alias's own, stale, copy of the boundary flag; noted
  for the author);
- with the notes kept, the full round trip loses only the theme and those two
  representation differences;
- the survey's 45 files all import; COPASI's global role and type styles (in
  Level 2) and SBMLNetwork's styles come through;
- shown in the web build: our upper glycolysis (Escher theme) export with every
  note removed opens in its Escher look; SBMLNetwork's MAPK "sunset" opens in
  its colours (its names and junctions are R5's to improve);
- 4 integration tests (`tests/render_import.rs`) and 3 unit tests; 23
  deliberate breakages, all caught; 1110 workspace tests pass.

## 6. Render and PathwayDesigner's notes

Several looks are now in both: shapes, border dashes, compartment fill and border,
text size, colour and font, modifier line colour, width, pattern, size and distance.
Rule: **Render wins where it says something; the notes give the rest.** The notes
still carry what Render can't say: Automatic border dashes (as opposed to the dashes
drawn), names outside their node, Linear mode, smooth junctions, hidden
compartments. If another tool edits colours in Render and keeps our notes, its
colours are the ones shown. Both are written from the same model, so in our own
files they agree.

## 7. Tests and checks

1. **sbml-io (R1):** every survey file read and written back keeps everything
   libSBML sees (as E2); the Render model round-trips.
2. **Export (R2):** each Gallery model's export validates under libSBML with Render
   checks (to confirm that python-libsbml 5.20.5 has the Render package; otherwise a
   build that has); a glyph without a style is an error in a test; unit tests for
   each row of the tables in 3.3 and 3.4.
3. **External (R3):** the Escher map, MAPK and Millard exports drawn by COPASI (the
   author) and by SBMLNetwork (`draw`), compared by eye with PathwayDesigner's
   drawing; decides the open points on text colour and line-ending colours.
4. **Round trip (R4):** E4's comparison extended: with the notes removed, every
   look in section 3.3 comes back; with them, everything but section 3.5. The E6
   appearance test applies a look, exports, imports and compares.
5. **Other tools' files (R5):** each survey file opens without error; the warning
   counts match `render-survey.md`; MAPK in SBMLNetwork's 24 styles drawn by
   PathwayDesigner beside SBMLNetwork's own pictures.
6. Each step broken on purpose (wrong colour id, a style's `idList` dropped, role
   styles ignored, defaults kept as custom), each caught.

## 8. Steps

| Step | What | Done when |
|---|---|---|
| R1 | sbml-io Render model, read and write | survey corpus round-trips; tests pass |
| R2 | Export | Gallery validates; tests pass |
| R3 | Checked in COPASI and SBMLNetwork | the author's COPASI pictures; open points settled |
| R4 | Import of our own Render | E4 extended passes |
| R5 | Import of other tools' Render | survey files open as expected |
| R6 | SPECIFICATION (§11.5, new §11.5.1), README, paper table | |

One step at a time, each committed when the author says.

## 9. Open questions for the author

1. ~~Read global render information~~: yes.
2. ~~Gradients~~: one colour, with a warning.
3. ~~Dotted modifier lines as short dashes~~: accepted (the minimum is accepted).
4. ~~Precedence~~: Render first, notes for the rest.
5. ~~Saved styles as Render style names~~: yes.
6. ~~Export option to leave Render out~~: no.

## 10. In the paper: a proposed minimal Render profile

The paper lays out this profile as a proposed minimum for Render: what a writer
should produce and a reader should understand so that a diagram's look survives
between tools. Its content:

- the evidence: the survey (which features real files use, Table from
  `Paper/render-survey.md`);
- the profile: the table in section 2, as *must write / must read / may
  approximate*;
- the rules a reader needs: style resolution (5.1), the shape mapping and the
  approximations (5.2, 5.3), and reporting what was not shown (5.6);
- PathwayDesigner as one implementation, with the round-trip and external checks
  (section 7) as evidence that it works.

Wording: a *proposed* profile, not a standard (only the SBML editors can make one),
and offered to the Render package's authors. Where: an appendix beside the diagram
format, with a short Results subsection.

## 11. Consistency with the readable diagram format (docs/diagram-format.md)

Checked 2026-10-20 against the format's draft 0.2. Every look the format can write
is either in this profile or in PathwayDesigner's existing notes, with these
exceptions and fixes:

| Format (0.2) | In the profile? | Fix |
|---|---|---|
| Species shape, size, fill, border, label colour, font size | yes | none |
| `borderStyle automatic` | the dashes drawn, yes; "automatic" itself, notes only | none (as now) |
| `label outside (dx, dy)` | the place, yes (Layout text glyph); "outside", notes only | none |
| Species and reaction names in **bold** and *italic* (the app has them, S §5.11.1) | yes | **the format lacks them**: add `bold`, `italic` to species and reaction properties |
| Reaction `line … dashed/dotted` | Render could carry it | **the app has no reaction line pattern**: drop the pattern from reaction `line`, keep it for modifier lines |
| Reaction `junction` colour, `hidden` | yes, with the junction dot added above | none, once added |
| Reaction `name`, `nameSize`, `nameColour` | yes | none |
| Compartment `fill`, `border` | yes | none |
| Compartment `labelColour`, `fontSize` | yes, with the compartment name's text glyph added above | the app has no colour of its own for the name: it takes a custom border colour, else its default. Export writes the colour drawn; import ignores it. **Done (format 0.3):** the format's compartment `labelColour` removed (author, 2026-10-20: the colours used look good) |
| Text `size`, `font`, `bold`, `italic`, `align`, `colour` | yes | none |
| Text `wrap` | the box width (Layout); wrapping itself, notes only | none |
| Modifier `line`, `symbol`, `distance` | yes (dotted as short dashes) | none |
| `theme` | no: the profile writes what is drawn, not defaults for new items | **§14 of the format says "Render default values"**: change to "not carried by Render; PathwayDesigner's file only" |
| `background` | yes | none |
| Styles and classes | one named Render style per item that follows exactly one saved style | **§14 says `typeList`/`idList`**: change to `idList` with `name`; note that an item with several classes or its own overrides is written with its combined look, unnamed |
| `locked` | not appearance; neither Layout nor Render | none (JSON and the format only) |

The format was brought into line in its version 0.3 (2026-10-20): bold and italic
names added, the reaction line pattern and the compartment `labelColour` removed,
and its §14 mapping corrected for themes and styles.
