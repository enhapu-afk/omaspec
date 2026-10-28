# 0008: An MCP server over standard input and output

Status: accepted, 2026-10-21

## Decision

`omagram mcp` is a Model Context Protocol server. An agent's client
starts it as a program and speaks JSON-RPC 2.0 with it over standard input
and output, one message per line. It offers what the command line does, as
tools an agent calls while it writes a specification:

| Tool      | Does                                                                 | Like             |
| --------- | -------------------------------------------------------------------- | ---------------- |
| `lint`    | the lint's findings and the specification checks, for a diagram or every diagram in a folder | `omagram lint --json` |
| `trace`   | a recorded trace (JSON Lines) held against the state machines of its project | `omagram trace --json` |
| `inspect` | language, kind, findings, outline, and whether Omagram draws it    | `omagram inspect` |
| `outline` | the structure of a diagram                                           | `omagram outline --json` |
| `diff`    | what changed in the behavior between two versions, or against a git revision | `omagram diff --json` |
| `convert` | the diagram in the other language, or why not ([0009](0009-diff-and-convert.md)) | `omagram convert --json` |
| `format`  | the diagram re-indented; with `write`, the file rewritten            | `omagram fmt`  |
| `render`  | the picture: a PNG the agent sees, SVG text, or a file of any export format | `omagram render` |

A diagram is given as `path` (a file) or as `code` (its text). A path
also names the project, as on the command line: the state machines a
sequence diagram or a trace is checked against are those of the file's
folder, or of `project`. Lines are one-based, as in `--json`. Relative
paths are taken from the folder the server was started in.

## How

- Written by hand on `serde_json`, without an MCP library or an async
  runtime. The protocol a tools-only server needs is small: `initialize`,
  `ping`, `tools/list`, `tools/call`, and the notifications it may ignore.
  `rmcp` would bring tokio and much more for that, against the goal of a
  small, independent, auditable program.
- No network: the transport is the two pipes the client opened. Nothing in
  `deny.toml` changes.
- One request at a time, in order. Every check takes milliseconds; a render
  through the Java fallback takes longer and still runs confined
  ([0005](0005-offline-no-telemetry.md)).
- A problem with the arguments (no diagram, an unreadable file, a format the
  tool does not know) is a tool result marked `isError`, so the agent reads
  it and tries again. Only what is no request at all, an unknown method, or
  an unknown tool is a JSON-RPC error.
- Results come as JSON text and as `structuredContent`, which newer
  clients read directly.
- Protocol versions 2024-11-05 to 2025-11-25 are understood; the server
  answers in the client's version when it knows it, else in its newest.
- A tool that panics answers with an error; the server stays up.
- Nothing but protocol messages goes to standard output.

## Why

Agents write more and more of the diagrams in a specification. Through the
command line they can already lint and render, but they parse text and
guess at flags. As MCP tools, the checks come with their schema and their
description into the agent's context, and a rendered PNG comes back as an
image the agent can look at before it shows the diagram to a person.

## Not covered

- The server reads and writes only inside the folder it was started in.
  Every path a call gives (`path`, `old_path`, `project`, `output`) is taken
  from there and refused when it leaves it, leads through a hidden file or
  folder (`.ssh`, `.git`, `.bashrc`) or is a link out of it. An agent
  steered by text it read, a README or a diagram, could otherwise give
  `~/.ssh/id_ed25519` to `format` and get the key back, or write a shell
  start file through `render` with `output`. Within the folder it writes
  where a call says so (`format` with `write`, `render` and `convert` with
  `output`), and the client decides which calls to allow.
- A PNG returned to look at has at most 25 megapixels; a larger picture
  goes to a file through `output`.
- No resources, prompts or subscriptions; no playing a specification step
  by step yet.
