//! Names an NFS client invents for its own bookkeeping, kept out of the
//! Workspace by the export itself so they never reach a Commit.
//!
//! - **Silly renames.** A client that unlinks a file it still has open
//!   renames it instead, and removes that name after the last close. Linux
//!   names it `.nfs` + 16 hex digits (fileid) + 8 hex digits (counter);
//!   macOS `.nfs.` + 8 hex + `.` + 4 hex. Such a RENAME is served as an
//!   UNLINK of the source name while the export keeps the inode alive by its
//!   own reference and an open descriptor; the silly name exists only in the
//!   export's table (LOOKUP, GETATTR, READ, WRITE, SETATTR and REMOVE of it
//!   work; READDIR does not list it). The final REMOVE gives the inode back.
//! - **AppleDouble.** On NFSv3 the macOS client stores extended attributes
//!   and resource forks in `._<name>` companion files. When the export is
//!   configured to keep them out, such a regular file is created in the
//!   export's memory alone (see [`crate::apple_double`]): the client's
//!   attribute write (`copyfile`, `cp`, `std::fs::copy`, `ditto`, `tar`)
//!   succeeds and reads back for the export's life, and nothing reaches the
//!   Workspace. Any other kind under such a name (directory, symlink, link,
//!   rename target) is refused with `NFS3ERR_ACCES`.
//!
//! The patterns are exact so that a user's own `.nfsrc` or `._config` is an
//! ordinary name (the AppleDouble refusal is the one deliberate exception
//! and is configurable).

/// A client-generated silly-rename name.
pub fn silly(name: &[u8]) -> bool {
    let hex = |bytes: &[u8]| bytes.iter().all(u8::is_ascii_hexdigit);
    if let Some(rest) = name.strip_prefix(b".nfs.") {
        // macOS: .nfs.XXXXXXXX.XXXX
        return rest.len() == 13 && rest[8] == b'.' && hex(&rest[..8]) && hex(&rest[9..]);
    }
    if let Some(rest) = name.strip_prefix(b".nfs") {
        // Linux: .nfs + 16 + 8 hex digits
        return rest.len() == 24 && hex(rest);
    }
    false
}
/// A macOS AppleDouble companion name (`._` followed by at least one byte).
pub fn apple_double(name: &[u8]) -> bool {
    name.len() > 2 && name.starts_with(b"._")
}
