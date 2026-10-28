# 0009: Behavior compared, and written in the other language

Status: accepted, 2026-10-21. Follows [0007](0007-behavior-specifications.md).

## Decision

Two features stand on the behavior model of `omagram-model`, which reads
PlantUML and Mermaid alike:

- **A semantic diff** (`omagram_model::diff`) tells what changed in the
  behavior between two versions of a diagram, also between the languages.
  For a state machine: states new, removed, moved into another state or
  region, of another kind; transitions new, removed, with another target,
  other events, another guard or action. For a sequence diagram:
  participants, messages (another text, now a reply) and the blocks
  around them. Then what the specification checks find now, or no longer:
  a change that leaves a state unreachable says so. Layout is not
  behavior: moved or re-indented lines, colors, notes and shown labels
  change nothing.
- **A conversion** (`omagram_model::convert`) writes a state machine or a
  sequence diagram in the other language, from the parse the checks use.
  The result is read back and compared with the original by the diff; if
  the two would not mean the same, nothing is written. Behavior the other
  language cannot say refuses, with its line: a history state in Mermaid,
  a `ref`, a `group` or a message from the edge in a Mermaid sequence. What
  it cannot say about the looks is left out and told as a notice: colors,
  skin parameters, notes above or below a state, the direction of a nested
  state, the shapes of participants, dividers and delays.

Both are in `omagram diff` and `omagram convert`, in the MCP server
([0008](0008-mcp-server.md)), and in the editor: Compare shows B as the
changes in behavior, and a command writes the tab in the other language in
a new tab.

## How

- States and participants go by their names, a start or an end by the
  state and region it sits in. Transitions are paired in rounds, from the
  closest match to the loosest (same ends and trigger; same source, events
  and guard; same ends and events; same source and events; same ends), so
  that a changed transition is told as one change, not as one removed and
  one added. Messages are aligned with the line diff's algorithm, and a
  message that kept its ends at its place changed.
- A finding that names a line (`first on line 12`) is the same finding on
  another line, so moving lines finds nothing new.
- The writers declare every state in its own container first, so that both
  readers make it there, and write transitions between containers last, at
  the top, where both languages take states from anywhere.
- A name the other language does not take as it is (`"Long wait"` in
  Mermaid, a participant with a space) becomes a plain id; its box or head
  still shows the name, and a notice says so.
- `@startuml Name` becomes the Mermaid title when there is none, so
  sequence diagrams still find the machine by its name.
- `omagram diff FILE` compares with the version in the last commit
  (`--rev` for another); a revision that looks like an option is refused
  before git sees it. The exit code is 0 for the same behavior, 1 for a
  change, as `diff` does.

## Why

Specifications change all the time, and a line diff tells a reviewer which
lines moved, not what the system now does. A one-word change can make a
state unreachable; a reordered file changes nothing. The diff says which.
The conversion lets a specification live where it is read, Mermaid in a
README, PlantUML in a company wiki, without retyping it and without a
silent loss: what is written means the same, or is not written.

## Not covered

- A renamed state or participant is one removed and one added.
- Descriptions, notes and styles are not compared.
- Activity diagrams and flowcharts are neither compared nor converted.
