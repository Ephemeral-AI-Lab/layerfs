//! READDIR and READDIRPLUS over the engine's published directory cookies.
//!
//! The NFS cookie is the engine's kernel offset (1 for `.`, 2 for `..`, then
//! the offsets the engine publishes) on one implicit directory descriptor,
//! named to the client by `cookieverf`. Cookie 0 opens a fresh descriptor and
//! a fresh verifier. A verifier whose descriptor was closed, or an offset a
//! rewind retired, is `NFS3ERR_BAD_COOKIE`; the client then lists again from 0.
//!
//! One reply carries at most one engine window: exactly the names that fit
//! the client's byte budget are published. A window that listed the
//! directory's last name and every one of its entries fits the reply is the
//! directory's end, so the same reply says so; any other reply's next call
//! resumes after the last accepted cookie.
//!
//! A READDIRPLUS entry carries the attributes the listing read — the local
//! inode row of the name, or the inherited inode's immutable facts, read for
//! the window in one batch demand for file lengths and one inode batch per
//! level of the inode tree the cache does not already answer —
//! and the handle of the entry's serial. Both are optional in RFC 1813; here
//! they are present for every named entry whose offsets this request
//! publishes, and the publishing visit takes one kernel lookup reference per
//! handed-out handle in that same job, so nothing is looked up twice and no
//! entry is left without the reference its handle presents.
use crate::{
    attributes::{post_op_maybe, Identity, Status},
    block::wait,
    handle::HANDLE_BYTES,
    kernel::Kernel,
    read::failure,
    xdr::{opaque_size, Writer},
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_mount_ports::operations::{DirectoryFailure, DirectoryStep, DirectoryStream};
use layerfs_workspace::{KernelDirectoryEntry, ViewStat};
/// One enumeration request.
pub struct Listing {
    pub serial: u64,
    pub cookie: u64,
    pub verifier: u64,
    /// The client's byte budget for the whole reply (`maxcount`).
    pub budget: usize,
    /// READDIRPLUS: each entry also carries its attributes and handle.
    pub plus: bool,
}
/// Bytes of the reply around the entries: status, the directory's
/// `post_op_attr` (reserved present, so a reply never exceeds the client's
/// budget by it), `cookieverf`, the list terminator and the EOF flag, plus
/// the RPC reply header.
const FIXED: usize = 4 + 4 + ATTR_BYTES + 8 + 4 + 4 + 24;
/// `fattr3` in one `post_op_attr`: 21 thirty-two-bit fields.
const ATTR_BYTES: usize = 84;
/// Bytes one READDIRPLUS entry adds besides its name: the attribute that
/// follows, or its absence, and the handle that follows, or its absence.
const PLUS_ATTRIBUTES: usize = 4 + ATTR_BYTES;
const PLUS_HANDLE: usize = 4 + opaque_size(HANDLE_BYTES);
const PLUS_ABSENT: usize = 4;

struct Entries {
    out: Writer,
    used: usize,
    budget: usize,
    plus: bool,
    identity: Identity,
    count: usize,
}
impl Entries {
    /// Bytes one listed entry of this reply costs against the client's budget.
    fn size(&self, name: &[u8], attributes: bool, hand_out: bool) -> usize {
        4 + 8
            + opaque_size(name.len())
            + 8
            + if self.plus {
                (if attributes {
                    PLUS_ATTRIBUTES
                } else {
                    PLUS_ABSENT
                }) + if hand_out { PLUS_HANDLE } else { PLUS_ABSENT }
            } else {
                0
            }
    }
    /// Appends `.` or `..` when it fits the budget. Both name an object the
    /// client already holds a handle for, so neither carries attributes:
    /// their two absent optional fields are all a plus reply adds.
    fn push_dot(&mut self, fileid: u64, name: &[u8], cookie: u64) -> bool {
        let size = 4 + 8 + opaque_size(name.len()) + 8 + if self.plus { 8 } else { 0 };
        if self.used + size > self.budget {
            return false;
        }
        self.used += size;
        self.count += 1;
        self.out.bool(true).u64(fileid).opaque(name).u64(cookie);
        if self.plus {
            self.out.bool(false).bool(false);
        }
        true
    }
    /// Appends one listed name when it fits the budget, with the facts this
    /// reply says about it and, when this request publishes its offset, the
    /// handle of its serial; false when it does not fit.
    fn push_entry(
        &mut self,
        kernel: &Kernel,
        handles: bool,
        entry: &KernelDirectoryEntry,
        cookie: u64,
    ) -> bool {
        // A handle needs the entry's facts: a name whose facts this reply
        // could not say anything about is listed without either.
        let facts = entry.stat.as_ref();
        let hand_out = handles && facts.is_some();
        let size = self.size(&entry.name, facts.is_some(), hand_out);
        if self.used + size > self.budget {
            return false;
        }
        self.used += size;
        self.count += 1;
        self.out
            .bool(true)
            .u64(entry.serial)
            .opaque(&entry.name)
            .u64(cookie);
        if self.plus {
            match facts {
                Some(stat) => self.identity.post_op(&mut self.out, stat),
                None => {
                    self.out.bool(false);
                }
            }
            if hand_out {
                self.out.bool(true).opaque(&kernel.encode(entry.serial));
            } else {
                self.out.bool(false);
            }
        }
        true
    }
}

/// The encoded `READDIR3resok`/`READDIRPLUS3resok` body after the status,
/// or the failure status.
pub fn list(kernel: &Kernel, request: Listing) -> Result<Vec<u8>, Status> {
    let handle = if request.cookie == 0 {
        kernel.open_directory(request.serial)?
    } else {
        kernel
            .directory(request.serial, request.verifier)
            .ok_or(Status::BadCookie)?
    };
    let (services, id) = kernel.request()?;
    let stream = wait(DirectoryStream::prepare(
        services,
        kernel.mount(),
        id,
        request.serial,
        handle,
        request.cookie,
        request.plus,
    ))
    .map_err(directory_failure)?;
    let mut entries = Entries {
        out: Writer::with_capacity(request.budget.min(64 * 1024)),
        used: FIXED,
        budget: request.budget,
        plus: request.plus,
        identity: kernel.identity(),
        count: 0,
    };
    let dots: Vec<(&str, u64, u64)> = stream.dots().collect();
    for (name, serial, cookie) in dots {
        if name == ".." && serial != request.serial {
            kernel.saw_directory(request.serial, serial);
        }
        if !entries.push_dot(serial, name.as_bytes(), cookie) {
            return finish(entries, None, handle, false);
        }
    }
    // Handles are handed out exactly for the entries this request publishes:
    // that visit takes their kernel references, so a reused published reply
    // hands out none, and no handle is left without a reference.
    let (eof, directory) = match wait(stream.next()).map_err(directory_failure)? {
        DirectoryStep::End(_) => (true, None),
        DirectoryStep::Batch(mut batch) => {
            let handed_out = batch.publishes();
            let mut accepted = 0;
            let mut handed: Vec<(ViewStat, Vec<u8>)> = Vec::new();
            for (entry, cookie) in batch.entries() {
                if !entries.push_entry(kernel, handed_out, entry, cookie) {
                    break;
                }
                if entry.kind == InodeKind::Directory {
                    kernel.saw_directory(entry.serial, request.serial);
                }
                if handed_out {
                    if let Some(stat) = entry.stat.clone() {
                        handed.push((stat, entry.name.clone()));
                    }
                }
                accepted += 1;
            }
            let at_end = batch.at_end(accepted);
            if handed_out {
                batch.adopt(accepted);
            }
            let directory = batch.directory().cloned();
            // Publishing ends this reply's window even when names remain.
            drop(wait(batch.accept(accepted)).map_err(directory_failure)?);
            // Each handed-out handle keeps exactly one reference, recorded
            // after the visit that took it, as a LOOKUP records its own.
            for (stat, name) in &handed {
                kernel.entry_taken(stat, request.serial, name);
            }
            (at_end, directory)
        }
    };
    // An inherited directory has no local row in this window: its attributes
    // are the facts this export last observed or published for it (its
    // OPENDIR read them), so the reply carries them without a read of its own.
    let directory = directory.or_else(|| kernel.directory_wcc(request.serial).0);
    finish(entries, directory.as_ref(), handle, eof)
}

fn finish(
    entries: Entries,
    directory: Option<&ViewStat>,
    verifier: u64,
    eof: bool,
) -> Result<Vec<u8>, Status> {
    if entries.count == 0 && !eof {
        return Err(Status::TooSmall);
    }
    let mut out = Writer::with_capacity(entries.out.len() + 4 + ATTR_BYTES + 4);
    post_op_maybe(entries.identity, &mut out, directory);
    out.u64(verifier);
    out.fixed(entries.out.as_bytes());
    out.bool(false); // no further entry
    out.bool(eof);
    Ok(out.into_bytes())
}
fn directory_failure(failure: DirectoryFailure) -> Status {
    if failure.rewound() {
        Status::BadCookie
    } else {
        self::failure(failure.reason)
    }
}
