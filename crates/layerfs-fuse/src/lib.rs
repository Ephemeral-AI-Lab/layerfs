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
pub use port::{Attr, FilesystemPort, Kind, NodeId, PortError, PortResult, SharedPort, ROOT};
pub use proxy_client::ProxyClient;
#[doc(hidden)]
pub use proxy_client::{serve_remote_control, RemoteControl};
pub use proxy_host::ProxyHost;
pub use write_metrics::{FuseReadMetrics, FuseWriteMetrics};
