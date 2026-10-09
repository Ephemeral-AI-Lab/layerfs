//! Pinned fuser consumes replies once; send/delivery outcomes are unavailable.
use super::terminal;
use crate::{
    attributes::{refusal, Identity},
    operations::{NativeData, NativeRead, ReadDataInput},
    ports::{Fence, RequestServices},
    RequestDisposition,
};
use fuser::{
    Errno, FileHandle, FopenFlags, Generation, ReplyAttr, ReplyData, ReplyEntry, ReplyOpen,
};
use layerfs_overlay::NativeMount;
use layerfs_workspace::NativeReadOperation;
use std::{sync::Arc, time::Duration};
/// Entry and attribute lifetime of every reply. It bounds how long the kernel
/// may answer without asking; it is never what makes an answer correct.
pub(super) const TTL: Duration = Duration::from_secs(60);

pub(super) enum ReadReply {
    Entry(ReplyEntry),
    Attr(ReplyAttr),
    Open(ReplyOpen, bool),
    Data(ReplyData, u64, u32),
    Link(ReplyData),
}
impl ReadReply {
    pub fn data_input(&self) -> Option<ReadDataInput> {
        match self {
            Self::Data(_, offset, length) => Some(ReadDataInput::File {
                offset: *offset,
                length: *length,
            }),
            Self::Link(_) => Some(ReadDataInput::Link),
            _ => None,
        }
    }
    pub fn error(self, error: Errno) {
        match self {
            Self::Entry(reply) => reply.error(error),
            Self::Attr(reply) => reply.error(error),
            Self::Open(reply, _) => reply.error(error),
            Self::Data(reply, ..) | Self::Link(reply) => reply.error(error),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn serve(
        self,
        services: Arc<dyn RequestServices>,
        fence: Fence,
        mount: NativeMount,
        request: u64,
        protected: u64,
        handle: Option<u64>,
        operation: NativeReadOperation,
        identity: Identity,
    ) -> RequestDisposition {
        let (reply, input) = match self {
            Self::Data(reply, offset, length) => (reply, ReadDataInput::File { offset, length }),
            Self::Link(reply) => (reply, ReadDataInput::Link),
            reply => {
                return reply
                    .attributes(
                        services, fence, mount, request, protected, handle, operation, identity,
                    )
                    .await
            }
        };
        // Nothing is held in the engine after the visit: the reply ends it.
        match NativeData::read(services, mount, request, protected, handle, input).await {
            Ok(Ok(data)) => {
                reply.data(data.bytes());
                RequestDisposition::Complete
            }
            Ok(Err(reason)) => {
                reply.error(refusal(reason));
                RequestDisposition::Complete
            }
            Err(error) => {
                reply.error(terminal::errno(error.fenced()));
                terminal::read(&fence, error).await
            }
        }
    }
    /// LOOKUP, GETATTR, OPEN and OPENDIR: one decided inode and its reply.
    #[allow(clippy::too_many_arguments)]
    async fn attributes(
        self,
        services: Arc<dyn RequestServices>,
        fence: Fence,
        mount: NativeMount,
        request: u64,
        protected: u64,
        handle: Option<u64>,
        operation: NativeReadOperation,
        identity: Identity,
    ) -> RequestDisposition {
        let answer =
            match NativeRead::prepare(services, mount, request, protected, handle, operation).await
            {
                Ok(answer) => answer,
                Err(error) => {
                    self.error(terminal::errno(error.fenced()));
                    return terminal::read(&fence, error).await;
                }
            };
        let value = match answer.value() {
            Ok(value) => value,
            Err(reason) => {
                self.error(refusal(reason));
                return dispose(answer).await;
            }
        };
        match self {
            Self::Open(reply, directory) => {
                let handle = if directory {
                    value.directory.map(|value| value.owner_id())
                } else {
                    value.file.map(|value| value.owner_id())
                };
                let Some(handle) = handle else {
                    reply.error(Errno::EIO);
                    return RequestDisposition::Retained(Box::new(answer.retain(Box::new(
                        std::io::Error::other("successful native open has no handle"),
                    ))));
                };
                reply.opened(FileHandle(handle), open_flags());
            }
            reply => {
                let attributes = match identity.attributes(&value.stat) {
                    Ok(attributes) => attributes,
                    Err(error) => {
                        reply.error(error);
                        return RequestDisposition::Retained(Box::new(
                            answer
                                .retain(Box::new(std::io::Error::from_raw_os_error(error.code()))),
                        ));
                    }
                };
                match reply {
                    Self::Entry(reply) => {
                        reply.entry(&TTL, &attributes, Generation(mount.owner_id()))
                    }
                    Self::Attr(reply) => reply.attr(&TTL, &attributes),
                    _ => unreachable!("data is served before, opens above"),
                }
            }
        }
        dispose(answer).await
    }
}
async fn dispose(answer: NativeRead) -> RequestDisposition {
    match answer.dispose().await {
        Ok(()) => RequestDisposition::Complete,
        Err(error) => RequestDisposition::Retained(Box::new(error)),
    }
}
/// The declared page-cache treatment of every opened or created file.
pub(super) fn open_flags() -> FopenFlags {
    let cache = crate::coherence::pages::open_cache();
    let mut flags = FopenFlags::empty();
    flags.set(FopenFlags::FOPEN_KEEP_CACHE, cache.keep);
    flags.set(FopenFlags::FOPEN_DIRECT_IO, cache.direct);
    flags
}
