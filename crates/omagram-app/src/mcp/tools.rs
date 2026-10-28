//! The tools of `omagram mcp`: what the command line does, each with the
//! schema and the description an agent reads before it calls it. Results
//! are the command line's JSON, as text and as structured content.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use base64::Engine as _;
use omagram_core::lint::{Finding, Severity};
use omagram_core::{Language, format, outline};
use omagram_render::export::{self, Format};
use omagram_render::{Place, Renderer};
use serde_json::{Map, Value, json};

use crate::commands::{
    failure, inspection, language, millis, one_based, other, render_config, reported,
    reported_changes, versions,
};
use crate::editor::folder::scan;
use crate::files::{Input, MAX_SOURCE, read_input, write_atomic};
use crate::specs::Projects;

/// The largest picture `render` returns for the agent to look at; a file
/// through `output` may be larger.
const MAX_LOOK_PIXELS: f64 = 25_000_000.0;

/// The tools, in the order they are listed.
const NAMES: [&str; 8] = [
    "lint", "trace", "inspect", "outline", "diff", "convert", "format", "render",
];

pub fn exists(name: &str) -> bool {
    NAMES.contains(&name)
}

/// What a tool call gives back.
pub struct Answer {
    content: Vec<Value>,
    structured: Option<Value>,
    is_error: bool,
}

impl Answer {
    /// `data` as JSON text, for every client, and as structured content,
    /// for those that read it.
    fn data(data: Value) -> Answer {
        Answer {
            content: vec![text(&pretty(&data))],
            structured: Some(data),
            is_error: false,
        }
    }

    /// A failure the agent reads, to try again otherwise.
    pub fn failed(message: &str) -> Answer {
        Answer {
            content: vec![text(message)],
            structured: None,
            is_error: true,
        }
    }

    pub fn into_json(self) -> Value {
        let mut result = json!({ "content": self.content });
        if let Some(structured) = self.structured {
            result["structuredContent"] = structured;
        }
        if self.is_error {
            result["isError"] = Value::Bool(true);
        }
        result
    }
}

fn text(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// Every tool's name, title, description, input schema and hints.
pub fn definitions() -> Vec<Value> {
    let path = json!({
        "type": "string",
        "description": "A diagram file (.puml, .plantuml, .mmd, .md and the like), relative to the folder the server was started in.",
    });
    let code = json!({
        "type": "string",
        "description": "The diagram's text, instead of `path`.",
    });
    let lang = json!({
        "type": "string",
        "enum": ["plantuml", "mermaid"],
        "description": "The diagram language; detected from the text and the file name if left out.",
    });
    let project = json!({
        "type": "string",
        "description": "The folder whose state machines are checked against; by default the file's own folder. Without it, `code` is checked on its own.",
    });
    let reading = json!({ "readOnlyHint": true, "openWorldHint": false });
    vec![
        json!({
            "name": "lint",
            "title": "Check diagrams",
            "description": "Checks PlantUML and Mermaid diagrams for syntax problems, likely typos and contradictions in a specification. A state diagram is checked as a state machine: states no path reaches, states nothing leaves, one event leading to two states. A sequence diagram is checked against the state machines of its project: each message a participant receives must be an event its machine takes, in order. Give a file, a folder (every diagram in it, checked against the folder's state machines) or the code. Errors are contradictions or broken syntax; warnings may be meant and say how to make them explicit.",
            "inputSchema": schema(&[
                ("path", json!({
                    "type": "string",
                    "description": "A diagram file, or a folder whose diagrams are all checked; relative to the folder the server was started in.",
                })),
                ("code", code.clone()),
                ("lang", lang.clone()),
                ("project", project.clone()),
            ]),
            "annotations": reading,
        }),
        json!({
            "name": "trace",
            "title": "Check a recorded trace",
            "description": "Holds a recorded trace against the state machines of its project. The trace is JSON Lines, one event per line: `object` and `event`, optionally `state` (where the object is afterwards), `time`, and `machine` when the object is called otherwise than its state machine. Each object is played through the state machine named like it; an event no state it can be in takes, an unknown event and a state it cannot be in are errors. The project is the trace file's folder, or `project`.",
            "inputSchema": schema(&[
                ("path", json!({
                    "type": "string",
                    "description": "A trace file (.jsonl), relative to the folder the server was started in.",
                })),
                ("code", json!({
                    "type": "string",
                    "description": "The trace's lines, instead of `path`; then `project` names the state machines.",
                })),
                ("project", project.clone()),
            ]),
            "annotations": reading,
        }),
        json!({
            "name": "inspect",
            "title": "Inspect a diagram",
            "description": "Tells the language and kind of a diagram, its findings (as `lint`), its outline, and whether Omagram draws it itself or leaves it to mmdr or the Java PlantUML, and why.",
            "inputSchema": schema(&[
                ("path", path.clone()),
                ("code", code.clone()),
                ("lang", lang.clone()),
                ("project", project.clone()),
            ]),
            "annotations": reading,
        }),
        json!({
            "name": "outline",
            "title": "Outline a diagram",
            "description": "The structure of a diagram: participants, states, groups, notes and the like, each with its line and depth.",
            "inputSchema": schema(&[
                ("path", path.clone()),
                ("code", code.clone()),
                ("lang", lang.clone()),
            ]),
            "annotations": reading,
        }),
        json!({
            "name": "diff",
            "title": "Compare two versions",
            "description": "Tells what changed in the behavior between two versions of a diagram, also between PlantUML and Mermaid: for a state machine its states (new, removed, moved, of another kind) and transitions (new, removed, another target, other events, guard or action); for a sequence diagram its participants, messages and blocks; then what the specification checks find now, or no longer. Layout, colors and notes do not count. The new version is `path` or `code`; the old one `old_path`, `old_code`, or `rev`, a git revision of `path` (HEAD when nothing else is given). Lines are one-based, each in its own version.",
            "inputSchema": schema(&[
                ("path", json!({
                    "type": "string",
                    "description": "The new version: a diagram file, relative to the folder the server was started in.",
                })),
                ("code", json!({
                    "type": "string",
                    "description": "The new version's text, instead of `path`.",
                })),
                ("old_path", json!({
                    "type": "string",
                    "description": "The old version: a diagram file.",
                })),
                ("old_code", json!({
                    "type": "string",
                    "description": "The old version's text.",
                })),
                ("rev", json!({
                    "type": "string",
                    "description": "The git revision of `path` that is the old version, such as HEAD or HEAD~2.",
                })),
                ("lang", lang.clone()),
                ("project", project.clone()),
            ]),
            "annotations": reading,
        }),
        json!({
            "name": "convert",
            "title": "Convert to the other language",
            "description": "Writes a state machine or a sequence diagram in the other language: PlantUML as Mermaid, Mermaid as PlantUML. It goes through what the diagram means and reads the result back: the states, transitions, participants and messages must come out the same, or nothing is written. Behavior the other language cannot say (a history state, a ref, a message from the edge in Mermaid) refuses with its line. Looks it cannot say (colors, note places, directions, shapes) are left out and listed as notices. Gives the new text back, or writes it to `output`.",
            "inputSchema": schema(&[
                ("path", path.clone()),
                ("code", code.clone()),
                ("lang", json!({
                    "type": "string",
                    "enum": ["plantuml", "mermaid"],
                    "description": "The language the diagram is written in; detected if left out. It is converted to the other one.",
                })),
                ("output", json!({
                    "type": "string",
                    "description": "Write the converted diagram to this file; it is replaced if it exists.",
                })),
            ]),
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false },
        }),
        json!({
            "name": "format",
            "title": "Format a diagram",
            "description": "Re-indents a diagram: two spaces per block, line breaks kept. Gives the formatted text back; with `write`, rewrites the file instead.",
            "inputSchema": schema(&[
                ("path", path.clone()),
                ("code", code.clone()),
                ("lang", lang.clone()),
                ("write", json!({
                    "type": "boolean",
                    "description": "Rewrite the file at `path` in place.",
                })),
            ]),
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        }),
        json!({
            "name": "render",
            "title": "Render a diagram",
            "description": "Draws a diagram. Without `output`, the picture comes back: a PNG to look at (the default), or the SVG's text. With `output`, it is written to that file instead, in the format its extension names (svg, png, jpeg, pdf, md, zip); SVG and PNG files carry the diagram's source. A diagram with an error comes back as an error with its line.",
            "inputSchema": schema(&[
                ("path", path),
                ("code", code),
                ("lang", lang),
                ("project", json!({
                    "type": "string",
                    "description": "The folder `!include` may read from; by default the file's own folder.",
                })),
                ("output", json!({
                    "type": "string",
                    "description": "Write the picture to this file; it is replaced if it exists.",
                })),
                ("format", json!({
                    "type": "string",
                    "enum": ["svg", "png", "jpeg", "pdf", "markdown", "zip"],
                    "description": "The format; by default the output's extension, else PNG to look at.",
                })),
                ("scale", json!({
                    "type": "number",
                    "exclusiveMinimum": 0,
                    "maximum": 16,
                    "description": "PNG and JPEG pixels per diagram unit; 1 for a picture to look at, 2 for a file, by default.",
                })),
                ("source", json!({
                    "type": "boolean",
                    "description": "Store the diagram's source in an SVG or PNG file (the default), so it can be opened again.",
                })),
            ]),
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false },
        }),
    ]
}

fn schema(properties: &[(&str, Value)]) -> Value {
    let properties: Map<String, Value> = properties
        .iter()
        .map(|(name, property)| ((*name).to_owned(), property.clone()))
        .collect();
    json!({ "type": "object", "properties": properties, "additionalProperties": false })
}

/// The tools and what they keep between calls.
pub struct Tools {
    renderer: Renderer,
    /// The folder the server runs in: no tool reads or writes outside it.
    root: PathBuf,
}

impl Default for Tools {
    /// The tools confined to the folder the server runs in.
    fn default() -> Self {
        Tools::with_root(PathBuf::new())
    }
}

impl Tools {
    /// The tools confined to the folder `root`; an empty `root` is the
    /// current folder, and a relative path stays relative in what the
    /// tools tell, as the agent wrote it.
    pub fn with_root(root: PathBuf) -> Self {
        Tools {
            renderer: Renderer::new(&render_config()),
            root,
        }
    }
}

impl Tools {
    /// Calls the tool `name`, which [`exists`].
    pub fn call(&mut self, name: &str, arguments: &Map<String, Value>) -> Result<Answer> {
        let definition = definitions()
            .into_iter()
            .find(|d| d["name"] == name)
            .with_context(|| format!("no tool {name}"))?;
        let known = definition["inputSchema"]["properties"]
            .as_object()
            .context("a tool without arguments")?;
        if let Some(unknown) = arguments.keys().find(|key| !known.contains_key(*key)) {
            let names: Vec<&str> = known.keys().map(String::as_str).collect();
            bail!("no argument `{unknown}`; {name} takes {}", names.join(", "));
        }
        let args = Args {
            map: arguments,
            root: &self.root,
        };
        match name {
            "lint" => lint(&args),
            "trace" => trace(&args),
            "inspect" => inspect(&args),
            "outline" => outline(&args),
            "diff" => diff(&args),
            "convert" => convert(&args),
            "format" => format(&args),
            "render" => Self::render(&mut self.renderer, &args),
            _ => bail!("no tool {name}"),
        }
    }

    fn render(renderer: &mut Renderer, args: &Args) -> Result<Answer> {
        let input = diagram(args)?;
        let lang = language(&input, args.lang()?);
        let project = args.path("project")?;
        let output = args.path("output")?;
        let format = match args.text("format")? {
            Some(name) => Some(Format::from_extension(name).with_context(|| {
                format!("no format `{name}`; take svg, png, jpeg, pdf, markdown or zip")
            })?),
            None => None,
        };
        let scale = args
            .number("scale")?
            .unwrap_or(if output.is_some() { 2.0 } else { 1.0 }) as f32;
        ensure!(
            scale > 0.0 && scale <= 16.0,
            "`scale` must be more than 0 and at most 16"
        );
        let place = input
            .path
            .as_deref()
            .and_then(|path| Place::of(path, project.as_deref()));
        let rendered = match renderer.render_at(&input.text, lang, place.as_ref()) {
            Ok(rendered) => rendered,
            Err(err) => {
                let (line, engine, message) = failure(&err);
                let mut answer = Answer::data(json!({
                    "line": line,
                    "engine": engine,
                    "message": message,
                }));
                answer.is_error = true;
                return Ok(answer);
            }
        };
        let mut report = json!({
            "engine": rendered.engine.name(),
            "elapsed_ms": millis(rendered.elapsed),
        });
        let Some(dest) = output else {
            let format = format.unwrap_or(Format::Png);
            report["format"] = json!(format.extension());
            // The picture is for the agent to look at; the source is no
            // use to it there.
            let options = export::Options {
                scale,
                source: None,
            };
            let picture = match format {
                Format::Png | Format::Jpeg => {
                    // The agent reads this image; keep it small enough to
                    // send, whatever the scale asks.
                    let size = export::parse(&rendered.svg)?.size();
                    let pixels = (f64::from(size.width()) * f64::from(scale)).ceil()
                        * (f64::from(size.height()) * f64::from(scale)).ceil();
                    ensure!(
                        pixels <= MAX_LOOK_PIXELS,
                        "the picture is {pixels:.0} pixels; use a smaller `scale`, or `output` to write a file"
                    );
                    let bytes = export::export(&rendered.svg, format, &options)?;
                    json!({
                        "type": "image",
                        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                        "mimeType": format.mime(),
                    })
                }
                Format::Svg => text(&rendered.svg),
                _ => bail!("`{}` needs `output`, the file to write", format.extension()),
            };
            let mut answer = Answer::data(report);
            answer.content.insert(0, picture);
            return Ok(answer);
        };
        let format = format
            .or_else(|| Format::from_extension(dest.extension()?.to_str()?))
            .unwrap_or(Format::Svg);
        if let Some(path) = &input.path {
            ensure!(
                std::path::absolute(path)? != std::path::absolute(&dest)?,
                "{} would overwrite the diagram itself",
                dest.display()
            );
        }
        let options = export::Options {
            scale,
            source: args
                .flag("source")?
                .unwrap_or(true)
                .then(|| export::Source {
                    code: &input.text,
                    language: lang,
                    file: input.file_name(),
                }),
        };
        let bytes = export::export(&rendered.svg, format, &options)?;
        write_atomic(&dest, &bytes)?;
        report["format"] = json!(format.extension());
        report["output"] = json!(dest.display().to_string());
        Ok(Answer::data(report))
    }
}

/// A call's arguments, read with the type each one must have. `root` is
/// the folder the server runs in; a path argument is resolved against it
/// and may not leave it.
struct Args<'a> {
    map: &'a Map<String, Value>,
    root: &'a Path,
}

impl Args<'_> {
    fn text(&self, key: &str) -> Result<Option<&str>> {
        match self.map.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) => Ok(Some(text)),
            Some(_) => bail!("`{key}` must be a string"),
        }
    }

    /// A path argument, resolved against the folder the server runs in and
    /// kept inside it.
    fn path(&self, key: &str) -> Result<Option<PathBuf>> {
        match self.text(key)? {
            None => Ok(None),
            Some(text) => Ok(Some(confine(self.root, Path::new(text))?)),
        }
    }

    fn flag(&self, key: &str) -> Result<Option<bool>> {
        match self.map.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(flag)) => Ok(Some(*flag)),
            Some(_) => bail!("`{key}` must be true or false"),
        }
    }

    fn number(&self, key: &str) -> Result<Option<f64>> {
        match self.map.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Number(n)) => Ok(n.as_f64()),
            Some(_) => bail!("`{key}` must be a number"),
        }
    }

    fn lang(&self) -> Result<Option<Language>> {
        Ok(match self.text("lang")? {
            None => None,
            Some(lang) if lang.eq_ignore_ascii_case("plantuml") => Some(Language::PlantUml),
            Some(lang) if lang.eq_ignore_ascii_case("mermaid") => Some(Language::Mermaid),
            Some(lang) => bail!("`lang` is plantuml or mermaid, not {lang}"),
        })
    }
}

/// `path`, resolved against the folder `root` the server runs in and kept
/// inside it. A path that leaves the folder, leads through a hidden folder,
/// or is a symbolic link out of it is refused, so a tool cannot read
/// `~/.ssh/id_ed25519` or write `~/.bashrc`. Works for a file that does not
/// exist yet (an output), whose folder must still be inside `root`. The
/// path comes back as written, joined to `root`.
fn confine(root: &Path, path: &Path) -> Result<PathBuf> {
    let base = if root.as_os_str().is_empty() {
        std::env::current_dir().context("no current folder")?
    } else {
        root.to_path_buf()
    };
    let base = base.canonicalize().unwrap_or(base);
    let joined = base.join(path);
    let name = joined
        .file_name()
        .with_context(|| format!("`{}` is not a file", path.display()))?;
    let parent = joined.parent().unwrap_or(&base);
    let parent = parent
        .canonicalize()
        .map_err(|_| anyhow::anyhow!("`{}` is not in a folder that exists", path.display()))?;
    let full = parent.join(name);
    // Follow it if it is there: a symbolic link out of the folder is refused.
    let real = full.canonicalize().unwrap_or_else(|_| full.clone());
    let inside = real.strip_prefix(&base).map_err(|_| {
        anyhow::anyhow!(
            "`{}` is outside the folder the server runs in",
            path.display()
        )
    })?;
    if inside
        .components()
        .any(|c| matches!(c, Component::Normal(n) if n.to_string_lossy().starts_with('.')))
    {
        bail!("`{}` is hidden or in a hidden folder", path.display());
    }
    Ok(root.join(path))
}

/// The diagram a call names, by `path` or by `code`.
fn diagram(args: &Args) -> Result<Input> {
    match (args.path("path")?, args.text("code")?) {
        (Some(path), None) => read_input(Some(&path)),
        (None, Some(code)) => {
            ensure!(code.len() as u64 <= MAX_SOURCE, "`code` larger than 16 MB");
            Ok(Input {
                path: None,
                text: code.to_owned(),
            })
        }
        (Some(_), Some(_)) => bail!("give `path` or `code`, not both"),
        (None, None) => bail!("give the diagram as `path` (a file) or `code` (its text)"),
    }
}

/// The name findings carry: the file as given, or `<code>`.
fn name(input: &Input) -> String {
    input
        .path
        .as_ref()
        .map_or_else(|| "<code>".into(), |path| path.display().to_string())
}

/// Findings of several files, with how many are errors and warnings.
fn findings(all: &[(String, Vec<Finding>)]) -> Result<Value> {
    let count = |severity| {
        all.iter()
            .flat_map(|(_, findings)| findings)
            .filter(|f| f.severity == severity)
            .count()
    };
    let reported: Vec<_> = all
        .iter()
        .flat_map(|(file, findings)| reported(file, findings))
        .collect();
    Ok(json!({
        "errors": count(Severity::Error),
        "warnings": count(Severity::Warning),
        "findings": serde_json::to_value(reported)?,
    }))
}

fn lint(args: &Args) -> Result<Answer> {
    let lang = args.lang()?;
    let mut project = args.path("project")?;
    let inputs = match args.path("path")? {
        Some(dir) if dir.is_dir() => {
            ensure!(
                args.text("code")?.is_none(),
                "give `path` or `code`, not both"
            );
            // A folder is its diagrams' project.
            project.get_or_insert_with(|| dir.clone());
            let mut files = scan(&dir);
            files.sort();
            files
                .iter()
                .map(|file| read_input(Some(&dir.join(file))))
                .collect::<Result<Vec<_>>>()?
        }
        _ => vec![diagram(args)?],
    };
    let mut projects = Projects::default();
    let mut all = Vec::new();
    for input in &inputs {
        let specs = projects.of(input.path.as_deref(), project.as_deref());
        let found = omagram_model::findings_with(&input.text, language(input, lang), specs);
        all.push((name(input), found));
    }
    let mut data = findings(&all)?;
    data["files"] = json!(inputs.len());
    Ok(Answer::data(data))
}

fn trace(args: &Args) -> Result<Answer> {
    let input = diagram(args)?;
    let project = args.path("project")?;
    let mut projects = Projects::default();
    let specs = projects.of(input.path.as_deref(), project.as_deref());
    let found = omagram_model::check_trace(&input.text, specs);
    let mut data = findings(&[(name(&input), found)])?;
    data["machines"] = json!(specs.len());
    Ok(Answer::data(data))
}

fn inspect(args: &Args) -> Result<Answer> {
    let input = diagram(args)?;
    let lang = language(&input, args.lang()?);
    let project = args.path("project")?;
    let specs = Projects::default()
        .of(input.path.as_deref(), project.as_deref())
        .to_vec();
    let found = omagram_model::findings_with(&input.text, lang, &specs);
    let inspection = inspection(&name(&input), &input.text, lang, &found);
    Ok(Answer::data(serde_json::to_value(inspection)?))
}

fn outline(args: &Args) -> Result<Answer> {
    let input = diagram(args)?;
    let lang = language(&input, args.lang()?);
    let entries = one_based(&outline::outline(&input.text, lang));
    Ok(Answer::data(json!({
        "language": lang,
        "outline": serde_json::to_value(entries)?,
    })))
}

fn diff(args: &Args) -> Result<Answer> {
    let lang = args.lang()?;
    let project = args.path("project")?;
    let new = diagram(args)?;
    let (old, old_name) = match (
        args.path("old_path")?,
        args.text("old_code")?,
        args.text("rev")?,
    ) {
        (Some(path), None, None) => {
            let input = read_input(Some(&path))?;
            (input, path.display().to_string())
        }
        (None, Some(code), None) => {
            ensure!(
                code.len() as u64 <= MAX_SOURCE,
                "`old_code` larger than 16 MB"
            );
            let input = Input {
                path: None,
                text: code.to_owned(),
            };
            (input, "<old code>".to_owned())
        }
        (None, None, rev) => {
            let path = new
                .path
                .as_deref()
                .context("give the old version as `old_path`, `old_code` or `rev`")?;
            let (old, name, _) = versions(None, path, Some(rev.unwrap_or("HEAD")))?;
            (old, name)
        }
        _ => bail!("give one old version: `old_path`, `old_code` or `rev`"),
    };
    let specs = Projects::default()
        .of(new.path.as_deref(), project.as_deref())
        .to_vec();
    let changes = omagram_model::diff(
        &old.text,
        language(&old, lang),
        &new.text,
        language(&new, lang),
        &specs,
    )
    .map_err(|reason| anyhow::anyhow!(reason))?;
    Ok(Answer::data(json!({
        "same": changes.is_empty(),
        "changes": serde_json::to_value(reported_changes(&old_name, &name(&new), &changes))?,
    })))
}

fn convert(args: &Args) -> Result<Answer> {
    let input = diagram(args)?;
    let from = language(&input, args.lang()?);
    let to = other(from);
    let output = args.path("output")?;
    let converted = match omagram_model::convert(&input.text, from, to) {
        Ok(converted) => converted,
        Err(refused) => {
            let mut answer = Answer::data(json!({
                "line": refused.line.map(|l| l + 1),
                "reason": refused.reason,
            }));
            answer.is_error = true;
            return Ok(answer);
        }
    };
    let notices: Vec<Value> = converted
        .notices
        .iter()
        .map(|n| json!({ "line": n.line.map(|l| l + 1), "message": n.message }))
        .collect();
    let Some(dest) = output else {
        return Ok(Answer::data(json!({
            "language": to,
            "code": converted.code,
            "notices": notices,
        })));
    };
    if let Some(path) = &input.path {
        ensure!(
            std::path::absolute(path)? != std::path::absolute(&dest)?,
            "{} would overwrite the diagram itself",
            dest.display()
        );
    }
    write_atomic(&dest, converted.code.as_bytes())?;
    Ok(Answer::data(json!({
        "language": to,
        "output": dest.display().to_string(),
        "notices": notices,
    })))
}

fn format(args: &Args) -> Result<Answer> {
    let input = diagram(args)?;
    let lang = language(&input, args.lang()?);
    let formatted = format::format(&input.text, lang);
    let changed = formatted != input.text;
    if !args.flag("write")?.unwrap_or(false) {
        return Ok(Answer::data(json!({
            "changed": changed,
            "formatted": formatted,
        })));
    }
    let path: &Path = input.path.as_deref().context("`write` needs `path`")?;
    if changed {
        write_atomic(path, formatted.as_bytes())?;
    }
    Ok(Answer::data(
        json!({ "changed": changed, "written": changed }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(tool: &str, arguments: Value) -> Value {
        // The scratch folders are under the system temp folder; root the
        // tools there, as the server is rooted in the folder it runs in.
        call_in(std::env::temp_dir(), tool, arguments)
    }

    fn call_in(root: PathBuf, tool: &str, arguments: Value) -> Value {
        let Value::Object(arguments) = arguments else {
            panic!("arguments")
        };
        match Tools::with_root(root).call(tool, &arguments) {
            Ok(answer) => answer.into_json(),
            Err(err) => Answer::failed(&format!("{err:#}")).into_json(),
        }
    }

    /// A fresh folder for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omagram-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lint_checks_a_state_machine_given_as_code() {
        let result = call(
            "lint",
            json!({"code": "@startuml\n[*] --> Idle\nIdle --> Busy : go\nIdle --> Done : go\nLost --> Idle : back\n@enduml\n"}),
        );
        assert!(result.get("isError").is_none(), "{result}");
        let data = &result["structuredContent"];
        assert_eq!(data["files"], 1);
        assert!(data["errors"].as_u64().unwrap() >= 1, "{data}");
        let findings = data["findings"].as_array().unwrap();
        let lost = findings
            .iter()
            .find(|f| f["message"].as_str().unwrap().contains("Lost"))
            .unwrap();
        assert_eq!(lost["line"], 5);
        assert_eq!(lost["file"], "<code>");
        // The same JSON, as text, for clients that read no structure.
        let text: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(&text, data);
    }

    #[test]
    fn lint_checks_a_folder_against_its_state_machines() {
        let dir = scratch("folder");
        std::fs::write(
            dir.join("door.puml"),
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("flows")).unwrap();
        std::fs::write(
            dir.join("flows/visit.puml"),
            "@startuml\nparticipant Door\nUser -> Door : open\nUser -> Door : kick\n@enduml\n",
        )
        .unwrap();
        let result = call("lint", json!({"path": dir.to_str().unwrap()}));
        let data = &result["structuredContent"];
        assert_eq!(data["files"], 2, "{result}");
        let kick = data["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["message"].as_str().unwrap().contains("kick"))
            .unwrap_or_else(|| panic!("{data}"));
        assert!(kick["file"].as_str().unwrap().ends_with("flows/visit.puml"));
        assert_eq!(kick["line"], 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trace_holds_lines_against_the_project() {
        let dir = scratch("trace");
        std::fs::write(
            dir.join("door.puml"),
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
        )
        .unwrap();
        let result = call(
            "trace",
            json!({
                "code": "{\"object\":\"door\",\"event\":\"open\"}\n{\"object\":\"door\",\"event\":\"open\"}\n",
                "project": dir.to_str().unwrap(),
            }),
        );
        let data = &result["structuredContent"];
        assert_eq!(data["machines"], 1, "{result}");
        assert_eq!(data["errors"], 1, "{data}");
        assert_eq!(data["findings"][0]["line"], 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inspect_and_outline_tell_the_structure() {
        let code = "sequenceDiagram\n  Alice->>Bob: hi\n  loop every minute\n    Bob->>Alice: ping\n  end\n";
        let inspected = call("inspect", json!({"code": code}));
        let data = &inspected["structuredContent"];
        assert_eq!(data["language"], "mermaid", "{data}");
        assert_eq!(data["native"], "understood", "{data}");
        let outline = call("outline", json!({"code": code}));
        let entries = outline["structuredContent"]["outline"].as_array().unwrap();
        assert!(entries.iter().any(|e| e["line"] == 3), "{outline}");
    }

    #[test]
    fn diff_tells_what_the_behavior_does_now() {
        let old =
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
        let new =
            "stateDiagram-v2\n[*] --> Closed\nClosed --> Open : open\nOpen --> Jammed : close\n";
        let result = call("diff", json!({"old_code": old, "code": new}));
        let data = &result["structuredContent"];
        assert_eq!(data["same"], false, "{result}");
        let changes = data["changes"].as_array().unwrap();
        let moved = changes
            .iter()
            .find(|c| c["change"] == "changed")
            .unwrap_or_else(|| panic!("{data}"));
        assert_eq!(
            moved["message"],
            "On close, Open now goes to Jammed instead of Closed."
        );
        assert_eq!(moved["old"]["line"], 4);
        assert_eq!(moved["new"]["line"], 4);
        let same = call("diff", json!({"old_code": old, "code": old}));
        assert_eq!(same["structuredContent"]["same"], true);
        let apart = call(
            "diff",
            json!({"old_code": old, "code": "@startuml\nA -> B : x\n@enduml\n"}),
        );
        assert_eq!(apart["isError"], true, "{apart}");
        let unnamed = call("diff", json!({"code": old}));
        assert_eq!(unnamed["isError"], true, "{unnamed}");
    }

    #[test]
    fn convert_writes_the_other_language() {
        let code = "@startuml\n[*] --> Idle\nIdle --> Busy : job\nstate Busy #pink\n@enduml\n";
        let result = call("convert", json!({"code": code}));
        let data = &result["structuredContent"];
        assert_eq!(data["language"], "mermaid", "{result}");
        assert!(
            data["code"]
                .as_str()
                .unwrap()
                .contains("Idle --> Busy : job"),
            "{data}"
        );
        assert!(
            data["notices"][0]["message"]
                .as_str()
                .unwrap()
                .contains("Colors"),
            "{data}"
        );
        let refused = call(
            "convert",
            json!({"code": "@startuml\nA -> B : x\nref over A, B : more\n@enduml\n"}),
        );
        assert_eq!(refused["isError"], true, "{refused}");
        assert_eq!(refused["structuredContent"]["line"], 3, "{refused}");
    }

    #[test]
    fn format_gives_the_text_back_or_rewrites_the_file() {
        let code = "@startuml\nalt yes\nA -> B\nend\n@enduml\n";
        let result = call("format", json!({"code": code}));
        assert_eq!(result["structuredContent"]["changed"], true);
        assert!(
            result["structuredContent"]["formatted"]
                .as_str()
                .unwrap()
                .contains("\n  A -> B\n")
        );
        let refused = call("format", json!({"code": code, "write": true}));
        assert_eq!(refused["isError"], true);

        let dir = scratch("format");
        let file = dir.join("alt.puml");
        std::fs::write(&file, code).unwrap();
        let written = call(
            "format",
            json!({"path": file.to_str().unwrap(), "write": true}),
        );
        assert_eq!(written["structuredContent"]["written"], true, "{written}");
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("\n  A -> B\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_shows_the_picture_or_writes_the_file() {
        let code = "@startuml\nAlice -> Bob : hi\n@enduml\n";
        let shown = call("render", json!({"code": code}));
        let picture = &shown["content"][0];
        assert_eq!(picture["type"], "image", "{shown}");
        assert_eq!(picture["mimeType"], "image/png");
        let png = base64::engine::general_purpose::STANDARD
            .decode(picture["data"].as_str().unwrap())
            .unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        assert_eq!(shown["structuredContent"]["format"], "png");

        let svg = call("render", json!({"code": code, "format": "svg"}));
        assert!(svg["content"][0]["text"].as_str().unwrap().contains("<svg"));
        let pdf = call("render", json!({"code": code, "format": "pdf"}));
        assert_eq!(pdf["isError"], true);

        let dir = scratch("render");
        let out = dir.join("hi.pdf");
        let written = call(
            "render",
            json!({"code": code, "output": out.to_str().unwrap()}),
        );
        assert_eq!(written["structuredContent"]["format"], "pdf", "{written}");
        assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF"));
        let diagram = dir.join("hi.puml");
        std::fs::write(&diagram, code).unwrap();
        let over = call(
            "render",
            json!({"path": diagram.to_str().unwrap(), "output": diagram.to_str().unwrap()}),
        );
        assert_eq!(over["isError"], true, "{over}");
        assert_eq!(std::fs::read_to_string(&diagram).unwrap(), code);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_tool_reads_or_writes_outside_the_servers_folder() {
        let root = scratch("confine");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(
            root.join("sub/door.puml"),
            "@startuml\n[*] --> A\n@enduml\n",
        )
        .unwrap();
        // A secret beside the folder, and a hidden one inside it.
        std::fs::write(root.join("secret.txt"), "key").unwrap();
        std::fs::create_dir_all(root.join(".ssh")).unwrap();
        std::fs::write(root.join(".ssh/id"), "private").unwrap();

        // A relative path inside the folder is fine.
        let ok = call_in(root.clone(), "lint", json!({"path": "sub/door.puml"}));
        assert!(ok.get("isError").is_none(), "{ok}");

        for (tool, args, says) in [
            ("format", json!({"path": "/etc/os-release"}), "outside"),
            (
                "format",
                json!({"path": "../../../etc/os-release"}),
                "outside",
            ),
            ("lint", json!({"path": "../secret.txt"}), "outside"),
            (
                "render",
                json!({"code": "@startuml\n[*]-->A\n@enduml\n", "output": "/tmp/pwn.svg"}),
                "outside",
            ),
            (
                "convert",
                json!({"code": "@startuml\n[*]-->A\n@enduml\n", "output": "../pwn.mmd"}),
                "outside",
            ),
            (
                "lint",
                json!({"code": "@startuml\n@enduml\n", "project": "/etc"}),
                "outside",
            ),
        ] {
            let got = call_in(root.join("sub"), tool, args.clone());
            assert_eq!(got["isError"], true, "{tool} {args}: {got}");
            let text = got["content"][0]["text"].as_str().unwrap();
            assert!(text.contains(says), "{tool} {args}: {text}");
        }
        assert!(!std::path::Path::new("/tmp/pwn.svg").exists());
        // A hidden folder inside the running folder is refused too.
        let hidden = call_in(root.clone(), "inspect", json!({"path": ".ssh/id"}));
        assert_eq!(hidden["isError"], true, "{hidden}");
        assert!(
            hidden["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("hidden")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_symlink_out_of_the_folder_is_refused() {
        let root = scratch("symlink");
        std::fs::write(root.join("real.puml"), "@startuml\n[*] --> A\n@enduml\n").unwrap();
        let secret = scratch("symlink-secret");
        std::fs::write(secret.join("key"), "@startuml\nsecret\n@enduml\n").unwrap();
        std::os::unix::fs::symlink(secret.join("key"), root.join("link.puml")).unwrap();
        let got = call_in(root.clone(), "format", json!({"path": "link.puml"}));
        assert_eq!(got["isError"], true, "{got}");
        assert!(
            got["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("outside")
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&secret);
    }

    #[test]
    fn a_huge_picture_to_look_at_is_refused() {
        let code = "@startuml\n[*] --> A\n@enduml\n";
        let huge = call("render", json!({"code": code, "scale": 16}));
        // The tiny diagram at 16x is still small; a real cap needs a large
        // diagram, so check the message path with a wide one.
        assert!(huge.get("isError").is_none(), "{huge}");
        let wide = format!(
            "@startuml\n{}\n@enduml\n",
            (0..400)
                .map(|i| format!("participant P{i}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let got = call("render", json!({"code": wide, "scale": 16}));
        assert_eq!(got["isError"], true, "{got}");
        assert!(
            got["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("smaller `scale`")
        );
    }

    #[test]
    fn render_reports_a_broken_diagram_with_its_line() {
        let broken = call(
            "render",
            json!({"code": "flowchart TD\n  subgraph S\n  A --> B\n"}),
        );
        assert_eq!(broken["isError"], true, "{broken}");
        assert_eq!(broken["structuredContent"]["line"], 2, "{broken}");
        assert!(
            broken["structuredContent"]["message"].is_string(),
            "{broken}"
        );
    }
}
