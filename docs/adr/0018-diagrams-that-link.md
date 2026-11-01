# 0018: Diagrams link by name, and a project has a map

Status: accepted, 2026-11-01. Builds on [0017](0017-projects.md) and
replaces its decision 5: a new project's main diagram is a map.

## Context

A project ([0017](0017-projects.md)) is a folder of diagrams, and its
diagrams already find each other by name: a participant of a scenario its
state machine. What was missing is the way through them. The roadmap's
next item: a state links to the diagram that describes it, a click zooms
into it, zooming out goes back, so a specification reads from the whole
system down to a single step. And a project needs a place to start from: a
picture of all its diagrams, which says where each one sits and what links
to what. A mind map or a WBS, which PlantUML (and, as a mind map, Mermaid)
already draw, has the shape for that.

## Decision

1. **A box links by name.** A state of a state machine, or a box of a mind
   map or a WBS, links to the project's diagram of that name, the way a
   participant finds its machine: the name after `@start…`, the title, the
   file name; letters and digits only, case aside. Never to the diagram
   itself. A name two diagrams go by links to neither, and says so. No new
   syntax: the files stay ordinary PlantUML and Mermaid, and a PlantUML
   that knows nothing of Omaspec draws a map as the plain tree it is.
2. **A mind map or a WBS in a project is a map.** Each box that names a
   diagram is drawn as a tile: that diagram's picture, its name below.
   A box that names none stays a box, a heading of its own. The map's own
   lines are the tree the writer drew. A dashed arrow joins two tiles where
   one diagram links to the other (a state of *Payment* named *Checkout*)
   and the tree does not already join them. The main diagram (`main` in
   `omaspec-project.toml`) also shows, below a heading *Not on the map*,
   every diagram of the project no box names.
3. **The tiles are vectors from Omaspec's own engines.** Each picture is
   drawn by the native engines (never Java, so twenty tiles take
   milliseconds, not twenty starts of a JVM) in the colors the map is drawn
   in, and nested into the map's SVG, which passes `sanitize` as a whole: a
   tile stays sharp at any zoom, and an export shows what the pane shows. A
   diagram only Java draws gets a tile that says so. Tiles show the files
   as saved.
4. **A click zooms in.** A plain click on a tile or on a linked state glides
   into it and opens its diagram in its tab (an open one is taken). A trail
   over the diagram names the way there, `shop › Orders › Checkout`; a click
   on one of its names, or Alt+Left, opens that one again and glides out of
   the box it was left by. A drag still pans, Ctrl+click still jumps to the
   source (on a tile, to its box), and a play ignores links. A linked state
   carries a small mark, two squares, so the reader sees there is more
   inside. The preview window follows a click too; Backspace goes back.
5. **A new project starts with its map.** `omaspec new` and *New project*
   write `system.puml` as a WBS whose root names the project (`system.mmd`
   as a mind map in Mermaid, which has no WBS), and the first state
   machine, titled with that name, in a file of the name: the map's root
   shows it at once. A comment at the map's top says how boxes become
   tiles.

## Why

Names keep the project file free of structure ([0017](0017-projects.md)),
and a link that is a name reads the same in the source as in the picture.
Tiles drawn by the map's own engine keep the WBS's and the mind map's own
layouts; nesting the pictures as vectors keeps them sharp, makes one
picture for the screen and the export, and lets the sanitizer see all of
it. The tree and the dashed arrows say two things: what the writer
arranged, and what the diagrams say. When the project grows, the two
part, and seeing both shows where the map is behind; the diagrams on no
box show what it has not taken in yet.

## Consequences

- `omaspec_core::project` holds the names and their key, the map's starter
  and the machine's file name; `omaspec_model` takes its names from there.
- `omaspec_uml::tree::Tiles` makes the mind map and WBS drawers draw tiles,
  the diagrams on no box and the dashed arrows; `Map::tiles` says where.
- `omaspec_render::Atlas` holds a project's diagrams; `render_linked` draws
  a map with its tiles (pictures kept between drawings) and says where a
  map's tiles and a machine's linked states are (`Rendered::links`).
- The editor reads the project's diagrams with its state machines and draws
  its diagrams linked; it draws a project's diagrams again when one of them
  changes on disk. `omaspec render`, the preview window and the MCP
  server's `render` draw a map with its tiles; the MCP server reads no
  further than its folder ([0008](0008-mcp-server.md)).
- Not now: a participant of a scenario, an action of an activity or a
  class do not link yet; the trail is the way taken, not where a diagram
  sits on the map; no lint finding for a name two diagrams share; a tile
  does not play.
