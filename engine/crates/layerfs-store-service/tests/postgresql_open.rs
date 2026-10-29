//! #506: the Store service opens a PostgreSQL Store through the same path as
//! a SQLite one. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#![cfg(feature = "postgresql")]
use layerfs_history::records::StackInitialization;
use layerfs_history::{HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{
    BackendSelection, Handles, PersistenceConfig, PostgresHandles, PostgresSelection,
    SqlitePersistenceProfile,
};
use layerfs_storage::{ReservationBlocks, StoragePolicy};
use layerfs_store_service::bootstrap::open_store_observed;

const BINDING: &[u8] = b"store-service-postgresql";
const CURSOR: [u8; 32] = [41; 32];

#[test]
fn a_postgresql_store_opens_through_the_store_open_path() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lfs506-open-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sqlite = |path| {
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
    };
    let created = Handles::create(
        sqlite(dir.join("store")),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 2,
        },
    )
    .unwrap();
    let id = |tag: u8| layerfs_content::ObjectId::for_bytes(&[tag]);
    let stack = LayerStackId::from_authority([5; 16]);
    layerfs_history::HistoryCatalog::initialize_layerstack(
        &created.history,
        &StackInitialization {
            logical: 0,
            stack,
            name: HistoryName::new("main").unwrap(),
            scope: id(1),
            profile: id(2),
            genesis_root: id(3),
        },
    )
    .unwrap();
    let copy = created.snapshot(&dir.join("copy")).unwrap();
    let schema = format!("lfs506_open_{}", std::process::id());
    let selection = PostgresSelection::new(&url, &schema);
    let result = (|| {
        PostgresHandles::import(sqlite(copy.path.clone()), &selection, BINDING, CURSOR).unwrap();
        let mut config = sqlite(dir.join("unused"));
        config.backend = BackendSelection::Postgres(selection.clone());
        let opened = open_store_observed(
            config,
            BINDING,
            CURSOR,
            4,
            64 * 1024,
            ReservationBlocks::default(),
        )
        .unwrap();
        assert!(opened.sqlite_version.starts_with("postgresql "));
        assert_eq!(
            opened
                .store
                .history()
                .layer_stack(stack)
                .unwrap()
                .unwrap()
                .id,
            stack
        );
        opened.store.producer().unwrap();
        assert!(opened.snapshot(&dir.join("refused")).is_err());
        assert!(opened.diagnostics().unwrap().writer.transactions > 0);
        merge_records_past_resident(&opened.store);
    })
    .pipe_unwind();
    if let Ok(mut client) = postgres_cleanup(&url) {
        let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

/// #506 d: a merge's records on PostgreSQL, driven past the resident bound
/// so they reach the writer connection's temporary tables: guarded writes,
/// buffered and stored reads, key windows, deletions, and removal at the end.
fn merge_records_past_resident(store: &layerfs_store_service::store::Store) {
    use layerfs_content::{
        EditRecordApply, EditRecordChange, EditRecordExpected, EditRecordKey, IndexedEditBacking,
    };
    let key = |n: u32| {
        let mut key = [0; 32];
        key[..4].copy_from_slice(&n.to_be_bytes());
        EditRecordKey { kind: 7, key }
    };
    let value = |n: u32| vec![(n % 251) as u8; 1000];
    // 160 batches of 60 records of 1000 bytes: past the 8 MiB resident bound
    let total = 160 * 60;
    let (checked, failure) = store
        .merge_records(|records| {
            for batch in 0..160u32 {
                let changes = (batch * 60..batch * 60 + 60)
                    .map(|n| EditRecordChange {
                        key: key(n),
                        expected: EditRecordExpected::Missing,
                        value: Some(value(n)),
                    })
                    .collect();
                assert_eq!(records.apply(changes).unwrap(), EditRecordApply::Applied);
            }
            // stored reads, in order, across the buffer and the tables
            let asked: Vec<_> = (0..64).map(|n| key(n * 150)).collect();
            let got = records.get_many(&asked).unwrap();
            assert!(got
                .iter()
                .enumerate()
                .all(|(i, v)| v.as_deref() == Some(value(i as u32 * 150).as_slice())));
            assert_eq!(records.get(key(total + 5)).unwrap(), None);
            // a guard that does not hold applies nothing
            let refused = records
                .apply(vec![EditRecordChange {
                    key: key(3),
                    expected: EditRecordExpected::Missing,
                    value: Some(vec![1]),
                }])
                .unwrap();
            assert!(matches!(
                refused,
                EditRecordApply::NotApplied { index: 0, .. }
            ));
            // a deletion and an update, then key windows from the tables
            let changed = records
                .apply(vec![
                    EditRecordChange {
                        key: key(0),
                        expected: EditRecordExpected::ExactBytes(value(0)),
                        value: None,
                    },
                    EditRecordChange {
                        key: key(1),
                        expected: EditRecordExpected::ExactBytes(value(1)),
                        value: Some(vec![9; 3]),
                    },
                ])
                .unwrap();
            assert_eq!(changed, EditRecordApply::Applied);
            assert_eq!(records.get(key(0)).unwrap(), None);
            assert_eq!(records.get(key(1)).unwrap(), Some(vec![9; 3]));
            let mut seen = 0;
            let mut after = None;
            loop {
                let window = records.keys_after(7, after).unwrap();
                if window.is_empty() {
                    break;
                }
                seen += window.len();
                after = window.last().copied();
            }
            assert_eq!(seen as u32, total - 1);
            let first = records.first_keys(7, Some(key(1).key)).unwrap();
            assert_eq!(first.first(), Some(&key(2).key));
            assert!(records.keys_after(8, None).unwrap().is_empty());
            records.work()
        })
        .unwrap();
    assert!(failure.is_none(), "{failure:?}");
    assert!(
        checked.flushes > 0,
        "the records reached the tables: {checked:?}"
    );
}

trait Unwind {
    fn pipe_unwind(self) -> std::thread::Result<()>;
}
impl<F: FnOnce() + std::panic::UnwindSafe> Unwind for F {
    fn pipe_unwind(self) -> std::thread::Result<()> {
        std::panic::catch_unwind(self)
    }
}

fn postgres_cleanup(url: &str) -> Result<postgres::Client, postgres::Error> {
    postgres::Client::connect(url, postgres::NoTls)
}
