//! Secrets (#536 §9.2): a PostgreSQL URL with a saved login put into it
//! (the login is in the home's secret folder, `home::secrets`); the full
//! URL is never written down. Worker secrets in `worker.json` and Sandbox
//! daemon keys in `control.json`.
use crate::home::{paths, read_record, write_record};
use crate::outcome::{failed, refused};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::net::SocketAddr;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `(scheme, userinfo, rest)` of a PostgreSQL URL, split as the daemon's
/// client (tokio-postgres 0.7, `UrlParser::parse_credentials`) splits it:
/// the userinfo is everything before the first `@` anywhere after the
/// scheme, not only before the first `/`, so the user part a login
/// replaces is the one the client reads. What is shown or written down
/// strips more: the widest user part this split or the URL's authority
/// (to the last `@` before the first `/`) finds, and every login
/// parameter (`layerfs_store::postgresql::connect::without_login`).
pub fn parts(url: &str) -> Result<(&str, Option<&str>, &str)> {
    let (scheme, after) = url
        .split_once("://")
        .filter(|(scheme, _)| matches!(*scheme, "postgresql" | "postgres"))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "a PostgreSQL URL starts postgresql:// or postgres://",
            )
        })?;
    match after.split_once('@') {
        Some((userinfo, rest)) => Ok((scheme, Some(userinfo), rest)),
        None => Ok((scheme, None, after)),
    }
}

fn percent(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// `url` (an address with no user and no password) with the login `user`
/// and `password` put into it, each percent-encoded; any user part the
/// URL holds is replaced. The URL it makes is never written down.
pub fn with_login(url: &str, user: &str, password: &str) -> Result<String> {
    let (scheme, _, rest) = parts(url)?;
    Ok(format!(
        "{scheme}://{}:{}@{rest}",
        percent(user),
        percent(password)
    ))
}

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
