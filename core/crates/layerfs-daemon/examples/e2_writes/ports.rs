//! Observing decorators execute identical typed commands through real OwnerClient.
//! Recorder failure retains the original Completion/Pending; no command replay.
use super::{
    json::Json,
    records::{self, JobInfo},
    streams::Recorder,
};
use layerfs_daemon::{
    Command, Completion, OwnerClient, OwnerError, Pending, Response, ServiceClass,
};
use layerfs_overlay::{
    BaseSource, Cell, DirectoryEntry, DirectoryEntryWindow, Inode, LocalRead, Publication, Route,
};
use layerfs_workspace::{
    CanonicalClient, ClientWork, JobOutcome, NamespaceJob, OverlayJobs, OverlayRead,
    WorkspaceError, WorkspaceResult,
};
use std::{
    cell::{Cell as Flag, RefCell},
    error::Error,
    fmt, io,
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct ScopeRefusal(pub layerfs_daemon::upstream::OperationRefusal);
impl fmt::Debug for ScopeRefusal {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, out)
    }
}
impl fmt::Display for ScopeRefusal {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{self:?}")
    }
}
impl Error for ScopeRefusal {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0.error)
    }
}

pub enum OriginalFailure {
    Admission {
        cause: OwnerError,
        command: Box<Command>,
        recorder: Option<io::Error>,
    },
    Pending {
        cause: Option<OwnerError>,
        pending: Mutex<Pending>,
        recorder: Option<io::Error>,
    },
    Completion(Box<Completion>),
    Shape(Box<Completion>),
    Record {
        cause: io::Error,
        completion: Box<Completion>,
    },
}
impl fmt::Debug for OriginalFailure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admission {
                cause,
                command,
                recorder,
            } => out
                .debug_struct("OriginalUnattempted")
                .field("cause", cause)
                .field("command", command)
                .field("recorder", recorder)
                .finish(),
            Self::Pending {
                cause,
                pending,
                recorder,
            } => {
                let _retained = pending;
                out.debug_struct("OriginalPending")
                    .field("cause", cause)
                    .field("recorder", recorder)
                    .finish()
            }
            Self::Completion(value) | Self::Shape(value) => out
                .debug_tuple("OriginalCompletion")
                .field(value.result())
                .finish(),
            Self::Record { cause, completion } => out
                .debug_struct("OriginalRecorderFailure")
                .field("cause", cause)
                .field("completion", completion.result())
                .finish(),
        }
    }
}
impl fmt::Display for OriginalFailure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{self:?}")
    }
}
impl Error for OriginalFailure {}
fn service(error: OriginalFailure) -> WorkspaceError {
    WorkspaceError::Service(Box::new(error))
}

pub struct Observed<'a> {
    pub owner: OwnerClient,
    pub recorder: &'a RefCell<Recorder>,
    pub client: Option<&'a CanonicalClient>,
    pub phase: &'static str,
    pub operation: Option<u64>,
    base_interval: RefCell<Option<(ClientWork, usize, u64, BaseSource)>>,
    terminal: Flag<bool>,
}
impl<'a> Observed<'a> {
    pub fn new(
        owner: OwnerClient,
        recorder: &'a RefCell<Recorder>,
        client: Option<&'a CanonicalClient>,
        phase: &'static str,
        operation: Option<u64>,
    ) -> Self {
        Self {
            owner,
            recorder,
            client,
            phase,
            operation,
            base_interval: RefCell::new(None),
            terminal: Flag::new(false),
        }
    }
    fn info(
        &self,
        name: &'static str,
        class: ServiceClass,
        route: Option<Route>,
        source: Option<BaseSource>,
    ) -> JobInfo {
        JobInfo {
            record_id: self
                .recorder
                .borrow()
                .id("owner-job", self.recorder.borrow().jobs.records),
            external_job_id: self
                .recorder
                .borrow()
                .id("job", self.recorder.borrow().jobs.records),
            name,
            class,
            phase: self.phase,
            operation: self.operation,
            route,
            source,
            publication: None,
            opened: self.recorder.borrow().at(),
            span_scope:
                "external-command-admission-through-original-result-including-observer-wait",
        }
    }
    pub fn command(
        &self,
        route: Route,
        command: Command,
        name: &'static str,
        class: ServiceClass,
        source: Option<BaseSource>,
    ) -> WorkspaceResult<Completion> {
        if self.terminal.get() || !self.recorder.borrow().healthy() {
            return Err(io::Error::other("original observing port is terminal").into_workspace());
        }
        let index = self.recorder.borrow().jobs.records;
        // External index/identity and boundary exist before this single admission.
        let mut info = self.info(name, class, Some(route), source);
        if let Command::ReplyAttempted(value) = &command {
            info.publication = Some(*value);
        }
        let pending = match self.owner.try_submit(Some(route), command) {
            Ok(value) => value,
            Err((cause, command)) => {
                self.terminal.set(true);
                let message = format!("{cause:?}");
                let record = records::job(
                    &self.recorder.borrow(),
                    index,
                    &info,
                    None,
                    "unattempted",
                    Some(&message),
                );
                let recorder = self.append_job(record).err();
                return Err(service(OriginalFailure::Admission {
                    cause,
                    command: Box::new(command),
                    recorder,
                }));
            }
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        let completion = loop {
            match pending.try_complete() {
                Ok(Some(value)) => break value,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                outcome => {
                    self.terminal.set(true);
                    let cause = outcome.err();
                    let record=records::job(&self.recorder.borrow(),index,&info,None,"pending_failure",Some("original pending unavailable or external5s wait bound; no outcome inferred"));
                    let recorder = self.append_job(record).err();
                    return Err(service(OriginalFailure::Pending {
                        cause,
                        pending: Mutex::new(pending),
                        recorder,
                    }));
                }
            }
        };
        let record = records::job(
            &self.recorder.borrow(),
            index,
            &info,
            Some(&completion),
            "completed",
            None,
        );
        if let Err(cause) = self.append_job(record) {
            self.terminal.set(true);
            return Err(service(OriginalFailure::Record {
                cause,
                completion: Box::new(completion),
            }));
        }
        if completion.result().is_err() {
            self.terminal.set(true);
            return Err(service(OriginalFailure::Completion(Box::new(completion))));
        }
        Ok(completion)
    }
    fn shape(&self, done: Completion) -> WorkspaceError {
        self.terminal.set(true);
        service(OriginalFailure::Shape(Box::new(done)))
    }
    fn append_job(&self, record: io::Result<Vec<u8>>) -> io::Result<()> {
        self.recorder.borrow_mut().append("jobs", record?)
    }
    pub fn done(
        &self,
        route: Route,
        command: Command,
        name: &'static str,
        source: Option<BaseSource>,
    ) -> WorkspaceResult<()> {
        let done = self.command(route, command, name, ServiceClass::Lifecycle, source)?;
        if !matches!(done.result(), Ok(Response::Done)) {
            self.terminal.set(true);
            return Err(service(OriginalFailure::Shape(Box::new(done))));
        }
        drop(done);
        Ok(())
    }
    pub fn acquire(&self, route: Route, owner: u64) -> WorkspaceResult<BaseSource> {
        let done = self.command(
            route,
            Command::AcquireBaseSource { owner },
            "AcquireBaseSource",
            ServiceClass::Source,
            None,
        )?;
        let source = match done.result() {
            Ok(Response::BaseSource(value)) => *value,
            _ => {
                self.terminal.set(true);
                return Err(service(OriginalFailure::Shape(Box::new(done))));
            }
        };
        drop(done);
        Ok(source)
    }
    pub fn reply(&self, value: Publication) -> WorkspaceResult<()> {
        self.done(
            value.route(),
            Command::ReplyAttempted(value),
            "ReplyAttempted",
            None,
        )
    }
    pub fn release(&self, value: BaseSource) -> WorkspaceResult<()> {
        self.done(
            value.route(),
            Command::ReleaseBaseSource(value),
            "ReleaseBaseSource",
            Some(value),
        )
    }
    pub fn finish_base_interval(&self, status: &str) -> WorkspaceResult<()> {
        let Some((before, count, job_index, source)) = self.base_interval.borrow_mut().take()
        else {
            return Ok(());
        };
        let probe_index = self.recorder.borrow().probes.records;
        let probe_id = self.recorder.borrow().id("base-fact-interval", probe_index);
        let after = self
            .client
            .ok_or_else(|| io::Error::other("missing actual base client").into_workspace())?
            .diagnostics()?;
        let mut recorder = self.recorder.borrow_mut();
        let mut out = Json::new();
        recorder
            .header_with_id(&mut out, "base-fact-interval", probe_index, &probe_id)
            .map_err(io_workspace)?;
        out.raw(",\"operation_index\":").map_err(io_workspace)?;
        match self.operation {
            Some(value) => out.raw(&value.to_string()).map_err(io_workspace)?,
            None => out.raw("null").map_err(io_workspace)?,
        };
        out.raw(",").map_err(io_workspace)?;
        out.field("needs_job_index", job_index)
            .map_err(io_workspace)?;
        out.raw(",").map_err(io_workspace)?;
        out.text("needs_external_job_id", &recorder.id("job", job_index))
            .map_err(io_workspace)?;
        out.raw(",\"source\":").map_err(io_workspace)?;
        records::source(&mut out, source).map_err(io_workspace)?;
        out.raw(",").map_err(io_workspace)?;
        out.field("needs_count", count).map_err(io_workspace)?;
        out.raw(",").map_err(io_workspace)?;
        out.text("status", status).map_err(io_workspace)?;
        out.raw(",\"client_before\":").map_err(io_workspace)?;
        records::client(&mut out, before).map_err(io_workspace)?;
        out.raw(",\"client_after\":").map_err(io_workspace)?;
        records::client(&mut out, after).map_err(io_workspace)?;
        out.raw(",\"private_fact_provider_trace\":\"UNAVAILABLE\",\"scope\":\"actual-shared-client-counters-between-Needs-and-next-round-not-per-id-provider-trace\"}").map_err(io_workspace)?;
        recorder
            .append("probes", out.finish().map_err(io_workspace)?)
            .map_err(io_workspace)
    }
}
trait IoWorkspace {
    fn into_workspace(self) -> WorkspaceError;
}
impl IoWorkspace for io::Error {
    fn into_workspace(self) -> WorkspaceError {
        WorkspaceError::Service(Box::new(self))
    }
}
fn io_workspace(error: io::Error) -> WorkspaceError {
    error.into_workspace()
}

impl OverlayJobs for Observed<'_> {
    fn namespace(&self, job: &NamespaceJob) -> WorkspaceResult<JobOutcome> {
        self.finish_base_interval("SUPPLY_RETURNED")?;
        let job_index = self.recorder.borrow().jobs.records;
        let done = self.command(
            job.source().route(),
            Command::Namespace(Box::new(job.clone())),
            "Namespace",
            ServiceClass::Mutation,
            Some(job.source()),
        )?;
        let outcome = match done.result() {
            Ok(Response::Namespace(value)) => value.clone(),
            _ => {
                self.terminal.set(true);
                return Err(service(OriginalFailure::Shape(Box::new(done))));
            }
        };
        if let JobOutcome::Needs(needs) = &outcome {
            let before = self
                .client
                .ok_or_else(|| io::Error::other("missing actual source client").into_workspace())?
                .diagnostics()?;
            *self.base_interval.borrow_mut() = Some((before, needs.len(), job_index, job.source()));
        }
        drop(done);
        Ok(outcome)
    }
}
impl OverlayRead for Observed<'_> {
    fn inode(&self, source: BaseSource, serial: u64) -> WorkspaceResult<Option<Inode>> {
        let done = self.command(
            source.route(),
            Command::SourceInode { source, serial },
            "SourceInode",
            ServiceClass::Read,
            Some(source),
        )?;
        match done.result() {
            Ok(Response::Inode(value)) => Ok(value.clone()),
            _ => Err(self.shape(done)),
        }
    }
    fn directory_entry(
        &self,
        source: BaseSource,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<DirectoryEntry>> {
        let done = self.command(
            source.route(),
            Command::SourceDirectoryEntry {
                source,
                parent,
                name: name.to_vec(),
            },
            "SourceDirectoryEntry",
            ServiceClass::Read,
            Some(source),
        )?;
        match done.result() {
            Ok(Response::DirectoryEntry(value)) => Ok(value.clone()),
            _ => Err(self.shape(done)),
        }
    }
    fn directory_entries(
        &self,
        source: BaseSource,
        parent: u64,
        after: Option<&[u8]>,
    ) -> WorkspaceResult<DirectoryEntryWindow> {
        let done = self.command(
            source.route(),
            Command::SourceDirectoryEntries {
                source,
                parent,
                after: after.map(<[u8]>::to_vec),
            },
            "SourceDirectoryEntries",
            ServiceClass::Read,
            Some(source),
        )?;
        match done.result() {
            Ok(Response::DirectoryEntryWindow(value)) => Ok(value.clone()),
            _ => Err(self.shape(done)),
        }
    }
    fn cell(
        &self,
        source: BaseSource,
        serial: u64,
        generation: u64,
        offset: u64,
    ) -> WorkspaceResult<Option<Cell>> {
        let done = self.command(
            source.route(),
            Command::SourceCell {
                source,
                serial,
                generation,
                offset,
            },
            "SourceCell",
            ServiceClass::Read,
            Some(source),
        )?;
        match done.result() {
            Ok(Response::Cell(value)) => Ok(value.clone()),
            _ => Err(self.shape(done)),
        }
    }
    fn read(
        &self,
        source: BaseSource,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<Option<LocalRead>> {
        let done = self.command(
            source.route(),
            Command::SourceRead {
                source,
                serial,
                offset,
                length,
            },
            "SourceRead",
            ServiceClass::Read,
            Some(source),
        )?;
        match done.result() {
            Ok(Response::Read(value)) => Ok(value.clone()),
            _ => Err(self.shape(done)),
        }
    }
}
