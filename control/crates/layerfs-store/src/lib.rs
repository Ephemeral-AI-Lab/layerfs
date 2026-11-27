//! Stores in the 2.0 home (#533; cut from `layerfs-control` by
//! layerfs-project#105). `admin/` is the Store itself: its records, the
//! route each Store call takes, chosen from records, the Stores this process
//! holds open with their leases, maintenance, status and the host worker's
//! Store side. `backend/` is where a Store lives: `sqlite/` and
//! `postgresql/` never import each other; the kind-neutral `admin/` chooses
//! by `StoreKind`. `data/` is what a Store holds: its projects and the
//! `branch`, `commit` and `file` families. Backups to buckets (`backup/`)
//! are Store records too. What uses a Store from outside (a Sandbox of one
//! of its projects) comes from the caller ([`Related`]).
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod admin;
pub mod backend;
#[doc(hidden)]
pub mod backup;
pub mod data;
pub(crate) mod users;
pub mod words;

/// The Store connection reaper (not lifecycle, #535 D11), for its test.
#[doc(hidden)]
pub use admin::open::close::close_idle;
pub use admin::record::store::stores;
pub use users::{Related, StoreUsers};

// below re-exports an old path; wave 3 rewrites its callers and deletes it.
#[doc(hidden)]
pub use crate::admin::host_worker::stream as host_stream;
#[doc(hidden)]
pub use crate::admin::maintenance::upgrade;
