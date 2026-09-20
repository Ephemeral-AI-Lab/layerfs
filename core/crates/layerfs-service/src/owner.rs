//! Configured Store authority and process-wide Q=0 admission.
use crate::operation::dispatch;
use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_storage::Store;
use layerfs_telemetry::operation::{Diagnostic, OperationRecorder};
use std::{
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
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
}
pub struct Service {
    stores: Vec<StoreAccess>,
    active: AtomicBool,
    recorder: OperationRecorder,
}
struct Active<'a>(&'a AtomicBool);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
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
            active: AtomicBool::new(false),
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
        self.recorder.run(r.id, r.operation.label(), |scope| {
            r.validate()?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Code::Denied)?
                .as_secs();
            let store = self
                .stores
                .iter()
                .find(|s| s.id == r.store)
                .ok_or(Code::Denied)?;
            if !store.grants.iter().any(|g| {
                g.public_key == *peer.public_key()
                    && g.expires_unix > now
                    && g.operations & (1 << (r.operation.opcode() - 1)) != 0
            }) {
                return Err(Code::Denied.into());
            }
            if self
                .active
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Err(Code::Capacity.into());
            }
            let _active = Active(&self.active);
            let deadline = Instant::now() + Duration::from_millis(r.deadline_ms as u64);
            dispatch(&store.store, r, input, output, deadline, scope)
        })
    }
}
