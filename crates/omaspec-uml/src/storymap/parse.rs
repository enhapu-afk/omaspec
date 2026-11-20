//! From the body's lines to a [`Diagram`]. A release may be written
//! before or after the stories that go into it; a story names one that
//! is written somewhere in the map, or it refuses. What is no line of a
//! story map is a mistake: PlantUML does not know the diagram, so no
//! other engine draws it.

use super::{Activity, Diagram, Release, Step, Story};
use crate::Declined;
use crate::common::{Frame, keyword, snippet};
use crate::source::Line;

pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut frame = Frame::default();
    let mut releases: Vec<Release> = Vec::new();
    let mut activities: Vec<Activity> = Vec::new();
    // Each story's release as written, with its line, until all are known.
    let mut named: Vec<Option<(String, usize)>> = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        if let Some(next) = frame.line(lines, at)? {
            at = next;
            continue;
        }
        let Line {
            no,
            text,
            at: start,
            ..
        } = lines[at];
        if let Some(rest) = keyword(text, "release") {
            let (name, goal) = split(rest, Colon::First);
            let name = unquote(name);
            if name.is_empty() {
                return Err(Declined::refused(no, "a release needs a name"));
            }
            if let Some(first) = releases.iter().find(|r| same(&r.name, name)) {
                return Err(Declined::refused(
                    no,
                    format!(
                        "the release {name} is written twice; the first is on line {}",
                        first.line + 1
                    ),
                ));
            }
            releases.push(Release {
                name: name.to_owned(),
                goal: goal
                    .map(unquote)
                    .filter(|g| !g.is_empty())
                    .map(str::to_owned),
                line: no,
                col: start + (name.as_ptr() as usize - text.as_ptr() as usize),
            });
        } else if let Some(rest) = keyword(text, "activity") {
            let name = unquote(rest);
            if name.is_empty() {
                return Err(Declined::refused(no, "an activity needs a name"));
            }
            activities.push(Activity {
                name: name.to_owned(),
                line: no,
                steps: Vec::new(),
            });
        } else if let Some(rest) = keyword(text, "step") {
            let name = unquote(rest);
            let Some(activity) = activities.last_mut() else {
                return Err(Declined::refused(
                    no,
                    "a step stands under an activity: write `activity …` before it",
                ));
            };
            if name.is_empty() {
                return Err(Declined::refused(no, "a step needs a name"));
            }
            activity.steps.push(Step {
                name: Some(name.to_owned()),
                line: no,
                stories: Vec::new(),
            });
        } else if let Some(rest) = text.strip_prefix('*') {
            let (words, release) = split(rest, Colon::Last);
            let words = unquote(words);
            let Some(activity) = activities.last_mut() else {
                return Err(Declined::refused(
                    no,
                    "a story stands under an activity or a step: write `activity …` before it",
                ));
            };
            if words.is_empty() {
                return Err(Declined::refused(no, "a story needs words"));
            }
            if release.is_some_and(|r| unquote(r).is_empty()) {
                return Err(Declined::refused(
                    no,
                    "a story's colon names no release: write one after it, or put a story with a colon of its own in quotes",
                ));
            }
            // Stories before the activity's first step stand in a column
            // of their own, without a step.
            if activity.steps.is_empty() {
                activity.steps.push(Step {
                    name: None,
                    line: activity.line,
                    stories: Vec::new(),
                });
            }
            let step = activity.steps.last_mut().expect("a step");
            step.stories.push(Story {
                text: words.to_owned(),
                release: None,
                line: no,
            });
            named.push(release.map(|r| (unquote(r).to_owned(), no)));
        } else {
            return Err(Declined::refused(
                no,
                format!(
                    "`{}` is no line of a story map: write `activity`, `step`, `* a story` or `release`",
                    snippet(text)
                ),
            ));
        }
        at += 1;
    }
    // Each story into its release, now that all are known.
    let mut named = named.into_iter();
    for story in activities
        .iter_mut()
        .flat_map(|a| a.steps.iter_mut().flat_map(|s| s.stories.iter_mut()))
    {
        let Some(Some((name, no))) = named.next() else {
            continue;
        };
        let Some(index) = releases.iter().position(|r| same(&r.name, &name)) else {
            let known = if releases.is_empty() {
                "the map has none yet: write `release …` for it".to_owned()
            } else {
                format!(
                    "the releases are {}",
                    releases
                        .iter()
                        .map(|r| r.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            return Err(Declined::refused(
                no,
                format!(
                    "{name} is no release: the words after a story's last colon name its release, and {known}; a story with a colon of its own goes in quotes"
                ),
            ));
        };
        story.release = Some(index);
    }
    Ok(Diagram {
        releases,
        activities,
        frame,
    })
}

/// Which colon outside quotes splits a line: a release's name ends at
/// its first, a story's words at their last.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Colon {
    First,
    Last,
}

/// `text` before and after its first or last colon outside quotes,
/// trimmed; all of it when it has none.
fn split(text: &str, which: Colon) -> (&str, Option<&str>) {
    let mut quote = false;
    let mut found = None;
    for (at, c) in text.char_indices() {
        match c {
            '"' => quote = !quote,
            ':' if !quote => {
                found = Some(at);
                if which == Colon::First {
                    break;
                }
            }
            _ => {}
        }
    }
    match found {
        Some(at) => (text[..at].trim(), Some(text[at + 1..].trim())),
        None => (text.trim(), None),
    }
}

/// `text` without the quotes around it, trimmed.
fn unquote(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .map_or(text, str::trim)
}

/// Whether two release names are one: as written, or in another case.
fn same(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(body: &str) -> Result<Diagram, Declined> {
        let source = format!("@startstorymap\n{body}\n@endstorymap\n");
        match crate::parse(&source)? {
            crate::Diagram::StoryMap(diagram) => Ok(diagram),
            other => panic!("no story map: {other:?}"),
        }
    }

    #[test]
    fn activities_steps_and_stories_are_read_into_their_releases() {
        let d = read(
            "title Shop\nactivity Find a book\n  * Browse the shelves\n  step Search\n    * By title : MVP\n    * \"Search: by author\" : later\n    * By color\nactivity Buy\nrelease MVP : sell the first book\nrelease Later",
        )
        .unwrap();
        assert_eq!(d.activities.len(), 2);
        let find = &d.activities[0];
        assert_eq!(find.name, "Find a book");
        // The story before the first step stands in a column without one.
        assert_eq!(find.steps[0].name, None);
        assert_eq!(find.steps[0].stories[0].text, "Browse the shelves");
        assert_eq!(find.steps[1].name.as_deref(), Some("Search"));
        let stories = &find.steps[1].stories;
        assert_eq!(
            stories
                .iter()
                .map(|s| (s.text.as_str(), s.release, s.line))
                .collect::<Vec<_>>(),
            [
                ("By title", Some(0), 5),
                ("Search: by author", Some(1), 6),
                ("By color", None, 7)
            ]
        );
        assert!(d.activities[1].steps.is_empty());
        assert_eq!(d.releases[0].goal.as_deref(), Some("sell the first book"));
        assert_eq!(
            (d.releases[1].name.as_str(), d.releases[1].line),
            ("Later", 10)
        );
        assert_eq!(d.releases[0].col, "release ".len());
        assert!(d.frame.title.is_some());
    }

    #[test]
    fn a_mistake_is_refused_on_its_line() {
        let refused = |body: &str| {
            let err = read(body).unwrap_err();
            assert!(err.refused, "{body}: {err:?}");
            (err.line, err.reason)
        };
        let (line, reason) = refused("activity Buy\n* Pay: by card");
        assert_eq!(line, Some(2));
        assert!(reason.contains("by card is no release"), "{reason}");
        assert!(reason.contains("in quotes"), "{reason}");
        let (_, reason) = refused("release MVP\nactivity Buy\n* Pay : Later");
        assert!(reason.contains("the releases are MVP"), "{reason}");
        let (line, reason) = refused("step Pay");
        assert_eq!(line, Some(1));
        assert!(reason.contains("under an activity"), "{reason}");
        let (_, reason) = refused("* Pay");
        assert!(reason.contains("under an activity"), "{reason}");
        let (line, reason) = refused("release MVP\nrelease mvp");
        assert_eq!(line, Some(2));
        assert!(reason.contains("written twice"), "{reason}");
        let (_, reason) = refused("activity Buy\nPay by card");
        assert!(reason.contains("no line of a story map"), "{reason}");
        let (_, reason) = refused("activity");
        assert!(reason.contains("needs a name"), "{reason}");
        let (line, reason) = refused("release MVP\nactivity Buy\n* Pay :");
        assert_eq!(line, Some(3));
        assert!(reason.contains("names no release"), "{reason}");
    }

    #[test]
    fn an_empty_map_reads() {
        let d = read("").unwrap();
        assert!(d.activities.is_empty() && d.releases.is_empty());
    }
}
