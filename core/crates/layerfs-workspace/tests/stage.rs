//! Real Workspace capture/save/stage calls. The external driver owns the live
//! service, independent Store copy, Linux backing and native save observation.
#[cfg(target_os = "linux")]
mod linux {
    use layerfs_bridge::{
        adapters::native::{client::Client, connection::connect_until, pipe},
        contract::*,
    };
    use layerfs_workspace::*;
    use std::{
        io::Write,
        net::{SocketAddr, ToSocketAddrs},
        path::PathBuf,
        sync::{Arc, Condvar, Mutex},
        time::{Duration, Instant},
    };

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(10)
    }
    fn hex<const N: usize>(name: &str) -> [u8; N] {
        let value = std::env::var(name).unwrap();
        assert_eq!(value.len(), N * 2);
        std::array::from_fn(|i| u8::from_str_radix(&value[2 * i..2 * i + 2], 16).unwrap())
    }
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Gate {
        None,
        Delivery,
        NativeSave,
        CompletionFailure,
    }
    #[derive(Default)]
    struct Observations {
        operations: Vec<Operation>,
        entered: bool,
        released: bool,
        saved_files: Vec<Root>,
    }
    struct Native {
        endpoint: SocketAddr,
        private: Root,
        server: Root,
        gate: Gate,
        observations: Mutex<Observations>,
        changed: Condvar,
    }
    impl Native {
        fn new(gate: Gate) -> Arc<Self> {
            Arc::new(Self {
                endpoint: std::env::var("LAYERFS_ENDPOINT")
                    .unwrap()
                    .to_socket_addrs()
                    .unwrap()
                    .next()
                    .unwrap(),
                private: pipe::key(&std::env::var("LAYERFS_PRIVATE_KEY").unwrap()).unwrap(),
                server: pipe::key(&std::env::var("LAYERFS_SERVER_KEY").unwrap()).unwrap(),
                gate,
                observations: Mutex::new(Observations::default()),
                changed: Condvar::new(),
            })
        }
        fn call(
            &self,
            request: &Request,
            input: &mut dyn Source,
            output: &mut dyn Write,
            end: Instant,
        ) -> Result<Response, Failure> {
            let first_save = {
                let mut observations = self.observations.lock().unwrap();
                assert!(
                    !matches!(
                        request.operation,
                        Operation::UpdatePreparedFilesystem { .. }
                            | Operation::ConstructFile { .. }
                    ),
                    "unexpected construction route"
                );
                let first = matches!(request.operation, Operation::EditFile { .. })
                    && !observations.entered;
                observations.operations.push(request.operation.clone());
                if first {
                    observations.entered = true;
                    self.changed.notify_all();
                }
                first
            };
            if first_save && self.gate == Gate::Delivery {
                let observations = self.observations.lock().unwrap();
                let (_state, timed) = self
                    .changed
                    .wait_timeout_while(observations, Duration::from_secs(5), |s| !s.released)
                    .unwrap();
                assert!(!timed.timed_out(), "external test gate was not released");
            }
            if first_save && self.gate == Gate::NativeSave {
                println!("STAGE_SAVE_START");
                std::io::stdout().flush().unwrap();
            }
            let mut client = Client::new(connect_until(
                self.endpoint,
                1,
                &self.private,
                &self.server,
                end,
            )?)?;
            let response = client.call_until(request, input, output, end);
            if matches!(request.operation, Operation::EditFile { .. }) {
                if let Ok(Response::Saved { root, .. }) = &response {
                    self.observations.lock().unwrap().saved_files.push(*root);
                }
            }
            if first_save && self.gate == Gate::CompletionFailure && response.is_ok() {
                file_limit("2048");
                println!("STAGE_BACKING_LIMIT_APPLIED");
            }
            if first_save && self.gate == Gate::NativeSave {
                println!("STAGE_SAVE_DONE");
                std::io::stdout().flush().unwrap();
            }
            response
        }
        fn delivery(self: &Arc<Self>) -> OperationDelivery {
            let native = self.clone();
            Arc::new(move |r, i, o, d| native.call(r, i, o, d))
        }
        fn wait_entered(&self) {
            let state = self.observations.lock().unwrap();
            let (_state, timeout) = self
                .changed
                .wait_timeout_while(state, Duration::from_secs(5), |s| !s.entered)
                .unwrap();
            assert!(!timeout.timed_out());
        }
        fn release(&self) {
            self.observations.lock().unwrap().released = true;
            self.changed.notify_all();
        }
        fn request(
            &self,
            operation: Operation,
            response_bytes: u64,
            output: &mut dyn Write,
        ) -> Result<Response, Failure> {
            let profile = if matches!(
                operation,
                Operation::HistoryQuery(_) | Operation::HistoryCommand(_)
            ) {
                HISTORY_PROFILE
            } else {
                1
            };
            self.call(
                &Request {
                    id: 55,
                    generation: 1,
                    store: 1,
                    profile,
                    deadline_ms: 10000,
                    response_bytes,
                    operation,
                },
                &mut &[][..],
                output,
                deadline(),
            )
        }
        fn query(&self, query: HistoryQuery) -> Response {
            self.request(Operation::HistoryQuery(query), 16384, &mut std::io::sink())
                .unwrap()
        }
        fn attributes(&self, root: Root, path: &[u8]) -> Response {
            self.request(
                Operation::Inspect {
                    root,
                    query: Inspect::Attributes {
                        path: path.to_vec(),
                    },
                },
                16384,
                &mut std::io::sink(),
            )
            .unwrap()
        }
        fn bytes(&self, root: Root, start: u64, length: usize) -> Vec<u8> {
            let mut output = Vec::new();
            assert_eq!(
                self.request(
                    Operation::ReadFile {
                        root,
                        start,
                        end: start + length as u64
                    },
                    length as u64,
                    &mut output
                )
                .unwrap(),
                Response::Read {
                    length: length as u64
                }
            );
            output
        }
    }
    struct Fixture {
        host: WorkspaceHost,
        workspace: Workspace,
        native: Arc<Native>,
    }
    impl Fixture {
        fn new(gate: Gate) -> Self {
            Self::with_quota(gate, 64 * 1024 * 1024)
        }
        fn with_quota(gate: Gate, quota: u64) -> Self {
            let native = Native::new(gate);
            let host = WorkspaceHost::new(
                WorkspaceConfig {
                    root: PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()),
                    max_count: 3,
                    memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                    disk_budget_bytes: Some(quota),
                },
                native.delivery(),
            )
            .unwrap();
            let workspace = host
                .attach(
                    Self::options("stage", 31, WorkspaceAccess::LocalEdit),
                    deadline(),
                )
                .unwrap();
            Self {
                host,
                workspace,
                native,
            }
        }
        fn options(id: &str, incarnation: u8, access: WorkspaceAccess) -> AttachOptions {
            AttachOptions {
                id: id.into(),
                incarnation: [incarnation; 32],
                store: 1,
                base: Base::Branch(hex("LAYERFS_STAGE_BRANCH")),
                access,
                owner_uid: 0,
                owner_gid: 0,
            }
        }
        fn lookup(&self, name: &[u8]) -> NodeAttributes {
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
        fn edit(&self, name: &[u8], start: u64, end: u64, bytes: &[u8]) -> MutationReceipt {
            self.workspace
                .edit_file_range(
                    &WorkspacePath::new(name).unwrap(),
                    &RangeEdit {
                        start,
                        end,
                        replacement: self.own(bytes),
                    },
                    deadline(),
                )
                .unwrap()
        }
        fn read(&self, handle: HandleId, start: u64, length: usize) -> Vec<u8> {
            self.workspace
                .read(handle, start, length, deadline())
                .unwrap()
                .as_ref()
                .to_vec()
        }
        fn branch(&self) -> Response {
            self.native.query(HistoryQuery::GetBranch {
                branch: hex("LAYERFS_STAGE_BRANCH"),
            })
        }
        fn counts(&self, files: usize) {
            let observed = self.native.observations.lock().unwrap();
            assert_eq!(
                observed
                    .operations
                    .iter()
                    .filter(|op| matches!(op, Operation::EditFile { .. }))
                    .count(),
                files
            );
            assert_eq!(
                observed
                    .operations
                    .iter()
                    .filter(|op| matches!(op, Operation::UpdatePortableMetadata { .. }))
                    .count(),
                files
            );
            assert_eq!(
                observed
                    .operations
                    .iter()
                    .filter(|op| matches!(
                        op,
                        Operation::HistoryCommand(HistoryCommand::StageChanges(_))
                    ))
                    .count(),
                1
            );
            assert_eq!(observed.saved_files.len(), files);
            for operation in &observed.operations {
                if let Operation::HistoryCommand(HistoryCommand::StageChanges(prepared)) = operation
                {
                    assert!(prepared.directories.is_empty());
                    assert_eq!(prepared.inodes.len(), files);
                    let mut serials: Vec<_> = prepared.inodes.iter().map(|i| i.serial).collect();
                    serials.sort();
                    serials.dedup();
                    assert_eq!(serials.len(), files);
                }
            }
        }
    }
    fn attr(response: Response) -> (u64, Root, u64, i64, u32) {
        let Response::Attributes {
            serial,
            content,
            size,
            mtime,
            nanoseconds,
            ..
        } = response
        else {
            panic!("attributes missing")
        };
        (serial, content, size, mtime, nanoseconds)
    }
    fn check(id: &str) {
        println!("STAGE_CHECK {id} PASS");
    }
    fn stage_retained(f: &Fixture, selector: StageSelector) {
        let stage = selector.stage().clone();
        assert_eq!(
            f.native.query(HistoryQuery::GetStage {
                workspace: [31; 32]
            }),
            Response::History(Box::new(HistoryResult::Stage(stage.clone())))
        );
        let status = f.workspace.status().unwrap();
        let submission = status.submission.unwrap();
        assert_eq!(submission.phase, StagePhase::Staged);
        assert_eq!(submission.stage_token, Some(stage.token));
        assert_eq!(submission.candidate_root, Some(stage.candidate_root));
        assert_eq!(submission.generation, stage.generation);
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        drop(selector);
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
        println!(
            "STAGE_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
        check("exact-stage-and-retained-submission");
    }

    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_semantics() {
        let f = Fixture::new(Gate::Delivery);
        let branch = f.branch();
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::InvalidInput)
        ));
        assert_eq!(f.workspace.status().unwrap().generation, 1);
        let readonly = f
            .host
            .attach(
                Fixture::options("readonly", 32, WorkspaceAccess::ReadOnly),
                deadline(),
            )
            .unwrap();
        assert!(matches!(
            readonly.stage(deadline()),
            Err(WorkspaceError::ReadOnly)
        ));
        readonly.close_clean().unwrap();
        let data = f.lookup(b"data.bin");
        let alias = f.lookup(b"alias");
        assert_eq!(data.serial, alias.serial);
        f.lookup(b"other.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        let captured = f.edit(b"data.bin", 10, 14, b"GGGG");
        f.edit(b"other.bin", 8, 10, b"OO");
        let captured_attr = f.workspace.getattr(data.serial).unwrap();
        let second = f
            .host
            .attach(
                Fixture::options("second", 33, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        second
            .lookup(
                second.root().serial,
                b"data.bin",
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        let payload = second.own_payload(1, &mut &b"B"[..], deadline()).unwrap();
        second
            .edit_file_range(
                &WorkspacePath::new(b"data.bin").unwrap(),
                &RangeEdit {
                    start: 0,
                    end: 1,
                    replacement: payload,
                },
                deadline(),
            )
            .unwrap();
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        f.native.wait_entered();
        assert!(matches!(
            second.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(
            f.workspace.status().unwrap().generation,
            captured.generation + 1
        );
        let successor = f.edit(b"data.bin", 10, 14, b"LIVE");
        assert_eq!(successor.generation, captured.generation + 1);
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        check("capture-live-successor-and-consumer-admission");
        f.native.release();
        let selector = saving.join().unwrap().unwrap();
        let saved = attr(
            f.native
                .attributes(selector.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(saved.0, data.serial);
        assert_eq!(saved.2, data.size);
        assert_eq!(
            (saved.3, saved.4),
            (captured_attr.mtime_seconds, captured_attr.mtime_nanoseconds)
        );
        assert_eq!(
            f.native.bytes(saved.1, 8, 8),
            [8, 9, b'G', b'G', b'G', b'G', 14, 15]
        );
        assert_eq!(
            attr(
                f.native
                    .attributes(selector.stage().candidate_root, b"alias")
            ),
            saved
        );
        let other = attr(
            f.native
                .attributes(selector.stage().candidate_root, b"other.bin"),
        );
        assert_eq!(f.native.bytes(other.1, 8, 2), b"OO");
        assert_eq!(f.branch(), branch);
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        f.edit(b"alias", 11, 13, b"x");
        assert_eq!(f.read(handle, 10, 3), b"LxE");
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        assert_eq!(f.read(handle, 10, 3), b"LxE");
        f.counts(2);
        check("frozen-bytes-attributes-aliases-and-one-tree-save");
        stage_retained(&f, selector);
        f.workspace.release(handle).unwrap();
    }

    fn native_save(kill: bool) {
        let f = Fixture::new(Gate::NativeSave);
        let branch = f.branch();
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        let mut bytes = vec![0; 4 * 1024 * 1024];
        let mut random = 0x6a09e667f3bcc909u64;
        for byte in &mut bytes {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            *byte = random as u8;
        }
        f.edit(b"data.bin", 0, bytes.len() as u64, &bytes);
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "SAVE_PAUSED");
        f.edit(b"data.bin", 10, 14, b"LIVE");
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        println!("STAGE_LIVE_READY");
        std::io::stdout().flush().unwrap();
        let result = saving.join().unwrap();
        if kill {
            let error = result.unwrap_err();
            let WorkspaceError::Stage(failure) = error else {
                panic!("lost save must retain capture: {error:?}")
            };
            assert_eq!(failure.disposition, StageFailureDisposition::Unknown);
            assert_eq!(failure.phase, StagePhase::FileSave);
            assert!(matches!(&failure.cause,WorkspaceError::Service(failure) if failure.unknown));
            assert_eq!(
                f.workspace.status().unwrap().submission.unwrap().phase,
                StagePhase::Failed
            );
            assert!(matches!(
                f.workspace.stage(deadline()),
                Err(WorkspaceError::Busy)
            ));
            f.edit(b"data.bin", 10, 14, b"NEXT");
            assert_eq!(f.read(handle, 10, 4), b"NEXT");
            assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
            println!("STAGE_FAILURE {failure:?}");
            check("unknown-save-retains-G-and-live-successor");
        } else {
            let selector = result.unwrap();
            let saved = attr(
                f.native
                    .attributes(selector.stage().candidate_root, b"data.bin"),
            );
            assert_eq!(
                f.native.bytes(saved.1, 0, MAX_READ_BYTES),
                bytes[..MAX_READ_BYTES]
            );
            assert_eq!(f.read(handle, 10, 4), b"LIVE");
            assert_eq!(f.branch(), branch);
            f.counts(1);
            check("actual-service-save-with-live-successor-progress");
            stage_retained(&f, selector);
        }
    }
    #[test]
    #[ignore = "requires stage_route.py native SQLite save observation"]
    fn stage_native_save() {
        native_save(false);
    }
    #[test]
    #[ignore = "requires stage_route.py native SQLite save loss"]
    fn stage_unknown_save() {
        native_save(true);
    }

    #[test]
    #[ignore = "requires stage_route.py with metadata grant denied"]
    fn stage_metadata_denied() {
        let f = Fixture::new(Gate::None);
        let branch = f.branch();
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let error = f.workspace.stage(deadline()).unwrap_err();
        let WorkspaceError::Stage(failure) = error else {
            panic!("capture failure not retained: {error:?}")
        };
        assert_eq!(failure.phase, StagePhase::MetadataSave);
        assert_eq!(
            failure.disposition,
            StageFailureDisposition::KnownBeforeStage
        );
        assert!(
            matches!(&failure.cause,WorkspaceError::Service(failure) if failure.code==Code::Denied && !failure.unknown)
        );
        let pending = failure.pending.unwrap();
        assert_eq!(pending.serial, data.serial);
        assert!(pending.metadata.is_none());
        assert_eq!(f.native.bytes(pending.content, 10, 4), b"GGGG");
        assert!(failure.source_failure.is_none());
        let status = f.workspace.status().unwrap().submission.unwrap();
        assert_eq!(status.saved_files, 1);
        assert_eq!(status.saved_metadata, 0);
        assert_eq!(status.stage_token, None);
        assert_eq!(f.branch(), branch);
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        f.edit(b"data.bin", 10, 14, b"LIVE");
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
        let observations = f.native.observations.lock().unwrap();
        assert!(!observations
            .operations
            .iter()
            .any(|op| matches!(op, Operation::HistoryCommand(_))));
        println!("STAGE_FAILURE {failure:?}");
        check("known-metadata-denial-retains-saved-file-G-and-D1");
    }
    #[test]
    #[ignore = "requires stage_route.py and native service"]
    fn stage_lowering() {
        let f = Fixture::new(Gate::None);
        let branch = f.branch();
        let data = f.lookup(b"data.bin");
        let mut oracle: Vec<u8> = (0..2048).map(|i| (i % 251) as u8).collect();
        for (start, end, bytes) in [
            (100, 110, b"abc".as_slice()),
            (200, 200, b"12345".as_slice()),
            (500, 700, b"".as_slice()),
        ] {
            f.edit(b"data.bin", start, end, bytes);
            oracle.splice(start as usize..end as usize, bytes.iter().copied());
        }
        let selector = f.workspace.stage(deadline()).unwrap();
        let saved = attr(
            f.native
                .attributes(selector.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(saved.2, data.size - 202);
        assert_eq!(f.native.bytes(saved.1, 0, 1024), oracle[..1024]);
        let offset = 32 * 1024 * 1024u64;
        let distant: Vec<_> = (offset + 202..offset + 202 + 512)
            .map(|i| (i % 251) as u8)
            .collect();
        assert_eq!(f.native.bytes(saved.1, offset, 512), distant);
        assert_eq!(f.branch(), branch);
        f.counts(1);
        let observed = f.native.observations.lock().unwrap();
        let edits = observed
            .operations
            .iter()
            .find_map(|op| {
                if let Operation::EditFile { edits, .. } = op {
                    Some(edits)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(edits.iter().map(|e| e.replacement).sum::<u64>(), 8);
        drop(observed);
        check("normalized-splice-lowering-and-streamed-exact-input");
        stage_retained(&f, selector);
    }

    #[test]
    #[ignore = "requires stage_route.py and an independent Branch writer"]
    fn stage_head_moved() {
        let f = Fixture::new(Gate::Delivery);
        let data = f.lookup(b"data.bin");
        let Response::History(before) = f.branch() else {
            panic!("missing branch")
        };
        let HistoryResult::BranchSnapshot(before) = *before else {
            panic!("missing snapshot")
        };
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        f.native.wait_entered();
        let response = f
            .native
            .request(
                Operation::HistoryCommand(HistoryCommand::Commit(PreparedChanges {
                    workspace: [44; 32],
                    branch: before.branch.branch,
                    expected_head: before.branch.head_commit,
                    expected_base: before.branch.base_layer,
                    generation: 1,
                    base: before.effective_root,
                    scope: before.scope,
                    root_serial: before.root_serial.unwrap(),
                    directories: vec![DirectoryChange {
                        parent: before.root_serial.unwrap(),
                        changes: vec![
                            (b"alias".to_vec(), None),
                            (b"new-alias".to_vec(), Some(data.serial)),
                        ],
                    }],
                    inodes: vec![],
                })),
                16384,
                &mut std::io::sink(),
            )
            .unwrap();
        let Response::History(response) = response else {
            panic!("missing Commit")
        };
        let HistoryResult::Committed(CommitOutcomeWire::Committed(committed)) = *response else {
            panic!("Commit did not advance")
        };
        f.native.release();
        let error = saving.join().unwrap().unwrap_err();
        let WorkspaceError::Stage(failure) = error else {
            panic!("missing retained failure: {error:?}")
        };
        assert_eq!(failure.phase, StagePhase::StageChanges);
        assert_eq!(
            failure.disposition,
            StageFailureDisposition::KnownBeforeStage
        );
        let WorkspaceError::Service(cause) = &failure.cause else {
            panic!("missing service conflict")
        };
        assert_eq!(cause.code, Code::HeadMoved);
        assert!(!cause.unknown);
        let context = cause.history.as_ref().unwrap();
        assert_eq!(
            context.conflict,
            Some(HistoryConflict::BranchMoved {
                expected_head: before.branch.head_commit,
                actual_head: Some(committed.commit),
                expected_base: before.branch.base_layer,
                actual_base: before.branch.base_layer
            })
        );
        assert!(matches!(
            context.stage,
            StageObservation::Unobserved | StageObservation::Absent(_)
        ));
        assert!(failure.observed_stage.is_none());
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        f.edit(b"data.bin", 10, 14, b"LIVE");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        let query = f
            .native
            .request(
                Operation::HistoryQuery(HistoryQuery::GetStage {
                    workspace: [31; 32],
                }),
                16384,
                &mut std::io::sink(),
            )
            .unwrap_err();
        assert_eq!(query.code, Code::NotFound);
        assert!(!query.unknown);
        println!("STAGE_FAILURE {failure:?}");
        check("stale-captured-head-preserves-exact-conflict-and-live-state");
    }
    #[test]
    #[ignore = "requires stage_route.py wide live fixture"]
    fn stage_frontier() {
        let f = Fixture::new(Gate::None);
        let branch = f.branch();
        let mut names = vec![b"data.bin".to_vec(), b"other.bin".to_vec()];
        names.extend((0..102).map(|i| format!("f{i:03}").into_bytes()));
        let mut serials = Vec::new();
        for (index, name) in names.iter().enumerate() {
            serials.push(f.lookup(name).serial);
            f.edit(name, 10, 12, &[index as u8, 0xfe]);
            f.workspace.reclaim_metadata(deadline()).unwrap();
        }
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 104);
        let selector = f
            .workspace
            .stage(Instant::now() + Duration::from_secs(25))
            .unwrap();
        let status = f.workspace.status().unwrap().submission.unwrap();
        assert_eq!(status.saved_files, 104);
        assert_eq!(status.saved_metadata, 104);
        for (index, name) in names.iter().enumerate() {
            let saved = attr(f.native.attributes(selector.stage().candidate_root, name));
            assert_eq!(saved.0, serials[index]);
            assert_eq!(f.native.bytes(saved.1, 10, 2), [index as u8, 0xfe]);
        }
        assert_eq!(f.branch(), branch);
        f.counts(104);
        check("all-104-inode-completions-persist-and-stage");
        stage_retained(&f, selector);
    }
    fn file_limit(value: &str) {
        assert!(std::process::Command::new("prlimit")
            .arg("--pid")
            .arg(std::process::id().to_string())
            .arg(format!("--fsize={value}:"))
            .status()
            .unwrap()
            .success());
    }
    struct RestoreFileLimit;
    impl Drop for RestoreFileLimit {
        fn drop(&mut self) {
            file_limit("unlimited");
        }
    }
    #[test]
    #[ignore = "requires stage_route.py ignoring XFSZ for actual native failure"]
    fn stage_completion_failure() {
        let f = Fixture::new(Gate::CompletionFailure);
        let data = f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let restore = RestoreFileLimit;
        let result = f.workspace.stage(deadline());
        drop(restore);
        let WorkspaceError::Stage(failure) = result.unwrap_err() else {
            panic!("missing captured native failure")
        };
        assert!(matches!(failure.cause, WorkspaceError::Backing(_)));
        let observed = f.native.observations.lock().unwrap();
        assert_eq!(observed.saved_files.len(), 1);
        let root = observed.saved_files[0];
        let pending = failure.pending.unwrap();
        assert_eq!(pending.content, root);
        assert_eq!(pending.serial, data.serial);
        assert!(pending.metadata.is_some());
        assert_eq!(failure.phase, StagePhase::LocalBookkeeping);
        assert_eq!(
            observed
                .operations
                .iter()
                .filter(|op| matches!(op, Operation::UpdatePortableMetadata { .. }))
                .count(),
            1
        );
        assert!(!observed
            .operations
            .iter()
            .any(|op| matches!(op, Operation::HistoryCommand(_))));
        drop(observed);
        assert_eq!(f.native.bytes(root, 10, 4), b"GGGG");
        let status = f.workspace.status().unwrap().submission.unwrap();
        assert_eq!(status.phase, StagePhase::Failed);
        assert_eq!(status.saved_files, 1);
        assert_eq!(status.saved_metadata, 1);
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
        assert_eq!(f.workspace.getattr(data.serial).unwrap().size, data.size);
        println!("STAGE_FAILURE {failure:?}");
        println!(
            "STAGE_RESOURCE {:?} {:?}",
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
        check("known-file-save-survives-native-completion-publication-failure");
    }
    #[test]
    #[ignore = "requires stage_route.py live metadata-only save"]
    fn stage_metadata_only() {
        let f = Fixture::new(Gate::None);
        let branch = f.branch();
        let data = f.lookup(b"data.bin");
        let Response::History(snapshot) = branch.clone() else {
            panic!("missing branch")
        };
        let HistoryResult::BranchSnapshot(snapshot) = *snapshot else {
            panic!("missing snapshot")
        };
        let original = attr(f.native.attributes(snapshot.effective_root, b"data.bin"));
        f.edit(b"data.bin", 100, 100, b"");
        let captured = f.workspace.getattr(data.serial).unwrap();
        let selector = f.workspace.stage(deadline()).unwrap();
        let saved = attr(
            f.native
                .attributes(selector.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(
            (saved.0, saved.1, saved.2),
            (original.0, original.1, original.2)
        );
        assert_eq!(
            (saved.3, saved.4),
            (captured.mtime_seconds, captured.mtime_nanoseconds)
        );
        assert_eq!(f.branch(), branch);
        let observed = f.native.observations.lock().unwrap();
        assert_eq!(
            observed
                .operations
                .iter()
                .filter(|op| matches!(op, Operation::EditFile { .. }))
                .count(),
            0
        );
        assert_eq!(
            observed
                .operations
                .iter()
                .filter(|op| matches!(op, Operation::UpdatePortableMetadata { .. }))
                .count(),
            1
        );
        assert_eq!(
            observed
                .operations
                .iter()
                .filter(|op| matches!(
                    op,
                    Operation::HistoryCommand(HistoryCommand::StageChanges(_))
                ))
                .count(),
            1
        );
        drop(observed);
        let status = f.workspace.status().unwrap().submission.unwrap();
        assert_eq!(status.saved_files, 0);
        assert_eq!(status.saved_metadata, 1);
        check("metadata-only-stage-preserves-root-without-file-save");
        stage_retained(&f, selector);
    }
    #[test]
    #[ignore = "requires stage_route.py with ordinary quota occupied"]
    fn stage_headroom() {
        let f = Fixture::with_quota(Gate::None, 2 * 1024 * 1024);
        let data = f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let before = f.workspace.backing_status().unwrap();
        let available = before.quota_bytes - before.allocated_bytes - before.reserved_bytes;
        let mut blocks = available / 4096;
        while blocks + blocks.div_ceil(256) > available / 4096 {
            blocks -= 1;
        }
        let spare = f.own(&vec![0xcc; blocks as usize * 4096]);
        let full = f.workspace.backing_status().unwrap();
        assert!(full.quota_bytes - full.allocated_bytes - full.reserved_bytes <= 4096);
        let selector = f.workspace.stage(deadline()).unwrap();
        let after = f.workspace.backing_status().unwrap();
        assert!(after.allocated_bytes > full.allocated_bytes);
        assert!(after.reserved_bytes < full.reserved_bytes);
        assert_eq!(
            after.allocated_bytes + after.reserved_bytes,
            full.allocated_bytes + full.reserved_bytes
        );
        let saved = attr(
            f.native
                .attributes(selector.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(saved.0, data.serial);
        assert_eq!(f.native.bytes(saved.1, 10, 4), b"GGGG");
        println!("STAGE_HEADROOM before={before:?} full={full:?} after={after:?} unrelated_owned_bytes={}",spare.len());
        check("reserved-stage-progress-with-ordinary-disk-quota-occupied");
        stage_retained(&f, selector);
    }
}
