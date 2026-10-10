//! The Antimony reader.
//!
//! The parser works statement by statement and carries on after a problem, so
//! every error in the text is reported at once. A statement it doesn't
//! understand is an error; an Antimony feature it knows but doesn't support
//! (functions, events, units, …) is skipped with a warning.

use crate::description::{
    Assignment, Compartment, Description, Diagnostic, Reaction, Severity, Species, Term, Text,
};

/// The result of reading a text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parsed {
    pub description: Description,
    pub diagnostics: Vec<Diagnostic>,
}

impl Parsed {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }
}

/// The 1-based line and column (in characters) of a byte offset in `src`.
pub fn line_column(src: &str, offset: usize) -> (usize, usize) {
    let before = &src[..offset.min(src.len())];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[line_start..].chars().count() + 1)
}

/// Antimony features that are known but not supported: their statements are
/// skipped with a warning.
const UNSUPPORTED: [&str; 12] = [
    "function",
    "unit",
    "import",
    "at",
    "event",
    "DNA",
    "gene",
    "operator",
    "promoter",
    "terminator",
    "delete",
    "substanceOnly",
];

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Number(String),
    Sym(&'static str),
    /// Something the reader has no use for, kept for error messages.
    Other(String),
    /// A line break outside brackets: it ends a statement.
    Newline,
    End,
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    start: usize,
    end: usize,
}

const SYMBOLS: [&str; 26] = [
    "->", "=>", ":=", "<=", ">=", "==", "!=", "&&", "||", "=", ":", ";", ",", "+", "-", "*", "/",
    "^", "$", "(", ")", "<", ">", "!", ".", "'",
];

fn lex(src: &str) -> Vec<Token> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut depth = 0i32;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if c == b'\n' {
            i += 1;
            if depth <= 0 {
                out.push(Token {
                    tok: Tok::Newline,
                    start,
                    end: i,
                });
            }
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Comments: `//` and `#` to the end of the line; `#` with six hex
        // digits is a colour, not a comment (S §11.2).
        let colour = c == b'#'
            && bytes.len() >= i + 7
            && bytes[i + 1..i + 7].iter().all(u8::is_ascii_hexdigit)
            && bytes.get(i + 7).is_none_or(|b| !b.is_ascii_alphanumeric());
        if src[i..].starts_with("//") || (c == b'#' && !colour) {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if colour {
            i += 7;
            out.push(Token {
                tok: Tok::Other(src[start..i].to_owned()),
                start,
                end: i,
            });
            continue;
        }
        let digits = |mut j: usize| {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            j
        };
        if c.is_ascii_digit() || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            i = digits(i);
            if i < bytes.len() && bytes[i] == b'.' {
                i = digits(i + 1);
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                let mut j = i + 1;
                if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j].is_ascii_digit() {
                    i = digits(j);
                }
            }
            out.push(Token {
                tok: Tok::Number(src[start..i].to_owned()),
                start,
                end: i,
            });
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(src[start..i].to_owned()),
                start,
                end: i,
            });
            continue;
        }
        if let Some(sym) = SYMBOLS.iter().find(|s| src[i..].starts_with(*s)) {
            i += sym.len();
            match *sym {
                "(" => depth += 1,
                ")" => depth -= 1,
                _ => {}
            }
            out.push(Token {
                tok: Tok::Sym(sym),
                start,
                end: i,
            });
            continue;
        }
        let ch = src[i..].chars().next().expect("not at the end");
        i += ch.len_utf8();
        out.push(Token {
            tok: Tok::Other(ch.to_string()),
            start,
            end: i,
        });
    }
    out.push(Token {
        tok: Tok::End,
        start: src.len(),
        end: src.len(),
    });
    out
}

/// Reads Antimony text. A byte-order mark at the start is ignored (offsets
/// still count it, so they match the text the caller has).
pub fn parse(src: &str) -> Parsed {
    let skip = if src.starts_with('\u{FEFF}') { 3 } else { 0 };
    let mut tokens = lex(&src[skip..]);
    for t in &mut tokens {
        t.start += skip;
        t.end += skip;
    }
    let mut p = Parser {
        src,
        tokens,
        pos: 0,
        out: Parsed::default(),
    };
    p.run();
    p.out
}

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    out: Parsed,
}

/// Something went wrong in a statement: report it and skip to the next one.
struct Bail;

impl Parser<'_> {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].tok
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn at_sym(&self, s: &str) -> bool {
        self.peek()
            == &Tok::Sym(match s {
                "->" => "->",
                "=>" => "=>",
                _ => return matches!(self.peek(), Tok::Sym(x) if *x == s),
            })
    }

    /// True at `;`, a line break or the end: where a statement stops.
    fn at_statement_end(&self) -> bool {
        matches!(self.peek(), Tok::Sym(";") | Tok::Newline | Tok::End)
    }

    fn line_of(&self, offset: usize) -> usize {
        line_column(self.src, offset).0
    }

    fn report(&mut self, severity: Severity, offset: usize, message: impl Into<String>) {
        let (line, column) = line_column(self.src, offset);
        self.out.diagnostics.push(Diagnostic {
            severity,
            line,
            column,
            message: message.into(),
        });
    }

    fn error<T>(&mut self, message: impl Into<String>) -> Result<T, Bail> {
        let at = self.tokens[self.pos].start;
        let found = match self.peek() {
            Tok::Ident(s) | Tok::Number(s) | Tok::Other(s) => format!(" (found '{s}')"),
            Tok::Sym(s) => format!(" (found '{s}')"),
            Tok::Newline => " (at the end of the line)".to_owned(),
            Tok::End => " (at the end)".to_owned(),
        };
        self.report(Severity::Error, at, format!("{}{found}", message.into()));
        Err(Bail)
    }

    /// Skips to just past the end of the current statement.
    fn skip_statement(&mut self) {
        while !self.at_statement_end() {
            self.bump();
        }
        if !matches!(self.peek(), Tok::End) {
            self.bump();
        }
    }

    fn ident(&mut self, what: &str) -> Result<(String, usize), Bail> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let t = self.bump();
                Ok((name, t.start))
            }
            _ => self.error(format!("expected {what}")),
        }
    }

    fn run(&mut self) {
        loop {
            match self.peek() {
                Tok::End => return,
                Tok::Newline | Tok::Sym(";") => {
                    self.bump();
                }
                _ => {
                    if self.statement().is_err() {
                        self.skip_statement();
                    } else if !self.at_statement_end() {
                        let _ = self.error::<()>("expected the end of the statement");
                        self.skip_statement();
                    }
                }
            }
        }
    }

    fn statement(&mut self) -> Result<(), Bail> {
        let first = self.peek().clone();
        let start = self.tokens[self.pos].start;
        match &first {
            Tok::Ident(kw) if kw == "model" => self.model_header(),
            Tok::Ident(kw) if kw == "end" => {
                self.bump();
                Ok(())
            }
            Tok::Ident(kw) if kw == "compartment" => {
                self.bump();
                self.compartments()
            }
            Tok::Ident(kw) if kw == "species" || kw == "var" => {
                self.bump();
                self.species_list()
            }
            // `const` only says a value doesn't change; the rest of the
            // statement is read as usual.
            Tok::Ident(kw) if kw == "const" => {
                self.bump();
                if matches!(self.peek(), Tok::Ident(k) if k == "species" || k == "var") {
                    self.bump();
                    return self.species_list();
                }
                self.statement()
            }
            Tok::Ident(kw) if UNSUPPORTED.contains(&kw.as_str()) => self.unsupported(start, kw),
            // `E1: at (…): …` is an event.
            Tok::Ident(_)
                if self.peek_at(1) == &Tok::Sym(":")
                    && matches!(self.peek_at(2), Tok::Ident(k) if k == "at") =>
            {
                self.unsupported(start, "at")
            }
            // `x is "…"` and `x has …` describe things; nothing to draw.
            Tok::Ident(_) if matches!(self.peek_at(1), Tok::Ident(k) if k == "is" || k == "has") => {
                self.unsupported(start, "is")
            }
            _ => self.general(),
        }
    }

    fn unsupported(&mut self, start: usize, what: &str) -> Result<(), Bail> {
        self.report(
            Severity::Warning,
            start,
            format!("'{what}' statements aren't supported; this one was skipped"),
        );
        // A function runs over several lines, up to its `end`.
        if what == "function" {
            loop {
                match self.peek() {
                    Tok::End => return Ok(()),
                    Tok::Ident(k) if k == "end" => {
                        self.bump();
                        return Ok(());
                    }
                    _ => {
                        self.bump();
                    }
                }
            }
        }
        // Up to the statement's end, which the caller deals with.
        while !self.at_statement_end() {
            self.bump();
        }
        Ok(())
    }

    /// `model name`, `model name()`, `model *name()`.
    fn model_header(&mut self) -> Result<(), Bail> {
        self.bump();
        if self.at_sym("*") {
            self.bump();
        }
        let (name, _) = self.ident("the model's name")?;
        if self.at_sym("(") {
            self.bump();
            while !self.at_sym(")") {
                if self.at_statement_end() {
                    return self.error("expected ')'");
                }
                self.bump();
            }
            self.bump();
        }
        self.out.description.name = Some(name);
        Ok(())
    }

    /// The text of an expression running to `,` (if `stop_at_comma`), `;`, a
    /// line break or the end.
    fn expression(&mut self, stop_at_comma: bool) -> Result<Text, Bail> {
        let first = self.pos;
        let mut depth = 0;
        loop {
            match self.peek() {
                Tok::Sym(";") | Tok::Newline | Tok::End => break,
                Tok::Sym(",") if stop_at_comma && depth == 0 => break,
                Tok::Sym("(") => depth += 1,
                Tok::Sym(")") => depth -= 1,
                _ => {}
            }
            self.bump();
        }
        if self.pos == first {
            return self.error("expected an expression");
        }
        let (start, end) = (self.tokens[first].start, self.tokens[self.pos - 1].end);
        Ok(Text {
            text: self.src[start..end].to_owned(),
            offset: start,
        })
    }

    fn compartments(&mut self) -> Result<(), Bail> {
        loop {
            let (id, _) = self.ident("a compartment name")?;
            let size = if self.at_sym("=") {
                self.bump();
                Some(self.expression(true)?)
            } else {
                None
            };
            self.out
                .description
                .compartments
                .push(Compartment { id, size });
            if !self.at_sym(",") {
                return Ok(());
            }
            self.bump();
        }
    }

    /// `[$]S [in C] [= value]`, separated by commas.
    fn species_list(&mut self) -> Result<(), Bail> {
        loop {
            let boundary = self.at_sym("$");
            if boundary {
                self.bump();
            }
            let (id, _) = self.ident("a species name")?;
            let mut compartment = None;
            if matches!(self.peek(), Tok::Ident(k) if k == "in") {
                self.bump();
                compartment = Some(self.ident("a compartment name after 'in'")?.0);
            }
            let initial = if self.at_sym("=") {
                self.bump();
                Some(self.expression(true)?)
            } else {
                None
            };
            self.out.description.species.push(Species {
                id,
                boundary,
                compartment,
                initial,
            });
            if !self.at_sym(",") {
                return Ok(());
            }
            self.bump();
        }
    }

    /// Anything that doesn't start with a keyword: a reaction, `x = expr`,
    /// `x := expr` or `$S = value`.
    fn general(&mut self) -> Result<(), Bail> {
        // A reaction has an arrow before the statement ends.
        let mut i = self.pos;
        let mut is_reaction = false;
        while !matches!(self.tokens[i].tok, Tok::Sym(";") | Tok::Newline | Tok::End) {
            if matches!(self.tokens[i].tok, Tok::Sym("->") | Tok::Sym("=>")) {
                is_reaction = true;
                break;
            }
            i += 1;
        }
        if is_reaction {
            return self.reaction();
        }
        let line = self.line_of(self.tokens[self.pos].start);
        let boundary = self.at_sym("$");
        if boundary {
            self.bump();
        }
        let (variable, _) = self.ident("a reaction, a declaration or an assignment")?;
        if !boundary && self.at_sym(":=") {
            self.bump();
            let expression = self.expression(false)?;
            self.out.description.rules.push(Assignment {
                variable,
                expression,
                line,
            });
            return Ok(());
        }
        if !self.at_sym("=") {
            return self.error("expected '=', ':=' or a reaction arrow");
        }
        self.bump();
        let expression = self.expression(false)?;
        if boundary {
            // `$S = value` declares a boundary species with that value.
            self.out.description.species.push(Species {
                id: variable,
                boundary: true,
                compartment: None,
                initial: Some(expression),
            });
        } else {
            self.out.description.assignments.push(Assignment {
                variable,
                expression,
                line,
            });
        }
        Ok(())
    }

    /// `[id:] side -> side [; rate]`.
    fn reaction(&mut self) -> Result<(), Bail> {
        let line = self.line_of(self.tokens[self.pos].start);
        let mut id = None;
        if matches!(self.peek(), Tok::Ident(_)) && self.peek_at(1) == &Tok::Sym(":") {
            id = Some(self.ident("a reaction id")?.0);
            self.bump();
        }
        let reactants = self.side()?;
        if !(self.at_sym("->") || self.at_sym("=>")) {
            return self.error("expected '->' or '+'");
        }
        self.bump();
        let products = self.side()?;
        let rate = if self.at_sym(";") {
            self.bump();
            if matches!(self.peek(), Tok::Newline | Tok::End | Tok::Sym(";")) {
                None
            } else {
                Some(self.expression(false)?)
            }
        } else {
            None
        };
        self.out.description.reactions.push(Reaction {
            id,
            reactants,
            products,
            rate,
            line,
        });
        Ok(())
    }

    /// `term (+ term)*`, or nothing.
    fn side(&mut self) -> Result<Vec<Term>, Bail> {
        let mut terms = Vec::new();
        if self.at_sym("->") || self.at_sym("=>") || self.at_statement_end() {
            return Ok(terms);
        }
        loop {
            let mut stoichiometry = 1.0;
            if let Tok::Number(n) = self.peek().clone() {
                self.bump();
                stoichiometry = match n.parse() {
                    Ok(v) => v,
                    Err(_) => return self.error("expected a stoichiometry"),
                };
                if self.at_sym("*") {
                    self.bump();
                }
            }
            let boundary = self.at_sym("$");
            if boundary {
                self.bump();
            }
            let (species, _) = self.ident("a species name")?;
            terms.push(Term {
                stoichiometry,
                species,
                boundary,
            });
            if !self.at_sym("+") {
                return Ok(terms);
            }
            self.bump();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> Description {
        let p = parse(src);
        assert!(!p.has_errors(), "{src}: {:?}", p.diagnostics);
        p.description
    }

    fn first_error(src: &str) -> Diagnostic {
        let p = parse(src);
        p.errors()
            .next()
            .cloned()
            .unwrap_or_else(|| panic!("no error for {src}"))
    }

    #[test]
    fn reactions_with_ids_stoichiometry_and_rates() {
        let d = ok("J0: 2 A + 3*$B -> C; k1*A*B\nS1 => ; k2");
        assert_eq!(d.reactions.len(), 2);
        let r = &d.reactions[0];
        assert_eq!(r.id.as_deref(), Some("J0"));
        assert_eq!(
            r.reactants,
            [
                Term {
                    stoichiometry: 2.0,
                    species: "A".into(),
                    boundary: false
                },
                Term {
                    stoichiometry: 3.0,
                    species: "B".into(),
                    boundary: true
                },
            ]
        );
        assert_eq!(r.products[0].species, "C");
        assert_eq!(r.rate.as_ref().unwrap().text, "k1*A*B");
        assert_eq!(r.line, 1);
        // `=>` is the same arrow; an empty side is fine.
        let r = &d.reactions[1];
        assert_eq!(r.id, None);
        assert!(r.products.is_empty());
        assert_eq!(r.rate.as_ref().unwrap().text, "k2");
        assert_eq!(r.line, 2);
    }

    #[test]
    fn empty_sides_and_no_rate() {
        let d = ok("-> A; k\nB ->\nC -> D;");
        assert!(d.reactions[0].reactants.is_empty());
        assert!(d.reactions[1].products.is_empty() && d.reactions[1].rate.is_none());
        assert!(d.reactions[2].rate.is_none());
    }

    #[test]
    fn declarations() {
        let d = ok(
            "compartment cell = 2.5, nucleus\nspecies $S in cell = 1, T, U = 0.5\nvar V\n$W = 3",
        );
        assert_eq!(d.compartments[0].id, "cell");
        assert_eq!(d.compartments[0].size.as_ref().unwrap().text, "2.5");
        assert!(d.compartments[1].size.is_none());
        let s = &d.species;
        assert_eq!(s.len(), 5);
        assert!(s[0].boundary);
        assert_eq!(s[0].compartment.as_deref(), Some("cell"));
        assert_eq!(s[0].initial.as_ref().unwrap().text, "1");
        assert_eq!(s[2].initial.as_ref().unwrap().text, "0.5");
        assert_eq!(s[3].id, "V");
        assert!(s[4].boundary && s[4].initial.as_ref().unwrap().text == "3");
    }

    #[test]
    fn assignments_and_rules() {
        let src = "k1 = 0.1\nVt := Vm * (E + 2);\nS1 = 3.5";
        let d = ok(src);
        assert_eq!(d.assignments.len(), 2);
        assert_eq!(d.assignments[0].variable, "k1");
        assert_eq!(d.assignments[0].expression.text, "0.1");
        assert_eq!(d.rules[0].variable, "Vt");
        let e = &d.rules[0].expression;
        assert_eq!(e.text, "Vm * (E + 2)");
        assert_eq!(&src[e.offset..e.offset + e.text.len()], e.text);
        assert_eq!(d.rules[0].line, 2);
    }

    #[test]
    fn model_wrapper() {
        let d = ok("model test\n  S1 -> S2; k\nend");
        assert_eq!(d.name.as_deref(), Some("test"));
        assert_eq!(d.reactions.len(), 1);
        assert_eq!(ok("model *main()\nend").name.as_deref(), Some("main"));
        assert_eq!(ok("S1 -> S2").name, None);
    }

    #[test]
    fn comments_and_colours() {
        let d = ok("// a comment\nA -> B; k # another\n# whole line\nk = 1 // trailing");
        assert_eq!(d.reactions[0].rate.as_ref().unwrap().text, "k");
        assert_eq!(d.assignments[0].expression.text, "1");
        // `#` with six hex digits is a colour: not a comment, so here it is
        // an error rather than silently dropped text.
        assert!(parse("A -> B #FF0000").has_errors());
    }

    #[test]
    fn statements_end_at_semicolons_or_new_lines_but_not_inside_brackets() {
        let d = ok("k1 = 1; k2 = 2\nA -> B; k*(A +\n  B)");
        assert_eq!(d.assignments.len(), 2);
        assert_eq!(d.reactions[0].rate.as_ref().unwrap().text, "k*(A +\n  B)");
    }

    #[test]
    fn byte_order_mark() {
        let src = "\u{FEFF}  species A = 1;";
        let d = ok(src);
        assert_eq!(d.species[0].id, "A");
        let e = d.species[0].initial.as_ref().unwrap();
        assert_eq!(&src[e.offset..e.offset + 1], "1");
    }

    #[test]
    fn errors_have_positions_and_parsing_carries_on() {
        let p = parse("A -> B; k\nA + -> C\nk = \nD -> E");
        let errors: Vec<_> = p.errors().collect();
        assert_eq!(errors.len(), 2);
        assert_eq!((errors[0].line, errors[0].column), (2, 5));
        assert!(errors[0].message.contains("species name"));
        assert_eq!(errors[1].line, 3);
        assert!(errors[1].message.contains("expression"));
        // The good statements around them were still read.
        assert_eq!(p.description.reactions.len(), 2);
    }

    #[test]
    fn more_errors() {
        assert!(
            first_error("A B")
                .message
                .contains("'=', ':=' or a reaction arrow")
        );
        assert!(first_error("species").message.contains("species name"));
        assert!(first_error("species A in").message.contains("compartment"));
        assert!(first_error("model").message.contains("model's name"));
        assert!(
            first_error("A -> B C")
                .message
                .contains("end of the statement")
        );
        assert!(first_error("§").message.contains("'§'"));
    }

    #[test]
    fn unsupported_statements_are_skipped_with_a_warning() {
        let p = parse(
            "function f(x)\n  x^2\nend\nunit mM = 1e-3 mole/litre\nE1: at (time > 5): S1 = 3\nS1 is \"glucose\"\nA -> B; k",
        );
        assert!(!p.has_errors(), "{:?}", p.diagnostics);
        assert_eq!(p.warnings().count(), 4);
        assert_eq!(p.description.reactions.len(), 1);
    }

    #[test]
    fn const_is_read_through() {
        let d = ok("const k1 = 0.5\nconst species S = 2");
        assert_eq!(d.assignments[0].variable, "k1");
        assert_eq!(d.species[0].id, "S");
    }

    #[test]
    fn line_and_column() {
        let s = "ab\ncdé f";
        assert_eq!(line_column(s, 0), (1, 1));
        assert_eq!(line_column(s, 3), (2, 1));
        // Columns count characters, not bytes.
        assert_eq!(line_column(s, s.find('f').unwrap()), (2, 5));
    }
}
