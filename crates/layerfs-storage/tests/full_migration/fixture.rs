use layerfs_core::content::rope::build;
use layerfs_core::inode::{inode_table_from_root, InodeId, InodeKind, InodeRecordV1};
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root, profile_id};
use layerfs_core::{ObjectId, ObjectKind};
use layerfs_storage::{
    branch_push_bundle_page_digest, BranchHead, BranchId, BranchPushIdentityBuilder,
    BranchPushOutcome, BranchPushRequest, Engine, LayerStackHead, RequestId, SyncTransferCounters,
};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-full-migration-{label}-{}-{}.sqlite",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

pub(super) fn remove(path: &Path) {
    if path.exists() {
        std::fs::remove_file(path).unwrap();
    }
}

pub(super) fn schema_counts(path: &Path) -> (i64, i64) {
    let connection = Connection::open(path).unwrap();
    let tables = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema
             WHERE type = 'table' AND name NOT GLOB 'sqlite_*'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let indexes = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'index' AND sql IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    (tables, indexes)
}

pub(super) fn object_row(path: &Path, object_id: &[u8; 32]) -> (i64, Vec<u8>, i64, i64, Vec<u8>) {
    Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT rowid, object_id, kind, canonical_length, canonical_bytes
             FROM layerfs_objects WHERE object_id = ?1",
            params![object_id.as_slice()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap()
}

pub(super) fn publish_valid_root(
    engine: &Engine,
    name: &str,
    seed: [u8; 32],
) -> (ObjectId, Vec<u8>) {
    let mut publication = engine.begin_publication(None, name).unwrap();
    let (mode, _) = build(&mut publication, 0o755_u32.to_be_bytes().as_slice()).unwrap();
    let mut mtime = 0_i64.to_be_bytes().to_vec();
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(&mut publication, mtime.as_slice()).unwrap();
    let metadata = build_metadata_tree(
        &mut publication,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_file_root: mtime.0,
            },
        ],
    )
    .unwrap();
    let directory = empty_directory(&mut publication).unwrap();
    let root_inode = InodeId::allocate(seed, 0);
    let inode_record = publication
        .put_object(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let inode_table = inode_table_from_root(&mut publication, root_inode, inode_record)
        .unwrap()
        .0;
    let canonical = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: inode_table,
    })
    .unwrap();
    (
        publication.publish_namespace(&canonical).unwrap().root,
        canonical,
    )
}

pub(super) fn transfer_all(
    source: &Engine,
    destination: &Engine,
    owner: RequestId,
    request: RequestId,
    direction: &str,
) -> (Vec<ObjectId>, SyncTransferCounters) {
    let ids = source.object_ids_page(None, 1024).unwrap();
    let objects = ids
        .iter()
        .map(|id| {
            (
                *id,
                source
                    .load_canonical_authenticated_bounded(*id, 1024 * 1024)
                    .unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let counters = SyncTransferCounters {
        unique_bytes: objects
            .iter()
            .map(|(_, canonical)| canonical.len() as u64)
            .sum(),
        ..SyncTransferCounters::default()
    };
    destination
        .accept_canonical_batch_pinned(owner, request, direction, &objects)
        .unwrap();
    (ids, counters)
}

pub(super) fn push_branch_to_authority(
    working: &Engine,
    durable: &Engine,
    stack: LayerStackHead,
    branch_id: BranchId,
    seed: u8,
) -> BranchHead {
    let transfer = RequestId::from_bytes([seed; 32]);
    let data_request = RequestId::from_bytes([seed + 1; 32]);
    let (_, counters) = transfer_all(working, durable, transfer, data_request, "push");
    durable
        .product_create_layer_stack(
            stack.layer_stack_id,
            stack.layer_id,
            "authority",
            stack.root,
        )
        .unwrap();
    let bundle = working.product_export_branch_push(branch_id, None).unwrap();
    durable
        .product_stage_branch_push_page(transfer, 0, data_request, &bundle, counters)
        .unwrap();
    let digest =
        branch_push_bundle_page_digest(transfer, 0, data_request, &bundle, counters).unwrap();
    let mut identity = BranchPushIdentityBuilder::new(transfer);
    identity.absorb_page(0, digest).unwrap();
    match durable
        .product_commit_staged_branch_push(
            BranchPushRequest {
                request_id: transfer,
                transfer_id: transfer,
                candidate_digest: identity.finish(bundle.head),
                expected: None,
                counters,
            },
            branch_id,
        )
        .unwrap()
    {
        BranchPushOutcome::DurablyAccepted { head, .. } => head,
        BranchPushOutcome::Conflict { .. } => panic!("unexpected authority conflict"),
    }
}

pub(super) const BYTES_KIND: i64 = ObjectKind::Bytes as u8 as i64;
