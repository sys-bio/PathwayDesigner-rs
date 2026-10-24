//! What a diagram section says (docs/diagram-format.md §3–§10), as the parser
//! reads it. Nothing here knows the model: whether `Glc` is a species or a
//! reaction, and whether a clause makes sense for it, is for the reader.

use crate::geometry::Pt;
use crate::model::Rgba;

/// A place in the file, 1-based.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub column: usize,
}

/// A diagram section: `diagram <name> v<version>`, its statements, `end`.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagram {
    /// The model it draws.
    pub name: String,
    pub version: u32,
    pub pos: Pos,
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Statement {
    pub pos: Pos,
    pub kind: StatementKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StatementKind {
    /// `theme escher`
    Theme(String),
    /// `background white`
    Background(Color),
    /// `modifierLines hidden` (`true`) or `shown`.
    ModifierLinesHidden(bool),
    /// `color metab = #FFD54F`
    ColorDef { name: String, value: Color },
    /// `style metabolite [species] { … }`
    Style {
        name: String,
        kind: Option<String>,
        props: Vec<Property>,
    },
    /// `compartment cytosol at … size … [hidden] [style …] [{ … }]`
    Compartment {
        id: String,
        clauses: Vec<Clause>,
        props: Vec<Property>,
    },
    /// `text "…" at … [style …] [{ … }]`
    Text {
        text: String,
        clauses: Vec<Clause>,
        props: Vec<Property>,
    },
    /// `leg A -> B …`, a reactant's line (`A` a drawing, `B` a reaction) or a
    /// product's (`A` a reaction, `B` a drawing); the reader decides which.
    Leg {
        from: Drawing,
        to: Drawing,
        clauses: Vec<Clause>,
        props: Vec<Property>,
    },
    /// `line M to R …`, a modifier's line.
    Line {
        modifier: Drawing,
        reaction: String,
        clauses: Vec<Clause>,
        props: Vec<Property>,
    },
    /// A species drawing or a reaction: `Glc at …`, `J1 at … curved …`.
    Item {
        drawing: Drawing,
        clauses: Vec<Clause>,
        props: Vec<Property>,
    },
}

/// A species' drawing (`ATP`, `ATP#2`) or any other item's id (`copy` 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drawing {
    pub id: String,
    /// 1 for the species' own drawing, 2 for `#2`, …
    pub copy: u32,
}

impl Drawing {
    pub fn plain(id: &str) -> Drawing {
        Drawing {
            id: id.to_owned(),
            copy: 1,
        }
    }
}

/// A color as written; a name is resolved by the reader (a constant, else a
/// basic name).
#[derive(Clone, Debug, PartialEq)]
pub enum Color {
    Value(Rgba),
    None,
    Name(String),
}

/// A width and a height; one number for a circle gives both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub w: f64,
    pub h: f64,
}

/// How a reaction's lines are drawn (format §9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Straight,
    Linear,
    Curved,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clause {
    pub pos: Pos,
    pub kind: ClauseKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClauseKind {
    At(Pt),
    Size(Size),
    Hidden,
    Locked,
    Shape(Shape),
    Smooth,
    /// The reactants' end and the products' end, from the junction.
    Stem(Pt, Pt),
    Free,
    Uses(Vec<Drawing>),
    Source(Pt),
    Sink(Pt),
    Styles(Vec<String>),
    Via(Vec<Pt>),
    Curve(Pt, Pt),
    Kind(String),
    /// A part this version doesn't know (format §13), as written.
    Unknown(String),
}

/// A property in braces or a style: a name and its values (`border #8D6E00
/// 1.5`, `id offset (0, -28)`, `bold`).
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub pos: Pos,
    pub name: String,
    pub values: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Point(Pt),
    /// A word: a color name, `none`, `inside`, `x` in `80 x 36`, …
    Word(String),
    Str(String),
    /// A hex color.
    Hex(Rgba),
}
