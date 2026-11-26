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
    named(
        "kanban\n  todo[Todo]\n    t1[Write the docs]\n  done[Done]\n    t2[Ship]\n",
        Language::Mermaid,
        &[
            "column-Todo",
            "card-Write_20_the_20_docs",
            "column-Done",
            "card-Ship",
        ],
    );
}

#[test]
fn a_journey_names_its_sections_and_tasks() {
    named(
        "journey\n  title My day\n  section Morning\n    Make tea: 5: Me\n    Read: 3: Me\n",
        Language::Mermaid,
        &["section-Morning", "task-Make_20_tea", "task-Read", "title"],
    );
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
}
