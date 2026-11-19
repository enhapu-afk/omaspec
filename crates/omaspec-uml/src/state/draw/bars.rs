//! Fork and join bars as drawn: as long as what they split and join
//! (`graph::span`, which `graph::open` lays out for every container that
//! has bars).

use crate::state::{draw_mapped, parse};

/// The SVG `source` draws.
fn drawn(source: &str) -> String {
    let (_, lines) = crate::source::body(source).unwrap();
    draw_mapped(&parse(&lines).unwrap()).unwrap().0
}

/// The number `name` holds in `tag`.
fn number(tag: &str, name: &str) -> f32 {
    format!(" {tag}")
        .split(&format!(" {name}=\""))
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

/// The bars of `svg`, each as `(x, y, width)`.
fn bars(svg: &str) -> Vec<(f32, f32, f32)> {
    svg.split("<rect ")
        .skip(1)
        .filter(|rect| rect.contains(" height=\"8\""))
        .map(|rect| (number(rect, "x"), number(rect, "y"), number(rect, "width")))
        .collect()
}

/// The first point of each line of `svg`.
fn starts(svg: &str) -> Vec<(f32, f32)> {
    let point = |text: &str| -> Option<(f32, f32)> {
        let (x, y) = text.split_once(',')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    };
    let polylines = svg
        .split("<polyline points=\"")
        .skip(1)
        .filter_map(|rest| point(rest.split(' ').next()?));
    let paths = svg
        .split("<path d=\"M")
        .skip(1)
        .filter_map(|rest| point(rest.split(' ').next()?));
    polylines.chain(paths).collect()
}

#[test]
fn a_bar_spans_the_states_it_splits_and_joins() {
    let source = "@startuml\nstate f <<fork>>\nstate j <<join>>\n[*] --> f\nf --> Saving\nf --> Uploading\nSaving --> j\nUploading --> j\nj --> [*]\n@enduml\n";
    let drawn = bars(&drawn(source));
    assert_eq!(drawn.len(), 2, "{drawn:?}");
    for (_, _, width) in &drawn {
        assert!(*width > super::BAR_LONG, "{drawn:?}");
    }
    // A bar with one state keeps its length.
    let one = "@startuml\nstate f <<fork>>\n[*] --> f\nf --> A\n@enduml\n";
    assert!(
        bars(&self::drawn(one))
            .iter()
            .all(|(_, _, width)| (*width - super::BAR_LONG).abs() < 0.01)
    );
}

#[test]
fn each_line_meets_a_join_at_a_place_of_its_own() {
    let source = "@startuml\nstate j <<join>>\n[*] --> Saving\n[*] --> Uploading\nSaving --> j\nUploading --> j\nj --> [*]\n@enduml\n";
    let svg = drawn(source);
    // The tips of the two heads into the join stand apart.
    let tips: Vec<&str> = svg
        .split("<polygon points=\"")
        .skip(1)
        .filter_map(|rest| rest.split(' ').nth(1))
        .collect();
    let mut distinct = tips.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), tips.len(), "{tips:?}");
}

#[test]
fn a_line_sideways_from_a_bar_starts_on_its_end_and_the_bar_stays_short() {
    for source in [
        "@startuml\nstate f <<fork>>\n[*] --> f\nf -right-> Gamma\n@enduml\n",
        "@startuml\nstate f <<fork>>\n[*] --> f\nf --> Alpha\nf --> Beta\nf -right-> Gamma\n@enduml\n",
    ] {
        let svg = drawn(source);
        let (x, y, width) = bars(&svg)[0];
        // Some line starts on the bar's right end.
        let end = starts(&svg)
            .into_iter()
            .any(|(sx, sy)| (sx - (x + width)).abs() < 1.0 && sy > y && sy < y + 8.0);
        assert!(end, "{svg}");
        assert!(width < 3.0 * super::BAR_LONG, "{width}");
    }
}

#[test]
fn notes_stand_by_the_bar_as_drawn() {
    let source = "@startuml\nstate \"A wide state name here\" as A\nstate \"Another wide state name\" as B\nstate f <<fork>>\n[*] --> f\nf --> A\nf --> B\nf --> C\nnote right of f : split\nnote left of f : here\n@enduml\n";
    let svg = drawn(source);
    let (x, _, width) = bars(&svg)[0];
    // Each note's outline, a path that closes, from its least to its most x.
    let notes: Vec<(f32, f32)> = svg
        .split("<path d=\"M")
        .skip(1)
        .filter_map(|rest| rest.split('"').next()?.strip_suffix(" Z"))
        .map(|outline| {
            let xs: Vec<f32> = outline
                .split([' ', 'L'])
                .filter_map(|pair| pair.split(',').next()?.parse().ok())
                .collect();
            let low = xs.iter().copied().fold(f32::MAX, f32::min);
            let high = xs.iter().copied().fold(f32::MIN, f32::max);
            (low, high)
        })
        .collect();
    assert_eq!(notes.len(), 2, "{svg}");
    for (low, high) in notes {
        let gap = if low >= x + width {
            low - (x + width)
        } else {
            x - high
        };
        assert!((0.0..30.0).contains(&gap), "{gap}: {svg}");
    }
}

#[test]
fn a_bar_in_a_state_grows_only_as_far_as_its_frame() {
    let source = "@startuml\nstate \"Wide state number one\" as W1\nstate Outer {\n  state f <<fork>>\n  [*] --> f\n  f --> W1\n  f --> W2\n}\n[*] --> Outer\nOuter --> Done\n@enduml\n";
    let (_, _, width) = bars(&drawn(source))[0];
    assert!((width - super::BAR_LONG).abs() < 0.01, "{width}");
}
