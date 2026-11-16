//! Closed statement selection and measured SQLite execution.
use super::connection::SqlWork;
use super::rows;
use crate::backend::records::{BackendError, Param, Record};
use rusqlite::{Connection, StatementStatus};
use std::{cell::RefCell, time::Instant};
pub(crate) fn history(key: &str) -> Result<&'static str, BackendError> {
    match key {
        "statistics_totals" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/statistics_totals.sql"
        )),
        "statistics_record_init" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/statistics_record_init.sql"
        )),
        "allocation_advance_scope" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/allocation_advance_scope.sql"
        )),
        "allocation_insert_scope" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/allocation_insert_scope.sql"
        )),
        "allocation_scope_by_id" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/allocation_scope_by_id.sql"
        )),
        "hint_insert_block" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/hint_insert_block.sql"
        )),
        "hint_blocks_of" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/hint_blocks_of.sql"
        )),
        "digest_insert" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/digest_insert.sql"
        )),
        "digest_of" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/digest_of.sql"
        )),
        "branch_snapshot" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/branch_snapshot.sql"
        )),
        "branch_snapshot_named" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/branch_snapshot_named.sql"
        )),
        "lineage_parent" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_parent.sql"
        )),
        "branch_branch_by_id" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/branch_branch_by_id.sql"
        )),
        "branch_branch_by_id_any" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/branch_branch_by_id_any.sql"
        )),
        "branch_retire_insert" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/branch_retire_insert.sql"
        )),
        "branch_branch_by_name" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/branch_branch_by_name.sql"
        )),
        "branch_branch_first_page" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/branch_branch_first_page.sql"
        )),
        "branch_branch_name_taken" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/branch_branch_name_taken.sql"
        )),
        "branch_branch_next_page" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/branch_branch_next_page.sql"
        )),
        "branch_insert_branch" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/branch_insert_branch.sql"
        )),
        "commit_advance_branch" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/commit_advance_branch.sql"
        )),
        "commit_commit_by_id" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/commit_commit_by_id.sql"
        )),
        "commit_insert_commit" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/commit_insert_commit.sql"
        )),
        "layerstack_advance_stack" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_advance_stack.sql"
        )),
        "layerstack_insert_layer" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_insert_layer.sql"
        )),
        "layerstack_insert_stack" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_insert_stack.sql"
        )),
        "layerstack_layer_by_id" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_layer_by_id.sql"
        )),
        "layerstack_layer_by_source" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_layer_by_source.sql"
        )),
        "layerstack_layer_root" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_layer_root.sql"
        )),
        "layerstack_stack_by_id" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_stack_by_id.sql"
        )),
        "layerstack_stack_first_page" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_stack_first_page.sql"
        )),
        "layerstack_stack_head" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_stack_head.sql"
        )),
        "layerstack_stack_by_name" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_stack_by_name.sql"
        )),
        "layerstack_stack_name_taken" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_stack_name_taken.sql"
        )),
        "layerstack_insert_project_epoch" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_insert_project_epoch.sql"
        )),
        "layerstack_bump_project_epoch" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_bump_project_epoch.sql"
        )),
        "layerstack_insert_retired" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_insert_retired.sql"
        )),
        "layerstack_deleted_ms" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_deleted_ms.sql"
        )),
        "layerstack_stack_next_page" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/layerstack_stack_next_page.sql"
        )),
        "lineage_commit" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_commit.sql"
        )),
        "lineage_layer" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_layer.sql"
        )),
        "lineage_layers_above" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_layers_above.sql"
        )),
        "lineage_commit_at" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_commit_at.sql"
        )),
        "lineage_insert_commit" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_insert_commit.sql"
        )),
        "lineage_insert_layer" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_insert_layer.sql"
        )),
        "lineage_merges" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_merges.sql"
        )),
        "lineage_absorbed_below" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_absorbed_below.sql"
        )),
        "lineage_absorbed_height" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_absorbed_height.sql"
        )),
        "lineage_absorb" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_absorb.sql"
        )),
        "lineage_reach" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_reach.sql"
        )),
        "lineage_reach_prune" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_reach_prune.sql"
        )),
        "lineage_reach_insert" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/lineage_reach_insert.sql"
        )),
        "removal_totals" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/removal_totals.sql"
        )),
        "removal_absorbed" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/removal_absorbed.sql"
        )),
        "removal_reach" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/removal_reach.sql"
        )),
        "removal_commit_lineage" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/removal_commit_lineage.sql"
        )),
        "removal_layer_lineage" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/removal_layer_lineage.sql"
        )),
        "removal_branches" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/removal_branches.sql"
        )),
        "removal_commits" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/removal_commits.sql"
        )),
        "removal_layers" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/removal_layers.sql"
        )),
        "removal_stack" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/removal_stack.sql"
        )),
        "position_high_water" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/position_high_water.sql"
        )),
        "position_stack_high_water" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/position_stack_high_water.sql"
        )),
        "position_next_serial" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/position_next_serial.sql"
        )),
        "position_node_serial" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/position_node_serial.sql"
        )),
        "paths_marker" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/paths_marker.sql"
        )),
        "paths_markers" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/paths_markers.sql"
        )),
        "paths_upsert" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/paths_upsert.sql"
        )),
        "paths_advance_epoch" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/paths_advance_epoch.sql"
        )),
        "gc_intent_insert" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_insert.sql"
        )),
        "gc_intent_get" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_get.sql"
        )),
        "gc_intent_release" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_release.sql"
        )),
        "gc_intent_uncertain" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_uncertain.sql"
        )),
        "gc_intent_list" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_list.sql"
        )),
        "gc_intent_ops_branch" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_ops_branch.sql"
        )),
        "gc_intent_ops_project" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_ops_project.sql"
        )),
        "gc_intent_ops_store" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_ops_store.sql"
        )),
        "gc_intent_all_branch" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_all_branch.sql"
        )),
        "gc_intent_all_project" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_all_project.sql"
        )),
        "gc_intent_all_store" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_intent_all_store.sql"
        )),
        "gc_branch_stack_any" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_branch_stack_any.sql"
        )),
        "gc_epoch_read" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_epoch_read.sql"
        )),
        "gc_epoch_share" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_epoch_share.sql"
        )),
        "gc_epoch_lock" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_epoch_lock.sql"
        )),
        "gc_kept_roots" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_kept_roots.sql"
        )),
        "gc_object_locations" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_object_locations.sql"
        )),
        "gc_live_stacks" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_live_stacks.sql"
        )),
        "gc_live_branches" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_live_branches.sql"
        )),
        "gc_inode_blocks" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_inode_blocks.sql"
        )),
        "gc_tuple_read" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/gc_tuple_read.sql"
        )),
        "kept_node_clear" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/kept_node_clear.sql"
        )),
        "kept_node_mark" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/kept_node_mark.sql"
        )),
        "kept_node_read" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/kept_node_read.sql"
        )),
        "kept_node_roots" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/kept_node_roots.sql"
        )),
        "kept_node_kept_roots" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/kept_node_kept_roots.sql"
        )),
        "history_unreachable_count" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/history_unreachable_count.sql"
        )),
        "history_remove_unreachable_commit" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/history_remove_unreachable_commit.sql"
        )),
        "history_remove_unreachable_layer" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/history_remove_unreachable_layer.sql"
        )),
        "history_remove_unreachable_branch" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/history_remove_unreachable_branch.sql"
        )),
        "history_remove_unreachable_stack" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/history_remove_unreachable_stack.sql"
        )),
        "history_remove_unreachable_lineage" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/history_remove_unreachable_lineage.sql"
        )),
        "history_remove_unreachable_commit_lineage" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/history_remove_unreachable_commit_lineage.sql"
        )),
        "history_remove_unreachable_absorbed" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/history_remove_unreachable_absorbed.sql"
        )),
        "history_remove_unreachable_reach" => Ok(include_str!(
            "../../../../sql/sqlite/queries/history/history_remove_unreachable_reach.sql"
        )),
        _ => Err(BackendError::Integrity),
    }
}
pub(crate) fn run(
    connection: &Connection,
    sql: &str,
    params: Vec<Param>,
    work: &RefCell<SqlWork>,
) -> Result<Vec<Record>, BackendError> {
    let values: Vec<_> = params.into_iter().map(rows::bind).collect();
    let bytes = values
        .iter()
        .map(|v| match v {
            rusqlite::types::Value::Blob(v) => v.len(),
            rusqlite::types::Value::Text(v) => v.len(),
            _ => 8,
        })
        .sum::<usize>() as u64;
    let bindings: Vec<&dyn rusqlite::ToSql> =
        values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    borrowed(connection, sql, &bindings, bytes, work)
}
pub(crate) fn borrowed(
    connection: &Connection,
    sql: &str,
    values: &[&dyn rusqlite::ToSql],
    bytes: u64,
    work: &RefCell<SqlWork>,
) -> Result<Vec<Record>, BackendError> {
    mapped(connection, sql, values, bytes, 0, work, |row| {
        rows::record(row, row.as_ref().column_count())
    })
}
pub(crate) fn mapped<T>(
    connection: &Connection,
    sql: &str,
    values: &[&dyn rusqlite::ToSql],
    bytes: u64,
    capacity: usize,
    work: &RefCell<SqlWork>,
    decode: impl FnMut(&rusqlite::Row<'_>) -> Result<T, BackendError>,
) -> Result<Vec<T>, BackendError> {
    super::prepared::Prepared::new(connection, sql, work, None)?
        .mapped(values, bytes, capacity, decode)
}

pub(crate) fn batch(
    connection: &Connection,
    sql: &str,
    work: &RefCell<SqlWork>,
) -> Result<(), BackendError> {
    use rusqlite::fallible_iterator::FallibleIterator;
    let start = Instant::now();
    let result = (|| {
        let mut batch = rusqlite::Batch::new(connection, sql);
        while let Some(mut statement) = batch.next().map_err(rows::error)? {
            let result = statement.execute([]).map_err(rows::error);
            let mut w = work.borrow_mut();
            w.statements += 1;
            w.vm_steps += statement.get_status(StatementStatus::VmStep).max(0) as u64;
            w.fullscan_steps += statement.get_status(StatementStatus::FullscanStep).max(0) as u64;
            w.sorts += statement.get_status(StatementStatus::Sort).max(0) as u64;
            w.autoindex_rows += statement.get_status(StatementStatus::AutoIndex).max(0) as u64;
            w.reprepares += statement.get_status(StatementStatus::RePrepare).max(0) as u64;
            result?;
        }
        Ok(())
    })();
    work.borrow_mut().statement_ns += start.elapsed().as_nanos() as u64;
    result
}
