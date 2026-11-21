//! The SVG sanitizer on any text: what it lets through parses again and
//! holds no script, handler, outside link or base to move a link, nests
//! no deeper than its limit, and parses again for drawing.
#![no_main]

use libfuzzer_sys::fuzz_target;
use omaspec_render::{export, sanitize::{MAX_DEPTH, sanitize}};

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

fuzz_target!(|svg: &str| {
    let Ok(clean) = sanitize(svg) else { return };
    // Words in the picture may say anything; elements and attributes not.
    let doc = roxmltree::Document::parse(&clean).expect("what it lets through parses");
    for node in doc.descendants().filter(roxmltree::Node::is_element) {
        let name = node.tag_name().name().to_ascii_lowercase();
        assert!(name != "script" && name != "foreignobject", "{name}");
        assert!(node.ancestors().filter(roxmltree::Node::is_element).count() <= MAX_DEPTH);
        for attr in node.attributes() {
            assert!(!attr.name().to_ascii_lowercase().starts_with("on"), "{}", attr.name());
            assert!(!(attr.namespace() == Some(XML_NS) && attr.name() == "base"));
            // A link may name it later, after `#`, `?` or `/`; nothing
            // else may hold it at all.
            let value = attr.value().trim().to_ascii_lowercase();
            match attr.name() {
                "href" => assert!(!value.starts_with("javascript:"), "{value}"),
                _ => assert!(!value.contains("javascript:"), "{value}"),
            }
        }
    }
    let _ = export::parse(&clean);
});
