//! Native continuation orchestration over independently reusable Workspace plans.
mod lookup;
mod read;

pub use lookup::{NativeRead, ReadFailure};
pub use read::NativeData;
