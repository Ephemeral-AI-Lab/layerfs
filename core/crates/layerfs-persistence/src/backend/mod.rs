//! Small typed backend seam; PostgreSQL is unavailable in config.
pub(crate) mod records;
pub(crate) mod sqlite;
pub(crate) use sqlite::connection::Session;
pub(crate) use sqlite::metadata_allocation;
pub(crate) use sqlite::metadata_locations;
pub(crate) use sqlite::metadata_policy;
pub(crate) use sqlite::metadata_pooling;
pub(crate) use sqlite::metadata_signatures;
pub(crate) use sqlite::objects_read;
pub(crate) use sqlite::publish;
pub(crate) use sqlite::transaction::Transaction;
