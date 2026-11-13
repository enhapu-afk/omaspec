//! ZenUML (`zenuml`), read as the sequence diagram it describes: each
//! statement is written as the PlantUML line it stands for, on the line
//! it was written on, and the sequence engine draws them, so a ZenUML
//! diagram looks and plays as every other sequence diagram.
//!
//! What is read: participants (`@Actor Client`, `@Database Orders`, a
//! name alone, `#color`), a group of them (`group Shop { … }`), the
//! starter (`@Starter(Client)`), a call (`Orders.save(order)`, with a
//! result `id = Orders.save(order)`, and a block of the calls it makes,
//! `{ … }`, which activates the callee), a self call (`check()`), a
//! creation (`order = new Order(items)`), an async message (`A->B: text`),
//! `return`, and the fragments `if`/`else if`/`else`, `while`, `for`,
//! `forEach`, `loop`, `par`, `opt`, `try`/`catch`/`finally` and
//! `critical`. Comments (`//`) are left out. Anything else leaves the
//! diagram to mmdr.

use super::Diagram;
use crate::source::Line;

/// A block that is open: a call whose callee is active, or a fragment.
#[derive(Debug, Clone, PartialEq)]
enum Open {
    Call {
        callee: String,
        caller: String,
        result: Option<String>,
        /// A `return` in its block already gave the result back.
        returned: bool,
    },
    Fragment,
    Group,
}

/// The sequence diagram `source` describes; `None` when it is no ZenUML
/// this reader takes.
pub fn from_zenuml(source: &str) -> Option<Diagram> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut open: Vec<Open> = Vec::new();
    let mut starter: Option<String> = None;
    let mut declared: Vec<String> = Vec::new();
    let mut saw_header = false;
    let mut front = false;
    for (no, raw) in source.lines().enumerate() {
        let text = strip_comment(raw).trim();
        // Mermaid's own comments, and its directives, too.
        if text.is_empty() || text.starts_with("%%") {
            continue;
        }
        // Front matter before the header: its title is the diagram's.
        if !saw_header && (front || text == "---") {
            if text == "---" {
                front = !front;
            } else if let Some(title) = text.strip_prefix("title:") {
                out.push((no, format!("title {}", title.trim().trim_matches('"'))));
            }
            continue;
        }
        if !saw_header {
            if text.eq_ignore_ascii_case("zenuml") {
                saw_header = true;
                continue;
            }
            return None;
        }
        // A closing brace, perhaps with what follows it on the line.
        let mut text = text;
        while let Some(rest) = text.strip_prefix('}') {
            let rest = rest.trim();
            if let Some(clause) = clause(rest) {
                // `} else {`, `} catch (e) {`: the same fragment goes on.
                if !matches!(open.last(), Some(Open::Fragment)) {
                    return None;
                }
                out.push((no, format!("else {clause}")));
                text = "";
                break;
            }
            match open.pop()? {
                Open::Call {
                    callee,
                    caller,
                    result,
                    returned,
                } => {
                    if let Some(result) = result.filter(|_| !returned) {
                        out.push((no, back(&callee, &caller, &result)));
                    }
                    out.push((no, format!("deactivate {callee}")));
                }
                Open::Fragment => out.push((no, "end".into())),
                Open::Group => out.push((no, "end box".into())),
            }
            text = rest;
        }
        let text = text.trim_end_matches(';').trim();
        if text.is_empty() {
            continue;
        }
        let caller = || {
            open.iter()
                .rev()
                .find_map(|o| match o {
                    Open::Call { callee, .. } => Some(callee.clone()),
                    _ => None,
                })
                .or_else(|| starter.clone())
        };
        if let Some(title) = text.strip_prefix("title ") {
            out.push((no, format!("title {}", title.trim())));
            continue;
        }
        if let Some(name) = text
            .strip_prefix("@Starter(")
            .or_else(|| text.strip_prefix("@starter("))
        {
            let name = name.trim_end_matches(')').trim().to_owned();
            if !declared.contains(&name) {
                out.push((no, format!("participant {name}")));
                declared.push(name.clone());
            }
            starter = Some(name);
            continue;
        }
        if let Some(rest) = text.strip_prefix('@') {
            let (kind, rest) = rest.split_once(char::is_whitespace)?;
            let (name, color) = named(rest);
            let word = match kind.to_ascii_lowercase().as_str() {
                "actor" => "actor",
                "database" | "dynamodb" | "rds" => "database",
                "boundary" => "boundary",
                "control" => "control",
                "entity" => "entity",
                "queue" | "sqs" | "kinesis" => "queue",
                "collections" => "collections",
                _ => "participant",
            };
            out.push((no, format!("{word} {name}{color}")));
            declared.push(name);
            continue;
        }
        if let Some(rest) = keyword(text, "group") {
            let name = rest.trim().trim_end_matches('{').trim();
            out.push((no, format!("box \"{}\"", name.trim_matches('"'))));
            open.push(Open::Group);
            continue;
        }
        if let Some((kind, label)) = fragment(text) {
            out.push((no, format!("{kind} {label}").trim().to_owned()));
            open.push(Open::Fragment);
            continue;
        }
        if text == "return" || text.starts_with("return ") || text.starts_with("@return") {
            let value = text
                .trim_start_matches("@return")
                .trim_start_matches("return")
                .trim();
            let Some(Open::Call {
                callee,
                caller,
                returned,
                ..
            }) = open
                .iter_mut()
                .rev()
                .find(|o| matches!(o, Open::Call { .. }))
            else {
                return None;
            };
            *returned = true;
            out.push((no, back(callee, caller, value)));
            continue;
        }
        // An async message: `A->B: text`.
        if let Some((from, rest)) = text.split_once("->")
            && let Some((to, message)) = rest.split_once(':')
            && is_name(from.trim())
            && is_name(to.trim())
        {
            out.push((
                no,
                format!("{} ->> {} : {}", from.trim(), to.trim(), message.trim()),
            ));
            continue;
        }
        // A call or a creation, with its result and its block.
        let (result, call) = match text.split_once('=') {
            Some((name, call)) if is_name(name.trim()) && !call.starts_with('=') => {
                (Some(name.trim().to_owned()), call.trim())
            }
            _ => (None, text),
        };
        let block = call.ends_with('{');
        let call = call.trim_end_matches('{').trim();
        let from = caller();
        if let Some(created) = call.strip_prefix("new ") {
            let (name, args) = created.split_once('(').unwrap_or((created, ")"));
            let name = name.trim().to_owned();
            if !is_name(&name) {
                return None;
            }
            let args = args.trim_end_matches(')');
            out.push((no, format!("create {name}")));
            let from = from.unwrap_or_else(|| "[".into());
            out.push((no, calling(&from, &name, &format!("<<create>> {args}"))));
            if let Some(result) = &result {
                out.push((no, back(&name, &from, result)));
            }
            continue;
        }
        let open_paren = call.find('(')?;
        if !call.ends_with(')') {
            return None;
        }
        let target = &call[..open_paren];
        let (callee, method) = match target.split_once('.') {
            Some((callee, method)) => (
                callee.trim().to_owned(),
                format!("{}{}", method, &call[open_paren..]),
            ),
            // A call on itself.
            None => (from.clone()?, call.to_owned()),
        };
        if !is_name(&callee) {
            return None;
        }
        let from_name = from.unwrap_or_else(|| "[".into());
        out.push((no, calling(&from_name, &callee, &method)));
        if block {
            out.push((no, format!("activate {callee}")));
            open.push(Open::Call {
                callee,
                caller: from_name,
                result,
                returned: false,
            });
        } else if let Some(result) = result {
            out.push((no, back(&callee, &from_name, &result)));
        }
    }
    if !saw_header || !open.is_empty() {
        return None;
    }
    let lines: Vec<Line> = out
        .iter()
        .map(|(no, text)| Line {
            no: *no,
            text: text.as_str(),
            indent: 0,
            blank_before: 0,
        })
        .collect();
    super::parse(&lines).ok()
}

/// The fragment a line opens, and what it says: `if (paid) {`.
fn fragment(text: &str) -> Option<(&'static str, String)> {
    let head = text.strip_suffix('{')?.trim();
    let (word, rest) = match head.find('(') {
        Some(at) => (head[..at].trim(), head[at..].trim()),
        None => (head, ""),
    };
    let label = rest
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim()
        .to_owned();
    let kind = match word {
        "if" => "alt",
        "while" | "for" | "forEach" | "foreach" | "loop" => "loop",
        "par" => "par",
        "opt" => "opt",
        "try" => "group try",
        "critical" => "critical",
        _ => return None,
    };
    Some((kind, label))
}

/// What follows `word` at the start of `text` when it is that word, not
/// the start of a name: `group Shop {`, never `groupRepo.find()`.
fn keyword<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word)?;
    rest.chars()
        .next()
        .is_none_or(|c| c.is_whitespace() || matches!(c, '(' | '{' | '"'))
        .then_some(rest)
}

/// What an `else` of a fragment says: `else if (x) {`, `catch (e) {`.
fn clause(text: &str) -> Option<String> {
    let head = text.strip_suffix('{')?.trim();
    let label = |rest: &str| {
        rest.trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim()
            .to_owned()
    };
    if let Some(rest) = keyword(head, "else if") {
        return Some(label(rest));
    }
    if head == "else" {
        return Some(String::new());
    }
    if let Some(rest) = keyword(head, "catch") {
        return Some(format!("catch {}", label(rest)).trim().to_owned());
    }
    if head == "finally" {
        return Some("finally".into());
    }
    None
}

/// A participant's name and its `#color`, written for PlantUML.
fn named(text: &str) -> (String, String) {
    let mut name = String::new();
    let mut color = String::new();
    for word in text.split_whitespace() {
        if word.starts_with('#') {
            color = format!(" {word}");
        } else if word.starts_with("<<") {
            continue;
        } else if name.is_empty() {
            name = word.to_owned();
        }
    }
    (name, color)
}

/// A call from `from` to `to`; from the left edge (`[`) where no
/// participant starts it.
fn calling(from: &str, to: &str, message: &str) -> String {
    let message = message.trim();
    match from {
        "[" => format!("[-> {to} : {message}"),
        from => format!("{from} -> {to} : {message}"),
    }
}

/// What `from` gives back to `to`, dashed.
fn back(from: &str, to: &str, message: &str) -> String {
    let message = message.trim();
    match to {
        "[" => format!("[<-- {from} : {message}"),
        to => format!("{from} --> {to} : {message}"),
    }
}

fn is_name(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// A line without its `//` comment, outside quotes.
fn strip_comment(text: &str) -> &str {
    let mut quoted = false;
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'"' => quoted = !quoted,
            b'/' if !quoted && bytes.get(i + 1) == Some(&b'/') => return &text[..i],
            _ => {}
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::Item;

    const ORDERS: &str = "zenuml\n  title Ordering\n  @Actor Client\n  @Database Orders\n  @Starter(Client)\n  // an order\n  id = Shop.place(items) {\n    check()\n    if (items.ok) {\n      saved = Orders.save(items)\n      return saved\n    } else {\n      Mail.warn()\n    }\n  }\n";

    #[test]
    fn calls_blocks_and_fragments_become_a_sequence() {
        let d = from_zenuml(ORDERS).unwrap();
        let names: Vec<&str> = d.participants.iter().map(|p| p.code.as_str()).collect();
        assert!(names.starts_with(&["Client", "Orders"]), "{names:?}");
        assert!(
            names.contains(&"Shop") && names.contains(&"Mail"),
            "{names:?}"
        );
        let svg = super::super::draw(&d).unwrap();
        for words in [
            "place(items)",
            "check()",
            "save(items)",
            "warn()",
            ">saved<",
            "items.ok",
        ] {
            assert!(svg.contains(words), "{words}: {svg}");
        }
        // The `return` in the block gives the result back; its end does
        // not again.
        assert!(!svg.contains(">id<"), "{svg}");
        let plain =
            from_zenuml("zenuml\n  @Starter(A)\n  id = B.make() {\n    C.help()\n  }\n").unwrap();
        assert!(super::super::draw(&plain).unwrap().contains(">id<"));
        // Each item stands on its own line of the ZenUML.
        let lines: Vec<usize> = d
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Message(m) => Some(m.line),
                _ => None,
            })
            .collect();
        assert!(lines.contains(&6) && lines.contains(&9), "{lines:?}");
        // A participant whose name starts with a keyword is called.
        let group = from_zenuml("zenuml\n  groupRepo.find() {\n    Db.read()\n  }\n").unwrap();
        let names: Vec<&str> = group.participants.iter().map(|p| p.code.as_str()).collect();
        assert!(names.contains(&"groupRepo"), "{names:?}");
        assert_eq!(keyword("group Shop {", "group"), Some(" Shop {"));
        assert_eq!(keyword("catch(e) {", "catch"), Some("(e) {"));
        assert_eq!(keyword("catchAll() {", "catch"), None);
    }

    #[test]
    fn what_it_does_not_read_is_left_to_mmdr() {
        assert!(from_zenuml("zenuml\n  A.b(\n").is_none());
        assert!(
            from_zenuml("zenuml\n  A.b() {\n").is_none(),
            "an open block"
        );
        assert!(from_zenuml("sequenceDiagram\n  A->>B: hi\n").is_none());
    }
}
