//! The mark on words: a quiet wash behind the letters. The hue is the
//! theme accent, nudged until it reads on the ground. The wash stays
//! faint, so light letters are not covered.

use iced::{Color, Point, Rectangle, Size};

/// The mark over a label, before the light's own fade.
#[derive(Clone, Copy, Debug)]
pub(super) struct Plate {
    pub color: Color,
    /// How much of the wash covers the letters, 0 to 1.
    pub fill: f32,
}

/// The sharp rectangle around `letters`, a little wider than them and a
/// little taller. The extra room follows the letter height.
pub(super) fn bounds(letters: Rectangle) -> Rectangle {
    let py = (letters.height * 0.18).clamp(2.0, 8.0);
    let px = (letters.height * 0.5).clamp(5.0, 18.0);
    Rectangle::new(
        Point::new(letters.x - px, letters.y - py),
        Size::new(letters.width + px * 2.0, letters.height + py * 2.0),
    )
}

/// `accent` for letters of color `ink` (0 to 1). `ground` is the fill under
/// the letters, when the picture has one; otherwise they sit on `paper`.
/// The wash keeps the accent, and moves off the ground when the two are close.
/// It stays faint, fainter still over light letters.
pub(super) fn plate(accent: Color, paper: Color, ink: [f32; 3], ground: Option<[f32; 3]>) -> Plate {
    let surface_l = ground.map_or_else(|| luma(paper), luma_rgb);
    let away = if surface_l >= 0.55 {
        Color::BLACK
    } else {
        Color::WHITE
    };
    let mut t = 0.0;
    let mut color = accent;
    while apart(luma(color), surface_l) < 1.7 && t < 0.8 {
        t += 0.2;
        color = mix(accent, away, t);
    }
    let fill = if luma_rgb(ink) >= 0.60 { 0.10 } else { 0.16 };
    Plate { color, fill }
}

fn luma(color: Color) -> f32 {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
}

fn luma_rgb([r, g, b]: [f32; 3]) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
}

/// Contrast of two luminances, 1 when they match.
fn apart(a: f32, b: f32) -> f32 {
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(hex: u32) -> Color {
        Color::from_rgb(
            ((hex >> 16) & 0xff) as f32 / 255.0,
            ((hex >> 8) & 0xff) as f32 / 255.0,
            (hex & 0xff) as f32 / 255.0,
        )
    }

    #[test]
    fn the_rectangle_is_wider_than_the_letters_and_square_in_spirit() {
        let letters = Rectangle::new(Point::new(10.0, 20.0), Size::new(40.0, 14.0));
        let plate = bounds(letters);
        assert!(plate.width > letters.width + 8.0, "{}", plate.width);
        assert!(plate.height > letters.height);
        assert!(plate.height - letters.height < plate.width - letters.width);
        assert!(plate.x < letters.x && plate.y < letters.y);
    }

    #[test]
    fn the_wash_stays_faint_over_light_letters() {
        let accent = rgb(0x7aa2f7);
        let paper = rgb(0x1a1b26);
        let plate = plate(accent, paper, [0.93, 0.94, 0.96], None);
        assert!(plate.fill <= 0.12, "fill {}", plate.fill);
        // The accent already reads on the dark paper, so the wash keeps it.
        assert!(
            (luma(plate.color) - luma(accent)).abs() < 0.08,
            "wash {} accent {}",
            luma(plate.color),
            luma(accent)
        );
    }

    #[test]
    fn a_pale_accent_moves_off_a_light_ground() {
        let accent = rgb(0xf5f7ff);
        let paper = rgb(0xf4f4f5);
        let plate = plate(accent, paper, [0.12, 0.12, 0.14], None);
        assert!(
            luma(plate.color) + 0.15 < luma(paper),
            "wash {} on {}",
            luma(plate.color),
            luma(paper)
        );
        assert!(plate.fill <= 0.18);
    }

    #[test]
    fn a_bright_accent_on_dark_paper_is_not_crushed() {
        let accent = rgb(0xf5f7ff);
        let paper = rgb(0x121214);
        let plate = plate(accent, paper, [1.0, 1.0, 1.0], None);
        assert!(luma(plate.color) > 0.7, "wash {}", luma(plate.color));
        assert!(plate.fill <= 0.12);
    }
}
