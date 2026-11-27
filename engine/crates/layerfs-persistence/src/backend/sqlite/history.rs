//! Shipped history statement texts by key, portable to both engines;
//! PostgreSQL's `history/statements.rs` overrides some and falls back here.
use crate::backend::error::BackendError;

pub(crate) fn history(key: &str) -> Result<&'static str, BackendError> {
    match key {
        "statistics_totals" => Ok(include_str!(
            "../../../sql/sqlite/history/statistics/totals.sql"
        )),
        "statistics_record_init" => Ok(include_str!(
            "../../../sql/sqlite/history/statistics/record_init.sql"
        )),
        "allocation_advance_scope" => Ok(include_str!(
            "../../../sql/sqlite/history/allocation/advance_scope.sql"
        )),
        "allocation_insert_scope" => Ok(include_str!(
            "../../../sql/sqlite/history/allocation/insert_scope.sql"
        )),
        "allocation_scope_by_id" => Ok(include_str!(
            "../../../sql/sqlite/history/allocation/scope_by_id.sql"
        )),
        "hint_insert_block" => Ok(include_str!(
            "../../../sql/sqlite/history/index/hint_insert_block.sql"
        )),
        "hint_blocks_of" => Ok(include_str!(
            "../../../sql/sqlite/history/index/hint_blocks_of.sql"
        )),
        "digest_insert" => Ok(include_str!(
            "../../../sql/sqlite/history/index/digest_insert.sql"
        )),
        "digest_of" => Ok(include_str!(
            "../../../sql/sqlite/history/index/digest_of.sql"
        )),
        "branch_snapshot" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/snapshot.sql"
        )),
        "commit_snapshot_epoch" => Ok(include_str!(
            "../../../sql/sqlite/history/commit/snapshot_epoch.sql"
        )),
        "branch_snapshot_named" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/snapshot_named.sql"
        )),
        "lineage_parent" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/parent.sql"
        )),
        "branch_branch_by_id" => Ok(include_str!("../../../sql/sqlite/history/branch/by_id.sql")),
        "branch_branch_by_id_any" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/by_id_any.sql"
        )),
        "branch_retire_insert" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/retire_insert.sql"
        )),
        "branch_branch_by_name" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/by_name.sql"
        )),
        "branch_branch_first_page" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/first_page.sql"
        )),
        "branch_branch_name_taken" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/name_taken.sql"
        )),
        "branch_branch_next_page" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/next_page.sql"
        )),
        "branch_branch_from_page" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/from_page.sql"
        )),
        "branch_move_branch" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/move_branch.sql"
        )),
        "commit_identity_range" => Ok(include_str!(
            "../../../sql/sqlite/history/commit/identity_range.sql"
        )),
        "layerstack_identity_range" => Ok(include_str!(
            "../../../sql/sqlite/history/layer/identity_range.sql"
        )),
        "branch_insert_branch" => Ok(include_str!(
            "../../../sql/sqlite/history/branch/insert_branch.sql"
        )),
        "commit_advance_branch" => Ok(include_str!(
            "../../../sql/sqlite/history/commit/advance_branch.sql"
        )),
        "commit_commit_by_id" => Ok(include_str!("../../../sql/sqlite/history/commit/by_id.sql")),
        "commit_commit_by_attempt" => Ok(include_str!(
            "../../../sql/sqlite/history/commit/by_attempt.sql"
        )),
        "commit_insert_commit" => Ok(include_str!(
            "../../../sql/sqlite/history/commit/insert_commit.sql"
        )),
        "layerstack_advance_stack" => Ok(include_str!(
            "../../../sql/sqlite/history/stack/advance.sql"
        )),
        "layerstack_insert_layer" => {
            Ok(include_str!("../../../sql/sqlite/history/layer/insert.sql"))
        }
        "layerstack_insert_stack" => {
            Ok(include_str!("../../../sql/sqlite/history/stack/insert.sql"))
        }
        "layerstack_layer_by_id" => Ok(include_str!("../../../sql/sqlite/history/layer/by_id.sql")),
        "layerstack_layer_by_source" => Ok(include_str!(
            "../../../sql/sqlite/history/layer/by_source.sql"
        )),
        "layerstack_layer_root" => Ok(include_str!("../../../sql/sqlite/history/layer/root.sql")),
        "layerstack_stack_by_id" => Ok(include_str!("../../../sql/sqlite/history/stack/by_id.sql")),
        "layerstack_stack_first_page" => Ok(include_str!(
            "../../../sql/sqlite/history/stack/first_page.sql"
        )),
        "layerstack_stack_head" => Ok(include_str!("../../../sql/sqlite/history/stack/head.sql")),
        "layerstack_stack_by_name" => Ok(include_str!(
            "../../../sql/sqlite/history/stack/by_name.sql"
        )),
        "layerstack_stack_name_taken" => Ok(include_str!(
            "../../../sql/sqlite/history/stack/name_taken.sql"
        )),
        "layerstack_insert_project_epoch" => {
            Ok(include_str!("../../../sql/sqlite/gc/epoch/insert.sql"))
        }
        "layerstack_bump_project_epoch" => {
            Ok(include_str!("../../../sql/sqlite/gc/epoch/bump.sql"))
        }
        "layerstack_insert_retired" => Ok(include_str!(
            "../../../sql/sqlite/history/stack/insert_retired.sql"
        )),
        "layerstack_deleted_ms" => Ok(include_str!(
            "../../../sql/sqlite/history/stack/deleted_ms.sql"
        )),
        "layerstack_stack_next_page" => Ok(include_str!(
            "../../../sql/sqlite/history/stack/next_page.sql"
        )),
        "lineage_commit" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/commit.sql"
        )),
        "lineage_layer" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/layer.sql"
        )),
        "lineage_layers_above" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/layers_above.sql"
        )),
        "lineage_commit_at" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/commit_at.sql"
        )),
        "lineage_insert_commit" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/insert_commit.sql"
        )),
        "lineage_insert_layer" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/insert_layer.sql"
        )),
        "lineage_merges" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/merges.sql"
        )),
        "lineage_absorbed_below" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/absorbed_below.sql"
        )),
        "lineage_absorbed_height" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/absorbed_height.sql"
        )),
        "lineage_absorb" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/absorb.sql"
        )),
        "lineage_reach" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/reach.sql"
        )),
        "lineage_reach_prune" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/reach_prune.sql"
        )),
        "lineage_reach_insert" => Ok(include_str!(
            "../../../sql/sqlite/history/lineage/reach_insert.sql"
        )),
        "removal_totals" => Ok(include_str!(
            "../../../sql/sqlite/history/removal/totals.sql"
        )),
        "removal_absorbed" => Ok(include_str!(
            "../../../sql/sqlite/history/removal/absorbed.sql"
        )),
        "removal_reach" => Ok(include_str!(
            "../../../sql/sqlite/history/removal/reach.sql"
        )),
        "removal_commit_lineage" => Ok(include_str!(
            "../../../sql/sqlite/history/removal/commit_lineage.sql"
        )),
        "removal_layer_lineage" => Ok(include_str!(
            "../../../sql/sqlite/history/removal/layer_lineage.sql"
        )),
        "removal_branches" => Ok(include_str!(
            "../../../sql/sqlite/history/removal/branches.sql"
        )),
        "removal_commits" => Ok(include_str!(
            "../../../sql/sqlite/history/removal/commits.sql"
        )),
        "removal_layers" => Ok(include_str!(
            "../../../sql/sqlite/history/removal/layers.sql"
        )),
        "removal_stack" => Ok(include_str!(
            "../../../sql/sqlite/history/removal/stack.sql"
        )),
        "position_high_water" => Ok(include_str!(
            "../../../sql/sqlite/history/position/high_water.sql"
        )),
        "position_stack_high_water" => Ok(include_str!(
            "../../../sql/sqlite/history/position/stack_high_water.sql"
        )),
        "position_next_serial" => Ok(include_str!(
            "../../../sql/sqlite/history/position/next_serial.sql"
        )),
        "position_node_serial" => Ok(include_str!(
            "../../../sql/sqlite/history/position/node_serial.sql"
        )),
        "paths_marker" => Ok(include_str!("../../../sql/sqlite/history/paths/marker.sql")),
        "paths_markers" => Ok(include_str!(
            "../../../sql/sqlite/history/paths/markers.sql"
        )),
        "paths_node_markers" => Ok(include_str!(
            "../../../sql/sqlite/history/paths/node_markers.sql"
        )),
        "paths_upsert" => Ok(include_str!("../../../sql/sqlite/history/paths/upsert.sql")),
        "paths_advance_epoch" => Ok(include_str!(
            "../../../sql/sqlite/history/paths/advance_epoch.sql"
        )),
        "gc_intent_insert" => Ok(include_str!("../../../sql/sqlite/gc/intent/insert.sql")),
        "gc_intent_release" => Ok(include_str!("../../../sql/sqlite/gc/intent/release.sql")),
        "gc_intent_release_done" => Ok(include_str!(
            "../../../sql/sqlite/gc/intent/release_done.sql"
        )),
        "gc_intent_uncertain" => Ok(include_str!("../../../sql/sqlite/gc/intent/uncertain.sql")),
        "gc_intent_list" => Ok(include_str!("../../../sql/sqlite/gc/intent/list.sql")),
        "gc_intent_ops_branch" => Ok(include_str!(
            "../../../sql/sqlite/gc/intent_scope/ops_branch.sql"
        )),
        "gc_intent_ops_project" => Ok(include_str!(
            "../../../sql/sqlite/gc/intent_scope/ops_project.sql"
        )),
        "gc_intent_ops_store" => Ok(include_str!(
            "../../../sql/sqlite/gc/intent_scope/ops_store.sql"
        )),
        "gc_intent_all_branch" => Ok(include_str!(
            "../../../sql/sqlite/gc/intent_scope/all_branch.sql"
        )),
        "gc_intent_all_project" => Ok(include_str!(
            "../../../sql/sqlite/gc/intent_scope/all_project.sql"
        )),
        "gc_intent_all_store" => Ok(include_str!(
            "../../../sql/sqlite/gc/intent_scope/all_store.sql"
        )),
        "gc_intent_list_machine" => Ok(include_str!(
            "../../../sql/sqlite/gc/intent/list_machine.sql"
        )),
        "gc_intent_done" => Ok(include_str!("../../../sql/sqlite/gc/intent/done.sql")),
        "gc_intent_done_branch" => Ok(include_str!(
            "../../../sql/sqlite/gc/intent/done_branch.sql"
        )),
        "gc_intent_done_cap" => Ok(include_str!("../../../sql/sqlite/gc/intent/done_cap.sql")),
        "gc_intent_done_newest" => Ok(include_str!(
            "../../../sql/sqlite/gc/intent/done_newest.sql"
        )),

        "gc_epoch_read" => Ok(include_str!("../../../sql/sqlite/gc/epoch/read.sql")),
        "gc_epoch_share" => Ok(include_str!("../../../sql/sqlite/gc/epoch/share.sql")),
        "gc_epoch_lock" => Ok(include_str!("../../../sql/sqlite/gc/epoch/lock.sql")),
        "gc_live_stacks" => Ok(include_str!("../../../sql/sqlite/gc/mark/live_stacks.sql")),
        "gc_live_branches" => Ok(include_str!(
            "../../../sql/sqlite/gc/mark/live_branches.sql"
        )),
        "gc_inode_blocks" => Ok(include_str!("../../../sql/sqlite/gc/mark/inode_blocks.sql")),
        "gc_pack_lengths" => Ok(include_str!("../../../sql/sqlite/gc/mark/pack_lengths.sql")),
        "gc_tuple_read" => Ok(include_str!("../../../sql/sqlite/gc/mark/tuple_read.sql")),
        "kept_node_clear" => Ok(include_str!("../../../sql/sqlite/gc/kept_node/clear.sql")),
        "kept_node_seed" => Ok(include_str!("../../../sql/sqlite/gc/kept_node/seed.sql")),
        "kept_node_expand" => Ok(include_str!("../../../sql/sqlite/gc/kept_node/expand.sql")),
        "kept_node_page" => Ok(include_str!("../../../sql/sqlite/gc/kept_node/page.sql")),
        "kept_node_roots" => Ok(include_str!("../../../sql/sqlite/gc/kept_node/roots.sql")),
        "kept_node_kept_roots" => Ok(include_str!(
            "../../../sql/sqlite/gc/kept_node/kept_roots.sql"
        )),
        "history_unreachable_count" => Ok(include_str!(
            "../../../sql/sqlite/history/statistics/unreachable_count.sql"
        )),
        "history_remove_unreachable_totals" => Ok(include_str!(
            "../../../sql/sqlite/gc/unreachable_removal/totals.sql"
        )),
        "history_remove_unreachable_commit" => Ok(include_str!(
            "../../../sql/sqlite/gc/unreachable_removal/commit.sql"
        )),
        "history_remove_unreachable_layer" => Ok(include_str!(
            "../../../sql/sqlite/gc/unreachable_removal/layer.sql"
        )),
        "history_remove_unreachable_branch" => Ok(include_str!(
            "../../../sql/sqlite/gc/unreachable_removal/branch.sql"
        )),
        "history_remove_unreachable_stack" => Ok(include_str!(
            "../../../sql/sqlite/gc/unreachable_removal/stack.sql"
        )),
        "history_remove_unreachable_lineage" => Ok(include_str!(
            "../../../sql/sqlite/gc/unreachable_removal/lineage.sql"
        )),
        "history_remove_unreachable_commit_lineage" => Ok(include_str!(
            "../../../sql/sqlite/gc/unreachable_removal/commit_lineage.sql"
        )),
        "history_remove_unreachable_absorbed" => Ok(include_str!(
            "../../../sql/sqlite/gc/unreachable_removal/absorbed.sql"
        )),
        "history_remove_unreachable_reach" => Ok(include_str!(
            "../../../sql/sqlite/gc/unreachable_removal/reach.sql"
        )),
        _ => Err(BackendError::Integrity),
    }
}
