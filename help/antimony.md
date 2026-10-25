# Antimony

Antimony is a plain-text way of writing a biochemical model: one line for
each reaction, with its rate law, and lines giving the starting values. The
**Antimony** tab in the panel on the right holds the text of a model, and
PathwayDesigner draws it.

## An example

```
compartment cell = 1
species $Glc in cell, G6P in cell, F6P in cell, ATP in cell

J1: $Glc -> G6P; k1*Glc
J2: G6P -> F6P; Vm*G6P/(1 + ATP/Ki)
J3: F6P -> ; k3*F6P

k1 = 0.5; Vm = 2; Ki = 0.1; k3 = 0.2
Glc = 10; ATP = 1
```

Paste it into the Antimony tab and press **Load Antimony**, then
**Auto-layout** to arrange it.

## What the lines mean

- `J1: $Glc -> G6P; k1*Glc` is a reaction called J1 turning Glc into G6P.
  The rate law `k1*Glc` comes after the semicolon.
- `2 A + B -> C`: two A and one B make one C.
- `J3: F6P -> ; k3*F6P` has nothing on one side, so F6P is used up. The
  empty side is drawn as a small empty-set symbol.
- `$Glc` is a boundary species: its amount is held fixed.
- `species ... in cell` puts the species in the compartment `cell`, drawn
  as a box around them.
- `k1 = 0.5` gives a parameter its value, or a species its starting amount.
- `y := 2*x` is a value worked out from others (an assignment rule).
- `// note` is a comment, ignored when the model is read.

A species that appears in a rate law but is not a reactant or product, such as
ATP in J2, is a modifier of the reaction and is drawn joined to it by a dashed
line.

## Load Antimony and Get Antimony

- **Load Antimony** replaces the diagram with the model in the text. **Edit ▸
  Undo** puts the old diagram back.
- **Get Antimony** replaces the text with the model on the canvas, written as
  Antimony. Use it after drawing or changing the network by hand.

Tick **Diagram** before Get Antimony to have the drawing written too, as a
comment after the model. Where each item is and how it looks is then kept with
the text: loading it again gives back the same picture rather than a new
layout. Other Antimony tools skip the comment and read just the model.

Tick **Layout statements** to add Antimony's own layout statements as well,
which tellurium and SBMLNetwork read.

## Mistakes in the text

If the model cannot be read, nothing on the canvas changes. Each line with a
mistake is shaded, and a message under the text says what is wrong; click the
message to go to the line.

Some things that Antimony allows cannot be shown in a diagram, such as events,
units, rate rules and notes. These are skipped and listed as warnings under the
text, and the rest of the model loads as usual. Functions are written into the
rate laws that use them.

## Files

- **File ▸ Import Antimony…** reads an Antimony file (`.ant` or `.txt`) into
  the diagram and the Antimony tab.
- **File ▸ Export Antimony…** writes the model with its drawing as a comment
  after it.
- **File ▸ Export Antimony with Layout Statements…** writes Antimony's layout
  statements too.

## Catalytic reactions

A reaction such as `E + S -> E + P` has E on both sides. With **Convert
catalytic reactions** ticked in **File ▸ Preferences…**, it is read as
`S -> P` with E as a modifier, which is how it is usually drawn.

Back to the [contents](contents.md).
