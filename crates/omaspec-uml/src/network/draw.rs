//! Laying out and painting a network diagram. The networks are bars, one
//! under another; between two bars runs a lane, and each server stands in
//! one: between the first and the last network it is on, below the one
//! network it is on, or above it when a server on no network hangs above
//! it by a peer line (`internet -- router`). Servers stand in columns; a
//! column holds servers of different lanes as long as no line of one runs
//! through another.

use super::{Diagram, Server};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::{self, Color, Paint};
use crate::common::{Skinparam, normalize_key};
use crate::graph::Rect;
use crate::structure::{Shape, paint_outline, words_area};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const MARGIN: f32 = 10.0;
/// A network's bar.
const BAR: f32 = 8.0;
/// Between a bar and a server: room for the address on the line.
const LANE_GAP: f32 = 30.0;
/// Between two columns.
const COLUMN_GAP: f32 = 36.0;
/// Around the words in a server.
const PAD: (f32, f32) = (14.0, 9.0);
/// Between a server on no network and the one it hangs above.
const HANG: f32 = 34.0;
/// Around a group's servers.
const GROUP_PAD: f32 = 10.0;

pub fn draw(d: &Diagram) -> Result<String, Declined> {
    let look = Look::read(&d.frame.skinparams)?;
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let annotations = Annotations::new(
        &d.frame,
        [
            look.title,
            Font::new(10.0, look.muted),
            Font::new(10.0, look.muted),
            look.font,
        ],
        family,
    )?;
    let words = |text: &str, font: Font, line: usize| {
        text::block(text, font, family).map_err(|e| Declined::at(line, e))
    };

    // Each server's words and its box.
    let mut labels: Vec<Block> = Vec::with_capacity(d.servers.len());
    let mut sizes: Vec<(f32, f32)> = Vec::with_capacity(d.servers.len());
    for server in &d.servers {
        let shown = server.description.as_deref().unwrap_or(&server.name);
        let label = words(shown, look.server_font, server.line)?;
        sizes.push(size_of(server.shape, &label));
        labels.push(label);
    }

    // The lanes: lane `l` lies above bar `l`, the last below the last bar.
    let count = d.networks.len();
    // Room for the addresses between a bar and a server; above the first
    // lane and below the last none stands.
    let gap_above = |l: usize| if l == 0 { 8.0 } else { LANE_GAP };
    let gap_below = |l: usize| if l == count { 8.0 } else { LANE_GAP };
    let nets: Vec<Vec<usize>> = (0..d.servers.len())
        .map(|s| {
            (0..count)
                .filter(|&n| d.networks[n].members.iter().any(|(m, _)| *m == s))
                .collect()
        })
        .collect();
    let floating = |s: usize| nets[s].is_empty();
    let peers_of = |s: usize| {
        d.peers.iter().filter_map(move |&(a, b, _)| {
            if a == s {
                Some(b)
            } else if b == s {
                Some(a)
            } else {
                None
            }
        })
    };
    let mut lane: Vec<Option<usize>> = vec![None; d.servers.len()];
    for s in 0..d.servers.len() {
        let Some((&first, &last)) = nets[s].first().zip(nets[s].last()) else {
            continue;
        };
        lane[s] = Some(if last > first {
            first + 1
        } else if peers_of(s).any(&floating) {
            first
        } else {
            first + 1
        });
    }

    // Columns: the first in which nothing the server covers is taken.
    let span = |s: usize, l: usize| {
        let (first, last) = (nets[s][0], *nets[s].last().unwrap_or(&nets[s][0]));
        let (top, bottom) = (2 * first + 1, 2 * last + 1);
        ((2 * l).min(top), (2 * l).max(bottom), 2 * l)
    };
    let mut column: Vec<Option<usize>> = vec![None; d.servers.len()];
    let mut taken: Vec<Vec<(usize, usize, usize)>> = Vec::new();
    for s in 0..d.servers.len() {
        let Some(l) = lane[s] else { continue };
        let (top, bottom, at) = span(s, l);
        let free = |held: &Vec<(usize, usize, usize)>| {
            held.iter()
                .all(|&(t, b, a)| a != at && (top.max(t) >= bottom.min(b)))
        };
        let c = match taken.iter().position(free) {
            Some(c) => c,
            None => {
                taken.push(Vec::new());
                taken.len() - 1
            }
        };
        taken[c].push((top, bottom, at));
        column[s] = Some(c);
    }
    // A server on no network stands in the column of a peer: above it
    // where the peer stands above its bar, else in the lane above the
    // peer's first network, its line running through the bar. One with no
    // such peer stands in a column of its own above the first lane.
    let mut hangs: Vec<Option<usize>> = vec![None; d.servers.len()];
    for _ in 0..d.servers.len() {
        for s in (0..d.servers.len()).filter(|&s| floating(s)) {
            if column[s].is_some() {
                continue;
            }
            if let Some(p) = peers_of(s).find(|&p| column[p].is_some() && hangs[p] != Some(s)) {
                column[s] = column[p];
                match (nets[p].first(), lane[p]) {
                    (Some(&first), Some(l)) if l != first => lane[s] = Some(first),
                    _ => hangs[s] = Some(p),
                }
            }
        }
    }
    for c in &mut column {
        if c.is_none() {
            taken.push(Vec::new());
            *c = Some(taken.len() - 1);
        }
    }

    // Widths of the columns and the room for the networks' names.
    let names: Vec<Block> = d
        .networks
        .iter()
        .map(|n| {
            let mut text = n.name.clone();
            if let Some(address) = &n.address {
                text = format!("{text}\n{address}");
            }
            if let Some(description) = &n.description {
                text = format!("{text}\n{description}");
            }
            words(text.trim(), look.network_font, n.line)
        })
        .collect::<Result<_, _>>()?;
    let label_w = names.iter().map(|b| b.width).fold(0.0, f32::max);
    let left = if label_w > 0.0 { label_w + 14.0 } else { 0.0 };
    let mut widths = vec![0.0f32; taken.len()];
    for s in 0..d.servers.len() {
        let c = column[s].unwrap_or(0);
        widths[c] = widths[c].max(sizes[s].0);
    }
    let mut centers = Vec::with_capacity(widths.len());
    let mut x = left + COLUMN_GAP / 2.0;
    for w in &widths {
        centers.push(x + w / 2.0);
        x += w + COLUMN_GAP;
    }
    let right = x - COLUMN_GAP / 2.0;

    // Heights of the lanes, and what hangs above the first.
    let lanes = count + 1;
    let mut tall = vec![0.0f32; lanes];
    for s in 0..d.servers.len() {
        if let Some(l) = lane[s] {
            tall[l] = tall[l].max(sizes[s].1);
        }
    }
    // How far above its lane's top each hanging server's top is.
    let mut lift = vec![0.0f32; d.servers.len()];
    let mut above_first: f32 = 0.0;
    for (s, lifted) in lift.iter_mut().enumerate() {
        let mut chain = 0.0;
        let mut at = s;
        let mut steps = 0;
        while let Some(p) = hangs[at] {
            chain += sizes[at].1 + HANG;
            at = p;
            steps += 1;
            if steps > d.servers.len() {
                break;
            }
        }
        *lifted = chain;
        // The lane keeps room above its servers already.
        if lane[at] == Some(0) || lane[at].is_none() {
            above_first = above_first.max(chain - gap_above(0) + 6.0);
        }
    }
    let top = annotations.top() + above_first;
    let mut lane_top = vec![0.0f32; lanes];
    let mut bar_y = vec![0.0f32; count];
    let mut y = top;
    for l in 0..lanes {
        lane_top[l] = y;
        let h = if tall[l] > 0.0 {
            tall[l] + gap_above(l) + gap_below(l)
        } else if l == 0 || l == count {
            0.0
        } else {
            LANE_GAP
        };
        y += h;
        if l < count {
            bar_y[l] = y;
            y += BAR;
        }
    }

    // Where each server's box is: in its lane, or above the server it
    // hangs from.
    let root_of = |s: usize| {
        let mut at = s;
        for _ in 0..d.servers.len() {
            match hangs[at] {
                Some(p) => at = p,
                None => break,
            }
        }
        at
    };
    let in_lane = |l: usize, h: f32| lane_top[l] + gap_above(l) + (tall[l] - h) / 2.0;
    let boxes: Vec<Rect> = (0..d.servers.len())
        .map(|s| {
            let (w, h) = sizes[s];
            let y = match lane[s] {
                Some(l) => in_lane(l, h),
                None => {
                    let root = root_of(s);
                    match lane[root] {
                        Some(l) => in_lane(l, sizes[root].1) - lift[s],
                        None => top,
                    }
                }
            };
            Rect {
                x: centers[column[s].unwrap_or(0)] - w / 2.0,
                y,
                w,
                h,
            }
        })
        .collect();

    // Groups behind their servers.
    for group in &d.groups {
        let members: Vec<Rect> = group.members.iter().map(|&s| boxes[s]).collect();
        let Some(first) = members.first().copied() else {
            continue;
        };
        let (mut x0, mut y0, mut x1, mut y1) =
            (first.x, first.y, first.x + first.w, first.y + first.h);
        for r in &members {
            x0 = x0.min(r.x);
            y0 = y0.min(r.y);
            x1 = x1.max(r.x + r.w);
            y1 = y1.max(r.y + r.h);
        }
        let fill = group.color.unwrap_or(look.group_fill);
        let mut head = 0.0;
        let description = match &group.description {
            Some(text) => {
                let block = words(text, look.group_font, group.line)?;
                head = block.height + 4.0;
                Some(block)
            }
            None => None,
        };
        // The description under the servers, clear of the addresses on
        // the lines above them.
        let rect = (
            x0 - GROUP_PAD,
            y0 - GROUP_PAD,
            x1 - x0 + 2.0 * GROUP_PAD,
            y1 - y0 + 2.0 * GROUP_PAD + head,
        );
        canvas.rect(
            Layer::Background,
            rect,
            look.round.min(8.0),
            Some(fill),
            look.group_border.map(|c| Stroke::new(c, 1.0)),
        );
        if let Some(block) = description {
            let ink = legible(look.group_font.color, fill);
            let block = words(
                group.description.as_deref().unwrap_or_default(),
                Font {
                    color: ink,
                    ..look.group_font
                },
                group.line,
            )
            .unwrap_or(block);
            canvas.text(
                Layer::Front,
                &block,
                (rect.0 + rect.2 / 2.0, y1 + GROUP_PAD - 2.0),
                Align::Center,
            );
        }
    }

    // The networks: a bar each, its name at its left.
    let line = Stroke::new(look.line, look.line_width);
    for (n, network) in d.networks.iter().enumerate() {
        let members: Vec<f32> = network
            .members
            .iter()
            .map(|(s, _)| centers[column[*s].unwrap_or(0)])
            .collect();
        let (x0, x1) = if network.full || members.is_empty() {
            (left, right.max(left + 80.0))
        } else {
            let lo = members.iter().copied().fold(f32::MAX, f32::min);
            let hi = members.iter().copied().fold(f32::MIN, f32::max);
            let reach = |c: f32| {
                let i = centers.iter().position(|&x| x == c).unwrap_or(0);
                widths[i] / 2.0 + COLUMN_GAP / 2.0
            };
            (lo - reach(lo), hi + reach(hi))
        };
        let fill = network.color.unwrap_or(look.network_fill);
        canvas.rect(
            Layer::Frames,
            (x0, bar_y[n], x1 - x0, BAR),
            BAR / 2.0,
            Some(fill),
            Some(Stroke::new(look.network_border, look.network_width)),
        );
        let name = &names[n];
        canvas.text(
            Layer::Front,
            name,
            (x0 - 8.0, bar_y[n] + BAR / 2.0 - name.height / 2.0),
            Align::Right,
        );
        // The line from each server to the bar, its address beside it.
        for (s, address) in &network.members {
            let b = boxes[*s];
            let cx = b.x + b.w / 2.0;
            let below = b.y > bar_y[n];
            let (from, to) = if below {
                (bar_y[n] + BAR, b.y)
            } else {
                (b.y + b.h, bar_y[n])
            };
            canvas.line(Layer::Lifelines, (cx, from), (cx, to), line);
            if let Some(address) = address {
                let block = words(address, look.address_font, network.line)?;
                let y = if below {
                    bar_y[n] + BAR + 3.0
                } else {
                    bar_y[n] - block.height - 3.0
                };
                canvas.text(Layer::Front, &block, (cx + 5.0, y), Align::Left);
            }
        }
    }

    // Peer lines.
    for &(a, b, _) in &d.peers {
        let (ra, rb) = (boxes[a], boxes[b]);
        let (ca, cb) = (centre(ra), centre(rb));
        let (from, to) = if (ca.0 - cb.0).abs() < 0.5 {
            let (upper, lower) = if ra.y < rb.y { (ra, rb) } else { (rb, ra) };
            ((ca.0, upper.y + upper.h), (ca.0, lower.y))
        } else {
            (ca, cb)
        };
        canvas.line(Layer::Lifelines, from, to, line);
        // An address the server has outside a network, on its line.
        for (s, r) in [(a, ra), (b, rb)] {
            if let Some(address) = &d.servers[s].address {
                let block = words(address, look.address_font, d.servers[s].line)?;
                let y = if r.y > from.1.min(to.1) {
                    r.y - block.height - 3.0
                } else {
                    r.y + r.h + 3.0
                };
                canvas.text(Layer::Front, &block, (centre(r).0 + 5.0, y), Align::Left);
            }
        }
    }

    // The servers over their lines.
    for (s, server) in d.servers.iter().enumerate() {
        paint_server(&mut canvas, &look, server, boxes[s], &labels[s], family)?;
    }
    annotations.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, look.page))
}

fn paint_server(
    canvas: &mut Canvas,
    look: &Look,
    server: &Server,
    rect: Rect,
    label: &Block,
    family: Family,
) -> Result<(), Declined> {
    let fill = server.color.or(look.server_fill);
    paint_outline(
        canvas,
        server.shape,
        rect,
        look.round,
        fill,
        Stroke::new(look.server_border, look.server_width),
    );
    // The words read on the server's own color.
    let ground = fill.or(look.page).unwrap_or(Color::WHITE);
    let ink = legible(look.server_font.color, ground);
    let label = if ink == look.server_font.color {
        label.clone()
    } else {
        text::block(
            server.description.as_deref().unwrap_or(&server.name),
            Font {
                color: ink,
                ..look.server_font
            },
            family,
        )
        .map_err(|e| Declined::at(server.line, e))?
    };
    let area = words_area(server.shape, rect);
    canvas.text(
        Layer::Front,
        &label,
        (
            area.x + area.w / 2.0,
            area.y + (area.h - label.height) / 2.0,
        ),
        Align::Center,
    );
    Ok(())
}

/// `ink`, or black or white where it does not read on `ground`.
fn legible(ink: Color, ground: Color) -> Color {
    if ink.contrast(ground) >= 4.5 {
        return ink;
    }
    let far = if Color::WHITE.contrast(ground) > Color::BLACK.contrast(ground) {
        Color::WHITE
    } else {
        Color::BLACK
    };
    ink.legible_on(ground, far, 4.5)
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// The box a server of `shape` with `label` takes.
fn size_of(shape: Shape, label: &Block) -> (f32, f32) {
    let w = (label.width + 2.0 * PAD.0).max(64.0);
    let h = (label.height + 2.0 * PAD.1).max(34.0);
    match shape {
        Shape::Database | Shape::Storage => (w, h + 22.0),
        Shape::Cloud => (w * 1.35, h * 1.5),
        Shape::Node => (w + 10.0, h + 10.0),
        Shape::Actor => (w.max(48.0), h + crate::structure::ACTOR_FIGURE),
        Shape::Person => (w + 10.0, h + 30.0),
        Shape::Queue => (w + 24.0, h),
        Shape::UseCase => (w * 1.3, h * 1.3),
        Shape::Hexagon => (w + 20.0, h),
        Shape::Package | Shape::Folder => (w, h + 12.0),
        _ => (w, h),
    }
}

/// How a network diagram looks: PlantUML's, changed by the skinparams
/// and styles that reach it, and by the colors a pack lays over the
/// other types (`NodeBackgroundColor`, `ArrowColor`).
struct Look {
    font_name: String,
    sketch: bool,
    page: Paint,
    font: Font,
    title: Font,
    muted: Color,
    round: f32,
    server_fill: Paint,
    server_border: Color,
    server_width: f32,
    server_font: Font,
    network_fill: Color,
    network_border: Color,
    network_width: f32,
    network_font: Font,
    line: Color,
    line_width: f32,
    address_font: Font,
    group_fill: Color,
    group_border: Option<Color>,
    group_font: Font,
}

impl Look {
    fn read(params: &[Skinparam]) -> Result<Look, Declined> {
        let black = Color::BLACK;
        let dark = Color::hex(0x181818);
        let mut page: Paint = None;
        let mut ink = black;
        let mut size = 14.0;
        let mut font_name = "sans-serif".to_owned();
        let mut sketch = false;
        let mut round = 4.0;
        // What a style or a skinparam of the network diagram says.
        let mut own: std::collections::HashMap<String, (&Skinparam, String)> =
            std::collections::HashMap::new();
        for p in params {
            let key = normalize_key(&p.key);
            let value = p.value.trim().to_owned();
            match key.as_str() {
                "backgroundcolor" => page = color::parse(&value).map_err(|e| p.fail(e))?,
                "defaultfontcolor" => ink = p.solid()?,
                "defaultfontsize" => size = p.number(4.0, 100.0)?,
                "defaultfontname" => font_name = value.clone(),
                "handwritten" => sketch = value.eq_ignore_ascii_case("true"),
                "roundcorner" => round = p.number(0.0, 200.0)? / 2.0,
                _ => {}
            }
            own.insert(key, (p, value));
        }
        let color_of = |keys: &[&str]| -> Result<Option<Color>, Declined> {
            for key in keys {
                if let Some((p, _)) = own.get(*key) {
                    return color::parse(&p.value).map_err(|e| p.fail(e));
                }
            }
            Ok(None)
        };
        let number = |key: &str, lo: f32, hi: f32| -> Result<Option<f32>, Declined> {
            own.get(key).map(|(p, _)| p.number(lo, hi)).transpose()
        };
        let font = |prefix: &str, base: Font| -> Result<Font, Declined> {
            let mut f = base;
            if let Some(c) = color_of(&[&format!("{prefix}fontcolor")])? {
                f.color = c;
            }
            if let Some(s) = number(&format!("{prefix}fontsize"), 4.0, 100.0)? {
                f.size = s;
            }
            if let Some((_, style)) = own.get(&format!("{prefix}fontstyle")) {
                let style = style.to_ascii_lowercase();
                f.bold = style.contains("bold");
                f.italic = style.contains("italic");
            }
            Ok(f)
        };
        let base = Font::new(size, ink);
        let dark_page = page.is_some_and(Color::is_dark);
        let server_fill = color_of(&[
            "serverbackgroundcolor",
            "nodebackgroundcolor",
            "rectanglebackgroundcolor",
        ])?
        .or(Some(Color::hex(0xF1F1F1)));
        let server_border = color_of(&[
            "serverbordercolor",
            "nodebordercolor",
            "rectanglebordercolor",
        ])?
        .unwrap_or(if dark_page { ink } else { dark });
        let line = color_of(&["arrowcolor"])?.unwrap_or(if dark_page { ink } else { dark });
        let network_fill = color_of(&["networkbackgroundcolor"])?
            .or(color_of(&["nodebackgroundcolor"])?)
            .unwrap_or(Color::hex(0xE2E2F0));
        let network_border = color_of(&["networkbordercolor"])?
            .or(color_of(&["nodebordercolor"])?)
            .unwrap_or(server_border);
        let group_fill = color_of(&["groupbackgroundcolor"])?.unwrap_or(match page {
            Some(page) if dark_page => page.mix(ink, 0.12),
            _ => Color::hex(0xFFE3E3),
        });
        Ok(Look {
            font_name,
            sketch,
            page,
            font: base,
            title: font("title", base.bold())?,
            muted: if dark_page {
                ink.mix(page.unwrap_or(black), 0.35)
            } else {
                Color::hex(0x888888)
            },
            round: number("serverroundcorner", 0.0, 200.0)?.map_or(round, |r| r / 2.0),
            server_fill,
            server_border,
            server_width: number("serverborderthickness", 0.0, 20.0)?.unwrap_or(1.0),
            server_font: font("server", base)?,
            network_fill,
            network_border,
            network_width: number("networkborderthickness", 0.0, 20.0)?.unwrap_or(1.0),
            network_font: font("network", Font::new(size - 1.0, ink))?,
            line,
            line_width: number("arrowthickness", 0.0, 20.0)?.unwrap_or(1.0),
            address_font: font("arrow", Font::new((size - 3.0).max(8.0), ink))?,
            group_fill,
            group_border: color_of(&["groupbordercolor"])?,
            group_font: font("group", Font::new((size - 2.0).max(8.0), ink))?,
        })
    }
}
