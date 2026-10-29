# 0003: Mermaid through mmdr

Status: accepted, 2026-10-15 (spike `spikes/mermaid`)

## Decision

Mermaid diagrams render with [mmdr](https://github.com/1jehuang/mermaid-rs-renderer)
(`mermaid-rs-renderer` 0.3, pure Rust, no browser). There is no fallback to
mermaid-cli: what mmdr cannot draw is an accepted gap.

## What the spike showed

26 test diagrams covering all 19 Mermaid types Omaspec offers (sequence,
flowchart, class, state, ER, gantt, mindmap, timeline, C4, pie, journey,
quadrant, XY, sankey, block, packet, kanban, radar, requirement): all rendered,
none failed.

- Speed: most types under 5 ms, flowcharts 60 ms, sequence diagrams 3–7 ms.
  One outlier: a flowchart with about 60 nodes in nested subgraphs took 2.1 s.
- Quality: sequence and class diagrams look clean. State diagrams with many
  labeled transitions overlap their labels. The large subgraph flowchart came
  out wide and tangled. mmdr's README says output quality is still catching up
  with mermaid-cli.
- Rasterizing with resvg at 2x: 4–105 ms.

## Consequences

- Rendering runs off the UI thread with a generation counter, so a stale
  render is dropped and a slow one never blocks typing.
- Layout bugs go upstream as minimal reproductions.
- mmdr's own theme presets are the starting point for theme-aware diagrams.

## Phase 2 findings

- mmdr reads `%%{init}%%` only on one line; Omaspec joins a multi-line one
  first and pads with blank lines so error lines stay right.
- mmdr refuses messages to participants that were never declared; Mermaid
  creates them. Lint already warns about such names.
- Flowchart layout costs 40–100 ms even for seven nodes; port assignment
  takes about 70 % of it (`render_with_detailed_timing`). Worth an upstream
  issue with a minimal example.
- mmdr has panicking code paths; Omaspec catches panics so a diagram can
  never crash the editor.

## Phase 3 findings

- mmdr places states well but routes some state transitions badly: through
  other states, back into their own, in a zigzag where two states nearly
  touch, or at a slant. Its repair passes run for flowcharts only, and its
  spacing options do not change state diagrams.
- Omaspec repairs state diagrams between mmdr's layout and its SVG writer
  (`omaspec-render/src/mermaid/repair.rs`), through mmdr's public `Layout`:
  gaps under 32 units are opened, broken transitions are routed again at
  right angles around the states (A* over a grid of channels), and labels
  that cover a state move along their line. Lines mmdr drew well stay.
  Cost: about 1–2 ms on typical diagrams, about 100 ms at 40 states and
  76 transitions (mmdr itself takes 340 ms there).
- Later, state diagrams moved onto the graph engine PlantUML state uses
  (`omaspec-uml::graph`): mmdr still parses, and
  `omaspec-render/src/mermaid/state/` lays out and draws, concurrent
  regions and transitions between nested states included. A transition
  between a state and a state inside it, a shape the drawer does not know
  and a style color it cannot read keep mmdr's layout and the repair above.
  The benchmark diagram takes 0.3 ms instead of 1.7 ms; one with nested
  states, regions and five transitions between them 15 ms instead of 20.
- The routing of the repair moved into `omaspec-uml::graph::route`, which
  also routes those transitions between nested states.
- Flowcharts followed, on the same engine and the same drawer, now
  `omaspec-render/src/mermaid/layered/`. A real diagram with ten nested
  subgraphs took mmdr 3.7 s and came out 18,000 px wide; Omaspec draws it
  in 20 ms, 2,200 px wide. An invisible link is ranked and not drawn.
  `@{ shape: … }` is read for the shapes the drawer knows; an unknown shape,
  and `img:`, still go to mmdr. An empty subgraph is drawn, and a line
  between a subgraph and a node inside it stays inside the frame. An icon
  is the icon's short name, since no icon font is fetched.
- Sequence diagrams followed, on the PlantUML sequence engine. mmdr's
  graph drops the arrow head, so Omaspec reads it from the source:
  `->` has none, `->>` is filled, `-x` is a cross, `-)` is open, and
  `<<->>` points both ways. `+` turns the receiver on and `-` turns the
  sender off. The `alt` tab sits clear of the actors, and a note that
  follows a block stays outside the block. `rect` highlights the rows it
  wraps, `create` puts the head on the creating message and `destroy`
  crosses the lifeline on the destroying one. `@{ "type", "alias" }` on a
  participant is read. An `@{ }` anywhere else, and a color that does not
  parse, still go to mmdr. Links are not drawn.
- mmdr reads `((text))` as a double circle, as it does `(((text)))`;
  Mermaid draws a circle. It drops an invisible link (`A ~~~ B`) and the
  node after it, reads `A@{ shape: rect }` as a diamond, and ignores
  `classDef default`. Omaspec reads those from the source: the double
  circle, the invisible link, the `@{ }` shape and label, and `default`.
- mmdr's labels are text with `<br>`; Mermaid's take a little HTML, entity
  codes and markdown strings, and wrap at 200 px. The drawer reads those
  itself and no other markup.
- mmdr gives a nested state one `[*]` start and one end, shared by all its
  concurrent regions, and draws a shared start with several transitions as
  a fork bar. Mermaid gives each region its own; Omaspec splits them
  before drawing.
- State diagrams then got a reader of Omaspec's own
  (`omaspec-uml::state::mermaid`), which keeps the line of every state
  and transition. The drawer draws from it, in the form of mmdr's graph,
  and the specification checks go by it, so the machine checked is the one
  drawn. It follows Mermaid where mmdr did not: mmdr drops a state written
  on its own line and a long name in quotes that holds a `{`, reads
  `A --> B:::hot` as a transition labelled `::hot`, and takes a note whose
  text holds an arrow for a transition. A diagram the reader does not know
  is still parsed and drawn by mmdr, and not checked.

## Phase 5 findings

- mmdr parses `%%{init}%%` but applies it only in its command line tool;
  the library renders every diagram in its default theme. Omaspec applies
  the directive itself (`theme`, the common `themeVariables` for colors and
  font, flowchart spacing) between parsing and layout. Before that a
  diagram's own `%%{init: {'theme': 'dark'}}%%` had no effect.
- Skin packs for Mermaid are such `%%{init}%%` lines. mmdr has no
  hand-drawn look. A drawn pack that only picked a handwriting font was
  removed: Omaspec's own PlantUML engine declines `handwritten`.

## Phase 6 findings

- In sequence diagrams mmdr draws an `alt` block's title partly behind the
  second actor box, and a note that follows a block inside the block's frame.
- Two edges between the same two flowchart nodes: one is routed in a loop
  around the target.
