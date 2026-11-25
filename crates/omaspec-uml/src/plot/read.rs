//! A chart's lines read: the axes, the series, the layout's few words, and
//! the frame's lines around them. A number no chart can draw (`Infinity`,
//! `NaN`, `1e999`) and an axis that runs nowhere or backwards are mistakes
//! on their lines, which PlantUML refuses too; a line Omaspec does not
//! read is left to the Java PlantUML.

use super::{Annotation, Axis, Diagram, Kind, Place, Series, Side, Values};
use crate::common::Frame;
use crate::source::Line;
use crate::style::{Element, Prop};
use crate::{Declined, color};

/// What `<style>` reaches in a chart.
pub(super) const STYLE: &[Element] = &[
    Element::new(
        &["document", "chartDiagram"],
        "Chart",
        &[Prop::BackGroundColor, Prop::FontColor, Prop::FontSize],
    ),
    Element::new(
        &["document", "chartDiagram", "bar"],
        "ChartBar",
        &[Prop::BackGroundColor, Prop::LineColor, Prop::LineThickness],
    )
    .own(&[(Prop::BarWidth, "ChartBarWidth")]),
    Element::new(
        &["document", "chartDiagram", "line"],
        "ChartLine",
        &[Prop::LineColor, Prop::LineThickness],
    ),
    Element::new(
        &["document", "chartDiagram", "area"],
        "ChartArea",
        &[Prop::BackGroundColor, Prop::LineColor, Prop::LineThickness],
    ),
    Element::new(
        &["document", "chartDiagram", "scatter"],
        "ChartScatter",
        &[],
    )
    .own(&[
        (Prop::MarkerShape, "ChartScatterMarkerShape"),
        (Prop::MarkerSize, "ChartScatterMarkerSize"),
        (Prop::MarkerColor, "ChartScatterMarkerColor"),
    ]),
    Element::new(
        &["document", "chartDiagram", "axis"],
        "ChartAxis",
        &[
            Prop::LineColor,
            Prop::LineThickness,
            Prop::FontColor,
            Prop::FontSize,
        ],
    ),
    Element::new(
        &["document", "chartDiagram", "grid"],
        "ChartGrid",
        &[Prop::LineColor, Prop::LineThickness],
    ),
    Element::new(
        &["document", "chartDiagram", "legend"],
        "ChartLegend",
        &[
            Prop::BackGroundColor,
            Prop::LineColor,
            Prop::FontColor,
            Prop::FontSize,
        ],
    )
    .own(&[
        (Prop::Padding, "ChartLegendPadding"),
        (Prop::Margin, "ChartLegendMargin"),
    ]),
    Element::new(
        &["document", "chartDiagram", "annotation"],
        "ChartAnnotation",
        &[Prop::LineColor, Prop::FontColor, Prop::FontSize],
    ),
    // A series by its class (`<<highlights>>` under `.highlights { … }`).
    Element::new(
        &["document", "chartDiagram", "series"],
        "ChartSeries",
        &[Prop::BackGroundColor, Prop::LineColor, Prop::LineThickness],
    )
    .own(&[
        (Prop::MarkerShape, "ChartSeriesMarkerShape"),
        (Prop::MarkerSize, "ChartSeriesMarkerSize"),
        (Prop::MarkerColor, "ChartSeriesMarkerColor"),
    ])
    .varied(),
];

/// The most ticks an axis takes: a `spacing` finer than this is a mistake.
pub(super) const MOST_TICKS: f64 = 200.0;

/// Reads the chart in `lines`.
pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let first = lines.first().map_or(0, |line| line.no);
    let mut d = Diagram {
        h: Axis {
            line: first,
            ..Axis::default()
        },
        v: Axis {
            line: first,
            ..Axis::default()
        },
        v2: None,
        series: Vec::new(),
        horizontal: false,
        stacked: false,
        legend: None,
        annotations: Vec::new(),
        frame: Frame::styled(STYLE),
    };
    let mut at = 0;
    while at < lines.len() {
        let Line { no, text, .. } = lines[at];
        // `legend right` places the chart's legend; a legend block of the
        // frame's runs on to `endlegend`, before another legend opens.
        if let Some(side) = legend_side(text)
            && !opens_block(&lines[at + 1..])
        {
            d.legend = Some(side);
            at += 1;
            continue;
        }
        if let Some(next) = d.frame.line(lines, at)? {
            at = next;
            continue;
        }
        statement(&mut d, text, no)?;
        at += 1;
    }
    check(&mut d)?;
    Ok(d)
}

/// Whether `endlegend` closes a legend opened before `rest`: it comes
/// before the next line that opens a legend.
fn opens_block(rest: &[Line]) -> bool {
    for line in rest {
        let words: String = line.text.split_whitespace().collect();
        if words.eq_ignore_ascii_case("endlegend") {
            return true;
        }
        let first = line.text.split_whitespace().next().unwrap_or("");
        if first.eq_ignore_ascii_case("legend") {
            return false;
        }
    }
    false
}

/// `legend left|right|top|bottom` on its own.
fn legend_side(text: &str) -> Option<Side> {
    let mut words = text.split_whitespace();
    if !words.next()?.eq_ignore_ascii_case("legend") {
        return None;
    }
    let side = match words.next()?.to_ascii_lowercase().as_str() {
        "left" => Side::Left,
        "right" => Side::Right,
        "top" => Side::Top,
        "bottom" => Side::Bottom,
        _ => return None,
    };
    words.next().is_none().then_some(side)
}

fn statement(d: &mut Diagram, text: &str, no: usize) -> Result<(), Declined> {
    let (word, rest) = text
        .split_once(char::is_whitespace)
        .map_or((text, ""), |(word, rest)| (word, rest.trim()));
    let word = word.to_ascii_lowercase();
    let kind = match word.as_str() {
        "h-axis" => {
            d.h = axis(rest, no)?;
            return Ok(());
        }
        "v-axis" => {
            d.v = axis(rest, no)?;
            return Ok(());
        }
        "v2-axis" => {
            d.v2 = Some(axis(rest, no)?);
            return Ok(());
        }
        "stackmode" => {
            d.stacked = match rest.to_ascii_lowercase().as_str() {
                "stacked" => true,
                "grouped" => false,
                _ => return Err(unread(text, no)),
            };
            return Ok(());
        }
        "orientation" => {
            d.horizontal = match rest.to_ascii_lowercase().as_str() {
                "horizontal" => true,
                "vertical" => false,
                _ => return Err(unread(text, no)),
            };
            return Ok(());
        }
        "annotation" => {
            let note = annotation(rest, no)?;
            d.annotations.push(note);
            return Ok(());
        }
        "bar" => Kind::Bar,
        "line" => Kind::Line,
        "area" => Kind::Area,
        "scatter" => Kind::Scatter,
        _ => return Err(unread(text, no)),
    };
    let series = series(kind, rest, no)?;
    d.series.push(series);
    Ok(())
}

fn unread(text: &str, no: usize) -> Declined {
    Declined::at(
        no,
        format!("`{text}` is no line of a chart Omaspec reads yet"),
    )
}

/// A piece of a line: words in quotes, a list in brackets, a class in
/// `<<…>>`, a place in parentheses, a color after `#`, or a word.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Token<'a> {
    Quoted(&'a str),
    List(&'a str),
    Class(&'a str),
    Paren(&'a str),
    Color(&'a str),
    Word(&'a str),
}

fn tokens(text: &str, no: usize) -> Result<Vec<Token<'_>>, Declined> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    // The end of a run that opens with `open` and closes with `close`,
    // quotes inside it kept whole.
    let closing = |text: &str, close: char| -> Option<usize> {
        let mut quoted = false;
        for (at, c) in text.char_indices() {
            match c {
                '"' => quoted = !quoted,
                c if c == close && !quoted => return Some(at),
                _ => {}
            }
        }
        None
    };
    let unclosed = |what: &str| Declined::refused(no, format!("this {what} is not closed"));
    while !rest.is_empty() {
        let (token, used) = if let Some(after) = rest.strip_prefix('"') {
            let end = after.find('"').ok_or_else(|| unclosed("quote"))?;
            (Token::Quoted(&after[..end]), end + 2)
        } else if let Some(after) = rest.strip_prefix("<<") {
            let end = after.find(">>").ok_or_else(|| unclosed("`<<`"))?;
            (Token::Class(after[..end].trim()), end + 4)
        } else if let Some(after) = rest.strip_prefix('[') {
            let end = closing(after, ']').ok_or_else(|| unclosed("`[`"))?;
            (Token::List(&after[..end]), end + 2)
        } else if let Some(after) = rest.strip_prefix('(') {
            let end = closing(after, ')').ok_or_else(|| unclosed("`(`"))?;
            (Token::Paren(&after[..end]), end + 2)
        } else if let Some(after) = rest.strip_prefix('#') {
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            (Token::Color(&after[..end]), end + 1)
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let word = &rest[..end];
            // `0-->100` is three words.
            match word.find("-->") {
                Some(0) => (Token::Word("-->"), 3),
                Some(at) => (Token::Word(&word[..at]), at),
                None => (Token::Word(word), end),
            }
        };
        out.push(token);
        rest = rest[used.min(rest.len())..].trim_start();
    }
    Ok(out)
}

/// A number a chart can draw; any other is a mistake on the line.
fn number(text: &str, no: usize) -> Result<f64, Declined> {
    let text = text.trim();
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        Ok(_) => Err(Declined::refused(
            no,
            format!("`{text}` is no number a chart can draw"),
        )),
        Err(_) => Err(Declined::refused(no, format!("`{text}` is no number"))),
    }
}

/// Text without the quotes around it.
fn unquoted(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
}

/// The items of a list, split at the commas outside quotes.
fn items(list: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut quoted, mut depth) = (0, false, 0);
    for (at, c) in list.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => {
                out.push(list[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(list[start..].trim());
    out.retain(|item| !item.is_empty());
    out
}

fn axis(rest: &str, no: usize) -> Result<Axis, Declined> {
    let mut axis = Axis {
        line: no,
        ..Axis::default()
    };
    let tokens = tokens(rest, no)?;
    let mut at = 0;
    let wrong = || Declined::at(no, format!("`{rest}` is no axis Omaspec reads yet"));
    while at < tokens.len() {
        match tokens[at] {
            Token::Quoted(title) if axis.title.is_none() => axis.title = Some(title.to_owned()),
            Token::List(list) if axis.categories.is_empty() && axis.range.is_none() => {
                axis.categories = items(list)
                    .into_iter()
                    .map(|item| unquoted(item).to_owned())
                    .collect();
            }
            Token::Word(low) if tokens.get(at + 1) == Some(&Token::Word("-->")) => {
                let Some(Token::Word(high)) = tokens.get(at + 2) else {
                    return Err(wrong());
                };
                let (low, high) = (number(low, no)?, number(high, no)?);
                if low >= high {
                    return Err(Declined::refused(
                        no,
                        format!(
                            "`{low} --> {high}`: an axis runs from a lower value to a higher one"
                        ),
                    ));
                }
                if !(high - low).is_finite() {
                    return Err(Declined::refused(
                        no,
                        "this axis runs too far for a chart to draw",
                    ));
                }
                axis.range = Some((low, high));
                at += 2;
            }
            Token::Word(word) => match word.to_ascii_lowercase().as_str() {
                "spacing" => {
                    let Some(Token::Word(step)) = tokens.get(at + 1) else {
                        return Err(wrong());
                    };
                    let step = number(step, no)?;
                    if step <= 0.0 {
                        return Err(Declined::refused(
                            no,
                            format!("`spacing {step}`: ticks are a step apart that is more than 0"),
                        ));
                    }
                    axis.spacing = Some(step);
                    at += 1;
                }
                "ticks" => {
                    let Some(Token::List(list)) = tokens.get(at + 1) else {
                        return Err(wrong());
                    };
                    axis.ticks = ticks(list, no)?;
                    at += 1;
                }
                "label-top" => axis.label_top = true,
                "label-right" => axis.label_right = true,
                "grid" => axis.grid = true,
                _ => return Err(wrong()),
            },
            _ => return Err(wrong()),
        }
        at += 1;
    }
    if let (Some((low, high)), Some(step)) = (axis.range, axis.spacing)
        && (high - low) / step > MOST_TICKS
    {
        return Err(Declined::refused(
            no,
            format!("`spacing {step}` sets more ticks than an axis can show"),
        ));
    }
    Ok(axis)
}

/// `[0:"Poor", 50:"Average"]`.
fn ticks(list: &str, no: usize) -> Result<Vec<(f64, String)>, Declined> {
    items(list)
        .into_iter()
        .map(|item| {
            let (value, words) = item.split_once(':').ok_or_else(|| {
                Declined::refused(
                    no,
                    format!("`{item}`: a tick is a value, `:` and its words"),
                )
            })?;
            Ok((number(value, no)?, unquoted(words).to_owned()))
        })
        .collect()
}

fn series(kind: Kind, rest: &str, no: usize) -> Result<Series, Declined> {
    let mut series = Series {
        kind,
        name: None,
        values: Values::Each(Vec::new()),
        color: None,
        v2: false,
        class: None,
        labels: false,
        line: no,
    };
    let mut valued = false;
    for token in tokens(rest, no)? {
        match token {
            Token::Quoted(name) if series.name.is_none() => series.name = Some(name.to_owned()),
            Token::List(list) if !valued => {
                series.values = values(list, no)?;
                valued = true;
            }
            Token::Color(name) => {
                series.color = Some(
                    color::parse(&format!("#{name}"))
                        .map_err(|why| Declined::at(no, why))?
                        .ok_or_else(|| {
                            Declined::at(no, format!("`#{name}`: a series needs a color it shows"))
                        })?,
                );
            }
            Token::Class(class) if series.class.is_none() => {
                series.class = Some(class.to_ascii_lowercase());
            }
            Token::Word(word) if word.eq_ignore_ascii_case("v2") => series.v2 = true,
            Token::Word(word) if word.eq_ignore_ascii_case("labels") => series.labels = true,
            _ => {
                return Err(Declined::at(
                    no,
                    format!("`{rest}` is no {} Omaspec reads yet", kind.keyword()),
                ));
            }
        }
    }
    if !valued {
        return Err(Declined::refused(
            no,
            format!("a {} needs its values in brackets", kind.keyword()),
        ));
    }
    Ok(series)
}

/// `45, 62, 58`, or points `(1, 2), (3, 4)`.
fn values(list: &str, no: usize) -> Result<Values, Declined> {
    let items = items(list);
    if items.first().is_some_and(|item| item.starts_with('(')) {
        let points = items
            .into_iter()
            .map(|item| {
                let pair = item
                    .strip_prefix('(')
                    .and_then(|i| i.strip_suffix(')'))
                    .and_then(|i| i.split_once(','))
                    .ok_or_else(|| {
                        Declined::refused(no, format!("`{item}` is no point `(x, y)`"))
                    })?;
                Ok((number(pair.0, no)?, number(pair.1, no)?))
            })
            .collect::<Result<Vec<_>, Declined>>()?;
        return Ok(Values::Points(points));
    }
    items
        .into_iter()
        .map(|item| number(item, no))
        .collect::<Result<Vec<_>, _>>()
        .map(Values::Each)
}

/// `"text" at (x, y) <<arrow>>`; `x` a category of the chart, or a number.
fn annotation(rest: &str, no: usize) -> Result<Annotation, Declined> {
    let wrong = || {
        Declined::at(
            no,
            format!("`annotation {rest}` is no annotation Omaspec reads yet"),
        )
    };
    let tokens = tokens(rest, no)?;
    let (Some(Token::Quoted(text)), Some(Token::Word(at)), Some(Token::Paren(place))) =
        (tokens.first(), tokens.get(1), tokens.get(2))
    else {
        return Err(wrong());
    };
    if !at.eq_ignore_ascii_case("at") {
        return Err(wrong());
    }
    let arrow = match tokens.get(3) {
        None => false,
        Some(Token::Class(class)) if class.eq_ignore_ascii_case("arrow") => true,
        Some(_) => return Err(wrong()),
    };
    if tokens.len() > 4 {
        return Err(wrong());
    }
    let (x, y) = place
        .rsplit_once(',')
        .ok_or_else(|| Declined::refused(no, format!("`({place})` is no place `(x, y)`")))?;
    Ok(Annotation {
        text: (*text).to_owned(),
        // A category or a number, once the chart is read.
        at: Place::Named(unquoted(x).to_owned()),
        value: number(y, no)?,
        arrow,
        line: no,
    })
}

/// What the lines say together: an annotation stands at a category or a
/// number, points stand on a range, every series gives them alike, and
/// each lies on its axis.
fn check(d: &mut Diagram) -> Result<(), Declined> {
    let categories = if d.horizontal {
        &d.v.categories
    } else {
        &d.h.categories
    };
    for note in &mut d.annotations {
        if let Place::Named(x) = &note.at {
            note.at = match categories.iter().position(|c| c == x) {
                Some(index) => Place::Category(index),
                None => Place::At(number(x, note.line).map_err(|_| {
                    Declined::refused(
                        note.line,
                        format!("`{x}` is no category of the chart, nor a number"),
                    )
                })?),
            };
        }
    }
    let points = d
        .series
        .iter()
        .find(|s| matches!(s.values, Values::Points(_)));
    let Some(first) = points else {
        return Ok(());
    };
    let line = first.line;
    if d.horizontal {
        return Err(Declined::at(
            line,
            "points on a horizontal chart are not supported yet",
        ));
    }
    if let Some(each) = d
        .series
        .iter()
        .find(|s| matches!(s.values, Values::Each(_)))
    {
        return Err(Declined::refused(
            each.line,
            "all series give points `(x, y)`, or all give values, not some of each",
        ));
    }
    if let Some(bar) = d
        .series
        .iter()
        .find(|s| matches!(s.kind, Kind::Bar | Kind::Area))
    {
        return Err(Declined::refused(
            bar.line,
            "points `(x, y)` are for a line or a scatter",
        ));
    }
    let Some((low, high)) = d.h.range.filter(|_| d.h.categories.is_empty()) else {
        return Err(Declined::refused(
            line,
            "points `(x, y)` stand on an h-axis with a range, as `h-axis 0 --> 10`",
        ));
    };
    for series in &d.series {
        if let Values::Points(points) = &series.values
            && let Some((x, _)) = points.iter().find(|(x, _)| *x < low || *x > high)
        {
            return Err(Declined::refused(
                series.line,
                format!("{x} lies outside the h-axis, which runs from {low} to {high}"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::body;

    fn read(text: &str) -> Result<Diagram, Declined> {
        let source = format!("@startchart\n{text}\n@endchart\n");
        let (_, lines) = body(&source).unwrap();
        parse(&lines)
    }

    #[test]
    fn a_chart_reads_its_axes_and_series() {
        let d = read(
            "title Quarters\nh-axis \"Quarter\" [Q1, Q2, Q3]\nv-axis \"Revenue\" 0 --> 100 spacing 25 label-top grid\nv2-axis 0-->50\nstackMode stacked\nbar \"Sales\" [45, 62, 58] #3498db labels\nline <<trend>> \"Share\" [15, 20, 18] v2\nlegend bottom\nannotation \"Peak\" at (Q2, 62) <<arrow>>",
        )
        .unwrap();
        assert_eq!(d.h.title.as_deref(), Some("Quarter"));
        assert_eq!(d.h.categories, ["Q1", "Q2", "Q3"]);
        assert_eq!(
            (d.v.range, d.v.spacing, d.v.label_top, d.v.grid),
            (Some((0.0, 100.0)), Some(25.0), true, true)
        );
        assert_eq!(d.v2.as_ref().unwrap().range, Some((0.0, 50.0)));
        assert!(d.stacked && d.legend == Some(Side::Bottom));
        let (sales, share) = (&d.series[0], &d.series[1]);
        assert_eq!(sales.values, Values::Each(vec![45.0, 62.0, 58.0]));
        assert!(sales.labels && sales.color.is_some() && !sales.v2);
        assert_eq!(
            (share.kind, share.class.as_deref(), share.v2),
            (Kind::Line, Some("trend"), true)
        );
        assert_eq!(d.annotations[0].at, Place::Category(1));
        assert!(d.annotations[0].arrow && d.annotations[0].value == 62.0);
        assert!(d.frame.title.is_some());
    }

    /// plantuml/plantuml#3030 and #3031: a number no chart draws and an
    /// axis that runs nowhere or backwards hang or fail the Java PlantUML;
    /// here each is a mistake on its line.
    #[test]
    fn what_hangs_the_java_plantuml_is_a_mistake_on_its_line() {
        for (text, line) in [
            ("h-axis [A, B]\nbar [Infinity]", 2),
            ("bar [NaN, 1]", 1),
            ("bar [1e999]", 1),
            ("v-axis 5 --> 5\nline [5, 6]", 1),
            ("v-axis 10 --> 0", 1),
            ("v-axis 0 --> 10 spacing 0", 1),
            ("v-axis 0 --> 1000 spacing 0.001", 1),
            ("h-axis [A]\nannotation \"x\" at (A, Infinity)", 2),
        ] {
            let why = read(text).unwrap_err();
            assert!(why.refused, "{text}: {why:?}");
            assert_eq!(why.line, Some(line), "{text}: {why:?}");
        }
    }

    #[test]
    fn points_stand_on_a_range() {
        let d = read(
            "h-axis \"t\" -10 --> 10 spacing 2 label-right\nline \"Path\" [(-10,0), (2,10)]\nscatter [(1, 12)] #e74c3c",
        )
        .unwrap();
        assert_eq!(
            d.series[0].values,
            Values::Points(vec![(-10.0, 0.0), (2.0, 10.0)])
        );
        assert!(d.h.label_right);
        for text in [
            "h-axis [A, B]\nline [(1, 2)]",
            "h-axis 0 --> 10\nline [(1, 2)]\nline [3]",
            "h-axis 0 --> 10\nbar [(1, 2)]",
            "h-axis 0 --> 10\nscatter [(11, 2)]",
        ] {
            assert!(read(text).unwrap_err().refused, "{text}");
        }
    }

    /// An annotation finds its category however the lines are ordered; a
    /// legend at a side stays the chart's though a legend block follows.
    #[test]
    fn the_order_of_the_lines_changes_nothing() {
        let d = read("annotation \"Peak\" at (Q2, 62)\nh-axis [Q1, Q2]\nbar [1, 62]").unwrap();
        assert_eq!(d.annotations[0].at, Place::Category(1));
        let d =
            read("annotation \"x\" at (B, 1)\nv-axis [A, B]\norientation horizontal\nbar [1, 2]")
                .unwrap();
        assert_eq!(d.annotations[0].at, Place::Category(1));
        let why = read("h-axis [A]\nannotation \"x\" at (Z, 1)").unwrap_err();
        assert!(why.refused && why.reason.contains("no category"), "{why:?}");
        let d = read("legend right\nbar \"A\" [1]\nlegend\n  note\nendlegend").unwrap();
        assert_eq!(d.legend, Some(Side::Right));
        assert_eq!(d.series.len(), 1);
        assert!(d.frame.legend.is_some());
    }

    #[test]
    fn a_line_it_does_not_read_is_left_to_the_java_plantuml() {
        let why = read("bar [1, 2]\nbubble [3]").unwrap_err();
        assert!(!why.refused && why.line == Some(2), "{why:?}");
        // A legend block is the frame's.
        let d = read("bar [1]\nlegend right\n  words\nendlegend").unwrap();
        assert!(d.legend.is_none() && d.frame.legend.is_some());
    }
}
