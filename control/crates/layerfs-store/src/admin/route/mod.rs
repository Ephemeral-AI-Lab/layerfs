//! How a Store call reaches its Store: the route chosen from records, the
//! executor it runs on, the Store and project it names, the Store methods it
//! sends and the placed mount intents its admission compares.
pub mod call;
#[doc(hidden)]
pub mod executor;
pub(crate) mod methods;
pub mod mount_intents;
pub mod place_view;
#[doc(hidden)]
pub mod resolve;
