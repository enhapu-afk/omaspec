//! `omaspec diff -o`: what changed in a state machine, drawn over its new
//! version: a transition or a state that came in green, one that changed
//! in amber, a transition that went dashed in red between its two states
//! where both are still there, its label struck through. What went and
//! cannot be drawn so (a state, a transition from one) is said under the
//! picture. The sources stay as they are.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use omaspec_core::Language;
use omaspec_model::{Change, ChangeKind, Kind, Machine, Transition};
use omaspec_render::export::{self, Format};
use omaspec_uml::state::{Ghost, Mark, Marks};

/// The new version `new` (at `file`), with what changed since `old`
/// drawn over it, written to `output` in the format its extension names.
pub(crate) fn draw(
    (old, old_lang): (&str, Language),
    (new, new_lang): (&str, Language),
    file: Option<&Path>,
    changes: &[Change],
    output: &Path,
) -> Result<()> {
    let format = output
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(Format::from_extension)
        .filter(|format| matches!(format, Format::Svg | Format::Png | Format::Pdf))
        .with_context(|| format!("{} is no .svg, .png or .pdf", output.display()))?;
    let (Some(a), Some(b)) = (Machine::read(old, old_lang), Machine::read(new, new_lang)) else {
        bail!(
            "only what changed in a state machine is drawn; a sequence diagram's changes are listed"
        );
    };
    let marks = marks(&a, &b, changes);
    let place = file.and_then(|file| omaspec_render::Place::of(file, None));
    let rendered = omaspec_render::render_marked(new, new_lang, place.as_ref(), &marks)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let bytes = export::export(&rendered.svg, format, &export::Options::default())?;
    fs::write(output, bytes).with_context(|| format!("writing {}", output.display()))?;
    Ok(())
}

/// What `changes` from the machine `a` to `b` mark in `b`'s picture.
pub(crate) fn marks(a: &Machine, b: &Machine, changes: &[Change]) -> Marks {
    let mut marks = Marks::default();
    let (mut came, mut changed, mut went) = (0, 0, 0);
    let mut unseen: Vec<String> = Vec::new();
    let named = |m: &Machine, s: usize| {
        let state = &m.states[s];
        (state.kind == Kind::State && !state.name.is_empty()).then(|| state.name.clone())
    };
    for change in changes {
        let mark = match change.kind {
            ChangeKind::Added => Mark::Added,
            ChangeKind::Changed => Mark::Changed,
            ChangeKind::Removed => {
                let Some(line) = change.old_line else {
                    continue;
                };
                went += 1;
                if let Some(name) = state_of(a, line, change) {
                    unseen.push(format!("state {name}"));
                } else if let Some(t) = transition_at(a, line) {
                    let ends = (named(a, t.from), named(a, t.to));
                    let still = |name: &Option<String>| {
                        name.as_ref()
                            .is_some_and(|name| b.states.iter().any(|s| &s.name == name))
                    };
                    match ends {
                        (Some(from), Some(to)) if still(&ends.0) && still(&ends.1) => {
                            marks.ghosts.push(Ghost {
                                from,
                                to,
                                label: Some(trigger(t)).filter(|t| !t.is_empty()),
                            })
                        }
                        _ => unseen.push(described(a, t)),
                    }
                }
                continue;
            }
            ChangeKind::Found | ChangeKind::Resolved => continue,
        };
        let Some(line) = change.new_line else {
            continue;
        };
        if mark == Mark::Added {
            came += 1;
        } else {
            changed += 1;
        }
        if let Some(name) = state_of(b, line, change) {
            marks.states.entry(name).or_insert(mark);
        } else if transition_at(b, line).is_some() {
            let had = marks.transitions.insert(line, mark);
            // A transition that came is not also one that changed.
            if had == Some(Mark::Added) {
                marks.transitions.insert(line, Mark::Added);
            }
        }
    }
    let mut caption = format!("{came} came · {changed} changed · {went} went");
    if !unseen.is_empty() {
        caption.push_str(&format!("; went: {}", unseen.join(", ")));
    }
    marks.caption = Some(caption);
    marks
}

fn transition_at(m: &Machine, line: usize) -> Option<&Transition> {
    m.transitions.iter().find(|t| t.line == line)
}

/// The state `change` is about, when it is about one of `m` (first written
/// on `line`) rather than a transition: the diff says a state's change in
/// its name (`New state Jammed.`, `State B removed.`, `B now sits …`).
fn state_of(m: &Machine, line: usize, change: &Change) -> Option<String> {
    let said = &change.message;
    if said.starts_with("New transition") || said.starts_with("Transition removed") {
        return None;
    }
    let about = |name: &str| {
        said.ends_with(&format!(" {name} removed."))
            || said.starts_with(&format!("{name} now sits "))
            || (said.starts_with("New ")
                && (said.ends_with(&format!(" {name}.")) || said.contains(&format!(" {name} in "))))
    };
    m.states
        .iter()
        .find(|s| s.line == line && s.kind == Kind::State && !s.name.is_empty() && about(&s.name))
        .map(|s| s.name.clone())
}

fn trigger(t: &Transition) -> String {
    let mut text = t.trigger.events.join(", ");
    if let Some(guard) = &t.trigger.guard {
        text = format!("{text} [{guard}]").trim().to_owned();
    }
    if let Some(action) = &t.trigger.action {
        text = format!("{text} / {action}").trim().to_owned();
    }
    text
}

/// `Closed --> Locked : lock`.
fn described(m: &Machine, t: &Transition) -> String {
    let name = |s: usize| match m.states[s].name.as_str() {
        "" => "[*]".to_owned(),
        name => name.to_owned(),
    };
    let arrow = format!("{} --> {}", name(t.from), name(t.to));
    match trigger(t) {
        trigger if trigger.is_empty() => arrow,
        trigger => format!("{arrow} : {trigger}"),
    }
}

/// How many of each kind, for a test to read from a caption.
#[cfg(test)]
fn counted(marks: &Marks) -> std::collections::HashMap<&'static str, usize> {
    let mut out = std::collections::HashMap::new();
    for mark in marks.transitions.values().chain(marks.states.values()) {
        let name = match mark {
            Mark::Added => "added",
            Mark::Changed => "changed",
            _ => "other",
        };
        *out.entry(name).or_insert(0) += 1;
    }
    out.insert("ghosts", marks.ghosts.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
    const NEW: &str = "@startuml\n[*] --> Closed\nClosed --> Open : open [allowed]\nOpen --> Closed : close\nClosed --> Locked : lock\nOpen --> Jammed : force\nstate Jammed\n@enduml\n";

    #[test]
    fn what_came_changed_and_went_is_marked_in_the_new_machine() {
        let a = Machine::read(OLD, Language::PlantUml).unwrap();
        let b = Machine::read(NEW, Language::PlantUml).unwrap();
        let changes = omaspec_model::diff(
            OLD,
            Language::PlantUml,
            NEW,
            Language::PlantUml,
            &omaspec_model::Specs::default(),
        )
        .unwrap();
        let marks = marks(&a, &b, &changes);
        assert_eq!(marks.transitions.get(&2), Some(&Mark::Changed), "{marks:?}");
        assert_eq!(marks.transitions.get(&5), Some(&Mark::Added), "{marks:?}");
        assert_eq!(marks.states.get("Jammed"), Some(&Mark::Added), "{marks:?}");
        assert_eq!(
            marks.ghosts,
            [Ghost {
                from: "Locked".into(),
                to: "Closed".into(),
                label: Some("unlock".into())
            }]
        );
        assert_eq!(counted(&marks)["ghosts"], 1);
        let caption = marks.caption.as_deref().unwrap();
        assert!(
            caption.starts_with("2 came · 1 changed · 1 went"),
            "{caption}"
        );
        // Drawn: the ghost between the two states it joined, struck through.
        let (svg, _) = omaspec_uml::render_marked(NEW, None, &marks).unwrap();
        assert!(svg.contains("#C77700") && svg.contains("#2F9E44"), "{svg}");
        assert!(
            svg.contains("#E8590C") && svg.contains("line-through"),
            "{svg}"
        );
    }

    #[test]
    fn what_went_with_its_state_is_said_under_the_picture() {
        let old = "@startuml\n[*] --> A\nA --> B : go\nB --> A : back\n@enduml\n";
        let new = "@startuml\n[*] --> A\n@enduml\n";
        let a = Machine::read(old, Language::PlantUml).unwrap();
        let b = Machine::read(new, Language::PlantUml).unwrap();
        let changes = omaspec_model::diff(
            old,
            Language::PlantUml,
            new,
            Language::PlantUml,
            &Default::default(),
        )
        .unwrap();
        let marks = marks(&a, &b, &changes);
        assert!(marks.ghosts.is_empty());
        let caption = marks.caption.unwrap();
        assert!(
            caption.contains("went: ") && caption.contains("state B"),
            "{caption}"
        );
        assert!(caption.contains("A --> B : go"), "{caption}");
    }
}
