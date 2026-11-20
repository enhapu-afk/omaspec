# 0035: A capability map of its own

Status: accepted, 2026-11-20. Follows [0034](0034-a-story-map-of-its-own.md).

## Context

A capability map shows what a business can do as tiles inside tiles,
each colored by a value given to it: how mature it is, what it costs,
how much risk it carries. It is the picture an architecture is planned
on. A WBS has the tree but no values on a scale and no legend; a
treemap sizes its tiles by a number, where a capability map keeps them
even so that every one can be read.

## Decision

1. **A block of its own**, `@startcapabilities`, among
   `kind::OWN_BLOCKS`: never sent to the Java PlantUML, and lint says
   that no other tool draws it (`omaspec-only`).
2. **Stars for the depth**, as in a mind map or a WBS: `* Area`,
   `** Capability`, `*** deeper`, up to six levels. A star more than one
   level under the line before is refused on its line. `omaspec fmt`
   leaves the block as written, as it does a mind map's.
3. **One scale**, `levels Maturity : none, basic, managed, leading`,
   written once, anywhere; a value after a capability's last colon names
   one of its steps (`** Sales : managed`), or the line is refused with
   the steps there are. A step may carry a color of its own
   (`low #2F9E44`); without one the steps run from near the page to the
   theme's accent, so the map follows every theme and no color says
   "good" or "bad" where the writer did not.
4. **Read without the colors too.** A capability with a value counts its
   step in dots under its name, filled up to it; one with neither a
   value nor capabilities of its own has a dashed edge, and the legend
   names it *not rated*. A capability that holds others has its step's
   color under its name alone, so what it holds still reads on it.
5. **Areas as columns.** The areas stand side by side, a few in a row
   where there are many, each as high as the highest of its row; what
   is in an area stands one under the other, and deeper capabilities side
   by side in as many columns as fit.

## Consequences

The map is read and drawn in `omaspec-uml/src/capabilities/`, and shares
its look with the story map (`own_skin`). A second scale at once (cost
beside maturity) would be a second `levels` line and a choice of which
one colors the map; it is not there yet.
