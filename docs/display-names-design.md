# Display names: design

Status: agreed with the author 2026-10-28 (D1–D6); all five steps done.

## The problem

SBML and Antimony give every species, reaction and compartment an id and,
optionally, a display name (SBML `name`; Antimony `S1 is "glucose"`). The
`antimony` and `sbml-io` crates read both, but PathwayDesigner's model keeps
only the id, so:

- a file whose ids are generated shows meaningless labels: MINERVA's maps
  label L-glutamate `species_11`, its compartments `comp_0`, its reactions
  `re12`;
- the names are lost when the file is saved again, as SBML or Antimony.

## Decisions

**D1. Keep the name.** Species, reactions and compartments each get an
optional display name. It is read from SBML (`name`) and Antimony
(`is "…"`), written back to both, and saved in the JSON file (a new
version). The diagram format writes it too (its syntax to be settled with the
author, words over punctuation). An alias shows its species' name; it has no
name of its own.

**D2. No guessing.** A file opens with ids as labels, as now. PathwayDesigner
does not try to tell generated ids from meaningful ones; the user decides.

**D3. Panels show both.** Every component's properties panel shows its id and
its display name. The name can be edited there; editing it changes neither the
id nor any rate law. One undo step.

**D4. Labels: all at once, or one at a time.**
- A diagram-wide setting, View ▸ Show Display Names, saved with the diagram.
- A per-item choice in the panel, Label: id / name / as the diagram, saved
  with the item.
- An item with no display name always shows its id.

**D5. All components.** Species, reactions (their labels, when shown) and
compartments (the name in the corner) all follow D3 and D4.

**D6. KEGG import uses display names.** At present it puts PubChem's names
into the ids. It will keep KEGG's numbers as ids and put the names (PubChem's
for compounds, KEGG's for enzymes) in the display names, with Show Display
Names turned on for the imported diagram.

## What stays the same

- Ids remain what rate laws, the diagram format's references and the
  Structure tab use. Two items may share a display name (ATP in two
  compartments); ids stay unique.
- Rendering draws whatever label D4 selects; layout and node sizes use the
  drawn label (a longer name may need Fit to Name).

## Steps

1. Model, JSON, SBML and Antimony read and write (D1), with round-trip tests.
2. Panels (D3).
3. Labels (D4, D5): the setting, the per-item choice, drawing and export.
4. Diagram format, spec and help pages.
5. KEGG import (D6).
