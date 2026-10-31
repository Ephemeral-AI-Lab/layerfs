//! NFSv3 (RFC 1813) status codes and the projection of Workspace inode facts.
use crate::xdr::Writer;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_workspace::{Refusal, ViewStat};

/// `nfsstat3` values this server returns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Status {
    Ok = 0,
    Perm = 1,
    NoEnt = 2,
    Io = 5,
    Access = 13,
    Exist = 17,
    NotDir = 20,
    IsDir = 21,
    Inval = 22,
    FBig = 27,
    NoSpc = 28,
    RoFs = 30,
    MLink = 31,
    NameTooLong = 63,
    NotEmpty = 66,
    Stale = 70,
    BadHandle = 10001,
    NotSync = 10002,
    BadCookie = 10003,
    NotSupp = 10004,
    TooSmall = 10005,
    ServerFault = 10006,
    /// The request may succeed later; the client retries after a delay.
    Jukebox = 10008,
}
impl Status {
    pub const fn code(self) -> u32 {
        self as u32
    }
}
/// A Workspace refusal as the status the client sees; the same errnos the
/// FUSE adapter replies with.
pub fn refusal(value: Refusal) -> Status {
    match value {
        Refusal::Exists => Status::Exist,
        Refusal::Missing => Status::NoEnt,
        Refusal::NotDirectory => Status::NotDir,
        Refusal::IsDirectory => Status::IsDir,
        Refusal::NotEmpty => Status::NotEmpty,
        Refusal::NotPermitted => Status::Perm,
        Refusal::Unsupported => Status::NotSupp,
        Refusal::Invalid | Refusal::AncestryRequired | Refusal::AncestryMismatch => Status::Inval,
        Refusal::TooManyLinks => Status::MLink,
        Refusal::TooLarge => Status::FBig,
    }
}

/// `ftype3` of a Workspace inode kind.
pub const fn file_type(kind: InodeKind) -> u32 {
    match kind {
        InodeKind::RegularFile => 1,
        InodeKind::Directory => 2,
        InodeKind::Symlink => 5,
    }
}

/// The identity every inode is shown as, and the export's fixed facts.
#[derive(Clone, Copy, Debug)]
pub struct Identity {
    /// Serial of the mount's root directory.
    pub root: u64,
    pub uid: u32,
    pub gid: u32,
    /// `fsid` of every attribute; one export is one filesystem.
    pub fsid: u64,
}
impl Identity {
    /// POSIX link count, as the FUSE adapter projects it: a directory's is
    /// derived (0 once removed, else two plus its child directories); any
    /// other kind's is its namespace reference count.
    pub fn nlink(self, value: &ViewStat) -> u32 {
        if value.kind == InodeKind::Directory {
            if value.namespace_refs == 0 && value.serial != self.root {
                0
            } else {
                u32::try_from(value.subdirs.saturating_add(2)).unwrap_or(u32::MAX)
            }
        } else {
            u32::try_from(value.namespace_refs).unwrap_or(u32::MAX)
        }
    }
    /// Encodes one `fattr3`. The Workspace keeps one timestamp; atime, mtime
    /// and ctime all show it. `fileid` is the serial itself.
    pub fn fattr3(self, out: &mut Writer, value: &ViewStat) {
        let (seconds, nanoseconds) = time(
            value.metadata.mtime_seconds,
            value.metadata.mtime_nanoseconds,
        );
        let used = value.logical_len.div_ceil(512).saturating_mul(512);
        out.u32(file_type(value.kind));
        out.u32(value.metadata.mode & 0o7777);
        out.u32(self.nlink(value));
        out.u32(self.uid).u32(self.gid);
        out.u64(value.logical_len).u64(used);
        out.u32(0).u32(0); // rdev
        out.u64(self.fsid).u64(value.serial);
        for _ in 0..3 {
            out.u32(seconds).u32(nanoseconds);
        }
    }
    /// `post_op_attr` carrying attributes.
    pub fn post_op(self, out: &mut Writer, value: &ViewStat) {
        out.bool(true);
        self.fattr3(out, value);
    }
}
/// The `ctime` a client sees for `value` (the one stored timestamp), as the
/// SETATTR guard compares it.
pub fn ctime(value: &ViewStat) -> (u32, u32) {
    time(
        value.metadata.mtime_seconds,
        value.metadata.mtime_nanoseconds,
    )
}
/// `wcc_attr` (size, mtime, ctime) of a `pre_op_attr` carrying attributes.
pub fn pre_op(out: &mut Writer, value: Option<&ViewStat>) {
    match value {
        Some(value) => {
            let (seconds, nanoseconds) = ctime(value);
            out.bool(true).u64(value.logical_len);
            out.u32(seconds).u32(nanoseconds);
            out.u32(seconds).u32(nanoseconds);
        }
        None => {
            out.bool(false);
        }
    }
}
/// `post_op_attr`, present or absent.
pub fn post_op_maybe(identity: Identity, out: &mut Writer, value: Option<&ViewStat>) {
    match value {
        Some(value) => identity.post_op(out, value),
        None => {
            out.bool(false);
        }
    }
}
/// `nfstime3` holds unsigned seconds; times before the epoch show as the epoch.
fn time(seconds: i64, nanoseconds: u32) -> (u32, u32) {
    match u32::try_from(seconds) {
        Ok(seconds) => (seconds, nanoseconds.min(999_999_999)),
        Err(_) if seconds < 0 => (0, 0),
        Err(_) => (u32::MAX, 999_999_999),
    }
}

/// ACCESS3 bits.
pub const ACCESS_READ: u32 = 0x01;
pub const ACCESS_LOOKUP: u32 = 0x02;
pub const ACCESS_MODIFY: u32 = 0x04;
pub const ACCESS_EXTEND: u32 = 0x08;
pub const ACCESS_DELETE: u32 = 0x10;
pub const ACCESS_EXECUTE: u32 = 0x20;

/// The ACCESS answer for a caller. Every inode belongs to the one command
/// identity; a caller with that uid gets the owner bits, a caller in that
/// group the group bits, anyone else the other bits. uid 0 gets no
/// superuser override: LayerFS has no root semantics, and root inside a
/// sandbox must not bypass the mode. A read-only export grants no change.
pub fn access(
    identity: Identity,
    value: &ViewStat,
    caller: Option<(u32, u32, &[u32])>,
    requested: u32,
    read_only: bool,
) -> u32 {
    let mode = value.metadata.mode;
    let bits = match caller {
        Some((uid, _, _)) if uid == identity.uid || uid == 0 => (mode >> 6) & 7,
        Some((_, gid, groups)) if gid == identity.gid || groups.contains(&identity.gid) => {
            (mode >> 3) & 7
        }
        _ => mode & 7,
    };
    let (read, write, execute) = (bits & 4 != 0, bits & 2 != 0, bits & 1 != 0);
    let directory = value.kind == InodeKind::Directory;
    let mut granted = 0;
    if read {
        granted |= ACCESS_READ;
    }
    if write && !read_only {
        granted |= ACCESS_MODIFY | ACCESS_EXTEND;
        if directory {
            granted |= ACCESS_DELETE;
        }
    }
    if execute {
        granted |= if directory {
            ACCESS_LOOKUP
        } else {
            ACCESS_EXECUTE
        };
    }
    granted & requested
}
