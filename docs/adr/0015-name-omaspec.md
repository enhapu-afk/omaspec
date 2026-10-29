# 0015: The name Omaspec

Status: accepted, 2026-10-29. Supersedes the name of [0014](0014-name-omagram.md);
the mark stays.

## Decision

The tool is **Omaspec**: the binary `omaspec`, the crates `omaspec-*`, the
Wayland app ids `omaspec` (editor) and `omaspec-preview`, the environment
variables `OMASPEC_*`, the desktop entry, the MIME file and the AUR packages
`omaspec`, `omaspec-bin` and `omaspec-git`. It keeps its data in
`$XDG_STATE_HOME/omaspec` and `$XDG_DATA_HOME/omaspec`, and the help folder
is `Help Omaspec`. The repository is `github.com/enhapu-afk/omaspec`.

Release 0.4.0 still went out as Omagram. Images it exported carry their
source under `urn:omagram:source` and `Omagram file`; Omaspec reads those too
and writes its own. A skin block it marked `(Omagram)` is replaced like any
other, since only `skin: <pack>` is read. Its session in
`~/.local/state/omagram` and an `omagram.toml` from the theme template are not
carried over.

## Why

- In the Omarchy community "Omagram" already names Telegram clients
  (Omarchy and Telegram): three projects on GitHub, all older than this one,
  one of them installing a binary `omagram` and an `omagram.desktop`. A
  fourth Omagram would confuse people and collide on the same machine.
- "Oma-" still says where the tool belongs; "-spec" says what sets it apart
  from other diagram editors: it reads a diagram as the specification of a
  behavior and checks, plays and traces it
  ([0007](0007-behavior-specifications.md)).
- On 2026-10-29 no package, crate or repository was named omaspec (AUR, Arch,
  crates.io, npm, PyPI, GitHub), and no trademark of that name was found.
