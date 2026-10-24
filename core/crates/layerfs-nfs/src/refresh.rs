//! Cache refresh after a Commit installed another root (the EdenFS method).
//!
//! An NFS client has no invalidation channel: it keeps attributes and
//! listings until `actimeo` or its close-to-open check. A SETATTR it sends
//! itself is answered with fresh post-operation attributes, and a client
//! that sees them changed drops what it cached for that inode. So the daemon
//! names the inodes an install made stale ([`Kernel::refresh`]) and changes
//! their mode to the value it already has through the mount point. The
//! export answers each such mode-only SETATTR on a marked inode with its
//! current attributes and changes nothing (the mark is taken once and ends
//! after [`MARK_LIFE`]), so the refresh never writes into the Workspace and
//! never reverts a mode the install changed.
use crate::{change::Sattr, kernel::Kernel};
use std::time::{Duration, Instant};

/// How long one refresh mark waits for its SETATTR.
pub const MARK_LIFE: Duration = Duration::from_secs(30);
/// Directory depth one path is built to at most.
const DEPTH: usize = 4096;

impl Kernel {
    /// Marks each of `serials` this export handed out and still knows by
    /// name, and returns their paths below the export root (empty for the
    /// root itself), in the order given. A serial the client never saw has
    /// nothing cached and is left out.
    pub fn refresh(&self, serials: impl IntoIterator<Item = u64>) -> Vec<Vec<Vec<u8>>> {
        let root = self.root();
        let mut table = self.table();
        let now = Instant::now();
        table
            .refreshing
            .retain(|_, marked| now.duration_since(*marked) < MARK_LIFE);
        let mut paths = Vec::new();
        for serial in serials {
            let mut path = Vec::new();
            let mut at = serial;
            let mut known = true;
            while at != root {
                let Some((parent, name)) = table.names.get(&at) else {
                    known = false;
                    break;
                };
                if path.len() >= DEPTH {
                    known = false;
                    break;
                }
                path.push(name.clone());
                at = *parent;
            }
            if known {
                path.reverse();
                table.refreshing.insert(serial, now);
                paths.push(path);
            }
        }
        paths
    }
    /// Whether a SETATTR of `serial` is a pending refresh: mode only, on a
    /// marked inode whose mark has not ended. Takes the mark.
    pub(crate) fn refreshed(&self, serial: u64, sattr: &Sattr) -> bool {
        let mode_only = sattr.mode.is_some()
            && sattr.uid.is_none()
            && sattr.gid.is_none()
            && sattr.size.is_none()
            && sattr.mtime.is_none();
        if !mode_only {
            return false;
        }
        let mut table = self.table();
        match table.refreshing.remove(&serial) {
            Some(marked) => marked.elapsed() < MARK_LIFE,
            None => false,
        }
    }
}
