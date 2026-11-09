//! A project: a folder of diagrams with `omaspec-project.toml` in its root
//! (see `docs/adr/0017-projects.md`). The file marks the root, so a diagram
//! anywhere below it finds its project; it names the project. The
//! project's map is made from its diagrams and written nowhere
//! (`docs/adr/0026-the-map-is-made.md`).
//!
//! The file is a small part of TOML: comments, `name` as a string, and a
//! `[look]` table ([`Look`]) of strings. Anything else is an error that
//! names its line, so a typo does not go unnoticed.
//!
//! The diagrams of a project find each other by name ([`names`], [`key`]):
//! a participant its state machine, a state the diagram it opens
//! (`docs/adr/0018-diagrams-that-link.md`).

use crate::Language;

/// The file that makes a folder a project.
pub const FILE: &str = "omaspec-project.toml";

/// What `omaspec-project.toml` says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Project {
    /// What the project is called; the folder's name when not set.
    pub name: Option<String>,
    /// `[look]`: what every diagram of the project is drawn in.
    pub look: Option<Look>,
}

/// `[look]`: what every diagram of the project is drawn in, the same in
/// the pane, `render`, the MCP server and CI, on every machine. The file
/// stays as written; the look is laid over it as `render --look` lays one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Look {
    pub pack: LookPack,
    /// Colors that replace the palette's, by name ([`COLOR_NAMES`]), as
    /// `#rrggbb`; with `light` and `dark` only.
    pub colors: Vec<(String, String)>,
    /// The font every diagram is written in ([`crate::skin::is_font`]).
    pub font: Option<String>,
}

/// The pack a project's look lays over its diagrams.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LookPack {
    /// Black lines, no fills.
    Plain,
    /// Black and white, strict UML, a serif font.
    Spec,
    /// Larger type and round corners.
    Presentation,
    /// Omaspec's light palette; the pack when only colors are set.
    #[default]
    Light,
    /// Omaspec's dark palette.
    Dark,
    /// The light palette, and the dark one under a dark scheme (SVG only).
    Auto,
    /// Drawn by hand, in pastel colors on warm paper.
    Sketch,
    /// A palette by kind on white.
    Vivid,
}

impl LookPack {
    pub const ALL: [LookPack; 8] = [
        LookPack::Plain,
        LookPack::Spec,
        LookPack::Presentation,
        LookPack::Light,
        LookPack::Dark,
        LookPack::Auto,
        LookPack::Sketch,
        LookPack::Vivid,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LookPack::Plain => "plain",
            LookPack::Spec => "spec",
            LookPack::Presentation => "presentation",
            LookPack::Light => "light",
            LookPack::Dark => "dark",
            LookPack::Auto => "auto",
            LookPack::Sketch => "sketch",
            LookPack::Vivid => "vivid",
        }
    }
}

/// The colors a look may set, as the color packs name them.
pub const COLOR_NAMES: [&str; 6] = ["background", "surface", "text", "accent", "muted", "note"];

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
        // The line of `[look]`, once it began, and of its `pack`.
        let mut look_at: Option<usize> = None;
        let mut pack_at = 0;
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
                let table = line.split('#').next().unwrap_or(line).trim();
                if table != "[look]" {
                    return Err(fail(format!(
                        "`{line}`: a project file has one table, `[look]`"
                    )));
                }
                if look_at.is_some() {
                    return Err(fail("`[look]` is set twice".into()));
                }
                look_at = Some(line_no);
                project.look = Some(Look::default());
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(fail(format!("`{line}` is no `key = \"value\"`")));
            };
            let key = key.trim();
            let value = string(value.trim()).map_err(fail)?;
            if let Some(look) = &mut project.look {
                if key == "pack" {
                    pack_at = line_no;
                }
                look.set(key, &value).map_err(fail)?;
                continue;
            }
            let slot = match key {
                "name" => &mut project.name,
                // What named the map a writer kept, before maps were made.
                "main" => {
                    return Err(fail(
                        "`main` is no key any more: the project's map is made from its \
                         diagrams, so this line and the map it names can go"
                            .to_owned(),
                    ));
                }
                _ => {
                    return Err(fail(format!(
                        "unknown key `{key}`; a project file knows `name`"
                    )));
                }
            };
            if slot.is_some() {
                return Err(fail(format!("`{key}` is set twice")));
            }
            if value.trim().is_empty() {
                return Err(fail(format!("`{key}` is empty")));
            }
            *slot = Some(value);
        }
        if let Some(look) = &project.look
            && !look.colors.is_empty()
        {
            let why = match look.pack {
                LookPack::Plain | LookPack::Spec => "draws in black and white",
                LookPack::Presentation | LookPack::Sketch | LookPack::Vivid => {
                    "has a palette of its own"
                }
                LookPack::Auto => "takes Omaspec's two palettes",
                LookPack::Light | LookPack::Dark => "",
            };
            if !why.is_empty() {
                return Err(Error {
                    line: pack_at.max(look_at.unwrap_or(1)),
                    message: format!(
                        "`pack = \"{}\"` {why}; colors go with `light` or `dark`",
                        look.pack.name()
                    ),
                });
            }
        }
        Ok(project)
    }

    /// The text of `omaspec-project.toml` for this project.
    pub fn to_toml(&self) -> String {
        let mut out = self.top_toml();
        if let Some(look) = &self.look {
            out.push_str(&format!("\n[look]\npack = {}\n", quoted(look.pack.name())));
            for (name, color) in &look.colors {
                out.push_str(&format!("{name} = {}\n", quoted(color)));
            }
            if let Some(font) = &look.font {
                out.push_str(&format!("font = {}\n", quoted(font)));
            }
        }
        out
    }

    fn top_toml(&self) -> String {
        let mut out =
            String::from("# An Omaspec project: the diagrams in this folder and below it.\n");
        if let Some(name) = &self.name {
            out.push_str(&format!("name = {}\n", quoted(name)));
        }
        out
    }
}

impl Look {
    /// Sets `key` of `[look]` to `value`, or says why it does not hold.
    fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "pack" => {
                if value == "omarchy" {
                    return Err("`omarchy` follows each machine's theme; a project pins a look that is the same everywhere: light, dark or auto".into());
                }
                self.pack = LookPack::ALL
                    .into_iter()
                    .find(|p| p.name() == value)
                    .ok_or_else(|| {
                        format!(
                            "no pack `{value}`; take plain, spec, presentation, light, dark or auto"
                        )
                    })?;
            }
            "font" => {
                if self.font.is_some() {
                    return Err("`font` is set twice".into());
                }
                if !crate::skin::is_font(value) {
                    return Err(format!(
                        "`{value}` is no font name: letters, digits, spaces, `-`, `_` and `.`"
                    ));
                }
                self.font = Some(value.trim().to_owned());
            }
            name if COLOR_NAMES.contains(&name) => {
                if self.colors.iter().any(|(n, _)| n == name) {
                    return Err(format!("`{name}` is set twice"));
                }
                if !is_hex_color(value) {
                    return Err(format!(
                        "`{name} = \"{value}\"`: a color is written `#rrggbb`"
                    ));
                }
                self.colors
                    .push((name.to_owned(), value.to_ascii_lowercase()));
            }
            "name" => {
                return Err(format!("`{key}` belongs above `[look]`"));
            }
            _ => {
                return Err(format!(
                    "unknown key `{key}` in `[look]`; it knows pack, font and the colors {}",
                    COLOR_NAMES.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// The color `name` sets, if it sets one.
    pub fn color(&self, name: &str) -> Option<&str> {
        self.colors
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c.as_str())
    }
}

/// `#rgb` or `#rrggbb`.
fn is_hex_color(value: &str) -> bool {
    value
        .strip_prefix('#')
        .is_some_and(|hex| matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The file name of a new project's first state machine: the project's
/// name.
pub fn machine_file(language: Language, name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let file = format!("{stem}.{}", language.extension());
    if stem.is_empty() || stem.starts_with('.') {
        format!("machine.{}", language.extension())
    } else {
        file
    }
}

/// A new project's first state machine, titled with the project's name,
/// which the writer replaces.
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

/// The names a diagram goes by, as written: the name after `@start…`
/// (PlantUML), its title (`title` in PlantUML, `title:` in a Mermaid
/// front matter) and the file name without its extension. Names without
/// a letter or a digit are left out.
pub fn names(code: &str, language: Language, file: &str) -> Vec<String> {
    let mut names = Vec::new();
    names.extend(start_name(code, language));
    names.extend(title(code, language));
    let stem = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = stem.split('.').next().unwrap_or(stem);
    names.push(stem.to_owned());
    names.retain(|n| !key(n).is_empty());
    names
}

/// How names compare: letters and digits only, in lower case, so that
/// `Door control`, `door_control` and `DoorControl` are one name.
pub fn key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// `@startuml Name`, `@startwbs Name`, …: the name a PlantUML diagram
/// gives itself.
pub fn start_name(code: &str, language: Language) -> Option<String> {
    if language != Language::PlantUml {
        return None;
    }
    let line = code
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}').trim())
        .find(|l| l.starts_with("@start"))?;
    let rest = line["@start".len()..].trim_start_matches(|c: char| c.is_ascii_alphanumeric());
    let name = rest.trim();
    (!name.is_empty() && !name.starts_with('(')).then(|| name.trim_matches('"').to_owned())
}

/// `title Name` in PlantUML, `title: Name` in a Mermaid front matter.
pub fn title(code: &str, language: Language) -> Option<String> {
    let mut front = false;
    for line in code.lines().map(str::trim) {
        match language {
            Language::PlantUml => {
                let lower = line.to_ascii_lowercase();
                if let Some(rest) = lower.strip_prefix("title ") {
                    return Some(line[line.len() - rest.len()..].trim().to_owned());
                }
            }
            Language::Mermaid => {
                if line == "---" {
                    if front {
                        return None;
                    }
                    front = true;
                } else if front && let Some(rest) = line.strip_prefix("title:") {
                    return Some(rest.trim().trim_matches('"').to_owned());
                }
            }
        }
    }
    None
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
    fn reads_its_name_with_comments() {
        let text = "# a project\n\nname = \"Shop\"  # its name\n";
        let project = Project::parse(text).unwrap();
        assert_eq!(project.name.as_deref(), Some("Shop"));
        assert_eq!(Project::parse("").unwrap(), Project::default());
        // The map is made, so no line names one; one that still does is
        // told what to do with it.
        let err = Project::parse("name = \"Shop\"\nmain = \"system.puml\"\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("is made from its diagrams"), "{err}");
    }

    #[test]
    fn writes_what_it_reads() {
        for name in ["Shop", "say \"hi\"", "back\\slash", "tab\there", "Café ☕"] {
            let project = Project {
                name: Some(name.into()),
                look: None,
            };
            assert_eq!(
                Project::parse(&project.to_toml()).unwrap(),
                project,
                "{name}"
            );
        }
    }

    #[test]
    fn a_look_pins_a_pack_its_colors_and_a_font() {
        let text = "name = \"Shop\"\n\n[look]  # the same everywhere\npack = \"dark\"\naccent = \"#D73A49\"\nfont = \"Inter\"\n";
        let project = Project::parse(text).unwrap();
        let look = project.look.clone().unwrap();
        assert_eq!(look.pack, LookPack::Dark);
        assert_eq!(look.color("accent"), Some("#d73a49"));
        assert_eq!(look.font.as_deref(), Some("Inter"));
        assert_eq!(Project::parse(&project.to_toml()).unwrap(), project);
        // Colors alone are the light palette's, changed.
        let colors = Project::parse("[look]\nbackground = \"#fff\"").unwrap();
        assert_eq!(colors.look.unwrap().pack, LookPack::Light);
        assert_eq!(
            Project::parse("[look]").unwrap().look,
            Some(Look::default())
        );
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
            ("[project]\nname = \"A\"", 1, "one table, `[look]`"),
            ("[look]\n[look]", 2, "set twice"),
            ("[look]\nname = \"A\"", 2, "belongs above `[look]`"),
            ("[look]\npack = \"omarchy\"", 2, "each machine's theme"),
            ("[look]\npack = \"sepia\"", 2, "no pack `sepia`"),
            ("[look]\naccent = \"red\"", 2, "written `#rrggbb`"),
            ("[look]\nfont = \"a{b}\"", 2, "no font name"),
            (
                "[look]\nshadow = \"none\"",
                2,
                "unknown key `shadow` in `[look]`",
            ),
            (
                "[look]\npack = \"plain\"\naccent = \"#ff0000\"",
                2,
                "black and white",
            ),
            (
                "[look]\naccent = \"#ff0000\"\npack = \"auto\"",
                3,
                "two palettes",
            ),
            ("name = A", 1, "no string"),
            ("name = \"A", 1, "no closing"),
            ("name = \"A\" B", 1, "after the string"),
            ("name", 1, "is no `key"),
            ("name = \"\"", 1, "empty"),
            ("name = \"\\q\"", 1, "no escape"),
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
            assert_eq!(crate::lang::detect(&text, None), language, "{text}");
        }
    }

    #[test]
    fn the_first_machine_is_called_as_the_project() {
        for language in [Language::PlantUml, Language::Mermaid] {
            for name in ["Shop", "my shop (2)", "system"] {
                let machine = machine_file(language, name);
                let titled = names(&starter(language, name), language, &machine);
                assert!(
                    titled.iter().any(|n| key(n) == key(name)),
                    "{name}: {titled:?}"
                );
            }
        }
        assert_eq!(machine_file(Language::PlantUml, "Shop"), "Shop.puml");
        // A name is free now that no map is written beside the machine.
        assert_eq!(machine_file(Language::Mermaid, "System"), "System.mmd");
        assert_eq!(machine_file(Language::PlantUml, "a/b"), "a-b.puml");
        assert_eq!(machine_file(Language::PlantUml, " "), "machine.puml");
    }

    #[test]
    fn a_diagram_goes_by_its_start_name_title_and_file() {
        let puml = "@startuml Door control\ntitle The door\n[*] --> A\n@enduml";
        assert_eq!(
            names(puml, Language::PlantUml, "parts/door.puml"),
            ["Door control", "The door", "door"]
        );
        let wbs = "@startwbs Map\n* shop\n@endwbs";
        assert_eq!(
            names(wbs, Language::PlantUml, "system.puml"),
            ["Map", "system"]
        );
        let mmd = "---\ntitle: \"Checkout\"\n---\nflowchart LR\nA --> B";
        assert_eq!(
            names(mmd, Language::Mermaid, "flow.mmd"),
            ["Checkout", "flow"]
        );
        assert_eq!(key("Door control"), key("door_control"));
        assert_eq!(key("Café"), "café");
        assert!(names("@startuml\n@enduml", Language::PlantUml, "---.puml").is_empty());
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
    }
}
