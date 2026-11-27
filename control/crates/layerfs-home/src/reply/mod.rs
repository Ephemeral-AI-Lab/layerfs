//! What every answer is built from: the paging cursor, progress events,
//! the call outcome, state words, the time format and answer values.
pub mod cursor;
pub mod events;
#[doc(hidden)]
pub mod outcome;
pub mod state;
pub mod time;
pub mod values;
