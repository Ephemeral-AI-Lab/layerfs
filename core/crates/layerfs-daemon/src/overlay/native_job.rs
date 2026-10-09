//! Native custody jobs on the existing fair owner, independent of fuser types.
use crate::{NativeDirectoryJob, NativeDirectoryReply, ServiceClass};
use layerfs_overlay::{
    NativeMount, NativeMountState, OpenFile, Overlay, OverlayError, OverlayResult, Route,
};
use layerfs_workspace::{
    NativeDataVisit, NativeMutationOutcome, NativeMutationVisit, NativeReadOutcome,
    NativeReadVisit, NativeWindow,
};
use std::sync::Arc;

#[derive(Debug)]
pub enum NativeJob {
    Mount {
        root: u64,
    },
    RetainedMount,
    RetainedFile {
        mount: NativeMount,
        request: u64,
    },
    File {
        mount: NativeMount,
        serial: u64,
        handle: u64,
    },
    CloseFile {
        mount: NativeMount,
        serial: u64,
        handle: u64,
    },
    /// LOOKUP or GETATTR decided in this one job, with no request source.
    ObserveVisit(Box<NativeReadVisit>),
    /// READ or READLINK: the window's local part, read in this one job.
    ReadVisit(Box<NativeDataVisit>),
    /// A native mutation decided and published in this one job.
    MutateVisit(Box<NativeMutationVisit>),
    Directory(Box<NativeDirectoryJob>),
    Forget {
        mount: NativeMount,
        serial: u64,
        count: u64,
    },
    State(NativeMount),
    Revoke(NativeMount),
}
#[derive(Debug)]
pub enum NativeReply {
    Mount(NativeMount),
    RetainedMount(Option<NativeMount>),
    RetainedFile(Option<OpenFile>),
    File(OpenFile),
    Observed(Arc<NativeReadOutcome>),
    Window(Arc<NativeWindow>),
    Mutated(Arc<NativeMutationOutcome>),
    Directory(NativeDirectoryReply),
    State(NativeMountState),
    Done,
}
impl NativeJob {
    pub(crate) fn class(&self) -> ServiceClass {
        match self {
            Self::ObserveVisit(_) | Self::ReadVisit(_) => ServiceClass::Read,
            Self::MutateVisit(_) => ServiceClass::Mutation,
            Self::Directory(job) => job.class(),
            _ => ServiceClass::Lifecycle,
        }
    }
    pub(crate) fn charge(&self) -> (usize, usize) {
        match self {
            Self::ObserveVisit(job) => (
                std::mem::size_of::<NativeReadVisit>() + job.charge(),
                std::mem::size_of::<NativeReadOutcome>()
                    + 4 * (std::mem::size_of::<layerfs_workspace::Need>() + 255)
                    + 2 * std::mem::size_of::<usize>(),
            ),
            // Decided bytes plus one inherited bit per byte of the window.
            Self::ReadVisit(job) => (
                std::mem::size_of::<NativeDataVisit>(),
                std::mem::size_of::<NativeWindow>() + job.charge(),
            ),
            // The boxed job with its bounded facts, names and write window,
            // and at most one reply of needed names or one changed inode.
            Self::MutateVisit(job) => (
                std::mem::size_of::<NativeMutationVisit>() + job.charge(),
                std::mem::size_of::<NativeMutationOutcome>()
                    + layerfs_overlay::PAGE_ROWS
                        * (std::mem::size_of::<layerfs_workspace::Need>() + 255),
            ),
            Self::Directory(job) => job.charge(),
            _ => (0, 256),
        }
    }
    pub(crate) fn perform(self, db: &Overlay, route: Route) -> OverlayResult<NativeReply> {
        let expected = match &self {
            Self::RetainedFile { mount, .. }
            | Self::File { mount, .. }
            | Self::CloseFile { mount, .. }
            | Self::Forget { mount, .. }
            | Self::State(mount)
            | Self::Revoke(mount) => Some(mount.route()),
            Self::ObserveVisit(job) => Some(job.mount().route()),
            Self::ReadVisit(job) => Some(job.mount().route()),
            Self::MutateVisit(job) => Some(job.mount().route()),
            Self::Directory(job) => Some(job.route()),
            Self::Mount { .. } | Self::RetainedMount => None,
        };
        if expected.is_some_and(|expected| expected != route) {
            return Err(OverlayError::Stale);
        }
        match self {
            Self::Mount { root } => db.create_native_mount(route, root).map(NativeReply::Mount),
            Self::RetainedMount => db
                .retained_native_mount(route)
                .map(NativeReply::RetainedMount),
            Self::RetainedFile { mount, request } => db
                .retained_native_file(mount, request)
                .map(NativeReply::RetainedFile),
            Self::File {
                mount,
                serial,
                handle,
            } => db.native_file(mount, serial, handle).map(NativeReply::File),
            Self::CloseFile {
                mount,
                serial,
                handle,
            } => db
                .close_native_file(mount, serial, handle)
                .map(|()| NativeReply::Done),
            Self::ObserveVisit(job) => Ok(NativeReply::Observed(Arc::new(job.perform(db)))),
            Self::ReadVisit(job) => job
                .perform(db)
                .map(|window| NativeReply::Window(Arc::new(window))),
            Self::MutateVisit(job) => Ok(NativeReply::Mutated(Arc::new(job.perform(db)))),
            Self::Directory(job) => job.perform(db).map(NativeReply::Directory),
            Self::Forget {
                mount,
                serial,
                count,
            } => db
                .forget_native(mount, serial, count)
                .map(|()| NativeReply::Done),
            Self::State(mount) => db.native_mount_state(mount).map(NativeReply::State),
            Self::Revoke(mount) => db.revoke_native_mount(mount).map(|()| NativeReply::Done),
        }
    }
}
