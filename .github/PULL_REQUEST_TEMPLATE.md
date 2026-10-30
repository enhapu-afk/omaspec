<!-- What does this change, and why? Link the issue: Fixes #123 -->

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets` is clean
- [ ] `cargo test --workspace` passes
- [ ] Tests added or updated; regenerated golden files reviewed (`UPDATE_GOLDEN=1`)
- [ ] Follows the rules in `AGENTS.md` (no `unsafe`, no network at run time, no private data)
- [ ] Commit messages use `feat:` / `fix:` / `docs:` / `chore:` with a lowercase summary
- [ ] Docs, README or ROADMAP updated if behavior changed
- [ ] For a rendering change: a before/after screenshot below
