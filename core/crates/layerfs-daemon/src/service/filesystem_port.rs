//! Async Fuse ports over the existing fair SQL owner and direct Store readers.
use crate::{
    store::{BoundWorkspace, PortError, ReadAdmissionError, StoreOperation},
    Command, Completion, NativeDirectoryJob, NativeDirectoryReply, NativeJob, NativeReply,
    OwnerError, Response,
};
use layerfs_fuse::ports::{
    BaseDemandFailed, Fence, Fenced, MountServices, RequestServices, ServiceError, ServiceFuture,
    ServiceReply,
};
use layerfs_history::HistoryError;
use layerfs_overlay::{NativeCookieOffer, NativeMount, Publication};
use layerfs_workspace::{
    BaseView, NativeDirectoryWindow, NativeMutationOutcome, NativeReadOperation, NativeReadOutcome,
    NativeVisitRequest, NativeWindow, VisitFacts, WorkspaceError,
};
use std::{
    future::{poll_fn, Future},
    pin::Pin,
    sync::Arc,
    task::Poll,
};

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
impl MountServices for BoundWorkspace {
    fn request(&self, fence: &Fence) -> Result<Arc<dyn RequestServices>, ServiceError> {
        Ok(Arc::new(FilesystemPort(self.operation()?, fence.clone())))
    }
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
    fn complete(&self, command: Command, gated: bool) -> ServiceFuture<'_, Completion> {
        Box::pin(async move {
            let unattempted = |(cause, command)| -> ServiceError {
                Box::new(OwnerError::Unattempted {
                    cause: Box::new(cause),
                    command: Box::new(command),
                })
            };
            self.fenced(gated)?;
            let pending = {
                let mut admission = self
                    .0
                    .overlay()
                    .submit_when_available(Some(self.0.workspace().route()), command)
                    .map_err(unattempted)?;
                // Leaving this block on a stop drops the admission and with
                // it the command, which was never submitted.
                poll_fn(|cx| match self.fenced(gated) {
                    Ok(()) => Pin::new(&mut admission).poll(cx).map_err(unattempted),
                    Err(fenced) => Poll::Ready(Err(fenced)),
                })
                .await?
            };
            Ok(pending.await?)
        })
    }
    fn job<T: Send + 'static>(
        &self,
        command: Command,
        gated: bool,
        project: fn(&Response) -> Option<T>,
    ) -> ServiceFuture<'_, ServiceReply<T>> {
        Box::pin(async move {
            let original = self.complete(command, gated).await?;
            let value = original.result().as_ref().ok().and_then(project);
            match value {
                Some(value) => Ok(ServiceReply::new(value, original)),
                None => Err(Box::new(original) as ServiceError),
            }
        })
    }
    /// One admitted Store reader as this request's canonical client. A reader
    /// that cannot be had is a failed base demand of this request: nothing
    /// was read and nothing is asked again.
    async fn admitted(&self) -> Result<Arc<layerfs_workspace::CanonicalClient>, ServiceError> {
        self.fenced(true)?;
        let lease = {
            let mut ticket = self
                .0
                .ports()
                .read_ticket()
                .map_err(|cause| self.demand(cause))?;
            // Leaving this block on a stop cancels the unstarted ticket.
            poll_fn(|cx| match self.fenced(true) {
                Ok(()) => Pin::new(&mut ticket)
                    .poll(cx)
                    .map_err(|error| self.demand(Arc::new(PortError::ReadAdmission(error)))),
                Err(fenced) => Poll::Ready(Err(fenced)),
            })
            .await?
        };
        // A reader of another Store or Workspace is a wiring failure,
        // not this request's base demand.
        self.0
            .admitted_client(lease)
            .map_err(|cause| -> ServiceError { Box::new(cause) })
    }
    /// A call that acquires: refused by a stopped fence before its attempt.
    fn acquire<T: Send + 'static>(
        &self,
        command: Command,
        project: fn(&Response) -> Option<T>,
    ) -> ServiceFuture<'_, ServiceReply<T>> {
        self.job(command, true, project)
    }
    /// A call that only gives back: never refused by the fence.
    fn dispose<T: Send + 'static>(
        &self,
        command: Command,
        project: fn(&Response) -> Option<T>,
    ) -> ServiceFuture<'_, ServiceReply<T>> {
        self.job(command, false, project)
    }
}
impl RequestServices for FilesystemPort {
    fn directory_visit(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
        offset: u64,
        after: Option<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeDirectoryWindow>>> {
        // A stopped mount refuses before anything about the request is read.
        if let Err(fenced) = self.fenced(true) {
            return Box::pin(async move { Err(fenced) });
        }
        let visit = self.0.workspace().native_directory_visit(
            self.0.resident(),
            mount,
            serial,
            handle,
            offset,
            after,
        );
        match visit {
            Ok(visit) => self.acquire(
                Command::Native(NativeJob::Directory(Box::new(NativeDirectoryJob::Visit(
                    visit,
                )))),
                |response| match response {
                    Response::Native(NativeReply::Directory(NativeDirectoryReply::Window(
                        value,
                    ))) => Some(value.clone()),
                    _ => None,
                },
            ),
            Err(error) => Box::pin(async move { Err(Box::new(error) as ServiceError) }),
        }
    }
    fn publish_cookies(
        &self,
        offer: NativeCookieOffer,
        names: Vec<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.acquire(
            Command::Native(NativeJob::Directory(Box::new(
                NativeDirectoryJob::Publish { offer, names },
            ))),
            directory_done,
        )
    }
    fn close_directory(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.dispose(
            Command::Native(NativeJob::Directory(Box::new(NativeDirectoryJob::Close {
                mount,
                serial,
                handle,
            }))),
            directory_done,
        )
    }
    fn read_visit(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: Option<u64>,
        offset: u64,
        length: u32,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeWindow>>> {
        // A stopped mount refuses before anything about the request is read.
        if let Err(fenced) = self.fenced(true) {
            return Box::pin(async move { Err(fenced) });
        }
        let visit = self.0.workspace().native_data_visit(
            self.0.resident(),
            mount,
            serial,
            handle,
            offset,
            length,
        );
        match visit {
            Ok(visit) => self.acquire(
                Command::Native(NativeJob::ReadVisit(Box::new(visit))),
                |response| match response {
                    Response::Native(NativeReply::Window(value)) => Some(value.clone()),
                    _ => None,
                },
            ),
            Err(error) => Box::pin(async move { Err(Box::new(error) as ServiceError) }),
        }
    }
    fn close_file(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.dispose(
            Command::Native(NativeJob::CloseFile {
                mount,
                serial,
                handle,
            }),
            |response| matches!(response, Response::Native(NativeReply::Done)).then_some(()),
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
            let client = self.admitted().await?;
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
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: Option<u64>,
        operation: NativeReadOperation,
        facts: Arc<VisitFacts>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeReadOutcome>>> {
        // A stopped mount refuses before anything about the request is read.
        if let Err(fenced) = self.fenced(true) {
            return Box::pin(async move { Err(fenced) });
        }
        let (workspace, resident) = (self.0.workspace(), self.0.resident());
        let visit = match operation {
            NativeReadOperation::Open { writable, .. } => {
                workspace.native_open_visit(resident, mount, request, serial, writable, facts)
            }
            NativeReadOperation::Opendir { .. } => {
                workspace.native_opendir_visit(resident, mount, request, serial, facts)
            }
            operation => {
                workspace.native_read_visit(resident, mount, serial, handle, operation, facts)
            }
        };
        match visit {
            Ok(visit) => self.acquire(
                Command::Native(NativeJob::ObserveVisit(Box::new(visit))),
                |response| match response {
                    Response::Native(NativeReply::Observed(value)) => Some(value.clone()),
                    _ => None,
                },
            ),
            Err(error) => Box::pin(async move { Err(Box::new(error) as ServiceError) }),
        }
    }
    fn mutate_visit(
        &self,
        request: NativeVisitRequest,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeMutationOutcome>>> {
        if let Err(fenced) = self.fenced(true) {
            return Box::pin(async move { Err(fenced) });
        }
        let visit = self
            .0
            .workspace()
            .native_mutation_visit(self.0.resident(), request);
        match visit {
            Ok(visit) => self.acquire(
                Command::Native(NativeJob::MutateVisit(Box::new(visit))),
                |response| match response {
                    Response::Native(NativeReply::Mutated(value)) => Some(value.clone()),
                    _ => None,
                },
            ),
            Err(error) => Box::pin(async move { Err(Box::new(error) as ServiceError) }),
        }
    }
    fn forget(
        &self,
        mount: NativeMount,
        serial: u64,
        count: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.dispose(
            Command::Native(NativeJob::Forget {
                mount,
                serial,
                count,
            }),
            |response| matches!(response, Response::Native(NativeReply::Done)).then_some(()),
        )
    }
    fn reserve_serial(&self) -> Result<Option<u64>, ServiceError> {
        self.fenced(true)?;
        match self.0.workspace().next_serial(self.0.ports()) {
            Ok(serial) => Ok(Some(serial)),
            Err(error) if writer_contended(&error) => Ok(None),
            Err(error) => Err(Box::new(error)),
        }
    }
    /// The attempt is recorded from this thread, with no owner turn. Only
    /// the last attempt of a Workspace that an owner job waits for owes one.
    fn reply_attempted(&self, publication: Publication) -> ServiceFuture<'_, ServiceReply<()>> {
        let settled = if publication.route() == self.0.workspace().route() {
            self.0.overlay().reply_attempted(publication)
        } else {
            Err(OwnerError::Overlay(layerfs_overlay::OverlayError::Stale))
        };
        match settled {
            Ok(layerfs_overlay::Settled::Watched) => self.dispose(Command::ReplySettled, done),
            Ok(_) => Box::pin(std::future::ready(Ok(ServiceReply::new((), ())))),
            Err(error) => Box::pin(std::future::ready(Err(Box::new(error) as ServiceError))),
        }
    }
}
/// The allocator's immediate admission refusal: nothing was reserved.
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
fn done(response: &Response) -> Option<()> {
    matches!(response, Response::Done).then_some(())
}
fn directory_done(response: &Response) -> Option<()> {
    matches!(
        response,
        Response::Native(NativeReply::Directory(NativeDirectoryReply::Done))
    )
    .then_some(())
}
