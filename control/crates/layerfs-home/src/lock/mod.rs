//! The home's locks: the home lock (`home`), a per-entity lock (`entity`),
//! the `flock` both take, and the poison-tolerant lock of a process table
//! (`mutex`).
pub mod entity;
pub mod flock;
pub mod home;
pub mod mutex;
