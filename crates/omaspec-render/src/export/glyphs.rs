//! The characters of a picture that no installed face draws: an emoji
//! where no emoji face is installed, say. resvg draws each as an empty box,
//! as any program would, so `render` says which ones, and what to install.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use resvg::usvg;

/// Whether the face `id` has a glyph for `c`.
pub(super) fn face_has(fonts: &usvg::fontdb::Database, id: usvg::fontdb::ID, c: char) -> bool {
    fonts
        .with_face_data(id, |data, index| {
            use skrifa::MetadataProvider;
            skrifa::FontRef::from_index(data, index)
                .ok()
                .and_then(|face| face.charmap().map(c))
                .is_some()
        })
        .unwrap_or(false)
}

/// The characters of `tree`'s words that no face in `fonts` draws, each
/// once, in the order they first stand.
pub(super) fn undrawable(tree: &usvg::Tree, fonts: &usvg::fontdb::Database) -> Vec<char> {
    let mut asked = Vec::new();
    words(tree.root(), &mut |text| {
        for c in text.chars().filter(|c| needs_a_face(*c)) {
            if !asked.contains(&c) {
                asked.push(c);
            }
        }
    });
    asked.retain(|c| !drawn(fonts, *c));
    asked
}

/// Whether some face draws `c`. Every face is opened to ask it, so an
/// answer is kept.
fn drawn(fonts: &usvg::fontdb::Database, c: char) -> bool {
    static KNOWN: LazyLock<Mutex<HashMap<char, bool>>> = LazyLock::new(Mutex::default);
    if let Some(known) = KNOWN.lock().unwrap_or_else(PoisonError::into_inner).get(&c) {
        return *known;
    }
    let found = fonts.faces().any(|face| face_has(fonts, face.id, c));
    KNOWN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(c, found);
    found
}

/// Whether `c` is drawn with a glyph of its own: not ASCII, which every
/// text face has, and not a space, a control or a joiner or selector that
/// only shapes the characters around it.
fn needs_a_face(c: char) -> bool {
    !(c.is_ascii()
        || c.is_whitespace()
        || c.is_control()
        || matches!(
            c,
            '\u{200B}'..='\u{200F}'
                | '\u{2060}'..='\u{206F}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{E0000}'..='\u{E007F}'
        ))
}

/// Hands the text of every word run in `group` to `each`.
fn words(group: &usvg::Group, each: &mut impl FnMut(&str)) {
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => words(group, each),
            usvg::Node::Text(text) => {
                for chunk in text.chunks() {
                    each(chunk.text());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_spaces_and_joiners_need_no_face_of_their_own() {
        for c in ['a', ' ', '\n', '\u{200D}', '\u{FE0F}'] {
            assert!(!needs_a_face(c), "{c:?}");
        }
        for c in ['ü', '🍕', '✅', '→'] {
            assert!(needs_a_face(c), "{c:?}");
        }
    }

    #[test]
    fn a_character_no_face_has_is_found_once() {
        // A character Unicode has not assigned, so no face draws it, twice,
        // beside letters every face draws.
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="40"><text x="4" y="20" font-family="sans-serif">a&#x378;b &#x378;</text></svg>"#;
        let tree = super::super::parse(svg).unwrap();
        let fonts = super::super::fonts();
        assert_eq!(undrawable(&tree, fonts), ['\u{378}']);
        // A letter is never asked about.
        let plain = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="40"><text x="4" y="20">plain</text></svg>"#;
        assert!(undrawable(&super::super::parse(plain).unwrap(), fonts).is_empty());
    }
}
