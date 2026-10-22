//! A committed Branch from a host tree, bound to one Workspace and one engine
//! kernel mount, with no kernel connection: the NFS export is the only client.
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace, Store},
    Mounted, Owner, OwnerClient, OwnerConfig,
};
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId, WorkspaceId,
};
use layerfs_nfs::{ExportConfig, Released, Server};
use layerfs_overlay::{KernelMount, ProfileConfig};
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const BINDING: &[u8] = b"layerfs-nfs-rig";
const CURSOR: [u8; 32] = [71; 32];
const CACHE: usize = 32 * 1024 * 1024;

pub struct Rig {
    pub directory: PathBuf,
    store: Arc<Store>,
    owner: Owner,
    client: OwnerClient,
    pub bound: Arc<BoundWorkspace>,
    pub mount: KernelMount,
    pub branch: BranchId,
    /// Paths below the root that Init acquired.
    pub entries: u64,
    /// The reference bound exports of this rig are configured with.
    pub lookups_held: usize,
}
impl Rig {
    /// Initializes a Store from `source`, publishes Branch `main` at that
    /// root, binds one Workspace and creates one engine kernel mount.
    /// `directory` must not exist; everything the rig writes goes there.
    pub fn new(source: &Path, directory: &Path, timeout: Duration) -> Self {
        std::fs::create_dir_all(directory).unwrap();
        let sealed = directory.join("sealed.sqlite");
        let config = PersistenceConfig::sqlite(&sealed)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
        let handles = Handles::create(
            config.clone(),
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: BINDING.to_vec(),
                cursor_key: CURSOR,
                incarnation: 1,
            },
        )
        .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let initialized = Timing::disabled("nfs.rig.init", |timing| {
            init(
                &storage,
                &handles.history,
                InitRequest {
                    source,
                    acquisition: &handles.acquisition,
                    stack: LayerStackId::from_authority([72; 16]),
                    name: HistoryName::new("nfs").unwrap(),
                    scope_seed: [73; 32],
                    deadline: Instant::now() + timeout,
                },
                timing,
            )
        })
        .0
        .unwrap();
        let branch = BranchId::from_authority([74; 16]);
        handles
            .history
            .fork(&ForkRequest {
                stack: initialized.stack.id,
                branch,
                name: HistoryName::new("main").unwrap(),
                source: ForkSource::Layer(initialized.stack.head_layer),
            })
            .unwrap();
        drop(storage);
        let sealed = handles.seal().unwrap();
        let store = open_store(
            PersistenceConfig {
                path: sealed.path.clone(),
                ..config
            },
            BINDING,
            CURSOR,
            4,
            CACHE,
            Default::default(),
        )
        .unwrap();
        let owner = Owner::start(
            &directory.join("overlay"),
            ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let client = owner.client();
        let bound = Arc::new(
            store
                .bind(
                    client.clone(),
                    BindRequest {
                        branch,
                        workspace: WorkspaceId::from_authority([75; 32]).unwrap(),
                    },
                )
                .unwrap()
                .workspace,
        );
        let root = bound
            .operation()
            .unwrap()
            .workspace()
            .base()
            .unwrap()
            .root()
            .root_inode()
            .serial();
        let mount = client.kernel_mount(bound.route(), root).unwrap();
        Self {
            directory: directory.to_owned(),
            store,
            owner,
            client,
            bound,
            mount,
            branch,
            entries: initialized.entries,
            lookups_held: layerfs_nfs::kernel::LOOKUPS_HELD,
        }
    }
    /// Serves this rig's engine mount read-only on `listener`.
    pub fn serve(&self, listener: TcpListener, uid: u32, gid: u32) -> Server {
        self.export(listener, uid, gid, true)
    }
    /// Serves this rig's engine mount on `listener`, writable or not.
    pub fn export(&self, listener: TcpListener, uid: u32, gid: u32, read_only: bool) -> Server {
        self.export_services(
            listener,
            Arc::new(Mounted(self.bound.clone())),
            uid,
            gid,
            read_only,
        )
    }
    /// Serves this rig's engine mount over `services` (a wrapper of the
    /// rig's own ports, for example).
    pub fn export_services(
        &self,
        listener: TcpListener,
        services: Arc<dyn layerfs_fuse::ports::MountServices>,
        uid: u32,
        gid: u32,
        read_only: bool,
    ) -> Server {
        Server::start(
            listener,
            services,
            self.mount,
            ExportConfig {
                path: "/".into(),
                uid,
                gid,
                read_only,
                refuse_apple_double: true,
                lookups_held: self.lookups_held,
            },
        )
        .unwrap()
    }
    /// A daemon control Service over this rig's Store and overlay owner whose
    /// Attach serves NFS (the native route's mount entry).
    pub fn service(
        &self,
        export: Arc<layerfs_daemon::control::ExportServing>,
    ) -> Arc<layerfs_daemon::control::Service> {
        Arc::new(layerfs_daemon::control::Service::with_export(
            self.store.clone(),
            &self.owner,
            export,
        ))
    }
    /// The product Commit of everything the Workspace published locally.
    pub fn commit(&self) -> layerfs_daemon::store::CommitSuccess {
        self.bound
            .commit_captured(&layerfs_daemon::store::CommitOptions::default())
            .unwrap_or_else(|failure| panic!("Commit: {:?}", failure.error))
    }
    /// A second Workspace bound to Branch `main` as it is now, with its own
    /// engine mount: an independent view of what was committed.
    pub fn view(&self, workspace: [u8; 32]) -> (Arc<BoundWorkspace>, KernelMount) {
        let bound = Arc::new(
            self.store
                .bind(
                    self.client.clone(),
                    BindRequest {
                        branch: self.branch,
                        workspace: WorkspaceId::from_authority(workspace).unwrap(),
                    },
                )
                .unwrap()
                .workspace,
        );
        let root = bound
            .operation()
            .unwrap()
            .workspace()
            .base()
            .unwrap()
            .root()
            .root_inode()
            .serial();
        let mount = self.client.kernel_mount(bound.route(), root).unwrap();
        (bound, mount)
    }
    /// Serves a view read-only.
    pub fn serve_view(
        &self,
        listener: TcpListener,
        view: &(Arc<BoundWorkspace>, KernelMount),
        uid: u32,
        gid: u32,
    ) -> Server {
        Server::start(
            listener,
            Arc::new(Mounted(view.0.clone())),
            view.1,
            ExportConfig {
                path: "/".into(),
                uid,
                gid,
                read_only: true,
                refuse_apple_double: true,
                lookups_held: self.lookups_held,
            },
        )
        .unwrap()
    }
    /// Revokes and closes a view.
    pub fn close_view(&self, view: (Arc<BoundWorkspace>, KernelMount)) {
        self.client.kernel_revoke(view.0.route(), view.1).unwrap();
        view.0.close().unwrap();
    }
    /// A second engine mount of the same Workspace, as a restarted export
    /// would create: handles of the first are stale under it.
    pub fn remount(&mut self) {
        self.client
            .kernel_revoke(self.bound.route(), self.mount)
            .unwrap();
        self.mount = self
            .client
            .kernel_mount(self.bound.route(), self.mount.root_serial())
            .unwrap();
    }
    /// Revokes the engine mount, closes the Workspace and stops the owner.
    /// Returns the Store readers still open or quarantined afterwards.
    pub fn stop(self, served: Released) -> (Released, u64) {
        let Self {
            directory,
            store,
            owner,
            client,
            bound,
            mount,
            ..
        } = self;
        client.kernel_revoke(bound.route(), mount).unwrap();
        bound.close().unwrap();
        drop(bound);
        // A closed Workspace keeps no Store reader and none was quarantined;
        // the Store's own control reader stays open.
        let work = store.read_work();
        assert!(!work.poisoned && work.opening_failures == 0, "{work:?}");
        let outstanding = work.readers.saturating_sub(1) + work.quarantined;
        owner.stop().unwrap();
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
        (served, outstanding as u64)
    }
}
