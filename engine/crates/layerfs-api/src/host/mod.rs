//! The host logic behind the Node binding, moved from layerfs-node (#533
//! step 2) so that every front end shares it: Store calls in this process,
//! project discovery, Sandboxes and their daemons, Store registry, remote
//! transfers and PostgreSQL Stores. layerfs-node keeps only the napi
//! conversion.
mod copies;
pub mod direct;
mod discovery;
pub mod dispatch;
mod exec;
mod flock;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod local;
mod local_stores;
pub mod outcome;
pub mod output;
mod pool;
mod postgres;
mod progress;
mod project;
mod registry;
mod runtime;
mod sandbox;
mod stores;
mod sync;
pub mod trace;
mod unmount;
mod workspace;
