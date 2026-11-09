//! A cancel received before registration. Callers hold the Sandbox record lock.
//! The mark is keyed by the Workspace and exec id's record path, consumed once
//! at registration, and ignored after thirty seconds. Unmount removes its folder.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{now_ms, read_record, write_entity};
use layerfs_home::outcome::refused;
use std::path::Path;

const LIFETIME_MS: i64 = 30_000;

pub fn mark(record: &Path) -> Result<(), SdkRefusal> {
    write_entity(
        &record.with_extension("cancel"),
        &SdkValue::map([(
            "expires",
            SdkValue::Int(now_ms().saturating_add(LIFETIME_MS)),
        )]),
    )
}

pub fn consume(record: &Path) -> Result<Option<i64>, SdkRefusal> {
    let path = record.with_extension("cancel");
    let Some(value) = read_record(&path)? else {
        return Ok(None);
    };
    let expires = value
        .get_int("expires")
        .ok_or_else(|| refused("Failed", "invalid pending exec cancel"))?;
    super::record::remove(&path)?;
    let now = now_ms();
    // Reject clock rollback as well as expiration; a stale mark cannot last forever.
    Ok((expires > now && expires.saturating_sub(now) <= LIFETIME_MS).then_some(now))
}
