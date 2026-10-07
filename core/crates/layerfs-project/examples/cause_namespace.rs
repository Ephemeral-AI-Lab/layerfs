//! Paired public-API diagnostic caller, not the admission Init route.
#![allow(dead_code)]
#[path = "cause_support/batch.rs"]
mod batch;
#[path = "cause_support/engine.rs"]
mod engine;
#[path = "cause_support/error.rs"]
mod error;
#[path = "cause_support/init.rs"]
mod init;
#[path = "cause_support/metadata.rs"]
mod metadata;
#[path = "cause_support/namespace.rs"]
mod namespace;
#[path = "cause_support/namespace_work.rs"]
mod namespace_work;
#[path = "cause_support/observer.rs"]
mod observer;
#[path = "cause_support/scan.rs"]
mod scan;
#[path = "cause_support/sql_observer.rs"]
mod sql_observer;
#[path = "cause_support/vfs_observer.rs"]
mod vfs_observer;
#[rustfmt::skip]
#[allow(clippy::needless_range_loop)]
#[path="../../../benchmark/fs-bench-pro-storage-content/src/workload/digest.rs"]mod digest;
pub use error::{ProjectError, ProjectResult};
use layerfs_history::{HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{Handles, PersistenceConfig};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
pub use namespace_work::NamespaceWork;
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 5 {
        return Err("source fresh-db scratch name required".into());
    }
    if std::env::var("LAYERFS_CONSTRUCTION_WORKERS").as_deref() != Ok("1") {
        return Err("required construction worker env1; Init constructors4".into());
    }
    sql_observer::initialize()?;
    vfs_observer::initialize()?;
    let started = Instant::now();
    let config = HistoryCatalogConfig {
        binding_key: b"layerfs-bench-pro".to_vec(),
        incarnation: 1,
        cursor_key: [0x28; 32],
    };
    let handles = std::sync::Arc::new(Handles::create(
        PersistenceConfig::sqlite(&args[2]),
        StoragePolicy::frozen_default(),
        &config,
    )?);
    let storage = engine::Storage::new(
        layerfs_storage::Storage::new(handles.storage.clone())?,
        handles.clone(),
    );
    let bootstrap_ns = started.elapsed().as_nanos();
    let import = Instant::now();
    let name = HistoryName::new(&args[4])?;
    let result = Timing::disabled("cause.namespace", |t| {
        init::init(
            &storage,
            &handles.history,
            init::InitRequest {
                source: Path::new(&args[1]),
                scratch_parent: Path::new(&args[3]),
                stack: LayerStackId::from_authority([0x41; 16]),
                name,
                scope_seed: [0x42; 32],
                deadline: started + Duration::from_secs(15),
            },
            t,
        )
    })
    .0?;
    let init_ns = import.elapsed().as_nanos();
    let (objects, bytes, inventory) = storage.census();
    let saves = storage.records();
    let stages = observer::json();
    eprintln!(
        "CAUSE_CANDIDATE_SQL_NATIVE_VM_UNQUALIFIED {:?} C2 {:?}",
        handles.diagnostics()?,
        storage.inner.diagnostics()
    );
    let close = Instant::now();
    drop(storage);
    let handles = std::sync::Arc::try_unwrap(handles).map_err(|_| "retained diagnostic handle")?;
    let sealed = handles.seal()?;
    eprintln!("SEALED_STORE {sealed:?}");
    let seal_ns = close.elapsed().as_nanos();
    let close_ns = seal_ns;
    eprintln!("CAUSE_SQL_OBSERVER {}", sql_observer::json());
    println!("{{\"status\":\"DIAGNOSTIC\",\"route\":\"shared-public-C1-C2-C5-caller\",\"operation_ns\":{},\"bootstrap_ns\":{},\"init_ns\":{},\"seal_ns\":{},\"close_ns\":{},\"root\":\"{}\",\"stack\":\"{}\",\"root_serial\":{},\"canonical_objects\":{},\"canonical_bytes\":{},\"canonical_inventory_sha256\":\"{}\",\"stages\":{{{}}},\"saves\":[{}]}}",started.elapsed().as_nanos(),bootstrap_ns,init_ns,seal_ns,close_ns,digest::hex(result.root.as_bytes()),"41".repeat(16),result.root_serial,objects,bytes,inventory,stages,saves);
    Ok(())
}
