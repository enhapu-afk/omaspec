//! The editor window: diagram source in the middle, the diagram on the
//! right, the project folder as a tree on the left. Lint findings show in
//! the text while typing; the diagram is drawn again after a short pause,
//! or at once with Ctrl+Enter. Ctrl+Shift+[ folds the block at the caret,
//! Ctrl+Shift+] unfolds it. Ctrl+click in the diagram jumps to its line.
//! The label at the caret lights on the sheet in the theme accent and glides
//! there, the way a zoom does.
//!
//! Ctrl+Shift+P opens every command as a list, in the window whose keys
//! asked, so typing starts in its field. Ctrl+Shift+N moves the diagram
//! into a window of its own (a second monitor), with the keys of the
//! preview window; closing it brings the diagram back. Super+F is left
//! to Omarchy, which fullscreens the whole window.
//!
//! The window keeps its session (tabs, unsaved text, folder) and brings it
//! back on the next start, so closing it loses nothing.
//!
//! Keys: Ctrl+S save, Ctrl+Shift+S save as, Ctrl+O open a file, folder or
//! image with a diagram inside, Ctrl+P find a diagram, Ctrl+Shift+E browse
//! the examples, Ctrl+Shift+F find
//! in all tabs, Ctrl+Shift+H replace in all tabs, Ctrl+Shift+D compare
//! with another tab or a git commit, Ctrl+Shift+I format, Ctrl+Shift+C
//! copy the diagram as PNG, Ctrl+Enter render now, Ctrl+N new tab, Ctrl+W
//! close tab, Ctrl+PageDown/PageUp next/previous tab, Ctrl+B sidebar
//! on/off, Ctrl+Alt+arrows move between the project, the editor and the
//! diagram, Ctrl+Q quit, Alt+= / Alt+- zoom the diagram, Alt+0 100 %,
//! Alt+F fit. The
//! editor's own: Ctrl+F find, Ctrl+H replace, Ctrl+/ comment, F8 next
//! finding, Ctrl+Z undo.

mod anchored;
pub(crate) mod compare;
mod doc;
mod files;
mod fold;
pub(crate) mod folder;
mod gallery;
mod help;
mod keys;
mod links;
mod menu;
mod panes;
mod picker;
mod play;
mod rename;
mod search;
mod session;
mod syntax;
pub mod table;
mod takes;
mod tools;
mod view;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iced::animation::Easing;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::time::Instant;
use iced::widget::pane_grid;
use iced::{Animation, Font, Size, Subscription, Task, Theme, window};
use omaspec_core::Language;
use omaspec_core::skin::{self, Pack};
use omaspec_model::Specs;
use omaspec_render::export::Format;
use omaspec_theme::{Palette, Theme as Omarchy};
use omaspec_uml::map::Map;

use crate::icons::Icons;
use crate::look::{Hex, LineShift};
use crate::paper;
use crate::ui;
use crate::worker::{self, Alarm, Worker};
use compare::{Compare, Target};
use doc::{Doc, Look};
use picker::{Choice, Item, Kind, Picker};
use session::Store;

/// Lint this long after the last keystroke.
const LINT_PAUSE: Duration = Duration::from_millis(150);
/// Render this long after the last keystroke.
const RENDER_PAUSE: Duration = Duration::from_millis(400);
/// Write the session this long after the last change.
const SESSION_PAUSE: Duration = Duration::from_secs(1);
/// A line jumped to stays lit this long.
const FLASH: Duration = Duration::from_millis(700);
/// Editors write a file in several steps; read it once they are done.
const FILE_SETTLE: Duration = Duration::from_millis(100);
/// A theme switch replaces several files.
const THEME_SETTLE: Duration = Duration::from_millis(200);
const KEY_ZOOM: f32 = 1.25;
const PROMPT: &str = "prompt";
const PICKER: &str = "picker";
const REPLACE: &str = "replace";
/// The tab bar, so the active tab can be brought into view.
const TABS: &str = "tabs";

/// The id of the tab of the document with `key`, so the bar can find it.
fn tab_id(key: u64) -> iced::widget::Id {
    iced::widget::Id::from(format!("tab-{key}"))
}

/// Finds a scrolling area and one item in it as they are laid out, then
/// scrolls the area the least that shows the whole item: a tab in the tab
/// bar, a tile in the gallery.
struct Reveal {
    scroller: iced::widget::Id,
    item: iced::widget::Id,
    /// Along the height; else along the width.
    vertical: bool,
    /// The area's bounds, its content's bounds and how far it is scrolled.
    bar: Option<(iced::Rectangle, iced::Rectangle, f32)>,
    found: Option<iced::Rectangle>,
}

impl Reveal {
    fn new(scroller: iced::widget::Id, item: iced::widget::Id, vertical: bool) -> Self {
        Self {
            scroller,
            item,
            vertical,
            bar: None,
            found: None,
        }
    }
}

impl<T: Send + 'static> iced::advanced::widget::Operation<T> for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<T>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
        if id == Some(&self.item) {
            self.found = Some(bounds);
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&iced::widget::Id>,
        bounds: iced::Rectangle,
        content_bounds: iced::Rectangle,
        translation: iced::Vector,
        _state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        if id == Some(&self.scroller) {
            let scrolled = if self.vertical {
                translation.y
            } else {
                translation.x
            };
            self.bar = Some((bounds, content_bounds, scrolled));
        }
    }

    fn finish(&self) -> iced::advanced::widget::operation::Outcome<T> {
        use iced::advanced::widget::operation::{Outcome, scrollable};
        let (Some((bar, content, scrolled)), Some(item)) = (self.bar, self.found) else {
            return Outcome::None;
        };
        let (size, left, length) = if self.vertical {
            (bar.height, item.y - content.y, item.height)
        } else {
            (bar.width, item.x - content.x, item.width)
        };
        match reveal_offset(size, scrolled, left, length) {
            Some(at) => Outcome::Chain(Box::new(scrollable::scroll_to(
                self.scroller.clone(),
                scrollable::AbsoluteOffset {
                    x: (!self.vertical).then_some(at),
                    y: self.vertical.then_some(at),
                },
            ))),
            None => Outcome::None,
        }
    }
}

/// Where a bar `width` wide, scrolled to `scrolled`, has to scroll to show
/// a tab from `left` to `left + tab` whole; `None` if it already does.
fn reveal_offset(width: f32, scrolled: f32, left: f32, tab: f32) -> Option<f32> {
    if left < scrolled {
        Some(left.max(0.0))
    } else if left + tab > scrolled + width {
        Some((left + tab - width).max(0.0))
    } else {
        None
    }
}
/// The sidebar's share of the left half of the window (of all of it while
/// the diagram has its own window: half that).
const SIDEBAR_SHARE: f32 = 0.32;

/// What the editor opens with: files and a folder from the command line.
#[derive(Debug, Default)]
pub struct Start {
    pub files: Vec<PathBuf>,
    pub folder: Option<PathBuf>,
    pub forced: Option<Language>,
}

impl Start {
    /// Command line paths: folders set the folder (the last one wins),
    /// everything else opens as a file.
    pub fn new(paths: Vec<PathBuf>, forced: Option<Language>) -> Self {
        let (folders, files): (Vec<PathBuf>, Vec<PathBuf>) =
            paths.into_iter().partition(|p| p.is_dir());
        Self {
            files,
            folder: folders.into_iter().last(),
            forced,
        }
    }
}

pub fn run(start: Start) -> iced::Result {
    let (store, notice) = match Store::open() {
        Ok(Some(store)) => (Some(store), None),
        Ok(None) => (
            None,
            Some("another Omaspec window keeps the session; this one keeps none".to_owned()),
        ),
        Err(err) => (None, Some(format!("no session: {err:#}"))),
    };
    // The boot function may be called more than once; only the first gets
    // the store.
    let boot = Mutex::new(Some((start, store, notice)));
    let mut app = iced::daemon(
        move || {
            let (start, store, notice) = boot
                .lock()
                .ok()
                .and_then(|mut b| b.take())
                .unwrap_or_default();
            let (mut app, task) = App::new(start, store);
            if notice.is_some() {
                app.notice = notice;
            }
            (app, task)
        },
        App::update_timed,
        App::view_timed,
    )
    .title(App::title)
    .theme(App::theme)
    .subscription(App::subscription)
    .default_font(ui::mono_font());
    for font in scrive_iced::required_fonts() {
        app = app.font(*font);
    }
    app.run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Editor(u64, scrive_iced::Event),
    Worker(worker::Event),
    Paper(u64, paper::Message),
    Resized(pane_grid::ResizeEvent),
    /// A click chose this area.
    Focus(pane_grid::Pane),
    ScaleFactor(window::Id, f32),
    Command(Command),
    /// A shortcut from this window. The picker opens there, so the keys
    /// that follow reach its field.
    Keyed(window::Id, Command),
    Select(usize),
    Close(usize),
    /// Jump to a zero-based line of the active document.
    JumpTo(usize),
    /// Open a file in a tab (or show the tab it is in).
    Open(PathBuf),
    /// Open or close one folder of the project tree. The path is relative
    /// to the project folder.
    ToggleDir(PathBuf),
    /// The diagrams found in a folder, for the file picker.
    Scanned(PathBuf, Vec<PathBuf>),
    /// The state machines and all diagrams found in a project folder.
    Specs(PathBuf, Arc<Specs>, Arc<omaspec_render::Atlas>),
    /// The same from a project's index, with the folders it looked into
    /// and its documents.
    Indexed(PathBuf, crate::index::Snapshot),
    /// A click on the trail over the diagram: back to that step.
    BackTo(usize),
    /// A click on a folder in the trail over a project's map: that
    /// folder's map, by its path in the project.
    MapFolder(String),
    PromptInput(String),
    PromptSubmit,
    /// A click on a look of the export prompt.
    PromptLook(ExportLook),
    PickerInput(String),
    PickerSubmit,
    ReplaceInput(String),
    /// Replace every match in every tab.
    ReplaceSubmit,
    PickerClick(usize),
    /// The window got the focus back: files may have come or gone.
    Focused,
    Copied(Result<&'static str, String>),
    Exported(Result<String, String>),
    /// What a project's export wrote, or why it wrote nothing.
    ProjectExported(Result<String, String>),
    /// Commits of the file in a tab, for the compare picker.
    Revisions(u64, Vec<(String, String)>),
    /// Revisions of the project at this folder a tab is in, for the compare
    /// picker.
    ProjectRevisions(u64, PathBuf, Vec<(String, String)>),
    /// What changed in the project at this folder since a revision (the
    /// revision and its label), or why that is not told.
    ProjectChanges(
        PathBuf,
        String,
        String,
        Result<Vec<omaspec_model::DiagramChanges>, String>,
    ),
    /// The recorded traces of a tab's state machine: label, file, object.
    Traces(u64, Vec<(String, PathBuf, String)>),
    /// The text of a version to compare a tab with.
    Version(u64, Target, Result<String, String>),
    CloseRequested(window::Id),
    /// A frame while something moves; drawing again is all it does.
    Frame,
    /// The mouse came onto a tab (`true`) or left it.
    HoverTab(usize, bool),
    /// A vertical wheel over the tab bar. The bar scrolls sideways by this
    /// many pixels.
    ScrollTabs(f32),
    /// The window has this size now.
    WindowSized(window::Id, Size),
    /// The right mouse button at this point of the main window, over the
    /// editor: its context menu opens there (docs/adr/0025).
    ContextAt(table::Spot, iced::Point),
    /// A click on a root of the menu bar: its menu opens, or closes when
    /// it is the one open.
    OpenMenu(table::Menu),
    /// The mouse came onto a root while another menu of the bar is open:
    /// that one opens instead.
    HoverMenu(table::Menu),
    /// The mouse came onto this row of the open menu.
    MenuPoint(usize),
    /// The mouse came onto this entry of the child menu open.
    MenuPointChild(usize),
    /// A click on an entry of a menu.
    MenuRun(Command),
    /// A key went down or up in this window; a tap on Alt alone shows and
    /// hides the menu bar.
    Key(window::Id, iced::keyboard::Event),
    GalleryInput(String),
    /// Show only this group of examples, or all.
    GalleryGroup(Option<crate::gallery::Group>),
    /// Show only the complex examples, or all again.
    GalleryComplex,
    /// Open this example (or the chosen one); and play it with `true`.
    GalleryOpen(Option<usize>, bool),
    /// The mouse came onto an example's tile (`true`) or left it.
    GalleryHover(usize, bool),
    /// An example's picture for its tile, drawn for the look named.
    Thumb(u64, usize, Result<gallery::Thumb, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Palette,
    /// An empty page takes the template of this language.
    StartFrom(Language),
    Save,
    SaveAs,
    Revert,
    Open,
    /// Asks how, then where, and makes a project there.
    NewProject,
    Pick,
    FindAll,
    ReplaceAll,
    Compare,
    StopCompare,
    /// B as changed lines or as a diagram.
    CompareDiff,
    /// B as the changes in behavior or as a diagram.
    CompareBehavior,
    /// The tab written in the other language, in a new tab.
    Convert,
    /// Zoom and pan A and B together or apart.
    CompareSync,
    /// Between the search and the replacement.
    SwitchField,
    /// The export prompt's next look.
    NextLook,
    Format,
    /// Folds the block the caret is in.
    Collapse,
    /// Unfolds the block the caret is in.
    Expand,
    /// Folds every block.
    CollapseAll,
    /// Unfolds every block.
    ExpandAll,
    LanguageDetected,
    LanguagePlantUml,
    LanguageMermaid,
    CopyPng,
    CopySvg,
    ExportSvg,
    ExportPng,
    ExportJpeg,
    ExportPdf,
    ExportMarkdown,
    ExportZip,
    /// Asks where, then writes the project as SVGs linked to one another
    /// and a ZIP of them.
    ExportProject,
    CloseOthers,
    RenderNow,
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    ToggleSidebar,
    /// Moves the focus to the area in that direction.
    Focus(pane_grid::Direction),
    /// The diagram in a window of its own, or back.
    Detach,
    /// Puts a skin pack into the source, or takes it out.
    Skin(Option<Pack>),
    /// Renames the diagram, state or event at the caret at every use in
    /// the project.
    RenameEverywhere,
    /// Lists the diagram's labels; the one chosen lights up.
    FindInDiagram,
    /// Takes the bends of the line at the caret out of the source.
    ResetLine,
    /// Takes every line by hand out of the source.
    ResetLines,
    /// Focuses the diagram on the box at the caret, or ends the focus.
    FocusBox,
    /// Folds the container at the caret to its box, or unfolds it.
    FoldBox,
    /// Unfolds every container of the diagram.
    UnfoldAll,
    /// Colors the diagram with the active Omarchy theme, without writing
    /// that into the file. Again uses the colors the file already has.
    FollowTheme,
    /// Opens the gallery of examples, or closes it.
    Gallery,
    /// Opens the example chosen in the gallery; and plays it with `true`.
    OpenExample(bool),
    /// The gallery's next group of examples.
    NextGroup,
    PreviousGroup,
    /// Opens the Help Omaspec folder and its menu diagram.
    OpenHelp,
    /// Back to the folder before Help Omaspec.
    LeaveHelp,
    /// Back out of a diagram a link was followed into.
    Back,
    Quit,
    Cancel,
    Up,
    Down,
    Left,
    Right,
    ZoomIn,
    ZoomOut,
    ActualSize,
    Fit,
    /// Plays the diagram's state machine or scenario, or stops it.
    Play,
    /// Plays a recorded trace of the state machine on screen.
    PlayTrace,
    /// A step of the play.
    PlayAsk(paper::Ask),
    StopPlay,
    /// Enter in a menu: the entry the highlight is on.
    Choose,
    /// The menu bar shows or hides; the session keeps it.
    ToggleMenuBar,
    /// The toolbar over the diagram shows or hides; the session keeps it.
    ToggleToolbar,
    /// This menu of the bar opens (F10: File).
    OpenMenu(table::Menu),
}

/// Images larger than this are not searched for a diagram.
const MAX_IMAGE: u64 = 64 << 20;

/// What the keyboard is busy with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Editing,
    Prompt,
    Picker,
    /// The gallery of examples: the arrows move between its tiles.
    Gallery,
    /// A play runs in the focused diagram: digits take its options.
    Playing,
    /// A menu is open: the arrows move in it, Enter chooses, any other
    /// key closes it.
    Menu,
}

/// The window's keys; Escape closes the prompt or the picker, the arrows
/// move in the picker.
fn shortcut(key: &Key, mods: Modifiers, mode: Mode) -> Option<Command> {
    if mode == Mode::Menu {
        return Some(match key.as_ref() {
            Key::Named(Named::ArrowUp) => Command::Up,
            Key::Named(Named::ArrowDown) => Command::Down,
            Key::Named(Named::ArrowLeft) => Command::Left,
            Key::Named(Named::ArrowRight) => Command::Right,
            Key::Named(Named::Enter) => Command::Choose,
            Key::Named(Named::F10) => Command::Cancel,
            Key::Named(
                Named::Shift
                | Named::Control
                | Named::Alt
                | Named::AltGraph
                | Named::Super
                | Named::CapsLock,
            ) => return None,
            _ => Command::Cancel,
        });
    }
    let ctrl = mods.control() && !mods.alt();
    let alt = mods.alt() && !mods.control();
    let plain = !mods.control() && !mods.alt() && !mods.logo();
    let ctrl_alt = mods.control() && mods.alt() && !mods.shift();
    let playing = mode == Mode::Playing && plain;
    let gallery = mode == Mode::Gallery;
    let command = match key.as_ref() {
        Key::Named(Named::F5) if gallery => Some(Command::OpenExample(true)),
        Key::Named(Named::Enter) if gallery && !mods.control() => {
            Some(Command::OpenExample(mods.shift()))
        }
        Key::Named(Named::ArrowLeft) if gallery && plain => Some(Command::Left),
        Key::Named(Named::ArrowRight) if gallery && plain => Some(Command::Right),
        Key::Named(Named::Tab) if gallery && !mods.control() => Some(if mods.shift() {
            Command::PreviousGroup
        } else {
            Command::NextGroup
        }),
        Key::Named(Named::Tab) if mode == Mode::Prompt && !mods.control() => {
            Some(Command::NextLook)
        }
        Key::Named(Named::F5) if !matches!(mode, Mode::Prompt | Mode::Picker) => {
            Some(Command::Play)
        }
        Key::Named(Named::F2) if plain && mode == Mode::Editing => Some(Command::RenameEverywhere),
        Key::Named(Named::Escape) if mode == Mode::Playing => Some(Command::StopPlay),
        Key::Character(c)
            if playing && matches!(c, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9") =>
        {
            let n = c.parse::<usize>().unwrap_or(1);
            Some(Command::PlayAsk(paper::Ask::Take(n - 1)))
        }
        Key::Named(Named::Space | Named::ArrowRight | Named::Enter) if playing => {
            Some(Command::PlayAsk(paper::Ask::Next))
        }
        Key::Named(Named::ArrowLeft | Named::Backspace) if playing => {
            Some(Command::PlayAsk(paper::Ask::Back))
        }
        Key::Named(Named::Home) if playing => Some(Command::PlayAsk(paper::Ask::Restart)),
        Key::Named(Named::Escape) if mode != Mode::Editing => Some(Command::Cancel),
        Key::Named(Named::F10) if plain && mode == Mode::Editing => {
            Some(Command::OpenMenu(table::Menu::File))
        }
        Key::Named(Named::ArrowUp) if plain && (mode == Mode::Picker || gallery) => {
            Some(Command::Up)
        }
        Key::Named(Named::ArrowDown) if plain && (mode == Mode::Picker || gallery) => {
            Some(Command::Down)
        }
        Key::Named(Named::ArrowLeft) if alt && !mods.shift() && mode == Mode::Editing => {
            Some(Command::Back)
        }
        Key::Named(Named::ArrowLeft) if ctrl_alt => {
            Some(Command::Focus(pane_grid::Direction::Left))
        }
        Key::Named(Named::ArrowRight) if ctrl_alt => {
            Some(Command::Focus(pane_grid::Direction::Right))
        }
        Key::Named(Named::ArrowUp) if ctrl_alt => Some(Command::Focus(pane_grid::Direction::Up)),
        Key::Named(Named::ArrowDown) if ctrl_alt => {
            Some(Command::Focus(pane_grid::Direction::Down))
        }
        Key::Named(Named::Tab) if !mods.control() && mode == Mode::Picker => {
            Some(Command::SwitchField)
        }
        Key::Named(Named::Enter) if ctrl => Some(Command::RenderNow),
        Key::Named(Named::PageDown) if ctrl => Some(Command::NextTab),
        Key::Named(Named::PageUp) if ctrl => Some(Command::PreviousTab),
        Key::Named(Named::Tab) if ctrl && mods.shift() => Some(Command::PreviousTab),
        Key::Named(Named::Tab) if ctrl => Some(Command::NextTab),
        Key::Character(c) if ctrl => match c.to_ascii_lowercase().as_str() {
            "s" if mods.shift() => Some(Command::SaveAs),
            "p" if mods.shift() => Some(Command::Palette),
            "f" if mods.shift() => Some(Command::FindAll),
            "h" if mods.shift() => Some(Command::ReplaceAll),
            "d" if mods.shift() => Some(Command::Compare),
            "e" if mods.shift() => Some(Command::Gallery),
            "i" if mods.shift() => Some(Command::Format),
            "c" if mods.shift() => Some(Command::CopyPng),
            "s" => Some(Command::Save),
            "o" => Some(Command::Open),
            "p" => Some(Command::Pick),
            "n" if mods.shift() => Some(Command::Detach),
            "n" => Some(Command::NewTab),
            "w" => Some(Command::CloseTab),
            "b" => Some(Command::ToggleSidebar),
            "q" => Some(Command::Quit),
            "[" if mods.shift() => Some(Command::Collapse),
            "]" if mods.shift() => Some(Command::Expand),
            _ => None,
        },
        Key::Character(c) if alt => match c.to_ascii_lowercase().as_str() {
            "=" | "+" => Some(Command::ZoomIn),
            "-" => Some(Command::ZoomOut),
            "0" => Some(Command::ActualSize),
            "f" => Some(Command::Fit),
            _ => None,
        },
        _ => None,
    };
    command.filter(|command| shortcut_reaches(*command, mode))
}

/// The prompt and the picker keep their own keys. Area keys belong to editing.
fn shortcut_reaches(command: Command, mode: Mode) -> bool {
    if mode == Mode::Prompt && blocks_while_asking(command) {
        return false;
    }
    mode == Mode::Editing || !matches!(command, Command::Focus(_))
}

/// Close, save, tab and quit keys stay out of the prompt, so Enter still
/// answers the question that is open.
fn blocks_while_asking(command: Command) -> bool {
    matches!(
        command,
        Command::CloseTab
            | Command::CloseOthers
            | Command::NextTab
            | Command::PreviousTab
            | Command::NewTab
            | Command::Save
            | Command::SaveAs
            | Command::Open
            | Command::NewProject
            | Command::Quit
    )
}

/// One area of the grid. The project tree sits left of the source, the
/// diagram on the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Files,
    Source,
    Diagram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asking {
    SaveAs,
    Open,
    /// The folder of a new project, its first diagram in this language.
    NewProject {
        language: Language,
        git: bool,
    },
    /// Where to write this export. Enter writes it; an existing file asks
    /// once more.
    Export(Format),
    /// The folder to export the project into. A folder that is there asks
    /// once more.
    ExportProject,
    /// The new name of what the caret names ([`App::renaming`]).
    Rename,
}

/// A question in the bottom line.
struct Prompt {
    asking: Asking,
    input: String,
    /// The file named exists; Enter again overwrites it.
    overwrite: bool,
    /// The tab that opened the prompt. Enter saves that tab.
    doc: u64,
    /// What an export is drawn in; Tab takes the next.
    look: ExportLook,
}

/// What an export is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportLook {
    /// As the diagram pane shows it.
    Shown,
    /// In the file's own colors.
    File,
    /// In a look laid over the file's colors ([`crate::look`]).
    Look(crate::look::Look),
}

impl ExportLook {
    pub fn label(self) -> &'static str {
        match self {
            ExportLook::Shown => "as shown",
            ExportLook::File => "file's colors",
            ExportLook::Look(look) => look.name(),
        }
    }
}

/// How long the tab highlight and the area frame take to move over.
const TAB_FADE: Duration = Duration::from_millis(160);

/// Help Omaspec in the sidebar, in place of the project folder.
struct Help {
    dir: PathBuf,
    /// The folder before it, which leaving brings back.
    back: Option<PathBuf>,
}

struct App {
    docs: Vec<Doc>,
    active: usize,
    next_key: u64,
    forced: Option<Language>,
    worker: Option<Worker>,
    /// Where the session goes; `None` when another window keeps it.
    store: Option<Store>,

    /// The folder in the sidebar and the picker.
    folder: Option<PathBuf>,
    /// The diagrams in it, relative to it. The file picker uses this; the
    /// sidebar draws `tree` instead.
    files: Vec<PathBuf>,
    /// Project folders the tree has open, relative to `folder`.
    open_dirs: HashSet<PathBuf>,
    /// Immediate children of a relative directory. The project root is an
    /// empty path.
    tree: HashMap<PathBuf, folder::Listing>,
    /// The state machines of each project folder an open diagram is in,
    /// which sequence diagrams are checked against.
    specs: HashMap<PathBuf, Arc<Specs>>,
    /// All diagrams of each such project folder, which maps show and
    /// states link to; and the same in the diagram's colors.
    atlases: HashMap<PathBuf, Arc<omaspec_render::Atlas>>,
    themed_atlases: HashMap<PathBuf, Arc<omaspec_render::Atlas>>,
    /// Each such project folder's index, which reads again only the
    /// files that changed; shared with the refresh under way.
    indexes: HashMap<PathBuf, Arc<Mutex<crate::index::Index>>>,
    /// The folders of each such project, and of all of them those watched.
    project_dirs: HashMap<PathBuf, Arc<Vec<PathBuf>>>,
    /// What the rename prompt renames.
    renaming: Option<rename::Renaming>,
    /// The looks the projects pin.
    looks: crate::look::Pinned,
    /// The Markdown documents of each project open, with their texts, for
    /// the search; its diagrams' texts are in `atlases`.
    documents: HashMap<PathBuf, Arc<Vec<(String, String)>>>,
    watched: Arc<Vec<PathBuf>>,
    /// A link being followed, while the zoom into it runs.
    following: Option<links::Following>,
    /// The diagrams followed into, from the first.
    trail: Vec<links::Step>,
    /// Set while the sidebar shows Help Omaspec.
    help: Option<Help>,
    /// The name `omaspec-project.toml` gives the sidebar's folder, when it
    /// is a project's root.
    project_name: Option<String>,

    theme_name: String,
    palette: Palette,
    theme: Theme,
    look: Look,
    font: Font,

    panes: pane_grid::State<Pane>,
    /// The area a click or Ctrl+Alt+arrows last chose.
    focus: Pane,
    /// The diagram pane's size, for a document that comes into view.
    paper_size: Size,
    /// The editor window, and the diagram's own window while it has one.
    main: window::Id,
    detached: Option<window::Id>,
    main_scale: f32,
    detached_scale: f32,

    prompt: Option<Prompt>,
    picker: Option<Picker>,
    /// The window the picker is drawn in. It has to be the one that has
    /// the keyboard, or typing never reaches the field.
    picker_on: Option<window::Id>,
    /// The window whose keys are running a command, for that call only.
    command_window: Option<window::Id>,
    /// The gallery of examples over the editor window, while it is open.
    gallery: Option<gallery::Gallery>,
    /// The pictures of its tiles, kept for the next time.
    thumbs: gallery::Thumbs,
    /// The editor window's size, which sets how many tiles fit in a row.
    main_size: Size,
    detached_size: Size,
    /// The menu open in one of the windows (docs/adr/0025).
    menu: Option<menu::Open>,
    /// The menu bar is shown; the session keeps it.
    bar_shown: bool,
    /// The toolbar over the diagram is shown with the bar; the session
    /// keeps it.
    toolbar: bool,
    /// Alt went down and no other key since: letting it go shows or hides
    /// the bar.
    alt_armed: bool,
    /// An example opened to be played: its tab plays once it is drawn.
    play_when_drawn: Option<u64>,
    /// The diagram takes the active Omarchy colors at render time. The
    /// file stays as it is.
    follow_theme: bool,
    /// Render requests whose picture was colored on top of the file, so
    /// the answer can be read back against the file's lines.
    pending: HashMap<(u64, u64), Pending>,
    /// The mark in the status line.
    mark: iced::widget::image::Handle,
    /// The mark, larger, for the diagram pane while it is empty.
    mark_large: iced::widget::image::Handle,
    /// The icons of the tabs, the sidebar and the status line.
    icons: Icons,
    /// The file picker has been used: the sidebar no longer points to it.
    picked: bool,
    /// Commands chosen from the palette lately, the latest first.
    recent: Vec<Command>,
    /// Compare A|B, for one tab.
    compare: Option<Compare>,
    notice: Option<String>,
    /// Files reported changed, read once the writer is done.
    changed: Vec<PathBuf>,
    /// A close was asked with unsaved changes; the next one discards them.
    leaving: bool,
    /// The document a Ctrl+W was refused for; the next one closes it.
    closing: Option<u64>,
    /// The active tab's name takes the accent color from this; the tab it
    /// left (`left_tab`) gives it back at the same time.
    tab_fade: Animation<bool>,
    left_tab: Option<u64>,
    /// The tab under the mouse.
    hovered_tab: Option<usize>,
    /// The focused area's frame takes the accent color from this; the
    /// area it left (`left_focus`) gives it back at the same time.
    focus_fade: Animation<bool>,
    left_focus: Option<Pane>,
}

impl App {
    fn new(start: Start, store: Option<Store>) -> (Self, Task<Message>) {
        Self::themed(start, store, Omarchy::load_current())
    }

    /// The window in the colors of `omarchy`.
    fn themed(start: Start, store: Option<Store>, omarchy: Omarchy) -> (Self, Task<Message>) {
        let palette = omarchy.palette.legible();
        let theme_name = omarchy.name.unwrap_or_else(|| "built-in".into());
        let font = ui::mono_font();
        let look = Look::new(&palette, &omarchy.tweaks, font);
        let icons = icons_for(&palette, 1.0);
        let session = store.as_ref().and_then(Store::load).unwrap_or_default();
        let (main, open) = window::open(window::Settings {
            icon: crate::logo::icon(),
            platform_specific: ui::app_id("omaspec"),
            exit_on_close_request: false,
            ..Default::default()
        });
        let mut app = Self {
            docs: Vec::new(),
            active: 0,
            next_key: 1,
            forced: start.forced,
            worker: None,
            store,
            folder: None,
            files: Vec::new(),
            open_dirs: HashSet::new(),
            tree: HashMap::new(),
            specs: HashMap::new(),
            atlases: HashMap::new(),
            themed_atlases: HashMap::new(),
            indexes: HashMap::new(),
            project_dirs: HashMap::new(),
            renaming: None,
            looks: crate::look::Pinned::default(),
            documents: HashMap::new(),
            watched: Arc::default(),
            following: None,
            trail: Vec::new(),
            help: None,
            project_name: None,
            theme: ui::iced_theme(&theme_name, &palette),
            theme_name,
            palette,
            look,
            font,
            panes: layout(session.sidebar, true),
            focus: Pane::Source,
            paper_size: Size::ZERO,
            main,
            detached: None,
            main_scale: 1.0,
            detached_scale: 1.0,
            prompt: None,
            picker: None,
            picker_on: None,
            command_window: None,
            gallery: None,
            thumbs: gallery::Thumbs::default(),
            main_size: window::Settings::default().size,
            detached_size: window::Settings::default().size,
            menu: None,
            bar_shown: session.menu_bar,
            toolbar: session.toolbar,
            alt_armed: false,
            play_when_drawn: None,
            follow_theme: session.follow_theme,
            pending: HashMap::new(),
            mark: crate::logo::handle(),
            mark_large: crate::logo::large(),
            icons,
            picked: session.picked,
            recent: Vec::new(),
            compare: None,
            notice: None,
            changed: Vec::new(),
            leaving: false,
            closing: None,
            tab_fade: Animation::new(true),
            left_tab: None,
            hovered_tab: None,
            focus_fade: Animation::new(true),
            left_focus: None,
        };

        // The active tab, counted among those that come back.
        let mut active = 0;
        for (i, saved) in session.docs.iter().enumerate() {
            if let Some(doc) = app.restore(saved) {
                app.docs.push(doc);
            }
            if i < session.active {
                active = app.docs.len();
            }
        }
        app.active = active.min(app.docs.len().saturating_sub(1));
        let mut problems = Vec::new();
        let mut first = None;
        for file in &start.files {
            match app.open_doc(file) {
                Ok(index) => {
                    first.get_or_insert(index);
                }
                Err(err) => problems.push(format!("{}: {err:#}", file.display())),
            }
        }
        if app.docs.is_empty() {
            let key = app.take_key();
            app.docs.push(Doc::untitled(key, app.forced, &app.look));
        }
        if let Some(index) = first {
            app.active = index;
        }
        if !problems.is_empty() {
            let more = problems.join("; ");
            app.notice = Some(match app.notice.take() {
                Some(earlier) => format!("{earlier}; {more}"),
                None => more,
            });
        }

        let from_files = start
            .files
            .first()
            .and_then(|f| std::path::absolute(f).ok())
            .and_then(|f| f.parent().map(Path::to_owned));
        let folder = start
            .folder
            .and_then(|f| std::path::absolute(f).ok())
            .or(session.folder)
            .or(from_files)
            .or_else(|| start.files.is_empty().then(working_folder).flatten());

        let mut focus = Some(app.focus_editor());
        let mut reveal = Some(app.reveal_tab());
        let shown = open.then(move |id| {
            let scale = window::scale_factor(id).map(move |f| Message::ScaleFactor(id, f));
            Task::batch([
                scale,
                focus.take().unwrap_or_else(Task::none),
                reveal.take().unwrap_or_else(Task::none),
            ])
        });
        let scan = app.set_folder(folder);
        // A session may still name the help folder; leaving it then closes it.
        if let Some(dir) = app.folder.clone().filter(|dir| help::is_help_dir(dir)) {
            app.help = Some(Help { dir, back: None });
        }
        (app, Task::batch([shown, scan]))
    }

    fn take_key(&mut self) -> u64 {
        self.next_key += 1;
        self.next_key - 1
    }

    fn doc(&self) -> &Doc {
        &self.docs[self.active]
    }

    fn doc_mut(&mut self) -> &mut Doc {
        &mut self.docs[self.active]
    }

    fn find(&mut self, key: u64) -> Option<&mut Doc> {
        self.docs.iter_mut().find(|d| d.key == key)
    }

    /// What this window's keys are busy with. The picker and the prompt
    /// each belong to one window; the other keeps its editing keys.
    fn mode(&self, window: window::Id) -> Mode {
        if self.gallery.is_some() && window == self.main {
            Mode::Gallery
        } else if self.menu.as_ref().is_some_and(|m| m.on == window) {
            Mode::Menu
        } else if self.picker.is_some() && self.picker_on == Some(window) {
            Mode::Picker
        } else if self.prompt.is_some() && window == self.main {
            Mode::Prompt
        } else if self.doc().paper.playing()
            && (self.focus == Pane::Diagram || Some(window) == self.detached)
        {
            Mode::Playing
        } else {
            Mode::Editing
        }
    }

    /// A tap on Alt alone, down and up with no other key between, shows
    /// or hides the menu bar; Alt with a key (Alt+F fits) does not.
    fn alt_tap(&mut self, event: &iced::keyboard::Event) {
        use iced::keyboard::Event;
        match event {
            Event::KeyPressed {
                key: Key::Named(Named::Alt),
                modifiers,
                ..
            } => self.alt_armed = (*modifiers - Modifiers::ALT).is_empty(),
            Event::KeyPressed { .. } => self.alt_armed = false,
            Event::KeyReleased {
                key: Key::Named(Named::Alt),
                ..
            } if std::mem::take(&mut self.alt_armed) => self.toggle_menu_bar(),
            _ => {}
        }
    }

    fn title(&self, window: window::Id) -> String {
        let doc = self.doc();
        let dot = if doc.modified { " ●" } else { "" };
        if Some(window) == self.detached {
            format!("Omaspec · {} · diagram", doc.name())
        } else {
            format!("Omaspec · {}{dot}", doc.name())
        }
    }

    fn theme(&self, _window: window::Id) -> Theme {
        self.theme.clone()
    }

    /// The scale factor of the window the diagram is in.
    fn paper_scale(&self) -> f32 {
        if self.detached.is_some() {
            self.detached_scale
        } else {
            self.main_scale
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let theme_dir = omaspec_theme::current_dir().filter(|d| d.is_dir());
        let mut files: Vec<PathBuf> = self.docs.iter().filter_map(|d| d.path.clone()).collect();
        // A look the project pins is read again when its file changes.
        files.extend(
            self.docs
                .iter()
                .filter_map(|d| self.project_of(d))
                .map(|root| root.join(omaspec_core::project::FILE)),
        );
        files.sort();
        files.dedup();
        let doc = self.doc();
        Subscription::batch([
            worker::service(worker::Watched {
                theme_dir,
                font_dir: ui::fontconfig_dir(),
            })
            .map(Message::Worker),
            worker::watch(files).map(Message::Worker),
            worker::watch_projects(Arc::clone(&self.watched)).map(Message::Worker),
            doc.editor
                .subscription()
                .with(doc.key)
                .map(|(key, event)| Message::Editor(key, event)),
            iced::event::listen_with(|event, _, window| match event {
                iced::Event::Keyboard(event) => Some(Message::Key(window, event)),
                _ => None,
            }),
            window::events().filter_map(|(id, event)| match event {
                window::Event::Rescaled(factor) => Some(Message::ScaleFactor(id, factor)),
                window::Event::Resized(size) => Some(Message::WindowSized(id, size)),
                window::Event::CloseRequested => Some(Message::CloseRequested(id)),
                window::Event::Focused => Some(Message::Focused),
                window::Event::FileDropped(path) => Some(Message::Open(path)),
                _ => None,
            }),
            if self.animating(Instant::now()) {
                window::frames().map(|_| Message::Frame)
            } else {
                Subscription::none()
            },
        ])
    }

    /// [`Self::update`], noted for `OMASPEC_FRAMES` when it takes long.
    fn update_timed(&mut self, message: Message) -> Task<Message> {
        if !paper::frames::on() {
            return self.update(message);
        }
        let what = paper::frames::short(&message);
        let start = Instant::now();
        let task = self.update(message);
        paper::frames::took(|| format!("update of {what}"), start.elapsed());
        task
    }

    /// [`Self::view`], noted for `OMASPEC_FRAMES` when it takes long.
    fn view_timed(&self, window: iced::window::Id) -> iced::Element<'_, Message> {
        let start = Instant::now();
        let view = self.view(window);
        paper::frames::took(|| "the view".to_owned(), start.elapsed());
        view
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Editor(key, event) => self.editor_event(key, event),
            Message::Worker(event) => self.worker_event(event),
            Message::Paper(compare::KEY, message) => self.compare_paper(message),
            Message::Paper(key, paper::Message::Pick(at)) => self.pick_in_diagram(key, at),
            Message::Paper(key, paper::Message::Follow(at)) => self.follow_link(key, at),
            Message::Paper(key, paper::Message::Context(at, in_window)) => {
                self.context_in_diagram(key, at, in_window)
            }
            Message::ContextAt(spot, at) => {
                self.context_at(spot, at);
                Task::none()
            }
            Message::OpenMenu(menu) => {
                self.open_bar_menu(menu);
                Task::none()
            }
            Message::HoverMenu(menu) => {
                self.switch_bar_menu(menu);
                Task::none()
            }
            Message::MenuPoint(i) => {
                self.menu_point(i);
                Task::none()
            }
            Message::MenuPointChild(i) => {
                self.menu_point_child(i);
                Task::none()
            }
            Message::MenuRun(command) => {
                self.close_menu();
                self.run(command)
            }
            Message::Key(window, event) => {
                if window == self.main {
                    self.alt_tap(&event);
                }
                Task::none()
            }
            Message::Paper(key, message) => {
                if let paper::Message::Viewport(size) = message {
                    self.paper_size = size;
                }
                let step = matches!(message, paper::Message::Play(_));
                let relight = key != compare::KEY && matches!(message, paper::Message::PlayGone);
                let moves = paper::Paper::moves_view(&message);
                let (task, by_hand, held, refold) = match self.find(key) {
                    Some(doc) => (
                        doc.paper
                            .update(message)
                            .map(move |m| Message::Paper(key, m)),
                        doc.paper.take_by_hand(),
                        doc.paper.take_held(),
                        doc.paper.take_refold(),
                    ),
                    None => (Task::none(), None, None, false),
                };
                // A container folded or unfolded in the pane: drawn again.
                if refold {
                    return Task::batch([task, self.render(key, true)]);
                }
                if let Some(line) = by_hand {
                    return Task::batch([task, self.draw_by_hand(key, &line)]);
                }
                if let Some((name, hold)) = held {
                    return Task::batch([task, self.hold_box(key, &name, hold.as_ref())]);
                }
                // The diagram a zoom leads to got its picture.
                let arrived = if self.is_arriving(key) {
                    self.arrive()
                } else {
                    Task::none()
                };
                let follow = match &mut self.compare {
                    Some(c) if moves && c.sync && c.a == key => {
                        let view = self
                            .docs
                            .iter()
                            .find(|d| d.key == key)
                            .map(|d| d.paper.view);
                        view.map_or_else(Task::none, |v| c.paper.follow(v))
                            .map(|m| Message::Paper(compare::KEY, m))
                    }
                    _ => Task::none(),
                };
                if step {
                    self.light_played_line(key);
                }
                if relight {
                    self.follow_caret(key);
                }
                Task::batch([task, follow, arrived])
            }
            Message::Resized(event) => {
                self.panes.resize(event.split, event.ratio);
                Task::none()
            }
            Message::Focus(pane) => self.focus_pane(pane),
            Message::ScaleFactor(window, factor) => {
                if Some(window) == self.detached {
                    self.detached_scale = factor;
                } else {
                    self.main_scale = factor;
                    self.icons = icons_for(&self.palette, factor);
                }
                let thumbs = self.draw_thumbs();
                let factor = self.paper_scale();
                let key = self.doc().key;
                let b = match &mut self.compare {
                    Some(c) => c
                        .paper
                        .set_scale_factor(factor)
                        .map(|m| Message::Paper(compare::KEY, m)),
                    None => Task::none(),
                };
                let a = self
                    .doc_mut()
                    .paper
                    .set_scale_factor(factor)
                    .map(move |m| Message::Paper(key, m));
                Task::batch([a, b, thumbs])
            }
            Message::Command(command) => self.run(command),
            Message::Keyed(window, command) => {
                self.command_window = Some(window);
                let task = self.run(command);
                self.command_window = None;
                task
            }
            Message::Select(index) => self.select(index),
            Message::Close(index) => self.close(index),
            Message::Frame => {
                let now = Instant::now();
                for doc in &mut self.docs {
                    doc.fade_flash(now);
                }
                // A play's line lights when the light of its step sets off.
                let waiting: Vec<u64> = self
                    .docs
                    .iter()
                    .filter(|doc| doc.paper.line_waiting())
                    .map(|doc| doc.key)
                    .collect();
                for key in waiting {
                    self.light_played_line(key);
                }
                Task::none()
            }
            Message::HoverTab(index, true) => {
                self.hovered_tab = Some(index);
                Task::none()
            }
            Message::HoverTab(index, false) => {
                // Leaving one tab may come after entering the next.
                if self.hovered_tab == Some(index) {
                    self.hovered_tab = None;
                }
                Task::none()
            }
            Message::ScrollTabs(x) => iced::widget::operation::scroll_by(
                TABS,
                iced::widget::operation::AbsoluteOffset { x, y: 0.0 },
            ),
            Message::WindowSized(window, size) => {
                if Some(window) == self.detached {
                    self.detached_size = size;
                }
                if window == self.main {
                    self.main_size = size;
                    self.gallery_resized();
                }
                Task::none()
            }
            Message::GalleryInput(query) => {
                if let Some(gallery) = &mut self.gallery {
                    gallery.set_query(query);
                }
                self.reveal_chosen()
            }
            Message::GalleryGroup(group) => {
                if let Some(gallery) = &mut self.gallery {
                    gallery.set_group(group);
                }
                Task::batch([
                    iced::widget::operation::focus(gallery::FIELD),
                    self.reveal_chosen(),
                ])
            }
            Message::GalleryComplex => {
                if let Some(gallery) = &mut self.gallery {
                    gallery.toggle_complex();
                }
                Task::batch([
                    iced::widget::operation::focus(gallery::FIELD),
                    self.reveal_chosen(),
                ])
            }
            Message::GalleryOpen(Some(index), play) => self.open_example(index, play),
            Message::GalleryOpen(None, play) => self.open_chosen(play),
            Message::GalleryHover(index, inside) => {
                if let Some(gallery) = &mut self.gallery {
                    gallery.hover(index, inside);
                }
                Task::none()
            }
            Message::Thumb(look, index, thumb) => {
                self.thumb_came(look, index, thumb);
                Task::none()
            }
            Message::JumpTo(line) => self.jump_to(line),
            Message::Open(path) => self.open(&path),
            Message::ToggleDir(dir) => {
                self.toggle_dir(dir);
                Task::none()
            }
            Message::Scanned(folder, files) => {
                if self.folder.as_ref() == Some(&folder) {
                    self.files = files;
                    self.fill_tree();
                }
                Task::none()
            }
            Message::Indexed(root, found) => {
                self.found_dirs(root.clone(), found.dirs);
                if self.indexes.contains_key(&root) {
                    self.documents.insert(root.clone(), found.documents);
                }
                self.update(Message::Specs(root, found.specs, found.atlas))
            }
            Message::Specs(root, specs, atlas) => {
                self.found_specs(root.clone(), specs);
                if root.as_os_str().is_empty() {
                    return Task::none();
                }
                self.found_atlas(root, atlas)
            }
            Message::BackTo(index) => self.back_to(index),
            Message::MapFolder(folder) => {
                let key = self.doc().key;
                self.show_folder(key, folder)
            }
            Message::PromptInput(input) => {
                if let Some(prompt) = &mut self.prompt {
                    prompt.input = input;
                    prompt.overwrite = false;
                }
                Task::none()
            }
            Message::PromptLook(look) => {
                self.next_export_look(Some(look));
                Task::none()
            }
            Message::PromptSubmit => match self.prompt.as_ref().map(|p| p.asking) {
                Some(Asking::SaveAs) => self.save_as_submitted(),
                Some(Asking::Open) => self.open_submitted(),
                Some(Asking::NewProject { language, git }) => {
                    self.new_project_submitted(language, git)
                }
                Some(Asking::Export(_)) => self.export_submitted(),
                Some(Asking::ExportProject) => self.export_project_submitted(),
                Some(Asking::Rename) => self.rename_submitted(),
                None => Task::none(),
            },
            Message::PickerInput(query) => {
                let search = self
                    .picker
                    .as_ref()
                    .filter(|p| p.kind == Kind::Search)
                    .map(|p| p.replace.is_none());
                let hits = search.map(|project| self.search_items(&query, project));
                if let Some(picker) = &mut self.picker {
                    picker.set_query(query);
                    if let Some(hits) = hits {
                        picker.set_items(hits);
                    }
                }
                Task::none()
            }
            Message::ReplaceInput(with) => {
                if let Some(replace) = self.picker.as_mut().and_then(|p| p.replace.as_mut()) {
                    *replace = with;
                }
                Task::none()
            }
            Message::ReplaceSubmit => self.replace_all(),
            Message::PickerSubmit => self.pick(),
            Message::PickerClick(row) => {
                if let Some(picker) = &mut self.picker {
                    picker.select(row);
                }
                self.pick()
            }
            Message::Focused => {
                self.fill_tree();
                Task::batch([self.rescan(), self.load_specs()])
            }
            Message::Copied(result) => {
                self.notice = Some(match result {
                    Ok(what) => {
                        ui::notify(
                            "Diagram copied",
                            &format!("as {what}, with its source inside"),
                        );
                        format!("copied the diagram as {what}")
                    }
                    Err(err) => format!("not copied: {err}"),
                });
                Task::none()
            }
            Message::Exported(result) => {
                self.notice = Some(match result {
                    Ok(path) => {
                        ui::notify("Diagram exported", &path);
                        format!("exported {path}")
                    }
                    Err(err) => format!("not exported: {err}"),
                });
                Task::none()
            }
            Message::ProjectExported(result) => {
                self.notice = Some(match result {
                    Ok(summary) => {
                        ui::notify("Project exported", &summary);
                        summary
                    }
                    Err(err) => format!("project not exported: {err}"),
                });
                Task::none()
            }
            Message::Revisions(key, revisions) => {
                let items: Vec<Item> = revisions
                    .into_iter()
                    .map(|(rev, label)| Item {
                        label,
                        hint: "git".into(),
                        choice: Choice::Compare(Target::Commit {
                            label: format!("git {rev}"),
                            rev,
                        }),
                    })
                    .collect();
                let current = self.doc().key == key;
                if let Some(picker) = &mut self.picker
                    && picker.kind == Kind::Versions
                    && current
                {
                    picker.add_items(items);
                }
                Task::none()
            }
            Message::ProjectRevisions(key, root, found) => {
                self.project_revisions(key, root, found);
                Task::none()
            }
            Message::ProjectChanges(root, rev, label, changes) => {
                self.project_changes(&root, &rev, &label, changes)
            }
            Message::Traces(key, found) => self.found_traces(key, found),
            Message::Version(key, target, text) => {
                match text {
                    Ok(text) => self.start_compare(key, target, text),
                    Err(err) => self.notice = Some(format!("cannot compare: {err}")),
                }
                Task::none()
            }
            Message::CloseRequested(window) if Some(window) == self.detached => self.attach(),
            Message::CloseRequested(_) => self.quit(),
        }
    }

    fn editor_event(&mut self, key: u64, event: scrive_iced::Event) -> Task<Message> {
        let worker = self.worker.clone();
        let Some(doc) = self.find(key) else {
            return Task::none();
        };
        let task = doc
            .editor
            .update(event)
            .map(move |e| Message::Editor(key, e));
        // In a Markdown document, the caret picks the diagram shown.
        let moved = doc.caret_moved();
        if doc.editor.take_dirty() {
            doc.edited();
            if let Some(worker) = worker {
                worker.after(Alarm::Lint(key), LINT_PAUSE);
                worker.after(Alarm::Render(key), RENDER_PAUSE);
            }
            self.leaving = false;
            self.closing = None;
            self.keep_session();
        }
        self.follow_caret(key);
        if moved {
            return Task::batch([task, self.render(key, false)]);
        }
        task
    }

    fn worker_event(&mut self, event: worker::Event) -> Task<Message> {
        match event {
            worker::Event::Connected(worker) => {
                self.worker = Some(worker);
                return self.render(self.doc().key, false);
            }
            worker::Event::Rendered(answer) => {
                let mut answer = *answer;
                self.untangle(&mut answer);
                let key = answer.ticket.doc;
                if key == compare::KEY {
                    if let Some(c) = &mut self.compare {
                        return c
                            .paper
                            .rendered(answer)
                            .map(|m| Message::Paper(compare::KEY, m));
                    }
                } else if self.find(key).is_some() {
                    let drawn = answer.result.is_ok();
                    // What the drawing says about the lines by hand; one
                    // that failed says nothing, and an answer older than
                    // the picture on screen is dropped.
                    let said: Vec<omaspec_core::lint::Finding> = match &answer.result {
                        Ok(d) => d
                            .map
                            .misses
                            .iter()
                            .map(|(line, why)| omaspec_core::lint::Finding {
                                line: *line,
                                severity: omaspec_core::lint::Severity::Warning,
                                message: why.clone(),
                            })
                            .collect(),
                        Err(_) => Vec::new(),
                    };
                    if let Some(doc) = self.find(key)
                        && answer.ticket.id > doc.paper.shown()
                    {
                        doc.set_drawn_findings(said);
                    }
                    let task = self
                        .find(key)
                        .expect("the document")
                        .paper
                        .rendered(answer)
                        .map(move |m| Message::Paper(key, m));
                    self.follow_caret(key);
                    // The diagram a zoom into a link leads to is drawn.
                    let task = if self.is_arriving(key) {
                        Task::batch([task, self.arrive()])
                    } else {
                        task
                    };
                    // An example opened to be played starts once it is drawn.
                    if self.play_when_drawn == Some(key) {
                        self.play_when_drawn = None;
                        if drawn && self.doc().key == key && !self.doc().paper.playing() {
                            return Task::batch([task, self.toggle_play()]);
                        }
                    }
                    return task;
                }
            }
            worker::Event::Due(Alarm::Lint(key)) => {
                let changed = self.find(key).is_some_and(Doc::analyze);
                // Another language draws differently; no need to wait.
                if changed {
                    return self.render(key, false);
                }
            }
            worker::Event::Due(Alarm::Render(key)) => {
                self.compare_edited(key);
                return self.render(key, false);
            }
            worker::Event::Due(Alarm::Flash(key)) => {
                if let Some(doc) = self.find(key) {
                    doc.unflash();
                }
            }
            worker::Event::Due(Alarm::Session) => self.write_session(),
            worker::Event::Due(Alarm::Follow) => return self.followed(),
            worker::Event::Due(Alarm::Dive) => return self.dive_anyway(),
            worker::Event::Due(Alarm::Thumbs) => return self.draw_thumbs(),
            worker::Event::FileChanged(path) => {
                if !self.changed.contains(&path) {
                    self.changed.push(path);
                }
                self.after(Alarm::File, FILE_SETTLE);
            }
            worker::Event::Due(Alarm::File) => return self.files_changed(),
            worker::Event::ProjectChanged => self.after(Alarm::Project, FILE_SETTLE),
            worker::Event::Due(Alarm::Project) => return self.load_specs(),
            worker::Event::ThemeChanged => self.after(Alarm::Theme, THEME_SETTLE),
            worker::Event::FontChanged => self.after(Alarm::Font, THEME_SETTLE),
            worker::Event::Due(Alarm::Font) => self.set_font(ui::current_mono_font()),
            worker::Event::Due(Alarm::Theme) => {
                self.reload_theme();
                return self.draw_thumbs();
            }
            worker::Event::WatchFailed(err) => {
                self.notice = Some(format!("not following changes on disk: {err}"));
            }
        }
        Task::none()
    }

    fn after(&self, alarm: Alarm, delay: Duration) {
        if let Some(worker) = &self.worker {
            worker.after(alarm, delay);
        }
    }

    /// Sends the document's text to the renderer if it changed since it was
    /// last sent, or anyway with `again`.
    fn render(&mut self, key: u64, again: bool) -> Task<Message> {
        if self.worker.is_none() {
            return Task::none();
        }
        let Some(doc) = self.find(key) else {
            return Task::none();
        };
        let source = if again {
            Some(doc.source_anyway())
        } else {
            doc.source_if_new()
        };
        let Some(source) = source else {
            return Task::none();
        };
        let language = doc.language;
        doc.paper.document = doc.is_document();
        // The maps of its project show it as this tab does.
        let maps = self.retile(key);
        self.dispatch(key, &source, language);
        maps
    }

    /// Sends `text` for document `doc` (or the compare diagram). When the
    /// diagram follows the Omarchy theme, the renderer sees those colors
    /// and the answer is mapped back onto `text`.
    fn dispatch(&mut self, doc: u64, text: &str, language: Language) {
        let Some(worker) = self.worker.clone() else {
            return;
        };
        // A project's file: its map, which is made from the project.
        if doc != compare::KEY && self.docs.iter().any(|d| d.key == doc && d.is_project()) {
            return self.dispatch_map(doc, &worker);
        }
        let place = self.place_of(doc);
        let (source, pending) =
            self.diagram_source_in(place.as_ref().map(|p| p.root.as_path()), text, language);
        let linked = if doc == compare::KEY {
            None
        } else {
            self.linked_of(doc)
        };
        let id = {
            let paper = if doc == compare::KEY {
                let Some(compare) = &mut self.compare else {
                    return;
                };
                &mut compare.paper
            } else {
                let Some(found) = self.find(doc) else {
                    return;
                };
                &mut found.paper
            };
            paper.request_linked(&worker, doc, source, language, place, linked);
            paper.requested()
        };
        if let Some(pending) = pending {
            self.pending.insert((doc, id), pending);
        }
    }

    /// Asks for the map beside the project's file in document `doc`: of
    /// the folder that document shows, from the project's diagrams as
    /// their tabs hold them, in the pane's colors (docs/adr/0026). Before
    /// the project is read nothing is asked; it is drawn when it is.
    fn dispatch_map(&mut self, doc: u64, worker: &Worker) {
        let Some((atlas, _)) = self.linked_of(doc) else {
            return;
        };
        let root = self.place_of(doc).map(|p| p.root);
        let (skin, _) =
            self.diagram_source_in(root.as_deref(), crate::look::MAP_SKIN, Language::PlantUml);
        if let Some(found) = self.find(doc) {
            let folder = found.folder.clone();
            found.paper.request_map(worker, doc, skin, atlas, folder);
        }
    }

    /// Where the file of document `doc` lives, inside the sidebar's folder
    /// when it is there; the compare diagram lives with its tab. An unsaved
    /// diagram lives nowhere, so its `!include`s decline.
    fn place_of(&self, doc: u64) -> Option<omaspec_render::Place> {
        let doc = match &self.compare {
            Some(compare) if doc == compare::KEY => compare.a,
            _ => doc,
        };
        let path = self.docs.iter().find(|d| d.key == doc)?.path.as_deref()?;
        omaspec_render::Place::of(path, self.folder.as_deref())
    }

    /// The text the renderer should see, and how to read its lines back
    /// when the Omarchy colors were laid over the file.
    fn diagram_source(&self, text: &str, language: Language) -> (Arc<str>, Option<Pending>) {
        self.diagram_source_in(None, text, language)
    }

    /// [`Self::diagram_source`] for a diagram of the project at `root`,
    /// in the look it pins ([`Self::pane_skin`]).
    fn diagram_source_in(
        &self,
        root: Option<&Path>,
        text: &str,
        language: Language,
    ) -> (Arc<str>, Option<Pending>) {
        let Some(skin) = self.pane_skin(root) else {
            return (Arc::from(text), None);
        };
        match crate::look::apply(text, language, &skin) {
            (shown, Some(shift)) => (
                Arc::from(shown),
                Some(Pending {
                    original: Arc::from(text),
                    shift,
                }),
            ),
            (_, None) => (Arc::from(text), None),
        }
    }

    /// What the pane draws a diagram of the project at `root` in: the
    /// look the project pins (`auto` as light or dark as the theme is),
    /// else the Omarchy colors when the diagram follows them, else nothing,
    /// the file's own colors.
    fn pane_skin(&self, root: Option<&Path>) -> Option<crate::look::Skin> {
        if let Some(pinned) = self.pinned_look(root) {
            let dark = self
                .palette
                .background
                .contrast(omaspec_theme::Rgb::new(255, 255, 255))
                > self
                    .palette
                    .background
                    .contrast(omaspec_theme::Rgb::new(0, 0, 0));
            return Some(pinned.on(dark));
        }
        self.follow_theme
            .then(|| crate::look::Skin::colors(self.hex_colors()))
    }

    /// The look the project at `root` pins, if its file reads; read again
    /// only when the file changed.
    fn pinned_look(&self, root: Option<&Path>) -> Option<crate::look::Skin> {
        self.looks.of(root)
    }

    /// Puts a render answer back on the file it was drawn from.
    fn untangle(&mut self, answer: &mut worker::Answer) {
        let key = (answer.ticket.doc, answer.ticket.id);
        let Some(pending) = self.pending.remove(&key) else {
            return;
        };
        self.pending
            .retain(|&(doc, id), _| doc != key.0 || id > key.1);
        match &mut answer.result {
            Ok(drawn) => drawn.map = Arc::new(remap_map(&drawn.map, pending.shift)),
            Err(err) => pending.shift.remap(err),
        }
        answer.source = pending.original;
    }

    /// Draws the shown diagram again, and the one beside it. The other
    /// tabs draw again when they are shown.
    fn rerender_all(&mut self) {
        self.forget_themed_atlases();
        let active = self.doc().key;
        for doc in &mut self.docs {
            if doc.key != active {
                doc.redraw_later();
            }
        }
        let _ = self.render(active, true);
        let compare = self
            .compare
            .as_ref()
            .map(|compare| (compare.text.clone(), compare.language));
        if let Some((text, language)) = compare {
            self.dispatch(compare::KEY, &text, language);
        }
    }

    fn run(&mut self, command: Command) -> Task<Message> {
        if self.prompt.is_some() && blocks_while_asking(command) {
            return Task::none();
        }
        if !matches!(command, Command::Quit) {
            self.leaving = false;
        }
        if !matches!(command, Command::CloseTab) {
            self.closing = None;
        }
        if self.doc().is_document()
            && let Err(why) = command.in_a_document()
        {
            self.notice = Some(why.into());
            return Task::none();
        }
        let key = self.doc().key;
        let paper = |task: Task<paper::Message>| task.map(move |m| Message::Paper(key, m));
        match command {
            Command::Palette => self.open_palette(),
            Command::Save => {
                let doc = self.doc();
                match doc.path.clone() {
                    // A name not written yet may have been taken since.
                    Some(path) if !doc.on_disk() && path.exists() => {
                        let task = self.ask(Asking::SaveAs);
                        self.notice = Some(format!(
                            "{} exists now: Enter overwrites it",
                            path.display()
                        ));
                        if let Some(prompt) = &mut self.prompt {
                            prompt.overwrite = true;
                        }
                        task
                    }
                    Some(path) => {
                        self.save(&path);
                        Task::none()
                    }
                    None => self.ask(Asking::SaveAs),
                }
            }
            Command::SaveAs => self.ask(Asking::SaveAs),
            Command::Revert => self.revert(),
            Command::Open => self.ask(Asking::Open),
            Command::NewProject => self.ask_project_kind(),
            Command::Pick => {
                self.picked = true;
                self.open_picker()
            }
            Command::Compare => self.open_versions(),
            Command::StopCompare => {
                self.stop_compare();
                Task::none()
            }
            Command::CompareDiff => {
                if let Some(c) = &mut self.compare {
                    c.show_diff = !c.show_diff;
                    c.show_behavior = false;
                }
                Task::none()
            }
            Command::CompareBehavior => {
                if let Some(c) = &mut self.compare {
                    c.show_behavior = !c.show_behavior;
                    c.show_diff = false;
                }
                Task::none()
            }
            Command::Convert => self.convert(),
            Command::CompareSync => {
                if let Some(c) = &mut self.compare {
                    c.sync = !c.sync;
                    self.notice = Some(if c.sync {
                        "A and B zoom together".into()
                    } else {
                        "A and B zoom apart".into()
                    });
                }
                Task::none()
            }
            Command::FindAll => self.open_search(false),
            Command::ReplaceAll => self.open_search(true),
            Command::RenameEverywhere => self.start_rename(),
            Command::FindInDiagram => self.find_in_diagram(),
            Command::SwitchField => self.switch_field(),
            Command::NextLook => {
                self.next_export_look(None);
                Task::none()
            }
            Command::Format => {
                if self.doc_mut().format() {
                    self.edited(key);
                } else {
                    self.notice = Some("already formatted".into());
                }
                Task::none()
            }
            Command::Collapse => self.fold_at_caret(false),
            Command::Expand => self.fold_at_caret(true),
            Command::CollapseAll => {
                self.doc_mut().editor.collapse_all();
                Task::none()
            }
            Command::ExpandAll => {
                self.doc_mut().editor.expand_all();
                Task::none()
            }
            Command::LanguageDetected => self.set_language(None),
            Command::LanguagePlantUml => self.set_language(Some(Language::PlantUml)),
            Command::LanguageMermaid => self.set_language(Some(Language::Mermaid)),
            Command::CopyPng | Command::CopySvg => {
                let format = if command == Command::CopyPng {
                    Format::Png
                } else {
                    Format::Svg
                };
                let doc = self.doc();
                let name = doc
                    .shown_diagram()
                    .map_or_else(|| doc.name(), |(_, _, name)| name);
                doc.paper.copy(format, &name).map(Message::Copied)
            }
            Command::ExportSvg => self.export(Format::Svg),
            Command::ExportPng => self.export(Format::Png),
            Command::ExportJpeg => self.export(Format::Jpeg),
            Command::ExportPdf => self.export(Format::Pdf),
            Command::ExportMarkdown => self.export(Format::Markdown),
            Command::ExportZip => self.export(Format::Zip),
            Command::ExportProject => self.export_project(),
            Command::CloseOthers => self.close_others(),
            Command::RenderNow => self.render(key, true),
            Command::NewTab => {
                let key = self.take_key();
                self.docs.push(Doc::untitled(key, self.forced, &self.look));
                self.select(self.docs.len() - 1)
            }
            Command::CloseTab => self.close(self.active),
            Command::NextTab => self.select((self.active + 1) % self.docs.len()),
            Command::PreviousTab => {
                self.select((self.active + self.docs.len() - 1) % self.docs.len())
            }
            Command::ToggleSidebar => {
                self.toggle_sidebar();
                self.keep_session();
                self.apply_keyboard_focus()
            }
            Command::Focus(direction) => self.move_focus(direction),
            Command::Skin(pack) => self.skin(pack),
            Command::ResetLine => self.reset_line(),
            Command::ResetLines => self.reset_lines(),
            Command::FocusBox => self.focus_box(),
            Command::FoldBox => self.fold_box(),
            Command::UnfoldAll => self.unfold_all(),
            Command::StartFrom(language) => {
                if !self.doc().text().trim().is_empty() {
                    return Task::none();
                }
                let key = self.doc().key;
                self.doc_mut().start_from(language);
                self.edited(key);
                self.render(key, false)
            }
            Command::FollowTheme => {
                self.follow_theme = !self.follow_theme;
                let root = self.place_of(key).map(|p| p.root);
                self.notice = Some(if self.pinned_look(root.as_deref()).is_some() {
                    "this project pins its look in omaspec-project.toml; other diagrams follow the change".into()
                } else if self.follow_theme {
                    "diagram colors follow the Omarchy theme".into()
                } else {
                    "diagram colors are the ones in the file".into()
                });
                self.keep_session();
                self.rerender_all();
                self.draw_thumbs()
            }
            Command::Gallery => self.toggle_gallery(),
            Command::OpenExample(play) => self.open_chosen(play),
            Command::NextGroup | Command::PreviousGroup => {
                if let Some(gallery) = &mut self.gallery {
                    gallery.step_group(command == Command::PreviousGroup);
                }
                self.reveal_chosen()
            }
            Command::OpenHelp => self.open_help(),
            Command::LeaveHelp => self.leave_help(),
            Command::Back => self.back(),
            Command::Detach => match self.detached {
                Some(_) => self.attach(),
                None => self.detach(),
            },
            Command::Quit => self.quit(),
            Command::Cancel if self.menu.is_some() => {
                self.close_menu();
                Task::none()
            }
            Command::Choose => self.menu_choose(),
            Command::ToggleMenuBar => {
                self.toggle_menu_bar();
                Task::none()
            }
            Command::ToggleToolbar => {
                self.toggle_toolbar();
                Task::none()
            }
            Command::OpenMenu(menu) => {
                self.open_bar_menu(menu);
                Task::none()
            }
            Command::Cancel => {
                self.prompt = None;
                self.renaming = None;
                self.gallery = None;
                self.close_picker();
                self.focus_editor()
            }
            Command::Up | Command::Down if self.menu.is_some() => {
                self.menu_move(if command == Command::Up { -1 } else { 1 });
                Task::none()
            }
            Command::Left | Command::Right if self.menu.is_some() => {
                self.menu_side(command == Command::Right);
                Task::none()
            }
            Command::Up | Command::Down if self.gallery.is_some() => {
                self.gallery_row(command == Command::Down)
            }
            Command::Up | Command::Down => {
                if let Some(picker) = &mut self.picker {
                    picker.move_by(if command == Command::Up { -1 } else { 1 });
                }
                Task::none()
            }
            Command::Left => self.gallery_move(-1),
            Command::Right => self.gallery_move(1),
            Command::ZoomIn => paper(self.doc_mut().paper.zoom(KEY_ZOOM)),
            Command::ZoomOut => paper(self.doc_mut().paper.zoom(1.0 / KEY_ZOOM)),
            Command::ActualSize => paper(self.doc_mut().paper.actual_size()),
            Command::Fit => paper(self.doc_mut().paper.fit()),
            Command::Play => self.toggle_play(),
            Command::PlayTrace => self.open_traces(),
            Command::PlayAsk(ask) => {
                let task = paper(self.doc_mut().paper.update(paper::Message::Play(ask)));
                self.light_played_line(key);
                task
            }
            Command::StopPlay => paper(self.doc_mut().paper.update(paper::Message::StopPlay)),
        }
    }

    /// After a change made by a command: marks, lint and render later.
    fn edited(&mut self, key: u64) {
        if let Some(doc) = self.find(key) {
            let _ = doc.editor.take_dirty();
            doc.edited();
        }
        self.after(Alarm::Lint(key), LINT_PAUSE);
        self.after(Alarm::Render(key), RENDER_PAUSE);
        self.keep_session();
    }

    fn set_language(&mut self, forced: Option<Language>) -> Task<Message> {
        let doc = self.doc_mut();
        let changed = doc.set_forced(forced);
        let (key, name) = (doc.key, doc.language.name());
        self.notice = Some(match forced {
            Some(_) => format!("language set to {name}"),
            None => format!("language detected: {name}"),
        });
        self.keep_session();
        if changed {
            self.render(key, false)
        } else {
            Task::none()
        }
    }

    fn select(&mut self, index: usize) -> Task<Message> {
        if index >= self.docs.len() {
            return Task::none();
        }
        if index != self.active {
            // A diagram still on the trail keeps its picture: the way back
            // out of a link zooms over it. Dropping it here is the black
            // frame, then the picture popping in.
            let keep = self.docs[self.active]
                .path
                .as_ref()
                .is_some_and(|path| self.trail.iter().any(|step| &step.path == path));
            if !keep {
                self.docs[self.active].paper.release();
            }
            self.left_tab = Some(self.docs[self.active].key);
            self.tab_fade = Animation::new(false)
                .duration(TAB_FADE)
                .easing(Easing::EaseOut)
                .go(true, Instant::now());
            self.active = index;
            self.prompt = None;
            self.keep_session();
        }
        let key = self.doc().key;
        let (size, factor) = (self.paper_size, self.paper_scale());
        let paper = &mut self.doc_mut().paper;
        let shown = Task::batch([
            paper.set_scale_factor(factor),
            paper.update(paper::Message::Viewport(size)),
        ])
        .map(move |m| Message::Paper(key, m));
        Task::batch([
            shown,
            self.render(key, false),
            self.focus_editor(),
            self.reveal_tab(),
        ])
    }

    /// Drops the pictures of diagrams that fell off the trail. The one on
    /// screen stays, and so does each step still on it.
    fn release_off_trail(&mut self) {
        let active = self.active;
        let drop: Vec<usize> = self
            .docs
            .iter()
            .enumerate()
            .filter(|(index, doc)| {
                *index != active
                    && !doc
                        .path
                        .as_ref()
                        .is_some_and(|path| self.trail.iter().any(|step| &step.path == path))
            })
            .map(|(index, _)| index)
            .collect();
        for index in drop {
            self.docs[index].paper.release();
        }
    }

    /// Brings the active tab into the bar, scrolling only as far as it
    /// takes. A tab that is already whole in the bar leaves it where it is.
    fn reveal_tab(&self) -> Task<Message> {
        let Some(doc) = self.docs.get(self.active) else {
            return Task::none();
        };
        iced::advanced::widget::operate(Reveal::new(TABS.into(), tab_id(doc.key), false))
    }

    /// Ctrl+click in the diagram: to the word that draws what is there.
    fn pick_in_diagram(&mut self, key: u64, at: iced::Point) -> Task<Message> {
        if key != self.doc().key {
            return Task::none();
        }
        // A map is made and has no source: its tile opens as by a click.
        if self.doc().is_project() {
            return self.follow_link(key, at);
        }
        match self.land_on(at) {
            Ok(task) => {
                self.notice = None;
                task
            }
            Err(why) => {
                self.notice = Some(why);
                Task::none()
            }
        }
    }

    /// The caret lands on what is at `at` of the active tab's diagram, or
    /// `Err` says why it cannot.
    pub(super) fn land_on(&mut self, at: iced::Point) -> Result<Task<Message>, String> {
        // Right on a line: the line that draws it.
        if let Some(line) = self.doc().paper.wire_at(at).and_then(|w| w.line) {
            return Ok(self.jump_to(line));
        }
        let Some((label, source)) = self.doc().paper.label_at(at) else {
            return Err("no label there to find in the source".into());
        };
        match omaspec_core::locate::column_of(&source, &label.text, label.nth) {
            Some((line, column)) => Ok(self.jump_to_at(line, column)),
            None => Err(format!("“{}” is not in the source", label.text)),
        }
    }

    fn jump_to(&mut self, line: usize) -> Task<Message> {
        self.jump_to_at(line, 0)
    }

    /// `column` is a byte on `line`. Zero stays at the indentation, which is
    /// where a tile or a wire lands; a label lands on its own first letter.
    fn jump_to_at(&mut self, line: usize, column: usize) -> Task<Message> {
        let key = {
            let doc = self.doc_mut();
            doc.jump_to_line_at(line, column);
            doc.key
        };
        self.after(Alarm::Flash(key), FLASH);
        self.follow_caret(key);
        let front = match self.detached {
            Some(_) => window::gain_focus(self.main),
            None => Task::none(),
        };
        Task::batch([self.focus_editor(), front])
    }

    /// Folds or unfolds the block at the caret. A caret outside every block
    /// leaves the text as it is and says so.
    /// The active tab written in the other language, in a new tab; what
    /// was left out, or why nothing was written, in the notice.
    fn convert(&mut self) -> Task<Message> {
        let (from, text) = (self.doc().current_language(), self.doc().text());
        let to = crate::commands::other(from);
        let converted = match omaspec_model::convert(&text, from, to) {
            Ok(converted) => converted,
            Err(refused) => {
                let at = refused
                    .line
                    .map(|l| format!("line {}: ", l + 1))
                    .unwrap_or_default();
                self.notice = Some(format!(
                    "not written in {}: {at}{}",
                    to.name(),
                    refused.reason
                ));
                return Task::none();
            }
        };
        let key = self.take_key();
        self.docs
            .push(Doc::unsaved(key, &converted.code, &self.look));
        let task = self.select(self.docs.len() - 1);
        self.notice = Some(match converted.notices.as_slice() {
            [] => format!("written in {} in a new tab", to.name()),
            [only] => format!("written in {} in a new tab; {}", to.name(), only.message),
            [first, rest @ ..] => format!(
                "written in {} in a new tab; {} (and {} more)",
                to.name(),
                first.message,
                rest.len()
            ),
        });
        task
    }

    fn fold_at_caret(&mut self, unfold: bool) -> Task<Message> {
        let before = self.doc().editor.document().fold_map().display_row_count();
        self.doc_mut().editor.collapse_at_caret(unfold);
        let after = self.doc().editor.document().fold_map().display_row_count();
        if before == after {
            self.notice = Some(if unfold {
                "nothing to expand here".into()
            } else {
                "nothing to collapse here".into()
            });
        }
        Task::none()
    }

    /// Puts `pack` into the source (one undo step), replacing a pack
    /// there, or takes it out.
    fn skin(&mut self, pack: Option<Pack>) -> Task<Message> {
        let hex = self.hex_colors();
        let colors = hex.colors();
        let doc = self.doc();
        let key = doc.key;
        let Some(edit) = skin::edit(&doc.text(), doc.language, pack, &colors) else {
            self.notice = Some(match pack {
                Some(p) => format!("the {} skin is in already", p.name()),
                None => "no skin to remove".into(),
            });
            return Task::none();
        };
        self.doc_mut().replace(vec![edit]);
        self.edited(key);
        self.notice = Some(match pack {
            Some(p) => format!("skin: {} (Ctrl+Z undoes it)", p.name()),
            None => "skin removed".into(),
        });
        self.render(key, false)
    }

    /// Writes the bends of a line drawn by hand in the diagram into the
    /// source, as one undo step (docs/adr/0016).
    fn draw_by_hand(&mut self, key: u64, line: &omaspec_uml::hand::Line) -> Task<Message> {
        let Some(doc) = self.find(key) else {
            return Task::none();
        };
        let language = doc.current_language();
        let Some(edit) = omaspec_core::hand::edit(&doc.text(), language, line) else {
            doc.paper.by_hand_unchanged();
            return Task::none();
        };
        doc.replace(vec![edit]);
        self.edited(key);
        self.notice = Some(if line.corners.is_empty() {
            format!("{} runs its own way again (Ctrl+Z undoes it)", line.key)
        } else {
            format!("{} drawn by hand (Ctrl+Z undoes it)", line.key)
        });
        self.render(key, false)
    }

    /// Holds the box `name` where `hold` says, or gives it back to the
    /// layout, in the source, as one undo step (docs/adr/0021).
    fn hold_box(
        &mut self,
        key: u64,
        name: &str,
        hold: Option<&omaspec_uml::hand::Hold>,
    ) -> Task<Message> {
        let Some(doc) = self.find(key) else {
            return Task::none();
        };
        let language = doc.current_language();
        let Some(edit) = omaspec_core::hand::hold(&doc.text(), language, name, hold) else {
            doc.paper.held_unchanged();
            return Task::none();
        };
        doc.replace(vec![edit]);
        self.edited(key);
        self.notice = Some(match hold {
            Some(hold) => format!("{hold}: held in place (Ctrl+Z undoes it)"),
            None => format!("{name} stands where the layout puts it again (Ctrl+Z undoes it)"),
        });
        self.render(key, false)
    }

    /// The line at the caret back to the way the layout gives it, or the
    /// box whose entry is at the caret.
    fn reset_line(&mut self) -> Task<Message> {
        let row = self.doc().editor.cursor().row as usize;
        if let Some(name) = self.doc().paper.place_on(row).map(|p| p.name.clone()) {
            let doc = self.doc().key;
            return self.hold_box(doc, &name, None);
        }
        let found = self
            .doc()
            .paper
            .wire_on(row)
            .map(|w| (w.key.clone(), w.entry));
        match found {
            Some((key, Some(_))) => {
                let doc = self.doc().key;
                let line = omaspec_uml::hand::Line {
                    key,
                    corners: Vec::new(),
                };
                self.draw_by_hand(doc, &line)
            }
            Some((key, None)) => {
                self.notice = Some(format!("{key} runs its own way already"));
                Task::none()
            }
            None => {
                self.notice = Some("no line of the diagram at the caret".into());
                Task::none()
            }
        }
    }

    /// Every line and every box back to the way the layout gives them.
    fn reset_lines(&mut self) -> Task<Message> {
        let key = self.doc().key;
        let Some(edit) = omaspec_core::hand::clear(&self.doc().text()) else {
            self.notice = Some("no line is drawn by hand and no box held".into());
            return Task::none();
        };
        self.doc_mut().replace(vec![edit]);
        self.edited(key);
        self.notice = Some("every line and box goes back to the layout (Ctrl+Z undoes it)".into());
        self.render(key, false)
    }

    fn reload_theme(&mut self) {
        let omarchy = Omarchy::load_current();
        let palette = omarchy.palette.legible();
        self.theme_name = omarchy.name.unwrap_or_else(|| "built-in".into());
        self.theme = ui::iced_theme(&self.theme_name, &palette);
        self.look = Look::new(&palette, &omarchy.tweaks, self.font);
        self.icons = icons_for(&palette, self.main_scale);
        self.palette = palette;
        for doc in &mut self.docs {
            doc.set_look(&self.look);
        }
        if self.follow_theme {
            self.rerender_all();
        }
    }

    /// Writes in `font` from now on (`omarchy font set` changed
    /// `monospace`).
    fn set_font(&mut self, font: Font) {
        if font == self.font {
            return;
        }
        self.font = font;
        self.look.font = font;
        for doc in &mut self.docs {
            doc.set_font(font);
        }
    }

    /// Whether something on screen still moves at `now`, so the window
    /// draws frames. The diagram's fade asks for its own. The light on
    /// the caret's name is here, so a frame keeps coming while it glides.
    fn animating(&self, now: Instant) -> bool {
        self.tab_fade.is_animating(now)
            || self.focus_fade.is_animating(now)
            || self.gallery_moving(now)
            || self.docs.iter().any(Doc::flash_fading)
            || self.docs.iter().any(|doc| doc.paper.breath_moving(now))
            || self.docs.iter().any(|doc| doc.paper.line_waiting())
    }

    /// Lights, on the sheet, the label the caret is in.
    fn follow_caret(&mut self, key: u64) {
        let Some(index) = self.docs.iter().position(|doc| doc.key == key) else {
            return;
        };
        let (text, row, col) = {
            let doc = &self.docs[index];
            let cursor = doc.editor.cursor();
            (doc.shown_source(), cursor.row, cursor.col)
        };
        self.docs[index].paper.follow_source(&text, row, col);
        self.focus_follows(index, &text, row as usize, col as usize);
    }

    /// Omarchy colors as `#rrggbb`, for a skin block.
    fn hex_colors(&self) -> Hex {
        Hex::of(&self.palette)
    }
}

/// A render whose source had the Omarchy colors laid over the file.
struct Pending {
    original: Arc<str>,
    shift: LineShift,
}

/// Moves the lines a drawn diagram's states, transitions, messages and
/// lines come from back to the file, where a play and a Ctrl+click look
/// for them.
fn remap_map(map: &Map, shift: LineShift) -> Map {
    let mut map = map.clone();
    let lines = map
        .states
        .iter_mut()
        .map(|spot| &mut spot.line)
        .chain(map.transitions.iter_mut().map(|route| &mut route.line))
        .chain(map.messages.iter_mut().map(|step| &mut step.line))
        .chain(map.misses.iter_mut().map(|(line, _)| line))
        .chain(
            map.wires
                .iter_mut()
                .flat_map(|wire| [wire.line.as_mut(), wire.entry.as_mut()])
                .flatten(),
        );
    for line in lines {
        if let Some(file) = shift.file_line(*line) {
            *line = file;
        }
    }
    map
}

/// Drops the keyboard focus, so typing does not change the source while
/// another area is the one in front.
fn blur() -> Task<Message> {
    iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::unfocus())
}

/// The project tree left of the source, the diagram on the right. Without
/// the diagram (in its own window) the other areas take the whole width.
/// Without the sidebar, source and diagram share it.
/// The icons in the colors the window draws them in, at its `scale`.
fn icons_for(p: &Palette, scale: f32) -> Icons {
    Icons::new(&[p.accent, p.magenta, ui::quiet(p), p.foreground], scale)
}

fn layout(sidebar: bool, diagram: bool) -> pane_grid::State<Pane> {
    use pane_grid::Configuration;
    let source = Configuration::Pane(Pane::Source);
    let left = if sidebar {
        Configuration::Split {
            axis: pane_grid::Axis::Vertical,
            ratio: if diagram {
                SIDEBAR_SHARE
            } else {
                SIDEBAR_SHARE / 2.0
            },
            a: Box::new(Configuration::Pane(Pane::Files)),
            b: Box::new(source),
        }
    } else {
        source
    };
    if !diagram {
        return pane_grid::State::with_configuration(left);
    }
    pane_grid::State::with_configuration(Configuration::Split {
        axis: pane_grid::Axis::Vertical,
        ratio: 0.5,
        a: Box::new(left),
        b: Box::new(Configuration::Pane(Pane::Diagram)),
    })
}

/// The working directory as the folder, unless it is the home folder or
/// the root (a start from a menu), which hold far more than diagrams.
fn working_folder() -> Option<PathBuf> {
    let dir = std::env::current_dir().ok()?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    (dir.parent().is_some() && Some(&dir) != home.as_ref()).then_some(dir)
}

fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png" | "svg" | "jpg" | "jpeg"
        )
    })
}

/// `dir/stem.extension`, or with `-2`, `-3` … when a file or a tab has
/// that name.
fn free_name(dir: &Path, stem: &str, extension: &str, taken: &[&Path]) -> PathBuf {
    (1..)
        .map(|n| {
            let name = match n {
                1 => format!("{stem}.{extension}"),
                n => format!("{stem}-{n}.{extension}"),
            };
            dir.join(name)
        })
        .find(|path| !path.exists() && !taken.contains(&path.as_path()))
        .expect("some number is free")
}

/// `path` with the home folder as `~`.
fn home_relative(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.and_then(|h| path.strip_prefix(h).ok().map(Path::to_owned)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.to_owned()
    } else {
        format!("{word}s")
    }
}

/// Where an export starts: beside a saved diagram, or `untitled` in the
/// folder. Never the source file itself.
fn suggested_export(doc: &Doc, folder: &Path, kind: Format) -> String {
    let path = match &doc.path {
        Some(path) => {
            let out = if doc.is_document() {
                // A document's diagram by its number: README-2.svg.
                let name = doc
                    .shown_diagram()
                    .map_or_else(|| doc.name(), |(_, _, name)| name);
                let stem = name
                    .rsplit_once('.')
                    .map_or(name.as_str(), |(stem, _)| stem);
                path.with_file_name(format!("{stem}.{}", kind.extension()))
            } else {
                path.with_extension(kind.extension())
            };
            if doc.path.as_deref() == Some(out.as_path()) {
                let stem = path
                    .file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "diagram".into());
                path.with_file_name(format!("{stem}-export.{}", kind.extension()))
            } else {
                out
            }
        }
        None => folder.join(format!("untitled.{}", kind.extension())),
    };
    path.display().to_string()
}

/// A path typed into the prompt: `~` is the home folder, a relative path
/// starts in the working directory.
fn expand(input: &str) -> Option<PathBuf> {
    if input.is_empty() {
        return None;
    }
    let path = match input.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME")?).join(rest),
        None => PathBuf::from(input),
    };
    std::path::absolute(path).ok()
}

/// `typed` with the extension of `format`: kept when it names that format,
/// swapped when it names another export format, added otherwise, so a dot
/// in the name stays (`flow-1.2` → `flow-1.2.svg`).
fn export_path(typed: PathBuf, format: Format) -> PathBuf {
    let named = typed
        .extension()
        .and_then(|ext| Format::from_extension(&ext.to_string_lossy()));
    match named {
        Some(named) if named == format => typed,
        Some(_) => typed.with_extension(format.extension()),
        None => {
            let mut name = typed.into_os_string();
            name.push(".");
            name.push(format.extension());
            PathBuf::from(name)
        }
    }
}

#[cfg(test)]
mod legible_tests;
#[cfg(test)]
mod tests;
