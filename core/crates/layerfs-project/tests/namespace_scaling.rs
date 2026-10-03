//! Collection-count qualification, not a whole-importer resident-memory proof.
mod support;
use std::sync::Arc;
use support::*;
#[test]
fn retained_collection_counts_expose_entry_count_growth() {
    for count in [100, 1000] {
        let fixture = Fixture::new(count);
        let storage = storage(Arc::new(memory_metadata::MemoryMetadata::default()));
        let history = memory_history::MemoryHistory::default();
        let initialized = fixture.run(&storage, &history);
        let w = initialized.namespace_work;
        assert_eq!(w.entries, count + 11);
        assert_eq!(w.jobs, count);
        assert_eq!(w.directory_bindings, count + 10);
        assert!(w.entry_capacity_bytes >= w.entries * std::mem::size_of::<usize>());
        assert!(w.job_capacity_bytes > w.jobs * std::mem::size_of::<usize>());
        assert!(w.serial_capacity_bytes >= w.entries * 8);
        assert!(w.inode_capacity_bytes > 0);
        assert_eq!(w.directory_children, (count / 10).max(10));
        println!("DIAGNOSTIC namespace-count-{count} {w:?}");
    }
}
