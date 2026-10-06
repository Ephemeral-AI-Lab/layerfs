mod support;
use std::sync::Arc;
use support::*;
#[test]
fn init_100_and_1000_over_memory_ports_pass_the_full_namespace_oracle() {
    for count in [100, 1000] {
        let fixture = Fixture::new(count);
        let metadata = Arc::new(memory_metadata::MemoryMetadata::default());
        let storage = storage(metadata);
        let history = memory_history::MemoryHistory::default();
        let initialized = fixture.run(&storage, &history);
        fixture.verify_namespace(&storage, &history, &initialized);
        println!(
            "DIAGNOSTIC init-memory-{count} {:?}",
            initialized.diagnostics
        );
    }
}

#[test]
fn scan_and_file_prerequisites_share_a_save_before_tree_publication() {
    let fixture = Fixture::new(20);
    let storage = storage(Arc::new(memory_metadata::MemoryMetadata::default()));
    let history = memory_history::MemoryHistory::default();
    let initialized = fixture.run(&storage, &history);
    fixture.verify_namespace(&storage, &history, &initialized);
    assert!(
        initialized.diagnostics.publish <= 3,
        "the small scan must share a publication with files: {:?}",
        initialized.diagnostics
    );
}
