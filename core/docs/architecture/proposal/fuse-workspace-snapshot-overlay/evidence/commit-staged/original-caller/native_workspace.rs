//! Shared external native-service fixture; no product test hooks.
#![allow(dead_code)]
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

pub fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
pub fn hex<const N: usize>(name: &str) -> [u8; N] {
    let value = std::env::var(name).unwrap();
    assert_eq!(value.len(), N * 2);
    std::array::from_fn(|i| u8::from_str_radix(&value[2 * i..2 * i + 2], 16).unwrap())
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    None,
    Delivery,
    NativeSave,
    CompletionFailure,
    CommitReply,
    CommitCompletionFailure,
    CommitUnknown,
    CommitDenied,
    ReadHold,
}
#[derive(Default)]
pub struct Observations {
    pub operations: Vec<Operation>,
    pub entered: bool,
    pub released: bool,
    pub saved_files: Vec<Root>,
    pub commits: Vec<CommitOutcomeWire>,
    pub commit_entered: bool,
    pub commit_released: bool,
    pub read_entered: bool,
    pub read_released: bool,
}
pub struct Native {
    pub endpoint: SocketAddr,
    pub private: Root,
    pub server: Root,
    pub gate: Gate,
    pub observations: Mutex<Observations>,
    pub changed: Condvar,
}
impl Native {
    pub fn new(gate: Gate) -> Arc<Self> {
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
    pub fn call(
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
                    Operation::UpdatePreparedFilesystem { .. } | Operation::ConstructFile { .. }
                ),
                "unexpected construction route"
            );
            let first =
                matches!(request.operation, Operation::EditFile { .. }) && !observations.entered;
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
        if self.gate == Gate::ReadHold && matches!(request.operation, Operation::ReadFile { .. }) {
            let mut observed = self.observations.lock().unwrap();
            observed.read_entered = true;
            self.changed.notify_all();
            let (_state, timed) = self
                .changed
                .wait_timeout_while(observed, Duration::from_secs(5), |s| !s.read_released)
                .unwrap();
            assert!(!timed.timed_out());
        }
        let is_commit = matches!(
            request.operation,
            Operation::HistoryCommand(HistoryCommand::CommitStaged { .. })
        );
        let endpoint = if is_commit && self.gate == Gate::CommitUnknown {
            std::env::var("LAYERFS_COMMIT_ENDPOINT")
                .unwrap()
                .to_socket_addrs()
                .unwrap()
                .next()
                .unwrap()
        } else {
            self.endpoint
        };
        let (principal, private) = if is_commit && self.gate == Gate::CommitDenied {
            (
                2,
                pipe::key(&std::env::var("LAYERFS_COMMIT_PRIVATE_KEY").unwrap()).unwrap(),
            )
        } else {
            (1, self.private)
        };
        let mut client = Client::new(connect_until(
            endpoint,
            principal,
            &private,
            &self.server,
            end,
        )?)?;
        let response = client.call_until(request, input, output, end);
        if matches!(request.operation, Operation::EditFile { .. }) {
            if let Ok(Response::Saved { root, .. }) = &response {
                self.observations.lock().unwrap().saved_files.push(*root);
            }
        }
        if is_commit {
            if let Ok(Response::History(result)) = &response {
                if let HistoryResult::Committed(outcome) = result.as_ref() {
                    self.observations
                        .lock()
                        .unwrap()
                        .commits
                        .push(outcome.clone());
                    if self.gate == Gate::CommitCompletionFailure {
                        file_limit("2048");
                        println!("COMMIT_BACKING_LIMIT_APPLIED");
                    }
                    if self.gate == Gate::CommitReply {
                        let mut observations = self.observations.lock().unwrap();
                        observations.commit_entered = true;
                        self.changed.notify_all();
                        let (_state, timed) = self
                            .changed
                            .wait_timeout_while(observations, Duration::from_secs(5), |s| {
                                !s.commit_released
                            })
                            .unwrap();
                        assert!(
                            !timed.timed_out(),
                            "external commit reply gate was not released"
                        );
                    }
                }
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
    pub fn delivery(self: &Arc<Self>) -> OperationDelivery {
        let native = self.clone();
        Arc::new(move |r, i, o, d| native.call(r, i, o, d))
    }
    pub fn wait_entered(&self) {
        let state = self.observations.lock().unwrap();
        let (_state, timeout) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |s| !s.entered)
            .unwrap();
        assert!(!timeout.timed_out());
    }
    pub fn release(&self) {
        self.observations.lock().unwrap().released = true;
        self.changed.notify_all();
    }
    pub fn wait_read(&self) {
        let state = self.observations.lock().unwrap();
        let (_state, timed) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |s| !s.read_entered)
            .unwrap();
        assert!(!timed.timed_out());
    }
    pub fn release_read(&self) {
        self.observations.lock().unwrap().read_released = true;
        self.changed.notify_all();
    }
    pub fn wait_commit(&self) {
        let state = self.observations.lock().unwrap();
        let (_state, timed) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |s| !s.commit_entered)
            .unwrap();
        assert!(!timed.timed_out());
    }
    pub fn release_commit(&self) {
        self.observations.lock().unwrap().commit_released = true;
        self.changed.notify_all();
    }
    pub fn request(
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
    pub fn query(&self, query: HistoryQuery) -> Response {
        self.request(Operation::HistoryQuery(query), 16384, &mut std::io::sink())
            .unwrap()
    }
    pub fn attributes(&self, root: Root, path: &[u8]) -> Response {
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
    pub fn bytes(&self, root: Root, start: u64, length: usize) -> Vec<u8> {
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
pub struct Fixture {
    pub host: WorkspaceHost,
    pub workspace: Workspace,
    pub native: Arc<Native>,
}
impl Fixture {
    pub fn new(gate: Gate) -> Self {
        Self::with_quota(gate, 64 * 1024 * 1024)
    }
    pub fn with_quota(gate: Gate, quota: u64) -> Self {
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
    pub fn options(id: &str, incarnation: u8, access: WorkspaceAccess) -> AttachOptions {
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
    pub fn lookup(&self, name: &[u8]) -> NodeAttributes {
        self.workspace
            .lookup(
                self.workspace.root().serial,
                name,
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap()
    }
    pub fn own(&self, bytes: &[u8]) -> OwnedPayload {
        self.workspace
            .own_payload(bytes.len() as u64, &mut &bytes[..], deadline())
            .unwrap()
    }
    pub fn edit(&self, name: &[u8], start: u64, end: u64, bytes: &[u8]) -> MutationReceipt {
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
    pub fn read(&self, handle: HandleId, start: u64, length: usize) -> Vec<u8> {
        self.workspace
            .read(handle, start, length, deadline())
            .unwrap()
            .as_ref()
            .to_vec()
    }
    pub fn branch(&self) -> Response {
        self.native.query(HistoryQuery::GetBranch {
            branch: hex("LAYERFS_STAGE_BRANCH"),
        })
    }
    pub fn counts(&self, files: usize) {
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
            if let Operation::HistoryCommand(HistoryCommand::StageChanges(prepared)) = operation {
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
pub fn attr(response: Response) -> (u64, Root, u64, i64, u32) {
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
pub fn check(id: &str) {
    println!("STAGE_CHECK {id} PASS");
}
pub fn stage_retained(f: &Fixture, selector: StageSelector) {
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

pub fn file_limit(value: &str) {
    assert!(std::process::Command::new("prlimit")
        .arg("--pid")
        .arg(std::process::id().to_string())
        .arg(format!("--fsize={value}:"))
        .status()
        .unwrap()
        .success());
}
