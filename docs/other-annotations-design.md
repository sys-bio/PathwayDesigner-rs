# Other tools' annotations in the model

2026-11-03. Design; O1–O6 settled with the author, as proposed (O3: the metaid
must be kept). Revises decision A3 of docs/annotations-design.md. Built: SPEC-Rust
§8.3 (other tools' annotations) and the JSON version 22 entry describe what was
made; checked on the 1,062 curated BioModels, all of which pass libSBML's checks
after a round trip.

## Now

An SBML item's `<annotation>` can hold MIRIAM RDF (database references and
the model's history), which PathwayDesigner reads, shows and writes back, and
XML another tool wrote for its own use: COPASI's settings, CellDesigner's
record of what kind of molecule a species is and of its own diagram, and so
on. Under A3 that other XML is dropped, with one warning on opening ("another
tool's own annotations on 12 items aren't kept: saving will lose them"),
because after editing it may no longer be true.

So a user who opens a COPASI or CellDesigner file only to draw or annotate it,
saves and goes back to that tool, loses the tool's extras without need. Much
of it describes the model, not the drawing, and stays true however the diagram
is rearranged or restyled.

The reader already separates the two: `sbml_io::miriam::read_annotation`
returns the references and history it understands, and every other element of
the annotation as raw XML (`other`).

## What changes

- **Kept in the model part.** Another tool's XML in the annotation of the
  model, a compartment, species, reaction, parameter or assignment rule is
  kept with the item and written back unchanged when the model is saved as
  SBML, beside the item's references.
- **Gone with its item.** Deleting an item deletes its other annotations.
- **Not in the layout and render parts.** PathwayDesigner writes those from its
  own drawing; nothing another tool put on a glyph is carried.
- **The warning on opening** names only what is still dropped (below), or
  goes.

## Decisions

- **O1. CellDesigner's annotations.** CellDesigner keeps its own diagram (its
  species' aliases, positions and sizes) in annotations in its own namespace
  (`http://www.sbml.org/2001/ns/celldesigner`) on the model and the species.
  After the diagram is edited in PathwayDesigner that copy is wrong.
  *Proposed:* drop XML in CellDesigner's namespace, with the warning as now;
  keep every other tool's. *Settled:* JDesigner's too
  (`http://www.sys-bio.org/sbml`, its `jd:display` and `jd:listOfShadows`), for
  the same reason; neither tool is maintained.
- **O2. RDF the reader can't take apart** (an RDF element with structures the
  MIRIAM reader doesn't read). Writing it back beside PathwayDesigner's own
  `rdf:RDF` would give the item two RDF blocks. *Proposed:* drop it, with a
  warning, as now.
- **O3. The item's `metaid`.** Another tool's XML may point at the item by its
  `metaid`. *Proposed:* keep the `metaid` as read and write it back, rather
  than making one from the id as now, for any item with other annotations
  kept.
- **O4. Renamed items.** Another tool's XML may use the item's old id.
  *Proposed:* keep it anyway; PathwayDesigner can't check XML it doesn't
  understand.
- **O5. Native files.** *Proposed:* saved as `"otherAnnotation"`, a list of the
  raw XML elements, on the item, left out when empty; version 22. On a species,
  the primary's only, as with references.
- **O6. Antimony.** It has no place for such XML. *Proposed:* not carried,
  and saving as Antimony says so once.

## Steps

1. **Core.** The model field, SBML import (kept, CellDesigner's and unreadable
   RDF dropped and reported) and export (written back, with the `metaid`),
   native files (version 22). Tests, broken on purpose; a COPASI and a
   CellDesigner file checked, with libSBML on the result.
2. **Docs.** SPEC-Rust §8.3, docs/annotations-design.md (A3 superseded), the
   paper's Limitations sentence.
