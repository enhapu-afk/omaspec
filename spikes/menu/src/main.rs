//! Spike: menus for the mouse (docs/adr/0025). Two engines draw the same
//! menu bar, context menus and toolbar from one command table: `iced_aw`'s
//! `MenuBar` and `ContextMenu`, and an overlay of our own built from
//! `stack`, `mouse_area` and padding. The spike answered which one Omaspec
//! takes (the own overlay, see docs/adr/0025): how each looks in the
//! Omarchy colors, how each closes, and what the keyboard can do with it.
//!
//! Usage:
//!   spike-menu                  a window; F9 switches the engine, a tap on
//!                               Alt shows and hides the bar, F10 opens File
//!   spike-menu --snapshot DIR   pictures of both engines, menus open, in DIR
//!
//! Own engine, keys: Left/Right move between menus, Up/Down between
//! entries, Enter runs one, Escape closes. iced_aw's menus have no keys.

use std::path::Path;

use anyhow::Result;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{button, column, container, mouse_area, row, stack, text, tooltip};
use iced::{
    Border, Color, Element, Font, Length, Padding, Point, Shadow, Size, Subscription, Task, Theme,
};
use iced_aw::ContextMenu;
use iced_aw::menu::{Item, Menu as AwMenu, MenuBar};
use omaspec_theme::{Palette, Rgb, Theme as OmarchyTheme};

mod table;
use table::{COMMANDS, Cmd, Spot, Top, context_entries, entries};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, dir] if flag == "--snapshot" => snapshot(Path::new(dir)),
        [] => window(),
        [flag, ..] => anyhow::bail!("unknown option {flag}"),
    }
}

type AwItem<'a> = Item<'a, Message, Theme, iced::Renderer>;

// ---------------------------------------------------------------- the state

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Engine {
    Aw,
    Own,
}

#[derive(Debug, Clone)]
enum Message {
    Run(usize),
    SwitchEngine,
    ToggleBar,
    /// The own engine: a root was pressed or entered.
    Open(Top),
    /// The own engine: the highlight moved onto an entry.
    Point(usize),
    Close,
    Moved(Point),
    RightClick(Spot),
    Key(Key, Modifiers),
    Released(Key),
    Resized(Size),
}

struct Spike {
    palette: Palette,
    theme: Theme,
    theme_name: String,
    font: Font,
    engine: Engine,
    bar_shown: bool,
    /// Own engine: the open root menu.
    open: Option<Top>,
    /// Own engine: the entry the keys point at, as an index into COMMANDS.
    pointed: Option<usize>,
    /// Own engine: a context menu, where it was called and for what.
    context: Option<(Point, Spot)>,
    /// Where the mouse is, for the context menu.
    mouse: Point,
    /// Alt went down and nothing else since.
    alt_armed: bool,
    window: Size,
    last: String,
}

/// Width of one root of the own bar; the dropdown's x comes from it.
const ROOT_W: f32 = 76.0;
const BAR_H: f32 = 28.0;
/// Width of a menu; the longest entry with its key fits.
const MENU_W: f32 = 300.0;
/// Height of one entry, for placing a context menu inside the window.
const ENTRY_H: f32 = 25.0;
const TEXT: f32 = 13.0;
const WINDOW: Size = Size::new(1100.0, 680.0);

impl Spike {
    fn new() -> Self {
        let mut omarchy = OmarchyTheme::load_current();
        if let Some(path) = std::env::var_os("SPIKE_COLORS") {
            let path = std::path::PathBuf::from(path);
            if let Ok(p) = Palette::load(&path) {
                omarchy.palette = p;
                omarchy.name = path
                    .parent()
                    .and_then(|d| d.file_name())
                    .map(|n| n.to_string_lossy().into_owned());
            }
        }
        let palette = omarchy.palette.clone();
        Self {
            theme: iced_theme(&palette),
            palette,
            theme_name: omarchy.name.unwrap_or_else(|| "fallback".into()),
            font: Font::with_name(mono_family()),
            engine: Engine::Own,
            bar_shown: true,
            open: None,
            pointed: None,
            context: None,
            mouse: Point::ORIGIN,
            alt_armed: false,
            window: WINDOW,
            last: "nothing yet".into(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Run(i) => {
                let c = &COMMANDS[i];
                self.last = format!("{} {}", c.menu.label(), c.label.trim_end_matches('…'));
                self.open = None;
                self.pointed = None;
                self.context = None;
                if c.label == "Menu bar" {
                    return self.update(Message::ToggleBar);
                }
            }
            Message::SwitchEngine => {
                self.engine = match self.engine {
                    Engine::Aw => Engine::Own,
                    Engine::Own => Engine::Aw,
                };
                self.open = None;
                self.context = None;
            }
            Message::ToggleBar => self.bar_shown = !self.bar_shown,
            Message::Open(top) => {
                self.open = if self.open == Some(top) {
                    None
                } else {
                    Some(top)
                };
                self.pointed = None;
                self.context = None;
            }
            Message::Point(i) => self.pointed = Some(i),
            Message::Close => {
                self.open = None;
                self.pointed = None;
                self.context = None;
            }
            Message::Moved(p) => self.mouse = p,
            Message::RightClick(spot) => {
                self.open = None;
                self.context = Some((self.mouse, spot));
                self.pointed = None;
            }
            Message::Key(key, mods) => return self.key(&key, mods),
            Message::Resized(size) => self.window = size,
            Message::Released(key) => {
                if matches!(key, Key::Named(Named::Alt)) && self.alt_armed {
                    self.bar_shown = !self.bar_shown;
                }
                self.alt_armed = false;
            }
        }
        Task::none()
    }

    fn key(&mut self, key: &Key, mods: Modifiers) -> Task<Message> {
        // Alt alone shows the bar; Alt with a key is a shortcut (Alt+F fits).
        self.alt_armed = matches!(key, Key::Named(Named::Alt)) && mods.is_empty();
        match key {
            Key::Named(Named::F9) => return self.update(Message::SwitchEngine),
            Key::Named(Named::F10) if self.engine == Engine::Own => {
                self.bar_shown = true;
                return self.update(Message::Open(Top::File));
            }
            Key::Named(Named::Escape) => return self.update(Message::Close),
            _ => {}
        }
        if self.engine != Engine::Own {
            return Task::none();
        }
        let listed: Vec<usize> = match (self.open, self.context) {
            (Some(top), _) => entries(top).map(|(i, _)| i).collect(),
            (None, Some((_, spot))) => context_entries(spot).map(|(i, _)| i).collect(),
            (None, None) => return Task::none(),
        };
        let at = self
            .pointed
            .and_then(|p| listed.iter().position(|i| *i == p));
        match key {
            Key::Named(Named::ArrowDown) => {
                self.pointed = listed
                    .get(at.map_or(0, |a| a + 1))
                    .or(listed.first())
                    .copied();
            }
            Key::Named(Named::ArrowUp) => {
                self.pointed = match at {
                    Some(0) | None => listed.last().copied(),
                    Some(a) => listed.get(a - 1).copied(),
                };
            }
            Key::Named(Named::ArrowRight) if self.open.is_some() => {
                let next = (self.open.map_or(0, Top::index) + 1) % Top::ALL.len();
                self.open = Some(Top::ALL[next]);
                self.pointed = None;
            }
            Key::Named(Named::ArrowLeft) if self.open.is_some() => {
                let n = Top::ALL.len();
                let prev = (self.open.map_or(0, Top::index) + n - 1) % n;
                self.open = Some(Top::ALL[prev]);
                self.pointed = None;
            }
            Key::Named(Named::Enter) => {
                if let Some(i) = self.pointed {
                    return self.update(Message::Run(i));
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        use iced::keyboard::Event;
        Subscription::batch([
            iced::keyboard::listen().filter_map(|event| match event {
                Event::KeyPressed { key, modifiers, .. } => Some(Message::Key(key, modifiers)),
                Event::KeyReleased { key, .. } => Some(Message::Released(key)),
                _ => None,
            }),
            iced::event::listen_with(|event, _, _| match event {
                iced::Event::Window(iced::window::Event::Resized(size)) => {
                    Some(Message::Resized(size))
                }
                _ => None,
            }),
        ])
    }

    // ------------------------------------------------------------ the view

    fn view<'a>(&'a self) -> Element<'a, Message> {
        let body = row![self.editor_pane(), self.diagram_pane()].height(Length::Fill);
        let bar: Option<Element<'_, Message>> = match (self.bar_shown, self.engine) {
            (false, _) => None,
            (true, Engine::Aw) => Some(self.aw_bar()),
            (true, Engine::Own) => Some(self.own_bar()),
        };
        let mut base = column![];
        if let Some(bar) = bar {
            base = base.push(bar);
        }
        let base = base.push(body).push(self.status());
        let base: Element<'_, Message> = mouse_area(base).on_move(Message::Moved).into();
        if self.engine != Engine::Own {
            return base;
        }
        // The own engine: a backdrop that closes, and the open menu above it.
        let layer: Option<Element<'_, Message>> = match (self.open, self.context) {
            (Some(top), _) => Some(self.own_dropdown(top)),
            (None, Some((at, spot))) => Some(self.own_context(at, spot)),
            (None, None) => None,
        };
        match layer {
            None => base,
            Some(layer) => {
                let backdrop =
                    mouse_area(container(text("")).width(Length::Fill).height(Length::Fill))
                        .on_press(Message::Close)
                        .on_right_press(Message::Close);
                stack![base, backdrop, layer].into()
            }
        }
    }

    fn editor_pane<'a>(&'a self) -> Element<'a, Message> {
        let p = &self.palette;
        let sample = "@startuml\nstate Idle\nstate Running {\n  state Fast\n  state Slow\n}\nIdle --> Running : start\nRunning --> Idle : stop\n@enduml\n";
        let lines = column(sample.lines().enumerate().map(|(n, l)| {
            row![
                text(format!("{:>3} ", n + 1))
                    .font(self.font)
                    .size(TEXT)
                    .color(color(p.muted)),
                text(l)
                    .font(self.font)
                    .size(TEXT)
                    .color(color(p.foreground)),
            ]
            .into()
        }))
        .spacing(2);
        let pane = container(lines)
            .padding(10)
            .width(Length::FillPortion(2))
            .height(Length::Fill)
            .style(|_: &Theme| container::Style::default());
        self.with_context(pane.into(), Spot::Editor)
    }

    fn diagram_pane<'a>(&'a self) -> Element<'a, Message> {
        let p = &self.palette;
        let boxes = |name: &str| {
            container(
                text(name.to_owned())
                    .font(self.font)
                    .size(TEXT)
                    .color(color(p.foreground)),
            )
            .padding([6, 14])
            .style(move |_: &Theme| container::Style {
                background: Some(color(p.lighter_background).into()),
                border: Border {
                    color: color(p.foreground),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
        };
        let picture = column![
            boxes("Idle"),
            text("│ start")
                .font(self.font)
                .size(TEXT)
                .color(color(p.muted)),
            boxes("Running"),
            text("│ stop")
                .font(self.font)
                .size(TEXT)
                .color(color(p.muted)),
        ]
        .spacing(6)
        .align_x(iced::Alignment::Center);
        let picture = container(picture)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(move |_: &Theme| container::Style {
                background: Some(color(p.dark_background).into()),
                ..Default::default()
            });
        let picture = self.with_context(picture.into(), Spot::Diagram);
        column![self.toolbar(), picture]
            .width(Length::FillPortion(3))
            .height(Length::Fill)
            .into()
    }

    /// `pane` with a context menu on the right mouse button, by the engine.
    fn with_context<'a>(&'a self, pane: Element<'a, Message>, spot: Spot) -> Element<'a, Message> {
        match self.engine {
            Engine::Own => mouse_area(pane)
                .on_right_press(Message::RightClick(spot))
                .into(),
            Engine::Aw => self.aw_context(pane, spot, false),
        }
    }

    fn toolbar<'a>(&'a self) -> Element<'a, Message> {
        let p = &self.palette;
        let tools = COMMANDS
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.tool.map(|g| (i, c, g)))
            .map(|(i, c, glyph)| {
                let tip = if c.key.is_empty() {
                    c.label.to_owned()
                } else {
                    format!("{}  {}", c.label, c.key)
                };
                tooltip(
                    button(text(glyph).font(self.font).size(TEXT).center())
                        .width(28)
                        .height(24)
                        .padding(0)
                        .on_press(Message::Run(i))
                        .style(self.flat_button()),
                    container(
                        text(tip)
                            .font(self.font)
                            .size(TEXT - 1.0)
                            .color(color(p.foreground)),
                    )
                    .padding([3, 8])
                    .style(self.menu_box()),
                    tooltip::Position::Bottom,
                )
                .into()
            });
        container(row(tools).spacing(2))
            .width(Length::Fill)
            .padding([2, 6])
            .style(move |_: &Theme| container::Style {
                background: Some(color(p.darker_background).into()),
                ..Default::default()
            })
            .into()
    }

    fn status<'a>(&'a self) -> Element<'a, Message> {
        let p = &self.palette;
        let dim = |s: String| {
            text(s)
                .font(self.font)
                .size(TEXT)
                .color(color(p.dark_foreground))
        };
        let engine = match self.engine {
            Engine::Aw => "iced_aw  (keys: none)",
            Engine::Own => "own overlay  (keys: arrows, Enter, Esc)",
        };
        let bar = row![
            text(format!("engine: {engine}"))
                .font(self.font)
                .size(TEXT)
                .color(color(p.accent)),
            dim(format!("   ran: {}", self.last)),
            iced::widget::space::horizontal(),
            dim(format!(
                "{}   F9 engine   Alt bar   F10 File   right click menus",
                self.theme_name
            )),
        ];
        container(bar)
            .width(Length::Fill)
            .padding([4, 10])
            .style(move |_: &Theme| container::Style {
                background: Some(color(p.darker_background).into()),
                ..Default::default()
            })
            .into()
    }

    // ---------------------------------------------------------- styles

    fn flat_button<'a>(&'a self) -> impl Fn(&Theme, button::Status) -> button::Style + 'a {
        let p = &self.palette;
        move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hovered.then(|| color(p.selection).into()),
                text_color: color(if hovered {
                    p.selection_foreground
                } else {
                    p.foreground
                }),
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                shadow: Shadow::default(),
                snap: true,
            }
        }
    }

    /// The box a menu or a tooltip sits in.
    fn menu_box<'a>(&'a self) -> impl Fn(&Theme) -> container::Style + 'a {
        let p = &self.palette;
        move |_| container::Style {
            background: Some(color(p.background).into()),
            border: Border {
                color: color(p.muted),
                width: 1.0,
                radius: 6.0.into(),
            },
            shadow: Shadow {
                color: Color {
                    a: 0.45,
                    ..Color::BLACK
                },
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 14.0,
            },
            ..Default::default()
        }
    }

    /// A menu entry: the label, and its key at the right, dimmed.
    fn entry<'a>(&'a self, i: usize, lit: bool) -> Element<'a, Message> {
        let p = &self.palette;
        let c = &COMMANDS[i];
        let ink = if lit {
            p.selection_foreground
        } else {
            p.foreground
        };
        let key_ink = if lit {
            p.selection_foreground
        } else {
            p.dark_foreground
        };
        let line = row![
            text(c.label).font(self.font).size(TEXT).color(color(ink)),
            iced::widget::space::horizontal(),
            text(c.key)
                .font(self.font)
                .size(TEXT - 1.0)
                .color(color(key_ink)),
        ]
        .spacing(28)
        .width(Length::Fill);
        let flat = self.flat_button();
        let entry = button(line)
            .width(Length::Fill)
            .padding([4, 10])
            .on_press(Message::Run(i))
            .style(move |t, status| {
                let mut style = flat(t, status);
                if lit {
                    style.background = Some(color(p.selection).into());
                }
                style
            });
        // The mouse and the keys point at the same entry.
        mouse_area(entry).on_enter(Message::Point(i)).into()
    }

    fn separator<'a>(&'a self) -> Element<'a, Message> {
        let p = &self.palette;
        container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(move |_: &Theme| container::Style {
                background: Some(color(p.muted).into()),
                ..Default::default()
            })
            .into()
    }

    /// The entries of a menu as a column, with gaps and a child menu's
    /// entries inlined under a heading (the own engine has no child menus
    /// yet; iced_aw's are real).
    fn own_list<'a>(
        &'a self,
        items: impl Iterator<Item = (usize, &'static Cmd)>,
    ) -> Element<'a, Message> {
        let p = &self.palette;
        let mut col = column![].width(Length::Fill);
        let mut sub: Option<&str> = None;
        for (i, c) in items {
            if c.gap {
                col = col.push(container(self.separator()).padding([3, 0]));
            }
            if c.sub != sub {
                sub = c.sub;
                if let Some(name) = sub {
                    col = col.push(
                        container(
                            text(name)
                                .font(self.font)
                                .size(TEXT - 2.0)
                                .color(color(p.muted)),
                        )
                        .padding([2, 10]),
                    );
                }
            }
            col = col.push(self.entry(i, self.pointed == Some(i)));
        }
        container(col.padding(4))
            .width(MENU_W)
            .style(self.menu_box())
            .into()
    }

    // ---------------------------------------------------------- own engine

    fn own_bar<'a>(&'a self) -> Element<'a, Message> {
        let p = &self.palette;
        let roots = Top::ALL.into_iter().map(|top| {
            let open = self.open == Some(top);
            let flat = self.flat_button();
            let root = button(text(top.label()).font(self.font).size(TEXT).center())
                .width(ROOT_W)
                .height(BAR_H - 4.0)
                .padding(0)
                .on_press(Message::Open(top))
                .style(move |t, status| {
                    let mut style = flat(t, status);
                    if open {
                        style.background = Some(color(p.selection).into());
                        style.text_color = color(p.selection_foreground);
                    }
                    style
                });
            // While a menu is open, the mouse moving onto another root opens it.
            match self.open {
                Some(_) => mouse_area(root).on_enter(Message::Open(top)).into(),
                None => root.into(),
            }
        });
        container(row(roots).spacing(0))
            .width(Length::Fill)
            .height(BAR_H)
            .padding([2, 4])
            .style(move |_: &Theme| container::Style {
                background: Some(color(p.darker_background).into()),
                ..Default::default()
            })
            .into()
    }

    /// The bar again (so its roots stay clickable over the backdrop) with
    /// the open menu under its root.
    fn own_dropdown<'a>(&'a self, top: Top) -> Element<'a, Message> {
        let x = 4.0 + ROOT_W * top.index() as f32;
        let list = self.own_list(entries(top));
        column![
            self.own_bar(),
            container(list).padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: x
            }),
        ]
        .into()
    }

    /// The context menu at `at`, moved up or left when it would leave the
    /// window (its height is estimated from the entries).
    fn own_context<'a>(&'a self, at: Point, spot: Spot) -> Element<'a, Message> {
        let items: Vec<_> = context_entries(spot).collect();
        let subs = items
            .iter()
            .filter(|(_, c)| c.sub.is_some())
            .map(|(_, c)| c.sub)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let height = items.len() as f32 * ENTRY_H
            + items.iter().filter(|(_, c)| c.gap).count() as f32 * 7.0
            + subs as f32 * 20.0
            + 10.0;
        let x = at.x.min((self.window.width - MENU_W - 4.0).max(0.0));
        let y = if at.y + height > self.window.height {
            (at.y - height).max(0.0)
        } else {
            at.y
        };
        let list = self.own_list(items.into_iter());
        container(list)
            .padding(Padding {
                top: y,
                right: 0.0,
                bottom: 0.0,
                left: x,
            })
            .into()
    }

    // ---------------------------------------------------------- iced_aw

    fn aw_bar<'a>(&'a self) -> Element<'a, Message> {
        let p = &self.palette;
        let roots: Vec<AwItem<'a>> = Top::ALL
            .into_iter()
            .map(|top| {
                let root = button(text(top.label()).font(self.font).size(TEXT).center())
                    .width(ROOT_W)
                    .height(BAR_H - 4.0)
                    .padding(0)
                    .style(self.flat_button());
                Item::with_menu(root, self.aw_menu(top))
            })
            .collect();
        let bar = MenuBar::new(roots)
            .height(BAR_H)
            .padding([2, 4])
            .close_on_item_click_global(true)
            .close_on_background_click_global(true)
            .safe_bounds_margin(f32::MAX)
            .style(move |_, _| iced_aw::style::menu_bar::Style {
                bar_background: color(p.darker_background).into(),
                bar_border: Border::default(),
                bar_shadow: Shadow::default(),
                menu_background: color(p.background).into(),
                menu_border: Border {
                    color: color(p.muted),
                    width: 1.0,
                    radius: 6.0.into(),
                },
                menu_shadow: Shadow {
                    color: Color {
                        a: 0.45,
                        ..Color::BLACK
                    },
                    offset: iced::Vector::new(0.0, 4.0),
                    blur_radius: 14.0,
                },
                path: color(p.selection).into(),
                path_border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
            });
        container(bar)
            .width(Length::Fill)
            .style(move |_: &Theme| container::Style {
                background: Some(color(p.darker_background).into()),
                ..Default::default()
            })
            .into()
    }

    fn aw_menu<'a>(&'a self, top: Top) -> AwMenu<'a, Message, Theme, iced::Renderer> {
        let mut items: Vec<AwItem<'a>> = Vec::new();
        let mut child: Option<(&'static str, Vec<AwItem<'a>>)> = None;
        for (i, c) in entries(top) {
            if c.gap {
                if let Some((name, sub)) = child.take() {
                    items.push(self.aw_child(name, sub));
                }
                items.push(Item::new(container(self.separator()).padding([3, 0])));
            }
            match c.sub {
                Some(name) => {
                    if child.as_ref().is_none_or(|(n, _)| *n != name) {
                        if let Some((name, sub)) = child.take() {
                            items.push(self.aw_child(name, sub));
                        }
                        child = Some((name, Vec::new()));
                    }
                    if let Some((_, sub)) = &mut child {
                        sub.push(Item::new(self.entry(i, false)));
                    }
                }
                None => {
                    if let Some((name, sub)) = child.take() {
                        items.push(self.aw_child(name, sub));
                    }
                    items.push(Item::new(self.entry(i, false)));
                }
            }
        }
        if let Some((name, sub)) = child.take() {
            items.push(self.aw_child(name, sub));
        }
        AwMenu::new(items).width(MENU_W).offset(2.0).padding(4)
    }

    /// A child menu: its heading with a chevron, and the entries beside it.
    fn aw_child<'a>(&'a self, name: &'static str, sub: Vec<AwItem<'a>>) -> AwItem<'a> {
        let p = &self.palette;
        let head = row![
            text(name)
                .font(self.font)
                .size(TEXT)
                .color(color(p.foreground)),
            iced::widget::space::horizontal(),
            text("›")
                .font(self.font)
                .size(TEXT)
                .color(color(p.dark_foreground)),
        ]
        .width(Length::Fill);
        let head = button(head)
            .width(Length::Fill)
            .padding([4, 10])
            .style(self.flat_button());
        Item::with_menu(
            head,
            AwMenu::new(sub).width(MENU_W - 60.0).offset(6.0).padding(4),
        )
    }

    fn aw_context<'a>(
        &'a self,
        pane: Element<'a, Message>,
        spot: Spot,
        open: bool,
    ) -> Element<'a, Message> {
        let menu =
            ContextMenu::new(pane, move || self.own_list(context_entries(spot))).style(|_, _| {
                iced_aw::style::context_menu::Style {
                    background: Color::TRANSPARENT.into(),
                }
            });
        if open {
            menu.open(true).into()
        } else {
            menu.into()
        }
    }
}

// ---------------------------------------------------------------- running

fn window() -> Result<()> {
    iced::application(Spike::new, Spike::update, Spike::view)
        .title("Omaspec menu spike")
        .theme(|s: &Spike| s.theme.clone())
        .subscription(Spike::subscription)
        .window(iced::window::Settings {
            size: WINDOW,
            platform_specific: iced::window::settings::PlatformSpecific {
                application_id: "omaspec".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()?;
    Ok(())
}

fn snapshot(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut spike = Spike::new();
    type Set = Box<dyn Fn(&mut Spike)>;
    let shots: [(&str, Set); 4] = [
        (
            "own-menu",
            Box::new(|s| {
                s.engine = Engine::Own;
                s.open = Some(Top::Diagram);
                s.pointed = entries(Top::Diagram).nth(3).map(|(i, _)| i);
            }),
        ),
        (
            "own-context",
            Box::new(|s| {
                s.engine = Engine::Own;
                s.open = None;
                s.pointed = None;
                s.context = Some((Point::new(620.0, 300.0), Spot::Diagram));
            }),
        ),
        (
            "aw-bar",
            Box::new(|s| {
                s.engine = Engine::Aw;
                s.context = None;
            }),
        ),
        (
            "aw-context",
            Box::new(|s| {
                s.engine = Engine::Aw;
                s.context = Some((Point::new(620.0, 300.0), Spot::Diagram));
            }),
        ),
    ];
    for (name, set) in shots {
        set(&mut spike);
        let view: Element<'_, Message> = match (spike.engine, spike.context) {
            // iced_aw's context menu opens on a click only; `open` forces it.
            (Engine::Aw, Some((_, spot))) => spike.aw_context(spike.view(), spot, true),
            _ => spike.view(),
        };
        let settings = iced::Settings {
            default_font: spike.font,
            ..Default::default()
        };
        let mut ui = iced_test::simulator::Simulator::with_size(settings, WINDOW, view);
        let shot = ui
            .snapshot(&spike.theme)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let png = dir.join(format!("{name}.png"));
        let _ = std::fs::remove_file(&png);
        shot.matches_image(&png)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        println!("wrote {}", png.display());
    }
    Ok(())
}

fn mono_family() -> &'static str {
    let family = std::process::Command::new("fc-match")
        .args(["-f", "%{family[0]}", "monospace"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| "monospace".into());
    Box::leak(family.into_boxed_str())
}

fn color(c: Rgb) -> Color {
    Color::from_rgb8(c.r, c.g, c.b)
}

fn iced_theme(p: &Palette) -> Theme {
    Theme::custom(
        "Omarchy".to_owned(),
        iced::theme::Palette {
            background: color(p.background),
            text: color(p.foreground),
            primary: color(p.accent),
            success: color(p.green),
            warning: color(p.yellow),
            danger: color(p.red),
        },
    )
}
