//! Bounded pack publication, reads and allocation over one PostgreSQL session.
use super::super::{
    objects,
    session::{connection::Session, transaction::Mode},
};
use crate::{metadata, publication, SqlWork};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{InodeValueRow, LocatedChains, LocatedObject, SignatureRow},
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
    /// Merge build records over this writer connection's temporary tables
    /// (#506 d); writable sessions only.
    pub fn merge_scratch(&self) -> crate::MergeScratch {
        crate::MergeScratch {
            session: crate::backend::sqlite::storage::merge_records::Scratch::Postgres(
                self.session.clone(),
            ),
        }
    }
    /// Whether this idle session can still serve (#64): it was not
    /// quarantined and its connection answers an empty query within 5 s (one
    /// round trip, counted). A session that does not is never handed to a
    /// mount; its caller drops it and opens a new one. This check runs
    /// before any operation uses the session, so no operation is retried.
    pub fn usable(&self) -> bool {
        let Ok(mut state) = self.session.state.lock() else {
            return false;
        };
        if state.quarantined {
            return false;
        }
        state.work.round_trips += 1;
        let alive = state.client.is_valid(std::time::Duration::from_secs(5));
        if alive.is_err() {
            state.quarantine();
        }
        alive.is_ok()
    }
    /// The server's reported version.
    pub fn server_version(&self) -> &str {
        self.session.server_version.get().map_or("", String::as_str)
    }
}

impl PackPersistence for PostgresStorage {
    /// The frozen policy the open's check read (#63), with no round trip;
    /// read in a transaction only by a session opened without one.
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        if let Some(policy) = self.session.policy.get() {
            return Ok(*policy);
        }
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
    /// Design B (#758): the roots, their lists and their packs in one
    /// statement, then the member packs it did not return in a second, both
    /// autocommit reads of immutable rows (`metadata::chains`).
    fn locate_chains(
        &self,
        roots: &[ObjectId],
        rows: &mut Vec<LocatedObject>,
        chains: &mut LocatedChains,
    ) -> Result<(), PersistenceError> {
        rows.clear();
        chains.chains.clear();
        chains.directories.clear();
        let policy = self.policy()?;
        self.session.read_immutable(|tx| {
            super::super::metadata::chains::locate_chains(tx, &policy, roots, rows, chains)
        })
    }
    /// Whole packs for every read (#758 phase 2; group ranges are phase 3):
    /// one `read_packs` statement per 2 MiB of packs, as the port's default,
    /// but answered in request order, which `read_packs` (pack-id order) is
    /// not.
    fn read_groups(
        &self,
        reads: &[GroupRead],
        out: &mut Vec<GroupReadAnswer>,
    ) -> Result<(), PersistenceError> {
        out.clear();
        let mut rest = reads;
        while !rest.is_empty() {
            let mut bytes = 0;
            let take = rest
                .iter()
                .take_while(|read| {
                    bytes += read.pack.length;
                    bytes <= layerfs_storage::policy::DEPENDENCY_PACK_CACHE_BYTES
                })
                .count()
                .max(1);
            let ids: Vec<i64> = rest[..take].iter().map(|read| read.pack.pack_id).collect();
            let mut packs = Vec::new();
            self.read_packs(&ids, &mut packs)?;
            let mut by_id: std::collections::HashMap<i64, PersistedPack> = packs
                .into_iter()
                .map(|pack| (pack.info().pack_id, pack))
                .collect();
            for (at, id) in ids.iter().enumerate() {
                // A pack asked twice is copied for all but its last read.
                let pack = match ids[at + 1..].contains(id) {
                    true => by_id.get(id).cloned(),
                    false => by_id.remove(id),
                };
                out.push(GroupReadAnswer::Whole(
                    pack.ok_or(PersistenceError::Missing)?,
                ));
            }
            rest = &rest[take..];
        }
        Ok(())
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
    /// Every ranged read is a round trip, so a pack up to the pack limit is
    /// taken whole in one statement and kept for the next demands.
    fn whole_pack_bytes(&self) -> usize {
        layerfs_storage::policy::PACK_LIMIT
    }
    /// Every read is a round trip, so the bases this handle published are
    /// fetched in the same group-read wave as provider chains (#758).
    fn batched_chain_reads(&self) -> bool {
        true
    }
    /// Each statement is a round trip and a publication costs about seven
    /// of them whatever its size, so PostgreSQL publishes up to 16 MiB at
    /// once (owner decision, layerfs-project #61); SQLite keeps 4 MiB.
    fn publication_bytes(&self) -> u64 {
        layerfs_storage::policy::PUBLICATION_BYTES_MAX
    }
    fn publication_pack_cost(
        &self,
        pack: &PublishedPack,
    ) -> Result<(usize, u64), PersistenceError> {
        Ok((1, pack.body().len() as u64))
    }
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        publication::validate(batch, self.publication_bytes())?;
        self.session
            .run_as::<_, PersistenceError>(Mode::Publish, |tx| {
                objects::publish::run(tx, batch).map_err(Into::into)
            })
    }
}
