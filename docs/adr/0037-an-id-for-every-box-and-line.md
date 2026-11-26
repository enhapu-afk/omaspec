# 0037: Every box and every line has an id of its own

Status: accepted, 2026-11-26 (REQ-132); its first step (point 7) is
done the same day, and of the second its sequence diagrams and the
lines of structure diagrams, flowcharts, mind maps, WBS and activity
diagrams, and ER, C4, architecture, block, kanban, journey, timeline,
flow, network, story map, timing, Gantt and capability diagrams. Replaces point 2 of [0029](0029-a-glide-by-parts.md),
the parts only for the pane, and the way its point 1 writes their names.

## Context

Mermaid's 13 alpha (2026-11-20) promises "a stable ID on every node and
edge". Omaspec's SVG has one id, `lines`, besides those of its gradients
and icons. A page around a picture cannot say "the state Idle" or "the
transition on `start`" to it: not in CSS, not in a script, not in a link
(`door.svg#…`). Since `--id-prefix` (REQ-117) several pictures stand on
one page, and there the wish comes up first: light the state a trace is
in, mark what `omaspec diff` found, link a story's step to its message.
An agent that checks a picture would read it by the same names.

The engine knows what it draws for which element. The pane asks for that
already ([0029](0029-a-glide-by-parts.md)): with `' parts (Omaspec)` each
part lies in groups `<g id="part-…-N">`. Those names do not serve a page:

- they are hex (`part-73746174653a49646c65-4` is `state:Idle`), with a
  number counted over the whole picture, so they change when anything
  before them is added;
- some are places in the tree, not names (`box:2.0`, `mark:0`,
  `words:3:1`), and change when a box is added before them;
- a line has none: its words are a part, its path lies in `lines` with
  every other path;
- a render or an export has none at all.

## Decision

1. **An element is named by what it is and what the source calls it.**
   `state-Idle`, `class-Order`, `participant-Alice`, `node-A` (a
   flowchart's), `package-Shop`. The word before the name is the
   element's kind as the diagram writes it, from one table in the engine,
   so a package and a class of one name differ, and no element's id meets
   one of the engine's own (`lines`, `grid`, `icon`, a gradient's).
2. **A line is named by what it joins and what it says.**
   `transition-Idle-Busy-start`, `message-Alice-Bob-hello`,
   `link-Order-Customer` (without words, the ends alone). A line whose
   ends and words an earlier one has takes `-2`, `-3`, in the order
   written. A transition so has the name it has for `omaspec diff` and
   for `omaspec trace`, which know it by where it starts, its event and
   where it leads: a step of a trace, or a line `diff` reports, is found
   in the picture by its id.
3. **What the source does not name is named by what it belongs to.** A
   start and an end by the state they are in (`start-Brewing`, `end`
   for the diagram's own), a note by what it is on, its side and its
   number there (`note-state-Idle-right-1`), the title, header, footer
   and legend by what they are (`title`). Nothing is named by its place
   in the layout or in the order of drawing. A folded container keeps
   its id; what it holds is not drawn and has none.
4. **Written so every id is valid in XML and CSS.** Letters, digits, `_`
   and `.` stand as written; any other character is `_`, its code in
   hex and `_` (a space is `_20_`): `Order parts` is
   `state-Order_20_parts`. Two that still meet are told apart by `-2`.
   The ids of a source are the same on every render, byte for byte.
5. **One id on the shape, one class on all of it.** An element draws on
   several layers: its box under the lines, its words over them. The
   group of its shape carries the id; every one of its groups, that one
   too, carries it as a class. `#state-Idle` is the element to link to
   or to find, `.state-Idle` all of it to style. `--id-prefix` prefixes
   both, so two pictures on a page still share nothing.
6. **Every picture, the pane's too.** A render, an export, a film and
   the pane draw these groups; the parts of 0029 are these groups and
   their names. The hex, the counted number and `' parts (Omaspec)`
   go; the map's `Spot`, `Place` and `Wire` carry the new names. Every
   golden SVG changes once, in one commit that changes nothing else.
7. **The names first, then what has none.** In one step every drawer
   that marks parts today (state machines of both languages, structure
   diagrams, flowcharts, activity diagrams, trees, JSON, git graphs)
   writes them in this form and in every picture, and the pane reads
   them: there is never a second naming to keep. A state machine's lines
   take their ids in that step too, as the trace and the diff need them.
   Then, drawer by drawer, the lines of the others and the kinds that
   mark no parts yet, sequence diagrams (participants, messages, notes,
   fragments) first. The guide lists the kinds that carry ids.

## Why

A name from the source is what stays when the layout moves, which is when
a page around the picture needs it most; a counted or placed name moves
with the layout. Naming a line by its ends and its words, rather than by
its number, keeps every other line's id when one is added, and it is
the name Omaspec's behavior model already uses for a transition, so a
trace and a diff need no table to find their lines. A class beside the
id keeps one id per element though its drawing lies on several layers,
which the lines between them need. One naming for the pane and the page
leaves one thing to test where there are two now.

Measured on a state machine of seven states, 16 part groups made its SVG
682 bytes longer (1.4 %), with hex names longer than these.

## Costs

- Every golden SVG changes once; the size of a picture grows by about a
  percent.
- An id changes when what it is named by changes: renaming a state
  renames its id and its transitions', and a transition's event its own.
  That is the point, and it is said in the guide.
- The pane's glide reads its parts by the new names; the promises of
  0029 (`paper/tests/promises.rs`) hold it to the same frames.

## Not now

- `data-from`, `data-to` or a line's source line on the groups: the map
  ties words to lines already, and a page finds a transition by its id.
- Ids for what the Java PlantUML draws: its pictures keep what it writes.
- Ids for kinds no drawer names yet (point 7).

## Decided with it

- The kind words stand in full (`transition-`, `participant-`), as a
  page's CSS reads them.
- Every picture carries the ids, without asking, as Mermaid's promise
  is; no `render --ids`.
- The first step takes every drawer with parts at once, so the pane
  switches once; it is one change of names, in each drawer's calls of
  `Canvas::part` and in `part_name`, held by the glide's tests.
- A sequence diagram names its elements but is drawn as a whole, as
  before: its root says `data-omaspec="named"`, not `"parts"`, so the
  pane fades it, a build uncovers it from the top down and a play lifts
  its veil message by message. A participant's head, foot, lifeline and
  bars are its element, its id on its head; a note beside a message is
  one of its own. So a timeline's period and a journey's task have
  their id on their card (`Canvas::id_on`), not on the line under it.
  Whether messages should glide is a question of its own. The kinds
  that marked no parts before are named so too: drawn as a whole, as
  they were.
- A line many links share (a WBS's spine, the bar over a row, an
  activity diagram's run into a point where branches meet) is no one
  link's: it stays in the lines' group without an id, and each link's
  id is on the piece that is its own. An activity diagram's line is
  named by the shapes it joins without their kind
  (`link-green_3f_-Publish-yes`); its words keep the part of their own
  they glide in (`label-yes`).
