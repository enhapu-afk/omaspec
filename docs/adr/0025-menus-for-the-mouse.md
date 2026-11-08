# 0025: A context menu, a menu bar and a small toolbar

Status: accepted, 2026-11-08; the engine chosen after the spike
`spikes/menu` the same day. Builds on [0001](0001-gui-iced.md); the first
step toward Omaspec on other desktops in the [roadmap](../../ROADMAP.md),
*other desktops, and a face for the mouse*.

## Context

Omaspec is driven from the keyboard and from the diagram. The command
list (`Ctrl+Shift+P`) holds every command; the status bar shows the mark
and that key. In the diagram the mouse already does a lot: a click
follows a link, a button folds a container, Alt and a drag shape a line or
hold a box, Shift and a click set the focus, Ctrl and a click find the
source, the wheel zooms. Beside the diagram there is nothing to click: no
menu, no toolbar, no right mouse button.

That is fine for someone who knows the keys. It hides the tool from
everyone else: a reader who opens a project sees an editor and a picture,
and has no way to learn that it folds, plays, compares, renames and
exports. The tool has grown past what one list behind one key can show.
On Windows, where Omaspec is to run next, a window without a menu bar
reads as unfinished, and no one there knows `Ctrl+Shift+P`.

The engine is not the question: `omaspec-core`, `omaspec-uml`,
`omaspec-model` and `omaspec-render` type-check for Windows as they are,
and the binary does too, with the Linux parts (Landlock, the Wayland app
id, Unix modes) behind `cfg`. Nor is the toolkit: iced draws on Windows
through wgpu, and the pane's work (the glides, the fades, the lights,
the lines by hand) is in its canvases.

## Decision

1. **A context menu, on the right mouse button.** In the diagram it
   holds the actions of what is under the pointer: fold this container
   or unfold it, put this box in focus or end the focus, go to its
   source, open the diagram it links to, reset this line or this box
   drawn by hand, copy the diagram as PNG or SVG, export it. In the
   editor it holds the palette's commands for the caret: fold the
   container at the caret, rename what it names, find its uses, format,
   a skin. The menu is the pane's and the editor's; nothing in it is new,
   every entry is a command the palette has.
2. **A menu bar: File, Edit, View, Diagram, Project, Help.** Every entry
   shows its shortcut beside it, so the bar teaches the keys. The entries
   come from the same table as the command list, each command saying
   which menu it sits in; a command with no menu is in the list only.
   On Omarchy, whose windows have no decorations and whose users have
   the list, the bar is hidden by default and Alt shows it; a setting,
   kept with the session, shows it always. Elsewhere it is shown.
3. **A small toolbar over the diagram pane**, for what a reader does
   most: fit, zoom in and out, 100 %, play, fold all and unfold all,
   copy, export. At most ten, each with a tooltip that names the key. It
   is the pane's, not the window's: the editor pane has none. It hides
   with the menu bar and has a setting of its own.
4. **The command list stays the whole.** Every command is in it, and it
   stays the fastest way; the menus and the toolbar are doors to the same
   rooms, not new rooms. A key the guide lists keeps working everywhere.
5. **Drawn by iced, as an overlay of Omaspec's own.** iced has no menu
   bar. The spike `spikes/menu` drew the same bar, context menus and
   toolbar twice, from one command table: with `iced_aw` 0.14's `MenuBar`
   and `ContextMenu`, and with `stack`, `mouse_area` and padding. The own
   overlay won: it takes the keys (arrows between menus and entries,
   Enter, Escape, F10, a tap on Alt for the bar) where `iced_aw`'s menus
   take none, it closes on a click like every desktop's menus where
   `iced_aw` closes when the pointer leaves a margin, it is the same
   widgets as the rest of the window so it looks like it, and it adds no
   dependency. What it lacks, `iced_aw` has: child menus (File › Export)
   and placement from the layout; the app gets both from an overlay
   widget of its own, as `TabWheel` in `editor/view.rs` already is. A
   native menu is not an option: iced draws the whole window, on every
   desktop.

## Why

What is missing is not the mouse but the way in. A context menu puts an
action where the thing is, which a complex tool needs more than a simple
one; a menu bar is what a Windows user looks for first, and its shortcuts
beside the entries are how they learn the keys; a short toolbar carries
the handful of actions a reader repeats. One table of commands behind
all three keeps the list, the menus, the toolbar and the guide saying the
same. Rebuilding the interface in another toolkit for the sake of menus
would throw away the pane, and iced runs where Omaspec is going anyway.

## Consequences

- The command list's table grows a menu, a toolbar place and a context
  (diagram, editor, both) per command; the guide's key table gains the
  menu beside each key.
- `editor/menu.rs` for the bar and the context menu (an overlay widget
  placed by the layout, a child menu as a second level, the keys as in
  the spike), `paper/tools.rs` for the toolbar; headless snapshots
  `menu`, `context-menu` and `toolbar`, in the bundled themes.
- A context menu that would leave the window opens upward or leftward;
  a tap on Alt alone shows the bar, Alt with a key stays a shortcut
  (Alt+F fits).
- Omarchy's look is kept: hidden by default there, the bar and the
  toolbar take the theme's colors when shown.
- Not now: a ribbon, a sidebar of icons, a native menu bar, and menus in
  the preview window, which keeps its keys and its right-click-free
  diagram until the editor's are settled.
