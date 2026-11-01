//! A project: a folder of diagrams with `omaspec-project.toml` in its root
//! (see `docs/adr/0017-projects.md`). The file marks the root, so a diagram
//! anywhere below it finds its project; it names the project and the
//! diagram that describes the whole system.
//!
//! The file is a small part of TOML: comments, and `name` and `main` as
//! strings. Anything else is an error that names its line, so a typo does
//! not go unnoticed.

use std::path::{Component, Path};

use crate::Language;

/// The file that makes a folder a project.
pub const FILE: &str = "omaspec-project.toml";

/// What `omaspec-project.toml` says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Project {
    /// What the project is called; the folder's name when not set.
    pub name: Option<String>,
    /// The diagram that describes the whole system, relative to the root.
    pub main: Option<String>,
}

/// A line of `omaspec-project.toml` that does not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// 1-based.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{FILE}:{}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

impl Project {
    /// Reads the text of `omaspec-project.toml`.
    pub fn parse(text: &str) -> Result<Project, Error> {
        let mut project = Project::default();
        for (index, line) in text.lines().enumerate() {
            let line_no = index + 1;
            let fail = |message: String| Error {
                line: line_no,
                message,
            };
            let line = line.trim_start_matches('\u{feff}').trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                return Err(fail(format!(
                    "`{line}`: a project file has no tables, only `name` and `main`"
                )));
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(fail(format!("`{line}` is no `key = \"value\"`")));
            };
            let key = key.trim();
            let value = string(value.trim()).map_err(fail)?;
            let slot = match key {
                "name" => &mut project.name,
                "main" => &mut project.main,
                _ => {
                    return Err(fail(format!(
                        "unknown key `{key}`; a project file knows `name` and `main`"
                    )));
                }
            };
            if slot.is_some() {
                return Err(fail(format!("`{key}` is set twice")));
            }
            if value.trim().is_empty() {
                return Err(fail(format!("`{key}` is empty")));
            }
            if key == "main" && !inside(&value) {
                return Err(fail(format!(
                    "`main = \"{value}\"` leaves the project; name a diagram in this folder or below it"
                )));
            }
            *slot = Some(value);
        }
        Ok(project)
    }

    /// The text of `omaspec-project.toml` for this project.
    pub fn to_toml(&self) -> String {
        let mut out =
            String::from("# An Omaspec project: the diagrams in this folder and below it.\n");
        if let Some(name) = &self.name {
            out.push_str(&format!("name = {}\n", quoted(name)));
        }
        if let Some(main) = &self.main {
            out.push_str(&format!("main = {}\n", quoted(main)));
        }
        out
    }
}

/// The file name of a new project's first diagram.
pub fn main_file(language: Language) -> String {
    format!("system.{}", language.extension())
}

/// A new project's first diagram: the whole system as a small state
/// machine, titled with the project's name, which the writer replaces.
pub fn starter(language: Language, name: &str) -> String {
    match language {
        Language::PlantUml => format!(
            "@startuml\ntitle {}\n[*] --> Idle\nIdle --> Busy : start\nBusy --> Idle : done\n@enduml\n",
            name.trim()
        ),
        Language::Mermaid => format!(
            "---\ntitle: {}\n---\nstateDiagram-v2\n[*] --> Idle\nIdle --> Busy : start\nBusy --> Idle : done\n",
            yaml_scalar(name.trim())
        ),
    }
}

/// A relative path that stays inside the folder it starts in.
fn inside(path: &str) -> bool {
    let path = Path::new(path);
    path.components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// A TOML string: basic (`"…"`, with escapes) or literal (`'…'`), and
/// nothing after it but a comment.
fn string(value: &str) -> Result<String, String> {
    let mut chars = value.char_indices();
    let quote = match chars.next() {
        Some((_, q @ ('"' | '\''))) => q,
        _ => return Err(format!("`{value}` is no string; write it in double quotes")),
    };
    let mut out = String::new();
    let mut end = None;
    while let Some((at, c)) = chars.next() {
        match c {
            c if c == quote => {
                end = Some(at + c.len_utf8());
                break;
            }
            '\\' if quote == '"' => {
                let escaped = match chars.next().map(|(_, e)| e) {
                    Some('"') => '"',
                    Some('\\') => '\\',
                    Some('n') => '\n',
                    Some('t') => '\t',
                    Some('r') => '\r',
                    Some('b') => '\u{8}',
                    Some('f') => '\u{c}',
                    Some(u @ ('u' | 'U')) => {
                        let len = if u == 'u' { 4 } else { 8 };
                        let hex: String = chars.by_ref().take(len).map(|(_, h)| h).collect();
                        u32::from_str_radix(&hex, 16)
                            .ok()
                            .filter(|_| hex.len() == len)
                            .and_then(char::from_u32)
                            .ok_or_else(|| format!("`\\{u}{hex}` is no character"))?
                    }
                    Some(other) => return Err(format!("`\\{other}` is no escape in TOML")),
                    None => break,
                };
                out.push(escaped);
            }
            c => out.push(c),
        }
    }
    let end = end.ok_or_else(|| format!("`{value}` has no closing {quote}"))?;
    let rest = value[end..].trim();
    if !rest.is_empty() && !rest.starts_with('#') {
        return Err(format!("`{rest}` after the string"));
    }
    Ok(out)
}

/// `text` as a TOML basic string.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `text` as a YAML value on one line: plain where it can be, else in
/// double quotes.
fn yaml_scalar(text: &str) -> String {
    let plain = text
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        && text.starts_with(|c: char| c.is_alphanumeric());
    if plain {
        return text.to_owned();
    }
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_name_and_main_with_comments() {
        let text = "# a project\n\nname = \"Shop\"  # its name\nmain = 'states/system.puml'\n";
        let project = Project::parse(text).unwrap();
        assert_eq!(project.name.as_deref(), Some("Shop"));
        assert_eq!(project.main.as_deref(), Some("states/system.puml"));
        assert_eq!(Project::parse("").unwrap(), Project::default());
    }

    #[test]
    fn writes_what_it_reads() {
        for name in ["Shop", "say \"hi\"", "back\\slash", "tab\there", "Café ☕"] {
            let project = Project {
                name: Some(name.into()),
                main: Some("system.mmd".into()),
            };
            assert_eq!(
                Project::parse(&project.to_toml()).unwrap(),
                project,
                "{name}"
            );
        }
    }

    #[test]
    fn escapes_read_as_toml_says() {
        let project = Project::parse("name = \"a\\u00e9\\U0001F600\\\"b\"").unwrap();
        assert_eq!(project.name.as_deref(), Some("aé😀\"b"));
    }

    #[test]
    fn says_which_line_does_not_hold() {
        let cases = [
            ("name = \"A\"\ncolor = \"red\"", 2, "unknown key `color`"),
            ("name = \"A\"\nname = \"B\"", 2, "set twice"),
            ("[project]\nname = \"A\"", 1, "no tables"),
            ("name = A", 1, "no string"),
            ("name = \"A", 1, "no closing"),
            ("name = \"A\" B", 1, "after the string"),
            ("name", 1, "is no `key"),
            ("name = \"\"", 1, "empty"),
            ("name = \"\\q\"", 1, "no escape"),
            ("main = \"../other.puml\"", 1, "leaves the project"),
            ("main = \"/etc/system.puml\"", 1, "leaves the project"),
        ];
        for (text, line, message) in cases {
            let err = Project::parse(text).unwrap_err();
            assert_eq!(err.line, line, "{text}");
            assert!(err.message.contains(message), "{text}: {}", err.message);
        }
        let err = Project::parse("x = \"1\"").unwrap_err();
        assert!(err.to_string().starts_with("omaspec-project.toml:1: "));
    }

    #[test]
    fn the_starter_is_formatted_and_clean() {
        for language in [Language::PlantUml, Language::Mermaid] {
            let text = starter(language, "Shop");
            assert_eq!(crate::format::format(&text, language), text, "{text}");
            assert!(crate::lint::lint(&text, language).is_empty(), "{text}");
        }
    }

    #[test]
    fn the_starter_is_titled_with_the_name() {
        let puml = starter(Language::PlantUml, "Shop");
        assert!(puml.starts_with("@startuml\ntitle Shop\n"), "{puml}");
        assert!(puml.ends_with("@enduml\n"));
        let mmd = starter(Language::Mermaid, "Shop");
        assert!(
            mmd.starts_with("---\ntitle: Shop\n---\nstateDiagram-v2\n"),
            "{mmd}"
        );
        let odd = starter(Language::Mermaid, "a: \"b\"");
        assert!(odd.contains("title: \"a: \\\"b\\\"\"\n"), "{odd}");
        assert_eq!(main_file(Language::Mermaid), "system.mmd");
    }
}
