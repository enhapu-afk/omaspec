# 0031: A presentation is one HTML page that plays what the pane plays

Status: accepted, 2026-11-15.

## Context

A diagram that is shown to others is stepped through: a scenario message
by message, a state machine option by option, as the pane plays it (F5).
Shown from Omaspec, that needs Omaspec on the machine at the front of the
room. The roadmap's *Presenting, animating, and diagrams only Omaspec
draws* asks for a page that needs nothing but a browser, offline, and
names it the foundation of the pages after it (a fold as vectors, Orbit,
the project as a page).

## Decision

1. **One file, nothing fetched.** *Export as › Presentation…* and
   `omaspec render FILE -o talk.html` write one HTML page: the picture as
   an export draws it (the look chosen, its links laid over it as
   `sanitize_with` keeps them), the play as data, a small script and its
   style, all inside. A Content-Security-Policy in the page allows no
   request: no script, style, font or picture from anywhere, inline ones
   alone. The source goes with it as an SVG export keeps it, in the
   picture's `<metadata>`, unchanged; `omaspec extract` reads it from the
   page. The play's data is JSON in which every `<` is `\u003c`, so no name
   can end its element.
2. **Rust plays, the page only walks.** What a play does is worked out
   where the pane works it out: a scenario's steps are the map's messages
   (where each one is, the box grounds under them); a state machine's
   places are `omaspec_model::play::Graph`, every run its start can reach
   (the first 256), each with the states it is in, named as the play bar
   names them, its options as the bar says them, the place each leads to
   and the transitions it takes in beats: the steps of one beat (a fork's
   arms, the starts of a state's regions) light at once, as in the pane.
   Each transition's route is written once.
   The page's script has no reading of PlantUML or Mermaid and no rules of
   a machine; it steps through the table. Nothing is decided twice.
3. **The pane's light.** The picture lies on white, as the pane shows
   it. A scenario veils what is still to come in the paper's color at the
   pane's strength with the pane's soft edge, a box in its own fill
   instead; the message played is lit, a light runs along its arrow, the
   ones before it are quiet. A state machine veils the diagram but the
   states it is in, a choice or a join too, which shine in the accent; a
   light runs along each transition taken. The veil and the lights take
   no clicks: a link in the picture still opens.
4. **Keys, as a presenter's remote sends them.** →, ↓, Space, PageDown,
   Enter and a click go on (a machine's first option); ←, ↑, PageUp and
   Backspace go back; Home starts again; 1 to 9 take a machine's option;
   F shows the page full screen. A machine's options stand under the
   picture as the play bar shows them.
5. **The colors come with the look.** The veil is the paper of the look
   the picture is drawn in, the light its accent: what the pane would
   show in that look. *As shown* takes the window's, as the pane plays.

## Consequences

A presentation plays the same steps as the pane, from the same reading,
and stays right when the play does. A machine with more than 256 places
offers the first of them; an option past them is shown but leads nowhere.
The page is as large as its picture and a few kilobytes more. Its script
is tested by the data it is given (`present` in `omaspec-render`) and,
where Node is installed, by running it.
