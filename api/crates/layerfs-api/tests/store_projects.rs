//! One Store holds many projects (#506): an empty Store takes projects
//! added through its writer lane, a taken name is refused before anything
//! is written, a Store made by Init takes a second project, and a Store
//! without the acquisition tables is refused before any scan.
use layerfs_api::{projects, AddRequest, CreateRequest, InitError, InitRequest, ProjectApi};
use layerfs_content::{
    file::FileView,
    filesystem::{FilesystemRead, FilesystemRootId, LogicalPath},
};
use layerfs_history::{BranchId, HistoryCatalog, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const BINDING: &[u8] = b"store-first-proof";
const CURSOR: [u8; 32] = [41; 32];

fn store(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

fn source(directory: &Path, name: &str, body: &[u8]) -> PathBuf {
    let source = directory.join(format!("source-{name}"));
    fs::create_dir_all(source.join("src")).unwrap();
    fs::write(source.join("src/main.txt"), body).unwrap();
    fs::write(source.join("README"), name.as_bytes()).unwrap();
    source
}

fn add(
    path: &Path,
    source: PathBuf,
    name: &str,
    seed: u8,
) -> Result<layerfs_api::AddedProject, InitError> {
    let request = AddRequest {
        source,
        store: store(path),
        binding: BINDING.to_vec(),
        cursor_key: CURSOR,
        stack: LayerStackId::from_authority([seed; 16]),
        stack_name: HistoryName::new(name).unwrap(),
        branch: BranchId::from_authority([seed + 1; 16]),
        branch_name: HistoryName::new("main").unwrap(),
        scope_seed: [seed + 2; 32],
        deadline: Instant::now() + Duration::from_secs(20),
    };
    Timing::disabled("store.add_project", |timer| {
        ProjectApi::new().add_project(request, timer, Arc::new(Default::default()))
    })
    .0
    .map_err(|failure| failure.error)
}

/// The text of `path` on the Branch `name` of project `project`.
fn read(handles: &Handles, project: &str, file: &str) -> Vec<u8> {
    let stack = handles
        .history
        .layer_stack_named(&HistoryName::new(project).unwrap())
        .unwrap()
        .unwrap();
    let branch = handles
        .history
        .branch_named(stack.id, &HistoryName::new("main").unwrap())
        .unwrap()
        .unwrap();
    let snapshot = handles.history.branch_snapshot(branch.id).unwrap().unwrap();
    let storage = layerfs_storage::Storage::new(handles.storage.clone()).unwrap();
    let reader = storage.reader().unwrap();
    let mut tree = FilesystemRead::new(&reader, FilesystemRootId(snapshot.effective_root)).unwrap();
    let item = tree.resolve(&LogicalPath::new(file).unwrap()).unwrap();
    Timing::disabled("oracle", |timer| {
        let view = FileView::open(&reader, item.value.content_root, timer.child("open")).unwrap();
        let mut actual = Vec::new();
        view.read_range(&reader, 0..view.logical_len(), &mut actual, timer)
            .unwrap();
        Ok::<_, layerfs_content::ContentError>(actual)
    })
    .0
    .unwrap()
}

fn directory(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-store-projects-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    directory
}

#[test]
fn an_empty_store_holds_two_projects_and_refuses_a_taken_name() {
    let directory = directory("empty");
    let path = directory.join("store.sqlite");
    let sealed = ProjectApi::new()
        .create_store(CreateRequest {
            store: store(&path),
            policy: StoragePolicy::frozen_default(),
            catalog: catalog(),
        })
        .unwrap();
    assert_eq!(sealed.path, path);
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(!directory.join(format!("store.sqlite{suffix}")).exists());
    }
    let shop = add(&path, source(&directory, "shop", b"shop body"), "shop", 10).unwrap();
    let api = add(&path, source(&directory, "api", b"api body"), "api", 20).unwrap();
    assert_ne!(shop.initialized.root, api.initialized.root);
    assert_eq!(shop.branch.effective_root, shop.initialized.root);
    let taken = add(&path, source(&directory, "again", b"x"), "shop", 30);
    assert!(matches!(taken, Err(InitError::Exists)), "{taken:?}");

    let handles = Handles::open_writable(store(&path), BINDING, CURSOR).unwrap();
    let names: Vec<_> = projects(&handles.history)
        .unwrap()
        .into_iter()
        .map(|stack| stack.name.as_str().to_owned())
        .collect();
    assert_eq!(names, ["api", "shop"]);
    assert_eq!(read(&handles, "shop", "src/main.txt"), b"shop body");
    assert_eq!(read(&handles, "api", "src/main.txt"), b"api body");
    assert_eq!(read(&handles, "api", "README"), b"api");
    drop(handles);
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn a_store_made_by_init_takes_a_second_project() {
    let directory = directory("init");
    let path = directory.join("sealed.sqlite");
    let first = source(&directory, "web", b"web body");
    let sealed = Timing::disabled("sdk.init", |timer| {
        ProjectApi::new().init(
            InitRequest {
                source: first,
                store: store(&path),
                locator: "/store/store.sqlite".into(),
                policy: StoragePolicy::frozen_default(),
                catalog: catalog(),
                stack: LayerStackId::from_authority([1; 16]),
                stack_name: HistoryName::new("web").unwrap(),
                branch: BranchId::from_authority([2; 16]),
                branch_name: HistoryName::new("main").unwrap(),
                scope_seed: [3; 32],
                deadline: Instant::now() + Duration::from_secs(20),
            },
            timer,
        )
    })
    .0
    .unwrap();
    add(
        &sealed.store.path,
        source(&directory, "docs", b"docs body"),
        "docs",
        40,
    )
    .unwrap();
    let handles = Handles::open_writable(store(&path), BINDING, CURSOR).unwrap();
    let names: Vec<_> = projects(&handles.history)
        .unwrap()
        .into_iter()
        .map(|stack| stack.name.as_str().to_owned())
        .collect();
    assert_eq!(names, ["docs", "web"]);
    assert_eq!(read(&handles, "web", "src/main.txt"), b"web body");
    assert_eq!(read(&handles, "docs", "src/main.txt"), b"docs body");
    drop(handles);
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn a_store_without_acquisition_tables_is_refused_before_any_scan() {
    let directory = directory("absent");
    let path = directory.join("store.sqlite");
    // created without the acquisition tables (schema 16): no project can be added
    Handles::create(store(&path), StoragePolicy::frozen_default(), &catalog())
        .unwrap()
        .seal()
        .unwrap();
    let refused = add(&path, source(&directory, "late", b"x"), "late", 50);
    assert!(
        matches!(refused, Err(InitError::NoAcquisition)),
        "{refused:?}"
    );
    let handles = Handles::open_writable(store(&path), BINDING, CURSOR).unwrap();
    assert!(projects(&handles.history).unwrap().is_empty());
    drop(handles);
    fs::remove_dir_all(&directory).unwrap();
}
