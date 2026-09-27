//! Public Workspace operations against the production in-process Service.
#![cfg(target_os = "linux")]

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
        let root = std::env::var_os("LAYERFS_TEST_BACKING_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.canonicalize().unwrap().join(format!(
            "layerfs-inherited-workspace-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let temp = Temp(path.clone());
        let source = path.join("source");
        fs::create_dir_all(source.join("packages/old/subtree/child")).unwrap();
        fs::create_dir(source.join("packages/new")).unwrap();
        fs::write(
            source.join("packages/old/subtree/child/grand.txt"),
            b"grand-base",
        )
        .unwrap();
        fs::write(
            source.join("packages/old/subtree/sibling.txt"),
            b"sibling-base",
        )
        .unwrap();
        for index in 0..extra {
            fs::write(
                source.join(format!("packages/old/subtree/child/extra{index:03}.txt")),
                b"x",
            )
            .unwrap();
        }
        if deep {
            let mut path = source.join("src");
            for _ in 0..15 {
                path.push("d".repeat(250));
            }
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("leaf"), b"deep-base").unwrap();
        }
        let server = Arc::new(
            Server::create(ServerConfig {
                store_path: path.join("store.sqlite"),
                history_path: path.join("history.sqlite"),
                binding_key: b"inherited-workspace".to_vec(),
                incarnation: 1,
                cursor_key: [53; 32],
                history: HistoryMode::Create,
                service_host: "127.0.0.1".into(),
                runtime: Runtime::disabled(),
                telemetry_run: None,
            })
            .unwrap(),
        );
        let project = ProjectApi::new(&server).init("inherited", &source).unwrap();
        let branch = ProjectApi::new(&server)
            .fork(&project, [54; 16], "main")
            .unwrap();
        let gate = Arc::new(CommitGate::default());
        let delivery_server = server.clone();
        let delivery_gate = gate.clone();
        let delivery = Arc::new(
            move |request: &Request,
                  input: &mut dyn Source,
                  output: &mut dyn io::Write,
                  end: Instant| {
                let mut reader = SourceReader {
                    source: input,
                    end,
                    cancel: AtomicBool::new(false),
                };
                let response = delivery_server
                    .service()
                    .handle_until(&delivery_server.peer()?, request, &mut reader, output, end)
                    .0?;
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
                disk_budget_bytes: Some(64 * 1024 * 1024),
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
                    base: Base::Branch(branch.id),
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
fn growing_move_checks_near_and_overlong_cached_descendant_before_publication() {
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
    let revision = f.workspace.status().unwrap().revision;
    assert_eq!(
        f.workspace.rename(
            top,
            b"src",
            target.serial,
            b"mmmmmmm",
            RenameFlags::default(),
            deadline(),
        ),
        Err(WorkspaceError::Capacity)
    );
    assert_eq!(f.workspace.status().unwrap().revision, revision);
    assert_eq!(f.lookup(top, b"src").serial, source.serial);
    assert!(f
        .workspace
        .lookup(target.serial, b"mmmmmmm", ReferenceScope::Local, deadline())
        .is_err());
    let after = f.commit();
    assert!(
        matches!(f.attributes(before, b"src"), Response::Attributes { serial, .. } if serial == source.serial)
    );
    assert!(
        matches!(f.attributes(after, b"src"), Response::Attributes { serial, .. } if serial == source.serial)
    );
}

#[test]
fn growing_inherited_move_checks_uncached_descendant_before_publication() {
    let f = Fixture::with_layout(0, true);
    let top = f.workspace.root().serial;
    let source = f.lookup(top, b"src");
    let first = f
        .workspace
        .mkdir(top, &vec![b'a'; 255], 0o755, 0, deadline())
        .unwrap();
    let target = f
        .workspace
        .mkdir(first.serial, &[b'b'; 63], 0o755, 0, deadline())
        .unwrap();
    let revision = f.workspace.status().unwrap().revision;
    assert_eq!(
        f.workspace.rename(
            top,
            b"src",
            target.serial,
            b"mmmmmmm",
            RenameFlags::default(),
            deadline(),
        ),
        Err(WorkspaceError::Capacity)
    );
    assert_eq!(f.workspace.status().unwrap().revision, revision);
    assert_eq!(f.lookup(top, b"src").serial, source.serial);
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
fn equal_length_move_uses_changed_paths_and_resident_pins() {
    let measure = |extra| {
        let f = Fixture::with_extra_children(extra);
        let top = f.workspace.root().serial;
        let packages = f.lookup(top, b"packages");
        let old = f.lookup(packages.serial, b"old");
        let new = f.lookup(packages.serial, b"new");
        let subtree = f.lookup(old.serial, b"subtree");
        let before = f.workspace.status().unwrap();
        let before_backing = f.workspace.backing_status().unwrap();
        let before_metadata = f.workspace.metadata_status().unwrap();
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
        let after = f.workspace.status().unwrap();
        let after_backing = f.workspace.backing_status().unwrap();
        let after_metadata = f.workspace.metadata_status().unwrap();
        assert_eq!(f.lookup(new.serial, b"subtree").serial, subtree.serial);
        let counters = (
            after.upstream_calls - before.upstream_calls,
            after_backing.metadata_reads - before_backing.metadata_reads,
            after_metadata.allocated_bytes - before_metadata.allocated_bytes,
            after_backing.allocated_bytes - before_backing.allocated_bytes,
            after.accounted_bytes - before.accounted_bytes,
            before.nodes,
        );
        f.commit();
        for (serial, count) in [
            (packages.serial, 1),
            (old.serial, 1),
            (new.serial, 1),
            (subtree.serial, 2),
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
        "INHERITED_MOVE_COST descendants=3,67 store_calls={} metadata_page_reads={} metadata_bytes={} payload_bytes={} accounted_memory_delta={} resident_nodes={} cleanup=PASS",
        small.0, small.1, small.2, small.3, small.4, small.5
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
