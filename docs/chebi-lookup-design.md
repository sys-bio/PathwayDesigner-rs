# Finding ChEBI entries by name: design

Status: decisions C1–C9 made with the author, 2026-10-29. All four steps done
(2026-10-29). Tried on the Millard model, OLS4's loose search gave only wrong
matches and is not used; "sure" was made stricter (a match's name must be the
name looked for). Added with step 3: a species' name in the review selects it
and centres the diagram on it, and the list on the left has a Find box.

## The problem

Annotations can now be kept and edited (docs/annotations-design.md), but a
reference has to be typed: the user searches ChEBI's web site for each
species and copies its number. For the Millard *E. coli* model's 77 species
that is 77 searches. PathwayDesigner could do the searching, and leave the
user only to check and accept what it finds.

## What is available

Three free services, each tried on 2026-10-29; all three answer web pages
from any site (CORS `*`), over https, so they work in the browser version as
well as on the desktop, through the same fetch the Gallery and the KEGG
names lookup use.

- **EBI's Ontology Lookup Service (OLS4)**, which holds ChEBI, searched by
  name: `https://www.ebi.ac.uk/ols4/api/search?q=<name>&ontology=chebi`.
  An exact search for `ATP` gives CHEBI:15422 first; a free search for
  `glucose 6-phosphate` gives β-D-glucose 6-phosphate (CHEBI:17719), its
  charged form and others.
- **BiGG**, the database of genome-scale metabolic models:
  `https://bigg.ucsd.edu/api/v2/universal/metabolites/<id>`. Its ids are the
  abbreviations models like Millard's use (`g6p`, `accoa`, `fdp`), and each
  metabolite has a full name and its ChEBI entries: `g6p` is
  "D-Glucose 6-phosphate", with CHEBI:4170 among them. Some abbreviations
  differ (Millard's `PGA2` is BiGG's `2pg`).
- **PubChem**, as the KEGG names lookup uses it: a name gives a compound,
  whose synonyms include its ChEBI number (`glucose 6-phosphate` →
  CHEBI:4170).

## Proposal

**One item: Find in ChEBI.** In a species' Annotations part, a search field,
filled in with its display name (or its id), and a Search button. The
matches are listed, each its ChEBI number and name and a link to its page;
clicking Add on one adds it as a reference (`bqbiol:is
https://identifiers.org/CHEBI:…`), one undo step. The user can change the
words and search again.

**All species: Annotate Species from Names….** For each species without a
ChEBI reference (the selected ones, or all if none is selected), the app
asks, one request at a time:

1. BiGG, by the species' display name, then its id, made lower case;
2. OLS4, an exact search by the display name (or BiGG's full name, if BiGG
   found one), then a free search;
3. PubChem, by the same name.

The first that gives an answer is used; what it gives are one or more ChEBI
entries and, where known, a full name. Progress shows as *Searching 12 of
77…*, and the search can be stopped.

**The review.** When the searching is done, a table lists each species: its
id and name, what was found (the full name, and where it came from), and
the ChEBI entry proposed, with a list when there are several. Each row has a
tick, on for a single clear match and off when the choice is uncertain or
nothing was found. **Apply** adds the ticked entries as references, all as
one undo step, *Annotate Species*; nothing is changed before that. A link on
each entry opens its ChEBI page, to check.

**What isn't done.** Nothing is added without the user seeing it. A species
that already has a ChEBI reference is left alone. Reactions are not looked
up (BiGG has reactions too, with EC numbers and Rhea; a later step).

**Tests.** As for the KEGG names lookup: the searching is a state machine in
pd-core, fed answers by the tests, never the network; the app test swaps the
fetch for one with canned answers.

## Dec## Decisions (made 2026-10-29)

- **C1. Order of the services.** BiGG first, then OLS4, then PubChem: BiGG
  knows the abbreviations metabolic models use, OLS4 searches ChEBI itself,
  PubChem catches what both miss.
- **C2. Which entry when there are several.** ChEBI often has several for
  one compound: charge forms (glucose 6-phosphate(2−)), α and β. The one
  whose name is the found name, if any, is proposed; else the first; always
  shown with the others to choose from, and the tick off.
- **C3. Display names too.** When a full name is found, it is offered as the
  species' display name (FDP → "D-Fructose 1,6-bisphosphate"), in a second
  tick per row: on for a species without a display name, off for one that has
  one.
- **C4. Other references.** The ChEBI entry, and, when BiGG found the
  species, BiGG's KEGG compound (`kegg.compound/C00092`) and MetaNetX
  chemical (`metanetx.chemical/MNXM160`) links, each `bqbiol:is`. BiGG gives
  others (SEED, BioCyc, Reactome, HMDB, the InChI key), which are not added.
- **C5. Which species.** The selected ones, or all if none is selected.
- **C6. Where the review goes.** A temporary *Annotate* tab in the right-hand
  panel, which appears for the review and closes after Apply or Cancel; to be
  looked at once it is built.
- **C7. The menu.** Edit ▸ Annotate Species from Names…, and the same in a
  species' right-click menu (for the selected species).
- **C8. Load on the services.** One request at a time, none faster than
  every 100 ms, so a whole model's lookup takes a minute or less: for 77
  species at most about 230 requests.
- **C9. Without a network.** If no service can be reached, say so and change
  nothing; if some answered, review what they found.

ps

1. The lookup (pd-core): the three services' requests and answers, the order
   and the choice (C1, C2), with tests on canned answers.
2. Find in ChEBI in the Annotations part.
3. Annotate Species from Names: the menu item, the searching, the review
   tab and Apply (C3–C9).
4. The specification and help.
