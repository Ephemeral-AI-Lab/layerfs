//! Bounded pack publication, reads and allocation over one PostgreSQL session.
use super::super::{
    objects,
    session::{connection::Session, transaction::Mode},
};
use crate::{metadata, publication, SqlWork};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, SignatureRow},
    port::*,
    StoragePolicy,
};
use std::sync::Arc;

/// Application-owned bounded physical persistence over a PostgreSQL Store.
pub struct PostgresStorage {
    pub(crate) session: Arc<Session>,
}

impl PostgresStorage {
    /// Counters of this session's attempted work, read without a query.
    pub fn diagnostics(&self) -> Result<SqlWork, PersistenceError> {
        self.session.diagnostics().map_err(Into::into)
    }
    /// The server's reported version.
    pub fn server_version(&self) -> &str {
        &self.session.server_version
    }
}

impl PackPersistence for PostgresStorage {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        self.session.run(false, |tx| metadata::policy::read(tx))
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        out.clear();
        self.session
            .read_immutable(|tx| metadata::locations::read(tx, ids, out))
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        out.clear();
        self.session
            .read_immutable(|tx| crate::objects::read::read(tx, ids, out))
    }
    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<PersistedPackRead, PersistenceError> {
        self.session
            .read_immutable(|tx| objects::read::selection(tx, id, plan))
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<AcquiredPackRead, PersistenceError> {
        self.session
            .read_immutable(|tx| objects::read::scoped(tx, id, plan))
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        out.clear();
        self.session
            .read_immutable(|tx| metadata::values::read(tx, ordinals, out))
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
        out.clear();
        self.session
            .run(false, |tx| metadata::signatures::read(tx, out))
    }
    fn reserve(&self, request: Reserve) -> Result<Reserved, PersistenceError> {
        self.session
            .run(true, |tx| metadata::allocation::reserve(tx, request))
    }
    fn publication_pack_cost(
        &self,
        pack: &PublishedPack,
    ) -> Result<(usize, u64), PersistenceError> {
        Ok((1, pack.body.len() as u64))
    }
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        publication::validate(batch)?;
        if !batch.packs.is_empty() {
            self.session
                .run_as::<_, PersistenceError>(Mode::Stage, |tx| {
                    objects::publish::packs(tx, &batch.packs).map_err(Into::into)
                })?;
        }
        self.session.run(true, |tx| {
            objects::publish::rows(tx, batch).map_err(PersistenceError::from)
        })
    }
}
