//! My Styles (S §8.6): the saved styles kept with the app, in eframe's
//! storage beside the preferences (a file on the desktop, `localStorage` in
//! the browser), as the JSON of a list of styles (SPEC-Rust §4.2).

use pd_core::model::SavedStyle;

/// The storage key.
pub const KEY: &str = "pathwayDesigner.styles";

/// My Styles as saved, or none. A list that doesn't read is ignored, and
/// styles that don't read are skipped.
pub fn load(storage: Option<&dyn eframe::Storage>) -> Vec<SavedStyle> {
    storage
        .and_then(|s| s.get_string(KEY))
        .and_then(|text| pd_core::json::styles_from_json(&text).ok())
        .map(|(styles, _)| styles)
        .unwrap_or_default()
}

pub fn save(styles: &[SavedStyle], storage: &mut dyn eframe::Storage) {
    storage.set_string(KEY, pd_core::json::styles_to_json(styles));
}

#[cfg(test)]
mod tests {
    use super::*;
    use pd_core::model::{Look, TextAlign, TextFont, VisualStyle};
    use std::collections::HashMap;

    #[derive(Default)]
    struct Memory(HashMap<String, String>);

    impl eframe::Storage for Memory {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn my_styles_come_back() {
        assert!(load(None).is_empty());
        let styles = vec![
            SavedStyle {
                name: "Title".into(),
                look: Look::Text {
                    font: TextFont::default(),
                    align: TextAlign::Center,
                    size: 30.0,
                    color: pd_core::model::Rgba::rgb(0xCC, 0, 0),
                },
            },
            SavedStyle {
                name: "Quiet".into(),
                look: Look::Reaction {
                    style: VisualStyle {
                        has_custom_style: true,
                        line_width: 4.0,
                        ..VisualStyle::default()
                    },
                },
            },
        ];
        let mut m = Memory::default();
        save(&styles, &mut m);
        assert!(m.0[KEY].contains("\"Title\""));
        assert_eq!(load(Some(&m)), styles);
        // Something that isn't a list of styles is ignored.
        m.0.insert(KEY.into(), "not json".into());
        assert!(load(Some(&m)).is_empty());
    }
}
