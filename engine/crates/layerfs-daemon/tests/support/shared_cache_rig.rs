//! Several Workspaces of one daemon on branches forked from one Layer, over
//! a real sealed Disposable Store and the real local owner, without a mount.
//! A daemon here is one opened Store with its one shared canonical cache and
//! one owner; stopping it and opening another gives an empty cache over the
//! same Store file. Every wait is bounded.
use super::{drive, model, support};
use layerfs_content::{
    filesystem::{PathName, SymlinkTarget},
    object::inode_leaf::InodeKind,
    AuthenticatedObjects, ContentError, ContentResult, ObjectId,
};
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace, Store, StoreOperation, StorePorts},
    Owner, OwnerConfig,
};
use layerfs_history::{
    BranchId, BranchSnapshot, ForkRequest, ForkSource, HistoryName, WorkspaceId,
};
use layerfs_overlay::ProfileConfig;

use layerfs_storage::ReservationBlocks;
use layerfs_workspace::{
    CanonicalClient, ClientWork, Operation, Outcome, Position, SourceView, Time, ViewStat,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

pub use drive::Do;

/// Entries below `source`, of every kind, as Init counts them.
pub fn entries(source: &Path) -> usize {
    let mut count = 0;
    let mut pending = vec![source.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            count += 1;
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            }
        }
    }
    count
}

/// One sealed Store whose `branches` branches all start at the Layer Init
/// published. Branch 0 is the fixture's own.
pub struct Tree {
    pub fixture: support::Fixture,
    pub branches: Vec<BranchId>,
    daemons: AtomicUsize,
}
impl Tree {
    pub fn new(label: &str, branches: usize, build: impl FnOnce(&Path)) -> Self {
        let fixture = support::Fixture::built(label, |source| {
            build(source);
            entries(source)
        });
        let mut tree = Self {
            branches: vec![fixture.branch],
            fixture,
            daemons: AtomicUsize::new(0),
        };
        // The other branches are forked from the same Layer through the
        // ordinary writable History of a first daemon, which reads nothing.
        let first = tree.daemon(1, 0);
        let main = first.snapshot(tree.fixture.branch);
        for n in 1..branches {
            let branch = BranchId::from_authority([100 + n as u8; 16]);
            let forked = first
                .store
                .history()
                .fork(&ForkRequest {
                    stack: main.branch.stack,
                    branch,
                    name: HistoryName::new(&format!("branch-{n}")).unwrap(),
                    source: ForkSource::Layer(main.branch.base_layer),
                })
                .unwrap();
            assert_eq!(forked.effective_root, main.effective_root);
            tree.branches.push(branch);
        }
        first.stop();
        tree
    }
    /// A daemon with an empty cache of `cache` bytes and `readers` sessions.
    pub fn daemon(&self, _readers: usize, cache: usize) -> Daemon {
        let number = self.daemons.fetch_add(1, Ordering::Relaxed);
        let store = open_store(
            self.fixture.config.clone(),
            support::BINDING,
            support::CURSOR,
            OwnerConfig::default().namespaces,
            cache,
            ReservationBlocks::default(),
        )
        .unwrap();
        // Each daemon keeps its Workspace files beside its own overlay path.
        let local = self.fixture.directory.join(format!("overlay-{number}"));
        std::fs::create_dir_all(&local).unwrap();
        let owner = Owner::start(
            &local.join("overlay.sqlite"),
            ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        Daemon { store, owner }
    }
    pub fn finish(self) {
        self.fixture.cleanup();
    }
}

pub struct Daemon {
    pub store: Arc<Store>,
    pub owner: Owner,
}
impl Daemon {
    pub fn snapshot(&self, branch: BranchId) -> BranchSnapshot {
        self.store
            .history()
            .branch_snapshot(branch)
            .unwrap()
            .unwrap()
    }
    pub fn bind(&self, branch: BranchId, tag: u8) -> Arc<BoundWorkspace> {
        Arc::new(
            self.store
                .bind(
                    self.owner.client(),
                    BindRequest {
                        branch,
                        workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
                    },
                )
                .unwrap()
                .workspace,
        )
    }
    /// A Workspace of its own identity for each `number`, for a run that
    /// binds a new one per Commit.
    pub fn bind_numbered(&self, branch: BranchId, number: u32) -> Arc<BoundWorkspace> {
        let mut identity = [0xA5; 32];
        identity[..4].copy_from_slice(&number.to_be_bytes());
        Arc::new(
            self.store
                .bind(
                    self.owner.client(),
                    BindRequest {
                        branch,
                        workspace: WorkspaceId::from_authority(identity).unwrap(),
                    },
                )
                .unwrap()
                .workspace,
        )
    }
    pub fn close(&self, workspace: &BoundWorkspace) {
        workspace.close().unwrap();
    }
    /// Cumulative counters of the daemon's one shared cache.
    pub fn cache(&self) -> ClientWork {
        self.store.cache_work().unwrap()
    }
    /// The complete tree of one published root, read straight from the Store
    /// port with no cache, and the identity and length of every object that
    /// reading demanded. Nothing here touches the shared cache.
    pub fn published(&self, snapshot: &BranchSnapshot) -> (model::Flat, BTreeMap<ObjectId, usize>) {
        let recorder = Recorder {
            ports: self.store.ports(snapshot.scope),
            seen: Mutex::new(BTreeMap::new()),
        };
        let flat = model::canonical(&recorder, snapshot.effective_root);
        (flat, recorder.seen.into_inner().unwrap())
    }
    /// A client of the test's own over the Store port, with a cache of its
    /// own that never evicts, that remembers which objects it was the first
    /// to demand. Independent of the shared cache.
    pub fn tracker(&self, snapshot: &BranchSnapshot) -> Tracker {
        let recorder = Arc::new(Recorder {
            ports: self.store.ports(snapshot.scope),
            seen: Mutex::new(BTreeMap::new()),
        });
        Tracker {
            client: CanonicalClient::new(recorder.clone(), 1 << 30),
            recorder,
        }
    }
    /// The complete tree of one published root through a client of the
    /// test's own, with a cache of its own: independent of the shared cache
    /// and of the Workspace view, and it records nothing.
    pub fn expected(&self, snapshot: &BranchSnapshot) -> model::Flat {
        let client = CanonicalClient::new(self.store.ports(snapshot.scope), 256 * 1024 * 1024);
        model::canonical(&client, snapshot.effective_root)
    }
    pub fn stop(self) {
        drop(self.store);
        self.owner.stop().unwrap();
    }
}

/// The Store's own object port, uncached: it records what was demanded and
/// checks each returned object against its identity itself.
struct Recorder {
    ports: Arc<StorePorts>,
    seen: Mutex<BTreeMap<ObjectId, usize>>,
}
impl AuthenticatedObjects for Recorder {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let values = self.ports.read_canonical_batch(ids)?;
        let mut seen = self.seen.lock().unwrap();
        for (id, value) in ids.iter().zip(&values) {
            if ObjectId::for_bytes(value) != *id {
                return Err(ContentError::IdentityMismatch);
            }
            seen.insert(*id, value.len());
        }
        Ok(values)
    }
}

/// See [`Daemon::tracker`].
pub struct Tracker {
    client: CanonicalClient,
    recorder: Arc<Recorder>,
}
impl Tracker {
    /// Walks one published root completely and returns the objects no
    /// earlier walk of this tracker had demanded, with their lengths.
    pub fn fresh(&self, root: ObjectId) -> (model::Flat, Vec<(ObjectId, usize)>) {
        let flat = model::canonical(&self.client, root);
        let fresh = std::mem::take(&mut *self.recorder.seen.lock().unwrap());
        (flat, fresh.into_iter().collect())
    }
}

fn name(value: &str) -> PathName {
    PathName::new(value).unwrap()
}
fn parts(path: &str) -> Vec<&str> {
    path.split('/').filter(|n| !n.is_empty()).collect()
}
fn walk(operation: &StoreOperation, view: &SourceView, names: &[&str]) -> ViewStat {
    let mut stat = view.stat(operation.overlay(), view.root_serial()).unwrap();
    for part in names {
        stat = view
            .lookup(operation.overlay(), stat.serial, &name(part))
            .unwrap_or_else(|e| panic!("lookup {part}: {e:?}"));
    }
    stat
}
fn at(operation: &StoreOperation, view: &SourceView, path: &str) -> (u64, PathName) {
    let mut names = parts(path);
    let last = names.pop().expect("a path below the root");
    (walk(operation, view, &names).serial, name(last))
}

/// One attempted Workspace operation and its reply release, at `clock`.
/// True when it changed the Workspace.
pub fn apply(workspace: &BoundWorkspace, clock: &mut i64, step: Do<'_>) -> bool {
    *clock += 1;
    let now = Time {
        seconds: *clock,
        nanoseconds: 123_456_789,
    };
    drive::window(workspace, |operation, view| {
        let serial = |path: &str| walk(operation, view, &parts(path)).serial;
        let request = match step {
            Do::Create(path, mode) => {
                let (parent, name) = at(operation, view, path);
                Operation::Create { parent, name, mode }
            }
            Do::Mkdir(path, mode) => {
                let (parent, name) = at(operation, view, path);
                Operation::Mkdir { parent, name, mode }
            }
            Do::Symlink(path, target) => {
                let (parent, name) = at(operation, view, path);
                Operation::Symlink {
                    parent,
                    name,
                    target: SymlinkTarget::new(target.to_vec()).unwrap(),
                }
            }
            Do::Link(existing, path) => {
                let (parent, name) = at(operation, view, path);
                Operation::Link {
                    serial: serial(existing),
                    parent,
                    name,
                }
            }
            Do::Remove(path) => {
                let (parent, name) = at(operation, view, path);
                if walk(operation, view, &parts(path)).kind == InodeKind::Directory {
                    Operation::Rmdir { parent, name }
                } else {
                    Operation::Unlink { parent, name }
                }
            }
            Do::Rename(from, to) => {
                let (parent, name) = at(operation, view, from);
                let (new_parent, new_name) = at(operation, view, to);
                let mut destination = parts(to);
                destination.pop();
                Operation::Rename {
                    parent,
                    name,
                    new_parent,
                    new_name,
                    replace: true,
                    destination_path: Some(destination.into_iter().map(self::name).collect()),
                }
            }
            Do::Write(path, offset, data) => Operation::Write {
                serial: serial(path),
                position: Position::At(offset),
                data: data.into(),
            },
            Do::Truncate(path, size) => Operation::SetAttributes {
                serial: serial(path),
                mode: None,
                mtime: None,
                size: Some(size),
            },
            Do::Chmod(path, mode) => Operation::SetAttributes {
                serial: serial(path),
                mode: Some(mode),
                mtime: None,
                size: None,
            },
            Do::Utimens(path, (seconds, nanoseconds)) => Operation::SetAttributes {
                serial: serial(path),
                mode: None,
                mtime: Some(Time {
                    seconds,
                    nanoseconds,
                }),
                size: None,
            },
            Do::Claimed(..) | Do::Recognized(..) => panic!("{step:?}: not driven by this rig"),
        };
        let outcome = operation
            .workspace()
            .mutate(operation.overlay(), operation.ports(), view, request, now)
            .unwrap_or_else(|e| panic!("{step:?}: {e:?}"));
        match outcome {
            Outcome::Applied { publication, .. } => {
                drive::job(
                    operation.overlay(),
                    workspace.route(),
                    |job_client: &layerfs_daemon::OwnerClient, _| {
                        job_client.reply_attempted(publication)
                    },
                );
                true
            }
            Outcome::Unchanged { .. } => false,
        }
    })
}

/// What an ordinary reader asks about one path: every component looked up,
/// the last one's attributes, a directory's whole listing and a symbolic
/// link's target. No file content.
pub fn visit(workspace: &BoundWorkspace, path: &str) -> ViewStat {
    drive::window(workspace, |operation, view| {
        let overlay = operation.overlay();
        let stat = walk(operation, view, &parts(path));
        match stat.kind {
            InodeKind::Directory => {
                let mut after: Option<Vec<u8>> = None;
                loop {
                    let page = view.list(overlay, stat.serial, after.as_deref()).unwrap();
                    match page.continuation {
                        Some(next) => {
                            assert!(after.as_ref() < Some(&next), "listing did not advance");
                            after = Some(next);
                        }
                        None => break,
                    }
                }
            }
            InodeKind::Symlink => {
                view.readlink(overlay, stat.serial).unwrap();
            }
            InodeKind::RegularFile => {}
        }
        stat
    })
}

/// Every byte of one regular file through the Workspace's ordinary view.
pub fn read(workspace: &BoundWorkspace, path: &str) -> Vec<u8> {
    drive::window(workspace, |operation, view| {
        let stat = walk(operation, view, &parts(path));
        assert_eq!(stat.kind, InodeKind::RegularFile, "{path}");
        let mut bytes = Vec::new();
        while (bytes.len() as u64) < stat.logical_len {
            let read = view
                .read(
                    operation.overlay(),
                    stat.serial,
                    bytes.len() as u64,
                    65536,
                    &mut bytes,
                )
                .unwrap();
            assert!(read > 0, "short read of {path}");
        }
        bytes
    })
}

/// One product Commit of everything the Workspace has published locally.
/// Its completions are released before this returns.
pub fn commit(workspace: &BoundWorkspace) -> ObjectId {
    let done = workspace
        .commit_captured(&layerfs_daemon::store::CommitOptions::default())
        .unwrap_or_else(|failure| panic!("Commit: {:?}", failure.error));
    drop(done);
    workspace.snapshot().unwrap().effective_root
}

/// The bytes a test writes: a function of its seed and length alone.
pub fn pattern(seed: u32, length: usize) -> Vec<u8> {
    (0..length)
        .map(|n| {
            (n as u32)
                .wrapping_mul(31)
                .wrapping_add(seed.wrapping_mul(97)) as u8
        })
        .collect()
}
