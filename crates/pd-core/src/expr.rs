//! Kinetic-law expressions (S §8.3, SPEC-Rust §8.1).
//!
//! Rate laws, parameter expressions and assignment rules are stored as text.
//! This module parses them where they need to be understood: modifier
//! detection now, MathML export later. The full grammar is implemented, so
//! the reference's gaps are fixed (D-20): `&&`, `||`, `and`, `or` and `not`
//! work, a `-` is always an operator (`k1-2` is `k1 − 2`), and a number may
//! start with a point (`.5`).
//!
//! ```text
//! expr       := or
//! or         := and ( ('||' | 'or') and )*
//! and        := equality ( ('&&' | 'and') equality )*
//! equality   := comparison ( ('==' | '!=') comparison )*
//! comparison := additive ( ('<' | '<=' | '>' | '>=') additive )*
//! additive   := term ( ('+' | '-') term )*
//! term       := power ( ('*' | '/') power )*
//! power      := unary ( '^' unary )*          // right-associative
//! unary      := ('-' | '!' | 'not') unary | primary
//! primary    := NUMBER | IDENT | IDENT '(' [expr (',' expr)*] ')' | '(' expr ')'
//! ```

use std::fmt;

/// Functions with a known meaning (S §8.3). Others are passed through.
pub const KNOWN_FUNCTIONS: [&str; 21] = [
    "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp", "ln", "log",
    "log10", "sqrt", "abs", "floor", "ceil", "ceiling", "pow", "min", "max",
];

/// A byte range in the source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Or => "||",
            BinOp::And => "&&",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Pow => "^",
        }
    }

    /// Binding strength, as in the grammar: higher binds tighter.
    fn precedence(self) -> u8 {
        match self {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::Eq | BinOp::Ne => 3,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
            BinOp::Add | BinOp::Sub => 5,
            BinOp::Mul | BinOp::Div => 6,
            BinOp::Pow => 7,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

/// Unary operators bind tighter than `^` in this grammar: `-a^2` is `(-a)^2`.
const UNARY_PRECEDENCE: u8 = 8;
const ATOM_PRECEDENCE: u8 = 9;

/// A parsed expression. Every node keeps the span of its source text.
#[derive(Clone, Debug, PartialEq)]
pub enum Ast {
    /// The number as written, so it is printed back exactly.
    Number(String, Span),
    Ident(String, Span),
    Unary(UnOp, Box<Ast>, Span),
    Binary(BinOp, Box<Ast>, Box<Ast>, Span),
    Call(String, Vec<Ast>, Span),
}

impl Ast {
    pub fn span(&self) -> Span {
        match self {
            Ast::Number(_, s) | Ast::Ident(_, s) | Ast::Unary(_, _, s) => *s,
            Ast::Binary(_, _, _, s) | Ast::Call(_, _, s) => *s,
        }
    }

    fn precedence(&self) -> u8 {
        match self {
            Ast::Binary(op, ..) => op.precedence(),
            Ast::Unary(..) => UNARY_PRECEDENCE,
            _ => ATOM_PRECEDENCE,
        }
    }
}

/// A syntax error, with where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at {}: {}", self.span.start + 1, self.message)
    }
}

impl std::error::Error for ParseError {}

// ----- Lexer -----------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Number(String),
    Ident(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
    End,
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    span: Span,
}

fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let err = |start: usize, end: usize, message: String| ParseError {
        span: Span { start, end },
        message,
    };
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if c.is_ascii_whitespace() {
            i += 1;
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
                } else {
                    return Err(err(start, j, "a number's exponent needs digits".into()));
                }
            }
            out.push(Token {
                tok: Tok::Number(src[start..i].to_owned()),
                span: Span { start, end: i },
            });
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &src[start..i];
            let tok = match word {
                "and" => Tok::Op("&&"),
                "or" => Tok::Op("||"),
                "not" => Tok::Op("!"),
                _ => Tok::Ident(word.to_owned()),
            };
            out.push(Token {
                tok,
                span: Span { start, end: i },
            });
            continue;
        }
        let two = src.get(i..i + 2).unwrap_or("");
        let (tok, len) = match two {
            "&&" => (Tok::Op("&&"), 2),
            "||" => (Tok::Op("||"), 2),
            "==" => (Tok::Op("=="), 2),
            "!=" => (Tok::Op("!="), 2),
            "<=" => (Tok::Op("<="), 2),
            ">=" => (Tok::Op(">="), 2),
            _ => match c {
                b'+' => (Tok::Op("+"), 1),
                b'-' => (Tok::Op("-"), 1),
                b'*' => (Tok::Op("*"), 1),
                b'/' => (Tok::Op("/"), 1),
                b'^' => (Tok::Op("^"), 1),
                b'<' => (Tok::Op("<"), 1),
                b'>' => (Tok::Op(">"), 1),
                b'!' => (Tok::Op("!"), 1),
                b'(' => (Tok::LParen, 1),
                b')' => (Tok::RParen, 1),
                b',' => (Tok::Comma, 1),
                _ => {
                    let ch = src[i..].chars().next().expect("not at the end");
                    return Err(err(
                        i,
                        i + ch.len_utf8(),
                        format!("unexpected character '{ch}'"),
                    ));
                }
            },
        };
        i += len;
        out.push(Token {
            tok,
            span: Span { start, end: i },
        });
    }
    out.push(Token {
        tok: Tok::End,
        span: Span {
            start: src.len(),
            end: src.len(),
        },
    });
    Ok(out)
}

// ----- Parser ----------------------------------------------------------------

/// Parses an expression (S §8.3).
pub fn parse(src: &str) -> Result<Ast, ParseError> {
    let tokens = lex(src)?;
    let mut p = Parser { tokens, pos: 0 };
    if p.peek() == &Tok::End {
        return Err(p.error("the expression is empty"));
    }
    let ast = p.expr()?;
    if p.peek() != &Tok::End {
        return Err(p.error("unexpected text after the expression"));
    }
    Ok(ast)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

/// The binary levels from loosest to tightest, below `power`.
const LEVELS: [&[(&str, BinOp)]; 6] = [
    &[("||", BinOp::Or)],
    &[("&&", BinOp::And)],
    &[("==", BinOp::Eq), ("!=", BinOp::Ne)],
    &[
        ("<", BinOp::Lt),
        ("<=", BinOp::Le),
        (">", BinOp::Gt),
        (">=", BinOp::Ge),
    ],
    &[("+", BinOp::Add), ("-", BinOp::Sub)],
    &[("*", BinOp::Mul), ("/", BinOp::Div)],
];

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].tok
    }

    fn span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn error(&self, message: &str) -> ParseError {
        let message = match self.peek() {
            Tok::End if !message.contains("empty") => format!("{message} (at the end)"),
            _ => message.to_owned(),
        };
        ParseError {
            span: self.span(),
            message,
        }
    }

    fn expr(&mut self) -> Result<Ast, ParseError> {
        self.level(0)
    }

    /// A left-associative binary level, or `power` below the last one.
    fn level(&mut self, n: usize) -> Result<Ast, ParseError> {
        if n == LEVELS.len() {
            return self.power();
        }
        let mut left = self.level(n + 1)?;
        loop {
            let op = match self.peek() {
                Tok::Op(s) => LEVELS[n]
                    .iter()
                    .find(|(sym, _)| sym == s)
                    .map(|&(_, op)| op),
                _ => None,
            };
            let Some(op) = op else {
                return Ok(left);
            };
            self.bump();
            let right = self.level(n + 1)?;
            let span = Span {
                start: left.span().start,
                end: right.span().end,
            };
            left = Ast::Binary(op, Box::new(left), Box::new(right), span);
        }
    }

    /// `unary ('^' unary)*`, grouping from the right.
    fn power(&mut self) -> Result<Ast, ParseError> {
        let base = self.unary()?;
        if self.peek() != &Tok::Op("^") {
            return Ok(base);
        }
        self.bump();
        let exponent = self.power()?;
        let span = Span {
            start: base.span().start,
            end: exponent.span().end,
        };
        Ok(Ast::Binary(
            BinOp::Pow,
            Box::new(base),
            Box::new(exponent),
            span,
        ))
    }

    fn unary(&mut self) -> Result<Ast, ParseError> {
        let op = match self.peek() {
            Tok::Op("-") => Some(UnOp::Neg),
            Tok::Op("!") => Some(UnOp::Not),
            _ => None,
        };
        let Some(op) = op else {
            return self.primary();
        };
        let start = self.bump().span.start;
        let operand = self.unary()?;
        let span = Span {
            start,
            end: operand.span().end,
        };
        Ok(Ast::Unary(op, Box::new(operand), span))
    }

    fn primary(&mut self) -> Result<Ast, ParseError> {
        match self.peek().clone() {
            Tok::Number(n) => {
                let t = self.bump();
                Ok(Ast::Number(n, t.span))
            }
            Tok::Ident(name) => {
                let t = self.bump();
                if self.peek() != &Tok::LParen {
                    return Ok(Ast::Ident(name, t.span));
                }
                self.bump();
                let mut args = Vec::new();
                if self.peek() != &Tok::RParen {
                    loop {
                        args.push(self.expr()?);
                        match self.peek() {
                            Tok::Comma => {
                                self.bump();
                            }
                            Tok::RParen => break,
                            _ => {
                                return Err(
                                    self.error("expected ',' or ')' in the function's arguments")
                                );
                            }
                        }
                    }
                }
                let end = self.bump().span.end;
                Ok(Ast::Call(
                    name,
                    args,
                    Span {
                        start: t.span.start,
                        end,
                    },
                ))
            }
            Tok::LParen => {
                self.bump();
                let inner = self.expr()?;
                if self.peek() != &Tok::RParen {
                    return Err(self.error("expected ')'"));
                }
                self.bump();
                Ok(inner)
            }
            Tok::RParen => Err(self.error("unexpected ')'")),
            Tok::End => Err(self.error("the expression ends too soon")),
            _ => Err(self.error("expected a number, a name or '('")),
        }
    }
}

// ----- Using the tree --------------------------------------------------------

/// The identifiers in an expression, in order of first use, without repeats.
/// Function names are not included.
pub fn identifiers(ast: &Ast) -> Vec<&str> {
    fn walk<'a>(a: &'a Ast, out: &mut Vec<&'a str>) {
        match a {
            Ast::Number(..) => {}
            Ast::Ident(name, _) => {
                if !out.contains(&name.as_str()) {
                    out.push(name);
                }
            }
            Ast::Unary(_, e, _) => walk(e, out),
            Ast::Binary(_, l, r, _) => {
                walk(l, out);
                walk(r, out);
            }
            Ast::Call(_, args, _) => args.iter().for_each(|e| walk(e, out)),
        }
    }
    let mut out = Vec::new();
    walk(ast, &mut out);
    out
}

/// A function the model defines (SBML's function definitions, Antimony's
/// `function` statements): its parameters and its body.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDef {
    pub params: Vec<String>,
    pub body: Ast,
}

/// `ast` with every call to a function in `defs` replaced by that function's
/// body, each parameter replaced by the call's argument (S §11.2). Calls in a
/// body are expanded too. Fails on a call with the wrong number of arguments,
/// and on functions that call each other without end.
pub fn expand_functions(
    ast: &Ast,
    defs: &std::collections::HashMap<String, FunctionDef>,
) -> Result<Ast, String> {
    const MAX_DEPTH: usize = 64;
    fn subst(body: &Ast, args: &std::collections::HashMap<&str, &Ast>) -> Ast {
        match body {
            Ast::Ident(name, _) => args
                .get(name.as_str())
                .map_or_else(|| body.clone(), |a| (*a).clone()),
            Ast::Number(..) => body.clone(),
            Ast::Unary(op, e, s) => Ast::Unary(*op, Box::new(subst(e, args)), *s),
            Ast::Binary(op, l, r, s) => {
                Ast::Binary(*op, Box::new(subst(l, args)), Box::new(subst(r, args)), *s)
            }
            Ast::Call(f, a, s) => {
                Ast::Call(f.clone(), a.iter().map(|e| subst(e, args)).collect(), *s)
            }
        }
    }
    fn walk(
        a: &Ast,
        defs: &std::collections::HashMap<String, FunctionDef>,
        depth: usize,
    ) -> Result<Ast, String> {
        if depth > MAX_DEPTH {
            return Err("functions that call each other without end".into());
        }
        Ok(match a {
            Ast::Number(..) | Ast::Ident(..) => a.clone(),
            Ast::Unary(op, e, s) => Ast::Unary(*op, Box::new(walk(e, defs, depth)?), *s),
            Ast::Binary(op, l, r, s) => Ast::Binary(
                *op,
                Box::new(walk(l, defs, depth)?),
                Box::new(walk(r, defs, depth)?),
                *s,
            ),
            Ast::Call(f, args, s) => {
                let args = args
                    .iter()
                    .map(|e| walk(e, defs, depth))
                    .collect::<Result<Vec<_>, _>>()?;
                match defs.get(f) {
                    None => Ast::Call(f.clone(), args, *s),
                    Some(def) if def.params.len() != args.len() => {
                        return Err(format!(
                            "the function {f}, called with {} arguments instead of {}",
                            args.len(),
                            def.params.len()
                        ));
                    }
                    Some(def) => {
                        let by_name = def
                            .params
                            .iter()
                            .map(String::as_str)
                            .zip(args.iter())
                            .collect();
                        walk(&subst(&def.body, &by_name), defs, depth + 1)?
                    }
                }
            }
        })
    }
    walk(ast, defs, 0)
}

/// Prints an expression with as few brackets as its structure needs.
/// `*`, `/` and `^` are written without spaces, other operators with them.
pub fn to_infix(ast: &Ast) -> String {
    match ast {
        Ast::Number(n, _) => n.clone(),
        Ast::Ident(name, _) => name.clone(),
        Ast::Unary(op, e, _) => {
            let inner = wrap(e, e.precedence() < UNARY_PRECEDENCE);
            match op {
                UnOp::Neg => format!("-{inner}"),
                UnOp::Not => format!("!{inner}"),
            }
        }
        Ast::Binary(op, l, r, _) => {
            let p = op.precedence();
            // Left-associative operators need brackets round a right operand
            // of the same strength; `^` groups from the right, so the other
            // way round.
            let (left_parens, right_parens) = if *op == BinOp::Pow {
                (l.precedence() <= p, r.precedence() < p)
            } else {
                (l.precedence() < p, r.precedence() <= p)
            };
            let sep = if matches!(op, BinOp::Mul | BinOp::Div | BinOp::Pow) {
                op.symbol().to_owned()
            } else {
                format!(" {} ", op.symbol())
            };
            format!("{}{sep}{}", wrap(l, left_parens), wrap(r, right_parens))
        }
        Ast::Call(name, args, _) => {
            let args: Vec<String> = args.iter().map(to_infix).collect();
            format!("{name}({})", args.join(", "))
        }
    }
}

fn wrap(a: &Ast, parens: bool) -> String {
    if parens {
        format!("({})", to_infix(a))
    } else {
        to_infix(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn infix(src: &str) -> String {
        to_infix(&parse(src).unwrap_or_else(|e| panic!("{src}: {e}")))
    }

    fn err(src: &str) -> ParseError {
        parse(src).unwrap_err()
    }

    #[test]
    fn grammar_table() {
        let cases = [
            ("k1*S1", "k1*S1"),
            ("k1 * S1 / ( Km + S1 )", "k1*S1/(Km + S1)"),
            ("a + b * c", "a + b*c"),
            ("(a + b) * c", "(a + b)*c"),
            ("a - (b - c)", "a - (b - c)"),
            ("(a - b) - c", "a - b - c"),
            ("a / (b * c)", "a/(b*c)"),
            ("a < b == c > d", "a < b == c > d"),
            ("a <= b && c >= d || e != f", "a <= b && c >= d || e != f"),
            ("a || b && c", "a || b && c"),
            ("(a || b) && c", "(a || b) && c"),
            ("sin(x) + max(a, b, 2)", "sin(x) + max(a, b, 2)"),
            ("f()", "f()"),
            ("myfun(a*b)", "myfun(a*b)"),
            ("-x", "-x"),
            ("-(a + b)", "-(a + b)"),
            ("!a", "!a"),
            ("--a", "--a"),
        ];
        for (src, out) in cases {
            assert_eq!(infix(src), out, "{src}");
        }
    }

    #[test]
    fn logical_words_and_symbols_d20() {
        assert_eq!(infix("a and b or not c"), "a && b || !c");
        assert_eq!(infix("a && b || !c"), "a && b || !c");
        let a = parse("x and y").unwrap();
        assert!(matches!(a, Ast::Binary(BinOp::And, ..)));
    }

    #[test]
    fn minus_before_a_digit_is_an_operator_d20() {
        let a = parse("k1-2").unwrap();
        assert!(matches!(&a, Ast::Binary(BinOp::Sub, l, r, _)
            if matches!(**l, Ast::Ident(ref n, _) if n == "k1")
            && matches!(**r, Ast::Number(ref n, _) if n == "2")));
        assert_eq!(infix("k1-2"), "k1 - 2");
        assert_eq!(infix("3*-2"), "3*-2");
    }

    #[test]
    fn numbers() {
        for (src, out) in [
            (".5", ".5"),
            ("0.1", "0.1"),
            ("1e-3", "1e-3"),
            ("2.5E+10", "2.5E+10"),
            ("10", "10"),
        ] {
            assert_eq!(infix(src), out);
        }
        assert!(matches!(parse(".5").unwrap(), Ast::Number(..)));
        assert_eq!(err("1e").span, Span { start: 0, end: 2 });
    }

    #[test]
    fn power_groups_from_the_right_and_unary_binds_tighter() {
        // a^b^c = a^(b^c)
        let a = parse("a^b^c").unwrap();
        assert!(matches!(&a, Ast::Binary(BinOp::Pow, l, r, _)
            if matches!(**l, Ast::Ident(..)) && matches!(**r, Ast::Binary(BinOp::Pow, ..))));
        assert_eq!(infix("a^b^c"), "a^b^c");
        assert_eq!(infix("(a^b)^c"), "(a^b)^c");
        // By the grammar, -a^2 is (-a)^2.
        assert!(matches!(
            parse("-a^2").unwrap(),
            Ast::Binary(BinOp::Pow, ..)
        ));
        assert_eq!(infix("-a^2"), "-a^2");
        assert_eq!(infix("-(a^2)"), "-(a^2)");
        assert_eq!(infix("2*a^2"), "2*a^2");
    }

    #[test]
    fn printing_then_parsing_gives_the_same_tree() {
        let strip = |a: &Ast| to_infix(a);
        for src in [
            "Vm*S/(Km + S)*E",
            "(k1*A)/(Km + (A*(1 + (M/Ki))))",
            "a - b - (c - d) + -e^-f",
            "!(a < b) && (c || d)",
            "pow(x, 2)^3 / -y",
            "((a))",
        ] {
            let once = parse(src).unwrap();
            let again = parse(&strip(&once)).unwrap();
            assert_eq!(strip(&once), strip(&again), "{src}");
        }
    }

    #[test]
    fn identifiers_in_order_without_repeats_or_functions() {
        let a = parse("Vm*S/(Km + S)*E + sin(S) + f(k, Vm)").unwrap();
        assert_eq!(identifiers(&a), ["Vm", "S", "Km", "E", "k"]);
        assert!(identifiers(&parse("2 + 3").unwrap()).is_empty());
    }

    #[test]
    fn spans_point_at_the_source() {
        let a = parse("k1 * (S1 + S2)").unwrap();
        assert_eq!(a.span(), Span { start: 0, end: 13 });
        let Ast::Binary(_, l, r, _) = a else { panic!() };
        assert_eq!(l.span(), Span { start: 0, end: 2 });
        // The bracketed sum's span is its contents.
        assert_eq!(r.span(), Span { start: 6, end: 13 });
    }

    #[test]
    fn errors_say_where() {
        assert_eq!(err("").message, "the expression is empty");
        assert_eq!(err("   ").message, "the expression is empty");
        let e = err("k1 * ");
        assert_eq!(e.span.start, 5);
        assert!(e.message.contains("ends too soon"));
        let e = err("k1 * (S1 + S2");
        assert!(e.message.contains("expected ')'"));
        assert_eq!(err("a + b)").span, Span { start: 5, end: 6 });
        let e = err("a # b");
        assert_eq!(e.span, Span { start: 2, end: 3 });
        assert!(e.message.contains('#'));
        assert!(err("f(a b)").message.contains("arguments"));
        assert!(err("a & b").message.contains('&'));
        assert!(err("a = b").message.contains('='));
        assert_eq!(err("a +* b").span.start, 3);
    }

    #[test]
    fn known_functions_list() {
        assert!(KNOWN_FUNCTIONS.contains(&"ceiling"));
        assert!(KNOWN_FUNCTIONS.contains(&"log10"));
    }

    fn defs(list: &[(&str, &[&str], &str)]) -> std::collections::HashMap<String, FunctionDef> {
        list.iter()
            .map(|(name, params, body)| {
                let def = FunctionDef {
                    params: params.iter().map(|p| p.to_string()).collect(),
                    body: parse(body).unwrap(),
                };
                (name.to_string(), def)
            })
            .collect()
    }

    fn expanded(src: &str, d: &std::collections::HashMap<String, FunctionDef>) -> String {
        to_infix(&expand_functions(&parse(src).unwrap(), d).unwrap())
    }

    #[test]
    fn calls_become_the_functions_body() {
        let d = defs(&[("mm", &["s", "v", "k"], "v*s/(k + s)")]);
        assert_eq!(expanded("mm(S1, Vm, Km)", &d), "Vm*S1/(Km + S1)");
        // Arguments that are expressions keep their meaning.
        assert_eq!(
            expanded("mm(A + B, V1*E, 2)", &d),
            "V1*E*(A + B)/(2 + (A + B))"
        );
        assert_eq!(
            expanded("2*mm(S, V, K) - k3*P", &d),
            "2*(V*S/(K + S)) - k3*P"
        );
        // Other calls, and names that aren't parameters, are left alone.
        let d = defs(&[("f", &["x"], "x^n")]);
        assert_eq!(expanded("sin(f(a - b))", &d), "sin((a - b)^n)");
        assert_eq!(expanded("g(x)", &d), "g(x)");
    }

    #[test]
    fn calls_in_bodies_and_in_arguments_are_expanded() {
        let d = defs(&[
            ("sq", &["x"], "x*x"),
            ("hill", &["s", "k"], "sq(s)/(sq(k) + sq(s))"),
        ]);
        assert_eq!(expanded("hill(S, K)", &d), "S*S/(K*K + S*S)");
        assert_eq!(expanded("sq(sq(a))", &d), "a*a*(a*a)");
        // A parameter named like another function's parameter is still the
        // right one.
        let d = defs(&[("f", &["x", "y"], "x - y")]);
        assert_eq!(expanded("f(y, x)", &d), "y - x");
    }

    #[test]
    fn bad_calls_fail() {
        let d = defs(&[("mm", &["s", "v", "k"], "v*s/(k + s)")]);
        let e = expand_functions(&parse("mm(S, V)").unwrap(), &d).unwrap_err();
        assert!(e.contains("2 arguments instead of 3"), "{e}");
        let d = defs(&[("f", &["x"], "g(x)"), ("g", &["x"], "f(x)")]);
        let e = expand_functions(&parse("f(1)").unwrap(), &d).unwrap_err();
        assert!(e.contains("without end"), "{e}");
    }
}
