# 0029: A glide draws each part from a picture of its own

Status: accepted, 2026-11-14. Replaces how a glide takes its boxes in the
addendum to [0024](0024-focus-and-collapse.md); the lines keep gliding as
written there. Point 2, and how point 1 writes the names, are replaced
by [0037](0037-an-id-for-every-box-and-line.md) once its first step is
done.

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
   key (`state:Brewing`, `box:Pack`) or by where it is in the diagram's
   tree (`box:2.0`), a mark and a line's words by where they are
   (`mark:0`, `words:3:0`), the title and its kind by what they are
   (`frame:title`); each kind of name has its own prefix, so no box's
   words name another part. The map's `Spot`, `Place` and `Wire` carry
   the names. A container's group holds its rim and its title, never what
   it holds.
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
   rim and its fill come from its own outline, read from its part alone
   (`pick::fills_of`). The room a box gains is filled from a patch of
   its fill on the same sheet, so it is on the GPU with the sheet
   (2026-11-15). The sheet is mapped onto the screen by the size its
   ground's pixels cover, as the pane maps the overview, so the first
   frame lies on the picture before to the pixel and the last on the new
   one; `paper/tests/promises.rs` holds a glide to that.
4. **Words go with their lines.** A line's words are a part of their own
   and travel with the line (`paper::wires`): from their old place to their
   new one, or away and in with the line when it goes or comes.
5. **What no box takes glides on its own.** The title, header, caption,
   footer and legend (`frame:title` and so on) and the top card of a JSON
   diagram are parts without a place. One in both pictures glides from
   where it was to where it is; one in only one of them comes or goes with
   the ground. The main frame stays on the ground: around everything, its
   picture would be as large as the ground again.
6. **A card keeps its rows where they are.** A box whose rows are laid
   from its top left and whose size follows them, as a JSON card or a
   tree's box (`Place::fixed`), and a part of point 5, is drawn at its own
   size from the top left of its place of the moment and within it,
   rather than stretched: rows that are in both pictures lie on each
   other, and what it gains or loses is wiped in or out. Its old picture
   stays opaque to the end where the new one covers it (a card with a
   fill, or one that keeps its size); words that change fade from one to
   the other.
7. **One drawer at a time, then no cutting.** State diagrams of both
   languages marked their parts first, then structure, flowcharts and
   trees, activity diagrams on 2026-11-18 (their shapes named by kind
   and words, as their layout knows no names). Every drawer whose boxes glide marks them now, and the cutting
   and its guesses are gone: the holes cut into the picture, the columns
   that filled them, the faded inside of a folding container, the search
   of the whole picture for each box's outline. A glide runs only between
   two sheets of parts; a picture without parts (the Java engine's), or
   one too large for a sheet, fades into the other, as a recolor does.

## Why

The engine has the answer the glide was guessing; asking it for the answer
removes a kind of bug rather than one bug. Groups with ids are what an SVG
has for this, `sanitize` keeps them, and resvg draws a group alone. A
picture of each part costs about one more rendering of the diagram, in the
background, once a glide.

## Not now

The sketch look keeps drawing its lines straight while they glide.
