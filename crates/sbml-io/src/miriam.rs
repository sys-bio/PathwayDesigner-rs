//! MIRIAM annotations: the RDF in an element's `<annotation>` that relates it
//! to database entries (controlled-vocabulary terms, `bqbiol:is` ChEBI 15422)
//! and, on a model, says who made it and when. [`read_annotation`] takes an
//! annotation, as `sbml-io` keeps it (raw XML), apart; [`write_annotation`]
//! writes terms and a history back as RDF, in the form libSBML writes.
//!
//! Anything else in an annotation, such as another tool's own XML, is
//! returned as it is, for the caller to keep or drop.

use crate::read::escape_attr;

/// The RDF namespace.
pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
/// Biology qualifiers (`bqbiol:is`, `bqbiol:isVersionOf`, …).
pub const BQBIOL: &str = "http://biomodels.net/biology-qualifiers/";
/// Model qualifiers (`bqmodel:is`, `bqmodel:isDescribedBy`, …).
pub const BQMODEL: &str = "http://biomodels.net/model-qualifiers/";
pub const DCTERMS: &str = "http://purl.org/dc/terms/";
pub const DC: &str = "http://purl.org/dc/elements/1.1/";
pub const VCARD: &str = "http://www.w3.org/2001/vcard-rdf/3.0#";
pub const VCARD4: &str = "http://www.w3.org/2006/vcard/ns#";

/// A qualifier and the resources (URIs, as written) it relates the element
/// to: `bqbiol:is` with `http://identifiers.org/chebi/CHEBI:15422`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CvTerm {
    /// A model qualifier (`bqmodel:`), else a biology one (`bqbiol:`).
    pub model: bool,
    /// Its local name: `is`, `isVersionOf`, `hasPart`, `isDescribedBy`, …
    pub qualifier: String,
    pub resources: Vec<String>,
}

/// A model's creator, from a vCard.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Creator {
    pub family: Option<String>,
    pub given: Option<String>,
    pub email: Option<String>,
    pub organization: Option<String>,
}

/// Who made a model and when: its creators, and its created and modified
/// dates (W3CDTF, as written).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct History {
    pub creators: Vec<Creator>,
    pub created: Option<String>,
    pub modified: Vec<String>,
}

impl History {
    pub fn is_empty(&self) -> bool {
        self.creators.is_empty() && self.created.is_none() && self.modified.is_empty()
    }
}

/// What [`read_annotation`] found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Annotation {
    pub terms: Vec<CvTerm>,
    pub history: History,
    /// Every other element in the annotation, as raw XML: another tool's own,
    /// or RDF this reader doesn't take apart.
    pub other: Vec<String>,
}

/// Takes an annotation's content apart. `namespaces` are those declared on
/// the document's `<sbml>` element (`Document::namespaces`), which the raw
/// XML may use without declaring. XML that doesn't read is returned whole
/// in `other`.
pub fn read_annotation(xml: &str, namespaces: &[(Option<String>, String)]) -> Annotation {
    let mut decl = String::new();
    for (prefix, uri) in namespaces {
        match prefix {
            Some(p) => decl.push_str(&format!(" xmlns:{p}=\"{}\"", escape_attr(uri))),
            None => decl.push_str(&format!(" xmlns=\"{}\"", escape_attr(uri))),
        }
    }
    let open = format!("<annotation{decl}>");
    let wrapped = format!("{open}{xml}</annotation>");
    let mut out = Annotation::default();
    let Ok(doc) = roxmltree::Document::parse(&wrapped) else {
        if !xml.trim().is_empty() {
            out.other.push(xml.to_owned());
        }
        return out;
    };
    for top in doc.root_element().children().filter(|n| n.is_element()) {
        if is(top, RDF, "RDF") && read_rdf(top, &mut out) {
            continue;
        }
        out.other.push(wrapped[top.range()].to_owned());
    }
    out
}

/// Reads an `rdf:RDF` into `out`; false, with `out` unchanged, if it holds
/// anything this reader doesn't know.
fn read_rdf(rdf: roxmltree::Node, out: &mut Annotation) -> bool {
    let mut found = Annotation::default();
    for d in rdf.children().filter(|n| n.is_element()) {
        if !is(d, RDF, "Description") {
            return false;
        }
        for c in d.children().filter(|n| n.is_element()) {
            let ns = c.tag_name().namespace();
            let name = c.tag_name().name();
            if ns == Some(BQBIOL) || ns == Some(BQMODEL) {
                found.terms.push(CvTerm {
                    model: ns == Some(BQMODEL),
                    qualifier: name.to_owned(),
                    resources: resources(c),
                });
            } else if (ns == Some(DCTERMS) || ns == Some(DC)) && name == "creator" {
                found.history.creators.extend(creators(c));
            } else if ns == Some(DCTERMS) && name == "created" {
                found.history.created = date(c);
            } else if ns == Some(DCTERMS) && name == "modified" {
                found.history.modified.extend(date(c));
            } else {
                return false;
            }
        }
    }
    out.terms.extend(found.terms);
    out.history.creators.extend(found.history.creators);
    if found.history.created.is_some() {
        out.history.created = found.history.created;
    }
    out.history.modified.extend(found.history.modified);
    true
}

fn is(n: roxmltree::Node, ns: &str, name: &str) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(ns) && n.tag_name().name() == name
}

/// The `rdf:resource` of every `rdf:li` in a qualifier (in a Bag, Seq or
/// Alt), in order.
fn resources(q: roxmltree::Node) -> Vec<String> {
    q.descendants()
        .filter(|n| is(*n, RDF, "li"))
        .filter_map(|n| n.attribute((RDF, "resource")).map(str::to_owned))
        .collect()
}

/// A date, as W3CDTF: `<dcterms:W3CDTF>2005-02-02T14:56:11Z</dcterms:W3CDTF>`.
fn date(n: roxmltree::Node) -> Option<String> {
    n.descendants()
        .find(|d| is(*d, DCTERMS, "W3CDTF"))
        .or(Some(n))
        .and_then(|d| d.text())
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}

/// The creators in a `dcterms:creator`, each an `rdf:li` with a vCard (3.0
/// or 4).
fn creators(c: roxmltree::Node) -> Vec<Creator> {
    c.descendants()
        .filter(|n| is(*n, RDF, "li"))
        .map(|li| {
            let text = |names: &[&str]| {
                li.descendants()
                    .find(|d| {
                        d.is_element()
                            && matches!(d.tag_name().namespace(), Some(VCARD) | Some(VCARD4))
                            && names.contains(&d.tag_name().name())
                    })
                    .and_then(|d| d.text())
                    .map(|t| t.trim().to_owned())
                    .filter(|t| !t.is_empty())
            };
            Creator {
                family: text(&["Family", "family-name"]),
                given: text(&["Given", "given-name"]),
                email: text(&["EMAIL", "hasEmail"]),
                organization: text(&["Orgname", "organization-name"]),
            }
        })
        .filter(|c| *c != Creator::default())
        .collect()
}

/// The RDF for `terms` and `history` on the element whose metaid is
/// `metaid`: creators, dates, then terms, as libSBML writes them; `None`
/// when there is nothing to write.
pub fn write_annotation(metaid: &str, terms: &[CvTerm], history: &History) -> Option<String> {
    if terms.iter().all(|t| t.resources.is_empty()) && history.is_empty() {
        return None;
    }
    let esc = escape_text;
    let mut out = format!(
        "<rdf:RDF xmlns:rdf=\"{RDF}\" xmlns:dcterms=\"{DCTERMS}\" xmlns:vCard=\"{VCARD}\" \
         xmlns:bqbiol=\"{BQBIOL}\" xmlns:bqmodel=\"{BQMODEL}\">\n  <rdf:Description \
         rdf:about=\"#{}\">\n",
        escape_attr(metaid)
    );
    if !history.creators.is_empty() {
        out.push_str("    <dcterms:creator>\n      <rdf:Bag>\n");
        for c in &history.creators {
            out.push_str("        <rdf:li rdf:parseType=\"Resource\">\n");
            if c.family.is_some() || c.given.is_some() {
                out.push_str("          <vCard:N rdf:parseType=\"Resource\">\n");
                if let Some(f) = &c.family {
                    out.push_str(&format!(
                        "            <vCard:Family>{}</vCard:Family>\n",
                        esc(f)
                    ));
                }
                if let Some(g) = &c.given {
                    out.push_str(&format!(
                        "            <vCard:Given>{}</vCard:Given>\n",
                        esc(g)
                    ));
                }
                out.push_str("          </vCard:N>\n");
            }
            if let Some(e) = &c.email {
                out.push_str(&format!(
                    "          <vCard:EMAIL>{}</vCard:EMAIL>\n",
                    esc(e)
                ));
            }
            if let Some(o) = &c.organization {
                out.push_str(&format!(
                    "          <vCard:ORG rdf:parseType=\"Resource\">\n            \
                     <vCard:Orgname>{}</vCard:Orgname>\n          </vCard:ORG>\n",
                    esc(o)
                ));
            }
            out.push_str("        </rdf:li>\n");
        }
        out.push_str("      </rdf:Bag>\n    </dcterms:creator>\n");
    }
    let mut date = |tag: &str, d: &str| {
        out.push_str(&format!(
            "    <dcterms:{tag} rdf:parseType=\"Resource\">\n      \
             <dcterms:W3CDTF>{}</dcterms:W3CDTF>\n    </dcterms:{tag}>\n",
            esc(d)
        ));
    };
    if let Some(d) = &history.created {
        date("created", d);
    }
    for d in &history.modified {
        date("modified", d);
    }
    for t in terms.iter().filter(|t| !t.resources.is_empty()) {
        let prefix = if t.model { "bqmodel" } else { "bqbiol" };
        out.push_str(&format!(
            "    <{prefix}:{}>\n      <rdf:Bag>\n",
            t.qualifier
        ));
        for r in &t.resources {
            out.push_str(&format!(
                "        <rdf:li rdf:resource=\"{}\"/>\n",
                escape_attr(r)
            ));
        }
        out.push_str(&format!(
            "      </rdf:Bag>\n    </{prefix}:{}>\n",
            t.qualifier
        ));
    }
    out.push_str("  </rdf:Description>\n</rdf:RDF>");
    Some(out)
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPECIES: &str = r##"
      <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:bqbiol="http://biomodels.net/biology-qualifiers/">
        <rdf:Description rdf:about="#S">
          <bqbiol:is>
            <rdf:Bag>
              <rdf:li rdf:resource="http://identifiers.org/chebi/CHEBI:17234"/>
              <rdf:li rdf:resource="urn:miriam:kegg.compound:C00031"/>
            </rdf:Bag>
          </bqbiol:is>
          <bqbiol:isPartOf><rdf:Bag><rdf:li rdf:resource="http://identifiers.org/go/GO:0005829"/></rdf:Bag></bqbiol:isPartOf>
        </rdf:Description>
      </rdf:RDF>
      <COPASI xmlns="http://www.copasi.org/static/sbml"><x/></COPASI>"##;

    #[test]
    fn terms_are_read_and_another_tools_xml_returned() {
        let a = read_annotation(SPECIES, &[]);
        assert_eq!(a.terms.len(), 2);
        assert_eq!(a.terms[0].qualifier, "is");
        assert!(!a.terms[0].model);
        assert_eq!(
            a.terms[0].resources,
            [
                "http://identifiers.org/chebi/CHEBI:17234",
                "urn:miriam:kegg.compound:C00031"
            ]
        );
        assert_eq!(a.terms[1].qualifier, "isPartOf");
        assert_eq!(a.other.len(), 1);
        assert!(a.other[0].starts_with("<COPASI"), "{:?}", a.other);
    }

    #[test]
    fn prefixes_declared_on_the_sbml_element_are_used() {
        let xml = r##"<rdf:RDF><rdf:Description rdf:about="#S"><bqbiol:is><rdf:Bag><rdf:li rdf:resource="http://identifiers.org/chebi/CHEBI:1"/></rdf:Bag></bqbiol:is></rdf:Description></rdf:RDF>"##;
        let ns = [
            (Some("rdf".to_owned()), RDF.to_owned()),
            (Some("bqbiol".to_owned()), BQBIOL.to_owned()),
        ];
        assert_eq!(read_annotation(xml, &ns).terms.len(), 1);
        // Without them it doesn't read, and comes back whole.
        let a = read_annotation(xml, &[]);
        assert!(a.terms.is_empty());
        assert_eq!(a.other, [xml]);
    }

    #[test]
    fn a_model_history_is_read_and_written_back_the_same() {
        let terms = vec![CvTerm {
            model: true,
            qualifier: "isDescribedBy".into(),
            resources: vec!["http://identifiers.org/pubmed/10659856".into()],
        }];
        let history = History {
            creators: vec![Creator {
                family: Some("Elowitz".into()),
                given: Some("Michael".into()),
                email: Some("m@example.org".into()),
                organization: Some("Caltech & co".into()),
            }],
            created: Some("2005-02-02T14:56:11Z".into()),
            modified: vec!["2012-07-05T16:44:43Z".into()],
        };
        let xml = write_annotation("model1", &terms, &history).unwrap();
        let back = read_annotation(&xml, &[]);
        assert_eq!(back.terms, terms);
        assert_eq!(back.history, history);
        assert!(back.other.is_empty());
        assert_eq!(write_annotation("x", &[], &History::default()), None);
    }

    #[test]
    fn rdf_with_something_unknown_is_returned_whole() {
        let xml = r##"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:x="urn:x"><rdf:Description rdf:about="#S"><x:y>1</x:y></rdf:Description></rdf:RDF>"##;
        let a = read_annotation(xml, &[]);
        assert!(a.terms.is_empty());
        assert_eq!(a.other.len(), 1);
    }
}
