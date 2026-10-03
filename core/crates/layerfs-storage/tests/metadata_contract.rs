//! The metadata port contract shared verbatim with the PostgreSQL engine.
#[path = "support/metadata_contract.rs"]
mod contract;
#[path = "support/memory_metadata.rs"]
mod memory;
#[test]
fn the_memory_engine_covers_every_metadata_unit() {
    let store = memory::MemoryMetadata::default();
    contract::all_units(&store, &|| {
        let n = store.calls.lock().unwrap().len() as u64;
        contract::Counts {
            submitted: n,
            completed: n,
        }
    });
}
