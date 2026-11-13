//! The names a formula knows: TeX's symbols, Greek letters, relations,
//! operators and function names, and AsciiMath's names for them.

use super::{Atom, LARGE, Limits};

pub(super) const ASCII_GREEK: &[&str] = &[
    "alpha",
    "beta",
    "chi",
    "delta",
    "Delta",
    "epsilon",
    "varepsilon",
    "eta",
    "gamma",
    "Gamma",
    "iota",
    "kappa",
    "lambda",
    "Lambda",
    "mu",
    "nu",
    "omega",
    "Omega",
    "phi",
    "varphi",
    "Phi",
    "pi",
    "Pi",
    "psi",
    "Psi",
    "rho",
    "sigma",
    "Sigma",
    "tau",
    "theta",
    "vartheta",
    "Theta",
    "upsilon",
    "xi",
    "Xi",
    "zeta",
];

pub(super) const ASCII_FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "cot", "coth",
    "sech", "csch", "sec", "csc", "log", "ln", "det", "exp", "gcd", "lcm",
];

/// Names PlantUML's AsciiMath reads that this module does not draw: accents,
/// fonts, colors, angle and invisible brackets, the capitalized functions
/// and the few the Java PlantUML cannot draw either.
pub(super) const ASCII_DECLINED: &[&str] = &[
    "stackrel",
    "overset",
    "underset",
    "color",
    "cancel",
    "Sqrt",
    "hat",
    "bar",
    "overline",
    "vec",
    "tilde",
    "dot",
    "ddot",
    "overarc",
    "overparen",
    "ul",
    "underline",
    "ubrace",
    "underbrace",
    "obrace",
    "overbrace",
    "bb",
    "mathbf",
    "sf",
    "mathsf",
    "bbb",
    "mathbb",
    "cc",
    "mathcal",
    "tt",
    "mathtt",
    "fr",
    "mathfrak",
    "floor",
    "ceil",
    "Abs",
    "Lim",
    "Sin",
    "Cos",
    "Tan",
    "Arcsin",
    "Arccos",
    "Arctan",
    "Sinh",
    "Cosh",
    "Tanh",
    "Cot",
    "Sec",
    "Csc",
    "Log",
    "Ln",
    "lub",
    "glb",
    "-lt",
    ">->>",
    "twoheadrightarrowtail",
    "(:",
    ":)",
    "<<",
    ">>",
    "langle",
    "rangle",
    "{:",
    ":}",
    "|:",
    ":|",
    ":|:",
    "lbrace",
    "rbrace",
    "left(",
    "right)",
    "left[",
    "right]",
];

/// The AsciiMath names drawn as one symbol, with the TeX names AsciiMath
/// reads too.
pub(super) fn ascii_symbol(name: &str) -> Option<Sym> {
    if ASCII_GREEK.contains(&name)
        || matches!(
            name,
            "sum" | "prod" | "int" | "oint" | "lim" | "min" | "max"
        )
    {
        return symbol(name);
    }
    match name {
        "epsi" => return symbol("epsilon"),
        "lamda" => return symbol("lambda"),
        "Lamda" => return symbol("Lambda"),
        "^^^" | "bigwedge" => return Some(big("⋀", Limits::Above)),
        "vvv" | "bigvee" => return Some(big("⋁", Limits::Above)),
        "nnn" | "bigcap" => return Some(big("⋂", Limits::Above)),
        "uuu" | "bigcup" => return Some(big("⋃", Limits::Above)),
        _ => {}
    }
    ascii_operator(name)
        .or_else(|| ascii_relation(name))
        .or_else(|| ascii_other(name))
        .map(|(text, atom)| sym(text, atom, true))
}

pub(super) fn ascii_operator(name: &str) -> Option<(&'static str, Atom)> {
    let text = match name {
        "*" | "cdot" => "·",
        "**" | "ast" => "∗",
        "***" | "star" => "⋆",
        "setminus" => "∖",
        "xx" | "times" => "×",
        "|><" | "ltimes" => "⋉",
        "><|" | "rtimes" => "⋊",
        "-:" | "div" | "divide" => "÷",
        "@" | "circ" => "∘",
        "o+" | "oplus" => "⊕",
        "ox" | "otimes" => "⊗",
        "o." | "odot" => "⊙",
        "^^" | "wedge" => "∧",
        "vv" | "vee" => "∨",
        "nn" | "cap" => "∩",
        "uu" | "cup" => "∪",
        "+-" | "pm" => "±",
        "-+" | "mp" => "∓",
        "diamond" => "⋄",
        _ => return None,
    };
    Some((text, Atom::Bin))
}

pub(super) fn ascii_relation(name: &str) -> Option<(&'static str, Atom)> {
    let text = match name {
        "!=" | "ne" => "≠",
        ":=" => ":=",
        "lt" => "<",
        "gt" => ">",
        "<=" | "lt=" | "le" | "leq" => "≤",
        ">=" | "gt=" | "ge" | "geq" => "≥",
        "mlt" | "ll" => "≪",
        "mgt" | "gg" => "≫",
        "-<" | "prec" => "≺",
        ">-" | "succ" => "≻",
        "-<=" | "preceq" => "⪯",
        ">-=" | "succeq" => "⪰",
        "in" => "∈",
        "!in" | "notin" => "∉",
        "sub" | "subset" => "⊂",
        "sup" | "supset" => "⊃",
        "sube" | "subseteq" => "⊆",
        "supe" | "supseteq" => "⊇",
        "-=" | "equiv" => "≡",
        "~=" | "cong" => "≅",
        "~" | "sim" => "∼",
        "~~" | "approx" => "≈",
        "prop" | "propto" => "∝",
        "|><|" | "bowtie" => "⋈",
        "=>" | "implies" | "Rightarrow" | "rArr" => "⇒",
        "<=>" | "iff" | "Leftrightarrow" | "hArr" => "⇔",
        "|--" | "vdash" => "⊢",
        "|==" | "models" => "⊨",
        "frown" => "⌢",
        "uarr" | "uparrow" => "↑",
        "darr" | "downarrow" => "↓",
        "rarr" | "rightarrow" | "->" | "to" => "→",
        ">->" | "rightarrowtail" => "↣",
        "->>" | "twoheadrightarrow" => "↠",
        "|->" | "mapsto" => "↦",
        "larr" | "leftarrow" => "←",
        "harr" | "leftrightarrow" => "↔",
        "uArr" | "Uparrow" => "⇑",
        "dArr" | "Downarrow" => "⇓",
        "lArr" | "Leftarrow" => "⇐",
        _ => return None,
    };
    Some((text, Atom::Rel))
}

pub(super) fn ascii_other(name: &str) -> Option<(&'static str, Atom)> {
    Some(match name {
        "//" => ("/", Atom::Ord),
        "\\\\" | "backslash" => ("\\", Atom::Ord),
        "not" | "neg" => ("¬", Atom::Ord),
        "AA" | "forall" => ("∀", Atom::Ord),
        "EE" | "exists" => ("∃", Atom::Ord),
        "_|_" | "bot" => ("⊥", Atom::Ord),
        "TT" | "top" => ("⊤", Atom::Ord),
        "del" | "partial" => ("∂", Atom::Ord),
        "grad" | "nabla" => ("∇", Atom::Ord),
        "O/" | "emptyset" => ("∅", Atom::Ord),
        "oo" | "infty" => ("∞", Atom::Ord),
        "aleph" => ("ℵ", Atom::Ord),
        "..." | "ldots" => ("…", Atom::Ord),
        "cdots" => ("⋯", Atom::Ord),
        "vdots" => ("⋮", Atom::Ord),
        "ddots" => ("⋱", Atom::Ord),
        ":." | "therefore" => ("∴", Atom::Ord),
        ":'" | "because" => ("∵", Atom::Ord),
        "/_" | "angle" => ("∠", Atom::Ord),
        "/_\\" | "triangle" => ("△", Atom::Ord),
        "square" | "boxempty" => ("□", Atom::Ord),
        "%" => ("%", Atom::Ord),
        "prime" => ("′", Atom::Ord),
        "CC" => ("ℂ", Atom::Ord),
        "NN" => ("ℕ", Atom::Ord),
        "QQ" => ("ℚ", Atom::Ord),
        "RR" => ("ℝ", Atom::Ord),
        "ZZ" => ("ℤ", Atom::Ord),
        "|__" | "lfloor" => ("⌊", Atom::Open),
        "__|" | "rfloor" => ("⌋", Atom::Close),
        "|~" | "lceil" | "lceiling" => ("⌈", Atom::Open),
        "~|" | "rceil" | "rceiling" => ("⌉", Atom::Close),
        "dim" => ("dim", Atom::Op),
        "mod" => ("mod", Atom::Ord),
        _ => return None,
    })
}

pub(super) struct Sym {
    pub(super) text: &'static str,
    pub(super) atom: Atom,
    pub(super) upright: bool,
    pub(super) scale: f32,
    pub(super) limits: Limits,
}

pub(super) fn sym(text: &'static str, atom: Atom, upright: bool) -> Sym {
    Sym {
        text,
        atom,
        upright,
        scale: 1.0,
        limits: Limits::Side,
    }
}

pub(super) fn big(text: &'static str, limits: Limits) -> Sym {
    Sym {
        text,
        atom: Atom::Op,
        upright: true,
        scale: LARGE,
        limits,
    }
}

pub(super) fn symbol(name: &str) -> Option<Sym> {
    greek(name)
        .or_else(|| more_greek(name))
        .or_else(|| relation(name))
        .or_else(|| operator(name))
        .or_else(|| function_name(name))
}

pub(super) fn greek(name: &str) -> Option<Sym> {
    let text = match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ϵ",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        _ => return None,
    };
    Some(sym(text, Atom::Ord, false))
}

pub(super) fn more_greek(name: &str) -> Option<Sym> {
    let text = match name {
        "omicron" => "ο",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" => "ρ",
        "varrho" => "ϱ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "ϕ",
        "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        _ => return None,
    };
    // A capital Greek letter is upright, as in TeX.
    let upright = name.starts_with(|c: char| c.is_ascii_uppercase());
    Some(sym(text, Atom::Ord, upright))
}

pub(super) fn relation(name: &str) -> Option<Sym> {
    let text = match name {
        "leq" | "le" => "≤",
        "geq" | "ge" => "≥",
        "neq" | "ne" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "Rightarrow" => "⇒",
        "Leftarrow" => "⇐",
        "in" => "∈",
        "notin" => "∉",
        "subset" => "⊂",
        "subseteq" => "⊆",
        _ => return None,
    };
    Some(sym(text, Atom::Rel, true))
}

pub(super) fn operator(name: &str) -> Option<Sym> {
    Some(match name {
        "infty" | "oo" => sym("∞", Atom::Ord, true),
        "partial" => sym("∂", Atom::Ord, true),
        "nabla" => sym("∇", Atom::Ord, true),
        "ldots" => sym("…", Atom::Ord, true),
        "cdots" => sym("⋯", Atom::Ord, true),
        "cdot" => sym("·", Atom::Bin, true),
        "times" | "xx" => sym("×", Atom::Bin, true),
        "pm" => sym("±", Atom::Bin, true),
        "mp" => sym("∓", Atom::Bin, true),
        "cup" => sym("∪", Atom::Bin, true),
        "cap" => sym("∩", Atom::Bin, true),
        "forall" => sym("∀", Atom::Ord, true),
        "exists" => sym("∃", Atom::Ord, true),
        "sum" => big("∑", Limits::Above),
        "prod" => big("∏", Limits::Above),
        "int" => big("∫", Limits::Side),
        "oint" => big("∮", Limits::Side),
        _ => return None,
    })
}

pub(super) fn function_name(name: &str) -> Option<Sym> {
    let text = match name {
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "log" => "log",
        "ln" => "ln",
        "exp" => "exp",
        "lim" => "lim",
        "min" => "min",
        "max" => "max",
        _ => return None,
    };
    Some(Sym {
        limits: function_limits(name),
        ..sym(text, Atom::Op, true)
    })
}

/// A display formula puts the limits of `lim`, `min`, `max`, `det` and
/// `gcd` below them, as on a sum, and those of `log` beside it.
pub(super) fn function_limits(name: &str) -> Limits {
    if matches!(name, "lim" | "min" | "max" | "det" | "gcd") {
        Limits::Above
    } else {
        Limits::Side
    }
}

/// What an AsciiMath name stands for.
pub(super) enum Word {
    Symbol(Sym),
    /// A function such as `sin`, which takes the next item as its argument.
    Function,
    Space(f32),
    /// `and`, `or` and `if`, with a quad on either side.
    Spaced(&'static str),
    /// `dx`, one item for a `/`, so `dy/dx` is a fraction of the two.
    Differential(&'static str),
    /// A name with arguments: `sqrt`, `root`, `frac`, `text`, `mbox`,
    /// `abs` and `norm`.
    Construct,
    /// A name this module does not draw yet. It declines instead of
    /// drawing the letters, so the Java PlantUML draws the sheet.
    Declined,
}

/// The length of the longest name, `twoheadrightarrowtail`.
pub(super) const LONGEST_NAME: usize = 21;

/// The longest AsciiMath name `rest` starts with. AsciiMath reads names
/// this way, so `sinx` is sin x and `xin` is ξ n; a letter that starts no
/// name is a variable.
pub(super) fn ascii_name(rest: &str) -> Option<(&str, Word)> {
    let run = rest
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || c.is_ascii_digit())
        .map_or(rest.len(), |(at, _)| at);
    (1..=run.min(LONGEST_NAME))
        .rev()
        .filter(|&end| rest.is_char_boundary(end))
        .find_map(|end| ascii_word(&rest[..end]).map(|word| (&rest[..end], word)))
}

pub(super) fn ascii_word(name: &str) -> Option<Word> {
    if let Some(sym) = ascii_symbol(name) {
        return Some(Word::Symbol(sym));
    }
    Some(match name {
        "sqrt" | "root" | "frac" | "text" | "mbox" | "abs" | "norm" => Word::Construct,
        "quad" => Word::Space(1.0),
        "qquad" => Word::Space(2.0),
        "and" => Word::Spaced("and"),
        "or" => Word::Spaced("or"),
        "if" => Word::Spaced("if"),
        "dx" => Word::Differential("x"),
        "dy" => Word::Differential("y"),
        "dz" => Word::Differential("z"),
        "dt" => Word::Differential("t"),
        _ if ASCII_FUNCTIONS.contains(&name) => Word::Function,
        _ if ASCII_DECLINED.contains(&name) => Word::Declined,
        _ => return None,
    })
}
