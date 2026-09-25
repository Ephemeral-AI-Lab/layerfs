//! Checked Linux LFB3/LFD3/LFA3/LFX3 frames; no staged mutation here.
use fuser::{Errno, IoctlFlags};

pub(super) const BEGIN: u32 = 0xc080_f542;
pub(super) const DATA: u32 = 0x5080_f543;
pub(super) const APPLY: u32 = 0x4080_f544;
pub(super) const ABORT: u32 = 0x4080_f545;
const MAX_REPLACEMENT: u64 = 8 * 1024 * 1024;

fn u16_at(input: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(input[at..at + 2].try_into().unwrap())
}
pub(super) fn u32_at(input: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(input[at..at + 4].try_into().unwrap())
}
pub(super) fn u64_at(input: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(input[at..at + 8].try_into().unwrap())
}

/// Validate the exact frame before any stage or Workspace state is touched.
pub(super) fn validate(
    cmd: u32,
    input: &[u8],
    out_size: u32,
    flags: IoctlFlags,
) -> Result<(), Errno> {
    if !flags.is_empty() {
        return Err(Errno::EOPNOTSUPP);
    }
    let (size, output, magic) = match cmd {
        BEGIN => (128, 128, b"LFB3"),
        DATA => (4224, 0, b"LFD3"),
        APPLY => (128, 0, b"LFA3"),
        ABORT => (128, 0, b"LFX3"),
        _ => return Err(Errno::ENOTTY),
    };
    if input.len() != size || out_size != output || &input[..4] != magic {
        return Err(Errno::EINVAL);
    }
    if u16_at(input, 4) != 3 {
        return Err(Errno::EOPNOTSUPP);
    }
    match cmd {
        BEGIN => {
            if u16_at(input, 6) != 0 {
                return Err(Errno::EOPNOTSUPP);
            }
            let offset = u64_at(input, 64);
            let deleted = u64_at(input, 72);
            let logical = u64_at(input, 80);
            let literal = u64_at(input, 88);
            if offset.checked_add(deleted).is_none()
                || literal > logical
                || (deleted == 0 && logical == 0)
            {
                return Err(Errno::EINVAL);
            }
            if logical > MAX_REPLACEMENT {
                return Err(Errno::ENOSPC);
            }
        }
        DATA => {
            let kind = u16_at(input, 6);
            let offset = u64_at(input, 24);
            let logical = u64_at(input, 32);
            let literal = u32_at(input, 40);
            if input[8..24].iter().all(|byte| *byte == 0)
                || input[44..128].iter().any(|byte| *byte != 0)
                || offset
                    .checked_add(logical)
                    .is_none_or(|end| end > MAX_REPLACEMENT)
            {
                return Err(Errno::EINVAL);
            }
            match kind {
                1 if (1..=4096).contains(&literal)
                    && logical == u64::from(literal)
                    && input[128 + literal as usize..]
                        .iter()
                        .all(|byte| *byte == 0) => {}
                2 if (1..=MAX_REPLACEMENT).contains(&logical)
                    && literal == 0
                    && input[128..].iter().all(|byte| *byte == 0) => {}
                _ => return Err(Errno::EINVAL),
            }
        }
        APPLY | ABORT => {
            if u16_at(input, 6) != 0 {
                return Err(Errno::EOPNOTSUPP);
            }
            if input[8..24].iter().all(|byte| *byte == 0)
                || input[24..].iter().any(|byte| *byte != 0)
            {
                return Err(Errno::EINVAL);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
