//! Turns diagram source into SVG and exports it.
//!
//! [`Renderer`] routes a diagram to an engine: Mermaid to mmdr, PlantUML to
//! the native engine (`omaspec-uml`) and, where that declines, to the
//! Java PlantUML if it is installed. Every SVG an engine returns passes
//! [`sanitize`] before anyone sees it. [`RenderService`] runs a renderer on
//! its own thread and drops requests that a newer one has overtaken.

pub mod anchors;
mod atlas;
pub mod contrast;
pub mod export;
mod fonts;
mod include;
mod java;
mod mermaid;
pub mod pick;
pub mod sanitize;
pub mod schemes;
mod service;
pub mod stack;

use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use omaspec_core::Language;
use omaspec_core::kind;
pub use omaspec_uml::map::Map;

pub use atlas::{Atlas, Entry, Link};
pub use include::{Place, project_root};
pub use java::JavaPlantUml;
pub use mermaid::{MermaidDeclined, coverage as mermaid_coverage};
pub use service::{Generation, Outcome, RenderService};

/// The engine that drew (or failed to draw) a diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// mmdr: it reads every Mermaid diagram and draws what Omaspec declines.
    Mermaid,
    /// Omaspec's own drawers for Mermaid.
    NativeMermaid,
    NativePlantUml,
    JavaPlantUml,
}

impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Engine::Mermaid => "mmdr",
            Engine::NativeMermaid => "Omaspec Mermaid",
            Engine::NativePlantUml => "Omaspec PlantUML",
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
    /// far as Omaspec drew them; empty for the fallbacks.
    pub map: Map,
    /// The tiles and states that name other diagrams of the project
    /// ([`Renderer::render_linked`]).
    pub links: Vec<Link>,
    /// A map's picture with its tiles' rooms left empty: each card shows
    /// the name and the kind of its diagram, and the whole is quick to
    /// show while the tiles are small on screen. [`Self::svg`] holds the
    /// pictures. `None` for anything but a map.
    pub cards: Option<String>,
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
        /// Zero-based line of what Omaspec's own engine does not draw,
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

/// Diagrams Omaspec does not draw at all, by their `@start…` word. A
/// declined line of a kind it does draw (timing, Gantt, mind map, WBS,
/// sequence, state, structure, signal flow, JSON, YAML, formulas) keeps that line.
/// Signal flow is Omaspec's own syntax, so a declined line is not sent to Java.
const WHOLE: &[&str] = &["chronology", "dot", "tree"];

/// For a PlantUML diagram Omaspec's own engine does not draw at all,
/// the error that says so. Chronology, Graphviz and tree diagrams are
/// that kind: a line would only point at the first line
/// of a different language. A timing, Gantt, JSON, YAML or formula diagram that
/// declines keeps that line.
fn not_native(source: &str) -> Option<RenderError> {
    if !WHOLE.contains(&kind::plantuml_block(source).as_str()) {
        return None;
    }
    let found = kind::classify(source, Language::PlantUml);
    // PlantUML itself no longer draws it: no package would help.
    if kind::plantuml_block(source) == "chronology" {
        return Some(RenderError::Unavailable {
            line: None,
            reason: "a chronology is not drawn: PlantUML's own release does not draw it \
                     either; a Gantt chart or a timeline says the same"
                .into(),
        });
    }
    Some(RenderError::Unavailable {
        line: None,
        reason: format!(
            "`@start{}` ({}) is not drawn by Omaspec's own engine yet; install the \
                 `plantuml` package to draw it with the Java PlantUML",
            kind::plantuml_block(source),
            found.name
        ),
    })
}

/// What Omaspec's own engines say about the block of lines and boxes by
/// hand in `source` as they draw it (docs/adr/0016, 0021, 0022): an entry
/// that names no line or no box, a box the layout could not hold, each by
/// its zero-based line. The editor shows the same under the picture. Only
/// a diagram with that block is drawn; the Java PlantUML never is, and a
/// diagram that does not draw says nothing here.
pub fn by_hand_misses(source: &str, language: Language) -> Vec<(usize, String)> {
    if omaspec_uml::hand::read(source).is_none() {
        return Vec::new();
    }
    let map = match language {
        Language::Mermaid => mermaid::render_mapped(source).ok().map(|(_, _, map)| map),
        Language::PlantUml => omaspec_uml::render_mapped(source, None)
            .ok()
            .map(|(_, map)| map),
    };
    map.map(|map| map.misses).unwrap_or_default()
}

/// The state machine `source`, drawn by Omaspec's own engines with
/// `marks` over it: what a trace took and what it never took. Only a state
/// diagram Omaspec draws itself takes marks; the Java PlantUML and mmdr
/// do not. A diagram that lives at `place` reads its `!include`s there.
pub fn render_marked(
    source: &str,
    language: Language,
    place: Option<&Place>,
    marks: &omaspec_uml::state::Marks,
) -> Result<Rendered, RenderError> {
    let start = Instant::now();
    let (svg, engine, map) = match language {
        Language::Mermaid => {
            let (svg, map) = mermaid::render_marked(source, marks)?;
            (svg, Engine::NativeMermaid, map)
        }
        Language::PlantUml => {
            let folder = place.map(include::Folder::new);
            let includes = folder.as_ref().map(|f| f as &dyn omaspec_uml::Includes);
            let (svg, map) =
                omaspec_uml::render_marked(source, includes, marks).map_err(|declined| {
                    RenderError::Unavailable {
                        line: declined.line,
                        reason: declined.reason,
                    }
                })?;
            (svg, Engine::NativePlantUml, map)
        }
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
        links: Vec::new(),
        cards: None,
    })
}

/// The state machine `source` as a table, a row for each state and a
/// column for each event (`omaspec_model::matrix`), drawn by Omaspec's own
/// engines in the look of its states; [`Rendered::map`] says where each
/// line of a cell is and which transition it says
/// ([`omaspec_uml::map::Cell`]). A diagram that lives at `place` reads its
/// `!include`s there.
pub fn render_table(
    source: &str,
    language: Language,
    place: Option<&Place>,
) -> Result<Rendered, RenderError> {
    let start = Instant::now();
    let Some(machine) = omaspec_model::Machine::read(source, language) else {
        return Err(RenderError::Unavailable {
            line: None,
            reason: "only a state machine Omaspec reads has a table of states and events".into(),
        });
    };
    let table = omaspec_model::matrix(&machine).table(&machine);
    let (svg, engine, map) = match language {
        Language::Mermaid => {
            let (svg, map) = mermaid::render_table(source, &table)?;
            (svg, Engine::NativeMermaid, map)
        }
        Language::PlantUml => {
            let folder = place.map(include::Folder::new);
            let includes = folder.as_ref().map(|f| f as &dyn omaspec_uml::Includes);
            let (svg, map) =
                omaspec_uml::render_table(source, includes, &table).map_err(|declined| {
                    RenderError::Unavailable {
                        line: declined.line,
                        reason: declined.reason,
                    }
                })?;
            (svg, Engine::NativePlantUml, map)
        }
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
        links: Vec::new(),
        cards: None,
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
    /// A program that runs a command confined (Omaspec's own
    /// `omaspec __sandbox SCRATCH -- COMMAND …`); the Java PlantUML is
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
    /// The pictures of a project's map, kept between drawings.
    pictures: atlas::Pictures,
}

impl Renderer {
    pub fn new(config: &Config) -> Self {
        let java = config
            .java_fallback
            .then(|| JavaPlantUml::find(config))
            .flatten();
        Self {
            java,
            pictures: atlas::Pictures::default(),
        }
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
        // The pane's table view of a state machine (`' view (Omaspec) table`).
        if omaspec_uml::fold::table(source) {
            return render_table(source, language, place);
        }
        let start = Instant::now();
        let (svg, engine, map) = match language {
            Language::Mermaid => mermaid::render_mapped(source)?,
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
            links: Vec::new(),
            cards: None,
        })
    }

    /// Renders every diagram of the Markdown document `text` (its fenced
    /// `plantuml` and `mermaid` blocks) one under another
    /// ([`stack::stack`]). Each is drawn from the document with every other
    /// line empty, so an error names the document's line; the first error
    /// ends the drawing, and a document without a diagram says so.
    pub fn render_document(
        &mut self,
        text: &str,
        place: Option<&Place>,
    ) -> Result<Rendered, RenderError> {
        self.render_document_with(text, place, false)
    }

    /// [`Self::render_document`], its JSON and YAML diagrams drawn nested
    /// when `nested` says so (`' layout (Omaspec) nested`).
    pub fn render_document_with(
        &mut self,
        text: &str,
        place: Option<&Place>,
        nested: bool,
    ) -> Result<Rendered, RenderError> {
        let start = Instant::now();
        let blocks = omaspec_core::markdown::blocks(text);
        let Some(first) = blocks.first() else {
            return Err(RenderError::Unavailable {
                line: None,
                reason: "no PlantUML or Mermaid diagram in this document".into(),
            });
        };
        let mut engine = None;
        let mut pictures = Vec::with_capacity(blocks.len());
        for block in &blocks {
            let mut masked = omaspec_core::markdown::masked(text, block);
            let tables = matches!(kind::plantuml_block(&block.code).as_str(), "json" | "yaml");
            if nested && block.language == Language::PlantUml && tables {
                masked.push('\n');
                masked.push_str(&omaspec_uml::fold::nested_line(false));
            }
            let drawn = self.render_at(&masked, block.language, place)?;
            engine.get_or_insert(drawn.engine);
            pictures.push(drawn.svg);
        }
        let shown: Vec<&str> = pictures.iter().map(String::as_str).collect();
        let engine = engine.unwrap_or(match first.language {
            Language::Mermaid => Engine::Mermaid,
            Language::PlantUml => Engine::NativePlantUml,
        });
        let svg = stack::stack(&shown).map_err(|e| RenderError::Engine {
            engine,
            message: format!("invalid SVG: {e}"),
        })?;
        Ok(Rendered {
            svg,
            engine,
            elapsed: start.elapsed(),
            map: Map::default(),
            links: Vec::new(),
            cards: None,
        })
    }

    /// Renders a diagram of a project, the one in `own`, that links to the
    /// project's diagrams in `atlas`
    /// (`docs/adr/0018-diagrams-that-link.md`): a state machine with the
    /// states that name one, which [`Rendered::links`] says the places
    /// of. Any other diagram draws as it is, a mind map and a WBS too;
    /// the project's map is made ([`Self::render_map`]).
    pub fn render_linked(
        &mut self,
        source: &str,
        language: Language,
        place: Option<&Place>,
        own: Option<&str>,
        atlas: &Atlas,
    ) -> Result<Rendered, RenderError> {
        let mut rendered = self.render_at(source, language, place)?;
        rendered.links = atlas::state_links(&rendered.map, atlas, own);
        Ok(rendered)
    }

    /// Draws the map of `folder`, a folder of the project `atlas` holds by
    /// its path there (the empty one: the project itself), in the look of
    /// `skin` ([`atlas::folder`], `docs/adr/0026-the-map-is-made.md`): its
    /// diagrams as tiles that show them, its folders as labels over
    /// theirs or, in a large project, as tiles that open their own maps.
    /// No file is written for it; [`Rendered::links`] says where the
    /// tiles lead.
    pub fn render_map(
        &mut self,
        atlas: &Atlas,
        folder: &str,
        skin: &str,
    ) -> Result<Rendered, RenderError> {
        atlas::folder::render(atlas, folder, skin, &mut self.pictures)
    }

    fn plantuml(
        &mut self,
        source: &str,
        place: Option<&Place>,
    ) -> Result<(String, Engine, Map), RenderError> {
        let folder = place.map(include::Folder::new);
        let includes = folder.as_ref().map(|f| f as &dyn omaspec_uml::Includes);
        let declined = match omaspec_uml::render_mapped(source, includes) {
            Ok((svg, map)) => return Ok((svg, Engine::NativePlantUml, map)),
            // A mistake PlantUML refuses too: Java would only say it worse.
            Err(declined) if declined.refused => {
                return Err(RenderError::Diagram {
                    engine: Engine::NativePlantUml,
                    line: declined.line,
                    message: declined.reason,
                });
            }
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
