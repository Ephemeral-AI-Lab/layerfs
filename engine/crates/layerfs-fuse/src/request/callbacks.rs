//! Borrowed kernel input enters fixed receive accounting before owned copies.
use super::{
    accounting::Opcode,
    failure::{failed, KernelInput},
    mutate::MutationReply,
    reply::ReadReply,
    state::KernelFilesystem,
};
use crate::{
    operations::{
        create, flush, link, remove, rename, unsupported,
        write::{self, AttributeChange, Stamp},
        Declined, MutationInput,
    },
    RequestDisposition,
};
use fuser::{
    AccessFlags, BsdFileFlags, CopyFileRangeFlags, Errno, FileHandle, Filesystem, INodeNo,
    IoctlFlags, KernelConfig, LockOwner, OpenAccMode, OpenFlags, PollEvents, PollFlags,
    PollNotifier, RenameFlags, ReplyAttr, ReplyBmap, ReplyCreate, ReplyData, ReplyDirectory,
    ReplyDirectoryPlus, ReplyEmpty, ReplyEntry, ReplyIoctl, ReplyLock, ReplyLseek, ReplyOpen,
    ReplyPoll, ReplyStatfs, ReplyWrite, ReplyXattr, Request, TimeOrNow, WriteFlags,
};
use layerfs_content::filesystem::PathName;
use layerfs_workspace::KernelReadOperation;
use std::{ffi::OsStr, io, os::unix::ffi::OsStrExt, path::Path, time::SystemTime};

/// FLUSH, FSYNC and FSYNCDIR: the declared answer, with no work.
fn unsynchronized() -> Errno {
    match flush::synchronize() {
        flush::Synchronize::NotImplemented => Errno::ENOSYS,
    }
}
/// `ENOSYS` for a request no capability was negotiated for.
fn absent() -> Errno {
    match unsupported::absent() {
        unsupported::Absent::NotImplemented => Errno::ENOSYS,
    }
}

impl Filesystem for KernelFilesystem {
    fn init(&mut self, _: &Request, config: &mut KernelConfig) -> io::Result<()> {
        let negotiated = crate::mount::negotiate(config)?;
        self.negotiation
            .set(negotiated)
            .map_err(|_| io::Error::other("duplicate kernel initialization"))
    }
    fn lookup(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let _timed = self.accounting.timed(Opcode::Lookup);
        if name.as_bytes().len() > 255 {
            return self.refuse(
                Opcode::Lookup,
                reply,
                ReplyEntry::error,
                Errno::ENAMETOOLONG,
            );
        }
        let Some((permit, reply)) = self.receive_request(
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
            KernelReadOperation::Lookup { parent, name },
            reply,
        );
    }
    fn forget(&self, req: &Request, inode: INodeNo, count: u64) {
        let _timed = self.accounting.timed(Opcode::Forget);
        self.forget_unit(req.unique().0, inode, count);
    }
    fn getattr(&self, req: &Request, inode: INodeNo, handle: Option<FileHandle>, reply: ReplyAttr) {
        let _timed = self.accounting.timed(Opcode::Getattr);
        let Some((permit, reply)) =
            self.receive_request(Opcode::Getattr, ReadReply::Attr(reply), 0)
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
            handle.map(|handle| handle.0),
            KernelReadOperation::Getattr { serial },
            reply,
        );
    }
    fn readlink(&self, req: &Request, inode: INodeNo, reply: ReplyData) {
        let _timed = self.accounting.timed(Opcode::Readlink);
        let Some((permit, reply)) =
            self.receive_request(Opcode::Readlink, ReadReply::Link(reply), 0)
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
            KernelReadOperation::Data { serial },
            reply,
        );
    }
    fn open(&self, req: &Request, inode: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        let _timed = self.accounting.timed(Opcode::Open);
        let Some((permit, reply)) =
            self.receive_request(Opcode::Open, ReadReply::Open(reply, false), 0)
        else {
            return;
        };
        let Some((serial, reply)) = self.serial(inode, reply, ReadReply::error) else {
            return;
        };
        // The descriptor's access mode is its engine custody. O_TRUNC arrives
        // separately as a size-0 SETATTR; O_APPEND is resolved by the kernel.
        self.submit(
            permit,
            req.unique().0,
            serial,
            None,
            KernelReadOperation::Open {
                serial,
                writable: flags.acc_mode() != OpenAccMode::O_RDONLY,
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
        let _timed = self.accounting.timed(Opcode::Read);
        self.accounting.read_size(size);
        let Some((permit, reply)) =
            self.receive_request(Opcode::Read, ReadReply::Data(reply, offset, size), 0)
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
            KernelReadOperation::Data { serial },
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
        let _timed = self.accounting.timed(Opcode::Release);
        let Some((permit, reply)) =
            self.receive_reply(Opcode::Release, 0, reply, ReplyEmpty::error)
        else {
            return;
        };
        let mount = self.queue.identity();
        let serial = self.identity.serial(inode);
        let services = self.services.clone();
        let fence = self.fence.clone();
        let request = req.unique().0;
        self.serve(
            permit,
            Box::pin(async move {
                let outcome = async {
                    let serial =
                        serial.map_err(|error| io::Error::from_raw_os_error(error.code()))?;
                    let services = services.request(&fence)?;
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
        let _timed = self.accounting.timed(Opcode::Opendir);
        let Some((permit, reply)) =
            self.receive_request(Opcode::Opendir, ReadReply::Open(reply, true), 0)
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
            KernelReadOperation::Opendir { serial },
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
        let _timed = self.accounting.timed(Opcode::Readdir);
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
        let _timed = self.accounting.timed(Opcode::Releasedir);
        self.release_directory(req, inode, handle, reply);
    }

    // Declared in operations/flush.rs: nothing is buffered here to flush and
    // no durability is claimed. `ENOSYS` is sticky per connection: the kernel
    // answers every later one itself, with success.
    fn flush(&self, _: &Request, _: INodeNo, _: FileHandle, _: LockOwner, reply: ReplyEmpty) {
        let _timed = self.accounting.timed(Opcode::Flush);
        self.refuse(Opcode::Flush, reply, ReplyEmpty::error, unsynchronized());
    }
    fn fsync(&self, _: &Request, _: INodeNo, _: FileHandle, _: bool, reply: ReplyEmpty) {
        let _timed = self.accounting.timed(Opcode::Fsync);
        self.refuse(Opcode::Fsync, reply, ReplyEmpty::error, unsynchronized());
    }
    fn fsyncdir(&self, _: &Request, _: INodeNo, _: FileHandle, _: bool, reply: ReplyEmpty) {
        let _timed = self.accounting.timed(Opcode::Fsyncdir);
        self.refuse(Opcode::Fsyncdir, reply, ReplyEmpty::error, unsynchronized());
    }
    fn statfs(&self, _: &Request, _: INodeNo, reply: ReplyStatfs) {
        let _timed = self.accounting.timed(Opcode::Statfs);
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
        let _timed = self.accounting.timed(Opcode::Setxattr);
        self.refuse(Opcode::Setxattr, reply, ReplyEmpty::error, absent());
    }
    fn getxattr(&self, _: &Request, _: INodeNo, _: &OsStr, _: u32, reply: ReplyXattr) {
        let _timed = self.accounting.timed(Opcode::Getxattr);
        self.refuse(Opcode::Getxattr, reply, ReplyXattr::error, absent());
    }
    fn listxattr(&self, _: &Request, _: INodeNo, _: u32, reply: ReplyXattr) {
        let _timed = self.accounting.timed(Opcode::Listxattr);
        self.refuse(Opcode::Listxattr, reply, ReplyXattr::error, absent());
    }
    fn removexattr(&self, _: &Request, _: INodeNo, _: &OsStr, reply: ReplyEmpty) {
        let _timed = self.accounting.timed(Opcode::Removexattr);
        self.refuse(Opcode::Removexattr, reply, ReplyEmpty::error, absent());
    }

    // Mutations: one atomic owner job each, replied from its published result.
    fn setattr(
        &self,
        req: &Request,
        inode: INodeNo,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        handle: Option<FileHandle>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        let _timed = self.accounting.timed(Opcode::Setattr);
        let serial = self.identity.serial(inode);
        let reply = MutationReply::Attr(reply, serial.unwrap_or(0));
        let Some((permit, reply)) =
            self.receive_reply(Opcode::Setattr, 0, reply, MutationReply::error)
        else {
            return;
        };
        let Some((serial, reply)) = self.serial(inode, reply, MutationReply::error) else {
            return;
        };
        let mtime = match mtime {
            None => None,
            Some(TimeOrNow::Now) => Some(Stamp::Processing),
            Some(TimeOrNow::SpecificTime(instant)) => match write::time(instant) {
                Some(time) => Some(Stamp::At(time)),
                None => return self.refused(reply, MutationReply::error, Errno::EINVAL),
            },
        };
        let change = AttributeChange {
            mode,
            owner: uid,
            group: gid,
            size,
            mtime,
        };
        // Only a size change travels through the descriptor: it must keep
        // working after the last name is removed.
        let handle = handle.filter(|_| size.is_some()).map(|handle| handle.0);
        let identity = (self.identity.uid, self.identity.gid);
        self.mutate(
            permit,
            req.unique().0,
            serial,
            handle,
            None,
            reply,
            Box::new(move |now| {
                write::set_attributes(serial, handle.is_some(), identity, change, now)
            }),
        );
    }
    fn mknod(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: ReplyEntry,
    ) {
        let _timed = self.accounting.timed(Opcode::Mknod);
        self.named(
            Opcode::Mknod,
            req.unique().0,
            parent,
            [name],
            0,
            None,
            MutationReply::Entry(reply, None),
            move |parent, [name], _| create::node(parent, name, mode).map(MutationInput::Named),
        );
    }
    fn mkdir(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let _timed = self.accounting.timed(Opcode::Mkdir);
        self.named(
            Opcode::Mkdir,
            req.unique().0,
            parent,
            [name],
            0,
            None,
            MutationReply::Entry(reply, None),
            move |parent, [name], _| Ok(MutationInput::Named(create::mkdir(parent, name, mode))),
        );
    }
    fn unlink(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let _timed = self.accounting.timed(Opcode::Unlink);
        self.named(
            Opcode::Unlink,
            req.unique().0,
            parent,
            [name],
            0,
            None,
            MutationReply::Done(reply),
            |parent, [name], _| Ok(MutationInput::Named(remove::unlink(parent, name))),
        );
    }
    fn rmdir(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let _timed = self.accounting.timed(Opcode::Rmdir);
        self.named(
            Opcode::Rmdir,
            req.unique().0,
            parent,
            [name],
            0,
            None,
            MutationReply::Done(reply),
            |parent, [name], _| Ok(MutationInput::Named(remove::rmdir(parent, name))),
        );
    }
    fn symlink(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
        let _timed = self.accounting.timed(Opcode::Symlink);
        let target = target.as_os_str().as_bytes();
        let page = self.negotiation.get().map_or(0, |value| value.page_size);
        // One page minus one is the longest target the kernel can read back;
        // a longer one is refused before its bytes are copied.
        if target.len() >= page as usize {
            return self.refuse(
                Opcode::Symlink,
                MutationReply::Entry(reply, None),
                MutationReply::error,
                crate::attributes::declined(Declined::TargetTooLong),
            );
        }
        let target = target.to_vec();
        self.named(
            Opcode::Symlink,
            req.unique().0,
            parent,
            [name],
            target.len(),
            None,
            MutationReply::Entry(reply, None),
            move |parent, [name], _| {
                create::symlink(parent, name, &target, page).map(MutationInput::Named)
            },
        );
    }
    fn rename(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        new_parent: INodeNo,
        new_name: &OsStr,
        flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
        let _timed = self.accounting.timed(Opcode::Rename);
        let identity = self.identity;
        let flags = flags.bits();
        self.named(
            Opcode::Rename,
            req.unique().0,
            parent,
            [name, new_name],
            0,
            None,
            MutationReply::Done(reply),
            move |parent, [name, new_name], _| {
                let new_parent = Self::other(identity, new_parent)?;
                rename::rename(parent, name, new_parent, new_name, flags).map(MutationInput::Named)
            },
        );
    }
    fn link(
        &self,
        req: &Request,
        inode: INodeNo,
        new_parent: INodeNo,
        new_name: &OsStr,
        reply: ReplyEntry,
    ) {
        let _timed = self.accounting.timed(Opcode::Link);
        let identity = self.identity;
        let expected = identity.serial(inode).ok();
        self.named(
            Opcode::Link,
            req.unique().0,
            new_parent,
            [new_name],
            0,
            None,
            MutationReply::Entry(reply, expected),
            move |parent, [name], _| {
                let serial = Self::other(identity, inode)?;
                Ok(MutationInput::Named(link::link(serial, parent, name)))
            },
        );
    }
    fn write(
        &self,
        req: &Request,
        inode: INodeNo,
        handle: FileHandle,
        offset: u64,
        data: &[u8],
        flags: WriteFlags,
        _: OpenFlags,
        _: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        let _timed = self.accounting.timed(Opcode::Write);
        self.accounting.write_size(data.len());
        if data.len() > layerfs_overlay::WRITE_WINDOW {
            return self.refuse(Opcode::Write, reply, ReplyWrite::error, Errno::EINVAL);
        }
        let reply = MutationReply::Written(reply, data.len());
        let Some((permit, reply)) =
            self.receive_reply(Opcode::Write, data.len(), reply, MutationReply::error)
        else {
            return;
        };
        let Some((serial, reply)) = self.serial(inode, reply, MutationReply::error) else {
            return;
        };
        // The one copy of the window. A store from a shared mapping carries
        // the per-request page-cache flag and is accepted as such.
        let cached = flags.contains(WriteFlags::FUSE_WRITE_CACHE);
        if cached {
            self.accounting.store_unit();
        }
        let input = write::write(offset, data, cached);
        self.mutate(
            permit,
            req.unique().0,
            serial,
            Some(handle.0),
            None,
            reply,
            Box::new(move |_| input),
        );
    }
    fn create(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        let _timed = self.accounting.timed(Opcode::Create);
        let writable = OpenFlags(flags).acc_mode() != OpenAccMode::O_RDONLY;
        self.named(
            Opcode::Create,
            req.unique().0,
            parent,
            [name],
            0,
            Some(writable),
            MutationReply::Created(reply),
            move |parent, [name], _| Ok(MutationInput::Named(create::create(parent, name, mode))),
        );
    }
    // Preallocation and in-kernel range copies are not implemented: the kernel
    // remembers the answer and copies through ordinary READ and WRITE instead.
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
        let _timed = self.accounting.timed(Opcode::Fallocate);
        self.refuse(Opcode::Fallocate, reply, ReplyEmpty::error, absent());
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
        let _timed = self.accounting.timed(Opcode::CopyFileRange);
        self.refuse(Opcode::CopyFileRange, reply, ReplyWrite::error, absent());
    }

    // No capability for these is negotiated; the kernel keeps them local or
    // falls back. ENOSYS states that explicitly rather than by default.
    fn access(&self, _: &Request, _: INodeNo, _: AccessFlags, reply: ReplyEmpty) {
        let _timed = self.accounting.timed(Opcode::Access);
        self.refuse(Opcode::Access, reply, ReplyEmpty::error, absent());
    }
    fn readdirplus(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: u64,
        reply: ReplyDirectoryPlus,
    ) {
        let _timed = self.accounting.timed(Opcode::Readdirplus);
        self.refuse(
            Opcode::Readdirplus,
            reply,
            ReplyDirectoryPlus::error,
            absent(),
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
        let _timed = self.accounting.timed(Opcode::Getlk);
        self.refuse(Opcode::Getlk, reply, ReplyLock::error, absent());
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
        let _timed = self.accounting.timed(Opcode::Setlk);
        self.refuse(Opcode::Setlk, reply, ReplyEmpty::error, absent());
    }
    fn bmap(&self, _: &Request, _: INodeNo, _: u32, _: u64, reply: ReplyBmap) {
        let _timed = self.accounting.timed(Opcode::Bmap);
        self.refuse(Opcode::Bmap, reply, ReplyBmap::error, absent());
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
        let _timed = self.accounting.timed(Opcode::Ioctl);
        self.refuse(Opcode::Ioctl, reply, ReplyIoctl::error, absent());
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
        let _timed = self.accounting.timed(Opcode::Poll);
        self.refuse(Opcode::Poll, reply, ReplyPoll::error, absent());
    }
    fn lseek(&self, _: &Request, _: INodeNo, _: FileHandle, _: i64, _: i32, reply: ReplyLseek) {
        let _timed = self.accounting.timed(Opcode::Lseek);
        self.refuse(Opcode::Lseek, reply, ReplyLseek::error, absent());
    }
}
