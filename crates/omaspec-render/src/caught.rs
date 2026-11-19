//! Panics the engine catches and answers for: mmdr is young and has
//! panicking code paths, and a diagram it fails on is said to have failed,
//! or a line it fails on read alone is left to the count. Rust's panic
//! hook would still print each one, on every redraw while the diagram is
//! written. [`quiet`] tells a program's hook that the panic now is such a
//! one ([`std::panic::set_hook`]); the engine itself sets no hook.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

thread_local! {
    static QUIET: Cell<bool> = const { Cell::new(false) };
}

/// `f`, whose panic is caught and answered for by the caller.
pub(crate) fn caught<T>(f: impl FnOnce() -> T) -> std::thread::Result<T> {
    let before = QUIET.replace(true);
    let out = catch_unwind(AssertUnwindSafe(f));
    QUIET.set(before);
    out
}

/// Whether a panic on this thread now is one the engine catches and
/// answers for, which a program's panic hook can leave unsaid.
pub fn quiet() -> bool {
    QUIET.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caught_panic_is_quiet_while_it_is_caught() {
        assert!(!quiet());
        let seen = caught(quiet);
        assert_eq!(seen.ok(), Some(true));
        assert!(caught(|| panic!("mmdr")).is_err());
        assert!(!quiet());
    }
}
