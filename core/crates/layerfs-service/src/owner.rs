//! Configured Store authority, per-Store write admission and the service read bound.
//!
//! One logical mutation holds exactly one writer permit of the Store it targets.
//! The permit count is not a second policy: it is the Store's own persisted
//! `max_concurrent_writes` (#216), read once when the service is assembled, so
//! every sandbox pointing at that Store competes for the same budget while
//! another Store keeps its own. A read-only operation takes no writer permit at
//! all; it holds one of the process's bounded read permits, which exist because a
//! read owns a decode workspace and a connection, not because writers are busy.
use crate::operation::dispatch;
use layerfs_bridge::contract::*;
use layerfs_history::HistoryCatalog;
use layerfs_storage::Store;
use layerfs_telemetry::operation::{Diagnostic, OperationRecorder};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
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
/// One Store's writer budget and its live writer count.
struct WriteBudget {
    live: AtomicUsize,
    limit: usize,
}
/// One admitted operation's permit; released when the operation ends.
struct Permit<'a>(&'a AtomicUsize);
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}
/// Takes one permit without waiting; the refusal is explicit and bounded.
fn admit(counter: &AtomicUsize, limit: usize) -> Result<Permit<'_>, Failure> {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
            (live < limit).then_some(live + 1)
        })
        .map_err(|_| Code::Capacity)?;
    Ok(Permit(counter))
}
pub struct Service {
    stores: Vec<StoreAccess>,
    import_root: Option<PathBuf>,
    /// Writer budgets, index-aligned with `stores`.
    writers: Vec<WriteBudget>,
    readers: AtomicUsize,
    recorder: OperationRecorder,
}
impl Service {
    pub fn new(stores: Vec<StoreAccess>, recorder: OperationRecorder) -> Result<Self, Failure> {
        if stores.is_empty() || stores.len() > 4 {
            return Err(Code::Capacity.into());
        }
        let mut writers = Vec::with_capacity(stores.len());
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
            // The Store file owns the writer budget. The service reads it here so
            // its admission is the same number the storage layer enforces, and it
            // refuses rather than waiting when that number is reached.
            let limit = s
                .store
                .max_concurrent_writes()
                .map_err(crate::operation::failure::storage)?;
            writers.push(WriteBudget {
                live: AtomicUsize::new(0),
                limit: usize::from(limit),
            });
        }
        Ok(Self {
            stores,
            import_root: None,
            writers,
            readers: AtomicUsize::new(0),
            recorder,
        })
    }
    /// Operator-owned source directory for one public native import request.
    pub fn set_import_root(&mut self, root: &Path) -> Result<(), Failure> {
        let metadata = std::fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(Code::InvalidInput.into());
        }
        self.import_root = Some(root.canonicalize()?);
        Ok(())
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
            // Daemon control has no Store permission bit or service admission.
            if matches!(
                r.operation,
                Operation::WorkspaceStatus { .. }
                    | Operation::WorkspaceUnmount { .. }
                    | Operation::WorkspaceCloseClean { .. }
                    | Operation::WorkspaceCommit { .. }
                    | Operation::WorkspaceAttach { .. }
                    | Operation::WorkspaceMount { .. }
            ) {
                return Err(Code::Unsupported.into());
            }
            if Instant::now() >= deadline {
                return Err(Code::Deadline.into());
            }
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Code::Denied)?
                .as_secs();
            let index = self
                .stores
                .iter()
                .position(|s| s.id == r.store)
                .ok_or(Code::Denied)?;
            let store = &self.stores[index];
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
            // One permit per logical operation, taken once for the whole request:
            // a mutation crossing service and storage, or a command passing
            // through several sequential saves, is charged exactly once here. The
            // classification is the contract's own read-only predicate, so no
            // opcode list is duplicated.
            let budget = &self.writers[index];
            let _permit = match r.operation.read_only() {
                true => admit(&self.readers, MAX_READ_OPERATIONS)?,
                false => admit(&budget.live, budget.limit)?,
            };
            dispatch(
                &store.store,
                store.history.as_deref(),
                self.import_root.as_deref(),
                r,
                input,
                output,
                deadline,
                scope,
            )
        })
    }
}
