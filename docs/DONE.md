# What Omaspec has built

The items of the [roadmap](../ROADMAP.md) that are done, under the headings they
stood under there, in its order, with how each was built. The roadmap keeps what
is open and the reasoning around it.

# Omaspec roadmap

## Between agents and people (2026-11-24)

- [x] The trace as a contract (2026-11-24, [0036](adr/0036-between-agents-and-people.md)).
      [docs/trace.md](trace.md) writes the format down, version 1: the
      fields of a line, how a line is held, what comes back, how a
      program writes one and how an agent works with it. `omaspec trace
      --schema` prints the JSON Schema of a line
      (`omaspec_model::TRACE_SCHEMA`, `urn:omaspec:trace:1`). A
      deviation carries a task (`omaspec_core::lint::Task`, built in
      `omaspec-model/src/task.rs`): the object, its machine's file, what
      the line says happened, and what the machine expected there, the
      states the object could be in and the events they take, each with
      the line of the diagram it is written on; `--json` and MCP `trace`
      give it with lines counted from 1. A test holds every kind of
      deviation to a task whose lines lie in the diagram, each event's on
      the line that writes it.
- [x] What the review of the folds and the trace's contract found
      (2026-11-24). A task of a machine in a Markdown document tells the
      document's lines, not the block's (`Spec::line_in_file`). A line
      that names a state the machine lacks expects where its event leads,
      where the machine starts before an object's first line, and any
      state while the object is not followed, never nothing. The schema
      wants an event or a state that is not null, and text with more
      than spaces, as the check reads them. An open package keeps the
      shoulder past its tab too; a folded subgraph counts no `~~~`, and a
      line to a subgraph inside it ends on its box.
- [x] A run followed on the command line (2026-11-24): `omaspec trace
      run.jsonl --follow` holds a trace while it is written, each finding
      the moment its line is complete, with `--json` one object a line
      (`trace_follow.rs`). It waits for a file not there yet and for the
      end of a line half written, follows a file written anew from its
      start, and standard input until it ends. The check reads a line at
      a time (`omaspec_model::TraceChecking`), so a long run costs no
      more than its lines.
- [x] A run followed on its machine (2026-11-24): `omaspec preview
      flow.puml --follow run.jsonl` follows a trace as it is written
      (`paper/play/live.rs`, a third driver of a machine's play beside the
      writer and a recorded trace). Each object stands as a pill with its
      name beside the state it is in, past three a pill counts the rest; a
      light runs along each transition a new line takes, a burst of more
      than eight is shown where it leads; an object whose last line
      deviates stands in red and the bar says why (`live · 8 objects · 1
      deviates`, the file's line, the last step). The window's watcher
      says when the file changed; a line half written waits for its end, a
      file written anew is followed from its start, and the play starts
      again after each change to the machine. Esc stops following.
      `snapshot_live` draws an agents' workflow with eight objects.
- [x] What the review of the trace followed live found (2026-11-24). The
      preview follows each object a line at a time and keeps its last
      step (`omaspec_model::TraceFollowers`), not the whole trace read
      again at every write; both followers read bytes and cut them at the
      ends of lines, so a letter written across two reads stays whole,
      and start over where another file was put in its place by name,
      longer or not (its inode). `trace --follow` reads a pipe as it
      streams; the preview says it follows files only. Where an object
      deviates, its state alone flashes, and a state another object still
      stands in stays lit while one leaves it. The bar's ✕ and F5 end
      following as Esc does, a link to another diagram ends it, and the
      status line says only Esc stops. `--strict` fails a followed trace
      as it fails a whole one (`commands::fails`).
- [x] Where a person decides (2026-11-24): `state Review <<decide>>` in
      PlantUML and `class Review decide` in Mermaid mark a state where a
      person decides, not the system or its agents (`State::decide` in
      the model); `omaspec convert` writes each as the other, without a
      notice. In a trace followed live an object standing there waits:
      its pill is filled, `‖ pr-42`, and the head counts who waits
      (`live · 3 objects · 1 waits · 1 deviates`). The gallery's *Agents
      at work* (`examples/gallery/agents.puml`, 47 examples now) is an
      orchestrator's workflow with two such places, colored by a
      skinparam of the stereotype, and `agents.jsonl` a run of it to
      follow; `snapshot_live` draws that run.

## PlantUML's guide, drawn natively (2026-11-21)


### Step 0: the yardstick, and refusals that say why

- [x] The yardstick (`tests/guide.rs` in `omaspec-render`): a copy of
      PlantUML's *Language Reference Guide* (`OMASPEC_GUIDE`, through
      `pdftotext -layout`) cut into its 929 examples, each from `@start…`
      to `@end…`, the running header and footer left out where a page
      breaks one (a long section title wraps the header over three lines),
      each drawn by Omaspec's own engines on as many threads as there are
      cores (0.7 s). `tests/guide.tsv` keeps per example its printed page,
      chapter, whether it counts and whether it drew; the guide's text
      stays out of the repository. Whether an example counts was decided
      once with PlantUML 1.2026.8, which drew all 929 in Omaspec's sandbox:
      the 32 it refuses do not count, the 35 network diagrams it wants as
      `@startnwdiag` do. An example that drew and declines now fails the
      test; `OMASPEC_GUIDE_CHAPTER=N` lists what a chapter declines and why,
      `OMASPEC_GUIDE_DIR` writes the examples out. First count: 451 of 897.
- [x] A refusal says why (2026-11-22). A diagram whose first line is
      unusual is typed by what it is: a use case's actor alone (`:First
      Actor:`, `actor :Name: as A`), a `usecase` whose quoted words run over
      several lines (its `--` and `==` are rules, not a sequence's), a
      class with its visibility before it (`-class`), a line with an end no
      message has (`#--`, `x--`, `}--`, `+--`, `^--`, the crow's feet of
      `}|..||`), an element whose words stand in brackets (`folder f [`), an
      `action`; `newpage` no longer makes a diagram a sequence diagram. The
      same `@startuml` twice opens no second diagram. What stands beside a
      line's end names itself: a style (`#line:red`), an end the arrow does
      not take in (`}|`, `#`), anything else that is no bare multiplicity;
      only a bare `1` or `*` still asks for quotes. A `json` or `map` element
      in another diagram is called that, and `colors`, `skinparameters`,
      `listopeniconic`, `listsprite` and `stdlib` say they draw a page of
      PlantUML's own. The guide: 473 of 897.

### Step 1: the sequence diagram

- [x] Slanted messages (2026-11-21): `A ->(10) B` and `A (10)<- B` fall
      that many pixels from the sender to the receiver, whichever side of
      the arrow the number stands on; the heads and an `o` turn along the
      line, a bar the message opens or closes starts or ends where it
      arrives, and its row grows by the fall. As in PlantUML, the number
      stands right after a head on the right or right before one on the
      left, between two participants; one with heads at both ends falls
      toward the right column, and one between two open heads (`<<->>`)
      stays level. A fall of more than 1,000 pixels, or one on both sides,
      declines; the lint's patterns for a declined diagram read the new
      forms. Level arrows draw as they did, to the byte. With them, `note left S1` is `note left of S1`
      where S1 is a participant, as PlantUML reads it. The guide: 56 of
      79 sequence diagrams (from 52).
- [x] A circle at an arrow's end (`->o`, `o->`) is a dot, as PlantUML
      draws it (2026-11-21); it was a ring. It is filled in the arrow's
      color, which follows the theme, where PlantUML fills it black. A
      self message draws it too, at either end; it drew neither.
- [x] Numbers and pages (2026-11-21). `autonumber 1.1.1` counts in its
      last level and keeps the others; `autonumber inc A` raises the first,
      `inc B` the second, each setting those after it back to 1 (1.1.2,
      2.1.1, 2.2.1). `autoactivate on`: a solid message that says nothing of
      bars itself opens one on its receiver, in the color after it
      (`bill -> bob #005500`); a dashed one closes its sender's, `return` the
      newest, and `**` and `!!` open none. `newpage` (with a title or
      without) ends the page with its feet and starts the next under it,
      after a gap, with its title and the heads of whoever is alive; the
      bars open at the break go on, the lifelines leave the gap empty. A
      page inside a group declines. Converted to Mermaid, a page and a
      slant are said to be lost. The guide: 59 of 79 sequence diagrams.
- [x] Participants in their order and over several lines (2026-11-21).
      `participant A order 10` places it among the others, sorted stably,
      those without an order counting as 0, as PlantUML does; a declaration
      that ends with `[` takes its words from the lines up to `]` (its
      Creole rules and headings come with step 5). A header's and a
      footer's page numbers are filled in as the diagram is drawn: the
      header stands over the first page, the footer under the last, and
      `%lastpage%` counts the pages `newpage` makes.
- [x] The sequence skinparams of the guide (2026-11-21):
      `maxMessageSize` wraps a message's words at their spaces at that
      width; `responseMessageBelowArrow true` puts the words of an arrow
      that points left, an answer, under it; `guillemet false` shows a
      stereotype as `<< … >>`; `stereotypePosition bottom` sets it under
      the head's name, at the top and at the foot; `lifelineStrategy solid`
      draws the lifelines solid, as the guide describes it (PlantUML
      1.2026.8 draws them dashed all the same). The guide: 67 of 79.
- [x] A group's colors (2026-11-21): `alt#Gold` colors its tab,
      `alt #LightBlue` the part before the first `else` and `else #Pink`
      the part after it, each painted with the `rect` highlights, under
      the lifelines, an inner group's over an outer's. Converted to
      Mermaid, which has none, they are said to be left out. The guide: 68
      of 79.
- [x] What the review of the pages and the participants found
      (2026-11-22). A note beside a message (`note right:`) may follow the
      bars it opened or closed, under `autoactivate` too: it goes in
      right after the message. `autoactivate` opens no bar for a lost
      message (`->x`) and, between a participant and the edge, acts on the
      participant whichever way it points, as PlantUML does. The words in
      `participant P [ … ]` neither decide the diagram's type nor reach
      lint and the outline (`common::description_opens`); the block ends at
      a line whose `]` no `[` on it opens, as in PlantUML, and takes a plain
      name only. Declared again, a participant keeps its first `order` and
      its words. A number in levels stays bold whatever the format; a
      negative one pads after its sign; `autonumber inc` without a letter
      raises the level before the count, and on a plain number skips one,
      as PlantUML counts. Converted to Mermaid, lines of words become
      `<br>`. A participant named `newpage` stays one. On a page after the
      first the heads and a box's title are drawn as names again
      (`Canvas::echo`), each box stands on each page, the page's title
      shows even with nobody alive, and the heads held while scrolling
      cover the first page only. Left: a bar `autoactivate` opens has no
      `++` for the editor's recoloring to find.
- [x] Teoz's anchors and durations (2026-11-22): `{start} A -> B` names
      the height of a message's arrow, and `{start} <-> {end} : words`
      draws an arrow up and down between two of them, at the middle of the
      first, its words beside it, taking no room of its own. An anchor
      named twice, one no message names before, or one before something
      that is no message declines; converted to Mermaid, a duration is
      said to be left out. The guide: 69 of 79.
- [x] A box inside a box (2026-11-22): `box` within an open `box` nests,
      as Teoz draws it; a participant named inside stands in it and in
      every box around it. The inner box keeps the outer one's padding
      clear where they end at the same column and starts under the outer
      one's title, each depth with a band of its own, on every page.
      Converted to Mermaid, whose boxes do not nest, its participants go
      into the box around it, and that is said. With it, a participant
      named like a keyword sends messages (`Participant -> Bob`), and
      `skinparam sequenceParticipant underline` underlines every head's
      name. The guide: 71 of 79; the rest waits on Creole and font names.
- [x] What the review of the skinparams, colors, durations and boxes
      found (2026-11-22). `responseMessageBelowArrow` puts the words under
      a message written with its head on the left (`B <- A`), as PlantUML
      reads it, not under one that points left; they stand under the arrow
      where it ends lowest, a slant or a head made. `maxMessageSize` (and
      every wrap, `text::wrapped`) keeps a link whole and closes the
      markup open at a break, opening it again on the next line, so a
      wrapped `**bold words**` stays bold. Only `#` and word characters
      are a group's color: `alt # of retries > 3` is words, as before.
      `sequenceParticipant underline` underlines each line, also one
      broken by `\n`; `guillemet false` keeps the spaces a stereotype was
      written with (`<<svc>>`, `<< svc >>`). A box inside a box widens to
      its title inside the ones around it. In a play the ground under a
      point is the innermost box's, and the presentation veils each place
      once. A duration lets the row after it go on (`&`, `activate`),
      draws a piece on each page it crosses with its words by the longest,
      and declines between an anchor and itself; the outline lists
      neither a message from `Participant` as a declaration nor a group's
      colors in its label.

### Step 2: the state diagram

- [x] A start or an end with a name (2026-11-22): `state s1 <<start>>`
      and `state e1 <<end>>` are a start and an end of their own, beside
      the container's `[*]`, which is now only the one without a name.
      The guide: 20 of 36 state diagrams (from 17).
- [x] Hidden arrows (2026-11-22): `-[hidden]->` places its states as a
      transition would and is not drawn, neither its line, its head nor a
      play's step (`Dash::Hidden`, a stroke without width); the machine has
      no such transition, the coverage's marks leave it hidden, and
      converted to Mermaid it is said to be left out. The golden test
      refuses two files of one name in two folders, which wrote one golden.
      The guide: 21 of 36.
- [x] What the review of the named starts and hidden arrows found
      (2026-11-22): a hidden arrow was still a line of the map, which the
      pane glides, lights on a focus, shapes by hand and a build film
      draws; its words were drawn; `-[hidden,dashed]->` was visible, as
      the last word won; the orbit drew its head and words; a box folded
      counted it; a hidden arrow across boxes made the conversion refuse.
      It is now only in the layout: no arrow of the map (`nest::arrows`),
      no words, no way on the ring, hidden whichever its place among the
      words of its style. A start or an end with a name has its spot at
      the orbit's mark, as `[*]` has, so a play finds it; declared again as
      what it is, it stays that; converted to Mermaid, which has only
      `[*]`, it refuses and says why.
- [x] A state's style in one word (2026-11-22):
      `#pink;line:red;line.bold;text:red`, the fill first or none, `back:`,
      `line:`, `line.dashed`, `line.dotted`, `line.bold` and `text:`, which
      colors the state's name and not its description, as PlantUML does;
      `##[bold]` without a color is a bold border in the border's own. The
      colon of the description is no longer one of the style's words. The
      class diagrams read the same words from the same place now
      (`color::Inline`). The guide: 22 of 36.
- [x] A note of its own in state diagrams (2026-11-22): `note "…" as N1`
      and `note as N1` with its words up to
      `end note`, at the top or inside a state: a node of its own kind
      (`Kind::Note`), laid out among the states and drawn as a note, its
      words tied to their line. It is no state of the machine, the orbit
      or the coverage, no place of a play, and converted to Mermaid it is
      left out and told. A line that treats it as a state (a transition,
      `state N1`, `N1 : words`, a note on it) declines and says so. The
      guide: 23 of 36.
- [x] A note on a transition (2026-11-22): `note on link` with its words
      up to `end note`, or `note on link : words`, on the transition
      written last (`Transition::note`, so a fold and a crossing carry it).
      It stands under the transition's words, in the room the layout keeps
      for them, which grows by it (`edge_label`), so lines and other words
      keep off it as they keep off the words; on a transition between
      boxes laid out on their own once that is routed. Its part is named
      after the transition's words (`note:words:…#on0`,
      `graph::open::Note::on`), its words tied to their line. A side
      (`note left on link`), a color, a hidden arrow or a second note
      decline and say so; converted to Mermaid it is left out and told.
      Hidden arrows now reach the layout as its hidden lines, which rank
      the boxes and are not routed. The guide: 24 of 36.
- [x] What the review of the state styles and the notes of their own
      found (2026-11-22): the editor's color action took a style in one
      word for the fill and wrote over all of it; now it changes, adds or
      takes away the fill alone (`#pink;…`, `back:`), and colors a state's
      last declaration. PlantUML takes a state's look from its last
      `state` line as a whole (`state A #pink`, then `state A {`: no
      pink), and so does Omaspec now, where it merged. A note of its own
      needs its name, as PlantUML wants (`lint` and `fmt` read one
      without as an open block, and its made-up name could meet a
      state's); `A.N1` as a state declines as `N1` does; the conversion to
      Mermaid gives a note no id and counts a box with only notes as
      empty; a rename onto a note's name is refused. `text:` colors the
      name of a state without a body only, as PlantUML does, and the
      orbit draws it too.
- [x] A state that receives a signal (2026-11-22): `<<sdlreceive>>`, SDL's
      input, is a state of the machine drawn with a tab in its top left
      corner, as PlantUML draws it, its words below the tab. The guide:
      26 of 36.
- [x] What the review of the notes on transitions found (2026-11-22): a
      hidden arrow turned across the flow (`-right[hidden]->`) no longer
      put its second state beside its first, as a hidden line lost its
      level; it is now the hold of docs/adr/0021, as in a class diagram,
      and ranks neither state. A rename of a state called `On` or `Link`
      rewrote `note on link`; that line names no state now. The note is
      placed once everything is laid out, after lines by hand, which left
      it behind (`link_notes`); the words take their own size again, so a
      click on the note leads to the note and not to the transition, and
      both stand on the side of their room toward the line. A state's
      note keeps off the room of a transition without words.
- [x] Entry and exit points on a state's border (2026-11-22):
      `<<entryPoint>>`, `<<inputPin>>` and `<<expansionInput>>` on the top
      border of the state around them, `<<exitPoint>>` (a circle with a
      cross), `<<outputPin>>` and `<<expansionOutput>>` on its bottom one,
      drawn as PlantUML draws them, each name outside and beside it. They
      are pseudo states of the machine of their own kinds (`Kind::Entry`,
      `Kind::Exit`): a transition to one goes on at once along its way, as
      out of a choice with one way, so a play enters a state there and not
      at its start, and checks, the diff, the behavior and the oracle name
      them. Laid out among the states, they are set on the border once all
      is placed (`state/draw/points.rs`); the lines inside reach on to
      them, and the lines across the border are laid anew to them while
      they stand just outside, so they meet them from outside. A title
      steps off the lines from its entry points, and their names are in
      the way of lines between boxes. Outside a state with states inside,
      or in a concurrent region, a point declines; converted to Mermaid,
      which has none, the diagram is refused. The guide: 29 of 36.
- [x] What the review of the entry and exit points found (2026-11-22): a
      state left through its exit point was remembered as the point, so
      its history came back to leave again; it is remembered as it was on
      the way to the point. Where a state's states run left to right, its
      points stand on its left and right border, as PlantUML puts them. A
      line between boxes at a point is laid with the point just inside
      its state (under its title) when it stays inside, and just outside
      when it crosses the border, then follows the point back onto it; an
      end run across that step slides along instead of growing a hook. A
      note beside a point keeps its room, tied to the point by a dashed
      line; a state's own notes over and under it keep off its points'
      names, and a loop on a point moves with it. A hidden arrow turned
      across the flow onto a state with states inside ranks its states
      again, as no hold beside such a state can be kept. Lines inside a
      state are still routed past its border at times, by the router
      between boxes.
- [x] What the second review of the entry and exit points found
      (2026-11-22): the router between boxes takes, for each end of a
      line, where it stands for that line (`across::Link::stand`) and
      whether the box around it is closed to it (`sealed`). A point stays
      on its border; a line that leads out of its state is laid to it just
      outside, the state closed to it, so the line no longer cuts through
      it; one that stays inside to it just inside, each end on its own.
      The end is then led on to the point square (`points::meet`), not
      along the border, a short step the line took on the way smoothed
      out (`points::straighten`), and two points facing each other across
      a gap are joined straight. The state's own notes keep their dashed lines.
      Where the states run left to right, a point's name stands inside its
      state under its line, as the words of the lines stand over theirs
      and states stand close beside each other there. A hidden arrow
      turned across the flow is laid out as a line within the rank, not
      drawn, which puts its second state beside its first, one with states
      inside too, as PlantUML does. In activity diagrams a note left of a
      `backward` action gets its room between the loop and that action.
- [x] What the third review of the entry and exit points and of the
      hidden activity arrows found (2026-11-22): where a point stands for
      a line is a small square that keeps clear of every box that does not
      hold the point, nearer and smaller where a neighbor leaves little
      room, so a line no longer starts back over the point; the stretch
      from a point to where it stood is in the way of every other line
      (the router's corridors); a short step is smoothed out only where
      the run then lies on no other line; words the line now runs through
      move beside it; a point and a box facing each other across a gap
      are joined straight. Where the states run left to right a point's
      name stands under it, across the border, so two states close beside
      each other keep their names and the words between them apart. A
      hidden arrow turned across the flow holds its second state beside
      its first again between two states without states inside (no port
      taken, a note beside it kept), and is a line within the rank onto
      one with states inside. In activity diagrams only a split loses its
      line on top, a fork keeps its bar; a `while` at the end of a branch
      whose last arrow is hidden has no way out either; the words of a
      hidden arrow stay, as PlantUML draws them. Two points that the
      layout put at one spot on a border, and an expansion's row that
      reaches over an exit point beside it, are older and not yet mended.
- [x] What the review of the connectors and of the third fixes found
      (2026-11-22): a line no longer runs a spur back over itself where
      it meets a point (its end run counts as toward the point on the
      side where the point stood for it); a corridor that covers another
      line's stand is no obstacle for that line, so no diagram declines
      for it; a point is joined straight only to a box lines may meet
      along a side, not to a circle or a diamond; where no stand keeps
      clear, the line is laid to the point itself. A point's name takes
      the first of its spots where no line, box, words or other name is
      in its way, once every line is laid (`points::place_names`). A
      connector is named with one letter or digit, as PlantUML takes it,
      with `;` after it or not; the editor's color action colors it as
      it colors an action. A fork's join bar spans a flow that ends
      before it, so the line on from its middle starts on it.

- [x] The last four of the state diagram (2026-11-24). A state's color
      may run from one into another as PlantUML writes it, `#red-green`
      down, `#red|green` across, `#red/green` and `#red\green` aslant, in
      `#…;line:…` too (`color::parse_gradient`): the canvas fills with it
      while it is set (`Canvas::gradient`) and defines each gradient once,
      by an id its colors and its way name, so a picture inside another
      defines the same; a composite takes one, its border keeps its own
      color. A style for a stereotype (`.foo { state { BackGroundColor …
      } }`) is the look of that stereotype's states, as
      `BackgroundColor<<foo>>` is, its font style too, and a stereotype's
      fill on a composite stays as written rather than stepping toward the
      paper. `json name { … }` among the states is data (`Kind::Data`,
      `state/draw/data.rs`): its name in a band over its value, drawn
      nested as a table by the JSON drawer; no state of the machine, left
      out of a conversion to Mermaid, and a line to it declines. The
      guide: the state diagram 36 of 36, 553 of 897 in all.

- [x] What the review of the gradients, the styles for stereotypes and
      the data found (2026-11-24). Data is drawn through the JSON drawer's
      picture itself (`embedded::picture_or_why`), not as words with
      `{{json`: JSON that does not read declines on its line with what the
      drawer says, objects closed on one line draw, and the table takes
      the state's fill, lines and words, so a dark look reaches it. A line
      `json : words` describes a state named so; the name of data is kept
      as written, and a line, a note or words for it decline saying it is
      no state. A composite holding only data is refused in a conversion
      to Mermaid, as one holding only notes is. A gradient inside a note's
      picture keeps its id, and that picture's paper is left out past its
      definitions. In a dark look a gradient of the file's turns as a
      ground does, both its colors (`repaint`, `legible`). A typo in a
      gradient says so, and a style for a depth or a property a
      stereotype's states do not take names them.

### Step 3: the activity diagram

- [x] A loop that starts with an action (2026-11-22): `repeat :text;`, on
      its line or on more, puts the action where the loop starts, in place
      of the mark the ways back join; the way back and the rail of
      `is (yes)` meet it on its sides. A note after a `backward` stands
      beside that action. The guide: 43 of 62 activity diagrams.
- [x] Hidden arrows in activity diagrams (2026-11-22): `-[hidden]->` draws
      no line and no head, and is no line of the map; a `while` before one
      goes on by no way out, and a split whose flows all start with one
      has nothing joining them on top, as PlantUML draws them. The guide:
      45 of 62.
- [x] Connectors in activity diagrams (2026-11-22): `(A)`, and `#blue:(B)`
      in a color of its own, a circle with its name, which a flow leads to
      and goes on from, a place of the map, its name tied to where it
      stands on its line. A flow that ends at `kill` or `detach` has no
      arrow on into nothing, as PlantUML draws it. The guide: 49 of 62.
- [x] Words on `end fork` (2026-11-22): `end fork {or}`, `end fork {and}`,
      right of the bar that joins the flows, in the arrows' words, tied to
      where they stand on their line. The guide: 51 of 62.
- [x] Notes that find their action (2026-11-22): `floating note left: …`
      is drawn as every note is, without a tail to its action, as PlantUML
      draws a floating one; a note before the first action of its flow is
      that action's (in a partition before its first step, as PlantUML
      puts it), one after a group its last action's. A note that waits
      for an action and finds a step that is none, or none at all,
      declines and says so.
- [x] What the review of the point names and the fork's words found
      (2026-11-22): a point's name is placed once the picture has moved
      down under its title, so it stands at its point; it keeps off the
      title of its state and runs across none of its borders but the
      point's own, and where no spot is free it takes the one it harms
      least (another name, a title or a border most, a box less, a line
      least). `:(y);` is an action that reads `(y)`, as PlantUML takes
      it; `#red:(y);` and `(y);` are connectors; a line that reads `(A)`
      inside an action is the action's for the color action too. A fork
      whose flows all end has its join bar all the same, and what follows
      goes on from it, as PlantUML draws it; the words of `end fork {or}`
      stand beside that bar, and in swimlanes beside its rightmost end, in
      that lane. `end fork {or};` is read; fmt and the outline close a
      fork at `fork end` and fmt at `end merge`.
- [x] `switch`, `case`, `endswitch` in activity diagrams (2026-11-22):
      one diamond above the cases side by side, as PlantUML draws it, a
      line from its left corner to each case left of the middle, from its
      right corner to each right of it, from its foot to the one in the
      middle, the case's words beside its line and tied to it; a diamond
      below where the cases meet. An empty case goes straight on, one
      that stops ends; in swimlanes the cases go in the order of their
      lanes. A step before the first `case`, or a `switch` without one,
      declines and says so. The guide: 52 of 62.
- [x] What the review of the notes and of the point names found
      (2026-11-22): a note before the first action of its flow waits
      for that action in its own flow only; where the flow ends (`else`,
      `case`, `fork again`, the end of a loop, a block or a group,
      `backward`) or the swimlane changes before one comes, it declines,
      never going to an action elsewhere; so does a note after a group
      that holds no action. `floating note` after `backward` is read as a
      note is. The color action does not color an action that reads only
      `(y)`, which in a color PlantUML reads as a connector; it says so.
      A point's name that stands out over the top of the diagram makes
      room for itself under the title. fmt and the outline close a split
      at `split end`.
- [x] `backward` in a `while` (2026-11-22): the way back runs on the
      right through the action and the way out on the left, its words
      left of the diamond, as PlantUML draws it; words on the lines to
      and from the action, `(again) backward :Log; (next)`, as in a
      `repeat`. A note after it is the body's last action's, as PlantUML
      puts it. The guide: 53 of 62.
- [x] What the review of `switch` and the note fixes found (2026-11-22):
      the case in the middle goes straight down from the diamond's foot,
      and the cases on either side keep clear of it and of its words; a
      case in a swimlane left of the switch's takes the left corner, one
      right of it the right corner, so no line crosses another, and a
      branch from another lane is never taken for one straight above the
      diamond where the branches meet. The columns of `if`, `fork` and
      `switch` keep clear of the words on the arrow into each flow;
      branches that are all empty meet below the test, not on it.
      `end switch`, `endswitch;`, `switch (x);`, `case (y);`, a step
      before the first `case` and a `switch` without one are refused as
      PlantUML refuses them; an arrow before the first `case` declines.
      The color action does not color an action that reads only a name in
      brackets of any length; the outline reads a switch's and a case's
      words to their last bracket, as the engine does; a swimlane line
      that names the lane already in use no longer stops a note waiting
      for its action.
- [x] `skinparam conditionStyle diamond` and `ArrowHeadColor none` in
      activity diagrams (2026-11-22): the test of an `if` and a `while`
      stands above an empty diamond, right of the line into it, that of a
      `repeat` right of its diamond, above the line from its right corner,
      with no words on its ways out; a `switch` and a row of `elseif`s keep
      their hexagons, as PlantUML draws them. Words on the arrow into such
      a test stand above it. `ArrowHeadColor none` draws the lines without
      heads, but those written in a style of their own (`-[#red]->`,
      `-[dashed]->`), as PlantUML; a color of the heads' own declines until
      the map hands a head a color other than its line's. The guide: 56 of
      62.
- [x] What the review of `backward` in a `while` and of the `switch`
      fixes found (2026-11-22): the action on a `while`'s way back keeps
      clear of the `yes` and of the words on the arrow into the body. The
      sides of a `switch` move as a whole, so a case that runs on into
      another swimlane keeps clear of its neighbours there; words on the
      arrow into a branch keep the next column off only where it starts
      in the same lane. A branch that comes down within the width of the
      diamond where branches meet turns to its top just above it, not
      through it. `switch x`, `switch (x);` and `case (y);` are refused,
      as PlantUML refuses them; the outline closes a switch at
      `endswitch` only.
- [x] What the review of `conditionStyle diamond` and the last fixes found
      (2026-11-22): the words on the first arrow into a branch stand above
      the test it starts with; a `repeat`'s test keeps below its body and
      its `backward` action right of it; the words of such a test are a
      part of their own (a glide moves them with their test, not with the
      line into it). A `switch` moves only the cases that share a swimlane
      with it, its foot or each other, so a case alone in its lane stays;
      the columns of a row of `elseif`s, which come in the order written,
      keep clear of the words on any earlier arrow in their lane. A line
      that turns to another lane takes no room for what stands above the
      step it leads to; a line that turns into the top of the diamond
      where branches meet has room for its head.
- [x] `!pragma useVerticalIf on` (2026-11-22): the tests of an `if` with
      `elseif`s one below the other, each below the branch before, each
      branch right of its test, the `else` below the last; the branches
      come together on one rail on the right into the diamond where they
      meet, as PlantUML draws them. The preprocessor hands the pragma on
      to the frame every diagram type reads; the others take no notice of
      it, as PlantUML. A compromise: branches in swimlanes of their own
      stay in a row. The guide: 57 of 62.
- [x] `package`, `rectangle` and `card` around actions (2026-11-22), as
      PlantUML draws them: a package as a folder, its name in the tab
      above the frame; a rectangle and a card with their name in the
      middle at the top, a card with a line under it. A name in the middle
      gives way to the line into the frame, to its left where there is
      room, else to its right. The guide: 58 of 62.
- [x] What the review of the stacked `elseif`s found (2026-11-22): the
      words on the arrow out of a branch and out of the `else` are drawn,
      those of an empty branch above its line; the branches keep clear of
      the words beside the line down from one test to the next, and the
      rail of every word beside a branch. The rail is a line of its own,
      in pieces from each branch that ends on it to the next, so the map
      follows each from its branch to the diamond where they meet, and a
      hidden or a colored arrow out of a branch hides or colors its own
      line, not the rail. A branch whose way out comes in from the right,
      as a `while`'s does, goes on to the rail from there, not back over
      itself. An arrow written with brackets is marked so as it is laid,
      not told by its stroke, so `-[#181818]->` keeps its head under
      `ArrowHeadColor none` too. The preprocessor hands on only
      `useVerticalIf` on or off; without a value it is off, as in
      PlantUML.
- [x] A font name for the whole diagram in `<style>`, and a style for
      partitions (2026-11-22): `FontName` on the diagram's own element
      (`activityDiagram`, `stateDiagram`, …) reaches all of it as on
      `root`; the same name again on a part of it changes nothing, another
      still declines. `partition { … }` reaches the frames of an activity
      diagram, their `RoundCorner` too. The guide: activity 59 of 62, state
      30 of 36.
- [x] What the review of the frames and of the stacked `elseif`s found
      (2026-11-22): the words on the arrow out of the `else` take room
      beside the line down, or the rail keeps clear of them; words of
      several lines on an empty branch's arrow keep below the branch
      above; the rail's top corner is round again where the highest line
      into it is plain; where every way is hidden and there is no `else`,
      no diamond waits for lines. Words on the arrow out of a group stand
      below its frame, not on its edge, in every activity diagram. The
      outline knows `package`, `rectangle` and `card`; a stereotype on one
      says which; the parts of a glide name frames by their kind. The
      test of the stacked lines reads the map's lines, which it did not.
- [x] Loops as PlantUML draws them (2026-11-22): a `while` and a `repeat`
      go back on the right, a `while` leaves on the left, with a
      `backward` action or without, as the guide and PlantUML 1.2026.8
      draw them; they were the other way round. A `while` that a hidden
      arrow and `detach` leave, an endless loop, has no way out drawn.
- [x] The color action writes an action's color after its `;`
      (2026-11-22): `:Bake; <<#f7768e>>`, as PlantUML 1.2026 wants it; it
      refuses `#f7768e:Bake;`, which the color action wrote before. A
      color written so is moved there, a shape in brackets (`<<input>>`)
      stays beside it; an action that ends in an SDL mark (`|`) keeps its
      color before it, the only place one is read there. The engine reads
      a shape and a color in brackets one after the other.
- [x] Choices as PlantUML draws them (2026-11-22): a diamond 24 wide and
      high with no words in it, in both languages; its name, mostly what
      the source calls it by, was written in it and widened it. The
      layout gives it the room of a diamond 34 wide, which keeps the lines
      about it as clear as they were (one crossing fewer among the
      examples), and the lines go on to the diamond drawn. The caret's
      light finds a shape with no words by its name in the map, a
      choice's declaration and every line that names it.
- [x] What the review of the font names, the loops and the color action
      found (2026-11-22): `FontName` on the diagram's own element reaches
      its parts, not its title, header, footer or caption, as in PlantUML;
      one font draws everything here, so a diagram with one of those in
      another font declines. The diagram's own element is found by its
      name (`jsonDiagram` too); where one list of elements serves several
      types (`classDiagram`), only `root` counts. A part may repeat the
      diagram's font before it, and across `<style>` blocks. `partition`
      reaches partitions alone and `composite` every frame of an activity
      diagram, each with a look of its own, and `composite { LineStyle }`
      declines again. A `while`'s way out goes on from its rail where the
      line goes on to that side, into the diamond where branches meet or
      to a lane on the left, not back over itself. The color action finds
      an action's end through every line the engine reads (a `note` in its
      words too), keeps a block comment at its end, takes trailing spaces
      for no color, and colors no action that ends in an SDL mark, where
      PlantUML 1.2026 reads none.
- [x] What the review of the choices and of the last fixes found
      (2026-11-22): a way out that goes on from a loop's rail stops short
      of the diamond where branches meet, so it comes into the corner on
      its own side; the step in another lane stands under the loop all the
      same. A line or a note's dashed line that meets a choice beside a
      corner is drawn on to its diamond too. The caret's light finds only
      the shapes whose name the map says is not drawn (choices), never in
      a comment line, and drawn words under the caret come first. A
      skinparam for partitions colors every frame, as in PlantUML; only
      `<style>`'s `partition` keeps to partitions, and no skinparam names
      `composite`. The diagram's own element counts only where every
      element is that one diagram's; a `mainframe` beside a diagram's font
      of its own declines too. The outline reads `package P` without a
      bracket again, and the words of an action over several lines as its
      words, not as lines of their own.

- [x] `label` and `goto` (2026-11-24): a label marks the point halfway
      along the line into the step after it (or on from where its flow
      ends), and a goto stops its flow, as `detach` does, and goes on by a
      line of its own with its head on that point. As PlantUML aligns
      them, a label in another branch above its goto comes down to it: the
      diagram is laid out once more with room before that label
      (`activity/goto.rs`), so the line comes straight across, as in the
      guide's example with its labels "only for alignment". A goto back up
      its own flow, which PlantUML 1.2026.8 leaves as a stub, goes up
      outside everything on its own side. A goto first in its flow takes
      on the line into it, which ends with no head there. A goto to no
      label, or a label without a name of one word, declines on its line.
      The guide: the activity diagram 62 of 62.

- [x] The old activity syntax (2026-11-24): `(*) --> "First Action"`
      is read into the state diagram's tree (`state/legacy.rs`) and laid
      out on its graph, as PlantUML lays it out on one: an action is a
      state, `(*)` the start where an arrow leaves it and the one end where
      arrows lead to it, `if "test" then` … `else` … `endif` a choice whose
      branches start at it with their words in brackets, `===B1===` a bar,
      a partition a box around what is first named in it (named again, the
      same one), `note left: …` and `note right` … `end note` notes on the
      last thing named, words over several lines in quotes, `as A1`, a
      stereotype (`<< Begin >>`). An arrow without a start leaves the last
      thing; after `endif` the flow goes on from the last branch's end;
      one dash lays its end beside its start. It is drawn in an activity's
      look (`Skin::activity`): plain names of 12 points, their lines from
      the left, the test to the left above its diamond, dark bars, and the
      `activity…` skinparams as the state's, `activityShape octagon` too;
      a stereotype with spaces in a skinparam's key (`BackgroundColor<<
      Begin >>`) is read. It is no state machine: the model, a rename and
      a conversion leave it alone. The guide: the old syntax 13 of 13, 548
      of 897 in all.

- [x] What the review of the element faces and the old activity syntax
      found (2026-11-24). A skinparam's stereotype of more than one byte no
      longer panics: the `activity…` keys are read as the state's in the
      skin (`Skin::from_activity_params`), which names a key it declines
      as written, and takes the theme packs' `activityDiamond…` for the
      choices. A test of an `if` has room in the layout, as wide and as
      high as it reaches beside its diamond (`state/draw/actions.rs`, which
      also holds the octagon and the names from the left; a choice's room
      may now be wider than high, `choices.rs`), and it and a branch's
      words in brackets are tied to where they stand on their line. A new
      alias is a new action, `-[hidden,dashed]->` stays hidden, a quoted
      partition keeps a `#` in its name, and every name is kept as its line
      writes it. A note or a title that ends at `(*)` no longer makes a
      new-syntax diagram an old one, and `(*top)` starts one. A
      stereotype's words under its name follow its font, and by hand the
      hand writes every word, while words that name a face keep it. The
      gallery has an activity in the old syntax (`tea.puml`).

### Step 5: what every diagram shares

- [x] A face for one kind of element (2026-11-24): `ParticipantFontName
      Impact`, `ActorFontName`, `NoteFontName` and the rest of a sequence
      diagram's elements, `StateFontName`, `StateAttributeFontName` and
      the state diagram's others. A font carries the face
      (`text::Face`): the family whose metrics measure its words, and its
      place in the skin's list of faces (`text::Faces`), which the canvas
      takes (`Canvas::faces`) and writes for those words alone, the
      family's faces behind it (`Impact, Arial, …, sans-serif`), so a
      viewer without it draws a face of the same kind. With them the state
      diagram's `AttributeFontColor`, `-Size`, `-Style` and `-Name` set the
      words under a state's name, and `StartColor`, `EndColor` and
      `BarColor` its start, end and bars, which PlantUML 1.2026.8 ignores
      but the guide describes. The guide: the sequence diagram 79 of 79,
      the state diagram 32 of 36.

- [x] Creole's rules, headings and the rest of its inline markup
      (2026-11-24). A line of `----`, `====`, `____` or `....` (two of a
      mark are enough) is a rule, single, double, strong or dotted, and
      words between two marks on either side (`--title--`, `== Title ==`)
      stand in its middle; a rule reaches across the box the words are
      written in, edge to edge, in a note, a participant's head, an action
      and a state's note (`Canvas::text_across`), elsewhere across the
      words. `= Heading` to `==== Heading` (a space not needed) are
      headings, four, two and one points larger and bold, the fourth in
      italics, as PlantUML sets them. Inside a line: `<font color=red
      size=20>` (values quoted or not), `<font:monospaced>` a face by the
      family whose metrics fit it, `<back:yellow>` behind the words,
      `<U+221E>` a character by its code, `~~waved~~`, `<w>` and
      `<w:red>` a wavy underline drawn as a path of its own, and
      `<u:red>`, `<s:red>`, `<strike:red>` lines in a color of their own.
      A pair of toggles with nothing between (`----` amid words) marks
      nothing. A value of data (`@startjson`) reads the markup inside its
      lines alone, as PlantUML does: `----` there is words. A picture file
      (`<img:logo.png>`, `<img logo.png>`, `{scale=0.5}`) is read as an
      `!include` is, inside the project folder, outside hidden folders, at
      most 4 MB (`Includes::picture`), before the diagram is parsed and
      drawn, and embedded as it is, PNG, JPEG, GIF or WebP; one that
      cannot be read says so in its place, `(cannot read logo.png)`, as
      PlantUML says it cannot decode it. Mermaid's words take none of
      Creole's rules, headings and wavy lines (`omaspec_uml::as_mermaid`,
      around every Mermaid drawer), and a `~` in a Mermaid state's words
      is a tilde; a struck ghost in a diff's picture is struck with
      `<s>`, since a line of `--unlock--` is a rule. The guide: 528 of 897, the
      sequence diagram 78 of 79, the activity diagram 61 of 62, Creole's
      own chapter 21 of 44.

- [x] What the review of Creole, pictures and `label`/`goto` found
      (2026-11-24). A line starting with a mark and a letter of more than
      one byte (`=Übersicht`, `-→ next`) no longer panics: a rule's marks
      are compared as bytes. Two or three marks alone (`--`, `...`) are a
      rule only among other lines, as PlantUML reads them; a message of
      `--` is words, and a rule alone is 24 points long. The end of a
      wave, and the end of every tag, takes back only what it changed, so
      a color closed inside a wave stays closed; a wave wrapped onto the
      next line stays a wave. A value of data draws tables again, only no
      rules, headings and lists (`text::as_data`); Mermaid's words leave
      the markup only PlantUML draws (`~~`, `<font>`, `<back>`, `<w>`,
      `<U+…>`, `<img>`) to mmdr, as before. Pictures are read from the
      preprocessed text, so one an included file or a variable names is
      read too, relative to the diagram, and a diagram inside a note keeps
      them. A goto down goes around what stands between on a rail, a
      goto's rail stays in its own swimlane, a label after a flow that
      stopped stands below it and the room before it draws no line, a
      label first in a flow leaves its arrow's words room, a label written
      twice and a goto after `stop` decline on their line.

## Phase 0: foundation and spikes

- [x] Workspace, MIT license, `AGENTS.md`, CI (fmt, clippy, test, cargo-deny)
- [x] `unsafe_code = "forbid"` for the whole workspace
- [x] `omaspec-theme`: Omarchy palette with the same fallback cascade as `omarchy-theme-color`; `omaspec theme`
- [x] Spike editor widget → [0002](adr/0002-editor-widget-scrive.md): scrive
- [x] Spike Mermaid rendering → [0003](adr/0003-mermaid-mmdr.md): mmdr
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

## Phase 2: rendering (done)

- [x] `Renderer` routes: Mermaid → mmdr; PlantUML → `omaspec-uml` (sequence diagrams since
      N1, mind maps and WBS since N2, activity diagrams since N3, state diagrams since N4,
      class, component, use case, deployment and object diagrams since N5, timing
      diagrams and Gantt charts since N6, JSON, formulas, YAML, salt, nwdiag, EBNF,
      regular expressions and ditaa since; chronology, Graphviz and tree diagrams
      are still not drawn at all; anything else
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

## Phase 3: preview window, v0.1 (built; waiting for the side-by-side check and the AUR)

- [x] `omaspec preview FILE`: preview-only window that follows the file, for Neovim users.
      Watches the directory, so saves that write a new file and rename it count too;
      reading the file or the theme raises no event (no render loop)
- [x] Zoom, pan, fit, 100 %; re-rasterize on zoom so nothing blurs. Two layers: an overview
      of the whole diagram (at most 16 megapixels) and, when that is not sharp enough, the
      visible part at full resolution with a margin, so long diagrams stay sharp at any zoom
- [x] Errors shown in place, never a crash: status bar and frame color; the last good picture stays
- [x] Live theme switch: watch `~/.local/state/omarchy/current/`

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
      `vendor/`, see [0002](adr/0002-editor-widget-scrive.md))
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
      see [0003](adr/0003-mermaid-mmdr.md)). A diagram theme follows the active
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
- [x] Lines that curve ([0011](adr/0011-curved-lines.md)): in state diagrams (PlantUML
      and Mermaid), structure diagrams and Mermaid flowcharts each corner of a route turns
      in a curve of up to 48 units, no further than half of either run, so a jog becomes
      an S and a detour an arc, inside the corridor the router found; arrowheads stay on
      the line. A play's light runs along the curve. `skinparam linetype ortho` (or
      `polyline`) and Mermaid's `"curve": "linear"` or a step keep right angles; activity,
      sequence, timing and signal-flow diagrams keep them anyway
- [x] Lines laid out by Graphviz's rules ([0012](adr/0012-lines-like-graphviz.md)),
      found by drawing the gallery with Java PlantUML beside Omaspec: a back edge runs
      through the ranks beside the way there instead of around the diagram, lines spread
      along a side in the order of their other ends, a line between two boxes that overlap
      runs straight, labels of a pair go outside, a container's band is open above except
      for its name, and `left to right direction` holds for a whole structure diagram
- [x] Containers laid out with what is around them, as Graphviz lays out clusters
      ([0013](adr/0013-containers-like-graphviz.md)): a package, node, nested state
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
- [x] No network access and no telemetry ([0005](adr/0005-offline-no-telemetry.md)):
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

## Track V: behavior specifications, understood (`omaspec-model`)

- [x] State and activity diagrams tie what one line writes, as the sequence diagram
      does (2026-11-18). A state diagram, PlantUML's and Mermaid's, ties a transition's
      label, each line of a state's description and a note; an activity diagram its
      actions, tests, labels, notes and partitions, each with the lines it spans. A
      state's name and a swimlane's, which many lines mention, are counted: a second
      `|Build|` lights the lane, not the action `:Build;` after it. A measured block
      carries the line that alone writes it (`Block::written_on`), and every drawer
      ends its map with `Canvas::place`, which hands what was tied to the pane; the
      rule is in AGENTS.md. A line that names a state and labels a transition with the
      same words still counts as the state's mention. Mermaid flowcharts are laid out
      from mmdr's graph, which knows no source lines: they are counted as before
- [x] Class diagrams tie what one line writes, too (2026-11-19): a member, a heading
      among the members, a stereotype, a line's label and both its multiplicities, a
      note and a note of its own (`note "…" as N`), in PlantUML and in Mermaid, and so
      component, use case, deployment, object, C4, ArchiMate and requirement diagrams.
      A class's name, which many lines mention, is counted. The `"1"` of one line no
      longer lights the `"1"` of another, and a label or a note that reads as a class
      keeps apart from it
- [x] Mermaid flowcharts tie what one line writes, too (2026-11-19): a line's label
      (`-->|yes|`, `-- yes -->`), a node's label where a line gives it one
      (`A[Start]`), and a subgraph's title of its own. mmdr's graph keeps no lines, so
      each line is read by mmdr on its own, under a header; what it finds there,
      also a chain's lines and an `&`'s, is what that line writes, and the graph's
      lines are found among them in the order written. A `yes` drawn inside a
      subgraph no longer takes the light of a `yes` written above it. A node that
      shows its id, which every line naming it writes, is counted; so is a line a
      folded subgraph ends on. The reading costs nothing measurable: 150 boxes and
      476 lines draw in 0.41 s as before
- [x] What the reviews of the tied words found (2026-11-19). The pane guessed which of
      the same words on one line was which copy: the last ones, in the order drawn.
      A flowchart's label stands between its ends (`A -->|Done| Done`), nodes are
      drawn before lines, and a line turned `-up->` draws its multiplicities the
      other way round, so the light and the click went to the other copy. Words
      written amid others now say where they stand (`Block::written_at`, `Tied::col`):
      a flowchart's label and a node's, found by mmdr itself (each place of the
      words written as a mark of its own, the line read again), a multiplicity and a
      note's words before its alias. Words without a place close their line, as a
      message after its colon, and take the last places left there; what no tied
      copy takes is a mention, counted. Worked out once per text, not per copy. A
      caret on an alias before a message that reads as its participant lights the
      participant again. A note over several lines is tied to the line its words
      start on, not to the head that names its target (`block_text`), in sequence,
      state and class diagrams. A stereotype is found in its angles (`<<Entity>>`),
      not in the class's name before it. The flowchart reader skips `accTitle`,
      `accDescr` and the lines of `accDescr { … }`, but not a node named `access`;
      a label two lines give one node is counted, not tied to the last; and a line
      moved onto a folded subgraph keeps its line, one that stands for several
      alike none. mmdr's panics the engine catches no longer print on every redraw
      (`caught::quiet`, the app's panic hook). An activity tile boxes its arrows: a
      hundred nested blocks drew on the 2 MiB of a plain thread only just, and the
      tied place tipped it over; they now draw on 1.4 MiB. Reading where 2,000
      flowchart lines are written takes 22 ms, 39 ms where every line writes its
      label twice
- [x] What the review of 2026-11-18 (evening) found in the day's later work. A `++` in
      a quoted name, a title or a note is letters: the bar's sign is read where the
      engine reads an activation, in one place (`recolor::bar_sign`), where *color the
      box* wrote a color into a participant's name. A click finds words before a shape
      under them, a note on a bar. Tied words are told by the words themselves, not by
      a box they name; a line that names a participant and draws the same words
      (`User -> Login : Login`) keeps them apart by where the caret is; the words of a
      block are found on the lines after the one it begins on (`note … end note`). All
      mentions of a text are read in one pass (`locate::columns_of`): each caret move
      read the whole source once per mention, two seconds in a diagram of 800 lines. A
      click on a pill is a click on the participant's name, also under a figure; a
      pill begins to form as its head reaches the line it will stand on, without a
      jump at the pane's edge; halos lie under all pills; one picture is drawn at the
      cross-fade's strength, not the overview under the sharp one; a head's shadow is
      covered with it; a camera step keeps the pills' height free, not the band's
- [x] The right mouse button on a bar of a lifeline colors the bar (2026-11-18). A
      drawer tells a shape without words with the line that draws it
      (`Canvas::shape_of`, `Map::shapes`); a click on a bar puts the caret on the line
      that opens it, and *Color the box at the caret* writes the color there: after
      the participant of an `activate`, after the `++` of a message
      (`Alice -> Bob ++ #f7768e : hi`). And the lines a drawer ties words or shapes to
      are read as the file's lines where a look lays its skin before the source: they
      were some seventy lines off in the window under a theme's look, so words tied to
      a line were not found there
- [x] Every mention of a participant lights its head (2026-11-18). The second mention
      and every later one lit the foot of the lifeline, the second copy drawn, so a
      click on a name in a message sent the light to the bottom of the diagram. A
      drawer now says where it draws a name again for what is drawn already
      (`Canvas::echo`, `Map::echoes`), and a pane counts the first drawing only; a
      click on the foot lands where one on the head does
- [x] *Color the box at the caret* colors the bar on a lifeline too (2026-11-18): with
      the caret on an `activate` line of a PlantUML sequence diagram the color is
      written after the participant (`activate Bob #f7768e`), another takes its
      place, and none takes it away. On any other line the participant under the
      caret is colored, as before. Mermaid writes no color for a bar
- [x] Words one line alone draws belong to that line, whatever else reads the same
      (2026-11-18). A label in the picture was matched to the source by its words and
      by how many came before, so with `== Payment ==` in a diagram that has a
      participant `Payment` a click on the divider landed on the next message, and
      that message's line lit the divider. A drawer now says for which line it draws
      words (`Canvas::tie`), the map tells them (`Map::tied`), and a pane takes such
      words by their line, in both directions, and counts only the others
      (`paper/tied.rs`). The sequence drawer ties its messages, notes, dividers,
      delays, references and the words of its groups; a participant's name, which
      many lines mention, is counted as before. Other drawers tie nothing yet and are
      counted throughout
- [x] The light on the name under the caret goes to a participant's head from its
      declaration, also where a line before it holds the letters of its alias
      (2026-11-18): with `title Reviewing an AI-written PR` over
      `participant "Claude" as AI` the title counted as the first mention of Claude, so
      the declaration was the second and lit the foot. An alias counts from the line
      that declares it
- [x] A sequence diagram scrolled past its participants holds each of them as a
      pill at the top of the pane, PlantUML's and Mermaid's alike, not the whole band
      across the top (2026-11-18). As a participant, an actor, a database or a queue
      reaches the top it turns into a small pill with its name, in its own fill and
      border, over its lifeline; how far it has turned hangs on the scrolling alone, so
      it glides as the diagram does. What scrolls by is seen between the pills and goes
      under each softly, into the paper's color or the box's. The name is the picture's
      own, made small, so its font and its colors are the diagram's. The map names them
      (`Map::heads`); a click on a pill picks the participant, a click between two what
      scrolls by there. Swimlanes and a Gantt chart keep their band whole
      (`snapshot_heads`)
- [x] On a laptop with two graphics cards Omaspec draws with the one that takes
      little power (2026-11-18): iced asked for the strong one, which stayed awake while
      the editor was open. `WGPU_POWER_PREF=high` asks for it still
- [x] Bands in a real window where a color thins out (2026-11-18): under the
      participants held at the top four bands with a ramp in each stood where what
      scrolls under them fades out, and a play's veil was gray with a band at its edge.
      iced took `Rgb10a2Unorm` for the window's surface, the first format that is not
      sRGB a desktop with ten bits of color offers under Vulkan, and its two bits of
      alpha left every fill that lets something through four strengths, as the meshes
      are drawn on a layer of that format and blended from it. The vendored `iced_wgpu`
      takes a format with eight bits to each channel (`vendor/README.md`). Found in a
      window on a monitor of Hyprland's that shows nothing (`hyprctl output create
      headless`), captured with grim: right under OpenGL, wrong under Vulkan, right
      with the change. No snapshot shows it, as a picture drawn without a window has
      eight bits
- [x] What the review of 2026-11-18 found in that day's work. A `&` or `/` at the start
      of a line of a note over several lines is the note's text again, not a sign. A
      no-break space stays at the end of a line only after a bare `title`, `header` or
      `footer`; every other line ends with its last word as before. Lint and the
      messages that quote the source share one list of characters nobody sees
      (`omaspec_uml::unseen`), so a tab, a no-break space and an emoji's joiner are
      written as they are. A note with `/` that would lie on the note before it stands
      below it. No two shapes of an activity diagram share a part's name (`:A #2;`
      beside two `:A;`). The checks and the reading across machines begin a region
      without a start where a play does (`Machine::region_begins_in`). An activity
      diagram follows its lines from shape to shape only when a pane asks for the parts
- [x] Two finds of the nightly fuzzing (2026-11-18). A Mermaid state or subgraph opened
      more than once inside itself, with a line to it there, in a diagram Omaspec's own
      engines decline, sent mmdr's label placement onto a grid that grows each time,
      more than 4 GB of it; such a graph is refused before mmdr's layout, by the box's
      name (`mermaid::holds_itself`). One written once with a line to itself inside is
      drawn: mmdr takes milliseconds for it. And a PlantUML
      `note across` in a sequence diagram without a participant panicked the conversion
      to Mermaid, which now refuses it on its line.
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
- [x] V5 an MCP server, `omaspec mcp` ([0008](adr/0008-mcp-server.md)): JSON-RPC
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
- [x] V6 a semantic diff ([0009](adr/0009-diff-and-convert.md)): what changed in the
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

## Track N: native PlantUML (`omaspec-uml`)

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
      Messages and notes at one height since 2026-11-18 (REQ-078): `&` before a
      message or a note, `/` before a note, puts it at the height of the one before.
      The arrows of such a row lie on one line, under the tallest of their texts, and
      notes of one row make room for one another. `!pragma teoz true`, which PlantUML
      needs for `&`, is read and changes nothing. `&` before anything else declines.
      Converted to Mermaid, each stands at a height of its own, with a note saying so.
      Declines, naming the line: `newpage`, `autoactivate`, `handwritten`, `shadowing true`,
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
      (drawn, their corners curve since 2026-10-27, [0011](adr/0011-curved-lines.md)).
      Lines that join the same two boxes run side by side. Back edges run in a gutter left or right, whichever side
      their states are free on, each in a lane with room for its label, and the
      gap before a rank widens where they turn (since 2026-10-28 they run through the
      ranks instead, [0012](adr/0012-lines-like-graphviz.md)); self-loops sit on the free side. A nested state lays its own children out, in its own
      direction, and the box is the title plus that layout (`left to right
      direction` stays inside the box; a note's "right" stays on the right, in room
      the layout keeps for it). Names belong to the whole diagram, as in PlantUML:
      a transition may join states in different nested states (ranked where both
      meet, routed around everything afterwards, `omaspec-uml::graph::across`;
      since 2026-10-28 laid out with the states around them,
      [0013](adr/0013-containers-like-graphviz.md)),
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
      with curved corners ([0011](adr/0011-curved-lines.md)). Skinparams the
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
      color; timing `rectangle` rows take `Rectangle…`. `printscale` (or
      `projectscale`) `daily`, `weekly` (ISO weeks, or `with calendar date`),
      `monthly`, `quarterly` and `yearly`, with `zoom`; durations in months of 30
      days, as PlantUML counts them (2026-11-13). Declines, naming the line: dates and
      clock times on a timing axis, `{…}` states, `use date format`, a manual axis,
      compact mode, `<style>`, stereotypes, `has … as`, analog ticks and a pixel
      height, `@clk`, a note on a robust, analog or clock row, gradients, closed
      days, resources, links, week numbering, a chart wider than 3660 days on a
      daily scale (a line per day; a coarser scale draws more), months, quarters or
      years without a date, and an unknown line; a chart whose last day is more than
      7300 days (20 × 365, a few days short of 20 calendar years) after its first is refused, as PlantUML 1.2026.8
      refuses it, and the message names the last day the chart can reach
      (2026-11-15). Golden SVGs and Ctrl+click on every label.
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
- [x] The Java PlantUML as the oracle ([0004](adr/0004-native-plantuml.md)),
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
      of these declines naming the line; arrows with a direction (`-left->`) are read
      since 2026-11-16 (REQ-075, [0027](adr/0027-a-line-within-a-rank.md)). Mermaid→PlantUML conversion gives a Mermaid id PlantUML does not take a
      plain name and shows the id (`state "long-wait" as long_wait`); it quoted it in
      transitions before, which PlantUML refuses. In 4,000 changed diagrams no machine
      is read otherwise any more
- [x] Characters nobody sees (REQ-080, 2026-11-18): `title` and one no-break space is
      a title that shows nothing, as in PlantUML, not the first line of a block without
      its end (a no-break space that ends a line as a word of its own stays; at the
      end of a word it is trimmed as before). A message that quotes the source writes
      a zero-width space, a direction mark or any space but the plain one as its code
      point (`Al<U+200B>ice` is no participant name). Lint's `unseen-character` warns
      on each line that holds a zero-width space, a word joiner, a soft hyphen, a
      direction override or a control character, says after which word, and its fix
      takes them out; a no-break space, the joiners of emoji and scripts and a byte
      order mark at the start are left alone.
- [x] `hide`, `show`, `remove` and `restore` in class diagrams (REQ-081, 2026-11-19),
      and in the other structure diagrams: read wherever they stand
      (`structure/hide.rs`) and laid on the boxes once all are known, the last line
      about a box deciding, as PlantUML does. A part (`members`, `fields`/`attributes`,
      `methods`, `circle`, `stereotype`) of all boxes, of a kind (`class`, `interface`,
      `enum`, `abstract`, `annotation`, `entity`, `object`, unless a box has that name),
      of a stereotype, of one box, of the members of a visibility (`hide private,
      protected members`); a box itself by name, stereotype, kind, `@unlinked`, `$tag`
      (read off its declaration) or `*`. A hidden box keeps its place and is not drawn,
      nor are its lines or its notes; a removed one is gone with them and every index
      left is renumbered. The rules and headings around members that are hidden go
      with them. `hide empty …` changes nothing (no empty compartment is drawn), and
      `hide circle` takes away the kind's word that stands for the circle; hiding a
      stereotype gives that word back. A name that is no box and no kind's word
      declines on its line (`hide footbox` in a class diagram)
- [x] A class diagram of lines alone (REQ-082, 2026-11-19): a line between two names
      that the sequence diagram reads no message in (`Usuario <|-- Cliente`, `A *-- B`,
      `A -- B`, `A ..> B`) makes a class diagram, as PlantUML tries the sequence diagram
      first and then the class diagram; so does `hide`/`show` of a class's part. `A ->
      B` stays a sequence diagram
- [x] `skinparam groupInheritance` and a readable self-association (REQ-083,
      2026-11-19). The lines up into one parent under a triangle, as many as it says
      within one package, meet in one stem under a single triangle
      ([0033](adr/0033-subclasses-as-a-tree.md), `graph/join.rs`): the gap beside the
      parent is at least 60, the lines join 0.6 of the way into it and go on as one;
      a tree that would run along another line or through a box is not drawn. A
      self-loop with a multiplicity is 18 high and reaches 40 out, the multiplicity
      over the run that leaves and under the one that arrives, outside the loop where
      its words are not; the next loop around it passes over the marks. They stood
      inside a loop 14 high before, on top of each other
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

## Signal flow (`@startflow`)

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

## From the tester's round of 2026-11-23

- [x] What the nightly fuzzing found (2026-11-23): a state diagram's fork
      bar exactly as long as it must be cut itself back past its start,
      `505.93704 + 40 - 40` rounding to `505.937`, and `clamp` panicked
      on the bounds (`graph::span::trim`). The window's last start is now
      never below the bar's.
- [x] A scenario plays every message (REQ-108, 2026-11-23): `omaspec
      play` took 8 steps without `--steps` whatever it played, so a
      sequence of 13 messages stopped at `8 / 13` without a word. Now a
      state machine takes 8, which may go round for ever, a scenario
      every message and a git graph every commit. One stopped short by
      `--steps` says so (`played 3 of 10 messages`), and steps past a
      scenario's end no longer hold its last picture again for each.
- [x] What the review of the fork bar and the scenario's play found
      (2026-11-23): a scenario without `--steps` plays up to 50 messages
      or commits; each keeps some two seconds of frames until the film is
      written, 5 MB of them for a large sequence diagram in full color, so
      300 would have kept 1.6 GB. One that stops there says so and how to
      play them all (`--steps 300`), and a step that does not move it on
      ends the play. A state machine takes as many steps as `--events`
      names where they are more than 8, and a wrong event after the
      eighth is said before anything is drawn. A line's end on a short
      flat side near the origin had the same rounding as the fork bar, its
      stretch running backwards (`graph::route::usable`). One message or
      commit is said in the singular.
- [x] A box's name with a space is refused, and a `'` is a letter
      (REQ-112 and REQ-102, 2026-11-23). After `D --> Order parts`
      Mermaid stops at `parts`, a parse error (its `flow.jison` has no
      space in a name), and mmdr drew a box `Order`, the rest lost
      without a word; so with `Order parts --> B`, `A --> B & Order
      parts` and `B:Order parts`. Render and lint now refuse such a line
      alike, its words to go in brackets (`Order[Order parts]`). The
      lexer of a flowchart's statements and links moved to
      `omaspec_core::mermaid_flow` for both; its refusal reads past the
      front matter, directives and comments, and past quotes and brackets
      over several lines (a Markdown label, `accDescr { … }`, shape
      data). mmdr takes a `'` for a quote mark where Mermaid takes it for
      a letter: `a[it's] --> b` lost `a`, and after `a -->|don't| b --> c`
      the rest of the chain. In a box's text, in quotes and in a link's
      label it reaches mmdr as a letter of the private use area and is
      put back in the words mmdr read (`layered::reading::read`); a
      statement about parts keeps its quotes.
- [x] Lint says a flowchart's spacing is not carried out (REQ-110,
      2026-11-23): `flowchart.nodeSpacing` and `rankSpacing` reach mmdr's
      layout alone, which draws a flowchart only where Omaspec's drawer
      declines it, and lint counted them as carried out; a picture with
      150 was as large as one without. Now each is a warning on its line,
      so `lint --strict` fails, as with `padding` and `elk`.
- [x] What the review of the names with a space, the quote marks and
      the spacing found (2026-11-23): an invisible link with a space on
      one side only (`A ~~~B`) and an `accDescr { … }` whose words hold a
      `)` or a `]` were refused; `~~~` is a link to the lexer now, and a
      description is words up to its `}`. A `>` in a label in bars
      (`-->|n>0|`) no longer opens a box that hid the lines after it, nor
      one in the words of a link (`-- n>0 -->`). The header is found and
      judged as `mermaid_head` has it (`Flowchart TD`, a flowchart without
      one), and a swimlane diagram is refused alike, by its entry's
      `wrong`. The refusal names the whole run of words, and says so
      where an `&` is written onto a name or a box's text follows its name
      after a space. Omaspec's own scanners of the source took a `'` for
      a quote mark as mmdr did: a double circle, shape data or an
      invisible link after one was lost; they take it for a letter. An
      edge mmdr cannot read is found on its line with a `'` in it, as mmdr
      quotes its masked copy. The spacing's warning says that mmdr takes
      it where it lays a flowchart out.
- [x] A hidden arrow is named where it alone meets a state (REQ-113,
      2026-11-23): a hidden arrow is no transition, as decided on
      2026-11-22, so a state only it leads to cannot be reached and one
      only it leaves is left by nothing; the warnings said so without
      the arrow drawn there, which seemed to say otherwise. The PlantUML
      reader keeps the hidden arrows beside the machine
      (`plantuml::read_placed`), and the checks say "the hidden arrow
      into it only places it" and "nothing leaves A but a hidden arrow".
- [x] `play --help` says what takes `--look` (REQ-109, 2026-11-23): it
      said a play without `--ink` was drawn in that look, which the
      command line refuses (`--look` requires `--ink`; 1.0.0 took it and
      drew the play dark all the same). It says now that only `--ink`
      takes it, and that a light play takes a light theme's colors.toml
      as `--theme`.
- [x] What the review of the hidden arrows, the help and the last fixes
      found (2026-11-23): a hidden arrow is named only where a transition in
      its place would reach the state, from a state reached and not from
      itself; one into a state inside a composite names it at the
      composite, one that leaves a composite at a state inside, and two
      are said as two. `omaspec diff`, `log`, git's textconv and `render
      -o x.txt` say what lint says, from the same reading. The help says
      "without `--look`" where it said "else". Of the last fixes: a `>`
      in the words of a link (`-- n>0 -->`), a `|` in a quoted label
      (`-->|"a|b"|`) and a one-line `accTitle`, `accDescr` or `title`
      with an open bracket hid every line after them from the refusal;
      a link and its label are words to it now, and such a line is text.
      A `;` in a label in bars no longer cuts a statement in two. The
      refusal is worded by what follows the first name (boxes side by
      side want a link), and its words go in brackets after the bare id,
      without classes. Shape data is YAML, where a `'` quotes
      (`label: 'Order, ship'`); a box's text keeps its `'` as Mermaid
      draws it (`A['x y']`), on a line that is repaired too; and a box
      mmdr misread beside an invisible link is found by the name it read
      in its masked copy, so no stray box is left.
- [x] Containers folded on the command line (REQ-104, 2026-11-23): `omaspec
      render FILE --fold Brewing,Payment` draws those containers folded to
      their box, as the editor folds them (docs/adr/0024): a composite
      state, a package, a subgraph; the box says what it holds and its
      lines end on it. The names reach the engine as the pane hands its
      folds over, as lines after the diagram (`crate::folds`), so the
      source an image keeps is the file's. A diagram is drawn once as it
      is first, for the containers it holds; a name it does not hold is an
      error that lists those, and nothing is written. Of several diagrams
      (a folder, a Markdown document), each folds what it holds, and a
      name none holds is an error. The status line and `--json` say what
      was folded; text and a presentation, which are the whole behavior,
      refuse it. `omaspec export DIR --fold NAME` folds it in every
      diagram of the project that holds it, as SVGs and ZIP or as one page
      (`--html`), the map whole; a name no diagram holds stops it before
      anything is written. The MCP server's `render` takes `fold`, a list
      of names. `play --fold` says an unknown name in the same words.
- [x] What the review of the folds on the command line found
      (2026-11-23): the image keeps the folds with the source (an
      attribute `folds` of `<sf:source>`, a PNG chunk `Omaspec folds`), and
      `render --check` and `omaspec check` draw an SVG beside its diagram
      with the folds it keeps; a folded SVG written beside its file was
      said to differ. Names are taken apart at commas but for one in
      double quotes or with `\,`, as `--events` takes them, trimmed and
      once each, by `render`, `export` and `play`: `"Sales, EU"` folds.
      With several diagrams every picture is drawn before one is written,
      so a name none holds writes nothing, lists the containers they have,
      and is in the `--json` report; a diagram that does not draw is said
      instead of a name it may have held, in `render` and `export` alike.
      A project's map with `--fold` says it is drawn whole; a diagram the
      Java PlantUML drew says it folds nothing. A container the file folds
      itself counts as held. The time is both drawings'. The MCP server
      reports the names it folded.
- [x] A scenario's notes in its play (REQ-106, 2026-11-23): a note was
      never a step and never lit, at the last stop neither. A note of its
      own (`note over A, B`, `note left of A`) is a step of the play now,
      between the messages where it stands: the veil lifts to it and its
      box is lit, with no light to run along an arrow. A note beside a
      message (`note right` after it) is lit with the message. The drawer
      keeps them in the map (`Map::notes`, a step's `also`), and
      `Map::steps` gives a play the messages and notes in order, in the
      window, in a film and on a presentation's page. Lockstep steps on a
      note without a machine moving, and still counts messages. A play
      stopped short says steps where a scenario has notes of their own.
- [x] What the review of the notes as steps and the last fold fixes
      found (2026-11-23): a note's line was not given back to the file
      under a look or the Omarchy colors, as the editor draws by default,
      so the machines beside a scenario and in a lockstep film moved on
      its step too early; it is now, with the messages'. The presenter's
      view shows the speaker's notes of the next step, a note's too, and
      says steps where there are notes. A play's steps go by the lines
      they are written on, not by height: messages side by side (`&`)
      kept their order no more where a note stood among them. A note
      beside what stands before it (`/`) reaches as far down, so the veil
      does not climb back. The default of 50 counts messages, the notes
      besides. A lockstep's page in vectors steps on notes too. Of the
      folds: several diagrams are checked for the names before anything
      is drawn to be written, each holding only what it needs, not every
      picture at once; a name none holds is said in each diagram's report,
      not a report of its own; a fold line left in a file folds nothing
      where no box is folded, nor where the Java PlantUML drew; a folded
      picture beside its diagram needs the source kept (`--no-source` is
      refused), so `--check` can draw it so again; the map's refusal comes
      before anything is drawn; `--events` names an empty event again.
- [x] A state is a halt (REQ-114, 2026-11-24): the player took the one
      way without an event out of a state the moment it was entered, as
      UML takes a completion, so `A --> B : go` and `B --> [*]` ended a
      film of `--events go` on the end, never on B, and `Idle --> [*]`
      beside `Idle --> Running : start` ended the play as it began, start
      never offered. What a state stands for takes the time it takes,
      which the diagram does not say: its way without an event is now an
      option, *go on*, as an event is, and `--steps` counts it
      (`run.rs`). Only a start, a fork, a join, a choice or a point with
      one way, and a composite state whose regions have all ended go on by
      themselves. A trace may still pass a state's way without an event
      unlogged.

## From the tester's round of 1.1.0 (2026-11-24)

- [x] Lint says what of PlantUML's standard library Omaspec lacks
      (REQ-115, 2026-11-24): `render` said that `!include <awslib/…>`
      is not there, `lint` said nothing, since it leaves a diagram that
      includes files alone. The preprocessor now declines a path of the
      library itself, without reading anything (`library_missing`), and
      lint leaves alone only what includes files: a hint that only the
      Java PlantUML draws the diagram, a failure with `--strict`. C4 and
      ArchiMate, which Omaspec draws, are taken out before.
- [x] What the review of the halts and the standard library found
      (2026-11-24). `play --events` names a way without an event `go on`,
      or `go on B` where several lead on, and takes the one way on by
      itself on the way to the next event named, as a system takes it
      without logging it, so `--events start,stop` plays through
      `Running --> Stopping` again; where several lead on, the error
      tells them apart, and a guard is never taken by itself. Ways
      without an event in a row past 64 go round, and say so; an event
      named after the machine ended is said, not left out, and without
      `--steps` the events named bound the play, not 8. Lint takes a
      quoted path of the library too. Without the diagram's folder an
      include asks for it before its words are worked out. A state's
      halt asks first whether it is composite, the cheap question. Three
      regions of three states without events make 64 places, well within
      what a play visits. The landing page's coffee machine and the
      README's play are drawn again by the halts.
- [x] A folded subgraph says what it holds (REQ-104, the rest,
      2026-11-24): a flowchart's folded box said its title only, where a
      folded state or package says what it held. It says *3 nodes · 2
      links* under its title now, in a markdown string on a line of its
      own (`mermaid/layered/fold.rs`).
- [x] A package holding nothing as PlantUML draws one (2026-11-24): its
      name and stereotypes stood in a tab hardly narrower than the box,
      so a folded package's outline stepped at its bottom right, and a
      long name's tab left a shoulder of 4 pixels. Its name is in the tab
      now, its stereotypes, or what a fold says it holds, in the body
      under it; a package's body reaches at least 16 pixels past its tab
      (`SHOULDER`), open or folded.

## Next

- [x] Lines by hand: drag a part of a line in the diagram to shape it
      yourself; the diagram keeps the shape and draws it the same way next
      time ([0016](adr/0016-lines-by-hand.md)). Built: the block of
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
      ([0017](adr/0017-projects.md)). The file holds the project's
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
      ([0018](adr/0018-diagrams-that-link.md)). Each box of a map that
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

### After the code review (2026-11-01)

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
      out the containers that open ([0013](adr/0013-containers-like-graphviz.md))
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
- [x] Large files cut along the phases they already have (2026-11-13,
      after the review of 2026-11-13): `formula.rs` 3,285 lines into
      parse, nodes, pairs, symbols, layout and paint; Mermaid's `c4.rs`
      2,881 into read, args, lay, route and paint; `block.rs` 2,440 into
      read, tokens, lay, route and paint; `architecture.rs` 1,958 and
      `er.rs` 1,946 into read, lay, route and paint; each with its tests
      beside it, every file under 1,000 lines. C4's and architecture's
      `shorten` and `pull`, the same line for line, are `geom.rs`; ER
      keeps its own, which stops short of a step's end. The theme reading
      the C4, block and architecture drawers each did is
      `mermaid::head::themed`. The ways of the four stay their own: one
      search for all would change their pictures. Every picture of the
      examples, the tests and mmdr's 134 fixtures stayed the same, byte
      for byte, also in the omarchy, dark and plain looks.

## Plan for 0.6 to 0.9 (2026-11-03)

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
      ([0018](adr/0018-diagrams-that-link.md), amended): checked in a
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
      ([0016](adr/0016-lines-by-hand.md)), which the flatter diamond lowers:
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
      ([0019](adr/0019-where-a-name-is-used.md)): a use is compared as links
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
      [0018](adr/0018-diagrams-that-link.md)): the main map leaves a diagram
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
      commit. Built so far ([0009](adr/0009-diff-and-convert.md), amended):
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
      [0020](adr/0020-room-for-labels.md), accepted): ranks doubled, a
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
      block of [0016](adr/0016-lines-by-hand.md), relative to its rank and
      neighbors rather than in coordinates, so it holds when a theme or a font
      changes a size; lines by hand on it move with it; an entry that no longer
      holds is a finding. Its own ADR. So far: the ADR
      ([0021](adr/0021-boxes-held-in-place.md), proposed, 2026-11-06): an
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
      does (2026-11-06, [0022](adr/0022-lines-bundled.md)): on by
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
- [x] Focus and collapse (2026-11-06, [0024](adr/0024-focus-and-collapse.md)):
      a container folded to its box in the pane, drawn as a linked diagram is
      ([0018](adr/0018-diagrams-that-link.md)); a focus that dims all but a box
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
- [x] A sketch look (2026-11-06, [0023](adr/0023-sketch-look.md)): PlantUML's
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
      ([0022](adr/0022-lines-bundled.md)); the shortest take the lanes next
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
- [x] The map, made ([0026](adr/0026-the-map-is-made.md), 2026-11-09):
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
      neighbor's rank only. A box put into another rank came with
      [0028](adr/0028-a-box-in-another-rank.md) (2026-11-13).
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

### An editor that knows the diagram, and more diagrams (2026-11-11)

- [x] The editor offers what could be written (2026-11-11): while a name
      is typed, after an arrow and after the colon of a label a list
      opens, and Ctrl+Space asks for it anywhere (*Suggest at the caret*).
      It holds the diagram's states or participants, the events its state
      machine takes, and for a message to a participant with a state
      machine in the project the events that machine takes, before what
      the scenario sent so far; at a line's first word also the words and
      blocks of the language for this kind of diagram, a block with its
      places to fill in by Tab (`alt … else … end`, `state Name { }`,
      `subgraph`). In a Markdown document it goes by the fence the caret
      is in. The pointer resting on a state, an event or a participant
      says what it is: where it stands, what goes in and out, the state
      machine behind a participant (`omaspec_model::assist`; the list,
      its keys and the card are scrive's, which had them and no key to
      ask). The line the caret is in is left out of what is read, so a
      line half written does not empty the list.
- [x] Highlighting by what a word is (2026-11-11): in a state machine
      the states, and in a transition's label its event, its guard and
      its action, each in a color of its own; in a scenario the
      participants, and a message in the event's color where its
      receiver's state machine in the project takes it. In both
      languages and in the fences of a Markdown document, read line by
      line, so a diagram half written keeps its colors
      (`omaspec_model::assist::marks`). The colors are the theme's
      (`tokens::tones`; `name`, `event`, `guard` and `action` under
      `[syntax]` in `omaspec.toml` replace them). scrive colors by its
      grammar alone, so it got runs of a line in a color of the host's,
      laid over the grammar's spans (`scrive_iced::Tint`). Open: a name
      the diagram does not declare apart from one it does, and the other
      kinds of diagram.
- [x] In the list too (2026-11-12): where a link is written
      (PlantUML's `[[`, the quotes of Mermaid's `click … href`) the
      project's diagrams, each as the way to it from the file being
      written, without what stands there already; and in PlantUML after
      a class and `::` that class's fields and methods by their names
      (`note left of Order::total`). Mermaid has no way to name one
      member, so nothing is offered there. To watch in use: one letter
      opens the list, which may be too often.
- [x] *Prettify the diagram* and `omaspec fmt --pretty` (2026-11-11),
      above the indenting `fmt` does: in a state machine and a scenario
      each line from one name to another gets one space around its arrow
      and its colon, the arrows and colons of lines that follow one
      another stand in a column (not where the names differ by more than
      12 letters), lines end without spaces. Mermaid is written as its
      guide writes it (`A->>B: text`, `A --> B: text`), nothing in a
      column. Only spaces change, and that is held to: a result that
      differs in anything else, or whose behavior reads differently, is
      dropped for what `fmt` gives (`omaspec_model::pretty`).
      Declarations are not moved before their first use: in a scenario
      their order is the order of the columns. Open: the other kinds of
      diagram.
- [x] A finding carries its fix (2026-11-11): `Finding::fix`, lines taken
      out and lines put in, with what it does in a few words; in `lint
      --json` and over MCP too. Ctrl+. (*Fix the finding at the caret*)
      makes the one on the caret's line, *Fix every finding that has a
      fix* and `omaspec lint --fix` make all, and the card under the
      pointer names the key. Four findings carry one so far: a line
      mermaid.js would not read, `@start…` without its `@end…`, an entry
      of the lines by hand that is left out, and a state nothing leaves
      (`X --> [*]`). Open: a name close to a declared one (*Did you mean
      …?*), which needs the place of the name in its line.
- [x] An emoji keeps its color and the label its face (2026-11-11): in
      `finished ✅` the whole label was set in a monospace face, the
      check mark a small black box, in the pane, a PNG and a PDF; `🚀`
      and `🔴` were fine. usvg takes the first face that has a
      character and sets the whole text in it when it has all of the
      text's characters, and a text face with a check mark of its own
      came before the emoji face. The face is chosen here now
      (`export::fallback`): an emoji face first, then one of symbols
      alone, which cannot take a label over, then one as wide or as
      narrow as the text's own.
- [x] A window of our own to choose an emoji from, weighed and declined
      (2026-11-11): Omarchy has one for every window on Super+Ctrl+E,
      and it writes into the source as into any text. Left open: a
      window for arrows and UML's marks, if they are missed in use.
- [x] The caret glides and the list arrives (2026-11-11): a single caret
      goes to its new place in 90 ms, fast at first, and stands at once
      where scrolling moved it; the list of what could be written comes
      up in 110 ms, faint and a little low at first. *Motion in the
      editor: on or off* (View) turns both off, and the session keeps it
      (`scrive_iced::motion`, a file of ours in `vendor/`). The line a
      jump lands on lit up already.
- [x] A fold waited 13 seconds before it moved: each line looked for
      its partner by a walk through the other picture's lines. They are
      looked up now (`wires::plan`).
- [x] The glide itself came at nine frames a second, each frame 90 ms of
      work (`OMASPEC_FRAMES=1` in the window): every box is a picture
      of its own in every frame, cut out of two pictures with a hole for
      each box. A diagram of more than 300 boxes fades from one layout
      to the next now, one picture into the other
      (`arrive::MOST_GLIDING`). A compromise: the branch no longer goes
      into its box there, and *Compare: glide from B to A* fades there
      too instead of showing what moved; the limit is a count of boxes,
      also where only a few of them are in sight, and 300 is reckoned
      from the one measurement, not measured. Open: the glide box by box made cheap enough
      for such a tree (the holes of a box found once, not in every
      frame; fewer pictures cut to shape), and the fold answering at
      once while the new layout is still being made.
- [x] Mermaid's git graph (2026-11-11): `commit`, `branch`, `checkout`
      or `switch`, `merge` and `cherry-pick` run on a small repository
      (`omaspec_uml::gitgraph`), each branch a lane in a color of its
      own, the theme's accent turned round the color wheel; time runs
      right (`LR`), down (`TB`) or up (`BT`). Ids, tags, the three
      commit types, `order:` of a branch. What git would refuse (a
      branch that is not there, a merge that brings nothing) is refused
      by name and left to mmdr. An id Mermaid would make up is not
      shown. Open: the settings of `gitGraph` in `%%{init}%%`
      (`mainBranchName`, `showBranches`, `parallelCommits`), which are
      passed over; a commit is no place of the map, so a click on one
      does not lead to its line.
- [x] Mermaid's kanban board (2026-11-11): an item at the first level
      is a column, each one written deeper under it a card
      (`omaspec_uml::kanban`). The columns stand beside one another, as
      tall as the fullest; a card's words are broken to its width, its
      ticket and its assignee stand under them, its priority is a bar
      at its left edge. Open: `ticketBaseUrl`, which would make a ticket
      a link, and settings written over several lines; both are refused
      by name and left to mmdr.
- [x] Mermaid's requirement diagram (2026-11-12): read into the tree
      of the structure engine (`omaspec_uml::structure::requirement`),
      so it is laid out and drawn as a class diagram is, with lines by
      hand and boxes held in place. A requirement and an element are
      boxes with what they are over their name and a row for each thing
      said about them, a long text broken at its spaces; a relationship
      is a dashed arrow that names its kind. `style`, `classDef`,
      `class` and `:::` paint a box. The gallery has one
      (`brakes.mmd`), and the editor offers its words. A compromise:
      `contains` is a solid line with a filled diamond at the
      requirement that holds the other, where Mermaid draws a circle
      with a cross; the engine's line ends are polygons. Open:
      `direction BT` and `RL`, refused by name and left to mmdr, as in
      class diagrams, and Markdown in a quoted text, shown as written.
- [x] Mermaid's packet, radar and treemap (2026-11-13), on drawers of
      Omaspec's own (`omaspec_uml::chart`), where mmdr drew a packet as
      a row of boxes, a radar with its syntax for labels and a treemap's
      boxes out of their shares. A packet is rows of 32 bits, each field
      a box over its bits with its first and last bit over it, cut where
      its row ends, its name set smaller where the field is narrow
      (`+16:` counts on from the last). A radar has an axis for each
      thing measured, rings for the scale (`ticks`, `graticule polygon`),
      each curve a shape let through to those under it, values by place
      or by the axis's id, a legend. A treemap lays out its boxes as
      squares as near as they come (the squarified layout), a section
      around its parts with its name over them. They share a look: the
      theme's page and ink, and a palette of middle tones that read as a
      line on any page, the diagram's own `pieN` where it sets them. The
      gallery has a TCP header, a team's skills and a budget.
- [x] Mermaid's sankey, XY and quadrant charts (2026-11-13), on the
      same drawers (`omaspec_uml::chart`). A sankey's nodes stand in
      columns by how far they are from where the flows start, each as
      tall as what passes through it, a flow a band as wide as its value
      in the color it comes from, let through to the others; flows in a
      circle are left to mmdr. An XY chart has bars side by side in their
      category and lines with a dot at each value, its y-axis with its
      range or the data's in steps of 1, 2 or 5, a faint grid;
      `horizontal` is left to mmdr. A quadrant chart is a square in four,
      each quarter in a light color with its name, the axes' ends under
      and beside it, the points with their names, their own color and
      radius where they give one. The gallery has downloads, an energy
      flow and priorities.
- [x] Mermaid's ZenUML (2026-11-13), read as the sequence it describes
      (`sequence::from_zenuml`): each statement written as the PlantUML
      line it stands for, on its own line, so the sequence engine draws
      and plays it as every other. Participants with their kind
      (`@Actor`, `@Database`, …) and color, `group`, `@Starter`, calls
      with their arguments, results and blocks (which activate the
      callee), self calls, `new`, async messages, `return` (the end of a
      block gives back its result only where no `return` did), and the
      fragments `if`/`else`, the loops, `par`, `opt`, `try`/`catch`/
      `finally` and `critical`. A call at the top with no starter comes
      from the left edge. The gallery pays for an order. Only a
      horizontal XY chart is now left to mmdr, where a drawer reads it.
- [x] PlantUML's YAML diagram (2026-11-12): `@startyaml` is read here
      (`omaspec_uml::yaml`) into the value the JSON drawer draws, so a
      mapping is a table and a sequence a column, with `#highlight`, a
      title and a `<style>` (`yamlDiagram`) as there; the Omarchy skin
      writes that style. Read: mappings and sequences by their
      indentation, `- key: value`, flow values on one line, quoted and
      plain scalars, `|` and `>` blocks, comments. The gallery has one
      (`recipe.puml`). Open, declined with their line: anchors, aliases
      and tags, a `?` key, a second document, and a flow value or a
      plain text over several lines.
- [x] The same picture in every system language (2026-11-12): a test
      draws every example, and names a locale would sort or fold its
      own way, under C, German, Turkish, Japanese and Arabic and
      compares the SVGs byte for byte (`omaspec-app/tests/locale.rs`);
      CI installs the locales and fails when one is missing. Mermaid's
      swimlane layout ranks its nodes by the browser's language
      (mermaid-js/mermaid #8759). Not held to it: what the Java
      PlantUML draws, which is given `LANG`.
- [x] JSON and YAML as PlantUML draws them (2026-11-12): what an object
      or an array holds stands in a card of its own in the next column,
      reached by a dotted arrow from a dot in its row; an array has no
      index; `true` and `false` stand behind a box ticked or not (drawn,
      not a glyph a font may lack), `null` is `␀`; a string's Creole is
      read, markup the engine does not draw is drawn as written, a tab
      is four spaces and a line break breaks (PlantUML runs the lines
      together). The cards of a column keep the order of their rows, each
      as near as it can be to the row that reaches it
      (`omaspec_uml::json::tree`). The nested drawing stays, the value in
      its cell (`json::nested`): `skinparam jsonLayout nested` in the
      file, *JSON and YAML: as a tree or nested* (View) in the editor,
      which the session keeps and the pane hands the engine as a line of
      its own (`' layout (Omaspec) nested`), as it hands its folds. An
      object or an array folds in the pane by its path
      ([0024](adr/0024-focus-and-collapse.md)), and says what it
      holds in its row.
- [x] Network diagrams (nwdiag, 2026-11-12), `@startnwdiag` as PlantUML
      now asks, or `nwdiag { … }` in an `@startuml` as its manual still
      shows (`omaspec_uml::network`): the networks are bars one under
      another with their name, address and description at the left,
      `color` and `width = full` as written; each server stands in the
      lane between the first and the last network it is on, below the
      one it is on alone, above it when a server on no network hangs
      from it by a peer line (`internet -- router`), which then stands
      above the topmost bar. Servers share a column as long as no line
      of one runs through another; the line to each network carries the
      address there. A server takes the shapes of the structure engine
      (`shape = database`, `cloud`, `node`, …, drawn by the same code)
      and a description in its place, icons and Creole too; a group is a
      colored field behind its servers, its description under them,
      clear of the addresses. PlantUML's styles reach it
      (`nwdiagDiagram { network server arrow group }`), the Omarchy pack
      of an `@startnwdiag` is such a style, and the colors packs lay over
      the other types (`NodeBackgroundColor`, `ArrowColor`) dress the
      servers and the lines. The editor names it *Network diagram* and
      offers its words; the gallery has a home network.
- [x] ArchiMate (2026-11-12), both ways it is written. PlantUML's
      `archimate #Business "Customer" as c <<business-actor>>` is a box
      in the color of its layer (#Business, #Application, #Technology,
      #Motivation, #Strategy, #Implementation, #Physical, as PlantUML
      colors them) with the icon of its type over its name, as PlantUML
      draws it; the stereotype that names the type is not shown.
      Archimate-PlantUML (`!include <archimate/Archimate>`) is read as
      C4-PlantUML is (`structure::archimate`): `Business_Actor(c,
      "Customer")` is written as that line, `Grouping` a dashed frame,
      and each `Rel_…` as the arrow nearest to ArchiMate's (serving and
      triggering with a head, access and flow dotted, realization dotted
      with a hollow triangle, specialization solid with one, aggregation
      and composition with their diamonds), `_Up`, `_Down`, `_Left`,
      `_Right` as a written direction. The icons are Omaspec's own line
      art, drawn in the words' color, and serve any text as PlantUML's
      built-in sprites do (`<$archimate/technology-node>`). On the way
      the structure engine reads a dotted arrow with a direction
      (`.up.|>`, `<.left.>`). Open: junctions, and the library's themes.
      The editor names it *ArchiMate diagram* and offers its macros; the
      gallery handles a claim.
- [x] Salt (2026-11-13), `@startsalt` or `salt` in an `@startuml`
      (`omaspec_uml::salt`): grids with each kind of line (`{`, `{+`,
      `{#`, `{!`, `{-`), buttons, text fields (their words as written,
      so `****` stays a password), check boxes, radio buttons, drop-down
      lists, words with Creole and icons, empty cells, rules (`--`, `==`,
      `..`, `~~`), a group box with its title in its top line, tabs, a
      menu bar and a tree with its lines, laid out as PlantUML lays them:
      a column as wide as its widest cell. A pack's page and ink reach
      it; the gallery has a sign-up form. Open: scroll panes (`{S`), a
      block inside a row's cell, menus that open.
- [x] EBNF and regular expressions (2026-11-13), `@startebnf` and
      `@startregex`, as railroads by one drawer (`omaspec_uml::railroad`):
      a rule a track from an open circle to a dot, its name over it; a
      name in a gray box, a word as written in a white one, a special
      sequence and a class of characters in a dashed one; ways branch
      below the first, what may be left out runs below a straight track,
      a repetition loops back over itself with an arrow, a count
      (`{2,}`) over the loop. EBNF as ISO writes it (`,` or a space, `|`,
      `[ ]`, `{ }`, `{ }-`, `( )`, `"…"`, `'…'`, `? … ?`, `(* … *)`);
      regular expressions with groups, classes, `.`, anchors, `\d \w \s
      \b` and the quantifiers, characters in a row one word. Open: an
      EBNF exception (`a - b`), look-arounds. The gallery has JSON's
      grammar and an ISO date.
- [x] Ditaa (2026-11-13), `@startditaa` (`omaspec_uml::ditaa`): the
      drawing read on its grid of characters as ditaa reads it. A box is
      the smallest rectangle `+`, `-` and `|` close (`/` and `\` round
      its corners, `:` and `=` dash a side), a box split by a line two
      boxes; every other run of those characters a line, bending at a
      `+`, dashed where it has a `:` or `=`, with a head at `<`, `>`,
      `^` or `v` that reaches the side of a box right behind it. Codes
      inside a box give it a color (`cBLU`, `cGRE`, `cRED`, `cYEL`,
      `cPNK`, `cBLK`, `cWHT`, `c1AB`) and a shape (`{d}` a document,
      `{s}` storage, `{io}` input and output, `{c}` a choice, `{o}` an
      ellipse, `{tr}` a trapezoid) and are not shown; the words stay
      where they were typed, in black or white where the fill needs it.
      The boxes cast the shadows PlantUML's do; `--no-shadows` on the
      start line takes them away. A pack's page and ink reach it. Open:
      the other codes (`{mo}` is read as a box), bullets (`*` on a
      line), `--round-corners` and `--scale`. The Graphviz and tree
      diagrams stay with the Java PlantUML; the gallery has a release
      drawn in characters.
- [x] Chronology, left out (2026-11-13): PlantUML 1.2026 does not draw
      it either ("Diagram not supported by this release of PlantUML"), so
      no package helps, and a Gantt chart or a timeline says the same.
      Omaspec says so instead of offering the Java PlantUML.

### Architecture diagrams, elegant and playful (2026-11-12)

- [x] What was missing for it (2026-11-12): Creole lists in any label
      (`* item`, `** deeper`, `# numbered`), each item at the left edge
      of its box under a centered title; an emoji measured as wide as it
      is drawn, so it serves as a card's icon without the title standing
      on it; and the words on a box with a fill of the writer's own moved
      towards black or white until they read, so pastel cards stay
      legible in a dark look. PlantUML's inline style on a box
      (`#fill;line:color;line.dashed;line.dotted;line.bold;text:color`)
      gives a card a border of its own, as the dashed card of what a
      system can do. The gallery has one (`agent-platform.puml`),
      checked in every bundled theme.
- [x] Placed and aligned as the writer says (2026-11-12): a hidden line
      (`a -[hidden]-> b`) ranks its two boxes and is not drawn, not in
      the map and not a line to shape by hand; with a direction it says
      where the second box goes: along the flow it ranks them
      (`-[hidden]down-`), across it the box is held in the rank of the
      first, on that side (`-[hidden]right-`), as an entry of
      [0021](adr/0021-boxes-held-in-place.md) holds it. A box no
      line joins to anything stays beside its neighbor in the rank,
      where it drifted off as the others were pulled to their lines
      (`graph::fit_row`). `skinparam defaultTextAlignment left` starts a
      box's title at its left edge, over its list and beside its icon.
- [x] A line within a rank (2026-11-12,
      [0027](adr/0027-a-line-within-a-rank.md)): `a -right-> b`
      puts `b` beside `a` in its rank and runs straight between the two,
      its label over its middle; `-left->` puts it before; the words turn
      with the flow as PlantUML turns them, so where it runs to the right
      `-right->` puts `b` below `a` and `-down->` runs along the flow
      (corrected 2026-11-16, read as the page showed them before). Along
      the flow `-up->` runs from its second box to its first. The layout
      holds the two as an entry of 0021 does, keeps room for the label
      and the heads, and routes the line itself (`graph::level_route`);
      where the two cannot share a rank the line is one like any other.
      Holds are placed anchor first, so a chain stands however it was
      written. The gallery's architecture puts its gateway beside the
      user, its router beside the orchestrator. State diagrams too
      since 2026-11-16 (REQ-075): a transition turned against the flow is
      ranked from its second state to its first and drawn as written
      (`graph::open::Line::against`).
- [x] C4-PlantUML (2026-11-12), read into the structure engine
      (`omaspec_uml::structure::c4`), so it is laid out by its ranks as
      PlantUML lays it out. The library is not read: the include
      (`<C4/C4_Container>`, its address, or a copy named `C4_*.puml`) is
      taken out, and each macro is written as the PlantUML it stands
      for, on its own line: a person a `person`, systems, containers
      and components boxes in the colors of C4 (a container's blue a
      shade darker, so white reads on it; black on the light ones; the
      borders middle tones that read on any page), `…Db` a `database`,
      `…Queue` a `queue`, boundaries dashed frames, deployment nodes
      plain ones. `Rel_D` runs along the flow, `Rel_R` within a rank
      ([0027](adr/0027-a-line-within-a-rank.md)), `Rel_Back` from
      its second element to its first, `BiRel` both ways, `Lay_R` is a
      hidden line; a direction across the flow between elements of two
      boundaries is dropped, since no rank holds them both.
      `LAYOUT_LEFT_RIGHT()` turns the flow, `LAYOUT_AS_SKETCH()` draws by
      hand, `LAYOUT_WITH_LEGEND()` adds a legend: a dotted frame with a
      box of each kind drawn. Lines that are no macro are read as
      written. On the way PlantUML's `person` became its own shape, a
      round box under a round head (it was an actor), and `queue` is
      drawn. The gallery has one (`bike-rental.puml`), the editor names
      it *C4 diagram* and offers its macros. Open, declined by name:
      dynamic and sequence diagrams of the library, tags and the styles set by tag or by element
      (`AddElementTag`, `UpdateElementStyle`), a link on a
      relationship; the legend stands where the layout puts a frame
      without lines, not at the bottom right.
- [x] Icons (2026-11-12): PlantUML's `<&icon>` and sprites, in any
      text, drawn as the words are, in their color, so a card's icon
      reads in any theme where an emoji keeps its own colors
      (`omaspec_uml::icons`). Open Iconic's 223 icons are in the engine
      (MIT, its notice beside them), each a path drawn after the text,
      as tall as a capital and standing on the baseline; `<&cloud*2>`,
      `{scale=2}` and a color (`<#red&lock-locked>`, `,color=red`) as
      PlantUML reads them, and a tall icon lifts its line. A name Open
      Iconic lacks (`<&camera>`) is drawn as nothing, as PlantUML draws
      it, and lint names it with the icons that come close
      (`<&camera-slr>`; REQ-066, 2026-11-15). A sprite the
      diagram defines (`sprite $bolt [16x16/16] { … }`) is read in each
      form `plantuml -encodesprite` writes: 16 levels a hex digit a
      pixel, 8 levels two rows to a letter, 4 levels three, and each
      compressed (`z`: raw deflate of a byte a pixel in PlantUML's 64
      letters, a stream cut short leaving the page). Its definition is
      taken out after the preprocessor and each use carries its pixels
      (`<$:16x16:…>`), so a label is measured and drawn without looking
      anything up; a pixel is as dark as its level, `0` the page. In
      C4-PlantUML an element's or a boundary's `$sprite` stands above its
      name, a relationship's before its label: `&cog` an icon at twice
      its size, a name the diagram's sprite, `person` C4's own as an
      icon. Open: a sprite as a stereotype (`<<$bolt>>`), SVG and color
      sprites, the icon libraries of the standard library (no network),
      C4's robot. ArchiMate's icons came with ArchiMate.
- [x] The rest of the picture (2026-11-12): a title at the left with a
      subtitle under it, as a style says it (`<style> title {
      HorizontalAlignment left }`, header, footer and caption too): the
      frame takes the alignment, so no diagram's skin needs to know it,
      and a title's second line in a smaller `<size>` is the subtitle.
      Arrows in the words (`→ ← ↑ ↓ ↔ ⇒`), which Excalifont lacks, are
      drawn in the hand of the lines where the handwriting face writes
      them, as wide as the face would set them, so the state-event
      table and a label say `→` in one hand. Layers the writer names:
      `together { … }` keeps its boxes in one rank in the order written,
      each held after the one before it as a hidden line across the flow
      holds it ([0021](adr/0021-boxes-held-in-place.md)), whichever
      way the flow runs; a diagram that starts with one is a structure
      diagram.
- [x] A box put into another rank than its neighbor's (2026-11-13,
      [0028](adr/0028-a-box-in-another-rank.md)): the block reads
      `D below C`, `above`, `right of`, `left of` and `level with` by the
      page, as the flow runs. Another rank is an invisible line from the
      one to the other, as `dot`'s, which ranks and orders the two and is
      not drawn; `level with` ranks the two as one and leaves the order
      to the sweeps; `right of` and `left of` (`below` and `above` in a
      flow to the right) are `after` and `before`. An entry the lines
      rank the other way is said. All five drawers of graph diagrams;
      the drag still writes `after` or `before`.
- [x] An outline beside the source to jump from, and an example from
      the gallery put in at the caret (2026-11-13). *Outline beside the
      source* (View menu, kept in the session) shows the outline the
      editor keeps anyway (participants, states, fragments, classes,
      each with its kind) in a column left of the source, the entry the
      caret is in lit; a click puts the caret on its line. The column
      stays in the widget tree when hidden, so the editors keep their
      state. Ctrl+Enter in the gallery puts the chosen example in where
      the caret is in a Markdown document, as a fenced block of its
      language on lines of its own (not inside a fenced block of any
      language); from a
      diagram's file or an empty tab it opens as a copy.

### Views derived from the behavior (2026-11-12)

- [x] Coverage (REQ-034, 2026-11-12): `omaspec coverage door.puml
      --traces night.jsonl` follows the traces as `trace` follows them,
      object by object, through the machine their objects go by, and
      counts how often each transition was taken and each state entered
      (`omaspec_model::coverage`). Where a trace leaves open which way
      the object went, every way it could have gone counts, as the
      player lights them all. It prints the share taken and each
      transition never taken as `file:line`; `--json` gives the hits of
      each transition (line, from, to, trigger) and state with
      `covered_transitions` and `total_transitions`; `--min 80` fails
      below that share, for CI; `-o door-cov.svg` (or .png, .pdf) draws
      the machine with what was taken bold in green, how often after its
      label (`open ×12`), and what never was dashed in an orange red,
      both colors read on any page (`omaspec_uml::state::Marks`, laid
      over the tree before it is drawn, for PlantUML and Mermaid alike;
      the source stays as it is). The MCP server has it as `coverage`.
- [x] Coverage in the editor (2026-11-12): *Coverage of the recorded
      traces* (Diagram menu) reads the traces in the project folder that
      have an object of the machine on screen, in the background, and
      draws what they take over it as `coverage -o` does, while it is
      edited and while a trace plays; the status bar says the share and
      how many traces. The pane hands the marks to the engine as lines
      after the diagram, `' mark (Omaspec) line 4 taken 12`, as it hands
      a fold (`Marks::lines`, `Marks::read`), worked out by the render
      worker on the text drawn, Omarchy colors and all, so the lines
      match whatever the pack adds; the file never holds them. The marks
      of coverage are made once, in the model (`Coverage::marks`), for
      the command line and the editor.
- [x] The diff as a picture (REQ-035, 2026-11-12): `omaspec diff old new
      -o diff.svg` (or with `--rev`, or .png, .pdf) draws the new state
      machine with what came bold in green and what changed bold in
      amber, by the marks above. A transition that went is drawn dashed
      in red between its two states where both are still there, its
      label struck through (`Marks::ghosts`, laid into the tree before
      the layout); a state that went, and a transition from it, is said
      under the picture with the counts (`3 came · 1 changed · 1 went`).
      `--json` gives the changes and the picture; the MCP tool `diff`
      takes `output` too. No `--view`: `-o` says it, as for coverage. A
      sequence diagram's changes are listed, not drawn.
- [x] The state-event matrix (REQ-032, 2026-11-12): `omaspec matrix
      door.puml` writes a state machine as a Markdown table, every state
      a row, every event a column (`omaspec_model::matrix`); a cell says
      what the event fires there (`→ Open [allowed] / beep`, several
      apart by `;`), what it fires from the state around where the state
      has nothing of its own (`↑ → Off (from On)`), `⚠` where two ways
      have no guard to choose, or nothing. An empty cell is no finding.
      `-o door.md` writes the table, `--json` gives the states, the
      events and each cell with its kind and its transitions; the MCP
      server has it as `matrix`. Transitions without an event have no
      column.
- [x] The state-event matrix as a picture and in the editor
      (2026-11-12): `omaspec matrix door.puml -o door.svg` (or .png, .pdf;
      the MCP tool `matrix` takes `output`) draws the table in the look
      of the machine: the heads filled as a state is, the words in its
      ink, the grid in its border, under its title, so a skin pack and
      the Omarchy colors reach it too (`omaspec_uml::table`, drawn by
      `state::draw_table` for PlantUML and Mermaid alike). A cell holds a
      line for each way; an inherited one is in italics with `from On`
      under it, a conflict framed in the orange red of coverage with
      `⚠`; a row's head steps in for each state around it. In the
      editor, *State machines: as a diagram or a table* (View menu, kept
      in the session) shows every state machine as its table: the pane
      hands the engine `' view (Omaspec) table` after the diagram, as it
      hands a fold, and the file never holds it. The map says where each
      line of a cell is and which transition it says (`map::Cell`), so
      Ctrl+click on it puts the caret on that line, in the Omarchy colors
      too. A play asks for the diagram. In place of the diagram rather
      than beside it: the pane shows one picture, and the toggle is one
      key away.
- [x] A trace as a timeline (REQ-033, 2026-11-12): `omaspec trace
      run.jsonl --as timing` writes the states each object was in over
      time as a PlantUML timing diagram, which Omaspec draws itself and
      the writer can edit, rather than a drawer of its own: a concise
      row for each object the project's machines follow, the state it
      begins in before the first line, each line where the trace
      deviates as a red band with what it says (`line 3: Door lock?`).
      The time runs as the trace gives it: seconds from the first line
      where every line has a clock time (past midnight too), the numbers
      where every line has one, else the lines in order; the caption
      says which. Where the trace leaves the state open, the row says
      each (`Open or Locked`), where it loses the object `?`, a final
      state `[*]`. `-o night.svg` (or .png, .pdf, `-` for SVG on
      standard output) draws it, `-o night.puml` writes the source. The
      timing drawer now leaves out a state's name wider than its box and
      a time that would touch the one before it (a night and its
      morning's quick steps), and gives a highlight's caption room of
      its own above the rows.

### Dense diagrams as you type (2026-11-13)

- [x] Laid out in under half a second (2026-11-13), the same picture as
      before for every diagram of the tests. Swapping two neighbors of a
      rank counts only the crossings between their own lines, the only
      ones a swap changes, instead of every crossing of the two ranks
      around them (1.9 s a sweep to 3 ms); laying a long line again
      reuses one table of places and stops at a budget past which a
      dense diagram keeps the lines as the sweeps laid them; taking the
      small steps out of a line looks only at the lines near it and
      counts crossings only on the steps that change. `omaspec-render/
      tests/dense.rs` draws 50, 100 and 150 boxes and holds the last
      under a second; CI runs it in a release build. 100 boxes: 7.3 s
      to 0.19 s; 150: 53 s to 0.44 s.
- [x] Half as tall (2026-11-15). The flowchart of the issue itself, 150
      named boxes and 476 lines, 330 of them with words, was 71,715 units
      tall: its 9,175 crossings make its lines step aside 6,572 times
      across the gaps between ranks, and each step took a track of its own,
      10 units, nine tenths of the height. Lines whose runs across a gap
      lie apart now share a track; every two that overlap keep their order,
      so they cross as they did. It is 36,655 units tall, and drawn in
      1.2 s; thirty of the golden diagrams are shorter along their flow,
      by up to a quarter. `tests/dense.rs` draws the issue's own flowchart
      in a release build and holds its time, its lines, its words and its
      height. One line of the delivery drone, routed around a container
      afterwards, now crosses an arrow where it ran through an end mark.

### A glide that knows its parts (2026-11-14)

- [x] The engine marks the parts it draws, for the pane only: each box, mark
      and line's words in a group of its own; state diagrams of both
      languages first (2026-11-14). `' parts (Omaspec)` after the diagram
      (`%%` in Mermaid) asks for them; the canvas puts what a part draws on
      each layer into `<g id="part-…">`, its name in hex, a link inside it;
      the map's `Spot::part` and `Wire::words` name the same parts;
      `without_parts` leaves the ground. Without the line nothing changes,
      the golden SVGs neither.
- [x] The pane draws each part as a picture of its own and glides it; a
      line's words travel with the line. The ground is the picture without
      lines and parts (2026-11-14). The pane asks a state machine for its
      parts; the glide's two pictures become sheets (`raster::sheet`): the
      ground on top, each part below it, drawn alone from the picture
      (`resvg::render_node`), with where each lies; one picture to draw in
      the background and to upload, as before. A box drawn from its part
      is cut out of nothing and fills nothing; its rim still keeps its
      corners when it changes shape. An export of the pane's picture drops
      the groups (`unmarked`).
- [x] Structure, flowcharts and trees mark their parts too; then the
      cutting goes, with its box test, columns, swatches and the faded
      inside of a folding container (2026-11-14). Structures do: classes,
      packages, components, deployment, use cases, C4 and Mermaid's classes,
      a note inside a package with it, a line's words and each of its
      multiplicities a part of their own, the multiplicities going with
      their ends. Mermaid's flowcharts, mind maps of both languages, WBS
      and JSON and YAML cards do too (2026-11-14): a JSON card's arrows
      are lines of the map now and glide with their cards; the title, the
      header, the caption, the footer, the legend and the top JSON card are
      parts in both pictures that no box takes, and glide from where they
      were to where they are instead of fading with the ground; a card
      whose rows are laid from its left (a JSON card, a tree's box) is
      drawn at its size from its top left, so its rows stay put as it
      grows. Each kind of part name has its own prefix (`state:`, `box:`,
      `mark:`, `words:`, `frame:`), so a box called `frame:title` is never
      the title. Every drawer whose boxes glide marks them now, and the
      cutting is gone: a glide runs between two sheets of parts, a picture
      without them (the Java engine's, one too large for a sheet) fades;
      a box's rim and fill are read from its own part's outline alone.
- [x] Fade and glide as one small state machine instead of a dozen flags,
      written as a diagram of its own and checked by Omaspec (2026-11-15,
      [0030](adr/0030-arrival-as-a-state-machine.md)).
      `paper::arrival::Arrival` (still, coming, sheets, fading, gliding with
      a newer diagram alone, following or waiting) replaces `fresh`,
      `landed`, `no_bare`, `previous` and `fade`; `paper/arrival.puml` is
      the machine. In the tests each move is held against it by Omaspec's
      trace check as it is made, a tour walks all 31 transitions and
      Omaspec's coverage of it is the whole diagram. A diagram drawn while
      a short fade runs ends that fade at once; a glide runs to its end.
- [x] One way for the window and a recording to take their pictures: the
      recording runs the window's tasks, at once (2026-11-15). `at_once`
      runs the tasks an update gives to their end, their messages back
      into the update and the pictures they allocate onto the screen's
      GPU, as iced's runtime does in the background; `omaspec play` and
      the tests take their pictures that way. Gone: `deliver_overview`,
      `deliver_bare`, `finish_rasters`, `finish_small`, `Layer::finish`
      and the recording's upload of every picture before each frame. The
      colors a glide fills a box's room with are patches on its sheet
      now, on the GPU with it; the window drew them a frame late. Two
      faults of the window came out, as the tests now go its way: a
      recording's first picture skipped its fade (the arrival stayed in
      `Coming`), and the caret's fold said a container held nothing to
      fold while the last fold glided (its button hid, and the command
      asked the button). Frames drawn on the wall clock (`frames_`,
      `snapshot_dive`) take what they published on the side
      (`at_once::Background`), as the window does, rather than stalling;
      on a recording's clock it runs at once, without the debounce of a
      zoom's raster. The review of it found a fault of the window too: a
      pan or a landing zoom that showed a picture held while the view
      glided dropped what it started, so a glide waited for its sheets
      until the next edit; `settle` and `land` hand that on now. The
      diagram itself is still drawn by the recording, not by the render
      thread.
- [x] The files of `paper/` under 1,000 lines each; the glide's promises as
      tests: the first frame is the old picture, the last the new one,
      nothing lands outside a box, each pixel is drawn once (2026-11-15).
      `canvas`, `play`, `morph` and the pane's tests are folders of
      modules now (the paper's grid, the marks over it, the pointer; a
      machine, a scenario, a play's drawing; a glide's planning and its
      frames; the glides' tests), and the sheet of parts has its own
      `sheet.rs`; the largest file is `paper/mod.rs` at 977 lines.
      `tests/promises.rs` folds four of the gallery's diagrams (states,
      packages, a subgraph, JSON cards) and draws the glide's first and
      last frame by tiny-skia against the pictures as the pane shows them,
      and holds every box's pieces at every tenth of the way to its place
      and to one drawing per pixel. The first of them found the glide
      mapping the diagram onto its paper by the diagram's size, where the
      pane maps the picture by the whole pixels its overview covers: half
      a pixel down the picture, a card's lower rows were blurred for the
      whole glide. A glide maps by the ground's own size now, and a card
      laid from its top left keeps its scale each way.

### Presenting, animating, and diagrams only Omaspec draws (2026-11-13)

- [x] Save As and Open in the desktop's own dialog, as the export asks
      since 0.9.6: the folder and the name in xdg-desktop-portal's file
      chooser, the bottom line where none answers. A new project's
      folder too. Built (2026-11-15, `editor::dialogs`, `file_dialog`):
      Save As offers the tab's own name and kind of file; Ctrl+O opens a
      diagram, a document or a picture that carries its diagram, and
      *Open folder* (Ctrl+Shift+O) a folder for the sidebar, since the
      portal asks for one or the other; *New project* asks for its folder
      beside the project the sidebar is in.
- [x] A presentation as one HTML file: *Export as › Presentation…*
      writes a page that needs nothing but a browser, offline, every
      picture and script inside it, and no request to anywhere. A
      scenario steps message by message with the arrow keys, a click or
      a presenter's remote (which sends the same keys), the message
      played lit as the pane lights it, the ones before it quiet, the
      ones after it veiled; a state machine plays by its options; F
      shows it full screen. The page draws the diagram as the pane
      does, in the look chosen, and keeps its source. The foundation of
      the HTML items below. Built (2026-11-15,
      [0031](adr/0031-a-presentation-as-one-page.md)): the page is
      the export's picture, the play as data and a script of some 250
      lines that only walks it, under a Content-Security-Policy that
      allows no request. A machine's places are worked out in Rust
      (`omaspec_model::play::Graph`, which the play bar reads too), so
      the page has no rules of its own; `omaspec render FILE -o x.html`
      writes one too. A test runs the script's stepping in Node where
      it is installed; headless Chromium showed the light and the veil.
- [x] A picture that moves in full color (2026-11-17): `omaspec play
      FILE -o fold.webp` writes an animated WebP and `-o fold.png` an
      animated PNG, by the output's ending, where `.gif` has 256 colors
      and bands in a glow or a fade; at 60 frames a second unless
      `--fps` says otherwise (a GIF holds 50). Both without loss, and as
      the GIF is written: of each frame the box around what changed, the
      rest left clear and laid over what is shown (`editor/films.rs`; the
      WebP's frames by `image-webp`, the container around them our own;
      libwebp and ffmpeg read them back pixel for pixel). The coffee
      machine's Brewing folding and unfolding, 1200 wide: 4.3 MB as
      WebP, 5.1 MB as PNG, 2.1 MB as GIF at 50. The glide was eased in
      and out already, as the pane's is.
- [x] *Export as › Animation…* (2026-11-17), in File and in the
      diagram's menu: a container folding and unfolding, written as
      `omaspec play --fold` writes it. The desktop's save dialog asks for
      the file and beside its name for the container (*Fold*); the
      ending says what is written, `.webp` where it names none. Without
      a dialog the bottom line asks, Tab or a click taking the next
      container. Drawn on a thread of its own from the tab's text, saved
      or not, in the theme's colors or the file's as the pane has them
      (`editor/animate.rs`; `Moving::text`, `file_colors`); the program's
      end waits for one that is being drawn. A diagram without a
      container, or one that plays, says so.
- [x] A fold as a page, in vectors (2026-11-17): `omaspec play FILE -o
      fold.html --fold Brewing,Ready`, and the menu's animation with a
      name ending in `.html`, write one page as a presentation is
      ([0031](adr/0031-a-presentation-as-one-page.md)), sharp at any size
      and some 120 KB where the WebP has 4 MB. It holds the picture
      before the first fold and the one after each, their parts marked
      ([0029](adr/0029-a-glide-by-parts.md)), and the glide from each to
      the next as the pane works it out (`paper::headless::fold_glide`:
      `morph::plan_with`, `gather`, `wires::plan`), as data
      (`omaspec_render::export::folding`). The script only walks it
      (`Present.frame`, tested in Node): the sheet eases from one size to
      the other, each box's own groups travel, the old picture under the
      new, what goes fades before what comes, each line is drawn between
      its two routes with its marks, its words with it; a part no box
      takes that is in both pictures, as the title, travels too. Words
      keep their size where a box changes its own. It plays by itself,
      back and forth; → or a click steps, ← unfolds, Home plays again,
      `#1` opens it folded and still. Drawn without a GPU. The first
      and the last frame of a glide lie on the standing pictures (0.02 %
      and 0.07 % of the pixels differ, at edges). A presentation's
      picture in a look no longer has a white hairline at its edges: no
      ground is laid under a picture that paints its own.

### Long and large diagrams, readable (2026-11-16)

- [x] Headings kept in sight (REQ-073, 2026-11-16): a sequence
      diagram's participants, a swimlane's name and a Gantt chart's
      dates stay at the pane's top as it scrolls past them, a Gantt
      chart's tasks at its left, and go once what they head has gone by
      (the lifelines, the rows; not a footer). The drawers name the
      bands and what they head (`Map::bands`); the pane draws them again
      from the picture on screen, sharp from pixels of their own where
      the overview is not (`paper/canvas/bands.rs`, `Plan::band`), so
      every export is as drawn. Like a sheet's frozen rows they lie over
      what marks the picture, a play's light and veil too, and the
      camera keeps a step out from under them; a click on one picks the
      heading. A heading over a third of the pane scrolls away. A switch
      in View, kept by the session, Compare's pane too.
- [x] Words of a least size in a PNG or JPEG (REQ-070, 2026-11-16):
      `render --min-font PX` shrinks a picture too large for one file as
      `--fit` does, but not below where its smallest words are `PX`
      pixels high, and cuts it into tiles where it then does not fit,
      each within 100 million pixels and 16,384 a side, each cut between
      two lines of words where there is room, the source in each
      (`export/tiles.rs`), at most 100 of them, in a folder of their own
      beside the picture (`odyssey-tiles/1.png`), where a drawing takes
      away the tiles an earlier one left. The tester's Odyssey of 3,000 lines: four
      tiles of 7,460 × 11,925 at words of 10 pixels, in 8 seconds,
      where `--fit` gives one of 3,939 × 25,186 at 5.
- [x] SVG words that stay in their box without `textLength` (REQ-068,
      2026-11-16). cairosvg and rsvg-convert (pandoc's way to a PDF) do
      not stretch words to their `textLength`; where the viewer's
      sans-serif is wider than the Liberation Sans the words were
      measured in (DejaVu Sans, some 13 %), they ran out of their boxes.
      Words in the generic faces name the faces they were measured in
      first (`faces`: Arial, Liberation Sans, Arimo, Helvetica, Nimbus
      Sans, then sans-serif; Times New Roman and the like for serif,
      Courier New and the like for monospace; a list such as `Inter,
      sans-serif` has them behind its own; Omaspec's exports look for
      the same faces in the same order), so they are
      as wide as measured wherever one is installed: Windows, macOS,
      nearly every Linux desktop. For a bare system without them,
      `render --text-as-paths` writes the words as outlines (usvg, two
      places, sanitized, the source still inside): inside their boxes in
      any viewer, but no longer selectable, links no longer links, and
      the file some 25 times larger. `--look auto` keeps both color
      schemes as outlines, and `render --check` draws an SVG that has
      outlines with outlines. Boxes wide enough for DejaVu were
      the other way: every diagram some 10 % roomier, so not taken.

### A window that looks of one piece (2026-11-16)

- [x] A nested state's title in the middle of its band (2026-11-17): it
      sat on the line under it, 12 units above and none below; the band
      has 8 above and 11 below.
- [x] A label between two lines keeps off the one that is not its own
      (2026-11-17): lines side by side into one box keep the label's
      room and 8 units between them, a box too narrow for them grows,
      and a run straightened afterwards keeps 6 units off other lines'
      labels (`graph::together`, `room_for_labels`).
- [x] The comparison's switches (2026-11-17): pills, the one that is on
      framed in the accent; B shows one of `diagram`, `lines`,
      `behavior`; two diagrams wider than high lie one above the other.
- [x] Menus that name what applies (2026-11-17): `End the focus` while
      a box is in focus, `Fold this container` while nothing is folded
      (`App::menu_words`); the field that replaces carries an arrow.
- [x] The areas of the window are round (2026-11-17): the source, the
      diagram and the sidebar have corners of 8 units, and so has the
      frame around the one that has the keys, which was a thin square
      in the accent among pills.
- [x] A play's next steps stay readable (2026-11-17): the words of the
      transitions a state machine offers come through the veil while it
      rests, as the states it is in do, in the pane and in a
      presentation's page (`Placed::words`).
- [x] A folder's name too long for the sidebar ends in an ellipsis
      (2026-11-17). The hint `Ctrl+P finds diagrams` already goes once
      the picker has been used.
- [x] A block left open draws (2026-11-17): PlantUML ends an `alt`, a
      `box`, an `if`, a `while`, a `repeat`, a `fork`, a `split` or a
      partition that has no closing line with the diagram, the
      innermost first, and so do the engine's readers now; lint warns
      of each (`unclosed-block`), an activity diagram's from the
      reader's own list. The engine had declined such a diagram and
      advised the Java PlantUML for a typing mistake. A closing line
      without its block (`end`, `else`, `endif`, `end fork`, `}`) is a
      mistake PlantUML refuses too, and is said as one, without that
      advice.

- [x] A menu marks what is on (2026-11-17): the skin the diagram has,
      its language and each switch of the View menu carry a check in
      the accent (`App::ticked`), so `Sidebar`, `Menu bar` and `Toolbar`
      lost their `on/off`, and `Find in the project and all tabs…` is
      `Find in the project…`. The snapshot `menu-view`.
- [x] The editor's scroll bars are slim pills (2026-11-17), 6 units in
      their lanes of 12, as the window's other scrollers; they were
      thick gray bars. A change to the widget under `vendor/`.
- [x] Looked at and left (2026-11-17): a diagram in its file's colors
      is a white card in a dark window only where *Diagram in Omarchy's
      colors* is turned off or the file has a skin, which is on by
      default and the writer's choice.
- [x] A face of its own for the diagrams (2026-11-17,
      [0032](adr/0032-a-face-of-its-own.md)): a diagram that names no
      font is set in Inter 4.1, which ships with the program in four cuts
      (regular, bold, italic, bold italic; subsets, 0.6 MB in all), and
      is measured on Inter's widths (`Family::Inter`, `metrics.rs`). The
      cuts are loaded before the system's fonts, so the pane, a PNG and a
      PDF are the same on every machine. An exported SVG and a
      presentation carry the cuts their words use as `@font-face`s (21 KB
      each), and name the sans-serif's faces behind Inter, which are
      narrower. Without Inter's contextual alternates: `->` stays as
      written. `sans-serif` and any named font are as before; so are the
      pictures mmdr and the Java PlantUML draw, which measure in the
      fonts installed. The plain look no longer names `sans-serif`.

### The three diagrams first (2026-11-17)

The sequence diagram, the state machine and the activity diagram are what
Omaspec is seen by. Until they are elegant and without a flaw in any
theme, the roadmap waits (what is open is at the top of ROADMAP.md).

- [x] The focus on a box no longer cuts rectangles out of its veil
      (2026-11-17). The veil has the color of the picture's own paper, so
      the rest pales into it on a white card as on a dark one, where it
      was the window's dark over both. A box comes through in the shape
      its picture draws (`focus::Outline`: the largest shape its part
      fills, docs/adr/0029), a diamond as a diamond, a box drawn by hand
      with its bowed sides; its rim is drawn again over the veil. A line
      is drawn over the veil along the picture's own curve
      (`focus::route`), with its marks, where a row of rectangles let
      the picture through; its words stand in a hole that the paper's
      color hides. The light on the box in focus lies around its shape
      and not on it (rings between the shape and the shape grown), so
      the box keeps its own fill; strokes along the outline filled a
      small box, or one drawn by hand, in a dull tan. A box comes
      through a little larger than its outline, with its rim on it. A
      picture that marks no parts keeps the shapes the map knows.
- [x] A play's veil has the color of the picture's own paper
      (2026-11-17), as the focus's: what is still to come pales into the
      paper, white on a white card. It was the window's dark over a
      white card too, where a box with a fill of its own (which dims
      toward that fill) stood out as a bright stripe. The paper's
      color is the one most pixels along the picture's four sides have
      (`raster::paper_of`), where it was the first corner's alone, which
      a skin's picture had a shade off. The veil's soft edge is 28 steps
      of one color each on whole pixels: as a gradient it showed a line
      at each of its stops in the tester's window.
- [x] What scrolls under the participants held at the top fades out
      (2026-11-17): the paper goes on for 30 pixels below the band and
      thins out, in a box's own fill where the band lies over one
      (`map.grounds`), so a message dissolves before it goes under the
      band. It was cut off at the band's edge, with a gray shade of 6
      pixels under it. The same beside a band held at the left. In
      steps of one color on whole pixels of the screen, for the same
      reason as the veil's edge.

- [x] The raster shown before stays under a new one of the same
      picture for 400 ms (2026-11-17, `raster::Layer::under`), so sharper
      pixels coming in after a zoom never leave the paper alone for a
      frame. Made for a flicker that turned out to be another: see the
      top of ROADMAP.md.
- [x] The focus while a picture glides or fades in (2026-11-17): the
      veil alone is drawn. The boxes and lines in focus stood at their
      new places at once, beside the ones still on their way, so a fold
      showed them twice.
- [x] An activity diagram marks its parts (2026-11-18,
      [0029](adr/0029-a-glide-by-parts.md),
      `omaspec-uml/src/activity/parts.rs`). Each action, test, start,
      stop, end and bar is a place of the map and is drawn in a part of
      its own, named by its kind and its words (`action:Build and test`,
      the second of a name counted); notes, partitions and the words
      beside a line are parts without a place, a swimlane's name and each
      line between two as well. Its lines are drawn in the lines' group
      and told to the map from the shape they leave to the one they
      enter, followed through the points where lines meet. So the focus
      takes an action, lets it, the shapes its lines lead to and those
      lines through the veil in their own shapes, a test as its diamond,
      and after an edit the boxes glide to their new places, the lines
      with them, the swimlanes' names and borders standing. A picture
      for a file is the same to the pixel. The words beside a line
      (`yes`, `no`, an arrow's label) are that line's words, the nearest
      within 14 units: lit with it in the focus, travelling with it in a
      glide.
- [x] REQ-037, the rest (2026-11-21): a git graph builds itself up and plays. It marks
      its parts as states and classes do: each lane a part at the line that makes its
      branch, each commit a part at its own line, its mark, its words and the line it came
      by in it. A part says the line that writes it alone (`Canvas::part_on`,
      `Map::written`), as a commit's line runs past the words of others; a look's lines
      and a fence's are taken out of it as of the rest of the map. So `play --build` sets
      each commit down as the caret reaches its line, where the graph stood whole from
      the first frame. Each commit is also a step a play takes (`Map::messages`), with
      the way its time runs (`Map::run`: to the right, down or up), so F5 and `omaspec
      play` unfold it commit by commit: the veil goes back that way, a light runs along
      the line each commit came by (what a merge took in, what a cherry-pick copied), the
      bar counts them, a camera follows where the graph is too large to read whole. A
      presentation lifts its veil down the page only: a git graph whose time runs down
      plays there, one across or up shows as its picture. A commit's part is named by its
      id, else by its branch and the commits before it there, so a glide after an edit
      takes each to where it now stands; the veil keeps clear of the title, and a
      following camera takes in the whole way of a merge's light.
      The git graph's tests are a file of their own.
- [x] A chain with a label (2026-11-21, found on the way): in a Mermaid flowchart
      `a -->|yes| b --> c --> d` (or `a -- yes --> b --> c`) drew `a` and `b` only, and
      `c` and `d` not at all, without a word: mmdr splits a chain into its links only
      where none is labelled, and otherwise took all after the label for the name of
      `b`. The copy mmdr reads (`layered::readable`) now writes such a chain as a
      statement for each link, `a -->|yes| b; b --> c; c --> d`, in the line's own
      words and on its own line, so each link is drawn, its label found where its words
      stand, and `linkStyle` counts the links as written. Every kind of link is taken,
      with its label in bars or in words (`-- no -->`, `-. maybe .->`, `-.maybe.->`,
      `== sure ==>`), a circle's or a cross's end, `&` and classes at the ends; a link
      written in a box, in quotes or in a label stays words, `box-->c` is the box `box`,
      and a line with an invisible link stays as the repair after mmdr's reading has it.
      The copy is kept only where mmdr reads the statements together as it reads each
      alone (`reads_apart`): it takes a `'` for a quote mark even in a label, so after
      `-->|don't|` a `;` is words to it, and such a chain is still read as before, up to
      the label's link.
- [x] What the nightly fuzzing found (2026-11-21): the run had failed each night since
      2026-11-18. The `layout` target no longer built, as boxes had become round and
      lines part of a tree (`Node::round`, `Edge::tree`); it now fuzzes both, and CI
      checks on every push that the targets build. `model`: lint cut a line backwards
      where a space stood between two `:::` (`a::: :::b`), as both took it out, and
      `convert` panicked on `:Clerk:` in a state diagram, read as the description of a
      state without a name, which is now declined there, and refused by `convert` if
      one reaches it. `hand`: a corner more exact than the block writes it
      (`.44444…`) read back otherwise than written, and is now read to the three places
      written. `mermaid`: a letter of more than one byte where a word of a class
      diagram ends (`cl  ߞpply`), where `classDiagram` or its `-v2` ends, or after
      `block`, was cut through; and, found in the review, a block whose two ends share
      their middle (`a[/]`, the `/` of a parallelogram's opening and of its close) was
      cut backwards, and is now a box with that text. A slow input and one too large
      from those nights are neither on today's code.
- [x] REQ-090 to REQ-094, `--ink`, and REQ-092 for every film (2026-11-21):
      `--look` gives the look the sketch is inked into (plain, vivid, omarchy, light,
      dark, file; auto is light in a film; sketch is refused, being what the pen
      draws); without it, the play's colors, as before (REQ-090). The film starts on
      the clean picture, held, clears to the paper and the pen draws, so its first
      frame shows the diagram and it starts again where it ends (REQ-091). The pen
      takes as long as the diagram asks, 2 to 10 seconds at 700 units a second; with
      `--duration` it takes what holding, clearing and inking leave, and a length too
      short for those is refused (REQ-092). The ink comes in three: the sketch's words
      go, its lines and fills turn into the clean ones, the clean words come, where both
      pictures' words stood at once as ghosts (REQ-093); the page does the same. The
      film keeps a height between 0.3 and 1.4 of its width, as a play does, the
      diagram amid its paper: a tall state machine was a strip of 480 by 1052, a wide one
      1200 by 218 (REQ-094). `--duration` and `--hold` time every film (REQ-092): the
      last frame, the state the film starts again from, holds as long as `--hold`; the
      other holds share what the motions leave of `--duration`, each at least 0.3 s,
      and where that is not enough the motions go quicker, frames left out evenly, a
      hold never (`Frames::retime`). Six steps of a play were 14.4 s, seven 16.3 s.
      A page plays at its own pace and refuses both, but for ink.
- [x] REQ-051, the gap (2026-11-21): an emoji in a git graph's tag or commit (and in any
      words Omaspec's canvas writes) was a small outline in one color in a browser. The
      SVG named DejaVu Sans before the emoji faces, as Mermaid's list does, and a browser
      takes the first face of the list that has a character: DejaVu Sans has `😱` as an
      outline of its own. Each emoji now stands in a piece of its own, set in the emoji
      faces (`EMOJI_FACES`, `emoji` the generic family last), its words beside it in
      pieces of their own, each with its own length: resvg spread a length given around
      pieces over the letters outside them. PNG and PDF drew it in color before and
      still do. An emoji's piece takes no bold and no slant (a browser's would be its own);
      words with an emoji are underlined or struck by one line of their own under all of
      them, as the viewers' decoration of each piece stepped at the emoji; in monochrome
      the words with one are grayed by a filter, since an emoji keeps its colors whatever
      the fill. A stray mark asking for a picture after a letter (`A\u{FE0F}`) asks for
      none.
- [x] REQ-098, a git graph's settings (2026-11-20): `mainBranchName`, `mainBranchOrder`,
      `showBranches`, `showCommitLabel` and `rotateCommitLabel`, in the front matter's
      `config.gitGraph` or an init directive, are carried out by Omaspec's drawer, so
      lint no longer names them. The main branch has the name they give it, and
      `checkout trunk` is no branch that is not there; it takes its place among the
      branches by its order as Mermaid orders them (a branch of order 0 now stands before
      one that asks for none, as there). Without branches shown, the lanes' names and
      lines are left out; without commit labels, the ids (the tags stay). Turned, the ids
      stand at 45° on a plate of the page's color: under their commits where time runs to
      the right, the commits as close as the tags let them and the lanes as far apart as
      the ids drop; down or up the page in a column after the lane's tags, rising to the
      right, so that none meets another. Omaspec lays them level unless
      `rotateCommitLabel: true` is written (Mermaid turns them unless told not to). The
      switches are tested as Mermaid tests them (`0` and `""` are off); a name that is no
      text and an order that is no number are not carried out, and said so. Where the
      drawer leaves the graph to mmdr, lint says all of them are not carried out. The
      lanes stand as Mermaid orders them, its tenth plain branch beside its first too.
      The words of a commit's line, its id and its tag, are tied to it. The
      front matter's other `gitGraph` settings still leave the graph to mmdr. A setting
      Omaspec does not carry out is now a warning, not an info, so `--strict` fails on it:
      the picture is not the one the file asks for. The canvas draws turned words (`Canvas::text_turned`), in the room
      their turned corners take.
- [x] REQ-089, swimlanes (2026-11-20): Mermaid 11.16's `swimlane-beta` is drawn by
      Omaspec, an entry of its own in `NATIVE`. It is read as the flowchart it is, its
      lanes, the subgraphs at the top, taken out and the lines where they stood left
      empty (`mermaid/swimlane.rs`); the flowchart's drawer lays it out along its flow
      and puts each node in the band of the lane that first writes it, a node written in
      none in that of its first neighbor that is (`layered/lanes.rs`). The bands are of
      one size, one after another across the flow in the order written: rows with their
      names at their left for `LR` and `RL`, columns with their names over them for `TB`
      (the default) and `BT`. Within a lane the nodes keep the layout's order, two that
      would meet along the flow side by side. The lines are routed again around the boxes
      and the names, so none runs through a lane's name, and their words move along them
      where they would meet a box, a name or other words. All three dotted lines of
      Mermaid #8853 are dotted (Mermaid draws one of them solid). A subgraph inside a
      lane is no box of its own, and its `direction` none of the whole; a loop moves with
      its node, and the band is wide enough for it. Lines by hand are laid in the bands;
      a box stands where its lane puts it, so none is held. A line to a lane (or to a subgraph in one) is drawn as the flowchart
      the diagram is, its lanes as boxes; a lane not closed is mmdr's error on its line;
      the text mmdr reads where the drawer declines is said in the kind's `NATIVE` entry
      (`as_read`). The editor names it a swimlane diagram. The gallery has a bike repair
      shop.
- [x] `render --fit` fits again (2026-11-20, found in the test of 1.0.0): the scale that
      fits was the area's, to two places, but the pixels are the sizes rounded up, and a
      sequence of 60 messages at `--scale 16` came out at 12.73 as 4265 × 23449 pixels,
      ten thousand too many, so `--fit` refused it; 0.9.9's layout had happened to fall
      under the limit. The scale now steps down until the rounded pixels fit, by
      hundredths, or by a ten-thousandth of itself for a tiny picture, so it always ends
      (`tiles::fitting`); a picture too large even at a hundredth is cut into tiles at
      that, which are refused past their most, rather than drawn as one blank pixel. The
      CLI test that had caught it runs its `--fit` half in a release build only, which
      the checks before a push now include.
- [x] REQ-101, a git graph's outline (2026-11-20): `omaspec outline` and `inspect` gave
      a git graph none, and the editor's outline beside the source stood empty. It now
      lists the graph's branches and what it names: a commit by its id, a tag, a merge
      and a cherry-pick, each on its line; a commit without an id is not listed.
- [x] REQ-095, which films need a GPU (2026-11-20): `omaspec play --help` said the pane
      is drawn on the GPU, also for `--ink`, which draws without one. The help now says
      a play, a fold, a dive, a lockstep, a history and a time travel are drawn on the GPU (by
      tiny-skia where there is none), and `--build`, `--ink`, `--themes`, `--traffic`
      and every page without one; each of those options says it, as AGENTS.md and the
      reference do.
- [x] REQ-100, room on a branch's plate (2026-11-20): a git graph's branch names stood
      on plates 8 units wider than the name and 4 higher, so a short name of narrow
      letters (`jj`) was squeezed and read as one letter in a large graph's overview.
      A plate now has 12 units round the name across and 5 above and below, and is
      never narrower than 2.8 times the label's size (`gitgraph::plate`); the lanes
      and the first turn make room for it whichever way time runs, and lanes across
      stand as far apart as their plates are high, at a large label too. The first
      commit stands far enough on that its id and its tag keep clear of the plates.
- [x] REQ-097, an event with a comma in its name (2026-11-20): `omaspec play
      --events` split at every comma, so `hello, world` could not be taken, neither
      with a backslash nor in quotes, and the play stopped with exit 2. A comma written
      `\,`, or inside double quotes round a whole event, is now the event's
      (`--events 'start,"hello, world"'`); a quote inside a name (`[mode == "x"]`) stays
      the name's; `--events` may be given more than once (`moving::events`). An empty
      event or a quote not closed is an error, not a play of the first events offered.
      Where an event was cut at its comma, also inside a guard or where the step before
      took its first half (`hello` beside `hello, world`), the error says how to write
      it, quoted for the shell.
- [x] REQ-099, two git graphs in one text (2026-11-20): Mermaid refuses a second
      `gitGraph` in one text, and mmdr drew both as one picture without a word, while
      lint said only that Omaspec left the graph to mmdr. The second head is now an
      error on its line, from lint and from render, and nothing is drawn, also behind
      an init directive over several lines, a fence or a byte order mark, and before
      anything else the drawer leaves to mmdr. What Mermaid
      refuses as well is a part of each kind's entry in `NATIVE` (`wrong`), beside what
      Omaspec's drawer leaves to mmdr (`refusal`); for a git graph its reader says it
      (`omaspec_uml::gitgraph::mermaid_wrong`)
- [x] REQ-089, the head (2026-11-20): a Mermaid diagram whose first line is a word
      that names no kind is refused on that line, by render and by lint alike, in the
      same words (`omaspec_core::mermaid_head::unknown_said`). mmdr read such a head as
      a flowchart's first nodes: Mermaid 11.16's `swimlane-beta LR` came out as boxes
      `swimlane` and `beta`, its direction left aside, while lint only warned of an
      unknown head. A first line is what mmdr makes of it (`omaspec_core::mermaid_head`,
      its reading of a line, its masking of brackets and quotes and its arrow written
      again there, with one table of the kinds as mmdr reads them and as mermaid.js
      writes them): a kind
      as mmdr knows it, its words lowercased and ending where no letter or digit
      follows, so `block`, `xychart` without `-beta` and `Flowchart` are drawn as
      before (another case than mermaid.js's is told, as GitHub does not draw it); a
      flowchart's line, any with a `-` or `=` outside brackets (`A --> B`, `subgraph
      back-end`), drawn and only told; or nothing, refused: a line mmdr reads as
      nothing, and a kind's word with a hyphen in it, which mmdr took for two boxes.
      Lint finds the head as mmdr does, after front matter that follows a comment and
      behind a byte order mark; the formatter takes the kind from the head alone, under
      a fence too, and leaves a treemap's indentation as it leaves a mind map's; the
      language's detection takes a kind mmdr reads without `-beta` where it stands
      alone on its line (`xychart`, not `block = parse(x)`).
      `graph` without a direction is a flowchart, and says nothing.
- [x] REQ-096, the first frame of a play (2026-11-20): `omaspec play` kept the frames
      from the moment the play began, while its bar came under the picture and the
      picture glided to its fit above it, so the first frame was the picture as large
      as before the bar, cut at its top and not yet veiled: brighter than the rest
      (the 99th percentile 183 against 142, the studio measured). The play's opening is
      now drawn and left out, as the fade-in is (`Stage::begun`): until the pane has
      heard of the bar, which may tell its height a frame late, and until the picture
      has glided to its fit and the veil is down with the start lit. The film starts
      with the play standing at its start. `the_first_frame_of_a_play_has_the_picture_standing`
      holds the paper's top edge in every frame and the first frame's brightness to the
      last's, at the 60 frames a second of a WebP, without a GPU. And REQ-076, which the
      studio found playing through on 1.0.0, is off the roadmap: its rest was done on
      2026-11-18
- [x] Release 1.0.0 (2026-11-20): the landing page (`docs/index.html`) shows every
      function, the ones that move as live pages made by Omaspec itself
      (`docs/live/update.sh`); its film is the editor recorded headless, every frame on a
      recording's clock (`docs/images/film.sh`, `editor/tests/film.rs`); the guide is a
      PDF of 47 pages whose examples the release build draws (`docs/guide/pdf/build.py`),
      to download from the page
- [x] Release 1.1.0 (2026-11-24): the security review's fixes, what the nightly
      fuzzing found, and PlantUML's sequence, activity and state diagrams drawn
      natively in every example of its guide (553 of 897 in all). A release's notes
      are `docs/releases/X.Y.Z.md`, which the release workflow makes the text of the
      GitHub release (1.0.0's written afterwards); the guide drawn by 1.1.0 has 49
      pages, with the swimlanes
- [x] What the review of the presenter mode and the project's page found (2026-11-20).
      The speaker's window takes the presentation's keys and gives way to a question,
      a list or a menu opened there; a step back shows the notes of the step it went
      back to; tabs stay put while presenting, Escape ends it from any tab, and closing
      the diagram's window stops the play too; the opening notes are found in Mermaid
      and in a Markdown document; a diagram that does not play is presented all the
      same and the speaker is told why. The project's page keeps each picture's dark
      colors in `--look auto`, follows the links of a project inside the project as
      that one's, tells a folder's map that does not draw, says the project's name as
      written, survives a broken address, leaves the browser's shortcuts alone, takes
      no click on the trail for a tile, and `-o page.html` names the page itself
- [x] The project as one page (2026-11-20): `omaspec export DIR --html` writes
      `<project>.html`: the map, and a click on a tile, or on a state that opens a
      diagram of its own, zooms into what it opens as the editor dives into it, a light
      around the hop under the pointer; Escape, ←, Backspace or a name in the trail
      zooms back out, Home to the map, F full screen. Every picture the map leads to is
      in it once (`editor/site.rs`, from the map along the links, a link that names
      several diagrams going nowhere as in the window), in the export's look, the page
      dark with the viewer's scheme in `--look auto`; the address says where the camera
      is (`#Day/Order`), and less motion changes the pictures without the zoom
      (`export::project`, the camera shared with the flight's page in `zoom.js`). A
      diagram that does not draw is told and fails the export
- [x] Presenter mode (2026-11-20): Shift+F5 (*Present the diagram*) shows the diagram full
      screen in a window of its own (`omaspec-preview`) and plays it, and the editor's
      window becomes the speaker's: the time since the start, where the play is (a
      scenario's message of all, a machine's state with its options and their digits, a
      trace's line), the notes of the step in large letters (the comments right above
      its line, up to a blank one; before the first step those that open the diagram;
      Omaspec's own comment lines are none), the line itself, and for a scenario the
      notes of the next message. The keys of a presentation page and of a presenter's
      remote work in both windows (Page Down and Page Up too), F switches full screen,
      and Escape ends it as it began; closing the diagram's window ends it as well
      (`editor/present.rs`). Not tried in a real window on two screens yet
- [x] What the review of the capability map and time travel found (2026-11-20). A
      capability map gives each tile the room its words and the tiles in it need, so a
      name deep in an area no longer runs over its edge; an area with neither a value
      nor capabilities is dashed, as the legend says; a name is tied to where it stands
      before its value; drawn by hand a tile keeps its strokes by its name; and the
      areas lie on the rectangle's fill, the theme's surface. A box that went glows red
      where the glide paints it, not where it stood; an edit after a way from B that was
      not taken does not glow; stepping back on a history's page turns nothing red; a
      commit of a long history glows as long as it stays; and a time travel whose end
      does not draw says which. The outline reads each diagram of a file as its kind
- [x] Time travel (2026-11-20): what changed glows a moment after the glide from another
      version of a diagram, in the colors `omaspec diff -o` marks a machine in: a box
      that went turns red as it fades out, and once the new one stands a box that came
      glows green, one that changed its size or its color amber; a box that only moved
      does not (`paper::glow`, worked out from the glide's boxes, so every diagram that
      glides by its parts glows). In the editor beside a comparison, *Compare: glide
      from B to A*; in a history (`play --history`), each commit as it comes, in the GIF
      and on the page alike; and `omaspec play FILE --from REV [--to REV] -o
      change.gif|.html` travels from one commit to the file as it is now, or to
      another, for a review. The README's history picture glows now. A line that came
      does not glow yet, only its box; the page reads a box's change from its size,
      not its color
- [x] Capability map (2026-11-20, [ADR 0035](adr/0035-a-capability-map-of-its-own.md)):
      `@startcapabilities`, Omaspec's own: what a business can do as tiles inside
      tiles by their stars (`* Area`, `** Capability`, up to six levels), each on one
      scale (`levels Maturity : none, basic, managed, leading`, a step with a color of
      its own if wished) by the value after its last colon, the legend under the map.
      The steps run from near the page to the theme's accent where the file gives no
      colors; dots under a name count its step, so the map reads without the colors; a
      capability with neither a value nor others in it has a dashed edge (*not rated*),
      one holding others its step's color under its name alone. The areas stand as
      columns, a few to a row; deeper capabilities side by side as they fit. Read and
      drawn in `omaspec-uml/src/capabilities/`, its look shared with the story map
      (`own_skin`); its words are tied to their lines, the legend's to the scale's line
      where each step is written. Lint says no other tool draws it, the outline lists
      its areas and capabilities, and the gallery has a city library
- [x] What the review of the trace river and the story map found (2026-11-20). A
      story map draws in the spec pack (`style strictuml`), its stories take
      `NoteFontColor` and `NoteFontSize`, a story ending in a colon is refused as
      naming no release, and drawn by hand its cards keep their strokes when lines are
      added above them. `fmt` keeps a story under its step past a release written
      between them, and the outline shows an activity's whole name. A trace river puts
      a deviation at the state an object began in only for its first line, not for one
      no longer followed. Words no line of the source writes (where traces left a
      machine, a transition that went) are tied to no line (`NO_LINE`), and a pane
      counting the lines of tied words cannot run past the last
- [x] Story map (2026-11-20, [ADR 0034](adr/0034-a-story-map-of-its-own.md)):
      `@startstorymap`, Omaspec's own: activities across the top, the steps of each
      under it, the stories under each step (`* By card : MVP`) in the release they go
      into, the releases as bands across the whole map, their goals under their names,
      every other band tinted, the stories planned in none in a band after them. Read,
      measured and painted in `omaspec-uml/src/storymap/`; a story naming a release
      the map does not have is refused with the ones it has. The cards take the
      theme's colors (an activity the accent, a step a rectangle, a story a note), each
      card's words are tied to their line, and the map holds the backbone at the top
      of the pane and the releases' names at its left. A line the engine declines is
      not sent to the Java PlantUML (`kind::OWN_BLOCKS`, signal flow too), lint says
      on the `@start` line of both that no other tool draws them (`omaspec-only`),
      `omaspec fmt` indents a story map by its activities, steps and stories, the
      outline lists its releases, activities and steps, and the gallery has an
      online course
- [x] Trace river (2026-11-20): `omaspec coverage FILE --traces T --river -o river.svg`
      draws what recorded traces take of a machine as a river: each way as wide as it
      was taken against the way taken most, up to five times an arrow (a transition's
      `weight`, marked as `' mark (Omaspec) river`), one never taken thin and dashed,
      and where traces left the machine a note in the color of what was missed beside
      the state they left from (`' mark (Omaspec) left 2 Open`). Coverage counts where
      each trace was when its line deviated (`Coverage::left`), at the state it began
      in for a first line
- [x] What the review of the lockstep in the window and the orbit found (2026-11-20). A
      digit that steps the scenario steps the machines too; an edit that ends the
      scenario's play closes the machines beside it, and a play stopped in another tab
      leaves them; the panes are drawn at the window's scale, and a drawing for an
      earlier set of panes is not taken for a new one. On the ring a way on runs along
      it though its way back is written first, a machine without a start or an end has
      neither drawn, F5 on a machine the ring shows in part says to play it in the
      diagram, the boxes and ways are painted as the diagram paints them, so what
      recorded traces cover is marked on the ring too (Mermaid's as well), loops near
      the top get their room above the ring, and a Mermaid ring or table that cannot be
      drawn says why as the engine says it. A machine's quiet bar leaves the event its
      room beside the head
- [x] Orbit (2026-11-20): a state machine's life cycle on a ring, a view as the table is
      (`' view (Omaspec) orbit`, View menu, `omaspec orbit FILE -o`), for PlantUML and
      Mermaid alike, in the look of the machine's states (`omaspec_uml::state::draw_orbit`).
      The states of the top level stand round a circle in the order a run reaches them,
      clockwise from the top, where the ring opens with the start on one side and the
      end on the other: depth first, a detour (a state that only leads back) right after
      the state it leaves, then the way that reaches the most, a branch that ends soon
      after the rest, a state no way reaches last. A way on runs along the ring, a way
      back is an arc inside it running back beside the part it passes, a way that skips
      ahead an arc outside it, each a lane further where it runs beside another; a way to
      itself a loop outside its box. A state around others is one box that says how
      many it holds, the ways into and out of what it holds its own. The words of each
      way stand at the first place along it that keeps clear of the others and of the
      boxes, tied to its line; the map says where each state and way is. Golden SVGs in
      `tests/orbit`
- [x] The lockstep in the window (2026-11-20): F5 on a scenario whose participants have
      state machines in its project opens each machine in a pane beside it
      (`editor::beside`), drawn as the window draws the compare pane, each under a key
      of its own. As the scenario takes a message, each machine takes the steps of the
      messages that moved it, a group's now and the receiver's at the moment the
      message's light arrives (`Paper::play_ask_at`: a play is drawn from the time
      alone); ← and Home take the machines along, Esc closes them. Each pane's bar is
      the quiet one, its head as wide as the bar
- [x] What the review of the ink and the flight's page found (2026-11-20). An ink page
      gives its diagram back again: the source is in the sketch, its first picture. A
      line the sketch draws in two runs was drawn at twice the pace: the pen draws by
      its longest run and lets its dashes go once drawn. The sketch and the clean
      picture are held to one size before a page too. On a flight's page the picture
      coming lies over the one it leaves, the trail over both; a diagram with an
      `!include` is drawn in its folder there and in a flight's picture too, an error
      names the file's line, a room is measured from the picture's corner, and the
      map is given no source of its own. The pen finds the elements it draws through
      `omaspec_render::export::drawn`, as exports read SVG only there
- [x] A dive and a flight as a page in vectors (2026-11-20): `--dive Day/Order -o
      flight.html` on a project's file draws the map and each diagram on the way as the
      window draws them, in the theme's colors with their links (`editor::flight`), and
      for each step the room the next grows out of, a tile's picture or a state's box,
      in the picture's own units (`export::flight`); its script zooms about the one
      point a zoom keeps where it is, so the room becomes the whole of the next
      picture, the picture left fading before its words grow too large, the way back
      the same zooms backwards, a trail of the names at the foot. Without a GPU;
      `#t=4` holds it still
- [x] From sketch to ink (2026-11-20, the last of the animations that show what a
      diagram means): `omaspec play FILE --ink -o ink.gif` draws the diagram in the
      sketch look (docs/adr/0023) stroke by stroke as with a pen (`editor::ink`): each
      line of the sketch measured as usvg reads it and drawn in a time as long as it
      is, down the picture in bands, so a box, its words and the lines that leave it
      come together, the fills and words fading in as the pen reaches them, the paper
      there before the pen; then the clean look the pane draws it in fades over it,
      of one layout, so the ink lies on the sketch. Drawn without a GPU; as `.html` a
      page in vectors (`export::ink`) whose script measures each line itself, `#t=3`
      holding it still
- [x] What the review of the flight and the sweep found (2026-11-20). A flight clicked a
      link in its middle, where a state inside it that links too took the click: it
      clicks where only the link named lies. A name with a `/` of its own is one name
      where it names a link, a flight starts on the map, a link that opens no diagram
      says why, and a diagram without links says it has none. A flight through a
      folder's tile goes on in the folder's map and comes back out through it, and one
      that visits a diagram twice comes back the way it went in: the trail takes a
      diagram's latest step (in the window too). A sweep holds three pictures at a
      time, not every theme's, and mixes them row by row; its theme names are trimmed
      and given after an equals sign (`--themes=a,b`), so the file is never taken for
      one; a theme's style sheet stays out of the page, where it would reach every
      other theme
- [x] A theme sweep (2026-11-20, the sixth of the animations that show what a diagram
      means): `omaspec play FILE --themes -o sweep.gif` draws the diagram in every
      bundled Omarchy theme as the pane draws it there (`editor::sweep`), each theme's
      picture with its name and its place under it in its own colors, and sweeps each
      over the one before from the left with a soft edge, the last into the first so
      the picture loops; many themes stay shorter. `--themes=tokyo-night,gruvbox` names
      them (a theme of Omarchy's or the writer's own, a `colors.toml`, or `built-in`).
      Mixed in pixels without a GPU; as `.html` a page in vectors (`export::sweep`):
      the pictures laid on one another, each with ids of its own and the faces once,
      the one sweeping in masked with a soft edge, `#t=4` holding it still
- [x] A flight through the project (2026-11-20, the fifth of the animations that show
      what a diagram means): `--dive Day/Order/Payment` on a project's
      `omaspec-project.toml` zooms from the map into the tile Day, on along the state
      Order into the diagram of that name (docs/adr/0018), on into Payment, and back
      out step by step; the dive into one tile, chained, each link looked for in the
      diagram the flight is in, a link not there named with those there. The example
      is `examples/cafe`: a day, an order and its payment. `omaspec lint` reads a
      project's `omaspec-project.toml` as what it is, not as a diagram: nothing to say
      of a good one, the line where it is wrong
- [x] What the review of the time-lapse and the traffic found (2026-11-20). A history's
      glide ended off its middle and the picture jumped as it stood: the view is held
      through the pane (`Paper::hold_view`), the fade under way too. A commit that does
      not draw is passed over and counted, not the end of the history; each commit is
      read in its own language, and the pane is as large as the largest commit, not the
      newest. Traffic under a `scale` lay off its machine: the map's pixels become the
      picture's units. The dots of several traces lie apart, a leg ends before the
      object's next line, an asked width is the frame's and a large machine is made
      smaller to fit; a trace is read once for all its objects
      (`omaspec_model::follow_traces`, the timing diagram of a trace too). A fold's or a
      history's page under a `scale` glided at half its size: each glide is made in the
      picture's own units (`timelapse::remap`). A page held midway says so and lets the
      still glide go on the next step; `update.sh` finds `history.sh` from anywhere
- [x] Traffic (2026-11-20, the fourth of the animations that show what a diagram
      means): `omaspec play FILE --traffic T.jsonl[,U.jsonl] -o traffic.gif` lets
      recorded traces flow through a state machine at once (`editor::traffic`). Each
      object is a dot that rests in its state, spread on a spiral so many in one state
      stay apart, and runs along the transition each of its lines takes; each trace
      flows from its first line to its last in the same ten seconds. A transition
      glows the brighter and wider the more dots have passed it, those never taken
      blink grey and dashed once all have flowed, a dot turns red with a ring where its
      trace deviates, and a caption counts the objects, the transitions taken by then
      and the deviations. The ways are those `omaspec trace` follows, so the counts are
      coverage's. Drawn as SVG and rasterized without a GPU, a narrow machine in the
      middle of a wider frame; as `.html` a page in vectors (`export::traffic`),
      `#t=6` holding it still. The example is a project, `examples/orders`: an order's
      machine and a week of twenty orders, one shipped before it was paid (wrong on
      purpose, which the clean-examples test lets the trace say)
- [x] A time-lapse of a diagram's history (2026-11-20, the third of the animations that
      show what a diagram means): `omaspec play FILE --history -o history.gif` plays
      every commit that changed the file, oldest first (the newest 60, one that left it
      as it was or left it broken passed over), each picture gliding into the next as
      the pane glides when the diagram is edited (docs/adr/0029), what is new coming in
      and what went fading, the commit under it (`editor::timelapse`, the screen's
      captions). Every commit at the scale the largest fits at, its top where the
      largest's is, so the diagram grows as it grew and none is cut; the view is held
      through the paper (`hold_view`), so a glide ends where the picture then stands; a
      long history goes through faster. As `.html` a page in
      vectors: the fold's page with captions, kind `history`, that starts again after
      its last commit; each picture as large as the largest with itself in the middle,
      its glides moved with it; `#2` shows the third commit, `#2.5` the glide from it
      halfway (for a fold too). The README shows the gallery's parcel growing over six
      commits (`docs/images/history.sh` makes the repository, `update.sh` the picture)
- [x] What the review of the lockstep found (2026-11-20). A trace's line that deviates
      flashed in the window only while something else moved: the play counts its
      flashes until they end, and drops those that are over. A page took a group's
      steps all at once, so a state glowed that was never rested in: each machine's
      steps follow one another, as in the picture. Many machines or a narrow picture
      cut the last panes off: the picture grows as high as each needs. The page
      embeds the fonts once and gives each picture's ids their own prefix. A reply the
      machine knows nothing of, which the check leaves alone, is no step of it, and a
      machine begins where the first message it can take is taken. Where nothing is
      known of where a machine is, its pane's edge flashes red instead of its states
- [x] Lockstep (2026-11-20, the second of the animations that show what a diagram
      means): `omaspec play SCENARIO --lockstep -o lockstep.gif` plays a sequence
      diagram beside the state machines of its participants in its project, each in a
      pane of its own (`editor::lockstep`, the screen's `Shows::Panes`). As a message's
      light reaches its receiver, the receiver's machine takes the transition the
      message fires; a message the machine cannot take runs red in the scenario, the
      machine's states flash twice in red, and its bar says why in the check's words.
      Each participant's way is the scenario's own check played out
      (`omaspec_model::lockstep`, recorded by `across::moves`): it may start anywhere,
      lit where the first message can be taken, a group's branch starts where the
      group began, so the play is red exactly where the check reports. A machine
      plays its way as a recorded trace plays (`Play::followed`), with a quiet bar
      (`beside`: where it is, the event, what went wrong over two lines, nothing to
      press); a trace's deviation now flashes too. The machines share one scale in
      their column. GIF, WebP and animated PNG; the example is a project,
      `examples/home` (a door, an alarm, and a guest back for the keys), as a lockstep
      needs the machines beside the scenario, and it is the README's picture. As
      `.html` a page in vectors (`export::lockstep`, its own script and style): the
      times of everything worked out on the panes' beat (`lockstep::page`), the
      script only showing the moment it is at, `#t=5` holding it still 5 seconds in.
- [x] What the review of the build found (2026-11-20). A sequence with a picture in a
      note built only that picture: the frames now take the whole picture's root. A
      `scale` drew the build at the wrong size, and a page grew its boxes about the
      wrong point: the frames lay the picture's units into its size, and the page has
      its points in the picture's own units. A page left the lines of a picture
      without parts out, and drew dashed lines solid. The caret's line scrolls a third
      down on a page too, a very large diagram is no longer cut, a carriage return
      ends a line, the code's family is quoted with `monospace` behind it, and the
      paper is the rectangle before the first group. A class comes in at the line that
      opens it (`class B {`), not at its first member, the line that names a box being
      looked for outside titles, captions, notes and comments; and the letters of the
      code are measured in its face, not taken at 0.6 of its size
- [x] Code becomes a picture (2026-11-19, the first of the animations that show what a
      diagram means): `omaspec play FILE --build -o build.gif` stands the source on
      the left and the diagram on the right, and as a caret walks down the source the
      diagram builds itself up: each line sets down what it draws, a box growing in, a
      line drawing itself to its head with its words after it, while the lines not
      reached yet wait dimmed (`editor/building.rs`). A diagram that marks its parts
      (states, classes, flowcharts, activities) builds part by part, each at a state's
      first line, the line whose words it holds, else the first that names it, a part
      no line writes (an end, a bar) with the nearest box one does; a sequence builds
      from the top down, as far as each line's words and messages reach. Each frame is
      one SVG drawn by resvg, without a GPU; a source too long for twelve seconds is
      gone through faster. GIF, WebP and animated PNG, in the theme's colors or the
      file's own; and as `.html` a page in vectors (`export::building`, its own script
      and style): the source in its colors beside the picture with its parts marked,
      the script growing each part's groups in, drawing each line along its points and
      moving the caret, `#t=4` holding it still 4 seconds in. The README shows the
      gallery's new parcel built up (`docs/images/build.gif`, by `update.sh`)
- [x] What the review of Q1 and of F9 to F11 found (2026-11-19). An activity's line
      with a head keeps the head's length straight before its tip
      (`curve::path_arriving`): a short last run had curved under the head and out of
      its side. The map tells an activity's lines as drawn, round, and keeps their
      corners as their path: a glide and the light ran them at right angles. Labels
      below a row of `elseif` diamonds get the room they take, one right of a way
      down and one left of the next no longer overlap. A delay without words leaves
      a lone lifeline whole, where an empty gap had cut it. A `rect` that holds
      nothing no longer widens the frame around it across every participant. A
      number's circle stays in its message's band and in a `rect` around it, a
      message's words keep clear of it (a self message's of `10` lay under it), and
      its number reads on the arrow color of its own message
- [x] An activity's lines round their corners (2026-11-19, from the review sheet, Q1,
      decided so): they bent at right angles, as PlantUML draws them, where the lines
      of states and boxes curve. They now round their corners as those do, by 16 at
      most (`activity::skin::CURVE`; a state's reach 48, an activity's runs are
      short), and `skinparam linetype ortho` keeps right angles. A branch of an
      `elseif` chain whose line turns right below its diamond, toward a swimlane
      further right, has its label left of the way down: `bug` lay on its line, the
      curve made it plain
- [x] The media player's `failed` into Off without a hook (2026-11-19, from the review
      sheet, F12): the sheet showed the line bending into its head a few units under
      Off. The line back up into a box that F3 gave a bend of its own
      (`graph::ARRIVE`) arrives so here too: a curve, then a straight run into the
      head; `media-player.mmd`'s golden holds it
- [x] A Mermaid sequence's numbers in circles (2026-11-19, from the review sheet, F11):
      `autonumber` wrote `1 open`, as PlantUML does; Mermaid draws each number in a
      small circle at the start of its arrow. A sequence read from Mermaid now does
      (`sequence::Diagram::numbers_circled`): the circle in the arrows' color, the
      number in white or black, whichever reads on it, the words on their own
- [x] A Mermaid `rect` as wide as what it holds (2026-11-19, from the review sheet,
      F10): its band spanned every participant, from the first head to the last,
      where Mermaid's covers what stands in it. It now reaches 8 past what it holds
      (messages, notes, frames, a `rect` inside), and spans the participants only
      when it holds nothing
- [x] A delay's words between two lifelines (2026-11-19, from the review sheet, F9): the
      words of `... two days later ...` stood in the middle of the picture, on the
      lifeline that ran there, the dotted line through them. They now stand in the
      middle of the room between two lifelines that holds them 8 from either and
      from their bars, the one nearest the picture's middle; where none does, the
      room found by drawing (`sequence/draw/room.rs`) sets the two nearest the
      middle that much further apart. Over a lone lifeline the dotted line stops
      short of them. In `every-construct`, `5 minutes later` left Gate's lifeline
- [x] What the review of the room found by drawing and of F8 found (2026-11-19). A
      frame knows the lifelines its items stand by: a lifeline it does not hold keeps
      the margin from its sides wherever it stands, also just inside one a note
      pushed past it. A frame holding a message from the left edge or to the right
      edge reaches that edge and asks nothing of it: before, a frame around `[-> A`
      was drawn four times and still poked past the picture. A note keeps before
      where a message to the right edge ends, and a drawing that takes nothing away
      from what lacks room is the last; a sequence's words are measured once, not
      for each drawing. A line back out of a diamond moves only on a box whose
      outline reaches the middle of each side (`graph::Node::round`, `Nested::round`:
      a diamond, a circle, a flowchart's shapes, not an actor as wide as its name),
      onto a side point no line meets yet (one within the rank included), its far
      end on a flat box only, clear of labels, names and frames it was not in. A
      state's line meets a diamond or a circle on its outline, also where several
      share a side (`Nested::clip`)
- [x] A line back out of a diamond leaves from its side (2026-11-19, from the review
      sheet, F8): a box that is not flat (a choice's diamond, a circle, a stadium)
      meets its lines at one point of each side, so in the door with an alarm `wind`
      arrived at the top corner of `check` and `[calm]` left it there back up, and the
      two read as one line that splits. Where a line back against the flow starts or
      ends at the point another line meets, it takes the side corner the way it heads
      for, its run along the flow moved out past the corner where it ran nearer, onto
      the box it ends at still (`graph::corner`). Flowcharts gain it too: the pipeline's
      and the platform's dotted ways back no longer end where a line leaves; two
      corners near another line and two bends fewer in `tests/lines`
- [x] Room found by drawing (2026-11-19), after the review of F6 and F7: the columns
      were set apart by a guess of how far a frame's side or a note beside a lifeline
      would reach, and each guess missed a case (bars nested on bars, a self message
      at a frame's side, a note inside it, the head of one created, a frame over one
      lifeline without a bar). A sequence is now drawn, and what crowds a lifeline it
      does not belong to is measured (`sequence/draw/room.rs`): a frame's side or a
      note closer than its margin to a lifeline or a bar outside it, a frame's words
      over the last lifeline it holds, a frame past the picture's edges or the end
      of a message from outside. The columns between move that much further apart
      and it is drawn again, four times at most; most sequences are drawn once. A
      note beside a message stands by the bars at its end, a bar opened after a note
      beside its message takes the arrow, and a message that creates a wide head
      keeps its words between its ends. `every-construct` and `styled` had a frame's
      side on a lifeline. `mirrorActors: null` (or a list, an object) leaves the
      participants below, as Mermaid writes no such value over its own `true`
- [x] A frame's words keep off the lifeline it ends by (2026-11-19, from the review
      sheet, F7): the columns under a frame stood just far enough apart for the words
      of its tab to end on the lifeline at its right side, so `[every day until
      shipped]` ran up to the frame's border and over Stock's lifeline. The words of a
      tab and of an `else` now end a margin before that lifeline, clear of a bar on
      it, and a frame over one lifeline holds them right of it, clear of the lifeline
      beside
- [x] What the review of F6 found (2026-11-19). A note beside a message stands by its
      lifeline again, not by the bars the frames now keep clear of. A bar opened on the
      sender right after its message counts as one the message opens, and a frame
      around a message that creates a participant holds its head. The columns leave
      room for the frames' sides: as many margins as frames share a side, past the bar
      or the head there, and a full margin more to a bar on the lifeline beside; at
      the first and the last lifeline the frames stay in the picture, where two nested
      ones ran 8 past its left edge before. `straight::merge` measures only the run it
      moves and the step it grows against the boxes' sides, not the line's nearest
      part. `mirrorActors` is false as JavaScript has it (`0`, `null`, `""`), and the
      front matter's `null` and `~` are YAML's null. A bar grows for an opened state
      before or after it too
- [x] A frame keeps its margin from the bars at its sides (2026-11-19, from the review
      sheet, F6): a sequence's frame reached 10 past the lifelines of its outer
      participants, so where one was active (the checkout's `par` and `opt` at the
      web shop) its border ran 5 from the bar, and a frame inside another sat on it.
      A message now tells the frames around it where the bars at its ends stand,
      nested ones and one it opens too, and the frames keep their 10 from those
      edges, on the left and on the right
- [x] What the review of F4 and F5 found (2026-11-19). The bars of forks and joins
      are laid out by `graph::open` (`graph::span`, `Nested::spans`), not by the state
      drawer, as AGENTS.md has it: only the layout of the ranks is laid again while a
      bar grows, and the bar is cut back before notes and labels are placed, so a note
      beside a bar stands by it as drawn. A bar grows only for the states before and
      after it along the flow, in its own frame: a state beside it, by a line within
      the rank (`f -right-> C`), no longer pushes itself out on every pass, and a line
      into the bar's end keeps that end. Cut back, a bar is as long as it was measured
      at least, the window moved in from an end it would run past. No lengths are kept
      by thread any more, so a state diagram inside a note leaves the bars around it
      alone. A line `straight::merge` moves keeps 12 from the sides of frames too, and
      so does the step it grows. `omaspec convert` writes `hide footbox` as
      `mirrorActors: false` and back (`settings::mirrors_actors`); ZenUML takes the
      setting too, and the front matter's `False` and `TRUE` are YAML's booleans
- [x] A Mermaid sequence's participants again below their lifelines (2026-11-19, from
      the review sheet, F5): Mermaid draws them there unless `sequence.mirrorActors`
      is false; Omaspec drew them only above, so the lifelines ended in nothing. They
      now stand below too, as PlantUML's foot boxes do, and `mirrorActors: false` in
      the front matter's `config` or a `%%{init}%%` takes them away; lint no longer
      says the setting is not carried out. A participant destroyed has no foot, as in
      Mermaid; one created has one
- [x] Fork and join bars as long as what they split and join (2026-11-19, from the
      review sheet, F4): a bar was measured at 80 before the layout knew where its
      states go, so the lines to states wider apart than that ran to its ends in a
      hook, and both lines into a join met at one point of it. Once a state diagram
      is laid out, each bar grows to reach past the middles of its states, and the
      diagram is laid out again while one grows (`graph::span`); then each bar
      is cut back to 16 past the outermost line on it. Its lines keep out of bundles,
      so each runs straight into the bar at a place of its own, in both languages and
      both directions. A run `straight::merge` moves to take a step out keeps 12 from
      the side of a box it comes to pass, or as far as before (`straight::BESIDE`):
      the longer bars let the line `again` of `tests/state/states.puml` slide 3 units
      beside a nested state. The media player's `stop` loses its hook for it. No
      crossing or word more in `tests/lines`, one jog less
- [x] What the review of REQ-077, REQ-079, F1, F2 and F3 found (2026-11-19). A `{{`
      a note never closes no longer takes every line up to some later `}}` in the
      file: it opens a diagram only where its `}}` comes before the note's end, an end
      line inside the diagram being its own only after a block of its own opens.
      Mermaid notes write `{{` as text again, as Mermaid does: only a PlantUML
      diagram's text draws a diagram inside it. A note with a diagram inside stands
      on all its lines for the caret and a click (`text::Line::rows`). What draws a
      diagram inside is put back by a guard, also after a panic the renderer catches,
      and its pictures are kept by how deep they were drawn; lint says it of a
      diagram four deep too. The turn of a look's colors (F1) measures a picture only
      where the file writes colors of its own, read from its lines outside the look:
      a dark look costs no more than a light one for the rest (agent-platform's 30 ms
      in a fresh process are its fonts, 3 ms after), and a ground is chosen against
      its words as they are finally drawn. A tree of subclasses keeps off a head that
      stays, and a tree put back for one puts back the trees its heads stand by
      (`graph::join`). The line from a subgraph's title to a node inside it is routed
      by `graph::across` (`from_title`, `down_the_side`), not by the flowchart drawer
- [x] A Mermaid sequence's bars on a lifeline in the colors of a look (2026-11-19,
      from the review sheet, F2): a look's `%%{init}%%` set no `activationBkgColor`,
      so the bars kept mmdr's near white (`#F4F4F4`) on a dark paper. Every pack now
      writes them as the PlantUML packs write `SequenceLifeLineBackgroundColor`: the
      boxes' fill (Omarchy: its surface), the muted line around them
- [x] A line back up into a box ends on a bend, not on a hook (2026-11-19, from the
      review sheet, F3): the sheet showed the media player's back-and-forth lines as
      crossing, which the picture at full size does not; what it does show is the
      head of a line that runs back up into a box (`play` into Playing, `stop` into
      Stopped, `reset` into Off) sitting on a corner two pixels long, as its bend lay
      12 below the box. A gap where such a line bends from its target now bends all
      its lines 14 further on and is that much deeper (`graph::ARRIVE`), so the head
      stands after a curve of its own; a step the line is straightened by
      (`straight::SNAP`) asks for nothing. No crossing, jog or word more in
      `tests/lines`; three corners more stand near another line, and boxes move some
      2 % more when one is added
- [x] The file's own colors under a look (2026-11-19, from the review sheet of the
      three diagrams, F1): a box of a sequence (`box "Backend" #LightBlue`), a swimlane
      (`|#AntiqueWhite|Support|`), Mermaid's `rect rgb(…)` kept their light colors in a
      dark look, under words the look made light: they could not be read in dark,
      Omarchy, Gruvbox or Tokyo Night. Once a picture is drawn in a look whose paper
      lies on the other side of the middle than the file's (`omaspec_uml::repaint`,
      read from the look's block in the source), each text is measured against what
      lies under it (`omaspec_render::legible`); a ground the look does not write that
      reads below 4.5:1 keeps its hue in OKLab and takes the lightness as far from the
      look's paper as it was from the file's, or the nearest at which every text on
      it reads; else the file's words turn, and a line of its own that reads below
      3:1 on the paper. A color turns wherever it stands, so one box keeps one
      color; what reads stays, a dark box with white words of its own. A picture
      without such a color is not measured (half a millisecond to look); one with
      them takes some 18 ms more. The gallery's legibility test reads five diagrams
      with colors of their own in every bundled theme too: without the turn, 16 pairs
      read too faintly in the dark look alone. Mermaid's bars on a lifeline stay
      white in a dark look; that is the theme's color, not the file's (F2)
- [x] REQ-079 (2026-11-19): a line between a Mermaid subgraph and a node
      inside it (`Sub --> In`, `In --> Sub`) starts at the subgraph's
      title, as a state's line to a state inside it does (REQ-072). It was
      a stub of eight pixels from the frame's nearest border, which lay
      12 from the node. The subgraph keeps 20 more under its title, and
      the height of the line's words, so the line runs straight down from
      under it, or with a step across where the node stands to one side.
      Where another node stands before it, the subgraph keeps a lane of 12
      more on its right, and the line runs from the title's end down it
      into the node's side, as far from the nodes as from the border; a
      subgraph laid out with what is around it keeps the lane when it
      holds more than one node. Else `graph::across` routes it, and only
      where that finds no way the line runs from the nearest border. The
      router (`graph/route.rs`) also charges 12 a unit for a run on a
      box's border, beside the 3 for running close to it: in a gap of 12
      between a box and a frame it took the box's border
- [x] The flicker in the window (2026-11-19): with the focus on a box, the
      picture was gone for one frame as the pointer came over a box that
      folds (eleven times in 46 seconds in the tester's recording, since
      2026-11-17). Gone in the window by 2026-11-19, as the user found;
      `OMASPEC_DRAWS=1` still says on standard error what the diagram's
      canvas draws in each frame, and `a_zoom_never_shows_the_paper_alone`
      draws frame by frame with a pointer.
- [x] REQ-076, the rest (2026-11-18): a region without a start of its
      own begins in the one state in it that has one and that no
      transition enters from outside (`Machine::region_begins_in`), as
      the machine does at its top. A play entered the state around such
      regions as a whole and offered only its own transitions, so it
      stood half way. Lint and the scenario check already took such a
      region as entered.
- [x] REQ-077, the hint (2026-11-18): lint's `embedded-diagram` says on
      the line that opens a diagram inside a diagram (`{{`, or `{{wbs`,
      where a later line closes it) that Omaspec writes it as its text
      and the Java PlantUML draws it.
- [x] REQ-077, the picture (2026-11-19): a diagram inside a diagram is
      drawn by the engine (`embedded.rs`) and set in its text as a picture
      on a line of its own (`Picture::Diagram`, a nested `<svg>`), in a
      note, a legend or any text of several lines, of every kind: `{{` is
      an `@startuml`, `{{wbs`, `{{json`, `{{salt` and the rest their own
      kind. It is drawn in the outer diagram's skinparams, so a look laid
      over the outer one (the Omarchy colors, a skin pack) colors it too,
      and without them where the engine does not take them; its background
      is left out, and so are the ids of its groups. Three diagrams deep
      the next one stays text, and a thread keeps the last 64 pictures. An
      `end note` or `endlegend` inside it is its own: the block around it
      ends after `}}`, also for the lines that decide the type. Where the
      engine does not draw it (`{{dot`), the lines stay text and lint's
      `embedded-diagram` says why
- [x] A state machine's play lights each state in its own shape
      (2026-11-18): the state's outline as its picture draws it
      (`focus::Outline`, the one the focus lets a box through in) is what
      the veil opens along, what the glow hugs and what the ripple
      spreads from (`play::Glow::Own`), a state drawn by hand with its
      bowed sides and its rim, a choice as its diamond. A start, an end
      and a bar keep the shape the map knows, as does a picture without
      parts.

### After the security review (2026-11-21)

An outside review of the sandbox, the sanitizer, `!include`, MCP, Git and
the size limits found three openings and a fence that did not say where it
was open.

- [x] `xml:base` goes in the sanitizer: of the XML namespace's attributes
      only `xml:space` and `xml:lang` stay. A base on the `<svg>` sent the
      relative project links it lets through to another host. The fuzz
      target reads the elements and attributes of what it lets through,
      so words in the picture that say `xml:base` or `javascript:` are no
      finding.
- [x] What the sanitizer lets through draws within a thread's 2 MiB of
      stack. The review asked for a depth in the sanitizer's writer, which
      calls itself for each level. The first crash came earlier, in
      roxmltree 0.21.1's tokenizer (some 150 levels in a debug build);
      0.21.2 reads nested elements without a call for each, and every
      crate asks for it now. A second review found the rest in usvg, which
      follows references in calls too: some 300 `<use>` in a row, or
      chained clip paths, masks, markers and pattern fills, ended a debug
      build, as did 1,000 levels, below usvg's own limit of 1,024. The
      sanitizer now counts as it writes: elements nest 64 deep at most
      (`MAX_DEPTH`), references lead on from element to element 8 times
      in a row at most and never round in a circle (`MAX_CHAIN`, each
      element's references kept for the elements with an id around it),
      and a stylesheet keeps no `url(#…)`, since a rule may lay it on any
      element. Omaspec's own SVGs and PlantUML's hold no such reference
      at all. A test draws the worst picture these let through, a chain
      as long as may be with every link as deep as may be, of each kind,
      on a 2 MiB thread. The Java PlantUML's answer is sanitized before
      the repair of its colors reads it, and so are the three other ways
      a picture is drawn.
- [x] PlantUML's answer to one diagram ends at 64 MiB (`MAX_ANSWER`):
      the reader counts as it reads, a line too, and the process is
      stopped as a hang is. The stand-in tests one endless line and
      endless lines.
- [x] A fence that holds in part says so. The sandbox writes what
      Landlock lacks on this kernel as its first line on standard error
      (`omaspec-sandbox:`, then the rules by ABI: truncating files, TCP,
      device ioctls, signals, abstract Unix sockets; or no Landlock at
      all), before it becomes PlantUML. That line went to `/dev/null`
      before, so not even a missing Landlock showed. The Java bridge reads
      it, drains the rest, and hands it on as `Rendered::fence`: the
      status line turns yellow, `render` warns (also when the picture goes
      to standard output) and its JSON and the MCP server's answer carry
      `fence`. A sandbox that cannot start ends the drawing with what it
      said.

## Other desktops, and a face for the mouse (2026-11-08)

- [x] The engine and the binary type-check for Windows (2026-11-08): `cargo check
      --target x86_64-pc-windows-gnu` passes for `omaspec-core`, `omaspec-uml`,
      `omaspec-model`, `omaspec-render` and `omaspec-app`. The Linux parts sit
      behind `cfg`: the Landlock sandbox of the Java PlantUML (`sandbox.rs`, the
      crate, `__sandbox`) is Linux-only and `render_config` asks for it only
      there; Unix modes (the scratch folder, the session file, a hook file) are
      set on Unix and not elsewhere (`files::only_owner`); the Wayland app id is
      `ui::app_id`. The benches do not check for Windows: criterion's `alloca`
      wants a C compiler for that target.
- [x] A context menu, a menu bar and a small toolbar (2026-11-08,
      [0025](adr/0025-menus-for-the-mouse.md)): the right mouse
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
