//! Stores in the 2.0 home (#533; cut from `layerfs-control` by
//! layerfs-project#105): their records, the route each Store call takes,
//! chosen from records, the Stores this process holds open, placement and
//! leases, and the steps a push, pull or remove takes on the file. The
//! kind-neutral root chooses by `StoreKind`; `sqlite/` and `postgresql/`
//! never import each other. Backups (`backup/`) and projects (`project/`)
//! are Store records too. What uses a Store from outside (a Sandbox of one
//! of its projects) comes from the caller ([`Related`]).
#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[doc(hidden)]
pub mod backup;
pub mod call;
#[doc(hidden)]
pub mod calls;
#[doc(hidden)]
pub mod identity;
#[doc(hidden)]
pub mod lease;
mod list;
#[doc(hidden)]
pub mod open;
pub mod open_args;
#[doc(hidden)]
pub mod place;
pub mod postgresql;
#[doc(hidden)]
pub mod project;
#[doc(hidden)]
pub mod record;
mod related;
#[doc(hidden)]
pub mod resolve;
#[doc(hidden)]
pub mod service;
pub mod sqlite;
mod status;

/// The Store connection reaper (not lifecycle, #535 D11), for its test.
#[doc(hidden)]
pub use open::close_idle;
pub use record::stores;
pub use related::Related;
