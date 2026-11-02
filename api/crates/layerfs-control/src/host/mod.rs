//! The host logic behind the Node binding, moved from layerfs-node (#533
//! step 2) so that every front end shares it: Store calls in this process,
//! project discovery, Sandboxes and their daemons, Store registry, remote
//! transfers and PostgreSQL Stores. layerfs-node keeps only the napi
//! conversion.
pub(crate) mod copies;
pub(crate) mod discovery;
pub mod dispatch;
pub(crate) mod exec;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod local;
pub(crate) mod local_stores;
pub mod output;
pub(crate) mod pool;
pub(crate) mod progress;
pub(crate) mod registry;
pub(crate) mod runtime;
pub(crate) mod sandbox;
pub(crate) mod sync;
pub(crate) mod unmount;
pub(crate) mod workspace;
