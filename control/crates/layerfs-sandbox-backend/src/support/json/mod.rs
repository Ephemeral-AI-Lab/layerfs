//! Shallow JSON: the streamed reader (`read`) and string quoting (`strings`).
mod read;
mod strings;
pub use read::*;
pub use strings::{quoted, Text};
