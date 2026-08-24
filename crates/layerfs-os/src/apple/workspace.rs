use layerfs_vfs::driver::*;
use std::any::Any;
use std::fs::{self, File, FileTimes};
use std::io::{Read, Seek, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static TEMP_SERIAL: AtomicU64 = AtomicU64::new(0);
const RECOVERY_MARKER: &[u8] = b".layerfs-recovery-v1";
const RECOVERY_MAGIC: &[u8] = b"layerfs/apple-recovery/v1\0";

#[derive(Default)]
pub struct AppleDriver;

struct Workspace {
    root_dir: File,
    root_parent: File,
    root_name: Vec<u8>,
    staging_dir: Option<File>,
    staging_parent: File,
    staging_name: Vec<u8>,
    staging_identity: Vec<u8>,
    _recovery_marker: File,
    managed: bool,
}
struct Dir {
    file: File,
}
struct Regular(File);
struct Temp {
    file: File,
    staging: File,
    name: Vec<u8>,
    identity: Vec<u8>,
    expected_metadata: Mutex<Option<NativeMetadata>>,
    deferred_flags: u32,
}
struct Preflight {
    directory: File,
    staging: File,
    name: Vec<u8>,
    identity: Vec<u8>,
    active: bool,
}

impl NamePreflight for Preflight {
    fn add(&mut self, name: &[u8]) -> Result<()> {
        super::ffi::create_regular_at(&self.directory, name)?;
        Ok(())
    }
    fn finish(mut self: Box<Self>) -> Result<()> {
        super::ffi::remove_owned_tree(&self.directory, &self.staging, &self.name, &self.identity)?;
        self.active = false;
        Ok(())
    }
}

impl Drop for Preflight {
    fn drop(&mut self) {
        if self.active {
            let _ = super::ffi::remove_owned_tree(
                &self.directory,
                &self.staging,
                &self.name,
                &self.identity,
            );
        }
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = super::ffi::unlink_if_identity_at(&self.staging, &self.name, &self.identity);
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
        self.0.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
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
        self.file.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
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
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl ProjectionDriver for AppleDriver {
    fn open_workspace(
        &self,
        path: &Path,
        policy: WorkspacePolicy,
        store_id: [u8; 32],
    ) -> Result<Box<dyn ProjectionWorkspace>> {
        let parent_path = path.parent().unwrap_or_else(|| Path::new("."));
        let root_parent = super::ffi::open_directory_path_nofollow(parent_path)?;
        let root_name = path
            .file_name()
            .ok_or(DriverError::Conflict)?
            .as_bytes()
            .to_vec();
        let root_dir = if policy == WorkspacePolicy::ManagedCreateOwned {
            match super::ffi::mkdir_at(&root_parent, &root_name) {
                Ok(()) => super::ffi::open_directory_at(&root_parent, &root_name)?,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(DriverError::Conflict)
                }
                Err(error) => return Err(error.into()),
            }
        } else {
            match super::ffi::open_directory_at(&root_parent, &root_name) {
                Ok(file) => file,
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        && policy == WorkspacePolicy::ExternalCooperative =>
                {
                    super::ffi::mkdir_at(&root_parent, &root_name)?;
                    super::ffi::open_directory_at(&root_parent, &root_name)?
                }
                Err(error) => return Err(error.into()),
            }
        };
        let (staging_name, staging_dir) = (0..64)
            .find_map(|_| {
                let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
                let name = format!(".layerfs-staging-{}-{serial}", std::process::id()).into_bytes();
                match super::ffi::mkdir_at(&root_parent, &name) {
                    Ok(()) => Some(
                        super::ffi::open_directory_at(&root_parent, &name)
                            .map(|directory| (name, directory)),
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .transpose()?
            .ok_or(DriverError::Conflict)?;
        staging_dir.set_permissions(fs::Permissions::from_mode(0o700))?;
        let staging_identity = super::ffi::file_stable_token(&staging_dir)?;
        let root_identity = super::ffi::file_stable_token(&root_dir)?;
        let mut recovery_marker = super::ffi::create_regular_at(&staging_dir, RECOVERY_MARKER)?;
        recovery_marker.set_permissions(fs::Permissions::from_mode(0o600))?;
        recovery_marker.write_all(&encode_recovery_record(
            store_id,
            policy == WorkspacePolicy::ManagedCreateOwned,
            &root_name,
            &root_identity,
        ))?;
        recovery_marker.sync_all()?;
        if !super::ffi::try_lock_exclusive(&recovery_marker)? {
            return Err(DriverError::Conflict);
        }
        staging_dir.sync_all()?;
        root_parent.sync_all()?;
        Ok(Box::new(Workspace {
            root_dir,
            root_parent: root_parent.try_clone()?,
            root_name,
            staging_dir: Some(staging_dir),
            staging_parent: root_parent,
            staging_name,
            staging_identity,
            _recovery_marker: recovery_marker,
            managed: policy != WorkspacePolicy::ExternalCooperative,
        }))
    }

    fn recover_owned_workspaces(&self, parent: &Path, store_id: [u8; 32]) -> Result<()> {
        recover_owned_workspaces(parent, store_id)
    }
}

impl ProjectionWorkspace for Workspace {
    fn root_directory(&self) -> Result<Box<dyn DirectoryHandle>> {
        Ok(Box::new(Dir {
            file: self.root_dir.try_clone()?,
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
        Ok(Box::new(Dir { file }))
    }
    fn duplicate_directory(
        &self,
        directory: &dyn DirectoryHandle,
    ) -> Result<Box<dyn DirectoryHandle>> {
        let directory = dir(directory)?;
        Ok(Box::new(Dir {
            file: directory.file.try_clone()?,
        }))
    }
    fn directory_token(&self, directory: &dyn DirectoryHandle) -> Result<Vec<u8>> {
        Ok(super::ffi::file_token(&dir(directory)?.file)?)
    }
    fn directory_identity(&self, directory: &dyn DirectoryHandle) -> Result<Vec<u8>> {
        Ok(super::ffi::file_stable_token(&dir(directory)?.file)?)
    }
    fn revalidate_root_binding(&self) -> Result<()> {
        if super::ffi::stable_token_at(&self.root_parent, &self.root_name)?
            != super::ffi::file_stable_token(&self.root_dir)?
        {
            return Err(DriverError::Conflict);
        }
        Ok(())
    }
    fn begin_name_preflight(&self) -> Result<Box<dyn NamePreflight>> {
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
        Ok(Box::new(Preflight {
            directory,
            staging: staging.try_clone()?,
            name,
            identity,
            active: true,
        }))
    }
    fn open_regular_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn RegularFileHandle>> {
        let file = super::ffi::open_regular_at(&dir(parent)?.file, name, self.managed)?;
        validate_expected(&file, expected)?;
        Ok(Box::new(Regular(file)))
    }
    fn open_regular_read_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn RegularFileHandle>> {
        let file = super::ffi::open_regular_at(&dir(parent)?.file, name, false)?;
        validate_expected(&file, expected)?;
        Ok(Box::new(Regular(file)))
    }
    fn set_regular_len(&self, file: &mut dyn RegularFileHandle, len: u64) -> Result<()> {
        file.as_any()
            .downcast_ref::<Regular>()
            .ok_or(DriverError::Conflict)?
            .0
            .set_len(len)?;
        Ok(())
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
        Ok(Box::new(Dir {
            file: super::ffi::open_directory_at(&parent.file, name)?,
        }))
    }
    fn create_temp_at(&self, _parent: &dyn DirectoryHandle) -> Result<Box<dyn OwnedTempHandle>> {
        for _ in 0..64 {
            let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
            let name = format!("temp-{}-{serial}", std::process::id()).into_bytes();
            let staging = self.staging_dir.as_ref().ok_or(DriverError::Conflict)?;
            match super::ffi::create_regular_at(staging, &name) {
                Ok(file) => {
                    let identity = super::ffi::file_stable_token(&file)?;
                    return Ok(Box::new(Temp {
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
        if source.0.metadata()?.nlink() != 1 {
            return Err(DriverError::Unsupported);
        }
        let metadata = super::metadata::read(&source.0)?;
        let staging = self.staging_dir.as_ref().ok_or(DriverError::Conflict)?;
        for _ in 0..64 {
            let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
            let name = format!("clone-{}-{serial}", std::process::id()).into_bytes();
            match super::ffi::clone_file_at(&source.0, staging, &name) {
                Ok(file) => {
                    let identity = super::ffi::file_stable_token(&file)?;
                    super::ffi::set_flags_file(&file, 0)?;
                    return Ok(Box::new(Temp {
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
        super::metadata::preflight(&temp.file, metadata)?;
        temp.file
            .set_permissions(fs::Permissions::from_mode(metadata.mode))?;
        super::metadata::write(&temp.file, metadata)?;
        temp.file
            .set_times(FileTimes::new().set_modified(modified_time(metadata)?))?;
        super::metadata::verify_before_install(&temp.file, metadata)?;
        *temp
            .expected_metadata
            .lock()
            .map_err(|_| DriverError::Conflict)? = Some(metadata.clone());
        temp.file.sync_all()?;
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
        super::metadata::preflight(&entry, metadata)?;
        if native.file_type().is_symlink() {
            super::metadata::write(&entry, metadata)?;
            super::ffi::set_symlink_mtime_at(
                parent,
                name,
                metadata.mtime_seconds,
                metadata.mtime_nanoseconds,
            )?;
            return super::metadata::finish(&entry, metadata);
        }
        entry.set_permissions(fs::Permissions::from_mode(metadata.mode))?;
        super::metadata::write(&entry, metadata)?;
        entry.set_times(FileTimes::new().set_modified(modified_time(metadata)?))?;
        super::metadata::finish(&entry, metadata)
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
        temp.file.sync_all()?;
        let parent_dir = dir(parent)?;
        let requested = super::ffi::file_stable_token(&temp.file)?;
        let prior = optional_token(&parent_dir.file, name)?;
        if let Err(error) =
            super::ffi::replace_at(&temp.staging, &temp.name, &parent_dir.file, name)
        {
            return reconcile_replace(&parent_dir.file, name, prior, &requested, error);
        }
        if optional_token(&parent_dir.file, name)?.as_deref() != Some(requested.as_slice())
            || optional_token(&temp.staging, &temp.name)?.is_some()
        {
            return Err(DriverError::VisibilityAmbiguous);
        }
        let entry = super::ffi::open_entry_at(&parent_dir.file, name)?;
        let expected = temp
            .expected_metadata
            .lock()
            .map_err(|_| DriverError::Conflict)?
            .clone()
            .ok_or(DriverError::Conflict)?;
        super::metadata::finish(&entry, &expected)?;
        entry.sync_all()?;
        match parent_dir.file.sync_all() {
            Ok(()) => Ok(()),
            Err(error) => reconcile_replace(&parent_dir.file, name, prior, &requested, error),
        }
    }
    fn create_symlink_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        target: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()> {
        super::metadata::preflight_symlink(metadata)?;
        let parent_dir = dir(parent)?;
        super::ffi::symlink_at(&parent_dir.file, name, target)?;
        let entry = super::ffi::open_entry_at(&parent_dir.file, name)?;
        super::metadata::write(&entry, metadata)?;
        super::ffi::set_symlink_mtime_at(
            &parent_dir.file,
            name,
            metadata.mtime_seconds,
            metadata.mtime_nanoseconds,
        )?;
        super::metadata::finish(&entry, metadata)
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
        if super::ffi::stable_token_at(source_parent, source)? != source_expected {
            return Err(DriverError::Conflict);
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
        super::metadata::preflight(&entry, metadata)?;
        super::metadata::finish(&entry, metadata)?;
        entry.sync_all()?;
        Ok(())
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
        if let Err(error) = source_parent
            .sync_all()
            .and_then(|_| target_parent.sync_all())
        {
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
    fn sync_directory(&self, directory: &dyn DirectoryHandle) -> Result<()> {
        dir(directory)?.file.sync_all()?;
        Ok(())
    }
    fn set_root_metadata(&self, metadata: &NativeMetadata) -> Result<()> {
        super::metadata::preflight(&self.root_dir, metadata)?;
        self.root_dir
            .set_permissions(fs::Permissions::from_mode(metadata.mode))?;
        super::metadata::write(&self.root_dir, metadata)?;
        self.root_dir
            .set_times(FileTimes::new().set_modified(modified_time(metadata)?))?;
        super::metadata::finish(&self.root_dir, metadata)
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
            match super::ffi::detach_and_remove_owned_tree(
                &self.root_dir,
                &self.root_parent,
                &self.root_name,
                &tombstone,
                expected_identity,
            ) {
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
            let _ = super::ffi::remove_owned_tree(
                &staging,
                &self.staging_parent,
                &self.staging_name,
                &self.staging_identity,
            );
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

fn recover_owned_workspaces(parent_path: &Path, store_id: [u8; 32]) -> Result<()> {
    let parent = super::ffi::open_directory_path_nofollow(parent_path)?;
    for entry in super::ffi::directory_entries(&parent)? {
        let (staging_name, kind, _, token, staging_identity) = entry?;
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
                    )?;
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
        )?;
    }
    parent.sync_all()?;
    Ok(())
}

fn remove_recovered_tree(
    root: &File,
    parent: &File,
    name: &[u8],
    identity: &[u8],
    prefix: &[u8],
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
        match super::ffi::detach_and_remove_owned_tree(root, parent, name, &tombstone, identity) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::process::Command;

    fn test_parent() -> std::path::PathBuf {
        fs::canonicalize(std::env::temp_dir()).unwrap()
    }

    #[test]
    fn owned_recovery_crash_child() {
        let Some(base) = std::env::var_os("LAYERFS_OWNED_RECOVERY_CHILD") else {
            return;
        };
        let workspace = AppleDriver
            .open_workspace(
                &Path::new(&base).join("owned"),
                WorkspacePolicy::ManagedCreateOwned,
                [0x73; 32],
            )
            .unwrap();
        fs::write(Path::new(&base).join("owned/file"), b"crash residue").unwrap();
        std::mem::forget(workspace);
        std::process::exit(91);
    }

    #[test]
    fn reopen_removes_only_unlocked_store_bound_owned_workspace() {
        let base = test_parent().join(format!(
            "layerfs-owned-recovery-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&base).unwrap();
        let live_path = base.join("live");
        let live = AppleDriver
            .open_workspace(&live_path, WorkspacePolicy::ManagedCreateOwned, [0x73; 32])
            .unwrap();
        AppleDriver
            .recover_owned_workspaces(&base, [0x73; 32])
            .unwrap();
        assert!(live_path.exists(), "recovery removed a live owned root");
        let live_root = live.root_directory().unwrap();
        let live_identity = live.directory_identity(live_root.as_ref()).unwrap();
        live.remove_owned_root(&live_identity).unwrap();
        drop(live);

        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "apple::workspace::tests::owned_recovery_crash_child",
            ])
            .env("LAYERFS_OWNED_RECOVERY_CHILD", &base)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(91));
        assert!(base.join("owned").exists());
        AppleDriver
            .recover_owned_workspaces(&base, [0x73; 32])
            .unwrap();
        assert!(!base.join("owned").exists());
        assert!(!fs::read_dir(&base).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .as_bytes()
            .starts_with(b".layerfs-staging-")));
        fs::remove_dir(base).unwrap();
    }

    #[test]
    fn apfs_preflight_rejects_case_and_normalization_collisions() {
        let path = test_parent().join(format!(
            "layerfs-name-preflight-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let workspace = AppleDriver
            .open_workspace(&path, WorkspacePolicy::ExternalCooperative, [0; 32])
            .unwrap();
        let mut case = workspace.begin_name_preflight().unwrap();
        case.add(b"Readme").unwrap();
        assert!(case.add(b"README").is_err());
        drop(case);
        let mut normalized = workspace.begin_name_preflight().unwrap();
        normalized.add("é".as_bytes()).unwrap();
        assert!(normalized.add("e\u{301}".as_bytes()).is_err());
        drop(normalized);
        drop(workspace);
        fs::remove_dir(path).unwrap();
    }

    #[test]
    fn top_level_workspace_admission_never_follows_a_symlink() {
        let base = test_parent().join(format!(
            "layerfs-top-level-nofollow-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&base).unwrap();
        fs::create_dir(base.join("target")).unwrap();
        symlink("target", base.join("workspace")).unwrap();
        assert!(AppleDriver
            .open_workspace(
                &base.join("workspace"),
                WorkspacePolicy::ExternalCooperative,
                [0; 32],
            )
            .is_err());
        fs::remove_file(base.join("workspace")).unwrap();
        fs::remove_dir(base.join("target")).unwrap();
        fs::remove_dir(base).unwrap();
    }

    #[test]
    fn managed_owned_root_collision_preserves_preexisting_tree() {
        let base = test_parent().join(format!(
            "layerfs-owned-root-collision-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let workspace = base.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        fs::write(workspace.join("keep"), b"caller-owned").unwrap();
        assert!(matches!(
            AppleDriver.open_workspace(&workspace, WorkspacePolicy::ManagedCreateOwned, [0; 32],),
            Err(DriverError::Conflict)
        ));
        assert_eq!(fs::read(workspace.join("keep")).unwrap(), b"caller-owned");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn workspace_admission_never_follows_parent_symlinks() {
        let base = test_parent().join(format!(
            "layerfs-parent-nofollow-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(base.join("real")).unwrap();
        symlink("real", base.join("alias")).unwrap();
        assert!(AppleDriver
            .open_workspace(
                &base.join("alias/workspace"),
                WorkspacePolicy::ExternalCooperative,
                [0; 32],
            )
            .is_err());
        assert!(!base.join("real/workspace").exists());
        fs::remove_file(base.join("alias")).unwrap();
        fs::remove_dir(base.join("real")).unwrap();
        fs::remove_dir(base).unwrap();
    }

    #[test]
    fn workspace_metadata_refusal_precedes_permission_mutation() {
        let path = test_parent().join(format!(
            "layerfs-metadata-preflight-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let workspace = AppleDriver
            .open_workspace(&path, WorkspacePolicy::ExternalCooperative, [0; 32])
            .unwrap();
        let before = workspace.read_root_metadata().unwrap();
        let mut refused = before.clone();
        refused.mode = if before.mode == 0o700 { 0o755 } else { 0o700 };
        refused
            .xattrs
            .push((b"com.apple.quarantine".to_vec(), b"blocked".to_vec()));
        assert!(matches!(
            workspace.set_root_metadata(&refused),
            Err(DriverError::Unsupported)
        ));
        assert_eq!(workspace.read_root_metadata().unwrap(), before);
        drop(workspace);
        fs::remove_dir(path).unwrap();
    }

    #[test]
    fn native_n0_and_n4_n5_cleanup_and_reconciliation_matrix() {
        let base = test_parent().join(format!(
            "layerfs-native-fault-matrix-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&base).unwrap();
        let parent = File::open(&base).unwrap();

        let temp_file = crate::apple::ffi::create_regular_at(&parent, b"temp").unwrap();
        let temp_identity = crate::apple::ffi::file_stable_token(&temp_file).unwrap();
        drop(Temp {
            file: temp_file,
            staging: parent.try_clone().unwrap(),
            name: b"temp".to_vec(),
            identity: temp_identity,
            expected_metadata: Mutex::new(None),
            deferred_flags: 0,
        });
        assert!(!base.join("temp").exists(), "N0 temp drop left residue");

        fs::write(base.join("prior"), b"prior").unwrap();
        fs::write(base.join("requested"), b"requested").unwrap();
        let prior = crate::apple::ffi::stable_token_at(&parent, b"prior").unwrap();
        let requested = crate::apple::ffi::stable_token_at(&parent, b"requested").unwrap();
        let injected = || std::io::Error::other("lost acknowledgement");
        assert!(matches!(
            reconcile_replace(
                &parent,
                b"requested",
                Some(prior.clone()),
                &requested,
                injected()
            ),
            Err(DriverError::DurabilityAmbiguous)
        ));
        assert!(matches!(
            reconcile_replace(
                &parent,
                b"prior",
                Some(prior.clone()),
                &requested,
                injected()
            ),
            Err(DriverError::Io(_))
        ));
        assert!(matches!(
            reconcile_replace(&parent, b"prior", None, &requested, injected()),
            Err(DriverError::VisibilityAmbiguous)
        ));

        fs::write(base.join("source"), b"rename").unwrap();
        let rename_token = crate::apple::ffi::stable_token_at(&parent, b"source").unwrap();
        assert!(matches!(
            reconcile_rename(
                &parent,
                b"source",
                &parent,
                b"target",
                &rename_token,
                injected()
            ),
            Err(DriverError::Io(_))
        ));
        crate::apple::ffi::rename_at(&parent, b"source", &parent, b"target").unwrap();
        assert!(matches!(
            reconcile_rename(
                &parent,
                b"source",
                &parent,
                b"target",
                &rename_token,
                injected()
            ),
            Err(DriverError::DurabilityAmbiguous)
        ));
        fs::write(base.join("source"), b"substitute").unwrap();
        assert!(matches!(
            reconcile_rename(
                &parent,
                b"source",
                &parent,
                b"target",
                &rename_token,
                injected()
            ),
            Err(DriverError::VisibilityAmbiguous)
        ));
        drop(parent);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn same_size_clone_patch_replaces_an_existing_single_link_destination() {
        let base = test_parent().join(format!(
            "layerfs-clone-install-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let root = base.join("root");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("file"), vec![0x5a; 8192]).unwrap();
        let workspace = AppleDriver
            .open_workspace(&root, WorkspacePolicy::ExternalCooperative, [0; 32])
            .unwrap();
        let directory = workspace.root_directory().unwrap();
        let source = workspace
            .open_regular_at(directory.as_ref(), b"file", None)
            .unwrap();
        let mut temp = workspace.clone_temp_from_regular(source.as_ref()).unwrap();
        temp.seek(std::io::SeekFrom::Start(4096)).unwrap();
        temp.write_all(b"PATCH").unwrap();
        let metadata = workspace
            .read_metadata_at(directory.as_ref(), b"file", None)
            .unwrap();
        workspace
            .set_temp_metadata(temp.as_mut(), &metadata)
            .unwrap();
        workspace
            .atomic_replace(temp, directory.as_ref(), b"file")
            .unwrap();
        assert_eq!(&fs::read(root.join("file")).unwrap()[4096..4101], b"PATCH");
        drop(workspace);
        fs::remove_dir_all(base).unwrap();
    }
}
