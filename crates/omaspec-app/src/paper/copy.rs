//! The diagram on screen, with its source inside, copied to the
//! clipboard or written to a file.

use std::path::PathBuf;
use std::sync::Arc;

use iced::Task;
use omaspec_render::export::{self, Format};

use super::Paper;
use crate::worker::blocking;

impl Paper {
    /// Copies the diagram, with its source inside, to the clipboard; says
    /// what it copied.
    pub fn copy(&self, format: Format, file: &str) -> Task<Result<&'static str, String>> {
        // Pasted elsewhere, a link to a file beside it leads nowhere.
        self.exported(format, file, None, move |bytes| {
            crate::files::copy_to_clipboard(&bytes, format.mime()).map_err(|e| format!("{e:#}"))?;
            Ok(match format {
                Format::Png => "PNG",
                Format::Svg => "SVG",
                Format::Jpeg => "JPEG",
                Format::Pdf => "PDF",
                Format::Markdown => "Markdown",
                Format::Zip => "ZIP",
            })
        })
    }

    /// Writes the diagram, with its source inside, to `out`; says where.
    pub fn export_to(
        &self,
        format: Format,
        file: &str,
        own: Option<String>,
        out: PathBuf,
    ) -> Task<Result<String, String>> {
        self.exported(format, file, own, move |bytes| {
            crate::files::write_atomic(&out, &bytes).map_err(|e| format!("{e:#}"))?;
            Ok(out.display().to_string())
        })
    }

    /// Exports the diagram on screen in the background and hands the bytes
    /// to `then`. `file` is the source's file name, stored with it.
    fn exported<T: Send + 'static>(
        &self,
        format: Format,
        file: &str,
        own: Option<String>,
        then: impl FnOnce(Vec<u8>) -> Result<T, String> + Send + 'static,
    ) -> Task<Result<T, String>> {
        let Some(diagram) = &self.diagram else {
            return Task::done(Err("no diagram yet".into()));
        };
        let svg = Arc::clone(&diagram.drawn.svg);
        let links = Arc::clone(&diagram.drawn.links);
        let source = Arc::clone(&diagram.source);
        let document = self.document;
        let language = diagram.language;
        let file = file.to_owned();
        Task::perform(
            blocking(move || {
                // A document's diagram without the empty lines around it.
                let source = if document {
                    source.trim_matches('\n')
                } else {
                    &source
                };
                // Each tile or state that opens a diagram links to its export.
                let up = own.as_deref().map_or(0, crate::specs::depth);
                let links = own.map_or_else(Vec::new, |own| {
                    crate::specs::export_links(&links, &own, false)
                });
                let options = export::Options {
                    scale: 2.0,
                    source: Some(export::Source {
                        code: source,
                        language,
                        file: Some(&file),
                    }),
                    links: &links,
                    up,
                };
                let bytes = export::export(&svg, format, &options).map_err(|e| e.to_string())?;
                then(bytes)
            }),
            |result| result.unwrap_or_else(|| Err("export failed".into())),
        )
    }
}
