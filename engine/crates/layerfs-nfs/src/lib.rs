//! A mounted Workspace served as a loopback NFSv3 export.
//!
//! The platform-independent half of the native macOS route: the same
//! [`MountServices`](layerfs_vfs::ports::mount::MountServices) ports and kernel
//! operations the FUSE connection uses, answered over ONC RPC instead of
//! `/dev/fuse`. A stateless NFS client holds no lookup references or open
//! descriptors, so the export keeps them itself (see [`kernel`]).
//!
//! Every NFSv3 procedure is served (MKNOD answers `NFS3ERR_NOTSUPP`: the
//! canonical format has no special files) and the MOUNT protocol; an export
//! configured read-only answers every changing procedure `NFS3ERR_ROFS`.
//! A changing procedure's publication tickets are released only after its
//! reply was sent (see [`kernel::mutation`]); names a client invents for
//! itself are kept out of the Workspace (see [`kernel::names`] and
//! [`apple_double`]).
//! There is no third-party dependency: XDR, RPC record marking and both
//! programs are implemented here over `std::net`.
#![forbid(unsafe_code)]

pub mod apple_double;
pub mod kernel;
pub(crate) mod procedures;
pub mod protocol;
mod server;

pub use kernel::Released;
pub use procedures::program::TRANSFER_WINDOW;
pub use server::{Count, ExportConfig, Refresher, Server};

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::apple_double::codec as apple_double_codec; // #828 compat
#[doc(hidden)]
pub use crate::kernel::mutation as change; // #828 compat
#[doc(hidden)]
pub use crate::kernel::names; // #828 compat
#[doc(hidden)]
pub use crate::kernel::read; // #828 compat
#[doc(hidden)]
pub use crate::kernel::refresh; // #828 compat
#[doc(hidden)]
pub use crate::protocol::attributes; // #828 compat
#[doc(hidden)]
pub use crate::protocol::handle; // #828 compat
#[doc(hidden)]
pub use crate::protocol::rpc; // #828 compat
#[doc(hidden)]
pub use crate::protocol::xdr; // #828 compat
