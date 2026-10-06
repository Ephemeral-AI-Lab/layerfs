//! Real native acquisition and initialized host assignment for the Docker proof.
use layerfs_bridge::native;
use layerfs_daemon::upstream::{ExpectedBinding, PersistenceBootstrap};
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId, WorkspaceId,
};
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_sdk::{Authorization, Config, Runtime, RuntimeError, RuntimeResult};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{
    ffi::OsStr,
    os::unix::{
        ffi::OsStrExt,
        fs::{symlink, MetadataExt},
    },
    path::PathBuf,
    time::{Duration, Instant},
};

pub const HOST_PRIVATE: [u8; 32] = [33; 32];
pub const CLIENT_PRIVATE: [u8; 32] = [1; 32];
#[path = "upstream_payload.rs"]
mod payload;
use payload::{FILES, RAW_TARGET};

struct Authority {
    peer: [u8; 32],
    workspace: WorkspaceId,
    branch: BranchId,
}
impl Authorization for Authority {
    fn workspace(
        &self,
        peer: [u8; 32],
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> RuntimeResult<()> {
        if (peer, workspace, branch) == (self.peer, self.workspace, self.branch) {
            Ok(())
        } else {
            Err(RuntimeError::Denied)
        }
    }
    fn objects(
        &self,
        peer: [u8; 32],
        workspace: WorkspaceId,
        branch: BranchId,
        _: &[layerfs_content::ObjectId],
    ) -> RuntimeResult<()> {
        self.workspace(peer, workspace, branch)
    }
}
pub struct Temp(pub PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("owned proof directory cleanup");
    }
}
pub struct Host {
    pub runtime: Runtime,
    pub expected: ExpectedBinding,
    pub bootstrap: PersistenceBootstrap,
    pub native_metadata: String,
    pub native_raw_metadata: String,
    // Declared last, so initialized provider owners drop before test cleanup.
    pub temp: Temp,
}
impl Host {
    pub fn new(profile: SqlitePersistenceProfile) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/r4-upstream-host")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&path).unwrap();
        let source = path.join("native-source");
        std::fs::create_dir_all(source.join(".git")).unwrap();
        std::fs::create_dir_all(source.join(".cache/dependency")).unwrap();
        std::fs::create_dir_all(source.join("output")).unwrap();
        for (name, bytes) in FILES.iter().filter(|(name, _)| *name != "shared-alias.bin") {
            std::fs::write(source.join(name), bytes).unwrap();
        }
        std::fs::hard_link(source.join("shared.bin"), source.join("shared-alias.bin")).unwrap();
        symlink(OsStr::from_bytes(RAW_TARGET), source.join("raw-link")).unwrap();
        let names = [
            ".",
            ".git",
            ".cache",
            ".cache/dependency",
            "output",
            "raw-link",
        ]
        .into_iter()
        .chain(FILES.iter().map(|(name, _)| *name));
        let mut native_metadata = String::new();
        let mut native_raw_metadata = String::new();
        for name in names {
            let metadata = std::fs::symlink_metadata(source.join(name)).unwrap();
            let mode = if metadata.file_type().is_symlink() {
                // The public portable contract fixes symbolic-link mode to0777;
                // retain raw native bits separately, without deriving the oracle
                // from the imported candidate or changing any timestamp.
                0o777
            } else {
                metadata.mode() & 0o7777
            };
            native_metadata.push_str(&format!(
                "{name}\t{}\t{}\t{}\n",
                mode,
                metadata.mtime(),
                metadata.mtime_nsec()
            ));
            native_raw_metadata.push_str(&format!(
                "{name}\t{}\t{}\t{}\n",
                metadata.mode() & 0o7777,
                metadata.mtime(),
                metadata.mtime_nsec()
            ));
        }
        let policy = StoragePolicy::frozen_default();
        let handles = Handles::create(
            PersistenceConfig::sqlite(path.join("global-store"))
                .with_sqlite_profile(profile)
                .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
            policy,
            &HistoryCatalogConfig {
                binding_key: b"r4-upstream-full-root".to_vec(),
                cursor_key: [9; 32],
                incarnation: 1,
            },
        )
        .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let initialized = Timing::disabled("real native acquisition", |scope| {
            layerfs_project::init(
                &storage,
                &handles.history,
                layerfs_project::InitRequest {
                    source: &source,
                    acquisition: &handles.acquisition,
                    stack: LayerStackId::from_authority([5; 16]),
                    name: HistoryName::new("r4-root").unwrap(),
                    scope_seed: [8; 32],
                    deadline: Instant::now() + Duration::from_secs(10),
                },
                scope,
            )
        })
        .0
        .unwrap();
        let branch = BranchId::from_authority([6; 16]);
        handles
            .history
            .fork(&ForkRequest {
                stack: initialized.stack.id,
                branch,
                name: HistoryName::new("main").unwrap(),
                source: ForkSource::Layer(initialized.stack.head_layer),
            })
            .unwrap();
        let workspace = WorkspaceId::from_authority([7; 32]).unwrap();
        let expected = ExpectedBinding {
            host_peer: native::public_key(&HOST_PRIVATE).unwrap(),
            local_peer: native::public_key(&CLIENT_PRIVATE).unwrap(),
            runtime: [7; 32],
            catalog: handles.history.catalog_id(),
            provider_incarnation: handles.history.incarnation(),
            workspace,
            snapshot: handles.history.branch_snapshot(branch).unwrap().unwrap(),
            root_serial: initialized.root_serial,
            policy,
            persistence: profile,
        };
        let bootstrap = PersistenceBootstrap {
            host_peer: expected.host_peer,
            runtime: expected.runtime,
            catalog: expected.catalog,
            provider_incarnation: expected.provider_incarnation,
            profile: handles.profile().persistence,
        };
        assert_eq!(bootstrap.profile, profile);
        // Root demand can only use the saved Store: source removal precedes the
        // consumer process, root binding and every Linux operation.
        std::fs::remove_dir_all(&source).unwrap();
        drop(storage);
        let runtime = Runtime::new(
            handles,
            Config {
                incarnation: expected.runtime,
                save_slots: 4,
            },
            Box::new(Authority {
                peer: expected.local_peer,
                workspace,
                branch,
            }),
        )
        .unwrap();
        Self {
            runtime,
            expected,
            bootstrap,
            native_metadata,
            native_raw_metadata,
            temp: Temp(path),
        }
    }
}
