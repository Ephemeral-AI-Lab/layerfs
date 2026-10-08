//! One native enumeration request, bounded pages and exact accepted cookies.
use crate::{
    ports::{RequestServices, ServiceError, ServiceReply},
    NextTurn,
};
use layerfs_overlay::{
    BaseSource, NativeCookiePlan, NativeDirectory, NativeDirectoryPage, NativeDirectoryRead,
    NativeMount, OverlayError,
};
use layerfs_workspace::{NativeDirectoryEntry, NativeDirectoryListing, SourceView};
use std::{fmt, sync::Arc};

/// Source/cookie ownership of one kernel READDIR, independent of its open handle.
pub struct DirectoryStream {
    services: Arc<dyn RequestServices>,
    mount: NativeMount,
    request: u64,
    serial: u64,
    handle: u64,
    offset: u64,
    directory: Option<NativeDirectory>,
    source: Option<BaseSource>,
    read: Option<Arc<NativeDirectoryRead>>,
    view: Option<SourceView>,
    page: Option<ServiceReply<Arc<NativeDirectoryPage>>>,
    listing: Option<NativeDirectoryListing>,
    cookies: Option<ServiceReply<Arc<NativeCookiePlan>>>,
    after: Option<Vec<u8>>,
    ended: bool,
}
pub struct DirectoryBatch {
    stream: DirectoryStream,
    listing: NativeDirectoryListing,
    plan: Arc<NativeCookiePlan>,
}
pub enum DirectoryStep {
    Batch(DirectoryBatch),
    End(DirectoryStream),
}
pub struct DirectoryFailure {
    pub reason: ServiceError,
    pub provider: Option<ServiceError>,
    stream: DirectoryStream,
    batch: Option<(NativeDirectoryListing, Arc<NativeCookiePlan>)>,
    prefix: Option<(usize, bool)>,
}
impl DirectoryFailure {
    fn new(
        reason: ServiceError,
        stream: DirectoryStream,
        batch: Option<(NativeDirectoryListing, Arc<NativeCookiePlan>)>,
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
    pub fn retained_read(&self) -> Option<&NativeDirectoryRead> {
        self.stream.read.as_deref()
    }
    pub const fn retained_source(&self) -> Option<BaseSource> {
        self.stream.source
    }
    pub fn retained_cookies(&self) -> Option<&NativeCookiePlan> {
        self.batch
            .as_ref()
            .map(|(_, plan)| plan.as_ref())
            .or_else(|| {
                self.stream
                    .cookies
                    .as_ref()
                    .map(|reply| reply.get().as_ref())
            })
    }
    pub fn retained_page(&self) -> Option<&NativeDirectoryPage> {
        self.stream.page.as_ref().map(|reply| reply.get().as_ref())
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
            .field("read", &self.stream.read)
            .field("cookies", &self.retained_cookies())
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
            directory: None,
            source: None,
            read: None,
            view: None,
            page: None,
            listing: None,
            cookies: None,
            after: None,
            ended: false,
        };
        match stream.acquire().await {
            Ok(()) => Ok(stream),
            Err(reason) => Err(DirectoryFailure::new(reason, stream, None)),
        }
    }
    async fn acquire(&mut self) -> Result<(), ServiceError> {
        let reply = self
            .services
            .directory(self.mount, self.serial, self.handle)
            .await?;
        let directory = *reply.get();
        self.directory = Some(directory);
        drop(reply);
        let reply = self
            .services
            .directory_read(directory, self.request, self.offset)
            .await?;
        // Deep-copy the bounded capability/cursor into request-owned storage.
        // No shared reply Arc survives consumption of the original completion.
        let read = Arc::new(reply.get().as_ref().clone());
        self.source = Some(read.source());
        self.read = Some(read.clone());
        drop(reply);
        self.after = read.cursor().after_name().map(<[u8]>::to_vec);
        self.view = Some(self.services.view(read.source())?);
        Ok(())
    }
    /// Fixed dot positions; no directory scan and no nlookup acquisition.
    pub fn dots(&self) -> impl Iterator<Item = (&'static str, u64, u64)> {
        let read = self.read.as_ref().expect("prepared native directory");
        let values = [
            (".", read.directory().serial(), 1),
            ("..", read.parent(), 2),
        ];
        let offset = self.offset;
        values
            .into_iter()
            .filter(move |(_, _, cookie)| *cookie > offset)
    }
    /// Consume this state before another owner attempt. Empty whiteout windows
    /// keep advancing, yielding between windows, until an entry or real EOF.
    pub async fn next(mut self) -> Result<DirectoryStep, DirectoryFailure> {
        match self.advance().await {
            Ok(Some((listing, plan))) => Ok(DirectoryStep::Batch(DirectoryBatch {
                stream: self,
                listing,
                plan,
            })),
            Ok(None) => Ok(DirectoryStep::End(self)),
            Err(reason) => Err(DirectoryFailure::new(reason, self, None)),
        }
    }
    async fn advance(
        &mut self,
    ) -> Result<Option<(NativeDirectoryListing, Arc<NativeCookiePlan>)>, ServiceError> {
        if self.ended {
            return Ok(None);
        }
        loop {
            let read = self.read.as_ref().unwrap().clone();
            self.page = Some(
                self.services
                    .directory_page(read.clone(), self.after.clone())
                    .await?,
            );
            let page = self.page.as_ref().unwrap().get();
            if page.read != *read || page.after != self.after {
                return Err(Box::new(OverlayError::Invalid(
                    "native directory page identity",
                )));
            }
            let immutable = self.services.immutable(self.view.as_ref().unwrap()).await?;
            let listing = immutable.native_directory_listing(page)?;
            drop(immutable);
            self.page = None; // All original page consumers ended before another SQL job.
            if listing.entries.is_empty() {
                match listing.continuation {
                    Some(next) => {
                        self.after = Some(next);
                        NextTurn::default().await;
                        continue;
                    }
                    None => {
                        self.ended = true;
                        return Ok(None);
                    }
                }
            }
            self.listing = Some(listing);
            let names = self
                .listing
                .as_ref()
                .unwrap()
                .entries
                .iter()
                .map(|entry| entry.name.clone())
                .collect();
            self.cookies = Some(self.services.directory_cookies(read.clone(), names).await?);
            let cookies = self.cookies.as_ref().unwrap().get();
            if cookies.read() != read.as_ref()
                || !matches_listing(self.listing.as_ref().unwrap(), cookies)
            {
                return Err(Box::new(OverlayError::Invalid(
                    "native cookie names/source",
                )));
            }
            // This independent bounded copy retains every original reservation
            // value; the old reply has no surviving shared-payload consumers.
            // Publication can therefore obtain an ordinary slot even with all
            // sixteen native request slots occupied.
            let plan = Arc::new(cookies.as_ref().clone());
            self.cookies = None;
            return Ok(Some((self.listing.take().unwrap(), plan)));
        }
    }
    pub fn retain(self, reason: ServiceError) -> DirectoryFailure {
        DirectoryFailure::new(reason, self, None)
    }
    /// After the data reply consumer ends, release the original source once.
    /// No destructor performs SQL or retries a failed release.
    pub async fn dispose(mut self) -> Result<(), DirectoryFailure> {
        let source = self.source.unwrap();
        self.view = None;
        self.read = None;
        match self.services.release_source(source).await {
            Ok(reply) => {
                drop(reply);
                Ok(())
            }
            Err(reason) => {
                // Exact source remains recoverable by mount/request even when
                // release outcome is uncertain; retain its copied token too.
                Err(DirectoryFailure::new(reason, self, None))
            }
        }
    }
}
impl DirectoryBatch {
    pub fn entries(&self) -> impl Iterator<Item = (&NativeDirectoryEntry, u64)> {
        self.listing
            .entries
            .iter()
            .zip(self.plan.entries())
            .map(|(entry, cookie)| (entry, cookie.cookie()))
    }
    /// Publish exactly the prefix that actually fit the native reply buffer.
    /// Consuming self prevents a second attempt after an error or lost outcome.
    pub async fn accept(self, accepted: usize) -> Result<DirectoryStream, DirectoryFailure> {
        let Self {
            mut stream,
            listing,
            plan,
        } = self;
        let valid = accepted <= listing.entries.len();
        if !valid {
            return Err(DirectoryFailure::new(
                Box::new(OverlayError::Invalid("native cookie prefix")),
                stream,
                Some((listing, plan)),
            )
            .with_prefix(accepted, false));
        }
        let result = stream
            .services
            .publish_cookies(plan.clone(), accepted)
            .await;
        match result {
            Ok(reply) => {
                drop(reply);
                // A directory read owns one cookie plan, so one published
                // window ends this reply even when names remain; the next
                // native read resumes after the last published cookie.
                stream.ended = true;
                stream.after = listing.continuation;
                drop(plan);
                NextTurn::default().await;
                Ok(stream)
            }
            Err(reason) => Err(DirectoryFailure::new(reason, stream, Some((listing, plan)))
                .with_prefix(accepted, true)),
        }
    }
    pub fn retain(self, reason: ServiceError) -> DirectoryFailure {
        DirectoryFailure::new(reason, self.stream, Some((self.listing, self.plan)))
    }
}
fn matches_listing(listing: &NativeDirectoryListing, plan: &NativeCookiePlan) -> bool {
    plan.entries().len() == listing.entries.len()
        && listing
            .entries
            .iter()
            .zip(plan.entries())
            .all(|(entry, cookie)| entry.name == cookie.name())
}
