//! PostgreSQL implementation of the seven C2 metadata units.
mod provider;
mod read;
mod rows;
mod write;
pub use provider::PgMetadata;
