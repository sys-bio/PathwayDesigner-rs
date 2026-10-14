//! Compartments on the diagram (S §7.9): which one a species is in, boxes
//! fitted around their species, and what a dragged box carries.
//!
//! Two rules keep the drawing and the model in step:
//! - when the **user** moves a species, it takes the compartment it ends up
//!   in: the innermost box its centre is in, or the default compartment;
//! - when the **app** moves species (auto-layout) or a compartment has no box
//!   yet (an older file, an import), species keep their compartment and the
//!   boxes are fitted around them.

use crate::geometry::{Pt, pt};
use crate::model::{Frame, Id, Model};
use crate::render::draw::content_bounds;
use crate::textbox::text_box;

/// Space between a fitted box and the species inside it, room for the name
/// included.
pub const FIT_PADDING: f64 = 30.0;
/// The size of a box drawn with a click instead of a drag, or placed for a
/// compartment with no species.
pub const DEFAULT_SIZE: Pt = pt(300.0, 200.0);
/// The smallest a box can be made.
pub const MIN_SIZE: f64 = 40.0;

/// The innermost drawn compartment whose box contains `p`: the smallest, or
/// of equal ones the last. `None` means the default compartment.
pub fn innermost_at(model: &Model, p: Pt) -> Option<Id> {
    model
        .compartments
        .iter()
        .filter_map(|c| c.frame.filter(|f| f.contains(p)).map(|f| (c, f.area())))
        .fold(
            None,
            |best: Option<(&crate::model::Compartment, f64)>, (c, a)| match best {
                Some((_, b)) if b < a => best,
                _ => Some((c, a)),
            },
        )
        .map(|(c, _)| c.id.clone())
}

/// Puts each of these species in the compartment it sits in (S §7.9). Aliases
/// and null nodes are left alone: a species' compartment follows its primary.
/// With no compartment on the diagram, nothing changes.
pub fn reassign(model: &mut Model, ids: &[Id]) {
    if !model.compartments.iter().any(|c| c.frame.is_some()) {
        return;
    }
    for id in ids {
        let Some(s) = model.find_species(id) else {
            continue;
        };
        if s.is_alias() || s.is_null_node {
            continue;
        }
        let into = innermost_at(model, s.center).unwrap_or_default();
        model.find_species_mut(id).expect("found above").compartment = into;
    }
}

/// The ids of the primary species in compartment `id`.
pub fn members(model: &Model, id: &str) -> Vec<Id> {
    model
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node && s.compartment.eq_ignore_ascii_case(id))
        .map(|s| s.id.clone())
        .collect()
}

/// What a dragged box carries (S §7.9): the species whose centre, the
/// reactions whose junction, the texts whose box centre, and the compartments
/// whose box, are inside it. A Linear reaction's junction stays on its line,
/// so it isn't carried.
pub struct Contents {
    pub species: Vec<Id>,
    pub junctions: Vec<Id>,
    pub texts: Vec<Id>,
    pub compartments: Vec<Id>,
}

pub fn contents(model: &Model, frame: &Frame) -> Contents {
    Contents {
        species: model
            .species()
            .iter()
            .filter(|s| frame.contains(s.center))
            .map(|s| s.id.clone())
            .collect(),
        junctions: model
            .reactions()
            .iter()
            .filter(|r| {
                crate::render::draw::effective_mode(r) != crate::model::ReactionMode::Linear
                    && frame.contains(r.junction)
            })
            .map(|r| r.id.clone())
            .collect(),
        texts: model
            .texts()
            .iter()
            .filter(|t| {
                let (a, b) = text_box(t);
                frame.contains((a + b) / 2.0)
            })
            .map(|t| t.id.clone())
            .collect(),
        compartments: model
            .compartments
            .iter()
            .filter(|c| c.frame.is_some_and(|f| frame.encloses(&f) && f != *frame))
            .map(|c| c.id.clone())
            .collect(),
    }
}

/// Which drawn compartment sits directly inside which: (child, parent) by
/// box, the parent being the smallest box that encloses the child's.
pub fn nesting(model: &Model) -> Vec<(Id, Id)> {
    let framed: Vec<(&Id, Frame)> = model
        .compartments
        .iter()
        .filter_map(|c| c.frame.map(|f| (&c.id, f)))
        .collect();
    framed
        .iter()
        .filter_map(|(child, f)| {
            framed
                .iter()
                .filter(|(other, g)| other != child && g.encloses(f) && g.area() > f.area())
                .min_by(|a, b| a.1.area().total_cmp(&b.1.area()))
                .map(|(parent, _)| ((*child).clone(), (*parent).clone()))
        })
        .collect()
}

/// The box around these species' boxes and these frames, plus the padding.
fn fit_around(model: &Model, species: &[Id], frames: &[Frame]) -> Option<Frame> {
    let mut lo = pt(f64::INFINITY, f64::INFINITY);
    let mut hi = pt(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut grow = |a: Pt, b: Pt| {
        lo = pt(lo.x.min(a.x), lo.y.min(a.y));
        hi = pt(hi.x.max(b.x), hi.y.max(b.y));
    };
    for s in species.iter().filter_map(|id| model.find_species(id)) {
        let half = pt(s.width / 2.0, s.height / 2.0);
        grow(s.center - half, s.center + half);
    }
    for f in frames {
        grow(f.min(), f.max());
    }
    (lo.x <= hi.x).then(|| {
        let pad = pt(FIT_PADDING, FIT_PADDING);
        Frame::from_corners(lo - pad, hi + pad)
    })
}

/// Fits each drawn compartment's box around its species and the boxes nested
/// in it (as `nest` says), innermost first. A compartment with neither keeps
/// its box. Used while auto-layout moves species (S §10).
pub fn refit(model: &mut Model, nest: &[(Id, Id)]) {
    let depth = |id: &Id| {
        let mut d = 0;
        let mut at = id;
        while let Some((_, parent)) = nest.iter().find(|(c, _)| c == at) {
            d += 1;
            at = parent;
            if d > nest.len() {
                break;
            }
        }
        d
    };
    let mut order: Vec<(usize, Id)> = model
        .compartments
        .iter()
        .filter(|c| c.frame.is_some())
        .map(|c| (depth(&c.id), c.id.clone()))
        .collect();
    order.sort_by_key(|o| std::cmp::Reverse(o.0));
    for (_, id) in order {
        let children: Vec<Frame> = nest
            .iter()
            .filter(|(_, p)| *p == id)
            .filter_map(|(c, _)| model.find_compartment(c).and_then(|c| c.frame))
            .collect();
        let fitted = fit_around(model, &members(model, &id), &children);
        if let (Some(f), Some(c)) = (fitted, model.find_compartment_mut(&id)) {
            c.frame = Some(f);
        }
    }
}

/// Gives every compartment other than the default one a box, if it has none
/// (an older file, an import): fitted around its species, or, with none,
/// [`DEFAULT_SIZE`] to the right of the drawing. Species keep their
/// compartments.
pub fn place_unplaced(model: &mut Model) {
    let unplaced: Vec<Id> = model
        .compartments
        .iter()
        .filter(|c| c.frame.is_none() && !c.is_default())
        .map(|c| c.id.clone())
        .collect();
    for id in unplaced {
        let frame = fit_around(model, &members(model, &id), &[]).unwrap_or_else(|| {
            let (min, max) = content_bounds(model);
            Frame {
                center: pt(max.x + DEFAULT_SIZE.x / 2.0, (min.y + max.y) / 2.0),
                width: DEFAULT_SIZE.x,
                height: DEFAULT_SIZE.y,
            }
        });
        model.find_compartment_mut(&id).expect("listed above").frame = Some(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Compartment, SpeciesNode};

    /// S1 at (0,0) and S2 at (200,0) in "cell"; S3 at (500,0) in the default
    /// compartment; "cell" has no box yet.
    fn model() -> Model {
        let mut m = Model::new();
        for (id, x, comp) in [
            ("S1", 0.0, "cell"),
            ("S2", 200.0, "cell"),
            ("S3", 500.0, ""),
        ] {
            let mut s = SpeciesNode::new(id, pt(x, 0.0));
            s.compartment = comp.into();
            m.push_species(s).unwrap();
        }
        m.compartments.push(Compartment::new("cell"));
        m.compartments.push(Compartment::new("defaultCompartment"));
        m
    }

    #[test]
    fn unplaced_compartments_are_fitted_around_their_species() {
        let mut m = model();
        place_unplaced(&mut m);
        let f = m.find_compartment("cell").unwrap().frame.unwrap();
        assert_eq!(f.min(), pt(-40.0 - FIT_PADDING, -18.0 - FIT_PADDING));
        assert_eq!(f.max(), pt(240.0 + FIT_PADDING, 18.0 + FIT_PADDING));
        assert!(
            m.find_compartment("defaultCompartment")
                .unwrap()
                .frame
                .is_none()
        );
        // Membership by position now agrees with the file.
        assert_eq!(innermost_at(&m, pt(0.0, 0.0)).as_deref(), Some("cell"));
        assert_eq!(innermost_at(&m, pt(500.0, 0.0)), None);
        // An empty compartment gets a box of its own, out of the way.
        m.compartments.push(Compartment::new("empty"));
        place_unplaced(&mut m);
        let e = m.find_compartment("empty").unwrap().frame.unwrap();
        assert_eq!((e.width, e.height), (DEFAULT_SIZE.x, DEFAULT_SIZE.y));
        assert!(!e.contains(pt(500.0, 0.0)));
    }

    #[test]
    fn the_innermost_box_wins_and_species_follow_position() {
        let mut m = model();
        place_unplaced(&mut m);
        let inner = m.add_compartment(Frame::from_corners(pt(150.0, -30.0), pt(250.0, 30.0)));
        reassign(&mut m, &["S1".into(), "S2".into(), "S3".into()]);
        assert_eq!(m.find_species("S1").unwrap().compartment, "cell");
        assert_eq!(m.find_species("S2").unwrap().compartment, inner);
        assert_eq!(m.find_species("S3").unwrap().compartment, "");
        assert_eq!(nesting(&m), [(inner.clone(), "cell".to_owned())]);
        let c = contents(&m, &m.find_compartment("cell").unwrap().frame.unwrap());
        assert_eq!(c.species, ["S1", "S2"]);
        assert_eq!(c.compartments, [inner]);
    }

    #[test]
    fn refitting_follows_members_and_nested_boxes() {
        let mut m = model();
        place_unplaced(&mut m);
        let inner = m.add_compartment(Frame::from_corners(pt(150.0, -30.0), pt(250.0, 30.0)));
        reassign(&mut m, &["S2".into()]);
        let nest = nesting(&m);
        // Layout moves things far apart.
        m.find_species_mut("S1").unwrap().center = pt(-500.0, 0.0);
        m.find_species_mut("S2").unwrap().center = pt(900.0, 300.0);
        refit(&mut m, &nest);
        let i = m.find_compartment(&inner).unwrap().frame.unwrap();
        let c = m.find_compartment("cell").unwrap().frame.unwrap();
        assert!(i.contains(pt(900.0, 300.0)));
        assert!(c.encloses(&i) && c.contains(pt(-500.0, 0.0)));
        // Nobody changed compartment.
        assert_eq!(m.find_species("S1").unwrap().compartment, "cell");
        assert_eq!(m.find_species("S2").unwrap().compartment, inner);
    }
}
