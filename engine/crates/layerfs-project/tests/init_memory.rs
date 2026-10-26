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
