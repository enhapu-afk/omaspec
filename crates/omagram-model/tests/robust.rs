//! Robustness: mutated state diagrams in both languages through the
//! specification check, and mutated traces against them, from a fixed seed
//! so a failure repeats. Nothing may panic, and every finding names a line
//! inside the text.
//!
//! `OMAGRAM_ROBUST_ROUNDS` sets how many inputs to try (default 1000).

use omagram_core::Language;

/// xorshift64*: small, fast, the same everywhere.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const PLANTUML: &str = "@startuml\n[*] --> Idle\nIdle --> Busy : job [free] / start\nIdle --> Busy : job\nstate Busy {\n  [*] --> Work\n  --\n  [*] --> Watch\n}\nstate c <<choice>>\nBusy --> c : done\nc --> Idle : [ok]\nc --> [*] : [else]\nstate Lost\n@enduml\n";

const MERMAID: &str = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : job [free] / start\n  state Busy {\n    [*] --> Work\n    --\n    [*] --> Watch\n  }\n  state c <<choice>>\n  Busy --> c : done\n  c --> Idle : [ok]\n  c --> [*]\n  note right of Idle : waits\n  Lost : forgotten\n";

const SEQUENCE: &str = "@startuml\nactor User\nparticipant Busy\nUser -> Busy : job\nalt ok\n  User -> Busy : done\nelse no\n  Busy --> User : no\nend\nloop again\n  User -> Busy : job\nend\npar\n  User -> Busy : done\nend\nref over Busy : more\n[-> Busy : done\nBusy -> Busy : tick\n@enduml\n";

const MERMAID_SEQUENCE: &str = "sequenceDiagram\n  actor User\n  participant Busy\n  User->>Busy: job\n  alt ok\n    User->>Busy: done()\n  else no\n    Busy-->>User: no\n  end\n  loop again\n    User->>Busy: job\n  end\n";

const SEQUENCE_TOKENS: &[&str] = &[
    "->",
    "-->",
    "->>",
    "-x",
    "alt ",
    "else",
    "end",
    "loop",
    "opt",
    "par",
    "and",
    "break",
    "ref over Busy : x",
    "create ",
    "Busy",
    "User",
    ":",
    "(",
    ")",
    "\n",
    "[->",
    "return",
];

/// A scenario Mermaid can say too: no `ref`, no message from the edge.
const PLAIN_SEQUENCE: &str = "@startuml\nactor User\nparticipant Busy\nUser -> Busy : job\nactivate Busy\nalt ok\n  User -> Busy : done\nelse no\n  Busy --> User : no\nend\nloop again\n  User ->> Busy : job\nend\npar\n  User -> Busy : done\nend\ndeactivate Busy\nBusy -> Busy : tick\nnote over Busy : quiet\n@enduml\n";

const TRACE: &str = "{\"time\":\"10:30:01\",\"object\":\"Busy\",\"event\":\"job\"}\n{\"object\":\"Busy\",\"event\":\"done\",\"state\":\"Idle\"}\n{\"object\":\"b2\",\"machine\":\"Busy\",\"state\":\"Watch\"}\n{\"object\":\"b2\",\"event\":\"done\"}\n";

const TRACE_TOKENS: &[&str] = &[
    "{",
    "}",
    "\"",
    ":",
    ",",
    "\n",
    "\"object\":\"Busy\"",
    "\"event\":\"job\"",
    "\"event\":\"done\"",
    "\"state\":\"Work\"",
    "\"state\":\"Lost\"",
    "\"machine\":\"Busy\"",
    "null",
    "17",
    "[]",
    "é",
];

const TOKENS: &[&str] = &[
    "[*]",
    "-->",
    "->",
    "state ",
    "{",
    "}",
    "--",
    "||",
    ":",
    "[",
    "]",
    "/",
    ",",
    "<<choice>>",
    "<<fork>>",
    "<<join>>",
    "[H]",
    "[H*]",
    "\"",
    "as ",
    "note ",
    "end note",
    "\n",
    " ",
    "A",
    "B",
    "é",
    ";",
    ":::x",
    "%%",
    "!",
    "@enduml",
    "stateDiagram",
];

/// `start` with tokens put in, pieces cut out and lines doubled.
fn mutated(rng: &mut Rng, start: &str) -> String {
    let times = rng.below(5) + 1;
    mutated_times(rng, start, times)
}

/// `start` changed in `times` places.
fn mutated_times(rng: &mut Rng, start: &str, times: usize) -> String {
    let mut code = start.to_owned();
    for _ in 0..times {
        let mut at = rng.below(code.len() + 1);
        while !code.is_char_boundary(at) {
            at -= 1;
        }
        match rng.below(3) {
            0 => code.insert_str(at, TOKENS[rng.below(TOKENS.len())]),
            1 => {
                let mut end = (at + rng.below(12)).min(code.len());
                while !code.is_char_boundary(end) {
                    end -= 1;
                }
                code.replace_range(at..end, "");
            }
            _ => {
                let line = code
                    .lines()
                    .nth(rng.below(code.lines().count()))
                    .unwrap_or("");
                let line = format!("{line}\n");
                code.insert_str(at, &line);
            }
        }
    }
    code
}

/// Each PlantUML state diagram the engine draws: every state of its
/// machine is on the picture's map once, and every transition is a line
/// on it, so the play shows each step it takes.
#[test]
fn the_machine_is_the_one_drawn() {
    let rounds = std::env::var("OMAGRAM_ROBUST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let starts = [
        PLANTUML,
        include_str!("../../omagram-core/tests/fixtures/state-player.puml"),
        include_str!("../../../examples/player.puml"),
    ];
    let mut rng = Rng(0xd2a3_5eed_0000_0005);
    let (mut compared, mut wrong) = (0, Vec::new());
    for round in 0..rounds {
        let code = match starts.get(round) {
            Some(start) => (*start).to_owned(),
            None => {
                let start = starts[rng.below(starts.len())];
                mutated(&mut rng, start)
            }
        };
        let Some(machine) = omagram_model::Machine::read(&code, Language::PlantUml) else {
            continue;
        };
        let Ok((_, map)) = omagram_uml::render_mapped(&code, None) else {
            continue;
        };
        compared += 1;
        let placed = machine.place(&map);
        let mut taken = vec![0; map.states.len()];
        for spot in placed.states.iter().flatten() {
            if let Some(i) = map.states.iter().position(|s| s == spot) {
                taken[i] += 1;
            }
        }
        if placed.states.iter().any(Option::is_none)
            || placed.routes.iter().any(Option::is_none)
            || taken.iter().any(|&n| n != 1)
        {
            wrong.push(format!("round {round}: {code:?}"));
        }
    }
    assert!(compared > 50, "only {compared} compared");
    assert!(
        wrong.is_empty(),
        "{} of {compared}:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn the_check_survives_broken_state_diagrams() {
    let rounds = std::env::var("OMAGRAM_ROBUST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let mut rng = Rng(0x5eed_cafe_f00d_beef);
    for round in 0..rounds {
        let (start, lang) = if rng.below(2) == 0 {
            (PLANTUML, Language::PlantUml)
        } else {
            (MERMAID, Language::Mermaid)
        };
        let code = mutated(&mut rng, start);
        let lines = code.split('\n').count();
        for f in omagram_model::findings_with(&code, lang, &[]) {
            assert!(
                f.line < lines,
                "round {round}: line {} of {lines}: {code:?}",
                f.line
            );
        }
    }
}

#[test]
fn the_check_across_diagrams_survives_broken_sequences() {
    let rounds = std::env::var("OMAGRAM_ROBUST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let specs: Vec<omagram_model::Spec> =
        [(PLANTUML, Language::PlantUml), (MERMAID, Language::Mermaid)]
            .iter()
            .map(|&(code, lang)| omagram_model::Spec::read(code, lang, "busy.puml").unwrap())
            .collect();
    let mut rng = Rng(0xfeed_5eed_0000_0001);
    for round in 0..rounds {
        let (start, lang) = if rng.below(2) == 0 {
            (SEQUENCE, Language::PlantUml)
        } else {
            (MERMAID_SEQUENCE, Language::Mermaid)
        };
        let mut code = start.to_owned();
        for _ in 0..=rng.below(5) {
            let mut at = rng.below(code.len() + 1);
            while !code.is_char_boundary(at) {
                at -= 1;
            }
            if rng.below(2) == 0 {
                code.insert_str(at, SEQUENCE_TOKENS[rng.below(SEQUENCE_TOKENS.len())]);
            } else {
                let mut end = (at + rng.below(12)).min(code.len());
                while !code.is_char_boundary(end) {
                    end -= 1;
                }
                code.replace_range(at..end, "");
            }
        }
        let lines = code.split('\n').count();
        // Both specs carry the name `busy`: the participant is matched to
        // none of them. Each alone is matched.
        for specs in [&specs[..], &specs[..1], &specs[1..]] {
            for f in omagram_model::findings_with(&code, lang, specs) {
                assert!(
                    f.line < lines,
                    "round {round}: line {} of {lines}: {code:?}",
                    f.line
                );
            }
        }
    }
}

#[test]
fn runs_survive_broken_state_diagrams() {
    let rounds = std::env::var("OMAGRAM_ROBUST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let mut rng = Rng(0x0dd5_7a7e_5eed_0003);
    for _ in 0..rounds {
        let (start, lang) = if rng.below(2) == 0 {
            (PLANTUML, Language::PlantUml)
        } else {
            (MERMAID, Language::Mermaid)
        };
        let mut code = start.to_owned();
        for _ in 0..=rng.below(5) {
            let mut at = rng.below(code.len() + 1);
            while !code.is_char_boundary(at) {
                at -= 1;
            }
            code.insert_str(at, TOKENS[rng.below(TOKENS.len())]);
        }
        let Some(machine) = omagram_model::Machine::read(&code, lang) else {
            continue;
        };
        let Some((mut run, _)) = omagram_model::Run::start(&machine) else {
            continue;
        };
        for _ in 0..30 {
            let options = run.options(&machine);
            if options.is_empty() {
                break;
            }
            let steps = run.take(&machine, options[rng.below(options.len())]);
            for step in steps {
                assert!(step.transition < machine.transitions.len());
                assert!(step.entered.iter().all(|&s| s < machine.states.len()));
            }
            assert!(run.active().iter().all(|&s| s < machine.states.len()));
        }
    }
}

#[test]
fn the_trace_check_survives_broken_traces() {
    let rounds = std::env::var("OMAGRAM_ROBUST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let specs: Vec<omagram_model::Spec> =
        [(PLANTUML, Language::PlantUml), (MERMAID, Language::Mermaid)]
            .iter()
            .map(|&(code, lang)| omagram_model::Spec::read(code, lang, "busy.puml").unwrap())
            .collect();
    let mut rng = Rng(0x7ace_5eed_0000_0004);
    for round in 0..rounds {
        let mut trace = TRACE.to_owned();
        for _ in 0..=rng.below(5) {
            let mut at = rng.below(trace.len() + 1);
            while !trace.is_char_boundary(at) {
                at -= 1;
            }
            if rng.below(2) == 0 {
                trace.insert_str(at, TRACE_TOKENS[rng.below(TRACE_TOKENS.len())]);
            } else {
                let mut end = (at + rng.below(12)).min(trace.len());
                while !trace.is_char_boundary(end) {
                    end -= 1;
                }
                trace.replace_range(at..end, "");
            }
        }
        let lines = trace.split('\n').count();
        // Both specs carry the name `busy`: no object is matched. Each
        // alone is.
        for specs in [&specs[..], &specs[..1], &specs[1..]] {
            for f in omagram_model::check_trace(&trace, specs) {
                assert!(
                    f.line < lines,
                    "round {round}: line {} of {lines}: {trace:?}",
                    f.line
                );
            }
        }
    }
}

fn rounds() -> usize {
    std::env::var("OMAGRAM_ROBUST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000)
}

fn other(lang: Language) -> Language {
    match lang {
        Language::PlantUml => Language::Mermaid,
        Language::Mermaid => Language::PlantUml,
    }
}

/// Mutated state machines and scenarios in both languages, written in the
/// other one: nothing panics, a refusal names a line inside the text, and
/// what is converted means the same, there and back again.
#[test]
fn conversions_keep_the_behavior() {
    let mut rng = Rng(0x5eed_c0de_0f00_0001);
    let starts = [
        (PLANTUML, Language::PlantUml),
        (MERMAID, Language::Mermaid),
        (PLAIN_SEQUENCE, Language::PlantUml),
        (MERMAID_SEQUENCE, Language::Mermaid),
    ];
    let rounds = rounds();
    let mut converted = 0;
    for round in 0..rounds {
        let (start, lang) = starts[rng.below(starts.len())];
        let code = match round < starts.len() {
            true => starts[round].0.to_owned(),
            false => mutated_times(&mut rng, start, 1),
        };
        let lang = match round < starts.len() {
            true => starts[round].1,
            false => lang,
        };
        let lines = code.split('\n').count();
        let to = other(lang);
        let result = omagram_model::convert(&code, lang, to);
        let written = match result {
            Ok(written) => written,
            Err(refused) => {
                assert!(
                    refused.line.is_none_or(|l| l < lines),
                    "round {round}: {refused:?} for {code:?}"
                );
                continue;
            }
        };
        converted += 1;
        for notice in &written.notices {
            assert!(
                notice.line.is_none_or(|l| l < lines),
                "round {round}: {notice:?} for {code:?}"
            );
        }
        // A renamed state or participant is one gone and one new.
        if written
            .notices
            .iter()
            .any(|n| n.message.contains(" is called "))
        {
            continue;
        }
        let changes = omagram_model::diff(&code, lang, &written.code, to, &[])
            .unwrap_or_else(|e| panic!("round {round}: {e} for {code:?} → {:?}", written.code));
        assert!(
            changes.is_empty(),
            "round {round}: {changes:#?} for {code:?} → {:?}",
            written.code
        );
        if let Ok(back) = omagram_model::convert(&written.code, to, lang)
            && !back
                .notices
                .iter()
                .any(|n| n.message.contains(" is called "))
        {
            let changes = omagram_model::diff(&code, lang, &back.code, lang, &[]).unwrap();
            assert!(
                changes.is_empty(),
                "round {round}: back: {changes:#?} for {code:?} → {:?}",
                back.code
            );
        }
    }
    assert!(
        converted * 4 > rounds,
        "only {converted} of {rounds} converted"
    );
}

/// Two mutated versions of one diagram, compared: nothing panics, a
/// version compared with itself has no change, and every change names
/// lines inside its versions.
#[test]
fn diffs_survive_broken_diagrams() {
    let mut rng = Rng(0x5eed_d1ff_0000_0001);
    let starts = [
        (PLANTUML, Language::PlantUml),
        (MERMAID, Language::Mermaid),
        (PLAIN_SEQUENCE, Language::PlantUml),
        (MERMAID_SEQUENCE, Language::Mermaid),
    ];
    let mut compared = 0;
    for round in 0..rounds() {
        let (start, lang) = starts[rng.below(starts.len())];
        // Mostly one version as written and one changed.
        let old = match rng.below(4) {
            0 => mutated_times(&mut rng, start, 1),
            _ => start.to_owned(),
        };
        let times = 1 + rng.below(2);
        let new = mutated_times(&mut rng, start, times);
        if let Ok(same) = omagram_model::diff(&old, lang, &old, lang, &[]) {
            assert!(same.is_empty(), "round {round}: {same:#?} for {old:?}");
        }
        let Ok(changes) = omagram_model::diff(&old, lang, &new, lang, &[]) else {
            continue;
        };
        compared += 1;
        let (old_lines, new_lines) = (old.split('\n').count(), new.split('\n').count());
        for change in changes {
            assert!(
                change.old_line.is_none_or(|l| l < old_lines)
                    && change.new_line.is_none_or(|l| l < new_lines)
                    && !change.message.is_empty(),
                "round {round}: {change:?}"
            );
        }
    }
    assert!(compared * 6 > rounds(), "only {compared} compared");
}
