use ed25519_dalek::{Signer, SigningKey};
use layerfs_storage::{Result, StorageError};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) struct Writer(SigningKey);

impl Writer {
    pub(crate) fn create(database: &Path) -> Result<Self> {
        let path = appended(database, ".signer");
        let bytes = create_key(&path)?;
        Self::from_bytes(bytes)
    }

    pub(crate) fn connect(database: &Path) -> Result<Self> {
        let path = appended(database, ".signer");
        if !path.is_file() {
            return Err(StorageError::StoreMissing);
        }
        Self::from_bytes(std::fs::read(path)?)
    }

    fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| StorageError::Integrity("Stack signer"))?;
        Ok(Self(SigningKey::from_bytes(&key)))
    }

    pub(crate) fn public_key(&self) -> [u8; 32] {
        self.0.verifying_key().to_bytes()
    }

    pub(crate) fn sign(&self, bytes: &[u8]) -> [u8; 64] {
        self.0.sign(bytes).to_bytes()
    }
}

fn create_key(path: &Path) -> Result<Vec<u8>> {
    let mut key = [0; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut key)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Ok(std::fs::read(path)?);
        }
        Err(error) => return Err(error.into()),
    };
    if let Err(error) = file.write_all(&key).and_then(|_| file.sync_all()) {
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    Ok(key.to_vec())
}

fn appended(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    value.into()
}
