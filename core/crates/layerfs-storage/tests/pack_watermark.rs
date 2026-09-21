//! The pack-allocation watermark at a step boundary, and the save's own ceiling.
//!
//! The watermark is what stops two writers over one Store from being handed the
//! same pack identifier, and it is read at `begin_write` under the write lock. A
//! step that allocates no pack therefore has nothing to add to it, and a step that
//! does must publish it before it releases the lock - deferring it to publication
//! is the change that lets the second writer collide. These cases pin both halves
//! from outside the crate: the row is read through an independent connection while
//! the save is open, and the identifiers two interleaved writers actually wrote
//! are compared.
mod support;
use layerfs_storage::Store;
use support::{construct_file, create_store, disabled, noise, TempDir};

/// The policy row through a connection that is not the save's.
fn watermark(path: &std::path::Path) -> i64 {
    let connection = rusqlite::Connection::open(path).expect("independent connection");
    connection
        .query_row(
            "SELECT next_pack_id FROM store_policy WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .expect("watermark")
}

/// Every pack id in the Store, with the save that wrote it.
fn packs(path: &std::path::Path) -> Vec<(i64, i64)> {
    let connection = rusqlite::Connection::open(path).expect("independent connection");
    let mut statement = connection
        .prepare("SELECT pack_id, save_id FROM object_packs ORDER BY pack_id")
        .expect("pack query");
    let rows: Vec<(i64, i64)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("rows")
        .map(|row| row.expect("pack row"))
        .collect();
    rows
}

#[test]
fn a_step_never_leaves_the_watermark_behind_a_pack_it_wrote() {
    let temp = TempDir::new("watermark");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    let bytes = noise(900_000);
    let (objects, root, _) = construct_file(&bytes);

    assert_eq!(watermark(&path), 1, "a fresh Store hands out pack 1 first");
    let mut save = disabled(|s| store.begin_save(s.child("save"))).unwrap();
    for object in objects.finalized() {
        save.accept(object).unwrap();
        // Between steps the save holds no transaction, so the row is readable.
        let mark = watermark(&path);
        let highest = packs(&path)
            .last()
            .map(|(pack_id, _)| *pack_id)
            .unwrap_or(0);
        assert!(
            mark > highest,
            "watermark {mark} must be ahead of every committed pack id {highest}"
        );
    }
    disabled(|s| save.finish(s.child("finish"))).unwrap();
    let written = packs(&path);
    assert!(!written.is_empty(), "the save wrote at least one pack");
    assert!(
        watermark(&path) > written.last().unwrap().0,
        "the published watermark is ahead of every pack row"
    );
    assert_eq!(support::read_logical(&store, root), bytes);
}

#[test]
fn two_writers_interleaved_between_steps_never_share_a_pack_identifier() {
    let temp = TempDir::new("watermark-pair");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    let opened = disabled(|s| Store::open(&path, s.child("open"))).unwrap();
    let bytes = noise(900_000);
    let (objects, _, _) = construct_file(&bytes);

    let mut a = disabled(|s| store.begin_save(s.child("a"))).unwrap();
    let mut b = disabled(|s| opened.begin_save(s.child("b"))).unwrap();
    let payload = objects.finalized();

    // A steps first and allocates packs; B then begins between A's steps and
    // allocates its own; A steps again and must not be handed B's identifiers.
    for object in payload.iter().take(payload.len() / 3).cloned() {
        a.accept(object).unwrap();
    }
    for object in payload.iter().cloned() {
        b.accept(object).unwrap();
    }
    for object in payload.iter().skip(payload.len() / 3).cloned() {
        a.accept(object).unwrap();
    }
    disabled(|s| b.finish(s.child("finish-b"))).unwrap();
    disabled(|s| a.finish(s.child("finish-a"))).unwrap();

    let written = packs(&path);
    let mut seen = std::collections::BTreeSet::new();
    for (pack_id, _) in &written {
        assert!(seen.insert(*pack_id), "pack {pack_id} was written twice");
    }
    let a_saves: std::collections::BTreeSet<i64> = written
        .iter()
        .filter(|(pack_id, _)| *pack_id >= 1)
        .map(|(_, save_id)| *save_id)
        .collect();
    assert_eq!(a_saves.len(), 2, "two saves wrote packs: {written:?}");
    assert!(
        watermark(&path) > written.last().unwrap().0,
        "the watermark is ahead of every pack row"
    );
}
