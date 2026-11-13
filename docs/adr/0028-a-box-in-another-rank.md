# 0028: A box held in another rank, by the page

Status: accepted, 2026-11-13. Extends [0021](0021-boxes-held-in-place.md).

## Context

[0021](0021-boxes-held-in-place.md) holds a box in its neighbor's rank,
right after or before it across the flow (`Paused after Running`). A
writer also wants a box one rank further down, under the box it belongs
to, or two boxes in one rank without saying which comes first. The
words for that are the page's: `below`, `above`, `right of`, `left of`,
`level with`. They have to meet the ranks the layout gives: a box ranked
under another by a line cannot also stand above it.

Graphviz answers this with an invisible edge, which ranks and orders its
two ends like any other and is not drawn, and with `rank=same`, which
ranks its boxes as one and leaves their order to the sweeps. PlantUML
writes the first as `a -[hidden]down-> b`, and the layout has drawn such
lines since N5.

## Decision

1. **The words go by the page.** In a flow that runs down, `D below C`
   puts `D` in a rank after `C`'s, `D above C` in a rank before it;
   `right of` and `left of` are `after` and `before`, in `C`'s rank. In
   a flow that runs to the right the two pairs change places: `right of`
   is a later rank, `below` right after `C` in its rank. `after` and
   `before` keep going across the flow, whichever way it runs, as 0021
   wrote them. A container with a direction of its own reads the words
   by its own. A flow that runs up or to the left (Mermaid's `BT`, `RL`)
   is laid down or to the right and mirrored after, so its words turn
   with it: `below` on the page is laid above.
2. **Another rank is an invisible line.** `D below C` is a line from `C`
   to `D` that ranks and orders the two and is not drawn, as `dot`'s
   invisible edge: `D` stands at least one rank past `C`, and as near
   as the other lines let it, and the sweeps pull it towards `C`. No
   line is drawn and no room is kept for it.
3. **Level, and the order left to the layout.** `D level with C` ranks
   the two as one, as a hold of 0021 does, and leaves their order to
   the sweeps.
4. **What does not hold is said, as before.** Beside the reasons of
   0021, an entry into another rank does not hold when the lines between
   the two rank them the other way round (`A --> B` and `A below B`):
   the lines keep their way, and the entry shows among the findings with
   the box standing where the layout puts it. Whether the lines lead from
   the one box to the other is asked before the invisible line goes in, so
   it never turns a line around, whichever box was named first.
5. **One writer, one reader.** The block reads every word and writes it
   back as written; a name that is one of the words (`right`, `level`)
   is written in double quotes, and read bare too, since the word always
   stands second. A drag still writes `after` or `before`,
   replacing whatever entry the box had.

## Why

An invisible line over a fixed offset: the rank a box takes is the
layout's to give, and a line asks for no more than the writer means,
*under it*, while the layout keeps every other line short. By the page
over by the flow: a writer reads `below` on the page, and a diagram
turned to run to the right keeps its meaning only if the words turn
with it; `after` and `before` stay for the drag, which writes a place in
a rank whatever the flow.

## How it is checked

- `graph::hold`: a box below a neighbor stands in a rank under it, above
  in one over it, level with it in its rank; right of it in a flow to
  the right is a later rank; an entry against the lines is said.
- `omaspec-render/tests/hand.rs`: `C below B` and `C level with B` in
  every drawer of graph diagrams, with no line drawn more and lint as
  quiet as without the entry.
- The block reads and writes every word back as written.
