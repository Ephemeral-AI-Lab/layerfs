use super::state::Workspace;
use crate::filesystem::projection_counters::SizeHistogram;
use crate::{ReferenceScope, WorkspaceError, WorkspaceStatus};
use std::{
    fs,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

/// One projection binding. Dropping it never claims that kernel detach succeeded.
pub struct MountLease {
    pub(super) workspace: Workspace,
    pub(super) finished: bool,
}
impl Workspace {
    pub fn reserve_mount(&self) -> Result<MountLease, WorkspaceError> {
        {
            let state = self.state()?;
            self.available(&state)?;
            if state.mounted || state.active > 0 {
                return Err(WorkspaceError::Busy);
            }
        }
        let projection = super::coherence::ProjectionState::reserve(self)?;
        let mut state = self.state()?;
        self.available(&state)?;
        if state.mounted || state.active > 0 {
            return Err(WorkspaceError::Busy);
        }
        state.mounted = true;
        state.projection = Some(projection);
        Ok(MountLease {
            workspace: self.clone(),
            finished: false,
        })
    }
    pub fn status(&self) -> Result<WorkspaceStatus, WorkspaceError> {
        let state = self.state()?;
        let submission = state
            .submission
            .as_ref()
            .map(|submission| submission.status())
            .transpose()?;
        Ok(WorkspaceStatus {
            submission,
            generation: state.generation,
            revision: state.revision,
            dirty_inodes: state.dirty_inodes,
            mounted: state.mounted,
            stopping: self.inner.stopping.load(Ordering::Acquire),
            closed: state.closed,
            active_operations: state.active,
            nodes: state.nodes.len(),
            handles: state.handles.len(),
            projection_handles: state
                .handles
                .iter()
                .filter(|handle| handle.scope == ReferenceScope::Projection)
                .count(),
            projection_replies: state.projection.as_ref().map_or(0, |p| p.replies),
            projection_calls: crate::filesystem::projection_counters::ProjectionOp::ALL
                .iter()
                .map(|op| (op.label(), state.counters.count(*op)))
                .collect(),
            projection_bytes: crate::filesystem::projection_counters::PROJECTION_BYTE_LABELS
                .iter()
                .zip(state.counters.byte_totals())
                .map(|(label, total)| ((*label).to_string(), total))
                .collect(),
            projection_histogram: {
                let bytes = state.counters.bytes();
                [("read", bytes.read_sizes), ("write", bytes.write_sizes)]
                    .into_iter()
                    .flat_map(|(direction, sizes)| {
                        SizeHistogram::LABELS
                            .iter()
                            .zip(sizes.buckets())
                            .map(move |(label, count)| (format!("{direction}:{label}"), count))
                    })
                    .collect()
            },
            upstream_calls: state.counters.upstream(),
            coherence: state.projection.as_ref().map(|p| p.status),
            cookies: state.cookies.len(),
            accounted_bytes: self.host.budget.used(),
        })
    }
    pub fn close_clean(&self) -> Result<(), WorkspaceError> {
        self.close_clean_until(Instant::now() + Duration::from_secs(10))
    }
    pub fn close_clean_until(&self, deadline: Instant) -> Result<(), WorkspaceError> {
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let retired = {
            let mut state = self.state()?;
            if state.closed {
                return Err(WorkspaceError::Closed);
            }
            if state.submission.is_some()
                || state.dirty_inodes > 0
                || state.mounted
                || state.active > 0
                || !state.handles.is_empty()
                || state
                    .nodes
                    .iter()
                    .any(|node| node.lookups > 0 || node.projection_lookups > 0)
            {
                return Err(WorkspaceError::Busy);
            }
            if let Some(host) = &self.host.payloads {
                if host.has_external_pins(self.inner.incarnation)? {
                    return Err(WorkspaceError::Busy);
                }
            }
            self.inner.stopping.store(true, Ordering::Release);
            // The live overlay is this Workspace's own reference to arena roots,
            // exactly like the generations a Commit retires. Closing releases it
            // before the arena is reclaimed; nothing runs after `stopping` is set,
            // so a refused teardown has no later operation to mislead.
            state.overlay.take()
        };
        drop(retired);
        if let (Some(host), Some(arena)) = (&self.host.metadata, &self.inner.arena) {
            host.close(arena, deadline)?;
        }
        if let Some(host) = &self.host.payloads {
            host.reclaim(self.inner.incarnation, deadline)?;
            if host.remaining(self.inner.incarnation)? > 0 {
                return Err(WorkspaceError::Busy);
            }
        }
        if let Some(directory) = &self.inner.directory {
            crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
            directory.close().map_err(|error| {
                crate::backing::payload::bare_failure(crate::BackingPhase::Cleanup, error.kind())
            })?;
        }
        // A failed leaf removal keeps registry/count/table ownership for inspection.
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        fs::remove_dir(&self.inner.mount_path)?;
        let mut state = self.state()?;
        state.closed = true;
        state.nodes = Vec::new();
        state.handles = Vec::new();
        state.cookies = Vec::new();
        state.tables = None;
        drop(state);
        self.host
            .registry
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .entries
            .retain(|entry| entry.incarnation != self.inner.incarnation);
        Ok(())
    }
}
impl MountLease {
    pub fn stop_admission(&mut self) {
        self.workspace.inner.stopping.store(true, Ordering::Release);
    }
    /// Call only after the projection has detached and joined its callback loops.
    /// Open semantic handles remain owned; detached kernel lookup references end.
    pub fn finish(&mut self) -> Result<(), WorkspaceError> {
        if self.finished {
            return Err(WorkspaceError::Closed);
        }
        let mut state = self.workspace.state()?;
        if state.active > 0
            || state
                .projection
                .as_ref()
                .is_some_and(|p| p.replies > 0 || p.mutation_held || p.in_flight())
            || state
                .handles
                .iter()
                .any(|handle| handle.scope == ReferenceScope::Projection)
        {
            return Err(WorkspaceError::Busy);
        }
        state.mounted = false;
        let projection = state.projection.take();
        for node in &mut state.nodes {
            node.projection_lookups = 0;
        }
        state.collect(self.workspace.inner.root.serial);
        self.workspace
            .inner
            .stopping
            .store(false, Ordering::Release);
        self.finished = true;
        drop(state);
        // A checked detach retires even a failed binding; its applied mutation
        // stays in the live overlay and its returned failure retains the receipt.
        drop(projection);
        Ok(())
    }
}
