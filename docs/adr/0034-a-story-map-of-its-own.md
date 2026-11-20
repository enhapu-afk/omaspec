# 0034: A story map of its own

Status: accepted, 2026-11-20. Follows [0010](0010-signal-flow.md), the
first diagram of Omaspec's own syntax.

## Context

A story map lays out the backbone of a product: what its users do
across the top, the steps of each under it, and under each step the
stories, cut into releases by bands across the whole map. Neither
PlantUML nor Mermaid draws one. A kanban board comes near, but it has
no backbone and no bands across its columns; a WBS has the tree, but
not the release that cuts across every branch of it.

## Decision

1. **A block of its own.** `@startstorymap` to `@endstorymap`, as signal
   flow is `@startflow`: a file that says what it is, which PlantUML
   would refuse at once rather than draw as something else. A line the
   engine declines is never sent to the Java PlantUML
   (`kind::OWN_BLOCKS`), and lint says on the `@start` line that no
   other tool draws it (`omaspec-only`, a hint).
2. **Keywords and a bullet.** `activity Name` and `step Name` open the
   backbone, `* words` is a story under the step before it, or under its
   activity before its first step. The indentation is the writer's
   (`omaspec fmt` sets it); it is not read, so a story cannot land under
   the wrong step by a space too few. A line that is none of these is a
   mistake, refused on its line.
3. **A release at the end of the story.** `* By card : MVP` puts the
   story into the release `MVP`, as PlantUML puts a label after a colon:
   the story stays under its step, where it is read, and its release is
   one word away. The words after the last colon outside quotes name the
   release; a story with a colon of its own goes in quotes. A release
   is written once, anywhere in the map (`release MVP : what it is
   for`), and its order is the order of the bands; a story that names
   none that is written is refused, with the releases there are. A
   story without a release stands in a band after them, `not planned`.
4. **Drawn as the theme draws.** An activity is filled with the
   rectangle's border, the theme's accent; a step is a rectangle, a
   story a note; every other band is tinted. A theme pack colors the
   map without knowing it.
5. **Held as it scrolls.** The map says where the backbone is and where
   the releases' names are, so a pane holds the backbone at its top and
   the names at its left (REQ-073), and every card's words are tied to
   their line.

## Consequences

The map is read and drawn in `omaspec-uml/src/storymap/`. Stories have
no estimate, owner or state yet; a color by state, if wished, is a
skinparam or a tag after the release, not a new kind of line.
