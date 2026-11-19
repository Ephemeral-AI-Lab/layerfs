//! Stores on a bucket, whole file (#506 R): where a bucket keeps a Store
//! ([`link`]), the Store files it holds ([`copies`]), what an upload
//! records ([`create`]) and a load of a copy into a new Store
//! ([`restore`]). `backup.create` and `backup.restore` (layerfs-store) use
//! them.
//!
//! The unit is a Store, with every project in it. A bucket holds the whole
//! SQLite Store file under a file name; LayerFS never opens SQLite over the
//! network. Each request is sent once; nothing is resent after an uncertain
//! outcome.
pub mod copies;
pub mod create;
pub mod heads;
pub mod link;
pub mod restore;

pub use copies::{list_stores, Copy};
pub use create::Expected;
pub use heads::{heads, HEADS};
pub use link::{store_object, Provider, STORE_FILE};
pub use restore::{load, Loaded, PullFailure};
