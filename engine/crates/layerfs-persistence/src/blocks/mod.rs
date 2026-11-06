//! Append-only metadata blocks (#91, schema version 2): many inode values or
//! binding hints in one immutable row, field-packed so every entry expands
//! back to its exact canonical bytes. No compression: varints, implicit
//! keys and a per-block table of distinct modes only. Every block ends with
//! a CRC32C of the bytes before it, checked on every decode.
mod frame;
pub(crate) mod hints;
pub(crate) mod values;
