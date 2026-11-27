//! Shallow JSON: the streamed reader (`read`) and string quoting (`strings`).
mod read;
mod strings;
mod stream;
pub use read::*;
pub use strings::{quoted, Text};
