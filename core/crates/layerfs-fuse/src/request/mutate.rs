//! Mutating callbacks: owned bounded input, one decided plan, one reply attempt.
use super::{
    accounting::Opcode,
    failure::{failed, KernelInput},
    reply::{open_flags, TTL},
    state::NativeFilesystem,
};
use crate::{
    attributes::{declined, Identity},
    coherence::{attributes::published, pages, reply_order},
    operations::{
        write::time, Declined, MutationInput, MutationRequest, NativeMutation, Published,
    },
    ports::{RequestServices, ServiceError},
    Permit, RequestDisposition,
};
use fuser::{
    Errno, FileAttr, FileHandle, Generation, INodeNo, ReplyAttr, ReplyCreate, ReplyEmpty,
    ReplyEntry, ReplyWrite,
};
use layerfs_content::filesystem::PathName;
use layerfs_overlay::NativeMount;
use layerfs_workspace::{Refusal, Time};
use std::{ffi::OsStr, io, os::unix::ffi::OsStrExt, sync::Arc, time::SystemTime};

pub(super) enum MutationReply {
    /// MKNOD, MKDIR, SYMLINK and LINK; LINK names the serial it must bind.
    Entry(ReplyEntry, Option<u64>),
    Created(ReplyCreate),
    /// SETATTR of this serial.
    Attr(ReplyAttr, u64),
    /// WRITE of this many request bytes.
    Written(ReplyWrite, usize),
    Done(ReplyEmpty),
}
/// The values one successful reply carries, composed before it is consumed.
enum Composed {
    Nothing,
    Count(u32),
    Attributes(FileAttr),
    Opened(FileAttr, u64),
}
/// The builder runs at processing with the time assigned there.
pub(super) type Build = Box<dyn FnOnce(Time) -> Result<MutationInput, Declined> + Send>;

impl MutationReply {
    pub fn error(self, error: Errno) {
        match self {
            Self::Entry(reply, _) => reply.error(error),
            Self::Created(reply) => reply.error(error),
            Self::Attr(reply, _) => reply.error(error),
            Self::Written(reply, _) => reply.error(error),
            Self::Done(reply) => reply.error(error),
        }
    }
    /// Every value comes from the publishing job's own result. A reply that
    /// cannot be composed from it is an error for the caller and a retained
    /// request; nothing is synthesized from the request's inputs.
    fn compose(&self, value: &Published, identity: Identity) -> Result<Composed, ServiceError> {
        let attributes = |expected| -> Result<FileAttr, ServiceError> {
            let stat = published(value, expected)?;
            identity
                .attributes(stat)
                .map_err(|errno| io::Error::from_raw_os_error(errno.code()).into())
        };
        Ok(match self {
            Self::Done(_) => Composed::Nothing,
            Self::Written(_, requested) => Composed::Count(
                pages::written(*requested).map_err(|_| io::Error::other("native write count"))?,
            ),
            Self::Attr(_, serial) => Composed::Attributes(attributes(Some(*serial))?),
            Self::Entry(_, expected) => Composed::Attributes(attributes(*expected)?),
            Self::Created(_) => {
                let handle = value
                    .file
                    .ok_or_else(|| io::Error::other("created file has no descriptor"))?;
                Composed::Opened(attributes(Some(handle.serial()))?, handle.owner_id())
            }
        })
    }
    async fn serve(
        self,
        services: Arc<dyn RequestServices>,
        request: MutationRequest,
        identity: Identity,
    ) -> RequestDisposition {
        let generation = Generation(request.mount.owner_id());
        let mutation = match NativeMutation::perform(services, request).await {
            Ok(mutation) => mutation,
            Err(failure) => {
                self.error(Errno::EIO);
                return reply_order::retained(failure);
            }
        };
        let composed = match mutation.value() {
            Ok(value) => self.compose(value, identity),
            Err(reason) => {
                // Nothing was published: no ticket, only the source to release.
                self.error(declined(reason));
                return reply_order::attempted(mutation).await;
            }
        };
        match (self, composed) {
            (reply, Err(reason)) => {
                reply.error(Errno::EIO);
                return reply_order::retained(mutation.retain(reason));
            }
            (Self::Done(reply), Ok(_)) => reply.ok(),
            (Self::Written(reply, _), Ok(Composed::Count(count))) => reply.written(count),
            (Self::Attr(reply, _), Ok(Composed::Attributes(attributes))) => {
                reply.attr(&TTL, &attributes)
            }
            (Self::Entry(reply, _), Ok(Composed::Attributes(attributes))) => {
                reply.entry(&TTL, &attributes, generation)
            }
            (Self::Created(reply), Ok(Composed::Opened(attributes, handle))) => reply.created(
                &TTL,
                &attributes,
                generation,
                FileHandle(handle),
                open_flags(),
            ),
            (reply, Ok(_)) => {
                reply.error(Errno::EIO);
                return reply_order::retained(
                    mutation.retain(io::Error::other("native reply composition").into()),
                );
            }
        }
        reply_order::attempted(mutation).await
    }
}
impl NativeFilesystem {
    /// Hands one admitted mutation to the shared dispatcher. Its input is
    /// built at processing, so every stored time is assigned there.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn mutate(
        &self,
        permit: Permit,
        request: u64,
        protected: u64,
        handle: Option<u64>,
        open: Option<bool>,
        reply: MutationReply,
        build: Build,
    ) {
        let services = self.services.clone();
        let mount: NativeMount = self.queue.identity();
        let identity = self.identity;
        self.handoff(
            permit,
            Box::pin(async move {
                let Some(now) = time(SystemTime::now()) else {
                    reply.error(Errno::EIO);
                    return RequestDisposition::Complete;
                };
                let input = match build(now) {
                    Ok(input) => input,
                    Err(reason) => {
                        reply.error(declined(reason));
                        return RequestDisposition::Complete;
                    }
                };
                let services = match services.request() {
                    Ok(services) => services,
                    Err(error) => {
                        reply.error(Errno::EIO);
                        return failed(
                            request,
                            mount,
                            KernelInput::Mutation {
                                protected,
                                handle,
                                input,
                            },
                            error,
                        );
                    }
                };
                let request = MutationRequest {
                    mount,
                    request,
                    protected,
                    handle,
                    input,
                    open,
                    now,
                };
                reply.serve(services, request, identity).await
            }),
        );
    }
    /// A mutation addressed by a parent and one or two names. Names are
    /// checked and copied on the receive loop; `build` receives them owned.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn named<const N: usize>(
        &self,
        opcode: Opcode,
        request: u64,
        parent: INodeNo,
        names: [&OsStr; N],
        extra: usize,
        open: Option<bool>,
        reply: MutationReply,
        build: impl FnOnce(u64, [PathName; N], Time) -> Result<MutationInput, Declined> + Send + 'static,
    ) {
        if names.iter().any(|name| name.as_bytes().len() > 255) {
            return self.refuse(opcode, reply, MutationReply::error, Errno::ENAMETOOLONG);
        }
        let bytes = extra + names.iter().map(|name| name.len()).sum::<usize>();
        let Some((permit, reply)) = self.admit_reply(opcode, bytes, reply, MutationReply::error)
        else {
            return;
        };
        let Some((parent, reply)) = self.serial(parent, reply, MutationReply::error) else {
            return;
        };
        let mut owned = Vec::with_capacity(N);
        for name in names {
            match PathName::from_bytes(name.as_bytes()) {
                Ok(name) => owned.push(name),
                Err(_) => return self.refused(reply, MutationReply::error, Errno::EINVAL),
            }
        }
        let Ok(owned) = <[PathName; N]>::try_from(owned) else {
            return self.refused(reply, MutationReply::error, Errno::EINVAL);
        };
        self.mutate(
            permit,
            request,
            parent,
            None,
            open,
            reply,
            Box::new(move |now| build(parent, owned, now)),
        );
    }
    /// A second inode number of the same request, such as a rename's
    /// destination parent, as a definite refusal when it denotes nothing.
    pub(super) fn other(identity: Identity, inode: INodeNo) -> Result<u64, Declined> {
        identity
            .serial(inode)
            .map_err(|_| Declined::Refused(Refusal::Missing))
    }
}
