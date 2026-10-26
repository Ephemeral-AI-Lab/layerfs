//! #367: a thread's SQL totals hold only the work it ran itself. Threads
//! sharing one session split the session's counters between them.
mod support;
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{thread_work, Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{Storage, StoragePolicy};

fn leaf(serial: u64) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::InodeLeaf,
        InodeLeaf {
            subtree_bytes: LEAF_ROW_BYTES as u64,
            rows: vec![InodeLeafRow {
                serial,
                value: encode_inode_value(InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: ObjectId::for_bytes(&serial.to_le_bytes()),
                    metadata_root: layerfs_content::object::inode_leaf::InodeMetadata::Root(
                        ObjectId::for_bytes(b"thread-work-metadata"),
                    ),
                }),
            }],
        }
        .encode()
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn each_thread_counts_only_its_own_sql_work() {
    let temp = support::Temp::new("thread-work");
    let handles = Handles::create(
        PersistenceConfig::sqlite(temp.join("store.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"thread-work".to_vec(),
            cursor_key: [44; 32],
            incarnation: 1,
        },
    )
    .unwrap();
    let session_before = handles.storage.diagnostics().unwrap();
    let main_before = thread_work();
    let saves: Vec<_> = (1..=2u64)
        .map(|serial| {
            let provider = handles.storage.clone();
            std::thread::spawn(move || {
                let before = thread_work();
                let storage = Storage::new(provider).unwrap();
                let save = storage.begin_save().unwrap();
                save.accept(leaf(serial)).unwrap();
                save.finish().unwrap();
                thread_work().since(&before)
            })
        })
        .collect();
    let done: Vec<_> = saves.into_iter().map(|t| t.join().unwrap()).collect();
    let session = handles
        .storage
        .diagnostics()
        .unwrap()
        .since(&session_before);
    let main = thread_work().since(&main_before);
    assert_eq!(main.statements, 0, "the main thread ran no SQL");
    for work in &done {
        assert!(work.statements > 0 && work.write_commits > 0);
    }
    assert_eq!(done[0].statements + done[1].statements, session.statements);
    assert_eq!(done[0].vm_steps + done[1].vm_steps, session.vm_steps);
    assert_eq!(
        done[0].write_commits + done[1].write_commits,
        session.write_commits
    );
}
