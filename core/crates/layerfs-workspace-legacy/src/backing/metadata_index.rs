//! Relocated: the keyed page index now lives in
//! `backing::binary_plus_tree::keyed::{update,build}`.
//!
//! This path stays a reexport until nothing imports it. It is a relocation
//! only - no page format, algorithm or public path changed. The crate-wide
//! charged `vector` helper stays here because the extent specialization uses it
//! too; stage two moves it with the page store.
pub(crate) use super::binary_plus_tree::keyed::{edges, edges_raw, stored_key_limit};
use crate::WorkspaceError;
pub fn vector<T>(capacity: usize) -> Result<Vec<T>, WorkspaceError> {
    let mut out = Vec::new();
    out.try_reserve_exact(capacity)
        .map_err(|_| WorkspaceError::Capacity)?;
    if out.capacity() > capacity {
        return Err(WorkspaceError::Capacity);
    }
    Ok(out)
}
