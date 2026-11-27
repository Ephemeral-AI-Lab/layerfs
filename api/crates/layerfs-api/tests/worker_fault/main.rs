//! The worker's fault switches (layerfs-project#177): the connection, Store
//! and inspect cases (one binary since #828). Empty without the
//! `worker-fault` cargo feature:
//!
//! `cargo test -p layerfs-api --features worker-fault --test worker_fault`
#![cfg(feature = "worker-fault")]
mod connection;
mod inspect;
mod store;
#[path = "../support/worker_store.rs"]
mod worker_store;
