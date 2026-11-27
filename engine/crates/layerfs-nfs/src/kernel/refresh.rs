//! Cache refresh after a Commit installed another root (the EdenFS method).
//!
//! An NFS client has no invalidation channel: it keeps attributes and
//! listings until `actimeo` or its close-to-open check. A SETATTR it sends
//! itself is answered with fresh post-operation attributes, and a client
//! that sees them changed drops what it cached for that inode. So the daemon
//! names the inodes an install made stale ([`Kernel::refresh`]) and, one path
//! at a time, reads the mode the client shows through the mount point, marks
//! that inode with exactly that mode ([`Kernel::arm_refresh`]) and changes
//! the mode to it. The export answers a mode-only SETATTR on a marked inode
//! whose requested mode is the marked one with its current attributes and
//! changes nothing (the mark is taken once and ends after [`MARK_LIFE`]), so
//! the refresh never writes into the Workspace and never reverts a mode the
//! install changed (the client may still show the mode from before it).
//!
//! The mark suppresses only the refresh's own chmod. A client SETATTR asking
//! another mode is applied as always, even on a marked inode, and leaves the
//! mark for the refresh's chmod still on its way, which is then answered
//! without a change: it never overwrites the user's newer mode. The one
//! request the export cannot tell from the refresh's is a client chmod to
//! the very mode just marked, sent between the mark and the refresh's own
//! chmod (microseconds apart, one inode at a time): that one is answered
//! unchanged and the refresh's chmod that follows applies the same mode.
use crate::kernel::{mutation::Sattr, Kernel};
use std::time::{Duration, Instant};

/// How long one refresh mark waits for its SETATTR.
pub const MARK_LIFE: Duration = Duration::from_secs(30);
/// Directory depth one path is built to at most.
const DEPTH: usize = 4096;
/// The permission bits a mark compares.
const PERMISSIONS: u32 = 0o7777;

impl Kernel {
    /// The paths below the export root (empty for the root itself) of each
    /// of `serials` this export handed out and still knows by name, with the
    /// serial, in the order given. A serial the client never saw has nothing
    /// cached and is left out. Marks nothing: [`Kernel::arm_refresh`] marks
    /// one inode right before its refresh chmod.
    pub fn refresh(&self, serials: impl IntoIterator<Item = u64>) -> Vec<(u64, Vec<Vec<u8>>)> {
        let root = self.root();
        let table = self.table();
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
                paths.push((serial, path));
            }
        }
        paths
    }
    /// Marks `serial` for the refresh chmod to `mode` about to be sent, and
    /// drops the marks whose life ended.
    pub fn arm_refresh(&self, serial: u64, mode: u32) {
        let now = Instant::now();
        let mut table = self.table();
        table
            .refreshing
            .retain(|_, (_, marked)| now.duration_since(*marked) < MARK_LIFE);
        table.refreshing.insert(serial, (mode & PERMISSIONS, now));
    }
    /// Whether a SETATTR of `serial` is a pending refresh: mode only, asking
    /// the marked mode, on an inode whose mark has not ended. Takes the mark;
    /// a SETATTR asking another mode leaves it.
    pub(crate) fn refreshed(&self, serial: u64, sattr: &Sattr) -> bool {
        let Some(mode) = sattr.mode else {
            return false;
        };
        let mode_only = sattr.uid.is_none()
            && sattr.gid.is_none()
            && sattr.size.is_none()
            && sattr.mtime.is_none();
        if !mode_only {
            return false;
        }
        let mut table = self.table();
        match table.refreshing.get(&serial).copied() {
            Some((_, at)) if at.elapsed() >= MARK_LIFE => {
                table.refreshing.remove(&serial);
                false
            }
            Some((marked, _)) if marked == mode & PERMISSIONS => {
                table.refreshing.remove(&serial);
                true
            }
            _ => false,
        }
    }
}
