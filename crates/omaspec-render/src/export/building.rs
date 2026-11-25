//! Code becomes a picture, as a page: the source beside the diagram, which
//! builds itself up line by line, in vectors and sharp at any size
//! (`omaspec play FILE --build -o build.html`). The page holds the source,
//! each line in its colors, and the picture drawn with its parts marked
//! ([ADR 0029](../../../../docs/adr/0029-a-glide-by-parts.md)), and when
//! each line comes and what it sets down: its parts, its lines, or how far
//! down it wrote. The script only walks that: it grows each part's own
//! groups in, draws each line along its points to its head, and moves the
//! caret down the source.

use omaspec_uml::Color;
use omaspec_uml::map::{Head, HeadFill};
use serde_json::{Value, json};

use super::{ExportError, Source};

const SCRIPT: &str = include_str!("building.js");
const STYLE: &str = include_str!("building.css");

/// What a page of a build plays.
#[derive(Debug, Clone)]
pub struct Building {
    /// The picture, its parts marked, and its size.
    pub svg: String,
    pub size: (f32, f32),
    /// The source, each line as its runs of words with their color.
    pub lines: Vec<Vec<Run>>,
    /// When the caret reaches each line, in seconds; when the build ends,
    /// and how long the whole stays after.
    pub starts: Vec<f32>,
    pub end: f32,
    pub hold: f32,
    /// How long a box grows in, a line draws itself, its words come;
    /// how far into its boxes' growing a line starts; and how much faster
    /// than that a long source goes.
    pub grow: f32,
    pub draw: f32,
    pub words: f32,
    pub overlap: f32,
    pub pace: f32,
    /// What each line sets down, by line: whether boxes.
    pub boxes: Vec<bool>,
    pub parts: Vec<PartAt>,
    pub wires: Vec<WireAt>,
    /// Without parts: how far down each line wrote, by line, and how far
    /// down the picture shows before any line.
    pub reach: Vec<(usize, f32)>,
    pub first_reach: f32,
    /// The transform the picture's lines' group draws in, as SVG's
    /// `matrix(a b c d e f)` takes it, inverted: the lines are drawn anew
    /// in the picture's own units.
    pub lines_matrix: Option<[f32; 6]>,
    /// The page's ground and its colors: the panel, its words, the
    /// numbers, the caret's line and the caret.
    pub colors: Colors,
    /// Which lines are comments alone, left out of the source shown.
    pub comments: Vec<bool>,
    /// The diagram alone, its source not beside it.
    pub bare: bool,
}

/// Words of one color.
#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Debug, Clone)]
pub struct PartAt {
    /// The id its groups start with.
    pub prefix: String,
    pub line: usize,
    pub middle: (f32, f32),
    /// The words of a line: they come after it is drawn.
    pub words: bool,
}

#[derive(Debug, Clone)]
pub struct WireAt {
    pub line: usize,
    pub points: Vec<(f32, f32)>,
    pub heads: Vec<Head>,
    pub color: Color,
    pub width: f32,
    pub dash: Option<(f32, f32)>,
}

#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub paper: Color,
    pub panel: Color,
    pub ink: Color,
    pub muted: Color,
    pub selection: Color,
    pub accent: Color,
    /// The picture's own paper, which an empty head is filled with.
    pub picture: Color,
}

/// The page that plays `building`, called `title`, with the diagram's
/// `source` in its picture. The picture is sanitized; only the source is
/// written after.
pub fn page(
    building: &Building,
    source: Option<Source>,
    title: &str,
) -> Result<String, ExportError> {
    let svg = crate::fonts::embed_with(&building.svg, &[]);
    let svg = crate::sanitize::sanitize(&svg).map_err(|e| ExportError::Svg(e.to_string()))?;
    let svg = match source {
        Some(s) => omaspec_core::embed::svg_with_source(&svg, s.code, s.language, s.file),
        None => svg,
    };
    let c = building.colors;
    let mut out = String::with_capacity(svg.len() + SCRIPT.len() + 8192);
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    // Nothing from anywhere: what the page shows is in it.
    out.push_str(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:\">\n",
    );
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<meta name=\"generator\" content=\"Omaspec\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape(title)));
    out.push_str(&format!(
        "<style>\n:root {{ --paper: {}; --panel: {}; --ink: {}; --muted: {}; --selection: {}; --accent: {}; }}\n{STYLE}</style>\n",
        c.paper, c.panel, c.ink, c.muted, c.selection, c.accent
    ));
    out.push_str(match building.bare {
        true => "</head>\n<body>\n<main id=\"build\" class=\"bare\">\n<pre id=\"source\">",
        false => "</head>\n<body>\n<main id=\"build\">\n<pre id=\"source\">",
    });
    for (at, runs) in building.lines.iter().enumerate() {
        let comment = match building.comments.get(at) {
            Some(true) => " comment",
            _ => "",
        };
        out.push_str(&format!(
            "<span class=\"line ahead{comment}\" data-line=\"{at}\"><span class=\"number\">{}</span>",
            at + 1
        ));
        for run in runs {
            let mut style = format!("color:{}", run.color);
            if run.bold {
                style.push_str(";font-weight:bold");
            }
            if run.italic {
                style.push_str(";font-style:italic");
            }
            out.push_str(&format!(
                "<span style=\"{style}\">{}</span>",
                escape(&run.text)
            ));
        }
        out.push_str("</span>");
    }
    out.push_str("</pre>\n<div id=\"diagram\">\n");
    out.push_str(&svg);
    out.push_str("\n</div>\n</main>\n");
    out.push_str(&format!(
        "<script type=\"application/json\" id=\"play\">{}</script>\n",
        data(building).to_string().replace('<', "\\u003c")
    ));
    out.push_str(&format!("<script>\n{SCRIPT}</script>\n</body>\n</html>\n"));
    Ok(out)
}

fn data(b: &Building) -> Value {
    let point = |&(x, y): &(f32, f32)| json!([round(x), round(y)]);
    json!({
        "kind": "build",
        "size": [round(b.size.0), round(b.size.1)],
        "starts": b.starts.iter().map(|&s| round(s)).collect::<Vec<_>>(),
        "end": round(b.end),
        "hold": round(b.hold),
        "grow": round(b.grow),
        "draw": round(b.draw),
        "words": round(b.words),
        "overlap": round(b.overlap),
        "pace": round(b.pace),
        "boxes": b.boxes,
        "parts": b.parts.iter().map(|part| json!({
            "prefix": part.prefix,
            "line": part.line,
            "middle": point(&part.middle),
            "words": part.words,
        })).collect::<Vec<_>>(),
        "wires": b.wires.iter().map(|wire| json!({
            "line": wire.line,
            "points": wire.points.iter().map(point).collect::<Vec<_>>(),
            "heads": wire.heads.iter().map(|head| json!({
                "points": head.points.iter().map(point).collect::<Vec<_>>(),
                "closed": head.closed,
                "fill": match head.fill {
                    HeadFill::Ink => wire.color.to_string(),
                    HeadFill::Paper => b.colors.picture.to_string(),
                    HeadFill::None => "none".to_owned(),
                },
            })).collect::<Vec<_>>(),
            "color": wire.color.to_string(),
            "width": round(wire.width),
            "dash": wire.dash.map(|(on, off)| json!([round(on), round(off)])),
        })).collect::<Vec<_>>(),
        "reach": b.reach.iter().map(|&(line, y)| json!([line, round(y)])).collect::<Vec<_>>(),
        "firstReach": round(b.first_reach),
        "linesMatrix": b.lines_matrix.map(|m| m.iter().map(|&v| round(v)).collect::<Vec<_>>()),
    })
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

/// `text` for HTML: `&`, `<`, `>` and `"` written as entities.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn building() -> Building {
        Building {
            svg: r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><g id="part-61-1"><rect width="10" height="10" fill="#ccc"/></g><g id="lines"></g></svg>"##.to_owned(),
            size: (40.0, 20.0),
            lines: vec![vec![Run {
                text: "a --> <b>".to_owned(),
                color: Color::BLACK,
                bold: false,
                italic: false,
            }]],
            starts: vec![0.5],
            end: 1.0,
            hold: 2.0,
            grow: 0.26,
            draw: 0.34,
            words: 0.14,
            overlap: 0.5,
            pace: 1.0,
            boxes: vec![true],
            parts: vec![PartAt {
                prefix: "part-61-".to_owned(),
                line: 0,
                middle: (5.0, 5.0),
                words: false,
            }],
            wires: Vec::new(),
            reach: Vec::new(),
            first_reach: 0.0,
            lines_matrix: Some([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
            colors: Colors {
                paper: Color::WHITE,
                panel: Color::WHITE,
                ink: Color::BLACK,
                muted: Color::BLACK,
                selection: Color::WHITE,
                accent: Color::BLACK,
                picture: Color::WHITE,
            },
            comments: Vec::new(),
            bare: false,
        }
    }

    #[test]
    fn a_page_holds_its_source_its_picture_and_its_play() {
        let page = page(&building(), None, "door <1>").unwrap();
        assert!(page.contains("<title>door &lt;1&gt;</title>"));
        // The source's words escaped, the picture sanitized, the play's
        // data where the script reads it.
        assert!(page.contains("a --&gt; &lt;b&gt;"), "{page}");
        assert!(page.contains(r#"id="part-61-1""#), "{page}");
        assert!(page.contains(r#"<script type="application/json" id="play">{"#));
        assert!(page.contains(r#""kind":"build""#));
        // Nothing from anywhere.
        assert!(page.contains("default-src 'none'"));
    }

    /// A line of comments is a row the page hides; a build without its
    /// source hides the source.
    #[test]
    fn a_page_hides_its_comments_and_may_hide_its_source() {
        let mut built = building();
        built.comments = vec![true];
        let written = page(&built, None, "door").unwrap();
        assert!(
            written.contains(r#"class="line ahead comment" data-line="0""#),
            "{written}"
        );
        assert!(written.contains("#source .line.comment"), "{written}");
        assert!(written.contains(r#"<main id="build">"#));
        built.bare = true;
        let written = page(&built, None, "door").unwrap();
        assert!(
            written.contains(r#"<main id="build" class="bare">"#),
            "{written}"
        );
    }
}
