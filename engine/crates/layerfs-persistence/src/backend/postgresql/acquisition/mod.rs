//! Init's acquisition working state on PostgreSQL (#533 step 10b).
//!
//! The same three tables as SQLite's acquisition schema, in the Store's
//! schema, reached only through the operation-prefixed keys below. Row
//! encodings, charges and failures are SQLite's (`sqlite/acquisition`), so a
//! unit's outcome does not depend on the engine. Windows bind whole columns
//! as arrays, so a window costs a fixed number of round trips.
mod cleanup;
mod provider;
mod reads;
mod writes;

pub use provider::PostgresAcquisition;
