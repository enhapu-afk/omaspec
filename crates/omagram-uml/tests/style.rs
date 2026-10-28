//! `<style>` blocks draw what the skinparams they stand for draw, and
//! decline what the engine does not draw, naming the line.

fn render(head: &str, body: &str) -> String {
    let source = format!("@startuml\n{head}\n{body}\n@enduml\n");
    omagram_uml::render(&source).unwrap_or_else(|e| panic!("{head}: {e:?}"))
}

/// The same diagram drawn with a style and with skinparams.
fn same(body: &str, style: &str, skinparams: &str) {
    let styled = render(&format!("<style>\n{style}\n</style>"), body);
    let plain = render(skinparams, body);
    assert_eq!(styled, plain, "{style}");
}

const SEQUENCE: &str = "title Login\nactor User\nparticipant Server <<web>>\nUser -> Server : login\nnote right : checks\nactivate Server\nalt ok\n  Server --> User : token\nelse\n  Server --> User : error\nend\n== Done ==\nlegend\nkey\nendlegend";

#[test]
fn sequence_diagrams() {
    same(
        SEQUENCE,
        "root {\n  FontName serif\n  FontColor #333333\n}\nsequenceDiagram {\n  participant, actor {\n    BackGroundColor #FFEECC\n    LineColor #AA5500\n  }\n  arrow {\n    LineColor: #AA5500;\n    FontSize 12\n    HorizontalAlignment center\n  }\n  note { BackGroundColor #EEFFEE }\n  lifeLine { LineColor #999999 }\n  activationBox { BackGroundColor #FFDDDD }\n  group { LineColor #556677 }\n  groupHeader { BackGroundColor #DDEEFF }\n  separator { FontStyle italic }\n}\ntitle { FontSize 20 }\nlegend { BackGroundColor #F0F0F0 }",
        "skinparam defaultFontName serif\nskinparam defaultFontColor #333333\nskinparam ParticipantBackgroundColor #FFEECC\nskinparam ActorBackgroundColor #FFEECC\nskinparam ParticipantBorderColor #AA5500\nskinparam ActorBorderColor #AA5500\nskinparam ArrowColor #AA5500\nskinparam ArrowFontSize 12\nskinparam SequenceMessageAlignment center\nskinparam NoteBackgroundColor #EEFFEE\nskinparam SequenceLifeLineBorderColor #999999\nskinparam SequenceLifeLineBackgroundColor #FFDDDD\nskinparam SequenceGroupBorderColor #556677\nskinparam SequenceGroupHeaderBackgroundColor #DDEEFF\nskinparam SequenceDividerFontStyle italic\nskinparam TitleFontSize 20\nskinparam LegendBackgroundColor #F0F0F0",
    );
}

#[test]
fn the_later_setting_wins_whichever_it_is() {
    // PlantUML ranks styles and skinparams by their place alone.
    let body = "Alice -> Bob : hi";
    let blue = render("skinparam ArrowColor blue", body);
    assert_eq!(
        render(
            "skinparam ArrowColor red\n<style>\narrow { LineColor blue }\n</style>",
            body
        ),
        blue
    );
    assert_eq!(
        render(
            "<style>\nroot { LineColor red }\nsequenceDiagram { arrow { LineColor green } }\nroot { LineColor blue }\n</style>",
            body
        ),
        render(
            "skinparam ArrowColor blue\nskinparam ParticipantBorderColor blue\nskinparam SequenceLifeLineBorderColor blue",
            body
        )
    );
    // Single-line form, variables.
    assert_eq!(
        render("<style>arrow { LineColor blue }</style>", body),
        blue
    );
    assert_eq!(
        render(
            "<style>\n--ink: blue;\narrow { LineColor var(--ink) }\n</style>",
            body
        ),
        blue
    );
}

#[test]
fn activity_diagrams() {
    same(
        "|Lane|\nstart\n:step;\nif (ok?) then (yes)\n  :done;\n  note right: fine\nelse (no)\n  stop\nendif\nfork\n  :a;\nfork again\n  :b;\nend fork\npartition Part {\n  :inner;\n}\nend",
        "activityDiagram {\n  activity {\n    BackGroundColor #E0F0FF\n    LineColor #3366AA\n    RoundCorner 10\n  }\n  diamond { BackGroundColor #FFF0E0 }\n  arrow {\n    LineColor #3366AA\n    LineStyle 7-7\n    FontColor #3366AA\n  }\n  circle { start { BackGroundColor #22AA22 } stop { LineColor #AA2222 } }\n  activityBar { BackGroundColor #444444 }\n  swimlane { LineColor #888888; FontSize 14 }\n  note { BackGroundColor #FFFFE0 }\n  composite { LineColor #AA00AA }\n}",
        "skinparam ActivityBackgroundColor #E0F0FF\nskinparam ActivityDiamondBackgroundColor #FFF0E0\nskinparam ActivityBorderColor #3366AA\nskinparam ActivityDiamondBorderColor #3366AA\nskinparam ActivityRoundCorner 10\nskinparam ArrowColor #3366AA\nskinparam ArrowStyle dashed\nskinparam ArrowFontColor #3366AA\nskinparam ActivityStartColor #22AA22\nskinparam ActivityEndColor #AA2222\nskinparam ActivityBarColor #444444\nskinparam SwimlaneBorderColor #888888\nskinparam SwimlaneTitleFontSize 14\nskinparam NoteBackgroundColor #FFFFE0\nskinparam PartitionBorderColor #AA00AA",
    );
}

#[test]
fn state_diagrams() {
    same(
        "[*] --> Idle\nstate Busy {\n  [*] --> Work\n}\nIdle --> Busy : job\nBusy --> [*]\nnote right of Idle : waits\nmainframe Job",
        "document { BackGroundColor #1E1E2E }\nroot { FontColor #CDD6F4 }\nstateDiagram {\n  state {\n    BackGroundColor #313244\n    LineColor #89B4FA\n    LineThickness 1.5\n    RoundCorner 4\n  }\n  arrow { LineColor #89B4FA; LineThickness 2 }\n  note { BackGroundColor #45475A }\n}\nmainframe { LineColor #F38BA8; FontStyle bold }",
        "skinparam BackgroundColor #1E1E2E\nskinparam defaultFontColor #CDD6F4\nskinparam StateBackgroundColor #313244\nskinparam StateBorderColor #89B4FA\nskinparam StateBorderThickness 1.5\nskinparam StateRoundCorner 4\nskinparam ArrowColor #89B4FA\nskinparam ArrowThickness 2\nskinparam NoteBackgroundColor #45475A\nskinparam MainframeBorderColor #F38BA8\nskinparam MainframeFontStyle bold",
    );
}

#[test]
fn styles_for_other_types_and_elements_change_nothing() {
    let body = "Alice -> Bob : hi";
    assert_eq!(
        render(
            "<style>\nactivityDiagram { activity { BackGroundColor red } }\nganttDiagram { task { FontColor red } }\nroot { HyperLinkColor red; Shadowing 0 }\n</style>",
            body
        ),
        render("", body)
    );
}

#[test]
fn what_is_not_drawn_declines_at_its_line() {
    for (head, body, line, part) in [
        (
            "<style>\ntitle {\n  BackGroundColor yellow\n}\n</style>",
            "title T\nA -> B",
            3,
            "`title { BackGroundColor yellow }` is not supported yet",
        ),
        (
            "<style>\nparticipant { FontSize huge }\n</style>",
            "A -> B",
            2,
            "`participant { FontSize }` needs a number",
        ),
        (
            "<style>\n.warn { BackGroundColor red }\n</style>",
            "A -> B <<warn>>",
            2,
            "stereotypes",
        ),
        (
            "<style>\nstate { Padding 10 }\n</style>",
            "[*] --> A",
            2,
            "`state { Padding 10 }`",
        ),
        (
            "<style>\nroot { FontColor red }\n</style>",
            "class A",
            1,
            "`<style>` is not supported yet",
        ),
    ] {
        let source = format!("@startuml\n{head}\n{body}\n@enduml\n");
        let err = omagram_uml::render(&source).unwrap_err();
        assert_eq!(err.line, Some(line), "{head}: {}", err.reason);
        assert!(err.reason.contains(part), "{head}: {}", err.reason);
    }
}
