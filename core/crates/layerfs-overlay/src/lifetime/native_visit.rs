//! One owner visit of a native request. The job is the request's whole
//! window in the owner, so it records no request source and leaves nothing
//! to release: install, revoke, close and reclamation are owner jobs too and
//! cannot run inside it. A visit that cannot decide changes nothing; its
//! request holds nothing until it visits again.
use crate::{
    db::integer, inode, BaseSource, Changes, InodeKind, LocalRead, NativeApplied, NativeDecision,
    NativeEffect, NativeMount, NativeObservation, OpenFile, Overlay, OverlayError, OverlayResult,
    SourceRows, StatementKind, WorkspaceState,
};

/// The kernel's own reference on the inode a native request names.
#[derive(Clone, Copy)]
pub(crate) enum Held {
    /// A lookup count on the inode.
    Lookup,
    /// An open regular-file descriptor of the inode.
    File(u64),
    /// An open file or directory descriptor of the inode.
    Handle(u64),
}
impl Overlay {
    /// The current base as this job's own source. It names no row and is
    /// accepted only while the Workspace still has this base and install
    /// frontier, which holds for the whole job that made it.
    fn visit_source(&self, mount: NativeMount, state: WorkspaceState) -> OverlayResult<BaseSource> {
        Ok(BaseSource {
            route: mount.route,
            owner: self.mint_owner()?,
            class: 3,
            root: state.base_root,
            installed: state.installed,
        })
    }
    /// The fence of one visit in one statement: the Workspace row, this
    /// mount attached and not revoked, and the kernel's own reference on the
    /// inode the request names. A missing or foreign Workspace, mount or
    /// reference is Stale; `live` refuses a closed Workspace before the mount
    /// is considered. The flag is the access mode of a held file descriptor.
    pub(crate) fn native_fence(
        &self,
        mount: NativeMount,
        serial: u64,
        held: Held,
        live: bool,
    ) -> OverlayResult<(WorkspaceState, bool)> {
        self.check_route(mount.route)?;
        let (owner, key) = (integer(mount.owner)?, integer(serial)?);
        let incarnation = mount.route.incarnation.as_slice();
        let decode = |r: &rusqlite::Row<'_>| {
            Ok((
                crate::lifetime::workspace::decode(r)?,
                r.get::<_, Option<i64>>(11)?,
                r.get::<_, Option<i64>>(12)?,
                r.get::<_, Option<bool>>(13)?,
                r.get::<_, Option<bool>>(14)?,
            ))
        };
        let row = match held {
            Held::Lookup => self.query(
                StatementKind::Workspace,
                crate::sql::FENCE_LOOKUP,
                &[&mount.route.ns, &incarnation, &owner, &key],
                56,
                decode,
            ),
            Held::File(handle) | Held::Handle(handle) => self.query(
                StatementKind::Workspace,
                if matches!(held, Held::File(_)) {
                    crate::sql::FENCE_FILE
                } else {
                    crate::sql::FENCE_HANDLE
                },
                &[
                    &mount.route.ns,
                    &incarnation,
                    &owner,
                    &key,
                    &integer(handle)?,
                ],
                64,
                decode,
            ),
        }?
        .pop();
        let (state, attached, root, revoked, held) = row.ok_or(OverlayError::Stale)?;
        if live && state.closed {
            return Err(OverlayError::Closed);
        }
        if attached != Some(owner) || root != Some(integer(mount.root)?) || revoked != Some(false) {
            return Err(OverlayError::Stale);
        }
        Ok((state, held.ok_or(OverlayError::Stale)?))
    }
    /// LOOKUP or GETATTR in one visit. The callback is one bounded semantic
    /// decision over current rows and base facts it holds or reads from
    /// memory; it performs no provider I/O. A decision that still needs a
    /// base fact changes nothing here. A positive LOOKUP takes its kernel
    /// lookup reference in the same transaction as the answer.
    pub fn observe_native_visit<T>(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: Option<u64>,
        lookup: bool,
        decide: impl FnOnce(SourceRows<'_>, BaseSource) -> OverlayResult<NativeDecision<T>>,
    ) -> NativeObservation<T> {
        let mut decision = None;
        let result = self
            .atomic(|| {
                let held = handle.map_or(Held::Lookup, Held::Handle);
                let (state, _) = self.native_fence(mount, serial, held, true)?;
                let source = self.visit_source(mount, state)?;
                let inode = match decide(self.source_rows_at(source, state), source)? {
                    NativeDecision::Needs(value) => {
                        decision = Some(value);
                        return Ok(());
                    }
                    NativeDecision::Finished { inode, value } => {
                        decision = Some(value);
                        inode
                    }
                };
                let Some(inode) = inode else { return Ok(()) };
                inode::check(&inode)?;
                if lookup {
                    if inode.nlink == 0 && inode.serial != mount.root {
                        return Err(OverlayError::Missing);
                    }
                    self.add_native_lookup(mount, inode.serial)?;
                    if inode.kind == InodeKind::Directory {
                        self.set_native_parent(mount, inode.serial, serial)?;
                    }
                } else if inode.serial != serial {
                    return Err(OverlayError::Invalid("native observation serial"));
                }
                Ok(())
            })
            .map(|()| None);
        NativeObservation {
            decision,
            result,
            candidate: None,
            open_candidate: None,
            directory_candidate: None,
        }
    }
    /// The local part of one READ window, through its descriptor, or of one
    /// READLINK window, under the kernel's lookup reference, and the base
    /// root its inherited bytes belong to. The job only reads: it records no
    /// request, source or reader, so its request has nothing to release and
    /// holds back no install, revoke or close. An unlinked file is read
    /// through its descriptor from the layers and root its orphan retains.
    pub fn read_native_visit(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: Option<u64>,
        offset: u64,
        length: u32,
    ) -> OverlayResult<([u8; 32], Option<LocalRead>)> {
        let held = handle.map_or(Held::Lookup, Held::Handle);
        let (state, _) = self.native_fence(mount, serial, held, true)?;
        let (ns, key) = (mount.route.ns, integer(serial)?);
        // No orphan row exists before this engine created its first one.
        if self.orphan_seen.get() {
            if let Some(orphan) = self.orphan(ns, key)? {
                // Without a descriptor a removed regular file is gone.
                if handle.is_none() && self.orphan_layer(ns, key)?.kind == InodeKind::File {
                    return Err(OverlayError::Missing);
                }
                let local = self.orphan_read(ns, key, orphan, offset, length)?;
                return Ok((orphan.root, local));
            }
        }
        let local =
            self.read_layers(ns, serial, state.active.0, state.installed, offset, length)?;
        Ok((state.base_root, local))
    }
    /// One native mutation in one visit. The callback decides over current
    /// rows of the Workspace row the fence read; when it returns changes
    /// they are published with the kernel custody their reply hands over, in
    /// one transaction, against that same row. When it returns none, nothing
    /// is written. A handle-addressed mutation is given its exact open
    /// descriptor, which the fence read: it is not read again.
    pub fn mutate_native_visit(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: Option<u64>,
        decide: impl FnOnce(
            SourceRows<'_>,
            Option<OpenFile>,
        ) -> OverlayResult<Option<(Changes, NativeEffect)>>,
    ) -> OverlayResult<Option<NativeApplied>> {
        self.atomic(|| {
            let held = handle.map_or(Held::Lookup, Held::File);
            let (state, writable) = self.native_fence(mount, serial, held, true)?;
            let file = handle.map(|owner| OpenFile {
                route: mount.route,
                owner,
                serial,
                writable,
            });
            let source = self.visit_source(mount, state)?;
            let rows = self.source_rows_at(source, state);
            let Some((changes, effect)) = decide(rows, file)? else {
                return Ok(None);
            };
            let checked = self.check_changes(&changes)?;
            let publication = self.apply_checked(source, state, file, &changes, &checked)?;
            self.native_effect(
                mount,
                source,
                request.to_be_bytes(),
                &changes,
                effect,
                publication,
            )
            .map(Some)
        })
    }
    /// Plans of the statements a visit and its custody use: the three
    /// fences, the file-reference decrements that return what remains, and
    /// the VM program of the created-and-opened file's one custody write.
    pub fn explain_native_visit(&self, mount: NativeMount) -> OverlayResult<Vec<String>> {
        self.state(mount.route)?;
        let (ns, owner, one) = (mount.route.ns, integer(mount.owner)?, 1_i64);
        let incarnation = mount.route.incarnation.as_slice();
        let fence: [&dyn rusqlite::ToSql; 5] = [&ns, &incarnation, &owner, &one, &one];
        let custody: [&dyn rusqlite::ToSql; 2] = [&ns, &one];
        let mut plans = Vec::new();
        for (label, statement, params) in [
            ("fence-lookup", crate::sql::FENCE_LOOKUP, &fence[..4]),
            ("fence-file", crate::sql::FENCE_FILE, &fence[..]),
            ("fence-handle", crate::sql::FENCE_HANDLE, &fence[..]),
            ("drop-open", crate::sql::FILE_OPENS_DROP, &custody[..]),
            ("drop-lookup", crate::sql::FILE_LOOKUPS_DROP, &custody[..]),
            ("drop-reader", crate::sql::FILE_READERS_DROP, &custody[..]),
        ] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN QUERY PLAN {statement}"),
                params,
                64,
                |row| Ok(format!("{label}: {}", row.get::<_, String>(3)?)),
            )?);
        }
        plans.push(self.explain_program(
            "open-and-lookup",
            crate::sql::FILE_OPEN_LOOKUP_ADD,
            &custody,
        )?);
        Ok(plans)
    }
}
