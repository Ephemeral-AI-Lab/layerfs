//! The product's WTF-8 codec, included from its source file for the test.
//! Gated like every Windows-only target: the fast lane builds this file on
//! Linux too, and the codec's os::windows imports exist only there.
#![cfg(target_os = "windows")]
#[path = "../src/import/host_wtf8_inner.rs"]
mod wtf8;

pub fn encode(value: &std::ffi::OsStr) -> Vec<u8> {
    wtf8::encode(value)
}
pub fn decode(bytes: &[u8]) -> std::ffi::OsString {
    wtf8::decode(bytes)
}
