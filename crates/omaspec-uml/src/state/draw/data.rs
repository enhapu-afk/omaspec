//! Data among the states (`json name { … }`), as PlantUML draws it: a
//! box with its name in a band over its value, a JSON value drawn nested,
//! as a table (`skinparam jsonLayout nested`), by the engine's own JSON
//! drawer inside the box, in the state's look. Data it cannot draw
//! declines on its line, with what the JSON drawer says of it.

use super::super::Node;
use super::super::skin::Skin;
use super::{PAD_X, PAD_Y, Placed};
use crate::Declined;
use crate::svg::{Align, Canvas, Layer};
use crate::text::{self, Block, Family};

/// The name and the table of `node`, measured, and the room they take:
/// the name, which its line alone writes, in a band over the table.
pub(super) fn measure(
    node: &Node,
    skin: &Skin,
    family: Family,
) -> Result<(f32, f32, Block, Vec<Block>), Declined> {
    let fail = |err: String| Declined::at(node.line, err);
    let name = text::block(&node.key, skin.state.font, family)
        .map_err(fail)?
        .written_on(node.line);
    // Its value as the JSON drawer draws one nested, in the state's look,
    // or why not.
    let look = &skin.state;
    let mut lines = vec![
        "skinparam jsonLayout nested".to_owned(),
        "<style>".to_owned(),
        "jsonDiagram {".to_owned(),
        "  node {".to_owned(),
        format!("    LineColor {}", look.border),
        format!("    FontColor {}", look.font.color),
    ];
    if let Some(fill) = look.fill {
        lines.push(format!("    BackGroundColor {fill}"));
    }
    lines.extend(["  }".to_owned(), "}".to_owned(), "</style>".to_owned()]);
    lines.extend(node.label.value.lines().map(str::to_owned));
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let table = crate::embedded::picture_or_why("json", &lines)
        .map_err(|why| fail(format!("the data of `{}` is not drawn: {why}", node.key)))?;
    let table = text::picture_block(table, skin.state.font, family);
    // The table keeps the room around it its own picture has.
    let width = (name.width + 2.0 * PAD_X).max(table.width);
    let height = name.height + PAD_Y + table.height;
    Ok((width, height, name, vec![table]))
}

/// The box and the line under its band.
pub(super) fn paint_outline(canvas: &mut Canvas, node: &Placed) {
    let r = node.rect;
    canvas.rect(
        Layer::Frames,
        (r.x, r.y, r.w, r.h),
        0.0,
        node.item.fill,
        Some(node.item.border),
    );
    let y = r.y + node.item.title.height + PAD_Y;
    canvas.line(Layer::Frames, (r.x, y), (r.x + r.w, y), node.item.border);
}

/// The name in its band, the table under it.
pub(super) fn paint_words(canvas: &mut Canvas, node: &Placed) {
    let r = node.rect;
    let name = &node.item.title;
    canvas.text(
        Layer::Front,
        name,
        (r.x + r.w / 2.0, r.y + PAD_Y / 2.0),
        Align::Center,
    );
    if let Some(table) = node.item.description.first() {
        canvas.text(
            Layer::Front,
            table,
            (r.x + r.w / 2.0, r.y + name.height + PAD_Y),
            Align::Center,
        );
    }
}
