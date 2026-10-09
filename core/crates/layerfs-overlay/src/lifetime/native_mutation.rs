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
            let (request, _, state) = self.check_native_source(mount, source)?;
            let publication = self.apply_checked(source, state, None, changes, &checked)?;
            self.execute(
                StatementKind::Lease,
                "UPDATE native_source SET decided=1 WHERE ns=?1 AND owner=?2",
                &[&mount.route.ns, &integer(source.owner)?],
                16,
            )?;
            self.native_effect(mount, source, request, changes, effect, publication)
        })
    }
    /// The kernel custody an applied reply hands over, inside the publishing
    /// transaction: the entry's lookup reference and a created file's
    /// descriptor, keyed by the request that receives them.
    pub(crate) fn native_effect(
        &self,
        mount: NativeMount,
        source: BaseSource,
        request: [u8; 8],
        changes: &Changes,
        effect: NativeEffect,
        publication: crate::Publication,
    ) -> OverlayResult<NativeApplied> {
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
        // An inode this job created has no kernel reference yet: its row is
        // inserted without a read, and a duplicate is a definite failure.
        let created = changes.created == Some(serial);
        let key = -integer(source.owner)?;
        let opened = match (created, open) {
            (true, Some(writable)) => {
                // One custody row write covers the lookup and the descriptor.
                self.insert_native_lookup_row(mount, serial, false, 1)?;
                let file = self.retain_file_row(mount.route, key, serial, writable)?;
                self.execute(
                    StatementKind::Lease,
                    crate::sql::FILE_OPEN_LOOKUP_ADD,
                    &[&mount.route.ns, &integer(serial)?],
                    16,
                )?;
                Some(file)
            }
            (true, None) => {
                self.insert_native_lookup(mount, serial, false, 1)?;
                None
            }
            (false, open) => {
                self.add_native_lookup(mount, serial)?;
                open.map(|writable| self.retain_file(mount.route, key, serial, writable))
                    .transpose()?
            }
        };
        if directory {
            self.set_native_parent(mount, serial, parent)?;
        }
        let file = match opened {
            None => None,
            Some(file) => {
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
            let state = self.check_native_mount(mount)?;
            let file = self.native_file(mount, serial, handle)?;
            Ok((
                self.retain_native_source(mount, state, request, serial)?,
                file,
            ))
        })
    }
}
