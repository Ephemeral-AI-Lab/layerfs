mod docker_default_image;
mod force;
mod force_native;
mod no_start;
mod start_stop;
mod take_down;
mod worker_dial;

#[cfg(feature = "machine")]
#[path = "../support/env.rs"]
mod env;
#[cfg(feature = "machine")]
use env::env_lock;
