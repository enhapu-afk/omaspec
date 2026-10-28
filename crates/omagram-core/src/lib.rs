//! Text logic shared by the Omagram editor and CLI: language detection,
//! diagram kinds, lint, outline, formatting, line diffs, diagram source
//! embedded in images, finding the source line of a drawn label, and skin
//! packs.
//!
//! No I/O and no GUI: everything here works on strings and bytes. Regexes
//! come from the `regex` crate, which runs in linear time, so no input can
//! make a check hang.

mod text;

pub mod diff;
pub mod embed;
pub mod format;
pub mod kind;
pub mod lang;
pub mod lint;
pub mod locate;
pub mod outline;
pub mod skin;

pub use kind::{DiagramKind, Kind, classify};
pub use lang::{Language, detect};
