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
