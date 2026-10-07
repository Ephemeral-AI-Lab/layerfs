//! Current public Store/Storage observations, kept separate from engine families.
use super::{engine_records, json::Json};
use layerfs_daemon::{bootstrap::StoreDiagnostics, store::CommitSuccess};
use layerfs_overlay::DatabaseWork;
use layerfs_persistence::SqlWork;
use std::{fs::File, io::Write};
fn sql(out: &mut Json, from: SqlWork, to: SqlWork) {
    out.raw("{").unwrap();
    let mut comma = false;
    macro_rules! fields {($($field:ident),+)=>{$(if comma{out.raw(",").unwrap();}comma=true;out.field(stringify!($field),to.$field.checked_sub(from.$field).unwrap()).unwrap();)+};}
    fields!(
        statements,
        vm_steps,
        fullscan_steps,
        sorts,
        autoindex_rows,
        reprepares,
        returned_rows,
        bound_bytes,
        statement_ns,
        commit_ns,
        transactions,
        write_transactions,
        write_commits,
        commits,
        read_snapshots,
        rollbacks,
        transaction_ns,
        read_snapshot_ns,
        sealed_inserts,
        sealed_body_bytes,
        blob_open_calls,
        blob_reopen_calls,
        blob_read_calls,
        blob_requested_bytes,
        blob_read_bytes,
        blob_read_ns,
        blob_close_calls
    );
    assert!(comma);
    for (name, before, after) in [
        (
            "statement_phases",
            from.statement_phases,
            to.statement_phases,
        ),
        ("commit_phases", from.commit_phases, to.commit_phases),
    ] {
        out.raw(",").unwrap();
        out.string(name).unwrap();
        out.raw(":[").unwrap();
        for (i, (old, new)) in before.into_iter().zip(after).enumerate() {
            if i > 0 {
                out.raw(",").unwrap();
            }
            out.raw("{").unwrap();
            out.field("calls", new.calls.checked_sub(old.calls).unwrap())
                .unwrap();
            out.raw(",").unwrap();
            out.field("wall_ns", new.wall_ns.checked_sub(old.wall_ns).unwrap())
                .unwrap();
            out.raw("}").unwrap();
        }
        out.raw("]").unwrap();
    }
    out.raw("}").unwrap();
}
pub fn commit(
    file: &mut File,
    kind: &str,
    original: &CommitSuccess,
    before: StoreDiagnostics,
    after: StoreDiagnostics,
    owner: DatabaseWork,
    local: (layerfs_daemon::OwnerWork, layerfs_daemon::OwnerWork),
) {
    let (local_before, local_after) = local;
    let mut out = Json::new();
    out.raw("{\"schema\":\"cluster-two-store-accounting-v1\",\"attempt_count\":1,\"mode\":\"diagnostic\",\"namespace_files\":100000,").unwrap();
    out.text("outcome", kind).unwrap();
    let (head, root) = match &original.history {
        layerfs_history::CommitStagedOutcome::Committed(record) => (Some(record.id), record.root),
        layerfs_history::CommitStagedOutcome::UpToDate { head, root } => (*head, *root),
    };
    out.raw(",").unwrap();
    out.text("root", &root.to_string()).unwrap();
    out.raw(",\"head\":").unwrap();
    match head {
        Some(id) => out.string(&id.to_string()).unwrap(),
        None => out.raw("null").unwrap(),
    };

    out.raw(",\"engine_capture\":").unwrap();
    engine_records::database(&mut out, &original.captured.work().sql.expanded()).unwrap();
    out.raw(",\"engine_install\":").unwrap();
    engine_records::database(&mut out, &original.installed.work().sql.expanded()).unwrap();
    out.raw(",\"engine_whole_commit\":").unwrap();
    engine_records::database(&mut out, &owner).unwrap();
    out.raw(",\"engine_payload\":").unwrap();
    engine_records::payload(
        &mut out,
        local_after
            .payload_foreground
            .since(local_before.payload_foreground),
    )
    .unwrap();
    out.raw(",\"engine_allocation\":").unwrap();
    engine_records::allocation(
        &mut out,
        local_after
            .allocation_foreground
            .since(local_before.allocation_foreground),
    )
    .unwrap();
    out.raw(",\"store_writer\":").unwrap();
    sql(&mut out, before.writer, after.writer);
    assert_eq!(before.readers.len(), after.readers.len());
    out.raw(",\"store_readers\":[").unwrap();
    for (i, (from, to)) in before.readers.into_iter().zip(after.readers).enumerate() {
        if i > 0 {
            out.raw(",").unwrap();
        }
        sql(&mut out, from, to);
    }
    out.raw("],\"storage\":{").unwrap();
    let d = original.storage;
    let mut comma = false;
    macro_rules! fields {($($field:ident),+)=>{$(if comma{out.raw(",").unwrap();}comma=true;out.field(stringify!($field),d.$field).unwrap();)+};}
    fields!(
        policy,
        locate,
        read_packs,
        read_pack_selections,
        whole_selected,
        whole_due_density,
        whole_due_singleton,
        whole_due_small,
        whole_due_reuse,
        range_selected,
        range_scan_bytes,
        range_acquired_bytes,
        range_materialized_bytes,
        value_groups,
        signatures,
        reserve,
        initial_reservations,
        reservation_refills,
        publish,
        payload_reads,
        payload_read_bytes,
        pack_read_bytes,
        pack_write_bytes,
        pooled_packs,
        pooled_groups,
        reserved_directory_bytes,
        ordinal_reservations,
        locator_hits,
        locator_misses,
        locator_evictions,
        locator_eviction_probes,
        locator_second_chances,
        locator_bookkeeping_live_peak_bytes,
        pack_hits,
        pack_misses,
        pack_evictions,
        pack_evicted_bytes,
        directory_validations,
        directory_unshared_walks,
        directory_entry_bounds,
        prefetch_group_decodes,
        forced_seals
    );
    assert!(comma);
    out.raw("},\"limitations\":[\"Store clocks overlap; statement-phase subcounters are not exclusive operation time\",\"Store BLOB byte counts are not physical I/O\",\"family work is engine-scoped; Store counters are handle-scoped\",\"internal SQLite and driver copies are not supplied by these APIs\"]}").unwrap();
    file.write_all(&out.finish().unwrap()).unwrap();
}
