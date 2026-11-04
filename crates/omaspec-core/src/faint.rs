//! Pairs of colors the writer set that hardly differ: a text too close to
//! the fill it lies on, a line or a text too close to the page. Read from
//! PlantUML's `skinparam` lines and blocks and from Mermaid's `classDef`
//! and `style` lines; colors the engines choose themselves, and those of
//! the Omarchy theme laid over a diagram, are not looked at.

use omaspec_uml::{Color, parse_color};

/// The least contrast two colors the writer set need, as WCAG asks of
/// lines and shapes; text asks 4.5:1, so under 3:1 even a large one fades.
pub const LEAST: f32 = 3.0;

/// A pair that hardly differs, at the line of the later of the two.
#[derive(Debug, Clone, PartialEq)]
pub struct Faint {
    pub line: usize,
    pub message: String,
}

/// What a color is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Fill,
    Text,
    Line,
}

/// A color set by a `skinparam`.
struct Set {
    /// The element, lowercase: `state`, `arrow`; empty for the page and
    /// `default` for every element.
    element: String,
    /// A stereotype it is limited to, as written.
    stereotype: String,
    role: Role,
    color: Color,
    line: usize,
    written: String,
}

/// The faint pairs of a PlantUML diagram's `skinparam` colors.
pub fn plantuml(lines: &[&str]) -> Vec<Faint> {
    let sets = skinparams(lines);
    let find = |element: &str, stereotype: &str, role: Role| {
        sets.iter()
            .rev()
            .find(|s| s.element == element && s.stereotype == stereotype && s.role == role)
    };
    let page = find("", "", Role::Fill);
    let mut out = Vec::new();
    for set in &sets {
        match set.role {
            // A text lies on its element's fill; the default text and an
            // arrow's lie on the page too.
            Role::Text => {
                let on = match set.element.as_str() {
                    "" | "default" | "arrow" => page,
                    element => find(element, &set.stereotype, Role::Fill),
                };
                out.extend(on.and_then(|on| pair(set, on)));
            }
            Role::Line => out.extend(page.and_then(|page| pair(set, page))),
            // An element's fill without a text color of its own holds the
            // default text.
            Role::Fill
                if !set.element.is_empty()
                    && find(&set.element, &set.stereotype, Role::Text).is_none() =>
            {
                out.extend(find("default", "", Role::Text).and_then(|text| pair(text, set)));
            }
            Role::Fill => {}
        }
    }
    out.sort_by_key(|f| f.line);
    out.dedup();
    out
}

/// `a` on `b`, when they hardly differ.
fn pair(a: &Set, b: &Set) -> Option<Faint> {
    let ratio = a.color.contrast(b.color);
    (ratio < LEAST).then(|| Faint {
        line: a.line.max(b.line),
        message: faint(&a.written, &b.written, ratio),
    })
}

fn faint(a: &str, b: &str, ratio: f32) -> String {
    format!(
        "`{a}` on `{b}` reads at {ratio:.1}:1; under {LEAST:.0}:1 it hardly shows on screen or paper"
    )
}

/// Every color a `skinparam` line or block sets, in order.
fn skinparams(lines: &[&str]) -> Vec<Set> {
    let mut out = Vec::new();
    let mut block: Option<String> = None;
    for (no, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line.starts_with('\'') || line.is_empty() {
            continue;
        }
        if let Some(prefix) = &block {
            if line == "}" {
                block = None;
                continue;
            }
            if let Some((key, value)) = split(line) {
                let key = format!("{prefix}{key}");
                out.extend(set(&key, value, no, format!("skinparam {key} {value}")));
            }
            continue;
        }
        let Some(rest) = strip_word(line, "skinparam") else {
            continue;
        };
        if let Some(prefix) = rest.strip_suffix('{') {
            block = Some(prefix.trim().to_owned());
            continue;
        }
        if let Some((key, value)) = split(rest) {
            out.extend(set(key, value, no, format!("skinparam {key} {value}")));
        }
    }
    out
}

/// `line` without its first word, when that is `word` in any case.
fn strip_word<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    let head = line.get(..word.len())?;
    let rest = &line[word.len()..];
    (head.eq_ignore_ascii_case(word) && rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

/// A `key value` pair.
fn split(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(char::is_whitespace)?;
    Some((key, value.trim()))
}

/// The color `key` sets to `value`, if it sets one Omaspec pairs.
fn set(key: &str, value: &str, line: usize, written: String) -> Option<Set> {
    let (key, stereotype) = match key.split_once("<<") {
        Some((key, rest)) => (key, rest.trim_end_matches(">>").trim().to_owned()),
        None => (key, String::new()),
    };
    let key: String = key
        .chars()
        .filter(|c| !matches!(c, '_' | '.') && !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let (element, role) = [
        ("backgroundcolor", Role::Fill),
        ("fontcolor", Role::Text),
        ("bordercolor", Role::Line),
        ("linecolor", Role::Line),
    ]
    .into_iter()
    .find_map(|(suffix, role)| Some((key.strip_suffix(suffix)?, role)))
    .or_else(|| (key == "arrowcolor").then_some(("arrow", Role::Line)))?;
    let color = parse_color(value).ok()??;
    Some(Set {
        element: element.to_owned(),
        stereotype,
        role,
        color,
        line,
        written,
    })
}

/// The faint pairs of a Mermaid diagram's `classDef` and `style` lines: a
/// text color too close to the fill.
pub fn mermaid(lines: &[&str]) -> Vec<Faint> {
    let mut out = Vec::new();
    for (no, raw) in lines.iter().enumerate() {
        let line = raw.trim().trim_end_matches(';');
        let Some(rest) = strip_word(line, "classDef").or_else(|| strip_word(line, "style")) else {
            continue;
        };
        let Some((_, props)) = rest.split_once(char::is_whitespace) else {
            continue;
        };
        let prop = |name: &str| {
            props.split(',').find_map(|p| {
                let (key, value) = p.split_once(':')?;
                (key.trim() == name).then(|| value.trim())
            })
        };
        let (Some(fill), Some(text)) = (prop("fill"), prop("color")) else {
            continue;
        };
        let (Ok(Some(f)), Ok(Some(t))) = (parse_color(fill), parse_color(text)) else {
            continue;
        };
        let ratio = t.contrast(f);
        if ratio < LEAST {
            out.push(Faint {
                line: no,
                message: faint(&format!("color:{text}"), &format!("fill:{fill}"), ratio),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(code: &str) -> Vec<&str> {
        code.lines().collect()
    }

    #[test]
    fn a_text_close_to_its_fill_is_named_where_the_second_color_is_set() {
        let code = "@startuml\nskinparam stateBackgroundColor #EEEEEE\nskinparam stateFontColor #DDDDDD\n[*] --> Idle\n@enduml\n";
        let found = plantuml(&lines(code));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].line, 2);
        assert!(
            found[0]
                .message
                .starts_with("`skinparam stateFontColor #DDDDDD` on `skinparam stateBackgroundColor #EEEEEE` reads at 1.2:1"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn blocks_stereotypes_the_default_text_and_the_page_count_too() {
        let code = "\
@startuml
skinparam backgroundColor black
skinparam class {
  BackgroundColor<<Faint>> LightGray
  FontColor<<Faint>> Silver
  BackgroundColor White
}
skinparam defaultFontColor #222222
skinparam ArrowColor #111111
@enduml
";
        let found = plantuml(&lines(code));
        let at: Vec<usize> = found.iter().map(|f| f.line).collect();
        // Silver on light gray; the default text on the black page; the
        // arrow on it too. The default text on the white class reads.
        assert_eq!(at, [4, 7, 8], "{found:?}");
    }

    #[test]
    fn colors_that_differ_enough_or_are_not_colors_are_left_alone() {
        let code = "@startuml\nskinparam stateBackgroundColor White\nskinparam stateFontColor #767676\nskinparam stateBorderColor #FF0000/#00FF00\n@enduml\n";
        assert!(plantuml(&lines(code)).is_empty());
    }

    #[test]
    fn a_mermaid_class_or_style_with_a_text_close_to_its_fill() {
        let code = "flowchart LR\n  A --> B\n  classDef soft fill:#f9f9f9,stroke:#333,color:#eeeeee;\n  style B fill:#000,color:#fff\n";
        let found = mermaid(&lines(code));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].line, 2);
        assert!(
            found[0]
                .message
                .starts_with("`color:#eeeeee` on `fill:#f9f9f9`")
        );
    }
}
