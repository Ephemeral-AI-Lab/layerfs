//! Shared pieces: the prototype's generators, construction, cold read, the
//! Store ports and the strict-hints hook. The generators, seeds and sizes are
//! byte-for-byte those of `delta_hint_bench.rs` (#758 prototype, built from
//! `343c59b2ca`), so B1/B2 compare 1:1 with the baselines it produced.
use layerfs_content::{
    construct_bytes_with_predecessor, ConstructionPolicy, ContentResult, FinalizedConsumer,
    FinalizedObject, ObjectId, PredecessorBase,
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, PostgresSelection, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{port::PackPersistence, Storage, StoragePolicy};
use std::sync::Arc;
use std::time::Instant;

pub const BINDING: &[u8] = b"delta-hint-bench";
pub const CURSOR: [u8; 32] = [58; 32];

/// The prototype's environment overrides. Only the defaults are gate sizes;
/// any other value is printed with the row and makes it a smoke run.
pub fn env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[derive(Default)]
pub struct Emitted(pub Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

/// The prototype's xorshift noise, unchanged.
pub fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 7;
            state ^= state >> 9;
            state ^= state << 8;
            state as u8
        })
        .collect()
}

/// The storage bench's files: `chunked` files of 2 MiB and `small` files of
/// 16-64 KiB, exactly as the prototype builds them.
pub fn bench_files(chunked: usize, small: usize) -> Vec<Vec<u8>> {
    (0..chunked)
        .map(|i| noise(i as u64 + 1, 2 << 20))
        .chain((0..small).map(|i| noise(1000 + i as u64, (16 << 10) + (i * 761) % (48 << 10))))
        .collect()
}

/// The prototype's per-version edits: three 64-byte patches per chunked
/// file and one per small file, at the prototype's positions.
pub fn bench_edit(files: &mut [Vec<u8>], version: usize, chunked: usize) {
    for (index, bytes) in files.iter_mut().enumerate() {
        let edits = if index < chunked { 3 } else { 1 };
        for e in 0..edits {
            let at = (version * 7919 + index * 104_729 + e * 31_337) % (bytes.len() - 64);
            bytes[at..at + 64].copy_from_slice(&noise((version * 97 + index * 13 + e) as u64, 64));
        }
    }
}

/// A small seeded generator for the large-file and large-commit shapes
/// (B3/B4), which the prototype did not have.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub fn config() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 3,
    }
}

/// The port's own `SqlWork` counters, counted at the driver call: one
/// statement and one round trip per execution, one round trip per cached
/// prepare, two per COPY. These are the counters the baselines report.
#[derive(Clone, Copy, Default, Debug)]
pub struct Work {
    pub statements: u64,
    pub round_trips: u64,
    pub prepares: u64,
}
impl std::ops::Sub for Work {
    type Output = Work;
    fn sub(self, o: Work) -> Work {
        Work {
            statements: self.statements - o.statements,
            round_trips: self.round_trips - o.round_trips,
            prepares: self.prepares - o.prepares,
        }
    }
}

/// A Store port and a reader of its [`Work`] counters.
pub type Port = (Arc<dyn PackPersistence>, Box<dyn Fn() -> Work>);

pub enum Engine {
    Sqlite(std::path::PathBuf),
    Postgres(PostgresSelection, String, String),
}

impl Engine {
    /// A PostgreSQL engine on the shipped defaults (`synchronous_commit =
    /// local`); `SYNC=off` keeps the prototype's diagnostic switch (B6 only).
    pub fn postgres(url: &str, schema: &str) -> Engine {
        let selection = match std::env::var("SYNC").as_deref() {
            Ok("off") => PostgresSelection::new(url, schema)
                .with_synchronous_commit(layerfs_persistence::SynchronousCommit::Off),
            _ => PostgresSelection::new(url, schema),
        };
        Engine::Postgres(selection, url.to_owned(), schema.to_owned())
    }

    pub fn port(&self, create: bool) -> Port {
        match self {
            Engine::Sqlite(path) => {
                let config = PersistenceConfig::sqlite(path)
                    .with_sqlite_profile(SqlitePersistenceProfile::Normal);
                let handles = if create {
                    SqliteHandles::create(config, StoragePolicy::frozen_default(), &self::config())
                } else {
                    SqliteHandles::open_writable(config, BINDING, CURSOR)
                }
                .expect("SQLite Store");
                let storage = handles.storage.clone();
                let work = storage.clone();
                (
                    storage,
                    Box::new(move || {
                        let w = work.diagnostics().unwrap();
                        Work {
                            statements: w.statements,
                            round_trips: w.round_trips,
                            prepares: w.prepares,
                        }
                    }),
                )
            }
            Engine::Postgres(selection, _, _) => {
                let handles = if create {
                    PostgresHandles::create(selection, StoragePolicy::frozen_default(), &config())
                } else {
                    PostgresHandles::open_writable(selection, BINDING, CURSOR)
                }
                .expect("PostgreSQL Store");
                let storage = handles.storage.clone();
                let work = storage.clone();
                (
                    storage,
                    Box::new(move || {
                        let w = work.diagnostics().unwrap();
                        Work {
                            statements: w.statements,
                            round_trips: w.round_trips,
                            prepares: w.prepares,
                        }
                    }),
                )
            }
        }
    }
}

pub fn construct(
    reader: &layerfs_storage::Reader<'_>,
    bytes: &[u8],
    base: Option<ObjectId>,
    emitted: &mut Emitted,
) -> ObjectId {
    let policy = ConstructionPolicy::frozen_default();
    layerfs_telemetry::timer::Timing::disabled("bench.construct", |scope| {
        construct_bytes_with_predecessor(
            policy,
            &policy.capacities(),
            bytes,
            base.map(|root| PredecessorBase::new(reader, root)),
            emitted,
            scope.child("content"),
        )
    })
    .0
    .unwrap()
    .root
}

/// Publishes `emitted` as one Save; returns its wall in ms.
pub fn save(storage: &Storage, emitted: Emitted) -> f64 {
    let started = Instant::now();
    let save = storage.begin_save().unwrap();
    for object in emitted.0 {
        save.accept(object).unwrap();
    }
    save.finish().unwrap();
    started.elapsed().as_secs_f64() * 1e3
}

/// Whether reads run with the test-only `strict_hints` switch.
///
/// HOOK(#758 phase 1): Phase 1 adds a test-only `strict_hints` switch on the
/// storage configuration, under which a stale ancestor hint is an error
/// instead of a counted fallback. Until it lands this build cannot run
/// strict, so `--strict-hints` is refused (NOT_WIRED) rather than silently
/// ignored. Wire it in [`strict`], the one place every read arm calls.
pub fn strict_requested(args: &[String]) -> bool {
    args.iter().any(|a| a == "--strict-hints")
}

/// Applies the strict-hints switch to `storage` (see [`strict_requested`]).
pub fn strict(storage: &Storage, requested: bool) -> &'static str {
    let _ = storage;
    if requested {
        // HOOK(#758 phase 1): replace this refusal with the Phase 1 switch
        // (the storage config field or setter it adds) and return "on".
        eprintln!("NOT_WIRED: --strict-hints needs the Phase 1 strict_hints switch");
        std::process::exit(3);
    }
    "off"
}

/// A field of the storage `Diagnostics` `{:?}` dump, or `None` when this
/// build does not have it (for example `chain_hint_stale` before Phase 1).
pub fn diag_field(diag: &str, name: &str) -> Option<u64> {
    let key = format!(" {name}: ");
    let at = diag.find(&key)? + key.len();
    diag[at..]
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// `NA` for an absent diagnostics field, the number otherwise.
pub fn field_or_na(diag: &str, name: &str) -> String {
    diag_field(diag, name).map_or_else(|| "NA".to_owned(), |v| v.to_string())
}

/// The prototype's cold read: a fresh `Storage` over `port` (empty locator
/// and pack caches), one reader per root, every root read whole.
pub fn cold_read(
    port: Arc<dyn PackPersistence>,
    roots: &[ObjectId],
    strict_hints: bool,
) -> (f64, u64, String) {
    let storage = Storage::new(port).unwrap();
    strict(&storage, strict_hints);
    let started = Instant::now();
    let mut bytes = 0u64;
    for root in roots {
        let reader = storage.reader().unwrap();
        let mut out = Vec::new();
        layerfs_telemetry::timer::Timing::disabled("bench.read", |scope| {
            layerfs_content::read_all(&reader, *root, &mut out, scope.child("read"))
        })
        .0
        .unwrap();
        bytes += out.len() as u64;
    }
    let ms = started.elapsed().as_secs_f64() * 1e3;
    (ms, bytes, format!("{:?}", storage.diagnostics()))
}

/// Encodes roots for the state table.
pub fn roots_bytes(roots: &[ObjectId]) -> Vec<u8> {
    roots.iter().flat_map(|r| r.as_bytes().to_vec()).collect()
}

pub fn roots_from(bytes: &[u8]) -> Vec<ObjectId> {
    bytes
        .chunks(32)
        .map(|c| ObjectId::from_bytes(c).unwrap())
        .collect()
}

/// `p50` and max of `values`.
pub fn p50_max(values: &[f64]) -> (f64, f64) {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (sorted[sorted.len() / 2], sorted[sorted.len() - 1])
}
