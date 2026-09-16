//! Cutting one stored extent at a declared edit boundary.
//!
//! A retained range is a slice of exactly one base extent: the payload identity is
//! preserved and only the source offset and length move. Offsets are checked
//! against the extent's own coverage, so a boundary that does not lie inside the
//! extent is rejected rather than silently clamped.

use crate::error::{ContentError, ContentResult};
use crate::file::mapping::ExtentSlice;

/// Cuts `from..to` (absolute base offsets) out of `extent`, which starts at `origin`.
pub fn slice_of(
    extent: ExtentSlice,
    origin: u64,
    from: u64,
    to: u64,
) -> ContentResult<ExtentSlice> {
    let end = origin
        .checked_add(u64::from(extent.logical_length()))
        .ok_or(ContentError::LengthOverflow)?;
    if from >= to || from < origin || to > end {
        return Err(ContentError::InvalidRecord("retained range outside extent"));
    }
    let source = u64::from(extent.source_offset())
        .checked_add(from - origin)
        .ok_or(ContentError::LengthOverflow)?;
    ExtentSlice::new(
        extent.payload_object_id(),
        u32::try_from(source).map_err(|_| ContentError::LengthOverflow)?,
        u32::try_from(to - from).map_err(|_| ContentError::LengthOverflow)?,
    )
}
