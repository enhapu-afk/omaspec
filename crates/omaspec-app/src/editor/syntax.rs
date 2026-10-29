//! Highlighting and lint for the editor: the grammars, token colors from
//! the Omarchy palette, and lint findings as editor diagnostics.

use omaspec_core::Language;
use omaspec_core::lint::{self, Finding};
use scrive_core::{Diagnostic, Severity, SyntaxDef, TokenTheme};
use std::collections::HashMap;

use omaspec_theme::{Palette, Rgb, tokens};

const PLANTUML: &str = include_str!("../../assets/syntax/plantuml.sublime-syntax");
const MERMAID: &str = include_str!("../../assets/syntax/mermaid.sublime-syntax");

pub fn grammar(language: Language) -> SyntaxDef {
    let source = match language {
        Language::PlantUml => PLANTUML,
        Language::Mermaid => MERMAID,
    };
    SyntaxDef::from_sublime_syntax(source).expect("the bundled grammars are valid")
}

/// What Ctrl+/ puts in front of a line.
pub fn line_comment(language: Language) -> &'static str {
    match language {
        Language::PlantUml => "'",
        Language::Mermaid => "%%",
    }
}

/// The token colors: from the palette, with `tweaks` (`omaspec.toml`'s
/// `[syntax]`) on top.
pub fn token_theme(palette: &Palette, tweaks: &HashMap<String, Rgb>) -> TokenTheme {
    TokenTheme::from_tm_theme(&tokens::tm_theme(palette, tweaks))
        .expect("the palette makes a valid theme")
}

/// Findings as diagnostics on `text`: each underlines its line without the
/// indentation.
pub fn diagnostics(text: &str, findings: &[Finding]) -> Vec<Diagnostic> {
    let mut starts = vec![0u32];
    starts.extend(
        text.match_indices('\n')
            .map(|(i, _)| u32::try_from(i + 1).unwrap_or(u32::MAX)),
    );
    findings
        .iter()
        .filter_map(|f| {
            let start = *starts.get(f.line)?;
            let line = text[start as usize..]
                .split('\n')
                .next()
                .unwrap_or_default();
            let indent = (line.len() - line.trim_start().len()) as u32;
            let end = start + line.trim_end().len() as u32;
            let from = (start + indent).min(end);
            let severity = match f.severity {
                lint::Severity::Error => Severity::Error,
                lint::Severity::Warning => Severity::Warning,
                lint::Severity::Info => Severity::Info,
            };
            Some(Diagnostic::new(from..end, severity, f.message.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammars_and_theme_load() {
        let _ = grammar(Language::PlantUml);
        let _ = grammar(Language::Mermaid);
        let _ = token_theme(&Palette::fallback(), &HashMap::new());
    }

    #[test]
    fn findings_underline_their_line() {
        let text = "@startuml\n  alt ok\nA -> B\n";
        let findings = lint::lint(text, Language::PlantUml);
        let diags = diagnostics(text, &findings);
        assert!(!diags.is_empty());
        let alt = findings.iter().position(|f| f.line == 1).unwrap();
        assert_eq!(diags[alt].span, 12..18);
        assert_eq!(&text[12..18], "alt ok");
    }
}
