//! `omaspec orbit FILE -o orbit.svg`: a state machine's life cycle on a
//! ring ([`omaspec_uml::state::draw_orbit`]), in the look of its states;
//! the SVG on standard output without `-o`.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use omaspec_model::Machine;

use crate::commands::language;
use crate::files::read_input;

pub fn run(file: &Path, output: Option<&Path>) -> Result<ExitCode> {
    use omaspec_render::export::{self, Format};
    let input = read_input(Some(file))?;
    let lang = language(&input, None);
    if Machine::read(&input.text, lang).is_none() {
        bail!(
            "{} is no state machine Omaspec reads, so it has no life cycle to draw on a ring",
            file.display()
        );
    }
    let place = input
        .path
        .as_deref()
        .and_then(|path| omaspec_render::Place::of(path, None));
    let rendered = omaspec_render::render_orbit(&input.text, lang, place.as_ref())
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let Some(output) = output else {
        std::io::stdout().write_all(rendered.svg.as_bytes())?;
        return Ok(ExitCode::SUCCESS);
    };
    let format = output
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(Format::from_extension)
        .filter(|format| matches!(format, Format::Svg | Format::Png | Format::Pdf))
        .with_context(|| format!("{} is no .svg, .png or .pdf", output.display()))?;
    let bytes = export::export(&rendered.svg, format, &export::Options::default())?;
    fs::write(output, bytes).with_context(|| format!("writing {}", output.display()))?;
    Ok(ExitCode::SUCCESS)
}
