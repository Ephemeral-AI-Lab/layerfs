//! Shared physical provider; Objects and Metadata use one transaction boundary.
use crate::{backend::Session, metadata, objects, publication};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{LocatedObject, SignatureRow},
    port::*,
    StoragePolicy,
};
use std::sync::Arc;
/// Application-owned bounded physical persistence over the combined Store.
pub struct StorageProvider {
    pub(crate) session: Arc<Session>,
}
impl StorageProvider {
    /// Read-only counters from this already-open session, without a SQL query.
    /// Application observers need no history or acquisition capability.
    pub fn diagnostics(&self) -> Result<crate::SqlWork, PersistenceError> {
        self.session.diagnostics().map_err(Into::into)
    }
}
impl PackPersistence for StorageProvider {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        self.session.run(false, metadata::policy::read)
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        out.clear();
        self.session
            .run(false, |tx| metadata::locations::read(tx, ids, out))
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        out.clear();
        self.session
            .run(false, |tx| objects::read::read(tx, ids, out))
    }
    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<PersistedPackRead, PersistenceError> {
        self.session.run(false, |tx| {
            crate::backend::sqlite::objects_selection::read(tx, id, plan)
        })
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<AcquiredPackRead, PersistenceError> {
        self.session.run(false, |tx| {
            crate::backend::sqlite::objects_selection::read_scoped(tx, id, plan)
        })
    }
    fn value_groups(&self, query: ValueGroupQuery<'_>) -> Result<ValueGroups, PersistenceError> {
        self.session
            .run(false, |tx| metadata::pooling::read(tx, query))
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
        match self.session.layout {
            crate::SqlitePackLayout::Monolithic => Ok((1, pack.body.len() as u64)),
            crate::SqlitePackLayout::GroupRows | crate::SqlitePackLayout::GroupRowsIndexed => {
                crate::backend::sqlite::unit_layout::cost(pack)
            }
        }
    }
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        publication::validate(batch)?;
        if self.session.layout.uses_units() {
            crate::backend::sqlite::unit_layout::validate_batch(batch)?;
        }
        self.session.run(true, |tx| publication::publish(tx, batch))
    }
}
