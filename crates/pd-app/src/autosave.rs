//! Autosave and recovery (SPEC-Rust §10.2).
//!
//! While the diagram has unsaved changes, it is written to the app's storage
//! every 10 seconds (eframe's storage: `localStorage` in the browser, a file in
//! the app's data folder on the desktop). Once it is saved, or has no
//! changes, the autosave is removed. So an autosave found at startup means
//! changes were lost, and the user is offered them back.
//!
//! The stored text is one header line, then the diagram's JSON:
//! `PathwayDesigner autosave 1\t<unix seconds>\t<name>\n<json>`.
//! Autosave never adds anything to the document format.

/// The storage key (shared with the crash page in `web.rs`).
pub const KEY: &str = "pathwayDesigner.autosave";
/// How often eframe calls `save`, which writes the autosave.
pub const INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

const MAGIC: &str = "PathwayDesigner autosave 1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Autosave {
    /// The diagram's name when it was autosaved.
    pub name: String,
    /// When, in seconds since 1970.
    pub time: u64,
    /// The diagram, as native JSON.
    pub json: String,
}

impl Autosave {
    pub fn encode(&self) -> String {
        let name = self.name.replace(['\t', '\n'], " ");
        format!("{MAGIC}\t{}\t{name}\n{}", self.time, self.json)
    }

    pub fn decode(text: &str) -> Option<Autosave> {
        let (header, json) = text.split_once('\n')?;
        let mut parts = header.splitn(3, '\t');
        if parts.next()? != MAGIC {
            return None;
        }
        Some(Autosave {
            time: parts.next()?.parse().ok()?,
            name: parts.next()?.to_owned(),
            json: json.to_owned(),
        })
    }

    /// How long ago it was saved, in words.
    pub fn age(&self, now: u64) -> String {
        let s = now.saturating_sub(self.time);
        match s {
            0..60 => "less than a minute ago".to_owned(),
            60..7200 => format!("{} minutes ago", s / 60),
            7200..172_800 => format!("{} hours ago", s / 3600),
            _ => format!("{} days ago", s / 86_400),
        }
    }
}

/// The diagram's JSON in a stored autosave, for the crash page.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn document_of(text: &str) -> Option<String> {
    Autosave::decode(text).map(|a| a.json)
}

/// Seconds since 1970.
pub fn now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let a = Autosave {
            name: "my\tmodel.json".into(),
            time: 1_700_000_000,
            json: "{\n  \"version\": 3\n}\n".into(),
        };
        let back = Autosave::decode(&a.encode()).unwrap();
        assert_eq!(back.name, "my model.json");
        assert_eq!(back.time, a.time);
        assert_eq!(back.json, a.json);
        assert_eq!(document_of(&a.encode()).unwrap(), a.json);
    }

    #[test]
    fn anything_else_is_not_an_autosave() {
        assert_eq!(Autosave::decode("{\"version\": 3}"), None);
        assert_eq!(
            Autosave::decode("PathwayDesigner autosave 2\t1\tx\n{}"),
            None
        );
        assert_eq!(Autosave::decode(""), None);
    }

    #[test]
    fn ages() {
        let a = Autosave {
            name: String::new(),
            time: 1000,
            json: String::new(),
        };
        assert_eq!(a.age(1010), "less than a minute ago");
        assert_eq!(a.age(1000 + 300), "5 minutes ago");
        assert_eq!(a.age(1000 + 3 * 3600), "3 hours ago");
        assert_eq!(a.age(1000 + 3 * 86_400), "3 days ago");
    }
}
