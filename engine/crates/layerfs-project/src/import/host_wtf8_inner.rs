// Lossless name bytes on Windows: the host name is UTF-16 code units, and
// every unit - including an unpaired surrogate - encodes as WTF-8, so the
// bytes decode back to the very same units. This file is the standard's
// encoding table and nothing else; the product (import/host.rs) and the
// name-roundtrip test each include it as a module.
use std::ffi::OsStr;
use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::ffi::OsStringExt;

// One host name's WTF-8 bytes.
pub fn encode(value: &OsStr) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    for unit in value.encode_wide() {
        match unit {
            0x0000..=0x007F => out.push(unit as u8),
            0x0080..=0x07FF => {
                out.push(0xC0 | (unit >> 6) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
            0x0800..=0xFFFF => {
                out.push(0xE0 | (unit >> 12) as u8);
                out.push(0x80 | ((unit >> 6) & 0x3F) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
            _ => unreachable!("a UTF-16 code unit is at most 16 bits"),
        }
    }
    out
}

// The host name of WTF-8 `bytes`.
pub fn decode(bytes: &[u8]) -> OsString {
    let mut units = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let first = bytes[index];
        let (unit, width) = if first < 0x80 {
            (u16::from(first), 1)
        } else if first & 0xE0 == 0xC0 {
            let unit = (u16::from(first & 0x1F) << 6)
                | u16::from(bytes.get(index + 1).copied().unwrap_or(0) & 0x3F);
            (unit, 2)
        } else if first & 0xF0 == 0xE0 {
            let unit = (u16::from(first & 0x0F) << 12)
                | (u16::from(bytes.get(index + 1).copied().unwrap_or(0) & 0x3F) << 6)
                | u16::from(bytes.get(index + 2).copied().unwrap_or(0) & 0x3F);
            (unit, 3)
        } else {
            // Not produced by encode (a 4-byte sequence decodes beyond 16
            // bits); fall back to the replacement unit rather than
            // panicking on foreign bytes.
            (0xFFFD, 1)
        };
        units.push(unit);
        index += width;
    }
    OsString::from_wide(&units)
}
