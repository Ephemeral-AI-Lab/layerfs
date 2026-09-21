use layerfs_bridge::contract::*;
use layerfs_workspace::*;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture {
    path: PathBuf,
    host: WorkspaceHost,
    fail_read: Arc<AtomicBool>,
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn attr(serial: u64, kind: u8, size: u64) -> Response {
    Response::Attributes {
        serial,
        kind,
        references: if serial == 7 { 0 } else { 1 },
        content: [serial as u8; 32],
        metadata: [1; 32],
        mode: if kind == 3 { 0o777 } else { 0o755 },
        mtime: -1,
        nanoseconds: 125,
        size,
    }
}
impl Fixture {
    fn new(max_count: usize) -> Self {
        Self::with_mode(max_count, 0o755)
    }
    fn with_mode(max_count: usize, mode: u32) -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "layerfs-workspace-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).unwrap();
        let fail_read = Arc::new(AtomicBool::new(false));
        let fail = fail_read.clone();
        let deliver: OperationDelivery = Arc::new(move |request, _, output, _| {
            let mut result = match &request.operation {
                Operation::Inspect {
                    query: Inspect::Attributes { path },
                    ..
                } => match path.as_slice() {
                    b"" => Ok(attr(7, 2, 0)),
                    b"file" | b"alias" => Ok(attr(8, 1, 5)),
                    b"link" => Ok(attr(9, 3, 4)),
                    _ => Err(Code::PathNotFound.into()),
                },
                Operation::Inspect {
                    query: Inspect::Readlink { path },
                    ..
                } if path == b"link" => Ok(Response::Link(b"file".to_vec())),
                Operation::Inspect {
                    query: Inspect::List { after, entries, .. },
                    ..
                } => {
                    let mut names: Vec<_> = [
                        (b"alias".to_vec(), 8),
                        (b"file".to_vec(), 8),
                        (b"link".to_vec(), 9),
                    ]
                    .into_iter()
                    .filter(|(name, _)| name > after)
                    .collect();
                    let more = names.len() > usize::from(*entries);
                    names.truncate(usize::from(*entries));
                    let continuation = if more {
                        names.last().map(|(name, _)| name.clone())
                    } else {
                        None
                    };
                    Ok(Response::List {
                        entries: names,
                        continuation,
                    })
                }
                Operation::ReadFile { root, start, end } if *root == [8; 32] => {
                    output.write_all(&b"hello"[*start as usize..*end as usize])?;
                    if fail.load(Ordering::Acquire) {
                        return Err(Code::Io.into());
                    }
                    Ok(Response::Read {
                        length: end - start,
                    })
                }
                Operation::HistoryQuery(HistoryQuery::GetBranch { branch }) => {
                    Ok(Response::History(Box::new(HistoryResult::BranchSnapshot(
                        BranchSnapshotWire {
                            branch: BranchWire {
                                branch: branch.as_slice().try_into().unwrap(),
                                stack: [0x31; STACK_BYTES],
                                name: b"main".to_vec(),
                                base_layer: [0x32; LAYER_BYTES],
                                head_commit: None,
                            },
                            head_root: None,
                            base_root: [1; 32],
                            effective_root: [1; 32],
                            root_serial: Some(7),
                            scope: [2; 32],
                            profile: [3; 32],
                        },
                    ))))
                }
                _ => Err(Code::Unsupported.into()),
            };
            if let Ok(Response::Attributes {
                mode: actual, kind, ..
            }) = &mut result
            {
                if *kind != 3 {
                    *actual = mode;
                }
            }
            result
        });
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root: path.clone(),
                max_count,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: None,
            },
            deliver,
        )
        .unwrap();
        Self {
            path,
            host,
            fail_read,
        }
    }
    fn options(&self, id: &str, value: u8) -> AttachOptions {
        #[cfg(unix)]
        let owner_uid = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(&self.path).unwrap().uid()
        };
        #[cfg(not(unix))]
        let owner_uid = 1000;
        AttachOptions {
            id: id.into(),
            incarnation: [value; 32],
            store: 1,
            base: Base::Root([1; 32]),
            owner_uid,
            owner_gid: 1000,
        }
    }
    fn attach(&self) -> Workspace {
        self.host
            .attach(self.options("example", 1), deadline())
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn immutable_reads_handles_eof_and_terminal_failure() {
    let fixture = Fixture::new(2);
    let workspace = fixture.attach();
    assert_eq!(workspace.root().serial, 7);
    let file = workspace
        .lookup(7, b"file", ReferenceScope::Local, deadline())
        .unwrap();
    assert_eq!(file.mtime_seconds, -1);
    assert_eq!(
        workspace
            .lookup(7, b"alias", ReferenceScope::Local, deadline())
            .unwrap(),
        file
    );
    assert_eq!(workspace.status().unwrap().nodes, 2);
    let handle = workspace.open(file.serial, ReferenceScope::Local).unwrap();
    workspace.forget(file.serial, u64::MAX, ReferenceScope::Local);
    assert_eq!(workspace.handle_attributes(handle).unwrap(), file);
    assert_eq!(
        workspace.read(handle, 1, 100, deadline()).unwrap().as_ref(),
        b"ello"
    );
    assert!(workspace
        .read(handle, u64::MAX, 128, deadline())
        .unwrap()
        .as_ref()
        .is_empty());
    assert!(matches!(
        workspace.read(handle, 0, MAX_READ_BYTES + 1, deadline()),
        Err(WorkspaceError::Capacity)
    ));
    fixture.fail_read.store(true, Ordering::Release);
    assert!(matches!(
        workspace.read(handle, 0, 5, deadline()),
        Err(WorkspaceError::Service(Failure { code: Code::Io, .. }))
    ));
    assert_eq!(workspace.status().unwrap().active_operations, 0);
    fixture.fail_read.store(false, Ordering::Release);
    assert_eq!(
        workspace.read(handle, 0, 5, deadline()).unwrap().as_ref(),
        b"hello"
    );
    workspace.flush(handle).unwrap();
    workspace.flush(handle).unwrap();
    workspace.release(handle).unwrap();
    assert!(matches!(
        workspace.release(handle),
        Err(WorkspaceError::BadHandle)
    ));
    workspace.close_clean().unwrap();
}

#[test]
fn directory_cookies_are_stable_and_bound_to_handles() {
    let fixture = Fixture::new(1);
    let workspace = fixture.attach();
    let directory = workspace.opendir(7, ReferenceScope::Local).unwrap();
    let first = workspace.readdir(directory, 0, 3, deadline()).unwrap();
    assert_eq!(
        first
            .entries()
            .iter()
            .map(|entry| entry.name.as_slice())
            .collect::<Vec<_>>(),
        [b".".as_slice(), b"..", b"alias"]
    );
    let cookie = first.entries().last().unwrap().cookie;
    drop(first);
    let second = workspace.readdir(directory, cookie, 3, deadline()).unwrap();
    assert_eq!(
        second
            .entries()
            .iter()
            .map(|entry| entry.name.as_slice())
            .collect::<Vec<_>>(),
        [b"file".as_slice(), b"link"]
    );
    let end = second.entries().last().unwrap().cookie;
    drop(second);
    let repeat = workspace.readdir(directory, cookie, 3, deadline()).unwrap();
    assert_eq!(repeat.entries().last().unwrap().cookie, end);
    drop(repeat);
    assert!(workspace
        .readdir(directory, end, 3, deadline())
        .unwrap()
        .entries()
        .is_empty());
    let other = workspace.opendir(7, ReferenceScope::Local).unwrap();
    assert!(matches!(
        workspace.readdir(other, cookie, 3, deadline()),
        Err(WorkspaceError::InvalidInput)
    ));
    workspace.releasedir(directory).unwrap();
    workspace.releasedir(other).unwrap();
    assert_eq!(workspace.status().unwrap().cookies, 0);
    workspace.close_clean().unwrap();
}

#[test]
fn mount_lease_reply_ownership_admission_and_clean_close() {
    let fixture = Fixture::new(1);
    let workspace = fixture.attach();
    assert!(matches!(
        fixture
            .host
            .attach(fixture.options("second", 2), deadline()),
        Err(WorkspaceError::Capacity)
    ));
    let mut lease = workspace.reserve_mount().unwrap();
    assert!(matches!(
        workspace.reserve_mount(),
        Err(WorkspaceError::Busy)
    ));
    let local = workspace
        .lookup(7, b"link", ReferenceScope::Local, deadline())
        .unwrap();
    let file = workspace
        .lookup(7, b"file", ReferenceScope::Projection, deadline())
        .unwrap();
    let handle = workspace.open(file.serial, ReferenceScope::Local).unwrap();
    let reply = workspace.read(handle, 0, 5, deadline()).unwrap();
    assert_eq!(workspace.status().unwrap().active_operations, 1);
    assert!(matches!(
        workspace.lookup(7, b"link", ReferenceScope::Local, deadline()),
        Err(WorkspaceError::Busy)
    ));
    lease.stop_admission();
    assert!(matches!(lease.finish(), Err(WorkspaceError::Busy)));
    assert_eq!(workspace.handle_attributes(handle).unwrap(), file);
    drop(reply);
    workspace.release(handle).unwrap();
    lease.finish().unwrap();
    assert_eq!(workspace.getattr(local.serial).unwrap(), local);
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
    workspace.forget(local.serial, 1, ReferenceScope::Local);
    workspace.close_clean().unwrap();
    fixture
        .host
        .attach(fixture.options("second", 2), deadline())
        .unwrap()
        .close_clean()
        .unwrap();
}

#[test]
fn branch_symlink_permission_and_failed_attach_are_explicit() {
    let fixture = Fixture::new(1);
    let mut invalid = fixture.options("../bad", 1);
    assert!(matches!(
        fixture.host.attach(invalid.clone(), deadline()),
        Err(WorkspaceError::InvalidInput)
    ));
    invalid.id = "branch".into();
    invalid.base = Base::Branch([0x11; BRANCH_BYTES]);
    let workspace = fixture.host.attach(invalid, deadline()).unwrap();
    let link = workspace
        .lookup(7, b"link", ReferenceScope::Local, deadline())
        .unwrap();
    assert_eq!(
        workspace
            .readlink(link.serial, deadline())
            .unwrap()
            .as_ref(),
        b"file"
    );
    assert!(matches!(
        workspace.open(link.serial, ReferenceScope::Local),
        Err(WorkspaceError::WrongKind)
    ));
    assert!(matches!(
        workspace.access(7, workspace.root().uid, 1000, 2),
        Err(WorkspaceError::ReadOnly)
    ));
    assert!(matches!(
        workspace.access(7, workspace.root().uid + 1, 1000, 4),
        Err(WorkspaceError::Denied)
    ));
    assert!(matches!(
        workspace.lookup(7, b"absent", ReferenceScope::Local, deadline()),
        Err(WorkspaceError::Service(Failure {
            code: Code::PathNotFound,
            ..
        }))
    ));
    workspace.forget(link.serial, 1, ReferenceScope::Local);
    workspace.close_clean().unwrap();
}

#[test]
fn full_cookie_table_still_replays_existing_positions() {
    let mut fixture = Fixture::new(1);
    let deliver: OperationDelivery = Arc::new(|request, _, _, _| match &request.operation {
        Operation::Inspect {
            query: Inspect::Attributes { path },
            ..
        } if path.is_empty() => Ok(attr(7, 2, 0)),
        Operation::Inspect {
            query: Inspect::Attributes { path },
            ..
        } => {
            let index: u64 = std::str::from_utf8(path).unwrap().parse().unwrap();
            Ok(attr(index + 8, 1, 5))
        }
        Operation::Inspect {
            query: Inspect::List { after, entries, .. },
            ..
        } => {
            let start = if after.is_empty() {
                0
            } else {
                std::str::from_utf8(after).unwrap().parse::<u64>().unwrap() + 1
            };
            let end = (start + u64::from(*entries)).min(1022);
            let names: Vec<_> = (start..end)
                .map(|index| (format!("{index:04}").into_bytes(), index + 8))
                .collect();
            let continuation = if end < 1022 {
                names.last().map(|(name, _)| name.clone())
            } else {
                None
            };
            Ok(Response::List {
                entries: names,
                continuation,
            })
        }
        _ => Err(Code::Unsupported.into()),
    });
    fixture.host = WorkspaceHost::new(
        WorkspaceConfig {
            root: fixture.path.clone(),
            max_count: 1,
            memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
            disk_budget_bytes: None,
        },
        deliver,
    )
    .unwrap();
    let workspace = fixture.attach();
    let directory = workspace.opendir(7, ReferenceScope::Local).unwrap();
    let mut cookie = 0;
    loop {
        let page = workspace
            .readdir(directory, cookie, 128, deadline())
            .unwrap();
        let Some(last) = page.entries().last() else {
            break;
        };
        cookie = last.cookie;
    }
    assert_eq!(workspace.status().unwrap().cookies, 1024);
    let page = workspace.readdir(directory, 0, 128, deadline()).unwrap();
    assert_eq!(page.entries()[0].name, b".");
    assert_eq!(workspace.status().unwrap().cookies, 1024);
    drop(page);
    workspace.releasedir(directory).unwrap();
    workspace.close_clean().unwrap();
}

#[cfg(unix)]
#[test]
fn root_owner_reads_mode_zero_but_execute_requires_an_execute_bit() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::with_mode(1, 0);
    if fs::metadata(&fixture.path).unwrap().uid() != 0 {
        eprintln!("NOT_RUN: root DAC assertions require an actual UID0 execution owner");
        return;
    }
    let workspace = fixture.attach();
    workspace.access(7, 0, 0, 5).unwrap();
    let directory = workspace.opendir(7, ReferenceScope::Local).unwrap();
    workspace.releasedir(directory).unwrap();
    let file = workspace
        .lookup(7, b"file", ReferenceScope::Local, deadline())
        .unwrap();
    workspace.access(file.serial, 0, 0, 4).unwrap();
    assert!(matches!(
        workspace.access(file.serial, 0, 0, 1),
        Err(WorkspaceError::Denied)
    ));
    assert!(matches!(
        workspace.access(file.serial, 1, 0, 4),
        Err(WorkspaceError::Denied)
    ));
    let handle = workspace.open(file.serial, ReferenceScope::Local).unwrap();
    assert_eq!(
        workspace.read(handle, 0, 5, deadline()).unwrap().as_ref(),
        b"hello"
    );
    workspace.release(handle).unwrap();
    workspace.forget(file.serial, 1, ReferenceScope::Local);
    workspace.close_clean().unwrap();

    let fixture = Fixture::with_mode(1, 0o001);
    let workspace = fixture.attach();
    let file = workspace
        .lookup(7, b"file", ReferenceScope::Local, deadline())
        .unwrap();
    workspace.access(file.serial, 0, 0, 5).unwrap();
    let handle = workspace.open(file.serial, ReferenceScope::Local).unwrap();
    workspace.release(handle).unwrap();
    workspace.forget(file.serial, 1, ReferenceScope::Local);
    workspace.close_clean().unwrap();
}

#[test]
fn detach_waits_for_projection_handles_and_preserves_local_handles() {
    let fixture = Fixture::new(1);
    let workspace = fixture.attach();
    let file = workspace
        .lookup(7, b"file", ReferenceScope::Local, deadline())
        .unwrap();
    assert!(matches!(
        workspace.open(file.serial, ReferenceScope::Projection),
        Err(WorkspaceError::Busy)
    ));
    assert!(matches!(
        workspace.opendir(7, ReferenceScope::Projection),
        Err(WorkspaceError::Busy)
    ));
    let local = workspace.open(file.serial, ReferenceScope::Local).unwrap();
    let mut lease = workspace.reserve_mount().unwrap();
    let projected_file = workspace
        .open(file.serial, ReferenceScope::Projection)
        .unwrap();
    let projected_directory = workspace.opendir(7, ReferenceScope::Projection).unwrap();
    assert_eq!(workspace.status().unwrap().projection_handles, 2);
    lease.stop_admission();
    assert!(matches!(lease.finish(), Err(WorkspaceError::Busy)));
    workspace.flush(projected_file).unwrap();
    workspace.release(projected_file).unwrap();
    assert_eq!(workspace.status().unwrap().projection_handles, 1);
    assert!(matches!(lease.finish(), Err(WorkspaceError::Busy)));
    workspace.releasedir(projected_directory).unwrap();
    lease.finish().unwrap();
    let status = workspace.status().unwrap();
    assert!(!status.mounted);
    assert_eq!(status.projection_handles, 0);
    assert_eq!(status.handles, 1);
    assert_eq!(
        workspace.read(local, 0, 5, deadline()).unwrap().as_ref(),
        b"hello"
    );
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
    workspace.release(local).unwrap();
    workspace.forget(file.serial, 1, ReferenceScope::Local);
    workspace.close_clean().unwrap();
}

#[test]
fn registry_grows_on_demand_and_keeps_retained_capacity_charged() {
    let huge = Fixture::new(usize::MAX);
    let small = Fixture::new(1);
    let first = huge.attach();
    let only = small.attach();
    let initial = first.status().unwrap().accounted_bytes;
    assert_eq!(initial, only.status().unwrap().accounted_bytes);
    assert!(initial < DEFAULT_MEMORY_BUDGET_BYTES);
    assert!(matches!(
        small.host.attach(small.options("second", 2), deadline()),
        Err(WorkspaceError::Capacity)
    ));
    only.close_clean().unwrap();
    small
        .host
        .attach(small.options("second", 2), deadline())
        .unwrap()
        .close_clean()
        .unwrap();

    let second = huge
        .host
        .attach(huge.options("second", 2), deadline())
        .unwrap();
    let with_two = first.status().unwrap().accounted_bytes;
    second.close_clean().unwrap();
    drop(second);
    let retained = first.status().unwrap().accounted_bytes;
    assert!(
        retained > initial,
        "the now-unused second registry slot remains allocated"
    );
    assert!(
        retained < with_two,
        "closed Workspace tables and its ID have been released"
    );
    let again = huge
        .host
        .attach(huge.options("second", 3), deadline())
        .unwrap();
    assert_eq!(first.status().unwrap().accounted_bytes, with_two);
    again.close_clean().unwrap();
    first.close_clean().unwrap();
}

#[test]
fn failed_close_keeps_its_registry_count_until_cleanup_succeeds() {
    let fixture = Fixture::new(1);
    let workspace = fixture.attach();
    let obstruction = workspace.mount_path().join("owned-cleanup-obstruction");
    fs::write(&obstruction, b"retained").unwrap();
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Io)));
    assert!(matches!(
        fixture
            .host
            .attach(fixture.options("second", 2), deadline()),
        Err(WorkspaceError::Capacity)
    ));
    fs::remove_file(obstruction).unwrap();
    workspace.close_clean().unwrap();
    fixture
        .host
        .attach(fixture.options("second", 2), deadline())
        .unwrap()
        .close_clean()
        .unwrap();
}
