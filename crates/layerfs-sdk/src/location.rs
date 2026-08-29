use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StoreLocation(PathBuf);

impl StoreLocation {
    pub fn local(path: impl AsRef<Path>) -> Self {
        Self(path.as_ref().to_owned())
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl From<PathBuf> for StoreLocation {
    fn from(value: PathBuf) -> Self {
        Self(value)
    }
}
