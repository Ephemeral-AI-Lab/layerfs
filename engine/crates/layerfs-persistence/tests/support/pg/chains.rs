//! Shared fixture for the PostgreSQL ancestor-list tests (#758 phase 2): a
//! disposable Store schema, file histories saved with their predecessors as
//! delta bases, the reference chain of a stored row decoded from record
//! headers, and the counted cost of a cold read.
#![allow(dead_code)]
use layerfs_content::{
    construct_bytes_with_predecessor, encode_whole_file_payload, AdvisoryPredecessors,
    ConstructionPolicy, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
    PredecessorBase,
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PostgresHandles, PostgresSelection, SqlWork};
use layerfs_storage::{
    encoding::{delta::read::ChainBases, DecompressionWorkspace, PackCache},
    location::{ObjectLocation, SignatureRow, Source},
    Diagnostics, Storage, StorageError, StoragePolicy, StorageResult,
};
use std::cell::RefCell;
use std::collections::BTreeMap;

pub const BINDING: &[u8] = b"postgresql-ancestor-lists";
pub const CURSOR: [u8; 32] = [75; 32];

/// The test server, or `None` with the NOT_RUN note.
pub fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 3,
    }
}

/// One disposable Store schema, dropped when the test ends unless kept.
pub struct Store {
    pub url: String,
    pub name: String,
    pub selection: PostgresSelection,
    pub handles: PostgresHandles,
    /// Left in place when dropped, for a later test process ([`Store::kept`]).
    pub keep: bool,
}

impl Store {
    pub fn new(url: &str, tag: &str) -> Self {
        Self::with_policy(url, tag, StoragePolicy::frozen_default())
    }
    pub fn with_policy(url: &str, tag: &str, policy: StoragePolicy) -> Self {
        let name = format!("lfs758_{tag}_{}", std::process::id());
        let selection = PostgresSelection::new(url, &name);
        drop_schema(url, &name);
        let handles = PostgresHandles::create(&selection, policy, &catalog()).expect("create");
        Self {
            url: url.to_owned(),
            name,
            selection,
            handles,
            keep: false,
        }
    }
    /// The Store schema `name`, kept across test processes: created afresh
    /// when `create`, else opened. Never dropped here; [`drop_schema`] it.
    pub fn kept(url: &str, name: &str, create: bool) -> Self {
        let selection = PostgresSelection::new(url, name);
        let handles = if create {
            drop_schema(url, name);
            PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog())
                .expect("create")
        } else {
            PostgresHandles::open_writable(&selection, BINDING, CURSOR).expect("open")
        };
        Self {
            url: url.to_owned(),
            name: name.to_owned(),
            selection,
            handles,
            keep: true,
        }
    }
    /// A second writable session over the same Store.
    pub fn open(&self) -> PostgresHandles {
        PostgresHandles::open_writable(&self.selection, BINDING, CURSOR).expect("open")
    }
    /// An admin connection whose `search_path` is the Store schema.
    pub fn admin(&self) -> postgres::Client {
        let mut client = postgres::Client::connect(&self.url, postgres::NoTls).unwrap();
        client
            .batch_execute(&format!("SET search_path TO {}", self.name))
            .unwrap();
        client
    }
    pub fn scalar(&self, sql: &str) -> i64 {
        self.admin().query_one(sql, &[]).unwrap().get(0)
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        if !self.keep {
            drop_schema(&self.url, &self.name);
        }
    }
}

pub fn drop_schema(url: &str, name: &str) {
    if let Ok(mut client) = postgres::Client::connect(url, postgres::NoTls) {
        let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {name} CASCADE"));
    }
}

/// A strict storage handle over `handles` (tests gate `chain_hint_stale`).
pub fn storage(handles: &PostgresHandles) -> Storage {
    let storage = Storage::new(handles.storage.clone()).unwrap();
    storage.set_strict_hints(true);
    storage
}

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

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

/// A small WholeFile object of `bytes`.
pub fn whole(bytes: &[u8]) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).expect("whole file"),
    )
    .expect("object")
}

/// `object` offered `base` as its delta base.
pub fn hinted(object: FinalizedObject, base: &FinalizedObject) -> FinalizedObject {
    object.with_predecessors(AdvisoryPredecessors::explicit(base.id()).unwrap())
}

/// Saves `objects` in one Save.
pub fn save(storage: &Storage, objects: Vec<FinalizedObject>) {
    let save = storage.begin_save().unwrap();
    for object in objects {
        save.accept(object).unwrap();
    }
    save.finish().unwrap();
}

/// A set of files edited version by version, each version offered the last
/// as its predecessor, so the Store holds delta chains up to the depth caps.
pub struct Files {
    pub contents: Vec<Vec<u8>>,
    pub roots: Vec<Option<ObjectId>>,
    pub version: usize,
    seed: u64,
}

impl Files {
    /// `count` files of about `size` bytes (each a little longer than the last).
    pub fn new(count: usize, size: usize, seed: u64) -> Self {
        Self {
            contents: (0..count)
                .map(|file| noise(seed * 1000 + file as u64 + 1, size + file * 977))
                .collect(),
            roots: vec![None; count],
            version: 0,
            seed,
        }
    }
    /// The next version's objects: every file edited in 64 bytes (or
    /// rewritten whole), constructed against its last version.
    pub fn emit(&mut self, storage: &Storage, rewrite: bool) -> Vec<FinalizedObject> {
        let version = self.version;
        self.version += 1;
        for (file, bytes) in self.contents.iter_mut().enumerate() {
            if rewrite {
                *bytes = noise(
                    self.seed * 7919 + (version * 1000 + file) as u64 + 7,
                    bytes.len(),
                );
            } else {
                let at = (version * 7919 + file * 104_729) % (bytes.len() - 64);
                bytes[at..at + 64]
                    .copy_from_slice(&noise(self.seed * 31 + (version * 97 + file) as u64, 64));
            }
        }
        let policy = ConstructionPolicy::frozen_default();
        let reader = storage.reader().unwrap();
        let mut emitted = Emitted::default();
        let roots: Vec<ObjectId> = self
            .contents
            .iter()
            .zip(&self.roots)
            .map(|(bytes, base)| {
                layerfs_telemetry::timer::Timing::disabled("test.construct", |scope| {
                    construct_bytes_with_predecessor(
                        policy,
                        &policy.capacities(),
                        bytes,
                        base.map(|root| PredecessorBase::new(&reader, root)),
                        &mut emitted,
                        scope.child("content"),
                    )
                })
                .0
                .unwrap()
                .root
            })
            .collect();
        drop(reader);
        self.roots = roots.into_iter().map(Some).collect();
        emitted.0
    }
    /// One Save of the next version; returns the new roots.
    pub fn step(&mut self, storage: &Storage, rewrite: bool) -> Vec<ObjectId> {
        let objects = self.emit(storage, rewrite);
        save(storage, objects);
        self.latest()
    }
    pub fn latest(&self) -> Vec<ObjectId> {
        self.roots.iter().map(|root| root.unwrap()).collect()
    }
}

/// One stored location row and its list.
#[derive(Clone, Debug)]
pub struct Row {
    pub location: ObjectLocation,
    pub ancestors: Option<Vec<u8>>,
}

fn location(row: &postgres::Row) -> ObjectLocation {
    let id = ObjectId::from_bytes(&row.get::<_, Vec<u8>>(0)).unwrap();
    ObjectLocation {
        object_id: id,
        role: ObjectRole::from_code(row.get::<_, i16>(1) as u8).unwrap(),
        canonical_length: row.get::<_, i32>(2) as usize,
        pack_id: row.get(3),
        group_number: row.get::<_, i16>(4) as usize,
        record_number: row.get::<_, i16>(5) as usize,
        base: None,
    }
}

const ROW: &str =
    "SELECT object_id,role,canonical_length,pack_id,group_number,record_number,ancestors FROM object_location";

/// Every stored location row.
pub fn rows(store: &Store) -> Vec<Row> {
    store
        .admin()
        .query(ROW, &[])
        .unwrap()
        .iter()
        .map(|row| Row {
            location: location(row),
            ancestors: row.get(6),
        })
        .collect()
}

/// The stored row of `id`.
pub fn row(store: &Store, id: ObjectId) -> Row {
    let found = store
        .admin()
        .query(
            &format!("{ROW} WHERE object_id=$1"),
            &[&id.as_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(found.len(), 1, "one row of {id}");
    Row {
        location: location(&found[0]),
        ancestors: found[0].get(6),
    }
}

fn varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// One D1 entry for `base` in a list whose previous pack is `previous`.
pub fn entry(previous: i64, base: &ObjectLocation, out: &mut Vec<u8>) {
    let delta = base.pack_id - previous;
    varint(((delta << 1) ^ (delta >> 63)) as u64, out);
    out.push(base.group_number as u8);
    varint(base.record_number as u64, out);
    varint(base.canonical_length as u64 - 1, out);
}

/// The entries of a D1 list of a row in pack `own`: pack, group, record and
/// canonical length; panics on a malformed list.
pub fn decode(own: i64, list: &[u8]) -> Vec<(i64, usize, usize, usize)> {
    let mut at = 0;
    let next = |at: &mut usize| {
        let (mut value, mut shift) = (0u64, 0);
        loop {
            let byte = list[*at];
            *at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            shift += 7;
            if byte < 0x80 {
                return value;
            }
        }
    };
    let (mut previous, mut out) = (own, Vec::new());
    while at < list.len() {
        let zigzag = next(&mut at);
        let pack = previous + (((zigzag >> 1) as i64) ^ -((zigzag & 1) as i64));
        let group = list[at] as usize;
        at += 1;
        let record = next(&mut at) as usize;
        let length = next(&mut at) as usize + 1;
        out.push((pack, group, record, length));
        previous = pack;
    }
    out
}

/// A [`Source`] over the Store's own rows through an admin connection, for
/// the record-header reference walk.
struct Reference<'a> {
    client: RefCell<postgres::Client>,
    rows: &'a BTreeMap<ObjectId, ObjectLocation>,
}

impl Source for Reference<'_> {
    fn location(&self, id: ObjectId, _ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        Ok(self.rows.get(&id).copied())
    }
    fn pack_bytes(&self, pack_id: i64) -> StorageResult<Vec<u8>> {
        let row = self
            .client
            .borrow_mut()
            .query_one("SELECT body FROM pack WHERE pack_id=$1", &[&pack_id])
            .map_err(|_| StorageError::Integrity("reference pack body"))?;
        Ok(row.get(0))
    }
    fn inode_values(&self, _ordinals: &[u32]) -> StorageResult<Vec<[u8; 73]>> {
        Err(StorageError::Integrity("reference reads no values"))
    }
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>> {
        Err(StorageError::Integrity("reference reads no signatures"))
    }
    fn write_signatures(&self, _rows: &[SignatureRow]) -> StorageResult<usize> {
        Err(StorageError::Integrity("reference writes nothing"))
    }
}

/// Each stored row's list beside the list its record headers state: every
/// base the record names, then that base's, down to a FULL record, each at
/// its winning row. Returns `(stored, reference)` per row.
pub fn lists(store: &Store) -> BTreeMap<ObjectId, (Option<Vec<u8>>, Vec<u8>)> {
    let all = rows(store);
    let located: BTreeMap<ObjectId, ObjectLocation> = all
        .iter()
        .map(|row| (row.location.object_id, row.location))
        .collect();
    let source = Reference {
        client: RefCell::new(store.admin()),
        rows: &located,
    };
    let mut packs = PackCache::new();
    let mut chain = ChainBases::new(&mut packs);
    let mut workspace = DecompressionWorkspace::new().unwrap();
    let mut out = BTreeMap::new();
    for row in all {
        let mut reference = Vec::new();
        let (mut at, mut previous) = (row.location, row.location.pack_id);
        while let Some(base) = chain.base_of(&source, &mut workspace, &at).unwrap() {
            let found = located[&base];
            entry(previous, &found, &mut reference);
            previous = found.pack_id;
            at = found;
        }
        out.insert(row.location.object_id, (row.ancestors, reference));
    }
    out
}

/// Asserts every stored list equals its reference walk; returns the number
/// of delta rows (non-empty lists).
pub fn assert_lists_exact(store: &Store) -> usize {
    let mut deltas = 0;
    for (id, (stored, reference)) in lists(store) {
        assert_eq!(
            stored.as_deref(),
            Some(reference.as_slice()),
            "the list of {id} is the record-header chain"
        );
        deltas += usize::from(!reference.is_empty());
    }
    deltas
}

/// What one read cost: counted by the Store session and the reader.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    pub statements: u64,
    pub round_trips: u64,
    pub prepares: u64,
    pub waves: u64,
    pub group_reads: u64,
    pub read_bytes: u64,
    pub stale: u64,
    pub hinted: u64,
    pub unhinted: u64,
}

impl Cost {
    pub fn of(before: &SqlWork, after: &SqlWork, read: &Diagnostics) -> Self {
        Self {
            statements: after.statements - before.statements,
            round_trips: after.round_trips - before.round_trips,
            prepares: after.prepares - before.prepares,
            waves: read.locate,
            group_reads: read.group_reads,
            read_bytes: read.group_read_bytes + read.pack_read_bytes,
            stale: read.chain_hint_stale,
            hinted: read.chain_hinted_bases,
            unhinted: read.chain_unhinted_roots,
        }
    }
    /// The amended A8 budget (gate amendment B): at most 3 round trips per
    /// locate wave with chains, 2 without, plus 1 per 2 MiB returned.
    pub fn a8_budget(&self) -> u64 {
        3 * self.waves + self.read_bytes.div_ceil(2 * 1024 * 1024)
    }
}

/// Reads `ids` on a fresh Storage (cold reader caches) over `handles`, whose
/// session keeps its prepared statements.
pub fn cold_read(
    handles: &PostgresHandles,
    ids: &[ObjectId],
    strict: bool,
) -> (StorageResult<Vec<Vec<u8>>>, Cost) {
    let storage = Storage::new(handles.storage.clone()).unwrap();
    storage.set_strict_hints(strict);
    let before = handles.diagnostics().unwrap();
    let read = storage.reader().unwrap().read_objects(ids);
    let after = handles.diagnostics().unwrap();
    (read, Cost::of(&before, &after, &storage.diagnostics()))
}
