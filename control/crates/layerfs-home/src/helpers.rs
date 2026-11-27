//! General helpers every entity uses (moved from `sandbox/registry.rs`,
//! layerfs-project#103): the lock of a process table, a required text
//! argument, fresh random bytes, an answer's list with its `<list>_count`
//! companion, and a map of ordered fields.
use crate::outcome::{failed, refused};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::sync::{Mutex, MutexGuard};

/// The lock of a table each holder changes in one step (one insert, remove
/// or lookup of a map or set), so the table is whole whenever a holder
/// panics: its guard is taken back from a panicked holder, and a panic in
/// one call never refuses the calls after it (layerfs-project#177). State a
/// panic can leave half-changed takes its own lock, not this one. It never
/// refuses; the result keeps its callers' shape.
pub fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, SdkRefusal> {
    Ok(mutex.lock().unwrap_or_else(|poison| poison.into_inner()))
}

/// A required text field `name` of a map (`value`, a record's or a frame's).
pub fn text<'a>(value: &'a SdkValue, name: &str) -> Result<&'a str, SdkRefusal> {
    value
        .get_text(name)
        .ok_or_else(|| refused("InvalidInput", format!("missing text argument `{name}`")))
}

/// A required text field `name` of a typed request or record: refused as
/// [`text`] refuses a missing argument.
pub fn required<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, SdkRefusal> {
    value.ok_or_else(|| refused("InvalidInput", format!("missing text argument `{name}`")))
}

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
    let now = crate::home::now_ms().max(0) as u64;
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

/// The entries an answer's list holds (call surface 2.0 SCHEMA): a list
/// keeps its first `SHOWN`.
pub const SHOWN: usize = 20;

/// `items` as an answer's list and its `<list>_count` companion: the first
/// [`SHOWN`] entries, and the count only when the list was cut (SCHEMA:
/// "`<list>_count` only beside a cut list"), else null. `total` is how many
/// there are when the source knows more than `items` holds (at least
/// `items.len()` is used); `cut` says the source itself left some out
/// without knowing how many, so the count is then what is known.
pub fn shown(mut items: Vec<SdkValue>, total: usize, cut: bool) -> (SdkValue, SdkValue) {
    let total = total.max(items.len());
    items.truncate(SHOWN);
    let count = match total > items.len() || cut {
        true => SdkValue::Int(i64::try_from(total).unwrap_or(i64::MAX)),
        false => SdkValue::Null,
    };
    (SdkValue::List(items), count)
}

/// `names` as an answer's list of text and its `<list>_count` companion, as
/// [`shown`] makes them.
pub fn shown_names(names: &[String]) -> (SdkValue, SdkValue) {
    shown(names.iter().map(SdkValue::text).collect(), 0, false)
}

/// A map of `fields`, in their order.
pub fn map_of(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}
