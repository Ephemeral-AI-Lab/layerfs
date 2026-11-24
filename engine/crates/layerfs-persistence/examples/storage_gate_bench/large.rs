//! B3 (large file) and B4 (large commit), as resumable steps.
//!
//! **B3 shape** (spec S1's large-file edits, sizes frozen): one file of
//! 256 MiB (`noise(25_600, 256 MiB)`), 10 versions, one Save per version.
//! Version 0 is the file itself; version k = 1..9 applies, with
//! `Rng::new(758_000 + k)`: a rewrite fraction f uniform in [0.1%, 5%] of the
//! file, as overwrite runs of 4 KiB-256 KiB of fresh noise at uniform
//! offsets until f is reached; then one insert of 1-64 KiB of noise at a
//! uniform offset and one delete of 1-64 KiB at a uniform offset (both
//! shift chunk boundaries). Every version is offered the previous one as its
//! base, as the storage bench does.
//!
//! **B4 shape** (on top of B3's 10 versions, one Save): version 10 of the
//! large file with one 64-byte patch every 256 KiB (offset
//! `k*256 KiB + (k*7919 mod 4096)`), so its delta bases lie in nearly every
//! pack of the file; plus 600 new files of fresh noise, sizes log-uniform in
//! 4 KiB-4 MiB (`Rng::new(758_400)`), which add the packs. The Save must add
//! at least 578 packs (quarry's largest Commit); the row prints the count
//! and `shape_ok`.
//!
//! **Attribution (B4).** Server statements are classed by text
//! (`monitor::Monitor::stat_classes`) per phase: construction (the content
//! layer reading the previous version) and the Save (`begin_save` to
//! `finish`). The client's own round-trip and prepare counters per phase are
//! printed beside them as the cross-check.
//!
//! The gate's base-fetch figure counts the Save's delta-base fetches only
//! (spec `SPEC-758-2.0.md` 4.3; write-path spec: per Save "about 45
//! base-pack reads, 6.5 publication, 3 reservation, 2 locates"), so the
//! Save's port calls are attributed on the client by a pass-through port
//! wrapper ([`Attributing`]), each call's statements and round trips taken
//! from the port's own `SqlWork` counters around it. The classes follow the
//! call sites in `layerfs-storage` (#758 phase 3):
//!
//! - **membership** (`membership_locates`): the per-wave locate in
//!   `save/wave.rs` (`State::wave`), one `locate_chains` per wave of at most
//!   READ_OBJECT_LIMIT named ids (the demand names each object's own id, its
//!   references and its predecessors); recognised by its ids holding one of
//!   the Save's own new object ids. Its first statement (`chain_roots`, with
//!   any prepare round trips) is membership; a second statement in the same
//!   call (`chain_packs`: the member packs of the located bases' chains) is a
//!   base locate.
//! - **closure** (`closure_locates`): `save/publication.rs`'s locate of
//!   references outside the publication (ids all references of the Save's
//!   own objects, none of them its own); reported apart and not counted as
//!   base fetch.
//! - **base locates**: every other locate during the Save: the selection
//!   walk's `Fetch::location` per chain level (`read/fetch.rs`, from
//!   `encoding/delta/select.rs` and `save/select.rs`), the proposed base in
//!   `save/select.rs`, prefetch locates, plus the `chain_packs` statements
//!   above.
//! - **base reads**: every `read_groups`, `read_packs`, `read_scoped_pack`
//!   and `read_pack_selection` during the Save: the Save reads only stored
//!   bases' (and exact-reuse chains') bytes through `read::Fetch`
//!   (`save/source.rs`).
//! - **other**: reserve, publish, inode values, signatures and policy.
//!
//! `base_fetch_round_trips` = base locates + base reads (client round
//! trips; gate <= 40). The phase-0 figure (every server `locate` and
//! `pack_read` call of the Save, `old_base_fetch_round_trips`) and the old
//! all-locates total (`server_locate_calls`) are printed beside it.
//! `attributed_round_trips` must equal the Save's `round_trips`.
use crate::b1::{classes_line, fetched_bytes};
use crate::common::*;
use crate::model;
use crate::monitor::{self, Monitor};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{InodeValueRow, LocatedChains, LocatedObject, SignatureRow},
    policy::StoragePolicy,
    port::{
        AcquiredPackRead, GroupRead, GroupReadAnswer, PackPersistence, PackReadPlan, PersistedPack,
        PersistedPackRead, PersistenceError, Publication, Published, PublishedPack, Reserve,
        Reserved,
    },
    Storage,
};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const MIB: usize = 1 << 20;

fn b3_size() -> usize {
    env("B3_MIB", 256) * MIB
}

/// Applies version `k`'s edits; returns (fraction, overwrite runs).
fn edit(bytes: &mut Vec<u8>, k: u64) -> (f64, usize) {
    let mut rng = Rng::new(758_000 + k);
    let fraction = 0.001 + 0.049 * rng.unit();
    let target = (fraction * bytes.len() as f64) as usize;
    let (mut done, mut runs) = (0usize, 0usize);
    while done < target {
        let len = (rng.range(4096, 256 * 1024) as usize)
            .min(target - done)
            .max(1);
        let at = rng.range(0, (bytes.len() - len) as u64) as usize;
        bytes[at..at + len].copy_from_slice(&noise(rng.next(), len));
        done += len;
        runs += 1;
    }
    let len = rng.range(1, 64 * 1024) as usize;
    let at = rng.range(0, bytes.len() as u64) as usize;
    let inserted = noise(rng.next(), len);
    bytes.splice(at..at, inserted);
    let len = rng.range(1, 64 * 1024) as usize;
    let at = rng.range(0, (bytes.len() - len) as u64) as usize;
    bytes.drain(at..at + len);
    (fraction, runs)
}

/// The large file's content at version `v`.
fn content(v: u64) -> Vec<u8> {
    let mut bytes = noise(25_600, b3_size());
    for k in 1..=v {
        edit(&mut bytes, k);
    }
    bytes
}

fn arg_to(args: &[String]) -> u64 {
    args.iter()
        .position(|a| a == "--to")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .expect("--to <versions>")
}

/// `b3 build <conninfo> <schema> --to N`: Saves versions up to N (<= 10).
pub fn b3_build(args: &[String]) {
    let (url, schema) = (&args[3], &args[4]);
    let to = arg_to(args).min(10);
    let mut m = Monitor::connect(url);
    let done = m.state_u64(schema, "b3_versions").unwrap_or(0);
    if done == 0 && m.schema_exists(schema) {
        panic!("schema {schema} exists without B3 state: `reset` it first");
    }
    let engine = Engine::postgres(url, schema);
    let (port, work) = engine.port(done == 0);
    let storage = Storage::new(port).unwrap();
    let mut root = m.state_get(schema, "b3_root").map(|r| roots_from(&r)[0]);
    let mut bytes = content(done.saturating_sub(1));
    println!(
        "{schema} b3_shape: file_bytes={} versions=10 gate_size={} from={done} to={to}",
        b3_size(),
        b3_size() == 256 * MIB
    );
    for v in done..to {
        let (fraction, runs) = if v == 0 {
            (0.0, 0)
        } else {
            edit(&mut bytes, v)
        };
        let packs0 = m.catalog(schema).packs;
        let wal0 = m.wal_lsn();
        let w0 = work();
        let started = Instant::now();
        let reader = storage.reader().unwrap();
        let mut emitted = Emitted::default();
        let next = construct(&reader, &bytes, root, &mut emitted);
        drop(reader);
        let construct_ms = started.elapsed().as_secs_f64() * 1e3;
        let wc = work() - w0;
        let save_ms = save(&storage, emitted);
        let ws = work() - w0 - wc;
        let wal1 = m.wal_lsn();
        let packs1 = m.catalog(schema).packs;
        println!(
            "{schema} b3_save: version={v} bytes={} rewrite_fraction={fraction:.4} runs={runs} construct_ms={construct_ms:.1} \
             construct_round_trips={} save_ms={save_ms:.1} save_statements={} save_round_trips={} save_prepares={} wal_bytes={} packs_added={}",
            bytes.len(),
            wc.round_trips,
            ws.statements,
            ws.round_trips,
            ws.prepares,
            m.wal_bytes(&wal0, &wal1),
            packs1 - packs0
        );
        root = Some(next);
        m.state_set(schema, "b3_root", &roots_bytes(&[next]));
        m.state_set_u64(schema, "b3_versions", v + 1);
    }
}

/// `b3 read <conninfo> <schema> [--strict-hints]`: the cold read of the
/// last version, then its needed-groups model.
pub fn b3_read(args: &[String]) {
    let (url, schema) = (&args[3], &args[4]);
    let strict_hints = strict_requested(args);
    let mut m = Monitor::connect(url);
    let versions = m.state_u64(schema, "b3_versions").expect("b3 build first");
    let root = roots_from(&m.state_get(schema, "b3_root").unwrap())[0];
    let engine = Engine::postgres(url, schema);
    let (port, work) = engine.port(false);
    m.stat_reset();
    let wire0 = monitor::wire_bytes();
    let w0 = work();
    let (ms, read, diag) = cold_read(port.clone(), &[root], strict_hints);
    let w = work() - w0;
    let wire1 = monitor::wire_bytes();
    let classes = m.stat_classes();
    let waves = diag_field(&diag, "locate").unwrap_or(0);
    let fetched = fetched_bytes(&diag);
    let budget = 2 * waves + fetched.div_ceil(2 * MIB as u64);
    println!(
        "{schema} b3_cold_read: versions={versions} gate_size={} ms={ms:.2} bytes={read} statements={} round_trips={} prepares={} \
         locate_waves={waves} product_fetched_bytes={fetched} a8_budget_round_trips={budget} a8_ok={} strict_hints={} chain_hint_stale={} {diag}",
        b3_size() == 256 * MIB && versions == 10,
        w.statements,
        w.round_trips,
        w.prepares,
        w.round_trips <= budget,
        if strict_hints { "on" } else { "off" },
        field_or_na(&diag, "chain_hint_stale"),
    );
    let (tx, rx) = monitor::wire_delta(wire0, wire1);
    let model = model::needed_groups(url, schema, port, &[root]);
    // One root: the per-read and union models are equal by construction.
    println!(
        "{schema} b3_cold_read_bytes: wire_tx_bytes={tx} wire_rx_bytes={rx} product_fetched_bytes={fetched} {} {} models_equal={}",
        model.line(),
        model.ratios(fetched, &tx),
        model.per_read.bytes() == model.union.bytes()
    );
    println!(
        "{schema} b3_cold_read_server: {} server_ms={:.2}",
        classes_line(&classes),
        classes.iter().map(|c| c.exec_ms).sum::<f64>()
    );
}

fn calls(classes: &[monitor::Class], names: &[&str]) -> i64 {
    classes
        .iter()
        .filter(|c| names.contains(&c.class.as_str()))
        .map(|c| c.calls)
        .sum()
}

/// The B4 Save's port calls by class (see the header).
#[derive(Default)]
struct Attribution {
    active: bool,
    own: BTreeSet<ObjectId>,
    references: BTreeSet<ObjectId>,
    membership: (u64, Work),
    closure: (u64, Work),
    base_locate: (u64, Work),
    base_read: (u64, Work),
    other: (u64, Work),
}

impl Attribution {
    fn add(slot: &mut (u64, Work), work: Work) {
        slot.0 += 1;
        slot.1 += work;
    }
    fn line(&self) -> String {
        let f = |name: &str, (calls, w): &(u64, Work)| {
            format!(
                "{name}_calls={calls} {name}_statements={} {name}_round_trips={}",
                w.statements, w.round_trips
            )
        };
        [
            f("membership", &self.membership),
            f("closure", &self.closure),
            f("base_locate", &self.base_locate),
            f("base_read", &self.base_read),
            f("other", &self.other),
        ]
        .join(" ")
    }
}

/// A pass-through port that attributes each call while `active`; it forwards
/// every method, so the product sees the wrapped port's behaviour.
struct Attributing {
    inner: Arc<dyn PackPersistence>,
    work: Arc<dyn Fn() -> Work + Send + Sync>,
    state: Mutex<Attribution>,
}

#[derive(Clone, Copy)]
enum Kind {
    Locate,
    Read,
    Other,
}

impl Attributing {
    fn call<T>(&self, kind: Kind, ids: &[ObjectId], f: impl FnOnce() -> T) -> T {
        let w0 = (self.work)();
        let out = f();
        let w = (self.work)() - w0;
        let mut s = self.state.lock().unwrap();
        if !s.active {
            return out;
        }
        match kind {
            Kind::Read => Attribution::add(&mut s.base_read, w),
            Kind::Other => Attribution::add(&mut s.other, w),
            Kind::Locate if ids.iter().any(|id| s.own.contains(id)) => {
                // Statement 1 (with any prepares) is membership; the rest of
                // the call is `chain_packs` for the located bases' chains.
                let extra = w.statements.saturating_sub(1);
                let base = Work {
                    statements: extra,
                    round_trips: extra,
                    prepares: 0,
                };
                Attribution::add(&mut s.membership, w - base);
                if extra > 0 {
                    Attribution::add(&mut s.base_locate, base);
                }
            }
            Kind::Locate if !ids.is_empty() && ids.iter().all(|id| s.references.contains(id)) => {
                Attribution::add(&mut s.closure, w)
            }
            Kind::Locate => Attribution::add(&mut s.base_locate, w),
        }
        out
    }
}

impl PackPersistence for Attributing {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        self.call(Kind::Other, &[], || self.inner.policy())
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        self.call(Kind::Locate, ids, || self.inner.locate(ids, out))
    }
    fn locate_chains(
        &self,
        roots: &[ObjectId],
        rows: &mut Vec<LocatedObject>,
        chains: &mut LocatedChains,
    ) -> Result<(), PersistenceError> {
        self.call(Kind::Locate, roots, || {
            self.inner.locate_chains(roots, rows, chains)
        })
    }
    fn read_groups(
        &self,
        reads: &[GroupRead],
        out: &mut Vec<GroupReadAnswer>,
    ) -> Result<(), PersistenceError> {
        self.call(Kind::Read, &[], || self.inner.read_groups(reads, out))
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        self.call(Kind::Read, &[], || self.inner.read_packs(ids, out))
    }
    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<PersistedPackRead, PersistenceError> {
        self.call(Kind::Read, &[], || self.inner.read_pack_selection(id, plan))
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<AcquiredPackRead, PersistenceError> {
        self.call(Kind::Read, &[], || self.inner.read_scoped_pack(id, plan))
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        self.call(Kind::Other, &[], || self.inner.inode_values(ordinals, out))
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
        self.call(Kind::Other, &[], || self.inner.signatures(out))
    }
    fn reserve(&self, request: Reserve) -> Result<Reserved, PersistenceError> {
        self.call(Kind::Other, &[], || self.inner.reserve(request))
    }
    fn whole_pack_bytes(&self) -> usize {
        self.inner.whole_pack_bytes()
    }
    fn batched_chain_reads(&self) -> bool {
        self.inner.batched_chain_reads()
    }
    fn publication_bytes(&self) -> u64 {
        self.inner.publication_bytes()
    }
    fn publication_pack_cost(
        &self,
        pack: &PublishedPack,
    ) -> Result<(usize, u64), PersistenceError> {
        self.inner.publication_pack_cost(pack)
    }
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        self.call(Kind::Other, &[], || self.inner.publish(batch))
    }
}

/// `b4 save <conninfo> <schema>`: the one large-commit Save on B3's history.
pub fn b4_save(args: &[String]) {
    let (url, schema) = (&args[3], &args[4]);
    let mut m = Monitor::connect(url);
    let versions = m.state_u64(schema, "b3_versions").expect("b3 build first");
    if m.state_u64(schema, "b4_done").is_some() {
        panic!("B4 already ran on {schema}: one sample per case; rebuild B3 for another");
    }
    let root = roots_from(&m.state_get(schema, "b3_root").unwrap())[0];
    let mut large = content(versions - 1);
    let mut patches = 0usize;
    let mut k = 0usize;
    loop {
        let at = k * 256 * 1024 + (k * 7919) % 4096;
        if at + 64 > large.len() {
            break;
        }
        large[at..at + 64].copy_from_slice(&noise(900_000 + k as u64, 64));
        patches += 1;
        k += 1;
    }
    let count = env("B4_FILES", 600);
    let mut rng = Rng::new(758_400);
    let (lo, hi) = ((4096f64).ln(), ((4 * MIB) as f64).ln());
    let files: Vec<Vec<u8>> = (0..count)
        .map(|i| {
            let size = (lo + rng.unit() * (hi - lo)).exp() as usize;
            noise(5_000_000 + i as u64, size)
        })
        .collect();
    let new_bytes: usize = files.iter().map(Vec::len).sum();
    println!(
        "{schema} b4_shape: on_b3_versions={versions} large_file_bytes={} patches={patches} stride=262144 new_files={count} new_bytes={new_bytes} gate_size={}",
        large.len(),
        versions == 10 && b3_size() == 256 * MIB && count == 600
    );
    let engine = Engine::postgres(url, schema);
    let (port, work) = engine.port(false);
    let work: Arc<dyn Fn() -> Work + Send + Sync> = Arc::from(work);
    let attributing = Arc::new(Attributing {
        inner: port,
        work: work.clone(),
        state: Mutex::default(),
    });
    let storage = Storage::new(attributing.clone()).unwrap();
    let packs0 = m.catalog(schema).packs;

    m.stat_reset();
    let w0 = work();
    let started = Instant::now();
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let next = construct(&reader, &large, Some(root), &mut emitted);
    for bytes in &files {
        construct(&reader, bytes, None, &mut emitted);
    }
    drop(reader);
    let construct_ms = started.elapsed().as_secs_f64() * 1e3;
    let wc = work() - w0;
    let cc = m.stat_classes();
    println!(
        "{schema} b4_construct: ms={construct_ms:.1} statements={} round_trips={} prepares={} base_fetch_calls={} {}",
        wc.statements,
        wc.round_trips,
        wc.prepares,
        calls(&cc, &["locate", "pack_read"]),
        classes_line(&cc)
    );

    {
        let mut s = attributing.state.lock().unwrap();
        s.own = emitted.0.iter().map(|o| o.id()).collect();
        s.references = emitted
            .0
            .iter()
            .flat_map(|o| o.references().iter().copied())
            .filter(|id| !s.own.contains(id))
            .collect();
    }
    m.stat_reset();
    let wal0 = m.wal_lsn();
    let w1 = work();
    attributing.state.lock().unwrap().active = true;
    let save_ms = save(&storage, emitted);
    attributing.state.lock().unwrap().active = false;
    let ws = work() - w1;
    let sc = m.stat_classes();
    let wal1 = m.wal_lsn();
    let packs = m.catalog(schema).packs - packs0;
    let old_base_fetch = calls(&sc, &["locate", "pack_read"]);
    let a = attributing.state.lock().unwrap();
    let base_fetch = a.base_locate.1.round_trips + a.base_read.1.round_trips;
    let attributed: u64 = [
        &a.membership,
        &a.closure,
        &a.base_locate,
        &a.base_read,
        &a.other,
    ]
    .iter()
    .map(|slot| slot.1.round_trips)
    .sum();
    println!(
        "{schema} b4_save: ms={save_ms:.1} statements={} round_trips={} prepares={} packs_added={packs} shape_ok={} \
         base_fetch_round_trips={base_fetch} a_b4_ok={} membership_locates={} old_base_fetch_round_trips={old_base_fetch} \
         old_a_b4_ok={} server_locate_calls={} server_pack_read_calls={} attributed_round_trips={attributed} attribution_complete={} \
         publication_calls={} wal_bytes={} {} {}",
        ws.statements,
        ws.round_trips,
        ws.prepares,
        packs >= 578,
        base_fetch <= 40,
        a.membership.0,
        old_base_fetch <= 40,
        calls(&sc, &["locate"]),
        calls(&sc, &["pack_read"]),
        attributed == ws.round_trips,
        sc.iter().map(|c| c.calls).sum::<i64>() - old_base_fetch,
        m.wal_bytes(&wal0, &wal1),
        a.line(),
        classes_line(&sc)
    );
    drop(a);
    m.state_set(schema, "b4_root", &roots_bytes(&[next]));
    m.state_set_u64(schema, "b4_done", 1);
}

/// `reset <conninfo> <schema>`: drops a bench Store schema and its state.
pub fn reset(args: &[String]) {
    let (url, schema) = (&args[2], &args[3]);
    if schema == "r758" || schema == "public" || schema == "gate_state" {
        panic!("refusing to drop {schema}");
    }
    let mut m = Monitor::connect(url);
    m.batch(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
    m.state_clear(schema);
    println!("reset {schema}");
}
