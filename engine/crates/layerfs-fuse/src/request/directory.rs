//! Deferred kernel enumeration and exact descriptor release.
use super::{
    accounting::Opcode,
    failure::{failed, KernelInput},
    terminal, KernelFilesystem,
};
use crate::{attributes::Identity, RequestDisposition};
use fuser::{
    Errno, FileHandle, FileType, INodeNo, ReplyDirectory, ReplyDirectoryPlus, ReplyEmpty, Request,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_mount_ports::{
    operations::{DirectoryStep, DirectoryStream},
    ports::{Fence, ServiceError},
};
use std::{ffi::OsStr, io, os::unix::ffi::OsStrExt};

/// The reply of one enumeration: READDIR names, or (prototype, READDIRPLUS
/// experiment) READDIRPLUS entries with attributes and kernel references.
pub(super) enum Listing {
    Names(ReplyDirectory),
    Plus(ReplyDirectoryPlus),
}
impl Listing {
    pub(super) fn error(self, error: Errno) {
        match self {
            Self::Names(reply) => reply.error(error),
            Self::Plus(reply) => reply.error(error),
        }
    }
    fn ok(self) {
        match self {
            Self::Names(reply) => reply.ok(),
            Self::Plus(reply) => reply.ok(),
        }
    }
}
impl KernelFilesystem {
    pub(super) fn read_directory(
        &self,
        req: &Request,
        inode: INodeNo,
        handle: FileHandle,
        offset: u64,
        reply: Listing,
    ) {
        let (opcode, plus) = match reply {
            Listing::Names(_) => (Opcode::Readdir, false),
            Listing::Plus(_) => (Opcode::Readdirplus, true),
        };
        let Some((permit, reply)) = self.receive_reply(opcode, 0, reply, Listing::error) else {
            return;
        };
        let page = self.negotiation.get().map_or(0, |value| value.page_size);
        let mount = self.queue.identity();
        let services = self.services.clone();
        let fence = self.fence.clone();
        let identity = self.identity;
        let request = req.unique().0;
        self.serve(
            permit,
            Box::pin(async move {
                let input = KernelInput::Directory {
                    inode: inode.0,
                    handle: handle.0,
                    offset,
                };
                let prepared = async {
                    let serial = identity
                        .serial(inode)
                        .map_err(|error| io::Error::from_raw_os_error(error.code()))?;
                    let services = services.request(&fence)?;
                    Ok::<_, ServiceError>((
                        DirectoryStream::prepare_limited(
                            services.clone(),
                            mount,
                            request,
                            serial,
                            handle.0,
                            offset,
                            plus,
                            match plus {
                                true => Some(super::directory_plus::most(page)),
                                false => super::directory_plus::names_rows(),
                            },
                        )
                        .await,
                        services,
                        serial,
                    ))
                }
                .await;
                let prepared = prepared.map(|(stream, services, serial)| {
                    stream.map(|stream| {
                        let plus = super::directory_plus::Plus {
                            services,
                            mount,
                            request,
                            serial,
                            page,
                        };
                        (stream, plus)
                    })
                });
                match prepared {
                    Ok(Ok((stream, plus))) => {
                        enumerate(reply, stream, identity, &fence, plus).await
                    }
                    // An offset from before the handle's last rewind: the
                    // visit recorded nothing and nothing is retained.
                    Ok(Err(failure)) if failure.rewound() => {
                        reply.error(Errno::EINVAL);
                        RequestDisposition::Complete
                    }
                    // The typed failure still owns whatever was acquired.
                    Ok(Err(failure)) if failure.fenced() => {
                        reply.error(terminal::STOPPED);
                        terminal::directory(&fence, failure).await
                    }
                    Ok(Err(failure)) => {
                        reply.error(Errno::EIO);
                        failed(request, mount, input, Box::new(failure))
                    }
                    Err(error) => {
                        reply.error(Errno::EIO);
                        failed(request, mount, input, error)
                    }
                }
            }),
        );
    }
    pub(super) fn release_directory(
        &self,
        req: &Request,
        inode: INodeNo,
        handle: FileHandle,
        reply: ReplyEmpty,
    ) {
        let Some((permit, reply)) =
            self.receive_reply(Opcode::Releasedir, 0, reply, ReplyEmpty::error)
        else {
            return;
        };
        let mount = self.queue.identity();
        let services = self.services.clone();
        let fence = self.fence.clone();
        let serial = self.identity.serial(inode);
        let request = req.unique().0;
        self.serve(
            permit,
            Box::pin(async move {
                let outcome = async {
                    let serial =
                        serial.map_err(|error| io::Error::from_raw_os_error(error.code()))?;
                    let services = services.request(&fence)?;
                    // One owner job finds the open descriptor and closes it.
                    // It only gives back, so a stopped mount never refuses it.
                    let started = std::time::Instant::now();
                    let closed = services.close_directory(mount, serial, handle.0).await;
                    crate::request::charge(crate::request::Phase::CloseDirectory, started);
                    drop(closed?);
                    Ok::<_, ServiceError>(())
                }
                .await;
                match outcome {
                    Ok(()) => {
                        reply.ok();
                        RequestDisposition::Complete
                    }
                    Err(error) => {
                        reply.error(Errno::EIO);
                        failed(
                            request,
                            mount,
                            KernelInput::Release {
                                inode: inode.0,
                                handle: handle.0,
                                directory: true,
                            },
                            error,
                        )
                    }
                }
            }),
        );
    }
}
async fn enumerate(
    mut reply: Listing,
    mut stream: DirectoryStream,
    identity: Identity,
    fence: &Fence,
    plus: super::directory_plus::Plus,
) -> RequestDisposition {
    let mut reply_bytes = 0;
    for (name, serial, cookie) in stream.dots() {
        let inode = match identity.inode(serial) {
            Ok(inode) => inode,
            Err(error) => {
                reply.error(error);
                return RequestDisposition::Retained(Box::new(
                    stream.retain(Box::new(io::Error::from_raw_os_error(error.code()))),
                ));
            }
        };
        let full = match &mut reply {
            Listing::Names(names) => add(
                names,
                &mut reply_bytes,
                inode,
                cookie,
                FileType::Directory,
                OsStr::new(name),
            ),
            Listing::Plus(entries) => {
                super::directory_plus::dot(entries, &mut reply_bytes, inode, cookie, name)
            }
        };
        if full {
            reply.ok();
            return RequestDisposition::Complete;
        }
    }
    loop {
        let mut batch = match stream.next().await {
            // Nothing is held in the engine after the visits: the reply
            // ends the request.
            Ok(DirectoryStep::End(_)) => {
                reply.ok();
                return RequestDisposition::Complete;
            }
            Ok(DirectoryStep::Batch(batch)) => batch,
            Err(error) => {
                reply.error(terminal::errno(error.fenced()));
                return terminal::directory(fence, error).await;
            }
        };
        // Prototype: READDIRPLUS entries carry facts and kernel references.
        let filled = match &mut reply {
            Listing::Plus(entries) => Some(
                super::directory_plus::fill(entries, &mut reply_bytes, &batch, identity, &plus)
                    .await,
            ),
            Listing::Names(_) => None,
        };
        let accepted = match filled {
            Some(Ok(accepted)) => {
                // The publishing visit takes one kernel lookup reference
                // for each entry the reply hands out, as LOOKUP does.
                if batch.publishes() {
                    batch.adopt(accepted);
                }
                accepted
            }
            Some(Err(failure)) => {
                if failure.offered {
                    reply.ok();
                } else {
                    reply.error(failure.errno);
                }
                return failure.dispose(batch, fence).await;
            }
            None => {
                let Listing::Names(names) = &mut reply else {
                    unreachable!("READDIRPLUS entries are filled above")
                };
                let mut accepted = 0;
                let mut conversion_error = None;
                for (entry, cookie) in batch.entries() {
                    let inode = match identity.inode(entry.serial) {
                        Ok(inode) => inode,
                        Err(error) => {
                            conversion_error = Some(error);
                            break;
                        }
                    };
                    let kind = match entry.kind {
                        InodeKind::Directory => FileType::Directory,
                        InodeKind::RegularFile => FileType::RegularFile,
                        InodeKind::Symlink => FileType::Symlink,
                    };
                    if add(
                        names,
                        &mut reply_bytes,
                        inode,
                        cookie,
                        kind,
                        OsStr::from_bytes(&entry.name),
                    ) {
                        break;
                    }
                    accepted += 1;
                }
                if let Some(error) = conversion_error {
                    reply.error(error);
                    return RequestDisposition::Retained(Box::new(
                        batch.retain(Box::new(io::Error::from_raw_os_error(error.code()))),
                    ));
                }
                accepted
            }
        };
        stream = match batch.accept(accepted).await {
            Ok(stream) => stream,
            // A fenced publication published nothing: the offered entries
            // are never sent. Any other failure here is retained.
            Err(error) => {
                reply.error(terminal::errno(error.fenced()));
                return terminal::directory(fence, error).await;
            }
        };
    }
}
fn add(
    reply: &mut ReplyDirectory,
    used: &mut usize,
    inode: INodeNo,
    cookie: u64,
    kind: FileType,
    name: &OsStr,
) -> bool {
    // Pinned fuse_dirent has two u64s and two u32s; fuser pads each entry to8.
    // This bounds our reply even if the kernel offers a larger buffer. It does
    // not limit directory size: the last published cookie resumes the next call.
    let size = (24 + name.as_bytes().len()).next_multiple_of(8);
    if size > layerfs_overlay::READ_WINDOW - *used {
        return true;
    }
    if reply.add(inode, cookie, kind, name) {
        return true;
    }
    *used += size;
    false
}
