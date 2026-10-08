//! One native mutation transaction: its publication together with the kernel
//! lookup and open custody that the mutation's own reply hands to the kernel.
use crate::{
    db::integer, BaseSource, Changes, NativeApplied, NativeEffect, NativeMount, OpenFile, Overlay,
    OverlayError, OverlayResult, StatementKind,
};

/// Longest parent chain one cycle check reads: the canonical component limit
/// plus the root. A window of one job, not a namespace depth limit.
const ANCESTRY_STEPS: usize = 257;

impl Overlay {
    /// Publishes one checked compound job through an exact undecided native
    /// request source. The source is marked decided, so a second publication
    /// through it is refused. An entry-bearing success acquires its kernel
    /// lookup reference, and a created-and-opened file its descriptor, in this
    /// same transaction: the reply attempt follows known custody and no
    /// compensating release is ever guessed after an unobservable send.
    pub fn apply_native(
        &self,
        mount: NativeMount,
        source: BaseSource,
        changes: &Changes,
        effect: NativeEffect,
    ) -> OverlayResult<NativeApplied> {
        let checked = self.check_changes(changes)?;
        self.atomic(|| {
            let (request, _) = self.check_native_source(mount, source)?;
            let publication = self.apply_checked(source, changes, &checked)?;
            self.execute(
                StatementKind::Lease,
                "UPDATE native_source SET decided=1 WHERE ns=?1 AND owner=?2",
                &[&mount.route.ns, &integer(source.owner)?],
                16,
            )?;
            let (serial, parent, directory, open) = match effect {
                NativeEffect::None => {
                    return Ok(NativeApplied {
                        publication,
                        file: None,
                    })
                }
                NativeEffect::Entry {
                    serial,
                    parent,
                    directory,
                } => (serial, parent, directory, None),
                NativeEffect::Open {
                    serial,
                    parent,
                    writable,
                } => (serial, parent, false, Some(writable)),
            };
            if !changes.inodes.iter().any(|inode| {
                inode.serial == serial
                    && inode.nlink != 0
                    && (inode.kind == crate::InodeKind::Directory) == directory
            }) {
                return Err(OverlayError::Invalid("native entry final"));
            }
            self.add_native_lookup(mount, serial)?;
            if directory {
                self.set_native_parent(mount, serial, parent)?;
            }
            let file = match open {
                None => None,
                Some(writable) => {
                    let file =
                        self.retain_file(mount.route, -integer(source.owner)?, serial, writable)?;
                    self.execute(
                        StatementKind::Lease,
                        "INSERT INTO native_file VALUES(?1,?2,?3,?4)",
                        &[
                            &mount.route.ns,
                            &integer(mount.owner)?,
                            &request.as_slice(),
                            &integer(file.owner)?,
                        ],
                        32,
                    )?;
                    Some(file)
                }
            };
            Ok(NativeApplied { publication, file })
        })
    }
    /// The retained parent chain of one kernel-known directory, from itself to
    /// the root. Every step is an indexed point read of a row this connection
    /// recorded at LOOKUP, MKDIR or a directory move; a missing step is Stale,
    /// never an assumed root. No name, reverse index or subtree is read.
    pub fn native_ancestors(&self, mount: NativeMount, serial: u64) -> OverlayResult<Vec<u64>> {
        self.check_native_attached(mount)?;
        let mut chain = vec![serial];
        let mut current = serial;
        while current != mount.root {
            if chain.len() == ANCESTRY_STEPS {
                return Err(OverlayError::Invalid("native ancestry depth"));
            }
            current = self.native_parent(mount, current)?;
            chain.push(current);
        }
        Ok(chain)
    }
    /// The exact descriptor and an independent request source in one job, for
    /// a mutation addressed through a handle. The source survives RELEASE.
    pub fn acquire_native_open_source(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: u64,
    ) -> OverlayResult<(BaseSource, OpenFile)> {
        self.atomic(|| {
            self.check_native_mount(mount)?;
            let file = self.native_file(mount, serial, handle)?;
            Ok((self.retain_native_source(mount, request, serial)?, file))
        })
    }
}
