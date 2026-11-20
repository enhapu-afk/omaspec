# Omaspec

Native PlantUML and Mermaid editor for Omarchy Linux, in Rust with iced. Plan
and what is open: `ROADMAP.md`; what is built: `docs/DONE.md` (a finished item moves
there). Decisions: `docs/adr/`.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all
cargo run -p omaspec-app -- theme          # palette of the active Omarchy theme
cargo run -p omaspec-app -- new DIR        # a project: omaspec-project.toml and a first machine; its map is made (docs/adr/0017, 0026)
cargo run -p omaspec-app -- lint FILE      # --project DIR for the state machines; also fmt, outline, inspect, extract
cargo run -p omaspec-app -- trace FILE     # a JSON Lines trace against the state machines of its project; --as timing [-o picture]
cargo run -p omaspec-app -- coverage FILE --traces T.jsonl   # what traces take of a machine; -o picture, --river as a trace river, --json, --min
cargo run -p omaspec-app -- matrix FILE    # a state machine as a table, states down, events across; -o .md or a picture, --json
cargo run -p omaspec-app -- orbit FILE     # a state machine's life cycle on a ring; -o a picture, else SVG on standard output
cargo run -p omaspec-app -- refs NAME      # where a diagram, state or event is used in the project (docs/adr/0019)
cargo run -p omaspec-app -- rename OLD NEW # renamed at every use; --dry-run shows what would change
cargo run -p omaspec-app -- mcp            # MCP server over standard input and output (docs/adr/0008)
cargo run -p omaspec-app -- diff FILE      # what changed in the behavior since the last commit; or OLD NEW; a folder: the project; -o a picture
cargo run -p omaspec-app -- log FILE       # what each commit changed in the behavior of a diagram; -n, --json
cargo run -p omaspec-app -- git setup      # git diff, log -p and show tell diagrams by their behavior (textconv)
cargo run -p omaspec-app -- convert FILE   # the machine or scenario in the other language (docs/adr/0009)
cargo run -p omaspec-app -- example [NAME] # the gallery's examples (examples/gallery); prints one
UPDATE_GOLDEN=1 cargo test -p omaspec-core --test golden   # after an intended behavior change; review the diff
UPDATE_GOLDEN=1 cargo test -p omaspec-uml --test golden    # the same for the native PlantUML SVGs
UPDATE_GOLDEN=1 cargo test -p omaspec-render --test golden # the same for the Mermaid diagrams Omaspec draws
OMASPEC_LINES=1 cargo test -p omaspec-render --test lines -- --nocapture   # how clean the lines are; UPDATE_LINES=1 after an intended change
OMASPEC_SCALE=1 cargo test --release -p omaspec-app scale -- --nocapture  # generated projects of 100 to 2,000 diagrams, timed
OMASPEC_FRAMES=1 cargo run --release -p omaspec-app -- FILE   # how smooth each dive, glide, play and fade is, on standard error
OMASPEC_FRAMES=1 cargo test --release -p omaspec-app frames_ -- --nocapture  # the same, headless on the GPU at 2560x1440
OMASPEC_ARRIVAL_TRACES=/tmp/a.jsonl cargo test -p omaspec-app paper::   # each pane's way through paper/arrival.puml (docs/adr/0030); then omaspec coverage on it
OMASPEC_DIVE_DIR=/tmp OMASPEC_DIVE_PROJECT=DIR cargo test --release -p omaspec-app snapshot_dive   # every frame of the zoom into each tile of a project's map and back
cargo run -p omaspec-app -- render FILE    # SVG/PNG/PDF, or text (.txt: the behavior), or .html a presentation that plays it (docs/adr/0031); --json for the report; --fit a PNG past 100 million pixels, --min-font PX its words at least so high, in tiles where it must; --text-as-paths an SVG's words as outlines; OMASPEC_PLANTUML=path|none for the Java fallback
cargo run -p omaspec-app -- render DIR/omaspec-project.toml   # the project's map, made from its diagrams (docs/adr/0026)
cargo run -p omaspec-app -- render --check DIR   # each SVG drawn now against the one beside it; exit 1 when one differs
cargo run -p omaspec-app -- check DIR      # lint, the machines, the links and render --check; --json, --github, --strict
cargo run -p omaspec-app -- export DIR     # the project as SVGs linked to one another, index.svg and a ZIP, in DIR/export; --html as one page that zooms into each tile
cargo run -p omaspec-app -- [FILE...]      # editor window
cargo run -p omaspec-app -- preview FILE   # preview window that follows the file
cargo run -p omaspec-app -- play FILE -o play.gif   # a play as a GIF, or in full color as .webp or .png (animated), drawn headless on the GPU; a fold, a dive and the animations below also as .html, a page in vectors; --events, --steps, --width, --fps, --theme, --colors and --lossy (smaller); --fold NAME a fold, --dive TILE (on omaspec-project.toml) a zoom into the map, TILE/LINK/LINK a flight on down the links, --build the source beside the diagram building itself up line by line (no GPU), --lockstep a scenario beside the machines of its participants, each taking its step as a message reaches it, --history every commit of the file gliding into the next, --from REV [--to REV] a time travel from one commit to the file now (or to another), what changed glowing, --traffic T.jsonl recorded traces flowing through the machine as dots, --themes[=A,B] the diagram sweeping through Omarchy's themes, --ink the sketch drawn as with a pen and inked; a diagram too large to read whole is followed step by step, --whole keeps it whole
OMASPEC_SNAPSHOT_DIR=/tmp cargo test -p omaspec-app snapshot   # headless screenshots of the editor, the preview and the gallery
OMASPEC_SNAPSHOT_DIR=/tmp cargo test --release -p omaspec-app snapshot_   # README pictures, every bundled theme
docs/images/update.sh                      # the README's and the guide's pictures into docs/images (ImageMagick)
docs/live/update.sh                        # the landing page's live examples into docs/live: real Omaspec pages, from the examples
docs/images/film.sh                        # the landing page's film (omaspec.mp4): the editor recorded headless, every frame; ffmpeg
docs/guide/pdf/build.py                    # the guide as a PDF (docs/guide/omaspec-guide.pdf): its examples drawn by the release build; Chromium
cargo bench -p omaspec-render              # render and export timings
cargo test --release -p omaspec-render --test dense -- --nocapture   # 150 boxes, 476 lines in under a second (CI too)
cargo test --release -p omaspec-app --test budget -- --nocapture     # a play of 600 frames without a GPU within 400 MiB (CI too)
fuzz/run.sh [SECONDS] [TARGET...]          # coverage-guided fuzzing, nightly Rust and cargo-fuzz (fuzz/README.md); each night in CI
cargo check --target x86_64-pc-windows-gnu -p omaspec-app -p omaspec-render -p omaspec-core -p omaspec-uml -p omaspec-model   # the engine and the binary stay buildable for Windows
cargo run --release -p spike-editor          # editor spike window
cargo run --release -p spike-menu            # menu bar, context menus and toolbar: iced_aw against an overlay of our own (docs/adr/0025); --snapshot DIR
cargo run --release -p spike-mermaid -- DIR  # render .mmd files, time them
```

## Rules

- Public repo: no personal data and nothing private in any file.
- Rust 2024, `unsafe_code` is forbidden workspace-wide, clippy and rustfmt clean.
- No network access at run time, no telemetry.
- Every SVG from an engine goes through `omaspec_render::sanitize` before it is shown or
  exported; so do the links an export lays over it (`sanitize_with`, relative links up to
  the project, one rule for all links); only the embedded source is written after it.
  Exports parse SVG only through `omaspec_render::export::parse` (and `export::drawn` for
  where its drawing elements lie), which never read files an SVG points to.
- `omaspec-core`, `omaspec-uml` and `omaspec-model` do no I/O and have no GUI
  dependency.
- The engine crates have no platform code. What only Linux has (Landlock, the
  Wayland app id, Unix modes) sits behind `cfg` in `omaspec-app`, with a plain
  path for the other desktops; the Windows check above has to pass.
- Spikes are throwaway; crates never depend on them.
- `vendor/` holds third-party crates with our changes (scrive); keep its diff
  to the original small and listed in `vendor/README.md`.
- Docs, code and commit messages in English. Commits: `feat:`, `fix:`,
  `docs:`, `chore:` with a lowercase summary.
- A new diagram type is read, measured and painted in modules of their own
  (as `omaspec-uml/src/activity/`); no new file grows past about 1,000 lines.
- A Mermaid kind with a drawer of its own is one entry in `NATIVE`
  (`omaspec-render/src/mermaid.rs`), with the words of its header: no
  other list of kinds, no `try_draw` chain, no `named_decline` of its own.
  Its drawer reads the text itself; mmdr's parse is only the fallback.
  What a first line names is judged as mmdr reads it, by render and lint
  alike, in `omaspec_core::mermaid_head`: its table of every kind mmdr
  reads is not a list of drawers, and a test holds `NATIVE`'s headers to
  it.
- State diagrams of both languages are drawn by `omaspec-uml/src/state/`
  (Mermaid's lowered by `state::from_mermaid`). Containers that open
  (docs/adr/0013) are laid out once, by `omaspec_uml::graph::open`, for
  state, class and flowchart diagrams, and lines into closed ones are
  routed by `graph::across`: a fix to either goes there, not into a
  drawer.
- A lint rule about names (declared twice, written once, a namesake)
  reads `omaspec_uml::Mention`s, which the engine's readers keep as they
  read (`Diagram::names`): a new line that names something is recorded
  there. `lint/written.rs` reads by pattern only what the engine declines.
- Words one line of the source alone writes (a message, an action, a
  transition's label, a class's member, a line's multiplicity, a note)
  are tied to that line as they are drawn:
  measured with `Block::written_on`, or drawn inside `Canvas::tie`, from
  the line their words start on. Words written amid others on their line
  (a flowchart's label between its ends, a multiplicity) say where they
  stand: `Block::written_at` with the byte, kept by the reader
  (`Text::col`); words without it close their line. A
  name many lines mention is not tied; a name drawn a second time (a
  lifeline's foot) is drawn inside `Canvas::echo`. Every drawer ends its
  map with `Canvas::place`, which hands these to the pane: a click and
  the caret's light take tied words by their line, not by counting.

## Omarchy facts the code relies on

- Active theme: `~/.local/state/omarchy/current/theme/colors.toml`, name in
  `~/.local/state/omarchy/current/theme.name`. Switching themes replaces the
  directory, so watch `current/`, not the file.
- Missing colors resolve like `/usr/share/omarchy/bin/omarchy-theme-color`
  (legacy short names, ANSI `color0`..`color15`, derived shades);
  `omaspec-theme` mirrors that cascade.
- Font: fontconfig `monospace` (set by `omarchy font set`).
- Window classes (Wayland app ids) for Hyprland rules: `omaspec` (editor),
  `omaspec-preview` (preview window, and the editor's diagram in its own
  window).
- Bundled themes for tests: `/usr/share/omarchy/themes/*/colors.toml`.
