//! Async Fuse ports over typed Workspace calls and direct Store readers.
use crate::{
    store::{BoundWorkspace, PortError, ReadAdmissionError, StoreOperation},
    OwnerError,
};
use layerfs_fuse::ports::{
    BaseDemandFailed, Fence, Fenced, MountServices, RequestServices, ServiceError, ServiceFuture,
    ServiceReply,
};
use layerfs_history::HistoryError;
use layerfs_overlay::{KernelCookieOffer, KernelMount, Publication};
use layerfs_workspace::{
    BaseView, CanonicalClient, JobOutcome, KernelDirectoryWindow, KernelInput,
    KernelMutationOutcome, KernelReadOperation, KernelReadOutcome, KernelVisitRequest,
    KernelWindow, Recognition, RecognizeWork, VisitFacts, WorkspaceError, WriteData,
    RECOGNIZE_BYTES,
};
use std::sync::Arc;

/// One request's operation scope and its mount's fence.
///
/// The fence is consulted only before an attempt, at three kinds of point: on
/// entry to and on every poll of the wait for owner admission; on entry to and
/// on every poll of the wait for a Store reader; and on entry to the one
/// synchronous acquiring call, `reserve_serial`. A stopped fence there returns
/// `Fenced` and drops the unattempted wait. A job that was admitted is always
/// awaited to its original result, and the disposal calls never consult the
/// fence: a stopped mount starts nothing new and still gives back what it
/// holds.
struct FilesystemPort(StoreOperation, Fence);
/// A bound Workspace as one kernel mount's services.
pub struct Mounted(pub Arc<BoundWorkspace>);
impl MountServices for Mounted {
    fn request(&self, fence: &Fence) -> Result<Arc<dyn RequestServices>, ServiceError> {
        request(&self.0, fence)
    }
}
/// One kernel request's services over `workspace`, under `fence`.
pub fn request(
    workspace: &BoundWorkspace,
    fence: &Fence,
) -> Result<Arc<dyn RequestServices>, ServiceError> {
    Ok(Arc::new(FilesystemPort(
        workspace.operation()?,
        fence.clone(),
    )))
}
impl FilesystemPort {
    fn fenced(&self, gated: bool) -> Result<(), ServiceError> {
        if gated && self.1.stopped() {
            Err(Box::new(Fenced))
        } else {
            Ok(())
        }
    }
    /// The Store's own answer to this request's read: a provider read
    /// failure, or no reader left to admit it. A poisoned lock, a table
    /// bound, a concurrent demand and the History allocator's write are not
    /// base reads of one request and stay as they are.
    fn scoped(cause: &PortError) -> bool {
        matches!(
            cause,
            PortError::Storage(_)
                | PortError::ReadAdmission(
                    ReadAdmissionError::NoReaders | ReadAdmissionError::Stopped
                )
        )
    }
    /// The original cause of a failed Store read made for this request:
    /// recorded in the mount's bounded slots and returned as the marker that
    /// ends this request alone. Any other cause is returned unchanged.
    fn demand(&self, cause: Arc<PortError>) -> ServiceError {
        if !Self::scoped(&cause) {
            return Box::new(cause);
        }
        let marker: BaseDemandFailed = self.1.failed_demand(cause);
        Box::new(marker)
    }
    fn call<T: Send + 'static>(
        &self,
        command: impl FnOnce(&crate::OwnerClient, layerfs_workspace::Route) -> Result<T, OwnerError>
            + Send
            + 'static,
        gated: bool,
    ) -> ServiceFuture<'_, ServiceReply<T>> {
        Box::pin(async move {
            self.fenced(gated)?;
            let client = self.0.overlay();
            let value = command(client, self.0.workspace().route())
                .map_err(|cause| Box::new(cause) as ServiceError)?;
            // Keep the Workspace backing alive through the reply attempt.
            Ok(ServiceReply::new(value, client.clone()))
        })
    }
    /// Provider calls lock only this Workspace's already opened reader.
    fn base_client(&self) -> Result<Arc<CanonicalClient>, ServiceError> {
        self.fenced(true)?;
        Ok(self.0.client_handle())
    }
}
impl RequestServices for FilesystemPort {
    fn directory_visit(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
        offset: u64,
        after: Option<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelDirectoryWindow>>> {
        // A stopped mount refuses before anything about the request is read.
        if let Err(fenced) = self.fenced(true) {
            return Box::pin(async move { Err(fenced) });
        }
        let visit = self.0.workspace().kernel_directory_visit(
            self.0.resident(),
            mount,
            serial,
            handle,
            offset,
            after,
        );
        match visit {
            Ok(visit) => self.call(
                move |job_client: &crate::OwnerClient, route| {
                    job_client.kernel_directory_visit(route, visit)
                },
                true,
            ),
            Err(error) => Box::pin(async move { Err(Box::new(error) as ServiceError) }),
        }
    }
    fn publish_cookies(
        &self,
        offer: KernelCookieOffer,
        names: Vec<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.call(
            move |job_client: &crate::OwnerClient, route| {
                job_client.kernel_directory_publish(route, offer, names)
            },
            true,
        )
    }
    fn close_directory(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.call(
            move |job_client: &crate::OwnerClient, route| {
                job_client.kernel_directory_close(route, mount, serial, handle)
            },
            false,
        )
    }
    fn read_visit(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: Option<u64>,
        offset: u64,
        length: u32,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelWindow>>> {
        // A stopped mount refuses before anything about the request is read.
        if let Err(fenced) = self.fenced(true) {
            return Box::pin(async move { Err(fenced) });
        }
        let visit = self.0.workspace().kernel_data_visit(
            self.0.resident(),
            mount,
            serial,
            handle,
            offset,
            length,
        );
        match visit {
            Ok(visit) => self.call(
                move |job_client: &crate::OwnerClient, route| {
                    job_client.kernel_read_visit(route, Box::new(visit))
                },
                true,
            ),
            Err(error) => Box::pin(async move { Err(Box::new(error) as ServiceError) }),
        }
    }
    fn close_file(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.call(
            move |job_client: &crate::OwnerClient, route| {
                job_client.kernel_close_file(route, mount, serial, handle)
            },
            false,
        )
    }
    fn provider_failure(&self) -> Result<Option<ServiceError>, ServiceError> {
        Ok(self
            .0
            .ports()
            .failure()?
            .map(|error| Box::new(error) as ServiceError))
    }
    fn base(&self) -> ServiceFuture<'_, BaseView> {
        Box::pin(async move {
            let client = self.base_client()?;
            let base = self
                .0
                .workspace()
                .base()
                .map_err(|cause| -> ServiceError { Box::new(cause) })?;
            Ok(base.with_client(client))
        })
    }
    fn failed_base_read(&self, step: ServiceError) -> ServiceError {
        // The canonical read ran in Fuse on the admitted view; whether the
        // provider failed is known only to this request's demand scope.
        match self.0.ports().failure() {
            Ok(Some(cause)) if Self::scoped(&cause) => self.demand(cause),
            _ => step,
        }
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
        // A stopped mount refuses before anything about the request is read.
        if let Err(fenced) = self.fenced(true) {
            return Box::pin(async move { Err(fenced) });
        }
        let (workspace, resident) = (self.0.workspace(), self.0.resident());
        let visit = match operation {
            KernelReadOperation::Open { writable, .. } => {
                workspace.kernel_open_visit(resident, mount, request, serial, writable, facts)
            }
            KernelReadOperation::Opendir { .. } => {
                workspace.kernel_opendir_visit(resident, mount, request, serial, facts)
            }
            operation => {
                workspace.kernel_read_visit(resident, mount, serial, handle, operation, facts)
            }
        };
        match visit {
            Ok(visit) => self.call(
                move |job_client: &crate::OwnerClient, route| {
                    job_client.kernel_observe_visit(route, Box::new(visit))
                },
                true,
            ),
            Err(error) => Box::pin(async move { Err(Box::new(error) as ServiceError) }),
        }
    }
    fn mutate_visit(
        &self,
        request: KernelVisitRequest,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelMutationOutcome>>> {
        if let Err(fenced) = self.fenced(true) {
            return Box::pin(async move { Err(fenced) });
        }
        // The bytes of an examined write, for the chunk it may have carried.
        let written = match &request.input {
            KernelInput::Write {
                offset,
                data,
                cached: false,
            } if data.len() >= RECOGNIZE_BYTES => {
                Some((request.serial, (*offset, offset + data.len() as u64)))
            }
            _ => None,
        };
        let visit = self
            .0
            .workspace()
            .kernel_mutation_visit(self.0.resident(), request);
        let job = match visit {
            Ok(visit) => self.call(
                move |job_client: &crate::OwnerClient, route| {
                    job_client.kernel_mutate_visit(route, Box::new(visit))
                },
                true,
            ),
            Err(error) => return Box::pin(async move { Err(Box::new(error) as ServiceError) }),
        };
        let Some((serial, range)) = written else {
            return job;
        };
        Box::pin(async move {
            let reply = job.await?;
            if let Ok(JobOutcome::Applied { publication, .. }) = &reply.get().result {
                let route = self.0.workspace().route();
                let slots = self.0.ports().recognizer();
                slots.published(route, serial, range, publication.revision());
            }
            Ok(reply)
        })
    }
    /// No SQL and no owner turn: the write's bytes, mapping pages of the
    /// file's committed content and one slot of the Store's fixed table.
    fn recognize(&self, serial: u64, offset: u64, data: WriteData) -> ServiceFuture<'_, WriteData> {
        Box::pin(async move {
            if data.len() < RECOGNIZE_BYTES {
                return Ok(data);
            }
            self.fenced(true)?;
            let Ok(base) = self.0.workspace().base() else {
                return Ok(data);
            };
            let (slots, route) = (self.0.ports().recognizer(), self.0.workspace().route());
            let (mut run, mut work) = (slots.take(route, serial), RecognizeWork::default());
            if !run.wants(&base, data.len()) {
                slots.put(route, serial, run, &work);
                return Ok(data);
            }
            // Hashing and page reads never run on the loop that received
            // the request.
            let resident = base.with_client(self.0.resident());
            let mut outcome = run.recognize(&resident, serial, offset, &data, &mut work);
            if outcome == Recognition::NeedsBase {
                outcome = match self.fenced(true).map(|()| self.0.ports().fresh().client()) {
                    Ok(client) => {
                        let admitted = base.with_client(client);
                        run.recognize(&admitted, serial, offset, &data, &mut work)
                    }
                    Err(fenced) => {
                        slots.put(route, serial, run, &work);
                        return Err(fenced);
                    }
                };
            }
            slots.put(route, serial, run, &work);
            Ok(match outcome {
                Recognition::Decided(Some(claims)) => data.claimed(claims),
                _ => data,
            })
        })
    }
    fn forget(
        &self,
        mount: KernelMount,
        serial: u64,
        count: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.call(
            move |job_client: &crate::OwnerClient, route| {
                job_client.kernel_forget(route, mount, serial, count)
            },
            false,
        )
    }
    fn reserve_serial(&self) -> Result<Option<u64>, ServiceError> {
        self.fenced(true)?;
        match self.0.ports().take_serial(self.0.workspace()) {
            Ok((serial, None)) => Ok(Some(serial)),
            // A refused early refill reserved nothing and is not asked
            // again: this create keeps the serial it took.
            Ok((serial, Some(early))) if writer_contended(&early) => Ok(Some(serial)),
            // Any other early failure is this create's original error; its
            // serial is spent. As on the exhausted path, the request service
            // retains it and the mount then serves nothing further.
            Ok((_, Some(early))) => Err(Box::new(early)),
            Err(error) if writer_contended(&error) => Ok(None),
            Err(error) => Err(Box::new(error)),
        }
    }
    /// The attempt is one short job of this thread with no SQL; the last
    /// attempt wakes a Capture waiting on the same Workspace lock.
    fn reply_attempted(&self, publication: Publication) -> ServiceFuture<'_, ServiceReply<()>> {
        let settled = if publication.engine() == self.0.workspace().route().engine() {
            self.0.overlay().reply_attempted(publication)
        } else {
            Err(OwnerError::Overlay(layerfs_overlay::OverlayError::Stale))
        };
        Box::pin(std::future::ready(match settled {
            Ok(()) => Ok(ServiceReply::new((), self.0.overlay().clone())),
            Err(error) => Err(Box::new(error) as ServiceError),
        }))
    }
}
/// An exact SQLite `Busy` at the refill's one BEGIN: nothing was reserved.
/// Admission itself queues the refill with no time limit (#352 F-040); only
/// a writer outside the cooperative lock can still refuse it this way.
fn writer_contended(error: &WorkspaceError) -> bool {
    let WorkspaceError::Service(error) = error else {
        return false;
    };
    matches!(
        error
            .downcast_ref::<Arc<crate::store::PortError>>()
            .map(Arc::as_ref),
        Some(crate::store::PortError::History(HistoryError::Busy))
    )
}
