//! Broken flowcharts straight into the drawer, and broken state diagrams
//! into the state engine. The renderer catches panics, mmdr's and the
//! drawers' alike, so one here would only show as "mmdr crashed"; without
//! that net a panic fails.

use std::panic::catch_unwind;
use std::time::{Duration, Instant};

use mermaid_rs_renderer::{RenderOptions, parse_mermaid_strict};

use super::super::try_draw;

const SOURCES: [&str; 6] = [
    include_str!("../../../../../omaspec-core/tests/fixtures/flowchart.mmd"),
    include_str!("../../../../../omaspec-core/tests/fixtures/state.mmd"),
    include_str!("../../../../../../examples/shop.mmd"),
    include_str!("../../../../../../examples/lifecycle.mmd"),
    "flowchart TB\n  subgraph O[Outer]\n    subgraph I[Inner]\n      A((a)) --o B{b}\n    end\n    C[/c/] -.-> A\n  end\n  D[(d)] ==> O\n  B -->|yes| D\n  B x--x C\n",
    "stateDiagram-v2\n  [*] --> On\n  state On {\n    [*] --> A\n    A --> B : go\n    --\n    [*] --> C\n  }\n  B --> Off\n  C --> [*]\n",
];

const TOKENS: [&str; 24] = [
    "-->",
    "---",
    "-.->",
    "==>",
    "--o",
    "--x",
    "<-->",
    "|x|",
    "[",
    "]",
    "(",
    ")",
    "{",
    "}",
    "subgraph S",
    "end",
    "direction LR",
    ":::c",
    "[*]",
    "state",
    "\n",
    "A",
    "A --> A",
    "<br/>",
];

/// A small xorshift, so every run tries the same diagrams.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

fn mutate(rng: &mut Rng, mut code: String) -> String {
    for _ in 0..=rng.below(4) {
        let at = (0..=code.len())
            .filter(|i| code.is_char_boundary(*i))
            .nth(rng.below(code.len() + 1))
            .unwrap_or(0);
        if rng.below(3) == 0 {
            let end = (at + rng.below(20)).min(code.len());
            if code.is_char_boundary(end) {
                code.replace_range(at..end, "");
            }
        } else {
            code.insert_str(at, TOKENS[rng.below(TOKENS.len())]);
        }
    }
    code
}

#[test]
fn broken_diagrams_never_panic_the_drawer() {
    let theme = RenderOptions::default().theme;
    let mut rng = Rng(0x5eed_f10c_c4a7_0001);
    let mut drawn = 0;
    // `OMASPEC_ROBUST_ROUNDS` runs more, as the robustness tests do.
    let rounds = std::env::var("OMASPEC_ROBUST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(400);
    for round in 0..rounds {
        let start = SOURCES[rng.below(SOURCES.len())].to_owned();
        let code = mutate(&mut rng, start);
        // mmdr's own panics are not this test's business.
        let Ok(Ok(parsed)) = catch_unwind(|| parse_mermaid_strict(&code)) else {
            continue;
        };
        let started = Instant::now();
        if try_draw(&parsed.graph, &theme, &code, &[], 0.0).is_some() {
            drawn += 1;
        }
        if crate::mermaid::state::try_draw(&theme, &code).is_some() {
            drawn += 1;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "round {round} took {:?}: {code:?}",
            started.elapsed()
        );
    }
    assert!(drawn > 100, "only {drawn} drawn");
}

/// Each Mermaid state diagram the drawer draws and the state machine reader
/// reads: every state of the machine is a state in the picture and the
/// other way round, and every transition is a line in it. Otherwise the
/// checks and the play would go by a machine the picture does not show.
#[test]
fn the_machine_is_the_one_drawn() {
    use omaspec_core::Language;
    use omaspec_model::Machine;

    let mut rng = Rng(0x5eed_f10c_c4a7_0002);
    let rounds = std::env::var("OMASPEC_ROBUST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(400);
    let (mut compared, mut wrong) = (0, Vec::new());
    for round in 0..rounds * 4 {
        let code = match SOURCES.get(round) {
            Some(source) => (*source).to_owned(),
            None => {
                let start = SOURCES[rng.below(SOURCES.len())].to_owned();
                mutate(&mut rng, start)
            }
        };
        let Some(machine) = Machine::read(&code, Language::Mermaid) else {
            continue;
        };
        let Ok(Ok((_, _, map))) = catch_unwind(|| crate::mermaid::render_mapped(&code)) else {
            continue;
        };
        if map.states.is_empty() {
            continue;
        }
        compared += 1;
        let placed = machine.place(&map);
        let mut taken = vec![0; map.states.len()];
        for spot in placed.states.iter().flatten() {
            if let Some(i) = map.states.iter().position(|s| s == spot) {
                taken[i] += 1;
            }
        }
        let missing: Vec<&str> = machine
            .states
            .iter()
            .zip(&placed.states)
            .filter(|(_, spot)| spot.is_none())
            .map(|(s, _)| s.name.as_str())
            .collect();
        let extra: Vec<&str> = map
            .states
            .iter()
            .zip(&taken)
            .filter(|(_, n)| **n != 1)
            .map(|(s, _)| s.name.as_str())
            .collect();
        let unrouted = placed.routes.iter().filter(|r| r.is_none()).count();
        if !missing.is_empty() || !extra.is_empty() || unrouted > 0 {
            wrong.push(format!(
                "round {round}: not drawn {missing:?}, not read {extra:?}, {unrouted} lines not drawn: {code:?}"
            ));
        }
    }
    assert!(compared > 20, "only {compared} compared");
    assert!(
        wrong.is_empty(),
        "{} of {compared}:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
