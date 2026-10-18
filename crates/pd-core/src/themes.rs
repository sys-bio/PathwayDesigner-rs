//! Themes (S §8.7): a whole diagram restyled at once. A theme is a set of
//! saved styles (S §8.6), one for each kind of item (two for species:
//! metabolites and cofactors); applying it gives every item its style, and
//! while it is the diagram's theme, items added later get theirs too.

use crate::model::{
    BorderDash, Look, Model, NodeShape, ReactionMode, Rgba, SavedStyle, StyleKind, TextFont,
    Typeface, VisualStyle,
};
use crate::render::draw;
use crate::styles;

/// The built-in themes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Theme {
    /// Today's look: the defaults, no saved styles.
    Classic,
    /// After Escher's metabolic maps: small orange circles with their names
    /// beside them, smaller cofactors, thick slate-blue reactions.
    Escher,
}

impl Theme {
    pub const ALL: [Theme; 2] = [Theme::Classic, Theme::Escher];

    /// Its name in menus.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Classic => "Classic",
            Theme::Escher => "Escher",
        }
    }

    /// Its name in files.
    pub fn key(self) -> &'static str {
        match self {
            Theme::Classic => "classic",
            Theme::Escher => "escher",
        }
    }

    pub fn from_key(key: &str) -> Option<Theme> {
        Theme::ALL.into_iter().find(|t| t.key() == key)
    }
}

/// The Escher theme's styles, by name.
pub const ESCHER_METABOLITE: &str = "Escher metabolite";
pub const ESCHER_COFACTOR: &str = "Escher cofactor";
pub const ESCHER_REACTION: &str = "Escher reaction";
pub const ESCHER_COMPARTMENT: &str = "Escher compartment";

const ESCHER_FILL: Rgba = Rgba([0xE0, 0x86, 0x5B, 0xFF]);
const ESCHER_BORDER: Rgba = Rgba([0xA2, 0x45, 0x10, 0xFF]);
const ESCHER_LINE: Rgba = Rgba([0x33, 0x4E, 0x75, 0xFF]);
const ESCHER_COMPARTMENT_FILL: Rgba = Rgba([0xFA, 0xFA, 0xFA, 0xFF]);
const ESCHER_COMPARTMENT_BORDER: Rgba = Rgba([0xC8, 0xCD, 0xD3, 0xFF]);
/// The diameter of a metabolite's circle, and of a cofactor's.
const ESCHER_METABOLITE_SIZE: f64 = 26.0;
const ESCHER_COFACTOR_SIZE: f64 = 16.0;
/// The size of a metabolite's name, and of a cofactor's.
const ESCHER_METABOLITE_FONT: f64 = 18.0;
const ESCHER_COFACTOR_FONT: f64 = 14.0;
/// The size of a reaction's name (S §5.12).
const ESCHER_REACTION_FONT: f64 = 16.0;

/// The Escher theme's saved styles, kept as drawn.
pub fn escher_styles() -> Vec<SavedStyle> {
    let circle = |size: f64, border_width: f64, font_size: f64| Look::Species {
        style: VisualStyle {
            has_custom_style: true,
            fill_color: ESCHER_FILL,
            border_color: ESCHER_BORDER,
            border_width,
            font_size,
            ..VisualStyle::default()
        },
        shape: NodeShape::Circle,
        border_dash: BorderDash::Solid,
        label_outside: true,
        size: Some((size, size)),
        alias: false,
    };
    [
        (
            ESCHER_METABOLITE,
            circle(ESCHER_METABOLITE_SIZE, 2.0, ESCHER_METABOLITE_FONT),
        ),
        (
            ESCHER_COFACTOR,
            circle(ESCHER_COFACTOR_SIZE, 1.5, ESCHER_COFACTOR_FONT),
        ),
        (
            ESCHER_REACTION,
            Look::Reaction {
                style: VisualStyle {
                    has_custom_style: true,
                    line_color: ESCHER_LINE,
                    line_width: 5.0,
                    label_color: ESCHER_LINE,
                    font_size: ESCHER_REACTION_FONT,
                    junction_visible: false,
                    ..VisualStyle::default()
                },
            },
        ),
        (
            ESCHER_COMPARTMENT,
            Look::Compartment {
                style: VisualStyle {
                    has_custom_style: true,
                    fill_color: ESCHER_COMPARTMENT_FILL,
                    border_color: ESCHER_COMPARTMENT_BORDER,
                    border_width: 1.5,
                    ..VisualStyle::default()
                },
            },
        ),
    ]
    .into_iter()
    .map(|(name, look)| SavedStyle {
        name: name.to_owned(),
        look: styles::resolved(look),
    })
    .collect()
}

/// Cofactors and other small molecules drawn smaller in the Escher theme,
/// as names reduce to them (see [`is_cofactor`]).
const COFACTORS: &[&str] = &[
    "atp",
    "adp",
    "amp",
    "gtp",
    "gdp",
    "gmp",
    "utp",
    "udp",
    "ump",
    "ctp",
    "cdp",
    "cmp",
    "nad",
    "nadh",
    "nadp",
    "nadph",
    "fad",
    "fadh2",
    "fmn",
    "fmnh2",
    "coa",
    "h",
    "hin",
    "hout",
    "proton",
    "h2o",
    "water",
    "pi",
    "ppi",
    "phosphate",
    "co2",
    "o2",
    "nh3",
    "nh4",
    "hco3",
    "h2o2",
    "q",
    "qh2",
    "q8",
    "q8h2",
    "mgatp",
    "mgadp",
];

/// True if species `id` is a cofactor or other small molecule: its name,
/// without an SBML `M_` prefix or a one-letter compartment suffix (`_c`,
/// `_e`…) and ignoring case and a charge sign, is in a built-in list (ATP,
/// ADP, NAD(P)(H), H₂O, Pi, CO₂, H⁺, CoA…).
pub fn is_cofactor(id: &str) -> bool {
    let mut name = id.strip_prefix("M_").unwrap_or(id);
    if let Some((base, suffix)) = name.rsplit_once('_')
        && suffix.len() == 1
        && suffix.chars().all(|c| c.is_ascii_alphabetic())
    {
        name = base;
    }
    let name = name.trim_end_matches(['+', '-']).to_ascii_lowercase();
    COFACTORS.contains(&name.as_str())
}

/// The theme's style for the item `id` of `kind`, if it gives that kind one.
fn style_for(theme: Theme, model: &Model, kind: StyleKind, id: &str) -> Option<&'static str> {
    if theme != Theme::Escher {
        return None;
    }
    match kind {
        StyleKind::Species => {
            let s = model.find_species(id).filter(|s| !s.is_null_node)?;
            Some(if is_cofactor(s.primary_id()) {
                ESCHER_COFACTOR
            } else {
                ESCHER_METABOLITE
            })
        }
        StyleKind::Reaction => Some(ESCHER_REACTION),
        StyleKind::Compartment => Some(ESCHER_COMPARTMENT),
        StyleKind::Text => None,
    }
}

/// Gives the item the theme's look: its style (and the item follows it), or
/// for a text under Escher, the theme's typeface and colour.
fn theme_item(model: &mut Model, theme: Theme, kind: StyleKind, id: &str) {
    if kind == StyleKind::Text {
        if let Some(t) = model.find_text_mut(id) {
            match theme {
                Theme::Escher => {
                    t.font.typeface = Typeface::Sans;
                    t.color = ESCHER_LINE;
                }
                Theme::Classic => {
                    t.font.typeface = TextFont::default().typeface;
                    t.color = Rgba::DEFAULT;
                }
            }
        }
        return;
    }
    let Some(name) = style_for(theme, model, kind, id) else {
        return;
    };
    let Some(look) = styles::find(model, kind, name).map(|s| s.look) else {
        return;
    };
    // A name already outside (one taken outside now is placed at its new
    // size) and now a different size keeps its gap from the node, so a
    // bigger one doesn't overlap it. The same for a reaction's name.
    let was_outside = match kind {
        StyleKind::Species => model
            .find_species(id)
            .filter(|s| s.label_offset.is_some())
            .map(draw::label_size),
        StyleKind::Reaction => model
            .find_reaction(id)
            .filter(|r| r.label_offset.is_some())
            .map(draw::reaction_label_size),
        _ => None,
    };
    styles::give_look(model, id, &look);
    if let Some(size) = was_outside {
        if let Some(s) = model
            .find_species_mut(id)
            .filter(|_| kind == StyleKind::Species)
            && let Some(o) = s.label_offset
        {
            s.label_offset = Some(draw::label_offset_resized(s, o, size));
        }
        if let Some(r) = model
            .find_reaction_mut(id)
            .filter(|_| kind == StyleKind::Reaction)
            && let Some(o) = r.label_offset
        {
            r.label_offset = Some(draw::reaction_label_offset_resized(r, o, size));
        }
    }
    styles::set_link(model, kind, id, Some(name.to_owned()));
    if kind == StyleKind::Reaction {
        curve(model, id);
        show_name(model, id);
    }
}

/// Shows the reaction's name, as Escher does (S §8.7), beside its junction
/// on the side clearest of its lines (S §5.12); a name already shown stays
/// where it is.
fn show_name(model: &mut Model, id: &str) {
    let Some(r) = model.find_reaction(id).filter(|r| r.label_offset.is_none()) else {
        return;
    };
    let place = draw::clear_reaction_label_offset(model, r);
    if let Some(r) = model.find_reaction_mut(id) {
        r.label_offset = Some(place);
    }
}

/// Makes the reaction a Bézier with a smooth junction, as Escher draws its
/// reactions, and gives it the smooth junction's handles (S §7.4.4), as the
/// Smooth junction command does. A Linear reaction's junction, unused until
/// now, first goes halfway between its two species.
fn curve(model: &mut Model, id: &str) {
    let Some(r) = model.find_reaction(id) else {
        return;
    };
    // A reaction with a stem (an Escher map's, S §5.13) is drawn as it is.
    if r.stem.is_some() {
        if let Some(r) = model.find_reaction_mut(id) {
            r.mode = ReactionMode::Bezier;
        }
        return;
    }
    let mid = if r.mode == ReactionMode::Linear && r.is_uni_uni() {
        let a = model
            .find_species(&r.reactants[0].species)
            .map(|s| s.center);
        let b = model.find_species(&r.products[0].species).map(|s| s.center);
        a.zip(b).map(|(a, b)| (a + b) / 2.0)
    } else {
        None
    };
    let r = model.find_reaction_mut(id).expect("found above");
    if let Some(mid) = mid {
        r.junction = mid;
    }
    r.mode = ReactionMode::Bezier;
    r.junction_smooth = true;
    crate::bezier::apply_smooth_handles(model, id);
}

/// Applies `theme` to the whole diagram (S §8.7): every item, hand-styled
/// ones too, takes the theme's look. Escher adds its styles to the model
/// (replacing older copies) and every item follows them; Classic puts back
/// the default look of everything, unlinks every item and drops Escher's
/// styles. A species' size comes with Escher's circles; Classic makes each
/// species 80 × 36, or wide enough for its name.
pub fn apply(model: &mut Model, theme: Theme) {
    match theme {
        Theme::Escher => {
            for st in escher_styles() {
                styles::put(&mut model.styles, st);
            }
            for kind in StyleKind::ALL {
                for id in styles::item_ids(model, kind) {
                    theme_item(model, theme, kind, &id);
                }
            }
            model.theme = Some(Theme::Escher);
        }
        Theme::Classic => {
            use crate::model::{DEFAULT_SPECIES_HEIGHT, DEFAULT_SPECIES_WIDTH};
            for s in model.species_mut().filter(|s| !s.is_null_node) {
                s.style = VisualStyle {
                    junction_visible: s.style.junction_visible,
                    ..VisualStyle::default()
                };
                s.shape = NodeShape::default();
                s.border_dash = BorderDash::default();
                s.label_offset = None;
                let fit = crate::render::draw::width_for_name(s).ceil();
                s.resize(DEFAULT_SPECIES_WIDTH.max(fit), DEFAULT_SPECIES_HEIGHT);
                s.style_name = None;
            }
            for r in model.reactions_mut() {
                r.style = VisualStyle::default();
                r.style_name = None;
                r.label_offset = None;
            }
            for c in &mut model.compartments {
                c.style = VisualStyle::default();
                c.style_name = None;
            }
            for id in styles::item_ids(model, StyleKind::Text) {
                theme_item(model, theme, StyleKind::Text, &id);
                styles::set_link(model, StyleKind::Text, &id, None);
            }
            let escher: Vec<String> = escher_styles().into_iter().map(|s| s.name).collect();
            model.styles.retain(|s| !escher.contains(&s.name));
            model.theme = None;
        }
    }
}

/// While the diagram has a theme, items an edit added (in `model`, not in
/// `before`) take its look, as they would had the theme been applied after.
/// Only kinds with more items than before count, so a rename, which changes
/// an id, never re-themes an item restyled by hand.
pub fn style_new_items(before: &Model, model: &mut Model) {
    let Some(theme) = model.theme else {
        return;
    };
    for kind in StyleKind::ALL {
        let old: std::collections::HashSet<String> =
            styles::item_ids(before, kind).into_iter().collect();
        let now = styles::item_ids(model, kind);
        if now.len() <= old.len() {
            continue;
        }
        for id in now.iter().filter(|id| !old.contains(*id)) {
            theme_item(model, theme, kind, id);
        }
    }
}
