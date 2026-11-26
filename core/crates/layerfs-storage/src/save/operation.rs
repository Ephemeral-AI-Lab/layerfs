//! Save lifetime, one producer and explicit terminal failures.
use super::{state::State, WriteOutcome};
use crate::{
    error::{StorageError, StorageResult},
    pack::layout::PackLane,
    storage::Storage,
};
use layerfs_content::{ContentError, ContentResult, FinalizedConsumer, FinalizedObject};
use std::cell::{Cell, RefCell};

/// One bounded producer. Published reference-closed batches survive later failure.
pub struct Save<'a> {
    pub(super) state: RefCell<State<'a>>,
    pub(super) terminal: Cell<bool>,
    pub(super) join_metadata_reads: bool,
    pub(super) captured: Cell<bool>,
    failure: RefCell<Option<StorageError>>,
    finished: Cell<bool>,
    work_start: super::SaveWork,
}
impl<'a> Save<'a> {
    pub(crate) fn new(storage: &'a Storage) -> StorageResult<Self> {
        let work_start = storage.work.snapshot().total;
        let _work = storage.work.span(super::Stage::Begin);
        // The bodies and values earlier Saves on this handle published stay
        // (bounded) for this Save's reads (#768).
        Ok(Self {
            work_start,
            state: RefCell::new(State::new(storage)?),
            terminal: Cell::new(false),
            join_metadata_reads: false,
            captured: Cell::new(false),
            failure: RefCell::new(None),
            finished: Cell::new(false),
        })
    }
    /// Combines authoritative metadata locates and body reads where the
    /// provider supports it. Used by the captured Commit producer.
    pub fn joining_metadata_reads(mut self) -> Self {
        self.join_metadata_reads = true;
        self
    }
    /// Accepts a finalized object, preparing the preceding bounded wave as needed.
    pub fn accept(&self, object: FinalizedObject) -> StorageResult<()> {
        self.accept_output(object, false)
    }
    fn accept_output(&self, object: FinalizedObject, captured: bool) -> StorageResult<()> {
        if self.terminal.get() {
            return Err(StorageError::Aborted);
        }
        let result = (|| {
            let mut state = self.state.borrow_mut();
            let storage = state.storage;
            let _published = storage.source.serving_published();
            let _paired = storage.source.pairing_inode_values(
                captured
                    && self.join_metadata_reads
                    && storage.source.metadata.batched_chain_reads(),
            );
            if let Some(objects) = state.pending.push(object)? {
                state.captured_output = captured;
                if captured && storage.source.metadata.batched_chain_reads() {
                    state.packs.retain_requested_whole_file_groups();
                }
                let written = state.wave(objects);
                state.captured_output = false;
                written?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.terminal.set(true);
        }
        result
    }
    /// C1 handoff consumer sharing this save's same-operation provider.
    pub fn sink(&self) -> SaveSink<'_, 'a> {
        SaveSink {
            save: self,
            captured: false,
        }
    }
    /// Accepts output from the captured Commit producer before history work.
    pub fn captured_sink(&self) -> SaveSink<'_, 'a> {
        self.captured.set(true);
        SaveSink {
            save: self,
            captured: true,
        }
    }
    /// Seals and registers the remaining reference-closed output once.
    pub fn finish(self) -> StorageResult<WriteOutcome> {
        let held = self.seal(false)?;
        if held.is_some() {
            return Err(StorageError::Integrity("held finish batch"));
        }
        Ok(self.complete())
    }
    /// [`Self::finish`], except that the last publication is prepared and
    /// not sent: the tail carries it for its caller to publish in a
    /// transaction of its own (a fused Commit, #768), or to publish as
    /// `finish` would ([`PreparedTail::publish`]). Earlier batches that
    /// reached a publication bound are published here, as `finish` would.
    /// While the tail lives, every read of the handle (readers continuing
    /// this Save, [`crate::Storage::reader_after_save`], included) sees the
    /// held batch's locations, pack bodies and values whole, from the batch
    /// itself (`read/held.rs`), not from the bounded caches.
    pub fn finish_prepared(self) -> StorageResult<PreparedTail<'a>> {
        let held = self.seal(true)?;
        Ok(PreparedTail {
            save: Some(self),
            held,
            handed_out: std::cell::Cell::new(false),
        })
    }
    /// Seals the remaining output and registers it; with `hold`, the last
    /// batch is returned instead of published.
    fn seal(&self, hold: bool) -> StorageResult<Option<super::publication::Held>> {
        if let Some(error) = self.failure.borrow_mut().take() {
            return Err(error);
        }
        if self.terminal.get() {
            return Err(StorageError::Aborted);
        }
        let mut state = self.state.borrow_mut();
        let storage = state.storage;
        let _published = storage.source.serving_published();
        let _paired = storage.source.pairing_inode_values(
            self.captured.get()
                && self.join_metadata_reads
                && storage.source.metadata.batched_chain_reads(),
        );
        let objects = state.pending.drain();
        // The last wave is published below, with the sealed tail. Objects
        // held for a base of this Save (#810) are selected before it, after
        // one publication of those bases.
        state.prepare(objects)?;
        if !state.deferred.is_empty() {
            state.publish_wave()?;
        }
        let closing = state.storage.work.span(super::Stage::FinishClose);
        state.finishing = true;
        if state.packer.unfinished() {
            let required = state.packer.finish_pack_bound();
            state.reserve_packs(required)?;
        }
        for lane in PackLane::ALL {
            let State {
                packer,
                compression,
                next_pack,
                pack_end,
                ..
            } = &mut *state;
            packer.seal(lane, compression, next_pack, *pack_end)?;
        }
        {
            let State {
                packer,
                next_pack,
                pack_end,
                ..
            } = &mut *state;
            packer.flush(next_pack, *pack_end)?;
        }
        state.flush_signatures()?;
        drop(closing);
        state.register_ready(hold)
    }
    /// After every batch was acknowledged: the unused pack ids and the kept
    /// ordinal prefix stay with the handle (#768).
    fn complete(&self) -> WriteOutcome {
        let mut state = self.state.borrow_mut();
        state.keep_rest();
        state.outcome.pool = state.pool_stats;
        state.outcome.delta = state.delta;
        state.profile.group_ns += state.packer.group_ns;
        state.storage.work.close(self.work_start, state.profile);
        self.finished.set(true);
        state.outcome
    }
    /// Takes the original error retained by a C1 sink before abandoning a save.
    pub fn take_failure(&self) -> Option<StorageError> {
        self.failure.borrow_mut().take()
    }
    pub(super) fn remember_failure(&self, error: StorageError) {
        let mut first = self.failure.borrow_mut();
        if first.is_none() {
            *first = Some(error);
        }
    }
    /// Actual delta-selection outcomes for this producer.
    pub fn delta_counters(&self) -> crate::encoding::delta::select::DeltaCounters {
        self.state.borrow().delta
    }
    /// Actual dependency reconstruction work for this producer.
    pub fn chain_counters(&self) -> crate::encoding::delta::read::ChainCounters {
        self.state.borrow().chain_total
    }
    /// Bytes charged by this Save's bounded memo of the values it assigned.
    pub fn pooled_index_bytes(&self) -> usize {
        self.state.borrow().value_memo.len() * 12
    }
    /// Canonical bytes waiting in the bounded producer batch, and in the
    /// objects held for a base this Save has not published yet.
    pub fn pending_canonical_bytes(&self) -> u64 {
        let state = self.state.borrow();
        state.pending.canonical_bytes() + state.deferred.canonical_bytes()
    }
}
impl Drop for Save<'_> {
    fn drop(&mut self) {
        if !self.finished.get() {
            let state = self.state.get_mut();
            state.candidates.invalidate();
            state.storage.source.invalidate_signatures();
            // A Save that failed, refused or never finished keeps no
            // published bodies or values for the next (#768). Unless one of
            // its writes ended uncertain, its unused ids (from the first it
            // did not assign) go back to the handle: its next Save uses them,
            // or its owner returns the ordinals before an orderly drop.
            state.storage.kept.set(state.unused_rest());
            state.storage.source.forget_published();
        }
    }
}
/// A finished Save's last publication, prepared and not yet sent
/// ([`Save::finish_prepared`]). Exactly one of [`Self::acknowledged`] (its
/// caller published [`Self::publication`] in its own transaction, nothing
/// lost) and [`Self::publish`] (publish it now, as `finish` does) completes
/// the Save. Dropped without either, the Save did not finish: its held
/// locations and the handle's published bodies are forgotten. Its unused
/// ids go to the handle when its publication was never handed out, or after
/// [`Self::refused`]; once handed out, a drop is an unknown outcome and they
/// are lost.
pub struct PreparedTail<'a> {
    save: Option<Save<'a>>,
    held: Option<super::publication::Held>,
    /// [`Self::publication`] was handed to a caller that may have sent it.
    handed_out: std::cell::Cell<bool>,
}
impl PreparedTail<'_> {
    /// The publication still to send: `None` when everything was already
    /// published (or nothing was new).
    pub fn publication(&self) -> Option<&crate::port::Publication> {
        self.handed_out.set(self.held.is_some());
        self.held.as_ref().map(|held| &held.batch)
    }
    /// The Save's outcome once [`Self::publication`] is acknowledged with
    /// nothing lost, without completing it: the counts a Commit records.
    pub fn prospective(&self) -> WriteOutcome {
        let Some(save) = self.save.as_ref() else {
            return WriteOutcome::default();
        };
        let state = save.state.borrow();
        let mut outcome = state.outcome;
        if let Some(held) = &self.held {
            super::publication::counted(
                &mut outcome,
                &held.batch,
                &held.members,
                &Default::default(),
            );
        }
        outcome.pool = state.pool_stats;
        outcome.delta = state.delta;
        outcome
    }
    /// The caller's transaction published [`Self::publication`] exactly,
    /// every location inserted (none lost), and committed: the Save
    /// completes as an acknowledged `finish`.
    pub fn acknowledged(mut self) -> StorageResult<WriteOutcome> {
        if let (Some(save), Some(held)) = (self.save.as_ref(), self.held.as_ref()) {
            let mut state = save.state.borrow_mut();
            // The Store has the batch now, its ordinal release included.
            state.unapplied_release = None;
            state.storage.source.release_held();
            state.storage.source.note(|c| c.publish += 1);
            state.settle(&held.batch, &held.members, &[])?;
        }
        self.held = None;
        let save = self.save.take().ok_or(StorageError::Aborted)?;
        Ok(save.complete())
    }
    /// Publishes [`Self::publication`] on its own, as `finish` does (lost
    /// locations compared with their winners), and completes the Save.
    pub fn publish(mut self) -> StorageResult<WriteOutcome> {
        // Sent here: this Save's own write decides whether a failure is
        // uncertain (`State::written`); a caller that was handed the
        // publication had nothing applied (`Fused::Overflow`, a race).
        self.handed_out.set(false);
        if let (Some(save), Some(held)) = (self.save.as_ref(), self.held.as_mut()) {
            let mut state = save.state.borrow_mut();
            let storage = state.storage;
            let _published = storage.source.serving_published();
            // A lost location's winner must be read from the Store, not
            // from the held batch.
            storage.source.release_held();
            state.acknowledge(&mut held.batch, &held.members)?;
        }
        self.held = None;
        let save = self.save.take().ok_or(StorageError::Aborted)?;
        Ok(save.complete())
    }
}
impl PreparedTail<'_> {
    /// The caller's transaction carrying [`Self::publication`] was refused
    /// definitely: it was rolled back and none of it applied, its ordinal
    /// release included. The Save does not finish (as when the tail is
    /// dropped), but its unused pack ids and ordinals, from the first ones
    /// it did not assign up to its reserved ends, go to the handle's kept
    /// block, so the handle's next Save uses them or its owner returns the
    /// ordinals ([`crate::Storage::release_kept_ordinals`]). The ids the
    /// refused publication assigned are below them and never reused. Never
    /// call it after an unknown outcome: dropping the tail loses the block.
    pub fn refused(self) {
        self.handed_out.set(false);
    }
}
impl Drop for PreparedTail<'_> {
    fn drop(&mut self) {
        let Some(save) = self.save.as_ref() else {
            return;
        };
        let mut state = save.state.borrow_mut();
        // Not acknowledged: the held locations were never the Store's.
        if let Some(held) = self.held.as_ref() {
            let ids: Vec<_> = held.batch.objects.iter().map(|row| row.object_id).collect();
            state.storage.source.release_held();
            state.storage.source.invalidate(&ids);
        }
        // Handed out and neither acknowledged nor refused: whether the
        // caller's transaction applied it is unknown, so the Save (dropped
        // next) keeps no id for the handle.
        if self.handed_out.get() {
            state.write_uncertain = true;
        }
    }
}
/// C1's consumer adapter; the original typed storage error is retained for finish.
pub struct SaveSink<'s, 'a> {
    save: &'s Save<'a>,
    captured: bool,
}
impl FinalizedConsumer for SaveSink<'_, '_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.save
            .accept_output(object, self.captured)
            .map_err(|error| {
                self.save.remember_failure(error);
                ContentError::OutputRejected
            })
    }
}
