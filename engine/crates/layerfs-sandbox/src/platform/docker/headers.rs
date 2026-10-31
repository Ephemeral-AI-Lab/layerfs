//! One strict header grammar shared by initial metadata and chunk trailers.
use std::io;
pub(super) fn field(row: &[u8]) -> io::Result<(&[u8], &[u8])> {
    let colon = row
        .iter()
        .position(|b| *b == b':')
        .ok_or_else(|| invalid("HTTP header colon"))?;
    let name = &row[..colon];
    if name.is_empty()
        || !name
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(b))
    {
        return Err(invalid("HTTP header name"));
    }
    let mut value = &row[colon + 1..];
    while matches!(value.first(), Some(b' ' | b'\t')) {
        value = &value[1..]
    }
    while matches!(value.last(), Some(b' ' | b'\t')) {
        value = &value[..value.len() - 1]
    }
    if value.iter().any(|b| (*b < 32 && *b != b'\t') || *b == 127) {
        return Err(invalid("HTTP header control"));
    }
    Ok((name, value))
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
