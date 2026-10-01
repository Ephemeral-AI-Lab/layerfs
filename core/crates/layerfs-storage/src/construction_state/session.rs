//! One immutable selected SQL session, typed failures and explicit native release.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use layerfs_content::filesystem::state::{
    PageLimit, StateCapacity, StateKey, StateLedger, StatePage, StateRecord, StateScope, StateSeal,
    StateSelection, StateTable,
};
use layerfs_content::ContentError;
use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};

use super::authority::{Shared, Slot};
use super::header::Header;
use super::native::{NativeFile, RESERVED_BYTES};
use super::phased::Phased;
use super::plan::Plan;
use super::sites::Sites;
use super::status::{ScratchDisposition, ScratchOwnerStatus, ScratchProfile};
use super::{index, profile};

pub(crate) struct Resource {
    pub(crate) selection: StateSelection,
    pub(crate) native: NativeFile,
    pub(crate) connection: Option<Connection>,
    header: Option<Header>,
    ledger: Option<StateLedger>,
    seal: Option<StateSeal>,
    logical_released: bool,
    plan: Plan,
    pub(crate) phased: Option<Phased>,
    pub(crate) sites: Option<Sites>,
    pub(crate) release_attempted: bool,
    pub(crate) unknown: Cell<bool>,
}

impl Resource {
    pub(crate) fn new(selection: StateSelection, native: NativeFile, plan: Plan) -> Self {
        Self {
            selection,
            native,
            connection: None,
            header: None,
            ledger: None,
            seal: None,
            logical_released: false,
            plan,
            phased: None,
            sites: None,
            release_attempted: false,
            unknown: Cell::new(false),
        }
    }

    pub(crate) fn status(
        &self,
        disposition: ScratchDisposition,
        failure: Option<String>,
        unknown: bool,
        retained: bool,
    ) -> ScratchOwnerStatus {
        let failure = match self
            .phased
            .as_ref()
            .and_then(|state| state.attempt.as_ref())
        {
            Some(attempt) => Some(match failure {
                Some(failure) => format!("{failure}; {}", attempt.description()),
                None => attempt.description(),
            }),
            None => failure,
        };
        let failure = if let Some(sites) = &self.sites {
            if failure.is_some() || sites.failed.get() {
                Some(format!(
                    "{}; {}",
                    failure.unwrap_or_default(),
                    sites.description()
                ))
            } else {
                failure
            }
        } else {
            failure
        };
        let failure = match self
            .phased
            .as_ref()
            .and_then(|state| state.proposed_seal.as_ref())
        {
            Some((seal, maximum)) => Some(format!(
                "{}; proposed claim seal: records={}, bytes={}, digest={:?}, maximum={:?}",
                failure.unwrap_or_default(),
                seal.records(),
                seal.encoded_bytes(),
                seal.digest(),
                maximum.map(|key| key.serial())
            )),
            None => failure,
        };
        ScratchOwnerStatus {
            token: self.selection.token(),
            selector: *self.selection.selector(),
            disposition,
            path: self.native.path.clone(),
            parent: self.native.parent,
            directory: self.native.directory_identity,
            file: self.native.identity,
            reserved_bytes: RESERVED_BYTES,
            allocated_bytes: self.native.allocated,
            failure,
            quarantined: unknown || self.unknown.get() || self.native.quarantined,
            release_attempted: self.release_attempted,
            failed_close_descriptor: self.native.failed_close_descriptor(),
            retained,
        }
    }

    pub(crate) fn verify(&self) -> StorageResult<&Connection> {
        if self.unknown.get() || self.native.quarantined || self.release_attempted {
            return Err(StorageError::Integrity(
                "construction scratch quarantined/ended owner",
            ));
        }
        self.native.verify()?;
        let connection = self.connection.as_ref().ok_or(StorageError::Integrity(
            "construction scratch connection unavailable",
        ))?;
        index::verify_header(
            connection,
            self.header
                .as_ref()
                .ok_or(StorageError::Integrity(
                    "construction scratch header unavailable",
                ))?
                .as_bytes(),
            self.seal.as_ref(),
        )?;
        if let Some(phased) = &self.phased {
            super::claim_index::verify(connection, phased)?;
        }
        if let Some(sites) = &self.sites {
            super::site_index::verify(connection, sites)?;
        }
        Ok(connection)
    }

    fn check_scope(&self, scope: &StateScope) -> StorageResult<()> {
        if scope.selection() != &self.selection
            || self.logical_released
            || scope.table() != StateTable::DirectoryRoots
        {
            return Err(StorageError::Integrity(
                "construction scratch scope/phase released",
            ));
        }
        if let Some(phased) = &self.phased {
            phased.check_roots(scope)?;
        }
        if let Some(sites) = &self.sites {
            sites.check_roots(scope)?;
        }
        if self
            .ledger
            .as_ref()
            .is_some_and(|ledger| ledger.scope() != scope)
        {
            return Err(StorageError::Integrity(
                "construction scratch immutable phase",
            ));
        }
        Ok(())
    }

    fn check_seal(&self, seal: &StateSeal) -> StorageResult<()> {
        self.check_scope(seal.scope())?;
        if self.seal.as_ref() != Some(seal) {
            return Err(StorageError::Integrity("construction scratch exact seal"));
        }
        Ok(())
    }

    fn close(&mut self) -> StorageResult<()> {
        if self.unknown.get() || self.native.quarantined || self.release_attempted {
            return Err(StorageError::Integrity(
                "construction scratch native release denied",
            ));
        }
        self.release_attempted = true;
        if self.native.has_file_effects() {
            self.native.verify()?;
        }
        if let Some(connection) = self.connection.take() {
            if !connection.is_autocommit() {
                self.connection = Some(connection);
                self.unknown.set(true);
                return Err(StorageError::UnknownOutcome {
                    original: Box::new(StorageError::Integrity(
                        "construction scratch open transaction at close",
                    )),
                });
            }
            if let Err((connection, error)) = connection.close() {
                self.connection = Some(connection);
                return Err(StorageError::Engine(error));
            }
        }
        self.native.release()
    }
}

impl Drop for Resource {
    fn drop(&mut self) {
        // Releasing a session is the only close/unlink operation. The authority
        // normally retains this capsule. If its last owner itself is lost, keep
        // uncertain or unreleased native/engine handles until process teardown;
        // ordinary field destruction cannot issue a hidden close or rollback.
        if let Some(connection) = self.connection.take() {
            std::mem::forget(connection);
        }
        self.native.preserve_handles();
    }
}

fn unknown(error: &StorageError) -> bool {
    match error {
        StorageError::UnknownOutcome { .. } => true,
        StorageError::CleanupFailed { original, cleanup } => unknown(original) || unknown(cleanup),
        _ => false,
    }
}

/// One admitted producer's private construction session.
/// Dropping an unreleased session transfers exact ownership into its authority;
/// it performs no hidden close/unlink or retry. Native release is explicit.
pub struct ScratchSession {
    shared: Arc<Shared>,
    slot: usize,
    pub(crate) resource: Option<Resource>,
    failure: RefCell<Option<StorageError>>,
    description: RefCell<Option<String>>,
    failed: Cell<bool>,
    quarantined: Cell<bool>,
}

impl ScratchSession {
    pub(crate) fn new(shared: Arc<Shared>, slot: usize, resource: Resource) -> Self {
        Self {
            shared,
            slot,
            resource: Some(resource),
            failure: RefCell::new(None),
            description: RefCell::new(None),
            failed: Cell::new(false),
            quarantined: Cell::new(false),
        }
    }

    pub(crate) fn restore_failure(&mut self, failure: Option<StorageError>) {
        if let Some(error) = failure {
            self.note(&error);
            *self.failure.borrow_mut() = Some(error);
        }
    }

    fn note(&self, error: &StorageError) {
        self.failed.set(true);
        self.quarantined
            .set(self.quarantined.get() || unknown(error));
        if let Some(resource) = &self.resource {
            resource
                .unknown
                .set(resource.unknown.get() || self.quarantined.get());
            if let Some(phased) = &resource.phased {
                phased.failed.set(true);
            }
            if let Some(sites) = &resource.sites {
                sites.failed.set(true);
            }
        }
        if self.description.borrow().is_none() {
            *self.description.borrow_mut() = Some(error.to_string());
        }
    }

    pub(crate) fn keep_failure(&self, error: StorageError) -> ContentError {
        self.note(&error);
        if self.failure.borrow().is_none() {
            *self.failure.borrow_mut() = Some(error);
        }
        ContentError::ProviderFailure {
            what: "construction scratch state",
        }
    }

    fn sync_status(&self) -> StorageResult<()> {
        let Some(resource) = &self.resource else {
            return Ok(());
        };
        let disposition =
            if self.quarantined.get() || resource.unknown.get() || resource.native.quarantined {
                ScratchDisposition::Unknown
            } else if resource.release_attempted {
                ScratchDisposition::ReleaseFailed
            } else if self.failed.get()
                || resource
                    .phased
                    .as_ref()
                    .is_some_and(|state| state.failed.get())
                || resource
                    .sites
                    .as_ref()
                    .is_some_and(|state| state.failed.get())
            {
                ScratchDisposition::Failed
            } else if resource.seal.is_some()
                || resource
                    .sites
                    .as_ref()
                    .is_some_and(|state| state.seal.is_some())
            {
                ScratchDisposition::Sealed
            } else {
                ScratchDisposition::Open
            };
        let status = resource.status(
            disposition,
            self.description.borrow().clone(),
            self.quarantined.get(),
            false,
        );
        let mut slots = self
            .shared
            .slots
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
        slots[self.slot] = Slot::Active(status);
        Ok(())
    }

    pub(crate) fn finish<T>(&self, result: StorageResult<T>) -> StorageResult<T> {
        if let Err(error) = &result {
            self.note(error);
        }
        match self.sync_status() {
            Ok(()) => result,
            Err(cleanup) => {
                self.note(&cleanup);
                self.quarantined.set(true);
                if let Some(resource) = &self.resource {
                    resource.unknown.set(true);
                }
                match result {
                    Ok(_) => Err(cleanup),
                    Err(original) => Err(StorageError::CleanupFailed {
                        original: Box::new(original),
                        cleanup: Box::new(cleanup),
                    }),
                }
            }
        }
    }

    pub(crate) fn initialize(&mut self) -> StorageResult<()> {
        let result = (|| {
            let resource = self.resource.as_mut().unwrap();
            if let Err(error) = resource.native.initialize() {
                if resource.native.quarantined {
                    resource.unknown.set(true);
                    return Err(StorageError::UnknownOutcome {
                        original: Box::new(error),
                    });
                }
                return Err(error);
            }
            let binding = match resource.plan {
                Plan::SitesThenRoots { source, .. } => resource.native.sites_binding(
                    resource.selection.selector(),
                    resource.selection.token(),
                    source,
                )?,
                _ => resource
                    .native
                    .binding(resource.selection.selector(), resource.selection.token())?,
            };
            resource.selection.bind_owner(binding)?;
            let header = match resource.plan {
                Plan::SitesThenRoots { source, .. } => {
                    Header::Sites(resource.native.sites_header(
                        resource.selection.selector(),
                        resource.selection.token(),
                        &binding,
                        source,
                    )?)
                }
                _ => Header::Earlier(resource.native.header(
                    resource.selection.selector(),
                    resource.selection.token(),
                    &binding,
                    resource.plan.version(),
                )?),
            };
            resource.header = Some(header);
            if let Plan::ClaimsThenRoots {
                directories,
                bindings,
            } = resource.plan
            {
                resource.phased = Some(Phased::new(&resource.selection, directories, bindings)?);
            }
            if let Plan::SitesThenRoots {
                directories,
                bindings,
                source,
            } = resource.plan
            {
                resource.sites = Some(Sites::new(
                    &resource.selection,
                    directories,
                    bindings,
                    source,
                )?);
            }
            let connection = profile::open(&resource.native.path)?;
            resource.connection = Some(connection);
            resource.native.verify()?;
            let claims = resource
                .phased
                .as_ref()
                .map(|state| state.claims.as_bytes());
            let sites = resource.sites.as_ref().map(|state| state.scope.as_bytes());
            profile::initialize(
                resource.connection.as_ref().unwrap(),
                resource.header.as_ref().unwrap().as_bytes(),
                resource.plan,
                claims.as_ref(),
                sites.as_ref(),
            )?;
            resource.native.verify()?;
            Ok(())
        })();
        self.finish(result)
    }

    /// Exact live issued selection after native binding; callers may clone it.
    pub fn selection(&self) -> &StateSelection {
        &self
            .resource
            .as_ref()
            .expect("live scratch session")
            .selection
    }

    /// Original typed C2 failure retained by the C1 adapter, handed off once.
    pub fn take_failure(&mut self) -> Option<StorageError> {
        self.failure.borrow_mut().take()
    }

    /// Exact native/SQL custody that denies destructive cleanup. Taking the
    /// original error does not clear it; the caller retains this owner on Drop.
    pub fn is_quarantined(&self) -> bool {
        self.quarantined.get()
            || self
                .resource
                .as_ref()
                .is_some_and(|resource| resource.unknown.get() || resource.native.quarantined)
    }

    /// Actual private connection readbacks, without a global heap/RSS claim.
    pub fn profile(&self) -> StorageResult<ScratchProfile> {
        let result = (|| {
            let resource = self
                .resource
                .as_ref()
                .ok_or(StorageError::Integrity("construction scratch released"))?;
            profile::readback(resource.verify()?, resource.plan.version())
        })();
        self.finish(result)
    }

    /// Exact supported logical row/record-byte class, checked before producer effects.
    pub fn capacity(&self, scope: &StateScope) -> StorageResult<StateCapacity> {
        self.finish((|| {
            let resource = self
                .resource
                .as_ref()
                .ok_or(StorageError::Integrity("construction scratch released"))?;
            resource.check_scope(scope)?;
            resource.verify()?;
            Ok(StateCapacity::new(
                resource.plan.root_limit(),
                resource.plan.root_limit() * profile::RECORD_BYTES,
            )?)
        })())
    }

    /// Append one fully checked ordered batch, acknowledged before digest advances.
    pub fn append(&mut self, scope: &StateScope, records: &[StateRecord]) -> StorageResult<()> {
        let result = (|| {
            let resource = self
                .resource
                .as_mut()
                .ok_or(StorageError::Integrity("construction scratch released"))?;
            resource.check_scope(scope)?;
            if resource.seal.is_some() || self.failed.get() {
                return Err(StorageError::Integrity(
                    "construction scratch writable phase ended",
                ));
            }
            if resource.ledger.is_none() {
                resource.ledger = Some(StateLedger::new(scope.clone()));
            }
            let ledger = resource.ledger.as_ref().unwrap();
            ledger.validate_append(records)?;
            let count = ledger
                .records()
                .checked_add(records.len() as u64)
                .ok_or(StorageError::Integrity("construction scratch row overflow"))?;
            if count > resource.plan.root_limit() {
                return Err(StorageError::CapacityExceeded {
                    what: "construction scratch rows",
                    limit: resource.plan.root_limit(),
                    actual: count,
                });
            }
            let previous = ledger.records();
            resource.verify()?;
            resource.native.reserve()?;
            index::append(
                resource.connection.as_ref().unwrap(),
                scope,
                records,
                previous,
            )?;
            resource.native.observe_allocation()?;
            resource
                .ledger
                .as_mut()
                .unwrap()
                .acknowledge(records)
                .map_err(|error| StorageError::UnknownOutcome {
                    original: Box::new(StorageError::Content(error)),
                })?;
            Ok(())
        })();
        self.finish(result)
    }

    /// Seal this exact immutable phase once after all acknowledged appends.
    pub fn seal(&mut self, scope: &StateScope) -> StorageResult<StateSeal> {
        let result = (|| {
            let resource = self
                .resource
                .as_mut()
                .ok_or(StorageError::Integrity("construction scratch released"))?;
            resource.check_scope(scope)?;
            if resource.seal.is_some() || self.failed.get() {
                return Err(StorageError::Integrity(
                    "construction scratch seal already ended",
                ));
            }
            if resource.ledger.is_none() {
                resource.ledger = Some(StateLedger::new(scope.clone()));
            }
            let seal = resource.ledger.as_ref().unwrap().seal();
            resource.verify()?;
            resource.native.reserve()?;
            index::seal(resource.connection.as_ref().unwrap(), &seal)?;
            resource.seal = Some(seal.clone());
            resource.native.observe_allocation()?;
            Ok(seal)
        })();
        self.finish(result)
    }

    /// One point lookup under the exact private acknowledged seal.
    pub fn get(&mut self, seal: &StateSeal, key: StateKey) -> StorageResult<Option<StateRecord>> {
        let result = (|| {
            let resource = self
                .resource
                .as_ref()
                .ok_or(StorageError::Integrity("construction scratch released"))?;
            resource.check_seal(seal)?;
            index::get(resource.verify()?, seal, key)
        })();
        self.finish(result)
    }

    /// One count/byte bounded primary-key page with sealed-count EOF.
    pub fn page(
        &mut self,
        seal: &StateSeal,
        after: Option<StateKey>,
        limit: PageLimit,
    ) -> StorageResult<StatePage> {
        let result = (|| {
            let resource = self
                .resource
                .as_ref()
                .ok_or(StorageError::Integrity("construction scratch released"))?;
            resource.check_seal(seal)?;
            index::page(resource.verify()?, seal, after, limit)
        })();
        self.finish(result)
    }

    /// End only the logical selected phase; retain every native byte/credit.
    pub fn complete_phase(&mut self, scope: &StateScope) -> StorageResult<()> {
        let result = (|| {
            let resource = self
                .resource
                .as_mut()
                .ok_or(StorageError::Integrity("construction scratch released"))?;
            if self.quarantined.get() || resource.unknown.get() {
                return Err(StorageError::Integrity(
                    "construction scratch Unknown phase",
                ));
            }
            resource.check_scope(scope)?;
            resource.logical_released = true;
            Ok(())
        })();
        self.finish(result)
    }

    /// One explicit close/identity/unlink/removal confirmation, then credit refund.
    /// A known previously released session returns without new effects. Unknown
    /// or a previously failed native attempt never retries or guesses cleanup.
    pub fn release(&mut self) -> StorageResult<()> {
        if self.resource.is_none() {
            return Ok(());
        }
        if self.quarantined.get() {
            return Err(StorageError::Integrity(
                "construction scratch Unknown release denied",
            ));
        }
        let result = self.resource.as_mut().unwrap().close();
        self.finish(result)?;
        let mut slots = self
            .shared
            .slots
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
        slots[self.slot] = Slot::Empty;
        self.resource.take();
        Ok(())
    }
}

impl Drop for ScratchSession {
    fn drop(&mut self) {
        let Some(resource) = self.resource.take() else {
            return;
        };
        let disposition =
            if self.quarantined.get() || resource.unknown.get() || resource.native.quarantined {
                ScratchDisposition::Unknown
            } else if resource.release_attempted {
                ScratchDisposition::ReleaseFailed
            } else {
                ScratchDisposition::Failed
            };
        let mut status = resource.status(
            disposition,
            self.description.borrow().clone(),
            self.quarantined.get(),
            true,
        );
        match self.shared.slots.lock() {
            Ok(mut slots) => {
                slots[self.slot] = Slot::Retained {
                    resource: Box::new(resource),
                    status,
                    failure: self.failure.get_mut().take(),
                };
            }
            Err(poisoned) => {
                // Preserve custody only; do not clear poison or resume operations.
                // All public authority accesses continue to refuse this mutex.
                resource.unknown.set(true);
                status.disposition = ScratchDisposition::Unknown;
                status.quarantined = true;
                poisoned.into_inner()[self.slot] = Slot::Retained {
                    resource: Box::new(resource),
                    status,
                    failure: self.failure.get_mut().take(),
                };
            }
        }
    }
}
