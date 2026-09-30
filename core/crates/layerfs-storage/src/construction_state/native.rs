//! Fresh private native owners, descriptor association and one cleanup attempt.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::error::{StorageError, StorageResult};

use super::status::NativeIdentity;

pub(crate) const RESERVED_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) struct NativeDirectory {
    pub(crate) path: PathBuf,
    pub(crate) parent: NativeIdentity,
    pub(crate) identity: Option<NativeIdentity>,
    pub(crate) nonce: [u8; 32],
    base: File,
    base_path: PathBuf,
    name: String,
    file: Option<File>,
    created: bool,
    failed: bool,
    retirement_blocked: bool,
    cleanup_attempted: bool,
}

pub(crate) struct NativeFile {
    pub(crate) directory: Arc<Mutex<NativeDirectory>>,
    pub(crate) path: PathBuf,
    pub(crate) parent: NativeIdentity,
    pub(crate) directory_identity: Option<NativeIdentity>,
    pub(crate) identity: Option<NativeIdentity>,
    pub(crate) allocated: Option<u64>,
    pub(crate) quarantined: bool,
    name: String,
    file: Option<File>,
    created: bool,
    removed: bool,
    failed_close_fd: Option<i32>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn os_error(error: nix::errno::Errno) -> StorageError {
    StorageError::Io(std::io::Error::from_raw_os_error(error as i32))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn identity(metadata: &std::fs::Metadata) -> NativeIdentity {
    use std::os::unix::fs::MetadataExt;
    NativeIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        uid: metadata.uid(),
        mode: metadata.mode(),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn check_path(path: &Path, expected: NativeIdentity) -> StorageResult<()> {
    let metadata = std::fs::symlink_metadata(path).map_err(StorageError::Io)?;
    if metadata.file_type().is_symlink() || identity(&metadata) != expected {
        return Err(StorageError::Integrity(
            "construction scratch native path identity",
        ));
    }
    Ok(())
}

impl NativeDirectory {
    pub(crate) fn prepare(parent: &Path) -> StorageResult<Self> {
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = parent;
            Err(StorageError::UnsupportedPolicy {
                field: "construction scratch native platform",
            })
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            use nix::fcntl::{open, OFlag};
            use nix::sys::stat::Mode;
            use std::io::Read;
            use std::os::unix::fs::MetadataExt;
            if !parent.is_absolute() {
                return Err(StorageError::Integrity(
                    "construction scratch absolute parent",
                ));
            }
            let before = std::fs::symlink_metadata(parent).map_err(StorageError::Io)?;
            if !before.is_dir() || before.file_type().is_symlink() {
                return Err(StorageError::Integrity("construction scratch parent type"));
            }
            let base_path = std::fs::canonicalize(parent).map_err(StorageError::Io)?;
            let base = File::from(
                open(
                    &base_path,
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(os_error)?,
            );
            let actual = base.metadata().map_err(StorageError::Io)?;
            let parent = identity(&actual);
            if parent != identity(&before)
                || actual.uid() != nix::unistd::geteuid().as_raw()
                || actual.mode() & 0o022 != 0
            {
                return Err(StorageError::Integrity(
                    "construction scratch parent ownership",
                ));
            }
            let mut nonce = [0; 32];
            File::open("/dev/urandom")
                .and_then(|mut source| source.read_exact(&mut nonce))
                .map_err(StorageError::Io)?;
            if nonce == [0; 32] {
                return Err(StorageError::Integrity("construction scratch zero nonce"));
            }
            let mut name = String::from(".lfcs-");
            for byte in nonce {
                use std::fmt::Write;
                write!(&mut name, "{byte:02x}")
                    .map_err(|_| StorageError::Integrity("construction scratch name"))?;
            }
            Ok(Self {
                path: base_path.join(&name),
                parent,
                identity: None,
                nonce,
                base,
                base_path,
                name,
                file: None,
                created: false,
                failed: false,
                retirement_blocked: false,
                cleanup_attempted: false,
            })
        }
    }

    fn initialize(&mut self) -> StorageResult<()> {
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Err(StorageError::UnsupportedPolicy {
                field: "construction scratch native platform",
            })
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            use nix::fcntl::{openat, OFlag};
            use nix::sys::stat::{mkdirat, Mode};
            if self.file.is_some() && !self.failed {
                return self.verify();
            }
            if self.failed {
                return Err(StorageError::Integrity(
                    "construction scratch directory bootstrap failed",
                ));
            }
            check_path(&self.base_path, self.parent)?;
            self.failed = true;
            mkdirat(&self.base, self.name.as_str(), Mode::S_IRWXU).map_err(os_error)?;
            self.created = true;
            let before = std::fs::symlink_metadata(&self.path).map_err(StorageError::Io)?;
            self.identity = Some(identity(&before));
            let file = File::from(
                openat(
                    &self.base,
                    self.name.as_str(),
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(os_error)?,
            );
            let actual = file.metadata().map_err(StorageError::Io)?;
            if identity(&actual) != self.identity.unwrap()
                || !actual.is_dir()
                || self.identity.unwrap().uid != nix::unistd::geteuid().as_raw()
                || self.identity.unwrap().mode & 0o777 != 0o700
            {
                return Err(StorageError::Integrity(
                    "construction scratch directory ownership",
                ));
            }
            self.file = Some(file);
            self.failed = false;
            self.verify()
        }
    }

    fn verify(&self) -> StorageResult<()> {
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Err(StorageError::UnsupportedPolicy {
                field: "construction scratch native platform",
            })
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            check_path(&self.base_path, self.parent)?;
            if identity(&self.base.metadata().map_err(StorageError::Io)?) != self.parent {
                return Err(StorageError::Integrity(
                    "construction scratch parent descriptor",
                ));
            }
            let expected = self.identity.ok_or(StorageError::Integrity(
                "construction scratch missing directory identity",
            ))?;
            check_path(&self.path, expected)?;
            let file = self.file.as_ref().ok_or(StorageError::Integrity(
                "construction scratch missing directory descriptor",
            ))?;
            if identity(&file.metadata().map_err(StorageError::Io)?) != expected {
                return Err(StorageError::Integrity(
                    "construction scratch directory descriptor",
                ));
            }
            Ok(())
        }
    }
}

impl Drop for NativeDirectory {
    fn drop(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if self.created
            && !self.failed
            && !self.retirement_blocked
            && !self.cleanup_attempted
            && self.identity.is_some()
        {
            self.cleanup_attempted = true;
            if check_path(&self.base_path, self.parent).is_ok()
                && check_path(&self.path, self.identity.unwrap()).is_ok()
            {
                // rmdir removes only a known empty directory; no file recursion.
                let _ = nix::unistd::unlinkat(
                    &self.base,
                    self.name.as_str(),
                    nix::unistd::UnlinkatFlags::RemoveDir,
                );
            }
        }
    }
}

impl NativeFile {
    pub(crate) fn prepare(
        directory: Arc<Mutex<NativeDirectory>>,
        token: u64,
    ) -> StorageResult<Self> {
        let name = format!("state-{token:016x}.sqlite");
        let (path, parent, directory_identity) = {
            let native = directory
                .lock()
                .map_err(|_| StorageError::Integrity("construction scratch directory lock"))?;
            (native.path.join(&name), native.parent, native.identity)
        };
        Ok(Self {
            directory,
            path,
            parent,
            directory_identity,
            identity: None,
            allocated: None,
            quarantined: false,
            name,
            file: None,
            created: false,
            removed: false,
            failed_close_fd: None,
        })
    }

    pub(crate) fn has_file_effects(&self) -> bool {
        self.created
    }

    pub(crate) fn failed_close_descriptor(&self) -> Option<i32> {
        self.failed_close_fd
    }

    pub(crate) fn preserve_handles(&mut self) {
        // An ended authority cannot turn an unacknowledged or previously attempted
        // native close into another attempt. The process/kernel owns final teardown.
        // A raw failed-close number is observation only and is never reconstructed.
        if let Some(file) = self.file.take() {
            std::mem::forget(file);
        }
        if self.created && !self.removed || self.quarantined {
            // Last-authority loss cannot attempt directory retirement around an
            // unreleased/uncertain owner, even if another actor changed its path.
            match self.directory.lock() {
                Ok(mut directory) => directory.retirement_blocked = true,
                Err(poisoned) => poisoned.into_inner().retirement_blocked = true,
            }
        }
    }

    pub(crate) fn initialize(&mut self) -> StorageResult<()> {
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Err(StorageError::UnsupportedPolicy {
                field: "construction scratch native platform",
            })
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            use nix::fcntl::{openat, OFlag};
            use nix::sys::stat::Mode;
            use std::os::unix::fs::MetadataExt;
            let mut directory = self
                .directory
                .try_lock()
                .map_err(|_| StorageError::OwnershipUnavailable)?;
            let result = directory.initialize();
            self.directory_identity = directory.identity;
            if result.is_err() {
                self.quarantined = directory.created;
            }
            result?;
            let parent = directory.file.as_ref().unwrap();
            let file = File::from(
                openat(
                    parent,
                    self.name.as_str(),
                    OFlag::O_RDWR
                        | OFlag::O_CREAT
                        | OFlag::O_EXCL
                        | OFlag::O_NOFOLLOW
                        | OFlag::O_CLOEXEC,
                    Mode::S_IRUSR | Mode::S_IWUSR,
                )
                .map_err(os_error)?,
            );
            self.created = true;
            self.file = Some(file);
            let metadata = match self.file.as_ref().unwrap().metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    self.quarantined = true;
                    return Err(StorageError::Io(error));
                }
            };
            self.identity = Some(identity(&metadata));
            if !metadata.is_file()
                || metadata.nlink() != 1
                || metadata.len() != 0
                || metadata.uid() != nix::unistd::geteuid().as_raw()
                || metadata.mode() & 0o777 != 0o600
            {
                return Err(StorageError::Integrity(
                    "construction scratch file ownership",
                ));
            }
            drop(directory);
            self.reserve()
        }
    }

    pub(crate) fn verify(&self) -> StorageResult<()> {
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Err(StorageError::UnsupportedPolicy {
                field: "construction scratch native platform",
            })
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            use std::os::unix::fs::MetadataExt;
            if self.quarantined || self.failed_close_fd.is_some() {
                return Err(StorageError::Integrity(
                    "construction scratch native owner quarantined",
                ));
            }
            let directory = self
                .directory
                .try_lock()
                .map_err(|_| StorageError::OwnershipUnavailable)?;
            directory.verify()?;
            let expected = self.identity.ok_or(StorageError::Integrity(
                "construction scratch missing file identity",
            ))?;
            check_path(&self.path, expected)?;
            let file = self.file.as_ref().ok_or(StorageError::Integrity(
                "construction scratch missing file descriptor",
            ))?;
            let metadata = file.metadata().map_err(StorageError::Io)?;
            if identity(&metadata) != expected
                || metadata.nlink() != 1
                || metadata.len() > RESERVED_BYTES
            {
                return Err(StorageError::Integrity(
                    "construction scratch file descriptor",
                ));
            }
            Ok(())
        }
    }

    pub(crate) fn reserve(&mut self) -> StorageResult<()> {
        self.verify()?;
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Err(StorageError::UnsupportedPolicy {
                field: "construction scratch native platform",
            })
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let file = self.file.as_ref().unwrap();
            let metadata = file.metadata().map_err(StorageError::Io)?;
            crate::sqlite::native_reservation::reserve_to(
                file,
                &metadata,
                RESERVED_BYTES,
                RESERVED_BYTES,
                "construction scratch physical reservation",
            )?;
            self.observe_allocation()
        }
    }

    pub(crate) fn observe_allocation(&mut self) -> StorageResult<()> {
        self.verify()?;
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Err(StorageError::UnsupportedPolicy {
                field: "construction scratch native platform",
            })
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            use std::os::unix::fs::MetadataExt;
            let allocated = self
                .file
                .as_ref()
                .unwrap()
                .metadata()
                .map_err(StorageError::Io)?
                .blocks()
                .checked_mul(512)
                .ok_or(StorageError::Integrity(
                    "construction scratch allocated bytes",
                ))?;
            self.allocated = Some(allocated);
            if allocated != RESERVED_BYTES {
                return Err(StorageError::CapacityExceeded {
                    what: "construction scratch physical class",
                    limit: RESERVED_BYTES,
                    actual: allocated,
                });
            }
            Ok(())
        }
    }

    pub(crate) fn binding(&self, selector: &[u8; 32], token: u64) -> StorageResult<[u8; 32]> {
        let directory = self
            .directory
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch directory lock"))?;
        let mut digest = blake3::Hasher::new();
        digest.update(b"layerfs/construction-state/native/v1\0");
        digest.update(&directory.nonce);
        digest.update(&directory.parent.encode());
        digest.update(
            &directory
                .identity
                .ok_or(StorageError::Integrity(
                    "construction scratch directory binding",
                ))?
                .encode(),
        );
        digest.update(
            &self
                .identity
                .ok_or(StorageError::Integrity("construction scratch file binding"))?
                .encode(),
        );
        digest.update(selector);
        digest.update(&token.to_be_bytes());
        Ok(*digest.finalize().as_bytes())
    }

    pub(crate) fn header(
        &self,
        selector: &[u8; 32],
        token: u64,
        binding: &[u8; 32],
        version: u16,
    ) -> StorageResult<[u8; 192]> {
        let directory = self
            .directory
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch directory lock"))?;
        let mut header = [0; 192];
        header[..8].copy_from_slice(match version {
            1 => b"LFCSOWN1",
            2 => b"LFCSOWN2",
            _ => {
                return Err(StorageError::Integrity(
                    "construction scratch header version",
                ))
            }
        });
        header[8..10].copy_from_slice(&version.to_be_bytes());
        header[16..24].copy_from_slice(&token.to_be_bytes());
        header[24..56].copy_from_slice(selector);
        header[56..88].copy_from_slice(binding);
        header[88..120].copy_from_slice(&directory.nonce);
        header[120..144].copy_from_slice(&directory.parent.encode());
        header[144..168].copy_from_slice(
            &directory
                .identity
                .ok_or(StorageError::Integrity(
                    "construction scratch directory header",
                ))?
                .encode(),
        );
        header[168..192].copy_from_slice(
            &self
                .identity
                .ok_or(StorageError::Integrity("construction scratch file header"))?
                .encode(),
        );
        Ok(header)
    }

    pub(crate) fn release(&mut self) -> StorageResult<()> {
        if !self.created {
            return Ok(());
        }
        self.verify()?;
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Err(StorageError::UnsupportedPolicy {
                field: "construction scratch native platform",
            })
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            use std::os::fd::AsRawFd;
            let file = self.file.take().unwrap();
            let descriptor = file.as_raw_fd();
            if let Err(error) = nix::unistd::close(file) {
                self.failed_close_fd = Some(descriptor);
                self.quarantined = true;
                return Err(StorageError::UnknownOutcome {
                    original: Box::new(os_error(error)),
                });
            }
            let directory = self
                .directory
                .try_lock()
                .map_err(|_| StorageError::OwnershipUnavailable)?;
            directory.verify()?;
            check_path(&self.path, self.identity.unwrap())?;
            nix::unistd::unlinkat(
                directory.file.as_ref().unwrap(),
                self.name.as_str(),
                nix::unistd::UnlinkatFlags::NoRemoveDir,
            )
            .map_err(os_error)?;
            match std::fs::symlink_metadata(&self.path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.removed = true;
                    self.allocated = Some(0);
                    Ok(())
                }
                Err(error) => Err(StorageError::UnknownOutcome {
                    original: Box::new(StorageError::Io(error)),
                }),
                Ok(_) => Err(StorageError::UnknownOutcome {
                    original: Box::new(StorageError::Integrity(
                        "construction scratch replacement after unlink",
                    )),
                }),
            }
        }
    }
}
