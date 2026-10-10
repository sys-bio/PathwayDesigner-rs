# sbml-io

Read and write [SBML](https://sbml.org) as plain Rust data. Pure Rust, so it
also works in WebAssembly.

> **Status: design.** This README records the design agreed before
> implementation. Nothing below is implemented yet.

## Scope

- **SBML Level 3**, Versions 1 and 2, all of core:
  - function definitions;
  - unit definitions;
  - compartments, species and parameters;
  - initial assignments;
  - rules: assignment, rate and algebraic;
  - constraints;
  - reactions with species references, modifiers and kinetic laws with local
    parameters;
  - events with trigger, delay, priority and event assignments.
- **SBML Level 2**, Versions 1 to 5, including what only Level 2 has:
  - compartment types and species types;
  - `charge`;
  - stoichiometry given as MathML;
  - a kinetic law's `listOfParameters`;
  - a unit's `offset`.
- **The Layout package** (Level 3 Layout Version 1), read and written in full.
  In Level 2 files, layout lives in an annotation. `sbml-io` reads it from there
  too, into the same `Layout` data.
- **MathML**, the subset SBML allows, as a tree.
- **Everything else is kept.** Notes and annotations, other packages (fbc,
  render, comp, …), and any attribute or element `sbml-io` doesn't understand all
  survive a read and a write unchanged.

Not in scope, at least at first:
- converting between levels and versions;
- checking units;
- the full set of libSBML's consistency rules;
- an infix formula syntax. That belongs to the application. MathML is offered as
  a tree to convert from and to.

## Design

**Not libSBML's object model.** There is no class hierarchy, no getters and
setters, no `ListOf` objects, no plugins and no casting. A document is a tree
of ordinary structs with public fields:
- `Vec` for lists;
- `Option` for attributes that may be absent;
- enums where SBML has alternatives: rules, MathML nodes, curve segments.

Building a model is `model.species.push(Species { … })`; reading one is
`for s in &model.species`.

```rust
pub fn read(xml: &str) -> Result<Read, Vec<Diagnostic>>; // errors carry line and column
pub fn write(doc: &Document) -> String;
pub fn validate(doc: &Document) -> Vec<Issue>;            // ids unique, references resolve

pub struct Read { pub document: Document, pub warnings: Vec<Diagnostic> }

pub struct Document {
    pub level: u32,
    pub version: u32,
    /// Namespace declarations on `<sbml>`, in order. Kept so that preserved
    /// XML from other packages stays valid.
    pub namespaces: Vec<(Option<String>, String)>,
    pub model: Option<Model>,
    pub extra: Extra,
}

pub struct Model {
    pub id: Option<String>, pub name: Option<String>, pub meta: Meta,
    pub function_definitions: Vec<FunctionDefinition>,
    pub unit_definitions: Vec<UnitDefinition>,
    pub compartment_types: Vec<CompartmentType>,   // Level 2 only
    pub species_types: Vec<SpeciesType>,           // Level 2 only
    pub compartments: Vec<Compartment>,
    pub species: Vec<Species>,
    pub parameters: Vec<Parameter>,
    pub initial_assignments: Vec<InitialAssignment>,
    pub rules: Vec<Rule>,
    pub constraints: Vec<Constraint>,
    pub reactions: Vec<Reaction>,
    pub events: Vec<Event>,
    pub layouts: Vec<Layout>,                      // the Layout package
}

/// What any element may carry.
pub struct Meta {
    pub metaid: Option<String>,
    pub sbo_term: Option<String>,
    pub notes: Option<String>,       // raw XML
    pub annotation: Option<String>,  // raw XML
    pub extra: Extra,
}

/// What `sbml-io` doesn't understand on an element, kept for writing back.
pub struct Extra {
    pub attributes: Vec<Attribute>,  // qualified name, namespace, value
    pub elements: Vec<String>,       // raw XML, in document order
}

pub enum Rule {
    Assignment { variable: String, math: Math, meta: Meta },
    Rate { variable: String, math: Math, meta: Meta },
    Algebraic { math: Math, meta: Meta },
}

pub enum Math {
    Number(Number), Ident(String), Symbol(Symbol), Constant(Constant),
    Apply(Op, Vec<Math>), Call(String, Vec<Math>),
    Piecewise { pieces: Vec<(Math, Math)>, otherwise: Option<Box<Math>> },
    Lambda { args: Vec<String>, body: Box<Math> },
    // …
}

pub enum Segment {
    Line { start: Point, end: Point },
    Bezier { start: Point, end: Point, base1: Point, base2: Point },
}
```

**Rules that follow from this design:**
- **Attributes are stored as written.** Absent means `None`, not the default.
  Level 2 has defaults where Level 3 makes attributes required, and keeping what
  was written means a document read and written back doesn't gain attributes it
  didn't have. Applications that need the effective value apply the defaults of
  the document's level.
- **References between elements are id strings.** `validate` checks that they
  resolve.
- **A document is written at its own level and version.** The writer leaves out
  what is `None`, and writes kept XML back where it was found.

## Licence

MIT OR Apache-2.0.
