//! Authenticated service handlers.

mod dispatch;
mod history;
mod object;
mod pin;
mod publication;
mod reconcile;

pub(crate) use dispatch::dispatch;
