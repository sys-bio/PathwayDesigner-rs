# The diagram language

An Antimony model says what reacts with what. The diagram language says how
the network is drawn: where each species and reaction goes, and how it looks.
It is written after the model, inside an Antimony comment:

```
model upper
  ...
end

/* diagram upper v1
  ...
end */
```

Because the drawing is a comment, any Antimony tool still reads the file as
plain Antimony and skips the drawing. PathwayDesigner reads both.

- To see the drawing of the current diagram, tick **Diagram** in the Antimony
  tab and press **Get Antimony**.
- To change the drawing by hand, edit the lines in the comment and press
  **Load Antimony**.
- **File ▸ Export Antimony…** saves the model and its drawing as a `.ant`
  file.

## A first example

```
model upper
  species $Glc, G6P, F6P, ATP, ADP
  HK:  $Glc + ATP -> G6P + ADP; k1*Glc*ATP
  PGI: G6P -> F6P; k2*G6P/(1 + ATP/Ki)
  k1 = 0.1; k2 = 0.5; Ki = 1
  Glc = 10; ATP = 3
end

/* diagram upper v1
color gold = #FFD54F
style metabolite { shape circle; size 28; fill gold; id right }
style cofactor { shape circle; size 18; fill silver; id right }

Glc   at (200, 100) style metabolite
G6P   at (200, 260) style metabolite
F6P   at (200, 420) style metabolite
ATP   at (300, 140) style cofactor
ADP   at (300, 220) style cofactor
ATP#2 at (320, 340) style cofactor

HK  at (200, 180) curved
PGI at (200, 340) { id left }
line ATP#2 to PGI kind inhibitor

text "Upper glycolysis" at (60, 30) { size 20; bold }
end */
```

Paste it into the Antimony tab and press **Load Antimony**. Reading it line by
line:

- `color gold = #FFD54F` names a color, so it can be used by name below.
- `style metabolite { … }` defines a look, called a style, once. Every species
  given `style metabolite` is then drawn that way.
- `Glc at (200, 100) style metabolite` puts the species Glc with its centre at
  (200, 100), drawn as a metabolite.
- `ATP#2` is a second drawing of ATP, so that ATP can appear in two places.
- `HK at (200, 180) curved` puts the junction of reaction HK, the point where
  its lines meet, at (200, 180), with curved lines.
- `PGI at (200, 340) { id left }` shows the reaction's name to the left of
  its junction.
- `line ATP#2 to PGI kind inhibitor` draws ATP's line to PGI as an
  inhibitor, ending in a blunt bar. ATP is a modifier of PGI because its rate
  law names ATP.
- `text "…" at (…)` adds a title.

## How a statement is built

Each line describes one item. It starts with what is being drawn, then says
where it goes, then which style it has, and last, in braces, any settings of
its own:

```
Glc at (200, 100) style metabolite { fill orange }
```

- **Positions** are `(x, y)`. The y axis points down the screen, so a larger y
  is lower on the page.
- **Settings** in braces are separated by `;`, as in `{ size 20; bold }`.
- **Comments** start with `//` and run to the end of the line.
- **Leaving things out is fine.** Anything not given takes its default, and a
  species with no position is placed by automatic layout. A diagram can be as
  short as a few `at` lines.

## Species

```
Glc   at (200, 100)
Glc   at (200, 100) locked
ATP#2 at (320, 340) style cofactor
HK    at (480, 180) style enzyme { id above }
```

- `at` is the centre of the species.
- `locked` keeps it in place when automatic layout runs.
- `ATP#2`, `ATP#3` and so on are further drawings of a species (aliases). The
  species' own name is its first drawing.

Settings a species can have:

- `shape`: `roundedRect`, `rectangle`, `ellipse`, `circle` or `hexagon`.
- `size`: width and height, such as `80 x 36`, or one number for a circle.
- `fill`: a color, or `none` to leave it empty.
- `border`: a color and a width, such as `navy 2`, or `none`.
- `borderStyle`: `solid`, `dashed` or `automatic` (dashed for second drawings).
- `id`: where the name goes: `inside` (the default), `above`, `below`,
  `left`, `right`, `outside` (whichever side is clearest of lines), or
  `offset (dx, dy)` to move it by that much from the centre.
- `idColor`, `idSize`, `idBold` and `idItalic`: how the name is written.

## Reactions

```
HK  at (200, 180) curved
PFK at (200, 420) curved smooth uses ATP#2, ADP#2
R1  at (100, 50) source (40, 50)
```

The reaction's name comes first and `at` places its junction. If `at` is left
out, the junction goes between the reaction's species.

How the lines are drawn:

- `straight` (the default): each line runs straight to the junction.
- `curved`: each line is a gentle curve. `curved smooth` also lines up the
  curves on either side of the junction, so the reaction passes through it
  without a kink.
- `linear`: a reaction with one reactant and one product drawn as one straight
  line, with the junction on it.

Other parts of a reaction statement:

- `uses ATP#2` joins the reaction to a second drawing of a species instead of
  the first.
- `source (x, y)` and `sink (x, y)` place the empty-set symbol of a reaction
  that has nothing on one side.
- `stem (dx, dy) (dx, dy)` draws a straight centre piece through the junction,
  as Escher maps do: the reactants' lines end at the first point and the
  products' lines start at the second, each given as an offset from the
  junction.

Settings a reaction can have:

- `line`: a color and a width, such as `#37474F 2.5`.
- `junction`: a color, or `hidden`.
- `id`: the reaction's name is hidden unless shown: `shown`, `above`, `below`,
  `left`, `right`, or `offset (dx, dy)` from the junction.
- `idColor`, `idSize`, `idBold`, `idItalic`, as for species.
- `emptySetSize`: the size of its empty-set symbols.

## Shaping a single line

A reactant's or product's line gets a statement of its own only when its
shape has been set by hand:

```
leg Glc -> HK curve (210, 140) (205, 165)
leg HK -> G6P via (260, 200)
```

- `leg Glc -> HK` is the line from reactant Glc to HK; `leg HK -> G6P` is the
  line from HK to product G6P.
- `curve` gives the two points that pull the curve into shape.
- `via` gives corner points the line passes through, so it can go round
  other parts of the diagram.

## Modifier lines

A species named in a reaction's rate law, but not one of its reactants or
products, is a modifier, drawn joined to the reaction by a dashed line. A
`line` statement says what kind it is and how it is drawn:

```
line ATP to PGI kind inhibitor
line AMP to PFK kind activator
line E to R5 kind catalyst
line MAPK_PP to J1 via (1160, 650) (360, 116) kind inhibitor { line red 3 solid }
```

- `kind` is `inhibitor` (a blunt bar), `activator` (an open arrowhead),
  `catalyst` (an open circle) or `modifier` (a dot, the default).
- `via` gives corner points, as for a leg.
- Settings: `line` (color, width and `dashed`, `dotted` or `solid`), and
  `symbol`, the size of the end symbol.

A modifier with no `line` statement is drawn as a plain dashed line.

## Compartments and text

```
compartment cytosol at (60, 40) size 520 x 600
compartment cell hidden
text "Upper glycolysis" at (80, 72) { size 22; bold }
```

- A compartment's `at` is the top-left corner of its box, and `size` its width
  and height. `hidden` keeps the compartment in the model but draws no box.
  Settings: `fill`, `border` and `idSize`.
- A text's `at` is its top-left corner. Settings: `size`, `font` (`sans` or
  `serif`), `bold`, `italic`, `align` (`left`, `center` or `right`), `color`,
  and `wrap` with a width to wrap the lines.

## Colors

A color can be written in three ways:

- **A basic name**: `black`, `white`, `gray`, `silver`, `red`, `maroon`,
  `orange`, `yellow`, `olive`, `lime`, `green`, `teal`, `aqua`, `blue`,
  `navy`, `purple` or `fuchsia`.
- **A name you define**, such as `color gold = #FFD54F`, then used as `gold`.
- **A hex value**, such as `#FFD54F`.

`none` means no color, for a species or compartment with no fill.

## Styles

A style is a named look, defined once and given to as many items as you like:

```
style metabolite { shape circle; size 28; fill gold; id right }
style flux reaction { line #37474F 2.5 }
style enzymeLine modifier { line blue 2 dashed }
```

- After the name comes the kind of item the style is for: `species` (the
  default, so it can be left out), `reaction`, `compartment`, `text` or
  `modifier`.
- Give an item a style with the word `style`: `Glc at (200, 100) style metabolite`.
  An item can have several, separated by commas; a later one wins where they
  differ.
- An item's own settings in braces win over its style.
- A style name with a space in it goes in quotes: `style "Escher metabolite"`.

## The whole diagram

These lines come at the top of the diagram:

```
theme escher
background white
modifierLines hidden
defaults species { fill #EEF6FF; border #4A7FCB 1.5 }
```

- `theme` gives the whole diagram a ready-made look: `classic` or `escher`.
- `background` is the color behind the diagram.
- `modifierLines hidden` hides every modifier line, without losing them.
- `defaults` sets what every item of a kind looks like where nothing else
  says. PathwayDesigner always writes the five `defaults` lines and the
  background, so the file looks the same in any program that reads it.

What decides how an item looks, from strongest to weakest: its own settings in
braces, then its styles, then the theme, then the `defaults` lines, then the
built-in defaults.

## When something is wrong

A line PathwayDesigner can't use, such as one naming a species the model
doesn't have, is skipped with a warning under the text, and the rest of the
drawing is used. Words it doesn't know are skipped too, but kept and written
back, so a file written by a newer program isn't damaged.

## More

The full description of the language is the technical report *A concise
alternative syntax for describing biochemical network diagrams alongside
Antimony models*, <https://doi.org/10.5281/zenodo.23625331>.

Back to the [contents](contents.md).
