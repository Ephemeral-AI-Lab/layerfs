//! Path bytes for the import surface: unix reads OsStr as bytes; Windows
//! goes through UTF-16 to UTF-8 (lossy on unpaired surrogates), because the
//! persisted path grammar is valid UTF-8 - PathName enforces it.
pub(super) fn as_bytes(text: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        text.as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide = text.encode_wide().collect::<Vec<u16>>();
        String::from_utf16_lossy(&wide).into_bytes()
    }
}

pub(super) fn os_string(bytes: &[u8]) -> std::ffi::OsString {
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(bytes.to_vec())
    }
    #[cfg(windows)]
    {
        std::ffi::OsString::from(String::from_utf8_lossy(bytes).into_owned())
    }
}
