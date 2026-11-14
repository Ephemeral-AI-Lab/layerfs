//! Prototype (READDIRPLUS experiment, not a product contract): READDIRPLUS
//! entries with attributes and kernel lookup references.
//!
//! The kernel links every entry of a READDIRPLUS reply whose node id is
//! nonzero, except `.` and `..`, and takes one lookup reference for each,
//! exactly as for a LOOKUP reply; an entry it cannot link it forgets at once.
//! So the reply hands out a node id only for an entry whose reference the
//! engine took for this reply:
//!
//! - A reply that publishes its offsets takes one reference per handed-out
//!   entry in that same publishing visit (`DirectoryBatch::adopt`), as one
//!   owner job for the whole window. The facts are read once per window,
//!   outside the owner, from the authoritative base.
//! - A reply that reuses an already published reply publishes nothing, so
//!   each entry is looked up as LOOKUP does, taking its reference in its own
//!   visit. A name that no longer exists is offered with node id 0: the
//!   kernel links nothing and takes no reference for it.
//! - `.` and `..` take no reference; the kernel ignores their attributes.
//!
//! Entry and attribute lifetimes are LOOKUP's `TTL`, and the generation is
//! LOOKUP's.
use super::{reply::TTL, terminal};
use crate::{attributes::Identity, RequestDisposition};
use fuser::{Errno, FileAttr, FileType, Generation, INodeNo, ReplyDirectoryPlus};
use layerfs_content::{filesystem::PathName, object::inode_leaf::InodeKind};
use layerfs_mount_ports::{
    operations::{DirectoryBatch, KernelRead, ReadFailure},
    ports::{Fence, RequestServices},
};
use layerfs_overlay::{KernelMount, READ_WINDOW};
use layerfs_workspace::KernelReadOperation;
use std::{
    ffi::OsStr,
    io,
    os::unix::ffi::OsStrExt,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::UNIX_EPOCH,
};

/// Entries served by the per-entry LOOKUP fallback, process-wide.
static FALLBACK: AtomicU64 = AtomicU64::new(0);
/// Prototype diagnostic: entries of reused replies that were looked up one
/// by one, process-wide.
pub fn fallback_entries() -> u64 {
    FALLBACK.load(Ordering::Relaxed)
}

/// What the fallback needs to look a name up as LOOKUP does.
pub(super) struct Plus {
    pub services: Arc<dyn RequestServices>,
    pub mount: KernelMount,
    pub request: u64,
    /// The directory being listed.
    pub serial: u64,
    /// The negotiated page size: the least the kernel offers for a listing.
    pub page: u32,
}

/// Pinned `fuse_direntplus`: a 128-byte `fuse_entry_out` and a 24-byte
/// `fuse_dirent`, then the name, padded to 8.
fn size(name: usize) -> usize {
    (152 + name).next_multiple_of(8)
}

/// The most entries one reply of `page` bytes can take: every entry is at
/// least 160 bytes. Facts are read for no more entries of a window than this.
/// `LAYERFS_PROBE_READDIRPLUS_ROWS` overrides it, for comparison only.
pub(super) fn most(page: u32) -> usize {
    static ROWS: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let rows = ROWS.get_or_init(|| {
        std::env::var("LAYERFS_PROBE_READDIRPLUS_ROWS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| (1..=layerfs_overlay::PAGE_ROWS).contains(value))
    });
    let page = if page == 0 {
        READ_WINDOW
    } else {
        page as usize
    };
    rows.unwrap_or(page.min(READ_WINDOW) / size(1) + 1)
}

/// Prototype (READDIRPLUS experiment, side test): a READDIR window of at
/// most `LAYERFS_PROBE_READDIR_ROWS` keys instead of `PAGE_ROWS`. Unset:
/// unchanged.
pub(super) fn names_rows() -> Option<usize> {
    static ROWS: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    *ROWS.get_or_init(|| {
        std::env::var("LAYERFS_PROBE_READDIR_ROWS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| (1..=layerfs_overlay::PAGE_ROWS).contains(value))
    })
}

/// `.` or `..`: the kernel takes no reference and ignores the attributes,
/// so only the node id and the directory type are shown.
pub(super) fn dot(
    reply: &mut ReplyDirectoryPlus,
    used: &mut usize,
    inode: INodeNo,
    cookie: u64,
    name: &str,
) -> bool {
    let size = size(name.len());
    if size > READ_WINDOW - *used {
        return true;
    }
    if reply.add(
        inode,
        cookie,
        OsStr::new(name),
        &TTL,
        &shown(inode, FileType::Directory),
        Generation(0),
    ) {
        return true;
    }
    *used += size;
    false
}

/// A node id and type with no further facts.
fn shown(ino: INodeNo, kind: FileType) -> FileAttr {
    FileAttr {
        ino,
        size: 0,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind,
        perm: 0,
        nlink: 0,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    }
}

/// A failed fill: the errno for a reply that offered nothing, whether the
/// reply already offered entries (it is then sent as it stands, and those
/// entries keep their references), and what still owns the failure.
pub(super) struct Failure {
    pub errno: Errno,
    pub offered: bool,
    cause: Cause,
}
enum Cause {
    /// Conversion failed before any reference was taken for the batch.
    Batch(Errno),
    /// A fallback lookup failed.
    Read(ReadFailure),
    /// A fallback lookup took a reference that its reply does not carry.
    Held(ReadFailure),
}
impl Failure {
    pub(super) async fn dispose(
        self,
        batch: Box<DirectoryBatch>,
        fence: &Fence,
    ) -> RequestDisposition {
        match self.cause {
            Cause::Batch(errno) => RequestDisposition::Retained(Box::new(
                batch.retain(Box::new(io::Error::from_raw_os_error(errno.code()))),
            )),
            Cause::Read(failure) => terminal::read(fence, failure).await,
            Cause::Held(failure) => RequestDisposition::Retained(Box::new(failure)),
        }
    }
}

/// Fills the reply from one batch and returns how many of its entries the
/// reply accepted.
pub(super) async fn fill(
    reply: &mut ReplyDirectoryPlus,
    used: &mut usize,
    batch: &DirectoryBatch,
    identity: Identity,
    plus: &Plus,
) -> Result<usize, Failure> {
    let generation = Generation(plus.mount.owner_id());
    if batch.publishes() {
        let mut accepted = 0;
        for (entry, cookie) in batch.entries() {
            let attributes = match entry.stat.as_ref() {
                Some(stat) if stat.serial == entry.serial => identity.attributes(stat),
                Some(_) => Err(Errno::EIO),
                // Past the facts limit: the reply is full.
                None => break,
            };
            let attributes = attributes.map_err(|errno| Failure {
                errno,
                offered: false,
                cause: Cause::Batch(errno),
            })?;
            let size = size(entry.name.len());
            if size > READ_WINDOW - *used
                || reply.add(
                    attributes.ino,
                    cookie,
                    OsStr::from_bytes(&entry.name),
                    &TTL,
                    &attributes,
                    generation,
                )
            {
                break;
            }
            *used += size;
            accepted += 1;
        }
        return Ok(accepted);
    }
    // A reused reply: no publication takes references, so each name is
    // looked up as LOOKUP does. An entry is attempted only when it surely
    // fits, so no reference is taken for an entry the reply cannot carry.
    let budget = (plus.page as usize).min(READ_WINDOW);
    let mut offered = 0;
    for (entry, cookie) in batch.entries() {
        let size = size(entry.name.len());
        if size > budget.saturating_sub(*used) {
            break;
        }
        let before = offered != 0;
        let failed = move |errno, cause| Failure {
            errno,
            offered: before,
            cause,
        };
        let name = PathName::from_bytes(&entry.name)
            .map_err(|_| failed(Errno::EIO, Cause::Batch(Errno::EIO)))?;
        let operation = KernelReadOperation::Lookup {
            parent: plus.serial,
            name,
        };
        let answer = KernelRead::prepare(
            plus.services.clone(),
            plus.mount,
            plus.request,
            plus.serial,
            None,
            operation,
        )
        .await
        .map_err(|failure| failed(terminal::errno(failure.fenced()), Cause::Read(failure)))?;
        let attributes = match answer.value() {
            Ok(value) => identity.attributes(&value.stat).map(Some),
            // Gone since it was listed: no node id, so no reference.
            Err(_) => Ok(None),
        };
        let attributes = match attributes {
            Ok(Some(attributes)) => attributes,
            Ok(None) => shown(
                INodeNo(0),
                match entry.kind {
                    InodeKind::Directory => FileType::Directory,
                    InodeKind::RegularFile => FileType::RegularFile,
                    InodeKind::Symlink => FileType::Symlink,
                },
            ),
            Err(errno) => {
                let held = answer.retain(Box::new(io::Error::from_raw_os_error(errno.code())));
                return Err(failed(errno, Cause::Held(held)));
            }
        };
        if reply.add(
            attributes.ino,
            cookie,
            OsStr::from_bytes(&entry.name),
            &TTL,
            &attributes,
            generation,
        ) {
            let held = answer.retain(Box::new(io::Error::other(
                "READDIRPLUS entry did not fit after its lookup",
            )));
            return Err(failed(Errno::EIO, Cause::Held(held)));
        }
        *used += size;
        offered += 1;
        FALLBACK.fetch_add(1, Ordering::Relaxed);
        if let Err(failure) = answer.dispose().await {
            // This entry is in the reply with its reference.
            return Err(Failure {
                errno: Errno::EIO,
                offered: true,
                cause: Cause::Held(failure),
            });
        }
    }
    Ok(offered)
}
