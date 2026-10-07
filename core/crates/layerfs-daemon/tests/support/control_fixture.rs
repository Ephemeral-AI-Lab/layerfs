//! Real installed Store, engine mutations and a direct Content producer for control proofs.
use super::support;
use layerfs_bridge::control::WorkspaceToken;
use layerfs_content::{
    filesystem::{
        FilesystemInput, FilesystemObjects, FilesystemRead, FilesystemResources, FilesystemRootId,
        InodeUpdate, PathName,
    },
    ConstructionPolicy,
};
use layerfs_daemon::{
    control::Service,
    install::receive_install,
    install_types::{InstalledStore, StoreSettings},
    store::CommitError,
    Command, Owner, OwnerConfig, Response,
};
use layerfs_history::BranchSnapshot;
use layerfs_storage::Save;
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{Operation, Outcome, Position, Time};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
pub struct Fixture {
    pub native: support::Fixture,
    pub installed: InstalledStore,
    pub owner: Owner,
    pub service: Arc<Service>,
}
impl Fixture {
    pub fn new(label: &str) -> Self {
        let native = support::Fixture::new(label, None);
        let target = PathBuf::from(&native.project.manifest.locator);
        let (mut client, worker) = support::pair(move |connection| {
            receive_install(connection, &target, StoreSettings::default())
        });
        layerfs_sdk::install(&native.project, &mut client).unwrap();
        let installed = worker.join().unwrap();
        let owner = Owner::start(
            &native.directory.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let service = Arc::new(Service::new(installed.opened.store.clone(), &owner));
        Self {
            native,
            installed,
            owner,
            service,
        }
    }
    pub fn cleanup(self) {
        drop(self.service);
        self.owner.stop().unwrap();
        drop(self.installed);
        self.native.cleanup();
    }
    pub fn statements(&self) -> u64 {
        let work = self.installed.opened.diagnostics().unwrap();
        work.writer.statements
            + work
                .readers
                .iter()
                .map(|reader| reader.statements)
                .sum::<u64>()
    }
}
pub fn bytes(tag: u8) -> Vec<u8> {
    let mut bytes = (0..support::FILE_BYTES)
        .map(|n| (n % 251) as u8)
        .collect::<Vec<_>>();
    bytes[0] = tag;
    bytes
}
pub fn construct(
    save: &Save<'_>,
    snapshot: &BranchSnapshot,
    tag: u8,
    policy: ConstructionPolicy,
) -> Result<FilesystemRootId, CommitError> {
    let mut reader = FilesystemRead::new(save, FilesystemRootId(snapshot.effective_root))?;
    let root = reader.root();
    let mut file = reader.resolve_child(root.root_inode().serial(), &PathName::new("file")?)?;
    let mut sink = save.sink();
    let built = Timing::disabled("control.construct", |scope| {
        layerfs_content::construct_bytes(
            policy,
            &policy.capacities(),
            &bytes(tag),
            &mut sink,
            scope.child("file"),
        )
    })
    .0?;
    file.value.content_root = built.root;
    let input = FilesystemInput {
        base: Some(FilesystemRootId(snapshot.effective_root)),
        scope: root.scope(),
        root_serial: root.root_inode().serial(),
        directories: &[],
        inodes: &[InodeUpdate {
            serial: file.serial,
            value: file.value,
        }],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let mut objects = FilesystemObjects::new_with_accepted(save, &mut sink, save);
    Ok(layerfs_content::filesystem::update_filesystem(&mut objects, &input, None)?.root)
}
pub fn write(service: &Service, token: WorkspaceToken, tag: u8) {
    static NEXT: AtomicU64 = AtomicU64::new(100);
    let operation = service.operation(token).unwrap();
    let route = operation.workspace().route();
    let done = operation
        .overlay()
        .try_submit(
            Some(route),
            Command::AcquireBaseSource {
                owner: NEXT.fetch_add(1, Ordering::Relaxed),
            },
        )
        .unwrap()
        .wait()
        .unwrap();
    let source = match done.result() {
        Ok(Response::BaseSource(source)) => *source,
        other => panic!("{other:?}"),
    };
    drop(done);
    let view = operation.workspace().view_for_source(source).unwrap();
    let stat = view
        .lookup(
            operation.overlay(),
            view.root_serial(),
            &PathName::new("file").unwrap(),
        )
        .unwrap();
    let changed = operation
        .workspace()
        .mutate(
            operation.overlay(),
            operation.ports(),
            &view,
            Operation::Write {
                serial: stat.serial,
                position: Position::At(0),
                data: vec![tag].into(),
            },
            Time {
                seconds: stat.metadata.mtime_seconds,
                nanoseconds: stat.metadata.mtime_nanoseconds,
            },
        )
        .unwrap();
    let publication = match changed {
        Outcome::Applied { publication, .. } => publication,
        other => panic!("{other:?}"),
    };
    assert!(operation
        .overlay()
        .try_submit(Some(route), Command::ReplyAttempted(publication))
        .unwrap()
        .wait()
        .unwrap()
        .result()
        .is_ok());
    drop(view);
    assert!(operation
        .overlay()
        .try_submit(Some(route), Command::ReleaseBaseSource(source))
        .unwrap()
        .wait()
        .unwrap()
        .result()
        .is_ok());
}
