//! Preserved historical Phase-4 G3 materialization harness; not product code.

use std::ffi::{CStr, CString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::*;

const DESTINATION_NAME: &str = "materialized.bin";
const DESTINATION_C_NAME: &CStr = c"materialized.bin";
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
    stat_c_at(directory, &name)
}

fn stat_c_at(directory: &File, name: &CStr) -> std::io::Result<Option<NativeIdentity>> {
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

fn rename_exclusive_at(directory: &File, from: &str, to: &str) -> std::io::Result<()> {
    const RENAME_EXCL: libc::c_uint = 0x4;
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
            RENAME_EXCL | RENAME_NOFOLLOW_ANY,
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
    authority: ManagedCleanupAuthority,
    name: String,
    identity: Option<(u64, u64)>,
    owned_descriptor: Option<File>,
    active: bool,
}

#[derive(Clone, Copy)]
struct ManagedCleanupAuthority {
    directory: NativeIdentity,
}

impl ManagedCleanupAuthority {
    fn new(directory: &File) -> AnyResult<Self> {
        let identity = fstat_file(directory)?.0;
        if !identity.is_directory() {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
        Ok(Self {
            directory: identity,
        })
    }

    fn validate(self, directory: &File) -> AnyResult<()> {
        let current = fstat_file(directory)?.0;
        if !current.is_directory()
            || current.device != self.directory.device
            || current.inode != self.directory.inode
        {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
        Ok(())
    }
}

impl TempName {
    fn remove_owned_name(&mut self) -> AnyResult<bool> {
        if !self.active {
            return Ok(false);
        }
        self.authority.validate(&self.directory)?;
        let expected = self.identity.or_else(|| {
            self.owned_descriptor
                .as_ref()
                .and_then(|file| fstat_file(file).ok())
                .map(|(native, _)| (native.device, native.inode))
        });
        let Some((device, inode)) = expected else {
            self.active = false;
            return Err(CoreError::AmbiguousDurability.into());
        };
        match stat_at(&self.directory, &self.name)? {
            None => {
                self.active = false;
                return Ok(false);
            }
            Some(current) if current.device == device && current.inode == inode => {}
            Some(_) => {
                self.active = false;
                return Err(CoreError::AmbiguousDurability.into());
            }
        }
        let removed = unlink_at(&self.directory, &self.name)?;
        self.active = false;
        Ok(removed)
    }

    fn remove(&mut self, counters: &mut Counters) -> AnyResult<()> {
        if self.remove_owned_name()? {
            counters.temp_files_removed = counters
                .temp_files_removed
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn remove_with_fault(&mut self, counters: &mut Counters, fail: bool) -> AnyResult<()> {
        if fail {
            return Err(std::io::Error::from_raw_os_error(libc::EACCES).into());
        }
        self.remove(counters)
    }

    #[cfg(test)]
    fn remove_with_substitution_after_validation(
        &mut self,
        counters: &mut Counters,
        retained: &str,
    ) -> AnyResult<()> {
        self.authority.validate(&self.directory)?;
        let expected = self.identity.ok_or(CoreError::AmbiguousDurability)?;
        let current =
            stat_at(&self.directory, &self.name)?.ok_or(CoreError::AmbiguousDurability)?;
        if (current.device, current.inode) != expected {
            return Err(CoreError::AmbiguousDurability.into());
        }
        rename_at(&self.directory, &self.name, retained)?;
        let mut substitute = openat_file(
            &self.directory,
            &self.name,
            libc::O_WRONLY | libc::O_CLOEXEC | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
            0o600,
        )?;
        substitute.write_all(b"substitute")?;
        self.remove(counters)
    }
}

impl Drop for TempName {
    fn drop(&mut self) {
        let _ = self.remove_owned_name();
    }
}

fn create_temp(directory: &File, counters: &mut Counters) -> AnyResult<(File, TempName)> {
    let name = random_name(".g3-tmp-")?;
    let mut temp = TempName {
        directory: directory.try_clone()?,
        authority: ManagedCleanupAuthority::new(directory)?,
        name: name.clone(),
        identity: None,
        owned_descriptor: None,
        active: false,
    };
    let file = openat_file(
        directory,
        &name,
        libc::O_RDWR | libc::O_CLOEXEC | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
        0o600,
    )?;
    temp.owned_descriptor = Some(file);
    temp.active = true;
    let native = fstat_file(
        temp.owned_descriptor
            .as_ref()
            .ok_or(CoreError::ValidationAuthorityUnavailable)?,
    )?
    .0;
    temp.identity = Some((native.device, native.inode));
    let file = temp
        .owned_descriptor
        .as_ref()
        .ok_or(CoreError::ValidationAuthorityUnavailable)?
        .try_clone()?;
    counters.temp_files_created = counters
        .temp_files_created
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    Ok((file, temp))
}

#[derive(Debug)]
struct CloneCleanupUnresolved {
    first: FailureCause,
}

impl std::fmt::Display for CloneCleanupUnresolved {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "clone cleanup unresolved after {:?}", self.first)
    }
}

impl std::error::Error for CloneCleanupUnresolved {}

fn clone_temp(
    seed: &VerifiedSeed,
    directory: &File,
    counters: &mut Counters,
    fail_reopen: bool,
    fail_open: bool,
    fail_stat: bool,
) -> AnyResult<Option<(File, TempName)>> {
    let name = random_name(".g3-tmp-")?;
    let mut temp = TempName {
        directory: directory.try_clone()?,
        authority: ManagedCleanupAuthority::new(directory)?,
        name: name.clone(),
        identity: None,
        owned_descriptor: None,
        active: false,
    };
    counters.clone_calls = counters
        .clone_calls
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    let next_temp_files_created = counters
        .temp_files_created
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    match fclone_unlinked(&seed.file, directory, std::ffi::OsStr::new(&name)) {
        Ok(()) => {
            temp.active = true;
            let opened = if fail_open {
                Err(std::io::Error::from_raw_os_error(libc::EIO))
            } else {
                openat_file(
                    directory,
                    &name,
                    libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                    0,
                )
            };
            let file = match opened {
                Ok(file) => file,
                Err(error) => {
                    temp.active = false;
                    return Err(CloneCleanupUnresolved {
                        first: failure_cause(&error),
                    }
                    .into());
                }
            };
            temp.owned_descriptor = Some(file);
            let stated = if fail_stat {
                Err(std::io::Error::from_raw_os_error(libc::EIO))
            } else {
                fstat_file(
                    temp.owned_descriptor
                        .as_ref()
                        .ok_or(CoreError::ValidationAuthorityUnavailable)?,
                )
                .map(|value| value.0)
            };
            let descriptor = match stated {
                Ok(descriptor) => descriptor,
                Err(error) => {
                    temp.active = false;
                    return Err(CloneCleanupUnresolved {
                        first: failure_cause(&error),
                    }
                    .into());
                }
            };
            temp.identity = Some((descriptor.device, descriptor.inode));
            let native =
                stat_at(directory, &name)?.ok_or(CoreError::ValidationAuthorityUnavailable)?;
            counters.temp_files_created = next_temp_files_created;
            let reopened = (|| -> AnyResult<File> {
                if fail_reopen {
                    return Err(CoreError::ValidationAuthorityUnavailable.into());
                }
                if !native.is_regular()
                    || !descriptor.is_regular()
                    || native.device != descriptor.device
                    || native.inode != descriptor.inode
                {
                    return Err(CoreError::ValidationAuthorityUnavailable.into());
                }
                Ok(temp
                    .owned_descriptor
                    .as_ref()
                    .ok_or(CoreError::ValidationAuthorityUnavailable)?
                    .try_clone()?)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    clone_open_failure: bool,
    #[cfg(test)]
    clone_stat_failure: bool,
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
        #[allow(clippy::implicit_saturating_sub)]
        let parent_remaining = if offset < parent_length {
            parent_length - offset
        } else {
            0
        };
        #[allow(clippy::implicit_saturating_sub)]
        let target_remaining = if offset < target_length {
            target_length - offset
        } else {
            0
        };
        let parent_read = usize::try_from(parent_remaining)
            .map_err(|_| CoreError::LengthOverflow)?
            .min(take);
        let target_read = usize::try_from(target_remaining)
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
        build_and_publish_target_in_active_transaction(store, source, expected_references, metrics)
    })
}

fn build_and_publish_target_in_active_transaction(
    store: &mut Store,
    source: &Path,
    expected_references: u64,
    metrics: &mut Metrics,
) -> AnyResult<Roots> {
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
    let transition =
        publish_transition_with_operations(store, Some(prior.1), namespace, &operations, metrics)?;
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
        authority: ManagedCleanupAuthority::new(directory)?,
        name: name.clone(),
        identity: None,
        owned_descriptor: None,
        active: false,
    };
    let output = openat_file(
        directory,
        &name,
        libc::O_WRONLY | libc::O_CLOEXEC | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
        0o600,
    )?;
    cleanup.owned_descriptor = Some(output);
    cleanup.active = true;
    let native = fstat_file(
        cleanup
            .owned_descriptor
            .as_ref()
            .ok_or(CoreError::ValidationAuthorityUnavailable)?,
    )?
    .0;
    cleanup.identity = Some((native.device, native.inode));
    let mut output = cleanup
        .owned_descriptor
        .as_ref()
        .ok_or(CoreError::ValidationAuthorityUnavailable)?
        .try_clone()?;
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
    let _readback_charge = charge_capacity(metrics, 1024 * 1024)?;
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
    fs::set_permissions(&directory_path, fs::Permissions::from_mode(0o700))?;
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
        counters.authority_validation_failures = counters
            .authority_validation_failures
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        return Ok(false);
    };
    if permit.consumed {
        counters.authority_validation_failures = counters
            .authority_validation_failures
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
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
        counters.authority_validation_failures = counters
            .authority_validation_failures
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        return Ok(false);
    };
    let Ok((seed_native, _)) = fstat_file(&seed.file) else {
        counters.authority_validation_failures = counters
            .authority_validation_failures
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
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
        counters.authority_validation_failures = counters
            .authority_validation_failures
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
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
        counters.authority_validation_successes = counters
            .authority_validation_successes
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
    } else {
        counters.authority_validation_failures = counters
            .authority_validation_failures
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
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
    max_single_buffer_bytes: u64,
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
    let destination = stat_c_at(&prepared.directory, DESTINATION_C_NAME)?;
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
            max_single_buffer_bytes: 0,
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
            max_single_buffer_bytes: 0,
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
        #[cfg(test)]
        let fail_open = std::mem::take(&mut prepared.fault.clone_open_failure);
        #[cfg(not(test))]
        let fail_open = false;
        #[cfg(test)]
        let fail_stat = std::mem::take(&mut prepared.fault.clone_stat_failure);
        #[cfg(not(test))]
        let fail_stat = false;
        clone_temp(
            prepared
                .seed
                .as_ref()
                .ok_or(CoreError::ValidationAuthorityUnavailable)?,
            &prepared.directory,
            &mut counters,
            fail_reopen,
            fail_open,
            fail_stat,
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
        max_single_buffer_bytes: SOURCE_1,
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
            "\"reconciliation_outcome\":\"{}\",\"q_high_water\":{},\"q_terminal\":0,\"max_buffer_bytes\":1048576,",
            "\"max_single_buffer_bytes\":{},\"buffer_evidence_complete\":true,\"full_file_buffer_bytes\":0,",
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
        result.max_single_buffer_bytes,
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

fn g4_fixture_directory(root: &Path) -> PathBuf {
    root.join("g3-qualified-noop")
}

pub(super) fn prepare_g4_fixture(root: &Path, size: u64) -> AnyResult<String> {
    let seed = match size {
        SOURCE_1 => 0x41,
        SOURCE_10 => 0x4a,
        SOURCE_100 => 0x51,
        _ => return Err("G4 fixtures are limited to 1, 10, or 100 MiB".into()),
    };
    fs::create_dir_all(root)?;
    let directory = g4_fixture_directory(root);
    fs::create_dir(&directory).map_err(|error| {
        format!(
            "refusing to reuse G4 fixture directory {}: {error}",
            directory.display()
        )
    })?;
    let source = directory.join("target.source");
    fill_source(&source, size, seed)?;
    let (_, digest, _) = hash_file(&source)?;
    let mut metrics = Metrics::default();
    let mut store = Store::open(&directory.join("store.sqlite"), SELECTED_PROFILE)?;
    let roots = build_and_publish_parent(&mut store, &source, &mut metrics)?;
    if roots.length != size || roots.references != source_cdc_sequence(&source)?.0 {
        return Err(CoreError::PublicationConflict.into());
    }
    finish_q(&mut metrics)?;
    drop(store);
    Ok(format!(
        "{{\"status\":\"PASS\",\"schema\":\"phase4-g4-fixture-v1\",\"size_bytes\":{size},\"directory\":\"{}\",\"root\":\"{}\",\"file_root\":\"{}\",\"references\":{},\"output_digest\":\"{}\"}}",
        directory.display(),
        roots.namespace,
        roots.file,
        roots.references,
        hex_bytes(&digest),
    ))
}

#[derive(Clone, Copy)]
struct G4Usage {
    user_us: i128,
    system_us: i128,
    voluntary_switches: i128,
    involuntary_switches: i128,
}

fn g4_usage() -> AnyResult<G4Usage> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: `usage` points to writable storage for one `rusage` value.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: successful `getrusage` initialized the value.
    let usage = unsafe { usage.assume_init() };
    Ok(G4Usage {
        user_us: i128::from(usage.ru_utime.tv_sec) * 1_000_000 + i128::from(usage.ru_utime.tv_usec),
        system_us: i128::from(usage.ru_stime.tv_sec) * 1_000_000
            + i128::from(usage.ru_stime.tv_usec),
        voluntary_switches: i128::from(usage.ru_nvcsw),
        involuntary_switches: i128::from(usage.ru_nivcsw),
    })
}

fn g4_usage_delta(after: G4Usage, before: G4Usage) -> AnyResult<G4Usage> {
    let subtract = |after: i128, before: i128| {
        after
            .checked_sub(before)
            .filter(|value| *value >= 0)
            .ok_or(CoreError::LengthOverflow)
    };
    Ok(G4Usage {
        user_us: subtract(after.user_us, before.user_us)?,
        system_us: subtract(after.system_us, before.system_us)?,
        voluntary_switches: subtract(after.voluntary_switches, before.voluntary_switches)?,
        involuntary_switches: subtract(after.involuntary_switches, before.involuntary_switches)?,
    })
}

fn g4_roots(store: &Store, root: ObjectId, metrics: &mut Metrics) -> AnyResult<Roots> {
    let file = resolve_namespace_file_root(store, root, metrics)?;
    let bytes = store.get_bytes(file, metrics)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, true, metrics)?;
    let (_, length, references, level, children) = file_codec::parse_file_root(payload)?;
    if level != file_codec::expected_file_level(references)? {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    if references == 0 {
        if length != 0 || level != 0 || !children.is_empty() {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
    } else {
        file_codec::validate_file_children(&children, true)?;
    }
    Ok(Roots {
        namespace: root,
        file,
        length,
        references,
    })
}

fn g4_hash_path(path: &Path, metrics: &mut Metrics) -> AnyResult<(u64, [u8; 32], u32)> {
    let mut file = File::open(path)?;
    let mode = file.metadata()?.permissions().mode() & 0o7777;
    let _buffer_charge = charge_capacity(metrics, 1024 * 1024)?;
    let mut buffer = [0_u8; 1024 * 1024];
    let mut hasher = blake3::Hasher::new();
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

#[derive(Default)]
struct G4WriterCounters {
    calls: u64,
    bytes: u64,
    short_writes: u64,
    errors: u64,
}

struct G4Writer<'a> {
    file: &'a mut File,
    counters: &'a mut G4WriterCounters,
}

impl Write for G4Writer<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.counters.calls = self
            .counters
            .calls
            .checked_add(1)
            .ok_or_else(|| std::io::Error::from_raw_os_error(libc::EOVERFLOW))?;
        match self.file.write(bytes) {
            Ok(written) => {
                self.counters.bytes = self
                    .counters
                    .bytes
                    .checked_add(
                        u64::try_from(written)
                            .map_err(|_| std::io::Error::from_raw_os_error(libc::EOVERFLOW))?,
                    )
                    .ok_or_else(|| std::io::Error::from_raw_os_error(libc::EOVERFLOW))?;
                if written != bytes.len() {
                    self.counters.short_writes = self
                        .counters
                        .short_writes
                        .checked_add(1)
                        .ok_or_else(|| std::io::Error::from_raw_os_error(libc::EOVERFLOW))?;
                }
                Ok(written)
            }
            Err(error) => {
                self.counters.errors = self
                    .counters
                    .errors
                    .checked_add(1)
                    .ok_or_else(|| std::io::Error::from_raw_os_error(libc::EOVERFLOW))?;
                Err(error)
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

#[derive(Clone, Copy)]
enum G4NativeAlgorithm {
    ScalarControl,
    BatchedCandidate,
}

struct G4NativeResult {
    reconstructed: ReconstructedFile,
    wall_ns: u128,
    usage: G4Usage,
    writer: G4WriterCounters,
    metrics: Metrics,
    verification_ns: u128,
    cleanup_ns: u128,
    publication_status: &'static str,
    reconciliation_outcome: &'static str,
    diagnostic: Option<FailureProvenance>,
    temp_files_created: u64,
    temp_files_removed: u64,
    data_sync_calls: u64,
    metadata_operations: u64,
    metadata_sync_calls: u64,
    rename_calls: u64,
    directory_sync_calls: u64,
    reconciliation_calls: u64,
    max_single_buffer_bytes: u64,
}

#[derive(Default)]
struct G4NativeFault {
    #[cfg(test)]
    directory_sync_lost_ack: bool,
    #[cfg(test)]
    directory_sync_retry_failure: bool,
    #[cfg(test)]
    post_publish_substitution: bool,
    #[cfg(test)]
    verification_failure: bool,
}

#[derive(Debug)]
struct G4NativePublicationFailure(FailureProvenance);

impl std::fmt::Display for G4NativePublicationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "G4 native publication failed: {:?}",
            self.0.dominant
        )
    }
}

impl std::error::Error for G4NativePublicationFailure {}

fn g4_hash_descriptor(
    file: &File,
    expected: NativeIdentity,
    metrics: &mut Metrics,
) -> AnyResult<(u64, [u8; 32], u32)> {
    let before = fstat_file(file)?.0;
    if !before.is_regular()
        || before.device != expected.device
        || before.inode != expected.inode
        || before.length != expected.length
        || u32::from(before.mode & 0o7777) != MODE
    {
        return Err(CoreError::PublicationConflict.into());
    }
    let _buffer_charge = charge_capacity(metrics, 1024 * 1024)?;
    let mut buffer = [0_u8; 1024 * 1024];
    let mut hasher = blake3::Hasher::new();
    let mut offset = 0_u64;
    while offset < before.length {
        let take = usize::try_from(
            before
                .length
                .checked_sub(offset)
                .ok_or(CoreError::LengthOverflow)?
                .min(u64::try_from(buffer.len()).map_err(|_| CoreError::LengthOverflow)?),
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        let file_offset = libc::off_t::try_from(offset).map_err(|_| CoreError::LengthOverflow)?;
        // SAFETY: the retained descriptor is live and the bounded buffer is writable.
        let read = unsafe {
            libc::pread(
                file.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                take,
                file_offset,
            )
        };
        if read <= 0 {
            return Err(if read == 0 {
                CoreError::LengthMismatch {
                    expected: before.length,
                    actual: offset,
                }
                .into()
            } else {
                std::io::Error::last_os_error().into()
            });
        }
        let read = usize::try_from(read).map_err(|_| CoreError::LengthOverflow)?;
        hasher.update(&buffer[..read]);
        offset = offset
            .checked_add(u64::try_from(read).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    let after = fstat_file(file)?.0;
    if after != before {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok((offset, *hasher.finalize().as_bytes(), MODE))
}

fn g4_reconcile_absent(
    directory: &File,
    file: &File,
    owned: NativeIdentity,
    expected_length: u64,
    expected_digest: [u8; 32],
    counters: &mut Counters,
) -> AnyResult<Reconciliation> {
    counters.reconciliation_calls = counters
        .reconciliation_calls
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    let Some(named) = stat_at(directory, DESTINATION_NAME)? else {
        return Ok(Reconciliation::PriorVisible);
    };
    if !named.is_regular()
        || named.device != owned.device
        || named.inode != owned.inode
        || named.length != expected_length
        || u32::from(named.mode & 0o7777) != MODE
    {
        return Ok(Reconciliation::DifferentHead);
    }
    let mut metrics = Metrics::default();
    let (length, digest, mode) = g4_hash_descriptor(file, owned, &mut metrics)?;
    finish_q(&mut metrics)?;
    if length == expected_length && digest == expected_digest && mode == MODE {
        Ok(Reconciliation::RequestedVisible)
    } else {
        Ok(Reconciliation::DifferentHead)
    }
}

fn g4_cleanup_owned_name(
    temp: &mut TempName,
    directory: &File,
    counters: &mut Counters,
) -> AnyResult<()> {
    temp.remove(counters)?;
    sync_fd(directory)?;
    counters.directory_sync_calls = counters
        .directory_sync_calls
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn g4_materialize(
    store: &mut Store,
    head: &VisibleHead,
    roots: Roots,
    expected_digest: [u8; 32],
    expected_sequence: [u8; 32],
    output_root: &Path,
    algorithm: G4NativeAlgorithm,
    _fault: G4NativeFault,
) -> AnyResult<G4NativeResult> {
    fs::create_dir(output_root)?;
    fs::set_permissions(output_root, fs::Permissions::from_mode(0o700))?;
    let directory = open_dir(output_root)?;
    if stat_at(&directory, DESTINATION_NAME)?.is_some() {
        return Err(CoreError::PublicationConflict.into());
    }
    let mut native_counters = Counters::default();
    let (mut file, mut temp) = create_temp(&directory, &mut native_counters)?;
    let start_usage = g4_usage()?;
    let start = Instant::now();
    let mut metrics = Metrics::default();
    let mut writer_counters = G4WriterCounters::default();
    let mut publication_status = "committed";
    let mut reconciliation_outcome = "not-needed";
    let mut diagnostic = None;
    let operation = (|| -> AnyResult<ReconstructedFile> {
        let reconstructed = {
            let mut writer = G4Writer {
                file: &mut file,
                counters: &mut writer_counters,
            };
            match algorithm {
                G4NativeAlgorithm::ScalarControl => {
                    let (length, references, output_digest) =
                        stream_root(store, roots.namespace, &mut writer, &mut metrics)?;
                    ReconstructedFile {
                        content_closure: None,
                        output_digest,
                        occurrence_digest: [0_u8; 32],
                        references,
                        length,
                        evidence: ReconstructionEvidence::default(),
                    }
                }
                G4NativeAlgorithm::BatchedCandidate => {
                    let mut emit = |raw: &[u8]| -> AnyResult<()> {
                        writer.write_all(raw)?;
                        Ok(())
                    };
                    reconstruct_file_to(
                        store,
                        roots.namespace,
                        Some(&hex_bytes(&expected_digest)),
                        Some(&hex_bytes(&expected_sequence)),
                        true,
                        false,
                        &mut metrics,
                        &mut emit,
                    )?
                }
            }
        };
        if reconstructed.length != roots.length
            || reconstructed.references != roots.references
            || reconstructed.output_digest != expected_digest
            || (matches!(algorithm, G4NativeAlgorithm::BatchedCandidate)
                && reconstructed.occurrence_digest != expected_sequence)
            || store.current_head()?.as_ref() != Some(head)
        {
            return Err(CoreError::PublicationConflict.into());
        }
        sync_fd(&file)?;
        native_counters.data_sync_calls = native_counters
            .data_sync_calls
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        chmod_fd(&file, MODE)?;
        native_counters.metadata_operations = native_counters
            .metadata_operations
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        sync_fd(&file)?;
        native_counters.metadata_sync_calls = native_counters
            .metadata_sync_calls
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let descriptor = fstat_file(&file)?.0;
        let named =
            stat_at(&directory, &temp.name)?.ok_or(CoreError::ValidationAuthorityUnavailable)?;
        if !descriptor.is_regular()
            || !named.is_regular()
            || descriptor.device != named.device
            || descriptor.inode != named.inode
            || descriptor.length != reconstructed.length
            || u32::from(descriptor.mode & 0o7777) != MODE
        {
            return Err(CoreError::PublicationConflict.into());
        }
        native_counters.rename_calls = native_counters
            .rename_calls
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let rename = rename_exclusive_at(&directory, &temp.name, DESTINATION_NAME);
        if rename.is_ok() {
            temp.name = DESTINATION_NAME.to_string();
        }
        let mut first = rename.err();
        #[cfg(test)]
        if first.is_none() && _fault.post_publish_substitution {
            rename_at(&directory, DESTINATION_NAME, ".g4-fault-retained")?;
            std::os::unix::fs::symlink(".g4-fault-retained", output_root.join(DESTINATION_NAME))?;
            first = Some(std::io::Error::from_raw_os_error(libc::ESTALE));
        }
        if first.is_none() {
            native_counters.directory_sync_calls = native_counters
                .directory_sync_calls
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            let directory_sync = sync_fd(&directory);
            #[cfg(test)]
            let directory_sync = if directory_sync.is_ok() && _fault.directory_sync_lost_ack {
                Err(std::io::Error::from_raw_os_error(libc::EIO))
            } else {
                directory_sync
            };
            first = directory_sync.err();
        }
        if let Some(first) = first {
            let first = if first.raw_os_error() == Some(libc::EEXIST) {
                FailureCause::Core(CoreError::PublicationConflict)
            } else {
                failure_cause(&first)
            };
            let (reconciliation, reconciliation_error) = match g4_reconcile_absent(
                &directory,
                &file,
                descriptor,
                reconstructed.length,
                reconstructed.output_digest,
                &mut native_counters,
            ) {
                Ok(value) => (value, None),
                Err(error) => (
                    Reconciliation::Ambiguous,
                    Some(failure_cause(error.as_ref())),
                ),
            };
            reconciliation_outcome = match reconciliation {
                Reconciliation::RequestedVisible => "requested-visible",
                Reconciliation::PriorVisible => "prior-absent",
                Reconciliation::DifferentHead => "different",
                Reconciliation::Ambiguous | Reconciliation::NotAttempted => "unresolved",
            };
            if reconciliation == Reconciliation::RequestedVisible {
                native_counters.directory_sync_calls = native_counters
                    .directory_sync_calls
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
                let retry = sync_fd(&directory);
                #[cfg(test)]
                let retry = if retry.is_ok() && _fault.directory_sync_retry_failure {
                    Err(std::io::Error::from_raw_os_error(libc::EIO))
                } else {
                    retry
                };
                if let Err(error) = retry {
                    return Err(G4NativePublicationFailure(failure_provenance(
                        Some(first),
                        None,
                        Reconciliation::Ambiguous,
                        Some(failure_cause(&error)),
                    ))
                    .into());
                }
                temp.name = DESTINATION_NAME.to_string();
                publication_status = "requested-visible";
                diagnostic = Some(failure_provenance(
                    Some(first),
                    None,
                    reconciliation,
                    reconciliation_error,
                ));
            } else {
                return Err(G4NativePublicationFailure(failure_provenance(
                    Some(first),
                    None,
                    reconciliation,
                    reconciliation_error,
                ))
                .into());
            }
        }
        Ok(reconstructed)
    })();
    let wall_ns = start.elapsed().as_nanos();
    let usage = g4_usage_delta(g4_usage()?, start_usage)?;
    let reconstructed = match operation {
        Ok(value) => value,
        Err(first) => {
            let existing = first
                .downcast_ref::<G4NativePublicationFailure>()
                .map(|failure| failure.0);
            let first_cause = existing
                .and_then(|failure| failure.first)
                .unwrap_or_else(|| failure_cause(first.as_ref()));
            let existing_cleanup = existing.and_then(|failure| failure.cleanup_first);
            let q_failure = finish_q(&mut metrics).err().map(FailureCause::Core);
            let cleanup_failure =
                g4_cleanup_owned_name(&mut temp, &directory, &mut native_counters)
                    .err()
                    .map(|error| failure_cause(error.as_ref()));
            let cleanup_first = existing_cleanup.or(q_failure).or(cleanup_failure);
            return Err(G4NativePublicationFailure(failure_provenance(
                Some(first_cause),
                cleanup_first,
                existing.map_or(Reconciliation::NotAttempted, |failure| {
                    failure.reconciliation
                }),
                existing.and_then(|failure| failure.reconciliation_error),
            ))
            .into());
        }
    };
    let verification_start = Instant::now();
    let mut verification_metrics = Metrics::default();
    let verification = (|| -> AnyResult<()> {
        #[cfg(test)]
        if _fault.verification_failure {
            return Err(CoreError::IdentityMismatch.into());
        }
        let descriptor = fstat_file(&file)?.0;
        let (verified_length, verified_digest, verified_mode) =
            g4_hash_descriptor(&file, descriptor, &mut verification_metrics)?;
        if verified_length != reconstructed.length
            || verified_digest != reconstructed.output_digest
            || verified_mode != MODE
        {
            return Err(CoreError::PublicationConflict.into());
        }
        Ok(())
    })();
    let verification_failure = verification
        .err()
        .map(|error| failure_cause(error.as_ref()));
    let verification_q_failure = finish_q(&mut verification_metrics)
        .err()
        .map(FailureCause::Core);
    let verification_ns = verification_start.elapsed().as_nanos();
    if let Some(first) = verification_failure.or(verification_q_failure) {
        let q_failure = finish_q(&mut metrics).err().map(FailureCause::Core);
        let cleanup_failure = g4_cleanup_owned_name(&mut temp, &directory, &mut native_counters)
            .err()
            .map(|error| failure_cause(error.as_ref()));
        let cleanup_first = if verification_failure.is_some() {
            verification_q_failure
        } else {
            None
        }
        .or(q_failure)
        .or(cleanup_failure);
        return Err(G4NativePublicationFailure(failure_provenance(
            Some(first),
            cleanup_first,
            Reconciliation::NotAttempted,
            None,
        ))
        .into());
    }
    let q_failure = finish_q(&mut metrics).err().map(FailureCause::Core);
    let cleanup_start = Instant::now();
    let cleanup_failure = g4_cleanup_owned_name(&mut temp, &directory, &mut native_counters)
        .err()
        .map(|error| failure_cause(error.as_ref()));
    let residue_failure = if cleanup_failure.is_none() {
        match stat_at(&directory, DESTINATION_NAME) {
            Ok(Some(_)) => Some(FailureCause::Core(CoreError::PublicationConflict)),
            Ok(None) => None,
            Err(error) => Some(failure_cause(&error)),
        }
    } else {
        None
    };
    let cleanup_ns = cleanup_start.elapsed().as_nanos();
    if let Some(first) = q_failure.or(cleanup_failure).or(residue_failure) {
        let cleanup_first = if q_failure.is_some() {
            cleanup_failure.or(residue_failure)
        } else if cleanup_failure.is_some() {
            residue_failure
        } else {
            None
        };
        return Err(G4NativePublicationFailure(failure_provenance(
            Some(first),
            cleanup_first,
            Reconciliation::NotAttempted,
            None,
        ))
        .into());
    }
    Ok(G4NativeResult {
        reconstructed,
        wall_ns,
        usage,
        writer: writer_counters,
        metrics,
        verification_ns,
        cleanup_ns,
        publication_status,
        reconciliation_outcome,
        diagnostic,
        temp_files_created: native_counters.temp_files_created,
        temp_files_removed: native_counters.temp_files_removed,
        data_sync_calls: native_counters.data_sync_calls,
        metadata_operations: native_counters.metadata_operations,
        metadata_sync_calls: native_counters.metadata_sync_calls,
        rename_calls: native_counters.rename_calls,
        directory_sync_calls: native_counters.directory_sync_calls,
        reconciliation_calls: native_counters.reconciliation_calls,
        max_single_buffer_bytes: SOURCE_1,
    })
}

#[allow(clippy::type_complexity)]
fn g4_seed_read(
    seed: &VerifiedSeed,
    digest: bool,
    metrics: &mut Metrics,
) -> AnyResult<(u128, G4Usage, u64, u64, Option<[u8; 32]>)> {
    let _buffer_charge = charge_capacity(metrics, 1024 * 1024)?;
    let mut buffer = [0_u8; 1024 * 1024];
    let start_usage = g4_usage()?;
    let start = Instant::now();
    let mut offset = 0_u64;
    let mut calls = 0_u64;
    let mut hasher = digest.then(blake3::Hasher::new);
    while offset < seed.identity.length {
        let take = usize::try_from(
            seed.identity
                .length
                .checked_sub(offset)
                .ok_or(CoreError::LengthOverflow)?
                .min(u64::try_from(buffer.len()).map_err(|_| CoreError::LengthOverflow)?),
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        let file_offset = libc::off_t::try_from(offset).map_err(|_| CoreError::LengthOverflow)?;
        // SAFETY: the descriptor is live and the bounded buffer is writable for `take` bytes.
        let read = unsafe {
            libc::pread(
                seed.file.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                take,
                file_offset,
            )
        };
        calls = calls.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        if read <= 0 {
            return Err(if read == 0 {
                CoreError::LengthMismatch {
                    expected: seed.identity.length,
                    actual: offset,
                }
                .into()
            } else {
                std::io::Error::last_os_error().into()
            });
        }
        let read = usize::try_from(read).map_err(|_| CoreError::LengthOverflow)?;
        if let Some(hasher) = hasher.as_mut() {
            hasher.update(&buffer[..read]);
        }
        offset = offset
            .checked_add(u64::try_from(read).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    let wall_ns = start.elapsed().as_nanos();
    let usage = g4_usage_delta(g4_usage()?, start_usage)?;
    Ok((
        wall_ns,
        usage,
        calls,
        offset,
        hasher.map(|hasher| *hasher.finalize().as_bytes()),
    ))
}

fn g4_buffer_evidence(mut row: String, max_single_buffer_bytes: u64) -> AnyResult<String> {
    if row.pop() != Some('}') {
        return Err(CoreError::InvalidRecord("G4 row").into());
    }
    row.push_str(&format!(
        ",\"max_single_buffer_bytes\":{max_single_buffer_bytes},\"buffer_evidence_complete\":true,\"full_file_buffer_bytes\":0}}"
    ));
    Ok(row)
}

pub(super) fn run_g4_row(
    root: &Path,
    size: u64,
    mode: &str,
    output_root: &Path,
) -> AnyResult<String> {
    let directory = g4_fixture_directory(root);
    let source = directory.join("target.source");
    let database = directory.join("store.sqlite");
    if !source.is_file() || !database.is_file() || !authority_path(&database).is_file() {
        return Err("G4 fixture is missing or incomplete".into());
    }
    let preflight_start = Instant::now();
    let mut preflight_metrics = Metrics::default();
    let (source_length, source_digest, _) = g4_hash_path(&source, &mut preflight_metrics)?;
    let (source_references, source_sequence) = source_cdc_sequence(&source)?;
    let source_sequence_digest = source_sequence.parse::<ObjectId>()?.to_bytes();
    if source_length != size {
        return Err(CoreError::LengthMismatch {
            expected: size,
            actual: source_length,
        }
        .into());
    }
    finish_q(&mut preflight_metrics)?;
    let preflight_ns = preflight_start.elapsed().as_nanos();
    let mut metrics = Metrics::default();
    let mut store = Store::open(&database, SELECTED_PROFILE)?;
    let g4_cache_size_pages = if matches!(
        mode,
        "r1-closure-on" | "r1-closure-off" | "r1-fresh" | "m0-candidate"
    ) {
        // ponytail: G4's synchronous read/materialization processes need deterministic
        // RSS headroom; revisit only with a newly qualified shared read-cache profile.
        store
            .connection
            .pragma_update(None, "cache_size", 1_500_i64)?;
        1_500
    } else {
        2_000
    };
    let observed_cache_size = store
        .connection
        .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))?;
    if observed_cache_size != g4_cache_size_pages {
        return Err(CoreError::ProfileMismatch.into());
    }
    let head = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    let roots = g4_roots(&store, head.1, &mut metrics)?;
    if roots.length != size || roots.references != source_references {
        return Err(CoreError::PublicationConflict.into());
    }
    finish_q(&mut metrics)?;

    if mode == "seed-read" {
        fs::create_dir(output_root)?;
        fs::set_permissions(output_root, fs::Permissions::from_mode(0o700))?;
        let output_directory = open_dir(output_root)?;
        let mut seed_metrics = Metrics::default();
        let seed_start = Instant::now();
        let seed = create_verified_seed(
            &mut store,
            roots,
            &output_directory,
            source_digest,
            &mut seed_metrics,
            #[cfg(test)]
            false,
        )?;
        let seed_fill_ns = seed_start.elapsed().as_nanos();
        let identity_before = fstat_file(&seed.file)?.0;
        let (read_ns, read_usage, read_calls, read_bytes, _) =
            g4_seed_read(&seed, false, &mut seed_metrics)?;
        let (digest_ns, digest_usage, digest_calls, digest_bytes, digest) =
            g4_seed_read(&seed, true, &mut seed_metrics)?;
        let identity_after = fstat_file(&seed.file)?.0;
        if identity_before != identity_after
            || digest != Some(source_digest)
            || read_bytes != size
            || digest_bytes != size
        {
            return Err(CoreError::IdentityMismatch.into());
        }
        finish_q(&mut seed_metrics)?;
        return g4_buffer_evidence(format!(
            "{{\"status\":\"PASS\",\"schema\":\"phase4-g4-row-v1\",\"mode\":\"seed-read\",\"cache_class\":\"same-open-protected-seed-warm-or-unknown\",\"sqlite_cache_size_pages\":{g4_cache_size_pages},\"size_bytes\":{size},\"preflight_wall_ns\":{preflight_ns},\"seed_fill_reconstruction_native_sync_readback_wall_ns\":{seed_fill_ns},\"qualified_no_digest_wall_ns\":{read_ns},\"qualified_digest_wall_ns\":{digest_ns},\"read_calls\":{read_calls},\"digest_read_calls\":{digest_calls},\"read_bytes\":{read_bytes},\"digest_read_bytes\":{digest_bytes},\"digest\":\"{}\",\"identity_stable\":true,\"buffer_bytes\":1048576,\"max_buffer_bytes\":1048576,\"q_high_water\":{},\"q_current\":{},\"operation_user_us\":{},\"operation_system_us\":{},\"operation_voluntary_switches\":{},\"operation_involuntary_switches\":{},\"digest_user_us\":{},\"digest_system_us\":{},\"seed_links\":{},\"seed_mode\":{}}}",
            hex_bytes(&source_digest),
            seed_metrics.q_high_water,
            seed_metrics.q_current,
            read_usage.user_us,
            read_usage.system_us,
            read_usage.voluntary_switches,
            read_usage.involuntary_switches,
            digest_usage.user_us,
            digest_usage.system_us,
            seed.identity.native.links,
            seed.identity.native.mode & 0o7777,
        ), SOURCE_1);
    }

    if mode == "m0-control" || mode == "m0-candidate" {
        let algorithm = if mode == "m0-control" {
            G4NativeAlgorithm::ScalarControl
        } else {
            G4NativeAlgorithm::BatchedCandidate
        };
        let result = g4_materialize(
            &mut store,
            &head,
            roots,
            source_digest,
            source_sequence_digest,
            output_root,
            algorithm,
            G4NativeFault::default(),
        )?;
        let occurrence = if mode == "m0-control" {
            "null".to_string()
        } else {
            format!("\"{}\"", hex_bytes(&result.reconstructed.occurrence_digest))
        };
        let diagnostic = result
            .diagnostic
            .map_or_else(|| "null".to_string(), |value| format!("\"{value:?}\""));
        let temp_residue_count = count_residue(output_root, ".g3-tmp-")?;
        let final_residue_count =
            u64::from(stat_at(&open_dir(output_root)?, DESTINATION_NAME)?.is_some());
        if temp_residue_count != 0 || final_residue_count != 0 {
            return Err(CoreError::PublicationConflict.into());
        }
        let mapping_singleton_queries = result
            .metrics
            .sql_query_calls
            .checked_sub(result.metrics.leaf_batch_queries)
            .ok_or(CoreError::LengthOverflow)?;
        let mapping_rows_returned = result
            .metrics
            .sql_rows_returned
            .checked_sub(result.metrics.borrowed_row_blob_reads)
            .ok_or(CoreError::LengthOverflow)?;
        return g4_buffer_evidence(format!(
            "{{\"status\":\"PASS\",\"schema\":\"phase4-g4-row-v1\",\"mode\":\"{mode}\",\"control_label\":\"{}\",\"sqlite_cache_size_pages\":{g4_cache_size_pages},\"size_bytes\":{size},\"max_buffer_bytes\":1048576,\"preflight_wall_ns\":{preflight_ns},\"operation_wall_ns\":{},\"post_operation_exact_verification_wall_ns\":{},\"cleanup_wall_ns\":{},\"root\":\"{}\",\"output_digest\":\"{}\",\"occurrence_digest\":{occurrence},\"content_closure\":null,\"content_closure_status\":\"{}\",\"references\":{},\"total_sql_query_calls\":{},\"total_sql_rows_returned\":{},\"mapping_singleton_query_calls\":{},\"mapping_rows_returned\":{},\"chunk_scalar_query_calls\":{},\"chunk_batch_query_calls\":{},\"chunk_rows_returned\":{},\"leaf_batch_references\":{},\"leaf_batch_references_max\":{},\"borrowed_chunk_blob_reads\":{},\"borrowed_chunk_blob_bytes\":{},\"authenticated_objects\":{},\"canonical_bytes_authenticated\":{},\"output_digest_hashes\":{},\"output_digest_bytes_hashed\":{},\"occurrence_fold_entries\":{},\"occurrence_fold_bytes\":{},\"closure_fold_updates\":{},\"closure_fold_canonical_bytes\":{},\"sink_emit_calls\":{},\"sink_emit_bytes\":{},\"native_write_calls\":{},\"native_write_bytes\":{},\"native_short_writes\":{},\"native_write_errors\":{},\"data_sync_calls\":{},\"metadata_operations\":{},\"metadata_sync_calls\":{},\"rename_calls\":{},\"directory_sync_calls\":{},\"reconciliation_calls\":{},\"reconciliation_outcome\":\"{}\",\"publication_status\":\"{}\",\"publication_diagnostic\":{diagnostic},\"temp_files_created\":{},\"temp_files_removed\":{},\"temp_residue_count\":{temp_residue_count},\"final_residue_count\":{final_residue_count},\"q_high_water\":{},\"q_current\":{},\"operation_user_us\":{},\"operation_system_us\":{},\"operation_voluntary_switches\":{},\"operation_involuntary_switches\":{}}}",
            if mode == "m0-control" { "g3-fallback-algorithm-control" } else { "batched-authenticated-native-writer" },
            result.wall_ns,
            result.verification_ns,
            result.cleanup_ns,
            roots.namespace,
            hex_bytes(&result.reconstructed.output_digest),
            if mode == "m0-control" { "not-computed-diagnostic-control" } else { "derived-not-computed" },
            result.reconstructed.references,
            result.metrics.sql_query_calls,
            result.metrics.sql_rows_returned,
            mapping_singleton_queries,
            mapping_rows_returned,
            if mode == "m0-control" { result.metrics.borrowed_row_blob_reads } else { 0 },
            result.metrics.leaf_batch_queries,
            result.metrics.borrowed_row_blob_reads,
            result.metrics.leaf_batch_references,
            result.metrics.leaf_batch_references_max,
            result.metrics.borrowed_row_blob_reads,
            result.metrics.borrowed_row_blob_bytes,
            result.metrics.objects_authenticated,
            result.metrics.canonical_bytes_authenticated,
            if mode == "m0-control" { 1 } else { result.reconstructed.evidence.output_digest_hashes },
            if mode == "m0-control" { result.reconstructed.length } else { result.reconstructed.evidence.output_digest_bytes_hashed },
            result.reconstructed.evidence.occurrence_fold_entries,
            result.reconstructed.evidence.occurrence_fold_bytes,
            result.reconstructed.evidence.closure_fold_updates,
            result.reconstructed.evidence.closure_fold_canonical_bytes,
            result.reconstructed.evidence.sink_write_calls,
            result.reconstructed.evidence.sink_write_bytes,
            result.writer.calls,
            result.writer.bytes,
            result.writer.short_writes,
            result.writer.errors,
            result.data_sync_calls,
            result.metadata_operations,
            result.metadata_sync_calls,
            result.rename_calls,
            result.directory_sync_calls,
            result.reconciliation_calls,
            result.reconciliation_outcome,
            result.publication_status,
            result.temp_files_created,
            result.temp_files_removed,
            result.metrics.q_high_water,
            result.metrics.q_current,
            result.usage.user_us,
            result.usage.system_us,
            result.usage.voluntary_switches,
            result.usage.involuntary_switches,
        ), result.max_single_buffer_bytes);
    }

    let (compute_closure, warm, label) = match mode {
        "r0-control" => (true, true, "current-complete-authenticated-reconstruction"),
        "r1-closure-on" => (true, true, "g4-attribution-control"),
        "r1-closure-off" => (false, true, "g4-candidate"),
        "r1-fresh" => (false, false, "g4-candidate"),
        _ => return Err(format!("unknown G4 row mode {mode}").into()),
    };
    let mut primer_ns = 0_u128;
    let mut primer_metrics = Metrics::default();
    if warm {
        let mut emit = |_raw: &[u8]| Ok(());
        let primer_start = Instant::now();
        let _primer_evidence = reconstruct_file_to(
            &store,
            roots.namespace,
            Some(&hex_bytes(&source_digest)),
            Some(&source_sequence),
            true,
            true,
            &mut primer_metrics,
            &mut emit,
        )?
        .evidence;
        primer_ns = primer_start.elapsed().as_nanos();
        finish_q(&mut primer_metrics)?;
    }
    let mut operation_metrics = Metrics::default();
    let mut emit = |_raw: &[u8]| Ok(());
    let start_usage = g4_usage()?;
    let start = Instant::now();
    let reconstructed = reconstruct_file_to(
        &store,
        roots.namespace,
        Some(&hex_bytes(&source_digest)),
        Some(&source_sequence),
        true,
        compute_closure,
        &mut operation_metrics,
        &mut emit,
    )?;
    let operation_ns = start.elapsed().as_nanos();
    let usage = g4_usage_delta(g4_usage()?, start_usage)?;
    if reconstructed.length != size
        || reconstructed.references != source_references
        || reconstructed.output_digest != source_digest
        || reconstructed.occurrence_digest != source_sequence_digest
        || store.current_head()?.as_ref() != Some(&head)
    {
        return Err(CoreError::PublicationConflict.into());
    }
    finish_q(&mut operation_metrics)?;
    let closure = reconstructed
        .content_closure
        .map(|digest| format!("\"{}\"", hex_bytes(&digest)))
        .unwrap_or_else(|| "null".to_string());
    let mapping_singleton_queries = operation_metrics
        .sql_query_calls
        .checked_sub(operation_metrics.leaf_batch_queries)
        .ok_or(CoreError::LengthOverflow)?;
    let mapping_rows_returned = operation_metrics
        .sql_rows_returned
        .checked_sub(operation_metrics.borrowed_row_blob_reads)
        .ok_or(CoreError::LengthOverflow)?;
    g4_buffer_evidence(format!(
        "{{\"status\":\"PASS\",\"schema\":\"phase4-g4-row-v1\",\"mode\":\"{mode}\",\"label\":\"{label}\",\"sqlite_cache_size_pages\":{g4_cache_size_pages},\"size_bytes\":{size},\"max_buffer_bytes\":1048576,\"cache_class\":\"{}\",\"preflight_wall_ns\":{preflight_ns},\"primer_wall_ns\":{primer_ns},\"operation_wall_ns\":{operation_ns},\"root\":\"{}\",\"output_digest\":\"{}\",\"occurrence_digest\":\"{}\",\"content_closure\":{closure},\"content_closure_status\":\"{}\",\"references\":{},\"total_sql_query_calls\":{},\"total_sql_rows_returned\":{},\"mapping_singleton_query_calls\":{},\"mapping_rows_returned\":{},\"chunk_scalar_query_calls\":0,\"chunk_batch_query_calls\":{},\"chunk_rows_returned\":{},\"leaf_batch_references\":{},\"leaf_batch_references_max\":{},\"borrowed_chunk_blob_reads\":{},\"borrowed_chunk_blob_bytes\":{},\"all_row_blob_reads\":{},\"authenticated_objects\":{},\"canonical_bytes_authenticated\":{},\"output_digest_hashes\":{},\"output_digest_bytes_hashed\":{},\"occurrence_fold_entries\":{},\"occurrence_fold_bytes\":{},\"closure_fold_enabled\":{},\"closure_fold_updates\":{},\"closure_fold_canonical_bytes\":{},\"sink_write_calls\":0,\"sink_write_bytes\":0,\"sink_short_writes\":0,\"sink_errors\":{},\"primer_sql_query_calls\":{},\"primer_authenticated_objects\":{},\"primer_q_high_water\":{},\"q_high_water\":{},\"q_current\":{},\"operation_user_us\":{},\"operation_system_us\":{},\"operation_voluntary_switches\":{},\"operation_involuntary_switches\":{}}}",
        if warm { "warm-or-unknown-after-explicit-primer" } else { "fresh-process-warm-or-unknown" },
        roots.namespace,
        hex_bytes(&reconstructed.output_digest),
        hex_bytes(&reconstructed.occurrence_digest),
        if compute_closure { "computed" } else { "derived-not-computed" },
        reconstructed.references,
        operation_metrics.sql_query_calls,
        operation_metrics.sql_rows_returned,
        mapping_singleton_queries,
        mapping_rows_returned,
        operation_metrics.leaf_batch_queries,
        operation_metrics.borrowed_row_blob_reads,
        operation_metrics.leaf_batch_references,
        operation_metrics.leaf_batch_references_max,
        operation_metrics.borrowed_row_blob_reads,
        operation_metrics.borrowed_row_blob_bytes,
        operation_metrics.row_blob_reads,
        operation_metrics.objects_authenticated,
        operation_metrics.canonical_bytes_authenticated,
        reconstructed.evidence.output_digest_hashes,
        reconstructed.evidence.output_digest_bytes_hashed,
        reconstructed.evidence.occurrence_fold_entries,
        reconstructed.evidence.occurrence_fold_bytes,
        compute_closure,
        reconstructed.evidence.closure_fold_updates,
        reconstructed.evidence.closure_fold_canonical_bytes,
        reconstructed.evidence.sink_errors,
        primer_metrics.sql_query_calls,
        primer_metrics.objects_authenticated,
        primer_metrics.q_high_water,
        operation_metrics.q_high_water,
        operation_metrics.q_current,
        usage.user_us,
        usage.system_us,
        usage.voluntary_switches,
        usage.involuntary_switches,
    ), SOURCE_1)
}

const G5_PROJECTION_FIXTURE: &str = "G5-PROJECTION-FIXTURE-v2.tsv";
const G5_PROJECTION_MAX_RANGES: usize = 256;
const G5_PROJECTION_MAX_DIRTY_BYTES: u64 = 8 * 1024 * 1024;
const G5_PROJECTION_MAX_BUFFER: u64 = 1024 * 1024;
const G5_PROJECTION_MECHANISM_BYTES: u64 = 250_000;
const G5_PROJECTION_ROUTE_CLASS: &str = "CompositePredeclaredExactCloneSparsePatchAndFullFallback";

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProjectionPlan {
    Ranges(Vec<std::ops::Range<u64>>),
    FullFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectionServiceError {
    ParentChainMismatch,
    InvalidDirtyRange,
    FixtureMismatch,
    Shutdown,
    ExactRequestPending,
    InvalidPolicyReplacement,
    WorkerFailed,
    Cancelled,
}

impl std::fmt::Display for ProjectionServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ParentChainMismatch => "ProjectionParentChainMismatch",
            Self::InvalidDirtyRange => "ProjectionInvalidDirtyRange",
            Self::FixtureMismatch => "ProjectionFixtureMismatch",
            Self::Shutdown => "ProjectionServiceShutdown",
            Self::ExactRequestPending => "ProjectionExactRequestPending",
            Self::InvalidPolicyReplacement => "ProjectionInvalidPolicyReplacement",
            Self::WorkerFailed => "ProjectionWorkerFailed",
            Self::Cancelled => "ProjectionCancelled",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectionFault {
    None,
    CancelBeforeNative,
    CancelPrivateSuccessor,
    ShutdownInflight,
    CloneFailure,
    BeforeSync,
    BeforeRename,
    RenameLostAck,
    DirectorySyncLostAck,
    ReconciliationSyncFailure,
    PostRenameStatFailure,
    ReopenFailure,
    ReaderReopenFailure,
    MissingSeed,
}

impl ProjectionFault {
    fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::CancelBeforeNative => "CancelBeforeNative",
            Self::CancelPrivateSuccessor => "CancelPrivateSuccessor",
            Self::ShutdownInflight => "ShutdownInflight",
            Self::CloneFailure => "CloneFailure",
            Self::BeforeSync => "BeforeSync",
            Self::BeforeRename => "BeforeRename",
            Self::RenameLostAck => "RenameLostAck",
            Self::DirectorySyncLostAck => "DirectorySyncLostAck",
            Self::ReconciliationSyncFailure => "ReconciliationSyncFailure",
            Self::PostRenameStatFailure => "PostRenameStatFailure",
            Self::ReopenFailure => "ReopenFailure",
            Self::ReaderReopenFailure => "ReaderReopenFailure",
            Self::MissingSeed => "MissingSeed",
        }
    }

    fn release_mode(mode: &str) -> Option<Self> {
        match mode {
            "fault-clone" => Some(Self::CloneFailure),
            "fault-rename-lost-ack" => Some(Self::RenameLostAck),
            "fault-directory-sync-lost-ack" => Some(Self::DirectorySyncLostAck),
            "fault-post-rename-stat" => Some(Self::PostRenameStatFailure),
            "fault-reopen" => Some(Self::ReopenFailure),
            "fault-missing-seed" => Some(Self::MissingSeed),
            _ => None,
        }
    }
}

fn projection_fault_selectors_json() -> String {
    format!(
        "[{}]",
        [
            ProjectionFault::CloneFailure,
            ProjectionFault::RenameLostAck,
            ProjectionFault::DirectorySyncLostAck,
            ProjectionFault::PostRenameStatFailure,
            ProjectionFault::ReopenFailure,
            ProjectionFault::MissingSeed,
        ]
        .iter()
        .map(|fault| format!("\"{}\"", fault.name()))
        .collect::<Vec<_>>()
        .join(",")
    )
}

fn projection_fault_hooks_json() -> String {
    format!(
        "[{}]",
        [
            ProjectionFault::CancelBeforeNative,
            ProjectionFault::CancelPrivateSuccessor,
            ProjectionFault::ShutdownInflight,
            ProjectionFault::CloneFailure,
            ProjectionFault::BeforeSync,
            ProjectionFault::BeforeRename,
            ProjectionFault::RenameLostAck,
            ProjectionFault::DirectorySyncLostAck,
            ProjectionFault::ReconciliationSyncFailure,
            ProjectionFault::PostRenameStatFailure,
            ProjectionFault::ReopenFailure,
            ProjectionFault::ReaderReopenFailure,
            ProjectionFault::MissingSeed,
        ]
        .iter()
        .map(|fault| format!("\"{}\"", fault.name()))
        .collect::<Vec<_>>()
        .join(",")
    )
}

impl std::error::Error for ProjectionServiceError {}

fn projection_plan(
    ranges: impl IntoIterator<Item = std::ops::Range<u64>>,
    length: u64,
) -> AnyResult<ProjectionPlan> {
    let mut admitted = Vec::with_capacity(G5_PROJECTION_MAX_RANGES);
    for (index, range) in ranges.into_iter().enumerate() {
        if index >= G5_PROJECTION_MAX_RANGES {
            return Ok(ProjectionPlan::FullFallback);
        }
        if range.start > range.end || range.end > length {
            return Err(ProjectionServiceError::InvalidDirtyRange.into());
        }
        if !range.is_empty() {
            admitted.push(range);
        }
    }
    let mut ranges = admitted;
    ranges.sort_unstable_by_key(|range| (range.start, range.end));
    let mut merged: Vec<std::ops::Range<u64>> = Vec::with_capacity(ranges.len().min(256));
    for range in ranges {
        if let Some(last) = merged.last_mut().filter(|last| range.start <= last.end) {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    let bytes = merged.iter().try_fold(0_u64, |total, range| {
        total
            .checked_add(range.end - range.start)
            .ok_or(CoreError::LengthOverflow)
    })?;
    if merged.len() > G5_PROJECTION_MAX_RANGES || bytes > G5_PROJECTION_MAX_DIRTY_BYTES {
        Ok(ProjectionPlan::FullFallback)
    } else {
        Ok(ProjectionPlan::Ranges(merged))
    }
}

fn projection_build_class(
    plan: &ProjectionPlan,
    parent_length: u64,
    target_length: u64,
) -> Option<bool> {
    match plan {
        ProjectionPlan::Ranges(ranges) if parent_length == target_length && ranges.is_empty() => {
            Some(false)
        }
        ProjectionPlan::Ranges(_) if parent_length == target_length => Some(true),
        ProjectionPlan::FullFallback | ProjectionPlan::Ranges(_) => None,
    }
}

fn latest_following_builds_to_wait(requests: usize) -> u64 {
    match requests {
        0 => 0,
        1 => 1,
        _ => 2,
    }
}

struct ProjectionFixtureEdge {
    target: Roots,
    digest: [u8; 32],
    range: std::ops::Range<u64>,
    token: Option<ProjectionEdgeToken>,
}

struct ProjectionFixture {
    directory: PathBuf,
    parent: Roots,
    target: Roots,
    parent_digest: [u8; 32],
    target_digest: [u8; 32],
    patch_bytes: Vec<u8>,
    count: Roots,
    count_digest: [u8; 32],
    storm_a: Roots,
    storm_a_digest: [u8; 32],
    storm_a_token: Option<ProjectionEdgeToken>,
    storm_b: Roots,
    storm_b_digest: [u8; 32],
    storm_b_token: Option<ProjectionEdgeToken>,
    latest: Roots,
    latest_digest: [u8; 32],
    patch: std::ops::Range<u64>,
    exact_count: usize,
    chain: Vec<ProjectionFixtureEdge>,
}

fn write_projection_fixture(
    root: &Path,
    prepared: &Prepared,
    patch_bytes: &[u8],
    count: Roots,
    count_digest: [u8; 32],
    storm_a: Roots,
    storm_a_digest: [u8; 32],
    storm_a_token: &ProjectionEdgeToken,
    storm_b: Roots,
    storm_b_digest: [u8; 32],
    storm_b_token: &ProjectionEdgeToken,
    latest: Roots,
    latest_digest: [u8; 32],
    exact_count: usize,
    chain: &[ProjectionFixtureEdge],
) -> AnyResult<PathBuf> {
    let path = root.join(G5_PROJECTION_FIXTURE);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    writeln!(file, "directory\t{}", prepared.directory_path.display())?;
    writeln!(file, "parent_namespace\t{}", prepared.parent.namespace)?;
    writeln!(file, "parent_file\t{}", prepared.parent.file)?;
    writeln!(file, "parent_length\t{}", prepared.parent.length)?;
    writeln!(file, "parent_references\t{}", prepared.parent.references)?;
    writeln!(
        file,
        "parent_digest\t{}",
        hex_bytes(&prepared.parent_digest)
    )?;
    writeln!(file, "target_namespace\t{}", prepared.target.namespace)?;
    writeln!(file, "target_file\t{}", prepared.target.file)?;
    writeln!(file, "target_length\t{}", prepared.target.length)?;
    writeln!(file, "target_references\t{}", prepared.target.references)?;
    writeln!(
        file,
        "target_digest\t{}",
        hex_bytes(&prepared.target_digest)
    )?;
    writeln!(file, "patch_bytes\t{}", hex_bytes(patch_bytes))?;
    for (name, roots, digest) in [
        ("count", count, count_digest),
        ("storm_a", storm_a, storm_a_digest),
        ("storm_b", storm_b, storm_b_digest),
        ("latest", latest, latest_digest),
    ] {
        writeln!(file, "{name}_namespace\t{}", roots.namespace)?;
        writeln!(file, "{name}_file\t{}", roots.file)?;
        writeln!(file, "{name}_length\t{}", roots.length)?;
        writeln!(file, "{name}_references\t{}", roots.references)?;
        writeln!(file, "{name}_digest\t{}", hex_bytes(&digest))?;
    }
    writeln!(file, "storm_a_token\t{}", storm_a_token.serialize())?;
    writeln!(file, "storm_b_token\t{}", storm_b_token.serialize())?;
    writeln!(
        file,
        "patch\t{}\t{}",
        prepared.patch.start, prepared.patch.end
    )?;
    writeln!(file, "exact_count\t{exact_count}")?;
    writeln!(file, "chain_count\t{}", chain.len())?;
    for (index, edge) in chain.iter().enumerate() {
        writeln!(
            file,
            "chain_{index:03}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            edge.target.namespace,
            edge.target.file,
            edge.target.length,
            edge.target.references,
            hex_bytes(&edge.digest),
            edge.range.start,
            edge.range.end,
            edge.token
                .as_ref()
                .ok_or(CoreError::InvalidValidationReceipt)?
                .serialize(),
        )?;
    }
    file.sync_all()?;
    sync_fd(&open_dir(root)?)?;
    Ok(path)
}

fn parse_projection_fixture(root: &Path) -> AnyResult<ProjectionFixture> {
    let text = fs::read_to_string(root.join(G5_PROJECTION_FIXTURE))?;
    let mut fields = std::collections::BTreeMap::new();
    for line in text.lines() {
        let mut parts = line.split('\t');
        let key = parts
            .next()
            .ok_or(ProjectionServiceError::FixtureMismatch)?;
        let values: Vec<_> = parts.collect();
        if values.is_empty() || fields.insert(key, values).is_some() {
            return Err(ProjectionServiceError::FixtureMismatch.into());
        }
    }
    let one = |name: &str| -> AnyResult<&str> {
        let values = fields
            .get(name)
            .ok_or(ProjectionServiceError::FixtureMismatch)?;
        if values.len() != 1 {
            return Err(ProjectionServiceError::FixtureMismatch.into());
        }
        Ok(values[0])
    };
    let roots = |prefix: &str| -> AnyResult<Roots> {
        Ok(Roots {
            namespace: one(&format!("{prefix}_namespace"))?.parse()?,
            file: one(&format!("{prefix}_file"))?.parse()?,
            length: one(&format!("{prefix}_length"))?.parse()?,
            references: one(&format!("{prefix}_references"))?.parse()?,
        })
    };
    let digest =
        |name: &str| -> AnyResult<[u8; 32]> { Ok(one(name)?.parse::<ObjectId>()?.to_bytes()) };
    let patch = fields
        .get("patch")
        .filter(|values| values.len() == 2)
        .ok_or(ProjectionServiceError::FixtureMismatch)?;
    let fixture = ProjectionFixture {
        directory: PathBuf::from(one("directory")?),
        parent: roots("parent")?,
        target: roots("target")?,
        parent_digest: digest("parent_digest")?,
        target_digest: digest("target_digest")?,
        patch_bytes: decode_hex(one("patch_bytes")?)?,
        count: roots("count")?,
        count_digest: digest("count_digest")?,
        storm_a: roots("storm_a")?,
        storm_a_digest: digest("storm_a_digest")?,
        storm_a_token: Some(ProjectionEdgeToken::parse(one("storm_a_token")?)?),
        storm_b: roots("storm_b")?,
        storm_b_digest: digest("storm_b_digest")?,
        storm_b_token: Some(ProjectionEdgeToken::parse(one("storm_b_token")?)?),
        latest: roots("latest")?,
        latest_digest: digest("latest_digest")?,
        patch: patch[0].parse()?..patch[1].parse()?,
        exact_count: one("exact_count")?.parse()?,
        chain: (0..one("chain_count")?.parse::<usize>()?)
            .map(|index| -> AnyResult<_> {
                let key = format!("chain_{index:03}");
                let values = fields
                    .get(key.as_str())
                    .filter(|values| values.len() == 8)
                    .ok_or(ProjectionServiceError::FixtureMismatch)?;
                Ok(ProjectionFixtureEdge {
                    target: Roots {
                        namespace: values[0].parse()?,
                        file: values[1].parse()?,
                        length: values[2].parse()?,
                        references: values[3].parse()?,
                    },
                    digest: values[4].parse::<ObjectId>()?.to_bytes(),
                    range: values[5].parse()?..values[6].parse()?,
                    token: Some(ProjectionEdgeToken::parse(values[7])?),
                })
            })
            .collect::<AnyResult<Vec<_>>>()?,
    };
    if !fixture.directory.starts_with(root)
        || fixture.patch.end > fixture.target.length
        || fixture.patch_bytes.len()
            != usize::try_from(fixture.patch.end - fixture.patch.start)
                .map_err(|_| CoreError::LengthOverflow)?
    {
        return Err(ProjectionServiceError::FixtureMismatch.into());
    }
    Ok(fixture)
}

#[derive(Default)]
struct ProjectionCounters {
    submitted: u64,
    started: u64,
    completed: u64,
    superseded_pending: u64,
    cancelled: u64,
    failed: u64,
    stale: u64,
    full_fallbacks: u64,
    range_fetches: u64,
    fetched_bytes: u64,
    write_calls: u64,
    written_bytes: u64,
    clone_calls: u64,
    clone_successes: u64,
    clone_failures: u64,
    temp_files_created: u64,
    temp_files_removed: u64,
    private_build_cancellations: u64,
    restart_temps_discovered: u64,
    restart_temps_removed: u64,
    restart_temps_retained: u64,
    missing_seed_fallbacks: u64,
    seed_admission_rejections: u64,
    data_sync_calls: u64,
    metadata_sync_calls: u64,
    rename_calls: u64,
    directory_sync_calls: u64,
    reconciliation_calls: u64,
    sqlite_write_calls: u64,
    sqlite_transactions: u64,
    sqlite_commits: u64,
    sqlite_busy_errors: u64,
    sqlite_locked_errors: u64,
    seed_rotations: u64,
    q_high_water: u64,
    q_terminal: u64,
    max_buffer_bytes: u64,
    max_in_flight: u64,
    max_pending: u64,
    payload_ns: u128,
    durability_ns: u128,
    verification_ns: u128,
    sql_queries: u64,
    sql_rows: u64,
    blob_reads: u64,
    blob_bytes: u64,
    authenticated_objects: u64,
    authenticated_bytes: u64,
    exact_build_ns: Vec<u128>,
    sparse_build_ns: Vec<u128>,
    fallback_build_ns: Vec<u128>,
    contention_fallback_build_ns: Vec<u128>,
    build_evidence: Vec<ProjectionBuildEvidence>,
    reader_initialization_ns: u128,
    reader_initialization_calls: u64,
    reader_initialization_sql_queries: u64,
    reader_initialization_authenticated_objects: u64,
    reader_initialization_authenticated_bytes: u64,
    reader_initialization_q_high_water: u64,
    contention_worker_start_ns: u128,
    contention_worker_end_ns: u128,
    end_to_end_edit_t0_ns: u128,
    end_to_end_canonical_ack_t1_ns: u128,
    end_to_end_enqueue_t2_ns: u128,
    end_to_end_worker_start_t3_ns: u128,
    end_to_end_native_ack_t4_ns: u128,
    end_to_end_edit_wall_ns: u128,
    end_to_end_canonical_ack_wall_ns: u128,
    end_to_end_canonical_transactions: u64,
    end_to_end_canonical_commits: u64,
    end_to_end_canonical_sql_queries: u64,
    end_to_end_canonical_authenticated_objects: u64,
    end_to_end_canonical_authenticated_bytes: u64,
    end_to_end_canonical_q_high_water: u64,
    initial_descriptor_verification_bytes: u64,
    initial_storage_logical_bytes: u64,
    initial_storage_apparent_bytes: u64,
    initial_storage_allocated_bytes: u64,
    fault_finalizations: u64,
    fault_outcome: Option<ProjectionFaultOutcome>,
    fault_q_terminal: u64,
    fault_temp_residue: u64,
    fault_active_descriptors: u64,
    fault_successor_descriptors: u64,
    fault_storage_logical_bytes: u64,
    fault_storage_apparent_bytes: u64,
    fault_storage_allocated_bytes: u64,
    fault_apply_wall_ns: u128,
    fault_finalization_ns: u128,
    fault_finalization_complete: bool,
    fault_unwind_temp_removals: u64,
    fault_active_identity_matches: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectionFaultOutcome {
    Cancelled,
    Shutdown,
    Stale,
    WorkerFailed,
    IdentityMismatch,
    WrongLogicalRole,
    AmbiguousDurability,
    SqliteBusy,
    SqliteLocked,
    IoError,
    Rejected,
}

impl ProjectionFaultOutcome {
    fn name(self) -> &'static str {
        match self {
            Self::Cancelled => "Cancelled",
            Self::Shutdown => "Shutdown",
            Self::Stale => "Stale",
            Self::WorkerFailed => "WorkerFailed",
            Self::IdentityMismatch => "IdentityMismatch",
            Self::WrongLogicalRole => "WrongLogicalRole",
            Self::AmbiguousDurability => "AmbiguousDurability",
            Self::SqliteBusy => "SqliteBusy",
            Self::SqliteLocked => "SqliteLocked",
            Self::IoError => "IoError",
            Self::Rejected => "Rejected",
        }
    }
}

impl ProjectionCounters {
    fn fault_finalization_json(&self) -> String {
        let invariants_pass = self.fault_finalization_complete
            && self.fault_q_terminal == 0
            && self.fault_temp_residue == 0
            && self.fault_active_descriptors == 1
            && self.fault_successor_descriptors == 0;
        format!(
            concat!(
                "{{\"status\":\"{}\",\"receipt_complete\":{},\"invariants_pass\":{},",
                "\"typed_outcome\":\"{}\",",
                "\"fault_finalizations\":{},\"q_terminal\":{},\"temp_residue\":{},",
                "\"temp_files_created\":{},\"temp_files_removed\":{},",
                "\"unwind_temp_removals\":{},",
                "\"unwind_temp_removal_provenance\":\"ProvenByOwnedTempDropPlusObservedZeroResidueBeforeRename\",",
                "\"clone_calls\":{},\"clone_failures\":{},",
                "\"data_sync_calls\":{},\"metadata_sync_calls\":{},",
                "\"rename_calls\":{},\"directory_sync_calls\":{},",
                "\"reconciliation_calls\":{},\"sql_queries\":{},",
                "\"authenticated_objects\":{},\"authenticated_bytes\":{},",
                "\"active_descriptors\":{},\"successor_descriptors\":{},",
                "\"active_identity_matches_cached\":{},",
                "\"active_descriptor_provenance\":\"ObservedFstatRetainedActiveDescriptor\",",
                "\"successor_descriptor_provenance\":\"ProvenByApplyInnerStackUnwindBeforeFinalization\",",
                "\"storage_logical_bytes\":{},\"storage_apparent_bytes\":{},",
                "\"storage_allocated_bytes\":{},\"apply_wall_ns\":{},",
                "\"finalization_wall_ns\":{}}}"
            ),
            if invariants_pass {
                "PASS"
            } else {
                "REVISE"
            },
            self.fault_finalization_complete,
            invariants_pass,
            self.fault_outcome
                .map(ProjectionFaultOutcome::name)
                .unwrap_or("NotAttempted"),
            self.fault_finalizations,
            self.fault_q_terminal,
            self.fault_temp_residue,
            self.temp_files_created,
            self.temp_files_removed,
            self.fault_unwind_temp_removals,
            self.clone_calls,
            self.clone_failures,
            self.data_sync_calls,
            self.metadata_sync_calls,
            self.rename_calls,
            self.directory_sync_calls,
            self.reconciliation_calls,
            self.sql_queries,
            self.authenticated_objects,
            self.authenticated_bytes,
            self.fault_active_descriptors,
            self.fault_successor_descriptors,
            self.fault_active_identity_matches,
            self.fault_storage_logical_bytes,
            self.fault_storage_apparent_bytes,
            self.fault_storage_allocated_bytes,
            self.fault_apply_wall_ns,
            self.fault_finalization_ns,
        )
    }
}

struct ProjectionBuildEvidence {
    plan: &'static str,
    parent_length: u64,
    target_length: u64,
    range_count: usize,
    wall_ns: u128,
    contention: bool,
    policy: &'static str,
    ordinal: u64,
    fault: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestPolicy {
    ExactEveryRoot { ordinal: u64 },
    LatestFollowing { stream: LatestStream, ordinal: u64 },
    IsolatedSparseSentinel,
    IsolatedOrdinaryFallback,
}

impl RequestPolicy {
    fn evidence(self) -> (&'static str, u64) {
        match self {
            Self::ExactEveryRoot { ordinal } => ("ExactEveryRoot", ordinal),
            Self::LatestFollowing {
                stream: LatestStream::SameSize,
                ordinal,
            } => ("LatestFollowingSameSize", ordinal),
            Self::LatestFollowing {
                stream: LatestStream::CountStorm,
                ordinal,
            } => ("LatestFollowingCountStorm", ordinal),
            Self::IsolatedSparseSentinel => ("IsolatedSparseSentinel", 0),
            Self::IsolatedOrdinaryFallback => ("IsolatedOrdinaryFallback", 0),
        }
    }
}

fn projection_chain_policy(chain_index: usize, exact_count: usize) -> AnyResult<RequestPolicy> {
    if chain_index < exact_count.saturating_sub(1) {
        Ok(RequestPolicy::ExactEveryRoot {
            ordinal: u64::try_from(chain_index + 1).map_err(|_| CoreError::LengthOverflow)?,
        })
    } else {
        Ok(RequestPolicy::LatestFollowing {
            stream: LatestStream::SameSize,
            ordinal: u64::try_from(chain_index - exact_count.saturating_sub(1))
                .map_err(|_| CoreError::LengthOverflow)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LatestStream {
    SameSize,
    CountStorm,
}

#[derive(Clone, Copy)]
struct ProjectionTokenAuthority {
    store_instance_id: [u8; 16],
    validation_authority_id: [u8; 32],
    validation_key: [u8; 32],
    profile: [u8; 32],
    integrity_epoch: u64,
}

impl ProjectionTokenAuthority {
    fn from_store(store: &Store) -> Self {
        Self {
            store_instance_id: store.store_instance_id,
            validation_authority_id: store.validation_authority_id,
            validation_key: store.validation_key,
            profile: store.profile,
            integrity_epoch: store.integrity_epoch,
        }
    }
}

struct ProjectionEdgeBinding {
    store_instance_id: [u8; 16],
    validation_authority_id: [u8; 32],
    profile: [u8; 32],
    integrity_epoch: u64,
    generation: u64,
    head_root: ObjectId,
    transition: ObjectId,
    receipt: [u8; 216],
    parent: Roots,
    parent_digest: [u8; 32],
    target: Roots,
    target_digest: [u8; 32],
    ranges: Vec<std::ops::Range<u64>>,
    policy: RequestPolicy,
}

struct ProjectionEdgeToken {
    binding: ProjectionEdgeBinding,
    tag: [u8; 32],
    consumed: bool,
}

fn projection_edge_tag(
    validation_key: &[u8; 32],
    binding: &ProjectionEdgeBinding,
) -> AnyResult<[u8; 32]> {
    let mut hasher = blake3::Hasher::new_keyed(validation_key);
    hasher.update(b"layerfs/phase4/g5/projection-edge-token/v2\0");
    hasher.update(&binding.store_instance_id);
    hasher.update(&binding.validation_authority_id);
    hasher.update(&binding.profile);
    hasher.update(&binding.integrity_epoch.to_be_bytes());
    hasher.update(&binding.generation.to_be_bytes());
    hasher.update(binding.head_root.as_bytes());
    hasher.update(binding.transition.as_bytes());
    hasher.update(&binding.receipt);
    for roots in [binding.parent, binding.target] {
        hasher.update(roots.namespace.as_bytes());
        hasher.update(roots.file.as_bytes());
        hasher.update(&roots.length.to_be_bytes());
        hasher.update(&roots.references.to_be_bytes());
    }
    hasher.update(&binding.parent_digest);
    hasher.update(&binding.target_digest);
    let (name, ordinal) = binding.policy.evidence();
    hash_field(&mut hasher, name.as_bytes())?;
    hasher.update(&ordinal.to_be_bytes());
    hasher.update(
        &u64::try_from(binding.ranges.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    for range in &binding.ranges {
        hasher.update(&range.start.to_be_bytes());
        hasher.update(&range.end.to_be_bytes());
    }
    Ok(*hasher.finalize().as_bytes())
}

fn mint_projection_edge_token(
    store: &Store,
    head: &VisibleHead,
    parent: Roots,
    target: Roots,
    parent_digest: [u8; 32],
    target_digest: [u8; 32],
    ranges: &[std::ops::Range<u64>],
    policy: RequestPolicy,
) -> AnyResult<ProjectionEdgeToken> {
    if head.1 != target.namespace
        || projection_plan(ranges.iter().cloned(), target.length)?
            != ProjectionPlan::Ranges(ranges.to_vec())
    {
        return Err(CoreError::PublicationConflict.into());
    }
    let binding = ProjectionEdgeBinding {
        store_instance_id: store.store_instance_id,
        validation_authority_id: store.validation_authority_id,
        profile: store.profile,
        integrity_epoch: store.integrity_epoch,
        generation: head.0,
        head_root: head.1,
        transition: head.2,
        receipt: head.3,
        parent,
        parent_digest,
        target,
        target_digest,
        ranges: ranges.to_vec(),
        policy,
    };
    Ok(ProjectionEdgeToken {
        tag: projection_edge_tag(&store.validation_key, &binding)?,
        binding,
        consumed: false,
    })
}

impl ProjectionEdgeToken {
    fn serialize(&self) -> String {
        let binding = &self.binding;
        let (policy, ordinal) = binding.policy.evidence();
        let ranges = binding
            .ranges
            .iter()
            .map(|range| format!("{}-{}", range.start, range.end))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "v2|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            hex_bytes(&binding.store_instance_id),
            hex_bytes(&binding.validation_authority_id),
            hex_bytes(&binding.profile),
            binding.integrity_epoch,
            binding.generation,
            binding.head_root,
            binding.transition,
            hex_bytes(&binding.receipt),
            binding.parent.namespace,
            binding.parent.file,
            binding.parent.length,
            binding.parent.references,
            hex_bytes(&binding.parent_digest),
            binding.target.namespace,
            binding.target.file,
            binding.target.length,
            binding.target.references,
            hex_bytes(&binding.target_digest),
            policy,
            ordinal,
            binding.ranges.len(),
            ranges,
            hex_bytes(&self.tag),
            u8::from(self.consumed),
        )
    }

    fn parse(value: &str) -> AnyResult<Self> {
        let mut split = value.split('|');
        let mut fields = [""; 25];
        for field in &mut fields {
            *field = split.next().ok_or(CoreError::InvalidValidationReceipt)?;
        }
        if split.next().is_some() || fields[0] != "v2" || fields[24] != "0" {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        let fixed = |value: &str, length: usize| -> AnyResult<Vec<u8>> {
            let bytes = decode_hex(value)?;
            if bytes.len() != length {
                return Err(CoreError::InvalidValidationReceipt.into());
            }
            Ok(bytes)
        };
        let fixed_array = |value: &str| -> AnyResult<[u8; 32]> {
            Ok(fixed(value, 32)?
                .try_into()
                .map_err(|_| CoreError::InvalidValidationReceipt)?)
        };
        let policy = match (fields[19], fields[20].parse()?) {
            ("ExactEveryRoot", ordinal) => RequestPolicy::ExactEveryRoot { ordinal },
            ("LatestFollowingSameSize", ordinal) => RequestPolicy::LatestFollowing {
                stream: LatestStream::SameSize,
                ordinal,
            },
            ("LatestFollowingCountStorm", ordinal) => RequestPolicy::LatestFollowing {
                stream: LatestStream::CountStorm,
                ordinal,
            },
            ("IsolatedSparseSentinel", 0) => RequestPolicy::IsolatedSparseSentinel,
            ("IsolatedOrdinaryFallback", 0) => RequestPolicy::IsolatedOrdinaryFallback,
            _ => return Err(CoreError::InvalidValidationReceipt.into()),
        };
        let declared_ranges = fields[21].parse::<usize>()?;
        if declared_ranges > G5_PROJECTION_MAX_RANGES
            || (!fields[22].is_empty() && fields[22].split(',').count() != declared_ranges)
            || (fields[22].is_empty() && declared_ranges != 0)
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        let ranges = if fields[22].is_empty() {
            Vec::new()
        } else {
            fields[22]
                .split(',')
                .map(|value| -> AnyResult<_> {
                    let (start, end) = value
                        .split_once('-')
                        .ok_or(CoreError::InvalidValidationReceipt)?;
                    Ok(start.parse()?..end.parse()?)
                })
                .collect::<AnyResult<Vec<_>>>()?
        };
        if ranges.len() != declared_ranges
            || projection_plan(ranges.iter().cloned(), fields[16].parse()?)?
                != ProjectionPlan::Ranges(ranges.clone())
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        Ok(Self {
            binding: ProjectionEdgeBinding {
                store_instance_id: fixed(fields[1], 16)?
                    .try_into()
                    .map_err(|_| CoreError::InvalidValidationReceipt)?,
                validation_authority_id: fixed_array(fields[2])?,
                profile: fixed_array(fields[3])?,
                integrity_epoch: fields[4].parse()?,
                generation: fields[5].parse()?,
                head_root: fields[6].parse()?,
                transition: fields[7].parse()?,
                receipt: fixed(fields[8], 216)?
                    .try_into()
                    .map_err(|_| CoreError::InvalidValidationReceipt)?,
                parent: Roots {
                    namespace: fields[9].parse()?,
                    file: fields[10].parse()?,
                    length: fields[11].parse()?,
                    references: fields[12].parse()?,
                },
                parent_digest: fixed_array(fields[13])?,
                target: Roots {
                    namespace: fields[14].parse()?,
                    file: fields[15].parse()?,
                    length: fields[16].parse()?,
                    references: fields[17].parse()?,
                },
                target_digest: fixed_array(fields[18])?,
                ranges,
                policy,
            },
            tag: fixed_array(fields[23])?,
            consumed: false,
        })
    }

    fn verify(
        &self,
        authority: ProjectionTokenAuthority,
        request: &ProjectionRequest,
    ) -> AnyResult<()> {
        let binding = &self.binding;
        let receipt = ValidatedSnapshotReceiptV1::decode(
            &binding.receipt,
            &authority.validation_key,
            ObjectId::from_bytes(&authority.profile)?,
            authority.validation_authority_id,
        )?;
        if self.consumed
            || binding.store_instance_id != authority.store_instance_id
            || binding.validation_authority_id != authority.validation_authority_id
            || binding.profile != authority.profile
            || binding.integrity_epoch != authority.integrity_epoch
            || receipt.store_instance_id != binding.store_instance_id
            || receipt.integrity_epoch != binding.integrity_epoch
            || receipt.head_generation != binding.generation
            || receipt.child_root_id != binding.head_root
            || receipt.transition_id != binding.transition
            || binding.head_root != binding.target.namespace
            || binding.parent != request.parent
            || binding.parent_digest != request.parent_digest
            || binding.target != request.target
            || binding.target_digest != request.target_digest
            || binding.ranges != request.ranges()
            || binding.policy != request.policy
            || self.tag != projection_edge_tag(&authority.validation_key, binding)?
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        Ok(())
    }

    fn consume(&mut self) -> AnyResult<()> {
        if self.consumed {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        self.consumed = true;
        Ok(())
    }
}

fn build_projection_target(store: &mut Store, source: &Path) -> AnyResult<(Roots, VisibleHead)> {
    let mut metrics = Metrics::default();
    let roots = build_and_publish_target(store, source, &mut metrics)?;
    finish_q(&mut metrics)?;
    let head = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    if head.1 != roots.namespace {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok((roots, head))
}

#[allow(clippy::too_many_arguments)]
fn prove_and_mint_projection_edge(
    store: &mut Store,
    head: &VisibleHead,
    parent: Roots,
    parent_digest: [u8; 32],
    parent_source: &Path,
    target: Roots,
    target_digest: [u8; 32],
    target_source: &Path,
    range: &std::ops::Range<u64>,
    policy: RequestPolicy,
) -> AnyResult<ProjectionEdgeToken> {
    let mut metrics = Metrics::default();
    let proof = prove_canonical_range(
        store,
        parent,
        parent_digest,
        parent_source,
        target,
        target_digest,
        target_source,
        range,
        &mut metrics,
    )?;
    finish_q(&mut metrics)?;
    if proof.binding.parent_root != parent.namespace
        || proof.binding.target_root != target.namespace
        || proof.binding.range_start != range.start
        || proof.binding.range_end != range.end
    {
        return Err(CoreError::InvalidValidationReceipt.into());
    }
    verify_transition(
        store,
        head.2,
        Some(parent.namespace),
        target.namespace,
        None,
        &mut metrics,
    )?;
    finish_q(&mut metrics)?;
    mint_projection_edge_token(
        store,
        head,
        parent,
        target,
        parent_digest,
        target_digest,
        std::slice::from_ref(range),
        policy,
    )
}

struct ProjectionLiveEdge {
    roots: Roots,
    digest: [u8; 32],
    token: ProjectionEdgeToken,
    edit_t0_ns: u128,
    canonical_ack_t1_ns: u128,
    edit_wall_ns: u128,
    canonical_ack_wall_ns: u128,
    transactions: u64,
    commits: u64,
    sql_queries: u64,
    authenticated_objects: u64,
    authenticated_bytes: u64,
    q_high_water: u64,
}

fn build_live_projection_edge(
    store_path: &Path,
    directory: &Path,
    parent: Roots,
    parent_digest: [u8; 32],
    range: &std::ops::Range<u64>,
    replacement: &[u8],
    origin: Instant,
) -> AnyResult<ProjectionLiveEdge> {
    let edit_t0_ns = origin.elapsed().as_nanos();
    let edit_started = Instant::now();
    let source = directory.join("projection-live-first.source");
    fs::copy(directory.join("parent.source"), &source)?;
    if replacement.len() != usize::try_from(range.end - range.start)? {
        return Err(CoreError::LengthMismatch {
            expected: range.end - range.start,
            actual: u64::try_from(replacement.len())?,
        }
        .into());
    }
    let mut edited = OpenOptions::new().read(true).write(true).open(&source)?;
    edited.seek(SeekFrom::Start(range.start))?;
    edited.write_all(replacement)?;
    edited.sync_all()?;
    drop(edited);
    let edit_wall_ns = edit_started.elapsed().as_nanos();
    let (_, digest, _) = hash_file(&source)?;

    let canonical_started = Instant::now();
    let mut store = Store::open(store_path, SELECTED_PROFILE)?;
    let mut metrics = Metrics::default();
    let roots = build_and_publish_target(&mut store, &source, &mut metrics)?;
    let head = store
        .current_head_accounted(&mut metrics)?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    let proof = prove_canonical_range(
        &mut store,
        parent,
        parent_digest,
        &directory.join("parent.source"),
        roots,
        digest,
        &source,
        range,
        &mut metrics,
    )?;
    if proof.binding.parent_root != parent.namespace
        || proof.binding.target_root != roots.namespace
        || proof.binding.range_start != range.start
        || proof.binding.range_end != range.end
    {
        return Err(CoreError::InvalidValidationReceipt.into());
    }
    verify_transition(
        &store,
        head.2,
        Some(parent.namespace),
        roots.namespace,
        None,
        &mut metrics,
    )?;
    let token = mint_projection_edge_token(
        &store,
        &head,
        parent,
        roots,
        parent_digest,
        digest,
        std::slice::from_ref(range),
        RequestPolicy::IsolatedSparseSentinel,
    )?;
    finish_q(&mut metrics)?;
    let canonical_ack_wall_ns = canonical_started.elapsed().as_nanos();
    Ok(ProjectionLiveEdge {
        roots,
        digest,
        token,
        edit_t0_ns,
        canonical_ack_t1_ns: origin.elapsed().as_nanos(),
        edit_wall_ns,
        canonical_ack_wall_ns,
        transactions: metrics.transactions,
        commits: metrics.commits,
        sql_queries: metrics.sql_query_calls,
        authenticated_objects: metrics.objects_authenticated,
        authenticated_bytes: metrics.canonical_bytes_authenticated,
        q_high_water: metrics.q_high_water,
    })
}

struct ProjectionRequest {
    parent: Roots,
    parent_digest: [u8; 32],
    target: Roots,
    target_digest: [u8; 32],
    plan: ProjectionPlan,
    contended: bool,
    policy: RequestPolicy,
    force_full_fallback: bool,
    token: Option<ProjectionEdgeToken>,
    edge_authenticated: bool,
    end_to_end: Option<ProjectionEndToEndTiming>,
    fault: ProjectionFault,
}

impl ProjectionRequest {
    fn ranges(&self) -> &[std::ops::Range<u64>] {
        match &self.plan {
            ProjectionPlan::Ranges(ranges) => ranges,
            ProjectionPlan::FullFallback => &[],
        }
    }
}

#[derive(Clone, Copy)]
struct ProjectionEndToEndTiming {
    edit_t0_ns: u128,
    canonical_ack_t1_ns: u128,
    enqueue_t2_ns: Option<u128>,
}

struct ProjectionMailbox {
    in_flight: bool,
    pending: Option<ProjectionRequest>,
    shutdown: bool,
    release_first: bool,
    submitted: u64,
    coalesced: u64,
    started: u64,
    published: u64,
    cancelled: u64,
    failed: u64,
    stale: u64,
    sqlite_busy_errors: u64,
    sqlite_locked_errors: u64,
    worker_error: Option<String>,
    token_authority: ProjectionTokenAuthority,
    exact_ordinal: Option<u64>,
    same_size_ordinal: Option<u64>,
    count_storm_ordinal: Option<u64>,
    isolated_sparse_accepted: bool,
}

fn projection_sqlite_error_counts(mut error: &(dyn std::error::Error + 'static)) -> (u64, u64) {
    loop {
        if let Some(rusqlite::Error::SqliteFailure(code, _)) =
            error.downcast_ref::<rusqlite::Error>()
        {
            return match code.extended_code & 0xff {
                rusqlite::ffi::SQLITE_BUSY => (1, 0),
                rusqlite::ffi::SQLITE_LOCKED => (0, 1),
                _ => (0, 0),
            };
        }
        let Some(source) = error.source() else {
            return (0, 0);
        };
        error = source;
    }
}

fn projection_fault_outcome(error: &(dyn std::error::Error + 'static)) -> ProjectionFaultOutcome {
    let (busy, locked) = projection_sqlite_error_counts(error);
    if busy != 0 {
        return ProjectionFaultOutcome::SqliteBusy;
    }
    if locked != 0 {
        return ProjectionFaultOutcome::SqliteLocked;
    }
    if let Some(error) = error.downcast_ref::<ProjectionServiceError>() {
        return match error {
            ProjectionServiceError::Cancelled => ProjectionFaultOutcome::Cancelled,
            ProjectionServiceError::Shutdown => ProjectionFaultOutcome::Shutdown,
            ProjectionServiceError::ParentChainMismatch => ProjectionFaultOutcome::Stale,
            ProjectionServiceError::WorkerFailed => ProjectionFaultOutcome::WorkerFailed,
            ProjectionServiceError::InvalidDirtyRange
            | ProjectionServiceError::FixtureMismatch
            | ProjectionServiceError::ExactRequestPending
            | ProjectionServiceError::InvalidPolicyReplacement => ProjectionFaultOutcome::Rejected,
        };
    }
    if let Some(error) = error.downcast_ref::<CoreError>() {
        return match error {
            CoreError::IdentityMismatch => ProjectionFaultOutcome::IdentityMismatch,
            CoreError::WrongLogicalRole => ProjectionFaultOutcome::WrongLogicalRole,
            CoreError::AmbiguousDurability => ProjectionFaultOutcome::AmbiguousDurability,
            _ => ProjectionFaultOutcome::Rejected,
        };
    }
    if error.downcast_ref::<std::io::Error>().is_some() {
        ProjectionFaultOutcome::IoError
    } else {
        ProjectionFaultOutcome::Rejected
    }
}

struct ProjectionWorkerTerminal {
    shared: std::sync::Arc<(std::sync::Mutex<ProjectionMailbox>, std::sync::Condvar)>,
    success: bool,
}

impl Drop for ProjectionWorkerTerminal {
    fn drop(&mut self) {
        if self.success {
            return;
        }
        let (mutex, condition) = &*self.shared;
        if let Ok(mut mailbox) = mutex.lock() {
            mailbox.in_flight = false;
            mailbox.shutdown = true;
            if mailbox.worker_error.is_none() {
                mailbox.worker_error = Some("ProjectionWorkerTerminated".into());
            }
        }
        condition.notify_all();
    }
}

impl ProjectionMailbox {
    fn policy_is_fresh(&self, policy: RequestPolicy) -> bool {
        match policy {
            RequestPolicy::ExactEveryRoot { ordinal } => {
                self.exact_ordinal.is_none_or(|prior| ordinal > prior)
            }
            RequestPolicy::LatestFollowing {
                stream: LatestStream::SameSize,
                ordinal,
            } => self.same_size_ordinal.is_none_or(|prior| ordinal > prior),
            RequestPolicy::LatestFollowing {
                stream: LatestStream::CountStorm,
                ordinal,
            } => self.count_storm_ordinal.is_none_or(|prior| ordinal > prior),
            RequestPolicy::IsolatedSparseSentinel => !self.isolated_sparse_accepted,
            RequestPolicy::IsolatedOrdinaryFallback => true,
        }
    }

    fn record_policy(&mut self, policy: RequestPolicy) {
        match policy {
            RequestPolicy::ExactEveryRoot { ordinal } => self.exact_ordinal = Some(ordinal),
            RequestPolicy::LatestFollowing {
                stream: LatestStream::SameSize,
                ordinal,
            } => self.same_size_ordinal = Some(ordinal),
            RequestPolicy::LatestFollowing {
                stream: LatestStream::CountStorm,
                ordinal,
            } => self.count_storm_ordinal = Some(ordinal),
            RequestPolicy::IsolatedSparseSentinel => self.isolated_sparse_accepted = true,
            RequestPolicy::IsolatedOrdinaryFallback => {}
        }
    }

    fn submit(&mut self, request: ProjectionRequest) -> AnyResult<()> {
        self.submit_with_origin(request, None).map(|_| ())
    }

    fn submit_with_origin(
        &mut self,
        mut request: ProjectionRequest,
        origin: Option<Instant>,
    ) -> AnyResult<Option<u128>> {
        if self.shutdown {
            return Err(ProjectionServiceError::Shutdown.into());
        }
        if origin.is_some()
            && !matches!(
                request.end_to_end,
                Some(ProjectionEndToEndTiming {
                    enqueue_t2_ns: None,
                    ..
                })
            )
        {
            return Err(CoreError::PublicationConflict.into());
        }
        if matches!(request.plan, ProjectionPlan::FullFallback) {
            request.force_full_fallback = true;
        }
        let sparse_edge = request.parent.length == request.target.length
            && !request.force_full_fallback
            && !request.ranges().is_empty();
        match request.token.as_ref() {
            Some(token) => token.verify(self.token_authority, &request)?,
            None if sparse_edge => return Err(CoreError::InvalidValidationReceipt.into()),
            None => {}
        }
        if !self.policy_is_fresh(request.policy) {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        let next_submitted = self
            .submitted
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let next_coalesced = self
            .coalesced
            .checked_add(u64::from(self.pending.is_some()))
            .ok_or(CoreError::LengthOverflow)?;
        let aggregate = if let Some(pending) = self.pending.as_ref() {
            let (
                RequestPolicy::LatestFollowing {
                    stream: pending_stream,
                    ordinal: pending_ordinal,
                },
                RequestPolicy::LatestFollowing { stream, ordinal },
            ) = (pending.policy, request.policy)
            else {
                return Err(ProjectionServiceError::ExactRequestPending.into());
            };
            if stream != pending_stream || ordinal <= pending_ordinal {
                return Err(ProjectionServiceError::InvalidPolicyReplacement.into());
            }
            if pending.target.namespace != request.parent.namespace
                || pending.target.file != request.parent.file
                || pending.target.length != request.parent.length
                || pending.target.references != request.parent.references
                || pending.target_digest != request.parent_digest
            {
                return Err(ProjectionServiceError::ParentChainMismatch.into());
            }
            Some(match (&pending.plan, &request.plan) {
                (ProjectionPlan::Ranges(left), ProjectionPlan::Ranges(right)) => {
                    projection_plan(left.iter().chain(right).cloned(), request.target.length)?
                }
                _ => ProjectionPlan::FullFallback,
            })
        } else {
            None
        };
        if origin.is_some() && self.pending.is_some() {
            return Err(CoreError::PublicationConflict.into());
        }
        if let Some(token) = request.token.as_mut() {
            token.consume()?;
            request.edge_authenticated = true;
        }
        request.token = None;
        self.record_policy(request.policy);
        if let Some(mut pending) = self.pending.take() {
            pending.target = request.target;
            pending.target_digest = request.target_digest;
            pending.policy = request.policy;
            pending.force_full_fallback |= request.force_full_fallback;
            if pending.force_full_fallback {
                pending.plan = ProjectionPlan::FullFallback;
            } else {
                match aggregate.ok_or(CoreError::InvalidValidationReceipt)? {
                    ProjectionPlan::Ranges(ranges) => pending.plan = ProjectionPlan::Ranges(ranges),
                    ProjectionPlan::FullFallback => {
                        pending.plan = ProjectionPlan::FullFallback;
                        pending.force_full_fallback = true;
                    }
                }
            }
            pending.contended |= request.contended;
            pending.edge_authenticated &= request.edge_authenticated;
            self.pending = Some(pending);
        } else {
            self.pending = Some(request);
        }
        self.submitted = next_submitted;
        self.coalesced = next_coalesced;
        let enqueue_t2_ns = origin.map(|origin| origin.elapsed().as_nanos());
        if let Some(enqueue_t2_ns) = enqueue_t2_ns {
            self.pending
                .as_mut()
                .and_then(|request| request.end_to_end.as_mut())
                .expect("accepted end-to-end request retains timing")
                .enqueue_t2_ns = Some(enqueue_t2_ns);
        }
        Ok(enqueue_t2_ns)
    }

    fn take(&mut self) -> AnyResult<Option<ProjectionRequest>> {
        let Some(request) = self.pending.take() else {
            return Ok(None);
        };
        self.in_flight = true;
        self.started = self
            .started
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        Ok(Some(request))
    }

    fn complete(&mut self) -> AnyResult<()> {
        self.in_flight = false;
        self.published = self
            .published
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        Ok(())
    }

    fn record_failure(&mut self, error: &(dyn std::error::Error + 'static)) -> AnyResult<()> {
        self.in_flight = false;
        let (busy, locked) = projection_sqlite_error_counts(error);
        self.sqlite_busy_errors = self
            .sqlite_busy_errors
            .checked_add(busy)
            .ok_or(CoreError::LengthOverflow)?;
        self.sqlite_locked_errors = self
            .sqlite_locked_errors
            .checked_add(locked)
            .ok_or(CoreError::LengthOverflow)?;
        match error.downcast_ref::<ProjectionServiceError>() {
            Some(ProjectionServiceError::Cancelled) => {
                self.cancelled = self
                    .cancelled
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
            }
            Some(ProjectionServiceError::ParentChainMismatch) => {
                self.stale = self.stale.checked_add(1).ok_or(CoreError::LengthOverflow)?;
            }
            _ => {
                self.failed = self
                    .failed
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
            }
        }
        self.shutdown = true;
        Ok(())
    }

    fn equations_hold(&self) -> bool {
        self.submitted == self.coalesced + self.started
            && self.started == self.published + self.cancelled + self.failed + self.stale
    }

    fn ensure_worker_live(&self) -> AnyResult<()> {
        if let Some(error) = &self.worker_error {
            return Err(format!("{}: {error}", ProjectionServiceError::WorkerFailed).into());
        }
        Ok(())
    }
}

struct ProjectionWorker {
    directory_path: PathBuf,
    directory: File,
    store: Option<Store>,
    active: VerifiedSeed,
    counters: ProjectionCounters,
    apply_native: Counters,
    apply_rename_acknowledged: bool,
    shutdown_rendezvous: Option<ProjectionShutdownRendezvous>,
}

struct ProjectionShutdownRendezvous {
    private_ready: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
    shutdown_state: std::sync::Arc<(std::sync::Mutex<ProjectionMailbox>, std::sync::Condvar)>,
}

struct ProjectionContentionRendezvous {
    ready: std::sync::mpsc::SyncSender<()>,
    writer_started: std::sync::mpsc::Receiver<()>,
    reader_done: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectionReconciliation {
    Target,
    Prior,
}

fn reconcile_projection_publication(
    store: &mut Store,
    directory: &File,
    active: &SeedIdentity,
    target: Roots,
    counters: &mut Counters,
    metrics: &mut Metrics,
) -> AnyResult<ProjectionReconciliation> {
    if compare_destination_to_root(
        store,
        directory,
        target.namespace,
        metrics,
        counters,
        false,
        false,
    )
    .map_err(|_| CoreError::AmbiguousDurability)?
    {
        counters.absorb_reconciliation(metrics)?;
        return Ok(ProjectionReconciliation::Target);
    }
    if compare_destination_to_root(
        store,
        directory,
        active.namespace_root,
        metrics,
        counters,
        false,
        false,
    )
    .map_err(|_| CoreError::AmbiguousDurability)?
    {
        counters.absorb_reconciliation(metrics)?;
        return Ok(ProjectionReconciliation::Prior);
    }
    Err(CoreError::AmbiguousDurability.into())
}

fn reconcile_projection_publication_fresh(
    store_path: &Path,
    directory: &File,
    active: &SeedIdentity,
    target: Roots,
    counters: &mut Counters,
    metrics: &mut Metrics,
) -> AnyResult<ProjectionReconciliation> {
    let mut store = Store::open_existing_read_only(store_path, SELECTED_PROFILE)
        .map_err(|_| CoreError::AmbiguousDurability)?;
    reconcile_projection_publication(&mut store, directory, active, target, counters, metrics)
}

fn projection_read_scope<T>(
    store: &mut Store,
    operation: impl FnOnce(&mut Store) -> AnyResult<T>,
) -> AnyResult<T> {
    let diagnostic = store.projection_contention.clone();
    if let Some(diagnostic) = diagnostic.as_ref() {
        diagnostic.reader_autocommit.store(
            u64::from(store.connection.is_autocommit()),
            Ordering::SeqCst,
        );
        diagnostic.reader_scope_live.store(1, Ordering::SeqCst);
    }
    let result = operation(store);
    if let Some(diagnostic) = diagnostic {
        diagnostic.reader_scope_live.store(0, Ordering::SeqCst);
        diagnostic.reader_autocommit.store(
            u64::from(store.connection.is_autocommit()),
            Ordering::SeqCst,
        );
    }
    result
}

fn open_projection_seed(
    directory: &File,
    target: Roots,
    digest: [u8; 32],
) -> AnyResult<VerifiedSeed> {
    let visible = stat_at(directory, DESTINATION_NAME)?.ok_or(CoreError::PublicationConflict)?;
    if !visible.is_regular()
        || visible.length != target.length
        || u32::from(visible.mode & 0o7777) != MODE
    {
        return Err(CoreError::PublicationConflict.into());
    }
    let file = openat_file(
        directory,
        DESTINATION_NAME,
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        0,
    )?;
    let (native, storage) = fstat_file(&file)?;
    if native != visible {
        return Err(CoreError::IdentityMismatch.into());
    }
    Ok(VerifiedSeed {
        file,
        identity: SeedIdentity {
            native,
            namespace_root: target.namespace,
            file_root: target.file,
            length: target.length,
            references: target.references,
            digest,
        },
        storage,
    })
}

fn verify_projection_temp_name(
    directory: &File,
    name: &str,
    owned: NativeIdentity,
) -> AnyResult<()> {
    let named = stat_at(directory, name)?.ok_or(CoreError::IdentityMismatch)?;
    if !named.is_regular()
        || named.device != owned.device
        || named.inode != owned.inode
        || named.length != owned.length
        || u32::from(named.mode & 0o7777) != u32::from(owned.mode & 0o7777)
    {
        return Err(CoreError::IdentityMismatch.into());
    }
    Ok(())
}

fn admit_projection_seed(
    directory: &File,
    active: &SeedIdentity,
    metrics: &mut Metrics,
) -> AnyResult<bool> {
    let Some(before) = stat_at(directory, DESTINATION_NAME)? else {
        return Ok(false);
    };
    if !before.is_regular() {
        return Err(CoreError::WrongLogicalRole.into());
    }
    if before != active.native {
        return Err(CoreError::IdentityMismatch.into());
    }
    let file = openat_file(
        directory,
        DESTINATION_NAME,
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        0,
    )?;
    let opened = fstat_file(&file)?.0;
    if opened != before {
        return Err(CoreError::IdentityMismatch.into());
    }
    let (length, digest, mode) = g4_hash_descriptor(&file, opened, metrics)?;
    let after = fstat_file(&file)?.0;
    let named_after = stat_at(directory, DESTINATION_NAME)?;
    if after != opened
        || named_after != Some(after)
        || length != active.length
        || digest != active.digest
        || mode != MODE
    {
        return Err(CoreError::IdentityMismatch.into());
    }
    Ok(true)
}

const PROJECTION_TEMP_OWNERSHIP_XATTR: &CStr = c"com.layerfs.projection-owner-v1";

fn projection_temp_ownership_tag(
    authority: ProjectionTokenAuthority,
    name: &str,
    native: NativeIdentity,
) -> AnyResult<[u8; 32]> {
    let mut hasher = blake3::Hasher::new_keyed(&authority.validation_key);
    hasher.update(b"layerfs/phase4/g5/projection-temp-owner/v1\0");
    hasher.update(&authority.store_instance_id);
    hasher.update(&authority.validation_authority_id);
    hasher.update(&authority.profile);
    hasher.update(&authority.integrity_epoch.to_be_bytes());
    hash_field(&mut hasher, name.as_bytes())?;
    hasher.update(&native.device.to_be_bytes());
    hasher.update(&native.inode.to_be_bytes());
    Ok(*hasher.finalize().as_bytes())
}

fn mark_projection_temp_owned(
    file: &File,
    authority: ProjectionTokenAuthority,
    name: &str,
) -> AnyResult<()> {
    let before = fstat_file(file)?.0;
    if !before.is_regular() {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let tag = projection_temp_ownership_tag(authority, name, before)?;
    // SAFETY: the descriptor, NUL-terminated attribute name, and tag bytes are live.
    let result = unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            PROJECTION_TEMP_OWNERSHIP_XATTR.as_ptr(),
            tag.as_ptr().cast(),
            tag.len(),
            0,
            0,
        )
    };
    if result == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    let after = fstat_file(file)?.0;
    if after.device != before.device
        || after.inode != before.inode
        || after.length != before.length
        || !after.is_regular()
    {
        return Err(CoreError::IdentityMismatch.into());
    }
    Ok(())
}

fn projection_temp_is_owned(
    file: &File,
    authority: ProjectionTokenAuthority,
    name: &str,
    native: NativeIdentity,
) -> AnyResult<bool> {
    let expected = projection_temp_ownership_tag(authority, name, native)?;
    let mut observed = [0_u8; 32];
    // SAFETY: the descriptor, NUL-terminated attribute name, and output bytes are live.
    let read = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            PROJECTION_TEMP_OWNERSHIP_XATTR.as_ptr(),
            observed.as_mut_ptr().cast(),
            observed.len(),
            0,
            0,
        )
    };
    if read == -1 {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(code) if code == libc::ENOATTR || code == libc::ERANGE => Ok(false),
            _ => Err(error.into()),
        };
    }
    Ok(
        usize::try_from(read).map_err(|_| CoreError::LengthOverflow)? == observed.len()
            && observed == expected
            && fstat_file(file)?.0 == native,
    )
}

fn unlink_authenticated_projection_temp(
    directory: &File,
    name: &str,
    file: &File,
    authority: ProjectionTokenAuthority,
    authenticated: NativeIdentity,
    before_named_recheck: impl FnOnce() -> AnyResult<()>,
) -> AnyResult<bool> {
    if fstat_file(file)?.0 != authenticated
        || !authenticated.is_regular()
        || !projection_temp_is_owned(file, authority, name, authenticated)?
    {
        return Ok(false);
    }
    before_named_recheck()?;
    let descriptor = fstat_file(file)?.0;
    let named = stat_at(directory, name)?;
    if descriptor != authenticated
        || named != Some(authenticated)
        || !descriptor.is_regular()
        || u32::from(descriptor.mode & 0o7777) != u32::from(authenticated.mode & 0o7777)
        || !projection_temp_is_owned(file, authority, name, authenticated)?
    {
        return Ok(false);
    }
    unlink_at(directory, name).map_err(Into::into)
}

fn cleanup_projection_restart_temps(
    directory_path: &Path,
    directory: &File,
    authority: ProjectionTokenAuthority,
) -> AnyResult<(u64, u64, u64)> {
    let mut discovered = 0_u64;
    let mut removed = 0_u64;
    let mut retained = 0_u64;
    for entry in fs::read_dir(directory_path)? {
        let entry = entry?;
        let name = entry.file_name();
        if !name.as_bytes().starts_with(b".g3-tmp-") {
            continue;
        }
        discovered = discovered.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        let name = name
            .as_os_str()
            .to_str()
            .ok_or(CoreError::InvalidIdentityText)?;
        let Some(named) = stat_at(directory, name)? else {
            retained = retained.checked_add(1).ok_or(CoreError::LengthOverflow)?;
            continue;
        };
        let removed_owned = if named.is_regular() {
            match openat_file(
                directory,
                name,
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                0,
            ) {
                Ok(file) => {
                    let opened = fstat_file(&file)?.0;
                    opened == named
                        && unlink_authenticated_projection_temp(
                            directory,
                            name,
                            &file,
                            authority,
                            opened,
                            || Ok(()),
                        )?
                }
                Err(_) => false,
            }
        } else {
            false
        };
        if removed_owned {
            removed = removed.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        } else {
            retained = retained.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        }
    }
    if removed != 0 {
        sync_fd(directory)?;
    }
    Ok((discovered, removed, retained))
}

impl ProjectionWorker {
    fn absorb_apply_native(&mut self) -> AnyResult<()> {
        self.counters.clone_calls = self
            .counters
            .clone_calls
            .checked_add(self.apply_native.clone_calls)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.clone_successes = self
            .counters
            .clone_successes
            .checked_add(self.apply_native.clone_successes)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.clone_failures = self
            .counters
            .clone_failures
            .checked_add(self.apply_native.clone_failures)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.temp_files_created = self
            .counters
            .temp_files_created
            .checked_add(self.apply_native.temp_files_created)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.temp_files_removed = self
            .counters
            .temp_files_removed
            .checked_add(self.apply_native.temp_files_removed)
            .ok_or(CoreError::LengthOverflow)?;
        Ok(())
    }

    fn finalize_apply_error(
        &mut self,
        error: &(dyn std::error::Error + 'static),
        apply_wall_ns: u128,
    ) {
        let started = Instant::now();
        self.counters.fault_finalizations += 1;
        self.counters.fault_outcome = Some(projection_fault_outcome(error));
        self.counters.fault_q_terminal = q_current();
        self.counters.q_terminal = self.counters.fault_q_terminal;
        self.counters.fault_apply_wall_ns = apply_wall_ns;
        let mut complete = true;
        match count_residue(&self.directory_path, ".g3-tmp-") {
            Ok(residue) => {
                self.counters.fault_temp_residue = residue;
                if residue == 0 && !self.apply_rename_acknowledged {
                    self.counters.fault_unwind_temp_removals = self
                        .apply_native
                        .temp_files_created
                        .saturating_sub(self.apply_native.temp_files_removed);
                }
            }
            Err(_) => complete = false,
        }
        match fstat_file(&self.active.file) {
            Ok((native, storage)) => {
                self.counters.fault_active_descriptors = 1;
                self.counters.fault_active_identity_matches = native == self.active.identity.native;
                self.counters.fault_storage_logical_bytes = storage.logical;
                self.counters.fault_storage_apparent_bytes = storage.apparent;
                self.counters.fault_storage_allocated_bytes = storage.allocated;
            }
            _ => complete = false,
        }
        self.counters.fault_successor_descriptors = 0;
        self.counters.fault_finalization_complete = complete;
        self.counters.fault_finalization_ns = started.elapsed().as_nanos();
    }

    fn apply(
        &mut self,
        request: ProjectionRequest,
        origin: Instant,
        contention: Option<(&ProjectionContentionRendezvous, Instant)>,
    ) -> AnyResult<()> {
        self.apply_native = Counters::default();
        self.apply_rename_acknowledged = false;
        let started = Instant::now();
        let result = self.apply_inner(request, origin, contention);
        let accounting = self.absorb_apply_native();
        match result {
            Ok(()) => accounting,
            Err(error) => {
                self.finalize_apply_error(error.as_ref(), started.elapsed().as_nanos());
                if accounting.is_err() {
                    self.counters.fault_finalization_complete = false;
                }
                Err(error)
            }
        }
    }

    fn initialize_reader(&mut self) -> AnyResult<()> {
        let started = Instant::now();
        let target = self.active.identity.file_root;
        let end = self.active.identity.length.min(1);
        let mut metrics = Metrics::default();
        {
            let store = self
                .store
                .as_mut()
                .ok_or(CoreError::ValidationAuthorityUnavailable)?;
            let _ = projection_read_scope(store, |store| {
                read_file_range_segments(store, target, 0..end, &mut metrics)
            })?;
            store.connection.flush_prepared_statement_cache();
            if !store.connection.is_autocommit() {
                return Err(CoreError::PublicationConflict.into());
            }
        }
        finish_q(&mut metrics)?;
        self.counters.reader_initialization_calls = 1;
        self.counters.reader_initialization_sql_queries = metrics.sql_query_calls;
        self.counters.reader_initialization_authenticated_objects = metrics.objects_authenticated;
        self.counters.reader_initialization_authenticated_bytes =
            metrics.canonical_bytes_authenticated;
        self.counters.reader_initialization_q_high_water = metrics.q_high_water;
        self.absorb_metrics(&metrics)?;
        self.counters.reader_initialization_ns = started.elapsed().as_nanos();
        Ok(())
    }

    fn absorb_metrics(&mut self, metrics: &Metrics) -> AnyResult<()> {
        self.counters.sqlite_write_calls = self
            .counters
            .sqlite_write_calls
            .checked_add(metrics.sql_execute_calls)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.sqlite_transactions = self
            .counters
            .sqlite_transactions
            .checked_add(metrics.transactions)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.sqlite_commits = self
            .counters
            .sqlite_commits
            .checked_add(metrics.commits)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.sql_queries = self
            .counters
            .sql_queries
            .checked_add(metrics.sql_query_calls)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.sql_rows = self
            .counters
            .sql_rows
            .checked_add(metrics.sql_rows_returned)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.blob_reads = self
            .counters
            .blob_reads
            .checked_add(metrics.row_blob_reads)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.blob_bytes = self
            .counters
            .blob_bytes
            .checked_add(metrics.borrowed_row_blob_bytes)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.authenticated_objects = self
            .counters
            .authenticated_objects
            .checked_add(metrics.objects_authenticated)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.authenticated_bytes = self
            .counters
            .authenticated_bytes
            .checked_add(metrics.canonical_bytes_authenticated)
            .ok_or(CoreError::LengthOverflow)?;
        self.counters.q_high_water = self.counters.q_high_water.max(metrics.q_high_water);
        Ok(())
    }

    fn apply_inner(
        &mut self,
        request: ProjectionRequest,
        origin: Instant,
        contention: Option<(&ProjectionContentionRendezvous, Instant)>,
    ) -> AnyResult<()> {
        let build_started = Instant::now();
        if let Some(timing) = request.end_to_end {
            let enqueue_t2_ns = timing.enqueue_t2_ns.ok_or(CoreError::PublicationConflict)?;
            self.counters.end_to_end_edit_t0_ns = timing.edit_t0_ns;
            self.counters.end_to_end_canonical_ack_t1_ns = timing.canonical_ack_t1_ns;
            self.counters.end_to_end_enqueue_t2_ns = enqueue_t2_ns;
            self.counters.end_to_end_worker_start_t3_ns = origin.elapsed().as_nanos();
            if !(timing.edit_t0_ns <= timing.canonical_ack_t1_ns
                && timing.canonical_ack_t1_ns <= enqueue_t2_ns
                && enqueue_t2_ns <= self.counters.end_to_end_worker_start_t3_ns)
            {
                return Err(CoreError::PublicationConflict.into());
            }
        }
        if request.parent.namespace != self.active.identity.namespace_root
            || request.parent.file != self.active.identity.file_root
            || request.parent.length != self.active.identity.length
            || request.parent.references != self.active.identity.references
            || request.parent_digest != self.active.identity.digest
        {
            return Err(ProjectionServiceError::ParentChainMismatch.into());
        }
        if request.fault == ProjectionFault::CancelBeforeNative {
            return Err(ProjectionServiceError::Cancelled.into());
        }
        if request.fault == ProjectionFault::MissingSeed {
            if !unlink_at(&self.directory, DESTINATION_NAME)? {
                return Err(CoreError::IdentityMismatch.into());
            }
            sync_fd(&self.directory)?;
            self.counters.directory_sync_calls = self
                .counters
                .directory_sync_calls
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
        }
        let mut admission_metrics = Metrics::default();
        let seed_present = match admit_projection_seed(
            &self.directory,
            &self.active.identity,
            &mut admission_metrics,
        ) {
            Ok(present) => present,
            Err(error) => {
                self.counters.seed_admission_rejections = self
                    .counters
                    .seed_admission_rejections
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
                return Err(error);
            }
        };
        finish_q(&mut admission_metrics)?;
        self.absorb_metrics(&admission_metrics)?;
        if !seed_present {
            self.counters.missing_seed_fallbacks = self
                .counters
                .missing_seed_fallbacks
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
        }
        let prior_identity = self.active.identity.clone();
        let store_path = self
            .store
            .as_ref()
            .ok_or(CoreError::ValidationAuthorityUnavailable)?
            .path
            .clone();
        let mut binding_metrics = Metrics::default();
        let bound = g4_roots(
            self.store
                .as_ref()
                .ok_or(CoreError::ValidationAuthorityUnavailable)?,
            request.target.namespace,
            &mut binding_metrics,
        )?;
        finish_q(&mut binding_metrics)?;
        self.absorb_metrics(&binding_metrics)?;
        if bound.file != request.target.file
            || bound.length != request.target.length
            || bound.references != request.target.references
        {
            return Err(CoreError::IdentityMismatch.into());
        }
        let plan = if request.force_full_fallback || !seed_present {
            ProjectionPlan::FullFallback
        } else {
            request.plan
        };
        if matches!(&plan, ProjectionPlan::Ranges(ranges) if !ranges.is_empty())
            && !request.edge_authenticated
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        let (policy, ordinal) = request.policy.evidence();
        let mut plan_name = match &plan {
            ProjectionPlan::Ranges(_) => "Ranges",
            ProjectionPlan::FullFallback => "FullFallback",
        };
        let mut range_count = match &plan {
            ProjectionPlan::Ranges(ranges) => ranges.len(),
            ProjectionPlan::FullFallback => 0,
        };
        let mut build_class =
            projection_build_class(&plan, request.parent.length, request.target.length);
        let payload_started = Instant::now();
        let (file, mut temp) = match plan {
            ProjectionPlan::Ranges(ranges) if request.parent.length == request.target.length => {
                let clone = if request.fault == ProjectionFault::CloneFailure {
                    self.apply_native.clone_calls = self
                        .apply_native
                        .clone_calls
                        .checked_add(1)
                        .ok_or(CoreError::LengthOverflow)?;
                    self.apply_native.clone_failures = self
                        .apply_native
                        .clone_failures
                        .checked_add(1)
                        .ok_or(CoreError::LengthOverflow)?;
                    None
                } else {
                    clone_temp(
                        &self.active,
                        &self.directory,
                        &mut self.apply_native,
                        false,
                        false,
                        false,
                    )?
                };
                let (candidate, cloned) = match clone {
                    Some(candidate) => (candidate, true),
                    None => {
                        self.counters.full_fallbacks = self
                            .counters
                            .full_fallbacks
                            .checked_add(1)
                            .ok_or(CoreError::LengthOverflow)?;
                        let candidate = create_temp(&self.directory, &mut self.apply_native)?;
                        let mut output = candidate.0.try_clone()?;
                        let mut fallback_metrics = Metrics::default();
                        let store = self
                            .store
                            .as_mut()
                            .ok_or(CoreError::ValidationAuthorityUnavailable)?;
                        let (length, references, digest) = projection_read_scope(store, |store| {
                            stream_root(
                                store,
                                request.target.namespace,
                                &mut output,
                                &mut fallback_metrics,
                            )
                        })?;
                        finish_q(&mut fallback_metrics)?;
                        if length != request.target.length
                            || references != request.target.references
                            || digest != request.target_digest
                        {
                            return Err(CoreError::IdentityMismatch.into());
                        }
                        self.absorb_metrics(&fallback_metrics)?;
                        plan_name = "FullFallback";
                        build_class = None;
                        range_count = 0;
                        (candidate, false)
                    }
                };
                let mut metrics = Metrics::default();
                for range in ranges.into_iter().filter(|_| cloned) {
                    let mut start = range.start;
                    while start < range.end {
                        let end = start
                            .checked_add(G5_PROJECTION_MAX_BUFFER)
                            .ok_or(CoreError::LengthOverflow)?
                            .min(range.end);
                        let store = self
                            .store
                            .as_mut()
                            .ok_or(CoreError::ValidationAuthorityUnavailable)?;
                        let segments = projection_read_scope(store, |store| {
                            read_file_range_segments(
                                store,
                                request.target.file,
                                start..end,
                                &mut metrics,
                            )
                        })?;
                        let mut offset = start;
                        for bytes in segments.values.iter() {
                            pwrite_all(
                                &candidate.0,
                                offset,
                                bytes,
                                &mut self.counters.write_calls,
                            )?;
                            let length = u64::try_from(bytes.len())
                                .map_err(|_| CoreError::LengthOverflow)?;
                            offset = offset
                                .checked_add(length)
                                .ok_or(CoreError::LengthOverflow)?;
                            self.counters.fetched_bytes = self
                                .counters
                                .fetched_bytes
                                .checked_add(length)
                                .ok_or(CoreError::LengthOverflow)?;
                            self.counters.written_bytes = self
                                .counters
                                .written_bytes
                                .checked_add(length)
                                .ok_or(CoreError::LengthOverflow)?;
                            self.counters.max_buffer_bytes =
                                self.counters.max_buffer_bytes.max(length);
                        }
                        self.counters.range_fetches += 1;
                        start = end;
                    }
                }
                finish_q(&mut metrics)?;
                self.absorb_metrics(&metrics)?;
                candidate
            }
            _ => {
                self.counters.full_fallbacks += 1;
                let candidate = create_temp(&self.directory, &mut self.apply_native)?;
                let mut output = candidate.0.try_clone()?;
                let mut metrics = Metrics::default();
                let store = self
                    .store
                    .as_mut()
                    .ok_or(CoreError::ValidationAuthorityUnavailable)?;
                let (length, references, digest) = projection_read_scope(store, |store| {
                    stream_root(store, request.target.namespace, &mut output, &mut metrics)
                })?;
                finish_q(&mut metrics)?;
                if length != request.target.length
                    || references != request.target.references
                    || digest != request.target_digest
                {
                    return Err(CoreError::IdentityMismatch.into());
                }
                self.absorb_metrics(&metrics)?;
                self.counters.max_buffer_bytes = self.counters.max_buffer_bytes.max(
                    u64::try_from(layerfs_core::cdc::MAXIMUM_CHUNK_BYTES)
                        .map_err(|_| CoreError::LengthOverflow)?,
                );
                candidate
            }
        };
        self.counters.payload_ns = self
            .counters
            .payload_ns
            .checked_add(payload_started.elapsed().as_nanos())
            .ok_or(CoreError::LengthOverflow)?;
        let temp_authority = ProjectionTokenAuthority::from_store(
            self.store
                .as_ref()
                .ok_or(CoreError::ValidationAuthorityUnavailable)?,
        );
        mark_projection_temp_owned(&file, temp_authority, &temp.name)?;
        if request.fault == ProjectionFault::ShutdownInflight {
            let rendezvous = self
                .shutdown_rendezvous
                .as_ref()
                .ok_or(CoreError::ValidationAuthorityUnavailable)?;
            rendezvous
                .private_ready
                .send(())
                .map_err(|_| ProjectionServiceError::WorkerFailed)?;
            rendezvous
                .release
                .recv()
                .map_err(|_| ProjectionServiceError::WorkerFailed)?;
            if !rendezvous
                .shutdown_state
                .0
                .lock()
                .map_err(|_| ProjectionServiceError::WorkerFailed)?
                .shutdown
            {
                return Err(CoreError::PublicationConflict.into());
            }
        }
        if matches!(
            request.fault,
            ProjectionFault::CancelPrivateSuccessor | ProjectionFault::BeforeSync
        ) {
            temp.remove(&mut self.apply_native)?;
            sync_fd(&self.directory)?;
            self.counters.directory_sync_calls = self
                .counters
                .directory_sync_calls
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            return Err(match request.fault {
                ProjectionFault::CancelPrivateSuccessor => {
                    self.counters.private_build_cancellations = self
                        .counters
                        .private_build_cancellations
                        .checked_add(1)
                        .ok_or(CoreError::LengthOverflow)?;
                    ProjectionServiceError::Cancelled.into()
                }
                ProjectionFault::BeforeSync => std::io::Error::from_raw_os_error(libc::EIO).into(),
                _ => unreachable!(),
            });
        }
        let contention_metrics = {
            let store = self
                .store
                .as_mut()
                .ok_or(CoreError::ValidationAuthorityUnavailable)?;
            store.connection.flush_prepared_statement_cache();
            if !store.connection.is_autocommit() {
                return Err(CoreError::PublicationConflict.into());
            }
            if let Some(diagnostic) = store.projection_contention.as_ref() {
                diagnostic.reader_scope_live.store(0, Ordering::SeqCst);
                diagnostic.reader_autocommit.store(1, Ordering::SeqCst);
            }
            if let Some((rendezvous, origin)) = contention {
                self.counters.contention_worker_start_ns = origin.elapsed().as_nanos();
                let mut metrics = Metrics::default();
                let end = request.target.length.min(1);
                let _ = projection_read_scope(store, |store| {
                    rendezvous
                        .ready
                        .send(())
                        .map_err(|_| ProjectionServiceError::WorkerFailed)?;
                    rendezvous
                        .writer_started
                        .recv()
                        .map_err(|_| ProjectionServiceError::WorkerFailed)?;
                    if let Some(diagnostic) = store.projection_contention.as_ref() {
                        diagnostic.barrier_scope_live.store(
                            diagnostic.reader_scope_live.load(Ordering::SeqCst),
                            Ordering::SeqCst,
                        );
                        diagnostic.barrier_autocommit.store(
                            diagnostic.reader_autocommit.load(Ordering::SeqCst),
                            Ordering::SeqCst,
                        );
                    }
                    read_file_range_segments(store, request.target.file, 0..end, &mut metrics)
                })?;
                finish_q(&mut metrics)?;
                store.connection.flush_prepared_statement_cache();
                if !store.connection.is_autocommit() {
                    return Err(CoreError::PublicationConflict.into());
                }
                rendezvous
                    .reader_done
                    .send(())
                    .map_err(|_| ProjectionServiceError::WorkerFailed)?;
                Some(metrics)
            } else {
                None
            }
        };
        if let Some(metrics) = contention_metrics {
            self.absorb_metrics(&metrics)?;
        }
        let suspended_reader = if contention.is_some() {
            let store = self
                .store
                .take()
                .ok_or(CoreError::ValidationAuthorityUnavailable)?;
            let path = store.path.clone();
            let diagnostic = store.projection_contention.clone();
            drop(store);
            Some((path, diagnostic))
        } else {
            None
        };
        let durability_started = Instant::now();
        sync_fd(&file)?;
        self.counters.data_sync_calls += 1;
        chmod_fd(&file, MODE)?;
        sync_fd(&file)?;
        self.counters.metadata_sync_calls += 1;
        let candidate_identity = fstat_file(&file)?.0;
        if !candidate_identity.is_regular()
            || candidate_identity.length != request.target.length
            || u32::from(candidate_identity.mode & 0o7777) != MODE
        {
            return Err(CoreError::PublicationConflict.into());
        }
        verify_projection_temp_name(&self.directory, &temp.name, candidate_identity)?;
        if request.fault == ProjectionFault::BeforeRename {
            temp.remove(&mut self.apply_native)?;
            sync_fd(&self.directory)?;
            self.counters.directory_sync_calls = self
                .counters
                .directory_sync_calls
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            return Err(std::io::Error::from_raw_os_error(libc::EIO).into());
        }
        let mut rename_acknowledged = false;
        let publication = (|| -> AnyResult<VerifiedSeed> {
            self.counters.rename_calls += 1;
            rename_at(&self.directory, &temp.name, DESTINATION_NAME)?;
            temp.active = false;
            rename_acknowledged = true;
            self.apply_rename_acknowledged = true;
            if request.fault == ProjectionFault::RenameLostAck {
                return Err(std::io::Error::from_raw_os_error(libc::EIO).into());
            }
            sync_fd(&self.directory)?;
            self.counters.directory_sync_calls += 1;
            if matches!(
                request.fault,
                ProjectionFault::DirectorySyncLostAck | ProjectionFault::ReconciliationSyncFailure
            ) {
                return Err(std::io::Error::from_raw_os_error(libc::EIO).into());
            }
            if request.fault == ProjectionFault::PostRenameStatFailure {
                return Err(std::io::Error::from_raw_os_error(libc::ESTALE).into());
            }
            if request.fault == ProjectionFault::ReopenFailure {
                let _ = stat_at(&self.directory, DESTINATION_NAME)?
                    .ok_or(CoreError::PublicationConflict)?;
                return Err(std::io::Error::from_raw_os_error(libc::EIO).into());
            }
            let seed =
                open_projection_seed(&self.directory, request.target, request.target_digest)?;
            if seed.identity.native.device != candidate_identity.device
                || seed.identity.native.inode != candidate_identity.inode
                || seed.identity.native.length != candidate_identity.length
                || u32::from(seed.identity.native.mode & 0o7777)
                    != u32::from(candidate_identity.mode & 0o7777)
            {
                return Err(CoreError::IdentityMismatch.into());
            }
            Ok(seed)
        })();
        let next_seed = match publication {
            Ok(seed) => seed,
            Err(first) => {
                self.counters.reconciliation_calls = self
                    .counters
                    .reconciliation_calls
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
                let mut reconciliation_metrics = Metrics::default();
                let reconciliation = reconcile_projection_publication_fresh(
                    &store_path,
                    &self.directory,
                    &prior_identity,
                    request.target,
                    &mut self.apply_native,
                    &mut reconciliation_metrics,
                );
                self.absorb_metrics(&reconciliation_metrics)?;
                match reconciliation? {
                    ProjectionReconciliation::Target => {
                        temp.active = false;
                        if request.fault == ProjectionFault::ReconciliationSyncFailure {
                            return Err(CoreError::AmbiguousDurability.into());
                        }
                        sync_fd(&self.directory).map_err(|_| CoreError::AmbiguousDurability)?;
                        self.counters.directory_sync_calls = self
                            .counters
                            .directory_sync_calls
                            .checked_add(1)
                            .ok_or(CoreError::LengthOverflow)?;
                        open_projection_seed(&self.directory, request.target, request.target_digest)
                            .map_err(|_| -> Box<dyn std::error::Error> {
                                CoreError::AmbiguousDurability.into()
                            })?
                    }
                    ProjectionReconciliation::Prior => {
                        if temp.active {
                            temp.remove(&mut self.apply_native)?;
                        }
                        return Err(first);
                    }
                }
            }
        };
        if !rename_acknowledged {
            temp.active = false;
        }
        self.counters.durability_ns = self
            .counters
            .durability_ns
            .checked_add(durability_started.elapsed().as_nanos())
            .ok_or(CoreError::LengthOverflow)?;
        let verification_started = Instant::now();
        self.active = next_seed;
        self.counters.seed_rotations += 1;
        if request.end_to_end.is_some() {
            self.counters.end_to_end_native_ack_t4_ns = origin.elapsed().as_nanos();
        }
        self.counters.verification_ns = self
            .counters
            .verification_ns
            .checked_add(verification_started.elapsed().as_nanos())
            .ok_or(CoreError::LengthOverflow)?;
        let elapsed = build_started.elapsed().as_nanos();
        match build_class {
            Some(true) => self.counters.sparse_build_ns.push(elapsed),
            Some(false) => self.counters.exact_build_ns.push(elapsed),
            None if contention.is_some() => {
                self.counters.contention_fallback_build_ns.push(elapsed)
            }
            None => self.counters.fallback_build_ns.push(elapsed),
        }
        self.counters.build_evidence.push(ProjectionBuildEvidence {
            plan: plan_name,
            parent_length: request.parent.length,
            target_length: request.target.length,
            range_count,
            wall_ns: elapsed,
            contention: contention.is_some(),
            policy,
            ordinal,
            fault: request.fault.name(),
        });
        if contention.is_some() {
            let post_install = (|| -> AnyResult<()> {
                contention
                    .ok_or(CoreError::ValidationAuthorityUnavailable)?
                    .0
                    .release
                    .recv()
                    .map_err(|_| ProjectionServiceError::WorkerFailed)?;
                let (path, diagnostic) = suspended_reader
                    .as_ref()
                    .ok_or(CoreError::ValidationAuthorityUnavailable)?;
                if request.fault == ProjectionFault::ReaderReopenFailure {
                    return Err(std::io::Error::from_raw_os_error(libc::EIO).into());
                }
                let mut reopened = Store::open_existing_read_only(path, SELECTED_PROFILE)?;
                if let Some(diagnostic) = diagnostic {
                    reopened.observe_projection_contention(std::sync::Arc::clone(diagnostic));
                }
                self.store = Some(reopened);
                Ok(())
            })();
            if let Err(first) = post_install {
                self.counters.reconciliation_calls = self
                    .counters
                    .reconciliation_calls
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
                let mut reconciliation_metrics = Metrics::default();
                let reconciliation = reconcile_projection_publication_fresh(
                    &store_path,
                    &self.directory,
                    &prior_identity,
                    request.target,
                    &mut self.apply_native,
                    &mut reconciliation_metrics,
                );
                self.absorb_metrics(&reconciliation_metrics)?;
                match reconciliation? {
                    ProjectionReconciliation::Target => {
                        sync_fd(&self.directory).map_err(|_| CoreError::AmbiguousDurability)?;
                        self.counters.directory_sync_calls = self
                            .counters
                            .directory_sync_calls
                            .checked_add(1)
                            .ok_or(CoreError::LengthOverflow)?;
                        let (path, diagnostic) = suspended_reader
                            .as_ref()
                            .ok_or(CoreError::ValidationAuthorityUnavailable)?;
                        let mut reopened = Store::open_existing_read_only(path, SELECTED_PROFILE)
                            .map_err(|_| CoreError::AmbiguousDurability)?;
                        if let Some(diagnostic) = diagnostic {
                            reopened
                                .observe_projection_contention(std::sync::Arc::clone(diagnostic));
                        }
                        self.store = Some(reopened);
                    }
                    ProjectionReconciliation::Prior => {
                        self.active = open_projection_seed(
                            &self.directory,
                            Roots {
                                namespace: prior_identity.namespace_root,
                                file: prior_identity.file_root,
                                length: prior_identity.length,
                                references: prior_identity.references,
                            },
                            prior_identity.digest,
                        )?;
                    }
                }
                return Err(first);
            }
            self.counters.contention_worker_end_ns = contention
                .map(|(_, origin)| origin.elapsed().as_nanos())
                .unwrap_or_default();
        }
        Ok(())
    }
}

fn projection_percentile(values: &[u128], percentile: usize) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[(sorted.len() * percentile)
        .div_ceil(100)
        .saturating_sub(1)
        .min(sorted.len() - 1)]
}

fn projection_ns_array(values: &[u128]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(u128::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn projection_build_evidence_array(values: &[ProjectionBuildEvidence]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!(
                "{{\"plan\":\"{}\",\"policy\":\"{}\",\"ordinal\":{},\"parent_length\":{},\"target_length\":{},\"range_count\":{},\"wall_ns\":{},\"contention\":{},\"fault\":\"{}\"}}",
                value.plan,
                value.policy,
                value.ordinal,
                value.parent_length,
                value.target_length,
                value.range_count,
                value.wall_ns,
                value.contention,
                value.fault,
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn g5_projection_fixture_size(mode: &str) -> AnyResult<u64> {
    Ok(match mode {
        "self-check" | "screen-count" | "screen" | "gate" => G5_PROJECTION_MECHANISM_BYTES,
        _ => return Err(format!("unknown G5 projection mode {mode}").into()),
    })
}

pub(super) fn prepare_g5_projection_fixture(root: &Path, mode: &str) -> AnyResult<String> {
    let size = g5_projection_fixture_size(mode)?;
    let mut prepared = prepare(root, size, Scenario::QualifiedOneByte)?;
    let (exact_count, latest_count) = match mode {
        "self-check" => (1, 0),
        "screen-count" => (1, 1),
        "screen" => (2, 2),
        "gate" => (64, 100),
        _ => unreachable!(),
    };
    let mut target_source = open_readonly_nofollow(&prepared.directory_path.join("target.source"))?;
    target_source.seek(SeekFrom::Start(prepared.patch.start))?;
    let mut patch_bytes = vec![0_u8; usize::try_from(prepared.patch.end - prepared.patch.start)?];
    target_source.read_exact(&mut patch_bytes)?;
    drop(target_source);
    let mut prior_source = prepared.directory_path.join("target.source");
    let mut prior_roots = prepared.target;
    let mut prior_digest = prepared.target_digest;
    let mut chain = Vec::with_capacity(exact_count + latest_count - 1);
    for index in 1..(exact_count + latest_count) {
        let same_source = prepared
            .directory_path
            .join(format!("same-chain-{index:03}.source"));
        fs::copy(&prior_source, &same_source)?;
        let offset = u64::try_from(index).map_err(|_| CoreError::LengthOverflow)? % size;
        let mut source = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&same_source)?;
        source.seek(SeekFrom::Start(offset))?;
        let mut byte = [0_u8; 1];
        source.read_exact(&mut byte)?;
        byte[0] ^= 0x5a;
        source.seek(SeekFrom::Start(offset))?;
        source.write_all(&byte)?;
        source.sync_all()?;
        drop(source);
        let (_, digest, _) = hash_file(&same_source)?;
        let (roots, head) = build_projection_target(&mut prepared.store, &same_source)?;
        let range = offset..offset + 1;
        let chain_index = index - 1;
        let policy = projection_chain_policy(chain_index, exact_count)?;
        let token = prove_and_mint_projection_edge(
            &mut prepared.store,
            &head,
            prior_roots,
            prior_digest,
            &prior_source,
            roots,
            digest,
            &same_source,
            &range,
            policy,
        )?;
        chain.push(ProjectionFixtureEdge {
            target: roots,
            digest,
            range,
            token: Some(token),
        });
        let retired_source = std::mem::replace(&mut prior_source, same_source);
        if retired_source
            .file_name()
            .is_some_and(|name| name.as_bytes().starts_with(b"same-chain-"))
        {
            fs::remove_file(retired_source)?;
        }
        prior_roots = roots;
        prior_digest = digest;
    }
    let count_source = prepared.directory_path.join("count.source");
    fs::copy(&prior_source, &count_source)?;
    if prior_source
        .file_name()
        .is_some_and(|name| name.as_bytes().starts_with(b"same-chain-"))
    {
        fs::remove_file(&prior_source)?;
    }
    let mut count_writer = OpenOptions::new().append(true).open(&count_source)?;
    count_writer.write_all(&[0x5a])?;
    count_writer.sync_all()?;
    drop(count_writer);
    let (_, count_digest, _) = hash_file(&count_source)?;
    let (count, _) = build_projection_target(&mut prepared.store, &count_source)?;
    let storm_a_source = prepared.directory_path.join("storm-a.source");
    fs::copy(&count_source, &storm_a_source)?;
    let mut writer = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&storm_a_source)?;
    writer.seek(SeekFrom::Start(1))?;
    writer.write_all(&[0x33])?;
    writer.sync_all()?;
    drop(writer);
    let (_, storm_a_digest, _) = hash_file(&storm_a_source)?;
    let (storm_a, storm_a_head) = build_projection_target(&mut prepared.store, &storm_a_source)?;
    let storm_a_token = prove_and_mint_projection_edge(
        &mut prepared.store,
        &storm_a_head,
        count,
        count_digest,
        &count_source,
        storm_a,
        storm_a_digest,
        &storm_a_source,
        &(1..2),
        RequestPolicy::LatestFollowing {
            stream: LatestStream::CountStorm,
            ordinal: 0,
        },
    )?;
    let storm_b_source = prepared.directory_path.join("storm-b.source");
    fs::copy(&storm_a_source, &storm_b_source)?;
    let mut writer = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&storm_b_source)?;
    writer.seek(SeekFrom::Start(2))?;
    writer.write_all(&[0x66])?;
    writer.sync_all()?;
    drop(writer);
    let (_, storm_b_digest, _) = hash_file(&storm_b_source)?;
    let (storm_b, storm_b_head) = build_projection_target(&mut prepared.store, &storm_b_source)?;
    let storm_b_token = prove_and_mint_projection_edge(
        &mut prepared.store,
        &storm_b_head,
        storm_a,
        storm_a_digest,
        &storm_a_source,
        storm_b,
        storm_b_digest,
        &storm_b_source,
        &(2..3),
        RequestPolicy::LatestFollowing {
            stream: LatestStream::CountStorm,
            ordinal: 1,
        },
    )?;
    let latest_source = prepared.directory_path.join("latest.source");
    fs::copy(&storm_b_source, &latest_source)?;
    let mut latest_writer = OpenOptions::new().append(true).open(&latest_source)?;
    latest_writer.write_all(&[0xa5])?;
    latest_writer.sync_all()?;
    drop(latest_writer);
    let (_, latest_digest, _) = hash_file(&latest_source)?;
    let (latest, _) = build_projection_target(&mut prepared.store, &latest_source)?;
    let (reset_parent, reset_head) = build_projection_target(
        &mut prepared.store,
        &prepared.directory_path.join("parent.source"),
    )?;
    if reset_parent != prepared.parent || reset_head.1 != prepared.parent.namespace {
        return Err(CoreError::PublicationConflict.into());
    }
    fs::remove_file(&count_source)?;
    fs::remove_file(&storm_a_source)?;
    fs::remove_file(&storm_b_source)?;
    fs::remove_file(prepared.directory_path.join("target.source"))?;
    let fixture = write_projection_fixture(
        root,
        &prepared,
        &patch_bytes,
        count,
        count_digest,
        storm_a,
        storm_a_digest,
        &storm_a_token,
        storm_b,
        storm_b_digest,
        &storm_b_token,
        latest,
        latest_digest,
        exact_count,
        &chain,
    )?;
    Ok(format!("{{\"status\":\"PASS\",\"schema\":\"phase4-g5-projection-fixture-v2\",\"mode\":\"{mode}\",\"fixture\":\"{}\",\"size_bytes\":{size},\"preparation_timing\":\"outside-campaign\"}}", fixture.display()))
}

pub(super) fn run_g5_projection_suite(root: &Path, mode: &str) -> AnyResult<String> {
    let mut fixture = parse_projection_fixture(root)?;
    if fixture.parent.length != G5_PROJECTION_MECHANISM_BYTES {
        return Err(ProjectionServiceError::FixtureMismatch.into());
    }
    let selected_fault = ProjectionFault::release_mode(mode).unwrap_or(ProjectionFault::None);
    let requested_mode = mode;
    let mode = if selected_fault == ProjectionFault::None {
        mode
    } else {
        "self-check"
    };
    if !matches!(mode, "self-check" | "screen-count" | "screen" | "gate") {
        return Err(format!("unknown G5 projection mode {mode}").into());
    }
    let started = Instant::now();
    let live_edge = build_live_projection_edge(
        &fixture.directory.join("store.sqlite"),
        &fixture.directory,
        fixture.parent,
        fixture.parent_digest,
        &fixture.patch,
        &fixture.patch_bytes,
        started,
    )?;
    if live_edge.roots != fixture.target || live_edge.digest != fixture.target_digest {
        return Err(CoreError::IdentityMismatch.into());
    }
    let initial_end_to_end = (
        live_edge.edit_t0_ns,
        live_edge.canonical_ack_t1_ns,
        live_edge.edit_wall_ns,
        live_edge.canonical_ack_wall_ns,
        live_edge.transactions,
        live_edge.commits,
        live_edge.sql_queries,
        live_edge.authenticated_objects,
        live_edge.authenticated_bytes,
        live_edge.q_high_water,
    );
    let directory = open_dir(&fixture.directory)?;
    let diagnostic = std::sync::Arc::new(ProjectionContentionDiagnostic::default());
    let mut store =
        Store::open_existing_read_only(&fixture.directory.join("store.sqlite"), SELECTED_PROFILE)?;
    store.observe_projection_contention(std::sync::Arc::clone(&diagnostic));
    let token_authority = ProjectionTokenAuthority::from_store(&store);
    let restart_temps =
        cleanup_projection_restart_temps(&fixture.directory, &directory, token_authority)?;
    let active_file = openat_file(
        &directory,
        DESTINATION_NAME,
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        0,
    )?;
    let (active_native, active_storage) = fstat_file(&active_file)?;
    let mut active_metrics = Metrics::default();
    let active_roots = g4_roots(&store, fixture.parent.namespace, &mut active_metrics)?;
    let (active_length, active_digest, active_mode) =
        g4_hash_descriptor(&active_file, active_native, &mut active_metrics)?;
    finish_q(&mut active_metrics)?;
    if active_roots != fixture.parent
        || active_length != fixture.parent.length
        || active_digest != fixture.parent_digest
        || active_mode != MODE
    {
        return Err(CoreError::IdentityMismatch.into());
    }
    let active = VerifiedSeed {
        file: active_file,
        identity: SeedIdentity {
            native: active_native,
            namespace_root: fixture.parent.namespace,
            file_root: fixture.parent.file,
            length: fixture.parent.length,
            references: fixture.parent.references,
            digest: fixture.parent_digest,
        },
        storage: active_storage,
    };
    let shared = std::sync::Arc::new((
        std::sync::Mutex::new(ProjectionMailbox {
            in_flight: false,
            pending: None,
            shutdown: false,
            release_first: false,
            submitted: 0,
            coalesced: 0,
            started: 0,
            published: 0,
            cancelled: 0,
            failed: 0,
            stale: 0,
            sqlite_busy_errors: 0,
            sqlite_locked_errors: 0,
            worker_error: None,
            token_authority,
            exact_ordinal: None,
            same_size_ordinal: None,
            count_storm_ordinal: None,
            isolated_sparse_accepted: false,
        }),
        std::sync::Condvar::new(),
    ));
    let (worker_ready_tx, foreground_ready_rx) = std::sync::mpsc::sync_channel(0);
    let (foreground_started_tx, worker_started_rx) = std::sync::mpsc::sync_channel(0);
    let (worker_reader_done_tx, foreground_reader_done_rx) = std::sync::mpsc::sync_channel(0);
    let (foreground_release_tx, worker_release_rx) = std::sync::mpsc::sync_channel(0);
    let worker_contention = ProjectionContentionRendezvous {
        ready: worker_ready_tx,
        writer_started: worker_started_rx,
        reader_done: worker_reader_done_tx,
        release: worker_release_rx,
    };
    let origin = started;
    let worker_shared = std::sync::Arc::clone(&shared);
    let worker_directory_path = fixture.directory.clone();
    let handle = std::thread::spawn(move || -> Result<ProjectionCounters, String> {
        let mut terminal = ProjectionWorkerTerminal {
            shared: std::sync::Arc::clone(&worker_shared),
            success: false,
        };
        let mut worker = ProjectionWorker {
            directory_path: worker_directory_path,
            directory,
            store: Some(store),
            active,
            counters: ProjectionCounters {
                end_to_end_edit_t0_ns: initial_end_to_end.0,
                end_to_end_canonical_ack_t1_ns: initial_end_to_end.1,
                end_to_end_edit_wall_ns: initial_end_to_end.2,
                end_to_end_canonical_ack_wall_ns: initial_end_to_end.3,
                end_to_end_canonical_transactions: initial_end_to_end.4,
                end_to_end_canonical_commits: initial_end_to_end.5,
                end_to_end_canonical_sql_queries: initial_end_to_end.6,
                end_to_end_canonical_authenticated_objects: initial_end_to_end.7,
                end_to_end_canonical_authenticated_bytes: initial_end_to_end.8,
                end_to_end_canonical_q_high_water: initial_end_to_end.9,
                sql_queries: active_metrics.sql_query_calls,
                sql_rows: active_metrics.sql_rows_returned,
                sqlite_write_calls: active_metrics.sql_execute_calls,
                sqlite_transactions: active_metrics.transactions,
                sqlite_commits: active_metrics.commits,
                blob_reads: active_metrics.row_blob_reads,
                blob_bytes: active_metrics.borrowed_row_blob_bytes,
                authenticated_objects: active_metrics.objects_authenticated,
                authenticated_bytes: active_metrics.canonical_bytes_authenticated,
                q_high_water: active_metrics.q_high_water,
                initial_descriptor_verification_bytes: active_length,
                initial_storage_logical_bytes: active_storage.logical,
                initial_storage_apparent_bytes: active_storage.apparent,
                initial_storage_allocated_bytes: active_storage.allocated,
                restart_temps_discovered: restart_temps.0,
                restart_temps_removed: restart_temps.1,
                restart_temps_retained: restart_temps.2,
                directory_sync_calls: u64::from(restart_temps.1 != 0),
                ..ProjectionCounters::default()
            },
            apply_native: Counters::default(),
            apply_rename_acknowledged: false,
            shutdown_rendezvous: None,
        };
        worker
            .initialize_reader()
            .map_err(|error| format!("ProjectionReaderInitialization: {error:?}"))?;
        let mut contention_pending = true;
        let (mutex, condition) = &*worker_shared;
        loop {
            let mut mailbox = mutex
                .lock()
                .map_err(|_| "ProjectionMailboxPoisoned".to_string())?;
            while mailbox.pending.is_none() && !mailbox.shutdown {
                mailbox = condition
                    .wait(mailbox)
                    .map_err(|_| "ProjectionMailboxPoisoned".to_string())?;
            }
            let Some(request) = mailbox.take().map_err(|error| error.to_string())? else {
                break;
            };
            worker.counters.max_in_flight = 1;
            worker.counters.max_pending = 1;
            condition.notify_all();
            while !mailbox.release_first {
                mailbox = condition
                    .wait(mailbox)
                    .map_err(|_| "ProjectionMailboxPoisoned".to_string())?;
            }
            drop(mailbox);
            let contended = request.contended;
            let contention =
                (contended && contention_pending).then_some((&worker_contention, origin));
            if let Err(error) = worker.apply(request, origin, contention) {
                let fault_finalization = worker.counters.fault_finalization_json();
                let mut mailbox = mutex
                    .lock()
                    .map_err(|_| "ProjectionMailboxPoisoned".to_string())?;
                mailbox
                    .record_failure(error.as_ref())
                    .map_err(|record_error| record_error.to_string())?;
                mailbox.worker_error = Some(format!("{error:?};{fault_finalization}"));
                condition.notify_all();
                return Err(format!(
                    "ProjectionWorkerApply: {error:?};{fault_finalization}"
                ));
            }
            if contended && contention_pending {
                contention_pending = false;
            }
            let mut mailbox = mutex
                .lock()
                .map_err(|_| "ProjectionMailboxPoisoned".to_string())?;
            mailbox.complete().map_err(|error| error.to_string())?;
            condition.notify_all();
        }
        worker.counters.q_terminal = q_current();
        if worker.counters.q_terminal != 0 {
            return Err(format!(
                "ProjectionTerminalQ:{}",
                worker.counters.q_terminal
            ));
        }
        terminal.success = true;
        Ok(worker.counters)
    });
    let (mutex, condition) = &*shared;
    let mut mailbox = mutex.lock().map_err(|_| "ProjectionMailboxPoisoned")?;
    mailbox.submit(ProjectionRequest {
        parent: fixture.parent,
        parent_digest: fixture.parent_digest,
        target: fixture.parent,
        target_digest: fixture.parent_digest,
        plan: projection_plan(std::iter::empty(), fixture.parent.length)?,
        contended: false,
        policy: RequestPolicy::ExactEveryRoot { ordinal: 0 },
        force_full_fallback: false,
        token: None,
        edge_authenticated: false,
        end_to_end: None,
        fault: ProjectionFault::None,
    })?;
    condition.notify_one();
    while !mailbox.in_flight && mailbox.worker_error.is_none() {
        mailbox = condition
            .wait(mailbox)
            .map_err(|_| "ProjectionMailboxPoisoned")?;
    }
    mailbox.ensure_worker_live()?;
    mailbox.release_first = true;
    condition.notify_all();
    while mailbox.published < 1 && mailbox.worker_error.is_none() {
        mailbox = condition
            .wait(mailbox)
            .map_err(|_| "ProjectionMailboxPoisoned")?;
    }
    mailbox.ensure_worker_live()?;
    let mut parent = fixture.parent;
    let mut published = 1_u64;
    mailbox
        .submit_with_origin(
            ProjectionRequest {
                parent,
                parent_digest: fixture.parent_digest,
                target: fixture.target,
                target_digest: fixture.target_digest,
                plan: projection_plan(
                    std::iter::once(fixture.patch.clone()),
                    fixture.target.length,
                )?,
                contended: false,
                policy: RequestPolicy::IsolatedSparseSentinel,
                force_full_fallback: false,
                token: Some(live_edge.token),
                edge_authenticated: false,
                end_to_end: Some(ProjectionEndToEndTiming {
                    edit_t0_ns: live_edge.edit_t0_ns,
                    canonical_ack_t1_ns: live_edge.canonical_ack_t1_ns,
                    enqueue_t2_ns: None,
                }),
                fault: selected_fault,
            },
            Some(started),
        )?
        .ok_or(CoreError::PublicationConflict)?;
    condition.notify_all();
    published += 1;
    while mailbox.published < published && mailbox.worker_error.is_none() {
        mailbox = condition
            .wait(mailbox)
            .map_err(|_| "ProjectionMailboxPoisoned")?;
    }
    mailbox.ensure_worker_live()?;
    parent = fixture.target;
    let mut parent_digest = fixture.target_digest;
    for (exact_index, edge) in fixture
        .chain
        .iter_mut()
        .take(fixture.exact_count.saturating_sub(1))
        .enumerate()
    {
        mailbox.submit(ProjectionRequest {
            parent,
            parent_digest,
            target: edge.target,
            target_digest: edge.digest,
            plan: projection_plan(std::iter::once(edge.range.clone()), edge.target.length)?,
            contended: false,
            policy: RequestPolicy::ExactEveryRoot {
                ordinal: u64::try_from(exact_index + 1).map_err(|_| CoreError::LengthOverflow)?,
            },
            force_full_fallback: false,
            token: edge.token.take(),
            edge_authenticated: false,
            end_to_end: None,
            fault: ProjectionFault::None,
        })?;
        condition.notify_all();
        published += 1;
        while mailbox.published < published && mailbox.worker_error.is_none() {
            mailbox = condition
                .wait(mailbox)
                .map_err(|_| "ProjectionMailboxPoisoned")?;
        }
        mailbox.ensure_worker_live()?;
        parent = edge.target;
        parent_digest = edge.digest;
    }
    mailbox.release_first = false;
    let latest = &mut fixture.chain[fixture.exact_count.saturating_sub(1)..];
    if let Some(edge) = latest.first_mut() {
        mailbox.submit(ProjectionRequest {
            parent,
            parent_digest,
            target: edge.target,
            target_digest: edge.digest,
            plan: projection_plan(std::iter::once(edge.range.clone()), edge.target.length)?,
            contended: false,
            policy: RequestPolicy::LatestFollowing {
                stream: LatestStream::SameSize,
                ordinal: 0,
            },
            force_full_fallback: false,
            token: edge.token.take(),
            edge_authenticated: false,
            end_to_end: None,
            fault: ProjectionFault::None,
        })?;
        condition.notify_all();
        while !mailbox.in_flight && mailbox.worker_error.is_none() {
            mailbox = condition
                .wait(mailbox)
                .map_err(|_| "ProjectionMailboxPoisoned")?;
        }
        mailbox.ensure_worker_live()?;
        parent = edge.target;
        parent_digest = edge.digest;
        for (index, edge) in latest[1..].iter_mut().enumerate() {
            mailbox.submit(ProjectionRequest {
                parent,
                parent_digest,
                target: edge.target,
                target_digest: edge.digest,
                plan: projection_plan(std::iter::once(edge.range.clone()), edge.target.length)?,
                contended: false,
                policy: RequestPolicy::LatestFollowing {
                    stream: LatestStream::SameSize,
                    ordinal: u64::try_from(index + 1).map_err(|_| CoreError::LengthOverflow)?,
                },
                force_full_fallback: false,
                token: edge.token.take(),
                edge_authenticated: false,
                end_to_end: None,
                fault: ProjectionFault::None,
            })?;
            parent = edge.target;
            parent_digest = edge.digest;
        }
        mailbox.release_first = true;
        condition.notify_all();
        published = published
            .checked_add(latest_following_builds_to_wait(latest.len()))
            .ok_or(CoreError::LengthOverflow)?;
        while mailbox.published < published && mailbox.worker_error.is_none() {
            mailbox = condition
                .wait(mailbox)
                .map_err(|_| "ProjectionMailboxPoisoned")?;
        }
        mailbox.ensure_worker_live()?;
    }
    let same_size_root = parent;
    drop(mailbox);
    let (same_length, same_digest, _) = hash_at(&open_dir(&fixture.directory)?, DESTINATION_NAME)?;
    let expected_same_digest = latest
        .last()
        .map(|value| value.digest)
        .unwrap_or(fixture.target_digest);
    if same_length != same_size_root.length || same_digest != expected_same_digest {
        return Err(format!(
            "ProjectionSameSizeCheckpoint: length={same_length}/{} digest={}/{}",
            same_size_root.length,
            hex_bytes(&same_digest),
            hex_bytes(&expected_same_digest),
        )
        .into());
    }
    let foreground_root = fixture.directory.clone();
    let foreground_diagnostic = std::sync::Arc::clone(&diagnostic);
    let foreground = std::thread::spawn(move || -> Result<(u64, u64, u128, u128), String> {
        foreground_ready_rx
            .recv()
            .map_err(|_| "ProjectionForegroundReadyDisconnected".to_string())?;
        let mut store = Store::open(&foreground_root.join("store.sqlite"), SELECTED_PROFILE)
            .map_err(|error| format!("ProjectionForegroundOpen: {error:?}"))?;
        store.observe_projection_contention(std::sync::Arc::clone(&foreground_diagnostic));
        let mut metrics = Metrics::default();
        let foreground_start_ns = origin.elapsed().as_nanos();
        let source = foreground_root.join("latest.source");
        let publication = (|| -> AnyResult<Roots> {
            let expected_references = source_cdc_sequence(&source)?.0;
            store.begin(&mut metrics)?;
            foreground_started_tx
                .send(())
                .map_err(|_| ProjectionServiceError::WorkerFailed)?;
            foreground_reader_done_rx
                .recv()
                .map_err(|_| ProjectionServiceError::WorkerFailed)?;
            build_and_publish_target_in_active_transaction(
                &mut store,
                &source,
                expected_references,
                &mut metrics,
            )
        })();
        if publication.is_err() && store.active_transaction.is_some() {
            let _ = store.rollback(&mut metrics);
        }
        let foreground_end_ns = origin.elapsed().as_nanos();
        foreground_release_tx
            .send(())
            .map_err(|_| "ProjectionForegroundReleaseDisconnected".to_string())?;
        if let Err(error) = publication {
            return Err(format!(
                "ProjectionForegroundPublish: {error:?}; transactions={}; commits={}; commit_returns={}; commit_return_errors={}; commit_primary={}; commit_extended={}; reader_autocommit={}; reader_scope_live={}",
                metrics.transactions,
                metrics.commits,
                metrics.commit_returns,
                metrics.commit_return_errors,
                foreground_diagnostic.commit_primary_code.load(Ordering::SeqCst),
                foreground_diagnostic.commit_extended_code.load(Ordering::SeqCst),
                foreground_diagnostic.commit_autocommit.load(Ordering::SeqCst),
                foreground_diagnostic.commit_scope_live.load(Ordering::SeqCst),
            ));
        }
        finish_q(&mut metrics).map_err(|error| error.to_string())?;
        Ok((
            metrics.transactions,
            metrics.commits,
            foreground_start_ns,
            foreground_end_ns,
        ))
    });
    let mut mailbox = mutex.lock().map_err(|_| "ProjectionMailboxPoisoned")?;
    mailbox.release_first = true;
    mailbox.submit(ProjectionRequest {
        parent,
        parent_digest,
        target: fixture.count,
        target_digest: fixture.count_digest,
        plan: ProjectionPlan::FullFallback,
        contended: false,
        policy: RequestPolicy::IsolatedOrdinaryFallback,
        force_full_fallback: true,
        token: None,
        edge_authenticated: false,
        end_to_end: None,
        fault: ProjectionFault::None,
    })?;
    condition.notify_all();
    published += 1;
    while mailbox.published < published && mailbox.worker_error.is_none() {
        mailbox = condition
            .wait(mailbox)
            .map_err(|_| "ProjectionMailboxPoisoned")?;
    }
    mailbox.ensure_worker_live()?;
    mailbox.release_first = false;
    mailbox.submit(ProjectionRequest {
        parent: fixture.count,
        parent_digest: fixture.count_digest,
        target: fixture.storm_a,
        target_digest: fixture.storm_a_digest,
        plan: projection_plan(std::iter::once(1..2), fixture.storm_a.length)?,
        contended: false,
        policy: RequestPolicy::LatestFollowing {
            stream: LatestStream::CountStorm,
            ordinal: 0,
        },
        force_full_fallback: false,
        token: fixture.storm_a_token.take(),
        edge_authenticated: false,
        end_to_end: None,
        fault: ProjectionFault::None,
    })?;
    condition.notify_all();
    while !mailbox.in_flight && mailbox.worker_error.is_none() {
        mailbox = condition
            .wait(mailbox)
            .map_err(|_| "ProjectionMailboxPoisoned")?;
    }
    mailbox.ensure_worker_live()?;
    mailbox.submit(ProjectionRequest {
        parent: fixture.storm_a,
        parent_digest: fixture.storm_a_digest,
        target: fixture.storm_b,
        target_digest: fixture.storm_b_digest,
        plan: projection_plan(std::iter::once(2..3), fixture.storm_b.length)?,
        contended: false,
        policy: RequestPolicy::LatestFollowing {
            stream: LatestStream::CountStorm,
            ordinal: 1,
        },
        force_full_fallback: true,
        token: fixture.storm_b_token.take(),
        edge_authenticated: false,
        end_to_end: None,
        fault: ProjectionFault::None,
    })?;
    mailbox.submit(ProjectionRequest {
        parent: fixture.storm_b,
        parent_digest: fixture.storm_b_digest,
        target: fixture.latest,
        target_digest: fixture.latest_digest,
        plan: ProjectionPlan::FullFallback,
        contended: true,
        policy: RequestPolicy::LatestFollowing {
            stream: LatestStream::CountStorm,
            ordinal: 2,
        },
        force_full_fallback: true,
        token: None,
        edge_authenticated: false,
        end_to_end: None,
        fault: ProjectionFault::None,
    })?;
    mailbox.release_first = true;
    mailbox.shutdown = true;
    condition.notify_all();
    drop(mailbox);
    let mut counters = handle
        .join()
        .map_err(|_| "ProjectionWorkerPanicked")?
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    let (foreground_transactions, foreground_commits, foreground_start_ns, foreground_end_ns) =
        foreground
            .join()
            .map_err(|_| "ProjectionForegroundWriterPanicked")?
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    let mailbox = mutex.lock().map_err(|_| "ProjectionMailboxPoisoned")?;
    if !mailbox.equations_hold() || mailbox.in_flight || mailbox.pending.is_some() {
        return Err(format!(
            "ProjectionConservation: submitted={} coalesced={} started={} published={} cancelled={} failed={} stale={} inflight={} pending={}",
            mailbox.submitted,
            mailbox.coalesced,
            mailbox.started,
            mailbox.published,
            mailbox.cancelled,
            mailbox.failed,
            mailbox.stale,
            mailbox.in_flight,
            mailbox.pending.is_some(),
        )
        .into());
    }
    counters.submitted = mailbox.submitted;
    counters.started = mailbox.started;
    counters.completed = mailbox.published;
    counters.superseded_pending = mailbox.coalesced;
    counters.cancelled = mailbox.cancelled;
    counters.failed = mailbox.failed;
    counters.stale = mailbox.stale;
    counters.sqlite_busy_errors = mailbox.sqlite_busy_errors;
    counters.sqlite_locked_errors = mailbox.sqlite_locked_errors;
    drop(mailbox);
    let checkpoint_started = Instant::now();
    let (checkpoint_length, checkpoint_digest, checkpoint_mode) =
        hash_at(&open_dir(&fixture.directory)?, DESTINATION_NAME)?;
    let terminal_file = openat_file(
        &open_dir(&fixture.directory)?,
        DESTINATION_NAME,
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        0,
    )?;
    let (_, terminal_storage) = fstat_file(&terminal_file)?;
    drop(terminal_file);
    let checkpoint_ns = checkpoint_started.elapsed().as_nanos();
    if checkpoint_length != fixture.latest.length
        || checkpoint_digest != fixture.latest_digest
        || checkpoint_mode != MODE
        || counters.max_in_flight > 1
        || counters.max_pending > 1
        || counters.cancelled != 0
        || counters.failed != 0
        || counters.stale != 0
        || counters.q_terminal != 0
        || counters.max_buffer_bytes > G5_PROJECTION_MAX_BUFFER
        || counters.sqlite_write_calls != 0
        || counters.sqlite_transactions != 0
        || counters.sqlite_commits != 0
        || counters.sqlite_busy_errors != 0
        || counters.sqlite_locked_errors != 0
        || foreground_transactions != 1
        || foreground_commits != 1
        || counters.contention_worker_start_ns >= counters.contention_worker_end_ns
        || foreground_start_ns >= foreground_end_ns
        || foreground_start_ns >= counters.contention_worker_end_ns
        || counters.contention_worker_start_ns >= foreground_end_ns
        || counters.end_to_end_canonical_transactions != 1
        || counters.end_to_end_canonical_commits != 1
        || !(counters.end_to_end_edit_t0_ns <= counters.end_to_end_canonical_ack_t1_ns
            && counters.end_to_end_canonical_ack_t1_ns <= counters.end_to_end_enqueue_t2_ns
            && counters.end_to_end_enqueue_t2_ns <= counters.end_to_end_worker_start_t3_ns
            && counters.end_to_end_worker_start_t3_ns <= counters.end_to_end_native_ack_t4_ns)
    {
        return Err(format!(
            "ProjectionTerminalGate: length={checkpoint_length}/{} digest={}/{} mode={checkpoint_mode:o}/{MODE:o} inflight={} pending={} q={} buffer={} writes={} foreground={foreground_transactions}/{foreground_commits} worker_interval={}/{} foreground_interval={}/{}",
            fixture.latest.length,
            hex_bytes(&checkpoint_digest),
            hex_bytes(&fixture.latest_digest),
            counters.max_in_flight,
            counters.max_pending,
            counters.q_terminal,
            counters.max_buffer_bytes,
            counters.sqlite_write_calls,
            counters.contention_worker_start_ns,
            counters.contention_worker_end_ns,
            foreground_start_ns,
            foreground_end_ns,
        )
        .into());
    }
    let exact_p50 = projection_percentile(&counters.exact_build_ns, 50);
    let exact_p95 = projection_percentile(&counters.exact_build_ns, 95);
    let sparse_p50 = projection_percentile(&counters.sparse_build_ns, 50);
    let sparse_p95 = projection_percentile(&counters.sparse_build_ns, 95);
    let fallback_p50 = projection_percentile(&counters.fallback_build_ns, 50);
    let fallback_p95 = projection_percentile(&counters.fallback_build_ns, 95);
    let contention_fallback_p50 = projection_percentile(&counters.contention_fallback_build_ns, 50);
    let contention_fallback_p95 = projection_percentile(&counters.contention_fallback_build_ns, 95);
    const G3_ACCEPTED_FALLBACK_BOUND_NS: u128 = 329_237_000;
    let fallback_within_g3_bound = counters
        .fallback_build_ns
        .iter()
        .all(|value| *value <= G3_ACCEPTED_FALLBACK_BOUND_NS);
    if mode != "self-check"
        && (exact_p50 > 5_000_000
            || exact_p95 > 8_000_000
            || sparse_p50 > 6_000_000
            || sparse_p95 > 10_000_000
            || counters
                .fallback_build_ns
                .iter()
                .any(|value| *value > G3_ACCEPTED_FALLBACK_BOUND_NS))
    {
        return Err("ProjectionLatencyGateFailed".into());
    }
    let latest_count = fixture
        .chain
        .len()
        .checked_sub(fixture.exact_count.saturating_sub(1))
        .ok_or(CoreError::LengthOverflow)?;
    fs::remove_file(fixture.directory.join("projection-live-first.source"))?;
    sync_fd(&open_dir(&fixture.directory)?)?;
    let terminal_residue = count_residue(&fixture.directory, ".g3-tmp-")?;
    let wall = started.elapsed().as_nanos();
    Ok(format!(
        concat!(
            "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-projection-suite-v2\",",
            "\"mode\":\"{}\",\"size_bytes\":{},\"route_class\":\"{}\",",
            "\"worker_count\":1,\"submitted\":{},\"coalesced\":{},",
            "\"started\":{},\"published\":{},\"cancelled\":{},\"failed\":{},\"stale\":{},",
            "\"max_in_flight\":{},\"max_pending\":{},\"full_fallbacks\":{},",
            "\"exact_build_ns\":{},\"exact_p50_ns\":{},\"exact_p95_ns\":{},",
            "\"sparse_build_ns\":{},\"sparse_p50_ns\":{},\"sparse_p95_ns\":{},",
            "\"full_fallback_build_ns\":{},\"full_fallback_p50_ns\":{},",
            "\"full_fallback_p95_ns\":{},\"full_fallback_g3_bound_ns\":{},",
            "\"full_fallback_within_g3_bound\":{},",
            "\"contention_full_fallback_build_ns\":{},",
            "\"contention_full_fallback_p50_ns\":{},",
            "\"contention_full_fallback_p95_ns\":{},",
            "\"contention_full_fallback_latency_claim\":\"NotClaimedDifferentConcurrentExecutionShape\",",
            "\"build_evidence\":{},",
            "\"reader_initialization_ns\":{},",
            "\"reader_initialization_classification\":\"OneTimeReadOnlyProcessInitializationInsideCompleteWallOutsideServiceSamples\",",
            "\"reader_initialization_calls\":{},\"reader_initialization_bytes_requested\":1,",
            "\"reader_initialization_sql_queries\":{},",
            "\"reader_initialization_authenticated_objects\":{},",
            "\"reader_initialization_authenticated_bytes\":{},",
            "\"reader_initialization_q_high_water\":{},",
            "\"reader_initialization_read_only\":true,\"reader_initialization_query_only\":true,",
            "\"reader_initialization_inside_complete_wall\":true,",
            "\"reader_initialization_excluded_from_service_samples\":true,",
            "\"end_to_end_edit_t0_ns\":{},\"end_to_end_canonical_ack_t1_ns\":{},",
            "\"end_to_end_enqueue_t2_ns\":{},\"end_to_end_worker_start_t3_ns\":{},",
            "\"end_to_end_native_ack_t4_ns\":{},\"end_to_end_edit_wall_ns\":{},",
            "\"end_to_end_canonical_ack_wall_ns\":{},",
            "\"end_to_end_canonical_transactions\":{},\"end_to_end_canonical_commits\":{},",
            "\"end_to_end_canonical_sql_queries\":{},",
            "\"end_to_end_canonical_authenticated_objects\":{},",
            "\"end_to_end_canonical_authenticated_bytes\":{},",
            "\"end_to_end_canonical_q_high_water\":{},",
            "\"end_to_end_population\":1,",
            "\"end_to_end_scope\":\"ObservedEditT0CanonicalAckT1EnqueueT2WorkerT3NativeAckT4\",",
            "\"recurring_service_sample_scope\":\"WorkerT3ToNativeAckT4NotEndToEndEditLatency\",",
            "\"initial_descriptor_binding\":\"ObservedAuthenticatedRootAndFullDescriptorDigest\",",
            "\"initial_descriptor_verification_bytes\":{},",
            "\"initial_storage_logical_bytes\":{},\"initial_storage_apparent_bytes\":{},",
            "\"initial_storage_allocated_bytes\":{},",
            "\"exact_every_root_population\":{},\"latest_following_population\":{},",
            "\"projected_root\":\"{}\",\"last_requested_root\":\"{}\",",
            "\"projected_equals_last_requested\":true,\"range_fetches\":{},",
            "\"fetched_bytes\":{},\"write_calls\":{},\"written_bytes\":{},",
            "\"clone_calls\":{},\"clone_successes\":{},\"clone_failures\":{},\"data_sync_calls\":{},",
            "\"temp_files_created\":{},\"temp_files_removed\":{},",
            "\"private_build_cancellations\":{},",
            "\"restart_temps_discovered\":{},\"restart_temps_removed\":{},\"restart_temps_retained\":{},",
            "\"missing_seed_fallbacks\":{},\"seed_admission_rejections\":{},",
            "\"metadata_sync_calls\":{},\"rename_calls\":{},\"directory_sync_calls\":{},",
            "\"reconciliation_calls\":{},\"sqlite_write_calls\":{},",
            "\"sqlite_transactions\":{},\"sqlite_commits\":{},",
            "\"projection_sqlite_counter_provenance\":\"ObservedMetricsAndSuccessfulNoErrorCompletion\",",
            "\"foreground_transactions\":{},\"foreground_commits\":{},",
            "\"contention_worker_start_ns\":{},\"contention_worker_end_ns\":{},",
            "\"contention_foreground_start_ns\":{},\"contention_foreground_end_ns\":{},",
            "\"contention_worker_and_foreground_transaction_intervals_overlap\":true,",
            "\"contention_overlap_scope\":\"ObservedBroadWorkerAndForegroundTransactionIntervals\",",
            "\"foreground_commit_within_end_to_end_t3_t4_claim\":\"NotClaimedDifferentRequest\",",
            "\"reader_barrier_autocommit\":{},\"reader_barrier_scope_live\":{},",
            "\"reader_commit_autocommit\":{},\"reader_commit_scope_live\":{},",
            "\"foreground_commit_primary_code\":{},\"foreground_commit_extended_code\":{},",
            "\"sqlite_busy_errors\":{},\"sqlite_locked_errors\":{},",
            "\"sql_queries\":{},\"sql_rows\":{},\"blob_reads\":{},\"blob_bytes\":{},",
            "\"authenticated_objects\":{},\"authenticated_bytes\":{},\"seed_rotations\":{},",
            "\"q_high_water\":{},\"q_terminal\":{},",
            "\"q_terminal_provenance\":\"ObservedWorkerThreadLocalQCurrent\",",
            "\"max_buffer_bytes\":{},",
            "\"fault_selector\":\"{}\",",
            "\"fault_receipt\":{{\"status\":\"{}\",\"complete_apply_hooks\":true}},",
            "\"supported_fault_selectors\":{},",
            "\"focused_complete_apply_fault_hooks\":{},",
            "\"timer_payload_ns\":{},\"timer_durability_ns\":{},",
            "\"timer_descriptor_verification_ns\":{},",
            "\"checkpoint_full_verification_ns\":{},\"checkpoint_outside_service_timer\":true,",
            "\"terminal_in_flight\":0,\"terminal_pending\":0,\"terminal_workers\":0,",
            "\"terminal_active_descriptors\":0,\"terminal_successor_descriptors\":0,",
            "\"terminal_descriptor_classification\":\"ProvenByWorkerJoinAndOwnedDescriptorDrop\",",
            "\"terminal_storage_logical_bytes\":{},\"terminal_storage_apparent_bytes\":{},",
            "\"terminal_storage_allocated_bytes\":{},",
            "\"terminal_temp_residue\":{},\"shutdown\":\"drained\",",
            "\"operation_wall_ns\":{}}}"
        ),
        requested_mode,
        G5_PROJECTION_MECHANISM_BYTES,
        G5_PROJECTION_ROUTE_CLASS,
        counters.submitted,
        counters.superseded_pending,
        counters.started,
        counters.completed,
        counters.cancelled,
        counters.failed,
        counters.stale,
        counters.max_in_flight,
        counters.max_pending,
        counters.full_fallbacks,
        projection_ns_array(&counters.exact_build_ns),
        exact_p50,
        exact_p95,
        projection_ns_array(&counters.sparse_build_ns),
        sparse_p50,
        sparse_p95,
        projection_ns_array(&counters.fallback_build_ns),
        fallback_p50,
        fallback_p95,
        G3_ACCEPTED_FALLBACK_BOUND_NS,
        fallback_within_g3_bound,
        projection_ns_array(&counters.contention_fallback_build_ns),
        contention_fallback_p50,
        contention_fallback_p95,
        projection_build_evidence_array(&counters.build_evidence),
        counters.reader_initialization_ns,
        counters.reader_initialization_calls,
        counters.reader_initialization_sql_queries,
        counters.reader_initialization_authenticated_objects,
        counters.reader_initialization_authenticated_bytes,
        counters.reader_initialization_q_high_water,
        counters.end_to_end_edit_t0_ns,
        counters.end_to_end_canonical_ack_t1_ns,
        counters.end_to_end_enqueue_t2_ns,
        counters.end_to_end_worker_start_t3_ns,
        counters.end_to_end_native_ack_t4_ns,
        counters.end_to_end_edit_wall_ns,
        counters.end_to_end_canonical_ack_wall_ns,
        counters.end_to_end_canonical_transactions,
        counters.end_to_end_canonical_commits,
        counters.end_to_end_canonical_sql_queries,
        counters.end_to_end_canonical_authenticated_objects,
        counters.end_to_end_canonical_authenticated_bytes,
        counters.end_to_end_canonical_q_high_water,
        counters.initial_descriptor_verification_bytes,
        counters.initial_storage_logical_bytes,
        counters.initial_storage_apparent_bytes,
        counters.initial_storage_allocated_bytes,
        fixture.exact_count,
        latest_count,
        fixture.latest.namespace,
        fixture.latest.namespace,
        counters.range_fetches,
        counters.fetched_bytes,
        counters.write_calls,
        counters.written_bytes,
        counters.clone_calls,
        counters.clone_successes,
        counters.clone_failures,
        counters.data_sync_calls,
        counters.temp_files_created,
        counters.temp_files_removed,
        counters.private_build_cancellations,
        counters.restart_temps_discovered,
        counters.restart_temps_removed,
        counters.restart_temps_retained,
        counters.missing_seed_fallbacks,
        counters.seed_admission_rejections,
        counters.metadata_sync_calls,
        counters.rename_calls,
        counters.directory_sync_calls,
        counters.reconciliation_calls,
        counters.sqlite_write_calls,
        counters.sqlite_transactions,
        counters.sqlite_commits,
        foreground_transactions,
        foreground_commits,
        counters.contention_worker_start_ns,
        counters.contention_worker_end_ns,
        foreground_start_ns,
        foreground_end_ns,
        diagnostic.barrier_autocommit.load(Ordering::SeqCst),
        diagnostic.barrier_scope_live.load(Ordering::SeqCst),
        diagnostic.commit_autocommit.load(Ordering::SeqCst),
        diagnostic.commit_scope_live.load(Ordering::SeqCst),
        diagnostic.commit_primary_code.load(Ordering::SeqCst),
        diagnostic.commit_extended_code.load(Ordering::SeqCst),
        counters.sqlite_busy_errors,
        counters.sqlite_locked_errors,
        counters.sql_queries,
        counters.sql_rows,
        counters.blob_reads,
        counters.blob_bytes,
        counters.authenticated_objects,
        counters.authenticated_bytes,
        counters.seed_rotations,
        counters.q_high_water,
        counters.q_terminal,
        counters.max_buffer_bytes,
        selected_fault.name(),
        if selected_fault == ProjectionFault::None {
            "NotInjectedInPerformanceRun"
        } else {
            "ObservedCompleteApply"
        },
        projection_fault_selectors_json(),
        projection_fault_hooks_json(),
        counters.payload_ns,
        counters.durability_ns,
        counters.verification_ns,
        checkpoint_ns,
        terminal_storage.logical,
        terminal_storage.apparent,
        terminal_storage.allocated,
        terminal_residue,
        wall,
    ))
}

pub(super) fn g5_projection_self_check(root: &Path) -> AnyResult<String> {
    prepare_g5_projection_fixture(root, "self-check")?;
    run_g5_projection_suite(root, "self-check")
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

    fn projection_json_scalar<'a>(report: &'a str, key: &str) -> &'a str {
        let marker = format!("\"{key}\":");
        let mut matches = report.match_indices(&marker);
        let (offset, _) = matches.next().expect("receipt field");
        assert!(matches.next().is_none());
        let value = &report[offset + marker.len()..];
        if let Some(value) = value.strip_prefix('"') {
            &value[..value.find('"').expect("quoted receipt field")]
        } else {
            let end = value
                .find(|character| character == ',' || character == '}')
                .expect("scalar receipt field terminator");
            &value[..end]
        }
    }

    fn projection_request(label: &[u8]) -> ProjectionRequest {
        let root = ObjectId::for_bytes(label);
        let roots = Roots {
            namespace: root,
            file: root,
            length: 16,
            references: 1,
        };
        ProjectionRequest {
            parent: roots,
            parent_digest: root.to_bytes(),
            target: roots,
            target_digest: root.to_bytes(),
            plan: ProjectionPlan::FullFallback,
            contended: false,
            policy: RequestPolicy::LatestFollowing {
                stream: LatestStream::SameSize,
                ordinal: 0,
            },
            force_full_fallback: false,
            token: None,
            edge_authenticated: false,
            end_to_end: None,
            fault: ProjectionFault::None,
        }
    }

    fn projection_test_authority() -> ProjectionTokenAuthority {
        ProjectionTokenAuthority {
            store_instance_id: [0; 16],
            validation_authority_id: [0; 32],
            validation_key: [0; 32],
            profile: [0; 32],
            integrity_epoch: 0,
        }
    }

    fn projection_apply_fixture(
        label: &str,
        fault: ProjectionFault,
    ) -> (PathBuf, PathBuf, ProjectionWorker, ProjectionRequest) {
        let root = test_root(label);
        let prepared =
            prepare(&root, SOURCE_1, Scenario::QualifiedOneByte).expect("projection fixture");
        let directory_path = prepared.directory_path.clone();
        let directory = prepared.directory.try_clone().expect("directory");
        let parent = prepared.parent;
        let parent_digest = prepared.parent_digest;
        let target = prepared.target;
        let target_digest = prepared.target_digest;
        let patch = prepared.patch.clone();
        let active_file = openat_file(
            &directory,
            DESTINATION_NAME,
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0,
        )
        .expect("active file");
        let (active_native, active_storage) = fstat_file(&active_file).expect("active stat");
        let mut active_metrics = Metrics::default();
        let active_roots =
            g4_roots(&prepared.store, parent.namespace, &mut active_metrics).expect("active roots");
        let (active_length, active_digest, active_mode) =
            g4_hash_descriptor(&active_file, active_native, &mut active_metrics)
                .expect("active descriptor");
        finish_q(&mut active_metrics).expect("active Q0");
        assert_eq!(active_roots, parent);
        assert_eq!(active_length, parent.length);
        assert_eq!(active_digest, parent_digest);
        assert_eq!(active_mode, MODE);
        let active = VerifiedSeed {
            file: active_file,
            identity: SeedIdentity {
                native: active_native,
                namespace_root: parent.namespace,
                file_root: parent.file,
                length: parent.length,
                references: parent.references,
                digest: parent_digest,
            },
            storage: active_storage,
        };
        drop(prepared.store);
        let store =
            Store::open_existing_read_only(&directory_path.join("store.sqlite"), SELECTED_PROFILE)
                .expect("reader");
        let mut worker = ProjectionWorker {
            directory_path: directory_path.clone(),
            directory,
            store: Some(store),
            active,
            counters: ProjectionCounters::default(),
            apply_native: Counters::default(),
            apply_rename_acknowledged: false,
            shutdown_rendezvous: None,
        };
        worker.initialize_reader().expect("initialize");
        let request = ProjectionRequest {
            parent,
            parent_digest,
            target,
            target_digest,
            plan: projection_plan(std::iter::once(patch), target.length).expect("plan"),
            contended: false,
            policy: RequestPolicy::IsolatedSparseSentinel,
            force_full_fallback: false,
            token: None,
            edge_authenticated: true,
            end_to_end: None,
            fault,
        };
        (root, directory_path, worker, request)
    }

    fn assert_projection_fault_finalized(
        worker: &ProjectionWorker,
        outcome: ProjectionFaultOutcome,
    ) {
        assert_eq!(worker.counters.fault_finalizations, 1);
        assert_eq!(worker.counters.fault_outcome, Some(outcome));
        assert_eq!(worker.counters.fault_q_terminal, 0);
        assert_eq!(worker.counters.q_terminal, 0);
        assert_eq!(worker.counters.fault_temp_residue, 0);
        assert_eq!(worker.counters.fault_active_descriptors, 1);
        assert_eq!(worker.counters.fault_successor_descriptors, 0);
        assert_eq!(
            worker.counters.fault_storage_logical_bytes,
            worker.active.storage.logical
        );
        assert_eq!(
            worker.counters.fault_storage_apparent_bytes,
            worker.active.storage.apparent
        );
        assert_eq!(
            worker.counters.fault_storage_allocated_bytes,
            worker.active.storage.allocated
        );
        assert!(worker.counters.fault_apply_wall_ns > 0);
        assert!(worker.counters.fault_finalization_ns > 0);
        assert!(worker.counters.fault_finalization_complete);
        assert!(worker
            .counters
            .fault_finalization_json()
            .contains("\"status\":\"PASS\""));
    }

    fn projection_token_mailbox(store: &Store) -> ProjectionMailbox {
        ProjectionMailbox {
            in_flight: false,
            pending: None,
            shutdown: false,
            release_first: true,
            submitted: 0,
            coalesced: 0,
            started: 0,
            published: 0,
            cancelled: 0,
            failed: 0,
            stale: 0,
            sqlite_busy_errors: 0,
            sqlite_locked_errors: 0,
            worker_error: None,
            token_authority: ProjectionTokenAuthority::from_store(store),
            exact_ordinal: None,
            same_size_ordinal: None,
            count_storm_ordinal: None,
            isolated_sparse_accepted: false,
        }
    }

    #[test]
    fn g5_projection_edge_token_rejects_mutation_replay_and_cross_stream_use() {
        let root = test_root("g5-projection-edge-token");
        let mut prepared = prepare(&root, SOURCE_1, Scenario::QualifiedOneByte).expect("fixture");
        let head = prepared
            .store
            .current_head()
            .expect("head read")
            .expect("head");
        let token = prove_and_mint_projection_edge(
            &mut prepared.store,
            &head,
            prepared.parent,
            prepared.parent_digest,
            &prepared.directory_path.join("parent.source"),
            prepared.target,
            prepared.target_digest,
            &prepared.directory_path.join("target.source"),
            &prepared.patch,
            RequestPolicy::IsolatedSparseSentinel,
        )
        .expect("mint");
        let serialized = token.serialize();
        assert!(ProjectionEdgeToken::parse(&format!("{serialized}|overflow")).is_err());
        let mut oversized = serialized.split('|').collect::<Vec<_>>();
        oversized[21] = "257";
        assert!(ProjectionEdgeToken::parse(&oversized.join("|")).is_err());
        let request = |token: ProjectionEdgeToken| ProjectionRequest {
            parent: prepared.parent,
            parent_digest: prepared.parent_digest,
            target: prepared.target,
            target_digest: prepared.target_digest,
            plan: projection_plan(
                std::iter::once(prepared.patch.clone()),
                prepared.target.length,
            )
            .expect("plan"),
            contended: false,
            policy: RequestPolicy::IsolatedSparseSentinel,
            force_full_fallback: false,
            token: Some(token),
            edge_authenticated: false,
            end_to_end: None,
            fault: ProjectionFault::None,
        };

        let mut valid = projection_token_mailbox(&prepared.store);
        valid
            .submit(request(
                ProjectionEdgeToken::parse(&serialized).expect("parse"),
            ))
            .expect("valid token");
        let accepted = valid.take().expect("take").expect("request");
        assert!(accepted.edge_authenticated && accepted.token.is_none());
        valid.complete().expect("complete");
        assert!(valid
            .submit(request(
                ProjectionEdgeToken::parse(&serialized).expect("reparse"),
            ))
            .is_err());

        let mut wrong_root = request(ProjectionEdgeToken::parse(&serialized).expect("token"));
        wrong_root.target.namespace = ObjectId::for_bytes(b"wrong-root");
        assert!(projection_token_mailbox(&prepared.store)
            .submit(wrong_root)
            .is_err());

        let mut wrong_digest = request(ProjectionEdgeToken::parse(&serialized).expect("token"));
        wrong_digest.target_digest[0] ^= 1;
        assert!(projection_token_mailbox(&prepared.store)
            .submit(wrong_digest)
            .is_err());

        let mut underdeclared = request(ProjectionEdgeToken::parse(&serialized).expect("token"));
        underdeclared.plan = ProjectionPlan::Ranges(Vec::new());
        assert!(projection_token_mailbox(&prepared.store)
            .submit(underdeclared)
            .is_err());

        let mut substituted_seed = request(ProjectionEdgeToken::parse(&serialized).expect("token"));
        substituted_seed.parent_digest[0] ^= 1;
        assert!(projection_token_mailbox(&prepared.store)
            .submit(substituted_seed)
            .is_err());

        let mut wrong_transition = request(ProjectionEdgeToken::parse(&serialized).expect("token"));
        wrong_transition
            .token
            .as_mut()
            .expect("token")
            .binding
            .transition = ObjectId::for_bytes(b"wrong-transition");
        assert!(projection_token_mailbox(&prepared.store)
            .submit(wrong_transition)
            .is_err());

        let mut cross_stream = request(ProjectionEdgeToken::parse(&serialized).expect("token"));
        cross_stream.policy = RequestPolicy::LatestFollowing {
            stream: LatestStream::CountStorm,
            ordinal: 0,
        };
        assert!(projection_token_mailbox(&prepared.store)
            .submit(cross_stream)
            .is_err());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_mailbox_conserves_state_and_drains() {
        assert_eq!(
            (0..2)
                .map(|index| {
                    projection_chain_policy(index, 2)
                        .expect("screen policy")
                        .evidence()
                })
                .collect::<Vec<_>>(),
            vec![("ExactEveryRoot", 1), ("LatestFollowingSameSize", 0)]
        );
        assert_eq!(
            (0..63)
                .map(|index| projection_chain_policy(index, 64).expect("gate policy"))
                .filter_map(|policy| match policy {
                    RequestPolicy::ExactEveryRoot { ordinal } => Some(ordinal),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            (1..64).collect::<Vec<_>>()
        );
        assert_eq!(latest_following_builds_to_wait(0), 0);
        assert_eq!(latest_following_builds_to_wait(1), 1);
        assert_eq!(latest_following_builds_to_wait(2), 2);
        assert_eq!(latest_following_builds_to_wait(100), 2);
        let mut mailbox = ProjectionMailbox {
            in_flight: false,
            pending: None,
            shutdown: false,
            release_first: false,
            submitted: 0,
            coalesced: 0,
            started: 0,
            published: 0,
            cancelled: 0,
            failed: 0,
            stale: 0,
            sqlite_busy_errors: 0,
            sqlite_locked_errors: 0,
            worker_error: None,
            token_authority: projection_test_authority(),
            exact_ordinal: None,
            same_size_ordinal: None,
            count_storm_ordinal: None,
            isolated_sparse_accepted: false,
        };
        let first = projection_request(b"first");
        let mut latest = projection_request(b"latest");
        latest.parent = first.target;
        latest.parent_digest = first.target_digest;
        latest.policy = RequestPolicy::LatestFollowing {
            stream: LatestStream::SameSize,
            ordinal: 1,
        };
        latest.contended = true;
        mailbox.submit(first).expect("first");
        mailbox.submit(latest).expect("replace pending");
        let request = mailbox.take().expect("take").expect("pending");
        assert_eq!(request.target.namespace, ObjectId::for_bytes(b"latest"));
        assert!(request.contended);
        mailbox.complete().expect("complete");
        mailbox.shutdown = true;
        assert!(mailbox.equations_hold());
        assert!(!mailbox.in_flight && mailbox.pending.is_none());
        assert_eq!(
            mailbox
                .submit(projection_request(b"late"))
                .expect_err("shutdown")
                .downcast_ref::<ProjectionServiceError>(),
            Some(&ProjectionServiceError::Shutdown)
        );
    }

    #[test]
    fn g5_projection_policy_blocks_exact_pending_and_bounds_latest_replacement() {
        let mut mailbox = ProjectionMailbox {
            in_flight: false,
            pending: None,
            shutdown: false,
            release_first: true,
            submitted: 0,
            coalesced: 0,
            started: 0,
            published: 0,
            cancelled: 0,
            failed: 0,
            stale: 0,
            sqlite_busy_errors: 0,
            sqlite_locked_errors: 0,
            worker_error: None,
            token_authority: projection_test_authority(),
            exact_ordinal: None,
            same_size_ordinal: None,
            count_storm_ordinal: None,
            isolated_sparse_accepted: false,
        };
        let mut exact = projection_request(b"exact");
        exact.policy = RequestPolicy::ExactEveryRoot { ordinal: 3 };
        mailbox.submit(exact).expect("exact pending");
        let mut next = projection_request(b"next");
        next.policy = RequestPolicy::LatestFollowing {
            stream: LatestStream::SameSize,
            ordinal: 4,
        };
        next.end_to_end = Some(ProjectionEndToEndTiming {
            edit_t0_ns: 1,
            canonical_ack_t1_ns: 2,
            enqueue_t2_ns: None,
        });
        assert_eq!(
            mailbox
                .submit_with_origin(next, Some(Instant::now()))
                .expect_err("exact cannot coalesce")
                .downcast_ref::<ProjectionServiceError>(),
            Some(&ProjectionServiceError::ExactRequestPending)
        );
        assert_eq!(mailbox.submitted, 1);
        assert!(matches!(
            mailbox.pending.as_ref().expect("exact retained").policy,
            RequestPolicy::ExactEveryRoot { ordinal: 3 }
        ));
        assert!(mailbox
            .pending
            .as_ref()
            .expect("rejected submit did not replace pending")
            .end_to_end
            .is_none());

        mailbox.pending = None;
        mailbox.submitted = 0;
        let mut first = projection_request(b"latest-a");
        first.policy = RequestPolicy::LatestFollowing {
            stream: LatestStream::SameSize,
            ordinal: 0,
        };
        let mut replacement = projection_request(b"latest-b");
        replacement.parent = first.target;
        replacement.parent_digest = first.target_digest;
        replacement.policy = RequestPolicy::LatestFollowing {
            stream: LatestStream::SameSize,
            ordinal: 1,
        };
        replacement.target.length = 1024;
        replacement.plan = projection_plan(
            (0..=G5_PROJECTION_MAX_RANGES).map(|index| {
                let start = u64::try_from(index * 2).expect("offset");
                start..start + 1
            }),
            replacement.target.length,
        )
        .expect("bounded admission");
        mailbox.submit(first).expect("latest pending");
        mailbox.submit(replacement).expect("latest replacement");
        assert!(
            mailbox
                .pending
                .as_ref()
                .expect("aggregate")
                .force_full_fallback
        );
        assert!(matches!(
            mailbox.pending.as_ref().expect("aggregate").plan,
            ProjectionPlan::FullFallback
        ));
    }

    #[test]
    fn g5_projection_worker_error_is_observable_without_waiting() {
        let mailbox = ProjectionMailbox {
            in_flight: false,
            pending: None,
            shutdown: true,
            release_first: true,
            submitted: 1,
            coalesced: 0,
            started: 1,
            published: 0,
            cancelled: 0,
            failed: 1,
            stale: 0,
            sqlite_busy_errors: 0,
            sqlite_locked_errors: 0,
            worker_error: Some("injected".into()),
            token_authority: projection_test_authority(),
            exact_ordinal: None,
            same_size_ordinal: None,
            count_storm_ordinal: None,
            isolated_sparse_accepted: false,
        };
        assert!(mailbox
            .ensure_worker_live()
            .expect_err("worker failure")
            .to_string()
            .contains("ProjectionWorkerFailed"));
    }

    #[test]
    fn g5_projection_rendezvous_disconnects_before_either_phase() {
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
        drop(ready_rx);
        assert!(ready_tx.send(()).is_err());
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel::<()>(0);
        drop(release_tx);
        assert!(release_rx.recv().is_err());
    }

    #[test]
    fn g5_projection_clone_failure_streams_and_reconciliation_is_old_or_new() {
        let root = test_root("g5-projection-clone-fallback");
        let mut prepared = prepare(&root, SOURCE_1, Scenario::QualifiedOneByte).expect("fixture");
        assert_eq!(
            reconcile_projection_publication(
                &mut Store::open_existing_read_only(
                    &prepared.directory_path.join("store.sqlite"),
                    SELECTED_PROFILE,
                )
                .expect("fresh reconciliation reader"),
                &prepared.directory,
                &prepared.seed.as_ref().expect("seed").identity,
                prepared.target,
                &mut Counters::default(),
                &mut Metrics::default(),
            )
            .expect("prior"),
            ProjectionReconciliation::Prior
        );
        assert_eq!(
            reconcile_projection_publication_fresh(
                &prepared.directory_path.join("missing-store.sqlite"),
                &prepared.directory,
                &prepared.seed.as_ref().expect("seed").identity,
                prepared.target,
                &mut Counters::default(),
                &mut Metrics::default(),
            )
            .expect_err("missing reconciliation store")
            .downcast_ref::<CoreError>(),
            Some(&CoreError::AmbiguousDurability)
        );
        let mut native = Counters::default();
        assert!(clone_temp(
            prepared.seed.as_ref().expect("seed"),
            &prepared.directory,
            &mut native,
            true,
            false,
            false
        )
        .expect("clone failure cleanup")
        .is_none());
        let (mut output, mut temp) =
            create_temp(&prepared.directory, &mut native).expect("fallback temp");
        let mut metrics = Metrics::default();
        let (_, _, digest) = stream_root(
            &mut prepared.store,
            prepared.target.namespace,
            &mut output,
            &mut metrics,
        )
        .expect("stream fallback");
        finish_q(&mut metrics).expect("Q0");
        assert_eq!(digest, prepared.target_digest);
        sync_fd(&output).expect("sync");
        chmod_fd(&output, MODE).expect("mode");
        sync_fd(&output).expect("metadata sync");
        rename_at(&prepared.directory, &temp.name, DESTINATION_NAME).expect("publish");
        temp.active = false;
        sync_fd(&prepared.directory).expect("directory sync");
        assert_eq!(
            reconcile_projection_publication(
                &mut prepared.store,
                &prepared.directory,
                &prepared.seed.as_ref().expect("seed").identity,
                prepared.target,
                &mut native,
                &mut Metrics::default(),
            )
            .expect("target"),
            ProjectionReconciliation::Target
        );
        let reopened =
            open_projection_seed(&prepared.directory, prepared.target, prepared.target_digest)
                .expect("post-rename reopen");
        assert_eq!(reopened.identity.namespace_root, prepared.target.namespace);
        assert_eq!(reopened.identity.digest, prepared.target_digest);
        assert_eq!(
            count_residue(&prepared.directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_stale_parent_fails_before_native_work() {
        let root = test_root("g5-projection-stale-parent");
        let mut prepared = prepare(&root, SOURCE_1, Scenario::QualifiedOneByte).expect("fixture");
        let active = prepared.seed.take().expect("seed");
        let active_root = active.identity.namespace_root;
        let directory = prepared.directory.try_clone().expect("directory");
        let store = Store::open_existing_read_only(
            &prepared.directory_path.join("store.sqlite"),
            SELECTED_PROFILE,
        )
        .expect("reader");
        let mut worker = ProjectionWorker {
            directory_path: prepared.directory_path.clone(),
            directory,
            store: Some(store),
            active,
            counters: ProjectionCounters::default(),
            apply_native: Counters::default(),
            apply_rename_acknowledged: false,
            shutdown_rendezvous: None,
        };
        worker.initialize_reader().expect("initialize");
        let stale = ObjectId::for_bytes(b"stale-parent");
        let error = worker
            .apply(
                ProjectionRequest {
                    parent: Roots {
                        namespace: stale,
                        file: stale,
                        length: prepared.parent.length,
                        references: prepared.parent.references,
                    },
                    parent_digest: stale.to_bytes(),
                    target: prepared.target,
                    target_digest: prepared.target_digest,
                    plan: ProjectionPlan::FullFallback,
                    contended: false,
                    policy: RequestPolicy::IsolatedOrdinaryFallback,
                    force_full_fallback: true,
                    token: None,
                    edge_authenticated: false,
                    end_to_end: None,
                    fault: ProjectionFault::None,
                },
                Instant::now(),
                None,
            )
            .expect_err("stale parent");
        assert_eq!(
            error.downcast_ref::<ProjectionServiceError>(),
            Some(&ProjectionServiceError::ParentChainMismatch)
        );
        assert_eq!(worker.active.identity.namespace_root, active_root);
        assert_eq!(worker.counters.rename_calls, 0);
        assert_projection_fault_finalized(&worker, ProjectionFaultOutcome::Stale);
        let mut stale_mailbox = ProjectionMailbox {
            in_flight: true,
            pending: None,
            shutdown: false,
            release_first: true,
            submitted: 1,
            coalesced: 0,
            started: 1,
            published: 0,
            cancelled: 0,
            failed: 0,
            stale: 0,
            sqlite_busy_errors: 0,
            sqlite_locked_errors: 0,
            worker_error: None,
            token_authority: projection_test_authority(),
            exact_ordinal: None,
            same_size_ordinal: None,
            count_storm_ordinal: None,
            isolated_sparse_accepted: false,
        };
        stale_mailbox
            .record_failure(error.as_ref())
            .expect("record stale completion");
        assert_eq!(stale_mailbox.stale, 1);
        assert_eq!(stale_mailbox.failed, 0);
        assert!(stale_mailbox.equations_hold());
        worker.active.identity.digest[0] ^= 1;
        let substituted_seed = ProjectionRequest {
            parent: prepared.parent,
            parent_digest: prepared.parent_digest,
            target: prepared.target,
            target_digest: prepared.target_digest,
            plan: projection_plan(
                std::iter::once(prepared.patch.clone()),
                prepared.target.length,
            )
            .expect("plan"),
            contended: false,
            policy: RequestPolicy::IsolatedSparseSentinel,
            force_full_fallback: false,
            token: None,
            edge_authenticated: true,
            end_to_end: None,
            fault: ProjectionFault::None,
        };
        assert_eq!(
            worker
                .apply(substituted_seed, Instant::now(), None)
                .expect_err("substituted seed")
                .downcast_ref::<ProjectionServiceError>(),
            Some(&ProjectionServiceError::ParentChainMismatch)
        );
        assert_eq!(
            count_residue(&prepared.directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_release_faults_reconcile_through_complete_apply() {
        for fault in [
            ProjectionFault::CloneFailure,
            ProjectionFault::RenameLostAck,
            ProjectionFault::DirectorySyncLostAck,
            ProjectionFault::PostRenameStatFailure,
            ProjectionFault::ReopenFailure,
        ] {
            let (root, directory_path, mut worker, request) =
                projection_apply_fixture(fault.name(), fault);
            let target = request.target;
            worker
                .apply(request, Instant::now(), None)
                .unwrap_or_else(|error| panic!("{}: {error:?}", fault.name()));
            assert_eq!(worker.active.identity.namespace_root, target.namespace);
            assert_eq!(worker.active.identity.file_root, target.file);
            assert_eq!(
                count_residue(&directory_path, ".g3-tmp-").expect("residue"),
                0
            );
            if fault == ProjectionFault::CloneFailure {
                assert_eq!(worker.counters.clone_failures, 1);
                assert_eq!(worker.counters.full_fallbacks, 1);
                assert_eq!(worker.counters.reconciliation_calls, 0);
            } else {
                assert_eq!(worker.counters.reconciliation_calls, 1);
                assert_eq!(
                    worker.counters.directory_sync_calls,
                    if fault == ProjectionFault::RenameLostAck {
                        1
                    } else {
                        2
                    }
                );
            }
            drop(worker);
            fs::remove_dir_all(root).expect("cleanup");
        }

        let (root, directory_path, mut worker, request) = projection_apply_fixture(
            "reconciliation-sync-failure",
            ProjectionFault::ReconciliationSyncFailure,
        );
        let prior = worker.active.identity.namespace_root;
        assert_eq!(
            worker
                .apply(request, Instant::now(), None)
                .expect_err("reconciliation sync must fail closed")
                .downcast_ref::<CoreError>(),
            Some(&CoreError::AmbiguousDurability)
        );
        assert_eq!(worker.active.identity.namespace_root, prior);
        assert_eq!(worker.counters.reconciliation_calls, 1);
        assert_eq!(worker.counters.directory_sync_calls, 1);
        assert!(worker.apply_native.reconciliation_sql_queries > 0);
        assert!(worker.counters.sql_queries >= worker.apply_native.reconciliation_sql_queries);
        assert!(
            worker.counters.authenticated_bytes
                >= worker
                    .apply_native
                    .reconciliation_canonical_bytes_authenticated
        );
        assert_projection_fault_finalized(&worker, ProjectionFaultOutcome::AmbiguousDurability);
        assert_eq!(
            count_residue(&directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_cancel_private_successor_removes_temp_and_records_cancellation() {
        let (root, directory_path, mut worker, request) = projection_apply_fixture(
            "cancel-private-successor",
            ProjectionFault::CancelPrivateSuccessor,
        );
        let prior = worker.active.identity.namespace_root;
        let error = worker
            .apply(request, Instant::now(), None)
            .expect_err("private successor cancellation");
        assert_eq!(
            error.downcast_ref::<ProjectionServiceError>(),
            Some(&ProjectionServiceError::Cancelled)
        );
        assert_eq!(worker.active.identity.namespace_root, prior);
        assert_eq!(worker.counters.temp_files_created, 1);
        assert_eq!(worker.counters.temp_files_removed, 1);
        assert_eq!(worker.counters.private_build_cancellations, 1);
        assert_eq!(worker.counters.data_sync_calls, 0);
        assert_eq!(worker.counters.rename_calls, 0);
        assert_eq!(worker.counters.directory_sync_calls, 1);
        assert_projection_fault_finalized(&worker, ProjectionFaultOutcome::Cancelled);
        let mut mailbox = ProjectionMailbox {
            in_flight: true,
            pending: None,
            shutdown: false,
            release_first: true,
            submitted: 1,
            coalesced: 0,
            started: 1,
            published: 0,
            cancelled: 0,
            failed: 0,
            stale: 0,
            sqlite_busy_errors: 0,
            sqlite_locked_errors: 0,
            worker_error: None,
            token_authority: projection_test_authority(),
            exact_ordinal: None,
            same_size_ordinal: None,
            count_storm_ordinal: None,
            isolated_sparse_accepted: false,
        };
        mailbox
            .record_failure(error.as_ref())
            .expect("record cancellation");
        assert_eq!(mailbox.cancelled, 1);
        assert_eq!(mailbox.failed, 0);
        assert_eq!(mailbox.stale, 0);
        assert!(mailbox.shutdown && !mailbox.in_flight && mailbox.equations_hold());
        assert_eq!(
            count_residue(&directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        assert_eq!(q_current(), 0);
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_shutdown_inflight_drains_real_worker_loop() {
        let (root, directory_path, mut worker, mut request) =
            projection_apply_fixture("shutdown-inflight", ProjectionFault::ShutdownInflight);
        let prior = worker.active.identity.namespace_root;
        let (private_ready_tx, private_ready_rx) = std::sync::mpsc::sync_channel(0);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        request.plan = ProjectionPlan::FullFallback;
        request.force_full_fallback = true;
        request.policy = RequestPolicy::IsolatedOrdinaryFallback;
        request.token = None;
        request.edge_authenticated = false;
        let target = request.target;
        let shared = std::sync::Arc::new((
            std::sync::Mutex::new(ProjectionMailbox {
                in_flight: false,
                pending: None,
                shutdown: false,
                release_first: true,
                submitted: 0,
                coalesced: 0,
                started: 0,
                published: 0,
                cancelled: 0,
                failed: 0,
                stale: 0,
                sqlite_busy_errors: 0,
                sqlite_locked_errors: 0,
                worker_error: None,
                token_authority: projection_test_authority(),
                exact_ordinal: None,
                same_size_ordinal: None,
                count_storm_ordinal: None,
                isolated_sparse_accepted: false,
            }),
            std::sync::Condvar::new(),
        ));
        worker.shutdown_rendezvous = Some(ProjectionShutdownRendezvous {
            private_ready: private_ready_tx,
            release: release_rx,
            shutdown_state: std::sync::Arc::clone(&shared),
        });
        let worker_shared = std::sync::Arc::clone(&shared);
        let handle = std::thread::spawn(move || {
            let mut terminal = ProjectionWorkerTerminal {
                shared: std::sync::Arc::clone(&worker_shared),
                success: false,
            };
            loop {
                let (mutex, condition) = &*worker_shared;
                let mut mailbox = mutex.lock().expect("mailbox");
                while mailbox.pending.is_none() && !mailbox.shutdown {
                    mailbox = condition.wait(mailbox).expect("worker wait");
                }
                let Some(request) = mailbox.take().expect("worker take") else {
                    break;
                };
                condition.notify_all();
                drop(mailbox);
                worker
                    .apply(request, Instant::now(), None)
                    .expect("in-flight request drains to publication");
                let mut mailbox = mutex.lock().expect("completion mailbox");
                mailbox.complete().expect("complete drained request");
                condition.notify_all();
            }
            worker.counters.q_terminal = q_current();
            assert_eq!(worker.counters.q_terminal, 0);
            terminal.success = true;
            worker
        });
        {
            let (mutex, condition) = &*shared;
            let mut mailbox = mutex.lock().expect("submit mailbox");
            mailbox.submit(request).expect("submit shutdown request");
            condition.notify_all();
        }
        private_ready_rx.recv().expect("private build ready");
        {
            let (mutex, condition) = &*shared;
            let mut mailbox = mutex.lock().expect("controller mailbox");
            assert!(mailbox.in_flight && !mailbox.shutdown);
            mailbox.shutdown = true;
            condition.notify_all();
        }
        release_tx.send(()).expect("release shutdown worker");
        let worker = handle.join().expect("worker join");
        assert_eq!(worker.active.identity.namespace_root, target.namespace);
        assert_ne!(worker.active.identity.namespace_root, prior);
        assert_eq!(worker.counters.temp_files_created, 1);
        assert_eq!(worker.counters.temp_files_removed, 0);
        assert_eq!(worker.counters.rename_calls, 1);
        let mailbox = shared.0.lock().expect("terminal mailbox");
        assert!(mailbox.shutdown);
        assert!(!mailbox.in_flight && mailbox.pending.is_none());
        assert_eq!(mailbox.published, 1);
        assert_eq!(mailbox.failed, 0);
        assert_eq!(mailbox.cancelled, 0);
        assert_eq!(mailbox.stale, 0);
        assert!(mailbox.worker_error.is_none() && mailbox.equations_hold());
        drop(mailbox);
        assert_eq!(
            count_residue(&directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        assert_eq!(q_current(), 0);
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_restart_discovers_and_removes_only_owned_temps() {
        let (root, directory_path, mut worker, request) =
            projection_apply_fixture("restart-owned-temp", ProjectionFault::None);
        let mut native = Counters::default();
        let (file, mut abandoned) =
            create_temp(&worker.directory, &mut native).expect("abandoned owned temp");
        let abandoned_name = abandoned.name.clone();
        let authority = ProjectionTokenAuthority::from_store(worker.store.as_ref().expect("store"));
        mark_projection_temp_owned(&file, authority, &abandoned_name).expect("ownership marker");
        abandoned.active = false;
        drop(file);
        drop(abandoned);
        let retained_name = ".g3-tmp-retained-unowned";
        let retained = openat_file(
            &worker.directory,
            retained_name,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
        .expect("unowned same-kind residue");
        drop(retained);
        let restart =
            cleanup_projection_restart_temps(&directory_path, &worker.directory, authority)
                .expect("restart cleanup");
        assert_eq!(restart, (2, 1, 1));
        assert!(stat_at(&worker.directory, &abandoned_name)
            .expect("owned stat")
            .is_none());
        assert!(stat_at(&worker.directory, retained_name)
            .expect("unowned stat")
            .is_some_and(|native| native.is_regular()));
        worker.counters.restart_temps_discovered = restart.0;
        worker.counters.restart_temps_removed = restart.1;
        worker.counters.restart_temps_retained = restart.2;
        worker.counters.directory_sync_calls += u64::from(restart.1 != 0);
        let target = request.target;
        worker
            .apply(request, Instant::now(), None)
            .expect("publish after restart cleanup");
        assert_eq!(worker.active.identity.namespace_root, target.namespace);
        assert_eq!(worker.counters.restart_temps_discovered, 2);
        assert_eq!(worker.counters.restart_temps_removed, 1);
        assert_eq!(worker.counters.restart_temps_retained, 1);
        assert!(unlink_at(&worker.directory, retained_name).expect("retained cleanup"));
        assert_eq!(
            count_residue(&directory_path, ".g3-tmp-").expect("terminal residue"),
            0
        );
        assert_eq!(q_current(), 0);
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_restart_substitution_after_authentication_retains_replacement() {
        let (root, directory_path, worker, _) =
            projection_apply_fixture("restart-substitution", ProjectionFault::None);
        let authority = ProjectionTokenAuthority::from_store(worker.store.as_ref().expect("store"));
        let mut native = Counters::default();
        let (file, mut owned) =
            create_temp(&worker.directory, &mut native).expect("owned restart temp");
        let name = owned.name.clone();
        mark_projection_temp_owned(&file, authority, &name).expect("ownership marker");
        let authenticated = fstat_file(&file).expect("authenticated identity").0;
        owned.active = false;
        let retained_name = ".g5-authenticated-restart-temp";
        let replacement_identity = std::cell::Cell::new(None);
        let removed = unlink_authenticated_projection_temp(
            &worker.directory,
            &name,
            &file,
            authority,
            authenticated,
            || {
                rename_at(&worker.directory, &name, retained_name)?;
                let replacement = openat_file(
                    &worker.directory,
                    &name,
                    libc::O_RDWR
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_CLOEXEC
                        | libc::O_NOFOLLOW,
                    0o600,
                )?;
                replacement_identity.set(Some(fstat_file(&replacement)?.0));
                Ok(())
            },
        )
        .expect("substitution classification");
        assert!(!removed);
        let replacement_identity = replacement_identity.get().expect("replacement identity");
        assert_ne!(replacement_identity.inode, authenticated.inode);
        assert_eq!(
            stat_at(&worker.directory, &name).expect("replacement stat"),
            Some(replacement_identity)
        );
        let retained = stat_at(&worker.directory, retained_name)
            .expect("retained owned stat")
            .expect("retained owned temp");
        assert_eq!(retained.device, authenticated.device);
        assert_eq!(retained.inode, authenticated.inode);
        assert!(unlink_at(&worker.directory, &name).expect("replacement cleanup"));
        assert!(unlink_at(&worker.directory, retained_name).expect("owned cleanup"));
        sync_fd(&worker.directory).expect("cleanup sync");
        drop(file);
        drop(owned);
        assert_eq!(
            count_residue(&directory_path, ".g3-tmp-").expect("terminal residue"),
            0
        );
        assert_eq!(q_current(), 0);
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_before_sync_and_before_rename_fail_with_prior_active() {
        for (fault, data_sync, metadata_sync) in [
            (ProjectionFault::BeforeSync, 0, 0),
            (ProjectionFault::BeforeRename, 1, 1),
        ] {
            let (root, directory_path, mut worker, request) =
                projection_apply_fixture(fault.name(), fault);
            let prior = worker.active.identity.namespace_root;
            let error = worker
                .apply(request, Instant::now(), None)
                .expect_err(fault.name());
            assert_eq!(
                error
                    .downcast_ref::<std::io::Error>()
                    .and_then(std::io::Error::raw_os_error),
                Some(libc::EIO)
            );
            assert_eq!(worker.active.identity.namespace_root, prior);
            assert_eq!(worker.counters.temp_files_created, 1);
            assert_eq!(worker.counters.temp_files_removed, 1);
            assert_eq!(worker.counters.data_sync_calls, data_sync);
            assert_eq!(worker.counters.metadata_sync_calls, metadata_sync);
            assert_eq!(worker.counters.rename_calls, 0);
            assert_eq!(worker.counters.directory_sync_calls, 1);
            assert_projection_fault_finalized(&worker, ProjectionFaultOutcome::IoError);
            assert_eq!(
                count_residue(&directory_path, ".g3-tmp-").expect("residue"),
                0
            );
            assert_eq!(q_current(), 0);
            drop(worker);
            fs::remove_dir_all(root).expect("cleanup");
        }
    }

    #[test]
    fn g5_projection_missing_seed_streams_verified_fallback_to_exact_target() {
        let (root, directory_path, mut worker, request) =
            projection_apply_fixture("missing-seed", ProjectionFault::MissingSeed);
        let target = request.target;
        let target_digest = request.target_digest;
        worker
            .apply(request, Instant::now(), None)
            .expect("missing-seed fallback");
        assert_eq!(worker.active.identity.namespace_root, target.namespace);
        assert_eq!(worker.active.identity.file_root, target.file);
        assert_eq!(worker.active.identity.length, target.length);
        assert_eq!(worker.active.identity.digest, target_digest);
        assert_eq!(worker.counters.missing_seed_fallbacks, 1);
        assert_eq!(worker.counters.full_fallbacks, 1);
        assert_eq!(worker.counters.clone_calls, 0);
        assert_eq!(worker.counters.seed_rotations, 1);
        assert_eq!(
            count_residue(&directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        assert_eq!(q_current(), 0);
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_symlink_seed_is_rejected_before_private_work() {
        use std::os::unix::fs::symlink;

        let (root, directory_path, mut worker, request) =
            projection_apply_fixture("wrong-kind-symlink", ProjectionFault::None);
        let prior = worker.active.identity.namespace_root;
        let retained_name = ".g5-projection-retained-seed";
        rename_at(&worker.directory, DESTINATION_NAME, retained_name).expect("retain seed");
        symlink(retained_name, directory_path.join(DESTINATION_NAME)).expect("substitute symlink");
        let error = worker
            .apply(request, Instant::now(), None)
            .expect_err("symlink seed admission");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::WrongLogicalRole)
        );
        assert_eq!(worker.active.identity.namespace_root, prior);
        assert_eq!(worker.counters.seed_admission_rejections, 1);
        assert_eq!(worker.counters.temp_files_created, 0);
        assert_eq!(worker.counters.rename_calls, 0);
        assert_projection_fault_finalized(&worker, ProjectionFaultOutcome::WrongLogicalRole);
        assert!(unlink_at(&worker.directory, DESTINATION_NAME).expect("symlink cleanup"));
        rename_at(&worker.directory, retained_name, DESTINATION_NAME).expect("restore seed");
        assert_eq!(
            count_residue(&directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        assert_eq!(q_current(), 0);
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_in_place_untouched_seed_mutation_fails_before_private_work() {
        let (root, directory_path, mut worker, request) =
            projection_apply_fixture("in-place-seed-mutation", ProjectionFault::None);
        let prior = worker.active.identity.namespace_root;
        let before = stat_at(&worker.directory, DESTINATION_NAME)
            .expect("before stat")
            .expect("visible seed");
        let dirty = request.ranges().first().expect("dirty range");
        let offset = if dirty.start != 0 { 0 } else { dirty.end };
        assert!(offset < request.parent.length && !dirty.contains(&offset));
        let mut substitute = openat_file(
            &worker.directory,
            DESTINATION_NAME,
            libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0,
        )
        .expect("in-place writer");
        substitute.seek(SeekFrom::Start(offset)).expect("seek");
        let mut byte = [0_u8; 1];
        substitute.read_exact(&mut byte).expect("read");
        byte[0] ^= 0xff;
        substitute.seek(SeekFrom::Start(offset)).expect("seek");
        substitute.write_all(&byte).expect("write");
        substitute.sync_all().expect("sync mutation");
        let after = fstat_file(&substitute).expect("after stat").0;
        assert_eq!(after.device, before.device);
        assert_eq!(after.inode, before.inode);
        assert_eq!(after.length, before.length);
        drop(substitute);

        let error = worker
            .apply(request, Instant::now(), None)
            .expect_err("mutated seed");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::IdentityMismatch)
        );
        assert_eq!(worker.active.identity.namespace_root, prior);
        assert_eq!(worker.counters.seed_admission_rejections, 1);
        assert_eq!(worker.counters.clone_calls, 0);
        assert_eq!(worker.counters.temp_files_created, 0);
        assert_eq!(worker.counters.rename_calls, 0);
        assert_projection_fault_finalized(&worker, ProjectionFaultOutcome::IdentityMismatch);
        assert!(!worker.counters.fault_active_identity_matches);
        assert_eq!(
            count_residue(&directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        assert_eq!(q_current(), 0);
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_temp_name_substitution_fails_before_rename() {
        let root = test_root("projection-temp-name-substitution");
        fs::create_dir(&root).expect("root");
        let directory = open_dir(&root).expect("directory");
        let mut native = Counters::default();
        let (owned_file, temp) = create_temp(&directory, &mut native).expect("owned temp");
        let owned = fstat_file(&owned_file).expect("owned identity").0;
        let original_name = temp.name.clone();
        let retained_name = ".g5-owned-temp-retained";
        rename_at(&directory, &original_name, retained_name).expect("retain owned inode");
        let substitute = openat_file(
            &directory,
            &original_name,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
        .expect("substitute");
        assert_eq!(
            verify_projection_temp_name(&directory, &original_name, owned)
                .expect_err("substituted name")
                .downcast_ref::<CoreError>(),
            Some(&CoreError::IdentityMismatch)
        );
        drop(substitute);
        assert!(unlink_at(&directory, &original_name).expect("substitute cleanup"));
        drop(owned_file);
        drop(temp);
        assert!(unlink_at(&directory, retained_name).expect("retained cleanup"));
        drop(directory);
        fs::remove_dir(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_cancel_then_restart_preserves_seed_and_publishes() {
        let (root, directory_path, mut worker, cancelled) =
            projection_apply_fixture("cancel-restart-apply", ProjectionFault::CancelBeforeNative);
        let prior = worker.active.identity.namespace_root;
        let target = cancelled.target;
        let restarted = ProjectionRequest {
            parent: cancelled.parent,
            parent_digest: cancelled.parent_digest,
            target: cancelled.target,
            target_digest: cancelled.target_digest,
            plan: cancelled.plan.clone(),
            contended: false,
            policy: cancelled.policy,
            force_full_fallback: cancelled.force_full_fallback,
            token: None,
            edge_authenticated: true,
            end_to_end: None,
            fault: ProjectionFault::None,
        };
        assert_eq!(
            worker
                .apply(cancelled, Instant::now(), None)
                .expect_err("cancel")
                .downcast_ref::<ProjectionServiceError>(),
            Some(&ProjectionServiceError::Cancelled)
        );
        assert_eq!(worker.active.identity.namespace_root, prior);
        assert_eq!(worker.counters.rename_calls, 0);

        worker
            .apply(restarted, Instant::now(), None)
            .expect("restart publish");
        assert_eq!(worker.active.identity.namespace_root, target.namespace);
        assert_eq!(
            count_residue(&directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_reader_reopen_failure_reconciles_installed_target() {
        let (root, directory_path, mut worker, mut request) = projection_apply_fixture(
            "reader-reopen-reconcile",
            ProjectionFault::ReaderReopenFailure,
        );
        request.contended = true;
        let target = request.target;
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(0);
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(0);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let rendezvous = ProjectionContentionRendezvous {
            ready: ready_tx,
            writer_started: started_rx,
            reader_done: done_tx,
            release: release_rx,
        };
        let database = directory_path.join("store.sqlite");
        let foreground = std::thread::spawn(move || {
            ready_rx.recv().expect("ready");
            let mut store = Store::open(&database, SELECTED_PROFILE).expect("writer");
            let mut metrics = Metrics::default();
            store.begin(&mut metrics).expect("begin");
            started_tx.send(()).expect("started");
            done_rx.recv().expect("reader done");
            store.rollback(&mut metrics).expect("rollback");
            finish_q(&mut metrics).expect("Q0");
            release_tx.send(()).expect("release");
        });
        let origin = Instant::now();
        assert!(worker
            .apply(request, origin, Some((&rendezvous, origin)))
            .is_err());
        foreground.join().expect("foreground");
        assert_eq!(worker.active.identity.namespace_root, target.namespace);
        assert!(worker.store.is_some());
        assert_eq!(worker.counters.reconciliation_calls, 1);
        assert!(worker.counters.directory_sync_calls >= 2);
        assert_eq!(
            count_residue(&directory_path, ".g3-tmp-").expect("residue"),
            0
        );
        drop(worker);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_cancellation_releases_owned_temp_and_restart_conserves_state() {
        let root = test_root("g5-projection-cancel-restart");
        fs::create_dir(&root).expect("root");
        let directory = open_dir(&root).expect("directory");
        let mut native = Counters::default();
        let (file, temp) = create_temp(&directory, &mut native).expect("owned temp");
        drop(file);
        drop(temp);
        assert_eq!(count_residue(&root, ".g3-tmp-").expect("cancel cleanup"), 0);

        let mut mailbox = ProjectionMailbox {
            in_flight: false,
            pending: None,
            shutdown: false,
            release_first: true,
            submitted: 0,
            coalesced: 0,
            started: 0,
            published: 0,
            cancelled: 0,
            failed: 0,
            stale: 0,
            sqlite_busy_errors: 0,
            sqlite_locked_errors: 0,
            worker_error: None,
            token_authority: projection_test_authority(),
            exact_ordinal: None,
            same_size_ordinal: None,
            count_storm_ordinal: None,
            isolated_sparse_accepted: false,
        };
        mailbox
            .submit(projection_request(b"cancelled"))
            .expect("submit cancelled");
        mailbox.take().expect("take cancelled").expect("request");
        mailbox.in_flight = false;
        mailbox.cancelled = 1;
        assert!(mailbox.equations_hold());
        let mut restart = projection_request(b"restart");
        restart.policy = RequestPolicy::LatestFollowing {
            stream: LatestStream::SameSize,
            ordinal: 1,
        };
        mailbox.submit(restart).expect("restart submit");
        mailbox
            .take()
            .expect("restart take")
            .expect("restart request");
        mailbox.complete().expect("restart complete");
        mailbox.shutdown = true;
        assert!(mailbox.equations_hold());
        assert!(!mailbox.in_flight && mailbox.pending.is_none());
        drop(directory);
        fs::remove_dir(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_coalesces_ranges_and_falls_back_at_exact_caps() {
        assert_eq!(
            projection_plan(vec![8..12, 0..4, 4..8], 16).expect("merge"),
            ProjectionPlan::Ranges(vec![0..12])
        );
        let too_many = (0..=G5_PROJECTION_MAX_RANGES).map(|index| {
            let start = u64::try_from(index * 2).expect("start");
            start..start + 1
        });
        assert_eq!(
            projection_plan(too_many, 1024).expect("count fallback"),
            ProjectionPlan::FullFallback
        );
        assert_eq!(
            projection_plan((0..1_000_000).map(|_| 0..1), 16)
                .expect("overlapping producer fallback"),
            ProjectionPlan::FullFallback
        );
        assert_eq!(
            projection_plan(
                vec![0..G5_PROJECTION_MAX_DIRTY_BYTES + 1],
                G5_PROJECTION_MAX_DIRTY_BYTES + 1
            )
            .expect("byte fallback"),
            ProjectionPlan::FullFallback
        );
        assert_eq!(
            projection_plan(vec![9..8], 16)
                .expect_err("invalid range")
                .downcast_ref::<ProjectionServiceError>(),
            Some(&ProjectionServiceError::InvalidDirtyRange)
        );
        assert_eq!(
            projection_build_class(&ProjectionPlan::Ranges(vec![16..17]), 16, 17),
            None
        );
    }

    #[test]
    fn g5_projection_fixture_modes_are_exact_compact_and_within_preparation_budget() {
        let mut preparation_ns = 0_u128;
        for (mode, size, exact_count, latest_count) in [
            ("self-check", 250_000_u64, 1_usize, 0_usize),
            ("screen-count", 250_000, 1, 1),
            ("screen", 250_000, 2, 2),
            ("gate", 250_000, 64, 100),
        ] {
            assert_eq!(g5_projection_fixture_size(mode).expect("mode size"), size);
            let root = test_root(&format!("g5-projection-compact-{mode}"));
            let started = Instant::now();
            let report = prepare_g5_projection_fixture(&root, mode).expect("compact fixture");
            preparation_ns = preparation_ns
                .checked_add(started.elapsed().as_nanos())
                .expect("preparation wall");
            assert!(report.contains(&format!("\"mode\":\"{mode}\"")));
            assert!(report.contains(&format!("\"size_bytes\":{size}")));
            let fixture = parse_projection_fixture(&root).expect("parse compact fixture");
            assert_eq!(fixture.exact_count, exact_count);
            assert_eq!(fixture.chain.len(), exact_count + latest_count - 1);

            let mut pending = vec![root.clone()];
            while let Some(path) = pending.pop() {
                for entry in fs::read_dir(&path).expect("compact inventory") {
                    let entry = entry.expect("compact entry");
                    let metadata = fs::symlink_metadata(entry.path()).expect("compact metadata");
                    assert!(!metadata.file_type().is_symlink());
                    assert!(!entry.file_name().as_bytes().starts_with(b"same-chain-"));
                    if metadata.is_dir() {
                        pending.push(entry.path());
                    } else {
                        assert!(metadata.is_file());
                        assert!(metadata.len() <= 100_000_000);
                    }
                }
            }
            fs::remove_dir_all(root).expect("compact cleanup");
        }
        println!("G5ProjectionFourModePreparationElapsedNs={preparation_ns}");
        assert!(preparation_ns <= 60_000_000_000);
    }

    #[test]
    fn g5_projection_rotates_the_active_visible_seed() {
        let root = test_root("g5-projection-rotation");
        let report = g5_projection_self_check(&root).expect("projection self-check");
        assert!(report.contains("\"submitted\":6"));
        assert!(report.contains("\"coalesced\":1"));
        assert!(report.contains("\"published\":5"));
        assert!(report.contains("\"seed_rotations\":5"));
        assert!(report.contains("\"full_fallbacks\":2"));
        assert!(report.contains("\"q_terminal\":0"));
        assert!(report.contains("\"shutdown\":\"drained\""));
        assert_eq!(
            projection_json_scalar(&report, "size_bytes"),
            G5_PROJECTION_MECHANISM_BYTES.to_string()
        );
        assert_eq!(
            projection_json_scalar(&report, "route_class"),
            G5_PROJECTION_ROUTE_CLASS
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_release_fault_selector_emits_receipt() {
        let root = test_root("g5-projection-release-fault-selector");
        prepare_g5_projection_fixture(&root, "self-check").expect("fixture");
        let report =
            run_g5_projection_suite(&root, "fault-directory-sync-lost-ack").expect("fault receipt");
        assert!(report.contains("\"status\":\"PASS\""));
        assert!(report.contains("\"fault_selector\":\"DirectorySyncLostAck\""));
        assert!(report.contains("\"status\":\"ObservedCompleteApply\""));
        assert!(report.contains("\"reconciliation_calls\":1"));
        assert!(report.contains("\"directory_sync_calls\":6"));
        assert_eq!(projection_json_scalar(&report, "size_bytes"), "250000");
        assert_eq!(
            projection_json_scalar(&report, "route_class"),
            G5_PROJECTION_ROUTE_CLASS
        );
        fs::remove_dir_all(root).expect("cleanup");

        let root = test_root("g5-projection-release-missing-seed");
        prepare_g5_projection_fixture(&root, "self-check").expect("fixture");
        let report = run_g5_projection_suite(&root, "fault-missing-seed").expect("fault receipt");
        assert!(report.contains("\"status\":\"PASS\""));
        assert!(report.contains("\"fault_selector\":\"MissingSeed\""));
        assert!(report.contains("\"status\":\"ObservedCompleteApply\""));
        assert!(report.contains("\"missing_seed_fallbacks\":1"));
        assert!(report.contains("\"seed_admission_rejections\":0"));
        assert!(report.contains("\"q_terminal\":0"));
        assert!(report.contains("\"terminal_temp_residue\":0"));
        assert_eq!(projection_json_scalar(&report, "size_bytes"), "250000");
        assert_eq!(
            projection_json_scalar(&report, "route_class"),
            G5_PROJECTION_ROUTE_CLASS
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn g5_projection_parent_chain_and_fixture_errors_are_exact() {
        let active_root = ObjectId::for_bytes(b"active");
        let other = ObjectId::for_bytes(b"other");
        let root = test_root("g5-projection-errors");
        fs::create_dir(&root).expect("root");
        assert_eq!(
            parse_projection_fixture(&root)
                .err()
                .expect("missing fixture")
                .downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::raw_os_error),
            Some(libc::ENOENT)
        );
        let active = Roots {
            namespace: active_root,
            file: active_root,
            length: 1,
            references: 1,
        };
        let request = projection_request(b"other");
        assert_ne!(request.parent.namespace, active.namespace);
        assert_eq!(
            ProjectionServiceError::ParentChainMismatch.to_string(),
            "ProjectionParentChainMismatch"
        );
        assert_eq!(
            ProjectionServiceError::FixtureMismatch.to_string(),
            "ProjectionFixtureMismatch"
        );
        assert_eq!(other, request.target.namespace);
        fs::remove_dir(root).expect("cleanup");
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
        assert_eq!(result.max_single_buffer_bytes, 0);
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
        assert_eq!(
            count_residue(&prepared.directory_path, ".g3-quarantine-").expect("quarantine residue"),
            0
        );
        let mut counters = Counters::default();
        consume_permit(&mut prepared, &mut counters).expect("first consumption");
        assert!(consume_permit(&mut prepared, &mut counters).is_err());
        assert_eq!(counters.permit_consumptions, 1);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn post_fclone_open_and_stat_failures_are_typed_unresolved() {
        for fault in ["open", "stat"] {
            let root = test_root(&format!("clone-{fault}-unresolved"));
            let mut prepared =
                prepare(&root, 128 * 1024, Scenario::QualifiedOneByte).expect("prepare row");
            if fault == "open" {
                prepared.fault.clone_open_failure = true;
            } else {
                prepared.fault.clone_stat_failure = true;
            }
            let error = match run_operation(&mut prepared) {
                Ok(_) => panic!("binding fault must not fallback"),
                Err(error) => error,
            };
            let unresolved = error
                .downcast_ref::<CloneCleanupUnresolved>()
                .expect("typed unresolved clone cleanup");
            assert_eq!(unresolved.first, FailureCause::Core(CoreError::Io));
            assert_eq!(
                count_residue(&prepared.directory_path, ".g3-tmp-")
                    .expect("unresolved clone residue"),
                1
            );
            assert_eq!(
                count_residue(&prepared.directory_path, ".g3-quarantine-")
                    .expect("no quarantine mutation"),
                0
            );
            fs::remove_dir_all(root).expect("cleanup unresolved row");
        }
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

    #[test]
    fn g4_temp_cleanup_is_inode_bound_and_first_publish_is_exclusive() {
        let root = test_root("g4-native-races");
        fs::create_dir(&root).expect("create root");
        let directory = open_dir(&root).expect("open root");
        let mut counters = Counters::default();
        let (file, mut temp) = create_temp(&directory, &mut counters).expect("create temp");
        let original_name = temp.name.clone();
        drop(file);
        fs::rename(root.join(&original_name), root.join("retained-original"))
            .expect("move original");
        File::create(root.join(&original_name)).expect("substitute name");
        let error = temp
            .remove(&mut counters)
            .expect_err("must not unlink substitute");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::AmbiguousDurability)
        );
        assert!(root.join(&original_name).is_file());
        assert_eq!(
            count_residue(&root, ".g3-quarantine-").expect("quarantine residue"),
            0
        );
        fs::remove_file(root.join(&original_name)).expect("remove substitute");
        fs::remove_file(root.join("retained-original")).expect("remove original");

        let (_, mut raced) = create_temp(&directory, &mut counters).expect("raced temp");
        let raced_name = raced.name.clone();
        let error = raced
            .remove_with_substitution_after_validation(&mut counters, "retained-raced")
            .expect_err("substitution after validation must remain unresolved");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::AmbiguousDurability)
        );
        assert_eq!(
            fs::read(root.join(&raced_name)).expect("substitute survives"),
            b"substitute"
        );
        assert!(root.join("retained-raced").is_file());
        fs::remove_file(root.join(&raced_name)).expect("remove raced substitute");
        fs::remove_file(root.join("retained-raced")).expect("remove raced original");

        for (index, kind) in ["file", "symlink", "directory"].into_iter().enumerate() {
            let case = root.join(format!("case-{index}"));
            fs::create_dir(&case).expect("create case");
            let case_directory = open_dir(&case).expect("open case");
            let mut case_counters = Counters::default();
            let (_, mut candidate) =
                create_temp(&case_directory, &mut case_counters).expect("candidate temp");
            match kind {
                "file" => {
                    File::create(case.join(DESTINATION_NAME)).expect("target file");
                }
                "symlink" => {
                    std::os::unix::fs::symlink("missing", case.join(DESTINATION_NAME))
                        .expect("target symlink");
                }
                "directory" => {
                    fs::create_dir(case.join(DESTINATION_NAME)).expect("target directory");
                }
                _ => unreachable!(),
            }
            assert!(
                rename_exclusive_at(&case_directory, &candidate.name, DESTINATION_NAME).is_err()
            );
            assert!(stat_at(&case_directory, &candidate.name)
                .expect("candidate stat")
                .is_some());
            candidate
                .remove(&mut case_counters)
                .expect("candidate cleanup");
            match kind {
                "file" | "symlink" => {
                    fs::remove_file(case.join(DESTINATION_NAME)).expect("target cleanup")
                }
                "directory" => fs::remove_dir(case.join(DESTINATION_NAME)).expect("target cleanup"),
                _ => unreachable!(),
            }
            drop(case_directory);
            fs::remove_dir(case).expect("case cleanup");
        }
        drop(directory);
        fs::remove_dir(root).expect("root cleanup");
    }

    #[test]
    fn g4_rows_share_proofs_and_candidate_publishes_batched_bytes() {
        let root = test_root("g4-integrated");
        let fixture = prepare_g4_fixture(&root, SOURCE_1).expect("fixture");
        assert!(fixture.contains("\"status\":\"PASS\""));
        let closure_on =
            run_g4_row(&root, SOURCE_1, "r1-closure-on", &root.join("unused")).expect("closure on");
        let closure_off = run_g4_row(&root, SOURCE_1, "r1-closure-off", &root.join("unused"))
            .expect("closure off");
        assert!(closure_on.contains("\"content_closure_status\":\"computed\""));
        assert!(closure_on.contains("\"closure_fold_enabled\":true"));
        assert!(closure_off.contains("\"content_closure\":null"));
        assert!(closure_off.contains("\"closure_fold_enabled\":false"));
        assert!(closure_off.contains("\"closure_fold_updates\":0"));
        let candidate = run_g4_row(
            &root,
            SOURCE_1,
            "m0-candidate",
            &root.join("candidate-output"),
        )
        .expect("candidate materialization");
        assert!(candidate.contains("\"status\":\"PASS\""));
        assert!(candidate.contains("\"chunk_scalar_query_calls\":0"));
        assert!(candidate.contains("\"publication_status\":\"committed\""));
        assert!(candidate.contains("\"temp_residue_count\":0"));
        assert!(candidate.contains("\"final_residue_count\":0"));
        fs::remove_dir(root.join("candidate-output")).expect("output cleanup");
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn g4_first_publish_reconciles_lost_ack_and_never_unlinks_a_substitute() {
        let root = test_root("g4-first-publish-faults");
        prepare_g4_fixture(&root, SOURCE_1).expect("fixture");
        let directory = g4_fixture_directory(&root);
        let source = directory.join("target.source");
        let mut store =
            Store::open(&directory.join("store.sqlite"), SELECTED_PROFILE).expect("open fixture");
        let head = store.current_head().expect("head").expect("visible head");
        let mut root_metrics = Metrics::default();
        let roots = g4_roots(&store, head.1, &mut root_metrics).expect("roots");
        finish_q(&mut root_metrics).expect("root Q");
        let (_, digest, _) = hash_file(&source).expect("source digest");
        let sequence = source_cdc_sequence(&source)
            .expect("source sequence")
            .1
            .parse::<ObjectId>()
            .expect("sequence digest")
            .to_bytes();

        let lost_ack_root = root.join("lost-ack-output");
        let lost_ack = g4_materialize(
            &mut store,
            &head,
            roots,
            digest,
            sequence,
            &lost_ack_root,
            G4NativeAlgorithm::BatchedCandidate,
            G4NativeFault {
                directory_sync_lost_ack: true,
                ..G4NativeFault::default()
            },
        )
        .expect("lost acknowledgement reconciliation");
        assert_eq!(lost_ack.publication_status, "requested-visible");
        assert_eq!(lost_ack.reconciliation_outcome, "requested-visible");
        assert_eq!(lost_ack.reconciliation_calls, 1);
        assert_eq!(lost_ack.directory_sync_calls, 3);
        assert_eq!(
            lost_ack.diagnostic.expect("lost ack diagnostic").first,
            Some(FailureCause::Core(CoreError::Io))
        );
        assert_eq!(lost_ack.temp_files_created, 1);
        assert_eq!(lost_ack.temp_files_removed, 1);
        assert!(stat_at(
            &open_dir(&lost_ack_root).expect("lost ack directory"),
            DESTINATION_NAME
        )
        .expect("lost ack final stat")
        .is_none());
        fs::remove_dir(&lost_ack_root).expect("lost ack output cleanup");

        let retry_failure_root = root.join("retry-failure-output");
        let retry_failure = match g4_materialize(
            &mut store,
            &head,
            roots,
            digest,
            sequence,
            &retry_failure_root,
            G4NativeAlgorithm::BatchedCandidate,
            G4NativeFault {
                directory_sync_lost_ack: true,
                directory_sync_retry_failure: true,
                ..G4NativeFault::default()
            },
        ) {
            Ok(_) => panic!("requested-visible retry must be acknowledged"),
            Err(error) => error,
        };
        let failure = retry_failure
            .downcast_ref::<G4NativePublicationFailure>()
            .expect("typed retry failure");
        assert_eq!(failure.0.first, Some(FailureCause::Core(CoreError::Io)));
        assert_eq!(failure.0.reconciliation, Reconciliation::Ambiguous);
        assert_eq!(
            failure.0.reconciliation_error,
            Some(FailureCause::Core(CoreError::Io))
        );
        assert_eq!(
            failure.0.dominant,
            Some(FailureCause::Core(CoreError::AmbiguousDurability))
        );
        assert!(stat_at(
            &open_dir(&retry_failure_root).expect("retry failure directory"),
            DESTINATION_NAME,
        )
        .expect("retry failure final stat")
        .is_none());
        assert_eq!(
            count_residue(&retry_failure_root, ".g3-tmp-").expect("retry failure residue"),
            0
        );
        fs::remove_dir(&retry_failure_root).expect("retry failure output cleanup");

        let verification_root = root.join("verification-output");
        let verification = match g4_materialize(
            &mut store,
            &head,
            roots,
            digest,
            sequence,
            &verification_root,
            G4NativeAlgorithm::BatchedCandidate,
            G4NativeFault {
                verification_failure: true,
                ..G4NativeFault::default()
            },
        ) {
            Ok(_) => panic!("verification failure must preserve cleanup provenance"),
            Err(error) => error,
        };
        let failure = verification
            .downcast_ref::<G4NativePublicationFailure>()
            .expect("typed G4 publication failure");
        assert_eq!(
            failure.0.first,
            Some(FailureCause::Core(CoreError::IdentityMismatch))
        );
        assert!(stat_at(
            &open_dir(&verification_root).expect("verification directory"),
            DESTINATION_NAME,
        )
        .expect("verification final stat")
        .is_none());
        assert_eq!(
            count_residue(&verification_root, ".g3-tmp-").expect("verification residue"),
            0
        );
        fs::remove_dir(&verification_root).expect("verification output cleanup");

        let substitution_root = root.join("substitution-output");
        let substitution = match g4_materialize(
            &mut store,
            &head,
            roots,
            digest,
            sequence,
            &substitution_root,
            G4NativeAlgorithm::BatchedCandidate,
            G4NativeFault {
                post_publish_substitution: true,
                ..G4NativeFault::default()
            },
        ) {
            Ok(_) => panic!("post-publication substitution must reject"),
            Err(error) => error,
        };
        let failure = substitution
            .downcast_ref::<G4NativePublicationFailure>()
            .expect("typed substitution failure");
        assert_eq!(failure.0.reconciliation, Reconciliation::DifferentHead);
        assert!(
            fs::symlink_metadata(substitution_root.join(DESTINATION_NAME))
                .expect("substitute survives")
                .file_type()
                .is_symlink()
        );
        assert!(substitution_root.join(".g4-fault-retained").is_file());
        fs::remove_file(substitution_root.join(DESTINATION_NAME)).expect("remove substitute");
        fs::remove_file(substitution_root.join(".g4-fault-retained"))
            .expect("remove retained target");
        fs::remove_dir(&substitution_root).expect("substitution output cleanup");

        drop(store);
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
