//! A machine's Sandbox images: its image cache (`cache`), the copy of an
//! image from Docker (`copy`), the listing (`list`), the removal (`prune`)
//! and the image archive reader (`tar`).
#[doc(hidden)]
pub mod cache;
/// Copying an image from Docker to a machine.
pub mod copy;
#[doc(hidden)]
pub mod list;
#[doc(hidden)]
pub mod prune;
#[doc(hidden)]
pub mod tar;
