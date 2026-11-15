//! Emoji as they are drawn: one picture for each, however many characters
//! write it. A person and those a zero width joiner joins to it
//! (`👨‍👩‍👧`), a skin tone after it (`👍🏽`), two letters of a flag (`🇩🇪`),
//! the tags of a region's flag, the mark that asks for the picture
//! (`🛋️`, `❤️`) and a keycap's frame (`1️⃣`) are one picture of the emoji
//! face. The text is measured by them, and an export gives each to the
//! emoji face whole.

use std::ops::Range;

const JOINER: char = '\u{200D}';
const PICTURE: char = '\u{FE0F}';
const KEYCAP: char = '\u{20E3}';

/// Where `text` has an emoji, each as the bytes of the one picture it is
/// drawn as, in order.
pub fn emoji(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    // Nothing below the arrows can be one.
    if !text.chars().any(|c| u32::from(c) >= 0x2190) {
        return found;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let at = |i: usize| chars.get(i).map(|&(_, c)| c);
    let mut i = 0;
    while let Some(c) = at(i) {
        let asked = at(i + 1) == Some(PICTURE) && !c.is_whitespace() && !c.is_control();
        if !(is_picture(c) || asked) {
            i += 1;
            continue;
        }
        let start = chars[i].0;
        i += 1;
        // The second letter of a flag.
        if is_regional(c) && at(i).is_some_and(is_regional) {
            i += 1;
        }
        loop {
            match at(i) {
                Some(next) if is_skin_tone(next) || is_tag(next) => i += 1,
                Some(PICTURE | KEYCAP) => i += 1,
                Some(JOINER) if at(i + 1).is_some() => i += 2,
                _ => break,
            }
        }
        let end = chars.get(i).map_or(text.len(), |&(at, _)| at);
        found.push(start..end);
    }
    found
}

/// Whether the emoji face draws `c` as a picture of its own, without the
/// mark that asks for one.
pub(super) fn is_picture(c: char) -> bool {
    let code = u32::from(c);
    matches!(code, 0x1F000..=0x1FAFF) || super::emoji_sign(code)
}

fn is_regional(c: char) -> bool {
    matches!(u32::from(c), 0x1F1E6..=0x1F1FF)
}

fn is_skin_tone(c: char) -> bool {
    matches!(u32::from(c), 0x1F3FB..=0x1F3FF)
}

fn is_tag(c: char) -> bool {
    matches!(u32::from(c), 0xE0020..=0xE007F)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pictures(text: &str) -> Vec<&str> {
        emoji(text).into_iter().map(|at| &text[at]).collect()
    }

    #[test]
    fn each_emoji_is_one_picture_however_it_is_written() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let coder = "\u{1F9D1}\u{200D}\u{1F4BB}";
        let rainbow = "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}";
        let thumbs = "\u{1F44D}\u{1F3FD}";
        let sofa = "\u{1F6CB}\u{FE0F}";
        let germany = "\u{1F1E9}\u{1F1EA}";
        let france = "\u{1F1EB}\u{1F1F7}";
        let scotland = "\u{1F3F4}\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}";
        let heart = "\u{2764}\u{FE0F}";
        let fire_heart = "\u{2764}\u{FE0F}\u{200D}\u{1F525}";
        let one = "1\u{FE0F}\u{20E3}";
        let text = format!(
            "a {family} b {coder}{rainbow} {thumbs}{sofa} {germany}{france} {scotland} {heart} {fire_heart} {one} \u{1F408}\u{1F415} \u{2705}"
        );
        assert_eq!(
            pictures(&text),
            [
                family,
                coder,
                rainbow,
                thumbs,
                sofa,
                germany,
                france,
                scotland,
                heart,
                fire_heart,
                one,
                "\u{1F408}",
                "\u{1F415}",
                "\u{2705}"
            ]
        );
        // Letters, signs a text face draws and the mark asking for text
        // are none.
        assert!(emoji("plain \u{2192} \u{2713} \u{263A}\u{FE0E} \u{00E9}").is_empty());
    }
}
