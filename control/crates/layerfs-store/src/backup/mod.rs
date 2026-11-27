//! Backups of Stores to buckets (#533 step 11): bucket records and each
//! Store's links. A backup or a restore over a Store is the Store's `op`
//! ([`crate::intent`], ended in [`ended`]); the home's secret folder is
//! behind [`secret`], the place call surface 2.0 switches to the shared
//! one.
pub mod bucket;
pub mod copy;
pub mod jobs;
pub mod providers;
#[cfg(feature = "s3_api")]
pub mod s3_api;

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::backup::bucket::connect as family; // #828 compat
#[doc(hidden)]
pub use crate::backup::jobs::create; // #828 compat
#[doc(hidden)]
pub use crate::backup::jobs::ended; // #828 compat
#[doc(hidden)]
pub use crate::backup::jobs::list as heads; // #828 compat
#[doc(hidden)]
pub use crate::backup::jobs::list; // #828 compat
#[doc(hidden)]
pub use crate::backup::jobs::progress; // #828 compat
#[doc(hidden)]
pub mod record {
    // #828 compat
    pub use crate::backup::bucket::record::*;
    pub use crate::backup::copy::link::StoreLink;
}
#[doc(hidden)]
pub use crate::backup::bucket::secret;
#[doc(hidden)]
pub use crate::backup::jobs::restore; // #828 compat
#[doc(hidden)]
pub use crate::data::project::record::restored; // #828 compat // #828 compat
