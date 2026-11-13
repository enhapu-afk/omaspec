# Fuzzing

Coverage-guided fuzzing with [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
(nightly Rust and libFuzzer). `run.sh` seeds each target with the repository's own
diagrams, runs it for a while with the dictionary of both languages' words
(`dict/diagrams.dict`), and keeps what it finds:

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
fuzz/run.sh                 # every target, 300 s each
fuzz/run.sh 900 plantuml    # one target, 15 minutes
```

| Target | What it holds |
|---|---|
| `plantuml` | Omaspec's own PlantUML engine: no panic, a declined diagram names a line inside the text, what it draws passes the sanitizer |
| `mermaid` | Omaspec's Mermaid drawers and mmdr behind them |
| `layout` | the layout of graph diagrams on any graph: boxes, lines, frames, held boxes and rank words; every place finite, a route for every line, no two boxes on top of each other |
| `hand` | the block by hand: every entry that reads writes back as read, a box held by any name reads back as written |
| `text` | lint, outline, formatting (which settles after one pass) |
| `model` | checks, plays, conversion and diff of the behavior model |
| `document` | Markdown with fenced blocks of any kind: the diagrams found lie in fences |
| `trace` | a recorded trace (JSON Lines) of any text against a state machine |
| `sanitize` | the SVG sanitizer: what it lets through holds nothing active |
| `image_source` | the source read back from any PNG or SVG bytes |

A crash, or an input that takes longer than 20 seconds, lands in
`fuzz/artifacts/TARGET/`; `cargo +nightly fuzz run -O -s none TARGET FILE` repeats it,
`cargo +nightly fuzz tmin -O -s none TARGET FILE` shrinks it. The corpus grows in
`fuzz/corpus/TARGET` and is kept between runs.

The address sanitizer is off (`-s none`): Omaspec has no unsafe code, and it made every
input several times slower, so the known slow inputs passed the timeout under load. An
endless `!while` runs the preprocessor to its limit (0.4 s in a release build), and
mmdr takes about as long on a large broken flowchart.

The workflow `Fuzz` runs every target for ten minutes each night, from the corpus of
the nights before, and keeps what it finds as an artifact. Every `cargo test` runs a
quicker, seeded version on stable (`tests/robust.rs` in omaspec-core and omaspec-render):
mutated diagrams of every kind, each on a small stack within a time limit.
