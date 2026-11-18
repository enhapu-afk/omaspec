# Vendored crates

Third-party code Omaspec builds from here instead of crates.io, through
`[patch.crates-io]` in the workspace `Cargo.toml`. The workspace excludes this
folder: it is not formatted or linted as ours, so the difference to the
original stays small.

## scrive 0.3.0

`scrive-core` and `scrive-iced`, the editor widget
([0002](../docs/adr/0002-editor-widget-scrive.md)), by Robby Madruga, MIT
(`LICENSE` in each crate; the published packages ship none, the text comes from
the repository at the release commit `0d8132c`). The codicon font in
`scrive-iced/assets` is Microsoft's, CC BY 4.0 (see `CODICON-LICENSE.md`
there).

Copied from the crates.io packages unchanged, then changed as below. Every
addition says "Omaspec addition to scrive 0.3.0" in its doc comment. The
files keep their CRLF line endings.

- `scrive-core`: `Document::jump_to(range)` selects a range and asks the view
  to center it, expanding folds that hide it, like the F8 diagnostic jump.
- `scrive-iced`: `CodeEditor::jump_to(range)` calls it and closes popups.
- `scrive-iced`: `LineMarker` and `CodeEditor::set_line_markers` /
  `line_markers`, `Editor::line_markers`: a bar in the gutter's left pad for
  runs of rows, with an optional tint under the text, or a tick where rows
  were removed. Drawn only for rows in view and cut to the widget.
- `scrive-iced/Cargo.toml`: iced without default features, so the app's
  choice holds (it leaves out the D-Bus theme detection).
- `scrive-iced`: Ctrl+F and Ctrl+H open the find bar only without Shift
  or Alt. Iced reports keys without Shift, so Ctrl+Shift+F (search over
  all tabs) opened the bar too, and Ctrl+Alt+F did the same.
- `scrive-iced`: the hover card wraps to the editor's width, at most 60
  characters as before. It was as wide as its longest line, so a narrow
  editor cut off a finding's message at the right edge.
- `scrive-core`: `SyntaxDef::from_sublime_syntaxes(main, embedded)` loads a
  grammar with the grammars it embeds in one set, so the Markdown grammar
  highlights the PlantUML and Mermaid in its fences.
- `scrive-core`: diagram blocks that are not brackets (`alt`/`end`,
  partitions, subgraphs) can fold. `Document` keeps those regions beside
  the bracket folds (`set_fold_regions`, `collapse_all`, `expand_all`).
  A region is `(header byte, last byte)`. It shifts with edits, drops
  when the span collapses, and opens when an edit lands on a hidden row.
  `FoldMap::with_regions` resolves it, also for jumps (which open a
  collapsed region that hides their target), carets added above or below,
  and column selection. `scrive-iced` exposes the same on `CodeEditor`.
- `scrive-iced`: the word under the caret is washed at its other places
  only while the editor is focused. A play puts the caret on the line it
  lights while the diagram holds the keys, and the wash marked the state
  the machine had just left.

- `scrive-iced`: `CodeEditor::ask_completions()` asks for the completion
  list at the caret, and Ctrl+Space types nothing, so the app can bind it
  to that. scrive had no key for the list.

- `scrive-iced`: `Tint` and `CodeEditor::set_tints`, `Editor::tints`
  (`src/tint.rs`, a file of ours): a run of a line in a color of the
  host's, laid over the grammar's spans when a row is drawn, for what the
  host knows about a word (a state, an event). A tint that does not fit
  its line is left out.

- `scrive-iced`: motion (`src/motion.rs`, a file of ours): a single caret
  glides to where it goes in 90 ms, and the completion list arrives in
  110 ms, faint and a little low at first. `CodeEditor::set_motion` and
  `Editor::motion` turn both off. The widget asks for frames while either
  runs, from its `RedrawRequested`.
- `scrive-iced`: the scroll bars' thumbs are drawn as pills of 6 px in
  their lanes of 12, as the window's other scrollers; the lanes take the
  pointer as before.

To see the changes:

```bash
diff -ru ~/.cargo/registry/src/*/scrive-iced-0.3.0/src vendor/scrive-iced/src
diff -ru ~/.cargo/registry/src/*/scrive-core-0.3.0/src vendor/scrive-core/src
```

(The registry copy exists after `cargo fetch` with the `[patch]` entries
removed, or download the `.crate` files from crates.io.)

The changes may go upstream; until then, a new scrive release is taken by
copying it here again and applying them.

## winit 0.30.13

The windowing crate iced 0.14 uses, by the winit contributors, Apache-2.0
(`LICENSE`). Copied from the crates.io package. Examples and the crate's own tests
are left out, except `src/platform_impl/linux/x11/tests/xsettings.dat`,
which that backend includes while it is being tested. Changed as below.
The Wayland pinch is marked "Omaspec" in the touched files.

- `src/platform_impl/linux/wayland/seat/pointer/pinch.rs`: binds
  `zwp_pointer_gestures_v1` and, for each pointer, `zwp_pointer_gesture_pinch_v1`.
  The protocol's scale is absolute since the gesture began. Each update
  becomes a mouse-wheel line delta `y` with `1.2^y` equal to the scale
  change, which is how the diagram canvas zooms. A cancelled gesture
  sends the delta back to scale 1.
- `seat/mod.rs`, `seat/pointer/mod.rs`, `state.rs`: own the gesture
  objects next to the relative pointer, and drop them with the pointer.
- `seat/dnd.rs`: `wl_data_device` accepts `text/uri-list` and sends each
  local `file://` path as `WindowEvent::DroppedFile`. The list is read
  once the pipe is readable, after the event loop flushes the request.
  Omarchy is Wayland; without this a dropped SVG or PNG never reached
  the editor.
- `seat/mod.rs`, `state.rs`: one data device per seat, beside the pointer.
- `seat/dnd.rs`, `seat/mod.rs`, `wayland/mod.rs`, `platform/wayland.rs`:
  the data device keeps the clipboard offer it receives, and
  `platform::wayland::selection_offer()` hands it out. Hyprland sends the
  clipboard only to the first data device of a client, which is this one;
  `smithay-clipboard` (below) reads it from here.

```bash
diff -ru ~/.cargo/registry/src/*/winit-0.30.13/src/platform_impl/linux/wayland/seat \
  vendor/winit/src/platform_impl/linux/wayland/seat
diff -ru ~/.cargo/registry/src/*/winit-0.30.13/src/platform_impl/linux/wayland/state.rs \
  vendor/winit/src/platform_impl/linux/wayland/state.rs
diff -u ~/.cargo/registry/src/*/winit-0.30.13/src/platform_impl/linux/wayland/mod.rs \
  vendor/winit/src/platform_impl/linux/wayland/mod.rs
diff -u ~/.cargo/registry/src/*/winit-0.30.13/src/platform/wayland.rs \
  vendor/winit/src/platform/wayland.rs
```

A new winit release is taken by copying it here again and applying the
pinch file, `seat/dnd.rs`, and those edits. The wheel base `1.2` has to
stay the one in `crates/omaspec-app/src/paper/canvas.rs`.

## smithay-clipboard 0.7.3

The Wayland clipboard iced reads and writes (through `window_clipboard`),
by Kirill Chibisov and Elinor Berger, MIT (`LICENSE`). Copied from the
crates.io package without the example, then changed as below.

- `src/state.rs`, `Cargo.toml`: when its own data device has no clipboard
  offer, `load_selection` reads the one winit's data device received
  (`winit::platform::wayland::selection_offer`). Hyprland sends the
  clipboard only to a client's first data device. Winit makes its device
  for file drops first, so without this nothing could be pasted.

```bash
diff -ru ~/.cargo/registry/src/*/smithay-clipboard-0.7.3/src vendor/smithay-clipboard/src
```

A new release is taken by copying it here again and applying that edit,
as long as winit keeps a data device of its own.

## iced_tiny_skia 0.14.1

iced's renderer without a GPU, by Héctor Ramón and the iced contributors, MIT
(`LICENSE`, from the `iced` 0.14.0 package of the same repository; this
package ships none). `omaspec play` draws with it where there is no GPU.
Copied from the crates.io package unchanged, then changed as below; each
change says "Omaspec" in a comment.

- `src/engine.rs`, `draw_image`: a raster image keeps to its own clip, a
  canvas's `with_clip`, as well as to its layer's, as it does on the GPU.
  The package kept it to its layer only: a glide that draws each box as a
  part of its picture, cut out by a clip, drew the whole picture for each
  box, scaled as the box, all over the pane. An image that is not turned
  is drawn only inside the clip and without a mask; a turned one gets the
  clip as its mask, and the layer's mask back after it.
- `src/raster.rs`: `Pipeline::draw_within`, which fills that part with the
  image as a pattern, at the image's exact place (the package rounded it
  down to a whole pixel of the image). Its edges round half a pixel up, as
  tiny-skia's fills without anti-aliasing and its clip masks round theirs,
  so two parts side by side meet without a seam, and a part that holds no
  pixel's middle covers none (tiny-skia would fill one of no width a pixel
  wide).

```bash
diff -ru ~/.cargo/registry/src/*/iced_tiny_skia-0.14.1/src vendor/iced_tiny_skia/src
```

The change may go upstream; a new release is taken by copying it here
again and applying it. `paper::sprite`'s tests
`a_box_s_parts_keep_to_the_box_without_a_gpu` and
`parts_side_by_side_draw_each_pixel_once` fail without it.

## iced_wgpu 0.14.0

iced's renderer on the GPU, by Héctor Ramón and the iced contributors, MIT
(`LICENSE`, from the `iced` 0.14.0 package of the same repository; this
package ships none). Copied from the crates.io package unchanged, then
changed as below; each change says "Omaspec" in a comment.

- `src/window/compositor.rs`: a window's surface takes only a format with
  eight bits to each of red, green, blue and alpha. The package took the
  first format that is not sRGB (Omaspec builds iced with `web-colors`),
  and a Wayland compositor that shows ten bits of color offers
  `Rgb10a2Unorm` before the eight-bit ones under Vulkan. That format has
  two bits of alpha. With anti-aliasing on, the meshes of a canvas are
  drawn on a layer of the surface's format and blended from it, so every
  fill that lets something through had one of four strengths: the fade
  under the participants held at the top came out as four bands with a
  ramp in each, and a play's veil gray instead of the paper's color
  (Hyprland 0.56, Mesa 26.2 and NVIDIA 580, 2026-11-18). OpenGL offers no
  such format, and a picture drawn without a window has eight bits, so no
  snapshot shows it.

- `src/window/compositor.rs`: where a machine has two graphics cards, the
  one that takes little power is asked for. The package asked for the
  strong one, which a laptop then keeps awake for as long as the editor is
  open. `WGPU_POWER_PREF=high` asks for the strong one still. Both cards'
  drivers are still loaded to tell them apart.

```bash
diff -ru ~/.cargo/registry/src/*/iced_wgpu-0.14.0/src vendor/iced_wgpu/src
```

The first change may go upstream; a new release is taken by copying it here
again and applying it. No test fails without it: it shows only in a window
under Vulkan, on a desktop that offers the format.

## usvg 0.47.0

resvg's SVG reader, by Yevhenii Reizner and the resvg contributors,
Apache-2.0 OR MIT (`LICENSE-APACHE`, `LICENSE-MIT`, from the package).
Every picture Omaspec shows or exports is read by it
(`omaspec_render::export::parse`). Copied from the crates.io package
unchanged, then changed as below in `src/text/layout.rs`; each change says
"Omaspec addition to usvg 0.47.0" in a comment.

- `shape_text`: for a character its face lacks, usvg shapes the text again
  in a fallback face and copies that face's glyphs in, but only when both
  shapings have as many glyphs, and gave up otherwise. An emoji face draws
  a family joined by zero width joiners, a skin tone, a flag, a keycap and
  `🛋️` (with the mark that asks for a picture) as one glyph where a text
  face has one for each character, so every emoji of such a line was an
  empty box (REQ-051). Where the two shapings do not stand glyph by glyph
  on the same bytes (the counts differ, or a merge before changed them),
  `merge_by_bytes` now lays each of the fallback's clusters over the
  missing glyphs by the bytes they cover, where both faces cut the text at
  the cluster's ends.
- `process_chunk`: a run of words in another face (`<tspan
  font-weight="bold">`) replaces the glyphs of its clusters. A cluster's
  length is in bytes, and the package took it for a number of glyphs: for
  an emoji of four bytes a character it removed the glyphs after the
  cluster and could remove past the end, which ends the program. The
  glyphs that go or come are now those whose bytes the cluster covers,
  another glyph at its first byte (a base and its mark) among them.

```bash
diff -ru ~/.cargo/registry/src/*/usvg-0.47.0/src vendor/usvg/src
```

The changes may go upstream; a new release is taken by copying it here
again and applying them. `omaspec-render`'s `tests/emoji.rs` fails without
them.
