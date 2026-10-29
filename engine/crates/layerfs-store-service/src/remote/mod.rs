//! Stores on a remote, whole file (#506 R): a home's named remotes, push (a
//! consistent snapshot of a live Store, uploaded with its sha256 and
//! binding), pull (download, verify, then replace the local Store), clone
//! (the same into a new Store) and the last transfer of each Store.
//!
//! The unit is a Store, with every project in it. A remote holds the whole
//! SQLite Store file under the Store's name; LayerFS never opens SQLite over
//! the network. Push and pull are not atomic on the remote (no
//! compare-and-swap, by owner decision): a pull racing a push fails its
//! sha256 check instead of installing mixed bytes. Each request is sent
//! once; nothing is resent after an uncertain outcome.
pub mod copies;
pub mod heads;
pub mod pull;
pub mod push;
pub mod settings;
pub mod state;

pub use copies::{check, list_stores, Checked, Copy};
pub use heads::{heads, HEADS};
pub use layerfs_remote_storage as storage;
pub use pull::{clone_store, pull, Cloned, PullFailure, Pulled};
pub use push::{push, Expected, PushFailure, Pushed};
pub use settings::{store_object, Provider, Remotes, STORE_FILE};
pub use state::{Direction, Transfer};
