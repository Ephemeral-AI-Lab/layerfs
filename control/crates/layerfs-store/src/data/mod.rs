//! The calls on what a Store holds: its projects (`project`) and the
//! `branch`, `commit` and `file` families (#533 step 7): Store calls on one
//! project's Branches, Commits and files, each translated to the Store
//! method that does it ([`StoreOp`]) and run on the route its Store's
//! records choose.
#[doc(hidden)]
pub mod branch;
#[doc(hidden)]
pub mod commit;
#[doc(hidden)]
pub mod file;
pub mod project;
#[doc(hidden)]
pub mod unknown;

#[doc(hidden)]
pub use crate::admin::route::methods::{Names, Page, StoreOp};
#[doc(hidden)]
pub use file::{Change, Content};
