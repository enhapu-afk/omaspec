//! Links laid over a picture: an invisible box over each place a click
//! opens something else, as a map's tile opens its diagram. Omaspec lays
//! them over an engine's SVG after it was sanitized, with links it made
//! itself from the project's paths: relative ones, which may go up a
//! folder (`../orders.svg`) and never leave the project. A viewer that
//! does not follow links (resvg, a PDF) draws nothing of them.

use std::fmt::Write as _;

use omaspec_uml::graph::Rect;
use roxmltree::Document;

/// A place in a picture that a click follows.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    /// Where it is, in the picture's units: its width and height as an
    /// image, before a `viewBox` scales it.
    pub rect: Rect,
    /// A relative path, as [`relative`] takes it.
    pub href: String,
    /// Shown when the pointer rests on it.
    pub title: Option<String>,
}

/// Whether `href` is a relative path: no scheme, not from the root, no
/// backslash, no query. `..` may be in it.
pub fn relative(href: &str) -> bool {
    let path = href.split('#').next().unwrap_or_default();
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', '?'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .next()
            .is_some_and(|first| !first.contains(':'))
}

/// `svg` with an invisible link over each of `anchors`; an anchor whose
/// link is not [`relative`] is left out.
pub fn lay(svg: &str, anchors: &[Anchor]) -> String {
    let anchors: Vec<&Anchor> = anchors.iter().filter(|a| relative(&a.href)).collect();
    if anchors.is_empty() {
        return svg.to_owned();
    }
    let Ok(doc) = Document::parse(svg) else {
        return svg.to_owned();
    };
    let root = doc.root_element();
    let Some(end) = svg[..root.range().end].rfind("</") else {
        return svg.to_owned();
    };
    // The picture's units into the root's user units.
    let number = |name: &str| {
        root.attribute(name)
            .and_then(|v| v.trim().parse::<f32>().ok())
    };
    let view: Vec<f32> = root
        .attribute("viewBox")
        .map(|v| {
            v.split([' ', ','])
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    let (min_x, min_y, scale_x, scale_y) =
        match (view.as_slice(), number("width"), number("height")) {
            ([x, y, w, h], Some(width), Some(height)) if width > 0.0 && height > 0.0 => {
                (*x, *y, w / width, h / height)
            }
            ([x, y, _, _], _, _) => (*x, *y, 1.0, 1.0),
            _ => (0.0, 0.0, 1.0, 1.0),
        };
    let mut layer = String::from(r#"<g class="omaspec-links">"#);
    for a in anchors {
        let _ = write!(layer, r#"<a href="{}">"#, escape(&a.href));
        if let Some(title) = &a.title {
            let _ = write!(layer, "<title>{}</title>", escape(title));
        }
        let _ = write!(
            layer,
            r##"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="#000000" fill-opacity="0"/></a>"##,
            min_x + a.rect.x * scale_x,
            min_y + a.rect.y * scale_y,
            a.rect.w * scale_x,
            a.rect.h * scale_y
        );
    }
    layer.push_str("</g>");
    format!("{}{layer}{}", &svg[..end], &svg[end..])
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_relative_paths_are_links_omaspec_lays() {
        for ok in [
            "orders.svg",
            "shop/orders.svg",
            "../orders.svg",
            "orders.svg#top",
        ] {
            assert!(relative(ok), "{ok}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "//host/x.svg",
            "https://example.org",
            "javascript:alert(1)",
            "file:///x",
            "a\\b.svg",
            "x.svg?y",
            "c:/x.svg",
        ] {
            assert!(!relative(bad), "{bad}");
        }
    }

    #[test]
    fn a_link_lies_over_its_place_in_the_root_s_units() {
        // Shown at half its size: the viewBox doubles each unit.
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 200 100"><rect width="200" height="100" fill="white"/></svg>"#;
        let laid = lay(
            svg,
            &[
                Anchor {
                    rect: Rect {
                        x: 10.0,
                        y: 5.0,
                        w: 20.0,
                        h: 10.0,
                    },
                    href: "orders.svg".into(),
                    title: Some("Orders & co".into()),
                },
                Anchor {
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 1.0,
                        h: 1.0,
                    },
                    href: "javascript:alert(1)".into(),
                    title: None,
                },
            ],
        );
        assert!(laid.contains(r#"<a href="orders.svg"><title>Orders &amp; co</title><rect x="20.00" y="10.00" width="40.00" height="20.00""#), "{laid}");
        assert!(!laid.contains("javascript"));
        assert!(laid.ends_with("</g></svg>"));
        // Nothing of it shows.
        let tree = crate::export::parse(&laid).unwrap();
        let png = crate::export::rasterize(&tree, 1.0).unwrap();
        let p = png.pixel(15, 8).unwrap();
        assert_eq!((p.red(), p.green(), p.blue()), (255, 255, 255));
    }
}
