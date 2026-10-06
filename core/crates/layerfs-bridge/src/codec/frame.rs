//! Fixed header codec; flags are verified rather than trusted as completion.
use crate::contract::{
    Envelope, Fragment, FrameError, FrameResult, MessageClass, MessageKind, HEADER_BYTES,
    MAX_RECORD_BYTES,
};
/// Writes one checked fragment into a caller-owned fixed record window.
pub fn encode(fragment: Fragment<'_>, output: &mut [u8]) -> FrameResult<usize> {
    fragment.validate()?;
    let n = HEADER_BYTES + fragment.bytes.len();
    if output.len() < n {
        return Err(FrameError::Invalid("record output window"));
    }
    output[..4].copy_from_slice(b"LFR1");
    output[4] = fragment.envelope.kind as u8;
    output[5] = fragment.envelope.class as u8;
    output[6] = u8::from(fragment.offset == 0) | (u8::from(fragment.is_end()) << 1);
    output[7] = 0;
    for (start, value) in [
        (8, fragment.envelope.message),
        (16, fragment.envelope.correlation),
        (24, fragment.envelope.total_bytes),
        (32, fragment.offset),
    ] {
        output[start..start + 8].copy_from_slice(&value.to_be_bytes());
    }
    output[HEADER_BYTES..n].copy_from_slice(fragment.bytes);
    Ok(n)
}
/// Checks one exact authenticated record and borrows its body without allocation.
pub fn decode(input: &[u8]) -> FrameResult<Fragment<'_>> {
    if input.len() < HEADER_BYTES
        || input.len() > MAX_RECORD_BYTES
        || &input[..4] != b"LFR1"
        || input[7] != 0
    {
        return Err(FrameError::Invalid("record header"));
    }
    let kind = match input[4] {
        1 => MessageKind::Request,
        2 => MessageKind::Reply,
        _ => return Err(FrameError::Invalid("message kind")),
    };
    let class = match input[5] {
        1 => MessageClass::Demand,
        2 => MessageClass::Control,
        3 => MessageClass::Save,
        _ => return Err(FrameError::Invalid("message class")),
    };
    let integer = |start| {
        u64::from_be_bytes(
            input[start..start + 8]
                .try_into()
                .expect("fixed checked header"),
        )
    };
    let fragment = Fragment {
        envelope: Envelope {
            message: integer(8),
            correlation: integer(16),
            kind,
            class,
            total_bytes: integer(24),
        },
        offset: integer(32),
        bytes: &input[HEADER_BYTES..],
    };
    fragment.validate()?;
    let flags = u8::from(fragment.offset == 0) | (u8::from(fragment.is_end()) << 1);
    if input[6] != flags {
        return Err(FrameError::Invalid("fragment flags"));
    }
    Ok(fragment)
}
