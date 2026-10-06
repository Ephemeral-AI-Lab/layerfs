//! Stage/count diagnostic through public Init; uncontrolled cache, no speed gate.
use layerfs_history::{HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
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
#[cfg(unix)]
fn allocated(path: &Path) -> Result<u64, Box<dyn std::error::Error>> {
    use std::os::unix::fs::MetadataExt;
    let mut bytes = 0;
    for suffix in ["", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        match std::fs::metadata(Path::new(&name)) {
            Ok(m) => bytes += m.blocks() * 512,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(bytes)
}
#[cfg(not(unix))]
fn allocated(_path: &Path) -> Result<u64, Box<dyn std::error::Error>> {
    Err("allocation observation requires Unix".into())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if !matches!(args.len(), 4..=6) {
        return Err(
            "source fresh-database name [durable|disposable] [deadline-seconds] required".into(),
        );
    }
    let selected = match args.get(4).map(String::as_str).unwrap_or("durable") {
        "durable" => SqlitePersistenceProfile::Durable,
        "disposable" => SqlitePersistenceProfile::Disposable,
        _ => return Err("explicit durable/disposable profile required".into()),
    };
    let deadline_seconds = args
        .get(5)
        .map(|s| s.parse::<u64>())
        .transpose()?
        .unwrap_or(15);
    if !matches!(deadline_seconds, 15 | 30) {
        return Err("registered 15/30-second deadline required".into());
    }
    let start = Instant::now();
    let handles = Handles::create(
        PersistenceConfig::sqlite(&args[2])
            .with_sqlite_profile(selected)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
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
    let sql_before_init = handles.diagnostics()?;
    let (result, timing) = Timing::record("project.init", |timer| {
        init(
            &storage,
            &handles.history,
            InitRequest {
                source: Path::new(&args[1]),
                acquisition: &handles.acquisition,
                stack: LayerStackId::from_authority([0x41; 16]),
                name: HistoryName::new(&args[3]).unwrap(),
                scope_seed: [0x42; 32],
                deadline: start + Duration::from_secs(deadline_seconds),
            },
            timer,
        )
    });
    let result = result?;
    let sql_after_init = handles.diagnostics()?;
    let init_ns = init_start.elapsed().as_nanos();
    let allocation_before_checkpoint = allocated(Path::new(&args[2]))?;
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
    let allocation_after_close = allocated(Path::new(&args[2]))?;
    let complete_product_ns = start.elapsed().as_nanos();
    let path = Path::new(&args[2])
        .parent()
        .ok_or("output parent")?
        .join("init-timing.json");
    let output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    timing.write_json(output)?;
    eprintln!("DIAGNOSTIC_ONLY stages=enabled cache=uncontrolled no-performance-admission timing_complete={}", !timing.is_incomplete());
    eprintln!("INIT_SQL_DELTA statements={} vm_steps={} fullscan_steps={} sorts={} autoindex_rows={} reprepares={} returned_rows={} bound_bytes={} write_commits={} statement_ns={} commit_ns={}", sql_after_init.statements-sql_before_init.statements, sql_after_init.vm_steps-sql_before_init.vm_steps, sql_after_init.fullscan_steps-sql_before_init.fullscan_steps, sql_after_init.sorts-sql_before_init.sorts, sql_after_init.autoindex_rows-sql_before_init.autoindex_rows, sql_after_init.reprepares-sql_before_init.reprepares, sql_after_init.returned_rows-sql_before_init.returned_rows, sql_after_init.bound_bytes-sql_before_init.bound_bytes, sql_after_init.write_commits-sql_before_init.write_commits, sql_after_init.statement_ns-sql_before_init.statement_ns, sql_after_init.commit_ns-sql_before_init.commit_ns);

    println!("{{\"status\":\"COMPLETE\",\"operation_ns\":{complete_product_ns},\"bootstrap_ns\":{bootstrap_ns},\"init_ns\":{init_ns},\"checkpoint_ns\":{},\"close_ns\":{close_ns},\"root\":\"{}\",\"stack\":\"{}\",\"root_serial\":{},\"entries\":{}}}",checkpoint.wall_ns,hex(result.root.as_bytes()),hex(&[0x41;16]),result.root_serial,result.entries);
    eprintln!("EFFECTIVE_PROFILE {{\"identity\":\"{}\",\"journal_mode\":\"{}\",\"synchronous\":{},\"foreign_keys\":{},\"fullfsync\":{},\"checkpoint_fullfsync\":{},\"page_size\":{},\"cache_size\":{},\"mmap_size\":{},\"temp_store\":{},\"wal_checkpoint_performed\":{}}}",profile.identity,profile.journal_mode,profile.synchronous,profile.foreign_keys,profile.fullfsync,profile.checkpoint_fullfsync,profile.page_size,profile.cache_size,profile.mmap_size,profile.temp_store,checkpoint.wal_checkpoint_performed);
    eprintln!("DIAGNOSTIC profile={profile:?} allocation_before_checkpoint={allocation_before_checkpoint} allocation_after_close={allocation_after_close}");
    eprintln!("DIAGNOSTIC sqlite={work:?} checkpoint={checkpoint:?} storage={:?} saves={save_work:?} namespace={:?}",result.diagnostics,result.namespace_work);
    Ok(())
}
