//! Public Workspace operations against the production in-process Service.
#![cfg(target_os = "linux")]

#[path = "support/phase_b_commit.rs"]
mod phase_b_commit;
#[path = "support/phase_b_mutations.rs"]
mod phase_b_mutations;
#[path = "support/phase_b_namespace.rs"]
mod phase_b_namespace;

use layerfs_bridge::contract::{
    Code, CommitOutcomeWire, Failure, HistoryCommand, Inspect, Operation, Request, Response, Root,
    Source,
};
use layerfs_sdk::{HistoryMode, ProjectApi, Server, ServerConfig};
use layerfs_telemetry::runtime::Runtime;
use layerfs_workspace::{
    AttachOptions, Base, FileAccess, FileOpenOptions, HandleId, NodeAttributes, NodeKind,
    ReferenceScope, RenameFlags, Workspace, WorkspaceAccess, WorkspaceConfig, WorkspaceError,
    WorkspaceHost, DEFAULT_MEMORY_BUDGET_BYTES,
};
use std::{
    fs,
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(1);
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct SourceReader<'a> {
    source: &'a mut dyn Source,
    end: Instant,
    cancel: AtomicBool,
}
impl Read for SourceReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.source.read(buffer, self.end, &self.cancel)
    }
}
#[derive(Default)]
struct CommitGate {
    armed: AtomicBool,
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}
impl CommitGate {
    fn arm(&self) {
        *self.state.lock().unwrap() = (false, false);
        self.armed.store(true, Ordering::Release);
    }
    fn wait(&self) {
        let state = self.state.lock().unwrap();
        let (_state, timed) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |state| !state.0)
            .unwrap();
        assert!(!timed.timed_out(), "Commit did not reach the reply gate");
    }
    fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
    fn hold(&self) {
        let mut state = self.state.lock().unwrap();
        state.0 = true;
        self.changed.notify_all();
        let (_state, timed) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |state| !state.1)
            .unwrap();
        assert!(!timed.timed_out(), "Commit reply gate was not released");
    }
}
struct Fixture {
    workspace: Workspace,
    _host: WorkspaceHost,
    server: Arc<Server>,
    genesis: Root,
    gate: Arc<CommitGate>,
    fault: Arc<AtomicU64>,
    canonical_calls: Arc<AtomicU64>,
    saved_files: Arc<Mutex<Vec<(u64, u64)>>>,
    _temp: Temp,
}
impl Fixture {
    fn new() -> Self {
        Self::with_extra_children(0)
    }
    fn with_extra_children(extra: usize) -> Self {
        Self::with_layout(extra, false)
    }
    fn with_layout(extra: usize, deep: bool) -> Self {
        Self::with_layout_quota(extra, deep, 64 * 1024 * 1024)
    }
    fn with_layout_quota(extra: usize, deep: bool, quota: u64) -> Self {
        let root = std::env::var_os("LAYERFS_TEST_BACKING_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.canonicalize().unwrap().join(format!(
            "layerfs-inherited-workspace-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        let temp = Temp(path.clone());
        let (server, project, branch_id) = phase_b_commit::open_prepared(extra, deep)
            .unwrap_or_else(|| phase_b_commit::fresh_project(&path, extra, deep, 0));
        let gate = Arc::new(CommitGate::default());
        let fault = Arc::new(AtomicU64::new(0));
        let canonical_calls = Arc::new(AtomicU64::new(0));
        let saved_files = Arc::new(Mutex::new(Vec::new()));
        let delivery_files = saved_files.clone();
        let delivery_fault = fault.clone();
        let delivery_calls = canonical_calls.clone();
        let delivery_server = server.clone();
        let delivery_gate = gate.clone();
        let delivery = Arc::new(
            move |request: &Request,
                  input: &mut dyn Source,
                  output: &mut dyn io::Write,
                  end: Instant| {
                if delivery_fault.load(Ordering::Acquire) == 1
                    && matches!(
                        request.operation,
                        Operation::ConstructPortableMetadata { .. }
                            | Operation::UpdatePortableMetadata { .. }
                    )
                {
                    return Err(Code::Denied.into());
                }
                let canonical = matches!(
                    request.operation,
                    Operation::HistoryCommand(
                        HistoryCommand::Commit(_) | HistoryCommand::CommitStaged { .. }
                    )
                );
                if canonical {
                    delivery_calls.fetch_add(1, Ordering::AcqRel);
                }
                let mut reader = SourceReader {
                    source: input,
                    end,
                    cancel: AtomicBool::new(false),
                };
                let response = delivery_server
                    .service()
                    .handle_until(&delivery_server.peer()?, request, &mut reader, output, end)
                    .0?;
                if let Operation::SaveFileV2 {
                    extents,
                    replacement,
                    ..
                } = &request.operation
                {
                    delivery_files
                        .lock()
                        .unwrap()
                        .push((*extents, *replacement));
                }
                if canonical && delivery_fault.load(Ordering::Acquire) == 3 {
                    deny_fallocate_on_this_thread();
                }
                if canonical && delivery_fault.load(Ordering::Acquire) == 2 {
                    let mut lost: Failure = Code::Unknown.into();
                    lost.unknown = true;
                    return Err(lost);
                }
                if delivery_gate.armed.load(Ordering::Acquire)
                    && matches!(
                        request.operation,
                        Operation::HistoryCommand(
                            HistoryCommand::Commit(_) | HistoryCommand::CommitStaged { .. }
                        )
                    )
                {
                    delivery_gate.hold();
                }
                Ok::<Response, Failure>(response)
            },
        );
        let backing = path.join("backing");
        fs::create_dir(&backing).unwrap();
        let owner = fs::metadata(&backing).unwrap();
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root: backing,
                max_count: 2,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: Some(quota),
            },
            delivery,
        )
        .unwrap();
        let workspace = host
            .attach(
                AttachOptions {
                    id: "inherited".into(),
                    incarnation: [55; 32],
                    store: server.store(),
                    base: Base::Branch(branch_id),
                    access: WorkspaceAccess::LocalEdit,
                    owner_uid: owner.uid(),
                    owner_gid: owner.gid(),
                },
                deadline(),
            )
            .unwrap();
        Self {
            workspace,
            _host: host,
            server,
            genesis: project.root,
            gate,
            fault,
            canonical_calls,
            saved_files,
            _temp: temp,
        }
    }
    fn lookup(&self, parent: u64, name: &[u8]) -> NodeAttributes {
        self.workspace
            .lookup(parent, name, ReferenceScope::Local, deadline())
            .unwrap()
    }
    fn inspect(&self, root: Root, query: Inspect) -> Result<Response, Failure> {
        self.server
            .service()
            .handle(
                &self.server.peer()?,
                &Request {
                    id: 80,
                    generation: 1,
                    store: self.server.store(),
                    profile: 1,
                    deadline_ms: 10000,
                    response_bytes: 16384,
                    operation: Operation::Inspect { root, query },
                },
                &mut io::empty(),
                &mut io::sink(),
            )
            .0
    }
    fn attributes(&self, root: Root, path: &[u8]) -> Response {
        self.inspect(
            root,
            Inspect::Attributes {
                path: path.to_vec(),
            },
        )
        .unwrap()
    }
    fn content_bytes(&self, root: Root, path: &[u8]) -> Vec<u8> {
        let Response::Attributes { content, size, .. } = self.attributes(root, path) else {
            panic!("file attributes")
        };
        let mut output = Vec::new();
        let (response, _) = self.server.service().handle(
            &self.server.peer().unwrap(),
            &Request {
                id: 81,
                generation: 1,
                store: self.server.store(),
                profile: 1,
                deadline_ms: 10000,
                response_bytes: size,
                operation: Operation::ReadFile {
                    root: content,
                    start: 0,
                    end: size,
                },
            },
            &mut io::empty(),
            &mut output,
        );
        assert_eq!(response.unwrap(), Response::Read { length: size });
        output
    }
    fn commit(&self) -> Root {
        match self.workspace.commit(deadline()).unwrap().outcome {
            CommitOutcomeWire::Committed(commit) => commit.root,
            CommitOutcomeWire::UpToDate { root, .. } => root,
        }
    }
    fn open(&self, serial: u64) -> HandleId {
        self.workspace
            .open_file(
                serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    append: false,
                    truncate: false,
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap()
    }
}

#[test]
fn pinned_directory_retains_forgotten_ancestors_and_detached_parent_refuses_mutation() {
    let f = Fixture::new();
    let root = f.workspace.root().serial;
    let packages = f.lookup(root, b"packages");
    let old = f.lookup(packages.serial, b"old");
    let subtree = f.lookup(old.serial, b"subtree");
    f.workspace.forget(old.serial, 1, ReferenceScope::Local);
    f.workspace
        .forget(packages.serial, 1, ReferenceScope::Local);
    let child = f.lookup(subtree.serial, b"child");
    assert_eq!(f.lookup(child.serial, b"grand.txt").kind, NodeKind::File);
    let held = f
        .workspace
        .mkdir(subtree.serial, b"empty", 0o755, 0, deadline())
        .unwrap();
    let handle = f
        .workspace
        .opendir(held.serial, ReferenceScope::Local)
        .unwrap();
    f.workspace
        .rmdir(subtree.serial, b"empty", deadline())
        .unwrap();
    assert_eq!(
        f.workspace
            .mknod(held.serial, b"lost", 0o600, 0, deadline()),
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        f.workspace
            .lookup(held.serial, b"lost", ReferenceScope::Local, deadline()),
        Err(WorkspaceError::NotFound)
    );
    assert!(f.workspace.readdir(handle, 0, 2, deadline()).is_ok());
    f.workspace.releasedir(handle).unwrap();
    let new = f.lookup(packages.serial, b"new");
    let replaced = f
        .workspace
        .mkdir(new.serial, b"empty2", 0o755, 0, deadline())
        .unwrap();
    let held = f
        .workspace
        .opendir(replaced.serial, ReferenceScope::Local)
        .unwrap();
    f.workspace
        .rename(
            old.serial,
            b"subtree",
            new.serial,
            b"empty2",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(f.lookup(new.serial, b"empty2").serial, subtree.serial);
    assert_eq!(
        f.workspace
            .mknod(replaced.serial, b"lost", 0o600, 0, deadline()),
        Err(WorkspaceError::NotFound)
    );
    assert!(f.workspace.readdir(held, 0, 2, deadline()).is_ok());
    f.workspace.releasedir(held).unwrap();
}

#[test]
fn base_directory_move_keeps_inherited_children_handles_and_commits() {
    let f = Fixture::new();
    let top = f.workspace.root().serial;
    let packages = f.lookup(top, b"packages");
    let old = f.lookup(packages.serial, b"old");
    let new = f.lookup(packages.serial, b"new");
    let subtree = f.lookup(old.serial, b"subtree");
    let child = f.lookup(subtree.serial, b"child");
    let grand = f.lookup(child.serial, b"grand.txt");
    let file = f.open(grand.serial);
    let directory = f
        .workspace
        .opendir(subtree.serial, ReferenceScope::Local)
        .unwrap();
    let base = f.attributes(f.genesis, b"packages/old/subtree");
    assert!(matches!(base, Response::Attributes { serial, .. } if serial == subtree.serial));
    let obstacle = f
        .workspace
        .mknod(new.serial, b"obstacle", 0o600, 0, deadline())
        .unwrap();
    let revision = f.workspace.status().unwrap().revision;
    assert_eq!(
        f.workspace.rename(
            old.serial,
            b"subtree",
            new.serial,
            b"obstacle",
            RenameFlags::default(),
            deadline(),
        ),
        Err(WorkspaceError::NotDirectory)
    );
    assert_eq!(f.workspace.status().unwrap().revision, revision);
    assert_eq!(f.lookup(old.serial, b"subtree").serial, subtree.serial);
    assert_eq!(
        f.workspace.getattr(obstacle.serial).unwrap().serial,
        obstacle.serial
    );
    f.workspace
        .rename(
            old.serial,
            b"subtree",
            new.serial,
            b"subtree",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(f.lookup(new.serial, b"subtree").serial, subtree.serial);
    assert_eq!(f.lookup(subtree.serial, b"child").serial, child.serial);
    assert_eq!(f.lookup(child.serial, b"grand.txt").serial, grand.serial);
    let page = f.workspace.readdir(directory, 0, 8, deadline()).unwrap();
    assert_eq!(
        page.entries()
            .iter()
            .find(|entry| entry.name == b"..")
            .unwrap()
            .serial,
        new.serial
    );
    assert!(page
        .entries()
        .iter()
        .any(|entry| entry.name == b"sibling.txt"));
    assert_eq!(
        f.workspace.read(file, 0, 10, deadline()).unwrap().as_ref(),
        b"grand-base"
    );
    assert_eq!(
        f.workspace.rename(
            new.serial,
            b"subtree",
            child.serial,
            b"nested",
            RenameFlags::default(),
            deadline(),
        ),
        Err(WorkspaceError::InvalidInput)
    );
    f.workspace
        .rename(
            subtree.serial,
            b"child",
            subtree.serial,
            b"renamed",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    f.workspace
        .rename(
            subtree.serial,
            b"renamed",
            subtree.serial,
            b"child",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    let next = f
        .workspace
        .mknod(child.serial, b"grand.txt.next", 0o644, 0, deadline())
        .unwrap();
    let next_file = f.open(next.serial);
    let payload = f
        .workspace
        .own_payload(9, &mut b"grand-new".as_slice(), deadline())
        .unwrap();
    f.workspace
        .write_file(next_file, 0, &payload, deadline())
        .unwrap();
    f.workspace.release(next_file).unwrap();
    f.workspace
        .rename(
            child.serial,
            b"grand.txt.next",
            child.serial,
            b"grand.txt",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(
        f.workspace.read(file, 0, 10, deadline()).unwrap().as_ref(),
        b"grand-base"
    );
    let head = f.commit();
    let Response::Attributes { serial, .. } = f.attributes(head, b"packages/new/subtree") else {
        panic!("moved directory")
    };
    assert_eq!(serial, subtree.serial);
    let Response::Attributes { serial, .. } =
        f.attributes(head, b"packages/new/subtree/child/grand.txt")
    else {
        panic!("replacement")
    };
    assert_eq!(serial, next.serial);
    assert_eq!(
        f.workspace.getattr(child.serial).unwrap().serial,
        child.serial
    );
    assert_eq!(
        f.inspect(
            head,
            Inspect::Attributes {
                path: b"packages/old/subtree".to_vec()
            }
        )
        .unwrap_err()
        .code,
        Code::PathNotFound
    );
    assert_eq!(
        f.workspace.read(file, 0, 10, deadline()).unwrap().as_ref(),
        b"grand-base"
    );
    assert!(f
        .workspace
        .readdir(directory, 0, 8, deadline())
        .unwrap()
        .entries()
        .iter()
        .any(|entry| entry.name == b"child"));
    f.workspace.release(file).unwrap();
    f.workspace.releasedir(directory).unwrap();
}

#[test]
fn moved_canonical_symlink_reads_target_by_serial() {
    let f = Fixture::new();
    let root = f.workspace.root().serial;
    let packages = f.lookup(root, b"packages");
    let old = f.lookup(packages.serial, b"old");
    let new = f.lookup(packages.serial, b"new");
    let subtree = f.lookup(old.serial, b"subtree");
    let child = f.lookup(subtree.serial, b"child");
    let link = f
        .workspace
        .symlink(child.serial, b"ref", b"grand.txt", deadline())
        .unwrap();
    let old_head = f.commit();
    f.workspace
        .rename(
            old.serial,
            b"subtree",
            new.serial,
            b"subtree",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(
        f.workspace
            .readlink(link.serial, deadline())
            .unwrap()
            .as_ref(),
        b"grand.txt"
    );
    let new_head = f.commit();
    for head in [old_head, new_head] {
        assert_eq!(
            f.inspect(
                head,
                Inspect::InodeReadlink {
                    serial: link.serial
                }
            )
            .unwrap(),
            Response::Link(b"grand.txt".to_vec())
        );
    }
}

#[test]
fn moved_directory_keeps_frozen_g1_and_later_g2_write() {
    let f = Fixture::new();
    let top = f.workspace.root().serial;
    let packages = f.lookup(top, b"packages");
    let old = f.lookup(packages.serial, b"old");
    let new = f.lookup(packages.serial, b"new");
    let subtree = f.lookup(old.serial, b"subtree");
    let child = f.lookup(subtree.serial, b"child");
    let grand = f.lookup(child.serial, b"grand.txt");
    let held = f.open(grand.serial);
    f.workspace
        .rename(
            old.serial,
            b"subtree",
            new.serial,
            b"subtree",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    f.gate.arm();
    let first = std::thread::scope(|scope| {
        let saving = scope.spawn(|| f.commit());
        f.gate.wait();
        let payload = f
            .workspace
            .own_payload(10, &mut b"grand-live".as_slice(), deadline())
            .unwrap();
        f.workspace
            .write_file(held, 0, &payload, deadline())
            .unwrap();
        f.gate.release();
        saving.join().unwrap()
    });
    assert_eq!(
        f.content_bytes(first, b"packages/new/subtree/child/grand.txt"),
        b"grand-base"
    );
    assert_eq!(
        f.workspace.read(held, 0, 10, deadline()).unwrap().as_ref(),
        b"grand-live"
    );
    let second = f.commit();
    assert_eq!(
        f.content_bytes(second, b"packages/new/subtree/child/grand.txt"),
        b"grand-live"
    );
    assert_eq!(
        f.content_bytes(f.genesis, b"packages/old/subtree/child/grand.txt"),
        b"grand-base"
    );
    assert_eq!(
        f.inspect(
            second,
            Inspect::Attributes {
                path: b"packages/old/subtree".to_vec()
            }
        )
        .unwrap_err()
        .code,
        Code::PathNotFound
    );
    f.workspace.release(held).unwrap();
}

#[test]
fn growing_move_keeps_cached_descendant_reachable_beyond_4096_bytes() {
    let f = Fixture::new();
    let top = f.workspace.root().serial;
    let source = f
        .workspace
        .mkdir(top, b"src", 0o755, 0, deadline())
        .unwrap();
    let mut parent = source.serial;
    let long = vec![b'd'; 250];
    for _ in 0..15 {
        parent = f
            .workspace
            .mkdir(parent, &long, 0o755, 0, deadline())
            .unwrap()
            .serial;
    }
    let leaf = f
        .workspace
        .mknod(parent, b"leaf", 0o644, 0, deadline())
        .unwrap();
    let first = f
        .workspace
        .mkdir(top, &vec![b'a'; 255], 0o755, 0, deadline())
        .unwrap();
    let target = f
        .workspace
        .mkdir(first.serial, &[b'b'; 63], 0o755, 0, deadline())
        .unwrap();
    let before = f.commit();
    assert_eq!(3 + 15 * 251 + 5, 3773);
    assert_eq!(255 + 1 + 63 + 1 + 6 + (3773 - 3), 4096);
    f.workspace
        .rename(
            top,
            b"src",
            target.serial,
            b"mmmmmm",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(f.lookup(target.serial, b"mmmmmm").serial, source.serial);
    assert_eq!(
        f.workspace.getattr(leaf.serial).unwrap().serial,
        leaf.serial
    );
    f.workspace
        .rename(
            target.serial,
            b"mmmmmm",
            top,
            b"src",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    f.workspace
        .rename(
            top,
            b"src",
            target.serial,
            b"mmmmmmm",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    let mut current = f.lookup(target.serial, b"mmmmmmm").serial;
    for _ in 0..15 {
        current = f.lookup(current, &long).serial;
    }
    assert_eq!(f.lookup(current, b"leaf").serial, leaf.serial);
    f.workspace
        .rename(
            target.serial,
            b"mmmmmmm",
            top,
            b"src",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    let after = f.commit();
    assert!(
        matches!(f.attributes(before, b"src"), Response::Attributes { serial, .. } if serial == source.serial)
    );
    assert!(
        matches!(f.attributes(after, b"src"), Response::Attributes { serial, .. } if serial == source.serial)
    );
}

#[test]
fn growing_inherited_move_reaches_uncached_descendant_beyond_4096_bytes() {
    let f = Fixture::with_layout(0, true);
    let top = f.workspace.root().serial;
    f.lookup(top, b"src");
    let first = f
        .workspace
        .mkdir(top, &vec![b'a'; 255], 0o755, 0, deadline())
        .unwrap();
    let target = f
        .workspace
        .mkdir(first.serial, &[b'b'; 63], 0o755, 0, deadline())
        .unwrap();
    f.workspace
        .rename(
            top,
            b"src",
            target.serial,
            b"mmmmmmm",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    let mut current = f.lookup(target.serial, b"mmmmmmm").serial;
    for _ in 0..15 {
        current = f.lookup(current, &b"d".repeat(250)).serial;
    }
    let leaf = f.lookup(current, b"leaf");
    let held = f.open(leaf.serial);
    assert_eq!(
        f.workspace.read(held, 0, 9, deadline()).unwrap().as_ref(),
        b"deep-base"
    );
    f.workspace.release(held).unwrap();
    f.workspace
        .rename(
            target.serial,
            b"mmmmmmm",
            target.serial,
            b"mmmmmm",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    let head = f.commit();
    let mut path = b"a".repeat(255);
    path.extend_from_slice(b"/");
    path.extend_from_slice(&b"b".repeat(63));
    path.extend_from_slice(b"/mmmmmm");
    for _ in 0..15 {
        path.extend_from_slice(b"/");
        path.extend_from_slice(&b"d".repeat(250));
    }
    path.extend_from_slice(b"/leaf");
    assert_eq!(path.len(), 4096);
    assert_eq!(f.content_bytes(head, &path), b"deep-base");
    let mut old_path = b"src".to_vec();
    old_path.extend_from_slice(&path[255 + 1 + 63 + 1 + 6..]);
    assert_eq!(f.content_bytes(f.genesis, &old_path), b"deep-base");
}

#[test]
fn growing_rename_refuses_private_budget_before_publication() {
    let baseline = Fixture::new();
    let root = baseline.workspace.root().serial;
    let packages = baseline.lookup(root, b"packages");
    let old = baseline.lookup(packages.serial, b"old");
    let new = baseline.lookup(packages.serial, b"new");
    let baseline_bytes = baseline.workspace.backing_status().unwrap().allocated_bytes;
    let quota = baseline_bytes + 8192;
    for serial in [packages.serial, old.serial, new.serial] {
        baseline
            .workspace
            .forget(serial, u64::MAX, ReferenceScope::Local);
    }
    baseline.workspace.close_clean().unwrap();
    assert_eq!(
        baseline.workspace.backing_status().unwrap().allocated_bytes,
        0
    );
    drop(baseline);
    let f = Fixture::with_layout_quota(0, false, quota);
    let root = f.workspace.root().serial;
    let packages = f.lookup(root, b"packages");
    let old = f.lookup(packages.serial, b"old");
    let new = f.lookup(packages.serial, b"new");
    let revision = f.workspace.status().unwrap().revision;
    let error = f
        .workspace
        .rename(
            old.serial,
            b"subtree",
            new.serial,
            b"subtree-expanded",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap_err();
    assert!(
        matches!(error, WorkspaceError::Capacity | WorkspaceError::Backing(_)),
        "{error:?}"
    );
    assert_eq!(f.workspace.status().unwrap().revision, revision);
    assert!(f
        .workspace
        .lookup(
            new.serial,
            b"subtree-expanded",
            ReferenceScope::Local,
            deadline()
        )
        .is_err());
    let subtree = f.lookup(old.serial, b"subtree");
    assert_eq!(subtree.kind, NodeKind::Directory);
    for serial in [subtree.serial, old.serial, new.serial, packages.serial] {
        f.workspace.forget(serial, 1, ReferenceScope::Local);
    }
    f.workspace.close_clean().unwrap();
    println!("RENAME_BUDGET baseline_backing_bytes={baseline_bytes} quota_bytes={quota} extra_bytes=8192 refusal=atomic cleanup=PASS");
}

#[test]
fn a_successful_directory_rename_seals_once_and_refunds_exactly() {
    // The rename's publication seals its candidate exactly once: the sealed
    // candidate owns no pending cleanup, and its slot credits and reservations
    // return to the values the workspace held before the rename began. A
    // duplicate fallible seal could reject the prepared rename after the first
    // already published, so this pins the one-attempt shape from the public
    // surface: identical reserved slots and bytes before and after, a complete
    // accounting, and a clean close with no retained custody.
    let f = Fixture::new();
    let root = f.workspace.root().serial;
    let packages = f.lookup(root, b"packages");
    let old = f.lookup(packages.serial, b"old");
    let new = f.lookup(packages.serial, b"new");
    let subtree = f.lookup(old.serial, b"subtree");
    let before = f.workspace.metadata_status().unwrap();
    f.workspace
        .rename(
            old.serial,
            b"subtree",
            new.serial,
            b"subtree",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(f.lookup(new.serial, b"subtree").serial, subtree.serial);
    let sealed = f.workspace.metadata_status().unwrap();
    assert_eq!(
        sealed.reserved_slots, before.reserved_slots,
        "the sealed candidate refunds its slots exactly once: {before:?} -> {sealed:?}"
    );
    assert_eq!(
        sealed.reserved_bytes,
        layerfs_workspace::backing::metadata::ESCROW,
        "a rename that needs a completion retains exactly its escrow reservation"
    );
    assert!(
        sealed.accounting_complete,
        "a sealed root owns no pending cleanup"
    );
    f.commit();
    let completed = f.workspace.metadata_status().unwrap();
    assert_eq!(
        completed.reserved_bytes, before.reserved_bytes,
        "the completed rename releases its escrow exactly once: {before:?} -> {completed:?}"
    );
    assert_eq!(
        completed.reserved_slots, before.reserved_slots,
        "no slot credit was released twice across the completion"
    );
    // The moved directory carries both lookup references it acquired (once
    // before the rename, once when the moved name was resolved afterwards).
    for (serial, count) in [
        (packages.serial, 1),
        (old.serial, 1),
        (new.serial, 1),
        (subtree.serial, 2),
    ] {
        f.workspace.forget(serial, count, ReferenceScope::Local);
    }
    f.workspace.close_clean().unwrap();
    println!(
        "RENAME_SEAL once=PASS refund_slots={}->{} escrow={} ->{} cleanup=PASS",
        before.reserved_slots,
        sealed.reserved_slots,
        layerfs_workspace::backing::metadata::ESCROW,
        completed.reserved_bytes
    );
}

#[test]
fn growing_prefix_move_has_descendant_independent_private_counts() {
    let measure = |extra| {
        let f = Fixture::with_extra_children(extra);
        let top = f.workspace.root().serial;
        let packages = f.lookup(top, b"packages");
        let old = f.lookup(packages.serial, b"old");
        let new = f.lookup(packages.serial, b"new");
        let Response::Attributes {
            serial: subtree_serial,
            ..
        } = f.attributes(f.genesis, b"packages/old/subtree")
        else {
            panic!("source directory")
        };
        let before = f.workspace.status().unwrap();
        let before_backing = f.workspace.backing_status().unwrap();
        let before_metadata = f.workspace.metadata_status().unwrap();
        f.workspace
            .rename(
                old.serial,
                b"subtree",
                new.serial,
                b"subtree-expanded",
                RenameFlags::default(),
                deadline(),
            )
            .unwrap();
        let after = f.workspace.status().unwrap();
        let after_backing = f.workspace.backing_status().unwrap();
        let after_metadata = f.workspace.metadata_status().unwrap();
        assert_eq!(
            f.lookup(new.serial, b"subtree-expanded").serial,
            subtree_serial
        );
        let counters = (
            after.upstream_calls - before.upstream_calls,
            after_backing.metadata_reads - before_backing.metadata_reads,
            after_backing.metadata_writes - before_backing.metadata_writes,
            after_metadata.allocated_pages - before_metadata.allocated_pages,
            after_metadata.allocated_bytes - before_metadata.allocated_bytes,
            after_backing.allocated_bytes - before_backing.allocated_bytes,
            after.accounted_bytes - before.accounted_bytes,
            before.nodes,
        );
        let head = f.commit();
        let Response::Attributes {
            serial: old_serial,
            content: old_content,
            ..
        } = f.attributes(f.genesis, b"packages/old/subtree/sibling.txt")
        else {
            panic!("old file")
        };
        let Response::Attributes {
            serial: new_serial,
            content: new_content,
            ..
        } = f.attributes(head, b"packages/new/subtree-expanded/sibling.txt")
        else {
            panic!("moved file")
        };
        assert_eq!((new_serial, new_content), (old_serial, old_content));
        for (serial, count) in [
            (packages.serial, 1),
            (old.serial, 1),
            (new.serial, 1),
            (subtree_serial, 1),
        ] {
            f.workspace.forget(serial, count, ReferenceScope::Local);
        }
        f.workspace.close_clean().unwrap();
        counters
    };
    let small = measure(0);
    let large = measure(64);
    assert_eq!(small, large);
    println!(
        "INHERITED_MOVE_COST growing_prefix descendants=3,67 store_calls={} private_page_reads={} private_page_writes={} new_private_pages={} metadata_bytes={} payload_bytes={} accounted_memory_delta={} resident_nodes={} cleanup=PASS",
        small.0, small.1, small.2, small.3, small.4, small.5, small.6, small.7
    );
}

#[test]
fn fresh_upper_directory_move_is_the_control() {
    let f = Fixture::new();
    let top = f.workspace.root().serial;
    let packages = f.lookup(top, b"packages");
    let old = f.lookup(packages.serial, b"old");
    let new = f.lookup(packages.serial, b"new");
    let upper = f
        .workspace
        .mkdir(old.serial, b"upper", 0o755, 0, deadline())
        .unwrap();
    let file = f
        .workspace
        .mknod(upper.serial, b"file", 0o600, 0, deadline())
        .unwrap();
    f.workspace
        .rename(
            old.serial,
            b"upper",
            new.serial,
            b"upper",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(f.lookup(new.serial, b"upper").serial, upper.serial);
    assert_eq!(f.lookup(upper.serial, b"file").serial, file.serial);
    let root = f.commit();
    assert!(matches!(
        f.attributes(root, b"packages/new/upper/file"),
        Response::Attributes { serial, .. } if serial == file.serial
    ));
    assert_eq!(
        f.inspect(
            root,
            Inspect::Attributes {
                path: b"packages/old/upper".to_vec()
            }
        )
        .unwrap_err()
        .code,
        Code::PathNotFound
    );
}

#[test]
fn base_file_move_from_an_unmodified_root_keeps_its_identity() {
    let f = Fixture::new();
    let top = f.workspace.root().serial;
    let packages = f.lookup(top, b"packages");
    let old = f.lookup(packages.serial, b"old");
    let new = f.lookup(packages.serial, b"new");
    let subtree = f.lookup(old.serial, b"subtree");
    let sibling = f.lookup(subtree.serial, b"sibling.txt");
    f.workspace
        .rename(
            subtree.serial,
            b"sibling.txt",
            new.serial,
            b"sibling.txt",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(f.lookup(new.serial, b"sibling.txt").serial, sibling.serial);
    let root = f.commit();
    assert!(matches!(
        f.attributes(root, b"packages/new/sibling.txt"),
        Response::Attributes { serial, .. } if serial == sibling.serial
    ));
    assert_eq!(
        f.content_bytes(root, b"packages/new/sibling.txt"),
        b"sibling-base"
    );
}

#[test]
fn phase_a_selected_origins_overlap_zero_hardlinks_and_completion_credit() {
    let f = Fixture::new();
    let top = f.workspace.root().serial;
    let packages = f.lookup(top, b"packages");
    let old = f.lookup(packages.serial, b"old");
    let subtree = f.lookup(old.serial, b"subtree");
    let child = f.lookup(subtree.serial, b"child");
    let grand = f.lookup(child.serial, b"grand.txt");
    let handle = f.open(grand.serial);
    let write = |offset, bytes: &[u8]| {
        let payload = f
            .workspace
            .own_payload(bytes.len() as u64, &mut &bytes[..], deadline())
            .unwrap();
        f.workspace
            .write_file(handle, offset, &payload, deadline())
            .unwrap();
    };
    write(1, b"AB");
    assert_eq!(
        f.workspace.backing_status().unwrap().reserved_bytes,
        208 * 4096
    );
    let token = [73; 33];
    let lease = f.workspace.pin_view(token, deadline()).unwrap();
    let mut parent = lease.root.serial;
    for name in [b"packages".as_slice(), b"old", b"subtree", b"child"] {
        parent = f
            .workspace
            .view_lookup(&token, parent, name, deadline())
            .unwrap()
            .serial;
    }
    let selected = f
        .workspace
        .view_lookup(&token, parent, b"grand.txt", deadline())
        .unwrap();
    f.gate.arm();
    let first = std::thread::scope(|scope| {
        let saving = scope.spawn(|| f.commit());
        f.gate.wait();
        write(7, b"XY");
        assert!(f.workspace.backing_status().unwrap().reserved_bytes >= 208 * 4096);
        f.gate.release();
        saving.join().unwrap()
    });
    assert_eq!(
        f.content_bytes(first, b"packages/old/subtree/child/grand.txt"),
        b"gABnd-base"
    );
    assert_eq!(
        f.workspace
            .read(handle, 0, 10, deadline())
            .unwrap()
            .as_ref(),
        b"gABnd-bXYe"
    );
    assert_eq!(
        f.workspace
            .view_read(&token, selected.serial, 0, 10, deadline())
            .unwrap()
            .bytes,
        b"gABnd-base"
    );
    assert_eq!(
        f.workspace.backing_status().unwrap().reserved_bytes,
        208 * 4096
    );
    write(2, b"12345");
    f.workspace.set_len(grand.serial, 14, deadline()).unwrap();
    f.workspace
        .set_attributes(
            grand.serial,
            layerfs_workspace::PortableAttributes {
                mode: Some(0o640),
                mtime: Some((-7, 42)),
                ..Default::default()
            },
            deadline(),
        )
        .unwrap();
    let alias = f
        .workspace
        .link(child.serial, b"alias", grand.serial, deadline())
        .unwrap();
    assert_eq!(alias.serial, grand.serial);
    assert_eq!(f.workspace.getattr(grand.serial).unwrap().references, 2);
    f.workspace
        .unlink(child.serial, b"grand.txt", deadline())
        .unwrap();
    let second = f.commit();
    let mut expected = b"gA12345XYe".to_vec();
    expected.resize(14, 0);
    assert_eq!(
        f.content_bytes(second, b"packages/old/subtree/child/alias"),
        expected
    );
    assert_eq!(
        f.workspace
            .read(handle, 0, 20, deadline())
            .unwrap()
            .as_ref(),
        expected
    );
    assert_eq!(f.workspace.backing_status().unwrap().reserved_bytes, 0);
    assert!(matches!(
        f.workspace.release_view(&token).unwrap(),
        layerfs_workspace::ViewRelease::Completed
    ));
    f.workspace
        .unlink(child.serial, b"alias", deadline())
        .unwrap();
    assert_eq!(
        f.workspace
            .read(handle, 0, 20, deadline())
            .unwrap()
            .as_ref(),
        expected
    );
    f.commit();
    assert_eq!(
        f.workspace
            .read(handle, 0, 20, deadline())
            .unwrap()
            .as_ref(),
        expected
    );
    f.workspace.release(handle).unwrap();
    for serial in [
        packages.serial,
        old.serial,
        subtree.serial,
        child.serial,
        grand.serial,
    ] {
        f.workspace.forget(serial, u64::MAX, ReferenceScope::Local);
    }
    f.workspace.close_clean().unwrap();
    assert_eq!(f.workspace.backing_status().unwrap().allocated_bytes, 0);
    assert_eq!(f.workspace.backing_status().unwrap().reserved_bytes, 0);
}

#[test]
fn phase_a_lease_admission_frozen_names_and_checked_release() {
    let f = Fixture::new();
    f.workspace
        .set_attributes(
            f.workspace.root().serial,
            layerfs_workspace::PortableAttributes {
                mode: Some(0o700),
                mtime: Some((-9, 84)),
                ..Default::default()
            },
            deadline(),
        )
        .unwrap();
    let mut tokens = Vec::new();
    for index in 1..=32 {
        let token = [index; 33];
        let info = f.workspace.pin_view(token, deadline()).unwrap();
        assert_eq!(
            (
                info.root.mode,
                info.root.mtime_seconds,
                info.root.mtime_nanoseconds
            ),
            (0o700, -9, 84)
        );
        tokens.push(token);
    }
    assert!(matches!(
        f.workspace.pin_view([99; 33], deadline()),
        Err(WorkspaceError::Capacity)
    ));
    assert_eq!(f.workspace.held_view_leases().unwrap(), 32);
    assert!(matches!(
        f.workspace.close_clean(),
        Err(WorkspaceError::Busy)
    ));
    let top = f.workspace.root().serial;
    assert!(matches!(
        f.workspace
            .view_lookup(&[99; 33], top, b"packages", deadline()),
        Err(WorkspaceError::Denied)
    ));
    assert!(matches!(
        f.workspace
            .view_lookup(&tokens[1], 999, b"child", deadline()),
        Err(WorkspaceError::Denied)
    ));
    let held = f
        .workspace
        .view_lookup(&tokens[0], top, b"packages", deadline())
        .unwrap();
    let old = f
        .workspace
        .view_lookup(&tokens[0], held.serial, b"old", deadline())
        .unwrap();
    let subtree = f
        .workspace
        .view_lookup(&tokens[0], old.serial, b"subtree", deadline())
        .unwrap();
    let live_packages = f.lookup(top, b"packages");
    let live_old = f.lookup(live_packages.serial, b"old");
    let live_new = f.lookup(live_packages.serial, b"new");
    f.workspace
        .rename(
            live_old.serial,
            b"subtree",
            live_new.serial,
            b"moved",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    f.commit();
    assert_eq!(
        f.workspace
            .view_lookup(&tokens[0], old.serial, b"subtree", deadline())
            .unwrap()
            .serial,
        subtree.serial
    );
    assert_eq!(
        f.workspace
            .view_lookup(&tokens[0], subtree.serial, b"child", deadline())
            .unwrap()
            .kind,
        NodeKind::Directory
    );
    // Clean Commit still has to admit its own completion credit while old
    // selections remain held; it does not inherit G1's completed fund.
    f.commit();
    assert_eq!(f.workspace.backing_status().unwrap().reserved_bytes, 0);
    for token in tokens {
        assert!(matches!(
            f.workspace.release_view(&token).unwrap(),
            layerfs_workspace::ViewRelease::Completed
        ));
        assert!(matches!(
            f.workspace.view_status(&token),
            Err(WorkspaceError::Denied)
        ));
    }
    for serial in [live_packages.serial, live_old.serial, live_new.serial] {
        f.workspace.forget(serial, u64::MAX, ReferenceScope::Local);
    }
    f.workspace.close_clean().unwrap();
    assert_eq!(f.workspace.backing_status().unwrap().allocated_bytes, 0);
}

#[test]
fn phase_a_fresh_nonfile_removal_replacement_and_later_symlink_move() {
    let f = Fixture::new();
    let top = f.workspace.root().serial;
    let doomed = f
        .workspace
        .mkdir(top, b"doomed", 0o755, 0, deadline())
        .unwrap();
    let frozen = f
        .workspace
        .opendir(doomed.serial, ReferenceScope::Local)
        .unwrap();
    f.workspace.rmdir(top, b"doomed", deadline()).unwrap();
    let target = f
        .workspace
        .symlink(top, b"gone-link", b"opaque-target", deadline())
        .unwrap();
    f.workspace.unlink(top, b"gone-link", deadline()).unwrap();
    let source = f
        .workspace
        .mkdir(top, b"from", 0o755, 0, deadline())
        .unwrap();
    let replaced = f.workspace.mkdir(top, b"to", 0o755, 0, deadline()).unwrap();
    f.workspace
        .rename(top, b"from", top, b"to", RenameFlags::default(), deadline())
        .unwrap();
    let root = f.commit();
    assert_eq!(f.lookup(top, b"to").serial, source.serial);
    for path in [b"doomed".as_slice(), b"gone-link", b"from"] {
        assert_eq!(
            f.inspect(
                root,
                Inspect::Attributes {
                    path: path.to_vec()
                }
            )
            .unwrap_err()
            .code,
            Code::PathNotFound
        );
    }
    assert!(matches!(
        f.workspace.mkdir(doomed.serial, b"x", 0o755, 0, deadline()),
        Err(WorkspaceError::NotFound)
    ));
    f.workspace.releasedir(frozen).unwrap();
    let link = f
        .workspace
        .symlink(top, b"link", b"opaque-target", deadline())
        .unwrap();
    f.commit();
    f.workspace
        .rename(
            top,
            b"link",
            top,
            b"moved-link",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    let final_root = f.commit();
    assert_eq!(
        f.workspace
            .readlink(link.serial, deadline())
            .unwrap()
            .as_ref(),
        b"opaque-target"
    );
    assert_eq!(
        f.inspect(
            final_root,
            Inspect::InodeReadlink {
                serial: link.serial
            }
        )
        .unwrap(),
        Response::Link(b"opaque-target".to_vec())
    );
    for serial in [
        doomed.serial,
        target.serial,
        source.serial,
        replaced.serial,
        link.serial,
    ] {
        f.workspace.forget(serial, u64::MAX, ReferenceScope::Local);
    }
    f.workspace.close_clean().unwrap();
    assert_eq!(f.workspace.backing_status().unwrap().allocated_bytes, 0);
}

#[test]
fn phase_a_exact_capability_refuses_before_dependent_inspection() {
    let f = Fixture::new();
    for version in [1, 3] {
        let path = f._temp.0.join(format!("incompatible-{version}"));
        fs::create_dir(&path).unwrap();
        let metadata = fs::metadata(&path).unwrap();
        let calls = Arc::new(AtomicU64::new(0));
        let seen = calls.clone();
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root: path.clone(),
                max_count: 1,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: Some(64 * 1024 * 1024),
            },
            Arc::new(move |request, _, _, _| {
                assert!(matches!(request.operation, Operation::FileSaveCapabilities));
                seen.fetch_add(1, Ordering::AcqRel);
                Ok(Response::FileSaveCapabilities { version })
            }),
        )
        .unwrap();
        let result = host.attach(
            AttachOptions {
                id: "incompatible".into(),
                incarnation: [version; 32],
                store: f.server.store(),
                base: Base::Branch([17; 17]),
                access: WorkspaceAccess::LocalEdit,
                owner_uid: metadata.uid(),
                owner_gid: metadata.gid(),
            },
            deadline(),
        );
        assert!(matches!(result, Err(WorkspaceError::Unsupported)));
        assert_eq!(calls.load(Ordering::Acquire), 1);
        assert!(!path.join("private-backing/incompatible").exists());
        assert!(!path.join("workspace/incompatible").exists());
    }
}

#[test]
fn phase_a_known_save_and_unknown_commit_keep_custody_without_replay() {
    for fault in [1, 2] {
        let f = Fixture::new();
        let top = f.workspace.root().serial;
        let packages = f.lookup(top, b"packages");
        let old = f.lookup(packages.serial, b"old");
        let subtree = f.lookup(old.serial, b"subtree");
        let child = f.lookup(subtree.serial, b"child");
        let grand = f.lookup(child.serial, b"grand.txt");
        let handle = f.open(grand.serial);
        let payload = f
            .workspace
            .own_payload(2, &mut b"AB".as_slice(), deadline())
            .unwrap();
        f.workspace
            .write_file(handle, 1, &payload, deadline())
            .unwrap();
        f.fault.store(fault, Ordering::Release);
        let error = f.workspace.commit(deadline()).unwrap_err();
        match error {
            WorkspaceError::Commit(failure) if fault == 2 => {
                assert_eq!(
                    failure.disposition,
                    layerfs_workspace::CommitFailureDisposition::Unknown
                );
                assert!(failure.known_outcome.is_none());
                assert!(failure.installed_revision.is_none());
            }
            WorkspaceError::Commit(commit) if fault == 1 => {
                assert_eq!(
                    commit.disposition,
                    layerfs_workspace::CommitFailureDisposition::KnownBeforeCommit
                );
                let WorkspaceError::Stage(failure) = &commit.cause else {
                    panic!("stage custody missing");
                };
                let known = failure
                    .pending
                    .as_ref()
                    .expect("known file root survives denied metadata");
                assert_eq!(known.serial, grand.serial);
                assert!(known.metadata.is_none());
                assert!(f.inspect(known.content, Inspect::File).is_ok());
            }
            other => panic!("unexpected failure custody: {other:?}"),
        }
        assert!(f.workspace.status().unwrap().submission.is_some());
        assert!(matches!(
            f.workspace.commit(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(
            f.canonical_calls.load(Ordering::Acquire),
            u64::from(fault == 2)
        );
        assert!(matches!(
            f.workspace.close_clean(),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(
            f.workspace
                .read(handle, 0, 10, deadline())
                .unwrap()
                .as_ref(),
            b"gABnd-base"
        );
        let status = f.workspace.backing_status().unwrap();
        assert!(
            status.allocated_bytes > 0 && status.reserved_bytes > 0 && status.accounting_complete
        );
        println!("PHASE_A_RETAINED fault={fault} allocated={} reserved={} selector_retained=true canonical_calls={}", status.allocated_bytes, status.reserved_bytes, f.canonical_calls.load(Ordering::Acquire));
        // This external proof leaves the failed owner's backing for inspection.
        std::mem::forget(f);
    }
}

fn deny_syscall_on_this_thread(number: u32) {
    #[repr(C)]
    struct Filter {
        code: u16,
        jt: u8,
        jf: u8,
        value: u32,
    }
    #[repr(C)]
    struct Program {
        len: u16,
        filters: *const Filter,
    }
    unsafe extern "C" {
        fn prctl(option: i32, ...) -> i32;
    }
    let filters = [
        Filter {
            code: 0x20,
            jt: 0,
            jf: 0,
            value: 0,
        },
        Filter {
            code: 0x15,
            jt: 0,
            jf: 1,
            value: number,
        },
        Filter {
            code: 0x06,
            jt: 0,
            jf: 0,
            value: 0x0005_0005,
        },
        Filter {
            code: 0x06,
            jt: 0,
            jf: 0,
            value: 0x7fff_0000,
        },
    ];
    let program = Program {
        len: filters.len() as u16,
        filters: filters.as_ptr(),
    };
    // External Linux fault applies only to this worker. The parent resumes
    // locally through the same native selector, with no mutation replay.
    unsafe {
        assert_eq!(prctl(38, 1usize, 0usize, 0usize, 0usize), 0);
        assert_eq!(prctl(22, 2usize, &program, 0usize, 0usize), 0);
    }
}

#[test]
fn phase_a_known_canonical_local_failure_resumes_same_selector_once() {
    let f = Fixture::new();
    let top = f.workspace.root().serial;
    let packages = f.lookup(top, b"packages");
    let old = f.lookup(packages.serial, b"old");
    let subtree = f.lookup(old.serial, b"subtree");
    let child = f.lookup(subtree.serial, b"child");
    let grand = f.lookup(child.serial, b"grand.txt");
    let handle = f.open(grand.serial);
    let payload = f
        .workspace
        .own_payload(2, &mut b"AB".as_slice(), deadline())
        .unwrap();
    f.workspace
        .write_file(handle, 1, &payload, deadline())
        .unwrap();
    drop(payload);
    let stage = f.workspace.stage(deadline()).unwrap();
    f.fault.store(3, Ordering::Release);
    let error = std::thread::scope(|scope| {
        scope
            .spawn(|| f.workspace.commit_staged(&stage, deadline()))
            .join()
            .unwrap()
    })
    .unwrap_err();
    let WorkspaceError::Commit(failure) = error else {
        panic!("local C5 failure")
    };
    assert_eq!(
        failure.disposition,
        layerfs_workspace::CommitFailureDisposition::KnownCommitLocalFailure
    );
    assert!(failure.known_outcome.is_some());
    assert!(failure.installed_revision.is_none());
    assert!(f.workspace.status().unwrap().submission.is_some());
    assert_eq!(f.canonical_calls.load(Ordering::Acquire), 1);
    f.fault.store(0, Ordering::Release);
    let complete = f.workspace.commit_staged(&stage, deadline()).unwrap();
    assert_eq!(f.canonical_calls.load(Ordering::Acquire), 1);
    let root = match complete.outcome {
        CommitOutcomeWire::Committed(commit) => commit.root,
        CommitOutcomeWire::UpToDate { root, .. } => root,
    };
    assert_eq!(
        f.content_bytes(root, b"packages/old/subtree/child/grand.txt"),
        b"gABnd-base"
    );
    assert_eq!(f.workspace.backing_status().unwrap().reserved_bytes, 0);
    f.workspace.release(handle).unwrap();
    for serial in [
        packages.serial,
        old.serial,
        subtree.serial,
        child.serial,
        grand.serial,
    ] {
        f.workspace.forget(serial, u64::MAX, ReferenceScope::Local);
    }
    f.workspace.close_clean().unwrap();
    assert_eq!(f.workspace.backing_status().unwrap().allocated_bytes, 0);
}

#[test]
fn phase_a_large_payload_append_resize_and_backward_base_copy() {
    let f = Fixture::new();
    let top = f.workspace.root().serial;
    let file = f
        .workspace
        .mknod(top, b"bytes", 0o640, 0, deadline())
        .unwrap();
    let handle = f.open(file.serial);
    let mut expected: Vec<u8> = (0..512).map(|index| (index % 251) as u8).collect();
    let payload = f
        .workspace
        .own_payload(512, &mut expected.as_slice(), deadline())
        .unwrap();
    f.workspace
        .write_file(handle, 0, &payload, deadline())
        .unwrap();
    drop(payload);
    let before = f.commit();
    assert_eq!(f.content_bytes(before, b"bytes"), expected);
    let token = [86; 33];
    let lease = f.workspace.pin_view(token, deadline()).unwrap();
    let selected = f
        .workspace
        .view_lookup(&token, lease.root.serial, b"bytes", deadline())
        .unwrap();
    let read = f
        .workspace
        .read(handle, 64, 32, deadline())
        .unwrap()
        .as_ref()
        .to_vec();
    let payload = f
        .workspace
        .own_payload(32, &mut read.as_slice(), deadline())
        .unwrap();
    f.workspace
        .write_file(handle, 0, &payload, deadline())
        .unwrap();
    drop(payload);
    expected[..32].copy_from_slice(&read);
    let append = f
        .workspace
        .open_file(
            file.serial,
            FileOpenOptions {
                access: FileAccess::ReadWrite,
                append: true,
                truncate: false,
            },
            ReferenceScope::Local,
            deadline(),
        )
        .unwrap();
    let payload = f
        .workspace
        .own_payload(3, &mut b"end".as_slice(), deadline())
        .unwrap();
    f.workspace
        .write_file(append, u64::MAX, &payload, deadline())
        .unwrap();
    drop(payload);
    expected.extend_from_slice(b"end");
    assert_eq!(
        f.workspace
            .read(handle, 0, 600, deadline())
            .unwrap()
            .as_ref(),
        expected
    );
    f.workspace.set_len(file.serial, 100, deadline()).unwrap();
    expected.truncate(100);
    f.workspace.set_len(file.serial, 110, deadline()).unwrap();
    expected.resize(110, 0);
    let bytes = b"0123456789abcdef";
    let payload = f
        .workspace
        .own_payload(bytes.len() as u64, &mut bytes.as_slice(), deadline())
        .unwrap();
    f.workspace
        .write_file(handle, 95, &payload, deadline())
        .unwrap();
    drop(payload);
    expected.resize(111, 0);
    expected[95..111].copy_from_slice(bytes);
    let after = f.commit();
    assert_eq!(f.content_bytes(after, b"bytes"), expected);
    assert_eq!(
        f.workspace
            .view_read(&token, selected.serial, 0, 600, deadline())
            .unwrap()
            .bytes,
        (0..512)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>()
    );
    f.workspace.release_view(&token).unwrap();
    f.workspace.release(append).unwrap();
    f.workspace.release(handle).unwrap();
    f.workspace
        .forget(file.serial, u64::MAX, ReferenceScope::Local);
    f.workspace.close_clean().unwrap();
    assert_eq!(f.workspace.backing_status().unwrap().allocated_bytes, 0);
}

fn deny_fallocate_on_this_thread() {
    #[cfg(target_arch = "aarch64")]
    let number = 47;
    #[cfg(target_arch = "x86_64")]
    let number = 285;
    deny_syscall_on_this_thread(number);
}

#[test]
fn phase_a_namespace_cleanup_error_keeps_published_revision_and_identity() {
    let f = Fixture::new();
    let root = f.workspace.root().serial;
    f.workspace
        .mkdir(root, b"first", 0o755, 0, deadline())
        .unwrap();
    let before = f.workspace.status().unwrap().revision;
    let error = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                #[cfg(target_arch = "aarch64")]
                let number = 35;
                #[cfg(target_arch = "x86_64")]
                let number = 263;
                deny_syscall_on_this_thread(number);
                f.workspace.mkdir(root, b"accepted", 0o755, 0, deadline())
            })
            .join()
            .unwrap()
    })
    .unwrap_err();
    let WorkspaceError::Published {
        receipt,
        published_handle,
        ..
    } = error
    else {
        panic!("published cleanup custody: {error:?}")
    };
    assert!(published_handle.is_none());
    assert_eq!(receipt.revision, before + 1);
    assert_eq!(f.workspace.status().unwrap().revision, receipt.revision);
    assert_eq!(
        f.workspace.getattr(receipt.inode).unwrap().kind,
        NodeKind::Directory
    );
    assert!(f.workspace.backing_status().unwrap().allocated_bytes > 0);
    assert!(matches!(
        f.workspace.close_clean(),
        Err(WorkspaceError::Busy)
    ));
    println!("PHASE_A_PUBLISHED cleanup_failed=true accepted_revision={} identity={} owner_retained=true", receipt.revision, receipt.inode);
    std::mem::forget(f);
}
