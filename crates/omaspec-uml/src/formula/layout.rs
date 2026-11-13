//! Setting a formula: each node a box of glyphs, strokes and curves with
//! its width, height and depth, as TeX sets them on a math axis.

use super::{
    AXIS, Atom, FENCE_RATIO, Fence, Glyph, Limits, MED, Node, SCRIPT, SCRIPT_FLOOR, STROKE, THICK,
    THIN, atom_class,
};
use crate::color::Color;
use crate::text::{self, Family, Font};

#[derive(Clone, Copy)]
pub(super) struct Face {
    pub(super) size: f32,
    pub(super) family: Family,
    /// Inside a script, which keeps only the thin space around an operator name.
    pub(super) in_script: bool,
}

impl Face {
    pub(super) fn script(self) -> Face {
        Face {
            size: (self.size * SCRIPT).max(SCRIPT_FLOOR),
            in_script: true,
            ..self
        }
    }

    pub(super) fn at(self, size: f32) -> Face {
        Face { size, ..self }
    }
}

pub(super) struct Pack {
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) depth: f32,
    pub(super) glyphs: Vec<Gly>,
    pub(super) rules: Vec<Rule>,
    pub(super) paths: Vec<Curve>,
}

pub(super) struct Gly {
    pub(super) text: String,
    pub(super) x: f32,
    /// The top of the glyph, relative to the pack's baseline. Positive is down.
    pub(super) y: f32,
    pub(super) size: f32,
    pub(super) italic: bool,
}

pub(super) struct Rule {
    pub(super) x1: f32,
    pub(super) x2: f32,
    pub(super) y: f32,
    pub(super) width: f32,
}

pub(super) enum Curve {
    Radical {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        width: f32,
    },
    Paren {
        left: bool,
        x: f32,
        y0: f32,
        y1: f32,
        w: f32,
        width: f32,
    },
    Bracket {
        left: bool,
        x: f32,
        y0: f32,
        y1: f32,
        w: f32,
        width: f32,
    },
    Brace {
        left: bool,
        x: f32,
        y0: f32,
        y1: f32,
        w: f32,
        width: f32,
    },
    Bar {
        x: f32,
        y0: f32,
        y1: f32,
        double: bool,
        gap: f32,
        width: f32,
    },
}

impl Pack {
    pub(super) fn empty() -> Pack {
        Pack {
            width: 0.0,
            height: 0.0,
            depth: 0.0,
            glyphs: Vec::new(),
            rules: Vec::new(),
            paths: Vec::new(),
        }
    }

    pub(super) fn add(&mut self, mut child: Pack, dx: f32, dy: f32) {
        self.height = self.height.max(child.height - dy);
        self.depth = self.depth.max(child.depth + dy);
        child.shift(dx, dy);
        self.glyphs.append(&mut child.glyphs);
        self.rules.append(&mut child.rules);
        self.paths.append(&mut child.paths);
    }

    pub(super) fn shift(&mut self, dx: f32, dy: f32) {
        for g in &mut self.glyphs {
            g.x += dx;
            g.y += dy;
        }
        for r in &mut self.rules {
            r.x1 += dx;
            r.x2 += dx;
            r.y += dy;
        }
        for p in &mut self.paths {
            p.shift(dx, dy);
        }
    }

    pub(super) fn flush_left(mut self) -> Pack {
        let min_x = self.min_x();
        if min_x < 0.0 {
            self.shift(-min_x, 0.0);
            self.width -= min_x;
        }
        let right = self
            .paths
            .iter()
            .map(Curve::right)
            .fold(self.width, f32::max);
        let right = self
            .rules
            .iter()
            .map(|r| r.x1.max(r.x2))
            .fold(right, f32::max);
        self.width = right;
        self
    }

    pub(super) fn min_x(&self) -> f32 {
        let mut m = 0.0_f32;
        for g in &self.glyphs {
            m = m.min(g.x);
        }
        for r in &self.rules {
            m = m.min(r.x1.min(r.x2));
        }
        for p in &self.paths {
            m = m.min(p.left());
        }
        m
    }
}

impl Curve {
    pub(super) fn shift(&mut self, dx: f32, dy: f32) {
        match self {
            Curve::Radical {
                x0,
                y0,
                x1,
                y1,
                x2,
                y2,
                x3,
                ..
            } => {
                *x0 += dx;
                *y0 += dy;
                *x1 += dx;
                *y1 += dy;
                *x2 += dx;
                *y2 += dy;
                *x3 += dx;
            }
            Curve::Paren { x, y0, y1, .. }
            | Curve::Bracket { x, y0, y1, .. }
            | Curve::Brace { x, y0, y1, .. }
            | Curve::Bar { x, y0, y1, .. } => {
                *x += dx;
                *y0 += dy;
                *y1 += dy;
            }
        }
    }

    pub(super) fn left(&self) -> f32 {
        match *self {
            Curve::Radical { x0, x1, x2, x3, .. } => x0.min(x1).min(x2).min(x3),
            Curve::Paren { x, .. }
            | Curve::Bracket { x, .. }
            | Curve::Brace { x, .. }
            | Curve::Bar { x, .. } => x,
        }
    }

    pub(super) fn right(&self) -> f32 {
        match *self {
            Curve::Radical { x0, x1, x2, x3, .. } => x0.max(x1).max(x2).max(x3),
            Curve::Paren { x, w, .. } | Curve::Bracket { x, w, .. } | Curve::Brace { x, w, .. } => {
                x + w
            }
            Curve::Bar { x, double, gap, .. } => x + if double { gap } else { 0.0 },
        }
    }
}

pub(super) fn layout(node: &Node, face: Face) -> Pack {
    match node {
        Node::Glyph(g) => layout_glyph(g, face).flush_left(),
        Node::Row(items) => layout_row(items, face),
        Node::Frac(a, b) => layout_frac(a, b, face).flush_left(),
        Node::Sqrt { index, body } => layout_sqrt(index.as_deref(), body, face).flush_left(),
        Node::Script {
            base,
            sup,
            sub,
            limits,
        } => layout_script(base, sup.as_deref(), sub.as_deref(), *limits, face).flush_left(),
        Node::Fence { open, body, close } => layout_fence(*open, body, *close, face).flush_left(),
        Node::Space(w) => {
            let mut pack = strut(face);
            pack.width = w * face.size;
            pack
        }
    }
}

pub(super) fn layout_row(items: &[Node], face: Face) -> Pack {
    if items.is_empty() {
        return strut(face);
    }
    let mut pack = Pack::empty();
    let mut x = 0.0;
    let mut prev: Option<Atom> = None;
    for item in items {
        if let Node::Space(w) = item {
            x += w * face.size;
            continue;
        }
        let mut class = atom_class(item);
        // A minus with nothing ordinary on its left is a sign, not a sum.
        if class == Atom::Bin && prev.is_none_or(opens_unary) {
            class = Atom::Ord;
        }
        if let Some(left) = prev {
            x += gap(left, class, face);
        }
        let child = layout(item, face);
        let w = child.width;
        pack.add(child, x, 0.0);
        x += w;
        prev = Some(class);
    }
    pack.width = x.max(0.0);
    if pack.glyphs.is_empty() && pack.paths.is_empty() && pack.rules.is_empty() {
        let s = strut(face);
        pack.height = pack.height.max(s.height);
        pack.depth = pack.depth.max(s.depth);
    }
    pack.flush_left()
}

pub(super) fn opens_unary(atom: Atom) -> bool {
    matches!(
        atom,
        Atom::Open | Atom::Bin | Atom::Rel | Atom::Op | Atom::Punct
    )
}

/// The space between two atoms. A script keeps only the thin space around
/// an operator name, as TeX does, so a limit reads `i=1`, not `i = 1`.
pub(super) fn gap(left: Atom, right: Atom, face: Face) -> f32 {
    let around_op = (matches!(left, Atom::Op)
        && matches!(right, Atom::Ord | Atom::Open | Atom::Op))
        || (matches!(right, Atom::Op) && matches!(left, Atom::Ord | Atom::Close));
    let ems = if face.in_script {
        if around_op { THIN } else { 0.0 }
    } else if matches!(left, Atom::Rel) && matches!(right, Atom::Rel) {
        0.0
    } else if matches!(left, Atom::Rel) || matches!(right, Atom::Rel) {
        THICK
    } else if matches!(left, Atom::Bin) || matches!(right, Atom::Bin) {
        MED
    } else if around_op
        || matches!(left, Atom::Punct)
        || (matches!(left, Atom::Close) && matches!(right, Atom::Open))
    {
        THIN
    } else {
        0.0
    };
    ems * face.size
}

pub(super) fn layout_glyph(g: &Glyph, face: Face) -> Pack {
    let drawn = glyph_pack(&g.text, face.at(face.size * g.scale), !g.upright);
    if g.scale <= 1.0 {
        return drawn;
    }
    // The operator's box is centered on the math axis, not on the baseline.
    let center = (drawn.height - drawn.depth) / 2.0;
    nudge(drawn, center - face.size * AXIS)
}

pub(super) fn nudge(mut pack: Pack, dy: f32) -> Pack {
    pack.shift(0.0, dy);
    pack.height = (pack.height - dy).max(0.0);
    pack.depth = (pack.depth + dy).max(0.0);
    pack
}

pub(super) fn layout_frac(num: &Node, den: &Node, face: Face) -> Pack {
    let num = layout(num, face);
    let den = layout(den, face);
    let em = face.size;
    let thick = (0.06 * em).max(1.2);
    let gap = 0.16 * em;
    let pad = 0.2 * em;
    let axis_y = -AXIS * em;
    let w = num.width.max(den.width) + pad * 2.0;
    let num_w = num.width;
    let den_w = den.width;
    let num_dy = axis_y - gap - thick / 2.0 - num.depth;
    let den_dy = axis_y + gap + thick / 2.0 + den.height;
    let mut pack = Pack::empty();
    pack.add(num, (w - num_w) / 2.0, num_dy);
    pack.add(den, (w - den_w) / 2.0, den_dy);
    pack.rules.push(Rule {
        x1: 0.0,
        x2: w,
        y: axis_y,
        width: thick,
    });
    pack.width = w;
    pack.height = pack.height.max(-axis_y + thick / 2.0);
    pack.depth = pack.depth.max(axis_y + thick / 2.0);
    pack
}

pub(super) fn layout_sqrt(index: Option<&Node>, body: &Node, face: Face) -> Pack {
    let body = layout(body, face);
    let index = index.map(|n| layout(n, face.script()));
    let em = face.size;
    let thick = (0.06 * em).max(1.2);
    let gap = 0.14 * em;
    let pad = 0.3 * em;
    let hook = (0.58 * em).max(8.0);
    let body_x = hook;
    let bar_y = -(body.height + gap);
    let y_bot = body.depth + 0.16 * em;
    let x3 = hook + body.width + pad;
    let mut pack = Pack::empty();
    pack.add(body, body_x, 0.0);
    pack.paths.push(Curve::Radical {
        x0: hook * 0.02,
        y0: bar_y + (y_bot - bar_y) * 0.62,
        x1: hook * 0.42,
        y1: y_bot,
        x2: hook * 0.96,
        y2: bar_y,
        x3,
        width: thick.max(STROKE),
    });
    pack.width = x3;
    pack.height = pack.height.max(-bar_y + thick / 2.0);
    pack.depth = pack.depth.max(y_bot + thick / 2.0);
    if let Some(index) = index {
        let ix = hook * 0.42 - index.width;
        let iy = bar_y + (y_bot - bar_y) * 0.62 - index.depth - 0.04 * em;
        pack.add(index, ix, iy);
    }
    pack
}

pub(super) fn layout_script(
    base: &Node,
    sup: Option<&Node>,
    sub: Option<&Node>,
    limits: Limits,
    face: Face,
) -> Pack {
    let base = layout(base, face);
    let script = face.script();
    let sup = sup.map(|n| layout(n, script));
    let sub = sub.map(|n| layout(n, script));
    match limits {
        Limits::Above => place_limits(base, sup, sub, face),
        Limits::Side => place_side(base, sup, sub, face),
    }
}

pub(super) fn place_side(base: Pack, sup: Option<Pack>, sub: Option<Pack>, face: Face) -> Pack {
    let em = face.size;
    let mut sup_up = 0.45 * em;
    let mut sub_down = 0.22 * em;
    // A tall operator keeps its superscript at the top and its subscript at the foot.
    if base.height > em * 0.928 + 2.0
        && let Some(sup) = &sup
    {
        sup_up = sup_up.max(base.height - sup.depth - 0.12 * em);
    }
    if base.depth > em * 0.35
        && let Some(sub) = &sub
    {
        sub_down = sub_down.max(base.depth - sub.depth);
    }
    if let (Some(sup), Some(sub)) = (&sup, &sub) {
        let gap = 0.08 * em;
        let sup_bottom = -sup_up + sup.depth;
        let sub_top = sub_down - sub.height;
        if sub_top < sup_bottom + gap {
            let need = sup_bottom + gap - sub_top;
            sup_up += need / 2.0;
            sub_down += need / 2.0;
        }
    }
    let lean = if base.height > em * 1.2 {
        0.14 * em
    } else if leans(&base) {
        0.08 * em
    } else {
        0.05 * em
    };
    let x = base.width + lean;
    let script_w = sup
        .as_ref()
        .map(|s| s.width)
        .unwrap_or(0.0)
        .max(sub.as_ref().map(|s| s.width).unwrap_or(0.0));
    let mut pack = Pack::empty();
    let base_w = base.width;
    pack.add(base, 0.0, 0.0);
    if let Some(sup) = sup {
        pack.add(sup, x, -sup_up);
    }
    if let Some(sub) = sub {
        pack.add(sub, x, sub_down);
    }
    pack.width = if script_w > 0.0 { x + script_w } else { base_w };
    pack
}

pub(super) fn place_limits(base: Pack, sup: Option<Pack>, sub: Option<Pack>, face: Face) -> Pack {
    let gap = 0.18 * face.size;
    let base_w = base.width;
    let base_h = base.height;
    let base_d = base.depth;
    let sup_w = sup.as_ref().map(|s| s.width).unwrap_or(0.0);
    let sub_w = sub.as_ref().map(|s| s.width).unwrap_or(0.0);
    let w = base_w.max(sup_w).max(sub_w);
    let mut pack = Pack::empty();
    pack.add(base, (w - base_w) / 2.0, 0.0);
    if let Some(sup) = sup {
        let dy = -(base_h + gap + sup.depth);
        let sw = sup.width;
        pack.add(sup, (w - sw) / 2.0, dy);
    }
    if let Some(sub) = sub {
        let dy = base_d + gap + sub.height;
        let sw = sub.width;
        pack.add(sub, (w - sw) / 2.0, dy);
    }
    pack.width = w;
    pack
}

pub(super) fn leans(pack: &Pack) -> bool {
    pack.glyphs
        .iter()
        .max_by(|a, b| a.x.total_cmp(&b.x))
        .is_some_and(|g| g.italic)
}

pub(super) fn layout_fence(open: Fence, body: &Node, close: Fence, face: Face) -> Pack {
    let inner = layout(body, face);
    let sample = glyph_pack("(", face, false);
    let content = inner.height + inner.depth;
    if content > (sample.height + sample.depth) * FENCE_RATIO {
        stretch_fence(open, inner, close, face)
    } else {
        glyph_fence(open, inner, close, face)
    }
}

pub(super) fn glyph_fence(open: Fence, inner: Pack, close: Fence, face: Face) -> Pack {
    let mut pack = Pack::empty();
    let mut x = 0.0;
    let mut prev = None;
    if let Some(ch) = fence_char(open, false) {
        let g = glyph_pack(ch, face, false);
        x = place(&mut pack, g, x, prev, fence_atom(open, false), face);
        prev = Some(fence_atom(open, false));
    }
    let inner_w = inner.width;
    if let Some(left) = prev {
        x += gap(left, Atom::Ord, face);
    }
    pack.add(inner, x, 0.0);
    x += inner_w;
    if let Some(ch) = fence_char(close, true) {
        let class = fence_atom(close, true);
        x += gap(Atom::Ord, class, face);
        let g = glyph_pack(ch, face, false);
        let w = g.width;
        pack.add(g, x, 0.0);
        x += w;
    }
    pack.width = x;
    pack
}

pub(super) fn place(
    pack: &mut Pack,
    child: Pack,
    mut x: f32,
    prev: Option<Atom>,
    class: Atom,
    face: Face,
) -> f32 {
    if let Some(left) = prev {
        x += gap(left, class, face);
    }
    let w = child.width;
    pack.add(child, x, 0.0);
    x + w
}

pub(super) fn fence_atom(fence: Fence, close: bool) -> Atom {
    match fence {
        Fence::Bar | Fence::Double | Fence::None => Atom::Ord,
        _ if close => Atom::Close,
        _ => Atom::Open,
    }
}

pub(super) fn fence_char(fence: Fence, close: bool) -> Option<&'static str> {
    Some(match fence {
        Fence::Paren => {
            if close {
                ")"
            } else {
                "("
            }
        }
        Fence::Bracket => {
            if close {
                "]"
            } else {
                "["
            }
        }
        Fence::Brace => {
            if close {
                "}"
            } else {
                "{"
            }
        }
        Fence::Bar => "|",
        Fence::Double => "‖",
        Fence::None => return None,
    })
}

pub(super) fn stretch_fence(open: Fence, inner: Pack, close: Fence, face: Face) -> Pack {
    let em = face.size;
    let pad_x = 0.16 * em;
    let pad_y = 0.12 * em;
    let y0 = -(inner.height + pad_y);
    let y1 = inner.depth + pad_y;
    let mut pack = Pack::empty();
    let mut x = 0.0;
    if open != Fence::None {
        let w = fence_span(open, em);
        pack.paths.push(fence_curve(open, true, x, y0, y1, w));
        x += w + pad_x;
    }
    let inner_w = inner.width;
    pack.add(inner, x, 0.0);
    x += inner_w;
    if close != Fence::None {
        x += pad_x;
        let w = fence_span(close, em);
        pack.paths.push(fence_curve(close, false, x, y0, y1, w));
        x += w;
    }
    pack.width = x;
    pack.height = pack.height.max(-y0 + STROKE / 2.0);
    pack.depth = pack.depth.max(y1 + STROKE / 2.0);
    pack
}

pub(super) fn fence_span(fence: Fence, em: f32) -> f32 {
    match fence {
        Fence::None => 0.0,
        Fence::Bar => 0.12 * em,
        Fence::Double => 0.28 * em,
        Fence::Brace => 0.52 * em,
        Fence::Paren | Fence::Bracket => 0.4 * em,
    }
}

pub(super) fn fence_curve(fence: Fence, left: bool, x: f32, y0: f32, y1: f32, w: f32) -> Curve {
    match fence {
        Fence::Paren => Curve::Paren {
            left,
            x,
            y0,
            y1,
            w,
            width: STROKE,
        },
        Fence::Bracket => Curve::Bracket {
            left,
            x,
            y0,
            y1,
            w,
            width: STROKE,
        },
        Fence::Brace => Curve::Brace {
            left,
            x,
            y0,
            y1,
            w,
            width: STROKE,
        },
        Fence::Bar | Fence::Double | Fence::None => Curve::Bar {
            x: x + if fence == Fence::Double { 0.0 } else { w / 2.0 },
            y0,
            y1,
            double: fence == Fence::Double,
            gap: w * 0.6,
            width: STROKE,
        },
    }
}

pub(super) fn glyph_pack(text: &str, face: Face, italic: bool) -> Pack {
    if text.is_empty() {
        return strut(face);
    }
    let style = run_style(face.size, italic);
    let font = Font::new(face.size, Color::BLACK);
    let block = text::styled(vec![vec![(text.to_string(), style)]], font, face.family);
    let line = &block.lines[0];
    Pack {
        width: line.width,
        height: line.ascent,
        depth: (line.height - line.ascent).max(0.0),
        glyphs: vec![Gly {
            text: text.to_string(),
            x: 0.0,
            y: -line.ascent,
            size: face.size,
            italic,
        }],
        rules: Vec::new(),
        paths: Vec::new(),
    }
}

pub(super) fn strut(face: Face) -> Pack {
    let mut pack = glyph_pack("x", face, false);
    pack.width = 0.0;
    pack.glyphs.clear();
    pack
}

pub(super) fn run_style(size: f32, italic: bool) -> text::Style {
    text::Style {
        size,
        bold: false,
        italic,
        underline: false,
        strike: false,
        mono: false,
        color: Color::BLACK,
    }
}
