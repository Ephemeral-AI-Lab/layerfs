//! Backed-count growth with fixed resident buffers; not a whole-importer memory proof.
mod support;
use std::sync::Arc;
use support::*;
#[test]
fn backed_counts_grow_while_resident_buffers_stay_fixed() {
    let mut windows = Vec::new();
    let mut backing = Vec::new();
    for count in [100, 3000, 6000] {
        let fixture = Fixture::new(count);
        let storage = storage(Arc::new(memory_metadata::MemoryMetadata::default()));
        let history = memory_history::MemoryHistory::default();
        let initialized = fixture.run(&storage, &history);
        let w = initialized.namespace_work;
        assert_eq!(w.entries, count + 11);
        assert_eq!(w.jobs, count);
        assert_eq!(w.unique_files, count);
        assert_eq!(w.regular_aliases, 0);
        assert_eq!(w.directory_bindings, count + 10);
        assert_eq!(w.directory_children, (count / 10).max(10));
        assert_eq!(w.frontier, 10);
        // No serial, directory-update or binding collection is resident at all.
        assert_eq!(w.serial_capacity_bytes, 0);
        assert_eq!(w.directory_capacity_bytes, 0);
        assert_eq!(w.change_capacity_bytes, 0);
        assert!(w.entry_capacity_bytes > 0 && w.job_capacity_bytes > 0);
        assert!(w.inode_capacity_bytes > 0 && w.child_vector_bytes > 0);
        assert!(w.frontier_capacity_bytes > 0 && w.sort_capacity_bytes > 0);
        windows.push((
            w.entry_capacity_bytes,
            w.job_capacity_bytes,
            w.inode_capacity_bytes,
            w.child_vector_bytes,
            w.frontier_capacity_bytes,
        ));
        backing.push(w.backing_bytes);
        println!("DIAGNOSTIC namespace-count-{count} {w:?}");
    }
    // Read buffers, the job queue and the child buffer are fixed: a larger root
    // holds no larger one. The 6000-entry root also orders its 600-child
    // directories in backing.
    assert_eq!(windows[1], windows[2]);
    assert!(backing[0] < backing[1] && backing[1] < backing[2]);
}
