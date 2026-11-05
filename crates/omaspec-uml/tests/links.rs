//! Links: `[[url]]` in text, and on an element's declaration, where a
//! click on the whole element follows it. The engine writes every address
//! as it stands; the sanitizer decides which a picture keeps.

fn render(body: &str) -> String {
    let source = format!("@startuml\n{body}\n@enduml\n");
    omaspec_uml::render(&source).unwrap_or_else(|e| panic!("{body}: {e:?}"))
}

/// How many `<a>` lead to `url`, and whether one holds `what` first.
fn links(svg: &str, url: &str, what: &str) -> (usize, bool) {
    let open = format!(r#"<a href="{url}">"#);
    let count = svg.matches(&open).count();
    let holds = svg.match_indices(&open).any(|(at, _)| {
        let after = &svg[at + open.len()..];
        let after = match after.strip_prefix("<title>") {
            Some(titled) => &titled[titled.find("</title>").unwrap() + 8..],
            None => after,
        };
        after.starts_with(what)
    });
    (count, holds)
}

#[test]
fn a_class_and_its_words_are_one_link() {
    let svg = render(
        "class Foo <<entity>> [[https://example.org/foo{The foo}]] #pink {\n  +id: int\n}\n\
         [Comp] [[docs/comp.html]]\nFoo --> Comp",
    );
    // The box on one layer and the words on another.
    assert_eq!(links(&svg, "https://example.org/foo", "<rect"), (2, true));
    assert_eq!(links(&svg, "https://example.org/foo", "<text"), (2, true));
    assert!(svg.contains("<title>The foo</title>"));
    assert_eq!(links(&svg, "docs/comp.html", "<rect").0, 2);
}

#[test]
fn a_participant_s_heads_are_links() {
    let svg = render(
        "participant Alice [[https://example.org/alice]]\nactor Bob [[https://example.org/bob]]\n\
         Alice -> Bob : see [[https://example.org/docs the docs]]",
    );
    // Its head at the top and at the bottom.
    assert_eq!(links(&svg, "https://example.org/alice", "<rect"), (2, true));
    assert_eq!(
        links(&svg, "https://example.org/bob", "<ellipse"),
        (2, true)
    );
    assert_eq!(links(&svg, "https://example.org/docs", "<tspan"), (1, true));
}

#[test]
fn a_state_and_its_words_are_one_link() {
    let svg = render(
        "state S [[https://example.org/s{Tip}]] : does things\n\
         state \"Long name\" as L [[https://example.org/l]] {\n  [*] --> Inner\n}\n[*] --> S\nS --> L",
    );
    assert_eq!(links(&svg, "https://example.org/s", "<rect"), (2, true));
    assert_eq!(links(&svg, "https://example.org/s", "<text"), (2, true));
    assert_eq!(links(&svg, "https://example.org/l", "<rect"), (2, true));
    // What the link took out leaves the rest as it was.
    let plain = render(
        "state S : does things\nstate \"Long name\" as L {\n  [*] --> Inner\n}\n[*] --> S\nS --> L",
    );
    let unlinked = svg
        .replace(r#"<a href="https://example.org/s"><title>Tip</title>"#, "")
        .replace(r#"<a href="https://example.org/l">"#, "")
        .replace("</a>", "");
    assert_eq!(unlinked, plain);
}
