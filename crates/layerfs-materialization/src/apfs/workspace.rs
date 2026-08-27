use crate::driver::*;
use std::any::Any;
#[cfg(test)]
use std::cell::Cell;
use std::fs::{self, File, FileTimes};
use std::io::{Read, Seek, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static TEMP_SERIAL: AtomicU64 = AtomicU64::new(0);
const RECOVERY_MARKER: &[u8] = b".layerfs-recovery-v1";
const RECOVERY_MAGIC: &[u8] = b"layerfs/apple-recovery/v1\0";

#[cfg(feature = "test-hooks")]
std::thread_local! {
    static FORCE_CLONE_UNSUPPORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub fn inject_clone_unsupported_for_test() {
    FORCE_CLONE_UNSUPPORTED.set(true);
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetupFault {
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
fn setup_fault(point: SetupFault) -> Result<()> {
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
    facts: Recorder,
}

#[derive(Clone)]
struct Recorder(Arc<Mutex<ProjectionFacts>>);

impl Default for Recorder {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(ProjectionFacts::available())))
    }
}

struct MarkerWriter<'a> {
    file: &'a mut File,
    facts: &'a Recorder,
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
    fn update(&self, update: impl FnOnce(&mut ProjectionFacts)) {
        update(&mut self.0.lock().unwrap_or_else(|poison| poison.into_inner()));
    }

    fn snapshot(&self) -> ProjectionFacts {
        *self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

#[derive(Clone, Copy)]
enum FileSyncOwner {
    RecoveryMarker,
    ContentTemp,
    PostHardLink,
}

#[derive(Clone, Copy)]
enum DirectorySyncOwner {
    Staging,
    RootParent,
    InstallParent,
    DirtyTree,
    FinalRoot,
}

#[derive(Clone, Copy)]
enum DirectoryRole {
    Root,
    Tree,
}

struct Workspace {
    facts: Recorder,
    root_dir: File,
    root_parent: File,
    root_name: Vec<u8>,
    root_identity: Vec<u8>,
    staging_dir: Option<File>,
    staging_parent: File,
    staging_name: Vec<u8>,
    staging_identity: Vec<u8>,
    _recovery_marker: File,
    managed: bool,
    owned_root: bool,
}
struct SetupDirectory {
    name: Vec<u8>,
    file: Option<File>,
    identity: Option<Vec<u8>>,
    owned: bool,
}
struct SetupCleanup<'a> {
    facts: &'a Recorder,
    root_parent: Option<File>,
    root: SetupDirectory,
    staging: Option<SetupDirectory>,
    active: bool,
}
struct Dir {
    file: File,
    role: DirectoryRole,
}
struct Regular(File, Recorder);
struct Temp {
    facts: Recorder,
    file: File,
    staging: File,
    name: Vec<u8>,
    identity: Vec<u8>,
    expected_metadata: Mutex<Option<NativeMetadata>>,
    deferred_flags: u32,
}
struct Preflight {
    facts: Recorder,
    wall_ns: u64,
    observed: bool,
    directory: File,
    staging: File,
    name: Vec<u8>,
    identity: Vec<u8>,
    active: bool,
}

impl Drop for SetupCleanup<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let Some(root_parent) = self.root_parent.as_ref() else {
            return;
        };
        if let Some(staging) = self.staging.as_mut().filter(|staging| staging.owned) {
            cleanup_setup_directory(self.facts, root_parent, staging);
        }
        if self.root.owned {
            cleanup_setup_directory(self.facts, root_parent, &mut self.root);
        }
    }
}

fn cleanup_setup_directory(facts: &Recorder, parent: &File, entry: &mut SetupDirectory) {
    let start = Instant::now();
    let removed = (|| {
        let expected = entry
            .identity
            .as_deref()
            .ok_or_else(|| std::io::Error::from_raw_os_error(libc::ESTALE))?;
        if entry.file.is_none() {
            entry.file = Some(super::ffi::open_directory_at(parent, &entry.name)?);
        }
        let directory = entry.file.as_ref().expect("setup directory just opened");
        if super::ffi::file_stable_token(directory)? != expected {
            return Err(std::io::Error::from_raw_os_error(libc::ESTALE));
        }
        super::ffi::remove_owned_tree(directory, parent, &entry.name, expected)
    })();
    finish_cleanup(facts, start, removed.is_ok());
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn finish_call(call: &mut ProjectionCallFacts, elapsed: u64, success: bool) {
    call.attempts = call.attempts.saturating_add(1);
    if success {
        call.successes = call.successes.saturating_add(1);
    } else {
        call.failures = call.failures.saturating_add(1);
    }
    call.wall.nanoseconds = call.wall.nanoseconds.saturating_add(elapsed);
}

fn observed_call<T>(
    facts: &Recorder,
    select: fn(&mut ProjectionFacts) -> &mut ProjectionCallFacts,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let start = Instant::now();
    let result = operation();
    let elapsed = elapsed_ns(start);
    facts.update(|facts| finish_call(select(facts), elapsed, result.is_ok()));
    result
}

fn finish_write(write: &mut ProjectionWriteFacts, elapsed: u64, written: Option<usize>) {
    write.attempts = write.attempts.saturating_add(1);
    write.wall.nanoseconds = write.wall.nanoseconds.saturating_add(elapsed);
    match written {
        Some(bytes) => {
            write.successes = write.successes.saturating_add(1);
            write.bytes = write.bytes.saturating_add(bytes as u64);
        }
        None => write.failures = write.failures.saturating_add(1),
    }
}

fn increment_class(counts: &mut DurabilityClassCounts, class: DurabilityClass) {
    match class {
        DurabilityClass::ProcessCrashReconciled => {
            counts.process_crash_reconciled = counts.process_crash_reconciled.saturating_add(1)
        }
        DurabilityClass::HostCrashOrdered => {
            counts.host_crash_ordered = counts.host_crash_ordered.saturating_add(1)
        }
        DurabilityClass::DeviceFlushRequested => {
            counts.device_flush_requested = counts.device_flush_requested.saturating_add(1)
        }
        DurabilityClass::PowerLossQualified => {
            counts.power_loss_qualified = counts.power_loss_qualified.saturating_add(1)
        }
    }
}

fn finish_sync(sync: &mut ProjectionSyncFacts, elapsed: u64, success: bool) {
    sync.attempts = sync.attempts.saturating_add(1);
    increment_class(&mut sync.requested, DurabilityClass::ProcessCrashReconciled);
    if success {
        sync.successes = sync.successes.saturating_add(1);
        increment_class(&mut sync.achieved, DurabilityClass::ProcessCrashReconciled);
    } else {
        sync.failures = sync.failures.saturating_add(1);
    }
    sync.wall.nanoseconds = sync.wall.nanoseconds.saturating_add(elapsed);
}

fn sync_file(file: &File, facts: &Recorder, owner: FileSyncOwner) -> Result<()> {
    let start = Instant::now();
    let result = file.sync_all();
    let elapsed = elapsed_ns(start);
    facts.update(|facts| {
        finish_sync(&mut facts.regular_file_sync, elapsed, result.is_ok());
        let owner = match owner {
            FileSyncOwner::RecoveryMarker => &mut facts.recovery_marker_file_sync,
            FileSyncOwner::ContentTemp => &mut facts.content_temp_file_sync,
            FileSyncOwner::PostHardLink => &mut facts.post_hardlink_file_sync,
        };
        finish_sync(owner, elapsed, result.is_ok());
    });
    result.map_err(Into::into)
}

fn sync_directory_file_io(
    file: &File,
    facts: &Recorder,
    owner: DirectorySyncOwner,
) -> std::io::Result<()> {
    let start = Instant::now();
    let result = file.sync_all();
    let elapsed = elapsed_ns(start);
    facts.update(|facts| {
        finish_sync(&mut facts.directory_sync, elapsed, result.is_ok());
        let owner = match owner {
            DirectorySyncOwner::Staging => &mut facts.staging_directory_sync,
            DirectorySyncOwner::RootParent => &mut facts.root_parent_directory_sync,
            DirectorySyncOwner::InstallParent => &mut facts.install_parent_directory_sync,
            DirectorySyncOwner::DirtyTree => &mut facts.dirty_tree_directory_sync,
            DirectorySyncOwner::FinalRoot => &mut facts.final_root_directory_sync,
        };
        finish_sync(owner, elapsed, result.is_ok());
    });
    result
}

fn sync_directory_file(file: &File, facts: &Recorder, owner: DirectorySyncOwner) -> Result<()> {
    sync_directory_file_io(file, facts, owner).map_err(Into::into)
}

fn metadata_value_bytes(metadata: &NativeMetadata) -> u64 {
    metadata.xattrs.payload_bytes() as u64 + metadata.acl.as_ref().map_or(0, |acl| acl.len() as u64)
}

fn write_metadata_values(file: &File, metadata: &NativeMetadata, facts: &Recorder) -> Result<()> {
    let start = Instant::now();
    let result = super::metadata::write(file, metadata);
    let elapsed = elapsed_ns(start);
    let bytes = metadata_value_bytes(metadata);
    facts.update(|facts| {
        let written = result
            .is_ok()
            .then_some(usize::try_from(bytes).unwrap_or(usize::MAX));
        finish_write(&mut facts.metadata_value_write, elapsed, written);
        finish_write(&mut facts.aggregate_native_write, elapsed, written);
    });
    result
}

fn metadata_apply_step(elapsed: &mut u64, operation: impl FnOnce() -> Result<()>) -> Result<()> {
    let start = Instant::now();
    let result = operation();
    *elapsed = elapsed.saturating_add(elapsed_ns(start));
    result
}

fn finish_cleanup(facts: &Recorder, start: Instant, success: bool) {
    let elapsed = elapsed_ns(start);
    facts.update(|facts| {
        facts.cleanup.attempts = facts.cleanup.attempts.saturating_add(1);
        if success {
            facts.cleanup.successes = facts.cleanup.successes.saturating_add(1);
        } else {
            facts.cleanup.failures = facts.cleanup.failures.saturating_add(1);
            facts.cleanup.residue = facts.cleanup.residue.saturating_add(1);
        }
        facts.cleanup.wall.nanoseconds = facts.cleanup.wall.nanoseconds.saturating_add(elapsed);
    });
}

fn finish_replace(facts: &Recorder, start: Instant, prior_existed: bool, result: &Result<()>) {
    let elapsed = elapsed_ns(start);
    facts.update(|facts| {
        facts.replace.attempts = facts.replace.attempts.saturating_add(1);
        facts.replace.wall.nanoseconds = facts.replace.wall.nanoseconds.saturating_add(elapsed);
        if prior_existed {
            facts.replace.prior_visible = facts.replace.prior_visible.saturating_add(1);
        }
        match result {
            Ok(()) => {
                facts.replace.successes = facts.replace.successes.saturating_add(1);
                facts.replace.requested_visible = facts.replace.requested_visible.saturating_add(1);
            }
            Err(DriverError::DurabilityAmbiguous) => {
                facts.replace.failures = facts.replace.failures.saturating_add(1);
                facts.replace.requested_visible = facts.replace.requested_visible.saturating_add(1);
                facts.replace.durability_ambiguous =
                    facts.replace.durability_ambiguous.saturating_add(1);
            }
            Err(DriverError::VisibilityAmbiguous) => {
                facts.replace.failures = facts.replace.failures.saturating_add(1);
                facts.replace.visibility_ambiguous =
                    facts.replace.visibility_ambiguous.saturating_add(1);
            }
            Err(_) => facts.replace.failures = facts.replace.failures.saturating_add(1),
        }
    });
}

fn record_replace_durability_ambiguity<T>(facts: &Recorder, result: &Result<T>) {
    if matches!(result, Err(DriverError::DurabilityAmbiguous)) {
        facts.update(|facts| {
            facts.replace.durability_ambiguous =
                facts.replace.durability_ambiguous.saturating_add(1)
        });
    }
}

impl NamePreflight for Preflight {
    fn add(&mut self, name: &[u8]) -> Result<()> {
        let started = Instant::now();
        let result = super::ffi::create_regular_at(&self.directory, name);
        self.wall_ns = self.wall_ns.saturating_add(elapsed_ns(started));
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                if !self.observed {
                    self.facts.update(|facts| {
                        finish_call(&mut facts.name_preflight, self.wall_ns, false)
                    });
                    self.observed = true;
                }
                Err(error.into())
            }
        }
    }
    fn finish(mut self: Box<Self>) -> Result<()> {
        let cleanup_start = Instant::now();
        let result = super::ffi::remove_owned_tree(
            &self.directory,
            &self.staging,
            &self.name,
            &self.identity,
        );
        finish_cleanup(&self.facts, cleanup_start, result.is_ok());
        if !self.observed {
            self.facts.update(|facts| {
                finish_call(&mut facts.name_preflight, self.wall_ns, result.is_ok())
            });
            self.observed = true;
        }
        if result.is_ok() {
            self.active = false;
        }
        result.map_err(Into::into)
    }
}

impl Drop for Preflight {
    fn drop(&mut self) {
        if !self.observed {
            self.facts
                .update(|facts| finish_call(&mut facts.name_preflight, self.wall_ns, false));
            self.observed = true;
        }
        if self.active {
            let start = Instant::now();
            let removed = super::ffi::remove_owned_tree(
                &self.directory,
                &self.staging,
                &self.name,
                &self.identity,
            );
            finish_cleanup(&self.facts, start, removed.is_ok());
        }
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let start = Instant::now();
        let removed = super::ffi::unlink_if_identity_at(&self.staging, &self.name, &self.identity);
        finish_cleanup(&self.facts, start, removed.is_ok());
    }
}

impl DirectoryHandle for Dir {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl Read for Regular {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(bytes)
    }
}
impl Write for Regular {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let start = Instant::now();
        let result = self.0.write(bytes);
        let elapsed = elapsed_ns(start);
        let written = result.as_ref().ok().copied();
        self.1.update(|facts| {
            finish_write(&mut facts.content_write, elapsed, written);
            finish_write(&mut facts.aggregate_native_write, elapsed, written);
        });
        result
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let start = Instant::now();
        let result = self.0.flush();
        let elapsed = elapsed_ns(start);
        self.1
            .update(|facts| finish_call(&mut facts.content_flush, elapsed, result.is_ok()));
        result
    }
}
impl Seek for Regular {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.seek(position)
    }
}
impl RegularFileHandle for Regular {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl Read for Temp {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(bytes)
    }
}
impl Write for Temp {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let start = Instant::now();
        let result = self.file.write(bytes);
        let elapsed = elapsed_ns(start);
        let written = result.as_ref().ok().copied();
        self.facts.update(|facts| {
            finish_write(&mut facts.content_write, elapsed, written);
            finish_write(&mut facts.aggregate_native_write, elapsed, written);
        });
        result
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let start = Instant::now();
        let result = self.file.flush();
        let elapsed = elapsed_ns(start);
        self.facts
            .update(|facts| finish_call(&mut facts.content_flush, elapsed, result.is_ok()));
        result
    }
}
impl Seek for Temp {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.file.seek(position)
    }
}
impl OwnedTempHandle for Temp {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn set_len(&mut self, len: u64) -> Result<()> {
        self.file.set_len(len).map_err(Into::into)
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl ProjectionDriver for AppleDriver {
    fn projection_facts(&self) -> ProjectionFacts {
        self.facts.snapshot()
    }

    fn open_workspace(
        &self,
        path: &Path,
        store_id: [u8; 32],
    ) -> Result<Box<dyn ProjectionWorkspace>> {
        let setup_start = Instant::now();
        let result = (|| {
            let root_start = Instant::now();
            let root_result = (|| {
                let parent_path = path.parent().unwrap_or_else(|| Path::new("."));
                let root_parent = super::ffi::open_directory_path_nofollow(parent_path)?;
                let root_name = path
                    .file_name()
                    .ok_or(DriverError::Conflict)?
                    .as_bytes()
                    .to_vec();
                let mut cleanup = SetupCleanup {
                    facts: &self.facts,
                    root_parent: Some(root_parent),
                    root: SetupDirectory {
                        name: root_name,
                        file: None,
                        identity: None,
                        owned: false,
                    },
                    staging: None,
                    active: true,
                };
                let parent = cleanup
                    .root_parent
                    .as_ref()
                    .expect("root parent is present");
                let root_dir = match super::ffi::open_directory_at(parent, &cleanup.root.name) {
                    Ok(file) => file,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        super::ffi::mkdir_at(parent, &cleanup.root.name)?;
                        super::ffi::open_directory_at(parent, &cleanup.root.name)?
                    }
                    Err(error) => return Err(error.into()),
                };
                cleanup.root.file = Some(root_dir);
                #[cfg(test)]
                setup_fault(SetupFault::RootOpened)?;
                let root_identity = super::ffi::file_stable_token(
                    cleanup.root.file.as_ref().expect("root directory is open"),
                )?;
                if cleanup
                    .root
                    .identity
                    .as_deref()
                    .is_some_and(|expected| expected != root_identity)
                {
                    return Err(std::io::Error::from_raw_os_error(libc::ESTALE).into());
                }
                cleanup.root.identity = Some(root_identity);
                #[cfg(test)]
                setup_fault(SetupFault::RootIdentified)?;
                Ok::<_, DriverError>(cleanup)
            })();
            let root_elapsed = elapsed_ns(root_start);
            self.facts.update(|facts| {
                finish_call(
                    &mut facts.workspace_root_create_open,
                    root_elapsed,
                    root_result.is_ok(),
                )
            });
            let mut setup_cleanup = root_result?;
            let staging_start = Instant::now();
            let staging_result = (|| {
                for _ in 0..64 {
                    let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
                    let name =
                        format!(".layerfs-staging-{}-{serial}", std::process::id()).into_bytes();
                    match super::ffi::mkdir_at(
                        setup_cleanup
                            .root_parent
                            .as_ref()
                            .expect("root parent is present"),
                        &name,
                    ) {
                        Ok(()) => {
                            setup_cleanup.staging = Some(SetupDirectory {
                                name,
                                file: None,
                                identity: None,
                                owned: true,
                            });
                            let identity = super::ffi::stable_token_at(
                                setup_cleanup
                                    .root_parent
                                    .as_ref()
                                    .expect("root parent is present"),
                                &setup_cleanup
                                    .staging
                                    .as_ref()
                                    .expect("staging ownership was recorded")
                                    .name,
                            )?;
                            setup_cleanup
                                .staging
                                .as_mut()
                                .expect("staging ownership was recorded")
                                .identity = Some(identity);
                            #[cfg(test)]
                            setup_fault(SetupFault::StagingCreated)?;
                            let staging = setup_cleanup
                                .staging
                                .as_mut()
                                .expect("staging ownership was recorded");
                            staging.file = Some(super::ffi::open_directory_at(
                                setup_cleanup
                                    .root_parent
                                    .as_ref()
                                    .expect("root parent is present"),
                                &staging.name,
                            )?);
                            #[cfg(test)]
                            setup_fault(SetupFault::StagingOpened)?;
                            let actual = super::ffi::file_stable_token(
                                staging.file.as_ref().expect("staging directory is open"),
                            )?;
                            if staging.identity.as_deref() != Some(actual.as_slice()) {
                                return Err(std::io::Error::from_raw_os_error(libc::ESTALE).into());
                            }
                            #[cfg(test)]
                            setup_fault(SetupFault::StagingIdentified)?;
                            return Ok::<_, DriverError>(());
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(DriverError::Conflict)
            })();
            let staging_elapsed = elapsed_ns(staging_start);
            self.facts.update(|facts| {
                finish_call(
                    &mut facts.staging_create_open,
                    staging_elapsed,
                    staging_result.is_ok(),
                )
            });
            staging_result?;
            let staging_dir = setup_cleanup
                .staging
                .as_ref()
                .and_then(|staging| staging.file.as_ref())
                .expect("successful staging setup has an open directory");
            staging_dir.set_permissions(fs::Permissions::from_mode(0o700))?;
            let marker_start = Instant::now();
            let marker_result = super::ffi::create_regular_at(staging_dir, RECOVERY_MARKER);
            let marker_elapsed = elapsed_ns(marker_start);
            self.facts.update(|facts| {
                finish_call(
                    &mut facts.recovery_marker_create,
                    marker_elapsed,
                    marker_result.is_ok(),
                )
            });
            let mut recovery_marker = marker_result?;
            #[cfg(test)]
            setup_fault(SetupFault::MarkerCreated)?;
            recovery_marker.set_permissions(fs::Permissions::from_mode(0o600))?;
            MarkerWriter {
                file: &mut recovery_marker,
                facts: &self.facts,
            }
            .write_all(&encode_recovery_record(
                store_id,
                false,
                &setup_cleanup.root.name,
                setup_cleanup
                    .root
                    .identity
                    .as_ref()
                    .expect("successful root setup has an identity"),
            ))?;
            #[cfg(test)]
            setup_fault(SetupFault::MarkerWritten)?;
            sync_file(&recovery_marker, &self.facts, FileSyncOwner::RecoveryMarker)?;
            #[cfg(test)]
            setup_fault(SetupFault::MarkerSynced)?;
            if !super::ffi::try_lock_exclusive(&recovery_marker)? {
                return Err(DriverError::Conflict);
            }
            #[cfg(test)]
            setup_fault(SetupFault::MarkerLocked)?;
            sync_directory_file(staging_dir, &self.facts, DirectorySyncOwner::Staging)?;
            #[cfg(test)]
            setup_fault(SetupFault::StagingSynced)?;
            sync_directory_file(
                setup_cleanup
                    .root_parent
                    .as_ref()
                    .expect("root parent is present"),
                &self.facts,
                DirectorySyncOwner::RootParent,
            )?;
            #[cfg(test)]
            setup_fault(SetupFault::RootParentSynced)?;
            let workspace_root_parent = setup_cleanup
                .root_parent
                .as_ref()
                .expect("root parent is present")
                .try_clone()?;
            let root_dir = setup_cleanup
                .root
                .file
                .take()
                .expect("successful root setup has an open directory");
            let root_name = std::mem::take(&mut setup_cleanup.root.name);
            let root_identity = setup_cleanup
                .root
                .identity
                .take()
                .expect("successful root setup has an identity");
            let mut staging = setup_cleanup
                .staging
                .take()
                .expect("successful setup has staging ownership");
            let staging_dir = staging
                .file
                .take()
                .expect("successful staging setup has an open directory");
            let staging_name = staging.name;
            let staging_identity = staging
                .identity
                .take()
                .expect("successful staging setup has an identity");
            let root_parent = setup_cleanup
                .root_parent
                .take()
                .expect("root parent is present");
            setup_cleanup.active = false;
            drop(setup_cleanup);
            Ok::<_, DriverError>(Box::new(Workspace {
                facts: self.facts.clone(),
                root_dir,
                root_parent: workspace_root_parent,
                root_name,
                root_identity,
                staging_dir: Some(staging_dir),
                staging_parent: root_parent,
                staging_name,
                staging_identity,
                _recovery_marker: recovery_marker,
                managed: false,
                owned_root: false,
            }) as Box<dyn ProjectionWorkspace>)
        })();
        let setup_elapsed = elapsed_ns(setup_start);
        self.facts
            .update(|facts| finish_call(&mut facts.workspace_setup, setup_elapsed, result.is_ok()));
        result
    }

    fn recover_owned_workspaces(&self, parent: &Path, store_id: [u8; 32]) -> Result<()> {
        recover_owned_workspaces(parent, store_id, &self.facts)
    }
}

impl ProjectionWorkspace for Workspace {
    fn projection_facts(&self) -> ProjectionFacts {
        self.facts.snapshot()
    }

    fn root_directory(&self) -> Result<Box<dyn DirectoryHandle>> {
        Ok(Box::new(Dir {
            file: self.root_dir.try_clone()?,
            role: DirectoryRole::Root,
        }))
    }

    fn enumerate_at<'a>(
        &'a self,
        parent: &'a dyn DirectoryHandle,
    ) -> Result<Box<dyn Iterator<Item = Result<NativeEntry>> + 'a>> {
        let parent = dir(parent)?;
        Ok(Box::new(super::ffi::directory_entries(&parent.file)?.map(
            |entry| {
                let (name, kind, link_count, token, stable) = entry?;
                Ok(NativeEntry {
                    name,
                    kind,
                    token,
                    hard_link_key: (kind == NativeKind::RegularFile).then_some(stable),
                    link_count,
                })
            },
        )))
    }

    fn open_directory_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn DirectoryHandle>> {
        let parent = dir(parent)?;
        let file = super::ffi::open_directory_at(&parent.file, name)?;
        validate_expected(&file, expected)?;
        Ok(Box::new(Dir {
            file,
            role: DirectoryRole::Tree,
        }))
    }
    fn duplicate_directory(
        &self,
        directory: &dyn DirectoryHandle,
    ) -> Result<Box<dyn DirectoryHandle>> {
        let directory = dir(directory)?;
        Ok(Box::new(Dir {
            file: directory.file.try_clone()?,
            role: directory.role,
        }))
    }
    fn directory_token(&self, directory: &dyn DirectoryHandle) -> Result<Vec<u8>> {
        Ok(super::ffi::file_token(&dir(directory)?.file)?)
    }
    fn directory_identity(&self, directory: &dyn DirectoryHandle) -> Result<Vec<u8>> {
        Ok(super::ffi::file_stable_token(&dir(directory)?.file)?)
    }
    fn revalidate_root_binding(&self) -> Result<()> {
        let start = Instant::now();
        let result = (|| {
            if super::ffi::stable_token_at(&self.root_parent, &self.root_name)?
                != super::ffi::file_stable_token(&self.root_dir)?
            {
                return Err(DriverError::Conflict);
            }
            Ok(())
        })();
        let elapsed = elapsed_ns(start);
        self.facts.update(|facts| {
            finish_call(&mut facts.root_binding_revalidate, elapsed, result.is_ok());
            finish_call(&mut facts.authority_completion, elapsed, result.is_ok());
        });
        result
    }
    fn begin_name_preflight(&self) -> Result<Box<dyn NamePreflight>> {
        let started = Instant::now();
        let result = (|| {
            let staging = self.staging_dir.as_ref().ok_or(DriverError::Conflict)?;
            let name = format!(
                "preflight-{}-{}",
                std::process::id(),
                TEMP_SERIAL.fetch_add(1, Ordering::Relaxed)
            )
            .into_bytes();
            super::ffi::mkdir_at(staging, &name)?;
            let directory = super::ffi::open_directory_at(staging, &name)?;
            let identity = super::ffi::file_stable_token(&directory)?;
            Ok::<_, DriverError>(Box::new(Preflight {
                facts: self.facts.clone(),
                wall_ns: elapsed_ns(started),
                observed: false,
                directory,
                staging: staging.try_clone()?,
                name,
                identity,
                active: true,
            }) as Box<dyn NamePreflight>)
        })();
        if result.is_err() {
            let elapsed = elapsed_ns(started);
            self.facts
                .update(|facts| finish_call(&mut facts.name_preflight, elapsed, false));
        }
        result
    }
    fn open_regular_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn RegularFileHandle>> {
        let file = super::ffi::open_regular_at(&dir(parent)?.file, name, self.managed)?;
        validate_expected(&file, expected)?;
        Ok(Box::new(Regular(file, self.facts.clone())))
    }
    fn open_regular_read_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn RegularFileHandle>> {
        let file = super::ffi::open_regular_at(&dir(parent)?.file, name, false)?;
        validate_expected(&file, expected)?;
        Ok(Box::new(Regular(file, self.facts.clone())))
    }
    fn set_regular_len(&self, file: &mut dyn RegularFileHandle, len: u64) -> Result<()> {
        file.as_any()
            .downcast_ref::<Regular>()
            .ok_or(DriverError::Conflict)?
            .0
            .set_len(len)?;
        Ok(())
    }
    fn sync_regular(&self, file: &mut dyn RegularFileHandle) -> Result<()> {
        let file = file
            .as_any()
            .downcast_ref::<Regular>()
            .ok_or(DriverError::Conflict)?;
        let start = Instant::now();
        let result = file.0.sync_all();
        let elapsed = elapsed_ns(start);
        self.facts
            .update(|facts| finish_sync(&mut facts.regular_file_sync, elapsed, result.is_ok()));
        result.map_err(Into::into)
    }
    fn read_link_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        let parent = &dir(parent)?.file;
        validate_entry_expected(parent, name, expected)?;
        let target = super::ffi::read_link_at(parent, name)?;
        validate_entry_expected(parent, name, expected)?;
        Ok(target)
    }
    fn read_metadata_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<NativeMetadata> {
        let entry = super::ffi::open_entry_at(&dir(parent)?.file, name)?;
        validate_expected(&entry, expected)?;
        let metadata = super::metadata::read(&entry)?;
        validate_expected(&entry, expected)?;
        Ok(metadata)
    }
    fn token_at(&self, parent: &dyn DirectoryHandle, name: &[u8]) -> Result<Vec<u8>> {
        Ok(super::ffi::token_at(&dir(parent)?.file, name)?)
    }
    fn identity_at(&self, parent: &dyn DirectoryHandle, name: &[u8]) -> Result<Vec<u8>> {
        Ok(super::ffi::stable_token_at(&dir(parent)?.file, name)?)
    }
    fn read_root_metadata(&self) -> Result<NativeMetadata> {
        super::metadata::read(&self.root_dir)
    }
    fn read_directory_metadata(&self, directory: &dyn DirectoryHandle) -> Result<NativeMetadata> {
        super::metadata::read(&dir(directory)?.file)
    }
    fn create_directory_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
    ) -> Result<Box<dyn DirectoryHandle>> {
        let parent = dir(parent)?;
        super::ffi::mkdir_at(&parent.file, name)?;
        match super::ffi::open_directory_at(&parent.file, name) {
            Ok(file) => Ok(Box::new(Dir {
                file,
                role: DirectoryRole::Tree,
            })),
            Err(_) => Err(DriverError::VisibilityAmbiguous),
        }
    }
    fn create_temp_at(&self, _parent: &dyn DirectoryHandle) -> Result<Box<dyn OwnedTempHandle>> {
        for _ in 0..64 {
            let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
            let name = format!("temp-{}-{serial}", std::process::id()).into_bytes();
            let staging = self.staging_dir.as_ref().ok_or(DriverError::Conflict)?;
            let start = Instant::now();
            let created = super::ffi::create_regular_at(staging, &name);
            let elapsed = elapsed_ns(start);
            self.facts
                .update(|facts| finish_call(&mut facts.temp_create, elapsed, created.is_ok()));
            match created {
                Ok(file) => {
                    let identity = super::ffi::file_stable_token(&file)?;
                    return Ok(Box::new(Temp {
                        facts: self.facts.clone(),
                        file,
                        staging: staging.try_clone()?,
                        name,
                        identity,
                        expected_metadata: Mutex::new(None),
                        deferred_flags: 0,
                    }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(DriverError::Conflict)
    }
    fn clone_temp_from_regular(
        &self,
        source: &dyn RegularFileHandle,
    ) -> Result<Box<dyn OwnedTempHandle>> {
        let source = source
            .as_any()
            .downcast_ref::<Regular>()
            .ok_or(DriverError::Conflict)?;
        #[cfg(feature = "test-hooks")]
        if FORCE_CLONE_UNSUPPORTED.replace(false) {
            return Err(DriverError::Unsupported);
        }
        if source.0.metadata()?.nlink() != 1 {
            return Err(DriverError::Unsupported);
        }
        let metadata = super::metadata::read(&source.0)?;
        let staging = self.staging_dir.as_ref().ok_or(DriverError::Conflict)?;
        for _ in 0..64 {
            let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
            let name = format!("clone-{}-{serial}", std::process::id()).into_bytes();
            let start = Instant::now();
            let created = super::ffi::clone_file_at(&source.0, staging, &name);
            let elapsed = elapsed_ns(start);
            self.facts
                .update(|facts| finish_call(&mut facts.temp_create, elapsed, created.is_ok()));
            match created {
                Ok(file) => {
                    let identity = super::ffi::file_stable_token(&file)?;
                    super::ffi::set_flags_file(&file, 0)?;
                    return Ok(Box::new(Temp {
                        facts: self.facts.clone(),
                        file,
                        staging: staging.try_clone()?,
                        name,
                        identity,
                        expected_metadata: Mutex::new(None),
                        deferred_flags: metadata.bsd_flags,
                    }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) if matches!(error.raw_os_error(), Some(code) if code == libc::ENOTSUP || code == libc::EXDEV || code == libc::EOPNOTSUPP) => {
                    return Err(DriverError::Unsupported)
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(DriverError::Conflict)
    }
    fn read_temp_metadata(&self, temp: &dyn OwnedTempHandle) -> Result<NativeMetadata> {
        let temp = temp
            .as_any()
            .downcast_ref::<Temp>()
            .ok_or(DriverError::Conflict)?;
        let mut metadata = super::metadata::read(&temp.file)?;
        metadata.bsd_flags = temp.deferred_flags;
        Ok(metadata)
    }
    fn set_temp_metadata(
        &self,
        temp: &mut dyn OwnedTempHandle,
        metadata: &NativeMetadata,
    ) -> Result<()> {
        let temp = temp
            .as_any()
            .downcast_ref::<Temp>()
            .ok_or(DriverError::Conflict)?;
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_validate,
            || super::metadata::preflight(&temp.file, metadata),
        )?;
        let mut apply_elapsed = 0;
        let applied: Result<()> = (|| {
            metadata_apply_step(&mut apply_elapsed, || {
                temp.file
                    .set_permissions(fs::Permissions::from_mode(metadata.mode))
                    .map_err(Into::into)
            })?;
            write_metadata_values(&temp.file, metadata, &self.facts)?;
            let modified = modified_time(metadata)?;
            metadata_apply_step(&mut apply_elapsed, || {
                temp.file
                    .set_times(FileTimes::new().set_modified(modified))
                    .map_err(Into::into)
            })?;
            Ok(())
        })();
        self.facts
            .update(|facts| finish_call(&mut facts.metadata_apply, apply_elapsed, applied.is_ok()));
        applied?;
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_preinstall_verify,
            || super::metadata::verify_before_install(&temp.file, metadata),
        )?;
        *temp
            .expected_metadata
            .lock()
            .map_err(|_| DriverError::Conflict)? = Some(metadata.clone());
        Ok(())
    }
    fn set_entry_metadata(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()> {
        let parent = &dir(parent)?.file;
        if super::ffi::stable_token_at(parent, name)? != expected {
            return Err(DriverError::Conflict);
        }
        let entry = super::ffi::open_entry_at(parent, name)?;
        if super::ffi::file_stable_token(&entry)? != expected {
            return Err(DriverError::Conflict);
        }
        let native = entry.metadata()?;
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_validate,
            || super::metadata::preflight(&entry, metadata),
        )?;
        if native.file_type().is_symlink() {
            let mut apply_elapsed = 0;
            let applied: Result<()> = (|| {
                write_metadata_values(&entry, metadata, &self.facts)?;
                metadata_apply_step(&mut apply_elapsed, || {
                    super::ffi::set_symlink_mtime_at(
                        parent,
                        name,
                        metadata.mtime_seconds,
                        metadata.mtime_nanoseconds,
                    )
                    .map_err(Into::into)
                })?;
                Ok(())
            })();
            self.facts.update(|facts| {
                finish_call(&mut facts.metadata_apply, apply_elapsed, applied.is_ok())
            });
            applied?;
            return observed_call(
                &self.facts,
                |facts| &mut facts.metadata_postinstall_verify,
                || super::metadata::finish(&entry, metadata),
            );
        }
        let mut apply_elapsed = 0;
        let applied: Result<()> = (|| {
            metadata_apply_step(&mut apply_elapsed, || {
                entry
                    .set_permissions(fs::Permissions::from_mode(metadata.mode))
                    .map_err(Into::into)
            })?;
            write_metadata_values(&entry, metadata, &self.facts)?;
            let modified = modified_time(metadata)?;
            metadata_apply_step(&mut apply_elapsed, || {
                entry
                    .set_times(FileTimes::new().set_modified(modified))
                    .map_err(Into::into)
            })?;
            Ok(())
        })();
        self.facts
            .update(|facts| finish_call(&mut facts.metadata_apply, apply_elapsed, applied.is_ok()));
        applied?;
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_postinstall_verify,
            || super::metadata::finish(&entry, metadata),
        )
    }
    fn atomic_replace(
        &self,
        temp: Box<dyn OwnedTempHandle>,
        parent: &dyn DirectoryHandle,
        name: &[u8],
    ) -> Result<()> {
        let temp = temp
            .into_any()
            .downcast::<Temp>()
            .map_err(|_| DriverError::Conflict)?;
        atomic_replace_temp(
            temp,
            dir(parent)?,
            name,
            None,
            DirectoryDurability::ImmediateDirectoryDurability,
            &self.facts,
        )
        .map(drop)
    }
    fn atomic_replace_with_directory_durability(
        &self,
        temp: Box<dyn OwnedTempHandle>,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        requested: DirectoryDurability,
    ) -> Result<DirectoryDurability> {
        let temp = temp
            .into_any()
            .downcast::<Temp>()
            .map_err(|_| DriverError::Conflict)?;
        atomic_replace_temp(temp, dir(parent)?, name, None, requested, &self.facts)
    }
    fn atomic_replace_checked(
        &self,
        temp: Box<dyn OwnedTempHandle>,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<()> {
        let temp = temp
            .into_any()
            .downcast::<Temp>()
            .map_err(|_| DriverError::Conflict)?;
        atomic_replace_temp(
            temp,
            dir(parent)?,
            name,
            Some(expected),
            DirectoryDurability::ImmediateDirectoryDurability,
            &self.facts,
        )
        .map(drop)
    }
    fn create_symlink_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        target: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()> {
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_validate,
            || super::metadata::preflight_symlink(metadata),
        )?;
        let parent_dir = dir(parent)?;
        super::ffi::symlink_at(&parent_dir.file, name, target)?;
        let entry = super::ffi::open_entry_at(&parent_dir.file, name)?;
        let mut apply_elapsed = 0;
        let applied: Result<()> = (|| {
            write_metadata_values(&entry, metadata, &self.facts)?;
            metadata_apply_step(&mut apply_elapsed, || {
                super::ffi::set_symlink_mtime_at(
                    &parent_dir.file,
                    name,
                    metadata.mtime_seconds,
                    metadata.mtime_nanoseconds,
                )
                .map_err(Into::into)
            })?;
            Ok(())
        })();
        self.facts
            .update(|facts| finish_call(&mut facts.metadata_apply, apply_elapsed, applied.is_ok()));
        applied?;
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_postinstall_verify,
            || super::metadata::finish(&entry, metadata),
        )
    }
    fn atomic_replace_symlink(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
        target: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()> {
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_validate,
            || super::metadata::preflight_symlink(metadata),
        )?;
        let parent = &dir(parent)?.file;
        let prior = optional_token(parent, name)?;
        if prior.as_deref() != expected {
            return Err(DriverError::Conflict);
        }
        let staging = self.staging_dir.as_ref().ok_or(DriverError::Conflict)?;
        for _ in 0..64 {
            let temp_name = format!(
                "symlink-{}-{}",
                std::process::id(),
                TEMP_SERIAL.fetch_add(1, Ordering::Relaxed)
            )
            .into_bytes();
            match super::ffi::symlink_at(staging, &temp_name, target) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
            let requested = super::ffi::stable_token_at(staging, &temp_name)?;
            let prepared = (|| -> Result<()> {
                let entry = super::ffi::open_entry_at(staging, &temp_name)?;
                let mut apply_elapsed = 0;
                let applied: Result<()> = (|| {
                    write_metadata_values(&entry, metadata, &self.facts)?;
                    metadata_apply_step(&mut apply_elapsed, || {
                        super::ffi::set_symlink_mtime_at(
                            staging,
                            &temp_name,
                            metadata.mtime_seconds,
                            metadata.mtime_nanoseconds,
                        )
                        .map_err(Into::into)
                    })?;
                    Ok(())
                })();
                self.facts.update(|facts| {
                    finish_call(&mut facts.metadata_apply, apply_elapsed, applied.is_ok())
                });
                applied?;
                observed_call(
                    &self.facts,
                    |facts| &mut facts.metadata_preinstall_verify,
                    || super::metadata::finish(&entry, metadata),
                )
            })();
            if let Err(error) = prepared {
                let cleanup_start = Instant::now();
                let cleaned = super::ffi::unlink_if_identity_at(staging, &temp_name, &requested);
                finish_cleanup(&self.facts, cleanup_start, cleaned.is_ok());
                return Err(error);
            }
            let replace_start = Instant::now();
            let replaced = match super::ffi::replace_at(staging, &temp_name, parent, name) {
                Ok(()) => (|| {
                    if optional_token(parent, name)?.as_deref() != Some(requested.as_slice())
                        || optional_token(staging, &temp_name)?.is_some()
                    {
                        Err(DriverError::VisibilityAmbiguous)
                    } else {
                        Ok(())
                    }
                })(),
                Err(error) => reconcile_replace(parent, name, prior.clone(), &requested, error),
            };
            finish_replace(&self.facts, replace_start, prior.is_some(), &replaced);
            if let Err(error) = replaced {
                let cleanup_start = Instant::now();
                let cleaned = super::ffi::unlink_if_identity_at(staging, &temp_name, &requested);
                finish_cleanup(&self.facts, cleanup_start, cleaned.is_ok());
                return Err(error);
            }
            let verified = observed_call(
                &self.facts,
                |facts| &mut facts.metadata_postinstall_verify,
                || {
                    let entry = super::ffi::open_entry_at(parent, name)?;
                    super::metadata::verify(&entry, metadata)
                },
            );
            if verified.is_err() {
                return Err(DriverError::VisibilityAmbiguous);
            }
            let outcome = match sync_directory_file_io(
                parent,
                &self.facts,
                DirectorySyncOwner::InstallParent,
            ) {
                Ok(()) => Ok(()),
                Err(error) => reconcile_replace(parent, name, prior, &requested, error),
            };
            record_replace_durability_ambiguity(&self.facts, &outcome);
            return outcome;
        }
        Err(DriverError::Conflict)
    }
    fn create_hard_link_at(
        &self,
        source_parent: &dyn DirectoryHandle,
        source: &[u8],
        source_expected: &[u8],
        target_parent: &dyn DirectoryHandle,
        target: &[u8],
    ) -> Result<()> {
        let source_parent = &dir(source_parent)?.file;
        if super::ffi::stable_token_at(source_parent, source)? != source_expected {
            return Err(DriverError::Conflict);
        }
        super::ffi::hard_link_at(source_parent, source, &dir(target_parent)?.file, target)?;
        if super::ffi::stable_token_at(source_parent, source)
            .map(|actual| actual != source_expected)
            .unwrap_or(true)
        {
            return Err(DriverError::VisibilityAmbiguous);
        }
        Ok(())
    }
    fn finish_hard_link_at(
        &self,
        source_parent: &dyn DirectoryHandle,
        source: &[u8],
        source_expected: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()> {
        let source_parent = &dir(source_parent)?.file;
        if super::ffi::stable_token_at(source_parent, source)? != source_expected {
            return Err(DriverError::Conflict);
        }
        let entry = super::ffi::open_entry_at(source_parent, source)?;
        if super::ffi::file_stable_token(&entry)? != source_expected {
            return Err(DriverError::Conflict);
        }
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_validate,
            || super::metadata::preflight(&entry, metadata),
        )?;
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_postinstall_verify,
            || super::metadata::finish(&entry, metadata),
        )?;
        sync_file(&entry, &self.facts, FileSyncOwner::PostHardLink)
    }
    fn rename_at(
        &self,
        source_parent: &dyn DirectoryHandle,
        source: &[u8],
        target_parent: &dyn DirectoryHandle,
        target: &[u8],
    ) -> Result<()> {
        let source_parent = &dir(source_parent)?.file;
        let target_parent = &dir(target_parent)?.file;
        let requested = super::ffi::stable_token_at(source_parent, source)?;
        if let Err(error) = super::ffi::rename_at(source_parent, source, target_parent, target) {
            return reconcile_rename(
                source_parent,
                source,
                target_parent,
                target,
                &requested,
                error,
            );
        }
        let sync = if super::ffi::file_stable_token(source_parent)?
            == super::ffi::file_stable_token(target_parent)?
        {
            sync_directory_file_io(
                source_parent,
                &self.facts,
                DirectorySyncOwner::InstallParent,
            )
        } else {
            sync_directory_file_io(
                source_parent,
                &self.facts,
                DirectorySyncOwner::InstallParent,
            )
            .and_then(|_| {
                sync_directory_file_io(
                    target_parent,
                    &self.facts,
                    DirectorySyncOwner::InstallParent,
                )
            })
        };
        if let Err(error) = sync {
            return reconcile_rename(
                source_parent,
                source,
                target_parent,
                target,
                &requested,
                error,
            );
        }
        Ok(())
    }
    fn unlink_regular_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
    ) -> Result<()> {
        remove_entry(&dir(parent)?.file, name, expected, false, &self.facts)
    }
    fn unlink_symlink_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
    ) -> Result<()> {
        remove_entry(&dir(parent)?.file, name, expected, false, &self.facts)
    }
    fn remove_directory_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
    ) -> Result<()> {
        remove_entry(&dir(parent)?.file, name, expected, true, &self.facts)
    }
    fn sync_directory(&self, directory: &dyn DirectoryHandle) -> Result<()> {
        let directory = dir(directory)?;
        let owner = match directory.role {
            DirectoryRole::Root => DirectorySyncOwner::FinalRoot,
            DirectoryRole::Tree => DirectorySyncOwner::DirtyTree,
        };
        sync_directory_file(&directory.file, &self.facts, owner)
    }
    fn set_root_metadata(&self, metadata: &NativeMetadata) -> Result<()> {
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_validate,
            || super::metadata::preflight(&self.root_dir, metadata),
        )?;
        let mut apply_elapsed = 0;
        let applied: Result<()> = (|| {
            metadata_apply_step(&mut apply_elapsed, || {
                self.root_dir
                    .set_permissions(fs::Permissions::from_mode(metadata.mode))
                    .map_err(Into::into)
            })?;
            write_metadata_values(&self.root_dir, metadata, &self.facts)?;
            let modified = modified_time(metadata)?;
            metadata_apply_step(&mut apply_elapsed, || {
                self.root_dir
                    .set_times(FileTimes::new().set_modified(modified))
                    .map_err(Into::into)
            })?;
            Ok(())
        })();
        self.facts
            .update(|facts| finish_call(&mut facts.metadata_apply, apply_elapsed, applied.is_ok()));
        applied?;
        observed_call(
            &self.facts,
            |facts| &mut facts.metadata_postinstall_verify,
            || super::metadata::finish(&self.root_dir, metadata),
        )
    }
    fn discard_owned_root(self: Box<Self>) -> Result<()> {
        if !self.owned_root {
            return Err(DriverError::Conflict);
        }
        self.remove_owned_root(&self.root_identity)
    }
    fn remove_owned_root(&self, expected_identity: &[u8]) -> Result<()> {
        if super::ffi::file_stable_token(&self.root_dir)? != expected_identity {
            return Err(DriverError::Conflict);
        }
        for _ in 0..64 {
            let tombstone = format!(
                ".layerfs-owned-tombstone-{}-{}",
                std::process::id(),
                TEMP_SERIAL.fetch_add(1, Ordering::Relaxed)
            )
            .into_bytes();
            let start = Instant::now();
            let removed = super::ffi::detach_and_remove_owned_tree(
                &self.root_dir,
                &self.root_parent,
                &self.root_name,
                &tombstone,
                expected_identity,
            );
            finish_cleanup(&self.facts, start, removed.is_ok());
            match removed {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) if error.raw_os_error() == Some(libc::ESTALE) => {
                    return Err(DriverError::VisibilityAmbiguous)
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(DriverError::Conflict)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if let Some(staging) = self.staging_dir.take() {
            let start = Instant::now();
            let removed = super::ffi::remove_owned_tree(
                &staging,
                &self.staging_parent,
                &self.staging_name,
                &self.staging_identity,
            );
            finish_cleanup(&self.facts, start, removed.is_ok());
        }
    }
}

fn encode_recovery_record(
    store_id: [u8; 32],
    owned_root: bool,
    root_name: &[u8],
    root_identity: &[u8],
) -> Vec<u8> {
    let mut record =
        Vec::with_capacity(RECOVERY_MAGIC.len() + 37 + root_name.len() + root_identity.len());
    record.extend_from_slice(RECOVERY_MAGIC);
    record.extend_from_slice(&store_id);
    record.push(u8::from(owned_root));
    record.extend_from_slice(&(root_name.len() as u16).to_be_bytes());
    record.extend_from_slice(&(root_identity.len() as u16).to_be_bytes());
    record.extend_from_slice(root_name);
    record.extend_from_slice(root_identity);
    record
}

fn decode_recovery_record(
    record: &[u8],
    expected_store_id: [u8; 32],
) -> Result<(bool, Vec<u8>, Vec<u8>)> {
    let fixed = RECOVERY_MAGIC.len() + 37;
    if record.len() < fixed
        || !record.starts_with(RECOVERY_MAGIC)
        || record[RECOVERY_MAGIC.len()..RECOVERY_MAGIC.len() + 32] != expected_store_id
    {
        return Err(DriverError::Conflict);
    }
    let offset = RECOVERY_MAGIC.len() + 32;
    let owned = match record[offset] {
        0 => false,
        1 => true,
        _ => return Err(DriverError::Conflict),
    };
    let name_len = u16::from_be_bytes(record[offset + 1..offset + 3].try_into().unwrap()) as usize;
    let identity_len =
        u16::from_be_bytes(record[offset + 3..offset + 5].try_into().unwrap()) as usize;
    if name_len == 0
        || name_len > 255
        || identity_len == 0
        || fixed
            .checked_add(name_len)
            .and_then(|length| length.checked_add(identity_len))
            != Some(record.len())
    {
        return Err(DriverError::Conflict);
    }
    let name = record[fixed..fixed + name_len].to_vec();
    if name.contains(&0) || name == b"." || name == b".." || name.contains(&b'/') {
        return Err(DriverError::Conflict);
    }
    Ok((owned, name, record[fixed + name_len..].to_vec()))
}

fn recover_owned_workspaces(
    parent_path: &Path,
    store_id: [u8; 32],
    facts: &Recorder,
) -> Result<()> {
    let parent = super::ffi::open_directory_path_nofollow(parent_path)?;
    let mut removed = false;
    let recovery = (|| -> Result<()> {
        for entry in super::ffi::directory_entries(&parent)? {
            let (staging_name, kind, _, token, staging_identity) = match entry {
                Ok(entry) => entry,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if kind != NativeKind::Directory || !staging_name.starts_with(b".layerfs-staging-") {
                continue;
            }
            let staging = match super::ffi::open_directory_at(&parent, &staging_name) {
                Ok(staging) => staging,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            validate_expected(&staging, Some(&token))?;
            let mut marker = match super::ffi::open_regular_at(&staging, RECOVERY_MARKER, false) {
                Ok(marker) => marker,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(DriverError::Conflict)
                }
                Err(error) => {
                    return Err(DriverError::Io(std::io::Error::new(
                        error.kind(),
                        format!("open recovery marker: {error}"),
                    )))
                }
            };
            if !super::ffi::try_lock_exclusive(&marker)? {
                continue;
            }
            let mut record = Vec::new();
            Read::by_ref(&mut marker)
                .take(4097)
                .read_to_end(&mut record)?;
            if record.len() > 4096 {
                return Err(DriverError::Conflict);
            }
            let (owned_root, root_name, root_identity) = decode_recovery_record(&record, store_id)?;
            if owned_root {
                match super::ffi::open_directory_at(&parent, &root_name) {
                    Ok(root) => {
                        if super::ffi::file_stable_token(&root)? != root_identity {
                            return Err(DriverError::VisibilityAmbiguous);
                        }
                        remove_recovered_tree(
                            &root,
                            &parent,
                            &root_name,
                            &root_identity,
                            b".layerfs-recovered-root-",
                            facts,
                        )?;
                        removed = true;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            remove_recovered_tree(
                &staging,
                &parent,
                &staging_name,
                &staging_identity,
                b".layerfs-recovered-staging-",
                facts,
            )?;
            removed = true;
        }
        Ok(())
    })();
    if removed {
        sync_directory_file(&parent, facts, DirectorySyncOwner::RootParent)
            .map_err(|_| DriverError::DurabilityAmbiguous)?;
    }
    recovery
}

fn remove_recovered_tree(
    root: &File,
    parent: &File,
    name: &[u8],
    identity: &[u8],
    prefix: &[u8],
    facts: &Recorder,
) -> Result<()> {
    for _ in 0..64 {
        let mut tombstone = prefix.to_vec();
        tombstone.extend_from_slice(std::process::id().to_string().as_bytes());
        tombstone.push(b'-');
        tombstone.extend_from_slice(
            TEMP_SERIAL
                .fetch_add(1, Ordering::Relaxed)
                .to_string()
                .as_bytes(),
        );
        let start = Instant::now();
        let removed =
            super::ffi::detach_and_remove_owned_tree(root, parent, name, &tombstone, identity);
        finish_cleanup(facts, start, removed.is_ok());
        match removed {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) if error.raw_os_error() == Some(libc::ESTALE) => {
                return Err(DriverError::VisibilityAmbiguous)
            }
            Err(error) => {
                return Err(DriverError::Io(std::io::Error::new(
                    error.kind(),
                    format!("remove recovered tree: {error}"),
                )))
            }
        }
    }
    Err(DriverError::Conflict)
}

fn dir(handle: &dyn DirectoryHandle) -> Result<&Dir> {
    handle
        .as_any()
        .downcast_ref::<Dir>()
        .ok_or(DriverError::Conflict)
}
fn optional_token(parent: &File, name: &[u8]) -> Result<Option<Vec<u8>>> {
    match super::ffi::stable_token_at(parent, name) {
        Ok(token) => Ok(Some(token)),
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[allow(clippy::boxed_local)]
fn atomic_replace_temp(
    temp: Box<Temp>,
    parent: &Dir,
    name: &[u8],
    required_prior: Option<Option<&[u8]>>,
    directory_durability: DirectoryDurability,
    facts: &Recorder,
) -> Result<DirectoryDurability> {
    sync_file(&temp.file, facts, FileSyncOwner::ContentTemp)?;
    let requested = super::ffi::file_stable_token(&temp.file)?;
    let prior = optional_token(&parent.file, name)?;
    if required_prior.is_some_and(|expected| prior.as_deref() != expected) {
        return Err(DriverError::Conflict);
    }
    let replace_start = Instant::now();
    let replaced = match super::ffi::replace_at(&temp.staging, &temp.name, &parent.file, name) {
        Ok(()) => (|| {
            if optional_token(&parent.file, name)?.as_deref() != Some(requested.as_slice())
                || optional_token(&temp.staging, &temp.name)?.is_some()
            {
                Err(DriverError::VisibilityAmbiguous)
            } else {
                Ok(())
            }
        })(),
        Err(error) => reconcile_replace(&parent.file, name, prior.clone(), &requested, error),
    };
    finish_replace(facts, replace_start, prior.is_some(), &replaced);
    replaced?;
    let entry = super::ffi::open_entry_at(&parent.file, name)?;
    let expected = temp
        .expected_metadata
        .lock()
        .map_err(|_| DriverError::Conflict)?
        .clone()
        .ok_or(DriverError::Conflict)?;
    let restrictive_flags = expected.bsd_flags != 0;
    let finalized = observed_call(
        facts,
        |facts| &mut facts.metadata_postinstall_verify,
        || {
            if restrictive_flags {
                super::metadata::finish(&entry, &expected)
            } else {
                super::metadata::verify(&entry, &expected)
            }
        },
    );
    if finalized.is_err() {
        return Err(DriverError::VisibilityAmbiguous);
    }
    if restrictive_flags && sync_file(&entry, facts, FileSyncOwner::PostHardLink).is_err() {
        return Err(DriverError::VisibilityAmbiguous);
    }
    if directory_durability == DirectoryDurability::DeferredToIncompleteTreeBoundary {
        return Ok(DirectoryDurability::DeferredToIncompleteTreeBoundary);
    }
    let outcome =
        match sync_directory_file_io(&parent.file, facts, DirectorySyncOwner::InstallParent) {
            Ok(()) => Ok(DirectoryDurability::ImmediateDirectoryDurability),
            Err(error) => reconcile_replace(&parent.file, name, prior, &requested, error)
                .map(|()| DirectoryDurability::ImmediateDirectoryDurability),
        };
    record_replace_durability_ambiguity(facts, &outcome);
    outcome
}

fn validate_expected(file: &File, expected: Option<&[u8]>) -> Result<()> {
    if let Some(expected) = expected {
        if super::ffi::file_token(file)? != expected {
            return Err(DriverError::Conflict);
        }
    }
    Ok(())
}

fn validate_entry_expected(parent: &File, name: &[u8], expected: Option<&[u8]>) -> Result<()> {
    if let Some(expected) = expected {
        if super::ffi::token_at(parent, name)? != expected {
            return Err(DriverError::Conflict);
        }
    }
    Ok(())
}

fn reconcile_replace(
    parent: &File,
    name: &[u8],
    prior: Option<Vec<u8>>,
    requested: &[u8],
    error: std::io::Error,
) -> Result<()> {
    match optional_token(parent, name)? {
        Some(actual) if actual == requested => Err(DriverError::DurabilityAmbiguous),
        actual if actual == prior => Err(DriverError::Io(error)),
        _ => Err(DriverError::VisibilityAmbiguous),
    }
}

fn reconcile_rename(
    source_parent: &File,
    source: &[u8],
    target_parent: &File,
    target: &[u8],
    requested: &[u8],
    error: std::io::Error,
) -> Result<()> {
    let source = optional_token(source_parent, source)?;
    let target = optional_token(target_parent, target)?;
    if source.is_none() && target.as_deref() == Some(requested) {
        Err(DriverError::DurabilityAmbiguous)
    } else if source.as_deref() == Some(requested) && target.is_none() {
        Err(DriverError::Io(error))
    } else {
        Err(DriverError::VisibilityAmbiguous)
    }
}

fn remove_entry(
    parent: &File,
    name: &[u8],
    expected: &[u8],
    directory: bool,
    facts: &Recorder,
) -> Result<()> {
    if super::ffi::stable_token_at(parent, name)? != expected {
        return Err(DriverError::Conflict);
    }
    let removed = if directory {
        super::ffi::remove_directory_if_identity_at(parent, name, expected)
    } else {
        super::ffi::unlink_if_identity_at(parent, name, expected)
    };
    if let Err(error) = removed {
        return reconcile_remove(parent, name, expected, error);
    }
    match sync_directory_file_io(parent, facts, DirectorySyncOwner::InstallParent) {
        Ok(()) => Ok(()),
        Err(error) => reconcile_remove(parent, name, expected, error),
    }
}

fn reconcile_remove(
    parent: &File,
    name: &[u8],
    expected: &[u8],
    error: std::io::Error,
) -> Result<()> {
    match optional_token(parent, name)? {
        None => Err(DriverError::DurabilityAmbiguous),
        Some(actual) if actual == expected => Err(DriverError::Io(error)),
        Some(_) => Err(DriverError::VisibilityAmbiguous),
    }
}

pub(super) fn modified_time(metadata: &NativeMetadata) -> Result<SystemTime> {
    let seconds = metadata.mtime_seconds;
    let nanos = metadata.mtime_nanoseconds;
    if nanos >= 1_000_000_000 {
        return Err(DriverError::Unsupported);
    }
    let time = if seconds >= 0 {
        UNIX_EPOCH.checked_add(Duration::new(seconds as u64, nanos))
    } else if nanos == 0 {
        UNIX_EPOCH.checked_sub(Duration::new(seconds.unsigned_abs(), 0))
    } else {
        UNIX_EPOCH.checked_sub(Duration::new(
            seconds.unsigned_abs() - 1,
            1_000_000_000 - nanos,
        ))
    };
    time.ok_or(DriverError::Unsupported)
}
