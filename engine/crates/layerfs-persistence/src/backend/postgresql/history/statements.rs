//! History statement texts on PostgreSQL. The shipped SQLite statements are
//! portable SQL (positional `$n` parameters, `RETURNING`, `ON CONFLICT`) and
//! run unchanged. Those using SQLite-only syntax are replaced here, and so
//! is the history closure (`kept_node_mark`), whose shared text PostgreSQL
//! plans as one scan of every edge per recursion round. The replacement
//! must follow exactly the edges the shared text follows: add an edge to
//! both files or to neither.
//! Array forms bind whole columns, so many rows cost one round trip.
use crate::backend::records::BackendError;

pub(crate) fn statement(key: &str) -> Result<&'static str, BackendError> {
    match key {
        "hint_insert_block" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/hint_insert_block.sql"
        )),
        "statistics_totals" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/statistics_totals.sql"
        )),
        "gc_object_locations" => Ok(
            "SELECT object_id,pack_id,group_number::bigint,record_number::bigint,role::bigint,canonical_length::bigint FROM object_location",
        ),
        // The shared closure joins each round's new nodes to every history
        // edge, so a chain of n unpublished Commits costs n scans of the edge
        // set. This text follows each new node's own row by primary key.
        "kept_node_mark" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/kept_node_mark.sql"
        )),
        "gc_epoch_read" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/gc_epoch_read.sql"
        )),
        "gc_epoch_share" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/gc_epoch_share.sql"
        )),
        "gc_epoch_lock" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/gc_epoch_lock.sql"
        )),
        "gc_lock_xact_shared" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/gc_lock_xact_shared.sql"
        )),
        "gc_lock_xact_exclusive" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/gc_lock_xact_exclusive.sql"
        )),
        "gc_lock_session_shared" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/gc_lock_session_shared.sql"
        )),
        "gc_lock_session_exclusive" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/gc_lock_session_exclusive.sql"
        )),
        "gc_unlock_session_shared" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/gc_unlock_session_shared.sql"
        )),
        "gc_unlock_session_exclusive" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/gc_unlock_session_exclusive.sql"
        )),
        "gc_advisory_holder" => Ok(include_str!(
            "../../../../sql/postgresql/queries/history/gc_advisory_holder.sql"
        )),
        other => crate::backend::sqlite::session::query::history(other),
    }
}

/// Statements that name the Store's schema (`{s}`), so each also runs alone
/// outside a transaction: one statement, one snapshot, no `BEGIN`/`COMMIT`.
/// Every key a `read_statement` body sends must be here or in [`arrays`]: the
/// `postgresql_statement_schema` guard pins each one-statement read.
pub(crate) fn qualified(key: &str) -> Option<&'static str> {
    match key {
        "branch_branch_by_id_any" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/branch_branch_by_id_any.sql"
        )),
        "branch_snapshot" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/branch_snapshot.sql"
        )),
        "branch_snapshot_named" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/branch_snapshot_named.sql"
        )),
        "commit_commit_by_id" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/commit_commit_by_id.sql"
        )),
        "layerstack_layer_by_id" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/layerstack_layer_by_id.sql"
        )),
        "layerstack_layer_by_source" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/layerstack_layer_by_source.sql"
        )),
        "layerstack_stack_by_name" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/layerstack_stack_by_name.sql"
        )),
        "position_node_serial" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/position_node_serial.sql"
        )),
        _ => None,
    }
}

/// The array form of a per-row statement, where one exists.
pub(crate) fn arrays(key: &str) -> Option<&'static str> {
    match key {
        "hint_insert_blocks" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/hint_insert_blocks.sql"
        )),
        "digest_insert_rows" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/digest_insert_rows.sql"
        )),
        "digest_roots" => Some(include_str!(
            "../../../../sql/postgresql/queries/history/digest_roots.sql"
        )),
        _ => None,
    }
}
