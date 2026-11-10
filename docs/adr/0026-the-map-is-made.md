# 0026: A project's map is made, not written

Status: accepted, 2026-11-09. Replaces decision 2 of
[0018](0018-diagrams-that-link.md) and its amendments *a map in a look of
its own* and *maps within maps*: a mind map or a WBS is no map any more.
Built in steps; what is built is marked in the
[roadmap](../../ROADMAP.md), *the map, made*: the map is made, shown
everywhere and laid out by its arrows, and the written one is gone.

## Context

Since 0018 the map of a project was a mind map or a WBS: the writer lists
the names of the project's diagrams in its boxes, and each box that names
one is drawn as a tile. A week of using it showed what that costs.

- **A plan was taken for a map.** Every mind map and every WBS that was a
  file was drawn in the map's look, *Map · 0 diagrams on it*, also a lone
  file in no project, and an example from the gallery opened in a window
  whose folder is a project. A mind map lost its two sides, a WBS its
  boxes. A rule to tell the two apart (the project's `main`, a comment
  line `' map (Omaspec)`) was built and left: a comment that changes what
  a file is, is hidden.
- **The writer kept a list the program has.** A diagram that is in the
  folder and not in the list hangs below the map as *not on the map yet*,
  with the line to type. A name two diagrams go by (*2 diagrams go by this
  name*) shows no picture.
- **The tree is invented, the arrows are found.** What ties the diagrams
  of a project together is that a scenario plays against a machine, a
  state opens a diagram, a diagram includes another. The map laid those
  over a tree that knows nothing of them, so its arrows ran under cards
  until they were led around them, on lanes and by the router.
- **What another tool draws of it is little**: a tree of names, without
  pictures and without arrows.

No project has to be kept as it is: those there are, are examples.

## Decision

1. **The map of a project is made from the project.** Its diagrams are
   the diagram files in its folder; each is a tile that shows it, by its
   file, so none is missing and no name is matched. Nothing is written
   for it: no `system.puml`, no line to place a diagram.
2. **A mind map and a WBS are diagrams like any other.** They draw as
   PlantUML and Mermaid draw them, in a project too; on the map each is a
   tile, and its boxes are its own words: they place nothing and open
   nothing.
3. **A folder groups.** In a project of up to 36 diagrams the map shows
   all of them, a folder as a frame around its diagrams. In a larger one a
   folder is a tile that opens its own map and shows how much it holds, a
   small box for each diagram in it; an arrow to or from that tile stands
   for the arrows of the diagrams in it.
   So a map shows one level of a large project; a folder's map is made
   the same way. This is what *maps within maps* was for, without a file
   for each.
4. **The arrows are read from the diagrams**, as before: a scenario plays
   against the one machine a participant names, a state opens the one
   other diagram that goes by its name, a diagram includes a file. For a
   diagram inside a folder's tile they are read from its text without
   drawing it, which tells the first and the last of the three.
5. **The project's file stands for its map.** `omaspec render
   omaspec-project.toml` draws it, as the MCP server does for that path;
   the editor and the preview show it beside that file, which names the
   project and may come to say what the map cannot find out by itself (an
   order, a group that is no folder). A folder's tile opens the folder's
   map in the same tab, the trail naming the way back. `omaspec render
   DIR` writes the map as `omaspec-project.svg`, which `render --check`
   and `check` hold against the project; an export writes it as
   `index.svg`, and the map of each folder that is a tile as `index.svg`
   in that folder.
6. **The map is laid out by its arrows**, on the layout every diagram of
   boxes and lines has (`omaspec_uml::graph::open`,
   [0013](0013-containers-like-graphviz.md)): a card is a box, a folder a
   container that opens when a line runs into it, and a diagram stands
   above the one it plays against, opens or includes. The words of an
   arrow are a pill that is a box too, between the cards: the arrows that
   arrive alike at one card meet in it and go on as one line with one
   head, and so do those that leave one card alike. So no line runs under
   a card and no words lie on a line, without a router of the map's own.
   What a layered layout would make a row of, the map packs itself: each
   group of joined cards is laid out on its own, and the groups, the
   folders no line runs into and the cards no arrow reaches stand beside
   and below one another, about as wide as a screen; more than four cards
   that lead to the same diagrams are a block with one line, as twenty
   scenarios of one machine.
7. **The drawer takes boxes, not files** (`omaspec_uml::tree::Branch`,
   `render_branches`): words, a key, the boxes in it. The project feeds
   it folders and diagram files. A tool for another language, whose one
   file holds many elements in packages, would feed it those; Omaspec
   itself stays with PlantUML and Mermaid.

## Why not

- **A block of its own, `@startmap`**, written by hand. It ends the
  confusion with a WBS and keeps the list, its gaps and its matching of
  names. `@startflow` shows Omaspec may have a block of its own; a map is
  not worth one, since all a map says can be found.
- **Every diagram on one map, always.** 2,000 pictures as vectors took
  1.1 s to draw and 3.9 s to parse (0018, amended 2026-11-05).

## Consequences

- The map can no longer be arranged by hand: its arrows and its folders
  place the cards, and the files' order those that nothing joins.
- A scenario stands above the machine it plays, not beside it: all
  arrows lead down, so those into one card can share a head. The tree
  with its curves, the lanes below a row and the routing around cards
  that 0.9.1 gave the written map are gone with it.
- A box no line reaches has no place of its own in a layered layout. In
  a folder a line runs into, what that line does not reach is therefore
  packed and laid at the right inside the folder's frame, and the frame
  is laid out again where that is higher than its boxes.
- `main` in `omaspec-project.toml`, the map `omaspec new` writes, the
  hint *not on the map yet*, and the drawing of a mind map or a WBS as a
  map are gone. A project file that still has `main` is told so, with its
  line. No diagram may be `index.svg` in an export, in any folder: that
  is the map's name there once the folder is a tile, decided before
  anything is drawn.
- A tile has no line in a source: Ctrl+click on one opens its diagram, as
  a click does, and `refs` and `rename` know no *box*.
- A folder's tile does not show its map small: drawn for 40 folders of 50
  diagrams, those made the map 4.8 MB and took it 250 ms. With a box for
  each diagram the map of 2,000 diagrams is 0.3 MB and drawn in 40 ms
  (`omaspec render`, release, the process aside; the written main map
  took 65 ms).
- An export has no source to carry for a map: it is drawn from the
  project again.
