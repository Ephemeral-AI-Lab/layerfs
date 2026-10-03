//! One complete durable direct-API Init; fresh database creation belongs to work.
use layerfs_history::{HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{Handles, PersistenceConfig};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{
    fmt::Write as _,
    path::Path,
    time::{Duration, Instant},
};
fn hex(bytes: &[u8]) -> String {
    let mut s = String::new();
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 5 {
        return Err("source fresh-database scratch name required".into());
    }
    let start = Instant::now();
    let handles = Handles::create(
        PersistenceConfig::sqlite(&args[2]),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"layerfs-bench-pro".to_vec(),
            incarnation: 1,
            cursor_key: [0x28; 32],
        },
    )?;
    let storage = Storage::new(handles.storage.clone())?;
    let bootstrap_ns = start.elapsed().as_nanos();
    let init_start = Instant::now();
    let result = Timing::disabled("project.init", |timer| {
        init(
            &storage,
            &handles.history,
            InitRequest {
                source: Path::new(&args[1]),
                scratch_parent: Path::new(&args[3]),
                stack: LayerStackId::from_authority([0x41; 16]),
                name: HistoryName::new(&args[4]).unwrap(),
                scope_seed: [0x42; 32],
                deadline: start + Duration::from_secs(15),
            },
            timer,
        )
    })
    .0?;
    let init_ns = init_start.elapsed().as_nanos();
    let checkpoint = handles.checkpoint()?;
    if checkpoint.busy {
        return Err("final checkpoint obstructed".into());
    }
    let work = handles.diagnostics()?;
    let profile = handles.profile().clone();
    let save_work = storage.save_work();
    let close = Instant::now();
    drop(storage);
    drop(handles);
    let close_ns = close.elapsed().as_nanos();
    let complete_product_ns = start.elapsed().as_nanos();
    println!("{{\"status\":\"COMPLETE\",\"operation_ns\":{complete_product_ns},\"bootstrap_ns\":{bootstrap_ns},\"init_ns\":{init_ns},\"checkpoint_ns\":{},\"close_ns\":{close_ns},\"root\":\"{}\",\"stack\":\"{}\",\"root_serial\":{},\"entries\":{}}}",checkpoint.wall_ns,hex(result.root.as_bytes()),hex(&[0x41;16]),result.root_serial,result.entries);
    eprintln!("DIAGNOSTIC profile={profile:?}");
    eprintln!("DIAGNOSTIC sqlite={work:?} checkpoint={checkpoint:?} storage={:?} saves={save_work:?} namespace={:?}",result.diagnostics,result.namespace_work);
    Ok(())
}
