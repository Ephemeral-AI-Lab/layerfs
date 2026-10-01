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
        state::{GraphCapacity, GraphSubject},
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
            authority: Mutex::new(Authority::Unopened),
        })
    }

    /// Admits before Save/body effects after catalog/head/scope validation.
    pub(crate) fn begin(
        &self,
        changes: &PreparedChanges,
    ) -> Result<(ScratchSession, SpoolPreparation), Failure> {
        let (declaration, capacity) = super::save::prepared::planned_spool_admission(changes)?;
        let preparation = SpoolPreparation::new(declaration, capacity).map_err(content)?;
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
        let authority = {
            let mut cell = self.authority.try_lock().map_err(|_| Code::Ownership)?;
            if matches!(*cell, Authority::Unopened) {
                match ScratchAuthority::new(&self.parent, usize::from(self.maximum_owners)) {
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
        let state = authority
            .begin_graph(
                *ObjectId::for_bytes(&context).as_bytes(),
                changes.totals.directories,
                changes.totals.names,
                subject,
            )
            .map_err(storage)?;
        Ok((state, preparation))
    }
}
