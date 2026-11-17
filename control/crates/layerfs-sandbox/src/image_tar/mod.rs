//! The flat root filesystem tar a Sandbox image is copied as: `docker
//! export`'s output with the files `docker create`'s init layer adds taken
//! out (`.dockerenv`, `etc/hosts`, `etc/hostname`, `etc/resolv.conf`,
//! `etc/mtab`, everything under `dev/`), every device node and FIFO (which
//! the unpacker refuses), hardlinks targeting dropped entries refused clearly,
//! sparse entries expanded to regular files, and directory modification times
//! normalized to 0 with recomputed header checksums. Those are the bytes that
//! differ between two exports of one image; what is left is the image's own
//! tree. The filter streams: it holds at most one entry's extended headers and
//! sparse chunk map, never a file's bytes.

mod filter;
mod header;
mod pax;
mod sparse;

pub use filter::{dropped, Filtered, SparseReport, TarReport, SPARSE_MAX};
pub use pax::EXTENDED_TOTAL_MAX;
