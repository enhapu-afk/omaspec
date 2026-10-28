# Fuzzing

Coverage-guided fuzzing with [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
(nightly Rust and libFuzzer):

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
cargo +nightly fuzz run text           # lint, outline, formatting, …
cargo +nightly fuzz run image_source   # source from PNG and SVG bytes
cargo +nightly fuzz run sanitize       # the SVG sanitizer
cargo +nightly fuzz run mermaid        # mmdr and Omagram's repairs
cargo +nightly fuzz run plantuml       # Omagram's own PlantUML engine
cargo +nightly fuzz run model          # checks, plays, conversion and diff of the behavior model
```

Run `mermaid` with `-O` (`cargo +nightly fuzz run -O mermaid`): without it mmdr
checks its own geometry under debug assertions, which is slow and panics on flaws
in a picture that a release build draws as they are.

A timeout finds inputs that hang (libFuzzer waits 20 minutes without one); give
`text` and `mermaid` 30 seconds (`-- -timeout=30`), the others 10. A `!while` that
never ends runs the preprocessor to its limit, 0.2 s in a release build but about
5 s instrumented, and mmdr takes as long on a large broken flowchart the engine
leaves to it; with several fuzzers at once, 10 s stops those two on these known
slow inputs before they find anything new.

Seed `text` with the fixtures for a faster start:
`cargo +nightly fuzz run text crates/omagram-core/tests/fixtures`.

Every `cargo test` also runs a quicker, seeded version of the same checks on stable
(`tests/robust.rs` in omagram-core and omagram-render).
