//! B1 (published port) and B2 (+1 ms netem): the prototype's storage bench.
//!
//! Workload, seeds, sizes, Save pattern, cache-state declaration and the
//! `write:`, `catalog:` and `cold_read:` lines are those of
//! `delta_hint_bench.rs`, so the rows compare 1:1 with `pg-parent-0ms.txt`,
//! `pg-parent-1ms.txt` and `sqlite-parent.txt`. The prototype's hinted and
//! nulled arms are gone (2.0 has no `base` column); `hinted=` now counts
//! rows with a non-empty `ancestors` list (-1 without the column). Extra
//! lines after those three carry the gate's added fields.
use crate::common::*;
use crate::model;
use crate::monitor::{self, Monitor};
use layerfs_content::ObjectId;
use layerfs_storage::Storage;
use std::time::Instant;

pub fn run(args: &[String]) {
    let strict_hints = strict_requested(args);
    let (engine, label) = match args.get(2).map(String::as_str) {
        Some("sqlite") => {
            let dir = std::path::PathBuf::from(&args[3]);
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join(format!("{}.sqlite", args[4]));
            let _ = std::fs::remove_file(&path);
            (Engine::Sqlite(path), args[4].clone())
        }
        Some("postgresql") => {
            let (url, schema) = (&args[3], &args[4]);
            Monitor::connect(url).batch(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
            (Engine::postgres(url, schema), args[5].clone())
        }
        _ => panic!("usage: b1 sqlite <dir> <label> | b1 postgresql <conninfo> <schema> <label> [--strict-hints]"),
    };
    let mut monitor = match &engine {
        Engine::Postgres(_, url, _) => Some(Monitor::connect(url)),
        Engine::Sqlite(_) => None,
    };
    let versions = env("VERSIONS", 24);
    let chunked = env("FILES_CHUNKED", 8);
    let small = env("FILES_SMALL", 64);
    if let Some(m) = monitor.as_mut() {
        println!(
            "{label} arm: rtt_select1_median_us={:.1} synchronous_commit={} gate_size={}",
            m.rtt_us(200),
            if std::env::var("SYNC").as_deref() == Ok("off") {
                "off"
            } else {
                "local"
            },
            versions == 24 && chunked == 8 && small == 64
        );
    }
    let mut files = bench_files(chunked, small);
    let mut roots: Vec<Option<ObjectId>> = vec![None; files.len()];

    // Write: one Save per version holding every file's new version.
    let (port, work) = engine.port(true);
    let storage = Storage::new(port).unwrap();
    let mut saves = Vec::new();
    let wal0 = monitor.as_mut().map(|m| {
        m.stat_reset();
        m.wal_lsn()
    });
    let w0 = work();
    let mut construct_ms = 0.0;
    for version in 0..versions {
        bench_edit(&mut files, version, chunked);
        let started = Instant::now();
        let reader = storage.reader().unwrap();
        let mut emitted = Emitted::default();
        let next: Vec<ObjectId> = files
            .iter()
            .zip(&roots)
            .map(|(bytes, base)| construct(&reader, bytes, *base, &mut emitted))
            .collect();
        drop(reader);
        construct_ms += started.elapsed().as_secs_f64() * 1e3;
        saves.push(save(&storage, emitted));
        roots = next.into_iter().map(Some).collect();
    }
    let w = work() - w0;
    drop(storage);
    let (p50, max) = p50_max(&saves);
    let total: f64 = saves.iter().sum();
    println!(
        "{label} write: versions={versions} files={} save_total_ms={total:.1} save_p50_ms={p50:.2} save_max_ms={max:.2} construct_ms={construct_ms:.1} statements={} round_trips={}",
        files.len(),
        w.statements,
        w.round_trips
    );
    if let (Some(m), Some(wal0)) = (monitor.as_mut(), wal0) {
        let wal1 = m.wal_lsn();
        println!(
            "{label} write_detail: prepares={} wal_bytes={} {}",
            w.prepares,
            m.wal_bytes(&wal0, &wal1),
            classes_line(&m.stat_classes())
        );
    }
    catalog(&engine, &label, monitor.as_mut());

    let roots: Vec<ObjectId> = roots.into_iter().map(Option::unwrap).collect();
    let (port, work) = engine.port(false);
    if let Some(m) = monitor.as_mut() {
        m.stat_reset();
    }
    let wire0 = monitor.as_ref().and_then(|_| monitor::wire_bytes());
    let w0 = work();
    let (ms, read, diag) = cold_read(port.clone(), &roots, strict_hints);
    let w = work() - w0;
    let wire1 = monitor.as_ref().and_then(|_| monitor::wire_bytes());
    println!(
        "{label} cold_read: ms={ms:.2} bytes={read} statements={} round_trips={} {diag}",
        w.statements, w.round_trips
    );
    println!(
        "{label} cold_read_counters: strict_hints={} chain_hint_stale={} locate_waves={} prepares={}",
        if strict_hints { "on" } else { "off" },
        field_or_na(&diag, "chain_hint_stale"),
        field_or_na(&diag, "locate"),
        w.prepares
    );
    if let (Some(m), Engine::Postgres(_, url, schema)) = (monitor.as_mut(), &engine) {
        let classes = m.stat_classes();
        let (tx, rx) = monitor::wire_delta(wire0, wire1);
        let fetched = fetched_bytes(&diag);
        let model = model::needed_groups(url, schema, port.clone(), &roots);
        println!(
            "{label} cold_read_bytes: wire_tx_bytes={tx} wire_rx_bytes={rx} product_fetched_bytes={fetched} {} {}",
            model.line(),
            model.ratios(fetched, &tx)
        );
        println!(
            "{label} cold_read_server: {} server_ms={:.2}",
            classes_line(&classes),
            classes.iter().map(|c| c.exec_ms).sum::<f64>()
        );
    }
}

/// Bytes the product says it fetched: whole bodies plus localized
/// acquisitions, plus `group_read_bytes` when the build counts it (Phase 3).
pub fn fetched_bytes(diag: &str) -> u64 {
    diag_field(diag, "pack_read_bytes").unwrap_or(0)
        + diag_field(diag, "range_acquired_bytes").unwrap_or(0)
        + diag_field(diag, "group_read_bytes").unwrap_or(0)
}

pub fn classes_line(classes: &[monitor::Class]) -> String {
    classes
        .iter()
        .map(|c| {
            format!(
                "{}:calls={},ms={:.2},rows={},hit={},read={}",
                c.class, c.calls, c.exec_ms, c.rows, c.hit, c.read
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The prototype's `catalog:` line, then the 2.0 detail.
fn catalog(engine: &Engine, label: &str, monitor: Option<&mut Monitor>) {
    match (engine, monitor) {
        (Engine::Sqlite(path), _) => {
            let sql = rusqlite::Connection::open(path).unwrap();
            let _ = sql.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
            let rows: i64 = sql
                .query_row("SELECT count(*) FROM object_location", [], |r| r.get(0))
                .unwrap();
            let bytes = sql
                .query_row(
                    "SELECT sum(pgsize) FROM dbstat WHERE name='object_location' OR name LIKE 'sqlite_autoindex_object_location%'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(-1i64);
            // 2.0 SQLite carries no hint column (owner decision).
            println!("{label} catalog: object_location_bytes={bytes} rows={rows} hinted=-1");
        }
        (Engine::Postgres(_, _, schema), Some(m)) => {
            m.batch(&format!("VACUUM ANALYZE {schema}.object_location"));
            let c = m.catalog(schema);
            println!(
                "{label} catalog: object_location_bytes={} rows={} hinted={}",
                c.location_bytes, c.locations, c.list_rows
            );
            println!("{label} catalog_detail: {}", c.line());
        }
        (Engine::Postgres(..), None) => unreachable!(),
    }
}
