//! Appearance never changes the model (the paper's evaluation E6, S §8.5–8.8):
//! themes, colours, line widths, shapes, fonts, saved and pasted styles,
//! hidden boxes and the background change how a diagram looks, and nothing
//! a simulator would see. Checked on every model in the Gallery: after each
//! change, the model written as Antimony and as SBML without its layout is
//! exactly what it was. Undoing every change then gives back the diagram
//! itself.

use std::path::Path;

use pd_core::model::{BorderDash, Model, NodeShape, Rgba, StyleKind};
use pd_core::themes::Theme;
use pd_core::view::{ColourField, DiagramView, Target};

/// Every model the Gallery lists, Escher maps included, with its title.
fn gallery() -> Vec<(String, Model)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("Gallery/index.json")).unwrap())
            .unwrap();
    index["examples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let text = std::fs::read_to_string(root.join(e["file"].as_str().unwrap())).unwrap();
            let model = if pd_core::escher::looks_like_escher(&text) {
                pd_core::escher::import(&text).unwrap().model
            } else {
                pd_core::json::from_json(&text).unwrap().model
            };
            (e["title"].as_str().unwrap().to_owned(), model)
        })
        .collect()
}

/// The model as a simulator sees it: Antimony, and SBML with the layout
/// taken out.
fn biochemistry(model: &Model) -> (String, String) {
    let xml = pd_core::sbml::export(model).xml;
    let start = xml.find("<layout:listOfLayouts").expect("a layout");
    let end = xml.find("</layout:listOfLayouts>").unwrap() + "</layout:listOfLayouts>".len();
    (
        pd_core::antimony::export(model),
        format!("{}{}", &xml[..start], &xml[end..]),
    )
}

#[test]
fn appearance_never_changes_the_model() {
    let red = Rgba::rgb(0xCC, 0x22, 0x11);
    for (title, model) in gallery() {
        let mut v = DiagramView::new(model);
        // As opened: anything the app does on opening is not a change made here.
        let opened = v.model().clone();
        let want = biochemistry(&opened);
        let mut changed = Vec::new();
        let mut step = |v: &mut DiagramView, what: &str, f: &dyn Fn(&mut DiagramView)| {
            f(v);
            if biochemistry(v.model()) != want {
                changed.push(what.to_owned());
            }
        };
        step(&mut v, "Escher theme", &|v| {
            v.apply_theme(Theme::Escher);
        });
        step(&mut v, "Classic theme", &|v| {
            v.apply_theme(Theme::Classic);
        });
        step(&mut v, "Escher theme again", &|v| {
            v.apply_theme(Theme::Escher);
        });
        step(&mut v, "colours", &|v| {
            v.select_all();
            for field in [
                ColourField::Fill,
                ColourField::Border,
                ColourField::Label,
                ColourField::Line,
                ColourField::ReactionLabel,
                ColourField::Text,
                ColourField::CompartmentFill,
                ColourField::CompartmentBorder,
                ColourField::Background,
            ] {
                v.apply_colour(field, red);
            }
        });
        step(&mut v, "shapes, borders and names", &|v| {
            v.select_all();
            v.set_species_shape(NodeShape::Hexagon);
            v.set_species_border_dash(BorderDash::Dashed);
            v.set_species_label_outside(true);
            v.set_font_size(20.0);
            v.set_name_bold(true, true);
            v.set_name_italic(false, true);
            v.set_reaction_font_size(16.0);
            v.set_reaction_label_shown(true);
            v.set_junction_visible(false);
            v.set_species_size(Some(120.0), Some(50.0));
            v.fit_species_to_names();
            v.set_text_size(30.0);
            v.set_compartment_font_size(25.0);
        });
        step(&mut v, "line and border widths", &|v| {
            // A few of each: every one is an undo step, and the history keeps
            // only the last 100.
            let reactions: Vec<String> = v
                .model()
                .reactions()
                .iter()
                .take(5)
                .map(|r| r.id.clone())
                .collect();
            for id in reactions {
                v.set_line_width(&id, 6.0);
            }
            let species: Vec<String> = v
                .model()
                .species()
                .iter()
                .take(5)
                .map(|s| s.id.clone())
                .collect();
            for id in species {
                v.set_border_width(&id, 3.0);
            }
        });
        step(&mut v, "hidden compartments", &|v| {
            v.select_all();
            v.set_compartment_hidden(true);
        });
        step(&mut v, "saved and pasted styles", &|v| {
            let first = v.model().species().first().map(|s| s.id.clone());
            let Some(first) = first else { return };
            let item = Target::Species(first);
            v.save_style(&item, "E6 style").unwrap();
            v.select_all();
            v.apply_style(StyleKind::Species, "E6 style");
            v.copy_style(&item);
            v.select_all();
            v.paste_style();
        });
        assert!(
            changed.is_empty(),
            "{title}: the model changed after {changed:?}"
        );
        // The changes were made: the drawing is not what it was (null nodes
        // keep their own look).
        let m = v.model();
        assert_ne!(
            pd_core::json::to_json(m),
            pd_core::json::to_json(&opened),
            "{title}"
        );
        assert_eq!(m.background, red, "{title}: background");
        assert!(
            m.species()
                .iter()
                .all(|s| s.is_null_node || s.shape == NodeShape::Hexagon),
            "{title}: shapes"
        );

        // Every change undone: the diagram as it was opened.
        while v.can_undo() {
            v.undo();
        }
        assert_eq!(
            pd_core::json::to_json(v.model()),
            pd_core::json::to_json(&opened),
            "{title}: undo"
        );
    }
}
