//! 512-byte ustar/GNU tar header parsing, alignment, and checksum helpers.

use std::io;

pub(super) const BLOCK: usize = 512;

pub(super) fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

pub(super) fn padded_size(size: u64) -> io::Result<u64> {
    size.div_ceil(BLOCK as u64)
        .checked_mul(BLOCK as u64)
        .ok_or_else(|| invalid("tar entry size overflow"))
}

/// A header's name: ustar `prefix/name`, or the name alone.
pub(super) fn ustar_name(header: &[u8; BLOCK]) -> String {
    let name = text(&header[0..100]);
    if &header[257..263] == b"ustar\0" {
        let prefix = text(&header[345..500]);
        if !prefix.is_empty() {
            return format!("{prefix}/{name}");
        }
    }
    name
}

/// Bytes up to the first NUL, as text.
pub(super) fn text(field: &[u8]) -> String {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// An octal header number, or GNU's base-256 form.
pub(super) fn octal(field: &[u8]) -> io::Result<u64> {
    if field[0] & 0x80 != 0 {
        let mut n: u64 = u64::from(field[0] & 0x7f);
        for b in &field[1..] {
            n = n
                .checked_mul(256)
                .and_then(|n| n.checked_add(u64::from(*b)))
                .ok_or_else(|| invalid("tar size"))?;
        }
        return Ok(n);
    }
    let digits: String = field
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| *b as char)
        .collect();
    let digits = digits.trim();
    if digits.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(digits, 8).map_err(|_| invalid("tar header number"))
}

pub(super) fn write_size_field(field: &mut [u8], value: u64) -> io::Result<()> {
    if value <= 0o77_777_777_777 {
        write_octal(field, value);
        return Ok(());
    }
    if field.len() != 12 {
        return Err(invalid("tar size field"));
    }
    let be = value.to_be_bytes();
    field[0] = 0x80;
    field[1..4].fill(0);
    field[4..12].copy_from_slice(&be);
    Ok(())
}

pub(super) fn write_octal(field: &mut [u8], value: u64) {
    let width = field.len() - 1;
    let text = format!("{value:0width$o}\0");
    field.copy_from_slice(&text.as_bytes()[..field.len()]);
}

pub(super) fn recompute_checksum(header: &mut [u8; BLOCK]) {
    header[148..156].fill(b' ');
    let sum: u64 = header.iter().map(|b| u64::from(*b)).sum();
    let text = format!("{sum:06o}\0 ");
    header[148..156].copy_from_slice(text.as_bytes());
}

pub(super) fn zero_header_mtime(header: &mut [u8; BLOCK]) {
    header[136..148].copy_from_slice(b"00000000000\0");
    recompute_checksum(header);
}
