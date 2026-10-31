# 0016: Lines by hand live in the source

Status: accepted, 2026-10-31. Takes up "Lines by hand" from the roadmap's
[Next](../../ROADMAP.md#next).

## Context

The routers lay every line ([0011](0011-curved-lines.md),
[0012](0012-lines-like-graphviz.md), [0013](0013-containers-like-graphviz.md)),
and sometimes a writer knows a better way: around a box rather than past it,
off another line, clear of a label. Graphviz takes no hints of that kind,
and neither PlantUML nor Mermaid has a way to write one down.

## Decision

1. **The bends are visible source.** A block of comments at the end of the
   diagram holds them, as a skin pack holds a look
   ([`skin.rs`](../../crates/omaspec-core/src/skin.rs)): it travels with the
   file, its copies, git and the images that carry their source. Java
   PlantUML, Mermaid and mmdr read only comments in it, so the file stays
   what it was everywhere else.

   ```plantuml
   Idle --> Running : start
   Running --> Idle : stop

   ' lines by hand (Omaspec)
   ' Idle -> Running via 0.5 -60
   ' Running -> Idle via 0.3 40, 0.7 40
   @enduml
   ```

   In PlantUML the block stands before the `@end…` line; in Mermaid it is
   `%%` comments at the end, inside a code fence if there is one. A blank
   line goes before it.
2. **A line is named by its ends.** `From -> To` with the names the lines
   use (an alias where there is one), and `#2`, `#3` … for the second and
   later lines between the same two, in the order of the source. A name with
   a space or a quote is written in double quotes. A source line number
   would change with every line written above it.
3. **A bend is placed relative to the two ends.** `via 0.5 -60` is half way
   from the middle of the box the line leaves to the middle of the box it
   enters, then 60 units to the side: positive to the right as the line
   travels, negative to the left. When the diagram grows, both ends move and
   the bend moves with them; when they move apart, the bend stays at the same
   share of the way. A line from a box to itself counts both numbers in
   units from the middle of the box, to the right and down.
4. **Only the line changes.** The layout runs as before: ranks, order and
   boxes do not move for a line by hand. Then each such line is laid again
   in the coordinates of the finished picture, one run from its start to
   the first bend, to the next, to its end, each by the router that runs
   lines between containers (`graph::route`), so it still goes around every
   box. Its label is placed again along it. Curves
   ([0011](0011-curved-lines.md)) and `linetype ortho` hold as before.
5. **A bend that no longer fits is left out, and said.** A bend that lies in
   a box after an edit is skipped; a line the router finds no way for
   through its bends is drawn as if it had none. Lint names both, and an
   entry whose line is gone.
6. **The writer drags with Alt.** In the diagram pane of the editor and in
   the editor's diagram window, Alt shows the line under the pointer in the
   accent color with its bends as handles; Alt and a drag on the line puts a
   bend there, on a handle moves it. Alt and a double click on a handle, or
   dragging it back onto the straight way, takes it out. Escape stops a
   drag. Each drag is one undo step. A drag without Alt pans, as before;
   Hyprland moves windows with Super, so Alt is free. Commands reset the
   line at the caret and every line.
7. **The preview window only shows them.** `omaspec preview FILE` follows a
   file that another editor writes, so it draws lines by hand but does not
   write the file.
8. **Graph diagrams only.** State diagrams (PlantUML and Mermaid),
   structure diagrams (class, component, use case, deployment, object,
   Mermaid class) and Mermaid flowcharts: the types whose lines come from
   `graph`. Sequence, activity and timing diagrams keep the lines their type
   gives them. When Java PlantUML or mmdr draws instead, the block is left
   out and the status line says so.

## Why

Visible source over a side file: a side file is lost on a copy, a rename or
an export, and a reviewer does not see it in a diff. Relative bends over
absolute ones: absolute coordinates break with any edit that moves a box,
which is most edits. A bend relative to the nearest box would follow the
box it goes around, but jumps when another box becomes the nearest. Alt over
a plain drag: a plain drag on the paper pans, and a line is easy to hit by
accident in a dense diagram.

## Consequences

- `omaspec_uml::hand` reads and writes the block; `omaspec_core::hand` turns
  a drag into an edit of the source; lint checks entries.
- The diagram map names every line of the graph types by its key, with its
  drawn points and its bends, so the pane can find the line under the
  pointer. A play follows a line by hand, as it follows the drawn curve.
- `convert` carries the block over, since the keys are names; `diff` and
  the behavior model ignore it; `fmt` keeps it at the end.
- Not now: moving where a line meets its box, moving a label by hand, the
  routed line while dragging rather than after, signal-flow diagrams, and
  boxes by hand.
