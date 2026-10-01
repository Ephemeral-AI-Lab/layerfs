//! Replayable prepared input: issued directory headers and scalar final names.
//!
//! [`BindingRows`] is the bounded directory interface; [`RowSource`] retains the
//! explicit old whole-directory compatibility API and typed-value/fresh cursors.
//! [`SliceBindingRows`] borrows existing caller slices without cloning directories.
//! [`CompatibilityBindingRows`] deliberately selects the old profile for external
//! row providers, never as a fallback after a bounded source fails.
//!
//! [`RowSpool`] streams one directory at a time into a charged file, seals exact
//! counts/spans and serves full-name point probes from sparse immutable offsets.
//! The format is private input metadata, not a canonical or durability format.

mod binding;
mod borrowed;
mod check;
mod checked;
mod declaration;
mod point;
mod preparation;
mod resident_cursor;
mod source;
mod spool;
mod spool_bindings;
mod spool_compatibility;
mod spool_cursor;
mod spool_point;
mod spool_slots;
mod spool_writer;
mod update;
mod verified_empty;

pub use binding::{
    BindingAuthority, BindingLookup, BindingRowSource, BindingRows, DirectoryCompletion,
    DirectoryHeader, DirectoryHeaderSource, PreparedBindingRows,
};
pub use borrowed::{CompatibilityBindingRows, SliceBindingRows};
pub use check::{check_binding_input, check_input};
pub use checked::CheckedBindings;
pub use declaration::SpoolDeclaration;
pub use point::{BindingPoint, BindingSourceId, BINDING_POINT_BYTES, BINDING_SOURCE_BYTES};
pub use preparation::SpoolPreparation;
pub(crate) use source::lookup_binding;
pub(crate) use source::ordinal_binding;
pub(crate) use source::serial_in_range;
pub use source::{
    DirectoryRowSource, InodeRowSource, PreparedRows, RowSource, SerialRowSource,
    SliceDirectoryRows, SliceInodeRows, SliceSerialRows,
};
pub use spool::{RowSpool, SpoolReadWork, SPOOL_SLOT_BYTES};
pub use update::{PreparedBindingUpdate, PreparedUpdate};

pub use verified_empty::VerifiedEmptyRows;

mod small_file_pending;
mod small_file_rows;
mod small_file_source;
pub use small_file_pending::PendingSmallFiles;
pub use small_file_source::{SmallFileBuilder, VerifiedSmallFileRows, SMALL_FILE_ROWS};
