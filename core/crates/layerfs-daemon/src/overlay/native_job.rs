//! Native custody jobs on the existing fair owner, independent of fuser types.
use crate::{NativeDirectoryJob, NativeDirectoryReply, ServiceClass};
use layerfs_overlay::{
    BaseSource, FileRead, NativeMount, NativeMountState, OpenFile, Overlay, OverlayError,
    OverlayResult, Route,
};
use layerfs_workspace::{
    NativeDataVisit, NativeMutationJob, NativeMutationOutcome, NativeMutationVisit, NativeReadJob,
    NativeReadOutcome, NativeReadVisit, NativeWindow,
};
use std::sync::Arc;

#[derive(Debug)]
pub enum NativeJob {
    Mount {
        root: u64,
    },
    RetainedMount,
    Source {
        mount: NativeMount,
        request: u64,
        serial: u64,
    },
    RetainedSource {
        mount: NativeMount,
        request: u64,
    },
    RetainedRead {
        mount: NativeMount,
        request: u64,
    },
    FileSource {
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: u64,
    },
    HandleSource {
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: u64,
    },
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
    Observe(Box<NativeReadJob>),
    /// LOOKUP or GETATTR decided in this one job, with no request source.
    ObserveVisit(Box<NativeReadVisit>),
    /// READ or READLINK: the window's local part, read in this one job.
    ReadVisit(Box<NativeDataVisit>),
    /// A native mutation decided and published in this one job.
    MutateVisit(Box<NativeMutationVisit>),
    /// A handle-addressed mutation's descriptor and request source.
    OpenSource {
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: u64,
    },
    /// One owner round of a native mutation; publishes at most once.
    Mutate(Box<NativeMutationJob>),
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
    Source(BaseSource),
    RetainedSource(Option<BaseSource>),
    RetainedRead(Option<FileRead>),
    RetainedFile(Option<OpenFile>),
    File(OpenFile),
    Observed(Arc<NativeReadOutcome>),
    Window(Arc<NativeWindow>),
    OpenSource(BaseSource, OpenFile),
    Mutated(Arc<NativeMutationOutcome>),
    Directory(NativeDirectoryReply),
    State(NativeMountState),
    Done,
}
impl NativeJob {
    pub(crate) fn class(&self) -> ServiceClass {
        match self {
            Self::Source { .. }
            | Self::FileSource { .. }
            | Self::HandleSource { .. }
            | Self::OpenSource { .. } => ServiceClass::Source,
            Self::Observe(_) | Self::ObserveVisit(_) | Self::ReadVisit(_) => ServiceClass::Read,
            Self::Mutate(_) | Self::MutateVisit(_) => ServiceClass::Mutation,
            Self::Directory(job) => job.class(),
            _ => ServiceClass::Lifecycle,
        }
    }
    pub(crate) fn charge(&self) -> (usize, usize) {
        match self {
            Self::Observe(job) => (
                std::mem::size_of::<NativeReadJob>() + job.charge(),
                std::mem::size_of::<NativeReadOutcome>()
                    + 4 * (std::mem::size_of::<layerfs_workspace::Need>() + 255)
                    + 2 * std::mem::size_of::<usize>(),
            ),
            // The boxed job with its bounded facts, names and write window,
            // and at most one reply of needed names or one changed inode.
            Self::Mutate(job) => (
                std::mem::size_of::<NativeMutationJob>() + job.charge(),
                std::mem::size_of::<NativeMutationOutcome>()
                    + layerfs_overlay::PAGE_ROWS
                        * (std::mem::size_of::<layerfs_workspace::Need>() + 255),
            ),
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
            Self::Source { mount, .. }
            | Self::RetainedSource { mount, .. }
            | Self::RetainedRead { mount, .. }
            | Self::FileSource { mount, .. }
            | Self::HandleSource { mount, .. }
            | Self::RetainedFile { mount, .. }
            | Self::File { mount, .. }
            | Self::CloseFile { mount, .. }
            | Self::OpenSource { mount, .. }
            | Self::Forget { mount, .. }
            | Self::State(mount)
            | Self::Revoke(mount) => Some(mount.route()),
            Self::Observe(job) => Some(job.source().route()),
            Self::Mutate(job) => Some(job.source().route()),
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
            Self::HandleSource {
                mount,
                request,
                serial,
                handle,
            } => db
                .acquire_native_handle_source(mount, request, serial, handle)
                .map(NativeReply::Source),
            Self::Mount { root } => db.create_native_mount(route, root).map(NativeReply::Mount),
            Self::RetainedMount => db
                .retained_native_mount(route)
                .map(NativeReply::RetainedMount),
            Self::Source {
                mount,
                request,
                serial,
            } => db
                .acquire_native_source(mount, request, serial)
                .map(NativeReply::Source),
            Self::RetainedSource { mount, request } => db
                .retained_native_source(mount, request)
                .map(NativeReply::RetainedSource),
            Self::RetainedRead { mount, request } => db
                .retained_native_read(mount, request)
                .map(NativeReply::RetainedRead),
            Self::FileSource {
                mount,
                request,
                serial,
                handle,
            } => db
                .acquire_native_file_source(mount, request, serial, handle)
                .map(NativeReply::Source),
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
            Self::Observe(job) => Ok(NativeReply::Observed(Arc::new(job.perform(db)))),
            Self::OpenSource {
                mount,
                request,
                serial,
                handle,
            } => db
                .acquire_native_open_source(mount, request, serial, handle)
                .map(|(source, file)| NativeReply::OpenSource(source, file)),
            Self::Mutate(job) => Ok(NativeReply::Mutated(Arc::new(job.perform(db)))),
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
