# 0032: Diagrams are set in Inter, which ships with the program

Status: accepted, 2026-11-17.

## Context

The engine measured words on the advance widths of Liberation Sans and
named the generic `sans-serif` in its pictures
([0004](0004-native-plantuml.md)). What drew them was whatever the
machine had for it: Liberation Sans on Omarchy, DejaVu Sans elsewhere,
some 13 % wider than measured (REQ-068). The window around the diagram is
set in a modern monospace; the diagrams in it looked like a document from
an office suite, and a PNG differed from machine to machine.

## Decision

1. **Inter 4.1 is the face of a diagram** that names no other
   (`omaspec_uml::FACE`). `skinparam defaultFontName`, Mermaid's
   `fontFamily` and a project's `[look] font` name another as before. The
   generic `sans-serif` stays what it was, as wide as Arial.
2. **Measured in what is drawn.** `omaspec-uml` has Inter's advance
   widths for its regular and bold cuts beside Liberation's
   (`metrics.rs`, generated from the faces' `hmtx`); `text::Family::Inter`
   reads them. The italics are measured as the upright cuts: over a line
   they are half a percent wider, which each run's `textLength` takes up.
3. **Four cuts in the binary**, regular, bold, italic and bold italic,
   as subsets (`omaspec-render/fonts`, 130 KB each): Latin up to
   Extended-B, Greek, Cyrillic, punctuation, arrows and signs. They are
   loaded before the system's fonts, so an Inter installed there does not
   draw in their place: the pane, a PNG and a PDF are the same on every
   machine.
4. **Without contextual alternates.** Inter draws `->` as an arrow and an
   `x` between numbers as a times sign. A diagram shows what was written,
   so the subsets keep kerning, marks and the local forms and leave the
   rest out.
5. **An exported SVG and a presentation carry the cuts they use** as
   `@font-face`s (`fonts::embed`), 16 KB each as WOFF2 and 21 KB as text,
   with Latin up to Extended-A and the common punctuation and arrows: the
   characters that are measured. Behind Inter the picture names the faces
   of the generic sans-serif (`INTER_FACES`). They are narrower, so where
   a viewer takes no embedded face and has no Inter, words stay in their
   boxes.
6. **Pictures of another engine keep their face.** mmdr and the Java
   PlantUML measure in the fonts installed. mmdr's theme asks for Inter
   first; it is handed the list without it, or the bundled cuts would
   draw wider than it measured.

## Consequences

- Every picture changed, and with it every golden SVG: Inter is 7 to 8 %
  wider than Liberation Sans, so boxes are a little wider.
- An SVG with words is 21 KB larger, 43 KB with bold ones. A subset made
  for each picture would be a few KB, but needs a font subsetter that
  writes what a browser accepts, which the dependencies do not have; the
  fixed subsets are the compromise. `render --text-as-paths` still writes
  a picture that needs no face at all.
- A character outside the bundled cuts is drawn by an installed face, as
  before, and measured as an average letter.
- The repository and the binary grow by 0.6 MB.
- The subsets are made with fonttools from the release's static faces:
  `pyftsubset Inter-Regular.ttf --layout-features=kern,mark,mkmk,ccmp,locl
  --no-hinting --notdef-outline --unicodes=…` (and `--flavor=woff2`); the
  ranges are in `fonts.rs`'s header. A new Inter is taken by making them
  again and generating the tables again.
