//! Serving-scope Save registry over independent initialized Storage handles.
use super::{
    Authorization, Binding, Completion, CompletionPhase, ObjectReply, Runtime, RuntimeError,
    RuntimeResult, SaveId,
};
use layerfs_content::{FilesystemRoot, FinalizedObject, InodeScope, ObjectId, ObjectRole};
use layerfs_history::{BranchId, HistoryCatalog, WorkspaceId};
use layerfs_persistence::HistoryProvider;
use layerfs_storage::{save::Save, Storage, StoragePolicy};
use layerfs_telemetry::timer::TimingScope;

pub(super) struct Slot<'a> {
    pub(super) serial: u64,
    pub(super) binding: Option<Binding>,
    pub(super) save: Option<Save<'a>>,
    pub(super) completion: Option<Completion>,
    pub(super) history: super::handlers::history::HistoryReceipts,
}

/// One live host serving scope, independent of connection or command duration.
///
/// The application retains this registry across sequential/concurrent RPCs.
/// Methods are bounded synchronous service units; dispatch/framing/queue fairness
/// and disconnect fences must be provided by the host transport adapter.
pub struct Sessions<'a> {
    owners: &'a [Storage],
    pub(super) demand: &'a Storage,
    pub(super) history: &'a HistoryProvider,
    authority: &'a dyn Authorization,
    owner: [u8; 32],
    next_serial: &'a mut u64,
    pub(super) slots: Vec<Slot<'a>>,
    pub(super) service_owners: std::rc::Rc<std::cell::Cell<usize>>,
}
impl<'a> Sessions<'a> {
    pub(super) fn capability_serial(&mut self) -> RuntimeResult<u64> {
        let serial = *self.next_serial;
        *self.next_serial = serial
            .checked_add(1)
            .ok_or(RuntimeError::Invalid("runtime capability exhaustion"))?;
        Ok(serial)
    }

    pub(super) fn new(runtime: &'a mut Runtime) -> Self {
        let slots = runtime
            .owners
            .iter()
            .map(|_| Slot {
                serial: 0,
                binding: None,
                save: None,
                completion: None,
                history: Default::default(),
            })
            .collect();
        Self {
            owners: &runtime.owners,
            demand: &runtime.demand,
            history: &runtime.history,
            authority: runtime.authority.as_ref(),
            owner: runtime.incarnation,
            next_serial: &mut runtime.next_serial,
            slots,
            service_owners: std::rc::Rc::new(std::cell::Cell::new(0)),
        }
    }

    /// Authorizes one peer/Workspace/Branch and demand-loads only its root.
    /// The peer is established by the native bridge's completed KK handshake.
    pub fn bind(
        &self,
        peer: &layerfs_bridge::native::VerifiedPeer,
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> RuntimeResult<Binding> {
        let peer = peer.public_key();
        if peer == [0; 32] {
            return Err(RuntimeError::Invalid("authenticated peer"));
        }
        self.authority.workspace(peer, workspace, branch)?;
        let snapshot = self
            .history
            .branch_snapshot(branch)?
            .ok_or(RuntimeError::Invalid("missing Branch"))?;
        self.authority
            .objects(peer, workspace, branch, &[snapshot.effective_root])?;
        let reader = self.demand.reader()?;
        let mut values = reader.read_objects(&[snapshot.effective_root])?;
        if values.len() != 1 {
            return Err(RuntimeError::Invalid("root demand cardinality"));
        }
        let canonical = values
            .pop()
            .ok_or(RuntimeError::Invalid("missing root reply"))?;
        let root = FilesystemRoot::decode(&canonical)?;
        if root.scope().object() != snapshot.scope || root.profile() != snapshot.profile {
            return Err(RuntimeError::Invalid("Branch root scope/profile"));
        }
        Ok(Binding {
            owner: self.owner,
            peer,
            workspace,
            catalog: self.history.catalog_id(),
            incarnation: self.history.incarnation(),
            root_serial: root.root_inode().serial(),
            snapshot,
        })
    }

    /// Persisted selected policy; no provider reopen or query.
    pub fn policy(&self, binding: &Binding) -> RuntimeResult<StoragePolicy> {
        self.check_binding(binding)?;
        Ok(self.demand.policy())
    }
    /// Returns the original captured binding under current authority. This does
    /// not reread a Branch, reopen a provider or change construction expectations.
    pub fn bound_snapshot(&self, binding: &Binding) -> RuntimeResult<Binding> {
        self.check_binding(binding)?;
        Ok(binding.clone())
    }

    /// One authorized saved-file length window, without whole-file payload reads.
    /// Trusts owning Store metadata; this does not attest payload integrity.
    pub fn file_lengths(
        &self,
        binding: &Binding,
        ids: &[ObjectId],
        reply: &mut dyn super::LengthReply,
    ) -> RuntimeResult<()> {
        self.check_binding(binding)?;
        if ids.len() > self.demand.capacities().read_objects {
            return Err(RuntimeError::Invalid("file length window"));
        }
        self.authority.objects(
            binding.peer,
            binding.workspace,
            binding.snapshot.branch.id,
            ids,
        )?;
        let lengths = self.demand.reader()?.file_lengths(ids)?;
        if lengths.len() != ids.len() {
            return Err(RuntimeError::Invalid("file length cardinality"));
        }
        for (id, length) in ids.iter().zip(lengths) {
            reply.file_length(*id, length)?;
        }
        Ok(())
    }

    /// Admits one Save. An unavailable slot has no provider effect.
    pub fn begin(&mut self, binding: &Binding) -> RuntimeResult<SaveId> {
        self.check_binding(binding)?;
        let index = self
            .slots
            .iter()
            .position(|slot| slot.binding.is_none())
            .ok_or(RuntimeError::AdmissionUnavailable)?;
        let serial = *self.next_serial;
        let next = serial
            .checked_add(1)
            .ok_or(RuntimeError::Invalid("Save capability exhaustion"))?;
        // Burn exposed serials, including a failed begin, without any replay.
        *self.next_serial = next;
        let save = self.owners[index].begin_save()?;
        self.slots[index] = Slot {
            serial,
            binding: Some(binding.clone()),
            save: Some(save),
            completion: None,
            history: Default::default(),
        };
        Ok(SaveId {
            owner: self.owner,
            slot: index,
            serial,
        })
    }

    /// One semantically admitted object; no total Save/file/edit counter limit.
    pub fn accept(
        &mut self,
        binding: &Binding,
        id: SaveId,
        claimed: ObjectId,
        role: ObjectRole,
        canonical: Vec<u8>,
        timing: TimingScope<'_>,
    ) -> RuntimeResult<ObjectId> {
        self.check_binding(binding)?;
        self.slot(binding, id)?;
        if self.slots[id.slot].completion.is_some() {
            return Err(RuntimeError::AlreadyAttempted);
        }
        let object = match FinalizedObject::admit(
            claimed,
            role,
            canonical,
            self.owners[id.slot].policy().construction(),
            InodeScope::from_object(binding.snapshot.scope),
            timing,
        ) {
            Ok(object) => object,
            Err(error) => {
                self.slots[id.slot].completion = Some(Completion {
                    phase: CompletionPhase::Accept,
                    outcome: Err(RuntimeError::Content(error.clone())),
                });
                return Err(error.into());
            }
        };
        let mut refs = Vec::with_capacity(1 + object.references().len());
        refs.push(claimed);
        refs.extend_from_slice(object.references());
        self.authority.objects(
            binding.peer,
            binding.workspace,
            binding.snapshot.branch.id,
            &refs,
        )?;
        let save = self.slots[id.slot]
            .save
            .as_ref()
            .ok_or(RuntimeError::AlreadyAttempted)?;
        if let Err(error) = save.accept(object) {
            let error = std::sync::Arc::new(error);
            self.slots[id.slot].completion = Some(Completion {
                phase: CompletionPhase::Accept,
                outcome: Err(RuntimeError::Storage(error.clone())),
            });
            return Err(RuntimeError::Storage(error));
        }
        Ok(claimed)
    }

    /// One bounded saved or same-Save demand, streamed to a borrowed reply sink.
    pub fn read_objects(
        &self,
        binding: &Binding,
        save: Option<SaveId>,
        ids: &[ObjectId],
        reply: &mut dyn ObjectReply,
    ) -> RuntimeResult<()> {
        self.check_binding(binding)?;
        if ids.len() > layerfs_content::filesystem::objects::MAXIMUM_READ_DEMANDS {
            return Err(RuntimeError::Invalid("object demand window"));
        }
        self.authority.objects(
            binding.peer,
            binding.workspace,
            binding.snapshot.branch.id,
            ids,
        )?;
        let values = if let Some(id) = save {
            self.slot(binding, id)?
                .save
                .as_ref()
                .ok_or(RuntimeError::AlreadyAttempted)?
                .read_objects(ids)?
        } else {
            self.demand.reader()?.read_objects(ids)?
        };
        if values.len() != ids.len() {
            return Err(RuntimeError::Invalid("provider demand cardinality"));
        }
        for (id, canonical) in ids.iter().zip(&values) {
            reply.object(*id, canonical)?;
        }
        Ok(())
    }

    /// Attempts finish once and retains the exact borrowed completion receipt.
    pub fn finish(&mut self, binding: &Binding, id: SaveId) -> RuntimeResult<&Completion> {
        self.check_binding(binding)?;
        self.slot(binding, id)?;
        let slot = &mut self.slots[id.slot];
        if slot.completion.is_some() {
            return Err(RuntimeError::AlreadyAttempted);
        }
        let save = slot.save.take().ok_or(RuntimeError::AlreadyAttempted)?;
        slot.completion = Some(Completion {
            phase: CompletionPhase::Finish,
            outcome: save.finish().map_err(Into::into),
        });
        Ok(slot.completion.as_ref().expect("finish stored completion"))
    }

    /// Reads a receipt after a lost reply without repeating its operation.
    pub fn completion(&self, binding: &Binding, id: SaveId) -> RuntimeResult<&Completion> {
        self.check_binding(binding)?;
        self.slot(binding, id)?
            .completion
            .as_ref()
            .ok_or(RuntimeError::Invalid("Save not completed"))
    }

    /// Explicitly ends an active producer; acknowledged waves are not deleted.
    pub fn abort(&mut self, binding: &Binding, id: SaveId) -> RuntimeResult<&Completion> {
        self.check_binding(binding)?;
        self.slot(binding, id)?;
        let slot = &mut self.slots[id.slot];
        if slot.completion.is_some() {
            return Err(RuntimeError::AlreadyAttempted);
        }
        slot.save.take();
        slot.completion = Some(Completion {
            phase: CompletionPhase::Abort,
            outcome: Err(layerfs_storage::StorageError::Aborted.into()),
        });
        Ok(slot.completion.as_ref().expect("abort stored completion"))
    }

    /// Acknowledges a known terminal receipt and releases its local slot only.
    /// Unknown publication/failed cleanup cannot be released by this operation.
    pub fn release(&mut self, binding: &Binding, id: SaveId) -> RuntimeResult<()> {
        self.check_binding(binding)?;
        let completion = self
            .slot(binding, id)?
            .completion
            .as_ref()
            .ok_or(RuntimeError::Invalid("active Save release"))?;
        if completion.retains_custody() || self.slots[id.slot].history.retains_custody() {
            return Err(RuntimeError::RetainedCustody);
        }
        self.slots[id.slot] = Slot {
            serial: 0,
            binding: None,
            save: None,
            completion: None,
            history: Default::default(),
        };
        Ok(())
    }

    pub(super) fn check_binding(&self, binding: &Binding) -> RuntimeResult<()> {
        if binding.owner != self.owner
            || binding.catalog != self.history.catalog_id()
            || binding.incarnation != self.history.incarnation()
        {
            return Err(RuntimeError::StaleCapability);
        }
        self.authority
            .workspace(binding.peer, binding.workspace, binding.snapshot.branch.id)
    }
    pub(super) fn slot(&self, binding: &Binding, id: SaveId) -> RuntimeResult<&Slot<'a>> {
        let slot = self
            .slots
            .get(id.slot)
            .ok_or(RuntimeError::StaleCapability)?;
        if id.owner != self.owner
            || slot.serial != id.serial
            || slot.binding.as_ref() != Some(binding)
        {
            return Err(RuntimeError::StaleCapability);
        }
        Ok(slot)
    }
}
