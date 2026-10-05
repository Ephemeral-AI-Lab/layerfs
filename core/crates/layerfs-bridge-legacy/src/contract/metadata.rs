//! Portable metadata construction and updates over an existing attribute tree.
use super::{Code, Failure};

pub const UPDATE_PORTABLE_METADATA_OPCODE: u8 = 9;
pub const PORTABLE_METADATA_REQUEST_BYTES: usize = 76;
pub const PORTABLE_METADATA_RESULT_BYTES: usize = 98;
pub const CONSTRUCT_PORTABLE_METADATA_OPCODE: u8 = 15;
pub const CONSTRUCT_PORTABLE_METADATA_REQUEST_BYTES: usize = 44;
pub const CONSTRUCT_PORTABLE_METADATA_RESULT_BYTES: usize = 66;

pub(crate) fn check_portable_metadata(
    kind: u8,
    mode: u32,
    nanoseconds: u32,
) -> Result<(), Failure> {
    let valid = match kind {
        1 => mode & !0o777 == 0,
        2 => mode & !0o1777 == 0,
        3 => mode == 0o777,
        _ => false,
    };
    if !valid || nanoseconds >= 1_000_000_000 {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}
