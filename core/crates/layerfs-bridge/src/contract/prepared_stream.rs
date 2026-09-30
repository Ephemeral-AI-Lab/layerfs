//! The prepared namespace as an ordered stream: exact totals and row roles.
//!
//! A prepared update used to travel inside one metadata frame, so a generation
//! wider than that frame was refused as `Capacity` before any command was sent.
//! It now travels as a body: the request carries [`PreparedChanges`] - the
//! identity of the update plus the exact totals of the stream that follows it -
//! and the rows are delivered in one ordered, replayable sequence.
//!
//! **The totals are exact, and they are checked before the rows are read.** The
//! three sections and every row have a fixed width given their counts, so
//! [`PreparedTotals::stream_bytes`] is the whole body the sender will write. The
//! request is refused when that figure is not the one the counts imply, when it
//! exceeds [`MAX_PREPARED_STREAM_BYTES`], or when the body that arrives is not
//! exactly that long. A receiver can therefore size its window and its spool from
//! the declaration instead of discovering the size as it reads.
//!
//! **Rows are roles, not lists.** One identity row names what the serial is: an
//! existing regular file or symlink with the content and metadata roots the
//! workspace saved, a fresh one whose serial the caller's allocator just created,
//! or a directory that is either patched in place or declared for the first time.
//! The section is ordered by serial, so the three kinds that used to arrive as
//! three separate lists - typed values, portable patches and declarations - are
//! one pass here. That is what lets the service build its spool in a single pass
//! and hand C1 rows in the order C1 requires.
//!
//! [`PreparedChanges`]: super::PreparedChanges

use super::{metadata::check_portable_metadata, Code, Failure, Root};
use std::io::Read;

/// Version of the prepared stream body.
pub const PREPARED_STREAM_VERSION: u8 = 1;
/// Bytes of the prepared stream body version tag.
pub const PREPARED_STREAM_TAG_BYTES: u64 = 1;
/// Bytes one directory row occupies before its binding rows.
pub const PREPARED_DIRECTORY_ROW_BYTES: u64 = 12;
/// Bytes one binding row occupies before its name: its length and its serial.
///
/// The name itself follows, so a binding costs this plus one or more name bytes.
/// This is also the width the capture already charges a changed name at, so the
/// bytes a stream declares are the bytes the frontier reserved for it.
pub const PREPARED_BINDING_ROW_BYTES: u64 = 10;
/// Fewest name bytes one binding row can carry.
pub const PREPARED_BINDING_NAME_BYTES: u64 = 1;
/// Bytes one identity row that carries content and metadata roots.
pub const PREPARED_IDENTITY_ROW_BYTES: u64 = 73;
/// Bytes one identity row that carries portable directory attributes.
pub const PREPARED_DIRECTORY_ROW_ATTRIBUTE_BYTES: u64 = 25;
/// Largest name one binding row may carry.
pub const PREPARED_NAME_BYTES: usize = 255;
/// Charged ceiling of one prepared stream body.
///
/// The body is a spool on both sides rather than a resident vector, so this is a
/// declared resource bound and not a frame: it is the figure the transport
/// admits, the figure the service charges its receive spool against, and the
/// figure the workspace measures its own declared totals against before it sends
/// anything.
pub const MAX_PREPARED_STREAM_BYTES: u64 = 256 * 1024 * 1024;

/// One identity row of a prepared stream.
pub const ROLE_EXISTING_FILE: u8 = 1;
/// An existing symlink whose saved roots the row carries.
pub const ROLE_EXISTING_SYMLINK: u8 = 2;
/// A regular file whose serial the caller's allocator just created.
pub const ROLE_FRESH_FILE: u8 = 3;
/// A symlink whose serial the caller's allocator just created.
pub const ROLE_FRESH_SYMLINK: u8 = 4;
/// An existing directory whose portable attributes this update selects.
pub const ROLE_DIRECTORY_PATCH: u8 = 5;
/// A directory serial the service has not accepted before.
pub const ROLE_DIRECTORY_DECLARATION: u8 = 6;

/// The exact rows one prepared stream carries.
///
/// Every field is a count the sender measured over the frozen frontier it is
/// about to lower, not an estimate from an earlier capture: the two walks that
/// agree on the rows also agree on their totals, and the body is refused when it
/// does not deliver exactly them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PreparedTotals {
    /// Directory rows: one per changed directory that has a row.
    pub directories: u64,
    /// Binding rows across those directory rows.
    pub names: u64,
    /// Sum of [`PREPARED_BINDING_ROW_BYTES`] plus one name's bytes, per binding.
    pub name_bytes: u64,
    /// Identity rows: existing and fresh files, symlinks and directories.
    pub identities: u64,
    /// Of those, the directories patched in place.
    pub patches: u64,
    /// Of those, the directories declared for the first time.
    pub declarations: u64,
    /// Serials the caller's allocator just created, across every identity role.
    pub fresh: u64,
}

impl PreparedTotals {
    /// Rows that carry content and metadata roots.
    pub fn rooted_identities(&self) -> Result<u64, Failure> {
        self.identities
            .checked_sub(self.patches)
            .and_then(|rows| rows.checked_sub(self.declarations))
            .ok_or(Code::InvalidInput.into())
    }

    /// Fresh identities: the fresh files and symlinks plus the declarations.
    pub fn fresh_identities(&self) -> Result<u64, Failure> {
        let rooted = self.rooted_identities()?;
        let fresh_rooted = self
            .fresh
            .checked_sub(self.declarations)
            .ok_or_else(|| Failure::from(Code::InvalidInput))?;
        if fresh_rooted > rooted {
            return Err(Code::InvalidInput.into());
        }
        Ok(fresh_rooted)
    }

    /// Exact bytes the body carries, or a refusal when the counts cannot all
    /// describe one stream.
    ///
    /// A declaration is a fresh serial, so it cannot exceed the fresh total; a
    /// patch or a declaration is one identity row, so together they cannot exceed
    /// the identity rows. The three sections and the version tag then give the
    /// whole body.
    pub fn stream_bytes(&self) -> Result<u64, Failure> {
        let rooted = self.rooted_identities()?;
        self.fresh_identities()?;
        let attributes = self
            .patches
            .checked_add(self.declarations)
            .ok_or_else(|| Failure::from(Code::Capacity))?;
        let mut total = self
            .directories
            .checked_mul(PREPARED_DIRECTORY_ROW_BYTES)
            .and_then(|bytes| bytes.checked_add(PREPARED_STREAM_TAG_BYTES))
            .and_then(|bytes| bytes.checked_add(self.name_bytes))
            .ok_or_else(|| Failure::from(Code::Capacity))?;
        total = Some(total)
            .and_then(|bytes| {
                rooted
                    .checked_mul(PREPARED_IDENTITY_ROW_BYTES)
                    .and_then(|rows| bytes.checked_add(rows))
            })
            .and_then(|bytes| {
                attributes
                    .checked_mul(PREPARED_DIRECTORY_ROW_ATTRIBUTE_BYTES)
                    .and_then(|rows| bytes.checked_add(rows))
            })
            .ok_or_else(|| Failure::from(Code::Capacity))?;
        Ok(total)
    }

    /// Checks that the declared rows describe one stream inside the charged bound.
    pub fn check(&self) -> Result<(), Failure> {
        let bytes = self.stream_bytes()?;
        if bytes > MAX_PREPARED_STREAM_BYTES {
            return Err(Code::Capacity.into());
        }
        // A binding carries one name byte at least - a name is one byte or more -
        // so the naming bytes a stream declares can never be smaller than the
        // bindings it declares. A row may hold as many bindings as it has bytes:
        // the count is not capped here, because a directory's width is a property
        // of the directory rather than of the grammar.
        if self
            .names
            .checked_mul(PREPARED_BINDING_ROW_BYTES + PREPARED_BINDING_NAME_BYTES)
            .ok_or_else(|| Failure::from(Code::Capacity))?
            > self.name_bytes
        {
            return Err(Code::InvalidInput.into());
        }
        Ok(())
    }
}

/// One decoded identity row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparedIdentity {
    /// An existing regular file or symlink with the roots the workspace saved.
    Rooted {
        /// Serial of the identity.
        serial: u64,
        /// Kind code: 1 regular file, 3 symlink.
        kind: u8,
        /// Canonical content root.
        content: Root,
        /// Canonical portable-metadata root.
        metadata: Root,
        /// True when the caller's allocator just created this serial.
        fresh: bool,
    },
    /// An existing directory whose portable attributes this update selects.
    DirectoryPatch {
        /// Serial of the directory.
        serial: u64,
        /// Selected mode.
        mode: u32,
        /// Selected modification time, seconds.
        mtime_seconds: i64,
        /// Selected modification time, nanoseconds.
        mtime_nanoseconds: u32,
    },
    /// A directory serial the service has not accepted before.
    DirectoryDeclaration {
        /// Serial of the directory.
        serial: u64,
        /// Declared mode.
        mode: u32,
        /// Declared modification time, seconds.
        mtime_seconds: i64,
        /// Declared modification time, nanoseconds.
        mtime_nanoseconds: u32,
    },
}

impl PreparedIdentity {
    /// The serial this row names.
    pub fn serial(&self) -> u64 {
        match self {
            Self::Rooted { serial, .. }
            | Self::DirectoryPatch { serial, .. }
            | Self::DirectoryDeclaration { serial, .. } => *serial,
        }
    }
}

/// Receives one prepared row at a time as the body delivers it.
pub trait PreparedRowSink {
    /// One changed directory's final bindings, in name order.
    ///
    /// A binding's serial is `None` when the name is absent from the new
    /// directory; serial zero is never a binding, exactly as in the request.
    fn directory(
        &mut self,
        parent: u64,
        changes: Vec<(Vec<u8>, Option<u64>)>,
    ) -> Result<(), Failure>;
    /// One identity row, in serial order.
    fn identity(&mut self, row: PreparedIdentity) -> Result<(), Failure>;
}

/// Writes the stream's version tag.
pub fn put_stream_tag(out: &mut Vec<u8>) -> Result<(), Failure> {
    out.push(PREPARED_STREAM_VERSION);
    Ok(())
}

/// Writes one directory row: its parent, its binding count and its bindings.
pub fn put_directory_row(
    out: &mut Vec<u8>,
    parent: u64,
    changes: &[(Vec<u8>, Option<u64>)],
) -> Result<(), Failure> {
    let names = u32::try_from(changes.len()).map_err(|_| Failure::from(Code::Capacity))?;
    out.extend_from_slice(&parent.to_be_bytes());
    out.extend_from_slice(&names.to_be_bytes());
    for (name, serial) in changes {
        if name.is_empty() || name.len() > PREPARED_NAME_BYTES {
            return Err(Code::InvalidInput.into());
        }
        let length = u16::try_from(name.len()).map_err(|_| Failure::from(Code::Capacity))?;
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&serial.unwrap_or(0).to_be_bytes());
    }
    Ok(())
}

/// Writes one identity row that carries content and metadata roots.
pub fn put_rooted_identity(
    out: &mut Vec<u8>,
    role: u8,
    serial: u64,
    content: &Root,
    metadata: &Root,
) -> Result<(), Failure> {
    if !matches!(
        role,
        ROLE_EXISTING_FILE | ROLE_EXISTING_SYMLINK | ROLE_FRESH_FILE | ROLE_FRESH_SYMLINK
    ) {
        return Err(Code::InvalidInput.into());
    }
    out.push(role);
    out.extend_from_slice(&serial.to_be_bytes());
    out.extend_from_slice(content);
    out.extend_from_slice(metadata);
    Ok(())
}

/// Writes one identity row that carries portable directory attributes.
pub fn put_directory_identity(
    out: &mut Vec<u8>,
    role: u8,
    serial: u64,
    mode: u32,
    mtime_seconds: i64,
    mtime_nanoseconds: u32,
) -> Result<(), Failure> {
    if !matches!(role, ROLE_DIRECTORY_PATCH | ROLE_DIRECTORY_DECLARATION) {
        return Err(Code::InvalidInput.into());
    }
    check_portable_metadata(2, mode, mtime_nanoseconds)?;
    out.push(role);
    out.extend_from_slice(&serial.to_be_bytes());
    out.extend_from_slice(&mode.to_be_bytes());
    out.extend_from_slice(&mtime_seconds.to_be_bytes());
    out.extend_from_slice(&mtime_nanoseconds.to_be_bytes());
    Ok(())
}

/// Reads one prepared stream body and hands its rows to `sink`.
///
/// The declared totals are the whole contract: exactly that many rows are read,
/// exactly the bytes they imply are consumed, and a body that carries anything
/// else is refused. `root_serial` is the root the request addresses; no row may
/// name it as an allocated identity and no binding may bind it.
pub fn read_prepared_stream(
    totals: &PreparedTotals,
    root_serial: u64,
    input: &mut dyn Read,
    sink: &mut dyn PreparedRowSink,
) -> Result<(), Failure> {
    super::prepared_bindings::read_compatibility(totals, root_serial, input, sink)
}
