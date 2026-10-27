//! READDIR and READDIRPLUS over the engine's published directory cookies.
//!
//! The NFS cookie is the engine's kernel offset (1 for `.`, 2 for `..`, then
//! the offsets the engine publishes) on one implicit directory descriptor,
//! named to the client by `cookieverf`. Cookie 0 opens a fresh descriptor and
//! a fresh verifier. A verifier whose descriptor was closed, or an offset a
//! rewind retired, is `NFS3ERR_BAD_COOKIE`; the client then lists again from 0.
//!
//! One reply carries at most one engine window: exactly the names that fit
//! the client's byte budget are published, and the reply is not at EOF. The
//! next call resumes after the last accepted cookie; a call that finds no
//! names left replies EOF. READDIRPLUS entries carry no attributes and no
//! handle (both optional in RFC 1813), so the client looks each name up.
use crate::{
    attributes::Status,
    block::wait,
    kernel::{failure, Kernel},
    xdr::{opaque_size, Writer},
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_fuse::operations::{DirectoryFailure, DirectoryStep, DirectoryStream};

/// One enumeration request.
pub struct Listing {
    pub serial: u64,
    pub cookie: u64,
    pub verifier: u64,
    /// The client's byte budget for the whole reply (`count` / `maxcount`).
    pub budget: usize,
    /// READDIRPLUS: each entry also carries two absent optional fields.
    pub plus: bool,
}
/// Bytes of the reply around the entries: status, absent post_op_attr,
/// cookieverf, the list terminator and the EOF flag, plus the RPC reply header.
const FIXED: usize = 4 + 4 + 8 + 4 + 4 + 24;

struct Entries {
    out: Writer,
    used: usize,
    budget: usize,
    plus: bool,
    count: usize,
}
impl Entries {
    fn size(&self, name: &[u8]) -> usize {
        4 + 8 + opaque_size(name.len()) + 8 + if self.plus { 8 } else { 0 }
    }
    /// Appends one entry when it fits the budget.
    fn push(&mut self, fileid: u64, name: &[u8], cookie: u64) -> bool {
        let size = self.size(name);
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
    ))
    .map_err(directory_failure)?;
    let mut entries = Entries {
        out: Writer::with_capacity(request.budget.min(64 * 1024)),
        used: FIXED,
        budget: request.budget,
        plus: request.plus,
        count: 0,
    };
    let dots: Vec<(&str, u64, u64)> = stream.dots().collect();
    for (name, serial, cookie) in dots {
        if name == ".." && serial != request.serial {
            kernel.saw_directory(request.serial, serial);
        }
        if !entries.push(serial, name.as_bytes(), cookie) {
            return finish(entries, handle, false);
        }
    }
    let eof = match wait(stream.next()).map_err(directory_failure)? {
        DirectoryStep::End(_) => true,
        DirectoryStep::Batch(batch) => {
            let mut accepted = 0;
            for (entry, cookie) in batch.entries() {
                if !entries.push(entry.serial, &entry.name, cookie) {
                    break;
                }
                if entry.kind == InodeKind::Directory {
                    kernel.saw_directory(entry.serial, request.serial);
                }
                accepted += 1;
            }
            // Publishing ends this reply's window even when names remain.
            drop(wait(batch.accept(accepted)).map_err(directory_failure)?);
            false
        }
    };
    finish(entries, handle, eof)
}
fn finish(entries: Entries, verifier: u64, eof: bool) -> Result<Vec<u8>, Status> {
    if entries.count == 0 && !eof {
        return Err(Status::TooSmall);
    }
    let mut out = Writer::with_capacity(entries.out.len() + 24);
    out.bool(false); // dir_attributes
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
