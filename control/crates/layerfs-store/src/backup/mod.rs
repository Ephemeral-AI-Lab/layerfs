//! Backups of Stores to buckets (#533 step 11): bucket records and each
//! Store's links. A backup or a restore over a Store is the Store's `op`
//! ([`crate::intent`], ended in [`ended`]); the home's secret folder is
//! behind [`secret`], the place call surface 2.0 switches to the shared
//! one.
#[doc(hidden)]
pub mod create;
pub mod ended;
#[doc(hidden)]
pub mod family;
pub mod heads;
#[doc(hidden)]
pub mod list;
#[doc(hidden)]
pub mod progress;
#[doc(hidden)]
pub mod record;
#[doc(hidden)]
pub mod restore;
#[doc(hidden)]
pub mod restored;
pub mod secret;
