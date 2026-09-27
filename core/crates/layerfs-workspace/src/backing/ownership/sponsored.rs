//! Bounded old-page sponsorship for copied extent pages.
use super::*;
use crate::backing::metadata_index;
const MAX_SPONSOR_DEPTH: u64 = 4;

impl Arena {
    /// The sponsor keeps matching edges alive; this page owns only its new
    /// edges and one reference to that authenticated old page. Both decoded
    /// edge lists are sorted, so duplicates retain their exact multiplicity.
    pub(crate) fn sponsored_edges(
        &self,
        r: PageRef,
        bytes: &[u8],
        sponsor: PageRef,
        sponsor_owner: Option<Owner>,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Vec<PageRef>, WorkspaceError> {
        let current = metadata_index::edges_raw(self.directory.incarnation, r, bytes)?;
        if sponsor == PageRef::NULL {
            return Ok(current);
        }
        let owner = match sponsor_owner {
            Some(owner) => owner,
            None => self.read_owner(sponsor, window, deadline)?,
        };
        let previous = self.load_raw_with_owner(sponsor, owner, window, deadline)?;
        if bytes[49] != metadata_pages::FORMAT_PIECES
            || previous[49] != metadata_pages::FORMAT_PIECES
            || bytes[48] != previous[48]
        {
            return Err(WorkspaceError::Io);
        }
        let old = metadata_index::edges_raw(self.directory.incarnation, sponsor, &previous)?;
        let mut charged = metadata_index::vector(current.len() + 1)?;
        let mut at = 0;
        for edge in current {
            let key = (edge.slot, edge.epoch);
            while at < old.len() && (old[at].slot, old[at].epoch) < key {
                at += 1;
            }
            if at < old.len() && (old[at].slot, old[at].epoch) == key {
                at += 1;
            } else {
                charged.push(edge);
            }
        }
        charged.push(sponsor);
        Ok(charged)
    }
}

impl RootOwner {
    /// Writes one page whose body an encoder fills for the allocated identity.
    /// Ownership, references and accounting are the same as `write_page`; the
    /// raw form exists because a piece page is not a keyed-cell page.
    pub(crate) fn write_raw_page<S>(
        &self,
        proposed_sponsor: PageRef,
        window: &mut Window,
        deadline: Instant,
        encode: impl FnOnce(PageRef, &mut [u8]) -> Result<S, WorkspaceError>,
    ) -> Result<(PageRef, S), WorkspaceError> {
        if self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .temporary
            .len()
            == 128
        {
            return Err(WorkspaceError::Capacity);
        }
        let r = self.arena.allocate_slot(self, window, deadline)?;
        let state = encode(r, &mut window.0[..PAGE])?;
        // A page's edges are declared by the page itself, and the window is the
        // only place the encoded bytes exist: the ledger write below reuses the
        // same window page, so the edges must be taken before it.
        let mut edges =
            metadata_index::edges_raw(self.arena.directory.incarnation, r, &window.0[..PAGE])?;
        let mut sponsor = PageRef::NULL;
        let mut depth = 0;
        if proposed_sponsor != PageRef::NULL && edges.len() >= 4 {
            let mut encoded = [0u8; PAGE];
            encoded.copy_from_slice(&window.0[..PAGE]);
            let old = self.arena.read_owner(proposed_sponsor, window, deadline)?;
            if old.role != 1 || old.refs == 0 || !old.edges {
                return Err(WorkspaceError::Io);
            }
            if old.next == PageRef::NULL && old.length != 0 {
                return Err(WorkspaceError::Io);
            }
            if old.length < MAX_SPONSOR_DEPTH && (old.next == PageRef::NULL || old.length != 0) {
                let reduced = self.arena.sponsored_edges(
                    r,
                    &encoded,
                    proposed_sponsor,
                    Some(old),
                    window,
                    deadline,
                )?;
                if reduced.len() < edges.len() {
                    edges = reduced;
                    sponsor = proposed_sponsor;
                    depth = old.length + 1;
                }
            }
            window.0[..PAGE].copy_from_slice(&encoded);
        }
        let leaf = window.0[48] == 0;
        let identity = self.create_file(&page_name(r), window, deadline)?;
        self.state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .edge_progress = Some((r, 0));
        self.arena.set_owner(
            r,
            Owner {
                epoch: r.epoch,
                role: 1,
                refs: 1,
                edges: true,
                next: sponsor,
                length: depth,
                device: identity.0,
                inode: identity.1,
                ..Owner::default()
            },
            window,
            deadline,
        )?;
        {
            let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            s.temporary.push(r);
            s.pending = None;
            s.slot_pending = None;
        }
        self.arena
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .pages += 1;
        self.add_page_edges(r, &edges, window, deadline)?;
        let charged = (edges.len() - usize::from(sponsor != PageRef::NULL)) as u64;
        let counter = if leaf {
            &self.arena.extent_custody_edges_added
        } else {
            &self.arena.extent_child_edges_added
        };
        let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            Some(old.saturating_add(charged))
        });
        if sponsor != PageRef::NULL {
            self.arena
                .extent_sponsor_edges_added
                .fetch_add(1, Ordering::Relaxed);
        }
        self.state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .edge_progress = None;
        let _ = self.arena.owner_finalizations.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |old| Some(old.saturating_add(1)),
        );
        Ok((r, state))
    }
}
