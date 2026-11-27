//! The synthetic kernel: the lookup references, open descriptors and parent
//! index a FUSE kernel would hold, kept by the export for a stateless client.
//!
//! The engine fences every request on a kernel reference: the root's implicit
//! one, a lookup reference, or an open descriptor. An NFS client presents a
//! handle at any time and never sends FORGET, OPEN or RELEASE, so:
//!
//! - every serial the export hands out in a handle holds exactly one lookup
//!   reference, taken by the LOOKUP visit or the creating mutation that
//!   decided it; a later visit that adds one more for the same serial is
//!   answered by forgetting the extra one at once;
//! - READ and WRITE use one implicit descriptor per file (read-only until the
//!   first WRITE, which reopens it writable), and READDIR one implicit
//!   directory descriptor per enumeration (`cookieverf`); both are bounded
//!   and the least recently used is closed when the bound is reached;
//! - `..` is answered from the parent recorded when a directory was looked
//!   up, created, moved or listed, since the engine resolves names only
//!   downward;
//! - a serial whose last name a REMOVE, RMDIR or replacing RENAME of this
//!   export took away gives its reference and descriptor back at once, so the
//!   engine can reclaim it; its handle is stale afterwards, as on any server;
//! - a client's silly rename (`.nfs…`) is not a namespace change: the source
//!   name is unlinked and the inode is kept alive by the export alone, under
//!   a name only this export knows (see [`crate::kernel::names`]).
//!
//! - the references are bounded (reference collection, "plan A" of the A2
//!   design, with no engine interface added): every handed-out serial keeps
//!   the name it was last reached by. When more than the configured number
//!   of serials hold a reference, the least recently used quarter that holds
//!   no descriptor and no silly name is forgotten, its name memory kept. A
//!   later handle of a forgotten serial is resolved again by the ordinary
//!   LOOKUP of that remembered name (its parent first, recursively): the
//!   same serial continues; another serial or none is a stale handle, as for
//!   a file renamed or removed elsewhere.
//!
//! Everything still held is given back by [`Kernel::release`], which runs
//! before the engine mount is revoked.
mod descriptors;
pub mod mutation;
pub mod names;
pub mod read;
mod references;
pub mod refresh;
mod released;
mod state;
pub(crate) mod wait;

pub use released::Released;
pub(crate) use state::Table;
pub use state::{Kernel, DIRECTORIES_HELD, FILES_HELD, LOOKUPS_HELD, RECENT_KEPT, VERIFIERS_HELD};
