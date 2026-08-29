#![forbid(unsafe_code)]

mod capture;
mod materialize;
mod port;

pub use capture::capture;
pub use materialize::{matches, materialize};
pub use port::{
    Attr, CaptureSink, Entry, Kind, MaterializationError, MaterializationSource, NodeId, Result,
};
