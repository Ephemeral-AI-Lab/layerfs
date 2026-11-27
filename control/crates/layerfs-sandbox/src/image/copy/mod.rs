//! Copying an image from Docker to a machine: the export (`docker`), the
//! load (`load`), its lock (`lock`) and what an interrupted copy left
//! (`sweep`).
#[doc(hidden)]
pub mod docker;
#[doc(hidden)]
pub mod load;
#[doc(hidden)]
pub mod lock;
pub(crate) mod sweep;
