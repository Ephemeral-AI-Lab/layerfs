//! The host logic behind the Node binding, moved from layerfs-node (#533
//! step 2) so that every front end shares it: Store calls in this process,
//! project discovery, Sandboxes and their daemons, Store registry, remote
//! transfers and PostgreSQL Stores. layerfs-node keeps only the napi
//! conversion.
pub(crate) mod copies;
pub mod direct;
pub(crate) mod discovery;
pub mod dispatch;
pub(crate) mod exec;
pub(crate) mod flock;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod local;
pub(crate) mod local_stores;
pub mod outcome;
pub mod output;
pub(crate) mod pool;
pub(crate) mod postgres;
pub(crate) mod postgres_init;
pub(crate) mod progress;
pub(crate) mod project;
pub(crate) mod registry;
pub(crate) mod runtime;
pub(crate) mod sandbox;
pub(crate) mod stores;
pub(crate) mod sync;
pub mod trace;
pub(crate) mod unmount;
pub(crate) mod workspace;
