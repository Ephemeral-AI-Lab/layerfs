//! Untimed fresh-service schema bootstrap; never opens a workload input.
use layerfs_history::HistoryCatalogConfig;
use layerfs_metadata::{PgConfig, PgHistory, PgMetadata};
use layerfs_storage::StoragePolicy;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = PgConfig::from_env()?;
    let metadata = PgMetadata::create(config.clone(), StoragePolicy::frozen_default())?;
    let history = PgHistory::create(
        config,
        &HistoryCatalogConfig {
            binding_key: b"layerfs-bench-pro".to_vec(),
            incarnation: 1,
            cursor_key: [0x28; 32],
        },
    )?;
    eprintln!(
        "SETUP pg={:?} history_pg={:?}",
        metadata.diagnostics()?,
        history.diagnostics()?
    );
    println!("{{\"status\":\"PASS\",\"scope\":\"empty schema only; no workload input\"}}");
    Ok(())
}
