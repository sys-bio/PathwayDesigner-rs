# Reading KEGG pathway maps

Design for opening a KEGG pathway map (a KGML file) as a PathwayDesigner
diagram, at KEGG's own positions. Written 2026-10-26; nothing is built yet.
Decisions K1 to K6 were made by the author on 2026-10-26 (§10).

## 1. Purpose

KEGG draws several hundred metabolic pathway maps, for reference pathways and
for each sequenced organism. Each map is published as a KGML file: an XML file
with the map's compounds and enzymes, where each is drawn, and the reactions
between them. Reading these gives a user a laid-out network of a pathway they
know, to edit, extend with rate laws, and save as Antimony or SBML.

This is like reading Escher maps (S §11.7), with two differences: KGML has no
line routes, so reactions are drawn straight, and KEGG's licence keeps its maps
out of the Gallery (§8).

## 2. What a KGML file holds

Five files were checked, fetched from `https://rest.kegg.jp/get/<map>/kgml`:

| Map | Kind | Compounds | Enzyme boxes | Reactions |
|---|---|---|---|---|
| eco00010 | *E. coli* glycolysis | 31 | 35 genes, 28 orthologs | 35 |
| ko00010 | reference glycolysis | 31 | 63 orthologs | 63 |
| eco00020 | *E. coli* TCA cycle | 23 (3 drawn twice) | 21 genes, 12 orthologs | 21 |
| eco01100 | *E. coli* global metabolism | 3,587 | 4,168 drawn as lines | 1,013 |
| hsa04010 | human MAPK signalling | 5 | 121 genes | 0 |

- **`entry`** elements are the things drawn. Each has a `type` (`compound`,
  `gene`, `ortholog`, `map`, `group`) and a `graphics` child with its centre
  (`x`, `y`), size, shape and colours. A compound is a circle 8 across, with
  its KEGG number as its name (`C00031`). An enzyme box is a rectangle 46 by
  17, named by its first gene (`pykF...`, the dots meaning more genes), and
  green (`#BFFFBF`) when the organism has the gene. A `map` entry is a rounded
  box linking to another pathway (`TCA cycle`).
- **`reaction`** elements give a reaction's KEGG number (`rn:R00200`),
  `reversible` or `irreversible`, and its substrates and products by the id of
  the compound entry drawn. A reaction's own id is the id of its enzyme box,
  so the box shows where the reaction is drawn.
- **One reaction can be drawn twice** (35 reactions, 32 numbers, in eco00010):
  isozymes get a box each. **One box can carry several reactions** (3 boxes in
  eco00010), each with the same compounds.
- **A compound can be drawn twice** (3 compounds in eco00020); each drawing is its own entry, and a reaction names the one it
  uses.
- **The enzyme box sits beside the reaction's line**, not on it: for R00746 the
  compounds are at (550, 958) and (378, 958) and the box at (487, 948). Two
  isozyme reactions between the same compounds have boxes at different places
  along the line (487 and 439).
- **Not in the file:** rate laws, stoichiometry (every coefficient is 1),
  compound names (only the numbers; the names are a second request to KEGG),
  and the routes of the lines, which are part of KEGG's background picture.
- **Signalling maps** (hsa04010) have no reactions: their arrows are
  `relation`s between gene products (activation, inhibition, binding). **Global
  maps** (eco01100) draw each enzyme as a polyline instead of a box, over a map
  of thousands of compounds.

## 3. What is read

For a metabolic map (one with `reaction` elements):

| KGML | Becomes |
|---|---|
| A compound entry | A species at the entry's centre. Further drawings of the same compound are aliases (S §5.2.3), as Escher's are. |
| A reaction element | A reaction, reversible or not as KEGG says, its reactants and products the compound drawings it names, each with stoichiometry 1. No rate law (as for Escher). |
| The reaction's enzyme box | See **K1**. |
| A `map` entry | A free text (S §7.8) with the linked pathway's title, at the box's place (**K4**). |
| The map's title | The model's name, and a title text at the top left. |
| `relation`, `group`, ortholog boxes with no reaction | Not read (§6). |

**K1. Enzymes.** Two ways:

- **(a) Enzymes as species (recommended).** Each enzyme box becomes a species,
  a rectangle at the box's place and size, named by its gene (`pykF`), and a
  catalyst (S §5.7.1) of each reaction the box carries, drawn with a short
  catalyst line to the reaction's junction. This looks like KEGG's map, gives
  the enzymes names a biologist recognises, and puts them in the model, where a
  rate law can use them.
- **(b) Enzymes left out.** Only compounds and reactions; each reaction is named
  by its KEGG number (`R00200`), the only name the model can show for it. Simpler,
  but the map loses what KEGG shows most.

**Junctions.** A reaction's junction goes where its enzyme box projects onto the
straight line from the middle of its substrates to the middle of its products.
So isozymes between the same compounds get separate junctions along the line,
in KEGG's order, rather than lying on top of each other, and a catalyst line
is as short as KEGG's gap between box and line.

**Ids.** Antimony needs ids that are words.

- **K2. Compounds:** a real name, looked up in PubChem (§5); the KEGG number
  (`C00031`) only if no name is found. The author: KEGG numbers mean nothing
  to a reader, so names are used wherever possible. *Changed 2026-10-28
  (docs/display-names-design.md D6):* the name was first made into an id
  (`D-Glucose` → `D_Glucose`); now the KEGG number stays the id and the name,
  as PubChem writes it, is the display name, with the map's labels showing
  display names. Enzymes likewise: KEGG's id (`b1676`), the box's label
  (`pykF`) as the display name.
- Enzymes: the gene name without the dots (`pykF`); an ortholog box in a
  reference map, its KO name (`K00873`). A clash gets `_2`.
- Reactions: the KEGG number (`R00200`); a second drawing of it, `R00200_2`.

## 4. How it looks

**K3.** KEGG's coordinates are pixels of its map picture, about 1,000 to
1,500 wide, so the diagram is used at that scale. The look follows KEGG:

- compounds: white circles with a black border, 12 across (KEGG's 8 is too
  small to click), the name beside them on the clearest side (S §5.11);
- enzymes: rectangles at KEGG's size and colour (green when the organism has
  the gene), the name inside;
- reactions: thin dark straight lines, arrowheads at the products of an
  irreversible reaction; catalyst lines as S §5.7.1 draws them;
- the linked-pathway texts in grey.

These are written as the diagram's own looks, not a new theme, so Antimony
and SBML export carry them as for any diagram.

## 5. Opening a map

- **From a file.** File ▸ Open reads a KGML file (`.xml` or `.kgml`). A `.xml`
  file whose root element is `<pathway>` is KGML; one whose root is `<sbml>` is
  SBML, as now. KEGG's website offers the file under *Download KGML*, and
  `https://rest.kegg.jp/get/eco00010/kgml` fetches it for any map code.
- **K6. File ▸ Open from KEGG…** asks for a map code (`eco00010`,
  `hsa00020`, `map00010`).
  - On the desktop it fetches the file and opens it.
  - In the browser it can't: KEGG's server doesn't let a web page read its
    files (no `Access-Control-Allow-Origin` header). This was confirmed
    2026-10-26 by fetching from the published app's own page in Chromium: the
    KGML file and KEGG's name list were both blocked, while Reactome, as a
    control, was fetched. So in the browser the menu item opens KEGG's file
    for that map in a new tab (navigation, which is allowed); the user saves
    it and opens it with File ▸ Open. The author accepted this.
- **Names (K2), on the desktop and in the browser alike,** come from PubChem,
  which allows web pages to read it, and to which KEGG deposits its compounds
  under their numbers. Two requests per batch of compounds:
  1. KEGG numbers to PubChem compounds:
     `https://pubchem.ncbi.nlm.nih.gov/rest/pug/substance/sourceid/KEGG/C00031,C00103/cids/JSON`;
  2. the compounds' titles:
     `https://pubchem.ncbi.nlm.nih.gov/rest/pug/compound/cid/5793,65533/property/Title/JSON`.

  Checked 2026-10-26: C00031 is `D-Glucose`, C00103 `alpha-D-glucose
  1-phosphate`, C00022 `Pyruvic Acid`, C00024 `acetyl CoA`. A compound
  PubChem has only as a substance, such as C15972 (a lipoyl group bound to a
  protein), takes the first of that substance's synonyms that isn't a KEGG
  number or a CAS number (`Lipoamide-E`). Only if PubChem has neither is the
  KEGG number used, and the status bar says how many. PubChem asks for at most
  five requests a second, which a map of a few dozen compounds is well within.
  Its data are in the public domain. No mapping file is shipped: one built from
  KEGG's names would pass KEGG's data on, which its licence doesn't allow.
- **Without a connection** the map still opens, with KEGG numbers, and the
  status bar says the names couldn't be fetched.

## 6. What isn't read, and warnings

As for Escher (S §11.7), the import goes ahead, with one warning per kind of
thing left out:

- A map with no reactions (signalling maps): an error, *"This KEGG map has no
  reactions (signalling maps aren't supported yet)."*
- A global or overview map (`01100` to `01250`, enzymes drawn as lines): an
  error for now, *"Global KEGG maps aren't supported yet."* (Its compounds and
  reactions could be read; its enzyme lines would need corner points, a later
  step.)
- Relations (`ECrel`, `maplink`), groups, and ortholog boxes with no reaction:
  skipped without a warning, since a metabolic map always has them.
- A reaction naming an entry that isn't a compound: that reaction is skipped,
  with a warning naming it.

## 7. Tests

- KGML written for the tests (a small map with an isozyme pair, a box carrying
  two reactions, a compound drawn twice, a map link and a skipped relation),
  checking every row of §3, the junction rule, the ids and the warnings.
- Real maps, read from a folder named by an environment variable as the Antimony
  corpus is, since KEGG's files can't be committed (§8): every reaction read,
  no warnings beyond those expected, and the diagram written to Antimony and
  SBML and read back unchanged.
- Breaking each rule on purpose, as usual, and the result shown in the browser.

## 8. KEGG's licence

KEGG is free for academic use through its website and its API; downloading in
bulk, and passing its data on, need a licence from Pathway Solutions. So:

- PathwayDesigner reads a map the user fetches, one at a time; it doesn't bundle
  or cache KEGG's files.
- No KEGG map goes in the Gallery or the repository's tests.
- The help page and the README say where the data come from and that KEGG's
  terms apply to it.
- **K5.** A diagram made from a KEGG map is the user's to keep and share as they
  see fit under those terms; PathwayDesigner adds a note to the model saying
  which map it came from (`// From KEGG map eco00010, Glycolysis /
  Gluconeogenesis`), in the Antimony comment and the SBML notes. The SBML note
  is the model's ordinary `notes`, not a PathwayDesigner annotation.

## 9. Steps

1. The reader in pd-core (`kegg.rs`): §3 and §6, with K1 to K3 as decided, and
   the tests of §7.
2. File ▸ Open reads KGML; the look of §4; shown in the browser.
3. Compound names from PubChem, on the desktop and in the browser (K2).
4. File ▸ Open from KEGG… (K6).
5. A help page, README and SPECIFICATION entries, release notes.

## 10. Decisions (made 2026-10-26)

- **K1.** Enzymes as species (boxes) with catalyst lines.
- **K2.** Real names wherever possible, from PubChem, on the desktop and in the
  browser; KEGG numbers only when no name is found.
- **K3.** KEGG's coordinates and a KEGG-like look, compounds 12 across.
- **K4.** Links to other pathways as grey texts.
- **K5.** A note in the model saying which KEGG map it came from.
- **K6.** File ▸ Open from KEGG…: fetches on the desktop; in the browser, opens
  KEGG's file in a new tab to be saved and opened.
