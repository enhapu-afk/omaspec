//! Helpers for window tests: diagrams on screen without a render thread.

use omaspec_render::export;
use omaspec_render::{Config, Engine, Renderer};

use super::raster::Plan;
use super::*;
use crate::worker::Answer;

pub fn drawn(source: &str, language: Language) -> Drawn {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let out = renderer.render(source, language).unwrap();
    Drawn::of(out).unwrap()
}

/// Every diagram of the Markdown document `text`, one under another.
pub fn drawn_document(text: &str) -> Drawn {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let out = renderer.render_document(text, None).unwrap();
    Drawn::of(out).unwrap()
}

/// A project's map of two tiles, shown by its cards as a large map is.
pub fn map_by_cards() -> Drawn {
    let entry = |file: &str, source: &str| {
        let language = Language::from_file_name(file).unwrap();
        omaspec_render::Entry::new(file, source.into(), language, None)
    };
    let map = "@startwbs\n* Shop\n** Orders\n** Payment\n@endwbs\n";
    let atlas = omaspec_render::Atlas {
        diagrams: vec![
            entry("system.puml", map),
            entry(
                "orders.puml",
                "@startuml\ntitle Orders\n[*] --> Open\nOpen --> Paid : pay\n@enduml\n",
            ),
            entry(
                "payment.puml",
                "@startuml\ntitle Payment\n[*] --> Waiting\nWaiting --> Done : ok\n@enduml\n",
            ),
        ],
        main: Some("system.puml".into()),
        name: None,
    };
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let out = renderer
        .render_linked(map, Language::PlantUml, None, Some("system.puml"), &atlas)
        .unwrap();
    Drawn::parsed(out, 0).unwrap()
}

/// A box of 3000 × 12000 units, larger than the overview holds sharp.
pub fn tall() -> Drawn {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="3000" height="12000"><rect x="10" y="10" width="2980" height="11980" fill="none" stroke="black"/></svg>"#;
    Drawn {
        tree: Arc::new(export::parse(svg).unwrap()),
        svg: svg.into(),
        engine: Engine::Mermaid,
        elapsed: Duration::ZERO,
        map: Arc::default(),
        links: Arc::default(),
        cards: false,
        bare: None,
    }
}

pub fn answer(doc: u64, id: u64, result: Result<Drawn, RenderError>) -> Answer {
    Answer {
        ticket: Ticket { doc, id },
        source: "".into(),
        language: Language::Mermaid,
        result,
    }
}

impl Paper {
    /// Pretends `id` requests went out.
    pub fn pretend_requested(&mut self, id: u64) {
        self.requested = id;
    }

    /// Draws the play `after` from now on, whenever it is drawn.
    pub fn freeze_play(&mut self, after: Duration) {
        self.clock = Some(Instant::now() + after);
    }

    /// Draws the play at `at`, whenever it is drawn.
    pub fn freeze_play_at(&mut self, at: Instant) {
        self.clock = Some(at);
    }

    /// When the lights of the play's steps so far have all arrived.
    pub fn play_arrived(&self) -> Option<Instant> {
        self.play.as_ref()?.arrived()
    }

    /// Pretends one more request went out; its number.
    pub fn pretend_next(&mut self) -> u64 {
        self.requested += 1;
        self.requested
    }

    pub fn viewport(&self) -> Size {
        self.viewport
    }

    /// The glide under way, a dive too.
    pub fn glide(&self) -> Option<Glide> {
        self.glide
    }

    /// What the detail layer asked for, and whether one is shown.
    pub fn detail(&self) -> (Option<Plan>, bool) {
        (self.detail.asked, self.detail.shown.is_some())
    }

    pub fn overview_shown(&self) -> bool {
        self.overview.shown.is_some()
    }

    /// Rasterizes the overview the view asked for and hands it in as
    /// the background task's message would.
    pub fn deliver_overview(&mut self) {
        let tree = Arc::clone(self.shown_tree().unwrap());
        let raster = raster::render(&tree, self.overview.asked.unwrap());
        let _ = self.update(Message::Overview(self.overview.seq(), raster));
        self.deliver_bare();
    }

    /// The pictures without lines a glide asked for, rasterized here.
    pub fn deliver_bare(&mut self) {
        let old = self
            .previous
            .as_ref()
            .and_then(|previous| previous.bare.clone())
            .zip(self.bare_old.asked);
        let new = self
            .diagram
            .as_ref()
            .and_then(|diagram| diagram.drawn.bare.clone())
            .zip(self.bare_new.asked);
        if let Some((tree, plan)) = old {
            let raster = raster::render(&tree, plan);
            let _ = self.update(Message::BareOld(self.bare_old.seq(), raster));
        }
        if let Some((tree, plan)) = new {
            let raster = raster::render(&tree, plan);
            let _ = self.update(Message::BareNew(self.bare_new.seq(), raster));
        }
    }

    /// How many pictures of the diagram before fade out, if a fade runs.
    pub fn fading(&self) -> Option<usize> {
        self.fade.as_ref().map(|fade| fade.old.len())
    }

    /// The fade that runs takes `duration` from now: for a picture of it
    /// half way.
    pub fn stretch_fade(&mut self, duration: Duration) {
        if let Some(fade) = self.fade.as_mut() {
            fade.shown = Animation::new(false)
                .duration(duration)
                .easing(iced::animation::Easing::Linear)
                .go(true, Instant::now());
        }
    }

    /// Whether a glide runs with its lines drawn on their own.
    pub fn fading_lines(&self) -> bool {
        self.fade
            .as_ref()
            .is_some_and(|fade| fade.lines.is_some() && fade.bare.is_some())
    }

    /// Rasterizes what the view asked for, as the background tasks would.
    pub fn finish_rasters(&mut self) {
        let tree = Arc::clone(self.shown_tree().unwrap());
        self.overview.finish(&tree);
        self.detail.finish(&tree);
    }

    /// Where the first label reading `text` is in the pane.
    pub fn screen_of(&self, text: &str) -> Point {
        fn find(group: &export::usvg::Group, text: &str) -> Option<export::usvg::Rect> {
            group.children().iter().find_map(|node| match node {
                export::usvg::Node::Group(g) => find(g, text),
                export::usvg::Node::Text(t) => {
                    let content: String = t.chunks().iter().map(|c| c.text()).collect();
                    (content.trim() == text).then(|| t.abs_bounding_box())
                }
                _ => None,
            })
        }
        let tree = &self.diagram.as_ref().unwrap().drawn.tree;
        let rect = find(tree.root(), text).expect(text);
        let middle = Point::new(
            rect.left() + rect.width() / 2.0,
            rect.top() + rect.height() / 2.0,
        );
        let pane = iced::Rectangle::with_size(self.viewport);
        let on_screen = self
            .view
            .on_screen(iced::Rectangle::new(middle, Size::ZERO), pane);
        on_screen.position()
    }

    /// Where the line `key` draws runs, half way along its longest run,
    /// in the pane; and the way down the pane, across that run.
    /// Where the middle of the box `name` is on screen.
    pub fn screen_of_box(&self, name: &str) -> Point {
        let map = &self.diagram.as_ref().unwrap().drawn.map;
        let place = map
            .places
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no box {name}"));
        let r = place.rect;
        let middle = Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0);
        let pane = iced::Rectangle::with_size(self.viewport);
        self.view
            .on_screen(iced::Rectangle::new(middle, Size::ZERO), pane)
            .position()
    }

    /// The containers folded, by name.
    pub fn folded(&self) -> &[String] {
        &self.folds
    }

    pub fn screen_of_line(&self, key: &str) -> (Point, Vector) {
        let map = &self.diagram.as_ref().unwrap().drawn.map;
        let wire = map
            .wires
            .iter()
            .find(|w| w.key.to_string() == key)
            .unwrap_or_else(|| panic!("no line {key}"));
        let (a, b) = wire
            .points
            .windows(2)
            .map(|w| (w[0], w[1]))
            .max_by(|x, y| {
                let len = |(a, b): ((f32, f32), (f32, f32))| (b.0 - a.0).hypot(b.1 - a.1);
                len(*x).total_cmp(&len(*y))
            })
            .unwrap();
        let middle = Point::new((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let pane = iced::Rectangle::with_size(self.viewport);
        let at = self
            .view
            .on_screen(iced::Rectangle::new(middle, Size::ZERO), pane)
            .position();
        let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-3);
        (at, Vector::new(-(b.1 - a.1) / len, (b.0 - a.0) / len))
    }

    /// The line the hand shapes now, in the drawing's units.
    pub fn hand_shape(&self) -> Option<Vec<(f32, f32)>> {
        let hand = self.hand.as_ref()?;
        let map = &self.diagram.as_ref()?.drawn.map;
        let wire = map.wires.get(hand.wire)?;
        Some(
            wire.shaped(&hand.corners, map.fit)
                .into_iter()
                .map(|p| map.fit.back(p))
                .collect(),
        )
    }

    /// The runs of line `key` as laid, in the drawing's units.
    pub fn path_of_line(&self, key: &str) -> Vec<(f32, f32)> {
        let map = &self.diagram.as_ref().unwrap().drawn.map;
        let wire = map.wires.iter().find(|w| w.key.to_string() == key).unwrap();
        wire.path.iter().map(|&p| map.fit.back(p)).collect()
    }

    /// A raster of what the detail asks for now.
    pub fn early_detail(&self) -> Option<Raster> {
        let tree = &self.diagram.as_ref().unwrap().drawn.tree;
        raster::render(tree, self.detail.asked?)
    }
}
