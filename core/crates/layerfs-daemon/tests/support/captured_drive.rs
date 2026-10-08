//! Pairs every mutation of a bound Workspace with the same step of the
//! independent model, and reads the live view completely for comparison.
use super::{
    model::{self, Flat, Model, Raw, Seen, Stamp},
    support,
};
use layerfs_content::{
    filesystem::{PathName, SymlinkTarget},
    object::inode_leaf::InodeKind,
};
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace, Store, StoreOperation},
    Command, Completion, Owner, OwnerClient, OwnerConfig, Response,
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::{ProfileConfig, Route};
use layerfs_storage::ReservationBlocks;
use layerfs_workspace::{Operation, Outcome, Position, SourceView, Time, ViewStat};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// Evidence goes straight to the process output so a passing run's receipt
/// holds it; the test harness would otherwise keep it.
pub fn evidence(line: std::fmt::Arguments<'_>) {
    use std::io::Write;
    writeln!(std::io::stdout().lock(), "{line}").unwrap();
}
pub fn job(owner: &OwnerClient, route: Route, command: Command) -> Completion {
    let pending = owner
        .try_submit(Some(route), command)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < until, "bounded original local completion");
        std::thread::yield_now();
    }
}
/// The engine thread drops its own final credit after the consumer observes a
/// completion: bound the wait for that release before an exact counter check.
pub fn settled(client: &OwnerClient, outstanding: usize) {
    let end = Instant::now() + Duration::from_secs(3);
    while client.diagnostics().unwrap().outstanding != outstanding {
        assert!(
            Instant::now() < end,
            "engine did not release its final credit"
        );
        std::thread::yield_now();
    }
}

/// A sealed, installed Disposable Store whose base is the model of the native
/// source it was initialized from, plus one local owner.
pub struct Rig {
    pub f: support::Fixture,
    pub store: Arc<Store>,
    pub owner: Owner,
    pub base: Model,
}
impl Rig {
    pub fn new(label: &str, build: impl FnOnce(&Path)) -> Self {
        let mut base = None;
        let f = support::Fixture::built(label, |source| {
            build(source);
            let (model, names) = Model::native(source);
            base = Some(model);
            names
        });
        let store = open_store(
            f.config.clone(),
            support::BINDING,
            support::CURSOR,
            2,
            2 * 1024 * 1024,
            ReservationBlocks::default(),
        )
        .unwrap();
        let owner = Owner::start(
            &f.directory.join("overlay.sqlite"),
            ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        Self {
            f,
            store,
            owner,
            base: base.unwrap(),
        }
    }
    pub fn bind(&self, tag: u8) -> Arc<BoundWorkspace> {
        Arc::new(
            self.store
                .bind(
                    self.owner.client(),
                    BindRequest {
                        branch: self.f.branch,
                        workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
                    },
                )
                .unwrap()
                .workspace,
        )
    }
    pub fn close(&self, workspace: &BoundWorkspace) {
        assert!(matches!(
            job(&self.owner.client(), workspace.route(), Command::Close).result(),
            Ok(Response::Done)
        ));
    }
    pub fn finish(self) {
        drop(self.store);
        self.owner.stop().unwrap();
        self.f.cleanup();
    }
}

/// One processing source for a closure, released after its consumers.
pub fn window<T>(
    workspace: &BoundWorkspace,
    f: impl FnOnce(&StoreOperation, &SourceView) -> T,
) -> T {
    static NEXT: AtomicU64 = AtomicU64::new(100);
    let operation = workspace.operation().unwrap();
    let acquired = job(
        operation.overlay(),
        workspace.route(),
        Command::AcquireBaseSource {
            owner: NEXT.fetch_add(1, Ordering::Relaxed),
        },
    );
    let source = match acquired.result() {
        Ok(Response::BaseSource(source)) => *source,
        other => panic!("{other:?}"),
    };
    drop(acquired);
    let view = operation.workspace().view_for_source(source).unwrap();
    let value = f(&operation, &view);
    drop(view);
    assert!(job(
        operation.overlay(),
        workspace.route(),
        Command::ReleaseBaseSource(source)
    )
    .result()
    .is_ok());
    value
}

#[derive(Clone, Copy, Debug)]
pub enum Do<'a> {
    Create(&'a str, u32),
    Mkdir(&'a str, u32),
    Symlink(&'a str, &'a [u8]),
    Link(&'a str, &'a str),
    /// Unlink, or removal of an empty directory.
    Remove(&'a str),
    Rename(&'a str, &'a str),
    Write(&'a str, u64, &'a [u8]),
    Truncate(&'a str, u64),
    Chmod(&'a str, u32),
    Utimens(&'a str, Stamp),
}
fn name(value: &str) -> PathName {
    PathName::new(value).unwrap()
}
fn parted(path: &str) -> (Vec<&str>, &str) {
    let mut names = path
        .split('/')
        .filter(|n| !n.is_empty())
        .collect::<Vec<_>>();
    let last = names.pop().expect("a path below the root");
    (names, last)
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
fn resolve(operation: &StoreOperation, view: &SourceView, path: &str) -> ViewStat {
    let names = path
        .split('/')
        .filter(|n| !n.is_empty())
        .collect::<Vec<_>>();
    walk(operation, view, &names)
}

/// Mutations applied to the live Workspace and the model with one clock. Each
/// step is one attempted Workspace operation and its reply release.
pub struct Session<'a> {
    pub workspace: &'a BoundWorkspace,
    pub model: Model,
    clock: i64,
    pub steps: usize,
}
impl<'a> Session<'a> {
    pub fn new(workspace: &'a BoundWorkspace, model: Model) -> Self {
        Self {
            workspace,
            model,
            clock: 1_800_000_000,
            steps: 0,
        }
    }
    pub fn all(&mut self, steps: &[Do<'_>]) {
        for step in steps {
            self.apply(*step);
        }
    }
    pub fn apply(&mut self, step: Do<'_>) {
        self.clock += 1;
        self.steps += 1;
        let stamp = (self.clock, 123_456_789);
        let now = Time {
            seconds: stamp.0,
            nanoseconds: stamp.1,
        };
        let applied = window(self.workspace, |operation, view| {
            let at = |path: &str| {
                let (parents, last) = parted(path);
                (walk(operation, view, &parents).serial, name(last))
            };
            let request = match step {
                Do::Create(path, mode) => {
                    let (parent, name) = at(path);
                    Operation::Create { parent, name, mode }
                }
                Do::Mkdir(path, mode) => {
                    let (parent, name) = at(path);
                    Operation::Mkdir { parent, name, mode }
                }
                Do::Symlink(path, target) => {
                    let (parent, name) = at(path);
                    Operation::Symlink {
                        parent,
                        name,
                        target: SymlinkTarget::new(target.to_vec()).unwrap(),
                    }
                }
                Do::Link(existing, path) => {
                    let (parent, name) = at(path);
                    Operation::Link {
                        serial: resolve(operation, view, existing).serial,
                        parent,
                        name,
                    }
                }
                Do::Remove(path) => {
                    let (parent, name) = at(path);
                    if resolve(operation, view, path).kind == InodeKind::Directory {
                        Operation::Rmdir { parent, name }
                    } else {
                        Operation::Unlink { parent, name }
                    }
                }
                Do::Rename(from, to) => {
                    let (parent, name) = at(from);
                    let (new_parent, new_name) = at(to);
                    Operation::Rename {
                        parent,
                        name,
                        new_parent,
                        new_name,
                        replace: true,
                        destination_path: Some(parted(to).0.into_iter().map(self::name).collect()),
                    }
                }
                Do::Write(path, offset, data) => Operation::Write {
                    serial: resolve(operation, view, path).serial,
                    position: Position::At(offset),
                    data: data.into(),
                },
                Do::Truncate(path, size) => Operation::SetAttributes {
                    serial: resolve(operation, view, path).serial,
                    mode: None,
                    mtime: None,
                    size: Some(size),
                },
                Do::Chmod(path, mode) => Operation::SetAttributes {
                    serial: resolve(operation, view, path).serial,
                    mode: Some(mode),
                    mtime: None,
                    size: None,
                },
                Do::Utimens(path, (seconds, nanoseconds)) => Operation::SetAttributes {
                    serial: resolve(operation, view, path).serial,
                    mode: None,
                    mtime: Some(Time {
                        seconds,
                        nanoseconds,
                    }),
                    size: None,
                },
            };
            let outcome = operation
                .workspace()
                .mutate(operation.overlay(), operation.ports(), view, request, now)
                .unwrap_or_else(|e| panic!("{step:?}: {e:?}"));
            match outcome {
                Outcome::Applied { publication, .. } => {
                    assert!(job(
                        operation.overlay(),
                        self.workspace.route(),
                        Command::ReplyAttempted(publication)
                    )
                    .result()
                    .is_ok());
                    true
                }
                Outcome::Unchanged { .. } => false,
            }
        });
        let before = self.model.flat();
        match step {
            Do::Create(path, mode) => self.model.create(path, mode, stamp),
            Do::Mkdir(path, mode) => self.model.mkdir(path, mode, stamp),
            Do::Symlink(path, target) => self.model.symlink(path, target, stamp),
            Do::Link(existing, path) => self.model.link(existing, path, stamp),
            Do::Remove(path) => self.model.remove(path, stamp),
            Do::Rename(from, to) => self.model.rename(from, to, stamp),
            Do::Write(path, offset, data) => self.model.write(path, offset as usize, data, stamp),
            Do::Truncate(path, size) => self.model.truncate(path, size as usize, stamp),
            Do::Chmod(path, mode) => self.model.chmod(path, mode),
            Do::Utimens(path, mtime) => self.model.utimens(path, mtime),
        }
        assert_eq!(
            applied,
            before != self.model.flat(),
            "{step:?}: Workspace and model disagree on whether anything changed"
        );
    }
}

/// Complete walk of the live effective view through ordinary Workspace reads.
pub fn live(workspace: &BoundWorkspace) -> Flat {
    window(workspace, |operation, view| {
        let overlay = operation.overlay();
        let mut rows = Vec::new();
        let mut pending = vec![(Vec::new(), view.root_serial())];
        while let Some((path, serial)) = pending.pop() {
            let stat = view.stat(overlay, serial).unwrap();
            let (kind, payload) = match stat.kind {
                InodeKind::Directory => {
                    let mut after: Option<Vec<u8>> = None;
                    loop {
                        let page = view.list(overlay, serial, after.as_deref()).unwrap();
                        for (name, child) in &page.entries {
                            pending.push((model::join(&path, name), *child));
                        }
                        match page.continuation {
                            Some(next) => {
                                assert!(after.as_ref() < Some(&next), "listing did not advance");
                                after = Some(next);
                            }
                            None => break,
                        }
                    }
                    (model::DIRECTORY, Vec::new())
                }
                InodeKind::Symlink => (
                    model::SYMLINK,
                    view.readlink(overlay, serial).unwrap().as_bytes().to_vec(),
                ),
                InodeKind::RegularFile => {
                    let mut bytes = Vec::new();
                    while (bytes.len() as u64) < stat.logical_len {
                        let read = view
                            .read(overlay, serial, bytes.len() as u64, 65536, &mut bytes)
                            .unwrap();
                        assert!(read > 0, "short live read");
                    }
                    (model::FILE, bytes)
                }
            };
            rows.push(Raw {
                path,
                serial,
                seen: Seen {
                    kind,
                    mode: stat.metadata.mode,
                    mtime: (stat.metadata.mtime_seconds, stat.metadata.mtime_nanoseconds),
                    links: stat.namespace_refs,
                    payload,
                    identity: Vec::new(),
                },
            });
        }
        model::identified(rows)
    })
}
