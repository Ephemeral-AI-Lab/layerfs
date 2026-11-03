//! Secrets (#536 §9.2): PostgreSQL password references (`env:NAME` or
//! `file:/path`, resolved on each call; the full URL is never written
//! down), worker secrets in `worker.json` and Sandbox daemon keys in
//! `control.json`.
use crate::common::home::{paths, read_record, write_record};
use crate::common::outcome::{failed, refused};
use crate::store::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::net::SocketAddr;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `(scheme://, userinfo, rest)` of a PostgreSQL URL.
pub(crate) fn parts(url: &str) -> Result<(&str, Option<&str>, &str)> {
    let (scheme, after) = url
        .split_once("://")
        .filter(|(scheme, _)| matches!(*scheme, "postgresql" | "postgres"))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "a PostgreSQL URL starts postgresql:// or postgres://",
            )
        })?;
    let authority_end = after.find('/').unwrap_or(after.len());
    match after[..authority_end].rfind('@') {
        Some(at) => Ok((scheme, Some(&after[..at]), &after[at + 1..])),
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

/// A password reference's value: `env:NAME` or `file:/absolute/path`.
pub(crate) fn resolve(reference: &str) -> Result<String> {
    if let Some(name) = reference.strip_prefix("env:") {
        return std::env::var(name)
            .ok()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                refused(
                    "Unavailable",
                    format!("the password variable {name} is not set"),
                )
            });
    }
    if let Some(path) = reference
        .strip_prefix("file:")
        .filter(|p| Path::new(p).is_absolute())
    {
        let text = std::fs::read_to_string(path)
            .map_err(|e| refused("Unavailable", format!("{path}: {e}")))?;
        return Ok(text.trim_end_matches(['\n', '\r']).to_owned());
    }
    Err(refused(
        "InvalidInput",
        "a password is a reference: env:NAME or file:/absolute/path",
    ))
}

/// The full URL of a PostgreSQL Store's record, its password resolved now.
pub(crate) fn url(record: &StoreRecord) -> Result<String> {
    let url = record.text("url").ok_or_else(|| {
        refused(
            "Failed",
            format!("PostgreSQL Store {}'s record has no url", record.name()),
        )
    })?;
    with_password(url, record.text("password"))
}

fn with_password(url: &str, password: Option<&str>) -> Result<String> {
    let Some(reference) = password else {
        return Ok(url.to_owned());
    };
    let (scheme, user, rest) = parts(url)?;
    let user = user.ok_or_else(|| {
        refused(
            "InvalidInput",
            "a URL with a password reference names its user",
        )
    })?;
    Ok(format!(
        "{scheme}://{user}:{}@{rest}",
        percent(&resolve(reference)?)
    ))
}

/// What a machine's `worker.json` records: its worker's address and
/// secret, and the process of an ssh forward to it.
pub(crate) struct WorkerSecret {
    pub(crate) address: SocketAddr,
    pub(crate) secret: String,
    // read by the machine adapters only (`machine/spec.rs`)
    #[cfg_attr(not(feature = "machine"), allow(dead_code))]
    pub(crate) forward_pid: Option<u32>,
}

/// Machine `machine`'s `worker.json`, if it has one; one without an
/// address is refused `Failed`.
pub(crate) fn worker_secret(home: &Path, machine: &str) -> Result<Option<WorkerSecret>> {
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
        forward_pid: value.get_int("forward_pid").map(|p| p as u32),
    }))
}

/// The daemon keys in Sandbox folder `directory`'s `control.json`
/// (`endpoint`, `secret`, `peer`, `instance`), if it has them.
pub(crate) fn daemon_keys(directory: &Path) -> Result<Option<SdkValue>> {
    read_record(&directory.join("control.json"))
}

/// Writes `keys` as Sandbox folder `directory`'s `control.json` (0600).
pub(crate) fn save_daemon_keys(directory: &Path, keys: &SdkValue) -> Result<()> {
    write_record(&directory.join("control.json"), keys)
}

/// One `N`-byte key from daemon keys `keys`.
pub(crate) fn key<const N: usize>(keys: &SdkValue, name: &str) -> Result<[u8; N]> {
    match keys.get(name) {
        Some(SdkValue::Bytes(bytes)) => bytes.as_slice().try_into().map_err(failed),
        _ => Err(refused("InvalidInput", format!("discovery {name}"))),
    }
}
