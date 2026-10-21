# Design: the stoichiometry matrix, its null space and conserved moieties, on the network

Status: **draft, for decision** (2026-10-21). Nothing here is built yet.

## 1. Why

A reaction network's structure says a good deal before any rate law is known, and
most of it is easier to see on the network than in a matrix.

- **The flux null space.** The steady-state fluxes satisfy `N v = 0`. A basis of
  the null space of `N` has (reactions − rank `N`) vectors, so it stays small as
  the network grows, unlike the elementary modes, whose number can grow
  combinatorially. Each basis vector is a set of reactions whose rates can change
  together, in its proportions, while every metabolite stays balanced. Since most
  rate laws are proportional to their enzyme's level, changing the enzymes of a
  null-space vector in those proportions changes the fluxes along it and leaves
  the metabolite levels where they were: the null space tells an engineer which
  enzymes to change along a path. This is the structure behind the summation
  theorems of metabolic control analysis and the branch-point theorems, which
  come from the flux null space; any basis is informative, and the one used (section 5)
  is the K matrix of structural analysis, as COPASI and libRoadRunner compute it.
- **Conserved moieties.** The left null space of `N` gives the conservation laws:
  sums of species, such as ATP + ADP + AMP, that no reaction changes. Each is a
  set of species to highlight together.
- **The stoichiometry matrix** itself, to read, copy and export.

Other tools: COPASI computes conservation laws and elementary flux modes and can show
both on its network diagrams; CellNetAnalyzer (MATLAB) and CNApy (Python, desktop)
show elementary modes and flux distributions on maps; CNApy reports only the number
of conservation relations. None of them, as far as we found (2026-10-21), draws a
basis of the flux null space on the network. The paper describes the moiety view as
what COPASI also does, and the null-space view as this work's own, tied to its MCA
meaning.

## 2. The matrix

- **Rows:** the model's floating species: not boundary, not constant, and not set
  by an assignment rule. An alias is its primary's row (an alias is a drawing, not
  a species). Null nodes (sources and sinks) have no row. Order: the model's species
  order.
- **Columns:** every reaction, in the model's order.
- **Entries:** for each reaction, the sum of the product stoichiometries minus the
  sum of the reactant stoichiometries of each species, aliases merged into their
  primary. Modifiers contribute nothing.
- This is libRoadRunner's full stoichiometry matrix (`getFullStoichiometryMatrix`),
  and is checked against it (section 7).

## 3. The method: QR factorization with column pivoting

The method COPASI and libStructural use (Vallabhajosyula, Chickarmane and Sauro,
*Bioinformatics* 2006), checked in COPASI's source (`copasi/utilities/CLinkMatrix.cpp`,
2026-10-21): a QR factorization with column pivoting (LAPACK `DGEQP3`), the rank from
a tolerance, and the link matrix from the inverse of the triangular factor
(`DTRTRI`).

- **Floating point**, double precision, with Householder reflections and column
  pivoting by the largest remaining column norm, as `DGEQP3` does. **Our own code**
  in `pd-core` (decided 2026-10-21; about 150 lines, no new dependency; LAPACK is
  not available in the browser). nalgebra's `ColPivQR` was considered and not used:
  it pivots on the column holding the largest single entry (`icamax_full`), not the
  largest column norm, which reveals rank less reliably and gives a different
  split into independent and dependent species than COPASI and libRoadRunner, so
  the results could not be compared with theirs matrix for matrix.
- **Rank:** the number of diagonal entries of R larger than
  max(rows, columns) × |R₁₁| × machine epsilon, COPASI's tolerance.
- **Clean-up for display**, as COPASI zeroes entries below 100 × epsilon: an entry
  within a small tolerance of zero is zero, and one within it of a small integer or
  a simple fraction (denominator up to 12) is shown as that, so the usual results
  read 1, −1, 2, 1/2 rather than 0.9999999. Exported CSV is cleaned the same
  way (a value within the tolerance of a simple fraction is written as that
  fraction's value); any other value as computed.
- **Size.** The factorization is dense, O(m n min(m, n)). Measured natively
  (step 2, 2026-10-21): 66 × 43 in 0.4 ms, 155 × 205 in 12 ms, 366 × 340 in 91 ms;
  about a cube law, so some 1,000 reactions take seconds. Recon3D whole (about
  5,800 × 10,600) was stopped after ten minutes; it would also need about 1 GB for
  two dense copies. Every Gallery model, Escher map and BioModel tried is far
  inside the range. Genome-scale models would need a sparse factorization, a
  project of its own; until then the Structure tab says a network is too large
  above a limit (step 3; D4). It is computed only while the tab is open (§6).
- **Ties.** Columns of equal norm (common: most stoichiometry columns are a few
  ±1) go to the one first in the matrix, the species or reaction listed first in
  the model. LAPACK breaks such ties by its rounding, and libRoadRunner reorders
  species first, so the independent species it reports can be another, equally
  valid choice (step 2: the same species, or the same set in another order, in 23
  of the 36 BioModels where libRoadRunner reports them and 12 of the 16 Gallery
  models; every matrix, rank and space the same).

## 4. Conservation laws and the link matrix

QR with column pivoting of `Nᵀ`: the pivot order sorts the species into
**independent** (the first rank of them) and **dependent**; with `R = [R₁₁ R₁₂]`,
`L₀ = (R₁₁⁻¹ R₁₂)ᵀ`, the link matrix is `L = [I; L₀]`, and the conservation matrix
is `Γ = [−L₀ I]`. Each row of `Γ` is a conservation law: a dependent species and the
independent species it is tied to. Shown as an equation, `ATP + ADP + AMP =
constant`, coefficients other than 1 written (`2 A + B`). This is what
libRoadRunner's `getLinkMatrix` and `getConservationMatrix` give; species chosen as
dependent by the pivoting, as there.

## 5. The flux null space (the K matrix)

QR with column pivoting of `N` itself: the pivot order sorts the reactions into
**dependent** (the first rank of them) and **independent**; with `R = [R₁₁ R₁₂]`,
`K₀ = −R₁₁⁻¹ R₁₂`, and `K = [K₀; I]` (reactions in pivot order). Each column is a
basis vector of the null space: one independent reaction at 1, the other
independent ones at 0, and the dependent fluxes that must follow. This is
libRoadRunner's `getKMatrix`. Each vector is shown named by its independent
reaction, `J3: J3 + J4 − J5`, scaled so its entries are the smallest integers when
they are rational (they usually are).

## 5b. A sparse basis (step 3b, author's request 2026-10-21)

The K basis is the standard object, but on a large network its vectors are long:
the dependent reactions the pivoting chose are spread over the map, so every vector
is a mixture of routes (RECON1 carbohydrate: 35 reactions per vector on average, up
to 69). The same space has bases whose vectors are short routes, which read and
highlight far better. The tab offers both: **Basis: K matrix | Sparse**, the K
matrix by default.

`sparsify`: starting from the K basis, a vector is replaced by itself minus the
multiple of another that cancels an entry they share, whenever that leaves it fewer
nonzero entries, taking the cancellation that leaves fewest; repeated until nothing
helps. Each replacement keeps a basis of the same space, and the total number of
nonzero entries falls each time, so it ends. Greedy, so not always the sparsest
basis (which is hard to find in general), but deterministic. Vectors are then in
whole numbers where their proportions allow, first entry positive, sorted by their
first entry. The same is done to the conservation laws (a mixed law becomes
separate moieties).

Measured (step 3b): mean reactions per vector, K → sparse: BioModels 6.2 → 3.9 (41
models), Millard 30 → 9.3, E. coli core 40 → 15.9, RECON1 carbohydrate 34.7 → 6.5
(95 ms natively). Every sparse basis spans the same space as libRoadRunner's K
matrix and conservation matrix (scripts/structure_check.py). Taking the first
helpful cancellation instead of the best gives longer vectors (E. coli core 22.6,
RECON1 7.7), so the best is taken.

## 6. The interface

**A new tab, Structure,** beside Properties, Antimony and Styles, recomputed
whenever the model changes while it is open (built in step 3; the matrix in a
panel under the canvas, the author's choice; the basis choice of §5b in step 3b):

- A summary: species (floating), reactions, rank, number of independent fluxes,
  number of conservation laws.
- **Stoichiometry matrix:** a scrollable table, species by reactions, zeros shown
  as blank, with *Copy* (tab-separated, pastes into a spreadsheet) and *Export…*
  (CSV).
- **Flux null space:** one row per basis vector, named by its independent
  reaction ("J3: J3 + J4 − J5"), each a toggle.
- **Conservation laws:** one row per law, as its equation, each a toggle.
- Copy and Export… for each list too.

**Highlighting on the network.** Choosing a vector or a law highlights it:

- everything else is drawn faded (its colours mixed towards the background);
- a null-space vector's reactions are drawn in a highlight colour, with the
  coefficient beside each junction, and the reactions whose coefficient is
  negative in a second colour (they run the other way in that pattern);
- a conservation law's species, every drawing of each (aliases included), are
  outlined in the highlight colour, with the coefficient beside each.
- Choosing another replaces it; Esc or a click on the empty canvas clears it.
- The highlight is a view, not an edit: no undo step, not saved, and the model is
  not changed. Graphics export draws what is shown, highlight included, so a
  figure of a null-space vector can be made directly (D3).

## 7. Testing

- Unit tests on small networks whose answers are known by hand: a linear chain; a
  branch point (A → B, B → C, B → D); a cycle with a moiety (ATP/ADP);
  stoichiometry 2; an alias in two reactions; boundary species, constants, rule
  species and null nodes left out; an empty model; a reaction with no floating
  species (a zero column).
- Breaking it on purpose, as usual.
- **Against libRoadRunner:** for every Gallery model and a set of BioModels, the
  stoichiometry matrix is the same (rows and columns matched by id), the rank is
  the same, the null-space basis spans the same space as `getKMatrix`, and the
  conservation laws span the same space as `getConservationMatrix`. A script in
  `scripts/`, run by hand and reported in the evaluation log.
- The browser, with the MAPK cascade (three conservation laws) and the Millard
  model.

## 8. Decisions

**Settled (author, 2026-10-21):** the method is COPASI's: QR with column pivoting
(section 3). This also settles which species and reactions are independent (the
pivoting chooses, as in COPASI and libRoadRunner) and large networks (floating
point handles them).

**D1. Choosing an independent reaction.** Later, optionally: the user marks a
reaction to be independent (it is moved last before factorizing, so the pivoting
keeps it out of the dependent set where it can), to ask "if I change this enzyme,
what else must change?". Not in the first version.

**D4. The size limit (decided 2026-10-21).** The tab declines networks above
about 600 × 600 (`WORK_LIMIT`): measured, 366 × 340 takes 91 ms natively and
207 ms in the browser, so 600 × 600 is some 1.5 s in the browser, the most to
wait for while the analysis runs in the foreground.

**D2. Exported figures.**
- (a) Graphics export draws the highlight as shown *(recommended)*: the point of
  the feature is a picture.
- (b) Export never shows it.

**D3. Colours.** One highlight colour for positive and another for negative
coefficients, fixed, chosen to read in colour-blind vision (blue and orange)
*(recommended)*; or taken from the theme.

## 9. Steps

1. The matrix and the QR factorization in `pd-core` (`structure.rs`): `N`, rank,
   link and conservation matrices, K matrix; unit tests.
2. The check against libRoadRunner on the Gallery and BioModels (and Recon3D, for
   size and time, natively and in the browser); evaluation log entry.
3. The Structure tab: summary, matrix table, lists, copy and export.
4. Highlighting on the network, on screen and in graphics export.
5. Docs: SPECIFICATION, README, and the paper's Methods, Results (a figure of a
   null-space vector and a conservation law on the MAPK or Millard model) and
   Discussion.
