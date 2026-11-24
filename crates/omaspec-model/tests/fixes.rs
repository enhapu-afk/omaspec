//! Every rule that can say its fix says it, and each fix applied makes
//! its finding go away without bringing an error (ROADMAP, "Findings with
//! their fix"; [`omaspec_core::lint::RULES`]).

use omaspec_core::Language::{self, Mermaid, PlantUml};
use omaspec_core::lint::{Finding, RULES, Severity, fix_all, rule_of, share};

/// For each rule whose findings carry a fix, diagrams that bring one.
const EXAMPLES: &[(&str, Language, &str)] = &[
    ("missing-end", PlantUml, "@startuml\nA -> B : hi\n"),
    // The first of two diagrams ends before the second, its block too.
    (
        "missing-end",
        PlantUml,
        "@startuml\nA -> B : hi\nalt ok\n  B -> A : yes\n@startuml\nC -> D : hi\n@enduml\n",
    ),
    (
        "unclosed-block",
        PlantUml,
        "@startuml\nA -> B : hi\nalt ok\n  B -> A : yes\n@enduml\n",
    ),
    (
        "unclosed-block",
        PlantUml,
        "@startuml\nA -> B : hi\ngroup Talk {\n  loop again\n    B -> A : yes\n@enduml\n",
    ),
    (
        "unclosed-block",
        PlantUml,
        "@startuml\n|Desk|\nstart\ngroup Talk {\n  if (ok?) then (yes)\n    :go;\n@enduml\n",
    ),
    (
        "unclosed-block",
        PlantUml,
        "@startuml\nstart\nif (ok?) then (yes)\n  :go;\nelse (no)\n  :wait;\n@enduml\n",
    ),
    (
        "unclosed-block",
        PlantUml,
        "@startuml\nA -> B : hi\nnote over A\n  a note\n",
    ),
    (
        "unclosed-block",
        PlantUml,
        "@startuml\nstart\ngroup Talk\n  :hi;\n  partition Inner {\n    :yes;\nstop\n@enduml\n",
    ),
    (
        "unclosed-block",
        PlantUml,
        "@startuml\nstart\nwhile (more?)\n  fork\n    :a;\n  fork again\n    :b;\n@enduml\n",
    ),
    (
        "unclosed-block",
        Mermaid,
        "sequenceDiagram\n  A->>B: hi\n  loop every minute\n    B->>A: tick\n",
    ),
    ("unknown-head", Mermaid, "sequenceDiagam\n  A->>B: hi\n"),
    (
        "declared-twice",
        PlantUml,
        "@startuml\nparticipant Alice\nparticipant Bob\nparticipant Alice\nAlice -> Bob : hi\n@enduml\n",
    ),
    (
        "declared-twice",
        Mermaid,
        "sequenceDiagram\n  participant Alice\n  participant Bob\n  participant Alice\n  Alice->>Bob: hi\n",
    ),
    (
        "undeclared-participant",
        PlantUml,
        "@startuml\nparticipant Alice\nparticipant Bob\nAlcie -> Bob : hi\nAlice -> Bob : yes\n@enduml\n",
    ),
    (
        "lane-spelling",
        PlantUml,
        "@startuml\n|Sales|\nstart\n:order;\n|sales |\n:pay;\n|Sales|\n:ship;\nstop\n@enduml\n",
    ),
    (
        "dead-end",
        PlantUml,
        "@startuml\n[*] --> Idle\nIdle --> Busy : go\n@enduml\n",
    ),
    (
        "dead-end",
        Mermaid,
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : go\n",
    ),
    (
        "transition-twice",
        PlantUml,
        "@startuml\n[*] --> Idle\nIdle --> Busy : go\nBusy --> Idle : done\nIdle --> Busy : go\n@enduml\n",
    ),
    (
        "stale-entry",
        PlantUml,
        "@startuml\n[*] --> Idle\nIdle --> Busy : go\nBusy --> Idle : done\n' lines by hand (Omaspec)\n' Idle -> Busy at 0.5 -0.4\n' Idle -> Busy at 0.6 -0.4\n@enduml\n",
    ),
    (
        "unknown-icon",
        PlantUml,
        "@startuml\nA -> B : <&camera> a picture\n@enduml\n",
    ),
    (
        "faint-color",
        PlantUml,
        "@startuml\nskinparam stateBackgroundColor #EEEEEE\nskinparam stateFontColor #DDDDDD\n[*] --> Idle\nIdle --> [*]\n@enduml\n",
    ),
    (
        "faint-color",
        Mermaid,
        "flowchart LR\n  A --> B\n  classDef soft fill:#f9f9f9,stroke:#333,color:#eeeeee;\n  class A soft\n",
    ),
    (
        "unseen-character",
        PlantUml,
        "@startuml\nA -> B : h\u{200b}i\n@enduml\n",
    ),
    ("not-mermaid-js", Mermaid, "SequenceDiagram\n  A->>B: hi\n"),
    ("not-mermaid-js", Mermaid, "flowchart TD\n  A[a|b] --> B\n"),
];

/// The fixes made as a reader of `lint --json` or MCP `lint` is told to:
/// from the last line up, several at one line in the order of their
/// findings, each as it comes.
fn by_hand(text: &str, findings: &[Finding]) -> String {
    let mut fixes: Vec<_> = findings.iter().filter_map(|f| f.fix.as_ref()).collect();
    fixes.sort_by_key(|f| std::cmp::Reverse(f.line));
    fixes
        .iter()
        .fold(text.to_owned(), |text, fix| fix.apply(&text))
}

fn count(findings: &[Finding], code: &str) -> usize {
    findings.iter().filter(|f| f.code == code).count()
}

fn errors(findings: &[Finding]) -> usize {
    findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count()
}

/// Each example's finding carries a fix; made, the finding is gone, and
/// no error comes that was not there. All of them made at once do the
/// same.
#[test]
fn each_fix_makes_its_finding_go_away() {
    for &(code, lang, text) in EXAMPLES {
        let before = omaspec_model::findings(text, lang);
        let fixes: Vec<&Finding> = before
            .iter()
            .filter(|f| f.code == code && f.fix.is_some())
            .collect();
        assert!(!fixes.is_empty(), "{code}: no fix in {before:#?}\n{text}");
        for finding in fixes {
            let fixed = finding.fix.as_ref().unwrap().apply(text);
            let after = omaspec_model::findings(&fixed, lang);
            assert!(
                count(&after, code) < count(&before, code),
                "{code}: still found after the fix\n{fixed}\n{after:#?}"
            );
            assert!(
                errors(&after) <= errors(&before),
                "{code}: an error came with the fix\n{fixed}\n{after:#?}"
            );
            assert!(
                after
                    .iter()
                    .all(|f| before.iter().any(|b| b.code == f.code)),
                "{code}: a new kind of finding came with the fix\n{fixed}\n{after:#?}"
            );
        }
        let (all, made) = fix_all(text, &before);
        assert!(made > 0, "{code}");
        assert_eq!(
            by_hand(text, &before),
            all,
            "{code}: made by hand otherwise"
        );
        let after = omaspec_model::findings(&all, lang);
        assert_eq!(
            count(&after, code),
            0,
            "{code}: left by fix_all\n{all}\n{after:#?}"
        );
    }
}

/// Every rule that says it carries a fix has an example above, and no
/// finding of a rule that says it never does carries one.
#[test]
fn the_rules_say_which_carry_a_fix() {
    for rule in RULES.iter().filter(|r| r.fixed()) {
        assert!(
            EXAMPLES.iter().any(|(code, _, _)| *code == rule.code),
            "{} has no example of its fix",
            rule.code
        );
    }
    for &(code, lang, text) in EXAMPLES {
        assert!(rule_of(code).is_some_and(|r| r.fixed()), "{code}");
        for finding in omaspec_model::findings(text, lang) {
            let rule =
                rule_of(finding.code).unwrap_or_else(|| panic!("{} is in no rule", finding.code));
            assert!(
                finding.fix.is_none() || rule.fixed(),
                "{} carries a fix, though its rule says none",
                finding.code
            );
        }
    }
    let (fixed, all) = share();
    assert!(fixed * 3 >= all, "{fixed} of {all}");
    // The guide names them, and how many they are.
    let guide = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/guide/reference.md"),
    )
    .unwrap();
    let said = format!("These carry their fix where it is sure, {fixed} of the {all} rules:");
    let at = guide
        .find(&said)
        .unwrap_or_else(|| panic!("the guide does not say: {said}"));
    let paragraph = guide[at..].split("\n\n").next().unwrap();
    for rule in RULES {
        assert_eq!(
            paragraph.contains(&format!("`{}`", rule.code)),
            rule.fixed(),
            "{} in the guide's list of rules with a fix",
            rule.code
        );
    }
}
