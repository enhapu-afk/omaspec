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
    /// Shapes drawn by hand: PlantUML's `handwritten`, Mermaid's
    /// `handDrawn` look (docs/adr/0023), in pastel colors on warm paper.
    Sketch,
    /// A palette by kind: blue boxes, violet interfaces, green stores,
    /// amber actors, yellow notes, on white.
    Vivid,
}

impl Pack {
    pub const ALL: [Pack; 6] = [
        Pack::Plain,
        Pack::Spec,
        Pack::Presentation,
        Pack::Omarchy,
        Pack::Sketch,
        Pack::Vivid,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Pack::Plain => "plain",
            Pack::Spec => "spec",
            Pack::Presentation => "presentation",
            Pack::Omarchy => "omarchy",
            Pack::Sketch => "sketch",
            Pack::Vivid => "vivid",
        }
    }
}

/// The colors of a pack with a palette of its own, by the kind of element.
struct Swatch {
    paper: &'static str,
    text: &'static str,
    /// Lines, arrows and the bars of an activity.
    line: &'static str,
    /// Lifelines, frames of groups, partitions.
    muted: &'static str,
    /// Fill and border of boxes: states, classes, activities, components…
    boxes: (&'static str, &'static str),
    interface: (&'static str, &'static str),
    diamond: (&'static str, &'static str),
    /// Containers: packages, nodes, frames, folders.
    containers: (&'static str, &'static str),
    /// People and participants of a sequence.
    people: (&'static str, &'static str),
    /// Databases, queues, collections.
    stores: (&'static str, &'static str),
    note: (&'static str, &'static str),
    /// Start and end marks.
    marks: &'static str,
}

/// Black lines and frames on white, nothing filled: for print and for
/// documents, with `monochrome` for whatever else carries a color.
const PLAIN: Swatch = Swatch {
    paper: "#FFFFFF",
    text: "#000000",
    line: "#000000",
    muted: "#000000",
    boxes: ("#FFFFFF", "#000000"),
    interface: ("#FFFFFF", "#000000"),
    diamond: ("#FFFFFF", "#000000"),
    containers: ("#FFFFFF", "#000000"),
    people: ("#FFFFFF", "#000000"),
    stores: ("#FFFFFF", "#000000"),
    note: ("#FFFFFF", "#000000"),
    marks: "#000000",
};

/// Pastel fills in dark ink on warm paper, as a sketch on a whiteboard.
const SKETCH: Swatch = Swatch {
    paper: "#FFFDF6",
    text: "#1E1E1E",
    line: "#1E1E1E",
    muted: "#6C757D",
    boxes: ("#D0EBFF", "#1E1E1E"),
    interface: ("#E5DBFF", "#1E1E1E"),
    diamond: ("#FFD6E7", "#1E1E1E"),
    containers: ("#F8F9FA", "#1E1E1E"),
    people: ("#FFE8CC", "#1E1E1E"),
    stores: ("#D3F9D8", "#1E1E1E"),
    note: ("#FFF3BF", "#1E1E1E"),
    marks: "#1E1E1E",
};

/// Clear colors by kind on white: blue boxes, violet interfaces, pink
/// diamonds, green stores, amber people, yellow notes, slate lines.
const VIVID: Swatch = Swatch {
    paper: "#FFFFFF",
    text: "#1F2328",
    line: "#475569",
    muted: "#64748B",
    boxes: ("#DBEAFE", "#2563EB"),
    interface: ("#EDE9FE", "#7C3AED"),
    diamond: ("#FCE7F3", "#DB2777"),
    containers: ("#F8FAFC", "#64748B"),
    people: ("#FEF3C7", "#B45309"),
    stores: ("#DCFCE7", "#16A34A"),
    note: ("#FEF9C3", "#A16207"),
    marks: "#1F2328",
};

/// One blue on white, as the presentation pack draws in Mermaid.
const PRESENTATION: Swatch = Swatch {
    paper: "#FFFFFF",
    text: "#1F2328",
    line: "#3367D6",
    muted: "#5B7FC7",
    boxes: ("#E8F0FE", "#3367D6"),
    interface: ("#E8F0FE", "#3367D6"),
    diamond: ("#E8F0FE", "#3367D6"),
    containers: ("#F5F8FD", "#5B7FC7"),
    people: ("#E8F0FE", "#3367D6"),
    stores: ("#E8F0FE", "#3367D6"),
    note: ("#FFF8DC", "#A17C0F"),
    marks: "#1F2328",
};

/// The skinparam lines that paint a swatch, every element kind the native
/// engine colors.
fn painted(s: &Swatch) -> Vec<String> {
    let mut lines = vec![
        format!("skinparam backgroundColor {}", s.paper),
        format!("skinparam defaultFontColor {}", s.text),
        format!("skinparam ArrowColor {}", s.line),
        format!("skinparam ArrowFontColor {}", s.text),
    ];
    let kinds: [(&[&str], (&str, &str)); 6] = [
        (
            &[
                "Activity",
                "State",
                "Class",
                "Object",
                "Component",
                "Usecase",
                "Rectangle",
                "Legend",
            ],
            s.boxes,
        ),
        (&["Interface"], s.interface),
        (&["ActivityDiamond"], s.diamond),
        (
            &["Node", "Package", "Artifact", "Cloud", "Folder", "Frame"],
            s.containers,
        ),
        (
            &["Participant", "Actor", "Boundary", "Control", "Entity"],
            s.people,
        ),
        (&["Database", "Collections", "Queue"], s.stores),
    ];
    for (names, (fill, border)) in kinds {
        for name in names {
            lines.push(format!("skinparam {name}BackgroundColor {fill}"));
            lines.push(format!("skinparam {name}BorderColor {border}"));
        }
    }
    lines.extend([
        format!("skinparam NoteBackgroundColor {}", s.note.0),
        format!("skinparam NoteBorderColor {}", s.note.1),
        format!("skinparam SequenceLifeLineBorderColor {}", s.muted),
        format!("skinparam SequenceLifeLineBackgroundColor {}", s.boxes.0),
        format!("skinparam SequenceGroupBorderColor {}", s.muted),
        format!("skinparam SequenceGroupBackgroundColor {}", s.paper),
        format!(
            "skinparam SequenceDividerBackgroundColor {}",
            s.containers.0
        ),
        format!("skinparam SequenceDividerBorderColor {}", s.muted),
        format!("skinparam SequenceReferenceBackgroundColor {}", s.paper),
        format!(
            "skinparam SequenceReferenceHeaderBackgroundColor {}",
            s.containers.0
        ),
        format!("skinparam SequenceReferenceBorderColor {}", s.muted),
        format!("skinparam SequenceBoxBackgroundColor {}", s.containers.0),
        format!("skinparam SequenceBoxBorderColor {}", s.muted),
        format!("skinparam PartitionBorderColor {}", s.muted),
        format!("skinparam SwimlaneBorderColor {}", s.muted),
        format!("skinparam ActivityStartColor {}", s.marks),
        format!("skinparam ActivityEndColor {}", s.marks),
        format!("skinparam ActivityBarColor {}", s.line),
    ]);
    lines
}

/// The Mermaid theme variables that paint a swatch. Mermaid colors every
/// node of a flowchart alike; Omaspec's drawer takes `secondaryColor` for
/// the decisions, `tertiaryColor` for the stores and `actorBkg` for the
/// terminals when a pack is in the file.
fn mermaid_painted(s: &Swatch) -> String {
    format!(
        r#""theme": "base", "themeVariables": {{"background": "{paper}", "primaryColor": "{fill}", "primaryTextColor": "{text}", "primaryBorderColor": "{border}", "lineColor": "{line}", "secondaryColor": "{decision}", "secondaryBorderColor": "{decision_border}", "tertiaryColor": "{stores}", "tertiaryBorderColor": "{stores_border}", "textColor": "{text}", "clusterBkg": "{cluster}", "clusterBorder": "{cluster_border}", "edgeLabelBackground": "{paper}", "actorBkg": "{people}", "actorBorder": "{people_border}", "actorLine": "{muted}", "noteBkg": "{note}", "noteBorderColor": "{note_border}"}}"#,
        paper = s.paper,
        fill = s.boxes.0,
        text = s.text,
        border = s.boxes.1,
        line = s.line,
        decision = s.diamond.0,
        decision_border = s.diamond.1,
        stores = s.stores.0,
        stores_border = s.stores.1,
        cluster = s.containers.0,
        cluster_border = s.containers.1,
        people = s.people.0,
        people_border = s.people.1,
        muted = s.muted,
        note = s.note.0,
        note_border = s.note.1,
    )
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
    edit_in(code, language, pack, colors, None)
}

/// [`edit`], the pack written in `font` (a name [`is_font`] takes) when
/// one is given: `skinparam defaultFontName` in PlantUML, `fontFamily` in
/// Mermaid.
pub fn edit_in(
    code: &str,
    language: Language,
    pack: Option<Pack>,
    colors: &Colors,
    font: Option<&str>,
) -> Option<(Range<usize>, String)> {
    let font = font.filter(|f| is_font(f));
    let lines = line_ranges(code);
    let block = match language {
        Language::PlantUml => plantuml_block(code, &lines),
        Language::Mermaid => mermaid_block(code, &lines),
    };
    let text = pack.map(|p| match language {
        Language::PlantUml if p == Pack::Omarchy => {
            match crate::kind::plantuml_block(code).as_str() {
                "json" => tables_omarchy("jsonDiagram", colors, font),
                "yaml" => tables_omarchy("yamlDiagram", colors, font),
                "nwdiag" => network_omarchy(colors, font),
                _ => plantuml(p, colors, font),
            }
        }
        Language::PlantUml => plantuml(p, colors, font),
        Language::Mermaid => mermaid(p, colors, font),
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

/// Whether `name` can be a font a pack is written in: letters, digits,
/// spaces, `-`, `_` and `.`, at most 64 characters, so it stays one value
/// in a skinparam line and in Mermaid's JSON.
pub fn is_font(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.chars().count() <= 64
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
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

fn plantuml(pack: Pack, c: &Colors, font: Option<&str>) -> String {
    let mut body: Vec<String> = match pack {
        Pack::Plain => {
            let mut lines = vec![
                "skinparam monochrome true".into(),
                "skinparam shadowing false".into(),
                "skinparam defaultFontName sans-serif".into(),
            ];
            lines.extend(painted(&PLAIN));
            lines
        }
        Pack::Spec => vec![
            "skinparam style strictuml".into(),
            "skinparam monochrome true".into(),
            "skinparam shadowing false".into(),
            "skinparam roundCorner 0".into(),
            "skinparam defaultFontName serif".into(),
        ],
        Pack::Presentation => {
            let mut lines = vec![
                "skinparam defaultFontSize 16".into(),
                "skinparam roundCorner 12".into(),
                "skinparam shadowing false".into(),
                "skinparam ArrowThickness 1.5".into(),
            ];
            lines.extend(painted(&PRESENTATION));
            lines
        }
        Pack::Sketch => {
            let mut lines = vec![
                "skinparam handwritten true".into(),
                "skinparam shadowing false".into(),
            ];
            lines.extend(painted(&SKETCH));
            lines
        }
        Pack::Vivid => {
            let mut lines = vec!["skinparam shadowing false".into()];
            lines.extend(painted(&VIVID));
            lines
        }
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
    if let Some(font) = font {
        body.retain(|line| !line.starts_with("skinparam defaultFontName "));
        body.push(format!("skinparam defaultFontName {}", font.trim()));
    }
    let mut text = format!("{PLANTUML_MARK}{} (Omaspec)\n", pack.name());
    for line in body {
        text.push_str(&line);
        text.push('\n');
    }
    text.push_str(PLANTUML_END);
    text.push('\n');
    text
}

/// The Omarchy pack for a JSON or a YAML diagram, by the name its styles
/// go by (`jsonDiagram`, `yamlDiagram`). PlantUML draws one from its
/// styles alone, so the colors go into a `<style>` block.
fn tables_omarchy(diagram: &str, c: &Colors, font: Option<&str>) -> String {
    // A font name is read in `root` only.
    let font = font.map_or_else(String::new, |f| {
        format!("root {{\n  FontName {}\n}}\n", f.trim())
    });
    format!(
        "{PLANTUML_MARK}{} (Omaspec)\n\
         <style>\n\
         {font}document {{\n  BackGroundColor {bg}\n  FontColor {text}\n}}\n\
         {diagram} {{\n  \
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

/// The Omarchy pack for an `@startnwdiag` diagram, which PlantUML draws
/// from its styles alone (`nwdiagDiagram`).
fn network_omarchy(c: &Colors, font: Option<&str>) -> String {
    // A font name is read in `root` only.
    let font = font.map_or_else(String::new, |f| {
        format!("root {{\n  FontName {}\n}}\n", f.trim())
    });
    format!(
        "{PLANTUML_MARK}{} (Omaspec)\n\
         <style>\n\
         {font}document {{\n  BackGroundColor {bg}\n  FontColor {text}\n}}\n\
         nwdiagDiagram {{\n  \
           network {{\n    BackGroundColor {surface}\n    LineColor {accent}\n    FontColor {text}\n  }}\n  \
           server {{\n    BackGroundColor {surface}\n    LineColor {accent}\n    FontColor {text}\n  }}\n  \
           arrow {{\n    LineColor {accent}\n    FontColor {text}\n  }}\n  \
           group {{\n    BackGroundColor {bg}\n    LineColor {muted}\n    FontColor {text}\n  }}\n\
         }}\n\
         </style>\n\
         {PLANTUML_END}\n",
        Pack::Omarchy.name(),
        bg = c.background,
        text = c.text,
        surface = c.surface,
        accent = c.accent,
        muted = c.muted,
    )
}

fn mermaid(pack: Pack, c: &Colors, font: Option<&str>) -> String {
    let mut init = match pack {
        Pack::Plain => format!("{{{}}}", mermaid_painted(&PLAIN)),
        Pack::Spec => {
            r#"{"theme": "neutral", "themeVariables": {"fontFamily": "serif"}}"#.to_owned()
        }
        Pack::Presentation => format!(
            r#"{{{}, "themeVariables": {{"fontSize": "18px", {}}}}}"#,
            r#""theme": "base""#,
            mermaid_painted(&PRESENTATION)
                .trim_start_matches(r#""theme": "base", "themeVariables": {"#)
                .trim_end_matches('}')
        ),
        Pack::Sketch => format!(r#"{{"look": "handDrawn", {}}}"#, mermaid_painted(&SKETCH)),
        Pack::Vivid => format!("{{{}}}", mermaid_painted(&VIVID)),
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
    if let Some(font) = font {
        let font = format!("\"fontFamily\": \"{}\"", font.trim());
        init = match init.find("\"fontFamily\": \"") {
            Some(at) => {
                let value = at + "\"fontFamily\": \"".len();
                let end = value + init[value..].find('"').unwrap_or(0) + 1;
                format!("{}{font}{}", &init[..at], &init[end..])
            }
            None if init.contains("\"themeVariables\": {") => init.replacen(
                "\"themeVariables\": {",
                &format!("\"themeVariables\": {{{font}, "),
                1,
            ),
            None => format!(
                "{}, \"themeVariables\": {{{font}}}}}",
                init.trim_end_matches('}')
            ),
        };
    }
    format!(
        "{MERMAID_MARK}{} (Omaspec)\n%%{{init: {init}}}%%\n",
        pack.name()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_is_written_in_a_font() {
        let font = Some("IBM Plex Sans");
        let (_, puml) = edit_in(
            "@startuml\nA -> B\n@enduml\n",
            Language::PlantUml,
            Some(Pack::Plain),
            &COLORS,
            font,
        )
        .unwrap();
        assert!(
            puml.contains("skinparam defaultFontName IBM Plex Sans\n"),
            "{puml}"
        );
        assert!(!puml.contains("defaultFontName sans-serif"), "{puml}");
        let (_, spec) =
            edit_in("pie\n", Language::Mermaid, Some(Pack::Spec), &COLORS, font).unwrap();
        assert!(
            spec.contains(r#""fontFamily": "IBM Plex Sans""#) && !spec.contains("serif\""),
            "{spec}"
        );
        let (_, colors) = edit_in(
            "pie\n",
            Language::Mermaid,
            Some(Pack::Omarchy),
            &COLORS,
            font,
        )
        .unwrap();
        assert!(
            colors.contains(r#""themeVariables": {"fontFamily": "IBM Plex Sans", "background""#),
            "{colors}"
        );
        let (_, sketch) = edit_in(
            "pie\n",
            Language::Mermaid,
            Some(Pack::Sketch),
            &COLORS,
            font,
        )
        .unwrap();
        assert!(
            sketch.contains(r##"{"look": "handDrawn", "theme": "base", "themeVariables": {"fontFamily": "IBM Plex Sans", "background": "#FFFDF6""##),
            "{sketch}"
        );
        // A name that would break out of its line or its string is left out.
        let (_, plain) = edit_in(
            "pie\n",
            Language::Mermaid,
            Some(Pack::Plain),
            &COLORS,
            Some("x\", \"y"),
        )
        .unwrap();
        assert!(!plain.contains("fontFamily"), "{plain}");
        assert!(is_font("Inter") && !is_font("") && !is_font("a{b}"));
    }

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
        let yaml = apply(
            "@startyaml\na: 1\n@endyaml\n",
            Language::PlantUml,
            Some(Pack::Omarchy),
        );
        assert!(yaml.contains("\nyamlDiagram {\n") && !yaml.contains("skinparam"));
    }

    #[test]
    fn the_sketch_the_vivid_and_the_presentation_pack_carry_colors() {
        let code = "@startuml\n@enduml\n";
        let plain = apply(code, Language::PlantUml, Some(Pack::Plain));
        assert!(plain.contains("skinparam monochrome true\n"));
        assert!(plain.contains("skinparam StateBackgroundColor #FFFFFF\n"));
        assert!(plain.contains("skinparam NoteBorderColor #000000\n"));
        let sketch = apply(code, Language::PlantUml, Some(Pack::Sketch));
        assert!(sketch.contains("skinparam handwritten true\n"));
        assert!(
            sketch.contains("skinparam backgroundColor #FFFDF6\n"),
            "{sketch}"
        );
        assert!(sketch.contains("skinparam StateBackgroundColor #D0EBFF\n"));
        assert!(sketch.contains("skinparam NoteBackgroundColor #FFF3BF\n"));
        let vivid = apply(code, Language::PlantUml, Some(Pack::Vivid));
        assert!(vivid.contains("skinparam ClassBorderColor #2563EB\n"));
        assert!(vivid.contains("skinparam InterfaceBackgroundColor #EDE9FE\n"));
        assert!(vivid.contains("skinparam DatabaseBackgroundColor #DCFCE7\n"));
        assert!(!vivid.contains("handwritten"));
        let slides = apply(code, Language::PlantUml, Some(Pack::Presentation));
        assert!(slides.contains("skinparam roundCorner 12\n"));
        assert!(slides.contains("skinparam StateBackgroundColor #E8F0FE\n"));
        // Mermaid: the same colors as theme variables, the sketch keeps its look.
        let sketch = apply("pie\n", Language::Mermaid, Some(Pack::Sketch));
        assert!(sketch.contains(r##"{"look": "handDrawn", "theme": "base", "themeVariables": {"background": "#FFFDF6""##), "{sketch}");
        let vivid = apply("pie\n", Language::Mermaid, Some(Pack::Vivid));
        assert!(
            vivid.contains(r##""primaryBorderColor": "#2563EB""##)
                && vivid.ends_with("}}%%\npie\n"),
            "{vivid}"
        );
        let slides = apply("pie\n", Language::Mermaid, Some(Pack::Presentation));
        assert!(
            slides.contains(r##""fontSize": "18px", "background": "#FFFFFF""##)
                && slides.contains(r##""lineColor": "#3367D6""##),
            "{slides}"
        );
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
