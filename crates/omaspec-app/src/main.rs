mod check;
mod check_command;
mod commands;
mod document;
mod editor;
mod files;
mod gallery;
mod git;
mod git_setup;
mod history;
mod icons;
mod index;
mod logo;
mod look;
mod mcp;
mod names;
mod paper;
mod preview;
mod project;
mod project_export;
mod render_command;
mod sandbox;
#[cfg(test)]
mod scale;
mod specs;
mod ui;
mod worker;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use omaspec_core::Language;
use omaspec_render::export::Format;

/// Native PlantUML and Mermaid editor for Omarchy.
///
/// Without a command, opens the editor: the last session, plus the given
/// diagrams (a file that does not exist yet is created on the first save);
/// a folder becomes the folder in the sidebar.
#[derive(Parser)]
#[command(name = "omaspec", version, args_conflicts_with_subcommands = true)]
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
    /// Start a project: a folder with `omaspec-project.toml` and a first
    /// diagram.
    ///
    /// The file marks the project's root. A diagram anywhere below it is
    /// checked against the state machines of the whole project, and its
    /// `!include`s reach into all of it. The file names the project and its
    /// main diagram, `system.puml` or `system.mmd`: the whole system as a
    /// small state machine to start from. A folder that is there keeps its
    /// files; one that is a project already is refused. With --git, a git
    /// repository starts in it, unless it is inside one, and a pre-commit
    /// hook runs `omaspec check` unless a hook is there already. Exits
    /// with 1 when git could not start one.
    New {
        /// The project's folder; created if it is not there.
        dir: PathBuf,
        /// The language of the first diagram.
        #[arg(long, value_enum, default_value = "plantuml")]
        lang: LangChoice,
        /// Start a git repository in the folder too.
        #[arg(long)]
        git: bool,
        /// Print what was made as JSON.
        #[arg(long)]
        json: bool,
    },
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
        /// against; by default each file's project (the nearest folder
        /// above it with `omaspec-project.toml`), else its own folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Check a project for CI.
    ///
    /// Runs lint on every diagram and Markdown document in the folder,
    /// including the checks against its state machines and links to
    /// diagrams it does not have, then holds each SVG against the one
    /// drawn now, as `render --check` does. Exits with 1 on an error or
    /// an SVG that differs or is missing (with --strict also a warning).
    /// In GitHub Actions the annotations are printed too.
    Check {
        /// The project folder.
        dir: PathBuf,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
        /// Fail on warnings too.
        #[arg(long)]
        strict: bool,
        /// Print GitHub Actions annotations instead of the report.
        #[arg(long, conflicts_with = "json")]
        github: bool,
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
        /// default each trace's project, else its own folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// List every place in the project that uses a name.
    ///
    /// A diagram of that name: where it says its name, the maps' boxes, the
    /// scenarios' participants that play it, the states of other machines
    /// that open it, the traces of it. A state or an event of a state
    /// machine: the transitions, the messages sent to its participants, the
    /// traces. Names compare as links compare them: letters and digits,
    /// case aside, so `Door control` is `DoorControl`. One use a line,
    /// `file:line: kind: text`. Exits with 1 when nothing uses the name.
    Refs {
        /// The name of a diagram, a state or an event.
        name: String,
        /// Only this state machine's own name, states and events.
        #[arg(long, value_name = "NAME")]
        machine: Option<String>,
        /// Print the uses as JSON.
        #[arg(long)]
        json: bool,
        /// The project folder; by default the project the current folder
        /// is in, else the current folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Rename a diagram, a state or an event everywhere it is used.
    ///
    /// A diagram: its title or `@start…` name, the maps' boxes, the
    /// participants that play it, the states of other machines that open
    /// it. A state: its machine's lines, left of their labels. An event:
    /// its machine's triggers and the messages sent to its participants.
    /// Each use is spelled as the old one was (`FrontGate`, `front_gate`,
    /// or words where it had spaces). Files keep their names; recorded
    /// traces are left alone and listed. When the old name is several
    /// things, --machine and --diagram, --state or --event say which.
    /// Exits with 1 when the rename is refused.
    Rename {
        /// The name as it is now.
        old: String,
        /// The new name.
        new: String,
        /// The state machine whose state or event (or own name) it is.
        #[arg(long, value_name = "NAME")]
        machine: Option<String>,
        /// Rename the diagram of that name.
        #[arg(long, conflicts_with_all = ["state", "event"])]
        diagram: bool,
        /// Rename the state of that name.
        #[arg(long, conflicts_with = "event")]
        state: bool,
        /// Rename the event of that name.
        #[arg(long)]
        event: bool,
        /// Show what would change; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Print what changed as JSON.
        #[arg(long)]
        json: bool,
        /// The project folder; by default the project the current folder
        /// is in, else the current folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Tell what changed in the behavior between two versions of a diagram,
    /// or of a project.
    ///
    /// State machines are compared by their states, transitions and
    /// triggers, sequence diagrams by their participants, messages and
    /// blocks, also between PlantUML and Mermaid; then what the
    /// specification checks find now, or no longer. Layout, colors and
    /// notes do not count. With one file, the version in its last commit
    /// (or --rev) is the old one. A folder is a project: every diagram in
    /// it, and in its Markdown documents, compared with the one it was in
    /// the last commit (or --rev), or in another folder; a diagram that
    /// moved is followed by its text or its name, and each version's
    /// sequence diagrams are checked against that version's state
    /// machines. Exits with 0 when the behavior is the same, 1 when it
    /// changed.
    Diff {
        /// The old version and the new one; or only the new one. Two files,
        /// or two folders.
        #[arg(required = true, num_args = 1..=2)]
        files: Vec<PathBuf>,
        /// The git revision one file or folder is compared with [default:
        /// HEAD].
        #[arg(long)]
        rev: Option<String>,
        /// Print the changes as JSON.
        #[arg(long)]
        json: bool,
        /// The folder whose state machines sequence diagrams are checked
        /// against; by default the new file's project, else its own folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        #[command(flatten)]
        lang: LangArg,
    },
    /// Tell, commit by commit, what changed in the behavior of a diagram.
    ///
    /// Each commit that changed the file, newest first, following it where
    /// it was renamed: what `diff` tells between the file before and after
    /// the commit, or that its behavior stayed the same. A sequence diagram
    /// is checked against the state machines of its project as each commit
    /// had them. What is not committed yet is `omaspec diff FILE`'s.
    Log {
        /// A diagram file or a Markdown document in a git repository.
        file: PathBuf,
        /// At most this many commits.
        #[arg(short = 'n', long = "max-count", value_name = "N")]
        count: Option<usize>,
        /// Print the commits and their changes as JSON.
        #[arg(long)]
        json: bool,
        /// The folder whose state machines sequence diagrams are checked
        /// against; by default the file's project, else its own folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Let git show diagrams by their behavior.
    Git {
        #[command(subcommand)]
        command: GitCommand,
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
        /// against; by default the file's project, else its own folder.
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
    /// otherwise; a Markdown document gives one per diagram (README-1.svg,
    /// …), and a folder its diagrams and documents; SVG and PNG carry the diagram source. PlantUML diagrams
    /// the native engine cannot draw yet go to the `plantuml` command if
    /// installed (or the one in $OMASPEC_PLANTUML; `none` turns it off).
    /// `--json` prints where each image went, or the error and its line.
    /// As text (`-o door.txt`, `--format text`) a state machine or a
    /// sequence diagram is written as its behavior, one fact a line, for an
    /// agent or a terminal that shows no pictures.
    Render {
        /// Diagrams, Markdown documents or folders of them to render; `-`
        /// or none reads standard input.
        files: Vec<PathBuf>,
        /// Output file, `-` for standard output; only with one diagram.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Image format, or text (a state machine's or sequence diagram's
        /// behavior); default from the output's extension, else SVG.
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
        /// Draw in a look, laid over the file's own colors as the editor
        /// lays the Omarchy theme over: plain (black lines), omarchy (the
        /// active theme), light, dark, auto (one SVG, dark under the
        /// viewer's dark scheme), or file (the file's own colors). The file
        /// and the source stored in the image stay as written. Without it,
        /// a diagram of a project that pins a look (`[look]` in
        /// omaspec-project.toml) is drawn in that.
        #[arg(long, value_enum)]
        look: Option<crate::look::Look>,
        /// Print a JSON report on stdout instead of status lines.
        ///
        /// Each diagram is one object: where the image went, or the error
        /// and its line. The images are still written. Standard output
        /// cannot be the image in this mode.
        #[arg(long)]
        json: bool,
        /// Write nothing: hold each diagram's SVG, drawn now, against the
        /// one beside it (as committed) and say what changed; exit 1 when
        /// one differs or is missing. For CI, after an update of Omaspec.
        #[arg(long, conflicts_with_all = ["output", "copy"])]
        check: bool,
        #[command(flatten)]
        lang: LangArg,
    },
    /// Export a project as files anyone can open: every diagram as an SVG
    /// with its source, linked to one another, and a ZIP of it.
    ///
    /// The SVGs lie beside one another as the diagrams do, in the project's
    /// `export` folder unless --output names another; the main diagram is
    /// `index.svg` too. Each tile of a map and each state that opens a
    /// diagram links to that diagram's SVG, so a browser is enough to
    /// follow them. `<project>.zip` in the folder holds all of it. Nothing
    /// is written outside the folder.
    Export {
        /// The project's folder; the current one when left out.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The folder to write into.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Draw in a look, as `render --look` does; auto is one SVG that
        /// turns dark under the viewer's dark scheme.
        #[arg(long, value_enum)]
        look: Option<crate::look::Look>,
        /// Print a JSON report on stdout instead of status lines.
        #[arg(long)]
        json: bool,
    },
    /// Serve the checks, formatting and rendering to an AI agent over the
    /// Model Context Protocol.
    ///
    /// The agent's client starts `omaspec mcp` and speaks JSON-RPC with it
    /// over standard input and output, one message per line. The tools are
    /// lint, trace, inspect, outline, format and render, as on the command
    /// line; relative paths are taken from the folder the server was
    /// started in. Nothing goes over the network.
    Mcp,
    /// List the examples that come with Omaspec, or print one.
    ///
    /// The editor shows them as tiles (Ctrl+Shift+E). Without a name, prints
    /// each example's file name, group and title; with one, its source, to
    /// start a diagram from: `omaspec example coffee-machine > mine.puml`.
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
    /// Runs a command confined with Landlock (Omaspec starts the Java
    /// PlantUML this way).
    #[command(name = "__sandbox", hide = true)]
    Sandbox {
        /// The only folder the command may write to.
        scratch: PathBuf,
        /// The command and its arguments, after `--`.
        #[arg(last = true, required = true)]
        command: Vec<std::ffi::OsString>,
    },
    /// Show the palette Omaspec reads from the active Omarchy theme.
    Theme {
        /// Read this colors.toml instead of the active theme.
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum GitCommand {
    /// Make `git diff`, `git log -p` and `git show` tell diagrams by their
    /// behavior.
    ///
    /// Adds `diff=omaspec` for the diagram files to the project's
    /// .gitattributes, to be committed, and sets the command git runs for
    /// it (`diff.omaspec.textconv`) in the repository's own configuration,
    /// as git takes no command from a repository: in another clone, run it
    /// once. `git diff --no-textconv` shows the lines again.
    Setup {
        /// The project folder; by default the project the current folder
        /// is in, else the current folder.
        dir: Option<PathBuf>,
    },
    /// Print a diagram as git compares it: the states, transitions and
    /// findings of a state machine, sorted, the participants and messages
    /// of a sequence diagram; any other file as it is.
    Textconv {
        /// The file git hands over.
        file: PathBuf,
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
    /// A state machine's or a sequence diagram's behavior as text, one
    /// fact a line; so is an output ending in `.txt`.
    Text,
}

impl FormatChoice {
    /// The picture's format; `None` for text.
    fn image(self) -> Option<Format> {
        Some(match self {
            FormatChoice::Svg => Format::Svg,
            FormatChoice::Png => Format::Png,
            FormatChoice::Jpeg => Format::Jpeg,
            FormatChoice::Pdf => Format::Pdf,
            FormatChoice::Markdown => Format::Markdown,
            FormatChoice::Zip => Format::Zip,
            FormatChoice::Text => return None,
        })
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum LangChoice {
    Plantuml,
    Mermaid,
}

impl From<LangChoice> for Language {
    fn from(choice: LangChoice) -> Self {
        match choice {
            LangChoice::Plantuml => Language::PlantUml,
            LangChoice::Mermaid => Language::Mermaid,
        }
    }
}

impl LangArg {
    fn get(self) -> Option<Language> {
        self.lang.map(Language::from)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::New {
            dir,
            lang,
            git,
            json,
        }) => commands::new_project(&dir, lang.into(), git, json),
        Some(Command::Lint {
            files,
            lang,
            json,
            strict,
            project,
        }) => commands::lint(&files, lang.get(), json, strict, project.as_deref()),
        Some(Command::Check {
            dir,
            json,
            strict,
            github,
        }) => check_command::check(&dir, json, strict, github),
        Some(Command::Trace {
            files,
            json,
            strict,
            project,
        }) => commands::trace(&files, json, strict, project.as_deref()),
        Some(Command::Refs {
            name,
            machine,
            json,
            project,
        }) => names::refs_command(&name, machine.as_deref(), project.as_deref(), json),
        Some(Command::Rename {
            old,
            new,
            machine,
            diagram,
            state,
            event,
            dry_run,
            json,
            project,
        }) => {
            use omaspec_model::Thing;
            let thing = match (diagram, state, event) {
                (true, _, _) => Some(Thing::Diagram),
                (_, true, _) => Some(Thing::State),
                (_, _, true) => Some(Thing::Event),
                _ => None,
            };
            let which = names::Which {
                machine: machine.as_deref(),
                thing,
            };
            names::rename_command(&old, &new, &which, project.as_deref(), dry_run, json)
        }
        Some(Command::Git {
            command: GitCommand::Setup { dir },
        }) => git_setup::setup(dir.as_deref()),
        Some(Command::Git {
            command: GitCommand::Textconv { file },
        }) => git_setup::textconv(&file),
        Some(Command::Log {
            file,
            count,
            json,
            project,
        }) => history::log(&file, count, project.as_deref(), json),
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
            look,
            json,
            check,
            lang,
        }) => render_command::render(&render_command::RenderArgs {
            files: &files,
            output: output.as_deref(),
            format: format.and_then(FormatChoice::image),
            text: matches!(format, Some(FormatChoice::Text)),
            scale,
            source: !no_source,
            copy,
            look,
            lang: lang.get(),
            json,
            check,
        }),
        Some(Command::Export {
            dir,
            output,
            look,
            json,
        }) => project_export::command(&dir, output.as_deref(), look, json),
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
        // `| head` read what it wanted; the rest is not missed.
        Err(err) if commands::reader_gone(&err) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("omaspec: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn preview(file: &std::path::Path, lang: Option<Language>) -> anyhow::Result<ExitCode> {
    let path = preview::check(file)?;
    preview::run(path, lang).map_err(|e| anyhow::anyhow!("the preview window failed: {e}"))?;
    Ok(ExitCode::SUCCESS)
}
