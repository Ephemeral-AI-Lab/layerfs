//! Async Fuse ports over the existing fair SQL owner and direct Store readers.
use crate::{
    store::{BoundWorkspace, StoreOperation},
    Command, Completion, NativeJob, NativeReply, OwnerError, Response,
};
use layerfs_fuse::ports::{
    MountServices, RequestServices, ServiceError, ServiceFuture, ServiceReply,
};
use layerfs_overlay::{BaseSource, FileRead, LocalRead, NativeMount};
use layerfs_workspace::{NativeReadJob, NativeReadOutcome, SourceView};
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
            Some(handle) => NativeJob::FileSource {
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
}
fn done(response: &Response) -> Option<()> {
    matches!(response, Response::Done).then_some(())
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
