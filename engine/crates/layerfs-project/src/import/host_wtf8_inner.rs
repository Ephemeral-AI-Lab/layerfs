// Lossless name bytes on Windows: the host name is UTF-16 code units, and
// a well-formed pair encodes as its supplementary scalar (the same UTF-8
// bytes Linux stores for that name), while an unpaired surrogate keeps its
// three-byte WTF-8 form. The bytes decode back to the very same units.
// This file is the standard's encoding table and nothing else; the product
// (import/host.rs) and the name-roundtrip test each include it as a module.
use std::ffi::OsStr;
use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::ffi::OsStringExt;

// One host name's WTF-8 bytes.
pub fn encode(value: &OsStr) -> Vec<u8> {
    let units: Vec<u16> = value.encode_wide().collect();
    let mut out = Vec::with_capacity(units.len() * 3);
    let mut index = 0;
    while index < units.len() {
        let unit = units[index];
        if (0xD800..=0xDBFF).contains(&unit) {
            if let Some(&low) = units.get(index + 1) {
                if (0xDC00..=0xDFFF).contains(&low) {
                    let scalar =
                        0x1_0000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
                    out.push(0xF0 | (scalar >> 18) as u8);
                    out.push(0x80 | ((scalar >> 12) & 0x3F) as u8);
                    out.push(0x80 | ((scalar >> 6) & 0x3F) as u8);
                    out.push(0x80 | (scalar & 0x3F) as u8);
                    index += 2;
                    continue;
                }
            }
        }
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
        index += 1;
    }
    out
}

// The host name of WTF-8 `bytes`.
pub fn decode(bytes: &[u8]) -> OsString {
    let mut units = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let first = bytes[index];
        let (width, scalar) = if first < 0x80 {
            (1, u32::from(first))
        } else if first & 0xE0 == 0xC0 {
            (
                2,
                (u32::from(first & 0x1F) << 6)
                    | u32::from(bytes.get(index + 1).copied().unwrap_or(0) & 0x3F),
            )
        } else if first & 0xF0 == 0xE0 {
            (
                3,
                (u32::from(first & 0x0F) << 12)
                    | (u32::from(bytes.get(index + 1).copied().unwrap_or(0) & 0x3F) << 6)
                    | u32::from(bytes.get(index + 2).copied().unwrap_or(0) & 0x3F),
            )
        } else if first & 0xF8 == 0xF0 {
            (
                4,
                (u32::from(first & 0x07) << 18)
                    | (u32::from(bytes.get(index + 1).copied().unwrap_or(0) & 0x3F) << 12)
                    | (u32::from(bytes.get(index + 2).copied().unwrap_or(0) & 0x3F) << 6)
                    | u32::from(bytes.get(index + 3).copied().unwrap_or(0) & 0x3F),
            )
        } else {
            // A lead byte encode never produces; fall back to the
            // replacement unit rather than panicking on foreign bytes.
            units.push(0xFFFD);
            index += 1;
            continue;
        };
        if width == 4 {
            if let scalar @ 0x1_0000..=0x10FFFF = scalar {
                let relative = scalar - 0x1_0000;
                units.push(0xD800 + (relative >> 10) as u16);
                units.push(0xDC00 + (relative & 0x3FF) as u16);
            } else {
                units.push(0xFFFD);
            }
        } else {
            units.push(scalar as u16);
        }
        index += width;
    }
    OsString::from_wide(&units)
}
