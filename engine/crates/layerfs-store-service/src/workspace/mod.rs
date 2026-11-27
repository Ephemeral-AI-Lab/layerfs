//! A Workspace over the Store: its lock holder (`owner`), its bind to a Store
//! snapshot (`bind`) and its Commit (`commit`).
pub mod bind;
pub mod commit;
pub mod owner;
