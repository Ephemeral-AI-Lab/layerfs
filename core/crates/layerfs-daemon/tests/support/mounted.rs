//! One real native serving assembly over an installed Store, for mounted proofs.
use layerfs_bridge::control::{
    NativePhase, ReadyMount, Reply, Request, WorkspaceStatus, WorkspaceToken,
};
use layerfs_daemon::{
    control::{Failure, NativeConfig, NativeServing, Service, Success},
    store::Store,
    Command, Owner, OwnerConfig, Response,
};
use layerfs_history::{BranchId, WorkspaceId};
use nix::unistd::{Gid, Uid};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// Runtime command identity: every inode's owner, never the mount owner.
pub const COMMAND: u32 = 65534;
pub const READ_HANDLES: usize = 4;
pub struct Harness {
    pub owner: Owner,
    pub serving: Arc<NativeServing>,
    pub service: Arc<Service>,
    pub mounts: PathBuf,
    pub branch: BranchId,
}
impl Harness {
    pub fn new(store: Arc<Store>, directory: &Path, branch: BranchId) -> Self {
        let owner = Owner::start(
            &directory.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let mounts = directory.join("mounts");
        fs::create_dir(&mounts).unwrap();
        fs::set_permissions(&mounts, fs::Permissions::from_mode(0o755)).unwrap();
        let serving = Arc::new(
            NativeServing::start(NativeConfig {
                mounts: mounts.clone(),
                owner_uid: Uid::effective().as_raw(),
                owner_gid: Gid::effective().as_raw(),
                command_uid: COMMAND,
                command_gid: COMMAND,
                read_handles: READ_HANDLES,
                namespaces: OwnerConfig::default().namespaces,
                ready_wait: Duration::from_secs(3),
                drain_wait: Duration::from_secs(3),
            })
            .unwrap(),
        );
        let service = Arc::new(Service::with_native(store, &owner, serving.clone()));
        Self {
            owner,
            serving,
            service,
            mounts,
            branch,
        }
    }
    pub fn bind(&self, tag: u8) -> WorkspaceToken {
        let workspace = WorkspaceId::from_authority([tag; 32]).unwrap();
        match self
            .service
            .execute_control(&Request::Mount {
                workspace,
                branch: self.branch,
            })
            .unwrap()
            .reply
        {
            Reply::Bound { token, .. } => token,
            other => panic!("{other:?}"),
        }
    }
    pub fn try_attach(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        self.service.execute_control(&Request::Attach(token))
    }
    pub fn attach(&self, token: WorkspaceToken) -> ReadyMount {
        match self.try_attach(token).unwrap().reply {
            Reply::Ready(ready) => *ready,
            other => panic!("{other:?}"),
        }
    }
    pub fn mount(&self, tag: u8) -> ReadyMount {
        self.attach(self.bind(tag))
    }
    pub fn status(&self, token: WorkspaceToken) -> WorkspaceStatus {
        match self
            .service
            .execute_control(&Request::Status(token))
            .unwrap()
            .reply
        {
            Reply::Status(status) => *status,
            other => panic!("{other:?}"),
        }
    }
    pub fn phase(&self, token: WorkspaceToken) -> NativePhase {
        self.status(token).native.unwrap().phase
    }
    /// One product control Commit: the captured namespace producer through
    /// `execute_control`, attempted once on the caller's thread.
    pub fn try_commit(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        self.service.execute_control(&Request::Commit(token))
    }
    pub fn try_unmount(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        self.service.execute_control(&Request::Unmount(token))
    }
    /// Terminal success with its complete receipts checked.
    pub fn unmount(&self, ready: &ReadyMount) -> Success {
        let done = self.try_unmount(ready.token).unwrap();
        assert_eq!(done.reply, Reply::Unmounted(ready.token));
        assert_eq!(done.earlier.len(), 1, "native-owner revocation receipt");
        assert!(done.completion.is_some(), "logical Close receipt");
        let receipt = format!("{:?}", done.native.as_ref().expect("drain receipt"));
        assert!(receipt.contains("Drained"), "{receipt}");
        assert!(mount_entry(&ready.directory).is_none());
        assert!(!Path::new(&ready.directory).exists());
        done
    }
    /// Daemon-wide maintained engine counts, observed through a live route.
    pub fn engine(&self, through: WorkspaceToken) -> layerfs_overlay::StoredCounts {
        let operation = self.service.operation(through).unwrap();
        let done = operation
            .overlay()
            .try_submit(
                Some(operation.workspace().route()),
                Command::Resources { global: true },
            )
            .unwrap()
            .wait()
            .unwrap();
        match done.result() {
            Ok(Response::Resources(resources)) => resources.counts,
            other => panic!("{other:?}"),
        }
    }
    pub fn stop(self) {
        let work = self.serving.work().unwrap();
        assert_eq!(work.mounts, 0, "{work:?}");
        assert_eq!(work.live_workers, READ_HANDLES + 2, "{work:?}");
        assert_eq!((work.queued, work.running, work.retained), (0, 0, 0));
        drop(self.service);
        drop(self.serving);
        self.owner.stop().unwrap();
    }
}
/// A bounded observation loop for asynchronous kernel or maintenance effects.
pub fn until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "bounded observation expired: {what}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[derive(Debug)]
pub struct Entry {
    pub filesystem: String,
    pub source: String,
    pub options: String,
    pub super_options: String,
}
/// Independent kernel observation of the mount table; no daemon cooperation.
pub fn mount_entry(directory: &str) -> Option<Entry> {
    let table = fs::read_to_string("/proc/self/mountinfo").unwrap();
    table.lines().rev().find_map(|line| {
        let (before, after) = line.split_once(" - ")?;
        let fields: Vec<&str> = before.split(' ').collect();
        if fields.get(4) != Some(&directory) {
            return None;
        }
        let mut tail = after.split(' ');
        Some(Entry {
            filesystem: tail.next()?.to_owned(),
            source: tail.next()?.to_owned(),
            super_options: tail.next()?.to_owned(),
            options: fields.get(5)?.to_string(),
        })
    })
}
