//! Folding containers and the focus on a box, from the keyboard
//! (docs/adr/0024): the box whose name the caret is on.

use iced::Task;
use omaspec_core::recolor::{self, Target};
use omaspec_theme::{Palette, Rgb};

use super::picker::{Choice, Item, Kind, Picker};
use super::{App, Message, PICKER};

/// What a color looks like, in a word: a theme calls its colors by the
/// slots of a terminal, and its `yellow` may be a peach, its `green` an
/// orange.
fn hue_name(color: Rgb) -> &'static str {
    let [r, g, b] = [color.r, color.g, color.b].map(|c| f32::from(c) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let light = (max + min) / 2.0;
    let spread = max - min;
    let saturation = if spread < 1e-4 {
        0.0
    } else {
        spread / (1.0 - (2.0 * light - 1.0).abs())
    };
    if saturation < 0.12 {
        return match light {
            l if l > 0.9 => "White",
            l if l < 0.12 => "Black",
            _ => "Gray",
        };
    }
    let hue = 60.0
        * if max == r {
            ((g - b) / spread).rem_euclid(6.0)
        } else if max == g {
            (b - r) / spread + 2.0
        } else {
            (r - g) / spread + 4.0
        };
    match hue {
        h if !(12.0..345.0).contains(&h) => {
            if light > 0.72 {
                "Pink"
            } else {
                "Red"
            }
        }
        h if h < 20.0 && light < 0.4 => "Brown",
        h if h < 38.0 => "Orange",
        h if h < 68.0 => "Yellow",
        h if h < 165.0 => "Green",
        h if h < 200.0 => "Cyan",
        h if h < 255.0 => "Blue",
        h if h < 290.0 => "Purple",
        h if h < 320.0 => "Magenta",
        _ => "Pink",
    }
}

/// Colors that are what they are called, whatever the theme: who picks
/// yellow gets a yellow.
const BASE: [(&str, Rgb); 9] = [
    ("Red", Rgb::new(0xe5, 0x48, 0x4d)),
    ("Orange", Rgb::new(0xf7, 0x6b, 0x15)),
    ("Yellow", Rgb::new(0xff, 0xd6, 0x0a)),
    ("Green", Rgb::new(0x30, 0xa4, 0x6c)),
    ("Cyan", Rgb::new(0x12, 0xb5, 0xcb)),
    ("Blue", Rgb::new(0x3e, 0x63, 0xdd)),
    ("Purple", Rgb::new(0x8e, 0x4e, 0xc6)),
    ("Pink", Rgb::new(0xe9, 0x3d, 0x82)),
    ("Gray", Rgb::new(0x8b, 0x8d, 0x98)),
];

/// The colors a box may take: the fixed ones by their names, then the
/// theme's, the accent first, each called what it looks like (`Theme
/// orange`), one the theme has twice once, and two of a name told apart
/// (`Theme light orange`, `Theme orange 2`).
fn offered(palette: &Palette) -> Vec<(String, Rgb)> {
    let p = palette;
    let mut out: Vec<(String, Rgb)> = BASE
        .iter()
        .map(|&(name, color)| (name.to_owned(), color))
        .collect();
    let lightness = |c: Rgb| {
        let v = [c.r, c.g, c.b];
        (f32::from(*v.iter().max().unwrap_or(&0)) + f32::from(*v.iter().min().unwrap_or(&0)))
            / 510.0
    };
    let theme = out.len();
    if out.iter().all(|(_, have)| *have != p.accent) {
        out.push(("Accent".to_owned(), p.accent));
    }
    for color in [
        p.red, p.orange, p.yellow, p.green, p.cyan, p.blue, p.magenta,
    ] {
        if out.iter().any(|(_, have)| *have == color) {
            continue;
        }
        let lower = hue_name(color).to_lowercase();
        let name = format!("Theme {lower}");
        let namesake = out[theme..]
            .iter()
            .find(|(label, _)| *label == name)
            .map(|(_, other)| lightness(color) - lightness(*other));
        let mut label = match namesake {
            None => name.clone(),
            Some(lighter) if lighter > 0.06 => format!("Theme light {lower}"),
            Some(lighter) if lighter < -0.06 => format!("Theme dark {lower}"),
            Some(_) => format!("{name} 2"),
        };
        let mut nth = 1;
        while out.iter().any(|(have, _)| *have == label) {
            nth += 1;
            label = format!("{name} {nth}");
        }
        out.push((label, color));
    }
    out
}

impl App {
    /// The source on screen and where the caret is in it.
    fn caret_in_source(&self) -> (String, usize, usize) {
        let doc = self.doc();
        let cursor = doc.editor.cursor();
        (doc.shown_source(), cursor.row as usize, cursor.col as usize)
    }

    /// Focuses the box at the caret, or ends the focus.
    pub(super) fn focus_box(&mut self) -> Task<Message> {
        if self.doc().paper.focused().is_some() {
            self.doc_mut().paper.set_focus(None);
            self.notice = Some("the focus ended".into());
            return Task::none();
        }
        let (text, row, col) = self.caret_in_source();
        match self.doc().paper.box_at_caret(&text, row, col) {
            Some(name) => {
                self.notice = Some(format!(
                    "focus on {name}: it follows the caret; Shift+click a box or the paper"
                ));
                self.doc_mut().paper.set_focus(Some(name));
            }
            None => self.notice = Some("put the caret on the name of a box in the diagram".into()),
        }
        Task::none()
    }

    /// Folds the container at the caret, or unfolds it.
    pub(super) fn fold_box(&mut self) -> Task<Message> {
        let (text, row, col) = self.caret_in_source();
        let key = self.doc().key;
        match self.doc_mut().paper.fold_at_caret(&text, row, col) {
            Ok(done) => {
                self.notice = Some(done);
                self.doc_mut().paper.take_refold();
                self.render(key, true)
            }
            Err(why) => {
                self.notice = Some(why);
                Task::none()
            }
        }
    }

    /// Offers the colors of the window for the box at the caret: the one
    /// the source tells (a node of a tree, an action, a participant, a
    /// task), else the one the picture's map names.
    pub(super) fn color_box(&mut self) -> Task<Message> {
        let (text, row, col) = self.caret_in_source();
        let language = self.doc().language;
        let target = recolor::target(&text, language, row, col).or_else(|| {
            self.doc()
                .paper
                .box_at_caret(&text, row, col)
                .map(Target::Named)
        });
        let Some(target) = target else {
            self.notice = Some(
                recolor::refusal(&text, language)
                    .unwrap_or_else(|| "put the caret on a box of the diagram".into()),
            );
            return Task::none();
        };
        if let Some(why) = recolor::refusal(&text, language) {
            self.notice = Some(why);
            return Task::none();
        }
        let name = target.label().to_owned();
        let colors = offered(&self.palette);
        let item = |label: &str, color: Option<Rgb>| {
            let color = color.map(|rgb| rgb.to_string());
            Item {
                label: format!("{name}: {label}"),
                hint: color.clone().unwrap_or_default(),
                choice: Choice::Color {
                    target: target.clone(),
                    color,
                },
            }
        };
        // The fixed colors, none, then the theme's: none stays in sight.
        let mut items: Vec<Item> = Vec::with_capacity(colors.len() + 1);
        for (index, (label, rgb)) in colors.iter().enumerate() {
            if index == BASE.len() {
                items.push(item("no color of its own", None));
            }
            items.push(item(label, Some(*rgb)));
        }
        self.place_picker(Picker::new(Kind::Colors, "color the box", items));
        iced::widget::operation::focus(PICKER)
    }

    /// Colors the box `target` in the source, as one undo step, and draws
    /// the diagram again.
    pub(super) fn colored(&mut self, target: &Target, color: Option<&str>) -> Task<Message> {
        let key = self.doc().key;
        let doc = self.doc();
        let edit = recolor::edit(&doc.text(), doc.language, target, color);
        let name = target.label();
        match edit {
            Ok(edit) => {
                self.doc_mut().replace(vec![edit]);
                self.edited(key);
                self.notice = Some(match color {
                    Some(color) => format!("{name} colored {color} (Ctrl+Z undoes it)"),
                    None => format!("{name} in the diagram's colors again (Ctrl+Z undoes it)"),
                });
            }
            Err(why) => self.notice = Some(why),
        }
        Task::none()
    }

    /// Unfolds every container of the diagram on screen.
    pub(super) fn unfold_all(&mut self) -> Task<Message> {
        let key = self.doc().key;
        if !self.doc_mut().paper.unfold_all() {
            self.notice = Some("nothing is folded".into());
            return Task::none();
        }
        self.doc_mut().paper.take_refold();
        self.notice = Some("every container unfolded".into());
        self.render(key, true)
    }

    /// The focus moves with the caret onto another box, while it is on.
    pub(super) fn focus_follows(&mut self, index: usize, text: &str, row: usize, col: usize) {
        let paper = &mut self.docs[index].paper;
        if paper.focused().is_none() {
            return;
        }
        if let Some(name) = paper.box_at_caret(text, row, col) {
            paper.set_focus(Some(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_color_is_called_what_it_looks_like() {
        let hex = |value: &str| Rgb::parse_hex(value).unwrap();
        for (value, name) in [
            ("#ff5f4a", "Red"),
            ("#ff6a3d", "Orange"),
            ("#ff9f0a", "Orange"),
            ("#f9e2af", "Yellow"),
            ("#e0af68", "Orange"),
            ("#9ece6a", "Green"),
            ("#a6dcef", "Cyan"),
            ("#7aa2f7", "Blue"),
            ("#bb9af7", "Purple"),
            ("#ff8fa3", "Pink"),
            ("#8e8e93", "Gray"),
        ] {
            assert_eq!(hue_name(hex(value)), name, "{value}");
        }
        // A theme whose green is its accent and whose yellow is a peach.
        let palette = Palette {
            accent: hex("#ff9f0a"),
            red: hex("#ff5f4a"),
            orange: hex("#ff6a3d"),
            yellow: hex("#ffc27a"),
            green: hex("#ff9f0a"),
            cyan: hex("#a6dcef"),
            blue: hex("#8fb8de"),
            magenta: hex("#ff8fa3"),
            ..Palette::fallback()
        };
        let labels: Vec<String> = offered(&palette).into_iter().map(|(l, _)| l).collect();
        assert_eq!(
            labels[BASE.len()..],
            [
                "Accent",
                "Theme red",
                "Theme orange",
                "Theme light orange",
                "Theme cyan",
                "Theme blue",
                "Theme pink"
            ]
        );
        // The fixed ones are what they are called.
        for (name, color) in BASE {
            assert_eq!(hue_name(color), name);
            assert!(labels.iter().any(|l| l == name));
        }
    }
}
