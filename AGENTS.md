# Omagram

Native PlantUML and Mermaid editor for Omarchy Linux, in Rust with iced. Plan
and status: `ROADMAP.md`. Decisions: `docs/adr/`.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all
cargo run -p omagram-app -- theme          # palette of the active Omarchy theme
cargo run -p omagram-app -- lint FILE      # --project DIR for the state machines; also fmt, outline, inspect, extract
cargo run -p omagram-app -- trace FILE     # a JSON Lines trace against the state machines of its folder
cargo run -p omagram-app -- mcp            # MCP server over standard input and output (docs/adr/0008)
cargo run -p omagram-app -- diff FILE      # what changed in the behavior since the last commit; or OLD NEW
cargo run -p omagram-app -- convert FILE   # the machine or scenario in the other language (docs/adr/0009)
cargo run -p omagram-app -- example [NAME] # the gallery's examples (examples/gallery); prints one
UPDATE_GOLDEN=1 cargo test -p omagram-core --test golden   # after an intended behavior change; review the diff
UPDATE_GOLDEN=1 cargo test -p omagram-uml --test golden    # the same for the native PlantUML SVGs
cargo run -p omagram-app -- render FILE    # SVG/PNG/PDF; --json for the report; OMAGRAM_PLANTUML=path|none for the Java fallback
cargo run -p omagram-app -- [FILE...]      # editor window
cargo run -p omagram-app -- preview FILE   # preview window that follows the file
OMAGRAM_SNAPSHOT_DIR=/tmp cargo test -p omagram-app snapshot   # headless screenshots of the editor, the preview and the gallery
OMAGRAM_SNAPSHOT_DIR=/tmp cargo test --release -p omagram-app snapshot_   # README pictures, every bundled theme
docs/images/update.sh                      # the README's and the guide's pictures into docs/images (ImageMagick, ffmpeg)
cargo bench -p omagram-render              # render and export timings
cargo run --release -p spike-editor          # editor spike window
cargo run --release -p spike-mermaid -- DIR  # render .mmd files, time them
```

## Rules

- Public repo: no personal data and nothing private in any file.
- Rust 2024, `unsafe_code` is forbidden workspace-wide, clippy and rustfmt clean.
- No network access at run time, no telemetry.
- Every SVG from an engine goes through `omagram_render::sanitize` before it is shown or
  exported; exports parse SVG only through `omagram_render::export::parse`, which never
  reads files an SVG points to.
- `omagram-core`, `omagram-uml` and `omagram-model` do no I/O and have no GUI
  dependency.
- Spikes are throwaway; crates never depend on them.
- `vendor/` holds third-party crates with our changes (scrive); keep its diff
  to the original small and listed in `vendor/README.md`.
- Docs, code and commit messages in English. Commits: `feat:`, `fix:`,
  `docs:`, `chore:` with a lowercase summary.

## Omarchy facts the code relies on

- Active theme: `~/.local/state/omarchy/current/theme/colors.toml`, name in
  `~/.local/state/omarchy/current/theme.name`. Switching themes replaces the
  directory, so watch `current/`, not the file.
- Missing colors resolve like `/usr/share/omarchy/bin/omarchy-theme-color`
  (legacy short names, ANSI `color0`..`color15`, derived shades);
  `omagram-theme` mirrors that cascade.
- Font: fontconfig `monospace` (set by `omarchy font set`).
- Window classes (Wayland app ids) for Hyprland rules: `omagram` (editor),
  `omagram-preview` (preview window, and the editor's diagram in its own
  window).
- Bundled themes for tests: `/usr/share/omarchy/themes/*/colors.toml`.
