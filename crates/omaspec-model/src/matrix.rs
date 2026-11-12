//! A state machine as a table (`omaspec matrix`): a row for each state,
//! a column for each event, in each cell what the event does there. It
//! is the classic state-transition table of a review or a test design;
//! lint already says what conflicts and what no path reaches, so an
//! empty cell is no finding: most are empty.
//!
//! A cell holds the transitions from the state on the event, several
//! with their guards; where the state has none, those of the nearest
//! state around it that has some (`inherited`, as a composite state's
//! transition leaves from every state inside it); `conflict` where two
//! ways have no guard to choose between them. Transitions without an
//! event, and those from a start, a choice or a fork, have no column.

use crate::across::key;
use crate::{Kind, Machine};

/// What an event does in a state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Transition,
    Inherited,
    Conflict,
    Empty,
}

impl CellKind {
    pub fn name(self) -> &'static str {
        match self {
            CellKind::Transition => "transition",
            CellKind::Inherited => "inherited",
            CellKind::Conflict => "conflict",
            CellKind::Empty => "empty",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub kind: CellKind,
    /// The transitions it fires, by their places in the machine.
    pub ways: Vec<usize>,
    /// For an inherited cell, the state around whose transitions they are.
    pub from: Option<usize>,
}

/// The table of a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matrix {
    /// The rows: the states with a name, in the order they are written.
    pub states: Vec<usize>,
    /// The columns: each event once, as first written.
    pub events: Vec<String>,
    /// `cells[row][column]`.
    pub cells: Vec<Vec<Cell>>,
}

/// The table of `m`.
pub fn matrix(m: &Machine) -> Matrix {
    let states: Vec<usize> = (0..m.states.len())
        .filter(|&s| m.states[s].kind == Kind::State && !m.states[s].name.is_empty())
        .collect();
    let mut events: Vec<String> = Vec::new();
    for t in &m.transitions {
        if m.states[t.from].kind != Kind::State {
            continue;
        }
        for event in &t.trigger.events {
            if !events.iter().any(|e| key(e) == key(event)) {
                events.push(event.clone());
            }
        }
    }
    let ways = |state: usize, event: &str| -> Vec<usize> {
        (0..m.transitions.len())
            .filter(|&t| {
                m.transitions[t].from == state
                    && m.transitions[t]
                        .trigger
                        .events
                        .iter()
                        .any(|e| key(e) == key(event))
            })
            .collect()
    };
    let cells = states
        .iter()
        .map(|&state| {
            events
                .iter()
                .map(|event| {
                    let own = ways(state, event);
                    let (ways, from) = if own.is_empty() {
                        m.ancestors(state)
                            .map(|up| (ways(up, event), Some(up)))
                            .find(|(ways, _)| !ways.is_empty())
                            .unwrap_or((Vec::new(), None))
                    } else {
                        (own, None)
                    };
                    let unguarded = ways
                        .iter()
                        .filter(|&&t| m.transitions[t].trigger.guard.is_none())
                        .count();
                    let kind = match (ways.is_empty(), from.is_some()) {
                        (true, _) => CellKind::Empty,
                        _ if unguarded >= 2 => CellKind::Conflict,
                        (false, true) => CellKind::Inherited,
                        (false, false) => CellKind::Transition,
                    };
                    Cell { kind, ways, from }
                })
                .collect()
        })
        .collect();
    Matrix {
        states,
        events,
        cells,
    }
}

impl Matrix {
    /// What a cell says, as a table shows it: `→ Open [allowed] / beep`,
    /// several apart by `;`, an inherited one with the state it comes
    /// from, `↑ Off (from On)`, a conflict marked.
    pub fn said(&self, m: &Machine, row: usize, column: usize) -> String {
        let cell = &self.cells[row][column];
        let ways: Vec<String> = cell.ways.iter().map(|&t| way(m, t)).collect();
        let ways = ways.join("; ");
        match cell.kind {
            CellKind::Empty => String::new(),
            CellKind::Transition => ways,
            CellKind::Conflict => format!("⚠ {ways}"),
            CellKind::Inherited => {
                let from = cell.from.map_or("", |s| m.states[s].name.as_str());
                format!("↑ {ways} (from {from})")
            }
        }
    }

    /// The table to draw ([`omaspec_uml::table`]): a line for each way of
    /// a cell, which stands for its transition's line; an inherited cell
    /// in italics with the state it comes from under it, a conflict framed.
    pub fn table(&self, m: &Machine) -> omaspec_uml::table::Table {
        use omaspec_uml::table::{Cell as Drawn, Row, Table, Tone};
        let rows = self
            .states
            .iter()
            .enumerate()
            .map(|(row, &state)| Row {
                head: m.states[state].name.clone(),
                depth: m
                    .ancestors(state)
                    .filter(|&up| !m.states[up].name.is_empty())
                    .count(),
                cells: self.cells[row]
                    .iter()
                    .map(|cell| {
                        let mut lines: Vec<(String, Option<usize>)> = cell
                            .ways
                            .iter()
                            .map(|&t| (way(m, t), Some(m.transitions[t].line)))
                            .collect();
                        if cell.kind == CellKind::Conflict
                            && let Some((first, _)) = lines.first_mut()
                        {
                            *first = format!("⚠ {first}");
                        }
                        Drawn {
                            tone: match cell.kind {
                                CellKind::Conflict => Tone::Warned,
                                CellKind::Inherited => Tone::Borrowed,
                                _ => Tone::Plain,
                            },
                            lines,
                            from: (cell.kind == CellKind::Inherited)
                                .then(|| cell.from.map(|s| format!("from {}", m.states[s].name)))
                                .flatten(),
                        }
                    })
                    .collect(),
            })
            .collect();
        Table {
            corner: "state".into(),
            columns: self.events.clone(),
            rows,
        }
    }

    /// The table in Markdown: the states down, the events across.
    pub fn markdown(&self, m: &Machine) -> String {
        let escape = |text: &str| text.replace('|', "\\|");
        let mut out = String::from("| state |");
        for event in &self.events {
            out.push_str(&format!(" {} |", escape(event)));
        }
        out.push_str("\n|---|");
        for _ in &self.events {
            out.push_str("---|");
        }
        out.push('\n');
        for (row, &state) in self.states.iter().enumerate() {
            out.push_str(&format!("| {} |", escape(&m.states[state].name)));
            for column in 0..self.events.len() {
                out.push_str(&format!(" {} |", escape(&self.said(m, row, column))));
            }
            out.push('\n');
        }
        out
    }
}

/// What the transition `t` does: `→ Open [allowed] / beep`.
fn way(m: &Machine, t: usize) -> String {
    let t = &m.transitions[t];
    let target = match m.states[t.to].name.as_str() {
        "" => "[*]",
        name => name,
    };
    let mut text = format!("→ {target}");
    if let Some(guard) = &t.trigger.guard {
        text.push_str(&format!(" [{guard}]"));
    }
    if let Some(action) = &t.trigger.action {
        text.push_str(&format!(" / {action}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use omaspec_core::Language;

    use super::*;

    const BREWER: &str = "@startuml\n[*] --> Off\nOff --> On : power\nstate On {\n  [*] --> Idle\n  Idle --> Brewing : brew [water]\n  Idle --> Empty : brew [no water]\n  Brewing --> Idle : done\n  Brewing --> Idle : cancel\n  Brewing --> Empty : cancel\n}\nOn --> Off : power\n@enduml\n";

    #[test]
    fn each_cell_says_what_the_event_does_in_the_state() {
        let m = Machine::read(BREWER, Language::PlantUml).unwrap();
        let table = matrix(&m);
        let names: Vec<&str> = table
            .states
            .iter()
            .map(|&s| m.states[s].name.as_str())
            .collect();
        assert_eq!(names, ["Off", "On", "Idle", "Brewing", "Empty"]);
        assert_eq!(table.events, ["power", "brew", "done", "cancel"]);
        let cell = |state: &str, event: &str| {
            let row = names.iter().position(|n| *n == state).unwrap();
            let column = table.events.iter().position(|e| e == event).unwrap();
            (table.cells[row][column].kind, table.said(&m, row, column))
        };
        assert_eq!(cell("Off", "power"), (CellKind::Transition, "→ On".into()));
        assert_eq!(cell("Off", "brew"), (CellKind::Empty, String::new()));
        assert_eq!(
            cell("Idle", "brew"),
            (
                CellKind::Transition,
                "→ Brewing [water]; → Empty [no water]".into()
            )
        );
        assert_eq!(
            cell("Idle", "power"),
            (CellKind::Inherited, "↑ → Off (from On)".into())
        );
        assert_eq!(cell("Brewing", "cancel").0, CellKind::Conflict);
        assert!(cell("Brewing", "cancel").1.starts_with("⚠ → Idle; → Empty"));
        // To draw: a line for each way, on the line of its transition.
        let drawn = table.table(&m);
        assert_eq!(drawn.columns, ["power", "brew", "done", "cancel"]);
        let idle = &drawn.rows[2];
        assert_eq!((idle.head.as_str(), idle.depth), ("Idle", 1));
        assert_eq!(
            idle.cells[1].lines,
            [
                ("→ Brewing [water]".to_owned(), Some(5)),
                ("→ Empty [no water]".to_owned(), Some(6))
            ]
        );
        assert_eq!(idle.cells[0].tone, omaspec_uml::table::Tone::Borrowed);
        assert_eq!(idle.cells[0].from.as_deref(), Some("from On"));
        let brewing = &drawn.rows[3];
        assert_eq!(brewing.cells[3].tone, omaspec_uml::table::Tone::Warned);
        assert!(brewing.cells[3].lines[0].0.starts_with("⚠ → Idle"));
        assert!(drawn.rows[0].cells[1].lines.is_empty());
        let md = table.markdown(&m);
        assert!(
            md.starts_with("| state | power | brew | done | cancel |\n|---|---|---|---|---|\n"),
            "{md}"
        );
        assert!(
            md.contains(
                "| Idle | ↑ → Off (from On) | → Brewing [water]; → Empty [no water] |  |  |"
            ),
            "{md}"
        );
    }
}
