//! A mounted Workspace served as a loopback NFSv3 export.
//!
//! The platform-independent half of the native macOS route: the same
//! [`MountServices`](layerfs_fuse::ports::MountServices) ports and kernel
//! operations the FUSE connection uses, answered over ONC RPC instead of
//! `/dev/fuse`. A stateless NFS client holds no lookup references or open
//! descriptors, so the export keeps them itself (see [`kernel`]).
//!
//! Every NFSv3 procedure is served (MKNOD answers `NFS3ERR_NOTSUPP`: the
//! canonical format has no special files) and the MOUNT protocol; an export
//! configured read-only answers every changing procedure `NFS3ERR_ROFS`.
//! A changing procedure's publication tickets are released only after its
//! reply was sent (see [`change`]); names a client invents for itself are
//! kept out of the Workspace (see [`names`] and [`apple_double`]).
//! There is no third-party dependency: XDR, RPC record marking and both
//! programs are implemented here over `std::net`.
#![forbid(unsafe_code)]

pub mod apple_double;
pub mod attributes;
mod block;
pub mod change;
mod changing;
mod directory;
pub mod handle;
pub mod kernel;
mod mount;
pub mod names;
mod nfs;
pub mod refresh;
pub mod rpc;
mod server;
pub mod xdr;

pub use kernel::Released;
pub use server::{Count, ExportConfig, Server};
