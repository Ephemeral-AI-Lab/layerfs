//! One attribute set per kernel inode, always taken from published state.
use layerfs_vfs::requests::Published;
use layerfs_vfs::ViewStat;
use std::{error::Error, fmt};

/// A successful mutation whose published value cannot describe the inode the
/// kernel is updating. The reply is refused rather than composed from request
/// inputs or an older view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Incoherent {
    /// The publishing job returned no inode for an attribute-bearing reply.
    Unpublished,
    /// The published inode is not the one this reply's node denotes.
    Foreign { published: u64, expected: u64 },
}
impl fmt::Display for Incoherent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "kernel attribute coherence: {self:?}")
    }
}
impl Error for Incoherent {}

/// The attributes an attribute-bearing mutation reply must carry. Hard-link
/// aliases share one serial, so a change through any name updates the single
/// kernel inode that every alias reads. `expected` is the serial the kernel
/// addressed (SETATTR, LINK); a new entry names its own fresh serial.
pub fn published(value: &Published, expected: Option<u64>) -> Result<&ViewStat, Incoherent> {
    let stat = value.stat.as_ref().ok_or(Incoherent::Unpublished)?;
    match expected {
        Some(expected) if stat.serial != expected => Err(Incoherent::Foreign {
            published: stat.serial,
            expected,
        }),
        _ => Ok(stat),
    }
}

// Cached pages of an open file. A known base install changes no name, byte,
// link, attribute or serial of the mounted view, so nothing here acts on it.
use layerfs_vfs::Refusal;

/// How the kernel treats cached pages of one opened file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenCache {
    /// Pages cached by an earlier open stay valid: every change to a mounted
    /// view arrives as a request on this connection, which updates or drops
    /// the affected pages itself.
    pub keep: bool,
    /// Reads and writes go through the page cache, never around it.
    pub direct: bool,
}
/// The same answer for read-only, writable and created descriptors.
pub const fn open_cache() -> OpenCache {
    OpenCache {
        keep: true,
        direct: false,
    }
}
/// WRITE replies the full count or an error, never a short count: a short
/// reply would make the kernel shrink its size or resend a tail the daemon
/// already published. A mapped store clipped by the engine is still answered
/// in full, because the kernel had already clipped it to its own size.
pub fn written(requested: usize) -> Result<u32, Refusal> {
    u32::try_from(requested).map_err(|_| Refusal::Invalid)
}

// Publication before reply, and ticket release only after the reply attempt.
//
// A mutation takes effect at the commit of its single owner job. Its reply is
// computed from that job's published values, so it is never older than the
// state it reports and never older than an earlier mutation's attempted
// reply: the one SQL owner orders every publication. A read-class reply is
// computed by an owner job that runs after its request was received. The
// kernel discards an attribute reply sampled before a newer inode version;
// that discard, not reply order on the wire, is what the daemon relies on.
use crate::RequestDisposition;
use layerfs_vfs::requests::{KernelMutation, MutationFailure};

/// Called once the reply method returned. The pinned library reports no send
/// result, so this is a reply *attempt*: the ticket is released whether or not
/// the kernel received it, and a published mutation is never undone.
pub async fn attempted(mutation: KernelMutation) -> RequestDisposition {
    match mutation.replied().await {
        Ok(()) => RequestDisposition::Complete,
        Err(failure) => retained(failure),
    }
}
/// A mutation that stopped before or after publication keeps every owner it
/// still has; nothing is released, resent or rolled back on a guess.
pub fn retained(failure: MutationFailure) -> RequestDisposition {
    RequestDisposition::Retained(Box::new(failure))
}
