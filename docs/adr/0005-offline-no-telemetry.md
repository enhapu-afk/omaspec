# 0005: Offline, no telemetry

Status: accepted, 2026-10-16

## Decision

Omaspec never uses the network and reports nothing about its use: no update
checks, no crash reports, no usage counts, no fonts or engines fetched at run
time. Diagrams stay on the machine.

## What Omaspec talks to

All of it local:

- Wayland or X11 (the windows), the GPU driver.
- inotify, through `notify`: the open files, the folder, the Omarchy theme,
  the fontconfig folder.
- Programs it starts: `fc-match` (the font), `wl-copy` (the clipboard),
  `omarchy-notification-send` or `notify-send` (notifications), `git log`
  and `git show` (Compare), and the Java `plantuml` if installed.
- Files: the diagrams, its session in `$XDG_STATE_HOME/omaspec`, the
  Omarchy theme in `$XDG_STATE_HOME/omarchy/current`.

The D-Bus theme detection of iced (`linux-theme-detection`) is switched off:
Omaspec reads the Omarchy theme and needs no D-Bus client.

## How it is held

- `deny.toml` bans HTTP, TLS and telemetry crates (`reqwest`, `hyper`,
  `ureq`, `rustls`, `openssl`, `sentry`, `opentelemetry`, …); `cargo deny`
  runs in CI, so no dependency can bring one in unnoticed.
- The Java PlantUML is the one engine that could reach out (`!include` of
  URLs, Java's own networking). It runs with PlantUML's
  `PLANTUML_SECURITY_PROFILE=SANDBOX` and, through `omaspec __sandbox`,
  confined with Landlock: it reads only the system's folders (`/usr`,
  `/etc`, `/opt`, `/sys`), its own `/proc` entry, the folder of the
  `plantuml` command, the JDK of `JAVA_HOME` and of `PATH` and the user's
  fonts, so neither `~/.ssh` nor another process's environment; it writes
  only in a scratch folder of its own; no TCP, no abstract Unix sockets
  and no signals outside (Linux 6.7 and newer for TCP, 6.12 for the last
  two; older kernels get what they support). It starts with a small part
  of Omaspec's environment (where programs, Java and fonts are, the
  language), so no token reaches it, confined or not. A CLI test checks
  that it can neither write outside, connect, read outside nor see the
  environment. `OMASPEC_SANDBOX=off` turns the confinement off for
  debugging. Confined, it may hold at most 4,096 files open: a process
  Java starts (Graphviz) cannot list its own descriptors there, so Java
  closes every one up to the limit, which with Omarchy's 524,288 cost
  160 ms a class diagram. Tried with PlantUML 1.2026.8 on OpenJDK 27 and
  Graphviz 16.1: diagrams, Graphviz layouts and errors come back
  confined, an `!include` of `/etc/passwd` is refused.
- Exports never read files an SVG points to (see Phase 2), so an image
  cannot pull local files into what is shared.
- The native engine's `!include` reads local files only, inside the
  project folder and outside hidden folders; a URL or PlantUML's standard
  library declines.

## Not covered

- UDP is outside Landlock's network rules; PlantUML's SANDBOX profile is
  the fence there.
- The programs Omaspec starts besides PlantUML run unconfined; they are
  the user's own system tools. `git` runs with the settings that would
  start other programs switched off (signature checks, a file system
  monitor, a pager, external diffs and text conversions), since a
  repository's `.git/config` comes with a folder someone hands over.
- A PlantUML or a Java installed somewhere else in the home folder (a
  `java` that is a version manager's shim, a `plantuml` script that starts
  a jar from another folder) cannot be read inside the confinement;
  `JAVA_HOME` or `OMASPEC_SANDBOX=off` helps there.
