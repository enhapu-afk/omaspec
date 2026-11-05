# 0018: Diagrams link by name, and a project has a map

Status: accepted, 2026-11-01. Builds on [0017](0017-projects.md) and
replaces its decision 5: a new project's main diagram is a map. Its
decision 2 is amended on 2026-11-03: a map has a look of its own, and on
2026-11-05: maps within maps (see *Amended* below).

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
  a map with its tiles (pictures kept per file until it changes) and says
  where a map's tiles and a machine's linked states are (`Rendered::links`).
- The editor reads the project's diagrams with its state machines and draws
  its diagrams linked. It keeps an index of each project open
  (`omaspec-app/src/index.rs`, 2026-11-05): it watches the project's
  folders, reads again only the files that changed, and draws again only
  the documents a change touches: a map, a document that names the changed
  file, and every one when a diagram comes, goes or takes another name.
  `omaspec render`, the preview window and the MCP server's `render` draw
  a map with its tiles; the preview window keeps an index for the map it
  shows; the MCP server reads no further than its folder
  ([0008](0008-mcp-server.md)).
- Not now: a participant of a scenario, an action of an activity or a
  class do not link yet; the trail is the way taken, not where a diagram
  sits on the map; no lint finding for a name two diagrams share; a tile
  does not play.

## Amended: a map in a look of its own (2026-11-03)

Checked in a real window, the map read as a PlantUML WBS with pictures in
it: heavy black boxes, square lines, a stem under a root alone that led
nowhere, and diagrams on no box in a list apart, joined to nothing. A map
is the first thing a project shows; it should look like the rest of
Omaspec, not like the tree it is written as.

- **Cards, from the top down.** A WBS and a mind map, of either language,
  are laid out alike as a map: each box that names a diagram is a card
  with rounded corners and a soft shadow, the diagram's picture on its own
  paper and, below it, the box's words, the diagram's kind and language
  (*State machine · PlantUML*) and a few facts (*7 states*). A box that
  names nothing is a small label between the cards. Curves hang each card
  below its box's parent, level by level. The root's card is larger and
  ringed in the accent; on the project's main map it says *MAIN*, and the
  project's name heads the map with what it holds.
- **A card takes its diagram's shape.** A tall diagram gets a tall card, a
  wide one a wide card; a small diagram grows a quarter at most, so its
  words keep their size. Its picture is always the whole diagram, never a
  part; at the map's size a large one is a thumbnail, sharp when zoomed
  into, and its caption still says what it is.
- **Diagrams on no box hang from the root, dashed.** On the main map they
  stand in rows beside the root's children, their cards and curves
  dashed, under a label *not on the map yet*, and the map says which line
  puts the first of them on it.
- **Arrows say how diagrams use each other**, beyond the tree: a scenario
  *plays* against the state machine of a participant (as the checks of
  [0007](0007-behavior-specifications.md) pair them), a state or a box
  *opens* the diagram of its name, a diagram *includes* another's file.
  One arrow for a pair, naming each relation.
- **The colors come from the map's skin**, so in the Omarchy colors the
  page, the cards and the accent are the theme's, as the diagrams on them
  are.

A PlantUML or Mermaid that knows nothing of Omaspec still draws the file
as the plain tree it is, and Omaspec draws a WBS or a mind map outside a
project as before. `omaspec_uml::tree::cards` lays out and paints the
map; `Tiles` carries what each card says, the relations and the heading,
which `omaspec_render::atlas` works out from the project.

## Amended: maps within maps (2026-11-05)

Measured on generated projects (`omaspec-app/src/scale.rs`), the main map
did not scale: it showed every diagram on none of *its* boxes, so a project
whose main map names its areas showed all of its diagrams again, 2,000
pictures as vectors, drawn in 1.1 s and parsed by the pane in 3.9 s.

- **A box may name another map**, so a large project is maps within maps:
  the main map names its areas, each area's map its diagrams. The main map
  shows the diagrams that no box of *any* map names; a diagram written on
  an area's map has its place there.
- **A large map shows its cards first.** Each card names its diagram, its
  kind and a few facts without the picture; the pane shows a map's SVG
  whole when it is small (up to 128 kB, about 20 tiles) and, for a larger
  one, its cards (`Rendered::cards`), parsing the pictures in the
  background once a tile is about 96 pixels wide on screen, where a
  state's name in it can be read. `omaspec render`, exports and the MCP
  server always draw the pictures.
- **Pictures are kept per file until it changes**, not 256 at most, and
  the missing ones are drawn side by side on the cores there are.

The main map of a project of 2,001 diagrams in areas of 50 is drawn in
about 65 ms and shown in about 20 ms more. Half of the drawing is the map
nested with its pictures and sanitized as a whole, which the pane needs
only when zoomed in; drawing the cards first and the pictures after is
the next step should a project need it.
