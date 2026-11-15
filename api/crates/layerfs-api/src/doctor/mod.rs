//! The `doctor` family: its one call, `doctor.fix`, which removes a
//! leftover the report listed (layerfs-project#180). The report itself
//! (`options`, `run`) is an explicit diagnostic outside the call table and
//! the home migration dispatcher.
mod fix;
mod options;
mod run;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[fix::CALL]
}

pub use options::{doctor_request, duration};
pub use run::{doctor_options_json, doctor_with_options};
