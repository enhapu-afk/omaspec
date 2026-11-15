# 0030: How a new picture comes on screen is a state machine of Omaspec's

Status: accepted, 2026-11-15.

## Context

A new picture comes on screen in steps: the diagram is drawn, the picture
on screen stays until the new one's overview is rasterized, then the two
fade into each other, or, when boxes moved, the sheets of parts of both
pictures are drawn and the boxes glide (0029). A newer diagram can come at
any of these steps. The pane held where it was in five fields that each
step set and read: `fresh` (a new diagram is drawn), `landed` (its overview
is in), `no_bare` (a sheet could not be drawn), `previous` (the pictures it
comes from) and `fade` (what moves on screen), with a rule for each pair of
them. The review of 0029's last step found one rule that read a field
before another step had set it: a fold that fell back to a fade showed its
buttons over the picture still fading.

## Decision

1. **One enum, the states of a diagram.** `paper::arrival::Arrival` is
   `Still`, `Coming` (with the pictures it comes from), `Sheets` (the same,
   its sheets on their way), `Fading` (with the fade) and `Gliding` (with
   the glide, and a newer diagram that waits for its end: `Alone`,
   `Following`, `Waiting`). What a step needs travels in its state: there
   are no pictures to come from without a diagram coming, and no sheets
   asked for without pictures to glide from.
2. **The diagram is beside the code.** `paper/arrival.puml` is the machine,
   its events the ones the pane takes (`drawn`, `overview`, `no overview`,
   `sheet`, `no sheet`, `faded`, `released`), its guards what the pane
   decides. The pane moves on only through `Paper::arrive`.
3. **Omaspec checks it.** In the tests every move is written to a trace
   and held against the diagram by Omaspec's own trace check
   (`omaspec_model::check_trace`) as it is made; a move the diagram does
   not have fails the test that made it, with the check's own message. A
   tour walks every transition, and Omaspec's coverage of the walks must
   be the whole diagram; a test holds the diagram's states and events to
   the enum's. `OMASPEC_ARRIVAL_TRACES=FILE` gathers the traces of every
   test, for `omaspec coverage paper/arrival.puml --traces FILE`.
4. **A short fade gives way at once.** A diagram drawn while one picture
   fades into another ends that fade and holds the picture it faded to;
   the next fade starts from that picture, as it did before. A glide runs
   to its end. So no fade runs beside a diagram that is coming, and the
   machine needs no second region.

## Why

A dozen rules over five fields is the kind of code a state machine
replaces, and Omaspec is a tool for state machines: the pane's own
behavior is a diagram that can be read, played and checked like a user's.
The check costs nothing at run time; in the tests it reads a trace of a
few dozen lines.

## Not now

The play (`paper::play`) and the passage into a link (`paper::passage`)
keep their own fields; each is one animation with a start and an end.
