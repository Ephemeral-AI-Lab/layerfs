//! #41 T12.1: SQLite `store.compact` fixtures and the golden receipt.
//!
//! One transcript pins the §11 report for all three outcomes: an empty Store
//! (`none`/`empty-store`), a Store with no wholly dead pack (`none`/`no-waste`)
//! and a Store with retained history plus known dead objects, packs and delta
//! bases (`rebuild`). The fixture is built only through the public APIs; the
//! golden JSON is compared byte for byte, so a change to any reported value is
//! deliberate. The same file pins the §6.4 live-heads digest vector (fixed ids,
//! a retired Branch and stack, an absent head).
//!
//! Covered matrix rows (SQLite half where the row is cross-backend):
//! AC-04 (deterministic arm), AC-05, AC-17, AC-18, AC-25 (Branch/stack markers),
//! AC-38 (successful settle), AC-40 (SQLite half), AC-47 (`none`/success),
//! AC-48 (empty Store and rebuild arms), AC-52, AC-56 (SQLite half), AC-58.
//!
//! Update the golden deliberately with `UPDATE_GOLDEN=1 cargo test -p
//! layerfs-persistence --test sqlite_compact_fixtures`; the second run checks
//! the new file.
#![cfg(any(target_os = "macos", target_os = "linux"))]
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
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitId, CommitMeta, CommitOutcome, CommitRequest,
    CommitStatistics, CommitVia, FileDigest, ForkRequest, ForkSource, HistoryCatalog, LayerId,
    LayerRecord, LayerStackId, LayerStackRecord, StackInitialization,
};
use layerfs_persistence::{
    diagnostic::{Observation, Options, Outcome},
    CompactPath, CompactReceipt, CompactRequest, PersistenceConfig, SqliteHandles,
    SqlitePersistenceProfile,
};
use layerfs_storage::{
    encoding::{encode_full, encode_prefix, raw_payload, CompressionWorkspace},
    location::{ObjectLocation, PackDomain},
    pack::{assemble, build_group, layout, PackLane},
    port::{PackPersistence, Publication, PublishedPack, Reserve},
    save::SaveProfile,
    Storage, StorageCapacities,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use support::doctor::Fixture;
use support::Temp;

const BINDING: &[u8] = b"doctor-adversarial";
const CURSOR: [u8; 32] = [71; 32];
const SCOPE_SEED: [u8; 32] = [11; 32];
const GOLDEN: &str = include_str!("fixtures/sqlite_compact_golden.json");

// A deterministic pseudo-random payload: the dead packs must not compress away
// to nothing, and the same bytes must produce the same receipt on every run.
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

fn request(op: u8) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    }
}

fn reopen(path: &Path) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        BINDING,
        CURSOR,
    )
    .unwrap()
}

fn store_path(temp: &Temp) -> PathBuf {
    temp.join("store.sqlite")
}

fn sidecar(store: &Path, suffix: &str) -> PathBuf {
    let mut name = store.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn generation(handles: &SqliteHandles) -> u64 {
    handles.history.incarnation()
}

// ---- fixture objects -------------------------------------------------------

fn whole_file(f: &Fixture, bytes: &[u8]) -> ObjectId {
    f.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).unwrap(),
    )
    .id()
}

/// One chunked file's content root (a FileState): the representation a file
/// at or above the small-file threshold takes, and the only kind whose
/// file_digest row the Store records.
fn chunked_file(f: &Fixture, bytes: &[u8]) -> ObjectId {
    let mut extents = Vec::new();
    for part in bytes.chunks(32_768) {
        let chunk = f
            .put(
                ObjectRole::Chunk,
                mapping::encode_chunk_object(part).unwrap(),
            )
            .id();
        extents.push(ExtentSlice::new(chunk, 0, part.len() as u32).unwrap());
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
            .unwrap(),
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
        .unwrap(),
    )
    .id()
}

/// One valid filesystem tree: a root directory whose only entry is a regular
/// file whose content root is `content`. The inode leaf is pooled, so the
/// fixture also owns `inode_value_block` rows; a rebuild keeps the blocks
/// its kept trees read.
fn tree(f: &Fixture, content: ObjectId, seed: u8) -> ObjectId {
    let directory = f
        .put(
            ObjectRole::DirectoryLeaf,
            encode_directory_page(&DirectoryPage::Leaf {
                entries: vec![(PathName::new(&format!("file-{seed}")).unwrap(), 2)],
            })
            .unwrap(),
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
        .unwrap(),
        values,
    );
    f.put(
        ObjectRole::FilesystemRoot,
        FilesystemRoot::new(profile_id(), scope_for_seed(SCOPE_SEED), 1, inode)
            .unwrap()
            .encode()
            .unwrap(),
    )
    .id()
}

/// Publishes one pack holding each object as its own group, so a retained pack
/// can hold one reached and one unreached record.
fn publish_groups(f: &Fixture, objects: &[FinalizedObject]) -> (i64, Vec<u8>) {
    let capacities = StorageCapacities::from_policy(f.policy).unwrap();
    let mut workspace = CompressionWorkspace::new().unwrap();
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
            .unwrap();
            build_group(PackLane::WholeFile, &[record.record], None).unwrap()
        })
        .collect();
    let body = assemble(PackLane::WholeFile, &groups).unwrap();
    let reserved = f
        .handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
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
        .unwrap();
    (reserved.first_pack_id, body)
}

/// Publishes `target` as a PREFIX record over `base`, asserting the encoder
/// kept the delta (a full record would hide the base from the mark).
fn prefix_record(f: &Fixture, target: &FinalizedObject, base: &FinalizedObject, base_id: ObjectId) {
    let capacities = StorageCapacities::from_policy(f.policy).unwrap();
    let record = encode_prefix(
        target.canonical(),
        target.role(),
        base_id,
        raw_payload(base.canonical(), base.role()).unwrap(),
        &capacities,
        &mut CompressionWorkspace::new().unwrap(),
        &mut SaveProfile::default(),
    )
    .unwrap();
    assert_eq!(
        record.base,
        Some(base_id),
        "the fixture needs a PREFIX record, not a full one"
    );
    f.record(target, record.lane, record.record, Vec::new());
}

// ---- fixture history ------------------------------------------------------

fn initialize(handles: &SqliteHandles, tag: u8, genesis: ObjectId) -> LayerStackRecord {
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
        .unwrap()
}

fn fork(handles: &SqliteHandles, stack: LayerStackId, branch: BranchId, tag: u8) {
    let record = handles.history.layer_stack(stack).unwrap().unwrap();
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: support::name(&format!("branch-{tag}")),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
}

fn commit(
    handles: &SqliteHandles,
    branch: BranchId,
    candidate_root: ObjectId,
    logical: u64,
) -> CommitId {
    let snapshot = handles.history.branch(branch).unwrap().unwrap();
    let expected_root = match snapshot.head_commit {
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
            expected_head: snapshot.head_commit,
            expected_base: snapshot.base_layer,
            expected_root,
            candidate_root,
            profile: profile_id(),
            scope: scope_for_seed(SCOPE_SEED).object(),
            second_parent: None,
            meta,
            force: false,
            required_head: None,
        })
        .unwrap()
    {
        CommitOutcome::Committed { record, .. } => record.id,
        other => panic!("commit: {other:?}"),
    }
}

fn publish(handles: &SqliteHandles, stack: LayerStackId, branch: BranchId) -> LayerRecord {
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
        .unwrap()
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

fn totals(path: &Path) -> Totals {
    let connection = rusqlite::Connection::open(path).unwrap();
    let (logical, canonical, stored, metadata): (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT logical,canonical,stored,metadata FROM store_totals WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    Totals {
        logical: logical as u64,
        canonical: canonical as u64,
        stored: stored as u64,
        metadata: metadata as u64,
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

/// The Store file's reusable backend bytes: `freelist_count * page_size`.
fn free_page_bytes(path: &Path) -> u64 {
    let connection = rusqlite::Connection::open(path).unwrap();
    let free: i64 = connection
        .query_row("PRAGMA freelist_count", [], |row| row.get(0))
        .unwrap();
    let page: i64 = connection
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .unwrap();
    u64::try_from(free.saturating_mul(page)).unwrap_or(0)
}

fn pack_facts(path: &Path, live_packs: &[i64]) -> PackFacts {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut statement = connection
        .prepare("SELECT pack_id, length FROM pack ORDER BY pack_id")
        .unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
        .unwrap();
    let mut facts = PackFacts {
        total: 0,
        live_bytes: 0,
        dead_bytes: 0,
        live: Vec::new(),
        dead: Vec::new(),
    };
    for row in rows {
        let (pack, length) = row.unwrap();
        facts.total += length as u64;
        if live_packs.contains(&pack) {
            facts.live.push(pack);
            facts.live_bytes += length as u64;
        } else {
            facts.dead.push(pack);
        }
    }
    facts.dead_bytes = facts.total - facts.live_bytes;
    facts
}

fn scalar(path: &Path, sql: &str, param: impl rusqlite::ToSql) -> i64 {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .query_row(sql, [param], |row| row.get(0))
        .unwrap()
}

fn scalar0(path: &Path, sql: &str) -> i64 {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection.query_row(sql, [], |row| row.get(0)).unwrap()
}

/// The Store file's `(size, mtime, database change counter)`: the tuple the
/// install fence seals, so an equal tuple across a call means no write was
/// committed to the file.
fn store_file_tuple(path: &Path) -> (u64, std::time::SystemTime, u32) {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let metadata = std::fs::metadata(path).unwrap();
    let modified = metadata.modified().unwrap();
    let mut file = std::fs::File::open(path).unwrap();
    let mut header = [0u8; 4];
    file.seek(SeekFrom::Start(24)).unwrap();
    file.read_exact(&mut header).unwrap();
    (metadata.len(), modified, u32::from_be_bytes(header))
}

fn pack_of(path: &Path, id: ObjectId) -> Option<i64> {
    let connection = rusqlite::Connection::open(path).unwrap();
    match connection.query_row(
        "SELECT pack_id FROM object_location WHERE object_id=?1",
        [id.as_bytes().as_slice()],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(pack) => Some(pack),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => panic!("object location: {error}"),
    }
}

fn object_count(path: &Path, id: ObjectId) -> i64 {
    scalar(
        path,
        "SELECT count(*) FROM object_location WHERE object_id=?1",
        id.as_bytes().as_slice(),
    )
}

fn pack_length(path: &Path, pack: i64) -> u64 {
    scalar(path, "SELECT length FROM pack WHERE pack_id=?1", pack) as u64
}

fn pack_row(path: &Path, pack: i64) -> i64 {
    scalar(path, "SELECT count(*) FROM pack WHERE pack_id=?1", pack)
}

fn branch_row(path: &Path, id: BranchId) -> i64 {
    scalar(
        path,
        "SELECT count(*) FROM branch WHERE branch_id=?1",
        id.as_slice(),
    )
}

fn stack_row(path: &Path, id: LayerStackId) -> i64 {
    scalar(
        path,
        "SELECT count(*) FROM layer_stack WHERE layer_stack_id=?1",
        id.as_slice(),
    )
}

fn retired_branches(path: &Path) -> Vec<BranchId> {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut statement = connection
        .prepare("SELECT branch_id FROM branch_retired ORDER BY branch_id")
        .unwrap();
    let rows = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .unwrap();
    rows.map(|row| BranchId::from_slice(&row.unwrap()).unwrap())
        .collect()
}

fn retired_stacks(path: &Path) -> i64 {
    scalar0(path, "SELECT count(*) FROM layer_stack_retired")
}

fn digest_rows(path: &Path, digest: &[u8; 32]) -> i64 {
    scalar(
        path,
        "SELECT count(*) FROM file_digest WHERE digest=?1",
        digest.as_slice(),
    )
}

fn inode_block_bytes(path: &Path) -> u64 {
    scalar0(
        path,
        "SELECT COALESCE(SUM(length(body)),0) FROM inode_value_block",
    ) as u64
}

fn sum_pack_lengths(path: &Path) -> u64 {
    scalar0(path, "SELECT COALESCE(SUM(length),0) FROM pack") as u64
}

fn sum_canonical(path: &Path) -> u64 {
    scalar0(
        path,
        "SELECT COALESCE(SUM(canonical_length),0) FROM object_location",
    ) as u64
}

fn sum_metadata_packs(path: &Path) -> u64 {
    scalar0(
        path,
        "SELECT COALESCE(SUM(length),0) FROM pack WHERE domain=0",
    ) as u64
}

fn sum_history_logical(path: &Path) -> u64 {
    (scalar0(path, "SELECT COALESCE(SUM(logical),0) FROM [commit]")
        + scalar0(path, "SELECT COALESCE(SUM(logical),0) FROM layer")) as u64
}

fn located(handles: &SqliteHandles, id: ObjectId) -> bool {
    let mut found = Vec::new();
    handles.storage.locate(&[id], &mut found).unwrap();
    !found.is_empty()
}

fn audit(handles: &SqliteHandles) -> Vec<Observation> {
    let mut out = Vec::new();
    handles
        .diagnose(
            Options {
                quick: false,
                ..Options::default()
            },
            &mut |observation| out.push(observation),
        )
        .unwrap();
    out
}

fn assert_doctor_clean(handles: &SqliteHandles) {
    let out = audit(handles);
    assert!(
        !out.iter()
            .any(|observation| matches!(observation.outcome, Outcome::Fail | Outcome::Unknown)),
        "the compacted Store must pass the full doctor: {out:?}"
    );
}

fn assert_no_artifacts(store: &Path) {
    for suffix in [
        ".compact",
        ".compact-journal",
        ".compact-wal",
        ".compact-shm",
        ".compact-visited",
        ".compact-visited-journal",
        ".compact-op",
        ".compact-op.tmp",
    ] {
        assert!(
            !sidecar(store, suffix).exists(),
            "compaction artifact {suffix} remains"
        );
    }
    let name = store.file_name().unwrap().to_string_lossy().to_string();
    for entry in std::fs::read_dir(store.parent().unwrap()).unwrap() {
        let entry = entry.unwrap();
        let file = entry.file_name().to_string_lossy().to_string();
        if file == name {
            continue;
        }
        let rest = file.strip_prefix(&name).unwrap_or(&file);
        assert!(
            matches!(rest, "-wal" | "-shm" | "-journal"),
            "unexpected file beside the Store: {file}"
        );
    }
}

// ---- JSON -----------------------------------------------------------------

enum Json {
    Arr(Vec<Json>),
    Bool(bool),
    Null,
    Num(u64),
    Obj(Vec<(&'static str, Json)>),
    Str(String),
}

impl Json {
    fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Null => out.push_str("null"),
            Self::Num(value) => out.push_str(&value.to_string()),
            Self::Str(value) => {
                out.push('"');
                for character in value.chars() {
                    match character {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        character if character.is_control() => {
                            out.push_str(&format!("\\u{:04x}", character as u32));
                        }
                        character => out.push(character),
                    }
                }
                out.push('"');
            }
            Self::Arr(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push_str("[\n");
                for (index, item) in items.iter().enumerate() {
                    pad(out, depth + 1);
                    item.write(out, depth + 1);
                    if index + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, depth);
                out.push(']');
            }
            Self::Obj(fields) => {
                if fields.is_empty() {
                    out.push_str("{}");
                    return;
                }
                let mut sorted: Vec<_> = fields.iter().collect();
                sorted.sort_by(|left, right| left.0.cmp(right.0));
                out.push_str("{\n");
                for (index, (key, value)) in sorted.iter().enumerate() {
                    pad(out, depth + 1);
                    out.push('"');
                    out.push_str(key);
                    out.push_str("\": ");
                    value.write(out, depth + 1);
                    if index + 1 < sorted.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, depth);
                out.push('}');
            }
        }
    }
}

fn pad(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
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

fn strings(items: impl IntoIterator<Item = String>) -> Json {
    Json::Arr(items.into_iter().map(Json::Str).collect())
}

fn object_ids(ids: impl IntoIterator<Item = ObjectId>) -> Json {
    strings(ids.into_iter().map(|id| hex(id.as_bytes())))
}

fn byte_ids(ids: impl IntoIterator<Item = [u8; 32]>) -> Json {
    strings(ids.into_iter().map(|id| hex(&id)))
}

fn branch_ids(ids: impl IntoIterator<Item = BranchId>) -> Json {
    strings(ids.into_iter().map(|id| hex(id.as_slice())))
}

fn commit_ids(ids: impl IntoIterator<Item = CommitId>) -> Json {
    strings(ids.into_iter().map(|id| hex(id.as_slice())))
}

fn layer_ids(ids: impl IntoIterator<Item = LayerId>) -> Json {
    strings(ids.into_iter().map(|id| hex(id.as_slice())))
}

fn stack_ids(ids: impl IntoIterator<Item = LayerStackId>) -> Json {
    strings(ids.into_iter().map(|id| hex(id.as_slice())))
}

fn numbers(items: impl IntoIterator<Item = i64>) -> Json {
    Json::Arr(
        items
            .into_iter()
            .map(|item| Json::Num(item as u64))
            .collect(),
    )
}

fn totals_json(total: Totals) -> Json {
    Json::Obj(vec![
        ("canonical", Json::Num(total.canonical)),
        ("logical", Json::Num(total.logical)),
        ("metadata", Json::Num(total.metadata)),
        ("stored", Json::Num(total.stored)),
    ])
}

fn receipt_json(receipt: &CompactReceipt) -> Json {
    Json::Obj(vec![
        ("backendReusable", Json::Num(receipt.backend_reusable)),
        ("deadBytesBound", Json::Num(receipt.dead_bytes_bound)),
        ("deadBytesRefined", Json::Num(receipt.dead_bytes_refined)),
        ("estimate", Json::Bool(receipt.estimate)),
        ("generationAfter", Json::Num(receipt.generation_after)),
        ("generationBefore", Json::Num(receipt.generation_before)),
        (
            "keptWaste",
            Json::Obj(vec![
                (
                    "deadInRetainedPacksBytes",
                    Json::Num(receipt.kept_waste.dead_in_retained_packs_bytes),
                ),
                (
                    "deltaBases",
                    Json::Obj(vec![
                        ("bytes", Json::Num(receipt.kept_waste.delta_bases_bytes)),
                        (
                            "references",
                            byte_ids(receipt.kept_waste.delta_base_references.iter().copied()),
                        ),
                    ]),
                ),
                (
                    "inodeBlocksBytes",
                    Json::Num(receipt.kept_waste.inode_blocks_bytes),
                ),
            ]),
        ),
        ("logicalDiscarded", Json::Num(receipt.logical_discarded)),
        ("path", Json::Str(receipt.path.as_str().to_owned())),
        ("peakTempBytes", Json::Num(receipt.peak_temp_bytes)),
        ("physicalShrink", Json::Num(receipt.physical_shrink)),
        (
            "residue",
            Json::Obj(vec![
                (
                    "artifacts",
                    strings(receipt.residue.artifacts.iter().cloned()),
                ),
                ("bytes", Json::Num(receipt.residue.bytes)),
                (
                    "reason",
                    match &receipt.residue.reason {
                        Some(reason) => Json::Str(reason.clone()),
                        None => Json::Null,
                    },
                ),
            ]),
        ),
        (
            "zeroReasons",
            Json::Arr(
                receipt
                    .zero_reasons
                    .iter()
                    .map(|zero| {
                        Json::Obj(vec![
                            ("field", Json::Str(zero.field.to_owned())),
                            ("reason", Json::Str(zero.reason.to_owned())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn mark_json(digest: [u8; 32], objects: usize, packs: &[i64]) -> Json {
    Json::Obj(vec![
        ("liveHeadsDigest", Json::Str(hex(&digest))),
        ("objectCount", Json::Num(objects as u64)),
        ("packs", numbers(packs.iter().copied())),
    ])
}

fn check_golden(actual: &Json) {
    let rendered = actual.render();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sqlite_compact_golden.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &rendered).unwrap();
        eprintln!("updated golden transcript {}", path.display());
        return;
    }
    assert_eq!(
        rendered, GOLDEN,
        "the SQLite compaction golden transcript changed. Review every value; if \
         the change is deliberate, rerun with UPDATE_GOLDEN=1."
    );
}

/// The §6.4 live-heads vector, written out independently of the mark: the
/// fixed prefix, the non-retired stacks in identity order as `(17, 33)`, then
/// the marker-free Branches in identity order as `(17, 33-or-33-zero, 33)`.
fn expected_heads_digest(
    stacks: &[(LayerStackId, LayerId)],
    branches: &[(BranchId, Option<CommitId>, LayerId)],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/live-heads/v1\0");
    let mut stacks = stacks.to_vec();
    stacks.sort_by(|left, right| left.0.as_slice().cmp(right.0.as_slice()));
    for (stack, head) in stacks {
        assert_eq!(stack.as_slice().len(), 17, "stack id width");
        assert_eq!(head.as_slice().len(), 33, "layer id width");
        hasher.update(stack.as_slice());
        hasher.update(head.as_slice());
    }
    let mut branches = branches.to_vec();
    branches.sort_by(|left, right| left.0.as_slice().cmp(right.0.as_slice()));
    for (branch, commit, base) in branches {
        assert_eq!(branch.as_slice().len(), 17, "branch id width");
        assert_eq!(base.as_slice().len(), 33, "base layer id width");
        hasher.update(branch.as_slice());
        match commit {
            Some(commit) => {
                assert_eq!(commit.as_slice().len(), 33, "commit id width");
                hasher.update(commit.as_slice());
            }
            None => {
                hasher.update(&[0u8; 33]);
            }
        }
        hasher.update(base.as_slice());
    }
    *hasher.finalize().as_bytes()
}

// ---- cases ----------------------------------------------------------------

fn empty_none_case() -> Json {
    let f = Fixture::new();
    let Fixture { temp, handles, .. } = f;
    let path = store_path(&temp);
    let before = totals(&path);
    assert_eq!(
        (
            before.logical,
            before.canonical,
            before.stored,
            before.metadata
        ),
        (0, 0, 0, 0)
    );
    let generation_before = generation(&handles);
    // Close the seeding session so its WAL is checkpointed: the tuple below is
    // then the quiesced file the `none` call must leave untouched.
    drop(handles);
    let handles = reopen(&path);
    let file_before = store_file_tuple(&path);
    let receipt = handles.compact(&request(0x70)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert_eq!(receipt.generation_before, generation_before);
    assert_eq!(receipt.generation_after, generation_before);
    assert_eq!(receipt.dead_bytes_bound, 0);
    assert_eq!(receipt.dead_bytes_refined, 0);
    assert_eq!(receipt.logical_discarded, 0);
    assert_eq!(receipt.physical_shrink, 0);
    assert_eq!(
        receipt.peak_temp_bytes, 0,
        "the none path writes no visited scratch (§6.3)"
    );
    assert!(receipt.estimate);
    assert!(receipt.kept_waste.is_empty());
    assert_eq!(receipt.residue.bytes, 0);
    assert!(receipt.residue.artifacts.is_empty());
    assert!(receipt.residue.reason.is_none());
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "deadBytesBound" && zero.reason == "empty-store"));
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "deadBytesRefined" && zero.reason == "empty-store"));
    assert_no_artifacts(&path);
    assert_eq!(
        scalar0(&path, "SELECT count(*) FROM kept_node"),
        0,
        "the none path leaves no mark state"
    );
    assert_eq!(
        scalar0(&path, "SELECT count(*) FROM gc_intent"),
        0,
        "the rolled-back mark leaves no admission intent"
    );
    assert_eq!(
        store_file_tuple(&path),
        file_before,
        "the none path commits no write: the Store file is unchanged"
    );
    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), generation_before);
    assert_doctor_clean(&reopened);
    let digest = reopened.history.mark().unwrap().live_heads_digest();
    let after = totals(&path);
    Json::Obj(vec![
        ("mark", mark_json(digest, 0, &[])),
        ("receipt", receipt_json(&receipt)),
        ("totalsAfter", totals_json(after)),
        ("totalsBefore", totals_json(before)),
    ])
}

fn no_dead_none_case() -> Json {
    let f = Fixture::new();
    let genesis_content = whole_file(&f, b"the genesis tree content");
    let genesis = tree(&f, genesis_content, 7);
    let stack = initialize(&f.handles, 1, genesis);
    let branch = support::branch_id(1);
    fork(&f.handles, stack.id, branch, 1);
    let commit_content = whole_file(&f, b"a retained commit tree content");
    let commit_root = tree(&f, commit_content, 8);
    let commit_id = commit(&f.handles, branch, commit_root, 200);
    let layer = publish(&f.handles, stack.id, branch);

    let Fixture { temp, handles, .. } = f;
    let path = store_path(&temp);
    let before = totals(&path);
    let generation_before = generation(&handles);
    let mark = handles.history.mark().unwrap();
    let mark_rows = i64::try_from(mark.history().len()).unwrap();
    assert_eq!(
        mark.live_heads_digest(),
        handles.history.mark().unwrap().live_heads_digest(),
        "the digest is stable across marks"
    );
    assert!(mark.objects().waste().inode_blocks_bytes() > 0);
    let facts = pack_facts(&path, mark.objects().packs());
    assert_eq!(facts.dead_bytes, 0, "no wholly dead pack exists");
    let digest = mark.live_heads_digest();
    let live_packs = mark.objects().packs().to_vec();
    // Quiesce the test's own mark write, so the tuple below is the file the
    // `none` call must leave untouched.
    drop(handles);
    let handles = reopen(&path);
    let file_before = store_file_tuple(&path);
    let receipt = handles.compact(&request(0x71)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert_eq!(receipt.generation_before, generation_before);
    assert_eq!(receipt.generation_after, generation_before);
    assert_eq!(receipt.dead_bytes_bound, 0);
    assert_eq!(receipt.dead_bytes_refined, receipt.dead_bytes_bound);
    assert_eq!(
        receipt.logical_discarded, 0,
        "the none path collects nothing"
    );
    assert_eq!(receipt.physical_shrink, 0);
    assert_eq!(
        receipt.peak_temp_bytes, 0,
        "the none path writes no visited scratch (§6.3)"
    );
    assert!(receipt.estimate);
    assert!(receipt.residue.artifacts.is_empty());
    assert_eq!(receipt.residue.bytes, 0);
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "deadBytesBound" && zero.reason == "no-waste"));
    assert_no_artifacts(&path);
    assert_eq!(
        scalar0(&path, "SELECT count(*) FROM kept_node"),
        mark_rows,
        "the rolled-back mark restores the pre-call kept_node closure untouched"
    );
    assert_eq!(
        scalar0(&path, "SELECT count(*) FROM gc_intent"),
        0,
        "the rolled-back mark leaves no admission intent"
    );
    assert_eq!(
        store_file_tuple(&path),
        file_before,
        "the none path commits no write: the Store file is unchanged"
    );

    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), generation_before);
    assert_doctor_clean(&reopened);
    assert!(reopened.history.commit(commit_id).unwrap().is_some());
    assert!(reopened.history.layer(layer.id).unwrap().is_some());
    assert_eq!(
        reopened.history.mark().unwrap().live_heads_digest(),
        digest,
        "the none path leaves the heads unchanged"
    );
    let after = totals(&path);
    Json::Obj(vec![
        ("commit", commit_ids([commit_id])),
        ("layer", layer_ids([layer.id])),
        ("mark", mark_json(digest, mark.objects().len(), &live_packs)),
        ("receipt", receipt_json(&receipt)),
        ("totalsAfter", totals_json(after)),
        ("totalsBefore", totals_json(before)),
    ])
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

fn rebuild_fixture() -> (Temp, SqliteHandles, FixtureIds) {
    let f = Fixture::new();

    // The dead packs: unreferenced whole files, each in its own pack.
    let mut objects_dead = Vec::new();
    for index in 0..12u8 {
        let payload = pseudo_bytes(4096, 0xA0 + index);
        objects_dead.push(whole_file(&f, &payload));
    }
    // AC-18 needs chunked file roots: the digest floor is the small-file
    // threshold, so a WholeFile root is below it and records no row.
    let retained_chunked_payload = pseudo_bytes(131_072, 0x77);
    let retained_chunked = chunked_file(&f, &retained_chunked_payload);
    let collected_chunked_payload = pseudo_bytes(131_072, 0x88);
    let collected_chunked = chunked_file(&f, &collected_chunked_payload);
    let digest_collected = *blake3::hash(&collected_chunked_payload).as_bytes();
    let digest_retained = *blake3::hash(&retained_chunked_payload).as_bytes();

    // A depth-2 delta chain: base2 <- base1 <- target. Only the target is a
    // retained root, so both bases are kept only as delta bases.
    let base2_payload = pseudo_bytes(512, 0x11);
    let base1_payload = [base2_payload.as_slice(), b" v2"].concat();
    let target_payload = [base1_payload.as_slice(), b" v3"].concat();
    let base2 = whole_file(&f, &base2_payload);
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
    prefix_record(&f, &base1_object, &base2_object, base2);
    let base1 = base1_object.id();
    let target_object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&target_payload).unwrap(),
    )
    .unwrap();
    prefix_record(&f, &target_object, &base1_object, base1);
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
    let (shared_pack, shared_body) = publish_groups(&f, &[live_group.clone(), dead_group.clone()]);
    // The shared pack's live object is the genesis tree's file content.
    let shared_header = layout::parse_header(&shared_body).unwrap();
    let dead_view = layout::group_view(&shared_body, shared_header, 1).unwrap();
    let shared_dead_group = (dead_view.end - dead_view.start) as u64;
    let shared_pack_length = pack_length(&store_path(&f.temp), shared_pack);

    // Content objects of the trees that are expected to be collected.
    let collected_content_a4 = collected_chunked;
    let collected_content_b0 = whole_file(&f, b"deleted project genesis content");
    let collected_content_b1 = whole_file(&f, b"deleted project commit content");
    let content_c3 = whole_file(&f, b"live branch commit content");

    let root_a0 = tree(&f, live_group.id(), 0);
    let root_a1 = tree(&f, delta_target, 1);
    let root_a2 = tree(&f, retained_chunked, 2);
    let root_a3 = tree(&f, content_c3, 3);
    let root_a4 = tree(&f, collected_content_a4, 4);
    let root_b0 = tree(&f, collected_content_b0, 5);
    let root_b1 = tree(&f, collected_content_b1, 6);

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

    let path = store_path(&f.temp);
    let delta_chain: Vec<(ObjectId, i64)> = [base2, base1, delta_target]
        .iter()
        .map(|id| (*id, pack_of(&path, *id).expect("a delta-chain pack")))
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
    let ids = FixtureIds {
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
    };
    let Fixture { temp, handles, .. } = f;
    (temp, handles, ids)
}

fn rebuild_case() -> Json {
    let (temp, handles, ids) = rebuild_fixture();
    let path = store_path(&temp);
    let before = totals(&path);
    let generation_before = generation(&handles);

    let mark = handles.history.mark().unwrap();
    assert_eq!(
        mark.live_heads_digest(),
        handles.history.mark().unwrap().live_heads_digest(),
        "the digest is stable across marks"
    );
    let expected_digest = expected_heads_digest(
        &[(ids.stacks_live[0], *ids.layers_retained.last().unwrap())],
        &[(
            ids.branches_live[0],
            Some(ids.commits_retained[2]),
            *ids.layers_retained.last().unwrap(),
        )],
    );
    assert_eq!(
        mark.live_heads_digest(),
        expected_digest,
        "the mark hashes the §6.4 vector"
    );
    let live_packs = mark.objects().packs().to_vec();
    let facts = pack_facts(&path, &live_packs);
    let inode_blocks = inode_block_bytes(&path);
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
        assert!(
            !mark.objects().contains(*id),
            "dead object {id} is collected"
        );
    }
    assert_eq!(facts.dead_bytes, facts.total - facts.live_bytes);
    assert!(
        facts.dead_bytes * 10 >= facts.total,
        "the fixture must take the rebuild path: {} dead of {}",
        facts.dead_bytes,
        facts.total
    );

    let reusable_before = free_page_bytes(&path);
    let receipt = handles.compact(&request(0x72)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, generation_before);
    assert_eq!(receipt.generation_after, generation_before + 1);
    assert_eq!(receipt.dead_bytes_bound, facts.dead_bytes);
    assert_eq!(receipt.dead_bytes_refined, receipt.dead_bytes_bound);
    assert_eq!(
        receipt.backend_reusable, reusable_before,
        "backendReusable is the source Store's reusable free pages, not the \
         freshly vacuumed target's empty freelist"
    );
    assert!(receipt.physical_shrink > 0);
    assert!(receipt.peak_temp_bytes > 0);
    assert!(receipt.estimate);
    assert_eq!(
        receipt.kept_waste.dead_in_retained_packs_bytes, 0,
        "the rebuild writes the shared pack again without its unreached group"
    );
    let inode_blocks_after = inode_block_bytes(&path);
    assert_eq!(
        receipt.kept_waste.inode_blocks_bytes, inode_blocks_after,
        "the report names the inode value blocks the rebuilt Store holds"
    );
    assert!(
        inode_blocks_after > 0 && inode_blocks_after < inode_blocks,
        "the blocks only the collected trees read are left behind"
    );
    assert_eq!(receipt.kept_waste.delta_base_references, expected_bases);
    assert!(receipt.kept_waste.delta_bases_bytes > 0);
    assert_eq!(receipt.residue.bytes, 0);
    assert!(receipt.residue.artifacts.is_empty());
    assert!(receipt.residue.reason.is_none());
    assert_no_artifacts(&path);

    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), generation_before + 1);
    let after = totals(&path);
    // #711: `logicalDiscarded` is the canonical bytes of the collected
    // objects (shared objects once), not the history rows' `logical` sum; the
    // history delta for this fixture is only 160 bytes, while over 180 kB of
    // canonical objects are actually discarded.
    assert_eq!(
        receipt.logical_discarded,
        before.canonical - after.canonical,
        "logicalDiscarded is the canonical bytes of the collected objects"
    );
    assert_eq!(before.logical - after.logical, 160); // C4 + stack B genesis + C5 + Lb
    assert!(receipt.logical_discarded > 160);
    assert_eq!(after.stored, sum_pack_lengths(&path));
    assert_eq!(after.canonical, sum_canonical(&path));
    assert_eq!(after.metadata, sum_metadata_packs(&path));
    assert_eq!(after.logical, sum_history_logical(&path));

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
        assert_eq!(pack_row(&path, *pack), 0, "dead pack {pack} is removed");
    }
    for pack in &facts.live {
        assert_eq!(pack_row(&path, *pack), 1, "live pack {pack} stays");
    }

    // The retained pack is written again without the unreached group: its
    // body and its whole-file directory entry are gone, and the pack keeps
    // its id.
    assert_eq!(pack_of(&path, ids.live_group), Some(ids.shared_pack));
    assert_eq!(object_count(&path, ids.live_group), 1);
    assert_eq!(object_count(&path, ids.dead_group), 0);
    assert_eq!(
        pack_length(&path, ids.shared_pack),
        ids.shared_pack_length - ids.shared_dead_group - layout::WHOLE_FILE_ENTRY_LEN as u64
    );
    // AC-17: a copied pack keeps its identity.
    for (id, pack) in &ids.delta_chain {
        assert_eq!(
            pack_of(&path, *id),
            Some(*pack),
            "pack_id of {id} is preserved"
        );
    }

    // Retained history and markers survive; collected rows take their markers.
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
        assert_eq!(stack_row(&path, *id), 1);
    }
    for id in &ids.stacks_collected {
        assert!(reopened.history.layer_stack(*id).unwrap().is_none());
        assert_eq!(stack_row(&path, *id), 0);
    }
    for id in &ids.branches_live {
        assert!(reopened.history.branch(*id).unwrap().is_some());
    }
    assert_eq!(retired_branches(&path), ids.branches_retained_retired);
    for id in &ids.branches_collected {
        assert_eq!(branch_row(&path, *id), 0, "collected branch row {id}");
    }
    assert_eq!(retired_stacks(&path), 0);

    // AC-18: a digest row survives only when a retained root reaches the root.
    assert_eq!(digest_rows(&path, &ids.digest_retained), 1);
    assert_eq!(digest_rows(&path, &ids.digest_collected), 0);

    assert_doctor_clean(&reopened);
    assert_eq!(
        reopened.history.mark().unwrap().live_heads_digest(),
        mark.live_heads_digest(),
        "S9: the live-heads digest matches before and after"
    );

    let fixture = Json::Obj(vec![
        (
            "branches",
            Json::Obj(vec![
                (
                    "collected",
                    branch_ids(ids.branches_collected.iter().copied()),
                ),
                ("live", branch_ids(ids.branches_live.iter().copied())),
                (
                    "retainedRetired",
                    branch_ids(ids.branches_retained_retired.iter().copied()),
                ),
            ]),
        ),
        (
            "commits",
            Json::Obj(vec![
                (
                    "collected",
                    commit_ids(ids.commits_collected.iter().copied()),
                ),
                ("retained", commit_ids(ids.commits_retained.iter().copied())),
            ]),
        ),
        ("deadBytesBound", Json::Num(facts.dead_bytes)),
        (
            "deltaBaseReferences",
            object_ids(ids.delta_bases_sorted.iter().copied()),
        ),
        (
            "deltaChainPackIds",
            numbers(ids.delta_chain.iter().map(|(_, pack)| *pack)),
        ),
        ("inodeBlocksBytes", Json::Num(inode_blocks)),
        (
            "layers",
            Json::Obj(vec![
                ("collected", layer_ids(ids.layers_collected.iter().copied())),
                ("retained", layer_ids(ids.layers_retained.iter().copied())),
            ]),
        ),
        (
            "objects",
            Json::Obj(vec![
                ("dead", object_ids(ids.objects_dead.iter().copied())),
                ("live", object_ids(ids.objects_live.iter().copied())),
            ]),
        ),
        (
            "packs",
            Json::Obj(vec![
                ("dead", numbers(facts.dead.iter().copied())),
                ("live", numbers(facts.live.iter().copied())),
                ("shared", Json::Num(ids.shared_pack as u64)),
                ("sharedDeadGroupBytes", Json::Num(ids.shared_dead_group)),
                ("sharedLength", Json::Num(ids.shared_pack_length)),
            ]),
        ),
        (
            "stacks",
            Json::Obj(vec![
                ("collected", stack_ids(ids.stacks_collected.iter().copied())),
                ("live", stack_ids(ids.stacks_live.iter().copied())),
            ]),
        ),
    ]);
    let after_state = Json::Obj(vec![
        ("branchRetired", branch_ids(retired_branches(&path))),
        ("deadPacksGone", Json::Bool(true)),
        ("fileDigestCollected", Json::Bool(false)),
        ("fileDigestRetained", Json::Bool(true)),
        ("layerStackRetired", Json::Num(retired_stacks(&path) as u64)),
        ("liveReadable", Json::Bool(true)),
        ("liveHeadsDigestUnchanged", Json::Bool(true)),
        ("fullDoctor", Json::Str("clean".to_owned())),
    ]);
    Json::Obj(vec![
        ("after", after_state),
        ("fixture", fixture),
        (
            "mark",
            mark_json(mark.live_heads_digest(), mark.objects().len(), &live_packs),
        ),
        ("receipt", receipt_json(&receipt)),
        ("totalsAfter", totals_json(after)),
        ("totalsBefore", totals_json(before)),
    ])
}

fn digest_vector_case() -> Json {
    let f = Fixture::new();
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

    // branch_5 has never committed: an absent head is hashed as 33 zero bytes.
    let with_empty_head = f.handles.history.mark().unwrap().live_heads_digest();
    assert_eq!(
        with_empty_head,
        f.handles.history.mark().unwrap().live_heads_digest(),
        "the fixed fixture pins one digest"
    );
    assert_eq!(
        with_empty_head,
        expected_heads_digest(
            &[
                (stack_3.id, stack_3.head_layer),
                (stack_9.id, stack_9.head_layer)
            ],
            &[
                (branch_3, Some(commit_id), stack_3.head_layer),
                (branch_5, None, stack_9.head_layer),
                (branch_9, None, stack_9.head_layer),
            ],
        ),
        "an absent head is 33 zero bytes"
    );
    // A retired Branch is absent, not zeroed.
    f.handles
        .history
        .mark_branch_removed(branch_5, 5_000)
        .unwrap();
    let retired_branch = f.handles.history.mark().unwrap().live_heads_digest();
    assert_ne!(
        with_empty_head, retired_branch,
        "a retired Branch leaves the digest instead of hashing zeros"
    );
    assert_eq!(
        retired_branch,
        expected_heads_digest(
            &[
                (stack_3.id, stack_3.head_layer),
                (stack_9.id, stack_9.head_layer)
            ],
            &[
                (branch_3, Some(commit_id), stack_3.head_layer),
                (branch_9, None, stack_9.head_layer),
            ],
        ),
        "a retired Branch row is absent, not zeroed"
    );
    assert_eq!(
        retired_branch,
        f.handles.history.mark().unwrap().live_heads_digest()
    );
    Json::Obj(vec![
        ("absentHeadDigest", Json::Str(hex(&with_empty_head))),
        ("commit", commit_ids([commit_id])),
        ("retiredBranchDigest", Json::Str(hex(&retired_branch))),
        ("retiredStack", stack_ids([stack_7.id])),
    ])
}

#[test]
fn the_sqlite_compaction_receipts_match_the_golden_transcript() {
    let transcript = Json::Obj(vec![
        ("digestVector", digest_vector_case()),
        ("emptyNone", empty_none_case()),
        ("noDeadNone", no_dead_none_case()),
        ("rebuild", rebuild_case()),
    ]);
    check_golden(&transcript);
}
