//! Backups of Stores to buckets (#533 step 11): bucket records and each
//! Store's links. A backup or a restore over a Store is the Store's `op`
//! ([`crate::admin::record::intent`], ended in [`ended`]); the home's secret folder is
//! behind [`secret`], the place call surface 2.0 switches to the shared
//! one.
pub mod bucket;
pub mod copy;
pub mod jobs;
pub mod providers;
#[cfg(feature = "s3_api")]
pub mod s3_api;

#[doc(hidden)]
pub use crate::backup::bucket::secret;
