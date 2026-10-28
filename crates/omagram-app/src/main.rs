mod commands;
mod editor;
mod files;
mod gallery;
mod icons;
mod logo;
mod mcp;
mod paper;
mod preview;
mod sandbox;
mod specs;
mod ui;
mod worker;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use omagram_core::Language;
use omagram_render::export::Format;

/// Native PlantUML and Mermaid editor for Omarchy.
///
/// Without a command, opens the editor: the last session, plus the given
/// diagrams (a file that does not exist yet is created on the first save);
/// a folder becomes the folder in the sidebar.
#[derive(Parser)]
#[command(name = "omagram", version, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Diagrams (or a folder) to open in the editor.
    files: Vec<PathBuf>,
    #[command(flatten)]
    lang: LangArg,
}

#[derive(Subcommand)]
enum Command {
    /// Check diagrams for syntax problems, likely typos and contradictions
    /// in a specification.
    ///
    /// A state diagram is checked as a state machine. A sequence diagram is
    /// checked against the state machines of its project: the messages a
    /// participant receives must be events its machine takes, in order.
    /// Exits with 1 if a file has an error (with --strict also a warning).
    Lint {
        /// Files to check; `-` or none reads standard input.
        files: Vec<PathBuf>,
        #[command(flatten)]
        lang: LangArg,
        /// Print the findings as JSON.
        #[arg(long)]
        json: bool,
        /// Fail on warnings too.
        #[arg(long)]
        strict: bool,
        /// The folder whose state machines sequence diagrams are checked
        /// against; by default each file's own folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Hold recorded traces against the state machines of their project.
    ///
    /// A trace is JSON Lines, one event per line: `object` and `event`, and
    /// optionally `state` (where the object is afterwards), `time`, and
    /// `machine` (the state machine's name, when the object is called
    /// otherwise). Other fields are left alone. Exits with 1 if a trace
    /// deviates from its machines or has a line that is no event (with
    /// --strict also on a warning).
    Trace {
        /// Traces to check; `-` or none reads standard input.
        files: Vec<PathBuf>,
        /// Print the findings as JSON.
        #[arg(long)]
        json: bool,
        /// Fail on warnings too.
        #[arg(long)]
        strict: bool,
        /// The folder whose state machines the traces are held against; by
        /// default each trace's own folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Tell what changed in the behavior between two versions of a diagram.
    ///
    /// State machines are compared by their states, transitions and
    /// triggers, sequence diagrams by their participants, messages and
    /// blocks, also between PlantUML and Mermaid; then what the
    /// specification checks find now, or no longer. Layout, colors and
    /// notes do not count. With one file, the version in its last commit
    /// (or --rev) is the old one. Exits with 0 when the behavior is the
    /// same, 1 when it changed.
    Diff {
        /// The old version and the new one; or only the new one.
        #[arg(required = true, num_args = 1..=2)]
        files: Vec<PathBuf>,
        /// The git revision one file is compared with [default: HEAD].
        #[arg(long)]
        rev: Option<String>,
        /// Print the changes as JSON.
        #[arg(long)]
        json: bool,
        /// The folder whose state machines sequence diagrams are checked
        /// against; by default the new file's own folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        #[command(flatten)]
        lang: LangArg,
    },
    /// Write a state machine or a sequence diagram in the other language.
    ///
    /// PlantUML becomes Mermaid, Mermaid becomes PlantUML, through what the
    /// diagram means. The result is read back and must have the same
    /// states, transitions, participants and messages, or nothing is
    /// written; behavior the other language cannot say (a history state in
    /// Mermaid) refuses. Looks it cannot say are left out and told on
    /// standard error. Exits with 1 when the diagram is not converted.
    Convert {
        /// The diagram; `-` or none reads standard input.
        file: Option<PathBuf>,
        /// Write the result to this file instead of standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the result, or why there is none, as JSON.
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        lang: LangArg,
    },
    /// Re-indent diagrams (two spaces per block; line breaks stay).
    Fmt {
        /// Files to format; `-` or none reads standard input.
        files: Vec<PathBuf>,
        #[command(flatten)]
        lang: LangArg,
        /// Rewrite the files in place.
        #[arg(long, conflicts_with = "check")]
        write: bool,
        /// Only report files that are not formatted (exit 1 if any).
        #[arg(long)]
        check: bool,
    },
    /// Show the structure of a diagram.
    Outline {
        file: Option<PathBuf>,
        #[command(flatten)]
        lang: LangArg,
        #[arg(long)]
        json: bool,
    },
    /// Print language, kind, findings and outline of a diagram as JSON.
    Inspect {
        file: Option<PathBuf>,
        #[command(flatten)]
        lang: LangArg,
        /// The folder whose state machines a sequence diagram is checked
        /// against; by default the file's own folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Open a window that shows a diagram and follows every save.
    ///
    /// Keys: +/- zoom, 0 100 %, f fit, h j k l pan, c copy PNG, C copy SVG,
    /// r render again, p play a state machine or scenario (1-9 a step, Space
    /// the next, Backspace back, Esc stops), q quit. Mouse wheel zooms,
    /// dragging pans.
    Preview {
        file: PathBuf,
        #[command(flatten)]
        lang: LangArg,
    },
    /// Render diagrams to SVG, PNG or PDF.
    ///
    /// Each image is written next to its diagram unless --output says
    /// otherwise; SVG and PNG carry the diagram source. PlantUML diagrams
    /// the native engine cannot draw yet go to the `plantuml` command if
    /// installed (or the one in $OMAGRAM_PLANTUML; `none` turns it off).
    /// `--json` prints where each image went, or the error and its line.
    Render {
        /// Diagrams to render; `-` or none reads standard input.
        files: Vec<PathBuf>,
        /// Output file, `-` for standard output; only with one diagram.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Image format; default from the output's extension, else SVG.
        #[arg(short, long, value_enum)]
        format: Option<FormatChoice>,
        /// PNG pixels per diagram unit.
        #[arg(long, default_value_t = 2.0)]
        scale: f32,
        /// Do not store the diagram source in SVG and PNG images.
        #[arg(long)]
        no_source: bool,
        /// Copy the image to the clipboard instead of writing a file.
        #[arg(long, conflicts_with = "output")]
        copy: bool,
        /// Print a JSON report on stdout instead of status lines.
        ///
        /// Each diagram is one object: where the image went, or the error
        /// and its line. The images are still written. Standard output
        /// cannot be the image in this mode.
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        lang: LangArg,
    },
    /// Serve the checks, formatting and rendering to an AI agent over the
    /// Model Context Protocol.
    ///
    /// The agent's client starts `omagram mcp` and speaks JSON-RPC with it
    /// over standard input and output, one message per line. The tools are
    /// lint, trace, inspect, outline, format and render, as on the command
    /// line; relative paths are taken from the folder the server was
    /// started in. Nothing goes over the network.
    Mcp,
    /// List the examples that come with Omagram, or print one.
    ///
    /// The editor shows them as tiles (Ctrl+Shift+E). Without a name, prints
    /// each example's file name, group and title; with one, its source, to
    /// start a diagram from: `omagram example coffee-machine > mine.puml`.
    Example {
        /// The example's file name, with or without its extension.
        name: Option<String>,
        /// Print the list, or the example with its source, as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print the diagram source stored in an exported PNG or SVG.
    Extract {
        image: PathBuf,
        /// Write the source to this file instead of standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Overwrite the output file if it exists.
        #[arg(long)]
        force: bool,
    },
    /// Runs a command confined with Landlock (Omagram starts the Java
    /// PlantUML this way).
    #[command(name = "__sandbox", hide = true)]
    Sandbox {
        /// The only folder the command may write to.
        scratch: PathBuf,
        /// The command and its arguments, after `--`.
        #[arg(last = true, required = true)]
        command: Vec<std::ffi::OsString>,
    },
    /// Show the palette Omagram reads from the active Omarchy theme.
    Theme {
        /// Read this colors.toml instead of the active theme.
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

#[derive(Args, Clone, Copy)]
struct LangArg {
    /// The diagram language; detected from the content (and file name) if omitted.
    #[arg(long, value_enum)]
    lang: Option<LangChoice>,
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatChoice {
    Svg,
    Png,
    Jpeg,
    Pdf,
    Markdown,
    Zip,
}

impl From<FormatChoice> for Format {
    fn from(f: FormatChoice) -> Self {
        match f {
            FormatChoice::Svg => Format::Svg,
            FormatChoice::Png => Format::Png,
            FormatChoice::Jpeg => Format::Jpeg,
            FormatChoice::Pdf => Format::Pdf,
            FormatChoice::Markdown => Format::Markdown,
            FormatChoice::Zip => Format::Zip,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum LangChoice {
    Plantuml,
    Mermaid,
}

impl LangArg {
    fn get(self) -> Option<Language> {
        self.lang.map(|l| match l {
            LangChoice::Plantuml => Language::PlantUml,
            LangChoice::Mermaid => Language::Mermaid,
        })
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::Lint {
            files,
            lang,
            json,
            strict,
            project,
        }) => commands::lint(&files, lang.get(), json, strict, project.as_deref()),
        Some(Command::Trace {
            files,
            json,
            strict,
            project,
        }) => commands::trace(&files, json, strict, project.as_deref()),
        Some(Command::Diff {
            files,
            rev,
            json,
            project,
            lang,
        }) => commands::diff(&files, rev.as_deref(), lang.get(), json, project.as_deref()),
        Some(Command::Convert {
            file,
            output,
            json,
            lang,
        }) => commands::convert(file, output.as_deref(), lang.get(), json),
        Some(Command::Fmt {
            files,
            lang,
            write,
            check,
        }) => commands::fmt(&files, lang.get(), write, check),
        Some(Command::Outline { file, lang, json }) => commands::outline(file, lang.get(), json),
        Some(Command::Inspect {
            file,
            lang,
            project,
        }) => commands::inspect(file, lang.get(), project.as_deref()),
        Some(Command::Preview { file, lang }) => preview(&file, lang.get()),
        Some(Command::Render {
            files,
            output,
            format,
            scale,
            no_source,
            copy,
            json,
            lang,
        }) => commands::render(&commands::RenderArgs {
            files: &files,
            output: output.as_deref(),
            format: format.map(Format::from),
            scale,
            source: !no_source,
            copy,
            lang: lang.get(),
            json,
        }),
        Some(Command::Mcp) => mcp::run(),
        Some(Command::Example { name, json }) => commands::example(name.as_deref(), json),
        Some(Command::Extract {
            image,
            output,
            force,
        }) => commands::extract(&image, output.as_deref(), force),
        Some(Command::Theme { file }) => commands::theme(file),
        Some(Command::Sandbox { scratch, command }) => sandbox::run(&scratch, &command),
        None => editor::run(editor::Start::new(cli.files, cli.lang.get()))
            .map(|()| ExitCode::SUCCESS)
            .map_err(|e| anyhow::anyhow!("the editor window failed: {e}")),
    };
    match result {
        Ok(code) => code,
        Err(err) => {
            eprintln!("omagram: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn preview(file: &std::path::Path, lang: Option<Language>) -> anyhow::Result<ExitCode> {
    let path = preview::check(file)?;
    preview::run(path, lang).map_err(|e| anyhow::anyhow!("the preview window failed: {e}"))?;
    Ok(ExitCode::SUCCESS)
}
