# 0019: Where a name is used, and renaming it everywhere

Status: accepted, 2026-11-05. Builds on [0007](0007-behavior-specifications.md),
[0017](0017-projects.md) and [0018](0018-diagrams-that-link.md).

## Context

A project's diagrams hold together by names alone: a scenario's participant
plays the state machine of its name ([0007](0007-behavior-specifications.md)),
a map's box and a state open the diagram of theirs
([0018](0018-diagrams-that-link.md)), a trace's object runs through its
machine. That keeps the files ordinary PlantUML and Mermaid, but a name
changed in one place quietly unties the others: the participant still
says *Door* after the machine became *Gate*, and nothing is checked
against it any more. With hundreds of diagrams nobody finds every use by
hand, and a text search finds too much (the event *open* beside the state
*Open*) and too little (*DoorControl* for *Door control*).

## Decision

1. **A use is a place that names one of three things**, compared as links
   compare names (letters and digits, case aside):
   - a **diagram**, by its `@start…` name, its title or its file name:
     the names it goes by (*diagram*), a map's box (*box*), a scenario's
     participant (*participant*), a state of another machine named like it
     (*link*), a trace's object or machine (*trace*);
   - a **state** of a machine: the lines of that machine that declare it or
     lead into or out of it (*state*), a trace's state for an object of that
     machine (*trace*);
   - an **event** of a machine: its transitions' triggers (*event*), a
     scenario's message to a participant that plays that machine
     (*message*), a trace's event for an object of it (*trace*).

   Diagrams in Markdown documents count as files of their own, on the
   document's lines.
2. **`omaspec refs NAME` and the MCP tool `refs` list every use**, by file,
   line and kind, the machine a state or an event belongs to, and the
   line's text; `--json` for agents. A name may be several things (*Idle* is
   a state of many machines): all are listed, and `--machine M` keeps to M's
   name, states and events. Exit 1 when nothing uses it.
3. **A rename renames one thing at every use.** `omaspec rename OLD NEW`,
   and *Rename everywhere* in the editor on the name at the caret. When
   OLD is several things, the rename asks for `--machine` (and `--state`
   or `--event` where a machine has both, as *Open* and *open*) rather than
   guess. A diagram's rename writes its title or `@start…` name, its boxes,
   its participants, and renames the states of other machines that link to
   it, since the link is their name. A state's rename touches its machine's
   lines left of a label, never a label; an event's only labels, in its
   machine and in the messages sent to its players. Each use is spelled as
   the old one was: words as written where it had spaces, else one word in
   its style (`FrontGate`, `front_gate`, `front-gate`, `FRONT_GATE`), so an
   identifier stays one. It is refused when the new name is taken by
   another diagram, state or event of that machine, or is the old one.
4. **Files keep their names.** A diagram called only by its file name
   cannot be renamed until it has a title (`title Door`), as a moved file
   breaks `!include`s, exports and the project file's `main`; when the
   file name still says the old name, the rename says so and how to move it
   (`git mv`).
5. **Traces are records, not specifications**: they are listed, never
   rewritten, and the rename names those that will no longer match.
6. **The editor**: an open document takes the rename as one undo step and
   stays unsaved; a file not open is written as a save writes it. The
   notice says how many files changed and what was left alone.
7. **A link to a diagram that is gone is a finding.** A link written into a
   diagram (`[[orders.svg]]`, `click A href "orders.svg"`) that points into
   the project, at a diagram's file or its picture, where no diagram is,
   is a warning of `omaspec lint`, the MCP tool `lint` and the editor. A
   document's numbered picture (`README-2.svg`) and an address that leaves
   the project are not this check's.

## Why

Names are the only glue the files have, so the tool that keeps them must
understand them as links do, not as text; the same comparison everywhere
means a rename finds exactly what `refs` lists and what the checks pair.
Spelling each use in its own style keeps PlantUML and Mermaid valid without
a parser that writes code back. Files, traces and `!include`s are left to
the writer, as moving or rewriting them reaches beyond the project's
specification.

## Consequences

- `omaspec_model::refs` finds the uses in a project's files without I/O;
  `omaspec_model::rename` plans the edits, line by line, so the command
  line and the editor apply the same plan; `named_at` tells what the
  caret names, `gone_links` the links to diagrams that are not there.
- The project's state machines (`Specs`) say which participant plays which
  machine; documents and traces are read when asked, not kept.
- Not now: a class, an activity's action or a flowchart's node is no
  name that links; files are not moved; a name inside a note's text or a
  comment is left as it is.
