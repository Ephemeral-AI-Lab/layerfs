#![forbid(unsafe_code)]

#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
mod adapter;
#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
mod filesystem;
#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
mod handles;
#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
mod host_mount;
#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
mod inode_table;
mod port;
mod protocol;
mod proxy_client;
mod proxy_host;
mod write_metrics;

#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
pub use adapter::LayerFs;
#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
pub use host_mount::{mount_host, HostMount};
pub use port::{
    Attr, CallbackGuard, FilesystemPort, KernelOperation, Kind, NodeId, PortError, PortResult,
    SharedPort, ROOT,
};
pub use proxy_client::ProxyClient;
#[doc(hidden)]
pub use proxy_client::{serve_remote_control, RemoteControl};
pub use proxy_host::ProxyHost;
pub use write_metrics::{FuseReadMetrics, FuseWriteMetrics};

#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
pub use port::{ReadReply, WriteReply};

#[cfg(feature = "live")]
pub mod live_runtime;

#[cfg(feature = "live")]
mod immutable_read_cache;

#[cfg(feature = "live")]
pub mod local_spool;

#[cfg(feature = "live")]
pub mod live_wire;

#[cfg(feature = "live")]
pub mod live_transport;

#[cfg(feature = "live")]
pub mod live_owner;

#[cfg(feature = "live")]
pub use port::PortFuture;

#[cfg(feature = "live")]
pub use port::{DirectoryPage, KernelEntry, KernelReferences};
