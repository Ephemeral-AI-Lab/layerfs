//! One Storage producer handle of a bound Workspace, kept between its Commits.
//!
//! A Storage handle owns the bounded indexes a completed Save seeds: located
//! objects, pack descriptors and the content hints. A handle made for each
//! Commit reads the policy row and the content-hint table again; the handle
//! of a Commit that completed is kept and its next Save does not. A Commit
//! that did not complete drops its handle, and the next one starts from the
//! Store as before. The kept handle also holds its unused pack ids and a
//! small prefix of unused ordinals, so a small Commit reserves nothing, and
//! the metadata pack bodies and inode values its Saves published (bounded),
//! so the next Save reads its bases' recent chains without fetching them
//! (#768, `layerfs-storage`).
//!
//! The Store keeps one more idle handle (#352 F-025 fix 4). A closing
//! Workspace hands its idle handle there, and a Workspace without one takes
//! it, so an SDK file call (a private Workspace per call) does not reload
//! the policy row and content hints each time. One Save per handle at a
//! time still holds: a taken handle is in no slot.
//!
//! A handle dropped in order returns its kept ordinals first
//! (`Storage::release_kept_ordinals`, one write, never repeated): a closing
//! Workspace whose handle finds the Store's slot taken, a Store closed in
//! order ([`Store::release_producer`]) and a Commit that failed definitely
//! (`commit.rs`). Only a crash, a Workspace dropped without its close or an
//! unknown outcome leaves them lost; a steady Commit sends nothing for it.
use super::Store;
use layerfs_storage::{Diagnostics, Storage, StorageResult};
use std::sync::Mutex;

/// The idle handle, if the last Commit of this Workspace completed.
#[derive(Default)]
pub(super) struct Producer {
    idle: Mutex<Option<Storage>>,
}
impl Producer {
    /// The kept handle with its counts so far: this Workspace's, else the
    /// Store's idle one; or a new one with none.
    pub(super) fn take(&self, store: &Store) -> StorageResult<(Storage, Diagnostics)> {
        let kept = self.idle().or_else(|| store.idle_producer.idle());
        match kept {
            Some(storage) => {
                let before = storage.diagnostics();
                Ok((storage, before))
            }
            None => Ok((store.producer()?.keeping_ordinals(), Diagnostics::default())),
        }
    }
    /// Keeps the handle of a completed Commit. Admission makes this the only
    /// Commit of the Workspace, so no other handle can be idle here.
    pub(super) fn keep(&self, storage: Storage) {
        if let Ok(mut idle) = self.idle.lock() {
            *idle = Some(storage);
        }
    }
    fn idle(&self) -> Option<Storage> {
        self.idle.lock().ok().and_then(|mut idle| idle.take())
    }
    /// Its idle handle, if any, returns its kept ordinals and stays idle
    /// (its next Save reserves ordinals again).
    pub(super) fn release_ordinals(&self) {
        if let Some(storage) = self.idle() {
            let _ = storage.release_kept_ordinals();
            self.keep(storage);
        }
    }
    /// Moves this Workspace's idle handle to `to` when `to` has none;
    /// otherwise the handle returns its kept ordinals and is dropped.
    pub(super) fn hand_to(&self, to: &Self) {
        let Some(storage) = self.idle() else { return };
        let left = match to.idle.lock() {
            Ok(mut idle) if idle.is_none() => {
                *idle = Some(storage);
                None
            }
            _ => Some(storage),
        };
        if let Some(storage) = left {
            let _ = storage.release_kept_ordinals();
        }
    }
}

impl Store {
    /// Before an orderly close: the Store's idle producer handle returns its
    /// kept ordinals and is dropped (one write when it kept any). A refused
    /// or uncertain release leaves them lost; nothing is retried.
    pub fn release_producer(&self) {
        if let Some(storage) = self.idle_producer.idle() {
            let _ = storage.release_kept_ordinals();
        }
    }
}

/// One Commit's own counts of a handle that served earlier Commits. Peaks are
/// the handle's, not a difference.
pub(super) fn since(after: Diagnostics, before: Diagnostics) -> Diagnostics {
    Diagnostics {
        policy: after.policy - before.policy,
        locate: after.locate - before.locate,
        read_packs: after.read_packs - before.read_packs,
        read_pack_selections: after.read_pack_selections - before.read_pack_selections,
        whole_selected: after.whole_selected - before.whole_selected,
        whole_due_density: after.whole_due_density - before.whole_due_density,
        whole_due_singleton: after.whole_due_singleton - before.whole_due_singleton,
        whole_due_small: after.whole_due_small - before.whole_due_small,
        whole_due_reuse: after.whole_due_reuse - before.whole_due_reuse,
        range_selected: after.range_selected - before.range_selected,
        range_scan_bytes: after.range_scan_bytes - before.range_scan_bytes,
        range_acquired_bytes: after.range_acquired_bytes - before.range_acquired_bytes,
        range_materialized_bytes: after.range_materialized_bytes - before.range_materialized_bytes,
        value_groups: after.value_groups - before.value_groups,
        signatures: after.signatures - before.signatures,
        reserve: after.reserve - before.reserve,
        initial_reservations: after.initial_reservations - before.initial_reservations,
        reservation_refills: after.reservation_refills - before.reservation_refills,
        publish: after.publish - before.publish,
        payload_reads: after.payload_reads - before.payload_reads,
        payload_read_bytes: after.payload_read_bytes - before.payload_read_bytes,
        pack_read_bytes: after.pack_read_bytes - before.pack_read_bytes,
        pack_write_bytes: after.pack_write_bytes - before.pack_write_bytes,
        pooled_packs: after.pooled_packs - before.pooled_packs,
        pooled_groups: after.pooled_groups - before.pooled_groups,
        reserved_directory_bytes: after.reserved_directory_bytes - before.reserved_directory_bytes,
        ordinal_reservations: after.ordinal_reservations - before.ordinal_reservations,
        locator_hits: after.locator_hits - before.locator_hits,
        locator_misses: after.locator_misses - before.locator_misses,
        locator_evictions: after.locator_evictions - before.locator_evictions,
        locator_eviction_probes: after.locator_eviction_probes - before.locator_eviction_probes,
        locator_second_chances: after.locator_second_chances - before.locator_second_chances,
        pack_hits: after.pack_hits - before.pack_hits,
        pack_misses: after.pack_misses - before.pack_misses,
        pack_evictions: after.pack_evictions - before.pack_evictions,
        pack_evicted_bytes: after.pack_evicted_bytes - before.pack_evicted_bytes,
        directory_validations: after.directory_validations - before.directory_validations,
        directory_unshared_walks: after.directory_unshared_walks - before.directory_unshared_walks,
        directory_entry_bounds: after.directory_entry_bounds - before.directory_entry_bounds,
        prefetch_group_decodes: after.prefetch_group_decodes - before.prefetch_group_decodes,
        forced_seals: after.forced_seals - before.forced_seals,
        chain_rows: after.chain_rows - before.chain_rows,
        chain_members: after.chain_members - before.chain_members,
        chain_hinted_bases: after.chain_hinted_bases - before.chain_hinted_bases,
        chain_verified_bases: after.chain_verified_bases - before.chain_verified_bases,
        chain_hint_stale: after.chain_hint_stale - before.chain_hint_stale,
        chain_unhinted_roots: after.chain_unhinted_roots - before.chain_unhinted_roots,
        group_reads: after.group_reads - before.group_reads,
        group_read_packs: after.group_read_packs - before.group_read_packs,
        group_read_ranges: after.group_read_ranges - before.group_read_ranges,
        group_read_bytes: after.group_read_bytes - before.group_read_bytes,
        group_read_whole: after.group_read_whole - before.group_read_whole,
        ..after
    }
}
