//! B7 history scaling: the storage bench's 72-file workload grown to 1,600
//! versions in resumable `grow` steps, probed at 100, 400 and 1,600.
//!
//! Version v holds the prototype's edits 0..=v (`bench_edit`), one Save per
//! version, every file offered its previous version as its base, exactly as
//! B1 (B1 is this history cut at 24 versions).
//!
//! A probe at history length V, in this order:
//! 1. **catalog**: sizes after `VACUUM ANALYZE object_location`, bytes per
//!    locator, list bytes per delta row, directory bytes, and catalog bytes
//!    per Commit (whole history, and since the previous probe);
//! 2. **cold read** of the fixed sample (the 72 head versions) on a fresh
//!    `Storage`: wall, statements, round trips, bytes, wire bytes, server
//!    time by class, `chain_hint_stale`;
//! 3. **chain statement** for the same 72 roots, from `--chain-sql FILE`
//!    (the Phase 2 product statement, B8 file form); NOT_RUN without it;
//! 4. **Save**: version V+1, measured alone on a fresh `Storage` (cold
//!    client caches, the same at every probe, unlike the warm `grow`
//!    Saves): construction and Save wall,
//!    statements, round trips, WAL bytes, server time, catalog bytes added,
//!    and `n_tup_upd`/`n_tup_del` on `object_location` and `pack` (must not
//!    move during Saves). The probe Save is part of the history: the next
//!    `grow` continues from V+1.
//!
//! Every `grow` step also prints the row-update delta over its Saves.
use crate::b1::classes_line;
use crate::b8;
use crate::common::*;
use crate::monitor::{self, Monitor};
use layerfs_content::ObjectId;
use layerfs_storage::Storage;
use std::time::Instant;

const CHUNKED: usize = 8;
const SMALL: usize = 64;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// The files as they stand after `versions` Saves.
fn files_at(versions: u64) -> Vec<Vec<u8>> {
    let mut files = bench_files(CHUNKED, SMALL);
    for version in 0..versions as usize {
        bench_edit(&mut files, version, CHUNKED);
    }
    files
}

/// One version: edits, construction against the previous roots, one Save.
/// Returns (new roots, construct ms, save ms, construct work, save work).
fn one_version(
    storage: &Storage,
    work: &dyn Fn() -> Work,
    files: &mut [Vec<u8>],
    roots: &[Option<ObjectId>],
    version: usize,
) -> (Vec<Option<ObjectId>>, f64, f64, Work, Work) {
    bench_edit(files, version, CHUNKED);
    let w0 = work();
    let started = Instant::now();
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let next: Vec<Option<ObjectId>> = files
        .iter()
        .zip(roots)
        .map(|(bytes, base)| Some(construct(&reader, bytes, *base, &mut emitted)))
        .collect();
    drop(reader);
    let construct_ms = started.elapsed().as_secs_f64() * 1e3;
    let w1 = work();
    let save_ms = save(storage, emitted);
    let w2 = work();
    (next, construct_ms, save_ms, w1 - w0, w2 - w1)
}

fn load_roots(m: &mut Monitor, schema: &str) -> Vec<Option<ObjectId>> {
    match m.state_get(schema, "b7_roots") {
        Some(bytes) => roots_from(&bytes).into_iter().map(Some).collect(),
        None => vec![None; CHUNKED + SMALL],
    }
}

/// `b7 grow <conninfo> <schema> --to N`.
pub fn grow(args: &[String]) {
    let (url, schema) = (&args[3], &args[4]);
    let to: u64 = arg(args, "--to").expect("--to N").parse().unwrap();
    let mut m = Monitor::connect(url);
    let done = m.state_u64(schema, "b7_versions").unwrap_or(0);
    if done == 0 && m.schema_exists(schema) {
        panic!("schema {schema} exists without B7 state: `reset` it first");
    }
    let engine = Engine::postgres(url, schema);
    let (port, work) = engine.port(done == 0);
    let storage = Storage::new(port).unwrap();
    let mut files = files_at(done);
    let mut roots = load_roots(&mut m, schema);
    let upd0 = m.tuple_updates(schema);
    let (mut saves, mut construct_ms) = (Vec::new(), 0.0);
    let mut total = Work::default();
    let started = Instant::now();
    for version in done..to {
        let (next, c, s, wc, ws) =
            one_version(&storage, &work, &mut files, &roots, version as usize);
        roots = next;
        construct_ms += c;
        saves.push(s);
        total.statements += wc.statements + ws.statements;
        total.round_trips += wc.round_trips + ws.round_trips;
        let ids: Vec<ObjectId> = roots.iter().map(|r| r.unwrap()).collect();
        m.state_set(schema, "b7_roots", &roots_bytes(&ids));
        m.state_set_u64(schema, "b7_versions", version + 1);
    }
    let upd1 = m.tuple_updates(schema);
    let (p50, max) = if saves.is_empty() {
        (0.0, 0.0)
    } else {
        p50_max(&saves)
    };
    println!(
        "{schema} b7_grow: from={done} to={to} wall_s={:.1} save_total_ms={:.1} save_p50_ms={p50:.2} save_max_ms={max:.2} \
         construct_ms={construct_ms:.1} statements={} round_trips={} n_tup_upd_delta={} n_tup_del_delta={}",
        started.elapsed().as_secs_f64(),
        saves.iter().sum::<f64>(),
        total.statements,
        total.round_trips,
        upd1.0 - upd0.0,
        upd1.1 - upd0.1
    );
}

/// `b7 probe <conninfo> <schema> [--chain-sql FILE] [--strict-hints]`.
pub fn probe(args: &[String]) {
    let (url, schema) = (&args[3], &args[4]);
    let strict_hints = strict_requested(args);
    let mut m = Monitor::connect(url);
    let versions = m.state_u64(schema, "b7_versions").expect("b7 grow first");
    let label = format!("{schema}@{versions}");

    // 1. Catalog.
    m.batch(&format!("VACUUM ANALYZE {schema}.object_location"));
    let c = m.catalog(schema);
    let total = (c.location_bytes + c.pack_relation_bytes) as u64;
    let prev = m.state_get(schema, "b7_probe_prev").map(|v| {
        (
            u64::from_le_bytes(v[..8].try_into().unwrap()),
            u64::from_le_bytes(v[8..].try_into().unwrap()),
        )
    });
    let since = prev.map_or("NA".to_owned(), |(bytes, at)| {
        format!(
            "{:.0}",
            (total as f64 - bytes as f64) / (versions - at).max(1) as f64
        )
    });
    println!(
        "{label} b7_catalog: versions={versions} catalog_bytes={total} catalog_bytes_per_commit={:.0} \
         catalog_bytes_per_commit_since_prev_probe={since} {}",
        total as f64 / versions as f64,
        c.line()
    );

    // 2. Cold read of the fixed sample.
    let roots: Vec<ObjectId> = load_roots(&mut m, schema)
        .into_iter()
        .map(|r| r.unwrap())
        .collect();
    let engine = Engine::postgres(url, schema);
    let (port, work) = engine.port(false);
    m.stat_reset();
    let wire0 = monitor::wire_bytes();
    let w0 = work();
    let (ms, read, diag) = cold_read(port, &roots, strict_hints);
    let w = work() - w0;
    let wire1 = monitor::wire_bytes();
    let classes = m.stat_classes();
    let (tx, _) = monitor::wire_delta(wire0, wire1);
    println!(
        "{label} b7_cold_read: files={} ms={ms:.2} bytes={read} statements={} round_trips={} prepares={} wire_tx_bytes={tx} \
         product_fetched_bytes={} locate_waves={} chain_hint_stale={} strict_hints={} server_ms={:.2} {}",
        roots.len(),
        w.statements,
        w.round_trips,
        w.prepares,
        crate::b1::fetched_bytes(&diag),
        field_or_na(&diag, "locate"),
        field_or_na(&diag, "chain_hint_stale"),
        if strict_hints { "on" } else { "off" },
        classes.iter().map(|c| c.exec_ms).sum::<f64>(),
        classes_line(&classes)
    );
    println!("{label} b7_cold_read_diag: {diag}");

    // 3. Chain statement for the same roots.
    match arg(args, "--chain-sql") {
        Some(path) => {
            let sql = std::fs::read_to_string(&path).unwrap();
            let ids: Vec<Vec<u8>> = roots.iter().map(|r| r.as_bytes().to_vec()).collect();
            let e = b8::explain(&mut m.client, schema, &sql, &ids);
            println!("{label} b7_chain_statement: statement={path} {}", e.line(ids.len()));
        }
        None => println!(
            "{label} b7_chain_statement: NOT_RUN (needs the Phase 2 chain statement: --chain-sql FILE)"
        ),
    }

    // 4. One fixed-size Save (version `versions`).
    let (port, work) = engine.port(false);
    let storage = Storage::new(port).unwrap();
    let mut files = files_at(versions);
    let before = load_roots(&mut m, schema);
    let upd0 = m.tuple_updates(schema);
    let c0 = m.catalog(schema);
    let wal0 = m.wal_lsn();
    m.stat_reset();
    let (next, construct_ms, save_ms, wc, ws) =
        one_version(&storage, &work, &mut files, &before, versions as usize);
    let classes = m.stat_classes();
    let wal1 = m.wal_lsn();
    let c1 = m.catalog(schema);
    let upd1 = m.tuple_updates(schema);
    println!(
        "{label} b7_save: version={versions} construct_ms={construct_ms:.1} construct_round_trips={} save_ms={save_ms:.1} \
         save_statements={} save_round_trips={} save_prepares={} wal_bytes={} catalog_bytes_added={} rows_added={} packs_added={} \
         n_tup_upd_delta={} n_tup_del_delta={} server_ms={:.2} {}",
        wc.round_trips,
        ws.statements,
        ws.round_trips,
        ws.prepares,
        m.wal_bytes(&wal0, &wal1),
        (c1.location_bytes + c1.pack_relation_bytes) - (c0.location_bytes + c0.pack_relation_bytes),
        c1.locations - c0.locations,
        c1.packs - c0.packs,
        upd1.0 - upd0.0,
        upd1.1 - upd0.1,
        classes.iter().map(|c| c.exec_ms).sum::<f64>(),
        classes_line(&classes)
    );
    let ids: Vec<ObjectId> = next.iter().map(|r| r.unwrap()).collect();
    m.state_set(schema, "b7_roots", &roots_bytes(&ids));
    m.state_set_u64(schema, "b7_versions", versions + 1);
    let mut prev = total.to_le_bytes().to_vec();
    prev.extend_from_slice(&versions.to_le_bytes());
    m.state_set(schema, "b7_probe_prev", &prev);
}
