//! Reading the Layout package. The same code reads Level 3 layouts and the
//! Level 2 ones kept in annotations; `pkg` is the namespace in each case.

use roxmltree::Node;

use super::{E, Reader};
use crate::layout::*;
use crate::ns;

type P<'p> = Option<&'p str>;

impl<'a, 'i: 'a> Reader<'a, 'i> {
    pub(super) fn layout(&mut self, n: Node<'a, 'i>, pkg: P) -> Layout {
        let lns = pkg.unwrap_or(ns::LAYOUT);
        let mut e = E::new(n);
        let mut l = Layout {
            id: self.req_pkg(&mut e, pkg, "id"),
            name: e.take_pkg(pkg, "name").map(str::to_string),
            ..Layout::default()
        };
        let mut dims = None;
        l.meta = self.meta(e, pkg, |r, c| {
            match r.name_in(c, lns) {
                Some("dimensions") => dims = Some(r.dimensions(c, pkg)),
                Some("listOfCompartmentGlyphs") => {
                    l.compartment_glyphs = r.list(c, lns, &["compartmentGlyph"], |r, c| {
                        r.compartment_glyph(c, pkg)
                    })
                }
                Some("listOfSpeciesGlyphs") => {
                    l.species_glyphs =
                        r.list(c, lns, &["speciesGlyph"], |r, c| r.species_glyph(c, pkg))
                }
                Some("listOfReactionGlyphs") => {
                    l.reaction_glyphs =
                        r.list(c, lns, &["reactionGlyph"], |r, c| r.reaction_glyph(c, pkg))
                }
                Some("listOfTextGlyphs") => {
                    l.text_glyphs = r.list(c, lns, &["textGlyph"], |r, c| r.text_glyph(c, pkg))
                }
                Some("listOfAdditionalGraphicalObjects") => {
                    l.additional = r.list(c, lns, ADDITIONAL, |r, c| r.additional(c, pkg))
                }
                _ => return false,
            }
            true
        });
        match dims {
            Some(d) => l.dimensions = d,
            None => self.error(n, "<layout> needs <dimensions>"),
        }
        l
    }

    fn additional(&mut self, n: Node<'a, 'i>, pkg: P) -> AdditionalGlyph {
        match n.tag_name().name() {
            "generalGlyph" => AdditionalGlyph::General(self.general_glyph(n, pkg)),
            "compartmentGlyph" => AdditionalGlyph::Compartment(self.compartment_glyph(n, pkg)),
            "speciesGlyph" => AdditionalGlyph::Species(self.species_glyph(n, pkg)),
            "reactionGlyph" => AdditionalGlyph::Reaction(self.reaction_glyph(n, pkg)),
            "textGlyph" => AdditionalGlyph::Text(self.text_glyph(n, pkg)),
            _ => {
                let mut e = E::new(n);
                let id = self.req_pkg(&mut e, pkg, "id");
                let mut bounding_box = None;
                let meta = self.meta(e, pkg, |r, c| {
                    r.bounding_box_child(c, pkg, &mut bounding_box)
                });
                AdditionalGlyph::Graphical(GraphicalObject {
                    id,
                    bounding_box,
                    meta,
                })
            }
        }
    }

    fn compartment_glyph(&mut self, n: Node<'a, 'i>, pkg: P) -> CompartmentGlyph {
        let mut e = E::new(n);
        let mut g = CompartmentGlyph {
            id: self.req_pkg(&mut e, pkg, "id"),
            compartment: e.take_pkg(pkg, "compartment").map(str::to_string),
            order: self.double_pkg(&mut e, pkg, "order"),
            ..Default::default()
        };
        g.meta = self.meta(e, pkg, |r, c| {
            r.bounding_box_child(c, pkg, &mut g.bounding_box)
        });
        g
    }

    fn species_glyph(&mut self, n: Node<'a, 'i>, pkg: P) -> SpeciesGlyph {
        let mut e = E::new(n);
        let mut g = SpeciesGlyph {
            id: self.req_pkg(&mut e, pkg, "id"),
            species: e.take_pkg(pkg, "species").map(str::to_string),
            ..Default::default()
        };
        g.meta = self.meta(e, pkg, |r, c| {
            r.bounding_box_child(c, pkg, &mut g.bounding_box)
        });
        g
    }

    fn reaction_glyph(&mut self, n: Node<'a, 'i>, pkg: P) -> ReactionGlyph {
        let lns = pkg.unwrap_or(ns::LAYOUT);
        let mut e = E::new(n);
        let mut g = ReactionGlyph {
            id: self.req_pkg(&mut e, pkg, "id"),
            reaction: e.take_pkg(pkg, "reaction").map(str::to_string),
            ..Default::default()
        };
        g.meta = self.meta(e, pkg, |r, c| {
            if r.name_in(c, lns) == Some("listOfSpeciesReferenceGlyphs") {
                g.species_reference_glyphs = r.list(c, lns, &["speciesReferenceGlyph"], |r, c| {
                    r.species_reference_glyph(c, pkg)
                });
                return true;
            }
            r.bounding_box_child(c, pkg, &mut g.bounding_box) || r.curve_child(c, pkg, &mut g.curve)
        });
        g
    }

    fn species_reference_glyph(&mut self, n: Node<'a, 'i>, pkg: P) -> SpeciesReferenceGlyph {
        let mut e = E::new(n);
        let mut g = SpeciesReferenceGlyph {
            id: self.req_pkg(&mut e, pkg, "id"),
            species_glyph: self.req_pkg(&mut e, pkg, "speciesGlyph"),
            species_reference: e.take_pkg(pkg, "speciesReference").map(str::to_string),
            role: e.take_pkg(pkg, "role").map(Role::parse),
            ..Default::default()
        };
        g.meta = self.meta(e, pkg, |r, c| {
            r.bounding_box_child(c, pkg, &mut g.bounding_box) || r.curve_child(c, pkg, &mut g.curve)
        });
        g
    }

    fn text_glyph(&mut self, n: Node<'a, 'i>, pkg: P) -> TextGlyph {
        let mut e = E::new(n);
        let mut g = TextGlyph {
            id: self.req_pkg(&mut e, pkg, "id"),
            text: e.take_pkg(pkg, "text").map(str::to_string),
            graphical_object: e.take_pkg(pkg, "graphicalObject").map(str::to_string),
            origin_of_text: e.take_pkg(pkg, "originOfText").map(str::to_string),
            ..Default::default()
        };
        g.meta = self.meta(e, pkg, |r, c| {
            r.bounding_box_child(c, pkg, &mut g.bounding_box)
        });
        g
    }

    fn general_glyph(&mut self, n: Node<'a, 'i>, pkg: P) -> GeneralGlyph {
        let lns = pkg.unwrap_or(ns::LAYOUT);
        let mut e = E::new(n);
        let mut g = GeneralGlyph {
            id: self.req_pkg(&mut e, pkg, "id"),
            reference: e.take_pkg(pkg, "reference").map(str::to_string),
            ..Default::default()
        };
        g.meta = self.meta(e, pkg, |r, c| {
            match r.name_in(c, lns) {
                Some("listOfReferenceGlyphs") => {
                    g.reference_glyphs = r.list(c, lns, &["referenceGlyph"], |r, c| {
                        r.reference_glyph(c, pkg)
                    })
                }
                Some("listOfSubGlyphs") => {
                    g.sub_glyphs = r.list(c, lns, ADDITIONAL, |r, c| r.additional(c, pkg))
                }
                _ => {
                    return r.bounding_box_child(c, pkg, &mut g.bounding_box)
                        || r.curve_child(c, pkg, &mut g.curve);
                }
            }
            true
        });
        g
    }

    fn reference_glyph(&mut self, n: Node<'a, 'i>, pkg: P) -> ReferenceGlyph {
        let mut e = E::new(n);
        let mut g = ReferenceGlyph {
            id: self.req_pkg(&mut e, pkg, "id"),
            glyph: self.req_pkg(&mut e, pkg, "glyph"),
            reference: e.take_pkg(pkg, "reference").map(str::to_string),
            role: e.take_pkg(pkg, "role").map(str::to_string),
            ..Default::default()
        };
        g.meta = self.meta(e, pkg, |r, c| {
            r.bounding_box_child(c, pkg, &mut g.bounding_box) || r.curve_child(c, pkg, &mut g.curve)
        });
        g
    }

    fn bounding_box_child(
        &mut self,
        c: Node<'a, 'i>,
        pkg: P,
        slot: &mut Option<BoundingBox>,
    ) -> bool {
        if self.name_in(c, pkg.unwrap_or(ns::LAYOUT)) != Some("boundingBox") {
            return false;
        }
        let lns = pkg.unwrap_or(ns::LAYOUT);
        let mut e = E::new(c);
        let mut bb = BoundingBox {
            id: e.take_pkg(pkg, "id").map(str::to_string),
            ..Default::default()
        };
        let (mut pos, mut dims) = (None, None);
        for k in c.children().filter(|k| k.is_element()) {
            match self.name_in(k, lns) {
                Some("position") => pos = Some(self.point(k, pkg)),
                Some("dimensions") => dims = Some(self.dimensions(k, pkg)),
                _ => self.warn(k, "dropped from <boundingBox>"),
            }
        }
        match (pos, dims) {
            (Some(p), Some(d)) => (bb.position, bb.dimensions) = (p, d),
            _ => self.error(c, "<boundingBox> needs <position> and <dimensions>"),
        }
        *slot = Some(bb);
        true
    }

    fn curve_child(&mut self, c: Node<'a, 'i>, pkg: P, slot: &mut Option<Curve>) -> bool {
        let lns = pkg.unwrap_or(ns::LAYOUT);
        if self.name_in(c, lns) != Some("curve") {
            return false;
        }
        let mut curve = Curve::default();
        for k in c.children().filter(|k| k.is_element()) {
            if self.name_in(k, lns) != Some("listOfCurveSegments") {
                self.warn(k, "dropped from <curve>");
                continue;
            }
            curve.segments = self.list(k, lns, &["curveSegment"], |r, s| r.segment(s, pkg));
        }
        *slot = Some(curve);
        true
    }

    fn segment(&mut self, n: Node<'a, 'i>, pkg: P) -> Segment {
        let lns = pkg.unwrap_or(ns::LAYOUT);
        let kind = n
            .attribute((ns::XSI, "type"))
            .map(|t| t.rsplit(':').next().unwrap_or(t));
        let mut points = [None; 4];
        for k in n.children().filter(|k| k.is_element()) {
            let i = match self.name_in(k, lns) {
                Some("start") => 0,
                Some("end") => 1,
                Some("basePoint1") => 2,
                Some("basePoint2") => 3,
                _ => {
                    self.warn(k, "dropped from <curveSegment>");
                    continue;
                }
            };
            points[i] = Some(self.point(k, pkg));
        }
        let [start, end, b1, b2] = points;
        let (start, end) = match (start, end) {
            (Some(s), Some(e)) => (s, e),
            _ => {
                self.error(n, "<curveSegment> needs <start> and <end>");
                (Point::default(), Point::default())
            }
        };
        let bezier = match kind {
            Some("CubicBezier") => true,
            Some("LineSegment") => false,
            _ => b1.is_some() || b2.is_some(),
        };
        if bezier {
            // A Bézier missing a base point degenerates towards its ends.
            Segment::Bezier {
                start,
                end,
                base1: b1.unwrap_or(start),
                base2: b2.unwrap_or(end),
            }
        } else {
            Segment::Line { start, end }
        }
    }

    fn point(&mut self, n: Node<'a, 'i>, pkg: P) -> Point {
        let mut e = E::new(n);
        let x = self.double_pkg(&mut e, pkg, "x");
        let y = self.double_pkg(&mut e, pkg, "y");
        if x.is_none() || y.is_none() {
            self.error(n, &format!("<{}> needs x and y", n.tag_name().name()));
        }
        Point {
            x: x.unwrap_or(0.0),
            y: y.unwrap_or(0.0),
            z: self.double_pkg(&mut e, pkg, "z"),
        }
    }

    fn dimensions(&mut self, n: Node<'a, 'i>, pkg: P) -> Dimensions {
        let mut e = E::new(n);
        let w = self.double_pkg(&mut e, pkg, "width");
        let h = self.double_pkg(&mut e, pkg, "height");
        if w.is_none() || h.is_none() {
            self.error(n, "<dimensions> needs width and height");
        }
        Dimensions {
            width: w.unwrap_or(0.0),
            height: h.unwrap_or(0.0),
            depth: self.double_pkg(&mut e, pkg, "depth"),
        }
    }
}

const ADDITIONAL: &[&str] = &[
    "graphicalObject",
    "generalGlyph",
    "compartmentGlyph",
    "speciesGlyph",
    "reactionGlyph",
    "textGlyph",
];
