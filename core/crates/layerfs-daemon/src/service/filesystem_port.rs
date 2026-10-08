//! Async Fuse ports over the existing fair SQL owner and direct Store readers.
use crate::{
    store::{BoundWorkspace, PortError, StoreOperation},
    Command, Completion, NativeDirectoryJob, NativeDirectoryReply, NativeJob, NativeReply,
    OwnerError, Response,
};
use layerfs_fuse::ports::{
    BaseDemandFailed, Fence, Fenced, MountServices, RequestServices, ServiceError, ServiceFuture,
    ServiceReply,
};
use layerfs_history::HistoryError;
use layerfs_overlay::{
    BaseSource, FileRead, LocalRead, NativeCookiePlan, NativeDirectory, NativeDirectoryPage,
    NativeDirectoryRead, NativeMount, OpenFile, Publication,
};
use layerfs_workspace::{
    MutationInputFailure, MutationPlan, NativeMutationJob, NativeMutationOutcome, NativeReadJob,
    NativeReadOutcome, Operation, SourceView, Time, WorkspaceError,
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
    /// The original cause of a failed Store read made for this request:
    /// recorded in the mount's bounded slot and returned as the marker that
    /// ends this request alone. A serial reservation is the History
    /// allocator's write, not a base read; its failure is left as it is.
    fn demand(&self, cause: Arc<PortError>) -> ServiceError {
        if matches!(cause.as_ref(), PortError::History(_)) {
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
    fn directory(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<NativeDirectory>> {
        self.acquire(
            Command::Native(NativeJob::Directory(Box::new(NativeDirectoryJob::Handle {
                mount,
                serial,
                handle,
            }))),
            |response| match response {
                Response::Native(NativeReply::Directory(NativeDirectoryReply::Handle(value))) => {
                    Some(*value)
                }
                _ => None,
            },
        )
    }
    fn directory_read(
        &self,
        directory: NativeDirectory,
        request: u64,
        offset: u64,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeDirectoryRead>>> {
        self.acquire(
            Command::Native(NativeJob::Directory(Box::new(NativeDirectoryJob::Read {
                directory,
                request,
                offset,
            }))),
            |response| match response {
                Response::Native(NativeReply::Directory(NativeDirectoryReply::Read(value))) => {
                    Some(value.clone())
                }
                _ => None,
            },
        )
    }
    fn directory_page(
        &self,
        read: Arc<NativeDirectoryRead>,
        after: Option<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeDirectoryPage>>> {
        self.acquire(
            Command::Native(NativeJob::Directory(Box::new(NativeDirectoryJob::Page {
                read,
                after,
            }))),
            |response| match response {
                Response::Native(NativeReply::Directory(NativeDirectoryReply::Page(value))) => {
                    Some(value.clone())
                }
                _ => None,
            },
        )
    }
    fn directory_cookies(
        &self,
        read: Arc<NativeDirectoryRead>,
        names: Vec<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeCookiePlan>>> {
        self.acquire(
            Command::Native(NativeJob::Directory(Box::new(
                NativeDirectoryJob::PrepareCookies { read, names },
            ))),
            |response| match response {
                Response::Native(NativeReply::Directory(NativeDirectoryReply::Cookies(value))) => {
                    Some(value.clone())
                }
                _ => None,
            },
        )
    }
    fn publish_cookies(
        &self,
        plan: Arc<NativeCookiePlan>,
        accepted: usize,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.acquire(
            Command::Native(NativeJob::Directory(Box::new(
                NativeDirectoryJob::PublishCookies { plan, accepted },
            ))),
            directory_done,
        )
    }
    fn close_directory(&self, directory: NativeDirectory) -> ServiceFuture<'_, ServiceReply<()>> {
        self.dispose(
            Command::Native(NativeJob::Directory(Box::new(NativeDirectoryJob::Close(
                directory,
            )))),
            directory_done,
        )
    }
    fn local_read(
        &self,
        read: FileRead,
        offset: u64,
        length: u32,
    ) -> ServiceFuture<'_, ServiceReply<Option<LocalRead>>> {
        Box::pin(async move {
            let original = self
                .complete(
                    Command::FileRead {
                        read,
                        offset,
                        length,
                    },
                    true,
                )
                .await?;
            if matches!(original.result(), Ok(Response::Read(_))) {
                Ok(ServiceReply::borrowed(LocalWindow(original)))
            } else {
                Err(Box::new(original) as ServiceError)
            }
        })
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
    fn source(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: Option<u64>,
    ) -> ServiceFuture<'_, ServiceReply<BaseSource>> {
        let command = match handle {
            Some(handle) => NativeJob::HandleSource {
                mount,
                request,
                serial,
                handle,
            },
            None => NativeJob::Source {
                mount,
                request,
                serial,
            },
        };
        self.acquire(Command::Native(command), |response| match response {
            Response::Native(NativeReply::Source(source)) => Some(*source),
            _ => None,
        })
    }
    fn view(&self, source: BaseSource) -> Result<SourceView, ServiceError> {
        Ok(self.0.workspace().view_for_source(source)?)
    }
    fn immutable<'a>(&'a self, view: &'a SourceView) -> ServiceFuture<'a, SourceView> {
        Box::pin(async move {
            self.fenced(true)?;
            // A reader that cannot be had is a failed base demand of this
            // request: nothing was read and nothing is asked again.
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
            let client = self
                .0
                .admitted_client(lease)
                .map_err(|cause| self.demand(cause))?;
            Ok(view.with_client(client))
        })
    }
    fn failed_base_read(&self, step: ServiceError) -> ServiceError {
        // The canonical read ran in Fuse on the admitted view; whether the
        // provider failed is known only to this request's demand scope.
        match self.0.ports().failure() {
            Ok(Some(cause)) => self.demand(cause),
            Ok(None) | Err(_) => step,
        }
    }
    fn observe(
        &self,
        job: NativeReadJob,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeReadOutcome>>> {
        self.acquire(
            Command::Native(NativeJob::Observe(Box::new(job))),
            |response| match response {
                Response::Native(NativeReply::Observed(value)) => Some(value.clone()),
                _ => None,
            },
        )
    }
    fn release_read(&self, read: FileRead) -> ServiceFuture<'_, ServiceReply<()>> {
        self.dispose(Command::ReleaseFileRead(read), done)
    }
    fn release_source(&self, source: BaseSource) -> ServiceFuture<'_, ServiceReply<()>> {
        self.dispose(Command::ReleaseBaseSource(source), done)
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
    fn open_source(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<(BaseSource, OpenFile)>> {
        self.acquire(
            Command::Native(NativeJob::OpenSource {
                mount,
                request,
                serial,
                handle,
            }),
            |response| match response {
                Response::Native(NativeReply::OpenSource(source, file)) => Some((*source, *file)),
                _ => None,
            },
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
    fn prepare(
        &self,
        view: &SourceView,
        operation: Operation,
        now: Time,
        serial: Option<u64>,
    ) -> Result<MutationPlan, Box<MutationInputFailure>> {
        self.0
            .workspace()
            .prepare_mutation(view, operation, now, serial)
    }
    fn mutate(
        &self,
        job: NativeMutationJob,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeMutationOutcome>>> {
        self.acquire(
            Command::Native(NativeJob::Mutate(Box::new(job))),
            |response| match response {
                Response::Native(NativeReply::Mutated(value)) => Some(value.clone()),
                _ => None,
            },
        )
    }
    fn reply_attempted(&self, publication: Publication) -> ServiceFuture<'_, ServiceReply<()>> {
        self.dispose(Command::ReplyAttempted(publication), done)
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

/// The local payload stays in the same original Completion and credit cell.
struct LocalWindow(Completion);
impl AsRef<Option<LocalRead>> for LocalWindow {
    fn as_ref(&self) -> &Option<LocalRead> {
        match self.0.result() {
            Ok(Response::Read(value)) => value,
            _ => unreachable!("LocalWindow follows its checked immutable completion"),
        }
    }
}
