//! Where a remote's secret lives. Settings keep a reference, never the secret.
use crate::error::{local, RemoteError};
use std::fmt;
use std::path::PathBuf;

/// An access key and secret, redacted by `Debug`.
#[derive(Clone, Eq, PartialEq)]
pub struct Credentials {
    pub access_key: String,
    pub secret_key: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}

/// A reference to credentials, safe to store in settings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CredentialRef {
    /// `env:PREFIX`: `PREFIX_ACCESS_KEY` and `PREFIX_SECRET_KEY`; the prefix
    /// starts with `LAYERFS_` (LayerFS reads only its own settings).
    Env(String),
    /// `file:/absolute/path`: lines `access_key=…` and `secret_key=…`, in a
    /// file only its owner can read.
    File(PathBuf),
}

impl CredentialRef {
    /// Parses `env:LAYERFS_…` or `file:/absolute/path`.
    pub fn parse(text: &str) -> Result<Self, RemoteError> {
        let invalid = || RemoteError::Invalid(format!("credential reference {text:?}"));
        if let Some(prefix) = text.strip_prefix("env:") {
            if !prefix.starts_with("LAYERFS_")
                || !prefix
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(invalid());
            }
            return Ok(Self::Env(prefix.to_owned()));
        }
        let path = PathBuf::from(text.strip_prefix("file:").ok_or_else(invalid)?);
        if !path.is_absolute() {
            return Err(invalid());
        }
        Ok(Self::File(path))
    }

    /// The reference as settings store it.
    pub fn text(&self) -> String {
        match self {
            Self::Env(prefix) => format!("env:{prefix}"),
            Self::File(path) => format!("file:{}", path.display()),
        }
    }

    /// Reads the referenced credentials.
    pub fn resolve(&self) -> Result<Credentials, RemoteError> {
        let (access_key, secret_key) = match self {
            Self::Env(prefix) => {
                let read = |suffix: &str| {
                    let name = format!("{prefix}_{suffix}");
                    std::env::var(&name)
                        .map_err(|_| RemoteError::Invalid(format!("{name} is not set")))
                };
                (read("ACCESS_KEY")?, read("SECRET_KEY")?)
            }
            Self::File(path) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = std::fs::metadata(path)
                        .map_err(|e| local("credential file", e))?
                        .permissions()
                        .mode();
                    if mode & 0o077 != 0 {
                        return Err(RemoteError::Invalid(format!(
                            "credential file {} is readable by others",
                            path.display()
                        )));
                    }
                }
                let text =
                    std::fs::read_to_string(path).map_err(|e| local("credential file", e))?;
                let field = |name: &str| {
                    text.lines()
                        .find_map(|line| line.trim().strip_prefix(name)?.strip_prefix('='))
                        .map(|value| value.trim().to_owned())
                        .ok_or_else(|| {
                            RemoteError::Invalid(format!("credential file lacks {name}"))
                        })
                };
                (field("access_key")?, field("secret_key")?)
            }
        };
        if access_key.is_empty()
            || secret_key.is_empty()
            || access_key
                .bytes()
                .chain(secret_key.bytes())
                .any(|b| !(0x21..0x7f).contains(&b))
        {
            return Err(RemoteError::Invalid(
                "empty or unprintable credentials".to_owned(),
            ));
        }
        Ok(Credentials {
            access_key,
            secret_key,
        })
    }
}
