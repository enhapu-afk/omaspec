# Omaspec roadmap

A native PlantUML and Mermaid editor for Omarchy, in Rust. Independent, secure
and fast; at ease with projects of hundreds of diagrams; friendly to AI agents;
for keyboard, mouse and touchpad; in the colors of the active Omarchy theme.
Goals: [0006](docs/adr/0006-goals-and-scope.md). Focus: behavior specifications,
checked and later run, not every diagram type
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
      states to itself; outside any state, `--` and `||` split the diagram into regions
      that PlantUML draws side by side, each from its own start, without a line between
      them (drawn so since 2026-11-05, in the order written; Mermaid has none); inside `state On {`, `On[H]` is another `On` in PlantUML, so it
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
lays out and draws with the engines of track N, and mmdr only draws what they
decline. State diagrams are lowered into PlantUML's state tree and drawn by its
drawer (since the code review of 2026-11-01); flowcharts are laid out on the
same graph in `src/mermaid/layered/`. Sequence and state diagrams are read again
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

What comes after 0.4, announced in the README. Lines by hand, projects and
diagrams that link were released as 0.5.0 on 2026-11-01.

- [x] Lines by hand: drag a part of a line in the diagram to shape it
      yourself; the diagram keeps the shape and draws it the same way next
      time ([0016](docs/adr/0016-lines-by-hand.md)). Built: the block of
      comments that holds each line's corners (`omaspec_uml::hand`), its edits
      and lint (`omaspec_core::hand`); the drawers of state, structure and
      Mermaid state, class and flowchart diagrams draw a line as shaped, with
      no router, and say when it runs through a box; the map names every line
      with its runs and boxes. Every gallery example draws the same with an
      entry for no line, and draws one of its own lines exactly as the pane
      predicts. In the editor's diagram, Alt lights the line under the
      pointer with its corners as handles; Alt and a drag moves a run sideways
      or a corner, shown as it will be drawn, one undo step each; Alt and a
      double click gives the line back; the palette resets the line at the
      caret or every line; entries that did not hold are among the findings;
      Ctrl+click on a line jumps to it. A first version routed the line
      through bends the writer placed; checked in a real window, the router
      chose too much on its own, so the line now keeps its shape.
- [x] Projects: a folder with `omaspec-project.toml` is a project, found from
      any diagram below it, so the editor, the preview, every command and
      the MCP server judge a diagram against the same state machines and
      `!include`s however it was opened
      ([0017](docs/adr/0017-projects.md)). The file holds the project's
      `name`, which heads the sidebar, and its `main` diagram; anything else
      is an error with its line. *New project* in the command list asks for
      PlantUML or Mermaid and git, then the folder; `omaspec new DIR`
      (`--lang`, `--git`, `--json`) does the same. A folder that is there
      keeps its files; the first diagram is the whole system as a small
      state machine, clean for lint and `fmt`. git starts only on request
      and not inside another repository. The MCP server's project ends at
      the folder it runs in. Found on the way, checked in a real window: a
      question asked from the command list (the new project's folder, but
      also *Open*, *Save as*, an export) let the source editor take what was
      typed as well. The list coming and going rebuilt the widgets under it,
      and a new editor starts with the keys; the window now keeps them.
- [x] Diagrams that link: a state links to the diagram of its name, and a
      mind map or a WBS in a project is its map
      ([0018](docs/adr/0018-diagrams-that-link.md)). Each box of a map that
      names a diagram is a tile with that diagram's picture (vectors from
      Omaspec's engines, nested and sanitized); dashed arrows show where one
      diagram links to another beyond the tree, and the main diagram also
      shows the diagrams on no box. A click on a tile or on a linked state
      (marked with two small squares) glides into it and opens its diagram;
      a trail over the diagram (`shop › Orders › Checkout`) and Alt+Left
      zoom back out. `omaspec render`, the preview window (which follows a
      click as well) and the MCP server draw maps with their tiles. A new
      project starts with its map, a WBS (a mind map in Mermaid) whose root
      shows the first state machine.
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

### After the code review (2026-11-01)

An outside review found the drawing doubled in one place and the Mermaid
dispatch spread over two lists. The rules it set for the next change are in
`AGENTS.md`.

- [x] The map with its tiles is drawn in one place (`specs::Maps`) for
      `omaspec render`, the MCP server and the preview window. The Mermaid
      kinds with a drawer of their own are one table (`NATIVE` in
      `omaspec-render/src/mermaid.rs`), which both draws and says why a
      diagram went to mmdr; the eleven `named_decline` copies are gone, and
      `lang::mermaid_body` returns the fence's lines instead of the callers
      counting them through pointers. Every Mermaid file in the repository
      draws the same bytes as before.
- [x] Golden SVGs for the Mermaid side: `omaspec-render/tests/layered`
      holds state diagrams and flowcharts with every construct the layered
      drawer lays out (nesting, regions, notes, choice, fork and join,
      directions, styles, loops, shapes, edge kinds, subgraphs, lines by
      hand), and with the repository's Mermaid examples Omaspec draws they
      are pinned in `omaspec-render/tests/golden` (32 pictures). A diagram
      of the corpus that falls back to mmdr fails the test.
- [x] One drawer for state diagrams: `omaspec-uml/src/state/draw*` and
      `omaspec-render/src/mermaid/layered/` laid out the same picture twice,
      and measured, only about 100 lines were still the same; regions,
      notes, open clusters and crossings into frames had grown apart. Now
      `state::from_mermaid` lowers Mermaid's reading into PlantUML's state
      tree (containers, regions side by side as Mermaid draws them,
      crossings, notes, `classDef` and `style` as colors and a stereotype
      of the state's own, front-matter titles, words with `<br>`, `<b>`,
      `<i>` and entity codes), and the state drawer draws both languages in
      the Mermaid theme's colors. It mirrors a layout for `BT` and `RL`.
      `mermaid/state.rs` (the chart turned back into mmdr's graph) is a
      small adapter, the layered drawer draws flowcharts only, and its
      regions, notes and state shapes are gone: 1,600 lines out, 700 in.
      The 14 state pictures among the goldens changed and were looked at
      one by one; every flowchart drew the same bytes. A state diagram the
      reader does not read, which the layered drawer used to lay out from
      mmdr's graph, is now mmdr's, as the reader always said; the title of
      a state diagram is drawn now, and a flowchart's since
      [#3](https://github.com/enhapu-afk/omaspec/issues/3).
- [x] Flowcharts on the same scene: state diagrams and flowcharts each laid
      out the containers that open ([0013](docs/adr/0013-containers-like-graphviz.md))
      with a copy of the same driver: gather the boxes into one layout,
      build the nested scenes from it, mirror it, place the labels. That
      driver is `graph::open` now, once, on one scene for every drawer
      (`graph::open::Scene`, with what a box and a line keep for painting
      as the drawer's own types). A drawer says what its containers hold,
      which may open, how big a box, a frame and a label is, and where a
      line ends on a round box (`graph::open::Nested`). A line into a
      container laid out on its own keeps its place among the lines,
      without points, until `graph::across` routes it, for flowcharts too.
      Every diagram of the repository draws as before: 118 the same bytes,
      and 2 flowcharts the same pixels, a line between subgraphs now drawn
      in its order rather than last (1 Mermaid golden). Class diagrams
      run on it too: the multiplicities labels keep off come from them
      (`Nested::marks`), every diagram draws the same bytes.
- [x] mmdr's reading only as the fallback: where Omaspec reads the text
      itself, mmdr's parse no longer decides whether it draws. The header
      and the `%%{init}%%` directive are read as mmdr reads them
      (`mermaid/head.rs`); each kind in `NATIVE` names the words of its
      header, and its drawer gets the theme and the text, not mmdr's graph.
      mmdr parses only when no drawer of Omaspec's own draws the diagram,
      and for flowcharts, which lay out on its graph. A test holds the
      header to mmdr's reading for every kind and every Mermaid diagram of
      the repository. Every diagram draws as before, and one more: a
      sequence diagram with a message to a participant it never declared,
      which mmdr refused while Mermaid creates the participant
      (`fixtures/sequence.mmd`); lint still names a declared one it is
      close to.
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
  - [ ] Still in `mod.rs`: `update` and `run`, about 370 and 280 lines,
        one arm for each message and command, and the window's start
        (`themed`, 150). To cut when one of them is next touched.
- [ ] Large files cut along the phases they already have, when they are
      next touched: `formula.rs` (parse, symbols, layout, paint); `c4.rs`,
      `block.rs`, `er.rs`, `architecture.rs` (read, measure, paint).

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
off its icon ([#2](https://github.com/enhapu-afk/omaspec/issues/2)).

### Step 0: the yardstick

- [x] Scale measured (`OMASPEC_SCALE=1 cargo test --release -p omaspec-app scale
      -- --nocapture`, `omaspec-app/src/scale.rs`): generated projects in areas of
      50 diagrams (40 state machines, every third in Mermaid, 9 scenarios between
      them, the area's map), the main map naming the areas. Release build:

      | diagrams | scan | read | lint all | main map: tiles, drawn, again, SVG, parsed | an area's map, again |
      |---:|---:|---:|---:|---|---|
      | 101 | 0 ms | 5 ms | 14 ms | 100, 60 ms, 23 ms, 0.6 MB, 192 ms | 27 ms, 11 ms |
      | 501 | 0 ms | 16 ms | 45 ms | 500, 288 ms, 288 ms, 3.2 MB, 876 ms | 25 ms, 10 ms |
      | 2001 | 2 ms | 63 ms | 388 ms | 2000, 1132 ms, 1160 ms, 12.8 MB, 3880 ms | 31 ms, 13 ms |

      Scanning and reading hold up. What does not: the main map shows every
      diagram on none of *its* boxes, so with areas it shows all of them, as
      vectors; past 256 tiles the kept pictures are thrown away on every drawing
      (`atlas.rs`, `KEPT`), and the pane takes 4 s to parse what it gets. And
      lint grows with the square: each participant of a scenario compares its
      name with every name of every machine afresh (`across.rs`). Both go first
      in 0.8.

### 0.6: it fits, it glides, it follows the theme

- [x] A diagram fills its pane: `View::fitted` never enlarged past 100 %, so a
      small diagram opened small in a large pane, with paper all around. Fit fills
      the pane but for its margin, in the editor, the preview window, Compare and
      the diagram's own window, growing a diagram up to three times its size
      (`MAX_FIT`; to be judged in a real window). Tested on its own and in the
      preview window; the screenshots of a map and of a diagram followed into
      from it now fill the pane (their test had given the pane a size the window
      did not have, which a fit up to 100 % had hidden).
- [x] A dive without a jump: the zoom into a linked state ended with the state's
      box filling 90 % of the pane, then the linked diagram opened fitted, so the
      view jumped from close to far, and its picture faded in only once it was
      drawn. Now the linked diagram opens in its tab at the click and is drawn in
      the background, in the pane's size; once it is, the zoom ends where the
      box holds it exactly as its tab will show it (`passage::view_holding`), in
      the time the zoom had left. On the last half of the way its picture grows
      out of the box on its own paper while the rest sinks under the pane's
      background, so the switch to the tab does not show. Back out (Alt+Left,
      the trail) starts there and runs the other way. Zoom and pan move together
      on van Wijk and Nuij's way (*Smooth and efficient zooming and panning*,
      2003, as d3's `interpolateZoom` has it), sine in and out, 0.3 to 0.72 s by
      the length of the way. Tested: the linked diagram's place at the end of
      the zoom and in its tab, and at the start of the way back, to a pixel; a
      far dive zooms out on the way; no frame at 60 per second zooms by more
      than 12 %. The screenshots `links-arriving` and `links-orders` show the
      same picture. Found on the way: in the Omarchy colors, Ctrl+click on a
      map's box jumped past the end of the text, as the line of a tile was not
      moved back past the colors laid over the source (`remap_map`).
- [x] The zoom without a hitch: it started at once and turned when the
      linked diagram was drawn a few milliseconds later, starting again from
      a standstill; and the map asked for sharp pixels of the end, under the
      veil, which came in mid-way and on the switch. It now waits until the
      linked diagram is drawn and its picture made (at most 160 ms) and runs
      on one way; nothing under the veil is rasterized. Tested: the way does
      not turn once the zoom runs.
- [x] The project's map in a look of its own
      ([0018](docs/adr/0018-diagrams-that-link.md), amended): checked in a
      real window, a map read as a PlantUML WBS with pictures in it, and a
      lone root had a stem that led nowhere. Now each diagram is a card
      (`omaspec_uml::tree::cards`): its picture on its own paper, shaped
      like the diagram, a small one grown a quarter at most; below it the
      box's words, kind and language and a few facts. The root's card is
      larger, ringed in the accent and, on the main map, marked *MAIN*,
      under the project's name and what the map holds. Curves hang the
      cards from the top down; boxes that name nothing are labels; the
      diagrams on no box hang from the root beside its children, dashed,
      with the line that places the first. Arrows show where a scenario
      plays against a machine, a state opens a diagram and a diagram
      includes another. The colors come from the map's skin, so the map
      takes the Omarchy colors. A WBS or a mind map outside a project draws
      as before.
- [x] The map drawn in a moment again: checked in a real window, a map took
      seconds to show. Its cards' soft shadows were blurs, which resvg
      computes for each card over its whole area: rasterizing the test
      project's map took 0.76 s at a scale of 1 and 2.7 s at 2 (`omaspec
      render`, process start and PNG included; a state machine 0.11 s). The
      shadows are rings of rounded rectangles now, only around each card:
      0.09 s at 1, 0.15 s at 2, 0.18 s at 3, measured the same way. And a map in a project was first drawn without the project's diagrams,
      as a plain tree, then again once they were read; it waits for them
      now, so it is drawn once.
- [x] Smooth frames, measured: with `OMASPEC_FRAMES=1` each dive, glide, play
      and fade ends with a line on standard error: how long it took, how far
      apart its frames came and how long each one's work took, from the start of
      its redraw to the last layer of the diagram drawn (every layer of the pane
      is drawn through `paper::frames::Timed`); a frame that came over 20 ms
      after the one before has a line of its own. `OMASPEC_FRAMES=1 cargo test
      --release -p omaspec-app frames_ -- --nocapture` draws a dive into a map's
      tile, a play, the way back and a stepped zoom headless on the GPU in a
      2560×1440 window, at 60 frames a second: the work of a frame is 0.2 to
      0.4 ms, the view and its layout 0.1 ms. Making the pixels and reading them
      back takes 19 to 26 ms a frame, as much when nothing moves, so a motion
      costs the GPU nothing that shows there. The suspicion was the overview
      stretched on the first half of a dive: that is one picture drawn larger,
      no cost, and asking sharp pixels for the whole way would upload more
      while it runs, so it stays as it is until a real window says otherwise.
      Found on the way: the way back out of a link stopped on its first frame
      when the diagram left had a play's bar under it. The tabs share the
      pane's widgets, so the taller pane reported its size on the next frame,
      and a new size stopped every glide. A glide runs on now, a fitted end
      moving with the pane in the time left (tested).
- [x] A play's bar clear of the picture, and no jump when it grows (found in a real
      window): zoomed in, the picture and a play's veil reached past the pane's foot
      over the bar of steps, which looked covered while the light travelled. The
      diagram's layers lie in a container clipped to their room now
      (`play-machine-zoomed`). And at a state with more ways on, the bar takes a
      second line, the pane grows shorter, and the picture jumped: refitted, or moved
      by half the difference. A pane that only grows or shrinks in height holds the
      picture where it is, from its top, and a fitted one glides to its new fit
      (tested). The headless check at a scale of 1.5 had drawn on white paper, which
      the Omarchy colors now replace; it asks for the file's own colors again.
- [x] Smooth frames in a real window: no frame over 16.7 ms on a 1440p pane,
      read from `OMASPEC_FRAMES=1` while diving into a map's tiles and back,
      zooming and playing; the GPU's part shows only there, in the gaps. Measured
      on a 60 Hz laptop screen (2026-11-04): dives, fades and plays kept every
      refresh (frames 16.7 ms apart, at most 0.2 to 1.1 ms of work each); glides
      missed one now and then, about one frame in 200. A late frame names what
      happened since the frame before it now, which showed each of those came
      right after a new overview was shown, once one of 16 megapixels. Pixels
      that would replace a picture on screen wait until the glide stops now
      (tested), and a zoom into a link no longer runs on into the way back in
      the log. Gaps of 20 to 23 ms on the second or third frame of a glide stay:
      the first frame after a key or a wheel notch comes off the refresh, and
      the next falls back in step. Checked again in the large diagram whose
      overview had 16 megapixels: the glides keep every refresh.
- [x] Diagrams in the theme's colors by default: `follow_theme` started off, so the
      paper stayed white on a dark theme (`docs/images/themes.png`). It starts on now,
      saved as `theme_colors`, so a session that switches it off keeps that; the
      `follow_theme: false` every earlier session saved is not read. A test
      (`editor/legible_tests.rs`) draws the 24 gallery examples in the 22 bundled
      themes as the pane does and holds each text to 4.5:1 against what lies under it
      and each line to 3:1 against the paper, read from the SVG
      (`omaspec_render::contrast`: the last filled shape under a text's middle, the
      paper under a line). It found the gray of a JSON `null`, an index and an empty
      card (3.1:1 on PlantUML's light card, down to 1.9:1 on a theme's), which now moves towards the text
      color until it reads at 4.5:1; and the day grid of a Gantt chart, faint on
      purpose as PlantUML draws it, which lies in a group of its own
      (`omaspec_uml::GRID`) that the check leaves out. Colors the writer set stay
      theirs, as the theme's skin goes before the file's own lines; lint names a pair
      of them under 3:1 (`omaspec-core/src/faint.rs`): an element's `FontColor` on its
      `BackgroundColor`, the default text, an arrow and a border on the page, in
      `skinparam` lines and blocks, with stereotypes, and Mermaid's `color` on `fill`
      in `classDef` and `style`. The headless screenshots draw as the app does now,
      in the theme's colors (the helper had handed the renderer the bare file), so
      the README's pictures show the diagrams in the theme. The preview window still
      draws the file's own colors.
- [x] Gallery tiles that read: a tile's picture was the whole SVG fitted into 272 by
      170 with room left for the badges, so most diagrams came out small in much
      paper. It is cut to what is drawn now (the paper the diagram lies on left out)
      and fills the tile: as wide as fits and up to 1.4 times taller than fits, so a
      tall diagram shows its top and is cut at the bottom; a wide one keeps its sides,
      where a timing diagram or a signal flow names its rows. The badges lie on the
      picture. The tiles fill the panel, 272 to 480 wide at a ratio of 1.6, fewer and
      larger in a narrow window (one in a window up to 700 wide, two up to 1,000), and
      their pictures are drawn again for their size once the window stops changing
      (150 ms after the last change). In a narrow panel the switch of colors goes
      under the filters, which ran past its edge. The colors are the theme's, which
      the contrast test holds. Screenshots: `gallery-omarchy` and `gallery-narrow`
      (`snapshot_gallery`).
- [x] Diamonds 1.6 times as wide as high: `layered/shape.rs` made a decision with a
      short word as wide as high (the ratio follows the text, between 1 and 3); at
      least 1.6 now (`DIAMOND_RATIO`), the text's corners still on the outline.
      PlantUML's activity diamonds looked at the same way: a test is drawn with
      pointed ends, at least twice as wide as high already, and the empty diamond
      where branches meet is a mark of 24 by 24, as PlantUML draws both; a test
      inside its diamond (`conditionStyle InsideDiamond`) took the text's own
      shape, so a short one stood taller than wide, and is at least 1.6 as wide
      now. Six Mermaid goldens looked at side by side, old against new. Lines
      (`tests/lines`): no crossing, jog or word in the way more, 45 units shorter
      in all. A line by hand whose corner lies just below a diamond's row comes
      closer, as its corners are measured in the span of its two boxes
      ([0016](docs/adr/0016-lines-by-hand.md)), which the flatter diamond lowers:
      in `flow-by-hand` the line back to *Edit* now meets its side, not its
      bottom.

### 0.7: handed on without Omaspec

- [x] Export looks: `render --look plain|omarchy|light|dark|auto`, in the export
      prompt and the MCP tool, laid over at render time as the pane lays the theme
      over; the file and the source in the image stay as written, the colors stay in
      the SVG. `auto` is one SVG with a light and a dark palette under
      `prefers-color-scheme` rather than a transparent background, which is how
      Mermaid's lines vanish on a dark page. Checked by the contrast test of 0.6 in
      both palettes. Built (`look.rs`): plain is the plain pack, omarchy the active
      theme, light and dark two palettes of Omaspec's own, the same on every
      machine; a map draws its tiles in the look too, and a line an error names is
      the file's. `auto` draws the light and the dark look and makes one SVG of them
      (`omaspec_render::schemes`): the two may differ in colors only, the light one is
      kept, and a style sheet at its end turns a light color into its dark one by
      its attribute (`[fill="#ffffff"]`), or by a class on each element where one
      light color turns into several; a viewer without the query (resvg, a PDF)
      draws the light one, so auto is an SVG's only. Every gallery example makes one
      (tested), all by attribute, and a browser shows it light or dark by its scheme
      (headless Chromium, `--force-dark-mode`). The export prompt offers as shown,
      the file's colors, plain, light, dark and, for an SVG, auto (Tab, or a click);
      the MCP tool `render` takes `look`. The contrast test holds the gallery in the
      light and the dark palette too.
- [x] Links in exports: PlantUML's `[[url]]` (the native engine declined it),
      Mermaid's `click … href`, and a linked state or a map's tile as a relative link
      to that diagram's export. The sanitizer takes relative links that stay in the
      export's folder; scripts and callbacks never. Built: a map's tile and a state
      that opens a diagram of the project link to that diagram's SVG beside its file
      (`../shop.svg` from `orders/`), laid over the export as invisible boxes
      (`omaspec_render::anchors`) by `render`, the MCP tool and the export prompt in
      every look; a copy to the clipboard gets none, and a PNG or a PDF has no links
      to follow. Creole's `[[url]]`, `[[url label]]` and `[[url{tooltip} label]]` are
      drawn underlined in the text's color, as PlantUML's blue would vanish on a dark
      page. `[[url]]` on a class or another structure element, a participant, a state
      or a gantt task (`links to [[url]]`) makes its shape and words one link; the
      layout stays as it is without it (tested). Mermaid's `click A "url"`,
      `click A href "url" "tooltip"` and a class's `link` do the same for a
      flowchart's node, a class and a gantt task, read by one reader of Omaspec's own
      (`omaspec_uml::click`) rather than mmdr's, which takes a callback's name for an
      address; a callback is read and left out, and a target (`_blank`) is not kept.
      A link the writer wrote may not climb out of the picture's folder (`..`), as
      the engine does not know where the picture goes. Tested end to end, from the
      source to the SVG: a script's address never reaches it.
- [x] A project as files anyone can open: `omaspec export DIR` (and the palette)
      writes every diagram as an SVG with its source, the map as `index.svg`, linked
      to one another, and a ZIP of it. A browser is enough; dropping any of the
      pictures on Omaspec opens its source. Built (`project_export.rs`): the SVGs lie
      beside one another as the diagrams do, in the project's `export` folder unless
      `-o` names another, the main diagram once more as `index.svg` with its links
      from the top, and `<project>.zip` in the folder unpacks into a folder of the
      project's name; nothing is written outside the folder. `--look` as `render`
      takes it, `--json` for a report. A diagram that does not draw is named and left
      out (exit 1); two that would be one SVG (`a.puml` and `a.mmd`), or one that
      would be `index.svg` beside another main diagram, stop the export before
      anything is written. The palette's *Export the project* asks for the folder in
      the bottom line with the looks an SVG takes, asks once more when the folder is
      there, and exports in the background. Tested end to end with a map, a linked
      state in a subfolder and `--look auto`, which a map with cards could not take
      before: the shadows are stronger on the dark page, and opacities now turn like
      colors (`schemes.rs`). An SVG an earlier export left in the folder stays.
- [x] Diagrams in Markdown: a `.md` file is a document of diagrams. Every `plantuml`
      and `mermaid` block is linted, checked and drawn, findings on the note's own
      lines; the pane draws the block at the caret, the preview window all of them
      one under another. `lint`, `inspect` and `render` take it (`render README.md`
      writes one image per block). Built: `omaspec_core::markdown` finds the blocks
      by CommonMark's fences (` ``` ` and `~~~`, `puml` and `mmd` too, a body without
      `@startuml` read inside it) and draws one from the document with every other
      line empty (`masked`), so each engine, check, play and Ctrl+click names the
      document's lines without a mapping of its own. `render README.md` writes
      README-1.svg, … each with its diagram's source (README-1.puml); `inspect` lists
      the diagrams with their fence lines; the MCP tools take a document, `render`
      with `diagram` to pick one. The editor highlights a document with a Markdown
      grammar that embeds PlantUML's and Mermaid's (scrive loads them in one set),
      draws the diagram at the caret, and says that formatting, skin packs,
      converting, lines by hand and the language commands are for diagram files.
      The preview window stacks the diagrams (`omaspec_render::stack`). A document's
      state machines belong to its project, named by `@startuml` and `title` only.
      Found on the way: mmdr names line 1 for a flowchart it cannot read at all; on
      a blank line, the error moves to the header after it.
- [x] The look pinned per project: `[look]` in `omaspec-project.toml` (pack, colors,
      font), used alike by the pane, `render`, the MCP server and CI. An SVG is laid
      out the same on every machine already (text is measured on built-in metrics);
      a PNG depends on the installed font. Not the same pixels across updates, as
      the layout keeps improving: `render --check DIR` compares a project with its
      committed SVGs and names what changed, so an update's change is seen before it
      is committed. Built: `[look]` takes `pack` (plain, spec, presentation, light,
      dark, auto), the six colors with light or dark, and `font` (written as
      `defaultFontName` and Mermaid's `fontFamily`); every line that does not hold
      is named, and `omarchy` is refused, as it differs from machine to machine.
      `render`, the MCP tool and `export` draw in it unless `--look` says otherwise
      (`--look file`: the file's colors; a pinned auto gives a PNG its light one),
      and the pane shows it over the theme's colors, its map's tiles too, drawn
      again when the project file changes; a `--look` and a pinned look are one
      `Skin`. `render` takes folders, and `render --check` writes nothing: it holds
      each SVG drawn now against the one beside its diagram (`check.rs`) and says
      whether the diagram changed since, or how the same one is drawn otherwise:
      its size, words new or gone, parts moved, parts in other colors; exit 1, and
      `--json` with `same` for each. The render command moved out of `commands.rs`
      (`render_command.rs`), which had grown past 1,000 lines.

### 0.8: hundreds of diagrams

- [x] A project index kept up to date: a file is read again only when it changed,
      a watch on the project folder replaces the full read on every focus, and only
      the documents a change touches are drawn again. The index knows each diagram's
      names and machines; a scenario finds its machines by name there instead of
      comparing with all of them. Measured against step 0. Built: `index.rs` keeps
      each project's diagrams (as maps show them) and state machines by path with
      the size and time of change they were read at; a refresh scans the folder
      and reads only what differs, and hands back the same `Arc`s when nothing
      did, so nothing is checked or drawn again. `omaspec_model::Specs` finds a
      machine by the key of any of its names, for scenarios and traces alike. The
      editor keeps an index per project open and watches the folders the scan
      looks into (not the whole tree, at most 512; hidden, build and dependency
      folders aside), a burst of changes reported once; the focus only compares
      sizes and times. After a change it draws again a map, a document that names
      the changed file (an `!include`), a Markdown document, and every document of
      the project only when a diagram came, went or took another name. The preview
      window keeps an index for the map it shows, which it read whole for every
      drawing. `read_project` (the command line, the MCP server) reads through the
      same index. Release build, 2,001 diagrams: read whole 84 ms, brought up to
      date with nothing changed 6 ms, with one file changed 11 ms; lint over every
      file 152 ms, was 424 ms (103 ms without the machines; the rest is each file
      parsed again as a scenario), and now linear: 11, 43 and 152 ms for 101, 501
      and 2,001.
- [x] Where a name is used: `omaspec refs NAME` and an MCP tool `refs` list every
      diagram, scenario, link and trace that uses a machine, state or event
      (`--json`). *Rename everywhere* in the editor and `omaspec rename OLD NEW`
      rename it across the project, one undo step per file, so a rename leaves no
      stale use behind; lint names a link to a diagram that is gone. Built
      ([0019](docs/adr/0019-where-a-name-is-used.md)): a use is compared as links
      compare names, so `DoorControl` counts for *Door control*: where a diagram
      says its name, the maps' boxes, the participants that play it, the states
      that open it, and a machine's states (its lines left of a label) and events
      (its triggers, the messages to its players), and the traces of each, in
      diagram files and Markdown documents alike (`omaspec_model::refs`). A rename
      renames one of them and asks which when the old name is several (*Open* the
      state, *open* the event) or refuses a name that is taken; each use is
      spelled as the old one was (`FrontGate`, `front_gate`, words where it had
      spaces), so PlantUML and Mermaid stay valid. Files keep their names (the
      rename says how to `git mv` one) and recorded traces are listed, not
      rewritten. F2 in the editor renames what the caret names; an open tab takes
      it as one undo step, a file not open is written. A `[[link]]` or a
      `click … href` to a diagram or a picture of the project where no diagram is
      is a lint warning, in the editor too. The project's documents and traces are
      read when asked, not kept by the index, which keeps its machines.
- [x] Search the project: Ctrl+Shift+F over every file of the project, not only the
      open tabs; Ctrl+F in the diagram finds a state or a message and lights it.
      Built: the search goes through the open tabs first, as typed, then through
      every diagram and Markdown document of the project as saved, from the
      project's index (which now keeps the documents' texts too); a hit in a file
      not open opens it with the hit selected. Replacing stays with the open tabs,
      as it writes. Where the diagram has the keyboard (its area, or its own
      window), Ctrl+F lists the labels it shows with the line that writes each; the
      one chosen is lit as the caret's name is, and stays lit until the caret moves
      (each editor event lit the caret's name again, which put it out at once:
      seen on a headless screenshot); the view glides to it when it is out of
      sight.
- [x] Maps that scale, first in 0.8 (step 0): the main map shows only the
      diagrams on no map's box, not every one on none of its own; a tile small on
      screen shows its name and kind, its picture once it is large enough to read;
      pictures kept per file until it changes, not 256 at most; a map's box may
      name another map, so a large project is maps within maps. Goal: the main map
      of 2,000 diagrams drawn and shown in under 100 ms. Built (amending
      [0018](docs/adr/0018-diagrams-that-link.md)): the main map leaves a diagram
      that another map's box names to that map (`atlas::on_other_maps`, the words
      on each map's boxes kept by file); a map comes with its cards, the same SVG
      without the pictures (`Rendered::cards`), which the pane shows for a map
      over 128 kB, parsing the pictures in the background once a tile is 96
      pixels wide on screen and showing the cards again below that; a small map
      is shown whole as before (checked on the screenshot of a map of five
      tiles). Pictures are kept by file and fingerprint, those missing drawn on
      as many threads as there are cores. Release build, the main map naming the
      areas, the pane's first parse and a fitted raster for 1600 × 1000:

      | diagrams | main map: tiles, drawn, again, SVG, parsed, rastered | an area's map: drawn, again, parsed, rastered |
      |---:|---|---|
      | 101 | 2, 4 ms, 2 ms, 44 kB, 22 ms (whole), 9 ms | 26 ms, 21 ms, 26 ms, 12 ms |
      | 501 | 10, 17 ms, 9 ms, 219 kB, 6 ms, 5 ms | 27 ms, 20 ms, 25 ms, 12 ms |
      | 2001 | 40, 63 ms, 39 ms, 877 kB, 15 ms, 6 ms | 27 ms, 19 ms, 24 ms, 12 ms |

      2,000 diagrams: about 84 ms from nothing drawn to pixels, 60 ms again (was
      1.1 s and 3.9 s). Of the 63 ms, 15 go to reading the boxes of the 40 areas'
      maps and 15 to drawing their pictures, and 36 to nesting the pictures into
      the map and sanitizing the whole, which the pane needs only once zoomed in;
      handing it the cards before the whole is the next step should a project
      need it. Reworked after a review the same day: the words on a map's boxes
      are read with the file into its atlas entry (`Entry::new`), so the main map's
      rule is a filter on the atlas, not a side effect of the picture cache; the
      cache only keeps pictures, drawing them side by side is `draw_all`'s; the
      pane alone decides when the pictures show (`paper/cards.rs`), `Drawn` only
      says its tree is the cards. The main map now draws in 56 ms, its boxes read
      once per file change.
- [x] A project checked in CI: `omaspec check DIR` runs lint on every diagram and
      Markdown document in the folder (syntax, the checks against the state machines,
      links to diagrams the project does not have) and holds each SVG against the one
      drawn now, the same drawing as `render --check`. It exits with 1 on an error or
      an SVG that differs or is missing, and with `--strict` on a warning. `--json`
      prints the findings and the pictures; `--github` prints GitHub Actions
      annotations, and in Actions those are printed with the report too. `omaspec new
      --git` writes a pre-commit hook that runs it, and leaves a hook that is there
      already.
- [x] Versions through git, made to read behavior: `omaspec diff --project REV` lists
      what changed in behavior across the project since a commit or a tag (a
      changelog of the specification), `omaspec log FILE` the changes commit by
      commit, and `git diff` shows them once `.gitattributes` names Omaspec as the
      textconv (`omaspec git setup` writes it). Compare offers the project since a
      commit. Built so far ([0009](docs/adr/0009-diff-and-convert.md), amended):
      the project's diff, as `omaspec diff DIR` (`--rev`, or two folders), since
      `--project` already names the folder a file's machines come from. The commit
      is read through one `git ls-tree` and one `git cat-file --batch` under the
      rules the index reads the folder by; diagrams pair by file, a moved one by its
      text, its name or its file's name, a document's by name, then order; each
      version's scenarios are checked against its own machines, so one that did not
      change tells what a changed machine no longer takes. `--json` for scripts,
      exit 1 on a change. The git helpers moved out of the editor (`git.rs`).
      Release build, one machine changed: 9, 46 and 186 ms to compare 101, 501 and
      2,001 diagrams, 8, 21 and 73 ms to read the old version from git (the scale
      test's `diff` and `git` columns). `omaspec log FILE`: each commit that changed
      a diagram (or a Markdown document), newest first, followed across renames,
      with what it changed in the behavior or that it kept it; a sequence diagram
      is checked against its project's machines as that commit had them. `-n`,
      `--json`. `omaspec git setup`: the project's `.gitattributes` gives the
      diagram files `diff=omaspec`, the repository's own configuration names
      `omaspec git textconv` for it, so `git diff`, `git log -p` and `git show`
      compare a version's behavior one fact a line (`omaspec_model::behavior`: a
      machine's sorted states, transitions and findings, a scenario's messages in
      order), the same for PlantUML and Mermaid; any other file as it is. Compare
      (Ctrl+Shift+D) offers *the project since* `HEAD`, the newest tags and the
      folder's commits: the project diff in the background, the open tabs as they
      are, its changes a list filtered by words and files; one picked opens its file
      where it changed, next to that commit's version (found where it lay, if it
      moved). Checked on headless screenshots of the list, the pick and the behavior
      view.

### 0.9: a layout you can steer

- [x] Room for labels, as planned under [Next](#next), with its own ADR and two
      more measures in `tests/lines`: how far the other boxes move when one box is
      added to each diagram of the corpus (after aligning both pictures), and how
      far the crossings change when the lines are written in another order. The
      two measures (2026-11-05): a line from each diagram's first named box to a
      new one, both pictures aligned on their median shift, moves a box 908 units
      summed over the diagrams' averages (`smart-home` 84, `platform` 60); each run
      of lines written the other way round crosses 17 times, not 7 (`platform`
      3 → 9). A box added that does not draw (a line across regions) is not
      measured (`-`). Built (2026-11-06,
      [0020](docs/adr/0020-room-for-labels.md), accepted): ranks doubled, a
      labelled line's point in the rank between its ends a box of its label's
      size, in `graph::doubled`. Words in the way 8 → 0, crossings 7 → 6, written
      the other way round 17 → 15, jogs 1 → 1, lines too near 13 → 13; boxes moved
      908 → 932 taken knowingly (the ADR says where); bends 404 → 478, length
      62,009 → 67,809; the pictures 1.9 % wider and 2.9 % longer in all, nearly
      only those with labels. `flow-subgraphs.mmd`, without labels, had come out
      12 % longer: the one point of a short line no longer holds its place while
      placing, and a point before a pass beside a frame's name heads for it; it
      keeps its length now. Open: `flow-subgraph-direction.mmd`, without labels,
      6 % longer. A line by hand with one corner below two boxes side
      by side lost that corner as a spike (`hand::shape`); it turns there now.
      42 goldens changed, each looked at next to its old picture; render and
      scale timings unchanged.
- [x] Boxes held in place: a box dragged to where it should stay is kept in the
      block of [0016](docs/adr/0016-lines-by-hand.md), relative to its rank and
      neighbors rather than in coordinates, so it holds when a theme or a font
      changes a size; lines by hand on it move with it; an entry that no longer
      holds is a finding. Its own ADR. So far: the ADR
      ([0021](docs/adr/0021-boxes-held-in-place.md), proposed, 2026-11-06): an
      entry `Box after Neighbor` (or `before`) in the block holds a box in its
      neighbor's rank, right beside it; ranking takes the two as one, the order
      puts the box next to its neighbor after the sweeps, placement is as
      before. Alt and a drag on a box shows where it would go and writes the
      entry. `rename` carries the names in the block, of lines too, which it
      did not. Built so far (2026-11-06): the layout (`graph::hold`: the two
      ranked as one, the box moved beside its neighbor after the sweeps; a
      hold that cannot be kept is said and changes nothing), the entries
      (`hand::Hold`, read, written, linted), all five drawers of graph
      diagrams (states and structure in PlantUML, flowcharts, states and
      classes in Mermaid), the findings in the map, and `rename`. Then the
      drag (2026-11-06, ADR accepted): the map's places (every named box, its
      container and which way its ranks run), Alt and a drag on a box where no
      line runs with its outline and the bar (`paper::held`), Alt and a double
      click, the palette's *By hand* commands; headless snapshots
      `held-lit` and `held-drag`. Two fixtures hold boxes; every other
      picture stayed as it was.
- [x] Lines bundled where many end at one box, as Graphviz's `concentrate`
      does (2026-11-06, [0022](docs/adr/0022-lines-bundled.md)): on by
      default. Lines that arrive on one flat side of one box, from different
      boxes, with no words on them, share the last 12 units beside the box.
      Only lines that arrive alike share one: the same head at the box (an
      arrow and a triangle, or a circle and a cross, would cover each
      other), the same color, width and dash.
      Each keeps its run through the ranks, so a label keeps its room, and
      turns onto the trunk only as it arrives. The line in the middle stays
      straight. A trunk that would cross a line of the same layout, or that
      would jog, is left undrawn. `apart` and `apart at Name`, in the block
      of 0016, keep a diagram or one box's lines on their own arrivals; lint
      warns on a repeat, a missing name is a finding (in `lint` and `check`
      too, as a hold's, which only the editor said), and `rename` carries
      the name. The points in the rank stay put, so no box moves and the
      signal flow is left alone. `tests/lines`: crossings 8, lines through
      boxes 0, jogs 1, words in the way 0 and the crossings written the other
      way round 21, as before; near lines 15 → 17; bends 504 → 530 and length
      69,788 → 70,715, the short turn onto a trunk in six diagrams plus the
      fixture; boxes moved 966.0 → 986.4, and the 20.4 is
      `tests/state/bundled.puml`, where three lines arrive at Done as one.
      Every diagram that was already there moves as before. Six goldens
      changed, and `bundled.puml` is a new one.
- [x] Focus and collapse (2026-11-06, [0024](docs/adr/0024-focus-and-collapse.md)):
      a container folded to its box in the pane, drawn as a linked diagram is
      ([0018](docs/adr/0018-diagrams-that-link.md)); a focus that dims all but a box
      and its neighbors. Built: a composite state (PlantUML and Mermaid), a package
      and a flowchart subgraph fold to a box of their name, what they held gone,
      their lines ending on the box and the lines that came out alike drawn once
      (`omaspec_uml::fold`, the flowchart's `layered::fold`); the folded box carries
      the two small squares of a linked state. The pane keeps the folds per tab and
      hands them to the engine as comment lines after the diagram (`' fold (Omaspec)
      Name`), so the file, the checks and the play never see them. A button on the
      corner of the container under the pointer folds it, a click on a folded box
      unfolds it; the palette folds the container at the caret and unfolds them all,
      the preview window with `u`. Shift and a click on a box puts it in focus: the
      play's veil over the diagram, and through it the box in the accent's light,
      its neighbors, their lines and words; Shift and a click on paper ends it, in the
      editor the focus follows the caret onto another box. A play unfolds first and
      starts once that picture is in. `tests/fold.rs` folds in each drawer; headless
      snapshots `focus` and `folded`. Every golden and `tests/lines.txt` stayed as
      they were. Then (2026-11-08): a fold fades longer than an edit and eases both
      ways; when the new picture moved boxes that were there before, they glide to
      their places as their own pixels, a nested box not baked into its container,
      while the lines leave and come back on their new routes (`paper::morph`). The
      box folded or unfolded stays where it was on screen when the diagram is
      zoomed. Only a render asked for after the fold carries its anchor and its fade.
      Then (2026-11-09, addendum to 0024): the lines glide with the boxes
      (`paper::wires`), each from its old route to its new one with the marks at
      its ends, over the two pictures without their lines, which the engine keeps
      in an SVG group of their own (`Layer::Lines`) and the worker parses out; a
      line into a container that folds travels onto its box, and back out when it
      unfolds. The words on the lines still fade with the pictures.
- [x] A sketch look (2026-11-06, [0023](docs/adr/0023-sketch-look.md)): PlantUML's
      `skinparam handwritten true` and Mermaid's `look: handDrawn` (in the init or the
      front matter's `config`), declined until now, with a seed from each element's
      name, so the same source always draws the same picture. Built in the canvas
      every engine draws with (`omaspec_uml::sketch`): every run drawn twice as
      rough.js draws it, a little bowed, its ends a little off, curves and circles
      through points a little off, a fill a little off its outline, text as it is; a
      line keeps its ends on its box and its head. States, classes and flowchart
      nodes seed their strokes by name, other shapes by their size, so a box keeps
      its strokes when the layout moves it. Every diagram kind Omaspec draws, of
      both languages; a Mermaid front matter that only sets the look no longer sends
      a diagram to mmdr. *Skin: sketch* writes it into the source. `tests/sketch.rs`
      draws every diagram of the repository by hand: the same engine, the same words,
      the same picture twice. Every golden stayed as it was. Then (2026-11-07): the
      words in Excalifont, the handwriting face shipped with the program (SIL Open Font
      License, its Latin subset in `omaspec-render/fonts`), still measured as the UI
      font and each run locked to that width, so a box keeps its size and a click, a
      play's lights and Ctrl+click find what they found; a monospaced run stays
      monospace, and bold or italic is not asked of a face with one cut. An exported
      SVG, in a ZIP too, embeds the face as a `@font-face` before the sanitizer's last
      pass, which keeps a `data:font/woff2` or `data:font/ttf` and drops any other
      data URL; the picture on screen does not carry it. A fill with no border (a
      card, a tile, a pie's slice) goes through the hand too, and a card gains a light
      rim; a label's ground, a highlight and a fill that only sits inside another
      shape stay straight (`Canvas::plain`), so they still cover what is under them.
      A package or a folder is one outline, tab and body, so a sketch no longer draws
      the edge they share twice; three goldens (class, component, deploy) changed by
      that alone.

### Since 0.8.0

- [x] Diagrams as text (2026-11-05): `render --format text`, or an output ending in
      `.txt`, and the MCP tool `render` with `format: text` write a state machine or a
      sequence diagram as its behavior, one fact a line, as `git diff` compares it
      after `omaspec git setup` (`omaspec_model::behavior`): for an agent or a
      terminal that shows no pictures. A diagram of another kind says so and points
      to `outline`. Box drawings in characters were weighed and left: a layout engine
      of their own for a picture that reads worse than the facts.
- [x] A Mermaid subgraph's own `direction` documented: Omaspec keeps it also when the
      subgraph's nodes have lines to the outside, where Mermaid drops it (its issue
      6438); held by `tests/layered/flow-subgraph-direction.mmd`.
- [x] Ctrl+click on the word (2026-11-07): a Ctrl+click in the editor's diagram puts
      the caret on the first letter of the label it found, not at the start of its
      line, so `On --> Updating` clicked on Updating lands on `Updating`
      (`omaspec_core::locate::column_of`); a whole word outranks a part of one, and
      the written case any case. The other way, the caret's light knows a label the
      drawing wraps over several lines (an activity's `:…;`) from any of them, and a
      note from any column of it, the participants it stands over included; `end
      note` lights nothing, and a wrapped label or a note is one copy however many of
      its lines match. In the picture (`omaspec_render::pick`), words on a line that
      lie on their own ground are words, not a box; only a shape that reaches every
      edge of the page is its background, so a container that fills most of it is
      still a box, and a click in its empty part names it.
- [x] The pane, smoother (2026-11-08): the light on the name under the caret
      shrinks to a point, travels and spreads over the next place; a box takes it
      over its shape (a wash, a gap of paper and a bright rim, so it reads on a box
      of the accent's own color), and words, a message or a label on a line, a
      quiet wash that stays faint over light letters (`paper::breath`,
      `paper::wash`). A play's light runs along each arrow as one stroke whose head
      eases into the brightness the line rests at; the message it leaves fades from
      where it stood, and the band over the row slides with soft edges
      (`paper::light`). A new diagram's picture waits for its raster with the old
      one still on screen, moving with a pan, a zoom or a new pane size, its marks
      on it and no clicks on them; a zoom under way goes on to its end. Where two
      sheets cross in a fade, solid paper lies under both, so a dark theme no
      longer flashes through. A diagram still on the trail of links keeps its
      picture, so the way back zooms out over it; the veil stays up until that
      diagram has a picture, and a pan, a zoom or a picture that could not be drawn
      ends the wait.

### Since 0.9.0

- [x] The map's arrows around its cards (2026-11-09): seen on a project's map
      with a machine, a use case diagram and three scenarios in one row. Each
      arrow was one curve from the side of its card to the side of the other,
      under every card between them, which hid most of it, and its words lay in
      the middle of that curve, on a card or on other words; between two cards
      next to each other the words covered the whole arrow. Now
      (`tree::cards::arrows`) an arrow between cards of one row runs on a lane
      below the row, 14 units from the next, and the rows further down make room
      for the lanes. The arrows that arrive alike at one card share a lane, a
      head and their words, as lines into one box share a trunk
      ([0022](docs/adr/0022-lines-bundled.md)); the shortest take the lanes next
      to the cards. Between two cards next to each other an arrow runs straight
      across, its words beside it. An arrow from one row to another, or to a
      diagram on no box, is routed around the cards by `graph::route`. Routing
      every arrow that way was tried first and left for the rows: on the
      generated area map of 49 cards and 18 arrows it took 180 ms where the map
      took 31 ms before, and laid lines 4 units apart. With the lanes that map
      takes 33 ms (`OMASPEC_SCALE=1`, cold, 2,001 diagrams).
- [x] A zoom into a tile as one picture (2026-11-09): seen in a real window,
      the zoom into a tile of a map and back still jumped. Every frame of it
      was written out (`snapshot_dive`, with `OMASPEC_DIVE_DIR` and
      `OMASPEC_DIVE_PROJECT`), which showed three things. The linked diagram
      grew out of the whole card, fitted into its frame, while the tile's own
      picture of that diagram lies in the room above the card's words: half
      way, the same diagram stood twice, one beside and larger than the
      other. A tile's link names that room now (`Link::room`), and the
      diagram grows out of it. The tile's picture was the map's overview
      drawn up to five times larger, blurred, until the sharp one faded in
      over it on the last half of the way. Now the linked diagram's picture
      lies on the tile's from the first steps of the way in to the last of
      the way out (`Passage::picture`), so one sharp picture grows and
      shrinks; a state, which shows no picture of its diagram, has it come
      with the veil as before. And that picture, made for its tab, is small
      in its tile: drawn at less than half its size the GPU skips texels and
      thin lines break up, a few with every frame. It is drawn from copies
      halved down to its tile's size (`raster::halves`, made in the
      background and uploaded before they show), the one next above the size
      shown and the next smaller one under it. A frame's work stays at 0.2
      to 0.3 ms (`OMASPEC_FRAMES=1`, headless).
- [x] A map shows a diagram as its tab holds it (2026-11-09): a diagram
      changed in its tab and not saved, in another skin say, stood on the
      map as its file has it, so the zoom into its tile ended on another
      picture. The diagrams a map draws are the open tabs' texts now, each
      in the pane's colors; the map is drawn again when one of them changes
      (after the pause a render waits) and when a tab closes without saving.
      Names and links are still read from the files.

### Since 0.9.1

- [x] Several self-loops on one box, each on its own (2026-11-09): a state
      with two transitions to itself, `Ready --> Ready` twice, had both loops
      leave and arrive at the same two places, one reaching 12 units further,
      so they ended in one head; their words were laid at one height and then
      moved where there was room, which with three loops, or in a nested
      state, was on a line. Now (`graph::loops`) the loops of a box lie around
      one another in the order they are written: each leaves and arrives 10
      units further from the middle of the side than the one before, so each
      has its head, and each reaches out past the words of the one before,
      which lie in the ring between the two; a loop is wide enough for the
      curve it turns in to pass those words. A box grows along that side
      until every loop ends on it: a state with two is 46 units high, with
      three 66. One loop is where it was, so of the goldens only
      `state-loops.mmd` changed; `tests/lines` counts no line near another
      there any more (2 before). Seen in state diagrams of both languages,
      down and sideways, and in a Mermaid flowchart. Found on the way: PlantUML's SCXML
      export keeps only the first line of a transition's label
      (`pay\nnow [ok]` is the event `pay`), which the oracle's random changes
      had not hit until the new golden moved them on; a machine is held
      against the first lines now.
- [x] The map, made ([0026](docs/adr/0026-the-map-is-made.md), 2026-11-09):
      a project's map is made from its diagrams and written nowhere, and a
      mind map or a WBS is a diagram like any other. Seen with the gallery's
      *A garden year* and *Moving house*: every mind map and every WBS that
      is a file is drawn in the map's look, *Map · 0 diagrams on it*, its
      boxes as labels from the top down, also a lone file in no project
      (`Maps::of` takes any place for a project) and an example opened in a
      window whose folder is a project. A rule to tell a map from a plan
      (the project's `main`, a comment line) was built and left for this.
  - [x] The map made by the renderer (`omaspec_render::atlas::folder`,
        `Renderer::render_map`; `omaspec_uml::tree::Branch` for a tree of
        boxes that is made): a tile for each diagram of the folder by its
        file, a folder a label over its diagrams, the arrows read from the
        diagrams. Over 36 diagrams a folder is a tile that opens its map
        and shows a small box for each diagram in it, with one arrow for
        those of the diagrams in it, read from their text without drawing
        them. `omaspec render DIR/omaspec-project.toml` draws it, in any
        look. Still in the tree of the written map. A project of 2,000
        diagrams in 40 folders: 40 ms and 0.3 MB (release; a folder's own
        map small in each tile was tried first, 250 ms and 4.8 MB).
  - [x] The editor shows it: the project's file opens beside its map
        (*New project* opens it); a tile zooms into its diagram as before,
        a folder's tile opens the folder's map in the same tab, the trail
        (`shop › orders`) leading back; a diagram changed in its tab shows
        that way on the map. The preview window shows it for the project's
        file. The written map is gone: `main` (a line that still has it is
        told so), the `system.puml` of `omaspec new`, *not on the map
        yet*, a mind map or a WBS drawn as a map (the fault above), the
        *box* of `refs` and `rename`, a tile's line in a source (Ctrl+click
        on a tile opens its diagram). `render DIR` writes the map as
        `omaspec-project.svg`, `render --check` and `check` hold it against
        the project, `export` writes it as `index.svg` (a folder's as
        `index.svg` in the folder), the MCP server draws it for the
        project's file. Generated projects (`OMASPEC_SCALE=1`, release):
        2,000 diagrams in 40 folders, the map in 35 ms, again in 19 ms,
        0.3 MB; a folder's map of 50 diagrams in 32 ms. Open: the map of a
        folder can be drawn only in the editor and by an export
        (`render` and the MCP server draw the project's); a folder's map
        opens with a fade, not with the zoom a diagram has; the preview
        window does not follow a folder's tile; what a diagram inside a
        folder's tile *opens* is not on the map, since it is read and not
        drawn there.
  - [x] Laid out by its arrows, on the layout of every diagram of boxes
        and lines (`graph::open`; `tree/cards/layout.rs`): a diagram stands
        above the one it plays against, opens or includes; a folder is a
        frame around its cards, laid out with what is around it when a
        line runs into it. The words of an arrow are a pill that is a box
        of the layout: arrows that arrive alike at one card meet in it and
        share one head, and so do those that leave one card alike. More
        than four cards that lead to the same diagrams are a block with
        one line; what no arrow joins is packed beside one another, about
        1.6 times as wide as high, the cards no arrow reaches in a block
        by kind, folders first. The tree, its curves, the lanes below a
        row and the map's own routing are gone. A scenario stands above
        its machine, not beside it: all arrows lead down, so they can
        share a head. Generated projects (release): 2,000 diagrams in 40
        folders, the map in 42 ms, again in 21 ms; a folder's map of 50
        diagrams in 33 ms. Open: a scenario stands in the top row however
        deep the machine it plays lies, so its line is long (the layout
        ranks a box nothing leads to at the top); a chain of diagrams is a
        column, however wide the window.
- [x] A zoom into a state as one picture: the linked diagram's picture
      lies in the state's box from the first steps of the way, as it lies on
      a tile, and grows out of it; it no longer comes with the veil on the
      last half (`Passage::since`; `pictured` is gone).
- [x] The map around a tile, sharp on the way in: the overview is drawn
      twice as sharp as the view wherever that stays under 4 megapixels
      (`SHARPER_PIXELS`), so a zoom of five times magnifies it two and a
      half times, not five. Measured: a map's picture of 2.7 megapixels
      takes about 30 ms in the background; a frame of the zoom still draws
      in 0.3 ms (`OMASPEC_FRAMES=1`).
- [x] A zoom that waits for a slow diagram: the zoom starts as soon as the
      size of the diagram it leads to is known, which the layout gives in
      a few milliseconds, and ends where that diagram will show; its
      picture, rasterized later, fades in on the way from where the zoom
      is by then. Only a diagram whose layout takes over 160 ms (the Java
      PlantUML) still starts without knowing its end, and turns.
- [x] The map's arrows between rows, the dashed curves under the cards of
      a second row, and the pill of a short arrow over both cards' edges:
      gone with the map laid out by its arrows (*the map, made*), which has
      no rows to route between, no curves and a pill that is a box of the
      layout with room of its own.
- [x] Several self-relations in an entity relationship diagram: Mermaid's
      `PERSON ||--o{ PERSON : manages` with a second one on the same entity
      drew both loops and their words on one another, and a line to another
      entity through them. Now each loop has ports of its own, off the
      middle of the side where the lines leave, the loops lie around one
      another, and their words stand past the outermost, beside one another
      (`omaspec-uml/src/er.rs`, `loop_ports`).
- [x] A class called like a keyword: `class Node` with `Node --> Node :
      parent` was read as a node called `-->` (`is not a name this diagram
      draws`), and so was any line from a class called Person, Actor, Queue
      or File. A keyword followed by a line is a name now
      (`structure/parse.rs`, `names_an_end`). PlantUML's `entity Person {
      … }` was declined (`entity is not supported yet`); it is a class of a
      data model now, drawn with «entity», while an entity without a body
      stays a participant, as in PlantUML.
- [x] Self-loops beside one another, weighed and left as they are: with
      `left to right direction` several loops lie around one another below
      the box, which grows as wide as the widest words (three loops, one
      saying *refresh the cache*: 70 units to 154). In a row along the side,
      each with its words below it, the box would grow to the sum of the
      words (about 200 for the same three), so the loops stay around one
      another.
- [x] Lines routed around boxes keep apart: the lines seen 4 units apart
      were the map's, which is laid out by its arrows now and routes
      nothing. Four lines from one box into a container that does not open,
      two on each side, run 12 units apart (`CLEARANCE`), as they should.
- [x] A picture of a project's map in the README and the guide
      (`docs/images/map.png`, `snapshot_readme_map`, `docs/images/update.sh`).

### Ideas after 0.9 (2026-11-09)

Named when 0.9 was done, and weighed the same evening ([0006](docs/adr/0006-goals-and-scope.md)):
two are built, two are declined, four are steps still to take, each with
what it costs.

- [x] A diff as a glide (2026-11-09): *Compare: glide from B to A* shows
      the change in A's pane: B stands as it was, then its boxes and lines
      glide to their places in A, what came fades in and what went fades
      out, on the glide a changed layout has after an edit
      (`Paper::glide_from`: B is shown as if it had just been drawn, with
      nothing gliding to it, and A comes back once B's picture is in).
- [x] A play as a picture that moves (2026-11-10): `omaspec play FILE -o
      play.gif` draws the diagram's pane headless on the GPU, as the
      window draws it (the snapshot tests' `Screen`, now
      `editor/screen.rs`), runs the play (`--events open,close`, else the
      first the machine offers, `--steps`; a scenario its messages) and
      writes the frames as a GIF (`editor/gifs.rs`): one table of colors
      for all of them and of each frame only the part that changed, so
      frames alike are one that stays longer and a light that moves costs
      its own few pixels. An event is named as its label says it (`power
      on`; with its guard where two share a name); `--theme built-in` or a
      `colors.toml` instead of the active theme. The pane is twice the diagram's size up to `--width`,
      the height follows the diagram, and the picture stays in its place
      when the bar takes a second line; it ends on the state the play
      reached. The gallery's coffee machine, six events: 181 frames,
      1200×1020, 1.2 MB, 17 s (9.4 MB with a table and a whole picture
      per frame); it is the README's `play.gif` now, written by
      `docs/images/update.sh`, where a screen recording cut by hand was
      (5 MB). Without a GPU the command says so.
- [x] A fold and a dive as pictures that move (2026-11-10), by the same
      command. `--fold Brewing` folds the container as a click on its
      button does, the diagram glides to its new layout, and it unfolds
      again; several fold one after another and unfold the other way
      round. `--dive Orders` on a project's `omaspec-project.toml` zooms
      from its map into that tile, the diagram stands in its tab under
      its trail, and the way leads back out; several tiles one after
      another. Each ends on the picture it began with, to the pixel, so
      the loop has no seam. What the tests used to draw a glide without
      a render thread (`deliver_overview`, `finish_small`) is in
      `paper/headless.rs` now. The coffee machine's `Brewing`: 23 frames,
      1200×930, 1 MB, 3 s; a map of three tiles, two dives: 44 frames,
      3 MB, 6 s (a glide moves every pixel, so a frame is all of it).
      The project is read and not written. Open: a dive into a linked
      state of a diagram, which is no tile; a fold of a map's folder.
- [x] A color for one box (2026-11-10): *Color this box…* in the palette
      and the menus offers the window's colors for the state or class the
      caret is on, and none. It goes into the source as the language
      writes it, as one undo step (`omaspec_core::recolor`): onto the
      PlantUML declaration (`state Busy #f7768e`, `class Order #ffcc00`,
      the border's `##color` kept), or a declaration added before the
      first line that names a box without one; Mermaid's `style Name
      fill:#…` line, its other properties kept, the line gone when the
      fill was all it held.
- [x] A color for a box of every diagram (2026-11-10): the command found
      a box only in the graph diagrams and wrote a color only for a state
      or a class. Now the source tells the box at the caret
      (`recolor::target`): a node of a mind map or a WBS
      (`**[#f7768e] Tests`), an action or a swimlane (`#f7768e:Bake;`), a
      participant (declared before its first message, the one the same
      line names before it too, so its column stays), a Gantt task
      (`is colored in`), a timing participant, an ER entity, a C4 element
      (`UpdateElementStyle(…, $bgColor=…)`, whose quoted value the C4
      drawer now reads); components, use cases and nodes take it in their
      own form (`[Order] #ffcc00`), a Mermaid flowchart with a decision
      too. The color stands where PlantUML reads it, after the stereotype
      and before `extends`. Where a language has no way (Mermaid's
      sequence diagram, mind map, Gantt, pie; a clock of a timing
      diagram), the command says so. Open: a color of Omaspec's own for
      those. The colors offered: nine fixed ones that are what they are
      called (Yellow is a yellow in every theme), then the theme's, each
      called what it looks like (`hue_name`, *Theme orange*), not by its
      slot: in starship `yellow` is a peach and `green` the accent, in
      lupine `yellow` is a blue.
- [x] A branch of a mind map or a WBS folds (2026-11-10), in PlantUML and
      Mermaid: the box stays as it is, by the same button, click and
      command as a container (0024). The boxes below glide into it and
      come out of it again (`morph::gather`), and the links glide along:
      each is a line of the map (`Map::wires`) in the picture's group
      of lines, so the pane draws it between its old route and its new
      one. No link of a tree is shaped by hand. A link whose box folds
      away shrinks with it to where it left the box above
      (`wires::Line::folding`), and a fold glides also when no other
      box moved: a branch on the side only faded before. The trees' maps
      name their boxes now (`Place::fixed`: none is held beside another),
      two with the same words as `Tests` and `Tests (2)`.
- [x] `<style>` in mind maps and WBS diagrams (2026-11-10), which
      PlantUML styles by nothing else: `node`, `rootNode`, `leafNode`,
      `boxless` and `arrow`, and each box by its stereotype (`.urgent`,
      `<<urgent>>` after its label) and its depth (`:depth(1)`), a later
      setting counting over an earlier one whichever boxes each is for
      (`tree::skin::Rule`). Held against the Java PlantUML's picture.
      Open: `Padding`, `Margin`, `MaximumWidth`, `HorizontalAlignment`
      and a font name on a box decline by name.
- [x] `<style>` in class, component, use case, deployment and object
      diagrams (2026-11-10): each kind of box with its fill, its border
      and its font, and the lines (`structure::skin::STYLE`), as the
      skinparams of the same names; a box by its stereotype too
      (`.important { … }` for `class Order <<important>>`,
      `structure::skin::Rule`), over its kind's look. Open:
      `RoundCorner` on one kind of box and a depth decline on their
      line; timing and Gantt diagrams still decline a `<style>`.
- [x] The menus a mouse expects (2026-11-10): Edit has Undo, Redo, Cut,
      Copy, Paste and Select all, with the editor's own keys beside them
      (the field binds them, the window none; a paste from the menu puts
      a copied line in as text, not as a line of its own). Project has
      *Check the project* (lint of every saved diagram, the findings a
      list of lines to go to, as `omaspec check` without the pictures)
      and *Where it is used* (`omaspec refs` for the name at the caret).
      Then the three that were left: *Find in this file* (Ctrl+F in
      the source; in the diagram it still finds a label), *History of
      the diagram* (the file's last 50 commits, each with how much it
      changed in the behavior, as `omaspec log`; Enter compares the tab
      with it) and *Open recent* (the last 20 files, kept by the
      session).
- [x] Shadows (2026-11-10): `skinparam shadowing true` and a style's
      `root { Shadowing 3 }` declined the whole diagram. Now every box
      casts one, in all PlantUML types: a filled shape with a border, 3
      down and to the right, under the shapes of its layer
      (`Canvas::shadows`), without an SVG filter; a mark as small as an
      arrowhead casts none, the sketch look neither. `Shadowing` on one
      element alone still declines.
- [x] No Mermaid setting passed over in silence (2026-11-10): `themeCSS`
      and every other setting no drawer carries out is a finding on its
      line (`omaspec_core::settings::unheeded`), in the editor, `lint`
      and `check`; a theme no drawer knows too. The `config:` of the front
      matter is read as the directive is, a directive counting over it
      (it was skipped but for `look`), and `fontFamily` and `fontSize`
      count where Mermaid's configuration names them, not only under
      `themeVariables`. Open: a font the machine does not have is still
      replaced without a word (the check does no I/O).
- [x] A label moved to the other side of its line (2026-11-10): *Label
      to the other side* puts the label of the line at the caret on the
      side it is not on, as `Idle -> Busy label left` in the block of
      lines by hand (`hand::Label`, `right`, `above`, `below`); the layout
      keeps a chosen side (`Edge::label_before`, `doubled::label_sides`),
      in state and class diagrams of both languages; *Line: the layout's
      way again* takes it back. From the caret and the menus, not by
      dragging the label: the light on the caret's word already shows
      which label is meant.
- [x] The skins shown before one is chosen (2026-11-10): not as small
      pictures in the menu, which draws text (0025), but in the pane
      itself: pointing at a pack in Edit › Skin, with the mouse or the
      arrow keys, draws the diagram at hand in that pack, the source as it
      is; pointing elsewhere or closing the menu brings its own picture
      back (`App::preview_pointed`). The picture is the real one, at full
      size, instead of a thumbnail.
- [x] Mermaid that mermaid.js reads too (2026-11-11): Omaspec draws a
      flowchart that mermaid.js 12.3.0 refuses, and the same file is a
      parse error on GitHub: a `|` in a label without quotes
      (mermaid-js/mermaid#8750), a symbol in a subgraph's title without
      quotes (#8752), a space beside the `:::` of a class (#8753). Each
      is a hint on its line now, with the way both read it
      (`omaspec_core::lint::portable`), and *Rewrite for mermaid.js
      (GitHub)* or `omaspec fmt --portable` writes it that way, in a
      Markdown document too. When mermaid.js learns a case, it goes
      from the list.
- [x] `D below C` among the boxes held in place said how a line by hand
      reads (2026-11-11); it says now that a box is held in its
      neighbor's rank only. Open: a box put into another rank (`below`,
      `right of`, `level with`), which needs a decision on how it meets
      the ranks the layout gives.
- [x] PlantUML's themes, weighed and declined for now: `!theme` other than
      `plain` stays declined (`the preprocessor's !theme is not supported
      yet`). Omaspec does not ship PlantUML's theme files and reads nothing
      at run time; a swatch made up for a theme would draw it in colors
      that are not the theme's, and a diagram with `!theme` is drawn by the
      Java PlantUML where that is installed, as it is.
- [x] A folded container says what it holds (2026-11-09): a folded state
      *4 states · 6 transitions* under its name, a folded package *3
      elements · 2 lines* where its stereotype stands (`fold::counted`).
- [x] The source over the diagram in a window taller than wide
      (2026-11-09): the diagram stands below the source then, beside it in a
      window wider than tall; the areas are laid out again only when the
      window's shape crosses over, so a split dragged by hand holds
      (`App::restack`).
- [x] Words spelled wrong, weighed and declined for now: lint hints at a
      name close to another name of the diagram already; a dictionary would
      find a misspelled word in a label or a note. `hunspell` was installed
      on the Omarchy this was looked at on, with no dictionary beside it, so
      one would have to be asked for, and read at run time from a path that
      differs by machine; Omaspec reads nothing it does not ship. Left until
      a dictionary comes with Omarchy.

## Other desktops, and a face for the mouse (2026-11-08)

Omaspec's engine is plain Rust; what ties the program to Linux is at the
edges of the window. A Windows build is the next release after 0.9, and the
window grows the menus a desktop user looks for.

- [x] The engine and the binary type-check for Windows (2026-11-08): `cargo check
      --target x86_64-pc-windows-gnu` passes for `omaspec-core`, `omaspec-uml`,
      `omaspec-model`, `omaspec-render` and `omaspec-app`. The Linux parts sit
      behind `cfg`: the Landlock sandbox of the Java PlantUML (`sandbox.rs`, the
      crate, `__sandbox`) is Linux-only and `render_config` asks for it only
      there; Unix modes (the scratch folder, the session file, a hook file) are
      set on Unix and not elsewhere (`files::only_owner`); the Wayland app id is
      `ui::app_id`. The benches do not check for Windows: criterion's `alloca`
      wants a C compiler for that target.
- [ ] Omaspec on Windows: the clipboard without `wl-copy` (iced's clipboard or
      `arboard`), the UI font without `fc-match` (Cascadia Mono, then Consolas),
      no notifications, and a palette without Omarchy: light or dark from the
      system, and the bundled themes to choose from (`omaspec-theme` already
      has `Palette::fallback`). Paths that are joined with `/`, if any. A
      `windows-latest` job in the release workflow that builds and zips the
      binary; a run on a real Windows, since nothing here has been seen there.
- [x] A context menu, a menu bar and a small toolbar (2026-11-08,
      [0025](docs/adr/0025-menus-for-the-mouse.md)): the right mouse
      button over the diagram and the editor with the actions of what is under
      the pointer; a bar of File, Edit, View, Diagram, Project and Help with
      the shortcuts beside the entries, hidden by default on Omarchy and shown
      with Alt; at most ten tools over the diagram pane. All from the command
      list's one table; the list stays the whole.
  - [x] The spike (2026-11-08): `spikes/menu` draws the three from one
        table with `iced_aw` and with an overlay of our own, F9 between
        them. The own overlay is chosen: keys, closing on a click, the
        window's own widgets, no dependency; the spike's pictures come from
        `--snapshot DIR`.
  - [x] The command table (2026-11-08): `editor/table.rs` holds every
        command with its label, its keys, the menu it sits in (and a short
        label for it, a child menu, a line above), its place on the toolbar
        and the context menus it shows in; the list reads it as before, the
        palette itself is in the Help menu and not in its own list.
  - [x] The context menus (2026-11-08): the right mouse button over the
        diagram lands the caret on the box or line there, as Ctrl+click
        does, and opens the diagram's entries at the pointer; over the
        text it opens the caret's. Arrows, Enter and Escape work in it,
        any other key closes it, so does a click beside it; the menu moves
        up or left to stay inside the window, in the diagram's own window
        too. `editor/menu.rs`; the snapshot `context-menu`.
  - [x] The bar (2026-11-08): File, Edit, View, Diagram, Project and Help
        over the tabs, each entry with its keys, a child menu (File ›
        Export, Edit › Skin, Edit › Language) beside its row. Hidden by
        default on Omarchy; a tap on Alt alone or the View entry shows and
        hides it, and the session keeps it. F10 opens File, Left and Right
        walk the menus and go into a child and out of it; the mouse coming
        onto another root switches. The panels float as iced overlays
        (`editor/anchored.rs`), placed by the layout of their root or
        heading and kept inside the window by their measured size. The
        snapshot `menu`.
  - [x] The toolbar (2026-11-08): eight tools over the diagram pane, as
        line icons with a tooltip naming the command and its key: fit,
        zoom in and out, 100 %, play (lit while one runs), unfold every
        container, copy as PNG, export as SVG. It shows with the menu bar
        and has a setting of its own in View, kept by the session; the
        diagram's own window carries it too. `editor/tools.rs`; the
        snapshot `toolbar`.
