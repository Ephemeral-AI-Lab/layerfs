//! A wrapper of the rig's request ports that watches the publication
//! ticket contract from outside the export: every `reply_attempted` waits
//! until the test's client has received the reply of the call that made the
//! publication,
//! and records whether that happened (a release before the reply reached the
//! wire would wait forever on a synchronous client, so it times out and is
//! counted as a violation). It can also refuse serial reservations, as a
//! contended allocator does.
use super::client::Wire;
use layerfs_fuse::ports::{
    MountServices, RequestServices, ServiceError, ServiceFuture, ServiceReply,
};
use layerfs_overlay::{KernelCookieOffer, KernelMount, Publication};
use layerfs_workspace::{
    BaseView, KernelDirectoryWindow, KernelMutationOutcome, KernelReadOperation, KernelReadOutcome,
    KernelVisitRequest, KernelWindow, VisitFacts, WriteData,
};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

/// What the spy saw.
#[derive(Default)]
pub struct Watch {
    pub wire: Arc<Wire>,
    /// Releases made after the reply was received, and before it.
    pub released_after: AtomicU64,
    pub released_before: AtomicU64,
    /// Publishing visits (`Applied` outcomes) and reservations refused.
    pub published: AtomicU64,
    /// Reservations still to refuse.
    pub refuse_serials: AtomicU64,
    pub refused: AtomicU64,
    pub log: Mutex<Vec<String>>,
    /// Each publication and the call count when it was made: the client was
    /// then waiting for exactly that call's reply.
    pub owed: Mutex<Vec<(Publication, u64)>>,
}
pub struct Spy {
    pub inner: Arc<dyn MountServices>,
    pub watch: Arc<Watch>,
}
impl MountServices for Spy {
    fn request(
        &self,
        fence: &layerfs_fuse::ports::Fence,
    ) -> Result<Arc<dyn RequestServices>, ServiceError> {
        Ok(Arc::new(Request {
            inner: self.inner.request(fence)?,
            watch: self.watch.clone(),
        }))
    }
}
struct Request {
    inner: Arc<dyn RequestServices>,
    watch: Arc<Watch>,
}
impl RequestServices for Request {
    fn directory_visit(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
        offset: u64,
        after: Option<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelDirectoryWindow>>> {
        self.inner
            .directory_visit(mount, serial, handle, offset, after)
    }
    fn publish_cookies(
        &self,
        offer: KernelCookieOffer,
        names: Vec<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.inner.publish_cookies(offer, names)
    }
    fn close_directory(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.inner.close_directory(mount, serial, handle)
    }
    fn read_visit(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: Option<u64>,
        offset: u64,
        length: u32,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelWindow>>> {
        self.inner.read_visit(mount, serial, handle, offset, length)
    }
    fn close_file(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.inner.close_file(mount, serial, handle)
    }
    fn provider_failure(&self) -> Result<Option<ServiceError>, ServiceError> {
        self.inner.provider_failure()
    }
    fn failed_base_read(&self, step: ServiceError) -> ServiceError {
        self.inner.failed_base_read(step)
    }
    fn observe_visit(
        &self,
        mount: KernelMount,
        request: u64,
        serial: u64,
        handle: Option<u64>,
        operation: KernelReadOperation,
        facts: Arc<VisitFacts>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelReadOutcome>>> {
        self.inner
            .observe_visit(mount, request, serial, handle, operation, facts)
    }
    fn base(&self) -> ServiceFuture<'_, BaseView> {
        self.inner.base()
    }
    fn mutate_visit(
        &self,
        request: KernelVisitRequest,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelMutationOutcome>>> {
        let watch = self.watch.clone();
        let future = self.inner.mutate_visit(request);
        Box::pin(async move {
            let reply = future.await?;
            if let Ok(layerfs_workspace::JobOutcome::Applied { publication, .. }) =
                &reply.get().result
            {
                watch.published.fetch_add(1, Ordering::AcqRel);
                let sent = watch.wire.counts.lock().unwrap().0;
                watch.owed.lock().unwrap().push((*publication, sent));
            }
            Ok(reply)
        })
    }
    fn recognize(&self, serial: u64, offset: u64, data: WriteData) -> ServiceFuture<'_, WriteData> {
        self.inner.recognize(serial, offset, data)
    }
    fn forget(
        &self,
        mount: KernelMount,
        serial: u64,
        count: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.inner.forget(mount, serial, count)
    }
    fn reserve_serial(&self) -> Result<Option<u64>, ServiceError> {
        let refuse = self
            .watch
            .refuse_serials
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(1)
            })
            .is_ok();
        if refuse {
            self.watch.refused.fetch_add(1, Ordering::AcqRel);
            return Ok(None);
        }
        self.inner.reserve_serial()
    }
    fn reply_attempted(&self, publication: Publication) -> ServiceFuture<'_, ServiceReply<()>> {
        let wire = &self.watch.wire;
        let owed = {
            let mut owed = self.watch.owed.lock().unwrap();
            let at = owed.iter().position(|(made, _)| *made == publication);
            at.map_or(u64::MAX, |at| owed.remove(at).1)
        };
        let counts = wire.counts.lock().unwrap();
        // The reply of the call that published this ticket must reach the
        // client before the release, whatever the client sends next.
        let (counts, timeout) = wire
            .changed
            .wait_timeout_while(counts, Duration::from_secs(5), |(_, received)| {
                *received < owed
            })
            .unwrap();
        let (sent, received) = *counts;
        drop(counts);
        if timeout.timed_out() {
            self.watch.released_before.fetch_add(1, Ordering::AcqRel);
            self.watch.log.lock().unwrap().push(format!(
                "released before reply: sent {sent} received {received}"
            ));
        } else {
            self.watch.released_after.fetch_add(1, Ordering::AcqRel);
        }
        self.inner.reply_attempted(publication)
    }
}
