//! Expressions of the preprocessor: numbers, strings, variables, calls and
//! the operators PlantUML knows. They are parsed into a tree before they
//! run, so `&&` and `||` skip the side they do not need.

use std::fmt;

/// Brackets, signs and calls inside each other. Each level is a few frames
/// of the parser, and a deep one would overflow the stack.
const MAX_NESTING: usize = 100;
/// Operators in one expression. A chain such as `1 + 1 + …` is read and run
/// in a loop, but its tree is as deep as it is long when it is freed.
const MAX_OPERATORS: usize = 1000;

/// A value: PlantUML's preprocessor knows integers and strings (and JSON,
/// which declines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Str(String),
}

impl Value {
    /// `0` and the empty string are false.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Int(n) => *n != 0,
            Value::Str(s) => !s.is_empty(),
        }
    }

    pub fn from_bool(value: bool) -> Value {
        Value::Int(i64::from(value))
    }

    /// The number this is or is written as.
    pub fn number(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Str(s) => s.trim().parse().ok(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Str(s) => f.write_str(s),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Lit(Value),
    Var(String),
    Call(String, Vec<Arg>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
}

/// One argument of a call: `expr`, or `$name = expr` by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    pub name: Option<String>,
    pub expr: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

/// Parses a whole expression; the error says what is not understood.
pub fn parse(text: &str) -> Result<Expr, String> {
    let mut parser = Parser::new(tokens(text)?);
    if parser.tokens.is_empty() {
        return Err("an expression is missing".into());
    }
    let expr = parser.or()?;
    parser.end()?;
    Ok(expr)
}

/// Parses one argument written in a line: `expr` or `$name = expr`.
pub fn parse_arg(text: &str) -> Result<Arg, String> {
    let mut parser = Parser::new(tokens(text)?);
    let arg = parser.arg()?;
    parser.end()?;
    Ok(arg)
}

/// `a op b` for every operator but `&&` and `||`.
pub fn binary(op: Op, a: Value, b: Value) -> Result<Value, String> {
    use std::cmp::Ordering;
    let order = |a: &Value, b: &Value| match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        _ => a.to_string().cmp(&b.to_string()),
    };
    let numbers = |sign: &str| match (a.number(), b.number()) {
        (Some(x), Some(y)) => Ok((x, y)),
        _ => Err(format!("`{sign}` needs numbers")),
    };
    let overflow = || "a number is too large".to_owned();
    Ok(match op {
        Op::Eq => Value::from_bool(order(&a, &b) == Ordering::Equal),
        Op::Ne => Value::from_bool(order(&a, &b) != Ordering::Equal),
        Op::Lt => Value::from_bool(order(&a, &b) == Ordering::Less),
        Op::Le => Value::from_bool(order(&a, &b) != Ordering::Greater),
        Op::Gt => Value::from_bool(order(&a, &b) == Ordering::Greater),
        Op::Ge => Value::from_bool(order(&a, &b) != Ordering::Less),
        Op::Add => match (&a, &b) {
            (Value::Int(x), Value::Int(y)) => Value::Int(x.checked_add(*y).ok_or_else(overflow)?),
            _ => Value::Str(format!("{a}{b}")),
        },
        Op::Sub => {
            let (x, y) = numbers("-")?;
            Value::Int(x.checked_sub(y).ok_or_else(overflow)?)
        }
        Op::Mul => {
            let (x, y) = numbers("*")?;
            Value::Int(x.checked_mul(y).ok_or_else(overflow)?)
        }
        Op::Div | Op::Rem => {
            let (x, y) = numbers(if op == Op::Div { "/" } else { "%" })?;
            if y == 0 {
                return Err("division by zero".into());
            }
            let result = if op == Op::Div {
                x.checked_div(y)
            } else {
                x.checked_rem(y)
            };
            Value::Int(result.ok_or_else(overflow)?)
        }
        Op::And | Op::Or => unreachable!("`&&` and `||` are evaluated lazily"),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Int(i64),
    Str(String),
    Name(String),
    Open,
    Close,
    Comma,
    Assign,
    Op(&'static str),
}

fn tokens(text: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let digits: String = chars[start..i].iter().collect();
            let value = digits
                .parse()
                .map_err(|_| format!("`{digits}` is too large"))?;
            out.push(Tok::Int(value));
        } else if c == '"' || c == '\'' {
            let start = i + 1;
            i = start;
            while i < chars.len() && chars[i] != c {
                i += 1;
            }
            if i == chars.len() {
                return Err("a quote is not closed".into());
            }
            out.push(Tok::Str(chars[start..i].iter().collect()));
            i += 1;
        } else if is_name_start(c, next) {
            let start = i;
            i += 1;
            while i < chars.len() && is_name_char(chars[i]) {
                i += 1;
            }
            out.push(Tok::Name(chars[start..i].iter().collect()));
        } else {
            let pair: String = [Some(c), next].into_iter().flatten().collect();
            if let Some(op) = ["&&", "||", "==", "!=", "<=", ">="]
                .into_iter()
                .find(|op| pair == *op)
            {
                out.push(Tok::Op(op));
                i += 2;
                continue;
            }
            out.push(match c {
                '(' => Tok::Open,
                ')' => Tok::Close,
                ',' => Tok::Comma,
                '=' => Tok::Assign,
                '<' => Tok::Op("<"),
                '>' => Tok::Op(">"),
                '+' => Tok::Op("+"),
                '-' => Tok::Op("-"),
                '*' => Tok::Op("*"),
                '/' => Tok::Op("/"),
                '%' => Tok::Op("%"),
                '!' => Tok::Op("!"),
                '{' | '[' => return Err("JSON values are not supported yet".into()),
                _ => return Err(format!("`{c}` is not understood")),
            });
            i += 1;
        }
    }
    Ok(out)
}

/// `$name`, `%name` or a bare name.
fn is_name_start(c: char, next: Option<char>) -> bool {
    match c {
        '$' | '%' => next.is_some_and(|n| n.is_alphabetic() || n == '_'),
        _ => c.is_alphabetic() || c == '_',
    }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

struct Parser {
    tokens: Vec<Tok>,
    at: usize,
    /// Brackets, signs and calls open around the token being read.
    depth: usize,
    operators: usize,
}

impl Parser {
    fn new(tokens: Vec<Tok>) -> Self {
        Parser {
            tokens,
            at: 0,
            depth: 0,
            operators: 0,
        }
    }

    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return Err(format!(
                "an expression nests more than {MAX_NESTING} levels deep"
            ));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// Counts one operator of the tree.
    fn operator(&mut self) -> Result<(), String> {
        self.operators += 1;
        if self.operators > MAX_OPERATORS {
            return Err(format!(
                "an expression has more than {MAX_OPERATORS} operators"
            ));
        }
        Ok(())
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at)
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == Some(tok) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn end(&self) -> Result<(), String> {
        match self.peek() {
            None => Ok(()),
            Some(tok) => Err(format!("{} is not expected here", describe(tok))),
        }
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut left = self.and()?;
        while self.eat_op("||") {
            self.operator()?;
            left = Expr::Bin(Op::Or, Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut left = self.compare()?;
        while self.eat_op("&&") {
            self.operator()?;
            left = Expr::Bin(Op::And, Box::new(left), Box::new(self.compare()?));
        }
        Ok(left)
    }

    fn compare(&mut self) -> Result<Expr, String> {
        let left = self.add()?;
        for (sign, op) in [
            ("==", Op::Eq),
            ("!=", Op::Ne),
            ("<=", Op::Le),
            (">=", Op::Ge),
            ("<", Op::Lt),
            (">", Op::Gt),
        ] {
            if self.eat_op(sign) {
                self.operator()?;
                return Ok(Expr::Bin(op, Box::new(left), Box::new(self.add()?)));
            }
        }
        Ok(left)
    }

    fn add(&mut self) -> Result<Expr, String> {
        let mut left = self.mul()?;
        loop {
            let op = if self.eat_op("+") {
                Op::Add
            } else if self.eat_op("-") {
                Op::Sub
            } else {
                return Ok(left);
            };
            self.operator()?;
            left = Expr::Bin(op, Box::new(left), Box::new(self.mul()?));
        }
    }

    fn mul(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        loop {
            let op = if self.eat_op("*") {
                Op::Mul
            } else if self.eat_op("/") {
                Op::Div
            } else if self.eat_op("%") {
                Op::Rem
            } else {
                return Ok(left);
            };
            self.operator()?;
            left = Expr::Bin(op, Box::new(left), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        let neg = if self.eat_op("-") {
            true
        } else if self.eat_op("!") {
            false
        } else {
            return self.primary();
        };
        self.operator()?;
        self.enter()?;
        let inner = Box::new(self.unary()?);
        self.leave();
        Ok(if neg {
            Expr::Neg(inner)
        } else {
            Expr::Not(inner)
        })
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let Some(tok) = self.tokens.get(self.at).cloned() else {
            return Err("an expression ends too early".into());
        };
        self.at += 1;
        match tok {
            Tok::Int(n) => Ok(Expr::Lit(Value::Int(n))),
            Tok::Str(s) => Ok(Expr::Lit(Value::Str(s))),
            Tok::Open => {
                self.enter()?;
                let inner = self.or()?;
                self.leave();
                if !self.eat(&Tok::Close) {
                    return Err("a `(` is not closed".into());
                }
                Ok(inner)
            }
            Tok::Name(name) if self.eat(&Tok::Open) => {
                self.operator()?;
                let mut args = Vec::new();
                if self.eat(&Tok::Close) {
                    return Ok(Expr::Call(name, args));
                }
                self.enter()?;
                loop {
                    args.push(self.arg()?);
                    if self.eat(&Tok::Close) {
                        self.leave();
                        return Ok(Expr::Call(name, args));
                    }
                    if !self.eat(&Tok::Comma) {
                        return Err(format!("the call of `{name}` needs `,` or `)`"));
                    }
                }
            }
            Tok::Name(name) => Ok(Expr::Var(name)),
            other => Err(format!("{} is not expected here", describe(&other))),
        }
    }

    fn arg(&mut self) -> Result<Arg, String> {
        if let (Some(Tok::Name(name)), Some(Tok::Assign)) =
            (self.tokens.get(self.at), self.tokens.get(self.at + 1))
        {
            let name = name.clone();
            self.at += 2;
            return Ok(Arg {
                name: Some(name),
                expr: self.or()?,
            });
        }
        Ok(Arg {
            name: None,
            expr: self.or()?,
        })
    }
}

fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Int(n) => format!("`{n}`"),
        Tok::Str(s) => format!("`\"{s}\"`"),
        Tok::Name(name) => format!("`{name}`"),
        Tok::Open => "`(`".into(),
        Tok::Close => "`)`".into(),
        Tok::Comma => "`,`".into(),
        Tok::Assign => "`=`".into(),
        Tok::Op(op) => format!("`{op}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(n: i64) -> Box<Expr> {
        Box::new(Expr::Lit(Value::Int(n)))
    }

    #[test]
    fn deep_and_long_expressions_decline() {
        let nest =
            |open: &str, close: &str, n: usize| format!("{}1{}", open.repeat(n), close.repeat(n));
        // A hundred levels parse, even on a test thread's small stack.
        assert!(parse(&nest("(", ")", 100)).is_ok());
        let err = parse(&nest("(", ")", 101)).unwrap_err();
        assert!(err.contains("100 levels"), "{err}");
        for (open, close) in [("(", ")"), ("-", ""), ("!", ""), ("$f(", ")")] {
            assert!(parse(&nest(open, close, 100_000)).is_err(), "{open}");
        }
        let chain = |n: usize| vec!["1"; n + 1].join(" + ");
        assert!(parse(&chain(1000)).is_ok());
        let err = parse(&chain(1001)).unwrap_err();
        assert!(err.contains("1000 operators"), "{err}");
        assert!(parse(&chain(400_000)).is_err());
    }

    #[test]
    fn precedence_and_calls() {
        assert_eq!(
            parse("1 + 2 * 3").unwrap(),
            Expr::Bin(
                Op::Add,
                lit(1),
                Box::new(Expr::Bin(Op::Mul, lit(2), lit(3)))
            )
        );
        assert_eq!(
            parse("$a == 'x' || !%f($b, $c = 2)").unwrap(),
            Expr::Bin(
                Op::Or,
                Box::new(Expr::Bin(
                    Op::Eq,
                    Box::new(Expr::Var("$a".into())),
                    Box::new(Expr::Lit(Value::Str("x".into())))
                )),
                Box::new(Expr::Not(Box::new(Expr::Call(
                    "%f".into(),
                    vec![
                        Arg {
                            name: None,
                            expr: Expr::Var("$b".into())
                        },
                        Arg {
                            name: Some("$c".into()),
                            expr: Expr::Lit(Value::Int(2))
                        },
                    ]
                ))))
            )
        );
    }

    #[test]
    fn values_add_compare_and_fail() {
        let s = |v: &str| Value::Str(v.into());
        assert_eq!(binary(Op::Add, s("a"), Value::Int(1)), Ok(s("a1")));
        assert_eq!(
            binary(Op::Add, Value::Int(2), Value::Int(3)),
            Ok(Value::Int(5))
        );
        assert_eq!(binary(Op::Eq, Value::Int(1), s("1")), Ok(Value::Int(1)));
        assert_eq!(
            binary(Op::Lt, Value::Int(9), Value::Int(10)),
            Ok(Value::Int(1))
        );
        assert!(binary(Op::Div, Value::Int(1), Value::Int(0)).is_err());
        assert!(binary(Op::Mul, s("a"), Value::Int(2)).is_err());
        assert!(binary(Op::Add, Value::Int(i64::MAX), Value::Int(1)).is_err());
        assert!(parse("{\"a\": 1}").unwrap_err().contains("JSON"));
        assert!(parse("\"open").is_err());
        assert!(parse("1 2").is_err());
        assert!(parse("").is_err());
    }
}
