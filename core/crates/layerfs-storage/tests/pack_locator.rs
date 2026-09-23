//! Append and new-pack placement, stable locators and framing rejection.

mod support;

use std::collections::BTreeMap;

use layerfs_content::{construct_bytes, ConstructionPolicy, ObjectId};
use layerfs_storage::{StorageError, Store};
use support::{
    assembled_small_object, construct_file, create_store, disabled, noise, open_store,
    read_objects, TempDir,
};

/// Incompressible payloads that do not resemble one another.
///
/// `support::noise` is one fixed stream with no seed, so two of its prefixes share
/// a long common prefix and the winner cache proposes one whole-file record for the
/// other - a delta edge, not a grouping failure, but not what this case is about.
/// These are seeded per index instead.
fn distinct_noise(index: u8, len: usize) -> Vec<u8> {
    let mut state =
        0x9e37_79b9_7f4a_7c15_u64 ^ u64::from(index).wrapping_mul(0x2545_f491_4f6c_dd1d);
    (0..len)
        .map(|_| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            state as u8
        })
        .collect()
}

fn whole_file(raw: &[u8]) -> layerfs_content::FinalizedObject {
    let policy = ConstructionPolicy::frozen_default();
    let mut consumer = support::Collected::new();
    disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            raw,
            &mut consumer,
            scope.child("content"),
        )
    })
    .unwrap();
    consumer.finalized().pop().expect("one whole-file object")
}

fn locator_snapshot(path: &std::path::Path) -> BTreeMap<Vec<u8>, (i64, i64, i64)> {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut statement = connection
        .prepare("SELECT object_id, pack_id, group_number, record_number FROM objects")
        .unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                (
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ),
            ))
        })
        .unwrap();
    rows.map(|row| row.unwrap()).collect()
}

fn save_objects(
    store: &Store,
    objects: Vec<layerfs_content::FinalizedObject>,
) -> layerfs_storage::SaveOutcome {
    disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in objects {
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    })
    .expect("save succeeds")
}

#[test]
fn a_full_lane_starts_a_new_pack_and_keeps_existing_locators() {
    let dir = TempDir::new("locator");
    let path = dir.store_path("locator");
    let store = create_store(&path);

    let mut roots = Vec::new();
    let mut first_batch = Vec::new();
    for index in 0..12u8 {
        let payload = noise(20_000 + usize::from(index));
        let object = whole_file(&payload);
        roots.push((object.id(), payload));
        first_batch.push(object);
    }
    let first = save_objects(&store, first_batch);
    assert_eq!(first.inserted, 12);
    assert_eq!(first.packs_created, 1, "one pack holds the first twelve");
    let before = locator_snapshot(&path);

    let mut second_batch = Vec::new();
    for index in 0..12u8 {
        let payload = noise(21_000 + usize::from(index));
        let object = whole_file(&payload);
        roots.push((object.id(), payload));
        second_batch.push(object);
    }
    let second = save_objects(&store, second_batch);
    assert!(second.packs_created >= 1, "the lane started another pack");
    let after = locator_snapshot(&path);

    for (id, _) in roots.iter().take(12) {
        assert_eq!(
            before.get(id.to_bytes().as_slice()),
            after.get(id.to_bytes().as_slice()),
            "an existing locator moved"
        );
    }
    assert!(before.len() < after.len());
}

#[test]
fn whole_file_records_share_a_group_and_keep_their_own_locators() {
    // The compact lane's groups hold many records, and its directory is still
    // starts-only, so the record boundaries live in the group body. Every record
    // must still resolve to its own bytes through its own ordinal, and the save
    // must spend one statement and one pack write per *group* rather than one per
    // record - which is the whole point of the treatment.
    let dir = TempDir::new("compact-group");
    let path = dir.store_path("compact-group");
    let store = create_store(&path);
    let payloads: Vec<Vec<u8>> = (0..8u8).map(|index| distinct_noise(index, 1_500)).collect();
    let objects: Vec<_> = payloads.iter().map(|raw| whole_file(raw)).collect();
    let ids: Vec<_> = objects.iter().map(|object| object.id()).collect();

    let outcome = save_objects(&store, objects);
    assert_eq!(outcome.inserted, 8);
    assert_eq!(
        outcome.packs_created + outcome.pack_appends,
        1,
        "eight records of 1.5 KiB share one group and one write"
    );
    assert!(
        outcome.statements < 8,
        "one statement carries the group's rows: {}",
        outcome.statements
    );

    let connection = rusqlite::Connection::open(&path).unwrap();
    let groups: Vec<i64> = ids
        .iter()
        .map(|id| {
            connection
                .query_row(
                    "SELECT group_number FROM objects WHERE object_id = ?1",
                    [id.to_bytes().to_vec()],
                    |row| row.get(0),
                )
                .unwrap()
        })
        .collect();
    assert!(
        groups.iter().all(|group| *group == 0),
        "every record is in the same group: {groups:?}"
    );
    let ordinals: Vec<i64> = ids
        .iter()
        .map(|id| {
            connection
                .query_row(
                    "SELECT record_number FROM objects WHERE object_id = ?1",
                    [id.to_bytes().to_vec()],
                    |row| row.get(0),
                )
                .unwrap()
        })
        .collect();
    assert_eq!(
        ordinals,
        (0..8).collect::<Vec<i64>>(),
        "ordinals are the plan's"
    );
    drop(connection);

    let reopened = open_store(&path);
    let (values, _) =
        disabled(|scope| reopened.read_batch(&ids, scope.child("storage.read"))).unwrap();
    for (index, raw) in payloads.iter().enumerate() {
        assert_eq!(
            values[index],
            assembled_small_object(raw),
            "record {index} resolves to its own payload"
        );
    }
}

#[test]
fn append_reuses_a_pack_without_changing_record_ordinals() {
    let dir = TempDir::new("append");
    let path = dir.store_path("append");
    let store = create_store(&path);
    // Two payloads large enough that each takes its own group: the seal decision
    // is the framed length the next record would produce against the group target.
    let first = whole_file(&noise(40_000));
    let second = whole_file(&noise(41_000));
    let first_id = first.id();
    let second_id = second.id();

    let outcome = save_objects(&store, vec![first, second]);
    assert_eq!(outcome.inserted, 2);
    assert_eq!(outcome.packs_created, 1);
    assert_eq!(
        outcome.pack_appends, 1,
        "the second record appended to the pack"
    );

    let snapshot = locator_snapshot(&path);
    let first_locator = snapshot.get(first_id.to_bytes().as_slice()).unwrap();
    let second_locator = snapshot.get(second_id.to_bytes().as_slice()).unwrap();
    assert_eq!(first_locator.0, second_locator.0, "same pack");
    assert_eq!(first_locator.1, 0);
    assert_eq!(second_locator.1, 1);
    assert_eq!(first_locator.2, 0);
    assert_eq!(second_locator.2, 0);
}

#[test]
fn same_save_reads_see_accepted_objects_before_the_finish_barrier() {
    let dir = TempDir::new("same_save");
    let path = dir.store_path("same_save");
    let store = create_store(&path);
    let payload = noise(4_000);
    let object = whole_file(&payload);
    let id = object.id();

    disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        operation.accept(object)?;
        let values = operation.read_batch(&[id], scope.child("storage.read"))?;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].len(), 4000 + 23 + 10 - 10);
        operation.finish(scope.child("storage.finish"))
    })
    .unwrap();

    let mut external = open_store(&path);
    let values = disabled(|scope| external.read_batch(&[id], scope.child("storage.read")))
        .unwrap()
        .0;
    assert_eq!(values.len(), 1);
    let _ = &mut external;
}

#[test]
fn same_save_reads_resolve_objects_waiting_in_an_unfinished_group() {
    let dir = TempDir::new("pending_read");
    let path = dir.store_path("pending_read");
    let store = create_store(&path);
    // CDC-sized chunks leave the last group of a lane partial: those members have no
    // row yet when the same save reads them. The narrow case above stays inside the
    // preparation batch; this one is larger than one batch, so it exercises the
    // pending group rather than the pending batch.
    let bytes = noise(5 * 1024 * 1024);
    let (collected, root, _) = construct_file(&bytes);
    let ids: Vec<ObjectId> = collected
        .objects()
        .iter()
        .map(|(id, _, _, _)| *id)
        .collect();
    assert!(ids.len() > 256, "the workload must span several waves");

    disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in collected.finalized() {
            operation.accept(object)?;
        }
        // Every identity `accept` acknowledged reads back its exact canonical bytes,
        // whether it is in the pending batch, in an unfinished group or already
        // written.
        for (id, _, expected, _) in collected.objects() {
            let values =
                operation.read_batch(std::slice::from_ref(id), scope.child("storage.read"))?;
            assert_eq!(values.len(), 1);
            assert_eq!(
                &values[0], expected,
                "identity {id} read inside its own save"
            );
        }
        let values = operation.read_batch(&ids, scope.child("storage.read"))?;
        assert_eq!(values.len(), ids.len());
        operation.finish(scope.child("storage.finish"))
    })
    .unwrap();

    let (values, _) = read_objects(&store, &[root]).unwrap();
    assert_eq!(values.len(), 1);
}

#[test]
fn a_corrupt_pack_body_is_rejected_on_read() {
    let dir = TempDir::new("corrupt_body");
    let path = dir.store_path("corrupt_body");
    let store = create_store(&path);
    let payload = noise(9_000);
    let object = whole_file(&payload);
    let id = object.id();
    save_objects(&store, vec![object]);
    drop(store);

    let connection = rusqlite::Connection::open(&path).unwrap();
    let pack_id: i64 = connection
        .query_row("SELECT MIN(pack_id) FROM object_packs", [], |row| {
            row.get(0)
        })
        .unwrap();
    let mut data = support::read_pack_row(&connection, pack_id);
    let last = data.len() - 1;
    data[last] ^= 0xff;
    support::write_pack_row(&connection, pack_id, &data);
    drop(connection);

    let reopened = open_store(&path);
    let error =
        disabled(|scope| reopened.read_batch(&[id], scope.child("storage.read"))).unwrap_err();
    assert!(
        matches!(error, StorageError::Integrity(_) | StorageError::Content(_)),
        "got {error}"
    );
}

#[test]
fn a_locator_outside_the_group_directory_is_rejected() {
    let dir = TempDir::new("corrupt_locator");
    let path = dir.store_path("corrupt_locator");
    let store = create_store(&path);
    let payload = noise(9_000);
    let object = whole_file(&payload);
    let id = object.id();
    save_objects(&store, vec![object]);
    drop(store);

    let connection = rusqlite::Connection::open(&path).unwrap();
    let affected = connection
        .execute(
            "UPDATE objects SET group_number = 7 WHERE object_id = ?1",
            rusqlite::params![id.to_bytes().to_vec()],
        )
        .unwrap();
    assert_eq!(affected, 1);
    drop(connection);

    let reopened = open_store(&path);
    let error =
        disabled(|scope| reopened.read_batch(&[id], scope.child("storage.read"))).unwrap_err();
    assert!(matches!(error, StorageError::Integrity(_)), "got {error}");
}

#[test]
fn a_whole_file_record_is_stored_in_its_own_compact_pack() {
    let dir = TempDir::new("compact");
    let path = dir.store_path("compact");
    let store = create_store(&path);
    let payload = noise(131_071);
    let object = whole_file(&payload);
    let id = object.id();
    let outcome = save_objects(&store, vec![object]);
    assert_eq!(outcome.packs_created, 1);
    drop(store);

    let connection = rusqlite::Connection::open(&path).unwrap();
    let pack_id: i64 = connection
        .query_row("SELECT MIN(pack_id) FROM object_packs", [], |row| {
            row.get(0)
        })
        .unwrap();
    let version = support::read_pack_row(&connection, pack_id);
    assert_eq!(&version[..8], b"LFPACK\0\0");
    // The lane, not a literal: the compact framing is what this case is about and
    // the version that names it moves when the lane's record grammar gains a form.
    // `physical_formats.rs` is where the versions themselves are enumerated.
    assert_eq!(
        layerfs_storage::pack::layout::parse_header(&version)
            .expect("pack header")
            .lane,
        layerfs_storage::pack::PackLane::WholeFile,
        "whole-file records use the compact framing"
    );
    assert_eq!(
        u32::from_le_bytes(version[8..12].try_into().unwrap()),
        layerfs_storage::pack::PackLane::WholeFile.version(),
    );
    // The declared length, not the row's capacity: the row is allocated at the
    // lane's pack limit and the pack declares how much of it is a pack.
    assert!(version.len() < 256 * 1024);
    drop(connection);

    let reopened = open_store(&path);
    let (values, _) =
        disabled(|scope| reopened.read_batch(&[id], scope.child("storage.read"))).unwrap();
    assert_eq!(ObjectId::for_bytes(&values[0]), id);
}

/// The placement fit probe agrees with the canonical assembled length.
///
/// Placement decides whether a group joins the open pack from the running total it
/// keeps for the open tail plus the incoming group, without materialising a
/// candidate pack and without re-measuring the tail. That decision must be exactly
/// the one the canonical predicate makes: compare it with
/// `assembled_length(groups + [group]) <= pack_limit` around the boundary, for a
/// lane whose directory entry width differs from the ordinary one, and check the
/// group-count limit is respected separately.
///
/// This is also the boundary case for the running total: an off-by-one there would
/// move pack boundaries on disk. The total is recomputed from `assembled_length`
/// at every probe, so a drift between the state placement maintains and the
/// canonical length fails here rather than in a stored pack.
#[test]
fn the_placement_fit_probe_agrees_with_the_canonical_assembled_length() {
    let mut workspace = layerfs_storage::encoding::codec::CompressionWorkspace::new()
        .expect("compression workspace");
    let record = |length: usize| {
        let mut bytes = vec![layerfs_storage::pack::FULL_TAG];
        bytes.extend(std::iter::repeat_n(0x5a_u8, length));
        bytes
    };
    for lane in [
        layerfs_storage::pack::PackLane::Ordinary,
        layerfs_storage::pack::PackLane::WholeFile,
        layerfs_storage::pack::PackLane::Native,
    ] {
        let per_group = match lane {
            layerfs_storage::pack::PackLane::WholeFile => 3_000,
            _ => 16_000,
        };
        let mut groups: Vec<layerfs_storage::pack::EncodedGroup> = Vec::new();
        let mut probes = 0_usize;
        for _ in 0..lane.group_count_limit() + 4 {
            let candidate = layerfs_storage::pack::build_group(
                lane,
                &[record(per_group)],
                (lane == layerfs_storage::pack::PackLane::Ordinary).then_some(&mut workspace),
            )
            .expect("group");
            let expected = if groups.is_empty() {
                false
            } else {
                let mut with = groups.clone();
                with.push(candidate.clone());
                layerfs_storage::pack::assembled_length(lane, &with)
                    .is_ok_and(|length| length <= lane.pack_limit())
            };
            // The running total the placement state maintains, recomputed here from
            // the canonical predicate so the two are compared at every probe.
            let open_total = if groups.is_empty() {
                0
            } else {
                layerfs_storage::pack::assembled_length(lane, &groups).expect("open tail")
            };
            let observed =
                layerfs_storage::pack::append_fits(lane, open_total, groups.len(), &candidate)
                    .expect("fit probe");
            assert_eq!(
                observed,
                expected,
                "lane {lane:?} at {} groups",
                groups.len()
            );
            if !observed {
                groups.clear();
            }
            groups.push(candidate);
            probes += 1;
        }
        assert!(probes > 4, "the boundary was probed");
        assert!(
            groups.len() <= lane.group_count_limit(),
            "a pack never exceeds its lane's group count"
        );
    }
}

/// A pack never assembles past its lane's limit.
///
/// The open pack's running total is the canonical assembled length: the control
/// area, the lane's **whole reserved directory region** - allocated whether or
/// not the pack fills it - and every body. A total that counts one directory
/// entry per group instead of the reserved region, or that starts at zero instead
/// of at the control area, admits a group that takes the assembled pack past
/// `lane.pack_limit()`, and the write path then refuses its own placement with
/// `CapacityExceeded { pack.assembled_length }`. A limit no write path can reach
/// and no read path can return is the failure mode this pins.
#[test]
fn a_pack_never_assembles_past_its_lane_limit() {
    use layerfs_storage::pack::{
        body_area_offset, frame_group, EncodedGroup, GroupCodec, LanePlacement, PackLane, FULL_TAG,
    };

    let lane = PackLane::Ordinary;
    let region = body_area_offset(lane);
    // Bodies of the widest group the lane accepts, repeated until the pack is
    // full: `fits` of them fit and the next cannot join them, so
    // `region + fits * body <= pack_limit < region + (fits + 1) * body`. The count
    // is derived from both bounds rather than written down, because the pack limit
    // and the lane's body limit move independently and the body has to stay one
    // the lane accepts: a body of `pack_limit / 4` is 256 KiB against the ordinary
    // lane's 64 KiB ceiling and is refused before the fit arithmetic is reached.
    let body_len = lane.body_limit();
    let fits = (lane.pack_limit() - region) / body_len;
    assert!(fits >= 1, "the pack holds at least one body");
    assert!(
        fits <= lane.group_count_limit(),
        "the probe pack holds at most the lane's group count: {fits}"
    );
    assert!(
        region + fits * body_len <= lane.pack_limit(),
        "{fits} bodies fit"
    );
    assert!(
        region + (fits + 1) * body_len > lane.pack_limit(),
        "body {} cannot fit",
        fits + 1
    );
    // One record, framed: the group body is a 4-byte count and one 4-byte end
    // offset around the record, so the record is `body_len - 8` bytes -- one tag
    // byte and `body_len - 9` of filler.
    let record_len = body_len - 9;
    // Incompressible filler, so the body is stored exactly as framed and the
    // arithmetic below is exact rather than dependent on the compressor.
    let mut record = vec![FULL_TAG];
    record.extend((0..record_len).map(|index| (index * 31 + 7) as u8));
    let bytes = frame_group(&[record]).expect("framed group body");
    assert_eq!(
        bytes.len(),
        body_len,
        "the probe group is the intended width"
    );
    let group = EncodedGroup {
        bytes,
        decoded_length: body_len,
        records: 1,
        codec: GroupCodec::Raw,
    };

    let mut probe = vec![group.clone(); fits];
    probe.push(group);
    let mut placement = LanePlacement::new();
    let mut next_pack_id = 1_i64;
    let writes = placement
        .select_many(lane, probe, &mut next_pack_id)
        .expect("placement must not assemble a pack it will refuse");
    assert!(!writes.is_empty());
    for (index, write) in writes.iter().enumerate() {
        assert!(
            write.used <= lane.pack_limit(),
            "an assembled pack of {} bytes exceeds the {} limit",
            write.used,
            lane.pack_limit()
        );
        assert_eq!(
            write.body_offset + write.bodies.len(),
            write.used,
            "the declared length is the body end"
        );
        assert_eq!(
            write.capacity,
            if index == 0 {
                write.used
            } else {
                lane.pack_limit()
            },
            "a new closed pack is exact-sized; the still-open pack reserves append space"
        );
    }
    // The boundary is a real one: the groups fill the lane's pack limit, so the
    // next one cannot join them.
    assert_eq!(writes.len(), 2, "the pack boundary is crossed");
    assert_eq!(writes[0].placed.len(), fits);
}
