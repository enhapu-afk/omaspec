# Omaspec roadmap

A native PlantUML and Mermaid editor for Omarchy, in Rust. Independent, secure
and fast; at ease with projects of hundreds of diagrams; friendly to AI agents;
for keyboard, mouse and touchpad; in the colors of the active Omarchy theme.
Goals: [0006](docs/adr/0006-goals-and-scope.md). Focus: behavior specifications,
checked and later run, not every diagram type
([0007](docs/adr/0007-behavior-specifications.md)).

This file holds what is open. What is built, item by item with how it was
built, is in [docs/DONE.md](docs/DONE.md) under the same headings.

Decisions live in [docs/adr](docs/adr). Releases go to the AUR early, starting
with the preview window (v0.1), so the community can try it while the editor
grows.

## Architecture

```
crates/
  omaspec-core/    language detection, lint, outline, formatting, diff, source in images
  omaspec-uml/     native PlantUML engine: preprocessor, parser with spans, layout, SVG
  omaspec-model/   one behavior model for PlantUML and Mermaid, and the specification checks
  omaspec-render/  router (omaspec-uml → Java PlantUML if installed → error), Mermaid (Omaspec
                     reads sequence, class, state and other diagrams itself and draws them on
                     omaspec-uml's engines, flowcharts from mmdr's parse on the graph engine;
                     mmdr draws what they decline), export
  omaspec-theme/   Omarchy palette and font, syntax colors
  omaspec-app/     iced GUI and CLI, binary `omaspec`
spikes/              throwaway experiments behind the ADRs
```

One binary: `omaspec` opens the editor; `preview`, `render`, `lint`, `fmt`,
`extract` and `theme` are subcommands.

## Phase 0: foundation and spikes

## Phase 1: `omaspec-core` (done)

Checked on 53 real diagrams: language, kind, lint, outline and formatting. Choices made on
the way:

- No "blank line before `@startuml`" error: PlantUML ignores text before `@startuml`.
- A multi-line `%%{init}%%` is skipped as a whole (no false "unknown head", the kind is detected).
- `note {` inside `<style>` is a selector, not an unclosed note.
- Flowchart outline: an arrow like `-->` is no longer read as a node, so nodes and decisions after arrows appear.
- Mermaid front matter keeps its YAML indentation when formatting.
- A swapped pair of letters (`Auht`) counts as one edit when looking for typos.

Speed (release build, 20,000 lines, including process start): lint 48 ms, outline 35 ms, fmt 40 ms.

## Phase 2: rendering (done)

Speed (release build, sanitizing included): Mermaid sequence 1.5 ms, class 2.1 ms, state 0.2 ms
(8 ms with nested states, regions and five transitions between containers; mmdr took 20),
flowchart with a subgraph 0.6 ms (mmdr took 40–100 ms, 70 % of it in its port assignment),
three levels of subgraphs with most lines between them 14 ms (mmdr 400 ms).
PNG export at 2x 68 ms, PDF 10 ms.

Found on the way, with the real PlantUML: it ends an SVG without a line break, so the
delimiter followed `</svg>` on its line and every diagram it drew ran into the timeout; it
counts an error's line from 0 at `@start…`, so every error pointed one line too high. The
stand-in had written neither way. Confined, each Graphviz Java started cost 160 ms: it
could not list its descriptors and closed all 524,288 one by one, so the sandbox now
lowers the limit to 4,096.

Found on the way: mmdr refuses messages to undeclared participants, which Mermaid itself
creates on the fly. Omaspec's lint warns about those names anyway.

## Phase 3: preview window, v0.1 (built; waiting for the side-by-side check and the AUR)

- [ ] AUR package: `packaging/aur/omaspec-git` builds the newest commit of
      github.com/enhapu-afk/omaspec; not submitted yet (releases: Phase 7)

Done when Neovim and the preview sit side by side and the preview follows every save.

Found on the way: iced uploads images over 2 MB to the GPU in the background and draws
nothing until then. New rasters are shown only after `image::allocate`, or the paper would
flash empty after every save and zoom step.

Found on the way: mmdr routes state transitions badly (through other states,
in zigzags). Reported upstream as
[mmdr#162](https://github.com/1jehuang/mermaid-rs-renderer/issues/162); state
diagrams and flowcharts are now laid out by Omaspec itself (track M).

## Phase 4: editor, v0.2 (built; waiting for the check in a real window)

## Phase 5: power features, v0.3 (built; waiting for the check in a real window)

## Phase 6: Omarchy polish, v0.4 (built; waiting for the check in a real window)

Found on the way, making the gallery: lint read `Alt.height -> height` in a signal flow
as an unclosed `alt` ([#1](https://github.com/enhapu-afk/omaspec/issues/1)), right since
0.4.4; the native engine draws the component icon against the end of a name, over it when
the name is short (`[Lamp]`, [#2](https://github.com/enhapu-afk/omaspec/issues/2)), which
the examples avoid; a Mermaid `title:` in front matter was not drawn for state diagrams
and flowcharts ([#3](https://github.com/enhapu-afk/omaspec/issues/3)), drawn for state
diagrams since 0.5.0 and for flowcharts after it (the gallery's pipeline had one); a
sequence diagram still drops it.
Three more did not show again in 0.4.2 (2026-10-30, several tries each): use case lines
detouring around the frame, a `stop` in one branch of an `if` landing on the bar of a
`fork` in another, nested signal-flow blocks overlapping their port names. Use case lines
from several actors still cross in tight curves; that belongs to the cleaner lines under
[Next](#next).

Found on the way: mmdr draws the title of a sequence `alt` block behind the second actor
box, puts a note that follows a block inside its frame, and loops one of two edges between
the same nodes around the target. The examples avoid all three.

## Phase 7: hardening, v1.0 (hardening built; released on 2026-10-29 as Omagram 0.4.0, then renamed: Omaspec 0.4.1; 0.4.2 to 0.4.4 on 2026-10-30)

- [ ] AUR `omaspec` and `omaspec-bin`, signed tags, announcement: PKGBUILDs in
      `packaging/aur`, a release workflow that builds on Arch, the steps in
      [packaging/RELEASING.md](packaging/RELEASING.md) and a draft in
      `docs/announcement.md` are ready, and so is the SSH key that signs the tags

Found on the way: the robustness tests found three bugs. Lint counted a lone CR as a line
break where the editor does not (now only CRLF is turned into LF); a self-closing `<svg/>`
got no embedded source; formatting changed a line twice when a BOM followed the indent.

## Track V: behavior specifications, understood (`omaspec-model`)

What PlantUML and Mermaid cannot do: check a specification, run it, hold it against
what a system did. See [0007](docs/adr/0007-behavior-specifications.md).

Found on the way: mmdr read Mermaid state diagrams otherwise than Mermaid in places, and
the drawing went by mmdr. It dropped a state written on its own line and a long name in
quotes that holds a `{`, read `A --> B:::hot` as a transition labelled `::hot`, and a note
whose text holds an arrow as a transition. Omaspec's reading takes them as Mermaid does.

Found on the way: the lint's hint for a state name seen once next to a frequent one
compared every name with every other, and the state parser walked the whole diagram to
find each name. A 3,700-line state diagram took 450 ms on every change, lint and check
together; now 68 ms. The hint compares only names of a near length whose letters are
near enough, with the same result; the parser keeps an index of names.

## Track N: native PlantUML (`omaspec-uml`)

Runs alongside the phases from v0.1 on. See [0004](docs/adr/0004-native-plantuml.md).
Since [0007](docs/adr/0007-behavior-specifications.md), new work here serves the behavior
types only; the others stay as they are.

Found on the way: resvg looked up `sans-serif` as Arial, which Linux rarely has, and fell
back to whichever font came first; text came out too narrow or too wide. The generic
families now point at installed fonts, metric twins of Arial, Times and Courier first
(Liberation on Omarchy). The Omarchy skin pack now also colors the participant icons,
group tabs, activation bars, dividers, references and boxes, and (`Node…`) the boxes of
mind maps, WBS and deployment diagrams. Blank lines inside a note, title or other block
of text stay as empty lines, as in PlantUML.

Found in review: the type of an `@startuml` diagram was picked from every line, so
members (`state : State`, `#id : int`, enum values), note text (`binary data`) and
messages (`clock -> server`) made class and sequence diagrams timing, activity or
state diagrams. The text of notes, legends, titles, styles and class bodies no
longer decides, and a timing row needs a name. `A --> B : at 10:30` keeps its time:
`:Name:` is an actor only where a name starts.

Found on the way: a class, component or other structure diagram with a `<style>` block
was taken for a sequence diagram, because the check for its type waited for an
`endstyle` line after `<style>`, which never comes; it now ends at `</style>`.

Found in PlantUML's own examples (386 of them, by type): a state named `X` or `O` was
taken for an arrow head (`X --> Z`), `state A.X` became a state called "A.X", and
`state alias4 as "long name"` a second, loose box. Lists of actions (`- one`, `* two`)
and a long action whose first line looks like an actor (`:Creole:`) made activity
diagrams sequence diagrams. The lint saw a new note in a note's text that starts with
`note`, an unclosed group in `alt#Gold` and in `group Name { … }`. A footer's
`%page%` and `%lastpage%` stayed as written; they are 1, as a diagram is one page here.
Lines that crossed one gap between ranks shared one run across it, and the labels of
lines into one state landed on each other and were pushed off to the right, away from
their lines (`06-long-name`). Each line now takes a track of its own across a gap, in
an order that keeps them from crossing, and a label on another label, on a box or across
another line takes the best place along its own line, in state, structure and Mermaid
diagrams alike.

Found on the way, with the oracle: after a delay (`...`) PlantUML takes no `activate`,
`deactivate`, `create` or `destroy` until the next message, whatever comes between; the
engine drew such a diagram, and the test with every construct had one. It now declines
there. PlantUML 1.2026.8 itself crashes on a sequence participant with a spot without a
color (`<< (S) >>`). Its SCXML export gives a transition written backwards (`B <-- A`)
from `B`, so those machines are not compared.

## Signal flow (`@startflow`)

Omaspec's own diagram, not PlantUML's. See
[0010](docs/adr/0010-signal-flow.md). Software as blocks that receive and
send named signals, placed left to right on the graph engine. The wires are
drawn to the ports; the graph only ranks the blocks. A line it declines is
that line, and is not sent to the Java PlantUML.

## Track M: Mermaid on Omaspec's engines

mmdr still parses every Mermaid diagram; for these types `omaspec-render`
lays out and draws with the engines of track N, and mmdr only draws what they
decline. State diagrams are lowered into PlantUML's state tree and drawn by its
drawer (since the code review of 2026-11-01); flowcharts are laid out on the
same graph in `src/mermaid/layered/`. Sequence and state diagrams are read again
by readers of Omaspec's own in `omaspec-uml`, which keep every line. As in
track N, new work serves the behavior types only.

Target for 1.0 ([0006](docs/adr/0006-goals-and-scope.md)): sequence, activity, state and
flowcharts drawn by Omaspec in both languages. A touchpad pinch zooms at the
pointer (two fingers still pan) and a touchscreen pinch zooms at the fingers.
Lint and outline move onto the syntax tree as each type lands.

## Next

What comes after 0.4, announced in the README. Lines by hand, projects and
diagrams that link were released as 0.5.0 on 2026-11-01. The diagram types
that follow (2026-11-13: Mermaid's charts of numbers next) stand at the end of *An editor that knows the diagram, and more
diagrams* below.

- [ ] Even cleaner lines: fewer crossings and detours, lines that keep clear of
      frames and labels, closer to what Graphviz draws; the open points of
      [0012](docs/adr/0012-lines-like-graphviz.md) and
      [0013](docs/adr/0013-containers-like-graphviz.md) among them.
  - [x] Measured: `omaspec-render/tests/lines` counts, over every state,
        structure and flowchart diagram of the repository, where lines
        cross, run through a box, step aside by a few units (jogs) or come
        so near they look tangled, with their bends and length, and fails
        when crossings, lines through boxes or jogs grow
        (`tests/lines.txt`). At the start: 474 lines, 7 crossings, none
        through a box, 39 jogs, 479 bends.
  - [x] No small steps: where a line left one box and entered the next a
        few units further across, or a run through the ranks sat a unit
        off the one before, it stepped aside by that much. `graph::straight`
        moves the run before or after such a jog (under 8.5 units) onto
        the other's line, an end sliding along a flat side, where that
        keeps off every other box and other line. Jogs 39 → 1, bends
        479 → 403; 2 PlantUML and 8 Mermaid goldens changed, looked at
        side by side.
  - [x] Skins with colors (2026-11-09): the sketch pack drew in PlantUML's
        own pale yellow and black, presentation in the file's colors, and the
        only colored pack took the Omarchy theme, dull on a dark theme. The
        sketch pack now draws in pastel colors on warm paper (light blue boxes,
        orange actors, green stores, yellow notes, dark ink), presentation in
        its one blue in PlantUML as it did in Mermaid, and a new `vivid` pack
        has a palette by kind: blue boxes, violet interfaces, pink choices,
        green stores, amber people, yellow notes, slate lines, on white
        (`skin::Swatch`, one list of skinparams and one of Mermaid's theme
        variables for all three). Every color keeps 3:1 against the paper,
        as lint asks. A project may pin `sketch` or `vivid` too. Then: plain
        fills nothing, black lines and frames on white, where `monochrome`
        alone left PlantUML's gray fills; and a state that holds others is a
        frame around them, its fill seven tenths of the way to the paper, so
        the states inside stand out instead of sitting blue on blue. And a
        Mermaid flowchart with a pack colors its shapes by kind (decisions,
        terminals, cylinders), where Mermaid's theme knows one node color:
        the pack's `secondaryColor`, `actorBkg` and `tertiaryColor` with
        their borders, read by the layered drawer only when a pack marker
        is in the file, so a file's own init keeps Mermaid's look.
  - [x] Words off the curves (2026-11-09): a label sat at the top of its
        rank beside the straight run of its line, where the curve of the
        corner above, up to 48 units long, cut its first letter. `tests/lines`
        now measures words against the lines as drawn (5 cut, 0 before, since
        it measured the straight runs); `label_beside` moves the label along
        the run until the curve stays its gap off, and on a run too short for
        that sets it in the middle, off the line by what the curve reaches.
        5 cut → 0; 5 PlantUML and 1 Mermaid goldens changed, looked at side
        by side.
  - [x] One step where there were several (2026-11-09): a line through
        several ranks stepped aside in every gap, as far as the point it
        passed through there sat off the one before, three small steps
        the same way where one would do. `straight::merge` moves the run
        between two steps onto the far side of the longer one, a step the
        other way too when nothing or a step as large as the layout's is
        left, where the run keeps off every box, frame name and label,
        enters no frame the line kept out of, crosses nothing new and
        keeps 10 units from the lines beside it; its label keeps 4 off
        other words. Bends 530 → 458, crossings, jogs and words
        unchanged, near lines 17 → 15; 4 PlantUML and 7 Mermaid goldens
        changed, looked at side by side.
  - [x] Fewer crossings, as Graphviz orders ranks (`graph::order`): the
        crossings between neighboring ranks are counted after every sweep,
        two neighbors in the same frames swap where that crosses less, and
        an earlier order that crossed less wins over the last; a tie keeps
        the last. On 300 random graphs 3479 crossings became 3058; the
        repository's diagrams were ordered as well as their frames allow
        already and did not change. Their 7 crossings left come from lines
        into containers laid out on their own (routed after placement) and
        from the order of frames.
  - [x] Words beside their lines: each line carries where its label and,
        in a class diagram, its multiplicities are (`Wire::labels`), and
        `tests/lines` fails as well when more of them are cut by a line,
        their own included, or lie on other words or on a box a line ends
        at (a frame around one of their line's ends may hold them). At the
        start 22. A label no longer stays on its own line (`place_labels`,
        `label_spot`), where it was put beside its line another line
        through it counts as beside as well, and one that has to move
        stays near where it was, on its side of the line; a multiplicity
        beside an upright line keeps its width off it, and labels keep
        off multiplicities. 22 → 10, the rest cut by other lines or
        crowded (`history.puml`: lines 12 units apart leave no room); 2
        PlantUML and 4 Mermaid goldens changed, looked at side by side.
        Open:
        labels of lines into containers laid out on their own, which do not
        know the multiplicities yet, and notes and boxes no line ends at,
        which the count does not see.
  - [x] Labels inside a container or outside it, not across its border:
        `place_labels` knew the name bands of the containers a layout
        opens but not their borders, so a label of a line leaving one
        could sit across its edge. It knows them now, and in `label_spot`
        a border through a label costs what a line through it costs
        instead of growing with the area across it, which had sent a
        label 3 units over a border onto another line. 10 → 8; 1 PlantUML
        and 2 Mermaid goldens changed, looked at side by side (in
        `state-open.mmd` a label off a border the count does not see).
        Left: `state-deep.mmd`, whose `Shop` is 3 units too narrow for
        `continue` beside its line, and the lines too close for their
        labels; both need the layout to leave room for labels.
  - [x] Multiplicities off a container's border: `many` in `class.puml`
        and `1..*` in the library sat on a package's border where their
        line leaves it. One across a border takes the other side of its
        line now, or moves on along it up to where it stops anyway, and
        labels keep off where it goes. 1 PlantUML golden changed, and the
        library in the gallery.
  - [x] Room for labels, as Graphviz makes it (2026-11-06, under 0.9 below,
        [0020](docs/adr/0020-room-for-labels.md)): the 8 words left in the way
        (`tests/lines.txt`) all lie where lines run closer than their labels
        are wide. Five belong to lines side by side: the pair keeps its labels
        outside (0012, rule 4), but outside lies a third line 43 to 73 units
        off for a label 67 to 81 wide (`wind >= 12 m/s`, which then takes the
        place of `hold over`, and `no signal 3 s` in the drone), or three and
        four lines leave a narrow box (`pause` in `state-loops.mmd`, `power`
        in `history.puml`). Two lie between a pair and a third line
        (`no clearance`, the second `power` in `state-nested.mmd`), one beside
        a container too narrow (`continue`). Where the flow runs down,
        `graph::ports` makes no room for a label between two lines, though
        0012 (rule 2) says it does. Tried and left (2026-11-02): room for
        labels between the ports. Squeezing every step to fit took the words
        8 → 5 but pressed pairs to 5 units apart (near lines 13 → 18); taking
        only spare room took them 8 → 7 for 6 more bends, lines once straight
        bending to reach boxes that had not moved. The ports are the wrong
        place: the room has to come from where the boxes stand. Next: a label
        as a node of its width in a rank of its own between two ranks, as
        `dot` does, so placement leaves room and a pair bends around its
        labels. A change to ranking, order and placement that moves most
        goldens with labels; its own ADR first.
  - [x] A long line changes sides as a whole (2026-11-10): the order of
        a rank swapped two neighbors where that crossed less, so a line
        through several ranks that came down on the wrong side of a box
        stayed there: each of its points alone crosses as much on the
        other side, only all of them cross less. `order::reroute` lays
        such a line again, one pass down its ranks for the places that
        cross the fewest other lines, each point among the neighbors of
        its own frames; the other slots stay, so only the line's own
        crossings change. On 300 random graphs 3058 crossings became
        2203. In `examples/platform.mmd` the report builder's line to the
        replica passes the queue on its far side (3 crossings → 1), and
        the drone's outer layout changed with it, Emergency and
        Maintenance beside Monitoring instead of under it (a quarter less
        high; 18.3 → 6.8 units moved when a box is added). There a line
        routed afterwards then ran 4 units under `motors armed`, its
        corner's curve through the words: `graph::route` now counts a run
        within 6 units of words as under them (`WORD_AIR`). Crossings
        8 → 4, two of them the writer's own in `held.puml`; no word in
        the way; 1 Mermaid golden changed, looked at side by side.
        `tests/lines` lists the words in the way now (`words`). Left:
        `Auth -> Db` over `Search -> Index` in the platform, where every
        order of the data tier crosses once, and in the drone
        `Maintenance -> Docked` over `SensorFault -> Maintenance`. Looked
        into the same day: only the second is routed after placement
        (its start lies in Preflight, which has a direction of its own
        and stays closed). The first is the layout's, a line back
        through the ranks that runs under Flight and steps up 67 units
        under Emergency; with Flight above, that step closes the way
        from Preflight to Maintenance, so whatever the router does
        crosses it. The layout does not know the second line when it
        lays the first: a line into a closed container only ranks its
        boxes. Tried and left: a line's point following the line's next
        point before the box it ends at, as Graphviz weighs them, which
        takes the step out but cost more elsewhere (crossings 4 → 5,
        jogs 1 → 3, near lines 14 → 21, boxes moved 988 → 1100). What
        would settle it: the layout carries such a line from the closed
        container's border to its other end as it carries any line, and
        only the stretch inside is routed afterwards; a change to
        `graph::open` and `graph::across` with an ADR of its own.
        The README's picture of the drone is taken again; the other
        pictures show nothing that moved.
  - [x] Lines by hand curve like the layout's (2026-11-08): a corner the
        writer placed was rounded by at most 16 units, so a run dragged
        out of the layout turned a wide curve into a near right angle; now
        it curves as any corner does ([0016](docs/adr/0016-lines-by-hand.md)
        point 4), and the drag preview in the pane curves the same way
        instead of drawing the runs straight. Two Mermaid goldens changed.
  - [ ] Two lines PlantUML draws across containers (REQ-055, 2026-11-15),
        which go to the Java PlantUML now, said so by lint and failed by
        `--strict`:
        - a direction across the flow between boxes of different packages
          (`[A] -right-> [B]` with A in P and B in Q, `-up->` under `left
          to right direction`): Graphviz puts A and B in one rank and P
          beside Q. Holds stop at a frame's edge (`hold::Unheld::Apart`);
          what it takes is a rank shared across frames and the frames
          ordered as the word says, in `graph::hold`, `graph::nest` and
          the order of the frames.
        - a line between a box and something inside it (`Server -->
          [App]` with App in the node Server): the line runs from the
          container's own border in to its member. `graph::across` routes
          between boxes, not from a frame to what it holds.

        Tried and left: the across line taking no part in the ranks (the
        packages then stood one under the other, the line in a bow), and
        a crossing from the container to its member (a stub at the
        member's side). Both want an ADR of their own, on the shared layout
        of states, structures and flowcharts.

### After the code review (2026-11-01)

An outside review found the drawing doubled in one place and the Mermaid
dispatch spread over two lists. The rules it set for the next change are in
`AGENTS.md`.

- [ ] The keys beside their modes: the gallery's, the play's and the
      prompt's keys in their modules, `shortcut` only puts them together;
      then `App` handed out along the same lines.
  - [x] The keys (2026-11-10): `gallery::key`, `play::key`,
        `files::prompt_key`, `picker::key` and `menu::key` hold what each
        mode takes; `shortcut` asks the mode first, then the window's own
        (`editing_key` at rest, `window_key` in every mode). The keys are
        the same: before the old function went, a test held it against
        the new one for 102 keys with every set of modifiers in all six
        modes.
  - [x] What the window's main module held for others (2026-11-10),
        moved as it was: the prompt is `editor/prompt.rs` (what it asks,
        its key, what Enter does with the answer, the paths typed into
        it), out of `mod.rs` and `files.rs`, which keeps opening, saving
        and closing; lines and boxes placed by hand are `editor/hand.rs`;
        the scroll that shows a tab or a tile whole is `editor/reveal.rs`;
        the help's state is in `help.rs`. `mod.rs` 2,806 → 2,418 lines,
        `files.rs` 744 → 376.
  - [x] The stories out of `run` and `update` (2026-11-13, after a review
        found `mod.rs` grown back to 2,622 lines): saving under a name taken
        since is `files::save_tab`; the comparison's diff, behavior, glide
        and sync are `compare.rs`; the views (motion, JSON nested, outline,
        state tables, colors that follow the theme) and `redraw_views` are
        `editor/views.rs`; formatting, writing evenly, portable and the
        fixes are `editor/tidy.rs`; what the diagram's pane hands back is
        `hand::paper_event`; a new scale is `panes::scale_factor`. Each arm
        is one call. `mod.rs` 2,622 → 2,373 lines. From here a new command
        goes into the module its story belongs to, and `run` calls it.

## Plan for 0.6 to 0.9 (2026-11-03)

What people find hardest with PlantUML and Mermaid, gathered after 0.5: a layout
they cannot steer, diagrams that turn unreadable on a dark page, links and colors
lost on export, and projects of hundreds of diagrams nobody keeps track of, with no
history of what changed and no easy way to hand them on. And what still jars in
Omaspec itself: a diagram opens smaller than its pane, and the zoom into a linked
diagram ends close up and then jumps out. So the goals gain one, **Scales**
([0006](docs/adr/0006-goals-and-scope.md)). Versions and exchange go through git and
plain files that carry their source: no server, no account, no format of Omaspec's
own ([0005](docs/adr/0005-offline-no-telemetry.md)). Each step says how it is
checked; numbers are measured, not assumed. 0.6 to 0.8 were released together as
0.8.0 on 2026-11-05, 0.9 as 0.9.0 on 2026-11-09 with everything under *Since
0.8.0*, the menus, the colored skins and the cleaner lines of that week. 0.9.1
followed the same day with what is under *Since 0.9.0*: the map's arrows around
its cards, and the zoom into a tile as one picture. 0.9.2 followed on 2026-11-10
with what is under *Since 0.9.1* and what was built of *Ideas after 0.9*: the map
made from the project's diagrams, a color for one box, a label's side, the skins
shown before one is chosen, a play, a fold and a dive as a GIF, and long lines
that change sides as a whole. Its tag was set before the version was: the
program in it calls itself 0.9.1. 0.9.3 followed the same day with the version
right, a sequence diagram's front matter title
([#4](https://github.com/enhapu-afk/omaspec/issues/4)) and a component's name
off its icon ([#2](https://github.com/enhapu-afk/omaspec/issues/2)). 0.9.4
followed on 2026-11-12 with what was built since: the menus, find in a file, the
colored source and what the editor offers, `<style>` and shadows in more PlantUML
diagrams, branches of a tree that fold, Mermaid's git graph and kanban board, and
a release binary for aarch64 and for glibc 2.35 and newer, where 0.9.3's started
only on Arch. 0.9.5 followed on 2026-11-13: views derived from the behavior (a
machine as a table, its changes and the coverage of its traces drawn over it, a
trace as a timing diagram), architectures drawn by hand (C4-PlantUML, ArchiMate,
icons and sprites, a line within a rank, a box placed by a word), PlantUML's
JSON, YAML, network diagrams, Salt, EBNF, regular expressions and Ditaa without
Java, Mermaid's charts, requirements and ZenUML on drawers of their own, the
outline beside the source, and a dense flowchart laid out in under half a
second. 0.9.6 followed the same day: an export through the desktop's own save
dialog from the diagram's menu, Gantt charts by week, month, quarter or year
over up to 20 years with durations in months, a play whose bar keeps one line
and calls states and options as the diagram does, a timeline whose long words
break inside their cards, and every reader and drawer fuzzed each night, with
properties checked on generated machines and traces. 0.9.7 took in what a stress
test of some forty diagrams found: a play without a GPU drawn as on one and
recorded on a clock of its own, Mermaid flowcharts that lose no node and draw
Mermaid 11's thirty named shapes, generics in both languages, Salt trees with
their columns, clouds, Sankey columns, timing names and state notes that keep
clear of what is around them, a hyphen in Ditaa kept, lint naming what only Java
draws, `render --fit` and the characters no installed font draws, and a play
that checks its events first and keeps to its width. 0.9.8 made the glide of a
fold or an edit know its parts: the engine marks each box, mark and line's
words, and each glides as a picture of its own (states, structures, flowcharts,
trees, JSON cards), the lines and their words with them, titles and legends too;
how a new picture comes on screen is a state machine Omaspec checks; a recording
and the tests take their pictures the window's way; and `--version` tells the
commit a build comes from. 0.9.9 took in a tester's round of 0.9.8: errors that
name their cause and their line, an emoji of several characters drawn whole, an
Open Iconic name the set lacks drawn as nothing, a play's options in a second row
rather than their numbers alone, a fold's inner end kept apart from the
diagram's, interfaces as lollipops beside components, `componentStyle`,
`packageStyle` and `+{static}` without Java, a Mermaid Gantt chart's axis and
`vert` lines drawn natively, a GIF made smaller on request, `--strict` failing
on a diagram only Java draws, and lines whose runs lie apart sharing a track.

### Step 0: the yardstick

### 0.6: it fits, it glides, it follows the theme

### 0.7: handed on without Omaspec

### 0.8: hundreds of diagrams

### 0.9: a layout you can steer

### Since 0.8.0

### Since 0.9.0

### Since 0.9.1

### Ideas after 0.9 (2026-11-09)

Named when 0.9 was done, and weighed the same evening ([0006](docs/adr/0006-goals-and-scope.md)):
two are built, two are declined, four are steps still to take, each with
what it costs.

### An editor that knows the diagram, and more diagrams (2026-11-11)

Asked for on 2026-11-11: a list of what could be written, highlighting
that knows a state from an event, a formatter that does more than
indent, a window to choose an emoji from, motion in the editor, and the
diagrams Omaspec does not draw itself yet. In the order they are meant
to be built; each stage stands on its own.

Stage 1, the editor understands the diagram:

Stage 2, a formatter and fixes at the finding:

Stage 3, emoji and symbols:

Stage 4, motion in the editor:

- [ ] Scrolling that eases: left out. The wheel's steps go through the
      widget's reckoning of rows, of what is in sight and of where a
      click lands, so a place between two steps touches all of them; a
      touchpad already scrolls by pixels.

Seen in use, on a tree of 2,790 boxes (2026-11-11):

Stage 5, more diagrams drawn by Omaspec (each a step of its own). Set
right on 2026-11-11: the list first written here named the ER diagram,
journey, timeline, C4, architecture and block as mmdr's, which Omaspec
draws itself already (`NATIVE` in `omaspec-render/src/mermaid.rs` has
twelve kinds, and flowcharts beside them). What mmdr still draws,
without folding, lines by hand and plays, and the Java PlantUML:

### Architecture diagrams, elegant and playful (2026-11-12)

The picture to match is a reference architecture drawn by hand: cards in
pastel colors by layer, each with an icon, a title and a short list,
arrows with a word on them, dashed ones for what runs beside. No new
kind of diagram is needed for it: PlantUML's boxes (`rectangle`,
`component`, `node`, `database`, an `actor`) are that diagram, drawn by
the structure engine, and `skinparam handwritten true` or the sketch
skin draws them by hand.

Beside the stages:

### Views derived from the behavior (2026-11-12)

What PlantUML and Mermaid cannot draw, since they do not read a state
machine as one: views of a machine made from the files that are there,
with no syntax of their own (REQ-032 to REQ-035 of the requirements
gathered for Omaspec). Three of them mark a drawn machine, so the marks
are built once, with coverage, and the others take them.

### Dense diagrams as you type (2026-11-13)

A flowchart of 150 boxes and 476 lines between random pairs (Mermaid's
issue 8777, where Mermaid takes 36 seconds) took Omaspec 53 seconds:
the ordering of the ranks grew far faster than the lines.

### A glide that knows its parts (2026-11-14)

The glide of a fold or an edit took its boxes by cutting them out of the
picture and guessed what belongs to which box; two reviews found fifteen
wrong guesses each. Before the presenting below builds on it, in this
order ([0029](docs/adr/0029-a-glide-by-parts.md)):

### Presenting, animating, and diagrams only Omaspec draws (2026-11-13)

Wished for after 0.9.6: a diagram that is shown to others, not only read,
and a few kinds of diagram no other tool has, made to be a highlight.
Each new kind gets a gallery example and golden SVGs; one of Omaspec's own
syntax is an `@start…` block, as signal flow is, and the portability lint
says that PlantUML does not know it. In this order, as each one builds on
the one before:

- [ ] A fold and an unfold as a moving picture, from the diagram's menu
      (*Export as › Animation…*, the container to fold chosen there),
      not only from `omaspec play --fold`: animated WebP or APNG in full
      color at 60 frames a second instead of GIF's 256 colors and
      banded gradients, the glide eased in and out; and in the HTML
      page as vectors, sharp at any size.
- [ ] Orbit: a state machine's life cycle on a circle, the way through
      it round the ring, a way back as an arc inside it, an end where
      the ring opens, from the machines that are there with no syntax
      of their own (a view, as the table is: View menu, `omaspec orbit
      FILE -o`). For orders, tickets, sprints, anything that comes
      round again.
- [ ] Trace river: many recorded traces flowing through a machine, each
      way as wide as it was taken, the ways never taken thin and
      dashed, a deviation branching off in red where the trace left the
      machine; from coverage, which counts it all already.
- [ ] Story map: activities across, the steps of each under it, release
      slices across the whole as bands (`@startstorymap`): the backbone
      of a product and what goes into which release, which neither
      PlantUML nor Mermaid draws.
- [ ] Capability map: what a business can do as nested tiles, each
      colored by a value given to it (maturity, cost, risk) on a scale
      with its legend (`@startcapabilities`), for planning an
      architecture.
- [ ] Time travel: two commits of a diagram, one gliding into the other
      as the pane glides after an edit, what changed glowing a moment
      (the diff's colors); in the editor beside the compare, and as an
      animation or a page to export, for a review.
- [ ] A presenter mode in the editor: the diagram full screen, the keys
      of the presentation page, and the comments next to the lines
      played as speaker notes on a second screen.
- [ ] The project as an interactive page: its map, a click zooming into
      a tile as the editor dives into it, and back out, every diagram
      inside; `omaspec export --html`.

### Long and large diagrams, readable (2026-11-16)

From a tester's round of 0.9.9: a diagram far longer or larger than the
screen, or than one picture file, stays readable.

- [ ] A dead end placed beside what leads to it, not far off in empty
      room (REQ-069).

### A window that looks of one piece (2026-11-16)

From a look at twenty snapshots of the window, in dark and light themes:
it reads as a good developer's tool, not yet as an elegant one. What was
set right the next day (a nested state's title in its band, a label
between two lines, the comparison's switches, menus that name what
applies, the field that replaces, round areas, a play's next steps, the
sidebar's heading, a block left open, a menu's marks, the editor's scroll
bar) is in docs/DONE.md.

- [ ] An SVG carries only the letters of Inter it uses: a subset made
      for each picture is a few KB where the fixed one is 21 KB a cut
      ([0032](docs/adr/0032-a-face-of-its-own.md)). Needs a subsetter
      that writes a face a browser accepts.
- [ ] In a real window, which the snapshots do not show: whether the
      comparison's pictures take the pane's width and B the colors of A;
      and the focus on a box, a frame and a glow in the snapshots, where
      one view showed the box filled in a dull tan.
- [ ] What is not there: a settings dialog, scroll bars or an overview
      in the diagram, a grip between the panes, a hint at each tool's
      icon, sizes of type in an order.

## Other desktops, and a face for the mouse (2026-11-08)

Omaspec's engine is plain Rust; what ties the program to Linux is at the
edges of the window. A Windows build is the next release after 0.9, and the
window grows the menus a desktop user looks for.

- [ ] Omaspec on Windows: the clipboard without `wl-copy` (iced's clipboard or
      `arboard`), the UI font without `fc-match` (Cascadia Mono, then Consolas),
      no notifications, and a palette without Omarchy: light or dark from the
      system, and the bundled themes to choose from (`omaspec-theme` already
      has `Palette::fallback`). Paths that are joined with `/`, if any. A
      `windows-latest` job in the release workflow that builds and zips the
      binary; a run on a real Windows, since nothing here has been seen there.
