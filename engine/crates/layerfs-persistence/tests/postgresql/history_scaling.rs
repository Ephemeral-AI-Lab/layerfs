//! #758, history scaling: the storage bench's 72-file workload (8 files of
//! 2 MiB, 64 of 16-64 KiB, the prototype's `bench_edit` edits, one Save per
//! version, every file offered its last version as its base, as bench
//! B1/B7) grown to 800 versions and probed at 50, 200 and 800.
//!
//! A probe at V: the Save row updates since the last probe, the catalog
//! over the last depth period (16 versions: every whole-file depth once),
//! a cold strict read of the 72 head files, the chain locate of the same
//! roots, and the Save of version V on a fresh session (cold caches, all
//! 72 files' objects). Counted costs must be equal at every probe; at 800
//! the locate, both chain statements and the publish statement must plan
//! through `object_location_prefix` and `pack_pkey`, with no `Seq Scan`.
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
//!
//! **Stages.** 800 versions do not fit one bounded test command, so the
//! history grows in stages, one test process each, over a kept Store schema
//! (`lfs758_history_scaling`) and a state schema beside it holding the
//! heads, pack marks and probe results. Stage 1 runs by default and starts
//! afresh; the later stages are `#[ignore]`d and run by name, in this order,
//! each under the 120 s ceiling:
//!
//! ```text
//! cargo +1.85.1 test --locked -p layerfs-persistence --test postgresql history_scaling -- --exact stage_1_grow_to_50_and_probe
//! cargo +1.85.1 test --locked -p layerfs-persistence --test postgresql history_scaling -- --exact --ignored stage_2_grow_to_200_and_probe
//! cargo +1.85.1 test --locked -p layerfs-persistence --test postgresql history_scaling -- --exact --ignored stage_3_grow_to_500
//! cargo +1.85.1 test --locked -p layerfs-persistence --test postgresql history_scaling -- --exact --ignored stage_4_grow_to_800
//! cargo +1.85.1 test --locked -p layerfs-persistence --test postgresql history_scaling -- --exact --ignored stage_5_probe_800_compare_and_drop
//! ```
//!
//! The last stage compares every checkpoint's probe and drops both schemas.
use crate::chains;

use chains::*;
use layerfs_content::{
    construct_bytes_with_predecessor, ConstructionPolicy, ContentResult, FinalizedConsumer,
    FinalizedObject, ObjectId, PredecessorBase,
};
use layerfs_persistence::PostgresHandles;
use layerfs_storage::{location::LocatedChains, port::PackPersistence, Storage};
use std::collections::{BTreeSet, HashSet};
use std::time::{Duration, Instant};

const CHUNKED: usize = 8;
const SMALL: usize = 64;
const CHECKPOINTS: [usize; 3] = [50, 200, 800];
/// The kept Store schema and its state schema.
const SCHEMA: &str = "lfs758_history_scaling";
const STATE: &str = "lfs758_history_scaling_state";
/// The probe Save's edit, the same at every probe: `bench_edit` positions
/// and bytes of a version the history never reaches.
const PROBE_EDIT: usize = 1_000_000;
/// Whole-file depth cap 16, chunk and metadata caps 8: every depth recurs
/// within 16 versions.
const PERIOD: usize = 16;

/// Marks a statement explained as its generic plan (parameters unbound).
const GENERIC: &str = "-- generic\n";
const SQL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/sql/postgresql/");

/// The bench's files and edits (`storage_gate_bench/common.rs`), unchanged.
fn bench_files() -> Vec<Vec<u8>> {
    (0..CHUNKED)
        .map(|i| noise(i as u64 + 1, 2 << 20))
        .chain((0..SMALL).map(|i| noise(1000 + i as u64, (16 << 10) + (i * 761) % (48 << 10))))
        .collect()
}

fn bench_edit(files: &mut [Vec<u8>], version: usize) {
    for (index, bytes) in files.iter_mut().enumerate() {
        let edits = if index < CHUNKED { 3 } else { 1 };
        for e in 0..edits {
            let at = (version * 7919 + index * 104_729 + e * 31_337) % (bytes.len() - 64);
            bytes[at..at + 64].copy_from_slice(&noise((version * 97 + index * 13 + e) as u64, 64));
        }
    }
}

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

/// Version `version`'s edits and objects, constructed against `roots`.
fn version(
    storage: &Storage,
    files: &mut [Vec<u8>],
    roots: &mut [Option<ObjectId>],
    version: usize,
) -> Vec<FinalizedObject> {
    bench_edit(files, version);
    let policy = ConstructionPolicy::frozen_default();
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    for (bytes, root) in files.iter().zip(roots.iter_mut()) {
        let built = layerfs_telemetry::timer::Timing::disabled("test.construct", |scope| {
            construct_bytes_with_predecessor(
                policy,
                &policy.capacities(),
                bytes,
                root.map(|base| PredecessorBase::new(&reader, base)),
                &mut emitted,
                scope.child("content"),
            )
        });
        *root = Some(built.0.unwrap().root);
    }
    emitted.0
}

fn key(id: &ObjectId) -> i64 {
    i64::from_be_bytes(id.as_bytes()[..8].try_into().unwrap())
}

/// Insert and update counts of `object_location` and `pack` once every
/// closed session's statistics have arrived (inserts equal the row
/// counts); a bounded readiness wait.
fn updates(store: &Store) -> (i64, i64) {
    let mut admin = store.admin();
    let started = Instant::now();
    loop {
        admin
            .batch_execute("SELECT pg_stat_clear_snapshot()")
            .unwrap();
        let row = admin
            .query_one(
                "SELECT (SELECT count(*) FROM object_location),(SELECT count(*) FROM pack),\
                 l.n_tup_ins,l.n_tup_upd,p.n_tup_ins,p.n_tup_upd \
                 FROM pg_stat_user_tables l,pg_stat_user_tables p \
                 WHERE l.schemaname=$1 AND l.relname='object_location' AND p.schemaname=$1 AND p.relname='pack'",
                &[&store.name],
            )
            .unwrap();
        let v: Vec<i64> = (0..6).map(|i| row.get(i)).collect();
        if v[0] == v[2] && v[1] == v[4] {
            return (v[3], v[5]);
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "statistics did not arrive: {v:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Counted {
    read: (u64, u64, u64, u64),
    chain: (u64, u64, u64),
    save: (u64, u64, u64),
}

#[derive(Debug)]
struct Probe {
    at: usize,
    counted: Counted,
    read_bytes: u64,
    read_packs: u64,
    chain_directory_bytes: usize,
    save_catalog_bytes: i64,
    period_catalog_bytes: i64,
    period_list_bytes: (i64, i64),
}

fn probe(store: &Store, at: usize, sample: &[ObjectId], pack_marks: &[i64]) -> Probe {
    let mut admin = store.admin();
    let since = pack_marks[pack_marks.len() - 1 - PERIOD];
    let row = admin
        .query_one(
            "SELECT coalesce(sum(pg_column_size(o.*)),0)::bigint,\
             coalesce(sum(octet_length(o.ancestors)),0)::bigint,\
             count(*) FILTER (WHERE octet_length(o.ancestors)>0) \
             FROM object_location o WHERE o.pack_id>$1",
            &[&since],
        )
        .unwrap();
    // Cold strict read of the sample on a fresh session.
    let handles = store.open();
    let (read, cost) = cold_read(&handles, sample, true);
    read.unwrap();
    assert_eq!(cost.stale, 0, "at {at}: {cost:?}");
    drop(handles);
    // The chain locate of the same roots on a fresh session.
    let handles = store.open();
    let before = handles.diagnostics().unwrap();
    let (mut rows, mut located) = (Vec::new(), LocatedChains::default());
    handles
        .storage
        .locate_chains(sample, &mut rows, &mut located)
        .unwrap();
    let after = handles.diagnostics().unwrap();
    drop(handles);
    // The fixed Save on a fresh session: the sample's files with the probe
    // edit, offered the sample as bases.
    let mark = max_pack(&mut admin);
    let handles = store.open();
    let storage = storage(&handles);
    let mut files = files_at(sample_version(at));
    let mut roots: Vec<Option<ObjectId>> = sample.iter().copied().map(Some).collect();
    let objects = version(&storage, &mut files, &mut roots, PROBE_EDIT);
    let s0 = handles.diagnostics().unwrap();
    save(&storage, objects);
    let s1 = handles.diagnostics().unwrap();
    drop(storage);
    drop(handles);
    let save_catalog_bytes: i64 = admin
        .query_one(
            "SELECT coalesce(sum(pg_column_size(o.*)),0)::bigint FROM object_location o WHERE o.pack_id>$1",
            &[&mark],
        )
        .unwrap()
        .get(0);
    Probe {
        at,
        counted: Counted {
            read: (cost.statements, cost.round_trips, cost.prepares, cost.waves),
            chain: (
                after.statements - before.statements,
                after.round_trips - before.round_trips,
                after.prepares - before.prepares,
            ),
            save: (
                s1.statements - s0.statements,
                s1.round_trips - s0.round_trips,
                s1.prepares - s0.prepares,
            ),
        },
        read_bytes: cost.read_bytes,
        read_packs: cost.group_reads,
        chain_directory_bytes: located.directories.iter().map(|d| d.prefix.len()).sum(),
        save_catalog_bytes,
        period_catalog_bytes: row.get(0),
        period_list_bytes: (row.get(1), row.get(2)),
    }
}

/// The version a probe at `at` samples: the last one at the end of a depth
/// period, so every probe reads and extends chains of the same depths.
fn sample_version(at: usize) -> usize {
    (at - 1) / PERIOD * PERIOD
}

/// The files after versions `0..=version`.
fn files_at(version: usize) -> Vec<Vec<u8>> {
    let mut files = bench_files();
    for v in 0..=version {
        bench_edit(&mut files, v);
    }
    files
}

fn max_pack(admin: &mut postgres::Client) -> i64 {
    admin
        .query_one("SELECT coalesce(max(pack_id),0) FROM pack", &[])
        .unwrap()
        .get(0)
}

/// The plan nodes of an `EXPLAIN (FORMAT JSON)` text: node type, relation
/// and index, each from the node's own keys (before its `Plans`).
fn nodes(plan: &str) -> Vec<(String, Option<String>, Option<String>)> {
    let field = |segment: &str, name: &str| {
        let key = format!("\"{name}\": \"");
        segment.find(&key).map(|at| {
            let rest = &segment[at + key.len()..];
            rest[..rest.find('"').unwrap()].to_owned()
        })
    };
    plan.split("\"Node Type\": \"")
        .skip(1)
        .map(|segment| {
            let own = &segment[..segment.find("\"Plans\"").unwrap_or(segment.len())];
            (
                own[..own.find('"').unwrap()].to_owned(),
                field(own, "Relation Name"),
                field(own, "Index Name"),
            )
        })
        .collect()
}

/// Asserts `plan` scans neither table sequentially and reaches each table
/// in `tables` through its index.
fn assert_indexed(what: &str, plan: &str, tables: &[(&str, &str)]) {
    let nodes = nodes(plan);
    for (kind, relation, _) in &nodes {
        assert!(
            !(kind == "Seq Scan"
                && matches!(relation.as_deref(), Some("object_location" | "pack"))),
            "{what}: sequential scan of {relation:?}\n{plan}"
        );
    }
    for (table, index) in tables {
        assert!(
            nodes.iter().any(|(_, _, i)| i.as_deref() == Some(index)),
            "{what}: {table} is not reached through {index}\n{plan}"
        );
    }
}

fn explain(store: &Store, heads: &[ObjectId]) {
    let sql = |file: &str| {
        std::fs::read_to_string(format!("{SQL}{file}"))
            .unwrap()
            .replace("{schema}", &store.name)
    };
    let mut admin = store.admin();
    // The plan as text: `simple_query` returns `json` as text, so the
    // arrays are spelled inline as the bound values would be.
    let hex = |b: &[u8]| {
        b.iter().fold(String::new(), |mut out, x| {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{x:02x}"));
            out
        })
    };
    let ids = format!(
        "(ARRAY[{}])",
        heads
            .iter()
            .map(|id| format!("decode('{}','hex')", hex(id.as_bytes())))
            .collect::<Vec<_>>()
            .join(",")
    );
    let keys = format!(
        "(ARRAY[{}]::bigint[])",
        heads
            .iter()
            .map(|id| key(id).to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let explain = |admin: &mut postgres::Client, sql: &str, params: &[&str]| {
        let mut sql = sql.to_owned();
        for (at, value) in params.iter().enumerate().rev() {
            sql = sql.replace(&format!("${}", at + 1), value);
        }
        admin
            .simple_query(&match sql.strip_prefix(GENERIC) {
                Some(sql) => format!("EXPLAIN (GENERIC_PLAN, FORMAT JSON) {sql}"),
                None => format!("EXPLAIN (FORMAT JSON) {sql}"),
            })
            .unwrap()
            .into_iter()
            .filter_map(|m| match m {
                postgres::SimpleQueryMessage::Row(row) => row.get(0).map(str::to_owned),
                _ => None,
            })
            .collect::<String>()
    };
    let locate = explain(&mut admin, &sql("metadata/locate.sql"), &[&ids, &keys]);
    assert_indexed(
        "locate",
        &locate,
        &[
            ("object_location", "object_location_prefix"),
            ("pack", "pack_pkey"),
        ],
    );
    let roots = explain(&mut admin, &sql("metadata/chain/roots.sql"), &[&ids, &keys]);
    assert_indexed(
        "chain statement 1",
        &roots,
        &[
            ("object_location", "object_location_prefix"),
            ("pack", "pack_pkey"),
        ],
    );
    // Statement 2 for the members' packs the roots' packs do not cover.
    let handles = store.open();
    let (mut rows, mut located) = (Vec::new(), LocatedChains::default());
    handles
        .storage
        .locate_chains(heads, &mut rows, &mut located)
        .unwrap();
    let own: BTreeSet<i64> = rows.iter().map(|r| r.pack.pack_id).collect();
    let members: Vec<i64> = located
        .chains
        .iter()
        .flat_map(|c| c.members.iter().map(|m| m.pack.pack_id))
        .filter(|p| !own.contains(p))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert!(
        !members.is_empty(),
        "the heads have member packs of their own"
    );
    let members = format!(
        "(ARRAY[{}]::bigint[])",
        members
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    let packs = explain(&mut admin, &sql("metadata/chain/packs.sql"), &[&members]);
    assert_indexed("chain statement 2", &packs, &[("pack", "pack_pkey")]);
    // The publish statement names no schema (it runs on the session's
    // search_path) and binds 22 arrays; its generic plan is the plan any
    // batch gets. It reads `object_location` only; packs arrive by COPY.
    let generic = explain(
        &mut admin,
        &format!("{GENERIC}{}", sql("metadata/publish_rows.sql")),
        &[],
    );
    assert_indexed(
        "publish",
        &generic,
        &[("object_location", "object_location_prefix")],
    );
}

impl Probe {
    fn fields(&self) -> Vec<i64> {
        let c = &self.counted;
        [
            c.read.0,
            c.read.1,
            c.read.2,
            c.read.3,
            c.chain.0,
            c.chain.1,
            c.chain.2,
            c.save.0,
            c.save.1,
            c.save.2,
            self.read_bytes,
            self.read_packs,
        ]
        .into_iter()
        .map(|v| v as i64)
        .chain([
            self.chain_directory_bytes as i64,
            self.save_catalog_bytes,
            self.period_catalog_bytes,
            self.period_list_bytes.0,
            self.period_list_bytes.1,
        ])
        .collect()
    }
    fn from_fields(at: usize, f: &[i64]) -> Self {
        let u = |i: usize| f[i] as u64;
        Self {
            at,
            counted: Counted {
                read: (u(0), u(1), u(2), u(3)),
                chain: (u(4), u(5), u(6)),
                save: (u(7), u(8), u(9)),
            },
            read_bytes: u(10),
            read_packs: u(11),
            chain_directory_bytes: f[12] as usize,
            save_catalog_bytes: f[13],
            period_catalog_bytes: f[14],
            period_list_bytes: (f[15], f[16]),
        }
    }
}

/// The grown history kept between stages: each version's heads and the
/// largest pack id after it, and the objects already stored.
struct History {
    heads: Vec<Vec<ObjectId>>,
    pack_marks: Vec<i64>,
    saved: HashSet<ObjectId>,
}

fn state(store: &Store) -> postgres::Client {
    let mut client = postgres::Client::connect(&store.url, postgres::NoTls).unwrap();
    client
        .batch_execute(&format!("SET search_path TO {STATE}"))
        .unwrap();
    client
}

/// Stage 1's fresh Store and state, or a later stage's kept ones.
fn open(url: &str, first: bool) -> (Store, History) {
    let store = Store::kept(url, SCHEMA, first);
    let mut history = History {
        heads: Vec::new(),
        pack_marks: vec![0],
        saved: HashSet::new(),
    };
    let mut admin = postgres::Client::connect(url, postgres::NoTls).unwrap();
    if first {
        admin
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {STATE} CASCADE; CREATE SCHEMA {STATE};\
                 CREATE TABLE {STATE}.version(version int PRIMARY KEY, heads bytea NOT NULL, pack_mark bigint NOT NULL);\
                 CREATE TABLE {STATE}.probe(at int PRIMARY KEY, fields bigint[] NOT NULL)"
            ))
            .unwrap();
        return (store, history);
    }
    for row in state(&store)
        .query("SELECT heads,pack_mark FROM version ORDER BY version", &[])
        .unwrap()
    {
        let heads: Vec<u8> = row.get(0);
        history.heads.push(
            heads
                .chunks(32)
                .map(|id| ObjectId::from_bytes(id).unwrap())
                .collect(),
        );
        history.pack_marks.push(row.get(1));
    }
    for row in store
        .admin()
        .query("SELECT object_id FROM object_location", &[])
        .unwrap()
    {
        history
            .saved
            .insert(ObjectId::from_bytes(&row.get::<_, Vec<u8>>(0)).unwrap());
    }
    (store, history)
}

/// Grows the history to `to` versions on its own session, closed before
/// any probe so its row statistics arrive. Objects already stored are not
/// offered again: the Save would drop them as duplicates, so the Store is
/// the same.
fn grow(store: &Store, history: &mut History, to: usize) {
    let mut admin = store.admin();
    let mut client = state(store);
    let mut files = match history.heads.len() {
        0 => bench_files(),
        done => files_at(done - 1),
    };
    let mut roots: Vec<Option<ObjectId>> = match history.heads.last() {
        Some(heads) => heads.iter().copied().map(Some).collect(),
        None => vec![None; CHUNKED + SMALL],
    };
    let handles: PostgresHandles = store.open();
    let storage = storage(&handles);
    while history.heads.len() < to {
        let objects: Vec<FinalizedObject> =
            version(&storage, &mut files, &mut roots, history.heads.len())
                .into_iter()
                .filter(|o| history.saved.insert(o.id()))
                .collect();
        save(&storage, objects);
        let mark = max_pack(&mut admin);
        let heads: Vec<ObjectId> = roots.iter().map(|r| r.unwrap()).collect();
        let bytes: Vec<u8> = heads.iter().flat_map(|id| id.as_bytes().to_vec()).collect();
        client
            .execute(
                "INSERT INTO version VALUES ($1,$2,$3)",
                &[&(history.heads.len() as i32), &bytes, &mark],
            )
            .unwrap();
        history.pack_marks.push(mark);
        history.heads.push(heads);
    }
    drop(storage);
    drop(handles);
    assert_eq!(updates(store), (0, 0), "Saves update no row (to {to})");
}

/// The probe at checkpoint `at`, kept for the last stage.
fn checkpoint(store: &Store, history: &History, at: usize) {
    assert_eq!(history.heads.len(), at, "the history reached {at}");
    let sample = &history.heads[sample_version(at)];
    if at == CHECKPOINTS[CHECKPOINTS.len() - 1] {
        explain(store, sample);
    }
    let probe = probe(store, at, sample, &history.pack_marks);
    eprintln!("{probe:?}");
    assert_eq!(
        updates(store),
        (0, 0),
        "the probe Save updates no row (at {at})"
    );
    state(store)
        .execute(
            "INSERT INTO probe VALUES ($1,$2)",
            &[&(at as i32), &probe.fields()],
        )
        .unwrap();
}

#[test]
fn stage_1_grow_to_50_and_probe() {
    let Some(url) = url() else { return };
    // The bench's statement file is the product's.
    let bench = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/storage_gate_bench/b8_locate.sql"
    );
    assert_eq!(
        std::fs::read_to_string(bench).unwrap().trim(),
        std::fs::read_to_string(format!("{SQL}metadata/locate.sql"))
            .unwrap()
            .trim(),
        "b8_locate.sql is the product locate"
    );
    let (store, mut history) = open(&url, true);
    grow(&store, &mut history, CHECKPOINTS[0]);
    checkpoint(&store, &history, CHECKPOINTS[0]);
}

#[test]
#[ignore = "stage 2 of 5: run by name after stage 1 (see the module comment)"]
fn stage_2_grow_to_200_and_probe() {
    let Some(url) = url() else { return };
    let (store, mut history) = open(&url, false);
    grow(&store, &mut history, CHECKPOINTS[1]);
    checkpoint(&store, &history, CHECKPOINTS[1]);
}

#[test]
#[ignore = "stage 3 of 5: run by name after stage 2 (see the module comment)"]
fn stage_3_grow_to_500() {
    let Some(url) = url() else { return };
    let (store, mut history) = open(&url, false);
    grow(&store, &mut history, 500);
}

#[test]
#[ignore = "stage 4 of 5: run by name after stage 3 (see the module comment)"]
fn stage_4_grow_to_800() {
    let Some(url) = url() else { return };
    let (store, mut history) = open(&url, false);
    grow(&store, &mut history, CHECKPOINTS[2]);
}

#[test]
#[ignore = "stage 5 of 5: run by name after stage 4 (see the module comment)"]
fn stage_5_probe_800_compare_and_drop() {
    let Some(url) = url() else { return };
    let (store, history) = open(&url, false);
    checkpoint(&store, &history, CHECKPOINTS[2]);
    let probes: Vec<Probe> = state(&store)
        .query("SELECT at,fields FROM probe ORDER BY at", &[])
        .unwrap()
        .iter()
        .map(|row| Probe::from_fields(row.get::<_, i32>(0) as usize, &row.get::<_, Vec<i64>>(1)))
        .collect();
    drop_schema(&url, STATE);
    drop_schema(&url, SCHEMA);
    let at: Vec<usize> = probes.iter().map(|p| p.at).collect();
    assert_eq!(at, CHECKPOINTS, "every checkpoint was probed");
    let first = &probes[0];
    for probe in &probes[1..] {
        let at = (first.at, probe.at);
        assert_eq!(probe.counted, first.counted, "counted costs at {at:?}");
        assert_eq!(probe.read_packs, first.read_packs, "packs read at {at:?}");
        assert_eq!(probe.read_bytes, first.read_bytes, "bytes read at {at:?}");
        assert_eq!(
            probe.chain_directory_bytes, first.chain_directory_bytes,
            "directory bytes at {at:?}"
        );
        assert_eq!(
            probe.save_catalog_bytes, first.save_catalog_bytes,
            "catalog bytes per Save at {at:?}"
        );
        assert_eq!(
            probe.period_list_bytes, first.period_list_bytes,
            "list bytes per delta row at {at:?}"
        );
        assert_eq!(
            probe.period_catalog_bytes, first.period_catalog_bytes,
            "catalog bytes per period at {at:?}"
        );
    }
}
