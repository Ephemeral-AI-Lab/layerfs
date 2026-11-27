//! One bounded canonical demand using the existing authenticated resolver.

use super::{prefetch, Fetch};
use crate::{
    encoding::{
        codec::DecompressionWorkspace,
        delta::read::{BodyCaches, ChainCounters, Resolver},
        pool::{read::PooledCaches, PoolReader},
        GroupCache,
    },
    error::{StorageError, StorageResult},
    policy::{StorageCapacities, READ_CANONICAL_BYTES_LIMIT},
    port::PersistenceError,
    source::Source,
};
use layerfs_content::ObjectId;
use std::{
    collections::BTreeSet,
    ops::{Deref, DerefMut},
    sync::{Mutex, PoisonError},
};

/// Decode regions of finished reads, kept for the next read (F-043).
///
/// A freed 1 MiB decode region stays in the glibc arena of the thread that
/// freed it, like the encode regions (`save/state.rs`): about 80 MB in the
/// SDK runner. Only plain regions are kept, never a zstd context, and at most
/// one per available CPU; a region past that is freed. Releasing them when
/// the Store closes is a follow-up once `store/handle.rs` is free.
static IDLE_DECODE: Mutex<Vec<Vec<u64>>> = Mutex::new(Vec::new());

/// A decode workspace that takes an idle region when there is one (so its
/// region is already resident) and returns its region when dropped.
pub(super) struct Decoder(Option<DecompressionWorkspace>);
impl Decoder {
    pub(super) fn new() -> StorageResult<Self> {
        let idle = IDLE_DECODE
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();
        Ok(Self(Some(match idle {
            Some(region) => DecompressionWorkspace::in_region(region)?,
            None => DecompressionWorkspace::new()?,
        })))
    }
}
impl Deref for Decoder {
    type Target = DecompressionWorkspace;
    fn deref(&self) -> &DecompressionWorkspace {
        self.0.as_ref().expect("decoder until drop")
    }
}
impl DerefMut for Decoder {
    fn deref_mut(&mut self) -> &mut DecompressionWorkspace {
        self.0.as_mut().expect("decoder until drop")
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        let Some(region) = self.0.take().and_then(DecompressionWorkspace::into_region) else {
            return;
        };
        let bound = std::thread::available_parallelism().map_or(1, usize::from);
        let mut idle = IDLE_DECODE.lock().unwrap_or_else(PoisonError::into_inner);
        if idle.len() < bound {
            idle.push(region);
        }
    }
}

/// One owner's bounded pack, group and pooled-value caches. The decode
/// arena is the caller's, passed to each read.
pub(crate) struct ReadState {
    groups: GroupCache,
    pool: PoolReader,
    prefetch_pool: crate::encoding::pool::PoolReadCounters,
    packs: crate::encoding::PackCache,
}
impl ReadState {
    pub(crate) fn new() -> Self {
        Self {
            groups: GroupCache::new(),
            pool: PoolReader::new(),
            prefetch_pool: crate::encoding::pool::PoolReadCounters::default(),
            packs: crate::encoding::PackCache::new(),
        }
    }
    /// Body, decoded-group and pooled-value bytes kept now; the decode arena
    /// is apart. The read path passes its pool reader the shared pack cache,
    /// so the pool's own pack cache stays empty here.
    pub(crate) fn retained_bytes(&self) -> usize {
        self.packs.retained_bytes() + self.groups.retained_bytes() + self.pool.retained_bytes()
    }
    pub(crate) fn pooled_read_counters(&self) -> crate::encoding::pool::PoolReadCounters {
        let mut work = self.pool.counters();
        work.accumulate(self.prefetch_pool);
        work
    }
    pub(crate) fn read(
        &mut self,
        workspace: &mut DecompressionWorkspace,
        source: &Fetch,
        capacities: &StorageCapacities,
        ids: &[ObjectId],
    ) -> StorageResult<Vec<Vec<u8>>> {
        if ids.len() > capacities.read_objects {
            return Err(StorageError::CapacityExceeded {
                what: "storage.read_objects",
                limit: capacities.read_objects as u64,
                actual: ids.len() as u64,
            });
        }
        source.begin_demand(true);
        self.pool.forget_prepared();
        source.locate(ids)?;
        let roots = ids
            .iter()
            .map(|id| {
                source
                    .location(*id, i64::MAX)?
                    .ok_or(StorageError::ObjectMissing(*id))
            })
            .collect::<StorageResult<Vec<_>>>()?;
        let bytes = roots.iter().try_fold(0usize, |total, row| {
            total
                .checked_add(row.canonical_length)
                .ok_or(StorageError::Integrity("read byte accounting"))
        })?;
        if bytes > READ_CANONICAL_BYTES_LIMIT {
            return Err(StorageError::CapacityExceeded {
                what: "storage.read_canonical_bytes",
                limit: READ_CANONICAL_BYTES_LIMIT as u64,
                actual: bytes as u64,
            });
        }
        let mut counters = ChainCounters::default();
        // Output slots preserve demand order, including repeated identities.
        // Only the bounded root worklist is scheduled by physical location.
        let mut order: Vec<_> = (0..roots.len()).collect();
        order.sort_unstable_by_key(|position| {
            let row = roots[*position];
            (row.pack_id, row.group_number, row.record_number)
        });
        let mut out = vec![Vec::new(); ids.len()];
        let sorted: Vec<_> = order.iter().map(|p| roots[*p]).collect();
        // A demand whose packs pass the pack cache's bound is served one
        // cohort of packs at a time, each within the bound, so a pack it
        // acquires stays cached until its objects are resolved (#97). When
        // the locate returned chains (#758), a cohort is the roots one page
        // of groups covers, their chains' included, in one call.
        let mut start = 0;
        while start < order.len() {
            let paged = source.page(sorted[start..].chunks(1), &mut self.packs)?;
            let end = start
                + match paged {
                    0 => source.cohort(sorted[start..].iter().map(|row| row.pack_id)),
                    covered => covered,
                };
            let cohort = &sorted[start..end];
            if paged == 0 {
                let located: Vec<i64> = cohort.iter().map(|row| row.pack_id).collect();
                source.whole_packs(&located, &mut self.packs)?;
            }
            let mut walk =
                || prefetch::chains(source, cohort, &mut self.packs, workspace, &mut self.groups);
            // A walk a stale hint broke runs once more without hints.
            let prefetch = match walk() {
                Err(error) if super::hint_failure(&error) && source.drop_hints(&[])? => walk()?,
                other => other?,
            };
            self.prefetch_pool.accumulate(prefetch);
            let mut from = start;
            while from < end {
                let id = roots[order[from]].pack_id;
                let next = from
                    + order[from..end]
                        .iter()
                        .take_while(|position| roots[**position].pack_id == id)
                        .count();
                let needed: Vec<_> = order[from..next]
                    .iter()
                    .map(|position| roots[*position].group_number)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                self.packs.acquire_groups(source, id, &needed)?;
                for (at, &position) in order[from..next].iter().enumerate() {
                    let root = roots[position];
                    // The pooled leaves from here on are rebuilt ahead and their
                    // stored values looked up together, a bounded window at a
                    // time (#95); their packs are this cohort's, kept cached.
                    if root.role == layerfs_content::ObjectRole::InodeLeaf
                        && !self.pool.is_prepared(&root.object_id)
                    {
                        let ahead: Vec<_> =
                            order[from + at..end].iter().map(|p| roots[*p]).collect();
                        self.pool.prepare_leaves(
                            source,
                            capacities,
                            i64::MAX,
                            workspace,
                            &ahead,
                            Some(PooledCaches {
                                encoded: &mut self.packs,
                                decoded: &mut self.groups,
                            }),
                        )?;
                    }
                    let (canonical, _) = Resolver::new(
                        source,
                        i64::MAX,
                        capacities,
                        BodyCaches {
                            packs: &mut self.packs,
                            pool: &mut self.pool,
                        },
                        &mut self.groups,
                        workspace,
                        &mut counters,
                    )
                    .resolve_at(root)?;
                    out[position] = canonical;
                }
                from = next;
            }
            start = end;
        }
        Ok(out)
    }
    /// [`Self::read`] with one result per requested position (#818 H5): the
    /// same demand, steps, caches and order, so healthy objects cost what
    /// they cost there. A session, transport or admission failure, a
    /// failure of a call that serves several units at once (locate, page,
    /// whole packs, a chain level's group read and locate, a leaf window's
    /// value lookup that fails as a call) and the demand's bounds fail the
    /// call, as there. Any other failure is the result of exactly the
    /// objects that need the failed unit (an absent object or base, a pack,
    /// a chain record, a leaf, a body): the failure each one's point read
    /// meets, since that read runs the same steps over its own units in
    /// the same order. Pack reads over several packs are classified
    /// ([`Fetch::per_object`]), so a pack refused among them is that pack's
    /// failure. A failed unit is not requested again in the demand.
    pub(crate) fn read_each(
        &mut self,
        workspace: &mut DecompressionWorkspace,
        source: &Fetch,
        capacities: &StorageCapacities,
        ids: &[ObjectId],
    ) -> StorageResult<Vec<StorageResult<Vec<u8>>>> {
        if ids.len() > capacities.read_objects {
            return Err(StorageError::CapacityExceeded {
                what: "storage.read_objects",
                limit: capacities.read_objects as u64,
                actual: ids.len() as u64,
            });
        }
        source.begin_demand(true);
        let _per_object = source.per_object();
        self.pool.forget_prepared();
        source.locate(ids)?;
        let mut out: Vec<Option<StorageResult<Vec<u8>>>> = ids.iter().map(|_| None).collect();
        // Located roots with their output slots; an absent one is its own result.
        let mut roots = Vec::with_capacity(ids.len());
        for (position, id) in ids.iter().enumerate() {
            match source.location(*id, i64::MAX)? {
                Some(row) => roots.push((position, row)),
                None => out[position] = Some(Err(StorageError::ObjectMissing(*id))),
            }
        }
        let bytes = roots.iter().try_fold(0usize, |total, (_, row)| {
            total
                .checked_add(row.canonical_length)
                .ok_or(StorageError::Integrity("read byte accounting"))
        })?;
        if bytes > READ_CANONICAL_BYTES_LIMIT {
            return Err(StorageError::CapacityExceeded {
                what: "storage.read_canonical_bytes",
                limit: READ_CANONICAL_BYTES_LIMIT as u64,
                actual: bytes as u64,
            });
        }
        let mut counters = ChainCounters::default();
        roots.sort_by_key(|(_, row)| (row.pack_id, row.group_number, row.record_number));
        let sorted: Vec<_> = roots.iter().map(|(_, row)| *row).collect();
        let live = |out: &[Option<_>], at: usize| out[roots[at].0].is_none();
        let mut start = 0;
        while start < sorted.len() {
            let paged = source.page(sorted[start..].chunks(1), &mut self.packs)?;
            let end = start
                + match paged {
                    0 => source.cohort(sorted[start..].iter().map(|row| row.pack_id)),
                    covered => covered,
                };
            let cohort = &sorted[start..end];
            if paged == 0 {
                let located: Vec<i64> = cohort.iter().map(|row| row.pack_id).collect();
                source.whole_packs_each(&located, &mut self.packs)?;
            }
            let mut walk = || {
                prefetch::chains_each(source, cohort, &mut self.packs, workspace, &mut self.groups)
            };
            // A walk a stale hint broke runs once more without hints, as in `read`.
            let (prefetch, failed) = match walk()? {
                (_, failed)
                    if failed.iter().any(|(_, error)| super::hint_failure(error))
                        && source.drop_hints(&[])? =>
                {
                    walk()?
                }
                walked => walked,
            };
            self.prefetch_pool.accumulate(prefetch);
            for (at, error) in failed {
                out[roots[start + at].0] = Some(Err(error));
            }
            let mut from = start;
            while from < end {
                let id = sorted[from].pack_id;
                let next = from
                    + sorted[from..end]
                        .iter()
                        .take_while(|row| row.pack_id == id)
                        .count();
                let needed: Vec<_> = (from..next)
                    .filter(|at| live(&out, *at))
                    .map(|at| sorted[at].group_number)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if !needed.is_empty() {
                    let acquired = match source.refused_pack(id) {
                        Some(error) => Err(error),
                        None => self.packs.acquire_groups(source, id, &needed).map(drop),
                    };
                    if let Err(error) = acquired {
                        if call_failure(&error) {
                            return Err(error);
                        }
                        for at in from..next {
                            if live(&out, at) {
                                out[roots[at].0] = Some(Err(duplicate(&error)));
                            }
                        }
                        source.refuse(id, error);
                    }
                }
                for at in from..next {
                    let (position, root) = roots[at];
                    if !live(&out, at) {
                        continue;
                    }
                    if root.role == layerfs_content::ObjectRole::InodeLeaf
                        && !self.pool.is_prepared(&root.object_id)
                    {
                        let ahead: Vec<_> = (at..end)
                            .filter(|k| live(&out, *k))
                            .map(|k| sorted[k])
                            .collect();
                        let failed = self.pool.prepare_leaves_each(
                            source,
                            capacities,
                            i64::MAX,
                            workspace,
                            &ahead,
                            Some(PooledCaches {
                                encoded: &mut self.packs,
                                decoded: &mut self.groups,
                            }),
                            call_failure,
                        )?;
                        for (object, error) in failed {
                            for k in at..end {
                                if sorted[k].object_id == object && live(&out, k) {
                                    out[roots[k].0] = Some(Err(duplicate(&error)));
                                }
                            }
                        }
                        if !live(&out, at) {
                            continue;
                        }
                    }
                    let resolved = Resolver::new(
                        source,
                        i64::MAX,
                        capacities,
                        BodyCaches {
                            packs: &mut self.packs,
                            pool: &mut self.pool,
                        },
                        &mut self.groups,
                        workspace,
                        &mut counters,
                    )
                    .resolve_at(root);
                    out[position] = Some(match resolved {
                        Ok((canonical, _)) => Ok(canonical),
                        Err(error) if call_failure(&error) => return Err(error),
                        Err(error) => Err(error),
                    });
                }
                from = next;
            }
            start = end;
        }
        out.into_iter()
            .map(|slot| slot.ok_or(StorageError::Integrity("per-object read outcome")))
            .collect()
    }
}

/// Whether `error` fails a per-object demand as a whole: a session,
/// transport or admission failure, or an unknown outcome. A provider's
/// `Missing` or `Malformed` answer for one unit, and every physical, framing,
/// content or bound failure of one unit, belong to the objects that need it.
pub(crate) fn call_failure(error: &StorageError) -> bool {
    match error {
        StorageError::Io(io) => !matches!(
            io.get_ref()
                .and_then(|inner| inner.downcast_ref::<PersistenceError>()),
            Some(PersistenceError::Missing | PersistenceError::Malformed)
        ),
        StorageError::Busy
        | StorageError::OwnershipUnavailable
        | StorageError::UninspectedState { .. }
        | StorageError::UnsupportedPolicy { .. }
        | StorageError::UnknownOutcome { .. }
        | StorageError::CleanupFailed { .. }
        | StorageError::Aborted => true,
        _ => false,
    }
}

/// The same failure again, for each object that needs one failed unit.
pub(crate) fn duplicate(error: &StorageError) -> StorageError {
    use StorageError as E;
    match error {
        E::Busy => E::Busy,
        E::Content(error) => E::Content(error.clone()),
        E::Io(io) => E::Io(match (io.raw_os_error(), io.get_ref()) {
            (Some(code), _) => std::io::Error::from_raw_os_error(code),
            (None, None) => std::io::Error::from(io.kind()),
            (None, Some(inner)) => match inner.downcast_ref::<PersistenceError>() {
                Some(inner) => std::io::Error::new(io.kind(), inner.clone()),
                None => std::io::Error::new(io.kind(), inner.to_string()),
            },
        }),
        E::ObjectMissing(id) => E::ObjectMissing(*id),
        E::Unpublished(id) => E::Unpublished(*id),
        E::Collision(id) => E::Collision(*id),
        E::MissingDependency { object, reference } => E::MissingDependency {
            object: *object,
            reference: *reference,
        },
        E::VisibilityCeiling { pack_id, ceiling } => E::VisibilityCeiling {
            pack_id: *pack_id,
            ceiling: *ceiling,
        },
        E::OwnershipUnavailable => E::OwnershipUnavailable,
        E::UninspectedState {
            ceiling,
            highest_pack_id,
        } => E::UninspectedState {
            ceiling: *ceiling,
            highest_pack_id: *highest_pack_id,
        },
        E::UnsupportedPolicy { field } => E::UnsupportedPolicy { field },
        E::Integrity(what) => E::Integrity(what),
        E::CapacityExceeded {
            what,
            limit,
            actual,
        } => E::CapacityExceeded {
            what,
            limit: *limit,
            actual: *actual,
        },
        E::UnknownOutcome { original } => E::UnknownOutcome {
            original: Box::new(duplicate(original)),
        },
        E::CleanupFailed { original, cleanup } => E::CleanupFailed {
            original: Box::new(duplicate(original)),
            cleanup: Box::new(duplicate(cleanup)),
        },
        E::Aborted => E::Aborted,
    }
}
