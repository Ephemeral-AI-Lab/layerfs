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

pub mod admin;
pub mod backend;
#[doc(hidden)]
pub mod backup;
pub mod data;
pub(crate) mod users;
pub mod words;

/// The Store connection reaper (not lifecycle, #535 D11), for its test.
#[doc(hidden)]
pub use admin::open::cache::close_idle;
pub use admin::record::store::stores;
pub use users::{Related, StoreUsers};

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::admin::route::call; // #828 compat
#[doc(hidden)]
pub mod calls {
    // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::hide_path as project;
    pub use crate::data::*; // #828 compat // #828 compat
}
#[doc(hidden)]
pub use crate::admin::host_worker::handoff as host_maintenance; // #828 compat
#[doc(hidden)]
pub use crate::admin::host_worker::postgresql as host_postgresql; // #828 compat
#[doc(hidden)]
pub use crate::admin::host_worker::read as host_read; // #828 compat
#[doc(hidden)]
pub use crate::admin::host_worker::stream as host_stream; // #828 compat
#[doc(hidden)]
pub use crate::admin::host_worker::write as host_write; // #828 compat
#[doc(hidden)]
pub use crate::admin::maintenance::compact; // #828 compat
#[doc(hidden)]
pub use crate::admin::open::args as open_args; // #828 compat
#[doc(hidden)]
pub use crate::admin::open::cache as open; // #828 compat
#[doc(hidden)]
pub use crate::admin::open::lease; // #828 compat
#[doc(hidden)]
pub use crate::admin::record::identity; // #828 compat
#[doc(hidden)]
pub use crate::admin::record::intent; // #828 compat
#[doc(hidden)]
pub use crate::admin::route::executor as place; // #828 compat
#[doc(hidden)]
pub use crate::admin::route::place_view; // #828 compat
#[doc(hidden)]
pub use crate::admin::status::formats; // #828 compat
#[doc(hidden)]
pub use crate::admin::status::get; // #828 compat
#[doc(hidden)]
pub use crate::admin::status::list; // #828 compat
#[doc(hidden)]
pub use crate::admin::status::observed; // #828 compat
#[doc(hidden)]
pub use crate::backend::postgresql;
#[doc(hidden)]
pub use crate::backend::sqlite::diagnostic; // #828 compat // #828 compat
#[doc(hidden)]
pub mod project {
    // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::delete::run as delete; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::init::parts; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::init::postgresql as postgres_init; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::init::run as family; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::init::send; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::init::sqlite as add; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::init::staged; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::init::tree;
    #[doc(hidden)]
    pub use crate::data::project::init::worker as init; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::record::discovery; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::record::intent as op; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::record::project as record; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::status::get as read; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::status::listing; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::status::live; // #828 compat
    #[doc(hidden)]
    pub use crate::data::project::status::state; // #828 compat
    pub use crate::data::project::*; // #828 compat // #828 compat
}
#[doc(hidden)]
pub use crate::admin::maintenance::compact_report as report; // #828 compat
#[doc(hidden)]
pub use crate::admin::maintenance::repair_mount_holders as repair; // #828 compat
#[doc(hidden)]
pub use crate::admin::open::service;
#[doc(hidden)]
pub use crate::admin::record::store as record; // #828 compat
#[doc(hidden)]
pub use crate::admin::route::resolve; // #828 compat // #828 compat
#[doc(hidden)]
pub mod sqlite {
    // #828 compat
    pub use crate::backend::sqlite::*; // #828 compat
    #[doc(hidden)]
    pub mod transfer {
        // #828 compat
        #[doc(hidden)]
        pub use crate::backend::sqlite::move_leftover as resolve; // #828 compat
        #[doc(hidden)]
        pub use crate::backend::sqlite::move_phases as phases; // #828 compat
        #[doc(hidden)]
        pub use crate::backend::sqlite::step_request as step; // #828 compat
        #[doc(hidden)]
        pub use crate::backend::sqlite::step_run as steps;
        pub use crate::backend::sqlite::*; // #828 compat // #828 compat
    }
}
#[doc(hidden)]
pub use crate::admin::maintenance::upgrade;
#[doc(hidden)]
pub use crate::admin::status::state; // #828 compat
#[doc(hidden)]
pub use crate::data::unknown; // #828 compat // #828 compat
