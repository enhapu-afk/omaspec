//! Text logic shared by the Omaspec editor and CLI: language detection,
//! diagram kinds, lint, outline, formatting, line diffs, diagram source
//! embedded in images, finding the source line of a drawn label, skin
//! packs, lines by hand, the project file, and the diagrams of a Markdown
//! document.
//!
//! No I/O and no GUI: everything here works on strings and bytes. Regexes
//! come from the `regex` crate, which runs in linear time, so no input can
//! make a check hang.

mod faint;
mod text;

pub mod diff;
pub mod embed;
pub mod format;
pub mod hand;
pub mod kind;
pub mod lang;
pub mod lint;
pub mod locate;
pub mod markdown;
pub mod outline;
pub mod project;
pub mod recolor;
pub mod settings;
pub mod skin;

pub use kind::{DiagramKind, Kind, classify};
pub use lang::{Language, detect};
