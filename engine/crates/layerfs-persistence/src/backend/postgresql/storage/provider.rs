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
    fn locate_root_packs(
        &self,
        roots: &[ObjectId],
        rows: &mut Vec<LocatedObject>,
        chains: &mut LocatedChains,
        joined: Option<(&[i64], &mut Vec<PersistedPack>)>,
    ) -> Result<(), PersistenceError> {
        rows.clear();
        chains.chains.clear();
        chains.directories.clear();
        let policy = self.policy()?;
        let incarnation = self
            .session
            .incarnation
            .load(std::sync::atomic::Ordering::Relaxed);
        let kept = (&self.session.chain_packs, incarnation);
        self.session.read_immutable(|tx| {
            super::super::metadata::chains::locate_chains(
                tx, &policy, kept, roots, rows, chains, joined,
            )
        })
    }

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
    /// Whether this idle session can still serve a mount (#64): it was not
    /// quarantined and its connection still stands, judged locally (#768: a
    /// connection the server closed, by a restart, a terminated backend or
    /// an idle timeout, is seen with no round trip). A connection on which
    /// the server has not answered for at least the selection's
    /// `reader_idle_check_ms` ([`crate::READER_IDLE_CHECK_MS`], 30 s by
    /// default) must also answer an empty query within
    /// [`crate::READER_CHECK_TIMEOUT`] (one round trip, counted). The silence
    /// is the connection's own, counted from the server's last reply, not
    /// the time the reader spent among the idle readers: a Workspace that
    /// read nothing for long before its unmount hands back a silent
    /// connection. A path that ended it without telling the client (reset at
    /// the next packet, or dropped silently, where a read would wait for the
    /// TCP retransmission timeout) is found here, not by the mount's first
    /// read. A session that cannot serve is quarantined and never handed to
    /// a mount; its caller opens a new one. The check runs before any
    /// operation uses the session, so no operation is retried.
    pub fn usable(&self) -> bool {
        let Ok(mut state) = self.session.state.lock() else {
            return false;
        };
        if state.quarantined {
            return false;
        }
        let threshold = u64::from(self.session.settings.reader_idle_check_ms);
        let quiet = state.client.silent() < std::time::Duration::from_millis(threshold);
        let alive = state.client.alive()
            && (quiet || {
                state.work.round_trips += 1;
                state.client.answers(crate::READER_CHECK_TIMEOUT)
            });
        if !alive {
            state.quarantine();
        }
        alive
    }
    /// Prepares the statements a Workspace mount sends on this reader
    /// session in one flight (#768): one round trip when the reader opens,
    /// so the mount prepares none.
    pub fn prepare_mount_reads(&self) -> Result<(), PersistenceError> {
        self.session.prepare_mount_reads().map_err(Into::into)
    }
    /// Prepares the writer session's mount admission and owner release
    /// statements in one flight (#768): one round trip when the Store opens,
    /// so neither call prepares one.
    pub fn prepare_mount_writes(&self) -> Result<(), PersistenceError> {
        self.session.prepare_mount_writes().map_err(Into::into)
    }
    /// Makes this writer session reopen after a quarantine: the next call
    /// outside a held lane opens one new connection and settles what the
    /// closed one left before its own work (`session/reopen.rs`). The Store
    /// service's writer enables it; every other session stays quarantined
    /// until it is dropped. Reopens are counted in [`SqlWork::reopens`].
    pub fn enable_reopen(&self) {
        self.session.enable_reopen();
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
        self.locate_root_packs(roots, rows, chains, None)
    }
    fn locate_chains_with_packs(
        &self,
        roots: &[ObjectId],
        resident: &[i64],
        rows: &mut Vec<LocatedObject>,
        chains: &mut LocatedChains,
        bodies: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        bodies.clear();
        self.locate_root_packs(roots, rows, chains, Some((resident, bodies)))
    }
    /// Group reads (#758): every read's bytes in one statement per 2 MiB of
    /// returned bytes, through the multi-range `substring` reader
    /// (`objects::read::ranges`): a ranged read's ranges, and a whole read's
    /// body as one range, authenticated against the descriptor it named.
    /// Answers come in request order; an absent pack is `Missing`.
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
                    bytes += match read.ranges.is_empty() {
                        true => read.pack.length,
                        false => read.ranges.iter().map(|range| range.length).sum(),
                    };
                    bytes <= layerfs_storage::policy::DEPENDENCY_PACK_CACHE_BYTES
                })
                .count()
                .max(1);
            let requests: Vec<(i64, Vec<PackRange>)> = rest[..take]
                .iter()
                .map(|read| match read.ranges.is_empty() {
                    true => (
                        read.pack.pack_id,
                        vec![PackRange {
                            offset: 0,
                            length: read.pack.length,
                        }],
                    ),
                    false => (read.pack.pack_id, read.ranges.clone()),
                })
                .collect();
            let mut bodies = Vec::new();
            self.session
                .read_immutable(|tx| objects::read::ranges(tx, &requests, &mut bodies))?;
            for (read, mut body) in rest[..take].iter().zip(bodies) {
                out.push(match read.ranges.is_empty() {
                    true => GroupReadAnswer::Whole(PersistedPack::authenticate(
                        read.pack,
                        body.pop().ok_or(PersistenceError::Malformed)?,
                    )?),
                    false => GroupReadAnswer::Ranges(body),
                });
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

    fn publish_captured(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        publication::validate(batch, self.publication_bytes())?;
        match objects::publish_flight::eligible(&self.session, batch)? {
            true => objects::publish_flight::run(&self.session, batch).map_err(Into::into),
            false => self.publish(batch),
        }
    }
}
