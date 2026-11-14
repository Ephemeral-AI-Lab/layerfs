//! Pinned fuser consumes replies once; send/delivery outcomes are unavailable.
use super::{accounting::Accounting, passthrough::Passthrough, terminal};
use crate::{
    attributes::{refusal, Identity},
    RequestDisposition,
};
use fuser::{
    Errno, FileAttr, FileHandle, FileType, FopenFlags, Generation, INodeNo, ReplyAttr, ReplyData,
    ReplyEntry, ReplyOpen,
};
use layerfs_mount_ports::{
    operations::{KernelData, KernelRead, ReadDataInput},
    ports::{Fence, RequestServices},
};
use layerfs_overlay::KernelMount;
use layerfs_workspace::{KernelReadOperation, Refusal};
use std::{
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
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
        mount: KernelMount,
        request: u64,
        protected: u64,
        handle: Option<u64>,
        operation: KernelReadOperation,
        identity: Identity,
        accounting: Arc<Accounting>,
        passthrough: Option<Arc<Passthrough>>,
    ) -> RequestDisposition {
        let (reply, input) = match self {
            Self::Data(reply, offset, length) => (reply, ReadDataInput::File { offset, length }),
            Self::Link(reply) => (reply, ReadDataInput::Link),
            reply => {
                return reply
                    .attributes(
                        services,
                        fence,
                        mount,
                        request,
                        protected,
                        handle,
                        operation,
                        identity,
                        accounting,
                        passthrough,
                    )
                    .await
            }
        };
        if let ReadDataInput::File { offset, length } = input {
            if length as usize > layerfs_overlay::READ_WINDOW {
                return windows(
                    services, fence, mount, request, protected, handle, offset, length, reply,
                )
                .await;
            }
        }
        // Nothing is held in the engine after the visit: the reply ends it.
        match KernelData::read(services, mount, request, protected, handle, input).await {
            Ok(Ok(data)) => {
                let _stage = layerfs_telemetry::timer::stages::Timer::start(
                    layerfs_telemetry::timer::stages::Stage::Reply,
                );
                layerfs_telemetry::timer::copies::copied(
                    layerfs_telemetry::timer::copies::Site::Reply,
                    data.bytes().len(),
                );
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
        mount: KernelMount,
        request: u64,
        protected: u64,
        handle: Option<u64>,
        operation: KernelReadOperation,
        identity: Identity,
        accounting: Arc<Accounting>,
        passthrough: Option<Arc<Passthrough>>,
    ) -> RequestDisposition {
        // Prototype (experiment `passthrough`): a read-only OPEN of a regular
        // file asks, before it is decided, whether its whole content is a
        // complete decoded cached file.
        let cached = match (&self, &operation, &passthrough) {
            (
                Self::Open(_, false),
                KernelReadOperation::Open {
                    writable: false, ..
                },
                Some(_),
            ) => layerfs_mount_ports::operations::unchanged_open(&services, mount, protected)
                .await
                .and_then(|root| layerfs_mount_ports::decoded::get()?.complete(root))
                .map(|(file, _)| file),
            _ => None,
        };
        let answer =
            match KernelRead::prepare(services, mount, request, protected, handle, operation).await
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
                match self {
                    // A missing name is a negative entry the kernel keeps for
                    // TTL. It takes no lookup reference. Every create, link,
                    // mkdir, symlink and rename through the mount replaces it
                    // in the kernel, and a merged install invalidates every
                    // added name (`control/invalidate.rs`).
                    Self::Entry(reply) if reason == Refusal::Missing => {
                        accounting.lookup_missing();
                        reply.entry_with_ttls(&Duration::ZERO, &TTL, &missing(), Generation(0));
                    }
                    reply => reply.error(refusal(reason)),
                }
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
                        std::io::Error::other("successful kernel open has no handle"),
                    ))));
                };
                match passthrough.filter(|_| !directory) {
                    Some(passthrough) => {
                        passthrough.open(reply, protected, handle, open_flags(), cached.as_deref())
                    }
                    None => reply.opened(FileHandle(handle), open_flags()),
                }
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
/// The entry of a name that does not exist: node id 0, which the kernel
/// treats as ENOENT with a valid entry lifetime. No attribute is read.
fn missing() -> FileAttr {
    FileAttr {
        ino: INodeNo(0),
        size: 0,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0,
        nlink: 0,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    }
}
async fn dispose(answer: KernelRead) -> RequestDisposition {
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
/// Prototype (FUSE profile probe): a READ above the engine's read window is
/// served as consecutive window reads, each its own owner visit, and replied
/// once. A short window ends it; a failure after the first window replies
/// the bytes already read.
#[allow(clippy::too_many_arguments)]
async fn windows(
    services: Arc<dyn RequestServices>,
    fence: Fence,
    mount: KernelMount,
    request: u64,
    protected: u64,
    handle: Option<u64>,
    offset: u64,
    length: u32,
    reply: ReplyData,
) -> RequestDisposition {
    let window = layerfs_overlay::READ_WINDOW as u32;
    let mut data = Vec::with_capacity(length as usize);
    while (data.len() as u32) < length {
        let want = (length - data.len() as u32).min(window);
        let input = ReadDataInput::File {
            offset: offset + data.len() as u64,
            length: want,
        };
        // Prototype (copies experiment, `join`): each window is emitted into
        // the one reply instead of copied there; a failed window's partial
        // bytes are dropped, as a failed window added none before.
        let before = data.len();
        let answer = if layerfs_telemetry::timer::copies::switches().join {
            KernelData::read_into(
                services.clone(),
                mount,
                request,
                protected,
                handle,
                input,
                &mut data,
            )
            .await
            .map(|answer| answer.map(|()| None))
        } else {
            KernelData::read(services.clone(), mount, request, protected, handle, input)
                .await
                .map(|answer| answer.map(Some))
        };
        if !matches!(answer, Ok(Ok(_))) {
            data.truncate(before);
        }
        match answer {
            Ok(Ok(part)) => {
                let got = match part {
                    Some(part) => {
                        let bytes = part.bytes();
                        layerfs_telemetry::timer::copies::copied(
                            layerfs_telemetry::timer::copies::Site::Join,
                            bytes.len(),
                        );
                        data.extend_from_slice(bytes);
                        bytes.len()
                    }
                    None => data.len() - before,
                };
                if (got as u32) < want {
                    break;
                }
            }
            Ok(Err(reason)) if !data.is_empty() => {
                eprintln!(
                    "PROBE_SHORT_READ at {} refusal {reason:?}",
                    offset + data.len() as u64
                );
                break;
            }
            Err(error) if !data.is_empty() => {
                eprintln!(
                    "PROBE_SHORT_READ at {} failure {error:?} / {error}",
                    offset + data.len() as u64
                );
                break;
            }
            Ok(Err(reason)) => {
                reply.error(refusal(reason));
                return RequestDisposition::Complete;
            }
            Err(error) => {
                reply.error(terminal::errno(error.fenced()));
                return terminal::read(&fence, error).await;
            }
        }
    }
    let _stage = layerfs_telemetry::timer::stages::Timer::start(
        layerfs_telemetry::timer::stages::Stage::Reply,
    );
    layerfs_telemetry::timer::copies::copied(
        layerfs_telemetry::timer::copies::Site::Reply,
        data.len(),
    );
    reply.data(&data);
    RequestDisposition::Complete
}
