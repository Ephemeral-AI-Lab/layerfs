//! FIFO current-job completion and arbitrarily deep external LIFO cursors.
use super::{count_index, release_index, release_state::ReleaseAttempt, ScratchSession};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    BaseFact, CanonicalScope, ReleaseFrame, ReleaseJob, ReleaseSeal,
};
use layerfs_content::ObjectId;
impl ScratchSession {
    /// Take the oldest pending FIFO job once into the exact current capsule.
    /// Removal from SQL retains the complete job and its shared occupancy until
    /// known completion; a pending/uncertain take cannot be replayed.
    pub fn release_take(&mut self, scope: &CanonicalScope) -> StorageResult<Option<ReleaseJob>> {
        self.release_context(scope)?;
        let result = (|| {
            let r = self.release_context(scope)?;
            let state = r
                .releasing
                .as_ref()
                .ok_or(StorageError::Integrity("release not begun"))?;
            if state.snapshot.stage != 2 || state.snapshot.current.is_some() {
                return Err(StorageError::Integrity("release take phase/current"));
            }
            let job = release_index::job(r.verify()?, scope)?;
            let Some(job) = job else {
                if state.snapshot.pending != 0 {
                    return Err(StorageError::Integrity("release FIFO empty count"));
                }
                return Ok(None);
            };
            if state.snapshot.pending == 0 {
                return Err(StorageError::Integrity("release FIFO unexpected row"));
            }
            let mut a = ReleaseAttempt::new(state, "take", 0)?;
            a.taken = Some(job);
            a.after.pending -= 1;
            a.after.current = Some(job);
            self.release_change(scope, a)?;
            Ok(Some(job))
        })();
        self.finish(result)
    }
    /// Complete this exact current job, optionally pushing one directory frame.
    /// Admit the proposed depth/population before SQL and preserve full current
    /// job plus frame custody through an uncertain acknowledgement.
    pub fn release_complete_job(
        &mut self,
        scope: &CanonicalScope,
        job: &ReleaseJob,
        directory: Option<ObjectId>,
    ) -> StorageResult<()> {
        self.release_context(scope)?;
        let result = (|| {
            let r = self.release_context(scope)?;
            let state = r.releasing.as_ref().unwrap();
            if state.snapshot.stage != 2 || state.snapshot.current != Some(*job) {
                return Err(StorageError::Integrity("release exact current completion"));
            }
            let mut a = ReleaseAttempt::new(state, "complete", 0)?;
            a.after.current = None;
            a.after.completed = a
                .after
                .completed
                .checked_add(1)
                .filter(|n| *n <= i64::MAX as u64)
                .ok_or(StorageError::Integrity("release completed overflow"))?;
            if let Some(root) = directory {
                let depth = a
                    .after
                    .frames
                    .checked_add(1)
                    .filter(|n| *n <= i64::MAX as u64)
                    .ok_or(StorageError::Integrity("release depth exhausted"))?;
                let frame = ReleaseFrame {
                    depth,
                    root,
                    after: None,
                    finished: false,
                };
                let frames = scope.frames()?;
                ReleaseFrame::decode(&frames, &frames.key(depth)?, &frame.encode_value())?;
                a.frame = Some((None, Some(frame)));
                a.after.frames = depth;
                a.after.maximum_depth = a.after.maximum_depth.max(depth);
                a.after.directories = a
                    .after
                    .directories
                    .checked_add(1)
                    .filter(|n| *n <= i64::MAX as u64)
                    .ok_or(StorageError::Integrity("release directory overflow"))?;
            }
            self.release_change(scope, a)
        })();
        self.finish(result)
    }
    /// Read the highest exact LIFO cursor only after pending/current jobs end.
    /// Arbitrary admitted depth resides in the native table, not a resident stack.
    pub fn release_frame(&mut self, scope: &CanonicalScope) -> StorageResult<Option<ReleaseFrame>> {
        self.release_context(scope)?;
        self.finish((|| {
            let r = self.release_context(scope)?;
            let state = r.releasing.as_ref().unwrap();
            if state.snapshot.stage != 2
                || state.snapshot.current.is_some()
                || state.snapshot.pending != 0
            {
                return Err(StorageError::Integrity("release frame before pending EOF"));
            }
            let frame = release_index::frame(r.verify()?, &scope.frames()?)?;
            if frame.map(|f| f.depth)
                != ((state.snapshot.frames > 0).then_some(state.snapshot.frames))
            {
                return Err(StorageError::Integrity("release LIFO exact depth"));
            }
            Ok(frame)
        })())
    }
    /// Compare a complete highest frame and advance its full continuation once.
    /// Enqueue at most128 carried-base children in the same known transaction;
    /// all selected values and the proposed frame remain retained through Unknown.
    pub fn release_advance(
        &mut self,
        scope: &CanonicalScope,
        before: &ReleaseFrame,
        after: &ReleaseFrame,
        children: &[BaseFact],
    ) -> StorageResult<ReleaseFrame> {
        self.release_context(scope)?;
        let result = (|| {
            let r = self.release_context(scope)?;
            let state = r.releasing.as_ref().unwrap();
            before.advances_to(*after)?;
            if children.len() > 128
                || state.snapshot.stage != 2
                || state.snapshot.pending != 0
                || state.snapshot.current.is_some()
                || before.depth != state.snapshot.frames
                || release_index::frame(r.verify()?, &scope.frames()?)? != Some(*before)
            {
                return Err(StorageError::Integrity("release selected frame advance"));
            }
            let frames = scope.frames()?;
            ReleaseFrame::decode(&frames, &frames.key(after.depth)?, &after.encode_value())?;
            let mut a = ReleaseAttempt::new(state, "advance", children.len())?;
            a.frame = Some((Some(*before), Some(*after)));
            for child in children {
                let facts = r.facts.as_ref().unwrap();
                if super::fact_index::base(
                    r.verify()?,
                    facts.base.scope.as_ref().unwrap(),
                    child.serial,
                )? != Some(*child)
                {
                    return Err(StorageError::Integrity("release established child base"));
                }
                a.enqueue(child.serial, child.value, scope)?;
            }
            self.release_change(scope, a)?;
            Ok(*after)
        })();
        self.finish(result)
    }
    /// Pop one exact finished highest frame after pending/current jobs end.
    /// An unfinished or stale cursor cannot remove any frame.
    pub fn release_pop(
        &mut self,
        scope: &CanonicalScope,
        frame: &ReleaseFrame,
    ) -> StorageResult<()> {
        self.release_context(scope)?;
        let result = (|| {
            let r = self.release_context(scope)?;
            let state = r.releasing.as_ref().unwrap();
            if state.snapshot.stage != 2
                || state.snapshot.pending != 0
                || state.snapshot.current.is_some()
                || !frame.finished
                || frame.depth != state.snapshot.frames
                || release_index::frame(r.verify()?, &scope.frames()?)? != Some(*frame)
            {
                return Err(StorageError::Integrity("release exact finished pop"));
            }
            let mut a = ReleaseAttempt::new(state, "pop", 0)?;
            a.frame = Some((Some(*frame), None));
            a.after.frames -= 1;
            self.release_change(scope, a)
        })();
        self.finish(result)
    }
    /// Seal only exact current-job, pending-job and frame EOF with full counters.
    /// Check actual native projections before the terminal acknowledgement.
    pub fn release_seal(&mut self, scope: &CanonicalScope) -> StorageResult<ReleaseSeal> {
        self.release_context(scope)?;
        let result = (|| {
            let r = self.release_context(scope)?;
            let state = r.releasing.as_ref().unwrap();
            if state.snapshot.stage != 2
                || state.snapshot.current.is_some()
                || state.snapshot.pending != 0
                || state.snapshot.frames != 0
                || !count_index::empty(r.verify()?, "release_jobs")?
                || !count_index::empty(r.verify()?, "release_frames")?
            {
                return Err(StorageError::Integrity("release seal exact EOF"));
            }
            let seal = state.seal();
            let mut a = ReleaseAttempt::new(state, "seal", 0)?;
            a.after.stage = 4;
            self.release_change(scope, a)?;
            Ok(seal)
        })();
        self.finish(result)
    }
    /// Retire only this exact known empty release seal; retain native S.
    /// Unknown, pending or different terminal counters cannot authorize retirement.
    pub fn release_retire(&mut self, seal: &ReleaseSeal) -> StorageResult<()> {
        self.release_context(&seal.scope)?;
        let result = (|| {
            let state = self
                .release_context(&seal.scope)?
                .releasing
                .as_ref()
                .unwrap();
            if state.seal() != *seal || !matches!(state.snapshot.stage, 4 | 5) {
                return Err(StorageError::Integrity("release retire exact seal"));
            }
            if state.snapshot.stage == 5 {
                return Ok(());
            }
            let mut a = ReleaseAttempt::new(state, "retire", 0)?;
            a.after.stage = 5;
            self.release_change(&seal.scope, a)
        })();
        self.finish(result)
    }
}
