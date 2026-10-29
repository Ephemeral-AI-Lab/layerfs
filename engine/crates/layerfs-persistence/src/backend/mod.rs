//! Small typed backend seam; PostgreSQL is unavailable in config.
pub(crate) mod records;
pub(crate) mod sqlite;
pub(crate) use sqlite::metadata::allocation as metadata_allocation;
pub(crate) use sqlite::metadata::locations as metadata_locations;
pub(crate) use sqlite::metadata::policy as metadata_policy;
pub(crate) use sqlite::metadata::signatures as metadata_signatures;
pub(crate) use sqlite::metadata::values as metadata_values;
pub(crate) use sqlite::objects::publish;
pub(crate) use sqlite::objects::read as objects_read;
pub(crate) use sqlite::session::connection::Session;
pub(crate) use sqlite::session::transaction::Transaction;
