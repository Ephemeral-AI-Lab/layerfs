//! PostgreSQL Stores: connect and disconnect, the Store's saved login
//! (`login`), a Store URL as its client reads it (`address`), the check of a
//! connection from the side that dials it and the machines attached to it
//! (`check`), the family's exclusive operation (`family`, #804), opening a
//! database session
//! (`open`, the PostgreSQL URL and Store open), the held host sessions
//! Store calls read through (`reader`), the engine diagnostic entry points
//! (`diagnostic`), and a host worker's held read
//! sessions for the CLI's file reads (`pool`, #819 P-B). Never imports
//! `sqlite/`.
#[doc(hidden)]
pub mod address;
#[doc(hidden)]
pub mod check;
#[doc(hidden)]
pub mod connect;
pub mod diagnostic;
#[doc(hidden)]
pub mod family;
#[doc(hidden)]
pub mod login;
#[doc(hidden)]
pub mod open;
#[doc(hidden)]
pub mod pool;
#[doc(hidden)]
pub mod reader;
