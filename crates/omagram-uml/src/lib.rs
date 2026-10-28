//! Native PlantUML engine: preprocessor, parser with source spans, layout
//! and SVG output, one diagram type at a time: sequence diagrams, activity
//! diagrams, state diagrams, structure diagrams (class, component, use
//! case, deployment and object), timing diagrams, Gantt charts, mind maps,
//! WBS diagrams, JSON diagrams and formulas so far. `@startflow` is Omagram's own
//! signal-flow diagram: PlantUML has no such type, and a line it declines
//! is not sent to the Java PlantUML.
//!
//! It renders a diagram only when it understands every construct in it; for
//! anything else it declines and names the line, so the caller can fall back
//! to the Java PlantUML or show the reason. Filled in track N of the roadmap
//! (see `docs/adr/0004-native-plantuml.md`).

pub mod activity;
mod annotations;
pub mod architecture;
pub mod block;
pub mod c4;
mod color;
mod common;
pub mod curve;
pub mod er;
pub mod flow;
pub mod formula;
pub mod gantt;
pub mod graph;
pub mod journey;
pub mod json;
pub mod map;
mod metrics;
pub mod pie;
mod preprocess;
pub mod sequence;
mod source;
pub mod state;
pub mod structure;
mod style;
mod svg;
mod text;
pub mod timeline;
pub mod timing;
pub mod tree;

// The Mermaid drawer in omagram-render lays out state diagrams and
// flowcharts on the same graph and draws with the same canvas. The modules
// stay private.
pub use color::{Color, Paint, parse as parse_color};
pub use svg::{Align, Canvas, Layer, Stroke};
pub use text::{
    Block, Family, Font, Style, block as measure_text, measure as measure_run,
    styled as measure_styled,
};

/// Why the engine did not draw a diagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declined {
    /// Zero-based line of the first construct the engine does not know,
    /// when a single line is to blame.
    pub line: Option<usize>,
    pub reason: String,
}

impl Declined {
    pub(crate) fn at(line: usize, reason: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            reason: reason.into(),
        }
    }
}

/// A parsed diagram of one of the types the engine draws.
#[derive(Debug, Clone, PartialEq)]
pub enum Diagram {
    Sequence(sequence::Diagram),
    Activity(activity::Diagram),
    State(state::Diagram),
    MindMap(tree::MindMap),
    Wbs(tree::Wbs),
    Structure(structure::Diagram),
    Timing(timing::Diagram),
    Gantt(gantt::Diagram),
    Flow(flow::Diagram),
    Json(json::Diagram),
    Formula(formula::Diagram),
}

pub use preprocess::{Included, Includes};
pub use source::Line;

/// Parses `source`, or declines. The preprocessor runs first; `!include`
/// declines, since nothing says where the diagram is.
pub fn parse(source: &str) -> Result<Diagram, Declined> {
    parse_with(source, None)
}

/// Parses `source`, reading its `!include`s through `includes`.
pub fn parse_with(source: &str, includes: Option<&dyn Includes>) -> Result<Diagram, Declined> {
    let (kind, raw) = source::body(source)?;
    let expanded = preprocess::run(&raw, includes)?;
    let lines = match &expanded {
        Some(expanded) => expanded.iter().map(preprocess::Owned::line).collect(),
        None => raw,
    };
    Ok(match kind {
        source::Kind::Uml => uml(&lines)?,
        source::Kind::MindMap => Diagram::MindMap(tree::parse_mindmap(&lines)?),
        source::Kind::Wbs => Diagram::Wbs(tree::parse_wbs(&lines)?),
        source::Kind::Gantt => Diagram::Gantt(gantt::parse(&lines)?),
        source::Kind::Flow => Diagram::Flow(flow::parse(&lines)?),
        source::Kind::Json => Diagram::Json(json::parse(&lines)?),
        source::Kind::Latex => Diagram::Formula(formula::parse(&lines, formula::Notation::Latex)?),
        source::Kind::Math => Diagram::Formula(formula::parse(&lines, formula::Notation::Ascii)?),
    })
}

/// The type of an `@startuml` diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UmlType {
    Timing,
    Activity,
    State,
    Structure,
    /// What no other type claims, as in PlantUML.
    Sequence,
    /// Nothing but what goes around every diagram: a title, a legend,
    /// skinparams. Drawn as an empty sequence diagram.
    Empty,
}

/// The type the engine parses `source` as; `None` if it is no `@startuml`
/// diagram or its body cannot be read. Lint, outline and the state
/// machines go by this, so they never read a diagram as another type than
/// the picture shows. If the preprocessor declines, the lines as written
/// decide, so a diagram left to the Java PlantUML still has a type.
pub fn uml_type(source: &str) -> Option<UmlType> {
    let (source::Kind::Uml, raw) = source::body(source).ok()? else {
        return None;
    };
    Some(match preprocess::run(&raw, None) {
        Ok(Some(expanded)) => decide(
            &expanded
                .iter()
                .map(preprocess::Owned::line)
                .collect::<Vec<_>>(),
        ),
        _ => decide(&raw),
    })
}

/// The lines of the first diagram in `source` that may hold its
/// elements, as written, each with its line in `source`: without comments,
/// blank lines, multi-line notes, legends, titles and styles, and without
/// the bodies of classes, objects and skinparams. The engine decides the
/// type by these lines; lint and outline read them so that they skip the
/// same blocks. `None` if the body cannot be read.
pub fn element_lines(source: &str) -> Option<Vec<Line<'_>>> {
    let (_, raw) = source::body(source).ok()?;
    Some(common::deciding(&raw))
}

/// The type of an `@startuml` diagram, by its deciding lines.
fn decide(lines: &[source::Line]) -> UmlType {
    // A one-line title stays in the element lines (the outline reads it)
    // and still does not decide the type: `title Sidewa[*] --> Idle` is
    // title text, and PlantUML draws a class diagram.
    let deciding: Vec<source::Line> = common::deciding(lines)
        .into_iter()
        .filter(|line| !common::one_line_frame(line.text))
        .collect();
    // A timing keyword wins: `rectangle` is a timing row only once the
    // diagram is already a timing diagram, and on its own it stays a
    // structure diagram.
    if timing::is_timing(&deciding) {
        UmlType::Timing
    } else if activity::is_activity(&deciding) {
        UmlType::Activity
    } else if state::is_state(&deciding) {
        UmlType::State
    } else if structure::is_structure(&deciding) {
        UmlType::Structure
    } else if only_frame(lines) {
        UmlType::Empty
    } else {
        UmlType::Sequence
    }
}

/// Whether `lines` hold nothing but the lines every type takes the same
/// way; it stops at the first other line.
fn only_frame(lines: &[source::Line]) -> bool {
    let mut frame = common::Frame::default();
    let mut at = 0;
    while at < lines.len() {
        match frame.line(lines, at) {
            Ok(Some(next)) => at = next,
            _ => return false,
        }
    }
    true
}

/// Parses an `@startuml` diagram as the type its deciding lines name.
fn uml(lines: &[source::Line]) -> Result<Diagram, Declined> {
    Ok(match decide(lines) {
        UmlType::Timing => Diagram::Timing(timing::parse(lines)?),
        UmlType::Activity => Diagram::Activity(activity::parse(lines)?),
        UmlType::State => Diagram::State(state::parse(lines)?),
        UmlType::Structure => Diagram::Structure(structure::parse(lines)?),
        UmlType::Sequence | UmlType::Empty => Diagram::Sequence(sequence::parse(lines)?),
    })
}

/// Draws `source` as SVG, or declines. The renderer uses the Java
/// PlantUML for the other types if it is installed.
pub fn render(source: &str) -> Result<String, Declined> {
    render_with(source, None)
}

/// Draws `source` as SVG, reading its `!include`s through `includes`.
pub fn render_with(source: &str, includes: Option<&dyn Includes>) -> Result<String, Declined> {
    render_mapped(source, includes).map(|(svg, _)| svg)
}

/// Draws `source` as SVG, reading its `!include`s through `includes`, and
/// says where its states, transitions and messages are in it. The map of
/// the other types is empty.
pub fn render_mapped(
    source: &str,
    includes: Option<&dyn Includes>,
) -> Result<(String, map::Map), Declined> {
    let unmapped = |svg: Result<String, Declined>| svg.map(|svg| (svg, map::Map::default()));
    match parse_with(source, includes)? {
        Diagram::Sequence(d) => sequence::draw_mapped(&d),
        Diagram::State(d) => state::draw_mapped(&d),
        Diagram::Activity(d) => unmapped(activity::draw(&d)),
        Diagram::MindMap(d) => unmapped(tree::draw_mindmap(&d)),
        Diagram::Wbs(d) => unmapped(tree::draw_wbs(&d)),
        Diagram::Structure(d) => unmapped(structure::draw(&d)),
        Diagram::Timing(d) => unmapped(timing::draw(&d)),
        Diagram::Gantt(d) => unmapped(gantt::draw(&d)),
        Diagram::Flow(d) => unmapped(flow::draw(&d)),
        Diagram::Json(d) => unmapped(json::draw(&d)),
        Diagram::Formula(d) => unmapped(formula::draw(&d)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(source: &str) -> &'static str {
        match parse(source) {
            Ok(Diagram::Sequence(_)) => "sequence",
            Ok(Diagram::Activity(_)) => "activity",
            Ok(Diagram::State(_)) => "state",
            Ok(Diagram::Structure(_)) => "structure",
            Ok(Diagram::Timing(_)) => "timing",
            Ok(Diagram::Flow(_)) => "flow",
            Ok(_) => "other",
            Err(err) => panic!("{source}: {err:?}"),
        }
    }

    #[test]
    fn the_map_lies_on_the_picture_and_grows_with_scale() {
        let mapped = |head: &str| {
            let source = format!(
                "@startuml\n{head}\n[*] --> Idle\nIdle --> Busy : go\nBusy --> [*]\n@enduml\n"
            );
            render_mapped(&source, None).unwrap()
        };
        let (svg, map) = mapped("");
        let size = |svg: &str, attr: &str| -> f32 {
            let at = svg.find(&format!("{attr}=\"")).unwrap() + attr.len() + 2;
            svg[at..].split('"').next().unwrap().parse().unwrap()
        };
        let (w, h) = (size(&svg, "width"), size(&svg, "height"));
        let names: Vec<&str> = map.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["", "Idle", "Busy", ""]);
        for spot in &map.states {
            let r = spot.rect;
            assert!(
                r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= w && r.y + r.h <= h,
                "{spot:?}"
            );
        }
        assert_eq!(
            map.transitions.iter().map(|t| t.line).collect::<Vec<_>>(),
            [2, 3, 4]
        );
        // `Idle` lies above `Busy`, and the route between them leads down.
        let go = &map.transitions[1].points;
        assert!(go.first().unwrap().1 < go.last().unwrap().1);

        let (_, double) = mapped("scale 2");
        let (a, b) = (map.states[1].rect, double.states[1].rect);
        assert!((b.x - 2.0 * a.x).abs() < 0.5 && (b.w - 2.0 * a.w).abs() < 0.5);

        let (_, seq) =
            render_mapped("@startuml\nA -> B : hi\nB --> A : ok\n@enduml\n", None).unwrap();
        let lines: Vec<usize> = seq.messages.iter().map(|m| m.line).collect();
        assert_eq!(lines, [1, 2]);
        assert!(seq.messages[0].bottom <= seq.messages[1].top + 0.01);
    }

    #[test]
    fn bodies_notes_and_messages_do_not_pick_the_type() {
        for (source, expected) in [
            (
                "class Foo {\n  #id : int\n  state : OrderState\n  Clock clock\n}\nenum Mode {\n  START\n  STOP\n}\nFoo --> Mode",
                "structure",
            ),
            (
                "clock -> server : tick\nserver --> clock : tock",
                "sequence",
            ),
            (
                "Alice -> Bob : hi\nnote over Alice\nbinary data here\nend note",
                "sequence",
            ),
            (
                "[*] --> Analog\nAnalog --> Clock : t\nClock : entry / tick",
                "state",
            ),
            (":Status: OK;\n:Step 2: open the log;", "activity"),
            (":Status: x-ray;", "activity"),
            ("clock clk with period 1\nrobust \"Web\" as W", "timing"),
            ("binary EN\n@0\nEN is high", "timing"),
            (":User: --> (Login)", "structure"),
            (":User: o-- (Login)", "structure"),
        ] {
            let source = format!("@startuml\n{source}\n@enduml\n");
            assert_eq!(kind(&source), expected, "{source}");
        }
    }

    #[test]
    fn a_leading_use_case_or_component_is_a_structure_diagram() {
        for (source, expected) in [
            ("(Login) -- User", "structure"),
            ("[Order] --> [Billing]", "structure"),
            ("[Xserver] ..> [Shop]", "structure"),
            ("[o-> A : in\nA -> B", "sequence"),
            ("[-> A : in", "sequence"),
        ] {
            let source = format!("@startuml\n{source}\n@enduml\n");
            assert_eq!(kind(&source), expected, "{source}");
        }
    }

    #[test]
    fn the_type_without_a_picture() {
        let of = |source: &str| uml_type(&format!("@startuml\n{source}\n@enduml\n"));
        assert_eq!(of("hide empty description\nA --> B"), Some(UmlType::State));
        // The preprocessor declines without the files, the lines still decide.
        assert_eq!(of("!include parts.puml\n[*] --> A"), Some(UmlType::State));
        assert_eq!(of("!$who = \"A\"\n:$who works;"), Some(UmlType::Activity));
        assert_eq!(of(""), Some(UmlType::Empty));
        assert_eq!(
            of("title Soon\nskinparam shadowing false"),
            Some(UmlType::Empty)
        );
        assert_eq!(of("title Soon\nA -> B"), Some(UmlType::Sequence));
        // A `[*]` that only a title holds is the title's text. The arrows
        // beside it are not a state machine; PlantUML draws a class diagram.
        assert_eq!(
            of("left to right direction\ntitle Sidewa[*] --> Idle\nIdle --> Busy : go"),
            Some(UmlType::Sequence)
        );
        assert_eq!(
            of("title Side[*]ways\n[*] --> Idle\nIdle --> Busy"),
            Some(UmlType::State)
        );
        assert_eq!(uml_type("@startmindmap\n* a\n@endmindmap"), None);
        assert_eq!(uml_type("@startsalt\n{\n}\n@endsalt"), None);
    }
}
