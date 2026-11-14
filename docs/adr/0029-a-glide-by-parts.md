# 0029: A glide draws each part from a picture of its own

Status: accepted, 2026-11-14. Replaces how a glide takes its boxes in the
addendum to [0024](0024-focus-and-collapse.md); the lines keep gliding as
written there.

## Context

When a picture changes its layout, after an edit or a fold, its boxes glide
to their new places (`paper::morph`). Each box was cut out of the picture as
a rectangle and drawn at its place of the moment. A picture does not say
what in it belongs to which box, so the glide guessed:

- what is a box, and where its rim runs, from the outline of the shape
  nearest its rectangle (`pick::Fills::bend`): a hand-drawn side, a step in
  a side, a box drawn only as its outline each needed a rule of their own;
- what a container holds, by rectangles inside it: their holes in its
  pixels were filled from a column beside them that is one color, else
  from its fill as the picture names it;
- what a line's words are: they lie in the picture of the box around them,
  and stayed in a folding container until its inside was faded by hand.

Each guess had cases it got wrong, and each fix added the next guess. Two
reviews of a fold's glide found fifteen such cases each, most of them in
the fixes of the one before. The engine knows all of it: which shapes and
words it draws for which state, which words belong to which line.

## Decision

1. **The engine marks the parts it draws.** While it draws a box, a mark
   (a start, an end, a choice) or the words of a line, everything it draws
   on each of its layers lies in a group of that part,
   `<g id="part-…">`, the part's name written in hex. A box is named by its
   key, as the map names it; a mark and a line's words by where they are in
   the diagram's tree, and the map's `Spot` and `Wire` carry that name. A
   container's group holds its rim and its title, never what it holds.
2. **Only for the pane.** The pane asks for the parts with a line after
   the diagram, `' parts (Omaspec)` (`%%` in Mermaid), as it asks for a
   fold. A render for a file or an export draws no part groups: the
   pictures and the golden SVGs stay as they are.
3. **A picture of each part.** For a glide the pane draws, in the
   background, the picture without its lines and its parts (the ground),
   and each part on its own (`resvg::render_node` on its groups), at the
   scale of the overview. A box glides as its own picture: nothing of
   another box is in it, nothing has to be cut out of it, filled or faded.
   A box that changes its shape is still drawn in parts, its corners kept
   ([`paper::sprite`](../../crates/omaspec-app/src/paper/sprite.rs)); its
   rim comes from its own outline.
4. **Words go with their lines.** A line's words are a part of their own
   and travel with the line (`paper::wires`): from their old place to their
   new one, or away and in with the line when it goes or comes.
5. **One drawer at a time.** State diagrams of both languages mark their
   parts first, then structure, flowcharts and trees. A picture without
   parts glides as it did, by cutting; once every drawer that glides marks
   its parts, the cutting and its guesses go: the box test of `pick`, the
   columns and swatches of `sprite`, the faded inside of a folding
   container.

## Why

The engine has the answer the glide was guessing; asking it for the answer
removes a kind of bug rather than one bug. Groups with ids are what an SVG
has for this, `sanitize` keeps them, and resvg draws a group alone. A
picture of each part costs about one more rendering of the diagram, in the
background, once a glide.

## Not now

Shadows (`skinparam shadowing true`) lie in a layer of their own under the
shapes and stay with the ground while their boxes glide. The sketch look
keeps drawing its lines straight while they glide.
