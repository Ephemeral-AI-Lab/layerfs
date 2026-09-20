//! Configured Store authority and service-wide Q=0 admission.
use crate::operation::dispatch;
use layerfs_bridge::contract::*;
use layerfs_history::HistoryCatalog;
use layerfs_storage::Store;
use layerfs_telemetry::operation::{Diagnostic, OperationRecorder};
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub struct Grant {
    pub public_key: [u8; 32],
    pub operations: u8,
    pub expires_unix: u64,
}
pub struct StoreAccess {
    pub id: u32,
    pub store: Store,
    pub grants: Vec<Grant>,
    /// History catalog bound to this Store, when the operator configured one.
    ///
    /// It is shared by every connection of this service process, which is what
    /// lets a stage be created on one connection and committed on another while
    /// the same continuing authority owns the serials.
    pub history: Option<Arc<dyn HistoryCatalog>>,
}
pub struct Service {
    stores: Vec<StoreAccess>,
    active: AtomicUsize,
    recorder: OperationRecorder,
}
struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}
impl Service {
    pub fn new(stores: Vec<StoreAccess>, recorder: OperationRecorder) -> Result<Self, Failure> {
        if stores.is_empty() || stores.len() > 4 {
            return Err(Code::Capacity.into());
        }
        for (i, s) in stores.iter().enumerate() {
            if s.store.policy() != Store::default_policy() {
                return Err(Code::Unsupported.into());
            }
            if s.grants.is_empty()
                || s.grants.len() > 16
                || stores[..i].iter().any(|v| v.id == s.id)
            {
                return Err(Code::InvalidInput.into());
            }
        }
        Ok(Self {
            stores,
            active: AtomicUsize::new(0),
            recorder,
        })
    }
    /// Direct and remote calls enter this exact authorization/admission/operation body.
    /// Input must terminate only at its validated boundary. No output byte is final
    /// before the returned successful Response; mutations are never automatically retried.
    pub fn handle(
        &self,
        peer: &VerifiedPeer,
        r: &Request,
        input: &mut dyn Read,
        output: &mut dyn Write,
    ) -> (Result<Response, Failure>, Diagnostic) {
        let deadline = Instant::now() + Duration::from_millis(r.deadline_ms as u64);
        self.handle_until(peer, r, input, output, deadline)
    }

    /// Shares the native adapter's local deadline with the authorized handler.
    pub fn handle_until(
        &self,
        peer: &VerifiedPeer,
        r: &Request,
        input: &mut dyn Read,
        output: &mut dyn Write,
        deadline: Instant,
    ) -> (Result<Response, Failure>, Diagnostic) {
        let deadline = deadline.min(Instant::now() + Duration::from_millis(r.deadline_ms as u64));
        self.recorder.run(r.id, r.operation.label(), |scope| {
            r.validate()?;
            if Instant::now() >= deadline {
                return Err(Code::Deadline.into());
            }
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Code::Denied)?
                .as_secs();
            let store = self
                .stores
                .iter()
                .find(|s| s.id == r.store)
                .ok_or(Code::Denied)?;
            // The permission mapping is checked and exhaustive: an opcode with
            // no declared bit cannot be granted by any mask, and a legacy mask
            // of 31 therefore grants neither history opcode.
            let bit = permission_bit(r.operation.opcode()).ok_or(Code::Unsupported)?;
            if !store.grants.iter().any(|g| {
                g.public_key == *peer.public_key()
                    && g.expires_unix > now
                    && g.operations & bit != 0
            }) {
                return Err(Code::Denied.into());
            }
            if self
                .active
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                    (active < MAX_OPERATIONS).then_some(active + 1)
                })
                .is_err()
            {
                return Err(Code::Capacity.into());
            }
            let _active = Active(&self.active);
            dispatch(
                &store.store,
                store.history.as_deref(),
                r,
                input,
                output,
                deadline,
                scope,
            )
        })
    }
}
