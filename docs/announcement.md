<!-- The release announcement for r/omarchy and the Omarchy community. Before
posting: the version in the title, and the install section once the AUR takes
new accounts again (then it is one line: yay -S omaspec-bin). -->

# Omaspec 0.4.4: a PlantUML and Mermaid editor for Omarchy that understands behavior

Omaspec is a native diagram editor for Omarchy: the source next to the live diagram, in
your Omarchy theme and font. It is pure Rust, one binary that draws with its own
engines: no Java, no browser, no network, and it sends nothing anywhere.

![Omaspec](https://raw.githubusercontent.com/enhapu-afk/omaspec/main/docs/images/hero.png)

## What PlantUML and Mermaid cannot do

Omaspec reads a state diagram as the state machine it specifies, and a sequence
diagram as a scenario, in PlantUML and Mermaid alike:

- **Checks while you type.** A state no path reaches, a state nothing leaves, one event
  leading two ways with no guard to choose. A sequence diagram is checked against the
  state machines of its folder: every message must be an event its receiver can take,
  in the order the scenario sends them.
- **Play it.** F5 runs the diagram on screen: the states the machine is in light up, you
  pick what happens next, a light runs along each transition or message.
  [See it play](https://raw.githubusercontent.com/enhapu-afk/omaspec/main/docs/images/play.gif).
- **Hold a recorded trace against it.** `omaspec trace run.jsonl` plays what a running
  system logged through its state machines and says where it deviated, with the time.
  The editor plays the trace in the diagram, line by line.
- **What changed in the behavior.** `omaspec diff` compares two versions, or a file
  with its last commit, by states, transitions and messages, not by lines; layout and
  colors do not count.
- **Write it in the other language.** `omaspec convert` turns a PlantUML machine or
  scenario into Mermaid and back, and only when the result means the same.
- **For AI agents.** `omaspec mcp` is an MCP server with the command line's tools:
  lint, trace, diff, convert, format, render and more.

## The editor

- Lint findings while you type; outline; Ctrl+click in the diagram jumps to the line.
- Compare a diagram with another tab or with any git commit, as pictures or changed lines.
- The diagram in its own window for a second monitor; a preview window for Neovim users.
- Follows `omarchy theme` and `omarchy font set` live; a Hyprland binding, a menu entry
  and themed colors included.
- Exports SVG, PNG and PDF with the source inside, so an image opens as a diagram again.
- 24 examples to start from (Ctrl+Shift+E), and every command in a palette
  (Ctrl+Shift+P).

## Drawing

Omaspec draws most diagrams with its own engines, in a few milliseconds: PlantUML
sequence, activity, state, class, component, use case, deployment, object, timing and
Gantt diagrams, mind maps, WBS, JSON and formulas, and Mermaid flowcharts, state and
sequence diagrams. The other Mermaid types are drawn by mmdr, a Rust library, in the same
process. A few PlantUML types (YAML, Salt and more) it does not draw yet and says so; if you
install the optional `plantuml` package (Java), Omaspec hands just those to it,
confined with Landlock. Omaspec reads state and sequence diagrams as PlantUML 1.2026.8
does; a test holds the two readings against each other.

Large diagrams too: a delivery drone with 45 states up to four levels deep, two regions
side by side and 72 transitions, drawn in about 30 ms
([its source](https://github.com/enhapu-afk/omaspec/blob/main/examples/drone/delivery-drone.puml)).

![A large state machine: the flight of a delivery drone with two regions side by side, and its delivery](https://raw.githubusercontent.com/enhapu-afk/omaspec/main/docs/images/drone.png)

## Install

Omaspec is not in the AUR yet, because the AUR takes no new accounts at the moment.
Until it does, build the package from the repository:

```bash
git clone https://github.com/enhapu-afk/omaspec
cd omaspec/packaging/aur/omaspec-bin
makepkg -si
```

This installs the release binary with the desktop entry, the icon and the file types,
so file managers open `.puml` and `.mmd` files with Omaspec. On other distributions, take
the binary from the [release](https://github.com/enhapu-afk/omaspec/releases/latest).

The [getting started guide](https://github.com/enhapu-afk/omaspec/blob/main/docs/guide/README.md)
walks you through a state machine, a scenario and a recorded trace in ten minutes.

## What's next

- **Lines by hand.** Drag a line in the diagram, or a bend in it, to route it yourself;
  the diagram keeps your route.
- **Diagrams that link.** Click a state and the view zooms into it: inside is the
  activity diagram that says what the state does. A specification you can read from the
  whole system down to a single step.
- **Even cleaner lines.** Fewer crossings and detours, lines that keep clear of frames and
  labels, closer to what Graphviz draws.

## Status

Omaspec is young: well tested (unit and golden tests, a comparison with the Java
PlantUML, fuzzing), but little used outside its tests so far, so you will find rough
edges. Issues, diagrams it draws wrong, and wishes are very welcome:
https://github.com/enhapu-afk/omaspec/issues
