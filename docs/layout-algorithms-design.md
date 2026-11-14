# Other layout methods — design

2026-11-14. Design; L1, L2, L5 and L7 settled with the author as proposed,
the rest proposed. Step 1 built (2026-11-14): the measures
(`pd_core::layout_measures`) and the report (`pd-export` example
`layout_quality`); the baseline is below. Step 2 built (2026-11-14):
separating cofactors, with its results below. Step 3 built (2026-11-14):
*Stress* and the Method list, below. Step 4 built (2026-11-14): *Layered*,
below.

Models without a drawing (Antimony without layout statements, SBML without a
Layout, generated networks) are drawn by the automatic layout. This proposes
methods besides the one there is, and what goes with them.

## Now

- **One method**, force-directed in the style of ForceAtlas2 (Jacomy et al.
  2014), for networks with reaction junctions (S §10.1, `pd-core/src/layout.rs`).
  Every species and every junction is a node; springs join a reaction's
  species to its junction; every pair of nodes repels; gravity pulls towards
  the centre. 800 iterations, a few per frame, so the app animates it.
- **Options**: *Randomize* scatters the unlocked species first (seeded);
  *Deckard* runs the Deckard & Bergmann handle pass afterwards (S §10.2).
- Locked species stay put, and an alias rides on its primary.
- A model read without a drawing has its species put at random in
  [0, 600)², and the app offers Auto-layout (Antimony panel, SBML import).
  The diagram format's reader lays out what its text doesn't place.
- The result depends on the starting positions and the seed. It tends to a
  round cloud: it shows which species are near which, not the direction of
  the pathway, and a few species used by many reactions (ATP, NADH, H₂O, H⁺)
  pull everything into a knot at the centre.
- The Escher theme already knows cofactors by name
  (`themes::is_cofactor`: ATP, ADP, NAD(P)(H), H₂O, Pi, CO₂, H⁺, CoA, … with
  BiGG's `M_` prefix and compartment suffix ignored).

## Methods considered

- **Layered** (Sugiyama, Tagawa & Toda 1981; Graphviz *dot*, Gansner et al.
  1993). Every reaction's substrates go above its junction and its products
  below, so the network reads top to bottom, as pathways are drawn in
  textbooks. Steps: break cycles by turning the fewest edges round (Eades, Lin
  & Smyth 1993); put nodes in layers (longest path, or dot's network simplex
  for shorter edges); reduce crossings by reordering each layer (barycentre
  sweeps); set x positions (Brandes & Köpf 2001). Best for metabolism; a
  cycle such as TCA comes out as a long loop, not a ring.
- **Stress majorization** (Gansner, Koren & North 2004; Graphviz *neato*).
  Places nodes so that distances on the screen match distances in the
  network (shortest paths). Deterministic (started from the network's own
  distances, not at random), keeps related parts together and unrelated ones
  apart, and honours fixed nodes naturally. A few hundred lines. All-pairs
  distances are O(n²) in memory: fine to a few thousand nodes; a sparse
  version (Ortmann, Klimenta & Brandes 2016) is the way past that.
- **Stress with constraints** (IPSep-CoLa, Dwyer, Koren & Marriott 2006; the
  *cola* layout in Cytoscape). Stress, plus rules kept exactly: a reaction's
  products below its substrates, no two nodes overlapping, chosen nodes in a
  line, a compartment's species inside its box. The flow of *Layered* with
  the shape of *Stress*. A constraint solver is the larger part of the work.
- **Templates** (Karp & Paley 1994, Pathway Tools). Find linear chains,
  cycles and branches, draw each as a line, a ring or a fork, then arrange
  the pieces. The most textbook-like for metabolism; only for networks made
  of such pieces.
- **Compartments first.** Lay out the compartments as boxes, then each one's
  species inside it, transport reactions across the borders. Goes with any of
  the others.

**Graphviz's code is not used.** Graphviz is under the Eclipse Public
License; a translation of it into Rust would be a derivative work under that
licence, which PathwayDesigner's MIT licence does not allow, and *dot* alone is
tens of thousands of lines of C. The algorithms are published, and are
written here from the papers.

## Decisions

- **L1. Which methods, in what order.** *Settled:* separating cofactors
  first (L2), since it helps every method; then *Stress* (L4); then
  *Layered* (L5). Stress with constraints and templates later, if those two
  leave gaps.
- **L2. Cofactors.** A cofactor used by many reactions draws them all
  together. *Settled:* a **Separate cofactors** tick beside Auto-layout,
  on by default: before the layout, every cofactor (the Escher theme's list)
  used by more than one reaction gets one alias per reaction, which takes no
  part in the layout and is then put beside its reaction, off the main line,
  as Escher maps draw them. Part of the same undo step. Aliases are drawings,
  so the model's meaning does not change. Cofactors are the built-in list
  only: nothing new to store.
- **L3. Choosing the method.** *Proposed:* a **Method** list beside
  Auto-layout: *Organic* (the force layout, as now), *Stress*, *Layered*
  (top to bottom), and later *Layered, left to right*. The choice is a
  preference, remembered. *Randomize* applies to *Organic* only; the others
  need no seed. *Deckard* applies to all.
- **L4. Stress.** *Proposed:* species and junctions are nodes, as now;
  distances are counted in species–junction steps, a step 100 units long
  (a node's width and a gap). Locked species are fixed; aliases ride on their
  primary, as now. Run in steps, a few per frame, so it animates like the
  force layout and can be cancelled. Started from classical scaling
  (positions from the distances alone), so the same model always gives the
  same drawing.
- **L5. Layered.** *Settled:*
  - Each reaction is a substrate → junction → product chain; a reversible
    reaction is drawn the way it is written. A modifier takes no part in the
    layers and is put beside its reaction.
  - Cycles are broken by turning round the fewest reactions; those are drawn
    upwards.
  - Locked species: *Layered* rearranges everything, so with species locked
    it is offered greyed, with the hint "Unlock species to use Layered".
  - One step, not animated: it is fast.
  - Then the handles: straight lines between layers, or the Deckard pass when
    ticked.
- **L6. Compartments.** *Proposed:* for now, species keep their
  compartments and each compartment's box is refitted around its species
  afterwards (as `compartments::place_unplaced` does for a box it fits).
  Laying out each compartment as a block is a later step, with constraints.
- **L7. A model read without a drawing.** *Settled:* as now, species at
  random and Auto-layout offered, which then uses the method chosen. Once
  the new methods are proven (L9), the default may change, so that a model
  read without a drawing is drawn by *Stress* or *Layered* at once.
- **L8. Our own code.** *Proposed:* written in `pd-core` from the papers,
  with no new dependency: deterministic on every platform, works in the
  browser, and knows junctions, aliases and locks, which general graph
  layout crates (`layout-rs`, `rust-sugiyama`) do not. Their code is looked
  at only if their licences allow.
- **L9. How to judge.** *Proposed:* measures computed in `pd-core` for any
  drawing: line crossings, overlapping nodes, spread of line lengths, and,
  for directed methods, the share of reactions drawn downwards. A test set:
  the MAPK cascade, the Millard *E. coli* model, the *E. coli* core model
  without its Escher map (and compared with the map), a sample of BioModels
  without drawings, and a Recon3D subsystem. A report of every method on
  every model, measures and pictures, before choosing defaults.

## Baseline (step 1)

The force layout on each test model read without a drawing (the mean of
three seeds), against the model's own drawing where it has one:

| Model | Species (drawn / laid out) | Crossings | Lines through species | Down |
|---|---|---|---|---|
| MAPK cascade | 8 / 8 | 0 / 0.7 | 0 / 0 | 0% / 50% |
| TCA cycle | 25 / 18 | 0 / 11 | 0 / 3.7 | 50% / 58% |
| Yeast glycolysis (Gepasi) | 45 / 25 | 1 / 55 | 0 / 9 | 79% / 46% |
| Millard *E. coli* | 234 / 78 | 3 / 1,084 | 23 / 106 | 54% / 46% |
| *E. coli* core, Escher's map | 244 / 72 | 4 / 3,336 | 0 / 163 | 45% / 50% |
| Recon3D glutathione map | 47 / 31 | 6 / 35 | 2 / 6.7 | 81% / 54% |

Eight BioModels without drawings (10 to 30 reactions) get 0.3 to 95
crossings. What it shows:

- The hand-drawn and Escher drawings have a few crossings; the force
  layout has hundreds to thousands on the metabolic models.
- The drawings have far more species than the models laid out: they draw
  cofactors as one alias per reaction (244 drawn against 72 species in the
  core model). That is L2's separation, and much of the gap.
- About half of the force layout's reactions point down, as chance would
  have it; a layered layout should be near 100%, as the Recon3D map's 81%
  suggests a drawn pathway is.

## Separating cofactors (step 2)

`layout::separate_cofactors` gives every cofactor two or more reactions
use one drawing per reaction: the first reaction keeps the species, each
other gets an alias. In the layout (`LayoutParams::separate_cofactors`), a
cofactor drawn to one reaction only, not locked and not a modifier
elsewhere, of a reaction with a species that is not a cofactor, takes no
part: it is put beside its reaction, a reactant before the junction and a
product after it along the reaction's direction, on the side with fewer
species near the junction, and, where that place is taken, farther out or
on the other side. Each such cofactor adds 2 to its junction's mass, so the
layout leaves room for it. A reaction of cofactors only (ATP → ADP) is laid
out as before. The app's **Separate cofactors** tick, beside Auto-layout,
is on by default; the aliases are part of the layout's undo step, and
Cancel takes them back.

The force layout with and without it (the mean of three seeds):

| Model | Crossings | Lines through species | Overlaps |
|---|---|---|---|
| TCA cycle | 11 → 3.7 | 3.7 → 6.7 | 0 → 0 |
| Yeast glycolysis (Gepasi) | 55 → 13 | 9 → 8.7 | 0.3 → 0.3 |
| Millard *E. coli* | 1,084 → 513 | 106 → 178 | 0.7 → 5.3 |
| *E. coli* core | 3,336 → 738 | 163 → 356 | 0.3 → 18 |
| Recon3D glutathione map | 35 → 13 | 6.7 → 13 | 0 → 0.3 |

Crossings fall by half to four fifths. Lines through species rise: the
force layout packs the drawing more tightly when the cofactors are out of
it, and a cofactor beside one reaction can sit on another's line. A heavier
junction (4 or 6 for each cofactor) gives fewer overlaps and more
crossings; 2 is the middle. Models without cofactors are unchanged.

## Stress (step 3)

`layout/stress.rs`, chosen in the **Method** list beside Auto-layout
(*Organic*, *Stress*), remembered as a preference; *Randomize* is greyed
for Stress, which needs no seed. As built, beyond L4:

- **Lines.** Each species–junction line is 130 long (100 left too many
  lines through boxes; 160 a much larger drawing), longer into a junction
  with more than four lines (by the square root of how many more), so a
  busy reaction's species fit round it. Modifier lines count too, so a
  regulator is drawn by the reaction it regulates.
- **Parts.** Each part of the network not joined to the rest is laid out on
  its own and the parts are packed in rows, the largest first; before, they
  were pushed to a fixed distance, scattered round the drawing.
- **Boxes.** Stress places points: afterwards, boxes that overlap are pushed
  apart, each pair along the axis it overlaps less on.
- **Cofactors.** With Stress, separated cofactors stay in the layout as
  nodes of their own, one line from their junction, instead of being put
  beside it afterwards: placed in free space by the method itself, they
  overlap nothing (put beside afterwards, 78–107 overlaps on the two
  *E. coli* models).
- It stops once an iteration improves the stress by less than 1 in 10⁵, at
  most 300 iterations; the same model gives the same drawing, about where
  its species were.

| Model | Organic + cofactors | Stress + cofactors |
|---|---|---|
| TCA cycle | 3.7 crossings, 6.7 through | 0, 0 |
| Yeast glycolysis | 13, 8.7 | 0, 0 |
| Millard *E. coli* | 513, 178, 5.3 overlaps | 326, 136, 0 |
| *E. coli* core | 738, 356, 18 overlaps | 138, 117, 0 |
| Recon3D glutathione | 13, 13 | 1, 0 |
| BioModels 1, 7, 9, 11 (no cofactors) | 12.7, 12.3, 41.7, 95 crossings | 0, 1, 7, 25 |

Stress gives fewer crossings and no overlaps on every model; line lengths
are even (spread 0.1–0.4 against 0.3–0.8). About half the reactions point
down, as with Organic: direction is for *Layered*.

## Layered (step 4)

`layout/layered.rs`, the third entry in the Method list. Each reaction is
a reactant → junction → product chain; cycles are broken by the greedy
order of Eades, Lin & Smyth, those lines drawn upwards; each node goes one
layer below its lowest predecessor and then as low as its successors allow,
so an input sits just above its use; a line across several layers gets a
corner point in each (kept on the line's leg, S §5.6.5), so it runs between
boxes, not through them; each layer is ordered by barycentre sweeps with
Gansner et al.'s transpose, keeping the order with the fewest crossings;
places across are each layer's least-squares fit to its neighbours, in
order and with gaps, exact (pool adjacent violators). Parts not joined to
the rest are packed in rows. Cofactors, when separated, are put beside
their reaction as with Organic, and a species that only regulates is put
left of the junction of the first reaction it regulates; the layout keeps
room for both. One step, not animated. With a species locked, Layered and
Auto-layout with it are greyed out: "Unlock species to use Layered".

| Model | Stress + cofactors | Layered + cofactors |
|---|---|---|
| TCA cycle | 0 crossings, 38% down | 0, 88% |
| Yeast glycolysis | 0, 47% | 2, 100% |
| Millard *E. coli* | 326, 54% | 266, 76% |
| *E. coli* core | 138, 51% | 273, 91% |
| Recon3D glutathione | 1, 56% | 13, 75% |
| BioModels 7, 9, 11 (signalling) | 1, 7, 25 crossings | 9, 23, 90 |

*Layered* draws most reactions downwards, as pathways are drawn (yeast
glycolysis entirely), where *Stress* points them any way; *Stress* has
fewer crossings on dense and signalling networks. So: *Layered* for a
pathway, *Stress* for a network without one direction.

## Steps

1. The measures and the test set (L9), with the force layout as the
   baseline.
2. Separate cofactors (L2), with tests; shown in the browser.
3. *Stress* (L4) and the Method list (L3); measured against the baseline;
   shown in the browser.
4. *Layered* (L5); measured; shown in the browser.
5. Defaults chosen from the report (L7); docs: help, README, SPECIFICATION
   §10, SPEC-Rust.
6. Later: stress with constraints (flow, no overlaps, compartments as
   blocks, L6); templates for chains and cycles.

## References

- Brandes U, Köpf B. Fast and simple horizontal coordinate assignment.
  Graph Drawing 2001.
- Deckard A, Bergmann FT, Sauro HM. Supporting the SBML layout extension.
  Bioinformatics 2006.
- Dwyer T, Koren Y, Marriott K. IPSep-CoLa: an incremental procedure for
  separation constraint layout of graphs. IEEE TVCG 2006.
- Eades P, Lin X, Smyth WF. A fast and effective heuristic for the feedback
  arc set problem. Information Processing Letters 1993.
- Gansner ER, Koutsofios E, North SC, Vo KP. A technique for drawing directed
  graphs. IEEE Transactions on Software Engineering 1993.
- Gansner ER, Koren Y, North S. Graph drawing by stress majorization. Graph
  Drawing 2004.
- Jacomy M, Venturini T, Heymann S, Bastian M. ForceAtlas2, a continuous
  graph layout algorithm. PLoS ONE 2014.
- Karp PD, Paley SM. Automated drawing of metabolic pathways. Third
  International Conference on Bioinformatics and Genome Research, 1994.
- Ortmann M, Klimenta M, Brandes U. A sparse stress model. Graph Drawing
  2016.
- Sugiyama K, Tagawa S, Toda M. Methods for visual understanding of
  hierarchical system structures. IEEE Transactions on Systems, Man, and
  Cybernetics 1981.
