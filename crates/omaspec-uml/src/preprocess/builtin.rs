//! The `%` built-ins that need nothing from outside the diagram. Those that
//! read the clock, the environment, files or chance (`%date`, `%getenv`,
//! `%load_json`, `%random`, …) are not here, so they decline.

use super::expr::Value;

/// The pure built-ins, by name.
const NAMES: [&str; 17] = [
    "%strlen", "%substr", "%strpos", "%upper", "%lower", "%intval", "%string", "%boolval", "%not",
    "%true", "%false", "%chr", "%ord", "%dec2hex", "%hex2dec", "%newline", "%size",
];

/// Whether `name` is a pure built-in.
pub fn knows(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Runs the pure built-in `name`; the error says what is wrong.
pub fn call(name: &str, args: &[Value]) -> Result<Value, String> {
    let arity = |counts: &[usize]| {
        if counts.contains(&args.len()) {
            Ok(())
        } else {
            let wanted: Vec<String> = counts.iter().map(usize::to_string).collect();
            Err(format!(
                "`{name}` takes {} argument(s), not {}",
                wanted.join(" or "),
                args.len()
            ))
        }
    };
    let number = |value: &Value| {
        value
            .number()
            .ok_or_else(|| format!("`{name}` needs a number, not `{value}`"))
    };
    Ok(match name {
        "%strlen" | "%size" => {
            arity(&[1])?;
            Value::Int(count(&args[0].to_string()))
        }
        "%substr" => {
            arity(&[2, 3])?;
            let text = args[0].to_string();
            let start = usize::try_from(number(&args[1])?).unwrap_or(0);
            let chars = text.chars().skip(start);
            Value::Str(match args.get(2) {
                Some(len) => chars
                    .take(usize::try_from(number(len)?).unwrap_or(0))
                    .collect(),
                None => chars.collect(),
            })
        }
        "%strpos" => {
            arity(&[2])?;
            let (text, part) = (args[0].to_string(), args[1].to_string());
            Value::Int(text.find(&part).map_or(-1, |at| count(&text[..at])))
        }
        "%upper" => {
            arity(&[1])?;
            Value::Str(args[0].to_string().to_uppercase())
        }
        "%lower" => {
            arity(&[1])?;
            Value::Str(args[0].to_string().to_lowercase())
        }
        "%intval" => {
            arity(&[1])?;
            Value::Int(number(&args[0])?)
        }
        "%string" => {
            arity(&[1])?;
            Value::Str(args[0].to_string())
        }
        "%boolval" => {
            arity(&[1])?;
            Value::from_bool(match &args[0] {
                Value::Int(n) => *n != 0,
                Value::Str(s) => {
                    s.eq_ignore_ascii_case("true") || s.trim().parse::<i64>().is_ok_and(|n| n != 0)
                }
            })
        }
        "%not" => {
            arity(&[1])?;
            Value::from_bool(!args[0].truthy())
        }
        "%true" => {
            arity(&[0])?;
            Value::Int(1)
        }
        "%false" => {
            arity(&[0])?;
            Value::Int(0)
        }
        "%chr" => {
            arity(&[1])?;
            let code = number(&args[0])?;
            let c = u32::try_from(code)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| format!("`{code}` is no character"))?;
            Value::Str(c.to_string())
        }
        "%ord" => {
            arity(&[1])?;
            let c = args[0]
                .to_string()
                .chars()
                .next()
                .ok_or_else(|| "`%ord` needs a character".to_owned())?;
            Value::Int(i64::from(u32::from(c)))
        }
        "%dec2hex" => {
            arity(&[1])?;
            Value::Str(format!("{:x}", number(&args[0])?))
        }
        "%hex2dec" => {
            arity(&[1])?;
            let text = args[0].to_string();
            Value::Int(
                i64::from_str_radix(text.trim(), 16)
                    .map_err(|_| format!("`{text}` is no hexadecimal number"))?,
            )
        }
        "%newline" => {
            arity(&[0])?;
            Value::Str("\n".into())
        }
        _ => return Err(format!("the built-in `{name}` is not supported yet")),
    })
}

fn count(text: &str) -> i64 {
    i64::try_from(text.chars().count()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Value {
        Value::Str(text.into())
    }

    #[test]
    fn strings_numbers_and_arity() {
        assert_eq!(call("%strlen", &[s("héllo")]), Ok(Value::Int(5)));
        assert_eq!(
            call("%substr", &[s("abcdef"), Value::Int(2), Value::Int(3)]),
            Ok(s("cde"))
        );
        assert_eq!(call("%substr", &[s("abc"), Value::Int(9)]), Ok(s("")));
        assert_eq!(call("%strpos", &[s("äbc"), s("c")]), Ok(Value::Int(2)));
        assert_eq!(call("%strpos", &[s("abc"), s("x")]), Ok(Value::Int(-1)));
        assert_eq!(call("%upper", &[s("a")]), Ok(s("A")));
        assert_eq!(call("%intval", &[s(" 42 ")]), Ok(Value::Int(42)));
        assert!(call("%intval", &[s("x")]).is_err());
        assert_eq!(call("%boolval", &[s("TRUE")]), Ok(Value::Int(1)));
        assert_eq!(call("%dec2hex", &[Value::Int(255)]), Ok(s("ff")));
        assert_eq!(call("%hex2dec", &[s("ff")]), Ok(Value::Int(255)));
        assert_eq!(call("%chr", &[Value::Int(65)]), Ok(s("A")));
        assert_eq!(call("%ord", &[s("A")]), Ok(Value::Int(65)));
        assert!(call("%upper", &[]).is_err());
        assert!(call("%date", &[]).is_err());
        assert!(knows("%newline") && !knows("%getenv"));
    }
}
