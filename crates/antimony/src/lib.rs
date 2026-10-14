//! Read and write [Antimony](https://tellurium.readthedocs.io/en/latest/antimony.html),
//! the human-readable language for biochemical models, and convert it to SBML.
//! Pure Rust with no dependencies, so it also works in WebAssembly.
//!
//! **Status: under construction.** This is the crate skeleton; the lexer,
//! parser, writers and API arrive milestone by milestone (see `PLAN.md`).
//! The design is recorded in `SPEC.md`.

#![forbid(unsafe_code)]
