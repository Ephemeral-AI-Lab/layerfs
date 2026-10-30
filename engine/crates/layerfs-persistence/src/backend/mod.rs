//! The Store engine seam (#506): SQLite always, PostgreSQL behind the
//! `postgresql` feature. Shared Metadata, Objects and history semantics reach
//! an engine only through [`Mechanics`] and [`HistoryStatements`].
pub(crate) mod admission;
#[cfg(feature = "postgresql")]
pub(crate) mod postgresql;
pub(crate) mod records;
mod seam;
pub(crate) mod sqlite;
pub(crate) use seam::{Column, Engine, HistoryStatements, Mechanics};
pub(crate) use sqlite::metadata::locations as metadata_locations;
pub(crate) use sqlite::objects::publish;
pub(crate) use sqlite::session::connection::Session;
pub(crate) use sqlite::session::transaction::Transaction;
