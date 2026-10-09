//! One owner visit of a native request. The job is the request's whole
//! window in the owner, so it records no request source and leaves nothing
//! to release: install, revoke, close and reclamation are owner jobs too and
//! cannot run inside it. A visit that cannot decide changes nothing; its
//! request holds nothing until it visits again.
use crate::{
    db::integer, inode, BaseSource, Changes, InodeKind, NativeApplied, NativeDecision,
    NativeEffect, NativeMount, NativeObservation, OpenFile, Overlay, OverlayError, OverlayResult,
    SourceRows, StatementKind, WorkspaceState,
};

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
    /// The kernel's own reference on what the request names: a lookup count
    /// on the inode, or an open file or directory descriptor on it.
    fn visit_reference(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: Option<u64>,
    ) -> OverlayResult<()> {
        let held = match handle {
            None => self.native_lookup_row(mount, serial)?.is_some(),
            Some(handle) => {
                self.query(
                    StatementKind::Lease,
                    crate::sql::NATIVE_HANDLE_HELD,
                    &[
                        &mount.route.ns,
                        &integer(mount.owner)?,
                        &integer(handle)?,
                        &integer(serial)?,
                    ],
                    32,
                    |_| Ok(()),
                )?
                .len()
                    == 1
            }
        };
        if held {
            Ok(())
        } else {
            Err(OverlayError::Stale)
        }
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
                let state = self.check_native_mount(mount)?;
                self.visit_reference(mount, serial, handle)?;
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
    /// One native mutation in one visit. The callback decides over current
    /// rows; when it returns changes they are published with the kernel
    /// custody their reply hands over, in one transaction. When it returns
    /// none, nothing is written. A handle-addressed mutation is given its
    /// exact open descriptor.
    pub fn mutate_native_visit(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: Option<u64>,
        decide: impl FnOnce(
            BaseSource,
            Option<OpenFile>,
        ) -> OverlayResult<Option<(Changes, NativeEffect)>>,
    ) -> OverlayResult<Option<NativeApplied>> {
        self.atomic(|| {
            let state = self.check_native_mount(mount)?;
            let file = match handle {
                Some(handle) => Some(self.native_file_row(mount, serial, handle)?),
                None => {
                    self.visit_reference(mount, serial, None)?;
                    None
                }
            };
            let source = self.visit_source(mount, state)?;
            let Some((changes, effect)) = decide(source, file)? else {
                return Ok(None);
            };
            let checked = self.check_changes(&changes)?;
            let publication = self.apply_checked(source, &changes, &checked)?;
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
}
