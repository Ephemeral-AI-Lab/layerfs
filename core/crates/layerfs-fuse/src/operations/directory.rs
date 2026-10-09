//! One native enumeration request: reading visits, the reply's accepted
//! prefix, one publishing visit. Nothing is held in the engine between them
//! or after the reply.
use crate::{
    ports::{BaseDemandFailed, Fenced, RequestServices, ServiceError},
    NextTurn,
};
use layerfs_overlay::{NativeMount, OverlayError};
use layerfs_workspace::{NativeDirectoryBatch, NativeDirectoryEntry, NativeDirectoryWindow};
use std::{fmt, sync::Arc};

/// One kernel READDIR on an open directory handle. It owns plain values
/// only: the window its last reading visit returned and the parent for `..`.
pub struct DirectoryStream {
    services: Arc<dyn RequestServices>,
    mount: NativeMount,
    request: u64,
    serial: u64,
    handle: u64,
    offset: u64,
    parent: Option<u64>,
    window: Option<NativeDirectoryWindow>,
    ended: bool,
}
pub struct DirectoryBatch {
    stream: DirectoryStream,
    batch: NativeDirectoryBatch,
}
pub enum DirectoryStep {
    Batch(DirectoryBatch),
    End(DirectoryStream),
}
/// The request's offset was handed out before its handle was last listed
/// from offset 0 over published replies. The visit recorded nothing.
#[derive(Clone, Copy, Debug)]
struct Rewound;
impl fmt::Display for Rewound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("directory offset from before the handle's rewind")
    }
}
impl std::error::Error for Rewound {}
/// The exact failed step and the request's identity. Nothing is owned in the
/// engine: a reading visit records nothing, and a publication that failed
/// or whose outcome is unknown is never repeated.
pub struct DirectoryFailure {
    pub reason: ServiceError,
    pub provider: Option<ServiceError>,
    stream: DirectoryStream,
    batch: Option<NativeDirectoryBatch>,
    prefix: Option<(usize, bool)>,
}
impl DirectoryFailure {
    fn new(
        reason: ServiceError,
        stream: DirectoryStream,
        batch: Option<NativeDirectoryBatch>,
    ) -> Self {
        let provider = match stream.services.provider_failure() {
            Ok(error) => error,
            Err(error) => Some(error),
        };
        Self {
            reason,
            provider,
            stream,
            batch,
            prefix: None,
        }
    }
    pub const fn mount(&self) -> NativeMount {
        self.stream.mount
    }
    pub const fn request(&self) -> u64 {
        self.stream.request
    }
    /// The batch whose reply conversion or publication failed.
    pub fn offered(&self) -> Option<&NativeDirectoryBatch> {
        self.batch.as_ref()
    }
    /// Requested accepted count and whether the publication port was invoked.
    /// Actual admission/effect is described by the original service outcome.
    pub const fn prefix(&self) -> Option<(usize, bool)> {
        self.prefix
    }
    fn with_prefix(mut self, accepted: usize, attempted: bool) -> Self {
        self.prefix = Some((accepted, attempted));
        self
    }
    /// The mount's stopped fence refused the failed step before its attempt.
    pub fn fenced(&self) -> bool {
        self.reason.is::<Fenced>()
    }
    /// The failed step was a base demand of this request alone.
    pub fn base_demand(&self) -> Option<&BaseDemandFailed> {
        self.reason.downcast_ref()
    }
    /// The offset is from before the handle's last rewind: an invalid
    /// argument of this request, and no failure of the mount.
    pub fn rewound(&self) -> bool {
        self.reason.is::<Rewound>()
    }
    /// Ends an enumeration that was fenced, whose base demand failed or
    /// whose offset a rewind retired. It holds nothing in the engine, so
    /// nothing is released and no job is submitted; an unpublished batch
    /// made no offset valid. Any other failure is returned unchanged.
    pub async fn relinquish(self) -> Result<(), DirectoryFailure> {
        if !self.fenced() && self.base_demand().is_none() && !self.rewound() {
            return Err(self);
        }
        Ok(())
    }
}
impl fmt::Debug for DirectoryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DirectoryFailure")
            .field("reason", &self.reason)
            .field("provider", &self.provider)
            .field("mount", &self.stream.mount)
            .field("request", &self.stream.request)
            .field("serial", &self.stream.serial)
            .field("handle", &self.stream.handle)
            .field("offset", &self.stream.offset)
            .field("batch", &self.batch)
            .field("prefix", &self.prefix)
            .finish()
    }
}
impl fmt::Display for DirectoryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.reason.fmt(f)
    }
}
impl std::error::Error for DirectoryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.reason.as_ref())
    }
}
impl DirectoryStream {
    /// The request's first reading visit: an offset that no reply of this
    /// open handle handed out, one that a rewind retired, a closed handle
    /// and a stopped mount fail here.
    pub async fn prepare(
        services: Arc<dyn RequestServices>,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: u64,
        offset: u64,
    ) -> Result<Self, DirectoryFailure> {
        let mut stream = Self {
            services,
            mount,
            request,
            serial,
            handle,
            offset,
            parent: None,
            window: None,
            ended: false,
        };
        match stream.visit(None).await {
            Ok(())
                if stream
                    .window
                    .as_ref()
                    .is_some_and(|window| window.rewound()) =>
            {
                Err(DirectoryFailure::new(Box::new(Rewound), stream, None))
            }
            Ok(()) => Ok(stream),
            Err(reason) => Err(DirectoryFailure::new(reason, stream, None)),
        }
    }
    async fn visit(&mut self, after: Option<Vec<u8>>) -> Result<(), ServiceError> {
        let reply = self
            .services
            .directory_visit(self.mount, self.serial, self.handle, self.offset, after)
            .await?;
        // The request's own copy: the job and its credit are gone before any
        // provider wait and before the publishing visit.
        let window = NativeDirectoryWindow::clone(reply.get());
        drop(reply);
        self.parent = self.parent.or(window.page.parent());
        self.window = Some(window);
        Ok(())
    }
    /// Fixed dot positions; no directory scan and no nlookup acquisition.
    pub fn dots(&self) -> impl Iterator<Item = (&'static str, u64, u64)> {
        let offset = self.offset;
        [
            Some((".", self.serial, 1)),
            self.parent.map(|parent| ("..", parent, 2)),
        ]
        .into_iter()
        .flatten()
        .filter(move |(_, _, cookie)| *cookie > offset)
    }
    /// Consume this state before another owner attempt. Empty whiteout windows
    /// keep advancing, yielding between windows, until an entry or real EOF.
    pub async fn next(mut self) -> Result<DirectoryStep, DirectoryFailure> {
        match self.advance().await {
            Ok(Some(batch)) => Ok(DirectoryStep::Batch(DirectoryBatch {
                stream: self,
                batch,
            })),
            Ok(None) => Ok(DirectoryStep::End(self)),
            Err(reason) => Err(DirectoryFailure::new(reason, self, None)),
        }
    }
    async fn advance(&mut self) -> Result<Option<NativeDirectoryBatch>, ServiceError> {
        loop {
            let Some(window) = self.window.take().filter(|_| !self.ended) else {
                return Ok(None);
            };
            // Only a window whose inherited names or kinds memory did not
            // hold leaves the receive loop and takes one Store reader.
            let base = if window.listing.is_none() {
                // Provider reads never run on the loop that received the request.
                crate::LeaveReceiver::default().await;
                Some(self.services.base().await?)
            } else {
                None
            };
            let batch = window.finish(base.as_ref());
            drop(base); // Return the actual reader before another SQL job.
            let batch = match batch {
                Ok(batch) => batch,
                Err(error) => return Err(self.services.failed_base_read(error.into())),
            };
            if !batch.entries.is_empty() {
                return Ok(Some(batch));
            }
            match batch.continuation {
                Some(next) => {
                    NextTurn::default().await;
                    self.visit(Some(next)).await?;
                }
                None => {
                    self.ended = true;
                    return Ok(None);
                }
            }
        }
    }
    pub fn retain(self, reason: ServiceError) -> DirectoryFailure {
        DirectoryFailure::new(reason, self, None)
    }
}
impl DirectoryBatch {
    pub fn entries(&self) -> impl Iterator<Item = (&NativeDirectoryEntry, u64)> {
        self.batch.entries.iter().zip(self.batch.first..)
    }
    /// Publish exactly the prefix that actually fit the native reply buffer,
    /// before the reply is sent. Offsets of an already published reply and
    /// an empty prefix publish nothing and submit no job, except that a
    /// rewinding offer is published with whatever prefix it has. Consuming
    /// self prevents a second attempt after an error or lost outcome.
    pub async fn accept(self, accepted: usize) -> Result<DirectoryStream, DirectoryFailure> {
        let Self {
            mut stream,
            mut batch,
        } = self;
        if accepted > batch.entries.len() {
            return Err(DirectoryFailure::new(
                Box::new(OverlayError::Invalid("native cookie prefix")),
                stream,
                Some(batch),
            )
            .with_prefix(accepted, false));
        }
        // One published window ends this reply even when names remain; the
        // next native read resumes after the last accepted offset.
        stream.ended = true;
        let Some(offer) = batch
            .publish
            .take()
            .filter(|offer| accepted != 0 || offer.rewinds())
        else {
            return Ok(stream);
        };
        let names = batch.entries[..accepted]
            .iter()
            .map(|entry| entry.name.clone())
            .collect();
        match stream.services.publish_cookies(offer, names).await {
            Ok(reply) => {
                drop(reply);
                Ok(stream)
            }
            Err(reason) => {
                Err(DirectoryFailure::new(reason, stream, Some(batch)).with_prefix(accepted, true))
            }
        }
    }
    pub fn retain(self, reason: ServiceError) -> DirectoryFailure {
        DirectoryFailure::new(reason, self.stream, Some(self.batch))
    }
}
