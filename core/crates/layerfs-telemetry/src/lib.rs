//! Environment-independent parent/child timing trees for LayerFS.
//!
//! One operation owns one bounded timing tree. The operation injects child
//! scopes into the components it calls and can attach independently completed
//! timing reports. The caller receives its original `Result` together with a
//! completed [`timer::TimingReport`] and decides whether to render or save it.
//!
//! The crate is `std`-only, depends on no product crate, performs no file I/O
//! and reads no global configuration. Recording opens no file; output is written
//! through caller-supplied [`std::io::Write`] implementations. See the crate
//! README for the model, the aggregate limits and runnable examples.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod timer;

#[cfg(feature = "native")]
mod health;
pub mod observation;
pub mod operation;
#[cfg(feature = "native")]
pub mod output;
#[cfg(feature = "native")]
mod platform;
#[cfg(feature = "native")]
pub mod runtime;
