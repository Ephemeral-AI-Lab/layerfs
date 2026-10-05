//! One authenticated ownership-ledger update for adjacent page references.
use super::{ledger_check, Arena, Owner, RootOwner, RECORDS};
use crate::{
    backing::{
        metadata_pages::{self, PageRef, HEADER, PAGE},
        payload::clock,
        segments::{self, Window},
    },
    BackingPhase, WorkspaceError,
};
use std::{sync::atomic::Ordering, time::Instant};

impl Arena {
    /// Advances a prefix that shares one ledger page. A reclaim stops at the
    /// first child reaching zero, so its existing depth-first cleanup stack
    /// still owns that child before the next edge is advanced.
    pub(crate) fn change_refs_run(
        &self,
        refs: &[PageRef],
        delta: i64,
        stop_on_zero: bool,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<(usize, Option<PageRef>), WorkspaceError> {
        let first = *refs.first().ok_or(WorkspaceError::Io)?;
        if first == PageRef::NULL {
            return Err(WorkspaceError::Io);
        }
        clock(deadline).map_err(|error| self.read_error(BackingPhase::Read, error))?;
        let index = (first.slot - 1) / RECORDS;
        let file = self.ledger_file(index, true, BackingPhase::Write)?;
        self.ledger_reads.fetch_add(1, Ordering::Relaxed);
        segments::read(&file, window, 0, PAGE)
            .map_err(|error| self.read_error(BackingPhase::Read, error))?;
        ledger_check(self, index, &window.0[..PAGE])?;
        let mut advanced = 0;
        let mut released = None;
        for &r in refs {
            if r == PageRef::NULL || (r.slot - 1) / RECORDS != index {
                break;
            }
            let at = HEADER + ((r.slot - 1) % RECORDS) as usize * 64;
            let mut owner = Owner::parse(&window.0[at..at + 64])?;
            if owner.epoch != r.epoch || owner.role == 0 {
                return Err(WorkspaceError::Io);
            }
            owner.refs = if delta >= 0 {
                owner.refs.checked_add(delta as u64)
            } else {
                owner.refs.checked_sub(delta.unsigned_abs())
            }
            .ok_or(WorkspaceError::Io)?;
            window.0[at..at + 64].copy_from_slice(&owner.bytes());
            advanced += 1;
            if stop_on_zero && owner.refs == 0 {
                released = Some(r);
                break;
            }
        }
        if advanced == 0 {
            return Err(WorkspaceError::Io);
        }
        metadata_pages::seal(&mut window.0[..PAGE]);
        self.ledger_writes.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = segments::write(&file, window, 0, PAGE) {
            self.host()?.quarantine(self, true);
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .unrecoverable = true;
            return Err(self.failure(BackingPhase::Write, error.kind()));
        }
        Ok((advanced, released))
    }
}

impl RootOwner {
    pub(super) fn add_page_edges(
        &self,
        page: PageRef,
        edges: &[PageRef],
        window: &mut Window,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        self.state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .edge_progress = Some((page, 0));
        let mut index = 0;
        while index < edges.len() {
            let (advanced, _) =
                self.arena
                    .change_refs_run(&edges[index..], 1, false, window, deadline)?;
            index += advanced;
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .edge_progress = Some((page, index));
        }
        Ok(())
    }
}
