# 0023: A diagram may be drawn by hand

Status: accepted, 2026-11-06. Reverses the native engine's decline of
`handwritten` ([0004](0004-native-plantuml.md), track N of the roadmap)
and the removal of the drawn skin pack ([0003](0003-mermaid-mmdr.md),
Phase 5 findings); the last step of 0.9 in the
[roadmap](../../ROADMAP.md), with focus and collapse
([0024](0024-focus-and-collapse.md)).

## Context

Both languages have a sketched look. PlantUML's `skinparam handwritten
true` wobbles every line and shape; Mermaid's `look: handDrawn` draws its
shapes with rough.js, in the front matter's `config` or in an
`%%{init}%%`. People use it for a draft: a picture that says *not final*
before anyone asks, and for slides, where the plain look reads as a
finished specification.

Omaspec declined both. A PlantUML diagram with `handwritten` went to the
Java PlantUML or showed the reason; a Mermaid diagram drew in the plain
look. The one pack that offered a sketch only picked a handwriting font
and was removed.

A sketch drawn by chance is a problem of its own. rough.js seeds its hand
anew for each drawing unless told otherwise, so the same file draws a
different picture each time: an export changes with every run, a golden
test cannot hold it, and `render --check` reports every diagram as
changed.

## Decision

1. **Both ways of asking draw by hand.** `skinparam handwritten true` in
   every PlantUML diagram Omaspec draws (sequence, activity, state,
   structure, timing, Gantt, mind map, WBS, JSON and `@startflow`), and
   `look: handDrawn` in every Mermaid diagram it draws (flowchart,
   sequence, class, state, mind map, Gantt, pie, journey, timeline, ER, C4,
   architecture, block), in `%%{init}%%` or in the front matter's
   `config`; the init wins, as it does in Mermaid. A front matter that
   holds a `config` with only a `look` (`classic` or `handDrawn`) no longer
   sends a diagram to mmdr. A diagram mmdr draws stays plain.
2. **Shapes, not text.** Every line, border and fill goes through the
   hand; the words stay as they are, so a sketch reads as well as the
   plain picture, and the click map, the lights of a play and Ctrl+click
   stay where they were. Neither PlantUML nor Mermaid changes the font
   either.
3. **Drawn as rough.js draws.** Each run is a slightly bowed curve, drawn
   twice, its ends a little off; a short run strays less (a tenth of its
   length at most), a long one bows less. Curves, rounded corners and
   circles go through points a little off their place. A box of straight
   sides draws each side on its own, past its corners, as a hand draws a
   box; a rounded one in one stroke, so its corners stay round. A line
   keeps its two ends where they are, so it meets its box and its
   arrowhead. A fill is a little off its outline. A dashed line is drawn
   once, as two dashed strokes would not meet. A shape with no border, a
   label's ground among them, keeps its form.
4. **A steady hand.** What is off, and how far, follows from a seed: the
   element's name where the drawer knows it (states, classes and the other
   boxes of a structure diagram, flowchart nodes and subgraphs), the
   shape's own size and form elsewhere, never where it stands. The same
   source draws the same picture, a box keeps its strokes when the layout
   moves it, and two boxes of one size look apart.
5. **One place.** The hand lives in the canvas every engine draws with
   (`omaspec_uml::sketch`, turned on by `Canvas::sketch`), so a new drawer
   draws by hand by turning it on; `Canvas::named` gives its boxes their
   names. The sanitizer sees paths, as it sees every other shape.
6. **A skin pack.** *Skin: sketch* in the palette writes
   `skinparam handwritten true` or `%%{init: {"look": "handDrawn"}}%%`
   into the source, marked as the other packs are, so it travels with the
   file and a Java PlantUML or Mermaid draws it sketched too.

## Why

The look is what the writer wrote down; drawing it plain, or handing it to
Java for that one line, ignores it. Seeded by name rather than by chance,
the sketch is a picture like any other: exports, `render --check` and the
goldens stay exact. Rough.js's way is the one Mermaid's readers know, and
it is cheap: a few curves per shape, no font of its own. Text left plain
keeps every measure of the layout, the legibility checks and the places a
click finds.

## Consequences

- `omaspec_uml::sketch` draws the strokes and fills and reads an SVG path
  into outlines (lines, cubics, quadratics and arcs, absolute and
  relative); `Canvas` turns each shape into them when sketched.
- The skins of the PlantUML drawers read `handwritten` (`shadowing true`
  still declines); the Mermaid drawers take `look` through
  `head::hand_drawn`, as a skinparam or their look's `sketch`.
- A sketched SVG is two to four times the size of the plain one (the
  gallery's coffee machine 10 kB → 41 kB); drawing it takes the same
  milliseconds.
- `omaspec-render/tests/sketch.rs` draws every diagram of the repository
  and one of each Mermaid kind by hand: the same engine, the same words,
  the same picture twice, and a box that keeps its strokes when it moves.
- Not now: a sketched look for a project as a whole (`[look]` in
  `omaspec-project.toml`), a handwriting font, rough.js's hatched fills,
  and the maps of [0018](0018-diagrams-that-link.md), which draw their
  cards in Omaspec's own look.
