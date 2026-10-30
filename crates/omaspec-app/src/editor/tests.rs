use omaspec_core::lint::Severity;
use scrive_iced::Action;

use super::session::{SavedDoc, Session};
use super::*;
use crate::paper::testing::{answer, drawn};
use crate::specs;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("omaspec-ed-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn open(files: &[PathBuf]) -> App {
    let start = Start {
        files: files.to_vec(),
        ..Start::default()
    };
    App::new(start, None).0
}

/// Applies an editor action to the active document, as typing would.
fn act(app: &mut App, action: Action) {
    let key = app.doc().key;
    let _ = app.update(Message::Editor(key, scrive_iced::Event::Editor(action)));
}

fn paste(app: &mut App, text: &str) {
    act(
        app,
        Action::Paste {
            text: text.into(),
            entire_line: false,
        },
    );
}

fn select_all_and_paste(app: &mut App, text: &str) {
    act(app, Action::SelectAll);
    paste(app, text);
}

fn due(app: &mut App, alarm: Alarm) {
    let _ = app.update(Message::Worker(worker::Event::Due(alarm)));
}

fn press(app: &mut App, shortcut: Command) {
    let _ = app.update(Message::Command(shortcut));
}

#[test]
fn a_play_takes_digits_only_while_the_diagram_has_the_keys() {
    let c = |s: &str| Key::Character(s.into());
    let none = Modifiers::empty();
    let named = |n: Named| Key::Named(n);
    assert_eq!(
        shortcut(&c("2"), none, Mode::Playing),
        Some(Command::PlayAsk(paper::Ask::Take(1)))
    );
    assert_eq!(shortcut(&c("2"), none, Mode::Editing), None);
    assert_eq!(
        shortcut(&named(Named::Space), none, Mode::Playing),
        Some(Command::PlayAsk(paper::Ask::Next))
    );
    assert_eq!(shortcut(&named(Named::Space), none, Mode::Editing), None);
    assert_eq!(
        shortcut(&named(Named::Escape), none, Mode::Playing),
        Some(Command::StopPlay)
    );
    assert_eq!(
        shortcut(&named(Named::F5), none, Mode::Editing),
        Some(Command::Play)
    );
    assert_eq!(shortcut(&named(Named::F5), none, Mode::Picker), None);

    // The play holds the keys while the diagram has the focus.
    let work = temp_dir("play-keys");
    let file = work.join("lamp.puml");
    let lamp = "@startuml\n[*] --> Off\nOff --> On : switch\nOn --> Off : switch\n@enduml\n";
    std::fs::write(&file, lamp).unwrap();
    let mut app = open(std::slice::from_ref(&file));
    show(&mut app, lamp, Language::PlantUml);
    let _ = app.update(Message::Command(Command::Play));
    assert_eq!(app.mode(app.main), Mode::Playing);
    // Once the start's light sets off, its line lights up in the source.
    app.doc_mut().paper.freeze_play(Duration::from_secs(2));
    let _ = app.update(Message::Frame);
    assert_eq!(app.doc().editor.cursor().row, 1);
    app.set_focus(Pane::Source);
    assert_eq!(app.mode(app.main), Mode::Editing);
    app.set_focus(Pane::Diagram);
    let _ = app.update(Message::Command(Command::PlayAsk(paper::Ask::Take(0))));
    // The step's line lights up when its light sets off.
    app.doc_mut().paper.freeze_play(Duration::from_secs(10));
    let _ = app.update(Message::Frame);
    assert_eq!(app.doc().editor.cursor().row, 2);
    let _ = app.update(Message::Command(Command::Play));
    assert!(!app.doc().paper.playing());
}

#[test]
fn a_recorded_trace_is_found_beside_its_machine_and_played() {
    let work = temp_dir("play-trace");
    let file = work.join("lamp.puml");
    let lamp = "@startuml\n[*] --> Off\nOff --> On : switch\nOn --> Off : switch\n@enduml\n";
    std::fs::write(&file, lamp).unwrap();
    std::fs::create_dir_all(work.join("logs")).unwrap();
    std::fs::write(
        work.join("logs/night.jsonl"),
        "{\"object\":\"Lamp\",\"event\":\"switch\"}\n{\"object\":\"hall\",\"machine\":\"lamp\",\"event\":\"switch\"}\n{\"object\":\"Fan\",\"event\":\"spin\"}\n",
    )
    .unwrap();
    let mut app = open(std::slice::from_ref(&file));
    show(&mut app, lamp, Language::PlantUml);
    let key = app.doc().key;
    let spec = app.doc().paper.spec("lamp.puml").unwrap();
    let found = specs::traces(&work, &spec);
    let labels: Vec<&str> = found.iter().map(|(label, _, _)| label.as_str()).collect();
    assert_eq!(
        labels,
        ["Lamp in logs/night.jsonl", "hall in logs/night.jsonl"]
    );

    // Two objects: the writer picks one.
    let _ = app.update(Message::Traces(key, found.clone()));
    assert_eq!(app.picker.as_ref().unwrap().kind, Kind::Traces);
    let _ = app.update(Message::PickerSubmit);
    assert!(app.doc().paper.playing(), "{:?}", app.notice);
    let _ = app.update(Message::Command(Command::PlayAsk(paper::Ask::Next)));
    // The step's line lights up in the source when its light sets off.
    app.doc_mut().paper.freeze_play(Duration::from_secs(10));
    let _ = app.update(Message::Frame);
    assert_eq!(app.doc().editor.cursor().row, 2);
    let _ = app.update(Message::Command(Command::StopPlay));

    // One object: it plays at once. None: the writer is told.
    let _ = app.update(Message::Traces(key, found[1..].to_vec()));
    assert!(app.picker.is_none());
    assert!(app.doc().paper.playing(), "{:?}", app.notice);
    let _ = app.update(Message::Traces(key, Vec::new()));
    assert!(app.notice.as_deref().unwrap().starts_with("no trace"));
}

#[test]
fn keys_map_to_shortcuts() {
    let ctrl = Modifiers::CTRL;
    let c = |s: &str| Key::Character(s.into());
    assert_eq!(shortcut(&c("s"), ctrl, Mode::Editing), Some(Command::Save));
    assert_eq!(
        shortcut(&c("S"), ctrl | Modifiers::SHIFT, Mode::Editing),
        Some(Command::SaveAs)
    );
    assert_eq!(
        shortcut(&Key::Named(Named::Enter), ctrl, Mode::Editing),
        Some(Command::RenderNow)
    );
    assert_eq!(
        shortcut(&c("0"), Modifiers::ALT, Mode::Editing),
        Some(Command::ActualSize)
    );
    // Plain typing, the editor's own chords and AltGr stay with the editor.
    assert_eq!(shortcut(&c("s"), Modifiers::empty(), Mode::Editing), None);
    assert_eq!(shortcut(&c("z"), ctrl, Mode::Editing), None);
    assert_eq!(
        shortcut(&c("s"), ctrl | Modifiers::ALT, Mode::Editing),
        None
    );
    // Super+F belongs to Omarchy's window fullscreen. Ctrl+Alt+F is
    // not an editor command.
    assert_eq!(
        shortcut(&c("f"), ctrl | Modifiers::ALT, Mode::Editing),
        None
    );
    assert_eq!(shortcut(&c("f"), Modifiers::LOGO, Mode::Editing), None);
    assert_eq!(
        shortcut(
            &c("f"),
            Modifiers::LOGO | Modifiers::CTRL | Modifiers::ALT,
            Mode::Editing
        ),
        None
    );
    assert_eq!(
        shortcut(&c("f"), Modifiers::ALT, Mode::Editing),
        Some(Command::Fit)
    );
    assert_eq!(shortcut(&c("f"), Modifiers::LOGO, Mode::Picker), None);
    assert_eq!(
        shortcut(
            &Key::Named(Named::ArrowLeft),
            ctrl | Modifiers::ALT,
            Mode::Editing
        ),
        Some(Command::Focus(pane_grid::Direction::Left))
    );
    assert_eq!(
        shortcut(
            &Key::Named(Named::Escape),
            Modifiers::empty(),
            Mode::Editing
        ),
        None
    );
    assert_eq!(
        shortcut(&Key::Named(Named::Escape), Modifiers::empty(), Mode::Prompt),
        Some(Command::Cancel)
    );
    assert_eq!(shortcut(&c("w"), ctrl, Mode::Prompt), None);
    assert_eq!(shortcut(&c("s"), ctrl, Mode::Prompt), None);
    assert_eq!(shortcut(&c("q"), ctrl, Mode::Prompt), None);
    assert_eq!(
        shortcut(&Key::Named(Named::PageDown), ctrl, Mode::Prompt),
        None
    );
    assert_eq!(
        shortcut(&Key::Named(Named::Enter), ctrl, Mode::Prompt),
        Some(Command::RenderNow)
    );
}

#[test]
fn the_command_list_opens_in_the_window_that_asked() {
    let mut app = open(&[]);
    press(&mut app, Command::Detach);
    let diagram = app.detached.expect("diagram window");
    let _ = app.update(Message::Keyed(diagram, Command::Palette));
    assert_eq!(app.picker_on, Some(diagram));
    assert!(app.picker.is_some());
    assert_eq!(app.mode(diagram), Mode::Picker);
    assert_eq!(app.mode(app.main), Mode::Editing);

    // Choosing another list keeps it in the diagram window.
    let _ = app.update(Message::PickerInput("find in all tabs".into()));
    let _ = app.update(Message::PickerSubmit);
    assert_eq!(app.picker_on, Some(diagram));
    assert_eq!(
        app.picker.as_ref().map(|picker| picker.kind),
        Some(Kind::Search)
    );

    let _ = app.update(Message::Keyed(diagram, Command::Cancel));
    assert!(app.picker.is_none());
    assert_eq!(app.picker_on, None);

    // A click on the editor's status line still opens the list there.
    press(&mut app, Command::Palette);
    assert_eq!(app.picker_on, Some(app.main));

    // Closing the diagram window brings an open list back with it.
    let _ = app.update(Message::Keyed(diagram, Command::Palette));
    assert_eq!(app.picker_on, Some(diagram));
    let _ = app.update(Message::Keyed(diagram, Command::Detach));
    assert_eq!(app.detached, None);
    assert_eq!(app.picker_on, Some(app.main));
    assert!(app.picker.is_some());
}

/// Later tabs stay inside a narrow window: the bar scrolls instead of
/// growing past the frame.
#[test]
fn tabs_stay_inside_a_narrow_window() {
    let mut app = open(&[]);
    for _ in 0..24 {
        press(&mut app, Command::NewTab);
    }
    let width = 360.0;
    let settings = iced::Settings {
        default_font: app.font,
        fonts: scrive_iced::required_fonts()
            .iter()
            .map(|font| std::borrow::Cow::Borrowed(*font))
            .collect(),
        ..Default::default()
    };
    let mut ui = iced_test::simulator::Simulator::with_size(
        settings,
        Size::new(width, 640.0),
        app.main_view(),
    );
    let target = ui
        .find(iced_test::selector::id(TABS))
        .expect("the tab bar scrolls");
    let iced_test::selector::Target::Scrollable {
        bounds,
        content_bounds,
        ..
    } = target
    else {
        panic!("the tab bar is a scrollable, got {target:?}");
    };
    assert!(
        bounds.width <= width + 1.0,
        "tab bar is {} wide in a {width} window",
        bounds.width
    );
    assert!(
        content_bounds.width > bounds.width,
        "tabs ({}) should overflow the bar ({})",
        content_bounds.width,
        bounds.width
    );
    // The last tab lies past the bar's right end: revealing it scrolls
    // just far enough, and the first tab needs no scroll at all.
    let place = |ui: &mut iced_test::simulator::Simulator<'_, Message, _, _>, key| {
        ui.find(iced_test::selector::id(tab_id(key)))
            .expect("every tab has an id")
            .bounds()
    };
    let last = place(&mut ui, app.docs.last().expect("tabs").key);
    let first = place(&mut ui, app.docs[0].key);
    let left = last.x - content_bounds.x;
    assert_eq!(
        reveal_offset(bounds.width, 0.0, left, last.width),
        Some(left + last.width - bounds.width)
    );
    assert_eq!(
        reveal_offset(bounds.width, 0.0, first.x - content_bounds.x, first.width),
        None
    );
    // A downward notch brings the tabs on the right into view.
    ui.point_at(iced::Point::new(
        bounds.x + 8.0,
        bounds.y + bounds.height / 2.0,
    ));
    let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
        delta: iced::mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
    })]);
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::ScrollTabs(x) if x > 0.0))
    );
}

#[test]
fn revealing_a_tab_scrolls_the_least() {
    // Already whole in the bar: no scroll.
    assert_eq!(reveal_offset(300.0, 100.0, 150.0, 80.0), None);
    // Cut off on the left: its left edge goes to the bar's.
    assert_eq!(reveal_offset(300.0, 100.0, 60.0, 80.0), Some(60.0));
    // Cut off on the right: its right edge goes to the bar's.
    assert_eq!(reveal_offset(300.0, 100.0, 350.0, 80.0), Some(130.0));
}

#[test]
fn starts_with_an_empty_plantuml_diagram() {
    let app = open(&[]);
    assert_eq!(app.docs.len(), 1);
    let doc = app.doc();
    assert_eq!(doc.name(), "untitled");
    assert_eq!(doc.language, Language::PlantUml);
    assert_eq!(doc.text(), "@startuml\n\n@enduml\n");
    assert_eq!(doc.editor.cursor().row, 1);
    assert!(!doc.modified && doc.findings.is_empty());
}

#[test]
fn findings_follow_the_text_and_the_language_is_detected() {
    let mut app = open(&[]);
    select_all_and_paste(&mut app, "@startuml\nalt ok\nA -> B\n");
    assert!(app.doc().modified);
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    assert!(
        app.doc()
            .findings
            .iter()
            .any(|f| f.line == 1 && f.severity == Severity::Warning)
    );

    select_all_and_paste(&mut app, "sequenceDiagram\n  Alice->>Bob: hi\n");
    due(&mut app, Alarm::Lint(key));
    assert_eq!(app.doc().language, Language::Mermaid);
    assert!(app.doc().findings.is_empty());
}

#[test]
fn saves_atomically_with_the_line_endings_of_the_file() {
    let dir = temp_dir("save");
    let file = dir.join("a.puml");
    std::fs::write(&file, "@startuml\r\nA -> B\r\n@enduml\r\n").unwrap();
    let mut app = open(std::slice::from_ref(&file));
    act(
        &mut app,
        Action::Move {
            motion: scrive_core::Motion::DocEnd,
            extend: false,
        },
    );
    paste(&mut app, "' end\n");
    assert!(app.doc().modified);
    press(&mut app, Command::Save);
    assert!(!app.doc().modified);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "@startuml\r\nA -> B\r\n@enduml\r\n' end\r\n"
    );
    // No temporary file is left behind.
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn marks_lines_edited_since_the_last_save() {
    let dir = temp_dir("marks");
    let file = dir.join("a.puml");
    std::fs::write(&file, "@startuml\nA -> B\nB -> C\n@enduml\n").unwrap();
    let mut app = open(std::slice::from_ref(&file));
    let rows = |app: &App| -> Vec<(u32, u32)> {
        app.doc()
            .editor
            .line_markers()
            .iter()
            .map(|m| (m.rows.start, m.rows.end))
            .collect()
    };
    assert!(rows(&app).is_empty());

    // A new line after line 1, and line 3 ("B -> C") deleted.
    act(&mut app, Action::PlaceCaret(10));
    paste(&mut app, "' new\n");
    act(&mut app, Action::PlaceCaret(23));
    act(&mut app, Action::DeleteLine);
    assert_eq!(app.doc().text(), "@startuml\n' new\nA -> B\n@enduml\n");
    assert_eq!(rows(&app), [(1, 2), (3, 3)]);

    // Typing it back as it was clears the marks; so does saving.
    act(&mut app, Action::PlaceCaret(23));
    paste(&mut app, "B -> C\n");
    assert_eq!(rows(&app), [(1, 2)]);
    press(&mut app, Command::Save);
    assert!(rows(&app).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_outline_follows_the_caret_and_jumps() {
    let mut app = open(&[]);
    select_all_and_paste(
        &mut app,
        "@startuml\nparticipant A\nparticipant B\nalt ok\n    A -> B\nelse no\n    B -> A\nend\n@enduml\n",
    );
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    let labels: Vec<(usize, &str)> = app
        .doc()
        .outline
        .iter()
        .map(|e| (e.line, e.label.as_str()))
        .collect();
    assert_eq!(labels, [(1, "A"), (2, "B"), (3, "ok"), (5, "no")]);

    // Jumping lands after the indentation and lights the line up.
    let _ = app.update(Message::JumpTo(6));
    let cursor = app.doc().editor.cursor();
    assert_eq!((cursor.row, cursor.col), (6, 4));
    assert_eq!(app.doc().outline_at_caret(), Some(3));
    // Never saved, so no line counts as changed; only the jump is lit.
    let marked = |app: &App| -> Vec<std::ops::Range<u32>> {
        let markers = app.doc().editor.line_markers();
        markers.iter().map(|m| m.rows.clone()).collect()
    };
    assert_eq!(marked(&app), vec![6..7]);
    // The light goes out over a few frames, then the mark is gone.
    due(&mut app, Alarm::Flash(key));
    assert!(app.animating(Instant::now()));
    app.doc_mut()
        .fade_flash(Instant::now() + Duration::from_millis(200));
    let wash = app.doc().editor.line_markers()[0].wash.unwrap().a;
    assert!(wash > 0.0 && wash < 0.25, "{wash}");
    app.doc_mut()
        .fade_flash(Instant::now() + Duration::from_secs(1));
    assert!(marked(&app).is_empty());
    assert!(!app.animating(Instant::now() + Duration::from_secs(1)));

    // Above the first entry, none is current.
    let _ = app.update(Message::JumpTo(0));
    assert_eq!(app.doc().outline_at_caret(), None);
    // A line past the end goes to the last one.
    let _ = app.update(Message::JumpTo(99));
    assert_eq!(app.doc().editor.cursor().row, 9);
}

#[test]
fn the_sidebar_hides_and_comes_back_left_of_the_source() {
    let mut app = open(&[]);
    assert_eq!(app.panes.len(), 3);
    let files = app.pane(Pane::Files).unwrap();
    let source = app.pane(Pane::Source).unwrap();
    assert_eq!(
        app.panes.adjacent(files, pane_grid::Direction::Right),
        Some(source)
    );
    assert_eq!(
        app.panes.adjacent(source, pane_grid::Direction::Left),
        Some(files)
    );
    assert_eq!(app.panes.adjacent(files, pane_grid::Direction::Down), None);

    press(&mut app, Command::ToggleSidebar);
    assert_eq!(app.panes.len(), 2);
    assert!(app.pane(Pane::Files).is_none());

    press(&mut app, Command::ToggleSidebar);
    let files = app.pane(Pane::Files).unwrap();
    let source = app.pane(Pane::Source).unwrap();
    assert_eq!(
        app.panes.adjacent(files, pane_grid::Direction::Right),
        Some(source)
    );
    assert_eq!(
        app.panes.adjacent(source, pane_grid::Direction::Left),
        Some(files)
    );
}

#[test]
fn arrows_move_between_the_areas() {
    let mut app = open(&[]);
    assert_eq!(app.focus, Pane::Source);
    press(&mut app, Command::Focus(pane_grid::Direction::Left));
    assert_eq!(app.focus, Pane::Files);
    // Nothing sits under the project tree, so the focus stays.
    press(&mut app, Command::Focus(pane_grid::Direction::Down));
    assert_eq!(app.focus, Pane::Files);
    press(&mut app, Command::Focus(pane_grid::Direction::Right));
    assert_eq!(app.focus, Pane::Source);
    press(&mut app, Command::Focus(pane_grid::Direction::Right));
    assert_eq!(app.focus, Pane::Diagram);
    // Nothing sits above the diagram, so the focus stays.
    press(&mut app, Command::Focus(pane_grid::Direction::Up));
    assert_eq!(app.focus, Pane::Diagram);

    press(&mut app, Command::Focus(pane_grid::Direction::Left));
    assert_eq!(app.focus, Pane::Source);
    press(&mut app, Command::Focus(pane_grid::Direction::Left));
    assert_eq!(app.focus, Pane::Files);
    press(&mut app, Command::ToggleSidebar);
    assert_eq!(app.focus, Pane::Source);
    assert_eq!(app.panes.len(), 2);
}

#[test]
fn the_session_brings_back_tabs_unsaved_text_and_folder() {
    let dir = temp_dir("session");
    let (state, work) = (dir.join("state"), dir.join("work"));
    std::fs::create_dir_all(&work).unwrap();
    let (a, gone) = (work.join("a.puml"), work.join("gone.puml"));
    std::fs::write(&a, "@startuml\nA -> B\n@enduml\n").unwrap();
    std::fs::write(&gone, "@startuml\n@enduml\n").unwrap();

    let store = Store::at(&state).unwrap().unwrap();
    // The tab whose file goes away comes first, before the active one.
    let start = Start::new(vec![work.clone(), gone.clone(), a.clone()], None);
    let mut app = App::new(start, Some(store)).0;
    assert_eq!(app.folder.as_deref(), Some(work.as_path()));
    let _ = app.update(Message::Select(1));
    act(&mut app, Action::PlaceCaret(10));
    paste(&mut app, "' unsaved\n");
    press(&mut app, Command::NewTab);
    select_all_and_paste(&mut app, "flowchart LR\n  X --> Y\n");
    press(&mut app, Command::Quit);
    // Nothing is asked: the session keeps it all.
    assert!(!app.leaving);
    drop(app);
    std::fs::remove_file(&gone).unwrap();

    let store = Store::at(&state)
        .unwrap()
        .expect("the first window is gone");
    let app = App::new(Start::default(), Some(store)).0;
    let names: Vec<String> = app.docs.iter().map(Doc::name).collect();
    assert_eq!(names, ["a.puml", "untitled"]);
    assert_eq!(app.active, 1);
    assert_eq!(app.folder.as_deref(), Some(work.as_path()));
    let restored = &app.docs[0];
    assert_eq!(restored.text(), "@startuml\n' unsaved\nA -> B\n@enduml\n");
    assert!(restored.modified);
    assert_eq!(restored.editor.cursor().row, 2);
    assert_eq!(app.docs[1].text(), "flowchart LR\n  X --> Y\n");
    assert_eq!(app.docs[1].language, Language::Mermaid);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_picker_and_the_open_prompt_open_files_and_folders() {
    let dir = temp_dir("pick");
    for file in ["docs/flow.mmd", "auth/login.puml", "readme.md"] {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "flowchart LR\n  A --> B\n").unwrap();
    }
    let mut app = open(&[]);
    press(&mut app, Command::Open);
    assert_eq!(app.mode(app.main), Mode::Prompt);
    let _ = app.update(Message::PromptInput(dir.display().to_string()));
    let _ = app.update(Message::PromptSubmit);
    assert_eq!(app.folder.as_deref(), Some(dir.as_path()));
    // The tree is one level. The deep scan stays with the picker.
    let names: Vec<&str> = app.tree[&PathBuf::new()]
        .children
        .iter()
        .map(|child| child.name.as_str())
        .collect();
    assert_eq!(names, ["auth", "docs", "readme.md"]);
    let _ = app.update(Message::ToggleDir(PathBuf::from("docs")));
    let nested: Vec<&str> = app.tree[Path::new("docs")]
        .children
        .iter()
        .map(|child| child.name.as_str())
        .collect();
    assert_eq!(nested, ["flow.mmd"]);
    let _ = app.update(Message::Scanned(dir.clone(), folder::scan(&dir)));
    assert_eq!(app.files.len(), 2);

    press(&mut app, Command::Pick);
    assert_eq!(app.mode(app.main), Mode::Picker);
    let _ = app.update(Message::PickerInput("flw".into()));
    let _ = app.update(Message::PickerSubmit);
    assert_eq!(app.mode(app.main), Mode::Editing);
    // The untouched new tab gave way.
    assert_eq!(app.docs.len(), 1);
    assert_eq!(
        app.doc().path.as_deref(),
        Some(dir.join("docs/flow.mmd").as_path())
    );

    // Opening it again shows its tab; another file gets a new one.
    let _ = app.update(Message::Open(dir.join("docs/flow.mmd")));
    assert_eq!(app.docs.len(), 1);
    press(&mut app, Command::Open);
    let _ = app.update(Message::PromptInput(
        dir.join("auth/login.puml").display().to_string(),
    ));
    let _ = app.update(Message::PromptSubmit);
    assert_eq!((app.docs.len(), app.active), (2, 1));
    // Escape closes the picker without opening anything.
    press(&mut app, Command::Pick);
    press(&mut app, Command::Cancel);
    assert_eq!((app.mode(app.main), app.docs.len()), (Mode::Editing, 2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_palette_runs_commands() {
    let mut app = open(&[]);
    select_all_and_paste(&mut app, "@startuml\nalt ok\nA -> B\nend\n@enduml\n");
    press(&mut app, Command::Palette);
    assert_eq!(app.mode(app.main), Mode::Picker);
    let _ = app.update(Message::PickerInput("format".into()));
    let _ = app.update(Message::PickerSubmit);
    assert_eq!(app.mode(app.main), Mode::Editing);
    assert_eq!(
        app.doc().text(),
        "@startuml\nalt ok\n  A -> B\nend\n@enduml\n"
    );

    // Every command in the palette has its keys right.
    for &(label, keys, command) in PALETTE {
        if keys.is_empty() {
            continue;
        }
        let mut mods = Modifiers::empty();
        let mut key = Key::Character("".into());
        for part in keys.split('+').filter(|p| !p.is_empty()) {
            match part {
                "Ctrl" => mods |= Modifiers::CTRL,
                "Shift" => mods |= Modifiers::SHIFT,
                "Alt" => mods |= Modifiers::ALT,
                "Enter" => key = Key::Named(Named::Enter),
                "PageDown" => key = Key::Named(Named::PageDown),
                "PageUp" => key = Key::Named(Named::PageUp),
                "Left" => key = Key::Named(Named::ArrowLeft),
                "Right" => key = Key::Named(Named::ArrowRight),
                "Up" => key = Key::Named(Named::ArrowUp),
                "Down" => key = Key::Named(Named::ArrowDown),
                "F5" => key = Key::Named(Named::F5),
                c => key = Key::Character(c.to_lowercase().into()),
            }
        }
        if keys.ends_with("++") || keys.ends_with("+=") {
            key = Key::Character("=".into());
        }
        assert_eq!(
            shortcut(&key, mods, Mode::Editing),
            Some(command),
            "{label}"
        );
    }
}

#[test]
fn blocks_collapse_and_expand() {
    let mut app = open(&[]);
    select_all_and_paste(&mut app, "@startuml\nalt ok\nA -> B\nend\n@enduml\n");
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    let lines = app.doc().editor.document().buffer().line_count();
    let _ = app.update(Message::JumpTo(1));
    press(&mut app, Command::Collapse);
    let shown = app.doc().editor.document().fold_map().display_row_count();
    assert!(shown < lines, "collapsed {shown} of {lines}");
    press(&mut app, Command::Expand);
    assert_eq!(
        app.doc().editor.document().fold_map().display_row_count(),
        lines
    );
    press(&mut app, Command::CollapseAll);
    assert!(app.doc().editor.document().fold_map().display_row_count() < lines);
    press(&mut app, Command::ExpandAll);
    assert_eq!(
        app.doc().editor.document().fold_map().display_row_count(),
        lines
    );
}

#[test]
fn a_collapsed_block_survives_indenting_and_opens_for_a_jump() {
    let mut app = open(&[]);
    select_all_and_paste(
        &mut app,
        "@startuml\nparticipant A\nalt ok\nA -> B\nend\n@enduml\n",
    );
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    let rows = |app: &App| app.doc().editor.document().fold_map().display_row_count();
    let lines = app.doc().editor.document().buffer().line_count();
    // A declaration is not a block.
    let _ = app.update(Message::JumpTo(1));
    press(&mut app, Command::Collapse);
    assert_eq!(rows(&app), lines);
    let _ = app.update(Message::JumpTo(2));
    press(&mut app, Command::Collapse);
    // `alt ok ⋯ end` on one row.
    assert_eq!(rows(&app), lines - 2);

    let header = app
        .doc()
        .editor
        .document()
        .buffer()
        .point_to_offset(scrive_core::Point::new(2, 0));
    act(&mut app, Action::PlaceCaret(header));
    paste(&mut app, "  ");
    due(&mut app, Alarm::Lint(key));
    assert_eq!(rows(&app), lines - 2);

    let _ = app.update(Message::JumpTo(3));
    assert_eq!(rows(&app), lines);
}

#[test]
fn hidden_tabs_draw_in_the_new_colors_when_shown() {
    let mut app = open(&[]);
    press(&mut app, Command::NewTab);
    assert_eq!(app.active, 1);
    let _ = app.docs[0].source_if_new();
    assert!(app.docs[0].source_if_new().is_none());
    press(&mut app, Command::FollowTheme);
    assert!(app.docs[0].source_if_new().is_some());
}

#[test]
fn the_diagram_theme_follows_omarchy_without_editing_the_file() {
    let mut app = open(&[]);
    select_all_and_paste(&mut app, "@startuml\nA -> B\n@enduml\n");
    press(&mut app, Command::Skin(Some(Pack::Spec)));
    let file = app.doc().text();
    let user = file
        .lines()
        .position(|line| line.contains("A -> B"))
        .unwrap();
    press(&mut app, Command::FollowTheme);
    assert!(app.follow_theme);
    assert_eq!(app.doc().text(), file);
    assert!(app.session().follow_theme);
    let (shown, pending) = app.diagram_source(&file, Language::PlantUml);
    let pending = pending.expect("an overlay");
    assert!(shown.contains("skinparam backgroundColor"));
    assert!(!shown.contains("strictuml"));
    let rendered = shown
        .lines()
        .position(|line| line.contains("A -> B"))
        .unwrap();
    assert_eq!(pending.shift.file_line(rendered), Some(user));
    assert_eq!(pending.shift.file_line(pending.shift.at), None);
    press(&mut app, Command::FollowTheme);
    assert!(!app.follow_theme);
    let (shown, pending) = app.diagram_source(&file, Language::PlantUml);
    assert!(pending.is_none());
    assert_eq!(&*shown, &file);
}

#[test]
fn a_machine_plays_while_the_diagram_follows_the_theme() {
    // The theme's skin block moves the rendered lines; the map is moved
    // back onto the file with the rest of the answer, so a play finds every
    // state there.
    let mut app = open(&[]);
    let machine =
        "@startuml\n[*] --> State1\nState1 --> State2 : next step\nState2 --> [*]\n@enduml\n";
    select_all_and_paste(&mut app, machine);
    press(&mut app, Command::FollowTheme);
    let text = app.doc().text();
    let (shown, pending) = app.diagram_source(&text, Language::PlantUml);
    assert_ne!(&*shown, text.as_str());
    let key = app.doc().key;
    let _ = app.update(Message::Paper(
        key,
        paper::Message::Viewport(Size::new(600.0, 560.0)),
    ));
    let id = app.doc_mut().paper.pretend_next();
    app.pending.insert((key, id), pending.expect("an overlay"));
    let mut reply = answer(key, id, Ok(drawn(&shown, Language::PlantUml)));
    reply.language = Language::PlantUml;
    reply.source = shown;
    app.untangle(&mut reply);
    assert_eq!(&*reply.source, text.as_str());
    let state1 = text.lines().position(|l| l.contains("--> State1")).unwrap();
    let map = &reply.result.as_ref().unwrap().map;
    assert!(
        map.states
            .iter()
            .any(|s| s.name == "State1" && s.line == state1)
    );
    let _ = app.doc_mut().paper.rendered(reply);
    app.doc_mut().paper.finish_rasters();
    press(&mut app, Command::Play);
    assert_eq!(app.mode(app.main), Mode::Playing, "{:?}", app.notice);
}

#[test]
fn help_pages_draw() {
    let dir = temp_dir("help-draw");
    help::write_pages(&dir).unwrap();
    for page in ["menu.puml", "editing.puml", "windows.puml", "export.puml"] {
        let text = std::fs::read_to_string(dir.join(page)).unwrap();
        let _ = drawn(&text, Language::PlantUml);
    }
}

#[test]
fn help_is_left_for_the_project_it_came_from() {
    let project = temp_dir("help-project");
    std::fs::write(project.join("a.puml"), "@startuml\nA -> B\n@enduml\n").unwrap();
    let pages = temp_dir("help-pages");
    help::write_pages(&pages).unwrap();
    let mut app = open(&[project.join("a.puml")]);
    let _ = app.set_folder(Some(project.clone()));

    let _ = app.show_help(pages.clone());
    assert_eq!(app.folder.as_deref(), Some(pages.as_path()));
    assert_eq!(
        app.doc().path.as_deref(),
        Some(help::menu_file(&pages).as_path())
    );
    // The session keeps the project, not the visit.
    assert_eq!(app.session().folder.as_deref(), Some(project.as_path()));
    // A second visit still leads back to the project.
    let _ = app.show_help(pages.clone());
    assert!(
        app.palette_items()
            .iter()
            .any(|item| item.label == "Leave Help Omaspec")
    );

    press(&mut app, Command::LeaveHelp);
    assert_eq!(app.folder.as_deref(), Some(project.as_path()));
    assert!(app.help.is_none());
    let paths: Vec<_> = app.docs.iter().filter_map(|d| d.path.clone()).collect();
    assert_eq!(paths, [project.join("a.puml")]);
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&pages);
}

#[test]
fn the_area_frame_moves_with_the_focus() {
    let mut app = open(&[]);
    assert_eq!(app.focus, Pane::Source);
    press(&mut app, Command::Focus(pane_grid::Direction::Right));
    assert_eq!(app.focus, Pane::Diagram);
    assert_eq!(app.left_focus, Some(Pane::Source));
    assert!(app.animating(Instant::now()));
    assert!(!app.animating(Instant::now() + TAB_FADE));
}

#[test]
fn the_highlight_moves_to_the_chosen_tab() {
    let mut app = open(&[]);
    press(&mut app, Command::NewTab);
    let (first, second) = (app.docs[0].key, app.docs[1].key);
    let _ = app.update(Message::Select(0));
    assert_eq!(app.left_tab, Some(second));
    assert!(app.tab_fade.value());
    let later = Instant::now() + TAB_FADE;
    assert!(!app.tab_fade.is_animating(later));
    let _ = app.update(Message::HoverTab(1, true));
    let _ = app.update(Message::HoverTab(0, false));
    assert_eq!(app.hovered_tab, Some(1));
    let _ = app.update(Message::HoverTab(1, false));
    assert_eq!(app.hovered_tab, None);
    assert_eq!(app.doc().key, first);
}

#[test]
fn language_by_hand_and_back() {
    let mut app = open(&[]);
    press(&mut app, Command::LanguageMermaid);
    assert_eq!(
        (app.doc().language, app.doc().forced),
        (Language::Mermaid, Some(Language::Mermaid))
    );
    press(&mut app, Command::LanguageDetected);
    assert_eq!(
        (app.doc().language, app.doc().forced),
        (Language::PlantUml, None)
    );
}

#[test]
fn images_open_their_diagram_under_a_free_name() {
    use omaspec_render::export;

    let dir = temp_dir("image");
    let flow = "flowchart LR\n  A --> B\n";
    std::fs::write(dir.join("flow.mmd"), flow).unwrap();
    let svg = drawn(flow, Language::Mermaid).svg;
    let options = export::Options {
        scale: 1.0,
        source: Some(export::Source {
            code: flow,
            language: Language::Mermaid,
            file: Some("flow.mmd"),
        }),
    };
    let png = export::export(&svg, Format::Png, &options).unwrap();
    std::fs::write(dir.join("flow.png"), &png).unwrap();
    std::fs::write(
        dir.join("plain.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    )
    .unwrap();

    let mut app = open(&[]);
    let _ = app.update(Message::Open(dir.join("flow.png")));
    let doc = app.doc();
    assert_eq!(doc.path.as_deref(), Some(dir.join("flow-2.mmd").as_path()));
    assert_eq!(
        (doc.text(), doc.language),
        (flow.to_owned(), Language::Mermaid)
    );
    assert!(doc.modified && !doc.on_disk());
    // A second import takes the next name.
    let _ = app.update(Message::Open(dir.join("flow.png")));
    assert_eq!(app.doc().name(), "flow-3.mmd");

    // The name was taken meanwhile: saving asks first.
    std::fs::write(dir.join("flow-3.mmd"), "pie\n").unwrap();
    press(&mut app, Command::Save);
    assert!(app.prompt.as_ref().is_some_and(|p| p.overwrite));
    assert_eq!(
        std::fs::read_to_string(dir.join("flow-3.mmd")).unwrap(),
        "pie\n"
    );
    press(&mut app, Command::Cancel);
    let _ = app.update(Message::Select(0));
    press(&mut app, Command::Save);
    assert_eq!(
        std::fs::read_to_string(dir.join("flow-2.mmd")).unwrap(),
        flow
    );

    let _ = app.update(Message::Open(dir.join("plain.svg")));
    assert!(app.notice.as_ref().unwrap().contains("no diagram source"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_asks_for_a_path_and_refuses_the_source() {
    let dir = temp_dir("export-to");
    let src = dir.join("flow.puml");
    let text = "@startuml\nA -> B\n@enduml\n";
    std::fs::write(&src, text).unwrap();
    let mut app = open(std::slice::from_ref(&src));

    press(&mut app, Command::ExportSvg);
    let prompt = app.prompt.as_ref().unwrap();
    assert!(matches!(prompt.asking, Asking::Export(Format::Svg)));
    assert_eq!(prompt.input, dir.join("flow.svg").display().to_string());
    assert!(!dir.join("flow.svg").exists());

    // Enter starts the write and leaves the source file alone.
    let _ = app.update(Message::PromptSubmit);
    assert!(app.prompt.is_none());
    assert_eq!(std::fs::read_to_string(&src).unwrap(), text);

    // A file that is already there asks before it is replaced.
    std::fs::write(dir.join("flow.svg"), b"old").unwrap();
    press(&mut app, Command::ExportSvg);
    let _ = app.update(Message::PromptSubmit);
    assert!(app.prompt.as_ref().is_some_and(|prompt| prompt.overwrite));
    assert_eq!(std::fs::read(dir.join("flow.svg")).unwrap(), b"old");

    // A folder is not a file.
    let _ = app.update(Message::PromptInput(dir.display().to_string()));
    let _ = app.update(Message::PromptSubmit);
    assert!(app.notice.as_ref().unwrap().contains("folder"));

    // Markdown of a `.md` source must not offer that same path.
    let notes = dir.join("notes.md");
    std::fs::write(&notes, "flowchart LR\n  A --> B\n").unwrap();
    let _ = app.update(Message::Open(notes.clone()));
    press(&mut app, Command::ExportMarkdown);
    assert!(
        app.prompt
            .as_ref()
            .unwrap()
            .input
            .ends_with("notes-export.md")
    );
    let _ = app.update(Message::PromptInput(notes.display().to_string()));
    let _ = app.update(Message::PromptSubmit);
    assert!(app.notice.as_ref().unwrap().contains("source"));
    assert_eq!(
        std::fs::read_to_string(&notes).unwrap(),
        "flowchart LR\n  A --> B\n"
    );

    press(&mut app, Command::Cancel);
    press(&mut app, Command::NewTab);
    press(&mut app, Command::ExportPng);
    assert!(app.prompt.as_ref().unwrap().input.ends_with("untitled.png"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_names_keep_their_dots() {
    let path = |typed: &str, format| export_path(PathBuf::from(typed), format);
    assert_eq!(path("flow-1.2", Format::Svg), PathBuf::from("flow-1.2.svg"));
    assert_eq!(path("api.v2", Format::Png), PathBuf::from("api.v2.png"));
    assert_eq!(path("flow.jpeg", Format::Jpeg), PathBuf::from("flow.jpeg"));
    assert_eq!(path("flow.png", Format::Svg), PathBuf::from("flow.svg"));
    assert_eq!(path("flow", Format::Zip), PathBuf::from("flow.zip"));
}

/// Three tabs: two mention Bob, the active one first.
fn tabs_with_bob() -> App {
    let mut app = open(&[]);
    select_all_and_paste(&mut app, "@startuml\nAlice -> Bob : hi\n@enduml\n");
    press(&mut app, Command::NewTab);
    select_all_and_paste(&mut app, "@startuml\nCarol -> Dave\n@enduml\n");
    press(&mut app, Command::NewTab);
    select_all_and_paste(&mut app, "@startuml\nbob -> Carol\nBob -> Bob\n@enduml\n");
    app
}

#[test]
fn the_search_lists_hits_in_all_tabs_and_shows_them() {
    let mut app = tabs_with_bob();
    press(&mut app, Command::FindAll);
    assert_eq!(app.mode(app.main), Mode::Picker);
    let _ = app.update(Message::PickerInput("bob".into()));
    let picker = app.picker.as_ref().unwrap();
    let hints: Vec<&str> = picker
        .visible()
        .map(|(_, item)| item.hint.as_str())
        .collect();
    // This tab first, then the others in order; case counts only with a capital.
    assert_eq!(
        hints,
        ["untitled:2", "untitled:3", "untitled:3", "untitled:2"]
    );
    let _ = app.update(Message::PickerInput("Bob".into()));
    assert_eq!(app.picker.as_ref().unwrap().count(), 3);

    // The last hit is in the first tab: it comes up with the hit selected.
    let _ = app.update(Message::PickerClick(2));
    assert_eq!((app.mode(app.main), app.active), (Mode::Editing, 0));
    let selection = app.doc().editor.selection();
    assert_eq!(
        &app.doc().text()[selection.start as usize..selection.end as usize],
        "Bob"
    );
    assert_eq!(app.doc().editor.cursor().row, 1);
}

#[test]
fn replacing_in_all_tabs_is_undone_per_tab() {
    let mut app = tabs_with_bob();
    // The selection seeds the search.
    act(&mut app, Action::PlaceCaret(10));
    act(
        &mut app,
        Action::Move {
            motion: scrive_core::Motion::Right,
            extend: true,
        },
    );
    act(
        &mut app,
        Action::Move {
            motion: scrive_core::Motion::Right,
            extend: true,
        },
    );
    act(
        &mut app,
        Action::Move {
            motion: scrive_core::Motion::Right,
            extend: true,
        },
    );
    press(&mut app, Command::ReplaceAll);
    assert_eq!(app.picker.as_ref().unwrap().query, "bob");
    // Tab goes to the replacement and back.
    press(&mut app, Command::SwitchField);
    assert!(app.picker.as_ref().unwrap().on_replace);
    let _ = app.update(Message::ReplaceInput("Eve".into()));
    let _ = app.update(Message::ReplaceSubmit);
    assert_eq!(app.mode(app.main), Mode::Editing);
    assert!(
        app.notice
            .as_ref()
            .unwrap()
            .starts_with("replaced 4 in 2 tabs")
    );
    assert_eq!(
        app.docs[0].text(),
        "@startuml\nAlice -> Eve : hi\n@enduml\n"
    );
    assert_eq!(app.docs[1].text(), "@startuml\nCarol -> Dave\n@enduml\n");
    assert_eq!(
        app.docs[2].text(),
        "@startuml\nEve -> Carol\nEve -> Eve\n@enduml\n"
    );

    act(&mut app, Action::Undo);
    assert_eq!(
        app.docs[2].text(),
        "@startuml\nbob -> Carol\nBob -> Bob\n@enduml\n"
    );
    assert!(app.docs[0].text().contains("Eve"));
}

#[test]
fn ctrl_click_in_the_diagram_jumps_to_its_line() {
    let flow = "flowchart LR\n  %% Saved? is asked\n  A[Edit] --> B{Saved?}\n  B -->|yes| C[Render]\n  B -->|no| A\n";
    let mut app = open(&[]);
    select_all_and_paste(&mut app, flow);
    show(&mut app, flow, Language::Mermaid);
    let key = app.doc().key;
    for (label, line) in [("Render", 3), ("Saved?", 2), ("no", 4), ("Edit", 2)] {
        let at = app.doc().paper.screen_of(label);
        let _ = app.update(Message::Paper(key, paper::Message::Pick(at)));
        assert_eq!(app.doc().editor.cursor().row, line, "{label}");
    }
    // Nothing drawn there.
    let _ = app.update(Message::Paper(
        key,
        paper::Message::Pick(iced::Point::new(1.0, 1.0)),
    ));
    assert!(app.notice.as_ref().unwrap().starts_with("no label"));
}

#[test]
fn compare_with_another_tab_follows_both() {
    let mut app = open(&[]);
    select_all_and_paste(&mut app, "@startuml\nA -> B : one\nB -> C : two\n@enduml\n");
    press(&mut app, Command::NewTab);
    select_all_and_paste(
        &mut app,
        "@startuml\nA -> B : one\nB -> C : TWO\nC -> D\n@enduml\n",
    );
    let (b, a) = (app.docs[0].key, app.docs[1].key);

    press(&mut app, Command::Compare);
    assert_eq!(app.picker.as_ref().unwrap().kind, Kind::Versions);
    let _ = app.update(Message::PickerSubmit);
    let c = app.compare.as_ref().expect("comparing");
    assert_eq!((c.a, &c.target), (a, &Target::Tab(b)));
    assert!(c.rows.as_ref().unwrap().contains(&compare::Row::Added {
        line: 3,
        text: "C -> D".into()
    }));
    // The gutter marks the difference to B, not to the saved text.
    let marked: Vec<std::ops::Range<u32>> = app
        .doc()
        .editor
        .line_markers()
        .iter()
        .map(|m| m.rows.clone())
        .collect();
    assert_eq!(marked, vec![2..4]);

    // B follows its tab, the list follows A.
    let _ = app.update(Message::Select(0));
    select_all_and_paste(&mut app, "@startuml\nA -> B : one\nB -> C : TWO\n@enduml\n");
    due(&mut app, Alarm::Render(b));
    let _ = app.update(Message::Select(1));
    let rows = app.compare.as_ref().unwrap().rows.clone().unwrap();
    assert_eq!(
        rows.iter()
            .filter(|r| !matches!(r, compare::Row::Same { .. }))
            .count(),
        1
    );

    // Zooming A zooms B; the diff toggles; closing B stops comparing.
    let _ = app.update(Message::Paper(
        a,
        paper::Message::Viewport(Size::new(400.0, 300.0)),
    ));
    let _ = app.update(Message::Paper(
        a,
        paper::Message::ZoomAt(2.0, iced::Point::ORIGIN),
    ));
    let zoom = app.doc().paper.view.zoom;
    assert_eq!(app.compare.as_ref().unwrap().paper.view.zoom, zoom);
    press(&mut app, Command::CompareDiff);
    assert!(app.compare.as_ref().unwrap().show_diff);
    // B has unsaved changes: the second ask closes it.
    let _ = app.update(Message::Close(0));
    let _ = app.update(Message::Close(0));
    assert!(app.compare.is_none());
    assert!(app.doc().editor.line_markers().is_empty());
}

#[test]
fn compare_with_a_commit() {
    let mut app = open(&[]);
    select_all_and_paste(&mut app, "flowchart LR\n  A --> C\n");
    let key = app.doc().key;
    let target = Target::Commit {
        rev: "abc1234".into(),
        label: "git abc1234".into(),
    };
    let old = "flowchart LR\n  A --> B\n".to_owned();
    let _ = app.update(Message::Version(key, target.clone(), Ok(old)));
    let c = app.compare.as_ref().unwrap();
    assert_eq!(
        (c.title.as_str(), c.language),
        ("git abc1234", Language::Mermaid)
    );
    press(&mut app, Command::StopCompare);
    assert!(app.compare.is_none());
    let _ = app.update(Message::Version(key, target, Err("no such file".into())));
    assert!(app.notice.as_ref().unwrap().starts_with("cannot compare"));
}

#[test]
fn the_diagram_gets_its_own_window_and_comes_back() {
    let mut app = open(&[]);
    let main = app.main;
    let _ = app.update(Message::ScaleFactor(main, 2.0));
    press(&mut app, Command::Detach);
    let detached = app.detached.expect("a second window");
    assert!(app.pane(Pane::Diagram).is_none());
    assert!(app.title(detached).ends_with("· diagram"));
    // The diagram takes the scale of the window it is in.
    let _ = app.update(Message::ScaleFactor(detached, 1.0));
    assert_eq!(app.paper_scale(), 1.0);
    let _ = app.update(Message::ScaleFactor(main, 1.5));
    assert_eq!(app.paper_scale(), 1.0);

    // Closing that window brings the diagram back beside the source.
    let _ = app.update(Message::CloseRequested(detached));
    assert!(app.detached.is_none() && app.pane(Pane::Diagram).is_some());
    assert_eq!(app.paper_scale(), 1.5);
    assert_eq!(app.panes.len(), 3);
    // Closing the editor window still quits (here: nothing unsaved).
    press(&mut app, Command::Detach);
    press(&mut app, Command::Detach);
    assert!(app.detached.is_none());
}

#[test]
fn skin_packs_go_in_and_out_as_one_step() {
    let mut app = open(&[]);
    let start = app.doc().text();
    press(&mut app, Command::Skin(Some(Pack::Spec)));
    assert!(app.doc().text().contains("skinparam style strictuml"));
    press(&mut app, Command::Skin(Some(Pack::Omarchy)));
    let text = app.doc().text();
    let background = format!("skinparam backgroundColor {}", app.palette.background);
    assert!(text.contains(&background) && !text.contains("strictuml"));
    press(&mut app, Command::Skin(Some(Pack::Omarchy)));
    assert!(app.notice.as_ref().unwrap().contains("in already"));
    act(&mut app, Action::Undo);
    assert!(app.doc().text().contains("strictuml"));
    press(&mut app, Command::Skin(None));
    assert_eq!(app.doc().text(), start);
}

#[test]
fn a_new_font_reaches_every_tab_and_new_ones() {
    let mut app = open(&[]);
    press(&mut app, Command::NewTab);
    let font = Font::with_name("Omaspec Test Mono");
    app.set_font(font);
    assert_eq!((app.font, app.look.font), (font, font));
    press(&mut app, Command::NewTab);
    assert_eq!(app.docs.len(), 3);
}

#[test]
fn closing_the_other_tabs_keeps_unsaved_ones() {
    let mut app = open(&[]);
    press(&mut app, Command::NewTab);
    paste(&mut app, "' changed\n");
    press(&mut app, Command::NewTab);
    press(&mut app, Command::NewTab);
    press(&mut app, Command::CloseOthers);
    assert_eq!((app.docs.len(), app.active), (2, 1));
    assert!(app.docs[0].modified);
}

#[test]
fn folders_on_the_command_line_set_the_folder() {
    let dir = temp_dir("start");
    let file = dir.join("a.puml");
    let start = Start::new(vec![file.clone(), dir.clone()], None);
    assert_eq!(start.files, [file]);
    assert_eq!(start.folder, Some(dir.clone()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_as_asks_before_overwriting_another_file() {
    let dir = temp_dir("save-as");
    let other = dir.join("other.mmd");
    std::fs::write(&other, "pie\n").unwrap();
    let mut app = open(&[]);
    press(&mut app, Command::Save);
    let prompt = app
        .prompt
        .as_ref()
        .expect("an untitled diagram asks for a name");
    assert!(prompt.input.ends_with("/untitled.puml"));

    let _ = app.update(Message::PromptInput(other.display().to_string()));
    let _ = app.update(Message::PromptSubmit);
    assert_eq!(std::fs::read_to_string(&other).unwrap(), "pie\n");
    assert!(app.prompt.as_ref().unwrap().overwrite);
    let _ = app.update(Message::PromptSubmit);
    assert!(app.prompt.is_none());
    assert_eq!(
        std::fs::read_to_string(&other).unwrap(),
        "@startuml\n\n@enduml\n"
    );
    assert_eq!(app.doc().path.as_deref(), Some(other.as_path()));
    assert_eq!(app.doc().name(), "other.mmd");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_as_keeps_its_tab_when_close_is_asked() {
    let dir = temp_dir("save-as-tab");
    let path = dir.join("out.puml");
    let mut app = open(&[]);
    select_all_and_paste(&mut app, "TEXT-A\n");
    press(&mut app, Command::NewTab);
    select_all_and_paste(&mut app, "TEXT-B\n");
    press(&mut app, Command::SaveAs);
    assert!(app.prompt.is_some());
    let _ = app.update(Message::PromptInput(path.display().to_string()));

    press(&mut app, Command::CloseTab);
    press(&mut app, Command::CloseTab);
    assert!(app.prompt.is_some());
    assert_eq!(app.docs.len(), 2);

    let _ = app.update(Message::PromptSubmit);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "TEXT-B\n");
    assert!(app.docs.iter().any(|doc| doc.text() == "TEXT-A\n"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn closing_the_save_as_tab_does_not_write_another() {
    let dir = temp_dir("save-as-closed");
    let path = dir.join("out.puml");
    let mut app = open(&[]);
    select_all_and_paste(&mut app, "TEXT-A\n");
    press(&mut app, Command::NewTab);
    select_all_and_paste(&mut app, "TEXT-B\n");
    press(&mut app, Command::SaveAs);
    let _ = app.update(Message::PromptInput(path.display().to_string()));
    let index = app.active;

    let _ = app.update(Message::Close(index));
    assert!(app.prompt.is_some());
    let _ = app.update(Message::Close(index));
    assert!(app.prompt.is_none());
    let _ = app.update(Message::PromptSubmit);
    assert!(!path.exists());
    assert_eq!(app.doc().text(), "TEXT-A\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restore_keeps_unsaved_text_when_the_file_cannot_be_read() {
    let dir = temp_dir("restore-unreadable");
    let path = dir.join("diagram.puml");
    std::fs::create_dir(&path).unwrap();
    let state = dir.join("state");
    let store = Store::at(&state).unwrap().unwrap();
    store
        .save(&Session {
            docs: vec![SavedDoc {
                path: Some(path.clone()),
                text: Some("@startuml\nkept\n@enduml\n".into()),
                language: None,
                line: 1,
                column: 0,
            }],
            ..Session::default()
        })
        .unwrap();
    let app = App::new(Start::default(), Some(store)).0;
    assert!(app.doc().text().contains("kept"), "{}", app.doc().text());
    assert!(!app.doc().on_disk());
    assert_eq!(app.doc().path.as_deref(), Some(path.as_path()));
    let notice = app.notice.as_deref().unwrap_or("");
    assert!(notice.contains("diagram.puml"), "{notice}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unsaved_changes_need_a_second_ask() {
    let mut app = open(&[]);
    press(&mut app, Command::NewTab);
    assert_eq!((app.docs.len(), app.active), (2, 1));
    paste(&mut app, "A -> B\n");

    press(&mut app, Command::CloseTab);
    assert_eq!(app.docs.len(), 2);
    assert!(app.notice.as_ref().unwrap().contains("unsaved"));
    press(&mut app, Command::CloseTab);
    assert_eq!((app.docs.len(), app.active), (1, 0));

    paste(&mut app, "A -> B\n");
    press(&mut app, Command::Quit);
    assert!(app.leaving);
    // Typing on cancels the leaving.
    paste(&mut app, "B -> A\n");
    assert!(!app.leaving);

    // The last tab closed gives way to an empty one.
    press(&mut app, Command::CloseTab);
    press(&mut app, Command::CloseTab);
    assert_eq!(app.docs.len(), 1);
    assert!(!app.doc().modified);
}

#[test]
fn changes_on_disk_reload_clean_documents_only() {
    let dir = temp_dir("disk");
    let (a, b) = (dir.join("a.puml"), dir.join("b.puml"));
    std::fs::write(&a, "@startuml\nA -> B\n@enduml\n").unwrap();
    std::fs::write(&b, "@startuml\nC -> D\n@enduml\n").unwrap();
    let mut app = open(&[a.clone(), b.clone()]);
    let _ = app.update(Message::Select(1));
    paste(&mut app, "' mine\n");

    // Our own save is no change.
    let _ = app.update(Message::Select(0));
    press(&mut app, Command::Save);
    let _ = app.update(Message::Worker(worker::Event::FileChanged(a.clone())));
    due(&mut app, Alarm::File);
    assert_eq!(
        app.notice.as_deref(),
        Some(format!("saved {}", a.display()).as_str())
    );

    std::fs::write(&a, "@startuml\nA -> C\n@enduml\n").unwrap();
    std::fs::write(&b, "@startuml\nC -> E\n@enduml\n").unwrap();
    for file in [&a, &b] {
        let _ = app.update(Message::Worker(worker::Event::FileChanged(file.clone())));
    }
    due(&mut app, Alarm::File);
    assert_eq!(app.docs[0].text(), "@startuml\nA -> C\n@enduml\n");
    assert!(!app.docs[0].modified);
    assert!(app.docs[1].text().contains("' mine") && app.docs[1].modified);
    let notice = app.notice.clone().unwrap();
    assert!(notice.contains("a.puml reloaded") && notice.contains("b.puml changed on disk"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn each_tab_keeps_its_own_diagram() {
    let mut app = open(&[]);
    press(&mut app, Command::NewTab);
    let (first, second) = (app.docs[0].key, app.docs[1].key);
    let _ = app.update(Message::Paper(
        second,
        paper::Message::Viewport(Size::new(600.0, 400.0)),
    ));
    app.docs[1].paper.pretend_requested(1);
    let flow = drawn("flowchart LR\n  A --> B\n", Language::Mermaid);
    let _ = app.update(Message::Worker(worker::Event::Rendered(Box::new(answer(
        second,
        1,
        Ok(flow),
    )))));
    assert!(app.docs[1].paper.diagram.is_some());
    assert!(app.docs[0].paper.diagram.is_none());

    // An answer for a closed tab goes nowhere.
    press(&mut app, Command::CloseTab);
    let late = answer(second, 2, Ok(drawn("pie\n  \"a\": 1\n", Language::Mermaid)));
    let _ = app.update(Message::Worker(worker::Event::Rendered(Box::new(late))));
    assert_eq!(app.doc().key, first);
    assert!(app.doc().paper.diagram.is_none());
}

#[test]
fn a_sequence_is_checked_against_the_state_machines_of_its_folder() {
    let work = temp_dir("specs");
    std::fs::write(
        work.join("door.puml"),
        "@startuml\n[*] --> Closed\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n",
    )
    .unwrap();
    let scenario = work.join("scenario.puml");
    std::fs::write(
        &scenario,
        "@startuml\nX -> Door : lock\nX -> Door : lock\n@enduml\n",
    )
    .unwrap();
    let mut app = open(std::slice::from_ref(&scenario));
    assert!(app.doc().findings.is_empty());
    // Without a folder in the sidebar, the file's own folder is its project.
    let root = app.project_of(app.doc()).unwrap();
    assert_eq!(root, std::path::absolute(&work).unwrap());
    // What the read in the background brings.
    let _ = app.update(Message::Specs(root.clone(), Arc::new(specs::read(&root))));
    let found: Vec<(usize, &str)> = app
        .doc()
        .findings
        .iter()
        .map(|f| (f.line, f.message.as_str()))
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, 2);
    assert!(
        found[0].1.contains("Door does not take lock here"),
        "{found:?}"
    );
}

/// Writes headless screenshots of a play when OMASPEC_SNAPSHOT_DIR
/// is set: a state machine at rest, a light on its way and arriving,
/// a scenario half unfolded.
#[test]
fn snapshot_play() {
    let Some(dir) = std::env::var_os("OMASPEC_SNAPSHOT_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let work = temp_dir("play");
    let door = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Opening : open [unlocked]\nClosed --> Locked : lock\nLocked --> Closed : unlock\nOpening --> Open : end position\nOpen --> Closing : close\nClosing --> Closed : end position\nClosing --> Opening : obstacle\n@enduml\n";
    let file = work.join("door.puml");
    std::fs::write(&file, door).unwrap();
    let mut app = open(std::slice::from_ref(&file));
    show(&mut app, door, Language::PlantUml);
    let _ = app.update(Message::Command(Command::Play));
    assert!(app.doc().paper.playing(), "{:?}", app.notice);
    app.doc_mut().paper.freeze_play(Duration::from_secs(3));
    shoot(&mut app, &dir, "play-machine");
    // Opening first, then on to Open: the light on its way, arriving.
    let _ = app.update(Message::Command(Command::PlayAsk(paper::Ask::Take(0))));
    std::thread::sleep(Duration::from_millis(600));
    let _ = app.update(Message::Command(Command::PlayAsk(paper::Ask::Take(0))));
    for (ms, name) in [(240, "play-machine-travel"), (560, "play-machine-arrive")] {
        app.doc_mut().paper.freeze_play(Duration::from_millis(ms));
        shoot(&mut app, &dir, name);
    }
    app.doc_mut().paper.freeze_play(Duration::from_secs(3));
    let _ = app.update(Message::Paper(
        app.doc().key,
        paper::Message::Play(paper::Ask::Hover(Some(0))),
    ));
    shoot(&mut app, &dir, "play-machine-hover");

    let composite = "@startuml\nscale 600 width\n\n[*] -> State1\nState1 --> State2 : Succeeded\nState1 --> [*] : Aborted\nState2 --> State3 : Succeeded\nState2 --> [*] : Aborted\nstate State3 {\n  state \"Accumulate Enough Data\\nLong State Name\" as long1\n  long1 : Just a test\n  [*] --> long1\n  long1 --> long1 : New Data\n  long1 --> ProcessData : Enough Data\n}\nState3 --> State3 : Failed\nState3 --> [*] : Succeeded / Save Result\nState3 --> [*] : Aborted\n\n@enduml\n";
    let file = work.join("composite.puml");
    std::fs::write(&file, composite).unwrap();
    let mut app = open(std::slice::from_ref(&file));
    show(&mut app, composite, Language::PlantUml);
    let _ = app.update(Message::Command(Command::Play));
    for _ in 0..2 {
        std::thread::sleep(Duration::from_millis(900));
        let _ = app.update(Message::Command(Command::PlayAsk(paper::Ask::Take(0))));
    }
    app.doc_mut().paper.freeze_play(Duration::from_secs(3));
    shoot(&mut app, &dir, "play-composite");

    let regions = "stateDiagram-v2\n    [*] --> Idle\n    Idle --> Busy : job\n    state Busy {\n        [*] --> Work\n        Work --> Check : done\n        --\n        [*] --> Watch\n        Watch --> Watch : tick\n    }\n    Busy --> Idle : stop\n    Idle --> [*] : off\n";
    let file = work.join("busy.mmd");
    std::fs::write(&file, regions).unwrap();
    let mut app = open(std::slice::from_ref(&file));
    show(&mut app, regions, Language::Mermaid);
    let _ = app.update(Message::Command(Command::Play));
    std::thread::sleep(Duration::from_millis(900));
    let _ = app.update(Message::Command(Command::PlayAsk(paper::Ask::Take(0))));
    // Into Busy, then both regions start at once.
    app.doc_mut().paper.freeze_play(Duration::from_millis(820));
    shoot(&mut app, &dir, "play-regions");
    app.doc_mut().paper.freeze_play(Duration::from_secs(3));
    shoot(&mut app, &dir, "play-regions-rest");

    let scenario = "@startuml\nactor Driver\nparticipant Door\nparticipant Car\nDriver -> Car : park\nCar -> Door : unlock\nDriver -> Door : open\nDoor --> Driver : opened\nDriver -> Door : close\nCar -> Door : lock\n@enduml\n";
    let file = work.join("leave.puml");
    std::fs::write(&file, scenario).unwrap();
    let mut app = open(std::slice::from_ref(&file));
    show(&mut app, scenario, Language::PlantUml);
    let _ = app.update(Message::Command(Command::Play));
    for _ in 0..3 {
        let _ = app.update(Message::Command(Command::PlayAsk(paper::Ask::Next)));
    }
    app.doc_mut().paper.freeze_play(Duration::from_millis(260));
    shoot(&mut app, &dir, "play-scenario");
    app.doc_mut().paper.freeze_play(Duration::from_secs(2));
    shoot(&mut app, &dir, "play-scenario-rest");

    // A recorded trace on the door: four lines it allows, then one it
    // does not.
    std::fs::write(
        work.join("night.jsonl"),
        [
            r#"{"time":"22:10:04","object":"Door","event":"lock"}"#,
            r#"{"time":"06:58:40","object":"Door","event":"unlock"}"#,
            r#"{"time":"07:02:11","object":"Door","event":"open","state":"Opening"}"#,
            r#"{"time":"07:02:15","object":"Door","event":"end position","state":"Open"}"#,
            r#"{"time":"07:05:30","object":"Door","event":"lock"}"#,
        ]
        .join("\n"),
    )
    .unwrap();
    let file = work.join("door.puml");
    let mut app = open(std::slice::from_ref(&file));
    show(&mut app, door, Language::PlantUml);
    let key = app.doc().key;
    let spec = app.doc().paper.spec("door.puml").unwrap();
    let _ = app.update(Message::Traces(key, specs::traces(&work, &spec)));
    assert!(app.doc().paper.playing(), "{:?}", app.notice);
    for _ in 0..4 {
        let _ = app.update(Message::Command(Command::PlayAsk(paper::Ask::Next)));
    }
    app.doc_mut().paper.freeze_play(Duration::from_secs(8));
    shoot(&mut app, &dir, "play-trace");
    let _ = app.update(Message::Command(Command::PlayAsk(paper::Ask::Next)));
    app.doc_mut().paper.freeze_play(Duration::from_secs(10));
    shoot(&mut app, &dir, "play-trace-wrong");
}

/// Writes headless screenshots when OMASPEC_SNAPSHOT_DIR is set:
/// `editor` with a sequence diagram and a finding, `editor-mermaid`.
#[test]
fn snapshot() {
    let Some(dir) = std::env::var_os("OMASPEC_SNAPSHOT_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let source = "@startuml\n' A login\nactor User\nparticipant \"Web app\" as App #lightblue\nUser -> App : log in\nalt password ok\n  App --> User : welcome\nelse wrong\n  App --> User : try again\n@enduml\n";
    let work = temp_dir("snapshot");
    let file = work.join("login.puml");
    std::fs::write(&file, source).unwrap();
    for other in ["docs/release.puml", "docs/flow.mmd", "states/player.puml"] {
        std::fs::create_dir_all(work.join(other).parent().unwrap()).unwrap();
        std::fs::write(work.join(other), "@startuml\n@enduml\n").unwrap();
    }
    let mut app = shown(&file);
    let _ = app.set_folder(Some(work.clone()));
    let _ = app.update(Message::Scanned(work.clone(), folder::scan(&work)));
    let _ = app.update(Message::Command(Command::NewTab));
    let flow = "flowchart LR\n  %% build and ship\n  A[Edit] --> B{Saved?}\n  B -->|yes| C[Render]\n  B -.->|no| A\n  classDef done fill:#9f9\n";
    select_all_and_paste(&mut app, flow);
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    show(&mut app, flow, Language::Mermaid);
    let _ = app.update(Message::Select(0));
    shoot(&mut app, &dir, "editor");
    let _ = app.update(Message::Select(1));
    show(&mut app, flow, Language::Mermaid);
    shoot(&mut app, &dir, "editor-mermaid");

    // A long diagram, jumped to an outline entry far down.
    let mut long = String::from("@startuml\nparticipant Client\nparticipant Server\n");
    for round in 1..=12 {
        long.push_str(&format!("== Round {round} ==\n"));
        long.push_str(&format!("loop retry {round}\n"));
        for _ in 0..6 {
            long.push_str("    Client -> Server : request\n    Server --> Client : answer\n");
        }
        long.push_str("end\n");
    }
    long.push_str("@enduml\n");
    let _ = app.update(Message::Command(Command::NewTab));
    select_all_and_paste(&mut app, &long);
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    let round9 = app
        .doc()
        .outline
        .iter()
        .find(|e| e.label == "Round 9")
        .unwrap()
        .line;
    let _ = app.update(Message::JumpTo(round9));
    shoot(&mut app, &dir, "editor-outline");

    let _ = app.update(Message::Command(Command::Pick));
    let _ = app.update(Message::PickerInput("pl".into()));
    shoot(&mut app, &dir, "editor-picker");

    let _ = app.update(Message::Command(Command::Cancel));
    let _ = app.update(Message::Command(Command::Palette));
    let _ = app.update(Message::PickerInput("diag".into()));
    shoot(&mut app, &dir, "editor-palette");

    let _ = app.update(Message::Command(Command::Cancel));
    let _ = app.update(Message::Command(Command::ReplaceAll));
    let _ = app.update(Message::PickerInput("client".into()));
    let _ = app.update(Message::ReplaceInput("Browser".into()));
    shoot(&mut app, &dir, "editor-search");

    // Help Omaspec in the sidebar, with the way back on top.
    let _ = app.update(Message::Command(Command::Cancel));
    let data = temp_dir("snapshot-help");
    let pages = data.join("omaspec").join(help::DIR_NAME);
    help::write_pages(&pages).unwrap();
    let _ = app.show_help(pages);
    shoot(&mut app, &dir, "editor-help");
    press(&mut app, Command::LeaveHelp);
    let _ = std::fs::remove_dir_all(&data);

    // Compare A|B: the flow with a change against the flow as it was.
    let _ = app.update(Message::Command(Command::Cancel));
    let _ = app.update(Message::Select(1));
    let old = app.doc().text();
    let new = old
        .replace("A[Edit]", "A[Write]")
        .replace("  classDef", "  C --> D[Publish]\n  classDef");
    let target = Target::Commit {
        rev: "3f969eb".into(),
        label: "HEAD · 3f969eb 2026-10-16 add the flow".into(),
    };
    select_all_and_paste(&mut app, &new);
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    show(&mut app, &new, Language::Mermaid);
    let _ = app.update(Message::Version(key, target, Ok(old.clone())));
    let c = app.compare.as_mut().unwrap();
    let _ = c
        .paper
        .update(paper::Message::Viewport(Size::new(330.0, 540.0)));
    c.paper.pretend_requested(1);
    let mut reply = answer(compare::KEY, 1, Ok(drawn(&old, Language::Mermaid)));
    reply.source = old.into();
    let _ = c.paper.rendered(reply);
    c.paper.finish_rasters();
    let _ = app.update(Message::Paper(
        key,
        paper::Message::Viewport(Size::new(330.0, 540.0)),
    ));
    app.doc_mut().paper.finish_rasters();
    app.notice = None;
    shoot(&mut app, &dir, "editor-compare");
    let _ = app.update(Message::Command(Command::CompareDiff));
    shoot(&mut app, &dir, "editor-compare-diff");

    // What a change did to the behavior of a state machine.
    let _ = app.update(Message::Command(Command::StopCompare));
    let door = "stateDiagram-v2\n  [*] --> Closed\n  Closed --> Open : open\n  Open --> Closed : close\n  Closed --> Locked : lock\n  Locked --> Closed : unlock\n";
    let jammed = door
        .replace(
            "Open --> Closed : close",
            "Open --> Jammed : close [stuck]\n  Jammed --> Closed : kick",
        )
        .replace("  Locked --> Closed : unlock\n", "");
    select_all_and_paste(&mut app, &jammed);
    due(&mut app, Alarm::Lint(key));
    show(&mut app, &jammed, Language::Mermaid);
    let target = Target::Commit {
        rev: "5a1e0c2".into(),
        label: "HEAD · 5a1e0c2 2026-10-20 the door".into(),
    };
    let _ = app.update(Message::Version(key, target, Ok(door.into())));
    let _ = app.update(Message::Command(Command::CompareBehavior));
    let _ = app.update(Message::Paper(
        key,
        paper::Message::Viewport(Size::new(330.0, 540.0)),
    ));
    app.doc_mut().paper.finish_rasters();
    app.notice = None;
    shoot(&mut app, &dir, "editor-compare-behavior");

    // The diagram in its own window; the editor takes the room.
    let _ = app.update(Message::Command(Command::StopCompare));
    let _ = app.update(Message::Command(Command::Detach));
    app.notice = None;
    shoot(&mut app, &dir, "editor-detached");
    let _ = app.update(Message::Paper(
        key,
        paper::Message::Viewport(Size::new(900.0, 520.0)),
    ));
    app.doc_mut().paper.finish_rasters();
    let settings = iced::Settings {
        default_font: app.font,
        ..Default::default()
    };
    let mut ui = iced_test::simulator::Simulator::with_size(
        settings,
        Size::new(900.0, 545.0),
        app.detached_view(),
    );
    ui::snapshot(&mut ui, &app.theme, &dir, "diagram-window");
    drop(ui);

    // The Omarchy skin pack: the diagram in the theme's colors.
    let _ = app.update(Message::Command(Command::Detach));
    let _ = app.update(Message::Command(Command::Skin(Some(Pack::Omarchy))));
    let text = app.doc().text();
    show(&mut app, &text, Language::Mermaid);
    app.notice = None;
    shoot(&mut app, &dir, "editor-skin");
}

/// The editor over a copy of the examples (the pictures show unsaved
/// edits) in `omarchy`'s colors: order.mmd with a changed message, drawn;
/// login.puml and shop.mmd in tabs. The copy's folder is returned for
/// removal.
fn examples_app(omarchy: Omarchy, name: &str) -> (App, PathBuf) {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .canonicalize()
        .unwrap();
    let root = temp_dir(name);
    let work = root.join("examples");
    std::fs::create_dir_all(&work).unwrap();
    // The diagrams only; the Help Omaspec folder stays out of the tree.
    for entry in std::fs::read_dir(&examples).unwrap().flatten() {
        if entry.path().is_file() {
            std::fs::copy(entry.path(), work.join(entry.file_name())).unwrap();
        }
    }
    let start = Start {
        files: ["order.mmd", "login.puml", "shop.mmd"]
            .iter()
            .map(|f| work.join(f))
            .collect(),
        folder: Some(work.clone()),
        forced: None,
    };
    let mut app = App::themed(start, None, omarchy).0;
    let main = app.main;
    let _ = app.update(Message::ScaleFactor(main, 2.0));
    let _ = app.update(Message::Scanned(work.clone(), folder::scan(&work)));
    let _ = app.update(Message::Select(0));
    let text = app
        .doc()
        .text()
        .replace("check out", "check out with the saved address");
    let at = text.find("check out").unwrap();
    select_all_and_paste(&mut app, &text);
    act(&mut app, Action::PlaceCaret(u32::try_from(at).unwrap()));
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    show(&mut app, &text, Language::Mermaid);
    app.notice = None;
    (app, root)
}

fn theme_named(name: &str, palette: omaspec_theme::Palette) -> Omarchy {
    Omarchy {
        name: Some(name.into()),
        palette,
        source: omaspec_theme::Source::Fallback {
            reason: "pictures".into(),
        },
        tweaks: omaspec_theme::Tweaks::default(),
    }
}

/// Writes the README's comparison when OMASPEC_SNAPSHOT_DIR is set,
/// with the examples in Tokyo Night (the built-in palette):
/// `readme-compare`. Take it with `--release`, or
/// the status bar shows debug render times.
#[test]
fn snapshot_readme() {
    let Some(dir) = std::env::var_os("OMASPEC_SNAPSHOT_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let theme = theme_named("tokyo-night", omaspec_theme::Palette::fallback());
    let (mut app, root) = examples_app(theme, "readme");

    // The shop flow against an older version, as changed lines.
    let _ = app.update(Message::Select(2));
    let old = app.doc().text();
    let new = old.replace("Saved[Save the cart]", "Saved[Keep the cart]")
        + "  Done --> Mail[Mail the receipt]\n";
    select_all_and_paste(&mut app, &new);
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    show(&mut app, &new, Language::Mermaid);
    let target = Target::Commit {
        rev: "4f2a9c1".into(),
        label: "HEAD · 4f2a9c1 2026-10-12 first shop flow".into(),
    };
    let _ = app.update(Message::Version(key, target, Ok(old.clone())));
    let c = app.compare.as_mut().unwrap();
    let _ = c
        .paper
        .update(paper::Message::Viewport(Size::new(330.0, 540.0)));
    let id = c.paper.pretend_next();
    let mut reply = answer(compare::KEY, id, Ok(drawn(&old, Language::Mermaid)));
    reply.source = old.into();
    let _ = c.paper.rendered(reply);
    c.paper.finish_rasters();
    let _ = app.update(Message::Paper(
        key,
        paper::Message::Viewport(Size::new(330.0, 540.0)),
    ));
    app.doc_mut().paper.finish_rasters();
    let _ = app.update(Message::Command(Command::CompareDiff));
    app.notice = None;
    shoot(&mut app, &dir, "readme-compare");
    let _ = std::fs::remove_dir_all(root);
}

/// The door of the getting started guide (docs/guide), its scenario and
/// a night of it.
const DOOR: &str = include_str!("../../../../examples/door/door.puml");
const LEAVE: &str = include_str!("../../../../examples/door/leave.puml");
const NIGHT: &str = include_str!("../../../../examples/door/night.jsonl");

/// `text` without the lines that hold `drop`.
fn without(text: &str, drop: &str) -> String {
    text.lines()
        .filter(|l| !l.contains(drop))
        .map(|l| format!("{l}\n"))
        .collect()
}

/// Draws the active tab with *Diagram theme: follow Omarchy* on, as a
/// render would: the theme's skin over the source, the answer moved back
/// onto the file.
fn show_themed(app: &mut App, language: Language) {
    let key = app.doc().key;
    let text = app.doc().text();
    let (shown, pending) = app.diagram_source(&text, language);
    // The diagram's share of the window `shoot` takes.
    let share = match app.panes.layout() {
        iced::widget::pane_grid::Node::Split { ratio, .. } => 1.0 - ratio,
        iced::widget::pane_grid::Node::Pane(_) => 0.5,
    };
    let _ = app.update(Message::Paper(
        key,
        paper::Message::Viewport(Size::new(1200.0 * share, 560.0)),
    ));
    let id = app.doc_mut().paper.pretend_next();
    if let Some(pending) = pending {
        app.pending.insert((key, id), pending);
    }
    let mut reply = answer(key, id, Ok(drawn(&shown, language)));
    reply.language = language;
    reply.source = shown;
    app.untangle(&mut reply);
    let _ = app.doc_mut().paper.rendered(reply);
    app.doc_mut().paper.finish_rasters();
}

/// The active tab's text replaced with `text`, linted and drawn in the
/// theme's colors.
fn retype(app: &mut App, text: &str, language: Language) {
    select_all_and_paste(app, text);
    let key = app.doc().key;
    due(app, Alarm::Lint(key));
    show_themed(app, language);
}

/// Moves the line between the source and the diagram to `ratio` of the
/// window, as dragging it would.
fn split_at(app: &mut App, ratio: f32) {
    if let iced::widget::pane_grid::Node::Split { id, .. } = app.panes.layout() {
        let split = *id;
        let _ = app.update(Message::Resized(iced::widget::pane_grid::ResizeEvent {
            split,
            ratio,
        }));
    }
}

/// The hover card of the finding on the line that holds `line`, as when
/// the pointer rests on `word` there.
fn hover_finding(app: &mut App, line: &str, word: &str) {
    let text = app.doc().text();
    let at = text.find(line).unwrap() + line.find(word).unwrap();
    let at = u32::try_from(at).unwrap();
    act(app, Action::PlaceCaret(at));
    act(app, Action::HoverQuery(at));
}

/// Writes the pictures of the README and of the getting started guide
/// when OMASPEC_SNAPSHOT_DIR is set, in Tokyo Night with the diagrams in
/// the theme's colors: `readme-hero`; the guide's door as it grows
/// (`guide-machine`, `guide-finding`, `guide-play`); its scenario with a
/// mistake (`readme-specs`); a night that goes wrong (`readme-trace`);
/// and the coffee machine at play, frame by frame (`play-NNN`). Take
/// them with `--release`, or the status bar shows debug render times.
#[test]
fn snapshot_story() {
    let Some(dir) = std::env::var_os("OMASPEC_SNAPSHOT_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let work = temp_dir("story").join("specs");
    std::fs::create_dir_all(&work).unwrap();
    let coffee = crate::gallery::EXAMPLES
        .iter()
        .find(|e| e.file == "coffee-machine.puml")
        .unwrap();
    for (file, text) in [
        ("coffee-machine.puml", coffee.text),
        ("door.puml", DOOR),
        ("leave.puml", LEAVE),
        ("night.jsonl", NIGHT),
    ] {
        std::fs::write(work.join(file), text).unwrap();
    }
    let theme = theme_named("tokyo-night", omaspec_theme::Palette::fallback());
    let start = Start {
        files: ["coffee-machine.puml", "door.puml", "leave.puml"]
            .iter()
            .map(|f| work.join(f))
            .collect(),
        folder: Some(work.clone()),
        forced: None,
    };
    let mut app = App::themed(start, None, theme).0;
    let main = app.main;
    let _ = app.update(Message::ScaleFactor(main, 2.0));
    let _ = app.update(Message::Scanned(work.clone(), folder::scan(&work)));
    press(&mut app, Command::FollowTheme);
    let _ = app.update(Message::Select(0));
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    show_themed(&mut app, Language::PlantUml);
    app.notice = None;
    shoot(&mut app, &dir, "readme-hero");

    // The guide: the door with three states, then a state nothing leaves.
    let _ = app.update(Message::Select(1));
    retype(&mut app, &without(DOOR, "Jammed"), Language::PlantUml);
    shoot(&mut app, &dir, "guide-machine");
    retype(&mut app, &without(DOOR, "repaired"), Language::PlantUml);
    assert!(
        app.doc()
            .findings
            .iter()
            .any(|f| f.message.contains("Nothing leaves Jammed")),
        "{:?}",
        app.doc().findings
    );
    // The source wide enough for the finding's card in two lines.
    split_at(&mut app, 0.66);
    show_themed(&mut app, Language::PlantUml);
    hover_finding(&mut app, "Open --> Jammed", "Jammed");
    shoot(&mut app, &dir, "guide-finding");
    split_at(&mut app, 0.5);
    retype(&mut app, DOOR, Language::PlantUml);
    press(&mut app, Command::Play);
    assert!(app.doc().paper.playing(), "{:?}", app.notice);
    app.doc_mut().paper.freeze_play(Duration::from_secs(3));
    shoot(&mut app, &dir, "guide-play");
    press(&mut app, Command::StopPlay);

    // The scenario locks the door while it is open.
    let _ = app.update(Message::Select(2));
    retype(&mut app, &without(LEAVE, ": close"), Language::PlantUml);
    let root = app.project_of(app.doc()).unwrap();
    let _ = app.update(Message::Specs(root.clone(), Arc::new(specs::read(&root))));
    assert!(
        app.doc()
            .findings
            .iter()
            .any(|f| f.message.contains("Door does not take lock here")),
        "{:?}",
        app.doc().findings
    );
    split_at(&mut app, 0.66);
    show_themed(&mut app, Language::PlantUml);
    hover_finding(&mut app, "Car -> Door : lock", "lock");
    app.notice = None;
    shoot(&mut app, &dir, "readme-specs");
    split_at(&mut app, 0.5);
    retype(&mut app, LEAVE, Language::PlantUml);
    let _ = app.update(Message::Specs(root.clone(), Arc::new(specs::read(&root))));

    // A night in which the door is locked while it stands open.
    let _ = app.update(Message::Select(1));
    show_themed(&mut app, Language::PlantUml);
    let wrong =
        without(NIGHT, "\"close\"") + r#"{"time":"07:09:52","object":"Door","event":"lock"}"#;
    std::fs::write(work.join("night.jsonl"), wrong).unwrap();
    let key = app.doc().key;
    let spec = app.doc().paper.spec("door.puml").unwrap();
    let _ = app.update(Message::Traces(key, specs::traces(&work, &spec)));
    assert!(app.doc().paper.playing(), "{:?}", app.notice);
    for _ in 0..4 {
        press(&mut app, Command::PlayAsk(paper::Ask::Next));
    }
    app.doc_mut().paper.freeze_play(Duration::from_secs(10));
    app.notice = None;
    shoot(&mut app, &dir, "readme-trace");
    press(&mut app, Command::StopPlay);

    // The coffee machine at play: on, a coffee, back to ready, and off, so
    // the picture loops. Each frame is drawn at its moment after the step,
    // however long the frames before took to draw; `-hold` marks a rest.
    let _ = app.update(Message::Select(0));
    show_themed(&mut app, Language::PlantUml);
    let key = app.doc().key;
    let begun = Instant::now();
    press(&mut app, Command::Play);
    assert!(app.doc().paper.playing(), "{:?}", app.notice);
    let mut frame = 0;
    let mut film = |app: &mut App, at: Instant, rest: bool| {
        app.doc_mut().paper.freeze_play_at(at);
        app.light_played_line(key);
        let hold = if rest { "-hold" } else { "" };
        shoot(app, &dir, &format!("play-{frame:03}{hold}"));
        frame += 1;
    };
    film(&mut app, begun + Duration::from_secs(3), true);
    for take in [0, 1, 0, 0, 0, 3] {
        let asked = Instant::now();
        press(&mut app, Command::PlayAsk(paper::Ask::Take(take)));
        let arrived = app.doc().paper.play_arrived().unwrap();
        let mut at = asked;
        while at < arrived + Duration::from_millis(600) {
            film(&mut app, at, false);
            at += Duration::from_millis(60);
        }
        film(&mut app, arrived + Duration::from_secs(3), true);
    }
    let _ = std::fs::remove_dir_all(work.parent().unwrap());
}

/// Writes `theme-<name>.png` for every bundled Omarchy theme when
/// OMASPEC_SNAPSHOT_DIR is set: the examples as in the README.
#[test]
fn snapshot_themes() {
    let Some(dir) = std::env::var_os("OMASPEC_SNAPSHOT_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let Ok(themes) = std::fs::read_dir("/usr/share/omarchy/themes") else {
        return;
    };
    let mut themes: Vec<_> = themes.flatten().map(|e| e.path()).collect();
    themes.sort();
    for theme_dir in themes {
        let Ok(palette) = omaspec_theme::Palette::load(&theme_dir.join("colors.toml")) else {
            continue;
        };
        let name = theme_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let (mut app, root) = examples_app(theme_named(&name, palette), "themes");
        shoot(&mut app, &dir, &format!("theme-{name}"));
        let _ = std::fs::remove_dir_all(root);
    }
}

/// In every bundled Omarchy theme the focus frame stands out as much as a
/// control has to (3:1), from its area and from the line between areas.
#[test]
fn the_focus_frame_stands_out_in_every_bundled_theme() {
    let Ok(themes) = std::fs::read_dir("/usr/share/omarchy/themes") else {
        return;
    };
    for theme_dir in themes.flatten().map(|e| e.path()) {
        let Ok(raw) = omaspec_theme::Palette::load(&theme_dir.join("colors.toml")) else {
            continue;
        };
        let p = raw.legible();
        for quiet in [p.background, p.dark_background] {
            let frame = super::view::focus_color(&p, quiet);
            for next in [quiet, p.dark_background] {
                let ratio = frame.contrast(next);
                assert!(ratio >= 3.0, "{theme_dir:?}: {ratio:.2}");
            }
        }
    }
}

#[test]
fn the_gallery_has_keys_of_its_own() {
    let none = Modifiers::empty();
    let named = |n: Named| Key::Named(n);
    let g = Mode::Gallery;
    assert_eq!(
        shortcut(
            &Key::Character("e".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
            Mode::Editing
        ),
        Some(Command::Gallery)
    );
    assert_eq!(
        shortcut(&named(Named::Enter), none, g),
        Some(Command::OpenExample(false))
    );
    assert_eq!(
        shortcut(&named(Named::Enter), Modifiers::SHIFT, g),
        Some(Command::OpenExample(true))
    );
    assert_eq!(
        shortcut(&named(Named::F5), none, g),
        Some(Command::OpenExample(true))
    );
    assert_eq!(
        shortcut(&named(Named::ArrowLeft), none, g),
        Some(Command::Left)
    );
    assert_eq!(
        shortcut(&named(Named::ArrowDown), none, g),
        Some(Command::Down)
    );
    assert_eq!(
        shortcut(&named(Named::Tab), none, g),
        Some(Command::NextGroup)
    );
    assert_eq!(
        shortcut(&named(Named::Tab), Modifiers::SHIFT, g),
        Some(Command::PreviousGroup)
    );
    assert_eq!(
        shortcut(&named(Named::Escape), none, g),
        Some(Command::Cancel)
    );
    // Outside the gallery the arrows stay with the text.
    assert_eq!(
        shortcut(&named(Named::ArrowLeft), none, Mode::Editing),
        None
    );
}

#[test]
fn an_example_opens_as_an_unsaved_copy_in_the_project_folder() {
    let work = temp_dir("gallery");
    let mut app = open(&[]);
    let _ = app.set_folder(Some(work.clone()));
    press(&mut app, Command::Gallery);
    assert_eq!(app.mode(app.main), Mode::Gallery);
    press(&mut app, Command::Right);
    press(&mut app, Command::OpenExample(false));
    assert!(app.gallery.is_none());
    // The empty first tab gave way.
    assert_eq!(app.docs.len(), 1);
    let second = &crate::gallery::EXAMPLES[1];
    let doc = app.doc();
    assert_eq!(doc.path.as_deref(), Some(work.join(second.file).as_path()));
    assert_eq!(doc.text(), second.text);
    assert!(doc.modified && !doc.on_disk());
    assert_eq!(app.mode(app.main), Mode::Editing);

    // The same one again takes a free name, in a tab of its own.
    press(&mut app, Command::Gallery);
    let _ = app.update(Message::GalleryOpen(Some(1), false));
    assert_eq!(app.docs.len(), 2);
    let stem = second.stem();
    let extension = second.language().extension();
    assert_eq!(
        app.doc().path.as_deref(),
        Some(work.join(format!("{stem}-2.{extension}")).as_path())
    );
    // Nothing was written.
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);

    // Escape closes the gallery and leaves the tabs.
    press(&mut app, Command::Gallery);
    press(&mut app, Command::Cancel);
    assert!(app.gallery.is_none());
    assert_eq!(app.docs.len(), 2);
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn an_example_opened_to_play_plays_once_it_is_drawn() {
    let mut app = open(&[]);
    press(&mut app, Command::Gallery);
    let _ = app.update(Message::GalleryInput("coffee".into()));
    press(&mut app, Command::OpenExample(true));
    let (key, text) = (app.doc().key, app.doc().text());
    assert!(!app.doc().paper.playing());
    let id = app.doc_mut().paper.pretend_next();
    let mut reply = answer(key, id, Ok(drawn(&text, Language::PlantUml)));
    reply.source = text.into();
    reply.language = Language::PlantUml;
    let _ = app.update(Message::Worker(worker::Event::Rendered(Box::new(reply))));
    assert!(app.doc().paper.playing());
    assert_eq!(app.focus, Pane::Diagram);
}

/// Writes `gallery.png`, `gallery-omarchy.png` and `gallery-complex.png`
/// when OMASPEC_SNAPSHOT_DIR is set: the gallery of examples in Tokyo
/// Night with the third tile chosen, then with the diagrams in the theme's
/// colors, then only the complex examples.
#[test]
fn snapshot_gallery() {
    let Some(dir) = std::env::var_os("OMASPEC_SNAPSHOT_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let theme = theme_named("tokyo-night", omaspec_theme::Palette::fallback());
    let mut app = App::themed(Start::default(), None, theme).0;
    let (main, size) = (app.main, Size::new(1360.0, 880.0));
    let _ = app.update(Message::ScaleFactor(main, 2.0));
    let _ = app.update(Message::WindowSized(main, size));
    press(&mut app, Command::Gallery);
    press(&mut app, Command::Right);
    press(&mut app, Command::Right);
    app.settle_gallery();
    shoot_at(&mut app, &dir, "gallery", size);
    press(&mut app, Command::FollowTheme);
    app.settle_gallery();
    shoot_at(&mut app, &dir, "gallery-omarchy", size);
    // Only the complex ones, in the colors of the files.
    press(&mut app, Command::FollowTheme);
    let _ = app.update(Message::GalleryComplex);
    app.settle_gallery();
    shoot_at(&mut app, &dir, "gallery-complex", size);
}

#[test]
fn the_palette_puts_the_command_chosen_last_on_top() {
    let mut app = open(&[]);
    press(&mut app, Command::Palette);
    let _ = app.update(Message::PickerInput("fit the".into()));
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.chosen(), Some(&picker::Choice::Run(Command::Fit)));
    // The typed letters light up where the score found them.
    assert_eq!(picker.lit("Fit the diagram"), [0, 1, 2, 4, 5, 6]);
    let _ = app.update(Message::PickerSubmit);
    press(&mut app, Command::Palette);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.chosen(), Some(&picker::Choice::Run(Command::Fit)));
    assert!(picker.is_recent(0) && !picker.is_recent(1));
    // A letter typed sorts by the match again; the label stays recent.
    let _ = app.update(Message::PickerInput("save as".into()));
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.chosen(), Some(&picker::Choice::Run(Command::SaveAs)));
}

#[test]
fn an_empty_page_starts_from_a_template_and_nothing_else_does() {
    let mut app = open(&[]);
    press(&mut app, Command::NewTab);
    act(&mut app, Action::SelectAll);
    act(&mut app, Action::Backspace);
    assert_eq!(app.doc().text(), "");
    press(&mut app, Command::StartFrom(Language::Mermaid));
    assert_eq!(app.doc().text(), "flowchart LR\n  A --> B\n");
    // Text that is there stays.
    press(&mut app, Command::StartFrom(Language::PlantUml));
    assert_eq!(app.doc().text(), "flowchart LR\n  A --> B\n");
}

/// Writes `editor-palette-recent` (the palette after a command was chosen
/// from it) and `editor-empty` (an empty page that does not draw) when
/// OMASPEC_SNAPSHOT_DIR is set.
#[test]
fn snapshot_palette_and_empty_page() {
    let Some(dir) = std::env::var_os("OMASPEC_SNAPSHOT_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let work = temp_dir("empty-page");
    let file = work.join("door.puml");
    let door =
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
    std::fs::write(&file, door).unwrap();
    let mut app = open(std::slice::from_ref(&file));
    show(&mut app, door, Language::PlantUml);
    press(&mut app, Command::Palette);
    let _ = app.update(Message::PickerInput("fit the".into()));
    let _ = app.update(Message::PickerSubmit);
    press(&mut app, Command::Palette);
    shoot(&mut app, &dir, "editor-palette-recent");
    press(&mut app, Command::Cancel);

    press(&mut app, Command::NewTab);
    act(&mut app, Action::SelectAll);
    act(&mut app, Action::Backspace);
    let mut renderer = omaspec_render::Renderer::new(&omaspec_render::Config {
        java_fallback: false,
        ..Default::default()
    });
    let Err(err) = renderer.render("", Language::PlantUml) else {
        panic!("an empty page draws");
    };
    let key = app.doc().key;
    let paper = &mut app.doc_mut().paper;
    let id = paper.pretend_next();
    let _ = paper.rendered(answer(key, id, Err(err)));
    shoot(&mut app, &dir, "editor-empty");
    let _ = std::fs::remove_dir_all(work);
}

/// The name at the caret lights on the sheet. A comment, a mark and a
/// play do not; when the play is over the name lights again.
#[test]
fn the_caret_lights_its_name_on_the_sheet() {
    let source = "\
@startuml
' Player is a note
[*] --> Player
Player --> Idle : stop
@enduml
";
    let mut app = open(&[]);
    select_all_and_paste(&mut app, source);
    let source = app.doc().text();
    show(&mut app, &source, Language::PlantUml);

    let at = |text: &str, word: &str, which: usize| {
        let mut from = 0;
        for _ in 0..which {
            from += text[from..].find(word).unwrap() + word.len();
        }
        let start = from + text[from..].find(word).unwrap();
        u32::try_from(start + word.len() / 2).unwrap()
    };
    let holds = |app: &App, label: &str| {
        let paper = &app.doc().paper;
        let Some(aim) = paper.breath_target() else {
            return false;
        };
        let on_screen = paper
            .view
            .on_screen(aim, iced::Rectangle::with_size(paper.viewport()));
        let spot = paper.screen_of(label);
        spot.x >= on_screen.x - 1.0
            && spot.x <= on_screen.x + on_screen.width + 1.0
            && spot.y >= on_screen.y - 1.0
            && spot.y <= on_screen.y + on_screen.height + 1.0
    };

    act(&mut app, Action::PlaceCaret(at(&source, "Player", 0)));
    assert!(app.doc().paper.breath_target().is_none());

    act(&mut app, Action::PlaceCaret(at(&source, "Player", 1)));
    assert!(
        holds(&app, "Player"),
        "{:?}",
        app.doc().paper.breath_target()
    );
    assert!(!holds(&app, "Idle"));

    act(&mut app, Action::PlaceCaret(at(&source, "Idle", 0)));
    assert!(holds(&app, "Idle"), "{:?}", app.doc().paper.breath_target());
    assert!(!holds(&app, "Player"));

    let arrow = u32::try_from(source.find("-->").unwrap()).unwrap() + 1;
    act(&mut app, Action::PlaceCaret(arrow));
    assert!(app.doc().paper.breath_target().is_none());

    act(&mut app, Action::PlaceCaret(at(&source, "stop", 0)));
    assert!(holds(&app, "stop"), "{:?}", app.doc().paper.breath_target());

    press(&mut app, Command::Play);
    assert!(
        app.doc().paper.breath_target().is_none(),
        "{:?}",
        app.notice
    );
    let key = app.doc().key;
    let _ = app.update(Message::Paper(key, paper::Message::PlayGone));
    assert!(holds(&app, "stop"), "{:?}", app.doc().paper.breath_target());
}

/// The PlantUML `file` in the first tab with a line added and one
/// deleted, as the renderer without Java answers it.
fn shown(file: &Path) -> App {
    let mut app = open(&[file.to_owned()]);
    let main = app.main;
    let _ = app.update(Message::ScaleFactor(main, 2.0));
    let text = app.doc().text();
    let at = |line: &str| u32::try_from(text.find(line).unwrap()).unwrap();
    act(&mut app, Action::PlaceCaret(at("else wrong")));
    act(&mut app, Action::DeleteLine);
    act(&mut app, Action::PlaceCaret(at("alt password")));
    paste(&mut app, "User -> App : remember me\n");
    let key = app.doc().key;
    due(&mut app, Alarm::Lint(key));
    let mut renderer = omaspec_render::Renderer::new(&omaspec_render::Config {
        java_fallback: false,
        ..Default::default()
    });
    // The `alt` has no `end`.
    let Err(err) = renderer.render(&app.doc().text(), Language::PlantUml) else {
        panic!("the unfinished login draws");
    };
    let paper = &mut app.doc_mut().paper;
    paper.pretend_requested(1);
    let _ = paper.rendered(answer(key, 1, Err(err)));
    app
}

/// Draws `source` in the active tab's diagram pane, as a render would.
fn show(app: &mut App, source: &str, language: Language) {
    let key = app.doc().key;
    let _ = app.update(Message::Paper(
        key,
        paper::Message::Viewport(Size::new(600.0, 560.0)),
    ));
    let paper = &mut app.doc_mut().paper;
    let id = paper.pretend_next();
    let mut reply = answer(key, id, Ok(drawn(source, language)));
    reply.language = language;
    reply.source = source.into();
    let _ = paper.rendered(reply);
    paper.finish_rasters();
}

/// Settles what the window animates: the tab highlight and the focus frame
/// where they end, the highlighting caught up as frame by frame.
fn settle(app: &mut App) {
    app.tab_fade = Animation::new(true);
    app.focus_fade = Animation::new(true);
    let key = app.doc().key;
    for _ in 0..20 {
        let _ = app.update(Message::Editor(key, scrive_iced::Event::HighlightSweep));
    }
}

fn shoot(app: &mut App, dir: &Path, name: &str) {
    shoot_at(app, dir, name, Size::new(1200.0, 640.0));
}

fn shoot_at(app: &mut App, dir: &Path, name: &str, size: Size) {
    settle(app);
    let settings = iced::Settings {
        default_font: app.font,
        fonts: scrive_iced::required_fonts()
            .iter()
            .map(|f| std::borrow::Cow::Borrowed(*f))
            .collect(),
        ..Default::default()
    };
    let mut ui = iced_test::simulator::Simulator::with_size(settings, size, app.main_view());
    // With the keys elsewhere the caret gives way, as `apply_keyboard_focus`
    // has it: a click in the empty end of the tab bar takes the editor's focus.
    if app.focus != Pane::Source {
        ui.point_at(iced::Point::new(size.width - 8.0, 18.0));
        let _ = ui.simulate(iced_test::simulator::click());
    }
    ui::snapshot(&mut ui, &app.theme, dir, name);
}

/// The editor window drawn headless at any scale; iced_test's simulator
/// draws at 2 only.
struct Screen {
    renderer: iced::Renderer,
    scale: f32,
    warm: bool,
}

/// A screenshot, as RGBA rows.
struct Shot {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

impl Screen {
    const SIZE: Size = Size::new(1200.0, 640.0);

    fn new(app: &App, scale: f32) -> Screen {
        use iced::advanced::graphics::text::font_system;
        use iced::advanced::renderer::Headless;
        for font in scrive_iced::required_fonts() {
            font_system()
                .write()
                .unwrap()
                .load_font(std::borrow::Cow::Borrowed(*font));
        }
        let backend = std::env::var("ICED_TEST_BACKEND").ok();
        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            app.font,
            iced::Pixels(16.0),
            backend.as_deref(),
        ))
        .expect("a headless renderer");
        Screen {
            renderer,
            scale,
            warm: false,
        }
    }

    fn shoot(&mut self, app: &App) -> Shot {
        // The first frame starts uploading the pictures in the background.
        if !self.warm {
            let _ = self.frame(app);
            std::thread::sleep(Duration::from_millis(300));
            self.warm = true;
        }
        self.frame(app)
    }

    fn frame(&mut self, app: &App) -> Shot {
        use iced::advanced::renderer::{Headless, Style};
        use iced::theme::Base;
        let mut ui = iced_runtime::UserInterface::build(
            app.main_view(),
            Self::SIZE,
            iced_runtime::user_interface::Cache::default(),
            &mut self.renderer,
        );
        let redraw = iced::Event::Window(iced::window::Event::RedrawRequested(
            iced::time::Instant::now(),
        ));
        let cursor = iced::mouse::Cursor::Unavailable;
        let _ = ui.update(
            &[redraw],
            cursor,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Vec::new(),
        );
        let base = app.theme.base();
        let style = Style {
            text_color: base.text_color,
        };
        ui.draw(&mut self.renderer, &app.theme, &style, cursor);
        let (width, height) = (
            (Self::SIZE.width * self.scale).round() as u32,
            (Self::SIZE.height * self.scale).round() as u32,
        );
        // The interface lives on until the screenshot: the renderer holds
        // its text only weakly.
        let rgba = self.renderer.screenshot(
            iced::Size::new(width, height),
            self.scale,
            base.background_color,
        );
        drop(ui);
        Shot {
            width: width as usize,
            height: height as usize,
            rgba,
        }
    }
}

impl Shot {
    fn red(&self, (x, y): (usize, usize)) -> u8 {
        self.rgba[(y * self.width + x) * 4]
    }

    fn green(&self, (x, y): (usize, usize)) -> u8 {
        self.rgba[(y * self.width + x) * 4 + 1]
    }

    /// Whether the pixel is Tokyo Night's accent, as the crisp line round a
    /// lit state is.
    fn accent(&self, (x, y): (usize, usize)) -> bool {
        self.red((x, y)) <= 0x90 && self.rgba[(y * self.width + x) * 4 + 2] >= 0xE8
    }

    /// The first pixel from `from`, in steps of `step`, redder than `above`.
    fn first(&self, from: (usize, usize), step: (usize, usize), above: u8) -> (usize, usize) {
        let mut at = from;
        while self.red(at) <= above {
            at = (at.0 + step.0, at.1 + step.1);
            assert!(
                at.0 < self.width && at.1 < self.height,
                "nothing from {from:?}"
            );
        }
        at
    }

    /// The red of the pixels across an edge found by [`Self::first`]: ten
    /// before it, three after.
    fn across(&self, edge: (usize, usize), step: (usize, usize)) -> Vec<u8> {
        (0..13)
            .map(|i| {
                self.red((
                    edge.0 + i * step.0 - 10 * step.0,
                    edge.1 + i * step.1 - 10 * step.1,
                ))
            })
            .collect()
    }

    fn save(&self, path: &Path) {
        use omaspec_render::export::tiny_skia::{IntSize, Pixmap};
        let size = IntSize::from_wh(self.width as u32, self.height as u32).unwrap();
        let png = Pixmap::from_vec(self.rgba.clone(), size)
            .unwrap()
            .encode_png()
            .unwrap();
        std::fs::write(path, png).unwrap();
    }
}

/// At a fractional scale, as on a 1.5 screen, when OMASPEC_SNAPSHOT_DIR
/// is set: the paper's edge and glow keep their pixels wherever a pan
/// leaves them; the veil of a play leaves no light fleck in the paper's
/// corners, nor a lit state's picture in its own; the red edge of a
/// diagram that does not draw lies over the veil; and a diagram dragged
/// over the source text stays in its pane.
/// Writes `scale-1.5-paper`, `scale-1.5-play`, `scale-1.5-stale` and
/// `scale-1.5-dragged`.
#[test]
fn snapshot_fractional_scale() {
    let Some(dir) = std::env::var_os("OMASPEC_SNAPSHOT_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let door = "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
    let work = temp_dir("fractional");
    let file = work.join("door.puml");
    std::fs::write(&file, door).unwrap();
    let start = Start {
        files: vec![file],
        ..Start::default()
    };
    let theme = theme_named("tokyo-night", omaspec_theme::Palette::fallback());
    let mut app = App::themed(start, None, theme).0;
    let main = app.main;
    let _ = app.update(Message::ScaleFactor(main, 1.5));
    show(&mut app, door, Language::PlantUml);
    settle(&mut app);
    let scale = 1.5;
    let px = |logical: f32| (logical * scale).round() as usize;
    let mut screen = Screen::new(&app, scale);

    // The white paper on Tokyo Night: from the middle of the pane up, and
    // from its left side across.
    let (down, right) = ((0, 1), (1, 0));
    let mut profiles = None;
    for step in 0..6 {
        if step > 0 {
            let _ = app.doc_mut().paper.pan(0.23, 0.17);
        }
        let shot = screen.shoot(&app);
        if step == 0 {
            shot.save(&dir.join("scale-1.5-paper.png"));
        }
        let top = shot.first((px(900.0), px(60.0)), down, 0x80);
        let left = shot.first((px(620.0), top.1 + px(60.0)), right, 0x80);
        let seen = (shot.across(top, down), shot.across(left, right));
        match &profiles {
            None => profiles = Some(seen),
            Some(first) => assert_eq!(&seen, first, "the edge after pan {step}"),
        }
    }

    // The veil covers the paper to its rounded edge, corners too.
    let _ = app.update(Message::Command(Command::Play));
    app.doc_mut().paper.freeze_play(Duration::from_secs(3));
    let shot = screen.shoot(&app);
    shot.save(&dir.join("scale-1.5-play.png"));
    let top = shot.first((px(900.0), px(60.0)), down, 0x48);
    let left = shot.first((px(620.0), top.1 + px(60.0)), right, 0x48);
    let veiled = shot.red((left.0 + px(20.0), top.1 + 2));
    for y in top.1 - 3..top.1 + px(12.0) {
        for x in left.0 - 3..left.0 + px(12.0) {
            let red = shot.red((x, y));
            assert!(red <= veiled + 1, "({x}, {y}): {red:#x} over {veiled:#x}");
        }
    }

    // The lit state's picture shows within its rounded outline only; its
    // clip's corners lie under the veil like the rest.
    let (zoom, closed) = {
        let paper = &app.doc().paper;
        let map = &paper.diagram.as_ref().unwrap().drawn.map;
        let spot = map.states.iter().find(|s| s.name == "Closed").unwrap();
        (paper.view.zoom * scale, spot.rect)
    };
    let on = |x: f32, y: f32| {
        (
            left.0 + (x * zoom).round() as usize,
            top.1 + (y * zoom).round() as usize,
        )
    };
    let mut outline_top = on(closed.x + closed.w * 0.75, closed.y);
    outline_top.1 -= 12;
    while !shot.accent(outline_top) {
        outline_top.1 += 1;
    }
    let mut outline_left = on(closed.x, closed.y + closed.h * 0.25);
    outline_left.0 -= 12;
    while !shot.accent(outline_left) {
        outline_left.0 += 1;
    }
    for y in outline_top.1 + 1..=outline_top.1 + 2 {
        for x in outline_left.0 + 1..=outline_left.0 + 2 {
            let red = shot.red((x, y));
            assert!(red <= 0xB0, "the lit corner at ({x}, {y}): {red:#x}");
        }
    }

    // The text stops drawing: the red edge stays red over the veil.
    let broken = "@startuml\n[*] --> Closed\nClosed --> \n@enduml\n";
    let mut renderer = omaspec_render::Renderer::new(&omaspec_render::Config {
        java_fallback: false,
        ..Default::default()
    });
    let Err(err) = renderer.render(broken, Language::PlantUml) else {
        panic!("the broken door draws");
    };
    let key = app.doc().key;
    let paper = &mut app.doc_mut().paper;
    let id = paper.pretend_next();
    let _ = paper.rendered(answer(key, id, Err(err)));
    let stale = screen.shoot(&app);
    stale.save(&dir.join("scale-1.5-stale.png"));
    let red = stale.red(left).saturating_sub(stale.green(left));
    assert!(red > 0x50, "the edge inside the paper: {red:#x}");

    // Dragged over the source text, the picture stops at its pane. Its left
    // edge goes to 500, 100 past the pane's, however wide the paper is.
    let below_text = |shot: &Shot| -> Vec<u8> {
        (px(300.0)..px(470.0))
            .flat_map(|y| (px(340.0)..px(580.0)).map(move |x| (x, y)))
            .map(|at| shot.red(at))
            .collect()
    };
    let before = below_text(&stale);
    let _ = app.doc_mut().paper.pan(500.0 - left.0 as f32 / scale, 0.0);
    let dragged = screen.shoot(&app);
    dragged.save(&dir.join("scale-1.5-dragged.png"));
    assert!(
        dragged.red((px(612.0), left.1)) > 0x48,
        "the paper reaches past the pane"
    );
    assert!(
        below_text(&dragged) == before,
        "the source text is drawn over"
    );
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn compare_tells_the_changes_in_behavior() {
    let mut app = open(&[]);
    select_all_and_paste(
        &mut app,
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
    );
    press(&mut app, Command::NewTab);
    // The same door in Mermaid, which now jams.
    select_all_and_paste(
        &mut app,
        "stateDiagram-v2\n[*] --> Closed\nClosed --> Open : open\nOpen --> Jammed : close\n",
    );
    press(&mut app, Command::Compare);
    let _ = app.update(Message::PickerSubmit);
    let c = app.compare.as_ref().expect("comparing");
    let messages: Vec<&str> = c
        .behavior
        .as_ref()
        .unwrap()
        .iter()
        .map(|change| change.message.as_str())
        .collect();
    assert!(
        messages.contains(&"On close, Open now goes to Jammed instead of Closed."),
        "{messages:?}"
    );
    press(&mut app, Command::CompareDiff);
    press(&mut app, Command::CompareBehavior);
    let c = app.compare.as_ref().unwrap();
    assert!(c.show_behavior && !c.show_diff);

    // Edited back, the behavior is the same again.
    select_all_and_paste(
        &mut app,
        "stateDiagram-v2\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n",
    );
    let a = app.doc().key;
    due(&mut app, Alarm::Render(a));
    let c = app.compare.as_ref().unwrap();
    assert_eq!(c.behavior, Ok(Vec::new()));
}

#[test]
fn convert_opens_the_other_language_in_a_new_tab() {
    let mut app = open(&[]);
    select_all_and_paste(
        &mut app,
        "@startuml\n[*] --> Idle\nIdle --> Busy : job\nstate Busy #pink\n@enduml\n",
    );
    press(&mut app, Command::Convert);
    assert_eq!(app.docs.len(), 2);
    assert_eq!(app.doc().language, Language::Mermaid);
    assert!(app.doc().text().contains("Idle --> Busy : job"));
    let notice = app.notice.clone().unwrap();
    assert!(
        notice.starts_with("written in Mermaid in a new tab; Colors"),
        "{notice}"
    );

    select_all_and_paste(
        &mut app,
        "@startuml\nA -> B : x\nref over A, B : more\n@enduml\n",
    );
    press(&mut app, Command::Convert);
    assert_eq!(app.docs.len(), 2);
    let notice = app.notice.clone().unwrap();
    assert_eq!(notice, "not written in Mermaid: line 3: Mermaid has no ref");
}
