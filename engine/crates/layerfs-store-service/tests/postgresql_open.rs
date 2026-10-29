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
