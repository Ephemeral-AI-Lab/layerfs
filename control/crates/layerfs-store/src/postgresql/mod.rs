//! PostgreSQL Stores: connect, update and disconnect, saved logins
//! (`login`), a Store URL as its client reads it (`address`), the check of a
//! connection from the side that dials it (`check`), opening a database session
//! (`open`, the PostgreSQL URL and Store open), and the held host sessions
//! Store calls read through (`reader`). Never imports `sqlite/`.
#[doc(hidden)]
pub mod address;
#[doc(hidden)]
pub mod check;
#[doc(hidden)]
pub mod connect;
#[doc(hidden)]
pub mod login;
#[doc(hidden)]
pub mod open;
#[doc(hidden)]
pub mod reader;
#[doc(hidden)]
pub mod update;
