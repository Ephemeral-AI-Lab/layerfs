//! History statement texts on PostgreSQL. The shipped SQLite statements are
//! portable SQL (positional `$n` parameters, `RETURNING`, `ON CONFLICT`) and
//! run unchanged. Those using SQLite-only syntax are replaced here, and so
//! is the history closure (`kept_node_mark`), whose shared text PostgreSQL
//! plans as one scan of every edge per recursion round. The replacement
//! must follow exactly the edges the shared text follows: add an edge to
//! both files or to neither.
//! Array forms bind whole columns, so many rows cost one round trip.
use crate::backend::error::BackendError;

pub(crate) fn statement(key: &str) -> Result<&'static str, BackendError> {
    match key {
        "hint_insert_block" => Ok(include_str!(
            "../../../../sql/postgresql/history/index/hint_insert_block.sql"
        )),
        "statistics_totals" => Ok(include_str!(
            "../../../../sql/postgresql/history/index/statistics_totals.sql"
        )),
        // The shared closure joins each round's new nodes to every history
        // edge, so a chain of n unpublished Commits costs n scans of the edge
        // set. This text follows each new node's own row by primary key.
        "kept_node_mark" => Ok(include_str!(
            "../../../../sql/postgresql/gc/kept_node_mark.sql"
        )),
        "gc_epoch_read" => Ok(include_str!("../../../../sql/postgresql/gc/epoch_read.sql")),
        "gc_epoch_share" => Ok(include_str!(
            "../../../../sql/postgresql/gc/epoch_share.sql"
        )),
        "gc_epoch_lock" => Ok(include_str!("../../../../sql/postgresql/gc/epoch_lock.sql")),
        "gc_lock_xact_shared" => Ok(include_str!(
            "../../../../sql/postgresql/gc/lock_xact_shared.sql"
        )),
        "gc_lock_xact_exclusive" => Ok(include_str!(
            "../../../../sql/postgresql/gc/lock_xact_exclusive.sql"
        )),
        "gc_advisory_holder" => Ok(include_str!(
            "../../../../sql/postgresql/gc/advisory_holder.sql"
        )),
        other => crate::backend::sqlite::history::history(other),
    }
}

/// Statements that name the Store's schema (`{s}`), so each also runs alone
/// outside a transaction: one statement, one snapshot, no `BEGIN`/`COMMIT`.
/// Every key a `read_statement` body sends must be here or in [`arrays`]: the
/// `postgresql_statement_schema` guard pins each one-statement read.
pub(crate) fn qualified(key: &str) -> Option<&'static str> {
    match key {
        "branch_branch_by_id_any" => Some(include_str!(
            "../../../../sql/postgresql/history/branch/by_id_any.sql"
        )),
        "branch_branch_by_name" => Some(include_str!(
            "../../../../sql/postgresql/history/branch/by_name.sql"
        )),
        "branch_snapshot" => Some(include_str!(
            "../../../../sql/postgresql/history/branch/snapshot.sql"
        )),
        "branch_snapshot_named" => Some(include_str!(
            "../../../../sql/postgresql/history/branch/snapshot_named.sql"
        )),
        "commit_commit_by_id" => Some(include_str!(
            "../../../../sql/postgresql/history/node/commit_by_id.sql"
        )),
        "layerstack_layer_by_id" => Some(include_str!(
            "../../../../sql/postgresql/history/layerstack/layer_by_id.sql"
        )),
        "layerstack_layer_by_source" => Some(include_str!(
            "../../../../sql/postgresql/history/layerstack/layer_by_source.sql"
        )),
        "layerstack_stack_by_name" => Some(include_str!(
            "../../../../sql/postgresql/history/layerstack/stack_by_name.sql"
        )),
        "paths_markers" => Some(include_str!(
            "../../../../sql/postgresql/history/node/paths_markers.sql"
        )),
        "paths_node_markers" => Some(include_str!(
            "../../../../sql/postgresql/history/node/paths_node_markers.sql"
        )),
        "position_node_serial" => Some(include_str!(
            "../../../../sql/postgresql/history/node/position_node_serial.sql"
        )),
        // A read call's context in one statement (#818 R5); PostgreSQL only:
        // SQLite reads it through the catalog's default, statement by
        // statement.
        "read_context_named" => Some(include_str!(
            "../../../../sql/postgresql/history/node/read_context_named.sql"
        )),
        "read_context_node" => Some(include_str!(
            "../../../../sql/postgresql/history/node/read_context_node.sql"
        )),
        // First-parent Commits in windows (#818 H4); PostgreSQL only.
        "lineage_chain_window" => Some(include_str!(
            "../../../../sql/postgresql/history/node/lineage_chain_window.sql"
        )),
        // Init's acquisition reads (#818 I3a): each window joined to the
        // live owner row; the owner row alone serves `work` and every
        // ordinary write unit's owner check.
        "acquisition_owner" => Some(include_str!(
            "../../../../sql/postgresql/acquisition/owner.sql"
        )),
        "acquisition_unplaced" => Some(include_str!(
            "../../../../sql/postgresql/acquisition/unplaced.sql"
        )),
        "acquisition_directories" => Some(include_str!(
            "../../../../sql/postgresql/acquisition/directories.sql"
        )),
        "acquisition_directory_path" => Some(include_str!(
            "../../../../sql/postgresql/acquisition/directory_path.sql"
        )),
        "acquisition_jobs" => Some(include_str!(
            "../../../../sql/postgresql/acquisition/jobs.sql"
        )),
        "acquisition_job" => Some(include_str!(
            "../../../../sql/postgresql/acquisition/job.sql"
        )),
        "acquisition_file_roots" => Some(include_str!(
            "../../../../sql/postgresql/acquisition/file_roots.sql"
        )),
        "acquisition_entries" => Some(include_str!(
            "../../../../sql/postgresql/acquisition/entries.sql"
        )),
        "acquisition_bindings" => Some(include_str!(
            "../../../../sql/postgresql/acquisition/bindings.sql"
        )),
        _ => None,
    }
}

/// The GC admission statements `Mode::Bare` sends as written (#768): the
/// session keys of one intent, taken and given back in one statement each,
/// and the owner's release. Each carries any generation check it needs in its
/// own text; `{s}` names the schema.
pub(crate) fn bare(key: &str) -> Option<&'static str> {
    match key {
        "gc_lock_session" => Some(include_str!(
            "../../../../sql/postgresql/gc/lock_session.sql"
        )),
        "gc_unlock_session" => Some(include_str!(
            "../../../../sql/postgresql/gc/unlock_session.sql"
        )),
        "gc_intent_release_owner" => Some(include_str!(
            "../../../../sql/postgresql/gc/intent_release_owner.sql"
        )),
        _ => None,
    }
}

/// The array form of a per-row statement, where one exists.
pub(crate) fn arrays(key: &str) -> Option<&'static str> {
    match key {
        "hint_insert_blocks" => Some(include_str!(
            "../../../../sql/postgresql/history/index/hint_insert_blocks.sql"
        )),
        "digest_insert_rows" => Some(include_str!(
            "../../../../sql/postgresql/history/index/digest_insert_rows.sql"
        )),
        "digest_roots" => Some(include_str!(
            "../../../../sql/postgresql/history/index/digest_roots.sql"
        )),
        _ => None,
    }
}
