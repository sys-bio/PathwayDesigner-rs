# Reading Reactome pathway diagrams

Design notes for opening a Reactome pathway as a PathwayDesigner diagram, at
Reactome's own positions. Written 2026-10-26 from one pathway checked by hand;
nothing is built, and this is to be taken up after KEGG
(docs/kegg-import-design.md). The decisions marked **R1** to **R5** are for
the author.

## 1. Purpose

Reactome draws human biology, signalling included, as reactions: each step
has inputs, outputs, catalysts and regulators, and every pathway has a drawn
diagram. Unlike KEGG's signalling maps (relations between proteins), these
translate directly into a PathwayDesigner model, and Reactome's data are in the
public domain, so its pathways can be opened in the browser, put in the
Gallery and used in tests.

## 2. What was checked

RAF/MAP kinase cascade, `R-HSA-5673001`, from Reactome version 97, in the three
forms Reactome publishes:

| File | Address | Size |
|---|---|---|
| Diagram | `https://reactome.org/download/current/diagram/R-HSA-5673001.json` | 345 KB |
| Graph | `https://reactome.org/download/current/diagram/R-HSA-5673001.graph.json` | 261 KB |
| SBML | `https://reactome.org/ContentService/exporter/sbml/R-HSA-5673001.xml` | 1.1 MB |

All three are sent with `Access-Control-Allow-Origin: *`, so the browser
version can fetch them.

**The diagram file** is the drawing:

- 160 nodes: 51 complexes, 38 chemicals, 22 sets, 17 proteins, 10 drugs, and
  22 boxes standing for other pathways (`EncapsulatedNode`, `ProcessNode`).
  Each has its name as drawn (`p-S446,T599,S602,S729 BRAF`), its box, and
  small attachments for modifications (a `P` for each phosphorylated residue).
- 54 reactions, each with its junction (`position`), its backbone
  (`segments`, the straight piece through the junction), a type (25
  transitions, 23 bindings, 3 dissociations, 3 omitted steps), and its
  `inputs`, `outputs`, `catalysts` (24), `activators` (3) and `inhibitors`
  (5), each naming a node and the points of its line. Stoichiometry is given
  where it isn't 1.
- Each node's `connectors` give the route of each line to it, some with
  several segments.
- 4 compartment boxes, 4 notes, 30 links between nodes, and the diagram's size
  (4,746 by 1,867).
- A molecule drawn twice is two nodes with the same `reactomeId` (6 of 127).

**The graph file** gives, for each entity, its UniProt or ChEBI identifier,
its gene names, and what a complex or set is made of; and for each reaction,
the reactions before it. It isn't needed for the drawing.

**The SBML export** has an SBML Layout, but its `<layout>` has neither the `id`
nor the `dimensions` the Layout specification requires, so PathwayDesigner's
SBML reader refuses the file. With those two added by hand it opens (186
species, 75 reactions, 7 compartments), but poorly: 46 species have no glyph
and go at random, lines cross the whole map, and every species shows an id
such as `species_5672665`, since Reactome puts the readable name in SBML's
`name`, which PathwayDesigner doesn't show. The SBML holds 75 reactions to the
diagram's 54: the extra ones belong to pathways the diagram shows only as
boxes.

**Licence.** Reactome's data, and files derived from them, are under CC0 (public
domain); its pathway illustrations are under CC BY 4.0. Attribution is
asked for.

## 3. What would be read (from the diagram file)

| Reactome | Becomes |
|---|---|
| A protein, complex, chemical, set or drug node | A species at the node's box, its first drawing the species and further ones aliases |
| A reaction | A reaction with its junction, inputs as reactants and outputs as products, with their stoichiometry; no rate law |
| Its backbone | The reaction's stem (S §5.13) |
| A line's route | Corner points on that leg (S §5.6.5) |
| Catalysts, activators, inhibitors | Modifiers of those kinds (S §5.7.1), with their routes as corner points |
| A compartment box | A compartment with that box |
| A box for another pathway | A free text with the pathway's name, at the box's place (**R4**) |
| A note | A free text |

Reaction types: a `binding` is an ordinary reaction (A + B → A:B), a
`dissociation` the reverse, and an `omitted` step (one standing for several) is
read as one reaction, marked dashed (**R5**).

## 4. Questions for the author

- **R1. Names.** Reactome's names are long, with spaces and punctuation
  (`activated RAF:scaffold:p-2S MAP2K:p-2T MAPK:single mechanism MAPK
  inhibitors`), and PathwayDesigner shows a species by its id, which must be a
  word. Either make ids from the names (`p_S446_T599_S602_S729_BRAF`), which is
  faithful but long; or give PathwayDesigner display names, a name drawn on the
  node apart from the id, which Antimony already has (`S1 is "glucose"`) and
  SBML too (`name`). Display names would help KEGG and SBML files as well, but
  are a feature of their own.
- **R2. How to open.** From a downloaded diagram file, by File ▸ Open; and, since
  Reactome allows it, by pathway ID (`R-HSA-5673001`) in the browser and on the
  desktop.
- **R3. The Gallery.** A few Reactome pathways in the Gallery, credited to
  Reactome, since the licence allows it.
- **R4. Boxes for other pathways:** as texts, or left out.
- **R5. Omitted steps:** as a dashed reaction, or as an ordinary one.
- **The SBML route.** Separately, PathwayDesigner's SBML reader could accept a
  layout with no id or dimensions, with a warning instead of a refusal, so
  Reactome's SBML (and that of other JSBML-written files) opens at all.

## 5. Steps, when taken up

1. Check two or three more pathways, a metabolic one among them, and how
   Reactome draws sets and drugs.
2. Decide R1 to R5.
3. The reader in pd-core, its tests on committed Reactome files, the menu, a
   help page, Gallery entries.
