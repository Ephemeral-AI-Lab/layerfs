//! Immutable pack bodies: `COPY BINARY` before the lane, ranged reads by `substring`.
pub(crate) mod probe;
pub(crate) mod provider;
pub(crate) mod publish;
pub(crate) mod publish_flight;
pub(crate) mod read;
