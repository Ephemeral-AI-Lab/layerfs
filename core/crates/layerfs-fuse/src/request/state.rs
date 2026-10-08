//! Per-mount callback state; the request worker pool remains daemon-wide.
use super::reply::ReadReply;
use crate::{
    attributes::Identity,
    mount::Negotiation,
    ports::{MountServices, ServiceError},
    DispatchError, MountQueue, Permit, RequestDisposition,
};
use fuser::Errno;
use layerfs_overlay::NativeMount;
use layerfs_workspace::NativeReadOperation;
use std::{
    fmt,
    sync::{Arc, OnceLock},
};

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
        let received = match self.queue.receive() {
            Ok(received) => received,
            Err(_) => {
                reply.error(Errno::EIO);
                return None;
            }
        };
        match received.admit(bytes) {
            Ok(permit) => Some((permit, reply)),
            Err(failure) => {
                reply.error(Errno::ENOTCONN);
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
                    reply.error(Errno::EIO);
                    return RequestDisposition::Retained(Box::new(RequestFailure {
                        request,
                        mount,
                        reason: error,
                        read_input: Some((protected, handle, operation)),
                    }));
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
#[derive(Debug)]
struct RequestFailure {
    request: u64,
    mount: NativeMount,
    reason: ServiceError,
    read_input: Option<(u64, Option<u64>, NativeReadOperation)>,
}
impl fmt::Display for RequestFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "native request {} on {:?}, input {:?}: {}",
            self.request, self.mount, self.read_input, self.reason
        )
    }
}
impl std::error::Error for RequestFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.reason.as_ref())
    }
}
pub(super) fn failed(request: u64, mount: NativeMount, reason: ServiceError) -> RequestDisposition {
    RequestDisposition::Retained(Box::new(RequestFailure {
        request,
        mount,
        reason,
        read_input: None,
    }))
}
