//! Optional bounded work against an idle published comparison context.
use super::{Attempt, CommitCoordinator, PublishedContext};
use crate::correspondence::Description;
use crate::host_overlay::HostOverlay;
use crate::overlay_budget::{Charge, Operation};
use layerfs_layerstack_store::{Result, StoreError};
use layerfs_workspace_core::NodeId;
use std::path::Path;
use std::sync::{Arc, MutexGuard, TryLockError};

// #144 R3a: one maintenance step processes a bounded batch of pending nodes
// through a single `prepare`/`install` pair. The drain stays O(D) — batching
// changes the per-node constant, not the bound — and the batch is small enough
// that a step remains an interruptible, bounded unit of work.
const NODES_PER_STEP: usize = 64;
const COVERED_KEYS_PER_STEP: usize = 64;

impl CommitCoordinator {
    fn idle_maintenance_attempt(&self) -> Result<Option<MutexGuard<'_, Option<Attempt>>>> {
        let slot = match self.attempt.try_lock() {
            Ok(slot) => slot,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => {
                return Err(StoreError::Integrity("Commit maintenance attempt lock"))
            }
        };
        Ok(slot.is_none().then_some(slot))
    }

    /// True means some work remains or an active/retained Commit requires a
    /// later attempt. False means this bounded observation found no more work.
    /// Commit and ordinary operations never wait for a maintenance-wide FS cut.
    pub(crate) fn maintenance_step(&self, host: &HostOverlay, spool: &Path) -> Result<bool> {
        self.maintenance_before_context_install(host, spool, || {})
    }

    fn maintenance_before_context_install(
        &self,
        host: &HostOverlay,
        spool: &Path,
        before_context_install: impl FnOnce(),
    ) -> Result<bool> {
        if host.origins().workspace_id() != self.workspace {
            return Err(StoreError::InvalidInput("foreign maintenance Workspace"));
        }
        let context = {
            let Some(_idle) = self.idle_maintenance_attempt()? else {
                return Ok(true);
            };
            self.published()?
        };
        // Snapshot::changes owns at most128 encoded keys plus decoded names;
        // this allowance also covers the pending batch, its decoded
        // descriptions and the context bookkeeping.
        let _work = host.budget.enter(
            Operation::Scratch,
            Charge {
                memory_bytes: 128 * 1024
                    + NODES_PER_STEP as u64 * crate::host_canonicalize::BATCH_NODE_BYTES,
                ..Charge::default()
            },
        )?;
        // No attempt/published lock is held during index, canonical or payload
        // I/O. A new Commit may start here; its owned snapshot stays unchanged.
        let mut replacement: Option<PublishedContext> = None;
        let mut retry = false;
        let mut complete: Vec<NodeId> = Vec::new();
        let mut batch: Vec<(NodeId, Description)> = Vec::new();
        for node in context
            .correspondence
            .maintenance_nodes(None)?
            .into_iter()
            .take(NODES_PER_STEP)
        {
            match context.correspondence.description(node)? {
                Some(description) => batch.push((node, description)),
                // No published descriptor: this node needs no installation for
                // this comparison and its pending entry is complete.
                None => complete.push(node),
            }
        }
        let requests: Vec<_> = batch
            .iter()
            .map(|(node, description)| (*node, description))
            .collect();
        let done = host.canonicalize_batch(&requests, spool)?;
        for ((node, _), done) in batch.iter().zip(done) {
            if done {
                complete.push(*node);
            } else {
                retry = true;
            }
        }
        if !complete.is_empty() {
            let next = replacement.get_or_insert_with(|| (*context).clone());
            next.correspondence.maintenance_complete_batch(&complete)?;
        }
        let covered_sequence = context.correspondence.covered_sequence;
        let covered = {
            let snapshot = host.snapshot()?;
            snapshot
                .changes(0, None)?
                .into_iter()
                .take_while(|(_, sequence, _)| *sequence <= covered_sequence)
                .take(COVERED_KEYS_PER_STEP)
                .collect::<Vec<_>>()
        };
        if !covered.is_empty() {
            host.mutate(|m| {
                for (_, sequence, key) in &covered {
                    // Latest versions that changed after capture are preserved.
                    m.forget_covered(key, *sequence)?;
                }
                Ok(())
            })?;
        }
        before_context_install();
        let (published, previous) = {
            // Updating only bookkeeping still changes the context Arc. Never
            // do that while a Commit holds it as its expected publication owner.
            let Some(_idle) = self.idle_maintenance_attempt()? else {
                return Ok(true);
            };
            let mut published = self
                .published
                .lock()
                .map_err(|_| StoreError::Integrity("Commit maintenance context lock"))?;
            if !Arc::ptr_eq(&published, &context) {
                return Ok(true);
            }
            let previous =
                replacement.map(|next| std::mem::replace(&mut *published, Arc::new(next)));
            (Arc::clone(&published), previous)
        };
        drop(previous);
        drop(context);
        let mut remaining = retry || !published.correspondence.maintenance_nodes(None)?.is_empty();
        let covered_sequence = published.correspondence.covered_sequence;
        drop(published);
        remaining |= {
            let snapshot = host.snapshot()?;
            snapshot
                .changes(0, None)?
                .first()
                .is_some_and(|(_, sequence, _)| *sequence <= covered_sequence)
        };
        // Release temporary roots before observing the existing reclaimer. One
        // host step also services its disk-queued deleted-directory cleanup.
        remaining |= host.maintain()?;
        Ok(remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::Fixture;
    use super::*;
    use crate::snapshot::Snapshot;
    use layerfs_workspace_core::ROOT;
    use std::ops::Deref;

    struct OwnedFixture(Option<Fixture>);
    impl OwnedFixture {
        fn new() -> Self {
            Self(Some(Fixture::new()))
        }
    }
    impl Deref for OwnedFixture {
        type Target = Fixture;
        fn deref(&self) -> &Fixture {
            self.0.as_ref().unwrap()
        }
    }
    impl Drop for OwnedFixture {
        fn drop(&mut self) {
            let fixture = self.0.take().unwrap();
            let directory = fixture.directory.clone();
            drop(fixture);
            let cleanup = std::fs::remove_dir_all(directory);
            if std::thread::panicking() {
                if let Err(error) = cleanup {
                    eprintln!("maintenance fixture cleanup: {error}");
                }
            } else {
                cleanup.unwrap();
            }
        }
    }
    fn change_count(snapshot: &Snapshot) -> usize {
        let mut after = None;
        let mut count = 0;
        loop {
            let page = snapshot.changes(0, after.as_deref()).unwrap();
            if page.is_empty() {
                return count;
            }
            count += page.len();
            after = page.last().map(|(key, _, _)| key.clone());
        }
    }
    fn pending_count(context: &PublishedContext) -> usize {
        let mut after = None;
        let mut count = 0;
        loop {
            let page = context.correspondence.maintenance_nodes(after).unwrap();
            if page.is_empty() {
                return count;
            }
            count += page.len();
            after = page.last().copied();
        }
    }

    #[test]
    fn maintenance_caps_nodes_and_covered_keys_without_bumping_live_sequence() {
        let fixture = OwnedFixture::new();
        for n in 0..130 {
            fixture
                .host
                .create_file(ROOT, format!("new{n:03}").as_bytes(), 0o600)
                .unwrap();
        }
        fixture.commit().unwrap();
        let context = fixture.coordinator.published().unwrap();
        let before = fixture.host.snapshot().unwrap();
        let keys = change_count(&before);
        let nodes = pending_count(&context);
        assert!(keys > COVERED_KEYS_PER_STEP);
        assert!(nodes > NODES_PER_STEP);
        assert!(fixture
            .coordinator
            .maintenance_step(&fixture.host, &fixture.directory)
            .unwrap());
        let current = fixture.host.snapshot().unwrap();
        assert_eq!(current.root.sequence, before.root.sequence);
        assert_eq!(keys - change_count(&current), COVERED_KEYS_PER_STEP);
        assert_eq!(
            nodes - pending_count(&fixture.coordinator.published().unwrap()),
            NODES_PER_STEP
        );
        assert_eq!(
            change_count(&before),
            keys,
            "retained snapshot bookkeeping stays immutable"
        );
    }

    /// #144 R3a: one step drains a bounded batch of pending nodes through a
    /// single root preparation, and the pending queue drops by the whole batch.
    #[test]
    fn one_step_installs_a_bounded_batch_through_a_single_root_preparation() {
        let fixture = OwnedFixture::new();
        for n in 0..130 {
            fixture
                .host
                .create_file(ROOT, format!("new{n:03}").as_bytes(), 0o600)
                .unwrap();
        }
        fixture.commit().unwrap();
        let before = pending_count(&fixture.coordinator.published().unwrap());
        assert!(
            before > NODES_PER_STEP,
            "the batch bound must actually bind"
        );
        let preparations = fixture.host.preparation_attempts();
        assert!(fixture
            .coordinator
            .maintenance_step(&fixture.host, &fixture.directory)
            .unwrap());
        let attempts = fixture.host.preparation_attempts() - preparations;
        let after = pending_count(&fixture.coordinator.published().unwrap());
        assert_eq!(before - after, NODES_PER_STEP);
        assert!(
            attempts <= 4,
            "one bounded batch installs through one preparation per batch, not one per node: {attempts} preparations for {} nodes",
            before - after
        );
    }

    #[test]
    fn concurrent_c2_publication_keeps_new_pending_context_and_later_write() {
        let fixture = OwnedFixture::new();
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        fixture.host.write(node, 0, b"X").unwrap();
        fixture.commit().unwrap();
        let c1 = fixture.coordinator.published().unwrap();
        fixture.host.write(node, 1, b"Y").unwrap();
        let later_sequence = fixture.host.snapshot().unwrap().root.sequence;
        assert!(fixture
            .coordinator
            .maintenance_before_context_install(&fixture.host, &fixture.directory, || {
                fixture.commit().unwrap();
            })
            .unwrap());
        let c2 = fixture.coordinator.published().unwrap();
        assert!(!Arc::ptr_eq(&c1, &c2));
        assert_eq!(c2.correspondence.covered_sequence, later_sequence);
        assert!(
            c2.correspondence
                .maintenance_nodes(None)
                .unwrap()
                .contains(&node),
            "stale C1 maintenance must not overwrite the C2 pending queue"
        );
        let mut bytes = [0; 2];
        fixture.host.read_into(node, 0, &mut bytes).unwrap();
        assert_eq!(&bytes, b"XY");
        drop((c1, c2));
        let mut steps = 0;
        while fixture
            .coordinator
            .maintenance_step(&fixture.host, &fixture.directory)
            .unwrap()
        {
            steps += 1;
            assert!(steps < 10_000);
        }
        let current = fixture.host.snapshot().unwrap();
        assert_eq!(current.root.sequence, later_sequence);
        assert_eq!(change_count(&current), 0);
        assert_eq!(
            current
                .file_ranges(&fixture.host.ranges, node)
                .unwrap()
                .payload_bytes(),
            0
        );
        fixture.host.read_into(node, 0, &mut bytes).unwrap();
        assert_eq!(&bytes, b"XY");
    }

    #[test]
    fn active_or_retained_attempt_is_skipped_and_foreign_host_is_rejected() {
        let fixture = OwnedFixture::new();
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        fixture.host.write(node, 0, b"X").unwrap();
        let before = fixture.host.snapshot().unwrap();
        let held = fixture.coordinator.attempt.lock().unwrap();
        assert!(fixture
            .coordinator
            .maintenance_step(&fixture.host, &fixture.directory)
            .unwrap());
        assert!(Arc::ptr_eq(
            &before.root,
            &fixture.host.snapshot().unwrap().root
        ));
        drop(held);
        assert!(fixture
            .coordinator
            .commit(
                || fixture.host.snapshot(),
                |_, _| {
                    Err(StoreError::InvalidInput(
                        "injected pre-stage construction failure",
                    ))
                }
            )
            .is_err());
        assert!(fixture.coordinator.attempt.lock().unwrap().is_none());
        layerfs_layerstack_store::set_transaction_failure_at(Some(u64::MAX - 2));
        let failed = fixture.commit();
        layerfs_layerstack_store::set_transaction_failure_at(None);
        assert!(failed.is_err());
        assert!(
            fixture
                .coordinator
                .store
                .workspace_stage(fixture.coordinator.workspace)
                .unwrap()
                .is_some(),
            "the injected fault must retain an actual canonical stage"
        );
        assert!(fixture.coordinator.attempt.lock().unwrap().is_some());
        assert!(fixture
            .coordinator
            .maintenance_step(&fixture.host, &fixture.directory)
            .unwrap());
        assert!(Arc::ptr_eq(
            &before.root,
            &fixture.host.snapshot().unwrap().root
        ));
        fixture.coordinator.abandon().unwrap();
        let foreign = OwnedFixture::new();
        let other = foreign.host.snapshot().unwrap();
        assert!(fixture
            .coordinator
            .maintenance_step(&foreign.host, &foreign.directory)
            .is_err());
        assert!(Arc::ptr_eq(
            &other.root,
            &foreign.host.snapshot().unwrap().root
        ));
    }
}
