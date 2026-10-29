# 0014: The name Omagram, and its mark

Status: accepted, 2026-10-28; the name superseded by [0015](0015-name-omaspec.md) on 2026-10-29, the mark kept.

## Decision

The tool is **Omagram**: the binary `omagram`, the crates `omagram-*`, the
Wayland app ids `omagram` (editor) and `omagram-preview`, the environment
variables `OMAGRAM_*`, the desktop entry, the MIME file and the AUR packages
`omagram`, `omagram-bin` and `omagram-git`. It keeps its data in
`$XDG_STATE_HOME/omagram` and `$XDG_DATA_HOME/omagram`, and the help folder
is `Help Omagram`.

The mark (`packaging/omagram.svg`) is what the tool does: lines of source on
the left, the diagram they make on the right, on a dark tile in Tokyo Night
colors. It reads at the 14 px of the status line.

## Why

- "Oma-" is how tools around Omarchy are named (Omakub, Omarchy itself), so
  the community sees at once where the tool belongs; "-gram" says that it
  draws diagrams.
- The mark shows the one thing every diagram tool of this kind does, text
  becoming a picture, rather than a single kind of diagram.
