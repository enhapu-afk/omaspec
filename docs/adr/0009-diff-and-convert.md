# 0009: Behavior compared, and written in the other language

Status: accepted, 2026-10-21. Follows [0007](0007-behavior-specifications.md).

## Decision

Two features stand on the behavior model of `omaspec-model`, which reads
PlantUML and Mermaid alike:

- **A semantic diff** (`omaspec_model::diff`) tells what changed in the
  behavior between two versions of a diagram, also between the languages.
  For a state machine: states new, removed, moved into another state or
  region, of another kind; transitions new, removed, with another target,
  other events, another guard or action. For a sequence diagram:
  participants, messages (another text, now a reply) and the blocks
  around them. Then what the specification checks find now, or no longer:
  a change that leaves a state unreachable says so. Layout is not
  behavior: moved or re-indented lines, colors, notes and shown labels
  change nothing.
- **A conversion** (`omaspec_model::convert`) writes a state machine or a
  sequence diagram in the other language, from the parse the checks use.
  The result is read back and compared with the original by the diff; if
  the two would not mean the same, nothing is written. Behavior the other
  language cannot say refuses, with its line: a history state in Mermaid,
  a `ref`, a `group` or a message from the edge in a Mermaid sequence. What
  it cannot say about the looks is left out and told as a notice: colors,
  skin parameters, notes above or below a state, the direction of a nested
  state, the shapes of participants, dividers and delays.

Both are in `omaspec diff` and `omaspec convert`, in the MCP server
([0008](0008-mcp-server.md)), and in the editor: Compare shows B as the
changes in behavior, and a command writes the tab in the other language in
a new tab.

## How

- States and participants go by their names, a start or an end by the
  state and region it sits in. Transitions are paired in rounds, from the
  closest match to the loosest (same ends and trigger; same source, events
  and guard; same ends and events; same source and events; same ends), so
  that a changed transition is told as one change, not as one removed and
  one added. Messages are aligned with the line diff's algorithm, and a
  message that kept its ends at its place changed.
- A finding that names a line (`first on line 12`) is the same finding on
  another line, so moving lines finds nothing new.
- The writers declare every state in its own container first, so that both
  readers make it there, and write transitions between containers last, at
  the top, where both languages take states from anywhere.
- A name the other language does not take as it is (`"Long wait"` in
  Mermaid, a participant with a space) becomes a plain id; its box or head
  still shows the name, and a notice says so.
- `@startuml Name` becomes the Mermaid title when there is none, so
  sequence diagrams still find the machine by its name.
- `omaspec diff FILE` compares with the version in the last commit
  (`--rev` for another); a revision that looks like an option is refused
  before git sees it. The exit code is 0 for the same behavior, 1 for a
  change, as `diff` does.

## Why

Specifications change all the time, and a line diff tells a reviewer which
lines moved, not what the system now does. A one-word change can make a
state unreachable; a reordered file changes nothing. The diff says which.
The conversion lets a specification live where it is read, Mermaid in a
README, PlantUML in a company wiki, without retyping it and without a
silent loss: what is written means the same, or is not written.

## Amended: a whole project (2026-11-05)

`omaspec diff DIR` compares a project with its version in a commit (`--rev`,
`HEAD` unless given) or with another folder, as a changelog of the
specification (ROADMAP, 0.8). The roadmap named it `--project REV`, but
`--project` already names the folder whose machines a file is checked
against; a folder given as the file is the project, as `check`, `render` and
`export` take one.

- **Both versions are read alike.** The folder as the project's index reads
  it, the commit through `git ls-tree` and one `git cat-file --batch`, under
  the same rules (hidden and dependency folders aside, 1 MB a file); git runs
  as Compare runs it, without filters or text conversions, and fetches
  nothing a partial clone is missing.
- **Diagrams are paired** by their files; a Markdown document's diagrams by
  their names, then their texts, then in order. One that left its file is
  found by its text, then its name, then its file's name, and told as moved.
- **Each version is checked against its own machines**, so a scenario that
  did not change tells what a changed machine no longer takes; a finding
  that only names a moved machine's new file is no change.
- A new diagram is told with what the checks find in it, a removed one with
  what they found; a diagram whose behavior is not compared (an activity
  diagram, a map) only says that its lines changed, blank lines and
  indentation aside. The exit code is 0 when nothing changed, as for a file.

Measured on the generated projects of the scale test, one machine changed:
2,001 diagrams compare in 186 ms, and their versions come out of git in
73 ms (release build).

`omaspec log FILE` tells the same for one diagram, commit by commit, newest
first: each commit that changed the file, as `git log --follow` finds it
across renames, compared as the project diff compares a diagram (so a
rename is told as moved, and a commit that only moved lines says its
behavior stayed the same). A sequence diagram is checked against the state
machines of its project as that commit and its parent had them, so a
finding belongs to the commit that caused it. One `git log --raw` names the
commits and their blobs, one `git cat-file --batch` reads them; a project
is read from git only for a sequence diagram, once per commit. Lines of the
version before a commit are named `file@commit^`, of the version after it
`file@commit`, as `git show` takes them.

`omaspec git setup` lets git itself compare diagrams by their behavior. The
project's `.gitattributes` gives the diagram files `diff=omaspec`, and the
repository's own configuration names `omaspec git textconv` for it: git
takes no command from a repository, so the attributes are committed and
each clone runs the setup once. The text conversion writes a version's
behavior one fact a line (`omaspec_model::behavior`): a state machine's
states and transitions sorted, as their order is layout, with the machine
checks' findings; a scenario's participants sorted and its messages and
blocks in their order, as that is behavior. `git diff`, `git log -p` and
`git show` then line-diff those, which tells less than `omaspec diff` (a
changed target is a transition removed and one added) but works in every
git tool that converts text; any other file goes through as it is, and
`--no-textconv` shows the lines. The command is `omaspec` where that is the
one on the path, else the whole path of the one that ran the setup.

In the editor, Compare (Ctrl+Shift+D) offers the project a tab is in since
`HEAD`, its five newest tags and the commits that changed its folder. The
project diff runs in the background, the open tabs as they are, and its
changes are a list to pick from, as the project search lists its hits:
filtered by their words and files, one picked opens its file where it
changed and compares the tab with the same commit, the file's version then
found where it lay (a moved file, a folder gone since).

## Not covered

- A renamed state or participant is one removed and one added.
- Descriptions, notes and styles are not compared.
- Activity diagrams and flowcharts are neither compared nor converted.
