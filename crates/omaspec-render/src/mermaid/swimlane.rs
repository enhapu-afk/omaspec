//! Mermaid's swimlanes (`swimlane-beta`, Mermaid 11.16; REQ-089): a
//! flowchart whose subgraphs at the top are its lanes. A node belongs to
//! the first lane that writes it; the lines may run from lane to lane.
//!
//! The diagram is read as the flowchart it is, without its lanes, the
//! lines where they were written left empty so every line keeps its
//! place; the flowchart's own drawer lays it out along its flow and puts
//! each node in the band of its lane ([`super::layered::lanes`]). Where
//! that drawer declines, the text with its lanes as subgraphs is a
//! flowchart mmdr draws ([`as_flowchart`]).

use std::collections::HashMap;

use mermaid_rs_renderer::Theme;
use omaspec_uml::map::Map;

use super::head;

/// Draws the swimlane diagram `body` in `theme`, its lines by hand as
/// `fenced` writes them, or leaves it to the flowchart it is
/// ([`as_flowchart`]).
pub(super) fn draw(theme: &Theme, body: &str, fenced: &str) -> Option<(String, Map)> {
    let (text, lanes) = read(body)?;
    let parsed = super::layered::read(&text).ok()?;
    let curve = super::flowchart_curve(omaspec_core::settings::merged(body).as_ref());
    let hands = omaspec_uml::hand::numbered(fenced);
    super::layered::try_draw_lanes(&parsed.graph, theme, &text, &hands, curve, &lanes)
}

/// The lanes of a swimlane diagram.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Lanes {
    pub lanes: Vec<Lane>,
    /// The lane each node is written in, by its id: the first that
    /// writes it.
    pub member: HashMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Lane {
    pub title: String,
    /// The zero-based line of the text that opens it.
    pub line: usize,
}

/// The direction a swimlane's head names, as a flowchart's: `TB` unless it
/// names another. A comment after it and a `;` are no part of it.
fn direction(header: &str) -> &'static str {
    let said = header
        .split("%%")
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .trim_end_matches(';');
    match said.to_ascii_uppercase().as_str() {
        "LR" => "LR",
        "RL" => "RL",
        "BT" => "BT",
        _ => "TB",
    }
}

/// The swimlane diagram `body` with its head written as a flowchart's,
/// its lanes kept as subgraphs: what mmdr draws where the lanes' drawer
/// declines.
pub(super) fn as_flowchart(body: &str) -> Option<String> {
    let (at, header) = head::header_at(body)?;
    if !head::names(&header.to_ascii_lowercase(), "swimlane") {
        return None;
    }
    let lines: Vec<String> = body
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            if index == at {
                format!("flowchart {}", direction(header))
            } else {
                line.to_owned()
            }
        })
        .collect();
    Some(lines.join("\n"))
}

/// The swimlane diagram `body` as a flowchart without its lanes, each
/// line that opened or closed one, or turned its direction, left empty;
/// and its lanes. `None` for another diagram, one without a lane, one
/// whose lane is not closed (mmdr says so on its line) and one with a
/// line to a lane or to a subgraph in one: there the lanes stay boxes, as
/// the flowchart [`as_flowchart`] writes.
pub(super) fn read(body: &str) -> Option<(String, Lanes)> {
    let (at, header) = head::header_at(body)?;
    if !head::names(&header.to_ascii_lowercase(), "swimlane") {
        return None;
    }
    let mut lanes = Vec::new();
    let mut member = HashMap::new();
    let mut boxes: Vec<String> = Vec::new();
    let mut written: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut out: Vec<String> = Vec::new();
    for (index, line) in body.split('\n').enumerate() {
        if index == at {
            out.push(format!("flowchart {}", direction(header)));
            continue;
        }
        let text = line.split("%%").next().unwrap_or("").trim();
        if let Some(rest) = word_after(text, "subgraph") {
            if depth == 0 {
                lanes.push(Lane {
                    title: title_of(rest),
                    line: index,
                });
            }
            boxes.extend(subgraph_id(rest));
            // A subgraph inside a lane is no box of its own here: what it
            // writes is the lane's.
            depth += 1;
            out.push(String::new());
            continue;
        }
        if depth > 0 && matches!(text, "end" | "end;") {
            depth -= 1;
            out.push(String::new());
            continue;
        }
        // Without its subgraph a lane's direction would turn the whole
        // diagram; the lanes have the head's.
        if depth > 0 && word_after(text, "direction").is_some() {
            out.push(String::new());
            continue;
        }
        let ids = written_ids(text);
        if depth > 0 {
            let lane = lanes.len() - 1;
            for id in &ids {
                member.entry(id.clone()).or_insert(lane);
            }
        }
        written.extend(ids);
        out.push(line.to_owned());
    }
    if lanes.is_empty() || depth > 0 || written.iter().any(|id| boxes.contains(id)) {
        return None;
    }
    Some((out.join("\n"), Lanes { lanes, member }))
}

/// The id of a subgraph from what follows `subgraph`: the word before its
/// name in brackets, or its one word; a name in quotes has none.
fn subgraph_id(rest: &str) -> Option<String> {
    let id = match rest.find('[') {
        Some(open) => rest[..open].trim(),
        None => rest.trim(),
    };
    (!id.is_empty() && !id.starts_with('"') && !id.contains(char::is_whitespace))
        .then(|| id.to_owned())
}

/// What follows `word` at the start of `text`, as a word of its own.
fn word_after<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word)?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

/// A lane's name: `id [Its name]`, `id["Its name"]`, `"Its name"`, or the
/// words after `subgraph`.
fn title_of(rest: &str) -> String {
    let unquote = |text: &str| {
        let text = text.trim();
        text.strip_prefix('"')
            .and_then(|t| t.strip_suffix('"'))
            .unwrap_or(text)
            .to_owned()
    };
    match rest.find('[') {
        Some(open) if rest.trim_end().ends_with(']') => {
            let inner = &rest[open + 1..rest.trim_end().len() - 1];
            unquote(inner)
        }
        _ => unquote(rest),
    }
}

/// The ids a flowchart's line writes: each box it declares and each end
/// of its lines. A line of styling, a click or a direction writes none.
fn written_ids(text: &str) -> Vec<String> {
    const NONE: [&str; 8] = [
        "style",
        "classDef",
        "class",
        "click",
        "linkStyle",
        "direction",
        "accTitle",
        "accDescr",
    ];
    if text.is_empty()
        || text.starts_with("%%")
        || NONE
            .iter()
            .any(|word| word_after(text, word).is_some() || text.starts_with(&format!("{word}:")))
    {
        return Vec::new();
    }
    let masked: Vec<char> = masked(text).chars().collect();
    // Split where a line runs (`-->`, `-.->`, `==>`, `---`, `~~~`, a head
    // of `x` or `o`), at `&` and at `;`; a hyphen alone is an id's. What
    // stands between the halves of a line (`A -- yes --> B`, `A -. maybe
    // .-> B`) is its label.
    let link = |c: char| matches!(c, '-' | '=' | '.' | '~' | '<' | '>');
    let mut pieces = vec![String::new()];
    let mut label = false;
    let mut i = 0;
    while i < masked.len() {
        let c = masked[i];
        if c == '&' || c == ';' {
            pieces.push(String::new());
            label = false;
            i += 1;
        } else if link(c) {
            let start = i;
            while i < masked.len() && link(masked[i]) {
                i += 1;
            }
            if i - start >= 2 || masked[start] == '>' {
                let run: String = masked[start..i].iter().collect();
                let head = masked.get(i).is_some_and(|c| matches!(c, 'x' | 'o'))
                    && masked.get(i + 1).is_none_or(|c| c.is_whitespace());
                if head {
                    i += 1;
                }
                // `--`, `==` and `-.` open a label the next half closes.
                label = !label
                    && !head
                    && matches!(run.as_str(), "--" | "==" | "-." | "<--" | "<==" | "<-.");
                pieces.push(String::new());
            } else if let Some(piece) = pieces.last_mut()
                && !label
            {
                piece.extend(&masked[start..i]);
            }
        } else if label {
            i += 1;
        } else {
            if let Some(piece) = pieces.last_mut() {
                piece.push(c);
            }
            i += 1;
        }
    }
    let mut ids: Vec<String> = Vec::new();
    for piece in &pieces {
        let piece = piece.trim();
        if !piece.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let end = piece
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(piece.len());
        let id = piece[..end].trim_end_matches('-');
        if !id.is_empty() && !ids.iter().any(|known| known == id) {
            ids.push(id.to_owned());
        }
    }
    ids
}

/// `text` with what stands in brackets, between `|` or in quotes blanked:
/// a box's words and a line's label are no ids. The `>` that opens an
/// asymmetric box (`id>Ship it]`) right after its id is a bracket.
fn masked(text: &str) -> String {
    let mut depth = 0usize;
    let (mut quoted, mut piped) = (false, false);
    let mut before = ' ';
    text.chars()
        .map(|c| {
            let after_id = before.is_alphanumeric() || before == '_';
            before = c;
            match c {
                '"' => {
                    quoted = !quoted;
                    ' '
                }
                '|' if !quoted => {
                    piped = !piped;
                    ' '
                }
                '>' if !quoted && !piped && depth == 0 && after_id => {
                    depth += 1;
                    '['
                }
                '[' | '(' | '{' if !quoted && !piped => {
                    depth += 1;
                    c
                }
                ']' | ')' | '}' if !quoted && !piped && depth > 0 => {
                    depth -= 1;
                    c
                }
                _ if quoted || piped || depth > 0 => ' ',
                c => c,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example of Mermaid's documentation of swimlanes.
    pub(crate) const DOCS: &str = "swimlane-beta LR
  subgraph Customer
    request[Request service]
    receive[Receive update]
  end

  subgraph Support
    triage[Triage request]
    answer[Send answer]
  end

  subgraph Engineering
    investigate[Investigate issue]
    fix[Prepare fix]
  end

  request --> triage
  triage -->|Known issue| answer
  triage -->|Needs code change| investigate
  investigate --> fix --> answer
  answer --> receive
";

    #[test]
    fn the_lanes_are_read_and_taken_out_of_the_flowchart() {
        let (text, lanes) = read(DOCS).unwrap();
        let titles: Vec<&str> = lanes.lanes.iter().map(|l| l.title.as_str()).collect();
        assert_eq!(titles, ["Customer", "Support", "Engineering"]);
        assert_eq!(lanes.lanes[1].line, 6);
        let lane = |id: &str| lanes.member.get(id).copied();
        assert_eq!(
            [
                lane("request"),
                lane("triage"),
                lane("fix"),
                lane("nowhere")
            ],
            [Some(0), Some(1), Some(2), None]
        );
        // The same lines, the head a flowchart's, no subgraph left.
        assert_eq!(text.lines().count(), DOCS.lines().count());
        assert!(text.starts_with("flowchart LR\n"));
        assert!(!text.contains("subgraph") && !text.contains("\n  end"));
        assert!(text.contains("request --> triage"));
    }

    #[test]
    fn a_lane_s_name_and_what_it_writes_are_read_as_mermaid_writes_them() {
        assert_eq!(title_of("sales [Sales team]"), "Sales team");
        assert_eq!(title_of("ops[\"Ops, nights\"]"), "Ops, nights");
        assert_eq!(title_of("\"Back office\""), "Back office");
        assert_eq!(title_of("Back office"), "Back office");
        assert_eq!(written_ids("A(A)"), ["A"]);
        assert_eq!(written_ids("C & F & G -.- I"), ["C", "F", "G", "I"]);
        assert_eq!(
            written_ids("triage -->|Needs code change| investigate"),
            ["triage", "investigate"]
        );
        assert_eq!(
            written_ids("back-end[The back end] --> db[(Data)]"),
            ["back-end", "db"]
        );
        assert_eq!(written_ids("quote -- repair --> agree"), ["quote", "agree"]);
        assert_eq!(written_ids("a -. maybe .-> b"), ["a", "b"]);
        assert_eq!(written_ids("a == sure ==> b --- c"), ["a", "b", "c"]);
        assert_eq!(written_ids("ship>Ship it] --x done"), ["ship", "done"]);
        assert_eq!(written_ids("a --o b"), ["a", "b"]);
        assert!(written_ids("classDef attention fill:#fff2cc").is_empty());
        assert!(written_ids("class review attention").is_empty());
        let (_, lanes) = read(
            "swimlane-beta\n  subgraph A\n    x[X]\n  end\n  subgraph B\n    x\n    y\n  end\n",
        )
        .unwrap();
        assert_eq!(lanes.member["x"], 0, "the first lane that writes it");
        assert_eq!(lanes.member["y"], 1);
    }

    /// The example of the documentation is drawn by Omaspec, without a
    /// word of its head as a box (REQ-089).
    #[test]
    fn the_documentation_s_example_is_drawn_by_omaspec() {
        let drawn = super::super::draw(DOCS).unwrap();
        assert!(drawn.declined.is_none(), "{:?}", drawn.declined);
        for word in [">swimlane<", ">beta<", ">LR<"] {
            assert!(!drawn.svg.contains(word), "{word}");
        }
        for name in [
            "Customer",
            "Support",
            "Engineering",
            "Triage request",
            "Known issue",
        ] {
            assert!(drawn.svg.contains(name), "{name}");
        }
    }

    /// What the lanes cannot hold is left to the flowchart with its lanes
    /// as boxes: a lane not closed (mmdr says so), a line to a lane or to
    /// a subgraph in one.
    #[test]
    fn what_bands_cannot_hold_is_left_to_the_flowchart() {
        let open =
            "swimlane-beta LR\n  subgraph A\n    x[X]\n  subgraph B\n    y\n  end\n  x --> y\n";
        assert!(read(open).is_none());
        let to_lane = DOCS.replace("answer --> receive", "answer --> Customer");
        assert!(read(&to_lane).is_none());
        let inner = "swimlane-beta\n  subgraph A\n    subgraph inner [Inner]\n      x\n    end\n  end\n  y --> inner\n";
        assert!(read(inner).is_none());
        assert_eq!(subgraph_id("sales [Sales team]").as_deref(), Some("sales"));
        assert_eq!(subgraph_id("Support").as_deref(), Some("Support"));
        assert_eq!(subgraph_id("\"Back office\""), None);
        // A lane's direction is no direction of the whole.
        let turned = DOCS.replace(
            "  subgraph Support\n",
            "  subgraph Support\n    direction TB\n",
        );
        let (text, _) = read(&turned).unwrap();
        assert!(!text.contains("direction"));
        assert_eq!(text.lines().count(), turned.lines().count());
    }

    /// Where the bands decline, the diagram is the flowchart it is: drawn
    /// by Omaspec with its lanes as boxes, or refused on the line mmdr
    /// refuses; never with the words of its head as boxes.
    #[test]
    fn the_flowchart_it_is_is_drawn_where_the_bands_decline() {
        let to_lane = DOCS.replace("answer --> receive", "answer --> Customer");
        let drawn = super::super::draw(&to_lane).unwrap();
        assert!(drawn.declined.is_none(), "{:?}", drawn.declined);
        assert!(!drawn.svg.contains(">swimlane<") && !drawn.svg.contains(">beta<"));
        assert!(drawn.svg.contains("Customer") && drawn.svg.contains("Send answer"));
        let open = "swimlane-beta LR\n  subgraph A\n    x[X]\n  x --> y\n";
        match super::super::draw(open) {
            Err(crate::RenderError::Diagram { line, .. }) => {
                assert_eq!(line, Some(1), "the lane's line")
            }
            Err(other) => panic!("{other:?}"),
            Ok(_) => panic!("a lane not closed is drawn"),
        }
    }

    #[test]
    fn the_head_names_the_flowchart_s_direction() {
        assert_eq!(direction("swimlane-beta"), "TB");
        assert_eq!(direction("swimlane-beta LR"), "LR");
        assert_eq!(direction("swimlane-beta TD"), "TB");
        assert_eq!(direction("swimlane-beta BT"), "BT");
        assert_eq!(direction("swimlane-beta LR %% the repair flow"), "LR");
        assert_eq!(direction("swimlane-beta RL;"), "RL");
        assert_eq!(direction("swimlane-beta %% LR later"), "TB");
        assert!(
            as_flowchart(DOCS)
                .unwrap()
                .starts_with("flowchart LR\n  subgraph Customer")
        );
        assert!(read("flowchart LR\n  A --> B\n").is_none());
        assert!(read("swimlane-beta LR\n  A --> B\n").is_none(), "no lane");
    }
}
