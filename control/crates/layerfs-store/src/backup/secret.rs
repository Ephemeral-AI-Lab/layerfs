//! A bucket key's secret, kept in the home's secret folder
//! (`layerfs_home::home::secrets`, kind `bucket`: `secret/bucket/<bucket>.json`,
//! a 0600 file in a 0700 folder), never answered and never logged. The
//! backup family names a secret `bucket/<bucket>` ([`super::record::secret_name`]);
//! nothing else in it reads or writes one.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::secrets::{read_secret, remove_secret, save_secret};
use layerfs_home::outcome::refused;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The secret folder's kind and name of `name` (`bucket/<bucket>`).
fn split(name: &str) -> Result<(&str, &str)> {
    name.split_once('/')
        .ok_or_else(|| refused("InvalidInput", format!("{name} is not a kind/name secret")))
}

/// The secret named `name`; a missing or empty one is `Failed`, naming the
/// call that keeps it again.
pub fn read(home: &Path, name: &str) -> Result<String> {
    let (kind, secret) = split(name)?;
    let kept = read_secret(home, kind, secret)?;
    match kept.as_ref().and_then(|value| value.get_text("secret")) {
        Some(text) if !text.is_empty() => Ok(text.to_owned()),
        Some(_) => Err(refused(
            "Failed",
            format!("the home's secret {name} is empty"),
        )),
        None => Err(refused(
            "Failed",
            format!("the home keeps no secret {name}; disconnect it and connect it again"),
        )),
    }
}

/// Keeps `secret` as the secret named `name`, replacing one kept before.
pub fn write(home: &Path, name: &str, secret: &str) -> Result<()> {
    let (kind, key) = split(name)?;
    save_secret(
        home,
        kind,
        key,
        &SdkValue::map([("secret", SdkValue::text(secret))]),
    )
}

/// Forgets the secret named `name`; one already gone is no error.
pub fn remove(home: &Path, name: &str) -> Result<()> {
    let (kind, secret) = split(name)?;
    remove_secret(home, kind, secret).map(|_| ())
}
