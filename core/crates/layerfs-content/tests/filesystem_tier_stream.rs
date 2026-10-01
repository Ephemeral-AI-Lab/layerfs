//! Independent event folds and real immutable-file failures for compatibility streams.
#[path = "support/ordering_observer.rs"]
mod ordering_observer;
mod support;
use layerfs_content::filesystem::inode::read::InodeTable;
use layerfs_content::filesystem::references::{
    PendingState, ReferenceReducer, Row, RunStore, ROW_BYTES,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::ContentError;
use ordering_observer::ObservedBacking;
use std::collections::BTreeMap;
use std::io::{Seek, SeekFrom, Write};
use support::filesystem::{name_of, synthetic, value, Session, TempDir, TreeStore};

fn supplied(serial: u64, generation: usize) -> InodeValue {
    let mut value = value(
        InodeKind::RegularFile,
        synthetic(&format!("tier/content/{serial}/{generation}")),
        synthetic(&format!("tier/meta/{serial}/{generation}")),
    );
    value.namespace_ref_count = 999;
    value
}

#[test]
fn final_and_touched_stream_original_tiers_with_independent_complete_event_rows() {
    let mut base = Session::new(1).unwrap();
    let serials = (2..=8).collect::<Vec<_>>();
    let directories = [layerfs_content::filesystem::DirectoryUpdate {
        parent: 1,
        changes: serials
            .iter()
            .map(|serial| (name_of(&format!("f{serial}")), Some(*serial)))
            .collect(),
    }];
    let values = serials
        .iter()
        .map(|serial| layerfs_content::filesystem::InodeUpdate {
            serial: *serial,
            value: supplied(*serial, 0),
        })
        .collect::<Vec<_>>();
    base.apply(&directories, &values, &serials).unwrap();
    let table = InodeTable {
        root: base.value.inode_table(),
        root_serial: 1,
    };
    for batch in [1, 64] {
        let temp = TempDir::new("tier-event-stream");
        let mut backing = ObservedBacking::new(temp.path(), 1024 * 1024);
        let observer = backing.observer();
        let mut reducer = ReferenceReducer::new(1, Some(&mut backing), 96, 1024 * 1024);
        reducer.declare_new(40).unwrap();
        // Expected rows are folded from the original events, never from a
        // candidate lookup, touched scan, merge or final stream.
        let mut expected: BTreeMap<u64, (i64, InodeValue)> = BTreeMap::new();
        for (generation, serial) in [2, 3, 2, 4, 40, 5, 2, 6, 3, 7, 40, 8, 4, 5]
            .into_iter()
            .enumerate()
        {
            let delta = if generation % 4 == 3 && serial != 40 {
                -1
            } else {
                1
            };
            let typed = supplied(serial, generation + 1);
            let prior = expected.get(&serial).map_or(0, |row| row.0);
            expected.insert(serial, (prior + delta, typed));
            if delta > 0 {
                reducer.note_retained_binding(serial).unwrap();
            } else {
                reducer.note_removed_binding(serial).unwrap();
            }
            reducer.note_value(serial, typed).unwrap();
        }
        assert!(std::fs::read_dir(temp.path()).unwrap().count() >= 2);
        let before = observer.counts();
        let touched = reducer.touched_serials(batch).unwrap();
        let expected_touched = expected
            .iter()
            .map(|(&serial, &(tally, value))| {
                (
                    serial,
                    if serial == 40 {
                        PendingState::New {
                            count: tally as u64,
                            value: Some(value),
                        }
                    } else {
                        PendingState::Existing {
                            delta: tally,
                            value: Some(value),
                        }
                    },
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(touched, expected_touched);
        let mut final_rows = reducer.finish(&base.store, table, batch, 1).unwrap();
        let mut actual = Vec::new();
        while let Some(row) = final_rows.next_change().unwrap() {
            actual.push((row.serial, row.value));
        }
        let expected_final = expected
            .iter()
            .map(|(&serial, &(delta, typed))| {
                let count = delta + i64::from(serial != 40);
                (
                    serial,
                    (count > 0).then_some(InodeValue {
                        namespace_ref_count: count.max(0) as u64,
                        ..typed
                    }),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected_final);
        let ended = observer.counts();
        assert!(ended.reads > before.reads);
        assert_eq!(ended.maximum_request, ROW_BYTES);
        assert_eq!(ended.creates, before.creates, "no consolidation output");
        assert_eq!(ended.appends, before.appends, "immutable inputs");
        assert_eq!(ended.flushes, before.flushes);
        let eof_reads = ended.reads;
        assert!(final_rows.next_change().unwrap().is_none());
        assert_eq!(observer.counts().reads, eof_reads, "known EOF stays closed");
        drop(final_rows);
        reducer.release().unwrap();
        drop(reducer);
        assert_eq!(observer.counts().releases, 1);
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

fn largest_run(directory: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .max_by_key(|path| std::fs::metadata(path).unwrap().len())
        .unwrap()
}

#[test]
fn hidden_older_duplicate_corruption_is_not_skipped_or_replaced() {
    let temp = TempDir::new("tier-hidden-corrupt");
    let mut backing = ObservedBacking::new(temp.path(), 1024 * 1024);
    let observer = backing.observer();
    let mut store = RunStore::new(Some(&mut backing), 96, 1024 * 1024);
    for (generation, keys) in [[2, 6], [3, 7], [2, 8]].into_iter().enumerate() {
        let rows = keys
            .into_iter()
            .map(|serial| {
                (
                    serial,
                    Row::Count {
                        serial,
                        count: generation as u64 + 1,
                        value: Some(supplied(serial, generation)),
                    },
                )
            })
            .collect();
        store.spill(&rows).unwrap();
    }
    let old = largest_run(temp.path());
    assert_eq!(std::fs::metadata(&old).unwrap().len(), 4 * ROW_BYTES as u64);
    let mut file = std::fs::OpenOptions::new().write(true).open(old).unwrap();
    file.seek(SeekFrom::Start(95)).unwrap();
    file.write_all(&[1]).unwrap();
    drop(file);
    let before = observer.counts();
    let mut emitted = 0;
    assert!(matches!(
        store.visit_newest_first(|_| {
            emitted += 1;
            Ok(true)
        }),
        Err(ContentError::InvalidOrderingRecord("reserved"))
    ));
    assert_eq!(emitted, 0);
    assert_eq!(observer.counts().creates, before.creates);
    assert_eq!(observer.counts().appends, before.appends);
    store.release().unwrap();
    drop(store);
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn final_owned_input_truncation_is_terminal_without_reread_or_output_run() {
    let temp = TempDir::new("tier-final-truncated");
    let mut backing = ObservedBacking::new(temp.path(), 1024 * 1024);
    let observer = backing.observer();
    let mut reducer = ReferenceReducer::new(1, Some(&mut backing), 96, 1024 * 1024);
    for serial in 2..=14 {
        reducer.declare_new(serial).unwrap();
        reducer.note_retained_binding(serial).unwrap();
        reducer.note_value(serial, supplied(serial, 0)).unwrap();
    }
    let store = TreeStore::new();
    let table = InodeTable {
        root: synthetic("unused/table"),
        root_serial: 1,
    };
    let before = observer.counts();
    let mut rows = reducer.finish(&store, table, 1, 1).unwrap();
    assert!(
        std::fs::read_dir(temp.path()).unwrap().count() >= 2,
        "finish retains actual immutable input handles"
    );
    let path = largest_run(temp.path());
    let bytes = std::fs::metadata(&path).unwrap().len();
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(bytes - 1)
        .unwrap();
    let error = loop {
        match rows.next_change() {
            Ok(Some(_)) => {}
            Ok(None) => panic!("truncated input accepted"),
            Err(error) => break error,
        }
    };
    assert!(matches!(error, ContentError::Io));
    let reads = observer.counts().reads;
    assert_eq!(rows.next_change().unwrap_err(), error);
    assert_eq!(observer.counts().reads, reads, "failed input never retried");
    assert_eq!(observer.counts().creates, before.creates);
    assert_eq!(observer.counts().appends, before.appends);
    drop(rows);
    reducer.release().unwrap();
    drop(reducer);
    assert_eq!(observer.counts().releases, 1);
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
