//! How a `BYTES` value is written: standard, padded base64 (RFC 4648).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub fn encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// The bytes `text` encodes, or `None` if it isn't base64.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    STANDARD.decode(text).ok()
}
