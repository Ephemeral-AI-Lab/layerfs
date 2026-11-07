//! PostgreSQL Stores: connect and disconnect, opening a database session
//! (`open`, the PostgreSQL URL and Store open), and the held host sessions
//! Store calls read through (`reader`). Never imports `sqlite/`.
#[doc(hidden)]
pub mod connect;
#[doc(hidden)]
pub mod open;
#[doc(hidden)]
pub mod reader;
