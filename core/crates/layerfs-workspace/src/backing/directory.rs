use super::{
    budget::{Budget, Charge},
    segments,
};
use crate::WorkspaceError;
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

pub(crate) struct Directory {
    pub path: Box<Path>,
    pub incarnation: [u8; 32],
    state: Mutex<State>,
    _charge: Charge,
}
struct State {
    file: Option<Arc<File>>,
    created: bool,
    ambiguous: bool,
    identity: Option<(u64, u64)>,
    initializing: bool,
    failure: Option<io::ErrorKind>,
    slot: Option<Slot>,
}
struct Slot(Arc<AtomicUsize>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Directory {
    pub fn reserve(
        path: PathBuf,
        incarnation: [u8; 32],
        budget: &Arc<Budget>,
        slots: Option<&Arc<AtomicUsize>>,
    ) -> Result<Arc<Self>, WorkspaceError> {
        // Covers the retained path, descriptor and bounded native-path scratch.
        let charge = budget.reserve(16384)?;
        let path = path.into_boxed_path();
        let slot = if let Some(slots) = slots {
            slots
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                    (count < 11).then_some(count + 1)
                })
                .map_err(|_| WorkspaceError::Capacity)?;
            Some(Slot(slots.clone()))
        } else {
            None
        };
        Ok(Arc::new(Self {
            path,
            incarnation,
            state: Mutex::new(State {
                file: None,
                created: false,
                ambiguous: false,
                identity: None,
                initializing: false,
                failure: None,
                slot,
            }),
            _charge: charge,
        }))
    }
    pub fn initialize(&self, fresh: bool) -> io::Result<()> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("directory ownership poisoned"))?;
            if state.file.is_some() {
                return Ok(());
            }
            if let Some(kind) = state.failure {
                return Err(io::Error::from(kind));
            }
            if state.initializing {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            state.initializing = true;
            state.ambiguous = fresh;
        }
        let result = (|| {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&self.path) {
                Ok(()) => {
                    let observed = fs::symlink_metadata(&self.path)
                        .ok()
                        .map(|value| identity(&value));
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|_| io::Error::other("directory ownership poisoned"))?;
                    state.created = true;
                    state.ambiguous = false;
                    state.identity = observed;
                }
                Err(error) if !fresh && error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    if fresh && error.kind() == io::ErrorKind::AlreadyExists {
                        self.state
                            .lock()
                            .map_err(|_| io::Error::other("directory ownership poisoned"))?
                            .ambiguous = false;
                    }
                    return Err(error);
                }
            }
            let file = segments::open_directory(&self.path)?;
            let identity = identity(&file.metadata()?);
            let mut state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("directory ownership poisoned"))?;
            state.identity = Some(identity);
            state.file = Some(Arc::new(file));
            Ok(())
        })();
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("directory ownership poisoned"))?;
        state.initializing = false;
        if let Err(error) = &result {
            state.failure = Some(error.kind());
        }
        result
    }
    pub fn file(&self) -> io::Result<Arc<File>> {
        self.state
            .lock()
            .map_err(|_| io::Error::other("directory ownership poisoned"))?
            .file
            .clone()
            .ok_or_else(|| io::Error::other("backing directory is not initialized"))
    }
    pub fn close(&self) -> io::Result<()> {
        let (created, ambiguous, expected, file) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("directory ownership poisoned"))?;
            if state.initializing
                || state
                    .file
                    .as_ref()
                    .is_some_and(|file| Arc::strong_count(file) != 1)
            {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            (
                state.created,
                state.ambiguous,
                state.identity,
                state.file.take(),
            )
        };
        drop(file);
        if !created && ambiguous {
            match fs::symlink_metadata(&self.path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
                Ok(_) => return Err(io::ErrorKind::PermissionDenied.into()),
            }
        }
        if created {
            match fs::symlink_metadata(&self.path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
                Ok(metadata) => {
                    if !metadata.is_dir() || expected != Some(identity(&metadata)) {
                        return Err(io::ErrorKind::PermissionDenied.into());
                    }
                    fs::remove_dir(&self.path)?;
                }
            }
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("directory ownership poisoned"))?;
        state.created = false;
        state.ambiguous = false;
        state.slot = None;
        Ok(())
    }
}
#[cfg(unix)]
pub(crate) fn identity(metadata: &fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}
#[cfg(not(unix))]
pub(crate) fn identity(_: &fs::Metadata) -> (u64, u64) {
    (0, 0)
}
