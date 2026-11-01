//! #533 step 10b: Init into a PostgreSQL Store. The same folder added to a
//! new SQLite Store and to a new PostgreSQL schema gives the same root, and
//! the PostgreSQL Init leaves no acquisition rows behind.
//!
//! Needs a PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` (for example
//! `host=127.0.0.1 port=55433 user=postgres password=... dbname=postgres
//! sslmode=disable`). Without it the test reports NOT_RUN and passes nothing.
#![cfg(all(unix, feature = "postgresql"))]
use layerfs_content::{filesystem::root::FilesystemRootId, read_all, FilesystemRead, LogicalPath};
use layerfs_history::{BranchId, HistoryCatalog, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, PostgresSelection, SqlitePersistenceProfile,
};
use layerfs_project::store::{
    projects, AddRequest, AddedProject, CreateRequest, InitError, ProjectApi,
};
use layerfs_storage::{port::PersistenceError, Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const BINDING: &[u8] = b"init-postgresql";
const CURSOR: [u8; 32] = [61; 32];
const WIDE: usize = 1300;

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

/// A temporary directory and a PostgreSQL schema, both removed at the end.
struct Scratch {
    path: PathBuf,
    url: String,
    schema: String,
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
        if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
            let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema));
        }
    }
}

/// Nested, wide (placed in windows), aliased, linked and chunked content.
fn source(root: &Path) -> PathBuf {
    let source = root.join("source");
    for directory in ["wide/zz-dir", "empty", "a/b/c", "a/empty-too"] {
        fs::create_dir_all(source.join(directory)).unwrap();
    }
    for n in (0..WIDE).rev() {
        let body = vec![(n % 251) as u8; n % 97 + 1];
        fs::write(source.join(format!("wide/n{n:05}")), body).unwrap();
    }
    let large: Vec<u8> = (0..700_000u32)
        .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
        .collect();
    fs::write(source.join("a/large.bin"), large).unwrap();
    fs::write(source.join("a/b/c/file"), b"deep").unwrap();
    symlink("n00003", source.join("wide/aa-link")).unwrap();
    symlink("../missing", source.join("a/dangling")).unwrap();
    fs::hard_link(source.join("wide/n00007"), source.join("a/alias")).unwrap();
    fs::hard_link(source.join("wide/n00007"), source.join("a/b/alias-two")).unwrap();
    source
}

fn add(store: PersistenceConfig, source: &Path, name: &str) -> Result<AddedProject, InitError> {
    let request = AddRequest {
        source: source.to_path_buf(),
        store,
        binding: BINDING.to_vec(),
        cursor_key: CURSOR,
        stack: LayerStackId::from_authority([name.len() as u8; 16]),
        stack_name: HistoryName::new(name).unwrap(),
        branch: BranchId::from_authority([name.len() as u8 + 100; 16]),
        branch_name: HistoryName::new("main").unwrap(),
        scope_seed: [29; 32],
        deadline: Instant::now() + Duration::from_secs(60),
    };
    Timing::disabled("store.add_project", |timer| {
        ProjectApi::new().add_project(request, timer, Arc::new(Default::default()))
    })
    .0
    .map_err(|failure| failure.error)
}

fn read(storage: &Storage, root: layerfs_content::ObjectId, path: &str) -> Vec<u8> {
    let reader = storage.reader().unwrap();
    let mut view = FilesystemRead::new(&reader, FilesystemRootId(root)).unwrap();
    let inode = view
        .resolve(&LogicalPath::new(path).unwrap())
        .unwrap()
        .value;
    let mut body = Vec::new();
    Timing::disabled("read", |scope| {
        read_all(&reader, inode.content_root, &mut body, scope.child("file"))
    })
    .0
    .unwrap();
    body
}

#[test]
fn a_folder_inits_into_postgresql_with_the_root_sqlite_gives() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let scratch = Scratch {
        path: std::env::temp_dir().join(format!("lfs533-init-pg-{}", std::process::id())),
        url: url.clone(),
        schema: format!("lfs533_init_{}", std::process::id()),
    };
    let _ = fs::remove_dir_all(&scratch.path);
    fs::create_dir_all(&scratch.path).unwrap();
    let source = source(&scratch.path);
    let api = ProjectApi::new();

    let file = scratch.path.join("store.sqlite");
    let sqlite = || {
        PersistenceConfig::sqlite(&file).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
    };
    api.create_store(CreateRequest {
        store: sqlite(),
        policy: StoragePolicy::frozen_default(),
        catalog: catalog(),
    })
    .unwrap();
    let on_sqlite = add(sqlite(), &source, "app").unwrap();

    let selection = PostgresSelection::new(&url, &scratch.schema);
    let postgresql = || PersistenceConfig::postgresql(selection.clone());
    // No schema yet: an add is refused before anything is scanned.
    assert!(matches!(
        add(postgresql(), &source, "app"),
        Err(InitError::Persistence(_))
    ));
    api.create_postgresql_store(CreateRequest {
        store: postgresql(),
        policy: StoragePolicy::frozen_default(),
        catalog: catalog(),
    })
    .unwrap();
    let refused = api.create_postgresql_store(CreateRequest {
        store: postgresql(),
        policy: StoragePolicy::frozen_default(),
        catalog: catalog(),
    });
    assert!(matches!(
        refused,
        Err(InitError::Persistence(PersistenceError::Refused { .. }))
    ));
    let on_postgresql = add(postgresql(), &source, "app").unwrap();

    // The same canonical tree, Layer and Branch on both engines.
    assert_eq!(on_postgresql.initialized.root, on_sqlite.initialized.root);
    assert_eq!(
        on_postgresql.initialized.entries,
        on_sqlite.initialized.entries
    );
    assert_eq!(on_postgresql.initialized.entries as usize, WIDE + 14);
    assert_eq!(on_postgresql.branch, on_sqlite.branch);
    assert!(on_postgresql.sqlite_version.starts_with("postgresql "));
    let work = on_postgresql.initialized.namespace_work;
    assert_eq!((work.wide_directories, work.regular_aliases), (1, 2));

    // A taken name is refused before anything is written; another project is added.
    assert!(matches!(
        add(postgresql(), &source, "app"),
        Err(InitError::Exists)
    ));
    let other = scratch.path.join("other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("README"), b"other").unwrap();
    add(postgresql(), &other, "other").unwrap();

    // A fresh session reads both projects and the files back.
    let opened = PostgresHandles::open_read_only(&selection, BINDING, CURSOR).unwrap();
    let names: Vec<String> = projects(&opened.history)
        .unwrap()
        .into_iter()
        .map(|stack| stack.name.as_str().to_owned())
        .collect();
    assert_eq!(names, ["app", "other"]);
    let storage = Storage::new(opened.storage.clone()).unwrap();
    let root = on_postgresql.initialized.root;
    assert_eq!(
        read(&storage, root, "a/large.bin"),
        fs::read(source.join("a/large.bin")).unwrap()
    );
    assert_eq!(
        read(&storage, root, "a/b/alias-two"),
        fs::read(source.join("wide/n00007")).unwrap()
    );
    let head = opened
        .history
        .branch_named(
            on_postgresql.initialized.stack.id,
            &HistoryName::new("main").unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(head.base_layer, on_postgresql.initialized.stack.head_layer);

    // Every acquisition working row and operation record is gone.
    let mut client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    for table in ["init_operation", "init_entry", "init_host_file"] {
        let count: i64 = client
            .query_one(
                &format!("SELECT count(*) FROM {}.{table}", scratch.schema),
                &[],
            )
            .unwrap()
            .get(0);
        assert_eq!(count, 0, "{table}");
    }
}
