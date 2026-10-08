//! Borrowed kernel input enters fixed receive accounting before owned copies.
use super::{
    reply::ReadReply,
    state::{failed, NativeFilesystem},
};
use crate::RequestDisposition;
use fuser::{
    Errno, FileHandle, Filesystem, INodeNo, KernelConfig, LockOwner, OpenAccMode, OpenFlags,
    ReplyAttr, ReplyData, ReplyEmpty, ReplyEntry, ReplyOpen, Request,
};
use layerfs_content::filesystem::PathName;
use layerfs_workspace::NativeReadOperation;
use std::{ffi::OsStr, io, os::unix::ffi::OsStrExt};

impl Filesystem for NativeFilesystem {
    fn init(&mut self, _: &Request, config: &mut KernelConfig) -> io::Result<()> {
        let negotiated = crate::mount::negotiate(config)?;
        self.negotiation
            .set(negotiated)
            .map_err(|_| io::Error::other("duplicate native initialization"))
    }
    fn lookup(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let Some((permit, reply)) = self.admit(ReadReply::Entry(reply), name.as_bytes().len())
        else {
            return;
        };
        let parent = match self.identity.serial(parent) {
            Ok(parent) => parent,
            Err(error) => {
                reply.error(error);
                return;
            }
        };
        let name = match PathName::from_bytes(name.as_bytes()) {
            Ok(name) => name,
            Err(_) => {
                reply.error(Errno::EINVAL);
                return;
            }
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
    fn getattr(&self, req: &Request, inode: INodeNo, handle: Option<FileHandle>, reply: ReplyAttr) {
        let Some((permit, reply)) = self.admit(ReadReply::Attr(reply), 0) else {
            return;
        };
        let serial = match self.identity.serial(inode) {
            Ok(serial) => serial,
            Err(error) => {
                reply.error(error);
                return;
            }
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
    fn open(&self, req: &Request, inode: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        let Some((permit, reply)) = self.admit(ReadReply::Open(reply, false), 0) else {
            return;
        };
        let serial = match self.identity.serial(inode) {
            Ok(serial) => serial,
            Err(error) => {
                reply.error(error);
                return;
            }
        };
        self.submit(
            permit,
            req.unique().0,
            serial,
            None,
            NativeReadOperation::Open {
                serial,
                writable: flags.acc_mode() != OpenAccMode::O_RDONLY,
            },
            reply,
        );
    }
    fn opendir(&self, req: &Request, inode: INodeNo, _: OpenFlags, reply: ReplyOpen) {
        let Some((permit, reply)) = self.admit(ReadReply::Open(reply, true), 0) else {
            return;
        };
        let serial = match self.identity.serial(inode) {
            Ok(serial) => serial,
            Err(error) => {
                reply.error(error);
                return;
            }
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
        let Some((permit, reply)) = self.admit(ReadReply::Data(reply, offset, size), 0) else {
            return;
        };
        if size as usize > layerfs_overlay::READ_WINDOW {
            reply.error(Errno::EINVAL);
            return;
        }
        let serial = match self.identity.serial(inode) {
            Ok(serial) => serial,
            Err(error) => {
                reply.error(error);
                return;
            }
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
    fn readlink(&self, req: &Request, inode: INodeNo, reply: ReplyData) {
        let Some((permit, reply)) = self.admit(ReadReply::Link(reply), 0) else {
            return;
        };
        let serial = match self.identity.serial(inode) {
            Ok(serial) => serial,
            Err(error) => {
                reply.error(error);
                return;
            }
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
    fn forget(&self, req: &Request, inode: INodeNo, count: u64) {
        let Ok(received) = self.queue.receive() else {
            return;
        };
        let Ok(permit) = received.admit(0) else {
            return;
        };
        let mount = self.queue.identity();
        let serial = self.identity.serial(inode);
        let services = self.services.clone();
        let request = req.unique().0;
        let _ = permit.handoff(Box::pin(async move {
            let outcome = async {
                let serial = serial.map_err(|error| io::Error::from_raw_os_error(error.code()))?;
                let services = services.request()?;
                drop(services.forget(mount, serial, count).await?);
                Ok::<_, crate::ports::ServiceError>(())
            }
            .await;
            match outcome {
                Ok(()) => RequestDisposition::Complete,
                Err(error) => failed(request, mount, error),
            }
        }));
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
        let received = match self.queue.receive() {
            Ok(received) => received,
            Err(_) => {
                reply.error(Errno::EIO);
                return;
            }
        };
        let permit = match received.admit(0) {
            Ok(permit) => permit,
            Err(failure) => {
                reply.error(Errno::ENOTCONN);
                drop(failure);
                return;
            }
        };
        let mount = self.queue.identity();
        let serial = self.identity.serial(inode);
        let services = self.services.clone();
        let request = req.unique().0;
        let _ = permit.handoff(Box::pin(async move {
            let outcome = async {
                let serial = serial.map_err(|error| io::Error::from_raw_os_error(error.code()))?;
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
                    failed(request, mount, error)
                }
            }
        }));
    }
}
