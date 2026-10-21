//! The structure of a reaction network (docs/structural-analysis-design.md):
//! its stoichiometry matrix `N`, the conservation laws (the left null space,
//! through the link matrix) and a basis of the flux null space (the K matrix).
//!
//! Computed as COPASI and libStructural compute them (Vallabhajosyula,
//! Chickarmane and Sauro, Bioinformatics 2006): a QR factorization with column
//! pivoting by the largest remaining column norm, as LAPACK's `DGEQP3` does,
//! the rank from COPASI's tolerance, and the triangular factor inverted. Our
//! own code: LAPACK is not available in the browser, and nalgebra's
//! `ColPivQR` pivots on the largest entry, not the largest column norm.

use crate::model::{Id, Model};

/// A dense matrix, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    data: Vec<f64>,
}

impl Matrix {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    pub fn get(&self, r: usize, c: usize) -> f64 {
        self.data[r * self.cols + c]
    }

    pub fn set(&mut self, r: usize, c: usize, v: f64) {
        self.data[r * self.cols + c] = v;
    }

    fn add(&mut self, r: usize, c: usize, v: f64) {
        self.data[r * self.cols + c] += v;
    }

    pub fn transpose(&self) -> Self {
        let mut t = Self::zeros(self.cols, self.rows);
        for r in 0..self.rows {
            for c in 0..self.cols {
                t.set(c, r, self.get(r, c));
            }
        }
        t
    }

    fn swap_cols(&mut self, a: usize, b: usize) {
        if a != b {
            for r in 0..self.rows {
                self.data.swap(r * self.cols + a, r * self.cols + b);
            }
        }
    }
}

/// The upper triangular factor of `A P = Q R`, the column permutation, and
/// the rank.
#[derive(Clone, Debug)]
pub struct PivotedQr {
    /// R, `rows × cols` (below the diagonal zero).
    pub r: Matrix,
    /// `perm[k]` is the column of A that is column `k` of `A P`.
    pub perm: Vec<usize>,
    pub rank: usize,
}

/// QR with column pivoting, as `DGEQP3`: at each step the remaining column
/// with the largest norm (over the rows not yet reduced) comes next, ties to
/// the column first in A, and a Householder reflection clears it below the
/// diagonal. The rank is the
/// number of diagonal entries of R larger than
/// `max(rows, cols) × |R₁₁| × ε`, COPASI's tolerance.
pub fn pivoted_qr(a: &Matrix) -> PivotedQr {
    let (m, n) = (a.rows, a.cols);
    let mut r = a.clone();
    let mut perm: Vec<usize> = (0..n).collect();
    let steps = m.min(n);
    for k in 0..steps {
        // The column with the largest norm over rows k.., computed afresh
        // (no downdating), so ties and tiny norms are judged exactly.
        let norm2 = |r: &Matrix, c: usize| (k..m).map(|i| r.get(i, c).powi(2)).sum::<f64>();
        // Columns whose norms are equal but for rounding tie, and a tie goes
        // to the column that comes first in A (the species or reaction
        // listed first), whatever the swaps so far: a choice that is stable
        // and can be explained, where DGEQP3's depends on its rounding.
        let norms: Vec<f64> = (k..n).map(|c| norm2(&r, c)).collect();
        let largest = norms.iter().copied().fold(0.0, f64::max);
        let tie = largest * (1.0 - 1e-12);
        let best = (k..n)
            .filter(|&c| norms[c - k] >= tie)
            .min_by_key(|&c| perm[c])
            .unwrap_or(k);
        let best_norm = norms[best - k];
        r.swap_cols(k, best);
        perm.swap(k, best);
        if best_norm == 0.0 {
            // The rest is zero: nothing more to reduce.
            continue;
        }
        // Householder: v = x + sign(x₀)‖x‖e₀, H = I − 2vvᵀ/vᵀv.
        let alpha = best_norm.sqrt();
        let x0 = r.get(k, k);
        let s = if x0 >= 0.0 { 1.0 } else { -1.0 };
        let mut v: Vec<f64> = (k..m).map(|i| r.get(i, k)).collect();
        v[0] += s * alpha;
        let vtv: f64 = v.iter().map(|x| x * x).sum();
        if vtv == 0.0 {
            continue;
        }
        for c in k..n {
            let dot: f64 = (k..m).map(|i| v[i - k] * r.get(i, c)).sum();
            let f = 2.0 * dot / vtv;
            for i in k..m {
                r.add(i, c, -f * v[i - k]);
            }
        }
        // Exactly what the reflection makes it.
        r.set(k, k, -s * alpha);
        for i in k + 1..m {
            r.set(i, k, 0.0);
        }
    }
    let largest = if steps > 0 { r.get(0, 0).abs() } else { 0.0 };
    let tol = m.max(n) as f64 * largest * f64::EPSILON;
    let rank = (0..steps)
        .take_while(|&i| largest > 0.0 && r.get(i, i).abs() > tol)
        .count();
    PivotedQr { r, perm, rank }
}

/// `R₁₁⁻¹ R₁₂` for the first `rank` rows of R: back substitution, column by
/// column. `rank × (cols − rank)`.
fn r11_inv_r12(qr: &PivotedQr) -> Matrix {
    let (k, n) = (qr.rank, qr.r.cols);
    let mut x = Matrix::zeros(k, n - k);
    for c in 0..n - k {
        for i in (0..k).rev() {
            let mut v = qr.r.get(i, k + c);
            for j in i + 1..k {
                v -= qr.r.get(i, j) * x.get(j, c);
            }
            x.set(i, c, v / qr.r.get(i, i));
        }
    }
    x
}

/// A conservation law: `Σ coefficient × species = constant`, its species as
/// row indices into [`Structure::species`], the dependent species first.
#[derive(Clone, Debug, PartialEq)]
pub struct ConservationLaw {
    pub terms: Vec<(usize, f64)>,
}

/// A basis vector of the flux null space: the fluxes, as column indices into
/// [`Structure::reactions`], that can change together while every species
/// stays balanced (`N v = 0`); its independent reaction first, at 1.
#[derive(Clone, Debug, PartialEq)]
pub struct FluxVector {
    pub independent: usize,
    pub terms: Vec<(usize, f64)>,
}

/// The structure of a model's reaction network.
#[derive(Clone, Debug)]
pub struct Structure {
    /// The rows of N: the floating species (not boundary, not constant, not
    /// set by an assignment rule, not null nodes), primaries only, in the
    /// model's order.
    pub species: Vec<Id>,
    /// The columns of N: every reaction, in the model's order.
    pub reactions: Vec<Id>,
    /// The stoichiometry matrix: products minus reactants, aliases counted as
    /// their primary, modifiers not at all.
    pub n: Matrix,
    pub rank: usize,
    /// Species indices: the independent ones and the dependent ones, in the
    /// order the pivoting chose.
    pub independent_species: Vec<usize>,
    pub dependent_species: Vec<usize>,
    /// The link matrix's lower part: dependent species (rows) in terms of the
    /// independent ones (columns), `N_dep = L₀ N_indep`.
    pub l0: Matrix,
    /// Reaction indices: dependent and independent fluxes.
    pub dependent_reactions: Vec<usize>,
    pub independent_reactions: Vec<usize>,
    /// `K₀`: the dependent fluxes (rows) in terms of the independent ones
    /// (columns), `K = [K₀; I]`.
    pub k0: Matrix,
}

impl Structure {
    /// The structure of `model`.
    pub fn of(model: &Model) -> Self {
        let (species, reactions, n) = stoichiometry(model);
        Self::from_matrix(species, reactions, n)
    }

    /// The structure of the stoichiometry matrix `n`, rows `species`,
    /// columns `reactions`.
    pub fn from_matrix(species: Vec<Id>, reactions: Vec<Id>, n: Matrix) -> Self {
        Self::factorize(species, reactions, n)
    }
}

/// A model's stoichiometry matrix: its rows (the floating species, see
/// [`Structure::species`]), its columns (every reaction) and its entries,
/// products minus reactants, aliases counted as their primary, modifiers not
/// at all. Cheap: the factorization is the costly part, so a view can build
/// this every frame and factorize only when it changes.
pub fn stoichiometry(model: &Model) -> (Vec<Id>, Vec<Id>, Matrix) {
    {
        let ruled: std::collections::HashSet<&str> = model
            .assignment_rules
            .iter()
            .map(|r| r.variable.as_str())
            .collect();
        let species: Vec<Id> = model
            .species()
            .iter()
            .filter(|s| {
                !s.is_alias()
                    && !s.is_null_node
                    && !s.is_boundary
                    && !s.is_constant
                    && !ruled.contains(s.id.as_str())
            })
            .map(|s| s.id.clone())
            .collect();
        let row: std::collections::HashMap<&str, usize> = species
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        let reactions: Vec<Id> = model.reactions().iter().map(|r| r.id.clone()).collect();
        let mut n = Matrix::zeros(species.len(), reactions.len());
        for (c, r) in model.reactions().iter().enumerate() {
            for (list, sign) in [(&r.reactants, -1.0), (&r.products, 1.0)] {
                for p in list {
                    let primary = model
                        .find_species(&p.species)
                        .map_or(p.species.as_str(), |s| s.primary_id());
                    if let Some(&i) = row.get(primary) {
                        n.add(i, c, sign * p.stoichiometry);
                    }
                }
            }
        }
        (species, reactions, n)
    }
}

/// How much work factorizing an `rows × cols` matrix is, in multiply-adds,
/// roughly (`rows × cols × min`, for each of the two factorizations).
pub fn work(rows: usize, cols: usize) -> f64 {
    2.0 * rows as f64 * cols as f64 * rows.min(cols) as f64
}

/// The most work the Structure tab does (docs/structural-analysis-design.md
/// D4, author 2026-10-21): about a 600 × 600 network, some 0.6 s natively and
/// 1.5 s in the browser (366 × 340 measured at 91 and 207 ms), since the
/// analysis runs while the user waits. Larger networks would need a sparse
/// factorization.
pub const WORK_LIMIT: f64 = 4.5e8;

impl Structure {
    fn factorize(species: Vec<Id>, reactions: Vec<Id>, n: Matrix) -> Self {
        // Conservation: Nᵀ P = Q [R₁₁ R₁₂] over the species; the pivot
        // columns are the independent species, and N_dep = (R₁₁⁻¹R₁₂)ᵀ N_indep.
        let qr_t = pivoted_qr(&n.transpose());
        let rank = qr_t.rank;
        let l0 = r11_inv_r12(&qr_t).transpose();
        let independent_species = qr_t.perm[..rank].to_vec();
        let dependent_species = qr_t.perm[rank..].to_vec();
        // The null space: N P = Q [R₁₁ R₁₂] over the reactions; the pivot
        // columns are the dependent fluxes, and K₀ = −R₁₁⁻¹R₁₂.
        let qr = pivoted_qr(&n);
        let mut k0 = r11_inv_r12(&qr);
        for v in &mut k0.data {
            *v = -*v;
        }
        let dependent_reactions = qr.perm[..qr.rank].to_vec();
        let independent_reactions = qr.perm[qr.rank..].to_vec();
        debug_assert!(n.rows == 0 || n.cols == 0 || qr.rank == rank);
        Self {
            species,
            reactions,
            n,
            rank,
            independent_species,
            dependent_species,
            l0,
            dependent_reactions,
            independent_reactions,
            k0,
        }
    }

    /// The conservation laws, one per dependent species: that species, then
    /// the independent species it is tied to with minus their `L₀`
    /// coefficients (`x_d − Σ L₀ x_i = constant`). Entries within [`TINY`]
    /// of zero are left out.
    pub fn conservation_laws(&self) -> Vec<ConservationLaw> {
        self.dependent_species
            .iter()
            .enumerate()
            .map(|(d, &dep)| {
                let mut terms = vec![(dep, 1.0)];
                for (i, &ind) in self.independent_species.iter().enumerate() {
                    let v = -self.l0.get(d, i);
                    if v.abs() > TINY {
                        terms.push((ind, v));
                    }
                }
                ConservationLaw { terms }
            })
            .collect()
    }

    /// A basis of the flux null space, one vector per independent reaction:
    /// it at 1, the dependent fluxes that must follow with their `K₀`
    /// coefficients. Entries within [`TINY`] of zero are left out.
    pub fn null_space(&self) -> Vec<FluxVector> {
        self.independent_reactions
            .iter()
            .enumerate()
            .map(|(i, &ind)| {
                let mut terms = vec![(ind, 1.0)];
                for (d, &dep) in self.dependent_reactions.iter().enumerate() {
                    let v = self.k0.get(d, i);
                    if v.abs() > TINY {
                        terms.push((dep, v));
                    }
                }
                FluxVector {
                    independent: ind,
                    terms,
                }
            })
            .collect()
    }
}

impl Structure {
    /// A null-space vector as text: its independent reaction first, the
    /// others in the model's order; in the smallest whole numbers of its
    /// proportions when they are simple fractions, else with the independent
    /// reaction at 1 and the rest to four significant figures:
    /// `J1 + J0`, `2 J1 + J0`, `PTA + 0.0795 PGI − 0.0909 GLT`.
    pub fn flux_text(&self, v: &FluxVector) -> String {
        terms_text(&v.terms, Some(v.independent), |c| {
            self.reactions[c].as_str()
        })
    }

    /// A conservation law as text, its species in the model's order:
    /// `ATP + ADP = constant`, `A + 2 B = constant`.
    pub fn law_text(&self, law: &ConservationLaw) -> String {
        format!(
            "{} = constant",
            terms_text(&law.terms, None, |i| self.species[i].as_str())
        )
    }

    /// The stoichiometry matrix as CSV: a header of reaction ids, then one
    /// row per species, its id first.
    pub fn n_csv(&self) -> String {
        let mut out = csv_row(std::iter::once(String::new()).chain(self.reactions.iter().cloned()));
        for (i, sp) in self.species.iter().enumerate() {
            out += &csv_row(
                std::iter::once(sp.clone())
                    .chain((0..self.n.cols).map(|c| number(self.n.get(i, c)))),
            );
        }
        out
    }

    /// The stoichiometry matrix tab-separated, as a spreadsheet pastes it: a
    /// header of reaction ids, then one row per species.
    pub fn n_tsv(&self) -> String {
        let row = |fields: Vec<String>| fields.join("\t") + "\n";
        let mut out = row(std::iter::once(String::new())
            .chain(self.reactions.iter().cloned())
            .collect());
        for (i, sp) in self.species.iter().enumerate() {
            out += &row(std::iter::once(sp.clone())
                .chain((0..self.n.cols).map(|c| number(self.n.get(i, c))))
                .collect());
        }
        out
    }

    /// The null-space basis as CSV: one column per vector, named by its
    /// independent reaction; one row per reaction, in the model's order.
    pub fn null_space_csv(&self) -> String {
        let basis = self.null_space();
        let mut out = csv_row(
            std::iter::once(String::new())
                .chain(basis.iter().map(|v| self.reactions[v.independent].clone())),
        );
        for (c, rx) in self.reactions.iter().enumerate() {
            out += &csv_row(
                std::iter::once(rx.clone()).chain(
                    basis
                        .iter()
                        .map(|v| number(v.terms.iter().find(|t| t.0 == c).map_or(0.0, |t| t.1))),
                ),
            );
        }
        out
    }

    /// The conservation laws as CSV: one row per law, named by its dependent
    /// species; one column per species, in the model's order.
    pub fn laws_csv(&self) -> String {
        let laws = self.conservation_laws();
        let mut out = csv_row(std::iter::once(String::new()).chain(self.species.iter().cloned()));
        for (law, &dep) in laws.iter().zip(&self.dependent_species) {
            out += &csv_row(
                std::iter::once(self.species[dep].clone()).chain(
                    (0..self.species.len())
                        .map(|i| number(law.terms.iter().find(|t| t.0 == i).map_or(0.0, |t| t.1))),
                ),
            );
        }
        out
    }
}

/// A sum of terms `(index, coefficient)` as text, by `name`: the term for
/// `lead` first (if given), the rest in index order. Whole numbers when the
/// proportions allow; otherwise scaled so the first term is 1, to four
/// significant figures. The first coefficient is positive.
fn terms_text<'a>(
    terms: &[(usize, f64)],
    lead: Option<usize>,
    name: impl Fn(usize) -> &'a str,
) -> String {
    let mut terms = terms.to_vec();
    terms.sort_by_key(|t| (Some(t.0) != lead, t.0));
    let values: Vec<f64> = terms.iter().map(|t| t.1).collect();
    let flip = values.first().is_some_and(|&v| v < 0.0);
    let shown: Vec<String> = match smallest_integers(&values) {
        Some(ints) => ints
            .iter()
            .map(|&k| if flip { -k } else { k })
            .map(|k| k.to_string())
            .collect(),
        None => {
            let first = values.first().copied().unwrap_or(1.0);
            values.iter().map(|&v| significant(v / first)).collect()
        }
    };
    let mut out = String::new();
    for (i, ((index, _), k)) in terms.iter().zip(&shown).enumerate() {
        let (negative, magnitude) = match k.strip_prefix('-') {
            Some(m) => (true, m),
            None => (false, k.as_str()),
        };
        match (i, negative) {
            (0, true) => out.push('−'),
            (0, false) => {}
            (_, true) => out.push_str(" − "),
            (_, false) => out.push_str(" + "),
        }
        if magnitude != "1" {
            out.push_str(magnitude);
            out.push(' ');
        }
        out.push_str(name(*index));
    }
    out
}

/// `v` to four significant figures, without trailing zeros: `0.0795`,
/// `1.25`, `1`.
fn significant(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    let digits = (3 - v.abs().log10().floor() as i32).max(0) as usize;
    let text = format!("{v:.digits$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// A number for CSV: cleaned as for display (within [`TINY`] of a simple
/// fraction, that fraction's value, so a 1 the factorization left as
/// 0.9999999999999999 is 1), then as Rust writes it, the shortest that reads
/// back the same; negative zero as 0.
fn number(v: f64) -> String {
    let v = as_fraction(v).map_or(v, |(n, d)| n as f64 / d as f64);
    if v == 0.0 { "0".into() } else { format!("{v}") }
}

/// One CSV line; a field with a comma, quote or line break is quoted.
fn csv_row(fields: impl Iterator<Item = String>) -> String {
    let quoted: Vec<String> = fields
        .map(|f| {
            if f.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", f.replace('"', "\"\""))
            } else {
                f
            }
        })
        .collect();
    quoted.join(",") + "\n"
}

/// Entries this close to zero are zero, as COPASI clears entries below
/// 100 ε after inverting R; a little wider, since the entries have been
/// through a factorization and a solve.
pub const TINY: f64 = 1e-9;

/// `x` as a fraction with a small denominator (up to 12), if it is within
/// [`TINY`] of one: `(numerator, denominator)`, denominator positive.
pub fn as_fraction(x: f64) -> Option<(i64, i64)> {
    if !x.is_finite() {
        return None;
    }
    (1..=12_i64).find_map(|d| {
        let n = (x * d as f64).round();
        ((x * d as f64 - n).abs() <= TINY * d as f64 && n.abs() < 1e12).then(|| reduce(n as i64, d))
    })
}

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn reduce(n: i64, d: i64) -> (i64, i64) {
    let g = gcd(n, d).max(1);
    (n / g, d / g)
}

/// The smallest integers in the proportions of `values`, keeping their
/// signs, if every value is a simple fraction ([`as_fraction`]); `None`
/// otherwise.
pub fn smallest_integers(values: &[f64]) -> Option<Vec<i64>> {
    let fractions: Vec<(i64, i64)> = values
        .iter()
        .map(|&v| as_fraction(v))
        .collect::<Option<_>>()?;
    let lcm = fractions.iter().fold(1_i64, |l, &(_, d)| l / gcd(l, d) * d);
    let ints: Vec<i64> = fractions.iter().map(|&(n, d)| n * (lcm / d)).collect();
    let g = ints.iter().fold(0, |g, &v| gcd(g, v));
    Some(if g > 1 {
        ints.iter().map(|v| v / g).collect()
    } else {
        ints
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::pt;
    use crate::model::{Participant, Reaction, SpeciesNode};

    /// A model from reactions written as (id, [(reactant, stoich)],
    /// [(product, stoich)]); species made as they first appear.
    /// Species and their stoichiometries on one side of a reaction.
    type Side<'a> = &'a [(&'a str, f64)];

    fn model(reactions: &[(&str, Side, Side)]) -> Model {
        let mut m = Model::new();
        for (id, ins, outs) in reactions {
            for (s, _) in ins.iter().chain(outs.iter()) {
                if m.find_species(s).is_none() {
                    m.push_species(SpeciesNode::new(*s, pt(0.0, 0.0))).unwrap();
                }
            }
            let mut r = Reaction::new(*id, pt(0.0, 0.0));
            r.reactants = ins.iter().map(|(s, k)| Participant::new(*s, *k)).collect();
            r.products = outs.iter().map(|(s, k)| Participant::new(*s, *k)).collect();
            m.push_reaction(r).unwrap();
        }
        m
    }

    /// `N v` for each null-space vector, and `Γ N` for each law: all zero.
    fn check_exact(s: &Structure) {
        for v in s.null_space() {
            for i in 0..s.n.rows {
                let sum: f64 = v.terms.iter().map(|&(c, k)| s.n.get(i, c) * k).sum();
                assert!(sum.abs() < 1e-9, "N v ≠ 0 for {v:?}");
            }
        }
        for law in s.conservation_laws() {
            for c in 0..s.n.cols {
                let sum: f64 = law.terms.iter().map(|&(i, k)| s.n.get(i, c) * k).sum();
                assert!(sum.abs() < 1e-9, "Γ N ≠ 0 for {law:?}");
            }
        }
        assert_eq!(s.independent_reactions.len(), s.n.cols - s.rank);
        assert_eq!(s.dependent_species.len(), s.n.rows - s.rank);
    }

    #[test]
    fn the_matrix_is_products_minus_reactants() {
        // X → A (boundary X), A + A → B, B → A, B → Y (boundary Y).
        let mut m = model(&[
            ("J0", &[("X", 1.0)], &[("A", 1.0)]),
            ("J1", &[("A", 2.0)], &[("B", 1.0)]),
            ("J2", &[("B", 1.0)], &[("A", 1.0)]),
            ("J3", &[("B", 1.0)], &[("Y", 1.0)]),
        ]);
        for id in ["X", "Y"] {
            m.find_species_mut(id).unwrap().is_boundary = true;
        }
        let s = Structure::of(&m);
        assert_eq!(s.species, ["A", "B"]);
        assert_eq!(s.reactions, ["J0", "J1", "J2", "J3"]);
        let row = |i: usize| (0..4).map(|c| s.n.get(i, c)).collect::<Vec<_>>();
        assert_eq!(row(0), [1.0, -2.0, 1.0, 0.0]);
        assert_eq!(row(1), [0.0, 1.0, -1.0, -1.0]);
    }

    #[test]
    fn aliases_count_as_their_primary_and_others_have_no_row() {
        // A → B, B → C, with B's second reaction drawn to an alias; C is
        // constant, D set by a rule, and a null node is a sink.
        let mut m = model(&[
            ("J1", &[("A", 1.0)], &[("B", 1.0)]),
            ("J2", &[("B", 1.0)], &[("C", 1.0)]),
        ]);
        let alias = m.add_alias("B", pt(10.0, 10.0)).unwrap();
        m.find_reaction_mut("J2").unwrap().reactants[0].species = alias;
        m.find_species_mut("C").unwrap().is_constant = true;
        m.push_species(SpeciesNode::new("D", pt(0.0, 0.0))).unwrap();
        m.assignment_rules.push(crate::model::AssignmentRule {
            variable: "D".into(),
            expression: "2*A".into(),
        });
        let mut sink = SpeciesNode::new("_sink", pt(0.0, 0.0));
        sink.is_null_node = true;
        m.push_species(sink).unwrap();
        m.find_reaction_mut("J2")
            .unwrap()
            .products
            .push(Participant::new("_sink", 1.0));
        let s = Structure::of(&m);
        assert_eq!(s.species, ["A", "B"]);
        assert_eq!(
            (s.n.get(1, 0), s.n.get(1, 1)),
            (1.0, -1.0),
            "B's two entries"
        );
    }

    #[test]
    fn a_branch_point_has_one_pathway_per_branch() {
        // X → A, A → Y, A → Z: one species, three reactions; rank 1, two
        // independent fluxes; J0 = J1 + J2.
        let mut m = model(&[
            ("J0", &[("X", 1.0)], &[("A", 1.0)]),
            ("J1", &[("A", 1.0)], &[("Y", 1.0)]),
            ("J2", &[("A", 1.0)], &[("Z", 1.0)]),
        ]);
        for id in ["X", "Y", "Z"] {
            m.find_species_mut(id).unwrap().is_boundary = true;
        }
        let s = Structure::of(&m);
        assert_eq!(s.rank, 1);
        assert!(s.conservation_laws().is_empty());
        let basis = s.null_space();
        assert_eq!(basis.len(), 2);
        check_exact(&s);
        // Each vector: its independent branch at 1 and J0 at 1 too.
        for v in &basis {
            let j0 = v.terms.iter().find(|t| s.reactions[t.0] == "J0");
            let ints = smallest_integers(&v.terms.iter().map(|t| t.1).collect::<Vec<_>>());
            assert!(ints.unwrap().iter().all(|&k| k == 1), "{v:?}");
            assert!(j0.is_some() || s.reactions[v.independent] == "J0");
        }
    }

    #[test]
    fn a_moiety_cycle_has_one_conservation_law() {
        // ATP → ADP (J1), ADP → ATP (J2), and S → P using ATP (J3: S + ATP
        // → P + ADP), S and P boundary: ATP + ADP is conserved.
        let mut m = model(&[
            ("J1", &[("ATP", 1.0)], &[("ADP", 1.0)]),
            ("J2", &[("ADP", 1.0)], &[("ATP", 1.0)]),
            (
                "J3",
                &[("S", 1.0), ("ATP", 1.0)],
                &[("P", 1.0), ("ADP", 1.0)],
            ),
        ]);
        for id in ["S", "P"] {
            m.find_species_mut(id).unwrap().is_boundary = true;
        }
        let s = Structure::of(&m);
        assert_eq!(s.rank, 1);
        let laws = s.conservation_laws();
        assert_eq!(laws.len(), 1);
        let mut named: Vec<(String, i64)> = laws[0]
            .terms
            .iter()
            .map(|&(i, k)| (s.species[i].clone(), k.round() as i64))
            .collect();
        named.sort();
        assert_eq!(named, [("ADP".to_string(), 1), ("ATP".to_string(), 1)]);
        // Two independent fluxes of three: the cycle J1 + J2, and J3 with J2.
        assert_eq!(s.null_space().len(), 2);
        check_exact(&s);
    }

    #[test]
    fn stoichiometry_two_gives_fractions_and_integers() {
        // X → 2 A, A → Y: J0 drives twice as much A, so J1 = 2 J0.
        let mut m = model(&[
            ("J0", &[("X", 1.0)], &[("A", 2.0)]),
            ("J1", &[("A", 1.0)], &[("Y", 1.0)]),
        ]);
        for id in ["X", "Y"] {
            m.find_species_mut(id).unwrap().is_boundary = true;
        }
        let s = Structure::of(&m);
        let basis = s.null_space();
        assert_eq!(basis.len(), 1);
        check_exact(&s);
        let v = &basis[0];
        let mut by_name: Vec<(String, f64)> = v
            .terms
            .iter()
            .map(|&(c, k)| (s.reactions[c].clone(), k))
            .collect();
        by_name.sort_by(|a, b| a.0.cmp(&b.0));
        let ints = smallest_integers(&by_name.iter().map(|t| t.1).collect::<Vec<_>>()).unwrap();
        assert_eq!(ints, [1, 2], "J0 : J1 = 1 : 2");
    }

    #[test]
    fn a_closed_chain_conserves_its_total() {
        // A → B → C → A, nothing in or out: A + B + C conserved; one
        // pathway, the cycle.
        let m = model(&[
            ("J1", &[("A", 1.0)], &[("B", 1.0)]),
            ("J2", &[("B", 1.0)], &[("C", 1.0)]),
            ("J3", &[("C", 1.0)], &[("A", 1.0)]),
        ]);
        let s = Structure::of(&m);
        assert_eq!(s.rank, 2);
        let laws = s.conservation_laws();
        assert_eq!(laws.len(), 1);
        assert_eq!(laws[0].terms.len(), 3);
        assert!(laws[0].terms.iter().all(|t| (t.1 - 1.0).abs() < 1e-12));
        let basis = s.null_space();
        assert_eq!(basis.len(), 1);
        assert_eq!(basis[0].terms.len(), 3);
        check_exact(&s);
    }

    #[test]
    fn the_pivoting_takes_the_largest_column_first() {
        // DGEQP3's choice: the column with the largest norm, so a column of
        // 3s comes before a column of 1s.
        let mut a = Matrix::zeros(2, 2);
        for (r, c, v) in [(0, 0, 1.0), (1, 0, 1.0), (0, 1, 3.0), (1, 1, -3.0)] {
            a.set(r, c, v);
        }
        let qr = pivoted_qr(&a);
        assert_eq!(qr.perm, [1, 0]);
        assert_eq!(qr.rank, 2);
        // |R₁₁| is that column's norm.
        assert!((qr.r.get(0, 0).abs() - 18f64.sqrt()).abs() < 1e-12);
        // A rank-deficient matrix: rank 1.
        let mut b = Matrix::zeros(3, 3);
        for c in 0..3 {
            for r in 0..3 {
                b.set(r, c, (c + 1) as f64 * (r + 1) as f64);
            }
        }
        assert_eq!(pivoted_qr(&b).rank, 1);
    }

    #[test]
    fn ties_go_to_the_species_and_reaction_listed_first() {
        // ATP ⇄ ADP: two species of equal norm. Listed ATP first, ATP is
        // independent; listed ADP first, ADP is.
        for (first, second) in [("ATP", "ADP"), ("ADP", "ATP")] {
            let mut m = Model::new();
            for id in [first, second] {
                m.push_species(SpeciesNode::new(id, pt(0.0, 0.0))).unwrap();
            }
            for (id, a, b) in [("J1", "ATP", "ADP"), ("J2", "ADP", "ATP")] {
                let mut r = Reaction::new(id, pt(0.0, 0.0));
                r.reactants = vec![Participant::new(a, 1.0)];
                r.products = vec![Participant::new(b, 1.0)];
                m.push_reaction(r).unwrap();
            }
            let s = Structure::of(&m);
            let name = |i: usize| s.species[i].as_str();
            assert_eq!(name(s.independent_species[0]), first);
            assert_eq!(name(s.dependent_species[0]), second);
            // J1 and J2 tie too: J1 comes first, so it is the dependent one
            // the pivoting takes, and J2 the independent flux.
            assert_eq!(s.reactions[s.independent_reactions[0]], "J2");
        }
    }

    #[test]
    fn empty_and_zero_networks() {
        let s = Structure::of(&Model::new());
        assert_eq!(
            (s.rank, s.null_space().len(), s.conservation_laws().len()),
            (0, 0, 0)
        );
        // A species in no reaction is conserved on its own; a reaction with
        // no floating species is free.
        let mut m = model(&[("J1", &[("X", 1.0)], &[("Y", 1.0)])]);
        for id in ["X", "Y"] {
            m.find_species_mut(id).unwrap().is_boundary = true;
        }
        m.push_species(SpeciesNode::new("Z", pt(0.0, 0.0))).unwrap();
        let s = Structure::of(&m);
        assert_eq!(s.rank, 0);
        assert_eq!(
            s.conservation_laws(),
            [ConservationLaw {
                terms: vec![(0, 1.0)]
            }]
        );
        let basis = s.null_space();
        assert_eq!(basis.len(), 1);
        assert_eq!(basis[0].terms, [(0, 1.0)]);
    }

    #[test]
    fn vectors_and_laws_read_as_equations() {
        // X → 2 A (J0), A → Y (J1), A → Z (J2): J0's basis vector is
        // J0 + 2 J1 (or with J2), the other J2 − J1.
        let mut m = model(&[
            ("J0", &[("X", 1.0)], &[("A", 2.0)]),
            ("J1", &[("A", 1.0)], &[("Y", 1.0)]),
            ("J2", &[("A", 1.0)], &[("Z", 1.0)]),
        ]);
        for id in ["X", "Y", "Z"] {
            m.find_species_mut(id).unwrap().is_boundary = true;
        }
        let s = Structure::of(&m);
        // J0 has the largest column (2), so it is the dependent flux; each
        // vector is led by its independent branch, in whole numbers.
        let texts: Vec<String> = s.null_space().iter().map(|v| s.flux_text(v)).collect();
        assert_eq!(texts, ["2 J1 + J0", "2 J2 + J0"]);
        // A law, whole numbers, species in the model's order.
        let m = model(&[
            ("J1", &[("A", 2.0)], &[("B", 1.0)]),
            ("J2", &[("B", 1.0)], &[("A", 2.0)]),
        ]);
        let s = Structure::of(&m);
        let laws = s.conservation_laws();
        assert_eq!(s.law_text(&laws[0]), "A + 2 B = constant");
        // Not simple fractions: the first at 1, four significant figures.
        let pq = |i: usize| ["P", "Q", "R"][i];
        let terms = [(0, 1.0), (1, 0.123456), (2, -0.0795)];
        assert_eq!(terms_text(&terms, None, pq), "P + 0.1235 Q − 0.0795 R");
        // The lead first, whatever its index.
        assert_eq!(terms_text(&terms, Some(2), pq), "R − 12.58 P − 1.553 Q");
        // The first coefficient is made positive.
        assert_eq!(terms_text(&[(0, -1.0), (1, 1.0)], None, pq), "P − Q");
        assert_eq!(significant(2.5), "2.5");
        assert_eq!(significant(1.0), "1");
        assert_eq!(significant(-0.000123456), "-0.0001235");
    }

    #[test]
    fn matrices_export_as_csv() {
        let m = model(&[
            ("J1", &[("A", 1.0)], &[("B", 1.0)]),
            ("J2", &[("B", 1.0)], &[("A", 1.0)]),
        ]);
        let s = Structure::of(&m);
        assert_eq!(s.n_csv(), ",J1,J2\nA,-1,1\nB,1,-1\n");
        assert_eq!(s.n_tsv(), "\tJ1\tJ2\nA\t-1\t1\nB\t1\t-1\n");
        let k = s.null_space_csv();
        let lines: Vec<&str> = k.lines().collect();
        assert_eq!(lines.len(), 3, "{k}");
        assert!(
            lines[1].starts_with("J1,1") && lines[2].starts_with("J2,1"),
            "{k}"
        );
        let l = s.laws_csv();
        assert_eq!(l.lines().count(), 2, "{l}");
        assert!(l.lines().nth(1).unwrap().ends_with(",1,1"), "{l}");
        // A name with a comma is quoted.
        assert_eq!(
            csv_row(["a,b".to_string(), "c\"d".to_string()].into_iter()),
            "\"a,b\",\"c\"\"d\"\n"
        );
    }

    #[test]
    fn the_limit_is_measured_in_work() {
        assert!(work(600, 600) <= WORK_LIMIT);
        assert!(work(1000, 1000) > WORK_LIMIT);
        assert!(work(5800, 10600) > WORK_LIMIT);
        assert_eq!(work(0, 50), 0.0);
    }

    #[test]
    fn simple_fractions_are_recognised() {
        assert_eq!(as_fraction(0.5), Some((1, 2)));
        assert_eq!(as_fraction(-2.0), Some((-2, 1)));
        assert_eq!(as_fraction(1.0 / 3.0 + 1e-12), Some((1, 3)));
        assert_eq!(as_fraction(0.123456), None);
        assert_eq!(smallest_integers(&[0.5, 1.0, -1.5]), Some(vec![1, 2, -3]));
        assert_eq!(smallest_integers(&[2.0, 4.0]), Some(vec![1, 2]));
        assert_eq!(smallest_integers(&[0.1234567, 1.0]), None);
    }
}
