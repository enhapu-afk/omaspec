# 0002: Editor widget scrive

Status: accepted, 2026-10-15 (spike `spikes/editor`)

## Decision

The editor uses [scrive](https://github.com/robbym/scrive) (`scrive-core` +
`scrive-iced` 0.3), an iced 0.14 code editor widget.

## What the spike showed

| Need | scrive | iced-code-editor 0.5 |
|---|---|---|
| Own grammar (PlantUML, Mermaid) | yes, `.sublime-syntax` | no: fixed syntect set |
| Token colors from the Omarchy theme | yes, `.tmTheme` built from the palette | no: fixed base16-ocean |
| Lint findings | diagnostics with severity, squiggles, scrollbar marks | only through an LSP process |
| Undo/redo, find/replace with regex, folding, multi-cursor | yes | yes |
| Gutter markers for "changed since save" | no | no |
| Vim mode | no | yes |
| `unsafe` | `forbid(unsafe_code)` | none found |
| Regex engine | fancy-regex (pure Rust) | syntect default |

Measured with the spike (release build, this machine): building the editor
with grammar, theme, first highlight and lint takes 37–47 ms for 500 to
50,000 lines; the highlight cache is windowed to the viewport, so the cost
barely grows with the file. Headless snapshots in matte-black, tokyo-night
and catppuccin-latte looked right; the window starts under Hyprland with the
class `omagram`.

## Risks

scrive is young: three releases in one week of July 2026, one maintainer, few
downloads. Mitigation:

- Pin the version. The crate is MIT and small enough (about 33k lines with
  tests) to vendor and maintain if it stalls.
- Gutter markers for "changed since save" are missing. Plan: propose a line
  marker decoration upstream; fall back to a fork if it is not wanted.
- No vim mode. iced-code-editor has one; revisit if users ask for it.

## Phase 4 findings

Building the editor window on scrive 0.3.0 as published:

- `CodeEditor` owns its `Document` and hands out only `&Document`, so
  `mark_saved`/`is_dirty` are out of reach; Omagram compares the text with
  the saved text instead (cheap at diagram sizes).
- There is no way to move the caret and scroll it into view from the app:
  `PlaceCaret` does not scroll, and the scroll-to-caret request is private to
  the core. Jumping to an outline entry or a render error needs that.
- The widget keeps its scroll position in iced's widget tree, not in the
  document. Tabs therefore keep every editor in the tree and give the hidden
  ones zero height, each with its own widget id.
- The find bar, popups and bracket colors use fixed VS Code dark colors; they
  stay readable on light Omarchy themes but do not follow the palette.
- Ctrl+Enter is the widget's "new line below"; a wrapper widget
  (`editor/keys.rs`) takes window shortcuts before the editor sees them.

The reveal call and line markers (for "changed since save") are small
additions to scrive. Decision (2026-10-16): Omagram builds scrive 0.3.0 from
a copy in `vendor/` with these additions (see `vendor/README.md`), instead of
waiting for an upstream release. The additions may be offered upstream later.
