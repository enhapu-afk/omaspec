//! A trace followed live, while a system or its agents write it (`omaspec
//! preview FILE --follow run.jsonl`, docs/adr/0036): each object of the
//! trace stands as a pill with its name beside the state it is in, a
//! light runs along each transition a new line takes, and an object whose
//! last line deviates stands in the color of what deviates. Beyond
//! [`PILLS`] objects in one state, a pill counts the rest.
//!
//! The file is read from where the last reading ended whenever the
//! window hears it changed; a line half written waits for its end, a file
//! written anew is followed from its start. The objects are followed as
//! `omaspec trace` follows them ([`omaspec_model::follow_traces`]).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use omaspec_core::lint::Severity;
use omaspec_model::{Followed, Kind, Machine, Spec, TraceStep};

/// Pills with names beside one state at most; one more counts the rest.
pub(super) const PILLS: usize = 3;

/// New lines past this many at once are shown where they lead, not run
/// along one by one: a trace read from its middle, or a burst.
const RUN_AT_MOST: usize = 8;

pub(super) struct LiveRun {
    pub(super) path: PathBuf,
    /// The trace's file name.
    pub(super) name: String,
    spec: Spec,
    /// The trace's complete lines so far, and what of a line is written.
    text: String,
    rest: String,
    /// The bytes of the file read.
    read: u64,
    /// Each object of the machine, followed through the lines so far, in
    /// the order they first appear.
    pub(super) objects: Vec<(String, Followed)>,
    /// How many of each object's steps were shown.
    shown: HashMap<String, usize>,
    pub(super) lines: usize,
}

/// What new lines did: their steps in line order, and whether they could
/// be run along one by one.
pub(super) struct Came {
    pub(super) steps: Vec<TraceStep>,
    pub(super) run: bool,
}

impl LiveRun {
    pub(super) fn new(path: PathBuf, spec: Spec) -> LiveRun {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        LiveRun {
            path,
            name,
            spec,
            text: String::new(),
            rest: String::new(),
            read: 0,
            objects: Vec::new(),
            shown: HashMap::new(),
            lines: 0,
        }
    }

    pub(super) fn machine(&self) -> &Machine {
        &self.spec.machine
    }

    /// Reads what was written since; the steps of the lines it completes,
    /// or `None` when nothing came. A file written anew starts over.
    pub(super) fn read_more(&mut self) -> Option<Came> {
        let mut file = File::open(&self.path).ok()?;
        let len = file.metadata().ok()?.len();
        let mut again = false;
        if len < self.read {
            self.text.clear();
            self.rest.clear();
            self.read = 0;
            self.lines = 0;
            self.objects.clear();
            self.shown.clear();
            again = true;
        }
        if len > self.read {
            file.seek(SeekFrom::Start(self.read)).ok()?;
            let mut bytes = Vec::new();
            file.take(len - self.read).read_to_end(&mut bytes).ok()?;
            self.read += bytes.len() as u64;
            self.rest.push_str(&String::from_utf8_lossy(&bytes));
        }
        let Some(end) = self.rest.rfind('\n') else {
            return again.then(|| self.came());
        };
        let complete: String = self.rest.drain(..=end).collect();
        self.lines += complete.lines().count();
        self.text.push_str(&complete);
        Some(self.came())
    }

    /// The steps not shown yet, in line order.
    fn came(&mut self) -> Came {
        self.objects = omaspec_model::follow_traces(&self.text, &self.spec);
        let mut steps: Vec<TraceStep> = Vec::new();
        for (object, followed) in &self.objects {
            let shown = self.shown.entry(object.clone()).or_insert(0);
            steps.extend(followed.steps.iter().skip(*shown).cloned());
            *shown = followed.steps.len();
        }
        steps.sort_by_key(|step| step.line);
        let run = steps.len() <= RUN_AT_MOST;
        Came { steps, run }
    }

    /// Where every object can be by now: the states to light.
    pub(super) fn lit(&self) -> Vec<usize> {
        let mut lit: Vec<usize> = Vec::new();
        for (_, followed) in &self.objects {
            for &state in at(followed) {
                if !lit.contains(&state) {
                    lit.push(state);
                }
            }
        }
        lit
    }

    /// Each state with objects in it, in the order of the states: their
    /// names, and whether each one's last line deviates. An object that
    /// may be in several states stands at the first; one not followed
    /// stands nowhere.
    pub(super) fn standing(&self) -> Vec<(usize, Vec<(String, bool)>)> {
        let m = self.machine();
        let mut by_state: Vec<(usize, Vec<(String, bool)>)> = Vec::new();
        for (object, followed) in &self.objects {
            let Some(&state) = at(followed)
                .iter()
                .find(|&&s| matches!(m.states[s].kind, Kind::State | Kind::End))
            else {
                continue;
            };
            let entry = (object.clone(), deviates(followed));
            match by_state.iter_mut().find(|(s, _)| *s == state) {
                Some((_, names)) => names.push(entry),
                None => by_state.push((state, vec![entry])),
            }
        }
        by_state.sort_by_key(|(state, _)| *state);
        by_state
    }

    /// The last line written of any object, and the object.
    pub(super) fn last(&self) -> Option<(&TraceStep, &str)> {
        self.objects
            .iter()
            .filter_map(|(object, followed)| followed.steps.last().map(|s| (s, object.as_str())))
            .max_by_key(|(step, _)| step.line)
    }

    /// What deviates at the last line of an object, the latest of them.
    pub(super) fn wrong(&self) -> Option<(Severity, String)> {
        self.objects
            .iter()
            .filter_map(|(_, followed)| followed.steps.last())
            .filter(|step| {
                step.finding
                    .as_ref()
                    .is_some_and(|f| f.severity == Severity::Error)
            })
            .max_by_key(|step| step.line)
            .and_then(|step| step.finding.as_ref())
            .map(|f| (f.severity, f.message.clone()))
    }

    /// Objects whose last line deviates.
    pub(super) fn deviating(&self) -> usize {
        self.objects.iter().filter(|(_, f)| deviates(f)).count()
    }
}

/// Where `followed` can be after its last line.
fn at(followed: &Followed) -> &[usize] {
    match followed.steps.last() {
        Some(step) => &step.at,
        None => &followed.begins,
    }
}

fn deviates(followed: &Followed) -> bool {
    followed
        .steps
        .last()
        .and_then(|step| step.finding.as_ref())
        .is_some_and(|f| f.severity == Severity::Error)
}

/// The pills of `names` beside the state drawn at `state` on screen: a
/// name each, in the color of what deviates where its last line does,
/// and past [`PILLS`] one that counts the rest.
pub(super) fn pills(
    frame: &mut iced::widget::canvas::Frame,
    look: &super::Look,
    state: iced::Rectangle,
    names: &[(String, bool)],
) {
    use iced::widget::canvas::{Stroke, Text};
    use iced::{Point, Rectangle, alignment};
    const SIZE: f32 = 12.0;
    const HIGH: f32 = 18.0;
    let mut x = state.x + state.width + 6.0;
    let y = state.y + (state.height - HIGH) / 2.0;
    let more = names.len().saturating_sub(PILLS);
    let shown = names
        .iter()
        .take(PILLS)
        .map(|(name, wrong)| (name.clone(), *wrong));
    let counted = (more > 0).then(|| (format!("+{more}"), false));
    for (words, wrong) in shown.chain(counted) {
        let color = if wrong { look.wrong } else { look.accent };
        // A monospace face is as wide as six tenths of its size a letter.
        let width = words.chars().count() as f32 * SIZE * 0.6 + 14.0;
        let pill = Rectangle {
            x,
            y,
            width,
            height: HIGH,
        };
        let shape = super::draw::rounded(pill, HIGH / 2.0);
        frame.fill(&shape, look.veil);
        frame.stroke(&shape, Stroke::default().with_color(color).with_width(1.5));
        frame.fill_text(Text {
            content: words,
            position: Point::new(x + width / 2.0, y + HIGH / 2.0),
            color,
            size: SIZE.into(),
            font: look.font,
            align_x: alignment::Horizontal::Center.into(),
            align_y: alignment::Vertical::Center,
            ..Text::default()
        });
        x += width + 4.0;
    }
}
