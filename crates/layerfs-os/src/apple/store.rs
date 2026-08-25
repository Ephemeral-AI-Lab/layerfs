use layerfs_engine::generation::{self, StoreGenerationDriver};
use layerfs_engine::integrity::IntegrityMode;
use layerfs_engine::{Engine, EngineResult};
use std::fs::File;
use std::io;
use std::path::Path;

impl StoreGenerationDriver for super::AppleDriver {
    fn available_bytes(&self, directory: &Path) -> io::Result<u64> {
        super::ffi::available_bytes(directory)
    }
    fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
        std::fs::rename(prepared, current)
    }
    fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        File::open(directory)?.sync_all()
    }
    fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
        let parent = super::ffi::open_directory_path_nofollow(
            path.parent().unwrap_or_else(|| Path::new(".")),
        )?;
        super::ffi::stable_token_at(
            &parent,
            path.file_name()
                .ok_or_else(|| io::Error::other("missing filename"))?
                .as_encoded_bytes(),
        )
    }
    fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
        let parent = super::ffi::open_directory_path_nofollow(
            path.parent().unwrap_or_else(|| Path::new(".")),
        )?;
        super::ffi::unlink_if_identity_at(
            &parent,
            path.file_name()
                .ok_or_else(|| io::Error::other("missing filename"))?
                .as_encoded_bytes(),
            expected,
        )
    }
}

impl super::AppleDriver {
    pub fn open_store(directory: &Path) -> EngineResult<Engine> {
        Self::open_store_with_integrity(directory, IntegrityMode::Verified)
    }

    pub fn open_store_with_integrity(
        directory: &Path,
        mode: IntegrityMode,
    ) -> EngineResult<Engine> {
        generation::open_or_create(directory, &Self::default(), mode)
    }

    pub fn compact_store(engine: Engine, directory: &Path) -> EngineResult<Engine> {
        generation::compact(engine, directory, &Self::default())
    }
}
