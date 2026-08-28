use super::super::native::{assert_apple_metadata, assert_managed_root};
use super::super::receipt::{
    operation_without_diagnostics, stage_receipt, OperationReceipt, StageReceipt,
};
use super::super::tree::{assert_tree_equal, read_range};
use crate::legacy_full::{CompactionDiagnostics, LayerFs, OpenedLayerFs, RootId};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub(super) struct CompactionEvidence {
    pub(super) page_size: i64,
    pub(super) cache_pages: i64,
    pub(super) database_bytes: Option<u64>,
    pub(super) logical_engine_bytes: Option<u64>,
    pub(super) operation_q_bound_bytes: u64,
    pub(super) compaction: CompactionDiagnostics,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compact_and_verify(
    store: &Path,
    base: &Path,
    reopened: OpenedLayerFs,
    root_a: RootId,
    root_final: RootId,
    root_diverged: RootId,
    retained_roots: [RootId; 4],
    source_path: &Path,
    reopened_final_path: &Path,
    stages: &mut Vec<StageReceipt>,
    operations: &mut Vec<OperationReceipt>,
) -> Result<CompactionEvidence, Box<dyn std::error::Error>> {
    let operation_started = Instant::now();
    let compacted = reopened.fs.compact(store)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_without_diagnostics(
        "compaction",
        "offline-compaction",
        operation_wall_ns,
        root_final,
        compacted.head,
    ));
    if compacted.head != root_final {
        return Err("S12 compacted head mismatch".into());
    }
    let compacted_diagnostics = compacted.fs.diagnostics()?;
    let compaction = compacted_diagnostics
        .compaction
        .ok_or("missing compaction observation")?;
    drop(compacted);

    let operation_started = Instant::now();
    let post_compact = LayerFs::open(store)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_without_diagnostics(
        "post-compaction-reopen",
        "store-open",
        operation_wall_ns,
        root_final,
        post_compact.head,
    ));
    if post_compact.head != root_final {
        return Err("S12 fresh post-compaction reopen head mismatch".into());
    }
    let retained = post_compact
        .fs
        .materialize_external(root_a, &base.join("post-compact-root-a"))?;
    assert_tree_equal(source_path, retained.path())?;
    drop(retained);
    let retained_final = post_compact
        .fs
        .materialize_external(root_final, &base.join("post-compact-root-final"))?;
    assert_tree_equal(reopened_final_path, retained_final.path())?;
    assert_apple_metadata(&retained_final.path().join("nested/scripts/run.sh"))?;
    let retained_xattr = Command::new("xattr")
        .args(["-p", "com.layerfs.eval"])
        .arg(retained_final.path().join("nested/large.bin"))
        .output()?;
    if !retained_xattr.status.success() || retained_xattr.stdout != b"exact\n" {
        return Err("S12 xattr oracle mismatch".into());
    }
    assert_eq!(
        fs::read(retained_final.path().join("made/moved.txt"))?,
        b"shell"
    );
    assert_eq!(
        read_range(&retained_final.path().join("nested/large.bin"), 32768, 4)?,
        b"MMAP"
    );
    drop(retained_final);
    let retained_diverged = post_compact
        .fs
        .materialize_external(root_diverged, &base.join("post-compact-root-diverged"))?;
    assert_eq!(
        read_range(&retained_diverged.path().join("nested/managed.bin"), 0, 1)?,
        b"D"
    );
    drop(retained_diverged);
    for (index, root) in retained_roots.into_iter().enumerate() {
        let retained = post_compact.fs.materialize_external(
            root,
            &base.join(format!("post-compact-retained-s{}", index + 3)),
        )?;
        assert_managed_root(retained.path(), index + 3)?;
        drop(retained);
    }
    stages.push(stage_receipt("S12", root_final, &post_compact.fs)?);

    Ok(CompactionEvidence {
        page_size: compacted_diagnostics.page_size,
        cache_pages: compacted_diagnostics.cache_pages,
        database_bytes: compacted_diagnostics.database_bytes,
        logical_engine_bytes: compacted_diagnostics.logical_engine_bytes,
        operation_q_bound_bytes: compacted_diagnostics.operation_q_bound_bytes,
        compaction,
    })
}
