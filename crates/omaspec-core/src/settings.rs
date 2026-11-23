//! The settings a Mermaid diagram gives itself: the `config:` of its front
//! matter and its `%%{init: …}%%` directives, each setting with the line
//! it is written on. The drawers read them here, and [`unheeded`] says
//! which of them Omaspec does not carry out, so none is passed over in
//! silence.

use serde_json::{Map, Value};

use crate::lint::{Finding, Severity};

/// One setting: `themeVariables.fontFamily` as `["themeVariables",
/// "fontFamily"]`, its value, and the zero-based line it is on.
#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
    pub path: Vec<String>,
    pub value: Value,
    pub line: usize,
}

/// A value of the front matter as JSON: a number, `true`, `false` or
/// `null` as YAML writes them (`True`, `FALSE`, `~` too, as Mermaid's
/// reader takes them), or the text without its quotes.
fn scalar(text: &str) -> Value {
    let text = text.trim();
    if let Some(inner) = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .or_else(|| text.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')))
    {
        return Value::String(inner.to_owned());
    }
    match text {
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        "null" | "Null" | "NULL" | "~" => Value::Null,
        _ => text
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or_else(|| Value::String(text.to_owned()), Value::Number),
    }
}

/// `text` cut at every `mark` that stands outside its quotes.
fn outside_quotes<'a>(text: &'a str, mark: &'a str) -> impl Iterator<Item = &'a str> {
    let mut cuts = vec![0];
    let mut quote = None;
    for (at, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if text[at..].starts_with(mark) => cuts.push(at),
            None => {}
        }
    }
    cuts.push(text.len());
    let mut first = true;
    cuts.windows(2)
        .map(move |pair| {
            let from = if std::mem::take(&mut first) {
                pair[0]
            } else {
                pair[0] + mark.len()
            };
            &text[from.min(pair[1])..pair[1]]
        })
        .collect::<Vec<_>>()
        .into_iter()
}

/// The settings under `config:` in the front matter, the YAML between
/// `---` lines before the diagram, in the order written.
pub fn front(code: &str) -> Vec<Setting> {
    let mut out = Vec::new();
    let mut lines = code
        .split('\n')
        .enumerate()
        .skip_while(|(_, line)| line.trim().is_empty());
    if lines.next().is_none_or(|(_, line)| line.trim() != "---") {
        return out;
    }
    // How far `config:` is indented, and the maps open below it: each
    // with the indentation of its key.
    let mut config: Option<usize> = None;
    let mut open: Vec<(usize, String)> = Vec::new();
    for (no, raw) in lines {
        let text = raw.trim();
        if text == "---" {
            break;
        }
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let Some(at) = config.filter(|at| indent > *at) else {
            config = (text == "config:").then_some(indent);
            open.clear();
            continue;
        };
        let _ = at;
        open.retain(|(depth, _)| *depth < indent);
        let Some((key, value)) = text.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches(['"', '\'']).to_owned();
        // A comment after the value; a `#` inside quotes or a color stays.
        let value = outside_quotes(value, " #")
            .next()
            .unwrap_or_default()
            .trim();
        if value.is_empty() {
            open.push((indent, key));
            continue;
        }
        let mut path: Vec<String> = open.iter().map(|(_, key)| key.clone()).collect();
        path.push(key);
        // `{ a: 1, b: x }` on one line holds settings of its own.
        match value
            .strip_prefix('{')
            .and_then(|inner| inner.strip_suffix('}'))
        {
            Some(inner) => {
                for pair in outside_quotes(inner, ",") {
                    if let Some((key, value)) = pair.split_once(':') {
                        let mut path = path.clone();
                        path.push(key.trim().trim_matches(['"', '\'']).to_owned());
                        out.push(Setting {
                            path,
                            value: scalar(value),
                            line: no,
                        });
                    }
                }
            }
            None => out.push(Setting {
                path,
                value: scalar(value),
                line: no,
            }),
        }
    }
    out
}

/// Every value of `value` that holds no settings of its own, with the
/// path to it.
fn leaves(value: &Value, path: &mut Vec<String>, visit: &mut impl FnMut(&[String], &Value)) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                path.push(key.clone());
                leaves(inner, path, visit);
                path.pop();
            }
        }
        other => visit(path, other),
    }
}

/// A directive runs over at most this many lines.
const MAX_LINES: usize = 500;

/// Values nested deeper than this are not read.
const MAX_NESTING: usize = 100;

/// How deep the brackets of `text` nest, outside its quotes.
fn depth(text: &str) -> usize {
    let (mut now, mut most) = (0usize, 0usize);
    let mut quote = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '{' | '[') => {
                now += 1;
                most = most.max(now);
            }
            (None, '}' | ']') => now = now.saturating_sub(1),
            _ => {}
        }
    }
    most
}

/// The settings of the `%%{init: …}%%` directives, in the order written;
/// a directive may run over several lines. One that is no JSON holds
/// none: the drawer names that error.
pub fn init(code: &str) -> Vec<Setting> {
    let lines: Vec<&str> = code.split('\n').collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let start = at;
        at += 1;
        let Some(rest) = lines[start].trim().strip_prefix("%%{") else {
            continue;
        };
        // Each line is looked at once: a directive still being typed,
        // with no end, costs no more than its lines.
        let mut text = rest.to_owned();
        let mut end = start;
        let mut closed = rest.contains("}%%");
        while !closed && end + 1 < lines.len() && end - start < MAX_LINES {
            let line = lines[end + 1].trim();
            // The next directive begins: this one has no end.
            if line.starts_with("%%{") {
                break;
            }
            end += 1;
            closed = line.contains("}%%");
            text.push(' ');
            text.push_str(line);
        }
        if !closed {
            continue;
        }
        let Some(inside) = text.split("}%%").next() else {
            continue;
        };
        at = end + 1;
        let Some((name, settings)) = inside.split_once(':') else {
            continue;
        };
        if !matches!(name.trim(), "init" | "initialize") {
            continue;
        }
        // A parser goes into a value by recursion: one nested without
        // end would overflow the stack. The drawer refuses it by name.
        if depth(settings) > MAX_NESTING {
            continue;
        }
        let Ok(value) =
            serde_json::from_str::<Value>(settings).or_else(|_| json5::from_str::<Value>(settings))
        else {
            continue;
        };
        leaves(&value, &mut Vec::new(), &mut |path, value| {
            // The line its key is written on.
            let line = path
                .last()
                .and_then(|key| (start..=end).find(|&no| lines[no].contains(key.as_str())))
                .unwrap_or(start);
            out.push(Setting {
                path: path.to_vec(),
                value: value.clone(),
                line,
            });
        });
    }
    out
}

/// Whether a Mermaid sequence draws its participants again below their
/// lifelines: as Mermaid does, unless `sequence.mirrorActors` is set to
/// what JavaScript takes as false (`false`, `0`, `""`), in the front
/// matter's `config` or a `%%{init}%%`, the last such setting counting.
/// Mermaid writes no `null`, list or object over its own `true`, so these
/// leave it as it was. PlantUML's `hide footbox` is the same.
pub fn mirrors_actors(code: &str) -> bool {
    switch(code, "sequence", "mirrorActors", true)
}

/// A switch among the settings, `section.key`, as Mermaid tests it: what
/// JavaScript takes as false (`false`, `0`, `""`) is off, the last such
/// setting counting. A `null`, list or object leaves it as it was; Mermaid
/// writes none of these over its own default.
fn switch(code: &str, section: &str, key: &str, default: bool) -> bool {
    front(code)
        .into_iter()
        .chain(init(code))
        .filter(|setting| setting.path == [section, key])
        .fold(default, |on, setting| match setting.value {
            Value::Bool(value) => value,
            Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
            Value::String(text) => !text.is_empty(),
            Value::Null | Value::Array(_) | Value::Object(_) => on,
        })
}

/// The front matter's settings and then the directives', which count
/// over them, as one value: what a drawer reads. `None` without any.
pub fn merged(code: &str) -> Option<Value> {
    let mut root = Map::new();
    let mut any = false;
    for setting in front(code).into_iter().chain(init(code)) {
        let Some((last, parents)) = setting.path.split_last() else {
            continue;
        };
        let mut map = &mut root;
        for key in parents {
            let slot = map
                .entry(key.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if !slot.is_object() {
                *slot = Value::Object(Map::new());
            }
            let Value::Object(inner) = slot else {
                unreachable!("made an object above");
            };
            map = inner;
        }
        map.insert(last.clone(), setting.value);
        any = true;
    }
    any.then_some(Value::Object(root))
}

/// What a git graph's `gitGraph` settings ask for, from its front matter
/// and its directives; Omaspec's defaults for what they leave out. Its
/// switches are tested as Mermaid tests them ([`switch`]); a name that is
/// no text, or empty, and an order that is no number are passed over.
pub fn git_graph(code: &str) -> omaspec_uml::gitgraph::Settings {
    let defaults = omaspec_uml::gitgraph::Settings::default();
    let merged = merged(code);
    let git = merged.as_ref().and_then(|root| root.get("gitGraph"));
    let name = git
        .and_then(|git| git.get("mainBranchName"))
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty());
    let order = git
        .and_then(|git| git.get("mainBranchOrder"))
        .and_then(Value::as_f64);
    omaspec_uml::gitgraph::Settings {
        main_name: name.map_or(defaults.main_name, str::to_owned),
        main_order: order.map_or(defaults.main_order, |order| order as f32),
        show_branches: switch(code, "gitGraph", "showBranches", defaults.show_branches),
        show_commit_label: switch(
            code,
            "gitGraph",
            "showCommitLabel",
            defaults.show_commit_label,
        ),
        rotate_commit_label: switch(
            code,
            "gitGraph",
            "rotateCommitLabel",
            defaults.rotate_commit_label,
        ),
    }
}

/// The colors of `themeVariables` the drawers take.
const COLORS: [&str; 23] = [
    "primaryColor",
    "primaryTextColor",
    "primaryBorderColor",
    "lineColor",
    "secondaryColor",
    "tertiaryColor",
    "textColor",
    "background",
    "edgeLabelBackground",
    "clusterBkg",
    "clusterBorder",
    "actorBkg",
    "actorBorder",
    "actorLine",
    "noteBkg",
    "noteBorderColor",
    "activationBkgColor",
    "activationBorderColor",
    "pieTitleTextColor",
    "pieSectionTextColor",
    "pieLegendTextColor",
    "pieStrokeColor",
    "pieOuterStrokeColor",
];

/// The other `themeVariables` the drawers take, past `pie1`..`pie12`.
const VARIABLES: [&str; 8] = [
    "fontFamily",
    "fontSize",
    "pieTitleTextSize",
    "pieSectionTextSize",
    "pieLegendTextSize",
    "pieStrokeWidth",
    "pieOuterStrokeWidth",
    "pieOpacity",
];

/// Settings that change no picture: how Mermaid loads, logs and guards
/// itself in a page.
const NO_PICTURE: [&str; 11] = [
    "securityLevel",
    "startOnLoad",
    "logLevel",
    "maxTextSize",
    "maxEdges",
    "deterministicIds",
    "deterministicIDSeed",
    "secure",
    "suppressErrorRendering",
    "legacyMathML",
    "forceLegacyMathML",
];

/// The themes by name.
const THEMES: [&str; 7] = [
    "default", "base", "dark", "forest", "neutral", "mermaid", "modern",
];

/// Whether Omaspec carries the setting at `path` out, or it changes no
/// picture. The drawers' own list is `apply_init` in `omaspec-render`:
/// a setting added there is added here. `flowchart.nodeSpacing` and
/// `rankSpacing` reach mmdr's layout alone, which draws a flowchart only
/// where Omaspec's own drawer declines it, so they are not (REQ-110).
fn heeded(path: &[String]) -> bool {
    let path: Vec<&str> = path.iter().map(String::as_str).collect();
    match path.as_slice() {
        ["theme" | "look" | "fontFamily" | "fontSize"] => true,
        [one] => NO_PICTURE.contains(one),
        ["themeVariables", name] => {
            COLORS.contains(name)
                || VARIABLES.contains(name)
                || name
                    .strip_prefix("pie")
                    .and_then(|n| n.parse::<u8>().ok())
                    .is_some_and(|n| (1..=12).contains(&n))
        }
        ["flowchart", "curve"] => true,
        ["sequence", "mirrorActors"] => true,
        ["gitGraph", name] => omaspec_uml::gitgraph::SETTINGS.contains(name),
        _ => false,
    }
}

/// Whether a setting of a git graph's own section is carried out: by its
/// drawer, `git_drawn`, and with a value of the kind it takes (a name
/// that is text, an order that is a number). Any other is as [`heeded`]
/// says.
fn carried(setting: &Setting, git_drawn: bool) -> bool {
    match setting.path.as_slice() {
        [section, key] if section == "gitGraph" => {
            git_drawn
                && match key.as_str() {
                    "mainBranchName" => setting.value.as_str().is_some_and(|name| !name.is_empty()),
                    "mainBranchOrder" => setting.value.is_number(),
                    _ => true,
                }
        }
        _ => true,
    }
}

/// A finding for every setting of a Mermaid diagram that Omaspec does not
/// carry out, on its line, and for a theme no drawer knows.
pub fn unheeded(code: &str) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    // A skin pack's flowchart takes the borders of its diamonds and its
    // stores from two variables more.
    let packed = crate::skin::current(code).is_some();
    // A git graph's own settings are carried out by its drawer alone:
    // where it leaves the graph to mmdr (a construct it refuses, another
    // setting of the front matter), they are not.
    let (body, _) = crate::lang::mermaid_body(code);
    let git_drawn = omaspec_uml::gitgraph::mermaid_refusal(body, &git_graph(body)).is_none();
    for setting in front(code).into_iter().chain(init(code)) {
        let name = setting.path.join(".");
        let of_pack = packed
            && matches!(
                name.as_str(),
                "themeVariables.secondaryBorderColor" | "themeVariables.tertiaryBorderColor"
            );
        let message = if of_pack {
            continue;
        } else if heeded(&setting.path) && carried(&setting, git_drawn) {
            match (name.as_str(), setting.value.as_str()) {
                ("theme", Some(theme))
                    if !THEMES.contains(&theme.trim().to_ascii_lowercase().as_str()) =>
                {
                    format!(
                        "`{theme}` is no theme Omaspec knows (default, base, dark, forest, neutral): the diagram keeps the default one"
                    )
                }
                _ => continue,
            }
        } else {
            format!("`{name}` is not carried out: Omaspec draws the diagram without this setting")
        };
        if !out
            .iter()
            .any(|f| f.line == setting.line && f.message == message)
        {
            out.push(Finding::new(
                "unheeded-setting",
                setting.line,
                // The picture is not the one the file asks for: `--strict`
                // fails on it (REQ-098).
                Severity::Warning,
                message,
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(settings: &[Setting]) -> Vec<(String, usize)> {
        settings
            .iter()
            .map(|s| (s.path.join("."), s.line))
            .collect()
    }

    #[test]
    fn participants_stand_below_unless_the_setting_is_false_in_javascript() {
        let body = "sequenceDiagram\n  A->>B: hi\n";
        assert!(mirrors_actors(body));
        for (value, on) in [
            ("false", false),
            ("0", false),
            ("\"\"", false),
            ("null", true),
            ("true", true),
            ("1", true),
            ("\"no\"", true),
        ] {
            let code =
                format!("%%{{init: {{\"sequence\": {{\"mirrorActors\": {value}}}}}}}%%\n{body}");
            assert_eq!(mirrors_actors(&code), on, "{code}");
        }
        for value in ["0", "False"] {
            let front =
                format!("---\nconfig:\n  sequence:\n    mirrorActors: {value}\n---\n{body}");
            assert!(!mirrors_actors(&front), "{front}");
        }
        // A null leaves what was set before as it was.
        for value in ["null", "~"] {
            let front =
                format!("---\nconfig:\n  sequence:\n    mirrorActors: {value}\n---\n{body}");
            assert!(mirrors_actors(&front), "{front}");
        }
        let both = format!(
            "---\nconfig:\n  sequence:\n    mirrorActors: false\n---\n%%{{init: {{\"sequence\": {{\"mirrorActors\": null}}}}}}%%\n{body}"
        );
        assert!(!mirrors_actors(&both), "{both}");
    }

    /// A git graph's settings from the front matter and a directive over
    /// it, its switches tested as Mermaid tests them; what they set is
    /// carried out, so lint says nothing of it, and of the rest it does.
    #[test]
    fn a_git_graph_s_settings_are_read_and_carried_out() {
        let code = "---\nconfig:\n  gitGraph:\n    mainBranchName: trunk\n    mainBranchOrder: 2\n    showBranches: 0\n---\n%%{init: {\"gitGraph\": {\"rotateCommitLabel\": true, \"showCommitLabel\": \"\", \"parallelCommits\": true}}}%%\ngitGraph\n  commit\n";
        let settings = git_graph(code);
        assert_eq!(settings.main_name, "trunk");
        assert_eq!(settings.main_order, 2.0);
        assert!(!settings.show_branches && !settings.show_commit_label);
        assert!(settings.rotate_commit_label);
        let said: Vec<String> = unheeded(code).into_iter().map(|f| f.message).collect();
        assert_eq!(
            said,
            [
                "`gitGraph.parallelCommits` is not carried out: Omaspec draws the diagram without this setting"
            ]
        );
        // Where mmdr draws the graph, none is carried out: with a setting
        // of the front matter the drawer does not read, or with a front
        // matter in flow style.
        let front = code.replace(
            "    showBranches: 0\n",
            "    showBranches: 0\n    parallelCommits: true\n",
        );
        assert_eq!(unheeded(&front).len(), 7, "{:?}", unheeded(&front));
        let flow = "---\nconfig:\n  gitGraph: { mainBranchName: trunk }\n---\ngitGraph\n  commit\n";
        let said: Vec<String> = unheeded(flow).into_iter().map(|f| f.message).collect();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].starts_with("`gitGraph.mainBranchName` is not carried out"));
        // Nor a name that is no text, nor an order that is no number.
        let numbers = "%%{init: {\"gitGraph\": {\"mainBranchName\": 2024, \"mainBranchOrder\": \"x\"}}}%%\ngitGraph\n  commit\n";
        assert_eq!(unheeded(numbers).len(), 2, "{:?}", unheeded(numbers));
        let plain = git_graph("gitGraph\n  commit\n");
        assert_eq!(plain, omaspec_uml::gitgraph::Settings::default());
        let empty = git_graph(
            "%%{init: {\"gitGraph\": {\"mainBranchName\": \"\", \"mainBranchOrder\": \"x\"}}}%%\ngitGraph\n",
        );
        assert_eq!((empty.main_name.as_str(), empty.main_order), ("main", 0.0));
        // A name as Mermaid takes it, spaces and all.
        let spaced =
            git_graph("%%{init: {\"gitGraph\": {\"mainBranchName\": \" trunk\"}}}%%\ngitGraph\n");
        assert_eq!(spaced.main_name, " trunk");
    }

    #[test]
    fn the_front_matter_and_the_directive_are_read_with_their_lines() {
        let code = "---\ntitle: Shop\nconfig:\n  theme: forest\n  themeVariables:\n    primaryColor: \"#ff0000\" # red\n    fontSize: 18\n  flowchart: { curve: linear }\n---\n%%{init: {\"themeCSS\": \".node { fill: red }\",\n  \"themeVariables\": {\"fontSize\": \"20px\"}}}%%\nflowchart TD\n  A --> B\n";
        assert_eq!(
            paths(&front(code)),
            [
                ("theme".to_owned(), 3),
                ("themeVariables.primaryColor".to_owned(), 5),
                ("themeVariables.fontSize".to_owned(), 6),
                ("flowchart.curve".to_owned(), 7),
            ]
        );
        assert_eq!(front(code)[1].value, Value::String("#ff0000".into()));
        assert_eq!(
            paths(&init(code)),
            [
                ("themeCSS".to_owned(), 9),
                ("themeVariables.fontSize".to_owned(), 10)
            ]
        );
        // The directive counts over the front matter.
        let merged = merged(code).unwrap();
        assert_eq!(merged["theme"], "forest");
        assert_eq!(merged["themeVariables"]["fontSize"], "20px");
        assert_eq!(merged["themeVariables"]["primaryColor"], "#ff0000");
        assert_eq!(merged["flowchart"]["curve"], "linear");
        assert_eq!(super::merged("flowchart TD\n  A --> B\n"), None);
        // A `#` and a comma inside quotes belong to the value.
        let quoted = "---\nconfig:\n  fontFamily: \"Segoe UI #2\" # the second\n  flowchart: { curve: \"a, b\", nodeSpacing: 9 }\n---\nflowchart TD\n";
        let read = front(quoted);
        assert_eq!(read[0].value, Value::String("Segoe UI #2".into()));
        assert_eq!(read[1].value, Value::String("a, b".into()));
        assert_eq!(read[2].path.join("."), "flowchart.nodeSpacing");
        // A directive with no end holds none, and the next one is read.
        let open = "%%{init: {\n%%{init: {\"theme\": \"dark\"}}%%\nflowchart TD\n";
        assert_eq!(paths(&init(open)), [("theme".to_owned(), 1)]);
    }

    #[test]
    fn a_setting_that_is_not_carried_out_is_told_on_its_line() {
        let code = "%%{init: {\"theme\": \"dark\", \"fontFamily\": \"courier\", \"themeCSS\": \".node { fill: red }\", \"securityLevel\": \"loose\", \"sequence\": {\"actorMargin\": 80}}}%%\nflowchart TD\n  A --> B\n";
        let found = unheeded(code);
        let said: Vec<&str> = found.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(
            said,
            [
                "`sequence.actorMargin` is not carried out: Omaspec draws the diagram without this setting",
                "`themeCSS` is not carried out: Omaspec draws the diagram without this setting",
            ]
        );
        assert!(
            found
                .iter()
                .all(|f| f.line == 0 && f.severity == Severity::Warning)
        );
        let front = "---\nconfig:\n  theme: neon\n  look: handDrawn\n  layout: elk\n---\nflowchart TD\n  A --> B\n";
        let found = unheeded(front);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].message.starts_with("`neon` is no theme") && found[0].line == 2);
        assert!(found[1].message.starts_with("`layout`") && found[1].line == 4);
        assert!(unheeded("flowchart TD\n  A --> B\n").is_empty());
        // A flowchart's spacing reaches mmdr's layout alone, which Omaspec's
        // drawer does not take (REQ-110); its curve it takes.
        let spaced = "%%{init: {\"flowchart\": {\"nodeSpacing\": 150, \"rankSpacing\": 150, \"curve\": \"basis\"}}}%%\nflowchart TD\n  A --> B\n";
        let said: Vec<String> = unheeded(spaced).into_iter().map(|f| f.message).collect();
        assert_eq!(
            said,
            [
                "`flowchart.nodeSpacing` is not carried out: Omaspec draws the diagram without this setting",
                "`flowchart.rankSpacing` is not carried out: Omaspec draws the diagram without this setting",
            ]
        );
        // One nested without end is left to the drawer, which refuses it.
        let deep = format!(
            "%%{{init: {}1{}}}%%\nflowchart TD\n",
            "[".repeat(5000),
            "]".repeat(5000)
        );
        assert!(init(&deep).is_empty());
    }
}
