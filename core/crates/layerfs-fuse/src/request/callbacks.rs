//! Borrowed kernel input enters fixed receive accounting before owned copies.
use super::{
    accounting::Opcode,
    failure::{failed, KernelInput},
    reply::ReadReply,
    state::NativeFilesystem,
};
use crate::RequestDisposition;
use fuser::{
    AccessFlags, BsdFileFlags, CopyFileRangeFlags, Errno, FileHandle, Filesystem, INodeNo,
    IoctlFlags, KernelConfig, LockOwner, OpenAccMode, OpenFlags, PollEvents, PollFlags,
    PollNotifier, RenameFlags, ReplyAttr, ReplyBmap, ReplyCreate, ReplyData, ReplyDirectory,
    ReplyDirectoryPlus, ReplyEmpty, ReplyEntry, ReplyIoctl, ReplyLock, ReplyLseek, ReplyOpen,
    ReplyPoll, ReplyStatfs, ReplyWrite, ReplyXattr, Request, TimeOrNow, WriteFlags,
};
use layerfs_content::filesystem::PathName;
use layerfs_workspace::NativeReadOperation;
use std::{ffi::OsStr, io, os::unix::ffi::OsStrExt, path::Path, time::SystemTime};

/// The canonical format stores regular files, directories and symlinks only.
const TYPE_MASK: u32 = 0o170000;
const REGULAR: u32 = 0o100000;

impl Filesystem for NativeFilesystem {
    fn init(&mut self, _: &Request, config: &mut KernelConfig) -> io::Result<()> {
        let negotiated = crate::mount::negotiate(config)?;
        self.negotiation
            .set(negotiated)
            .map_err(|_| io::Error::other("duplicate native initialization"))
    }
    fn lookup(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        if name.as_bytes().len() > 255 {
            return self.refuse(
                Opcode::Lookup,
                reply,
                ReplyEntry::error,
                Errno::ENAMETOOLONG,
            );
        }
        let Some((permit, reply)) = self.admit(
            Opcode::Lookup,
            ReadReply::Entry(reply),
            name.as_bytes().len(),
        ) else {
            return;
        };
        let Some((parent, reply)) = self.serial(parent, reply, ReadReply::error) else {
            return;
        };
        let name = match PathName::from_bytes(name.as_bytes()) {
            Ok(name) => name,
            Err(_) => return self.refused(reply, ReadReply::error, Errno::EINVAL),
        };
        self.submit(
            permit,
            req.unique().0,
            parent,
            None,
            NativeReadOperation::Lookup { parent, name },
            reply,
        );
    }
    fn forget(&self, req: &Request, inode: INodeNo, count: u64) {
        self.forget_unit(req.unique().0, inode, count);
    }
    fn getattr(&self, req: &Request, inode: INodeNo, handle: Option<FileHandle>, reply: ReplyAttr) {
        let Some((permit, reply)) = self.admit(Opcode::Getattr, ReadReply::Attr(reply), 0) else {
            return;
        };
        let Some((serial, reply)) = self.serial(inode, reply, ReadReply::error) else {
            return;
        };
        self.submit(
            permit,
            req.unique().0,
            serial,
            handle.map(|handle| handle.0),
            NativeReadOperation::Getattr { serial },
            reply,
        );
    }
    fn readlink(&self, req: &Request, inode: INodeNo, reply: ReplyData) {
        let Some((permit, reply)) = self.admit(Opcode::Readlink, ReadReply::Link(reply), 0) else {
            return;
        };
        let Some((serial, reply)) = self.serial(inode, reply, ReadReply::error) else {
            return;
        };
        self.submit(
            permit,
            req.unique().0,
            serial,
            None,
            NativeReadOperation::Getattr { serial },
            reply,
        );
    }
    fn open(&self, req: &Request, inode: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        let Some((permit, reply)) = self.admit(Opcode::Open, ReadReply::Open(reply, false), 0)
        else {
            return;
        };
        if flags.acc_mode() != OpenAccMode::O_RDONLY {
            // Native mutation composition is not wired yet: a writable handle
            // would own engine custody that no WRITE can use.
            return self.refused(reply, ReadReply::error, Errno::EROFS);
        }
        let Some((serial, reply)) = self.serial(inode, reply, ReadReply::error) else {
            return;
        };
        self.submit(
            permit,
            req.unique().0,
            serial,
            None,
            NativeReadOperation::Open {
                serial,
                writable: false,
            },
            reply,
        );
    }
    fn read(
        &self,
        req: &Request,
        inode: INodeNo,
        handle: FileHandle,
        offset: u64,
        size: u32,
        _: OpenFlags,
        _: Option<LockOwner>,
        reply: ReplyData,
    ) {
        let Some((permit, reply)) =
            self.admit(Opcode::Read, ReadReply::Data(reply, offset, size), 0)
        else {
            return;
        };
        if size as usize > layerfs_overlay::READ_WINDOW {
            return self.refused(reply, ReadReply::error, Errno::EINVAL);
        }
        let Some((serial, reply)) = self.serial(inode, reply, ReadReply::error) else {
            return;
        };
        self.submit(
            permit,
            req.unique().0,
            serial,
            Some(handle.0),
            NativeReadOperation::Getattr { serial },
            reply,
        );
    }
    fn release(
        &self,
        req: &Request,
        inode: INodeNo,
        handle: FileHandle,
        _: OpenFlags,
        _: Option<LockOwner>,
        _: bool,
        reply: ReplyEmpty,
    ) {
        let Some((permit, reply)) = self.admit_reply(Opcode::Release, 0, reply, ReplyEmpty::error)
        else {
            return;
        };
        let mount = self.queue.identity();
        let serial = self.identity.serial(inode);
        let services = self.services.clone();
        let request = req.unique().0;
        self.handoff(
            permit,
            Box::pin(async move {
                let outcome = async {
                    let serial =
                        serial.map_err(|error| io::Error::from_raw_os_error(error.code()))?;
                    let services = services.request()?;
                    drop(services.close_file(mount, serial, handle.0).await?);
                    Ok::<_, crate::ports::ServiceError>(())
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
                                directory: false,
                            },
                            error,
                        )
                    }
                }
            }),
        );
    }
    fn opendir(&self, req: &Request, inode: INodeNo, _: OpenFlags, reply: ReplyOpen) {
        let Some((permit, reply)) = self.admit(Opcode::Opendir, ReadReply::Open(reply, true), 0)
        else {
            return;
        };
        let Some((serial, reply)) = self.serial(inode, reply, ReadReply::error) else {
            return;
        };
        self.submit(
            permit,
            req.unique().0,
            serial,
            None,
            NativeReadOperation::Opendir { serial },
            reply,
        );
    }
    fn readdir(
        &self,
        req: &Request,
        inode: INodeNo,
        handle: FileHandle,
        offset: u64,
        reply: ReplyDirectory,
    ) {
        self.read_directory(req, inode, handle, offset, reply);
    }
    fn releasedir(
        &self,
        req: &Request,
        inode: INodeNo,
        handle: FileHandle,
        _: OpenFlags,
        reply: ReplyEmpty,
    ) {
        self.release_directory(req, inode, handle, reply);
    }

    // Success with no engine job: nothing is flushed and no durability is claimed.
    fn flush(&self, _: &Request, _: INodeNo, _: FileHandle, _: LockOwner, reply: ReplyEmpty) {
        self.inline(Opcode::Flush, reply, ReplyEmpty::error, ReplyEmpty::ok);
    }
    fn fsync(&self, _: &Request, _: INodeNo, _: FileHandle, _: bool, reply: ReplyEmpty) {
        self.inline(Opcode::Fsync, reply, ReplyEmpty::error, ReplyEmpty::ok);
    }
    fn fsyncdir(&self, _: &Request, _: INodeNo, _: FileHandle, _: bool, reply: ReplyEmpty) {
        self.inline(Opcode::Fsyncdir, reply, ReplyEmpty::error, ReplyEmpty::ok);
    }
    fn statfs(&self, _: &Request, _: INodeNo, reply: ReplyStatfs) {
        self.inline(
            Opcode::Statfs,
            reply,
            ReplyStatfs::error,
            super::inline::statfs,
        );
    }

    // The extended-attribute family is absent; ENOSYS is sticky per connection.
    fn setxattr(
        &self,
        _: &Request,
        _: INodeNo,
        _: &OsStr,
        _: &[u8],
        _: i32,
        _: u32,
        reply: ReplyEmpty,
    ) {
        self.refuse(Opcode::Setxattr, reply, ReplyEmpty::error, Errno::ENOSYS);
    }
    fn getxattr(&self, _: &Request, _: INodeNo, _: &OsStr, _: u32, reply: ReplyXattr) {
        self.refuse(Opcode::Getxattr, reply, ReplyXattr::error, Errno::ENOSYS);
    }
    fn listxattr(&self, _: &Request, _: INodeNo, _: u32, reply: ReplyXattr) {
        self.refuse(Opcode::Listxattr, reply, ReplyXattr::error, Errno::ENOSYS);
    }
    fn removexattr(&self, _: &Request, _: INodeNo, _: &OsStr, reply: ReplyEmpty) {
        self.refuse(Opcode::Removexattr, reply, ReplyEmpty::error, Errno::ENOSYS);
    }

    // Native mutation composition is not wired in this adapter yet. Each family
    // is refused before any effect instead of inheriting a library default.
    fn setattr(
        &self,
        _: &Request,
        _: INodeNo,
        _: Option<u32>,
        _: Option<u32>,
        _: Option<u32>,
        _: Option<u64>,
        _: Option<TimeOrNow>,
        _: Option<TimeOrNow>,
        _: Option<SystemTime>,
        _: Option<FileHandle>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        self.refuse(Opcode::Setattr, reply, ReplyAttr::error, Errno::EROFS);
    }
    fn mknod(
        &self,
        _: &Request,
        _: INodeNo,
        _: &OsStr,
        mode: u32,
        _: u32,
        _: u32,
        reply: ReplyEntry,
    ) {
        let errno = if mode & TYPE_MASK == REGULAR {
            Errno::EROFS
        } else {
            Errno::EPERM
        };
        self.refuse(Opcode::Mknod, reply, ReplyEntry::error, errno);
    }
    fn mkdir(&self, _: &Request, _: INodeNo, _: &OsStr, _: u32, _: u32, reply: ReplyEntry) {
        self.refuse(Opcode::Mkdir, reply, ReplyEntry::error, Errno::EROFS);
    }
    fn unlink(&self, _: &Request, _: INodeNo, _: &OsStr, reply: ReplyEmpty) {
        self.refuse(Opcode::Unlink, reply, ReplyEmpty::error, Errno::EROFS);
    }
    fn rmdir(&self, _: &Request, _: INodeNo, _: &OsStr, reply: ReplyEmpty) {
        self.refuse(Opcode::Rmdir, reply, ReplyEmpty::error, Errno::EROFS);
    }
    fn symlink(&self, _: &Request, _: INodeNo, _: &OsStr, _: &Path, reply: ReplyEntry) {
        self.refuse(Opcode::Symlink, reply, ReplyEntry::error, Errno::EROFS);
    }
    fn rename(
        &self,
        _: &Request,
        _: INodeNo,
        _: &OsStr,
        _: INodeNo,
        _: &OsStr,
        _: RenameFlags,
        reply: ReplyEmpty,
    ) {
        self.refuse(Opcode::Rename, reply, ReplyEmpty::error, Errno::EROFS);
    }
    fn link(&self, _: &Request, _: INodeNo, _: INodeNo, _: &OsStr, reply: ReplyEntry) {
        self.refuse(Opcode::Link, reply, ReplyEntry::error, Errno::EROFS);
    }
    fn write(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: u64,
        _: &[u8],
        _: WriteFlags,
        _: OpenFlags,
        _: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        self.refuse(Opcode::Write, reply, ReplyWrite::error, Errno::EROFS);
    }
    fn create(
        &self,
        _: &Request,
        _: INodeNo,
        _: &OsStr,
        _: u32,
        _: u32,
        _: i32,
        reply: ReplyCreate,
    ) {
        self.refuse(Opcode::Create, reply, ReplyCreate::error, Errno::EROFS);
    }
    fn fallocate(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: u64,
        _: u64,
        _: i32,
        reply: ReplyEmpty,
    ) {
        self.refuse(Opcode::Fallocate, reply, ReplyEmpty::error, Errno::EROFS);
    }
    fn copy_file_range(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: u64,
        _: INodeNo,
        _: FileHandle,
        _: u64,
        _: u64,
        _: CopyFileRangeFlags,
        reply: ReplyWrite,
    ) {
        self.refuse(
            Opcode::CopyFileRange,
            reply,
            ReplyWrite::error,
            Errno::EROFS,
        );
    }

    // No capability for these is negotiated; the kernel keeps them local or
    // falls back. ENOSYS states that explicitly rather than by default.
    fn access(&self, _: &Request, _: INodeNo, _: AccessFlags, reply: ReplyEmpty) {
        self.refuse(Opcode::Access, reply, ReplyEmpty::error, Errno::ENOSYS);
    }
    fn readdirplus(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: u64,
        reply: ReplyDirectoryPlus,
    ) {
        self.refuse(
            Opcode::Readdirplus,
            reply,
            ReplyDirectoryPlus::error,
            Errno::ENOSYS,
        );
    }
    fn getlk(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: LockOwner,
        _: u64,
        _: u64,
        _: i32,
        _: u32,
        reply: ReplyLock,
    ) {
        self.refuse(Opcode::Getlk, reply, ReplyLock::error, Errno::ENOSYS);
    }
    fn setlk(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: LockOwner,
        _: u64,
        _: u64,
        _: i32,
        _: u32,
        _: bool,
        reply: ReplyEmpty,
    ) {
        self.refuse(Opcode::Setlk, reply, ReplyEmpty::error, Errno::ENOSYS);
    }
    fn bmap(&self, _: &Request, _: INodeNo, _: u32, _: u64, reply: ReplyBmap) {
        self.refuse(Opcode::Bmap, reply, ReplyBmap::error, Errno::ENOSYS);
    }
    fn ioctl(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: IoctlFlags,
        _: u32,
        _: &[u8],
        _: u32,
        reply: ReplyIoctl,
    ) {
        self.refuse(Opcode::Ioctl, reply, ReplyIoctl::error, Errno::ENOSYS);
    }
    fn poll(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: PollNotifier,
        _: PollEvents,
        _: PollFlags,
        reply: ReplyPoll,
    ) {
        self.refuse(Opcode::Poll, reply, ReplyPoll::error, Errno::ENOSYS);
    }
    fn lseek(&self, _: &Request, _: INodeNo, _: FileHandle, _: i64, _: i32, reply: ReplyLseek) {
        self.refuse(Opcode::Lseek, reply, ReplyLseek::error, Errno::ENOSYS);
    }
}
