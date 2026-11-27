//! #41: the bounded mark on PostgreSQL at its edges. PostgreSQL hands the
//! Store's locators in signed-prefix order, so the blocks of 512 a mark
//! finds identities by are cut in that order, and the identities whose first
//! byte is 0x80 or more come first. These Stores sit on a block, on one read
//! of 16 blocks and on two, and retain the identities at every such edge and
//! at the wrap between the two halves; then every object of a Store is
//! reached, some of them many times.
//!
//! Each test creates disposable schemas. Needs `LAYERFS_TEST_POSTGRES_URL`;
//! NOT_RUN without it.
#[path = "../support/mod.rs"]
mod support;

use layerfs_content::{
    encode_whole_file_payload,
    file::mapping::{self, ExtentNode, ExtentSlice},
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::HistoryCatalog;
use layerfs_persistence::PostgresHandles;
use layerfs_storage::{
    encoding::{encode_full, CompressionWorkspace},
    location::{ObjectLocation, PackDomain},
    pack::{assemble, build_group, PackLane},
    port::{PackPersistence, Publication, PublishedPack, Reserve},
    save::SaveProfile,
    StorageCapacities, StoragePolicy,
};
use std::collections::BTreeSet;
use std::sync::Arc;
use support::pg::{commit_with, project_with_genesis, url, Schema};

const BINDING: &[u8] = b"postgresql-gc-mark-edges";
const CURSOR: [u8; 32] = [77; 32];
/// Records of one pack these tests publish.
const GROUP_RECORDS: usize = 4_000;

/// One small whole-file object, distinct for each `seed`.
fn small_file(seed: u64) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&seed.to_le_bytes()).unwrap(),
    )
    .unwrap()
}

/// Publishes `group` as one pack of one Raw group.
fn publish_group(handles: &PostgresHandles, lane: PackLane, group: &[FinalizedObject]) {
    let capacities = StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap();
    let mut workspace = CompressionWorkspace::new().unwrap();
    let mut profile = SaveProfile::default();
    let records: Vec<_> = group
        .iter()
        .map(|object| {
            encode_full(
                object.canonical(),
                object.role(),
                &capacities,
                &mut workspace,
                &mut profile,
            )
            .unwrap()
            .record
        })
        .collect();
    let body = assemble(lane, &[build_group(lane, &records, None).unwrap()]).unwrap();
    let reserved = handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::for_lane(lane),
                Arc::new(body),
            )],
            objects: group
                .iter()
                .enumerate()
                .map(|(record, object)| ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: reserved.first_pack_id,
                    group_number: 0,
                    record_number: record,
                    base: None,
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
}

/// Publishes `objects` as whole-file packs of [`GROUP_RECORDS`] records.
fn publish_files(handles: &PostgresHandles, objects: &[FinalizedObject]) {
    for group in objects.chunks(GROUP_RECORDS) {
        publish_group(handles, PackLane::WholeFile, group);
    }
}

/// Stores one extent page naming `files`, in a pack of its own.
fn leaf(handles: &PostgresHandles, files: &[ObjectId]) -> ObjectId {
    let extents: Vec<ExtentSlice> = files
        .iter()
        .map(|id| ExtentSlice::new(*id, 0, 8).unwrap())
        .collect();
    let object = FinalizedObject::new(
        ObjectRole::ExtentLeaf,
        mapping::encode_node(
            &ExtentNode::Leaf {
                subtree_logical_bytes: 8 * extents.len() as u64,
                extents,
            },
            true,
        )
        .unwrap(),
    )
    .unwrap();
    let capacities = StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap();
    let record = encode_full(
        object.canonical(),
        object.role(),
        &capacities,
        &mut CompressionWorkspace::new().unwrap(),
        &mut SaveProfile::default(),
    )
    .unwrap();
    let body = assemble(
        record.lane,
        &[build_group(record.lane, &[record.record], None).unwrap()],
    )
    .unwrap();
    let reserved = handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::for_lane(record.lane),
                Arc::new(body),
            )],
            objects: vec![ObjectLocation {
                object_id: object.id(),
                role: object.role(),
                canonical_length: object.canonical_len(),
                pack_id: reserved.first_pack_id,
                group_number: 0,
                record_number: 0,
                base: None,
            }],
            ..Publication::default()
        })
        .unwrap();
    object.id()
}

/// PostgreSQL's locator order: the signed first eight bytes, then all bytes.
fn signed_key(id: &ObjectId) -> (i64, [u8; 32]) {
    let mut head = [0u8; 8];
    head.copy_from_slice(&id.as_bytes()[..8]);
    let mut all = [0u8; 32];
    all.copy_from_slice(id.as_bytes());
    (i64::from_be_bytes(head), all)
}

/// Makes every identity of `roots` the root of a retained Commit.
fn retain(history: &dyn HistoryCatalog, roots: &[ObjectId]) {
    let (_stack, work, _layer) = project_with_genesis(history, 1, roots[0]);
    for root in &roots[1..] {
        commit_with(history, work, *root);
    }
}

/// Located objects of a Store: one, around one block, around one read of
/// blocks and around two.
const TOTALS: &[usize] = &[1, 2, 511, 512, 513, 8191, 8192, 8193, 16384, 16385];

/// Only the identities at the edges of the signed-prefix blocks, at the wrap
/// and at both ends are retained: each is found, and nothing else is.
#[test]
fn sparse_roots_at_block_edges_and_the_wrap() {
    let Some(url) = url() else { return };
    for &total in TOTALS {
        let schema = Schema::new(url.clone(), &format!("edge{total}"));
        let handles = schema.create(BINDING, CURSOR);
        let files: Vec<_> = (0..total as u64).map(small_file).collect();
        publish_files(&handles, &files);
        let mut signed: Vec<ObjectId> = files.iter().map(FinalizedObject::id).collect();
        signed.sort_unstable_by_key(signed_key);
        let negatives = signed.iter().filter(|id| id.as_bytes()[0] >= 0x80).count();
        let mut ranks: Vec<usize> = vec![
            0, 1, 510, 511, 512, 513, 1023, 1024, 1025, 8190, 8191, 8192, 8193, 16383, 16384,
        ];
        ranks.extend([
            negatives.saturating_sub(2),
            negatives.saturating_sub(1),
            negatives,
            negatives + 1,
            total.saturating_sub(2),
            total - 1,
        ]);
        let kept: BTreeSet<ObjectId> = ranks
            .iter()
            .filter(|rank| **rank < total)
            .map(|rank| signed[*rank])
            .collect();
        let mut roots: Vec<ObjectId> = kept.iter().copied().collect();
        roots.reverse();
        retain(&handles.history, &roots);
        let mark = handles.history.mark().unwrap();
        assert_eq!(
            mark.objects().objects(),
            kept.iter().copied().collect::<Vec<_>>(),
            "total {total} negatives {negatives}"
        );
        assert_eq!(mark.objects().len(), kept.len(), "total {total}");
    }
}

/// Every object is reached through extent pages, across more than two reads
/// of locators. The first object is named by every page, so one wave reaches
/// it many times, and two objects are roots as well, so the first wave
/// reaches them and the second reaches them again: each is kept once.
#[test]
fn every_object_is_reached_once_across_reads() {
    let Some(url) = url() else { return };
    for count in [600usize, 20_000] {
        let schema = Schema::new(url.clone(), &format!("full{count}"));
        let handles = schema.create(BINDING, CURSOR);
        let files: Vec<_> = (0..count as u64).map(small_file).collect();
        publish_files(&handles, &files);
        let ids: Vec<ObjectId> = files.iter().map(FinalizedObject::id).collect();
        let leaves: Vec<ObjectId> = ids
            .chunks(120)
            .enumerate()
            .map(|(index, group)| {
                let mut group = group.to_vec();
                if index > 0 {
                    group.push(ids[0]);
                }
                leaf(&handles, &group)
            })
            .collect();
        let mut roots = leaves.clone();
        roots.extend([ids[7], ids[count - 1]]);
        retain(&handles.history, &roots);
        let mut expected: BTreeSet<ObjectId> = ids.iter().copied().collect();
        expected.extend(leaves.iter().copied());
        let mark = handles.history.mark().unwrap();
        assert_eq!(mark.objects().len(), expected.len(), "count {count}");
        assert_eq!(
            mark.objects().objects(),
            expected.iter().copied().collect::<Vec<_>>(),
            "count {count}"
        );
        assert_eq!(mark.history().len(), mark.history().nodes().len());
    }
}
