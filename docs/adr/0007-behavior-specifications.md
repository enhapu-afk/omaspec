# 0007: Behavior specifications, understood

Status: accepted, 2026-10-20. Replaces the scope of
[0006](0006-goals-and-scope.md); its goals stay.

## Decision

Omagram is a tool for people who write specifications. Its core is
behavior: state machines, sequence diagrams, activity diagrams and
flowcharts, in PlantUML and in Mermaid. For these, Omagram does not only
draw the diagram, it understands it: it checks a specification, and later
lets it run and holds it against what a system did.

- **No new language.** PlantUML and Mermaid stay what people write. Many
  tools show them, much is written in them, and language models write
  them fluently. Underneath, `omagram-model` reads both into one
  behavior model: what a diagram means, not how it looks.
- **No race for coverage.** Omagram no longer aims to draw every type.
  Class, component, use case, deployment, object, timing and Gantt
  diagrams, mind maps and WBS stay as they are, drawn natively or through
  the fallback, and get no new work. In the behavior types, gaps close
  when real specifications need them.
- **Value first.** A new feature counts when it does what PlantUML and
  Mermaid cannot: check a specification, run it, compare it with a trace
  or with another version. Drawing more of PlantUML's constructs is not
  such a feature on its own.

## Order

1. Check state machines: states that cannot be reached, states with no way
   out, the same event leaving a state twice, composite states entered
   without a start. Transition labels are read as UML writes them:
   `event [guard] / action`.
2. Check diagrams against each other: a message in a sequence diagram that
   the receiver's state machine never takes.
3. Run a specification in the preview: fire events, step through a
   sequence.
4. Hold a recorded trace against the specification.

Then, as they are needed: an MCP server over the command line, conversion
between PlantUML and Mermaid, a semantic diff of two versions.

## Why

Diagrams are more and more written by language models, generated from
code or traced from running systems; the value moves from typing them to
checking them. PlantUML and Mermaid treat every diagram as a picture on its
own: nothing tells a writer that a state cannot be reached or that two
diagrams contradict each other. A native, millisecond engine for both
languages makes these checks possible while typing, without a JVM, a
browser or a network, which a specification from a company may not leave.

## Consequences

- Checks speak to specification writers: a finding names the state or
  event in the writer's words and says what to change. A contradiction
  (the same event leading to two states) is an error; what may be meant
  (a state with no way out) is a warning with the fix that makes it
  explicit.
- Findings of the model come with the lint findings, in the editor and in
  `omagram lint` (`file:line`, `--json`, exit codes), so agents use them
  the same way.
- `omagram-model` does no I/O and has no GUI dependency, like
  `omagram-core` and `omagram-uml`.
- The roadmap's track V holds this work; tracks N and M keep only what the
  behavior types need.
