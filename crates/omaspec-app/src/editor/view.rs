//! What the editor window shows: tabs, the project tree, the editors, the
//! diagram, the bottom line and the file picker.

use std::path::{Path, PathBuf};

use iced::widget::{
    Image, Space, button, column, container, keyed_column, mouse_area, opaque, pane_grid,
    responsive, rich_text, row, scrollable, stack, text, text_input,
};
use iced::{Alignment, Element, Font, Length, Padding, Theme};
use omaspec_render::export::Format;
use omaspec_theme::{Palette, Rgb, tokens};

use omaspec_core::Language;
use omaspec_core::lint::Severity;
use omaspec_model::{Change, ChangeKind};

use super::compare::{self, Compare, Row};
use super::picker::Kind;
use super::{
    App, Asking, Command, Message, Mode, PICKER, PROMPT, Pane, Prompt, REPLACE, TABS, keys, plural,
    shortcut,
};
use crate::icons::{self, Icon};
use crate::ui;

const EXPORT_HINT: &str = "Enter exports · Esc cancels";

fn prompt_text(asking: Asking) -> (&'static str, &'static str, &'static str) {
    match asking {
        Asking::SaveAs => ("Save as", "file to save to", "Enter saves · Esc cancels"),
        Asking::Open => (
            "Open",
            "file or folder to open",
            "Enter opens · Ctrl+P finds a file · Esc cancels",
        ),
        Asking::NewProject { language, git } => (
            match (language, git) {
                (Language::PlantUml, false) => "New PlantUML project",
                (Language::PlantUml, true) => "New PlantUML project with git",
                (Language::Mermaid, false) => "New Mermaid project",
                (Language::Mermaid, true) => "New Mermaid project with git",
            },
            "folder for the project",
            "Enter makes it · a folder that is there keeps its files · Esc cancels",
        ),
        Asking::Export(format) => match format {
            Format::Svg => ("Export SVG", "where to write the SVG", EXPORT_HINT),
            Format::Png => ("Export PNG", "where to write the PNG", EXPORT_HINT),
            Format::Jpeg => ("Export JPEG", "where to write the JPEG", EXPORT_HINT),
            Format::Pdf => ("Export PDF", "where to write the PDF", EXPORT_HINT),
            Format::Markdown => (
                "Export Markdown",
                "where to write the Markdown",
                EXPORT_HINT,
            ),
            Format::Zip => ("Export ZIP", "where to write the ZIP", EXPORT_HINT),
        },
    }
}

/// Width of the focus frame. Content is inset by the same amount: the editor
/// and the diagram paint their own background across every pixel they are
/// given, and a border drawn underneath would disappear.
const FRAME: f32 = 1.0;
/// How much of the accent the focus frame takes: a hint, not a fence.
const FOCUS: f32 = 0.6;

/// The focus frame's color on an area of `quiet`: that share of the accent,
/// or more of it, then of the text color, until it stands out from the area
/// and from the line between areas as much as a control has to (3:1).
pub(super) fn focus_color(p: &Palette, quiet: Rgb) -> Rgb {
    let around = [quiet, p.dark_background];
    ui::mix(quiet, p.accent, FOCUS)
        .legible_on(&around, p.accent, tokens::MIN_CONTRAST)
        .legible_on(&around, p.foreground, tokens::MIN_CONTRAST)
}

impl App {
    pub(super) fn view(&self, window: iced::window::Id) -> Element<'_, Message> {
        if Some(window) == self.detached {
            self.detached_view()
        } else {
            self.main_view()
        }
    }

    /// The diagram's own window: the diagram and one line below it. Keys
    /// as in the preview window; Escape brings the diagram back. A list
    /// opened from here is drawn here, and its keys replace those.
    pub(super) fn detached_view(&self) -> Element<'_, Message> {
        let p = &self.palette;
        let doc = self.doc();
        let (state, color) = doc.paper.state(p);
        let bar = row![
            self.mark(),
            self.label(doc.name(), p.foreground),
            container(self.label(state, color))
                .width(Length::Fill)
                .clip(true),
            self.label(
                "+/- zoom · f fit · Ctrl+click finds the line · Esc back",
                p.dark_foreground
            ),
            self.label(
                format!("{:.0}%", doc.paper.zoom_percent()),
                p.dark_foreground
            ),
        ]
        .spacing(12);
        let id = self.detached;
        let mode = id.map(|window| self.mode(window)).unwrap_or(Mode::Editing);
        let window = self.with_picker(id, column![self.diagram(), ui::bar(bar.into(), p)].into());
        keys::Guard::new(window, move |key, mods| {
            detached_key(key, mods, mode).map(|command| match id {
                Some(window) => Message::Keyed(window, command),
                None => Message::Command(command),
            })
        })
        .into()
    }

    pub(super) fn main_view(&self) -> Element<'_, Message> {
        // The focused area's frame fades in as the one before fades out.
        let lit = self
            .focus_fade
            .interpolate(0.0, 1.0, iced::time::Instant::now());
        let grid = pane_grid::PaneGrid::new(&self.panes, |_, pane, _maximized| {
            let (body, quiet, fill) = match pane {
                Pane::Files => (
                    self.project(),
                    self.palette.dark_background,
                    self.palette.dark_background,
                ),
                Pane::Source => (
                    self.sources(),
                    self.palette.background,
                    self.palette.background,
                ),
                Pane::Diagram => (
                    self.diagram(),
                    self.palette.background,
                    self.palette.background,
                ),
            };
            let accent = if *pane == self.focus {
                lit
            } else if self.left_focus == Some(*pane) {
                1.0 - lit
            } else {
                0.0
            };
            let body = self.area(accent, quiet, fill, body);
            pane_grid::Content::new(body)
        })
        .on_click(Message::Focus)
        .on_resize(8, Message::Resized)
        .spacing(2);
        // The gap between the panes shows as a line.
        let line = ui::color(self.palette.dark_background);
        let grid =
            container(grid).style(move |_: &Theme| container::Style::default().background(line));
        let bottom = match &self.prompt {
            Some(prompt) => self.prompt_bar(prompt),
            None => self.status_bar(),
        };
        let mode = self.mode(self.main);
        let window_id = self.main;
        let window = self.with_picker(Some(window_id), column![self.tabs(), grid, bottom].into());
        // A stack either way, as for the picker.
        let window = match self.gallery {
            Some(_) => stack![window, self.gallery_view()],
            None => stack![window],
        };
        keys::Guard::new(window, move |key, mods| {
            shortcut(key, mods, mode).map(|command| Message::Keyed(window_id, command))
        })
        .into()
    }

    /// The picker over `base` when it belongs to `window`.
    fn with_picker<'a>(
        &'a self,
        window: Option<iced::window::Id>,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        // Always a stack, so the picker coming and going keeps the widgets
        // under it. Built anew, the pane grid builds its panes once more on
        // the next frame, and a new source editor takes the keyboard: what
        // is typed into the prompt that follows would go there too.
        if self.picker.is_some() && window.is_some() && self.picker_on == window {
            stack![base, self.picker_view()].into()
        } else {
            stack![base].into()
        }
    }

    /// The active tab's diagram, or A|B while it is compared.
    fn diagram(&self) -> Element<'_, Message> {
        let key = self.doc().key;
        let paper = &self.doc().paper;
        let a = paper
            .view(&self.palette, self.font, true)
            .map(move |m| Message::Paper(key, m));
        // Without a picture the pane shows what brings one; an empty page
        // gets no note, its template is the way on.
        let a = match paper.trouble() {
            Some(_) if !paper.has_picture() && self.doc().text().trim().is_empty() => {
                stack![a, self.empty_state(true)].into()
            }
            Some(trouble) if !paper.has_picture() => stack![
                a,
                self.empty_state(false),
                self.trouble_note(trouble, paper.playing())
            ]
            .into(),
            Some(trouble) => stack![a, self.trouble_note(trouble, paper.playing())].into(),
            None => a,
        };
        let Some(c) = self.compare.as_ref().filter(|c| c.a == key) else {
            return a;
        };
        let p = &self.palette;
        let b: Element<'_, Message> = if c.show_behavior {
            self.behavior_list(c)
        } else if c.show_diff {
            self.diff_list(c)
        } else {
            c.paper
                .view(&self.palette, self.font, false)
                .map(|m| Message::Paper(compare::KEY, m))
        };
        let toggle = |on: bool, label: &'static str, command| {
            button(self.label(label, if on { p.accent } else { p.dark_foreground }))
                .padding([0, 6])
                .on_press(Message::Command(command))
                .style(|_: &Theme, _| button::Style::default())
        };
        let a_bar = row![
            self.label("A", p.accent),
            container(self.label(self.doc().name(), p.foreground)).clip(true),
        ]
        .spacing(8);
        let b_bar = row![
            self.label("B", p.accent),
            container(self.label(c.title.clone(), p.foreground))
                .width(Length::Fill)
                .clip(true),
            toggle(c.show_diff, "diff", Command::CompareDiff),
            toggle(c.show_behavior, "behavior", Command::CompareBehavior),
            toggle(c.sync, "sync", Command::CompareSync),
            toggle(false, "×", Command::StopCompare),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        row![side(a_bar.into(), a), side(b_bar.into(), b)]
            .spacing(2)
            .into()
    }

    /// B as the lines changed from B to A; a click goes to the line in A.
    fn diff_list(&self, c: &Compare) -> Element<'_, Message> {
        let p = &self.palette;
        let Some(rows) = &c.rows else {
            return container(self.label("too many changes to list", p.dark_foreground))
                .padding([6, 12])
                .into();
        };
        if rows.is_empty() {
            return container(self.label(
                "no changes (blank lines and indentation aside)",
                p.dark_foreground,
            ))
            .padding([6, 12])
            .into();
        }
        let line = |row: &Row| -> Element<'_, Message> {
            let (number, sign, text, color, line) = match row {
                Row::Same { line, text } => {
                    (format!("{}", line + 1), " ", text, p.dark_foreground, *line)
                }
                Row::Added { line, text } => (format!("{}", line + 1), "+", text, p.green, *line),
                Row::Removed { line, text } => (String::new(), "-", text, p.red, *line),
                Row::Gap => {
                    return container(self.label("⋯", p.dark_foreground))
                        .padding([0, 12])
                        .into();
                }
            };
            let content = row![
                container(self.label(number, p.dark_foreground))
                    .width(Length::Fixed(40.0))
                    .align_x(Alignment::End),
                self.label(format!("{sign} {text}"), color),
            ]
            .spacing(8);
            self.entry(content, 0, false, Message::JumpTo(line))
        };
        let bg = ui::color(p.background);
        container(scrollable(column(rows.iter().map(line)).padding([4, 0])).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_: &Theme| container::Style::default().background(bg))
            .into()
    }

    /// B as what changed in the behavior from B to A, a finding of the
    /// checks among them; a click goes to the line in A.
    fn behavior_list(&self, c: &Compare) -> Element<'_, Message> {
        let p = &self.palette;
        let quiet = |message: String| -> Element<'_, Message> {
            container(self.label(message, p.dark_foreground))
                .padding([6, 12])
                .into()
        };
        let changes = match &c.behavior {
            Err(reason) => return quiet(reason.clone()),
            Ok(changes) if changes.is_empty() => {
                return quiet("the behavior is the same".into());
            }
            Ok(changes) => changes,
        };
        let line = |change: &Change| -> Element<'_, Message> {
            let (sign, color) = match (change.kind, change.severity) {
                (ChangeKind::Added, _) => ("+", p.green),
                (ChangeKind::Removed, _) => ("-", p.red),
                (ChangeKind::Changed, _) => ("~", p.yellow),
                (ChangeKind::Found, Some(Severity::Error)) => ("!", p.red),
                (ChangeKind::Found, _) => ("!", p.yellow),
                (ChangeKind::Resolved, _) => ("✓", p.green),
            };
            let number = change
                .new_line
                .map(|l| format!("{}", l + 1))
                .unwrap_or_default();
            let content = row![
                container(self.label(number, p.dark_foreground))
                    .width(Length::Fixed(40.0))
                    .align_x(Alignment::End),
                text(format!("{sign} {}", change.message))
                    .font(self.font)
                    .size(13)
                    .color(ui::color(color)),
            ]
            .spacing(8);
            match change.new_line {
                Some(at) => self.entry(content, 0, false, Message::JumpTo(at)),
                None => container(content).padding([1, 12]).into(),
            }
        };
        let bg = ui::color(p.background);
        container(scrollable(column(changes.iter().map(line)).padding([4, 0])).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_: &Theme| container::Style::default().background(bg))
            .into()
    }

    fn label(&self, s: impl Into<String>, color: Rgb) -> text::Text<'_> {
        text(s.into())
            .font(self.font)
            .size(13)
            .color(ui::color(color))
            .wrapping(text::Wrapping::None)
    }

    /// Every document's editor, only the active one in sight; the others
    /// keep their scroll position and folds.
    fn sources(&self) -> Element<'_, Message> {
        keyed_column(self.docs.iter().enumerate().map(|(i, doc)| {
            let key = doc.key;
            let editor = doc.editor.view().map(move |e| Message::Editor(key, e));
            let height = if i == self.active {
                Length::Fill
            } else {
                Length::Fixed(0.0)
            };
            (key, container(editor).height(height).clip(true).into())
        }))
        .into()
    }

    /// A frame the accent color paints while this area is the one fullscreen
    /// will use; `accent` (0 to 1) is how far it has turned, while the focus
    /// moves. The quiet color matches the area, so the frame is invisible
    /// until then. Padding keeps the stroke in sight: iced draws it before
    /// the child, and the editor and the diagram would otherwise cover it.
    fn area<'a>(
        &self,
        accent: f32,
        quiet: Rgb,
        fill: Rgb,
        body: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let frame = ui::color(ui::mix(quiet, focus_color(&self.palette, quiet), accent));
        let fill = ui::color(fill);
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(FRAME)
            .clip(true)
            .style(move |_: &Theme| container::Style {
                background: Some(fill.into()),
                border: iced::Border {
                    color: frame,
                    width: FRAME,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    /// Text for the bars and the sidebar, a size under the editor's.
    pub(super) fn small(&self, s: impl Into<String>, color: Rgb) -> text::Text<'_> {
        text(s.into())
            .font(self.font)
            .size(12)
            .color(ui::color(color))
            .wrapping(text::Wrapping::None)
    }

    /// A word in a small pill on the sidebar's ground, as a file's kind.
    fn tag(&self, s: impl Into<String>) -> Element<'_, Message> {
        let p = &self.palette;
        ui::pill(
            text(s.into())
                .font(self.font)
                .size(10.5)
                .color(ui::color(ui::quiet(p)))
                .wrapping(text::Wrapping::None),
            Some(ui::color(p.background)),
            None,
        )
        .padding([0, 7])
        .into()
    }

    /// `icon` in `color`, at the size of the small text.
    pub(super) fn icon(&self, icon: Icon, color: Rgb) -> Element<'_, Message> {
        match self.icons.get(icon, color) {
            Some(handle) => Image::new(handle)
                .width(icons::SIZE)
                .height(icons::SIZE)
                .into(),
            None => Space::new().width(icons::SIZE).height(icons::SIZE).into(),
        }
    }

    /// The color of a document's icon: the accent while it is in front,
    /// else Mermaid's own or a quiet one.
    fn tint(&self, language: Language, front: bool) -> Rgb {
        let p = &self.palette;
        match (front, language) {
            (true, _) => p.accent,
            (false, Language::Mermaid) => p.magenta,
            (false, Language::PlantUml) => ui::quiet(p),
        }
    }

    /// The sidebar's heading: a folder and its name, small and quiet.
    fn heading(&self, title: &str) -> Element<'_, Message> {
        let quiet = ui::quiet(&self.palette);
        container(
            row![
                self.icon(Icon::Folder, quiet),
                text(title.to_uppercase())
                    .font(self.font)
                    .size(10.5)
                    .color(ui::color(quiet))
                    .wrapping(text::Wrapping::None),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 12.0,
            right: 10.0,
            bottom: 8.0,
            left: 10.0,
        })
        .clip(true)
        .into()
    }

    /// A row of the sidebar, a pill: `active` stands out, a click sends
    /// `message`.
    fn entry<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
        depth: usize,
        active: bool,
        message: Message,
    ) -> Element<'a, Message> {
        let p = &self.palette;
        let (chosen, hover) = (
            ui::color(p.lighter_background),
            ui::alpha(p.lighter_background, 0.6),
        );
        button(container(content).clip(true))
            .width(Length::Fill)
            .padding(Padding {
                top: 3.0,
                bottom: 3.0,
                left: 10.0 + 12.0 * depth as f32,
                right: 8.0,
            })
            .on_press(message)
            .style(move |_: &Theme, status| button::Style {
                background: match (active, status) {
                    (true, _) => Some(chosen.into()),
                    (false, button::Status::Hovered | button::Status::Pressed) => {
                        Some(hover.into())
                    }
                    _ => None,
                },
                border: iced::Border {
                    radius: 999.0.into(),
                    ..iced::Border::default()
                },
                ..button::Style::default()
            })
            .into()
    }

    fn project(&self) -> Element<'_, Message> {
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let hint = |s: &str| -> Element<'_, Message> {
            container(self.small(s, quiet))
                .padding(Padding {
                    top: 0.0,
                    right: 10.0,
                    bottom: 6.0,
                    left: 10.0,
                })
                .into()
        };
        let Some(folder) = &self.folder else {
            return column![self.heading("project"), hint("Ctrl+O opens a folder")]
                .padding([0, 8])
                .height(Length::Fill)
                .into();
        };
        let name = folder.file_name().map_or_else(
            || folder.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let empty = self
            .tree
            .get(Path::new(""))
            .is_none_or(|listing| listing.children.is_empty() && listing.more == 0);
        let body: Element<'_, Message> = if empty {
            hint("this folder is empty")
        } else {
            // The kinds of the files show only where every name fits
            // beside its kind.
            responsive(move |size| {
                let tags = self.widest_row(Path::new(""), 0) <= size.width;
                scrollable(
                    column(self.project_rows(Path::new(""), 0, tags))
                        .spacing(1)
                        .padding(Padding::ZERO.bottom(4)),
                )
                .height(Length::Fill)
                .into()
            })
            .into()
        };
        let title = match (&self.help, &self.project_name) {
            (Some(_), _) => "Omaspec help",
            (None, Some(project)) => project.as_str(),
            (None, None) => name.as_str(),
        };
        let mut parts = vec![self.heading(title)];
        // In Help Omaspec the line under the heading leads back; in a
        // project it points to the file picker until that has been used.
        match &self.help {
            Some(help) => {
                let back = match &help.back {
                    Some(dir) => format!(
                        "← back to {}",
                        dir.file_name().map_or_else(
                            || dir.display().to_string(),
                            |n| n.to_string_lossy().into_owned()
                        )
                    ),
                    None => "← leave Help Omaspec".into(),
                };
                parts.push(self.entry(
                    self.small(back, p.accent),
                    0,
                    false,
                    Message::Command(Command::LeaveHelp),
                ));
            }
            None if !self.picked => parts.push(hint("Ctrl+P finds diagrams")),
            None => {}
        }
        parts.push(body);
        column(parts).padding([0, 8]).height(Length::Fill).into()
    }

    /// How wide the widest file row under `dir` is with its kind beside
    /// the name, about: a monospace character is 0.6 of the text's size.
    fn widest_row(&self, dir: &Path, depth: usize) -> f32 {
        let Some(listing) = self.tree.get(dir) else {
            return 0.0;
        };
        let mut widest: f32 = 0.0;
        for child in &listing.children {
            let rel = dir.join(&child.name);
            if child.dir {
                if self.open_dirs.contains(&rel) {
                    widest = widest.max(self.widest_row(&rel, depth + 1));
                }
            } else if let Some(kind) = &child.kind {
                let name = child.name.chars().count() as f32 * 12.0 * 0.6;
                let tag = icons::label(kind).chars().count() as f32 * 10.5 * 0.6 + 14.0;
                let around = 10.0 + 12.0 * depth as f32 + 8.0 + icons::SIZE + 16.0 + 12.0;
                widest = widest.max(around + name + tag);
            }
        }
        widest
    }

    /// The rows under `dir` (relative to the project; empty is the root),
    /// with the files' kinds if `tags`.
    fn project_rows(&self, dir: &Path, depth: usize, tags: bool) -> Vec<Element<'_, Message>> {
        let Some(folder) = &self.folder else {
            return Vec::new();
        };
        let Some(listing) = self.tree.get(dir) else {
            return Vec::new();
        };
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let active = self.doc().path.as_deref();
        let mut rows = Vec::new();
        for child in &listing.children {
            let rel = if dir.as_os_str().is_empty() {
                PathBuf::from(&child.name)
            } else {
                dir.join(&child.name)
            };
            if child.dir {
                let open = self.open_dirs.contains(&rel);
                let mark = if open { "▾" } else { "▸" };
                let content = row![
                    self.small(mark, quiet),
                    self.small(child.name.clone(), p.foreground),
                ]
                .spacing(6);
                rows.push(self.entry(content, depth, false, Message::ToggleDir(rel.clone())));
                if open {
                    rows.extend(self.project_rows(&rel, depth + 1, tags));
                }
            } else {
                let path = folder.join(&rel);
                let front = active == Some(path.as_path());
                let open = self.docs.iter().any(|doc| doc.path.as_ref() == Some(&path));
                let icon = match Language::from_file_name(&child.name) {
                    Some(language) => {
                        self.icon(Icon::of(child.kind.as_ref()), self.tint(language, front))
                    }
                    None => Space::new().width(icons::SIZE).height(icons::SIZE).into(),
                };
                let name = self.small(
                    child.name.clone(),
                    if open {
                        p.bright_foreground
                    } else {
                        p.foreground
                    },
                );
                let mut content = vec![icon, container(name).width(Length::Fill).clip(true).into()];
                if let Some(kind) = child.kind.as_ref().filter(|_| tags) {
                    content.push(self.tag(icons::label(kind)));
                }
                rows.push(self.entry(
                    row(content).spacing(8).align_y(Alignment::Center),
                    depth,
                    front,
                    Message::Open(path),
                ));
            }
        }
        if listing.more > 0 {
            let content = self.small(format!("{} more · Ctrl+P", listing.more), quiet);
            rows.push(self.entry(content, depth, false, Message::Command(Command::Pick)));
        }
        rows
    }

    /// The Omaspec mark, about the height of the status line.
    fn mark(&self) -> Element<'_, Message> {
        Image::new(self.mark.clone()).width(18).height(18).into()
    }

    /// The tabs, pills on the bar's background, in the shape of the play
    /// bar's options. The one in front is filled and edged in the accent,
    /// which fades over from the tab before it; a tab under the mouse is
    /// brighter. Each shows the kind of its diagram, its findings as a
    /// badge, and at its end the close button, or a dot while it is unsaved.
    ///
    /// The bar keeps the window's width and scrolls sideways once the tabs
    /// no longer fit, so a narrower window (the diagram beside it, under a
    /// tiling compositor) does not push later tabs out of reach. The wheel
    /// scrolls it, and so does a sideways touchpad gesture.
    fn tabs(&self) -> Element<'_, Message> {
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let lit = self
            .tab_fade
            .interpolate(0.0, 1.0, iced::time::Instant::now());
        let tabs = self.docs.iter().enumerate().map(|(i, doc)| {
            let active = i == self.active;
            let hovered = self.hovered_tab == Some(i);
            let front = if active {
                lit
            } else if self.left_tab == Some(doc.key) {
                1.0 - lit
            } else {
                0.0
            };
            let rest = if hovered { p.foreground } else { quiet };
            let mut parts: Vec<Element<'_, Message>> = vec![
                self.icon(
                    Icon::of(doc.kind.as_ref()),
                    self.tint(doc.language, front > 0.5),
                ),
                self.small(doc.name(), ui::mix(rest, p.bright_foreground, front))
                    .into(),
            ];
            match doc.counts() {
                (0, 0) => {}
                (0, n) => parts.push(ui::badge(n.to_string(), p.yellow, p.background, self.font)),
                (e, _) => parts.push(ui::badge(e.to_string(), p.red, p.background, self.font)),
            }
            let end: Element<'_, Message> = if hovered || (active && !doc.modified) {
                mouse_area(
                    container(self.small("×", p.foreground))
                        .width(14)
                        .align_x(Alignment::Center),
                )
                .on_press(Message::Close(i))
                .on_middle_press(Message::Close(i))
                .interaction(iced::mouse::Interaction::Pointer)
                .into()
            } else if doc.modified {
                container(ui::pill(
                    Space::new().width(7).height(7),
                    Some(ui::color(p.foreground)),
                    None,
                ))
                .width(14)
                .align_x(Alignment::Center)
                .into()
            } else {
                Space::new().width(14).into()
            };
            parts.push(end);
            let pill = ui::pill(
                row(parts).spacing(8).align_y(Alignment::Center),
                Some(ui::alpha(p.lighter_background, front)),
                Some(ui::alpha(p.accent, 0.45 * front)),
            )
            .padding(Padding {
                top: 5.0,
                right: 8.0,
                bottom: 5.0,
                left: 12.0,
            });
            container(
                mouse_area(pill)
                    .on_press(Message::Select(i))
                    .on_middle_press(Message::Close(i))
                    .on_enter(Message::HoverTab(i, true))
                    .on_exit(Message::HoverTab(i, false))
                    .interaction(iced::mouse::Interaction::Pointer),
            )
            .id(super::tab_id(doc.key))
            .into()
        });
        let bg = ui::color(p.dark_background);
        // The bar only grows downwards while the tabs overflow, so a short
        // row looks as it did. The scroller then sits under the names.
        container(tab_wheel::TabWheel::new(
            scrollable(
                row(tabs)
                    .spacing(4)
                    .padding([7, 8])
                    .align_y(Alignment::Center),
            )
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new()
                    .width(6)
                    .scroller_width(6)
                    .spacing(2),
            ))
            .id(TABS)
            .width(Length::Fill)
            .anchor_left(),
        ))
        .width(Length::Fill)
        .style(move |_: &Theme| container::Style::default().background(bg))
        .into()
    }

    fn prompt_bar<'a>(&'a self, prompt: &'a Prompt) -> Element<'a, Message> {
        let p = &self.palette;
        let (title, placeholder, hint) = prompt_text(prompt.asking);
        let input = text_input(placeholder, &prompt.input)
            .id(PROMPT)
            .on_input(Message::PromptInput)
            .on_submit(Message::PromptSubmit)
            .font(self.font)
            .size(13)
            .padding(2);
        let note = self.notice.clone().unwrap_or_else(|| hint.into());
        let bar = row![
            self.label(title, p.accent),
            input,
            self.label(note, p.dark_foreground),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        ui::bar(bar.into(), p)
    }

    /// What the status bar tells in place of the diagram's state: the
    /// notice, else the keys of the play that runs; gone once it stops.
    pub(super) fn note(&self) -> Option<&str> {
        let paper = &self.doc().paper;
        self.notice
            .as_deref()
            .or_else(|| paper.playing().then(|| super::play::keys(paper.tracing())))
    }

    /// One line of pills, in the shape of the play bar: the language and
    /// kind of the diagram, the cursor, the findings with their count as a
    /// badge; then the diagram's state (or a notice), the zoom, the theme
    /// and the command palette. What keeps the text from drawing is told at
    /// the diagram.
    fn status_bar(&self) -> Element<'_, Message> {
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let doc = self.doc();
        let cursor = doc.editor.cursor();
        let lighter = ui::color(p.lighter_background);
        let language = match doc.forced {
            Some(_) => format!("{} •", doc.language.name()),
            None => doc.language.name().to_owned(),
        };
        let mut kind: Vec<Element<'_, Message>> = vec![
            Image::new(self.mark.clone()).width(14).height(14).into(),
            self.small(language, p.bright_foreground).into(),
        ];
        if let Some(k) = &doc.kind {
            kind.push(self.small(icons::label(k), quiet).into());
        }
        let (badge, words) = match doc.counts() {
            (0, 0) => (
                ui::badge("✓", p.green, p.background, self.font),
                "no findings".to_owned(),
            ),
            (0, n) => (
                ui::badge(n.to_string(), p.yellow, p.background, self.font),
                plural(n, "finding"),
            ),
            (e, _) => (
                ui::badge(e.to_string(), p.red, p.background, self.font),
                plural(e, "error"),
            ),
        };
        let middle = match self.note() {
            Some(note) => self.small(note, p.light_foreground),
            None => self.small(doc.paper.summary(), quiet),
        };
        let edge = ui::alpha(p.accent, 0.45);
        let hover = ui::color(p.selection);
        let commands = button(
            row![
                self.small("commands", p.bright_foreground),
                ui::badge("Ctrl+Shift+P", p.accent, p.background, self.font),
            ]
            .spacing(7)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 3.0,
            right: 4.0,
            bottom: 3.0,
            left: 10.0,
        })
        .on_press(Message::Command(Command::Palette))
        .style(move |_: &Theme, status| button::Style {
            background: Some(match status {
                button::Status::Hovered | button::Status::Pressed => hover.into(),
                _ => lighter.into(),
            }),
            border: iced::Border {
                color: edge,
                width: 1.0,
                radius: 999.0.into(),
            },
            ..button::Style::default()
        });
        let bar = row![
            ui::pill(
                row(kind).spacing(7).align_y(Alignment::Center),
                Some(lighter),
                None
            )
            .padding([3, 10]),
            ui::pill(
                self.small(
                    format!("{}:{}", cursor.row + 1, cursor.col + 1),
                    p.foreground
                ),
                Some(lighter),
                None
            )
            .padding([3, 10]),
            ui::pill(
                row![badge, self.small(words, p.foreground)]
                    .spacing(7)
                    .align_y(Alignment::Center),
                Some(lighter),
                None
            )
            .padding(Padding {
                top: 3.0,
                right: 10.0,
                bottom: 3.0,
                left: 4.0,
            }),
            container(middle)
                .width(Length::Fill)
                .padding([0, 8])
                .clip(true),
            self.small(format!("{:.0} %", doc.paper.zoom_percent()), quiet),
            self.small(self.theme_name.clone(), quiet),
            commands,
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let bg = ui::color(p.dark_background);
        container(bar)
            .width(Length::Fill)
            .padding([5, 8])
            .style(move |_: &Theme| container::Style::default().background(bg))
            .into()
    }

    /// What keeps the text from drawing, at the foot of the diagram: the
    /// line to blame as a badge, what is wrong, and a button to the line.
    /// It floats like the play bar, edged in red, above it while a play
    /// runs.
    fn trouble_note(
        &self,
        (line, what): (Option<usize>, String),
        above_play: bool,
    ) -> Element<'_, Message> {
        responsive(move |size| self.trouble_note_in(line, &what, above_play, size.width)).into()
    }

    /// The note of [`Self::trouble_note`] in a pane `width` wide.
    fn trouble_note_in(
        &self,
        line: Option<usize>,
        what: &str,
        above_play: bool,
        width: f32,
    ) -> Element<'_, Message> {
        let p = &self.palette;
        let mut parts: Vec<Element<'_, Message>> = Vec::new();
        let badge = line.map(|line| format!("line {}", line + 1));
        // The text is monospace, so its width is known: it takes what the
        // badge and the button leave and wraps beyond that. Left to iced, a
        // wrapping text takes the whole row and pushes the button out.
        let char_width = 12.0 * 0.6;
        let badge_width = badge
            .as_ref()
            .map_or(0.0, |b| b.chars().count() as f32 * 10.5 * 0.6 + 22.0);
        let button_width = if line.is_some() {
            10.0 * char_width + 30.0
        } else {
            0.0
        };
        let room = (width - 52.0 - badge_width - button_width).clamp(char_width * 12.0, 520.0);
        let text_width = (what.chars().count() as f32 * char_width + 2.0).min(room);
        if let Some(badge) = badge {
            parts.push(ui::badge(badge, p.red, p.background, self.font));
        }
        parts.push(
            container(
                self.small(what.to_owned(), p.bright_foreground)
                    .wrapping(text::Wrapping::WordOrGlyph),
            )
            .width(text_width)
            .into(),
        );
        if let Some(line) = line {
            let (lighter, hover) = (ui::color(p.lighter_background), ui::color(p.selection));
            parts.push(
                button(self.small("go to line", p.bright_foreground))
                    .padding([3, 10])
                    .on_press(Message::JumpTo(line))
                    .style(move |_: &Theme, status| button::Style {
                        background: Some(match status {
                            button::Status::Hovered | button::Status::Pressed => hover.into(),
                            _ => lighter.into(),
                        }),
                        border: iced::Border {
                            radius: 999.0.into(),
                            ..iced::Border::default()
                        },
                        ..button::Style::default()
                    })
                    .into(),
            );
        }
        let (fill, edge) = (ui::alpha(p.dark_background, 0.94), ui::alpha(p.red, 0.8));
        let note = container(row(parts).spacing(10).align_y(Alignment::Center))
            .padding(Padding {
                top: 6.0,
                right: 6.0,
                bottom: 6.0,
                left: 10.0,
            })
            .style(move |_: &Theme| {
                container::Style::default()
                    .background(fill)
                    .border(iced::Border {
                        color: edge,
                        width: 1.0,
                        radius: 14.0.into(),
                    })
                    .shadow(iced::Shadow {
                        color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.35),
                        offset: iced::Vector::new(0.0, 4.0),
                        blur_radius: 18.0,
                    })
            });
        container(note)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::End)
            .padding(Padding {
                top: 0.0,
                right: 18.0,
                bottom: if above_play { 76.0 } else { 20.0 },
                left: 18.0,
            })
            .into()
    }

    /// The diagram pane while it has no picture: on an empty page a start
    /// from a template, else what brings the picture.
    fn empty_state(&self, blank: bool) -> Element<'_, Message> {
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let (title, line) = if blank {
            (
                "An empty page",
                "Start from a template or an example, or type a diagram.",
            )
        } else {
            (
                "No picture yet",
                "It comes once the text draws; the note below says what stops it.",
            )
        };
        let key = |keys: &str, what: &'static str| -> Element<'_, Message> {
            row![self.keys(keys), self.small(what, quiet)]
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
        };
        let mut parts: Vec<Element<'_, Message>> = vec![
            Image::new(self.mark_large.clone())
                .width(44)
                .height(44)
                .into(),
            text(title)
                .font(self.font)
                .size(15)
                .color(ui::color(p.bright_foreground))
                .into(),
            self.small(line, quiet).into(),
        ];
        if blank {
            let (lighter, hover, edge) = (
                ui::color(p.lighter_background),
                ui::color(p.selection),
                ui::alpha(p.accent, 0.45),
            );
            let start = |language: Language, icon: Icon| -> Element<'_, Message> {
                button(
                    row![
                        self.icon(icon, p.accent),
                        self.small(language.name(), p.bright_foreground)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([5, 14])
                .on_press(Message::Command(Command::StartFrom(language)))
                .style(move |_: &Theme, status| button::Style {
                    background: Some(match status {
                        button::Status::Hovered | button::Status::Pressed => hover.into(),
                        _ => lighter.into(),
                    }),
                    border: iced::Border {
                        color: edge,
                        width: 1.0,
                        radius: 999.0.into(),
                    },
                    ..button::Style::default()
                })
                .into()
            };
            let examples = button(
                row![
                    self.icon(Icon::Gallery, p.accent),
                    self.small("Examples", p.bright_foreground)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([5, 14])
            .on_press(Message::Command(Command::Gallery))
            .style(move |_: &Theme, status| button::Style {
                background: Some(match status {
                    button::Status::Hovered | button::Status::Pressed => hover.into(),
                    _ => lighter.into(),
                }),
                border: iced::Border {
                    color: edge,
                    width: 1.0,
                    radius: 999.0.into(),
                },
                ..button::Style::default()
            });
            parts.push(
                row![
                    start(Language::PlantUml, Icon::State),
                    start(Language::Mermaid, Icon::Flow),
                    examples,
                ]
                .spacing(10)
                .into(),
            );
            parts.push(
                column![
                    key("Ctrl+O", "opens a file"),
                    key("Ctrl+P", "finds a diagram"),
                    key("Ctrl+Shift+E", "browses the examples"),
                ]
                .spacing(6)
                .into(),
            );
        } else {
            parts.push(
                column![
                    key("F8", "goes to the next finding"),
                    key("Ctrl+Enter", "draws now")
                ]
                .spacing(6)
                .into(),
            );
        }
        container(column(parts).spacing(12).align_x(Alignment::Center))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }

    /// A field of the picker: a pill on the lighter ground, the text in it
    /// without a frame of its own.
    pub(super) fn field<'a>(
        &self,
        input: text_input::TextInput<'a, Message>,
    ) -> text_input::TextInput<'a, Message> {
        let p = &self.palette;
        let (quiet, value, selection) = (
            ui::color(ui::quiet(p)),
            ui::color(p.bright_foreground),
            ui::color(p.selection),
        );
        input
            .font(self.font)
            .size(13)
            .padding([3, 4])
            .style(move |_: &Theme, _| text_input::Style {
                background: iced::Color::TRANSPARENT.into(),
                border: iced::Border::default(),
                icon: quiet,
                placeholder: quiet,
                value,
                selection,
            })
    }

    /// `label` with the typed letters (`lit`) in the accent, and a
    /// command's group (`Skin:`) quiet.
    fn lit_label(
        &self,
        label: &str,
        lit: &[usize],
        grouped: bool,
        chosen: bool,
    ) -> Element<'_, Message> {
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let ink = if chosen {
            p.bright_foreground
        } else {
            p.foreground
        };
        let group = if grouped {
            label.find(": ").map_or(0, |i| label[..=i].chars().count())
        } else {
            0
        };
        let bold = Font {
            weight: iced::font::Weight::Bold,
            ..self.font
        };
        let mut spans: Vec<text::Span<'_, (), Font>> = Vec::new();
        let mut run = String::new();
        let mut style: Option<(Rgb, bool)> = None;
        let mut flush = |run: &mut String, style: Option<(Rgb, bool)>| {
            if let Some((color, strong)) = style.filter(|_| !run.is_empty()) {
                let piece = text::Span::new(std::mem::take(run)).color(ui::color(color));
                spans.push(if strong { piece.font(bold) } else { piece });
            }
        };
        for (i, c) in label.chars().enumerate() {
            let now = if lit.contains(&i) {
                (p.accent, true)
            } else if i < group {
                (quiet, false)
            } else {
                (ink, false)
            };
            if style != Some(now) {
                flush(&mut run, style);
                style = Some(now);
            }
            run.push(c);
        }
        flush(&mut run, style);
        rich_text(spans)
            .font(self.font)
            .size(13)
            .wrapping(text::Wrapping::None)
            .into()
    }

    /// A command's keys in a pill outlined in the muted color.
    pub(super) fn keys(&self, keys: &str) -> Element<'_, Message> {
        let p = &self.palette;
        ui::pill(
            text(keys.to_owned())
                .font(self.font)
                .size(10.5)
                .color(ui::color(ui::quiet(p)))
                .wrapping(text::Wrapping::None),
            None,
            Some(ui::color(p.muted)),
        )
        .padding([0, 7])
        .into()
    }

    /// The file picker or the command palette over the window, floating
    /// like the play bar; a click beside it closes it. The typed letters
    /// light up in each row, the commands chosen lately lead the list.
    fn picker_view(&self) -> Element<'_, Message> {
        let Some(picker) = &self.picker else {
            return column![].into();
        };
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let lighter = ui::color(p.lighter_background);
        let input = self.field(
            text_input(picker.placeholder, &picker.query)
                .id(PICKER)
                .on_input(Message::PickerInput)
                .on_submit(Message::PickerSubmit),
        );
        let search = ui::pill(
            row![self.icon(Icon::Search, quiet), input]
                .spacing(8)
                .align_y(Alignment::Center),
            Some(lighter),
            None,
        )
        .padding([3, 12]);
        let commands = picker.kind == Kind::Commands;
        let list = picker.visible().map(|(row_at, item)| {
            let chosen = row_at == picker.selected;
            let mut parts: Vec<Element<'_, Message>> = vec![
                container(self.lit_label(&item.label, &picker.lit(&item.label), commands, chosen))
                    .width(Length::Fill)
                    .clip(true)
                    .into(),
            ];
            if picker.is_recent(row_at) {
                parts.push(self.small("recent", quiet).into());
            }
            if !item.hint.is_empty() {
                parts.push(if commands {
                    self.keys(&item.hint)
                } else {
                    self.small(item.hint.clone(), quiet).into()
                });
            }
            self.entry(
                row(parts).spacing(10).align_y(Alignment::Center),
                0,
                chosen,
                Message::PickerClick(row_at),
            )
        });
        let rows: Element<'_, Message> = column(list).spacing(1).into();
        let count = match (picker.kind, picker.count()) {
            (Kind::Files, 0) if self.folder.is_none() => "no folder: Ctrl+O opens one".to_owned(),
            (Kind::Search, 0) if picker.query.is_empty() => {
                "type to search the text of all tabs".to_owned()
            }
            (_, 0) => "no match".to_owned(),
            (Kind::Files, n) => format!("{n} {}", plural(n, "file")),
            (Kind::Commands, n) => format!("{n} {}", plural(n, "command")),
            (Kind::Versions, n) => format!("{n} {}", plural(n, "version")),
            (Kind::Traces, n) => format!("{n} {}", plural(n, "trace")),
            (Kind::NewProject, _) => "Enter, then name its folder".to_owned(),
            (Kind::Search, n) if n >= super::search::MAX_HITS => format!("the first {n} hits"),
            (Kind::Search, n) => format!("{n} {}", plural(n, "hit")),
        };
        let count = match &picker.replace {
            Some(_) => {
                format!("{count} · Enter in the second field replaces them all · Tab switches")
            }
            None => count,
        };
        let replace: Element<'_, Message> = match &picker.replace {
            Some(with) => ui::pill(
                self.field(
                    text_input("replace with", with)
                        .id(REPLACE)
                        .on_input(Message::ReplaceInput)
                        .on_submit(Message::ReplaceSubmit),
                ),
                Some(lighter),
                None,
            )
            .padding(Padding {
                top: 3.0,
                right: 12.0,
                bottom: 3.0,
                left: 12.0 + icons::SIZE + 8.0,
            })
            .into(),
            None => column![].into(),
        };
        let (fill, edge) = (ui::color(p.dark_background), ui::alpha(p.accent, 0.45));
        let panel = container(
            column![
                search,
                replace,
                rows,
                container(self.small(count, quiet)).padding([2, 12]),
            ]
            .spacing(6),
        )
        .width(Length::Fixed(620.0))
        .padding(10)
        .style(move |_: &Theme| {
            container::Style::default()
                .background(fill)
                .border(iced::Border {
                    color: edge,
                    width: 1.0,
                    radius: 14.0.into(),
                })
                .shadow(iced::Shadow {
                    color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                    offset: iced::Vector::new(0.0, 8.0),
                    blur_radius: 28.0,
                })
        });
        let backdrop = mouse_area(
            container(opaque(panel))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .padding(Padding {
                    top: 60.0,
                    ..Padding::ZERO
                }),
        )
        .on_press(Message::Command(super::Command::Cancel));
        backdrop.into()
    }
}

/// The plain wheel over the tab bar scrolls it sideways. The bar itself
/// only moves for a sideways gesture, or for the wheel while Shift is held.
mod tab_wheel {
    use iced::advanced::widget::{Operation, Tree, Widget};
    use iced::advanced::{Clipboard, Layout, Shell, layout, mouse, overlay, renderer};
    use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};

    use super::Message;

    pub struct TabWheel<'a> {
        content: Element<'a, Message>,
    }

    impl<'a> TabWheel<'a> {
        pub fn new(content: impl Into<Element<'a, Message>>) -> Self {
            Self {
                content: content.into(),
            }
        }
    }

    impl Widget<Message, Theme, Renderer> for TabWheel<'_> {
        fn children(&self) -> Vec<Tree> {
            vec![Tree::new(&self.content)]
        }

        fn diff(&self, tree: &mut Tree) {
            tree.diff_children(std::slice::from_ref(&self.content));
        }

        fn size(&self) -> Size<Length> {
            self.content.as_widget().size()
        }

        fn layout(
            &mut self,
            tree: &mut Tree,
            renderer: &Renderer,
            limits: &layout::Limits,
        ) -> layout::Node {
            self.content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, limits)
        }

        fn operate(
            &mut self,
            tree: &mut Tree,
            layout: Layout<'_>,
            renderer: &Renderer,
            operation: &mut dyn Operation,
        ) {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                operation,
            );
        }

        fn update(
            &mut self,
            tree: &mut Tree,
            event: &Event,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            renderer: &Renderer,
            clipboard: &mut dyn Clipboard,
            shell: &mut Shell<'_, Message>,
            viewport: &Rectangle,
        ) {
            // Taken before the bar sees the event. A horizontal bar treats a
            // vertical wheel as its own and then keeps it, so the notch would
            // never move the tabs. A notch downward brings the right-hand
            // tabs in. A sideways gesture is left to the bar.
            let along = match event {
                Event::Mouse(mouse::Event::WheelScrolled { delta })
                    if cursor.position_over(layout.bounds()).is_some() =>
                {
                    match *delta {
                        mouse::ScrollDelta::Lines { x, y } if x == 0.0 && y != 0.0 => {
                            Some(-y * 60.0)
                        }
                        mouse::ScrollDelta::Pixels { x, y } if x == 0.0 && y != 0.0 => Some(-y),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(x) = along {
                shell.publish(Message::ScrollTabs(x));
                shell.capture_event();
                return;
            }
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }

        fn mouse_interaction(
            &self,
            tree: &Tree,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            viewport: &Rectangle,
            renderer: &Renderer,
        ) -> mouse::Interaction {
            self.content.as_widget().mouse_interaction(
                &tree.children[0],
                layout,
                cursor,
                viewport,
                renderer,
            )
        }

        fn draw(
            &self,
            tree: &Tree,
            renderer: &mut Renderer,
            theme: &Theme,
            style: &renderer::Style,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            viewport: &Rectangle,
        ) {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
            )
        }

        fn overlay<'b>(
            &'b mut self,
            tree: &'b mut Tree,
            layout: Layout<'b>,
            renderer: &Renderer,
            viewport: &Rectangle,
            translation: Vector,
        ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
            self.content.as_widget_mut().overlay(
                &mut tree.children[0],
                layout,
                renderer,
                viewport,
                translation,
            )
        }
    }

    impl<'a> From<TabWheel<'a>> for Element<'a, Message> {
        fn from(wheel: TabWheel<'a>) -> Self {
            Element::new(wheel)
        }
    }
}

/// One half of A|B: its title bar above its body.
fn side<'a>(bar: Element<'a, Message>, body: Element<'a, Message>) -> Element<'a, Message> {
    column![container(bar).padding([2, 8]).width(Length::Fill), body]
        .width(Length::FillPortion(1))
        .into()
}

/// Keys in the diagram's own window: those of the preview window, and the
/// editor window's shortcuts. While a list or a question is open, its keys
/// win, so Escape closes the list instead of bringing the diagram back.
fn detached_key(
    key: &iced::keyboard::Key,
    mods: iced::keyboard::Modifiers,
    mode: Mode,
) -> Option<Command> {
    if mode != Mode::Editing {
        return shortcut(key, mods, mode);
    }
    use iced::keyboard::{Key, key::Named};
    let plain = !mods.control() && !mods.alt() && !mods.logo();
    match key.as_ref() {
        Key::Character("+" | "=") if plain => Some(Command::ZoomIn),
        Key::Character("-") if plain => Some(Command::ZoomOut),
        Key::Character("0") if plain => Some(Command::ActualSize),
        Key::Character("f") if plain => Some(Command::Fit),
        Key::Named(Named::Escape) => Some(Command::Detach),
        _ => shortcut(key, mods, Mode::Editing),
    }
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers, key::Named};

    use super::*;

    #[test]
    fn the_diagram_window_has_the_preview_keys() {
        let c = |s: &str| Key::Character(s.into());
        let editing = Mode::Editing;
        assert_eq!(
            detached_key(&c("+"), Modifiers::empty(), editing),
            Some(Command::ZoomIn)
        );
        assert_eq!(
            detached_key(&c("f"), Modifiers::empty(), editing),
            Some(Command::Fit)
        );
        assert_eq!(
            detached_key(&Key::Named(Named::Escape), Modifiers::empty(), editing),
            Some(Command::Detach)
        );
        assert_eq!(
            detached_key(&c("s"), Modifiers::CTRL, editing),
            Some(Command::Save)
        );
        assert_eq!(detached_key(&c("x"), Modifiers::empty(), editing), None);

        // The command list is open: typing and its keys stay with it.
        let picker = Mode::Picker;
        assert_eq!(
            detached_key(&Key::Named(Named::Escape), Modifiers::empty(), picker),
            Some(Command::Cancel)
        );
        assert_eq!(
            detached_key(&Key::Named(Named::ArrowDown), Modifiers::empty(), picker),
            Some(Command::Down)
        );
        assert_eq!(detached_key(&c("f"), Modifiers::empty(), picker), None);
        assert_eq!(detached_key(&c("+"), Modifiers::empty(), picker), None);
    }
}
