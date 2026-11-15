//! An emoji written with several characters is drawn whole, in the emoji
//! face's colors: a family joined by zero width joiners, a skin tone, a
//! flag, the mark that asks for the picture (`🛋️`), a keycap. usvg gave
//! up on the whole line where the emoji face drew fewer glyphs than the
//! text face, and every emoji of it was an empty box; a run of other
//! words around the emoji (`**bold**`) made it remove past its glyphs and
//! end the program. Both are mended in `vendor/usvg`.

use omaspec_render::export::{parse, rasterize};

/// The pixels of `text` drawn in black that are not grey: the emoji's.
/// (Noto draws a family in grey, so the emoji here have colors.)
fn colored(text: &str) -> usize {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="420" height="40"><text x="4" y="28" font-family="sans-serif" font-size="20" fill="black">{text}</text></svg>"#
    );
    let pixmap = rasterize(&parse(&svg).expect("the picture"), 1.0).expect("its pixels");
    pixmap
        .pixels()
        .iter()
        .filter(|p| {
            let (r, g, b) = (p.red(), p.green(), p.blue());
            r.max(g).max(b) - r.min(g).min(b) > 80
        })
        .count()
}

/// Whether an emoji face that draws in color is installed. Without one an
/// emoji is a box, or drawn in one color by a face of signs (GNU FreeFont
/// has a cat), and its colors tell nothing.
fn emoji_face() -> bool {
    omaspec_render::export::color_emoji()
}

#[test]
fn an_emoji_of_several_characters_is_drawn_whole() {
    if !emoji_face() {
        return;
    }
    let one = colored("a 🐈 b");
    assert!(one > 40, "{one}");
    for text in [
        "Astronaut 🧑‍🚀 here",
        "Couple 👩‍❤️‍👨 there",
        "Coder 🧑‍💻 x",
        "Flag 🏳️‍🌈 y",
        "Skin 👍🏽 k",
        "Flags 🇩🇪 and 🏴󠁧󠁢󠁳󠁣󠁴󠁿",
        "Keycap 1️⃣ fire ❤️‍🔥",
    ] {
        // An empty box has no color at all.
        assert!(colored(text) > 40, "{text}");
    }
    // Each emoji of a line, not the first alone: the sofa asked for as a
    // picture took the cat beside it into an empty box too.
    let both = colored("Sofa 🛋️ and 🐈 cat");
    assert!(both > colored("Sofa 🛋️ and cat") + one / 2, "{both}");
}

#[test]
fn an_emoji_among_runs_of_other_words_is_drawn_and_ends_nothing() {
    if !emoji_face() {
        return;
    }
    for text in [
        r#"Astronaut <tspan font-weight="bold">🧑‍🚀</tspan> here"#,
        r#"<tspan font-weight="bold">Sofa</tspan> 🛋️ and <tspan font-weight="bold">🐈</tspan>"#,
        r#"a <tspan font-style="italic">👍🏽 b</tspan> 👨‍👩‍👧"#,
    ] {
        assert!(colored(text) > 40, "{text}");
    }
}
