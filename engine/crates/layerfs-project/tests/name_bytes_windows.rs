//! The product's WTF-8 codec, included from its source file for the test.
#[path = "../src/import/host_wtf8_inner.rs"]
mod wtf8;

pub fn encode(value: &std::ffi::OsStr) -> Vec<u8> {
    wtf8::encode(value)
}
pub fn decode(bytes: &[u8]) -> std::ffi::OsString {
    wtf8::decode(bytes)
}
