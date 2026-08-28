use super::*;

pub(super) static TEMP_SERIAL: AtomicU64 = AtomicU64::new(0);
pub(super) const RECOVERY_MARKER: &[u8] = b".layerfs-recovery-v1";
pub(super) const RECOVERY_MAGIC: &[u8] = b"layerfs/apple-recovery/v1\0";

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SetupFault {
    ManagedRootCreated,
    RootOpened,
    RootIdentified,
    StagingCreated,
    StagingOpened,
    StagingIdentified,
    MarkerCreated,
    MarkerWritten,
    MarkerSynced,
    MarkerLocked,
    StagingSynced,
    RootParentSynced,
}

#[cfg(test)]
thread_local! {
    static SETUP_FAULT: Cell<Option<SetupFault>> = const { Cell::new(None) };
}

#[cfg(test)]
pub(super) fn setup_fault(point: SetupFault) -> Result<()> {
    if SETUP_FAULT.get() == Some(point) {
        Err(DriverError::Io(std::io::Error::other(format!(
            "test setup fault after {point:?}"
        ))))
    } else {
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct AppleDriver {
    pub(super) facts: Recorder,
}

#[derive(Clone)]
pub(super) struct Recorder(pub(super) Arc<Mutex<ProjectionFacts>>);

impl Default for Recorder {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(ProjectionFacts::available())))
    }
}

pub(super) struct MarkerWriter<'a> {
    pub(super) file: &'a mut File,
    pub(super) facts: &'a Recorder,
}

impl Write for MarkerWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let start = Instant::now();
        let result = self.file.write(bytes);
        let elapsed = elapsed_ns(start);
        let written = result.as_ref().ok().copied();
        self.facts.update(|facts| {
            finish_write(&mut facts.workspace_marker_write, elapsed, written);
            finish_write(&mut facts.aggregate_native_write, elapsed, written);
        });
        result
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl Recorder {
    pub(super) fn update(&self, update: impl FnOnce(&mut ProjectionFacts)) {
        update(&mut self.0.lock().unwrap_or_else(|poison| poison.into_inner()));
    }

    pub(super) fn snapshot(&self) -> ProjectionFacts {
        *self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

#[derive(Clone, Copy)]
pub(super) enum FileSyncOwner {
    RecoveryMarker,
    ContentTemp,
    PostHardLink,
}

#[derive(Clone, Copy)]
pub(super) enum DirectorySyncOwner {
    Staging,
    RootParent,
    InstallParent,
    DirtyTree,
    FinalRoot,
}

#[derive(Clone, Copy)]
pub(super) enum DirectoryRole {
    Root,
    Tree,
}

pub(super) struct Workspace {
    pub(super) facts: Recorder,
    pub(super) root_dir: File,
    pub(super) root_parent: File,
    pub(super) root_name: Vec<u8>,
    pub(super) root_identity: Vec<u8>,
    pub(super) staging_dir: Option<File>,
    pub(super) staging_parent: File,
    pub(super) staging_name: Vec<u8>,
    pub(super) staging_identity: Vec<u8>,
    pub(super) _recovery_marker: File,
    pub(super) managed: bool,
    pub(super) owned_root: bool,
}
pub(super) struct SetupDirectory {
    pub(super) name: Vec<u8>,
    pub(super) file: Option<File>,
    pub(super) identity: Option<Vec<u8>>,
    pub(super) owned: bool,
}
pub(super) struct SetupCleanup<'a> {
    pub(super) facts: &'a Recorder,
    pub(super) root_parent: Option<File>,
    pub(super) root: SetupDirectory,
    pub(super) staging: Option<SetupDirectory>,
    pub(super) active: bool,
}
pub(super) struct Dir {
    pub(super) file: File,
    pub(super) role: DirectoryRole,
}
pub(super) struct Regular(pub(super) File, pub(super) Recorder);
pub(super) struct Temp {
    pub(super) facts: Recorder,
    pub(super) file: File,
    pub(super) staging: File,
    pub(super) name: Vec<u8>,
    pub(super) identity: Vec<u8>,
    pub(super) expected_metadata: Mutex<Option<NativeMetadata>>,
    pub(super) deferred_flags: u32,
}
pub(super) struct Preflight {
    pub(super) facts: Recorder,
    pub(super) wall_ns: u64,
    pub(super) observed: bool,
    pub(super) directory: File,
    pub(super) staging: File,
    pub(super) name: Vec<u8>,
    pub(super) identity: Vec<u8>,
    pub(super) active: bool,
}
