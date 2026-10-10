# PathwayDesigner (Rust)

A drawing tool for biochemical reaction networks, written in Rust with
[egui](https://github.com/emilk/egui). It runs in the browser, and a desktop
version will follow from the same code.

**Try it:** https://sys-bio.github.io/PathwayDesigner-rs/

This is a re-implementation of the Delphi PathwayDesigner, and it is at an early
stage: the web page currently only shows a test of the font set-up (milestone R0).

## Documents

- [SPECIFICATION.md](SPECIFICATION.md): what the application does.
- [SPEC-Rust.md](SPEC-Rust.md): how it is built in Rust, and the milestone plan.

## Building

You need Rust, the `wasm32-unknown-unknown` target and
[Trunk](https://trunkrs.dev/).

```
cargo test --workspace                # tests
cargo run -p pd-app                   # desktop window
trunk serve                           # browser, at http://127.0.0.1:8080/
```

## Licence

MIT, see [LICENSE](LICENSE). The bundled Noto Sans font is under the SIL Open
Font Licence ([assets/fonts/NotoSans-OFL.txt](assets/fonts/NotoSans-OFL.txt)).
