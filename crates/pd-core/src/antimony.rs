//! Antimony import and export (S §11.2, §11.3, SPEC-Rust §8.2).
//!
//! The bridge between the `antimony` crate, which reads Antimony text into a
//! model of the language, and the diagram [`Model`]. Everything specific to
//! PathwayDesigner lives here: case resolution, implied parameters, the
//! catalytic rewrite, modifiers from the rate law, null nodes, random
//! placement, warnings for what a diagram cannot hold, and the layout of the
//! text it writes.

use std::collections::HashMap;
use std::fmt::Write as _;

use antimony::model::{Module, Participant as Term, Symbol, SymbolKind};
use antimony::{Code, Span};

use crate::expr;
use crate::geometry::{Pt, pt};
use crate::layout::Rng;
use crate::model::{
    AssignmentRule, Compartment, DEFAULT_COMPARTMENT, Model, ModifierKind, Parameter, Participant,
    ReactionMode, SpeciesNode,
};
use crate::passes;

/// New species go at random in [0, 600)² (S §11.2).
const PLACEMENT: f64 = 600.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportOptions {
    /// `A -> A + B` becomes `-> B` with `A` as a modifier (S §12).
    pub convert_catalytic: bool,
    /// For the random placement of the species.
    pub seed: u64,
}

/// A successful import.
#[derive(Clone, Debug)]
pub struct Imported {
    pub model: Model,
    /// Statements that were skipped; the import went ahead anyway.
    pub warnings: Vec<Diagnostic>,
    /// The text placed some species with Antimony's layout statements
    /// (`S1.pos = {28, 35}`).
    pub had_layout: bool,
    /// Species the layout statements didn't place, left at random, when
    /// they placed others.
    pub unplaced: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The text can't be used.
    Error,
    /// Something was skipped; the rest is fine.
    Warning,
}

/// A problem in the text, with a 1-based line and column (0 when it has no
/// place in the text).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl Diagnostic {
    pub(crate) fn at(severity: Severity, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity,
            line: span.start.line as usize,
            column: span.start.column as usize,
            message: message.into(),
        }
    }
}

/// Reads Antimony text into a new model (S §11.2). Any error means no model:
/// the caller leaves the current diagram as it is.
pub fn import(src: &str, opts: &ImportOptions) -> Result<Imported, Vec<Diagnostic>> {
    let loaded = antimony::parse(src);
    let errors: Vec<Diagnostic> = loaded
        .diagnostics
        .iter()
        .filter(|d| d.severity == antimony::Severity::Error)
        .map(|d| Diagnostic::at(Severity::Error, d.span, d.message.clone()))
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    // A name used and never given a value is a parameter at 0, or a species
    // at 0, by design (S §11.2), so the crate's warnings about that are not
    // passed on.
    let mut warnings: Vec<Diagnostic> = loaded
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == antimony::Severity::Warning
                && !matches!(d.code, Code::UninitialisedSymbol | Code::ImplicitParameter)
        })
        .map(|d| Diagnostic::at(Severity::Warning, d.span, d.message.clone()))
        .collect();
    let document = &loaded.document;
    let module = document.main_module();
    warnings.extend(skipped(document, module));
    let mut b = Builder {
        src,
        module,
        errors: Vec::new(),
        warnings: Vec::new(),
        functions: HashMap::new(),
    };
    let mut model = b.build(opts);
    warnings.append(&mut b.warnings);
    // The drawing: a diagram comment (docs/diagram-format.md), which says
    // more, else Antimony's layout statements (docs/antimony-layout-design.md).
    let (had_layout, unplaced) = match crate::diagram_format::parse(src) {
        Some(crate::diagram_format::Parsed {
            diagram: Some(d),
            warnings: found,
        }) => {
            warnings.extend(found);
            // Antimony's layout statements, if the text has them too (as
            // PathwayDesigner writes for tellurium), give way to the diagram.
            // What the diagram leaves out is laid out as it is read, so
            // nothing is left for the app to place.
            crate::diagram_format::read::apply(&mut model, &d, opts.seed, &mut warnings);
            (true, 0)
        }
        parsed => {
            warnings.extend(parsed.map(|p| p.warnings).unwrap_or_default());
            let applied = crate::antimony_layout::apply(&mut model, module, &mut warnings);
            (applied.had_layout, applied.unplaced)
        }
    };
    warnings.sort_by_key(|d| (d.line, d.column));
    if b.errors.is_empty() {
        Ok(Imported {
            model,
            warnings,
            had_layout,
            unplaced,
        })
    } else {
        Err(b.errors)
    }
}

/// A warning for each thing in the text that the diagram cannot hold: the
/// import goes ahead without it (SPEC-Rust §8.2).
fn skipped(document: &antimony::model::Document, module: &Module) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    // Functions are not skipped: calls to them are expanded (S §11.2).
    if let Some(first) = module.functions.first() {
        let n = module.functions.len();
        let what = if n == 1 {
            "function was"
        } else {
            "functions were"
        };
        out.push(Diagnostic::at(
            Severity::Warning,
            first.span,
            format!("{n} {what} written into the rate laws that use them"),
        ));
    }
    let mut skip = |span: Span, what: &str| {
        out.push(Diagnostic::at(
            Severity::Warning,
            span,
            format!("{what} aren't supported; this one was skipped"),
        ));
    };
    for m in &document.modules {
        if !std::ptr::eq(m, module) {
            skip(m.span, "Other models in the file");
        }
    }
    for u in &module.units {
        skip(u.span, "'unit' statements");
    }
    for e in &module.events {
        skip(e.span, "Events");
    }
    for s in &module.symbols {
        if let Some(rule) = s.rate_rule() {
            skip(rule.span, "Rate rules");
        }
        if s.notes.is_some() || !s.cv_terms.is_empty() || s.sbo_term.is_some() {
            skip(s.span, "Notes and annotations");
        }
        // Species, compartments and reactions keep their names; others don't.
        let named = matches!(
            s.kind,
            SymbolKind::Species | SymbolKind::Compartment | SymbolKind::Reaction
        ) || (s.is_boundary && s.kind == SymbolKind::Parameter);
        if s.display_name.is_some() && !named {
            skip(s.span, "Names of parameters and other items");
        }
        if s.units.is_some() || s.conversion_factor.is_some() {
            skip(s.span, "Units and conversion factors");
        }
    }
    for r in &module.algebraic_rules {
        skip(r.span, "Algebraic rules");
    }
    for c in &module.constraints {
        skip(c.span, "Constraints");
    }
    for o in &module.objectives {
        skip(o.span, "Objectives");
    }
    out.sort_by_key(|d| (d.line, d.column));
    out
}

/// Writes the model as Antimony (S §11.3): aliases and null nodes left out,
/// numbers with a `.` whatever the locale. One statement per line, indented
/// by two spaces, in sections introduced by `// …` comments: compartments,
/// species, reactions, parameters, initial values, then assignment rules.
/// A reaction without a rate law is written with the rate `v`. A reversible
/// reaction is written with `->`, an irreversible one with `=>` (S §11.3).
/// The model alone: [`export_with_layout`] writes its drawing too.
pub fn export(model: &Model) -> String {
    write_model(&safe_ids(model).0, Vec::new())
}

/// [`export`], with the drawing as Antimony's layout statements in a last
/// section, `// Layout` (docs/antimony-layout-export-design.md).
pub fn export_with_layout(model: &Model) -> String {
    let model = safe_ids(model).0;
    write_model(&model, crate::antimony_layout_out::statements(&model))
}

/// [`export`], with the drawing as a diagram after the model, inside a block
/// comment (docs/diagram-format.md), so any Antimony tool reads the model and
/// PathwayDesigner reads the drawing too.
pub fn export_with_diagram(model: &Model) -> String {
    diagram_export(model, false)
}

/// [`export_with_diagram`], with Antimony's layout statements as well, for
/// tools that draw from those (tellurium, SBMLNetwork); PathwayDesigner reads
/// the diagram.
pub fn export_with_diagram_and_layout(model: &Model) -> String {
    diagram_export(model, true)
}

fn diagram_export(model: &Model, layout: bool) -> String {
    let model = safe_ids(model).0;
    let statements = if layout {
        crate::antimony_layout_out::statements(&model)
    } else {
        Vec::new()
    };
    let mut text = write_model(&model, statements);
    text.push('\n');
    text.push_str(&crate::diagram_format::write::write(
        &model,
        model_name(&model),
    ));
    text
}

/// The name a model is written under: its own, if it is an Antimony id.
fn model_name(model: &Model) -> &str {
    Some(model.model_name.as_str())
        .filter(|n| is_identifier(n))
        .unwrap_or("anonymous")
}

/// What [`export_with_layout`] can't write, one line each.
pub fn layout_notes(model: &Model) -> Vec<String> {
    let (safe, renamed) = safe_ids(model);
    let mut notes = crate::antimony_layout_out::notes(&safe);
    renames_note(&mut notes, &renamed);
    notes
}

/// What [`export_with_diagram`] can't keep, for the user: ids written with
/// `_` added (Antimony reserves them), and texts whose line breaks and double
/// quotes a diagram's strings can't hold. With `layout`, what the layout
/// statements can't say too.
pub fn diagram_notes(model: &Model, layout: bool) -> Vec<String> {
    let (safe, renamed) = safe_ids(model);
    let mut notes = if layout {
        crate::antimony_layout_out::notes(&safe)
    } else {
        Vec::new()
    };
    let texts = model
        .texts()
        .iter()
        .filter(|t| t.text.contains(['\n', '\r', '"']) || t.text.contains("*/"))
        .count();
    if texts > 0 {
        notes.push(format!(
            "{texts} text{} written on one line, with ' for \" (a diagram's strings can't hold line breaks or double quotes)",
            if texts == 1 { " is" } else { "s are" }
        ));
    }
    renames_note(&mut notes, &renamed);
    notes
}

/// Puts first the note on ids written with `_` added, if any were.
fn renames_note(notes: &mut Vec<String>, renamed: &[(String, String)]) {
    if !renamed.is_empty() {
        let list: Vec<String> = renamed.iter().map(|(a, b)| format!("{a} as {b}")).collect();
        notes.insert(
            0,
            format!(
                "Ids Antimony reserves were written with `_` added: {}",
                list.join(", ")
            ),
        );
    }
}

/// Words libAntimony 3.2.1 won't take as an id, checked against it
/// (2026-10-23): its keywords, constants and function names. Our own
/// Antimony reader takes them all, so text it reads can still be rejected by
/// libAntimony.
const RESERVED: &[&str] = &[
    "DNA",
    "INF",
    "NaN",
    "abs",
    "acos",
    "after",
    "and",
    "arccos",
    "arccosh",
    "arccot",
    "arccoth",
    "arccsc",
    "arccsch",
    "arcsec",
    "arcsech",
    "arcsin",
    "arcsinh",
    "arctan",
    "arctanh",
    "asin",
    "at",
    "atan",
    "avogadro",
    "ceil",
    "ceiling",
    "compartment",
    "const",
    "constraint",
    "cos",
    "cosh",
    "cot",
    "coth",
    "csc",
    "csch",
    "delay",
    "delete",
    "divide",
    "end",
    "eq",
    "event",
    "exp",
    "exponentiale",
    "ext",
    "factorial",
    "false",
    "floor",
    "formula",
    "function",
    "gene",
    "geq",
    "gt",
    "has",
    "implies",
    "import",
    "in",
    "inf",
    "infinity",
    "is",
    "leq",
    "ln",
    "log",
    "log10",
    "lt",
    "max",
    "maximize",
    "min",
    "minimize",
    "minus",
    "model",
    "module",
    "nan",
    "neq",
    "not",
    "notanumber",
    "operator",
    "or",
    "pi",
    "piecewise",
    "plus",
    "pow",
    "power",
    "quotient",
    "rateOf",
    "reaction",
    "rem",
    "root",
    "sec",
    "sech",
    "sin",
    "sinh",
    "species",
    "sqrt",
    "substanceOnly",
    "tan",
    "tanh",
    "time",
    "times",
    "true",
    "unit",
    "var",
    "xor",
];

/// The model with every id libAntimony reserves renamed by adding `_` (more
/// if that is taken), as libAntimony's own SBML conversion does
/// (`compartment` becomes `compartment_`), everywhere it is used; and what
/// was renamed. The diagram itself is not changed: export works on a copy.
fn safe_ids(model: &Model) -> (std::borrow::Cow<'_, Model>, Vec<(String, String)>) {
    use crate::model::replace_whole_word;
    let reserved = |id: &str| RESERVED.contains(&id);
    let used = |m: &Model, id: &str| {
        m.is_id_used(id) || m.assignment_rules.iter().any(|r| r.variable == id)
    };
    let any = model
        .species()
        .iter()
        .any(|s| !s.is_null_node && reserved(&s.id))
        || model.reactions().iter().any(|r| reserved(&r.id))
        || model.compartments.iter().any(|c| reserved(&c.id))
        || model.parameters.iter().any(|p| reserved(&p.variable))
        || model.assignment_rules.iter().any(|r| reserved(&r.variable))
        || reserved(&model.model_name);
    if !any {
        return (std::borrow::Cow::Borrowed(model), Vec::new());
    }
    let mut m = model.clone();
    let mut renamed = Vec::new();
    let fresh = |m: &Model, id: &str| {
        let mut new = format!("{id}_");
        while used(m, &new) {
            new.push('_');
        }
        new
    };
    let ids: Vec<String> = m
        .species()
        .iter()
        .filter(|s| !s.is_null_node && !s.is_alias() && reserved(&s.id))
        .map(|s| s.id.clone())
        .collect();
    for old in ids {
        let new = fresh(&m, &old);
        if m.rename_species(&old, &new).is_ok() {
            // Rate laws are done; values and rules too.
            for p in &mut m.parameters {
                p.expression = replace_whole_word(&p.expression, &old, &new);
            }
            for r in &mut m.assignment_rules {
                r.expression = replace_whole_word(&r.expression, &old, &new);
                if r.variable == old {
                    r.variable = new.clone();
                }
            }
            renamed.push((old, new));
        }
    }
    let ids: Vec<String> = m
        .compartments
        .iter()
        .filter(|c| reserved(&c.id))
        .map(|c| c.id.clone())
        .collect();
    for old in ids {
        let new = fresh(&m, &old);
        if m.rename_compartment(&old, &new).is_ok() {
            renamed.push((old, new));
        }
    }
    let ids: Vec<String> = m
        .reactions()
        .iter()
        .filter(|r| reserved(&r.id))
        .map(|r| r.id.clone())
        .collect();
    for old in ids {
        let new = fresh(&m, &old);
        if m.rename_reaction(&old, &new).is_ok() {
            renamed.push((old, new));
        }
    }
    // Parameters, and rules that set a name nothing else uses.
    let ids: Vec<String> = m
        .parameters
        .iter()
        .map(|p| p.variable.clone())
        .chain(m.assignment_rules.iter().map(|r| r.variable.clone()))
        .filter(|id| reserved(id))
        .collect();
    for old in ids {
        if renamed.iter().any(|(o, _)| *o == old) {
            continue;
        }
        let new = fresh(&m, &old);
        for p in &mut m.parameters {
            if p.variable == old {
                p.variable = new.clone();
            }
            p.expression = replace_whole_word(&p.expression, &old, &new);
        }
        for r in &mut m.assignment_rules {
            if r.variable == old {
                r.variable = new.clone();
            }
            r.expression = replace_whole_word(&r.expression, &old, &new);
        }
        for r in m.reactions_mut() {
            r.kinetic_law = replace_whole_word(&r.kinetic_law, &old, &new);
        }
        renamed.push((old, new));
    }
    if reserved(&m.model_name) {
        let new = format!("{}_", m.model_name);
        renamed.push((m.model_name.clone(), new.clone()));
        m.model_name = new;
    }
    (std::borrow::Cow::Owned(m), renamed)
}

/// How many species an export declares on one line.
const SPECIES_PER_LINE: usize = 4;

fn write_model(model: &Model, layout: Vec<String>) -> String {
    let name = model_name(model);
    let compartments: Vec<String> = model
        .compartments
        .iter()
        .filter(|c| !c.id.eq_ignore_ascii_case(DEFAULT_COMPARTMENT))
        .map(|c| format!("compartment {} = {};", c.id, number(c.size)))
        .collect();
    let primaries: Vec<&SpeciesNode> = model
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node)
        .collect();
    // Four to a line (`species $Glc in cell, G6P in cell, …;`), in the
    // model's order. `const` makes a species constant, and a boundary species
    // with it, so only a constant boundary species is written so (S §11.3);
    // since it is said once for a line, those go on lines of their own.
    let declared: Vec<(bool, String)> = primaries
        .iter()
        .map(|s| {
            let boundary = if s.is_boundary { "$" } else { "" };
            let mut one = format!("{boundary}{}", s.id);
            if !s.compartment.is_empty() && !s.compartment.eq_ignore_ascii_case(DEFAULT_COMPARTMENT)
            {
                let _ = write!(one, " in {}", s.compartment);
            }
            (s.is_constant && s.is_boundary, one)
        })
        .collect();
    let mut species: Vec<String> = Vec::new();
    for run in declared.chunk_by(|a, b| a.0 == b.0) {
        let constant = if run[0].0 { "const " } else { "" };
        for four in run.chunks(SPECIES_PER_LINE) {
            let names: Vec<&str> = four.iter().map(|(_, n)| n.as_str()).collect();
            species.push(format!("{constant}species {};", names.join(", ")));
        }
    }
    // `[n ]A + [n ]B`, through aliases to the species, null nodes left out.
    let side = |list: &[Participant]| {
        list.iter()
            .filter_map(|p| {
                let node = model.find_species(&p.species)?;
                if node.is_null_node {
                    return None;
                }
                let primary = model.find_species(node.primary_id()).unwrap_or(node);
                let n = if p.stoichiometry == 1.0 {
                    String::new()
                } else {
                    format!("{} ", p.stoichiometry)
                };
                Some(format!(
                    "{n}{}{}",
                    if primary.is_boundary { "$" } else { "" },
                    primary.id
                ))
            })
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let reactions: Vec<String> = model
        .reactions()
        .iter()
        .map(|r| {
            let law = r.kinetic_law.trim();
            let rate = if law.is_empty() { "v" } else { law };
            let arrow = if r.is_reversible { "->" } else { "=>" };
            format!(
                "{}: {} {arrow} {}; {rate};",
                r.id,
                side(&r.reactants),
                side(&r.products)
            )
        })
        .collect();
    // Activators and inhibitors are not written as Antimony's interactions
    // (`X -| J1`): people don't write them and libAntimony is fussy about
    // them. Their kind goes in the diagram comment instead (`kind inhibitor`).
    let parameters: Vec<String> = model
        .parameters
        .iter()
        .map(|p| format!("{} = {};", p.variable, p.expression.trim()))
        .collect();
    let initial: Vec<String> = primaries
        .iter()
        .map(|s| format!("{} = {};", s.id, number(s.initial_value)))
        .collect();
    let rules: Vec<String> = model
        .assignment_rules
        .iter()
        .map(|r| format!("{} := {};", r.variable, r.expression.trim()))
        .collect();

    // `S1 is "glucose";` (docs/display-names-design.md D1), in the order of
    // the sections above.
    let named = model
        .compartments
        .iter()
        .filter(|c| !c.id.eq_ignore_ascii_case(DEFAULT_COMPARTMENT))
        .map(|c| (&c.id, &c.display_name))
        .chain(primaries.iter().map(|s| (&s.id, &s.display_name)))
        .chain(model.reactions().iter().map(|r| (&r.id, &r.display_name)));
    let names: Vec<String> = named
        .filter_map(|(id, name)| Some(format!("{id} is {};", quoted_name(name.as_deref()?))))
        .collect();

    let mut out = format!("model {name}\n");
    let mut first = true;
    for (title, lines) in [
        ("Compartments", compartments),
        ("Species", species),
        ("Reactions", reactions),
        ("Parameters", parameters),
        ("Initial values", initial),
        ("Assignment rules", rules),
        ("Display names", names),
        ("Layout", layout),
    ] {
        if lines.is_empty() {
            continue;
        }
        if !first {
            out.push('\n');
        }
        first = false;
        let _ = writeln!(out, "  // {title}");
        for line in lines {
            let _ = writeln!(out, "  {line}");
        }
    }
    out.push_str("end\n");
    out
}

fn is_identifier(s: &str) -> bool {
    let mut b = s.bytes();
    b.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && b.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

/// A number as Antimony text: shortest form, `.` as the decimal separator.
fn number(v: f64) -> String {
    format!("{v}")
}

/// One side of a reaction: species ids with their stoichiometry.
type Side = Vec<(String, f64)>;

/// The display name `id` is given (`S1 is "glucose"`), if any.
fn display_name_of(module: &Module, id: &str) -> Option<String> {
    let symbol = module.symbols.iter().find(|s| s.name == id)?;
    crate::model::display_name(symbol.display_name.clone())
}

/// A display name as an Antimony string: on one line, with ' for ", which
/// libAntimony's strings can't be relied on to hold.
fn quoted_name(name: &str) -> String {
    let one_line: String = name
        .chars()
        .map(|c| match c {
            '"' => '\'',
            '\n' | '\r' | '\t' => ' ',
            c => c,
        })
        .collect();
    format!("\"{one_line}\"")
}

/// What is known about one species while building.
struct SpeciesInfo {
    id: String,
    boundary: bool,
    /// Declared `const` (S §11.2).
    constant: bool,
    compartment: String,
    initial: f64,
}

struct Builder<'a> {
    src: &'a str,
    module: &'a Module,
    errors: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,
    /// The `function` statements, written into the expressions that call
    /// them (S §11.2).
    functions: HashMap<String, expr::FunctionDef>,
}

impl Builder<'_> {
    fn warning(&mut self, span: Span, message: impl Into<String>) {
        self.warnings
            .push(Diagnostic::at(Severity::Warning, span, message));
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.errors
            .push(Diagnostic::at(Severity::Error, span, message));
    }

    /// The text an expression was written as.
    fn text(&self, e: &antimony::expr::Expr) -> String {
        e.span.slice_of(self.src).trim().to_owned()
    }

    /// A number written in the text, or an error. A sign is part of the
    /// number; anything more is an expression, which a diagram's species or
    /// compartment can't hold.
    fn number(&mut self, e: &antimony::expr::Expr, what: &str) -> Option<f64> {
        let text = self.text(e);
        match text.parse::<f64>() {
            Ok(v) if v.is_finite() => Some(v),
            _ => {
                self.error(e.span, format!("{what} must be a number, not '{text}'"));
                None
            }
        }
    }

    /// An expression, parsed: the AST, or an error at the right place.
    fn expression(&mut self, e: &antimony::expr::Expr) -> Option<expr::Ast> {
        match expr::parse(&self.text(e)) {
            Ok(ast) => Some(ast),
            Err(err) => {
                let written = e.span.slice_of(self.src);
                let at = e.span.start.offset + written.len() - written.trim_start().len()
                    + err.span.start;
                let pos = antimony::span::LineIndex::new(self.src).pos(self.src, at);
                self.error(Span::from_pos(pos, 1), err.message);
                None
            }
        }
    }

    /// An expression with the functions it calls written into it.
    fn expanded(&mut self, e: &antimony::expr::Expr) -> Option<expr::Ast> {
        let ast = self.expression(e)?;
        match expr::expand_functions(&ast, &self.functions) {
            Ok(ast) => Some(ast),
            Err(why) => {
                self.error(e.span, format!("This uses {why}"));
                None
            }
        }
    }

    /// The text of an expression, as written unless it calls a function,
    /// which is written into it.
    fn expanded_text(&mut self, e: &antimony::expr::Expr) -> String {
        match (self.expression(e), self.expanded(e)) {
            (Some(written), Some(expanded)) if written != expanded => expr::to_infix(&expanded),
            _ => self.text(e),
        }
    }

    fn build(&mut self, opts: &ImportOptions) -> Model {
        let module = self.module;
        for f in &module.functions {
            if let Some(body) = f.body.as_ref().and_then(|b| self.expression(b)) {
                let def = expr::FunctionDef {
                    params: f.args.clone(),
                    body,
                };
                self.functions.insert(f.name.clone(), def);
            }
        }
        let mut model = Model::new();
        model.model_name = module
            .name
            .clone()
            .unwrap_or_else(|| "anonymous".to_owned());

        // Species, in the order the text first mentions them. Names typed in
        // another case resolve to the first spelling.
        let mut species: Vec<SpeciesInfo> = Vec::new();
        let mut by_lower: HashMap<String, usize> = HashMap::new();
        let mut resolve = |id: &str, boundary: bool, species: &mut Vec<SpeciesInfo>| -> String {
            let i = *by_lower.entry(id.to_ascii_lowercase()).or_insert_with(|| {
                species.push(SpeciesInfo {
                    id: id.to_owned(),
                    boundary: false,
                    constant: false,
                    compartment: String::new(),
                    initial: 0.0,
                });
                species.len() - 1
            });
            species[i].boundary |= boundary;
            species[i].id.clone()
        };
        // A `$` on a name that is not a species (`$B = 4`, which Antimony
        // reads as `const B`) has always meant a boundary species here.
        let species_like = module.symbols.iter().filter(|s| {
            s.kind == SymbolKind::Species || (s.is_boundary && s.kind == SymbolKind::Parameter)
        });
        for s in species_like {
            let id = resolve(&s.name, s.is_boundary, &mut species);
            let info = species
                .iter_mut()
                .find(|x| x.id == id)
                .expect("just resolved");
            info.constant |= s.constness == antimony::model::Constness::Const;
            if let Some(c) = &s.compartment {
                info.compartment = c.clone();
            }
        }
        let reactions: Vec<(Side, Side)> = module
            .reactions
            .iter()
            .map(|r| {
                let mut side = |terms: &[Term]| {
                    terms
                        .iter()
                        .map(|t| {
                            (
                                resolve(&t.species, t.is_boundary, &mut species),
                                module.stoichiometry(t),
                            )
                        })
                        .collect::<Vec<_>>()
                };
                (side(&r.reactants), side(&r.products))
            })
            .collect();

        // Compartments.
        let mut compartments: Vec<Compartment> = module
            .compartments()
            .map(|c| Compartment {
                display_name: crate::model::display_name(c.display_name.clone()),
                ..Compartment::new(c.name.clone())
            })
            .collect();

        // `x = expr`, in the order written: a species' initial value, a
        // compartment's size, or a parameter.
        let mut assignments: Vec<(&Symbol, &antimony::expr::Expr)> = module
            .symbols
            .iter()
            .filter_map(|s| Some((s, s.initial_value.as_ref()?)))
            .collect();
        assignments.sort_by_key(|(_, e)| e.span.start.offset);
        let mut parameters: Vec<Parameter> = Vec::new();
        for (s, value) in assignments {
            let compartment = compartments
                .iter()
                .position(|c| c.id.eq_ignore_ascii_case(&s.name));
            let as_species = (s.kind != SymbolKind::Compartment)
                .then(|| by_lower.get(&s.name.to_ascii_lowercase()).copied())
                .flatten();
            if let Some(i) = as_species {
                if let Some(v) = self.number(value, "A species' initial value") {
                    species[i].initial = v;
                }
            } else if let Some(c) = compartment {
                if let Some(v) = self.number(value, "A compartment's size") {
                    compartments[c].size = v;
                }
            } else {
                let text = self.expanded_text(value);
                match parameters.iter_mut().find(|p| p.variable == s.name) {
                    Some(p) => p.expression = text,
                    None => parameters.push(Parameter {
                        variable: s.name.clone(),
                        expression: text,
                    }),
                }
            }
        }
        // `x := expr`, in the order written.
        let mut rule_exprs: Vec<(&Symbol, &antimony::expr::Expr)> = module
            .symbols
            .iter()
            .filter_map(|s| Some((s, s.assignment_rule()?)))
            .collect();
        rule_exprs.sort_by_key(|(_, e)| e.span.start.offset);
        let rules: Vec<AssignmentRule> = rule_exprs
            .into_iter()
            .map(|(s, e)| AssignmentRule {
                variable: s.name.clone(),
                expression: self.expanded_text(e),
            })
            .collect();

        // Rate laws, tidied by the expression parser.
        let laws: Vec<Option<expr::Ast>> = module
            .reactions
            .iter()
            .map(|r| r.kinetic_law.as_ref().and_then(|e| self.expanded(e)))
            .collect();

        // A name in a rate law that is nothing else is a parameter, 0.
        for ast in laws.iter().flatten() {
            for name in expr::identifiers(ast) {
                let known = species.iter().any(|s| s.id == name)
                    || parameters
                        .iter()
                        .any(|p| p.variable.eq_ignore_ascii_case(name))
                    || compartments.iter().any(|c| c.id.eq_ignore_ascii_case(name))
                    || rules.iter().any(|r| r.variable == name);
                if !known {
                    parameters.push(Parameter {
                        variable: name.to_owned(),
                        expression: "0".to_owned(),
                    });
                }
            }
        }

        model.compartments = compartments;
        model.parameters = parameters;
        model.assignment_rules = rules;

        // One node per species, at random.
        let mut rng = Rng::new(opts.seed);
        for s in &species {
            let mut node = SpeciesNode::new(
                s.id.clone(),
                pt(rng.range(0.0, PLACEMENT), rng.range(0.0, PLACEMENT)),
            );
            node.is_boundary = s.boundary;
            node.is_constant = s.constant;
            node.compartment = s.compartment.clone();
            node.initial_value = s.initial;
            node.display_name = display_name_of(module, &s.id);
            if model.push_species(node).is_err() {
                // Unreachable: names were made unique above.
                continue;
            }
        }

        // Reactions. One the text leaves unnamed is named `R1`, `R2`, …,
        // stepping past the names the text uses.
        let declared: Vec<&str> = module
            .reactions
            .iter()
            .filter(|r| !r.name_generated)
            .map(|r| r.name.as_str())
            .collect();
        let mut next_generated = 1;
        for ((r, (reactants, products)), law) in module.reactions.iter().zip(reactions).zip(laws) {
            let id = if r.name_generated {
                loop {
                    let id = format!("R{next_generated}");
                    next_generated += 1;
                    if !declared.contains(&id.as_str()) && !model.is_id_used(&id) {
                        break id;
                    }
                }
            } else {
                r.name.clone()
            };
            if model.find_reaction(&id).is_some() {
                let mut line = r.span;
                line.start.column = 1;
                self.error(line, format!("a second reaction is called '{id}'"));
                continue;
            }
            // `=>` is irreversible; `->` and `<->` are reversible (S §11.2).
            let reversible = r.kind != antimony::model::ReactionKind::Irreversible;
            self.add_reaction(
                &mut model,
                &id,
                (reactants, products),
                reversible,
                law,
                opts,
            );
        }
        self.interactions(&mut model);
        for r in model.reactions_mut() {
            r.display_name = display_name_of(module, &r.id);
        }
        model.recompute_id_counter();
        model
    }

    /// `X -| J1` makes `X` an inhibitor of `J1`, `X -o J1` an activator
    /// (S §5.7.1): `X` becomes a modifier of `J1` if it isn't one. One that
    /// names no reaction, or no species, is skipped with a warning.
    fn interactions(&mut self, model: &mut Model) {
        use antimony::model::InteractionKind;
        for i in &self.module.interactions {
            let kind = match i.kind {
                InteractionKind::Inhibits => ModifierKind::Inhibitor,
                InteractionKind::Activates => ModifierKind::Activator,
                InteractionKind::Unknown => ModifierKind::Modifier,
            };
            let is_species = model
                .find_species(&i.species)
                .is_some_and(|s| !s.is_null_node);
            let Some(r) = model.find_reaction_mut(&i.reaction) else {
                self.warning(
                    i.span,
                    format!(
                        "'{}' isn't a reaction; this interaction was skipped",
                        i.reaction
                    ),
                );
                continue;
            };
            if !is_species {
                self.warning(
                    i.span,
                    format!(
                        "'{}' isn't a species; this interaction was skipped",
                        i.species
                    ),
                );
                continue;
            }
            if r.reactants
                .iter()
                .chain(&r.products)
                .any(|p| p.species == i.species)
            {
                self.warning(
                    i.span,
                    format!(
                        "'{}' is already a reactant or product of '{}'; this interaction was skipped",
                        i.species, i.reaction
                    ),
                );
                continue;
            }
            match r.modifiers.iter_mut().find(|m| m.species == i.species) {
                Some(m) => m.kind = kind,
                None => {
                    let mut p = Participant::new(i.species.as_str(), 1.0);
                    p.kind = kind;
                    r.modifiers.push(p);
                }
            }
        }
    }

    fn add_reaction(
        &mut self,
        model: &mut Model,
        id: &str,
        (reactants, products): (Side, Side),
        reversible: bool,
        law: Option<expr::Ast>,
        opts: &ImportOptions,
    ) {
        let centre = |list: &[(String, f64)]| {
            let pts: Vec<Pt> = list
                .iter()
                .filter_map(|(s, _)| model.find_species(s))
                .map(|s| s.center)
                .collect();
            (!pts.is_empty()).then(|| pts.iter().fold(Pt::ZERO, |a, &p| a + p) / pts.len() as f64)
        };
        let junction = match (centre(&reactants), centre(&products)) {
            (Some(a), Some(b)) => (a + b) / 2.0,
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => pt(PLACEMENT / 2.0, PLACEMENT / 2.0),
        };

        let mut r = crate::model::Reaction::new(id, junction);
        r.mode = ReactionMode::Straight;
        r.is_reversible = reversible;
        r.reactants = reactants
            .iter()
            .map(|(s, n)| Participant::new(s.as_str(), *n))
            .collect();
        r.products = products
            .iter()
            .map(|(s, n)| Participant::new(s.as_str(), *n))
            .collect();
        if opts.convert_catalytic {
            passes::catalytic_rewrite(&mut r);
        }
        if let Some(ast) = &law {
            r.kinetic_law = expr::to_infix(ast);
            passes::modifiers_from_law(model, &mut r, ast);
        }
        passes::add_null_nodes(model, &mut r, None, None);
        model.push_reaction(r).expect("checked unique above");
    }
}

/// The species, reaction, parameter or compartment ids that a model's text
/// uses, for tests.
#[cfg(test)]
fn ids(m: &Model) -> Vec<String> {
    m.species().iter().map(|s| s.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ids libAntimony reserves: the model, a compartment, a species, a
    /// reaction, a parameter and a rule's name.
    const RESERVED_IDS: &str = "model model\n  compartment compartment = 1\n  \
        species time in compartment, S2 in compartment\n  \
        reaction: time -> S2; pi*time*compartment\n  pi = 3\n  max := 2*pi*time\nend\n";

    #[test]
    fn reserved_ids_are_written_with_an_underscore() {
        let m = ok(RESERVED_IDS, &OFF);
        let text = export(&m);
        for line in [
            "model model_",
            "compartment compartment_ = 1;",
            "species time_ in compartment_, S2 in compartment_;",
            "reaction_: time_ -> S2; pi_*time_*compartment_;",
            "pi_ = 3;",
            "max_ := 2*pi_*time_;",
        ] {
            assert!(text.contains(line), "{line} not in:\n{text}");
        }
        // The diagram itself keeps its ids.
        assert!(m.find_species("time").is_some());
        // Read back, the model is the same but for the names.
        let back = ok(&text, &OFF);
        assert_eq!(
            back.find_reaction("reaction_").unwrap().kinetic_law,
            "pi_*time_*compartment_"
        );
    }

    #[test]
    fn the_layout_uses_the_new_ids_and_the_notes_say_so() {
        let m = ok(RESERVED_IDS, &OFF);
        let text = export_with_layout(&m);
        assert!(text.contains("time_.position = "), "{text}");
        assert!(text.contains("reaction_.position = "), "{text}");
        assert!(!text.contains("\n  time.position"), "{text}");
        let notes = layout_notes(&m);
        assert!(
            notes[0].starts_with("Ids Antimony reserves were written with `_` added:")
                && notes[0].contains("compartment as compartment_"),
            "{notes:?}"
        );
    }

    #[test]
    fn a_taken_name_gets_another_underscore() {
        let m = ok("J1: time -> time_; k*time\nk = 1\n", &OFF);
        let text = export(&m);
        assert!(text.contains("J1: time__ -> time_; k*time__;"), "{text}");
    }

    const ON: ImportOptions = ImportOptions {
        convert_catalytic: true,
        seed: 1,
    };
    const OFF: ImportOptions = ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };

    fn ok(src: &str, opts: &ImportOptions) -> Model {
        match import(src, opts) {
            Ok(i) => i.model,
            Err(e) => panic!("{src}: {e:?}"),
        }
    }

    fn reaction<'a>(m: &'a Model, id: &str) -> &'a crate::model::Reaction {
        m.find_reaction(id).unwrap()
    }

    fn sides(m: &Model, id: &str) -> (Side, Side, Vec<String>) {
        let r = reaction(m, id);
        let f = |l: &[Participant]| {
            l.iter()
                .map(|p| (p.species.clone(), p.stoichiometry))
                .collect()
        };
        (
            f(&r.reactants),
            f(&r.products),
            r.modifiers.iter().map(|p| p.species.clone()).collect(),
        )
    }

    #[test]
    fn the_arrow_says_whether_a_reaction_is_reversible() {
        let src = "model m\n  J1: A -> B; k*A\n  J2: B => C; k*B\n  J3: C <-> D; k*C\nend";
        let m = ok(src, &OFF);
        let rev = |id: &str| reaction(&m, id).is_reversible;
        assert!(rev("J1"), "-> is reversible");
        assert!(!rev("J2"), "=> is irreversible");
        assert!(rev("J3"), "<-> is reversible");
        // Written back with the same arrows, and read back the same.
        let text = export(&m);
        assert!(text.contains("J1: A -> B; k*A;"), "{text}");
        assert!(text.contains("J2: B => C; k*B;"), "{text}");
        assert!(text.contains("J3: C -> D; k*C;"), "{text}");
        let again = ok(&text, &OFF);
        for id in ["J1", "J2", "J3"] {
            assert_eq!(reaction(&again, id).is_reversible, rev(id), "{id}");
        }
    }

    #[test]
    fn interactions_set_modifier_kinds() {
        use crate::model::ModifierKind as K;
        let src = "model m\n  J1: A -> B; k*A*X\n  J2: B -> C; k*B\n  X -| J1\n  Y -o J2\n  \
                   species Y\n  Y = 1\n  Z -| J9\n  A -| J1\nend";
        let imported = import(src, &OFF).unwrap();
        let m = &imported.model;
        let kind = |r: &str, s: &str| {
            m.find_reaction(r)
                .unwrap()
                .modifiers
                .iter()
                .find(|p| p.species == s)
                .map(|p| p.kind)
        };
        // X was a modifier already (from the law); Y is added as one.
        assert_eq!(kind("J1", "X"), Some(K::Inhibitor));
        assert_eq!(kind("J2", "Y"), Some(K::Activator));
        // No reaction J9, and A is a reactant: each skipped. (Antimony
        // itself rejects an interaction on anything but a species.)
        let w: Vec<&str> = imported
            .warnings
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            w.iter().any(|w| w.contains("'J9' isn't a reaction")),
            "{w:?}"
        );
        assert!(
            w.iter().any(|w| w.contains("'A' is already a reactant")),
            "{w:?}"
        );
        assert_eq!(kind("J1", "A"), None);
        assert!(
            !w.iter()
                .any(|w| w.contains("Interactions aren't supported"))
        );
        // Not written back as interactions: X's kind goes in the diagram,
        // and comes back from it. Y, named in no rate law, has no place in
        // the Antimony text.
        let text = export(m);
        assert!(!text.contains("-|") && !text.contains("-o"), "{text}");
        assert!(!text.contains("Interactions"), "{text}");
        let with_diagram = export_with_diagram(m);
        assert!(
            with_diagram.contains("line X to J1 kind inhibitor"),
            "{with_diagram}"
        );
        let again = import(&with_diagram, &OFF).unwrap().model;
        assert_eq!(
            again.find_reaction("J1").unwrap().modifiers[0].kind,
            K::Inhibitor
        );
        assert!(again.find_reaction("J2").unwrap().modifiers.is_empty());
    }

    #[test]
    fn a_simple_model() {
        let m = ok(
            "model demo\n  S1 -> S2; k1*S1\n  k1 = 0.1\n  S1 = 10\nend",
            &ON,
        );
        assert_eq!(m.model_name, "demo");
        assert_eq!(ids(&m), ["S1", "S2"]);
        assert_eq!(m.find_species("S1").unwrap().initial_value, 10.0);
        assert_eq!(
            m.find_species("S2").unwrap().initial_value,
            0.0,
            "undeclared: 0"
        );
        assert_eq!(m.find_parameter("k1").unwrap().expression, "0.1");
        let r = reaction(&m, "R1");
        assert_eq!(r.kinetic_law, "k1*S1");
        assert_eq!(r.mode, ReactionMode::Straight);
        assert_eq!(m.species().iter().filter(|s| s.width == 80.0).count(), 2);
        assert!(
            m.species()
                .iter()
                .all(|s| (0.0..600.0).contains(&s.center.x) && (0.0..600.0).contains(&s.center.y))
        );
        let j = r.junction;
        let mid =
            (m.find_species("S1").unwrap().center + m.find_species("S2").unwrap().center) / 2.0;
        assert!(j.distance(mid) < 1e-9);
    }

    #[test]
    fn without_the_wrapper_the_name_is_anonymous() {
        assert_eq!(ok("A -> B", &ON).model_name, "anonymous");
    }

    #[test]
    fn catalytic_conversion_on_and_off() {
        // M8 acceptance: A -> A + B; k*A.
        let m = ok("R1: A -> A + B; k*A", &ON);
        let (r, p, mods) = sides(&m, "R1");
        assert_eq!(r, [("_source_R1".to_owned(), 1.0)]);
        assert_eq!(p, [("B".to_owned(), 1.0)]);
        assert_eq!(mods, ["A"]);
        assert!(m.find_species("_source_R1").unwrap().is_null_node);

        // Without a rate law, A is still the catalyst: a modifier.
        let m = ok("R1: A -> A + B", &ON);
        assert_eq!(sides(&m, "R1").2, ["A"]);

        let m = ok("R1: A -> A + B; k*A", &OFF);
        let (r, p, mods) = sides(&m, "R1");
        assert_eq!(r, [("A".to_owned(), 1.0)]);
        assert_eq!(p, [("A".to_owned(), 1.0), ("B".to_owned(), 1.0)]);
        assert!(mods.is_empty());
    }

    #[test]
    fn conversion_needs_equal_stoichiometry() {
        let m = ok("R1: A -> 2 A + B; k*A", &ON);
        let (r, _, _) = sides(&m, "R1");
        assert_eq!(r, [("A".to_owned(), 1.0)]);
        // `S4 + S4 -> S2 + S2` keeps everything; `S3 + S4 -> S4` loses S4.
        let m = ok("R1: S4 + S4 -> S2 + S2; k\nR2: S3 + S4 -> S4; k*S3*S4", &ON);
        assert_eq!(reaction(&m, "R1").reactants.len(), 2);
        let (r, p, mods) = sides(&m, "R2");
        assert_eq!(r, [("S3".to_owned(), 1.0)]);
        assert_eq!(p, [("_sink_R2".to_owned(), 1.0)]);
        assert_eq!(mods, ["S4"]);
    }

    #[test]
    fn null_nodes_for_empty_sides() {
        // M8 acceptance: `-> A` and `A ->`.
        let m = ok("R1: -> A; k\nR2: A -> ; k*A", &ON);
        let src = m.find_species("_source_R1").unwrap();
        assert!(src.is_null_node && src.width == 28.0 && src.height == 28.0);
        assert!(m.find_species("_sink_R2").unwrap().is_null_node);
        assert_eq!(reaction(&m, "R1").reactants[0].species, "_source_R1");
        assert_eq!(reaction(&m, "R2").products[0].species, "_sink_R2");
    }

    #[test]
    fn names_in_another_case_resolve_to_the_declared_spelling() {
        let m = ok("species Glucose\nR1: glucose -> G6P; k*Glucose", &ON);
        assert_eq!(ids(&m), ["Glucose", "G6P"]);
        assert_eq!(reaction(&m, "R1").reactants[0].species, "Glucose");
    }

    #[test]
    fn implied_parameters_and_modifiers() {
        let m = ok("R1: S -> P; Vm*S/(Km + S)*E\nspecies E\nKm = 0.5", &ON);
        assert_eq!(m.find_parameter("Vm").unwrap().expression, "0");
        assert_eq!(m.find_parameter("Km").unwrap().expression, "0.5");
        assert!(m.find_parameter("E").is_none(), "E is a species");
        assert_eq!(sides(&m, "R1").2, ["E"]);
        assert_eq!(reaction(&m, "R1").kinetic_law, "Vm*S/(Km + S)*E");
    }

    #[test]
    fn assignments_to_species_compartments_and_parameters() {
        let m = ok(
            "compartment cell\nspecies A in cell\ncell = 2\nA = 1.5\nk = 2*3\nT := A + 1\n$B = 4",
            &ON,
        );
        assert_eq!(m.find_compartment("cell").unwrap().size, 2.0);
        let a = m.find_species("A").unwrap();
        assert_eq!((a.initial_value, a.compartment.as_str()), (1.5, "cell"));
        assert_eq!(m.find_parameter("k").unwrap().expression, "2*3");
        assert_eq!(m.assignment_rules[0].expression, "A + 1");
        let b = m.find_species("B").unwrap();
        assert!(b.is_boundary && b.initial_value == 4.0);
    }

    #[test]
    fn reaction_ids_declared_or_generated() {
        let m = ok("A -> B; k\nR1: B -> C; k\nC -> D; k", &ON);
        let ids: Vec<&str> = m.reactions().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["R2", "R1", "R3"]);
    }

    #[test]
    fn errors_leave_no_model() {
        // M8 acceptance: a syntax error is reported with where it is.
        let e = import("A -> B; k\nC + -> D", &ON).unwrap_err();
        assert_eq!((e[0].line, e[0].column), (2, 5));
        // A bad rate law points into the law.
        let e = import("R1: A -> B; k*(A +\n", &ON).unwrap_err();
        assert_eq!(e[0].line, 1);
        assert!(e[0].column > 12);
        // A species' value must be a number.
        let e = import("species A\nA = k*2", &ON).unwrap_err();
        assert!(e[0].message.contains("must be a number"));
        assert_eq!((e[0].line, e[0].column), (2, 5));
        // Two reactions with one id.
        let e = import("R1: A -> B\nR1: B -> C", &ON).unwrap_err();
        assert_eq!(e[0].line, 2);
    }

    #[test]
    fn warnings_dont_stop_the_import() {
        let i = import("unit mM = 1e-3\nA -> B; k", &ON).unwrap();
        assert_eq!(i.warnings.len(), 1);
        assert_eq!(i.model.reactions().len(), 1);
    }

    #[test]
    fn what_the_diagram_cant_hold_is_skipped_with_a_warning() {
        let i = import(
            "function f(x)\n  x^2\nend\nunit mM = 1e-3 mole/litre\nE1: at (time > 5): S1 = 3\nS1 is \"glucose\"\nA -> B; k",
            &ON,
        )
        .unwrap();
        let lines: Vec<usize> = i.warnings.iter().map(|w| w.line).collect();
        assert_eq!(lines, [1, 4, 5, 6], "{:?}", i.warnings);
        assert_eq!(i.model.reactions().len(), 1);
    }

    /// D1 of docs/display-names-design.md: `x is "…"` on a species,
    /// compartment or reaction is kept, written back in its own section, and
    /// read again.
    #[test]
    fn display_names_round_trip() {
        let i = import(
            "compartment cell = 1
species S in cell
J1: S -> ; k*S
k = 1
             cell is \"astrocyte\"
S is \"L-glutamate\"
J1 is \"uptake\"",
            &ON,
        )
        .unwrap();
        assert!(i.warnings.is_empty(), "{:?}", i.warnings);
        let m = ok(
            "compartment cell = 1\nspecies S in cell, P in cell\n\
             J1: S -> P; k*S\nk = 1\n\
             cell is \"astrocyte\"\nS is \"L-glutamate\"\nJ1 is \"glutamate uptake\"",
            &ON,
        );
        let name = |id: &str| m.find_species(id).unwrap().display_name.clone();
        assert_eq!(name("S").as_deref(), Some("L-glutamate"));
        assert_eq!(name("P"), None);
        assert_eq!(
            m.find_reaction("J1").unwrap().display_name.as_deref(),
            Some("glutamate uptake")
        );
        assert_eq!(
            m.find_compartment("cell").unwrap().display_name.as_deref(),
            Some("astrocyte")
        );
        let text = export(&m);
        assert!(
            text.contains(
                "  // Display names\n  cell is \"astrocyte\";\n  S is \"L-glutamate\";\n  \
                 J1 is \"glutamate uptake\";\n"
            ),
            "{text}"
        );
        let back = ok(&text, &ON);
        assert_eq!(
            back.find_species("S").unwrap().display_name.as_deref(),
            Some("L-glutamate")
        );
        assert_eq!(
            back.find_reaction("J1").unwrap().display_name.as_deref(),
            Some("glutamate uptake")
        );
        assert_eq!(
            back.find_compartment("cell")
                .unwrap()
                .display_name
                .as_deref(),
            Some("astrocyte")
        );
    }

    #[test]
    fn a_display_name_is_written_on_one_line_without_double_quotes() {
        let mut m = ok("A -> B; k*A\nk = 1", &ON);
        m.find_species_mut("A").unwrap().display_name = Some("5\" cap\nmRNA".into());
        let text = export(&m);
        assert!(text.contains("  A is \"5' cap mRNA\";\n"), "{text}");
        assert_eq!(
            ok(&text, &ON)
                .find_species("A")
                .unwrap()
                .display_name
                .as_deref(),
            Some("5' cap mRNA")
        );
    }

    #[test]
    fn a_parameters_name_is_skipped_with_a_warning() {
        let i = import("A -> B; k*A\nk = 1\nk is \"rate constant\"", &ON).unwrap();
        assert_eq!(i.warnings.len(), 1, "{:?}", i.warnings);
        // At the symbol, as for its notes: where `k` is first given.
        assert_eq!(i.warnings[0].line, 2);
    }

    #[test]
    fn const_is_read_through() {
        let m = ok("const k1 = 0.5\nconst species S = 2", &ON);
        assert_eq!(m.find_parameter("k1").unwrap().expression, "0.5");
        assert_eq!(m.find_species("S").unwrap().initial_value, 2.0);
    }

    #[test]
    fn species_are_declared_four_to_a_line() {
        let m = ok(
            "species A, B, $C, D, E, F
const species $K, $L
species G
J1: A + B + C + D + E + F + G + K + L -> ; v",
            &OFF,
        );
        let text = export(&m);
        assert!(
            text.contains(
                "  species A, B, $C, D;
  species E, F;
"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "  const species $K, $L;
  species G;
"
            ),
            "{text}"
        );
        // libAntimony's form, and ours: reading it back gives them all.
        let back = ok(&text, &OFF);
        assert_eq!(back.species().iter().filter(|s| !s.is_null_node).count(), 9);
        assert!(back.find_species("K").unwrap().is_constant);
    }

    #[test]
    fn constant_species_go_through_const() {
        // `const` makes a species constant and boundary, as in libantimony; a
        // plain `$` only boundary.
        let m = ok(
            "const species S = 2
species $A, B
R1: S -> B; k*S
A = 1",
            &ON,
        );
        let s = m.find_species("S").unwrap();
        assert!(s.is_constant && s.is_boundary);
        let a = m.find_species("A").unwrap();
        assert!(a.is_boundary && !a.is_constant);
        assert!(!m.find_species("B").unwrap().is_constant);
        let text = export(&m);
        assert!(
            text.contains(
                "  const species $S;
"
            ),
            "{text}"
        );
        // The others on a line of their own, `const` being said once a line.
        assert!(
            text.contains(
                "  species $A, B;
"
            ),
            "{text}"
        );
        let back = ok(&text, &ON);
        for id in ["S", "A", "B"] {
            let (x, y) = (m.find_species(id).unwrap(), back.find_species(id).unwrap());
            assert_eq!(
                (x.is_constant, x.is_boundary),
                (y.is_constant, y.is_boundary),
                "{id}"
            );
        }
        // A constant species that isn't a boundary one can't be written:
        // `const` would make it one.
        let mut m = back;
        m.find_species_mut("B").unwrap().is_constant = true;
        let text = export(&m);
        assert!(text.contains("  species $A, B;\n"), "{text}");
    }

    #[test]
    fn parameters_keep_the_order_they_were_written() {
        let m = ok("const a, b\nR1: X -> Y; a*b\nb = 1\na = 2", &ON);
        let names: Vec<&str> = m.parameters.iter().map(|p| p.variable.as_str()).collect();
        assert_eq!(names, ["b", "a"]);
    }

    #[test]
    fn a_rate_law_the_diagram_cant_read_points_into_it() {
        // Antimony allows `%`; the diagram's expressions do not.
        let e = import("R1: A -> B; k % 2", &ON).unwrap_err();
        assert_eq!((e[0].line, e[0].column), (1, 15), "{e:?}");
    }

    #[test]
    fn a_model_without_its_end_is_an_error() {
        // The reader before the antimony crate accepted it; libantimony
        // does not.
        let e = import("model m\n  A -> B; k", &ON).unwrap_err();
        assert_eq!(e[0].line, 2, "{e:?}");
    }

    #[test]
    fn a_dollar_marks_a_boundary_species() {
        // `$B = 4` is `const B; B = 4` to libantimony; in a diagram the `$`
        // has always meant a boundary species.
        let m = ok("$B = 4\nk = 1", &ON);
        let b = m.find_species("B").unwrap();
        assert!(b.is_boundary && b.initial_value == 4.0);
        assert!(m.find_parameter("B").is_none());
        assert!(m.find_species("k").is_none(), "k is a parameter");
    }

    #[test]
    fn any_antimony_file_imports() {
        // Events and annotations are skipped, functions expanded; the rest
        // is drawn.
        let m = ok(
            "model *m()\n  function mm(s, v, k)\n    v*s/(k + s)\n  end\n  compartment c = 1;\n  species S in c, P in c;\n  J1: S => P; mm(S, Vm, Km);\n  S = 10; P = 0; Vm = 1; Km = 0.5;\n  E1: at time > 5: S = 10;\n  S is \"substrate\";\nend",
            &ON,
        );
        assert_eq!(ids(&m), ["S", "P"]);
        assert_eq!(m.find_species("S").unwrap().initial_value, 10.0);
        assert_eq!(reaction(&m, "J1").kinetic_law, "Vm*S/(Km + S)");
    }

    #[test]
    fn functions_are_written_into_what_calls_them() {
        let src = "function sq(x)
  x*x
end
function hill(s, k)
  sq(s)/(sq(k) + sq(s))
end
J1: S -> P; E*hill(S, K);
J2: P -> ; k2*P;
T := sq(S) + P;
U := k2 * P;
V = sq(2);
K = 1; k2 = 0.1; E = 1";
        let i = import(src, &ON).unwrap();
        let m = &i.model;
        assert_eq!(reaction(m, "J1").kinetic_law, "E*(S*S/(K*K + S*S))");
        // A law, rule or value that calls nothing is kept as written.
        assert_eq!(reaction(m, "J2").kinetic_law, "k2*P");
        assert_eq!(m.assignment_rules[0].expression, "S*S + P");
        assert_eq!(m.assignment_rules[1].expression, "k2 * P");
        assert_eq!(m.find_parameter("V").unwrap().expression, "2*2");
        assert_eq!(m.find_parameter("K").unwrap().expression, "1");
        let notes: Vec<_> = i
            .warnings
            .iter()
            .map(|d| (d.line, d.message.as_str()))
            .collect();
        assert_eq!(
            notes,
            [(
                1,
                "2 functions were written into the rate laws that use them"
            )]
        );
        // Exported, the model calls no function.
        assert!(!export(m).contains("hill") && !export(m).contains("sq("));

        // The wrong number of arguments is an error where the call is (the
        // antimony crate finds it before the bridge would).
        let e = import(
            "function sq(x)
  x*x
end
J1: S -> P; sq(S, P)",
            &ON,
        )
        .unwrap_err();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].line, 4);
        assert!(e[0].message.contains("\"sq\" takes 1"), "{}", e[0].message);
    }

    #[test]
    fn export_follows_s_11_3() {
        let mut m = ok(
            "model demo\ncompartment cell = 2\nspecies $A in cell = 1.5\nR1: 2 A -> B; k1*A\nR2: B -> ; \nk1 = 0.1\nT := A + B\nend",
            &ON,
        );
        m.add_alias("A", pt(0.0, 0.0));
        let text = export(&m);
        let expected = "\
model demo
  // Compartments
  compartment cell = 2;

  // Species
  species $A in cell, B;

  // Reactions
  R1: 2 $A -> B; k1*A;
  R2: B -> ; v;

  // Parameters
  k1 = 0.1;

  // Initial values
  A = 1.5;
  B = 0;

  // Assignment rules
  T := A + B;
end
";
        assert_eq!(text, expected);
    }

    /// The same biochemistry: species with their values and boundary flags,
    /// reactions with participants and laws, parameters.
    fn biochemistry(m: &Model) -> String {
        let mut out = String::new();
        for s in m
            .species()
            .iter()
            .filter(|s| !s.is_null_node && !s.is_alias())
        {
            out += &format!("{} {} {}|", s.id, s.initial_value, s.is_boundary);
        }
        for r in m.reactions() {
            let side = |l: &[Participant]| {
                l.iter()
                    .filter(|p| !m.find_species(&p.species).unwrap().is_null_node)
                    .map(|p| format!("{}*{}", p.stoichiometry, p.species))
                    .collect::<Vec<_>>()
                    .join("+")
            };
            out += &format!(
                "{}: {} -> {}; {}|",
                r.id,
                side(&r.reactants),
                side(&r.products),
                r.kinetic_law
            );
        }
        let mut params: Vec<_> = m
            .parameters
            .iter()
            .map(|p| format!("{}={}", p.variable, p.expression))
            .collect();
        params.sort();
        out + &params.join(",")
    }

    #[test]
    fn every_reference_file_imports_and_round_trips() {
        // M8 acceptance.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Models");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "ant"))
            .collect();
        files.sort();
        assert_eq!(files.len(), 9);
        for path in files {
            let src = std::fs::read_to_string(&path).unwrap();
            for opts in [ON, OFF] {
                let first = import(&src, &opts)
                    .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()))
                    .model;
                assert!(!first.reactions().is_empty(), "{}", path.display());
                let again = import(&export(&first), &opts)
                    .unwrap_or_else(|e| panic!("{} again: {e:?}", path.display()))
                    .model;
                assert_eq!(
                    biochemistry(&first),
                    biochemistry(&again),
                    "{}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn placement_is_seeded() {
        let a = ok("A -> B", &ON);
        let b = ok("A -> B", &ON);
        assert_eq!(a, b);
        let c = ok("A -> B", &ImportOptions { seed: 2, ..ON });
        assert_ne!(a.species()[0].center, c.species()[0].center);
    }
}
