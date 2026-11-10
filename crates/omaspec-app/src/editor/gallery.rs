//! The gallery of examples (Ctrl+Shift+E): every diagram that comes with
//! Omaspec as a tile, drawn on the spot by its engines in the colors the
//! diagram pane would show it in. The tiles rise in a wave as it opens,
//! each picture fades in as it comes, and the chosen tile glows in the
//! accent. Typing filters, Tab steps through the groups, the arrows move
//! between tiles; Enter opens a copy in a new tab, F5 or Shift+Enter opens
//! it and plays it, a click does the same.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::time::Instant;
use iced::widget::{
    Image, Space, button, column, container, image, mouse_area, opaque, row, scrollable, stack,
    text, text_input,
};
use iced::{Alignment, Color, Element, Length, Padding, Size, Task, Theme};
use omaspec_core::{DiagramKind, Language};
use omaspec_render::export::{
    self,
    tiny_skia::{self, Pixmap, PixmapPaint, Transform},
};
use omaspec_render::{Config, Renderer};

use super::doc::Doc;
use super::{App, Command, Message, Reveal, free_name, home_relative, plural};
use crate::gallery::{EXAMPLES, Group};
use crate::icons::{self, Icon};
use crate::ui;
use crate::worker::{Alarm, blocking};

/// The search field.
pub const FIELD: &str = "gallery";
/// The scrolling grid of tiles.
const GRID: &str = "gallery-grid";

/// How much wider than high a tile's picture is.
const THUMB_ASPECT: f32 = 1.6;
/// The narrowest and the widest a tile's picture is, in logical pixels:
/// the tiles fill the panel's width, fewer and larger in a narrow window.
const THUMB_NARROWEST: f32 = 272.0;
const THUMB_WIDEST: f32 = 480.0;
/// Room inside a tile around its picture and its words.
const TILE_PAD: f32 = 8.0;
/// Between tiles.
const GAP: f32 = 14.0;
/// Inside the panel, around everything.
const PANEL_PAD: f32 = 22.0;
/// Between the panel and the window's edge.
const MARGIN: f32 = 28.0;
/// Beside the grid, for its scroller.
const SCROLLER: f32 = 12.0;
const MAX_COLUMNS: usize = 5;
/// The filters and the switch of colors side by side take this much room.
const FILTERS_WIDTH: f32 = 780.0;
/// Between a diagram and the edge of its tile, where it is not cut.
const THUMB_INSET: f32 = 10.0;
/// A tall diagram is drawn up to this much larger than whole in its tile,
/// to fill more of it; its bottom is cut, not its top, where titles and
/// starts are. A wide one keeps its sides, where rows have their names.
const THUMB_FILL: f32 = 1.4;
/// A small diagram is not blown up beyond this.
const THUMB_ZOOM: f32 = 1.25;
const RADIUS: f32 = 10.0;

/// The veil comes and the panel settles in this long.
const VEIL: Duration = Duration::from_millis(200);
/// Each diagonal of tiles rises this much later than the one before.
const WAVE: Duration = Duration::from_millis(40);
/// How long a tile takes to rise, and how far it comes from.
const RISE: Duration = Duration::from_millis(320);
const RISE_BY: f32 = 12.0;
/// A picture fades in over this long.
const THUMB_FADE: Duration = Duration::from_millis(220);
/// The glow moves to the chosen tile in this long.
const LIGHT: Duration = Duration::from_millis(160);
/// The pictures are drawn again for a window this long after it last
/// changed its size.
const THUMBS_AFTER_RESIZE: Duration = Duration::from_millis(150);

/// A tile's picture, ready to show.
#[derive(Debug, Clone)]
pub struct Thumb {
    pub handle: image::Handle,
    /// The diagram's own background, which fills the tile around it.
    pub paper: Color,
    /// Whether it is a state machine or a scenario that F5 plays.
    pub playable: bool,
    /// How long the engine took.
    pub elapsed: Duration,
}

/// `source` drawn by Omaspec's engines (no Java) in `width` × `height`
/// pixels on its own paper: cut to what is drawn, as wide as fits and up
/// to [`THUMB_FILL`] taller than fits, at most [`THUMB_ZOOM`] times its
/// size at `scale`.
pub fn thumbnail(
    source: &str,
    language: Language,
    width: u32,
    height: u32,
    scale: f32,
) -> Result<Thumb, String> {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let rendered = renderer
        .render(source, language)
        .map_err(|e| e.to_string())?;
    let tree = export::parse(&rendered.svg).map_err(|e| e.to_string())?;
    let playable = crate::paper::Play::new(source, language, &rendered.map, Instant::now()).is_ok();
    let size = tree.size();
    let drawn_area = drawn_area(&tree).unwrap_or_else(|| {
        export::usvg::Rect::from_xywh(0.0, 0.0, size.width(), size.height()).expect("a size")
    });
    let inset = (THUMB_INSET * scale).round();
    let room = (width as f32 - inset * 2.0, height as f32 - inset * 2.0);
    let (across, down) = (
        room.0 / drawn_area.width().max(1.0),
        room.1 / drawn_area.height().max(1.0),
    );
    let fit = across.min(down * THUMB_FILL).min(THUMB_ZOOM * scale);
    // On a clear ground: drawn on white, the edge of a dark background
    // would be a light line on the dark paper around it.
    let drawn = export::rasterize_clear(&tree, fit).map_err(|e| e.to_string())?;
    let paper = drawn
        .pixel(0, 0)
        .filter(|p| p.alpha() > 0)
        .map_or(tiny_skia::Color::WHITE, |p| {
            let p = p.demultiply();
            tiny_skia::Color::from_rgba8(p.red(), p.green(), p.blue(), 255)
        });
    let mut tile = Pixmap::new(width, height).ok_or("no room for the picture")?;
    tile.fill(paper);
    // Centered across; down, centered when it fits and from its top when
    // it does not. Whole pixels, so it stays as sharp as it was drawn.
    let (w, h) = (drawn_area.width() * fit, drawn_area.height() * fit);
    let left = inset + (room.0 - w) / 2.0;
    let top = if h > room.1 {
        inset
    } else {
        inset + (room.1 - h) / 2.0
    };
    tile.draw_pixmap(
        (left - drawn_area.x() * fit).round() as i32,
        (top - drawn_area.y() * fit).round() as i32,
        drawn.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Ok(Thumb {
        handle: image::Handle::from_rgba(width, height, tile.take()),
        paper: Color::from_rgb(paper.red(), paper.green(), paper.blue()),
        playable,
        elapsed: rendered.elapsed,
    })
}

/// Where something is drawn in `tree`, in its units: the paper a diagram
/// lies on, a shape filled over the whole picture, left out.
fn drawn_area(tree: &export::usvg::Tree) -> Option<export::usvg::Rect> {
    fn walk(
        group: &export::usvg::Group,
        size: export::usvg::Size,
        area: &mut Option<export::usvg::Rect>,
    ) {
        use export::usvg::Node;
        for node in group.children() {
            let bounds = match node {
                Node::Group(inner) => {
                    walk(inner, size, area);
                    continue;
                }
                Node::Path(path) if !path.is_visible() => continue,
                Node::Path(path) => {
                    let b = path.abs_bounding_box();
                    let paper = path.fill().is_some()
                        && b.left() <= 0.5
                        && b.top() <= 0.5
                        && b.right() >= size.width() - 0.5
                        && b.bottom() >= size.height() - 0.5;
                    if paper {
                        continue;
                    }
                    node.abs_stroke_bounding_box()
                }
                _ => node.abs_stroke_bounding_box(),
            };
            *area = Some(match *area {
                Some(a) => export::usvg::Rect::from_ltrb(
                    a.left().min(bounds.left()),
                    a.top().min(bounds.top()),
                    a.right().max(bounds.right()),
                    a.bottom().max(bounds.bottom()),
                )
                .unwrap_or(a),
                None => bounds,
            });
        }
    }
    let mut area = None;
    walk(tree.root(), tree.size(), &mut area);
    area
}

/// A tile's picture as it came, and the one before it while it fades in.
struct Shown {
    /// The colors and the scale it was drawn for.
    look: u64,
    thumb: Result<Thumb, String>,
    came: Instant,
    before: Option<Thumb>,
}

/// The tiles' pictures. They outlive the gallery, so it opens at once the
/// next time; another theme or scale draws them again.
#[derive(Default)]
pub struct Thumbs {
    look: u64,
    shown: HashMap<usize, Shown>,
    /// Examples asked for in `look` whose picture has not come yet.
    asked: HashSet<usize>,
}

impl Thumbs {
    fn moving(&self, now: Instant) -> bool {
        self.shown
            .values()
            .any(|s| now.saturating_duration_since(s.came) < THUMB_FADE)
    }

    /// The pictures drawn so far in the current look, and how long the
    /// engines took for them together.
    fn drawn(&self) -> (usize, Duration) {
        self.shown.values().filter(|s| s.look == self.look).fold(
            (0, Duration::ZERO),
            |(n, total), s| match &s.thumb {
                Ok(thumb) => (n + 1, total + thumb.elapsed),
                Err(_) => (n + 1, total),
            },
        )
    }
}

/// The gallery while it is open.
pub struct Gallery {
    pub query: String,
    /// Only this group, or all.
    pub group: Option<Group>,
    /// Only the complex examples.
    pub complex: bool,
    /// Indices into [`EXAMPLES`] that match, in their order.
    shown: Vec<usize>,
    /// The chosen tile's place in `shown`.
    selected: usize,
    opened: Instant,
    /// When the glow last moved, and the example it left.
    chosen: Instant,
    left: Option<usize>,
    hovered: Option<usize>,
    /// The kind of each example, for its icon and its badge.
    kinds: Vec<DiagramKind>,
}

impl Gallery {
    pub fn new(now: Instant) -> Self {
        let mut gallery = Self {
            query: String::new(),
            group: None,
            complex: false,
            shown: Vec::new(),
            selected: 0,
            opened: now,
            chosen: now,
            left: None,
            hovered: None,
            kinds: EXAMPLES
                .iter()
                .map(|e| omaspec_core::classify(e.text, e.language()))
                .collect(),
        };
        gallery.refilter();
        gallery
    }

    /// The example chosen, as an index into [`EXAMPLES`].
    pub fn chosen(&self) -> Option<usize> {
        self.shown.get(self.selected).copied()
    }

    #[cfg(test)]
    pub fn shown(&self) -> &[usize] {
        &self.shown
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.refilter();
    }

    pub fn set_group(&mut self, group: Option<Group>) {
        self.group = group;
        self.refilter();
    }

    /// Only the complex examples, or all again.
    pub fn toggle_complex(&mut self) {
        self.complex = !self.complex;
        self.refilter();
    }

    /// Whether `example` passes the group, the complexity and the query.
    fn passes(&self, example: &crate::gallery::Example, group: Option<Group>) -> bool {
        group.is_none_or(|g| g == example.group)
            && (!self.complex || example.complex)
            && example.matches(&self.query)
    }

    /// The next group, or the one before (`back`); all comes between the
    /// last and the first.
    pub fn step_group(&mut self, back: bool) {
        let mut all: Vec<Option<Group>> = vec![None];
        all.extend(Group::ALL.map(Some));
        let at = all.iter().position(|g| *g == self.group).unwrap_or(0);
        let n = all.len();
        self.set_group(all[if back { (at + n - 1) % n } else { (at + 1) % n }]);
    }

    /// Keeps the chosen example when it still matches.
    fn refilter(&mut self) {
        let before = self.chosen();
        self.shown = (0..EXAMPLES.len())
            .filter(|&i| self.passes(&EXAMPLES[i], self.group))
            .collect();
        let place = before
            .and_then(|i| self.shown.iter().position(|&j| j == i))
            .unwrap_or(0);
        self.choose(place, Instant::now());
    }

    /// Moves the choice `by` tiles in reading order (a row is as many as
    /// fit side by side); it stops at the ends of the grid, so a row down
    /// from above a gap lands on the last tile.
    pub fn move_by(&mut self, by: isize) {
        let last = self.shown.len().saturating_sub(1) as isize;
        let to = (self.selected as isize + by).clamp(0, last.max(0));
        self.choose(to as usize, Instant::now());
    }

    fn choose(&mut self, place: usize, now: Instant) {
        let before = self.chosen();
        self.selected = place.min(self.shown.len().saturating_sub(1));
        if self.chosen() != before {
            self.left = before;
            self.chosen = now;
        }
    }

    /// Whether something in it still moves at `now`.
    fn moving(&self, now: Instant) -> bool {
        let wave = RISE + WAVE * (self.shown.len() as u32 + 1);
        now.saturating_duration_since(self.opened) < wave.max(VEIL)
            || now.saturating_duration_since(self.chosen) < LIGHT
    }

    /// How far the tile on `diagonal` (row plus column) has risen, 0 to 1.
    fn rise(&self, diagonal: usize, now: Instant) -> f32 {
        let start = self.opened + VEIL / 2 + WAVE * diagonal as u32;
        progress(start, RISE, now)
    }

    /// How much of the glow example `index` has, 0 to 1.
    fn light(&self, index: usize, now: Instant) -> f32 {
        let moved = progress(self.chosen, LIGHT, now);
        if self.chosen() == Some(index) {
            moved
        } else if self.left == Some(index) {
            1.0 - moved
        } else {
            0.0
        }
    }
}

/// How far a move of `length` that began at `start` has come at `now`,
/// eased out, 0 to 1.
fn progress(start: Instant, length: Duration, now: Instant) -> f32 {
    let t =
        (now.saturating_duration_since(start).as_secs_f32() / length.as_secs_f32()).clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// How many tiles go side by side in a window `width` wide, and the size
/// of their pictures: they fill the panel, at least [`THUMB_NARROWEST`]
/// and at most [`THUMB_WIDEST`] wide.
pub fn layout(width: f32) -> (usize, Size) {
    let room = width - MARGIN * 2.0 - PANEL_PAD * 2.0 - SCROLLER + GAP;
    let narrowest = THUMB_NARROWEST + TILE_PAD * 2.0 + GAP;
    let columns = ((room / narrowest).floor() as usize).clamp(1, MAX_COLUMNS);
    let thumb = (room / columns as f32 - GAP - TILE_PAD * 2.0)
        .clamp(THUMB_NARROWEST / 2.0, THUMB_WIDEST)
        .floor();
    (columns, Size::new(thumb, (thumb / THUMB_ASPECT).round()))
}

/// The id of example `index`'s tile, so the grid can scroll to it.
fn tile_id(index: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("example-{index}"))
}

/// The gallery's keys: the arrows move between its tiles, Tab between
/// its groups, Enter opens the example the highlight is on (with Shift it
/// plays) and F5 plays it.
pub(super) fn key(key: &Key, mods: Modifiers) -> Option<Command> {
    let plain = !mods.control() && !mods.alt() && !mods.logo();
    let Key::Named(named) = key.as_ref() else {
        return None;
    };
    Some(match named {
        Named::F5 => Command::OpenExample(true),
        Named::Enter if !mods.control() => Command::OpenExample(mods.shift()),
        Named::Tab if !mods.control() && mods.shift() => Command::PreviousGroup,
        Named::Tab if !mods.control() => Command::NextGroup,
        Named::ArrowLeft if plain => Command::Left,
        Named::ArrowRight if plain => Command::Right,
        Named::ArrowUp if plain => Command::Up,
        Named::ArrowDown if plain => Command::Down,
        _ => return None,
    })
}

impl App {
    /// Opens the gallery over the editor window, or closes it.
    pub(super) fn toggle_gallery(&mut self) -> Task<Message> {
        if self.gallery.is_some() {
            self.gallery = None;
            return self.focus_editor();
        }
        self.prompt = None;
        self.close_picker();
        self.gallery = Some(Gallery::new(Instant::now()));
        let from_diagram = self
            .command_window
            .is_some_and(|id| self.detached == Some(id));
        let front = if from_diagram {
            iced::window::gain_focus(self.main)
        } else {
            Task::none()
        };
        Task::batch([
            front,
            self.draw_thumbs(),
            iced::widget::operation::focus(FIELD),
        ])
    }

    /// What the pictures depend on: the Omarchy colors while the diagrams
    /// follow them, and the window's scale.
    fn thumb_look(&self) -> u64 {
        let mut hasher = std::hash::DefaultHasher::new();
        if self.follow_theme {
            let hex = self.hex_colors();
            (
                hex.background,
                hex.surface,
                hex.text,
                hex.accent,
                hex.muted,
                hex.note,
            )
                .hash(&mut hasher);
        }
        self.main_scale.to_bits().hash(&mut hasher);
        self.thumb_pixels().hash(&mut hasher);
        hasher.finish()
    }

    /// A tile's picture in pixels, in the window as it is now.
    fn thumb_pixels(&self) -> (u32, u32) {
        let thumb = layout(self.main_size.width).1;
        let scale = self.main_scale;
        (
            (thumb.width * scale).round() as u32,
            (thumb.height * scale).round() as u32,
        )
    }

    /// The window changed its size: the pictures are drawn again for the
    /// tiles' new size, once it has stopped changing.
    pub(super) fn gallery_resized(&self) {
        if self.gallery.is_some() {
            self.after(Alarm::Thumbs, THUMBS_AFTER_RESIZE);
        }
    }

    /// Draws, in the background, the pictures the open gallery lacks in the
    /// current look; each comes on its own.
    pub(super) fn draw_thumbs(&mut self) -> Task<Message> {
        if self.gallery.is_none() {
            return Task::none();
        }
        let look = self.thumb_look();
        if look != self.thumbs.look {
            self.thumbs.look = look;
            self.thumbs.asked.clear();
        }
        let scale = self.main_scale;
        let (width, height) = self.thumb_pixels();
        let mut tasks = Vec::new();
        for (index, example) in EXAMPLES.iter().enumerate() {
            let fresh = self
                .thumbs
                .shown
                .get(&index)
                .is_some_and(|s| s.look == look);
            if fresh || !self.thumbs.asked.insert(index) {
                continue;
            }
            let language = example.language();
            let (source, _) = self.diagram_source(example.text, language);
            tasks.push(Task::perform(
                blocking(move || thumbnail(&source, language, width, height, scale)),
                move |thumb| {
                    Message::Thumb(
                        look,
                        index,
                        thumb.unwrap_or_else(|| Err("the engine failed".into())),
                    )
                },
            ));
        }
        Task::batch(tasks)
    }

    /// A picture came; one drawn for an earlier look is dropped.
    pub(super) fn thumb_came(&mut self, look: u64, index: usize, thumb: Result<Thumb, String>) {
        if look != self.thumbs.look {
            return;
        }
        self.thumbs.asked.remove(&index);
        let before = self
            .thumbs
            .shown
            .remove(&index)
            .and_then(|shown| shown.thumb.ok());
        self.thumbs.shown.insert(
            index,
            Shown {
                look,
                thumb,
                came: Instant::now(),
                before,
            },
        );
    }

    pub(super) fn gallery_moving(&self, now: Instant) -> bool {
        self.gallery.as_ref().is_some_and(|g| g.moving(now)) || self.thumbs.moving(now)
    }

    /// Moves the choice `by` tiles (a row is as many as fit side by side)
    /// and scrolls the grid to it.
    pub(super) fn gallery_move(&mut self, by: isize) -> Task<Message> {
        let Some(gallery) = &mut self.gallery else {
            return Task::none();
        };
        gallery.move_by(by);
        self.reveal_chosen()
    }

    /// The grid scrolled the least that shows the chosen tile.
    pub(super) fn reveal_chosen(&self) -> Task<Message> {
        match self.gallery.as_ref().and_then(Gallery::chosen) {
            Some(index) => {
                iced::advanced::widget::operate(Reveal::new(GRID.into(), tile_id(index), true))
            }
            None => Task::none(),
        }
    }

    /// Steps the choice a row up or down.
    pub(super) fn gallery_row(&mut self, down: bool) -> Task<Message> {
        let n = layout(self.main_size.width).0 as isize;
        self.gallery_move(if down { n } else { -n })
    }

    /// Opens a copy of example `index` in a new tab (an untouched empty
    /// one gives way) under a free name in the project folder, unsaved; and
    /// with `play`, plays it once it is drawn.
    pub(super) fn open_example(&mut self, index: usize, play: bool) -> Task<Message> {
        let Some(example) = EXAMPLES.get(index) else {
            return Task::none();
        };
        let dir = match &self.help {
            Some(help) => help.back.clone(),
            None => self.folder.clone(),
        }
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
        let taken: Vec<&Path> = self.docs.iter().filter_map(|d| d.path.as_deref()).collect();
        let path = free_name(&dir, example.stem(), example.language().extension(), &taken);
        let key = self.take_key();
        let doc = Doc::unsaved_as(key, path.clone(), example.text, &self.look);
        let at = match self.docs.get(self.active) {
            Some(current) if current.is_blank() => {
                self.docs[self.active] = doc;
                self.active
            }
            _ => {
                self.docs.push(doc);
                self.docs.len() - 1
            }
        };
        self.gallery = None;
        self.play_when_drawn = play.then_some(key);
        self.notice = Some(format!(
            "{}, a copy to change: Ctrl+S saves it as {}",
            example.title,
            home_relative(&path)
        ));
        self.keep_session();
        self.select(at)
    }

    /// Opens the chosen example.
    pub(super) fn open_chosen(&mut self, play: bool) -> Task<Message> {
        match self.gallery.as_ref().and_then(Gallery::chosen) {
            Some(index) => self.open_example(index, play),
            None => Task::none(),
        }
    }

    /// The gallery over the editor window: a veil, and on it the panel with
    /// the search, the groups, the tiles and the keys.
    pub(super) fn gallery_view(&self) -> Element<'_, Message> {
        let Some(gallery) = &self.gallery else {
            return column![].into();
        };
        let now = Instant::now();
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let (columns, thumb) = layout(self.main_size.width);
        let settled = progress(gallery.opened, VEIL, now);

        let (drawn, took) = self.thumbs.drawn();
        let total = EXAMPLES.len();
        let subtitle = if drawn < total {
            format!("drawing {drawn} of {total} …")
        } else {
            let ms = took.as_secs_f64() * 1000.0;
            let colors = if self.follow_theme {
                format!(", in the colors of {}", self.theme_name)
            } else {
                String::new()
            };
            format!("{total} diagrams, drawn offline in {ms:.0} ms{colors}")
        };
        let title = row![
            Image::new(self.mark_large.clone()).width(30).height(30),
            column![
                text("Examples")
                    .font(self.font)
                    .size(17)
                    .color(ui::color(p.bright_foreground)),
                self.small(subtitle, quiet),
            ]
            .spacing(2),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let search = ui::pill(
            row![
                self.icon(Icon::Search, quiet),
                self.field(
                    text_input("type to filter", &gallery.query)
                        .id(FIELD)
                        .on_input(Message::GalleryInput)
                        .on_submit(Message::GalleryOpen(None, false)),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            Some(ui::color(p.lighter_background)),
            None,
        )
        .padding([3, 12])
        .width(Length::Fixed(250.0));
        let head = row![container(title).width(Length::Fill).clip(true), search]
            .spacing(16)
            .align_y(Alignment::Center);

        let matching =
            |group: Option<Group>| EXAMPLES.iter().filter(|e| gallery.passes(e, group)).count();
        let mut groups: Vec<Element<'_, Message>> = vec![self.filter_pill(
            "all",
            matching(None),
            gallery.group.is_none(),
            Message::GalleryGroup(None),
        )];
        for group in Group::ALL {
            groups.push(self.filter_pill(
                group.name(),
                matching(Some(group)),
                gallery.group == Some(group),
                Message::GalleryGroup(Some(group)),
            ));
        }
        // How many complex ones the group and the search leave.
        let complex = EXAMPLES
            .iter()
            .filter(|e| {
                e.complex && gallery.group.is_none_or(|g| g == e.group) && e.matches(&gallery.query)
            })
            .count();
        let tile_width = thumb.width + TILE_PAD * 2.0;
        let width =
            tile_width * columns as f32 + GAP * (columns as f32 - 1.0) + PANEL_PAD * 2.0 + SCROLLER;
        let pills = row![
            row(groups).spacing(6).align_y(Alignment::Center),
            Space::new().width(14),
            self.filter_pill("complex", complex, gallery.complex, Message::GalleryComplex),
        ]
        .align_y(Alignment::Center);
        // The switch of colors goes under the filters where they leave it
        // no room beside them.
        let filters: Element<'_, Message> = if width - PANEL_PAD * 2.0 < FILTERS_WIDTH {
            column![
                pills,
                row![Space::new().width(Length::Fill), self.colors_switch()]
            ]
            .spacing(8)
            .into()
        } else {
            row![
                pills,
                Space::new().width(Length::Fill),
                self.colors_switch()
            ]
            .align_y(Alignment::Center)
            .into()
        };

        let grid: Element<'_, Message> = if gallery.shown.is_empty() {
            container(
                column![
                    text("No example matches")
                        .font(self.font)
                        .size(15)
                        .color(ui::color(p.bright_foreground)),
                    self.small(
                        "Backspace widens the search; Tab tries another group.",
                        quiet
                    ),
                ]
                .spacing(8)
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
        } else {
            let rows = gallery.shown.chunks(columns).enumerate().map(|(r, chunk)| {
                row(chunk.iter().enumerate().map(|(c, &index)| {
                    self.tile(gallery, index, thumb, gallery.rise(r + c, now), now)
                }))
                .spacing(GAP)
                .into()
            });
            scrollable(column(rows).spacing(GAP - RISE_BY))
                .id(GRID)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(6)
                        .scroller_width(6)
                        .spacing(SCROLLER - 6.0),
                ))
                .height(Length::Fill)
                .into()
        };

        let shown = gallery.shown.len();
        let count = if shown == total {
            format!("{total} {}", plural(total, "example"))
        } else {
            format!("{shown} of {total}")
        };
        let key = |keys: &str, what: &'static str| -> Element<'_, Message> {
            row![self.keys(keys), self.small(what, quiet)]
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
        };
        let foot = row![
            container(self.small(count, quiet)).width(Length::Fill),
            key("Enter", "opens a copy"),
            key("F5", "plays it"),
            key("Tab", "next group"),
            key("Esc", "closes"),
        ]
        .spacing(16)
        .align_y(Alignment::Center);

        let (fill, edge) = (ui::color(p.dark_background), ui::alpha(p.accent, 0.45));
        let panel = container(column![head, filters, grid, foot].spacing(14))
            .width(Length::Fixed(width))
            .height(Length::Fill)
            .padding(PANEL_PAD)
            .style(move |_: &Theme| {
                container::Style::default()
                    .background(fill)
                    .border(iced::Border {
                        color: edge,
                        width: 1.0,
                        radius: 18.0.into(),
                    })
                    .shadow(iced::Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.45 * settled),
                        offset: iced::Vector::new(0.0, 10.0),
                        blur_radius: 36.0,
                    })
            });
        let veil = Color {
            a: 0.55 * settled,
            ..ui::color(p.dark_background)
        };
        // The panel settles from a little lower as the veil comes.
        let drop = (1.0 - settled) * 16.0;
        mouse_area(
            container(opaque(panel))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .padding(Padding {
                    top: MARGIN + drop,
                    right: MARGIN,
                    bottom: MARGIN - drop.min(MARGIN),
                    left: MARGIN,
                })
                .style(move |_: &Theme| container::Style::default().background(veil)),
        )
        .on_press(Message::Command(super::Command::Cancel))
        .into()
    }

    /// One example's tile: its picture on its own paper, its kind, title,
    /// language and what it shows. `rise` is how far it has come up in the
    /// opening wave.
    fn tile<'a>(
        &'a self,
        gallery: &Gallery,
        index: usize,
        thumb: Size,
        rise: f32,
        now: Instant,
    ) -> Element<'a, Message> {
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let example = &EXAMPLES[index];
        let language = example.language();
        let lit = gallery.light(index, now);
        let hovered = gallery.hovered == Some(index);
        let shown = self.thumbs.shown.get(&index);
        let came = shown.map_or(0.0, |s| progress(s.came, THUMB_FADE, now));
        let kind = &gallery.kinds[index];

        let image_of = |picture: &Thumb, opacity: f32| -> Element<'a, Message> {
            Image::new(picture.handle.clone())
                .width(thumb.width)
                .height(thumb.height)
                .border_radius(RADIUS)
                .opacity(opacity)
                .into()
        };
        let lighter = p.lighter_background;
        let empty = ui::alpha(ui::mix(p.dark_background, lighter, 0.6), rise);
        let mut layers: Vec<Element<'a, Message>> = vec![
            container(match self.icons.get(Icon::of(Some(kind)), quiet) {
                Some(handle) => Element::from(
                    Image::new(handle)
                        .width(icons::SIZE * 2.0)
                        .height(icons::SIZE * 2.0)
                        .opacity(0.5 * rise * (1.0 - came)),
                ),
                None => Space::new().into(),
            })
            .width(thumb.width)
            .height(thumb.height)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_: &Theme| {
                container::Style::default()
                    .background(empty)
                    .border(iced::Border {
                        radius: RADIUS.into(),
                        ..iced::Border::default()
                    })
            })
            .into(),
        ];
        let mut playable = false;
        match shown.map(|s| (&s.thumb, &s.before)) {
            Some((Ok(thumb), before)) => {
                if let Some(before) = before {
                    layers.push(image_of(before, rise * (1.0 - came)));
                }
                layers.push(image_of(thumb, rise * came));
                playable = thumb.playable;
            }
            Some((Err(why), _)) => layers.push(
                container(
                    self.small(ui::one_line(why), p.red)
                        .wrapping(text::Wrapping::WordOrGlyph),
                )
                .width(thumb.width)
                .height(thumb.height)
                .padding(14)
                .align_y(Alignment::Center)
                .into(),
            ),
            None => {}
        }
        let tint = match language {
            Language::Mermaid => p.magenta,
            Language::PlantUml => quiet,
        };
        let on_picture = ui::alpha(p.dark_background, 0.86 * rise * came.max(0.5));
        layers.push(
            container(
                ui::pill(
                    text(format!("{} · {}", icons::label(kind), language.name()))
                        .font(self.font)
                        .size(10.5)
                        .color(ui::alpha(ui::mix(quiet, p.bright_foreground, 0.3), rise))
                        .wrapping(text::Wrapping::None),
                    Some(on_picture),
                    None,
                )
                .padding([1, 8]),
            )
            .width(thumb.width)
            .height(thumb.height)
            .align_y(Alignment::End)
            .padding(7)
            .into(),
        );
        if EXAMPLES[index].complex {
            layers.push(
                container(
                    ui::pill(
                        text("complex")
                            .font(self.font)
                            .size(10.5)
                            .color(ui::alpha(p.bright_foreground, rise))
                            .wrapping(text::Wrapping::None),
                        Some(on_picture),
                        Some(ui::alpha(p.magenta, 0.7 * rise * came)),
                    )
                    .padding([1, 8]),
                )
                .width(thumb.width)
                .height(thumb.height)
                .align_x(Alignment::End)
                .align_y(Alignment::End)
                .padding(7)
                .into(),
            );
        }
        if playable {
            let badge = ui::pill(
                row![
                    text("▶")
                        .font(self.font)
                        .size(9)
                        .color(ui::alpha(p.accent, rise)),
                    text("plays")
                        .font(self.font)
                        .size(10.5)
                        .color(ui::alpha(p.bright_foreground, rise)),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
                Some(on_picture),
                Some(ui::alpha(p.accent, 0.6 * rise * came)),
            )
            .padding([1, 8]);
            layers.push(
                container(
                    mouse_area(badge)
                        .on_press(Message::GalleryOpen(Some(index), true))
                        .interaction(iced::mouse::Interaction::Pointer),
                )
                .width(thumb.width)
                .height(thumb.height)
                .align_x(Alignment::End)
                .padding(7)
                .into(),
            );
        }
        let picture = stack(layers).width(thumb.width).height(thumb.height);

        let icon: Element<'a, Message> = match self.icons.get(Icon::of(Some(kind)), tint) {
            Some(handle) => Image::new(handle)
                .width(icons::SIZE)
                .height(icons::SIZE)
                .opacity(rise)
                .into(),
            None => Space::new().width(icons::SIZE).into(),
        };
        let ink = ui::mix(
            p.foreground,
            p.bright_foreground,
            lit.max(f32::from(hovered)),
        );
        let words = |s: String, color: omaspec_theme::Rgb, size: f32| {
            text(s)
                .font(self.font)
                .size(size)
                .color(ui::alpha(color, rise))
        };
        let caption = row![
            icon,
            container(words(example.title.into(), ink, 13.0).wrapping(text::Wrapping::None))
                .width(Length::Fill)
                .clip(true),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let about =
            container(words(example.about.into(), quiet, 12.0).wrapping(text::Wrapping::Word))
                .height(Length::Fixed(32.0));
        let body = column![picture, caption, about].spacing(8);

        let focus = super::view::focus_color(p, lighter);
        let rest = ui::mix(p.dark_background, lighter, 0.55);
        let fill = ui::alpha(
            ui::mix(rest, lighter, lit.max(if hovered { 0.6 } else { 0.0 })),
            rise,
        );
        let border = ui::alpha(
            ui::mix(
                ui::mix(p.dark_background, lighter, 0.9),
                focus,
                lit.max(if hovered { 0.35 } else { 0.0 }),
            ),
            rise,
        );
        let glow = ui::alpha(p.accent, 0.4 * lit * rise);
        let card = container(body)
            .padding(TILE_PAD)
            .width(Length::Fixed(thumb.width + TILE_PAD * 2.0))
            .style(move |_: &Theme| {
                container::Style::default()
                    .background(fill)
                    .border(iced::Border {
                        color: border,
                        width: 1.0,
                        radius: 16.0.into(),
                    })
                    .shadow(iced::Shadow {
                        color: glow,
                        offset: iced::Vector::new(0.0, 4.0 * lit),
                        blur_radius: 30.0,
                    })
            });
        let card = mouse_area(card)
            .on_press(Message::GalleryOpen(Some(index), false))
            .on_enter(Message::GalleryHover(index, true))
            .on_exit(Message::GalleryHover(index, false))
            .interaction(iced::mouse::Interaction::Pointer);
        // It rises from a little lower; the room it leaves stays below, so
        // the grid does not move.
        container(card)
            .id(tile_id(index))
            .padding(Padding {
                top: (1.0 - rise) * RISE_BY,
                bottom: rise * RISE_BY,
                ..Padding::ZERO
            })
            .into()
    }

    /// A filter, with how many examples it leaves; a click sends `message`.
    fn filter_pill(
        &self,
        name: &'static str,
        count: usize,
        on: bool,
        message: Message,
    ) -> Element<'_, Message> {
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let (lighter, hover, edge) = (
            ui::color(p.lighter_background),
            ui::alpha(p.lighter_background, 0.6),
            ui::alpha(p.accent, 0.45),
        );
        let badge = if on {
            ui::badge(count.to_string(), p.accent, p.background, self.font)
        } else {
            ui::badge(
                count.to_string(),
                ui::mix(p.dark_background, quiet, 0.35),
                p.foreground,
                self.font,
            )
        };
        button(
            row![
                self.small(
                    name,
                    if on {
                        p.bright_foreground
                    } else {
                        p.foreground
                    }
                ),
                badge,
            ]
            .spacing(7)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 3.0,
            right: 5.0,
            bottom: 3.0,
            left: 11.0,
        })
        .on_press(message)
        .style(move |_: &Theme, status| button::Style {
            background: match (on, status) {
                (true, _) => Some(lighter.into()),
                (false, button::Status::Hovered | button::Status::Pressed) => Some(hover.into()),
                _ => None,
            },
            border: iced::Border {
                color: if on { edge } else { Color::TRANSPARENT },
                width: 1.0,
                radius: 999.0.into(),
            },
            ..button::Style::default()
        })
        .into()
    }

    /// The switch that colors every diagram with the Omarchy theme: a knob
    /// in a pill, lit while it is on. The tiles are drawn again.
    fn colors_switch(&self) -> Element<'_, Message> {
        let p = &self.palette;
        let on = self.follow_theme;
        let quiet = ui::quiet(p);
        let knob_fill = if on { Some(ui::color(p.accent)) } else { None };
        let knob = ui::pill(
            Space::new().width(8).height(8),
            knob_fill,
            Some(ui::color(if on { p.accent } else { quiet })),
        );
        let (lighter, hover, edge) = (
            ui::alpha(p.accent, if on { 0.16 } else { 0.0 }),
            ui::alpha(p.lighter_background, 0.6),
            ui::alpha(if on { p.accent } else { p.muted }, 0.7),
        );
        button(
            row![
                knob,
                self.small(
                    "Omarchy colors",
                    if on {
                        p.bright_foreground
                    } else {
                        p.foreground
                    }
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([3, 12])
        .on_press(Message::Command(super::Command::FollowTheme))
        .style(move |_: &Theme, status| button::Style {
            background: Some(match status {
                button::Status::Hovered | button::Status::Pressed if !on => hover.into(),
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
    }
}

impl Gallery {
    /// The mouse came onto a tile, or left it.
    pub fn hover(&mut self, index: usize, inside: bool) {
        if inside {
            self.hovered = Some(index);
        } else if self.hovered == Some(index) {
            self.hovered = None;
        }
    }
}

#[cfg(test)]
impl App {
    /// Draws the open gallery's pictures right here and ends every move,
    /// for a picture of one moment.
    pub(super) fn settle_gallery(&mut self) {
        let _ = self.draw_thumbs();
        let look = self.thumbs.look;
        let scale = self.main_scale;
        let (width, height) = self.thumb_pixels();
        for (index, example) in EXAMPLES.iter().enumerate() {
            if !self.thumbs.asked.contains(&index) {
                continue;
            }
            let language = example.language();
            let (source, _) = self.diagram_source(example.text, language);
            let thumb = thumbnail(&source, language, width, height, scale);
            self.thumb_came(look, index, thumb);
        }
        let past = Instant::now()
            .checked_sub(Duration::from_secs(10))
            .unwrap_or_else(Instant::now);
        for shown in self.thumbs.shown.values_mut() {
            shown.came = past;
            shown.before = None;
        }
        if let Some(gallery) = &mut self.gallery {
            gallery.opened = past;
            gallery.chosen = past;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_example_draws_without_java_and_the_machines_play() {
        for example in EXAMPLES {
            let thumb = thumbnail(example.text, example.language(), 544, 340, 2.0)
                .unwrap_or_else(|e| panic!("{}: {e}", example.file));
            let kind = omaspec_core::classify(example.text, example.language()).kind;
            let plays = matches!(
                kind,
                omaspec_core::Kind::State | omaspec_core::Kind::Sequence
            );
            assert_eq!(thumb.playable, plays, "{}", example.file);
        }
    }

    /// With the diagrams following Omarchy, every tile lies on the theme's
    /// background: no diagram type keeps its own colors.
    #[test]
    fn every_example_takes_the_omarchy_colors() {
        use omaspec_core::skin::{self, Colors, Pack};
        let colors = Colors {
            background: "#1a1b26",
            surface: "#24283b",
            text: "#c0caf5",
            accent: "#7aa2f7",
            muted: "#565f89",
            note: "#2f334d",
        };
        for example in EXAMPLES {
            let language = example.language();
            let (range, skin) =
                skin::edit(example.text, language, Some(Pack::Omarchy), &colors).unwrap();
            let source = format!(
                "{}{skin}{}",
                &example.text[..range.start],
                &example.text[range.end..]
            );
            let thumb = thumbnail(&source, language, 272, 170, 1.0)
                .unwrap_or_else(|e| panic!("{}: {e}", example.file));
            assert_eq!(
                thumb.paper,
                Color::from_rgb8(0x1a, 0x1b, 0x26),
                "{}",
                example.file
            );
        }
    }

    #[test]
    fn the_filter_keeps_the_choice_and_steps_through_the_groups() {
        let mut g = Gallery::new(Instant::now());
        assert_eq!(g.shown().len(), EXAMPLES.len());
        g.move_by(2);
        let third = g.chosen().unwrap();
        g.set_query("mermaid".into());
        assert!(
            g.shown()
                .iter()
                .all(|&i| EXAMPLES[i].language() == Language::Mermaid)
        );
        if EXAMPLES[third].language() == Language::Mermaid {
            assert_eq!(g.chosen(), Some(third));
        }
        g.set_query(String::new());
        g.step_group(false);
        assert_eq!(g.group, Some(Group::Behavior));
        g.step_group(true);
        g.step_group(true);
        assert_eq!(g.group, Some(Group::Data));
        g.set_query("no such example".into());
        assert_eq!(g.chosen(), None);
        g.move_by(1);
        assert_eq!(g.chosen(), None);
    }

    #[test]
    fn moves_stop_at_the_ends() {
        let mut g = Gallery::new(Instant::now());
        g.move_by(-1);
        assert_eq!(g.chosen(), Some(0));
        g.move_by(1000);
        assert_eq!(g.chosen(), Some(EXAMPLES.len() - 1));
    }

    #[test]
    fn the_tiles_fill_the_panel_fewer_and_larger_in_a_narrow_window() {
        let at = |width: f32| {
            let (columns, thumb) = layout(width);
            (columns, thumb.width)
        };
        assert_eq!(at(400.0), (1, 272.0));
        // One tile as wide as it may be, not two at the narrowest.
        assert_eq!(at(700.0), (1, THUMB_WIDEST));
        assert_eq!(at(1000.0), (2, 421.0));
        assert_eq!(at(1300.0), (3, 370.0));
        assert_eq!(at(1920.0), (5, 334.0));
        assert_eq!(at(5000.0), (MAX_COLUMNS, THUMB_WIDEST));
        let (_, thumb) = layout(1300.0);
        assert_eq!(thumb.height, (thumb.width / THUMB_ASPECT).round());
    }
}
