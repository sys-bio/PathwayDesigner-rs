//! Genes and gene–reaction rules (docs/gene-rules-design.md): a reaction's
//! rule says which genes' products catalyse it, `b3916 or b1723` (either
//! one) or `b0114 and b0115` (a complex of both). SBML says it with the fbc
//! package (version 2): the model's gene products, and on each reaction a
//! gene product association.
//!
//! The model holds the genes ([`Model::genes`]) and each reaction's rule as
//! text, gene ids joined by `and` and `or` ([`ReactionSbml::gene_rule`]).
//! The fbc XML a file gave stays among the other packages' raw XML: SBML
//! import reads the genes and rules out of it ([`from_packages`]), and export
//! puts them back ([`to_packages`]), writing the XML as read wherever the
//! genes or rule are still what it says, so its annotations and SBO terms
//! come back exactly.

use crate::model::{Gene, Model, PackageAttribute};

/// fbc version 2's namespace.
pub const FBC: &str = "http://www.sbml.org/sbml/level3/version1/fbc/version2";

/// A rule as a tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    Gene(String),
    And(Vec<Rule>),
    Or(Vec<Rule>),
}

// ----- Text -----------------------------------------------------------------------

/// A rule read from text: gene ids joined by `and` and `or` (in any case),
/// with brackets; `and` binds tighter than `or`.
pub fn parse(text: &str) -> Result<Rule, String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        if c == '(' || c == ')' || c.is_whitespace() {
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
            if !c.is_whitespace() {
                tokens.push(c.to_string());
            }
        } else if c.is_alphanumeric() || "_.:-".contains(c) {
            word.push(c);
        } else {
            return Err(format!("A gene rule can't have \u{201c}{c}\u{201d}"));
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    if tokens.is_empty() {
        return Err("Write the genes, joined by and and or".into());
    }
    let mut p = Parser { tokens, at: 0 };
    let rule = p.or()?;
    match p.tokens.get(p.at) {
        None => Ok(rule),
        Some(t) => Err(format!("Not expected here: {t}")),
    }
}

struct Parser {
    tokens: Vec<String>,
    at: usize,
}

impl Parser {
    fn peek_is(&self, word: &str) -> bool {
        self.tokens
            .get(self.at)
            .is_some_and(|t| t.eq_ignore_ascii_case(word))
    }

    fn or(&mut self) -> Result<Rule, String> {
        let mut parts = vec![self.and()?];
        while self.peek_is("or") {
            self.at += 1;
            parts.push(self.and()?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Rule::Or(parts)
        })
    }

    fn and(&mut self) -> Result<Rule, String> {
        let mut parts = vec![self.atom()?];
        while self.peek_is("and") {
            self.at += 1;
            parts.push(self.atom()?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Rule::And(parts)
        })
    }

    fn atom(&mut self) -> Result<Rule, String> {
        let Some(t) = self.tokens.get(self.at).cloned() else {
            return Err("A gene is missing at the end".into());
        };
        self.at += 1;
        if t == "(" {
            let inner = self.or()?;
            if self.tokens.get(self.at).map(String::as_str) != Some(")") {
                return Err("A bracket isn't closed".into());
            }
            self.at += 1;
            return Ok(inner);
        }
        if t == ")" || t.eq_ignore_ascii_case("and") || t.eq_ignore_ascii_case("or") {
            return Err(format!("A gene is missing before {t}"));
        }
        Ok(Rule::Gene(t))
    }
}

/// A rule as text, brackets only where needed but always around a group
/// inside another (`(b0978 and b0979) or b0733`), as COBRA models write it.
pub fn text(rule: &Rule) -> String {
    fn part(r: &Rule) -> String {
        match r {
            Rule::Gene(g) => g.clone(),
            _ => format!("({})", text(r)),
        }
    }
    match rule {
        Rule::Gene(g) => g.clone(),
        Rule::And(parts) => parts.iter().map(part).collect::<Vec<_>>().join(" and "),
        Rule::Or(parts) => parts.iter().map(part).collect::<Vec<_>>().join(" or "),
    }
}

/// The genes a rule names, each once, in order.
pub fn genes_of(rule: &Rule) -> Vec<String> {
    fn walk(r: &Rule, out: &mut Vec<String>) {
        match r {
            Rule::Gene(g) => {
                if !out.contains(g) {
                    out.push(g.clone());
                }
            }
            Rule::And(parts) | Rule::Or(parts) => parts.iter().for_each(|p| walk(p, out)),
        }
    }
    let mut out = Vec::new();
    walk(rule, &mut out);
    out
}

/// `rule` with each gene named as `name` gives it.
pub fn renamed(rule: &Rule, name: &dyn Fn(&str) -> String) -> Rule {
    match rule {
        Rule::Gene(g) => Rule::Gene(name(g)),
        Rule::And(parts) => Rule::And(parts.iter().map(|p| renamed(p, name)).collect()),
        Rule::Or(parts) => Rule::Or(parts.iter().map(|p| renamed(p, name)).collect()),
    }
}

/// A gene's id from its label, as BiGG's SBML writes them: `G_` before it,
/// and anything an SBML id can't hold as `_` (`b3916` → `G_b3916`).
pub fn gene_id(label: &str) -> String {
    let clean: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("G_{clean}")
}

// ----- Shown and typed ------------------------------------------------------------

/// A rule as a person reads it: each gene by its name, or its label if it
/// has none (`pfkA or pfkB`).
pub fn shown(model: &Model, rule: &str) -> String {
    by(model, rule, &|g: &Gene| {
        g.name.clone().unwrap_or_else(|| g.label.clone())
    })
}

/// A rule as it is typed: each gene by its label (`b3916 or b1723`).
pub fn editable(model: &Model, rule: &str) -> String {
    by(model, rule, &|g: &Gene| g.label.clone())
}

fn by(model: &Model, rule: &str, word: &dyn Fn(&Gene) -> String) -> String {
    match parse(rule) {
        Ok(r) => text(&renamed(&r, &|id| {
            model
                .genes
                .iter()
                .find(|g| g.id == id)
                .map_or_else(|| id.to_owned(), word)
        })),
        Err(_) => rule.to_owned(),
    }
}

/// The gene a typed word means: one with that id, else the one with that
/// label.
fn typed_gene<'a>(model: &'a Model, word: &str) -> Option<&'a Gene> {
    model
        .genes
        .iter()
        .find(|g| g.id == word)
        .or_else(|| model.genes.iter().find(|g| g.label == word))
}

/// What is wrong with a typed rule, if anything (an empty one takes the rule
/// away), and the genes it names that the model doesn't have yet.
pub fn check_typed(model: &Model, typed: &str) -> Result<Vec<String>, String> {
    if typed.trim().is_empty() {
        return Ok(Vec::new());
    }
    let rule = parse(typed)?;
    Ok(genes_of(&rule)
        .into_iter()
        .filter(|g| typed_gene(model, g).is_none())
        .collect())
}

/// Gives `reaction` the rule typed, in gene ids, adding a gene for each
/// word that names none, with that word as its label (an empty rule takes
/// the rule away). The model is unchanged if the rule doesn't read.
pub fn set_typed(model: &mut Model, reaction: &str, typed: &str) -> Result<(), String> {
    if model.find_reaction(reaction).is_none() {
        return Err(format!("No reaction {reaction}"));
    }
    let rule = if typed.trim().is_empty() {
        None
    } else {
        let parsed = parse(typed)?;
        for word in genes_of(&parsed) {
            if typed_gene(model, &word).is_none() {
                let mut id = gene_id(&word);
                while model.genes.iter().any(|g| g.id == id) {
                    id.push('_');
                }
                model.genes.push(Gene {
                    id,
                    label: word,
                    name: None,
                    ..Default::default()
                });
            }
        }
        let ids = renamed(&parsed, &|w| {
            typed_gene(model, w).map_or_else(|| w.to_owned(), |g| g.id.clone())
        });
        Some(text(&ids))
    };
    model
        .find_reaction_mut(reaction)
        .expect("checked")
        .sbml
        .gene_rule = rule;
    Ok(())
}

// ----- The organism ---------------------------------------------------------------

/// The NCBI Taxonomy number in a taxonomy reference, as files write it
/// (`https://identifiers.org/taxonomy:83333`, `…/taxonomy/83333`,
/// `urn:miriam:taxonomy:83333`).
fn taxon_of(resource: &str) -> Option<String> {
    let at = resource.find("taxonomy")? + "taxonomy".len();
    let n: String = resource[at..]
        .trim_start_matches([':', '/'])
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!n.is_empty()).then_some(n)
}

/// The model's organism, as SBML holds it: its `bqbiol:hasTaxon` reference
/// to NCBI Taxonomy, as a number (`83333`) (docs/uniprot-genes-design.md U2).
pub fn organism(model: &Model) -> Option<String> {
    model
        .annotation
        .references
        .iter()
        .filter(|r| r.qualifier == "bqbiol:hasTaxon")
        .find_map(|r| taxon_of(&r.resource))
}

/// Sets the model's organism to NCBI Taxonomy's `taxon`, in place of any
/// other.
pub fn set_organism(model: &mut Model, taxon: &str) {
    let refs = &mut model.annotation.references;
    refs.retain(|r| r.qualifier != "bqbiol:hasTaxon" || taxon_of(&r.resource).is_none());
    refs.push(crate::model::Reference {
        qualifier: "bqbiol:hasTaxon".to_owned(),
        resource: format!("https://identifiers.org/taxonomy:{taxon}"),
    });
}

// ----- fbc XML --------------------------------------------------------------------

/// A raw element's local name, from its first tag.
fn local_name(xml: &str) -> Option<&str> {
    let start = xml.trim_start().strip_prefix('<')?;
    let tag = start
        .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .next()?;
    Some(tag.rsplit(':').next().unwrap_or(tag))
}

/// `xml` parsed inside a wrapper that declares the document's namespaces, as
/// raw elements leave them to the `<sbml>` element.
fn with_doc<T>(
    xml: &str,
    namespaces: &[(Option<String>, String)],
    f: impl FnOnce(roxmltree::Node, usize) -> Option<T>,
) -> Option<T> {
    let mut open = String::from("<w");
    for (prefix, uri) in namespaces {
        match prefix {
            Some(p) => open.push_str(&format!(" xmlns:{p}=\"{uri}\"")),
            None => open.push_str(&format!(" xmlns=\"{uri}\"")),
        }
    }
    if !namespaces.iter().any(|(_, u)| u == FBC) {
        open.push_str(&format!(" xmlns:fbc=\"{FBC}\""));
    }
    open.push('>');
    let text = format!("{open}{xml}</w>");
    let doc = roxmltree::Document::parse(&text).ok()?;
    let node = doc.root_element().children().find(|n| n.is_element())?;
    f(node, open.len())
}

fn fbc_attr<'a>(n: &roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    n.attribute((FBC, name))
}

/// The genes in a raw `listOfGeneProducts`, if it holds nothing else (no
/// attributes or annotation of its own, nothing but gene products): else
/// `None`, and it stays as it is.
pub fn genes_from_fbc(xml: &str, namespaces: &[(Option<String>, String)]) -> Option<Vec<Gene>> {
    with_doc(xml, namespaces, |list, _| {
        if list.tag_name().namespace() != Some(FBC) || list.attributes().len() > 0 {
            return None;
        }
        let mut genes = Vec::new();
        for n in list.children() {
            if n.is_text() && n.text().is_some_and(|t| t.trim().is_empty()) || n.is_comment() {
                continue;
            }
            if !n.is_element() || n.tag_name() != (FBC, "geneProduct").into() {
                return None;
            }
            let id = fbc_attr(&n, "id")?.to_owned();
            genes.push(Gene {
                label: fbc_attr(&n, "label").unwrap_or(&id).to_owned(),
                name: fbc_attr(&n, "name").map(str::to_owned),
                annotation: gene_annotation(&n, namespaces),
                id,
            });
        }
        Some(genes)
    })
}

/// The inner XML of `n`, as written.
fn inner(n: &roxmltree::Node) -> String {
    let text = n.document().input_text();
    match (n.first_child(), n.last_child()) {
        (Some(a), Some(b)) => text[a.range().start..b.range().end].to_owned(),
        _ => String::new(),
    }
}

/// A gene product's annotation: its metaid, SBO term, notes and annotation,
/// read as any SBML element's.
fn gene_annotation(
    n: &roxmltree::Node,
    namespaces: &[(Option<String>, String)],
) -> crate::model::Annotation {
    let child = |name: &str| {
        n.children()
            .find(|c| c.is_element() && c.tag_name().name() == name)
            .map(|c| inner(&c))
    };
    let meta = sbml_io::Meta {
        metaid: n.attribute("metaid").map(str::to_owned),
        sbo_term: n.attribute("sboTerm").map(str::to_owned),
        notes: child("notes"),
        annotation: child("annotation"),
        extra: Default::default(),
    };
    crate::sbml::import::annotation_of(&meta, namespaces)
}

/// A gene product as fbc XML, `p` the fbc prefix.
fn gene_product_xml(g: &Gene, p: &str) -> String {
    let a = &g.annotation;
    let mut attrs = String::new();
    let has_rdf = !a.references.is_empty()
        || !a.history.is_empty()
        || !a.rdf.is_empty()
        || !a.other.is_empty();
    let metaid = a
        .metaid
        .clone()
        .or_else(|| has_rdf.then(|| format!("meta_{}", g.id)));
    if let Some(m) = &metaid {
        attrs.push_str(&format!(" metaid=\"{}\"", escape(m)));
    }
    if let Some(sbo) = a.sbo {
        attrs.push_str(&format!(" sboTerm=\"SBO:{sbo:07}\""));
    }
    attrs.push_str(&format!(
        " {p}:id=\"{}\" {p}:label=\"{}\"",
        escape(&g.id),
        escape(&g.label)
    ));
    if let Some(n) = &g.name {
        attrs.push_str(&format!(" {p}:name=\"{}\"", escape(n)));
    }
    let mut body = String::new();
    if let Some(notes) = &a.notes {
        body.push_str(&format!("<notes>{notes}</notes>"));
    }
    if let Some(inner) = metaid
        .as_deref()
        .and_then(|m| crate::sbml::export::annotation_inner(m, a, &a.history))
    {
        body.push_str(&format!("<annotation>{inner}</annotation>"));
    }
    if body.is_empty() {
        format!("<{p}:geneProduct{attrs}/>")
    } else {
        format!("<{p}:geneProduct{attrs}>{body}</{p}:geneProduct>")
    }
}

/// Each gene product's own XML in a raw `listOfGeneProducts`, by id.
fn gene_xml(xml: &str, namespaces: &[(Option<String>, String)]) -> Vec<(String, String)> {
    with_doc(xml, namespaces, |list, offset| {
        Some(
            list.children()
                .filter(|n| n.is_element())
                .filter_map(|n| {
                    let id = fbc_attr(&n, "id")?.to_owned();
                    let r = n.range();
                    Some((id, xml[r.start - offset..r.end - offset].to_owned()))
                })
                .collect(),
        )
    })
    .unwrap_or_default()
}

/// The rule in a raw `geneProductAssociation`, as text.
pub fn rule_from_fbc(xml: &str, namespaces: &[(Option<String>, String)]) -> Option<String> {
    fn rule(n: roxmltree::Node) -> Option<Rule> {
        if n.tag_name().namespace() != Some(FBC) {
            return None;
        }
        let parts =
            || -> Option<Vec<Rule>> { n.children().filter(|c| c.is_element()).map(rule).collect() };
        match n.tag_name().name() {
            "geneProductRef" => Some(Rule::Gene(fbc_attr(&n, "geneProduct")?.to_owned())),
            "and" => Some(Rule::And(parts()?)),
            "or" => Some(Rule::Or(parts()?)),
            _ => None,
        }
    }
    with_doc(xml, namespaces, |gpa, _| {
        if gpa.tag_name() != (FBC, "geneProductAssociation").into() {
            return None;
        }
        let inner = gpa.children().find(|c| {
            c.is_element() && c.tag_name().name() != "annotation" && c.tag_name().name() != "notes"
        })?;
        Some(text(&rule(inner)?))
    })
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A rule as fbc XML, with `p` the fbc prefix.
fn rule_xml(rule: &Rule, p: &str) -> String {
    match rule {
        Rule::Gene(g) => format!("<{p}:geneProductRef {p}:geneProduct=\"{}\"/>", escape(g)),
        Rule::And(parts) => format!(
            "<{p}:and>{}</{p}:and>",
            parts.iter().map(|r| rule_xml(r, p)).collect::<String>()
        ),
        Rule::Or(parts) => format!(
            "<{p}:or>{}</{p}:or>",
            parts.iter().map(|r| rule_xml(r, p)).collect::<String>()
        ),
    }
}

// ----- To and from the model ------------------------------------------------------

/// The prefix the model's file gives fbc version 2, or `fbc`.
fn prefix(model: &Model) -> String {
    model
        .sbml_namespaces
        .iter()
        .find(|(_, u)| u == FBC)
        .and_then(|(p, _)| p.clone())
        .unwrap_or_else(|| "fbc".to_owned())
}

/// The genes the model's fbc XML says, if it has a list of only genes.
pub fn genes_as_read(model: &Model) -> Option<Vec<Gene>> {
    model
        .annotation
        .package
        .elements
        .iter()
        .filter(|x| local_name(x) == Some("listOfGeneProducts"))
        .find_map(|x| genes_from_fbc(x, &model.sbml_namespaces))
}

/// The rule a reaction's fbc XML says, if any.
pub fn rule_as_read(r: &crate::model::Reaction, ns: &[(Option<String>, String)]) -> Option<String> {
    r.annotation
        .package
        .elements
        .iter()
        .filter(|x| local_name(x) == Some("geneProductAssociation"))
        .find_map(|x| rule_from_fbc(x, ns))
}

/// After an SBML import, and wherever a model comes with fbc XML but not
/// the genes read out of it (a native file older than version 24, Antimony's
/// `sbml` block): the genes and rules out of the fbc XML kept with the model
/// and its reactions.
pub fn from_packages(model: &mut Model) {
    let ns = model.sbml_namespaces.clone();
    if let Some(genes) = model
        .annotation
        .package
        .elements
        .iter()
        .filter(|x| local_name(x) == Some("listOfGeneProducts"))
        .find_map(|x| genes_from_fbc(x, &ns))
    {
        model.genes = genes;
    }
    for r in model.reactions_mut() {
        if let Some(rule) = r
            .annotation
            .package
            .elements
            .iter()
            .filter(|x| local_name(x) == Some("geneProductAssociation"))
            .find_map(|x| rule_from_fbc(x, &ns))
        {
            r.sbml.gene_rule = Some(rule);
        }
    }
}

/// Before an SBML export: the model's genes and rules into the fbc XML kept
/// with it, as read where they still say the same, new where they don't.
pub fn to_packages(model: &mut Model) {
    let ns = model.sbml_namespaces.clone();
    let p = prefix(model);
    let mut used = false;

    // The genes.
    let at = model
        .annotation
        .package
        .elements
        .iter()
        .position(|x| local_name(x) == Some("listOfGeneProducts"));
    let read = at.and_then(|i| genes_from_fbc(&model.annotation.package.elements[i], &ns));
    // A list that holds more than genes is left as it is.
    let leave = at.is_some() && read.is_none();
    if !leave && read.as_ref() != Some(&model.genes) {
        let old = at
            .map(|i| gene_xml(&model.annotation.package.elements[i], &ns))
            .unwrap_or_default();
        let as_read = |g: &Gene| {
            old.iter()
                .find(|(id, _)| *id == g.id)
                .map(|(_, x)| x.clone())
                .filter(|x| {
                    genes_from_fbc(
                        &format!("<{p}:listOfGeneProducts>{x}</{p}:listOfGeneProducts>"),
                        &ns,
                    )
                    .is_some_and(|v| v.len() == 1 && v[0] == *g)
                })
        };
        let xml = if model.genes.is_empty() {
            None
        } else {
            let items: String = model
                .genes
                .iter()
                .map(|g| as_read(g).unwrap_or_else(|| gene_product_xml(g, &p)))
                .collect();
            Some(format!(
                "<{p}:listOfGeneProducts>{items}</{p}:listOfGeneProducts>"
            ))
        };
        let elements = &mut model.annotation.package.elements;
        match (at, xml) {
            (Some(i), Some(x)) => elements[i] = x,
            (Some(i), None) => {
                elements.remove(i);
            }
            (None, Some(x)) => {
                elements.push(x);
                used = true;
            }
            (None, None) => {}
        }
    }

    // Each reaction's rule.
    for r in model.reactions_mut() {
        let elements = &mut r.annotation.package.elements;
        let at = elements
            .iter()
            .position(|x| local_name(x) == Some("geneProductAssociation"));
        let read = at.and_then(|i| rule_from_fbc(&elements[i], &ns));
        if at.is_some() && read.is_none() {
            continue;
        }
        if read == r.sbml.gene_rule {
            continue;
        }
        let xml = r
            .sbml
            .gene_rule
            .as_deref()
            .and_then(|t| parse(t).ok())
            .map(|rule| {
                format!(
                    "<{p}:geneProductAssociation>{}</{p}:geneProductAssociation>",
                    rule_xml(&rule, &p)
                )
            });
        match (at, xml) {
            (Some(i), Some(x)) => elements[i] = x,
            (Some(i), None) => {
                elements.remove(i);
            }
            (None, Some(x)) => {
                elements.push(x);
                used = true;
            }
            (None, None) => {}
        }
    }

    // fbc declared, where it wasn't: not required to read the model, and,
    // as fbc version 2 asks, not strict.
    if used && !model.sbml_namespaces.iter().any(|(_, u)| u == FBC) {
        model
            .sbml_namespaces
            .push((Some(p.clone()), FBC.to_owned()));
    }
    if used {
        let has = |list: &[PackageAttribute], name: &str| {
            list.iter()
                .any(|a| a.namespace.as_deref() == Some(FBC) && a.name.ends_with(name))
        };
        if !has(&model.sbml_document.attributes, ":required") {
            model.sbml_document.attributes.push(PackageAttribute {
                name: format!("{p}:required"),
                namespace: Some(FBC.to_owned()),
                value: "false".to_owned(),
            });
        }
        if !has(&model.annotation.package.attributes, ":strict") {
            model.annotation.package.attributes.push(PackageAttribute {
                name: format!("{p}:strict"),
                namespace: Some(FBC.to_owned()),
                value: "false".to_owned(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_read_and_print() {
        for (input, out) in [
            ("b2926", "b2926"),
            ("b3916 or b1723", "b3916 or b1723"),
            ("b0114 AND b0115 and b0116", "b0114 and b0115 and b0116"),
            (
                "(b0978 and b0979) or (b0733 and b0734)",
                "(b0978 and b0979) or (b0733 and b0734)",
            ),
            ("b0978 and b0979 or b0733", "(b0978 and b0979) or b0733"),
            ("a and (b or c)", "a and (b or c)"),
            ("((a))", "a"),
        ] {
            assert_eq!(text(&parse(input).unwrap()), out, "{input}");
        }
        for bad in ["", "a and", "and a", "(a or b", "a b", "a + b", "a or )"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert_eq!(
            genes_of(&parse("(a and b) or (b and c)").unwrap()),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn typed_by_label_shown_by_name() {
        let mut m = Model::new();
        m.push_reaction(crate::model::Reaction::new(
            "PFK",
            crate::geometry::Pt::ZERO,
        ))
        .unwrap();
        m.genes.push(Gene {
            id: "G_b3916".into(),
            label: "b3916".into(),
            name: Some("pfkA".into()),
            ..Default::default()
        });
        assert_eq!(check_typed(&m, "b3916 or b1723").unwrap(), ["b1723"]);
        assert!(check_typed(&m, "b3916 or").is_err());
        set_typed(&mut m, "PFK", "b3916 or b1723").unwrap();
        let rule = m
            .find_reaction("PFK")
            .unwrap()
            .sbml
            .gene_rule
            .clone()
            .unwrap();
        assert_eq!(rule, "G_b3916 or G_b1723");
        assert_eq!(m.genes[1].label, "b1723");
        assert_eq!(editable(&m, &rule), "b3916 or b1723");
        assert_eq!(shown(&m, &rule), "pfkA or b1723");
        // Ids work too, and nothing is added for them.
        set_typed(&mut m, "PFK", "G_b1723").unwrap();
        assert_eq!(m.genes.len(), 2);
        assert!(set_typed(&mut m, "PFK", "(b1723").is_err());
        assert_eq!(
            m.find_reaction("PFK").unwrap().sbml.gene_rule.as_deref(),
            Some("G_b1723")
        );
        set_typed(&mut m, "PFK", " ").unwrap();
        assert_eq!(m.find_reaction("PFK").unwrap().sbml.gene_rule, None);
    }

    #[test]
    fn fbc_xml_both_ways() {
        let ns = vec![(Some("fbc".to_owned()), FBC.to_owned())];
        let gpa = "<fbc:geneProductAssociation><fbc:or><fbc:and><fbc:geneProductRef fbc:geneProduct=\"G_a\"/><fbc:geneProductRef fbc:geneProduct=\"G_b\"/></fbc:and><fbc:geneProductRef fbc:geneProduct=\"G_c\"/></fbc:or></fbc:geneProductAssociation>";
        assert_eq!(
            rule_from_fbc(gpa, &ns).as_deref(),
            Some("(G_a and G_b) or G_c")
        );
        let rule = parse("(G_a and G_b) or G_c").unwrap();
        assert_eq!(
            format!(
                "<fbc:geneProductAssociation>{}</fbc:geneProductAssociation>",
                rule_xml(&rule, "fbc")
            ),
            gpa
        );
        let list = "<fbc:listOfGeneProducts>\n  <fbc:geneProduct fbc:id=\"G_a\" fbc:label=\"a\" fbc:name=\"pfkA\" sboTerm=\"SBO:0000243\"/>\n</fbc:listOfGeneProducts>";
        assert_eq!(
            genes_from_fbc(list, &ns),
            Some(vec![Gene {
                id: "G_a".into(),
                label: "a".into(),
                name: Some("pfkA".into()),
                annotation: crate::model::Annotation {
                    sbo: Some(243),
                    ..Default::default()
                },
            }])
        );
        // A list with more than genes is left alone.
        let more = "<fbc:listOfGeneProducts metaid=\"m\"><fbc:geneProduct fbc:id=\"G_a\" fbc:label=\"a\"/></fbc:listOfGeneProducts>";
        assert_eq!(genes_from_fbc(more, &ns), None);
    }

    #[test]
    fn written_as_read_unless_changed() {
        let mut m = Model::new();
        m.sbml_namespaces = vec![(Some("fbc".to_owned()), FBC.to_owned())];
        let list = "<fbc:listOfGeneProducts><fbc:geneProduct fbc:id=\"G_a\" fbc:label=\"a\" metaid=\"x\" sboTerm=\"SBO:0000243\"/></fbc:listOfGeneProducts>";
        m.annotation.package.elements.push(list.to_owned());
        from_packages(&mut m);
        assert_eq!(m.genes[0].label, "a");
        let mut out = m.clone();
        to_packages(&mut out);
        assert_eq!(
            out.annotation.package.elements,
            [list],
            "unchanged, as read"
        );
        // A new gene: the old one's XML kept, the new one's written.
        m.genes.push(Gene {
            id: "G_b".into(),
            label: "b".into(),
            name: Some("pfkB".into()),
            ..Default::default()
        });
        let mut out = m.clone();
        to_packages(&mut out);
        let xml = &out.annotation.package.elements[0];
        assert!(xml.contains("metaid=\"x\""), "{xml}");
        assert!(
            xml.contains("fbc:id=\"G_b\" fbc:label=\"b\" fbc:name=\"pfkB\""),
            "{xml}"
        );
    }
}
