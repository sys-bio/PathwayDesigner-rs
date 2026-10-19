//! The native JSON file format (S §11.1, SPEC-Rust §4).
//!
//! The file is a contract shared with the Delphi app, so it is described by
//! its own structs here, separate from the model types. Renaming a model field
//! can never change the file. Reading accepts versions 1–4 (and newer files, as
//! far as possible). Writing produces the lowest version that fits, with the
//! keys in the order the Delphi app writes them: version 3, which the Delphi
//! app reads, or a later one when the model uses what it brought
//! (SPEC-Rust §4.2).

pub mod colour;

use serde::{Deserialize, Deserializer, Serialize};

use crate::geometry::pt;
use crate::model::{
    AssignmentRule, BorderDash, Compartment, DEFAULT_SPECIES_HEIGHT, DEFAULT_SPECIES_WIDTH,
    DEFAULT_TEXT_SIZE, Frame, Look, Model, ModifierKind, NodeShape, Parameter, Participant,
    Reaction, ReactionMode, Rgba, SavedStyle, SpeciesNode, StyleKind, TextAlign, TextFont,
    TextItem, Typeface, VisualStyle,
};

/// The newest version this code understands, and writes when it must: the
/// one that brought modifier kinds (S §5.7.1).
pub const FORMAT_VERSION: u32 = 11;

/// The version that brought stems free to bend while straight (S §5.13).
pub const STEM_STRAIGHT_VERSION: u32 = 10;

/// The version that brought the diagram's background colour (S §8.8).
pub const BACKGROUND_VERSION: u32 = 9;

/// The version that brought reaction stems (S §5.13).
pub const STEM_VERSION: u32 = 8;

/// The version that brought reaction names (S §5.12).
pub const NAME_VERSION: u32 = 7;

/// The version that brought names drawn outside their node (S §5.11) and
/// saved styles (S §8.6).
pub const STYLE_VERSION: u32 = 6;

/// The version that brought species shapes and border styles.
pub const SHAPE_VERSION: u32 = 5;

/// The version that brought free text and drawn compartments.
pub const TEXT_VERSION: u32 = 4;

/// The version the Delphi app reads, and what is written when nothing newer
/// is needed.
pub const DELPHI_VERSION: u32 = 3;

/// The version `model` must be saved as, the oldest that holds it
/// (SPEC-Rust §4.2): 11 if a modifier has a kind or corner points
/// (S §5.7.1), 10 if a
/// straight stem is free to bend (S §5.13), 9 if
/// the diagram has its own background colour, 8 if a
/// reaction has a stem, 7 if a reaction shows its
/// name, 6 if a species has its name outside, a species or
/// compartment has no border, or an item follows a saved style, 5 if one has a
/// shape other than the rounded rectangle or a border style other than
/// automatic, 4 if it has free text or a drawn compartment, 3 otherwise.
pub fn needed_version(model: &Model) -> u32 {
    let no_border = model.species().iter().any(|s| s.style.no_border)
        || model.compartments.iter().any(|c| c.style.no_border);
    let kinds = model.reactions().iter().any(|r| {
        r.modifiers
            .iter()
            .any(|m| m.kind != ModifierKind::Modifier || !m.points.is_empty())
    });
    if kinds {
        FORMAT_VERSION
    } else if model.reactions().iter().any(needs_stem_straight) {
        STEM_STRAIGHT_VERSION
    } else if !model.background.is_default() {
        BACKGROUND_VERSION
    } else if model.reactions().iter().any(|r| r.stem.is_some()) {
        STEM_VERSION
    } else if model.reactions().iter().any(|r| r.label_offset.is_some()) {
        NAME_VERSION
    } else if model.species().iter().any(|s| s.label_offset.is_some())
        || no_border
        || crate::styles::any_links(model)
        || model.theme.is_some()
    {
        STYLE_VERSION
    } else if model
        .species()
        .iter()
        .any(|s| s.shape != NodeShape::default() || s.border_dash != BorderDash::default())
    {
        SHAPE_VERSION
    } else if !model.texts().is_empty() || model.compartments.iter().any(|c| c.frame.is_some()) {
        TEXT_VERSION
    } else {
        DELPHI_VERSION
    }
}

/// Whether a reaction's file entry needs `stemStraight`: only when reading
/// it back would guess wrong, a straight stem free to bend (S §5.13). A
/// stem without the key is straight-kept exactly when it is straight.
fn needs_stem_straight(r: &Reaction) -> bool {
    r.stem
        .is_some_and(|s| crate::model::stem_is_straight(s) && !r.stem_straight)
}

/// The result of reading a file.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub model: Model,
    /// The file's `version` (1 if it had none).
    pub version: u32,
    /// Things that were skipped or repaired, for the user to see. Never fatal.
    pub warnings: Vec<String>,
}

impl Loaded {
    /// True if the file comes from a newer version of the app. Saving it here
    /// would lose whatever this version doesn't understand, so the app makes
    /// Save behave as Save As (SPEC-Rust §4).
    pub fn is_newer_than_supported(&self) -> bool {
        self.version > FORMAT_VERSION
    }
}

/// A file that could not be read at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    /// 1-based line and column, or 0 when the position is unknown.
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(
                f,
                "line {}, column {}: {}",
                self.line, self.column, self.message
            )
        } else {
            f.write_str(&self.message)
        }
    }
}

impl std::error::Error for JsonError {}

impl From<serde_json::Error> for JsonError {
    fn from(e: serde_json::Error) -> Self {
        JsonError {
            line: e.line(),
            column: e.column(),
            message: e.to_string(),
        }
    }
}

/// Reads a native JSON document (S §11.1, load procedure). A byte-order mark
/// at the start is ignored.
pub fn from_json(text: &str) -> Result<Loaded, JsonError> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let file: FileModel = serde_json::from_str(text)?;
    Ok(file.into_model())
}

/// Writes `model` as a version 3 document, or 4 if it has free text: UTF-8,
/// 2-space indentation, keys in the Delphi app's order, colours as upper-case
/// `#RRGGBBAA`. Selection is not saved.
pub fn to_json(model: &Model) -> String {
    let file = FileModel::from_model(model);
    let mut text = serde_json::to_string_pretty(&file).expect("the file structs always serialise");
    text.push('\n');
    text
}

/// A list of saved styles on its own (My Styles, S §8.6): `{"version": 6,
/// "styles": [...]}`, each style as in a model file (SPEC-Rust §4.2).
pub fn styles_to_json(styles: &[SavedStyle]) -> String {
    let file = FileStyleList {
        version: Some(STYLE_VERSION),
        styles: styles.iter().map(saved_style_to_file).collect(),
    };
    let mut text = serde_json::to_string_pretty(&file).expect("the file structs always serialise");
    text.push('\n');
    text
}

/// Reads a list of saved styles, with warnings for those skipped.
pub fn styles_from_json(text: &str) -> Result<(Vec<SavedStyle>, Vec<String>), JsonError> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let file: FileStyleList = serde_json::from_str(text)?;
    let mut warnings = Vec::new();
    let styles = styles_from_file(file.styles, &mut warnings);
    Ok((styles, warnings))
}

// ----- File structs ----------------------------------------------------------
//
// Field order is key order in the written file. `default`s say what a missing
// key means; `skip_serializing_if` leaves keys out where the Delphi app does.

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileModel {
    #[serde(default, deserialize_with = "lenient_u32_opt")]
    version: Option<u32>,
    #[serde(default)]
    model_name: String,
    #[serde(default)]
    compartments: Vec<FileCompartment>,
    #[serde(default)]
    parameters: Vec<FileExpression>,
    #[serde(default)]
    assignment_rules: Vec<FileExpression>,
    #[serde(default)]
    species: Vec<FileSpecies>,
    #[serde(default)]
    reactions: Vec<FileReaction>,
    /// Version 4: free text. Left out when there is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    texts: Vec<FileText>,
    /// Version 6: the saved styles some item follows (S §8.6). Left out when
    /// there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    styles: Vec<FileSavedStyle>,
    /// Version 6: the diagram's theme (S §8.7), `"escher"`. Left out for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    theme: Option<String>,
    /// Version 9: the diagram's background colour (S §8.8). Left out for the
    /// default.
    #[serde(default, with = "colour", skip_serializing_if = "is_default_colour")]
    background: Rgba,
}

/// A list of saved styles on its own.
#[derive(Serialize, Deserialize)]
struct FileStyleList {
    #[serde(default, deserialize_with = "lenient_u32_opt")]
    version: Option<u32>,
    #[serde(default)]
    styles: Vec<FileSavedStyle>,
}

/// A saved style (version 6, S §8.6): a name, a kind, and the look, in the
/// keys an item of that kind uses. Keys that don't belong to the kind are
/// ignored.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileSavedStyle {
    name: String,
    /// `"species"`, `"reaction"`, `"compartment"` or `"text"`.
    kind: String,
    /// Species, reactions and compartments: their style object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style: Option<FileStyle>,
    /// Species and compartments: no border, as on the item.
    #[serde(default, skip_serializing_if = "is_false")]
    no_border: bool,
    /// Species: as on a species.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    border_dash: Option<String>,
    /// Species: the name drawn outside the node, and then the node's size.
    #[serde(default, skip_serializing_if = "is_false")]
    label_outside: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    h: Option<f64>,
    /// Texts: as on a text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    typeface: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    italic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    align: Option<String>,
    #[serde(default, with = "colour", skip_serializing_if = "is_default_colour")]
    color: Rgba,
}

/// Free text (version 4, S §7.8). Every key but `id` is optional; left out
/// at its default.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileText {
    id: String,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    text: String,
    #[serde(default = "default_text_size")]
    size: f64,
    /// `"sans"` or `"serif"`; anything else reads as sans.
    #[serde(default, skip_serializing_if = "is_sans")]
    typeface: String,
    #[serde(default, skip_serializing_if = "is_false")]
    bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    italic: bool,
    /// `"left"`, `"center"` or `"right"`; anything else reads as left.
    #[serde(default, skip_serializing_if = "is_left")]
    align: String,
    #[serde(default, with = "colour", skip_serializing_if = "is_default_colour")]
    color: Rgba,
    #[serde(default, skip_serializing_if = "is_zero")]
    wrap_width: f64,
    /// Version 6: the saved style it follows (S §8.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileCompartment {
    id: String,
    #[serde(default = "one")]
    size: f64,
    #[serde(default = "three", deserialize_with = "lenient_u32")]
    dimensions: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style: Option<FileStyle>,
    /// Version 4: the box it is drawn as, by its centre and size, as for a
    /// species. Left out until it is drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    h: Option<f64>,
    /// Version 4: its box is not shown (S §7.9). Left out when it is.
    #[serde(default, skip_serializing_if = "is_false")]
    hidden: bool,
    /// Version 6: the saved style it follows (S §8.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style_name: Option<String>,
    /// Version 6: no border (S §7.9). Left out when it has one.
    #[serde(default, skip_serializing_if = "is_false")]
    no_border: bool,
}

/// A parameter (`=`) or assignment rule (`:=`): both are a variable and an expression.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileExpression {
    variable: String,
    #[serde(default)]
    expression: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileSpecies {
    id: String,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default = "default_width")]
    w: f64,
    #[serde(default = "default_height")]
    h: f64,
    #[serde(default)]
    initial_value: f64,
    #[serde(default)]
    is_boundary: bool,
    #[serde(default)]
    is_constant: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    compartment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alias_of: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    is_null_node: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style: Option<FileStyle>,
    /// Version 5: what it is drawn as (S §5.10). Left out for the rounded
    /// rectangle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shape: Option<String>,
    /// Version 5: `"solid"` or `"dashed"` (S §5.3). Left out for automatic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    border_dash: Option<String>,
    /// Version 6: where its name is drawn outside the node, from the node's
    /// centre (S §5.11). Both or neither; left out for a name inside.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label_dx: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label_dy: Option<f64>,
    /// Version 6: the saved style it follows (S §8.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style_name: Option<String>,
    /// Version 6: no border (S §5.3). Left out when it has one.
    #[serde(default, skip_serializing_if = "is_false")]
    no_border: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileReaction {
    id: String,
    #[serde(default)]
    jx: f64,
    #[serde(default)]
    jy: f64,
    #[serde(default)]
    kinetic_law: String,
    #[serde(default)]
    is_reversible: bool,
    #[serde(default)]
    is_linear: bool,
    #[serde(default)]
    is_bezier: bool,
    #[serde(default)]
    is_junction_smooth: bool,
    #[serde(default)]
    reactants: Vec<FileParticipant>,
    #[serde(default)]
    products: Vec<FileParticipant>,
    /// Always written, even when empty, as the current Delphi writer does.
    #[serde(default)]
    modifiers: Vec<FileParticipant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style: Option<FileStyle>,
    /// Version 6: the saved style it follows (S §8.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style_name: Option<String>,
    /// Version 7: where its name is drawn, from its junction (S §5.12).
    /// Both or neither; left out when the name isn't shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label_dx: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label_dy: Option<f64>,
    /// Version 8: its stem (S §5.13), `[rdx, rdy, pdx, pdy]`: where the
    /// reactants' lines end and the products' lines start, from the junction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stem: Option<Vec<f64>>,
    /// Version 10: `false` for a straight stem whose ends may move freely
    /// (S §5.13). Left out otherwise: a stem is kept straight exactly when
    /// it is straight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stem_straight: Option<bool>,
}

/// Version 1 lists participants as bare species ids; later versions use objects.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum FileParticipant {
    Id(String),
    Full(FileParticipantObject),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileParticipantObject {
    id: String,
    /// Left out for modifiers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stoichiometry: Option<f64>,
    /// The handles, written only when `ctrlSet` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ctrl1x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ctrl1y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ctrl2x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ctrl2y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ctrl_set: Option<bool>,
    /// Version 11: a modifier's kind (S §5.7.1): `"activator"`,
    /// `"inhibitor"` or `"catalyst"`. Left out for a plain modifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    /// Version 11: a modifier's corner points (S §5.7.1), as
    /// `[x1, y1, x2, y2, …]`. Left out when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    points: Option<Vec<f64>>,
}

/// A style object. Missing keys mean 0 (use the default), or true for
/// `junctionVisible` (S §11.1). Written with all nine keys, as the Delphi app
/// does.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileStyle {
    #[serde(default, with = "colour")]
    fill_color: Rgba,
    #[serde(default, with = "colour")]
    border_color: Rgba,
    #[serde(default, with = "colour")]
    label_color: Rgba,
    #[serde(default)]
    border_width: f64,
    #[serde(default, with = "colour")]
    line_color: Rgba,
    #[serde(default)]
    line_width: f64,
    #[serde(default)]
    font_size: f64,
    #[serde(default, with = "colour")]
    junction_color: Rgba,
    #[serde(default = "yes")]
    junction_visible: bool,
}

fn one() -> f64 {
    1.0
}
fn three() -> u32 {
    3
}
fn yes() -> bool {
    true
}
fn default_width() -> f64 {
    DEFAULT_SPECIES_WIDTH
}
fn default_height() -> f64 {
    DEFAULT_SPECIES_HEIGHT
}
fn is_false(b: &bool) -> bool {
    !*b
}
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}
fn is_sans(s: &str) -> bool {
    s.is_empty() || s == "sans"
}
fn is_left(s: &str) -> bool {
    s.is_empty() || s == "left"
}
fn is_default_colour(c: &Rgba) -> bool {
    c.is_default()
}
fn default_text_size() -> f64 {
    DEFAULT_TEXT_SIZE
}

/// A whole number written either as `3` or `3.0`.
fn lenient_u32<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let v = f64::deserialize(d)?;
    if v.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&v) {
        Ok(v as u32)
    } else {
        Err(serde::de::Error::custom(format!(
            "expected a whole number, found {v}"
        )))
    }
}

fn lenient_u32_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    lenient_u32(d).map(Some)
}

// ----- File → model ----------------------------------------------------------

impl FileModel {
    fn into_model(self) -> Loaded {
        let mut warnings = Vec::new();
        let version = self.version.unwrap_or(1);
        if version > FORMAT_VERSION {
            warnings.push(format!(
                "This file was made by a newer version of PathwayDesigner (format {version}). \
                 Saving it here will lose features this version doesn't support."
            ));
        }

        let mut model = Model::new();
        model.model_name = self.model_name;
        model.compartments = self
            .compartments
            .into_iter()
            .map(|c| Compartment {
                id: c.id,
                size: c.size,
                dimensions: c.dimensions,
                style: VisualStyle {
                    no_border: c.no_border,
                    ..style_from_file(c.style)
                },
                // All four, and a size of at least 1, or not drawn yet.
                frame: match (c.x, c.y, c.w, c.h) {
                    (Some(x), Some(y), Some(w), Some(h)) if w >= 1.0 && h >= 1.0 => Some(Frame {
                        center: pt(x, y),
                        width: w,
                        height: h,
                    }),
                    _ => None,
                },
                hidden: c.hidden,
                selected: false,
                style_name: c.style_name,
            })
            .collect();
        model.parameters = self
            .parameters
            .into_iter()
            .map(|p| Parameter {
                variable: p.variable,
                expression: p.expression,
            })
            .collect();
        model.assignment_rules = self
            .assignment_rules
            .into_iter()
            .map(|r| AssignmentRule {
                variable: r.variable,
                expression: r.expression,
            })
            .collect();

        // All species first; aliases are resolved in a second pass, so an
        // alias may come before its primary in the file.
        for s in self.species {
            let shape = match s.shape.as_deref() {
                None => NodeShape::default(),
                Some(key) => NodeShape::from_key(key).unwrap_or_else(|| {
                    warnings.push(format!(
                        "Species `{}` has the unknown shape `{key}`; it is drawn as a rounded rectangle.",
                        s.id
                    ));
                    NodeShape::default()
                }),
            };
            let border_dash = match s.border_dash.as_deref() {
                None => BorderDash::default(),
                Some(key) => BorderDash::from_key(key).unwrap_or_else(|| {
                    warnings.push(format!(
                        "Species `{}` has the unknown border style `{key}`; it is drawn as usual.",
                        s.id
                    ));
                    BorderDash::default()
                }),
            };
            let label_offset = match (s.label_dx, s.label_dy) {
                (None, None) => None,
                (Some(dx), Some(dy)) if dx.is_finite() && dy.is_finite() => Some(pt(dx, dy)),
                _ => {
                    warnings.push(format!(
                        "Species `{}` has an incomplete label position; its name is drawn inside.",
                        s.id
                    ));
                    None
                }
            };
            let node = SpeciesNode {
                id: s.id,
                center: pt(s.x, s.y),
                width: s.w,
                height: s.h,
                shape,
                border_dash,
                label_offset,
                initial_value: s.initial_value,
                is_boundary: s.is_boundary,
                is_constant: s.is_constant,
                compartment: s.compartment,
                alias_of: s.alias_of,
                is_null_node: s.is_null_node,
                locked: s.locked,
                selected: false,
                style: VisualStyle {
                    no_border: s.no_border,
                    ..style_from_file(s.style)
                },
                style_name: s.style_name,
            };
            if let Err(dup) = model.push_species(node) {
                warnings.push(format!("A second species with id `{}` was skipped.", dup.0));
            }
        }
        resolve_aliases(&mut model, &mut warnings);

        for r in self.reactions {
            let mode = if r.is_bezier {
                ReactionMode::Bezier
            } else if r.is_linear {
                ReactionMode::Linear
            } else {
                ReactionMode::Straight
            };
            let mut reaction = Reaction::new(r.id, pt(r.jx, r.jy));
            reaction.kinetic_law = r.kinetic_law;
            reaction.is_reversible = r.is_reversible;
            reaction.mode = mode;
            reaction.junction_smooth = r.is_junction_smooth;
            reaction.style = style_from_file(r.style);
            reaction.style_name = r.style_name;
            reaction.stem = match r.stem.as_deref() {
                None => None,
                Some(&[a, b, c, d]) if [a, b, c, d].iter().all(|v| v.is_finite()) => {
                    Some((pt(a, b), pt(c, d)))
                }
                Some(_) => {
                    warnings.push(format!(
                        "Reaction `{}` has a stem that isn't four numbers; it was left out.",
                        reaction.id
                    ));
                    None
                }
            };
            reaction.stem_straight = reaction
                .stem
                .is_some_and(|s| r.stem_straight.unwrap_or(crate::model::stem_is_straight(s)));
            reaction.label_offset = match (r.label_dx, r.label_dy) {
                (None, None) => None,
                (Some(dx), Some(dy)) if dx.is_finite() && dy.is_finite() => Some(pt(dx, dy)),
                _ => {
                    warnings.push(format!(
                        "Reaction `{}` has an incomplete name position; its name isn't shown.",
                        reaction.id
                    ));
                    None
                }
            };
            for (list, out) in [
                (r.reactants, &mut reaction.reactants),
                (r.products, &mut reaction.products),
                (r.modifiers, &mut reaction.modifiers),
            ] {
                for p in list {
                    let p = participant_from_file(p);
                    if model.find_species(&p.species).is_some() {
                        out.push(p);
                    } else {
                        warnings.push(format!(
                            "Reaction `{}` refers to a species `{}` that isn't in the file; that \
                             entry was skipped.",
                            reaction.id, p.species
                        ));
                    }
                }
            }
            if let Err(dup) = model.push_reaction(reaction) {
                warnings.push(format!(
                    "A second reaction with id `{}` was skipped.",
                    dup.0
                ));
            }
        }

        for t in self.texts {
            let size = if t.size.is_finite() && t.size >= 1.0 {
                t.size
            } else {
                DEFAULT_TEXT_SIZE
            };
            let item = TextItem {
                id: t.id,
                position: pt(t.x, t.y),
                wrap_width: t.wrap_width.max(0.0),
                text: t.text,
                font: TextFont {
                    typeface: if t.typeface == "serif" {
                        Typeface::Serif
                    } else {
                        Typeface::Sans
                    },
                    bold: t.bold,
                    italic: t.italic,
                },
                align: match t.align.as_str() {
                    "center" => TextAlign::Center,
                    "right" => TextAlign::Right,
                    _ => TextAlign::Left,
                },
                size,
                color: t.color,
                selected: false,
                style_name: t.style_name,
            };
            if let Err(dup) = model.push_text(item) {
                warnings.push(format!(
                    "A text with id `{}`, already used, was skipped.",
                    dup.0
                ));
            }
        }

        model.styles = styles_from_file(self.styles, &mut warnings);
        model.theme = match self.theme.as_deref() {
            None => None,
            Some(key) => crate::themes::Theme::from_key(key).or_else(|| {
                warnings.push(format!(
                    "The file's theme `{key}` isn't one this version knows; new items get the \
                     default look."
                ));
                None
            }),
        };
        link_styles(&mut model, &mut warnings);
        model.background = self.background;

        model.recompute_id_counter();
        Loaded {
            model,
            version,
            warnings,
        }
    }
}

/// The saved styles (S §8.6). One with no name, a kind this version doesn't
/// know, or a name its kind already has, is skipped with a warning.
fn styles_from_file(list: Vec<FileSavedStyle>, warnings: &mut Vec<String>) -> Vec<SavedStyle> {
    let mut out: Vec<SavedStyle> = Vec::new();
    for f in list {
        let name = f.name.trim().to_owned();
        let Some(kind) = StyleKind::from_key(&f.kind) else {
            warnings.push(format!(
                "The style `{name}` is of a kind this version doesn't know (`{}`); it was skipped.",
                f.kind
            ));
            continue;
        };
        if name.is_empty() {
            warnings.push("A style with no name was skipped.".to_owned());
            continue;
        }
        if out.iter().any(|s| s.kind() == kind && s.name == name) {
            warnings.push(format!(
                "A second {} style named `{name}` was skipped.",
                kind.key()
            ));
            continue;
        }
        let look = match kind {
            StyleKind::Species => {
                let size = match (f.label_outside, f.w, f.h) {
                    (false, _, _) => None,
                    (true, Some(w), Some(h))
                        if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 =>
                    {
                        Some((w, h))
                    }
                    _ => {
                        warnings.push(format!(
                            "The species style `{name}` puts the name outside but has no size; \
                             it was skipped."
                        ));
                        continue;
                    }
                };
                Look::Species {
                    // As a species keeps it (S §8.5): the junction is a reaction's.
                    style: VisualStyle {
                        junction_visible: true,
                        no_border: f.no_border,
                        ..style_from_file(f.style)
                    },
                    shape: f
                        .shape
                        .as_deref()
                        .and_then(NodeShape::from_key)
                        .unwrap_or_default(),
                    border_dash: f
                        .border_dash
                        .as_deref()
                        .and_then(BorderDash::from_key)
                        .unwrap_or_default(),
                    label_outside: f.label_outside,
                    size,
                    // Saved styles are kept as drawn on a primary.
                    alias: false,
                }
            }
            StyleKind::Reaction => Look::Reaction {
                style: style_from_file(f.style),
            },
            StyleKind::Compartment => Look::Compartment {
                style: VisualStyle {
                    no_border: f.no_border,
                    ..style_from_file(f.style)
                },
            },
            StyleKind::Text => Look::Text {
                font: TextFont {
                    typeface: if f.typeface.as_deref() == Some("serif") {
                        Typeface::Serif
                    } else {
                        Typeface::Sans
                    },
                    bold: f.bold,
                    italic: f.italic,
                },
                align: match f.align.as_deref() {
                    Some("center") => TextAlign::Center,
                    Some("right") => TextAlign::Right,
                    _ => TextAlign::Left,
                },
                size: f
                    .size
                    .filter(|s| s.is_finite() && *s >= 1.0)
                    .unwrap_or(DEFAULT_TEXT_SIZE),
                color: f.color,
            },
        };
        // As drawn, as saved styles are kept (S §8.6).
        out.push(SavedStyle {
            name,
            look: crate::styles::resolved(look),
        });
    }
    out
}

/// An item following a style the file doesn't have is unlinked, with a
/// warning; one that no longer looks like its style (the file was edited by
/// hand) is unlinked quietly, keeping its own look.
fn link_styles(model: &mut Model, warnings: &mut Vec<String>) {
    for kind in StyleKind::ALL {
        for id in crate::styles::item_ids(model, kind) {
            let Some(name) = crate::styles::link(model, kind, &id) else {
                continue;
            };
            if crate::styles::find(model, kind, &name).is_none() {
                warnings.push(format!(
                    "`{id}` follows a style `{name}` that isn't in the file; it keeps its look."
                ));
                crate::styles::set_link(model, kind, &id, None);
            }
        }
    }
    crate::styles::drop_stale_links(model);
}

/// Makes every `alias_of` point at an existing primary (S §3.2). An alias of
/// an alias is pointed at the primary; an alias of a missing species, or of
/// itself, becomes a primary.
fn resolve_aliases(model: &mut Model, warnings: &mut Vec<String>) {
    let fixes: Vec<(String, Option<String>)> = model
        .species()
        .iter()
        .filter_map(|s| {
            let target = s.alias_of.as_deref()?;
            let resolved = follow_alias(model, &s.id, target);
            if resolved.as_deref() == Some(target) {
                return None;
            }
            match &resolved {
                Some(p) => warnings.push(format!(
                    "Alias `{}` pointed at another alias; it now points at the primary `{p}`.",
                    s.id
                )),
                None => warnings.push(format!(
                    "Alias `{}` pointed at `{target}`, which isn't a species in the file; it is \
                     now a species of its own.",
                    s.id
                )),
            }
            Some((s.id.clone(), resolved))
        })
        .collect();
    for (id, alias_of) in fixes {
        if let Some(node) = model.find_species_mut(&id) {
            node.alias_of = alias_of;
        }
    }
}

/// Follows a chain of aliases from `target` to a primary. `None` if the chain
/// ends at a missing species or loops back on itself.
fn follow_alias<'a>(model: &'a Model, start: &'a str, mut target: &'a str) -> Option<String> {
    let mut seen = vec![start];
    loop {
        if seen.contains(&target) {
            return None;
        }
        let node = model.find_species(target)?;
        match node.alias_of.as_deref() {
            None => return Some(node.id.clone()),
            Some(next) => {
                seen.push(target);
                target = next;
            }
        }
    }
}

fn participant_from_file(p: FileParticipant) -> Participant {
    match p {
        FileParticipant::Id(id) => Participant::new(id, 1.0),
        FileParticipant::Full(o) => {
            let ctrl_pts_set = o.ctrl_set.unwrap_or(false);
            Participant {
                species: o.id,
                stoichiometry: o.stoichiometry.unwrap_or(1.0),
                ctrl1: if ctrl_pts_set {
                    pt(o.ctrl1x.unwrap_or(0.0), o.ctrl1y.unwrap_or(0.0))
                } else {
                    pt(0.0, 0.0)
                },
                ctrl2: if ctrl_pts_set {
                    pt(o.ctrl2x.unwrap_or(0.0), o.ctrl2y.unwrap_or(0.0))
                } else {
                    pt(0.0, 0.0)
                },
                ctrl_pts_set,
                kind: o
                    .kind
                    .as_deref()
                    .map_or(ModifierKind::Modifier, ModifierKind::from_key),
                // An odd number leaves the last one out.
                points: o
                    .points
                    .unwrap_or_default()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&[x, y]| pt(x, y))
                    .collect(),
            }
        }
    }
}

/// Any style object makes the style custom, as in the Delphi app; the
/// per-field sentinels still apply (S §3.6).
fn style_from_file(s: Option<FileStyle>) -> VisualStyle {
    match s {
        None => VisualStyle::default(),
        Some(s) => VisualStyle {
            has_custom_style: true,
            fill_color: s.fill_color,
            border_color: s.border_color,
            label_color: s.label_color,
            border_width: s.border_width,
            line_color: s.line_color,
            line_width: s.line_width,
            font_size: s.font_size,
            junction_color: s.junction_color,
            junction_visible: s.junction_visible,
            no_border: false,
        },
    }
}

// ----- Model → file ----------------------------------------------------------

impl FileModel {
    fn from_model(m: &Model) -> Self {
        FileModel {
            version: Some(needed_version(m)),
            model_name: m.model_name.clone(),
            compartments: m
                .compartments
                .iter()
                .map(|c| FileCompartment {
                    id: c.id.clone(),
                    size: c.size,
                    dimensions: c.dimensions,
                    style: custom_style_to_file(&c.style),
                    x: c.frame.map(|f| f.center.x),
                    y: c.frame.map(|f| f.center.y),
                    w: c.frame.map(|f| f.width),
                    h: c.frame.map(|f| f.height),
                    hidden: c.hidden,
                    style_name: c.style_name.clone(),
                    no_border: c.style.no_border,
                })
                .collect(),
            parameters: m
                .parameters
                .iter()
                .map(|p| FileExpression {
                    variable: p.variable.clone(),
                    expression: p.expression.clone(),
                })
                .collect(),
            assignment_rules: m
                .assignment_rules
                .iter()
                .map(|r| FileExpression {
                    variable: r.variable.clone(),
                    expression: r.expression.clone(),
                })
                .collect(),
            species: m
                .species()
                .iter()
                .map(|s| FileSpecies {
                    id: s.id.clone(),
                    x: s.center.x,
                    y: s.center.y,
                    w: s.width,
                    h: s.height,
                    initial_value: s.initial_value,
                    is_boundary: s.is_boundary,
                    is_constant: s.is_constant,
                    compartment: s.compartment.clone(),
                    alias_of: s.alias_of.clone(),
                    is_null_node: s.is_null_node,
                    locked: s.locked,
                    style: custom_style_to_file(&s.style),
                    shape: (s.shape != NodeShape::default()).then(|| s.shape.key().to_owned()),
                    border_dash: (s.border_dash != BorderDash::default())
                        .then(|| s.border_dash.key().to_owned()),
                    label_dx: s.label_offset.map(|o| o.x),
                    label_dy: s.label_offset.map(|o| o.y),
                    style_name: s.style_name.clone(),
                    no_border: s.style.no_border,
                })
                .collect(),
            reactions: m.reactions().iter().map(reaction_to_file).collect(),
            texts: m
                .texts()
                .iter()
                .map(|t| FileText {
                    id: t.id.clone(),
                    x: t.position.x,
                    y: t.position.y,
                    text: t.text.clone(),
                    size: t.size,
                    typeface: match t.font.typeface {
                        Typeface::Sans => "sans",
                        Typeface::Serif => "serif",
                    }
                    .to_owned(),
                    bold: t.font.bold,
                    italic: t.font.italic,
                    align: match t.align {
                        TextAlign::Left => "left",
                        TextAlign::Center => "center",
                        TextAlign::Right => "right",
                    }
                    .to_owned(),
                    color: t.color,
                    wrap_width: t.wrap_width,
                    style_name: t.style_name.clone(),
                })
                .collect(),
            styles: m
                .styles
                .iter()
                .filter(|st| !crate::styles::users(m, st.kind(), &st.name).is_empty())
                .map(saved_style_to_file)
                .collect(),
            theme: m.theme.map(|t| t.key().to_owned()),
            background: m.background,
        }
    }
}

/// A saved style, in the keys an item of its kind uses.
fn saved_style_to_file(st: &SavedStyle) -> FileSavedStyle {
    let mut f = FileSavedStyle {
        name: st.name.clone(),
        kind: st.kind().key().to_owned(),
        style: None,
        no_border: false,
        shape: None,
        border_dash: None,
        label_outside: false,
        w: None,
        h: None,
        size: None,
        typeface: None,
        bold: false,
        italic: false,
        align: None,
        color: Rgba::DEFAULT,
    };
    match st.look {
        Look::Species {
            style,
            shape,
            border_dash,
            label_outside,
            size,
            alias: _,
        } => {
            f.style = custom_style_to_file(&style);
            f.no_border = style.no_border;
            f.shape = (shape != NodeShape::default()).then(|| shape.key().to_owned());
            f.border_dash =
                (border_dash != BorderDash::default()).then(|| border_dash.key().to_owned());
            f.label_outside = label_outside;
            (f.w, f.h) = (size.map(|s| s.0), size.map(|s| s.1));
        }
        // As a reaction's own (fix D-8): a hidden junction is kept.
        Look::Reaction { style } => f.style = reaction_style_to_file(&style),
        Look::Compartment { style } => {
            f.style = custom_style_to_file(&style);
            f.no_border = style.no_border;
        }
        Look::Text {
            font,
            align,
            size,
            color,
        } => {
            f.size = Some(size);
            f.typeface = (font.typeface == Typeface::Serif).then(|| "serif".to_owned());
            (f.bold, f.italic) = (font.bold, font.italic);
            f.align = match align {
                TextAlign::Left => None,
                TextAlign::Center => Some("center".to_owned()),
                TextAlign::Right => Some("right".to_owned()),
            };
            f.color = color;
        }
    }
    f
}

fn reaction_to_file(r: &Reaction) -> FileReaction {
    let participants = |list: &[Participant]| {
        list.iter()
            .map(|p| {
                let (c1, c2) = (p.ctrl1, p.ctrl2);
                let set = p.ctrl_pts_set;
                FileParticipant::Full(FileParticipantObject {
                    id: p.species.clone(),
                    stoichiometry: Some(p.stoichiometry),
                    ctrl1x: set.then_some(c1.x),
                    ctrl1y: set.then_some(c1.y),
                    ctrl2x: set.then_some(c2.x),
                    ctrl2y: set.then_some(c2.y),
                    ctrl_set: set.then_some(true),
                    kind: None,
                    points: None,
                })
            })
            .collect()
    };
    let style = reaction_style_to_file(&r.style);
    FileReaction {
        id: r.id.clone(),
        jx: r.junction.x,
        jy: r.junction.y,
        kinetic_law: r.kinetic_law.clone(),
        is_reversible: r.is_reversible,
        is_linear: r.mode == ReactionMode::Linear,
        is_bezier: r.mode == ReactionMode::Bezier,
        is_junction_smooth: r.junction_smooth,
        reactants: participants(&r.reactants),
        products: participants(&r.products),
        // Modifiers are written with their id, and their kind (S §5.7.1).
        modifiers: r
            .modifiers
            .iter()
            .map(|p| {
                FileParticipant::Full(FileParticipantObject {
                    id: p.species.clone(),
                    stoichiometry: None,
                    ctrl1x: None,
                    ctrl1y: None,
                    ctrl2x: None,
                    ctrl2y: None,
                    ctrl_set: None,
                    kind: p.kind.key().map(str::to_owned),
                    points: (!p.points.is_empty())
                        .then(|| p.points.iter().flat_map(|q| [q.x, q.y]).collect()),
                })
            })
            .collect(),
        style,
        style_name: r.style_name.clone(),
        label_dx: r.label_offset.map(|o| o.x),
        label_dy: r.label_offset.map(|o| o.y),
        stem: r.stem.map(|(a, b)| vec![a.x, a.y, b.x, b.y]),
        stem_straight: needs_stem_straight(r).then_some(false),
    }
}

/// A reaction's style object. Fix D-8: a hidden junction is saved even
/// without a custom style, as a style object whose other fields are all "use
/// the default".
fn reaction_style_to_file(s: &VisualStyle) -> Option<FileStyle> {
    if s.has_custom_style {
        Some(style_to_file(s))
    } else if !s.junction_visible {
        Some(style_to_file(&VisualStyle {
            junction_visible: false,
            ..VisualStyle::default()
        }))
    } else {
        None
    }
}

fn custom_style_to_file(s: &VisualStyle) -> Option<FileStyle> {
    s.has_custom_style.then(|| style_to_file(s))
}

fn style_to_file(s: &VisualStyle) -> FileStyle {
    FileStyle {
        fill_color: s.fill_color,
        border_color: s.border_color,
        label_color: s.label_color,
        border_width: s.border_width,
        line_color: s.line_color,
        line_width: s.line_width,
        font_size: s.font_size,
        junction_color: s.junction_color,
        junction_visible: s.junction_visible,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Pt;

    #[test]
    fn version_1_participants_are_bare_ids() {
        let text = r#"{
            "modelName": "v1",
            "species": [
                { "id": "A", "x": 10, "y": 20, "w": 80, "h": 36 },
                { "id": "B", "x": 200, "y": 20, "w": 80, "h": 36 }
            ],
            "reactions": [
                { "id": "R1", "jx": 100, "jy": 20, "kineticLaw": "k*A",
                  "reactants": ["A"], "products": ["B"] }
            ]
        }"#;
        let loaded = from_json(text).unwrap();
        assert_eq!(loaded.version, 1, "no version key means version 1");
        assert!(loaded.warnings.is_empty());
        let m = &loaded.model;
        let r = m.find_reaction("R1").unwrap();
        assert_eq!(r.reactants, vec![Participant::new("A", 1.0)]);
        assert_eq!(r.products, vec![Participant::new("B", 1.0)]);
        assert!(r.modifiers.is_empty());
        assert_eq!(m.find_species("A").unwrap().center, pt(10.0, 20.0));
    }

    #[test]
    fn version_2_participants_have_stoichiometry() {
        let text = r#"{
            "version": 2,
            "modelName": "v2",
            "compartments": [ { "id": "cell", "size": 2.5, "dimensions": 3 } ],
            "parameters": [ { "variable": "k1", "expression": "0.1" } ],
            "assignmentRules": [ { "variable": "T", "expression": "A + B" } ],
            "species": [
                { "id": "A", "x": 0, "y": 0, "w": 80, "h": 36, "initialValue": 5,
                  "isBoundary": true, "isConstant": false, "compartment": "cell" },
                { "id": "B", "x": 1, "y": 1, "w": 80, "h": 36 }
            ],
            "reactions": [
                { "id": "R1", "jx": 0, "jy": 0, "kineticLaw": "k1*A", "isReversible": true,
                  "isLinear": true, "isBezier": false, "isJunctionSmooth": false,
                  "reactants": [ { "id": "A", "stoichiometry": 2 } ],
                  "products": [ { "id": "B", "stoichiometry": 1 } ] }
            ]
        }"#;
        let m = from_json(text).unwrap().model;
        assert_eq!(m.compartments[0].size, 2.5);
        assert_eq!(m.parameters[0].expression, "0.1");
        assert_eq!(m.assignment_rules[0].variable, "T");
        let a = m.find_species("A").unwrap();
        assert_eq!(a.initial_value, 5.0);
        assert!(a.is_boundary);
        assert_eq!(a.compartment, "cell");
        let r = m.find_reaction("R1").unwrap();
        assert_eq!(r.reactants[0].stoichiometry, 2.0);
        assert_eq!(r.mode, ReactionMode::Linear);
        assert!(r.is_reversible);
    }

    #[test]
    fn styles_with_legacy_integer_and_short_hex_colours() {
        let text = r##"{
            "version": 3,
            "species": [
                { "id": "A", "x": 0, "y": 0, "w": 80, "h": 36,
                  "style": { "fillColor": -2572328, "borderColor": "4A7FCB", "borderWidth": 2 } }
            ]
        }"##;
        let style = from_json(text).unwrap().model.species()[0].style;
        assert!(style.has_custom_style);
        assert_eq!(style.fill_color, Rgba([0xD8, 0xBF, 0xD8, 0xFF]));
        assert_eq!(style.border_color, Rgba([0x4A, 0x7F, 0xCB, 0xFF]));
        assert_eq!(style.border_width, 2.0);
        // Missing keys mean "use the default".
        assert_eq!(style.label_color, Rgba::DEFAULT);
        assert!(style.junction_visible);
    }

    #[test]
    fn byte_order_mark_is_accepted() {
        let loaded = from_json("\u{FEFF}{\"version\": 3}").unwrap();
        assert_eq!(loaded.version, 3);
    }

    #[test]
    fn bezier_wins_if_both_flags_are_set() {
        let text = r#"{ "version": 3, "reactions": [
            { "id": "R1", "isLinear": true, "isBezier": true } ] }"#;
        let m = from_json(text).unwrap().model;
        assert_eq!(m.reactions()[0].mode, ReactionMode::Bezier);
        let saved: serde_json::Value = serde_json::from_str(&to_json(&m)).unwrap();
        assert_eq!(saved["reactions"][0]["isLinear"], false);
        assert_eq!(saved["reactions"][0]["isBezier"], true);
    }

    #[test]
    fn aliases_are_resolved_after_all_species_are_read() {
        let text = r#"{ "version": 3, "species": [
            { "id": "S2", "aliasOf": "S1" },
            { "id": "S3", "aliasOf": "S2" },
            { "id": "S1" },
            { "id": "S4", "aliasOf": "nobody" }
        ] }"#;
        let loaded = from_json(text).unwrap();
        let m = &loaded.model;
        assert_eq!(
            m.find_species("S2").unwrap().alias_of.as_deref(),
            Some("S1")
        );
        // An alias of an alias points at the primary.
        assert_eq!(
            m.find_species("S3").unwrap().alias_of.as_deref(),
            Some("S1")
        );
        // An alias of a missing species becomes a species of its own.
        assert_eq!(m.find_species("S4").unwrap().alias_of, None);
        assert_eq!(loaded.warnings.len(), 2);
    }

    #[test]
    fn participants_of_missing_species_are_skipped() {
        let text = r#"{ "version": 3, "species": [ { "id": "A" } ], "reactions": [
            { "id": "R1", "reactants": [ { "id": "A" } ], "products": [ { "id": "Z" } ],
              "modifiers": [ { "id": "Y" } ] } ] }"#;
        let loaded = from_json(text).unwrap();
        let r = loaded.model.find_reaction("R1").unwrap();
        assert_eq!(r.reactants.len(), 1);
        assert!(r.products.is_empty());
        assert!(r.modifiers.is_empty());
        assert_eq!(loaded.warnings.len(), 2);
    }

    #[test]
    fn duplicate_ids_are_skipped_with_a_warning() {
        let text =
            r#"{ "version": 3, "species": [ { "id": "A", "x": 1 }, { "id": "A", "x": 2 } ] }"#;
        let loaded = from_json(text).unwrap();
        assert_eq!(loaded.model.species().len(), 1);
        assert_eq!(loaded.model.species()[0].center.x, 1.0);
        assert_eq!(loaded.warnings.len(), 1);
    }

    #[test]
    fn id_counter_is_recomputed_on_load() {
        let text = r#"{ "version": 3, "species": [ { "id": "S7" } ],
            "reactions": [ { "id": "R12" } ] }"#;
        assert_eq!(from_json(text).unwrap().model.next_id(), 13);
    }

    #[test]
    fn free_text_is_version_4_and_round_trips() {
        let mut m = Model::new();
        m.add_species(Pt::ZERO, 80.0, 36.0);
        let plain = to_json(&m);
        assert!(plain.contains("\"version\": 3"), "no text: still version 3");
        assert!(!plain.contains("texts"));

        let id = m.add_text(pt(10.0, -20.0), "Glycolysis\nin yeast");
        let t = m.find_text_mut(&id).unwrap();
        t.font = TextFont {
            typeface: Typeface::Serif,
            bold: true,
            italic: true,
        };
        t.size = 18.0;
        t.align = TextAlign::Center;
        t.color = Rgba::rgb(0xCC, 0x00, 0x00);
        t.wrap_width = 120.0;
        m.add_text(pt(0.0, 0.0), "plain");
        let text = to_json(&m);
        assert!(text.contains("\"version\": 4"));
        assert!(text.contains("\"typeface\": \"serif\""));
        assert!(text.contains("\"align\": \"center\""));
        assert!(text.contains("\"color\": \"#CC0000FF\""));
        let loaded = from_json(&text).unwrap();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert!(!loaded.is_newer_than_supported());
        assert_eq!(loaded.model.texts(), m.texts());
        // Defaults are left out of the file.
        let plain_text = text.split("\"plain\"").nth(1).unwrap();
        assert!(!plain_text.contains("bold") && !plain_text.contains("wrapWidth"));
        assert!(!plain_text.contains("align"));
    }

    #[test]
    fn a_drawn_compartment_is_version_4_and_round_trips() {
        let mut m = Model::new();
        m.compartments.push(Compartment::new("hidden"));
        assert!(
            to_json(&m).contains("\"version\": 3"),
            "not drawn: version 3"
        );
        let id = m.add_compartment(Frame {
            center: pt(100.0, 50.0),
            width: 300.0,
            height: 200.0,
        });
        let text = to_json(&m);
        assert!(text.contains("\"version\": 4"));
        let back = from_json(&text).unwrap().model;
        assert_eq!(
            back.find_compartment(&id).unwrap().frame,
            m.find_compartment(&id).unwrap().frame
        );
        assert_eq!(back.find_compartment("hidden").unwrap().frame, None);
        assert!(!text.contains("\"hidden\": "), "a shown box says nothing");
        // A hidden box keeps its place.
        m.find_compartment_mut(&id).unwrap().hidden = true;
        let back = from_json(&to_json(&m)).unwrap().model;
        let c = back.find_compartment(&id).unwrap();
        assert!(c.hidden);
        assert_eq!(c.frame, m.find_compartment(&id).unwrap().frame);
        assert!(
            !text
                .split("\"hidden\"")
                .nth(1)
                .unwrap()
                .split('}')
                .next()
                .unwrap()
                .contains("\"x\"")
        );
    }

    #[test]
    fn missing_text_keys_take_their_defaults() {
        let text = r#"{ "version": 4, "texts": [ { "id": "T1", "text": "Hi" },
            { "id": "T1" } ] }"#;
        let loaded = from_json(text).unwrap();
        let t = &loaded.model.texts()[0];
        assert_eq!(t.size, DEFAULT_TEXT_SIZE);
        assert_eq!(t.font, TextFont::default());
        assert_eq!(t.color, Rgba::DEFAULT);
        assert_eq!(loaded.warnings.len(), 1, "the second T1 is skipped");
        assert_eq!(loaded.model.next_id(), 2);
    }

    #[test]
    fn newer_versions_load_with_a_warning() {
        let text = r#"{ "version": 12, "species": [ { "id": "A", "glow": true } ] }"#;
        let loaded = from_json(text).unwrap();
        assert!(loaded.is_newer_than_supported());
        assert_eq!(loaded.model.species().len(), 1);
        assert_eq!(loaded.warnings.len(), 1);
    }

    #[test]
    fn modifier_kinds_are_version_11_and_round_trip() {
        use crate::model::ModifierKind;
        let mut m = Model::new();
        let a = m.add_species(Pt::ZERO, 80.0, 36.0);
        let e = m.add_species(pt(0.0, -100.0), 80.0, 36.0);
        let i = m.add_species(pt(100.0, -100.0), 80.0, 36.0);
        let r = m.add_reaction(pt(100.0, 0.0));
        {
            let r = m.find_reaction_mut(&r).unwrap();
            r.reactants.push(Participant::new(&a, 1.0));
            r.modifiers.push(Participant::new(&e, 1.0));
            r.modifiers.push(Participant::new(&i, 1.0));
        }
        // Plain modifiers: no kind written, and no newer version.
        let text = to_json(&m);
        assert!(!text.contains("\"kind\""));
        assert_eq!(needed_version(&m), DELPHI_VERSION);
        for (k, key) in [
            (ModifierKind::Activator, "activator"),
            (ModifierKind::Inhibitor, "inhibitor"),
            (ModifierKind::Catalyst, "catalyst"),
        ] {
            m.find_reaction_mut(&r).unwrap().modifiers[1].kind = k;
            assert_eq!(needed_version(&m), 11);
            let text = to_json(&m);
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["version"], 11);
            let mods = &v["reactions"][0]["modifiers"];
            assert!(mods[0].get("kind").is_none(), "a plain modifier has none");
            assert_eq!(mods[1]["kind"], key);
            // Reactants and products never have one.
            assert!(v["reactions"][0]["reactants"][0].get("kind").is_none());
            let back = from_json(&text).unwrap().model;
            let kinds: Vec<_> = back.reactions()[0]
                .modifiers
                .iter()
                .map(|m| m.kind)
                .collect();
            assert_eq!(kinds, [ModifierKind::Modifier, k]);
        }
        // An unknown kind reads as a plain modifier.
        let text = r#"{ "version": 11, "species": [ { "id": "E" } ], "reactions": [
            { "id": "R1", "modifiers": [ { "id": "E", "kind": "glow" } ] } ] }"#;
        let back = from_json(text).unwrap().model;
        assert_eq!(
            back.reactions()[0].modifiers[0].kind,
            ModifierKind::Modifier
        );
    }

    #[test]
    fn broken_json_reports_where() {
        let err = from_json("{\n  \"version\": 3,\n  \"species\": [ { \"id\": } ]\n}").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.column > 0);
    }

    #[test]
    fn writer_uses_the_delphi_layout() {
        let mut m = Model::new();
        m.model_name = "demo".into();
        m.compartments.push(Compartment::new("defaultCompartment"));
        let a = m.add_species(Pt::ZERO, 80.0, 36.0);
        let b = m.add_species(pt(200.0, 0.0), 80.0, 36.0);
        let e = m.add_species(pt(100.0, -100.0), 80.0, 36.0);
        let r = m.add_reaction(pt(100.0, 0.0));
        {
            let r = m.find_reaction_mut(&r).unwrap();
            r.reactants.push(Participant::new(&a, 1.0));
            let mut p = Participant::new(&b, 2.0);
            p.ctrl_pts_set = true;
            p.ctrl1 = pt(1.5, 2.5);
            p.ctrl2 = pt(3.5, 4.5);
            r.products.push(p);
            r.modifiers.push(Participant::new(&e, 1.0));
            r.mode = ReactionMode::Bezier;
        }
        let s = m.find_species_mut(&a).unwrap();
        s.style.has_custom_style = true;
        s.style.fill_color = Rgba([0xFF, 0xE0, 0xB2, 0xFF]);
        s.selected = true;

        let text = to_json(&m);
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["version"], 3);
        assert!(text.starts_with("{\n  \"version\": 3,\n  \"modelName\": \"demo\","));
        assert_eq!(v["compartments"][0]["dimensions"], 3);

        let s0 = &v["species"][0];
        let keys: Vec<&str> = s0.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "id",
                "x",
                "y",
                "w",
                "h",
                "initialValue",
                "isBoundary",
                "isConstant",
                "style"
            ]
        );
        assert_eq!(s0["style"]["fillColor"], "#FFE0B2FF");
        assert_eq!(s0["style"]["labelColor"], "#00000000");
        assert_eq!(s0["style"].as_object().unwrap().len(), 9);
        assert!(v["species"][1].get("style").is_none());
        assert!(
            text.find("\"selected\"").is_none(),
            "selection is not saved"
        );

        let r0 = &v["reactions"][0];
        // Automatic handles are never saved.
        assert!(r0["reactants"][0].get("ctrl1x").is_none());
        assert!(r0["reactants"][0].get("ctrlSet").is_none());
        let p = &r0["products"][0];
        assert_eq!(
            (
                p["ctrl1x"].as_f64(),
                p["ctrl2y"].as_f64(),
                p["ctrlSet"].as_bool()
            ),
            (Some(1.5), Some(4.5), Some(true))
        );
        assert_eq!(p["stoichiometry"], 2.0);
        // Modifiers have only an id.
        assert_eq!(r0["modifiers"][0].as_object().unwrap().len(), 1);
        assert!(r0.get("style").is_none());
    }

    #[test]
    fn empty_modifier_lists_are_written() {
        let mut m = Model::new();
        m.add_reaction(Pt::ZERO);
        let v: serde_json::Value = serde_json::from_str(&to_json(&m)).unwrap();
        assert_eq!(v["reactions"][0]["modifiers"], serde_json::json!([]));
    }

    #[test]
    fn hidden_junction_is_saved_without_a_custom_style() {
        // D-8: junctionVisible = false must survive a save even when the
        // reaction has no other custom style.
        let mut m = Model::new();
        let r = m.add_reaction(Pt::ZERO);
        m.find_reaction_mut(&r).unwrap().style.junction_visible = false;
        let text = to_json(&m);
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let style = &v["reactions"][0]["style"];
        assert_eq!(style["junctionVisible"], false);
        assert_eq!(style["lineColor"], "#00000000");
        assert_eq!(style["lineWidth"], 0.0);
        let back = from_json(&text).unwrap().model;
        assert!(!back.reactions()[0].style.junction_visible);
    }

    #[test]
    fn locked_and_null_nodes_are_saved_only_when_true() {
        let mut m = Model::new();
        let a = m.add_species(Pt::ZERO, 80.0, 36.0);
        let b = m.add_species(Pt::ZERO, 28.0, 28.0);
        m.find_species_mut(&a).unwrap().locked = true;
        m.find_species_mut(&b).unwrap().is_null_node = true;
        let v: serde_json::Value = serde_json::from_str(&to_json(&m)).unwrap();
        assert_eq!(v["species"][0]["locked"], true);
        assert!(v["species"][0].get("isNullNode").is_none());
        assert_eq!(v["species"][1]["isNullNode"], true);
        assert!(v["species"][1].get("locked").is_none());
        let back = from_json(&to_json(&m)).unwrap().model;
        assert!(back.find_species(&a).unwrap().locked);
        assert!(back.find_species(&b).unwrap().is_null_node);
    }

    #[test]
    fn save_and_reload_gives_the_same_model() {
        let mut m = Model::new();
        m.model_name = "round trip".into();
        let a = m.add_species(pt(0.1, 0.2), 80.0, 36.0);
        let b = m.add_alias(&a, pt(1.0 / 3.0, 2.0 / 3.0)).unwrap();
        let r = m.add_reaction(pt(516.666625976562, 261.333343505859));
        let rr = m.find_reaction_mut(&r).unwrap();
        rr.reactants.push(Participant::new(&a, 1.5));
        rr.products.push(Participant::new(&b, 1.0));
        rr.kinetic_law = "k1*S1".into();
        let loaded = from_json(&to_json(&m)).unwrap();
        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.model, m);
    }
}
