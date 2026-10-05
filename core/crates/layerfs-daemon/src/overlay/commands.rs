//! Typed bounded SQL jobs. No closure can hold the owner across network/Exec work.
use layerfs_overlay::{
    BaseSource, Capture, CapturedReader, Cell, Dentry, FileRead, Inode, Lease, NameWindow,
    OpenFile, OperationOwner, Overlay, OverlayResult, Publication, Route, ScratchRecord,
    WorkspaceState, CELL_BYTES, MASK_BYTES, PAGE_ROWS, SCRATCH_BYTES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ServiceClass {
    Read,
    Mutation,
    Capture,
    Lifecycle,
    Scratch,
    Source,
}
/// One fixed SQL window, never a whole Exec or content-construction operation.
#[derive(Debug)]
pub enum Command {
    Open {
        incarnation: [u8; 32],
        base_root: [u8; 32],
    },
    OpenFile {
        source: BaseSource,
        request: u64,
        base: Inode,
        writable: bool,
    },
    RetainedFile {
        request: u64,
    },
    CloseFile(OpenFile),
    AcquireFileRead {
        source: BaseSource,
        file: OpenFile,
        request: u64,
    },
    RetainedFileRead {
        request: u64,
    },
    ReleaseFileRead(FileRead),
    FileRead {
        read: FileRead,
        offset: u64,
        length: u32,
    },
    AcquireCapturedReader {
        capture: Capture,
        request: u64,
    },
    RetainedCapturedReader {
        request: u64,
    },
    ReleaseCapturedReader(CapturedReader),
    CapturedRead {
        reader: CapturedReader,
        serial: u64,
        offset: u64,
        length: u32,
    },
    AcquireOperation {
        request: u64,
    },
    RetainedOperation {
        request: u64,
    },
    ReleaseOperation(OperationOwner),
    PutOwnedScratch {
        owner: OperationOwner,
        record: ScratchRecord,
    },
    OwnedScratchPage {
        owner: OperationOwner,
        kind: u32,
        after: Option<u64>,
    },
    State,
    /// Connection-scoped work snapshot for operator diagnostics; includes the
    /// route validation seek, without scanning namespace rows or payloads.
    DatabaseWork,
    /// Read-only pending maintenance observation; never drives cleanup.
    MaintenanceIdle,
    /// Exact plans of the payload statements under an owned source window.
    PayloadPlans(BaseSource),
    LifetimePlans,
    RetainedCapture,
    PendingPublications {
        after: u64,
    },
    AcquireBaseSource {
        owner: u64,
    },
    RetainedBaseSource {
        owner: u64,
    },
    ReleaseBaseSource(BaseSource),
    SourceInode {
        source: BaseSource,
        serial: u64,
    },
    SourceDentry {
        source: BaseSource,
        parent: u64,
        name: Vec<u8>,
    },
    SourceNames {
        source: BaseSource,
        parent: u64,
        after: Option<Vec<u8>>,
    },
    SourceCell {
        source: BaseSource,
        serial: u64,
        generation: u64,
        offset: u64,
    },
    /// Local layers of one read window; base bytes are fetched by the caller.
    SourceRead {
        source: BaseSource,
        serial: u64,
        offset: u64,
        length: u32,
    },
    /// One complete ordinary namespace operation round: Workspace evaluates it
    /// over the rows current in this job and publishes at most once.
    Namespace(Box<layerfs_workspace::NamespaceJob>),
    InstallPrepared {
        workspace: std::sync::Arc<layerfs_workspace::Workspace>,
        input: layerfs_workspace::PreparedBase,
    },
    CleanupState,
    Close,
    ReleaseClosedCapture(Capture),
    Inode(u64),
    Publish {
        inode: Inode,
        name: Option<Dentry>,
        cell: Option<Cell>,
    },
    ReplyAttempted(Publication),
    Capture,
    /// Caller has established definite nonpublication and fenced the exact
    /// capture's external work. Unknown history must retain that capture.
    ResolveFailed(Capture),
    Install {
        capture: Capture,
        root: [u8; 32],
    },
    CapturedInodes {
        capture: Capture,
        after: u64,
    },
    CapturedDentries {
        capture: Capture,
        after: Option<(u64, Vec<u8>)>,
    },
    CapturedCell {
        capture: Capture,
        serial: u64,
        offset: u64,
    },
    Acquire(Lease),
    Release(Lease),
    PutScratch {
        operation: u64,
        record: ScratchRecord,
    },
    ScratchPage {
        operation: u64,
        kind: u32,
        after: Option<u64>,
    },
}
#[derive(Debug)]
pub enum Response {
    Operation(Option<OperationOwner>),
    File(Option<OpenFile>),
    FileReader(Option<FileRead>),
    CapturedReader(Option<CapturedReader>),
    Opened(Route),
    State(WorkspaceState),
    DatabaseWork(Box<layerfs_overlay::DatabaseWork>),
    MaintenanceIdle(bool),
    PayloadPlans(Vec<String>),
    LifetimePlans(Vec<String>),
    CleanupState(layerfs_overlay::CleanupState),
    Inode(Option<Inode>),
    Published(Publication),
    Captured(Capture),
    RetainedCapture(Option<Capture>),
    Publications(Vec<Publication>),
    Inodes(Vec<Inode>),
    Dentries(Vec<Dentry>),
    Cell(Option<Cell>),
    BaseSource(BaseSource),
    Dentry(Option<Dentry>),
    Names(NameWindow),
    RetainedBaseSource(Option<BaseSource>),
    Scratch(Vec<ScratchRecord>),
    Namespace(layerfs_workspace::JobOutcome),
    Read(Option<layerfs_overlay::LocalRead>),
    Done,
}
impl Command {
    pub(crate) fn class(&self) -> ServiceClass {
        match self {
            Self::AcquireOperation { .. }
            | Self::RetainedOperation { .. }
            | Self::ReleaseOperation(_) => ServiceClass::Lifecycle,
            Self::PutOwnedScratch { .. } | Self::OwnedScratchPage { .. } => ServiceClass::Scratch,
            Self::FileRead { .. } | Self::CapturedRead { .. } => ServiceClass::Read,
            Self::OpenFile { .. }
            | Self::RetainedFile { .. }
            | Self::CloseFile(_)
            | Self::AcquireFileRead { .. }
            | Self::RetainedFileRead { .. }
            | Self::ReleaseFileRead(_)
            | Self::AcquireCapturedReader { .. }
            | Self::RetainedCapturedReader { .. }
            | Self::ReleaseCapturedReader(_) => ServiceClass::Lifecycle,
            Self::Open { .. }
            | Self::State
            | Self::DatabaseWork
            | Self::MaintenanceIdle
            | Self::RetainedCapture
            | Self::RetainedBaseSource { .. }
            | Self::ReleaseBaseSource(_)
            | Self::CleanupState
            | Self::Close
            | Self::ReleaseClosedCapture(_)
            | Self::ResolveFailed(_)
            | Self::ReplyAttempted(_)
            | Self::Acquire(_)
            | Self::Release(_) => ServiceClass::Lifecycle,
            Self::Inode(_)
            | Self::PendingPublications { .. }
            | Self::CapturedCell { .. }
            | Self::PayloadPlans(_)
            | Self::LifetimePlans => ServiceClass::Read,
            Self::SourceInode { .. }
            | Self::SourceDentry { .. }
            | Self::SourceNames { .. }
            | Self::SourceCell { .. }
            | Self::SourceRead { .. } => ServiceClass::Read,
            Self::AcquireBaseSource { .. } => ServiceClass::Source,
            Self::Publish { .. } | Self::Namespace(_) => ServiceClass::Mutation,
            Self::Capture
            | Self::Install { .. }
            | Self::InstallPrepared { .. }
            | Self::CapturedInodes { .. }
            | Self::CapturedDentries { .. } => ServiceClass::Capture,
            Self::PutScratch { .. } | Self::ScratchPage { .. } => ServiceClass::Scratch,
        }
    }
    pub(crate) fn charge(&self) -> Option<usize> {
        let base = std::mem::size_of::<Self>().checked_add(512)?;
        let (input, reply) = match self {
            Self::DatabaseWork => (0, std::mem::size_of::<layerfs_overlay::DatabaseWork>()),
            Self::PayloadPlans(_) | Self::LifetimePlans => (0, 8192),
            Self::Publish { name, cell, .. } => (
                name.as_ref().map_or(0, |n| n.name.capacity())
                    + if cell.is_some() {
                        CELL_BYTES + MASK_BYTES
                    } else {
                        0
                    },
                128,
            ),
            Self::PutScratch { record, .. } | Self::PutOwnedScratch { record, .. } => {
                (record.value.capacity(), 0)
            }
            Self::ScratchPage { .. } | Self::OwnedScratchPage { .. } => {
                (0, PAGE_ROWS * (SCRATCH_BYTES + 64))
            }
            Self::CapturedInodes { .. } => (0, PAGE_ROWS * std::mem::size_of::<Inode>()),
            Self::CapturedDentries { after, .. } => (
                after.as_ref().map_or(0, |(_, name)| name.capacity()),
                PAGE_ROWS * (std::mem::size_of::<Dentry>() + 255),
            ),
            Self::PendingPublications { .. } => (0, PAGE_ROWS * std::mem::size_of::<Publication>()),
            Self::CapturedCell { .. } | Self::SourceCell { .. } => {
                (0, CELL_BYTES + MASK_BYTES + std::mem::size_of::<Cell>())
            }
            // Decided bytes plus one inherited bit per byte of the window.
            Self::SourceRead { length, .. }
            | Self::FileRead { length, .. }
            | Self::CapturedRead { length, .. } => (
                0,
                *length as usize
                    + (*length as usize).div_ceil(8)
                    + std::mem::size_of::<layerfs_overlay::LocalRead>(),
            ),
            // The boxed job, its bounded facts/names, and at most one reply of
            // needed names or one changed inode.
            Self::Namespace(job) => (
                std::mem::size_of::<layerfs_workspace::NamespaceJob>() + job.charge(),
                PAGE_ROWS * (std::mem::size_of::<layerfs_workspace::Need>() + 255),
            ),
            Self::SourceDentry { name, .. } => {
                (name.capacity(), std::mem::size_of::<Dentry>() + 255)
            }
            Self::SourceNames { after, .. } => (
                after.as_ref().map_or(0, Vec::capacity),
                2 * PAGE_ROWS * (std::mem::size_of::<Dentry>() + 255)
                    + std::mem::size_of::<NameWindow>(),
            ),
            _ => (0, 256),
        };
        base.checked_add(input)?.checked_add(reply)
    }
    pub(crate) fn install_capture(&self) -> Option<Capture> {
        match self {
            Self::Install { capture, .. } => Some(*capture),
            Self::InstallPrepared { input, .. } => Some(input.capture()),
            _ => None,
        }
    }
    pub(crate) fn perform(
        self,
        db: &Overlay,
        route: Option<Route>,
    ) -> Result<Response, crate::OwnerError> {
        if let Self::Namespace(job) = self {
            if route != Some(job.source().route()) {
                return Err(crate::OwnerError::Overlay(
                    layerfs_overlay::OverlayError::Stale,
                ));
            }
            return job
                .perform(db)
                .map(Response::Namespace)
                .map_err(|cause| crate::OwnerError::Workspace(Box::new(cause)));
        }
        if let Self::InstallPrepared { workspace, input } = self {
            if route != Some(workspace.route()) {
                return Err(crate::OwnerError::Install {
                    cause: Box::new(layerfs_overlay::OverlayError::Stale.into()),
                    input: Box::new(input),
                });
            }
            return workspace
                .install_prepared_base(db, input)
                .map(|_| Response::Done)
                .map_err(|(cause, input)| crate::OwnerError::Install {
                    cause: Box::new(cause),
                    input: Box::new(input),
                });
        }
        self.perform_overlay(db, route)
            .map_err(crate::OwnerError::Overlay)
    }
    fn perform_overlay(self, db: &Overlay, route: Option<Route>) -> OverlayResult<Response> {
        if let Self::Open {
            incarnation,
            base_root,
        } = self
        {
            return db
                .open_workspace(incarnation, base_root)
                .map(Response::Opened);
        }
        let route = route.ok_or(layerfs_overlay::OverlayError::Invalid("missing route"))?;
        match self {
            Self::Open { .. } | Self::InstallPrepared { .. } | Self::Namespace(_) => unreachable!(),
            Self::AcquireOperation { request } => db
                .acquire_operation(route, request)
                .map(|o| Response::Operation(Some(o))),
            Self::RetainedOperation { request } => db
                .retained_operation(route, request)
                .map(Response::Operation),
            Self::ReleaseOperation(owner) => {
                if owner.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.release_operation(owner).map(|_| Response::Done)
            }
            Self::PutOwnedScratch { owner, record } => {
                if owner.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.put_owned_scratch(owner, &record).map(|_| Response::Done)
            }
            Self::OwnedScratchPage { owner, kind, after } => {
                if owner.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.owned_scratch_page(owner, kind, after)
                    .map(Response::Scratch)
            }
            Self::RetainedFile { request } => db.retained_file(route, request).map(Response::File),
            Self::RetainedFileRead { request } => db
                .retained_file_read(route, request)
                .map(Response::FileReader),
            Self::RetainedCapturedReader { request } => db
                .retained_captured_reader(route, request)
                .map(Response::CapturedReader),
            Self::OpenFile {
                source,
                request,
                base,
                writable,
            } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.open_file(source, request, &base, writable)
                    .map(|value| Response::File(Some(value)))
            }
            Self::CloseFile(file) => {
                if file.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.close_file(file).map(|_| Response::Done)
            }
            Self::AcquireFileRead {
                source,
                file,
                request,
            } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.acquire_file_read(source, file, request)
                    .map(|value| Response::FileReader(Some(value)))
            }
            Self::ReleaseFileRead(read) => {
                if read.source().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.release_file_read(read).map(|_| Response::Done)
            }
            Self::FileRead {
                read,
                offset,
                length,
            } => {
                if read.source().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.read_file(read, offset, length).map(Response::Read)
            }
            Self::AcquireCapturedReader { capture, request } => {
                if capture.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.acquire_captured_reader(capture, request)
                    .map(|value| Response::CapturedReader(Some(value)))
            }
            Self::ReleaseCapturedReader(reader) => {
                if reader.capture().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.release_captured_reader(reader).map(|_| Response::Done)
            }
            Self::CapturedRead {
                reader,
                serial,
                offset,
                length,
            } => {
                if reader.capture().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.read_captured(reader, serial, offset, length)
                    .map(Response::Read)
            }
            Self::DatabaseWork => {
                db.state(route)?;
                Ok(Response::DatabaseWork(Box::new(db.diagnostics())))
            }
            Self::LifetimePlans => db.explain_lifetimes(route).map(Response::LifetimePlans),
            Self::MaintenanceIdle => db.maintenance_idle(route).map(Response::MaintenanceIdle),
            Self::PayloadPlans(source) => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.explain_payload(source).map(Response::PayloadPlans)
            }
            Self::SourceRead {
                source,
                serial,
                offset,
                length,
            } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.source_read(source, serial, offset, length)
                    .map(Response::Read)
            }
            Self::SourceCell {
                source,
                serial,
                generation,
                offset,
            } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.source_cell(source, serial, generation, offset)
                    .map(Response::Cell)
            }
            Self::State => db.state(route).map(Response::State),
            Self::RetainedCapture => db.retained_capture(route).map(Response::RetainedCapture),
            Self::PendingPublications { after } => db
                .pending_publications(route, after)
                .map(Response::Publications),
            Self::AcquireBaseSource { owner } => db
                .acquire_base_source(route, owner)
                .map(Response::BaseSource),
            Self::RetainedBaseSource { owner } => db
                .retained_base_source(route, owner)
                .map(Response::RetainedBaseSource),
            Self::ReleaseBaseSource(source) => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.release_base_source(source).map(|_| Response::Done)
            }
            Self::SourceInode { source, serial } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.source_inode(source, serial).map(Response::Inode)
            }
            Self::SourceDentry {
                source,
                parent,
                name,
            } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.source_dentry(source, parent, &name)
                    .map(Response::Dentry)
            }
            Self::SourceNames {
                source,
                parent,
                after,
            } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.source_name_window(source, parent, after.as_deref())
                    .map(Response::Names)
            }
            Self::CleanupState => db.cleanup_state(route).map(Response::CleanupState),
            Self::Close => db.close(route).map(|_| Response::Done),
            Self::ReleaseClosedCapture(capture) => {
                if capture.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.release_closed_capture(capture).map(|_| Response::Done)
            }
            Self::Inode(serial) => db.inode(route, serial).map(Response::Inode),
            Self::Publish { inode, name, cell } => db
                .publish(route, &inode, name.as_ref(), cell.as_ref())
                .map(Response::Published),
            Self::ReplyAttempted(publication) => {
                if publication.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.reply_attempted(publication).map(|_| Response::Done)
            }
            Self::Capture => db.capture(route).map(Response::Captured),
            Self::ResolveFailed(capture) => {
                if capture.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.resolve_failed_capture(capture).map(|_| Response::Done)
            }
            Self::Install { capture, root } => {
                if capture.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.install(capture, root).map(|_| Response::Done)
            }
            Self::CapturedInodes { capture, after } => {
                if capture.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.captured_inodes(capture, after).map(Response::Inodes)
            }
            Self::CapturedDentries { capture, after } => {
                if capture.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.captured_dentries(
                    capture,
                    after
                        .as_ref()
                        .map(|(parent, name)| (*parent, name.as_slice())),
                )
                .map(Response::Dentries)
            }
            Self::CapturedCell {
                capture,
                serial,
                offset,
            } => {
                if capture.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.captured_cell(capture, serial, offset)
                    .map(Response::Cell)
            }
            Self::Acquire(lease) => db.acquire(route, lease).map(|_| Response::Done),
            Self::Release(lease) => db.release(route, lease).map(|_| Response::Done),
            Self::PutScratch { operation, record } => db
                .put_scratch(route, operation, &record)
                .map(|_| Response::Done),
            Self::ScratchPage {
                operation,
                kind,
                after,
            } => db
                .scratch_page(route, operation, kind, after)
                .map(Response::Scratch),
        }
    }
}
