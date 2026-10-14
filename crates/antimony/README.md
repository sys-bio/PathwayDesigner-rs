# antimony

Read and write [Antimony](https://tellurium.readthedocs.io/en/latest/antimony.html),
the human-readable language for biochemical models, and convert it to
[SBML](https://sbml.org). Pure Rust with no dependencies, so it also works in
WebAssembly.

> **Status: under construction, not yet published.** Reading and writing
> Antimony are built (milestone A6): `antimony::parse` gives a checked model
> and its diagnostics, and `antimony::write_document` writes it back. SBML
> output is next (A7). The package will be published as `antimony-rs` (the name
> `antimony` is taken on crates.io); the library is called `antimony`.

It is a port of a complete, tested Antimony implementation written in Delphi,
and is checked against it case by case.

## What it will do

- **Parse** Antimony text into a typed model: species, parameters,
  compartments, reactions, rules, events, functions, units, constraints,
  annotations. Every item carries its position in the source.
- **Report problems as diagnostics**, never by panicking. A malformed file
  gives back a model and a list of errors and warnings, each with a code,
  a message and a position, so an editor can show them all at once.
- **Write Antimony back.** Anything it writes, it reads back to the same
  model.
- **Write SBML Level 3 Version 2** (optional `sbml` feature), using the
  `sbml-io` crate.

Not in scope: layout and render syntax (a `layout` block is skipped and kept),
DNA strand syntax, reading SBML, simulation, and keeping the exact formatting
of the input.

## Design

`SPEC.md` records the design and every place it departs from the Delphi
version. In short: plain structs with public fields, `Vec` and `Option`, one
`Symbol` type whose kind is inferred as the file is read (as Antimony
requires), and expressions as an ordinary owned `enum`.

## Testing

```sh
cargo test
```

The tests that need the 1788-model test corpus, or the Delphi reference
implementation, find them through environment variables and are skipped
with a note when those are not set (see `SPEC.md` §8):

| Variable | Points at |
|---|---|
| `ANTIMONY_CORPUS` | the folder holding `NNNNN/NNNNN-antimony.ant` |
| `ANTIMONY_CORPUS_INDEX` | the corpus `index.json` |
| `ANTIMONY_BIOMODELS` | the curated BioModels folder |
| `ANTIMONY_ORACLE` | the reference converter, `ant2sbml.exe` |

With all three corpus variables set, the differential tier compares this
crate with the reference implementation over the whole corpus:

```sh
cargo run --release --example differential
```

## Licence

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
