//! Drawing constants and colours, copied exactly from S §17.1 and §17.2.
//! Don't change them: diagrams must match the reference.
//!
//! Lengths are world units unless the name ends in `_PX` (screen points).

use crate::model::Rgba;

const fn hex(rgb: u32) -> Rgba {
    Rgba([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255])
}

// ----- S §17.1 ------------------------------------------------------------

pub const NODE_CORNER_RADIUS: f64 = 8.0;
pub const LABEL_FONT_SIZE: f64 = 12.0;
/// The space each side of a species' name, when a rename widens the node to
/// fit it (S §5.3) and for Fit to Name (S §7.10).
pub const FIT_MARGIN: f64 = 10.0;
/// The smallest a species can be made, either way (S §7.10).
pub const MIN_SPECIES_SIZE: f64 = 10.0;
/// The space between a node and its name, when the name is taken outside
/// (S §5.11).
pub const LABEL_GAP: f64 = 8.0;
/// The margin round a name drawn outside its node: the box that selects it
/// and is outlined when it is selected (S §5.11).
pub const LABEL_BOX_PAD: f64 = 2.0;
pub const DEFAULT_LINE_WIDTH: f64 = 1.5;
pub const BOUNDARY_BORDER_FACTOR: f64 = 2.4;
pub const SELECTED_LINE_FACTOR: f64 = 1.5;
pub const JUNCTION_RADIUS: f64 = 4.0;
pub const JUNCTION_RADIUS_SELECTED: f64 = 6.4;
pub const JUNCTION_BORDER_WIDTH: f64 = 1.0;
pub const MODIFIER_DOT_RADIUS: f64 = 4.0;
/// Distance of the modifier dot's centre from the junction.
pub const MODIFIER_DOT_OFFSET: f64 = 10.0;
/// A modifier arc starts this far inside the modifier node's border.
pub const MODIFIER_INSET: f64 = 6.0;
pub const REACTANT_GAP: f64 = 6.0;
pub const PRODUCT_LINE_GAP: f64 = 6.0;
pub const ARROW_TIP_INSET: f64 = 3.0;
pub const ARROW_LENGTH: f64 = 12.0;
pub const ARROW_HALF_BASE: f64 = 5.0;
pub const HALO_OUTSET: f64 = 6.0;
pub const COMPARTMENT_BORDER_WIDTH: f64 = 2.0;
pub const COMPARTMENT_CORNER_RADIUS: f64 = 14.0;
pub const COMPARTMENT_LABEL_SIZE: f64 = 12.0;
/// The name's distance from the box's top-left corner.
pub const COMPARTMENT_LABEL_INSET: f64 = 8.0;
pub const HALO_WIDTH: f64 = 2.0;
pub const HALO_CORNER_RADIUS: f64 = 14.0;
pub const PENDING_RING_OUTSET: f64 = 4.0;
pub const PENDING_RING_WIDTH: f64 = 2.5;
pub const HANDLE_RADIUS: f64 = 4.0;
pub const HANDLE_HIT_TOLERANCE_PX: f64 = 8.0;
pub const JUNCTION_HIT_TOLERANCE_PX: f64 = 8.0;
pub const LEG_HIT_TOLERANCE_PX: f64 = 6.0;
pub const BEZIER_HIT_SAMPLES: usize = 20;
pub const HANDLE_FRACTION: f64 = 0.35;
pub const FAN_SPREAD: f64 = 50.0;
pub const ZOOM_STEP: f64 = 1.15;
pub const ZOOM_MIN: f64 = 0.1;
pub const ZOOM_MAX: f64 = 8.0;
pub const TOOLTIP_DELAY_MS: u64 = 600;
pub const CONTENT_MARGIN: f64 = 60.0;
pub const PNG_DEFAULT_DPI: f64 = 150.0;
/// Null node cross: stroke width, and arm length as a fraction of the radius (S §5.4).
pub const NULL_CROSS_WIDTH: f64 = 1.8;
pub const NULL_CROSS_ARM: f64 = 0.55;

/// Dash patterns, (on, off) in world units (S §5.3, §5.7, §5.6.4).
pub const ALIAS_DASH: (f64, f64) = (5.0, 4.0);
pub const HALO_DASH: (f64, f64) = (10.0, 5.0);
pub const MODIFIER_DASH: (f64, f64) = (6.0, 4.0);
pub const HANDLE_GUIDE_DASH: (f64, f64) = (8.0, 4.0);

// ----- S §17.2 ------------------------------------------------------------

pub const BACKGROUND: Rgba = hex(0xF8F9FA);
pub const SPECIES_FILL: Rgba = hex(0xEEF6FF);
pub const SPECIES_BORDER: Rgba = hex(0x4A7FCB);
pub const ALIAS_FILL: Rgba = hex(0xF5F0FF);
pub const ALIAS_BORDER: Rgba = hex(0x7A6FC8);
pub const LABEL: Rgba = hex(0x1A1A1A);
/// Compartments (S §7.9): a pale fill, a darker border, the name in a darker
/// shade still.
pub const COMPARTMENT_FILL: Rgba = hex(0xF0F6EA);
pub const COMPARTMENT_BORDER: Rgba = hex(0x7FA36B);
pub const COMPARTMENT_LABEL: Rgba = hex(0x3D5C2B);
pub const REACTION_LINE: Rgba = hex(0x444444);
pub const SELECTED_ACCENT: Rgba = hex(0x1144CC);
/// The colour a selected reaction is drawn in, unless the preferences say
/// otherwise: the selection accent, as for its modifier arcs.
pub const SELECTED_REACTION: Rgba = SELECTED_ACCENT;
pub const JUNCTION_FILL: Rgba = hex(0xFF8800);
pub const JUNCTION_BORDER: Rgba = hex(0xA05000);
pub const JUNCTION_BORDER_SELECTED: Rgba = hex(0x0033AA);
pub const MODIFIER: Rgba = hex(0x007777);
pub const NULL_FILL: Rgba = hex(0xB0B0B0);
pub const NULL_BORDER: Rgba = hex(0x505050);
pub const SELECTION_HALO: Rgba = hex(0xFF0000);
pub const PENDING_REACTANT: Rgba = hex(0x00AA44);
pub const PENDING_PRODUCT: Rgba = hex(0xCC3300);
pub const GUIDE_LINE: Rgba = hex(0x888888);
pub const RUBBER_BAND: Rgba = hex(0x0066CC);
pub const HANDLE_FILL: Rgba = hex(0xFFFFFF);
pub const HANDLE_BORDER: Rgba = hex(0x888800);
pub const HANDLE_SMOOTH_INNER: Rgba = hex(0x009999);
pub const HANDLE_GUIDE: Rgba = hex(0xAAAAAA);
pub const TOOLTIP_FILL: Rgba = hex(0xFFFDE7);
pub const TOOLTIP_BORDER: Rgba = hex(0x888866);
pub const TOOLTIP_TEXT: Rgba = hex(0x333322);

/// The colour picker's swatches, 8 per row (S §17.2). Every default diagram
/// colour is among them.
pub const PICKER_SWATCHES: [[u32; 8]; 5] = [
    [
        0xFFFFFF, 0xF0F0F0, 0xD0D0D0, 0xB0B0B0, 0x808080, 0x505050, 0x282828, 0x000000,
    ],
    [
        0xEEF6FF, 0xCCE0FF, 0x9DBFFF, 0x4A7FCB, 0x2255CC, 0x1144CC, 0x003399, 0x0099CC,
    ],
    [
        0xF5F0FF, 0xE8DEFF, 0xD4C8FF, 0xB39DDB, 0x9575CD, 0x7A6FC8, 0x553399, 0x330066,
    ],
    [
        0x00AA44, 0x33CC66, 0x88CC00, 0xFFCC00, 0xFF8800, 0xA05000, 0xCC3300, 0xFF4444,
    ],
    [
        0xF8F9FA, 0x1A1A1A, 0x444444, 0x888888, 0xFFF9C4, 0xFFE0B2, 0xE8F5E9, 0xFFE8E8,
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours() {
        assert_eq!(SPECIES_BORDER, Rgba([0x4A, 0x7F, 0xCB, 0xFF]));
        assert_eq!(BACKGROUND, Rgba([0xF8, 0xF9, 0xFA, 0xFF]));
    }

    #[test]
    fn picker_keeps_every_default_diagram_colour() {
        let swatches: Vec<Rgba> = PICKER_SWATCHES.iter().flatten().map(|&c| hex(c)).collect();
        for c in [
            BACKGROUND,
            SPECIES_FILL,
            SPECIES_BORDER,
            ALIAS_FILL,
            ALIAS_BORDER,
            LABEL,
            REACTION_LINE,
            SELECTED_ACCENT,
            JUNCTION_FILL,
            JUNCTION_BORDER,
            NULL_FILL,
            NULL_BORDER,
            PENDING_REACTANT,
            PENDING_PRODUCT,
            GUIDE_LINE,
        ] {
            assert!(swatches.contains(&c), "{c:?} missing from the picker");
        }
    }
}
