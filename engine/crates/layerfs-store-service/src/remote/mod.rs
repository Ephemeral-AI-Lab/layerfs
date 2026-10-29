//! Stores on a remote, whole file (#506 R): a home's named remotes, push (a
//! consistent snapshot of a live Store, uploaded with its sha256 and
//! binding), pull (download, verify, then replace the local Store), load
//! (the same into a new Store) and the link and last transfer of each Store.
//!
//! The unit is a Store, with every project in it. A remote holds the whole
//! SQLite Store file under a file name (#509: the one the Store links to,
//! not necessarily its own). Push, pull and load move whole files; a view
//! (#510) reads a pushed file in place, read-only, through ranged requests
//! pinned to one version. Push and pull are not atomic on the remote (no
//! compare-and-swap, by owner decision): a pull racing a push fails its
//! sha256 check instead of installing mixed bytes. Each request is sent
//! once; nothing is resent after an uncertain outcome.
pub mod copies;
pub mod heads;
pub mod pull;
pub mod push;
pub mod settings;
pub mod state;
pub mod view;

pub use copies::{check, list_stores, Checked, Copy};
pub use heads::{heads, HEADS};
pub use layerfs_remote_storage as storage;
pub use pull::{load, pull, Loaded, PullFailure, Pulled};
pub use push::{push, Expected, PushFailure, Pushed};
pub use settings::{store_object, Provider, Remotes, STORE_FILE};
pub use state::{Direction, Transfer};
pub use view::{open_view, OpenedView, ViewFailure, ViewStats};
