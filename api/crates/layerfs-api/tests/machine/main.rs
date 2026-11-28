mod family;
mod host_names;
mod nameless_records;
mod platforms_setup;
mod provider_names;
mod refusal_order;
mod removed_platforms;
#[path = "../support/root.rs"]
mod root;

#[cfg(feature = "machine")]
#[path = "../support/env.rs"]
mod env;
#[cfg(feature = "machine")]
use env::env_lock;
