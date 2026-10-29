//! `omaspec mcp`: a Model Context Protocol server over standard input and
//! output (`docs/adr/0008-mcp-server.md`). JSON-RPC 2.0, one message per
//! line, one request at a time; the tools are in [`tools`]. Nothing but
//! protocol messages goes to standard output.

mod tools;

use std::io::{self, BufRead, Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::ExitCode;

use anyhow::Result;
use serde_json::{Map, Value, json};

/// The protocol versions this server speaks, newest first.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// A message may hold at most this many bytes: a diagram of 16 MB, escaped.
const MAX_MESSAGE: u64 = 64 << 20;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// What an agent is told about the server when it connects.
const INSTRUCTIONS: &str = "Omaspec checks, formats and draws PlantUML and Mermaid \
diagrams, offline. State diagrams are checked as state machines; sequence diagrams and \
recorded traces against the state machines of their project folder. Run `lint` after \
changing a diagram, `diff` to see what the change did to its behavior, and `render` to \
look at it; `convert` writes it in the other language. Lines are one-based.";

/// Serves standard input until it ends.
pub fn run() -> Result<ExitCode> {
    let mut server = Server::default();
    serve(io::stdin().lock(), &mut io::stdout().lock(), &mut server)?;
    Ok(ExitCode::SUCCESS)
}

/// Answers each message read from `input` on `output`, until `input` ends.
fn serve(mut input: impl BufRead, output: &mut impl Write, server: &mut Server) -> io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = (&mut input)
            .take(MAX_MESSAGE)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(());
        }
        if line.last() != Some(&b'\n') && read as u64 == MAX_MESSAGE {
            // Too long: the rest of it is dropped unread.
            input.skip_until(b'\n')?;
            send(
                output,
                &error(Value::Null, INVALID_REQUEST, "message larger than 64 MB"),
            )?;
            continue;
        }
        let text = String::from_utf8_lossy(&line);
        if text.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle(text.trim()) {
            send(output, &reply)?;
        }
    }
}

fn send(output: &mut impl Write, message: &Value) -> io::Result<()> {
    // Compact JSON has no line breaks of its own.
    serde_json::to_writer(&mut *output, message)?;
    output.write_all(b"\n")?;
    output.flush()
}

#[derive(Default)]
struct Server {
    tools: tools::Tools,
}

impl Server {
    /// The reply to one line, if it calls for one.
    fn handle(&mut self, text: &str) -> Option<Value> {
        let message: Value = match serde_json::from_str(text) {
            Ok(message) => message,
            Err(err) => return Some(error(Value::Null, PARSE_ERROR, &format!("not JSON: {err}"))),
        };
        match message {
            // A batch, which protocol version 2025-03-26 allowed.
            Value::Array(batch) if !batch.is_empty() => {
                let replies: Vec<Value> = batch.into_iter().filter_map(|m| self.one(m)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            message => self.one(message),
        }
    }

    fn one(&mut self, message: Value) -> Option<Value> {
        let Value::Object(message) = message else {
            return Some(error(
                Value::Null,
                INVALID_REQUEST,
                "not a JSON-RPC message",
            ));
        };
        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response: this server asks nothing, so there is nothing to do.
            let response = message.contains_key("result") || message.contains_key("error");
            return (!response).then(|| {
                let id = id.unwrap_or(Value::Null);
                error(id, INVALID_REQUEST, "a request needs a method")
            });
        };
        // A notification (`notifications/initialized`, `…/cancelled`) asks
        // for no answer.
        let id = id?;
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => Ok(initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::definitions() })),
            "tools/call" => self.call(&params),
            _ => Err((METHOD_NOT_FOUND, format!("no method {method}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error(id, code, &message),
        })
    }

    fn call(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return Err((INVALID_PARAMS, "tools/call needs the tool's name".into()));
        };
        if !tools::exists(name) {
            return Err((INVALID_PARAMS, format!("no tool {name}")));
        }
        let empty = Map::new();
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => &empty,
            Some(Value::Object(arguments)) => arguments,
            Some(_) => {
                return Ok(tools::Answer::failed("the arguments must be an object").into_json());
            }
        };
        // A panic answers this call and leaves the server up; the hook has
        // printed it to standard error.
        let outcome = catch_unwind(AssertUnwindSafe(|| self.tools.call(name, arguments)));
        Ok(match outcome {
            Ok(Ok(answer)) => answer,
            Ok(Err(err)) => tools::Answer::failed(&format!("{err:#}")),
            Err(_) => tools::Answer::failed("Omaspec failed on this call; it is a bug"),
        }
        .into_json())
    }
}

/// The server's side of the handshake: the client's protocol version when
/// this server knows it, else its newest.
fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|asked| VERSIONS.contains(asked))
        .unwrap_or(VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "omaspec",
            "title": "Omaspec",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": INSTRUCTIONS,
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(server: &mut Server, message: Value) -> Value {
        server.handle(&message.to_string()).unwrap()
    }

    #[test]
    fn shakes_hands_in_the_clients_version_or_its_newest() {
        let mut server = Server::default();
        let reply = ask(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}
            }}),
        );
        assert_eq!(reply["id"], 1);
        assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(reply["result"]["serverInfo"]["name"], "omaspec");
        assert!(reply["result"]["capabilities"]["tools"].is_object());
        let newer = ask(
            &mut server,
            json!({"jsonrpc": "2.0", "id": "a", "method": "initialize", "params": {"protocolVersion": "2099-01-01"}}),
        );
        assert_eq!(newer["id"], "a");
        assert_eq!(newer["result"]["protocolVersion"], VERSIONS[0]);
    }

    #[test]
    fn answers_requests_and_leaves_notifications_alone() {
        let mut server = Server::default();
        let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert!(server.handle(&initialized.to_string()).is_none());
        let pong = ask(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}),
        );
        assert_eq!(pong["result"], json!({}));
        let listed = ask(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
        );
        let tools = listed["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            [
                "lint", "trace", "inspect", "outline", "diff", "convert", "format", "render"
            ]
        );
        for tool in tools {
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
            assert!(tool["description"].as_str().unwrap().len() > 40, "{tool}");
        }
        // A response to a request the server never sent.
        assert!(
            server
                .handle(r#"{"jsonrpc":"2.0","id":9,"result":{}}"#)
                .is_none()
        );
    }

    #[test]
    fn says_what_is_wrong_with_a_message() {
        let mut server = Server::default();
        let code = |reply: &Value| reply["error"]["code"].as_i64().unwrap();
        assert_eq!(code(&server.handle("{not json").unwrap()), PARSE_ERROR);
        assert_eq!(code(&server.handle("42").unwrap()), INVALID_REQUEST);
        let unknown = ask(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 4, "method": "resources/list"}),
        );
        assert_eq!(code(&unknown), METHOD_NOT_FOUND);
        let no_tool = ask(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "draw"}}),
        );
        assert_eq!(code(&no_tool), INVALID_PARAMS);
        assert_eq!(no_tool["id"], 5);
    }

    #[test]
    fn a_bad_argument_is_a_tool_error_the_agent_can_read() {
        let mut server = Server::default();
        let call = |server: &mut Server, arguments: Value| {
            ask(
                server,
                json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call",
                    "params": {"name": "lint", "arguments": arguments}}),
            )["result"]
                .clone()
        };
        for (arguments, says) in [
            (json!({}), "give the diagram"),
            (json!({"code": "a", "path": "b"}), "not both"),
            (json!({"file": "x.puml"}), "no argument `file`"),
            (json!({"code": 3}), "must be a string"),
            (
                json!({"code": "@startuml\n@enduml", "lang": "dot"}),
                "plantuml or mermaid",
            ),
            (json!({"path": "/no/such/file.puml"}), "/no/such/file.puml"),
        ] {
            let result = call(&mut server, arguments.clone());
            assert_eq!(result["isError"], true, "{arguments}");
            let text = result["content"][0]["text"].as_str().unwrap();
            assert!(text.contains(says), "{arguments}: {text}");
        }
    }

    #[test]
    fn serves_one_message_per_line() {
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
            "\n\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            r#"[{"jsonrpc":"2.0","id":2,"method":"ping"},{"jsonrpc":"2.0","id":3,"method":"ping"}]"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#,
        );
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output, &mut Server::default()).unwrap();
        let lines: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["id"], 1);
        assert_eq!(lines[1][1]["id"], 3);
        assert_eq!(lines[2]["id"], 4);
    }
}
