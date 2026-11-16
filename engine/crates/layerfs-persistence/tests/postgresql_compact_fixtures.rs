//! #41 T12.3b: the PostgreSQL half of the §12 acceptance matrix.
//!
//! One fixture, built through the public APIs only, mirrors the T12.1 SQLite
//! fixture on PostgreSQL so the cross-backend rows have the same evidence on
//! both engines: the §6.4 live-heads digest is compared against the *SQLite
//! golden literal* (AC-40, AC-56), and the §11 report, §7 worklist and §8
//! swap/cleanup are asserted on the Store rows (AC-02, AC-05, AC-17, AC-18,
//! AC-25, AC-41, AC-52). Two failpoint tests cover the §9 leftovers the T9
//! note left open: a save after the swap survives the resumed Phase C
//! (AC-28) and the resumed Store is usable with totals, ACL, trigger and
//! hints checked (AC-43). A marker test covers the PostgreSQL `project_epoch`
//! bump and fence (AC-24, AC-39).
//!
//! Genuinely unreachable arms are named with the reason (real standby,
//! killed-backend lost COMMIT, a save racing the swap) in the PostgreSQL
//! acceptance record on layerfs-project#52 and PR #711; the T12.3b note that
//! used to hold them was moved out of the tree.
//!
//! Needs a writable PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` (for
//! example `host=127.0.0.1 port=55432 user=postgres dbname=postgres
//! sslmode=disable`). Without it each test reports NOT_RUN.
#![cfg(feature = "postgresql")]
mod support;

use layerfs_content::{
    encode_whole_file_payload,
    file::mapping::{self, ExtentNode, ExtentSlice, FileState},
    filesystem::{
        attributes::PortableMetadata,
        directory::{encode_directory_page, DirectoryPage},
        root::profile_id,
        scope_for_seed, FilesystemRoot, PathName,
    },
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeMetadata, InodeValue,
        LEAF_ROW_BYTES,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::error::{HistoryError, Missing};
use layerfs_history::path::NormalizedPath;
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitId, CommitMeta, CommitOutcome, CommitRequest,
    CommitStatistics, CommitVia, FileDigest, ForkRequest, ForkSource, HistoryCatalog,
    HistoryCatalogConfig, LayerId, LayerRecord, LayerStackId, LayerStackRecord, PathRemovalKind,
    PathRemovalRequest, StackInitialization,
};
use layerfs_persistence::{
    diagnostic::{Observation, Options, Outcome},
    CompactFailpoint, CompactPath, CompactRequest, PostgresHandles, PostgresSelection,
};
use layerfs_storage::{
    encoding::{encode_full, encode_prefix, raw_payload, CompressionWorkspace},
    location::{InodeValueRow, ObjectLocation, PackDomain},
    pack::{assemble, build_group, layout, PackLane},
    port::{PackPersistence, Publication, PublishedPack, Reserve},
    save::SaveProfile,
    Storage, StorageCapacities, StoragePolicy,
};
use std::sync::Arc;

const BINDING: &[u8] = b"doctor-adversarial";
const CURSOR: [u8; 32] = [71; 32];
const SCOPE_SEED: [u8; 32] = [11; 32];
/// The T12.1 SQLite transcript: one literal vector for both backends.
const SQLITE_GOLDEN: &str = include_str!("fixtures/sqlite_compact_golden.json");

/// Every string value the SQLite golden holds under `key`, in file order.
fn sqlite_literals(key: &str) -> Vec<String> {
    let needle = format!("\"{key}\": \"");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(offset) = SQLITE_GOLDEN[from..].find(&needle) {
        let start = from + offset + needle.len();
        let rest = &SQLITE_GOLDEN[start..];
        let end = rest.find('"').expect("the literal ends");
        out.push(rest[..end].to_owned());
        from = start + end + 1;
    }
    out
}

fn sqlite_literal(key: &str) -> String {
    let mut values = sqlite_literals(key);
    assert!(
        values.len() == 1,
        "the SQLite golden names {key} once, found {}",
        values.len()
    );
    values.remove(0)
}

fn sqlite_commit_literal() -> String {
    let needle = "\"commit\": [";
    let start = SQLITE_GOLDEN
        .find(needle)
        .expect("the SQLite golden commit")
        + needle.len();
    let rest = &SQLITE_GOLDEN[start..];
    let first = rest.find('"').expect("the commit literal opens") + 1;
    let rest = &rest[first..];
    rest[..rest.find('"').expect("the commit literal closes")].to_owned()
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 7,
    }
}

fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

/// One schema, dropped when the test ends, pass or fail.
struct Schema {
    url: String,
    selection: PostgresSelection,
}

impl Schema {
    fn new(url: &str, tag: &str) -> Self {
        let name = format!("lfs123b_{tag}_{}", std::process::id());
        let selection = PostgresSelection::new(url, name);
        let schema = Self {
            url: url.to_owned(),
            selection,
        };
        schema.drop_schema();
        schema
    }
    fn open(&self) -> PostgresHandles {
        PostgresHandles::open_writable(&self.selection, BINDING, CURSOR).expect("Store open")
    }
    fn create(&self) -> PostgresHandles {
        PostgresHandles::create(&self.selection, StoragePolicy::frozen_default(), &catalog())
            .expect("Store creation")
    }
    fn client(&self) -> postgres::Client {
        let mut client = postgres::Client::connect(&self.url, postgres::NoTls).expect("client");
        client
            .batch_execute(&format!("SET search_path TO {}", self.selection.schema))
            .expect("search path");
        client
    }
    fn count(&self, table: &str) -> i64 {
        let sql = format!("SELECT count(*) FROM {}.{table}", self.selection.schema);
        self.client().query_one(&sql, &[]).expect("count").get(0)
    }
    fn count_where(&self, sql: &str) -> i64 {
        let sql = sql.replace("{s}", &self.selection.schema);
        self.client().query_one(&sql, &[]).expect("count").get(0)
    }
    fn scalar(&self, sql: &str) -> i64 {
        let sql = sql.replace("{s}", &self.selection.schema);
        self.client().query_one(&sql, &[]).expect("scalar").get(0)
    }
    fn exists(&self, sql: &str) -> bool {
        self.scalar(sql) != 0
    }
    fn scratch_exists(&self, op: u8) -> bool {
        let scratch = format!("lfs_gc_{}", format!("{op:02x}").repeat(16));
        self.exists(&format!(
            "SELECT (CASE WHEN EXISTS (SELECT 1 FROM pg_namespace WHERE nspname='{scratch}') THEN 1 ELSE 0 END)::bigint"
        ))
    }
    fn drop_schema(&self) {
        if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
            let _ = client.batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {} CASCADE",
                self.selection.schema
            ));
        }
    }
}

impl Drop for Schema {
    fn drop(&mut self) {
        self.drop_schema();
    }
}

/// A Store plus the storage helpers the fixture needs (`Fixture::put` and
/// `inode` publish directly, like the SQLite doctor fixture).
struct Fixture {
    schema: Schema,
    handles: PostgresHandles,
    policy: StoragePolicy,
}

impl Fixture {
    fn new(url: &str, tag: &str) -> Self {
        let schema = Schema::new(url, tag);
        let handles = schema.create();
        Self {
            schema,
            handles,
            policy: StoragePolicy::frozen_default(),
        }
    }
    fn put(&self, role: ObjectRole, canonical: Vec<u8>) -> FinalizedObject {
        let object = FinalizedObject::new(role, canonical).expect("object");
        let record = encode_full(
            object.canonical(),
            role,
            &StorageCapacities::from_policy(self.policy).expect("capacities"),
            &mut CompressionWorkspace::new().expect("workspace"),
            &mut SaveProfile::default(),
        )
        .expect("record");
        self.record(&object, record.lane, record.record, Vec::new());
        object
    }
    fn record(
        &self,
        object: &FinalizedObject,
        lane: PackLane,
        record: Vec<u8>,
        values: Vec<InodeValueRow>,
    ) {
        let body =
            assemble(lane, &[build_group(lane, &[record], None).expect("group")]).expect("body");
        self.body(object, lane, body, values);
    }
    fn body(
        &self,
        object: &FinalizedObject,
        lane: PackLane,
        body: Vec<u8>,
        values: Vec<InodeValueRow>,
    ) {
        let reserved = self
            .handles
            .storage
            .reserve(Reserve {
                packs: 1,
                ordinals: 0,
            })
            .expect("reserve");
        self.handles
            .storage
            .publish(&Publication {
                packs: vec![PublishedPack::seal(
                    reserved.first_pack_id,
                    PackDomain::for_lane(lane),
                    Arc::new(body),
                )],
                objects: vec![ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: reserved.first_pack_id,
                    group_number: 0,
                    record_number: 0,
                }],
                inode_values: values,
                ..Publication::default()
            })
            .expect("publication");
    }
    fn inode(&self, canonical: Vec<u8>, values: Vec<[u8; 73]>) -> ObjectId {
        let object = FinalizedObject::new(ObjectRole::InodeLeaf, canonical).expect("inode");
        let reserved = self
            .handles
            .storage
            .reserve(Reserve {
                packs: 0,
                ordinals: values.len(),
            })
            .expect("ordinals");
        let ordinals: Vec<_> =
            (reserved.first_ordinal..reserved.first_ordinal + values.len() as u32).collect();
        let pooled = layerfs_content::inode_leaf::pooled_body(object.canonical(), &ordinals)
            .expect("pooled");
        let record = layerfs_storage::encoding::pool::leaf::encode_full(&pooled).expect("leaf");
        self.record(
            &object,
            PackLane::Ordinary,
            record,
            ordinals
                .into_iter()
                .zip(values)
                .map(|(ordinal, value)| InodeValueRow { ordinal, value })
                .collect(),
        );
        object.id()
    }
}

fn request(op: u8) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    }
}

fn request_at(op: u8, failpoint: CompactFailpoint) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        failpoint: Some(failpoint),
        ..CompactRequest::default()
    }
}

// ---- fixture objects -------------------------------------------------------

fn pseudo_bytes(len: usize, seed: u8) -> Vec<u8> {
    let mut state = u64::from(seed).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.extend_from_slice(&state.to_le_bytes());
    }
    out.truncate(len);
    out
}

fn whole_file(f: &Fixture, bytes: &[u8]) -> ObjectId {
    f.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).expect("whole file"),
    )
    .id()
}

/// One object published through the real Save path, so its
/// `content_signature` slot exists and the mark can list it in `gc_dead`.
fn saved_whole_file(handles: &PostgresHandles, bytes: &[u8]) -> ObjectId {
    let storage = Storage::new(handles.storage.clone()).expect("storage");
    let save = storage.begin_save().expect("begin save");
    let object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).expect("whole file"),
    )
    .expect("object");
    let id = object.id();
    save.accept(object).expect("accept");
    save.finish().expect("finish");
    id
}

/// One chunked file's content root (a FileState): the only kind whose
/// `file_digest` row the Store records (a WholeFile root is below the
/// small-file threshold).
fn chunked_file(f: &Fixture, bytes: &[u8]) -> ObjectId {
    let mut extents = Vec::new();
    for part in bytes.chunks(32_768) {
        let chunk = f
            .put(
                ObjectRole::Chunk,
                mapping::encode_chunk_object(part).expect("chunk"),
            )
            .id();
        extents.push(ExtentSlice::new(chunk, 0, part.len() as u32).expect("slice"));
    }
    let leaf = f
        .put(
            ObjectRole::ExtentLeaf,
            mapping::encode_node(
                &ExtentNode::Leaf {
                    subtree_logical_bytes: bytes.len() as u64,
                    extents,
                },
                true,
            )
            .expect("node"),
        )
        .id();
    f.put(
        ObjectRole::FileState,
        mapping::encode_file_state(FileState {
            logical_len: bytes.len() as u64,
            extent_count: (bytes.len() as u64).div_ceil(32_768),
            tree_level: 0,
            profile_id: mapping::profile_id(),
            mapping_root: leaf,
        })
        .expect("file state"),
    )
    .id()
}

/// One valid filesystem tree: a root directory whose only entry is a regular
/// file whose content root is `content`. The inode leaf is pooled.
fn tree(f: &Fixture, content: ObjectId, seed: u8) -> ObjectId {
    let directory = f
        .put(
            ObjectRole::DirectoryLeaf,
            encode_directory_page(&DirectoryPage::Leaf {
                entries: vec![(PathName::new(&format!("file-{seed}")).expect("name"), 2)],
            })
            .expect("directory"),
        )
        .id();
    let mut values = Vec::new();
    let mut rows = Vec::new();
    for (serial, kind, count, root, mode) in [
        (1u64, InodeKind::Directory, 0u64, directory, 0o755),
        (2u64, InodeKind::RegularFile, 1u64, content, 0o644),
    ] {
        let value = encode_inode_value(InodeValue {
            kind,
            namespace_ref_count: count,
            content_root: root,
            metadata_root: InodeMetadata::Inline(PortableMetadata {
                mode,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
            }),
        });
        values.push(value);
        rows.push(InodeLeafRow { serial, value });
    }
    let inode = f.inode(
        InodeLeaf {
            subtree_bytes: LEAF_ROW_BYTES as u64 * rows.len() as u64,
            rows,
        }
        .encode()
        .expect("inode leaf"),
        values,
    );
    f.put(
        ObjectRole::FilesystemRoot,
        FilesystemRoot::new(profile_id(), scope_for_seed(SCOPE_SEED), 1, inode)
            .expect("root")
            .encode()
            .expect("root bytes"),
    )
    .id()
}

/// Publishes one pack holding each object as its own group, so a retained pack
/// can hold one reached and one unreached record.
fn publish_groups(f: &Fixture, objects: &[FinalizedObject]) -> (i64, Vec<u8>) {
    let capacities = StorageCapacities::from_policy(f.policy).expect("capacities");
    let mut workspace = CompressionWorkspace::new().expect("workspace");
    let mut profile = SaveProfile::default();
    let groups: Vec<_> = objects
        .iter()
        .map(|object| {
            let record = encode_full(
                object.canonical(),
                object.role(),
                &capacities,
                &mut workspace,
                &mut profile,
            )
            .expect("record");
            build_group(PackLane::WholeFile, &[record.record], None).expect("group")
        })
        .collect();
    let body = assemble(PackLane::WholeFile, &groups).expect("body");
    let reserved = f
        .handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .expect("reserve");
    f.handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::Payload,
                Arc::new(body.clone()),
            )],
            objects: objects
                .iter()
                .enumerate()
                .map(|(group, object)| ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: reserved.first_pack_id,
                    group_number: group,
                    record_number: 0,
                })
                .collect(),
            ..Publication::default()
        })
        .expect("publication");
    (reserved.first_pack_id, body)
}

/// Publishes `target` as a PREFIX record over `base`, asserting the encoder
/// kept the delta (a full record would hide the base from the mark).
fn prefix_record(f: &Fixture, target: &FinalizedObject, base: &FinalizedObject, base_id: ObjectId) {
    let capacities = StorageCapacities::from_policy(f.policy).expect("capacities");
    let record = encode_prefix(
        target.canonical(),
        target.role(),
        base_id,
        raw_payload(base.canonical(), base.role()).expect("payload"),
        &capacities,
        &mut CompressionWorkspace::new().expect("workspace"),
        &mut SaveProfile::default(),
    )
    .expect("prefix");
    assert_eq!(
        record.base,
        Some(base_id),
        "the fixture needs a PREFIX record, not a full one"
    );
    f.record(target, record.lane, record.record, Vec::new());
}

// ---- fixture history ------------------------------------------------------

fn initialize(handles: &PostgresHandles, tag: u8, genesis: ObjectId) -> LayerStackRecord {
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: support::stack(tag),
            name: support::name(&format!("project-{tag}")),
            scope: scope_for_seed(SCOPE_SEED).object(),
            profile: profile_id(),
            genesis_root: genesis,
        })
        .expect("stack")
}

fn fork(handles: &PostgresHandles, stack: LayerStackId, branch: BranchId, tag: u8) {
    let record = handles.history.layer_stack(stack).unwrap().unwrap();
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: support::name(&format!("branch-{tag}")),
            source: ForkSource::Layer(record.head_layer),
        })
        .expect("fork");
}

fn expected_root(
    handles: &PostgresHandles,
    branch: BranchId,
) -> (Option<CommitId>, LayerId, ObjectId) {
    let snapshot = handles.history.branch(branch).unwrap().unwrap();
    let root = match snapshot.head_commit {
        Some(head) => handles.history.commit(head).unwrap().unwrap().root,
        None => {
            handles
                .history
                .layer(snapshot.base_layer)
                .unwrap()
                .unwrap()
                .root
        }
    };
    (snapshot.head_commit, snapshot.base_layer, root)
}

fn commit_request(
    handles: &PostgresHandles,
    branch: BranchId,
    candidate_root: ObjectId,
) -> CommitRequest {
    let (head, base, root) = expected_root(handles, branch);
    CommitRequest {
        branch,
        expected_head: head,
        expected_base: base,
        expected_root: root,
        candidate_root,
        profile: profile_id(),
        scope: scope_for_seed(SCOPE_SEED).object(),
        second_parent: None,
        meta: CommitMeta::new(CommitVia::Files),
        force: false,
    }
}

fn commit(
    handles: &PostgresHandles,
    branch: BranchId,
    candidate_root: ObjectId,
    logical: u64,
) -> CommitId {
    let (head, base, root) = expected_root(handles, branch);
    let meta = CommitMeta {
        statistics: CommitStatistics {
            logical,
            ..CommitStatistics::ZERO
        },
        ..CommitMeta::new(CommitVia::Files)
    };
    match handles
        .history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: head,
            expected_base: base,
            expected_root: root,
            candidate_root,
            profile: profile_id(),
            scope: scope_for_seed(SCOPE_SEED).object(),
            second_parent: None,
            meta,
            force: false,
        })
        .expect("commit")
    {
        CommitOutcome::Committed { record, .. } => record.id,
        other => panic!("commit: {other:?}"),
    }
}

fn publish(handles: &PostgresHandles, stack: LayerStackId, branch: BranchId) -> LayerRecord {
    let branch_record = handles.history.branch(branch).unwrap().unwrap();
    let stack_record = handles.history.layer_stack(stack).unwrap().unwrap();
    match handles
        .history
        .add_layer(&AddLayerRequest {
            stack,
            branch,
            commit: branch_record.head_commit.expect("a committed Branch"),
            expected_stack_head: stack_record.head_layer,
            expected_branch_base: branch_record.base_layer,
        })
        .expect("publish")
    {
        AddLayerOutcome::Added { layer } => layer,
        other => panic!("publish: {other:?}"),
    }
}

// ---- raw Store facts ------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
struct Totals {
    logical: u64,
    canonical: u64,
    stored: u64,
    metadata: u64,
}

fn totals(schema: &Schema) -> Totals {
    let row = schema
        .client()
        .query_one(
            &format!(
                "SELECT logical,canonical,stored,metadata FROM {}.store_totals WHERE id=1",
                schema.selection.schema
            ),
            &[],
        )
        .expect("totals");
    Totals {
        logical: u64::try_from(row.get::<_, i64>(0)).unwrap(),
        canonical: u64::try_from(row.get::<_, i64>(1)).unwrap(),
        stored: u64::try_from(row.get::<_, i64>(2)).unwrap(),
        metadata: u64::try_from(row.get::<_, i64>(3)).unwrap(),
    }
}

#[derive(Debug)]
struct PackFacts {
    total: u64,
    live_bytes: u64,
    dead_bytes: u64,
    live: Vec<i64>,
    dead: Vec<i64>,
}

fn pack_facts(schema: &Schema, live_packs: &[i64]) -> PackFacts {
    let rows = schema
        .client()
        .query(
            &format!(
                "SELECT pack_id, length FROM {}.pack ORDER BY pack_id",
                schema.selection.schema
            ),
            &[],
        )
        .expect("packs");
    let mut facts = PackFacts {
        total: 0,
        live_bytes: 0,
        dead_bytes: 0,
        live: Vec::new(),
        dead: Vec::new(),
    };
    for row in rows {
        let pack: i64 = row.get(0);
        let length = u64::try_from(row.get::<_, i64>(1)).unwrap();
        facts.total += length;
        if live_packs.contains(&pack) {
            facts.live.push(pack);
            facts.live_bytes += length;
        } else {
            facts.dead.push(pack);
        }
    }
    facts.dead_bytes = facts.total - facts.live_bytes;
    facts
}

fn pack_of(schema: &Schema, id: ObjectId) -> Option<i64> {
    let object = id.as_bytes();
    let row = schema
        .client()
        .query_opt(
            &format!(
                "SELECT pack_id FROM {}.object_location WHERE object_id=$1",
                schema.selection.schema
            ),
            &[&object.as_slice()],
        )
        .expect("object location");
    row.map(|row| row.get(0))
}

fn object_count(schema: &Schema, id: ObjectId) -> i64 {
    let object = id.as_bytes();
    schema
        .client()
        .query_one(
            &format!(
                "SELECT count(*) FROM {}.object_location WHERE object_id=$1",
                schema.selection.schema
            ),
            &[&object.as_slice()],
        )
        .expect("object count")
        .get(0)
}

fn pack_length(schema: &Schema, pack: i64) -> u64 {
    let length = schema
        .client()
        .query_one(
            &format!(
                "SELECT length FROM {}.pack WHERE pack_id=$1",
                schema.selection.schema
            ),
            &[&pack],
        )
        .expect("pack length")
        .get::<_, i64>(0);
    u64::try_from(length).unwrap()
}

fn pack_row(schema: &Schema, pack: i64) -> i64 {
    schema
        .client()
        .query_one(
            &format!(
                "SELECT count(*) FROM {}.pack WHERE pack_id=$1",
                schema.selection.schema
            ),
            &[&pack],
        )
        .expect("pack row")
        .get(0)
}

fn branch_row(schema: &Schema, id: BranchId) -> i64 {
    let bytes = id.as_slice();
    schema
        .client()
        .query_one(
            &format!(
                "SELECT count(*) FROM {}.branch WHERE branch_id=$1",
                schema.selection.schema
            ),
            &[&bytes],
        )
        .expect("branch row")
        .get(0)
}

fn stack_row(schema: &Schema, id: LayerStackId) -> i64 {
    let bytes = id.as_slice();
    schema
        .client()
        .query_one(
            &format!(
                "SELECT count(*) FROM {}.layer_stack WHERE layer_stack_id=$1",
                schema.selection.schema
            ),
            &[&bytes],
        )
        .expect("stack row")
        .get(0)
}

fn retired_branches(schema: &Schema) -> Vec<BranchId> {
    let rows = schema
        .client()
        .query(
            &format!(
                "SELECT branch_id FROM {}.branch_retired ORDER BY branch_id",
                schema.selection.schema
            ),
            &[],
        )
        .expect("retired branches");
    rows.iter()
        .map(|row| {
            let bytes: Vec<u8> = row.get(0);
            BranchId::from_slice(&bytes).unwrap()
        })
        .collect()
}

fn retired_stacks(schema: &Schema) -> i64 {
    schema.count("layer_stack_retired")
}

fn digest_rows(schema: &Schema, digest: &[u8; 32]) -> i64 {
    schema
        .client()
        .query_one(
            &format!(
                "SELECT count(*) FROM {}.file_digest WHERE digest=$1",
                schema.selection.schema
            ),
            &[&digest.as_slice()],
        )
        .expect("digest rows")
        .get(0)
}

fn epoch(schema: &Schema, stack: LayerStackId) -> i64 {
    let bytes = stack.as_slice();
    schema
        .client()
        .query_one(
            &format!(
                "SELECT epoch FROM {}.project_epoch WHERE layer_stack_id=$1",
                schema.selection.schema
            ),
            &[&bytes],
        )
        .expect("project epoch")
        .get(0)
}

fn epoch_rows(schema: &Schema) -> i64 {
    schema.count("project_epoch")
}

fn next_serial(schema: &Schema) -> i64 {
    schema.scalar("SELECT next_history_serial FROM {s}.history_meta WHERE id=1")
}

fn inode_block_bytes(schema: &Schema) -> u64 {
    u64::try_from(
        schema.scalar("SELECT COALESCE(SUM(octet_length(body)),0) FROM {s}.inode_value_block"),
    )
    .unwrap()
}

fn sum_pack_lengths(schema: &Schema) -> u64 {
    u64::try_from(schema.scalar("SELECT COALESCE(SUM(length)::bigint,0) FROM {s}.pack")).unwrap()
}

/// `pg_total_relation_size` of the two payload tables, the sample
/// `physicalShrink` uses.
fn payload_relation_bytes(schema: &Schema) -> u64 {
    u64::try_from(schema.scalar(
        "SELECT COALESCE(SUM(pg_total_relation_size(c.oid))::bigint,0) FROM pg_class c \
             JOIN pg_namespace n ON n.oid=c.relnamespace \
             WHERE n.nspname='{s}' AND c.relname IN ('pack','object_location')",
    ))
    .unwrap()
}

fn sum_canonical(schema: &Schema) -> u64 {
    u64::try_from(
        schema.scalar("SELECT COALESCE(SUM(canonical_length)::bigint,0) FROM {s}.object_location"),
    )
    .unwrap()
}

fn sum_metadata_packs(schema: &Schema) -> u64 {
    u64::try_from(
        schema.scalar("SELECT COALESCE(SUM(length)::bigint,0) FROM {s}.pack WHERE domain=0"),
    )
    .unwrap()
}

fn sum_history_logical(schema: &Schema) -> u64 {
    u64::try_from(
        schema.scalar("SELECT COALESCE(SUM(logical),0)::bigint FROM {s}.\"commit\"")
            + schema.scalar("SELECT COALESCE(SUM(logical),0)::bigint FROM {s}.layer"),
    )
    .unwrap()
}

fn generation(handles: &PostgresHandles) -> u64 {
    handles.history.incarnation()
}

fn db_generation(schema: &Schema) -> u64 {
    u64::try_from(schema.scalar("SELECT catalog_incarnation FROM {s}.history_meta WHERE id=1"))
        .unwrap()
}

fn located(handles: &PostgresHandles, id: ObjectId) -> bool {
    let mut found = Vec::new();
    handles.storage.locate(&[id], &mut found).unwrap();
    !found.is_empty()
}

fn audit(handles: &PostgresHandles) -> Vec<Observation> {
    let mut out = Vec::new();
    handles
        .diagnose(
            Options {
                quick: false,
                ..Options::default()
            },
            &mut |observation| out.push(observation),
        )
        .expect("diagnose");
    out
}

fn assert_doctor_clean(handles: &PostgresHandles) {
    let out = audit(handles);
    assert!(
        !out.iter()
            .any(|observation| matches!(observation.outcome, Outcome::Fail | Outcome::Unknown)),
        "the compacted Store must pass the full doctor: {out:?}"
    );
}

/// §9/§8: after a settle no op row, dead set, intent or scratch schema remains
/// and every `store_totals` row equals the sum of the rows it summarises.
fn assert_settled(schema: &Schema, op: u8) {
    assert_eq!(schema.count("gc_op"), 0, "gc_op is cleared on settle");
    assert_eq!(schema.count("gc_dead"), 0, "gc_dead is cleared on settle");
    assert_eq!(
        schema.count_where("SELECT count(*) FROM {s}.gc_intent WHERE kind='compaction'"),
        0,
        "the compaction intent is released"
    );
    assert!(
        !schema.scratch_exists(op),
        "the recorded scratch schema is dropped"
    );
    let logical = schema.scalar("SELECT logical FROM {s}.store_totals WHERE id=1");
    let sum = schema.scalar(
        "SELECT (SELECT COALESCE(SUM(logical)::bigint,0) FROM {s}.\"commit\")+(SELECT COALESCE(SUM(logical)::bigint,0) FROM {s}.layer)",
    );
    assert_eq!(logical, sum, "store_totals.logical tracks the history rows");
    let canonical = schema.scalar("SELECT canonical FROM {s}.store_totals WHERE id=1");
    let canonical_sum =
        schema.scalar("SELECT COALESCE(SUM(canonical_length)::bigint,0) FROM {s}.object_location");
    assert_eq!(canonical, canonical_sum, "store_totals.canonical");
    let stored = schema.scalar("SELECT stored FROM {s}.store_totals WHERE id=1");
    let stored_sum = schema.scalar("SELECT COALESCE(SUM(length)::bigint,0) FROM {s}.pack");
    assert_eq!(stored, stored_sum, "store_totals.stored");
    let metadata = schema.scalar("SELECT metadata FROM {s}.store_totals WHERE id=1");
    let metadata_sum =
        schema.scalar("SELECT COALESCE(SUM(length)::bigint,0) FROM {s}.pack WHERE domain=0");
    assert_eq!(metadata, metadata_sum, "store_totals.metadata");
}

/// AC-43: the Store is usable at `G + 1` with its totals, the recreated
/// `pack_immutable` trigger and function, the same owner and cleared hints.
fn assert_phase_c_settled(schema: &Schema, op: u8) {
    assert_settled(schema, op);
    let trigger = schema.scalar(
        "SELECT count(*) FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='{s}' AND t.tgname='pack_immutable' AND NOT t.tgisinternal",
    );
    assert_eq!(trigger, 1, "the swap recreates pack_immutable");
    let function = schema.scalar(
        "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='{s}' AND p.proname='pack_immutable'",
    );
    assert_eq!(function, 1, "the trigger is bound to the schema's function");
    let mut client = schema.client();
    let refused = client.batch_execute(&format!(
        "UPDATE {schema}.pack SET length=length WHERE pack_id=(SELECT min(pack_id) FROM {schema}.pack)",
        schema = schema.selection.schema
    ));
    assert!(refused.is_err(), "the trigger still refuses an update");
    let owner: String = client
        .query_one(
            &format!(
                "SELECT tableowner FROM pg_tables WHERE schemaname='{}' AND tablename='pack'",
                schema.selection.schema
            ),
            &[],
        )
        .expect("owner")
        .get(0);
    let current: String = client
        .query_one("SELECT current_user", &[])
        .expect("current user")
        .get(0);
    assert_eq!(owner, current, "the swapped table keeps its owner");
    assert_eq!(
        schema.count("binding_hint_block"),
        0,
        "Phase C clears the binding hints"
    );
    assert_eq!(
        schema.scalar(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema='{s}' AND table_name='inode'"
        ),
        0,
        "no inode column on PostgreSQL"
    );
}

// ---- cases ----------------------------------------------------------------

/// The §6.4 vector: two live stacks, an absent head and a retirement, exactly
/// the T12.1 SQLite fixture. One literal must hold on both backends.
#[test]
fn the_live_heads_digest_vector_is_the_sqlite_literal() {
    let Some(url) = url() else { return };
    let f = Fixture::new(&url, "digest");
    // Out-of-order creation: the digest orders by identity, not creation.
    let root_9 = tree(&f, whole_file(&f, b"digest stack nine content"), 9);
    let stack_9 = initialize(&f.handles, 9, root_9);
    let root_3 = tree(&f, whole_file(&f, b"digest stack three content"), 10);
    let stack_3 = initialize(&f.handles, 3, root_3);
    let root_7 = tree(&f, whole_file(&f, b"digest retired stack content"), 11);
    let stack_7 = initialize(&f.handles, 7, root_7);
    f.handles
        .history
        .mark_stack_deleted(stack_7.id, 7_000)
        .unwrap();

    let branch_9 = support::branch_id(9);
    fork(&f.handles, stack_9.id, branch_9, 9);
    let branch_3 = support::branch_id(3);
    fork(&f.handles, stack_3.id, branch_3, 3);
    let branch_5 = support::branch_id(5);
    fork(&f.handles, stack_9.id, branch_5, 5);
    let commit_root = tree(&f, whole_file(&f, b"digest commit content"), 12);
    let commit_id = commit(&f.handles, branch_3, commit_root, 90);
    assert_eq!(
        hex(commit_id.as_slice()),
        sqlite_commit_literal(),
        "the same content-addressed Commit on both backends"
    );

    // branch_5 has never committed: an absent head is hashed as 33 zero bytes.
    let with_empty_head = f.handles.history.mark().unwrap().live_heads_digest();
    assert_eq!(
        with_empty_head,
        f.handles.history.mark().unwrap().live_heads_digest(),
        "the fixed fixture pins one digest"
    );
    assert_eq!(
        hex(&with_empty_head),
        sqlite_literal("absentHeadDigest"),
        "an absent head is the SQLite golden literal"
    );
    // A retired Branch is absent, not zeroed.
    f.handles
        .history
        .mark_branch_removed(branch_5, 5_000)
        .unwrap();
    let retired_branch = f.handles.history.mark().unwrap().live_heads_digest();
    assert_ne!(with_empty_head, retired_branch);
    assert_eq!(
        hex(&retired_branch),
        sqlite_literal("retiredBranchDigest"),
        "a retired Branch row is absent, not zeroed"
    );
    assert_doctor_clean(&f.handles);
    assert_eq!(retired_stacks(&f.schema), 1);
}

struct FixtureIds {
    stacks_live: Vec<LayerStackId>,
    stacks_collected: Vec<LayerStackId>,
    branches_live: Vec<BranchId>,
    branches_retained_retired: Vec<BranchId>,
    branches_collected: Vec<BranchId>,
    commits_retained: Vec<CommitId>,
    commits_collected: Vec<CommitId>,
    layers_retained: Vec<LayerId>,
    layers_collected: Vec<LayerId>,
    objects_live: Vec<ObjectId>,
    objects_dead: Vec<ObjectId>,
    delta_bases_sorted: Vec<ObjectId>,
    delta_chain: Vec<(ObjectId, i64)>,
    delta_target: ObjectId,
    live_group: ObjectId,
    dead_group: ObjectId,
    shared_pack: i64,
    shared_pack_length: u64,
    shared_dead_group: u64,
    digest_collected: [u8; 32],
    digest_retained: [u8; 32],
}

/// The T12.1 rebuild fixture, backend-for-backend: retained history with a
/// deleted project, a retained retired Branch, a collected retired Branch, a
/// depth-2 delta chain held only as bases, one retained pack with an unreached
/// group, pooled inode values and the two digest roots.
fn rebuild_fixture(f: &Fixture) -> FixtureIds {
    // The dead packs: unreferenced whole files, each in its own pack.
    let mut objects_dead = Vec::new();
    for index in 0..12u8 {
        let payload = pseudo_bytes(4096, 0xA0 + index);
        objects_dead.push(whole_file(f, &payload));
    }
    let retained_chunked_payload = pseudo_bytes(131_072, 0x77);
    let retained_chunked = chunked_file(f, &retained_chunked_payload);
    let collected_chunked_payload = pseudo_bytes(131_072, 0x88);
    let collected_chunked = chunked_file(f, &collected_chunked_payload);
    let digest_collected = *blake3::hash(&collected_chunked_payload).as_bytes();
    let digest_retained = *blake3::hash(&retained_chunked_payload).as_bytes();

    // A depth-2 delta chain: base2 <- base1 <- target. Only the target is a
    // retained root, so both bases are kept only as delta bases.
    let base2_payload = pseudo_bytes(512, 0x11);
    let base1_payload = [base2_payload.as_slice(), b" v2"].concat();
    let target_payload = [base1_payload.as_slice(), b" v3"].concat();
    let base2 = whole_file(f, &base2_payload);
    let base2_object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&base2_payload).unwrap(),
    )
    .unwrap();
    let base1_object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&base1_payload).unwrap(),
    )
    .unwrap();
    prefix_record(f, &base1_object, &base2_object, base2);
    let base1 = base1_object.id();
    let target_object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&target_payload).unwrap(),
    )
    .unwrap();
    prefix_record(f, &target_object, &base1_object, base1);
    let delta_target = target_object.id();

    // One retained pack holding one reached and one unreached record.
    let live_payload = pseudo_bytes(256, 0x55);
    let dead_payload = pseudo_bytes(256, 0x66);
    let live_group = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&live_payload).unwrap(),
    )
    .unwrap();
    let dead_group = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&dead_payload).unwrap(),
    )
    .unwrap();
    let (shared_pack, shared_body) = publish_groups(f, &[live_group.clone(), dead_group.clone()]);
    let shared_header = layout::parse_header(&shared_body).unwrap();
    let dead_view = layout::group_view(&shared_body, shared_header, 1).unwrap();
    let shared_dead_group = (dead_view.end - dead_view.start) as u64;
    let shared_pack_length = pack_length(&f.schema, shared_pack);

    // Content objects of the trees that are expected to be collected.
    let collected_content_a4 = collected_chunked;
    let collected_content_b0 = whole_file(f, b"deleted project genesis content");
    let collected_content_b1 = whole_file(f, b"deleted project commit content");
    let content_c3 = whole_file(f, b"live branch commit content");

    let root_a0 = tree(f, live_group.id(), 0);
    let root_a1 = tree(f, delta_target, 1);
    let root_a2 = tree(f, retained_chunked, 2);
    let root_a3 = tree(f, content_c3, 3);
    let root_a4 = tree(f, collected_content_a4, 4);
    let root_b0 = tree(f, collected_content_b0, 5);
    let root_b1 = tree(f, collected_content_b1, 6);

    let stack_a = initialize(&f.handles, 1, root_a0);
    let branch_published = support::branch_id(1);
    fork(&f.handles, stack_a.id, branch_published, 1);
    let commit_1 = commit(&f.handles, branch_published, root_a1, 120);
    let layer_1 = publish(&f.handles, stack_a.id, branch_published);
    let commit_2 = commit(&f.handles, branch_published, root_a2, 240);
    let layer_2 = publish(&f.handles, stack_a.id, branch_published);

    let branch_live = support::branch_id(2);
    fork(&f.handles, stack_a.id, branch_live, 2);
    let commit_3 = commit(&f.handles, branch_live, root_a3, 360);

    let branch_retired = support::branch_id(3);
    fork(&f.handles, stack_a.id, branch_retired, 3);
    let commit_4 = commit(&f.handles, branch_retired, root_a4, 70);

    let stack_b = initialize(&f.handles, 2, root_b0);
    let branch_b = support::branch_id(4);
    fork(&f.handles, stack_b.id, branch_b, 4);
    let commit_5 = commit(&f.handles, branch_b, root_b1, 40);
    let layer_b = publish(&f.handles, stack_b.id, branch_b);

    // A retired Branch a retained Layer still names keeps its row and marker.
    f.handles
        .history
        .mark_branch_removed(branch_published, 1_000)
        .unwrap();
    // A retired Branch nothing retained reaches is collected whole.
    f.handles
        .history
        .mark_branch_removed(branch_retired, 1_500)
        .unwrap();
    // A deleted project goes whole, marker included.
    f.handles
        .history
        .mark_stack_deleted(stack_b.id, 2_000)
        .unwrap();

    // AC-18: one digest root no retained root reaches, one it does.
    f.handles
        .history
        .record_file_digests(&[
            FileDigest {
                root: *collected_chunked.as_bytes(),
                length: collected_chunked_payload.len() as u64,
                digest: digest_collected,
            },
            FileDigest {
                root: *retained_chunked.as_bytes(),
                length: retained_chunked_payload.len() as u64,
                digest: digest_retained,
            },
        ])
        .unwrap();

    let delta_chain: Vec<(ObjectId, i64)> = [base2, base1, delta_target]
        .iter()
        .map(|id| (*id, pack_of(&f.schema, *id).expect("a delta-chain pack")))
        .collect();
    let mut delta_bases_sorted = vec![base1, base2];
    delta_bases_sorted.sort();
    objects_dead.push(dead_group.id());
    objects_dead.extend([
        root_a4,
        root_b0,
        root_b1,
        collected_content_a4,
        collected_content_b0,
        collected_content_b1,
    ]);
    let objects_live = vec![
        root_a0,
        root_a1,
        root_a2,
        root_a3,
        live_group.id(),
        retained_chunked,
        base2,
        base1,
        delta_target,
        content_c3,
    ];
    FixtureIds {
        stacks_live: vec![stack_a.id],
        stacks_collected: vec![stack_b.id],
        branches_live: vec![branch_live],
        branches_retained_retired: vec![branch_published],
        branches_collected: vec![branch_retired, branch_b],
        commits_retained: vec![commit_1, commit_2, commit_3],
        commits_collected: vec![commit_4, commit_5],
        layers_retained: vec![stack_a.head_layer, layer_1.id, layer_2.id],
        layers_collected: vec![stack_b.head_layer, layer_b.id],
        objects_live,
        objects_dead,
        delta_bases_sorted,
        delta_chain,
        delta_target,
        live_group: live_group.id(),
        dead_group: dead_group.id(),
        shared_pack,
        shared_pack_length,
        shared_dead_group,
        digest_collected,
        digest_retained,
    }
}

// ---- tests ----------------------------------------------------------------

/// AC-02, AC-05, AC-17, AC-18, AC-25, AC-40, AC-41, AC-43, AC-52: the whole
/// T12.1 rebuild fixture on PostgreSQL. The `live_heads_digest` is the SQLite
/// golden literal; the §11 report is non-trivial; the depth-2 chain is kept
/// and its packs keep their ids and lengths; markers and digest rows follow
/// the retained set; the settled Store passes the full doctor.
#[test]
fn the_rebuild_report_and_markers_hold_on_postgresql() {
    let Some(url) = url() else { return };
    let f = Fixture::new(&url, "rebuild");
    let ids = rebuild_fixture(&f);
    let schema = &f.schema;
    let before = totals(schema);
    let generation_before = generation(&f.handles);
    let serial_before = next_serial(schema);
    assert_eq!(epoch_rows(schema), 2, "one project_epoch row per stack");
    let epoch_before = epoch(schema, ids.stacks_live[0]);

    let mark = f.handles.history.mark().unwrap();
    assert_eq!(
        mark.live_heads_digest(),
        f.handles.history.mark().unwrap().live_heads_digest(),
        "the digest is stable across marks"
    );
    // AC-41: the Phase M mark reads live pack bodies (not the persisted dead
    // table), each live pack counted once by the body extent it transferred.
    let mark_bytes = mark.mark_bytes();
    assert!(
        mark_bytes > 0,
        "AC-41: the reference-bearing closure reads pack bodies"
    );
    assert!(
        mark_bytes <= sum_pack_lengths(schema),
        "AC-41: markBytes is bounded by the Store's stored bytes"
    );
    assert_eq!(
        mark_bytes,
        f.handles.history.mark().unwrap().mark_bytes(),
        "AC-41: markBytes is stable across marks"
    );
    let literals = sqlite_literals("liveHeadsDigest");
    assert_eq!(literals.len(), 3, "the SQLite golden holds three digests");
    assert_eq!(
        hex(&mark.live_heads_digest()),
        literals[2],
        "the rebuild digest is the SQLite literal: one vector on both backends"
    );
    let live_packs = mark.objects().packs().to_vec();
    let live_lengths: Vec<(i64, u64)> = live_packs
        .iter()
        .map(|pack| (*pack, pack_length(schema, *pack)))
        .collect();
    let facts = pack_facts(schema, &live_packs);
    let inode_blocks = inode_block_bytes(schema);
    assert_eq!(mark.objects().waste().inode_blocks_bytes(), inode_blocks);
    assert_eq!(
        mark.objects().waste().dead_in_retained_packs_bytes(),
        ids.shared_dead_group,
        "the unreached group of the retained pack is keptWaste"
    );
    assert_eq!(
        mark.objects().waste().delta_base_references(),
        ids.delta_bases_sorted.as_slice(),
        "a depth-2 chain reports both bases, in identity order"
    );
    let expected_bases: Vec<[u8; 32]> = ids
        .delta_bases_sorted
        .iter()
        .map(|id| *id.as_bytes())
        .collect();
    assert!(
        mark.objects().waste().delta_bases_bytes() > 0,
        "the bases are kept only as bases"
    );
    for id in &ids.objects_live {
        assert!(mark.objects().contains(*id), "live object {id} is retained");
    }
    for id in &ids.objects_dead {
        assert!(!mark.objects().contains(*id), "object {id} is collected");
    }
    assert!(
        facts.dead_bytes * 10 >= facts.total,
        "the fixture must take the rebuild path: {} dead of {}",
        facts.dead_bytes,
        facts.total
    );

    let receipt = f.handles.compact(&request(0x72)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, generation_before);
    assert_eq!(receipt.generation_after, generation_before + 1);
    assert_eq!(receipt.dead_bytes_bound, facts.dead_bytes);
    assert_eq!(receipt.dead_bytes_refined, receipt.dead_bytes_bound);
    assert!(receipt.physical_shrink > 0);
    assert!(receipt.peak_temp_bytes > 0);
    assert!(receipt.estimate);
    assert_eq!(
        receipt.kept_waste.dead_in_retained_packs_bytes,
        ids.shared_dead_group
    );
    assert_eq!(receipt.kept_waste.inode_blocks_bytes, inode_blocks);
    assert_eq!(
        receipt.kept_waste.delta_base_references, expected_bases,
        "PG keeps the delta bases and reports them"
    );
    assert!(receipt.kept_waste.delta_bases_bytes > 0);
    assert_eq!(
        receipt.residue.bytes, 0,
        "a successful settle leaves no residue"
    );
    assert!(receipt.residue.artifacts.is_empty());
    assert!(receipt.residue.reason.is_none());
    assert_eq!(
        receipt.backend_reusable, 0,
        "PostgreSQL rewrites in place: no reusable free pages"
    );
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "backendReusable" && zero.reason == "postgresql-in-place"));

    let reopened = schema.open();
    assert_eq!(generation(&reopened), generation_before + 1);
    let after = totals(schema);
    // #711: canonical bytes of the collected objects, deduplicated by object,
    // not the history-tree `logical` delta (which is only the 160 C4/stack-B/
    // C5/Lb rows here).
    assert_eq!(
        receipt.logical_discarded,
        before.canonical - after.canonical,
        "logicalDiscarded is the canonical bytes of the collected objects"
    );
    assert!(receipt.logical_discarded > 160);
    assert_eq!(before.logical - after.logical, 160);
    assert_eq!(after.stored, sum_pack_lengths(schema));
    assert_eq!(after.canonical, sum_canonical(schema));
    assert_eq!(after.metadata, sum_metadata_packs(schema));
    assert_eq!(after.logical, sum_history_logical(schema));

    // Live objects are readable, including the delta target through its chain.
    let storage = Storage::new(reopened.storage.clone()).unwrap();
    let reader = storage.reader().unwrap();
    let canonicals = reader
        .read_objects(&[ids.delta_target, ids.live_group])
        .unwrap();
    assert_eq!(canonicals.len(), 2);
    assert!(!canonicals[0].is_empty());
    assert!(!canonicals[1].is_empty());
    for id in &ids.objects_live {
        assert!(located(&reopened, *id), "live object {id} survives");
    }
    for id in &ids.objects_dead {
        assert!(!located(&reopened, *id), "dead object {id} is gone");
    }
    for pack in &facts.dead {
        assert_eq!(pack_row(schema, *pack), 0, "dead pack {pack} is removed");
    }
    for (pack, length) in &live_lengths {
        assert_eq!(pack_row(schema, *pack), 1, "live pack {pack} stays");
        assert_eq!(
            pack_length(schema, *pack),
            *length,
            "AC-41: the swap copies the row and moves no pack bytes"
        );
    }
    // The retained pack keeps the unreached group: repacking is out of 2.0.
    assert_eq!(pack_of(schema, ids.live_group), Some(ids.shared_pack));
    assert_eq!(object_count(schema, ids.live_group), 1);
    assert_eq!(object_count(schema, ids.dead_group), 0);
    assert_eq!(pack_length(schema, ids.shared_pack), ids.shared_pack_length);
    // AC-17: a copied pack keeps its identity.
    for (id, pack) in &ids.delta_chain {
        assert_eq!(
            pack_of(schema, *id),
            Some(*pack),
            "pack_id of {id} is preserved"
        );
    }

    for id in &ids.commits_retained {
        assert!(
            reopened.history.commit(*id).unwrap().is_some(),
            "commit {id}"
        );
    }
    for id in &ids.commits_collected {
        assert!(
            reopened.history.commit(*id).unwrap().is_none(),
            "commit {id}"
        );
    }
    for id in &ids.layers_retained {
        assert!(reopened.history.layer(*id).unwrap().is_some(), "layer {id}");
    }
    for id in &ids.layers_collected {
        assert!(reopened.history.layer(*id).unwrap().is_none(), "layer {id}");
    }
    for id in &ids.stacks_live {
        assert!(reopened.history.layer_stack(*id).unwrap().is_some());
        assert_eq!(stack_row(schema, *id), 1);
    }
    for id in &ids.stacks_collected {
        assert!(reopened.history.layer_stack(*id).unwrap().is_none());
        assert_eq!(stack_row(schema, *id), 0);
    }
    for id in &ids.branches_live {
        assert!(reopened.history.branch(*id).unwrap().is_some());
    }
    assert_eq!(retired_branches(schema), ids.branches_retained_retired);
    for id in &ids.branches_collected {
        assert_eq!(branch_row(schema, *id), 0, "collected branch row {id}");
    }
    assert_eq!(retired_stacks(schema), 0);

    // AC-18: a digest row survives only when a retained root reaches the root.
    assert_eq!(digest_rows(schema, &ids.digest_retained), 1);
    assert_eq!(digest_rows(schema, &ids.digest_collected), 0);

    assert_doctor_clean(&reopened);
    assert_eq!(
        reopened.history.mark().unwrap().live_heads_digest(),
        mark.live_heads_digest(),
        "the live-heads digest matches before and after"
    );
    // AC-24/AC-39: the rebuild copies the epoch, never bumps it, and never
    // lowers the serial counter.
    assert_eq!(epoch(schema, ids.stacks_live[0]), epoch_before);
    assert_eq!(
        epoch_rows(schema),
        1,
        "the collected project takes its epoch"
    );
    assert!(next_serial(schema) >= serial_before);
    // AC-43/AC-02: the settled PostgreSQL Store is usable at G + 1.
    assert_phase_c_settled(schema, 0x72);
}

/// AC-28, AC-43, AC-49 (Phase C arm): the swap is durably at `G + 1` when the
/// reply is lost. A save made after the swap is not a member of the persisted
/// dead set, so the resumed cleanup keeps it, and the settled Store passes the
/// full doctor.
#[test]
fn a_save_after_the_swap_survives_the_resumed_cleanup() {
    let Some(url) = url() else { return };
    let f = Fixture::new(&url, "late");
    let live = tree(&f, whole_file(&f, b"late live content"), 1);
    let stack = initialize(&f.handles, 1, live);
    let dead = saved_whole_file(&f.handles, b"a pack Phase C collects");
    // Enough dead packs that the swapped tables are smaller than the source
    // tables, so the resumed `physicalShrink` is a nonzero carried value.
    for index in 0..64u8 {
        let _ = saved_whole_file(&f.handles, &[0x31u8.wrapping_add(index); 96]);
    }
    let before = generation(&f.handles);
    let error = f
        .handles
        .compact(&request_at(0x31, CompactFailpoint::PostgresAfterSwap))
        .unwrap_err();
    assert!(matches!(error, HistoryError::UnknownOutcome));
    let schema = &f.schema;
    assert_eq!(db_generation(schema), before + 1);
    assert_eq!(schema.count("gc_op"), 1, "the op still names Phase C");
    let phase: String = schema
        .client()
        .query_one(
            &format!(
                "SELECT phase FROM {}.gc_op WHERE id=1",
                schema.selection.schema
            ),
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(phase, "swap", "AC-51: the op records its §8 phase");
    assert!(
        schema.scalar("SELECT count(*) FROM {s}.gc_dead") > 0,
        "the persisted dead set waits for Phase C"
    );
    assert!(
        !located(&f.handles, dead),
        "the swap dropped the dead pack already"
    );
    assert!(
        schema.scalar("SELECT pid FROM {s}.gc_op WHERE id=1") > 0,
        "the op records its owner backend"
    );

    // #711: the swap persists the report state the original reply would
    // have carried; a resumed G+1 call must report it, not zeros.
    let op_dead_stored = schema.scalar("SELECT dead_stored FROM {s}.gc_op WHERE id=1");
    let op_dead_canonical = schema.scalar("SELECT dead_canonical FROM {s}.gc_op WHERE id=1");
    let op_physical_before = schema.scalar("SELECT physical_before FROM {s}.gc_op WHERE id=1");
    let op_peak_temp = schema.scalar("SELECT peak_temp FROM {s}.gc_op WHERE id=1");
    assert_eq!(
        schema.scalar("SELECT CASE WHEN kept_waste IS NULL THEN 0::bigint ELSE 1::bigint END FROM {s}.gc_op WHERE id=1"),
        1,
        "the mark's kept waste is persisted for the resumed report"
    );
    let physical_after = payload_relation_bytes(schema);

    // AC-28: a publication after the swap is not in `gc_dead`.
    let late = saved_whole_file(&f.handles, b"written between swap and cleanup");
    assert!(located(&f.handles, late));
    let late_pack = pack_of(schema, late).expect("a late pack");
    let receipt = f.handles.compact(&request(0x32)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_eq!(
        receipt.dead_bytes_bound,
        u64::try_from(op_dead_stored).unwrap(),
        "the resumed report carries the op's deadBytesBound"
    );
    assert_eq!(receipt.dead_bytes_refined, receipt.dead_bytes_bound);
    assert_eq!(
        receipt.logical_discarded,
        u64::try_from(op_dead_canonical).unwrap(),
        "the resumed report carries the op's canonical logicalDiscarded"
    );
    assert_eq!(
        receipt.physical_shrink,
        u64::try_from(op_physical_before - i64::try_from(physical_after).unwrap()).unwrap(),
        "the resumed report derives the shrink from the pre-swap relation size"
    );
    assert!(receipt.physical_shrink > 0);
    assert_eq!(
        receipt.peak_temp_bytes,
        u64::try_from(op_peak_temp).unwrap(),
        "the resumed report carries the op's peak temp"
    );
    assert!(
        receipt.kept_waste.inode_blocks_bytes > 0,
        "the resumed report carries the mark's kept waste"
    );
    assert!(
        receipt
            .zero_reasons
            .iter()
            .all(|zero| zero.field != "physicalShrink" && zero.field != "logicalDiscarded"),
        "a resumed rebuild with collected bytes answers its zeros: {:?}",
        receipt.zero_reasons
    );
    assert!(
        located(&f.handles, late),
        "the late write survives the high-water bound"
    );
    assert_eq!(pack_row(schema, late_pack), 1);
    assert!(!located(&f.handles, dead));
    assert_phase_c_settled(schema, 0x31);

    let reopened = schema.open();
    assert_doctor_clean(&reopened);
    assert!(located(&reopened, late));
    assert!(!located(&reopened, dead));
    assert!(reopened.history.layer_stack(stack.id).unwrap().is_some());
}

/// AC-21, AC-24, AC-39: the PostgreSQL `project_epoch` fence. Commits and
/// Layers leave it alone; a marker write bumps it once; a repeated marker is
/// `UpToDate` and does not bump it; compact copies it; a missing row refuses
/// every writer with `Conflict` and writes nothing.
#[test]
fn a_marker_write_bumps_the_epoch_and_the_fence_refuses_a_racing_write() {
    let Some(url) = url() else { return };
    let f = Fixture::new(&url, "epoch");
    let root = tree(&f, whole_file(&f, b"epoch genesis content"), 1);
    let stack = initialize(&f.handles, 1, root);
    let branch = support::branch_id(1);
    fork(&f.handles, stack.id, branch, 1);
    let commit_root = tree(&f, whole_file(&f, b"epoch commit content"), 2);
    let _first = commit(&f.handles, branch, commit_root, 50);
    let _layer = publish(&f.handles, stack.id, branch);
    assert_eq!(
        epoch(&f.schema, stack.id),
        0,
        "a Commit or a published Layer does not bump the epoch"
    );
    assert_eq!(epoch_rows(&f.schema), 1);

    // Capture the request while the Branch is still visible, then let the
    // removal win: the fence refuses and leaves no Commit behind.
    let candidate = whole_file(&f, b"epoch racing commit content");
    let stale = commit_request(&f.handles, branch, candidate);
    let commits_before = f.schema.count_where("SELECT count(*) FROM {s}.\"commit\"");
    f.handles
        .history
        .mark_branch_removed(branch, 1_000)
        .unwrap();
    assert_eq!(
        epoch(&f.schema, stack.id),
        1,
        "the marker bumped the epoch once"
    );
    let refused = f.handles.history.commit_branch(&stale).unwrap_err();
    assert!(
        matches!(
            refused,
            HistoryError::Missing(Missing::Branch) | HistoryError::Conflict("project epoch")
        ),
        "{refused:?}"
    );
    assert_eq!(
        f.schema.count_where("SELECT count(*) FROM {s}.\"commit\""),
        commits_before,
        "the refused write left no Commit"
    );

    // A live project's marker write bumps it again; a repeat is not a write.
    let removal = || PathRemovalRequest {
        stack: stack.id,
        path: NormalizedPath::new("file-1").unwrap().as_bytes().to_vec(),
        removed_ms: 2_000,
    };
    let outcome = f.handles.history.remove_path(&removal()).unwrap();
    assert_eq!(outcome.kind, PathRemovalKind::Removed);
    assert_eq!(epoch(&f.schema, stack.id), 2);
    let repeat = f.handles.history.remove_path(&removal()).unwrap();
    assert_eq!(repeat.kind, PathRemovalKind::UpToDate);
    assert_eq!(
        epoch(&f.schema, stack.id),
        2,
        "UpToDate is not a marker write"
    );

    // AC-24/AC-39: compaction copies the epoch and never lowers the serial.
    let serial = next_serial(&f.schema);
    let receipt = f.handles.compact(&request(0x51)).unwrap();
    assert_eq!(
        receipt.path,
        CompactPath::None,
        "a live Store takes the none path"
    );
    assert_eq!(epoch(&f.schema, stack.id), 2);
    assert_eq!(epoch_rows(&f.schema), 1);
    assert!(next_serial(&f.schema) >= serial);
    assert_settled(&f.schema, 0x51);

    // No epoch row: every writer refuses `Conflict`, never "unchanged".
    let branch_2 = support::branch_id(2);
    fork(&f.handles, stack.id, branch_2, 2);
    f.schema
        .client()
        .batch_execute(&format!(
            "DELETE FROM {}.project_epoch WHERE layer_stack_id=decode('{}','hex')",
            f.schema.selection.schema,
            hex(stack.id.as_slice())
        ))
        .unwrap();
    let candidate = whole_file(&f, b"epoch missing row content");
    let stale = commit_request(&f.handles, branch_2, candidate);
    let before = f.schema.count_where("SELECT count(*) FROM {s}.\"commit\"");
    let refused = f.handles.history.commit_branch(&stale).unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    assert_eq!(
        f.schema.count_where("SELECT count(*) FROM {s}.\"commit\""),
        before,
        "the missing fence row wrote nothing"
    );
}

/// AC-01, AC-03, AC-28, AC-43: the PostgreSQL failpoint matrix. Each named
/// §8 point aborts the round, then the next `store.compact` resumes it: `mark`
/// and `build` leave the old Store at `G` and re-mark, `swap` leaves the new
/// Store at `G + 1` and finishes Phase C, `cleanup` leaves nothing to resume.
/// Every arm settles with the same heads digest and a clean doctor.
#[test]
fn every_postgresql_failpoint_leaves_a_resumable_leftover() {
    let Some(url) = url() else { return };
    for (tag, point, op, phase, scratch) in [
        (
            "cmark",
            CompactFailpoint::PostgresAfterMark,
            0x61u8,
            Some("mark"),
            false,
        ),
        (
            "cbuild",
            CompactFailpoint::PostgresAfterBuild,
            0x62,
            Some("build"),
            true,
        ),
        (
            "cswap",
            CompactFailpoint::PostgresAfterSwap,
            0x63,
            Some("swap"),
            false,
        ),
        (
            "ccleanup",
            CompactFailpoint::PostgresAfterCleanup,
            0x64,
            None,
            false,
        ),
    ] {
        let f = Fixture::new(&url, tag);
        let live = tree(&f, whole_file(&f, b"crash matrix live content"), 1);
        let stack = initialize(&f.handles, 1, live);
        let dead = saved_whole_file(&f.handles, b"crash matrix dead content");
        let before = generation(&f.handles);
        let digest = f.handles.history.mark().unwrap().live_heads_digest();
        let error = f.handles.compact(&request_at(op, point)).unwrap_err();
        assert!(
            matches!(error, HistoryError::UnknownOutcome),
            "{tag}: {error:?}"
        );
        let swapped = matches!(
            point,
            CompactFailpoint::PostgresAfterSwap | CompactFailpoint::PostgresAfterCleanup
        );
        let expected = if swapped { before + 1 } else { before };
        assert_eq!(db_generation(&f.schema), expected, "{tag}: generation");
        assert_eq!(
            located(&f.handles, dead),
            !swapped,
            "{tag}: the dead pack moves only at the swap"
        );
        assert!(located(&f.handles, live), "{tag}: the live object stays");
        assert_eq!(
            f.schema.scratch_exists(op),
            scratch,
            "{tag}: the scratch schema state"
        );
        if let Some(phase) = phase {
            let recorded: String = f
                .schema
                .client()
                .query_one(
                    &format!(
                        "SELECT phase FROM {}.gc_op WHERE id=1",
                        f.schema.selection.schema
                    ),
                    &[],
                )
                .unwrap()
                .get(0);
            assert_eq!(recorded, phase, "{tag}: the op records its phase");
        }
        // The next call either adopts the recorded round or starts clean.
        let receipt = f.handles.compact(&request(op + 1)).unwrap();
        assert_eq!(receipt.generation_after, before + 1, "{tag}: installed");
        if point == CompactFailpoint::PostgresAfterCleanup {
            assert_eq!(
                receipt.path,
                CompactPath::None,
                "{tag}: the lost cleanup reply left a clean Store"
            );
        } else {
            assert_eq!(
                receipt.path,
                CompactPath::Rebuild,
                "{tag}: the adopted round rebuilds"
            );
        }
        assert!(!located(&f.handles, dead), "{tag}: collected");
        assert!(located(&f.handles, live), "{tag}: retained");
        assert_settled(&f.schema, op);
        let reopened = f.schema.open();
        assert_doctor_clean(&reopened);
        assert_eq!(
            reopened.history.mark().unwrap().live_heads_digest(),
            digest,
            "{tag}: the heads digest is unchanged"
        );
        assert!(reopened.history.layer_stack(stack.id).unwrap().is_some());
    }
}

/// AC-28, AC-43 (Phase C signature arm): a `content_signature` slot the mark
/// listed as dead can be rewritten by a save between the mark and Phase C. The
/// mark's high-water stamp keeps the newer row; the old unconditional
/// delete-by-slot would have removed a live signature.
#[test]
fn a_signature_slot_rewritten_after_the_mark_survives_cleanup() {
    let Some(url) = url() else { return };
    let f = Fixture::new(&url, "sigstamp");
    let live = tree(&f, whole_file(&f, b"signature live content"), 1);
    let _stack = initialize(&f.handles, 1, live);
    let dead = saved_whole_file(&f.handles, b"the mark collects this signature");
    // Enough dead packs that the rebuilt tables are smaller than the source.
    for index in 0..64u8 {
        let _ = saved_whole_file(&f.handles, &[0x41u8.wrapping_add(index); 96]);
    }
    let schema = &f.schema;
    let before = generation(&f.handles);
    let error = f
        .handles
        .compact(&request_at(0x40, CompactFailpoint::PostgresAfterSwap))
        .unwrap_err();
    assert!(matches!(error, HistoryError::UnknownOutcome));
    assert_eq!(db_generation(schema), before + 1);
    assert_eq!(schema.count("gc_op"), 1, "the op still names Phase C");

    // The mark's highest signature stamp, and the dead object's slot.
    let mark_stamp = schema.scalar("SELECT mark_stamp FROM {s}.gc_op WHERE id=1");
    let dead_bytes = dead.as_bytes().to_vec();
    let row = schema
        .client()
        .query_one(
            &format!(
                "SELECT slot FROM {}.content_signature WHERE object_id=$1",
                schema.selection.schema
            ),
            &[&dead_bytes.as_slice()],
        )
        .expect("the dead object's signature slot");
    let slot: i64 = row.get(0);

    // A save after the swap, then a re-record of the dead slot with a newer
    // stamp (both steps are what a publication between mark and Phase C does).
    let late = saved_whole_file(&f.handles, b"signature written after the swap");
    let late_row = schema
        .client()
        .query_one(
            &format!(
                "SELECT signature FROM {}.content_signature WHERE object_id=$1",
                schema.selection.schema
            ),
            &[&late.as_bytes().as_slice()],
        )
        .expect("the late object's signature");
    let signature: Vec<u8> = late_row.get(0);
    let first = slot + 1;
    let base = mark_stamp + 1;
    let new_stamp = base + (first - base).rem_euclid(8192);
    assert!(new_stamp > mark_stamp);
    schema
        .client()
        .execute(
            &format!(
                "UPDATE {}.content_signature SET object_id=$1,stamp=$2,signature=$3 WHERE slot=$4",
                schema.selection.schema
            ),
            &[
                &late.as_bytes().as_slice(),
                &new_stamp,
                &signature.as_slice(),
                &slot,
            ],
        )
        .expect("rewrite the slot after the mark");

    let receipt = f.handles.compact(&request(0x41)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_eq!(
        schema.scalar(&format!(
            "SELECT count(*) FROM {{s}}.content_signature WHERE slot={slot} AND object_id=decode('{}','hex')",
            hex(late.as_bytes())
        )),
        1,
        "the post-mark slot update survives the high-water stamp"
    );
    assert_settled(schema, 0x40);
    let reopened = schema.open();
    assert_doctor_clean(&reopened);
    assert!(reopened.history.layer_stack(_stack.id).unwrap().is_some());
}

/// AC-18, AC-28, AC-43 (Phase C digest arm): a post-mark re-record updates the
/// same `(digest, length)` row to a live root. The dead set names the root the
/// mark saw, so the newer row is not deleted by its digest.
#[test]
fn a_digest_root_updated_after_the_mark_survives_cleanup() {
    let Some(url) = url() else { return };
    let f = Fixture::new(&url, "digestroot");
    let retained_payload = pseudo_bytes(131_072, 0x77);
    let retained = chunked_file(&f, &retained_payload);
    let live = tree(&f, retained, 1);
    let _stack = initialize(&f.handles, 1, live);
    let dead_payload = pseudo_bytes(131_072, 0x88);
    let dead = chunked_file(&f, &dead_payload);
    let dead_digest = *blake3::hash(&dead_payload).as_bytes();
    let retained_digest = *blake3::hash(&retained_payload).as_bytes();
    f.handles
        .history
        .record_file_digests(&[
            FileDigest {
                root: *dead.as_bytes(),
                length: dead_payload.len() as u64,
                digest: dead_digest,
            },
            FileDigest {
                root: *retained.as_bytes(),
                length: retained_payload.len() as u64,
                digest: retained_digest,
            },
        ])
        .unwrap();
    for index in 0..64u8 {
        let _ = saved_whole_file(&f.handles, &[0x51u8.wrapping_add(index); 96]);
    }
    let schema = &f.schema;
    let before = generation(&f.handles);
    let error = f
        .handles
        .compact(&request_at(0x42, CompactFailpoint::PostgresAfterSwap))
        .unwrap_err();
    assert!(matches!(error, HistoryError::UnknownOutcome));
    assert_eq!(db_generation(schema), before + 1);
    assert_eq!(schema.count("gc_op"), 1, "the op still names Phase C");
    assert_eq!(
        schema.scalar(&format!(
            "SELECT count(*) FROM {{s}}.gc_dead WHERE kind='digest' AND id=decode('{}','hex')",
            hex(dead.as_bytes())
        )),
        1,
        "the dead set names the dead root"
    );

    // A re-record after the mark moves the same (digest,length) row to a live
    // root. (The hint is deliberately inconsistent for the test; only the
    // cleanup's membership key is under test.)
    schema
        .client()
        .execute(
            &format!(
                "UPDATE {}.file_digest SET root=$1 WHERE digest=$2 AND length=$3",
                schema.selection.schema
            ),
            &[
                &retained.as_bytes().as_slice(),
                &dead_digest.as_slice(),
                &i64::try_from(dead_payload.len()).unwrap(),
            ],
        )
        .expect("move the digest root after the mark");

    let receipt = f.handles.compact(&request(0x43)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_eq!(
        digest_rows(schema, &dead_digest),
        1,
        "the post-mark root update survives Phase C"
    );
    let root: Vec<u8> = schema
        .client()
        .query_one(
            &format!(
                "SELECT root FROM {}.file_digest WHERE digest=$1 AND length=$2",
                schema.selection.schema
            ),
            &[
                &dead_digest.as_slice(),
                &i64::try_from(dead_payload.len()).unwrap(),
            ],
        )
        .expect("the surviving digest row")
        .get(0);
    assert_eq!(root, retained.as_bytes().to_vec());
    assert_settled(schema, 0x42);
}
