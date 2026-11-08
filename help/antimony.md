# Antimony

Antimony is a plain-text way of writing a biochemical model: one line for
each reaction, with its rate law, and lines giving the starting values. The
**Antimony** tab in the panel on the right holds the text of a model, and
PathwayDesigner draws it.

## Learning Antimony

The Antimony language is described in its official documentation:

- [Antimony reference](https://tellurium.readthedocs.io/en/latest/antimony.html),
  in the Tellurium documentation
- [Antimony tutorial](https://github.com/sys-bio/antimony/blob/develop/doc/AntimonyTutorial.md)

This page covers only how PathwayDesigner uses it.

## Load Antimony and Get Antimony

- **Load Antimony** replaces the diagram with the model in the text. **Edit ▸
  Undo** puts the old diagram back. What Antimony can't say, such as a rate
  law's own parameters, is kept from the old diagram for whatever the text
  still has, so getting the text, changing a line and loading it changes
  only that line.
- **Get Antimony** replaces the text with the model on the canvas, written as
  Antimony. Use it after drawing or changing the network by hand.

Tick **Diagram** before Get Antimony to have the drawing written too, as a
comment after the model (see [The diagram language](diagram-language.md)),
and what Antimony can't say in a second comment, so nothing is lost. Without
it, a line under the text says what the text leaves out.
Where each item is and how it looks is then kept with the text: loading it again gives back the same picture rather than a new
layout. Other Antimony tools skip the comment and read just the model.

Tick **Layout statements** to add Antimony's own layout statements as well,
which tellurium and SBMLNetwork read.

## Mistakes in the text

If the model cannot be read, nothing on the canvas changes. Each line with a
mistake is shaded, and a message under the text says what is wrong; click the
message to go to the line.

Everything in the model is kept. Events, rules, functions, units and the like
are listed in the [Events & Rules](events-and-rules.md) tab. Annotations are
kept too: SBO terms (`S1.sboTerm = 247`), database references
(`S1 identity "…"`), notes and the model's creators and dates. Only other
models in the same file, and objectives, are skipped, with a warning under
the text.

## Files

- **File ▸ Import Antimony…** reads an Antimony file (`.ant` or `.txt`) into
  the diagram and the Antimony tab.
- **File ▸ Export Antimony…** writes the model with its drawing as a comment
  after it, and what Antimony can't say in another.
- **File ▸ Export Antimony with Layout Statements…** writes Antimony's layout
  statements too.

Back to the [contents](contents.md).
