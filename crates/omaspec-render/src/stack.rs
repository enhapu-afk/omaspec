//! Pictures one under another in one SVG: a Markdown document's diagrams
//! as the preview window shows them. Each keeps its own size and look,
//! inside an `<svg>` of its own with its ids made its own, centered across
//! the widest; the result is sanitized again.

use std::fmt::Write as _;

use roxmltree::Document;

use crate::sanitize::{self, SanitizeError};

/// Between two pictures, in the pictures' units.
pub const GAP: f32 = 24.0;

/// `pictures` (sanitized SVG) one under another, `GAP` apart.
pub fn stack(pictures: &[&str]) -> Result<String, SanitizeError> {
    let mut parts = Vec::with_capacity(pictures.len());
    for (index, svg) in pictures.iter().enumerate() {
        let Some(part) = Part::of(svg, &format!("d{}-", index + 1)) else {
            continue;
        };
        parts.push(part);
    }
    let width = parts.iter().map(|p| p.width).fold(0.0, f32::max);
    let height =
        parts.iter().map(|p| p.height).sum::<f32>() + GAP * parts.len().saturating_sub(1) as f32;
    let mut out = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{0}" height="{1}" viewBox="0 0 {0} {1}">"#,
        number(width),
        number(height)
    );
    let mut y = 0.0;
    for part in &parts {
        let _ = write!(
            out,
            r#"<svg x="{}" y="{}" width="{}" height="{}" viewBox="{}">{}</svg>"#,
            number((width - part.width) / 2.0),
            number(y),
            number(part.width),
            number(part.height),
            part.view_box,
            part.body
        );
        y += part.height + GAP;
    }
    out.push_str("</svg>");
    sanitize::sanitize(&out)
}

/// One picture: its size, its view box and what it draws.
struct Part {
    width: f32,
    height: f32,
    view_box: String,
    body: String,
}

impl Part {
    fn of(svg: &str, prefix: &str) -> Option<Part> {
        let doc = Document::parse(svg).ok()?;
        let root = doc.root_element();
        let length = |name: &str| {
            root.attribute(name)
                .map(|v| v.trim().trim_end_matches("px"))
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| *v > 0.0)
        };
        let boxed: Vec<f32> = root
            .attribute("viewBox")
            .map(|v| {
                v.split([' ', ','])
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        let (width, height) = match (length("width"), length("height"), boxed.as_slice()) {
            (Some(w), Some(h), _) => (w, h),
            (_, _, [_, _, w, h]) if *w > 0.0 && *h > 0.0 => (*w, *h),
            _ => return None,
        };
        let view_box = match boxed.as_slice() {
            [x, y, w, h] => format!(
                "{} {} {} {}",
                number(*x),
                number(*y),
                number(*w),
                number(*h)
            ),
            _ => format!("0 0 {} {}", number(width), number(height)),
        };
        let range = root.range();
        let open = svg[range.start..].find('>')? + range.start + 1;
        let close = svg[..range.end].rfind("</")?;
        let body = svg.get(open..close).unwrap_or_default();
        Some(Part {
            width,
            height,
            view_box,
            body: crate::atlas::own_ids(body, prefix),
        })
    }
}

fn number(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, RenderError, Renderer};

    #[test]
    fn a_document_s_diagrams_stand_one_under_another() {
        let mut renderer = Renderer::new(&Config {
            java_fallback: false,
            ..Config::default()
        });
        let doc = "# Door\n\n```plantuml\n[*] --> Closed\n```\n\nText.\n\n```mermaid\npie\n  \"a\": 1\n```\n";
        let drawn = renderer.render_document(doc, None).unwrap();
        // The page and one picture each.
        assert_eq!(drawn.svg.matches("<svg").count(), 3, "{}", drawn.svg);
        // An error names the document's line.
        let broken = format!("{doc}\n```plantuml\nstate X {{\n```\n");
        let err = renderer.render_document(&broken, None).unwrap_err();
        let (RenderError::Unavailable { line, .. } | RenderError::Diagram { line, .. }) = &err
        else {
            panic!("{err:?}");
        };
        // Zero-based: `state X {` on the document's fifteenth line.
        assert_eq!(*line, Some(14), "{err:?}");
        let none = renderer.render_document("# Notes\n", None).unwrap_err();
        assert!(
            none.to_string().contains("no PlantUML or Mermaid diagram"),
            "{none}"
        );
    }

    #[test]
    fn pictures_stand_one_under_another_with_ids_of_their_own() {
        let a = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40" viewBox="0 0 100 40"><defs><marker id="m"/></defs><rect width="100" height="40" fill="#ffffff"/><path marker-end="url(#m)"/></svg>"##;
        let b = r##"<svg xmlns="http://www.w3.org/2000/svg" width="60px" height="20px"><defs><marker id="m"/></defs><rect width="60" height="20" fill="#000000"/></svg>"##;
        let both = stack(&[a, b]).unwrap();
        assert!(both.contains(r#"width="100" height="84""#), "{both}");
        assert!(
            both.contains(r#"<svg x="20" y="64" width="60" height="20" viewBox="0 0 60 20">"#),
            "{both}"
        );
        assert!(both.contains(r##"id="d1-m""##) && both.contains(r##"id="d2-m""##));
        assert!(both.contains("url(#d1-m)"));
        // A picture without a size is left out.
        let sizeless = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>"#;
        let one = stack(&[a, sizeless]).unwrap();
        assert!(one.contains(r#"width="100" height="40""#), "{one}");
        // Drawn, the second lies below the first.
        let tree = crate::export::parse(&both).unwrap();
        let png = crate::export::rasterize(&tree, 1.0).unwrap();
        let p = png.pixel(50, 70).unwrap();
        assert_eq!((p.red(), p.green(), p.blue()), (0, 0, 0));
        let q = png.pixel(50, 20).unwrap();
        assert_eq!((q.red(), q.green(), q.blue()), (255, 255, 255));
    }
}
