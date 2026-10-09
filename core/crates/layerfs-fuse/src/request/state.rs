//! Per-mount callback state; the request worker pool remains daemon-wide.
use super::accounting::{Accounting, Disposal, Opcode};
use super::failure::{failed, KernelInput};
use super::reply::ReadReply;
use crate::{
    attributes::Identity,
    mount::Negotiation,
    ports::{Fence, MountServices},
    DispatchError, MountQueue, Permit, RequestFuture,
};
use fuser::{Errno, INodeNo};
use layerfs_workspace::NativeReadOperation;
use std::sync::{Arc, OnceLock};

pub struct NativeFilesystem {
    pub(super) queue: MountQueue,
    /// The lane's terminal stop, handed to every request's services.
    pub(super) fence: Fence,
    pub(super) services: Arc<dyn MountServices>,
    pub(super) identity: Identity,
    pub(super) negotiation: Arc<OnceLock<Negotiation>>,
    pub(super) accounting: Arc<Accounting>,
}
impl NativeFilesystem {
    pub fn new(
        queue: MountQueue,
        services: Arc<dyn MountServices>,
        identity: Identity,
    ) -> Result<Self, DispatchError> {
        if identity.root != queue.identity().root_serial() {
            return Err(DispatchError::Stale);
        }
        Ok(Self {
            fence: queue.fence(),
            queue,
            services,
            identity,
            negotiation: Arc::new(OnceLock::new()),
            accounting: Arc::new(Accounting::default()),
        })
    }
    pub fn negotiation(&self) -> Arc<OnceLock<Negotiation>> {
        self.negotiation.clone()
    }
    pub fn accounting(&self) -> Arc<Accounting> {
        self.accounting.clone()
    }
    pub(super) fn admit(
        &self,
        opcode: Opcode,
        reply: ReadReply,
        bytes: usize,
    ) -> Option<(Permit, ReadReply)> {
        self.admit_reply(opcode, bytes, reply, ReadReply::error)
    }
    /// This handoff-capacity wait is the only wait of a receive loop. A refusal
    /// makes exactly one error attempt on the still-borrowed reply.
    pub(super) fn admit_reply<R>(
        &self,
        opcode: Opcode,
        bytes: usize,
        reply: R,
        error: fn(R, Errno),
    ) -> Option<(Permit, R)> {
        self.accounting.opcode(opcode);
        let received = match self.queue.receive() {
            Ok(received) => received,
            Err(_) => {
                error(reply, Errno::EIO);
                self.accounting.disposed(Disposal::Terminal);
                return None;
            }
        };
        match received.admit(bytes) {
            Ok(permit) => Some((permit, reply)),
            Err(failure) => {
                error(reply, Errno::ENOTCONN);
                self.accounting.disposed(Disposal::Terminal);
                drop(failure);
                None
            }
        }
    }
    /// Even a shutdown-racing handoff retains this exact future and reply.
    pub(super) fn handoff(&self, permit: Permit, future: RequestFuture) {
        // Counted before the first step, which may reply on this thread.
        self.accounting.disposed(Disposal::Handoff);
        let _ = permit.handoff(future);
    }
    /// An admitted unit decided on the loop: its permit returns unused.
    pub(super) fn serial<R>(
        &self,
        inode: INodeNo,
        reply: R,
        error: fn(R, Errno),
    ) -> Option<(u64, R)> {
        match self.identity.serial(inode) {
            Ok(serial) => Some((serial, reply)),
            Err(errno) => {
                self.refused(reply, error, errno);
                None
            }
        }
    }
    pub(super) fn refused<R>(&self, reply: R, error: fn(R, Errno), errno: Errno) {
        error(reply, errno);
        self.accounting.disposed(Disposal::Refused);
    }
    /// Complete inline reply with no engine job; holds one receive unit only.
    pub(super) fn inline<R>(
        &self,
        opcode: Opcode,
        reply: R,
        error: fn(R, Errno),
        respond: impl FnOnce(R),
    ) {
        self.accounting.opcode(opcode);
        match self.queue.receive() {
            Ok(received) => {
                respond(reply);
                self.accounting.disposed(Disposal::Inline);
                drop(received);
            }
            Err(_) => {
                error(reply, Errno::EIO);
                self.accounting.disposed(Disposal::Terminal);
            }
        }
    }
    /// Declared refusal: one explicit errno, never the library's default path.
    pub(super) fn refuse<R>(&self, opcode: Opcode, reply: R, error: fn(R, Errno), errno: Errno) {
        self.accounting.opcode(opcode);
        match self.queue.receive() {
            Ok(received) => {
                self.refused(reply, error, errno);
                drop(received);
            }
            Err(_) => {
                error(reply, Errno::EIO);
                self.accounting.disposed(Disposal::Terminal);
            }
        }
    }
    pub(super) fn submit(
        &self,
        permit: Permit,
        request: u64,
        protected: u64,
        handle: Option<u64>,
        operation: NativeReadOperation,
        reply: ReadReply,
    ) {
        let services = self.services.clone();
        let fence = self.fence.clone();
        let mount = self.queue.identity();
        let identity = self.identity;
        self.handoff(
            permit,
            Box::pin(async move {
                let services = match services.request(&fence) {
                    Ok(services) => services,
                    Err(error) => {
                        let data = reply.data_input();
                        reply.error(Errno::EIO);
                        return failed(
                            request,
                            mount,
                            KernelInput::Read {
                                protected,
                                handle,
                                operation,
                                data,
                            },
                            error,
                        );
                    }
                };
                reply
                    .serve(
                        services, fence, mount, request, protected, handle, operation, identity,
                    )
                    .await
            }),
        );
    }
}
