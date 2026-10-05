//! The prepared request header: identity, addressed fields and exact totals.
//!
//! The rows of a prepared update do not travel here. This is the fixed part of
//! the request - the Workspace, Branch, expected head and base, generation,
//! construction base, scope, root serial and the exact totals of the ordered
//! stream the body carries - so a metadata frame bounds the *request*, and the
//! rows are bounded by the charged stream declaration instead.

use super::metadata::{put_optional, take_array, take_optional};
use super::{Decoder, Encoder};
use crate::contract::*;

/// Writes the prepared request header.
pub(super) fn put_prepared(e: &mut Encoder, changes: &PreparedChanges) -> Result<(), Failure> {
    e.put(&changes.workspace)?;
    e.put(&changes.branch)?;
    put_optional(e, changes.expected_head.as_ref())?;
    e.put(&changes.expected_base)?;
    e.u64(changes.generation)?;
    e.put(&changes.base)?;
    e.put(&changes.scope)?;
    e.u64(changes.root_serial)?;
    let totals = changes.totals;
    for count in [
        totals.directories,
        totals.names,
        totals.name_bytes,
        totals.identities,
        totals.patches,
        totals.declarations,
        totals.fresh,
    ] {
        e.u64(count)?;
    }
    Ok(())
}

/// Reads the prepared request header.
pub(super) fn take_prepared(d: &mut Decoder<'_>) -> Result<PreparedChanges, Failure> {
    let changes = PreparedChanges {
        workspace: take_array::<32>(d)?,
        branch: take_array::<17>(d)?,
        expected_head: take_optional::<33>(d)?,
        expected_base: take_array::<33>(d)?,
        generation: d.u64()?,
        base: d.root()?,
        scope: d.root()?,
        root_serial: d.u64()?,
        totals: PreparedTotals {
            directories: d.u64()?,
            names: d.u64()?,
            name_bytes: d.u64()?,
            identities: d.u64()?,
            patches: d.u64()?,
            declarations: d.u64()?,
            fresh: d.u64()?,
        },
    };
    Ok(changes)
}
