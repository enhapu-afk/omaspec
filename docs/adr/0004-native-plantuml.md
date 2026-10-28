# 0004: Native PlantUML engine

Status: accepted, 2026-10-15

## Decision

Omagram gets its own PlantUML engine in Rust, `omagram-uml`, built one
diagram type at a time. The Java PlantUML (`plantuml` package, needs a JRE)
stays an optional fallback for users and the reference in CI.

## Why

There is no Rust PlantUML implementation. A full port is not realistic:
PlantUML has been developed for over 15 years, has about 25 diagram types,
a preprocessor that is a language of its own, skinparams, styles, Creole and
sprites. A subset is realistic, because PlantUML lays out several of the most
used types itself, without Graphviz: sequence, activity, mindmap, WBS, timing,
gantt.

A native engine also gives:

- exact source spans for every drawn element, so Ctrl+click jumps to the
  right line without heuristics;
- lint and outline from the same syntax tree as rendering;
- renders in milliseconds, no JVM, no external process to sandbox;
- Omarchy colors applied directly.

## Rules

- **Syntax compatible, not pixel identical.** Diagrams look like PlantUML,
  drawn cleanly; nobody should have to rewrite a `.puml` file.
- **Decline rather than draw wrong.** The engine renders a diagram only when
  it understands every construct in it. Otherwise it names the line and the
  construct; the caller then uses the Java PlantUML if installed.
- **Java as the oracle.** CI compares structure against the Java
  PlantUML's reading, for the behavior types: a state machine against its
  SCXML export, a scenario against its XMI (participants and their order,
  messages, groups). Its SVG says which type PlantUML took a diagram for.
  The other types have no export to compare with and are not held.
- **No font files.** `omagram-uml` does no I/O, so it measures text with
  built-in advance widths of the Liberation fonts, which share Arial's and
  Times' metrics (and are `sans-serif` and `serif` on Omarchy). Every run of
  text carries its measured `textLength`, so the layout holds with another
  font too, as PlantUML's own SVGs do.
- **Skinparams decline too.** One that would change the diagram but is not
  drawn yet declines it like an unknown construct; those for other diagram
  types are ignored. Mind maps and WBS diagrams take their look from
  PlantUML's styles only, so for them that means the skinparams PlantUML
  turns into styles that reach them (`Node…`, `Arrow…`, `default…`), where
  a later one wins over an earlier one.

## Order

| Step | Types | Layout |
|---|---|---|
| N1 | sequence | own, linear |
| N2 | mindmap, WBS | tree |
| N3 | activity (new syntax, swimlanes) | own block layout |
| N4 | state | layered graph, nested states |
| N5 | class, component, use case, deployment, object | layered graph from N4 |
| N6 | timing, gantt | own |

Not planned: JSON/YAML, salt, ditaa, math, the standard library includes,
sprites. They stay with the Java fallback.

The preprocessor grows in steps: comments, variables, `!define`, `!if`,
`!include` of local files inside the project folder; functions and
procedures later.

## Alternatives

- **GraalVM native image of PlantUML**: no JVM at run time, but not Rust,
  no official build, needs a Swing-capable GraalVM, still needs Graphviz.
- **The TeaVM build in an embedded JS engine**: brings JavaScript back; slow
  without a JIT or heavy with V8.
