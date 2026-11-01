# Omaspec

Native PlantUML and Mermaid editor for Omarchy Linux, in Rust with iced. Plan
and status: `ROADMAP.md`. Decisions: `docs/adr/`.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all
cargo run -p omaspec-app -- theme          # palette of the active Omarchy theme
cargo run -p omaspec-app -- new DIR        # a project: omaspec-project.toml, its map and a first machine (docs/adr/0017, 0018)
cargo run -p omaspec-app -- lint FILE      # --project DIR for the state machines; also fmt, outline, inspect, extract
cargo run -p omaspec-app -- trace FILE     # a JSON Lines trace against the state machines of its project
cargo run -p omaspec-app -- mcp            # MCP server over standard input and output (docs/adr/0008)
cargo run -p omaspec-app -- diff FILE      # what changed in the behavior since the last commit; or OLD NEW
cargo run -p omaspec-app -- convert FILE   # the machine or scenario in the other language (docs/adr/0009)
cargo run -p omaspec-app -- example [NAME] # the gallery's examples (examples/gallery); prints one
UPDATE_GOLDEN=1 cargo test -p omaspec-core --test golden   # after an intended behavior change; review the diff
UPDATE_GOLDEN=1 cargo test -p omaspec-uml --test golden    # the same for the native PlantUML SVGs
UPDATE_GOLDEN=1 cargo test -p omaspec-render --test golden # the same for the Mermaid diagrams Omaspec draws
cargo run -p omaspec-app -- render FILE    # SVG/PNG/PDF; --json for the report; OMASPEC_PLANTUML=path|none for the Java fallback
cargo run -p omaspec-app -- [FILE...]      # editor window
cargo run -p omaspec-app -- preview FILE   # preview window that follows the file
OMASPEC_SNAPSHOT_DIR=/tmp cargo test -p omaspec-app snapshot   # headless screenshots of the editor, the preview and the gallery
OMASPEC_SNAPSHOT_DIR=/tmp cargo test --release -p omaspec-app snapshot_   # README pictures, every bundled theme
docs/images/update.sh                      # the README's and the guide's pictures into docs/images (ImageMagick)
cargo bench -p omaspec-render              # render and export timings
cargo run --release -p spike-editor          # editor spike window
cargo run --release -p spike-mermaid -- DIR  # render .mmd files, time them
```

## Rules

- Public repo: no personal data and nothing private in any file.
- Rust 2024, `unsafe_code` is forbidden workspace-wide, clippy and rustfmt clean.
- No network access at run time, no telemetry.
- Every SVG from an engine goes through `omaspec_render::sanitize` before it is shown or
  exported; exports parse SVG only through `omaspec_render::export::parse`, which never
  reads files an SVG points to.
- `omaspec-core`, `omaspec-uml` and `omaspec-model` do no I/O and have no GUI
  dependency.
- Spikes are throwaway; crates never depend on them.
- `vendor/` holds third-party crates with our changes (scrive); keep its diff
  to the original small and listed in `vendor/README.md`.
- Docs, code and commit messages in English. Commits: `feat:`, `fix:`,
  `docs:`, `chore:` with a lowercase summary.
- A new diagram type is read, measured and painted in modules of their own
  (as `omaspec-uml/src/activity/`); no new file grows past about 1,000 lines.
- A Mermaid kind with a drawer of its own is one entry in `NATIVE`
  (`omaspec-render/src/mermaid.rs`): no other list of kinds, no `try_draw`
  chain, no `named_decline` of its own.
- State diagrams of both languages are drawn by `omaspec-uml/src/state/`
  (Mermaid's lowered by `state::from_mermaid`). Until flowcharts share that
  scene (`ROADMAP.md`, after the code review), a fix to open clusters or to
  lines between containers goes into both `omaspec-uml/src/state/draw*` and
  `omaspec-render/src/mermaid/layered/`.

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
