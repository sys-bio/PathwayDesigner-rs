//! The readable diagram format (docs/diagram-format.md, version 0.8): a
//! drawing written in an Antimony block comment, `/* diagram … end */`, after
//! the model it draws, so the file stays plain Antimony.
//!
//! This module finds that comment and reads it into statements
//! ([`ast::Diagram`]); applying them to a model, and writing a model back,
//! come later (docs/diagram-format-implementation-design.md, steps 2 and 3).

pub mod ast;
pub mod colors;
mod lex;
mod parse;
pub(crate) mod read;

use crate::antimony::{Diagnostic, Severity};
use ast::{Diagram, Pos};

/// The diagram comment of an Antimony text: what lies between `/*` and `*/`,
/// and where that starts in the file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section<'a> {
    pub text: &'a str,
    pub start: Pos,
}

/// The first block comment whose first word is `diagram`, or `None`. Strings
/// and line comments (`//`, `#`) are skipped, so a `/*` inside them doesn't
/// count. A comment with no `*/` runs to the end of the text.
pub fn find(antimony: &str) -> Option<Section<'_>> {
    let bytes = antimony.as_bytes();
    let (mut line, mut column) = (1usize, 1usize);
    let mut i = 0;
    // Moves past byte `i`, counting lines and columns (by character).
    let step = |i: &mut usize, line: &mut usize, column: &mut usize| {
        if bytes[*i] == b'\n' {
            *line += 1;
            *column = 1;
        } else if bytes[*i] & 0xC0 != 0x80 {
            *column += 1;
        }
        *i += 1;
    };
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                step(&mut i, &mut line, &mut column);
                while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'\n' {
                    step(&mut i, &mut line, &mut column);
                }
                if i < bytes.len() && bytes[i] == b'"' {
                    step(&mut i, &mut line, &mut column);
                }
            }
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    step(&mut i, &mut line, &mut column);
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    step(&mut i, &mut line, &mut column);
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                step(&mut i, &mut line, &mut column);
                step(&mut i, &mut line, &mut column);
                let start = Pos { line, column };
                let body_start = i;
                let end = antimony[i..].find("*/").map_or(antimony.len(), |e| i + e);
                let text = &antimony[body_start..end];
                let first_word = text
                    .trim_start()
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .next();
                if first_word == Some("diagram") {
                    return Some(Section { text, start });
                }
                while i < end {
                    step(&mut i, &mut line, &mut column);
                }
                i = (end + 2).min(bytes.len());
            }
            _ => step(&mut i, &mut line, &mut column),
        }
    }
    None
}

/// A diagram as read: the statements, if the comment starts as a diagram
/// should, and every problem found, as warnings (format §13: nothing is fatal).
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub diagram: Option<Diagram>,
    pub warnings: Vec<Diagnostic>,
}

/// The diagram in an Antimony text, or `None` if it has no diagram comment.
pub fn parse(antimony: &str) -> Option<Parsed> {
    let section = find(antimony)?;
    let (tokens, lex_errors) = lex::lex(section.text, section.start);
    let (diagram, problems) = parse::parse(&tokens);
    let warning = |pos: Pos, message: String| Diagnostic {
        severity: Severity::Warning,
        line: pos.line,
        column: pos.column,
        message,
    };
    let mut warnings: Vec<Diagnostic> = lex_errors
        .into_iter()
        .map(|e| warning(e.pos, e.message))
        .chain(problems.into_iter().map(|p| warning(p.pos, p.message)))
        .collect();
    warnings.sort_by_key(|w| (w.line, w.column));
    Some(Parsed { diagram, warnings })
}

#[cfg(test)]
mod tests {
    use super::ast::{ClauseKind, StatementKind};
    use super::*;

    const BIUNI: &str = "model biuni
  J1: A + B -> C; k1*A*B
  A = 10; B = 10; C = 0; k1 = 0.1
end

/* diagram biuni v1
  A  at (100, 100)
  B  at (100, 200)
  C  at (300, 150)
  J1 at (200, 150)
end */
";

    #[test]
    fn the_diagram_comment_is_found_where_it_is() {
        let s = find(BIUNI).unwrap();
        assert!(s.text.trim_start().starts_with("diagram biuni v1"));
        assert!(s.text.trim_end().ends_with("end"));
        assert_eq!(s.start, Pos { line: 6, column: 3 });
        let p = parse(BIUNI).unwrap();
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        let d = p.diagram.unwrap();
        assert_eq!(d.statements.len(), 4);
        // Positions are the file's.
        assert_eq!(
            (d.statements[0].pos.line, d.statements[0].pos.column),
            (7, 3)
        );
        assert!(matches!(
            &d.statements[3].kind,
            StatementKind::Item { drawing, clauses, .. }
                if drawing.id == "J1" && clauses[0].kind == ClauseKind::At(crate::geometry::pt(200.0, 150.0))
        ));
    }

    #[test]
    fn other_comments_and_strings_are_passed_over() {
        let text = "// see /* diagram x v1 */ here\n# and /* diagram y v1 */\nnote = \"/* diagram z v1 */\"\n/* a comment */\n/*diagram m v1\nA at (1, 2)\nend*/";
        let d = parse(text).unwrap().diagram.unwrap();
        assert_eq!(d.name, "m");
        assert_eq!(d.statements[0].pos.line, 6);
        // No diagram comment: nothing to read.
        assert!(find("model m\n/* diagrams are drawn elsewhere */\nend").is_none());
        assert!(parse("A -> B; k*A").is_none());
    }

    #[test]
    fn an_unclosed_comment_runs_to_the_end() {
        let p = parse("model m\nend\n/* diagram m v1\nA at (1, 2)\nend\n").unwrap();
        assert!(p.warnings.is_empty());
        assert_eq!(p.diagram.unwrap().statements.len(), 1);
    }

    #[test]
    fn problems_are_warnings_in_file_order() {
        let p =
            parse("model m\nend\n/* diagram m v1\nA at (1, two)\nB { fill #12 }\nC /* x\nend */")
                .unwrap();
        let at: Vec<(usize, usize)> = p.warnings.iter().map(|w| (w.line, w.column)).collect();
        assert_eq!(at, [(4, 3), (5, 5), (6, 3)], "{:?}", p.warnings);
        assert!(p.warnings.iter().all(|w| w.severity == Severity::Warning));
        assert!(p.diagram.is_some());
    }

    #[test]
    fn a_comment_that_isnt_a_diagram_header_is_reported() {
        let p = parse("/* diagram */").unwrap();
        assert!(p.diagram.is_none());
        assert_eq!(p.warnings.len(), 1);
    }

    /// Every complete example in the format's document reads without a
    /// problem, so the document and the reader can't drift apart.
    #[test]
    fn the_documents_examples_read_cleanly() {
        let doc = include_str!("../../../../docs/diagram-format.md");
        let mut n = 0;
        let mut rest = doc;
        // Diagrams that start a line; prose mentions them in passing.
        while let Some(i) = rest.find(
            "
/* diagram ",
        ) {
            let block = &rest[i + 1..];
            let end = block.find("*/").expect("closed") + 2;
            // The template in §3 isn't a diagram.
            if !block.starts_with("/* diagram <") {
                let p = parse(&block[..end]).unwrap();
                assert!(p.warnings.is_empty(), "{}: {:?}", &block[..40], p.warnings);
                assert!(p.diagram.is_some());
                n += 1;
            }
            rest = &block[end..];
        }
        assert!(n >= 2, "found {n} examples");
    }
}
