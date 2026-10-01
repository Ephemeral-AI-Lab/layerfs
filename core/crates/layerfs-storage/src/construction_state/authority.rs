//! Bounded slot admission and exact retained native ownership.

use std::path::Path;
use std::sync::{Arc, Mutex};

use layerfs_content::filesystem::rows::BindingSourceId;
use layerfs_content::filesystem::state::StateSelection;

use crate::error::{StorageError, StorageResult};

use super::native::{NativeDirectory, NativeFile, RESERVED_BYTES};
use super::plan::Plan;
use super::session::{Resource, ScratchSession};
use super::status::{ScratchDisposition, ScratchOwnerStatus};

pub(crate) enum Slot {
    Empty,
    Active(ScratchOwnerStatus),
    Retained {
        resource: Box<Resource>,
        status: ScratchOwnerStatus,
        failure: Option<StorageError>,
    },
}

pub(crate) struct Shared {
    pub(crate) directory: Arc<Mutex<NativeDirectory>>,
    pub(crate) slots: Mutex<Vec<Slot>>,
    pub(crate) limit: usize,
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
        slots.resize_with(maximum_owners, || Slot::Empty);
        let directory = NativeDirectory::prepare(parent)?;
        Ok(Self {
            shared: Arc::new(Shared {
                directory: Arc::new(Mutex::new(directory)),
                slots: Mutex::new(slots),
                limit: maximum_owners,
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
        self.admit(selector, Plan::Legacy)
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
        self.admit(selector, Plan::phased(directories, bindings)?)
    }

    /// Bind an already issued immutable row source before token/native/SQL effects.
    pub fn begin_sites(
        &self,
        selector: [u8; 32],
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
    ) -> StorageResult<ScratchSession> {
        self.admit(selector, Plan::sites(directories, bindings, source)?)
    }

    fn admit(&self, selector: [u8; 32], plan: Plan) -> StorageResult<ScratchSession> {
        let selection = StateSelection::issue(selector)?;
        let status = {
            let directory = self
                .shared
                .directory
                .lock()
                .map_err(|_| StorageError::Integrity("construction scratch directory lock"))?;
            ScratchOwnerStatus {
                token: selection.token(),
                selector: *selection.selector(),
                disposition: ScratchDisposition::Admitted,
                path: directory
                    .path
                    .join(format!("state-{:016x}.sqlite", selection.token())),
                parent: directory.parent,
                directory: directory.identity,
                file: None,
                reserved_bytes: RESERVED_BYTES,
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
            let index = slots
                .iter()
                .position(|slot| matches!(slot, Slot::Empty))
                .ok_or(StorageError::OwnershipUnavailable)?;
            slots[index] = Slot::Active(status);
            index
        };
        let native = match NativeFile::prepare(self.shared.directory.clone(), selection.token()) {
            Ok(native) => native,
            Err(original) => {
                // Preparation has no native/SQL effects. A known empty admission
                // can be refunded; bootstrap never enters this path.
                match self.shared.slots.lock() {
                    Ok(mut slots) => {
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
        let resource = Resource::new(selection, native, plan);
        let mut session = ScratchSession::new(self.shared.clone(), slot, resource);
        session.initialize()?;
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
                Slot::Active(status) | Slot::Retained { status, .. } => output.push(status.clone()),
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
        Ok(slots
            .iter()
            .filter(|slot| !matches!(slot, Slot::Empty))
            .count() as u64
            * RESERVED_BYTES)
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
