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
//!    (B8 file form: the product's `sql/postgresql/chain_roots.sql`, or
//!    `chain_packs.sql` for statement 2 over the roots' member packs);
//!    NOT_RUN without it;
//! 4. **Save**: version V+1, measured alone on a fresh `Storage` (cold
//!    client caches, the same at every probe, unlike the warm `grow`
//!    Saves): construction and Save wall,
//!    statements, round trips, WAL bytes, server time, catalog bytes added,
//!    and `n_tup_upd`/`n_tup_del` on `object_location` and `pack` (must not
//!    move during Saves). The probe Save is part of the history: the next
//!    `grow` continues from V+1.
//!
//! Every `grow` step also prints the row-update delta over its Saves.
//!
//! **Rows added by the gate-tooling round** (the spec's B7 constant rows
//! "head mount", "lane hold" and "client memory"; printed after step 4, so
//! steps 1-4 and their lines are unchanged):
//! 5. **Lane hold** (`b7_lane_hold:`), of the step-4 probe Save. The lane is
//!    the PostgreSQL `store_lane` row, locked `FOR UPDATE` and followed by
//!    the lane barrier (`pg_advisory_lock/unlock` on the two-key barrier,
//!    `session/profile.rs::lane_row`), held until the transaction ends. A
//!    Save takes it in two kinds of transaction: every non-nested
//!    `Mode::Write` (`write_begin(true)`: the pack/ordinal reservation,
//!    `UPDATE store_policy`), and the publication (`objects/publish.rs::run`:
//!    pack bodies first, then `tx.lane()`, then one `publish_rows.sql`
//!    statement, then COMMIT). Two observations: `lane_hold_server_ms`, the
//!    server execution time (`pg_stat_statements`, reset before step 4) of
//!    the statements that run while the row is held: the row lock, the
//!    barrier lock and unlock, `UPDATE store_policy` and `publish_rows` (a
//!    lower bound: it leaves out COMMIT, which `pg_stat_statements` does not
//!    tell apart from other COMMITs, and the round trips between
//!    statements); and `lane_hold_upper_ms`, the product's own Save
//!    bookkeeping (`Storage::save_work()`, client wall of the stages that
//!    wrap exactly the lane-taking port calls: `Metadata` = `publish`
//!    (including the pack-body COPY before the lane), `PackReserve` and
//!    `OrdinalReserve` = `reserve`), an upper bound. `lane_holds` counts
//!    executions of the lane-row lock (one per held lane). A Save at the
//!    shipped defaults holds the lane twice, so the line also gives each
//!    hold: `reservation_holds` / `reservation_hold_server_ms` (calls and
//!    server time of `UPDATE store_policy`) / `reservation_hold_upper_ms`
//!    (`PackReserve` + `OrdinalReserve` wall), and `publication_holds` /
//!    `publication_hold_server_ms` (`publish_rows`) /
//!    `publication_hold_upper_ms` (`Metadata` wall). The row lock and the
//!    barrier statements have the same text in every hold, so their server
//!    time is printed once for all holds (`lane_lock_calls`,
//!    `lane_lock_server_ms`); the sum of the three server times is
//!    `lane_hold_server_ms`, the sum of the two upper bounds is
//!    `lane_hold_upper_ms`. `holds_attributed=true` when the lane-row locks
//!    equal reservation + publication holds and each matches its stage's
//!    call count. `pg_stat_statements_reset()` is server-wide, so no other
//!    bench may run on the same server during a probe (the existing steps 2
//!    and 4 reset it too).
//! 6. **Head mount of a fixed tree** (`b7_head_mount:`): the tree is the
//!    step-2 sample (the 72 head files at version V, the same file sizes at
//!    every probe). Unmeasured first: the reference closure of those roots,
//!    level by level (roles from the bench's own SQL, references decoded
//!    with `decode_references` through a separate `Storage`). Measured: a
//!    fresh `Storage` (empty client caches, warm server buffers, as the cold
//!    read) and ONE reader reads every closure object (metadata and
//!    payload), one level at a time, in demands of at most
//!    `capacities().read_objects` ids: the closure acquisition the spec's
//!    head-mount model counts (closure = F objects plus capped chains).
//!    Prints `closure_objects`, `levels`, canonical bytes, wall, statements,
//!    round trips, fetched bytes, locate waves, `chain_hint_stale`.
//! 7. **Client memory** (`b7_client_memory:`): the bench process's peak
//!    resident set (`getrusage(RUSAGE_SELF).ru_maxrss`, normalised to bytes:
//!    bytes on macOS, KiB on Linux) and current resident set (`ps -o rss=`)
//!    after steps 2, 3, 4 and 6, sampled outside timed regions. The peak is
//!    process-wide and monotone, so a later step shows only when it raises
//!    it. Beside it, the product's own bookkeeping
//!    `locator_bookkeeping_live_peak_bytes` (not a process measurement) of
//!    the cold-read, Save and head-mount `Storage`s.
use crate::b1::classes_line;
use crate::b8;
use crate::common::*;
use crate::monitor::{self, Monitor, TAG};
use layerfs_content::ObjectId;
use layerfs_content::{object::decode_references, ConstructionPolicy, InodeScope, ObjectRole};
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
    let mut memory = vec![("cold_read", memory_sample())];
    let read_locator_peak = field_or_na(&diag, "locator_bookkeeping_live_peak_bytes");

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
    memory.push(("chain_statement", memory_sample()));

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
    memory.push(("save", memory_sample()));
    let ids: Vec<ObjectId> = next.iter().map(|r| r.unwrap()).collect();
    m.state_set(schema, "b7_roots", &roots_bytes(&ids));
    m.state_set_u64(schema, "b7_versions", versions + 1);
    let mut prev = total.to_le_bytes().to_vec();
    prev.extend_from_slice(&versions.to_le_bytes());
    m.state_set(schema, "b7_probe_prev", &prev);

    // 5. Lane hold of the step-4 Save (added row), each hold and the sum.
    let lane = lane_statements(&mut m);
    let stages = storage.save_work().total.stages;
    // Stage order (`layerfs-storage` `save/work.rs`): 5 Metadata (`publish`),
    // 6 PackReserve and 7 OrdinalReserve (`reserve`).
    let (publish, pack_reserve, ordinal_reserve) = (stages[5], stages[6], stages[7]);
    let reserve_upper_ns = pack_reserve.wall_ns + ordinal_reserve.wall_ns;
    let reserve_calls = pack_reserve.calls + ordinal_reserve.calls;
    println!(
        "{label} b7_lane_hold: version={versions} lane_hold_server_ms={:.3} lane_holds={} \
         lane_statement_calls={} lane_hold_upper_ms={:.3} publish_calls={} publish_ms={:.3} \
         reserve_calls={reserve_calls} reserve_ms={:.3} \
         reservation_holds={} reservation_hold_server_ms={:.3} reservation_hold_upper_ms={:.3} \
         publication_holds={} publication_hold_server_ms={:.3} publication_hold_upper_ms={:.3} \
         lane_lock_calls={} lane_lock_server_ms={:.3} holds_attributed={} source=pg_stat_statements+save_work",
        lane.total_ms(),
        lane.holds,
        lane.calls,
        (publish.wall_ns + reserve_upper_ns) as f64 / 1e6,
        publish.calls,
        publish.wall_ns as f64 / 1e6,
        reserve_upper_ns as f64 / 1e6,
        lane.reservation.0,
        lane.reservation.1,
        reserve_upper_ns as f64 / 1e6,
        lane.publication.0,
        lane.publication.1,
        publish.wall_ns as f64 / 1e6,
        lane.lock.0,
        lane.lock.1,
        lane.holds == lane.reservation.0 + lane.publication.0
            && lane.reservation.0 == reserve_calls as i64
            && lane.publication.0 == publish.calls as i64,
    );
    let save_locator_peak = field_or_na(
        &format!("{:?}", storage.diagnostics()),
        "locator_bookkeeping_live_peak_bytes",
    );
    drop(storage);

    // 6. Head mount of the fixed tree (added row).
    let levels = closure_levels(&mut m, &engine, schema, &roots);
    let (port, work) = engine.port(false);
    let mount = Storage::new(port).unwrap();
    strict(&mount, strict_hints);
    let w0 = work();
    let started = Instant::now();
    let reader = mount.reader().unwrap();
    let mut canonical = 0u64;
    let mut demands = 0u64;
    for level in &levels {
        for demand in level.chunks(mount.capacities().read_objects) {
            canonical += reader
                .read_objects(demand)
                .unwrap()
                .iter()
                .map(|o| o.len() as u64)
                .sum::<u64>();
            demands += 1;
        }
    }
    drop(reader);
    let ms = started.elapsed().as_secs_f64() * 1e3;
    let w = work() - w0;
    let diag = format!("{:?}", mount.diagnostics());
    println!(
        "{label} b7_head_mount: files={} closure_objects={} levels={} demands={demands} canonical_bytes={canonical} ms={ms:.2} \
         statements={} round_trips={} prepares={} product_fetched_bytes={} locate_waves={} chain_hint_stale={} strict_hints={}",
        roots.len(),
        levels.iter().map(Vec::len).sum::<usize>(),
        levels.len(),
        w.statements,
        w.round_trips,
        w.prepares,
        crate::b1::fetched_bytes(&diag),
        field_or_na(&diag, "locate"),
        field_or_na(&diag, "chain_hint_stale"),
        if strict_hints { "on" } else { "off" },
    );
    memory.push(("head_mount", memory_sample()));

    // 7. Client memory (added row).
    let fields: Vec<String> = memory
        .iter()
        .map(|(step, (peak, now))| {
            format!(
                "maxrss_after_{step}={} rss_after_{step}={}",
                peak.map_or("NA".to_owned(), |v| v.to_string()),
                now.map_or("NA".to_owned(), |v| v.to_string())
            )
        })
        .collect();
    println!(
        "{label} b7_client_memory: {} locator_bookkeeping_live_peak_bytes_cold_read={read_locator_peak} \
         locator_bookkeeping_live_peak_bytes_save={save_locator_peak} locator_bookkeeping_live_peak_bytes_head_mount={}",
        fields.join(" "),
        field_or_na(&diag, "locator_bookkeeping_live_peak_bytes")
    );
}

/// The statements that run while the `store_lane` row is held, since the
/// last `pg_stat_statements` reset (see the module doc, row 5), by hold.
struct Lane {
    /// Executions of the lane-row lock: one per held lane.
    holds: i64,
    /// Executions of every lane statement below.
    calls: i64,
    /// (calls, server ms) of `UPDATE store_policy` (reservation holds).
    reservation: (i64, f64),
    /// (calls, server ms) of `publish_rows` (publication holds).
    publication: (i64, f64),
    /// (calls, server ms) of the row lock and the barrier lock and unlock,
    /// over every hold: the text is the same in each hold, so
    /// `pg_stat_statements` cannot split it by hold.
    lock: (i64, f64),
}

impl Lane {
    fn total_ms(&self) -> f64 {
        self.reservation.1 + self.publication.1 + self.lock.1
    }
}

fn lane_statements(m: &mut Monitor) -> Lane {
    let rows = m
        .client
        .query(
            &format!(
                "{TAG} SELECT k, coalesce(sum(calls),0)::bigint, coalesce(sum(total_exec_time),0)::float8, \
                 coalesce(sum(calls) FILTER (WHERE query ~* 'store_lane.*for update'),0)::bigint FROM ( \
                 SELECT calls, total_exec_time, query, CASE WHEN query ~* '^update store_policy set next_' THEN 'reservation' \
                 WHEN query LIKE 'WITH RECURSIVE src AS MATERIALIZED%' THEN 'publication' ELSE 'lock' END AS k \
                 FROM pg_stat_statements \
                 WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database()) \
                 AND query NOT LIKE '%gate-monitor%' AND (query ~* 'store_lane.*for update' \
                 OR query ~* 'pg_advisory_(un)?lock[(][$]1, *[$]2[)]' \
                 OR query ~* '^update store_policy set next_' \
                 OR query LIKE 'WITH RECURSIVE src AS MATERIALIZED%')) s GROUP BY k"
            ),
            &[],
        )
        .expect("pg_stat_statements lane statements");
    let mut lane = Lane {
        holds: 0,
        calls: 0,
        reservation: (0, 0.0),
        publication: (0, 0.0),
        lock: (0, 0.0),
    };
    for r in rows {
        let (kind, calls, ms, holds): (String, i64, f64, i64) =
            (r.get(0), r.get(1), r.get(2), r.get(3));
        lane.holds += holds;
        lane.calls += calls;
        match kind.as_str() {
            "reservation" => lane.reservation = (calls, ms),
            "publication" => lane.publication = (calls, ms),
            _ => lane.lock = (calls, ms),
        }
    }
    lane
}

/// The reference closure of `roots`, level by level (each id once, at its
/// first level), found without timing: roles from the bench's own SQL,
/// references decoded from objects read through a separate `Storage`.
fn closure_levels(
    m: &mut Monitor,
    engine: &Engine,
    schema: &str,
    roots: &[ObjectId],
) -> Vec<Vec<ObjectId>> {
    let (port, _) = engine.port(false);
    let storage = Storage::new(port).unwrap();
    let reader = storage.reader().unwrap();
    let policy = ConstructionPolicy::frozen_default();
    let mut seen = std::collections::BTreeSet::new();
    let mut levels = Vec::new();
    let mut frontier: Vec<ObjectId> = roots.to_vec();
    while !frontier.is_empty() {
        frontier.sort();
        frontier.dedup();
        frontier.retain(|id| seen.insert(*id));
        if frontier.is_empty() {
            break;
        }
        let ids: Vec<Vec<u8>> = frontier.iter().map(|i| i.as_bytes().to_vec()).collect();
        let roles: std::collections::HashMap<Vec<u8>, i32> = m
            .client
            .query(
                &format!(
                    "{TAG} SELECT x.object_id, x.role::int4 FROM unnest($1::bytea[]) i(id) \
                     JOIN {schema}.object_location x ON x.object_id = i.id"
                ),
                &[&ids],
            )
            .unwrap()
            .iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect();
        let mut next = Vec::new();
        for id in &frontier {
            let role = ObjectRole::from_code(roles[id.as_bytes().as_slice()] as u8).unwrap();
            if role.has_references() {
                let canonical = reader.read_objects(&[*id]).unwrap().remove(0);
                next.extend(
                    decode_references(role, &canonical, policy, InodeScope::from_object(*id))
                        .unwrap(),
                );
            }
        }
        levels.push(std::mem::take(&mut frontier));
        frontier = next;
    }
    levels
}

/// `getrusage(2)`'s layout on 64-bit macOS and Linux: two 16-byte timevals,
/// then fourteen `long`s, `ru_maxrss` first.
#[repr(C)]
struct Rusage {
    times: [i64; 4],
    maxrss: i64,
    rest: [i64; 13],
}

extern "C" {
    fn getrusage(who: i32, usage: *mut Rusage) -> i32;
}

/// (peak resident bytes of this process, current resident bytes), each
/// `None` when unavailable. See the module doc, row 7.
fn memory_sample() -> (Option<u64>, Option<u64>) {
    let mut usage = Rusage {
        times: [0; 4],
        maxrss: 0,
        rest: [0; 13],
    };
    // SAFETY: `usage` is a writable `struct rusage` of the platform's size
    // and layout; RUSAGE_SELF is 0 on macOS and Linux.
    let peak = (unsafe { getrusage(0, &mut usage) } == 0).then(|| {
        let unit = if cfg!(target_os = "macos") { 1 } else { 1024 };
        usage.maxrss as u64 * unit
    });
    let now = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|t| t.trim().parse::<u64>().ok())
        .map(|kib| kib * 1024);
    (peak, now)
}
