//! The host logic behind the Node binding, moved from layerfs-node (#533
//! step 2) so that every front end shares it: Store calls in this process,
//! project discovery, Sandboxes and their daemons, Store registry, remote
//! transfers and PostgreSQL Stores. layerfs-node keeps only the napi
//! conversion.
pub(crate) mod copies;
pub(crate) mod local_stores;
pub(crate) mod progress;
pub(crate) mod sync;
