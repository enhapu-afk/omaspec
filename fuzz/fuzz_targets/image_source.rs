//! Reading a diagram's source back from any PNG or SVG bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use omaspec_core::embed;

fuzz_target!(|bytes: &[u8]| {
    let _ = embed::source_from_png(bytes);
    let _ = embed::source_from_svg(&String::from_utf8_lossy(bytes));
});
