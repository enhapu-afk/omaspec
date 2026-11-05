//! Mermaid's `click` and `link` lines: a node, a class or a task that a
//! click opens an address from. A callback (`click A callback`,
//! `click A call f()`) runs a script in Mermaid; it is read and left out,
//! as Mermaid's strict security level leaves it out too.

use crate::text::Link;

/// What a `click` or `link` line says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Click {
    /// The node, class or task as written; a gantt chart's may list
    /// several, between commas.
    pub id: String,
    /// `None` for a callback.
    pub link: Option<Link>,
}

/// `text` as a `click` or `link` line; `None` for any other line, and for
/// one without an id and something to do. The address is quoted, after
/// `href` or straight after the id; a quoted tooltip may follow, then a
/// target (`_blank`), which is not kept.
pub fn click(text: &str) -> Option<Click> {
    let text = text.trim();
    let (word, rest) = text.split_once(char::is_whitespace)?;
    if !word.eq_ignore_ascii_case("click") && !word.eq_ignore_ascii_case("link") {
        return None;
    }
    let (id, rest) = rest.trim_start().split_once(char::is_whitespace)?;
    let rest = rest.trim_start();
    let rest = match rest.strip_prefix("href") {
        Some(after) if after.starts_with(char::is_whitespace) => after.trim_start(),
        _ => rest,
    };
    let Some((url, rest)) = quoted(rest) else {
        // A callback: a function's name, or `call` and its call.
        return (!rest.is_empty()).then(|| Click {
            id: id.to_owned(),
            link: None,
        });
    };
    let tooltip = quoted(rest.trim_start())
        .map(|(tooltip, _)| tooltip.trim())
        .filter(|tooltip| !tooltip.is_empty())
        .map(str::to_owned);
    let url = url.trim();
    Some(Click {
        id: id.to_owned(),
        link: (!url.is_empty()).then(|| Link {
            url: url.to_owned(),
            tooltip,
        }),
    })
}

/// The text between the double quotes `text` starts with, and what
/// follows them.
fn quoted(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_prefix('"')?;
    let end = inner.find('"')?;
    Some((&inner[..end], &inner[end + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to(id: &str, url: &str, tooltip: Option<&str>) -> Option<Click> {
        Some(Click {
            id: id.into(),
            link: Some(Link {
                url: url.into(),
                tooltip: tooltip.map(Into::into),
            }),
        })
    }

    #[test]
    fn an_address_after_href_or_after_the_id() {
        assert_eq!(
            click(r#"click A "https://a.b""#),
            to("A", "https://a.b", None)
        );
        assert_eq!(
            click(r#"click A href "https://a.b" "Tip" _blank"#),
            to("A", "https://a.b", Some("Tip"))
        );
        assert_eq!(click(r#"click A "x.html" _self"#), to("A", "x.html", None));
        assert_eq!(
            click(r#"link Duck "docs/duck.html" "The duck""#),
            to("Duck", "docs/duck.html", Some("The duck"))
        );
        assert_eq!(
            click(r#"click a1,a2 href "https://a.b""#),
            to("a1,a2", "https://a.b", None)
        );
    }

    #[test]
    fn a_callback_is_read_and_has_no_link() {
        for text in [
            "click A callback",
            r#"click A callback "Tip""#,
            r#"click A call callback("x")"#,
            "click href callback",
        ] {
            assert!(click(text).is_some_and(|c| c.link.is_none()), "{text}");
        }
    }

    #[test]
    fn other_lines_are_not_clicks() {
        for text in [
            "A --> B",
            "click",
            "click A",
            "clicked A \"x\"",
            "linkStyle 0 stroke:red",
        ] {
            assert_eq!(click(text), None, "{text}");
        }
    }
}
