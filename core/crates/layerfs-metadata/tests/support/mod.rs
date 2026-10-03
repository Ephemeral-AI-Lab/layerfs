//! Explicit owned-service configuration; tests fail rather than skip when absent.
#![allow(dead_code)]
use layerfs_metadata::PgConfig;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub fn config(tag: &str) -> PgConfig {
    let mut config =
        PgConfig::from_env().expect("owned PostgreSQL environment required; no skipped tests");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    config.schema = format!(
        "lfs302_{tag}_{}_{}_{stamp}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    config
}

pub fn control(config: &PgConfig) -> postgres::Client {
    postgres::Config::new()
        .host(&config.host)
        .port(config.port)
        .user(&config.user)
        .password(&config.password)
        .dbname(&config.database)
        .connect(postgres::NoTls)
        .unwrap()
}
