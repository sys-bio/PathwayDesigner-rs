//! SBML Level 2 or 3 into a new model, with its layout when it has one
//! (S §16.2 F-1, SPEC-Rust §8.3).

use std::collections::{HashMap, HashSet};

use sbml_io as sb;

use super::math::from_math_with;
use super::render_in::Target;
use crate::antimony::ImportOptions;
use crate::expr::{self, Ast};
use crate::geometry::{Pt, pt};
use crate::layout::Rng;
use crate::model::{
    AlgebraicRule, AssignmentRule, Compartment, CompartmentSbml, Constraint, DEFAULT_COMPARTMENT,
    Event, EventAssignment, EventFormula, FunctionDefinition, History, InitialAssignment,
    LocalParameter, Model, ModelUnits, ModifierKind, PackageAttribute, PackageData, Parameter,
    Participant, RateRule, Reaction, ReactionMode, SpeciesNode, SpeciesReference, SpeciesSbml,
    Trigger, Unit, UnitDefinition, is_valid_sid,
};
use crate::passes::{self, NullPlace};

/// Species without a glyph go at random in [0, 600)², as for Antimony.
const PLACEMENT: f64 = 600.0;

/// Whether the model has anything with id `id`: a compartment, species,
/// parameter, reaction, function, or a reaction's local parameter.
fn uses_id(m: &sb::Model, id: &str) -> bool {
    m.compartments.iter().any(|c| c.id == id)
        || m.species.iter().any(|s| s.id == id)
        || m.parameters.iter().any(|p| p.id == id)
        || m.function_definitions.iter().any(|f| f.id == id)
        || m.reactions.iter().any(|r| {
            r.id == id
                || r.kinetic_law
                    .as_ref()
                    .is_some_and(|k| k.parameters.iter().any(|p| p.id == id))
        })
}

/// The value `name` has at the start, for showing a starting value that
/// only a formula gives: its initial assignment worked out, else a rule's
/// value, a parameter's value, a species' initial value or a compartment's
/// size. `None` when it can't be worked out.
fn start_value(
    model: &Model,
    functions: &HashMap<String, expr::FunctionDef>,
    name: &str,
) -> Option<f64> {
    fn value(
        model: &Model,
        functions: &HashMap<String, expr::FunctionDef>,
        name: &str,
        depth: usize,
    ) -> Option<f64> {
        if depth > 20 {
            return None;
        }
        let ev = |text: &str| {
            let ast = expr::parse(text).ok()?;
            let ast = expr::expand_functions(&ast, functions).ok()?;
            expr::evaluate(&ast, &|n: &str| value(model, functions, n, depth + 1))
        };
        if let Some(a) = model.initial_assignment(name) {
            return ev(&a.expression);
        }
        if let Some(r) = model.assignment_rules.iter().find(|r| r.variable == name) {
            return ev(&r.expression);
        }
        if let Some(p) = model.parameters.iter().find(|p| p.variable == name) {
            return p.value;
        }
        if let Some(s) = model.find_species(name) {
            return (!s.sbml.initial_unset).then_some(s.initial_value);
        }
        if let Some(c) = model.compartments.iter().find(|c| c.id == name) {
            return (!c.sbml.size_unset).then_some(c.size);
        }
        match name {
            "pi" => Some(std::f64::consts::PI),
            "exponentiale" => Some(std::f64::consts::E),
            "true" => Some(1.0),
            "false" => Some(0.0),
            "avogadro" => Some(6.02214179e23),
            _ => None,
        }
    }
    value(model, functions, name, 0)
}

/// A species or compartment given its starting value only by a formula
/// shows the value the formula gives, worked out here; the file's own
/// (absent) value is still what is written back.
pub(crate) fn show_start_values(model: &mut Model, functions: &HashMap<String, expr::FunctionDef>) {
    let targets: Vec<String> = model
        .initial_assignments
        .iter()
        .map(|a| a.symbol.clone())
        .collect();
    for symbol in targets {
        let Some(v) = start_value(model, functions, &symbol) else {
            continue;
        };
        if let Some(s) = model.find_species_mut(&symbol)
            && s.sbml.initial_unset
        {
            s.initial_value = v;
            s.initial_is_amount = s.in_amounts;
        } else if let Some(c) = model.compartments.iter_mut().find(|c| c.id == symbol)
            && c.sbml.size_unset
        {
            c.size = v;
        }
    }
}

/// [`show_start_values`] with the model's own functions.
pub(crate) fn show_start_values_of(model: &mut Model) {
    let functions: HashMap<String, expr::FunctionDef> = model
        .function_definitions
        .iter()
        .filter_map(|f| {
            let body = expr::parse(&f.body).ok()?;
            Some((
                f.id.clone(),
                expr::FunctionDef {
                    params: f.arguments.clone(),
                    body,
                },
            ))
        })
        .collect();
    show_start_values(model, &functions);
}

/// A successful import.
#[derive(Clone, Debug)]
pub struct Imported {
    pub model: Model,
    /// What was left out or changed; the import went ahead anyway.
    pub warnings: Vec<String>,
    /// Whether the file had a layout.
    pub had_layout: bool,
    /// Species placed at random because the layout had no glyph for them
    /// (all of them, without a layout). The app offers auto-layout if any.
    pub unplaced: usize,
}

/// Reads SBML into a new model. An error means no model: the caller leaves
/// the current diagram as it is.
pub fn import(xml: &str, opts: &ImportOptions) -> Result<Imported, Vec<sb::Diagnostic>> {
    let read = sb::read(xml)?;
    let Some(m) = read.document.model.as_ref() else {
        return Err(vec![sb::Diagnostic {
            line: 1,
            column: 1,
            message: "the file has no model".into(),
        }]);
    };
    let mut b = Builder {
        level: read.document.level,
        warnings: read.warnings.iter().map(|d| d.to_string()).collect(),
        model: Model::new(),
        rng: Rng::new(opts.seed),
        functions: HashMap::new(),
        targets: Vec::new(),
        taken: [
            "avogadro",
            "pi",
            "exponentiale",
            "true",
            "false",
            "inf",
            "NaN",
        ]
        .into_iter()
        .filter(|n| uses_id(m, n))
        .map(str::to_owned)
        .collect(),
        namespaces: read.document.namespaces.clone(),
        generated: 0,
        inline: HashSet::new(),
    };
    // What other packages put on the document, and their namespaces.
    b.model.sbml_document = package(&read.document.meta.extra, &read.document.namespaces);
    b.model.sbml_version = (read.document.level == 3).then_some(read.document.version);
    b.model.sbml_namespaces = read
        .document
        .namespaces
        .iter()
        .filter(|(_, uri)| !is_core_namespace(uri))
        .cloned()
        .collect();
    let (had_layout, unplaced) = b.build(m, opts);
    // Labelled by name when there are names (or label text) to show: SBML
    // can't say which the diagram shows, and ids such as Reactome's
    // `species_70469` read as if something were wrong.
    let m = &mut b.model;
    m.show_display_names = m
        .species()
        .iter()
        .any(|s| s.display_name.is_some() || s.label_text.is_some())
        || m.reactions()
            .iter()
            .any(|r| r.display_name.is_some() || r.label_text.is_some())
        || m.compartments
            .iter()
            .any(|c| c.display_name.is_some() || c.label_text.is_some());
    m.refresh_labels();
    Ok(Imported {
        model: b.model,
        warnings: b.warnings,
        had_layout,
        unplaced,
    })
}

struct Builder {
    level: u32,
    warnings: Vec<String>,
    model: Model,
    rng: Rng,
    /// The model's function definitions, to work out starting values with.
    functions: HashMap<String, expr::FunctionDef>,
    /// Each glyph and what it draws, for its look from Render.
    targets: Vec<(String, Target)>,
    /// The names of SBML's constants (`avogadro`, `pi`, …) that the model
    /// uses for items of its own, which in formulas mean those items.
    taken: HashSet<String>,
    /// The next number for an id libSBML's way, `generatedId_0`.
    generated: usize,
    /// Functions written into the formulas that call them, as they can't
    /// be Level 3 functions.
    inline: HashSet<String>,
    /// Those declared on `<sbml>`, which an annotation may use.
    namespaces: Vec<(Option<String>, String)>,
}

/// What the layout says about a reaction's empty side: where its null node
/// goes, and its handles.
#[derive(Default)]
struct NullLeg {
    place: Option<NullPlace>,
    handles: Option<(Pt, Pt)>,
}

impl Builder {
    fn warn(&mut self, message: String) {
        self.warnings.push(message);
    }

    /// An item's SBO term, database references and notes
    /// (docs/annotations-design.md).
    fn annotation(&self, meta: &sb::Meta) -> crate::model::Annotation {
        annotation_of(meta, &self.namespaces)
    }

    /// The MIRIAM part of an item's annotation, taken apart.
    fn miriam(&self, meta: &sb::Meta) -> sb::miriam::Annotation {
        meta.annotation
            .as_deref()
            .map(|a| sb::miriam::read_annotation(a, &self.namespaces))
            .unwrap_or_default()
    }

    /// An expression from MathML, or a warning saying why not.
    fn expression(
        &mut self,
        math: Option<&sb::Math>,
        rename: &HashMap<String, String>,
        what: &str,
    ) -> Option<Ast> {
        self.expression_with(math, rename, what, &[])
    }

    /// [`Self::expression`], with `own` names that mean themselves, not
    /// SBML's constants: a function's arguments.
    fn expression_with(
        &mut self,
        math: Option<&sb::Math>,
        rename: &HashMap<String, String>,
        what: &str,
        own: &[String],
    ) -> Option<Ast> {
        let taken = |n: &str| self.taken.contains(n) || own.iter().any(|a| a == n);
        // Calls of functions that can't be Level 3 ones, written in.
        let inline: HashMap<String, expr::FunctionDef> = self
            .functions
            .iter()
            .filter(|(id, _)| self.inline.contains(*id))
            .map(|(id, f)| (id.clone(), f.clone()))
            .collect();
        let read = from_math_with(math?, rename, &taken).and_then(|ast| {
            if inline.is_empty() {
                Ok(ast)
            } else {
                expr::expand_functions(&ast, &inline)
            }
        });
        match read {
            Ok(ast) => Some(ast),
            Err(why) => {
                self.warn(format!("{what} was left out: it uses {why}"));
                None
            }
        }
    }

    /// A formula as infix text: empty for none, `None` (with a warning) for
    /// MathML that can't be read, which leaves out what it belongs to.
    fn formula(&mut self, math: Option<&sb::Math>, what: &str) -> Option<String> {
        match math {
            None => Some(String::new()),
            Some(math) => self
                .expression(Some(math), &HashMap::new(), what)
                .map(|ast| expr::to_infix(&ast)),
        }
    }

    /// The function definitions, as written (F1), and to work out starting
    /// values with.
    ///
    /// A function that uses the time, or a name that isn't one of its
    /// arguments, as Level 2 Version 1 allowed, can't be one in Level 3: it is
    /// written into the formulas that call it, which means the same, with a
    /// warning.
    fn read_functions(&mut self, m: &sb::Model) {
        for f in &m.function_definitions {
            let (arguments, body) = match &f.math {
                Some(sb::Math::Lambda { args, body }) => {
                    let what = format!("The function {}", f.id);
                    match self.expression_with(Some(body), &HashMap::new(), &what, args) {
                        Some(ast) => {
                            self.functions.insert(
                                f.id.clone(),
                                expr::FunctionDef {
                                    params: args.clone(),
                                    body: ast.clone(),
                                },
                            );
                            // (An argument may be called `time`.)
                            let outside = expr::identifiers(&ast).into_iter().any(|n| {
                                !args.iter().any(|a| a == n)
                                    && (n == "time" || !super::math::is_constant_name(n))
                            });
                            if outside {
                                self.inline.insert(f.id.clone());
                                self.warn(format!(
                                    "The function {} uses the time or a name that isn't one of its \
                                     arguments, which Level 3 doesn't allow in a function, so it was \
                                     written into the formulas that call it",
                                    f.id
                                ));
                                continue;
                            }
                            (args.clone(), expr::to_infix(&ast))
                        }
                        None => continue,
                    }
                }
                _ => (Vec::new(), String::new()),
            };
            let annotation = self.annotation(&f.meta);
            self.model.function_definitions.push(FunctionDefinition {
                id: f.id.clone(),
                name: crate::model::display_name(f.name.clone()),
                arguments,
                body,
                annotation,
            });
        }
    }

    fn build(&mut self, m: &sb::Model, opts: &ImportOptions) -> (bool, usize) {
        self.report_unsupported(m);
        self.read_functions(m);
        self.model.model_name = m.name.clone().or_else(|| m.id.clone()).unwrap_or_default();
        self.model.model_id = m.id.clone();
        self.model.model_id_unset = m.id.is_none();
        self.model.model_name_unset = m.name.is_none();
        self.model.units = ModelUnits {
            substance: m.substance_units.clone(),
            time: m.time_units.clone(),
            volume: m.volume_units.clone(),
            area: m.area_units.clone(),
            length: m.length_units.clone(),
            extent: m.extent_units.clone(),
        };
        self.model.conversion_factor = m.conversion_factor.clone();
        self.model.annotation = self.annotation(&m.meta);
        // The model's history is its own field.
        self.model.history = std::mem::take(&mut self.model.annotation.history);

        for u in &m.unit_definitions {
            let units = u
                .units
                .iter()
                .map(|x| Unit {
                    kind: x.kind.clone(),
                    exponent: x.exponent.unwrap_or(1.0),
                    scale: x.scale.unwrap_or(0),
                    multiplier: x.multiplier.unwrap_or(1.0),
                    annotation: self.annotation(&x.meta),
                })
                .collect();
            let annotation = self.annotation(&u.meta);
            self.model.unit_definitions.push(UnitDefinition {
                id: u.id.clone(),
                name: crate::model::display_name(u.name.clone()),
                units,
                annotation,
            });
        }

        // Compartments, as written. Level 2 gives a missing number of
        // dimensions as 3.
        for c in &m.compartments {
            let mut comp = Compartment {
                size: c.size.unwrap_or(1.0),
                dimensions: c.spatial_dimensions.unwrap_or(3.0),
                display_name: crate::model::display_name(c.name.clone()),
                annotation: self.annotation(&c.meta),
                ..Compartment::new(c.id.clone())
            };
            comp.sbml = CompartmentSbml {
                units: c.units.clone(),
                constant: c.constant.unwrap_or(true),
                size_unset: c.size.is_none(),
                dimensions_unset: c.spatial_dimensions.is_none() && self.level >= 3,
            };
            self.model.compartments.push(comp);
        }

        if self.level < 3 {
            self.level_2_units(m);
        }

        // Species: the initial concentration, or amount, as given; whether
        // it stands for its amount in rate laws (found by the BioModels
        // round trip: these were all read as concentrations).
        for s in &m.species {
            let mut node = SpeciesNode::new(s.id.clone(), Pt::ZERO);
            node.in_amounts = s.has_only_substance_units.unwrap_or(false);
            (node.initial_value, node.initial_is_amount) =
                // A concentration of 0 beside an amount is the amount
                // (SPEC-Rust §8.3).
                match (s.initial_concentration, s.initial_amount) {
                    (Some(c), _) if c != 0.0 => (c, false),
                    (_, Some(a)) => (a, true),
                    (c, None) => (c.unwrap_or(0.0), c.is_none() && node.in_amounts),
                };
            node.is_boundary = s.boundary_condition.unwrap_or(false);
            node.is_constant = s.constant.unwrap_or(false);
            node.display_name = crate::model::display_name(s.name.clone());
            node.annotation = self.annotation(&s.meta);
            node.sbml = SpeciesSbml {
                // Level 2's default, as for the model's units (below).
                units: s
                    .substance_units
                    .clone()
                    .or_else(|| (self.level < 3).then(|| "substance".to_owned())),
                conversion_factor: s.conversion_factor.clone(),
                initial_unset: s.initial_concentration.is_none() && s.initial_amount.is_none(),
            };
            if s.compartment != DEFAULT_COMPARTMENT {
                node.compartment = s.compartment.clone();
            }
            if self.model.push_species(node).is_err() {
                self.warn(format!(
                    "a second species is called {}; it was left out",
                    s.id
                ));
            }
        }

        // Parameters, those rules set among them.
        for p in &m.parameters {
            let mut parameter = Parameter::new(p.id.clone(), p.value);
            parameter.name = crate::model::display_name(p.name.clone());
            parameter.units = p.units.clone();
            parameter.constant = p.constant.unwrap_or(true);
            parameter.annotation = self.annotation(&p.meta);
            self.model.parameters.push(parameter);
        }

        // Rules, each with its own annotation.
        for r in &m.rules {
            let annotation = self.annotation(r.meta());
            match r {
                sb::Rule::Assignment { variable, math, .. } => {
                    let what = format!("The rule for {variable}");
                    if let Some(expression) = self.formula(math.as_ref(), &what) {
                        self.model.assignment_rules.push(AssignmentRule {
                            variable: variable.clone(),
                            expression,
                            annotation,
                        });
                    }
                }
                sb::Rule::Rate { variable, math, .. } => {
                    let what = format!("The rate rule for {variable}");
                    if let Some(expression) = self.formula(math.as_ref(), &what) {
                        self.model.rate_rules.push(RateRule {
                            variable: variable.clone(),
                            expression,
                            annotation,
                        });
                    }
                }
                sb::Rule::Algebraic { math, .. } => {
                    if let Some(expression) = self.formula(math.as_ref(), "An algebraic rule") {
                        self.model.algebraic_rules.push(AlgebraicRule {
                            expression,
                            annotation,
                        });
                    }
                }
            }
        }

        // Initial assignments, kept as formulas (F3).
        for ia in &m.initial_assignments {
            let what = format!("The initial assignment to {}", ia.symbol);
            if let Some(expression) = self.formula(ia.math.as_ref(), &what) {
                let annotation = self.annotation(&ia.meta);
                self.model.initial_assignments.push(InitialAssignment {
                    symbol: ia.symbol.clone(),
                    expression,
                    annotation,
                });
            }
        }
        show_start_values(&mut self.model, &self.functions);

        for c in &m.constraints {
            if let Some(expression) = self.formula(c.math.as_ref(), "A constraint") {
                let annotation = self.annotation(&c.meta);
                self.model.constraints.push(Constraint {
                    expression,
                    message: c.message.clone(),
                    annotation,
                });
            }
        }

        for e in &m.events {
            if let Some(event) = self.event(e) {
                self.model.events.push(event);
            }
        }
        // What a rule or an event changes isn't constant, whatever Level 2's
        // default said.
        let changed: HashSet<String> = self
            .model
            .assignment_rules
            .iter()
            .map(|r| r.variable.clone())
            .chain(self.model.rate_rules.iter().map(|r| r.variable.clone()))
            .chain(
                self.model
                    .events
                    .iter()
                    .flat_map(|e| e.assignments.iter().map(|a| a.variable.clone())),
            )
            .collect();
        for p in &mut self.model.parameters {
            p.constant &= !changed.contains(&p.variable);
        }
        for c in &mut self.model.compartments {
            c.sbml.constant &= !changed.contains(&c.id);
        }

        // The layout: species first, so reactions can use their aliases. A
        // layout where no species glyph has a position counts as none.
        let layout = m.layouts.first().filter(|l| {
            l.species_glyphs
                .iter()
                .any(|g| positioned(g.bounding_box.as_ref()).is_some())
        });
        let (glyph_node, nulls) = match layout {
            Some(l) => self.place_species(l),
            None => (HashMap::new(), HashMap::new()),
        };
        let unplaced = self.place_the_rest(layout);

        for r in &m.reactions {
            self.reaction(r, layout, &glyph_node, &nulls, opts);
        }
        if let Some(l) = layout {
            self.reaction_names(l);
        }
        // Compartment boxes (S §7.9), and their style from PathwayDesigner's
        // note. Compartments without one are fitted around their species.
        if let Some(l) = m.layouts.first() {
            for g in &l.compartment_glyphs {
                let (Some(id), Some(b)) = (&g.compartment, positioned(g.bounding_box.as_ref()))
                else {
                    continue;
                };
                let (w, h) = (b.dimensions.width, b.dimensions.height);
                let look = g
                    .meta
                    .annotation
                    .as_deref()
                    .and_then(super::shape::read_compartment);
                // A box smaller than the smallest the app makes draws nothing:
                // Reactome gives the compartments its map leaves out a 1 × 1
                // box in a corner. Such a compartment is hidden, as one with
                // no glyph is (below).
                let min = crate::compartments::MIN_SIZE;
                if let Some(c) = self.model.find_compartment_mut(id)
                    && w >= min
                    && h >= min
                {
                    self.targets
                        .push((g.id.clone(), Target::Compartment(id.clone())));
                    for t in l
                        .text_glyphs
                        .iter()
                        .filter(|t| t.graphical_object.as_deref() == Some(g.id.as_str()))
                    {
                        self.targets
                            .push((t.id.clone(), Target::CompartmentName(id.clone())));
                        c.label_text = own_words(t, c.display_name.as_deref(), &c.id);
                    }
                    c.frame = Some(crate::model::Frame {
                        center: pt(b.position.x + w / 2.0, b.position.y + h / 2.0),
                        width: w,
                        height: h,
                    });
                    if let Some(look) = look {
                        if let Some(s) = look.style {
                            c.style = s;
                        }
                        c.hidden = look.hidden;
                        c.style.no_border = look.no_border;
                    }
                }
            }
        }
        // A layout that draws no box for a compartment doesn't show it: it is
        // hidden, so the box the app fits round it stays out of sight (S §7.9).
        if layout.is_some() {
            for c in self.model.compartments.iter_mut() {
                if c.frame.is_none() && !c.is_default() {
                    c.hidden = true;
                }
            }
        }
        // Free text: a text glyph with words of its own and nothing it labels
        // (S §7.8). Its look comes from PathwayDesigner's note, if it has one.
        if let Some(l) = m.layouts.first() {
            for g in &l.text_glyphs {
                let (None, None, Some(words)) = (&g.graphical_object, &g.origin_of_text, &g.text)
                else {
                    continue;
                };
                let at = positioned(g.bounding_box.as_ref())
                    .map_or(Pt::ZERO, |b| pt(b.position.x, b.position.y));
                let id = self.model.add_text(at, words);
                self.targets.push((g.id.clone(), Target::Text(id.clone())));
                let look = g
                    .meta
                    .annotation
                    .as_deref()
                    .and_then(super::shape::read_text);
                if let (Some(look), Some(t)) = (look, self.model.find_text_mut(&id)) {
                    t.font = look.font;
                    t.align = look.align;
                    t.size = look.size;
                    t.color = look.color;
                    t.wrap_width = look.wrap_width;
                }
            }
        }
        // How it all looks, from Render: after the notes, which give what
        // Render can't say (docs/render-design.md §6).
        if let Some(l) = layout
            && let Some(w) =
                super::render_in::apply(&mut self.model, l, m.global_render.as_ref(), &self.targets)
        {
            self.warn(w);
        }
        self.forget_made_up_reference_ids();
        self.forget_implied_default_compartment();
        crate::genes::from_packages(&mut self.model);
        self.model.recompute_id_counter();
        // A model built from submodels (docs/sbml-app-design.md A8).
        if self
            .model
            .annotation
            .package
            .elements
            .iter()
            .any(|e| e.contains("listOfSubmodels"))
        {
            self.warn(
                "This model is built from submodels (SBML's comp package). They are kept and \
                 saved with it, but only the top model is shown and edited here; an edit can break \
                 what links them"
                    .to_owned(),
            );
        }
        (layout.is_some(), unplaced)
    }

    /// The default compartment as export writes it for species in no
    /// compartment (size 1, three dimensions, constant, nothing more), which
    /// nothing uses or sets: the model's species are in no compartment again,
    /// as before it was saved, and saving writes it back the same (found by
    /// the Gallery's round trip through SBML).
    fn forget_implied_default_compartment(&mut self) {
        let m = &self.model;
        let Some(i) = m
            .compartments
            .iter()
            .position(|c| c.id == DEFAULT_COMPARTMENT)
        else {
            return;
        };
        let c = &m.compartments[i];
        let plain = c.size == 1.0
            && c.dimensions == 3.0
            && !c.sbml.size_unset
            && !c.sbml.dimensions_unset
            && c.sbml.constant
            && c.sbml.units.is_none()
            && c.display_name.is_none()
            && c.annotation == crate::model::Annotation::default();
        let used = m.formulas().iter().any(|(_, t)| {
            crate::model::names_used(t)
                .iter()
                .any(|n| n == DEFAULT_COMPARTMENT)
        }) || m.initial_assignment(DEFAULT_COMPARTMENT).is_some()
            || !crate::sentences::changed_by(m, DEFAULT_COMPARTMENT).is_empty()
            || m.reactions()
                .iter()
                .any(|r| r.sbml.compartment.as_deref() == Some(DEFAULT_COMPARTMENT))
            || m.species()
                .iter()
                .any(|s| s.sbml.conversion_factor.as_deref() == Some(DEFAULT_COMPARTMENT));
        if plain && !used {
            self.model.compartments.remove(i);
        }
    }

    /// Level 2's units by default, made explicit as Level 3 needs them,
    /// exactly as libSBML converts a model: without, a redefined `substance`
    /// would no longer be what amounts are in. The model's units are the
    /// default's names (`substance`, `volume`, `length`) where an item
    /// relies on that default or the file defines it, else the base units
    /// (`mole`, `litre`, `metre`); `area` always; `time` if the file defines
    /// it, else `second`. A default an item relies on that the file doesn't
    /// define is defined, as Level 2 has it; so is `area`. Compartments and
    /// species without units get the default's name. An item that names a
    /// default (`substanceUnits="substance"`) the file doesn't define relies
    /// on it too.
    fn level_2_units(&mut self, m: &sb::Model) {
        let defined = |id: &str| m.unit_definitions.iter().any(|u| u.id == id);
        // Every unit an item names.
        let named: Vec<&str> = m
            .compartments
            .iter()
            .filter_map(|c| c.units.as_deref())
            .chain(
                m.species
                    .iter()
                    .filter_map(|s| s.substance_units.as_deref()),
            )
            .chain(m.parameters.iter().filter_map(|p| p.units.as_deref()))
            .chain(m.reactions.iter().flat_map(|r| {
                r.kinetic_law
                    .iter()
                    .flat_map(|k| k.parameters.iter().filter_map(|p| p.units.as_deref()))
            }))
            .collect();
        let names = |id: &str| !defined(id) && named.contains(&id);
        let relies = |dims: f64| {
            m.compartments
                .iter()
                .any(|c| c.units.is_none() && c.spatial_dimensions.unwrap_or(3.0) == dims)
        };
        let substance = m.species.iter().any(|s| s.substance_units.is_none()) || names("substance");
        let volume = relies(3.0) || names("volume");
        let length = relies(1.0) || names("length");
        let add = |model: &mut Model, id: &str, kind: &str, exponent: f64| {
            if !defined(id) {
                model.unit_definitions.push(UnitDefinition {
                    id: id.to_owned(),
                    units: vec![Unit {
                        exponent,
                        ..Unit::new(kind)
                    }],
                    ..UnitDefinition::default()
                });
            }
        };
        if volume {
            add(&mut self.model, "volume", "litre", 1.0);
        }
        if substance {
            add(&mut self.model, "substance", "mole", 1.0);
        }
        add(&mut self.model, "area", "metre", 2.0);
        if length {
            add(&mut self.model, "length", "metre", 1.0);
        }
        // Level 2's `time`, which Level 3 doesn't know, named by an item:
        // defined as Level 2 has it (libSBML's own conversion leaves it
        // undefined, an error).
        if names("time") {
            add(&mut self.model, "time", "second", 1.0);
        }
        let named = |used: bool, id: &str, base: &str| {
            Some(if used || defined(id) { id } else { base }.to_owned())
        };
        let substance_units = named(substance, "substance", "mole");
        self.model.units = ModelUnits {
            substance: substance_units.clone(),
            time: named(false, "time", "second"),
            volume: named(volume, "volume", "litre"),
            area: Some("area".into()),
            length: named(length, "length", "metre"),
            extent: substance_units,
        };
        for c in &mut self.model.compartments {
            if c.sbml.units.is_none() {
                c.sbml.units = match c.dimensions {
                    3.0 => Some("volume".into()),
                    2.0 => Some("area".into()),
                    1.0 => Some("length".into()),
                    _ => None,
                };
            }
        }
    }

    /// A species reference id PathwayDesigner made up when writing, so its
    /// glyph could name it (`sr_J1_prod_B`), that nothing uses, is no id:
    /// otherwise each save and open would give the file ids it never had.
    fn forget_made_up_reference_ids(&mut self) {
        let used: HashSet<String> = self
            .model
            .formulas()
            .iter()
            .flat_map(|(_, f)| crate::model::names_used(f))
            .chain(
                self.model
                    .assignment_rules
                    .iter()
                    .map(|r| r.variable.clone()),
            )
            .chain(self.model.rate_rules.iter().map(|r| r.variable.clone()))
            .chain(
                self.model
                    .initial_assignments
                    .iter()
                    .map(|a| a.symbol.clone()),
            )
            .chain(
                self.model
                    .events
                    .iter()
                    .flat_map(|e| e.assignments.iter().map(|a| a.variable.clone())),
            )
            .collect();
        let nodes: HashMap<String, String> = self
            .model
            .species()
            .iter()
            .map(|s| (s.id.clone(), s.primary_id().to_owned()))
            .collect();
        for r in self.model.reactions_mut() {
            for (tag, list) in [
                ("sub", &mut r.reactants),
                ("prod", &mut r.products),
                ("mod", &mut r.modifiers),
            ] {
                for p in list.iter_mut() {
                    let species = nodes.get(&p.species).cloned().unwrap_or_default();
                    let made = format!("sr_{}_{tag}_{species}", r.id);
                    let Some(info) = p.sbml.as_deref_mut() else {
                        continue;
                    };
                    let Some(id) = info.id.as_deref() else {
                        continue;
                    };
                    let suffix = id.strip_prefix(made.as_str());
                    let ours = suffix.is_some_and(|rest| {
                        rest.is_empty()
                            || rest
                                .strip_prefix('_')
                                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
                    });
                    if ours && !used.contains(id) {
                        info.id = None;
                        if *info == SpeciesReference::default() {
                            p.sbml = None;
                        }
                    }
                }
            }
        }
    }

    /// An id for what needs one, as libSBML makes them: `generatedId_0`,
    /// `generatedId_1`, …, past any the model uses.
    fn generated_id(&mut self) -> String {
        loop {
            let id = format!("generatedId_{}", self.generated);
            self.generated += 1;
            if !self.model.is_id_used(&id) {
                return id;
            }
        }
    }

    /// An event, or `None` (with a warning) when one of its formulas can't
    /// be read.
    fn event(&mut self, e: &sb::Event) -> Option<Event> {
        let what = match &e.id {
            Some(id) => format!("The event {id}"),
            None => "An event".to_owned(),
        };
        let trigger = match &e.trigger {
            Some(t) => Trigger {
                expression: self.formula(t.math.as_ref(), &what)?,
                initial_value: t.initial_value.unwrap_or(true),
                persistent: t.persistent.unwrap_or(true),
                annotation: self.annotation(&t.meta),
            },
            None => Trigger::default(),
        };
        let delay = match &e.delay {
            Some(d) => Some(EventFormula {
                expression: self.formula(d.math.as_ref(), &what)?,
                annotation: self.annotation(&d.meta),
            }),
            None => None,
        };
        let priority = match &e.priority {
            Some(p) => Some(EventFormula {
                expression: self.formula(p.math.as_ref(), &what)?,
                annotation: self.annotation(&p.meta),
            }),
            None => None,
        };
        let mut assignments = Vec::new();
        for a in &e.assignments {
            assignments.push(EventAssignment {
                variable: a.variable.clone(),
                expression: self.formula(a.math.as_ref(), &what)?,
                annotation: self.annotation(&a.meta),
            });
        }
        Some(Event {
            id: e.id.clone(),
            name: crate::model::display_name(e.name.clone()),
            use_values_from_trigger_time: e.use_values_from_trigger_time.unwrap_or(true),
            trigger,
            no_trigger: e.trigger.is_none(),
            delay,
            priority,
            assignments,
            annotation: self.annotation(&e.meta),
        })
    }

    /// Says what the model has that PathwayDesigner doesn't keep: parts of
    /// Level 2 that Level 3 has no place for, and annotations of
    /// CellDesigner and JDesigner.
    fn report_unsupported(&mut self, m: &sb::Model) {
        let mut lost: Vec<&str> = Vec::new();
        if m.compartments.iter().any(|c| c.outside.is_some()) {
            lost.push("which compartment is outside another (Level 2's outside)");
        }
        if !m.compartment_types.is_empty()
            || m.compartments.iter().any(|c| c.compartment_type.is_some())
        {
            lost.push("compartment types");
        }
        if !m.species_types.is_empty() || m.species.iter().any(|s| s.species_type.is_some()) {
            lost.push("species types");
        }
        if m.species.iter().any(|s| s.charge.is_some()) {
            lost.push("species' charge");
        }
        if m.species.iter().any(|s| s.spatial_size_units.is_some()) {
            lost.push("species' spatial size units");
        }
        if m.reactions.iter().any(|r| {
            r.kinetic_law
                .as_ref()
                .is_some_and(|k| k.time_units.is_some() || k.substance_units.is_some())
        }) || m.events.iter().any(|e| e.time_units.is_some())
            || m.unit_definitions
                .iter()
                .any(|u| u.units.iter().any(|x| x.offset.is_some_and(|o| o != 0.0)))
        {
            lost.push("the units of rate laws and events, and unit offsets");
        }
        if !lost.is_empty() {
            self.warn(format!(
                "Level 2's {} have no place in Level 3, so they aren't kept",
                lost.join(", ")
            ));
        }
        if m.layouts.len() > 1 {
            self.warn(format!(
                "{} layouts: only the first was used",
                m.layouts.len()
            ));
        }
        // Another tool's own annotation is kept, but CellDesigner's and RDF
        // that can't be read (docs/other-annotations-design.md O1, O2): say
        // so once, before they are lost on saving.
        let annotated = |meta: &sb::Meta| kept_others(self.miriam(meta).other).1;
        let items = usize::from(annotated(&m.meta))
            + m.compartments.iter().filter(|c| annotated(&c.meta)).count()
            + m.species.iter().filter(|s| annotated(&s.meta)).count()
            + m.reactions.iter().filter(|r| annotated(&r.meta)).count()
            + m.parameters.iter().filter(|p| annotated(&p.meta)).count();
        if items > 0 {
            self.warn(format!(
                "CellDesigner's or JDesigner's own annotations, RDF that can't be read, or a second element \
                 in one namespace, on {items} item{} aren't kept: saving will lose them (other \
                 tools' annotations, SBO terms, database references and notes are kept)",
                if items == 1 { "" } else { "s" }
            ));
        }
    }

    /// Positions and sizes from species glyphs. A species' first glyph places
    /// it; each further glyph becomes an alias. Returns which node each glyph
    /// stands for, and the boxes of glyphs that name no species (null nodes).
    fn place_species(
        &mut self,
        l: &sb::Layout,
    ) -> (HashMap<String, String>, HashMap<String, NullPlace>) {
        let mut glyph_node = HashMap::new();
        let mut nulls = HashMap::new();
        // Which of a species' glyphs draws the species itself, the others
        // being aliases: the one this app names `sg_<id>` if there is one,
        // or else the first.
        let mut main_glyph: HashMap<&str, &str> = HashMap::new();
        for g in &l.species_glyphs {
            if positioned(g.bounding_box.as_ref()).is_none() {
                continue;
            }
            if let Some(species) = g.species.as_deref() {
                let named = g.id.strip_prefix("sg_") == Some(species);
                if named || !main_glyph.contains_key(species) {
                    main_glyph.insert(species, g.id.as_str());
                }
            }
        }
        let mut placed: HashSet<String> = HashSet::new();
        // Glyphs whose note says the name is outside.
        let mut outside: HashSet<String> = HashSet::new();
        for g in &l.species_glyphs {
            let Some(bb) = positioned(g.bounding_box.as_ref()) else {
                continue;
            };
            let (w, h) = (bb.dimensions.width, bb.dimensions.height);
            let center = pt(bb.position.x + w / 2.0, bb.position.y + h / 2.0);
            // Its shape and border style, from PathwayDesigner's note
            // (S §5.10, §5.3).
            let look = g
                .meta
                .annotation
                .as_deref()
                .and_then(super::shape::read_species)
                .unwrap_or_default();
            if look.label_outside {
                outside.insert(g.id.clone());
            }
            let species = g
                .species
                .as_deref()
                .filter(|s| self.model.find_species(s).is_some_and(|n| !n.is_null_node));
            let Some(species) = species else {
                nulls.insert(
                    g.id.clone(),
                    NullPlace {
                        center,
                        size: Some((w, h)),
                    },
                );
                continue;
            };
            if main_glyph.get(species) == Some(&g.id.as_str()) && placed.insert(species.to_owned())
            {
                let node = self.model.find_species_mut(species).expect("checked above");
                (node.center, node.width, node.height) = (center, w, h);
                (node.shape, node.border_dash) = (look.shape, look.border_dash);
                node.style.no_border = look.no_border;
                glyph_node.insert(g.id.clone(), species.to_owned());
                self.targets
                    .push((g.id.clone(), Target::Species(species.to_owned())));
                continue;
            }
            // An alias keeps the id it was exported with (`sg_<id>`) if it can.
            let wanted =
                g.id.strip_prefix("sg_")
                    .filter(|id| is_valid_sid(id) && !self.model.is_id_used(id));
            let id = match wanted {
                Some(id) => {
                    let mut node = SpeciesNode::new(id, center);
                    node.alias_of = Some(species.to_owned());
                    self.model.push_species(node).expect("checked unused");
                    id.to_owned()
                }
                None => self
                    .model
                    .add_alias(species, center)
                    .expect("species exists"),
            };
            let node = self.model.find_species_mut(&id).expect("just added");
            (node.width, node.height) = (w, h);
            (node.shape, node.border_dash) = (look.shape, look.border_dash);
            node.style.no_border = look.no_border;
            self.targets
                .push((g.id.clone(), Target::Species(id.clone())));
            glyph_node.insert(g.id.clone(), id);
        }
        // A species' name is outside its node when the note says so, or when
        // its text glyph's centre is outside the node's box, as other tools
        // write it (S §5.11). The name is drawn at that centre.
        for t in &l.text_glyphs {
            let Some(gid) = &t.graphical_object else {
                continue;
            };
            let Some(id) = glyph_node.get(gid).cloned() else {
                continue;
            };
            // Its own words, as Reactome gives its short labels
            // (docs/layout-labels-design.md L4).
            let name = self
                .model
                .find_species(&id)
                .and_then(|n| self.model.find_species(n.primary_id()))
                .and_then(|p| p.display_name.clone());
            let Some(node) = self.model.find_species_mut(&id) else {
                continue;
            };
            node.label_text = own_words(t, name.as_deref(), node.primary_id());
            let Some(bb) = positioned(t.bounding_box.as_ref()) else {
                continue;
            };
            self.targets
                .push((t.id.clone(), Target::SpeciesName(node.id.clone())));
            let c = pt(
                bb.position.x + bb.dimensions.width / 2.0,
                bb.position.y + bb.dimensions.height / 2.0,
            );
            let inside =
                crate::geometry::point_in_rect(c, node.center, node.width / 2.0, node.height / 2.0);
            if outside.contains(gid) || !inside {
                node.label_offset = Some(c - node.center);
            }
        }
        (glyph_node, nulls)
    }

    /// A text glyph on a reaction glyph shows the reaction's name, its
    /// centre where the name is drawn (S §5.12). Read once the reactions are
    /// in, as the name is placed from the junction drawn.
    fn reaction_names(&mut self, l: &sb::Layout) {
        for t in &l.text_glyphs {
            let (Some(gid), Some(bb)) = (&t.graphical_object, positioned(t.bounding_box.as_ref()))
            else {
                continue;
            };
            let Some(id) = l
                .reaction_glyphs
                .iter()
                .find(|g| &g.id == gid)
                .and_then(|g| g.reaction.clone())
            else {
                continue;
            };
            let Some(r) = self.model.find_reaction_mut(&id) else {
                continue;
            };
            r.label_text = own_words(t, r.display_name.as_deref(), &id);
            let r = self.model.find_reaction(&id).expect("found above");
            self.targets
                .push((t.id.clone(), Target::ReactionName(id.clone())));
            let c = pt(
                bb.position.x + bb.dimensions.width / 2.0,
                bb.position.y + bb.dimensions.height / 2.0,
            );
            let j = crate::render::draw::effective_junction(&self.model, r);
            if let Some(r) = self.model.find_reaction_mut(&id) {
                r.label_offset = Some(c - j);
            }
        }
    }

    /// Puts species the layout didn't place at random, inside the layout if
    /// there is one. Returns how many.
    fn place_the_rest(&mut self, layout: Option<&sb::Layout>) -> usize {
        let placed: HashSet<String> = layout
            .map(|l| {
                l.species_glyphs
                    .iter()
                    .filter(|g| positioned(g.bounding_box.as_ref()).is_some())
                    .filter_map(|g| g.species.clone())
                    .collect()
            })
            .unwrap_or_default();
        let (w, h) = layout
            .map(|l| (l.dimensions.width.max(1.0), l.dimensions.height.max(1.0)))
            .unwrap_or((PLACEMENT, PLACEMENT));
        let mut n = 0;
        let ids: Vec<String> = self
            .model
            .species()
            .iter()
            .filter(|s| !s.is_alias() && !placed.contains(&s.id))
            .map(|s| s.id.clone())
            .collect();
        for id in ids {
            let at = pt(self.rng.range(0.0, w), self.rng.range(0.0, h));
            self.model.find_species_mut(&id).expect("listed").center = at;
            n += 1;
        }
        if layout.is_some() && n > 0 {
            let were = if n == 1 { "was" } else { "were" };
            self.warn(format!(
                "{n} species had no glyph and {were} placed at random"
            ));
        }
        n
    }

    fn reaction(
        &mut self,
        r: &sb::Reaction,
        layout: Option<&sb::Layout>,
        glyph_node: &HashMap<String, String>,
        nulls: &HashMap<String, NullPlace>,
        opts: &ImportOptions,
    ) {
        if self.model.is_id_used(&r.id) {
            self.warn(format!(
                "the reaction id {} is used twice; the second was left out",
                r.id
            ));
            return;
        }
        let mut rx = Reaction::new(r.id.clone(), Pt::ZERO);
        rx.display_name = crate::model::display_name(r.name.clone());
        rx.annotation = self.annotation(&r.meta);
        rx.mode = ReactionMode::Straight;
        // Level 2 makes reactions reversible unless they say otherwise.
        rx.is_reversible = r.reversible.unwrap_or(self.level == 2);
        rx.sbml.fast = r.fast.unwrap_or(false);
        rx.sbml.compartment = r.compartment.clone();

        // Species references, remembering each one's id for the layout.
        let mut refs: Vec<(Option<String>, crate::model::Role, usize)> = Vec::new();
        for (role, list) in [
            (crate::model::Role::Reactant, &r.reactants),
            (crate::model::Role::Product, &r.products),
        ] {
            for sr in list {
                if self.model.find_species(&sr.species).is_none() {
                    self.warn(format!(
                        "reaction {} uses {}, which isn't a species; it was left out",
                        r.id, sr.species
                    ));
                    continue;
                }
                let mut n = sr.stoichiometry.unwrap_or(1.0);
                let mut info = SpeciesReference {
                    id: sr.id.clone(),
                    name: crate::model::display_name(sr.name.clone()),
                    constant: sr.constant.unwrap_or(true),
                    stoichiometry_unset: sr.stoichiometry.is_none() && self.level >= 3,
                    annotation: self.annotation(&sr.meta),
                };
                // Level 2's stoichiometry as a formula is, in Level 3, an
                // assignment rule on the species reference, as libSBML
                // converts it: the reference gets an id if it has none.
                if let Some(math) = &sr.stoichiometry_math {
                    let what = format!("The stoichiometry of {} in {}", sr.species, r.id);
                    if let Some(expression) = self.formula(Some(math), &what) {
                        let id = info.id.clone().unwrap_or_else(|| self.generated_id());
                        n = expression.parse().unwrap_or(1.0);
                        self.model.assignment_rules.push(AssignmentRule {
                            variable: id.clone(),
                            expression,
                            annotation: Default::default(),
                        });
                        info.id = Some(id);
                        info.constant = false;
                        info.stoichiometry_unset = true;
                    }
                }
                let mut p = Participant::new(sr.species.as_str(), n);
                p.sbml = reference(info);
                let list = rx.participants_mut(role);
                refs.push((sr.id.clone(), role, list.len()));
                list.push(p);
            }
        }
        for mr in &r.modifiers {
            if self.model.find_species(&mr.species).is_none() {
                self.warn(format!(
                    "reaction {} has {} as a modifier, which isn't a species; it was left out",
                    r.id, mr.species
                ));
                continue;
            }
            // A species may be a modifier twice: both are kept.
            {
                refs.push((
                    mr.id.clone(),
                    crate::model::Role::Modifier,
                    rx.modifiers.len(),
                ));
                let mut p = Participant::new(mr.species.as_str(), 1.0);
                // Its kind from its SBO term (S §5.7.1); a layout role
                // given below wins. The term itself is kept as written.
                if let Some(term) = &mr.meta.sbo_term {
                    p.kind = ModifierKind::from_sbo(term);
                }
                p.sbml = reference(SpeciesReference {
                    id: mr.id.clone(),
                    name: crate::model::display_name(mr.name.clone()),
                    annotation: self.annotation(&mr.meta),
                    ..SpeciesReference::default()
                });
                rx.modifiers.push(p);
            }
        }

        // The rate law, with its own local parameters (F2).
        let mut law = None;
        if let Some(kl) = &r.kinetic_law {
            for lp in &kl.parameters {
                let annotation = self.annotation(&lp.meta);
                rx.sbml.local_parameters.push(LocalParameter {
                    id: lp.id.clone(),
                    name: crate::model::display_name(lp.name.clone()),
                    value: lp.value,
                    units: lp.units.clone(),
                    annotation,
                });
            }
            rx.sbml.kinetic_law_annotation = self.annotation(&kl.meta);
            law = self.expression(
                kl.math.as_ref(),
                &HashMap::new(),
                &format!("The rate law of {}", r.id),
            );
        }
        if let Some(ast) = &law {
            rx.kinetic_law = expr::to_infix(ast);
        }

        // The layout: the junction, which node each leg is drawn to, handles.
        let glyph = layout.and_then(|l| {
            l.reaction_glyphs
                .iter()
                .find(|g| g.reaction.as_deref() == Some(r.id.as_str()))
        });
        rx.junction = match glyph.and_then(junction_of) {
            Some(j) => j,
            None => self.centroid(&rx),
        };
        let mut source = NullLeg::default();
        let mut sink = NullLeg::default();
        if let Some(g) = glyph {
            let mut matched: HashSet<(crate::model::Role, usize)> = HashSet::new();
            for srg in &g.species_reference_glyphs {
                let role = srg.role.as_ref();
                // A glyph that names no species: a null node.
                if let Some(place) = nulls.get(&srg.species_glyph) {
                    let is_source =
                        !matches!(role, Some(sb::Role::Product | sb::Role::SideProduct));
                    let leg = if is_source { &mut source } else { &mut sink };
                    leg.place = Some(*place);
                    leg.handles = srg.curve.as_ref().and_then(|c| {
                        null_leg_handles(c, rx.junction, place.center, is_source, None)
                    });
                    continue;
                }
                let Some(node) = glyph_node.get(&srg.species_glyph) else {
                    continue;
                };
                let primary = self
                    .model
                    .find_species(node)
                    .map(|n| n.primary_id().to_owned())
                    .unwrap_or_default();
                let found = find_participant(&rx, &refs, &matched, srg, &primary);
                let Some((prole, index)) = found else {
                    continue;
                };
                matched.insert((prole, index));
                let center = self
                    .model
                    .find_species(node)
                    .map(|n| n.center)
                    .unwrap_or_default();
                let junction = rx.junction;
                let p = &mut rx.participants_mut(prole)[index];
                p.species = node.clone();
                if prole == crate::model::Role::Modifier {
                    self.targets
                        .push((srg.id.clone(), Target::Modifier(r.id.clone(), node.clone())));
                    match srg.role {
                        Some(sb::Role::Activator) => p.kind = ModifierKind::Activator,
                        Some(sb::Role::Inhibitor) => p.kind = ModifierKind::Inhibitor,
                        _ => {}
                    }
                    p.points = corner_points(srg.curve.as_ref(), junction);
                    if let Some(look) = srg
                        .meta
                        .annotation
                        .as_deref()
                        .and_then(super::shape::read_modifier)
                    {
                        p.look = look;
                    }
                }
                if prole != crate::model::Role::Modifier
                    && let Some(curve) = srg.curve.as_ref()
                    && leg_from_curve(
                        p,
                        prole == crate::model::Role::Reactant,
                        curve,
                        junction,
                        center,
                        None,
                    )
                {
                    rx.mode = ReactionMode::Bezier;
                }
            }
        }

        // PathwayDesigner's own note: Linear, and smooth junctions.
        if let Some(note) = glyph.and_then(|g| g.meta.annotation.as_deref()) {
            let (linear, smooth) = super::shape::read(note);
            if linear {
                rx.mode = ReactionMode::Linear;
            } else if smooth && rx.mode == ReactionMode::Bezier {
                rx.junction_smooth = true;
            }
        }

        // The reaction glyph's own curve is its stem (S §5.13): its first point
        // the reactants' end, its last the products', whichever way round a
        // tool wrote it.
        if rx.mode != ReactionMode::Linear
            && !rx.junction_smooth
            && let Some(segs) = glyph.and_then(|g| g.curve.as_ref()).map(|c| &c.segments)
            && let (Some(first), Some(last)) = (segs.first(), segs.last())
        {
            let p = |q: sb::Point| pt(q.x, q.y);
            let (mut a, mut b) = (p(first.start()), p(last.end()));
            let centre = |list: &[crate::model::Participant]| {
                let pts: Vec<Pt> = list
                    .iter()
                    .filter_map(|p| self.model.find_species(&p.species))
                    .map(|s| s.center)
                    .collect();
                (!pts.is_empty())
                    .then(|| pts.iter().fold(Pt::ZERO, |s, &q| s + q) / pts.len() as f64)
            };
            if let (Some(cr), Some(cp)) = (centre(&rx.reactants), centre(&rx.products))
                && a.distance(cr) + b.distance(cp) > a.distance(cp) + b.distance(cr)
            {
                std::mem::swap(&mut a, &mut b);
            }
            let j = rx.junction;
            if a.distance(j) > 1e-9 || b.distance(j) > 1e-9 {
                let stem = (a - j, b - j);
                rx.stem = Some(stem);
                rx.stem_straight = crate::model::stem_is_straight(stem);
            }
        }

        // The passes every import gets (SPEC-Rust §8.4); not the catalytic
        // rewrite, which would change the file's reactions (F6).
        let _ = opts;
        if let Some(ast) = &law {
            passes::modifiers_from_law(&self.model, &mut rx, ast);
        }
        passes::add_null_nodes(&mut self.model, &mut rx, source.place, sink.place);
        for (leg, id) in [
            (&source, format!("_source_{}", rx.id)),
            (&sink, format!("_sink_{}", rx.id)),
        ] {
            let Some((c1, c2)) = leg.handles else {
                continue;
            };
            for p in rx
                .reactants
                .iter_mut()
                .chain(rx.products.iter_mut())
                .filter(|p| p.species == id)
            {
                (p.ctrl1, p.ctrl2, p.ctrl_pts_set) = (c1, c2, true);
                rx.mode = ReactionMode::Bezier;
            }
        }
        if let Some(g) = glyph {
            self.targets
                .push((g.id.clone(), Target::Reaction(r.id.clone())));
        }
        self.model.push_reaction(rx).expect("checked unused above");
    }

    /// Halfway between the reactants' and the products' centres.
    fn centroid(&self, r: &Reaction) -> Pt {
        let centre = |list: &[Participant]| {
            let pts: Vec<Pt> = list
                .iter()
                .filter_map(|p| self.model.find_species(&p.species))
                .map(|s| s.center)
                .collect();
            (!pts.is_empty()).then(|| pts.iter().fold(Pt::ZERO, |a, &p| a + p) / pts.len() as f64)
        };
        match (centre(&r.reactants), centre(&r.products)) {
            (Some(a), Some(b)) => (a + b) / 2.0,
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => pt(PLACEMENT / 2.0, PLACEMENT / 2.0),
        }
    }
}

/// A species reference's SBML part, or `None` when it says nothing.
fn reference(r: SpeciesReference) -> Option<Box<SpeciesReference>> {
    (r != SpeciesReference::default()).then(|| Box::new(r))
}

/// A history as the model keeps it.
fn history(h: sb::miriam::History) -> History {
    History {
        creators: h
            .creators
            .into_iter()
            .map(|c| crate::model::Creator {
                family: c.family,
                given: c.given,
                email: c.email,
                organization: c.organization,
            })
            .collect(),
        created: h.created,
        modified: h.modified,
    }
}

/// What other packages put on an element (F10).
/// Layout and Render are written from the drawing, so what they put on an
/// element isn't kept as another package's; an attribute in no namespace
/// is SBML core of another level (Level 2's `constant` on a rate law's
/// parameter), which Level 3 hasn't, not a package's.
fn package(extra: &sb::Extra, namespaces: &[(Option<String>, String)]) -> PackageData {
    let drawing = |uri: &str| {
        uri.contains("/layout/")
            || uri.contains("/render/")
            || uri.starts_with("http://projects.eml.org/bcb/sbml")
    };
    PackageData {
        attributes: extra
            .attributes
            .iter()
            .filter(|a| a.namespace.as_deref().is_some_and(|ns| !drawing(ns)))
            .map(|a| PackageAttribute {
                name: a.name.clone(),
                namespace: a.namespace.clone(),
                value: a.value.clone(),
            })
            .collect(),
        elements: extra
            .elements
            .iter()
            .filter(|x| !element_namespace(x, namespaces).is_some_and(drawing))
            .cloned()
            .collect(),
    }
}

/// The namespace of an element kept as raw XML: from its own declarations,
/// else from the document's (`namespaces`).
fn element_namespace<'a>(
    xml: &'a str,
    namespaces: &'a [(Option<String>, String)],
) -> Option<&'a str> {
    let start = &xml[..xml.find('>').unwrap_or(xml.len())];
    let tag_end = start[1..]
        .find(|c: char| c.is_whitespace() || c == '/')
        .map_or(start.len(), |i| i + 1);
    let tag = &start[1..tag_end];
    let attr = match tag.split_once(':') {
        Some((prefix, _)) => format!("xmlns:{prefix}=\""),
        None => "xmlns=\"".to_owned(),
    };
    if let Some(at) = start.find(&attr) {
        let at = at + attr.len();
        let end = start[at..].find('"')? + at;
        return Some(&start[at..end]);
    }
    let prefix = tag.split_once(':').map(|(p, _)| p);
    namespaces
        .iter()
        .find(|(p, _)| p.as_deref() == prefix)
        .map(|(_, uri)| uri.as_str())
}

/// SBML core's own namespaces, and Layout's and Render's, which the writer
/// declares itself.
fn is_core_namespace(uri: &str) -> bool {
    uri.starts_with("http://www.sbml.org/sbml/level1")
        || uri.starts_with("http://www.sbml.org/sbml/level2")
        || uri.ends_with("/core")
        || uri.contains("/layout/")
        || uri.contains("/render/")
        || uri == "http://projects.eml.org/bcb/sbml/level2"
        || uri == "http://projects.eml.org/bcb/sbml/render/level2"
}

/// A bounding box that gives a position. libSBML writes an all-zero box
/// for a glyph that was never given one, so that counts as no box.
fn positioned(bb: Option<&sb::BoundingBox>) -> Option<&sb::BoundingBox> {
    bb.filter(|b| {
        let (p, d) = (&b.position, &b.dimensions);
        p.x != 0.0 || p.y != 0.0 || d.width != 0.0 || d.height != 0.0
    })
}

/// A reactant's or product's line from its curve. With several segments,
/// corner points where they meet (S §5.6.5) and the end piece's handles from
/// the segment at the species; with one, the handles of the curve carried on
/// to the node's centre. Returns whether it gave the line Bézier handles.
/// Antimony's layout statements are read through this too, with a
/// `tolerance` for carrying a curve on to the centre that allows for their
/// rounded numbers (see [`handles_to_centre`]).
pub(crate) fn leg_from_curve(
    p: &mut crate::model::Participant,
    reactant: bool,
    curve: &sb::Curve,
    junction: Pt,
    center: Pt,
    tolerance: Option<f64>,
) -> bool {
    if curve.segments.len() > 1 {
        let (corners, end_piece) = leg_route(curve, center);
        let first = corners[0];
        p.points = corners;
        if let sb::Segment::Bezier {
            start,
            end,
            base1,
            base2,
        } = end_piece
        {
            let q = |v: sb::Point| pt(v.x, v.y);
            (p.ctrl1, p.ctrl2) = handles_to_centre(
                (q(start), q(end), q(base1), q(base2)),
                first,
                center,
                reactant,
                true,
                tolerance,
            );
            p.ctrl_pts_set = true;
            return true;
        }
        false
    } else if let Some(h) = handles(Some(curve)) {
        (p.ctrl1, p.ctrl2) = handles_to_centre(h, junction, center, reactant, true, tolerance);
        p.ctrl_pts_set = true;
        true
    } else {
        false
    }
}

/// The handles of a line to a reaction's null node, from its curve.
pub(crate) fn null_leg_handles(
    curve: &sb::Curve,
    junction: Pt,
    null: Pt,
    is_source: bool,
    tolerance: Option<f64>,
) -> Option<(Pt, Pt)> {
    let single = curve.segments.len() == 1;
    handles(Some(curve)).map(|h| handles_to_centre(h, junction, null, is_source, single, tolerance))
}

/// A modifier's corner points from its curve: see [`corner_points`].
pub(crate) fn modifier_points(curve: &sb::Curve, junction: Pt) -> Vec<Pt> {
    corner_points(Some(curve), junction)
}

/// A reaction glyph's junction: the centre of its bounding box, or else the
/// middle of its curve.
fn junction_of(g: &sb::ReactionGlyph) -> Option<Pt> {
    if let Some(bb) = positioned(g.bounding_box.as_ref()) {
        let d = &bb.dimensions;
        return Some(pt(
            bb.position.x + d.width / 2.0,
            bb.position.y + d.height / 2.0,
        ));
    }
    let segs = &g.curve.as_ref()?.segments;
    let (a, b) = (segs.first()?.start(), segs.last()?.end());
    Some(pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0))
}

/// A reactant's or product's curve of several segments (S §5.6.5): its corner
/// points, where the segments meet, in order from the species (the end
/// nearer `species` is taken as the species', whichever way round it was
/// written); and the segment at the species, run from the species' end to
/// the first corner point.
fn leg_route(curve: &sb::Curve, species: Pt) -> (Vec<Pt>, sb::Segment) {
    let q = |v: sb::Point| pt(v.x, v.y);
    let segs = &curve.segments;
    let (first, last) = (q(segs[0].start()), q(segs[segs.len() - 1].end()));
    let from_species = first.distance(species) <= last.distance(species);
    let mut ordered: Vec<sb::Segment> = segs.clone();
    if !from_species {
        ordered.reverse();
        for s in &mut ordered {
            *s = match *s {
                sb::Segment::Line { start, end } => sb::Segment::Line {
                    start: end,
                    end: start,
                },
                sb::Segment::Bezier {
                    start,
                    end,
                    base1,
                    base2,
                } => sb::Segment::Bezier {
                    start: end,
                    end: start,
                    base1: base2,
                    base2: base1,
                },
            };
        }
    }
    let corners = ordered[..ordered.len() - 1]
        .iter()
        .map(|s| q(s.end()))
        .collect();
    (corners, ordered[0].clone())
}

/// A modifier's corner points from its glyph's curve (S §5.7.1): where its
/// segments meet, in order from the modifier. A curve written from the
/// junction end is turned round; Bézier segments count as straight.
fn corner_points(curve: Option<&sb::Curve>, junction: Pt) -> Vec<Pt> {
    let Some(segs) = curve.map(|c| &c.segments).filter(|s| s.len() > 1) else {
        return Vec::new();
    };
    let p = |q: sb::Point| pt(q.x, q.y);
    let mut points: Vec<Pt> = segs[..segs.len() - 1].iter().map(|s| p(s.end())).collect();
    let (first, last) = (p(segs[0].start()), p(segs[segs.len() - 1].end()));
    if first.distance(junction) < last.distance(junction) {
        points.reverse();
    }
    points
}

/// A curve's ends and handles: (start, end, base point 1, base point 2), or
/// `None` unless it has a Bézier segment. With several segments, the first
/// segment gives the first handle and the last the second.
fn handles(curve: Option<&sb::Curve>) -> Option<(Pt, Pt, Pt, Pt)> {
    let segs = &curve?.segments;
    if !segs.iter().any(|s| matches!(s, sb::Segment::Bezier { .. })) {
        return None;
    }
    let p = |q: sb::Point| pt(q.x, q.y);
    let (first, last) = (segs.first()?, segs.last()?);
    let c1 = match first {
        sb::Segment::Bezier { base1, .. } => p(*base1),
        s => p(s.start()),
    };
    let c2 = match last {
        sb::Segment::Bezier { base2, .. } => p(*base2),
        s => p(s.end()),
    };
    Some((p(first.start()), p(last.end()), c1, c2))
}

/// `(ctrl1, ctrl2)` for a leg whose curve runs from `start` to `end`. The
/// handles are taken as stored (S §11.5), unless the curve runs the other way
/// round from this app's convention (a reactant's from the species to the
/// junction, a product's from the junction to the species), as some tools
/// write it.
fn orient(
    start: Pt,
    end: Pt,
    c1: Pt,
    c2: Pt,
    junction: Pt,
    species: Pt,
    reactant: bool,
) -> (Pt, Pt) {
    let from_species = start.distance(species) + end.distance(junction)
        <= start.distance(junction) + end.distance(species);
    if from_species == reactant {
        (c1, c2)
    } else {
        (c2, c1)
    }
}

/// `(ctrl1, ctrl2)` for a leg drawn by one Bézier segment from `start` to
/// `end`. This app's legs run to the node's centre and are drawn cut at its
/// border, and that drawn part is what it writes (S §11.5): when the segment,
/// carried on, passes through the node's centre, the handles are those of
/// the whole curve. Otherwise, or with several segments, they are the
/// segment's own, as other tools write them. How near the centre the curve
/// must pass: `tolerance`, or else a millionth of the coordinates' size.
fn handles_to_centre(
    (start, end, c1, c2): (Pt, Pt, Pt, Pt),
    junction: Pt,
    species: Pt,
    reactant: bool,
    single: bool,
    tolerance: Option<f64>,
) -> (Pt, Pt) {
    let (h1, h2) = orient(start, end, c1, c2, junction, species, reactant);
    if !single {
        return (h1, h2);
    }
    use crate::geometry::CubicBezier;
    // From the junction's end to the border, as written or turned round, as
    // `orient` decides.
    let from_species = start.distance(species) + end.distance(junction)
        <= start.distance(junction) + end.distance(species);
    let drawn = if from_species {
        CubicBezier::new(end, c2, c1, start)
    } else {
        CubicBezier::new(start, c1, c2, end)
    };
    let tolerance =
        tolerance.unwrap_or_else(|| 1e-6 * (1.0 + species.x.abs().max(species.y.abs())));
    match drawn.extended_to(species, tolerance) {
        // A reactant's curve runs from the species to the junction.
        Some(whole) if reactant => (whole.p2, whole.p1),
        Some(whole) => (whole.p1, whole.p2),
        None => (h1, h2),
    }
}

/// The participant a species reference glyph draws: the one its
/// `speciesReference` names, or else the first not yet matched with the
/// glyph's species, in the list its role says.
fn find_participant(
    rx: &Reaction,
    refs: &[(Option<String>, crate::model::Role, usize)],
    matched: &HashSet<(crate::model::Role, usize)>,
    srg: &sb::SpeciesReferenceGlyph,
    species: &str,
) -> Option<(crate::model::Role, usize)> {
    use crate::model::Role;
    if let Some(id) = &srg.species_reference
        && let Some((_, role, index)) = refs.iter().find(|(r, ..)| r.as_ref() == Some(id))
    {
        return Some((*role, *index));
    }
    let roles: &[Role] = match srg.role {
        Some(sb::Role::Substrate | sb::Role::SideSubstrate) => &[Role::Reactant],
        Some(sb::Role::Product | sb::Role::SideProduct) => &[Role::Product],
        Some(sb::Role::Modifier | sb::Role::Activator | sb::Role::Inhibitor) => &[Role::Modifier],
        _ => &[Role::Reactant, Role::Product, Role::Modifier],
    };
    roles.iter().find_map(|&role| {
        rx.participants(role)
            .iter()
            .enumerate()
            .find(|(i, p)| p.species == species && !matched.contains(&(role, *i)))
            .map(|(i, _)| (role, i))
    })
}

/// `SBO:0000247` → 247; `None` for anything else.
/// An element's annotation as the model holds it: its SBO term, MIRIAM
/// references, notes, metaid, history, other tools' XML and other packages'
/// parts. Gene products, read from fbc's raw XML, use it too
/// (docs/uniprot-genes-design.md U1).
pub(crate) fn annotation_of(
    meta: &sb::Meta,
    namespaces: &[(Option<String>, String)],
) -> crate::model::Annotation {
    let miriam = meta
        .annotation
        .as_deref()
        .map(|a| sb::miriam::read_annotation(a, namespaces))
        .unwrap_or_default();
    crate::model::Annotation {
        sbo: meta.sbo_term.as_deref().and_then(sbo_number),
        references: miriam
            .terms
            .iter()
            .flat_map(|t| {
                let q = format!(
                    "{}:{}",
                    if t.model { "bqmodel" } else { "bqbiol" },
                    t.qualifier
                );
                t.resources.iter().map(move |r| crate::model::Reference {
                    qualifier: q.clone(),
                    resource: r.clone(),
                })
            })
            .collect(),
        notes: meta.notes.clone().filter(|n| !n.trim().is_empty()),
        other: kept_others(miriam.other).0,
        metaid: meta.metaid.clone(),
        history: history(miriam.history),
        package: package(&meta.extra, namespaces),
        rdf: miriam.descriptions,
    }
}

fn sbo_number(term: &str) -> Option<u32> {
    term.trim().strip_prefix("SBO:")?.parse().ok()
}

/// A text glyph's own words, unless they are the item's display name or id,
/// which need nothing kept (docs/layout-labels-design.md L4).
fn own_words(t: &sb::TextGlyph, name: Option<&str>, id: &str) -> Option<String> {
    crate::model::display_name(t.text.clone()).filter(|w| Some(w.as_str()) != name && w != id)
}

/// The namespaces of CellDesigner and JDesigner, whose annotations hold
/// their own copy of their diagram (docs/other-annotations-design.md O1).
const DIAGRAM_TOOLS: [&str; 2] = [
    "http://www.sbml.org/2001/ns/celldesigner",
    "http://www.sys-bio.org/sbml",
];

/// Whether another tool's element of an annotation is kept: not
/// CellDesigner's or JDesigner's (O1), not RDF the reader couldn't take
/// apart (O2), and XML that reads.
fn kept(o: &sb::miriam::Other) -> bool {
    o.namespace
        .as_deref()
        .is_some_and(|ns| !DIAGRAM_TOOLS.contains(&ns))
        && !o.is_rdf()
}

/// The elements of an annotation kept ([`kept`]), as XML, the first of each
/// namespace only: SBML Level 3, which is written, allows one top-level
/// element per namespace in an annotation, where Level 2 files (JDesigner's
/// `jd:display` beside its `jd:listOfShadows`) can have more. And whether
/// any element was left out.
fn kept_others(others: Vec<sb::miriam::Other>) -> (Vec<String>, bool) {
    let all = others.len();
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for o in others.into_iter().filter(kept) {
        let ns = o.namespace.clone().unwrap_or_default();
        if !seen.contains(&ns) {
            seen.push(ns);
            out.push(o.xml);
        }
    }
    let dropped = out.len() < all;
    (out, dropped)
}
