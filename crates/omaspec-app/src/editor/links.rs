//! Diagrams that link (`docs/adr/0018-diagrams-that-link.md`): a click on
//! a tile of a map, or on a state that has a diagram of its own, zooms
//! into it and opens that diagram. The trail of diagrams followed sits
//! over the diagram, `shop › Orders › Checkout`; a click on one of them,
//! or Alt+Left, zooms back out.

use std::path::PathBuf;
use std::sync::Arc;

use iced::widget::{button, container, row};
use iced::{Alignment, Element, Length, Padding, Point, Rectangle, Task, Theme};
use omaspec_render::Atlas;

use super::doc::Doc;
use super::{App, Message};
use crate::paper::{self, Showing, View};

/// How long a zoom into a link waits at most for the diagram it leads to
/// to be drawn, so that it runs on one way.
const DIVE_WAIT: std::time::Duration = std::time::Duration::from_millis(160);
use crate::worker::{Alarm, Linked};

/// A diagram on the trail.
#[derive(Debug, Clone)]
pub struct Step {
    pub path: PathBuf,
    /// What the trail calls it: the words of the box it was entered by.
    pub name: String,
    /// Where it was entered, in the diagram before it, and the view that
    /// diagram had: zooming out ends there.
    pub from: Option<(Rectangle, View)>,
}

/// A link being followed: the zoom into it runs, and the diagram it
/// leads to is open in a tab of its own, drawn in the background.
#[derive(Debug, Clone)]
pub struct Following {
    from: u64,
    to: PathBuf,
    /// The tab of the diagram it leads to.
    target: u64,
    /// Whether the zoom runs: it waits for that diagram to be drawn, so it
    /// runs on one way from start to end.
    started: bool,
    name: String,
    rect: Rectangle,
    view: View,
}

impl App {
    /// The project document `doc` links into, with its own path there.
    pub(super) fn linked_of(&mut self, doc: u64) -> Option<Linked> {
        let found = self.docs.iter().find(|d| d.key == doc)?;
        let root = self.project_of(found)?;
        let own = found
            .path
            .as_deref()
            .and_then(|p| crate::specs::path_in(&root, p));
        let atlas = self.themed_atlas(&root)?;
        Some((atlas, own))
    }

    /// The diagrams of the project at `root` as the diagram pane draws
    /// them: in the Omarchy colors when the diagram follows them.
    fn themed_atlas(&mut self, root: &PathBuf) -> Option<Arc<Atlas>> {
        if let Some(found) = self.themed_atlases.get(root) {
            return Some(Arc::clone(found));
        }
        let raw = self.atlases.get(root)?;
        let themed = if self.follow_theme {
            let mut atlas = Atlas::clone(raw);
            for entry in &mut atlas.diagrams {
                entry.source = self
                    .diagram_source(&entry.source, entry.language)
                    .0
                    .to_string();
            }
            Arc::new(atlas)
        } else {
            Arc::clone(raw)
        };
        self.themed_atlases
            .insert(root.clone(), Arc::clone(&themed));
        Some(themed)
    }

    /// Takes the diagrams read for `root`; the documents of that project
    /// are drawn again when they changed.
    pub(super) fn found_atlas(&mut self, root: PathBuf, atlas: Arc<Atlas>) -> Task<Message> {
        if self.atlases.get(&root).is_some_and(|a| **a == *atlas) {
            return Task::none();
        }
        self.atlases.insert(root.clone(), atlas);
        self.themed_atlases.remove(&root);
        let keys: Vec<u64> = self
            .docs
            .iter()
            .filter(|d| self.project_of(d).as_ref() == Some(&root))
            .map(|d| d.key)
            .collect();
        Task::batch(keys.into_iter().map(|key| self.render(key, true)))
    }

    /// The colors changed: the tiles are drawn in the new ones.
    pub(super) fn forget_themed_atlases(&mut self) {
        self.themed_atlases.clear();
    }

    /// A click on a link at `at` in the diagram of `key`: zooms into it,
    /// and opens its diagram once there.
    pub(super) fn follow_link(&mut self, key: u64, at: Point) -> Task<Message> {
        let Some(doc) = self.docs.iter().find(|d| d.key == key) else {
            return Task::none();
        };
        if doc.paper.playing() {
            return Task::none();
        }
        let Some(link) = doc.paper.link_at(at).cloned() else {
            return Task::none();
        };
        let Some(root) = self.project_of(doc) else {
            return Task::none();
        };
        let file = match link.files.as_slice() {
            [file] => file.clone(),
            [] => {
                self.notice = Some(format!("no diagram of the project is called {}", link.name));
                return Task::none();
            }
            many => {
                self.notice = Some(format!(
                    "{} diagrams are called {}: {}; give one of them another title",
                    many.len(),
                    link.name,
                    many.join(", ")
                ));
                return Task::none();
            }
        };
        let rect = paper::link_rect(&link);
        let to = root.join(file);
        let view = doc.paper.view;
        // The linked diagram opens now and is drawn in the pane's size, so
        // the zoom ends where it will show.
        let index = match self.open_doc(&to) {
            Ok(index) => index,
            Err(err) => {
                self.notice = Some(format!("{} did not open: {err:#}", to.display()));
                return Task::none();
            }
        };
        let target = self.docs[index].key;
        if target == key {
            return Task::none();
        }
        let (size, factor) = (self.paper_size, self.paper_scale());
        let paper = &mut self.docs[index].paper;
        paper.view.fit = true;
        let prepared = Task::batch([
            paper.set_scale_factor(factor),
            paper.update(paper::Message::Viewport(size)),
        ])
        .map(move |m| Message::Paper(target, m));
        let drawn = self.render(target, false);
        self.following = Some(Following {
            from: key,
            to,
            target,
            started: false,
            name: link.name.replace('\n', " "),
            rect,
            view,
        });
        // Drawn already, it goes at once; else once it is, or after a while.
        self.after(Alarm::Dive, DIVE_WAIT);
        Task::batch([prepared, drawn, self.arrive()])
    }

    /// The diagram a link leads to got drawn or its picture: the zoom into
    /// the link starts, to end where that diagram will show, or, running
    /// already, learns where that is.
    pub(super) fn arrive(&mut self) -> Task<Message> {
        let Some(f) = &self.following else {
            return Task::none();
        };
        let (from, target, started, rect) = (f.from, f.target, f.started, f.rect);
        let linked = self.find(target).and_then(|d| d.paper.showing());
        if started {
            if let (Some(linked), Some(doc)) = (linked, self.find(from)) {
                doc.paper.arriving(linked);
            }
            return Task::none();
        }
        match linked {
            Some(linked) if linked.picture.is_some() => self.dive(from, rect, Some(linked)),
            _ => Task::none(),
        }
    }

    /// The diagram a link leads to took too long: the zoom starts without
    /// knowing where it will show.
    pub(super) fn dive_anyway(&mut self) -> Task<Message> {
        match &self.following {
            Some(f) if !f.started => {
                let (from, rect) = (f.from, f.rect);
                let linked = self.find(f.target).and_then(|d| d.paper.showing());
                self.dive(from, rect, linked)
            }
            _ => Task::none(),
        }
    }

    /// Starts the zoom from the diagram `from` into its link at `rect`.
    fn dive(&mut self, from: u64, rect: Rectangle, landing: Option<Showing>) -> Task<Message> {
        if let Some(f) = &mut self.following {
            f.started = true;
        }
        let Some(doc) = self.find(from) else {
            return Task::none();
        };
        let (dive, takes) = doc.paper.zoom_into(rect, landing);
        self.after(Alarm::Follow, takes);
        dive.map(move |m| Message::Paper(from, m))
    }

    /// Whether `key` is the diagram a zoom into a link leads to.
    pub(super) fn is_arriving(&self, key: u64) -> bool {
        self.following.as_ref().is_some_and(|f| f.target == key)
    }

    /// The zoom into a link has arrived: its diagram opens, and joins the
    /// trail.
    pub(super) fn followed(&mut self) -> Task<Message> {
        let Some(f) = self.following.take() else {
            return Task::none();
        };
        let Some(from) = self.docs.iter().find(|d| d.key == f.from) else {
            return Task::none();
        };
        let from_path = from.path.clone();
        let from_name = self.trail_name(from);
        // The diagram left keeps the view it had, for a tab click back.
        if let Some(doc) = self.find(f.from) {
            doc.paper.land(f.view);
        }
        let index = match self.docs.iter().position(|d| d.key == f.target) {
            Some(index) => index,
            None => match self.open_doc(&f.to) {
                Ok(index) => index,
                Err(err) => {
                    self.notice = Some(format!("{} did not open: {err:#}", f.to.display()));
                    return Task::none();
                }
            },
        };
        let at = from_path
            .as_ref()
            .and_then(|p| self.trail.iter().position(|s| &s.path == p));
        match (at, from_path) {
            (Some(at), _) => self.trail.truncate(at + 1),
            (None, Some(path)) => {
                self.trail.clear();
                self.trail.push(Step {
                    path,
                    name: from_name,
                    from: None,
                });
            }
            (None, None) => self.trail.clear(),
        }
        let to = self.docs[index]
            .path
            .clone()
            .unwrap_or_else(|| f.to.clone());
        self.trail.push(Step {
            path: to,
            name: f.name,
            from: Some((f.rect, f.view)),
        });
        self.notice = None;
        self.select(index)
    }

    /// Where the active diagram is on the trail, past its first step.
    pub(super) fn on_trail(&self) -> Option<usize> {
        let path = self.doc().path.as_ref()?;
        self.trail
            .iter()
            .position(|s| &s.path == path)
            .filter(|&at| at > 0)
    }

    /// Back to the diagram one step up the trail.
    pub(super) fn back(&mut self) -> Task<Message> {
        match self.on_trail() {
            Some(at) => self.back_to(at - 1),
            None => Task::none(),
        }
    }

    /// Back to the diagram at `index` on the trail, zooming out of the
    /// link it was left by.
    pub(super) fn back_to(&mut self, index: usize) -> Task<Message> {
        let Some(here) = self.on_trail() else {
            return Task::none();
        };
        if index >= here {
            return Task::none();
        }
        let (path, from) = (self.trail[index].path.clone(), self.trail[index + 1].from);
        // The diagram here, as it shows: it shrinks into its link's box.
        let here_shown = self.doc().paper.showing();
        let opened = match self.open_doc(&path) {
            Ok(opened) => opened,
            Err(err) => {
                self.notice = Some(format!("{} did not open: {err:#}", path.display()));
                return Task::none();
            }
        };
        self.trail.truncate(index + 1);
        let shown = self.select(opened);
        let key = self.doc().key;
        let out = match from {
            Some((rect, view)) => self
                .doc_mut()
                .paper
                .zoom_out_of(rect, view, here_shown)
                .map(move |m| Message::Paper(key, m)),
            None => Task::none(),
        };
        Task::batch([shown, out])
    }

    /// The trail over the diagram, up to the diagram on screen; `None`
    /// when it is not on one.
    pub(super) fn trail_view(&self) -> Option<Element<'_, Message>> {
        let here = self.on_trail()?;
        let p = &self.palette;
        let mut crumbs = row![].spacing(4).align_y(Alignment::Center);
        for (index, step) in self.trail[..=here].iter().enumerate() {
            if index > 0 {
                crumbs = crumbs.push(self.label("›", p.dark_foreground));
            }
            let color = if index == here {
                p.foreground
            } else {
                p.accent
            };
            let crumb = button(self.label(step.name.clone(), color))
                .padding([1, 4])
                .style(|_: &Theme, _| button::Style::default());
            crumbs = crumbs.push(if index == here {
                crumb
            } else {
                crumb.on_press(Message::BackTo(index))
            });
        }
        let back = crate::ui::color(p.dark_background);
        Some(
            container(
                container(crumbs)
                    .padding(Padding::from([3, 8]))
                    .style(move |_: &Theme| {
                        container::Style::default()
                            .background(back)
                            .border(iced::border::rounded(6))
                    }),
            )
            .padding(8)
            .width(Length::Fill)
            .into(),
        )
    }
}

impl App {
    /// What the trail calls a diagram it starts from: the project's name
    /// for its main diagram, else the diagram's title or file name.
    fn trail_name(&self, doc: &Doc) -> String {
        let main = self.project_of(doc).and_then(|root| {
            let atlas = self.atlases.get(&root)?;
            let own = crate::specs::path_in(&root, doc.path.as_deref()?)?;
            (atlas.main.as_deref() == Some(own.as_str()))
                .then(|| crate::project::read(&root)?.ok()?.name)
                .flatten()
        });
        main.unwrap_or_else(|| diagram_name(doc))
    }
}

/// A diagram's title or its file name.
fn diagram_name(doc: &Doc) -> String {
    let file = doc
        .path
        .as_deref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    omaspec_core::project::names(&doc.text(), doc.language, &file)
        .into_iter()
        .next()
        .unwrap_or_else(|| text_of(&file))
}

fn text_of(file: &str) -> String {
    file.split('.').next().unwrap_or(file).to_owned()
}
