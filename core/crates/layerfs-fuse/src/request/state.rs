//! Per-mount callback state; the request worker pool remains daemon-wide.
use super::failure::{failed, KernelInput};
use super::reply::ReadReply;
use crate::{
    attributes::Identity, mount::Negotiation, ports::MountServices, DispatchError, MountQueue,
    Permit,
};
use fuser::Errno;
use layerfs_workspace::NativeReadOperation;
use std::sync::{Arc, OnceLock};

pub struct NativeFilesystem {
    pub(super) queue: MountQueue,
    pub(super) services: Arc<dyn MountServices>,
    pub(super) identity: Identity,
    pub(super) negotiation: Arc<OnceLock<Negotiation>>,
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
            queue,
            services,
            identity,
            negotiation: Arc::new(OnceLock::new()),
        })
    }
    pub fn negotiation(&self) -> Arc<OnceLock<Negotiation>> {
        self.negotiation.clone()
    }
    pub(super) fn admit(&self, reply: ReadReply, bytes: usize) -> Option<(Permit, ReadReply)> {
        self.admit_reply(bytes, reply, ReadReply::error)
    }
    pub(super) fn admit_reply<R>(
        &self,
        bytes: usize,
        reply: R,
        error: fn(R, Errno),
    ) -> Option<(Permit, R)> {
        let received = match self.queue.receive() {
            Ok(received) => received,
            Err(_) => {
                error(reply, Errno::EIO);
                return None;
            }
        };
        match received.admit(bytes) {
            Ok(permit) => Some((permit, reply)),
            Err(failure) => {
                error(reply, Errno::ENOTCONN);
                drop(failure);
                None
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
        let mount = self.queue.identity();
        let identity = self.identity;
        // Even a shutdown-racing handoff retains this exact future and reply.
        let _ = permit.handoff(Box::pin(async move {
            let services = match services.request() {
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
                    services, mount, request, protected, handle, operation, identity,
                )
                .await
        }));
    }
}
