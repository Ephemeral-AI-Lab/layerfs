//! Scalar directory selections and exact final-name cursors.

use std::sync::atomic::{AtomicU64, Ordering};

use super::{PreparedRows, RowSource};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;

static NEXT_AUTHORITY: AtomicU64 = AtomicU64::new(1);

/// The result of looking up one name in a directory's final changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingLookup {
    /// This update does not mention the name; its base binding survives.
    Unmentioned,
    /// This update explicitly makes the name absent.
    Absent,
    /// This update binds the name to this positive inode serial.
    Present(u64),
}

/// One live source's issued header authority, without a resident registry.
#[derive(Debug)]
pub struct BindingAuthority {
    token: u64,
}

impl BindingAuthority {
    /// Burns one checked token from the live process issuer.
    pub fn new() -> ContentResult<Self> {
        let token = NEXT_AUTHORITY
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |next| {
                (next != 0).then(|| next.checked_add(1).unwrap_or(0))
            })
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "binding authority tokens",
            })?;
        Ok(Self { token })
    }

    /// Issues one scalar selection; its source checks the selected row on use.
    pub fn header(
        &self,
        parent: u64,
        ordinal: u64,
        bindings: u32,
        wire_name_bytes: u64,
    ) -> ContentResult<DirectoryHeader> {
        if !super::serial_in_range(parent) {
            return Err(ContentError::InvalidRecord("directory parent"));
        }
        check_name_totals(u64::from(bindings), wire_name_bytes)?;
        Ok(DirectoryHeader {
            issuer: self.token,
            parent,
            ordinal,
            bindings,
            wire_name_bytes,
        })
    }

    /// True only for a header issued by this live source.
    pub fn accepts(&self, header: &DirectoryHeader) -> bool {
        self.token == header.issuer
    }

    /// Wraps an independently supplied iterator in the exact completion checks.
    /// The iterator owns its provider/span checks; this cursor checks logical rows.
    pub fn cursor<I>(
        &self,
        header: &DirectoryHeader,
        rows: I,
    ) -> ContentResult<super::CheckedBindings<I>>
    where
        I: Iterator<Item = ContentResult<(PathName, Option<u64>)>>,
    {
        if !self.accepts(header) {
            return Err(ContentError::InvalidRecord("directory issuer"));
        }
        Ok(super::CheckedBindings::new(*header, rows))
    }
}

/// An issued directory selection with no caller-supplied physical offsets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryHeader {
    issuer: u64,
    parent: u64,
    ordinal: u64,
    bindings: u32,
    wire_name_bytes: u64,
}

impl DirectoryHeader {
    /// Parent serial whose final names this selection describes.
    pub const fn parent(&self) -> u64 {
        self.parent
    }
    /// Exact number of changed names.
    pub const fn binding_count(&self) -> u32 {
        self.bindings
    }
    /// Sum of wire framing10 plus full name bytes, per binding.
    pub const fn wire_name_bytes(&self) -> u64 {
        self.wire_name_bytes
    }
    /// Opaque selected descriptor interpreted and checked by its issuing source.
    /// Spool/Slice use table positions; the compatibility source uses the parent.
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }
}

/// Completion produced only after a cursor reaches its exact selected EOF.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryCompletion {
    header: DirectoryHeader,
}

impl DirectoryCompletion {
    pub(super) const fn finished(header: DirectoryHeader) -> Self {
        Self { header }
    }
    /// Parent whose complete changes were consumed.
    pub const fn parent(&self) -> u64 {
        self.header.parent
    }
    /// Exact consumed binding count.
    pub const fn binding_count(&self) -> u32 {
        self.header.bindings
    }
    /// Exact consumed wire binding bytes.
    pub const fn wire_name_bytes(&self) -> u64 {
        self.header.wire_name_bytes
    }
    /// True only for the exact issuer, ordinal and totals selected by `header`.
    pub fn matches(&self, header: &DirectoryHeader) -> bool {
        self.header == *header
    }
}

/// One ordered pass over fixed-size directory selections.
pub trait DirectoryHeaderSource {
    /// The next selection in parent order, or exact sequence EOF.
    fn next_header(&mut self) -> ContentResult<Option<DirectoryHeader>>;
}

/// One fallible pass over the final names of one selected directory.
pub trait BindingRowSource {
    /// One full name and its final binding, or exact selected EOF.
    fn next_binding(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>>;
    /// Confirms exact EOF; it never drains an unread cursor.
    fn finish(&mut self) -> ContentResult<DirectoryCompletion>;
}

/// Scalar directory input plus the retained typed-value/fresh row contract.
pub trait BindingRows: RowSource {
    /// Opens a parent-ordered pass over scalar directory selections.
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>>;
    /// Selects the changed directory with this parent, if it has a row.
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>>;
    /// Opens a pass from this source's exact issued directory selection.
    fn bindings(&self, header: &DirectoryHeader) -> ContentResult<Box<dyn BindingRowSource + '_>>;
    /// Looks up a changed name without conflating unmentioned and tombstone.
    fn binding_for(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup>;
}

/// An addressed prepared operation whose directory consumers use scalar names.
pub trait PreparedBindingRows: PreparedRows + BindingRows {}

impl<T: PreparedRows + BindingRows + ?Sized> PreparedBindingRows for T {}

pub(super) fn check_name_totals(bindings: u64, bytes: u64) -> ContentResult<()> {
    let minimum = bindings
        .checked_mul(11)
        .ok_or(ContentError::LengthOverflow)?;
    let maximum = bindings
        .checked_mul(265)
        .ok_or(ContentError::LengthOverflow)?;
    if bytes < minimum || bytes > maximum {
        return Err(ContentError::InvalidRecord("binding byte count"));
    }
    Ok(())
}
