//! Configured Store authority, authorization and typed operation dispatch.
//!
//! Content requests retain the Store's persisted writer setting. Pure history
//! metadata mutations use the exact catalog authority's separate allowance.
//! Legacy composite operations retain their existing whole-request content
//! owner until a versioned result owner can represent every completed Save.
use crate::service::{admission::Admission, construction::Construction, read, save};
use layerfs_bridge::contract::*;
use layerfs_content::filesystem::state::GraphCapacity;
use layerfs_history::HistoryCatalog;
use layerfs_storage::Store;
use layerfs_telemetry::operation::{Diagnostic, OperationRecorder};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
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
    import_root: Option<PathBuf>,
    admission: Admission,
    pub(super) construction: Vec<Arc<Construction>>,
    recorder: OperationRecorder,
}
impl Service {
    pub fn new(stores: Vec<StoreAccess>, recorder: OperationRecorder) -> Result<Self, Failure> {
        Self::with_construction_scratch(stores, recorder, GraphCapacity::default().scratch_bytes())
    }

    /// Captures one indexed construction-state disk budget per operation.
    /// Other temporary owners and the working/cache windows keep their own bounds.
    pub fn with_construction_scratch(
        stores: Vec<StoreAccess>,
        recorder: OperationRecorder,
        scratch_bytes: u64,
    ) -> Result<Self, Failure> {
        let capacity = GraphCapacity::new(scratch_bytes).map_err(crate::service::error::content)?;
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
        let admission = Admission::new(&stores)?;
        let mut construction: Vec<Arc<Construction>> = Vec::with_capacity(stores.len());
        for (index, store) in stores.iter().enumerate() {
            let shared = stores[..index]
                .iter()
                .position(|prior| prior.store.same_authority(&store.store));
            construction.push(match shared {
                Some(prior) => Arc::clone(&construction[prior]),
                None => Arc::new(Construction::new(
                    &store.store,
                    admission.content_limit(index),
                    capacity,
                )?),
            });
        }
        Ok(Self {
            stores,
            import_root: None,
            admission,
            construction,
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
        self.handle_until_bound(
            peer,
            r,
            input,
            output,
            deadline,
            self.import_root.as_deref(),
        )
    }

    /// Binds one checked host source to one local request without changing the
    /// startup root used by native daemon requests or other concurrent calls.
    pub(crate) fn handle_until_bound(
        &self,
        peer: &VerifiedPeer,
        r: &Request,
        input: &mut dyn Read,
        output: &mut dyn Write,
        deadline: Instant,
        import_root: Option<&Path>,
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
                    | Operation::WorkspaceOpen { .. }
                    | Operation::WorkspaceExec { .. }
                    | Operation::SandboxHello
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
            let _permit = self.admission.enter(index, &r.operation)?;
            dispatch(
                &store.store,
                (
                    store.history.as_deref(),
                    import_root,
                    &self.construction[index],
                ),
                r,
                input,
                output,
                deadline,
                scope,
            )
        })
    }
}

pub(crate) fn dispatch(
    store: &Store,
    history: (Option<&dyn HistoryCatalog>, Option<&Path>, &Construction),
    r: &Request,
    input: &mut dyn Read,
    output: &mut dyn Write,
    deadline: Instant,
    scope: &TimingScope<'_, Active>,
) -> Result<Response, Failure> {
    let (catalog, import_root, construction) = history;
    match &r.operation {
        Operation::WorkspaceStatus { .. }
        | Operation::WorkspaceUnmount { .. }
        | Operation::WorkspaceCloseClean { .. }
        | Operation::WorkspaceCommit { .. }
        | Operation::WorkspaceAttach { .. }
        | Operation::WorkspaceMount { .. }
        | Operation::WorkspaceOpen { .. }
        | Operation::WorkspaceExec { .. }
        | Operation::WorkspacePinView { .. }
        | Operation::WorkspaceViewLookup { .. }
        | Operation::WorkspaceViewList { .. }
        | Operation::WorkspaceViewRead { .. }
        | Operation::WorkspaceViewReadlink { .. }
        | Operation::WorkspaceViewStatus { .. }
        | Operation::WorkspaceReleaseView { .. }
        | Operation::SandboxHello => Err(Code::Unsupported.into()),
        Operation::HistoryQuery(query) => {
            end_input(input)?;
            read::catalog::query(catalog.ok_or(Code::Unsupported)?, query, store)
        }
        Operation::HistoryCommand(command) => {
            // A prepared update carries its rows in the body its own request
            // declared; every other command owns no body at all.
            if !matches!(
                command,
                HistoryCommand::Commit(_) | HistoryCommand::StageChanges(_)
            ) {
                end_input(input)?;
            }
            save::catalog::command(
                catalog.ok_or(Code::Unsupported)?,
                store,
                (import_root, construction),
                command,
                save::catalog::Streams { input, output },
                deadline,
                scope,
            )
        }
        Operation::FileSaveCapabilities => {
            end_input(input)?;
            Ok(Response::FileSaveCapabilities {
                version: SAVE_FILE_V2_VERSION,
            })
        }
        // A known successful C2 finish is never changed into a claimed abort.
        Operation::SaveFile { .. }
        | Operation::SaveFileV2 { .. }
        | Operation::ConstructSymlink { .. }
        | Operation::UpdatePortableMetadata { .. }
        | Operation::ConstructPortableMetadata { .. } => {
            save::content::mutate(store, construction, r, input, deadline, scope)
        }
        Operation::ReadFile { .. } | Operation::Inspect { .. } => {
            end_input(input)?;
            read::content::read(store, r, output, scope)
        }
    }
}

pub(crate) fn end_input(input: &mut dyn Read) -> Result<(), Failure> {
    let mut byte = [0; 1];
    if input.read(&mut byte)? != 0 {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}
