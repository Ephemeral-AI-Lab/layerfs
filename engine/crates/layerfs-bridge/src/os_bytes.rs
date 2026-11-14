//! Path bytes for the grammar: unix reads OsStr as bytes; Windows goes
//! through UTF-16 to UTF-8 (lossy), because the persisted path grammar
//! is valid UTF-8 - PathName enforces it.
/// The bytes of text as the grammar persists them.
pub fn as_bytes(text: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        text.as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = text.encode_wide().collect();
        String::from_utf16_lossy(&wide).into_bytes()
    }
}

/// The path for bytes (the grammar is valid UTF-8).
#[cfg(windows)]
pub fn from_bytes(bytes: &[u8]) -> std::ffi::OsString {
    std::ffi::OsString::from(String::from_utf8_lossy(bytes).into_owned())
}

/// The bytes of an owned path.
#[cfg(windows)]
pub fn into_vec(text: std::ffi::OsString) -> Vec<u8> {
    text.into_string().map(|s| s.into_bytes()).unwrap_or_default()
}

/// The path for bytes (unix: raw bytes; the grammar is UTF-8 on Windows).
pub fn os_string(bytes: &[u8]) -> std::ffi::OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(bytes.to_vec())
    }
    #[cfg(windows)]
    {
        std::ffi::OsString::from(String::from_utf8_lossy(bytes).into_owned())
    }
}
