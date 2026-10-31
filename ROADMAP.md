# Omaspec roadmap

A native PlantUML and Mermaid editor for Omarchy, in Rust. Independent, secure
and fast; friendly to AI agents; for keyboard, mouse and touchpad; in the colors
of the active Omarchy theme. Goals: [0006](docs/adr/0006-goals-and-scope.md). Focus:
behavior specifications, checked and later run, not every diagram type
([0007](docs/adr/0007-behavior-specifications.md)).

Decisions live in [docs/adr](docs/adr). Releases go to the AUR early, starting
with the preview window (v0.1), so the community can try it while the editor
grows.

## Architecture

```
crates/
  omaspec-core/    language detection, lint, outline, formatting, diff, source in images
  omaspec-uml/     native PlantUML engine: preprocessor, parser with spans, layout, SVG
  omaspec-model/   one behavior model for PlantUML and Mermaid, and the specification checks
  omaspec-render/  router (omaspec-uml → Java PlantUML if installed → error), Mermaid (mmdr parses,
                     omaspec-uml reads sequence and state diagrams again; sequence on the
                     sequence engine, state and flowchart on the graph engine, the rest drawn by
                     mmdr), export
  omaspec-theme/   Omarchy palette and font, syntax colors
  omaspec-app/     iced GUI and CLI, binary `omaspec`
spikes/              throwaway experiments behind the ADRs
```

One binary: `omaspec` opens the editor; `preview`, `render`, `lint`, `fmt`,
`extract` and `theme` are subcommands.

## Phase 0: foundation and spikes

- [x] Workspace, MIT license, `AGENTS.md`, CI (fmt, clippy, test, cargo-deny)
- [x] `unsafe_code = "forbid"` for the whole workspace
- [x] `omaspec-theme`: Omarchy palette with the same fallback cascade as `omarchy-theme-color`; `omaspec theme`
- [x] Spike editor widget → [0002](docs/adr/0002-editor-widget-scrive.md): scrive
- [x] Spike Mermaid rendering → [0003](docs/adr/0003-mermaid-mmdr.md): mmdr
- [x] PlantUML grammar for highlighting (`crates/omaspec-app/assets/syntax`)
- [x] Java PlantUML worker (`plantuml -pipe`): built in phase 2, measured there with a real JVM

## Phase 1: `omaspec-core` (done)

- [x] Language detection (PlantUML start markers, Mermaid headers, file name hint), fence stripping, paste detection
- [x] Diagram kind (sequence, activity, state, timing, structure, other) with a readable name; the
      native engine decides a PlantUML type (`omaspec_uml::uml_type`), so lint, outline and the
      state checks never read a diagram as another type than the picture shows
- [x] Lint: missing `@end`, unclosed blocks and notes, unknown Mermaid header,
      names close to a declared participant (swapped letters count as one edit), duplicate or unused participants,
      swimlanes differing only in case or spaces, rare state names close to frequent ones
- [x] Outline for sequence, activity, state, structure, timing, mindmap/WBS and the Mermaid types
- [x] Formatter (indentation only; layout-sensitive types untouched)
- [x] Line diff ignoring blank lines and indentation (Myers, at most 600 edits)
- [x] Diagram source in PNG (`iTXt`, compatible with PlantUML's own `plantuml` chunk) and SVG (`<metadata>`,
      plus PlantUML's `<?plantuml-src?>`), read and write, with size and decompression limits
- [x] CLI: `omaspec lint` (exit codes for CI and pre-commit), `fmt` (`--check`, atomic `--write`),
      `outline`, `inspect` (JSON for editor integrations), `extract`
- [x] Golden tests (`crates/omaspec-core/tests/golden`), CLI tests

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

- [x] `Renderer` routes: Mermaid → mmdr; PlantUML → `omaspec-uml` (sequence diagrams since
      N1, mind maps and WBS since N2, activity diagrams since N3, state diagrams since N4,
      class, component, use case, deployment and object diagrams since N5, timing
      diagrams and Gantt charts since N6, JSON and formulas since; salt, ditaa, YAML,
      chronology, Graphviz and tree diagrams are still not drawn at all; anything else
      declines naming the line) → Java PlantUML if
      installed → a message saying what to install, with the line the native engine stopped at
- [x] `RenderService`: own thread, newest request wins, results carry a generation number
- [x] Mermaid: fence stripped, a multi-line `%%{init}%%` joined for mmdr (line numbers kept),
      mmdr panics caught, parse errors with their line
- [x] SVG sanitizing: every engine's SVG is rebuilt from a whitelist (no scripts, handlers,
      `foreignObject`, animations; links only inside the document, `http(s)`/`mailto` on `<a>`,
      embedded raster images; CSS without `@import` and external `url()`). Pixel-identical
      output on 27 test diagrams.
- [x] Java PlantUML worker: one long-running `plantuml -pipe` process with a delimiter,
      timeout (30 s for the first diagram, 10 s after), restart after a crash,
      `PLANTUML_SECURITY_PROFILE=SANDBOX`. Tried with PlantUML 1.2026.8 on OpenJDK 27 and
      Graphviz 16.1, confined: the first diagram, the JVM's start included, 0.45 s (0.8 s
      when Graphviz lays it out), each one after 5–7 ms (25 ms with Graphviz). The tests'
      stand-in now writes as PlantUML does
- [x] Export SVG, PNG (resvg, white paper, at most 100 megapixels), JPEG (quality 90),
      PDF (krilla, selectable text, with the SVG and the source attached), Markdown (a fenced
      copy of the source) and a ZIP of the SVG, the source and that PDF. SVG and PNG carry the
      source, so dropping one opens it. Exports never read local files an SVG points to
      (usvg's default resolver would).
- [x] `omaspec render` (`-o`, `-f`, `--scale`, `--no-source`, `--copy` via `wl-copy`,
      `--json` for the report, `$OMASPEC_PLANTUML`); criterion benchmarks
      (`cargo bench -p omaspec-render`)

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

- [x] `omaspec preview FILE`: preview-only window that follows the file, for Neovim users.
      Watches the directory, so saves that write a new file and rename it count too;
      reading the file or the theme raises no event (no render loop)
- [x] Zoom, pan, fit, 100 %; re-rasterize on zoom so nothing blurs. Two layers: an overview
      of the whole diagram (at most 16 megapixels) and, when that is not sharp enough, the
      visible part at full resolution with a margin, so long diagrams stay sharp at any zoom
- [x] Errors shown in place, never a crash: status bar and frame color; the last good picture stays
- [x] Live theme switch: watch `~/.local/state/omarchy/current/`
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

- [x] Editor (scrive) with PlantUML and Mermaid grammars and Omarchy syntax colors, live
      theme switch; diagram pane beside it (shared with the preview window), resizable
- [x] Tabs (each with its own text, history, scroll, diagram and zoom); new, close, switch
- [x] Sidebar as the project folder, one directory level at a time (Ctrl+B). Ctrl+P
      still finds every diagram under it. Ctrl+Alt+arrows moves between the project, the
      editor and the diagram. Ctrl+Shift+N detaches the diagram; Super+F fullscreens the
      whole window, Omarchy's own chord. Blocks in the source fold with Ctrl+Shift+[
      and unfold with Ctrl+Shift+]. Ctrl+Shift+P opens the commands as a list. Help
      Omaspec is a folder of diagrams that describe the editor, including that menu.
      Ctrl+O opens a file or folder by path; session restore (tabs, unsaved text, folder,
      caret) in `$XDG_STATE_HOME/omaspec`, held by one window at a time
- [x] Atomic save (write, then rename) keeping the file's line endings; save as with an
      overwrite check; unsaved changes need a second close; clean tabs follow changes on disk
- [x] "Changed since save" gutter markers: a bar in the accent color and a light tint for
      new and changed lines, a red tick where lines were removed (scrive patched in
      `vendor/`, see [0002](docs/adr/0002-editor-widget-scrive.md))
- [x] Lint as diagnostics while typing; language detected again as the text changes
- [x] Jump to a line (from a render error, or Ctrl+click in the diagram): the line is
      centered and lit for a moment. Diagram blocks fold in the editor. `omaspec outline`
      still prints the structure
- [x] Debounced auto-render (0.4 s), Ctrl+Enter to render now

## Phase 5: power features, v0.3 (built; waiting for the check in a real window)

- [x] Ctrl+click in the diagram jumps to the source line: the label there (or of the box
      around the click), the n-th label of a text to the n-th line with it; whole words
      before parts, comments never match. Every written label of the Mermaid fixtures is
      found (`omaspec-render/tests/pick_coverage.rs`); computed text (axis dates,
      percentages) is not in the source. The preview window names the line
- [x] Find and replace: in the tab with Ctrl+F/Ctrl+H (scrive), across all tabs with
      Ctrl+Shift+F (hits with file and line, Enter shows one) and Ctrl+Shift+H (replace all,
      one undo step per tab); case counts only when the query has a capital
- [x] Compare A|B (Ctrl+Shift+D) with another tab (followed live) or the file in a git
      commit (`HEAD` and the last 30 that changed it; `git` runs locally): both diagrams,
      zoom and pan synced (toggle), or B as changed source lines with context, a click
      jumps; the gutter marks the difference to B meanwhile
- [x] The diagram in a window of its own (Ctrl+Shift+N, class `omaspec-preview`): the
      editor takes the room, the window follows the active tab (Compare A|B too), has the
      preview's keys, Ctrl+click there jumps in the editor and brings it to the front
- [x] Skin packs (plain, spec, presentation, omarchy) as visible source with a
      marker, the next pack replacing it; one undo step. PlantUML: `skinparam` lines;
      Mermaid: `%%{init}%%`, which Omaspec applies itself (mmdr does only in its CLI,
      see [0003](docs/adr/0003-mermaid-mmdr.md)). A diagram theme follows the active
      Omarchy colors at render time and leaves the file unchanged. The drawn pack was
      removed: the native engine declines `handwritten`.
- [x] Command palette (Ctrl+Shift+P, also in the status bar) with every command and its
      keys; format, copy and export (the bottom line asks where; Enter confirms an
      existing file), language by hand; images that carry their source open in a new tab
      under a free name (Ctrl+O or dropped on the window)

## Phase 6: Omarchy polish, v0.4 (built; waiting for the check in a real window)

- [x] Font from `omarchy font set` (fontconfig `monospace`), live: the fontconfig folder is
      watched, `fc-match` asked again
- [x] `.desktop` file, icon and MIME types for `.puml`/`.mmd` (installed by the PKGBUILD);
      Omarchy menu entry and Hyprland Lua snippet (SUPER+SHIFT+U, window rules) in
      `packaging/omarchy`; Omarchy notifications for copies and exports
- [x] Optional override template `omaspec.toml.tpl` for `~/.config/omarchy/themed/`:
      `[palette]`, `[syntax]`, `[marks]`, read from `current/theme/omaspec.toml`
- [x] Screenshot test over all bundled Omarchy themes, and a contrast check: every token
      color and the dimmed text reach 3:1 on the background (faint ones are moved
      towards the foreground until they do)
- [x] General examples (`examples/`, linted and rendered in CI), README with screenshots
      (`cargo test --release -p omaspec-app snapshot_` with OMASPEC_SNAPSHOT_DIR)
- [x] Calmer looks: the diagram lies on paper with rounded corners, a soft shadow on a
      light theme and a faint glow on a dark one, where a shadow would not show; paper,
      picture and shadow lie on whole screen pixels, so no edge of one shows past another
      at a fractional scale, and the paper takes the diagram's own background (the
      Omarchy theme's when it follows it); a play's veil keeps to the rounded paper and
      reaches a pixel past its edge, so no light fleck shows in a corner, and a lit state's
      picture keeps to its rounded outline (the veil over its corners lay under it, as
      iced draws pictures over the shapes of their layer). It is framed in red
      only while the text does not draw (the picture is the last good one), over a play's
      veil too; the focused area takes a thin line of the accent instead of a frame in it,
      more of it where the line would stand out less than 3:1. Checked headless at a scale
      of 1.5 (`snapshot_fractional_scale`), with a pan and a diagram dragged over the text.
      Mermaid sequences asked for Inter alone, which Omarchy lacks, so they fell back to a
      serif font; the whole font list goes into the picture now, and Liberation Sans draws
- [x] Pills: tabs, sidebar and status line take the shape of the play bar. A tab shows the
      kind of its diagram as an icon, its findings as a badge and a dot while unsaved; the
      sidebar is headed by the folder's name and shows each diagram's icon and, where every
      name fits, its kind; the status line shows language and kind, cursor, findings with a
      badge, the diagram's state and the command palette as pills. What keeps the text
      from drawing floats at the foot of the diagram, its line as a badge, with a button to
      it. Bars and sidebar write a size under the editor, their quiet text at 4.5:1, and
      the pointer to Ctrl+P goes once the file picker has been used
- [x] The command palette and the pickers float like the play bar: a search pill, the
      typed letters lit in the accent, a command's group (`Skin:`) quiet, keys in outlined
      pills, the commands chosen lately on top while nothing is typed. An empty page offers
      the PlantUML and Mermaid templates and the keys to open or find a file, instead of
      an error; a text that does not draw yet says what brings the picture
- [x] A gallery of examples (Ctrl+Shift+E, in the palette and on an empty page): 24
      diagrams in `examples/gallery`, in both languages, from a coffee machine's states to a
      treehouse's Gantt chart, as tiles in groups (behavior, structure, plans, data); seven
      complex ones (an elevator with regions and a choice, a media player, a checkout with
      `alt`, `loop`/`break`, `par` and `opt`, an incident in three lanes, a delivery
      pipeline in subgraphs, a library in packages, an app launch) carry a badge and a
      filter of their own. Each
      tile is drawn on the spot by Omaspec's engines (never Java), fitted on its own paper
      at the window's scale, in the colors the diagram pane would show; a switch there
      follows the Omarchy theme and draws them again, the new picture fading in over the
      old. The panel settles as a veil comes, the tiles rise in a wave along the diagonals,
      each picture fades in as it comes, and the chosen tile glows in the accent. Typing
      filters by title, kind, language, group or `complex`, with counts per filter; Tab steps through
      the groups, the arrows through the tiles, the grid scrolls to the chosen one. Enter or
      a click opens a copy, unsaved, under a free name in the project folder (an empty tab
      gives way); F5, Shift+Enter or the *plays* badge of a state machine or scenario opens
      it and plays it once it is drawn. `omaspec example` lists them, `example NAME`
      prints one (`--json`). A test draws each in the Omarchy colors and checks that it lies
      on the theme's background (the JSON diagram did not). The 24 take about 30 ms of
      engine time together (release build), the pipeline 8 ms and the library 5 ms of it
- [x] Lines that curve ([0011](docs/adr/0011-curved-lines.md)): in state diagrams (PlantUML
      and Mermaid), structure diagrams and Mermaid flowcharts each corner of a route turns
      in a curve of up to 48 units, no further than half of either run, so a jog becomes
      an S and a detour an arc, inside the corridor the router found; arrowheads stay on
      the line. A play's light runs along the curve. `skinparam linetype ortho` (or
      `polyline`) and Mermaid's `"curve": "linear"` or a step keep right angles; activity,
      sequence, timing and signal-flow diagrams keep them anyway
- [x] Lines laid out by Graphviz's rules ([0012](docs/adr/0012-lines-like-graphviz.md)),
      found by drawing the gallery with Java PlantUML beside Omaspec: a back edge runs
      through the ranks beside the way there instead of around the diagram, lines spread
      along a side in the order of their other ends, a line between two boxes that overlap
      runs straight, labels of a pair go outside, a container's band is open above except
      for its name, and `left to right direction` holds for a whole structure diagram
- [x] Containers laid out with what is around them, as Graphviz lays out clusters
      ([0013](docs/adr/0013-containers-like-graphviz.md)): a package, node, nested state
      or subgraph a line runs into from outside takes the ranks of the diagram, keeping
      its own and moving down as a whole as far as its lines ask, so containers sit side
      by side and overlap along the flow; the line runs through the ranks like any other,
      in through the top, beside the container's name, and out through the bottom; a line
      to the container itself ends on its border. Lines into a container with concurrent
      regions or a direction of its own are still routed afterwards
- [x] A stepped zoom glides: a wheel notch, the zoom keys, fit and 100 % move the picture
      there in 160 ms, the zoom growing by the same factor each moment and the point under
      the pointer staying put; the sharp pixels for the end are asked for at the start. A
      touchpad zoom, a pan or a pinch stays direct and stops a glide where it is

Found on the way, making the gallery: lint reads `Alt.height -> height` in a signal flow
as an unclosed `alt` ([#1](https://github.com/enhapu-afk/omaspec/issues/1)); the native
engine draws the component icon against the end of a name, over it when the name is short
(`[Lamp]`, [#2](https://github.com/enhapu-afk/omaspec/issues/2)); a Mermaid `title:` in
front matter is not drawn for state diagrams and flowcharts
([#3](https://github.com/enhapu-afk/omaspec/issues/3)). The examples avoid all three.
Three more did not show again in 0.4.2 (2026-10-30, several tries each): use case lines
detouring around the frame, a `stop` in one branch of an `if` landing on the bar of a
`fork` in another, nested signal-flow blocks overlapping their port names. Use case lines
from several actors still cross in tight curves; that belongs to the cleaner lines under
[Next](#next).

Found on the way: mmdr draws the title of a sequence `alt` block behind the second actor
box, puts a note that follows a block inside its frame, and loops one of two edges between
the same nodes around the target. The examples avoid all three.

## Phase 7: hardening, v1.0 (hardening built; released on 2026-10-29 as Omagram 0.4.0, then renamed: Omaspec 0.4.1; 0.4.2 to 0.4.4 on 2026-10-30)

- [x] Fuzzing: cargo-fuzz targets in `fuzz/` for the text functions (lint, outline, format,
      skins, diff), image metadata, the SVG sanitizer, Mermaid, the native PlantUML engine
      and the behavior model (checks, plays, conversion, diff). They need nightly to run.
      A first run of 30 minutes each (2026-10-22, seeded from the tests and examples)
      found no crash: the sanitizer took 404,000 inputs and the image metadata 13.5
      million without a finding; the other four stopped on inputs that took over 10 s
      in the instrumented build (see the open point below). Seeded robustness tests in
      `cargo test` run the same inputs on every build: 1000 mutated diagrams and images
      through `omaspec-core`, 400 booby-trapped SVGs through the sanitizer (its output
      must hold no scripts, handlers or outside links), 150 broken diagrams through mmdr
- [x] Java PlantUML confined with Landlock through `omaspec __sandbox`: it reads only the
      system's folders, its own `/proc` entry, its command's folder, its JDK and the fonts,
      writes only in its own scratch folder, no TCP, no abstract sockets or signals outside;
      it starts without Omaspec's environment but for where programs, Java and fonts are.
      A CLI test with a fake PlantUML checks that it can neither write, connect, read
      outside nor see a token
- [x] No network access and no telemetry ([0005](docs/adr/0005-offline-no-telemetry.md)):
      `deny.toml` bans HTTP, TLS and telemetry crates; iced's D-Bus theme detection is off
- [x] A `!while` that never ends, found by the fuzzing, runs the preprocessor to its limit
      of 200,000 steps (0.2 s); lint, outline, the checks and the type each ran it again,
      so `lint` took 0.7 s, `inspect` 1.2 s and the editor stood still for about a second
      after each pause in typing. Each thread now keeps the last run without includes
      and its lines, so one change costs one run: `lint` and `inspect` 0.18 s. With
      `!include` nothing is kept, as a file may change
- [x] A flowchart whose subgraph has a title wider than its content, found by the
      fuzzing: the title closes the way in from above, the free places on the node inside
      lay right under it, and the places beside it were near other lines' ends, which the
      router leaves out while a free one is there. It found no way, the engine declined
      and mmdr drew the diagram in 0.7 s. The router now tries the places near other ends
      too when the free ones lead nowhere: drawn by Omaspec in 10–20 ms
- [x] The fuzzing again, 30 minutes each (2026-10-22): `plantuml` 580,000 inputs,
      `model` 287,000, `sanitize` 254,000, `image_source` 10 million, no finding. `text`
      stopped on one `!while` that never ends: one run to the preprocessor's limit takes
      0.2 s, but 4.6 s in the instrumented build, over 10 s with six fuzzers at once.
      `mermaid` stopped on a broken flowchart whose subgraph holds one of its own name;
      the engine declines that, and mmdr takes 0.45 s for the whole diagram
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

- [x] V1 check state machines, in PlantUML (from the native parse) and Mermaid (from the
      reading the Mermaid drawer draws from). Transition labels read as UML writes
      them: `event [guard] / action`, several events with commas, line breaks as
      spaces. Findings: a state no path from the start reaches (the outermost one
      only), a state nothing leaves, neither it nor a state around it (with the
      `--> [*]` that ends the machine there), the same event leading from one state
      or choice to two others with no guard or the same guard to choose (an error: the
      specification contradicts itself), a choice or a state leading on without an
      event to two others that way (a warning: it may be left open, and a play lets
      the writer pick), a transition written twice, a composite
      state entered while a region of it has no start (reported where the start goes),
      and, without any start, that reachability is not checked. Reachability errs
      towards reaching too much, so an unreachable state surely is. A diagram either
      reader does not fully understand is not checked. In `lint`, `inspect` and the
      editor with the lint's findings, worded like them. A 61-state machine: 0.5 ms;
      2,000 states: 26 ms. 30,000 mutated diagrams in a test run. Mermaid state
      diagrams have a reader of Omaspec's own (`omaspec_uml::state::mermaid`) that
      keeps the line of every state, transition and note; the drawer draws from it and
      the checks go by it, so the machine is the one on the picture, and the map knows
      the lines, as for PlantUML. A line Mermaid may read otherwise (a state named again
      inside another state, `A -> B`, more after `direction`) leaves the diagram to
      mmdr, unchecked. A test draws and reads mutated diagrams in both languages: each
      machine is the one on the picture; another shows that where mmdr read a diagram
      the same way, the picture stayed. A play starts only when every state and
      transition is on it
- [x] V2 sequence diagrams checked against the state machines of their project. A
      participant has a machine when a state diagram is named like it: by the name after
      `@startuml`, its title or its file name; case, spaces, `_` and `-` do not count, and
      two machines of one name leave the participant unchecked. Each message it receives
      is an event (what comes before its parameters): one its machine has no transition
      for is reported with the events it has (a dashed reply may carry a value and is left
      alone), and, played in order, one no state it can be in by then takes. The scenario
      may start anywhere; the first message narrows where the machine can be. `alt`,
      `critical` and `else` join their branches, `opt` and `break` may be skipped, a
      `loop` that returns to where it began keeps what is known, `par` and `ref` leave
      the participant open. A message to oneself moves the machine when it is an event
      and is never reported. A message that creates its receiver starts the machine.
      Concurrent regions move on their own, guards are not evaluated, and an ignored
      event leaves the machine where it was, as in UML. The editor reads the machines of
      each open diagram's project folder in the background (the sidebar's folder, else
      the file's own) when the folder opens, a file opens or changes, and the window gets
      the focus; `lint` and `inspect` take `--project DIR`. Mermaid sequences are read by
      the reader the sequence engine draws them with, now in `omaspec-uml`. 20,000
      mutated scenarios in a test run
- [x] V3 a specification played in the diagram, F5 in the editor and `p` in the preview
      window. A state machine starts (`omaspec_model::Run`): it is in its innermost
      states, one per running region, and waits at a choice or a join; a transition
      leaves every state up to the one holding both ends and enters its target, a
      composite state starts its regions (with no start to begin at, it is entered as a
      whole), and what needs nobody is taken by itself (the way out of a start, a fork,
      a choice with one way, the one transition without event out of a state); where a
      choice or a state has several, the writer picks. A history brings its region back
      to where it was when its state was last left, `[H]` to the state, `[H*]` to
      everything inside. Guards are not evaluated; the writer picks. The bar under the diagram
      offers what can happen next, numbered, with where it leads; pointing at one draws
      its way. The diagram sinks under a veil, the states the machine is in come
      through it with a soft glow that follows their rounding; a light with a fading
      trail runs along the transition's own route, a ripple spreads where it arrives,
      regions and the ways of a fork go at once. A scenario unfolds message by message
      under a soft edge, a light running along each arrow. Each step lights its source
      line; back, again and stop. The engines draw a map beside the SVG
      (`omaspec_uml::map`): states, routes and messages in the picture's units, for
      PlantUML and Mermaid state diagrams and sequences. What is drawn is a function of
      the time, so tests freeze the clock for pictures. 20,000 mutated machines, played
      at random, in a test run
- [x] V4 a recorded trace held against the specification. `omaspec trace` reads JSON
      Lines, one event per line: `object` and `event`, optionally `state` (where the
      object is afterwards), `time`, and `machine` when the object is called otherwise, so
      many objects share one machine; other fields are left alone. An object goes by the
      state machine of its project named like it, as a participant does in V2, and is
      played through it with `Run`, exactly as in V3: it begins at the start, or where a
      first line without an event says it is. An event no state it can be in by then
      takes, an event its machine does not know, and a state it cannot be in are errors,
      with the time and the states it could be in; after a wrong state it goes on from
      the state the trace gives, after a wrong event from the next line that gives one.
      Every region with a way for an event takes it at once; guards are not evaluated,
      and what a writer picks in a play (a choice's way, one of several ways without an
      event) may happen unlogged. A line that is no event is an error; an object without
      a machine is an info. `--project`, `--json`, `--strict` and exit codes as in
      `lint`. 120,000 lines over 20 objects: 0.66 s; 20,000 mutated traces in a test run.
      In the editor, *Play a recorded trace* finds the traces (`.jsonl`) in the project
      folder with an object of the machine on screen, offers them when there are several,
      and plays one in the diagram with the play's lights and travels: Space the next
      line, back and again; the bar shows the line, its time, its event and where the
      object can be, and turns red with the finding where the trace deviates. Where the
      machine could go several ways, every state it may be in is lit.
      `examples/player.jsonl` holds against `examples/player.puml`
- [x] V5 an MCP server, `omaspec mcp` ([0008](docs/adr/0008-mcp-server.md)): JSON-RPC
      over standard input and output, one message per line, written on `serde_json`
      without an MCP library or an async runtime. The tools are the command line's:
      `lint` (a file, a folder checked against its own state machines, or the code),
      `trace`, `inspect`, `outline`, `format` (the text back, or the file rewritten) and
      `render` (a PNG back to look at, the SVG's text, or a file of any export format).
      Findings as in `--json`, as text and as structured content; a bad argument, an
      unknown one included, is a tool error the agent reads, a panic answers the call and
      the server stays up. Protocol versions 2024-11-05 to 2025-11-25. A test runs a
      session against the binary and finds nothing else on standard output. No tool reads
      or writes outside the folder the server runs in, through a hidden folder or a link
      out of it; a PNG to look at has at most 25 megapixels
- [x] V6 a semantic diff ([0009](docs/adr/0009-diff-and-convert.md)): what changed in the
      behavior between two versions, also between PlantUML and Mermaid. States new,
      removed, moved, of another kind; transitions new, removed, with another target,
      other events, guard or action, paired from the closest match to the loosest so a
      changed one is one change; participants, messages and blocks of a scenario, aligned
      as lines are; then what the checks find now or no longer. Layout, colors and notes
      do not count. `omaspec diff OLD NEW`, or one file against its last commit
      (`--rev`), exit 1 on a change; the MCP tool `diff`; in the editor, Compare shows B as
      the changes in behavior, a click going to the line in A
- [x] V7 a state machine or a scenario written in the other language, through the
      model: the result is read back and compared by the diff, and is written only if it
      means the same. Behavior the other language cannot say refuses with its line
      (history in Mermaid; `ref`, `group`, messages from the edge in a Mermaid sequence);
      looks are left out and told. Names Mermaid does not take become plain ids that
      still show the name. `omaspec convert`, the MCP tool `convert`, and the editor's
      *Write it in the other language*. 10,000 mutated diagrams converted there and back
      in a test run, each with the same behavior

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

- [x] N1 sequence: drawn without Java. Participants in all eight shapes with aliases,
      stereotypes (with a spot, `<< (C,#ADD1B2) Testable >>`) and colors, also named at
      a message (`"Long name" as L`); found by
      their code only, as in PlantUML; every arrow (heads, half heads, `x` and `o` at
      either end, `<->`, colors, bold, dashed), edge (`[->`) and short (`?->`) messages,
      self messages; comments by the preprocessor's rules; activations (`++`, `--`,
      `++--` and `--++` in their order, `activate`, nested, colored), `return`, `create` and `**`
      (as in PlantUML, the next message must go to the created participant), `destroy` and `!!`; `note`, `hnote`, `rnote` left, right, over, across and beside a
      message; `alt`/`else`, `opt`, `loop`, `par`, `break`, `critical`, `group`, nested;
      `ref`, dividers, delays, spaces; boxes; `autonumber` with start, step, format, stop
      and resume; title, header, footer, caption; `hide footbox`, `hide unlinked`; Creole
      inline markup and Creole tables (`|= a |= b |` over `| 1 | 2 |`, a header cell
      bold, `\|` a bar in a cell); skinparams for the colors, fonts and sizes of
      every sequence element,
      `monochrome`, `style strictuml`, so every skin pack but the sketched one draws.
      Declines, naming the line: `newpage`, `autoactivate`, teoz and
      parallel messages, `handwritten`, `shadowing true`,
      `maxMessageSize`, gradients, images, links, Creole lists, headings and rules.
      Golden SVGs (`crates/omaspec-uml/tests/golden`), 1000 mutated diagrams per test run
      (none may panic, all must pass the sanitizer unchanged), Ctrl+click finds the line of
      every label. Login example 0.26 ms, a diagram with every construct 0.95 ms.
      Held against the Java PlantUML: see the oracle below
- [x] N2 mind map, WBS: drawn without Java, laid out like PlantUML (a mind map packs each
      idea's children as tightly as their subtrees allow, then centers them; a WBS hangs a
      row below the root and columns below every other box). Org-mode `*` lines with
      PlantUML's indentation rules, `+`/`-` sides, `0` roots, several roots, `left side`,
      `right side`, `top to bottom direction`; `[#color]`, `_` for no box, `:…;` labels
      over several lines, stereotypes taken off; WBS `<` and `>` sides, `(code)`,
      `"label" as code`, empty nodes; title, header, footer, caption, Creole inline
      markup; the skinparams that reach these diagrams in PlantUML (`Node…`, `Arrow…`,
      `default…`, `roundCorner`, `BackgroundColor`; a later one wins). Declines, naming
      the line: arrows between WBS nodes, `<style>`, more than 100 levels.
      Golden SVGs, mutated diagrams, Ctrl+click on every label (labels with markup too).
      Mind map with every construct 0.31 ms, WBS 0.21 ms
- [x] N3 activity: drawn without Java, laid out the way PlantUML does it, as tiles with an
      entry and an exit that a flow stacks and branches set side by side, then moved into
      their swimlanes. `start`, `stop`, `end`, `kill`, `detach`; actions on one or several
      lines with `#color` or `; <<#color>>`, lists of actions (`- one`, `* two`), SDL
      shapes (`; <<input>>`, `<<output>>`, `<<procedure>>`, `<<save>>`, `<<load>>`,
      `<<continuous>>`, `<<task>>`, or the marks `< > | / \ } ]` in place of `;`) and UML's
      object, accept event and send signal; `skinparam conditionStyle` `inside` or
      `InsideDiamond` and `ConditionEndStyle` `diamond` or `hline`; `if`/`elseif`/`else`
      with `then (label)`, `is (label)` and
      `(label) elseif` (a row of diamonds for `elseif`); `while … is (yes)` /
      `endwhile (no)`; `repeat` / `repeat while … is … not …` with `backward` on the way
      back; `fork` / `fork again` / `end fork` or `end merge`; `split`; `partition` and
      `group` as frames with their name in a tab, colored, nested; arrows with labels,
      colors and `dashed`, `dotted`, `bold`; notes left and right of an action; swimlanes
      with colors and titles, a line to another swimlane turning between the two steps,
      branches ordered by their swimlane so lines do not cross. Detected by its lines among
      `@startuml` diagrams. Skinparams for actions, diamonds, arrows, start, end, bars,
      swimlanes, partitions and notes, so every skin pack but the sketched one draws (the
      Omarchy pack now also colors start, end and fork bars). Declines, naming the line:
      the old `(*)` syntax, `package`, `rectangle` and `card` groups, `switch`, `break`,
      `goto`, `repeat :action;`, a `backward` on several lines, a label on `end fork`,
      notes anywhere but after an action, `floating note`, a swimlane change inside a loop
      or a partition, more than 100 nested blocks, SDL shapes and stereotypes on actions,
      `hidden` arrows. Golden SVGs, mutated diagrams, Ctrl+click on every label.
      A 4,400-line diagram with 400 branches in two swimlanes renders in 87 ms (release
      build)
- [x] N4 state: drawn without Java, one container at a time. Ranks come from the
      longest path after back edges are set aside; a barycenter sweep orders each
      rank, and each rank then moves toward the boxes it joins (least squares
      under its order), so lines run straight where they can; routes are
      orthogonal and bend only in the gaps between ranks, so no line cuts a box
      (drawn, their corners curve since 2026-10-27, [0011](docs/adr/0011-curved-lines.md)).
      Lines that join the same two boxes run side by side. Back edges run in a gutter left or right, whichever side
      their states are free on, each in a lane with room for its label, and the
      gap before a rank widens where they turn (since 2026-10-28 they run through the
      ranks instead, [0012](docs/adr/0012-lines-like-graphviz.md)); self-loops sit on the free side. A nested state lays its own children out, in its own
      direction, and the box is the title plus that layout (`left to right
      direction` stays inside the box; a note's "right" stays on the right, in room
      the layout keeps for it). Names belong to the whole diagram, as in PlantUML:
      a transition may join states in different nested states (ranked where both
      meet, routed around everything afterwards, `omaspec-uml::graph::across`;
      since 2026-10-28 laid out with the states around them,
      [0013](docs/adr/0013-containers-like-graphviz.md)),
      and `state X {` opens `X` where it lives. Concurrent regions, `--` one under
      the other and `||` side by side, with a dashed line between. `[*]` as start
      and end, descriptions, aliases on either side of `as` (the quoted name is shown),
      `state A.X` for `X` inside `A`, colors, choice, fork, join, history and
      deep history (`<<history>>`, `[H]`, `[H*]`, `Name[H]`, `Name[H*]`), arrows with a label, a color, `dashed`, `dotted` and `bold`,
      notes on one state. Skinparams the packs already write (`State…`, `Arrow…`,
      `Note…`, `default…`, `roundCorner`, `backgroundColor`, `monochrome`, `style
      strictuml`), so every pack but the sketched one draws. Declines, naming the
      line: a transition between a state and a state inside it, unknown
      stereotypes and arrow heads, gradients,
      `handwritten` and `shadowing true`. Golden SVGs and Ctrl+click on every label.
- [x] N5 class, component, use case, deployment, object: drawn without Java, on the
      same graph as state diagrams. Each package, node or frame is its own layout,
      nested, and a line between two elements in different boxes is routed around
      what they hold (`omaspec-uml::graph::across`). Names belong to the whole
      diagram. `left to right direction` stays inside the box. Drawn: classes,
      interfaces, enums, abstract classes, annotations, protocols, structs,
      exceptions, records, dataclasses and metaclasses, with members, visibility,
      `{static}` and `{abstract}`, `{field}` and `{method}`, separators and
      headings; stereotypes as `« »`; `#` and `##` colors; relations with heads
      at either end, labels and quoted cardinalities; `extends` and `implements`;
      aliases; packages and namespaces; components (`[Name]` and `component`);
      use cases and actors; deployment nodes, artifacts, clouds, databases,
      folders, frames, rectangles, files, cards, stacks, hexagons, agents and
      storage; objects with fields; notes on one element and a floating note;
      title, header, footer and caption. Lines are routed orthogonally and drawn
      with curved corners ([0011](docs/adr/0011-curved-lines.md)). Skinparams the
      packs already write for these boxes (`Class…`, `Interface…`, `Object…`,
      `Component…`, `Usecase…`, `Node…`, `Package…`, `Rectangle…`, `Artifact…`,
      `Cloud…`, `Folder…`, `Frame…`, `Actor…`, `Database…`, `Arrow…`, `Note…`, `default…`,
      `roundCorner`, `backgroundColor`, `monochrome`, `style strictuml`,
      `minClassWidth`, `shadowing false`), so the four skin packs draw.
      `allowmixing` is ignored; `linetype`,
      `dpi`, `nodesep` and `ranksep` have no effect. Declines, naming the line:
      `together`, `hide`/`show`/`remove`, sprites, urls, `<style>`, ports, `map` and `json`, `hidden` arrows, a direction
      hint on an arrow, a line between a box and something inside it, unknown
      lines, skinparams that would change the picture (including stereotype
      skinparams, gradients, `handwritten` and `shadowing true`), and an unknown
      `{classifier}` on a member. A member such as `FILE` or `node : Node` stays a
      member; `class B {` inside a body is declined. Golden SVGs and Ctrl+click on
      every label.
- [x] N6 timing, gantt: drawn without Java, each on its own layout. Timing: robust,
      concise, binary, clock and analog rows, and a `rectangle` row once the diagram
      is already a timing diagram. States at `@time` and `@+N`, an initial state
      before the first time, anchors, messages (`->` only), constraints, highlights,
      `scale N as M pixels`, notes on a concise, binary or rectangle row, and title,
      header, footer and caption. A robust row's levels follow `has`, then the names
      that appear. A clock needs `with period`. Binary draws the step. Analog draws
      the number it was given, between two bounds when they are written. Time is
      proportional. Gantt: tasks with `requires` or `lasts` in days or weeks,
      `starts` and `ends` on a date or `D+N`, `starts at [Task]'s start` or `end`,
      `then`, `[A] -> [B]`, milestones, one color, a completion share, sections, a
      project start (including `the 20th of september 2017`), and `zoom`. Closed
      days are declined, so a week is seven days. The axis is days, or the day of
      the month when the chart has a date. Skinparams the packs already write
      (`Arrow…`, `Note…`, `default…`, `roundCorner`, `backgroundColor`,
      `monochrome`, `style strictuml`, `shadowing false`), so the four skin packs
      draw. Neither has skinparams for its boxes and lines in PlantUML (only
      `<style>`), so on a dark background they follow the background and the text
      color; timing `rectangle` rows take `Rectangle…`. `printscale daily` is
      ignored. Declines, naming the line: dates and
      clock times on a timing axis, `{…}` states, `use date format`, a manual axis,
      compact mode, `<style>`, stereotypes, `has … as`, analog ticks and a pixel
      height, `@clk`, a note on a robust, analog or clock row, gradients, closed
      days, resources, links, week numbering, a scale other than daily, months, a
      chart wider than 3660 days (a line per day; `D+20200701` would take minutes),
      and an unknown line. Golden SVGs and Ctrl+click on every label.
- [x] P1 preprocessor, for every type the engine draws: variables (`!$x = …`, `?=`,
      `!global`, `!local`), expressions with numbers, strings, `+ - * / %`,
      comparisons, `&&`, `||`, `!`; `!if`, `!elseif`, `!else`, `!ifdef`, `!ifndef`;
      `!define` with and without parameters, defaults and `##`, `!definelong`,
      `!undef`; `!function` (also on one line) and `!procedure` with defaults and
      named arguments, `!unquoted`, `!return`; `!while`; `!assert`; the built-ins
      that need nothing from outside (`%strlen`, `%substr`, `%strpos`, `%upper`,
      `%lower`, `%intval`, `%string`, `%boolval`, `%not`, `%true`, `%false`, `%chr`,
      `%ord`, `%dec2hex`, `%hex2dec`, `%newline`, `%size`, `%variable_exists`,
      `%function_exists`, `%get_variable_value`, `%set_variable_value`,
      `%call_user_func`). `!log`, `!dump_memory`, `!startsub` and `!endsub` change
      nothing. A line a procedure writes carries the line of its call. Lint holds
      its name rules back once a `!` line is there (a name can come from a
      procedure or a branch), the outline skips procedure bodies, and formatting
      indents them. Limits keep an endless `!while`, deep recursion, deeply nested
      expressions and growing strings from hanging or ending the editor. `!theme plain` is taken: it sets the colors,
      Verdana and a title size of 22, and keeps `$BGCOLOR`, `$FGCOLOR` and
      `$FONT_NAME` when they are set before it. Declines, naming the line: `!include` and
      its kin, `!import`, `!theme` other than `plain`, `!pragma`, `!foreach`, JSON
      values, `!exit`, a
      definition inside a definition, unknown variables, and the built-ins that read
      the clock, the environment, files or chance (`%date`, `%getenv`,
      `%load_json`, `%random`, …). A diagram without a `!` line or a `%` built-in
      does not go through it.
- [x] P2 `!include`, `!include_many` and `!include_once` of local files, as in
      PlantUML: a file comes in once unless `!include_many` says otherwise, and its
      first `@start…` block (or all of it) runs in place, its variables and
      procedures with it. Paths start from the including file. The project folder
      is the editor's folder when the diagram is inside it, else the diagram's
      own folder (also for `render` and `preview`). An included file has to stay
      inside it once links are followed, outside hidden files and folders (`.ssh`),
      be a regular file of at most 1 MB and text; 256 files and 8 MB per diagram.
      Its lines carry the `!include` line, and what declines in them names the
      file. An unsaved diagram has no folder, so its `!include` declines. Declines:
      URLs, the standard library (`<C4/…>`), `file!2` and `!includesub`. A change
      in an included file shows once the diagram renders again.
- [x] F1 `legend`, `mainframe` and `scale`, for every type the engine draws, placed
      the way PlantUML adds them, inside out: the main frame around the diagram
      with its label in a tab, the legend above or below it (`legend top left`,
      `legend right`; below and centered unless told otherwise, Creole inside),
      then title, caption, header and footer. `scale 1.5`, `2/3`, `800 width`,
      `600 height`, `800*600` and the `max` forms size the picture, at most ten
      times and 20,000 pixels a side. The `Legend…` skinparams for background,
      border, thickness and font size, color and style. On a dark background the
      legend and the frame take the text's color unless the legend has its own,
      and the Omarchy pack colors the legend like its boxes. `legend -> Bob` and
      `scale -> Bob` stay messages, and `scale 100 as 50 pixels` stays a timing
      diagram's time scale. Declines: a legend's place in another order
      (`legend right top`), an empty `mainframe`, `LegendFontName`.
- [x] F2 `<style>` in sequence, activity and state diagrams: PlantUML's style
      sheets, as a block or on one line, with nested selectors, selector lists,
      `:` and `;`, quoted values, `--variables` and `var()`, `//` and `/* */`
      comments; lenient about a missing or an extra `}` as PlantUML is. A `{` or a
      `*` of its own in a value, and a property with nothing before `}` or `;`,
      decline as PlantUML's parser does; `red*`, `Back++GroundColor` and a `{`
      inside quotes stay. PlantUML
      turns skinparams into styles and keeps the later of two settings, whatever
      their selectors, so each declaration becomes the skinparams of the
      elements it reaches, in its place among the `skinparam` lines; a block's
      own properties come before those of the blocks inside it, as PlantUML
      ranks them. `root`, `element`, `document`, the diagram's name and the
      elements' own names (`participant`, `arrow`, `lifeLine`, `activationBox`,
      `group`, `groupHeader`, `separator`, `reference`, `box`, `activity`,
      `diamond`, `circle`, `start`, `stop`, `activityBar`, `swimlane`,
      `composite`, `state`, `note`, `title`, `legend`, `mainframe`, …) reach
      what they reach in PlantUML: what is set for `activity` reaches the
      diamonds too. Background, line and font colors, line thickness, arrows'
      `LineStyle`, font size and style, `RoundCorner`, the messages'
      `HorizontalAlignment`; `FontName` and `RoundCorner` of `root`; `Shadowing 0`.
      The legend and the main frame take their look from styles as well. A
      declaration that names an element and that nothing draws declines
      (`title { BackGroundColor yellow }`, `Padding`, `Margin`); one that reaches
      an element only from further up leaves what the element does not draw as
      it is. Unknown properties change nothing, as in PlantUML. Declines, naming
      the line: stereotypes (`.name`, and any word PlantUML does not know, which
      it reads as one), `depth()` and `*`, `@media`, `<style file=…>`, a font
      name below `root`, shadows. JSON diagrams take `<style>` too (2026-10-27):
      PlantUML draws them from styles alone, so `document` colors the page and
      the texts around it, `jsonDiagram { node {…} }` the cards' background,
      lines and text, `highlight` the highlighted rows; skinparams change
      nothing there, as in PlantUML (checked against PlantUML 1.2026.8). The
      Omarchy pack writes a `<style>` block into a JSON diagram, so it follows
      the theme like every other type. The other types still decline `<style>`.
      Tests draw each styled diagram the same as its skinparams

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

- [x] The Java PlantUML as the oracle ([0004](docs/adr/0004-native-plantuml.md)),
      `crates/omaspec-model/tests/oracle.rs`: every PlantUML state and sequence
      diagram of the tests and examples Omaspec reads, and 300 of them changed at
      random (`OMASPEC_ORACLE_ROUNDS`), go through one `plantuml -pipe`. PlantUML must
      read each as the same type; a machine is held against its SCXML export (states,
      where they sit, kinds, transitions and labels), a scenario against its XMI
      (participants in order, messages with their ends, labels and group parts,
      groups). An example PlantUML finds an error in fails; a changed one is told.
      Skipped without `plantuml`; the CI job `oracle` installs it on Arch and requires
      it. 10 machines and 9 scenarios agree; 4.5 s with the changed ones
- [x] State diagrams read as PlantUML 1.2026.8 reads them, found by the oracle in 4,000
      changed diagrams: a colon starts a label only with a space beside it (`A --> B:x`
      goes to the state `B:x`), and only after the arrow (`A : text --> B` describes
      `A`); a stereotype after the second state changes nothing (`A --> B<<fork>>` goes
      to `B`). Names hold letters, digits, `_`, `.` and `:` and are never quoted in a
      transition (`state "B x" as B`); arrows are `->`, `-->`, `<--` and their longer
      kin, arrow colors start with `#`; a second or later concurrent region keeps its
      states to itself; inside `state On {`, `On[H]` is another `On` in PlantUML, so it
      declines with the hint to write `[H]`; a stereotype comes after the alias. Each
      of these declines naming the line, and arrows with a direction (`-left->`) still
      decline. Mermaid→PlantUML conversion gives a Mermaid id PlantUML does not take a
      plain name and shows the id (`state "long-wait" as long_wait`); it quoted it in
      transitions before, which PlantUML refuses. In 4,000 changed diagrams no machine
      is read otherwise any more
- [x] Sequence diagrams read as PlantUML 1.2026.8 reads them, found by the oracle:
      declaring a participant again, or after a message named it, moves it to the end and
      out of its box, into the one open then, and changes nothing else (its name, shape,
      stereotype and color stay as it first came); one named first inside a box stands
      in it. A declared name holds letters, digits, `_`, `.` and `@` unless quoted, a
      stereotype has a text (`<<>>` is an error), `#` alone is no color, and a
      preprocessor parameter is `$x` or `x` of letters, digits and `_`; each declines
      naming the line. A box can no longer end up apart. In 4,000 changed diagrams no
      scenario is read otherwise
- [x] What the oracle still found, of the looks. A skinparam name outside letters,
      digits, `_`, `.` and one `<<stereotype>>` declines, naming the line, so
      `Legend[H*]FontStyle` and `Backg"roundColor<<phase>>` do, and `NotAKey` and
      `BackgroundColor<<p]hase>>` do not. A `<style>` value with `{`, a `*` of its
      own, or a property with nothing before `}` or `;`, declines the same way.
      A color PlantUML does not know, `red*`, `Back++GroundColor` and a `{` inside
      quotes are still read, because PlantUML draws them. In 400 changed diagrams
      PlantUML finds no error in a diagram Omaspec still reads
- [x] Stereotypes of the writer's own on states (`state A <<sub>>`), as PlantUML draws
      them: not shown, and colored by `skinparam state { BackgroundColor<<sub>> … }` or
      `stateBackgroundColor<<sub>>` (background, border and its thickness, font color and
      size), in any case; the states inside a composite keep their own look. Stereotypes
      PlantUML draws as shapes of their own (`<<start>>`, `<<entryPoint>>` …) still
      decline, and so do skinparams for stereotypes of anything but states. A state only a
      transition made moves to the state where it is declared or described next, as
      PlantUML 1.2026.8 does: its own composite example now draws `Configuring` beside
      `NotShooting`, where Omaspec drew it inside; across a concurrent region this
      declines. `note over` in a state diagram declines saying PlantUML refuses it too
- [x] `!theme plain`, and Creole tables (`|= a |= b |`) in legends and notes. The
      theme is the part the behavior diagrams draw: background, font and line colors,
      Verdana, a title at 22, shadowing off, and the state, activity and note colors.
      `$BGCOLOR`, `$FGCOLOR` and `$FONT_NAME` set before it stay. Other themes still
      decline, and so do margins, padding and the class and icon settings of that
      theme, because naming them would decline diagrams the engine already draws.
      A table is a grid in the text color, a header cell bold, `\|` a bar inside a
      cell; lists, headings and rules still decline. Cells have a padding of 4 on
      each side and no outer margin
- [x] Three edges the oracle found once the stereotypes example joined the corpus.
      A `[*]` only in a one-line title, header, footer, caption or main frame is that
      line's text, so it does not make a state diagram (PlantUML takes a class diagram;
      the one-line title stays in the outline). A transition label drops one leading `-`
      that is not the start of `--` (`-go` and `- left->done` are `go` and `left->done`;
      `--go` and a lone `-` stay). `return o<-k` is a message: PlantUML's arrow matches
      before the `return` command, so `return` and `k` are participants; `return ok`
      stays the reply. In 4,000 changed diagrams no machine or scenario is read otherwise

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

- [x] S1 blocks, named `in` and `out` ports, boundary signals, and wires
      (`a -> Block.port : name`, `->` or `-->`). An input takes one wire, an
      output may fan out. A wire back to an earlier block, or between two
      ports of one block, runs in a gutter above. Unconnected ports are
      still drawn. The outline lists the blocks. The look follows the
      skinparams the theme packs already write (`backgroundColor`,
      `defaultFontColor`, `ArrowColor`, `RectangleBackgroundColor`,
      `RectangleBorderColor`); other diagram types' skinparams are ignored.
      Not in this slice: subsystems, buses, a Simulink import, checks,
      running the diagram, and any direction but left to right. The metrics
      are not those of another tool.
- [x] S2 a block may contain blocks and wires, and is drawn as a frame
      around that flow. Its `in` and `out` ports are the signals that cross
      the frame; a bare name inside names one of them, and an inner block's
      name stays inside. A wire back, or between two ports of one inner
      block, stays inside the frame and below the block's name. Not in this
      slice: buses, a Simulink import, checks, and running the diagram.

## Track M: Mermaid on Omaspec's engines

mmdr still parses every Mermaid diagram; for these types `omaspec-render`
lays out and draws with the engines of track N (`src/mermaid/layered/`), and
mmdr only draws what they decline. Sequence and state diagrams are read again
by readers of Omaspec's own in `omaspec-uml`, which keep every line. As in
track N, new work serves the behavior types only.

- [x] M1 state, on the N4 graph engine: nested states, concurrent regions (each
      with its own start and end, which mmdr shares), transitions between
      containers, notes, choice, fork, join, `classDef` and `style`. Declines: a
      transition between a state and a state inside it, a color it cannot read
- [x] M2 flowchart, on the same engine: every shape (`[ ]`, `( )`, `([ ])`,
      `[[ ]]`, `[( )]`, `(( ))`, `((( )))`, `{ }`, `{{ }}`, the slanted four,
      `> ]`), lines into a shape end on its outline; subgraphs nested to any
      depth, each in its own `direction`, lines to a subgraph and between
      subgraphs (they enter past the title); arrows, `--o`, `--x`, `<-->`,
      `---`, dotted and thick lines with labels; `classDef` (`default` too, as in
      Mermaid), `class`, `:::`, `style`, `linkStyle`. Labels wrap at 200 px as in
      Mermaid; `<br>`, `<b>`, `<i>`, `<u>`, `<s>`, `<code>`, entity codes
      (`#quot;`, `#9829;`) and markdown strings (`` "`**bold**`" ``) are read,
      and nothing else is markup (the state labels used PlantUML's, which struck
      out `a--b`). Edge labels mask the lines under them in the color they sit
      on. An invisible link (`~~~`) is ranked and not drawn. `@{ shape: … }`
      uses the shapes above, and an unknown name still falls back. An empty
      subgraph is a titled frame, and a line between a subgraph and a node
      inside it stays inside the frame. `@{ icon: … }` is the icon's short
      name, with `form` as the box and the label above or below; nothing is
      fetched, and `img:` falls back. `click` links are not kept (mmdr does
      not keep them either). A diagram with ten nested subgraphs and 25 lines,
      most between them: 20 ms instead of 3.7 s, 2,200 px wide instead of 18,000
- [x] M3 sequence, on the N1 engine: participants, actors and the other
      shapes, aliases, boxes, every Mermaid arrow (`->`, `->>`, dotted,
      `-x`, `-)`, both directions), `+`/`-` and `activate`, autonumber,
      notes, `alt`/`else`, `opt`, `loop`, `par`/`and`, `critical`/`option`,
      `break`. The `alt` tab sits clear of the actors, and a note after a
      block stays outside it. Colors come from the Mermaid theme. `rect`
      highlights the band behind its section, `create` draws the head on
      the creating message, `destroy` ends the lifeline on that message,
      and `@{ "type", "alias" }` on a participant sets its shape and label.
      `rgb()`, `rgba()`, `hsl()` and `hsla()` are read; a translucent
      highlight is mixed onto the background. What still goes to mmdr: an
      `@{ }` that is not that participant annotation, a color that still
      does not parse, and links, which are not drawn

Target for 1.0 ([0006](docs/adr/0006-goals-and-scope.md)): sequence, activity, state and
flowcharts drawn by Omaspec in both languages. A touchpad pinch zooms at the
pointer (two fingers still pan) and a touchscreen pinch zooms at the fingers.
Lint and outline move onto the syntax tree as each type lands.

## Next

What comes after 0.4, announced in the README.

- [ ] Lines by hand: drag a line in the diagram, or a bend in it, to route it
      yourself; the diagram keeps the route and draws it the same way next time
      ([0016](docs/adr/0016-lines-by-hand.md)). Built: the block of comments
      that holds the bends (`omaspec_uml::hand`), its edits and lint
      (`omaspec_core::hand`); every gallery example draws the same with one.
      Next: the engines lay lines through the bends, the map names every line,
      Alt and a drag in the diagram pane.
- [ ] Diagrams that link: a state links to the diagram that describes it. A
      click zooms into the state, and inside is its activity diagram; zooming
      out goes back. A specification reads from the whole system down to a
      single step.
- [ ] Even cleaner lines: fewer crossings and detours, lines that keep clear of
      frames and labels, closer to what Graphviz draws; the open points of
      [0012](docs/adr/0012-lines-like-graphviz.md) and
      [0013](docs/adr/0013-containers-like-graphviz.md) among them.
