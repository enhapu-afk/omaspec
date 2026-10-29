<!-- Draft for the release announcement (Omarchy community, r/omarchy, AUR
comments). Adjust version, links and screenshots before posting. -->

# Omagram X.Y: a PlantUML and Mermaid editor for Omarchy that understands behavior

Omagram is a native diagram editor for Omarchy: the source on the left, the live
diagram on the right, in your Omarchy theme and font. It is pure Rust, one binary that
draws with its own engines: no Java, no browser, no network, and it sends nothing
anywhere.

![Omagram](https://raw.githubusercontent.com/enhapu-afk/omagram/main/docs/images/editor.png)

## What PlantUML and Mermaid cannot do

Omagram reads a state diagram as the state machine it specifies, and a sequence
diagram as a scenario, in PlantUML and Mermaid alike:

- **Checks while you type.** A state no path reaches, a state nothing leaves, one event
  leading two ways with no guard to choose. A sequence diagram is checked against the
  state machines of its folder: every message must be an event its receiver can take,
  in the order the scenario sends them.
- **Play it.** F5 runs the diagram on screen: the states the machine is in light up, you
  pick what happens next, a light runs along each transition or message.
- **Hold a recorded trace against it.** `omagram trace run.jsonl` plays what a running
  system logged through its state machines and says where it deviated, with the time.
  The editor plays the trace in the diagram, line by line.
- **What changed in the behavior.** `omagram diff` compares two versions, or a file
  with its last commit, by states, transitions and messages, not by lines; layout and
  colors do not count.
- **Write it in the other language.** `omagram convert` turns a PlantUML machine or
  scenario into Mermaid and back, and only when the result means the same.
- **For AI agents.** `omagram mcp` is an MCP server with the command line's tools:
  lint, trace, diff, convert, format, render and more.

## The editor

- Lint findings while you type; outline; Ctrl+click in the diagram jumps to the line.
- Compare a diagram with another tab or with any git commit, as pictures or changed lines.
- The diagram in its own window for a second monitor; a preview window for Neovim users.
- Follows `omarchy theme` and `omarchy font set` live; Hyprland snippet, menu entry and
  a themed template included.
- Exports SVG, PNG and PDF with the source inside, so an image opens as a diagram again.
- Keyboard, mouse and touchpad alike.

## Drawing

Omagram draws most diagrams with its own engines, in a few milliseconds: PlantUML
sequence, activity, state, class, component, use case, deployment, object, timing and
Gantt diagrams, mind maps, WBS, JSON and formulas, and Mermaid flowcharts, state and
sequence diagrams. The other Mermaid types are drawn by mmdr, a Rust library, in the same
process. A few PlantUML types (YAML, Salt and more) it does not draw yet and says so; if you
install the optional `plantuml` package (Java), Omagram hands just those to it,
confined with Landlock. Omagram reads state and sequence diagrams as PlantUML 1.2026.8
does; a test holds the two readings against each other.

## Status

Pre-alpha: well tested (unit and golden tests, the comparison with PlantUML, fuzzing),
but little used outside its tests so far, so you will find rough edges. Issues, and
diagrams it draws wrong, are very welcome.

Install: `yay -S omagram-git` (the newest commit); `omagram` and `omagram-bin`
follow with the first signed release. Source, issues and the roadmap:
https://github.com/enhapu-afk/omagram
