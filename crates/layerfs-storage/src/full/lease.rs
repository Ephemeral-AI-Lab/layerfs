//! Full accepted-origin lease persistence.

use crate::error::{map_sqlite_error, EngineResult};
use crate::full::record_id::derive_id;
use crate::full::transfer::batch::BranchPushBundle;
use crate::working::lease::unix_seconds;
use rusqlite::params;
use rusqlite::Connection;

pub(crate) fn insert_branch_origin_lease(
    connection: &Connection,
    bundle: &BranchPushBundle,
) -> EngineResult<()> {
    let lease_id = derive_id(
        if bundle.ancestry.depth == 0 {
            b"top-level-branch-origin-lease"
        } else {
            b"child-branch-origin-lease"
        },
        &[
            bundle.head.branch_id.as_bytes(),
            bundle
                .ancestry
                .fork_operation_version_id
                .map(|id| id.0)
                .unwrap_or(bundle.ancestry.origin_layer_id.0)
                .as_slice(),
        ],
    );
    let (target_kind, target_id) = match bundle.ancestry.fork_operation_version_id {
        Some(version) => ("operation_version", version.0),
        None => ("layer", bundle.ancestry.origin_layer_id.0),
    };
    connection
        .execute(
            "INSERT INTO layerfs_version_leases
             (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
             VALUES (?1, ?2, ?3, 'branch', ?4, ?5)",
            params![
                lease_id.as_slice(),
                target_kind,
                target_id.as_slice(),
                bundle.head.branch_id.as_bytes(),
                unix_seconds()?,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}
