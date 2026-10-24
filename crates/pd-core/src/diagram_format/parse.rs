//! Tokens to statements (docs/diagram-format.md §16).
//!
//! One statement per line; a statement goes on past a line break inside braces
//! (where the break separates properties, like `;`) or after a comma. Words are
//! not reserved (format §4): `theme`, `compartment`, `leg` and the others start
//! their statements only when the line has that statement's shape, so a species
//! may be called `theme`. The parser takes any clause on any line; which ones
//! make sense for which item is for the reader, which knows the model.

use super::ast::*;
use super::colors;
use super::lex::{Tok, Token};
use crate::geometry::{Pt, pt};

/// Something that couldn't be read. Never fatal (format §13).
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub pos: Pos,
    pub message: String,
}

fn problem(pos: Pos, message: impl Into<String>) -> Problem {
    Problem {
        pos,
        message: message.into(),
    }
}

/// The diagram the tokens hold, or `None` if they don't start with
/// `diagram <name> v<n>`; and what couldn't be read.
pub fn parse(tokens: &[Token]) -> (Option<Diagram>, Vec<Problem>) {
    let mut problems = Vec::new();
    let lines = logical_lines(tokens);
    let Some((first, rest)) = lines.split_first() else {
        return (None, problems);
    };
    let (name, version, pos) = match header(first) {
        Ok(h) => h,
        Err(p) => {
            problems.push(p);
            return (None, problems);
        }
    };
    let end = rest
        .iter()
        .position(|l| l.len() == 1 && word(&l[0]) == Some("end"));
    let body = match end {
        Some(i) => {
            if let Some(after) = rest.get(i + 1) {
                problems.push(problem(
                    after[0].pos,
                    "text after the diagram's end is ignored",
                ));
            }
            &rest[..i]
        }
        None => {
            problems.push(problem(
                pos,
                "the diagram has no end; read to the end of the comment",
            ));
            rest
        }
    };
    let mut statements = Vec::new();
    for line in body {
        let before = problems.len();
        match statement(line, &mut problems) {
            Some(s) => statements.push(s),
            // Said why already.
            None if problems.len() > before => {}
            None => problems.push(problem(line[0].pos, "this line isn't a statement; skipped")),
        }
    }
    (
        Some(Diagram {
            name,
            version,
            pos,
            statements,
        }),
        problems,
    )
}

/// The tokens of each statement: line breaks inside braces become `;`, a line
/// ending in a comma goes on, and a `;` ending a statement is dropped.
fn logical_lines(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut lines = Vec::new();
    let mut line: Vec<Token> = Vec::new();
    let mut depth = 0usize;
    let mut finish = |line: &mut Vec<Token>| {
        while matches!(line.last(), Some(Token { tok: Tok::Semi, .. })) {
            line.pop();
        }
        if !line.is_empty() {
            lines.push(std::mem::take(line));
        }
    };
    for t in tokens {
        match t.tok {
            Tok::Newline => {
                if depth > 0 {
                    line.push(Token {
                        tok: Tok::Semi,
                        pos: t.pos,
                    });
                } else if !matches!(
                    line.last(),
                    Some(Token {
                        tok: Tok::Comma,
                        ..
                    })
                ) {
                    finish(&mut line);
                }
            }
            Tok::LBrace => {
                depth += 1;
                line.push(t.clone());
            }
            Tok::RBrace => {
                depth = depth.saturating_sub(1);
                line.push(t.clone());
            }
            _ => line.push(t.clone()),
        }
    }
    finish(&mut line);
    lines
}

fn word(t: &Token) -> Option<&str> {
    match &t.tok {
        Tok::Word(w) => Some(w),
        _ => None,
    }
}

fn header(line: &[Token]) -> Result<(String, u32, Pos), Problem> {
    let pos = line[0].pos;
    let bad = || problem(pos, "a diagram starts with 'diagram <model name> v1'");
    if line.len() != 3 || word(&line[0]) != Some("diagram") {
        return Err(bad());
    }
    let name = word(&line[1]).ok_or_else(bad)?.to_owned();
    let version = word(&line[2])
        .and_then(|v| v.strip_prefix('v'))
        .and_then(|n| n.parse::<u32>().ok())
        .filter(|&n| n >= 1)
        .ok_or_else(bad)?;
    Ok((name, version, pos))
}

/// The words that start a clause (format §16).
const CLAUSE_WORDS: [&str; 17] = [
    "at", "size", "hidden", "locked", "straight", "linear", "curved", "smooth", "stem", "free",
    "uses", "source", "sink", "style", "via", "curve", "kind",
];

struct Cursor<'a> {
    toks: &'a [Token],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&'a Tok> {
        self.toks.get(self.i).map(|t| &t.tok)
    }

    fn pos(&self) -> Pos {
        self.toks
            .get(self.i)
            .or(self.toks.last())
            .map(|t| t.pos)
            .unwrap_or_default()
    }

    fn done(&self) -> bool {
        self.i >= self.toks.len()
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == Some(tok) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self) -> Option<&'a str> {
        match self.peek() {
            Some(Tok::Word(w)) => {
                self.i += 1;
                Some(w)
            }
            _ => None,
        }
    }

    fn number(&mut self) -> Option<f64> {
        match self.peek() {
            Some(Tok::Number(v)) => {
                self.i += 1;
                Some(*v)
            }
            _ => None,
        }
    }

    /// A style's name: a word, or a string for one with spaces
    /// (`"Escher metabolite"`).
    fn name(&mut self) -> Option<String> {
        match self.peek() {
            Some(Tok::Word(w)) | Some(Tok::Str(w)) => {
                self.i += 1;
                Some(w.clone())
            }
            _ => None,
        }
    }

    /// `(x, y)`
    fn point(&mut self) -> Option<Pt> {
        let start = self.i;
        let p = (|| {
            if !self.eat(&Tok::LParen) {
                return None;
            }
            let x = self.number()?;
            if !self.eat(&Tok::Comma) {
                return None;
            }
            let y = self.number()?;
            self.eat(&Tok::RParen).then_some(pt(x, y))
        })();
        if p.is_none() {
            self.i = start;
        }
        p
    }

    /// `ATP` or `ATP#2`.
    fn drawing(&mut self) -> Option<Drawing> {
        let start = self.i;
        let id = self.word()?;
        if !self.eat(&Tok::CopyMark) {
            return Some(Drawing::plain(id));
        }
        match self.number() {
            Some(n) if n >= 2.0 && n.fract() == 0.0 && n <= f64::from(u32::MAX) => Some(Drawing {
                id: id.to_owned(),
                copy: n as u32,
            }),
            _ => {
                self.i = start;
                None
            }
        }
    }

    /// `w x h` or one number.
    fn size(&mut self) -> Option<Size> {
        let w = self.number()?;
        if matches!(self.peek(), Some(Tok::Word(x)) if x == "x") {
            self.i += 1;
            let h = self.number()?;
            return Some(Size { w, h });
        }
        Some(Size { w, h: w })
    }

    fn color(&mut self) -> Option<Color> {
        match self.peek()? {
            Tok::Hex(h) => {
                let c = colors::parse_hex(h)?;
                self.i += 1;
                Some(Color::Value(c))
            }
            Tok::Word(w) => {
                self.i += 1;
                Some(if w == "none" {
                    Color::None
                } else {
                    Color::Name(w.clone())
                })
            }
            _ => None,
        }
    }
}

fn statement(line: &[Token], problems: &mut Vec<Problem>) -> Option<Statement> {
    let pos = line[0].pos;
    let mut c = Cursor { toks: line, i: 0 };
    let first = word(&line[0]);
    let kind = (|| {
        let shaped = |kind| Some(kind);
        match first {
            Some("theme") if line.len() == 2 => {
                c.i = 1;
                return shaped(StatementKind::Theme(c.word()?.to_owned()));
            }
            Some("background") if line.len() == 2 => {
                c.i = 1;
                if let Some(col) = c.color() {
                    return shaped(StatementKind::Background(col));
                }
                problems.push(problem(line[1].pos, "the background's color doesn't read"));
                return None;
            }
            Some("modifierLines") if line.len() == 2 => match word(&line[1]) {
                Some("hidden") => return shaped(StatementKind::ModifierLinesHidden(true)),
                Some("shown") => return shaped(StatementKind::ModifierLinesHidden(false)),
                _ => {}
            },
            Some("color") if line.len() == 4 && line[2].tok == Tok::Equals => {
                c.i = 1;
                let name = c.word()?.to_owned();
                c.i = 3;
                if let Some(value) = c.color() {
                    return shaped(StatementKind::ColorDef { name, value });
                }
                problems.push(problem(
                    line[3].pos,
                    format!("the color of '{name}' doesn't read"),
                ));
                return None;
            }
            Some("style")
                if name_at(line, 1)
                    && (tok_at(line, 2) == Some(&Tok::LBrace)
                        || (word_at(line, 2).is_some()
                            && tok_at(line, 3) == Some(&Tok::LBrace))) =>
            {
                c.i = 1;
                let name = c.name()?;
                let kind = c.word().map(str::to_owned);
                let props = block(&mut c, problems);
                trailing(&c, problems);
                return shaped(StatementKind::Style { name, kind, props });
            }
            Some("compartment")
                if word_at(line, 1).is_some() && tok_at(line, 2) != Some(&Tok::LParen) =>
            {
                c.i = 1;
                let id = c.word()?.to_owned();
                let (clauses, props) = clauses_and_block(&mut c, problems);
                return shaped(StatementKind::Compartment { id, clauses, props });
            }
            Some("text") if matches!(tok_at(line, 1), Some(Tok::Str(_))) => {
                let Some(Tok::Str(text)) = tok_at(line, 1) else {
                    unreachable!()
                };
                c.i = 2;
                let (clauses, props) = clauses_and_block(&mut c, problems);
                return shaped(StatementKind::Text {
                    text: text.clone(),
                    clauses,
                    props,
                });
            }
            Some("leg") => {
                c.i = 1;
                if let Some(from) = c.drawing()
                    && c.eat(&Tok::Arrow)
                    && let Some(to) = c.drawing()
                {
                    let (clauses, props) = clauses_and_block(&mut c, problems);
                    return shaped(StatementKind::Leg {
                        from,
                        to,
                        clauses,
                        props,
                    });
                }
                c.i = 0;
            }
            Some("line") => {
                c.i = 1;
                if let Some(modifier) = c.drawing()
                    && c.word() == Some("to")
                    && let Some(reaction) = c.word()
                {
                    let reaction = reaction.to_owned();
                    let (clauses, props) = clauses_and_block(&mut c, problems);
                    return shaped(StatementKind::Line {
                        modifier,
                        reaction,
                        clauses,
                        props,
                    });
                }
                c.i = 0;
            }
            _ => {}
        }
        c.i = 0;
        let drawing = c.drawing()?;
        let (clauses, props) = clauses_and_block(&mut c, problems);
        Some(StatementKind::Item {
            drawing,
            clauses,
            props,
        })
    })()?;
    Some(Statement { pos, kind })
}

fn word_at(line: &[Token], i: usize) -> Option<&str> {
    line.get(i).and_then(word)
}

/// A style's name, a word or a string, at `i`.
fn name_at(line: &[Token], i: usize) -> bool {
    matches!(tok_at(line, i), Some(Tok::Word(_)) | Some(Tok::Str(_)))
}

fn tok_at(line: &[Token], i: usize) -> Option<&Tok> {
    line.get(i).map(|t| &t.tok)
}

/// Clauses to the end of the line or a block, then the block.
fn clauses_and_block(c: &mut Cursor, problems: &mut Vec<Problem>) -> (Vec<Clause>, Vec<Property>) {
    let mut clauses = Vec::new();
    while !c.done() && c.peek() != Some(&Tok::LBrace) {
        let pos = c.pos();
        match clause(c) {
            Ok(kind) => clauses.push(Clause { pos, kind }),
            Err(message) => {
                problems.push(problem(pos, message));
                skip_to_clause(c);
            }
        }
    }
    let props = if c.peek() == Some(&Tok::LBrace) {
        block(c, problems)
    } else {
        Vec::new()
    };
    trailing(c, problems);
    (clauses, props)
}

/// Anything after the block: kept as an unknown part would be, but a block
/// should end the line.
fn trailing(c: &Cursor, problems: &mut Vec<Problem>) {
    if !c.done() {
        problems.push(problem(c.pos(), "text after the braces is ignored"));
    }
}

fn skip_to_clause(c: &mut Cursor) {
    c.i += 1;
    while !c.done()
        && c.peek() != Some(&Tok::LBrace)
        && !matches!(c.peek(), Some(Tok::Word(w)) if CLAUSE_WORDS.contains(&w.as_str()))
    {
        c.i += 1;
    }
}

fn clause(c: &mut Cursor) -> Result<ClauseKind, String> {
    let start = c.i;
    let Some(Tok::Word(w)) = c.peek() else {
        return Ok(unknown(c, start));
    };
    let w = w.as_str();
    if !CLAUSE_WORDS.contains(&w) {
        return Ok(unknown(c, start));
    }
    c.i += 1;
    let point = |c: &mut Cursor| {
        c.point()
            .ok_or_else(|| format!("'{w}' needs a point (x, y)"))
    };
    Ok(match w {
        "at" => ClauseKind::At(point(c)?),
        "size" => ClauseKind::Size(
            c.size()
                .ok_or("'size' needs a size: 80 x 36, or one number")?,
        ),
        "hidden" => ClauseKind::Hidden,
        "locked" => ClauseKind::Locked,
        "straight" => ClauseKind::Shape(Shape::Straight),
        "linear" => ClauseKind::Shape(Shape::Linear),
        "curved" => ClauseKind::Shape(Shape::Curved),
        "smooth" => {
            let off = matches!(c.peek(), Some(Tok::Word(w)) if w == "off");
            if off {
                c.i += 1;
            }
            ClauseKind::Smooth(!off)
        }
        "free" => ClauseKind::Free,
        "stem" => {
            let a = point(c)?;
            ClauseKind::Stem(a, point(c)?)
        }
        "curve" => {
            let a = point(c)?;
            ClauseKind::Curve(a, point(c)?)
        }
        "source" => ClauseKind::Source(point(c)?),
        "sink" => ClauseKind::Sink(point(c)?),
        "via" => {
            let mut pts = vec![point(c)?];
            while let Some(p) = c.point() {
                pts.push(p);
            }
            ClauseKind::Via(pts)
        }
        "uses" => {
            let mut list = vec![c.drawing().ok_or("'uses' needs drawings: ATP#2, ADP#2")?];
            while c.eat(&Tok::Comma) {
                list.push(
                    c.drawing()
                        .ok_or("'uses' needs a drawing after the comma")?,
                );
            }
            ClauseKind::Uses(list)
        }
        "style" => {
            let mut list = vec![c.name().ok_or("'style' needs a style's name")?];
            while c.eat(&Tok::Comma) {
                list.push(c.name().ok_or("'style' needs a name after the comma")?);
            }
            ClauseKind::Styles(list)
        }
        "kind" => ClauseKind::Kind(
            c.word()
                .ok_or("'kind' needs a kind: activator, …")?
                .to_owned(),
        ),
        _ => unreachable!("every clause word is handled"),
    })
}

/// A part this version doesn't know: its first token and what follows up to
/// the next clause word or the block, as written.
fn unknown(c: &mut Cursor, start: usize) -> ClauseKind {
    skip_to_clause(c);
    ClauseKind::Unknown(render(&c.toks[start..c.i]))
}

/// Tokens back to text, for unknown parts kept as written.
fn render(toks: &[Token]) -> String {
    let mut s = String::new();
    for (i, t) in toks.iter().enumerate() {
        let joins = matches!(
            (&t.tok, toks.get(i.wrapping_sub(1)).map(|p| &p.tok)),
            (Tok::RParen | Tok::Comma | Tok::CopyMark, _) | (_, Some(Tok::LParen | Tok::CopyMark))
        );
        if i > 0 && !joins {
            s.push(' ');
        }
        match &t.tok {
            Tok::Word(w) => s.push_str(w),
            Tok::Number(v) => s.push_str(&v.to_string()),
            Tok::Str(v) => {
                s.push('"');
                s.push_str(v);
                s.push('"');
            }
            Tok::Hex(h) => s.push_str(h),
            Tok::CopyMark => s.push('#'),
            Tok::LParen => s.push('('),
            Tok::RParen => s.push(')'),
            Tok::LBrace => s.push('{'),
            Tok::RBrace => s.push('}'),
            Tok::Comma => s.push(','),
            Tok::Semi => s.push(';'),
            Tok::Equals => s.push('='),
            Tok::Arrow => s.push_str("->"),
            Tok::Newline => {}
        }
    }
    s
}

/// `{ name values; … }`; the cursor is on `{`.
fn block(c: &mut Cursor, problems: &mut Vec<Problem>) -> Vec<Property> {
    let open = c.pos();
    let mut props = Vec::new();
    if !c.eat(&Tok::LBrace) {
        problems.push(problem(open, "expected '{'"));
        return props;
    }
    loop {
        while c.eat(&Tok::Semi) {}
        match c.peek() {
            None => {
                problems.push(problem(open, "the '{' has no '}'"));
                return props;
            }
            Some(Tok::RBrace) => {
                c.i += 1;
                return props;
            }
            Some(Tok::Word(name)) => {
                let pos = c.pos();
                c.i += 1;
                let mut values = Vec::new();
                let mut bad = false;
                while !matches!(c.peek(), None | Some(Tok::Semi) | Some(Tok::RBrace)) {
                    match value(c) {
                        Some(v) => values.push(v),
                        None => {
                            bad = true;
                            c.i += 1;
                        }
                    }
                }
                if bad {
                    problems.push(problem(
                        pos,
                        format!("a value of '{name}' doesn't read; skipped"),
                    ));
                } else {
                    props.push(Property {
                        pos,
                        name: name.clone(),
                        values,
                    });
                }
            }
            Some(_) => {
                problems.push(problem(c.pos(), "a property starts with its name"));
                while !matches!(c.peek(), None | Some(Tok::Semi) | Some(Tok::RBrace)) {
                    c.i += 1;
                }
            }
        }
    }
}

fn value(c: &mut Cursor) -> Option<Value> {
    if let Some(p) = c.point() {
        return Some(Value::Point(p));
    }
    let v = match c.peek()? {
        Tok::Number(v) => Value::Number(*v),
        Tok::Word(w) => Value::Word(w.clone()),
        Tok::Str(s) => Value::Str(s.clone()),
        Tok::Hex(h) => Value::Hex(colors::parse_hex(h)?),
        _ => return None,
    };
    c.i += 1;
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::super::lex::lex;
    use super::*;
    use crate::model::Rgba;

    fn read(text: &str) -> (Option<Diagram>, Vec<Problem>) {
        let (t, e) = lex(text, Pos { line: 1, column: 1 });
        assert!(e.is_empty(), "{e:?}");
        parse(&t)
    }

    /// The statements of a diagram with `body` in it, which must read cleanly.
    fn body(body: &str) -> Vec<StatementKind> {
        let (d, p) = read(&format!("diagram m v1\n{body}\nend\n"));
        assert!(p.is_empty(), "{p:?}");
        d.unwrap().statements.into_iter().map(|s| s.kind).collect()
    }

    fn one(text: &str) -> StatementKind {
        let mut s = body(text);
        assert_eq!(s.len(), 1, "{s:?}");
        s.remove(0)
    }

    fn clauses(k: &StatementKind) -> Vec<ClauseKind> {
        match k {
            StatementKind::Item { clauses, .. }
            | StatementKind::Compartment { clauses, .. }
            | StatementKind::Text { clauses, .. }
            | StatementKind::Leg { clauses, .. }
            | StatementKind::Line { clauses, .. } => {
                clauses.iter().map(|c| c.kind.clone()).collect()
            }
            _ => panic!("{k:?}"),
        }
    }

    fn props(k: &StatementKind) -> Vec<(String, Vec<Value>)> {
        match k {
            StatementKind::Item { props, .. }
            | StatementKind::Compartment { props, .. }
            | StatementKind::Text { props, .. }
            | StatementKind::Leg { props, .. }
            | StatementKind::Line { props, .. }
            | StatementKind::Style { props, .. } => props
                .iter()
                .map(|p| (p.name.clone(), p.values.clone()))
                .collect(),
            _ => panic!("{k:?}"),
        }
    }

    fn word(s: &str) -> Value {
        Value::Word(s.into())
    }

    #[test]
    fn the_header_and_the_end() {
        let (d, p) = read("diagram upper_glycolysis v1\nend");
        assert!(p.is_empty());
        let d = d.unwrap();
        assert_eq!(
            (d.name.as_str(), d.version, d.statements.len()),
            ("upper_glycolysis", 1, 0)
        );
        for bad in [
            "diagram v1\nend",
            "diagram m 1\nend",
            "diagram m v0\nend",
            "model m\nend",
        ] {
            let (d, p) = read(bad);
            assert!(d.is_none() && p.len() == 1, "{bad}");
        }
        // No end, or text after it: read, with a note.
        let (d, p) = read("diagram m v1\nA at (1, 2)");
        assert_eq!(d.unwrap().statements.len(), 1);
        assert!(p[0].message.contains("no end"));
        let (_, p) = read("diagram m v1\nend\nB at (1, 2)");
        assert_eq!(p[0].pos.line, 3);
    }

    #[test]
    fn diagram_level_statements() {
        assert_eq!(
            body(
                "theme escher\nbackground white\nbackground #FFFFFF\nmodifierLines hidden\nmodifierLines shown\ncolor metab = #FFD54F\ncolor clear = none"
            ),
            [
                StatementKind::Theme("escher".into()),
                StatementKind::Background(Color::Name("white".into())),
                StatementKind::Background(Color::Value(Rgba::rgb(255, 255, 255))),
                StatementKind::ModifierLinesHidden(true),
                StatementKind::ModifierLinesHidden(false),
                StatementKind::ColorDef {
                    name: "metab".into(),
                    value: Color::Value(Rgba::rgb(0xFF, 0xD5, 0x4F))
                },
                StatementKind::ColorDef {
                    name: "clear".into(),
                    value: Color::None
                },
            ]
        );
    }

    #[test]
    fn styles_and_their_properties() {
        let s = one(
            "style metabolite { shape circle; size 24; fill #FFD54F; border metabEdge 1.5; id offset (0, -28); bold }",
        );
        let StatementKind::Style { name, kind, .. } = &s else {
            panic!()
        };
        assert_eq!((name.as_str(), kind), ("metabolite", &None));
        assert_eq!(
            props(&s),
            [
                ("shape".into(), vec![word("circle")]),
                ("size".into(), vec![Value::Number(24.0)]),
                ("fill".into(), vec![Value::Hex(Rgba::rgb(0xFF, 0xD5, 0x4F))]),
                ("border".into(), vec![word("metabEdge"), Value::Number(1.5)]),
                (
                    "id".into(),
                    vec![word("offset"), Value::Point(pt(0.0, -28.0))]
                ),
                ("bold".into(), vec![]),
            ]
        );
        let StatementKind::Style { kind, .. } = one("style flux reaction { line #37474F 2.5 }")
        else {
            panic!()
        };
        assert_eq!(kind.as_deref(), Some("reaction"));
    }

    #[test]
    fn a_block_may_span_lines_with_or_without_semicolons() {
        let s = one("style metabolite { shape circle; size 24;\n    fill #FFD54F\n    id right }");
        assert_eq!(props(&s).len(), 4);
    }

    #[test]
    fn species_drawings_and_their_clauses() {
        let s = one("ATP#2 at (320, 400) locked style cofactor, highlight { id above }");
        let StatementKind::Item { drawing, .. } = &s else {
            panic!()
        };
        assert_eq!(
            drawing,
            &Drawing {
                id: "ATP".into(),
                copy: 2
            }
        );
        assert_eq!(
            clauses(&s),
            [
                ClauseKind::At(pt(320.0, 400.0)),
                ClauseKind::Locked,
                ClauseKind::Styles(vec!["cofactor".into(), "highlight".into()]),
            ]
        );
        assert_eq!(props(&s), [("id".into(), vec![word("above")])]);
        // No position: still a statement.
        assert!(matches!(
            one("X style metabolite"),
            StatementKind::Item { .. }
        ));
    }

    #[test]
    fn reactions_and_their_clauses() {
        let s = one(
            "pfk at (200, 420) curved smooth stem (-40, 0) (40, 0) free uses ATP#2, ADP#2 source (1, 2) sink (3, 4) style flux { id shown }",
        );
        assert_eq!(
            clauses(&s),
            [
                ClauseKind::At(pt(200.0, 420.0)),
                ClauseKind::Shape(Shape::Curved),
                ClauseKind::Smooth(true),
                ClauseKind::Stem(pt(-40.0, 0.0), pt(40.0, 0.0)),
                ClauseKind::Free,
                ClauseKind::Uses(vec![
                    Drawing {
                        id: "ATP".into(),
                        copy: 2
                    },
                    Drawing {
                        id: "ADP".into(),
                        copy: 2
                    }
                ]),
                ClauseKind::Source(pt(1.0, 2.0)),
                ClauseKind::Sink(pt(3.0, 4.0)),
                ClauseKind::Styles(vec!["flux".into()]),
            ]
        );
    }

    #[test]
    fn compartments_texts_legs_and_lines() {
        let s = one("compartment cytosol at (60, 40) size 520 x 600 hidden");
        assert!(matches!(&s, StatementKind::Compartment { id, .. } if id == "cytosol"));
        assert_eq!(
            clauses(&s),
            [
                ClauseKind::At(pt(60.0, 40.0)),
                ClauseKind::Size(Size { w: 520.0, h: 600.0 }),
                ClauseKind::Hidden
            ]
        );
        let s = one("text \"Upper glycolysis\" at (80, 72) { size 22; bold }");
        assert!(matches!(&s, StatementKind::Text { text, .. } if text == "Upper glycolysis"));
        let s = one("leg ATP#2 -> pfk via (1, 2) (3, 4) curve (5, 6) (7, 8)");
        let StatementKind::Leg { from, to, .. } = &s else {
            panic!()
        };
        assert_eq!((from.copy, to.id.as_str()), (2, "pfk"));
        assert_eq!(
            clauses(&s),
            [
                ClauseKind::Via(vec![pt(1.0, 2.0), pt(3.0, 4.0)]),
                ClauseKind::Curve(pt(5.0, 6.0), pt(7.0, 8.0))
            ]
        );
        let s = one(
            "line MAPK_PP to J1 via (1160, 650) (360, 116) kind inhibitor {\n  line red 3 solid; symbol 30 }",
        );
        let StatementKind::Line {
            modifier, reaction, ..
        } = &s
        else {
            panic!()
        };
        assert_eq!((modifier.id.as_str(), reaction.as_str()), ("MAPK_PP", "J1"));
        assert_eq!(clauses(&s)[1], ClauseKind::Kind("inhibitor".into()));
        assert_eq!(
            props(&s)[0],
            (
                "line".into(),
                vec![word("red"), Value::Number(3.0), word("solid")]
            )
        );
    }

    #[test]
    fn statement_words_are_not_reserved() {
        // Species called theme, compartment, leg and line.
        for text in [
            "theme at (1, 2)",
            "compartment at (1, 2)",
            "leg at (1, 2)",
            "line at (1, 2)",
            "style at (1, 2)",
            "color at (1, 2)",
        ] {
            let s = one(text);
            assert!(matches!(&s, StatementKind::Item { .. }), "{text}: {s:?}");
            assert_eq!(clauses(&s), [ClauseKind::At(pt(1.0, 2.0))], "{text}");
        }
        // A style may be called by a clause word.
        assert_eq!(
            clauses(&one("J1 style curved")),
            [ClauseKind::Styles(vec!["curved".into()])]
        );
    }

    #[test]
    fn a_statement_goes_on_after_a_comma() {
        let s = one("pfk uses ATP#2,\n     ADP#2");
        assert!(matches!(&clauses(&s)[0], ClauseKind::Uses(l) if l.len() == 2));
    }

    #[test]
    fn unknown_parts_are_kept_as_written_and_the_rest_is_read() {
        // A later version's words (format §15, item 9).
        let s = one("J4 linear curve (368, 363) (423, 344) { line blue 2 }");
        assert_eq!(
            clauses(&s),
            [
                ClauseKind::Shape(Shape::Linear),
                ClauseKind::Curve(pt(368.0, 363.0), pt(423.0, 344.0))
            ]
        );
        let s = one("J1 at (1, 2) glow (3, 4) bright curved");
        assert_eq!(
            clauses(&s),
            [
                ClauseKind::At(pt(1.0, 2.0)),
                ClauseKind::Unknown("glow (3, 4) bright".into()),
                ClauseKind::Shape(Shape::Curved)
            ]
        );
        // Properties are words and values: any name reads.
        assert_eq!(
            props(&one("J1 { arrowSize 14; arrowColor red }")),
            [
                ("arrowSize".into(), vec![Value::Number(14.0)]),
                ("arrowColor".into(), vec![word("red")])
            ]
        );
    }

    #[test]
    fn what_doesnt_read_is_skipped_with_its_place() {
        let (d, p) = read("diagram m v1\nA at (1, two) locked\nB { fill #12; size 3 }\nend");
        let st: Vec<_> = d.unwrap().statements.into_iter().map(|s| s.kind).collect();
        // A's position is skipped, its lock kept.
        assert_eq!(clauses(&st[0]), [ClauseKind::Locked]);
        assert_eq!((p[0].pos.line, p[0].pos.column), (2, 3));
        // B's fill is skipped, its size kept.
        assert_eq!(props(&st[1]), [("size".into(), vec![Value::Number(3.0)])]);
        assert_eq!(p[1].pos.line, 3);
        assert_eq!(p.len(), 2, "{p:?}");
        // A bad color or an unclosed block.
        let (_, p) = read("diagram m v1\nbackground #GG\nC { fill red\nend");
        assert_eq!(p.len(), 3, "{p:?}");
    }

    #[test]
    fn drawings_are_numbered_from_two() {
        for bad in ["A#1 at (1, 2)", "A#0", "A#2.5"] {
            let (d, p) = read(&format!("diagram m v1\n{bad}\nend"));
            assert!(d.unwrap().statements.is_empty(), "{bad}");
            assert_eq!(p.len(), 1, "{bad}: {p:?}");
        }
    }

    #[test]
    fn a_semicolon_may_end_a_statement() {
        let s = body("A at (1, 2);\nB;;");
        assert_eq!(s.len(), 2);
        assert_eq!(clauses(&s[0]), [ClauseKind::At(pt(1.0, 2.0))]);
        assert_eq!(clauses(&s[1]), []);
    }
}
