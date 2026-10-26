//! One kernel mutation transaction: its publication together with the kernel
//! lookup and open custody that the mutation's own reply hands to the kernel.
use crate::{
    db::integer, Changes, KernelApplied, KernelEffect, KernelMount, Overlay, OverlayError,
    OverlayResult,
};

/// Longest parent chain one cycle check reads: the canonical component limit
/// plus the root. A window of one job, not a namespace depth limit.
const ANCESTRY_STEPS: usize = 257;

impl Overlay {
    /// The kernel custody an applied reply hands over, inside the publishing
    /// transaction: the entry's lookup reference and a created file's
    /// descriptor, keyed by the request that receives them.
    pub(crate) fn kernel_effect(
        &self,
        mount: KernelMount,
        request: [u8; 8],
        changes: &Changes,
        effect: KernelEffect,
        publication: crate::Publication,
    ) -> OverlayResult<KernelApplied> {
        let (serial, parent, directory, open) = match effect {
            KernelEffect::None => {
                return Ok(KernelApplied {
                    publication,
                    file: None,
                })
            }
            KernelEffect::Entry {
                serial,
                parent,
                directory,
            } => (serial, parent, directory, None),
            KernelEffect::Open {
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
            return Err(OverlayError::Invalid("kernel entry final"));
        }
        // An inode this job created has no kernel reference yet: its row is
        // inserted without a read, and a duplicate is a definite failure.
        let created = changes.created == Some(serial);
        // A descriptor's row names the mount and the kernel request that
        // receives it.
        let (kernel, key) = (Some(mount.owner), i64::from_be_bytes(request));
        let file = match (created, open) {
            (true, Some(writable)) => {
                // One custody row write covers the lookup and the descriptor.
                self.insert_kernel_lookup_row(mount, serial, false, 1)?;
                let file = self.retain_file_row(kernel, key, serial, writable)?;
                self.file_ref(integer(serial)?, crate::ReferenceKind::Lookup, true)?;
                self.file_ref(integer(serial)?, crate::ReferenceKind::File, true)?;
                Some(file)
            }
            (true, None) => {
                self.insert_kernel_lookup(mount, serial, false, 1)?;
                None
            }
            (false, open) => {
                self.add_kernel_lookup(mount, serial)?;
                open.map(|writable| self.retain_file(kernel, key, serial, writable))
                    .transpose()?
            }
        };
        if directory {
            self.set_kernel_parent(mount, serial, parent)?;
        }
        Ok(KernelApplied { publication, file })
    }
    /// The retained parent chain of one kernel-known directory, from itself to
    /// the root. Every step is an indexed point read of a row this connection
    /// recorded at LOOKUP, MKDIR or a directory move; a missing step is Stale,
    /// never an assumed root. No name, reverse index or subtree is read.
    pub fn kernel_ancestors(&self, mount: KernelMount, serial: u64) -> OverlayResult<Vec<u64>> {
        self.check_kernel_attached(mount)?;
        let mut chain = vec![serial];
        let mut current = serial;
        while current != mount.root {
            if chain.len() == ANCESTRY_STEPS {
                return Err(OverlayError::Invalid("kernel ancestry depth"));
            }
            current = self.kernel_parent(mount, current)?;
            chain.push(current);
        }
        Ok(chain)
    }
}
