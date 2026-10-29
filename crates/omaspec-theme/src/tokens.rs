//! Syntax colors for the editor, derived from the palette.
//!
//! The editor highlights with TextMate scopes, so the palette is written out
//! as a `.tmTheme`. The mapping follows what terminal themes do with the same
//! colors: keywords in the accent, strings green, comments dimmed. A color
//! too faint on the background (some themes' yellow on white) is moved
//! towards the foreground until it reads; colors set in `omaspec.toml`
//! stay as they are.

use std::collections::HashMap;

use crate::{Palette, Rgb};

/// A scope selector and how text in it looks.
struct Rule {
    name: &'static str,
    scope: &'static str,
    color: Rgb,
    italic: bool,
}

/// The least contrast a token color gets against the background (WCAG's
/// minimum for large text and graphics).
pub const MIN_CONTRAST: f32 = 3.0;

/// The `.tmTheme` (plist XML) for `palette`; `tweaks` (from
/// `omaspec.toml`'s `[syntax]`) replace colors by rule name in lower case:
/// `keyword`, `arrow`, `comment` and so on.
pub fn tm_theme(palette: &Palette, tweaks: &HashMap<String, Rgb>) -> String {
    let p = palette;
    let rules = [
        rule("Comment", "comment", p.dark_foreground, true),
        rule("Directive", "keyword.control.directive", p.magenta, false),
        rule(
            "Preprocessor",
            "keyword.other.preprocessor",
            p.magenta,
            false,
        ),
        rule("Keyword", "keyword.other", p.accent, false),
        rule("Arrow", "keyword.operator.arrow", p.cyan, false),
        rule("String", "string", p.green, false),
        rule("Message", "string.unquoted.message", p.green, false),
        rule(
            "Activity",
            "string.unquoted.activity",
            p.bright_foreground,
            false,
        ),
        rule("Color", "constant.other.color", p.orange, false),
        rule("Number", "constant.numeric", p.orange, false),
        rule("Variable", "variable", p.bright_blue, false),
        rule("Section", "entity.name.section", p.yellow, false),
        rule("Stereotype", "storage.modifier", p.blue, false),
        rule("Note", "markup.quote", p.light_foreground, true),
    ];

    let mut xml = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
        "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
        "<plist version=\"1.0\"><dict>\n",
        "<key>name</key><string>Omarchy</string>\n",
        "<key>settings</key><array>\n",
    ));
    xml.push_str(&format!(
        concat!(
            "<dict><key>settings</key><dict>",
            "<key>background</key><string>{}</string>",
            "<key>foreground</key><string>{}</string>",
            "<key>caret</key><string>{}</string>",
            "<key>selection</key><string>{}</string>",
            "<key>lineHighlight</key><string>{}</string>",
            "</dict></dict>\n"
        ),
        p.background, p.foreground, p.accent, p.selection, p.lighter_background
    ));
    for r in rules {
        xml.push_str(&format!(
            concat!(
                "<dict><key>name</key><string>{}</string>",
                "<key>scope</key><string>{}</string>",
                "<key>settings</key><dict><key>foreground</key><string>{}</string>{}</dict></dict>\n"
            ),
            r.name,
            r.scope,
            tweaks
                .get(&r.name.to_ascii_lowercase())
                .copied()
                .unwrap_or_else(|| r.color.legible_on(&[p.background], p.foreground, MIN_CONTRAST)),
            if r.italic { "<key>fontStyle</key><string>italic</string>" } else { "" }
        ));
    }
    xml.push_str("</array></dict></plist>\n");
    xml
}

fn rule(name: &'static str, scope: &'static str, color: Rgb, italic: bool) -> Rule {
    Rule {
        name,
        scope,
        color,
        italic,
    }
}

/// The token colors `tm_theme` writes for `palette` without tweaks, by rule
/// name, for checks.
pub fn token_colors(palette: &Palette) -> Vec<(&'static str, Rgb)> {
    let xml = tm_theme(palette, &HashMap::new());
    xml.split("<dict><key>name</key><string>")
        .skip(1)
        .filter_map(|rule| {
            let name = rule.split('<').next()?;
            let color = rule.split("<key>foreground</key><string>").nth(1)?;
            let name: &'static str = RULE_NAMES.iter().find(|n| **n == name)?;
            Some((name, Rgb::parse_hex(&color[..7])?))
        })
        .collect()
}

const RULE_NAMES: [&str; 14] = [
    "Comment",
    "Directive",
    "Preprocessor",
    "Keyword",
    "Arrow",
    "String",
    "Message",
    "Activity",
    "Color",
    "Number",
    "Variable",
    "Section",
    "Stereotype",
    "Note",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_palette_colors() {
        let p = Palette::fallback();
        let xml = tm_theme(&p, &HashMap::new());
        assert!(xml.contains(&format!(
            "<key>background</key><string>{}</string>",
            p.background
        )));
        assert!(xml.contains("<string>keyword.other</string>"));
        assert!(xml.ends_with("</plist>\n"));

        let tweaks = HashMap::from([("keyword".to_owned(), Rgb::new(1, 2, 3))]);
        let xml = tm_theme(&p, &tweaks);
        assert!(xml.contains("<string>keyword.other</string><key>settings</key><dict><key>foreground</key><string>#010203</string>"));
    }
}
