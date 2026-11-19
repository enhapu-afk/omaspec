# 0033: Subclasses under one triangle, as a tree

Status: accepted, 2026-11-19 (REQ-083). Extends
[0022](0022-lines-bundled.md).

## Context

`skinparam groupInheritance 2` asks PlantUML to draw the lines up from
two or more subclasses into one parent as one: a single triangle on the
parent, the lines joining under it. The engine declined the skinparam, so
a class diagram that set it went to the Java PlantUML or failed. Without
it, each subclass has its own triangle side by side on the parent's
border, the outer lines curving in from the side.

Bundling (0022) does not cover it. A trunk there is the last 12 units
beside the box, too short for a triangle and the stem past it, and lines
that leave a box stay on their own, which in the layout's terms is every
line from a parent down to its subclasses. Graphviz, under PlantUML, adds
a point where the lines meet and gives it a rank of its own.

## Decision

1. **Lines of a tree.** A line carries a key and the end its stem is at
   (`graph::Tree`). The class diagram gives one to each line up into a
   box under a triangle, without words or multiplicities, alike in dash,
   color and width, when there are as many such lines into that box as
   `groupInheritance` says (at least two). `0`, the default, gives none.
2. **Room beside the parent.** The gap between the parent's rank and the
   rank its lines come from is at least 60 units (`join::room`), so the
   stem, the curves where the lines meet and the runs on to the boxes
   each have some.
3. **Joined after routing** (`join::trees`). Each line keeps its route up
   to the junction, 0.6 of the way into the gap. There it turns across to
   the stem: a line that runs straight into the parent, the one nearest
   its middle, or else the middle of its side. Every line of the tree
   ends on the same stem, so their triangles stack as one. A point 18
   units out on the stem keeps its last part straight: the curve where
   the lines meet would otherwise take the whole last run up to the
   triangle.
4. **No tree through anything.** The stem keeps 16 units from where
   another line ends on that side, so no head stacks on another; it
   tries the middle of the side first, then where its own lines end,
   then a step further out each time. Two trees into one side (the
   solid lines of `<|--` and the dashed ones of `<|..`) so stand side by
   side. If the run across or the stem would lie along another line or
   pass through a box, the junction tries a few other depths, and if
   none is free the lines stay as they were; a tree whose stem would
   then stand by their heads goes back to its own lines too.

## Why

Joining after the routes are laid leaves every box where the layout put
it, as bundling does (0022): a tree changes only the last part of its
lines and the room in one gap. A junction of its own in the ranks, as
Graphviz has, would move the boxes beside it and the lines through them.
With the junction past the middle, the stem is the longer part and reads
as one line under the triangle. The runs from the junction to the boxes
are short enough to curve softly. The check against other lines keeps
the guarantee of 0022 that no line hides under another.
