//! Ordered place values and fresh random bytes: what a record names its
//! incarnation, generation or tombstone with.
use crate::reply::outcome::failed;
use layerfs_protocol::sdk::SdkRefusal;

/// One ordered place value, `p1:<home>.<kind>:<zero-padded Unix
/// milliseconds>:<nonce>`, minted by the home whose id is `home`: a newer
/// mint compares greater than every younger one of the same home without a
/// Store counter, and a home restore that brings an older one back compares
/// lower (admission calls that uncertain, never dead). Values of different
/// homes never order, so another home's machine or Sandbox of the same name
/// cannot prove this home's place gone. An empty `home` mints a value with
/// no home id, which (like a value written before this release) compares
/// equal only to itself.
///
/// `floor` is the value this one replaces (the previous incarnation, the
/// previous generation or the name's tombstone): the new value's
/// milliseconds are at least one past the floor's, so a wall clock stepped
/// backwards never mints a value older than the one it follows.
pub fn ordered_token(home: &str, kind: &str, floor: &[&str]) -> String {
    let now = crate::disk::now_ms().max(0) as u64;
    let milliseconds = floor
        .iter()
        .filter_map(|value| token_milliseconds(value))
        .map(|floor| floor.saturating_add(1))
        .fold(now, u64::max);
    let nonce = match random() {
        Ok(bytes) => layerfs_protocol::sdk::hex(&bytes[..8]),
        Err(_) => format!("{:016x}", milliseconds),
    };
    let home: String = home.chars().filter(char::is_ascii_alphanumeric).collect();
    let label = match home.is_empty() {
        true => kind.to_owned(),
        false => format!("{home}.{kind}"),
    };
    format!("p1:{label}:{milliseconds:016}:{nonce}")
}

/// The milliseconds an ordered place value names, when it is one.
fn token_milliseconds(value: &str) -> Option<u64> {
    let mut parts = value.strip_prefix("p1:")?.split(':');
    parts.next()?;
    parts.next()?.parse().ok()
}

/// 32 fresh random bytes from the transport's key generator.
pub fn random() -> Result<[u8; 32], SdkRefusal> {
    layerfs_protocol::duplex::generate_keypair()
        .map(|pair| pair.public)
        .map_err(failed)
}
