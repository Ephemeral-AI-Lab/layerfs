//! Ordered, replayable prepared rows: one cursor per kind, one row at a time.
//!
//! A prepared filesystem update is three ordered row sequences: a directory's
//! final bindings by parent serial, the typed inode values by serial, and the
//! serials the caller's allocator just created. [`RowSource`] is that part of an
//! update; [`PreparedRows`] adds the fixed fields one operation is addressed
//! with. Both are written so a caller can supply the rows from anywhere - the
//! resident slices of [`FilesystemInput`], a validated receive spool, a file -
//! without the operation holding a row per changed name in memory.
//!
//! **A cursor is a pass, not a position.** Asking for one walks the whole
//! sequence from its start; asking again walks it again. The validation and the
//! builder each take several passes, and a replayable source is what lets them do
//! that without materializing the rows between the passes. Nothing here requires
//! random access to a resident vector: a source whose rows are outside the process
//! answers [`RowSource::directory_for`] and [`RowSource::value_for`] from its own
//! bounded window.
//!
//! **Every row is owned by the caller.** A cursor returns one decoded row at a
//! time, so the resident cost of reading a sequence is that one row rather than
//! the sequence. The row is exactly as large as the directory it describes, which
//! is the unit the mounted route itself produces.
//!
//! **Declared totals are checked, not trusted.** A source declares how many rows
//! each sequence holds, and [`check_input`] refuses one whose cursors end
//! anywhere else. A source that under-declares would otherwise let an operation
//! succeed while rows it never read stayed unapplied.
//!
//! [`FilesystemInput`]: crate::filesystem::input::FilesystemInput

mod check;
mod source;
mod spool;
mod update;

pub use check::check_input;
pub(crate) use source::serial_in_range;
pub use source::{
    DirectoryRowSource, InodeRowSource, PreparedRows, RowSource, SerialRowSource,
    SliceDirectoryRows, SliceInodeRows, SliceSerialRows,
};
pub use spool::{RowSpool, SPOOL_SLOT_BYTES};
pub use update::PreparedUpdate;
