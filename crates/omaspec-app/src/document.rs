//! What a command reads from an input: its one diagram, or each diagram of
//! a Markdown document (`omaspec_core::markdown`), with lines that are the
//! document's. The command line and the MCP server decide here alike:
//! `lint`, `inspect`, `render` and `outline` take a document's diagrams,
//! `fmt` formats each in its place, and a command for one diagram file
//! (`convert`, `diff`) refuses a document through [`one_diagram`].

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use omaspec_core::lint::{Finding, Severity};
use omaspec_core::markdown::{self, Block};
use omaspec_core::{Language, outline};
use omaspec_model::Specs;
use serde::Serialize;

use crate::commands::{Inspection, inspection, language};
use crate::files::Input;

/// One diagram of an input.
pub(crate) struct Piece<'a> {
    pub input: &'a Input,
    pub code: Cow<'a, str>,
    pub language: Language,
    /// In a document: the diagram's number there, from 1, and its block.
    pub block: Option<(usize, Block)>,
}

/// The diagrams of `input`: a Markdown document's, each in the language
/// its fence names, or the input itself, in `forced` or the language it
/// is written in.
pub(crate) fn pieces(input: &Input, forced: Option<Language>) -> Vec<Piece<'_>> {
    if !is_document(input) {
        return vec![Piece {
            input,
            code: Cow::Borrowed(&input.text),
            language: language(input, forced),
            block: None,
        }];
    }
    markdown::blocks(&input.text)
        .into_iter()
        .enumerate()
        .map(|(index, block)| Piece {
            input,
            code: Cow::Owned(block.code.clone()),
            language: block.language,
            block: Some((index + 1, block)),
        })
        .collect()
}

/// Whether `input` is a Markdown document.
pub(crate) fn is_document(input: &Input) -> bool {
    input.file_name().is_some_and(markdown::is_markdown)
}

/// Refuses a Markdown document for a command that takes one diagram file
/// (`does`: what it does), on the command line and over MCP alike.
pub(crate) fn one_diagram(input: &Input, does: &str) -> anyhow::Result<()> {
    if is_document(input) {
        let count = markdown::blocks(&input.text).len();
        let diagrams = if count == 1 { "diagram" } else { "diagrams" };
        anyhow::bail!(
            "{} is a Markdown document with {count} {diagrams}; {does} takes a diagram file",
            input.name()
        );
    }
    Ok(())
}

/// `input` formatted: its diagram, or each diagram of a Markdown document
/// in its place, the document's prose as it is. With `portable`, a Mermaid
/// flowchart also the way mermaid.js reads it.
pub(crate) fn formatted(input: &Input, forced: Option<Language>, portable: bool) -> String {
    let format = |code: &str, language: Language| {
        let formatted = omaspec_core::format::format(code, language);
        if portable && language == Language::Mermaid {
            omaspec_core::lint::portable(&formatted)
        } else {
            formatted
        }
    };
    if is_document(input) {
        return markdown::rewrite_blocks(&input.text, |block| format(&block.code, block.language));
    }
    format(&input.text, language(input, forced))
}

/// The outline of `input`'s diagrams, on its lines.
pub(crate) fn outline(input: &Input, forced: Option<Language>) -> Vec<outline::Entry> {
    pieces(input, forced)
        .iter()
        .flat_map(|piece| match &piece.block {
            Some((_, block)) => {
                outline::outline(&markdown::masked(&input.text, block), piece.language)
            }
            None => outline::outline(&piece.code, piece.language),
        })
        .collect()
}

impl Piece<'_> {
    /// The input's line for the diagram's zero-based `line`.
    pub fn line(&self, line: usize) -> usize {
        match &self.block {
            Some((_, block)) => block.doc_line(line),
            None => line,
        }
    }

    /// Its findings, also against `specs`, on the input's lines.
    pub fn findings(&self, specs: &Specs) -> Vec<Finding> {
        let mut found = findings_of(&self.code, self.language, specs);
        for finding in &mut found {
            finding.line = self.line(finding.line);
        }
        found
    }

    /// Where `render` writes it unless told: beside the input, a
    /// document's diagrams numbered (`README-2.svg`).
    pub fn image_path(&self, extension: &str) -> Option<PathBuf> {
        let path = self.input.path.as_deref()?;
        Some(match &self.block {
            Some((number, _)) => numbered(path, *number, extension),
            None => path.with_extension(extension),
        })
    }

    /// The file name an image stores with the diagram's source, so it
    /// opens as a diagram again: a document's under its number and its
    /// language's extension (`README-2.puml`).
    pub fn source_name(&self) -> Option<String> {
        let path = self.input.path.as_deref()?;
        Some(match &self.block {
            Some((number, _)) => numbered(path, *number, self.language.extension())
                .file_name()?
                .to_string_lossy()
                .into_owned(),
            None => self.input.file_name()?.to_owned(),
        })
    }
}

/// Every finding of one diagram, `code`, in line order: the lint's, the
/// checks against `specs`, and what drawing it says about its lines and
/// boxes by hand, which the editor shows under the picture
/// ([`omaspec_render::by_hand_misses`]).
pub(crate) fn findings_of(code: &str, language: Language, specs: &Specs) -> Vec<Finding> {
    let mut found = omaspec_model::findings_with(code, language, specs);
    found.extend(
        omaspec_render::by_hand_misses(code, language)
            .into_iter()
            .map(|(line, message)| Finding {
                line,
                severity: Severity::Warning,
                message,
            }),
    );
    found.sort_by_key(|f| f.line);
    found
}

fn numbered(path: &Path, number: usize, extension: &str) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{stem}-{number}.{extension}"))
}

/// Every finding of `input`'s diagrams, on its lines, in line order.
pub(crate) fn findings(input: &Input, forced: Option<Language>, specs: &Specs) -> Vec<Finding> {
    let mut all: Vec<Finding> = pieces(input, forced)
        .iter()
        .flat_map(|piece| piece.findings(specs))
        .collect();
    all.sort_by_key(|f| f.line);
    all
}

/// The links of `input`'s diagrams, the file in the project at `root`,
/// to diagrams the project (its `atlas`) does not have, on its lines.
pub(crate) fn gone_links(
    input: &Input,
    forced: Option<Language>,
    root: &Path,
    atlas: &omaspec_render::Atlas,
) -> Vec<Finding> {
    let Some(own) = input
        .path
        .as_deref()
        .and_then(|path| crate::specs::path_in(root, path))
    else {
        return Vec::new();
    };
    let has = diagrams_of(atlas);
    pieces(input, forced)
        .iter()
        .flat_map(|piece| {
            omaspec_model::gone_links(&piece.code, piece.language, &own, &|p| has.contains(p))
                .into_iter()
                .map(|mut finding| {
                    finding.line = piece.line(finding.line);
                    finding
                })
        })
        .collect()
}

/// The paths of a project's diagrams without their extensions, as
/// [`omaspec_model::gone_links`] asks for them.
pub(crate) fn diagrams_of(atlas: &omaspec_render::Atlas) -> std::collections::HashSet<String> {
    atlas
        .diagrams
        .iter()
        .map(|d| {
            d.file
                .rsplit_once('.')
                .map_or(d.file.as_str(), |(stem, _)| stem)
                .to_owned()
        })
        .collect()
}

/// What `inspect` reports on a Markdown document: each diagram's
/// inspection, its lines the document's.
#[derive(Serialize)]
pub(crate) struct Document<'a> {
    file: String,
    diagrams: Vec<InDocument<'a>>,
}

#[derive(Serialize)]
struct InDocument<'a> {
    /// The line of its opening fence, one-based.
    line: usize,
    #[serde(flatten)]
    inspection: Inspection<'a>,
}

/// The inspection of the document `input`, given its pieces' findings.
pub(crate) fn inspect_document<'a>(
    input: &Input,
    pieces: &[Piece],
    findings: &'a [Vec<Finding>],
) -> Document<'a> {
    let file = input.name();
    let diagrams = pieces
        .iter()
        .zip(findings)
        .filter_map(|(piece, findings)| {
            let (_, block) = piece.block.as_ref()?;
            let mut inspection = inspection(&file, &piece.code, piece.language, findings);
            let entries = outline::outline(&piece.code, piece.language);
            inspection.outline_cut = crate::commands::outline_cut(&entries);
            inspection.outline = entries
                .into_iter()
                .map(|mut entry| {
                    entry.line = piece.line(entry.line) + 1;
                    entry
                })
                .collect();
            Some(InDocument {
                line: block.lines.start + 1,
                inspection,
            })
        })
        .collect();
    Document { file, diagrams }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_s_diagrams_are_numbered_beside_it() {
        let input = Input {
            path: Some(PathBuf::from("docs/README.md")),
            text:
                "# Door\n\n```plantuml\n[*] --> Closed\n```\n\n```mermaid\npie\n  \"a\": 1\n```\n"
                    .into(),
        };
        let pieces = pieces(&input, None);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].language, Language::PlantUml);
        assert_eq!(
            pieces[1].image_path("svg").unwrap(),
            Path::new("docs/README-2.svg")
        );
        assert_eq!(pieces[0].source_name().unwrap(), "README-1.puml");
        // A diagram's line 1 is the document's line 3 (the body after the
        // fence), its added `@startuml` the fence's.
        assert_eq!(pieces[0].line(1), 3);
        assert_eq!(pieces[0].line(0), 2);
        let file = Input {
            path: Some(PathBuf::from("flow.mmd")),
            text: "pie\n".into(),
        };
        let single = super::pieces(&file, None);
        assert!(single[0].block.is_none());
        assert_eq!(single[0].image_path("png").unwrap(), Path::new("flow.png"));
    }
}
