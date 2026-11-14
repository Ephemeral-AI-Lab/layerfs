//! Prototype (FUSE passthrough of decoded cached files, issue #684 experiment
//! `passthrough`; not a product contract): a read-only OPEN of an unchanged
//! base regular file whose whole decoded content is in the decoded-file cache
//! is answered with that plain file as its kernel backing file, so the kernel
//! reads it without a READ request. Off unless `LAYERFS_PROBE_PASSTHROUGH=1`.
//!
//! The kernel holds one backing file per inode and refuses (EIO to the
//! opener) a passthrough open of an inode open in cached mode, and a cached
//! open of an inode open in passthrough mode. So this mount counts its own
//! opens per inode: passthrough only while no other open of the inode is
//! live, one backing id per inode reused by every passthrough open and closed
//! at the last one's RELEASE.
use fuser::{BackingId, FileHandle, FopenFlags, ReplyOpen};
use layerfs_mount_ports::decoded::{count, Counter};
use std::{collections::HashMap, fs::File, sync::Mutex};

#[derive(Default)]
pub struct Passthrough {
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    inodes: HashMap<u64, Inode>,
    /// Open file handle -> (serial, answered with passthrough).
    handles: HashMap<u64, (u64, bool)>,
}
#[derive(Default)]
struct Inode {
    backing: Option<BackingId>,
    passthrough: usize,
    other: usize,
}
impl Passthrough {
    /// Answers one regular-file OPEN. `cached` is the complete decoded file
    /// when the open is read-only and the file unchanged.
    pub fn open(
        &self,
        reply: ReplyOpen,
        serial: u64,
        handle: u64,
        flags: FopenFlags,
        cached: Option<&File>,
    ) {
        let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let state = &mut *guard;
        let inode = state.inodes.entry(serial).or_default();
        if let Some(file) = cached.filter(|_| inode.other == 0) {
            if inode.backing.is_none() {
                match reply.open_backing(file) {
                    Ok(id) => inode.backing = Some(id),
                    Err(error) => eprintln!("PROBE_BACKING_OPEN failed: {error}"),
                }
            }
            if let Some(id) = &inode.backing {
                inode.passthrough += 1;
                state.handles.insert(handle, (serial, true));
                count(Counter::PassthroughOpens, 1);
                // The kernel refuses (EIO) a passthrough open with any flag
                // outside FOPEN_PASSTHROUGH_MASK, which excludes KEEP_CACHE:
                // the FUSE inode's page cache is not used for its bytes.
                let flags = flags - FopenFlags::FOPEN_KEEP_CACHE;
                reply.opened_passthrough(FileHandle(handle), flags, id);
                return;
            }
        }
        inode.other += 1;
        state.handles.insert(handle, (serial, false));
        reply.opened(FileHandle(handle), flags);
    }
    /// The RELEASE of one handle this mount answered.
    pub fn release(&self, handle: u64) {
        let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let state = &mut *guard;
        let Some((serial, passthrough)) = state.handles.remove(&handle) else {
            return;
        };
        let Some(inode) = state.inodes.get_mut(&serial) else {
            return;
        };
        if passthrough {
            inode.passthrough -= 1;
            if inode.passthrough == 0 {
                inode.backing = None; // Closes the backing id.
            }
        } else {
            inode.other -= 1;
        }
        if inode.passthrough == 0 && inode.other == 0 {
            state.inodes.remove(&serial);
        }
    }
}
