# 0017: A project is a folder with a project file

Status: accepted, 2026-11-01. Comes before "Diagrams that link" in the
roadmap's [Next](../../ROADMAP.md#next), which needs it. Decision 5 is
replaced by [0018](0018-diagrams-that-link.md): the first diagram is now
a map of the project, beside the first state machine.

## Context

A diagram already has a project folder. Its scenarios are checked against
the state machines in it, its `!include`s stay inside it, and its traces
are found there. But that folder was a guess: the sidebar's folder when
the diagram lay inside it, else the diagram's own folder. Open
`orders/checkout.puml` on its own, or run `omaspec lint` on it, and the
project shrank to `orders/`. The editor and the command line could judge
the same file differently. Diagrams that link, from the whole system down
to a single step, need a project that is the same wherever a diagram is
opened from, and a diagram at its top. Nothing started a project either.

## Decision

1. **A project is a folder of ordinary diagrams.** `.puml` and `.mmd`
   files as before, in any folders below it. No format holds them, so git,
   other tools and agents see what they saw.
2. **`omaspec-project.toml` in the folder marks it.** The nearest folder
   that holds the file, the diagram's own folder or one above it, is the
   project, for the editor, the preview, every command and the MCP server.
   Without one, the project is what it was. The home folder and the file
   system's root never count, so a file left there does not open every
   diagram's `!include` to all of the home folder. `omaspec.toml` already
   adjusts a theme ([`omaspec-theme`](../../crates/omaspec-theme/src/lib.rs)),
   hence the longer name.

   ```toml
   # An Omaspec project: the diagrams in this folder and below it.
   name = "shop"
   main = "system.puml"
   ```

   The file is a small part of TOML: comments and two strings. `name` is
   what the project is called (else the folder's name) and heads the
   sidebar; `main` is the diagram of the whole system, a path inside the
   project. Any other key, a table, a value that is not a string or a
   `main` that leaves the folder is an error that names its line. Only the
   file's presence makes the root, so an error never moves it.
3. **The structure is in the diagrams, by name.** The file lists no
   diagrams and no hierarchy. A diagram is found by its name, as a
   participant finds its state machine today: the name after `@startuml`,
   the title or the file name, letters and digits only, case aside. A
   project file of two lines stays right as diagrams come and go.
4. **New project starts one.** *New project* in the command list asks for
   the language, PlantUML or Mermaid, and whether to start a git
   repository, then for the folder (beside the current project, not in
   it). `omaspec new DIR [--lang mermaid] [--git] [--json]` does the same.
   The folder is created if it is not there, and a folder that is there
   keeps its files: the first diagram, `system.puml` or `system.mmd`, is
   written only when no file has its name, then the project file. A folder
   that is a project already, the home folder and the root are refused.
   The editor shows the folder in the sidebar and opens the first diagram.
5. **The first diagram is the whole system** as a small state machine
   titled with the project's name, clean for lint and `fmt`, which the
   writer replaces.
6. **git is asked for, not assumed.** `git init` runs only on request, and
   not when the folder is inside a repository already, which would make a
   repository in a repository. Without git, the project is made and the
   failure is said (`omaspec new` exits with 1).

## Why

The project file works as `Cargo.toml` and `.git` do: found from anywhere
below it, so the project of a diagram no longer depends on how it was
opened. A convention without a file (the folder in the sidebar, a
`system.puml` at the top) cannot be found from a diagram opened on its own.
Listing the hierarchy in the file would say twice what the diagrams say,
and the two would drift; names already tie a scenario to its machines.

## Consequences

- `omaspec_core::project` reads and writes the file and makes the first
  diagram; `omaspec_render::project_root` finds the root and
  `omaspec_render::Place` takes it for `!include`; the app's `project`
  module makes a project for the editor and `omaspec new`.
- `--project` still overrides the project for `lint`, `trace`, `diff` and
  `inspect`; without it the default is the file's project, else its own
  folder. A folder given to the MCP server's `lint` is checked against
  the project it is in.
- The MCP server keeps to the folder it runs in
  ([0008](0008-mcp-server.md)): a project whose root lies above that folder
  ends there, for state machines and for `!include`.
- A sidebar folder inside a project shows that folder; checks, plays and
  includes use the whole project.
- Not now: no project templates beyond the first diagram, no settings in
  the file, no `new_project` tool for the MCP server. (`main` was used only
  for the first tab; [0018](0018-diagrams-that-link.md) makes it the map
  that also shows the diagrams on no box.)
