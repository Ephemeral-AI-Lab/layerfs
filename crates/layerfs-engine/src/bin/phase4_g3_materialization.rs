use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::*;

const DESTINATION_NAME: &str = "materialized.bin";
const MODE: u32 = 0o644;
const RECONCILIATION_COMPARISON_BYTES: usize = layerfs_core::cdc::MAXIMUM_CHUNK_BYTES;
const AUTHORITY_BINDINGS: &str = "[\"store_instance\",\"validation_authority\",\"profile\",\"integrity_epoch\",\"generation\",\"receipt_transition\",\"parent_root\",\"target_root\",\"destination_identity\",\"open_serial\",\"mutation_serial\",\"publication_serial\",\"operation\",\"nonce\",\"seed_identity\"]";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeIdentity {
    device: u64,
    inode: u64,
    links: u64,
    length: u64,
    mode: libc::mode_t,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl NativeIdentity {
    fn from_stat(value: &libc::stat) -> std::io::Result<Self> {
        Ok(Self {
            device: u64::try_from(value.st_dev)
                .map_err(|_| std::io::Error::from_raw_os_error(libc::EOVERFLOW))?,
            inode: value.st_ino,
            links: u64::from(value.st_nlink),
            length: u64::try_from(value.st_size)
                .map_err(|_| std::io::Error::from_raw_os_error(libc::EOVERFLOW))?,
            mode: value.st_mode,
            modified_seconds: value.st_mtime,
            modified_nanoseconds: value.st_mtime_nsec,
            changed_seconds: value.st_ctime,
            changed_nanoseconds: value.st_ctime_nsec,
        })
    }

    fn is_regular(self) -> bool {
        self.mode & libc::S_IFMT == libc::S_IFREG
    }

    fn is_symlink(self) -> bool {
        self.mode & libc::S_IFMT == libc::S_IFLNK
    }

    fn is_directory(self) -> bool {
        self.mode & libc::S_IFMT == libc::S_IFDIR
    }
}

#[derive(Clone, Copy, Default)]
struct StorageBytes {
    logical: u64,
    apparent: u64,
    allocated: u64,
}

impl StorageBytes {
    fn from_stat(value: &libc::stat) -> std::io::Result<Self> {
        let logical = u64::try_from(value.st_size)
            .map_err(|_| std::io::Error::from_raw_os_error(libc::EOVERFLOW))?;
        let allocated = u64::try_from(value.st_blocks)
            .map_err(|_| std::io::Error::from_raw_os_error(libc::EOVERFLOW))?
            .checked_mul(512)
            .ok_or_else(|| std::io::Error::from_raw_os_error(libc::EOVERFLOW))?;
        Ok(Self {
            logical,
            apparent: logical,
            allocated,
        })
    }
}

#[derive(Default)]
struct Timers {
    preflight: u128,
    qualification: u128,
    payload_prepare: u128,
    data_sync: u128,
    metadata: u128,
    metadata_sync: u128,
    rename: u128,
    directory_sync: u128,
    reconciliation: u128,
    cleanup: u128,
}

impl Timers {
    fn attributed(&self) -> Option<u128> {
        self.preflight
            .checked_add(self.qualification)?
            .checked_add(self.payload_prepare)?
            .checked_add(self.data_sync)?
            .checked_add(self.metadata)?
            .checked_add(self.metadata_sync)?
            .checked_add(self.rename)?
            .checked_add(self.directory_sync)?
            .checked_add(self.reconciliation)?
            .checked_add(self.cleanup)
    }
}

#[derive(Default)]
struct Counters {
    authority_reads: u64,
    authority_bytes_read: u64,
    seed_authority_reads: u64,
    seed_authority_bytes_read: u64,
    authority_validations: u64,
    authority_validation_successes: u64,
    authority_validation_failures: u64,
    permit_consumptions: u64,
    mapping_sql_queries: u64,
    mapping_sql_rows: u64,
    object_sql_queries: u64,
    object_sql_rows: u64,
    canonical_blob_reads: u64,
    canonical_blob_bytes: u64,
    authenticated_objects: u64,
    canonical_bytes_authenticated: u64,
    source_bytes_reconstructed: u64,
    destination_bytes_read: u64,
    verification_bytes_read: u64,
    clone_calls: u64,
    clone_successes: u64,
    clone_failures: u64,
    clone_source_logical_bytes: u64,
    copy_calls: u64,
    copied_payload_bytes: u64,
    patch_calls: u64,
    patch_bytes: u64,
    fallback_calls: u64,
    fallback_write_bytes: u64,
    changed_ranges: u64,
    changed_bytes: u64,
    metadata_operations: u64,
    temp_files_created: u64,
    temp_files_removed: u64,
    seed_files_created: u64,
    seed_files_removed: u64,
    data_sync_calls: u64,
    metadata_sync_calls: u64,
    rename_calls: u64,
    directory_sync_calls: u64,
    reconciliation_calls: u64,
    reconciliation_sql_queries: u64,
    reconciliation_sql_rows: u64,
    reconciliation_blob_reads: u64,
    reconciliation_canonical_bytes_authenticated: u64,
    reconciliation_source_bytes_compared: u64,
    reconciliation_q_high_water: u64,
    q_high_water: u64,
}

impl Counters {
    fn absorb_payload(&mut self, metrics: &Metrics) -> AnyResult<()> {
        self.object_sql_queries = self
            .object_sql_queries
            .checked_add(metrics.borrowed_row_blob_reads)
            .ok_or(CoreError::LengthOverflow)?;
        self.object_sql_rows = self
            .object_sql_rows
            .checked_add(metrics.borrowed_row_blob_reads)
            .ok_or(CoreError::LengthOverflow)?;
        self.mapping_sql_queries = self
            .mapping_sql_queries
            .checked_add(
                metrics
                    .sql_query_calls
                    .checked_sub(metrics.borrowed_row_blob_reads)
                    .ok_or(CoreError::LengthOverflow)?,
            )
            .ok_or(CoreError::LengthOverflow)?;
        self.mapping_sql_rows = self
            .mapping_sql_rows
            .checked_add(
                metrics
                    .sql_rows_returned
                    .checked_sub(metrics.borrowed_row_blob_reads)
                    .ok_or(CoreError::LengthOverflow)?,
            )
            .ok_or(CoreError::LengthOverflow)?;
        self.canonical_blob_reads = self
            .canonical_blob_reads
            .checked_add(metrics.row_blob_reads)
            .ok_or(CoreError::LengthOverflow)?;
        self.canonical_blob_bytes = self
            .canonical_blob_bytes
            .checked_add(metrics.canonical_bytes_authenticated)
            .ok_or(CoreError::LengthOverflow)?;
        self.authenticated_objects = self
            .authenticated_objects
            .checked_add(metrics.objects_authenticated)
            .ok_or(CoreError::LengthOverflow)?;
        self.canonical_bytes_authenticated = self
            .canonical_bytes_authenticated
            .checked_add(metrics.canonical_bytes_authenticated)
            .ok_or(CoreError::LengthOverflow)?;
        self.q_high_water = self.q_high_water.max(metrics.q_high_water);
        Ok(())
    }

    fn absorb_reconciliation(&mut self, metrics: &Metrics) -> AnyResult<()> {
        self.reconciliation_sql_queries = self
            .reconciliation_sql_queries
            .checked_add(metrics.sql_query_calls)
            .ok_or(CoreError::LengthOverflow)?;
        self.reconciliation_sql_rows = self
            .reconciliation_sql_rows
            .checked_add(metrics.sql_rows_returned)
            .ok_or(CoreError::LengthOverflow)?;
        self.reconciliation_blob_reads = self
            .reconciliation_blob_reads
            .checked_add(metrics.row_blob_reads)
            .ok_or(CoreError::LengthOverflow)?;
        self.reconciliation_canonical_bytes_authenticated = self
            .reconciliation_canonical_bytes_authenticated
            .checked_add(metrics.canonical_bytes_authenticated)
            .ok_or(CoreError::LengthOverflow)?;
        self.reconciliation_q_high_water =
            self.reconciliation_q_high_water.max(metrics.q_high_water);
        self.q_high_water = self.q_high_water.max(metrics.q_high_water);
        Ok(())
    }
}

#[derive(Clone)]
struct SeedIdentity {
    native: NativeIdentity,
    namespace_root: ObjectId,
    file_root: ObjectId,
    length: u64,
    references: u64,
    digest: [u8; 32],
}

struct VerifiedSeed {
    file: File,
    identity: SeedIdentity,
    storage: StorageBytes,
}

#[derive(Debug)]
struct NativePublicationCleanupFailure {
    publication: std::io::Error,
    cleanup: Box<dyn std::error::Error>,
}

impl std::fmt::Display for NativePublicationCleanupFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "publication failed before cleanup: {}; cleanup also failed: {}",
            self.publication, self.cleanup
        )
    }
}

impl std::error::Error for NativePublicationCleanupFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.publication)
    }
}

struct PermitBinding {
    store_instance_id: [u8; 16],
    validation_authority_id: [u8; 32],
    profile: [u8; 32],
    integrity_epoch: u64,
    generation: u64,
    receipt: [u8; 216],
    transition: ObjectId,
    parent_root: ObjectId,
    parent_file_root: ObjectId,
    target_root: ObjectId,
    target_file_root: ObjectId,
    directory: NativeIdentity,
    basename: String,
    destination: NativeIdentity,
    open_identity: u64,
    authority_serial: u64,
    mutation_serial: u64,
    publication_serial: u64,
    operation: String,
    range_start: u64,
    range_end: u64,
    canonical_range_commitment: [u8; 32],
    nonce: [u8; 32],
    seed: SeedIdentity,
}

struct Permit {
    binding: PermitBinding,
    tag: [u8; 32],
    consumed: bool,
}

fn hash_field(hasher: &mut blake3::Hasher, bytes: &[u8]) -> AnyResult<()> {
    hasher.update(
        &u64::try_from(bytes.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    hasher.update(bytes);
    Ok(())
}

fn hash_native(hasher: &mut blake3::Hasher, value: NativeIdentity) {
    hasher.update(&value.device.to_be_bytes());
    hasher.update(&value.inode.to_be_bytes());
    hasher.update(&value.links.to_be_bytes());
    hasher.update(&value.length.to_be_bytes());
    hasher.update(&value.mode.to_be_bytes());
    hasher.update(&value.modified_seconds.to_be_bytes());
    hasher.update(&value.modified_nanoseconds.to_be_bytes());
    hasher.update(&value.changed_seconds.to_be_bytes());
    hasher.update(&value.changed_nanoseconds.to_be_bytes());
}

fn canonical_range_binding(
    store: &Store,
    head: &VisibleHead,
    parent: Roots,
    parent_digest: [u8; 32],
    target: Roots,
    target_digest: [u8; 32],
    range: &std::ops::Range<u64>,
) -> CanonicalRangeBinding {
    CanonicalRangeBinding {
        store_instance_id: store.store_instance_id,
        validation_authority_id: store.validation_authority_id,
        profile: store.profile,
        integrity_epoch: store.integrity_epoch,
        generation: head.0,
        receipt: head.3,
        transition: head.2,
        open_identity: store.open_identity,
        authority_serial: store.same_open_authority_serial,
        mutation_serial: store.mutation_serial,
        parent_root: parent.namespace,
        parent_file_root: parent.file,
        parent_length: parent.length,
        parent_references: parent.references,
        parent_digest,
        target_root: target.namespace,
        target_file_root: target.file,
        target_length: target.length,
        target_references: target.references,
        target_digest,
        range_start: range.start,
        range_end: range.end,
    }
}

fn canonical_range_commitment(value: &CanonicalRangeBinding) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/phase4/g3/canonical-range-proof/v1\0");
    hasher.update(&value.store_instance_id);
    hasher.update(&value.validation_authority_id);
    hasher.update(&value.profile);
    hasher.update(&value.integrity_epoch.to_be_bytes());
    hasher.update(&value.generation.to_be_bytes());
    hasher.update(&value.receipt);
    hasher.update(value.transition.as_bytes());
    hasher.update(&value.open_identity.to_be_bytes());
    hasher.update(&value.authority_serial.to_be_bytes());
    hasher.update(&value.mutation_serial.to_be_bytes());
    hasher.update(value.parent_root.as_bytes());
    hasher.update(value.parent_file_root.as_bytes());
    hasher.update(&value.parent_length.to_be_bytes());
    hasher.update(&value.parent_references.to_be_bytes());
    hasher.update(&value.parent_digest);
    hasher.update(value.target_root.as_bytes());
    hasher.update(value.target_file_root.as_bytes());
    hasher.update(&value.target_length.to_be_bytes());
    hasher.update(&value.target_references.to_be_bytes());
    hasher.update(&value.target_digest);
    hasher.update(&value.range_start.to_be_bytes());
    hasher.update(&value.range_end.to_be_bytes());
    *hasher.finalize().as_bytes()
}

fn permit_tag(key: &[u8; 32], value: &PermitBinding) -> AnyResult<[u8; 32]> {
    let mut hasher = blake3::Hasher::new_keyed(key);
    hasher.update(b"layerfs/phase4/g3/materialization-permit/v1\0");
    hasher.update(&value.store_instance_id);
    hasher.update(&value.validation_authority_id);
    hasher.update(&value.profile);
    hasher.update(&value.integrity_epoch.to_be_bytes());
    hasher.update(&value.generation.to_be_bytes());
    hasher.update(&value.receipt);
    hasher.update(value.transition.as_bytes());
    hasher.update(value.parent_root.as_bytes());
    hasher.update(value.parent_file_root.as_bytes());
    hasher.update(value.target_root.as_bytes());
    hasher.update(value.target_file_root.as_bytes());
    hash_native(&mut hasher, value.directory);
    hash_field(&mut hasher, value.basename.as_bytes())?;
    hash_native(&mut hasher, value.destination);
    hasher.update(&value.open_identity.to_be_bytes());
    hasher.update(&value.authority_serial.to_be_bytes());
    hasher.update(&value.mutation_serial.to_be_bytes());
    hasher.update(&value.publication_serial.to_be_bytes());
    hash_field(&mut hasher, value.operation.as_bytes())?;
    hasher.update(&value.range_start.to_be_bytes());
    hasher.update(&value.range_end.to_be_bytes());
    hasher.update(&value.canonical_range_commitment);
    hasher.update(&value.nonce);
    hash_native(&mut hasher, value.seed.native);
    hasher.update(value.seed.namespace_root.as_bytes());
    hasher.update(value.seed.file_root.as_bytes());
    hasher.update(&value.seed.length.to_be_bytes());
    hasher.update(&value.seed.references.to_be_bytes());
    hasher.update(&value.seed.digest);
    Ok(*hasher.finalize().as_bytes())
}

fn random_u64() -> AnyResult<u64> {
    Ok(u64::from_be_bytes(os_random()?))
}

fn random_name(prefix: &str) -> AnyResult<String> {
    let random: [u8; 16] = os_random()?;
    Ok(format!("{prefix}{}", hex_bytes(&random)))
}

fn c_name(name: &std::ffi::OsStr) -> std::io::Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))
}

fn open_readonly_nofollow(path: &Path) -> std::io::Result<File> {
    let path = c_name(path.as_os_str())?;
    // SAFETY: `path` is NUL-terminated and the returned descriptor is uniquely owned.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK | libc::O_NOFOLLOW,
        )
    };
    if fd == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: `fd` was returned by `open` and is transferred exactly once.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn open_dir(path: &Path) -> std::io::Result<File> {
    let path = c_name(path.as_os_str())?;
    // SAFETY: `path` is NUL-terminated and the returned descriptor is uniquely owned.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if fd == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: `fd` was returned by `open` and is transferred exactly once.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn fclone_unlinked(seed: &File, directory: &File, name: &std::ffi::OsStr) -> std::io::Result<()> {
    let name = c_name(name)?;
    // SAFETY: both descriptors are live and `name` is NUL-terminated. The destination is new.
    let result =
        unsafe { libc::fclonefileat(seed.as_raw_fd(), directory.as_raw_fd(), name.as_ptr(), 0) };
    if result == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn fstat_file(file: &File) -> std::io::Result<(NativeIdentity, StorageBytes)> {
    let mut value = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: the descriptor is live and the output pointer is valid for one `stat`.
    if unsafe { libc::fstat(file.as_raw_fd(), value.as_mut_ptr()) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: successful `fstat` initialized the whole structure.
    let value = unsafe { value.assume_init() };
    Ok((
        NativeIdentity::from_stat(&value)?,
        StorageBytes::from_stat(&value)?,
    ))
}

fn stat_at(directory: &File, name: &str) -> std::io::Result<Option<NativeIdentity>> {
    let name = CString::new(name).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    let mut value = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: the directory descriptor and NUL-terminated name are live; output is writable.
    let result = unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            value.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result == -1 {
        let error = std::io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ENOENT) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    // SAFETY: successful `fstatat` initialized the whole structure.
    Ok(Some(NativeIdentity::from_stat(unsafe {
        &value.assume_init()
    })?))
}

fn openat_file(directory: &File, name: &str, flags: i32, mode: u32) -> std::io::Result<File> {
    let name = CString::new(name).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    // SAFETY: the directory descriptor and NUL-terminated name are live. The returned fd is owned.
    let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags, mode) };
    if fd == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: `fd` is newly returned and transferred exactly once.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn sync_fd(file: &File) -> std::io::Result<()> {
    // SAFETY: the descriptor is live for the duration of `fsync`.
    if unsafe { libc::fsync(file.as_raw_fd()) } == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn chmod_fd(file: &File, mode: u32) -> std::io::Result<()> {
    let mode = libc::mode_t::try_from(mode)
        .map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    // SAFETY: the descriptor is live and `mode` is a valid POSIX mode.
    if unsafe { libc::fchmod(file.as_raw_fd(), mode) } == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn pwrite_all(file: &File, mut offset: u64, mut bytes: &[u8], calls: &mut u64) -> AnyResult<()> {
    while !bytes.is_empty() {
        let file_offset = libc::off_t::try_from(offset).map_err(|_| CoreError::LengthOverflow)?;
        // SAFETY: the descriptor is writable; the input slice and offset are valid for the call.
        let written = unsafe {
            libc::pwrite(
                file.as_raw_fd(),
                bytes.as_ptr().cast(),
                bytes.len(),
                file_offset,
            )
        };
        *calls = calls.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        if written == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
        if written == 0 {
            return Err(std::io::Error::from_raw_os_error(libc::EIO).into());
        }
        let written = usize::try_from(written).map_err(|_| CoreError::LengthOverflow)?;
        offset = offset
            .checked_add(u64::try_from(written).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
        bytes = &bytes[written..];
    }
    Ok(())
}

fn unlink_at(directory: &File, name: &str) -> std::io::Result<bool> {
    let name = CString::new(name).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    // SAFETY: the directory descriptor and NUL-terminated basename are live.
    let result = unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
    if result == 0 {
        Ok(true)
    } else {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            Ok(false)
        } else {
            Err(error)
        }
    }
}

fn rename_at(directory: &File, from: &str, to: &str) -> std::io::Result<()> {
    const RENAME_NOFOLLOW_ANY: libc::c_uint = 0x10;
    let from = CString::new(from).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    let to = CString::new(to).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    // SAFETY: both names are NUL-terminated basenames relative to the same live directory fd.
    let result = unsafe {
        libc::renameatx_np(
            directory.as_raw_fd(),
            from.as_ptr(),
            directory.as_raw_fd(),
            to.as_ptr(),
            RENAME_NOFOLLOW_ANY,
        )
    };
    if result == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

struct TempName {
    directory: File,
    name: String,
    active: bool,
}

impl TempName {
    fn remove(&mut self, counters: &mut Counters) -> AnyResult<()> {
        if self.active && unlink_at(&self.directory, &self.name)? {
            counters.temp_files_removed = counters
                .temp_files_removed
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
        }
        self.active = false;
        Ok(())
    }

    #[cfg(test)]
    fn remove_with_fault(&mut self, counters: &mut Counters, fail: bool) -> AnyResult<()> {
        if fail {
            return Err(std::io::Error::from_raw_os_error(libc::EACCES).into());
        }
        self.remove(counters)
    }
}

impl Drop for TempName {
    fn drop(&mut self) {
        if self.active {
            let _ = unlink_at(&self.directory, &self.name);
        }
    }
}

fn create_temp(directory: &File, counters: &mut Counters) -> AnyResult<(File, TempName)> {
    let name = random_name(".g3-tmp-")?;
    let mut temp = TempName {
        directory: directory.try_clone()?,
        name: name.clone(),
        active: false,
    };
    let file = openat_file(
        directory,
        &name,
        libc::O_RDWR | libc::O_CLOEXEC | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
        0o600,
    )?;
    temp.active = true;
    counters.temp_files_created = counters
        .temp_files_created
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    Ok((file, temp))
}

fn clone_temp(
    seed: &VerifiedSeed,
    directory: &File,
    counters: &mut Counters,
    fail_reopen: bool,
) -> AnyResult<Option<(File, TempName)>> {
    let name = random_name(".g3-tmp-")?;
    let mut temp = TempName {
        directory: directory.try_clone()?,
        name: name.clone(),
        active: false,
    };
    counters.clone_calls = counters
        .clone_calls
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    match fclone_unlinked(&seed.file, directory, std::ffi::OsStr::new(&name)) {
        Ok(()) => {
            temp.active = true;
            counters.temp_files_created = counters
                .temp_files_created
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            let reopened = (|| -> AnyResult<File> {
                if fail_reopen {
                    return Err(CoreError::ValidationAuthorityUnavailable.into());
                }
                let entry =
                    stat_at(directory, &name)?.ok_or(CoreError::ValidationAuthorityUnavailable)?;
                let file = openat_file(
                    directory,
                    &name,
                    libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                    0,
                )?;
                let descriptor = fstat_file(&file)?.0;
                if !entry.is_regular()
                    || !descriptor.is_regular()
                    || entry.device != descriptor.device
                    || entry.inode != descriptor.inode
                {
                    return Err(CoreError::ValidationAuthorityUnavailable.into());
                }
                Ok(file)
            })();
            match reopened {
                Ok(file) => {
                    counters.clone_successes = counters
                        .clone_successes
                        .checked_add(1)
                        .ok_or(CoreError::LengthOverflow)?;
                    counters.clone_source_logical_bytes = counters
                        .clone_source_logical_bytes
                        .checked_add(seed.identity.length)
                        .ok_or(CoreError::LengthOverflow)?;
                    Ok(Some((file, temp)))
                }
                Err(_) => {
                    counters.clone_failures = counters
                        .clone_failures
                        .checked_add(1)
                        .ok_or(CoreError::LengthOverflow)?;
                    temp.remove(counters)?;
                    Ok(None)
                }
            }
        }
        Err(_) => {
            counters.clone_failures = counters
                .clone_failures
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            temp.remove(counters)?;
            Ok(None)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scenario {
    QualifiedNoop,
    QualifiedOneByte,
    QualifiedOneMib,
    InvalidAuthority,
    ExternalMutation,
    SymlinkSubstitution,
    CountChange,
    BeforePublicationFault,
    LostAck,
}

impl Scenario {
    fn parse(value: &str) -> AnyResult<Self> {
        Ok(match value {
            "qualified-noop" => Self::QualifiedNoop,
            "qualified-one-byte" => Self::QualifiedOneByte,
            "qualified-one-mib" => Self::QualifiedOneMib,
            "invalid-authority" => Self::InvalidAuthority,
            "external-mutation" => Self::ExternalMutation,
            "symlink-substitution" => Self::SymlinkSubstitution,
            "count-change" => Self::CountChange,
            "before-publication-fault" => Self::BeforePublicationFault,
            "lost-ack" => Self::LostAck,
            _ => return Err("unknown G3 scenario".into()),
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::QualifiedNoop => "qualified-noop",
            Self::QualifiedOneByte => "qualified-one-byte",
            Self::QualifiedOneMib => "qualified-one-mib",
            Self::InvalidAuthority => "invalid-authority",
            Self::ExternalMutation => "external-mutation",
            Self::SymlinkSubstitution => "symlink-substitution",
            Self::CountChange => "count-change",
            Self::BeforePublicationFault => "before-publication-fault",
            Self::LostAck => "lost-ack",
        }
    }

    fn changed_length(self) -> u64 {
        match self {
            Self::QualifiedNoop => 0,
            Self::QualifiedOneMib => 1024 * 1024,
            Self::CountChange => 1,
            _ => 1,
        }
    }

    fn uses_seed(self) -> bool {
        self != Self::SymlinkSubstitution
    }
}

#[derive(Clone, Copy)]
struct Roots {
    namespace: ObjectId,
    file: ObjectId,
    length: u64,
    references: u64,
}

#[derive(Debug, Eq, PartialEq)]
struct CanonicalRangeBinding {
    store_instance_id: [u8; 16],
    validation_authority_id: [u8; 32],
    profile: [u8; 32],
    integrity_epoch: u64,
    generation: u64,
    receipt: [u8; 216],
    transition: ObjectId,
    open_identity: u64,
    authority_serial: u64,
    mutation_serial: u64,
    parent_root: ObjectId,
    parent_file_root: ObjectId,
    parent_length: u64,
    parent_references: u64,
    parent_digest: [u8; 32],
    target_root: ObjectId,
    target_file_root: ObjectId,
    target_length: u64,
    target_references: u64,
    target_digest: [u8; 32],
    range_start: u64,
    range_end: u64,
}

struct CanonicalRangeProof {
    binding: CanonicalRangeBinding,
    commitment: [u8; 32],
}

#[derive(Default)]
struct FaultInjection {
    clone_reopen_failure: bool,
    #[cfg(test)]
    cleanup_failure: bool,
    directory_sync_failure: bool,
    reconciliation_identity_mutation: bool,
    reconciliation_name_substitution: bool,
    rename_failure: bool,
}

struct Prepared {
    scenario: Scenario,
    directory_path: PathBuf,
    directory: File,
    store: Store,
    parent: Roots,
    target: Roots,
    target_digest: [u8; 32],
    parent_digest: [u8; 32],
    patch: std::ops::Range<u64>,
    seed: Option<VerifiedSeed>,
    permit_key: Option<[u8; 32]>,
    permit: Option<Permit>,
    generation: u64,
    seed_storage: StorageBytes,
    seed_files_created: u64,
    seed_files_removed: u64,
    fault: FaultInjection,
}

fn write_fixture(path: &Path, size: u64) -> AnyResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut offset = 0_u64;
    while offset < size {
        fill_retained_buffer(&mut buffer, offset, "G3-v1");
        let take = usize::try_from(
            size.checked_sub(offset)
                .ok_or(CoreError::LengthOverflow)?
                .min(u64::try_from(buffer.len()).map_err(|_| CoreError::LengthOverflow)?),
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        file.write_all(&buffer[..take])?;
        offset = offset
            .checked_add(u64::try_from(take).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    file.sync_all()?;
    Ok(())
}

fn hash_file(path: &Path) -> AnyResult<(u64, [u8; 32], u32)> {
    let mut file = File::open(path)?;
    let mode = file.metadata()?.permissions().mode() & 0o7777;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 1024 * 1024];
    let mut length = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        length = length
            .checked_add(u64::try_from(read).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok((length, *hasher.finalize().as_bytes(), mode))
}

fn choose_same_count_patch(
    parent: &Path,
    target: &Path,
    size: u64,
    patch_length: u64,
) -> AnyResult<std::ops::Range<u64>> {
    let parent_references = source_cdc_sequence(parent)?.0;
    choose_same_count_patch_with(parent, target, size, patch_length, |target| {
        Ok(source_cdc_sequence(target)?.0 == parent_references)
    })
}

fn choose_same_count_patch_with(
    parent: &Path,
    target: &Path,
    size: u64,
    patch_length: u64,
    mut accepts: impl FnMut(&Path) -> AnyResult<bool>,
) -> AnyResult<std::ops::Range<u64>> {
    if patch_length == 0 || patch_length > size {
        return Err(CoreError::InvalidRange {
            start: 0,
            end: patch_length,
            length: size,
        }
        .into());
    }
    let length = usize::try_from(patch_length).map_err(|_| CoreError::LengthOverflow)?;
    for attempt in 0_u64..64 {
        fs::copy(parent, target)?;
        let slots = size
            .checked_sub(patch_length)
            .ok_or(CoreError::LengthOverflow)?
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let start = if patch_length == 1 {
            (size / 2)
                .checked_add(
                    attempt
                        .checked_mul(1_048_583)
                        .ok_or(CoreError::LengthOverflow)?,
                )
                .ok_or(CoreError::LengthOverflow)?
                % slots
        } else {
            size.checked_sub(patch_length)
                .ok_or(CoreError::LengthOverflow)?
                / 2
        };
        let mut original = read_source_segment(parent, start, length)?;
        let mask = u8::try_from(attempt + 1).map_err(|_| CoreError::LengthOverflow)?;
        for byte in &mut original {
            *byte ^= mask;
        }
        let mut file = OpenOptions::new().write(true).open(target)?;
        file.seek(SeekFrom::Start(start))?;
        file.write_all(&original)?;
        file.sync_all()?;
        drop(file);
        if accepts(target)? {
            return Ok(start..start + patch_length);
        }
    }
    Err("could not prepare an equal-reference-count patch".into())
}

fn verify_exact_patch_relation(
    parent: &Path,
    target: &Path,
    range: &std::ops::Range<u64>,
) -> AnyResult<()> {
    let mut parent = File::open(parent)?;
    let mut target = File::open(target)?;
    verify_exact_patch_relation_files(&mut parent, &mut target, range)
}

fn verify_exact_patch_relation_files(
    parent: &mut File,
    target: &mut File,
    range: &std::ops::Range<u64>,
) -> AnyResult<()> {
    let parent_length = parent.metadata()?.len();
    let target_length = target.metadata()?.len();
    let maximum_length = parent_length.max(target_length);
    if range.start > range.end || range.end > maximum_length {
        return Err(CoreError::InvalidRange {
            start: range.start,
            end: range.end,
            length: maximum_length,
        }
        .into());
    }
    parent.seek(SeekFrom::Start(0))?;
    target.seek(SeekFrom::Start(0))?;
    let mut parent_buffer = [0_u8; 64 * 1024];
    let mut target_buffer = [0_u8; 64 * 1024];
    let mut offset = 0_u64;
    let mut changed = false;
    while offset < maximum_length {
        let take = usize::try_from(
            maximum_length
                .checked_sub(offset)
                .ok_or(CoreError::LengthOverflow)?
                .min(u64::try_from(parent_buffer.len()).map_err(|_| CoreError::LengthOverflow)?),
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        let parent_read = usize::try_from(parent_length.saturating_sub(offset))
            .map_err(|_| CoreError::LengthOverflow)?
            .min(take);
        let target_read = usize::try_from(target_length.saturating_sub(offset))
            .map_err(|_| CoreError::LengthOverflow)?
            .min(take);
        parent.read_exact(&mut parent_buffer[..parent_read])?;
        target.read_exact(&mut target_buffer[..target_read])?;
        for index in 0..take {
            let position = offset
                .checked_add(u64::try_from(index).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?;
            if parent_buffer.get(index).filter(|_| index < parent_read)
                != target_buffer.get(index).filter(|_| index < target_read)
            {
                if position < range.start || position >= range.end {
                    return Err(CoreError::PublicationConflict.into());
                }
                changed = true;
            }
        }
        offset = offset
            .checked_add(u64::try_from(take).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    if changed == range.is_empty() {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok(())
}

fn prepare_target(
    scenario: Scenario,
    parent: &Path,
    target: &Path,
    size: u64,
) -> AnyResult<std::ops::Range<u64>> {
    match scenario {
        Scenario::QualifiedNoop => {
            fs::copy(parent, target)?;
            Ok(0..0)
        }
        Scenario::CountChange => {
            fs::copy(parent, target)?;
            let mut file = OpenOptions::new().append(true).open(target)?;
            file.write_all(&[0xa5])?;
            file.sync_all()?;
            Ok(size..size + 1)
        }
        _ => choose_same_count_patch(parent, target, size, scenario.changed_length()),
    }
}

fn stream_root(
    store: &mut Store,
    root: ObjectId,
    output: &mut impl Write,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64, [u8; 32])> {
    let file_root = resolve_namespace_file_root(store, root, metrics)?;
    let active_capacity = MAX_DEPTH.checked_add(1).ok_or(CoreError::LengthOverflow)?;
    let _active_charge = charge_dfs_frames(active_capacity, metrics)?;
    let mut active = Vec::with_capacity(active_capacity);
    let mut hasher = blake3::Hasher::new();
    let mut callback = |store: &mut Store,
                        reference: file_codec::FileReference,
                        metrics: &mut Metrics|
     -> AnyResult<()> {
        store.with_borrowed_bytes(reference.object_id, metrics, |canonical, metrics| {
            let raw = layerfs_core::decode_bytes_object(canonical)?;
            if u32::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?
                != reference.raw_length
            {
                return Err(CoreError::ChunkLengthMismatch.into());
            }
            observe_stream_output(metrics, raw.len())?;
            output.write_all(raw)?;
            hasher.update(raw);
            Ok(())
        })
    };
    let (length, references) =
        walk_file_root_references(store, file_root, &mut active, &mut callback, metrics)?;
    Ok((length, references, *hasher.finalize().as_bytes()))
}

struct SourceVerifier<'a> {
    source: &'a mut File,
    buffer: [u8; layerfs_core::cdc::MAXIMUM_CHUNK_BYTES],
}

impl Write for SourceVerifier<'_> {
    fn write(&mut self, canonical: &[u8]) -> std::io::Result<usize> {
        if canonical.len() > self.buffer.len() {
            return Err(std::io::Error::from_raw_os_error(libc::EOVERFLOW));
        }
        self.source
            .read_exact(&mut self.buffer[..canonical.len()])?;
        if self.buffer[..canonical.len()] != *canonical {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "canonical root does not match its source descriptor",
            ));
        }
        Ok(canonical.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn verify_root_matches_source(
    store: &mut Store,
    roots: Roots,
    expected_digest: [u8; 32],
    source: &mut File,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    source.seek(SeekFrom::Start(0))?;
    let _source_buffer_charge = charge_capacity(metrics, layerfs_core::cdc::MAXIMUM_CHUNK_BYTES)?;
    let mut verifier = SourceVerifier {
        source,
        buffer: [0_u8; layerfs_core::cdc::MAXIMUM_CHUNK_BYTES],
    };
    let (length, references, digest) = stream_root(store, roots.namespace, &mut verifier, metrics)?;
    let mut extra = [0_u8; 1];
    if verifier.source.read(&mut extra)? != 0
        || length != roots.length
        || references != roots.references
        || digest != expected_digest
    {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn prove_canonical_range(
    store: &mut Store,
    parent: Roots,
    parent_digest: [u8; 32],
    parent_source: &Path,
    target: Roots,
    target_digest: [u8; 32],
    target_source: &Path,
    range: &std::ops::Range<u64>,
    metrics: &mut Metrics,
) -> AnyResult<CanonicalRangeProof> {
    let mut parent_source = open_readonly_nofollow(parent_source)?;
    let mut target_source = open_readonly_nofollow(target_source)?;
    let parent_before = fstat_file(&parent_source)?.0;
    let target_before = fstat_file(&target_source)?.0;
    if !parent_before.is_regular() || !target_before.is_regular() {
        return Err(CoreError::WrongLogicalRole.into());
    }
    verify_root_matches_source(store, parent, parent_digest, &mut parent_source, metrics)?;
    verify_root_matches_source(store, target, target_digest, &mut target_source, metrics)?;
    {
        let _relation_buffers_charge = charge_capacity(metrics, 2 * 64 * 1024)?;
        verify_exact_patch_relation_files(&mut parent_source, &mut target_source, range)?;
    }
    if fstat_file(&parent_source)?.0 != parent_before
        || fstat_file(&target_source)?.0 != target_before
    {
        return Err(CoreError::IdentityMismatch.into());
    }
    let head = store
        .current_head_accounted(metrics)?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    let binding = canonical_range_binding(
        store,
        &head,
        parent,
        parent_digest,
        target,
        target_digest,
        range,
    );
    let commitment = canonical_range_commitment(&binding);
    Ok(CanonicalRangeProof {
        binding,
        commitment,
    })
}

fn build_and_publish_parent(
    store: &mut Store,
    source: &Path,
    metrics: &mut Metrics,
) -> AnyResult<Roots> {
    let (namespace, transition) = build_file(store, source, SELECTED_PROFILE, metrics)?;
    let file = resolve_namespace_file_root(store, namespace, metrics)?;
    let (length, references) = scrub_file(store, namespace, SELECTED_PROFILE, metrics)?;
    store.publish(None, namespace, transition, metrics)?;
    Ok(Roots {
        namespace,
        file,
        length,
        references,
    })
}

fn build_and_publish_target(
    store: &mut Store,
    source: &Path,
    metrics: &mut Metrics,
) -> AnyResult<Roots> {
    let expected_references = source_cdc_sequence(source)?.0;
    store.transaction_attempt(metrics, |store, metrics| {
        let prior = store
            .current_head_accounted(metrics)?
            .ok_or(CoreError::InvalidValidationReceipt)?;
        let before = resolve_namespace_file_root(store, prior.1, metrics)?;
        let mut builder = FileBuilder::new(SELECTED_PROFILE, expected_references, metrics)?;
        let _cdc_charge = charge_capacity(metrics, 32 * 1024)?;
        FastCdc::new().scan(File::open(source)?, |chunk| {
            builder
                .push_bytes(store, chunk, metrics)
                .map_err(|error| core_failure(error.as_ref()))
        })?;
        let file = builder.finish(store, metrics)?;
        let namespace = namespace_file_root(store, file, metrics)?;
        let (operations, _operations_charge) =
            charged_replace_operation(b"file", before, file, metrics)?;
        let transition = publish_transition_with_operations(
            store,
            Some(prior.1),
            namespace,
            &operations,
            metrics,
        )?;
        verify_transition(
            store,
            transition,
            Some(prior.1),
            namespace,
            Some(&operations),
            metrics,
        )?;
        let (length, references) = scrub_file(store, namespace, SELECTED_PROFILE, metrics)?;
        store.publish(Some(&prior), namespace, transition, metrics)?;
        Ok(Roots {
            namespace,
            file,
            length,
            references,
        })
    })
}

fn materialize_for_preparation(
    store: &mut Store,
    root: ObjectId,
    path: &Path,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64, [u8; 32])> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    let result = stream_root(store, root, &mut output, metrics)?;
    output.sync_all()?;
    output.set_permissions(fs::Permissions::from_mode(MODE))?;
    output.sync_all()?;
    Ok(result)
}

fn create_verified_seed(
    store: &mut Store,
    roots: Roots,
    directory: &File,
    expected_digest: [u8; 32],
    metrics: &mut Metrics,
    #[cfg(test)] fail_after_create: bool,
) -> AnyResult<VerifiedSeed> {
    let name = random_name(".g3-seed-")?;
    let mut cleanup = TempName {
        directory: directory.try_clone()?,
        name: name.clone(),
        active: false,
    };
    let mut output = openat_file(
        directory,
        &name,
        libc::O_WRONLY | libc::O_CLOEXEC | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
        0o600,
    )?;
    cleanup.active = true;
    #[cfg(test)]
    if fail_after_create {
        return Err(std::io::Error::from_raw_os_error(libc::EIO).into());
    }
    let (length, references, digest) = stream_root(store, roots.namespace, &mut output, metrics)?;
    sync_fd(&output)?;
    chmod_fd(&output, MODE)?;
    sync_fd(&output)?;
    if length != roots.length || references != roots.references || digest != expected_digest {
        return Err(CoreError::PublicationConflict.into());
    }
    drop(output);
    let seed = openat_file(
        directory,
        &name,
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        0,
    )?;
    let flags = unsafe { libc::fcntl(seed.as_raw_fd(), libc::F_GETFL) };
    if flags == -1 || flags & libc::O_ACCMODE != libc::O_RDONLY {
        return Err(CoreError::ValidationAuthorityUnavailable.into());
    }
    let mut verifier = seed.try_clone()?;
    verifier.seek(SeekFrom::Start(0))?;
    let mut hasher = blake3::Hasher::new();
    let mut verified_length = 0_u64;
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = verifier.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        verified_length = verified_length
            .checked_add(u64::try_from(read).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    if verified_length != roots.length || hasher.finalize().as_bytes() != &expected_digest {
        return Err(CoreError::IdentityMismatch.into());
    }
    if !unlink_at(directory, &name)? {
        return Err(CoreError::ValidationAuthorityUnavailable.into());
    }
    cleanup.active = false;
    let (native, storage) = fstat_file(&seed)?;
    if !native.is_regular() || native.links != 0 || native.length != roots.length {
        return Err(CoreError::ValidationAuthorityUnavailable.into());
    }
    Ok(VerifiedSeed {
        file: seed,
        identity: SeedIdentity {
            native,
            namespace_root: roots.namespace,
            file_root: roots.file,
            length,
            references,
            digest,
        },
        storage,
    })
}

#[allow(clippy::too_many_arguments)]
fn mint_permit(
    store: &Store,
    directory: &File,
    destination: NativeIdentity,
    parent: Roots,
    target: Roots,
    target_digest: [u8; 32],
    scenario: Scenario,
    patch: &std::ops::Range<u64>,
    seed: &VerifiedSeed,
    proof: CanonicalRangeProof,
    metrics: &mut Metrics,
) -> AnyResult<([u8; 32], Permit, u64)> {
    let head = store
        .current_head_accounted(metrics)?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    let (directory_identity, _) = fstat_file(directory)?;
    let expected_proof = canonical_range_binding(
        store,
        &head,
        parent,
        seed.identity.digest,
        target,
        target_digest,
        patch,
    );
    if proof.binding != expected_proof
        || proof.commitment != canonical_range_commitment(&proof.binding)
    {
        return Err(CoreError::ValidationAuthorityUnavailable.into());
    }
    let key = os_random()?;
    let binding = PermitBinding {
        store_instance_id: store.store_instance_id,
        validation_authority_id: store.validation_authority_id,
        profile: store.profile,
        integrity_epoch: store.integrity_epoch,
        generation: head.0,
        receipt: head.3,
        transition: head.2,
        parent_root: parent.namespace,
        parent_file_root: parent.file,
        target_root: target.namespace,
        target_file_root: target.file,
        directory: directory_identity,
        basename: DESTINATION_NAME.to_string(),
        destination,
        open_identity: store.open_identity,
        authority_serial: store.same_open_authority_serial,
        mutation_serial: store.mutation_serial,
        publication_serial: random_u64()?,
        operation: scenario.name().to_string(),
        range_start: patch.start,
        range_end: patch.end,
        canonical_range_commitment: proof.commitment,
        nonce: os_random()?,
        seed: seed.identity.clone(),
    };
    let tag = permit_tag(&key, &binding)?;
    Ok((
        key,
        Permit {
            binding,
            tag,
            consumed: false,
        },
        head.0,
    ))
}

fn prepare(root: &Path, size: u64, scenario: Scenario) -> AnyResult<Prepared> {
    if size == 0 {
        return Err("G3 SIZE must be positive bytes".into());
    }
    fs::create_dir_all(root)?;
    let directory_path = root.join(format!("g3-{}", scenario.name()));
    fs::create_dir(&directory_path).map_err(|error| {
        format!(
            "refusing to reuse G3 operand directory {}: {error}",
            directory_path.display()
        )
    })?;
    let parent_path = directory_path.join("parent.source");
    let target_path = directory_path.join("target.source");
    write_fixture(&parent_path, size)?;
    let patch = prepare_target(scenario, &parent_path, &target_path, size)?;
    if scenario != Scenario::CountChange {
        verify_exact_patch_relation(&parent_path, &target_path, &patch)?;
    }
    let (_, parent_digest, _) = hash_file(&parent_path)?;
    let (_, target_digest, _) = hash_file(&target_path)?;

    let mut prep_metrics = Metrics::default();
    let mut store = Store::open(&directory_path.join("store.sqlite"), SELECTED_PROFILE)?;
    let parent = build_and_publish_parent(&mut store, &parent_path, &mut prep_metrics)?;
    let target = if scenario == Scenario::QualifiedNoop {
        parent
    } else {
        build_and_publish_target(&mut store, &target_path, &mut prep_metrics)?
    };
    if parent.length != size
        || target.length
            != if scenario == Scenario::CountChange {
                size.checked_add(1).ok_or(CoreError::LengthOverflow)?
            } else {
                size
            }
    {
        return Err(CoreError::LengthMismatch {
            expected: size,
            actual: target.length,
        }
        .into());
    }
    if !matches!(scenario, Scenario::QualifiedNoop | Scenario::CountChange)
        && parent.references != target.references
    {
        return Err("prepared same-size target changed reference count".into());
    }

    let range_proof = if scenario.uses_seed() {
        Some(prove_canonical_range(
            &mut store,
            parent,
            parent_digest,
            &parent_path,
            target,
            target_digest,
            &target_path,
            &patch,
            &mut prep_metrics,
        )?)
    } else {
        None
    };

    let destination_path = directory_path.join(DESTINATION_NAME);
    let (destination_length, _, destination_digest) = materialize_for_preparation(
        &mut store,
        parent.namespace,
        &destination_path,
        &mut prep_metrics,
    )?;
    if destination_length != parent.length || destination_digest != parent_digest {
        return Err(CoreError::PublicationConflict.into());
    }
    let directory = open_dir(&directory_path)?;
    let destination =
        stat_at(&directory, DESTINATION_NAME)?.ok_or(CoreError::ValidationAuthorityUnavailable)?;
    if !destination.is_regular() {
        return Err(CoreError::WrongLogicalRole.into());
    }

    let (
        seed,
        permit_key,
        permit,
        generation,
        seed_storage,
        seed_files_created,
        seed_files_removed,
    ) = if scenario.uses_seed() {
        let seed = create_verified_seed(
            &mut store,
            parent,
            &directory,
            parent_digest,
            &mut prep_metrics,
            #[cfg(test)]
            false,
        )?;
        let storage = seed.storage;
        let (key, mut permit, generation) = mint_permit(
            &store,
            &directory,
            destination,
            parent,
            target,
            target_digest,
            scenario,
            &patch,
            &seed,
            range_proof.ok_or(CoreError::ValidationAuthorityUnavailable)?,
            &mut prep_metrics,
        )?;
        if scenario == Scenario::InvalidAuthority {
            permit.tag[0] ^= 1;
        }
        (
            Some(seed),
            Some(key),
            Some(permit),
            generation,
            storage,
            1,
            1,
        )
    } else {
        let generation = store
            .current_head_accounted(&mut prep_metrics)?
            .ok_or(CoreError::InvalidValidationReceipt)?
            .0;
        (None, None, None, generation, StorageBytes::default(), 0, 0)
    };

    if scenario == Scenario::ExternalMutation {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&destination_path)?;
        let mut byte = [0_u8; 1];
        file.read_exact(&mut byte)?;
        byte[0] ^= 0xff;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&byte)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.sync_all()?;
    } else if scenario == Scenario::SymlinkSubstitution {
        let sentinel = directory_path.join("sentinel.bin");
        fs::rename(&destination_path, &sentinel)?;
        std::os::unix::fs::symlink("sentinel.bin", &destination_path)?;
    }

    finish_q(&mut Metrics::default())?;
    Ok(Prepared {
        scenario,
        directory_path,
        directory,
        store,
        parent,
        target,
        target_digest,
        parent_digest,
        patch,
        seed,
        permit_key,
        permit,
        generation,
        seed_storage,
        seed_files_created,
        seed_files_removed,
        fault: FaultInjection::default(),
    })
}

fn validate_permit(
    prepared: &mut Prepared,
    metrics: &mut Metrics,
    counters: &mut Counters,
) -> AnyResult<bool> {
    counters.authority_validations = counters
        .authority_validations
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    let Some(permit) = prepared.permit.as_mut() else {
        counters.authority_validation_failures += 1;
        return Ok(false);
    };
    if permit.consumed {
        counters.authority_validation_failures += 1;
        return Ok(false);
    }
    let head = prepared
        .store
        .current_head_accounted(metrics)?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    counters.authority_reads = counters
        .authority_reads
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    counters.authority_bytes_read = counters
        .authority_bytes_read
        .checked_add(8 + 32 + 32 + 216)
        .ok_or(CoreError::LengthOverflow)?;
    let Some(seed) = prepared.seed.as_ref() else {
        counters.authority_validation_failures += 1;
        return Ok(false);
    };
    let Ok((seed_native, _)) = fstat_file(&seed.file) else {
        counters.authority_validation_failures += 1;
        return Ok(false);
    };
    counters.seed_authority_reads = counters
        .seed_authority_reads
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    counters.seed_authority_bytes_read = counters
        .seed_authority_bytes_read
        .checked_add(u64::try_from(std::mem::size_of::<libc::stat>())?)
        .ok_or(CoreError::LengthOverflow)?;
    let Some(key) = prepared.permit_key.as_ref() else {
        counters.authority_validation_failures += 1;
        return Ok(false);
    };
    let directory_native = fstat_file(&prepared.directory).ok().map(|value| value.0);
    let expected_range_commitment = canonical_range_commitment(&canonical_range_binding(
        &prepared.store,
        &head,
        prepared.parent,
        prepared.parent_digest,
        prepared.target,
        prepared.target_digest,
        &prepared.patch,
    ));
    // SAFETY: the retained seed descriptor is live for this synchronous validation.
    let seed_flags = unsafe { libc::fcntl(seed.file.as_raw_fd(), libc::F_GETFL) };
    let valid = permit_tag(key, &permit.binding)? == permit.tag
        && permit.binding.store_instance_id == prepared.store.store_instance_id
        && permit.binding.validation_authority_id == prepared.store.validation_authority_id
        && permit.binding.profile == prepared.store.profile
        && permit.binding.integrity_epoch == prepared.store.integrity_epoch
        && permit.binding.generation == head.0
        && permit.binding.receipt == head.3
        && permit.binding.transition == head.2
        && permit.binding.parent_root == prepared.parent.namespace
        && permit.binding.parent_file_root == prepared.parent.file
        && permit.binding.target_root == prepared.target.namespace
        && permit.binding.target_file_root == prepared.target.file
        && permit.binding.basename == DESTINATION_NAME
        && permit.binding.open_identity == prepared.store.open_identity
        && permit.binding.authority_serial == prepared.store.same_open_authority_serial
        && permit.binding.mutation_serial == prepared.store.mutation_serial
        && permit.binding.operation == prepared.scenario.name()
        && permit.binding.range_start == prepared.patch.start
        && permit.binding.range_end == prepared.patch.end
        && permit.binding.canonical_range_commitment == expected_range_commitment
        && directory_native.is_some_and(|identity| {
            identity.is_directory()
                && identity.device == permit.binding.directory.device
                && identity.inode == permit.binding.directory.inode
        })
        && permit.binding.seed.native == seed_native
        && seed_native.is_regular()
        && seed_native.links == 0
        && seed_native.length == seed.identity.length
        && seed_flags != -1
        && seed_flags & libc::O_ACCMODE == libc::O_RDONLY
        && permit.binding.seed.namespace_root == seed.identity.namespace_root
        && permit.binding.seed.file_root == seed.identity.file_root
        && permit.binding.seed.length == seed.identity.length
        && permit.binding.seed.references == seed.identity.references
        && permit.binding.seed.digest == seed.identity.digest;
    if valid {
        counters.authority_validation_successes += 1;
    } else {
        counters.authority_validation_failures += 1;
    }
    counters.q_high_water = counters.q_high_water.max(metrics.q_high_water);
    Ok(valid)
}

fn consume_permit(prepared: &mut Prepared, counters: &mut Counters) -> AnyResult<()> {
    let permit = prepared
        .permit
        .as_mut()
        .ok_or(CoreError::ValidationAuthorityUnavailable)?;
    if permit.consumed {
        return Err(CoreError::ValidationAuthorityUnavailable.into());
    }
    permit.consumed = true;
    counters.permit_consumptions = counters
        .permit_consumptions
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn hash_at(directory: &File, name: &str) -> AnyResult<(u64, [u8; 32], u32)> {
    let mut file = openat_file(
        directory,
        name,
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK | libc::O_NOFOLLOW,
        0,
    )?;
    let identity = fstat_file(&file)?.0;
    if !identity.is_regular() {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let mode = u32::from(identity.mode & 0o7777);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 1024 * 1024];
    let mut length = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        length = length
            .checked_add(u64::try_from(read).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok((length, *hasher.finalize().as_bytes(), mode))
}

fn compare_destination_to_root(
    store: &mut Store,
    directory: &File,
    root: ObjectId,
    metrics: &mut Metrics,
    counters: &mut Counters,
    mutate_identity: bool,
    substitute_name: bool,
) -> AnyResult<bool> {
    let mut destination = openat_file(
        directory,
        DESTINATION_NAME,
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK | libc::O_NOFOLLOW,
        0,
    )?;
    let identity = fstat_file(&destination)?.0;
    if !identity.is_regular() {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let file_root = resolve_namespace_file_root(store, root, metrics)?;
    let active_capacity = MAX_DEPTH.checked_add(1).ok_or(CoreError::LengthOverflow)?;
    let _active_charge = charge_dfs_frames(active_capacity, metrics)?;
    let mut active = Vec::with_capacity(active_capacity);
    let _buffer_charge = charge_capacity(metrics, RECONCILIATION_COMPARISON_BYTES)?;
    let mut buffer = [0_u8; RECONCILIATION_COMPARISON_BYTES];
    let mut equal = u32::from(identity.mode & 0o7777) == MODE;
    let mut destination_eof = false;
    let mut callback = |store: &mut Store,
                        reference: file_codec::FileReference,
                        metrics: &mut Metrics|
     -> AnyResult<()> {
        store.with_borrowed_bytes(reference.object_id, metrics, |canonical, _| {
            let raw = layerfs_core::decode_bytes_object(canonical)?;
            if u32::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?
                != reference.raw_length
            {
                return Err(CoreError::ChunkLengthMismatch.into());
            }
            if raw.len() > buffer.len() {
                return Err(CoreError::ChunkLengthMismatch.into());
            }
            counters.reconciliation_source_bytes_compared = counters
                .reconciliation_source_bytes_compared
                .checked_add(u64::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?;
            let mut read = 0;
            while read < raw.len() && !destination_eof {
                match destination.read(&mut buffer[read..raw.len()]) {
                    Ok(0) => destination_eof = true,
                    Ok(amount) => {
                        read = read.checked_add(amount).ok_or(CoreError::LengthOverflow)?;
                        counters.destination_bytes_read = counters
                            .destination_bytes_read
                            .checked_add(
                                u64::try_from(amount).map_err(|_| CoreError::LengthOverflow)?,
                            )
                            .ok_or(CoreError::LengthOverflow)?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            if read != raw.len() || buffer[..read] != raw[..read] {
                equal = false;
            }
            Ok(())
        })
    };
    let (length, _) =
        walk_file_root_references(store, file_root, &mut active, &mut callback, metrics)?;
    let mut extra = [0_u8; 1];
    let extra = destination.read(&mut extra)?;
    counters.destination_bytes_read = counters
        .destination_bytes_read
        .checked_add(u64::try_from(extra).map_err(|_| CoreError::LengthOverflow)?)
        .ok_or(CoreError::LengthOverflow)?;
    if mutate_identity {
        chmod_fd(&destination, 0o600)?;
    }
    if substitute_name {
        let hidden = random_name(".g3-reconciliation-hidden-")?;
        rename_at(directory, DESTINATION_NAME, &hidden)?;
        let mut replacement = openat_file(
            directory,
            DESTINATION_NAME,
            libc::O_WRONLY | libc::O_CLOEXEC | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
            MODE,
        )?;
        replacement.write_all(b"substituted")?;
    }
    let after = fstat_file(&destination)?.0;
    let named = stat_at(directory, DESTINATION_NAME)?;
    if after != identity || named != Some(after) {
        return Err(CoreError::AmbiguousDurability.into());
    }
    Ok(equal && extra == 0 && identity.length == length)
}

enum NativeReconciliation {
    Target,
    Prior,
}

fn reconcile_native_publication(
    prepared: &mut Prepared,
    counters: &mut Counters,
) -> AnyResult<NativeReconciliation> {
    counters.reconciliation_calls = counters
        .reconciliation_calls
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    let mut metrics = Metrics::default();
    let mutate_identity = std::mem::take(&mut prepared.fault.reconciliation_identity_mutation);
    let substitute_name = std::mem::take(&mut prepared.fault.reconciliation_name_substitution);
    let target = compare_destination_to_root(
        &mut prepared.store,
        &prepared.directory,
        prepared.target.namespace,
        &mut metrics,
        counters,
        mutate_identity,
        substitute_name,
    )
    .map_err(|_| CoreError::AmbiguousDurability)?;
    if target {
        counters.absorb_reconciliation(&metrics)?;
        return Ok(NativeReconciliation::Target);
    }
    let prior = compare_destination_to_root(
        &mut prepared.store,
        &prepared.directory,
        prepared.parent.namespace,
        &mut metrics,
        counters,
        false,
        false,
    )
    .map_err(|_| CoreError::AmbiguousDurability)?;
    counters.absorb_reconciliation(&metrics)?;
    if prior {
        Ok(NativeReconciliation::Prior)
    } else {
        Err(CoreError::PublicationConflict.into())
    }
}

fn count_residue(path: &Path, prefix: &str) -> AnyResult<u64> {
    fs::read_dir(path)?.try_fold(0_u64, |count, entry| {
        let entry = entry?;
        Ok(
            if entry.file_name().as_bytes().starts_with(prefix.as_bytes()) {
                count.checked_add(1).ok_or(CoreError::LengthOverflow)?
            } else {
                count
            },
        )
    })
}

struct OperationResult {
    counters: Counters,
    timers: Timers,
    operation_total_ns: u128,
    route: &'static str,
    reason: &'static str,
    outcome: &'static str,
    error: Option<&'static str>,
    reconciliation: &'static str,
    old_or_new: &'static str,
    temp_storage: StorageBytes,
}

fn run_operation(prepared: &mut Prepared) -> AnyResult<OperationResult> {
    let operation_started = Instant::now();
    let mut counters = Counters {
        seed_files_created: prepared.seed_files_created,
        seed_files_removed: prepared.seed_files_removed,
        ..Counters::default()
    };
    let mut timers = Timers::default();
    let preflight_started = Instant::now();
    let destination = stat_at(&prepared.directory, DESTINATION_NAME)?;
    if destination.is_some_and(NativeIdentity::is_symlink) {
        timers.preflight = preflight_started.elapsed().as_nanos();
        return Ok(OperationResult {
            counters,
            timers,
            operation_total_ns: operation_started.elapsed().as_nanos(),
            route: "typed-rejection",
            reason: "destination-symlink",
            outcome: "typed-error",
            error: Some("NativeDestinationSymlink"),
            reconciliation: "not-needed",
            old_or_new: "old",
            temp_storage: StorageBytes::default(),
        });
    }
    if destination.is_some_and(|identity| !identity.is_regular()) {
        timers.preflight = preflight_started.elapsed().as_nanos();
        return Ok(OperationResult {
            counters,
            timers,
            operation_total_ns: operation_started.elapsed().as_nanos(),
            route: "typed-rejection",
            reason: "destination-wrong-kind",
            outcome: "typed-error",
            error: Some("NativeDestinationWrongKind"),
            reconciliation: "not-needed",
            old_or_new: "old",
            temp_storage: StorageBytes::default(),
        });
    }
    timers.preflight = preflight_started.elapsed().as_nanos();

    let qualification_started = Instant::now();
    let mut authority_metrics = Metrics::default();
    let valid = validate_permit(prepared, &mut authority_metrics, &mut counters)?;
    let destination_invalidated = prepared
        .permit
        .as_ref()
        .is_none_or(|permit| Some(permit.binding.destination) != destination);
    let count_change = prepared.parent.length != prepared.target.length
        || prepared.parent.references != prepared.target.references;
    let qualified = valid && !destination_invalidated && !count_change;
    counters.q_high_water = counters.q_high_water.max(authority_metrics.q_high_water);
    timers.qualification = qualification_started.elapsed().as_nanos();

    let mut route = if qualified {
        if prepared.patch.is_empty() {
            "qualified-noop"
        } else {
            "qualified-patch"
        }
    } else {
        "complete-fallback"
    };
    let mut reason = if !valid {
        "invalid-authority"
    } else if destination_invalidated {
        "destination-invalidated"
    } else if count_change {
        "count-change"
    } else {
        "seed-hit"
    };
    let mut outcome = "success";
    let mut error = None;
    let mut reconciliation = "not-needed";
    let mut old_or_new = "new";

    let payload_started = Instant::now();
    let mut payload_metrics = Metrics::default();
    let mut candidate = if qualified {
        let fail_reopen = std::mem::take(&mut prepared.fault.clone_reopen_failure);
        clone_temp(
            prepared
                .seed
                .as_ref()
                .ok_or(CoreError::ValidationAuthorityUnavailable)?,
            &prepared.directory,
            &mut counters,
            fail_reopen,
        )?
    } else {
        None
    };
    if qualified && candidate.is_some() {
        consume_permit(prepared, &mut counters)?;
    }
    if qualified && candidate.is_none() {
        route = "complete-fallback";
        reason = "clone-failed";
    }
    if candidate.is_none() {
        let (mut file, temp) = create_temp(&prepared.directory, &mut counters)?;
        let (length, references, digest) = stream_root(
            &mut prepared.store,
            prepared.target.namespace,
            &mut file,
            &mut payload_metrics,
        )?;
        if length != prepared.target.length
            || references != prepared.target.references
            || digest != prepared.target_digest
        {
            return Err(CoreError::PublicationConflict.into());
        }
        counters.fallback_calls = counters
            .fallback_calls
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        counters.source_bytes_reconstructed = counters
            .source_bytes_reconstructed
            .checked_add(length)
            .ok_or(CoreError::LengthOverflow)?;
        counters.fallback_write_bytes = counters
            .fallback_write_bytes
            .checked_add(length)
            .ok_or(CoreError::LengthOverflow)?;
        candidate = Some((file, temp));
    } else if !prepared.patch.is_empty() {
        let bytes = read_file_range(
            &prepared.store,
            prepared.target.file,
            SELECTED_PROFILE,
            prepared.patch.clone(),
            &mut payload_metrics,
        )?;
        let expected = usize::try_from(
            prepared
                .patch
                .end
                .checked_sub(prepared.patch.start)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        if bytes.len() != expected {
            return Err(CoreError::LengthMismatch {
                expected: u64::try_from(expected).map_err(|_| CoreError::LengthOverflow)?,
                actual: u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?,
            }
            .into());
        }
        let file = &candidate
            .as_ref()
            .ok_or(CoreError::ValidationAuthorityUnavailable)?
            .0;
        pwrite_all(
            file,
            prepared.patch.start,
            &bytes,
            &mut counters.patch_calls,
        )?;
        counters.patch_bytes = counters
            .patch_bytes
            .checked_add(u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
        counters.changed_ranges = 1;
        counters.changed_bytes = counters.patch_bytes;
    }
    timers.payload_prepare = payload_started.elapsed().as_nanos();
    counters.absorb_payload(&payload_metrics)?;

    let (file, mut temp) = candidate.ok_or(CoreError::ValidationAuthorityUnavailable)?;
    let data_sync_started = Instant::now();
    sync_fd(&file)?;
    counters.data_sync_calls = 1;
    timers.data_sync = data_sync_started.elapsed().as_nanos();

    let metadata_started = Instant::now();
    chmod_fd(&file, MODE)?;
    counters.metadata_operations = 1;
    timers.metadata = metadata_started.elapsed().as_nanos();

    let metadata_sync_started = Instant::now();
    sync_fd(&file)?;
    counters.metadata_sync_calls = 1;
    timers.metadata_sync = metadata_sync_started.elapsed().as_nanos();
    let temp_storage = fstat_file(&file)?.1;

    if prepared.scenario == Scenario::BeforePublicationFault {
        outcome = "typed-error";
        error = Some("InjectedBeforePublication");
        old_or_new = "old";
        let cleanup_started = Instant::now();
        drop(file);
        temp.remove(&mut counters)?;
        timers.cleanup = cleanup_started.elapsed().as_nanos();
    } else {
        let rename_started = Instant::now();
        let rename_result = if std::mem::take(&mut prepared.fault.rename_failure) {
            Err(std::io::Error::from_raw_os_error(libc::EIO))
        } else {
            rename_at(&prepared.directory, &temp.name, DESTINATION_NAME)
        };
        let mut publication_error = rename_result.err();
        let rename_failed = publication_error.is_some();
        counters.rename_calls = 1;
        if !rename_failed {
            temp.active = false;
        }
        timers.rename = rename_started.elapsed().as_nanos();

        let mut needs_reconciliation = rename_failed || prepared.scenario == Scenario::LostAck;
        if !needs_reconciliation {
            let directory_sync_started = Instant::now();
            counters.directory_sync_calls = counters
                .directory_sync_calls
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            let directory_sync = if std::mem::take(&mut prepared.fault.directory_sync_failure) {
                Err(std::io::Error::from_raw_os_error(libc::EIO))
            } else {
                sync_fd(&prepared.directory)
            };
            if let Err(error) = directory_sync {
                publication_error = Some(error);
                needs_reconciliation = true;
            }
            timers.directory_sync = directory_sync_started.elapsed().as_nanos();
        }
        if needs_reconciliation {
            let reconciliation_started = Instant::now();
            let reconciled = reconcile_native_publication(prepared, &mut counters)?;
            timers.reconciliation = reconciliation_started.elapsed().as_nanos();
            match reconciled {
                NativeReconciliation::Target => {
                    reconciliation = "target";
                    if rename_failed {
                        temp.remove(&mut counters)?;
                    } else {
                        temp.active = false;
                    }
                    let directory_sync_started = Instant::now();
                    counters.directory_sync_calls = counters
                        .directory_sync_calls
                        .checked_add(1)
                        .ok_or(CoreError::LengthOverflow)?;
                    sync_fd(&prepared.directory).map_err(|_| CoreError::AmbiguousDurability)?;
                    timers.directory_sync = timers
                        .directory_sync
                        .checked_add(directory_sync_started.elapsed().as_nanos())
                        .ok_or(CoreError::LengthOverflow)?;
                }
                NativeReconciliation::Prior => {
                    reconciliation = "prior";
                    old_or_new = "old";
                    #[cfg(test)]
                    let cleanup_result = temp.remove_with_fault(
                        &mut counters,
                        std::mem::take(&mut prepared.fault.cleanup_failure),
                    );
                    #[cfg(not(test))]
                    let cleanup_result = temp.remove(&mut counters);
                    let cleanup_error = cleanup_result.err();
                    if let Some(error) = publication_error.take() {
                        if let Some(cleanup) = cleanup_error {
                            return Err(NativePublicationCleanupFailure {
                                publication: error,
                                cleanup,
                            }
                            .into());
                        }
                        return Err(error.into());
                    }
                    if let Some(error) = cleanup_error {
                        return Err(error);
                    }
                }
            }
        }
        let cleanup_started = Instant::now();
        drop(file);
        drop(temp);
        timers.cleanup = cleanup_started.elapsed().as_nanos();
    }

    Ok(OperationResult {
        counters,
        timers,
        operation_total_ns: operation_started.elapsed().as_nanos(),
        route,
        reason,
        outcome,
        error,
        reconciliation,
        old_or_new,
        temp_storage,
    })
}

#[allow(clippy::too_many_arguments)]
fn render_row(
    prepared: &Prepared,
    size: u64,
    result: OperationResult,
    output_length: u64,
    output_mode: u32,
    output_digest: [u8; 32],
    expected_length: u64,
    expected_digest: [u8; 32],
    temp_residue_count: u64,
    seed_residue_count: u64,
) -> AnyResult<String> {
    let counters = result.counters;
    let timers = result.timers;
    let attributed = timers.attributed().ok_or(CoreError::LengthOverflow)?;
    let unattributed = result
        .operation_total_ns
        .checked_sub(attributed)
        .ok_or(CoreError::LengthOverflow)?;
    let payload_sql_queries = counters
        .mapping_sql_queries
        .checked_add(counters.object_sql_queries)
        .ok_or(CoreError::LengthOverflow)?;
    let payload_sql_rows = counters
        .mapping_sql_rows
        .checked_add(counters.object_sql_rows)
        .ok_or(CoreError::LengthOverflow)?;
    let byte_exact = output_length == expected_length && output_digest == expected_digest;
    let mode_exact = output_mode == MODE;
    if !byte_exact || !mode_exact || temp_residue_count != 0 || seed_residue_count != 0 {
        return Err(CoreError::PublicationConflict.into());
    }
    let error = result
        .error
        .map_or_else(|| "null".to_string(), |value| format!("\"{value}\""));
    let bindings = if prepared.scenario == Scenario::SymlinkSubstitution {
        "[]"
    } else {
        AUTHORITY_BINDINGS
    };
    Ok(format!(
        concat!(
            "{{\"schema\":\"phase4-g3-row-v1\",\"scenario\":\"{}\",\"size_bytes\":{},",
            "\"route\":\"{}\",\"outcome\":\"{}\",\"qualification_reason\":\"{}\",\"error\":{},",
            "\"generation\":{},\"parent_root\":\"{}\",\"target_root\":\"{}\",\"authority_bindings_checked\":{},",
            "\"authority_reads\":{},\"authority_bytes_read\":{},\"seed_authority_reads\":{},\"seed_authority_bytes_read\":{},",
            "\"authority_validations\":{},\"authority_validation_successes\":{},\"authority_validation_failures\":{},\"permit_consumptions\":{},",
            "\"mapping_sql_queries\":{},\"mapping_sql_rows\":{},\"object_sql_queries\":{},\"object_sql_rows\":{},",
            "\"payload_sql_queries\":{},\"payload_sql_rows\":{},\"canonical_blob_reads\":{},\"canonical_blob_bytes\":{},",
            "\"authenticated_objects\":{},\"canonical_bytes_authenticated\":{},\"source_bytes_reconstructed\":{},",
            "\"destination_bytes_read\":{},\"verification_bytes_read\":{},\"clone_calls\":{},\"clone_successes\":{},\"clone_failures\":{},",
            "\"clone_source_logical_bytes\":{},\"copy_calls\":{},\"copied_payload_bytes\":{},\"patch_calls\":{},\"patch_bytes\":{},",
            "\"fallback_calls\":{},\"fallback_write_bytes\":{},\"changed_ranges\":{},\"changed_bytes\":{},\"metadata_operations\":{},",
            "\"temp_files_created\":{},\"temp_files_removed\":{},\"seed_files_created\":{},\"seed_files_removed\":{},",
            "\"data_sync_calls\":{},\"metadata_sync_calls\":{},\"rename_calls\":{},\"directory_sync_calls\":{},\"reconciliation_calls\":{},",
            "\"reconciliation_sql_queries\":{},\"reconciliation_sql_rows\":{},\"reconciliation_blob_reads\":{},",
            "\"reconciliation_canonical_bytes_authenticated\":{},\"reconciliation_source_bytes_compared\":{},\"reconciliation_q_high_water\":{},",
            "\"reconciliation_outcome\":\"{}\",\"q_high_water\":{},\"q_terminal\":0,",
            "\"temp_logical_bytes\":{},\"temp_apparent_bytes\":{},\"temp_allocated_bytes\":{},",
            "\"seed_logical_bytes\":{},\"seed_apparent_bytes\":{},\"seed_allocated_bytes\":{},",
            "\"output_length\":{},\"output_mode\":{},\"output_digest\":\"{}\",\"expected_output_digest\":\"{}\",",
            "\"byte_exact\":{},\"mode_exact\":{},\"old_or_new\":\"{}\",\"temp_residue_count\":{},\"seed_residue_count\":{},",
            "\"timer_preflight_ns\":{},\"timer_qualification_ns\":{},\"timer_payload_prepare_ns\":{},\"timer_data_sync_ns\":{},",
            "\"timer_metadata_ns\":{},\"timer_metadata_sync_ns\":{},\"timer_rename_ns\":{},\"timer_directory_sync_ns\":{},",
            "\"timer_reconciliation_ns\":{},\"timer_cleanup_ns\":{},\"attributed_wall_ns\":{},\"unattributed_wall_ns\":{},\"operation_total_ns\":{},",
            "\"physical_io_status\":\"Unavailable: physical I/O is not derivable from logical clone and write counters\",",
            "\"cache_warmth_status\":\"Unavailable: selected APIs do not identify OS cache residency\",",
            "\"stable_media_status\":\"Unavailable: fsync dispatch does not prove device stable-media completion\"}}"
        ),
        prepared.scenario.name(),
        size,
        result.route,
        result.outcome,
        result.reason,
        error,
        prepared.generation,
        prepared.parent.file,
        prepared.target.file,
        bindings,
        counters.authority_reads,
        counters.authority_bytes_read,
        counters.seed_authority_reads,
        counters.seed_authority_bytes_read,
        counters.authority_validations,
        counters.authority_validation_successes,
        counters.authority_validation_failures,
        counters.permit_consumptions,
        counters.mapping_sql_queries,
        counters.mapping_sql_rows,
        counters.object_sql_queries,
        counters.object_sql_rows,
        payload_sql_queries,
        payload_sql_rows,
        counters.canonical_blob_reads,
        counters.canonical_blob_bytes,
        counters.authenticated_objects,
        counters.canonical_bytes_authenticated,
        counters.source_bytes_reconstructed,
        counters.destination_bytes_read,
        counters.verification_bytes_read,
        counters.clone_calls,
        counters.clone_successes,
        counters.clone_failures,
        counters.clone_source_logical_bytes,
        counters.copy_calls,
        counters.copied_payload_bytes,
        counters.patch_calls,
        counters.patch_bytes,
        counters.fallback_calls,
        counters.fallback_write_bytes,
        counters.changed_ranges,
        counters.changed_bytes,
        counters.metadata_operations,
        counters.temp_files_created,
        counters.temp_files_removed,
        counters.seed_files_created,
        counters.seed_files_removed,
        counters.data_sync_calls,
        counters.metadata_sync_calls,
        counters.rename_calls,
        counters.directory_sync_calls,
        counters.reconciliation_calls,
        counters.reconciliation_sql_queries,
        counters.reconciliation_sql_rows,
        counters.reconciliation_blob_reads,
        counters.reconciliation_canonical_bytes_authenticated,
        counters.reconciliation_source_bytes_compared,
        counters.reconciliation_q_high_water,
        result.reconciliation,
        counters.q_high_water,
        result.temp_storage.logical,
        result.temp_storage.apparent,
        result.temp_storage.allocated,
        prepared.seed_storage.logical,
        prepared.seed_storage.apparent,
        prepared.seed_storage.allocated,
        output_length,
        output_mode,
        hex_bytes(&output_digest),
        hex_bytes(&expected_digest),
        byte_exact,
        mode_exact,
        result.old_or_new,
        temp_residue_count,
        seed_residue_count,
        timers.preflight,
        timers.qualification,
        timers.payload_prepare,
        timers.data_sync,
        timers.metadata,
        timers.metadata_sync,
        timers.rename,
        timers.directory_sync,
        timers.reconciliation,
        timers.cleanup,
        attributed,
        unattributed,
        result.operation_total_ns,
    ))
}

pub(super) fn run_g3_row(root: &Path, size: u64, scenario: &str) -> AnyResult<String> {
    let scenario = Scenario::parse(scenario)?;
    let mut prepared = prepare(root, size, scenario)?;
    let mut result = run_operation(&mut prepared)?;
    let verification_name = if scenario == Scenario::SymlinkSubstitution {
        "sentinel.bin"
    } else {
        DESTINATION_NAME
    };
    if scenario == Scenario::SymlinkSubstitution
        && !fs::symlink_metadata(prepared.directory_path.join(DESTINATION_NAME))?
            .file_type()
            .is_symlink()
    {
        return Err(CoreError::PublicationConflict.into());
    }
    let (output_length, output_digest, output_mode) =
        hash_at(&prepared.directory, verification_name)?;
    result.counters.verification_bytes_read = output_length;
    let (expected_length, expected_digest) = if result.old_or_new == "old" {
        (prepared.parent.length, prepared.parent_digest)
    } else {
        (prepared.target.length, prepared.target_digest)
    };
    let temp_residue_count = count_residue(&prepared.directory_path, ".g3-tmp-")?;
    let seed_residue_count = count_residue(&prepared.directory_path, ".g3-seed-")?;
    finish_q(&mut Metrics::default())?;
    render_row(
        &prepared,
        size,
        result,
        output_length,
        output_mode,
        output_digest,
        expected_length,
        expected_digest,
        temp_residue_count,
        seed_residue_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "layerfs-g3-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn fclonefileat_clones_an_unlinked_read_only_seed_fd() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-g3-fclone-smoke-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create temp root");
        let seed_path = root.join("seed");
        let mut writable = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&seed_path)
            .expect("create seed");
        writable
            .write_all(b"authenticated seed")
            .expect("write seed");
        writable.sync_all().expect("sync seed");
        drop(writable);
        let seed = open_readonly_nofollow(&seed_path).expect("reopen seed read-only");
        fs::remove_file(&seed_path).expect("unlink seed");
        let directory = open_dir(&root).expect("open temp root");
        fclone_unlinked(&seed, &directory, std::ffi::OsStr::new("clone"))
            .expect("clone unlinked seed fd");
        let mut bytes = Vec::new();
        File::open(root.join("clone"))
            .expect("open clone")
            .read_to_end(&mut bytes)
            .expect("read clone");
        assert_eq!(bytes, b"authenticated seed");
        drop(directory);
        drop(seed);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g3_rows_cover_qualified_fallback_rejection_and_fault_routes() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-g3-routes-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let cases = [
            ("qualified-noop", "\"route\":\"qualified-noop\""),
            ("invalid-authority", "\"fallback_calls\":1"),
            (
                "symlink-substitution",
                "\"error\":\"NativeDestinationSymlink\"",
            ),
            ("count-change", "\"output_length\":131073"),
            (
                "before-publication-fault",
                "\"error\":\"InjectedBeforePublication\"",
            ),
            ("lost-ack", "\"reconciliation_outcome\":\"target\""),
        ];
        for (scenario, expected) in cases {
            let row = run_g3_row(&root, 128 * 1024, scenario).expect("run G3 row");
            assert!(row.contains(expected), "{scenario}: {row}");
            assert!(row.contains("\"byte_exact\":true"));
            assert!(row.contains("\"mode_exact\":true"));
            assert!(row.contains("\"temp_residue_count\":0"));
            assert!(row.contains("\"seed_residue_count\":0"));
            if scenario == "before-publication-fault" {
                assert!(row.contains("\"old_or_new\":\"old\""));
                assert!(row.contains("\"rename_calls\":0"));
            } else if scenario == "lost-ack" {
                assert!(row.contains("\"old_or_new\":\"new\""));
                assert!(row.contains("\"directory_sync_calls\":1"));
            }
        }
        let external =
            run_g3_row(&root, 128 * 1024, "external-mutation").expect("external mutation");
        assert!(external.contains("\"qualification_reason\":\"destination-invalidated\""));
        assert!(external.contains("\"authority_validation_successes\":1"));
        assert!(external.contains("\"authority_validation_failures\":0"));
        assert!(external.contains("\"permit_consumptions\":0"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn missing_destination_and_seed_are_complete_fallback_misses() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-g3-missing-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut missing_destination = prepare(
            &root.join("destination"),
            128 * 1024,
            Scenario::QualifiedOneByte,
        )
        .expect("prepare destination miss");
        fs::remove_file(missing_destination.directory_path.join(DESTINATION_NAME))
            .expect("remove destination");
        let result = run_operation(&mut missing_destination).expect("destination fallback");
        assert_eq!(result.route, "complete-fallback");
        assert_eq!(result.reason, "destination-invalidated");
        assert_eq!(result.counters.permit_consumptions, 0);

        let mut missing_seed = prepare(&root.join("seed"), 128 * 1024, Scenario::QualifiedOneByte)
            .expect("prepare seed miss");
        missing_seed.seed = None;
        let result = run_operation(&mut missing_seed).expect("seed fallback");
        assert_eq!(result.route, "complete-fallback");
        assert_eq!(result.reason, "invalid-authority");
        assert_eq!(result.counters.permit_consumptions, 0);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn symlink_preflight_precedes_invalid_authority_for_every_scenario() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-g3-precedence-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut prepared =
            prepare(&root, 128 * 1024, Scenario::InvalidAuthority).expect("prepare invalid");
        let destination = prepared.directory_path.join(DESTINATION_NAME);
        fs::rename(&destination, prepared.directory_path.join("sentinel.bin"))
            .expect("retain destination");
        std::os::unix::fs::symlink("sentinel.bin", destination).expect("substitute symlink");
        let result = run_operation(&mut prepared).expect("typed preflight");
        assert_eq!(result.route, "typed-rejection");
        assert_eq!(result.error, Some("NativeDestinationSymlink"));
        assert_eq!(result.counters.authority_validations, 0);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn permit_rechecks_retained_directory_identity() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-g3-directory-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut prepared =
            prepare(&root, 128 * 1024, Scenario::QualifiedOneByte).expect("prepare row");
        let key = prepared.permit_key.expect("permit key");
        let permit = prepared.permit.as_mut().expect("permit");
        permit.binding.directory.inode ^= 1;
        permit.tag = permit_tag(&key, &permit.binding).expect("retag altered binding");
        let mut metrics = Metrics::default();
        let mut counters = Counters::default();
        assert!(!validate_permit(&mut prepared, &mut metrics, &mut counters)
            .expect("validation result"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn patch_retry_resets_target_and_proves_one_exact_range() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-g3-retry-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create root");
        let parent = root.join("parent");
        let target = root.join("target");
        write_fixture(&parent, 128 * 1024).expect("fixture");
        let mut attempts = 0;
        let range = choose_same_count_patch_with(&parent, &target, 128 * 1024, 1, |_| {
            attempts += 1;
            Ok(attempts == 2)
        })
        .expect("second attempt");
        assert_eq!(attempts, 2);
        verify_exact_patch_relation(&parent, &target, &range).expect("single exact patch");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn canonical_range_proof_rejects_underdeclared_range_and_digest_replay() {
        let root = test_root("canonical-range");
        let mut prepared =
            prepare(&root, 128 * 1024, Scenario::QualifiedOneByte).expect("prepare row");
        let mut proof_metrics = Metrics::default();
        let error = match prove_canonical_range(
            &mut prepared.store,
            prepared.parent,
            prepared.parent_digest,
            &prepared.directory_path.join("parent.source"),
            prepared.target,
            prepared.target_digest,
            &prepared.directory_path.join("target.source"),
            &(0..0),
            &mut proof_metrics,
        ) {
            Ok(_) => panic!("underdeclared range must reject"),
            Err(error) => error,
        };
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::PublicationConflict)
        );
        finish_q(&mut proof_metrics).expect("proof Q decharged");

        prepared.target_digest[0] ^= 1;
        let mut authority_metrics = Metrics::default();
        let mut counters = Counters::default();
        assert!(
            !validate_permit(&mut prepared, &mut authority_metrics, &mut counters,)
                .expect("digest replay validation")
        );
        finish_q(&mut authority_metrics).expect("authority Q decharged");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn stream_root_dfs_q_decharges_after_success_and_writer_error() {
        struct FailingWriter;

        impl Write for FailingWriter {
            fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from_raw_os_error(libc::EIO))
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let root = test_root("stream-root-q");
        let mut prepared =
            prepare(&root, 128 * 1024, Scenario::SymlinkSubstitution).expect("prepare store");
        let mut success_metrics = Metrics::default();
        stream_root(
            &mut prepared.store,
            prepared.target.namespace,
            &mut std::io::sink(),
            &mut success_metrics,
        )
        .expect("successful stream");
        assert!(success_metrics.q_high_water > 0);
        finish_q(&mut success_metrics).expect("success Q decharged");

        let mut error_metrics = Metrics::default();
        assert!(stream_root(
            &mut prepared.store,
            prepared.target.namespace,
            &mut FailingWriter,
            &mut error_metrics,
        )
        .is_err());
        assert!(error_metrics.q_high_water > 0);
        finish_q(&mut error_metrics).expect("error Q decharged");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn clone_miss_falls_back_without_consuming_single_use_permit() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-g3-clone-miss-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut prepared =
            prepare(&root, 128 * 1024, Scenario::QualifiedOneByte).expect("prepare row");
        prepared.fault.clone_reopen_failure = true;
        let result = run_operation(&mut prepared).expect("clone-miss fallback");
        assert_eq!(result.route, "complete-fallback");
        assert_eq!(result.reason, "clone-failed");
        assert_eq!(result.counters.clone_calls, 1);
        assert_eq!(result.counters.clone_failures, 1);
        assert_eq!(result.counters.fallback_calls, 1);
        assert_eq!(result.counters.permit_consumptions, 0);
        assert_eq!(
            result.counters.temp_files_created,
            result.counters.temp_files_removed + result.counters.rename_calls
        );
        assert!(!prepared.permit.as_ref().expect("permit").consumed);
        assert_eq!(
            count_residue(&prepared.directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        let mut counters = Counters::default();
        consume_permit(&mut prepared, &mut counters).expect("first consumption");
        assert!(consume_permit(&mut prepared, &mut counters).is_err());
        assert_eq!(counters.permit_consumptions, 1);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn reconciliation_rejects_identity_change_during_complete_compare() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-g3-continuity-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut prepared = prepare(&root.join("identity"), 128 * 1024, Scenario::LostAck)
            .expect("prepare identity row");
        prepared.fault.reconciliation_identity_mutation = true;
        let error = match run_operation(&mut prepared) {
            Ok(_) => panic!("identity mutation must be ambiguous"),
            Err(error) => error,
        };
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::AmbiguousDurability)
        );
        assert_eq!(
            count_residue(&prepared.directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        let mut substituted =
            prepare(&root.join("name"), 128 * 1024, Scenario::LostAck).expect("prepare name row");
        substituted.fault.reconciliation_name_substitution = true;
        let error = match run_operation(&mut substituted) {
            Ok(_) => panic!("name substitution must be ambiguous"),
            Err(error) => error,
        };
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::AmbiguousDurability)
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rename_error_cleans_target_temp_and_preserves_prior_failure() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-g3-rename-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut target = prepare(&root.join("target"), 128 * 1024, Scenario::QualifiedNoop)
            .expect("prepare target-equivalent row");
        target.fault.rename_failure = true;
        let result = run_operation(&mut target).expect("target reconciliation");
        assert_eq!(result.reconciliation, "target");
        assert_eq!(
            count_residue(&target.directory_path, ".g3-tmp-").expect("residue"),
            0
        );

        let mut prior = prepare(&root.join("prior"), 128 * 1024, Scenario::QualifiedOneByte)
            .expect("prepare prior row");
        prior.fault.rename_failure = true;
        let error = match run_operation(&mut prior) {
            Ok(_) => panic!("prior reconciliation must preserve rename failure"),
            Err(error) => error,
        };
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::raw_os_error),
            Some(libc::EIO)
        );
        assert_eq!(
            count_residue(&prior.directory_path, ".g3-tmp-").expect("residue"),
            0
        );

        let mut sync = prepare(&root.join("sync"), 128 * 1024, Scenario::QualifiedOneByte)
            .expect("prepare sync row");
        sync.fault.directory_sync_failure = true;
        let result = run_operation(&mut sync).expect("sync reconciliation");
        assert_eq!(result.reconciliation, "target");
        assert_eq!(result.counters.directory_sync_calls, 2);
        assert_eq!(result.counters.reconciliation_calls, 1);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn reconciliation_q_charges_fixed_comparison_buffer_exactly() {
        let root = test_root("reconciliation-q");
        let mut prepared = prepare(&root, 128 * 1024, Scenario::LostAck).expect("prepare lost ack");
        let result = run_operation(&mut prepared).expect("reconcile target");
        assert_eq!(result.counters.reconciliation_q_high_water, 50_645);
        assert_eq!(result.counters.q_high_water, 50_645);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn temp_counter_failure_leaves_no_named_residue() {
        let root = test_root("temp-guard");
        fs::create_dir(&root).expect("create root");
        let directory = open_dir(&root).expect("open root");
        let mut counters = Counters {
            temp_files_created: u64::MAX,
            ..Counters::default()
        };
        assert!(create_temp(&directory, &mut counters).is_err());
        let residue = count_residue(&root, ".g3-tmp-").expect("count temp residue");
        drop(directory);
        fs::remove_dir_all(root).expect("cleanup");
        assert_eq!(residue, 0);
    }

    #[test]
    fn seed_post_create_failure_leaves_no_named_residue() {
        let root = test_root("seed-guard");
        let mut prepared =
            prepare(&root, 128 * 1024, Scenario::SymlinkSubstitution).expect("prepare store");
        let mut metrics = Metrics::default();
        assert!(create_verified_seed(
            &mut prepared.store,
            prepared.parent,
            &prepared.directory,
            prepared.parent_digest,
            &mut metrics,
            true,
        )
        .is_err());
        let residue =
            count_residue(&prepared.directory_path, ".g3-seed-").expect("count seed residue");
        drop(prepared);
        fs::remove_dir_all(root).expect("cleanup");
        assert_eq!(residue, 0);
    }

    #[test]
    fn publication_error_dominates_cleanup_error_with_both_provenances() {
        let root = test_root("first-error");
        let mut prepared =
            prepare(&root, 128 * 1024, Scenario::QualifiedOneByte).expect("prepare prior row");
        prepared.fault.rename_failure = true;
        prepared.fault.cleanup_failure = true;
        let error = match run_operation(&mut prepared) {
            Ok(_) => panic!("combined fault must fail"),
            Err(error) => error,
        };
        let provenance = error
            .downcast_ref::<NativePublicationCleanupFailure>()
            .expect("publication and cleanup errors");
        assert_eq!(provenance.publication.raw_os_error(), Some(libc::EIO));
        assert_eq!(
            provenance
                .cleanup
                .downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::raw_os_error),
            Some(libc::EACCES)
        );
        assert_eq!(
            std::error::Error::source(provenance)
                .and_then(|source| source.downcast_ref::<std::io::Error>())
                .and_then(std::io::Error::raw_os_error),
            Some(libc::EIO)
        );
        assert_eq!(
            count_residue(&prepared.directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
