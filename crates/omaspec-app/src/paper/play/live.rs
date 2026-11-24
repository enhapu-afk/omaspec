//! A trace followed live, while a system or its agents write it (`omaspec
//! preview FILE --follow run.jsonl`, docs/adr/0036): each object of the
//! trace stands as a pill with its name beside the state it is in, a
//! light runs along each transition a new line takes, and an object whose
//! last line deviates stands in the color of what deviates. Beyond
//! [`PILLS`] objects in one state, a pill counts the rest.
//!
//! The file is read from where the last reading ended whenever the
//! window hears it changed, as bytes cut at the ends of lines, so a
//! letter written across two reads stays whole; a line half written waits
//! for its end. A file written anew, shorter or another file by the same
//! name, is followed from its start. The objects are followed a line at a
//! time as `omaspec trace` follows them ([`omaspec_model::TraceFollowers`]),
//! each keeping its last step: a run of hours costs what its new lines do.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use omaspec_core::lint::Severity;
use omaspec_model::{Followed, Kind, Machine, Spec, TraceFollowers, TraceStep};

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
    followers: TraceFollowers,
    /// What of a line is written, as bytes.
    rest: Vec<u8>,
    /// The bytes of the file read, and which file it was.
    read: u64,
    file: Option<u64>,
    pub(super) lines: usize,
}

/// What new lines did: each one's object and step in line order, and
/// whether they could be run along one by one.
pub(super) struct Came {
    pub(super) steps: Vec<(String, TraceStep)>,
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
            followers: TraceFollowers::new(spec.clone()),
            spec,
            rest: Vec::new(),
            read: 0,
            file: None,
            lines: 0,
        }
    }

    pub(super) fn machine(&self) -> &Machine {
        self.followers.machine()
    }

    /// Reads what was written since; the steps of the lines it completes,
    /// or `None` when nothing came. A file written anew starts over.
    pub(super) fn read_more(&mut self) -> Option<Came> {
        let mut file = File::open(&self.path).ok()?;
        let meta = file.metadata().ok()?;
        let len = meta.len();
        let which = identity(&meta);
        let mut again = false;
        if len < self.read || self.file.is_some_and(|was| was != which) {
            self.followers = TraceFollowers::new(self.spec.clone());
            self.rest.clear();
            self.read = 0;
            self.lines = 0;
            again = true;
        }
        self.file = Some(which);
        if len > self.read {
            file.seek(SeekFrom::Start(self.read)).ok()?;
            let before = self.rest.len();
            file.take(len - self.read)
                .read_to_end(&mut self.rest)
                .ok()?;
            self.read += (self.rest.len() - before) as u64;
        }
        let mut steps = Vec::new();
        while let Some(end) = self.rest.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.rest.drain(..=end).collect();
            let text = String::from_utf8_lossy(&line);
            self.lines += 1;
            if let Some(step) = self.followers.line(text.trim_end_matches(['\n', '\r'])) {
                steps.push(step);
            }
        }
        if steps.is_empty() && !again {
            return None;
        }
        let run = steps.len() <= RUN_AT_MOST;
        Some(Came { steps, run })
    }

    /// Where every object can be by now: the states to light.
    pub(super) fn lit(&self) -> Vec<usize> {
        let mut lit: Vec<usize> = Vec::new();
        for (_, followed) in self.followers.objects() {
            for &state in at(followed) {
                if !lit.contains(&state) {
                    lit.push(state);
                }
            }
        }
        lit
    }

    /// How many objects it follows.
    pub(super) fn objects(&self) -> usize {
        self.followers.objects().count()
    }

    /// The one object it follows, when there is one.
    pub(super) fn only(&self) -> Option<&str> {
        let mut objects = self.followers.objects();
        match (objects.next(), objects.next()) {
            (Some((object, _)), None) => Some(object),
            _ => None,
        }
    }

    /// Each state with objects in it, in the order of the states: their
    /// names, and whether each one's last line deviates. An object that
    /// may be in several states stands at the first; one not followed
    /// stands nowhere.
    pub(super) fn standing(&self) -> Vec<(usize, Vec<(String, bool)>)> {
        let m = self.machine();
        let mut by_state: Vec<(usize, Vec<(String, bool)>)> = Vec::new();
        for (object, followed) in self.followers.objects() {
            let Some(&state) = at(followed)
                .iter()
                .find(|&&s| matches!(m.states[s].kind, Kind::State | Kind::End))
            else {
                continue;
            };
            let entry = (object.to_owned(), deviates(followed));
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
        self.followers
            .objects()
            .filter_map(|(object, followed)| followed.steps.last().map(|s| (s, object)))
            .max_by_key(|(step, _)| step.line)
    }

    /// What deviates at the last line of an object, the latest of them.
    pub(super) fn wrong(&self) -> Option<(Severity, String)> {
        self.followers
            .objects()
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
        self.followers
            .objects()
            .filter(|(_, f)| deviates(f))
            .count()
    }
}

/// Which file `meta` is, so one put in its place by name is told apart:
/// its inode where the system has them.
#[cfg(unix)]
fn identity(meta: &std::fs::Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::ino(meta)
}

#[cfg(not(unix))]
fn identity(_: &std::fs::Metadata) -> u64 {
    0
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
