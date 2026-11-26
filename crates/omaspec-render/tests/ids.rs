//! Every kind Omaspec draws names its elements from the source, as ids and
//! classes (docs/adr/0037); a kind drawn as a whole says so at its root.

use omaspec_core::Language;
use omaspec_render::{Config, Engine, Renderer};

fn drawn(source: &str, language: Language) -> String {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let out = renderer.render(source, language).unwrap();
    let native = match language {
        Language::Mermaid => Engine::NativeMermaid,
        Language::PlantUml => Engine::NativePlantUml,
    };
    assert_eq!(out.engine, native, "{source}");
    out.svg
}

/// What the group with the id `id` draws first in `svg`.
fn first_drawn<'a>(svg: &'a str, id: &str) -> &'a str {
    let own = format!(r#" id="{id}">"#);
    let at = svg.find(&own).unwrap_or_else(|| panic!("{id}: {svg}"));
    &svg[at + own.len()..]
}

/// `source` drawn has an element with each of `ids`, once, and its root
/// says it is drawn as a whole.
fn named(source: &str, language: Language, ids: &[&str]) {
    let svg = drawn(source, language);
    assert!(svg.contains(r#"data-omaspec="named""#), "{source}: {svg}");
    for id in ids {
        assert_eq!(
            svg.matches(&format!(r#" id="{id}""#)).count(),
            1,
            "{id} in {source}: {svg}"
        );
        assert!(svg.contains(&format!(r#"<g class="{id}""#)), "{id}");
    }
}

#[test]
fn an_er_diagram_names_its_entities_and_relations() {
    named(
        "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  ORDER ||--|{ LINE : holds\n",
        Language::Mermaid,
        &[
            "entity-CUSTOMER",
            "entity-ORDER",
            "link-CUSTOMER-ORDER-places",
        ],
    );
}

#[test]
fn a_c4_diagram_names_its_elements_and_relations() {
    named(
        "C4Context\n  title Bank\n  Person(customer, \"Customer\")\n  System_Boundary(b1, \"Bank\") {\n    System(bank, \"Banking\")\n  }\n  Rel(customer, bank, \"Uses\")\n",
        Language::Mermaid,
        &[
            "person-customer",
            "system-bank",
            "boundary-b1",
            "link-customer-bank-Uses",
            "title",
        ],
    );
}

#[test]
fn an_architecture_names_its_groups_services_and_edges() {
    named(
        "architecture-beta\n  group api(cloud)[API]\n  service db(database)[Database] in api\n  service server(server)[Server] in api\n  junction hub\n  db:L -- R:server\n  server:B --> T:hub\n",
        Language::Mermaid,
        &[
            "group-api",
            "service-db",
            "junction-hub",
            "link-db-server",
            "link-server-hub",
        ],
    );
}

#[test]
fn a_block_diagram_names_its_blocks_and_edges() {
    named(
        "block-beta\n  columns 2\n  a[\"Api\"] b[(\"Db\")]\n  a --> b\n",
        Language::Mermaid,
        &["block-a", "block-b", "link-a-b"],
    );
}

#[test]
fn a_kanban_names_its_columns_and_cards() {
    // By what the source calls them, by their words where it gives none.
    named(
        "kanban\n  todo[Todo]\n    t1[Write the docs]\n  [Done]\n    t2[Ship<br>today]\n",
        Language::Mermaid,
        &["column-todo", "card-t1", "column-Done", "card-t2"],
    );
}

#[test]
fn a_journey_names_its_sections_and_tasks() {
    named(
        "journey\n  title My day\n  section Morning\n    Make tea: 5: Me\n    Read: 3: Me\n",
        Language::Mermaid,
        &["section-Morning", "task-Make_20_tea", "task-Read", "title"],
    );
    // Its id on its card, not on the line drawn under it.
    let svg = drawn(
        "journey\n  section Morning\n    Make tea: 5: Me\n",
        Language::Mermaid,
    );
    assert!(!first_drawn(&svg, "task-Make_20_tea").starts_with("<line"));
}

#[test]
fn a_timeline_names_its_sections_periods_and_events() {
    named(
        "timeline\n  title Launch\n  section Early\n    2020 : Idea : Team\n    2021 : Launch\n",
        Language::Mermaid,
        &[
            "section-Early",
            "period-2020",
            "period-2021",
            "event-Idea",
            "event-Team",
            "event-Launch",
            "title",
        ],
    );
    // A period's id on its card, not on the stem drawn under it, across
    // and down.
    for source in [
        "timeline\n  2020 : Idea : Team\n",
        "timeline TD\n  2020 : Idea : Team\n",
    ] {
        let svg = drawn(source, Language::Mermaid);
        assert!(
            !first_drawn(&svg, "period-2020").starts_with("<line"),
            "{source}"
        );
    }
}

#[test]
fn a_flow_names_its_ports_blocks_and_wires() {
    named(
        "@startflow\nin sensor\nout valve\nblock Filter \"Filter\" {\n  in sensor\n  out room\n}\nsensor -> Filter.sensor\nFilter.room -> valve\n@endflow\n",
        Language::PlantUml,
        &[
            "input-sensor",
            "output-valve",
            "block-Filter",
            "link-sensor-Filter.sensor",
            "link-Filter.room-valve",
        ],
    );
}

#[test]
fn a_network_names_its_networks_servers_and_lines() {
    named(
        "@startnwdiag\nnwdiag {\n  network home {\n    router;\n    laptop;\n  }\n}\n@endnwdiag\n",
        Language::PlantUml,
        &[
            "network-home",
            "server-router",
            "server-laptop",
            "link-home-router",
            "link-home-laptop",
        ],
    );
}

#[test]
fn a_story_map_names_its_activities_steps_releases_and_stories() {
    named(
        "@startstorymap\nrelease Pilot : first\nactivity Enrol\n  step Sign up\n    * By email : Pilot\n@endstorymap\n",
        Language::PlantUml,
        &[
            "activity-Enrol",
            "step-Sign_20_up",
            "release-Pilot",
            "story-By_20_email",
        ],
    );
}

#[test]
fn a_timing_diagram_names_its_rows_notes_and_messages() {
    named(
        "@startuml\nconcise \"Program\" as P\nbinary \"Lock\" as L\n@0\nP is Idle\nL is low\n@5\nP is Wash\nP -> L : lock\nnote top of P : starts\n@enduml\n",
        Language::PlantUml,
        &[
            "concise-P",
            "binary-L",
            "link-P-L-lock",
            "note-concise-P-top-1",
        ],
    );
}

#[test]
fn a_gantt_chart_names_its_sections_tasks_and_links() {
    named(
        "@startgantt\n-- Plan --\n[Design] as [D] lasts 3 days\n[Build] as [B] lasts 4 days\n[D] -> [B]\n[Review] happens at [B]'s end\n@endgantt\n",
        Language::PlantUml,
        &[
            "section-Plan",
            "task-D",
            "task-B",
            "milestone-Review",
            "link-D-B",
        ],
    );
}

#[test]
fn a_capability_map_names_its_capabilities() {
    named(
        "@startcapabilities\nlevels Maturity : none, basic\n* Readers\n** Lending : basic\n@endcapabilities\n",
        Language::PlantUml,
        &["capability-Readers", "capability-Lending"],
    );
}

#[test]
fn a_flow_names_what_a_block_holds_by_the_block() {
    named(
        "@startflow\nin a\nblock Outer {\n  in a\n  block Inner {\n    in x\n  }\n  a -> Inner.x\n}\na -> Outer.a\n@endflow\n",
        Language::PlantUml,
        &[
            "block-Outer",
            "block-Inner",
            "link-a-Outer.a",
            "link-Outer.a-Inner.x",
        ],
    );
}

#[test]
fn a_timing_diagram_names_a_message_by_its_rows_keys() {
    // A row written by its label is named by its key; a note below is
    // counted apart from one above.
    named(
        "@startuml\nconcise \"Program\" as P\nbinary \"Lock\" as L\n@0\nP is Idle\nL is low\n@5\nProgram -> L : lock\nnote top of P : starts\nnote bottom of P : ends\n@enduml\n",
        Language::PlantUml,
        &[
            "link-P-L-lock",
            "note-concise-P-top-1",
            "note-concise-P-bottom-1",
        ],
    );
}

#[test]
fn a_mermaid_gantt_names_its_tasks_by_their_ids_or_their_words() {
    // A task without an id is named by its words, not by a count that
    // moves with the tasks before it.
    named(
        "gantt\n  dateFormat YYYY-MM-DD\n  section Plan\n  Build :b, 2026-01-05, 5d\n  Sketch : 2026-01-01, 4d\n  Freeze :vert, f, after b, 0d\n  Launch :milestone, l, after b, 0d\n",
        Language::Mermaid,
        &[
            "section-Plan",
            "task-b",
            "task-Sketch",
            "vert-f",
            "milestone-l",
        ],
    );
}

#[test]
fn the_charts_name_what_they_show() {
    for (source, ids) in [
        (
            "pie title Day\n  \"Sleep\" : 8\n  \"Work\" : 8\n",
            &["slice-Sleep", "slice-Work", "title"][..],
        ),
        (
            "sankey-beta\nSolar,Grid,60\nGrid,Homes,40\n",
            &[
                "node-Solar",
                "node-Grid",
                "link-Solar-Grid",
                "link-Grid-Homes",
            ][..],
        ),
        (
            "quadrantChart\n  quadrant-1 Do\n  Dark mode: [0.3, 0.6]\n",
            &["quadrant-1", "quadrant-4", "point-Dark_20_mode"][..],
        ),
        (
            "radar-beta\n  axis A, B, m[\"Math\"]\n  curve ada[\"Ada\"]{1, 2, 3}\n",
            &["axis-A", "axis-m", "curve-ada"][..],
        ),
        (
            "treemap-beta\n\"Team\"\n  \"Apps\": 3\n  \"Data\": 2\n",
            &["node-Team", "node-Apps", "node-Data"][..],
        ),
        (
            "packet-beta\n0-15: \"Source port\"\n16-31: \"Destination port\"\n",
            &["field-Source_20_port", "field-Destination_20_port"][..],
        ),
        (
            "xychart-beta\n  x-axis [a, b]\n  bar [1, 2]\n  line [2, 1]\n",
            &["bar", "line"][..],
        ),
    ] {
        named(source, Language::Mermaid, ids);
    }
    named(
        "@startchart\nh-axis [Q1, Q2]\nbar \"Bread\" [3, 4]\nline \"Margin\" [1, 2]\n@endchart\n",
        Language::PlantUml,
        &["bar-Bread", "line-Margin"],
    );
}

#[test]
fn a_grammar_names_its_rules_and_a_marble_diagram_its_lanes() {
    named(
        "@startebnf\nvalue = object | number;\nnumber = digit, { digit };\n@endebnf\n",
        Language::PlantUml,
        &["rule-value", "rule-number"],
    );
    named(
        "@startregex\n/a+b/\n@endregex\n",
        Language::PlantUml,
        &["rule"],
    );
    named(
        "marble-beta\n  keys    : -a-b--|\n  results : ---r--|\n",
        Language::Mermaid,
        &["lane-keys", "lane-results"],
    );
}

#[test]
fn a_name_holds_when_what_the_source_calls_it_is_not_its_words() {
    // A timing message's end is a row's key before it is another's label.
    named(
        "@startuml\nconcise \"B\" as A\nconcise \"X\" as B\n@0\nA is on\nB is off\n@5\nB -> A : go\n@enduml\n",
        Language::PlantUml,
        &["link-B-A-go"],
    );
    // Two blocks of one name hold wires of their own names, by the way down.
    named(
        "@startflow\nblock A {\n  block Inner {\n    in x\n    out y\n    x -> y\n  }\n}\nblock B {\n  block Inner {\n    in x\n    out y\n    x -> y\n  }\n}\n@endflow\n",
        Language::PlantUml,
        &["link-A.Inner.x-A.Inner.y", "link-B.Inner.x-B.Inner.y"],
    );
    // A chart's line of the legend is its series'.
    let svg = drawn(
        "@startchart\nlegend right\nh-axis [Q1, Q2]\nbar \"Bread\" [3, 4]\n@endchart\n",
        Language::PlantUml,
    );
    assert!(svg.matches(r#"<g class="bar-Bread""#).count() >= 2, "{svg}");
}
