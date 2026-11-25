# Omaspec in CI

[behavior-diff.yml](behavior-diff.yml) comments on a pull request with what it
changed in the behavior of the diagrams
([0036](../adr/0036-between-agents-and-people.md)). Copy it to
`.github/workflows/`; it downloads Omaspec's latest release and runs

```bash
omaspec diff . --rev origin/main --markdown
```

on the pull request's checkout: by diagram, each change with its file and
line (a state, a transition or a message that came, changed or went, a
finding of the specification checks that is new or resolved), and each state
machine that changed drawn as Mermaid below its changes, what came in green,
what changed in amber, which GitHub draws in the comment. The comment is
written once and edited on each push, found by its first line (`<!--
omaspec: behavior changes -->`), so another workflow's comment is left
alone. It runs on each push to the pull request, also one that touches no
diagram. Nothing is posted where no diagram changed in its behavior (exit
0); a comment posted before says so once a later push takes the change back.
The job's summary shows the same, also for a pull request from a fork, on
which GitHub's token may not comment.

Omaspec's own pull requests get it too
([behavior.yml](../../.github/workflows/behavior.yml)), with Omaspec built
from the pull request, so that a change to `diff` is reviewed by itself; a
test holds its steps to this file's.

The same Markdown is what an agent can put in the description of a pull
request it opens: `omaspec diff . --markdown` before it hands its work over.

`omaspec check DIR --github` writes the findings of the whole project as
annotations on the lines they are about.
