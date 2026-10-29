//! A project's Store on a remote, whole file (#506 R): named remotes,
//! push (a consistent snapshot of the live Store, uploaded with its
//! sha256) and pull (download, verify, then replace the local Store).
//!
//! A remote holds the whole SQLite Store file under one name; LayerFS never
//! opens SQLite over the network. Push and pull are not atomic on the remote
//! (no compare-and-swap, by owner decision): a pull racing a push fails its
//! sha256 check instead of installing mixed bytes. Each request is sent
//! once; nothing is resent after an uncertain outcome.
pub mod pull;
pub mod push;
pub mod settings;

pub use layerfs_remote_storage as storage;
pub use pull::{pull, PullFailure, Pulled};
pub use push::{push, PushFailure, Pushed};
pub use settings::{Provider, Remotes, STORE_OBJECT};
