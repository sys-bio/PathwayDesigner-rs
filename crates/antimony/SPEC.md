# The `antimony` crate — Rust design

This crate is a Rust port of the author's Delphi Antimony implementation
("the Delphi version"). **The Delphi `SPECIFICATION.md` stays the normative
description of the language surface and its semantics**, and its decisions
record (§13, 25 decisions) stands unless this file says otherwise. This file
records only what is different in Rust: the shape of the code, and every
place the port departs from the Delphi design (§9). Where a later milestone
settles something left open here, it is written in here at that time.

The Antimony language reference,
<https://tellurium.readthedocs.io/en/latest/antimony.html>, outranks both on
what the *language* is (Delphi decision 19).

Section numbers written "Delphi §n" refer to the Delphi `SPECIFICATION.md`.

Written 2026-10-13, milestone A0.

---

## 1. Name, place, licence

- **Package `antimony-rs`, library `antimony`.** The name `antimony` on
  crates.io belongs to an unrelated crate (a 2017 stream-processing topology
  builder). The library keeps the language's name, so users write
  `use antimony::…`; only the `Cargo.toml` line says `antimony-rs`. The folder
  is `crates/antimony`.
- **Licence: MIT OR Apache-2.0**, chosen by the author on 2026-10-13. This
  replaces Delphi decision 5 (BSD, to match libantimony) for this crate. The
  code is ported from the author's own Delphi source. If libantimony's C++
  source is ever consulted for a point of semantics, say so in the commit
  message and in this file; its BSD licence is compatible.
- **Movable.** Everything the crate needs is inside `crates/antimony/`:
  `Cargo.toml` uses no `workspace = true` keys, tests find their files from
  `CARGO_MANIFEST_DIR`, and nothing in the crate refers to the application
  that first uses it. The one exception is `PLAN.md`, the working plan, which
  is rewritten or dropped when the crate moves.

## 2. Dependencies and targets

- **The core has no dependencies.** Lexer, parser, semantics and the Antimony
  writer are plain `std`.
- **The `sbml` feature** (milestone A7) adds `sbml-io` and nothing else. It is
  declared when A7 starts, not before. Until `sbml-io` is published the
  dependency is a path with a version (`{ path = "../sbml-io", version =
  "0.1", optional = true }`), so publishing this crate requires publishing
  `sbml-io` first.
- **Dev-dependencies** are allowed for tests and examples, since they never
  reach a user, but are kept few. A0 has none.
- **Targets:** native (Windows, macOS, Linux) and `wasm32-unknown-unknown`.
  The core therefore never touches the file system, the clock or threads; it
  takes `&str` and returns values. Reading a file is the caller's job.
- `edition = "2024"`, `rust-version = "1.85"` (the first with that edition).
- `#![forbid(unsafe_code)]`.

## 3. The data model

The Delphi object model (Delphi §4) is a class hierarchy with owned object
lists and object references between items. In Rust it becomes **plain
structs with public fields**, `Vec` for lists, `Option` for things that may be
absent, and enums for alternatives. No trait objects, no `Rc`, no
interior mutability, and no getters or setters for plain data.

### 3.1 References between items are by name

Delphi binds names to objects (`TAntSymbol.Compartment: TAntSymbol`,
`TAntParticipant.Species`, the expression node's `Binding`). In Rust every
reference is the **identifier as a `String`** — `compartment: Option<String>`,
a participant's `species: String`, an expression's `Symbol(String)` — and is
looked up in its module when needed.

Why: Antimony allows forward references and type promotion, a caller may
edit the model freely between parse and write, and names are what both
writers emit. Pointers or indices would have to be kept in step with every
edit; names cannot go stale. Lookups are by name through methods on
`Module` (`module.symbol("S1")`); passes that do many lookups (the semantic
pass, the writers) build a temporary `HashMap<&str, usize>` for the duration
of the pass. The model itself stores only `Vec`s, so editing its public
fields can never leave an index out of date.

### 3.2 Absence is `Option`, not a sentinel

Delphi's sentinels become `Option`: an `SboTerm` of −1 becomes
`sbo_term: Option<u32>`; an empty `DisplayName`, `Notes`, `UnitsName` or
`CompartmentName` becomes `None`. Where the Delphi version uses a flag to
record that something was *written but empty* (`HasEmptyValue`,
`HasEmptyKineticLaw`, Delphi §9.1.4 #3), the flag stays a `bool` beside the
`Option`, because "written empty" and "not written" are different facts that
both have to survive a round trip.

### 3.3 One `Symbol`, with promotion

Delphi §4.1's unified symbol becomes one struct:

```rust
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,          // Delphi TAntSymbolType
    pub constness: Constness,      // Default, Const, Var
    pub is_boundary: bool,
    pub is_substance_only: bool,
    pub compartment: Option<String>,
    pub initial_value: Option<Expr>,
    pub has_empty_value: bool,
    pub rule: Option<Rule>,        // assignment or rate rule, with its expression
    pub units: Option<String>,
    pub conversion_factor: Option<String>,
    pub display_name: Option<String>,
    pub notes: Option<String>,
    pub sbo_term: Option<u32>,
    pub cv_terms: Vec<CvTerm>,
    pub metadata: Metadata,
    pub comment: Option<String>,
    pub is_implicit: bool,         // never declared: §6.5's typo case
    pub source_path: String,       // seam 3 (Delphi §2.2.2)
    pub span: Span,
    pub decl_span: Span,
}
```

`SymbolKind` keeps all thirteen Delphi values (`Unknown`, `Species`,
`Parameter`, `Compartment`, `Reaction`, `Event`, `Function`, `Submodule`,
`UnitDef`, `Constraint`, `Interaction`, `Deleted`, `Stoichiometry`), and the
promotion table of Delphi §6.2 (with `parameter → stoichiometry`, decision
17) is a function on it. This sketch fixes the *shape*; the exact field list
is settled in A3–A5 against `uAntModel`, and recorded here.

The other model types follow Delphi §4.2 one for one — `Participant`,
`Reaction`, `Interaction`, `Event`, `EventAssignment`, `Function`, `UnitDef`,
`Constraint`, `Objective`, `CvTerm`, `Creator`, `Metadata`, `Module`,
`Document` — with the same conventions.

### 3.4 Expressions are an owned `enum`

```rust
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

pub enum ExprKind {
    Number { value: f64, units: Option<String> },   // 2.5, inf, nan; `40 foo`
    Constant(Constant),                             // pi, exponentiale, true, false
    Symbol(String),                                 // possibly dotted: A.S1
    Unary { op: UnaryOp, operand: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Conditional { condition: Box<Expr>, then: Box<Expr>, otherwise: Box<Expr> },
    Call { name: String, args: Vec<Expr> },
}
```

This mirrors `TAntExprKind`. Ownership makes Delphi §3.3's clone-on-assign
rule automatic: an `Expr` cannot be aliased, and copying one is `.clone()`.
The V8 defects Delphi §12.1 lists (no clone, leaks, unary operand in the
wrong field, an argument list on every leaf) cannot be written in this shape.

### 3.5 Documents and modules

`Document { modules: Vec<Module>, … }` from the start, even though it holds
one module until modules are built (seam 1, Delphi §2.2.2), with a
`main_module()` method. `flatten(&Document) -> Module` exists from A4 and
returns a copy of the main module (seam 2); the SBML writer is written against
its result, never against the parse tree.

### 3.6 Determinism

Anything ordered is a `Vec`, kept in source order. `HashMap` is used only for
lookup and is never iterated to produce output, so the same input always
gives the same model, the same diagnostics in the same order, and the same
text.

## 4. Source positions

```rust
pub struct Pos {
    pub offset: usize,   // byte offset into the source, 0-based
    pub line: u32,       // 1-based
    pub column: u32,     // 1-based, counted in characters (Unicode scalar values)
}

pub struct Span {
    pub start: Pos,
    pub end: Pos,        // exclusive: one past the last character
}
```

Every token, expression node and model item carries a `Span` (Delphi G4). A
span's length in bytes is `end.offset - start.offset`, and
`&source[span.start.offset..span.end.offset]` is its text. `Span::default()`
(line 0) means "no position", for items built in code rather than parsed.

**Line and column are computed from the byte offset**, through a table of
line starts built once per source, rather than counted as the lexer moves.
Delphi recorded that only `ConsumeNewline` tracked lines and that any scanner
which crossed a newline by hand shifted every later diagnostic (its
non-negotiables). Deriving positions from the offset makes that defect
impossible to write. `\n`, `\r\n` and a lone `\r` each end a line. A leading
byte-order mark is skipped.

Departure: Delphi counts columns in UTF-16 code units; this crate counts
characters. They differ only for characters outside the Basic Multilingual
Plane, and the fixtures compare lines only.

## 5. Diagnostics

```rust
pub struct Diagnostic {
    pub severity: Severity,   // Hint, Warning, Error
    pub code: Code,
    pub message: String,
    pub span: Span,
}
```

- **`Code` keeps every Delphi `TAntDiagCode`**, without the `dc` prefix
  (`Code::TypeConflict`). `code.name()` gives the Delphi spelling,
  `"dcTypeConflict"`, so the `.expected` fixtures are shared unchanged and a
  host can match on the same names as before. Each code has the Delphi
  default severity.
- **Messages use the Delphi wording** (`uAntStrings`), so the two
  implementations can be compared on diagnostics as well as models.
- **Display:** `diagnostic.display(file)` gives Delphi's one-line form,
  `file(3:17): error: message [dcSyntaxError]`. The diagnostic does not store
  the file name: the parser takes a `&str`, and only the caller knows where it
  came from.
- Diagnostics are returned **in source order** (Delphi `SortBySpan`).

## 6. Entry points and the error contract

A sketch; the final names are settled by the milestone that builds each one
and recorded here.

```rust
let loaded = antimony::parse(source);                      // never fails, never panics
let loaded = antimony::parse_with(source, &Options { strict: true, ..Default::default() });
loaded.document          // the model, as far as it could be read
loaded.diagnostics       // Vec<Diagnostic>
loaded.has_errors()
let doc = loaded.into_result()?;   // Result<Document, Vec<Diagnostic>> for callers who want it

antimony::write(&module) -> String                         // A6
antimony::to_sbml(&module) -> sbml_io::Document            // A7, `sbml` feature
```

- **A malformed file is not an error for the caller to unwrap.** It is a
  document plus error diagnostics (Delphi §7.1). `into_result()` exists for
  callers who want Delphi's stricter facade rule — no model after a failed
  load — as an ordinary `Result`.
- **No input may panic.** Every path through the lexer and parser recovers;
  a panic on any input is a bug. A fuzz-style test over mangled corpus files
  is added in A3.
- **`Options`** (a plain struct with `Default`): `strict` turns warnings into
  errors (Delphi `StrictMode`); `initialise_missing` is Delphi §6.5.1's
  opt-in fill-in (`--init-zero`) — off by default, still reported, zero
  except for a compartment, which gets 1.
- **Queries return `Option`.** Delphi's facade raises `EAntArgumentError` for
  an id that names nothing; in Rust a lookup by id returns `Option<&T>`, and
  a lookup by index is a slice index, which panics like any other — the same
  "this is a bug in the caller" meaning, in Rust's idiom. The Delphi rule
  that a caller writes no tests for missing values (a species with no value
  reads 0, a compartment 1, a stoichiometry 1) becomes explicit helper
  methods in A8, so the raw `Option` is always there for a caller who needs
  to know the value was absent.
- **No index-based accessor family.** Delphi's `GetSpeciesCount` /
  `GetSpeciesId(i)` style exists for its facade; here the model's `Vec`s and
  iterator methods (`module.species()`, `module.parameters()`) serve
  instead. A8 decides what else the facade needs.

## 7. Layout of the code

| Module | Ports | Notes |
|---|---|---|
| `span` | `TSourcePos`, `TSourceSpan` | §4 |
| `diagnostics` | `uAntDiagnostics`, `uAntStrings` | §5 |
| `lexer` | `uAntLexer` | public: an editor can use the tokens for highlighting |
| `expr` | `uAntExprNode`, `uAntExprParser` | public |
| `model` | `uAntTypes` (model enums), `uAntModel` | |
| `parser` | `uAntParser` | |
| `semantics` | `uAntSemantics` | |
| `flatten` | `uAntFlatten` | identity copy, as in Delphi |
| `write` | `uAntWriter` | |
| `sbml` | `uAntSBMLWriter`'s mapping | `sbml` feature; built on `sbml-io` |
| crate root | `uAntFacade` | re-shaped (§6) |

Not ported: `uAntCompat` (a migration shim for Delphi callers; Delphi
decisions 23–25 do not apply), `Oracle/` (replaced by the differential tier,
§8), and the FPC concerns (Delphi decision 4).

## 8. Tests

- **Ported tests keep their Delphi names**, in snake case, in one integration
  test file per Delphi fixture (`tests/lexer.rs` for `uAntLexerTests`, …), so
  each Rust test can be traced to the test it ports. A Delphi test that does
  not apply in Rust is listed, with the reason, at the top of its file.
- **Before trusting a new test, make it fail once** (a deliberately broken
  expectation or input), then restore it.
- **`tests/invalid/`** holds the Delphi `Tests/corpus/invalid/` set: 12
  `.ant` files, each with a `.expected` file listing `<code> <line>` pairs
  (`#` lines are comments). These are the author's own files, copied in.
  The line may be one past the last line: that is end of file, where an
  unterminated block is reported.
- **External data** is found through environment variables. A test whose
  variable is unset prints a note and passes, so a fresh clone passes
  `cargo test`. Nothing from these locations is copied into the crate; the
  SBML Test Suite in particular is LGPL.

  | Variable | Points at | Used from |
  |---|---|---|
  | `ANTIMONY_CORPUS` | the folder holding `NNNNN/NNNNN-antimony.ant` (1788 cases) | A0 |
  | `ANTIMONY_CORPUS_INDEX` | the corpus `index.json` (milestone sets, `lossy`, `challenge`, `expect_reject`) | A3 |
  | `ANTIMONY_BIOMODELS` | the curated BioModels folder | A8 |
  | `ANTIMONY_ORACLE` | the Delphi `ant2sbml.exe` | A3 |

- **The differential tier** (A3 onward) is a `cargo run --example`, Windows
  only, that runs `ANTIMONY_ORACLE` and this crate on the same file and
  compares the results structurally, classifying each difference as Delphi
  decision 22 does. Only a divergence fails.
- **CI**: the workspace CI already runs `cargo fmt --check`, clippy (native
  and `wasm32-unknown-unknown`, warnings as errors) and `cargo test` over
  every member, which covers this crate. When the `sbml` feature exists (A7)
  a step with `--all-features` is added. When the crate moves to its own
  repository it gets its own workflow doing the same.

## 9. Departures from the Delphi design

Every difference from the Delphi version is recorded here, with the reason.
Language behaviour is not changed by the port; these are about code shape,
licence and API.

| # | Delphi | Rust | Why |
|---|---|---|---|
| R1 | Licence BSD (decision 5) | MIT OR Apache-2.0 | The author's choice; the Rust ecosystem's usual pair |
| R2 | Class hierarchy, owned object lists | Plain structs, `Vec`, `Option`, `enum` (§3) | Rust idiom; ownership makes clone-on-assign automatic |
| R3 | Items bound to each other by object reference | References by name, looked up when needed (§3.1) | Survives editing and promotion; nothing to keep in step |
| R4 | Sentinels (−1, empty string) | `Option` (§3.2) | The type says what may be absent |
| R5 | Line counting in the lexer | Line and column computed from the byte offset (§4) | Makes the Delphi line-drift defect impossible |
| R6 | Columns in UTF-16 units | Columns in characters (§4) | Rust strings are UTF-8; differs only outside the BMP |
| R7 | Diagnostic stores a file name | File name supplied when displaying (§5) | The parser takes `&str` and never sees a file |
| R8 | Facade raises on bad ids and on queries after a failed load | `Option` for lookups; `Loaded::into_result()` for the strict form (§6) | Rust idiom; no exceptions |
| R9 | Index-based query API (`GetXxxCount`/`GetXxxId(i)`) | `Vec`s and iterator methods (§6) | Unnecessary with public `Vec`s |
| R10 | Native SBML XML writer (decision 1) | Built on `sbml-io`, following the same mapping | `sbml-io` already writes and validates SBML; one XML writer to maintain |
| R11 | `uAntCompat`, `Oracle/`, FPC (decisions 4, 23–25) | Not ported (§7) | Delphi-specific |
| R12 | Invariant `TFormatSettings` for every number conversion | `str::parse::<f64>` and Rust's own formatting | Both are locale-independent by definition, so the Delphi hazard cannot arise; never call C library number functions |
| R13 | `TStringList` must be made case-sensitive | `String` comparison is case-sensitive | The Delphi defect cannot arise; the BIOMD0000000557 `duspn`/`DUSPn` case is still tested |

## 10. Left open, for the milestone that needs it

- The exact field lists of the model types (A3–A5), checked against
  `uAntModel`.
- How numbers are written back (A6): shortest round-trip form, or the Delphi
  writer's format. Round-trip closure compares values, not text, so either
  holds the invariant.
- The deep model comparison for round-trip closure (A6), following
  `uAntModelCompare`: it ignores spans, and treats `nan` as equal to itself.
- Reading `index.json` (A3): a small hand-written reader in the tests, or a
  JSON dev-dependency.
