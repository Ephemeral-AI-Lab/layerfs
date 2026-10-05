//! Typed bounded SQL jobs. No closure can hold the owner across network/Exec work.
use layerfs_overlay::{
    BaseSource, Capture, Cell, Dentry, Inode, Lease, NameWindow, Overlay, OverlayResult,
    Publication, Route, ScratchRecord, WorkspaceState, CELL_BYTES, MASK_BYTES, PAGE_ROWS,
    SCRATCH_BYTES,
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
    State,
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
    Opened(Route),
    State(WorkspaceState),
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
    Done,
}
impl Command {
    pub(crate) fn class(&self) -> ServiceClass {
        match self {
            Self::Open { .. }
            | Self::State
            | Self::RetainedCapture
            | Self::RetainedBaseSource { .. }
            | Self::ReleaseBaseSource(_)
            | Self::CleanupState
            | Self::Close
            | Self::ReleaseClosedCapture(_)
            | Self::ReplyAttempted(_)
            | Self::Acquire(_)
            | Self::Release(_) => ServiceClass::Lifecycle,
            Self::Inode(_) | Self::PendingPublications { .. } | Self::CapturedCell { .. } => {
                ServiceClass::Read
            }
            Self::SourceInode { .. }
            | Self::SourceDentry { .. }
            | Self::SourceNames { .. }
            | Self::SourceCell { .. } => ServiceClass::Read,
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
            Self::Publish { name, cell, .. } => (
                name.as_ref().map_or(0, |n| n.name.capacity())
                    + if cell.is_some() {
                        CELL_BYTES + MASK_BYTES
                    } else {
                        0
                    },
                128,
            ),
            Self::PutScratch { record, .. } => (record.value.capacity(), 0),
            Self::ScratchPage { .. } => (0, PAGE_ROWS * (SCRATCH_BYTES + 64)),
            Self::CapturedInodes { .. } => (0, PAGE_ROWS * std::mem::size_of::<Inode>()),
            Self::CapturedDentries { after, .. } => (
                after.as_ref().map_or(0, |(_, name)| name.capacity()),
                PAGE_ROWS * (std::mem::size_of::<Dentry>() + 255),
            ),
            Self::PendingPublications { .. } => (0, PAGE_ROWS * std::mem::size_of::<Publication>()),
            Self::CapturedCell { .. } | Self::SourceCell { .. } => {
                (0, CELL_BYTES + MASK_BYTES + std::mem::size_of::<Cell>())
            }
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
