//! JSON in the home's own forms: call values (`value`: the shell's stdout and
//! stderr, the worker's frames, the Python and Node front ends' arguments),
//! the bounded value stream (`stream`), and the shallow streamed reader
//! (`read`) and string quoting (`strings`) machine setup and the platform
//! adapters use. Every reader fails with [`JsonError`].
mod read;
mod stream;
mod strings;
mod value;
pub use read::*;
pub use stream::{read_value, read_value_within, Limits};
pub use strings::{quoted, Text};
pub use value::*;
