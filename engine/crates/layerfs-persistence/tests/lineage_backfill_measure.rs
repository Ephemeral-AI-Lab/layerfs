//! #353 measurement, run by hand on a copy of a real Store: one writable open
//! that backfills the lineage tables, then one plain writable open of the
//! migrated copy. Ignored by default; set `LAYERFS_353_STORE` to a working
//! copy (it is migrated in place) and `LAYERFS_353_BINDING` to the Store's
//! binding key in hex. Schema 22: monolithic packs with acquisition tables.
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use std::time::Instant;

fn open(path: &str, binding: &[u8]) -> f64 {
    let start = Instant::now();
    let handles = Handles::open_writable(
        PersistenceConfig::sqlite(path)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        binding,
        [53; 32],
    )
    .expect("open writable");
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    drop(handles);
    elapsed
}

#[test]
#[ignore]
fn backfill_on_a_store_copy() {
    let path = std::env::var("LAYERFS_353_STORE").expect("LAYERFS_353_STORE");
    let hex = std::env::var("LAYERFS_353_BINDING").expect("LAYERFS_353_BINDING");
    let binding: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let migrating = open(&path, &binding);
    let plain = open(&path, &binding);
    println!("LINEAGE_BACKFILL open_with_backfill_ms={migrating:.1} open_after_ms={plain:.1} profile=Disposable samples=1");
}
