//! Async Fuse ports over the existing fair SQL owner and direct Store readers.
use crate::{
    store::{BoundWorkspace, StoreOperation},
    Command, Completion, NativeDirectoryJob, NativeDirectoryReply, NativeJob, NativeReply,
    OwnerError, Response,
};
use layerfs_fuse::ports::{
    MountServices, RequestServices, ServiceError, ServiceFuture, ServiceReply,
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
use std::sync::Arc;

struct FilesystemPort(StoreOperation);
impl MountServices for BoundWorkspace {
    fn request(&self) -> Result<Arc<dyn RequestServices>, ServiceError> {
        Ok(Arc::new(FilesystemPort(self.operation()?)))
    }
}
impl FilesystemPort {
    fn complete(&self, command: Command) -> ServiceFuture<'_, Completion> {
        Box::pin(async move {
            let unattempted = |(cause, command)| -> ServiceError {
                Box::new(OwnerError::Unattempted {
                    cause: Box::new(cause),
                    command: Box::new(command),
                })
            };
            let admission = self
                .0
                .overlay()
                .submit_when_available(Some(self.0.workspace().route()), command)
                .map_err(unattempted)?;
            let pending = admission.await.map_err(unattempted)?;
            Ok(pending.await?)
        })
    }
    fn job<T: Send + 'static>(
        &self,
        command: Command,
        project: fn(&Response) -> Option<T>,
    ) -> ServiceFuture<'_, ServiceReply<T>> {
        Box::pin(async move {
            let original = self.complete(command).await?;
            let value = original.result().as_ref().ok().and_then(project);
            match value {
                Some(value) => Ok(ServiceReply::new(value, original)),
                None => Err(Box::new(original) as ServiceError),
            }
        })
    }
}
impl RequestServices for FilesystemPort {
    fn directory(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<NativeDirectory>> {
        self.job(
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
        self.job(
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
        self.job(
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
        self.job(
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
        self.job(
            Command::Native(NativeJob::Directory(Box::new(
                NativeDirectoryJob::PublishCookies { plan, accepted },
            ))),
            directory_done,
        )
    }
    fn close_directory(&self, directory: NativeDirectory) -> ServiceFuture<'_, ServiceReply<()>> {
        self.job(
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
                .complete(Command::FileRead {
                    read,
                    offset,
                    length,
                })
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
        self.job(
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
        self.job(Command::Native(command), |response| match response {
            Response::Native(NativeReply::Source(source)) => Some(*source),
            _ => None,
        })
    }
    fn view(&self, source: BaseSource) -> Result<SourceView, ServiceError> {
        Ok(self.0.workspace().view_for_source(source)?)
    }
    fn immutable<'a>(&'a self, view: &'a SourceView) -> ServiceFuture<'a, SourceView> {
        Box::pin(async move {
            let ticket = self.0.ports().read_ticket()?;
            let lease = ticket.await?;
            Ok(view.with_client(self.0.admitted_client(lease)?))
        })
    }
    fn observe(
        &self,
        job: NativeReadJob,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeReadOutcome>>> {
        self.job(
            Command::Native(NativeJob::Observe(Box::new(job))),
            |response| match response {
                Response::Native(NativeReply::Observed(value)) => Some(value.clone()),
                _ => None,
            },
        )
    }
    fn release_read(&self, read: FileRead) -> ServiceFuture<'_, ServiceReply<()>> {
        self.job(Command::ReleaseFileRead(read), done)
    }
    fn release_source(&self, source: BaseSource) -> ServiceFuture<'_, ServiceReply<()>> {
        self.job(Command::ReleaseBaseSource(source), done)
    }
    fn forget(
        &self,
        mount: NativeMount,
        serial: u64,
        count: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        self.job(
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
        self.job(
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
        self.job(
            Command::Native(NativeJob::Mutate(Box::new(job))),
            |response| match response {
                Response::Native(NativeReply::Mutated(value)) => Some(value.clone()),
                _ => None,
            },
        )
    }
    fn reply_attempted(&self, publication: Publication) -> ServiceFuture<'_, ServiceReply<()>> {
        self.job(Command::ReplyAttempted(publication), done)
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
