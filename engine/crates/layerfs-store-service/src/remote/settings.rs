//! The named remotes of one LayerFS home, kept in one text file in the home
//! (`remotes`). One remote holds many Stores, each under its own name: Store
//! `NAME` is the file `NAME/store.sqlite` in the remote's namespace (plus its
//! pieces when split). Each remote names its provider and where the
//! credentials are; the file never holds a secret.
//!
//! Each remote is a `remote NAME` line followed by `key value` lines:
//! `provider minio` with `endpoint http://host:port`, `bucket`, `prefix`,
//! `region`, `credentials env:LAYERFS_REMOTE` (or `file:/path`) and
//! `part-size`; `provider r2` (Cloudflare R2) with `account`, `jurisdiction`
//! (empty, `eu` or `fedramp`), `bucket`, `prefix`, `credentials` and
//! `part-size`; or `provider local_folder` with `path /absolute/dir`.
use layerfs_remote_storage::cloudflare_r2::{self, R2Settings};
use layerfs_remote_storage::local_folder::LocalFolder;
use layerfs_remote_storage::minio::{self, MinioSettings, DEFAULT_PART_SIZE};
use layerfs_remote_storage::{CredentialRef, Driver, Limits, RemoteError};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The settings file's name in a LayerFS home.
pub const HOME_FILE: &str = "remotes";
/// The file name of every Store on a remote, under the Store's name.
pub const STORE_FILE: &str = "store.sqlite";

/// The remote file of the Store named `store` (one segment of
/// `[A-Za-z0-9._-]`, not starting with a dot).
pub fn store_object(store: &str) -> Result<String, String> {
    let valid = !store.is_empty()
        && store.len() <= 128
        && !store.starts_with('.')
        && store
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    if !valid {
        return Err(format!(
            "Store name {store:?}: use [A-Za-z0-9._-], at most 128, not starting with a dot"
        ));
    }
    Ok(format!("{store}/{STORE_FILE}"))
}
const HEADER: &str = "# LayerFS remotes; credentials are references, never secrets.";

/// Where one remote keeps the Store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Provider {
    Minio(MinioSettings),
    /// Cloudflare R2, over HTTPS (a build with the `tls` feature).
    R2(R2Settings),
    /// A local directory, for tests and local copies.
    LocalFolder(PathBuf),
}

impl Provider {
    /// The provider's driver. Resolves credentials; sends nothing.
    pub fn driver(&self) -> Result<Box<dyn Driver>, RemoteError> {
        Ok(match self {
            Self::Minio(settings) => Box::new(minio::connect(settings)?),
            Self::R2(settings) => Box::new(cloudflare_r2::connect(settings)?),
            Self::LocalFolder(path) => Box::new(LocalFolder::new(
                path,
                Limits {
                    max_file: u64::MAX,
                    max_request: u64::MAX,
                    piece_size: u64::MAX,
                },
            )?),
        })
    }
}

/// A home's remotes by name.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Remotes {
    remotes: BTreeMap<String, Provider>,
}

fn check_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b));
    if valid {
        Ok(())
    } else {
        Err(format!("remote name {name:?}: use [a-z0-9_-], at most 64"))
    }
}

/// A value safe on one settings line.
fn check_value(key: &str, value: &str) -> Result<(), String> {
    if value.bytes().any(|b| b.is_ascii_control()) {
        return Err(format!("remote setting {key} has a control character"));
    }
    Ok(())
}

impl Remotes {
    pub fn get(&self, name: &str) -> Option<&Provider> {
        self.remotes.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.remotes.keys().map(String::as_str)
    }

    /// Adds or replaces the remote `name`.
    pub fn set(&mut self, name: &str, provider: Provider) -> Result<(), String> {
        check_name(name)?;
        let mut probe = Self::default();
        probe.remotes.insert(name.to_owned(), provider.clone());
        // what is set must read back unchanged
        if Self::decode(&probe.encode())? != probe {
            return Err(format!("remote {name} does not round-trip"));
        }
        self.remotes.insert(name.to_owned(), provider);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Option<Provider> {
        self.remotes.remove(name)
    }

    /// The settings text.
    pub fn encode(&self) -> String {
        let mut text = format!("{HEADER}\n");
        for (name, provider) in &self.remotes {
            text.push_str(&format!("\nremote {name}\n"));
            match provider {
                Provider::Minio(s) => {
                    text.push_str(&format!(
                        "provider minio\nendpoint {}\nbucket {}\nprefix {}\nregion {}\n\
                         credentials {}\npart-size {}\n",
                        s.endpoint,
                        s.bucket,
                        s.prefix,
                        s.region,
                        s.credentials.text(),
                        s.part_size
                    ));
                }
                Provider::R2(s) => {
                    text.push_str(&format!(
                        "provider r2\naccount {}\njurisdiction {}\nbucket {}\nprefix {}\n\
                         credentials {}\npart-size {}\n",
                        s.account,
                        s.jurisdiction,
                        s.bucket,
                        s.prefix,
                        s.credentials.text(),
                        s.part_size
                    ));
                }
                Provider::LocalFolder(path) => {
                    text.push_str(&format!("provider local_folder\npath {}\n", path.display()));
                }
            }
        }
        text
    }

    /// Parses settings text. Unknown keys, including any secret, are refused.
    pub fn decode(text: &str) -> Result<Self, String> {
        let mut blocks: Vec<(String, BTreeMap<String, String>)> = Vec::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            if key == "remote" {
                check_name(value)?;
                blocks.push((value.to_owned(), BTreeMap::new()));
                continue;
            }
            let (_, fields) = blocks
                .last_mut()
                .ok_or_else(|| format!("line {}: setting outside a remote", number + 1))?;
            if fields.insert(key.to_owned(), value.to_owned()).is_some() {
                return Err(format!("line {}: repeated {key}", number + 1));
            }
        }
        let mut remotes = BTreeMap::new();
        for (name, mut fields) in blocks {
            let mut take = |key: &str| {
                fields
                    .remove(key)
                    .ok_or_else(|| format!("remote {name}: {key} missing"))
            };
            let provider = match take("provider")?.as_str() {
                "minio" => Provider::Minio(MinioSettings {
                    endpoint: take("endpoint")?,
                    bucket: take("bucket")?,
                    prefix: take("prefix")?,
                    region: take("region")?,
                    credentials: CredentialRef::parse(&take("credentials")?)
                        .map_err(|e| e.to_string())?,
                    part_size: match take("part-size") {
                        Ok(size) => size.parse().map_err(|_| "part-size".to_owned())?,
                        Err(_) => DEFAULT_PART_SIZE,
                    },
                }),
                "r2" => Provider::R2(R2Settings {
                    account: take("account")?,
                    jurisdiction: take("jurisdiction")?,
                    bucket: take("bucket")?,
                    prefix: take("prefix")?,
                    credentials: CredentialRef::parse(&take("credentials")?)
                        .map_err(|e| e.to_string())?,
                    part_size: match take("part-size") {
                        Ok(size) => size.parse().map_err(|_| "part-size".to_owned())?,
                        Err(_) => DEFAULT_PART_SIZE,
                    },
                }),
                "local_folder" => Provider::LocalFolder(PathBuf::from(take("path")?)),
                other => return Err(format!("remote {name}: unknown provider {other:?}")),
            };
            if let Some(key) = fields.keys().next() {
                return Err(format!("remote {name}: unknown setting {key:?}"));
            }
            if remotes.insert(name.clone(), provider).is_some() {
                return Err(format!("remote {name} repeated"));
            }
        }
        let decoded = Self { remotes };
        for (name, provider) in &decoded.remotes {
            let values: Vec<(&str, String)> = match provider {
                Provider::Minio(s) => vec![
                    ("endpoint", s.endpoint.clone()),
                    ("bucket", s.bucket.clone()),
                    ("prefix", s.prefix.clone()),
                    ("region", s.region.clone()),
                ],
                Provider::R2(s) => vec![
                    ("account", s.account.clone()),
                    ("jurisdiction", s.jurisdiction.clone()),
                    ("bucket", s.bucket.clone()),
                    ("prefix", s.prefix.clone()),
                ],
                Provider::LocalFolder(path) => vec![("path", path.display().to_string())],
            };
            for (key, value) in values {
                check_value(key, &value).map_err(|e| format!("remote {name}: {e}"))?;
            }
        }
        Ok(decoded)
    }

    /// The settings file of the LayerFS home `home`.
    pub fn home_path(home: &Path) -> PathBuf {
        home.join(HOME_FILE)
    }

    /// Reads the settings file; an absent file has no remotes.
    pub fn load(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(text) => Self::decode(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("{}: {error}", path.display())),
        }
    }

    /// Writes the settings file (owner-only) through a sibling claimed once
    /// and renamed into place.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = |error: io::Error| format!("{}: {error}", path.display());
        let mut staging = path.as_os_str().to_owned();
        staging.push(".saving");
        let staging = PathBuf::from(staging);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&staging).map_err(text)?;
        file.write_all(self.encode().as_bytes()).map_err(text)?;
        drop(file);
        fs::rename(&staging, path).map_err(text)
    }
}
