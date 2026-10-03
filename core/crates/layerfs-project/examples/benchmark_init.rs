//! Standalone Init composition; the harness freezes inputs before any sample.
use layerfs_history::{HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_metadata::{PgConfig, PgHistory, PgMetadata};
use layerfs_project::{init, InitRequest};
use layerfs_s3::{S3Config, S3Objects};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{
    fmt::Write,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
fn unhex<const N: usize>(s: &str) -> Result<[u8; N], Box<dyn std::error::Error>> {
    if s.len() != N * 2 {
        return Err("hex width".into());
    }
    let mut out = [0; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
    }
    Ok(out)
}
fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for b in bytes {
        write!(out, "{b:02x}").unwrap();
    }
    out
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 6 {
        return Err("source scratch stack-body-hex scope-seed-hex name required".into());
    }
    let stack = LayerStackId::from_authority(unhex(&args[3])?);
    let seed = unhex(&args[4])?;
    let config = PgConfig::from_env()?;
    let cursor = unhex(&std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?)?;
    let name = HistoryName::new(&args[5])?;
    let object_config = S3Config::from_env()?;
    // Engine creation and all required connections belong to this operation.
    let start = Instant::now();
    let metadata = Arc::new(PgMetadata::create(
        config.clone(),
        StoragePolicy::frozen_default(),
    )?);
    let history = PgHistory::create(
        config,
        &HistoryCatalogConfig {
            binding_key: b"layerfs-bench-pro".to_vec(),
            incarnation: 1,
            cursor_key: cursor,
        },
    )?;
    let objects = Arc::new(S3Objects::connect_parallel(object_config)?);
    let storage = Storage::new(metadata.clone(), objects.clone())?;
    let bootstrap_ns = start.elapsed().as_nanos();
    let result = Timing::disabled("project.init", |timer| {
        init(
            &storage,
            &history,
            InitRequest {
                source: Path::new(&args[1]),
                scratch_parent: Path::new(&args[2]),
                stack,
                name,
                scope_seed: seed,
                deadline: start + Duration::from_secs(15),
            },
            timer,
        )
    })
    .0?;
    let operation_ns = start.elapsed().as_nanos();
    println!("{{\"status\":\"PASS\",\"operation_ns\":{operation_ns},\"bootstrap_ns\":{bootstrap_ns},\"root\":\"{}\",\"stack\":\"{}\",\"root_serial\":{},\"entries\":{}}}",hex(result.root.as_bytes()),hex(&stack.to_bytes()[1..]),result.root_serial,result.entries);
    eprintln!(
        "DIAGNOSTIC storage={:?} pg={:?} history_pg={:?} s3={:?}",
        result.diagnostics,
        metadata.diagnostics()?,
        history.diagnostics()?,
        objects.diagnostics()?
    );
    Ok(())
}
