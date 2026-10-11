//! Rate laws and other expressions between `pd-core::expr` and `sbml-io`'s
//! MathML tree (S §11.5).

use std::collections::HashMap;

use sbml_io::{Constant, Math, Number, Op, Symbol};

use crate::expr::{Ast, BinOp, Span, UnOp};

const NO_SPAN: Span = Span { start: 0, end: 0 };

/// An expression as MathML (the table in S §11.5). `level` and `version` say
/// which operators are allowed: `max` and `min` exist only from Level 3
/// Version 2, and are written as a `piecewise` before that.
pub fn to_math(ast: &Ast, level: u32, version: u32) -> Math {
    let me = |a: &Ast| to_math(a, level, version);
    match ast {
        Ast::Number(text, _) => number(text),
        Ast::Ident(name, _) if name == "time" => Math::Symbol(Symbol::new(Symbol::TIME, "time")),
        Ast::Ident(name, _) => Math::Ident(name.clone()),
        Ast::Unary(UnOp::Neg, e, _) => Math::Apply(Op::Minus, vec![me(e)]),
        Ast::Unary(UnOp::Not, e, _) => Math::Apply(Op::Not, vec![me(e)]),
        Ast::Binary(op, l, r, _) => {
            let op = match op {
                BinOp::Or => Op::Or,
                BinOp::And => Op::And,
                BinOp::Eq => Op::Eq,
                BinOp::Ne => Op::Neq,
                BinOp::Lt => Op::Lt,
                BinOp::Le => Op::Leq,
                BinOp::Gt => Op::Gt,
                BinOp::Ge => Op::Geq,
                BinOp::Add => Op::Plus,
                BinOp::Sub => Op::Minus,
                BinOp::Mul => Op::Times,
                BinOp::Div => Op::Divide,
                BinOp::Pow => Op::Power,
            };
            Math::Apply(op, vec![me(l), me(r)])
        }
        Ast::Call(name, args, _) => {
            let args: Vec<Math> = args.iter().map(me).collect();
            call(name, args, level, version)
        }
    }
}

fn number(text: &str) -> Math {
    let value = match text.parse::<i64>() {
        Ok(i) => Number::Integer(i),
        Err(_) => Number::Real(text.parse().unwrap_or(f64::NAN)),
    };
    Math::Number { value, units: None }
}

fn call(name: &str, mut args: Vec<Math>, level: u32, version: u32) -> Math {
    let one = |args: &mut Vec<Math>| (args.len() == 1).then(|| Box::new(args.remove(0)));
    let op = match name {
        "asin" => Some(Op::ArcSin),
        "acos" => Some(Op::ArcCos),
        "atan" => Some(Op::ArcTan),
        "ceil" | "ceiling" => Some(Op::Ceiling),
        "pow" => Some(Op::Power),
        "max" | "min" if level >= 3 && version >= 2 => Op::from_tag(name),
        "max" | "min" if !args.is_empty() => return min_max(name == "max", args),
        "log" | "log10" => {
            if let Some(arg) = one(&mut args) {
                return Math::Log {
                    base: Some(Box::new(Math::int(10))),
                    arg,
                };
            }
            None
        }
        "sqrt" => {
            if let Some(arg) = one(&mut args) {
                return Math::Root {
                    degree: Some(Box::new(Math::int(2))),
                    arg,
                };
            }
            None
        }
        // Other built-in functions share their MathML name; `abs`, `exp`,
        // `ln`, `floor`, the trigonometric and hyperbolic functions.
        _ => Op::from_tag(name).filter(|op| !matches!(op, Op::Plus | Op::Minus | Op::Times)),
    };
    match op {
        Some(op) => Math::Apply(op, args),
        None => Math::Call(name.to_owned(), args),
    }
}

/// `max(a, b, …)` or `min(a, b, …)` as nested `piecewise`, for SBML before
/// Level 3 Version 2.
fn min_max(max: bool, mut args: Vec<Math>) -> Math {
    let mut acc = args.remove(0);
    for b in args {
        let cmp = if max { Op::Geq } else { Op::Leq };
        acc = Math::Piecewise {
            pieces: vec![(acc.clone(), Math::Apply(cmp, vec![acc, b.clone()]))],
            otherwise: Some(Box::new(b)),
        };
    }
    acc
}

/// MathML as an expression. `rename` maps ids to the names they get in the
/// model (local parameters made global). Fails, with the reason, on MathML
/// the expression language can't say.
pub fn from_math(m: &Math, rename: &HashMap<String, String>) -> Result<Ast, String> {
    let me = |m: &Math| from_math(m, rename);
    let all = |args: &[Math]| args.iter().map(me).collect::<Result<Vec<_>, _>>();
    Ok(match m {
        Math::Number { value, .. } => num_ast(value.value()),
        Math::Ident(id) => ident(rename.get(id).unwrap_or(id)),
        Math::Symbol(s) if s.url == Symbol::TIME => ident("time"),
        Math::Symbol(s) if s.url == Symbol::AVOGADRO => num_ast(6.02214076e23),
        Math::Symbol(s) => return Err(format!("the symbol {}", s.url)),
        Math::Constant(c) => match c {
            Constant::True => num_ast(1.0),
            Constant::False => num_ast(0.0),
            Constant::Pi => num_ast(std::f64::consts::PI),
            Constant::ExponentialE => Ast::Call("exp".into(), vec![num_ast(1.0)], NO_SPAN),
            Constant::NotANumber | Constant::Infinity => {
                return Err(format!("<{}/>", c.tag()));
            }
        },
        Math::Apply(op, args) => {
            let a = all(args)?;
            apply(*op, a)?
        }
        Math::Call(f, args) => Ast::Call(f.clone(), all(args)?, NO_SPAN),
        Math::CallSymbol(s, args) => {
            let name = match s.url.as_str() {
                Symbol::DELAY => "delay",
                Symbol::RATE_OF => "rate",
                _ => return Err(format!("the symbol {}", s.url)),
            };
            Ast::Call(name.into(), all(args)?, NO_SPAN)
        }
        Math::Root { degree, arg } => {
            let arg = me(arg)?;
            match degree.as_deref().map(me).transpose()? {
                None => Ast::Call("sqrt".into(), vec![arg], NO_SPAN),
                Some(Ast::Number(n, _)) if n == "2" => Ast::Call("sqrt".into(), vec![arg], NO_SPAN),
                Some(d) => binary(BinOp::Pow, arg, binary(BinOp::Div, num_ast(1.0), d)),
            }
        }
        Math::Log { base, arg } => {
            let arg = me(arg)?;
            // `log` is base 10 here, as in MathML (S §11.5).
            match base.as_deref().map(me).transpose()? {
                None => Ast::Call("log10".into(), vec![arg], NO_SPAN),
                Some(Ast::Number(n, _)) if n == "10" => {
                    Ast::Call("log10".into(), vec![arg], NO_SPAN)
                }
                Some(b) => binary(
                    BinOp::Div,
                    Ast::Call("ln".into(), vec![arg], NO_SPAN),
                    Ast::Call("ln".into(), vec![b], NO_SPAN),
                ),
            }
        }
        Math::Piecewise { pieces, otherwise } => {
            let mut args = Vec::new();
            for (v, c) in pieces {
                args.push(me(v)?);
                args.push(me(c)?);
            }
            if let Some(o) = otherwise {
                args.push(me(o)?);
            }
            Ast::Call("piecewise".into(), args, NO_SPAN)
        }
        Math::Lambda { .. } => return Err("a lambda outside a function definition".into()),
        Math::Semantics { math, .. } => me(math)?,
        Math::Raw(_) => return Err("MathML outside the subset SBML allows".into()),
    })
}

fn apply(op: Op, mut a: Vec<Ast>) -> Result<Ast, String> {
    let fold = |op: BinOp, a: Vec<Ast>, empty: f64| {
        a.into_iter()
            .reduce(|l, r| binary(op, l, r))
            .unwrap_or_else(|| num_ast(empty))
    };
    Ok(match (op, a.len()) {
        (Op::Plus, _) => fold(BinOp::Add, a, 0.0),
        (Op::Times, _) => fold(BinOp::Mul, a, 1.0),
        (Op::And, _) => fold(BinOp::And, a, 1.0),
        (Op::Or, _) => fold(BinOp::Or, a, 0.0),
        (Op::Minus, 1) => Ast::Unary(UnOp::Neg, Box::new(a.remove(0)), NO_SPAN),
        (Op::Not, 1) => Ast::Unary(UnOp::Not, Box::new(a.remove(0)), NO_SPAN),
        (Op::Minus | Op::Divide | Op::Power, 2) => {
            let r = a.pop().unwrap();
            let l = a.pop().unwrap();
            let op = match op {
                Op::Minus => BinOp::Sub,
                Op::Divide => BinOp::Div,
                _ => BinOp::Pow,
            };
            binary(op, l, r)
        }
        (Op::Eq | Op::Neq | Op::Lt | Op::Leq | Op::Gt | Op::Geq, n) if n >= 2 => {
            let op = match op {
                Op::Eq => BinOp::Eq,
                Op::Neq => BinOp::Ne,
                Op::Lt => BinOp::Lt,
                Op::Leq => BinOp::Le,
                Op::Gt => BinOp::Gt,
                _ => BinOp::Ge,
            };
            // `a < b < c` means `a < b && b < c`.
            let pairs: Vec<Ast> = a
                .windows(2)
                .map(|w| binary(op, w[0].clone(), w[1].clone()))
                .collect();
            fold(BinOp::And, pairs, 1.0)
        }
        (Op::ArcSin, _) => Ast::Call("asin".into(), a, NO_SPAN),
        (Op::ArcCos, _) => Ast::Call("acos".into(), a, NO_SPAN),
        (Op::ArcTan, _) => Ast::Call("atan".into(), a, NO_SPAN),
        (Op::Ceiling, _) => Ast::Call("ceil".into(), a, NO_SPAN),
        (op, _) if !matches!(op, Op::Minus | Op::Divide | Op::Power | Op::Not) => {
            Ast::Call(op.tag().into(), a, NO_SPAN)
        }
        (op, n) => return Err(format!("<{}/> with {n} arguments", op.tag())),
    })
}

fn binary(op: BinOp, l: Ast, r: Ast) -> Ast {
    Ast::Binary(op, Box::new(l), Box::new(r), NO_SPAN)
}

fn ident(name: &str) -> Ast {
    Ast::Ident(name.to_owned(), NO_SPAN)
}

/// A number as the expression language writes it: negative numbers as a
/// minus applied to the number, since a literal has no sign.
fn num_ast(v: f64) -> Ast {
    let text = sbml_io::fmt_f64(v.abs());
    let n = Ast::Number(text, NO_SPAN);
    if v < 0.0 {
        Ast::Unary(UnOp::Neg, Box::new(n), NO_SPAN)
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{parse, to_infix};

    /// Infix → MathML → infix.
    fn round(src: &str, level: u32, version: u32) -> String {
        let math = to_math(&parse(src).unwrap(), level, version);
        to_infix(&from_math(&math, &HashMap::new()).unwrap())
    }

    #[test]
    fn the_export_table_round_trips() {
        let cases = [
            ("k1*S1/(Km + S1)", "k1*S1/(Km + S1)"),
            ("a - b - c", "a - b - c"),
            ("a - (b - c)", "a - (b - c)"),
            // Unary minus binds tighter than ^ here, so this is (-x)^2.
            ("-x^2", "-x^2"),
            ("a^b^c", "a^b^c"),
            ("a < b && c >= d || !e", "a < b && c >= d || !e"),
            ("a == b != c", "a == b != c"),
            ("asin(x) + acos(x) + atan(x)", "asin(x) + acos(x) + atan(x)"),
            ("log(x) + log10(y)", "log10(x) + log10(y)"),
            ("sqrt(x)", "sqrt(x)"),
            ("ceil(x) + ceiling(y)", "ceil(x) + ceil(y)"),
            ("pow(a, b)", "a^b"),
            (
                "exp(x) + ln(x) + abs(x) + floor(x) + sinh(x)",
                "exp(x) + ln(x) + abs(x) + floor(x) + sinh(x)",
            ),
            ("myfun(a, 2)", "myfun(a, 2)"),
            ("k*time", "k*time"),
            ("1e-20 + 0.5 + 7", "1e-20 + 0.5 + 7"),
        ];
        for (src, out) in cases {
            assert_eq!(round(src, 3, 1), out, "{src}");
        }
    }

    #[test]
    fn min_and_max() {
        // Level 3 Version 2 has them; before that they become piecewise.
        assert_eq!(round("max(a, b)", 3, 2), "max(a, b)");
        assert_eq!(round("min(a, b)", 3, 1), "piecewise(a, a <= b, b)");
        assert!(matches!(
            to_math(&parse("max(a, b, c)").unwrap(), 3, 1),
            Math::Piecewise { .. }
        ));
    }

    #[test]
    fn mathml_only_forms() {
        let none = HashMap::new();
        let infix = |m: Math| to_infix(&from_math(&m, &none).unwrap());
        // n-ary plus, unary minus, a negative number.
        assert_eq!(
            infix(Math::Apply(
                Op::Plus,
                vec![Math::ident("a"), Math::ident("b"), Math::real(-2.0)]
            )),
            "a + b + -2"
        );
        assert_eq!(
            infix(Math::Root {
                degree: Some(Box::new(Math::int(3))),
                arg: Box::new(Math::ident("x"))
            }),
            "x^(1/3)"
        );
        assert_eq!(
            infix(Math::Log {
                base: Some(Box::new(Math::int(2))),
                arg: Box::new(Math::ident("x"))
            }),
            "ln(x)/ln(2)"
        );
        assert_eq!(infix(Math::Constant(Constant::True)), "1");
        // Local parameters renamed.
        let rename = HashMap::from([("k".to_owned(), "J1_k".to_owned())]);
        let ast = from_math(
            &Math::Apply(Op::Times, vec![Math::ident("k"), Math::ident("S")]),
            &rename,
        )
        .unwrap();
        assert_eq!(to_infix(&ast), "J1_k*S");
        // What can't be said.
        assert!(from_math(&Math::Constant(Constant::Infinity), &none).is_err());
        assert!(from_math(&Math::Raw("<diff/>".into()), &none).is_err());
    }
}
