//! The random network generator, a test tool (S §7.7).

use crate::geometry::pt;
use crate::layout::Rng;
use crate::model::{Model, Parameter, Participant, ReactionMode};

const SPECIES: usize = 10;
const COLUMNS: usize = 5;
const EXTRA_REACTIONS: usize = 3;
/// Grid spacing and top-left corner for the species.
const GRID_DX: f64 = 160.0;
const GRID_DY: f64 = 120.0;
const GRID_ORIGIN: f64 = 100.0;

/// A new model: 10 species on a 5-column grid with initial values in [1, 5],
/// a backbone chain S1 → S2 → … → S10 of Linear UniUni reactions, and 3 more
/// reactions (12 in all), each with a 20 % chance of being BiUni and a 20 %
/// chance of being UniBi. Every reaction gets a mass-action law `k<N>*…` and
/// a parameter `k<N> = 0.1`.
pub fn random_network(seed: u64) -> Model {
    let mut rng = Rng::new(seed);
    let mut m = Model::new();
    let ids: Vec<String> = (0..SPECIES)
        .map(|i| {
            let at = pt(
                GRID_ORIGIN + (i % COLUMNS) as f64 * GRID_DX,
                GRID_ORIGIN + (i / COLUMNS) as f64 * GRID_DY,
            );
            let id = m.add_species(at, 80.0, 36.0);
            m.find_species_mut(&id).expect("just added").initial_value = rng.range(1.0, 5.0);
            id
        })
        .collect();

    let mut reactions: Vec<(Vec<usize>, Vec<usize>)> =
        (0..SPECIES - 1).map(|i| (vec![i], vec![i + 1])).collect();
    for _ in 0..EXTRA_REACTIONS {
        let roll = rng.next_f64();
        let (nr, np) = if roll < 0.2 {
            (2, 1)
        } else if roll < 0.4 {
            (1, 2)
        } else {
            (1, 1)
        };
        // Distinct species, drawn at random.
        let mut picked: Vec<usize> = Vec::new();
        while picked.len() < nr + np {
            let s = rng.below(SPECIES);
            if !picked.contains(&s) {
                picked.push(s);
            }
        }
        let products = picked.split_off(nr);
        reactions.push((picked, products));
    }

    for (reactants, products) in reactions {
        let centre = |list: &[usize]| {
            list.iter()
                .map(|&i| m.find_species(&ids[i]).expect("exists").center)
                .fold(crate::geometry::Pt::ZERO, |a, p| a + p)
                / list.len() as f64
        };
        let junction = (centre(&reactants) + centre(&products)) / 2.0;
        let id = m.add_reaction(junction);
        let k = format!("k{}", &id[1..]);
        let law = std::iter::once(k.clone())
            .chain(reactants.iter().map(|&i| ids[i].clone()))
            .collect::<Vec<_>>()
            .join("*");
        let uni_uni = reactants.len() == 1 && products.len() == 1;
        let r = m.find_reaction_mut(&id).expect("just added");
        r.reactants = reactants
            .iter()
            .map(|&i| Participant::new(&ids[i], 1.0))
            .collect();
        r.products = products
            .iter()
            .map(|&i| Participant::new(&ids[i], 1.0))
            .collect();
        r.kinetic_law = law;
        r.mode = if uni_uni {
            ReactionMode::Linear
        } else {
            ReactionMode::Straight
        };
        m.parameters.push(Parameter {
            variable: k,
            expression: "0.1".to_owned(),
            annotation: Default::default(),
        });
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutParams, LayoutRun};

    #[test]
    fn shape_of_the_network() {
        let m = random_network(1);
        assert_eq!(m.species().len(), 10);
        assert_eq!(m.reactions().len(), 12);
        assert_eq!(m.parameters.len(), 12);
        assert!(
            m.species()
                .iter()
                .all(|s| (1.0..5.0).contains(&s.initial_value))
        );
        // The backbone: S1 → S2 → … → S10, Linear.
        for (i, r) in m.reactions().iter().take(9).enumerate() {
            assert_eq!(r.reactants[0].species, format!("S{}", i + 1));
            assert_eq!(r.products[0].species, format!("S{}", i + 2));
            assert_eq!(r.mode, ReactionMode::Linear);
        }
        for r in m.reactions() {
            let k = format!("k{}", &r.id[1..]);
            assert!(r.kinetic_law.starts_with(&k));
            assert_eq!(m.find_parameter(&k).unwrap().expression, "0.1");
            // No species twice in one reaction.
            let mut all: Vec<_> = r
                .reactants
                .iter()
                .chain(&r.products)
                .map(|p| &p.species)
                .collect();
            all.sort();
            all.dedup();
            assert_eq!(all.len(), r.reactants.len() + r.products.len());
        }
    }

    #[test]
    fn deterministic_for_a_seed() {
        assert_eq!(random_network(5), random_network(5));
        let differs = (0..20).any(|s| random_network(s) != random_network(s + 100));
        assert!(differs);
    }

    #[test]
    fn some_seeds_give_bi_reactions() {
        let bi = (0..50).any(|s| {
            random_network(s)
                .reactions()
                .iter()
                .any(|r| r.reactants.len() == 2 || r.products.len() == 2)
        });
        assert!(bi);
    }

    /// Where a junction and one species balance: Kr·mᵢ·mⱼ/d² = Ka·d.
    const JUNCTION_SPACING: f64 = 155.0;

    fn overlap(m: &Model) -> bool {
        let s = m.species();
        (0..s.len()).any(|i| {
            (i + 1..s.len()).any(|k| {
                (s[i].center.x - s[k].center.x).abs() < (s[i].width + s[k].width) / 2.0
                    && (s[i].center.y - s[k].center.y).abs() < (s[i].height + s[k].height) / 2.0
            })
        })
    }

    /// M6 acceptance: laying out a random 10-species, 12-reaction network
    /// gives no overlapping nodes, junctions between their participants, and
    /// a second run doesn't make the drawing drift.
    #[test]
    fn laid_out_random_networks_are_tidy() {
        for seed in 0..10 {
            let m = random_network(seed);
            let params = LayoutParams {
                randomize: true,
                ..LayoutParams::default()
            };
            let mut out = m.clone();
            LayoutRun::new(&m, params, seed).finish(&mut out);
            assert!(!overlap(&out), "seed {seed}: nodes overlap");
            // Each junction stays near its participants. With the reference's
            // constants a junction settles about 150 from each of its species
            // (8000·3·3/d² = 0.02·d), so when two species are held close
            // together the junction sits off to the side. Allow for that
            // spacing; a junction flung away from its reaction still fails.
            for r in out.reactions() {
                let pts: Vec<_> = r
                    .reactants
                    .iter()
                    .chain(&r.products)
                    .map(|p| out.find_species(&p.species).unwrap().center)
                    .collect();
                let centre =
                    pts.iter().fold(crate::geometry::Pt::ZERO, |a, &p| a + p) / pts.len() as f64;
                let reach = pts.iter().map(|p| p.distance(centre)).fold(0.0, f64::max);
                assert!(
                    r.junction.distance(centre) <= reach + JUNCTION_SPACING,
                    "seed {seed}: junction of {} is far from its participants",
                    r.id
                );
            }
            // A second run (without Randomize) may rearrange the network, but
            // the drawing doesn't drift: the centroid of all species and
            // junctions stays put, and it is still tidy.
            let mut again = out.clone();
            LayoutRun::new(&out, LayoutParams::default(), seed).finish(&mut again);
            let centroid = |m: &Model| {
                let pts: Vec<_> = m
                    .species()
                    .iter()
                    .map(|s| s.center)
                    .chain(m.reactions().iter().map(|r| r.junction))
                    .collect();
                pts.iter().fold(crate::geometry::Pt::ZERO, |a, &p| a + p) / pts.len() as f64
            };
            assert!(
                centroid(&out).distance(centroid(&again)) < 1e-6,
                "seed {seed} drifted"
            );
            assert!(!overlap(&again), "seed {seed}: second run overlaps");
        }
    }

    /// With the Deckard pass, a Bézier reaction's junction goes exactly
    /// between its reactants and products.
    #[test]
    fn deckard_puts_junctions_between_their_species() {
        let mut m = random_network(2);
        for r in m.reactions_mut() {
            r.mode = ReactionMode::Bezier;
        }
        let params = LayoutParams {
            randomize: true,
            deckard: true,
            ..LayoutParams::default()
        };
        let mut out = m.clone();
        LayoutRun::new(&m, params, 2).finish(&mut out);
        for r in out.reactions() {
            let centre = |list: &[Participant]| {
                list.iter()
                    .map(|p| out.find_species(&p.species).unwrap().center)
                    .fold(crate::geometry::Pt::ZERO, |a, p| a + p)
                    / list.len() as f64
            };
            let mid = (centre(&r.reactants) + centre(&r.products)) / 2.0;
            assert!(r.junction.distance(mid) < 1e-6, "{}", r.id);
        }
    }
}
