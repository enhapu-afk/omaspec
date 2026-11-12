//! Reading a network diagram: statements apart by `;` or a line break,
//! blocks between braces, `//` comments, PlantUML's frame lines (title,
//! caption, legend, skinparams, styles) between them.

use super::{Diagram, Group, Network, Server};
use crate::Declined;
use crate::color;
use crate::common::{Frame, keyword, squeezed_eq};
use crate::source::Line;
use crate::structure::Shape;
use crate::style::{Element, LEGEND, Prop};

/// What `<style>` reaches in a network diagram, as PlantUML names it.
pub(super) const STYLE: &[Element] = &[
    Element::new(
        &["document", "nwdiagDiagram", "network"],
        "Network",
        &[
            Prop::BackGroundColor,
            Prop::LineColor,
            Prop::LineThickness,
            Prop::FontColor,
            Prop::FontSize,
            Prop::FontStyle,
        ],
    ),
    Element::new(
        &["document", "nwdiagDiagram", "server"],
        "Server",
        &[
            Prop::BackGroundColor,
            Prop::LineColor,
            Prop::LineThickness,
            Prop::FontColor,
            Prop::FontSize,
            Prop::FontStyle,
            Prop::RoundCorner,
        ],
    ),
    Element::keys(
        &["document", "nwdiagDiagram", "arrow"],
        &[
            (Prop::LineColor, "ArrowColor"),
            (Prop::LineThickness, "ArrowThickness"),
            (Prop::FontColor, "ArrowFontColor"),
            (Prop::FontSize, "ArrowFontSize"),
        ],
    ),
    Element::new(
        &["document", "nwdiagDiagram", "group"],
        "Group",
        &[
            Prop::BackGroundColor,
            Prop::LineColor,
            Prop::LineThickness,
            Prop::FontColor,
            Prop::FontSize,
            Prop::FontStyle,
        ],
    ),
    Element::new(&["document", "nwdiagDiagram", "legend"], "Legend", LEGEND),
];

/// Whether `lines` are a network diagram in an `@startuml`: a line opens
/// `nwdiag {`, which no other type has.
pub fn is_network(lines: &[Line]) -> bool {
    lines.iter().any(|line| squeezed_eq(line.text, "nwdiag{"))
}

/// Where a statement stands.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Block {
    /// `nwdiag { … }` around everything.
    Wrapper,
    Network(usize),
    Group(usize),
}

/// A piece of a line: a statement, one that opens a block, or the end of
/// one.
#[derive(Debug, PartialEq)]
enum Piece {
    Says(String),
    Opens(String),
    Closes,
}

pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut d = Diagram {
        networks: Vec::new(),
        servers: Vec::new(),
        groups: Vec::new(),
        peers: Vec::new(),
        frame: Frame::styled(STYLE),
    };
    let mut open: Vec<(Block, usize)> = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        // The frame's lines stand outside the networks and the groups.
        if open.iter().all(|(block, _)| *block == Block::Wrapper)
            && let Some(next) = d.frame.line(lines, at)?
        {
            at = next;
            continue;
        }
        let Line { no, text, .. } = lines[at];
        for piece in pieces(text, no)? {
            match piece {
                Piece::Closes => {
                    if open.pop().is_none() {
                        return Err(Declined::at(no, "there is no `{` for this `}`"));
                    }
                }
                Piece::Opens(head) => {
                    let block = opens(&mut d, &open, &head, no)?;
                    open.push((block, no));
                }
                Piece::Says(statement) => {
                    let here = open.last().map(|(block, _)| *block);
                    says(&mut d, here, &statement, no)?;
                }
            }
        }
        at += 1;
    }
    if let Some((_, line)) = open.last() {
        return Err(Declined::at(*line, "this block has no `}`"));
    }
    if d.networks.is_empty() && d.servers.is_empty() {
        return Err(Declined {
            line: None,
            reason: "a network diagram needs a network".into(),
            refused: true,
        });
    }
    Ok(d)
}

/// The block `head {` opens.
fn opens(
    d: &mut Diagram,
    open: &[(Block, usize)],
    head: &str,
    no: usize,
) -> Result<Block, Declined> {
    let top = open.iter().all(|(block, _)| *block == Block::Wrapper);
    if squeezed_eq(head, "nwdiag") && open.is_empty() {
        return Ok(Block::Wrapper);
    }
    if let Some(name) =
        keyword(head, "network").or_else(|| squeezed_eq(head, "network").then_some(""))
    {
        if !top {
            return Err(Declined::at(
                no,
                "a network inside another block is not supported",
            ));
        }
        d.networks.push(Network {
            name: unquote(name).to_owned(),
            address: None,
            description: None,
            color: None,
            full: false,
            members: Vec::new(),
            line: no,
        });
        return Ok(Block::Network(d.networks.len() - 1));
    }
    if let Some(_name) = keyword(head, "group").or_else(|| squeezed_eq(head, "group").then_some(""))
    {
        if !top {
            return Err(Declined::at(
                no,
                "a group inside another block is not supported",
            ));
        }
        d.groups.push(Group {
            description: None,
            color: None,
            members: Vec::new(),
            line: no,
        });
        return Ok(Block::Group(d.groups.len() - 1));
    }
    Err(Declined::at(
        no,
        format!("`{} {{` is not supported in a network diagram", head.trim()),
    ))
}

/// One statement in `here`: a setting, a server, or peer lines.
fn says(d: &mut Diagram, here: Option<Block>, text: &str, no: usize) -> Result<(), Declined> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    // `a -- b -- c`: lines between servers.
    if !text.contains('[') && text.contains("--") {
        let names: Vec<&str> = text.split("--").map(str::trim).collect();
        if names.iter().any(|n| n.is_empty() || !is_name(unquote(n))) {
            return Err(Declined::at(no, "a peer line joins two servers: `a -- b`"));
        }
        for pair in names.windows(2) {
            let a = server(d, unquote(pair[0]), no);
            let b = server(d, unquote(pair[1]), no);
            d.peers.push((a, b, no));
        }
        return Ok(());
    }
    if let Some((key, value)) = setting(text) {
        return set(d, here, key, value, no);
    }
    // A server, with its attributes.
    let (name, attrs) = match text.find('[') {
        Some(open) => {
            let close = text
                .rfind(']')
                .filter(|&close| close > open)
                .ok_or_else(|| Declined::at(no, "an attribute list needs its `]`"))?;
            (text[..open].trim(), attributes(&text[open + 1..close], no)?)
        }
        None => (text, Vec::new()),
    };
    let name = unquote(name);
    if !is_name(name) {
        return Err(Declined::at(
            no,
            format!("`{}` is not supported in a network diagram", text),
        ));
    }
    let index = server(d, name, no);
    let mut address = None;
    for (key, value) in attrs {
        match key.as_str() {
            "address" => address = Some(value),
            "description" => d.servers[index].description = Some(value),
            "shape" => d.servers[index].shape = shape(&value, no)?,
            "color" => d.servers[index].color = Some(solid(&value, no)?),
            other => {
                return Err(Declined::at(
                    no,
                    format!("a server's `{other}` is not supported yet"),
                ));
            }
        }
    }
    match here {
        Some(Block::Network(n)) => {
            let members = &mut d.networks[n].members;
            match members.iter_mut().find(|(s, _)| *s == index) {
                Some(member) => member.1 = address.or(member.1.take()),
                None => members.push((index, address)),
            }
        }
        Some(Block::Group(g)) => {
            if !d.groups[g].members.contains(&index) {
                d.groups[g].members.push(index);
            }
            if address.is_some() {
                d.servers[index].address = address;
            }
        }
        _ => {
            if address.is_some() {
                d.servers[index].address = address;
            }
        }
    }
    Ok(())
}

/// A setting of the block it stands in.
fn set(
    d: &mut Diagram,
    here: Option<Block>,
    key: &str,
    value: String,
    no: usize,
) -> Result<(), Declined> {
    match (here, key) {
        (Some(Block::Network(n)), "address") => d.networks[n].address = Some(value),
        (Some(Block::Network(n)), "description") => d.networks[n].description = Some(value),
        (Some(Block::Network(n)), "color") => d.networks[n].color = Some(solid(&value, no)?),
        (Some(Block::Network(n)), "width") => {
            if !value.eq_ignore_ascii_case("full") {
                return Err(Declined::at(no, "a network's width is `full`"));
            }
            d.networks[n].full = true;
        }
        (Some(Block::Group(g)), "color") => d.groups[g].color = Some(solid(&value, no)?),
        (Some(Block::Group(g)), "description") => d.groups[g].description = Some(value),
        (_, key) => {
            return Err(Declined::at(
                no,
                format!("`{key} = …` is not supported here yet"),
            ));
        }
    }
    Ok(())
}

/// The server called `name`, made when it is new.
fn server(d: &mut Diagram, name: &str, no: usize) -> usize {
    if let Some(found) = d.servers.iter().position(|s| s.name == name) {
        return found;
    }
    d.servers.push(Server {
        name: name.to_owned(),
        description: None,
        shape: Shape::Rectangle,
        color: None,
        address: None,
        line: no,
    });
    d.servers.len() - 1
}

/// `key = value`, the value unquoted.
fn setting(text: &str) -> Option<(&str, String)> {
    let (key, value) = text.split_once('=')?;
    let key = key.trim();
    (!key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then(|| (key, unquote(value.trim()).to_owned()))
}

/// `address = "a", shape = database` as pairs, keys in lower case.
fn attributes(text: &str, no: usize) -> Result<Vec<(String, String)>, Declined> {
    split_outside_quotes(text, ',')
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .map(|part| {
            setting(part.trim())
                .map(|(key, value)| (key.to_ascii_lowercase(), value))
                .ok_or_else(|| Declined::at(no, format!("`{}` is no attribute", part.trim())))
        })
        .collect()
}

fn shape(value: &str, no: usize) -> Result<Shape, Declined> {
    Ok(match value.to_ascii_lowercase().as_str() {
        "box" | "rectangle" => Shape::Rectangle,
        "database" => Shape::Database,
        "cloud" => Shape::Cloud,
        "node" => Shape::Node,
        "actor" => Shape::Actor,
        "person" => Shape::Person,
        "queue" => Shape::Queue,
        "storage" => Shape::Storage,
        "folder" => Shape::Folder,
        "frame" => Shape::Frame,
        "file" => Shape::File,
        "artifact" => Shape::Artifact,
        "card" => Shape::Card,
        "component" => Shape::Component,
        "stack" => Shape::Stack,
        "hexagon" => Shape::Hexagon,
        "agent" => Shape::Agent,
        "usecase" => Shape::UseCase,
        "package" => Shape::Package,
        other => {
            return Err(Declined::at(
                no,
                format!("the shape `{other}` is not drawn in a network diagram yet"),
            ));
        }
    })
}

fn solid(value: &str, no: usize) -> Result<color::Color, Declined> {
    color::parse(value)
        .map_err(|e| Declined::at(no, e))?
        .ok_or_else(|| Declined::at(no, "a color here cannot be transparent"))
}

fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
}

fn unquote(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
}

/// The pieces of a line, a `//` comment taken off: statements apart by
/// `;`, a block opened by `{` and closed by `}`, none of them inside
/// quotes or an attribute list.
fn pieces(text: &str, no: usize) -> Result<Vec<Piece>, Declined> {
    let mut out = Vec::new();
    let mut current = String::new();
    let (mut quoted, mut square) = (false, 0usize);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            '/' if !quoted && chars.peek() == Some(&'/') => break,
            '[' if !quoted => {
                square += 1;
                current.push(c);
            }
            ']' if !quoted => {
                square = square.saturating_sub(1);
                current.push(c);
            }
            ';' if !quoted && square == 0 => {
                out.push(Piece::Says(std::mem::take(&mut current)));
            }
            '{' if !quoted && square == 0 => {
                out.push(Piece::Opens(std::mem::take(&mut current).trim().to_owned()));
            }
            '}' if !quoted && square == 0 => {
                out.push(Piece::Says(std::mem::take(&mut current)));
                out.push(Piece::Closes);
            }
            _ => current.push(c),
        }
    }
    if quoted {
        return Err(Declined::at(no, "a quote is not closed"));
    }
    out.push(Piece::Says(current));
    Ok(out
        .into_iter()
        .filter(|piece| !matches!(piece, Piece::Says(text) if text.trim().is_empty()))
        .collect())
}

fn split_outside_quotes(text: &str, at: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == at && !quoted {
            out.push(&text[start..i]);
            start = i + c.len_utf8();
        }
    }
    out.push(&text[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<Line<'_>> {
        text.lines()
            .enumerate()
            .map(|(no, text)| Line {
                no,
                text: text.trim(),
                indent: 0,
                blank_before: 0,
            })
            .collect()
    }

    const OFFICE: &str = "nwdiag {\ninternet [shape = cloud];\ninternet -- router;\nnetwork dmz {\n  address = \"210.x.x.x/24\"\n  router [address = \"210.x.x.1\"];\n  web01 [address = \"210.x.x.2\"]; web02 [address = \"210.x.x.3\", description = \"web server\"];\n}\nnetwork internal {\n  address = \"172.x.x.x/24\"; // the office\n  color = \"#FFFFCC\"\n  web01 [address = \"172.x.x.1\"];\n  web02;\n  db01 [shape = database];\n}\ngroup {\n  color = \"#FFAAAA\";\n  db01;\n}\n}";

    #[test]
    fn networks_servers_groups_and_peers_are_read() {
        let d = parse(&lines(OFFICE)).unwrap();
        let names: Vec<&str> = d.servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["internet", "router", "web01", "web02", "db01"]);
        assert_eq!(d.servers[0].shape, Shape::Cloud);
        assert_eq!(d.servers[3].description.as_deref(), Some("web server"));
        assert_eq!(d.peers, [(0, 1, 2)]);
        assert_eq!(d.networks.len(), 2);
        assert_eq!(d.networks[0].address.as_deref(), Some("210.x.x.x/24"));
        assert_eq!(
            d.networks[0].members,
            [
                (1, Some("210.x.x.1".into())),
                (2, Some("210.x.x.2".into())),
                (3, Some("210.x.x.3".into()))
            ]
        );
        assert_eq!(d.networks[1].color, Some(color::Color::hex(0xFFFFCC)));
        assert_eq!(d.networks[1].members[1], (3, None));
        assert_eq!(d.groups[0].members, [4]);
        assert_eq!(d.groups[0].color, Some(color::Color::hex(0xFFAAAA)));
    }

    #[test]
    fn what_it_does_not_read_names_its_line() {
        for (text, line, part) in [
            ("network a {\n  x [shape = robot];\n}", 1, "robot"),
            ("network a {\n  x [weight = 2];\n}", 1, "weight"),
            ("network a {\n  x;\n", 0, "no `}`"),
            (
                "default_fontsize = 18;\nnetwork a {\n}",
                0,
                "default_fontsize",
            ),
            ("network a {\n  network b {\n  }\n}", 1, "inside"),
            ("network a {\n  x\n}\n}", 3, "no `{`"),
        ] {
            let err = parse(&lines(text)).unwrap_err();
            assert_eq!(err.line, Some(line), "{text}");
            assert!(err.reason.contains(part), "{text}: {}", err.reason);
        }
    }
}
