//! `project.init`: a project made in a Store, sqlite or PostgreSQL, from a
//! sealed folder sent to the Store's machine.
pub mod parts;
pub mod postgresql;
#[doc(hidden)]
pub mod run;
pub mod send;
pub mod sqlite;
pub mod staged;
pub mod tree;
pub mod worker;
