//! Directory visit, publication and release jobs with ordinary bounded
//! owner charging.
use crate::ServiceClass;
use layerfs_overlay::{
    NativeCookieOffer, NativeDirectory, NativeMount, Overlay, OverlayResult, Route,
};
use layerfs_workspace::{NativeDirectoryVisit, NativeDirectoryWindow};
use std::{mem::size_of, sync::Arc};

#[derive(Debug)]
pub enum NativeDirectoryJob {
    Retained {
        mount: NativeMount,
        request: u64,
    },
    /// READDIR's reading visit: nothing is written or recorded.
    Visit(NativeDirectoryVisit),
    /// READDIR's publishing visit: the accepted names of one reply.
    Publish {
        offer: NativeCookieOffer,
        names: Vec<Vec<u8>>,
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
    Retained(Option<NativeDirectory>),
    Window(Arc<NativeDirectoryWindow>),
    Done,
}
impl NativeDirectoryJob {
    pub(crate) fn route(&self) -> Route {
        match self {
            Self::Retained { mount, .. } | Self::Close { mount, .. } => mount.route(),
            Self::Visit(visit) => visit.mount().route(),
            Self::Publish { offer, .. } => offer.directory().mount().route(),
        }
    }
    pub(crate) fn class(&self) -> ServiceClass {
        match self {
            Self::Visit(_) | Self::Publish { .. } => ServiceClass::Read,
            _ => ServiceClass::Lifecycle,
        }
    }
    pub(crate) fn charge(&self) -> (usize, usize) {
        let (input, reply) = match self {
            Self::Visit(visit) => (
                visit.charge(),
                2 * size_of::<usize>()
                    + size_of::<NativeDirectoryWindow>()
                    + NativeDirectoryWindow::CHARGE,
            ),
            Self::Publish { offer, names } => (
                offer.heap_bytes()
                    + names.capacity() * size_of::<Vec<u8>>()
                    + names.iter().map(Vec::capacity).sum::<usize>(),
                0,
            ),
            _ => (0, 256),
        };
        (size_of::<Self>() + input, reply)
    }
    pub(crate) fn perform(self, db: &Overlay) -> OverlayResult<NativeDirectoryReply> {
        match self {
            Self::Retained { mount, request } => db
                .retained_native_directory(mount, request)
                .map(NativeDirectoryReply::Retained),
            Self::Visit(visit) => visit
                .perform(db)
                .map(|window| NativeDirectoryReply::Window(Arc::new(window))),
            Self::Publish { offer, names } => db
                .publish_native_cookies(&offer, &names)
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
