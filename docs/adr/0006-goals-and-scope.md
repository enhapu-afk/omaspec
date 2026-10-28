# 0006: Goals and scope

Status: accepted, 2026-10-18. Its scope is replaced by
[0007](0007-behavior-specifications.md); the goals stay.

## Decision

In the end Omagram draws every PlantUML and Mermaid diagram type itself.
Version 1.0 needs the behavior diagrams (sequence, activity, state) and
flowcharts, in both languages; the other types follow one at a time. Until a
type is Omagram's own, Mermaid goes through mmdr (compiled in, see
[0003](0003-mermaid-mmdr.md)) and PlantUML through the Java PlantUML if the
user installed it (see [0004](0004-native-plantuml.md)).

## Goals

Every change is checked against these.

- **Independent.** One binary. What 1.0 draws needs no Java, browser, Node
  or network. The Java PlantUML stays optional and confined.
- **Secure.** That independence is most of it: no network and no telemetry
  ([0005](0005-offline-no-telemetry.md)), no outside process for the 1.0
  types, every SVG sanitized, exports never read files an SVG points to,
  untrusted input fuzzed.
- **Fast.** Typical diagrams render in milliseconds and rendering never
  blocks typing. Timings are measured (`cargo bench -p omagram-render`
  and real diagrams) and kept in the roadmap, not assumed.
- **Friendly to AI agents.** What an agent needs to work on a diagram runs
  without a window: `lint` (exit codes, `--json`), `inspect` (JSON), `fmt
  --check`, `outline`, `extract`, and `render` to SVG or PNG, which an agent
  that reads images can look at. Errors read `file:line: error: …` and name
  the construct, so the line can be fixed. `render --json` prints the same
  fact as JSON, one object per diagram: where the image went, or the error
  and its line. `inspect` says, for PlantUML and for Mermaid, whether
  Omagram drew the diagram or a fallback did, and why. Output is
  deterministic. `AGENTS.md` lists the commands. A new feature that works
  on a diagram gets a command where one makes sense.
- **Keyboard, mouse and touchpad.** Each is a full way to work, none an
  afterthought. Every command has keys and is in the command palette;
  everything on screen can be clicked, dragged and resized; on a touchpad
  two fingers pan and a pinch zooms.
- **Beautiful.** It looks like part of Omarchy: the active theme's colors
  and font, switched live, with checked contrast. Diagrams are drawn
  cleanly: lines that curve or keep right angles as the type wants
  ([0011](0011-curved-lines.md)), nothing on top of anything else,
  labels readable. Where PlantUML or Mermaid draw something badly,
  Omagram draws it better rather than copying the flaw.

## Consequences

- Within one diagram type, closing what still falls back comes before the
  next type: a diagram should not change its look because one construct
  was added.
- A type counts as Omagram's own when it draws without the fallback and
  is at least as fast as the fallback was.
- A touchpad pinch is the Wayland pointer gesture and zooms at the pointer.
  Two-finger scrolling still pans. On a touchscreen, one finger pans and
  two fingers zoom at their midpoint.
