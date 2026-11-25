mod at_once;
mod check;
mod check_command;
mod clock;
mod commands;
mod coverage_command;
mod diff_markdown;
mod diff_picture;
mod document;
mod editor;
mod file_dialog;
mod files;
mod folds;
mod gallery;
mod git;
mod git_setup;
mod history;
mod icons;
mod index;
mod logo;
mod look;
mod matrix_command;
mod mcp;
mod names;
mod orbit_command;
mod paper;
mod preview;
mod project;
mod project_export;
mod render_command;
#[cfg(target_os = "linux")]
mod sandbox;
#[cfg(test)]
mod scale;
mod specs;
mod trace_follow;
mod trace_timing;
mod ui;
mod worker;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use omaspec_core::Language;
use omaspec_render::export::Format;

/// The version Omaspec tells: the release, and past it the commit it was
/// built from, when that is not the release's own (`0.9.8+3b94dc3`;
/// `build.rs`).
pub(crate) const VERSION: &str = env!("OMASPEC_VERSION");

/// Native PlantUML and Mermaid editor for Omarchy.
///
/// Without a command, opens the editor: the last session, plus the given
/// diagrams (a file that does not exist yet is created on the first save);
/// a folder becomes the folder in the sidebar.
#[derive(Parser)]
#[command(
    name = "omaspec",
    version = VERSION,
    args_conflicts_with_subcommands = true
)]
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
    /// `!include`s reach into all of it. The file names the project, and
    /// beside it stands the project's map, made from its diagrams: the
    /// editor shows it for that file, `omaspec render` draws it. The first
    /// diagram is a small state machine named after the project. A folder
    /// that is there keeps its files; one that is a project already is refused. With --git, a git
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
    /// Exits with 1 if a file has an error (with --strict also a warning,
    /// and a diagram only the Java PlantUML draws).
    Lint {
        /// Files to check; `-` or none reads standard input.
        files: Vec<PathBuf>,
        #[command(flatten)]
        lang: LangArg,
        /// Print the findings as JSON.
        #[arg(long)]
        json: bool,
        /// Fail on warnings too, and on a diagram only the Java PlantUML
        /// draws.
        #[arg(long)]
        strict: bool,
        /// Make every fix a finding carries in the files, then report
        /// what is left.
        #[arg(long)]
        fix: bool,
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
    /// an SVG that differs or is missing (with --strict also a warning, and
    /// a diagram only the Java PlantUML draws).
    /// In GitHub Actions the annotations are printed too.
    Check {
        /// The project folder.
        dir: PathBuf,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
        /// Fail on warnings too, and on a diagram only the Java PlantUML
        /// draws.
        #[arg(long)]
        strict: bool,
        /// Print GitHub Actions annotations instead of the report.
        #[arg(long, conflicts_with = "json")]
        github: bool,
    },
    /// A state machine as a table: a row for each state, a column for
    /// each event.
    ///
    /// Each cell says what the event does in the state: the transitions it
    /// fires, several with their guards (`→ Open [allowed]`); where the state
    /// has none, those of the state around it (`↑ → Off (from On)`); `⚠`
    /// where two ways have no guard to choose. An empty cell is no finding.
    /// Markdown on standard output, or into the .md file -o names.
    Matrix {
        /// The state machine, PlantUML or Mermaid.
        file: PathBuf,
        /// A .md file to write the table to, or a picture of it in the
        /// look of the machine: .svg, .png or .pdf.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the states, the events and every cell as JSON.
        #[arg(long)]
        json: bool,
    },
    /// A state machine's life cycle on a ring: its states round a circle
    /// in the order a run reaches them, a way on along the ring, a way
    /// back as an arc inside it, the start and the end where the ring
    /// opens. The SVG on standard output without -o.
    Orbit {
        /// The state machine, PlantUML or Mermaid.
        file: PathBuf,
        /// A picture of it in the look of the machine: .svg, .png or .pdf.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// How much of a state machine recorded traces take.
    ///
    /// The traces are followed as `trace` follows them, object by object,
    /// through the machine their objects go by (named like it, or by
    /// `machine`). Prints how many transitions and states were taken and
    /// each transition never taken, as `file:line`; with -o the diagram
    /// with what was taken drawn bold in green, how often after its label,
    /// and what never was dashed in red. The source stays as it is. Exits
    /// with 1 when fewer transitions were taken than --min asks for.
    Coverage {
        /// The state machine, PlantUML or Mermaid.
        file: PathBuf,
        /// Traces, JSON Lines as `trace` reads them.
        #[arg(long, required = true, num_args = 1.., value_delimiter = ',')]
        traces: Vec<PathBuf>,
        /// The diagram with the coverage drawn over it: .svg, .png or .pdf.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the report as JSON: the hits of each transition and state.
        #[arg(long)]
        json: bool,
        /// Fail when fewer than this many percent of the transitions are taken.
        #[arg(long, value_name = "PERCENT")]
        min: Option<f32>,
        /// Draw the picture as a trace river: each way as wide as it was
        /// taken, those never taken thin and dashed, and where traces left
        /// the machine said in red at the state they left from.
        #[arg(long, requires = "output")]
        river: bool,
    },
    /// Hold recorded traces against the state machines of their project.
    ///
    /// A trace is JSON Lines, one event per line: `object` and `event`, and
    /// optionally `state` (where the object is afterwards), `time`, and
    /// `machine` (the state machine's name, when the object is called
    /// otherwise). Other fields are left alone. Exits with 1 if a trace
    /// deviates from its machines or has a line that is no event (with
    /// --strict also on a warning). A deviation's JSON says what happened
    /// and what the machine expected, with the lines of the diagram: a
    /// task for whoever fixes it (docs/trace.md).
    Trace {
        /// Traces to check; `-` or none reads standard input.
        files: Vec<PathBuf>,
        /// Print the JSON Schema of a trace's line, for a program or an
        /// agent that writes traces, and nothing else.
        #[arg(long, conflicts_with_all = ["files", "json", "strict", "project", "as_kind"])]
        schema: bool,
        /// Follow one trace while it is written: each finding the moment
        /// its line is complete, with --json one object a line. A file
        /// not there yet is waited for; it runs until stopped. Standard
        /// input until it ends.
        #[arg(long, conflicts_with = "as_kind")]
        follow: bool,
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
        /// Write one trace as a PlantUML timing diagram instead: a row for
        /// each object, its states over time, a band where it deviates.
        #[arg(long = "as", value_name = "KIND")]
        as_kind: Option<TraceAs>,
        /// With --as: the diagram's file (.puml), or its picture (.svg,
        /// .png, .pdf); else standard output.
        #[arg(short, long, requires = "as_kind")]
        output: Option<PathBuf>,
    },
    /// List every place in the project that uses a name.
    ///
    /// A diagram of that name: where it says its name, the
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
    /// A diagram: its title or `@start…` name, the
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
        /// Write the changes as Markdown, a comment for a pull request: by
        /// diagram with their files and lines, and each state machine that
        /// changed drawn as Mermaid below them, what came in green, what
        /// changed in amber (docs/ci/).
        #[arg(long, conflicts_with_all = ["json", "output"])]
        markdown: bool,
        /// The folder whose state machines sequence diagrams are checked
        /// against; by default the new file's project, else its own folder.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Also draw a state machine's changes over its new version: what
        /// came in green, what changed in amber, a transition that went
        /// dashed in red; .svg, .png or .pdf.
        #[arg(short, long)]
        output: Option<PathBuf>,
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
        /// Also write what only Omaspec reads in a Mermaid flowchart the
        /// way mermaid.js (GitHub) reads it too.
        #[arg(long)]
        portable: bool,
        /// Also write a state machine or a scenario evenly: one space
        /// around arrows and colons, both in a column.
        #[arg(long)]
        pretty: bool,
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
        /// Follow a trace while it is written, on the state machine shown:
        /// each object a pill with its name beside the state it is in, a
        /// light along each transition a new line takes, in red where it
        /// deviates (docs/trace.md). Esc stops following.
        #[arg(long, value_name = "TRACE")]
        follow: Option<PathBuf>,
    },
    /// Write a play of a state machine or a scenario, a fold or a zoom into
    /// a project's map as a picture that moves: a GIF, or in full color an
    /// animated WebP or PNG, by the output's ending.
    ///
    /// A play, a fold, a dive, a lockstep, a history and a time travel
    /// (`--from`) are drawn as the window draws the diagram's pane, on the GPU without a window (by
    /// tiny-skia where there is none), in the active theme: the light
    /// runs along each transition, the states the machine is in shine.
    /// `--build`, `--ink`, `--themes` and `--traffic` draw without a GPU,
    /// and so does every page (`.html`). The events are taken by name
    /// (`--events open,close`), else the first the machine offers at each
    /// state, for `--steps` steps; a scenario plays its messages and its
    /// notes of their own in order,
    /// a git graph its commits, the veil going back as its time runs.
    /// For a presentation or a pull request.
    Play {
        file: PathBuf,
        /// The picture to write: `.gif`; `.webp` or `.png` (an animated
        /// PNG) in full color and without loss, some twice a GIF's size.
        #[arg(short, long)]
        output: PathBuf,
        /// The events to take, in order, with commas between them. An
        /// event with a comma in its name takes `\,` or double quotes round
        /// it: `--events 'start,"hello, world"'`. A state's way without an
        /// event is `go on`, or `go on B` where several lead on; where it is
        /// the one way on to the next event named, it is taken by itself.
        #[arg(long)]
        events: Vec<String>,
        /// How many steps at most, a scenario's notes of their own counted.
        /// Without it a state machine takes 8, or what `--events` names and
        /// the ways without an event on the way; a scenario every message
        /// and note and a git graph every commit, up to 50 messages or
        /// commits, and says where it stopped.
        #[arg(long)]
        steps: Option<usize>,
        /// The picture's width in pixels; its height follows the diagram.
        /// Without it, twice the diagram's size, up to 1200.
        #[arg(long)]
        width: Option<u32>,
        /// Frames per second: 20 without it and up to 50 in a GIF, 60 in
        /// full color.
        #[arg(long)]
        fps: Option<u32>,
        /// The places of the GIF's table of colors, 2 to 256, one of them
        /// for a pixel that stays: fewer make a smaller file, a glow and
        /// the veil's shades in steps (32 or 16 for a chat).
        #[arg(long, default_value_t = 256, value_parser = clap::value_parser!(u16).range(2..=256))]
        colors: u16,
        /// How far a pixel's red, green and blue may stray from what is
        /// shown: a pixel is drawn again when its color goes into another
        /// step of this many and one. 0 draws every change; 15 or 31 make
        /// a smaller file, a fade moving in a few even steps. With
        /// `--colors 32 --lossy 15` a play is about a third smaller.
        #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=63))]
        lossy: u8,
        /// Instead of a play: these containers fold to their boxes, one
        /// after another, and unfold again, the diagram gliding to each
        /// layout. Commas between them, as `render --fold` takes them.
        #[arg(long, value_name = "NAME", conflicts_with_all = ["events", "steps"])]
        fold: Vec<String>,
        /// On a project's omaspec-project.toml: its map zooms into
        /// these tiles, one after another, and back out. `Day/Order`
        /// flies on from the tile's diagram along its link to Order, and
        /// back out the same way.
        #[arg(long, value_delimiter = ',', value_name = "TILE", conflicts_with_all = ["events", "steps", "fold"])]
        dive: Vec<String>,
        /// Instead of a play: code becomes a picture. The source stands
        /// on the left, and as a caret walks down it, the diagram on the
        /// right builds itself up line by line: a box grows in, a line
        /// draws itself to its head. Drawn without a GPU.
        #[arg(long, conflicts_with_all = ["events", "steps", "fold", "dive"])]
        build: bool,
        /// On a sequence diagram: it plays beside the state machines of
        /// its participants, found in its project. As a message's light
        /// reaches its receiver, the receiver's machine takes the
        /// transition it fires; a message the machine cannot take runs
        /// red, and its states flash.
        #[arg(long, conflicts_with_all = ["events", "steps", "fold", "dive", "build"])]
        lockstep: bool,
        /// Instead of a play: the diagram's history in git, every commit
        /// that changed the file gliding into the next, oldest first, the
        /// commit under it (the newest 60).
        #[arg(long, conflicts_with_all = ["events", "steps", "fold", "dive", "build", "lockstep"])]
        history: bool,
        /// Instead of a play: time travel, the diagram as the commit REV
        /// left it gliding into it as it is now (or as `--to` left it),
        /// what came glowing in green, what changed in amber, what went
        /// fading in red.
        #[arg(long, value_name = "REV", conflicts_with_all = ["events", "steps", "fold", "dive", "build", "lockstep", "history"])]
        from: Option<String>,
        /// With `--from`: the commit it travels to, instead of the file
        /// as it is.
        #[arg(long, value_name = "REV", requires = "from")]
        to: Option<String>,
        /// Instead of a play: these recorded traces (JSON Lines) flowing
        /// through the state machine at once, each object a dot; a
        /// transition glows the more dots pass it, those never taken
        /// blink grey at the end, and a dot turns red where its trace
        /// leaves the machine. Drawn without a GPU.
        #[arg(long, value_delimiter = ',', value_name = "TRACE", conflicts_with_all = ["events", "steps", "fold", "dive", "build", "lockstep", "history", "from"])]
        traffic: Vec<PathBuf>,
        /// Instead of a play: the diagram through these Omarchy themes
        /// (names, colors.toml files or built-in), each sweeping over the
        /// one before, its name under it; without names, every bundled
        /// theme. Names follow an equals sign, as
        /// --themes=tokyo-night,gruvbox, so the file is never taken for
        /// one. Drawn without a GPU.
        #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "", value_delimiter = ',', value_name = "THEME", conflicts_with_all = ["events", "steps", "fold", "dive", "build", "lockstep", "history", "from", "traffic", "theme"])]
        themes: Option<Vec<String>>,
        /// Instead of a play: the diagram drawn in the sketch look stroke
        /// by stroke as with a pen, then inked into the clean look
        /// (`--look`, else the play's colors). The film starts and ends on
        /// the clean picture, in a frame of a screen's proportions. Drawn
        /// without a GPU.
        #[arg(long, conflicts_with_all = ["events", "steps", "fold", "dive", "build", "lockstep", "history", "from", "traffic", "themes"])]
        ink: bool,
        /// With `--ink`: the look the sketch is inked into (plain, vivid,
        /// omarchy, light, dark, file; auto is light in a film); without
        /// `--look`, the play's colors. Only `--ink` takes it: a play, a
        /// fold or a dive is drawn in `--theme`'s colors, a light one with
        /// a light theme's colors.toml.
        #[arg(long, value_enum, requires = "ink")]
        look: Option<crate::look::Look>,
        /// How long the film lasts, in seconds: its holds stretched or
        /// shortened and, where that does not do, its motions quicker.
        /// With `--ink` the pen takes the time the rest leaves.
        #[arg(long, value_name = "SECONDS")]
        duration: Option<f32>,
        /// How long the film holds the picture it starts again from, in
        /// seconds: a play's last state, an ink's clean picture.
        #[arg(long, value_name = "SECONDS")]
        hold: Option<f32>,
        /// The whole diagram in every frame. Without it, a play or a fold
        /// of a diagram too large to read whole follows each step or each
        /// container, near enough to read, and ends on the whole.
        #[arg(long, conflicts_with = "dive")]
        whole: bool,
        /// The colors: `built-in` for the palette Omaspec ships, or a
        /// theme's colors.toml; the active Omarchy theme without it.
        #[arg(long, value_name = "built-in|FILE")]
        theme: Option<String>,
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
        /// A PNG or JPEG too large at that scale (more than 100 million
        /// pixels) is drawn at the largest scale that fits, not refused.
        #[arg(long)]
        fit: bool,
        /// A PNG or JPEG whose smallest words are at least this many pixels
        /// high (their font size): drawn at --scale, shrunk to fit as --fit
        /// does but not below that size, raised to it where --scale is
        /// below it; where it then does not fit in one file, cut into tiles
        /// in a folder beside it (NAME-tiles/1.png, …, each cut between two
        /// lines of words, at most 100 tiles). Where it fits, one file.
        #[arg(long, value_name = "PX", conflicts_with = "fit")]
        min_font: Option<f32>,
        /// An SVG whose words are outlines, drawn here in the faces they
        /// were measured in: any viewer shows them inside their boxes,
        /// also one that takes no face from the picture, has none as wide
        /// and does not stretch words to their length (cairosvg,
        /// rsvg-convert on a bare system). The words can no longer be selected or searched, and
        /// links are no longer links.
        #[arg(long)]
        text_as_paths: bool,
        /// An SVG's ids, and every reference to one, start with this: two
        /// pictures on one HTML page then share none (`door-`, `lamp-`).
        /// Its words and the stored source are left as they are.
        #[arg(long, value_name = "PREFIX")]
        id_prefix: Option<String>,
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
        /// Fold these containers to their box, as the editor folds them: a
        /// composite state, a package or a subgraph, by name, with commas
        /// between them (`--fold Brewing,Payment`), or the option again; a
        /// name with a comma in it in double quotes or with `\,`. The
        /// picture shows the box and what it holds in numbers, its lines
        /// ending on it; the image keeps the file's source, and the names,
        /// so `--check` draws it so again (it needs the source kept beside
        /// the diagram). A name no diagram holds is an error that lists the
        /// containers there are, and nothing is written; where a diagram
        /// does not draw, that is said instead.
        #[arg(long, value_name = "NAME", conflicts_with = "check")]
        fold: Vec<String>,
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
    /// `export` folder unless --output names another; the project's map,
    /// made from its diagrams, is `index.svg`. Each tile of it and each state that opens a
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
        /// Fold these containers to their box in every diagram that holds
        /// them, as `render --fold` does; the map is drawn whole. A name
        /// no diagram holds stops the export before anything is written;
        /// where a diagram does not draw, that is said instead, and the
        /// rest is exported.
        #[arg(long, value_name = "NAME")]
        fold: Vec<String>,
        /// Print a JSON report on stdout instead of status lines.
        #[arg(long)]
        json: bool,
        /// Instead of SVGs: the project as one page, its map, a click on
        /// a tile or a linked state zooming into what it opens and back
        /// out, every diagram the map leads to inside. `-o` names the page
        /// where it ends in `.html`, else the folder it goes into.
        #[arg(long, conflicts_with = "json")]
        html: bool,
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

/// What a trace is written as, instead of checked.
#[derive(Clone, Copy, ValueEnum)]
enum TraceAs {
    /// A PlantUML timing diagram.
    Timing,
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatChoice {
    Svg,
    Png,
    Jpeg,
    Pdf,
    Markdown,
    Zip,
    /// A presentation: one page that plays the diagram.
    Html,
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
            FormatChoice::Html => Format::Html,
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
    // A panic the engine catches and answers for (mmdr's, on a diagram
    // or a line it fails on) is said where it is answered, not here too.
    let said = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !omaspec_render::caught::quiet() {
            said(info);
        }
    }));
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
            fix,
            project,
        }) => commands::lint(&files, lang.get(), json, strict, fix, project.as_deref()),
        Some(Command::Check {
            dir,
            json,
            strict,
            github,
        }) => check_command::check(&dir, json, strict, github),
        Some(Command::Orbit { file, output }) => orbit_command::run(&file, output.as_deref()),
        Some(Command::Matrix { file, output, json }) => {
            matrix_command::run(&file, output.as_deref(), json)
        }
        Some(Command::Coverage {
            file,
            traces,
            output,
            json,
            min,
            river,
        }) => coverage_command::run(&coverage_command::Args {
            file: &file,
            traces: &traces,
            output: output.as_deref(),
            json,
            min,
            river,
        }),
        Some(Command::Trace { schema: true, .. }) => {
            print!("{}", omaspec_model::TRACE_SCHEMA);
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Trace {
            follow: true,
            files,
            json,
            strict,
            project,
            ..
        }) => match files.as_slice() {
            [] | [_] => trace_follow::trace(
                files.first().map(PathBuf::as_path),
                json,
                strict,
                project.as_deref(),
            ),
            _ => Err(anyhow::anyhow!("--follow follows one trace")),
        },
        Some(Command::Trace {
            files,
            json,
            project,
            as_kind: Some(TraceAs::Timing),
            output,
            ..
        }) => commands::trace_timing(&files, project.as_deref(), output.as_deref(), json),
        Some(Command::Trace {
            files,
            json,
            strict,
            project,
            ..
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
            markdown,
            project,
            output,
            lang,
        }) => commands::diff(
            &files,
            rev.as_deref(),
            lang.get(),
            (json, markdown, output.as_deref()),
            project.as_deref(),
        ),
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
            portable,
            pretty,
        }) => commands::fmt(
            &files,
            lang.get(),
            write,
            check,
            commands::Also { portable, pretty },
        ),
        Some(Command::Outline { file, lang, json }) => commands::outline(file, lang.get(), json),
        Some(Command::Inspect {
            file,
            lang,
            project,
        }) => commands::inspect(file, lang.get(), project.as_deref()),
        Some(Command::Preview { file, lang, follow }) => preview(&file, lang.get(), follow),
        Some(Command::Play {
            file,
            output,
            events,
            steps,
            width,
            fps,
            colors,
            lossy,
            fold,
            dive,
            build,
            lockstep,
            history,
            from,
            to,
            traffic,
            themes,
            ink,
            look,
            duration,
            hold,
            whole,
            theme,
            lang,
        }) => editor::moving::events(&events)
            .and_then(|events| Ok((events, folds::names(&fold)?)))
            .and_then(|(events, fold)| {
                editor::moving::play(&editor::moving::Moving {
                    file,
                    output,
                    events,
                    steps,
                    width,
                    fps,
                    folds: fold,
                    dives: dive,
                    builds: build,
                    lockstep,
                    history,
                    travel: from.map(|from| editor::moving::Travel { from, to }),
                    traffic,
                    themes,
                    ink,
                    look,
                    timing: editor::gifs::Timing { duration, hold },
                    whole,
                    theme,
                    text: None,
                    file_colors: false,
                    root: None,
                    forced: lang.get(),
                    renderer: None,
                    quality: editor::gifs::Quality { colors, lossy },
                })
            })
            .map(|wrote| {
                eprintln!("{wrote}");
                ExitCode::SUCCESS
            }),
        Some(Command::Render {
            files,
            output,
            format,
            scale,
            fit,
            min_font,
            text_as_paths,
            id_prefix,
            no_source,
            copy,
            look,
            fold,
            json,
            check,
            lang,
        }) => folds::names(&fold).and_then(|fold| {
            render_command::render(&render_command::RenderArgs {
                files: &files,
                output: output.as_deref(),
                format: format.and_then(FormatChoice::image),
                text: matches!(format, Some(FormatChoice::Text)),
                scale,
                fit,
                min_font,
                text_as_paths,
                id_prefix: id_prefix.as_deref(),
                source: !no_source,
                copy,
                look,
                folds: &fold,
                lang: lang.get(),
                json,
                check,
            })
        }),
        Some(Command::Export {
            dir,
            output,
            look,
            fold,
            json: _,
            html: true,
        }) => folds::names(&fold)
            .and_then(|fold| project_export::page(&dir, output.as_deref(), look, &fold)),
        Some(Command::Export {
            dir,
            output,
            look,
            fold,
            json,
            html: _,
        }) => folds::names(&fold)
            .and_then(|fold| project_export::command(&dir, output.as_deref(), look, &fold, json)),
        Some(Command::Mcp) => mcp::run(),
        Some(Command::Example { name, json }) => commands::example(name.as_deref(), json),
        Some(Command::Extract {
            image,
            output,
            force,
        }) => commands::extract(&image, output.as_deref(), force),
        Some(Command::Theme { file }) => commands::theme(file),
        #[cfg(target_os = "linux")]
        Some(Command::Sandbox { scratch, command }) => sandbox::run(&scratch, &command),
        #[cfg(not(target_os = "linux"))]
        Some(Command::Sandbox { .. }) => Err(anyhow::anyhow!(
            "the sandbox needs Landlock, which only Linux has"
        )),
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

fn preview(
    file: &std::path::Path,
    lang: Option<Language>,
    follow: Option<PathBuf>,
) -> anyhow::Result<ExitCode> {
    let path = preview::check(file)?;
    // Followed by its own path, as the window's watcher names it.
    let follow = follow.map(|trace| std::path::absolute(&trace).unwrap_or(trace));
    preview::run(path, lang, follow)
        .map_err(|e| anyhow::anyhow!("the preview window failed: {e}"))?;
    Ok(ExitCode::SUCCESS)
}
