//! Pinned fuser consumes replies once; send/delivery outcomes are unavailable.
use crate::{
    attributes::{refusal, Identity},
    operations::NativeRead,
    ports::RequestServices,
    RequestDisposition,
};
use fuser::{
    Errno, FileHandle, FopenFlags, Generation, ReplyAttr, ReplyData, ReplyEntry, ReplyOpen,
};
use layerfs_overlay::NativeMount;
use layerfs_workspace::NativeReadOperation;
use std::{sync::Arc, time::Duration};
const TTL: Duration = Duration::from_secs(60);

pub(super) enum ReadReply {
    Entry(ReplyEntry),
    Attr(ReplyAttr),
    Open(ReplyOpen, bool),
    Data(ReplyData, u64, u32),
    Link(ReplyData),
}
impl ReadReply {
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
                    self.error(Errno::EIO);
                    return RequestDisposition::Retained(Box::new(error));
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
            Self::Data(reply, offset, length) => {
                return data(reply, answer.read_file(offset, length).await).await;
            }
            Self::Link(reply) => return data(reply, answer.readlink().await).await,
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
                reply.opened(FileHandle(handle), FopenFlags::FOPEN_KEEP_CACHE);
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
                    _ => unreachable!("data and opens handled above"),
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
async fn data(
    reply: ReplyData,
    result: Result<crate::operations::NativeData, crate::operations::ReadFailure>,
) -> RequestDisposition {
    match result {
        Ok(data) => {
            reply.data(data.bytes());
            match data.dispose().await {
                Ok(()) => RequestDisposition::Complete,
                Err(error) => RequestDisposition::Retained(Box::new(error)),
            }
        }
        Err(error) => {
            reply.error(Errno::EIO);
            RequestDisposition::Retained(Box::new(error))
        }
    }
}
