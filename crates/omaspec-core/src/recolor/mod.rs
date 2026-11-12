//! A color for one box, kept in the source as the language writes it: on
//! a declaration in PlantUML (`state Idle #f7768e`, `participant Bob
//! #f7768e`), in the box's own line of a tree or an activity
//! (`**[#f7768e] Tests`, `#f7768e:work;`), as a clause of a Gantt task, as
//! a `style` line in Mermaid (`style Idle fill:#f7768e`). The skins color
//! by kind, so every state of a machine is one color; this is for the one
//! box a writer points at.
//!
//! [`target`] is the box the caret is on, read from the source, in the
//! kinds whose boxes the source alone tells; in the graph diagrams the
//! pane's map names the box ([`Target::Named`]). [`edit`] is the change
//! that colors it. Where a language has no way to color one box,
//! [`refusal`] says so.

mod lines;
mod mermaid;
mod plantuml;

use std::ops::Range;

use crate::Kind;
use crate::lang::Language;

/// The change to the source: the byte range to replace and its new text.
pub type Edit = (Range<usize>, String);

/// A box that takes a color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A box lines name: a state, a class, a participant, a task.
    Named(String),
    /// A box written where it stands: a node of a mind map or a WBS, an
    /// action or a swimlane of an activity. `line` is the zero-based line
    /// it starts on, `label` what it shows.
    Line { line: usize, label: String },
}

impl Target {
    /// What to call it.
    pub fn label(&self) -> &str {
        match self {
            Target::Named(name) => name,
            Target::Line { label, .. } => label,
        }
    }
}

/// How a diagram writes the color of one box.
enum Flavor {
    State,
    Structure,
    Sequence,
    Activity,
    Timing,
    /// A PlantUML mind map or WBS.
    Tree,
    Gantt,
    /// Mermaid's `style Name fill:#…` line.
    Style {
        er: bool,
    },
    C4,
    /// No way to: why.
    No(String),
}

fn flavor(code: &str, language: Language) -> Flavor {
    let kind = crate::classify(code, language);
    match language {
        Language::Mermaid => match kind.name.as_ref() {
            "Flowchart" | "State diagram" | "Class diagram" | "Block diagram" => {
                Flavor::Style { er: false }
            }
            "ER diagram" => Flavor::Style { er: true },
            "C4 diagram" => Flavor::C4,
            "" => Flavor::No("nothing to color yet".into()),
            name => Flavor::No(format!(
                "Mermaid writes no color for one box of a {}",
                lower_first(name)
            )),
        },
        Language::PlantUml => match crate::kind::plantuml_block(code).as_str() {
            "mindmap" | "wbs" => Flavor::Tree,
            "gantt" => Flavor::Gantt,
            // C4-PlantUML colors one box by a tag (`AddElementTag`), which
            // the engine does not draw yet; a color written beside the
            // macro would be drawn over by it.
            "" | "uml" if omaspec_uml::is_c4(code) => Flavor::No(
                "C4-PlantUML colors one box by a tag, which Omaspec does not draw yet".into(),
            ),
            "" | "uml" => match kind.kind {
                Kind::State => Flavor::State,
                Kind::Structure => Flavor::Structure,
                Kind::Sequence => Flavor::Sequence,
                Kind::Activity => Flavor::Activity,
                Kind::Timing => Flavor::Timing,
                Kind::Other => Flavor::No("nothing to color yet".into()),
            },
            _ => Flavor::No(format!(
                "PlantUML writes no color for one box of a {}",
                lower_first(&kind.name)
            )),
        },
    }
}

/// `Sequence diagram` as `sequence diagram`; a name in capitals (`JSON`,
/// `WBS`, `ER diagram`) stays.
fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second)) if second.is_lowercase() => {
            format!("{}{second}{}", first.to_lowercase(), chars.as_str())
        }
        _ => name.to_owned(),
    }
}

/// Why no box of this diagram takes a color of its own, when its language
/// has no way to write one.
pub fn refusal(code: &str, language: Language) -> Option<String> {
    match flavor(code, language) {
        Flavor::No(why) => Some(why),
        _ => None,
    }
}

/// The box the caret is on, at the zero-based line `row` and the byte
/// `col` of it, as the source tells it. `None` where the caret is on no
/// box, and in the graph diagrams (state, class, flowchart), whose boxes
/// the picture's map names.
pub fn target(code: &str, language: Language, row: usize, col: usize) -> Option<Target> {
    let line = code.split('\n').nth(row)?;
    let col = col.min(line.len());
    match flavor(code, language) {
        Flavor::Tree => lines::tree_at(code, row),
        Flavor::Activity => lines::activity_at(code, row),
        Flavor::Gantt => lines::task_at(line, col),
        Flavor::Sequence => plantuml::participant_at(code, row, col),
        Flavor::Timing => plantuml::row_at(code, row, col),
        Flavor::C4 => mermaid::c4_at(code, row, col),
        Flavor::Style { er: true } => mermaid::entity_at(code, row, col),
        Flavor::State | Flavor::Structure | Flavor::Style { er: false } | Flavor::No(_) => None,
    }
}

/// The edit that colors `target` with `color` (`#rrggbb`), or takes its
/// color away with `None`. `Err` says why there is none.
pub fn edit(
    code: &str,
    language: Language,
    target: &Target,
    color: Option<&str>,
) -> Result<Edit, String> {
    match (flavor(code, language), target) {
        (Flavor::No(why), _) => Err(why),
        (Flavor::Tree, Target::Line { line, label }) => lines::tree(code, *line, label, color),
        (Flavor::Activity, Target::Line { line, label }) => {
            lines::activity(code, *line, label, color)
        }
        (Flavor::Gantt, Target::Named(name)) => lines::task(code, name, color),
        (Flavor::State, Target::Named(name)) => plantuml::state(code, name, color),
        (Flavor::Structure, Target::Named(name)) => plantuml::structure(code, name, color),
        (Flavor::Sequence, Target::Named(name)) => plantuml::participant(code, name, color),
        (Flavor::Timing, Target::Named(name)) => plantuml::row(code, name, color),
        (Flavor::Style { .. }, Target::Named(name)) => mermaid::style(code, name, color),
        (Flavor::C4, Target::Named(name)) => mermaid::c4(code, name, color),
        _ => Err("put the caret on the box to color".into()),
    }
}

/// The change to a line that was `old` and would be `new`, or why there
/// is none: `label` has that color already, or none to take away.
fn changed(
    label: &str,
    range: Range<usize>,
    old: &str,
    new: Option<String>,
    newline: &str,
) -> Result<Edit, String> {
    match new {
        Some(new) if new == old => Err(format!("{label} has that color already")),
        Some(new) => Ok((range, format!("{new}{newline}"))),
        None => Err(format!("{label} has no color of its own")),
    }
}

/// `line` without its line break, and that break.
fn split_newline(line: &str) -> (&str, &str) {
    let text = line.trim_end_matches(['\n', '\r']);
    (text, &line[text.len()..])
}

/// The spaces and tabs `line` starts with.
fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// The one of `names` the byte `col` of `line` is on or just after, as a
/// word of its own; the longest when several are.
fn covering<'a>(
    line: &str,
    col: usize,
    names: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut best: Option<&str> = None;
    for name in names {
        if name.is_empty() || best.is_some_and(|b| b.len() >= name.len()) {
            continue;
        }
        let on = line.match_indices(name).any(|(at, _)| {
            let end = at + name.len();
            at <= col
                && col <= end
                && !(name.starts_with(is_word) && line[..at].ends_with(is_word))
                && !(name.ends_with(is_word) && line[end..].starts_with(is_word))
        });
        if on {
            best = Some(name);
        }
    }
    best
}

/// What a picker shows of a label: its first line, cut short.
fn short(label: &str) -> String {
    let line = label.lines().next().unwrap_or("").trim();
    if line.chars().count() <= 40 {
        return line.to_owned();
    }
    let cut: String = line.chars().take(39).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Target {
        Target::Named(name.to_owned())
    }

    fn applied(code: &str, language: Language, name: &str, color: Option<&str>) -> String {
        put(code, language, &named(name), color)
    }

    fn put(code: &str, language: Language, target: &Target, color: Option<&str>) -> String {
        let (range, text) = edit(code, language, target, color).unwrap();
        let mut out = code.to_owned();
        out.replace_range(range, &text);
        out
    }

    #[test]
    fn a_state_s_declaration_takes_the_color_or_one_is_added() {
        let code = "@startuml\n[*] --> Idle\nIdle --> Busy : go\n@enduml\n";
        assert_eq!(
            applied(code, Language::PlantUml, "Busy", Some("#f7768e")),
            "@startuml\n[*] --> Idle\nstate Busy #f7768e\nIdle --> Busy : go\n@enduml\n"
        );
        let declared = "@startuml\nstate \"Long name\" as Busy ##red {\n  state In\n}\n[*] --> Busy\n@enduml\n";
        assert_eq!(
            applied(declared, Language::PlantUml, "Busy", Some("#aabbcc")),
            "@startuml\nstate \"Long name\" as Busy #aabbcc ##red {\n  state In\n}\n[*] --> Busy\n@enduml\n"
        );
        // Another color replaces it; none takes it away.
        let colored = "@startuml\n  state Busy #aabbcc <<hot>>\n[*] --> Busy\n@enduml\n";
        assert_eq!(
            applied(colored, Language::PlantUml, "busy", Some("#112233")),
            "@startuml\n  state Busy #112233 <<hot>>\n[*] --> Busy\n@enduml\n"
        );
        assert_eq!(
            applied(colored, Language::PlantUml, "Busy", None),
            "@startuml\n  state Busy <<hot>>\n[*] --> Busy\n@enduml\n"
        );
        assert!(edit(colored, Language::PlantUml, &named("Busy"), Some("#aabbcc")).is_err());
        assert!(edit(code, Language::PlantUml, &named("Idle"), None).is_err());
        assert!(edit(code, Language::PlantUml, &named("Nowhere"), Some("#000000")).is_err());
        // A short name inside another word is not named there.
        let nested = "@startuml\nstate Outer {\n  Connecting --> Done\n}\nOuter --> On\n@enduml\n";
        assert_eq!(
            applied(nested, Language::PlantUml, "On", Some("#f7768e")),
            nested.replace("Outer --> On", "state On #f7768e\nOuter --> On")
        );
    }

    #[test]
    fn a_class_takes_the_color_on_its_keyword_line() {
        let code =
            "@startuml\nclass Order {\n  id\n}\ninterface Payable\nOrder --> Payable\n@enduml\n";
        assert_eq!(
            applied(code, Language::PlantUml, "Order", Some("#ffcc00")),
            "@startuml\nclass Order #ffcc00 {\n  id\n}\ninterface Payable\nOrder --> Payable\n@enduml\n"
        );
        assert_eq!(
            applied(code, Language::PlantUml, "Payable", Some("#ffcc00")),
            "@startuml\nclass Order {\n  id\n}\ninterface Payable #ffcc00\nOrder --> Payable\n@enduml\n"
        );
        // A class that is only named gets a declaration.
        let bare = "@startuml\nclass Order\nOrder --> Line\n@enduml\n";
        assert_eq!(
            applied(bare, Language::PlantUml, "Line", Some("#ffcc00")),
            "@startuml\nclass Order\nclass Line #ffcc00\nOrder --> Line\n@enduml\n"
        );
    }

    /// The box at the first `|` of `marked`, colored, in the source without
    /// that mark.
    fn at_caret(marked: &str, language: Language, color: Option<&str>) -> String {
        let (code, target) = aimed(marked, language);
        put(&code, language, &target.expect("a box at the caret"), color)
    }

    /// `marked` without its `^`, and the box the caret is on where it was.
    fn aimed(marked: &str, language: Language) -> (String, Option<Target>) {
        let at = marked.find('^').expect("a caret");
        let code = marked.replacen('^', "", 1);
        let row = code[..at].matches('\n').count();
        let col = at - code[..at].rfind('\n').map_or(0, |i| i + 1);
        let target = target(&code, language, row, col);
        (code, target)
    }

    #[test]
    fn a_structure_diagram_colors_what_it_draws() {
        let p = Language::PlantUml;
        // A component only lines name gets a line of its own, in its brackets.
        let code = "@startuml\n[Order Service] --> [Billing]\n@enduml\n";
        assert_eq!(
            applied(code, p, "Order Service", Some("#ffcc00")),
            "@startuml\n[Order Service] #ffcc00\n[Order Service] --> [Billing]\n@enduml\n"
        );
        let code = "@startuml\n[Order Service] as OS <<core>>\nnode Server {\n  database Db\n}\nOS --> Db\n@enduml\n";
        assert_eq!(
            applied(code, p, "OS", Some("#ffcc00")),
            code.replace("<<core>>", "<<core>> #ffcc00")
        );
        assert_eq!(
            applied(code, p, "Server", Some("#ffcc00")),
            code.replace("node Server {", "node Server #ffcc00 {")
        );
        assert_eq!(
            applied(code, p, "Db", Some("#ffcc00")),
            code.replace("database Db", "database Db #ffcc00")
        );
        // A use case and its actor.
        let code = "@startuml\nactor User\nUser --> (Log in)\n@enduml\n";
        assert_eq!(
            applied(code, p, "User", Some("#ffcc00")),
            code.replace("actor User", "actor User #ffcc00")
        );
        assert_eq!(
            applied(code, p, "Log in", Some("#ffcc00")),
            code.replace("User -->", "(Log in) #ffcc00\nUser -->")
        );
        // A class keeps what it extends after the color.
        let code = "@startuml\nclass Cat<T> <<pet>> extends Animal {\n}\n@enduml\n";
        assert_eq!(
            applied(code, p, "Cat", Some("#ffcc00")),
            code.replace("<<pet>>", "<<pet>> #ffcc00")
        );
    }

    #[test]
    fn a_participant_takes_the_color_and_keeps_its_column() {
        let p = Language::PlantUml;
        let code =
            "@startuml\nactor \"Web user\" as U\nU -> Shop : order\nShop -> Bank : pay\n@enduml\n";
        assert_eq!(
            at_caret(&code.replace("U ->", "^U ->"), p, Some("#f7768e")),
            code.replace("as U", "as U #f7768e")
        );
        // On the line that declares it, wherever the caret is.
        assert_eq!(
            at_caret(&code.replace("actor", "ac^tor"), p, Some("#f7768e")),
            code.replace("as U", "as U #f7768e")
        );
        // One that messages only name is declared before the first of
        // them, after the one the same line names before it.
        assert_eq!(
            at_caret(&code.replace("Bank", "Ba^nk"), p, Some("#f7768e")),
            code.replace("Shop -> Bank", "participant Bank #f7768e\nShop -> Bank")
        );
        let bare = "@startuml\nAlice -> Bob : hi\n@enduml\n";
        let colored = at_caret(&bare.replace("Bob", "Bob^"), p, Some("#f7768e"));
        assert_eq!(
            colored,
            "@startuml\nparticipant Alice\nparticipant Bob #f7768e\nAlice -> Bob : hi\n@enduml\n"
        );
        assert_eq!(
            applied(&colored, p, "Bob", None),
            colored.replace(" #f7768e", "")
        );
        // The words of a message are no box.
        assert_eq!(aimed(&bare.replace("hi", "h^i"), p).1, None);
    }

    #[test]
    fn a_node_of_a_tree_takes_the_color_after_its_markers() {
        let p = Language::PlantUml;
        let wbs =
            "@startwbs\n* Launch\n** Build\n***< Tests\n**(B) Budget\n**:Two\nlines;\n@endwbs\n";
        let colored = at_caret(&wbs.replace("Build", "Bu^ild"), p, Some("#f7768e"));
        assert_eq!(colored, wbs.replace("** Build", "**[#f7768e] Build"));
        assert_eq!(
            at_caret(&colored.replace("Build", "Build^"), p, Some("#00ff00")),
            wbs.replace("** Build", "**[#00ff00] Build")
        );
        assert_eq!(at_caret(&colored.replace("Build", "^Build"), p, None), wbs);
        assert_eq!(
            at_caret(&wbs.replace("Tests", "T^ests"), p, Some("#f7768e")),
            wbs.replace("***< Tests", "***[#f7768e]< Tests")
        );
        assert_eq!(
            at_caret(&wbs.replace("Budget", "^Budget"), p, Some("#f7768e")),
            wbs.replace("**(B)", "**[#f7768e](B)")
        );
        // Any line of a label over several.
        let (code, target) = aimed(&wbs.replace("lines;", "li^nes;"), p);
        assert_eq!(target.as_ref().map(Target::label), Some("Two"));
        assert_eq!(
            put(&code, p, &target.unwrap(), Some("#f7768e")),
            wbs.replace("**:Two", "**[#f7768e]:Two")
        );
        let map = "@startmindmap\n* Root\n-- Left\n**_ Plain\n@endmindmap\n";
        assert_eq!(
            at_caret(&map.replace("Left", "^Left"), p, Some("#f7768e")),
            map.replace("-- Left", "--[#f7768e] Left")
        );
        assert_eq!(
            at_caret(&map.replace("Plain", "^Plain"), p, Some("#f7768e")),
            map.replace("**_ Plain", "**[#f7768e]_ Plain")
        );
        assert_eq!(
            aimed(&map.replace("@endmindmap", "@end^mindmap"), p).1,
            None
        );
    }

    #[test]
    fn an_action_and_a_swimlane_take_the_color_before_them() {
        let p = Language::PlantUml;
        let code = "@startuml\n|Kitchen|\nstart\n:Knead\nthe dough;\n|Oven|\n  :Bake;\n|Kitchen|\n:Serve;\nstop\n@enduml\n";
        let colored = at_caret(&code.replace("the dough", "the ^dough"), p, Some("#f7768e"));
        assert_eq!(colored, code.replace(":Knead", "#f7768e:Knead"));
        assert_eq!(
            at_caret(&colored.replace("Knead", "Kn^ead"), p, Some("#00ff00")),
            code.replace(":Knead", "#00ff00:Knead")
        );
        assert_eq!(at_caret(&colored.replace("Knead", "Kn^ead"), p, None), code);
        assert_eq!(
            at_caret(&code.replace("Bake", "^Bake"), p, Some("#f7768e")),
            code.replace("  :Bake;", "  #f7768e:Bake;")
        );
        // A lane takes it where it is named first.
        let lane = at_caret(&code.replace("|\n:Serve", "^|\n:Serve"), p, Some("#f7768e"));
        assert_eq!(lane, code.replacen("|Kitchen|", "|#f7768e|Kitchen|", 1));
        assert_eq!(
            at_caret(
                &lane.replace("|Kitchen|\n:Serve", "|Kit^chen|\n:Serve"),
                p,
                None
            ),
            code
        );
        assert_eq!(aimed(&code.replace("stop", "st^op"), p).1, None);
    }

    #[test]
    fn a_gantt_task_takes_the_color_as_a_clause() {
        let p = Language::PlantUml;
        let code = "@startgantt\n[Design] lasts 5 days\n[Build] lasts 3 days\n[Build] starts at [Design]'s end\n@endgantt\n";
        let colored = at_caret(
            &code.replace("[Build] lasts", "[Bu^ild] lasts"),
            p,
            Some("#f7768e"),
        );
        assert_eq!(
            colored,
            code.replace("end\n", "end\n[Build] is colored in #f7768e\n")
        );
        // The task in the brackets the caret is in.
        assert_eq!(
            aimed(&code.replace("[Design]'s", "[Des^ign]'s"), p).1,
            Some(named("Design"))
        );
        assert_eq!(
            applied(&colored, p, "Build", Some("#00ff00")),
            colored.replace("#f7768e", "#00ff00")
        );
        assert_eq!(applied(&colored, p, "Build", None), code);
        let chained = "@startgantt\n[Design] lasts 5 days and is colored in Fuchsia/FireBrick\n[Test] is colored in Red and lasts 2 days\n@endgantt\n";
        assert_eq!(
            applied(chained, p, "Design", Some("#00ff00")),
            chained.replace("Fuchsia/FireBrick", "#00ff00")
        );
        assert_eq!(
            applied(chained, p, "Design", None),
            chained.replace(" and is colored in Fuchsia/FireBrick", "")
        );
        assert_eq!(
            applied(chained, p, "Test", None),
            chained.replace(" is colored in Red and", "")
        );
    }

    #[test]
    fn a_timing_participant_takes_the_color_on_its_line() {
        let p = Language::PlantUml;
        let code = "@startuml\nconcise \"Web\" as W\nclock clk with period 1\n@0\nW is Idle\n@5\nW is Busy\n@enduml\n";
        let colored = at_caret(&code.replace("W is Busy", "W^ is Busy"), p, Some("#f7768e"));
        assert_eq!(colored, code.replace("as W", "as W #f7768e"));
        // PlantUML reads no color on a clock.
        let (clock, target) = aimed(&code.replace("clock", "clo^ck"), p);
        assert_eq!(target, Some(named("clk")));
        assert!(edit(&clock, p, &named("clk"), Some("#f7768e")).is_err());
        assert_eq!(applied(&colored, p, "W", None), code);
    }

    #[test]
    fn a_diagram_without_a_way_says_so() {
        let scenario = "sequenceDiagram\n  Alice->>Bob: hi\n";
        assert_eq!(
            refusal(scenario, Language::Mermaid).as_deref(),
            Some("Mermaid writes no color for one box of a sequence diagram")
        );
        assert!(edit(scenario, Language::Mermaid, &named("Bob"), Some("#fff")).is_err());
        assert_eq!(
            refusal("@startjson\n{\"a\": 1}\n@endjson\n", Language::PlantUml).as_deref(),
            Some("PlantUML writes no color for one box of a JSON")
        );
        assert_eq!(
            refusal(
                "@startuml\n!include <C4/C4_Container>\nContainer(api, \"API\")\n@enduml\n",
                Language::PlantUml
            )
            .as_deref(),
            Some("C4-PlantUML colors one box by a tag, which Omaspec does not draw yet")
        );
        assert_eq!(
            refusal("@startwbs\n* A\n@endwbs\n", Language::PlantUml),
            None
        );
    }

    #[test]
    fn mermaid_colors_a_flowchart_an_entity_and_a_c4_element() {
        let m = Language::Mermaid;
        // A flowchart with a decision is one too.
        let flow = "flowchart TD\n  A[Start] --> B{Ok?}\n";
        assert_eq!(
            applied(flow, m, "B", Some("#f7768e")),
            "flowchart TD\n  A[Start] --> B{Ok?}\n  style B fill:#f7768e\n"
        );
        let er = "erDiagram\n  ORDER ||--o{ LINE : has\n  ORDER {\n    int id\n  }\n";
        assert_eq!(
            aimed(&er.replace("LINE", "LI^NE"), m).1,
            Some(named("LINE"))
        );
        assert_eq!(aimed(&er.replace("has", "h^as"), m).1, Some(named("ORDER")));
        assert_eq!(
            at_caret(&er.replace("int id", "int ^id"), m, Some("#f7768e")),
            format!("{er}  style ORDER fill:#f7768e\n")
        );
        let c4 = "C4Context\n  Person(u, \"User\")\n  System(s, \"Shop\")\n  Rel(u, s, \"uses\")\n";
        let colored = at_caret(&c4.replace("System", "Sys^tem"), m, Some("#f7768e"));
        assert_eq!(
            colored,
            format!("{c4}  UpdateElementStyle(s, $bgColor=\"#f7768e\")\n")
        );
        assert_eq!(
            aimed(&c4.replace("Rel(u,", "Rel(u^,"), m).1,
            Some(named("u"))
        );
        assert_eq!(
            applied(&colored, m, "s", Some("#00ff00")),
            colored.replace("#f7768e", "#00ff00")
        );
        assert_eq!(applied(&colored, m, "s", None), c4);
        let more = "C4Context\n  Person(u, \"User\")\n  UpdateElementStyle(u, #112233, $fontColor=\"#fff\")\n";
        assert_eq!(applied(more, m, "u", None), more.replace("#112233, ", ""));
    }

    #[test]
    fn mermaid_keeps_the_color_in_a_style_line() {
        let code = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy\n";
        let colored = applied(code, Language::Mermaid, "Busy", Some("#f7768e"));
        assert_eq!(
            colored,
            "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy\n  style Busy fill:#f7768e\n"
        );
        // Another color replaces the fill and keeps the rest of the style.
        let styled = "stateDiagram-v2\n  Idle --> Busy\n  style Busy fill:#f7768e,stroke:#333\n";
        assert_eq!(
            applied(styled, Language::Mermaid, "Busy", Some("#00ff00")),
            "stateDiagram-v2\n  Idle --> Busy\n  style Busy fill:#00ff00,stroke:#333\n"
        );
        assert_eq!(
            applied(styled, Language::Mermaid, "Busy", None),
            "stateDiagram-v2\n  Idle --> Busy\n  style Busy stroke:#333\n"
        );
        // A line that held the fill alone goes with it.
        assert_eq!(applied(&colored, Language::Mermaid, "Busy", None), code);
        // A class diagram the same way.
        let classes = "classDiagram\n  class Order\n  Order --> Line\n";
        assert_eq!(
            applied(classes, Language::Mermaid, "Order", Some("#ffcc00")),
            "classDiagram\n  class Order\n  Order --> Line\n  style Order fill:#ffcc00\n"
        );
        assert!(edit(classes, Language::Mermaid, &named("Order"), None).is_err());
    }
}
