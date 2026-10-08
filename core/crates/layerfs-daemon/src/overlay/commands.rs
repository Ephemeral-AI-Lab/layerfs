//! Typed bounded SQL jobs. No closure can hold the owner across network/Exec work.
use layerfs_overlay::{
    BaseSource, Capture, CapturedReader, Cell, DirectoryEntry, DirectoryEntryWindow, FileRead,
    Inode, Lease, LookupOwner, OpenFile, OperationOwner, OperationRecord, Overlay, OverlayResult,
    Publication, Route, WorkspaceState, CELL_BYTES, MASK_BYTES, OPERATION_RECORD_BYTES, PAGE_ROWS,
};

/// Declared reply bytes of a command without a larger owned reply window.
const DEFAULT_REPLY: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ServiceClass {
    Read,
    Mutation,
    Capture,
    Lifecycle,
    OperationRecord,
    Source,
}
/// One fixed SQL window, never a whole Exec or content-construction operation.
#[derive(Debug)]
pub enum Command {
    Native(crate::NativeJob),
    IndexedOperationRecord(Box<crate::IndexedOperationRecordJob>),
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
    CapturedRun(Box<layerfs_overlay::CapturedRunCursor>),
    AcquireOperation {
        request: u64,
    },
    RetainedOperation {
        request: u64,
    },
    ReleaseOperation(OperationOwner),
    PutOwnedOperationRecord {
        owner: OperationOwner,
        record: OperationRecord,
    },
    OwnedOperationRecordPage {
        owner: OperationOwner,
        kind: u32,
        after: Option<u64>,
    },
    AcquireLookup {
        source: BaseSource,
        request: u64,
        base: Inode,
        root: u64,
    },
    RetainedLookup {
        request: u64,
    },
    ReleaseLookup(LookupOwner),
    AcquireLookupRead {
        source: BaseSource,
        lookup: LookupOwner,
        request: u64,
    },
    ReaderInodes {
        reader: CapturedReader,
        after: u64,
    },
    ReaderInode {
        reader: CapturedReader,
        serial: u64,
    },
    ReaderDirectoryEntries {
        reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>,
    },
    /// One parent's sealed names after a binary name, whiteouts included.
    ReaderParentDirectoryEntries {
        reader: CapturedReader,
        parent: u64,
        after: Option<Vec<u8>>,
    },
    /// The exact sealed row of one name, never a lower or active row.
    ReaderDirectoryEntry {
        reader: CapturedReader,
        parent: u64,
        name: Vec<u8>,
    },
    /// The local target of a live symlink in the reader's sealed range.
    ReaderSymlink {
        reader: CapturedReader,
        serial: u64,
    },
    Resources {
        global: bool,
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
    SourceDirectoryEntry {
        source: BaseSource,
        parent: u64,
        name: Vec<u8>,
    },
    SourceDirectoryEntries {
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
    /// Explicit read-only terminal observation; carries no mutable Route.
    ObserveCleanup {
        namespace: i64,
        incarnation: [u8; 32],
    },
    Close,
    ReleaseClosedCapture(Capture),
    Inode(u64),
    Publish {
        inode: Inode,
        name: Option<DirectoryEntry>,
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
    CapturedDirectoryEntries {
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
    PutOperationRecord {
        operation: u64,
        record: OperationRecord,
    },
    OperationRecordPage {
        operation: u64,
        kind: u32,
        after: Option<u64>,
    },
}
#[derive(Debug)]
pub enum Response {
    Native(crate::NativeReply),
    CapturedRun(Box<layerfs_overlay::CapturedRunReply>),
    IndexedOperationRecord(crate::IndexedOperationRecordReply),
    Lookup(Option<LookupOwner>),
    Operation(Option<OperationOwner>),
    File(Option<OpenFile>),
    FileReader(Option<FileRead>),
    CapturedReader(Option<CapturedReader>),
    Resources(Box<layerfs_overlay::Resources>),
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
    DirectoryEntries(Vec<DirectoryEntry>),
    Cell(Option<Cell>),
    BaseSource(BaseSource),
    DirectoryEntry(Option<DirectoryEntry>),
    DirectoryEntryWindow(DirectoryEntryWindow),
    RetainedBaseSource(Option<BaseSource>),
    OperationRecord(Vec<OperationRecord>),
    Namespace(layerfs_workspace::JobOutcome),
    Read(Option<layerfs_overlay::LocalRead>),
    Symlink(Vec<u8>),
    Done,
}
impl Command {
    pub(crate) fn class(&self) -> ServiceClass {
        match self {
            Self::Native(job) => job.class(),
            Self::IndexedOperationRecord(_) => ServiceClass::OperationRecord,
            Self::AcquireLookup { .. }
            | Self::RetainedLookup { .. }
            | Self::ReleaseLookup(_)
            | Self::AcquireLookupRead { .. } => ServiceClass::Lifecycle,
            Self::AcquireOperation { .. }
            | Self::RetainedOperation { .. }
            | Self::ReleaseOperation(_) => ServiceClass::Lifecycle,
            Self::PutOwnedOperationRecord { .. } | Self::OwnedOperationRecordPage { .. } => {
                ServiceClass::OperationRecord
            }
            Self::FileRead { .. }
            | Self::CapturedRead { .. }
            | Self::CapturedRun(_)
            | Self::ReaderInodes { .. }
            | Self::ReaderInode { .. }
            | Self::ReaderDirectoryEntries { .. }
            | Self::ReaderParentDirectoryEntries { .. }
            | Self::ReaderDirectoryEntry { .. }
            | Self::ReaderSymlink { .. } => ServiceClass::Read,
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
            | Self::ObserveCleanup { .. }
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
            | Self::LifetimePlans
            | Self::Resources { .. } => ServiceClass::Read,
            Self::SourceInode { .. }
            | Self::SourceDirectoryEntry { .. }
            | Self::SourceDirectoryEntries { .. }
            | Self::SourceCell { .. }
            | Self::SourceRead { .. } => ServiceClass::Read,
            Self::AcquireBaseSource { .. } => ServiceClass::Source,
            Self::Publish { .. } | Self::Namespace(_) => ServiceClass::Mutation,
            Self::Capture
            | Self::Install { .. }
            | Self::InstallPrepared { .. }
            | Self::CapturedInodes { .. }
            | Self::CapturedDirectoryEntries { .. } => ServiceClass::Capture,
            Self::PutOperationRecord { .. } | Self::OperationRecordPage { .. } => {
                ServiceClass::OperationRecord
            }
        }
    }
    /// Per-slot charge of a Lifecycle job with no owned input and the default
    /// reply. Startup reserves this for every configured lifecycle slot.
    pub(crate) fn lifecycle_charge() -> Option<usize> {
        crate::service::completion::charge(true, 0, DEFAULT_REPLY)
    }
    pub(crate) fn charge(&self) -> Option<usize> {
        let (input, reply) = match self {
            Self::Native(job) => job.charge(),
            Self::IndexedOperationRecord(job) => (job.charge()?, 0),
            Self::Resources { .. } => (0, std::mem::size_of::<layerfs_overlay::Resources>()),
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
            Self::PutOperationRecord { record, .. }
            | Self::PutOwnedOperationRecord { record, .. } => (record.value.capacity(), 0),
            Self::OperationRecordPage { .. } | Self::OwnedOperationRecordPage { .. } => {
                (0, PAGE_ROWS * (OPERATION_RECORD_BYTES + 64))
            }
            Self::CapturedInodes { .. } | Self::ReaderInodes { .. } => {
                (0, PAGE_ROWS * std::mem::size_of::<Inode>())
            }
            Self::CapturedDirectoryEntries { after, .. }
            | Self::ReaderDirectoryEntries { after, .. } => (
                after.as_ref().map_or(0, |(_, name)| name.capacity()),
                PAGE_ROWS * (std::mem::size_of::<DirectoryEntry>() + 255),
            ),
            Self::PendingPublications { .. } => (0, PAGE_ROWS * std::mem::size_of::<Publication>()),
            Self::CapturedCell { .. } | Self::SourceCell { .. } => {
                (0, CELL_BYTES + MASK_BYTES + std::mem::size_of::<Cell>())
            }
            Self::CapturedRun(_) => (
                std::mem::size_of::<layerfs_overlay::CapturedRunCursor>(),
                std::mem::size_of::<layerfs_overlay::CapturedRunReply>()
                    + std::mem::size_of::<layerfs_overlay::LocalRead>()
                    + CELL_BYTES
                    + MASK_BYTES,
            ),
            Self::ReaderInode { .. } => (0, std::mem::size_of::<Inode>()),
            Self::ReaderParentDirectoryEntries { after, .. } => (
                after.as_ref().map_or(0, Vec::capacity),
                PAGE_ROWS * (std::mem::size_of::<DirectoryEntry>() + 255),
            ),
            // The job composes one captured cell window like any captured read
            // and replies with at most that cell of locally decided bytes.
            Self::ReaderSymlink { .. } => (
                0,
                CELL_BYTES
                    + CELL_BYTES.div_ceil(8)
                    + std::mem::size_of::<layerfs_overlay::LocalRead>(),
            ),
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
            Self::SourceDirectoryEntry { name, .. } | Self::ReaderDirectoryEntry { name, .. } => {
                (name.capacity(), std::mem::size_of::<DirectoryEntry>() + 255)
            }
            Self::SourceDirectoryEntries { after, .. } => (
                after.as_ref().map_or(0, Vec::capacity),
                2 * PAGE_ROWS * (std::mem::size_of::<DirectoryEntry>() + 255)
                    + std::mem::size_of::<DirectoryEntryWindow>(),
            ),
            _ => (0, DEFAULT_REPLY),
        };
        crate::service::completion::charge(self.class() == ServiceClass::Lifecycle, input, reply)
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
        if route.is_none() && matches!(self, Self::Resources { global: true }) {
            return db
                .resources(None)
                .map(|resources| Response::Resources(Box::new(resources)));
        }
        if let Self::ObserveCleanup {
            namespace,
            incarnation,
        } = self
        {
            return db
                .observe_cleanup(namespace, incarnation)
                .map(Response::CleanupState);
        }
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
            Self::Native(job) => job.perform(db, route).map(Response::Native),
            Self::Open { .. }
            | Self::InstallPrepared { .. }
            | Self::Namespace(_)
            | Self::ObserveCleanup { .. } => {
                unreachable!()
            }
            Self::IndexedOperationRecord(job) => {
                job.perform(db, route).map(Response::IndexedOperationRecord)
            }
            Self::Resources { global } => {
                db.state(route)?;
                db.resources(if global { None } else { Some(route) })
                    .map(|r| Response::Resources(Box::new(r)))
            }
            Self::ReaderInodes { reader, after } => {
                if reader.capture().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.reader_inodes(reader, after).map(Response::Inodes)
            }
            Self::ReaderDirectoryEntries { reader, after } => {
                if reader.capture().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.reader_directory_entries(reader, after.as_ref().map(|(p, n)| (*p, n.as_slice())))
                    .map(Response::DirectoryEntries)
            }
            Self::ReaderParentDirectoryEntries {
                reader,
                parent,
                after,
            } => {
                if reader.capture().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.reader_parent_directory_entries(reader, parent, after.as_deref())
                    .map(Response::DirectoryEntries)
            }
            Self::ReaderDirectoryEntry {
                reader,
                parent,
                name,
            } => {
                if reader.capture().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.reader_directory_entry(reader, parent, &name)
                    .map(Response::DirectoryEntry)
            }
            Self::ReaderSymlink { reader, serial } => {
                if reader.capture().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.reader_symlink(reader, serial).map(Response::Symlink)
            }
            Self::AcquireLookup {
                source,
                request,
                base,
                root,
            } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.acquire_lookup(source, request, &base, root)
                    .map(|l| Response::Lookup(Some(l)))
            }
            Self::RetainedLookup { request } => {
                db.retained_lookup(route, request).map(Response::Lookup)
            }
            Self::ReleaseLookup(lookup) => {
                if lookup.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.release_lookup(lookup).map(|_| Response::Done)
            }
            Self::AcquireLookupRead {
                source,
                lookup,
                request,
            } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.acquire_lookup_read(source, lookup, request)
                    .map(|r| Response::FileReader(Some(r)))
            }
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
            Self::PutOwnedOperationRecord { owner, record } => {
                if owner.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.put_owned_operation_record(owner, &record)
                    .map(|_| Response::Done)
            }
            Self::OwnedOperationRecordPage { owner, kind, after } => {
                if owner.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.owned_operation_record_page(owner, kind, after)
                    .map(Response::OperationRecord)
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
            Self::CapturedRun(cursor) => {
                if cursor.reader().capture().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.captured_run_step(*cursor)
                    .map(|value| Response::CapturedRun(Box::new(value)))
            }
            Self::ReaderInode { reader, serial } => {
                if reader.capture().route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.reader_inode(reader, serial).map(Response::Inode)
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
            Self::SourceDirectoryEntry {
                source,
                parent,
                name,
            } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.source_directory_entry(source, parent, &name)
                    .map(Response::DirectoryEntry)
            }
            Self::SourceDirectoryEntries {
                source,
                parent,
                after,
            } => {
                if source.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.source_directory_entry_window(source, parent, after.as_deref())
                    .map(Response::DirectoryEntryWindow)
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
            Self::CapturedDirectoryEntries { capture, after } => {
                if capture.route() != route {
                    return Err(layerfs_overlay::OverlayError::Stale);
                }
                db.captured_directory_entries(
                    capture,
                    after
                        .as_ref()
                        .map(|(parent, name)| (*parent, name.as_slice())),
                )
                .map(Response::DirectoryEntries)
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
            Self::PutOperationRecord { operation, record } => db
                .put_operation_record(route, operation, &record)
                .map(|_| Response::Done),
            Self::OperationRecordPage {
                operation,
                kind,
                after,
            } => db
                .operation_record_page(route, operation, kind, after)
                .map(Response::OperationRecord),
        }
    }
}
