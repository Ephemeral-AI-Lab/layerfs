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
    sync::{atomic::AtomicBool, Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

pub fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
/// A baseline task can finish between procfs enumeration and this read.
/// Only ENOENT means vanished; live fault callers must require Some.
#[cfg(target_os = "linux")]
pub fn seccomp_filter_count(tid: i32) -> Option<u32> {
    // Linux errno ABI; keep this shared external helper independent of nix/libc.
    const LINUX_ENOENT: i32 = 2;
    let status = match std::fs::read_to_string(format!("/proc/self/task/{tid}/status")) {
        Ok(status) => status,
        Err(error) if error.raw_os_error() == Some(LINUX_ENOENT) => return None,
        Err(error) => panic!("cannot read task {tid} seccomp status: {error}"),
    };
    Some(
        status
            .lines()
            .find_map(|line| line.strip_prefix("Seccomp_filters:"))
            .expect("task status contains Seccomp_filters")
            .trim()
            .parse()
            .expect("Seccomp_filters is an integer"),
    )
}
pub fn diagnostic_trace() -> bool {
    std::env::var_os("LAYERFS_NATIVE_DIAGNOSTIC_TRACE").is_some_and(|v| v == "1")
}
#[derive(Default, Debug)]
struct SourceObservations {
    calls: u64,
    bytes: u64,
    eof: bool,
    error: Option<String>,
    max_read_us: u128,
}
struct TracedSource<'a> {
    inner: &'a mut dyn Source,
    observed: SourceObservations,
}
impl Source for TracedSource<'_> {
    fn read(
        &mut self,
        buffer: &mut [u8],
        end: Instant,
        cancel: &AtomicBool,
    ) -> std::io::Result<usize> {
        let started = Instant::now();
        let result = self.inner.read(buffer, end, cancel);
        self.observed.calls += 1;
        self.observed.max_read_us = self.observed.max_read_us.max(started.elapsed().as_micros());
        match &result {
            Ok(n) => {
                self.observed.bytes += *n as u64;
                self.observed.eof |= *n == 0 && !buffer.is_empty();
            }
            Err(error) => self.observed.error = Some(format!("{:?}: {error}", error.kind())),
        }
        result
    }
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
    CompositeBefore,
    CompositeCompletionFailure,
    AttributesAfterCommit,
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
    pub attributes_entered: bool,
    pub attributes_released: bool,
}
pub struct Native {
    pub endpoint: SocketAddr,
    pub private: Root,
    pub server: Root,
    pub gate: Gate,
    allow_fresh_files: bool,
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
            allow_fresh_files: false,
            observations: Mutex::new(Observations::default()),
            changed: Condvar::new(),
        })
    }
    pub fn new_fresh(gate: Gate) -> Arc<Self> {
        let mut native = Self::new(gate);
        Arc::get_mut(&mut native).unwrap().allow_fresh_files = true;
        native
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
                self.allow_fresh_files
                    || !matches!(
                        request.operation,
                        Operation::SaveFile { base: None, .. } | Operation::ConstructSymlink { .. }
                    ),
                "unexpected construction route"
            );
            let first = matches!(
                request.operation,
                Operation::SaveFile { .. } | Operation::ConstructSymlink { .. }
            ) && !observations.entered;
            observations.operations.push(request.operation.clone());
            if first {
                observations.entered = true;
                self.changed.notify_all();
            }
            first
        };
        if first_save && matches!(self.gate, Gate::Delivery | Gate::CompositeCompletionFailure) {
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
        if self.gate == Gate::AttributesAfterCommit
            && matches!(&request.operation, Operation::Inspect { query: Inspect::Attributes { path }, .. } if path == b"data.bin")
        {
            let mut observed = self.observations.lock().unwrap();
            if !observed.commits.is_empty() {
                observed.attributes_entered = true;
                self.changed.notify_all();
                let (_state, timed) = self
                    .changed
                    .wait_timeout_while(observed, Duration::from_secs(5), |s| {
                        !s.attributes_released
                    })
                    .unwrap();
                assert!(
                    !timed.timed_out(),
                    "external attribute gate was not released"
                );
            }
        }
        let is_commit = matches!(
            request.operation,
            Operation::HistoryCommand(
                HistoryCommand::CommitStaged { .. } | HistoryCommand::Commit(_)
            )
        );
        if self.gate == Gate::CompositeBefore
            && matches!(&request.operation, Operation::HistoryCommand(HistoryCommand::Commit(changes)) if changes.workspace == [31; 32])
        {
            let mut observed = self.observations.lock().unwrap();
            observed.commit_entered = true;
            self.changed.notify_all();
            let (_state, timed) = self
                .changed
                .wait_timeout_while(observed, Duration::from_secs(5), |s| !s.commit_released)
                .unwrap();
            assert!(
                !timed.timed_out(),
                "external composite gate was not released"
            );
        }
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
        let (client, response) = if diagnostic_trace() {
            self.traced_call((endpoint, principal, &private), request, input, output, end)?
        } else {
            let mut client = Client::new(connect_until(
                endpoint,
                principal,
                &private,
                &self.server,
                end,
            )?)?;
            let response = client.call_until(request, input, output, end);
            (client, response)
        };
        if matches!(
            request.operation,
            Operation::SaveFile { .. } | Operation::ConstructSymlink { .. }
        ) {
            if let Ok(Response::Saved { root, .. }) = &response {
                self.observations.lock().unwrap().saved_files.push(*root);
            }
        }
        if is_commit {
            if let Ok(Response::History(result)) = &response {
                if let HistoryResult::Committed(outcome) = result.as_ref() {
                    let mut observations = self.observations.lock().unwrap();
                    observations.commits.push(outcome.clone());
                    if matches!(
                        self.gate,
                        Gate::CommitCompletionFailure | Gate::CompositeCompletionFailure
                    ) && observations.commits.len() == 1
                    {
                        file_limit("2048");
                        println!("COMMIT_BACKING_LIMIT_APPLIED");
                    }
                    drop(observations);
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
        drop(client);
        response
    }
    fn traced_call(
        &self,
        authority: (SocketAddr, u32, &Root),
        request: &Request,
        input: &mut dyn Source,
        output: &mut dyn Write,
        end: Instant,
    ) -> Result<(Client, Result<Response, Failure>), Failure> {
        let (endpoint, principal, private) = authority;
        let operation = match &request.operation {
            Operation::SaveFile { .. } => "SaveFile",
            Operation::ConstructSymlink { .. } => "ConstructSymlink",
            Operation::ConstructPortableMetadata { .. } => "ConstructPortableMetadata",
            Operation::UpdatePortableMetadata { .. } => "UpdatePortableMetadata",
            Operation::HistoryCommand(HistoryCommand::ReserveInodes { .. }) => "ReserveInodes",
            Operation::HistoryCommand(HistoryCommand::Commit(_)) => "Commit",
            Operation::HistoryQuery(_) => "HistoryQuery",
            Operation::Inspect { .. } => "Inspect",
            Operation::ReadFile { .. } => "ReadFile",
            _ => "Other",
        };
        let started = Instant::now();
        let log = |phase: &str, phase_start: Instant, result: Option<&Failure>| {
            eprintln!("NATIVE_DIAGNOSTIC id={} operation={operation} phase={phase} phase_us={} elapsed_us={} remaining_us={} failure={result:?}",
                request.id, phase_start.elapsed().as_micros(), started.elapsed().as_micros(),
                end.saturating_duration_since(Instant::now()).as_micros());
        };
        eprintln!(
            "NATIVE_DIAGNOSTIC id={} operation={operation} request_ms={} input_bytes={:?}",
            request.id,
            request.deadline_ms,
            request.operation.input_length()
        );
        log("connect-start", started, None);
        let connection = connect_until(endpoint, principal, private, &self.server, end);
        log("connect-end", started, connection.as_ref().err());
        let connection = connection?;
        let hello_start = Instant::now();
        log("hello-start", hello_start, None);
        let client = Client::new(connection);
        log("hello-end", hello_start, client.as_ref().err());
        let mut client = client?;
        let mut source = TracedSource {
            inner: input,
            observed: SourceObservations::default(),
        };
        let call_start = Instant::now();
        log("call-start", call_start, None);
        let response = client.call_until(request, &mut source, output, end);
        log("call-end", call_start, response.as_ref().err());
        eprintln!(
            "NATIVE_DIAGNOSTIC id={} source={:?}",
            request.id, source.observed
        );
        Ok((client, response))
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
    pub fn wait_attributes(&self) {
        let observed = self.observations.lock().unwrap();
        let (_state, timed) = self
            .changed
            .wait_timeout_while(observed, Duration::from_secs(5), |s| !s.attributes_entered)
            .unwrap();
        assert!(!timed.timed_out());
    }
    pub fn release_attributes(&self) {
        self.observations.lock().unwrap().attributes_released = true;
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
        let file = self.lookup(name);
        assert!(start <= end && end <= file.size);
        let handle = self
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        let removed = end - start;
        let inserted = bytes.len() as u64;
        let next = file.size - removed + inserted;
        const CHUNK: u64 = 64 * 1024;
        if inserted > removed {
            let shift = inserted - removed;
            self.workspace
                .set_len(file.serial, next, deadline())
                .unwrap();
            let mut right = file.size;
            while right > end {
                let left = end.max(right.saturating_sub(CHUNK));
                let chunk = self.read(handle, left, (right - left) as usize);
                self.workspace
                    .write_file(handle, left + shift, &self.own(&chunk), deadline())
                    .unwrap();
                right = left;
            }
        } else if inserted < removed {
            let shift = removed - inserted;
            let mut left = end;
            while left < file.size {
                let right = file.size.min(left + CHUNK);
                let chunk = self.read(handle, left, (right - left) as usize);
                self.workspace
                    .write_file(handle, left - shift, &self.own(&chunk), deadline())
                    .unwrap();
                left = right;
            }
            self.workspace
                .set_len(file.serial, next, deadline())
                .unwrap();
        }
        let mut receipt = None;
        for (index, chunk) in bytes.chunks(CHUNK as usize).enumerate() {
            receipt = Some(
                self.workspace
                    .write_file(
                        handle,
                        start + index as u64 * CHUNK,
                        &self.own(chunk),
                        deadline(),
                    )
                    .unwrap(),
            );
        }
        let receipt = receipt.unwrap_or_else(|| {
            self.workspace
                .write_file(handle, start, &self.own(b""), deadline())
                .unwrap()
        });
        self.workspace.release(handle).unwrap();
        receipt
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
                .filter(|op| matches!(op, Operation::SaveFile { base: Some(_), .. }))
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
