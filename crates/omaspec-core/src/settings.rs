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

/// A value of the front matter as JSON: a number, `true`, `false`, or the
/// text without its quotes.
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
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
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
/// a setting added there is added here.
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
        ["flowchart", "nodeSpacing" | "rankSpacing" | "curve"] => true,
        _ => false,
    }
}

/// A finding for every setting of a Mermaid diagram that Omaspec does not
/// carry out, on its line, and for a theme no drawer knows.
pub fn unheeded(code: &str) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    // A skin pack's flowchart takes the borders of its diamonds and its
    // stores from two variables more.
    let packed = crate::skin::current(code).is_some();
    for setting in front(code).into_iter().chain(init(code)) {
        let name = setting.path.join(".");
        let of_pack = packed
            && matches!(
                name.as_str(),
                "themeVariables.secondaryBorderColor" | "themeVariables.tertiaryBorderColor"
            );
        let message = if of_pack {
            continue;
        } else if heeded(&setting.path) {
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
            out.push(Finding {
                line: setting.line,
                severity: Severity::Info,
                message,
            });
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
        let code = "%%{init: {\"theme\": \"dark\", \"fontFamily\": \"courier\", \"themeCSS\": \".node { fill: red }\", \"securityLevel\": \"loose\", \"sequence\": {\"mirrorActors\": false}}}%%\nflowchart TD\n  A --> B\n";
        let found = unheeded(code);
        let said: Vec<&str> = found.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(
            said,
            [
                "`sequence.mirrorActors` is not carried out: Omaspec draws the diagram without this setting",
                "`themeCSS` is not carried out: Omaspec draws the diagram without this setting",
            ]
        );
        assert!(
            found
                .iter()
                .all(|f| f.line == 0 && f.severity == Severity::Info)
        );
        let front = "---\nconfig:\n  theme: neon\n  look: handDrawn\n  layout: elk\n---\nflowchart TD\n  A --> B\n";
        let found = unheeded(front);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].message.starts_with("`neon` is no theme") && found[0].line == 2);
        assert!(found[1].message.starts_with("`layout`") && found[1].line == 4);
        assert!(unheeded("flowchart TD\n  A --> B\n").is_empty());
        // One nested without end is left to the drawer, which refuses it.
        let deep = format!(
            "%%{{init: {}1{}}}%%\nflowchart TD\n",
            "[".repeat(5000),
            "]".repeat(5000)
        );
        assert!(init(&deep).is_empty());
    }
}
