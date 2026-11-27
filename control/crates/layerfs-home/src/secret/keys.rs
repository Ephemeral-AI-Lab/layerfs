//! Secrets (#536 §9.2): worker secrets in `worker.json` and Sandbox daemon
//! keys in `control.json`. A PostgreSQL login is put into a Store's URL by
//! `layerfs_store::postgresql::address::with_login`, which reads the URL
//! with the PostgreSQL client itself.
use crate::disk::{paths, read_record, write_record};
use crate::reply::outcome::{failed, refused};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::net::SocketAddr;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What a machine's `worker.json` records: its worker's address and
/// secret.
pub struct WorkerSecret {
    /// The worker's loopback address.
    pub address: SocketAddr,
    /// The worker's secret.
    pub secret: String,
}

/// Machine `machine`'s `worker.json`, if it has one; one without an
/// address is refused `Failed`.
pub fn worker_secret(home: &Path, machine: &str) -> Result<Option<WorkerSecret>> {
    let Some(value) = read_record(&paths::worker_record(home, machine))? else {
        return Ok(None);
    };
    let address = value
        .get_text("address")
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| {
            refused(
                "Failed",
                format!("machine {machine}'s worker.json has no address"),
            )
        })?;
    Ok(Some(WorkerSecret {
        address,
        secret: value.get_text("secret").unwrap_or("").to_owned(),
    }))
}

/// The daemon keys in Sandbox folder `directory`'s `control.json`
/// (`endpoint`, `secret`, `peer`, `instance`), if it has them.
pub fn daemon_keys(directory: &Path) -> Result<Option<SdkValue>> {
    read_record(&directory.join("control.json"))
}

/// Writes `keys` as Sandbox folder `directory`'s `control.json` (0600).
pub fn save_daemon_keys(directory: &Path, keys: &SdkValue) -> Result<()> {
    write_record(&directory.join("control.json"), keys)
}

/// One `N`-byte key from daemon keys `keys`.
pub fn key<const N: usize>(keys: &SdkValue, name: &str) -> Result<[u8; N]> {
    match keys.get(name) {
        Some(SdkValue::Bytes(bytes)) => bytes.as_slice().try_into().map_err(failed),
        _ => Err(refused("InvalidInput", format!("discovery {name}"))),
    }
}
