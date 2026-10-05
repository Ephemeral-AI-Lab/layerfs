//! Typed bounded SQL jobs. No closure can hold the owner across network/Exec work.
use layerfs_overlay::{
    BaseSource, Capture, Cell, Dentry, Inode, Lease, Overlay, OverlayResult, Publication, Route,
    ScratchRecord, WorkspaceState, CELL_BYTES, MASK_BYTES, PAGE_ROWS, SCRATCH_BYTES,
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
    RetainedBaseSource(Option<BaseSource>),
    Scratch(Vec<ScratchRecord>),
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
            Self::SourceInode { .. } => ServiceClass::Read,
            Self::AcquireBaseSource { .. } => ServiceClass::Source,
            Self::Publish { .. } => ServiceClass::Mutation,
            Self::Capture
            | Self::Install { .. }
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
            Self::CapturedCell { .. } => (0, CELL_BYTES + MASK_BYTES + std::mem::size_of::<Cell>()),
            _ => (0, 256),
        };
        base.checked_add(input)?.checked_add(reply)
    }
    pub(crate) fn perform(self, db: &Overlay, route: Option<Route>) -> OverlayResult<Response> {
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
            Self::Open { .. } => unreachable!(),
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
