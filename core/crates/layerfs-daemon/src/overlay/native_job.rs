//! Native custody jobs on the existing fair owner, independent of fuser types.
use crate::ServiceClass;
use layerfs_overlay::{
    BaseSource, FileRead, NativeMount, NativeMountState, Overlay, OverlayError, OverlayResult,
    Route,
};
use layerfs_workspace::{NativeReadJob, NativeReadOutcome};
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
    Observe(Box<NativeReadJob>),
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
    Observed(Arc<NativeReadOutcome>),
    State(NativeMountState),
    Done,
}
impl NativeJob {
    pub(crate) fn class(&self) -> ServiceClass {
        match self {
            Self::Source { .. } => ServiceClass::Source,
            Self::Observe(_) => ServiceClass::Read,
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
            _ => (0, 256),
        }
    }
    pub(crate) fn perform(self, db: &Overlay, route: Route) -> OverlayResult<NativeReply> {
        let expected = match &self {
            Self::Source { mount, .. }
            | Self::RetainedSource { mount, .. }
            | Self::RetainedRead { mount, .. }
            | Self::Forget { mount, .. }
            | Self::State(mount)
            | Self::Revoke(mount) => Some(mount.route()),
            Self::Observe(job) => Some(job.source().route()),
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
            Self::Observe(job) => Ok(NativeReply::Observed(Arc::new(job.perform(db)))),
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
