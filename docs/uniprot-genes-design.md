# Looking genes up in UniProt — design

2026-11-09. Design; U2, U4 and U6 settled with the author as proposed (U6:
protein species later), the rest as proposed. Step 1 built (2026-11-09):
a gene's annotation read from and written to its fbc gene product
(`Gene::annotation`), the native file, Antimony's block; the model's organism
(`genes::organism`, `set_organism`). Step 2 built (2026-11-09): the
search in `pd-core/src/uniprot.rs` (`GeneSearch`: taxon, labels, names;
`accept`; the organism from UniProt's taxonomy or the BiGG model). Follows docs/gene-rules-design.md (genes
are now part of the model) and the species and reaction lookups
(docs/chebi-lookup-design.md, docs/reaction-lookup-design.md).

## Now

A model's genes have an id, a label and a name (`G_b3916`, `b3916`, `pfkA`)
and nothing else the app can show. Genes from BiGG's SBML bring references
(UniProt, NCBI Gene, …) in their fbc annotation, which is kept and written
back but not read; genes from an Escher map have none. Nothing looks genes up.
The paper's Limitations says proteins and genes are not looked up in UniProt.

## What UniProt gives

UniProt's REST interface (`https://rest.uniprot.org/uniprotkb/search`)
answers the browser (`Access-Control-Allow-Origin: *`) and searches by gene:

- `gene:b3916` (a locus name, as a b-number is) finds one entry: P0A796,
  *E. coli* K-12's 6-phosphofructokinase 1, gene names `pfkA b3916 JW3887`.
- `gene_exact:pfkA` alone finds 13,227 entries across organisms;
  `gene_exact:pfkA AND organism_id:83333` finds the one.
- One request asks for many genes at once
  (`(gene:b3916 OR gene:b1723 OR …) AND organism_id:83333`, up to 500 a page).

So the organism decides most of it: a locus id is nearly unique, a gene name
is unique only within an organism.

## Decisions

- **U1. What a gene keeps.** *Proposed:* a gene gets an annotation as other
  items have (references with their qualifiers, SBO term, notes): read from
  and written to its fbc gene product's own annotation, kept in the native
  file and Antimony's `sbml` block. A gene from BiGG's SBML then shows the
  references it came with.
- **U2. The organism.** *Proposed:* the model's organism, held as SBML holds
  it, a reference on the model, `bqbiol:hasTaxon` to NCBI Taxonomy
  (`taxonomy:83333`), shown in the Diagram group's Annotations. Asked for once,
  when genes are looked up and the model has none: typed as a name and found
  in UniProt's taxonomy, or suggested from the BiGG model an Escher map names
  (`e_coli_core` → *E. coli* K-12), to confirm.
- **U3. How genes are found.** *Proposed:* by label first (`gene:b3916`), then
  by name (`gene_exact:pfkA`), within the organism, reviewed (Swiss-Prot)
  entries first, many genes a request. One match: ticked; several: offered,
  unticked, with the protein names to choose from; none: marked, as species
  are.
- **U4. What is added.** *Proposed:* `bqbiol:is uniprot:P0A796` (the gene
  product is the protein), SBO:0000243 (gene) where it has no SBO term, and
  the gene's name from UniProt where it has none. Nothing else (NCBI Gene,
  KEGG genes) for now.
- **U5. Where.** *Proposed:* **Edit ▸ Annotate Genes from Names…**, with the
  results in the Annotate tab as for species and reactions, **Apply** one
  undo step. A reaction's Genes line shows each gene's UniProt entry beside
  its name (`pfkA` P0A796), a link to its page.
- **U6. Protein species.** Signalling models draw proteins as species
  (`MAPK`, `Raf`). *Proposed:* not in this step; the same UniProt search,
  with the organism, could later be offered for species ChEBI doesn't know.

## Steps

1. A gene's annotation, read and written (U1); the model's organism (U2's
   holding), with tests and the round-trip checks.
2. The UniProt lookup in pd-core (U3, U4), tested against recorded answers.
3. The app: the organism prompt, Annotate Genes from Names, the Genes line's
   entries (U2, U5). Shown in the browser.
4. Docs; the paper's Limitations.
