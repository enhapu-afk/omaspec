# 0024: A container folds to its box, and a box takes the focus

Status: accepted, 2026-11-06. Builds on
[0018](0018-diagrams-that-link.md) and [0013](0013-containers-like-graphviz.md);
the fourth step of 0.9 in the [roadmap](../../ROADMAP.md), *a layout you
can steer*, with the sketch look ([0023](0023-sketch-look.md)).

## Context

A large state machine nests its states three deep, a class diagram holds
its packages, a flowchart its subgraphs. Read whole, it is a wall; read in
the source, it has no shape. Graphviz-based tools let a reader collapse a
cluster; PlantUML and Mermaid draw everything, always.

A diagram that links to another ([0018](0018-diagrams-that-link.md))
already reads from the outside in: a state with a diagram of its own is a
box with two small squares on it, and a click glides into it. A container
is the same thing written in one file. And a reader following one state
wants to see where it leads and where it comes from, not the rest: what a
play does for the states it is in, without the play.

## Decision

1. **A container folds to its box, in the pane.** A composite state
   (PlantUML and Mermaid), a package, node or frame of a structure
   diagram, and a flowchart subgraph fold to a box of their own, with
   their name: what they held is gone, a line into it or out of it ends on
   the box, and lines that now run alike (the same ends, words and look)
   are drawn once. The folded box carries the two small squares of a
   linked state: there is more inside.
2. **The file never says so.** The pane keeps its folds per tab and hands
   them to the engine as lines after the diagram,
   `' fold (Omaspec) Brewing` in PlantUML and `%% fold (Omaspec) Brewing`
   in Mermaid, which every other reader takes for a comment. The answer
   carries the file as it is, so an export embeds the file's own source,
   while its picture is the one on screen. The checks, the play and lint
   read the file and never see a fold. A fold line a writer puts into a
   file folds there too, for `omaspec render` and exports.
3. **Folding by pointer and by caret.** Over a container the pointer
   shows a small button in its top right corner; a click folds it. A
   click on a folded box unfolds it, and the view moves to it when it
   comes back out of sight (a fitted view fits the new picture). In the
   editor the palette folds or unfolds the container whose name the caret
   is on, and unfolds every container; the preview window unfolds every
   container with `u`.
4. **The focus veils all but a box and its neighbors.** Shift and a click
   on a box puts it in focus: a veil, as a play lays one, over the
   diagram, and through it the box (with the accent's light round it),
   the boxes its lines lead to or come from, those lines and their words.
   Shift and a click on the box again, or on the paper, ends it; so do
   Escape in the preview window and the palette's command in the editor,
   which also starts it on the box at the caret. In the editor the focus
   follows the caret onto another box's name.
5. **A play shows the whole machine.** Starting a play unfolds every
   container first and plays once that picture is in; the focus steps
   aside while a play runs.
6. **The graph diagrams.** State diagrams of both languages, structure
   diagrams and Mermaid flowcharts and class diagrams: the types laid out
   by `graph`, whose map names each box. The map says which boxes hold
   others (`Place::frame`). Other kinds neither fold nor focus.

## Why

In the pane over in the file: a fold is how one reader reads, now, not
what the diagram says; written into the file it would change every
export and every colleague's view. Comment lines over a new channel to the
engine: every way a picture is drawn (the editor, the preview window,
`omaspec render`) takes a source, and the folds travel with it without a
field of their own through each; the line numbers of the file stay where
they are. Folding in the model before the layout over
hiding parts of the picture: the layout closes up around the box, so the
rest reads as one picture, with its lines on the box. The focus over
dimming in the SVG: it lies over the picture like a play's veil, costs no
drawing, and comes and goes at once.

## Consequences

- `omaspec_uml::fold` reads and writes the fold lines and folds the
  containers of state and structure diagrams (`state::Diagram::fold`,
  `structure::Diagram::fold`): it cuts the lines into a folded box short,
  keeps one of those alike, and leaves out the lines by hand and the
  holds that name what it held. `omaspec_render`'s flowchart folds a
  subgraph into a node of its id and title before its layout.
- The pane keeps `folds` and `focus`; the worker adds the fold lines
  (`Worker::render_folded`). `paper::fold` draws the marks and buttons and
  takes the clicks, `paper::focus` lays the veil and what comes through.
- `omaspec-render/tests/fold.rs` folds in each drawer; the editor's tests
  fold from the caret, unfold with a click, open the folds for a play, and
  move the focus with the caret; headless snapshots `focus` and `folded`.
- Not now: folding a sequence diagram's group, an activity's partition or
  a mind map's branch; a fold that the layout keeps in place (the boxes
  around close up); a focus that reaches further than one line; folds kept
  in the session.

## Addendum (2026-11-09): the lines glide too

When the boxes of a new picture glide to their places (`paper::morph`,
2026-11-08), the lines left with the old picture and came back on their new
routes once the boxes were nearly there: for a moment the diagram had no
lines, and a fold read as two pictures rather than one that changes shape.

Now the lines glide with the boxes. The engine draws the lines of a graph
diagram, with the marks at their ends, in an SVG group of their own
(`Layer::Lines`, `id="lines"`), and each `Wire` of the map carries its
stroke and its marks as polygons. The worker parses a graph diagram twice,
once as it is and once without that group (`omaspec_uml::without_lines`).
A glide waits until both the old and the new picture without lines are
rasterized (the old picture stays on screen meanwhile, as it does until the
first raster), draws the boxes as before over those, and draws each line
itself (`paper::wires`): a line in both pictures travels from its old route
to its new one, resampled to the same number of points, its marks with it;
a line that goes fades with the old picture, one that comes fades in with
the new. A line is matched by its key, the names of its boxes; where a
container folded, a box that left stands as the smallest box that stayed
and held it, so the lines into what it held travel onto the folded box, and
back out of it when it unfolds. A picture without lines that cannot be drawn
lets the glide start as before, with the lines in the pictures. Headless
snapshots `glide-start` and `glide` show a fold on its way.

Not now: the words on the lines, which fade with the pictures; the sketch
look, whose lines are drawn straight while they glide.
