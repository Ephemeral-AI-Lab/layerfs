//! The last push or pull of each Store, kept in a small file per Store in
//! the LayerFS home (`remote-state/NAME`), outside the Store: a pull
//! replaces the Store file, so state inside it would be overwritten.
use layerfs_remote_storage::pieces::{hex, parse_hex32};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
    pub direction: Direction,
    /// Completion time, in seconds since the Unix epoch.
    pub at: u64,
    pub sha256: [u8; 32],
    pub bytes: u64,
}

impl Transfer {
    /// A transfer that completed now.
    pub fn now(remote: &str, direction: Direction, sha256: [u8; 32], bytes: u64) -> Self {
        Self {
            remote: remote.to_owned(),
            direction,
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs()),
            sha256,
            bytes,
        }
    }

    fn encode(&self) -> String {
        let direction = match self.direction {
            Direction::Push => "push",
            Direction::Pull => "pull",
        };
        format!(
            "remote {}\ndirection {direction}\nat {}\nsha256 {}\nbytes {}\n",
            self.remote,
            self.at,
            hex(&self.sha256),
            self.bytes
        )
    }

    fn decode(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let mut field = |name: &str| {
            lines
                .next()
                .and_then(|line| line.strip_prefix(name)?.strip_prefix(' '))
                .map(str::to_owned)
                .ok_or_else(|| format!("remote state: {name}"))
        };
        let remote = field("remote")?;
        let direction = match field("direction")?.as_str() {
            "push" => Direction::Push,
            "pull" => Direction::Pull,
            other => return Err(format!("remote state: direction {other:?}")),
        };
        let number = |text: String| text.parse::<u64>().map_err(|_| "remote state: number");
        Ok(Self {
            remote,
            direction,
            at: number(field("at")?)?,
            sha256: parse_hex32(&field("sha256")?).ok_or("remote state: sha256")?,
            bytes: number(field("bytes")?)?,
        })
    }
}

/// The state file of the Store named `store` in the home `home`.
pub fn state_path(home: &Path, store: &str) -> Result<PathBuf, String> {
    crate::remote::settings::store_object(store)?;
    Ok(home.join(STATE_DIRECTORY).join(store))
}

/// The last transfer recorded at `path`, or `None` when there is none.
pub fn load(path: &Path) -> Result<Option<Transfer>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Transfer::decode(&text).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Records `transfer` at `path` (owner-only), through a sibling claimed once
/// and renamed into place.
pub fn save(path: &Path, transfer: &Transfer) -> Result<(), String> {
    let text = |error: io::Error| format!("{}: {error}", path.display());
    if let Some(directory) = path.parent() {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)
            .map_err(text)?;
    }
    let mut staging = path.as_os_str().to_owned();
    staging.push(".saving");
    let staging = PathBuf::from(staging);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&staging).map_err(text)?;
    file.write_all(transfer.encode().as_bytes()).map_err(text)?;
    drop(file);
    fs::rename(&staging, path).map_err(text)
}
