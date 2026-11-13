use super::*;

use super::layout::{Face, layout};
use crate::Declined;
use crate::common::Frame;
use crate::svg::Canvas;

fn latex(body: &str) -> String {
    format!("@startlatex\n{body}\n@endlatex\n")
}

fn math(body: &str) -> String {
    format!("@startmath\n{body}\n@endmath\n")
}

fn drawn(source: &str) -> String {
    crate::render(source).unwrap()
}

fn root(source: &str) -> Node {
    match crate::parse(source).unwrap() {
        crate::Diagram::Formula(diagram) => diagram.root,
        other => panic!("not a formula: {other:?}"),
    }
}

fn frame(source: &str) -> Frame {
    match crate::parse(source).unwrap() {
        crate::Diagram::Formula(diagram) => diagram.frame,
        other => panic!("not a formula: {other:?}"),
    }
}

fn declined(source: &str) -> Declined {
    crate::parse(source).unwrap_err()
}

fn tspan<'a>(svg: &'a str, text: &str) -> &'a str {
    let needle = format!(">{text}</tspan>");
    let at = svg
        .find(&needle)
        .unwrap_or_else(|| panic!("no {text} in {svg}"));
    let start = svg[..at].rfind("<tspan").unwrap();
    &svg[start..at + needle.len()]
}

fn bars(svg: &str) -> Vec<f32> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("<line ") {
        rest = &rest[at..];
        let end = rest.find("/>").unwrap();
        let tag = &rest[..end];
        let y1 = attr(tag, "y1");
        let y2 = attr(tag, "y2");
        if (y1 - y2).abs() < 0.05 {
            out.push((attr(tag, "x2") - attr(tag, "x1")).abs());
        }
        rest = &rest[end..];
    }
    out
}

fn attr(tag: &str, name: &str) -> f32 {
    let key = format!("{name}=\"");
    let at = tag.find(&key).unwrap() + key.len();
    tag[at..].split('"').next().unwrap().parse().unwrap()
}

fn glyph_text(node: &Node) -> Option<&str> {
    match node {
        Node::Glyph(g) => Some(&g.text),
        _ => None,
    }
}

#[test]
fn an_integral_keeps_tau_and_sets_the_italic() {
    let svg = drawn(&latex(r"f(t) = \int_{0}^{t} h(\tau)\,d\tau"));
    assert!(svg.contains("font-family=\"serif\""), "{svg}");
    assert!(svg.contains(">τ</tspan>"), "{svg}");
    assert!(svg.contains(">∫</tspan>"), "{svg}");
    assert!(!svg.contains("\\tau"), "{svg}");
    assert!(tspan(&svg, "f").contains("italic"), "{svg}");
    assert!(tspan(&svg, "τ").contains("italic"), "{svg}");
    assert!(!tspan(&svg, "0").contains("italic"), "{svg}");
    assert!(!svg.contains("<path"), "{svg}");
}

#[test]
fn a_sum_puts_the_limits_and_the_bar_on_the_page() {
    let svg = drawn(&latex(
        "title Energy\n\\sum_{i=0}^{n} i^{2} = \\frac{(n+1)(2n+1)}{6}",
    ));
    assert!(svg.contains(">Energy</tspan>"), "{svg}");
    assert!(svg.contains(">∑</tspan>"), "{svg}");
    assert_eq!(bars(&svg).len(), 1, "{svg}");
    assert!(
        svg.contains(">#FFFFFF<") || svg.contains("#FFFFFF"),
        "{svg}"
    );
}

#[test]
fn ascii_math_uses_the_same_boxes() {
    let svg = drawn(&math("sum_(i=0)^n i^2 = (n(n+1)(2n+1))/6"));
    assert!(svg.contains(">∑</tspan>"), "{svg}");
    assert_eq!(bars(&svg).len(), 1, "{svg}");
    let svg = drawn(&math(r"\tau"));
    assert!(svg.contains(">τ</tspan>"), "{svg}");
}

#[test]
fn a_slash_binds_the_previous_factor() {
    let row = |source: &str| match root(source) {
        Node::Row(items) => items,
        other => panic!("{other:?}"),
    };
    let items = row(&math("a+b/c"));
    assert_eq!(glyph_text(&items[0]), Some("a"));
    assert!(matches!(
        items[1],
        Node::Glyph(Glyph {
            atom: Atom::Bin,
            ..
        })
    ));
    match &items[2] {
        Node::Frac(num, den) => {
            assert_eq!(glyph_text(num), Some("b"));
            assert_eq!(glyph_text(den), Some("c"));
        }
        other => panic!("{other:?}"),
    }
    let wide = bars(&drawn(&math("(a+b)/c")));
    let narrow = bars(&drawn(&math("a+b/c")));
    assert!(wide[0] > narrow[0] + 8.0, "{wide:?} {narrow:?}");
}

#[test]
fn a_script_stops_at_one_atom() {
    let svg = drawn(&latex("x^n+1"));
    assert!(tspan(&svg, "n").contains("font-size=\"14\""), "{svg}");
    assert!(tspan(&svg, "+").contains("font-size=\"20\""), "{svg}");
    assert!(tspan(&svg, "1").contains("font-size=\"20\""), "{svg}");
}

#[test]
fn a_tall_parenthesis_is_a_stroke_and_a_short_one_is_a_glyph() {
    let plain = drawn(&latex("f(t)"));
    assert!(!plain.contains("<path"), "{plain}");
    assert!(plain.contains(">(</tspan>"), "{plain}");
    let tall = drawn(&latex(r"\left( \frac{1}{2} \right)"));
    assert!(tall.contains("<path"), "{tall}");
    let grown = drawn(&latex(r"(\frac{1}{2})"));
    assert!(grown.contains("<path"), "{grown}");
}

#[test]
fn a_radical_is_a_stroke() {
    let svg = drawn(&latex(r"\sqrt{x}"));
    assert!(svg.contains("<path"), "{svg}");
    assert!(svg.contains(">x</tspan>"), "{svg}");
    assert!(drawn(&latex(r"\sqrt[n]{x}")).contains("<path"));
    assert!(drawn(&math("sqrt(x+1)")).contains("<path"));
}

#[test]
fn unknowns_name_the_line() {
    let err = declined(&latex("a\n\\hat{x}"));
    assert_eq!(err.line, Some(2));
    assert!(err.reason.contains("\\hat"), "{}", err.reason);
    let err = declined(&latex("x^n^2"));
    assert!(err.reason.contains("superscript"), "{}", err.reason);
    let err = declined(&latex(""));
    assert!(err.reason.contains("expression"), "{}", err.reason);
    let err = declined(&latex("<style>\n</style>\na"));
    assert_eq!(err.line, Some(1));
    assert!(err.reason.contains("<style>"), "{}", err.reason);
    let err = declined(&math("[[a,b],[c,d]]"));
    assert!(err.reason.contains("matrix"), "{}", err.reason);
    let err = declined(&latex(r"\begin{matrix} a \end{matrix}"));
    assert!(err.reason.contains("\\begin"), "{}", err.reason);
    let err = declined(&latex("a}"));
    assert!(err.reason.contains('}'), "{}", err.reason);
}

#[test]
fn nesting_stops_at_one_hundred() {
    let mut body = "{".repeat(101);
    body.push('x');
    body.push_str(&"}".repeat(101));
    let err = declined(&latex(&body));
    assert!(err.reason.contains("100"), "{}", err.reason);
    let mut ok = "{".repeat(100);
    ok.push('x');
    ok.push_str(&"}".repeat(100));
    drawn(&latex(&ok));
}

#[test]
fn an_upright_without_braces_counts_as_a_level() {
    let err = declined(&latex(&format!("{} x", r"\mathrm".repeat(5000))));
    assert!(err.reason.contains("100"), "{}", err.reason);
    drawn(&latex(&format!("{} x", r"\mathrm".repeat(100))));
}

#[test]
fn a_slash_takes_one_item_on_either_side() {
    assert_eq!(ascii("E = 1/2 m v^2"), "[E, =, (1 / 2), m, v^2]");
    assert_eq!(ascii("ab/c"), "[a, (b / c)]");
    assert_eq!(ascii("1/xy^2"), "[(1 / x), y^2]");
    assert_eq!(ascii("-a/b"), "[−, (a / b)]");
    assert_eq!(ascii("x^2/y"), "[(x^2 / y)]");
    assert_eq!(ascii("a/b_1^2 c"), "[(a / b_1^2), c]");
    assert_eq!(ascii("a/-b c"), "[(a / [−, b]), c]");
    assert_eq!(ascii("a/-(b+c)"), "[(a / [−, ([b, +, c])])]");
    assert_eq!(ascii("sin x/2"), "[([sin, x] / 2)]");
    assert_eq!(ascii("sin^2 x/2"), "[([sin^2, x] / 2)]");
    assert_eq!(ascii("lim x/2"), "[lim, (x / 2)]");
    assert_eq!(ascii("\"sin\" x/2"), "[sin, (x / 2)]");
    assert_eq!(ascii("a/sin x y"), "[(a / [sin, x]), y]");
    assert_eq!(ascii("a/sin/2"), "[(a / sin), /, 2]");
    assert_eq!(ascii("frac a b/c"), "[((a / b) / c)]");
    assert!(declined(&math("a/+b")).reason.contains("denominator"));
    assert!(declined(&math("a/")).reason.contains("denominator"));
    assert!(declined(&math("+/b")).reason.contains("numerator"));
}

#[test]
fn a_slash_drops_the_brackets_around_its_sides() {
    assert_eq!(ascii("(a+b)/c"), "[([a, +, b] / c)]");
    assert_eq!(ascii("[x]/{y}"), "[([x] / [y])]");
    assert_eq!(ascii("|x|/2"), "[(|[x]| / 2)]");
    assert_eq!(ascii("(a)^2/b"), "[(([a])^2 / b)]");
    let svg = drawn(&math("(a+b)/(c+d)"));
    assert!(!svg.contains(">(</tspan>"), "{svg}");
    assert!(!svg.contains("<path"), "{svg}");
}

#[test]
fn a_slash_after_a_fraction_is_drawn() {
    assert_eq!(ascii("1/1/1/1/1"), "[(1 / 1), /, (1 / 1), /, 1]");
    let Node::Row(items) = root(&math(&format!("1{}", "/1".repeat(20000)))) else {
        panic!("not a row");
    };
    assert_eq!(items.len(), 20001);
    drawn(&math(&format!("1{}", "/1".repeat(2000))));
}

#[test]
fn ascii_math_reads_the_longest_name() {
    assert_eq!(ascii("sinx"), "[sin, x]");
    assert_eq!(ascii("xin RR"), "[ξ, n, ℝ]");
    assert_eq!(ascii("x in RR"), "[x, ∈, ℝ]");
    assert_eq!(ascii("alphabeta"), "[α, β]");
    assert_eq!(ascii("omicron"), "[o, m, i, c, r, o, n]");
    assert_eq!(ascii("x <=> y +- z"), "[x, ⇔, y, ±, z]");
    assert_eq!(ascii("a ~~ b ~ c"), "[a, ≈, b, ∼, c]");
    assert_eq!(ascii("a == b <- c"), "[a, =, =, b, <, −, c]");
    assert_eq!(ascii("x |-> y // z"), "[x, ↦, y, /, z]");
    assert_eq!(ascii("AA x EE y"), "[∀, x, ∃, y]");
    assert_eq!(ascii("p and q"), "[p, [~, and, ~], q]");
    assert_eq!(ascii("{1, 2}"), "[{[1, ,, 2]}]");
}

#[test]
fn ascii_math_names_it_does_not_draw_decline() {
    for (source, name) in [
        ("hat x", "`hat`"),
        ("bb A", "`bb`"),
        ("tt", "`tt`"),
        ("Lim x", "`Lim`"),
        ("floor(x)", "`floor`"),
        ("(:a, b:)", "`(:`"),
        ("{:a:}", "`{:`"),
        ("a lub b", "`lub`"),
    ] {
        let err = declined(&math(source));
        assert!(err.reason.contains(name), "{source}: {}", err.reason);
    }
    assert!(declined(&math("((a,b),(c,d))")).reason.contains("matrix"));
    assert!(declined(&math("[(1),(2)]")).reason.contains("matrix"));
    drawn(&math("((a,b),[c,d]) + f((a), b)"));
}

#[test]
fn ascii_math_takes_arguments_as_asciimath_does() {
    assert_eq!(ascii("dy/dx"), "[([d, y] / [d, x])]");
    assert_eq!(ascii("f(x)/2"), "[([f, ([x])] / 2)]");
    assert_eq!(ascii("f[x]/2"), "[f, ([x] / 2)]");
    assert_eq!(ascii("a/f(x) y"), "[(a / [f, ([x])]), y]");
    assert_eq!(ascii("a/f x"), "[(a / f), x]");
    assert_eq!(ascii("sqrt x^2"), "[√{x}^2]");
    assert_eq!(ascii("sqrt(x+1)"), "[√{[x, +, 1]}]");
    assert_eq!(ascii("abs x^2 + norm(v)"), "[|x|^2, +, ‖[v]‖]");
    assert_eq!(ascii("2^-1 e^-x^2"), "[2^[−, 1], e^[−, x^2]]");
    assert_eq!(ascii("x^(n+1)_[i]"), "[x_[i]^[n, +, 1]]");
    assert_eq!(ascii("text(m/s) + mbox{a b}"), "[m/s, +, a b]");
    assert!(declined(&math("text x")).reason.contains("bracket"));
}

#[test]
fn a_tex_script_takes_one_token() {
    assert_eq!(tex("x^23 a_12 x^.5"), "[x^2, 3, a_1, 2, x^., 5]");
    assert_eq!(tex("x^{23} + \\frac12"), "[x^[23], +, (1 / 2)]");
    // AsciiMath reads a number as one token.
    assert_eq!(ascii("x^23"), "[x^23]");
}

#[test]
fn mathrm_is_upright_math_and_text_is_text() {
    assert_eq!(tex(r"\mathrm{m^2} \mathrm{kg\,m}"), "[[m^2], [k, g, ~, m]]");
    let svg = drawn(&latex(r"\mathrm{d}x + \mathrm{\alpha b}"));
    assert!(!tspan(&svg, "d").contains("italic"), "{svg}");
    assert!(tspan(&svg, "x").contains("italic"), "{svg}");
    assert!(tspan(&svg, "α").contains("italic"), "{svg}");
    assert!(!tspan(&svg, "b").contains("italic"), "{svg}");
    assert_eq!(tex(r"\text{a{b}c \{d\}}"), "[abc {d}]");
    assert!(declined(&latex(r"\text{m^2}")).reason.contains('^'));
}

#[test]
fn greek_follows_tex() {
    assert_eq!(tex(r"\epsilon \varepsilon \phi \varphi"), "[ϵ, ε, ϕ, φ]");
    assert_eq!(ascii("epsilon varepsilon phi varphi"), "[ϵ, ε, ϕ, φ]");
    let svg = drawn(&latex(r"\Gamma \gamma"));
    assert!(!tspan(&svg, "Γ").contains("italic"), "{svg}");
    assert!(tspan(&svg, "γ").contains("italic"), "{svg}");
}

#[test]
fn lim_min_and_max_put_their_limits_below() {
    let limits = |node: &Node| match node {
        Node::Script { limits, .. } => *limits,
        other => panic!("not a script: {other:?}"),
    };
    for source in [r"\lim_{x\to0}", r"\min_i", r"\max_j"] {
        let Node::Row(items) = root(&latex(source)) else {
            panic!("not a row");
        };
        assert_eq!(limits(&items[0]), Limits::Above, "{source}");
    }
    let Node::Row(items) = root(&math("lim_(x->0) det_i A log_2 x")) else {
        panic!("not a row");
    };
    let found: Vec<_> = items
        .iter()
        .filter(|n| matches!(n, Node::Script { .. }))
        .map(limits)
        .collect();
    assert_eq!(found, [Limits::Above, Limits::Above, Limits::Side]);
}

#[test]
fn a_script_drops_the_space_around_a_relation() {
    let row = root(&latex("i=1"));
    let Node::Row(items) = &row else {
        panic!("not a row");
    };
    for (face, space) in [(face(), 2.0 * THICK * DISPLAY), (face().script(), 0.0)] {
        let glyphs: f32 = items.iter().map(|n| layout(n, face).width).sum();
        let width = layout(&row, face).width;
        assert!((width - glyphs - space).abs() < 0.01, "{width} {glyphs}");
    }
}

#[test]
fn tex_pairs_brackets_of_either_kind_and_draws_the_rest() {
    assert_eq!(
        tex(r"[0, 1) \cup ]2, 3[ + (a"),
        "[⟦[0, ,, 1]), ∪, ], 2, ,, 3, [, +, (, a]"
    );
    assert_eq!(tex("([a)]"), "[([⟦[a])]⟧]");
    assert_eq!(tex("a/b P(A|B)"), "[a, /, b, P, ([A, |, B])]");
    assert_eq!(tex(r"\left. (a \right) b)"), "[[(, a]), b, )]");
    // Unpaired brackets cost one pass, not one per bracket.
    let Node::Row(items) = root(&latex(&"(".repeat(20000))) else {
        panic!("not a row");
    };
    assert_eq!(items.len(), 20000);
}

#[test]
fn an_asciimath_bar_without_a_second_is_drawn() {
    assert_eq!(ascii("P(A|B)"), "[P, ([A, |, B])]");
    assert_eq!(ascii("{x | x > 0}"), "[{[x, |, x, >, 0]}]");
    assert_eq!(ascii("|a| b |c| d|e"), "[|[a]|, b, |[c]|, d, |, e]");
    assert_eq!(ascii("x |-> |y|"), "[x, ↦, |[y]|]");
    assert_eq!(ascii("[0, 1) uu (2, 3]"), "[⟦[0, ,, 1]), ∪, ([2, ,, 3]⟧]");
    assert!(declined(&math("(a")).reason.contains("`(` has no closing"));
}

#[test]
fn a_tex_bracket_left_open_keeps_the_frame_lines() {
    for body in [
        "x = (a",
        r"\left( \frac{a}{b} \right.",
        "]2, 3[",
        r"\left\{ a \right.",
    ] {
        let frame = frame(&latex(&format!("{body}\ncaption Ratio")));
        assert!(frame.caption.is_some(), "{body}");
    }
    // A group that runs over lines keeps a title inside it.
    assert!(frame(&latex("{a\ntitle T\nb}")).title.is_none());
    assert!(
        frame(&latex("\\left( a\ntitle T\nb \\right)"))
            .title
            .is_none()
    );
    // A group left open declines instead of taking the caption in.
    assert!(declined(&latex("{a\ncaption Ratio")).reason.contains('{'));
    assert!(declined(&math("(a\ncaption Ratio")).reason.contains('('));
}

#[test]
fn two_relations_have_no_space_between() {
    let row = root(&latex(":="));
    let Node::Row(items) = &row else {
        panic!("not a row");
    };
    let glyphs: f32 = items.iter().map(|n| layout(n, face()).width).sum();
    assert!((layout(&row, face()).width - glyphs).abs() < 0.01);
}

#[test]
fn an_italic_keeps_its_lean_inside_the_picture() {
    let svg = drawn(&latex("f"));
    let width = attr(&svg[..svg.find('>').unwrap()], "width");
    let pack = layout(&root(&latex("f")), face());
    assert!(
        width >= pack.width + 0.08 * DISPLAY + 2.0 * MARGIN - 0.01,
        "{width} {svg}"
    );
}

fn face() -> Face {
    Face {
        size: DISPLAY,
        family: Canvas::new("serif", false).family(),
        in_script: false,
    }
}

fn tex(source: &str) -> String {
    shape(&root(&latex(source)))
}

fn ascii(source: &str) -> String {
    shape(&root(&math(source)))
}

/// A row as a list, a fraction as `(num / den)`, a fence by its
/// brackets, a script as `base_sub^sup` and a radical as `√{body}`.
fn shape(node: &Node) -> String {
    match node {
        Node::Glyph(g) => g.text.clone(),
        Node::Row(items) => {
            let items: Vec<_> = items.iter().map(shape).collect();
            format!("[{}]", items.join(", "))
        }
        Node::Frac(num, den) => format!("({} / {})", shape(num), shape(den)),
        Node::Script { base, sup, sub, .. } => {
            let mut out = shape(base);
            if let Some(sub) = sub {
                out.push_str(&format!("_{}", shape(sub)));
            }
            if let Some(sup) = sup {
                out.push_str(&format!("^{}", shape(sup)));
            }
            out
        }
        Node::Fence { open, body, close } => {
            let side = |fence: &Fence| match fence {
                Fence::Paren => ("(", ")"),
                Fence::Bracket => ("⟦", "⟧"),
                Fence::Brace => ("{", "}"),
                Fence::Bar => ("|", "|"),
                Fence::Double => ("‖", "‖"),
                Fence::None => ("", ""),
            };
            format!("{}{}{}", side(open).0, shape(body), side(close).1)
        }
        Node::Sqrt { body, .. } => format!("√{{{}}}", shape(body)),
        Node::Space(_) => "~".into(),
    }
}
