# 0036: Between agents and the people who steer them

Status: accepted, 2026-11-24. Narrows the plan of 2026-11-21 to draw
every example of PlantUML's guide natively (ROADMAP); the goals of
[0006](0006-goals-and-scope.md) and the scope of
[0007](0007-behavior-specifications.md) stay.

## Context

More and more code is written by agents. What is left to a person is to
say what should happen, to check that it does, and, where several agents
work at once, to see what they are doing and where they wait for a
decision. Diagrams that serve these three keep their worth or gain it:

- **Behavior** (state machines, scenarios, activities and flowcharts):
  the contract an agent implements, the test a run is held against, the
  workflow of the agents themselves with the places a person decides.
- **Boundaries** (C4, components) and **data** (entities and their
  relations): what an agent must not cross, and decisions that are dear
  to undo.
- **Structure written by hand** (classes, objects, deployment) loses its
  worth as a design kept up to date: the agent reads the code, and the
  picture is stale after its first run. It stays worth drawing as a view
  an agent writes to explain code, which agents do all the time.

Omaspec already has much of what an agent needs: `lint --json`, the MCP
server ([0008](0008-mcp-server.md)), `render` as a picture or as text,
`trace` and `coverage`, `diff` by behavior, plays of recorded traffic.
They were built one at a time, each beside the window. This decision
makes the agent a user of its own, and the loop between agent and person
the order of the work.

## Decision

An agent is a first-class user of Omaspec, beside the person. A feature
is designed for both, and the work goes first to the loops between them:

1. **Specification, code, trace, finding.** A state machine, a scenario
   or a workflow is the contract. The agent that implements it writes a
   trace of what the system did, in Omaspec's JSON Lines; `omaspec
   trace` (and MCP `trace`) holds it against the contract; a finding
   comes back as a task: what the diagram expected, what happened, on
   which line of the diagram and of the trace. The agent fixes the code
   or proposes a change to the diagram; the person sees both in the
   picture. The trace format is a contract of its own: written down,
   with a schema and a version, so an agent can emit it without reading
   Omaspec's code.
2. **A run, live.** An orchestrator, or a hook of an agent's client,
   appends events to a file as its agents work. Omaspec follows the file
   as it grows: the workflow lit where each agent or task stands, what
   waits, where a person must decide, a deviation the moment it happens.
   The file is on disk; nothing goes over a network.
3. **Review by behavior.** What an agent changed in a specification is
   shown as behavior, not as lines of text: `omaspec diff` as a picture
   and as text, fit for a pull request, and for the agent itself to check
   before it hands its work over.
4. **Findings an agent acts on alone.** Every finding has a stable code,
   its line, and a fix where one can be said; every command an agent
   needs is an MCP tool; output is deterministic; `render --format text`
   lets an agent read what the person sees.

## Consequences

- PlantUML's guide is drawn natively next only where agents and people
  write it most: class, component and C4, use case. Object, deployment,
  timing's own constructs, Salt, Ditaa and charts stay with the Java
  PlantUML until the loops above stand. The guide stays the yardstick for
  what is drawn.
- The roadmap gets a section of its own for the loops, taken in the order
  above, each with how it is checked.
- The gallery gets examples of agents at work: an orchestrator's workflow
  with the places a person decides, an agent's life cycle, a tool call as
  a scenario.
- `AGENTS.md` and the guide say how an agent uses Omaspec in each loop,
  not only which commands exist.

## Not covered

- Omaspec reads no program code. An agent writes the diagram from the
  code; Omaspec checks and draws it.
- Omaspec runs no agents. It shows and checks what they do; it is no
  orchestrator.
- No network, no account, no server ([0005](0005-offline-no-telemetry.md)):
  a live run is a file, an agent speaks to Omaspec through the command
  line and MCP over standard input and output.
- Guards are still not evaluated; a finding errs towards a machine that
  can do more, so it is sure.

## Why

The demand is for agent workflows made visible, and for a way to trust
what agents built without reading all of it. A picture alone is a
screenshot; Omaspec's worth is that its pictures are checked: a diagram it
draws is also a contract it can hold a run against. Put between the agent
and the person, that turns a diagram from documentation into the place
where both agree on what the system does.
