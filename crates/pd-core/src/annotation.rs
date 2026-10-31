//! What the panels need to show and edit annotations
//! (docs/annotations-design.md, step 4): names for common SBO terms, links
//! for references, the qualifiers to choose from, and notes as text.

/// What kind of thing an SBO term is for: each has its own common terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SboKind {
    Species,
    Reaction,
    Compartment,
    Parameter,
    /// The whole model: its modelling framework.
    Model,
}

/// Common SBO terms for species (A5).
const SPECIES_TERMS: &[(u32, &str)] = &[
    (247, "simple chemical"),
    (245, "macromolecule"),
    (252, "polypeptide chain"),
    (250, "ribonucleic acid"),
    (251, "deoxyribonucleic acid"),
    (253, "non-covalent complex"),
];

const REACTION_TERMS: &[(u32, &str)] = &[
    (176, "biochemical reaction"),
    (182, "conversion"),
    (177, "non-covalent binding"),
    (180, "dissociation"),
    (179, "degradation"),
    (185, "transport reaction"),
    (183, "transcription"),
    (184, "translation"),
    (216, "phosphorylation"),
    (330, "dephosphorylation"),
];

const COMPARTMENT_TERMS: &[(u32, &str)] = &[(290, "physical compartment")];

const PARAMETER_TERMS: &[(u32, &str)] = &[
    (2, "quantitative systems description parameter"),
    (9, "kinetic constant"),
    (25, "catalytic rate constant"),
    (27, "Michaelis constant"),
    (186, "maximal velocity"),
    (193, "equilibrium or steady-state constant"),
    (356, "decay constant"),
];

/// A model's SBO term says which modelling framework its equations are for.
const MODEL_TERMS: &[(u32, &str)] = &[
    (62, "continuous framework"),
    (293, "non-spatial continuous framework"),
    (63, "discrete framework"),
    (234, "logical framework"),
    (624, "flux balance framework"),
];

/// The common SBO terms for `kind`, with their names: enough to name what
/// most models use; any other number can be typed, and is shown as its
/// number.
pub fn sbo_terms(kind: SboKind) -> &'static [(u32, &'static str)] {
    match kind {
        SboKind::Species => SPECIES_TERMS,
        SboKind::Reaction => REACTION_TERMS,
        SboKind::Compartment => COMPARTMENT_TERMS,
        SboKind::Parameter => PARAMETER_TERMS,
        SboKind::Model => MODEL_TERMS,
    }
}

const ALL_KINDS: [SboKind; 5] = [
    SboKind::Species,
    SboKind::Reaction,
    SboKind::Compartment,
    SboKind::Parameter,
    SboKind::Model,
];

/// The name of SBO term `n`, if it is one of the common terms of any kind.
pub fn sbo_name(n: u32) -> Option<&'static str> {
    ALL_KINDS
        .iter()
        .flat_map(|k| sbo_terms(*k))
        .find(|(t, _)| *t == n)
        .map(|(_, name)| *name)
}

/// `SBO:0000247`.
pub fn sbo_id(n: u32) -> String {
    format!("SBO:{n:07}")
}

/// Where a term is defined, to open in a browser.
pub fn sbo_url(n: u32) -> String {
    format!("https://identifiers.org/{}", sbo_id(n))
}

/// The biology qualifiers, for an item's references, as SBML names them.
pub const ITEM_QUALIFIERS: &[&str] = &[
    "bqbiol:is",
    "bqbiol:isVersionOf",
    "bqbiol:hasPart",
    "bqbiol:isPartOf",
    "bqbiol:hasVersion",
    "bqbiol:isHomologTo",
    "bqbiol:isDescribedBy",
    "bqbiol:isEncodedBy",
    "bqbiol:encodes",
    "bqbiol:occursIn",
    "bqbiol:hasProperty",
    "bqbiol:isPropertyOf",
    "bqbiol:hasTaxon",
];

/// The qualifiers for the model's references, the most used first: its
/// paper, its organism and the process it models, as curated models give
/// them; then its own entry in a repository, once it has one, and the rest.
pub const MODEL_QUALIFIERS: &[&str] = &[
    "bqmodel:isDescribedBy",
    "bqbiol:hasTaxon",
    "bqbiol:isVersionOf",
    "bqmodel:is",
    "bqmodel:isDerivedFrom",
    "bqbiol:isHomologTo",
    "bqbiol:hasProperty",
    "bqbiol:hasVersion",
];

/// What a model qualifier links the model to, and an identifier of that
/// kind to type, as BioModels' curated models use them.
pub fn model_qualifier_hint(q: &str) -> Option<(&'static str, &'static str)> {
    Some(match q {
        "bqmodel:isDescribedBy" => ("The paper that describes the model", "pubmed:8983160"),
        "bqbiol:hasTaxon" => ("The organism (here E. coli)", "taxonomy:562"),
        "bqbiol:isVersionOf" => (
            "The biological process or pathway it models (here glycolysis)",
            "GO:0006096",
        ),
        "bqmodel:is" => (
            "The model's own entry in a repository, once it has one",
            "biomodels.db:BIOMD0000000012",
        ),
        "bqmodel:isDerivedFrom" => (
            "A model this one was built from",
            "biomodels.db:BIOMD0000000012",
        ),
        "bqbiol:isHomologTo" => (
            "The same pathway in another organism or database (here Reactome's glycolysis)",
            "reactome:R-HSA-70171",
        ),
        "bqbiol:hasProperty" => (
            "What kind of model it is (here an ordinary differential equation model)",
            "mamo:MAMO_0000046",
        ),
        "bqbiol:hasVersion" => ("A narrower process it also covers", "GO:0006096"),
        _ => return None,
    })
}

/// A qualifier as the panel shows it: `is`, `isVersionOf`, and, for a model
/// qualifier, `model is`.
pub fn qualifier_text(q: &str) -> String {
    match q.split_once(':') {
        Some(("bqmodel", rest)) => format!("model {rest}"),
        Some((_, rest)) => rest.to_owned(),
        None => q.to_owned(),
    }
}

/// A reference's entry as the panel shows it: `chebi/CHEBI:17234` for
/// `http://identifiers.org/chebi/CHEBI:17234`, `kegg.compound:C00031` for
/// `urn:miriam:kegg.compound:C00031`; anything else as it is.
pub fn resource_text(resource: &str) -> String {
    let r = resource.trim();
    for prefix in [
        "https://identifiers.org/",
        "http://identifiers.org/",
        "urn:miriam:",
    ] {
        if let Some(rest) = r.strip_prefix(prefix) {
            return rest.replace("%3A", ":");
        }
    }
    r.to_owned()
}

/// Where a reference leads, to open in a browser: a web address as it is,
/// a MIRIAM URN through identifiers.org; `None` for anything else.
pub fn resource_url(resource: &str) -> Option<String> {
    let r = resource.trim();
    if r.starts_with("http://") || r.starts_with("https://") {
        return Some(r.to_owned());
    }
    let rest = r.strip_prefix("urn:miriam:")?;
    // `kegg.compound:C00031` → `kegg.compound/C00031`.
    let rest = rest.replace("%3A", ":");
    let (db, id) = rest.split_once(':')?;
    Some(format!("https://identifiers.org/{db}/{id}"))
}

/// A reference as the user typed it: a web address or URN as it is; a
/// compact identifier (`CHEBI:15422`, `uniprot:P12345`) through
/// identifiers.org; `None` for anything else.
pub fn resource_from_typed(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return None;
    }
    if t.contains("://") || t.starts_with("urn:") {
        return Some(t.to_owned());
    }
    let (prefix, id) = t.split_once(':')?;
    (!prefix.is_empty() && !id.is_empty()).then(|| format!("https://identifiers.org/{t}"))
}

/// Notes, which are XHTML, as plain text: a paragraph or line break a line
/// break, tags dropped, the five XML entities decoded, blank lines at the
/// ends trimmed.
pub fn notes_text(xhtml: &str) -> String {
    let mut out = String::new();
    let mut rest = xhtml;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let Some(gt) = rest[lt..].find('>') else {
            rest = "";
            break;
        };
        let tag = &rest[lt + 1..lt + gt];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let local = name.rsplit(':').next().unwrap_or(name);
        if matches!(local, "p" | "br" | "div" | "li" | "h1" | "h2" | "h3" | "tr")
            && (tag.starts_with('/') || local == "br")
        {
            out.push('\n');
        }
        rest = &rest[lt + gt + 1..];
    }
    out.push_str(rest);
    let decoded = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    // Each line's own spacing, as written out of an indented XML file, goes.
    let lines: Vec<&str> = decoded.lines().map(str::trim).collect();
    let mut text = lines.join("\n");
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text.trim().to_owned()
}

/// Plain text as notes: a paragraph per line, in an XHTML body. `None` for
/// empty text.
pub fn notes_xhtml(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let paragraphs: String = text
        .lines()
        .map(|l| format!("<p>{}</p>", esc(l.trim())))
        .collect();
    Some(format!(
        "<body xmlns=\"http://www.w3.org/1999/xhtml\">{paragraphs}</body>"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every model qualifier says what it is for, with an example that can
    /// be typed as it is.
    #[test]
    fn model_qualifiers_have_hints() {
        assert_eq!(
            MODEL_QUALIFIERS[0], "bqmodel:isDescribedBy",
            "the paper first"
        );
        for q in MODEL_QUALIFIERS {
            let (what, example) = model_qualifier_hint(q).unwrap_or_else(|| panic!("{q}"));
            assert!(!what.is_empty());
            assert_eq!(
                resource_from_typed(example),
                Some(format!("https://identifiers.org/{example}"))
            );
        }
        assert_eq!(model_qualifier_hint("bqbiol:is"), None);
    }

    #[test]
    fn sbo_terms_have_names_and_links() {
        assert_eq!(sbo_name(247), Some("simple chemical"));
        assert_eq!(sbo_name(999_999), None);
        assert_eq!(sbo_id(247), "SBO:0000247");
        assert_eq!(sbo_url(27), "https://identifiers.org/SBO:0000027");
        // No number twice, in any list.
        let all: Vec<u32> = ALL_KINDS
            .iter()
            .flat_map(|k| sbo_terms(*k))
            .map(|(n, _)| *n)
            .collect();
        for (i, a) in all.iter().enumerate() {
            assert!(all[i + 1..].iter().all(|b| a != b), "{a}");
        }
        // Each kind its own: a model's are frameworks, not chemicals.
        assert!(
            sbo_terms(SboKind::Model)
                .iter()
                .all(|(_, n)| n.ends_with("framework"))
        );
        assert!(sbo_terms(SboKind::Species).iter().any(|(n, _)| *n == 247));
        assert!(sbo_terms(SboKind::Model).iter().all(|(n, _)| *n != 247));
        assert_eq!(sbo_name(293), Some("non-spatial continuous framework"));
    }

    #[test]
    fn references_are_shown_short_and_link_to_identifiers_org() {
        let chebi = "http://identifiers.org/chebi/CHEBI:17234";
        assert_eq!(resource_text(chebi), "chebi/CHEBI:17234");
        assert_eq!(resource_url(chebi).as_deref(), Some(chebi));
        let urn = "urn:miriam:kegg.compound:C00031";
        assert_eq!(resource_text(urn), "kegg.compound:C00031");
        assert_eq!(
            resource_url(urn).as_deref(),
            Some("https://identifiers.org/kegg.compound/C00031")
        );
        assert_eq!(
            resource_url("urn:miriam:obo.go:GO%3A0005829").as_deref(),
            Some("https://identifiers.org/obo.go/GO:0005829")
        );
        assert_eq!(resource_url("not a link"), None);
        assert_eq!(qualifier_text("bqbiol:isVersionOf"), "isVersionOf");
        assert_eq!(
            qualifier_text("bqmodel:isDescribedBy"),
            "model isDescribedBy"
        );
    }

    #[test]
    fn a_typed_reference_becomes_a_link() {
        assert_eq!(
            resource_from_typed(" CHEBI:15422 ").as_deref(),
            Some("https://identifiers.org/CHEBI:15422")
        );
        assert_eq!(
            resource_from_typed("http://identifiers.org/uniprot/P12345").as_deref(),
            Some("http://identifiers.org/uniprot/P12345")
        );
        assert_eq!(
            resource_from_typed("urn:miriam:chebi:CHEBI:1").as_deref(),
            Some("urn:miriam:chebi:CHEBI:1")
        );
        for bad in ["", "glucose", "CHEBI:", ":15422", "two words:x"] {
            assert_eq!(resource_from_typed(bad), None, "{bad}");
        }
    }

    #[test]
    fn notes_are_shown_as_text_and_text_written_as_notes() {
        let xhtml = "\n  <body xmlns=\"http://www.w3.org/1999/xhtml\">\n    <p>Glucose &amp; ATP.</p>\n    <p>From <i>yeast</i>.</p>\n  </body>\n";
        assert_eq!(notes_text(xhtml), "Glucose & ATP.\n\nFrom yeast.");
        assert_eq!(notes_text("<p>One<br/>Two</p>"), "One\nTwo");
        let back = notes_xhtml("a < b\nsecond").unwrap();
        assert_eq!(
            back,
            "<body xmlns=\"http://www.w3.org/1999/xhtml\"><p>a &lt; b</p><p>second</p></body>"
        );
        assert_eq!(notes_text(&back), "a < b\nsecond");
        assert_eq!(notes_xhtml("  \n "), None);
    }
}
