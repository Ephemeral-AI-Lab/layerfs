//! Public local edits with a real authenticated service and Linux private backing.
//! The external driver supplies an independently copied, closed R1 fixture.
use layerfs_workspace::WorkspacePath;

#[test]
fn logical_edit_paths_cannot_escape_the_workspace() {
    for invalid in [
        b"/abs".as_slice(),
        b"../x",
        b"a/../b",
        b"a//b",
        b"a/",
        b"a\\b",
        b"a\0b",
    ] {
        assert!(WorkspacePath::new(invalid).is_err(), "{invalid:?}");
    }
    assert_eq!(WorkspacePath::new(b"a/b").unwrap().as_ref(), b"a/b");
}

#[cfg(target_os = "linux")]
mod linux {
    use layerfs_bridge::{
        adapters::native::{client::Client, connection::connect_until, pipe},
        contract::*,
    };
    use layerfs_workspace::*;
    use std::{
        fs,
        net::{SocketAddr, ToSocketAddrs},
        os::unix::fs::MetadataExt,
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(10)
    }
    fn bytes<const N: usize>(name: &str) -> [u8; N] {
        let text = std::env::var(name).unwrap();
        assert_eq!(text.len(), N * 2);
        std::array::from_fn(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap())
    }
    struct Native {
        endpoint: SocketAddr,
        private: [u8; 32],
        server: [u8; 32],
        read_bytes: AtomicU64,
        calls: AtomicU64,
    }
    impl Native {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                endpoint: std::env::var("LAYERFS_ENDPOINT")
                    .unwrap()
                    .to_socket_addrs()
                    .unwrap()
                    .next()
                    .unwrap(),
                private: pipe::key(&std::env::var("LAYERFS_PRIVATE_KEY").unwrap()).unwrap(),
                server: pipe::key(&std::env::var("LAYERFS_SERVER_KEY").unwrap()).unwrap(),
                read_bytes: AtomicU64::new(0),
                calls: AtomicU64::new(0),
            })
        }
        fn call(
            &self,
            request: &Request,
            input: &mut dyn Source,
            output: &mut dyn std::io::Write,
            end: Instant,
        ) -> Result<Response, Failure> {
            assert!(
                matches!(
                    request.operation,
                    Operation::Inspect { .. }
                        | Operation::ReadFile { .. }
                        | Operation::HistoryQuery(HistoryQuery::GetBranch { .. })
                ),
                "local edit attempted a service mutation: {:?}",
                request.operation
            );
            self.calls.fetch_add(1, Ordering::Relaxed);
            let mut client = Client::new(connect_until(
                self.endpoint,
                1,
                &self.private,
                &self.server,
                end,
            )?)?;
            let result = client.call_until(request, input, output, end)?;
            if let Response::Read { length } = result {
                self.read_bytes.fetch_add(length, Ordering::Relaxed);
            }
            Ok(result)
        }
        fn delivery(self: &Arc<Self>) -> OperationDelivery {
            let native = self.clone();
            Arc::new(move |request, input, output, end| native.call(request, input, output, end))
        }
        fn branch(&self) -> Response {
            self.call(
                &Request {
                    id: 77,
                    generation: 1,
                    store: 1,
                    profile: HISTORY_PROFILE,
                    deadline_ms: 10000,
                    response_bytes: 16384,
                    operation: Operation::HistoryQuery(HistoryQuery::GetBranch {
                        branch: bytes("LAYERFS_EDIT_BRANCH"),
                    }),
                },
                &mut &[][..],
                &mut std::io::sink(),
                deadline(),
            )
            .unwrap()
        }
    }
    struct Fixture {
        path: PathBuf,
        host: WorkspaceHost,
        workspace: Workspace,
        native: Arc<Native>,
    }
    impl Fixture {
        fn new(quota: u64) -> Self {
            Self::with_count(quota, 2)
        }
        fn with_count(quota: u64, max_count: usize) -> Self {
            let path = PathBuf::from(std::env::var("LAYERFS_EDIT_TEST_ROOT").unwrap());
            let native = Native::new();
            let host = WorkspaceHost::new(
                WorkspaceConfig {
                    root: path.clone(),
                    max_count,
                    memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                    disk_budget_bytes: Some(quota),
                },
                native.delivery(),
            )
            .unwrap();
            let workspace = host
                .attach(
                    Self::options(&path, "edit", 31, WorkspaceAccess::LocalEdit),
                    deadline(),
                )
                .unwrap();
            Self {
                path,
                host,
                workspace,
                native,
            }
        }
        fn options(
            path: &std::path::Path,
            id: &str,
            incarnation: u8,
            access: WorkspaceAccess,
        ) -> AttachOptions {
            let metadata = fs::metadata(path).unwrap();
            AttachOptions {
                id: id.into(),
                incarnation: [incarnation; 32],
                store: 1,
                base: Base::Branch(bytes("LAYERFS_EDIT_BRANCH")),
                access,
                owner_uid: metadata.uid(),
                owner_gid: metadata.gid(),
            }
        }
        fn file(&self, name: &[u8]) -> NodeAttributes {
            self.workspace
                .lookup(
                    self.workspace.root().serial,
                    name,
                    ReferenceScope::Local,
                    deadline(),
                )
                .unwrap()
        }
        fn own(&self, bytes: &[u8]) -> OwnedPayload {
            self.workspace
                .own_payload(bytes.len() as u64, &mut &bytes[..], deadline())
                .unwrap()
        }
        fn edit(&self, name: &[u8], start: u64, end: u64, replacement: &[u8]) -> MutationReceipt {
            let input = self.own(replacement);
            self.workspace
                .edit_file_range(
                    &WorkspacePath::new(name).unwrap(),
                    &RangeEdit {
                        start,
                        end,
                        replacement: input,
                    },
                    deadline(),
                )
                .unwrap()
        }
        fn all(&self, handle: HandleId) -> Vec<u8> {
            let mut result = Vec::new();
            loop {
                let reply = self
                    .workspace
                    .read(handle, result.len() as u64, MAX_READ_BYTES, deadline())
                    .unwrap();
                if reply.as_ref().is_empty() {
                    break;
                }
                result.extend_from_slice(reply.as_ref());
            }
            result
        }
        fn observe(&self, phase: &str) {
            let workspace = self.workspace.status().unwrap();
            let backing = self.workspace.backing_status().unwrap();
            let metadata = self.workspace.metadata_status().unwrap();
            assert!(workspace.accounted_bytes <= DEFAULT_MEMORY_BUDGET_BYTES);
            assert!(backing.allocated_bytes + backing.reserved_bytes <= backing.quota_bytes);
            assert!(backing.accounting_complete && metadata.accounting_complete);
            assert!(metadata.working_bytes <= 768 * 1024);
            let native_allocated = allocated_files(&self.path.join("private-backing"));
            assert_eq!(backing.allocated_bytes, native_allocated);
            let process_fds = fs::read_dir("/proc/self/fd").unwrap().count();
            println!("LOCAL_EDIT_RESOURCE {phase} workspace={workspace:?} backing={backing:?} metadata={metadata:?} native_file_allocated={native_allocated} process_fds_at_boundary={process_fds} fd_scope=includes_proc_enumeration_not_peak");
        }
    }
    fn base() -> Vec<u8> {
        (0..577551).map(|index| (index % 251) as u8).collect()
    }
    fn check(name: &str) {
        println!("LOCAL_EDIT_CHECK {name} PASS");
    }

    #[test]
    #[ignore = "requires the actual service and Linux ext4 driver"]
    fn local_range_edit_semantics() {
        let f = Fixture::new(128 * 1024 * 1024);
        let branch = f.native.branch();
        let file = f.file(b"data.bin");
        let alias = f.file(b"alias");
        assert_eq!(file, alias);
        let handle = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        let alias_handle = f
            .workspace
            .open(alias.serial, ReferenceScope::Local)
            .unwrap();
        let mut lease = f.workspace.reserve_mount().unwrap();
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Unbound)
        );
        assert!(matches!(
            f.workspace.set_len(file.serial, file.size, deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert!(matches!(
            f.workspace.begin_projection_reply(deadline()),
            Err(WorkspaceError::Busy)
        ));
        lease.finish().unwrap();
        assert!(!f.workspace.status().unwrap().mounted);
        check("explicit-local-access-and-unbound-projection-refusal");
        let before_read = f.native.read_bytes.load(Ordering::Relaxed);
        let receipt = f.edit(b"data.bin", 4093, 4101, b"replacement");
        assert_eq!(
            f.native.read_bytes.load(Ordering::Relaxed),
            before_read,
            "edit copied immutable content"
        );
        assert_eq!(receipt.incarnation, [31; 32]);
        assert_eq!((receipt.inode, receipt.accepted_bytes), (file.serial, 11));
        assert!(receipt.generation > 0 && receipt.revision > 0);
        let mut expected = base();
        expected.splice(4093..4101, b"replacement".iter().copied());
        let changed = f.workspace.getattr(file.serial).unwrap();
        assert_eq!(changed.size, expected.len() as u64);
        assert_eq!(
            (changed.mode, changed.references, changed.serial),
            (file.mode, 2, file.serial)
        );
        assert_ne!(
            (changed.mtime_seconds, changed.mtime_nanoseconds),
            (file.mtime_seconds, file.mtime_nanoseconds)
        );
        assert_eq!(
            f.workspace.handle_attributes(alias_handle).unwrap(),
            changed
        );
        assert_eq!(f.all(handle), expected);
        assert_eq!(f.all(alias_handle), expected);
        check("atomic-bytes-attributes-and-hardlinks");
        let second = f.edit(b"alias", 4095, 4106, b"XY");
        expected.splice(4095..4106, b"XY".iter().copied());
        assert_eq!(second.generation, receipt.generation);
        assert!(second.revision > receipt.revision);
        assert_eq!(f.all(handle), expected);
        f.edit(
            b"data.bin",
            expected.len() as u64,
            expected.len() as u64,
            b"end",
        );
        expected.extend_from_slice(b"end");
        f.edit(b"data.bin", 0, 3, b"");
        expected.drain(0..3);
        assert_eq!(f.all(alias_handle), expected);
        check("current-coordinate-overlap-insert-delete");
        f.workspace.release(handle).unwrap();
        f.workspace.release(alias_handle).unwrap();
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        let restored = f.file(b"alias");
        assert_eq!(restored.size, expected.len() as u64);
        let handle = f
            .workspace
            .open(restored.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(f.all(handle), expected);
        f.workspace.release(handle).unwrap();
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 1);
        assert_eq!(f.native.branch(), branch);
        check("node-forget-retains-dirty-index-and-branch");
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        assert!(matches!(
            f.workspace.close_clean(),
            Err(WorkspaceError::Busy)
        ));
        assert!(!f.workspace.status().unwrap().stopping);
        assert!(f.path.join("private-backing/edit").is_dir());
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        let after_reclaim = f.file(b"alias");
        let handle = f
            .workspace
            .open(after_reclaim.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(f.all(handle), expected);
        f.workspace.release(handle).unwrap();
        f.observe("semantic-end");
        check("dirty-close-preserves-owned-state");
    }

    #[test]
    #[ignore = "requires the actual service and Linux ext4 driver"]
    fn local_range_edit_refusals() {
        let f = Fixture::new(128 * 1024 * 1024);
        let branch = f.native.branch();
        let file = f.file(b"data.bin");
        let handle = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        let one = f.own(b"x");
        let path = WorkspacePath::new(b"data.bin").unwrap();
        for (start, end) in [(2, 1), (file.size + 1, file.size + 1), (0, u64::MAX)] {
            assert!(matches!(
                f.workspace.edit_file_range(
                    &path,
                    &RangeEdit {
                        start,
                        end,
                        replacement: one.clone()
                    },
                    deadline()
                ),
                Err(WorkspaceError::InvalidInput)
            ));
        }
        assert!(matches!(
            f.workspace.edit_file_range(
                &path,
                &RangeEdit {
                    start: 0,
                    end: 1,
                    replacement: one.clone()
                },
                Instant::now()
            ),
            Err(WorkspaceError::Deadline)
        ));
        for name in [b"empty".as_slice(), b"link", b"absent"] {
            let error = f
                .workspace
                .edit_file_range(
                    &WorkspacePath::new(name).unwrap(),
                    &RangeEdit {
                        start: 0,
                        end: 0,
                        replacement: one.clone(),
                    },
                    deadline(),
                )
                .unwrap_err();
            match name {
                b"empty" => assert_eq!(error, WorkspaceError::IsDirectory),
                b"link" => assert_eq!(error, WorkspaceError::WrongKind),
                _ => assert!(matches!(
                    error,
                    WorkspaceError::Service(Failure {
                        code: Code::PathNotFound,
                        ..
                    })
                )),
            }
        }
        assert_eq!(f.workspace.getattr(file.serial).unwrap(), file);
        assert_eq!(f.all(handle), base());
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 0);
        check("range-deadline-kind-failure-atomicity");
        let read_only = f
            .host
            .attach(
                Fixture::options(&f.path, "reader", 32, WorkspaceAccess::ReadOnly),
                deadline(),
            )
            .unwrap();
        assert!(matches!(
            read_only.edit_file_range(
                &path,
                &RangeEdit {
                    start: 0,
                    end: 1,
                    replacement: one.clone()
                },
                deadline()
            ),
            Err(WorkspaceError::ReadOnly)
        ));
        let foreign = read_only
            .own_payload(1, &mut &b"y"[..], deadline())
            .unwrap();
        assert!(f
            .workspace
            .edit_file_range(
                &path,
                &RangeEdit {
                    start: 0,
                    end: 1,
                    replacement: foreign.clone()
                },
                deadline()
            )
            .is_err());
        assert_eq!(f.all(handle), base());
        drop(foreign);
        read_only.reclaim_payloads(deadline()).unwrap();
        read_only.close_clean().unwrap();
        check("read-only-and-foreign-payload-refusal");
        let oversized = f.own(&vec![b'z'; 8 * 1024 * 1024 + 1]);
        assert!(matches!(
            f.workspace.edit_file_range(
                &path,
                &RangeEdit {
                    start: 0,
                    end: 0,
                    replacement: oversized.clone()
                },
                deadline()
            ),
            Err(WorkspaceError::Capacity)
        ));
        assert_eq!(f.workspace.getattr(file.serial).unwrap(), file);
        assert_eq!(f.native.branch(), branch);
        drop(oversized);
        drop(one);
        f.workspace.release(handle).unwrap();
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        f.observe("refusal-end");
        check("shared-replay-input-cap");
    }

    #[test]
    #[ignore = "requires the actual service and Linux ext4 driver"]
    fn local_range_edit_frontier() {
        let f = Fixture::new(128 * 1024 * 1024);
        let file = f.file(b"data.bin");
        let handle = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        let path = WorkspacePath::new(b"data.bin").unwrap();
        let replacement = f.own(b"x");
        let mut expected = base();
        for index in 0..256 {
            let start = index * 8 + 1;
            f.workspace
                .edit_file_range(
                    &path,
                    &RangeEdit {
                        start,
                        end: start + 1,
                        replacement: replacement.clone(),
                    },
                    deadline(),
                )
                .unwrap();
            expected[start as usize] = b'x';
            if index % 8 == 7 {
                f.workspace.reclaim_metadata(deadline()).unwrap();
                f.workspace.reclaim_payloads(deadline()).unwrap();
            }
        }
        assert_eq!(f.all(handle), expected);
        let attr = f.workspace.getattr(file.serial).unwrap();
        assert!(matches!(
            f.workspace.edit_file_range(
                &path,
                &RangeEdit {
                    start: 4097,
                    end: 4098,
                    replacement: replacement.clone()
                },
                deadline()
            ),
            Err(WorkspaceError::Capacity)
        ));
        assert_eq!(f.workspace.getattr(file.serial).unwrap(), attr);
        assert_eq!(f.all(handle), expected);
        check("exact-256-normalized-edits-refuses-257");
        for _ in 0..32 {
            f.workspace
                .edit_file_range(
                    &path,
                    &RangeEdit {
                        start: 1,
                        end: 2,
                        replacement: replacement.clone(),
                    },
                    deadline(),
                )
                .unwrap();
            f.workspace.reclaim_metadata(deadline()).unwrap();
        }
        assert_eq!(f.all(handle), expected);
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 1);
        f.workspace.release(handle).unwrap();
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        let restored = f.file(b"alias");
        let handle = f
            .workspace
            .open(restored.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(f.all(handle), expected);
        f.workspace.release(handle).unwrap();
        f.observe("frontier-end");
        check("repeated-overwrite-reuses-frontier-capacity");
    }

    fn allocated_files(path: &std::path::Path) -> u64 {
        fs::read_dir(path)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let metadata = entry.metadata().unwrap();
                if metadata.is_dir() {
                    allocated_files(&entry.path())
                } else {
                    assert!(metadata.is_file());
                    metadata.blocks() * 512
                }
            })
            .sum()
    }
    #[test]
    #[ignore = "requires the actual service and Linux ext4 driver"]
    fn local_range_edit_metadata_quota() {
        let f = Fixture::new(1024 * 1024);
        let file = f.file(b"data.bin");
        let replacement = f.own(b"x");
        assert!(f
            .workspace
            .edit_file_range(
                &WorkspacePath::new(b"data.bin").unwrap(),
                &RangeEdit {
                    start: 0,
                    end: 1,
                    replacement: replacement.clone()
                },
                deadline()
            )
            .is_err());
        assert_eq!(f.workspace.getattr(file.serial).unwrap(), file);
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 0);
        let handle = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(f.all(handle), base());
        f.workspace.release(handle).unwrap();
        let mut reader = replacement.reader(0..1).unwrap();
        let mut byte = [0];
        assert_eq!(
            reader
                .read(
                    &mut byte,
                    deadline(),
                    &std::sync::atomic::AtomicBool::new(false)
                )
                .unwrap(),
            1
        );
        assert_eq!(&byte, b"x");
        drop(reader);
        drop(replacement);
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        let backing = f.workspace.backing_status().unwrap();
        assert_eq!(
            backing.allocated_bytes,
            allocated_files(&f.path.join("private-backing"))
        );
        f.observe("quota-end");
        check("payload-fits-metadata-reserve-refuses-atomically");
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        assert!(!f.path.join("private-backing/edit").exists());
        check("clean-refusal-releases-confirmed-resources");
    }

    #[test]
    #[ignore = "requires the actual service and Linux ext4 driver"]
    fn local_range_edit_aggregate() {
        let f = Fixture::new(12 * 1024 * 1024);
        let second = f
            .host
            .attach(
                Fixture::options(&f.path, "second", 33, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        let file = f.file(b"data.bin");
        let body = vec![0x53; 6 * 1024 * 1024];
        let input = f.own(&body);
        f.workspace
            .edit_file_range(
                &WorkspacePath::new(b"data.bin").unwrap(),
                &RangeEdit {
                    start: 0,
                    end: file.size,
                    replacement: input.clone(),
                },
                deadline(),
            )
            .unwrap();
        assert!(second
            .own_payload(body.len() as u64, &mut &body[..], deadline())
            .is_err());
        let first_status = f.workspace.backing_status().unwrap();
        let second_status = second.backing_status().unwrap();
        assert_eq!(first_status.allocated_bytes, second_status.allocated_bytes);
        assert_eq!(first_status.reserved_bytes, second_status.reserved_bytes);
        assert_eq!(first_status.quota_bytes, 12 * 1024 * 1024);
        assert_eq!(
            f.workspace.status().unwrap().accounted_bytes,
            second.status().unwrap().accounted_bytes
        );
        assert!(f.workspace.status().unwrap().accounted_bytes <= DEFAULT_MEMORY_BUDGET_BYTES);
        check("shared-consumer-quota-and-memory-account");
        let owned = second.own_payload(1, &mut &b"z"[..], deadline()).unwrap();
        second
            .edit_file_range(
                &WorkspacePath::new(b"data.bin").unwrap(),
                &RangeEdit {
                    start: 0,
                    end: 1,
                    replacement: owned.clone(),
                },
                deadline(),
            )
            .unwrap();
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 1);
        assert_eq!(second.status().unwrap().dirty_inodes, 1);
        f.workspace.reclaim_metadata(deadline()).unwrap();
        second.reclaim_metadata(deadline()).unwrap();
        let handle = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(
            f.workspace
                .read(handle, 0, 16, deadline())
                .unwrap()
                .as_ref(),
            &[0x53; 16]
        );
        f.workspace.release(handle).unwrap();
        f.observe("aggregate-end");
        check("independent-local-roots-share-budget");
    }

    fn file_limit(value: &str) {
        let status = std::process::Command::new("prlimit")
            .arg("--pid")
            .arg(std::process::id().to_string())
            .arg(format!("--fsize={value}:"))
            .status()
            .unwrap();
        assert!(status.success());
    }
    struct LimitedFile;
    impl Drop for LimitedFile {
        fn drop(&mut self) {
            file_limit("unlimited");
        }
    }
    #[test]
    #[ignore = "requires the actual service and Linux ext4 driver"]
    fn local_range_edit_metadata_failure() {
        let f = Fixture::new(128 * 1024 * 1024);
        let file = f.file(b"data.bin");
        let input = f.own(b"x");
        assert!(fs::read_to_string("/proc/self/limits")
            .unwrap()
            .lines()
            .any(
                |line| line.starts_with("Max file size") && line.matches("unlimited").count() == 2
            ));
        file_limit("2048");
        let limit = LimitedFile;
        let result = f.workspace.edit_file_range(
            &WorkspacePath::new(b"data.bin").unwrap(),
            &RangeEdit {
                start: 0,
                end: 1,
                replacement: input.clone(),
            },
            deadline(),
        );
        drop(limit);
        assert!(
            result.is_err(),
            "4KiB metadata allocation succeeded past native 2KiB file limit"
        );
        println!("LOCAL_EDIT_NATIVE_FAILURE {result:?}");
        assert_eq!(f.workspace.getattr(file.serial).unwrap(), file);
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 0);
        let handle = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(f.all(handle), base());
        f.workspace.release(handle).unwrap();
        let before = f.workspace.metadata_status().unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        assert_eq!(
            f.workspace.metadata_status().unwrap().admission_stopped,
            before.admission_stopped,
            "payload cleanup cleared unrelated metadata failure"
        );
        check("native-metadata-failure-preserves-visible-base-and-quarantine");
        drop(input);
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        let backing = f.workspace.backing_status().unwrap();
        assert_eq!(
            backing.allocated_bytes,
            allocated_files(&f.path.join("private-backing"))
        );
        f.observe("metadata-failure-end");
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        check("explicit-metadata-cleanup-releases-known-partial-allocation");
    }

    fn metadata_files(path: &std::path::Path) -> Vec<PathBuf> {
        use std::os::unix::fs::FileExt;
        let mut found = Vec::new();
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                found.extend(metadata_files(&path));
            } else {
                let mut magic = [0; 8];
                if fs::File::open(&path)
                    .unwrap()
                    .read_at(&mut magic, 0)
                    .unwrap()
                    == 8
                    && &magic == b"LFSWMTA1"
                {
                    found.push(path);
                }
            }
        }
        found
    }
    #[test]
    #[ignore = "requires the actual service and Linux ext4 driver"]
    fn local_range_edit_corruption() {
        use std::os::unix::fs::FileExt;
        let f = Fixture::new(128 * 1024 * 1024);
        let file = f.file(b"data.bin");
        f.edit(b"data.bin", 0, 1, b"x");
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        let next = f.own(b"y");
        let handle = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        let before = f.workspace.backing_status().unwrap();
        let files = metadata_files(&f.path.join("private-backing/edit"));
        assert!(
            !files.is_empty(),
            "dirty pieces and index must exist on disk"
        );
        for path in &files {
            let raw = fs::OpenOptions::new().write(true).open(path).unwrap();
            assert_eq!(raw.write_at(&[0], 0).unwrap(), 1);
        }
        assert!(
            f.workspace.read(handle, 0, 16, deadline()).is_err(),
            "corrupt metadata served bytes"
        );
        assert!(f
            .workspace
            .edit_file_range(
                &WorkspacePath::new(b"data.bin").unwrap(),
                &RangeEdit {
                    start: 0,
                    end: 1,
                    replacement: next.clone()
                },
                deadline()
            )
            .is_err());
        f.workspace.release(handle).unwrap();
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 1);
        let _ = f.workspace.reclaim_metadata(deadline());
        let _ = f.workspace.reclaim_payloads(deadline());
        assert!(f.workspace.backing_status().unwrap().allocated_bytes >= before.allocated_bytes);
        for path in &files {
            assert!(
                path.exists(),
                "cleanup guessed ownership of a corrupt live page"
            );
        }
        // Restore only the bytes this external oracle damaged. No restart or
        // automatic recovery behavior is inferred from this fixture restoration.
        for path in &files {
            let raw = fs::OpenOptions::new().write(true).open(path).unwrap();
            assert_eq!(raw.write_at(b"L", 0).unwrap(), 1);
        }
        println!(
            "LOCAL_EDIT_CORRUPTION files={} retained_allocated={}",
            files.len(),
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        check("corrupt-live-page-refuses-without-dropping-owned-state");
    }

    #[test]
    #[ignore = "requires the actual service and Linux ext4 driver"]
    fn local_range_edit_ledger_collider() {
        use std::os::unix::fs::{FileExt, PermissionsExt};
        let f = Fixture::new(128 * 1024 * 1024);
        let file = f.file(b"data.bin");
        f.edit(b"data.bin", 0, 1, b"x");
        f.edit(b"data.bin", 0, 1, b"y");
        let backing = f.path.join("private-backing/edit");
        let ledger = fs::read_dir(&backing)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                if !path.is_file() {
                    return false;
                }
                let mut magic = [0; 8];
                fs::File::open(path)
                    .unwrap()
                    .read_at(&mut magic, 0)
                    .unwrap()
                    == 8
                    && &magic == b"LFSWOWN1"
            })
            .expect("live metadata needs its owned disk ledger");
        let original = f.path.join("oracle-owned-ledger");
        let contents = fs::read(&ledger).unwrap();
        let identity = fs::metadata(&ledger).unwrap().ino();
        fs::rename(&ledger, &original).unwrap();
        fs::write(&ledger, &contents).unwrap();
        fs::set_permissions(&ledger, fs::Permissions::from_mode(0o600)).unwrap();
        assert_ne!(fs::metadata(&ledger).unwrap().ino(), identity);
        let before = f.workspace.backing_status().unwrap().allocated_bytes;
        assert!(
            f.workspace.reclaim_metadata(deadline()).is_err(),
            "valid copied header conferred ledger ownership"
        );
        assert_eq!(
            fs::read(&ledger).unwrap(),
            contents,
            "cleanup modified the unowned ledger collider"
        );
        assert!(f.workspace.backing_status().unwrap().allocated_bytes >= before);
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 1);
        check("valid-ledger-header-does-not-confer-ownership");
        fs::remove_file(&ledger).unwrap();
        fs::rename(&original, &ledger).unwrap();
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        let handle = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(
            f.workspace.read(handle, 0, 1, deadline()).unwrap().as_ref(),
            b"y"
        );
        f.workspace.release(handle).unwrap();
        f.observe("ledger-collider-end");
        check("explicit-reclaim-after-known-collider-removal");
    }

    #[test]
    #[ignore = "requires the prepared 64MiB immutable-base fixture and actual native service"]
    fn local_range_edit_large_base() {
        let f = Fixture::new(128 * 1024 * 1024);
        let file = f.file(b"data.bin");
        let length = 64 * 1024 * 1024;
        assert_eq!(file.size, length);
        let start = length - 37;
        let replacement = b"local-range-edit";
        assert_eq!(replacement.len(), 16);
        let input = f.own(replacement);
        let before = f.native.read_bytes.load(Ordering::Relaxed);
        let receipt = f
            .workspace
            .edit_file_range(
                &WorkspacePath::new(b"data.bin").unwrap(),
                &RangeEdit {
                    start,
                    end: start + 16,
                    replacement: input.clone(),
                },
                deadline(),
            )
            .unwrap();
        assert_eq!(receipt.accepted_bytes, 16);
        assert_eq!(
            f.native.read_bytes.load(Ordering::Relaxed),
            before,
            "first edit read immutable file content"
        );
        assert_eq!(f.workspace.getattr(file.serial).unwrap().size, length);
        let backing = f.workspace.backing_status().unwrap();
        assert_eq!(backing.payloads, 1);
        assert!(
            backing.allocated_bytes + backing.reserved_bytes < 2 * 1024 * 1024,
            "small edit reserved or copied a file-sized private extent"
        );
        check("large-base-edit-keeps-inherited-bytes-as-references");
        let handle = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        for offset in [0, length / 2, start - 64, length - 128] {
            let count = (length - offset).min(128) as usize;
            let expected: Vec<_> = (offset..offset + count as u64)
                .map(|i| {
                    if i >= start && i < start + 16 {
                        replacement[(i - start) as usize]
                    } else {
                        (i % 251) as u8
                    }
                })
                .collect();
            assert_eq!(
                f.workspace
                    .read(handle, offset, count, deadline())
                    .unwrap()
                    .as_ref(),
                expected
            );
        }
        f.workspace.release(handle).unwrap();
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        f.observe("large-base-end");
        println!("LOCAL_EDIT_LARGE_BASE logical_bytes={length} accepted_bytes=16 oracle=edited-and-distant-windows full_workspace_readback=NOT_RUN");
        check("large-base-edited-and-distant-window-oracles");
    }

    #[test]
    #[ignore = "requires the actual service and Linux ext4 driver"]
    fn local_range_edit_ledger_write_failure() {
        let f = Fixture::new(128 * 1024 * 1024);
        let file = f.file(b"data.bin");
        f.edit(b"data.bin", 0, 1, b"x");
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        let original = f.workspace.getattr(file.serial).unwrap();
        let input = f.own(b"y");
        let before = f.workspace.backing_status().unwrap();
        let ledger = fs::read_dir(f.path.join("private-backing/edit"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("m-ledger-")
            })
            .unwrap();
        let ledger_before = fs::read(&ledger).unwrap();
        assert_eq!(ledger_before.len(), 4096);
        assert!(fs::read_to_string("/proc/self/limits")
            .unwrap()
            .lines()
            .any(
                |line| line.starts_with("Max file size") && line.matches("unlimited").count() == 2
            ));
        file_limit("2048");
        let limit = LimitedFile;
        let result = f.workspace.edit_file_range(
            &WorkspacePath::new(b"data.bin").unwrap(),
            &RangeEdit {
                start: 0,
                end: 1,
                replacement: input.clone(),
            },
            deadline(),
        );
        drop(limit);
        let WorkspaceError::Backing(failure) = result.unwrap_err() else {
            panic!("native ledger cause must remain typed")
        };
        assert_eq!(failure.phase, BackingPhase::Write);
        assert!(matches!(
            failure.kind,
            std::io::ErrorKind::WriteZero
                | std::io::ErrorKind::InvalidInput
                | std::io::ErrorKind::FileTooLarge
        ));
        let ledger_after = fs::read(&ledger).unwrap();
        assert_eq!(ledger_after.len(), 4096);
        let positive_short_write = failure.kind == std::io::ErrorKind::WriteZero
            && ledger_before[..2048] != ledger_after[..2048];
        if failure.kind == std::io::ErrorKind::WriteZero {
            assert!(
                positive_short_write,
                "WriteZero alone does not prove a positive native write"
            );
            assert_eq!(&ledger_before[2048..], &ledger_after[2048..]);
        }
        assert_eq!(f.workspace.getattr(file.serial).unwrap(), original);
        assert!(f.workspace.metadata_status().unwrap().admission_stopped);
        drop(input);
        f.workspace.reclaim_payloads(deadline()).unwrap();
        let after = f.workspace.backing_status().unwrap();
        assert!(after.allocated_bytes >= before.allocated_bytes);
        assert!(
            after.retained_payloads >= 2,
            "uncertain custody write released its prospective payload"
        );
        assert!(f.workspace.reclaim_metadata(deadline()).is_err());
        assert!(f.workspace.metadata_status().unwrap().admission_stopped);
        println!("LOCAL_EDIT_NATIVE_FAILURE existing_ledger_write={failure:?} positive_short_write={} retained={after:?}", positive_short_write);
        check("native-ledger-write-failure-retains-prospective-payload-custody");
    }

    #[test]
    #[ignore = "requires the actual service and Linux ext4 driver"]
    fn local_range_edit_native_shape() {
        use std::os::unix::fs::FileExt;
        let f = Fixture::with_count(128 * 1024 * 1024, 3);
        let file = f.file(b"data.bin");
        f.edit(b"data.bin", 0, 1, b"x");
        let grown = f
            .host
            .attach(
                Fixture::options(&f.path, "grown", 34, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        let healthy = f
            .host
            .attach(
                Fixture::options(&f.path, "healthy", 35, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        for workspace in [&grown, &healthy] {
            let input = workspace
                .own_payload(1, &mut &b"z"[..], deadline())
                .unwrap();
            workspace
                .edit_file_range(
                    &WorkspacePath::new(b"data.bin").unwrap(),
                    &RangeEdit {
                        start: 0,
                        end: 1,
                        replacement: input,
                    },
                    deadline(),
                )
                .unwrap();
        }
        let second = healthy.own_payload(1, &mut &b"q"[..], deadline()).unwrap();
        healthy
            .edit_file_range(
                &WorkspacePath::new(b"data.bin").unwrap(),
                &RangeEdit {
                    start: 0,
                    end: 1,
                    replacement: second,
                },
                deadline(),
            )
            .unwrap();
        let grown_file = grown
            .lookup(
                grown.root().serial,
                b"data.bin",
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        let handle = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        let grown_handle = grown
            .open(grown_file.serial, ReferenceScope::Local)
            .unwrap();
        let page = metadata_files(&f.path.join("private-backing/edit"))
            .pop()
            .unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&page)
            .unwrap()
            .set_len(0)
            .unwrap();
        for page in metadata_files(&f.path.join("private-backing/grown")) {
            let file = fs::OpenOptions::new().write(true).open(page).unwrap();
            file.set_len(8192).unwrap();
            assert_eq!(file.write_at(&[0; 4096], 4096).unwrap(), 4096);
        }
        assert!(matches!(
            f.workspace.read(handle, 0, 16, deadline()),
            Err(WorkspaceError::Backing(_))
        ));
        assert!(matches!(
            grown.read(grown_handle, 0, 16, deadline()),
            Err(WorkspaceError::Backing(_))
        ));
        assert!(!f.workspace.metadata_status().unwrap().accounting_complete);
        assert!(!f.workspace.backing_status().unwrap().accounting_complete);
        check("native-truncation-and-growth-invalidate-allocation-observation");
        healthy.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        assert!(
            !f.workspace.metadata_status().unwrap().accounting_complete,
            "healthy-arena cleanup erased another arena's incomplete observation"
        );
        assert!(f.workspace.metadata_status().unwrap().admission_stopped);
        f.workspace.release(handle).unwrap();
        grown.release(grown_handle).unwrap();
        println!(
            "LOCAL_EDIT_NATIVE_SHAPE metadata={:?} backing={:?} observed_file_blocks={}",
            f.workspace.metadata_status().unwrap(),
            f.workspace.backing_status().unwrap(),
            allocated_files(&f.path.join("private-backing"))
        );
        check("healthy-cleanup-preserves-other-arena-incompleteness");
    }

    #[test]
    #[ignore = "requires all 104 existing regular inodes in the closed R1 fixture"]
    fn local_range_edit_inode_frontier() {
        let f = Fixture::new(128 * 1024 * 1024);
        let mut names: Vec<Vec<u8>> = (0..100)
            .map(|i| format!("entry-{i:03}-{}", "x".repeat(180)).into_bytes())
            .collect();
        names.extend([
            b"data.bin".to_vec(),
            b"unread.bin".to_vec(),
            b"tool".to_vec(),
            b"root-readable".to_vec(),
        ]);
        assert_eq!(names.len(), 104);
        for (index, name) in names.iter().enumerate() {
            f.edit(name, 0, 1, b"z");
            if index % 8 == 7 {
                f.workspace.reclaim_metadata(deadline()).unwrap();
                f.workspace.reclaim_payloads(deadline()).unwrap();
            }
        }
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        let mut expected = std::collections::BTreeSet::new();
        for name in &names {
            let attr = f.file(name);
            assert!(expected.insert(attr.serial));
            let handle = f
                .workspace
                .open(attr.serial, ReferenceScope::Local)
                .unwrap();
            assert_eq!(
                f.workspace.read(handle, 0, 1, deadline()).unwrap().as_ref(),
                b"z"
            );
            f.workspace.release(handle).unwrap();
        }
        let alias = f.file(b"alias");
        assert!(expected.contains(&alias.serial));
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 104);
        check("all-104-existing-inodes-survive-global-index-splits");
        // Independent inspection of the documented private cell framing checks
        // the maintained frontier after explicit old-root reclamation. This is
        // an oracle, never a product lookup, decoder include or public test hook.
        let mut observed = std::collections::BTreeSet::new();
        for path in metadata_files(&f.path.join("private-backing/edit")) {
            let data = fs::read(path).unwrap();
            assert_eq!(data.len(), 4096);
            if data[48] != 0 {
                continue;
            }
            let count = u16::from_be_bytes([data[50], data[51]]);
            let mut at = 128;
            for _ in 0..count {
                let k = u16::from_be_bytes([data[at], data[at + 1]]) as usize;
                let v = u16::from_be_bytes([data[at + 2], data[at + 3]]) as usize;
                at += 4;
                let key = &data[at..at + k];
                let value = &data[at + k..at + k + v];
                at += k + v;
                if k == 17 && key[0] == b'D' {
                    assert_eq!(&key[1..9], &1u64.to_be_bytes());
                    assert_eq!(value, &[1]);
                    assert!(
                        observed.insert(u64::from_be_bytes(key[9..17].try_into().unwrap())),
                        "duplicate retained dirty record"
                    );
                }
            }
        }
        assert_eq!(observed, expected);
        f.observe("inode-frontier-end");
        check("disk-dirty-frontier-matches-all-104-public-edits");
    }
}
