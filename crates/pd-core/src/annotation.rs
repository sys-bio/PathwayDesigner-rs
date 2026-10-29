//! What the panels need to show and edit annotations
//! (docs/annotations-design.md, step 4): names for common SBO terms, links
//! for references, the qualifiers to choose from, and notes as text.

/// Common SBO terms with their names (A5): enough to name what most
/// models use; any other number can be typed, and is shown as its number.
pub const SBO_TERMS: &[(u32, &str)] = &[
    // Species.
    (247, "simple chemical"),
    (245, "macromolecule"),
    (252, "polypeptide chain"),
    (250, "ribonucleic acid"),
    (251, "deoxyribonucleic acid"),
    (253, "non-covalent complex"),
    // Reactions.
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
    // Compartments.
    (290, "physical compartment"),
    // Parameters.
    (2, "quantitative systems description parameter"),
    (9, "kinetic constant"),
    (25, "catalytic rate constant"),
    (27, "Michaelis constant"),
    (186, "maximal velocity"),
    (193, "equilibrium or steady-state constant"),
    (356, "decay constant"),
    // Models.
    (62, "continuous framework"),
];

/// The name of SBO term `n`, if it is one of [`SBO_TERMS`].
pub fn sbo_name(n: u32) -> Option<&'static str> {
    SBO_TERMS
        .iter()
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

/// The qualifiers for the model's references: the model ones first.
pub const MODEL_QUALIFIERS: &[&str] = &[
    "bqmodel:is",
    "bqmodel:isDescribedBy",
    "bqmodel:isDerivedFrom",
    "bqbiol:isVersionOf",
    "bqbiol:hasTaxon",
    "bqbiol:hasProperty",
    "bqbiol:isHomologTo",
    "bqbiol:hasVersion",
];

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

    #[test]
    fn sbo_terms_have_names_and_links() {
        assert_eq!(sbo_name(247), Some("simple chemical"));
        assert_eq!(sbo_name(999_999), None);
        assert_eq!(sbo_id(247), "SBO:0000247");
        assert_eq!(sbo_url(27), "https://identifiers.org/SBO:0000027");
        // No number twice.
        for (i, (a, _)) in SBO_TERMS.iter().enumerate() {
            assert!(SBO_TERMS[i + 1..].iter().all(|(b, _)| a != b), "{a}");
        }
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
