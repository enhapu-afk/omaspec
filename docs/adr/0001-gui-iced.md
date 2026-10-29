# 0001: GUI with iced

Status: accepted, 2026-10-15

## Decision

Omaspec's interface is built with [iced](https://iced.rs) 0.14.

## Why

- Native Wayland through winit, GPU rendering through wgpu with tiny-skia as a
  software fallback. COSMIC is built on it, so Linux desktop use is first-class.
- Multiple windows (`iced::daemon`), needed for the detached preview.
- The Elm architecture (state, message, update, view) keeps the app testable.
  `iced_test` renders frames headless, so screenshot tests run in CI without a
  display (see the editor spike).
- Stable, documented API. That matters because much of the code is written
  together with coding agents.

## Alternatives

- **gpui + gpui-component**: the strongest code editor (Tree-sitter, LSP), but
  a less stable API and thin documentation.
- **egui**: immediate mode, quick to start, looks less finished.
- **Tauri/webview**: would bring back the browser stack the port leaves behind.

## Consequences

The code editor widget is not part of iced; see [0002](0002-editor-widget-scrive.md).
