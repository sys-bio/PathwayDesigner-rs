# Keeping annotations: design

Status: agreed with the author 2026-10-28: A1 parameters included, A2–A6 and A8
as proposed; A7 (KEGG) open. Step 1 (A8) under way.

## The problem

A curated SBML model says what each item *is*, not only what it is called:

- **SBO terms** (`sboTerm="SBO:0000247"`, a simple chemical);
- **database references** (MIRIAM annotations): RDF in `<annotation>` linking an
  item to a database entry, with a qualifier saying how (`bqbiol:is` ChEBI
  15422, `bqbiol:isVersionOf` an EC number, `bqmodel:isDescribedBy` a PubMed
  paper);
- **notes**: free text, as XHTML;
- **model history**: creators, created and modified dates, in the model's
  annotation.

PathwayDesigner keeps none of these, apart from the display names (done
2026-10-28) and the SBO terms it writes itself for modifier kinds. Opening a
curated model and saving it loses them, silently. Measured on BioModels'
repressilator (BIOMD0000000012), read and written back as SBML:

| | Before | After |
|---|---|---|
| database references | 56 | 0 |
| SBO terms | 48 | 6 (the modifier kinds') |
| notes | 17 | 0 |
| `metaid`s | 82 | 0 |

Antimony is no better: its reader in PathwayDesigner skips annotations with a
warning ("Notes and annotations").

## What the crates already do

- `sbml-io` reads and writes every element's `metaid`, `sboTerm`, `<notes>` and
  `<annotation>`, the last two as raw XML (`Meta`). It doesn't take the RDF
  apart.
- `antimony` reads and writes Antimony's annotations in full: `S1 identity
  "http://identifiers.org/chebi/CHEBI:15422"` (any MIRIAM qualifier),
  `S1.sboTerm = 247`, notes, the model's creators and dates, and writes them
  to SBML as RDF.

What is missing is PathwayDesigner's own model holding them, and reading the
RDF of an SBML file into references.

## Proposal

**What is kept.** For each species, reaction, compartment and parameter, and
for the model (A1):

- its SBO term, a number (`247`);
- its database references, each a qualifier (`is`, `isVersionOf`, `hasPart`,
  …) and a resource, a URI as written (`http://identifiers.org/chebi/CHEBI:15422`,
  or the older `urn:miriam:…`);
- its notes, as text;
- for the model, also its creators and dates.

An alias shares its species' annotations, as it shares its display name.

**Reading SBML.** `sbml-io` gains a small reader for the MIRIAM RDF in an
annotation (`rdf:RDF` / `rdf:Description` / the `bqbiol:` and `bqmodel:`
qualifiers / `rdf:Bag` / `rdf:li rdf:resource`), and the model history
(`dc:creator` with `vCard`, `dcterms:created`, `dcterms:modified`). What else
an annotation holds (another tool's own XML) is decision A3.

**Writing SBML.** Each annotated item gets a `metaid` (its id, made unique
among metaids) and its references written back as RDF, in the form `antimony`
already writes. SBO terms as `sboTerm`. Notes as XHTML (`<p>` per paragraph).

**Antimony.** The Antimony importer keeps what `antimony` reads, instead of
skipping it; the exporter writes it, in a `// Annotations` section:
`ATP identity "http://identifiers.org/chebi/CHEBI:15422";`,
`ATP.sboTerm = 247;`. The diagram format needs nothing: annotations are the
model's, not the drawing's.

**PathwayDesigner's file.** A new version (18): an `annotation` object on
species, reactions, compartments and the model, left out when empty.

**In the panels.** Below the display name, an *Annotations* part, folded by
default:

- **SBO term:** the number and its name (`247 simple chemical`), from a short
  built-in list of the common terms (decision A5); editable.
- **References:** one line each, the qualifier and the entry
  (`is · CHEBI:15422`); clicking opens identifiers.org in the browser.
  Add (a qualifier list and a text field for the URI or `chebi:15422`) and
  remove. Editing is one undo step.
- **Notes:** a text box.

**Telling the user.** Until this is done, and afterwards for whatever isn't
kept (decision A3), opening a file that has annotations PathwayDesigner drops
says so, once, in the warnings.

## What stays the same

- Ids, names and the drawing are untouched; renaming an item keeps its
  annotations; deleting it drops them.
- Nothing is looked up on the network: a reference shows its identifier, not
  the database entry's name (that could come later).
- No PathwayDesigner-only annotation is added: everything written is standard
  SBML (MIRIAM RDF, `sboTerm`, `<notes>`).

## Decisions

- **A1. Scope.** Species, reactions, compartments and the model (proposed), or
  parameters too? (PathwayDesigner's parameters are plain name–value pairs; a
  parameter's SBO term, such as a Michaelis constant, is common in curated
  models.)
- **A2. Representation.** References kept as qualifier and URI (proposed), so
  they can be shown and edited; or the annotation kept as raw XML and only
  passed through (simpler, but nothing can be shown or edited).
- **A3. Other annotations.** XML in an annotation that isn't MIRIAM RDF, written
  by another tool (COPASI's, CellDesigner's): kept and written back unchanged,
  or dropped with a warning (proposed: dropped, since after editing it may no
  longer be true, as CellDesigner's layout annotation would not be).
- **A4. Notes.** Kept as the original XHTML and shown as text, written back
  unchanged unless edited (proposed); or kept as plain text only.
- **A5. SBO terms in the panel.** A short built-in list of the common terms
  with their names, and any number typed (proposed); or the whole ontology
  bundled (about 700 terms), which makes a searchable list possible.
- **A6. Which URI form is written.** As read (proposed), so a file comes back
  as it was; or always the `http://identifiers.org/…` form, which current
  BioModels uses.
- **A7. KEGG.** A KEGG map's compounds and enzymes could get references to
  their KEGG entries (`kegg.compound/C00031`) and PubChem's (from the names
  lookup). They are identifiers, not KEGG's data. Add them, or not?
- **A8. The warning first.** Add the warning about dropped annotations now, as
  a first small step, before the rest (proposed).

## Steps

1. The warning (A8).
2. The model, the file (version 18) and the SBML reader and writer, with a
   round trip of curated BioModels files: every reference, SBO term and note
   back as it was.
3. Antimony import and export, checked against libAntimony.
4. The panels: show, then edit.
5. KEGG (A7), the specification and help.
