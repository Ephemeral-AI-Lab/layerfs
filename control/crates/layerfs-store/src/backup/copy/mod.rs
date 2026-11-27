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
pub mod heads;
pub mod link;
pub mod list;
pub mod load;
pub mod object;

pub use super::bucket::registry::{store_object, Provider, STORE_FILE};
pub use heads::{heads, HEADS};
pub use list::{list_stores, Copy};
pub use load::{load, Loaded, PullFailure};
pub use object::Expected;
