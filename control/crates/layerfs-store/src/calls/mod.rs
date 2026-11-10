//! The `branch`, `commit` and `file` families (#533 step 7): Store calls on
//! one project's Branches, Commits and files, each translated to the Store
//! method that does it ([`StoreOp`]) and run on the route its Store's
//! records choose.
pub mod branch;
pub mod commit;
pub mod file;
mod op;
pub mod project;

pub use op::{Change, Content, DiffOptions, Page, StoreOp};
