//! Turns diagram source into SVG and exports it.
//!
//! [`Renderer`] routes a diagram to an engine: Mermaid to mmdr, PlantUML to
//! the native engine (`omagram-uml`) and, where that declines, to the
//! Java PlantUML if it is installed. Every SVG an engine returns passes
//! [`sanitize`] before anyone sees it. [`RenderService`] runs a renderer on
//! its own thread and drops requests that a newer one has overtaken.

pub mod export;
mod include;
mod java;
mod mermaid;
pub mod pick;
pub mod sanitize;
mod service;

use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use omagram_core::Language;
use omagram_core::kind;
pub use omagram_uml::map::Map;

pub use include::Place;
pub use java::JavaPlantUml;
pub use mermaid::{MermaidDeclined, coverage as mermaid_coverage};
pub use service::{Generation, Outcome, RenderService};

/// The engine that drew (or failed to draw) a diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Mermaid,
    NativePlantUml,
    JavaPlantUml,
}

impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Engine::Mermaid => "mmdr",
            Engine::NativePlantUml => "Omagram PlantUML",
            Engine::JavaPlantUml => "Java PlantUML",
        }
    }
}

/// A sanitized SVG and how it was made.
#[derive(Debug, Clone)]
pub struct Rendered {
    pub svg: String,
    pub engine: Engine,
    pub elapsed: Duration,
    /// Where the states, transitions and messages are in the picture, as
    /// far as Omagram drew them; empty for the fallbacks.
    pub map: Map,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The diagram has an error the engine names.
    Diagram {
        engine: Engine,
        /// Zero-based line in the source, if the engine says.
        line: Option<usize>,
        message: String,
    },
    /// No installed engine can draw this diagram.
    Unavailable {
        /// Zero-based line of what Omagram's own engine does not draw,
        /// when one line is to blame.
        line: Option<usize>,
        reason: String,
    },
    /// The engine itself failed: crashed, timed out, or produced no valid SVG.
    Engine { engine: Engine, message: String },
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Diagram {
                engine,
                line: Some(line),
                message,
            } => write!(f, "line {}: {message} ({})", line + 1, engine.name()),
            RenderError::Diagram {
                engine, message, ..
            } => {
                write!(f, "{message} ({})", engine.name())
            }
            RenderError::Unavailable {
                line: Some(line),
                reason,
            } => write!(f, "line {}: {reason}", line + 1),
            RenderError::Unavailable { reason, .. } => f.write_str(reason),
            RenderError::Engine { engine, message } => write!(f, "{}: {message}", engine.name()),
        }
    }
}

impl std::error::Error for RenderError {}

/// Diagrams Omagram does not draw at all, by their `@start…` word. A
/// declined line of a kind it does draw (timing, Gantt, mind map, WBS,
/// sequence, state, structure, signal flow, JSON, formulas) keeps that line.
/// Signal flow is Omagram's own syntax, so a declined line is not sent to Java.
const WHOLE: &[&str] = &["chronology", "yaml", "salt", "ditaa", "dot", "tree"];

/// For a PlantUML diagram Omagram's own engine does not draw at all,
/// the error that says so. Salt, ditaa, YAML, chronology, Graphviz and
/// tree diagrams are that kind: a line would only point at the first line
/// of a different language. A timing, Gantt, JSON or formula diagram that
/// declines keeps that line.
fn not_native(source: &str) -> Option<RenderError> {
    if !WHOLE.contains(&kind::plantuml_block(source).as_str()) {
        return None;
    }
    let found = kind::classify(source, Language::PlantUml);
    Some(RenderError::Unavailable {
        line: None,
        reason: format!(
            "{}s are not drawn by Omagram's own engine yet; install the `plantuml` \
                 package to draw them with the Java PlantUML",
            found.name
        ),
    })
}

#[derive(Debug, Clone)]
pub struct Config {
    /// The `plantuml` command for the Java fallback; searched in `PATH` if
    /// `None`. The fallback is off when it cannot be found.
    pub plantuml: Option<PathBuf>,
    pub java_fallback: bool,
    /// How long one diagram may take in the Java PlantUML.
    pub java_timeout: Duration,
    /// The same for the first diagram, which includes starting the JVM.
    pub java_start_timeout: Duration,
    /// A program that runs a command confined (Omagram's own
    /// `omagram __sandbox SCRATCH -- COMMAND …`); the Java PlantUML is
    /// started through it. `None` starts it directly.
    pub sandbox: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            plantuml: None,
            java_fallback: true,
            java_timeout: Duration::from_secs(10),
            java_start_timeout: Duration::from_secs(30),
            sandbox: None,
        }
    }
}

/// Renders diagrams with the engines a [`Config`] allows. Keeps the Java
/// PlantUML process running between diagrams.
pub struct Renderer {
    java: Option<JavaPlantUml>,
}

impl Renderer {
    pub fn new(config: &Config) -> Self {
        let java = config
            .java_fallback
            .then(|| JavaPlantUml::find(config))
            .flatten();
        Self { java }
    }

    /// Whether PlantUML diagrams the native engine declines can still be drawn.
    pub fn has_java_fallback(&self) -> bool {
        self.java.is_some()
    }

    /// Renders a diagram that lives nowhere: `!include` declines.
    pub fn render(&mut self, source: &str, language: Language) -> Result<Rendered, RenderError> {
        self.render_at(source, language, None)
    }

    /// Renders a diagram that lives at `place`, which its `!include`s
    /// start from and cannot leave.
    pub fn render_at(
        &mut self,
        source: &str,
        language: Language,
        place: Option<&Place>,
    ) -> Result<Rendered, RenderError> {
        let start = Instant::now();
        let (svg, engine, map) = match language {
            Language::Mermaid => {
                let (svg, map) = mermaid::render_mapped(source)?;
                (svg, Engine::Mermaid, map)
            }
            Language::PlantUml => self.plantuml(source, place)?,
        };
        let svg = sanitize::sanitize(&svg).map_err(|e| RenderError::Engine {
            engine,
            message: format!("invalid SVG: {e}"),
        })?;
        Ok(Rendered {
            svg,
            engine,
            elapsed: start.elapsed(),
            map,
        })
    }

    fn plantuml(
        &mut self,
        source: &str,
        place: Option<&Place>,
    ) -> Result<(String, Engine, Map), RenderError> {
        let folder = place.map(include::Folder::new);
        let includes = folder.as_ref().map(|f| f as &dyn omagram_uml::Includes);
        let declined = match omagram_uml::render_mapped(source, includes) {
            Ok((svg, map)) => return Ok((svg, Engine::NativePlantUml, map)),
            Err(declined) => declined,
        };
        // PlantUML has no signal-flow diagram. A line this engine declines
        // stays declined; Java would only say it does not know `@startflow`.
        if kind::plantuml_block(source) == "flow" {
            return Err(RenderError::Unavailable {
                line: declined.line,
                reason: declined.reason,
            });
        }
        match &mut self.java {
            Some(java) => Ok((java.render(source)?, Engine::JavaPlantUml, Map::default())),
            None => Err(not_native(source).unwrap_or(RenderError::Unavailable {
                line: declined.line,
                reason: format!(
                    "{}; install the `plantuml` package to draw it with the Java PlantUML",
                    declined.reason
                ),
            })),
        }
    }
}
