//! Skin packs: a look for the diagram as visible source, not a hidden
//! theme, so it travels with the file, exports and copies. PlantUML gets a
//! block of `skinparam` lines after `@startuml`, Mermaid an `%%{init}%%`
//! line at the top (after front matter). A marker line names the pack;
//! putting in another pack replaces the one there.

use std::ops::Range;

use crate::lang::Language;
use crate::text::line_ranges;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pack {
    /// Black lines, no fills.
    Plain,
    /// Black and white, strict UML, a serif font: for documents.
    Spec,
    /// Larger type and round corners: for slides.
    Presentation,
    /// The colors of the active Omarchy theme, written out.
    Omarchy,
}

impl Pack {
    pub const ALL: [Pack; 4] = [Pack::Plain, Pack::Spec, Pack::Presentation, Pack::Omarchy];

    pub fn name(self) -> &'static str {
        match self {
            Pack::Plain => "plain",
            Pack::Spec => "spec",
            Pack::Presentation => "presentation",
            Pack::Omarchy => "omarchy",
        }
    }
}

/// The colors the Omarchy pack writes in, as `#rrggbb`.
#[derive(Debug, Clone, Copy)]
pub struct Colors<'a> {
    pub background: &'a str,
    /// Boxes and other shapes.
    pub surface: &'a str,
    pub text: &'a str,
    /// Lines and borders.
    pub accent: &'a str,
    /// Secondary lines: lifelines, frames.
    pub muted: &'a str,
    /// Notes.
    pub note: &'a str,
}

const PLANTUML_MARK: &str = "' skin: ";
const PLANTUML_END: &str = "' end of skin";
const MERMAID_MARK: &str = "%% skin: ";

/// The edit that puts `pack` into `code` (replacing a pack there), or takes
/// the pack out with `None`: the byte range to replace and its new text.
/// `None` when there is nothing to do.
pub fn edit(
    code: &str,
    language: Language,
    pack: Option<Pack>,
    colors: &Colors,
) -> Option<(Range<usize>, String)> {
    let lines = line_ranges(code);
    let block = match language {
        Language::PlantUml => plantuml_block(code, &lines),
        Language::Mermaid => mermaid_block(code, &lines),
    };
    let text = pack.map(|p| match language {
        Language::PlantUml if p == Pack::Omarchy && crate::kind::plantuml_block(code) == "json" => {
            json_omarchy(colors)
        }
        Language::PlantUml => plantuml(p, colors),
        Language::Mermaid => mermaid(p, colors),
    });
    match (block, text) {
        (Some(range), Some(text)) => (code[range.clone()] != text).then_some((range, text)),
        (Some(range), None) => Some((range, String::new())),
        (None, Some(text)) => {
            let at = match language {
                Language::PlantUml => plantuml_spot(code, &lines),
                Language::Mermaid => mermaid_spot(code, &lines),
            };
            Some((at..at, text))
        }
        (None, None) => None,
    }
}

/// The pack in `code`, if a marker names one.
pub fn current(code: &str) -> Option<Pack> {
    code.lines().find_map(|line| {
        let name = line
            .trim()
            .strip_prefix(PLANTUML_MARK)
            .or_else(|| line.trim().strip_prefix(MERMAID_MARK))?;
        let name = name.split_whitespace().next()?;
        Pack::ALL.into_iter().find(|p| p.name() == name)
    })
}

fn plantuml_block(code: &str, lines: &[Range<usize>]) -> Option<Range<usize>> {
    let first = lines
        .iter()
        .position(|r| code[r.clone()].trim().starts_with(PLANTUML_MARK))?;
    let last = lines[first..]
        .iter()
        .position(|r| code[r.clone()].trim() == PLANTUML_END)
        .map_or(first, |n| first + n);
    Some(lines[first].start..lines[last].end)
}

/// Right after the `@start…` line, or at the top.
fn plantuml_spot(code: &str, lines: &[Range<usize>]) -> usize {
    lines
        .iter()
        .find(|r| code[(*r).clone()].trim_start().starts_with("@start"))
        .map_or(0, |r| r.end)
}

fn mermaid_block(code: &str, lines: &[Range<usize>]) -> Option<Range<usize>> {
    let first = lines
        .iter()
        .position(|r| code[r.clone()].trim().starts_with(MERMAID_MARK))?;
    let init = lines
        .get(first + 1)
        .filter(|r| code[(*r).clone()].trim_start().starts_with("%%{"));
    Some(lines[first].start..init.map_or(lines[first].end, |r| r.end))
}

/// At the top, after front matter (`---` … `---`) and a code fence.
fn mermaid_spot(code: &str, lines: &[Range<usize>]) -> usize {
    let text = |i: usize| code[lines[i].clone()].trim();
    let mut at = 0;
    if !lines.is_empty() && text(0).starts_with("```") {
        at = 1;
    }
    if at < lines.len()
        && text(at) == "---"
        && let Some(end) = (at + 1..lines.len()).find(|&i| text(i) == "---")
    {
        at = end + 1;
    }
    lines.get(at).map_or(code.len(), |r| r.start)
}

fn plantuml(pack: Pack, c: &Colors) -> String {
    let body: Vec<String> = match pack {
        Pack::Plain => vec![
            "skinparam monochrome true".into(),
            "skinparam shadowing false".into(),
            "skinparam defaultFontName sans-serif".into(),
        ],
        Pack::Spec => vec![
            "skinparam style strictuml".into(),
            "skinparam monochrome true".into(),
            "skinparam shadowing false".into(),
            "skinparam roundCorner 0".into(),
            "skinparam defaultFontName serif".into(),
        ],
        Pack::Presentation => vec![
            "skinparam defaultFontSize 16".into(),
            "skinparam roundCorner 12".into(),
            "skinparam shadowing false".into(),
            "skinparam ArrowThickness 1.5".into(),
        ],
        Pack::Omarchy => {
            let mut lines = vec![
                format!("skinparam backgroundColor {}", c.background),
                format!("skinparam defaultFontColor {}", c.text),
                format!("skinparam ArrowColor {}", c.accent),
                format!("skinparam ArrowFontColor {}", c.text),
                "skinparam shadowing false".into(),
            ];
            for element in [
                "Participant",
                "Actor",
                "Boundary",
                "Control",
                "Entity",
                "Database",
                "Collections",
                "Queue",
                "Activity",
                "ActivityDiamond",
                "State",
                "Class",
                "Interface",
                "Object",
                "Component",
                "Usecase",
                // Deployment nodes, and the boxes of mind maps and WBS.
                "Node",
                "Package",
                "Rectangle",
                "Artifact",
                "Cloud",
                "Folder",
                "Frame",
                "Legend",
            ] {
                lines.push(format!("skinparam {element}BackgroundColor {}", c.surface));
                lines.push(format!("skinparam {element}BorderColor {}", c.accent));
            }
            lines.extend([
                format!("skinparam SequenceLifeLineBorderColor {}", c.muted),
                format!("skinparam SequenceGroupBorderColor {}", c.muted),
                format!("skinparam SequenceGroupBackgroundColor {}", c.surface),
                format!("skinparam SequenceLifeLineBackgroundColor {}", c.surface),
                format!("skinparam SequenceDividerBackgroundColor {}", c.surface),
                format!("skinparam SequenceDividerBorderColor {}", c.muted),
                format!(
                    "skinparam SequenceReferenceBackgroundColor {}",
                    c.background
                ),
                format!(
                    "skinparam SequenceReferenceHeaderBackgroundColor {}",
                    c.surface
                ),
                format!("skinparam SequenceReferenceBorderColor {}", c.muted),
                format!("skinparam SequenceBoxBackgroundColor {}", c.surface),
                format!("skinparam SequenceBoxBorderColor {}", c.muted),
                format!("skinparam NoteBackgroundColor {}", c.note),
                format!("skinparam NoteBorderColor {}", c.muted),
                format!("skinparam PartitionBorderColor {}", c.muted),
                format!("skinparam SwimlaneBorderColor {}", c.muted),
                // Start and stop are dark circles, on a dark background too.
                format!("skinparam ActivityStartColor {}", c.text),
                format!("skinparam ActivityEndColor {}", c.text),
                format!("skinparam ActivityBarColor {}", c.muted),
            ]);
            lines
        }
    };
    let mut text = format!("{PLANTUML_MARK}{} (Omaspec)\n", pack.name());
    for line in body {
        text.push_str(&line);
        text.push('\n');
    }
    text.push_str(PLANTUML_END);
    text.push('\n');
    text
}

/// The Omarchy pack for a JSON diagram. PlantUML draws one from its styles
/// alone, so the colors go into a `<style>` block.
fn json_omarchy(c: &Colors) -> String {
    format!(
        "{PLANTUML_MARK}{} (Omaspec)\n\
         <style>\n\
         document {{\n  BackGroundColor {bg}\n  FontColor {text}\n}}\n\
         jsonDiagram {{\n  \
           node {{\n    BackGroundColor {surface}\n    LineColor {accent}\n    FontColor {text}\n  }}\n  \
           highlight {{\n    BackGroundColor {accent}\n    FontColor {bg}\n  }}\n\
         }}\n\
         </style>\n\
         {PLANTUML_END}\n",
        Pack::Omarchy.name(),
        bg = c.background,
        text = c.text,
        surface = c.surface,
        accent = c.accent,
    )
}

fn mermaid(pack: Pack, c: &Colors) -> String {
    let init = match pack {
        Pack::Plain => r##"{"theme": "base", "themeVariables": {"background": "#ffffff", "primaryColor": "#ffffff", "primaryTextColor": "#000000", "primaryBorderColor": "#000000", "lineColor": "#000000", "secondaryColor": "#ffffff", "tertiaryColor": "#ffffff", "textColor": "#000000", "clusterBkg": "#ffffff", "clusterBorder": "#000000", "edgeLabelBackground": "#ffffff", "actorBkg": "#ffffff", "actorBorder": "#000000", "noteBkg": "#ffffff", "noteBorderColor": "#000000"}}"##.to_owned(),
        Pack::Spec => r#"{"theme": "neutral", "themeVariables": {"fontFamily": "serif"}}"#.to_owned(),
        Pack::Presentation => r##"{"themeVariables": {"fontSize": "18px", "primaryColor": "#e8f0fe", "primaryBorderColor": "#3367d6", "lineColor": "#3367d6", "clusterBkg": "#f5f8fd"}}"##.to_owned(),
        Pack::Omarchy => format!(
            r#"{{"theme": "base", "themeVariables": {{"background": "{bg}", "primaryColor": "{surface}", "primaryTextColor": "{text}", "primaryBorderColor": "{accent}", "lineColor": "{accent}", "secondaryColor": "{surface}", "tertiaryColor": "{bg}", "textColor": "{text}", "clusterBkg": "{bg}", "clusterBorder": "{muted}", "edgeLabelBackground": "{bg}", "actorBkg": "{surface}", "actorBorder": "{accent}", "actorLine": "{muted}", "noteBkg": "{note}", "noteBorderColor": "{muted}"}}}}"#,
            bg = c.background,
            surface = c.surface,
            text = c.text,
            accent = c.accent,
            muted = c.muted,
            note = c.note,
        ),
    };
    format!(
        "{MERMAID_MARK}{} (Omaspec)\n%%{{init: {init}}}%%\n",
        pack.name()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORS: Colors = Colors {
        background: "#121212",
        surface: "#1e1e1e",
        text: "#bebebe",
        accent: "#e68e0d",
        muted: "#555555",
        note: "#2a2a2a",
    };

    fn apply(code: &str, language: Language, pack: Option<Pack>) -> String {
        match edit(code, language, pack, &COLORS) {
            Some((range, text)) => format!("{}{text}{}", &code[..range.start], &code[range.end..]),
            None => code.to_owned(),
        }
    }

    #[test]
    fn plantuml_packs_follow_the_start_and_replace_each_other() {
        let code = "' a comment\n@startuml\nA -> B\n@enduml\n";
        let spec = apply(code, Language::PlantUml, Some(Pack::Spec));
        assert!(spec.starts_with(
            "' a comment\n@startuml\n' skin: spec (Omaspec)\nskinparam style strictuml\n"
        ));
        assert!(spec.ends_with("' end of skin\nA -> B\n@enduml\n"));
        assert_eq!(current(&spec), Some(Pack::Spec));

        let next = apply(&spec, Language::PlantUml, Some(Pack::Presentation));
        assert!(!next.contains("strictuml") && next.contains("skinparam defaultFontSize 16"));
        assert_eq!(next.matches("' skin:").count(), 1);
        // The same pack again changes nothing; none takes it out.
        assert!(edit(&next, Language::PlantUml, Some(Pack::Presentation), &COLORS).is_none());
        assert_eq!(apply(&next, Language::PlantUml, None), code);
        assert!(edit(code, Language::PlantUml, None, &COLORS).is_none());
    }

    #[test]
    fn mermaid_packs_go_on_top_after_front_matter() {
        let code = "---\ntitle: Flow\n---\nflowchart LR\n  A --> B\n";
        let plain = apply(code, Language::Mermaid, Some(Pack::Plain));
        assert!(plain.starts_with(
            "---\ntitle: Flow\n---\n%% skin: plain (Omaspec)\n%%{init: {\"theme\": \"base\""
        ));
        assert!(plain.ends_with("}}%%\nflowchart LR\n  A --> B\n"));
        let omarchy = apply(&plain, Language::Mermaid, Some(Pack::Omarchy));
        assert!(omarchy.contains("\"background\": \"#121212\"") && !omarchy.contains("#ffffff"));
        assert_eq!(current(&omarchy), Some(Pack::Omarchy));
        assert_eq!(apply(&omarchy, Language::Mermaid, None), code);
        // In a fence, inside it.
        let fenced = apply(
            "```mermaid\npie\n```\n",
            Language::Mermaid,
            Some(Pack::Spec),
        );
        assert!(fenced.starts_with("```mermaid\n%% skin: spec"));
    }

    #[test]
    fn a_json_diagram_takes_the_omarchy_colors_as_a_style() {
        let code = "@startjson\n{\"a\": 1}\n@endjson\n";
        let text = apply(code, Language::PlantUml, Some(Pack::Omarchy));
        assert!(text.starts_with("@startjson\n' skin: omarchy (Omaspec)\n<style>\n"));
        assert!(text.contains("  BackGroundColor #121212\n"));
        assert!(text.contains("    LineColor #e68e0d\n"));
        assert!(text.ends_with("</style>\n' end of skin\n{\"a\": 1}\n@endjson\n"));
        assert!(!text.contains("skinparam"));
        assert_eq!(current(&text), Some(Pack::Omarchy));
        assert_eq!(apply(&text, Language::PlantUml, None), code);
    }

    #[test]
    fn the_omarchy_pack_writes_the_colors() {
        let text = apply(
            "@startuml\n@enduml\n",
            Language::PlantUml,
            Some(Pack::Omarchy),
        );
        assert!(text.contains("skinparam backgroundColor #121212"));
        assert!(text.contains("skinparam ParticipantBorderColor #e68e0d"));
    }
}
