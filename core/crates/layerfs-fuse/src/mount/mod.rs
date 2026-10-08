//! Linux mount profile and native negotiation facts.
mod profile;

pub(crate) use profile::negotiate;
pub use profile::Negotiation;
