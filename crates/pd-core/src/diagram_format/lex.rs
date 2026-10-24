//! The diagram section as tokens (docs/diagram-format.md §4).
//!
//! Comments are `//` to the end of the line; a block comment can't go inside,
//! since the section is one itself. Line breaks are tokens, because a statement
//! ends at one (the parser joins lines inside braces and after a comma).

use super::ast::Pos;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    /// An id or a word of the format.
    Word(String),
    /// A number, with its sign.
    Number(f64),
    /// A double-quoted string, without the quotes.
    Str(String),
    /// A hex color, `#` included.
    Hex(String),
    /// The `#` of a drawing number, written against its id (`ATP#2`).
    CopyMark,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Equals,
    /// `->`
    Arrow,
    Newline,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
}

/// A problem found while splitting the text into tokens.
#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub pos: Pos,
    pub message: String,
}

/// The tokens of `text`, which starts at `start` in the file.
pub fn lex(text: &str, start: Pos) -> (Vec<Token>, Vec<LexError>) {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let (mut line, mut column) = (start.line, start.column);
    let mut i = 0;
    // Whether the last character was part of a word or number, with nothing
    // between: `ATP#2` against `ATP #2`.
    let mut glued = false;
    while i < chars.len() {
        let c = chars[i];
        let pos = Pos { line, column };
        let advance = |n: usize, column: &mut usize, i: &mut usize| {
            *column += n;
            *i += n;
        };
        if c == '\n' {
            tokens.push(Token {
                tok: Tok::Newline,
                pos,
            });
            line += 1;
            column = 1;
            i += 1;
            glued = false;
            continue;
        }
        if c.is_whitespace() {
            advance(1, &mut column, &mut i);
            glued = false;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                advance(1, &mut column, &mut i);
            }
            glued = false;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            errors.push(LexError {
                pos,
                message: "a block comment can't go inside the diagram, which is one itself; use //"
                    .into(),
            });
            while i < chars.len() && chars[i] != '\n' {
                advance(1, &mut column, &mut i);
            }
            glued = false;
            continue;
        }
        let was_glued = glued;
        glued = false;
        if c.is_ascii_alphabetic() || c == '_' {
            let s = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                advance(1, &mut column, &mut i);
            }
            tokens.push(Token {
                tok: Tok::Word(chars[s..i].iter().collect()),
                pos,
            });
            glued = true;
            continue;
        }
        let starts_number = |j: usize| {
            chars.get(j).is_some_and(|d| d.is_ascii_digit())
                || (chars.get(j) == Some(&'.')
                    && chars.get(j + 1).is_some_and(|d| d.is_ascii_digit()))
        };
        if c.is_ascii_digit()
            || (c == '.' && starts_number(i))
            || (c == '-' && starts_number(i + 1))
        {
            let s = i;
            if c == '-' {
                advance(1, &mut column, &mut i);
            }
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                advance(1, &mut column, &mut i);
            }
            if i < chars.len()
                && (chars[i] == 'e' || chars[i] == 'E')
                && (starts_number(i + 1)
                    || (matches!(chars.get(i + 1), Some('+') | Some('-')) && starts_number(i + 2)))
            {
                advance(2, &mut column, &mut i);
                while i < chars.len() && chars[i].is_ascii_digit() {
                    advance(1, &mut column, &mut i);
                }
            }
            let raw: String = chars[s..i].iter().collect();
            match raw.parse::<f64>() {
                Ok(v) => tokens.push(Token {
                    tok: Tok::Number(v),
                    pos,
                }),
                Err(_) => errors.push(LexError {
                    pos,
                    message: format!("'{raw}' isn't a number"),
                }),
            }
            glued = true;
            continue;
        }
        if c == '"' {
            let s = i + 1;
            let mut j = s;
            while j < chars.len() && chars[j] != '"' && chars[j] != '\n' {
                j += 1;
            }
            if chars.get(j) == Some(&'"') {
                tokens.push(Token {
                    tok: Tok::Str(chars[s..j].iter().collect()),
                    pos,
                });
                advance(j + 1 - i, &mut column, &mut i);
            } else {
                errors.push(LexError {
                    pos,
                    message: "the string has no closing \" on its line".into(),
                });
                advance(j - i, &mut column, &mut i);
            }
            continue;
        }
        if c == '#' {
            // Against an id and followed by digits, a drawing number; otherwise
            // a hex color.
            if was_glued
                && matches!(
                    tokens.last(),
                    Some(Token {
                        tok: Tok::Word(_),
                        ..
                    })
                )
            {
                tokens.push(Token {
                    tok: Tok::CopyMark,
                    pos,
                });
                advance(1, &mut column, &mut i);
                glued = true;
                continue;
            }
            let s = i;
            advance(1, &mut column, &mut i);
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                advance(1, &mut column, &mut i);
            }
            tokens.push(Token {
                tok: Tok::Hex(chars[s..i].iter().collect()),
                pos,
            });
            continue;
        }
        if c == '-' && chars.get(i + 1) == Some(&'>') {
            tokens.push(Token {
                tok: Tok::Arrow,
                pos,
            });
            advance(2, &mut column, &mut i);
            continue;
        }
        let tok = match c {
            '(' => Some(Tok::LParen),
            ')' => Some(Tok::RParen),
            '{' => Some(Tok::LBrace),
            '}' => Some(Tok::RBrace),
            ',' => Some(Tok::Comma),
            ';' => Some(Tok::Semi),
            '=' => Some(Tok::Equals),
            _ => None,
        };
        match tok {
            Some(tok) => tokens.push(Token { tok, pos }),
            None => errors.push(LexError {
                pos,
                message: format!("'{c}' isn't used in a diagram"),
            }),
        }
        advance(1, &mut column, &mut i);
    }
    (tokens, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str) -> Vec<Tok> {
        let (t, e) = lex(text, Pos { line: 1, column: 1 });
        assert!(e.is_empty(), "{e:?}");
        t.into_iter().map(|t| t.tok).collect()
    }

    fn w(s: &str) -> Tok {
        Tok::Word(s.into())
    }

    #[test]
    fn words_numbers_points_and_punctuation() {
        assert_eq!(
            toks("Glc at (200, -12.5) style metabolite { id offset (0, -28); bold }"),
            [
                w("Glc"),
                w("at"),
                Tok::LParen,
                Tok::Number(200.0),
                Tok::Comma,
                Tok::Number(-12.5),
                Tok::RParen,
                w("style"),
                w("metabolite"),
                Tok::LBrace,
                w("id"),
                w("offset"),
                Tok::LParen,
                Tok::Number(0.0),
                Tok::Comma,
                Tok::Number(-28.0),
                Tok::RParen,
                Tok::Semi,
                w("bold"),
                Tok::RBrace,
            ]
        );
        assert_eq!(
            toks("1e-3 .5 2E2 80 x 36"),
            [
                Tok::Number(1e-3),
                Tok::Number(0.5),
                Tok::Number(200.0),
                Tok::Number(80.0),
                w("x"),
                Tok::Number(36.0)
            ]
        );
        assert_eq!(
            toks("leg Glc -> J1\ncolor a = #fff"),
            [
                w("leg"),
                w("Glc"),
                Tok::Arrow,
                w("J1"),
                Tok::Newline,
                w("color"),
                w("a"),
                Tok::Equals,
                Tok::Hex("#fff".into())
            ]
        );
    }

    #[test]
    fn a_hash_against_an_id_numbers_a_drawing_and_elsewhere_starts_a_color() {
        assert_eq!(
            toks("ATP#2 { fill #2E7D32 }"),
            [
                w("ATP"),
                Tok::CopyMark,
                Tok::Number(2.0),
                Tok::LBrace,
                w("fill"),
                Tok::Hex("#2E7D32".into()),
                Tok::RBrace
            ]
        );
        assert_eq!(toks("ATP #2"), [w("ATP"), Tok::Hex("#2".into())]);
    }

    #[test]
    fn strings_and_comments() {
        assert_eq!(
            toks("text \"A // b\" at (1, 2) // a note\nX"),
            [
                w("text"),
                Tok::Str("A // b".into()),
                w("at"),
                Tok::LParen,
                Tok::Number(1.0),
                Tok::Comma,
                Tok::Number(2.0),
                Tok::RParen,
                Tok::Newline,
                w("X")
            ]
        );
    }

    #[test]
    fn positions_count_from_where_the_section_starts() {
        let (t, _) = lex("ab\n  cd", Pos { line: 7, column: 3 });
        assert_eq!(t[0].pos, Pos { line: 7, column: 3 });
        assert_eq!(t[2].pos, Pos { line: 8, column: 3 });
    }

    #[test]
    fn what_doesnt_read_is_reported_where_it_is() {
        let (t, e) = lex("A /* x B\n\"open\nC @", Pos { line: 1, column: 1 });
        let msgs: Vec<(usize, usize)> = e.iter().map(|e| (e.pos.line, e.pos.column)).collect();
        assert_eq!(msgs, [(1, 3), (2, 1), (3, 3)], "{e:?}");
        assert!(e[0].message.contains("block comment"));
        assert!(t.contains(&Token {
            tok: w("C"),
            pos: Pos { line: 3, column: 1 }
        }));
    }
}
