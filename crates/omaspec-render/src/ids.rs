//! An SVG's ids made its own, for a page that holds several pictures
//! (REQ-117): each id and each reference to one starts with a prefix.

/// `svg` with each of its ids, and every reference to one, starting with
/// `prefix`: an `id`, a `href` or `xlink:href` to `#…`, a `url(#…)` in an
/// attribute. Only tags are read: words, the stored source, comments and
/// style sheets (which name no id: the sanitizer leaves no `url(#…)` in
/// one) are left as they are, so a note that says `url(#x)` still does.
pub fn prefix_ids(svg: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(svg.len() + 64);
    let mut rest = svg;
    'tags: while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        for (start, end) in [("<!--", "-->"), ("<![CDATA[", "]]>")] {
            if rest.starts_with(start) {
                let close = rest.find(end).map_or(rest.len(), |at| at + end.len());
                out.push_str(&rest[..close]);
                rest = &rest[close..];
                continue 'tags;
            }
        }
        let close = tag_end(rest);
        out.push_str(&prefixed_tag(&rest[..close], prefix));
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

/// Whether `prefix` may start an id: a letter or `_`, then letters,
/// digits, `_`, `-` and `.`, as XML's names are written.
pub fn is_id_prefix(prefix: &str) -> bool {
    let mut chars = prefix.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Where the tag `tag` starts with ends: after its `>` outside quotes.
fn tag_end(tag: &str) -> usize {
    let mut quote = None;
    for (at, c) in tag.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '>') => return at + 1,
            _ => {}
        }
    }
    tag.len()
}

/// `tag` with the values of its attributes prefixed where they name an id.
fn prefixed_tag(tag: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(tag.len() + 16);
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let after = &rest[eq + 1..];
        let Some(quote) = after.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            out.push_str(&rest[..=eq]);
            rest = after;
            continue;
        };
        let Some(end) = after[1..].find(quote) else {
            break;
        };
        let name = rest[..eq]
            .trim_end()
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or_default();
        let value = &after[1..=end];
        out.push_str(&rest[..=eq]);
        out.push(quote);
        match name {
            "id" => {
                out.push_str(prefix);
                out.push_str(value);
            }
            "href" | "xlink:href" if value.starts_with('#') => {
                out.push('#');
                out.push_str(prefix);
                out.push_str(&value[1..]);
            }
            _ => out.push_str(&value.replace("url(#", &format!("url(#{prefix}"))),
        }
        out.push(quote);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ids, the references to them in attributes and links take the
    /// prefix; the words, the stored source and a comment do not.
    #[test]
    fn ids_and_their_references_take_the_prefix_and_words_do_not() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><!-- id="c" --><metadata><sf:source xmlns:sf="urn:omaspec:source">note : url(#x) id="y" href="#z"</sf:source></metadata><defs><linearGradient id='g'/><marker id="m"/></defs><g id="lines"><path fill="url(#g)" marker-end="url(#m)" style="filter:url(#m)"/><use href="#m"/><a xlink:href="#lines" href="https://x.org/#top"><text>url(#x) and id="y"</text></a></g></svg>"##;
        let owned = prefix_ids(svg, "p1-");
        for kept in [
            r#"<!-- id="c" -->"#,
            r##"note : url(#x) id="y" href="#z"</sf:source>"##,
            r#"<text>url(#x) and id="y"</text>"#,
            r##"href="https://x.org/#top""##,
            r#"xmlns:sf="urn:omaspec:source""#,
        ] {
            assert!(owned.contains(kept), "{kept}: {owned}");
        }
        for made in [
            "id='p1-g'",
            r#"id="p1-m""#,
            r#"id="p1-lines""#,
            r#"fill="url(#p1-g)""#,
            r#"marker-end="url(#p1-m)""#,
            r#"style="filter:url(#p1-m)""#,
            r##"<use href="#p1-m"/>"##,
            r##"xlink:href="#p1-lines""##,
        ] {
            assert!(owned.contains(made), "{made}: {owned}");
        }
    }

    #[test]
    fn a_prefix_starts_as_an_id_does() {
        for good in ["a", "_x", "door-1.", "p1-"] {
            assert!(is_id_prefix(good), "{good}");
        }
        for bad in ["", "1a", "-a", "a b", "a\"", "a#", "ä"] {
            assert!(!is_id_prefix(bad), "{bad}");
        }
    }
}
