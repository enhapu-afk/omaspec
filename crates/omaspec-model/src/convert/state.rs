//! State diagrams between the languages: PlantUML's written from the
//! native engine's parse, Mermaid's from Omaspec's own reading. Every
//! state is declared in its own container first, so that both readers make
//! it there; transitions between containers come at the end, at the top,
//! where both languages take states from anywhere.

use std::collections::{HashMap, HashSet};

use omaspec_core::Language;
use omaspec_uml::Diagram;
use omaspec_uml::state::mermaid::{self as chart, Chart};
use omaspec_uml::state::{Container, Dash, Direction, Kind, Node, Side};

use super::{
    Converted, Notice, Refused, is_plain, not_the_same, plain_id, refused, to_mermaid_text,
    to_plantuml_text,
};
use crate::{Machine, across, diff};

const UNREAD: &str = "Omaspec does not fully read this state diagram, so it cannot convert it";

pub(super) fn convert(code: &str, from: Language, to: Language) -> Result<Converted, Refused> {
    let (text, notices, renames) = match from {
        Language::PlantUml => to_mermaid(code)?,
        Language::Mermaid => to_plantuml(code)?,
    };
    let mut original = Machine::read(code, from).ok_or_else(|| refused(None, UNREAD))?;
    for state in &mut original.states {
        if let Some(id) = renames.get(&state.name) {
            state.name.clone_from(id);
        }
    }
    let back = Machine::read(&text, to).ok_or_else(|| not_the_same(to, None))?;
    if let Some(change) = diff::machines(&original, &back).first() {
        return Err(not_the_same(to, Some(change)));
    }
    Ok(Converted {
        code: text,
        notices,
    })
}

/// What a writer gives back: the text, the notices, and the states whose
/// names had to change, by their old names.
type Written = (String, Vec<Notice>, HashMap<String, String>);

#[derive(Default)]
struct ToMermaid {
    out: Vec<String>,
    notices: Vec<Notice>,
    /// Each PlantUML name's Mermaid id.
    ids: HashMap<String, String>,
    renames: HashMap<String, String>,
    /// Transitions between containers, written last at the top.
    crossings: Vec<String>,
}

fn to_mermaid(code: &str) -> Result<Written, Refused> {
    let Ok(Diagram::State(diagram)) = omaspec_uml::parse(code) else {
        return Err(refused(None, UNREAD));
    };
    let mut w = ToMermaid::default();
    let mut taken = HashSet::new();
    w.name_all(&diagram.root, &mut taken);
    let frame = &diagram.frame;
    let title = match &frame.title {
        Some(title) => Some(title.value.clone()),
        None => across::diagram_name(code, Language::PlantUml).inspect(|name| {
            w.notice(
                Some(0),
                format!("The name after @startuml becomes the title {name}, so that sequence diagrams still find this machine"),
            );
        }),
    };
    if let Some(title) = title {
        let title = match title.contains(':') || title.starts_with(['"', '\'']) {
            true => format!("\"{}\"", title.replace('"', "'")),
            false => title,
        };
        w.out
            .extend(["---".into(), format!("title: {title}"), "---".into()]);
    }
    for (what, text) in [
        ("header", &frame.header),
        ("footer", &frame.footer),
        ("caption", &frame.caption),
        ("mainframe", &frame.mainframe),
    ] {
        if let Some(text) = text {
            w.notice(
                Some(text.line),
                format!("Mermaid has no {what}; it is left out"),
            );
        }
    }
    if let Some(legend) = &frame.legend {
        w.notice(
            Some(legend.text.line),
            "Mermaid has no legend; it is left out",
        );
    }
    if let Some(param) = frame.skinparams.first() {
        w.notice(
            Some(param.line),
            "Skin parameters and styles are not carried over",
        );
    }
    w.out.push("stateDiagram-v2".into());
    if diagram.root.direction == Direction::Right {
        w.out.push("    direction LR".into());
    }
    w.container(&diagram.root, 1)?;
    let crossings = std::mem::take(&mut w.crossings);
    w.out.extend(crossings);
    let mut text = w.out.join("\n");
    text.push('\n');
    Ok((text, w.notices, w.renames))
}

impl ToMermaid {
    fn notice(&mut self, line: Option<usize>, message: impl Into<String>) {
        self.notices.push(Notice {
            line,
            message: message.into(),
        });
    }

    /// Gives every named state an id Mermaid takes.
    fn name_all(&mut self, container: &Container, taken: &mut HashSet<String>) {
        for node in &container.nodes {
            if !node.key.is_empty() && !self.ids.contains_key(&node.key) {
                let id = match is_plain(&node.key) && !taken.contains(&node.key) {
                    true => node.key.clone(),
                    false => plain_id(&node.key, taken),
                };
                if id != node.key {
                    self.renames.insert(node.key.clone(), id.clone());
                    self.notice(
                        Some(node.line),
                        format!(
                            "{} is called {id} in Mermaid; its box still shows {}",
                            node.key, node.key
                        ),
                    );
                }
                taken.insert(id.clone());
                self.ids.insert(node.key.clone(), id);
            }
            self.name_all(&node.children, taken);
        }
    }

    fn container(&mut self, c: &Container, depth: usize) -> Result<(), Refused> {
        let pad = "    ".repeat(depth);
        if c.split.is_some() {
            for (i, region) in c.nodes.iter().enumerate() {
                if i > 0 {
                    self.out.push(format!("{pad}--"));
                }
                self.container(&region.children, depth)?;
            }
        } else {
            for node in &c.nodes {
                self.declare(node, depth)?;
            }
        }
        for edge in &c.edges {
            let from = self.end(&c.nodes[edge.from], edge.line)?;
            let to = self.end(&c.nodes[edge.to], edge.line)?;
            let label = self.label(edge.label.as_ref().map(|t| t.value.as_str()), edge.line)?;
            self.look(
                edge.color.is_some() || edge.dash != Dash::Solid || edge.bold,
                edge.line,
            );
            self.out.push(format!("{pad}{from} --> {to}{label}"));
        }
        for crossing in &c.crossings {
            let line = crossing.arrow.line;
            let from = self.end(walk(c, &crossing.from), line)?;
            let to = self.end(walk(c, &crossing.to), line)?;
            if from == "[*]" || to == "[*]" {
                return Err(refused(
                    Some(line),
                    "Mermaid cannot write a transition between [*] and a state in another container",
                ));
            }
            let label = self.label(
                crossing.arrow.label.as_ref().map(|t| t.value.as_str()),
                line,
            )?;
            self.crossings.push(format!("    {from} --> {to}{label}"));
        }
        for note in &c.notes {
            let target = &c.nodes[note.target];
            if target.key.is_empty() {
                self.notice(Some(note.line), "A note on [*] is left out");
                continue;
            }
            let side = match note.side {
                Side::Left => "left",
                Side::Right => "right",
                Side::Top | Side::Bottom => {
                    self.notice(
                        Some(note.line),
                        "Mermaid has notes only left and right of a state; this one goes right",
                    );
                    "right"
                }
            };
            let id = self.ids[&target.key].clone();
            self.out.push(format!("{pad}note {side} of {id}"));
            for line in note.text.value.split("\\n") {
                self.out.push(format!("{pad}    {}", line.trim()));
            }
            self.out.push(format!("{pad}end note"));
        }
        Ok(())
    }

    fn declare(&mut self, node: &Node, depth: usize) -> Result<(), Refused> {
        let pad = "    ".repeat(depth);
        let id = match node.kind {
            Kind::Start | Kind::End | Kind::Region => return Ok(()),
            Kind::History | Kind::DeepHistory => {
                return Err(refused(
                    Some(node.line),
                    "Mermaid has no history state ([H], [H*])",
                ));
            }
            _ => self.ids[&node.key].clone(),
        };
        self.look(
            node.fill.is_some()
                || node.line_color.is_some()
                || node.border_dash != Dash::Solid
                || node.border_bold,
            node.line,
        );
        if let Some(own) = &node.stereotype {
            self.notice(
                Some(node.line),
                format!("Mermaid has no stereotypes; <<{own}>> is left out"),
            );
        }
        let stereotype = match node.kind {
            Kind::Choice => Some("choice"),
            Kind::Fork => Some("fork"),
            Kind::Join => Some("join"),
            _ => None,
        };
        if let Some(stereotype) = stereotype {
            self.out.push(format!("{pad}state {id} <<{stereotype}>>"));
            return Ok(());
        }
        let shown = match node.label.value.is_empty() {
            true => node.key.as_str(),
            false => node.label.value.as_str(),
        };
        let head = match shown != id {
            true => {
                if shown.contains('"') {
                    self.notice(
                        Some(node.line),
                        "Mermaid takes no \" in a name; it is written '",
                    );
                }
                format!(
                    "state \"{}\" as {id}",
                    to_mermaid_text(&shown.replace('"', "'"))
                )
            }
            false if node.composite => format!("state {id}"),
            false => id.clone(),
        };
        if node.composite {
            if depth > 1 && node.children.nodes.is_empty() {
                return Err(refused(
                    Some(node.line),
                    "Mermaid draws an empty state inside another one at the top",
                ));
            }
            if node.children.direction == Direction::Right {
                self.notice(
                    Some(node.line),
                    "Mermaid lays the whole diagram out one way; this state's direction is left out",
                );
            }
            self.out.push(format!("{pad}{head} {{"));
            self.container(&node.children, depth + 1)?;
            self.out.push(format!("{pad}}}"));
        } else {
            self.out.push(format!("{pad}{head}"));
        }
        for description in &node.description {
            let text = self.text(&description.value, description.line);
            self.out.push(format!("{pad}{id} : {text}"));
        }
        Ok(())
    }

    /// A transition's end: `[*]` or the state's id.
    fn end(&mut self, node: &Node, line: usize) -> Result<String, Refused> {
        match node.kind {
            Kind::Start | Kind::End => Ok("[*]".into()),
            Kind::History | Kind::DeepHistory => Err(refused(
                Some(line),
                "Mermaid has no history state ([H], [H*])",
            )),
            _ => Ok(self.ids[&node.key].clone()),
        }
    }

    /// ` : label`, or nothing. A label is behavior: a `;` in it, which
    /// ends a statement in Mermaid, refuses.
    fn label(&mut self, label: Option<&str>, line: usize) -> Result<String, Refused> {
        let Some(label) = label.filter(|l| !l.trim().is_empty()) else {
            return Ok(String::new());
        };
        if label.contains(';') {
            return Err(refused(
                Some(line),
                "Mermaid ends a statement at ;, and this label has one",
            ));
        }
        Ok(format!(" : {}", to_mermaid_text(label)))
    }

    /// A description or note text; `;` becomes `,`.
    fn text(&mut self, text: &str, line: usize) -> String {
        if text.contains(';') {
            self.notice(
                Some(line),
                "Mermaid ends a statement at ;, so it is written ,",
            );
        }
        to_mermaid_text(&text.replace(';', ","))
    }

    fn look(&mut self, styled: bool, line: usize) {
        if styled {
            self.notice(Some(line), "Colors and line styles are not carried over");
        }
    }
}

/// The node at `path` below `c`.
fn walk<'a>(c: &'a Container, path: &[usize]) -> &'a Node {
    let mut node = &c.nodes[path[0]];
    for &step in &path[1..] {
        node = &node.children.nodes[step];
    }
    node
}

#[derive(Default)]
struct ToPlantUml {
    out: Vec<String>,
    notices: Vec<Notice>,
    /// Each Mermaid id's PlantUML name.
    ids: HashMap<String, String>,
    renames: HashMap<String, String>,
}

fn to_plantuml(code: &str) -> Result<Written, Refused> {
    let chart = chart::read(code).ok_or_else(|| refused(None, UNREAD))?;
    let mut w = ToPlantUml::default();
    w.out.push("@startuml".into());
    if let Some(title) = across::title(code, Language::Mermaid) {
        w.out.push(format!("title {title}"));
    }
    match chart.direction {
        Some(chart::Direction::Right) => w.out.push("left to right direction".into()),
        Some(chart::Direction::Left) => {
            w.out.push("left to right direction".into());
            w.notice(
                None,
                "PlantUML lays a state diagram out left to right, not right to left",
            );
        }
        Some(chart::Direction::Up) => {
            w.notice(
                None,
                "PlantUML lays a state diagram out top down, not bottom up",
            );
        }
        Some(chart::Direction::Down) | None => {}
    }
    if !chart.class_defs.is_empty() || !chart.classes.is_empty() || !chart.styles.is_empty() {
        w.notice(None, "classDef, class and style are not carried over");
    }
    w.name_all(&chart);
    w.block(&chart, None, 0)?;
    // Transitions between containers, at the top.
    for t in &chart.transitions {
        let (a, b) = (&chart.states[t.from], &chart.states[t.to]);
        if (a.parent, a.region) == (b.parent, b.region) {
            continue;
        }
        let marker = |s: &chart::State| matches!(s.kind, chart::Kind::Start | chart::Kind::End);
        if marker(a) || marker(b) {
            return Err(refused(
                Some(t.line),
                "PlantUML cannot write a transition between [*] and a state in another container",
            ));
        }
        let line = w.transition(&chart, t);
        w.out.push(line);
    }
    w.out.push("@enduml".into());
    let mut text = w.out.join("\n");
    text.push('\n');
    Ok((text, w.notices, w.renames))
}

impl ToPlantUml {
    fn notice(&mut self, line: Option<usize>, message: impl Into<String>) {
        self.notices.push(Notice {
            line,
            message: message.into(),
        });
    }

    /// Gives every named state a name PlantUML takes in a transition,
    /// which is never quoted.
    fn name_all(&mut self, chart: &Chart) {
        let mut taken = HashSet::new();
        for s in &chart.states {
            if s.id.is_empty() || self.ids.contains_key(&s.id) {
                continue;
            }
            let id = match is_plain(&s.id) && !taken.contains(&s.id) {
                true => s.id.clone(),
                false => plain_id(&s.id, &taken),
            };
            if id != s.id {
                self.renames.insert(s.id.clone(), id.clone());
                self.notice(
                    Some(s.line),
                    format!(
                        "{} is called {id} in PlantUML; its box still shows {}",
                        s.id, s.id
                    ),
                );
            }
            taken.insert(id.clone());
            self.ids.insert(s.id.clone(), id);
        }
    }

    fn name(&self, id: &str) -> String {
        self.ids.get(id).cloned().unwrap_or_else(|| id.to_owned())
    }

    /// The states, transitions and notes of the container `parent`, each
    /// region after a `--`.
    fn block(&mut self, chart: &Chart, parent: Option<usize>, depth: usize) -> Result<(), Refused> {
        let pad = "  ".repeat(depth);
        let regions = chart
            .states
            .iter()
            .filter(|s| s.parent == parent)
            .map(|s| s.region)
            .max()
            .unwrap_or(0);
        for region in 0..=regions {
            if region > 0 {
                self.out.push(format!("{pad}--"));
            }
            let here = |s: &chart::State| s.parent == parent && s.region == region;
            for (i, s) in chart.states.iter().enumerate() {
                if here(s) {
                    self.declare(chart, i, depth)?;
                }
            }
            for t in &chart.transitions {
                if here(&chart.states[t.from]) && here(&chart.states[t.to]) {
                    let line = self.transition(chart, t);
                    self.out.push(format!("{pad}{line}"));
                }
            }
            for note in &chart.notes {
                if !here(&chart.states[note.target]) {
                    continue;
                }
                let side = match note.side {
                    chart::Side::Left => "left",
                    chart::Side::Right => "right",
                };
                let name = self.name(&chart.states[note.target].id);
                self.out.push(format!("{pad}note {side} of {name}"));
                for line in note.text.split('\n') {
                    self.out.push(format!("{pad}  {line}"));
                }
                self.out.push(format!("{pad}end note"));
            }
        }
        Ok(())
    }

    fn declare(&mut self, chart: &Chart, i: usize, depth: usize) -> Result<(), Refused> {
        let pad = "  ".repeat(depth);
        let s = &chart.states[i];
        let stereotype = match s.kind {
            chart::Kind::Start | chart::Kind::End => return Ok(()),
            chart::Kind::Choice => Some("choice"),
            chart::Kind::Fork => Some("fork"),
            chart::Kind::Join => Some("join"),
            chart::Kind::State => None,
        };
        let name = self.name(&s.id);
        // A renamed state shows its Mermaid id.
        let shown = s.label.as_ref().or((name != s.id).then_some(&s.id));
        let head = match (shown, stereotype) {
            (_, Some(stereotype)) => format!("state {name} <<{stereotype}>>"),
            (Some(label), None) => {
                format!("state \"{}\" as {name}", to_plantuml_text(label))
            }
            (None, None) => format!("state {name}"),
        };
        if s.composite {
            self.out.push(format!("{pad}{head} {{"));
            self.block(chart, Some(i), depth + 1)?;
            self.out.push(format!("{pad}}}"));
        } else {
            self.out.push(format!("{pad}{head}"));
        }
        for description in &s.descriptions {
            self.out
                .push(format!("{pad}{name} : {}", to_plantuml_text(description)));
        }
        Ok(())
    }

    fn transition(&self, chart: &Chart, t: &chart::Transition) -> String {
        let end = |s: &chart::State| match s.kind {
            chart::Kind::Start | chart::Kind::End => "[*]".to_owned(),
            _ => self.name(&s.id),
        };
        let label = match t.label.is_empty() {
            true => String::new(),
            false => format!(" : {}", to_plantuml_text(&t.label)),
        };
        format!(
            "{} --> {}{label}",
            end(&chart.states[t.from]),
            end(&chart.states[t.to])
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::convert as both;
    use super::*;

    const PLAYER: &str = "@startuml Player\n[*] --> Idle\nstate \"Screen saver\" as Saver\nIdle --> Busy : job [ready] / start\nstate Busy {\n  [*] --> Work\n  Work --> Check : done\n  --\n  [*] --> Watch\n  Watch --> Watch : tick\n}\nstate pick <<choice>>\nCheck --> pick\npick --> Idle : [ok]\npick --> Saver : [tired]\nSaver --> Idle : wake\nBusy --> Idle : stop\nIdle : waits for work\nnote right of Idle : the start\nIdle --> [*] : off\n@enduml\n";

    #[test]
    fn a_plantuml_machine_reads_the_same_in_mermaid_and_back() {
        let mermaid = both(PLAYER, Language::PlantUml, Language::Mermaid).unwrap();
        assert!(
            mermaid
                .code
                .starts_with("---\ntitle: Player\n---\nstateDiagram-v2\n"),
            "{}",
            mermaid.code
        );
        assert!(
            mermaid.code.contains("state \"Screen saver\" as Saver"),
            "{}",
            mermaid.code
        );
        assert!(
            mermaid.code.contains("    Check --> pick\n"),
            "{}",
            mermaid.code
        );
        assert!(
            mermaid
                .notices
                .iter()
                .any(|n| n.message.contains("@startuml")),
            "{:?}",
            mermaid.notices
        );
        let back = both(&mermaid.code, Language::Mermaid, Language::PlantUml).unwrap();
        let machine = |code, lang| Machine::read(code, lang).unwrap();
        assert!(
            diff::machines(
                &machine(PLAYER, Language::PlantUml),
                &machine(&back.code, Language::PlantUml)
            )
            .is_empty(),
            "{}",
            back.code
        );
    }

    #[test]
    fn names_mermaid_does_not_take_are_renamed_and_shown() {
        let code = "@startuml\n[*] --> Long:wait\nLong:wait --> Done : go\n@enduml\n";
        let mermaid = both(code, Language::PlantUml, Language::Mermaid).unwrap();
        assert!(
            mermaid.code.contains("state \"Long:wait\" as Long_wait"),
            "{}",
            mermaid.code
        );
        assert!(
            mermaid.code.contains("Long_wait --> Done : go"),
            "{}",
            mermaid.code
        );
    }

    #[test]
    fn names_plantuml_does_not_take_are_renamed_and_shown() {
        // PlantUML takes no quoted name in a transition.
        let code = "stateDiagram-v2\n[*] --> long-wait\nstate \"Ready now\" as ready-now\nlong-wait --> ready-now : go\n";
        let plantuml = both(code, Language::Mermaid, Language::PlantUml).unwrap();
        for line in [
            "state \"long-wait\" as long_wait",
            "state \"Ready now\" as ready_now",
            "[*] --> long_wait",
            "long_wait --> ready_now : go",
        ] {
            assert!(plantuml.code.contains(line), "{line}:\n{}", plantuml.code);
        }
        assert!(
            !plantuml.code.contains("\"long-wait\" -->"),
            "{}",
            plantuml.code
        );
        assert!(
            plantuml.notices.iter().any(|n| n.message
                == "long-wait is called long_wait in PlantUML; its box still shows long-wait"),
            "{:?}",
            plantuml.notices
        );
    }

    #[test]
    fn behavior_mermaid_cannot_say_refuses() {
        let history = "@startuml\n[*] --> On\nstate On {\n[*] --> A\nA --> B : x\n}\nOn --> Off : pause\nOff --> On[H] : resume\n@enduml\n";
        let err = both(history, Language::PlantUml, Language::Mermaid).unwrap_err();
        assert!(err.reason.contains("history"), "{err:?}");
        let semicolon = "@startuml\n[*] --> A\nA --> B : x / a; b\n@enduml\n";
        let err = both(semicolon, Language::PlantUml, Language::Mermaid).unwrap_err();
        assert_eq!(err.line, Some(2), "{err:?}");
    }

    #[test]
    fn looks_are_left_out_and_told() {
        let code = "@startuml\nskinparam state { BackgroundColor pink }\n[*] --> A\nstate A #pink\nA -[#red]-> B : go\nnote top of B : up here\n@enduml\n";
        let mermaid = both(code, Language::PlantUml, Language::Mermaid).unwrap();
        let said: Vec<&str> = mermaid.notices.iter().map(|n| n.message.as_str()).collect();
        assert!(said.iter().any(|m| m.contains("Colors")), "{said:?}");
        assert!(
            said.iter().any(|m| m.contains("only left and right")),
            "{said:?}"
        );
        assert!(
            said.iter().any(|m| m.contains("Skin parameters")),
            "{said:?}"
        );
    }

    #[test]
    fn a_mermaid_machine_reads_the_same_in_plantuml() {
        let code = "---\ntitle: Lamp\n---\nstateDiagram-v2\n    direction LR\n    [*] --> Off\n    Off --> On : press\n    state On {\n        [*] --> Dim\n        Dim --> Bright : press\n    }\n    On --> Off : hold\n    Bright --> Off : timeout\n    note left of Off\n        saves power\n    end note\n    classDef hot fill:#f00\n    class On hot\n";
        let plantuml = both(code, Language::Mermaid, Language::PlantUml).unwrap();
        assert!(
            plantuml
                .code
                .contains("title Lamp\nleft to right direction\n"),
            "{}",
            plantuml.code
        );
        assert!(
            plantuml.code.contains("Bright --> Off : timeout\n@enduml"),
            "{}",
            plantuml.code
        );
        assert!(
            plantuml
                .notices
                .iter()
                .any(|n| n.message.contains("classDef")),
            "{:?}",
            plantuml.notices
        );
    }
}
