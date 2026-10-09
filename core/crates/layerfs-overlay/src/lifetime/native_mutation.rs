//! One native mutation transaction: its publication together with the kernel
//! lookup and open custody that the mutation's own reply hands to the kernel.
use crate::{
    db::integer, Changes, NativeApplied, NativeEffect, NativeMount, Overlay, OverlayError,
    OverlayResult, StatementKind,
};

/// Longest parent chain one cycle check reads: the canonical component limit
/// plus the root. A window of one job, not a namespace depth limit.
const ANCESTRY_STEPS: usize = 257;

impl Overlay {
    /// The kernel custody an applied reply hands over, inside the publishing
    /// transaction: the entry's lookup reference and a created file's
    /// descriptor, keyed by the request that receives them.
    pub(crate) fn native_effect(
        &self,
        mount: NativeMount,
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
        // A descriptor's row names the mount and the kernel request that
        // receives it.
        let (native, key) = (Some(mount.owner), i64::from_be_bytes(request));
        let file = match (created, open) {
            (true, Some(writable)) => {
                // One custody row write covers the lookup and the descriptor.
                self.insert_native_lookup_row(mount, serial, false, 1)?;
                let file = self.retain_file_row(mount.route, native, key, serial, writable)?;
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
                open.map(|writable| self.retain_file(mount.route, native, key, serial, writable))
                    .transpose()?
            }
        };
        if directory {
            self.set_native_parent(mount, serial, parent)?;
        }
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
}
