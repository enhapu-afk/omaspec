# Traces

A trace is what a running system did, written down as it happens, so that
`omaspec trace` can hold it against the state machines that specify the
system. This page is the format's contract, version 1: what a program, or
an agent implementing a machine, writes, and what comes back
([0036](adr/0036-between-agents-and-people.md)).

## A line

A trace is JSON Lines: one JSON object a line, in the order things
happened. `omaspec trace --schema` prints the JSON Schema of a line.

| Field     | Required                 | Says                                                                 |
| --------- | ------------------------ | -------------------------------------------------------------------- |
| `object`  | yes                      | who did it; it goes by the state machine named like it              |
| `event`   | `event` or `state`       | the event it took, as the machine writes it, without guard or action |
| `state`   | `event` or `state`       | the state it is in afterwards, by the name transitions use          |
| `machine` | no                       | the machine it goes by, where the object is called otherwise        |
| `time`    | no                       | when, as the system writes it; findings name it                     |

Text or a number in each; `null` counts as absent; other fields are left
alone, so a log line can carry what else the system wants to keep.

```text
{"time":"10:30:01.123","object":"Door","event":"unlock"}
{"time":"10:30:02.400","object":"Door","event":"open","state":"Open"}
{"time":"10:30:03.010","object":"back door","machine":"Door","state":"Locked"}
```

Many objects may share one machine (`"machine":"Door"`), and one trace may
hold many machines' objects, their lines mixed.

## How a line is held

- An object begins where its machine starts, or, when its first line
  gives a state and no event, in that state. Its first event may be the
  label of the one way out of the start (`[*] --> Idle : power`, then
  `{"event":"power"}`): the line says it started, where the state it
  starts in does not take that event itself; a play takes it so too.
- An event is taken from any state the object can be in by then; the
  regions of a composite state that have a way for it take it at once.
- What a person picks in a play, the way out of a choice or a state's way
  without an event, the system may take without writing a line. So the
  object may be wherever any of those leads, and a finding is sure: the
  check errs towards a machine that can do more.
- Guards are not evaluated.
- A `state` says where the object is; one it cannot be in by then is a
  deviation, and it goes on from there.
- After an event it cannot take, it is followed again from the next line
  that gives its state.

## What comes back

`omaspec trace run.jsonl` writes `file:line: severity: message`, one
finding a line, and exits with 1 on a deviation. `--json`, and the MCP
server's `trace`, give each finding as an object. A deviation
(`trace-deviation`) carries a `task`: what the line says happened, the
states the object could be in and the events they take, each with the line
of the diagram it is written on. Lines count from 1.

```json
{
  "file": "run.jsonl",
  "code": "trace-deviation",
  "line": 2,
  "severity": "error",
  "message": "At 10:00:02, Door does not take lock here: by now it is in Open, and it has no transition for lock (door.puml). …",
  "task": {
    "object": "Door",
    "machine": "door.puml",
    "happened": { "event": "lock", "time": "10:00:02" },
    "expected": {
      "states": [{ "name": "Open", "line": 3 }],
      "events": [{ "event": "close", "to": "Closed", "line": 4 }]
    }
  }
}
```

The other codes: `trace-line`, a line that is no event (not JSON, no
object, neither event nor state); `trace-unchecked`, an object no machine
of the project goes by; `trace-no-start`, a machine without a start, so
its object is followed from a line that gives its state;
`trace-too-many-ways`, an object that could be in too many ways at once to
follow.

## Following a run

`omaspec trace run.jsonl --follow` holds the trace while it is written:
each finding the moment its line is complete, with `--json` one object a
line, so an orchestrator or an agent reads them as they come. A file not
there yet is waited for, a line half written waits for its end, a file
written anew is followed from its start again; it runs until it is
stopped. `tail -f`-like on standard input too (`--follow -`), until the
input ends, with the exit code of a whole check.

`omaspec preview flow.puml --follow run.jsonl` shows the same on the
machine, for the person watching: each object a pill with its name beside
the state it is in, past three a pill that counts the rest; a light runs
along each transition a new line takes; an object whose last line
deviates stands in red, and the bar says why. Esc stops following. In
the editor, *Follow a trace…* (Diagram) follows one on the tab's machine.

A state where a person decides, not the system or its agents, is
`state Review <<decide>>` in PlantUML and `class Review decide` in
Mermaid (`omaspec convert` writes each as the other). An object standing
there waits for that person: its pill is filled, `‖ pr-42`, and the bar
counts who waits. The gallery's *Agents at work* has two such places
and a run to follow: `omaspec preview agents.puml --follow
agents.jsonl` in `examples/gallery`.

## Writing one

Append a line where the system takes an event, and flush it, so a run that
stops halfway leaves what it did:

```python
log.write(json.dumps({"object": "Door", "event": "open", "state": door.state}) + "\n")
```

```rust
writeln!(log, "{}", serde_json::json!({"object": "Door", "event": "open", "state": state}))?;
```

Giving `state` as well as `event` lets the check tell an event taken in
the wrong state from a state the code reached the wrong way.

## For an agent

An agent that implements a machine writes the trace with the code, runs
the system or its tests, and holds the trace against the machine
(`omaspec trace run.jsonl --json`, or MCP `trace`). For each task:

- the code took `happened.event` where the machine offers only
  `expected.events`: the code is wrong, and the lines of `expected` say
  what the diagram asks there;
- or the machine lacks a transition the system needs: that is a change to
  the specification, which the agent proposes and a person decides, drawn
  with `omaspec diff`.

Then it runs again until the trace holds.
