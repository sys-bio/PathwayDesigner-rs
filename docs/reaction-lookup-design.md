# Annotating reactions from their names: design

Status: decisions R1–R8 agreed with the author as proposed, 2026-10-30.
Step 1 done (pd-core/src/reaction_lookup.rs). Tried on the Millard model,
after looking its species up in ChEBI: BiGG's search alone found mostly other
reactions (threonine ammonia-lyase for a transaldolase), so a reaction it
finds is offered only when it shares the reaction's species: all of them, or
two and one of those not a common cofactor (ATP, NAD, CoA …). Then 31 of 68
reactions were offered (20 from BiGG by id, 6 from Rhea, 5 from the search),
each the right reaction or its close relative, and 7 ticked, all correct. The
others are Millard's lumped and exchange reactions, or have species the
species lookup didn't identify (DAP, P).

## The problem

Annotate Species from Names (docs/chebi-lookup-design.md) gives species their
ChEBI entries; reactions still have to be annotated by hand. A reaction's
usual references are its EC number (the enzyme), its Rhea reaction, and its
KEGG and MetaNetX reactions. For the Millard *E. coli* model's 68 reactions
that is again a lot of searching.

## What is available

Tried on 2026-10-30; both answer web pages from any site (CORS `*`).

- **BiGG reactions**, by id:
  `https://bigg.ucsd.edu/api/v2/universal/reactions/<id>`. Each has a name,
  its metabolites with stoichiometry, and links: EC numbers, Rhea, KEGG
  reaction, MetaNetX, SEED. `PPC` is "Phosphoenolpyruvate carboxylase",
  EC 4.1.1.31, Rhea 23072, KEGG R00345, MetaNetX MNXR103096. BiGG lists four
  Rhea ids per reaction (23072–23075): Rhea's master reaction and its three
  directions; the master is the lowest.
- **BiGG's search**, `…/api/v2/search?query=<words>&search_type=reactions`,
  by id or name.
- **Rhea**, searched by participants' ChEBI ids:
  `https://www.rhea-db.org/rhea?query=chebi:58702+AND+chebi:16452&columns=rhea-id,equation,ec&format=tsv`
  gives the Rhea reactions with both, their equations and EC numbers. This
  works for any model whose species have ChEBI entries, whatever its
  reactions are called.

**Tried on the Millard model.** BiGG knows 20 of its 68 reaction ids exactly
(PGI, PFK, FBA, TPI, PGK, ENO, PYK, PPC, PDH, MDH, …). The others are named
differently from BiGG (ZWF is BiGG's G6PDH2r, GPM is PGM, ICD is ICDHyr, ACK
is ACKr, FUMA is FUM), are lumped (PTS_0…PTS_4, ACEK_1) or are not reactions
in the chemical sense (GROWTH, ATP_MAINTENANCE, the XCH_ exchanges). BiGG's
search finds some of the renamed ones as its first answer (ACK → ACKr, ADK →
ADK1, PCK → PPCK) but not others (ZWF, GPM), and for FUMA its first answer is
the wrong enzyme (fumarylacetoacetase). An id alone is a weak clue; the
reaction's species are a strong one.

## Proposal

**Annotate Reactions from Names…**, next to Annotate Species from Names, and
in a reaction's right-click menu; it works as that does. For each reaction
without an EC number or Rhea reference (the selected ones, or all), one
request at a time:

1. **BiGG by id**: the reaction's id, as it is and upper case, then its
   display name.
2. Else **Rhea by species**: when two or more of the reaction's species have
   ChEBI entries, the Rhea reactions that have all of them.
3. Else **BiGG's search** by the id, its answers offered but never ticked.

**Checking a match against the species.** A BiGG reaction lists its
metabolites (`pep`, `oaa`, `co2`); a match is *sure* only when the
reaction's species agree with them: each species' id or display name is
one of BiGG's metabolites (by BiGG id or name, case aside, as the species
lookup matches them), leaving aside H+, water and the like that models
often omit. A Rhea match is sure when it is the only
one with all the species.

**The review**, in the Annotate tab as for species: each reaction, its name
found, where from, the EC numbers and Rhea reaction proposed (a list when
there are several), links to their pages, and BiGG's equation, to compare
with the reaction. A tick per row, on only for a sure match, and a tick to
take the name found as its display name. Apply adds the ticked rows as one
undo step, *Annotate Reactions*.

**What isn't done.** Nothing is added unseen. Reactions with an EC number or
Rhea reference already are left alone. The reaction's equation is never
changed, even when BiGG's differs. Kinetic laws are not looked up (SABIO-RK
would be a separate feature).

**Tests.** As for species: a state machine in pd-core fed canned answers, and
an app test with a canned fetch.

## Decisions (agreed 2026-10-30)

- **R1. The services and their order.** BiGG by id, then Rhea by species,
  then BiGG's search (proposed). Rhea could come first, as it doesn't depend
  on names at all, but it needs the species annotated first.
- **R2. Which references.** EC numbers as `bqbiol:isVersionOf` (an EC number
  is a class of enzymes, and BioModels writes it so); Rhea's master reaction,
  KEGG reaction and MetaNetX reaction as `bqbiol:is`. SEED left out, as for
  species. Proposed.
- **R3. Several EC numbers.** BiGG gives three for GDH and MDH has two. Add
  them all (proposed), or let the user pick.
- **R4. When a match is sure.** As above: the species agree with BiGG's
  metabolites, or a single Rhea match. Proposed.
- **R5. Display names.** Offer BiGG's name (PPC → "Phosphoenolpyruvate
  carboxylase") as the display name, on for a reaction without one, as for
  species. Proposed.
- **R6. Species not yet annotated.** Rhea needs species' ChEBI entries.
  Either say so in the review ("annotate the species first to find more"),
  or run the species lookup first in the same command. Proposed: say so.
- **R7. SBO term.** Set the reaction's SBO term too (biochemical reaction,
  SBO:0000176; transport, SBO:0000185, when BiGG's metabolites span
  compartments)? Proposed: no, leave SBO to the user.
- **R8. The menu.** Edit ▸ Annotate Reactions from Names…, and in a
  reaction's right-click menu. Proposed.

## Steps

1. The lookup (pd-core): BiGG and Rhea requests and answers, the order, the
   check against the species (R1–R4), with tests on canned answers.
2. The menu items, the searching, the review in the Annotate tab, Apply
   (R5–R8).
3. The specification and help.
