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
//!
//! The link of each SQLite Store and its last push or pull, kept in a small
//! file per Store in the LayerFS home (`remote-state/NAME`), outside the
//! Store: a pull replaces the Store file, so state inside it would be
//! overwritten. Since #506 E2 a transfer also records the Store's history
//! heads digest (`heads`), read from the file that moved; a record from
//! before has none.
//!
//! Since #509 a record names the remote file it moved (`file`; the object
//! `<prefix>/<file>/store.sqlite`) and its time in milliseconds. A record
//! written before has no `file` line: its file is the Store's own name (the
//! object path then) and its time, in seconds, is read as milliseconds.
use crate::backup::create::Expected;
use crate::bucket::local_folder::LocalFolder;
use crate::bucket::minio::{self, MinioSettings, DEFAULT_PART_SIZE};
use crate::bucket::pieces::{hex, parse_hex32};
use crate::bucket::r2::{self, R2Settings};
use crate::bucket::{CredentialRef, Driver, Limits, RemoteError};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
            Self::R2(settings) => Box::new(r2::connect(settings)?),
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

/// The state directory's name in a LayerFS home.
pub const STATE_DIRECTORY: &str = "remote-state";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Push,
    Pull,
}

/// One completed push or pull of a Store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transfer {
    /// The remote's name in the home's settings.
    pub remote: String,
    /// The remote file's name: the object `<prefix>/<file>/store.sqlite`.
    pub file: String,
    pub direction: Direction,
    /// Completion time, in milliseconds since the Unix epoch.
    pub at: u64,
    pub sha256: [u8; 32],
    pub bytes: u64,
    /// The history heads digest of the file that moved, when known.
    pub heads: Option<[u8; 32]>,
}

impl Transfer {
    /// A transfer that completed now.
    pub fn now(
        remote: &str,
        file: &str,
        direction: Direction,
        sha256: [u8; 32],
        bytes: u64,
    ) -> Self {
        Self {
            remote: remote.to_owned(),
            file: file.to_owned(),
            direction,
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_millis() as u64),
            sha256,
            bytes,
            heads: None,
        }
    }

    /// This transfer with the moved file's heads digest.
    pub fn with_heads(mut self, heads: Option<[u8; 32]>) -> Self {
        self.heads = heads;
        self
    }

    fn encode(&self) -> String {
        let direction = match self.direction {
            Direction::Push => "push",
            Direction::Pull => "pull",
        };
        let mut text = format!(
            "remote {}\nfile {}\ndirection {direction}\nat {}\nsha256 {}\nbytes {}\n",
            self.remote,
            self.file,
            self.at,
            hex(&self.sha256),
            self.bytes
        );
        if let Some(heads) = &self.heads {
            text.push_str(&format!("heads {}\n", hex(heads)));
        }
        text
    }

    /// Reads a record; `store` is the file of a record from before #509.
    fn decode(text: &str, store: &str) -> Result<Self, String> {
        let mut lines = text.lines().peekable();
        let remote = line(&mut lines, "remote")?;
        let named = match lines.peek() {
            Some(next) if next.starts_with("file ") => Some(line(&mut lines, "file")?),
            _ => None,
        };
        let mut field = |name: &str| line(&mut lines, name);
        let direction = match field("direction")?.as_str() {
            "push" => Direction::Push,
            "pull" => Direction::Pull,
            other => return Err(format!("remote state: direction {other:?}")),
        };
        let number = |text: String| text.parse::<u64>().map_err(|_| "remote state: number");
        let at = number(field("at")?)?;
        // a record from before #509 counts seconds
        let at = if named.is_some() {
            at
        } else {
            at.saturating_mul(1000)
        };
        let file = named.unwrap_or_else(|| store.to_owned());
        store_object(&file)?;
        let sha256 = parse_hex32(&field("sha256")?).ok_or("remote state: sha256")?;
        let bytes = number(field("bytes")?)?;
        // optional: a record from before #506 E2 ends here
        let heads = match lines.next() {
            None => None,
            Some(line) => Some(
                line.strip_prefix("heads ")
                    .and_then(parse_hex32)
                    .ok_or("remote state: heads")?,
            ),
        };
        if lines.next().is_some() {
            return Err("remote state: trailing lines".to_owned());
        }
        Ok(Self {
            remote,
            file,
            direction,
            at,
            sha256,
            bytes,
            heads,
        })
    }
}

/// The next line, which must be `name value`: its value.
fn line<'a>(lines: &mut impl Iterator<Item = &'a str>, name: &str) -> Result<String, String> {
    lines
        .next()
        .and_then(|line| line.strip_prefix(name)?.strip_prefix(' '))
        .map(str::to_owned)
        .ok_or_else(|| format!("remote state: {name}"))
}

/// The state file of the Store named `store` in the home `home`.
pub fn state_path(home: &Path, store: &str) -> Result<PathBuf, String> {
    store_object(store)?;
    Ok(home.join(STATE_DIRECTORY).join(store))
}

/// The last transfer recorded at `path`, or `None` when there is none.
pub fn load(path: &Path) -> Result<Option<Transfer>, String> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let store = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            Transfer::decode(&text, store).map(Some)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// What the linked file `(remote, file)` must hold for a push through the
/// link whose record is at `path`: the sha256 the record names when it is
/// that link's, whatever its direction; no file when the record names
/// another file or there is none (a push then never overwrites a file this
/// home did not move).
pub fn expected(path: &Path, remote: &str, file: &str) -> Result<Expected, String> {
    Ok(match load(path)? {
        Some(last) if last.remote == remote && last.file == file => Expected::Sha256(last.sha256),
        _ => Expected::Absent,
    })
}

/// Removes the record at `path` (an unlinked Store's); `false` when there was none.
pub fn remove(path: &Path) -> Result<bool, String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Records `transfer` at `path` (owner-only), through a sibling claimed once
/// and renamed into place.
pub fn save(path: &Path, transfer: &Transfer) -> Result<(), String> {
    let text = |error: io::Error| format!("{}: {error}", path.display());
    if let Some(directory) = path.parent() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(directory)
                .map_err(text)?;
        }
        #[cfg(not(unix))]
        {
            fs::DirBuilder::new()
                .recursive(true)
                .create(directory)
                .map_err(text)?;
        }
    }
    let mut staging = path.as_os_str().to_owned();
    staging.push(".saving");
    let staging = PathBuf::from(staging);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
    }
    let mut file = options.open(&staging).map_err(text)?;
    file.write_all(transfer.encode().as_bytes()).map_err(text)?;
    drop(file);
    fs::rename(&staging, path).map_err(text)
}
