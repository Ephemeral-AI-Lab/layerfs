//! Count and phase diagnostic, never an admission speed sample.
#[path = "../../../benchmark/fs-bench-pro/diagnostics/driver_support.rs"]
mod diag;
use layerfs_history::{HistoryName, LayerStackId};
use layerfs_metadata::{PgConfig, PgHistory, PgMetadata, PgWork};
use layerfs_project::{init, InitRequest};
use layerfs_s3::{S3Config, S3Objects};
use layerfs_storage::Storage;
use layerfs_telemetry::timer::Timing;
use std::{
    fmt::Write as _,
    fs::File,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
fn work(value: PgWork) -> String {
    let mut s = format!("{{\"omitted\":{},\"statements\":[", value.omitted);
    for (i, r) in value.statements.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        write!(s,"{{\"sql_id\":\"{:016x}\",\"calls\":{},\"mutating_calls\":{},\"caller_ns\":{},\"queue_ns\":{},\"driver_ns\":{}}}",r.sql_id,r.calls,r.mutating_calls,r.caller_ns,r.queue_ns,r.driver_ns).unwrap();
    }
    s.push_str("]}");
    s
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("source scratch report-dir required".into());
    }
    let folder = Path::new(&args[3]);
    let config = PgConfig::from_env()?;
    let objects_config = S3Config::from_env()?;
    let cursor = diag::unhex(&std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?)?;
    let mut phases = diag::Phases::new();
    let cpu = diag::cpu_ns();
    let started = Instant::now();
    let metadata =
        Arc::new(phases.measure("engine.metadata_open", || PgMetadata::open(config.clone()))?);
    let history = phases.measure("engine.history_open", || {
        PgHistory::open_writable(config, b"layerfs-bench-pro", cursor)
    })?;
    let objects = Arc::new(phases.measure("engine.s3_connect", || {
        S3Objects::connect_parallel(objects_config)
    })?);
    let storage = phases.measure("engine.storage_policy", || {
        Storage::new(metadata.clone(), objects.clone())
    })?;
    let bootstrap_ns = started.elapsed().as_nanos();
    let open_pg = metadata.diagnostics()?;
    let open_history = history.diagnostics()?;
    let open_s3 = objects.diagnostics()?;
    let (result, report) = Timing::record("project.init", |timer| {
        init(
            &storage,
            &history,
            InitRequest {
                source: Path::new(&args[1]),
                scratch_parent: Path::new(&args[2]),
                stack: LayerStackId::from_authority([0x41; 16]),
                name: HistoryName::new("diagnostic-init").unwrap(),
                scope_seed: [0x42; 32],
                deadline: started + Duration::from_secs(15),
            },
            timer,
        )
    });
    let operation_ns = started.elapsed().as_nanos();
    let cpu_ns = diag::cpu_ns() - cpu;
    let result = result?;
    report.write_json(File::create(folder.join("timing.json"))?)?;
    std::fs::write(
        folder.join("pg-work.json"),
        format!(
            "{{\"metadata\":{},\"history\":{}}}",
            work(metadata.statement_work()),
            work(history.statement_work())
        ),
    )?;
    let saves = storage.save_work();
    let names = [
        "begin",
        "membership",
        "admission",
        "pack_flush",
        "registration_ready",
        "upload_window",
        "metadata_register",
        "pack_reserve",
        "ordinal_reserve",
        "finish_close",
    ];
    let mut save_json = format!("{{\"completed\":{},\"saves\":[", saves.completed);
    for index in 0..saves.completed.min(4) as usize {
        if index > 0 {
            save_json.push(',');
        }
        save_json.push('[');
        for (stage, row) in saves.recent[index].stages.iter().enumerate() {
            if stage > 0 {
                save_json.push(',');
            }
            write!(
                save_json,
                "{{\"stage\":\"{}\",\"calls\":{},\"wall_ns\":{}}}",
                names[stage], row.calls, row.wall_ns
            )
            .unwrap();
        }
        save_json.push(']');
    }
    save_json.push_str("]}");
    std::fs::write(folder.join("save-work.json"), save_json)?;
    let s3 = objects.diagnostics()?;
    let mut detail = String::from("[");
    for (i, r) in s3.request_work.iter().enumerate() {
        if i > 0 {
            detail.push(',');
        }
        write!(detail,"{{\"method\":\"{}\",\"calls\":{},\"signing_ns\":{},\"validation_ns\":{},\"header_write_ns\":{},\"continue_wait_ns\":{},\"body_write_ns\":{},\"response_head_ns\":{},\"response_body_ns\":{},\"body_sent\":{},\"body_received\":{}}}", ["PUT","GET","HEAD"][i],r.calls,r.signing_ns,r.validation_ns,r.header_write_ns,r.continue_wait_ns,r.body_write_ns,r.response_head_ns,r.response_body_ns,r.body_sent,r.body_received).unwrap();
    }
    detail.push(']');
    std::fs::write(folder.join("s3-work.json"), detail)?;
    eprintln!("OPEN pg={open_pg:?} history_pg={open_history:?} s3={open_s3:?}");
    eprintln!(
        "DIAGNOSTIC storage={:?} pg={:?} history_pg={:?} s3={:?}",
        result.diagnostics,
        metadata.diagnostics()?,
        history.diagnostics()?,
        objects.diagnostics()?
    );
    println!("{{\"status\":\"PASS\",\"kind\":\"CAUSE_DIAGNOSTIC\",\"operation_ns\":{operation_ns},\"bootstrap_ns\":{bootstrap_ns},\"cpu_ns\":{cpu_ns},\"root\":\"{}\",\"stack\":\"{}\",\"phases\":{}}}",diag::hex(result.root.as_bytes()),diag::hex(&[0x41;16]),phases.json());
    Ok(())
}
