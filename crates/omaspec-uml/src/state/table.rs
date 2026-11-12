//! A state machine's table drawn in the look of its states
//! ([`crate::table`]): the heads filled as a state, the words in its ink,
//! the grid in its border, under the diagram's title, with its caption.

use super::Diagram;
use super::skin::Skin;
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::Color;
use crate::map::Map;
use crate::svg::Canvas;
use crate::table::{self, Look, Table};

const MARGIN: f32 = 10.0;

/// `table`, the state-event matrix of `diagram`, drawn in its look.
pub fn draw_table(diagram: &Diagram, table: &Table) -> Result<(String, Map), Declined> {
    let skin = Skin::from_params(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    canvas.sketch(skin.sketch);
    canvas.shadows(skin.shadow);
    let family = canvas.family();
    let annotations = Annotations::new(
        &diagram.frame,
        [
            skin.title_font,
            skin.header_font,
            skin.footer_font,
            skin.caption_font,
        ],
        family,
    )?;
    let page = skin.background.unwrap_or(Color::WHITE);
    let look = Look {
        font: skin.state.font,
        head: skin.state.fill,
        page: Some(page),
        grid: skin.state.border,
        round: skin.round,
        warn: Color::hex(0xE8590C),
    };
    let cells = table::draw(&mut canvas, table, &look, family, annotations.top());
    annotations.draw(&mut canvas);
    let map = Map {
        cells,
        ..Map::default()
    }
    .placed(canvas.fit(MARGIN));
    Ok((canvas.finish(MARGIN, skin.background), map))
}
