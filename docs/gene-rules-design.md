# Genes and gene–reaction rules — design

2026-11-09. Design; G1, G2 and the BiGG step settled with the author as proposed,
the rest as proposed. Steps 1–4 built (2026-11-09): `pd-core/src/genes.rs`,
native file version 24, fbc read and written, the Escher reader, Antimony's
`sbml` block (`genes`, `geneRule`), the Genes field in a reaction's
properties. Step 5, BiGG version 3, is next.

## Now

An Escher map gives each reaction a **gene–reaction rule** (`b3916 or b1723`:
the reaction is catalysed by the product of either gene; `and` means a complex
whose subunits are all needed) and its **genes**, each with an id (`b3916`,
E. coli's b-number) and a name (`pfkA`). Opening a map drops both, silently.

SBML says the same with the **fbc** package (version 2): a list of **gene
products** on the model (`fbc:id`, `fbc:label`, `fbc:name`) and, on each
reaction, a **gene product association**, the rule as a tree of `fbc:and`,
`fbc:or` and `fbc:geneProductRef`. BiGG's SBML models carry them so. Since
docs/sbml-full-support-design.md F10, fbc in an SBML file is kept as raw XML
and written back, but nothing in the app reads it: it can't be shown, and an
Escher map's rules have nowhere to go.

## Decisions

- **G1. Where the genes are held.** *Proposed:* as part of the model: the
  model's genes (id, label, name) and each reaction's rule, as text
  (`b3916 or b1723`). Read from Escher maps and from SBML's fbc, written as
  fbc version 2, kept in the native file (a new JSON version), and in
  Antimony's `sbml` block, since Antimony has no words for genes. The rest of
  fbc (flux bounds, objectives) stays kept as raw XML, as now. *Not
  proposed:* writing an Escher map's genes as raw fbc XML only, which needs
  less code but leaves them invisible in the app.
- **G2. Shown.** *Proposed:* a reaction's properties get a line **Genes**,
  the rule with the genes' names (`pfkA or pfkB`), each id on hover, as the
  diagram's labels choose ids or display names. Editable as text, checked
  as typed (`and`, `or`, brackets, gene ids; a new id becomes a new gene),
  one undo step.
- **G3. Deleting and renaming.** A reaction's rule goes with it. A gene no
  rule names any more stays in the model's list (a model may list genes for
  other reasons); the Structure tab is unchanged.
- **G4. SBML.** fbc version 2, with `fbc:required="false"` and, as fbc 2
  needs, `fbc:strict="false"` unless the file said otherwise. Gene product
  ids as BiGG writes them (`G_b3916`), labels the plain ids. A file's own
  fbc ids and attributes are kept as read.
- **G5. Checked.** BiGG's SBML for *E. coli* core, opened and saved: its
  gene products and associations the same, element by element
  (`sbml_compare.py` gains fbc). The Escher map opened and saved as SBML:
  every reaction's rule as in the map. The curated BioModels and the Test
  Suite still with no difference.

## BiGG's new site (separate step, after)

BiGG has moved to https://bigg.bio/, with a new interface,
`https://bigg.bio/api/v3/…`; the old one (`bigg.ucsd.edu/api/v2`), which the
annotation lookups use, still answers. Both allow requests from the browser.
Version 3 is shaped differently: a universal reaction no longer lists its
database links (EC, Rhea, KEGG) in the same answer. *Proposed:* move the
lookups to version 3, with links to entries on bigg.bio, once its way of
giving a reaction's references is worked out; until then version 2.

*Found (2026-11-09):* version 3 gives a species' ChEBI entry through a
curated reference compound (the charge form, `CHEBI:61548` for g6p) and its
other references (KEGG, MetaCyc, SEED) through annotation records, about five
chained requests where version 2 needs one; a reaction about four. Bulk
downloads exist (4.6 MB metabolites, 14 MB reactions). *Settled with the
author:* links to entries go to bigg.bio now, the lookups stay on version 2
while it answers, and move to version 3, an item at a time, in a later
version.

## Steps

1. The model, the native file, SBML (fbc read and written), Escher import;
   tests and the round-trip checks (G1, G3, G4, G5).
2. Antimony's `sbml` block.
3. The app: the Genes line, editing (G2). Shown in the browser.
4. Docs.
5. BiGG: links to bigg.bio (done 2026-11-09); the lookups on version 3 in a
   later version.
