//! The bytes face of path grammar on Windows: the grammar's paths are
//! valid UTF-8 (PathName::from_utf8 enforces it), so the bytes face is
//! UTF-8 bytes. One shared home for the twin conversions.
#[cfg(windows)]
use std::ffi::{OsStr, OsString};

/// The path for bytes (valid UTF-8; lossy for the impossible rest).
#[cfg(windows)]
pub fn from_bytes(bytes: &[u8]) -> OsString {
    OsString::from(String::from_utf8_lossy(bytes).into_owned())
}

/// The bytes of text (UTF-8; empty for the impossible non-UTF-8).
#[cfg(windows)]
pub fn as_bytes(text: &OsStr) -> &[u8] {
    text.to_str().map(|s| s.as_bytes()).unwrap_or(&[])
}

/// The bytes of an owned path (UTF-8; empty for the impossible rest).
#[cfg(windows)]
pub fn into_vec(text: OsString) -> Vec<u8> {
    text.into_string().map(|s| s.into_bytes()).unwrap_or_default()
}
