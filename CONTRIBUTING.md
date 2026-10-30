# Contributing to Omaspec

Thank you for helping. Issues, diagrams Omaspec draws wrong, and pull requests are all
welcome, and so are questions and ideas: open an
[issue](https://github.com/enhapu-afk/omaspec/issues/new/choose) for them too.

## Build and run

You need Rust 1.89 or newer. On Arch/Omarchy the system libraries iced needs are usually
present; on Debian/Ubuntu install them as CI does:

```bash
sudo apt-get install -y libxkbcommon-dev libwayland-dev libfontconfig1-dev libvulkan-dev
```

```bash
cargo build --release -p omaspec-app        # the binary is target/release/omaspec
cargo run -p omaspec-app -- [FILE...]       # the editor
cargo run -p omaspec-app -- render FILE     # SVG/PNG/PDF from the command line
cargo run -p omaspec-app -- lint FILE       # lint findings, exit code for CI
```

`AGENTS.md` lists every other subcommand and developer command.

## Tests and checks

CI (`.github/workflows/ci.yml`) runs these with `RUSTFLAGS=-D warnings`; please run them
before you open a pull request:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

More, when your change touches them:

```bash
# Golden tests: after an intended behavior change, regenerate and review the diff
UPDATE_GOLDEN=1 cargo test -p omaspec-core --test golden
UPDATE_GOLDEN=1 cargo test -p omaspec-uml --test golden

# Comparison with the Java PlantUML (needs plantuml, a JRE and Graphviz installed)
OMASPEC_REQUIRE_PLANTUML=1 cargo test --locked -p omaspec-model --test oracle

# Headless screenshots of the editor, the preview and the gallery
OMASPEC_SNAPSHOT_DIR=/tmp cargo test -p omaspec-app snapshot
```

CI also runs `cargo deny` (`deny.toml`) on dependencies.

## Reporting a diagram drawn wrong

Use the **Diagram drawn wrong** issue form. The most useful report has:

1. **The smallest source that still shows the problem.** Remove lines until the fault
   goes away, then put the last one back.
2. **A screenshot** of what Omaspec draws (Export or a screen capture).
3. **What you expected**: a sentence, or a picture from the Java PlantUML or
   mermaid.live for comparison.
4. The output of `omaspec --version` and your Omarchy version.

Please keep private data out of diagrams you post; the repository and its issues are
public.

## Code style and rules

The rules in [AGENTS.md](AGENTS.md) apply to every change. In short:

- Rust 2024; `unsafe_code` is forbidden workspace-wide; rustfmt and clippy clean.
- No network access at run time, no telemetry.
- `omaspec-core`, `omaspec-uml` and `omaspec-model` do no I/O and have no GUI dependency.
- Every SVG from an engine goes through `omaspec_render::sanitize`.
- `vendor/` changes stay small and are listed in `vendor/README.md`.
- Docs, code and commit messages in English. Commit messages: `feat:`, `fix:`, `docs:`,
  `chore:` with a lowercase summary, e.g. `fix: a reader that stops early is no error`.

Larger design decisions are recorded in [docs/adr](docs/adr); the plan is in
[ROADMAP.md](ROADMAP.md).

## Pull requests

1. For anything larger than a small fix, open an issue first so we can
   agree on the approach.
2. Branch from `main`, keep the change focused, add or update tests (golden files
   included, reviewed).
3. Make sure fmt, clippy and tests pass locally; CI must be green.
4. Fill in the pull request checklist. By contributing you agree that your work is
   licensed under the MIT license of this project.

## AI-assisted development

Omaspec is developed with the help of AI coding agents; `AGENTS.md` (and `CLAUDE.md`,
which points to it) hold the instructions those agents follow. Every change is reviewed
and tested by the maintainer before it is merged. Contributions made with AI tools are
welcome under the same rules as any other: you are responsible for the change, you
understand it, and it passes the checks above.
