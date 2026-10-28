//! Spike: is scrive (an iced 0.14 code editor) good enough for Omagram?
//!
//! Checks PlantUML highlighting from a `.sublime-syntax`, syntax and chrome
//! colors from the active Omarchy theme, lint findings as diagnostics, and the
//! cost of large files.
//!
//! Usage:
//!   spike-editor [FILE]                 open a window
//!   spike-editor --snapshot PNG [FILE]  render one frame headless
//!   spike-editor --bench LINES          time loading and linting a big file

use std::borrow::Cow;
use std::ops::Range;
use std::time::Instant;

use anyhow::{Context, Result};
use iced::widget::{column, container, row, text};
use iced::{Color, Element, Font, Length, Size, Subscription, Task, Theme};
use omagram_theme::{Palette, Rgb, Theme as OmarchyTheme, tokens};
use scrive_core::{Diagnostic, Severity, SyntaxDef, TokenTheme};
use scrive_iced::{CodeEditor, Event};

const GRAMMAR: &str =
    include_str!("../../../crates/omagram-app/assets/syntax/plantuml.sublime-syntax");
const SAMPLE: &str = include_str!("../sample.puml");

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, png, rest @ ..] if flag == "--snapshot" => snapshot(png, rest.first()),
        [flag, lines] if flag == "--bench" => bench(lines.parse()?),
        [flag, ..] if flag.starts_with("--") => anyhow::bail!("unknown option {flag}"),
        rest => window(rest.first()),
    }
}

struct Spike {
    editor: CodeEditor,
    theme: Theme,
    palette: Palette,
    theme_name: String,
    file: String,
    findings: usize,
    font: Font,
}

#[derive(Debug, Clone)]
enum Message {
    Editor(Event),
}

impl Spike {
    fn new(file: String, source: String) -> Result<Self> {
        let mut omarchy = OmarchyTheme::load_current();
        // SPIKE_COLORS=/usr/share/omarchy/themes/<name>/colors.toml tries another theme.
        if let Some(path) = std::env::var_os("SPIKE_COLORS") {
            let path = std::path::PathBuf::from(path);
            omarchy.palette = Palette::load(&path)?;
            omarchy.name = path
                .parent()
                .and_then(|d| d.file_name())
                .map(|n| n.to_string_lossy().into_owned());
        }
        let palette = omarchy.palette.clone();
        let grammar = SyntaxDef::from_sublime_syntax(GRAMMAR).context("PlantUML grammar")?;
        let tokens = TokenTheme::from_tm_theme(&tokens::tm_theme(&palette, &Default::default()))
            .context("token theme")?;
        let font = Font::with_name(mono_family());
        let editor = CodeEditor::new(source)
            .theme(tokens)
            .language(grammar)
            .font(font)
            .text_size(14.0);
        let mut spike = Self {
            editor,
            theme: iced_theme(&palette),
            palette,
            theme_name: omarchy.name.unwrap_or_else(|| "fallback".into()),
            file,
            findings: 0,
            font,
        };
        spike.relint();
        Ok(spike)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let Message::Editor(event) = message;
        let task = self.editor.update(event).map(Message::Editor);
        if self.editor.take_dirty() {
            self.relint();
        }
        task
    }

    fn relint(&mut self) {
        let doc = self.editor.document();
        let revision = doc.revision();
        let findings = lint(&doc.text());
        self.findings = findings.len();
        let _ = self.editor.set_diagnostics(revision, findings);
    }

    fn view(&self) -> Element<'_, Message> {
        let p = &self.palette;
        let bar = |content: Element<'static, Message>| {
            let bg = color(p.dark_background);
            container(content)
                .width(Length::Fill)
                .padding([4, 12])
                .style(move |_: &Theme| container::Style::default().background(bg))
        };
        let label = |s: String, c: Rgb| text(s).font(self.font).size(13).color(color(c));

        let top = row![
            label("󰈸 Omagram".into(), p.accent),
            label(format!("  {}", self.file), p.foreground),
            label("  PlantUML".into(), p.dark_foreground),
        ];
        let cursor = self.editor.cursor();
        let lint = if self.findings == 0 {
            label("✓ no findings".into(), p.green)
        } else {
            label(
                format!(
                    "● {} finding{}",
                    self.findings,
                    if self.findings == 1 { "" } else { "s" }
                ),
                p.yellow,
            )
        };
        let status = row![
            label(
                format!("{}:{}  ", cursor.row + 1, cursor.col + 1),
                p.foreground
            ),
            lint,
            label(format!("  {}", self.theme_name), p.dark_foreground),
            iced::widget::space::horizontal(),
            label(
                "ctrl+f find  ctrl+z undo  ctrl+/ comment".into(),
                p.dark_foreground
            ),
        ];
        column![
            bar(top.into()),
            self.editor.view().map(Message::Editor),
            bar(status.into()),
        ]
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        self.editor.subscription().map(Message::Editor)
    }
}

fn window(file: Option<&String>) -> Result<()> {
    let (name, source) = load(file)?;
    let spike = Spike::new(name, source)?;
    let mut app = iced::application(
        move || {
            Spike::new(
                spike.file.clone(),
                spike.editor.document().text().into_owned(),
            )
            .expect("loaded once")
        },
        Spike::update,
        Spike::view,
    )
    .title("Omagram editor spike")
    .theme(|s: &Spike| s.theme.clone())
    .subscription(Spike::subscription)
    .window(iced::window::Settings {
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: "omagram".into(),
            ..Default::default()
        },
        ..Default::default()
    });
    for font in scrive_iced::required_fonts() {
        app = app.font(*font);
    }
    app.run()?;
    Ok(())
}

fn snapshot(png: &str, file: Option<&String>) -> Result<()> {
    let (name, source) = load(file)?;
    let spike = Spike::new(name, source)?;
    let settings = iced::Settings {
        fonts: scrive_iced::required_fonts()
            .iter()
            .map(|f| Cow::Borrowed(*f))
            .collect(),
        default_font: spike.font,
        ..Default::default()
    };
    let mut ui = iced_test::simulator::Simulator::with_size(
        settings,
        Size::new(1000.0, 640.0),
        spike.view(),
    );
    let shot = ui
        .snapshot(&spike.theme)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let _ = std::fs::remove_file(png);
    shot.matches_image(png)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!(
        "wrote {png} (theme {}, {} findings)",
        spike.theme_name, spike.findings
    );
    Ok(())
}

/// Times building the editor (grammar, theme, first highlight) and linting
/// for a document of `lines` lines, made from the sample repeated.
fn bench(lines: usize) -> Result<()> {
    let body: Vec<&str> = SAMPLE.lines().filter(|l| !l.starts_with('@')).collect();
    let mut source = String::from("@startuml\n");
    for line in body.iter().cycle().take(lines) {
        source.push_str(line);
        source.push('\n');
    }
    source.push_str("@enduml\n");

    let start = Instant::now();
    let spike = Spike::new("bench.puml".into(), source.clone())?;
    let build = start.elapsed();

    let start = Instant::now();
    let findings = lint(&source);
    let lint_time = start.elapsed();

    println!(
        "{lines} lines, {} KB: editor ready (grammar, theme, highlight seed, lint) {:.1} ms; lint alone {:.2} ms; {} findings",
        source.len() / 1024,
        build.as_secs_f64() * 1000.0,
        lint_time.as_secs_f64() * 1000.0,
        findings.len().max(spike.findings),
    );
    Ok(())
}

fn load(file: Option<&String>) -> Result<(String, String)> {
    match file {
        Some(path) => Ok((
            path.clone(),
            std::fs::read_to_string(path).with_context(|| path.clone())?,
        )),
        None => Ok(("sample.puml".into(), SAMPLE.into())),
    }
}

/// The family `monospace` resolves to (Omarchy sets it with `omarchy font set`).
fn mono_family() -> &'static str {
    let family = std::process::Command::new("fc-match")
        .args(["-f", "%{family[0]}", "monospace"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| "monospace".into());
    Box::leak(family.into_boxed_str())
}

fn color(c: Rgb) -> Color {
    Color::from_rgb8(c.r, c.g, c.b)
}

fn iced_theme(p: &Palette) -> Theme {
    Theme::custom(
        "Omarchy",
        iced::theme::Palette {
            background: color(p.background),
            text: color(p.foreground),
            primary: color(p.accent),
            success: color(p.green),
            warning: color(p.yellow),
            danger: color(p.red),
        },
    )
}

/// A few line-based checks, enough to see diagnostics in the editor. The real
/// lint comes in phase 1 (`omagram-core`).
fn lint(source: &str) -> Vec<Diagnostic> {
    let mut findings = Vec::new();
    let mut open: Vec<(String, Range<u32>)> = Vec::new();
    let mut start: Option<Range<u32>> = None;
    let mut ended = false;
    let mut in_note = false;
    let mut offset = 0u32;
    for line in source.split('\n') {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        let span = offset + indent as u32..offset + (indent + trimmed.len()) as u32;
        offset += line.len() as u32 + 1;
        let word = trimmed
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();

        if in_note {
            in_note = !(word == "end" && trimmed.to_ascii_lowercase().contains("note")
                || word == "endnote");
            continue;
        }
        match word.as_str() {
            w if w.starts_with("@start") => start = Some(span),
            w if w.starts_with("@end") => ended = true,
            "alt" | "opt" | "loop" | "par" | "break" | "critical" | "group" | "box" => {
                open.push((word.clone(), span));
            }
            "note" | "hnote" | "rnote" if !trimmed.contains(':') => in_note = true,
            "end" => match open.pop() {
                Some(_) => {}
                None => findings.push(Diagnostic::new(
                    span,
                    Severity::Error,
                    "`end` without an open block",
                )),
            },
            _ => {}
        }
    }
    for (word, span) in open {
        findings.push(Diagnostic::new(
            span,
            Severity::Warning,
            format!("`{word}` is never closed with `end`"),
        ));
    }
    if let (Some(span), false) = (start, ended) {
        findings.push(Diagnostic::new(span, Severity::Error, "missing @enduml"));
    }
    findings
}
