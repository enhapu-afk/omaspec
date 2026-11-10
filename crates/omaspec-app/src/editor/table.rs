//! The command table: every command with its label and its keys, and
//! where the mouse finds it (docs/adr/0025): the menu it sits in, its
//! place on the toolbar, the context menus it appears in. The command
//! list (`Ctrl+Shift+P`), the menus, the toolbar and the context menus
//! all read this one table; no other list of commands exists.

use iced::widget::pane_grid;
use omaspec_core::skin::Pack;

use super::Command;

/// A menu of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Menu {
    File,
    Edit,
    View,
    Diagram,
    Project,
    Help,
}

impl Menu {
    pub const ALL: [Menu; 6] = [
        Menu::File,
        Menu::Edit,
        Menu::View,
        Menu::Diagram,
        Menu::Project,
        Menu::Help,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Menu::File => "File",
            Menu::Edit => "Edit",
            Menu::View => "View",
            Menu::Diagram => "Diagram",
            Menu::Project => "Project",
            Menu::Help => "Help",
        }
    }
}

/// Where a context menu is opened, and so which entries it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Diagram,
    Editor,
    /// Both the diagram's and the editor's menu.
    Both,
}

impl Spot {
    /// Whether an entry placed at `self` shows in the menu opened at `at`.
    pub fn covers(self, at: Spot) -> bool {
        self == Spot::Both || self == at
    }
}

/// One command of the table.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    /// The label in the command list: a sentence, as the list has room.
    pub label: &'static str,
    /// The keys, as shown (`Ctrl+Shift+S`); empty for a command without.
    pub keys: &'static str,
    pub command: Command,
    /// The menu of the bar; `None` keeps the command in the list only.
    pub menu: Option<Menu>,
    /// A shorter label for the menus; the list's label otherwise.
    pub short: Option<&'static str>,
    /// The child menu this entry sits in, e.g. `Export`; the entries of a
    /// child menu stand together in the table.
    pub child: Option<&'static str>,
    /// A line above this entry in its menu.
    pub gap: bool,
    /// Its place on the toolbar over the diagram pane, from the left.
    pub tool: Option<u8>,
    /// In the context menu opened here.
    pub context: Option<Spot>,
}

impl Entry {
    /// The label a menu shows.
    pub fn menu_label(&self) -> &'static str {
        self.short.unwrap_or(self.label)
    }
}

const fn cmd(label: &'static str, keys: &'static str, command: Command) -> Entry {
    Entry {
        label,
        keys,
        command,
        menu: None,
        short: None,
        child: None,
        gap: false,
        tool: None,
        context: None,
    }
}

impl Entry {
    const fn menu(self, menu: Menu) -> Self {
        Entry {
            menu: Some(menu),
            ..self
        }
    }
    const fn short(self, short: &'static str) -> Self {
        Entry {
            short: Some(short),
            ..self
        }
    }
    const fn child(self, child: &'static str) -> Self {
        Entry {
            child: Some(child),
            ..self
        }
    }
    const fn gap(self) -> Self {
        Entry { gap: true, ..self }
    }
    const fn tool(self, place: u8) -> Self {
        Entry {
            tool: Some(place),
            ..self
        }
    }
    const fn at(self, spot: Spot) -> Self {
        Entry {
            context: Some(spot),
            ..self
        }
    }
}

use Menu::{Diagram, Edit, File, Help, Project, View};

/// Every command, in the order the command list shows them. The menus
/// take their entries from here in this order too, so an entry's `gap`
/// and `child` read against the entries of the same menu before it.
pub const TABLE: &[Entry] = &[
    cmd("Find a file", "Ctrl+P", Command::Pick).menu(File),
    cmd("Open a file, folder or image", "Ctrl+O", Command::Open)
        .menu(File)
        .short("Open a file, folder or image…"),
    cmd("New project", "", Command::NewProject)
        .menu(Project)
        .short("New project…"),
    cmd("Browse the examples", "Ctrl+Shift+E", Command::Gallery).menu(Help),
    cmd("Open Help Omaspec", "", Command::OpenHelp)
        .menu(Help)
        .short("Help Omaspec"),
    cmd("Back out of the linked diagram", "Alt+Left", Command::Back)
        .menu(Diagram)
        .gap()
        .at(Spot::Diagram),
    cmd("Undo", "Ctrl+Z", Command::Undo).menu(Edit),
    cmd("Redo", "Ctrl+Shift+Z", Command::Redo).menu(Edit),
    cmd("Cut", "Ctrl+X", Command::Cut)
        .menu(Edit)
        .gap()
        .at(Spot::Editor),
    cmd("Copy", "Ctrl+C", Command::Copy)
        .menu(Edit)
        .at(Spot::Editor),
    cmd("Paste", "Ctrl+V", Command::Paste)
        .menu(Edit)
        .at(Spot::Editor),
    cmd("Select all", "Ctrl+A", Command::SelectAll).menu(Edit),
    cmd(
        "Find in the project and all tabs",
        "Ctrl+Shift+F",
        Command::FindAll,
    )
    .menu(Edit)
    .gap()
    .short("Find in the project and all tabs…")
    .at(Spot::Editor),
    cmd("Replace in all tabs", "Ctrl+Shift+H", Command::ReplaceAll)
        .menu(Edit)
        .short("Replace in all tabs…"),
    cmd(
        "Rename the diagram, state or event at the caret everywhere",
        "F2",
        Command::RenameEverywhere,
    )
    .menu(Edit)
    .short("Rename everywhere…")
    .at(Spot::Editor),
    cmd(
        "Find a state or a message in the diagram (Ctrl+F there)",
        "",
        Command::FindInDiagram,
    )
    .menu(View)
    .gap()
    .short("Find in the diagram…")
    .at(Spot::Diagram),
    cmd("Save", "Ctrl+S", Command::Save).menu(File).gap(),
    cmd("Save as", "Ctrl+Shift+S", Command::SaveAs)
        .menu(File)
        .short("Save as…"),
    cmd("Revert to the saved file", "", Command::Revert).menu(File),
    cmd("Render now", "Ctrl+Enter", Command::RenderNow).menu(Diagram),
    cmd(
        "Play the state machine or the scenario",
        "F5",
        Command::Play,
    )
    .menu(Diagram)
    .short("Play")
    .tool(5)
    .at(Spot::Diagram),
    cmd("Play a recorded trace", "", Command::PlayTrace)
        .menu(Diagram)
        .short("Play a recorded trace…"),
    cmd("Format the diagram", "Ctrl+Shift+I", Command::Format)
        .menu(Edit)
        .at(Spot::Editor),
    cmd(
        "Skin: plain (black lines)",
        "",
        Command::Skin(Some(Pack::Plain)),
    )
    .menu(Edit)
    .gap()
    .child("Skin")
    .short("Plain"),
    cmd(
        "Skin: spec (for documents)",
        "",
        Command::Skin(Some(Pack::Spec)),
    )
    .menu(Edit)
    .child("Skin")
    .short("Spec"),
    cmd(
        "Skin: presentation",
        "",
        Command::Skin(Some(Pack::Presentation)),
    )
    .menu(Edit)
    .child("Skin")
    .short("Presentation"),
    cmd(
        "Skin: Omarchy colors",
        "",
        Command::Skin(Some(Pack::Omarchy)),
    )
    .menu(Edit)
    .child("Skin")
    .short("Omarchy colors"),
    cmd("Skin: sketch", "", Command::Skin(Some(Pack::Sketch)))
        .menu(Edit)
        .child("Skin")
        .short("Sketch"),
    cmd("Skin: vivid", "", Command::Skin(Some(Pack::Vivid)))
        .menu(Edit)
        .child("Skin")
        .short("Vivid"),
    cmd("Skin: remove", "", Command::Skin(None))
        .menu(Edit)
        .child("Skin")
        .short("Remove the skin"),
    cmd(
        "By hand: reset the line or box at the caret",
        "",
        Command::ResetLine,
    )
    .menu(Diagram)
    .gap()
    .short("Reset this line or box by hand")
    .at(Spot::Both),
    cmd("By hand: reset every line and box", "", Command::ResetLines)
        .menu(Diagram)
        .short("Reset every line and box by hand"),
    cmd(
        "Focus on the box at the caret and its neighbors, or end it (Shift+click there)",
        "",
        Command::FocusBox,
    )
    .menu(Diagram)
    .gap()
    .short("Focus on this box, or end it")
    .at(Spot::Both),
    cmd(
        "Fold the container at the caret to its box, or unfold it",
        "",
        Command::FoldBox,
    )
    .menu(Diagram)
    .short("Fold this container, or unfold it")
    .at(Spot::Both),
    cmd(
        "Label of the line at the caret to the other side",
        "",
        Command::LabelSide,
    )
    .menu(Diagram)
    .short("Label to the other side")
    .at(Spot::Both),
    cmd(
        "Color the box at the caret, kept in the source",
        "",
        Command::ColorBox,
    )
    .menu(Diagram)
    .short("Color this box…")
    .at(Spot::Both),
    cmd("Unfold every container", "", Command::UnfoldAll)
        .menu(Diagram)
        .tool(6)
        .at(Spot::Diagram),
    cmd("Diagram theme: follow Omarchy", "", Command::FollowTheme)
        .menu(View)
        .gap(),
    cmd("Collapse the block", "Ctrl+Shift+[", Command::Collapse)
        .menu(Edit)
        .gap()
        .at(Spot::Editor),
    cmd("Expand the block", "Ctrl+Shift+]", Command::Expand)
        .menu(Edit)
        .at(Spot::Editor),
    cmd("Collapse all blocks", "", Command::CollapseAll).menu(Edit),
    cmd("Expand all blocks", "", Command::ExpandAll).menu(Edit),
    cmd(
        "Compare with a tab or a git commit",
        "Ctrl+Shift+D",
        Command::Compare,
    )
    .menu(Project)
    .gap()
    .short("Compare with a tab or a git commit…"),
    cmd(
        "Compare: changed lines or diagram",
        "",
        Command::CompareDiff,
    )
    .menu(Project),
    cmd(
        "Compare: changes in behavior or diagram",
        "",
        Command::CompareBehavior,
    )
    .menu(Project),
    cmd("Compare: zoom together on/off", "", Command::CompareSync).menu(Project),
    cmd("Compare: glide from B to A", "", Command::CompareGlide).menu(Project),
    cmd("Stop comparing", "", Command::StopCompare).menu(Project),
    cmd("Language: detect", "", Command::LanguageDetected)
        .menu(Edit)
        .gap()
        .child("Language")
        .short("Detect"),
    cmd("Language: PlantUML", "", Command::LanguagePlantUml)
        .menu(Edit)
        .child("Language")
        .short("PlantUML"),
    cmd("Language: Mermaid", "", Command::LanguageMermaid)
        .menu(Edit)
        .child("Language")
        .short("Mermaid"),
    cmd(
        "Write it in the other language (PlantUML or Mermaid)",
        "",
        Command::Convert,
    )
    .menu(Edit)
    .short("Write it in the other language")
    .at(Spot::Editor),
    cmd("Copy the diagram as PNG", "Ctrl+Shift+C", Command::CopyPng)
        .menu(Diagram)
        .gap()
        .short("Copy as PNG")
        .tool(7)
        .at(Spot::Diagram),
    cmd("Copy the diagram as SVG", "", Command::CopySvg)
        .menu(Diagram)
        .short("Copy as SVG")
        .at(Spot::Diagram),
    cmd("Export the diagram as SVG", "", Command::ExportSvg)
        .menu(File)
        .gap()
        .child("Export")
        .short("SVG…")
        .tool(8)
        .at(Spot::Diagram),
    cmd("Export the diagram as PNG", "", Command::ExportPng)
        .menu(File)
        .child("Export")
        .short("PNG…")
        .at(Spot::Diagram),
    cmd("Export the diagram as JPEG", "", Command::ExportJpeg)
        .menu(File)
        .child("Export")
        .short("JPEG…"),
    cmd("Export the diagram as PDF", "", Command::ExportPdf)
        .menu(File)
        .child("Export")
        .short("PDF…"),
    cmd(
        "Export the diagram as Markdown",
        "",
        Command::ExportMarkdown,
    )
    .menu(File)
    .child("Export")
    .short("Markdown…"),
    cmd(
        "Export the diagram as ZIP (SVG, source, PDF)",
        "",
        Command::ExportZip,
    )
    .menu(File)
    .child("Export")
    .short("ZIP…"),
    cmd(
        "Check the project: lint every diagram and list the findings",
        "",
        Command::CheckProject,
    )
    .menu(Project)
    .gap()
    .short("Check the project"),
    cmd(
        "Where the diagram, state or event at the caret is used",
        "",
        Command::ShowUses,
    )
    .menu(Project)
    .short("Where it is used")
    .at(Spot::Editor),
    cmd(
        "Export the project (SVGs linked to one another, and a ZIP)",
        "",
        Command::ExportProject,
    )
    .menu(Project)
    .short("Export the project…"),
    cmd("New tab", "Ctrl+N", Command::NewTab).menu(File),
    cmd("Close the tab", "Ctrl+W", Command::CloseTab)
        .menu(File)
        .gap(),
    cmd("Close the other tabs", "", Command::CloseOthers).menu(File),
    cmd("Next tab", "Ctrl+PageDown", Command::NextTab).menu(View),
    cmd("Previous tab", "Ctrl+PageUp", Command::PreviousTab).menu(View),
    cmd("Sidebar on/off", "Ctrl+B", Command::ToggleSidebar)
        .menu(View)
        .gap(),
    cmd(
        "Menu bar on/off, as a tap on Alt",
        "",
        Command::ToggleMenuBar,
    )
    .menu(View)
    .short("Menu bar on/off"),
    cmd(
        "Toolbar over the diagram on/off",
        "",
        Command::ToggleToolbar,
    )
    .menu(View)
    .short("Toolbar on/off"),
    cmd(
        "Focus the area to the left",
        "Ctrl+Alt+Left",
        Command::Focus(pane_grid::Direction::Left),
    ),
    cmd(
        "Focus the area to the right",
        "Ctrl+Alt+Right",
        Command::Focus(pane_grid::Direction::Right),
    ),
    cmd(
        "Focus the area above",
        "Ctrl+Alt+Up",
        Command::Focus(pane_grid::Direction::Up),
    ),
    cmd(
        "Focus the area below",
        "Ctrl+Alt+Down",
        Command::Focus(pane_grid::Direction::Down),
    ),
    cmd(
        "Diagram in its own window, or back",
        "Ctrl+Shift+N",
        Command::Detach,
    )
    .menu(View)
    .short("Diagram in its own window")
    .at(Spot::Diagram),
    cmd("Zoom in on the diagram", "Alt+=", Command::ZoomIn)
        .menu(View)
        .gap()
        .short("Zoom in")
        .tool(2),
    cmd("Zoom out of the diagram", "Alt+-", Command::ZoomOut)
        .menu(View)
        .short("Zoom out")
        .tool(3),
    cmd("Diagram at 100 %", "Alt+0", Command::ActualSize)
        .menu(View)
        .short("100 %")
        .tool(4)
        .at(Spot::Diagram),
    cmd("Fit the diagram", "Alt+F", Command::Fit)
        .menu(View)
        .short("Fit")
        .tool(1)
        .at(Spot::Diagram),
    cmd("Every command, as a list", "Ctrl+Shift+P", Command::Palette)
        .menu(Help)
        .gap()
        .short("Every command…"),
    cmd("Quit", "Ctrl+Q", Command::Quit).menu(File).gap(),
];

/// The entries of `menu`, in the table's order.
pub fn entries(menu: Menu) -> impl Iterator<Item = &'static Entry> {
    TABLE.iter().filter(move |e| e.menu == Some(menu))
}

/// A row of a menu of the bar: an entry, or a child menu with its entries.
pub enum Row {
    Entry(&'static Entry),
    Child {
        name: &'static str,
        entries: Vec<&'static Entry>,
        /// A line above, as the first entry of the child has it.
        gap: bool,
    },
}

impl Row {
    pub fn gap(&self) -> bool {
        match self {
            Row::Entry(e) => e.gap,
            Row::Child { gap, .. } => *gap,
        }
    }
}

/// The rows of `menu`: its entries, those of one child menu as one row.
pub fn rows(menu: Menu) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for entry in entries(menu) {
        match entry.child {
            None => rows.push(Row::Entry(entry)),
            Some(name) => match rows.last_mut() {
                Some(Row::Child {
                    name: open,
                    entries,
                    ..
                }) if *open == name => {
                    entries.push(entry);
                }
                _ => rows.push(Row::Child {
                    name,
                    entries: vec![entry],
                    gap: entry.gap,
                }),
            },
        }
    }
    rows
}

/// The entries of the context menu opened at `at`: first those placed in
/// both menus, which act on what is under the pointer (the box, the
/// line), then the rest in the table's order.
pub fn context(at: Spot) -> impl Iterator<Item = &'static Entry> {
    let placed = move |spot: Spot| {
        TABLE
            .iter()
            .filter(move |e| e.context == Some(spot) && spot.covers(at))
    };
    placed(Spot::Both).chain(placed(at))
}

/// The toolbar's entries, from the left.
pub fn tools() -> impl Iterator<Item = &'static Entry> {
    let mut tools: Vec<&'static Entry> = TABLE.iter().filter(|e| e.tool.is_some()).collect();
    tools.sort_by_key(|e| e.tool);
    tools.into_iter()
}

/// The entry of `command`, if the table has it.
#[cfg(test)]
pub fn entry(command: Command) -> Option<&'static Entry> {
    TABLE.iter().find(|e| e.command == command)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn each_command_once() {
        let mut seen = HashSet::new();
        for e in TABLE {
            assert!(
                seen.insert(format!("{:?}", e.command)),
                "{:?} twice",
                e.command
            );
        }
    }

    #[test]
    fn a_child_menu_is_one_row() {
        let rows = rows(Menu::File);
        let export = rows
            .iter()
            .find(|r| matches!(r, Row::Child { name: "Export", .. }))
            .expect("File has Export");
        let Row::Child {
            entries: children,
            gap,
            ..
        } = export
        else {
            unreachable!()
        };
        assert_eq!(children.len(), 6);
        assert!(gap, "a line before Export");
        assert!(rows.len() < entries(Menu::File).count());
    }

    #[test]
    fn the_toolbar_is_short_and_in_order() {
        let tools: Vec<_> = tools().collect();
        assert!((5..=10).contains(&tools.len()), "{} tools", tools.len());
        assert_eq!(tools[0].command, Command::Fit, "the view first");
        let places: Vec<u8> = tools.iter().filter_map(|e| e.tool).collect();
        assert!(places.windows(2).all(|w| w[0] < w[1]), "each place once");
    }

    #[test]
    fn child_menus_stand_together() {
        for menu in Menu::ALL {
            let mut closed: HashSet<&str> = HashSet::new();
            let mut open: Option<&str> = None;
            for e in entries(menu) {
                if e.child != open {
                    if let Some(name) = open {
                        closed.insert(name);
                    }
                    open = e.child;
                    if let Some(name) = open {
                        assert!(!closed.contains(name), "{name} in {menu:?} is split");
                    }
                }
            }
        }
    }

    #[test]
    fn every_menu_has_entries_and_none_too_many() {
        // The rows a menu shows: a child menu is one of them.
        for menu in Menu::ALL {
            let mut children = std::collections::HashSet::new();
            let n = entries(menu)
                .filter(|e| e.child.is_none_or(|child| children.insert(child)))
                .count();
            assert!((2..=20).contains(&n), "{menu:?} has {n} rows");
        }
        assert!(context(Spot::Diagram).count() >= 8);
        assert!(context(Spot::Editor).count() >= 5);
        // What acts under the pointer comes first, in both menus.
        for at in [Spot::Diagram, Spot::Editor] {
            let first = context(at).next().unwrap();
            assert_eq!(first.context, Some(Spot::Both), "{at:?}");
            assert!(context(at).all(|e| e.context.is_some_and(|s| s.covers(at))));
        }
    }

    #[test]
    fn menu_labels_are_short_and_dots_mark_a_prompt() {
        for e in TABLE.iter().filter(|e| e.menu.is_some()) {
            let label = e.menu_label();
            assert!(label.len() <= 42, "{label:?} is long for a menu");
            assert!(
                !label.contains('('),
                "{label:?} explains; the list does that"
            );
        }
        assert!(entry(Command::SaveAs).unwrap().menu_label().ends_with('…'));
        assert_eq!(entry(Command::Save).unwrap().menu_label(), "Save");
    }
}
