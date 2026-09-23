//! Bounded multi-group placement and same-save dependency barriers.

mod support;

use layerfs_content::{
    file::mapping::encode_chunk_object, AdvisoryPredecessors, FinalizedObject, ObjectRole,
    PredecessorProvenance,
};
use support::{
    assembled_small_object, create_store, disabled, noise, open_store, read_objects, TempDir,
};

fn whole(raw: &[u8]) -> FinalizedObject {
    FinalizedObject::new(ObjectRole::WholeFile, assembled_small_object(raw)).unwrap()
}

fn distinct(index: u64, len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ index.wrapping_mul(0x2545_f491_4f6c_dd1d);
    (0..len)
        .map(|_| {
            state ^= state << 7;
            state ^= state >> 9;
            state ^= state << 8;
            state as u8
        })
        .collect()
}

#[test]
fn several_groups_share_a_bounded_pack_write_and_reopen() {
    let dir = TempDir::new("c2-bulk");
    let path = dir.store_path("bulk");
    let store = create_store(&path);
    let payloads: Vec<_> = (0..120).map(|index| distinct(index, 2_000)).collect();
    let objects: Vec<_> = payloads.iter().map(|payload| whole(payload)).collect();
    let ids: Vec<_> = objects.iter().map(FinalizedObject::id).collect();

    let outcome = disabled(|scope| {
        let mut save = store.begin_save(scope.child("begin"))?;
        for object in objects {
            save.accept(object)?;
        }
        save.finish(scope.child("finish"))
    })
    .unwrap();
    assert_eq!(outcome.inserted, 120);
    assert!(
        outcome.packs_created + outcome.pack_appends <= 3,
        "several whole-file groups should share each pack increment: {outcome:?}"
    );

    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        4096
    );
    drop(connection);
    let reopened = open_store(&path);
    let (actual, _) = read_objects(&reopened, &ids).unwrap();
    for (value, expected) in actual.iter().zip(&payloads) {
        assert_eq!(value, &assembled_small_object(expected));
    }
}

#[test]
fn queued_predecessor_is_placed_before_same_save_delta_selection() {
    let dir = TempDir::new("c2-queued-base");
    let path = dir.store_path("queued-base");
    let store = create_store(&path);
    let raw = noise(40_000);
    let base = whole(&raw);
    let base_id = base.id();
    let filler = whole(&distinct(999, 40_000));
    let mut changed = raw.clone();
    changed[10_000..11_000].copy_from_slice(&distinct(1000, 1_000));
    let mut predecessors = AdvisoryPredecessors::new();
    predecessors
        .push(base_id, PredecessorProvenance::OriginalBase)
        .unwrap();
    let dependent = whole(&changed).with_predecessors(predecessors);
    let dependent_id = dependent.id();

    let outcome = disabled(|scope| {
        let mut save = store.begin_save(scope.child("begin"))?;
        save.accept(base)?;
        save.accept(filler)?;
        save.accept(dependent)?;
        save.finish(scope.child("finish"))
    })
    .unwrap();
    assert_eq!(outcome.inserted, 3);
    assert_eq!(
        outcome.delta.trials, 1,
        "queued base must be readable by selection"
    );
    assert_eq!(outcome.prefix_records, 1);

    drop(store);
    let reopened = open_store(&path);
    let (actual, _) = read_objects(&reopened, &[base_id, dependent_id]).unwrap();
    assert_eq!(actual[0], assembled_small_object(&raw));
    assert_eq!(actual[1], assembled_small_object(&changed));
}

#[test]
fn queued_duplicate_and_lane_switch_keep_exact_rows() {
    let dir = TempDir::new("c2-lane-switch");
    let path = dir.store_path("lane-switch");
    let store = create_store(&path);
    let payloads: Vec<_> = (0..40).map(|index| distinct(index + 200, 2_000)).collect();
    let whole_objects: Vec<_> = payloads.iter().map(|payload| whole(payload)).collect();
    let first_id = whole_objects[0].id();
    let chunk_payloads: Vec<_> = (0..3).map(|index| distinct(index + 300, 20_000)).collect();
    let chunks: Vec<_> = chunk_payloads
        .iter()
        .map(|raw| {
            FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(raw).unwrap()).unwrap()
        })
        .collect();
    let first_chunk = chunks[0].id();
    let chunk_ids: Vec<_> = chunks.iter().map(FinalizedObject::id).collect();

    let outcome = disabled(|scope| {
        let mut save = store.begin_save(scope.child("begin"))?;
        for object in whole_objects {
            save.accept(object)?;
        }
        save.accept(whole(&payloads[0]))?; // exact repeat while the first group is queued
        for object in chunks {
            save.accept(object)?;
        }
        save.finish(scope.child("finish"))
    })
    .unwrap();
    assert_eq!(outcome.inserted, 43);
    assert_eq!(outcome.reused, 1);

    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    let pack = |id: layerfs_content::ObjectId| {
        connection
            .query_row(
                "SELECT pack_id FROM objects WHERE object_id=?1",
                [id.to_bytes().to_vec()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
    };
    assert!(
        pack(first_id) < pack(first_chunk),
        "lane switch must publish earlier queued groups first"
    );
    drop(connection);
    let reopened = open_store(&path);
    let (actual, _) = read_objects(&reopened, &chunk_ids).unwrap();
    for (value, expected) in actual.iter().zip(&chunk_payloads) {
        assert_eq!(value, &encode_chunk_object(expected).unwrap());
    }
    let (first, _) = read_objects(&reopened, &[first_id]).unwrap();
    assert_eq!(first[0], assembled_small_object(&payloads[0]));
}

#[test]
fn queued_wave_commits_before_same_save_read_and_abort_removes_private_rows() {
    let dir = TempDir::new("c2-wave");
    let path = dir.store_path("wave");
    let store = create_store(&path);
    let payloads: Vec<_> = (0..520).map(|index| distinct(index + 400, 500)).collect();
    let objects: Vec<_> = payloads.iter().map(|payload| whole(payload)).collect();
    let ids: Vec<_> = objects.iter().map(FinalizedObject::id).collect();
    let outcome = disabled(|scope| {
        let mut save = store.begin_save(scope.child("begin"))?;
        for (index, object) in objects.into_iter().enumerate() {
            save.accept(object)?;
            if index == 512 {
                // The 513th accept drains the first 512-object wave. Its queue
                // must have been placed and committed before this public read.
                let values = save.read_batch(&[ids[0], ids[512]], scope.child("read"))?;
                assert_eq!(values[0], assembled_small_object(&payloads[0]));
                assert_eq!(values[1], assembled_small_object(&payloads[512]));
            }
        }
        save.finish(scope.child("finish"))
    })
    .unwrap();
    assert_eq!(outcome.inserted, 520);
    assert!(outcome.commits >= 2);

    drop(store);
    let reopened = open_store(&path);
    for (index, page) in ids.chunks(64).enumerate() {
        let (actual, _) = read_objects(&reopened, page).unwrap();
        for (offset, value) in actual.iter().enumerate() {
            assert_eq!(
                value,
                &assembled_small_object(&payloads[index * 64 + offset])
            );
        }
    }

    let abort_path = dir.store_path("aborted");
    let abort_store = create_store(&abort_path);
    let private: Vec<_> = (0..513)
        .map(|index| whole(&distinct(index + 1000, 500)))
        .collect();
    let private_id = private[0].id();
    disabled(|scope| {
        let mut save = abort_store.begin_save(scope.child("begin"))?;
        for object in private {
            save.accept(object)?;
        }
        save.abort(scope.child("abort"))
    })
    .unwrap();
    drop(abort_store);
    let reopened = open_store(&abort_path);
    assert!(read_objects(&reopened, &[private_id]).is_err());
}
