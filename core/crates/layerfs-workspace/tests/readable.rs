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
        Self::with_mode_and_disk(max_count, mode, None)
    }
    fn with_mode_and_disk(max_count: usize, mode: u32, disk: Option<u64>) -> Self {
        Self::with_directory(max_count, mode, disk, false)
    }
    fn with_directory(max_count: usize, mode: u32, disk: Option<u64>, directory: bool) -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "layerfs-workspace-test-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
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
        let next_serial = Arc::new(AtomicU64::new(20));
        let serials = next_serial.clone();
        let deliver: OperationDelivery = Arc::new(move |request, _, output, _| {
            let mut result = match &request.operation {
                Operation::Inspect {
                    query: Inspect::Attributes { path },
                    ..
                } => match path.as_slice() {
                    b"" => Ok(attr(7, 2, 0)),
                    b"file" | b"alias" => Ok(attr(8, 1, 5)),
                    b"link" => Ok(attr(9, 3, 4)),
                    b"folder" if directory => Ok(attr(10, 2, 0)),
                    b"folder/child" if directory => Ok(attr(11, 1, 5)),
                    _ => Err(Code::PathNotFound.into()),
                },
                Operation::Inspect {
                    query: Inspect::ChildAttributes { parent, name },
                    ..
                } => match (*parent, name.as_slice()) {
                    (7, b"file" | b"alias") => Ok(attr(8, 1, 5)),
                    (7, b"link") => Ok(attr(9, 3, 4)),
                    (7, b"folder") if directory => Ok(attr(10, 2, 0)),
                    (10, b"child") if directory => Ok(attr(11, 1, 5)),
                    _ => Err(Code::PathNotFound.into()),
                },
                Operation::Inspect {
                    query: Inspect::InodeAttributes { serial },
                    ..
                } => match *serial {
                    7 => Ok(attr(7, 2, 0)),
                    8 => Ok(attr(8, 1, 5)),
                    9 => Ok(attr(9, 3, 4)),
                    10 if directory => Ok(attr(10, 2, 0)),
                    11 if directory => Ok(attr(11, 1, 5)),
                    _ => Err(Code::NotFound.into()),
                },
                Operation::Inspect {
                    query: Inspect::Readlink { path },
                    ..
                } if path == b"link" => Ok(Response::Link(b"file".to_vec())),
                Operation::Inspect {
                    query: Inspect::InodeReadlink { serial },
                    ..
                } if *serial == 9 => Ok(Response::Link(b"file".to_vec())),
                Operation::Inspect {
                    query: Inspect::List { .. } | Inspect::InodeList { .. },
                    ..
                } => {
                    let (path, after, entries): (&[u8], &[u8], u16) = match &request.operation {
                        Operation::Inspect {
                            query:
                                Inspect::List {
                                    path,
                                    after,
                                    entries,
                                    ..
                                },
                            ..
                        } => (path, after, *entries),
                        Operation::Inspect {
                            query:
                                Inspect::InodeList {
                                    serial,
                                    after,
                                    entries,
                                    ..
                                },
                            ..
                        } => (
                            if *serial == 7 {
                                b""
                            } else if *serial == 10 {
                                b"folder"
                            } else {
                                return Err(Code::NotFound.into());
                            },
                            after,
                            *entries,
                        ),
                        _ => unreachable!(),
                    };
                    let mut names: Vec<_> = if path == b"folder" && directory {
                        vec![(b"child".to_vec(), 11)]
                    } else if path.is_empty() {
                        vec![
                            (b"alias".to_vec(), 8),
                            (b"file".to_vec(), 8),
                            (b"link".to_vec(), 9),
                        ]
                    } else {
                        return Err(Code::PathNotFound.into());
                    };
                    if path.is_empty() && directory {
                        names.push((b"folder".to_vec(), 10));
                        names.sort();
                    }
                    names.retain(|(name, _)| name.as_slice() > after);
                    let more = names.len() > usize::from(entries);
                    names.truncate(usize::from(entries));
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
                Operation::ReadFile { root, start, end } if *root == [11; 32] && directory => {
                    output.write_all(&b"world"[*start as usize..*end as usize])?;
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
                Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count: 1 }) => {
                    Ok(Response::History(Box::new(HistoryResult::Reservation {
                        scope: *scope,
                        start: serials.fetch_add(1, Ordering::Relaxed),
                        count: 1,
                    })))
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
                disk_budget_bytes: disk,
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
            access: WorkspaceAccess::ReadOnly,
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
#[cfg(target_os = "linux")]
fn local_edit_active_view_resolves_and_lists_inherited_names() {
    let fixture = Fixture::with_mode_and_disk(2, 0o755, Some(64 << 20));
    let mut options = fixture.options("active", 2);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    let file = workspace
        .lookup(7, b"file", ReferenceScope::Local, deadline())
        .unwrap();
    assert_eq!((file.serial, file.size), (8, 5));
    let alias = workspace
        .lookup(7, b"alias", ReferenceScope::Local, deadline())
        .unwrap();
    assert_eq!(alias.serial, file.serial);
    let handle = workspace.opendir(7, ReferenceScope::Local).unwrap();
    let page = workspace
        .readdir(handle, 0, MAX_DIRECTORY_ENTRIES, deadline())
        .unwrap();
    let names: Vec<_> = page
        .entries()
        .iter()
        .map(|entry| entry.name.as_slice())
        .collect();
    assert_eq!(names, [b".".as_slice(), b"..", b"alias", b"file", b"link"]);
    drop(page);
    workspace.releasedir(handle).unwrap();
    workspace.forget(8, 2, ReferenceScope::Local);
    workspace.close_clean().unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn local_edit_active_create_write_rename_unlink_keeps_open_bytes() {
    let fixture = Fixture::with_mode_and_disk(2, 0o755, Some(64 << 20));
    let mut options = fixture.options("active-mutations", 3);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    let root = workspace.root().serial;
    let options = FileCreateOptions {
        mode: 0o644,
        umask: 0,
        exclusive: true,
        open: FileOpenOptions {
            access: FileAccess::ReadWrite,
            append: false,
            truncate: false,
        },
    };
    let (created, handle) = workspace
        .create_file(root, b"temp", options, deadline())
        .unwrap();
    let mut input = &b"hello"[..];
    let payload = workspace.own_payload(5, &mut input, deadline()).unwrap();
    let receipt = workspace
        .write_file(handle, 0, &payload, deadline())
        .unwrap();
    assert_eq!((receipt.inode, receipt.accepted_bytes), (created.serial, 5));
    assert_eq!(
        workspace.read(handle, 0, 5, deadline()).unwrap().as_ref(),
        b"hello"
    );
    workspace
        .rename(
            root,
            b"temp",
            root,
            b"final",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert!(matches!(
        workspace.lookup(root, b"temp", ReferenceScope::Local, deadline()),
        Err(WorkspaceError::NotFound)
    ));
    assert_eq!(
        workspace
            .lookup(root, b"final", ReferenceScope::Local, deadline())
            .unwrap()
            .serial,
        created.serial
    );
    workspace.unlink(root, b"final", deadline()).unwrap();
    assert!(matches!(
        workspace.lookup(root, b"final", ReferenceScope::Local, deadline()),
        Err(WorkspaceError::NotFound)
    ));
    assert_eq!(
        workspace.read(handle, 0, 5, deadline()).unwrap().as_ref(),
        b"hello"
    );
    workspace.release(handle).unwrap();
    workspace.forget(created.serial, u64::MAX, ReferenceScope::Local);
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
}

#[test]
#[cfg(target_os = "linux")]
fn local_edit_active_symlink_target_survives_rename() {
    let fixture = Fixture::with_mode_and_disk(2, 0o755, Some(64 << 20));
    let mut options = fixture.options("active-link", 4);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    let root = workspace.root().serial;
    let link = workspace
        .symlink(root, b"shortcut", b"file", deadline())
        .unwrap();
    assert_eq!(
        workspace
            .readlink(link.serial, deadline())
            .unwrap()
            .as_ref(),
        b"file"
    );
    workspace
        .rename(
            root,
            b"shortcut",
            root,
            b"renamed",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(
        workspace
            .lookup(root, b"renamed", ReferenceScope::Local, deadline())
            .unwrap()
            .serial,
        link.serial
    );
    assert_eq!(
        workspace
            .readlink(link.serial, deadline())
            .unwrap()
            .as_ref(),
        b"file"
    );
    workspace.forget(link.serial, u64::MAX, ReferenceScope::Local);
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
}

#[test]
#[cfg(target_os = "linux")]
fn local_edit_active_links_survive_forgotten_lookup_cache() {
    let fixture = Fixture::with_mode_and_disk(2, 0o755, Some(64 << 20));
    let mut options = fixture.options("active-alias", 5);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    let root = workspace.root().serial;
    let options = FileCreateOptions {
        mode: 0o644,
        umask: 0,
        exclusive: true,
        open: FileOpenOptions {
            access: FileAccess::ReadWrite,
            append: false,
            truncate: false,
        },
    };
    let (created, handle) = workspace
        .create_file(root, b"first", options, deadline())
        .unwrap();
    let alias = workspace
        .link(root, b"second", created.serial, deadline())
        .unwrap();
    assert_eq!(alias.references, 2);
    workspace.release(handle).unwrap();
    workspace.forget(created.serial, u64::MAX, ReferenceScope::Local);
    let alias = workspace
        .lookup(root, b"second", ReferenceScope::Local, deadline())
        .unwrap();
    assert_eq!((alias.serial, alias.references), (created.serial, 2));
    workspace.unlink(root, b"first", deadline()).unwrap();
    workspace.forget(created.serial, u64::MAX, ReferenceScope::Local);
    let alias = workspace
        .lookup(root, b"second", ReferenceScope::Local, deadline())
        .unwrap();
    assert_eq!(alias.references, 1);
    workspace.forget(created.serial, u64::MAX, ReferenceScope::Local);
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
}

#[test]
#[cfg(target_os = "linux")]
fn local_edit_active_append_truncate_attributes_and_directory_pins() {
    let fixture = Fixture::with_mode_and_disk(2, 0o755, Some(64 << 20));
    let mut options = fixture.options("active-edits", 6);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    let directory = workspace.mkdir(7, b"newdir", 0o755, 0, deadline()).unwrap();
    let pinned = workspace
        .opendir(directory.serial, ReferenceScope::Local)
        .unwrap();
    let file = workspace
        .mknod(directory.serial, b"data", 0o644, 0, deadline())
        .unwrap();
    let before = workspace.readdir(pinned, 0, 8, deadline()).unwrap();
    assert_eq!(
        before
            .entries()
            .iter()
            .map(|entry| entry.name.as_slice())
            .collect::<Vec<_>>(),
        [b".".as_slice(), b".."]
    );
    drop(before);
    workspace.releasedir(pinned).unwrap();
    let current = workspace
        .opendir(directory.serial, ReferenceScope::Local)
        .unwrap();
    let page = workspace.readdir(current, 0, 8, deadline()).unwrap();
    assert_eq!(
        page.entries()
            .iter()
            .map(|entry| entry.name.as_slice())
            .collect::<Vec<_>>(),
        [b".".as_slice(), b"..", b"data"]
    );
    drop(page);
    workspace.releasedir(current).unwrap();
    let handle = workspace
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
    let mut first = &b"abcd"[..];
    let payload = workspace.own_payload(4, &mut first, deadline()).unwrap();
    workspace
        .write_file(handle, 0, &payload, deadline())
        .unwrap();
    let mut second = &b"E"[..];
    let payload = workspace.own_payload(1, &mut second, deadline()).unwrap();
    workspace
        .write_file(handle, 0, &payload, deadline())
        .unwrap();
    assert_eq!(
        workspace.read(handle, 0, 5, deadline()).unwrap().as_ref(),
        b"abcdE"
    );
    workspace.set_len(file.serial, 2, deadline()).unwrap();
    workspace.set_len(file.serial, 5, deadline()).unwrap();
    assert_eq!(
        workspace.read(handle, 0, 5, deadline()).unwrap().as_ref(),
        b"ab\0\0\0"
    );
    let attr = workspace
        .set_attributes(
            file.serial,
            PortableAttributes {
                size: None,
                mode: Some(0o600),
                mtime: Some((42, 7)),
            },
            deadline(),
        )
        .unwrap();
    assert_eq!(
        (attr.mode, attr.mtime_seconds, attr.mtime_nanoseconds),
        (0o600, 42, 7)
    );
    assert!(matches!(
        workspace.rmdir(7, b"newdir", deadline()),
        Err(WorkspaceError::NotEmpty)
    ));
    workspace
        .unlink(directory.serial, b"data", deadline())
        .unwrap();
    workspace.rmdir(7, b"newdir", deadline()).unwrap();
    workspace.release(handle).unwrap();
    workspace.forget(file.serial, u64::MAX, ReferenceScope::Local);
    workspace.forget(directory.serial, u64::MAX, ReferenceScope::Local);
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
}

#[test]
#[cfg(target_os = "linux")]
#[ignore = "requires privileged Docker with /dev/fuse"]
fn mounted_active_create_write_rename_unlink() {
    use std::io::Write;
    let fixture = Fixture::with_mode_and_disk(2, 0o755, Some(64 << 20));
    let mut options = fixture.options("active-mounted", 7);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    let mut mount = layerfs_fuse::mount_writable(&workspace, deadline()).unwrap();
    let first = workspace.mount_path().join("temp");
    let final_path = workspace.mount_path().join("final");
    let alias = workspace.mount_path().join("second");
    let directory = workspace.mount_path().join("newdir");
    let moved = directory.join("second");
    let shortcut = directory.join("shortcut");
    fs::write(&first, b"hello").unwrap();
    assert_eq!(fs::read(&first).unwrap(), b"hello");
    fs::OpenOptions::new()
        .append(true)
        .open(&first)
        .unwrap()
        .write_all(b"!")
        .unwrap();
    assert_eq!(fs::read(&first).unwrap(), b"hello!");
    fs::rename(&first, &final_path).unwrap();
    fs::hard_link(&final_path, &alias).unwrap();
    fs::remove_file(&final_path).unwrap();
    assert!(!final_path.exists());
    assert_eq!(fs::read(&alias).unwrap(), b"hello!");
    fs::create_dir(&directory).unwrap();
    fs::rename(&alias, &moved).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&moved)
        .unwrap()
        .set_len(3)
        .unwrap();
    assert_eq!(fs::read(&moved).unwrap(), b"hel");
    std::os::unix::fs::symlink("second", &shortcut).unwrap();
    assert_eq!(
        fs::read_link(&shortcut).unwrap(),
        std::path::Path::new("second")
    );
    fs::remove_file(&shortcut).unwrap();
    fs::remove_file(&moved).unwrap();
    fs::remove_dir(&directory).unwrap();
    assert_eq!(workspace.status().unwrap().revision, 13);
    mount.unmount(deadline()).unwrap();
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
}

#[test]
#[cfg(target_os = "linux")]
#[ignore = "requires privileged Docker FUSE and a scoped seccomp notifier fault"]
fn mounted_active_notifier_eio_keeps_published_receipt() {
    let fixture = Fixture::with_mode_and_disk(2, 0o755, Some(64 << 20));
    let mut options = fixture.options("active-mounted-fault", 12);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    let mut mount = layerfs_fuse::mount_writable(&workspace, deadline()).unwrap();
    let path = workspace.mount_path().join("retained");
    assert_eq!(
        fs::metadata(&path).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let fd = fuse_fd();
    let create = FileCreateOptions {
        mode: 0o644,
        umask: 0,
        exclusive: true,
        open: FileOpenOptions {
            access: FileAccess::ReadWrite,
            append: false,
            truncate: false,
        },
    };
    let error = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                deny_notifier_writev(fd, Some(4));
                workspace
                    .create_file(7, b"retained", create, deadline())
                    .unwrap_err()
            })
            .join()
            .unwrap()
    });
    let WorkspaceError::Coherence(failure) = error else {
        panic!("{error:?}");
    };
    assert_eq!(failure.raw_os_error, Some(nix::libc::EIO));
    assert_eq!(failure.receipt.revision, 1);
    assert!(!failure.notifier_returned_ok);
    let handle = failure.published_handle.unwrap();
    assert_eq!(
        workspace.handle_attributes(handle).unwrap().serial,
        failure.receipt.inode
    );
    assert_eq!(
        workspace.status().unwrap().coherence,
        Some(CoherenceStatus::Failed(failure))
    );
    mount.unmount(deadline()).unwrap();
    workspace.release(handle).unwrap();
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
}

#[cfg(target_os = "linux")]
fn fuse_fd() -> i32 {
    let fds: Vec<_> = fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            (fs::read_link(entry.path()).ok().as_deref() == Some(std::path::Path::new("/dev/fuse")))
                .then(|| entry.file_name().to_str().unwrap().parse::<i32>().unwrap())
        })
        .collect();
    assert_eq!(fds.len(), 1);
    fds[0]
}

#[cfg(target_os = "linux")]
fn deny_notifier_writev(fd: i32, iovcnt: Option<u32>) {
    use nix::libc;
    let arch = if cfg!(target_arch = "aarch64") {
        0xc00000b7
    } else {
        0xc000003e
    };
    let stmt = |code, k| libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    };
    let jump = |k, jf| libc::sock_filter {
        code: 0x15,
        jt: 0,
        jf,
        k,
    };
    let vectors = match iovcnt {
        Some(count) => jump(count, 1),
        None => stmt(0x05, 0),
    };
    let mut code = [
        stmt(0x20, 4),
        jump(arch, 7),
        stmt(0x20, 0),
        jump(libc::SYS_writev as u32, 5),
        stmt(0x20, 16),
        jump(fd as u32, 3),
        stmt(0x20, 32),
        vectors,
        stmt(0x06, 0x00050000 | libc::EIO as u32),
        stmt(0x06, 0x7fff0000),
    ];
    let program = libc::sock_fprog {
        len: code.len() as u16,
        filter: code.as_mut_ptr(),
    };
    unsafe {
        assert_eq!(libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), 0);
        assert_eq!(libc::syscall(libc::SYS_seccomp, 1, 0, &program), 0);
    }
}

#[test]
#[cfg(target_os = "linux")]
fn local_edit_active_notifier_failure_retains_published_handle_and_receipt() {
    let fixture = Fixture::with_mode_and_disk(2, 0o755, Some(64 << 20));
    let mut options = fixture.options("active-coherence", 8);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    let mut mount = workspace.reserve_mount().unwrap();
    mount
        .bind_invalidation(Arc::new(|_, _, _| {
            Err(std::io::Error::from_raw_os_error(5))
        }))
        .unwrap();
    let options = FileCreateOptions {
        mode: 0o644,
        umask: 0,
        exclusive: true,
        open: FileOpenOptions {
            access: FileAccess::ReadWrite,
            append: false,
            truncate: false,
        },
    };
    let error = workspace
        .create_file(7, b"retained", options, deadline())
        .unwrap_err();
    let WorkspaceError::Coherence(failure) = error else {
        panic!("{error:?}");
    };
    assert_eq!(
        (failure.receipt.revision, failure.receipt.accepted_bytes),
        (1, 0)
    );
    assert_eq!(failure.raw_os_error, Some(5));
    assert!(!failure.notifier_returned_ok);
    let handle = failure.published_handle.unwrap();
    assert_eq!(
        workspace.handle_attributes(handle).unwrap().serial,
        failure.receipt.inode
    );
    assert_eq!(
        workspace
            .lookup(7, b"retained", ReferenceScope::Local, deadline())
            .unwrap()
            .serial,
        failure.receipt.inode
    );
    assert_eq!(
        workspace.status().unwrap().coherence,
        Some(CoherenceStatus::Failed(failure))
    );
    workspace.release(handle).unwrap();
    workspace.forget(failure.receipt.inode, u64::MAX, ReferenceScope::Local);
    mount.finish().unwrap();
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
}

#[test]
#[cfg(target_os = "linux")]
fn local_edit_active_quota_refusal_keeps_old_namespace_revision() {
    let fixture = Fixture::with_mode_and_disk(2, 0o755, Some(1));
    let mut options = fixture.options("active-quota", 9);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    assert!(workspace
        .mknod(7, b"refused", 0o644, 0, deadline())
        .is_err());
    assert_eq!(workspace.status().unwrap().revision, 0);
    assert_eq!(workspace.status().unwrap().dirty_inodes, 0);
    assert!(matches!(
        workspace.lookup(7, b"refused", ReferenceScope::Local, deadline()),
        Err(WorkspaceError::Service(Failure {
            code: Code::PathNotFound,
            ..
        })) | Err(WorkspaceError::NotFound)
    ));
    workspace.close_clean().unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn local_edit_active_moved_inherited_directory_keeps_base_path() {
    let fixture = Fixture::with_directory(2, 0o755, Some(64 << 20), true);
    let mut options = fixture.options("active-origin", 10);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    let folder = workspace
        .lookup(7, b"folder", ReferenceScope::Local, deadline())
        .unwrap();
    let pinned = workspace
        .opendir(folder.serial, ReferenceScope::Local)
        .unwrap();
    workspace
        .rename(
            7,
            b"folder",
            7,
            b"moved",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(
        workspace
            .lookup(7, b"moved", ReferenceScope::Local, deadline())
            .unwrap()
            .serial,
        folder.serial
    );
    for handle in [
        pinned,
        workspace
            .opendir(folder.serial, ReferenceScope::Local)
            .unwrap(),
    ] {
        let page = workspace.readdir(handle, 0, 8, deadline()).unwrap();
        assert_eq!(
            page.entries()
                .iter()
                .map(|entry| entry.name.as_slice())
                .collect::<Vec<_>>(),
            [b".".as_slice(), b"..", b"child"]
        );
        drop(page);
        workspace.releasedir(handle).unwrap();
    }
    let child = workspace
        .lookup(folder.serial, b"child", ReferenceScope::Local, deadline())
        .unwrap();
    let handle = workspace.open(child.serial, ReferenceScope::Local).unwrap();
    assert_eq!(
        workspace.read(handle, 0, 5, deadline()).unwrap().as_ref(),
        b"world"
    );
    workspace.release(handle).unwrap();
    workspace.forget(child.serial, u64::MAX, ReferenceScope::Local);
    workspace.forget(folder.serial, u64::MAX, ReferenceScope::Local);
    assert_eq!(workspace.status().unwrap().revision, 1);
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
}

#[test]
#[cfg(target_os = "linux")]
fn local_edit_active_editor_replace_retains_open_old_inode() {
    let fixture = Fixture::with_mode_and_disk(2, 0o755, Some(64 << 20));
    let mut options = fixture.options("active-editor", 11);
    options.access = WorkspaceAccess::LocalEdit;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    options.base = Base::Branch(branch);
    let workspace = fixture.host.attach(options, deadline()).unwrap();
    let options = FileCreateOptions {
        mode: 0o644,
        umask: 0,
        exclusive: true,
        open: FileOpenOptions {
            access: FileAccess::ReadWrite,
            append: false,
            truncate: false,
        },
    };
    let (old, old_handle) = workspace
        .create_file(7, b"document", options, deadline())
        .unwrap();
    let mut old_bytes = &b"old"[..];
    let payload = workspace
        .own_payload(3, &mut old_bytes, deadline())
        .unwrap();
    workspace
        .write_file(old_handle, 0, &payload, deadline())
        .unwrap();
    let (new, new_handle) = workspace
        .create_file(7, b"document.tmp", options, deadline())
        .unwrap();
    let mut new_bytes = &b"new"[..];
    let payload = workspace
        .own_payload(3, &mut new_bytes, deadline())
        .unwrap();
    workspace
        .write_file(new_handle, 0, &payload, deadline())
        .unwrap();
    workspace
        .rename(
            7,
            b"document.tmp",
            7,
            b"document",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(
        workspace
            .lookup(7, b"document", ReferenceScope::Local, deadline())
            .unwrap()
            .serial,
        new.serial
    );
    assert_eq!(
        workspace
            .read(new_handle, 0, 3, deadline())
            .unwrap()
            .as_ref(),
        b"new"
    );
    assert_eq!(
        workspace
            .read(old_handle, 0, 3, deadline())
            .unwrap()
            .as_ref(),
        b"old"
    );
    workspace.release(old_handle).unwrap();
    workspace.release(new_handle).unwrap();
    workspace.forget(old.serial, u64::MAX, ReferenceScope::Local);
    workspace.forget(new.serial, u64::MAX, ReferenceScope::Local);
    assert_eq!(workspace.status().unwrap().revision, 5);
    assert!(matches!(workspace.close_clean(), Err(WorkspaceError::Busy)));
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
            query: Inspect::ChildAttributes { parent: 7, name },
            ..
        } => {
            let index: u64 = std::str::from_utf8(name).unwrap().parse().unwrap();
            Ok(attr(index + 8, 1, 5))
        }
        Operation::Inspect {
            query:
                Inspect::List { after, entries, .. }
                | Inspect::InodeList {
                    serial: 7,
                    after,
                    entries,
                    ..
                },
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
