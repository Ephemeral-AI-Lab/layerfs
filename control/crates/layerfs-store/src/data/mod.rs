//! The calls on what a Store holds: its projects (`project`).
pub mod branch;
pub mod commit;
pub mod file;
pub mod project;
#[doc(hidden)]
pub mod unknown;

// The `branch`, `commit` and `file` families (#533 step 7): Store calls on
// one project's Branches, Commits and files, each translated to the Store
// method that does it ([`StoreOp`]) and run on the route its Store's
// records choose.

pub use super::admin::route::methods::{Names, Page, StoreOp};
pub use file::{Change, Content};
