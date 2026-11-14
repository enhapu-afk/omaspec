//! One open document: its text in the editor widget, the file it belongs
//! to, its lint findings, outline and diagram.
//!
//! A Markdown document (`.md`) holds diagrams in fenced blocks
//! (`omaspec_core::markdown`): every one is linted and outlined on the
//! document's lines, and the pane shows the one at the caret. It is drawn
//! from the document with every other line empty, so whatever the engine,
//! a play or a Ctrl+click says about a line is the document's.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use iced::animation::Easing;
use iced::time::{Duration, Instant};
use iced::{Animation, Color, Font};
use omaspec_core::diff::{self, Change};
use omaspec_core::lint::{Finding, Severity};
use omaspec_core::markdown::{self, Block};
use omaspec_core::outline::{self, Entry};
use omaspec_core::{DiagramKind, Language, classify, detect};
use omaspec_model::Specs;
use omaspec_theme::{Palette, Tweaks, tokens};
use scrive_core::{Revision, TokenTheme};
use scrive_iced::{CodeEditor, LineMarker};

use super::syntax;
use crate::files;
use crate::paper::Paper;
use crate::ui;

/// What a new PlantUML document starts with.
const NEW_PLANTUML: &str = "@startuml\n\n@enduml\n";
const NEW_MERMAID: &str = "flowchart LR\n  A --> B\n";
/// How long the light on a line a jump landed on takes to go out.
const FLASH_FADE: Duration = Duration::from_millis(450);

/// How documents look: token colors, font, line marks. Follows the theme.
pub struct Look {
    pub tokens: TokenTheme,
    /// The colors of what a word is, over the grammar's.
    pub tones: tokens::Tones,
    pub font: Font,
    pub marks: MarkColors,
    /// The caret glides and the list of what could be written arrives;
    /// off, both stand in their place at once.
    pub motion: bool,
    /// How diagrams are drawn besides their files: JSON and YAML nested,
    /// state machines as tables.
    pub views: crate::worker::Views,
}

impl Look {
    /// From the theme's palette and its `omaspec.toml` tweaks.
    pub fn new(palette: &Palette, tweaks: &Tweaks, font: Font) -> Self {
        Self {
            tokens: syntax::token_theme(palette, &tweaks.syntax),
            tones: tokens::tones(palette, &tweaks.syntax),
            font,
            marks: MarkColors::new(palette, tweaks),
            motion: true,
            views: crate::worker::Views::default(),
        }
    }
}

/// Colors of the marks beside lines: the theme's accent for lines new or
/// changed since the last save (its "yellow" is red in some themes), its
/// blue for lines that differ from the version compared with, red where
/// lines are gone, and a short flash on the line a jump lands on.
#[derive(Debug, Clone, Copy)]
pub struct MarkColors {
    changed: Color,
    compared: Color,
    removed: Color,
}

impl MarkColors {
    pub fn new(palette: &Palette, tweaks: &Tweaks) -> Self {
        let pick =
            |name: &str, default| ui::color(tweaks.marks.get(name).copied().unwrap_or(default));
        Self {
            changed: pick("changed", palette.accent),
            compared: pick("compared", palette.blue),
            removed: pick("removed", palette.red),
        }
    }

    fn run(color: Color, rows: std::ops::Range<usize>) -> LineMarker {
        let row = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        LineMarker {
            rows: row(rows.start)..row(rows.end),
            color,
            wash: Some(Color { a: 0.05, ..color }),
        }
    }

    fn gone(self, at: usize) -> LineMarker {
        let at = u32::try_from(at).unwrap_or(u32::MAX);
        LineMarker {
            rows: at..at,
            color: self.removed,
            wash: None,
        }
    }

    /// The light on `row`, at `strength` from 0 (gone) to 1.
    fn flash(self, row: u32, strength: f32) -> LineMarker {
        LineMarker {
            rows: row..row + 1,
            color: Color {
                a: self.changed.a * strength,
                ..self.changed
            },
            wash: Some(Color {
                a: 0.25 * strength,
                ..self.changed
            }),
        }
    }

    fn marker(self, change: Change) -> LineMarker {
        match change {
            Change::Lines(lines) => Self::run(self.changed, lines),
            Change::Removed(at) => self.gone(at),
        }
    }
}

pub struct Doc {
    /// Names the document for alarms and render answers; never reused.
    pub key: u64,
    pub editor: CodeEditor,
    /// What the editor's list and its card under the pointer go by.
    assist: super::assist::Shared,
    /// The colors of what a word is.
    tones: tokens::Tones,
    /// Absolute; `None` until the document is saved.
    pub path: Option<PathBuf>,
    /// The text as last loaded or saved, to tell whether it changed since.
    saved: String,
    pub modified: bool,
    /// Whether `saved` is on disk; changed lines are marked only then.
    on_disk: bool,
    /// The language set by hand; otherwise it is detected while typing.
    pub forced: Option<Language>,
    pub language: Language,
    /// Lint's findings and the drawing's.
    pub findings: Vec<Finding>,
    /// What the last drawing said: entries of the lines by hand that did
    /// not hold.
    drawn_findings: Vec<Finding>,
    /// The kind of diagram, as of the last lint; `None` before it.
    pub kind: Option<DiagramKind>,
    /// The state machines of its project, which a sequence diagram is
    /// checked against.
    pub specs: Arc<Specs>,
    /// Its path in its project and the paths of the project's diagrams
    /// without their extensions, which the links it writes are held
    /// against; `None` outside a project.
    pub links: Option<(String, Arc<std::collections::HashSet<String>>)>,
    pub outline: Vec<Entry>,
    pub paper: Paper,
    /// The text last sent to the renderer.
    rendered: Option<Arc<str>>,
    marks: MarkColors,
    /// The version compared with (Compare A|B); while set, the marks show
    /// the difference to it instead of to the saved text.
    baseline: Option<String>,
    /// Marks for the lines changed since the last save, or against the
    /// baseline.
    changes: Vec<LineMarker>,
    /// The row a jump just landed on, lit for a moment.
    flash: Option<u32>,
    /// Set while that light goes out.
    flash_fade: Option<Animation<bool>>,
    /// In a Markdown document: the opening fence of the diagram shown.
    shown: Option<usize>,
    /// A Markdown document's diagrams as of a revision of its text, read
    /// again only when the text changed.
    blocks: RefCell<Option<(Revision, Arc<Vec<Block>>)>>,
    /// A project's file ([`Self::is_project`]): the folder of the project
    /// whose map it shows, by its path there; empty for the project's own.
    pub folder: String,
}

impl Doc {
    /// The document in `path`; a file that does not exist yet starts with
    /// the template for its extension and is created on the first save.
    pub fn open(key: u64, path: &Path, forced: Option<Language>, look: &Look) -> Result<Self> {
        let path = std::path::absolute(path)?;
        let on_disk = path.exists();
        let text = if on_disk {
            files::read_input(Some(&path))?.text
        } else {
            let hint = path
                .file_name()
                .and_then(|n| Language::from_file_name(&n.to_string_lossy()));
            template(forced.or(hint).unwrap_or(Language::PlantUml)).to_owned()
        };
        let mut doc = Self::new(key, Some(path), &text, forced, look);
        doc.on_disk = on_disk;
        Ok(doc)
    }

    /// A new, unsaved tab with `text` (the source found in an image, an
    /// example), to be saved as `path`, a name nothing has yet.
    pub fn unsaved_as(key: u64, path: PathBuf, text: &str, look: &Look) -> Self {
        let mut doc = Self::new(key, Some(path), text, None, look);
        doc.saved.clear();
        doc.modified = true;
        doc
    }

    /// An untitled tab holding `text`, unsaved.
    pub fn unsaved(key: u64, text: &str, look: &Look) -> Self {
        let mut doc = Self::new(key, None, text, None, look);
        doc.saved.clear();
        doc.modified = true;
        doc
    }

    pub fn untitled(key: u64, forced: Option<Language>, look: &Look) -> Self {
        let language = forced.unwrap_or(Language::PlantUml);
        let mut doc = Self::new(key, None, template(language), forced, look);
        // Between the markers, ready to type.
        if language == Language::PlantUml {
            let _ = doc
                .editor
                .update(scrive_iced::Event::Editor(scrive_iced::Action::PlaceCaret(
                    10,
                )));
        }
        doc
    }

    fn new(
        key: u64,
        path: Option<PathBuf>,
        text: &str,
        forced: Option<Language>,
        look: &Look,
    ) -> Self {
        let hint = path
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|n| Language::from_file_name(&n.to_string_lossy()));
        let document = path
            .as_deref()
            .and_then(Path::file_name)
            .is_some_and(|n| markdown::is_markdown(&n.to_string_lossy()));
        let language = if document {
            markdown::blocks(text)
                .first()
                .map_or(Language::PlantUml, |b| b.language)
        } else {
            forced.unwrap_or_else(|| detect(text, hint))
        };
        let editor = CodeEditor::new(text).theme(look.tokens.clone());
        let editor = if document {
            editor
                .language(syntax::document_grammar())
                .line_comment(None)
        } else {
            editor
                .language(syntax::grammar(language))
                .line_comment(Some(syntax::line_comment(language)))
        };
        let assist = super::assist::Shared::default();
        let editor = editor
            .completions(assist.offers())
            .hover(assist.says())
            .font(look.font)
            .text_size(14.0)
            .id(format!("editor-{key}"));
        let saved = editor.document().text().into_owned();
        let mut editor = editor;
        editor.set_motion(look.motion);
        let views = look.views;
        let mut doc = Self {
            key,
            editor,
            assist,
            tones: look.tones,
            path,
            saved,
            modified: false,
            on_disk: false,
            forced,
            language,
            findings: Vec::new(),
            drawn_findings: Vec::new(),
            kind: None,
            specs: Arc::default(),
            links: None,
            outline: Vec::new(),
            paper: Paper::editable(),
            rendered: None,
            marks: look.marks,
            baseline: None,
            changes: Vec::new(),
            flash: None,
            flash_fade: None,
            shown: None,
            blocks: RefCell::new(None),
            folder: String::new(),
        };
        doc.paper.views = views;
        doc.analyze();
        doc
    }

    /// What the tab holds, for what the editor offers while it is written.
    fn holds(&self) -> super::assist::Holds {
        use super::assist::Holds;
        if self.is_document() {
            Holds::Document
        } else if self.is_project() {
            Holds::Other
        } else {
            Holds::Diagram(self.language)
        }
    }

    /// Tells the editor's assistance where this file is in its project and
    /// which diagrams the project has, for the links it writes.
    pub fn tell_links(&self, own: &str, files: &Arc<Vec<String>>) {
        self.assist.tell_links(own, files);
    }

    /// Tells the editor's assistance the text, the language and the
    /// project's state machines as they are now, and colors the words by
    /// what they are.
    pub fn tell_assist(&mut self) {
        let holds = self.holds();
        let text = self.editor.document().text();
        self.assist.tell(&text, holds, &self.specs);
        let tints = super::assist::tints(&text, holds, &self.specs, &self.tones);
        drop(text);
        self.editor.set_tints(tints);
    }

    /// Whether this is a Markdown document of diagrams.
    pub fn is_document(&self) -> bool {
        markdown::is_markdown(&self.name())
    }

    /// Whether this is a project's file, `omaspec-project.toml`: beside
    /// it stands the project's map, which is made from the project's
    /// diagrams (docs/adr/0026), not drawn from this text.
    pub fn is_project(&self) -> bool {
        self.name() == omaspec_core::project::FILE
    }

    /// A Markdown document's diagrams; none for a diagram file. Read once
    /// for each revision of the text.
    pub fn blocks(&self) -> Arc<Vec<Block>> {
        if !self.is_document() {
            return Arc::default();
        }
        let revision = self.editor.document().revision();
        if let Some((read, blocks)) = &*self.blocks.borrow()
            && *read == revision
        {
            return Arc::clone(blocks);
        }
        let blocks = Arc::new(markdown::blocks(&self.text()));
        *self.blocks.borrow_mut() = Some((revision, Arc::clone(&blocks)));
        blocks
    }

    /// In a document, the diagram the pane shows and its number there,
    /// from 1: the caret's, the last above it, or the first.
    fn shown_block(&self) -> Option<(usize, Block)> {
        let blocks = self.blocks();
        let row = self.editor.cursor().row as usize;
        let at = markdown::block_at(&blocks, row)?;
        blocks.get(at).map(|block| (at + 1, block.clone()))
    }

    /// After the caret moved: whether a document shows another diagram
    /// now. Its language and kind become the document's.
    pub fn caret_moved(&mut self) -> bool {
        if !self.is_document() {
            return false;
        }
        let shown = self.shown_block();
        let start = shown.as_ref().map(|(_, block)| block.lines.start);
        if start == self.shown {
            return false;
        }
        self.shown = start;
        if let Some((_, block)) = &shown {
            self.language = block.language;
            self.kind = Some(classify(&block.code, block.language));
        }
        true
    }

    /// What the pane draws from: the text, a document's shown diagram on
    /// the document's lines with nothing else in them.
    pub fn shown_source(&self) -> String {
        let text = self.text();
        if !self.is_document() {
            return text;
        }
        self.shown_block()
            .map_or_else(String::new, |(_, block)| markdown::masked(&text, &block))
    }

    /// The diagram the pane shows as an image stores it: its code, its
    /// language and its file name, a document's numbered
    /// (`README-2.puml`); `None` for a document without one.
    pub fn shown_diagram(&self) -> Option<(String, Language, String)> {
        let text = self.text();
        if !self.is_document() {
            return Some((text, self.language, self.name()));
        }
        let (number, block) = self.shown_block()?;
        let name = self.name();
        let stem = name
            .rsplit_once('.')
            .map_or(name.as_str(), |(stem, _)| stem);
        Some((
            block.code,
            block.language,
            format!("{stem}-{number}.{}", block.language.extension()),
        ))
    }

    pub fn name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map_or_else(|| "untitled".into(), |n| n.to_string_lossy().into_owned())
    }

    pub fn text(&self) -> String {
        self.editor.document().text().into_owned()
    }

    /// Whether the file holds what was last loaded or saved; `false` for a
    /// name not written yet.
    pub fn on_disk(&self) -> bool {
        self.on_disk
    }

    /// Sets the language by hand, or back to detection with `None`; says
    /// whether the language changed.
    pub fn set_forced(&mut self, forced: Option<Language>) -> bool {
        self.forced = forced;
        self.analyze()
    }

    /// Re-indents the text (one undo step); says whether anything changed.
    /// The caret stays on its line.
    pub fn format(&mut self) -> bool {
        self.rewrite(omaspec_core::format::format)
    }

    /// Writes a state machine or a scenario evenly, arrows and colons in
    /// a column (one undo step); says whether anything changed.
    pub fn prettify(&mut self) -> bool {
        self.rewrite(omaspec_model::pretty)
    }

    /// Writes what only Omaspec reads in a Mermaid flowchart the way
    /// mermaid.js reads it too (one undo step); says whether anything
    /// changed.
    pub fn portable(&mut self) -> bool {
        self.rewrite(|code, language| match language {
            Language::Mermaid => omaspec_core::lint::portable(code),
            Language::PlantUml => code.to_owned(),
        })
    }

    /// Puts `rewritten` in the place of the text (one undo step); says
    /// whether anything changed. The caret stays on its line.
    fn rewrite(&mut self, rewritten: impl Fn(&str, Language) -> String) -> bool {
        let text = self.text();
        // A document's diagrams each in its place, its prose as it is.
        let formatted = if self.is_document() {
            markdown::rewrite_blocks(&text, |block| rewritten(&block.code, block.language))
        } else {
            rewritten(&text, self.language)
        };
        self.put(&text, formatted)
    }

    /// Puts `new` in the place of `text`, the text as it is (one undo
    /// step); says whether they differ. The caret stays on its line.
    fn put(&mut self, text: &str, new: String) -> bool {
        if new == text {
            return false;
        }
        let caret = self.editor.cursor();
        let len = self.editor.document().buffer().len();
        self.editor
            .edit(vec![scrive_core::EditOp::new(0..len, new)]);
        self.place_caret(caret.row as usize, caret.col as usize);
        true
    }

    /// Makes the fix a finding on the caret's line carries (one undo
    /// step), and says what it did; `None` when no finding there has one.
    /// The findings are read anew first, so a fix never goes by lines an
    /// edit has moved.
    pub fn fix_at_caret(&mut self) -> Option<String> {
        self.analyze();
        let row = self.editor.cursor().row as usize;
        let fix = self
            .findings
            .iter()
            .filter(|f| f.line == row)
            .find_map(|f| f.fix.clone())?;
        let text = self.text();
        self.put(&text, fix.apply(&text)).then_some(fix.title)
    }

    /// Makes every fix the findings carry (one undo step); how many.
    pub fn fix_all(&mut self) -> usize {
        self.analyze();
        let text = self.text();
        let (fixed, done) = omaspec_core::lint::fix_all(&text, &self.findings);
        if self.put(&text, fixed) { done } else { 0 }
    }

    /// After an edit: whether the text now differs from the saved one, and
    /// which lines.
    pub fn edited(&mut self) {
        self.tell_assist();
        let text = self.editor.document().text();
        self.modified = *text != *self.saved;
        self.changes = match &self.baseline {
            Some(baseline) => match super::compare::marks(baseline, &text) {
                Some((runs, gone)) => runs
                    .into_iter()
                    .map(|rows| MarkColors::run(self.marks.compared, rows))
                    .chain(gone.into_iter().map(|at| self.marks.gone(at)))
                    .collect(),
                None => Vec::new(),
            },
            None if self.modified && self.on_disk => diff::changes(&self.saved, &text)
                .into_iter()
                .map(|c| self.marks.marker(c))
                .collect(),
            None => Vec::new(),
        };
        drop(text);
        self.show_marks();
    }

    /// Marks the difference to `baseline` (a version compared with) instead
    /// of to the saved text, or back with `None`.
    pub fn set_baseline(&mut self, baseline: Option<String>) {
        self.baseline = baseline;
        self.edited();
    }

    /// Writes in `font` from now on.
    pub fn set_font(&mut self, font: Font) {
        let editor = std::mem::replace(&mut self.editor, CodeEditor::new(""));
        self.editor = editor.font(font);
    }

    /// Takes the theme's colors.
    pub fn set_look(&mut self, look: &Look) {
        self.editor.set_theme(look.tokens.clone());
        self.editor.set_motion(look.motion);
        self.paper.views = look.views;
        self.tones = look.tones;
        self.marks = look.marks;
        self.edited();
    }

    fn show_marks(&mut self) {
        self.show_marks_at(crate::clock::now());
    }

    /// The marks as they look at `now`, while the jump's light goes out.
    fn show_marks_at(&mut self, now: Instant) {
        let mut markers = self.changes.clone();
        let strength = self
            .flash_fade
            .as_ref()
            .map_or(1.0, |fade| fade.interpolate(0.0, 1.0, now));
        markers.extend(self.flash.map(|row| self.marks.flash(row, strength)));
        self.editor.set_line_markers(markers);
    }

    /// Puts the caret at the start of `line` (zero-based; after its
    /// indentation), scrolls it to the middle and lights it up until
    /// [`unflash`](Self::unflash).
    pub fn jump_to_line(&mut self, line: usize) {
        self.jump_to_line_at(line, 0);
    }

    /// Puts the caret on `line` (zero-based) at `column`, a byte on that
    /// line. A column that falls in the indentation lands just after it,
    /// where [`jump_to_line`](Self::jump_to_line) does. Scrolls the caret
    /// to the middle and lights the line until [`unflash`](Self::unflash).
    pub fn jump_to_line_at(&mut self, line: usize, column: usize) {
        let buffer = self.editor.document().buffer();
        let row = u32::try_from(line)
            .unwrap_or(u32::MAX)
            .min(buffer.line_count().saturating_sub(1));
        let text = buffer.line(row);
        let indent = text.len() - text.trim_start().len();
        let mut column = column.max(indent).min(text.len());
        while column > indent && !text.is_char_boundary(column) {
            column -= 1;
        }
        let column = u32::try_from(column).unwrap_or(u32::MAX);
        let offset = buffer.point_to_offset(scrive_core::Point::new(row, column));
        self.editor.jump_to(offset..offset);
        self.flash = Some(row);
        self.flash_fade = None;
        self.show_marks();
    }

    /// Selects `range` (bytes, clamped) in the middle of the view and
    /// lights its line up until [`unflash`](Self::unflash).
    pub fn show_range(&mut self, range: std::ops::Range<usize>) {
        let buffer = self.editor.document().buffer();
        let clamp = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).min(buffer.len());
        let (start, end) = (clamp(range.start), clamp(range.end));
        let row = buffer.offset_to_point(start).row;
        self.editor.jump_to(start..end);
        self.flash = Some(row);
        self.flash_fade = None;
        self.show_marks();
    }

    /// Replaces each range (bytes, disjoint, in order) with its text, as
    /// one undo step.
    pub fn replace(&mut self, edits: Vec<(std::ops::Range<usize>, String)>) {
        let clamp = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let ops = edits
            .into_iter()
            .map(|(range, text)| {
                scrive_core::EditOp::new(clamp(range.start)..clamp(range.end), text)
            })
            .collect();
        self.editor.edit(ops);
    }

    /// Lets the light on the jumped-to line go out;
    /// [`fade_flash`](Self::fade_flash) dims it frame by frame.
    pub fn unflash(&mut self) {
        if self.flash.is_some() && self.flash_fade.is_none() {
            self.flash_fade = Some(
                Animation::new(true)
                    .duration(FLASH_FADE)
                    .easing(Easing::EaseIn)
                    .go(false, crate::clock::now()),
            );
        }
    }

    /// Whether the light on the jumped-to line is going out.
    pub fn flash_fading(&self) -> bool {
        self.flash_fade.is_some()
    }

    /// Dims the light for `now`; takes it off once it is out.
    pub fn fade_flash(&mut self, now: Instant) {
        let Some(fade) = &self.flash_fade else {
            return;
        };
        if !fade.is_animating(now) {
            self.flash = None;
            self.flash_fade = None;
        }
        self.show_marks_at(now);
    }

    /// Puts `text` (unsaved, from the session) in place of the file's; undo
    /// brings the file's back.
    pub fn restore_text(&mut self, text: &str) {
        self.editor.load(text, None);
        let _ = self.editor.take_dirty();
        self.edited();
        self.analyze();
    }

    /// Puts the caret at `line`, `column` (zero-based, clamped) in the
    /// middle of the view.
    pub fn place_caret(&mut self, line: usize, column: usize) {
        let buffer = self.editor.document().buffer();
        let row = u32::try_from(line)
            .unwrap_or(u32::MAX)
            .min(buffer.line_count().saturating_sub(1));
        let column = u32::try_from(column)
            .unwrap_or(u32::MAX)
            .min(buffer.line_len(row));
        let offset = buffer.point_to_offset(scrive_core::Point::new(row, column));
        self.editor.jump_to(offset..offset);
    }

    /// The text for the session: `None` when the file on disk holds it.
    pub fn unsaved_text(&self) -> Option<String> {
        (self.modified || !self.on_disk).then(|| self.text())
    }

    /// An untouched new tab, which a file opened next may take the place
    /// of.
    pub fn is_blank(&self) -> bool {
        self.path.is_none() && !self.modified
    }

    /// The outline entry the caret is in: the last one at or above it.
    pub fn outline_at_caret(&self) -> Option<usize> {
        let row = self.editor.cursor().row as usize;
        self.outline.iter().rposition(|e| e.line <= row)
    }

    /// Lints the text, shows the findings in the editor and builds the
    /// outline. Detects the language again unless it is set by hand; says
    /// whether it changed.
    pub fn analyze(&mut self) -> bool {
        self.tell_assist();
        if self.is_document() {
            return self.analyze_document();
        }
        if self.is_project() {
            return self.analyze_project();
        }
        let text = self.text();
        let revision = self.editor.document().revision();
        let language = self.language_of(&text);
        let changed = language != self.language;
        if changed {
            self.set_language(language);
        }
        self.findings = omaspec_model::findings_with(&text, self.language, &self.specs);
        self.findings.extend(self.gone_links(&text, self.language));
        self.findings.extend(self.drawn_findings.iter().cloned());
        self.findings.sort_by_key(|f| f.line);
        self.kind = Some(omaspec_core::classify(&text, self.language));
        let _ = self
            .editor
            .set_diagnostics(revision, syntax::diagnostics(&text, &self.findings));
        let (entries, folds) = outline::outline_with_folds(&text, self.language);
        self.outline = entries;
        self.sync_fold_regions(&text, folds);
        changed
    }

    /// [`Self::analyze`] for a project's file: what does not read in it,
    /// on its line. It is no diagram, so it has no kind and no outline.
    fn analyze_project(&mut self) -> bool {
        let text = self.text();
        let revision = self.editor.document().revision();
        self.findings = match omaspec_core::project::Project::parse(&text) {
            Ok(_) => Vec::new(),
            Err(err) => vec![Finding::new(
                err.line.saturating_sub(1),
                Severity::Error,
                err.message,
            )],
        };
        self.kind = None;
        let _ = self
            .editor
            .set_diagnostics(revision, syntax::diagnostics(&text, &self.findings));
        self.outline = Vec::new();
        self.sync_fold_regions(&text, Vec::new());
        false
    }

    /// [`Self::analyze`] for a document: each diagram on the document's
    /// lines; the language and kind are the shown one's.
    fn analyze_document(&mut self) -> bool {
        let text = self.text();
        let revision = self.editor.document().revision();
        let blocks = self.blocks();
        let mut findings = Vec::new();
        let mut entries = Vec::new();
        let mut folds = Vec::new();
        for block in blocks.iter() {
            let masked = markdown::masked(&text, block);
            let mut found = omaspec_model::findings_with(&masked, block.language, &self.specs);
            // A line a fix puts in at the diagram's end goes before the
            // fence that closes the block, not to the document's end.
            let last = block.lines.end - usize::from(block.closed);
            for fix in found.iter_mut().filter_map(|f| f.fix.as_mut()) {
                fix.line = fix.line.min(last);
            }
            findings.extend(found);
            findings.extend(self.gone_links(&masked, block.language));
            let (more, more_folds) = outline::outline_with_folds(&masked, block.language);
            entries.extend(more);
            folds.extend(more_folds);
        }
        findings.extend(self.drawn_findings.iter().cloned());
        findings.sort_by_key(|f| f.line);
        self.findings = findings;
        let before = self.language;
        self.shown = None;
        self.caret_moved();
        if blocks.is_empty() {
            self.kind = None;
        }
        let _ = self
            .editor
            .set_diagnostics(revision, syntax::diagnostics(&text, &self.findings));
        self.outline = entries;
        self.sync_fold_regions(&text, folds);
        self.language != before
    }

    /// The links in `text` to diagrams its project does not have.
    fn gone_links(&self, text: &str, language: Language) -> Vec<Finding> {
        match &self.links {
            Some((own, has)) => {
                omaspec_model::gone_links(text, language, own, &|p| has.contains(p))
            }
            None => Vec::new(),
        }
    }

    /// The language of the text as it is now: the one set by hand, else
    /// the one detected. [`Doc::language`] follows it after a pause.
    pub fn current_language(&self) -> Language {
        self.language_of(&self.text())
    }

    fn language_of(&self, text: &str) -> Language {
        self.forced.unwrap_or_else(|| {
            let hint = Language::from_file_name(&self.name());
            detect(text, hint)
        })
    }

    /// Diagram blocks the gutter can fold, as byte offsets in the buffer.
    /// A region opens at the first character of its header that is not
    /// blank: an edit before it moves a collapsed fold's opener along, so
    /// both stay on the same byte and the fold survives the next resync.
    fn sync_fold_regions(&mut self, text: &str, ranges: Vec<(usize, usize)>) {
        let lines: Vec<&str> = text.split('\n').collect();
        let regions = {
            let buffer = self.editor.document().buffer();
            let mut regions = Vec::with_capacity(ranges.len());
            for (header, last) in ranges {
                let Ok(header) = u32::try_from(header) else {
                    continue;
                };
                let Ok(last) = u32::try_from(last) else {
                    continue;
                };
                if last >= buffer.line_count() {
                    continue;
                }
                let indent = lines
                    .get(header as usize)
                    .map_or(0, |line| line.len() - line.trim_start().len());
                let indent = u32::try_from(indent).unwrap_or(0);
                let open = buffer.point_to_offset(scrive_core::Point::new(header, indent));
                let close =
                    buffer.point_to_offset(scrive_core::Point::new(last, buffer.line_len(last)));
                if close > open {
                    regions.push((open, close));
                }
            }
            regions
        };
        self.editor.set_fold_regions(regions);
    }

    /// Highlights as `language`; the text and its undo history stay.
    fn set_language(&mut self, language: Language) {
        self.language = language;
        let editor = std::mem::replace(&mut self.editor, CodeEditor::new(""));
        self.editor = editor
            .language(syntax::grammar(language))
            .line_comment(Some(syntax::line_comment(language)));
    }

    /// The text to render, unless it was sent already: a document's shown
    /// diagram on the document's lines, nothing else in them.
    pub fn source_if_new(&mut self) -> Option<Arc<str>> {
        let text: Arc<str> = if self.is_document() {
            self.caret_moved();
            let text = self.text();
            let shown = self.shown_block();
            shown
                .map_or_else(String::new, |(_, block)| markdown::masked(&text, &block))
                .into()
        } else {
            self.editor.document().text().into()
        };
        if self.rendered.as_deref() == Some(&*text) {
            return None;
        }
        self.rendered = Some(Arc::clone(&text));
        Some(text)
    }

    /// The next [`source_if_new`](Self::source_if_new) sends the text
    /// again, as it was not drawn in the current colors.
    pub fn redraw_later(&mut self) {
        self.rendered = None;
    }

    /// The text to render even if it was sent already.
    pub fn source_anyway(&mut self) -> Arc<str> {
        self.rendered = None;
        self.source_if_new().expect("nothing was sent")
    }

    /// Writes the document to `path` (atomically, with the line endings it
    /// was loaded with); it belongs to that file from now on.
    pub fn save(&mut self, path: &Path) -> Result<()> {
        let doc = self.editor.document();
        let bytes = doc.serialize(doc.buffer().eol_flavor());
        files::write_atomic(path, bytes.as_bytes())?;
        self.saved = doc.text().into_owned();
        self.modified = false;
        self.on_disk = true;
        self.changes.clear();
        self.show_marks();
        self.path = Some(path.to_owned());
        Ok(())
    }

    /// Whether `text` (from disk) is what was last saved or loaded.
    pub fn is_saved_text(&self, text: &str) -> bool {
        normalize(text) == self.saved
    }

    /// Replaces the text with `text` from disk. Undo brings the old back.
    pub fn reload(&mut self, text: &str) {
        self.editor.load(text, None);
        self.saved = self.editor.document().text().into_owned();
        self.modified = false;
        self.changes.clear();
        self.flash = None;
        self.flash_fade = None;
        self.show_marks();
        let _ = self.editor.take_dirty();
    }

    /// Puts what the last drawing said among the findings: entries of the
    /// lines by hand that did not hold, each on its line.
    pub fn set_drawn_findings(&mut self, found: Vec<Finding>) {
        if found == self.drawn_findings {
            return;
        }
        let old = std::mem::replace(&mut self.drawn_findings, found);
        self.findings.retain(|f| !old.contains(f));
        self.findings.extend(self.drawn_findings.iter().cloned());
        self.findings.sort_by_key(|f| f.line);
        let text = self.text();
        let revision = self.editor.document().revision();
        let _ = self
            .editor
            .set_diagnostics(revision, syntax::diagnostics(&text, &self.findings));
    }

    /// Error and warning counts.
    pub fn counts(&self) -> (usize, usize) {
        let errors = self
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count();
        (errors, self.findings.len() - errors)
    }
}

impl Doc {
    /// Puts the template of `language` in place of the whole text, as one
    /// edit that Ctrl+Z takes back.
    pub fn start_from(&mut self, language: Language) {
        let end = self.text().len();
        self.replace(vec![(0..end, template(language).to_owned())]);
    }
}

fn template(language: Language) -> &'static str {
    match language {
        Language::PlantUml => NEW_PLANTUML,
        Language::Mermaid => NEW_MERMAID,
    }
}

/// Line endings as the editor keeps them.
fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}
