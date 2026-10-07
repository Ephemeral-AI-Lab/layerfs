//! External authenticated endpoints and complete small-root installation oracle.
use layerfs_bridge::native::{self, Connection};
use layerfs_daemon::{
    install_types::InstalledStore, store::BindRequest, Command, Owner, OwnerConfig, Response,
};
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId, WorkspaceId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_sdk::{initialize, InitRequest, SealedProject};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const CLIENT_PRIVATE: [u8; 32] = [51; 32];
pub const SERVER_PRIVATE: [u8; 32] = [52; 32];
pub const FILE_BYTES: usize = 150_017;
pub struct Fixture {
    pub directory: PathBuf,
    pub project: SealedProject,
}
impl Fixture {
    pub fn new(label: &str, locator: Option<&str>) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-native-install-{}-{label}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let source = directory.join("source");
        fs::create_dir(&source).unwrap();
        fs::create_dir(source.join(".git")).unwrap();
        fs::write(
            source.join("file"),
            (0..FILE_BYTES).map(|n| (n % 251) as u8).collect::<Vec<_>>(),
        )
        .unwrap();
        fs::hard_link(source.join("file"), source.join("alias")).unwrap();
        fs::write(source.join(".git/index"), b"complete index").unwrap();
        use std::os::unix::ffi::OsStrExt;
        std::os::unix::fs::symlink(
            std::ffi::OsStr::from_bytes(b"../opaque-\xff"),
            source.join(".git/link"),
        )
        .unwrap();
        let project = Timing::disabled("native.install.init", |timer| {
            initialize(
                InitRequest {
                    source,
                    store: PersistenceConfig::sqlite(directory.join("sealed.sqlite"))
                        .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
                    locator: locator
                        .map(str::to_owned)
                        .unwrap_or_else(|| directory.join("store.sqlite").to_str().unwrap().into()),
                    policy: StoragePolicy::frozen_default(),
                    catalog: HistoryCatalogConfig {
                        binding_key: b"native-install".to_vec(),
                        cursor_key: [53; 32],
                        incarnation: 1,
                    },
                    stack: LayerStackId::from_authority([54; 16]),
                    stack_name: HistoryName::new("project").unwrap(),
                    branch: BranchId::from_authority([55; 16]),
                    branch_name: HistoryName::new("main").unwrap(),
                    scope_seed: [56; 32],
                    deadline: Instant::now() + Duration::from_secs(5),
                },
                timer,
            )
        })
        .0
        .unwrap();
        // Native source is absent for every daemon oracle, even on the same host.
        fs::remove_dir_all(directory.join("source")).unwrap();
        Self { directory, project }
    }
    pub fn cleanup(self) {
        fs::remove_dir_all(self.directory).unwrap();
    }
}
pub fn socket(stream: TcpStream) -> TcpStream {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
}
pub struct Worker<T>(Option<JoinHandle<T>>);
impl<T> Worker<T> {
    pub fn join(mut self) -> T {
        self.0.take().unwrap().join().unwrap()
    }
}
impl<T> Drop for Worker<T> {
    fn drop(&mut self) {
        if let Some(thread) = self.0.take() {
            let _ = thread.join();
        }
    }
}
pub fn pair<T: Send + 'static>(
    serve: impl FnOnce(&mut Connection) -> T + Send + 'static,
) -> (Connection, Worker<T>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = socket(
        TcpStream::connect_timeout(&listener.local_addr().unwrap(), Duration::from_secs(2))
            .unwrap(),
    );
    let (server, _) = listener.accept().unwrap();
    let server = socket(server);
    let worker = Worker(Some(thread::spawn(move || {
        let mut channel = native::accept(
            server,
            &SERVER_PRIVATE,
            native::public_key(&CLIENT_PRIVATE).unwrap(),
        )
        .unwrap();
        serve(&mut channel)
    })));
    let channel = native::initiate(
        client,
        &CLIENT_PRIVATE,
        native::public_key(&SERVER_PRIVATE).unwrap(),
    )
    .unwrap();
    (channel, worker)
}
pub fn oracle(installed: &InstalledStore, overlay: &Path) {
    use layerfs_content::{
        file::FileView,
        filesystem::{FilesystemRead, FilesystemRootId, LogicalPath},
    };
    let owner = Owner::start(
        overlay,
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let bound = installed
        .opened
        .store
        .bind(
            owner.client(),
            BindRequest {
                branch: BranchId::from_bytes(installed.manifest.branch).unwrap(),
                workspace: WorkspaceId::from_authority([58; 32]).unwrap(),
            },
        )
        .unwrap();
    {
        let operation = bound.workspace.operation().unwrap();
        let root = bound.workspace.snapshot().unwrap().effective_root;
        assert_eq!(root.to_bytes(), installed.manifest.root);
        let mut tree = FilesystemRead::new(operation.client(), FilesystemRootId(root)).unwrap();
        let root_names = tree.list(&LogicalPath::root(), None, 32, 4096).unwrap();
        assert_eq!(root_names.entries.len(), 3);
        let git_names = tree
            .list(&LogicalPath::new(".git").unwrap(), None, 32, 4096)
            .unwrap();
        assert_eq!(git_names.entries.len(), 2);
        let file = tree.resolve(&LogicalPath::new("file").unwrap()).unwrap();
        assert_eq!(
            file,
            tree.resolve(&LogicalPath::new("alias").unwrap()).unwrap()
        );
        for name in ["file", "alias", ".git/index"] {
            let resolved = tree.resolve(&LogicalPath::new(name).unwrap()).unwrap();
            Timing::disabled("installed.oracle", |timer| {
                let view = FileView::open(
                    operation.client(),
                    resolved.value.content_root,
                    timer.child("open"),
                )?;
                let expected = if name == ".git/index" { 14 } else { FILE_BYTES };
                assert_eq!(view.logical_len(), expected as u64);
                for start in (0..expected).step_by(16 * 1024) {
                    let end = (start + 16 * 1024).min(expected);
                    let mut actual = Vec::new();
                    view.read_range(
                        operation.client(),
                        start as u64..end as u64,
                        &mut actual,
                        timer,
                    )?;
                    if name == ".git/index" {
                        assert_eq!(actual, b"complete index");
                    } else {
                        assert_eq!(
                            actual,
                            (start..end).map(|n| (n % 251) as u8).collect::<Vec<_>>()
                        );
                    }
                }
                Ok::<(), layerfs_content::ContentError>(())
            })
            .0
            .unwrap();
        }
        assert_eq!(
            tree.readlink(&LogicalPath::new(".git/link").unwrap())
                .unwrap()
                .as_bytes(),
            b"../opaque-\xff"
        );
        assert!(operation.ports().failure().unwrap().is_none());
    }
    let closed = owner
        .client()
        .try_submit(Some(bound.workspace.route()), Command::Close)
        .unwrap()
        .wait()
        .unwrap();
    assert!(matches!(closed.result(), Ok(Response::Done)));
    drop((closed, bound));
    owner.stop().unwrap();
    println!("INSTALL_ROOT_ORACLE paths=6 full_regular_bytes={} hardlink=true raw_symlink=true host_sqlite={} daemon_sqlite={} work={:?}",2*FILE_BYTES+14,installed.manifest.host_sqlite,installed.manifest.daemon_sqlite.as_deref().unwrap(),installed.work);
}
