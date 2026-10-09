//! Directory ownership/page/cookie jobs with ordinary bounded owner charging.
use crate::ServiceClass;
use layerfs_overlay::{
    DirectoryEntry, NativeCookie, NativeCookiePlan, NativeDirectory, NativeDirectoryPage,
    NativeDirectoryRead, NativeMount, Overlay, OverlayResult, Route, PAGE_ROWS,
};
use std::{mem::size_of, sync::Arc};

#[derive(Debug)]
pub enum NativeDirectoryJob {
    Handle {
        mount: NativeMount,
        serial: u64,
        handle: u64,
    },
    Retained {
        mount: NativeMount,
        request: u64,
    },
    Read {
        directory: NativeDirectory,
        request: u64,
        offset: u64,
    },
    RetainedRead {
        mount: NativeMount,
        request: u64,
    },
    Page {
        read: Arc<NativeDirectoryRead>,
        after: Option<Vec<u8>>,
    },
    PrepareCookies {
        read: Arc<NativeDirectoryRead>,
        names: Vec<Vec<u8>>,
    },
    PublishCookies {
        plan: Arc<NativeCookiePlan>,
        accepted: usize,
    },
    /// RELEASEDIR: the open descriptor is found and closed in this one job.
    Close {
        mount: NativeMount,
        serial: u64,
        handle: u64,
    },
}
#[derive(Debug)]
pub enum NativeDirectoryReply {
    Handle(NativeDirectory),
    Retained(Option<NativeDirectory>),
    Read(Arc<NativeDirectoryRead>),
    RetainedRead(Option<Arc<NativeDirectoryRead>>),
    Page(Arc<NativeDirectoryPage>),
    Cookies(Arc<NativeCookiePlan>),
    Done,
}
impl NativeDirectoryJob {
    pub(crate) fn route(&self) -> Route {
        match self {
            Self::Handle { mount, .. }
            | Self::Retained { mount, .. }
            | Self::RetainedRead { mount, .. }
            | Self::Close { mount, .. } => mount.route(),
            Self::Read { directory, .. } => directory.mount().route(),
            Self::Page { read, .. } | Self::PrepareCookies { read, .. } => read.source().route(),
            Self::PublishCookies { plan, .. } => plan.read().source().route(),
        }
    }
    pub(crate) fn class(&self) -> ServiceClass {
        match self {
            Self::Read { .. } => ServiceClass::Source,
            Self::Page { .. } | Self::PrepareCookies { .. } | Self::PublishCookies { .. } => {
                ServiceClass::Read
            }
            _ => ServiceClass::Lifecycle,
        }
    }
    pub(crate) fn charge(&self) -> (usize, usize) {
        let arc = 2 * size_of::<usize>();
        let read = arc + size_of::<NativeDirectoryRead>() + 255;
        let (input, reply) = match self {
            Self::Page { read: input, after } => (
                read + input.heap_bytes() + after.as_ref().map_or(0, Vec::capacity),
                arc + size_of::<NativeDirectoryPage>()
                    + 510
                    + 2 * PAGE_ROWS
                        * (size_of::<DirectoryEntry>()
                            + 255
                            + size_of::<(u64, layerfs_overlay::InodeKind)>()),
            ),
            Self::PrepareCookies { read: input, names } => (
                read + input.heap_bytes()
                    + names.capacity() * size_of::<Vec<u8>>()
                    + names.iter().map(Vec::capacity).sum::<usize>(),
                arc + size_of::<NativeCookiePlan>()
                    + 255
                    + PAGE_ROWS * (size_of::<NativeCookie>() + 255),
            ),
            Self::PublishCookies { plan, .. } => {
                (arc + size_of::<NativeCookiePlan>() + plan.heap_bytes(), 0)
            }
            Self::Read { .. } | Self::RetainedRead { .. } => (0, read),
            _ => (0, 256),
        };
        (size_of::<Self>() + input, reply)
    }
    pub(crate) fn perform(self, db: &Overlay) -> OverlayResult<NativeDirectoryReply> {
        match self {
            Self::Handle {
                mount,
                serial,
                handle,
            } => db
                .native_directory(mount, serial, handle)
                .map(NativeDirectoryReply::Handle),
            Self::Retained { mount, request } => db
                .retained_native_directory(mount, request)
                .map(NativeDirectoryReply::Retained),
            Self::Read {
                directory,
                request,
                offset,
            } => db
                .acquire_native_directory_read(directory, request, offset)
                .map(|read| NativeDirectoryReply::Read(Arc::new(read))),
            Self::RetainedRead { mount, request } => db
                .retained_native_directory_read(mount, request)
                .map(|read| NativeDirectoryReply::RetainedRead(read.map(Arc::new))),
            Self::Page { read, after } => db
                .native_directory_page(&read, after.as_deref())
                .map(|page| NativeDirectoryReply::Page(Arc::new(page))),
            Self::PrepareCookies { read, names } => db
                .prepare_native_cookies(&read, &names)
                .map(|plan| NativeDirectoryReply::Cookies(Arc::new(plan))),
            Self::PublishCookies { plan, accepted } => db
                .publish_native_cookies(&plan, accepted)
                .map(|()| NativeDirectoryReply::Done),
            Self::Close {
                mount,
                serial,
                handle,
            } => db
                .close_native_directory(mount, serial, handle)
                .map(|()| NativeDirectoryReply::Done),
        }
    }
}
