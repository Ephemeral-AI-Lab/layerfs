//! Fixed numeric schema order, documented in the application architecture.
use super::Records;
use crate::control::{Failure, Success};
use std::io::{self, Write};

pub(super) fn owner<W: Write>(
    rows: &mut Records<'_, W>,
    work: Option<crate::OwnerWork>,
) -> io::Result<()> {
    let Some(w) = work else {
        rows.row(1, 0, None)?;
        for family in 0..14 {
            rows.row(2, family, None)?;
            rows.row(3, family, None)?;
        }
        for section in 4..=7 {
            rows.row(section, 0, None)?;
        }
        return Ok(());
    };
    rows.row(
        1,
        0,
        Some(&[
            w.admitted,
            w.credited_bytes as u64,
            w.peak_credited_bytes as u64,
            w.outstanding as u64,
            w.scheduler_bytes as u64,
            w.queued as u64,
            w.peak_queued as u64,
            w.receipt_overruns,
            w.receipt_overrun_bytes as u64,
            w.maintenance_jobs,
            w.maintenance_rows,
            w.closed_namespaces,
            w.maintenance_data_bytes,
            w.maintenance_ns,
            w.completed[0],
            w.completed[1],
            w.completed[2],
            w.completed[3],
            w.completed[4],
            w.completed[5],
            w.queue_wait_ns[0],
            w.queue_wait_ns[1],
            w.queue_wait_ns[2],
            w.queue_wait_ns[3],
            w.queue_wait_ns[4],
            w.queue_wait_ns[5],
            w.service_ns[0],
            w.service_ns[1],
            w.service_ns[2],
            w.service_ns[3],
            w.service_ns[4],
            w.service_ns[5],
        ]),
    )?;
    for family in 0..14 {
        statement(rows, 2, family as i64, w.sql_foreground.statements[family])?;
        statement(rows, 3, family as i64, w.sql_maintenance.statements[family])?;
    }
    rows.row(
        4,
        0,
        Some(&[
            w.payload_foreground.write_input_bytes,
            w.payload_foreground.write_cells,
            w.payload_foreground.partial_write_cells,
            w.payload_foreground.cell_copy_bytes,
            w.payload_foreground.cell_zeroed_bytes,
            w.payload_foreground.read_window_zeroed_bytes,
            w.payload_foreground.read_local_copy_bytes,
        ]),
    )?;
    rows.row(
        5,
        0,
        Some(&[
            w.payload_maintenance.write_input_bytes,
            w.payload_maintenance.write_cells,
            w.payload_maintenance.partial_write_cells,
            w.payload_maintenance.cell_copy_bytes,
            w.payload_maintenance.cell_zeroed_bytes,
            w.payload_maintenance.read_window_zeroed_bytes,
            w.payload_maintenance.read_local_copy_bytes,
        ]),
    )?;
    rows.row(
        6,
        0,
        Some(&[
            w.allocation_foreground.attempts,
            w.allocation_foreground.requested_bytes,
            w.allocation_foreground.admitted_jobs,
            w.allocation_foreground.refusals,
            w.allocation_foreground.observations,
            w.allocation_foreground.freelist_queries,
        ]),
    )?;
    rows.row(
        7,
        0,
        Some(&[
            w.allocation_maintenance.attempts,
            w.allocation_maintenance.requested_bytes,
            w.allocation_maintenance.admitted_jobs,
            w.allocation_maintenance.refusals,
            w.allocation_maintenance.observations,
            w.allocation_maintenance.freelist_queries,
        ]),
    )?;
    Ok(())
}
fn statement<W: Write>(
    rows: &mut Records<'_, W>,
    section: u16,
    index: i64,
    w: layerfs_overlay::StatementWork,
) -> io::Result<()> {
    rows.row(
        section,
        index,
        Some(&[
            w.attempts,
            w.executions,
            w.rows_returned,
            w.rows_changed,
            w.direct_rows_changed,
            w.returned_blob_bytes,
            w.returned_value_bytes,
            w.vm_steps,
            w.fullscan_steps,
            w.sorts,
            w.autoindex_rows,
            w.reprepares,
            w.bound_bytes,
            w.sql_bytes,
            w.statement_memory_samples,
            w.statement_memory_sample_bytes,
            w.elapsed_ns,
        ]),
    )?;
    Ok(())
}
pub(super) fn store<W: Write>(
    rows: &mut Records<'_, W>,
    store: &crate::store::Store,
) -> io::Result<()> {
    let w = store.work();
    rows.row(
        8,
        0,
        Some(&[
            w.object_batches,
            w.object_ids,
            w.length_batches,
            w.length_ids,
            w.serial_reservations,
        ]),
    )?;
    let w = store.read_work();
    rows.row(
        9,
        0,
        Some(&[
            w.stopping as u64,
            w.poisoned as u64,
            w.readers as u64,
            w.quarantined as u64,
            w.waiting as u64,
            w.assigned as u64,
            w.leased as u64,
            w.outstanding as u64,
            w.peak_outstanding as u64,
            w.grants,
            w.queue_wait_ns,
            w.maximum_queue_wait_ns,
            w.scheduler_bytes as u64,
        ]),
    )?;
    match store.cache_work() {
        Ok(w) => {
            rows.row(
                10,
                0,
                Some(&[
                    w.cache_hits,
                    w.cache_misses,
                    w.upstream_batches,
                    w.authenticated_bytes,
                    w.evictions,
                    w.charged_cache_bytes as u64,
                    w.cached_objects as u64,
                ]),
            )?;
            Ok(())
        }
        Err(_) => rows.row(10, 0, None),
    }
}
#[cfg(target_os = "linux")]
pub(super) fn dispatch<W: Write>(
    rows: &mut Records<'_, W>,
    work: Option<layerfs_fuse::DispatchWork>,
) -> io::Result<()> {
    match work {
        Some(w) => rows.row(
            11,
            0,
            Some(&[
                w.configured_workers as u64,
                w.entered_workers as u64,
                w.live_workers as u64,
                w.mounts as u64,
                w.queued as u64,
                w.running as u64,
                w.parked as u64,
                w.retained as u64,
                w.stopping as u64,
                w.failed as u64,
            ]),
        ),
        None => rows.row(11, 0, None),
    }
}
pub(super) fn sql<W: Write>(
    rows: &mut Records<'_, W>,
    section: u16,
    index: i64,
    work: Option<layerfs_persistence::SqlWork>,
) -> io::Result<()> {
    let Some(w) = work else {
        return rows.row(section, index, None);
    };
    rows.row(
        section,
        index,
        Some(&[
            w.statements,
            w.vm_steps,
            w.fullscan_steps,
            w.sorts,
            w.autoindex_rows,
            w.reprepares,
            w.returned_rows,
            w.bound_bytes,
            w.statement_ns,
            w.commit_ns,
            w.transactions,
            w.write_transactions,
            w.write_commits,
            w.commits,
            w.read_snapshots,
            w.rollbacks,
            w.transaction_ns,
            w.read_snapshot_ns,
            w.sealed_inserts,
            w.sealed_body_bytes,
            w.blob_open_calls,
            w.blob_reopen_calls,
            w.blob_read_calls,
            w.blob_requested_bytes,
            w.blob_read_bytes,
            w.blob_read_ns,
            w.blob_close_calls,
            w.statement_phases[0].calls,
            w.statement_phases[0].wall_ns,
            w.statement_phases[1].calls,
            w.statement_phases[1].wall_ns,
            w.statement_phases[2].calls,
            w.statement_phases[2].wall_ns,
            w.statement_phases[3].calls,
            w.statement_phases[3].wall_ns,
            w.statement_phases[4].calls,
            w.statement_phases[4].wall_ns,
            w.statement_phases[5].calls,
            w.statement_phases[5].wall_ns,
            w.statement_phases[6].calls,
            w.statement_phases[6].wall_ns,
            w.commit_phases[0].calls,
            w.commit_phases[0].wall_ns,
            w.commit_phases[1].calls,
            w.commit_phases[1].wall_ns,
            w.commit_phases[2].calls,
            w.commit_phases[2].wall_ns,
            w.commit_phases[3].calls,
            w.commit_phases[3].wall_ns,
            w.commit_phases[4].calls,
            w.commit_phases[4].wall_ns,
            w.commit_phases[5].calls,
            w.commit_phases[5].wall_ns,
            w.commit_phases[6].calls,
            w.commit_phases[6].wall_ns,
        ]),
    )?;
    Ok(())
}
pub(super) fn construction<W: Write>(
    rows: &mut Records<'_, W>,
    outcome: &Result<Success, Failure>,
) -> io::Result<()> {
    let (namespace, storage) = match outcome {
        Ok(success) => success
            .commit
            .as_ref()
            .map(|commit| (commit.namespace.as_ref(), Some(&commit.storage)))
            .unwrap_or((None, None)),
        Err(Failure::Commit(failed)) => (failed.namespace.as_ref(), failed.storage.as_ref()),
        Err(Failure::After { original, .. }) => original
            .commit
            .as_ref()
            .map(|commit| (commit.namespace.as_ref(), Some(&commit.storage)))
            .unwrap_or((None, None)),
        _ => (None, None),
    };
    if let Some(w) = namespace.and_then(|namespace| namespace.work) {
        rows.row(
            15,
            0,
            Some(&[
                w.inode_rows,
                w.entry_rows,
                w.inode_pages,
                w.entry_pages,
                w.change_pages,
                w.largest_page,
                w.parent_points,
                w.base_lookups,
                w.headers_written,
                w.headers_dropped,
                w.values_written,
                w.fresh_serials,
                w.tombstones_skipped,
                w.files_constructed,
                w.files_unchanged,
                w.symlinks,
                w.metadata_built,
                w.metadata_patched,
                w.record_jobs,
                w.header_opens,
                w.header_rows,
                w.header_points,
                w.change_opens,
                w.change_rows,
                w.change_points,
                w.value_opens,
                w.value_rows,
                w.value_points,
                w.fresh_opens,
                w.fresh_rows,
                w.fresh_points,
            ]),
        )?;
    } else {
        rows.row(15, 0, None)?;
    }
    self::storage(rows, 16, 0, storage)?;
    content(rows, namespace.and_then(|namespace| namespace.counters))
}
fn content<W: Write>(
    rows: &mut Records<'_, W>,
    work: Option<layerfs_content::filesystem::FilesystemUpdateCounters>,
) -> io::Result<()> {
    let Some(w) = work else {
        for section in 17..=23 {
            rows.row(section, 0, None)?;
        }
        return Ok(());
    };
    rows.row(
        17,
        0,
        Some(&[
            w.objects.objects_read,
            w.objects.read_waves,
            w.objects.bytes_read,
            w.objects.objects_emitted,
            w.objects.bytes_emitted,
        ]),
    )?;
    rows.row(
        18,
        0,
        Some(&[
            w.directories.pages_read,
            w.directories.read_waves,
            w.directories.pages_created,
            w.directories.pages_reused,
            w.directories.change_keys,
            w.directories.untouched_subtrees,
            w.directories.peak_scratch_bytes as u64,
        ]),
    )?;
    rows.row(
        19,
        0,
        Some(&[
            w.inodes.pages_read,
            w.inodes.read_waves,
            w.inodes.pages_created,
            w.inodes.pages_reused,
            w.inodes.change_keys,
            w.inodes.untouched_subtrees,
            w.inodes.peak_scratch_bytes as u64,
        ]),
    )?;
    rows.row(
        20,
        0,
        Some(&[
            w.references.rows_touched,
            w.references.rows_spilled,
            w.references.base_records_read,
            w.references.base_waves,
            w.references.final_values,
            w.references.final_removals,
            w.references.serials_scanned,
            w.references.peak_pending as u64,
            w.references.runs.rows_written,
            w.references.runs.rows_read,
            w.references.runs.runs_created,
            w.references.runs.merges,
            w.references.runs.peak_level as u64,
            w.references.runs.peak_live_runs as u64,
            w.references.runs.peak_run_bytes,
        ]),
    )?;
    rows.row(
        21,
        0,
        Some(&[
            w.release.pages,
            w.release.entries,
            w.release.base_records,
            w.release.released,
            w.release.traversed_directories,
            w.release.peak_depth as u64,
        ]),
    )?;
    rows.row(
        22,
        0,
        Some(&[
            w.validation.objects_read,
            w.validation.read_waves,
            w.validation.inode_demands,
            w.validation.inode_pages_read,
            w.validation.directory_pages_read,
            w.validation.entries_examined,
            w.validation.inode_pages_by_site.allocation,
            w.validation.inode_pages_by_site.prefetch,
            w.validation.inode_pages_by_site.bindings,
            w.validation.inode_pages_by_site.aliases,
            w.validation.inode_pages_by_site.cycles,
            w.validation.inode_pages_by_site.reachability,
            w.validation.placements,
            w.validation.ancestry_steps,
            w.validation.territory_directories,
            w.validation.territory_entries,
            w.validation.peak_window_rows,
            w.validation.in_place_scans,
            w.validation.in_place_rows,
            w.validation.in_place_directories,
        ]),
    )?;
    rows.row(
        23,
        0,
        Some(&[
            w.bindings_added,
            w.bindings_removed,
            w.directory_updates,
            w.base_records_read,
        ]),
    )?;
    Ok(())
}

pub(super) fn reader_storage<W: Write>(
    rows: &mut Records<'_, W>,
    index: i64,
    work: Option<layerfs_storage::Diagnostics>,
) -> io::Result<()> {
    storage(rows, 29, index, work.as_ref())
}
fn storage<W: Write>(
    rows: &mut Records<'_, W>,
    section: u16,
    index: i64,
    work: Option<&layerfs_storage::Diagnostics>,
) -> io::Result<()> {
    if let Some(w) = work {
        rows.row(
            section,
            index,
            Some(&[
                w.policy,
                w.locate,
                w.read_packs,
                w.read_pack_selections,
                w.whole_selected,
                w.whole_due_density,
                w.whole_due_singleton,
                w.whole_due_small,
                w.whole_due_reuse,
                w.range_selected,
                w.range_scan_bytes,
                w.range_acquired_bytes,
                w.range_materialized_bytes,
                w.value_groups,
                w.signatures,
                w.reserve,
                w.initial_reservations,
                w.reservation_refills,
                w.publish,
                w.payload_reads,
                w.payload_read_bytes,
                w.pack_read_bytes,
                w.pack_write_bytes,
                w.pooled_packs,
                w.pooled_groups,
                w.reserved_directory_bytes,
                w.ordinal_reservations,
                w.locator_hits,
                w.locator_misses,
                w.locator_evictions,
                w.locator_eviction_probes,
                w.locator_second_chances,
                w.locator_bookkeeping_live_peak_bytes,
                w.pack_hits,
                w.pack_misses,
                w.pack_evictions,
                w.pack_evicted_bytes,
                w.directory_validations,
                w.directory_unshared_walks,
                w.directory_entry_bounds,
                w.prefetch_group_decodes,
                w.forced_seals,
            ]),
        )?;
    } else {
        rows.row(section, index, None)?;
    }
    Ok(())
}
