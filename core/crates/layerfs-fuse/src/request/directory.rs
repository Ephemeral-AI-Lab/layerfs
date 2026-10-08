//! Deferred native enumeration and exact descriptor release.
use super::{
    accounting::Opcode,
    failure::{failed, KernelInput},
    NativeFilesystem,
};
use crate::{
    attributes::Identity,
    operations::{DirectoryStep, DirectoryStream},
    ports::ServiceError,
    RequestDisposition,
};
use fuser::{Errno, FileHandle, FileType, INodeNo, ReplyDirectory, ReplyEmpty, Request};
use layerfs_content::object::inode_leaf::InodeKind;
use std::{ffi::OsStr, io, os::unix::ffi::OsStrExt};

impl NativeFilesystem {
    pub(super) fn read_directory(
        &self,
        req: &Request,
        inode: INodeNo,
        handle: FileHandle,
        offset: u64,
        reply: ReplyDirectory,
    ) {
        let Some((permit, reply)) =
            self.admit_reply(Opcode::Readdir, 0, reply, ReplyDirectory::error)
        else {
            return;
        };
        let mount = self.queue.identity();
        let services = self.services.clone();
        let identity = self.identity;
        let request = req.unique().0;
        self.handoff(
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
                    let services = services.request()?;
                    DirectoryStream::prepare(services, mount, request, serial, handle.0, offset)
                        .await
                        .map_err(|error| Box::new(error) as ServiceError)
                }
                .await;
                match prepared {
                    Ok(stream) => enumerate(reply, stream, identity).await,
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
            self.admit_reply(Opcode::Releasedir, 0, reply, ReplyEmpty::error)
        else {
            return;
        };
        let mount = self.queue.identity();
        let services = self.services.clone();
        let serial = self.identity.serial(inode);
        let request = req.unique().0;
        self.handoff(
            permit,
            Box::pin(async move {
                let outcome = async {
                    let serial =
                        serial.map_err(|error| io::Error::from_raw_os_error(error.code()))?;
                    let services = services.request()?;
                    let receipt = services.directory(mount, serial, handle.0).await?;
                    let directory = *receipt.get();
                    drop(receipt);
                    drop(services.close_directory(directory).await?);
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
    mut reply: ReplyDirectory,
    mut stream: DirectoryStream,
    identity: Identity,
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
        if add(
            &mut reply,
            &mut reply_bytes,
            inode,
            cookie,
            FileType::Directory,
            OsStr::new(name),
        ) {
            reply.ok();
            return dispose(stream).await;
        }
    }
    loop {
        let batch = match stream.next().await {
            Ok(DirectoryStep::End(stream)) => {
                reply.ok();
                return dispose(stream).await;
            }
            Ok(DirectoryStep::Batch(batch)) => batch,
            Err(error) => {
                reply.error(Errno::EIO);
                return RequestDisposition::Retained(Box::new(error));
            }
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
                &mut reply,
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
        stream = match batch.accept(accepted).await {
            Ok(stream) => stream,
            Err(error) => {
                reply.error(Errno::EIO);
                return RequestDisposition::Retained(Box::new(error));
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
async fn dispose(stream: DirectoryStream) -> RequestDisposition {
    match stream.dispose().await {
        Ok(()) => RequestDisposition::Complete,
        Err(error) => RequestDisposition::Retained(Box::new(error)),
    }
}
