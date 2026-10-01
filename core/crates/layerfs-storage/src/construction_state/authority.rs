//! Bounded slot admission and exact retained native ownership.

use std::path::Path;
use std::sync::{Arc, Mutex};

use layerfs_content::file::edit::DraftCapacity;
use layerfs_content::filesystem::rows::BindingSourceId;
use layerfs_content::filesystem::state::{
    AliasCapacity, CanonicalCapacity, FactCapacity, GraphSubject, StateSelection,
};

use crate::error::{StorageError, StorageResult};

use super::native::{NativeDirectory, NativeFile};
use super::plan::Plan;
use super::session::{Resource, ScratchSession};
use super::status::{ScratchDisposition, ScratchOwnerStatus};

pub(crate) enum Slot {
    Empty,
    Active(ScratchOwnerStatus),
    Idle {
        resource: Resource,
        status: ScratchOwnerStatus,
    },
    Retained {
        resource: Box<Resource>,
        status: ScratchOwnerStatus,
        failure: Option<StorageError>,
    },
}

pub(crate) struct Shared {
    pub(crate) directory: Arc<Mutex<NativeDirectory>>,
    pub(crate) slots: Mutex<super::pool_state::Slots>,
    pub(crate) limit: usize,
    pub(crate) engine: Option<&'static crate::engine::EngineGuard>,
}

/// Admits exact private DirectoryRoots owners before file creation or SQL.
///
/// One authority owns a fresh random private subdirectory under the supplied
/// base. It never chmods or adopts the base's files. Creation is lazy after the
/// first 16MiB slot admission, so failed bootstrap effects retain that slot.
/// Requested owner count is explicit and bounded by the existing Store class.
#[derive(Clone)]
pub struct ScratchAuthority {
    pub(crate) shared: Arc<Shared>,
}

impl ScratchAuthority {
    /// Opens/checks the owned base and prepares a fresh private directory name.
    /// No scratch directory/file or SQL effects occur until admitted `begin`.
    pub fn new(parent: &Path, maximum_owners: usize) -> StorageResult<Self> {
        Self::new_internal(parent, maximum_owners, None)
    }

    /// Explicit participation in the established actual process engine owner.
    /// Scratch keeps its own512KiB connection profile and claims no whole-shape fit.
    pub fn new_guarded(
        parent: &Path,
        maximum_owners: usize,
        guard: &'static crate::engine::EngineGuard,
    ) -> StorageResult<Self> {
        Self::new_internal(parent, maximum_owners, Some(guard))
    }

    fn new_internal(
        parent: &Path,
        maximum_owners: usize,
        engine: Option<&'static crate::engine::EngineGuard>,
    ) -> StorageResult<Self> {
        if let Some(guard) = engine {
            guard.validate()?;
        }
        if maximum_owners == 0
            || maximum_owners > crate::policy::MAX_CONCURRENT_WRITES_LIMIT as usize
        {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch owners",
                limit: crate::policy::MAX_CONCURRENT_WRITES_LIMIT as u64,
                actual: maximum_owners as u64,
            });
        }
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(maximum_owners)
            .map_err(|_| StorageError::CapacityExceeded {
                what: "construction scratch owner table",
                limit: maximum_owners as u64,
                actual: maximum_owners as u64,
            })?;
        if slots.capacity() != maximum_owners {
            return Err(StorageError::Integrity(
                "construction scratch owner table actual capacity",
            ));
        }
        slots.resize_with(maximum_owners, || Slot::Empty);
        let directory = NativeDirectory::prepare(parent)?;
        Ok(Self {
            shared: Arc::new(Shared {
                directory: Arc::new(Mutex::new(directory)),
                slots: Mutex::new(super::pool_state::Slots {
                    entries: slots,
                    counters: super::pool_state::PoolCounters::default(),
                }),
                limit: maximum_owners,
                engine,
            }),
        })
    }

    /// Refuses the declared row/byte shape before issuing a token, reserving a
    /// slot or creating native/SQL state, then admits the fixed 16MiB class.
    pub fn begin(
        &self,
        selector: [u8; 32],
        declared_records: u64,
    ) -> StorageResult<ScratchSession> {
        if declared_records > super::profile::ROW_LIMIT {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch declared rows",
                limit: super::profile::ROW_LIMIT,
                actual: declared_records,
            });
        }
        let declared_bytes = declared_records
            .checked_mul(super::profile::RECORD_BYTES)
            .ok_or(StorageError::Integrity(
                "construction scratch declared-byte overflow",
            ))?;
        let byte_limit = super::profile::ROW_LIMIT * super::profile::RECORD_BYTES;
        if declared_bytes > byte_limit {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch declared record bytes",
                limit: byte_limit,
                actual: declared_bytes,
            });
        }
        self.admit(selector, Plan::Legacy, None)
    }

    /// Admit exclusive claims followed by roots in the same native class.
    /// Both declarations and their maximum simultaneous framed size precede
    /// token, slot, file and SQL effects. Claims never reserve a second owner.
    pub fn begin_phased(
        &self,
        selector: [u8; 32],
        directories: u64,
        bindings: u64,
    ) -> StorageResult<ScratchSession> {
        self.admit(selector, Plan::phased(directories, bindings)?, None)
    }

    /// Bind an already issued immutable row source before token/native/SQL effects.
    pub fn begin_sites(
        &self,
        selector: [u8; 32],
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
    ) -> StorageResult<ScratchSession> {
        self.admit(selector, Plan::sites(directories, bindings, source)?, None)
    }

    /// Capture one source/namespace/base/budget subject for Sites, Graph and Roots.
    /// Native conversions and D/B declarations precede token, slot and file effects.
    pub fn begin_graph(
        &self,
        selector: [u8; 32],
        directories: u64,
        bindings: u64,
        subject: GraphSubject,
    ) -> StorageResult<ScratchSession> {
        let plan = Plan::graph(directories, bindings, &subject)?;
        self.admit(selector, plan, Some(subject))
    }

    /// Capture aliases and graph in the same admitted native/working owner.
    pub fn begin_alias_graph(
        &self,
        selector: [u8; 32],
        directories: u64,
        bindings: u64,
        subject: GraphSubject,
        aliases: AliasCapacity,
    ) -> StorageResult<ScratchSession> {
        let plan = Plan::alias_graph(directories, bindings, &subject, aliases)?;
        self.admit(selector, plan, Some(subject))
    }
    /// Capture independent facts and all simultaneous namespace tables in one S.
    pub fn begin_namespace(
        &self,
        selector: [u8; 32],
        directories: u64,
        bindings: u64,
        subject: GraphSubject,
        aliases: AliasCapacity,
        facts: FactCapacity,
    ) -> StorageResult<ScratchSession> {
        let plan = Plan::namespace_graph(directories, bindings, &subject, aliases, facts)?;
        self.admit(selector, plan, Some(subject))
    }

    /// Capture namespace, canonical counts and release in one admitted native S.
    #[expect(
        clippy::too_many_arguments,
        reason = "captured independent private profile classes"
    )]
    pub fn begin_canonical(
        &self,
        selector: [u8; 32],
        directories: u64,
        bindings: u64,
        subject: GraphSubject,
        aliases: AliasCapacity,
        facts: FactCapacity,
        canonical: CanonicalCapacity,
    ) -> StorageResult<ScratchSession> {
        let plan = Plan::canonical(directories, bindings, &subject, aliases, facts, canonical)?;
        self.admit(selector, plan, Some(subject))
    }

    /// Capture the dedicated metadata draft profile before native/SQL effects.
    pub fn begin_drafts(
        &self,
        selector: [u8; 32],
        capacity: DraftCapacity,
    ) -> StorageResult<ScratchSession> {
        let capacity = capacity.validated()?;
        self.admit(selector, Plan::Drafts { capacity }, None)
    }

    fn admit(
        &self,
        selector: [u8; 32],
        plan: Plan,
        subject: Option<GraphSubject>,
    ) -> StorageResult<ScratchSession> {
        if let Some(guard) = self.shared.engine {
            guard.validate()?;
        }
        let budget = plan.scratch_bytes();
        super::native::validate_budget(budget)?;
        let selection = StateSelection::issue(selector)?;
        if let Some((slot, resource)) = self.pick_idle(&selection, plan)? {
            let mut session = ScratchSession::new(self.shared.clone(), slot, resource);
            let result = session.rebind(selection, plan, subject);
            self.complete_pool_action(super::pool_state::Action::Rebind, result.is_ok())?;
            result?;
            return Ok(session);
        }
        let status = {
            let directory = self
                .shared
                .directory
                .lock()
                .map_err(|_| StorageError::Integrity("construction scratch directory lock"))?;
            ScratchOwnerStatus {
                root_retirement: None,
                known_clean: false,
                graph_working: None,
                token: selection.token(),
                selector: *selection.selector(),
                disposition: ScratchDisposition::Admitted,
                path: directory
                    .path
                    .join(format!("state-{:016x}.sqlite", selection.token())),
                parent: directory.parent,
                directory: directory.identity,
                file: None,
                reserved_bytes: budget,
                allocated_bytes: None,
                failure: None,
                quarantined: false,
                release_attempted: false,
                failed_close_descriptor: None,
                retained: false,
            }
        };
        let slot = {
            let mut slots = self
                .shared
                .slots
                .lock()
                .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
            let Some(index) = slots.iter().position(|slot| matches!(slot, Slot::Empty)) else {
                slots.refusal()?;
                return Err(StorageError::OwnershipUnavailable);
            };
            slots.counters.fresh.begin()?;
            slots[index] = Slot::Active(status);
            index
        };
        let native =
            match NativeFile::prepare(self.shared.directory.clone(), selection.token(), budget) {
                Ok(native) => native,
                Err(original) => {
                    // Preparation has no native/SQL effects. A known empty admission
                    // can be refunded; bootstrap never enters this path.
                    match self.shared.slots.lock() {
                        Ok(mut slots) => {
                            slots.counters.fresh.finish(false)?;
                            slots[slot] = Slot::Empty;
                            return Err(original);
                        }
                        Err(_) => {
                            return Err(StorageError::CleanupFailed {
                                original: Box::new(original),
                                cleanup: Box::new(StorageError::Integrity(
                                    "construction scratch owner lock",
                                )),
                            })
                        }
                    }
                }
            };
        let resource = match Resource::new(selection, native, plan, subject, self.shared.engine) {
            Ok(resource) => resource,
            Err(error) => {
                // Native preparation has no effects; return this unused slot once.
                let mut slots = self
                    .shared
                    .slots
                    .lock()
                    .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
                slots.counters.fresh.finish(false)?;
                slots[slot] = Slot::Empty;
                return Err(error);
            }
        };
        let mut session = ScratchSession::new(self.shared.clone(), slot, resource);
        let result = session.initialize();
        let born = session
            .resource
            .as_ref()
            .is_some_and(|r| r.native.has_file_effects() && r.native.identity.is_some());
        self.complete_pool_action(super::pool_state::Action::Fresh, born)?;
        result?;
        Ok(session)
    }

    /// Bounded exact active/retained owner facts. This never opens a leftover file.
    pub fn status(&self) -> StorageResult<Vec<ScratchOwnerStatus>> {
        let slots = self
            .shared
            .slots
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(self.shared.limit)
            .map_err(|_| StorageError::Integrity("construction scratch status capacity"))?;
        for slot in slots.iter() {
            match slot {
                Slot::Empty => {}
                Slot::Active(status)
                | Slot::Idle { status, .. }
                | Slot::Retained { status, .. } => output.push(status.clone()),
            }
        }
        Ok(output)
    }

    /// All admitted classes, including failed/Unknown/native-release failures.
    pub fn reserved_bytes(&self) -> StorageResult<u64> {
        let slots = self
            .shared
            .slots
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
        slots.iter().try_fold(0u64, |total, slot| {
            let bytes = match slot {
                Slot::Empty => 0,
                Slot::Active(status)
                | Slot::Idle { status, .. }
                | Slot::Retained { status, .. } => status.reserved_bytes,
            };
            total.checked_add(bytes).ok_or(StorageError::Integrity(
                "construction scratch aggregate reservation overflow",
            ))
        })
    }

    /// Takes the original typed adapter error once from a retained owner.
    /// Native disposition and credit remain unchanged when the error is handed off.
    pub fn take_failure(&self, token: u64) -> StorageResult<Option<StorageError>> {
        let mut slots = self
            .shared
            .slots
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
        for slot in slots.iter_mut() {
            if let Slot::Retained {
                status, failure, ..
            } = slot
            {
                if status.token == token {
                    return Ok(failure.take());
                }
            }
        }
        Err(StorageError::Integrity(
            "construction scratch retained token",
        ))
    }

    /// One explicit native cleanup of a known retained owner.
    /// Unknown and previously attempted release are refused without new I/O.
    pub fn release_retained(&self, token: u64) -> StorageResult<()> {
        let (index, resource, failure) = {
            let mut slots = self
                .shared
                .slots
                .lock()
                .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
            let index = slots
                .iter()
                .position(
                    |slot| matches!(slot, Slot::Retained { status, .. } if status.token == token),
                )
                .ok_or(StorageError::Integrity(
                    "construction scratch retained token",
                ))?;
            let Slot::Retained { status, .. } = &slots[index] else {
                unreachable!();
            };
            if status.quarantined || status.release_attempted {
                return Err(StorageError::Integrity(
                    "construction scratch retained release denied",
                ));
            }
            let mut active = status.clone();
            active.retained = false;
            let prior = std::mem::replace(&mut slots[index], Slot::Active(active));
            let Slot::Retained {
                resource, failure, ..
            } = prior
            else {
                unreachable!();
            };
            (index, *resource, failure)
        };
        let mut session = ScratchSession::new(self.shared.clone(), index, resource);
        session.restore_failure(failure);
        session.release()
    }
}
