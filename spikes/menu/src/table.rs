//! The command table of the spike: the app's commands and keys, placed in
//! menus, on the toolbar and in the context menus.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Top {
    File,
    Edit,
    View,
    Diagram,
    Project,
    Help,
}

impl Top {
    pub const ALL: [Top; 6] = [
        Top::File,
        Top::Edit,
        Top::View,
        Top::Diagram,
        Top::Project,
        Top::Help,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Top::File => "File",
            Top::Edit => "Edit",
            Top::View => "View",
            Top::Diagram => "Diagram",
            Top::Project => "Project",
            Top::Help => "Help",
        }
    }

    pub fn index(self) -> usize {
        Top::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// Where a command sits in a context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Diagram,
    Editor,
    Both,
}

impl Spot {
    pub fn covers(self, spot: Spot) -> bool {
        self == Spot::Both || self == spot
    }
}

/// One command, as the command list of the app would carry it.
pub struct Cmd {
    pub label: &'static str,
    pub key: &'static str,
    pub menu: Top,
    /// The child menu this entry sits in, e.g. "Export".
    pub sub: Option<&'static str>,
    /// A line above this entry.
    pub gap: bool,
    /// The glyph on the toolbar.
    pub tool: Option<&'static str>,
    pub context: Option<Spot>,
}

const fn cmd(label: &'static str, key: &'static str, menu: Top) -> Cmd {
    Cmd {
        label,
        key,
        menu,
        sub: None,
        gap: false,
        tool: None,
        context: None,
    }
}

impl Cmd {
    const fn gap(self) -> Self {
        Cmd { gap: true, ..self }
    }
    const fn sub(self, sub: &'static str) -> Self {
        Cmd {
            sub: Some(sub),
            ..self
        }
    }
    const fn tool(self, glyph: &'static str) -> Self {
        Cmd {
            tool: Some(glyph),
            ..self
        }
    }
    const fn at(self, spot: Spot) -> Self {
        Cmd {
            context: Some(spot),
            ..self
        }
    }
}

/// The real commands and keys of the app, placed in menus.
pub const COMMANDS: &[Cmd] = &[
    cmd("New tab", "Ctrl+N", Top::File),
    cmd("Open a file, folder or image…", "Ctrl+O", Top::File),
    cmd("Find a file…", "Ctrl+P", Top::File),
    cmd("Save", "Ctrl+S", Top::File).gap(),
    cmd("Save as…", "Ctrl+Shift+S", Top::File),
    cmd("Revert to the saved file", "", Top::File),
    cmd("SVG…", "", Top::File)
        .gap()
        .sub("Export")
        .at(Spot::Diagram),
    cmd("PNG…", "", Top::File).sub("Export").at(Spot::Diagram),
    cmd("JPEG…", "", Top::File).sub("Export"),
    cmd("PDF…", "", Top::File).sub("Export"),
    cmd("Close the tab", "Ctrl+W", Top::File).gap(),
    cmd("Close the other tabs", "", Top::File),
    cmd("Quit", "Ctrl+Q", Top::File).gap(),
    cmd("Format the diagram", "Ctrl+Shift+I", Top::Edit).at(Spot::Editor),
    cmd("Replace in all tabs…", "Ctrl+Shift+H", Top::Edit),
    cmd("Rename everywhere…", "F2", Top::Edit)
        .gap()
        .at(Spot::Editor),
    cmd("Collapse the block", "Ctrl+Shift+[", Top::Edit)
        .gap()
        .at(Spot::Editor),
    cmd("Expand the block", "Ctrl+Shift+]", Top::Edit).at(Spot::Editor),
    cmd("Collapse all blocks", "", Top::Edit),
    cmd("Expand all blocks", "", Top::Edit),
    cmd("Skin: sketch", "", Top::Edit).gap().at(Spot::Editor),
    cmd("Skin: remove", "", Top::Edit).at(Spot::Editor),
    cmd("Sidebar on/off", "Ctrl+B", Top::View),
    cmd("Diagram in its own window", "", Top::View).at(Spot::Diagram),
    cmd("Menu bar", "Alt", Top::View).gap(),
    cmd("Toolbar", "", Top::View),
    cmd("Zoom in", "Alt+=", Top::View)
        .gap()
        .tool("+")
        .at(Spot::Diagram),
    cmd("Zoom out", "Alt+-", Top::View)
        .tool("−")
        .at(Spot::Diagram),
    cmd("100 %", "Alt+0", Top::View)
        .tool("1:1")
        .at(Spot::Diagram),
    cmd("Fit the diagram", "Alt+F", Top::View)
        .tool("⤢")
        .at(Spot::Diagram),
    cmd("Render now", "Ctrl+Enter", Top::Diagram),
    cmd("Play", "F5", Top::Diagram).tool("▶").at(Spot::Diagram),
    cmd("Play a recorded trace…", "", Top::Diagram),
    cmd("Fold this container", "", Top::Diagram)
        .gap()
        .at(Spot::Both),
    cmd("Focus on this box", "Shift+click", Top::Diagram).at(Spot::Both),
    cmd("Go to the source", "Ctrl+click", Top::Diagram).at(Spot::Diagram),
    cmd("Unfold every container", "", Top::Diagram).tool("⊞"),
    cmd("Reset this line by hand", "", Top::Diagram)
        .gap()
        .at(Spot::Both),
    cmd("Reset every line and box", "", Top::Diagram),
    cmd("Copy as PNG", "Ctrl+Shift+C", Top::Diagram)
        .gap()
        .tool("⎘")
        .at(Spot::Diagram),
    cmd("Copy as SVG", "", Top::Diagram).at(Spot::Diagram),
    cmd("Theme: follow Omarchy", "", Top::Diagram).gap(),
    cmd("Language: detect", "", Top::Diagram).gap(),
    cmd("Language: PlantUML", "", Top::Diagram),
    cmd("Language: Mermaid", "", Top::Diagram),
    cmd("New project…", "", Top::Project),
    cmd("Export the project…", "", Top::Project).tool("↧"),
    cmd("Compare with the last commit", "", Top::Project).gap(),
    cmd("Stop comparing", "", Top::Project),
    cmd("Help Omaspec", "", Top::Help),
    cmd("Browse the examples", "Ctrl+Shift+E", Top::Help),
    cmd("Every command…", "Ctrl+Shift+P", Top::Help).gap(),
];

/// The entries of `top`, in order.
pub fn entries(top: Top) -> impl Iterator<Item = (usize, &'static Cmd)> {
    COMMANDS
        .iter()
        .enumerate()
        .filter(move |(_, c)| c.menu == top)
}

/// The entries a context menu at `spot` shows.
pub fn context_entries(spot: Spot) -> impl Iterator<Item = (usize, &'static Cmd)> {
    COMMANDS
        .iter()
        .enumerate()
        .filter(move |(_, c)| c.context.is_some_and(|s| s.covers(spot)))
}
