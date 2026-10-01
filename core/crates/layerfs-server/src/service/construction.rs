//! One lazy metadata-scratch authority for each configured Store.
//!
//! Only a validated prepared namespace operation initializes this owner. Pure
//! reads/catalog calls acquire no scratch file, Save or namespace state. Failed
//! initialization is retained rather than attempted again or silently bypassed.
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use layerfs_bridge::contract::{Code, Failure, PreparedChanges};
use layerfs_content::{
    filesystem::{
        root::FilesystemRootId,
        rows::SpoolPreparation,
        state::{AliasCapacity, CanonicalCapacity, FactCapacity, GraphCapacity, GraphSubject},
        InodeScope,
    },
    ObjectId,
};
use layerfs_storage::{
    construction_state::{ScratchAuthority, ScratchSession},
    Store,
};

use super::error::{content, storage};

enum Authority {
    Unopened,
    Ready(Arc<ScratchAuthority>),
    Refused(Failure),
}

/// Bounded configured ownership; live/retained sessions belong to C2.
pub(crate) struct Construction {
    parent: PathBuf,
    maximum_owners: u8,
    capacity: GraphCapacity,
    engine: Option<&'static layerfs_storage::engine::EngineGuard>,
    authority: Mutex<Authority>,
}

impl Construction {
    pub(crate) fn new(
        store: &Store,
        maximum_owners: u8,
        capacity: GraphCapacity,
    ) -> Result<Self, Failure> {
        let parent = store.path().parent().ok_or(Code::InvalidInput)?;
        let parent = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        let parent = if parent.is_absolute() {
            parent.to_path_buf()
        } else {
            std::env::current_dir()?.join(parent)
        };
        Ok(Self {
            parent,
            maximum_owners,
            capacity,
            engine: store.engine_guard(),
            authority: Mutex::new(Authority::Unopened),
        })
    }

    /// Shared lazy factory; all same-Store content owners use this authority.
    pub(super) fn authority(&self) -> Result<Arc<ScratchAuthority>, Failure> {
        let authority = {
            let mut cell = self.authority.try_lock().map_err(|_| Code::Ownership)?;
            if matches!(*cell, Authority::Unopened) {
                let opened = match self.engine {
                    Some(engine) => ScratchAuthority::new_guarded(
                        &self.parent,
                        usize::from(self.maximum_owners),
                        engine,
                    ),
                    None => ScratchAuthority::new(&self.parent, usize::from(self.maximum_owners)),
                };
                match opened {
                    Ok(authority) => *cell = Authority::Ready(Arc::new(authority)),
                    Err(error) => *cell = Authority::Refused(storage(error)),
                }
            }
            match &*cell {
                Authority::Ready(authority) => Arc::clone(authority),
                Authority::Refused(failure) => return Err(failure.clone()),
                Authority::Unopened => return Err(Code::Ownership.into()),
            }
        };
        Ok(authority)
    }

    /// Explicit shutdown drains only checked idle files, retaining failures.
    pub(super) fn drain_idle(&self) -> Result<usize, Failure> {
        let cell = self.authority.try_lock().map_err(|_| Code::Ownership)?;
        match &*cell {
            Authority::Unopened => Ok(0),
            Authority::Ready(authority) => authority.drain_idle().map_err(storage),
            Authority::Refused(error) => Err(error.clone()),
        }
    }

    /// Captured per-owner draft profile, before body or canonical Save effects.
    pub(crate) fn begin_file(
        &self,
        request: &layerfs_bridge::contract::Request,
    ) -> Result<Option<ScratchSession>, Failure> {
        let Some(selector) = super::construction_file::selection(request)? else {
            return Ok(None);
        };
        let capacity =
            layerfs_content::file::edit::DraftCapacity::new(self.capacity.scratch_bytes())
                .map_err(content)?;
        self.authority()?
            .begin_drafts(selector, capacity)
            .map(Some)
            .map_err(storage)
    }

    /// Admits before Save/body effects after catalog/head/scope validation.
    pub(crate) fn begin(
        &self,
        changes: &PreparedChanges,
    ) -> Result<
        (
            super::construction_session::ConstructionSession,
            Option<SpoolPreparation>,
        ),
        Failure,
    > {
        let (declaration, capacity) = super::save::prepared::planned_spool_admission(changes)?;
        let preparation = SpoolPreparation::new(declaration, capacity).map_err(content)?;
        if super::empty_admission::is_empty(changes)? {
            let state = super::empty_admission::admit(changes, &preparation, self.capacity)?;
            return Ok((
                super::construction_session::ConstructionSession::Empty(state),
                Some(preparation),
            ));
        }
        if super::small_file_admission::selected(changes)? {
            let pending =
                super::small_file_admission::pending(changes, preparation, self.capacity)?;
            return Ok((
                super::construction_session::ConstructionSession::Small(Some(pending)),
                None,
            ));
        }
        let subject = GraphSubject::new(
            preparation.source_id(),
            InodeScope::from_object(ObjectId::from_bytes(&changes.scope).map_err(content)?),
            Some(FilesystemRootId(
                ObjectId::from_bytes(&changes.base).map_err(content)?,
            )),
            changes.root_serial,
            self.capacity,
        )
        .map_err(content)?;
        let authority = self.authority()?;
        // This selection names the checked captured construction context, not a
        // guessed saved result. The issued token distinguishes separate attempts.
        let mut context = Vec::with_capacity(160);
        context.extend_from_slice(b"layerfs/server-construction/v1\0");
        context.extend_from_slice(&changes.workspace);
        context.extend_from_slice(&changes.branch);
        context.extend_from_slice(&changes.generation.to_be_bytes());
        context.extend_from_slice(&changes.base);
        context.extend_from_slice(&changes.scope);
        context.extend_from_slice(&changes.root_serial.to_be_bytes());
        let records = layerfs_content::filesystem::FilesystemResources::default()
            .ordering_bytes
            .checked_div(1024)
            .and_then(|n| n.checked_add(1))
            .ok_or(Code::Capacity)?;
        let aliases =
            AliasCapacity::new(records, self.capacity.scratch_bytes(), changes.totals.names)
                .map_err(content)?;
        let facts = FactCapacity::new(
            records,
            changes.totals.directories,
            self.capacity.scratch_bytes(),
        )
        .map_err(content)?;
        let canonical = CanonicalCapacity::new(
            layerfs_content::filesystem::FilesystemResources::default().ordering_bytes / 16,
            layerfs_content::filesystem::FilesystemResources::default().ordering_bytes / 16,
            records,
            records,
            self.capacity.scratch_bytes(),
        )
        .map_err(content)?;
        let state = authority
            .begin_canonical(
                *ObjectId::for_bytes(&context).as_bytes(),
                changes.totals.directories,
                changes.totals.names,
                subject,
                aliases,
                facts,
                canonical,
            )
            .map_err(storage)?;
        Ok((
            super::construction_session::ConstructionSession::Native(state),
            Some(preparation),
        ))
    }
}
