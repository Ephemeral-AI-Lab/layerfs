//! Store generation creation and filesystem driver port.

use super::selector::{generation_filename, install, selector, StoreSelector, SELECTOR_BYTES};
use crate::integrity::IntegrityMode;
use crate::{CompactionStorageObservation, Engine, EngineError, EngineResult};
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Installs an already-created, synced selector and syncs its containing directory.
///
/// Implementations must reconcile a potentially visible replacement before a caller
/// attempts another installation.
pub trait StoreGenerationDriver: Send + Sync {
    fn available_bytes(&self, directory: &Path) -> io::Result<u64>;
    fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()>;
    fn sync_directory(&self, directory: &Path) -> io::Result<()>;
    fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()>;
}

pub struct NativeGenerationDriver;

impl StoreGenerationDriver for NativeGenerationDriver {
    fn available_bytes(&self, directory: &Path) -> io::Result<u64> {
        let output = Command::new("df").arg("-Pk").arg(directory).output()?;
        if !output.status.success() {
            return Err(io::Error::other("df failed"));
        }
        let output = std::str::from_utf8(&output.stdout)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "df output"))?;
        let available = output
            .lines()
            .rfind(|line| !line.trim().is_empty())
            .and_then(|line| line.split_whitespace().nth(3))
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "df available bytes"))?;
        available
            .checked_mul(1024)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "df overflow"))
    }

    fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
        fs::rename(prepared, current)
    }

    fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        fs::File::open(directory)?.sync_all()
    }

    fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
        native_file_identity(path)
    }

    fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
        if native_file_identity(path)?.as_slice() != expected {
            return Err(io::Error::other("file identity changed"));
        }
        fs::remove_file(path)
    }
}

#[cfg(unix)]
fn native_file_identity(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::other("generation path is not a regular file"));
    }
    Ok(unix_file_identity(&metadata))
}

#[cfg(unix)]
pub(crate) fn opened_file_identity(file: &fs::File) -> io::Result<Vec<u8>> {
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::other("generation handle is not a regular file"));
    }
    Ok(unix_file_identity(&metadata))
}

#[cfg(unix)]
fn unix_file_identity(metadata: &fs::Metadata) -> Vec<u8> {
    use std::os::unix::fs::MetadataExt;

    let mut identity = Vec::with_capacity(24);
    identity.extend_from_slice(&metadata.dev().to_be_bytes());
    identity.extend_from_slice(&metadata.ino().to_be_bytes());
    identity.extend_from_slice(&metadata.len().to_be_bytes());
    identity
}

#[cfg(not(unix))]
fn native_file_identity(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    portable_file_identity(&metadata)
}

#[cfg(not(unix))]
pub(crate) fn opened_file_identity(file: &fs::File) -> io::Result<Vec<u8>> {
    portable_file_identity(&file.metadata()?)
}

#[cfg(not(unix))]
fn portable_file_identity(metadata: &fs::Metadata) -> io::Result<Vec<u8>> {
    if !metadata.file_type().is_file() {
        return Err(io::Error::other("generation is not a regular file"));
    }
    let modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut identity = Vec::new();
    identity.extend_from_slice(&metadata.len().to_be_bytes());
    identity.extend_from_slice(&modified.as_nanos().to_be_bytes());
    Ok(identity)
}

pub(super) fn create_genesis(
    directory: &Path,
    driver: &dyn StoreGenerationDriver,
    mode: IntegrityMode,
) -> EngineResult<()> {
    let generation = directory.join(generation_filename(0));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&generation)
        .map_err(super::super::io_engine_error)?;
    let mut custody = GenesisCustody {
        generation: generation.clone(),
        current: directory.join("CURRENT"),
        temporary: directory.join("CURRENT.tmp"),
        directory,
        driver,
        armed: true,
    };
    let engine = Engine::open_with_mode(&generation, mode)?;
    fs::File::open(&generation)
        .and_then(|file| file.sync_all())
        .map_err(super::super::io_engine_error)?;
    install(directory, selector(&engine, 0)?, None, driver)?;
    custody.armed = false;
    Ok(())
}

pub(super) fn create_compaction_candidate(
    engine: Engine,
    directory: &Path,
    prior: &StoreSelector,
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<(StoreSelector, CompactionStorageObservation)> {
    let source_bytes = fs::metadata(engine.path())
        .map_err(super::super::io_engine_error)?
        .len();
    let required_bytes = source_bytes
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add(8 * 1024 * 1024 + SELECTOR_BYTES as u64))
        .ok_or(EngineError::CounterOverflow)?;
    if driver
        .available_bytes(directory)
        .map_err(super::super::io_engine_error)?
        < required_bytes
    {
        return Err(EngineError::Sqlite {
            kind: crate::SqliteErrorKind::NoSpace,
            message: format!(
                "compaction requires {required_bytes} free bytes for candidate, mark, journal, and selector"
            ),
        });
    }
    let generation = prior
        .generation
        .checked_add(1)
        .ok_or(EngineError::CounterOverflow)?;
    let candidate_path = directory.join(generation_filename(generation));
    let observation = engine.compact_to_observed(&candidate_path)?;
    let candidate = Engine::open(&candidate_path)?;
    let next = selector(&candidate, generation)?;
    Ok((next, observation))
}

struct GenesisCustody<'a> {
    generation: PathBuf,
    current: PathBuf,
    temporary: PathBuf,
    directory: &'a Path,
    driver: &'a dyn StoreGenerationDriver,
    armed: bool,
}

impl Drop for GenesisCustody<'_> {
    fn drop(&mut self) {
        if self.armed && !self.current.exists() {
            let _ = fs::remove_file(&self.temporary);
            let _ = fs::remove_file(&self.generation);
            let mut journal = self.generation.as_os_str().to_os_string();
            journal.push("-journal");
            let _ = fs::remove_file(PathBuf::from(journal));
            let _ = self.driver.sync_directory(self.directory);
        }
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Copy)]
    pub(crate) enum InstallBehavior {
        Native,
        FailBefore,
        FailAfter,
        Advance,
        Record,
    }

    pub(crate) struct TestDriver {
        available: u64,
        install: InstallBehavior,
        fail_sync_call: Option<usize>,
        sync_calls: AtomicUsize,
        substitute_on_remove: bool,
        substituted: AtomicBool,
        calls: Option<Arc<Mutex<Vec<&'static str>>>>,
    }

    impl TestDriver {
        pub(crate) fn native() -> Self {
            Self::new(InstallBehavior::Native)
        }

        pub(crate) fn new(install: InstallBehavior) -> Self {
            Self {
                available: u64::MAX,
                install,
                fail_sync_call: None,
                sync_calls: AtomicUsize::new(0),
                substitute_on_remove: false,
                substituted: AtomicBool::new(false),
                calls: None,
            }
        }

        pub(crate) fn no_space() -> Self {
            Self {
                available: 0,
                ..Self::native()
            }
        }

        pub(crate) fn fail_sync(call: usize) -> Self {
            Self {
                fail_sync_call: Some(call),
                ..Self::native()
            }
        }

        pub(crate) fn substitute_on_remove() -> Self {
            Self {
                substitute_on_remove: true,
                ..Self::native()
            }
        }

        pub(crate) fn recording(calls: Arc<Mutex<Vec<&'static str>>>) -> Self {
            Self {
                install: InstallBehavior::Record,
                calls: Some(calls),
                ..Self::native()
            }
        }
    }

    impl StoreGenerationDriver for TestDriver {
        fn available_bytes(&self, _directory: &Path) -> io::Result<u64> {
            Ok(self.available)
        }

        fn install_selector(&self, prepared: &Path, current: &Path) -> io::Result<()> {
            if let Some(calls) = &self.calls {
                calls.lock().unwrap().push("install");
            }
            match self.install {
                InstallBehavior::FailBefore => {
                    Err(io::Error::other("injected before selector replace"))
                }
                InstallBehavior::Native | InstallBehavior::Record => {
                    if matches!(self.install, InstallBehavior::Native) {
                        fs::rename(prepared, current)?;
                    }
                    Ok(())
                }
                InstallBehavior::FailAfter => {
                    fs::rename(prepared, current)?;
                    Err(io::Error::other("injected lost selector acknowledgement"))
                }
                InstallBehavior::Advance => {
                    fs::rename(prepared, current)?;
                    let directory = current
                        .parent()
                        .ok_or_else(|| io::Error::other("missing selector parent"))?;
                    let selected = super::super::selector::read_selector(current)
                        .map_err(|error| io::Error::other(error.to_string()))?;
                    let next_generation = selected
                        .generation
                        .checked_add(1)
                        .ok_or_else(|| io::Error::other("generation overflow"))?;
                    let next_path = directory.join(generation_filename(next_generation));
                    fs::copy(
                        directory.join(generation_filename(selected.generation)),
                        &next_path,
                    )?;
                    let next_engine =
                        Engine::open_with_mode(&next_path, IntegrityMode::TrustedLocalDev)
                            .map_err(|error| io::Error::other(error.to_string()))?;
                    let next = selector(&next_engine, next_generation)
                        .map_err(|error| io::Error::other(error.to_string()))?;
                    fs::write(directory.join("CURRENT.raced"), next.encode())?;
                    fs::rename(directory.join("CURRENT.raced"), current)
                }
            }
        }

        fn sync_directory(&self, directory: &Path) -> io::Result<()> {
            if let Some(calls) = &self.calls {
                calls.lock().unwrap().push("sync");
            }
            let call = self.sync_calls.fetch_add(1, Ordering::AcqRel);
            if self.fail_sync_call == Some(call) {
                return Err(io::Error::other("injected directory sync"));
            }
            if matches!(self.install, InstallBehavior::Record) {
                Ok(())
            } else {
                fs::File::open(directory)?.sync_all()
            }
        }

        fn file_identity(&self, path: &Path) -> io::Result<Vec<u8>> {
            if self.substitute_on_remove {
                fs::read(path)
            } else {
                NativeGenerationDriver.file_identity(path)
            }
        }

        fn remove_file_if_identity(&self, path: &Path, expected: &[u8]) -> io::Result<()> {
            if self.substitute_on_remove && !self.substituted.swap(true, Ordering::AcqRel) {
                fs::rename(path, path.with_extension("custody-saved"))?;
                fs::write(path, b"substitute")?;
            }
            if self.substitute_on_remove && fs::read(path)? != expected {
                return Err(io::Error::other("identity changed"));
            }
            if !self.substitute_on_remove {
                return NativeGenerationDriver.remove_file_if_identity(path, expected);
            }
            match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        }
    }

    pub(crate) fn directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "layerfs-generation-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{directory, InstallBehavior, TestDriver};
    use super::*;
    use crate::generation::{compact, open_or_create};
    use layerfs_core::{encode_bytes_object, ObjectId};
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    #[test]
    fn port_is_object_safe_and_preserves_install_then_sync_order() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let driver: Box<dyn StoreGenerationDriver> = Box::new(TestDriver::recording(calls.clone()));
        driver
            .install_selector(Path::new("CURRENT.tmp"), Path::new("CURRENT"))
            .unwrap();
        driver.sync_directory(Path::new(".")).unwrap();
        assert_eq!(calls.lock().unwrap().as_slice(), ["install", "sync"]);
    }

    #[test]
    fn definitely_uninstalled_genesis_is_cleaned_in_the_same_call() {
        let directory = directory("failed-genesis");
        assert!(open_or_create(
            &directory,
            &TestDriver::new(InstallBehavior::FailBefore),
            IntegrityMode::Verified,
        )
        .is_err());
        assert!(!directory.join("CURRENT").exists());
        assert!(!directory.join("CURRENT.tmp").exists());
        assert!(!directory.join(generation_filename(0)).exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compaction_preflights_space_before_creating_candidate() {
        let directory = directory("space");
        let engine =
            open_or_create(&directory, &TestDriver::native(), IntegrityMode::Verified).unwrap();
        assert!(matches!(
            compact(engine, &directory, &TestDriver::no_space()),
            Err(EngineError::Sqlite {
                kind: crate::SqliteErrorKind::NoSpace,
                ..
            })
        ));
        assert!(!directory.join(generation_filename(1)).exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compaction_authenticates_unreachable_objects_before_discarding_them() {
        let directory = directory("corrupt-orphan");
        let driver = TestDriver::native();
        let engine = open_or_create(&directory, &driver, IntegrityMode::TrustedLocalDev).unwrap();
        let canonical = encode_bytes_object(b"unreachable").unwrap();
        let id = ObjectId::for_bytes(&canonical);
        engine.put_object_if_absent(id, &canonical).unwrap();
        let selected_path = engine.path().to_owned();
        drop(engine);
        Connection::open(&selected_path)
            .unwrap()
            .execute(
                "UPDATE layerfs_objects SET canonical_bytes = zeroblob(canonical_length) WHERE object_id = ?1",
                rusqlite::params![id.as_bytes().as_slice()],
            )
            .unwrap();
        let before = fs::read(directory.join("CURRENT")).unwrap();
        let engine =
            super::super::selector::open_current(&directory, IntegrityMode::TrustedLocalDev)
                .unwrap();
        assert!(compact(engine, &directory, &driver).is_err());
        assert_eq!(fs::read(directory.join("CURRENT")).unwrap(), before);
        assert!(!directory.join(generation_filename(1)).exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
