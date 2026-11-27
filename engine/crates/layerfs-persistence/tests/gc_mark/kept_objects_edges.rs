//! #41: the bounded mark at its edges. A mark reads the Store's locators in
//! blocks of 512 and in reads of 16 blocks, keeps one bit per object and
//! finds each reached identity by its block: these Stores sit exactly on
//! those boundaries. An object reached twice, in one wave or in two, is kept
//! once, and an identity the Store does not locate fails the mark instead of
//! being skipped.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use crate::support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::HistoryError;
use layerfs_persistence::{
    CompactPath, CompactRequest, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{
    encoding::{encode_prefix, raw_payload, CompressionWorkspace},
    save::SaveProfile,
    StorageCapacities,
};
use std::collections::BTreeSet;
use support::doctor::Fixture;
use support::marks::{leaf, left_beside, publish_files, retain, small_file};

/// Located objects of a Store: one, around one block, around one read of
/// blocks and around two.
const TOTALS: &[usize] = &[1, 2, 511, 512, 513, 8191, 8192, 8193, 16384, 16385];

/// A Store of exactly `total` located objects where only identities at block
/// edges are retained: each is found, and nothing else is.
#[test]
fn sparse_roots_at_block_edges() {
    for &total in TOTALS {
        let f = Fixture::new();
        let files: Vec<_> = (0..total as u64).map(small_file).collect();
        publish_files(&f, &files);
        let mut sorted: Vec<ObjectId> = files.iter().map(FinalizedObject::id).collect();
        sorted.sort_unstable();
        let ranks = [
            0usize, 1, 510, 511, 512, 513, 1022, 1023, 1024, 1025, 8190, 8191, 8192, 8193,
        ];
        let mut kept: BTreeSet<ObjectId> = ranks
            .iter()
            .filter(|rank| **rank < total)
            .map(|rank| sorted[*rank])
            .collect();
        kept.insert(sorted[total - 1]);
        kept.insert(sorted[total.saturating_sub(2)]);
        // Roots in an order that is not identity order.
        let mut roots: Vec<ObjectId> = kept.iter().copied().collect();
        roots.reverse();
        retain(&f.handles, &roots);
        let located: i64 = f
            .raw()
            .query_row("SELECT count(*) FROM object_location", [], |row| row.get(0))
            .unwrap();
        assert_eq!(located as usize, total, "the Store locates exactly {total}");
        let mark = f.handles.history.mark().unwrap();
        assert_eq!(
            mark.objects().objects(),
            kept.iter().copied().collect::<Vec<_>>(),
            "total {total}"
        );
        assert_eq!(mark.objects().len(), kept.len(), "total {total}");
        assert!(
            left_beside(&f).is_empty(),
            "total {total}: {:?}",
            left_beside(&f)
        );
    }
}

/// A Store of exactly `total` located objects, every one reached: extent
/// pages name every payload object.
#[test]
fn every_object_reached_at_block_edges() {
    for &total in TOTALS {
        // files + ceil(files / 120) == total
        let mut count = total;
        while count > 0 && count + count.div_ceil(120) > total {
            count -= 1;
        }
        if count == 0 {
            // One object cannot be a page and the file it names.
            continue;
        }
        let f = Fixture::new();
        let files: Vec<_> = (0..count as u64).map(small_file).collect();
        publish_files(&f, &files);
        let ids: Vec<ObjectId> = files.iter().map(FinalizedObject::id).collect();
        let mut leaves: Vec<ObjectId> = ids.chunks(120).map(|group| leaf(&f, group)).collect();
        let mut expected: BTreeSet<ObjectId> = ids.iter().copied().collect();
        expected.extend(leaves.iter().copied());
        // Top up to the exact total with single-file pages over fresh files.
        let mut seed = 1_000_000u64;
        while expected.len() + 2 <= total {
            let extra = small_file(seed);
            seed += 1;
            publish_files(&f, std::slice::from_ref(&extra));
            let page = leaf(&f, &[extra.id()]);
            expected.insert(extra.id());
            expected.insert(page);
            leaves.push(page);
        }
        if expected.len() < total {
            let extra = small_file(seed);
            publish_files(&f, std::slice::from_ref(&extra));
            expected.insert(extra.id());
            leaves.push(extra.id());
        }
        assert_eq!(expected.len(), total);
        retain(&f.handles, &leaves);
        let located: i64 = f
            .raw()
            .query_row("SELECT count(*) FROM object_location", [], |row| row.get(0))
            .unwrap();
        assert_eq!(located as usize, total);
        let mark = f.handles.history.mark().unwrap();
        assert_eq!(mark.objects().len(), total, "total {total}");
        assert_eq!(
            mark.objects().objects(),
            expected.iter().copied().collect::<Vec<_>>(),
            "total {total}"
        );
    }
}

/// An object reached twice in one wave (two pages name it) and across waves
/// (a root that a page of a later wave names again) is kept once.
#[test]
fn an_object_reached_twice_is_kept_once() {
    let f = Fixture::new();
    let files: Vec<_> = (0..300u64).map(small_file).collect();
    publish_files(&f, &files);
    let ids: Vec<ObjectId> = files.iter().map(FinalizedObject::id).collect();
    let first = leaf(&f, &ids[0..120]);
    let second = leaf(&f, &ids[100..220]);
    // ids[5] is a root (wave 1) and named by `first` (wave 2); ids[250] only a root.
    retain(
        &f.handles,
        &[ids[5], first, second, ids[250], ids[5], first],
    );
    let mut expected: BTreeSet<ObjectId> = ids[0..220].iter().copied().collect();
    expected.extend([first, second, ids[250]]);
    let mark = f.handles.history.mark().unwrap();
    assert_eq!(mark.objects().len(), expected.len());
    assert_eq!(
        mark.objects().objects(),
        expected.iter().copied().collect::<Vec<_>>()
    );
    assert!(mark.objects().waste().dead_in_retained_packs_bytes() > 0);
}

/// The same across waves when the sorters write runs to files.
#[test]
fn an_object_reached_twice_across_waves_with_sort_files() {
    const LEAVES: usize = 920;
    const EXTENTS: usize = 120;
    let f = Fixture::new();
    let files: Vec<_> = (0..(LEAVES * EXTENTS) as u64).map(small_file).collect();
    publish_files(&f, &files);
    let ids: Vec<ObjectId> = files.iter().map(FinalizedObject::id).collect();
    let mut roots: Vec<ObjectId> = ids
        .chunks(EXTENTS)
        .enumerate()
        .map(|(index, group)| {
            let mut group = group.to_vec();
            // Every page also names ids[0]: reached 920 times in one wave.
            if index > 0 {
                group.push(ids[0]);
            }
            leaf(&f, &group)
        })
        .collect();
    let leaves = roots.clone();
    // Payload objects that are roots too: located in wave 1, named again in wave 2.
    roots.extend([ids[7], ids[LEAVES * EXTENTS - 1], ids[50_000]]);
    retain(&f.handles, &roots);
    let mut expected: BTreeSet<ObjectId> = ids.iter().copied().collect();
    expected.extend(leaves.iter().copied());
    let mark = f.handles.history.mark().unwrap();
    assert!(mark.scratch_bytes() > 0);
    assert_eq!(mark.objects().len(), expected.len());
    assert_eq!(
        mark.objects().objects(),
        expected.iter().copied().collect::<Vec<_>>()
    );
    assert_eq!(mark.objects().waste().dead_in_retained_packs_bytes(), 0);
    assert!(left_beside(&f).is_empty());
}

/// Stores `canonical` as a delta of `base` and returns it.
fn delta_target(
    f: &Fixture,
    base: &FinalizedObject,
    role: ObjectRole,
    canonical: Vec<u8>,
) -> FinalizedObject {
    let target = FinalizedObject::new(role, canonical).unwrap();
    let capacities = StorageCapacities::from_policy(f.policy).unwrap();
    let record = encode_prefix(
        target.canonical(),
        target.role(),
        base.id(),
        raw_payload(base.canonical(), base.role()).unwrap(),
        &capacities,
        &mut CompressionWorkspace::new().unwrap(),
        &mut SaveProfile::default(),
    )
    .unwrap();
    f.record(&target, record.lane, record.record, vec![]);
    target
}

/// A delta base that the Store does not locate fails the mark and the
/// compaction; it is not skipped.
#[test]
fn a_missing_payload_delta_base_fails_the_mark() {
    let f = Fixture::new();
    // The base is never stored.
    let base = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"0123456789abcdef base payload").unwrap(),
    )
    .unwrap();
    let target = delta_target(
        &f,
        &base,
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"0123456789abcdef base payload, extended").unwrap(),
    );
    // Enough other objects that the Store has several blocks.
    let files: Vec<_> = (0..2_000u64).map(small_file).collect();
    publish_files(&f, &files);
    retain(&f.handles, &[target.id(), files[3].id()]);
    assert_eq!(
        f.handles.history.mark().unwrap_err(),
        HistoryError::Integrity("mark object locator")
    );
    let Fixture { handles, temp, .. } = f;
    assert_eq!(
        handles
            .compact(&CompactRequest {
                op_id: [3; 16],
                ..CompactRequest::default()
            })
            .unwrap_err(),
        HistoryError::Integrity("mark object locator")
    );
    let left: Vec<_> = std::fs::read_dir(temp.join("."))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with("store.sqlite") || name.contains("compact"))
        .collect();
    assert!(left.is_empty(), "left after a failed compaction: {left:?}");
}

/// A delta base whose locator row was removed after the fact fails the mark.
#[test]
fn a_payload_delta_base_whose_row_is_gone_fails_the_mark() {
    let f = Fixture::new();
    let base = f.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"0123456789abcdef base payload").unwrap(),
    );
    let target = delta_target(
        &f,
        &base,
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"0123456789abcdef base payload, extended").unwrap(),
    );
    retain(&f.handles, &[target.id()]);
    let mark = f.handles.history.mark().unwrap();
    assert!(mark.objects().contains(base.id()));
    assert_eq!(mark.objects().len(), 2);
    let removed = f
        .raw()
        .execute(
            "DELETE FROM object_location WHERE object_id=?1",
            [base.id().as_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(removed, 1);
    assert_eq!(
        f.handles.history.mark().unwrap_err(),
        HistoryError::Integrity("mark object locator")
    );
}

/// A logical reference the Store does not locate fails the mark.
#[test]
fn a_missing_logical_reference_fails_the_mark() {
    let f = Fixture::new();
    let files: Vec<_> = (0..120u64).map(small_file).collect();
    publish_files(&f, &files[..119]);
    let ids: Vec<ObjectId> = files.iter().map(FinalizedObject::id).collect();
    let page = leaf(&f, &ids);
    retain(&f.handles, &[page]);
    assert_eq!(
        f.handles.history.mark().unwrap_err(),
        HistoryError::Integrity("mark object locator")
    );
}

/// A rebuild after a sparse mark at the 512 and 8192 edges keeps exactly the roots.
#[test]
fn a_rebuild_at_block_edges_keeps_exactly_the_roots() {
    for total in [512usize, 513, 8192, 8193] {
        let f = Fixture::new();
        let files: Vec<_> = (0..total as u64).map(small_file).collect();
        publish_files(&f, &files);
        let mut sorted: Vec<ObjectId> = files.iter().map(FinalizedObject::id).collect();
        sorted.sort_unstable();
        let kept: BTreeSet<ObjectId> = [0, 511, 512, total - 1]
            .iter()
            .filter(|rank| **rank < total)
            .map(|rank| sorted[*rank])
            .collect();
        retain(&f.handles, &kept.iter().copied().collect::<Vec<_>>());
        let store = f.temp.join("store.sqlite");
        let Fixture { handles, temp, .. } = f;
        let receipt = handles
            .compact(&CompactRequest {
                op_id: [7; 16],
                ..CompactRequest::default()
            })
            .unwrap();
        assert_eq!(receipt.path, CompactPath::Rebuild);
        let reopened = SqliteHandles::open_writable(
            PersistenceConfig::sqlite(store).with_sqlite_profile(SqlitePersistenceProfile::Normal),
            b"doctor-adversarial",
            [71; 32],
        )
        .unwrap();
        let after = reopened.history.mark().unwrap();
        assert_eq!(
            after.objects().objects(),
            kept.iter().copied().collect::<Vec<_>>(),
            "total {total}"
        );
        let located: Vec<Vec<u8>> = {
            let raw = rusqlite::Connection::open(temp.join("store.sqlite")).unwrap();
            let mut statement = raw
                .prepare("SELECT object_id FROM object_location ORDER BY object_id")
                .unwrap();
            let rows = statement.query_map([], |row| row.get(0)).unwrap();
            rows.map(Result::unwrap).collect()
        };
        assert_eq!(
            located,
            kept.iter()
                .map(|id| id.as_bytes().to_vec())
                .collect::<Vec<_>>(),
            "total {total}"
        );
    }
}
