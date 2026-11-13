//! Arbitrary text through every identifier's parser.
//!
//! - Parsing never panics.
//! - Whatever parses writes back as the text it was read from, so a value
//!   has one spelling. The one exception is a device ID in its bare form,
//!   which may be in either case and writes back in lower case.
#![no_main]

use libfuzzer_sys::fuzz_target;
use quiddity_identity::{DeviceId, DeviceKey, GraphId, NodeId};

fuzz_target!(|text: &str| {
    if let Ok(id) = text.parse::<GraphId>() {
        assert_eq!(id.to_string(), text);
    }
    if let Ok(id) = text.parse::<NodeId>() {
        assert_eq!(id.to_string(), text);
    }
    if let Ok(id) = DeviceId::parse(text) {
        assert!(id.to_string() == text || id.to_bare() == text.to_ascii_lowercase());
    }
    if let Ok(key) = DeviceKey::from_token(text) {
        assert_eq!(&**key.to_token(), text);
    }
});
