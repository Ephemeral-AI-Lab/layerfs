//! Scalar prepared bindings, exact row completion and explicit legacy collection.
use super::metadata::check_portable_metadata;
use super::prepared_stream::*;
use super::{Code, Failure, Root};
use std::io::Read;

/// Exact completed directory facts from the checked common decoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedDirectoryCompletion {
    /// Selected directory serial.
    pub parent: u64,
    /// Exact bindings consumed for this directory.
    pub bindings: u32,
    /// Exact sum of ten framing bytes plus each full name.
    pub wire_name_bytes: u64,
}

/// Receives one name at a time; no whole directory is required by this port.
pub trait PreparedBindingSink {
    /// Begins a row after its count fits the remaining declared name budgets.
    fn begin_directory(&mut self, parent: u64, bindings: u32) -> Result<(), Failure>;
    /// Receives one checked ordered full name and explicit present/absent binding.
    fn binding(&mut self, name: &[u8], child: Option<u64>) -> Result<(), Failure>;
    /// Acknowledges the exact directory count and wire bytes before the next row.
    fn end_directory(&mut self, completion: PreparedDirectoryCompletion) -> Result<(), Failure>;
    /// Receives one typed identity in serial order.
    fn identity(&mut self, row: PreparedIdentity) -> Result<(), Failure>;
}

/// Reads the unchanged v1 wire grammar through fixed current/previous-name buffers.
/// Counts and name-byte budgets are checked before sink effects; exact global EOF
/// remains required. An error stops this route without legacy collection fallback.
pub fn read_prepared_bindings(
    totals: &PreparedTotals,
    root_serial: u64,
    input: &mut dyn Read,
    sink: &mut dyn PreparedBindingSink,
) -> Result<(), Failure> {
    if root_serial == 0 || root_serial > i64::MAX as u64 {
        return Err(Code::InvalidInput.into());
    }
    totals.check()?;
    let mut reader = StreamReader::new(input, totals.stream_bytes()?);
    if reader.byte()? != PREPARED_STREAM_VERSION {
        return Err(Code::Unsupported.into());
    }
    let mut previous_parent = 0;
    let mut names = 0u64;
    let mut name_bytes = 0u64;
    let mut name = [0u8; PREPARED_NAME_BYTES];
    let mut previous_name = [0u8; PREPARED_NAME_BYTES];
    for ordinal in 0..totals.directories {
        let parent = reader.word()?;
        if parent == 0 || parent > i64::MAX as u64 || (ordinal != 0 && parent <= previous_parent) {
            return Err(Code::InvalidInput.into());
        }
        previous_parent = parent;
        let count = reader.u32()?;
        let remaining_names = totals.names.checked_sub(names).ok_or(Code::InvalidInput)?;
        let remaining_bytes = totals
            .name_bytes
            .checked_sub(name_bytes)
            .ok_or(Code::InvalidInput)?;
        let minimum = u64::from(count)
            .checked_mul(PREPARED_BINDING_ROW_BYTES + PREPARED_BINDING_NAME_BYTES)
            .ok_or(Code::Capacity)?;
        if u64::from(count) > remaining_names
            || minimum > remaining_bytes
            || minimum > reader.remaining
        {
            return Err(Code::Capacity.into());
        }
        sink.begin_directory(parent, count)?;
        let mut previous_length = 0;
        let mut row_bytes = 0u64;
        for index in 0..count {
            let length = usize::from(reader.u16()?);
            if length == 0 || length > PREPARED_NAME_BYTES {
                return Err(Code::InvalidInput.into());
            }
            let encoded = PREPARED_BINDING_ROW_BYTES + length as u64;
            let next_bytes = name_bytes.checked_add(encoded).ok_or(Code::Capacity)?;
            if next_bytes > totals.name_bytes {
                return Err(Code::Capacity.into());
            }
            reader.take(&mut name[..length])?;
            let serial = reader.word()?;
            if serial > i64::MAX as u64
                || (serial != 0 && serial == root_serial)
                || (index != 0 && previous_name[..previous_length] >= name[..length])
            {
                return Err(Code::InvalidInput.into());
            }
            sink.binding(&name[..length], (serial != 0).then_some(serial))?;
            previous_name[..length].copy_from_slice(&name[..length]);
            previous_length = length;
            names = names.checked_add(1).ok_or(Code::Capacity)?;
            name_bytes = next_bytes;
            row_bytes = row_bytes.checked_add(encoded).ok_or(Code::Capacity)?;
        }
        sink.end_directory(PreparedDirectoryCompletion {
            parent,
            bindings: count,
            wire_name_bytes: row_bytes,
        })?;
    }
    if names != totals.names || name_bytes != totals.name_bytes {
        return Err(Code::InvalidInput.into());
    }
    let mut previous_serial = 0;
    let mut patches = 0u64;
    let mut declarations = 0u64;
    let mut fresh = 0u64;
    for ordinal in 0..totals.identities {
        let role = reader.byte()?;
        let serial = reader.word()?;
        if serial == 0 || serial > i64::MAX as u64 || (ordinal != 0 && serial <= previous_serial) {
            return Err(Code::InvalidInput.into());
        }
        previous_serial = serial;
        let identity = match role {
            ROLE_EXISTING_FILE | ROLE_EXISTING_SYMLINK | ROLE_FRESH_FILE | ROLE_FRESH_SYMLINK => {
                PreparedIdentity::Rooted {
                    serial,
                    kind: if matches!(role, ROLE_EXISTING_FILE | ROLE_FRESH_FILE) {
                        1
                    } else {
                        3
                    },
                    content: reader.root()?,
                    metadata: reader.root()?,
                    fresh: matches!(role, ROLE_FRESH_FILE | ROLE_FRESH_SYMLINK),
                }
            }
            ROLE_DIRECTORY_PATCH | ROLE_DIRECTORY_DECLARATION => {
                let mode = reader.u32()?;
                let mtime_seconds = reader.word()? as i64;
                let mtime_nanoseconds = reader.u32()?;
                check_portable_metadata(2, mode, mtime_nanoseconds)?;
                if role == ROLE_DIRECTORY_PATCH {
                    patches += 1;
                    PreparedIdentity::DirectoryPatch {
                        serial,
                        mode,
                        mtime_seconds,
                        mtime_nanoseconds,
                    }
                } else {
                    declarations += 1;
                    PreparedIdentity::DirectoryDeclaration {
                        serial,
                        mode,
                        mtime_seconds,
                        mtime_nanoseconds,
                    }
                }
            }
            _ => return Err(Code::Unsupported.into()),
        };
        if matches!(
            &identity,
            PreparedIdentity::Rooted { fresh: true, .. }
                | PreparedIdentity::DirectoryDeclaration { .. }
        ) {
            if serial == root_serial {
                return Err(Code::InvalidInput.into());
            }
            fresh = fresh.checked_add(1).ok_or(Code::Capacity)?;
        }
        if patches > totals.patches || declarations > totals.declarations || fresh > totals.fresh {
            return Err(Code::InvalidInput.into());
        }
        sink.identity(identity)?;
    }
    if patches != totals.patches || declarations != totals.declarations || fresh != totals.fresh {
        return Err(Code::InvalidInput.into());
    }
    reader.finish()
}

pub(super) fn read_compatibility(
    totals: &PreparedTotals,
    root_serial: u64,
    input: &mut dyn Read,
    sink: &mut dyn PreparedRowSink,
) -> Result<(), Failure> {
    let mut collector = Collector { sink, row: None };
    read_prepared_bindings(totals, root_serial, input, &mut collector)
}

struct Collector<'a> {
    sink: &'a mut dyn PreparedRowSink,
    row: Option<CollectedDirectory>,
}

struct CollectedDirectory {
    parent: u64,
    bindings: u32,
    changes: Vec<(Vec<u8>, Option<u64>)>,
}

impl PreparedBindingSink for Collector<'_> {
    fn begin_directory(&mut self, parent: u64, bindings: u32) -> Result<(), Failure> {
        if self.row.is_some() {
            return Err(Code::InvalidInput.into());
        }
        let mut row = Vec::new();
        row.try_reserve_exact(bindings as usize)
            .map_err(|_| Code::Capacity)?;
        self.row = Some(CollectedDirectory {
            parent,
            bindings,
            changes: row,
        });
        Ok(())
    }
    fn binding(&mut self, name: &[u8], child: Option<u64>) -> Result<(), Failure> {
        let row = self.row.as_mut().ok_or(Code::InvalidInput)?;
        if row.changes.len() >= row.bindings as usize {
            return Err(Code::InvalidInput.into());
        }
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(name.len())
            .map_err(|_| Code::Capacity)?;
        owned.extend_from_slice(name);
        row.changes.push((owned, child));
        Ok(())
    }
    fn end_directory(&mut self, completion: PreparedDirectoryCompletion) -> Result<(), Failure> {
        let row = self.row.take().ok_or(Code::InvalidInput)?;
        if row.parent != completion.parent
            || row.bindings != completion.bindings
            || row.changes.len() != row.bindings as usize
        {
            return Err(Code::InvalidInput.into());
        }
        self.sink.directory(row.parent, row.changes)
    }
    fn identity(&mut self, row: PreparedIdentity) -> Result<(), Failure> {
        self.sink.identity(row)
    }
}

struct StreamReader<'a> {
    input: &'a mut dyn Read,
    remaining: u64,
}
impl<'a> StreamReader<'a> {
    fn new(input: &'a mut dyn Read, bytes: u64) -> Self {
        Self {
            input,
            remaining: bytes,
        }
    }
    fn take(&mut self, bytes: &mut [u8]) -> Result<(), Failure> {
        let count = u64::try_from(bytes.len()).map_err(|_| Code::Capacity)?;
        if count > self.remaining {
            return Err(Code::InvalidInput.into());
        }
        self.input
            .read_exact(bytes)
            .map_err(|_| Code::InvalidInput)?;
        self.remaining -= count;
        Ok(())
    }
    fn byte(&mut self) -> Result<u8, Failure> {
        let mut b = [0; 1];
        self.take(&mut b)?;
        Ok(b[0])
    }
    fn u16(&mut self) -> Result<u16, Failure> {
        let mut b = [0; 2];
        self.take(&mut b)?;
        Ok(u16::from_be_bytes(b))
    }
    fn u32(&mut self) -> Result<u32, Failure> {
        let mut b = [0; 4];
        self.take(&mut b)?;
        Ok(u32::from_be_bytes(b))
    }
    fn word(&mut self) -> Result<u64, Failure> {
        let mut b = [0; 8];
        self.take(&mut b)?;
        Ok(u64::from_be_bytes(b))
    }
    fn root(&mut self) -> Result<Root, Failure> {
        let mut b = [0; 32];
        self.take(&mut b)?;
        Ok(b)
    }
    fn finish(self) -> Result<(), Failure> {
        let mut byte = [0; 1];
        if self.remaining != 0 || self.input.read(&mut byte).map_err(|_| Code::InvalidInput)? != 0 {
            return Err(Code::InvalidInput.into());
        }
        Ok(())
    }
}
