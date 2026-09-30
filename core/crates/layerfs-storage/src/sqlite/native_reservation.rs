//! Safe shared native reservation primitive, without changing logical bytes.

use std::fs::File;

use crate::error::{StorageError, StorageResult};

/// Adds physical allocation up to `wanted`, preserving the existing file length.
/// Store callers retain their existing headroom/arithmetic/one-request bounds.
pub(crate) fn reserve_to(
    file: &File,
    metadata: &std::fs::Metadata,
    wanted: u64,
    maximum_request: u64,
    what: &'static str,
) -> StorageResult<()> {
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (file, metadata, wanted, maximum_request, what);
        Err(StorageError::UnsupportedPolicy {
            field: "physical reservation platform",
        })
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::os::unix::fs::MetadataExt;
        let apparent = metadata.len();
        let allocated = metadata
            .blocks()
            .checked_mul(512)
            .ok_or(StorageError::Integrity("Store allocated-byte overflow"))?;
        let amount = wanted.saturating_sub(allocated);
        if amount == 0 {
            return Ok(());
        }
        if amount > maximum_request {
            return Err(StorageError::CapacityExceeded {
                what,
                limit: maximum_request,
                actual: amount,
            });
        }
        #[cfg(target_os = "macos")]
        {
            let mut request = nix::libc::fstore_t {
                fst_flags: 0,
                fst_posmode: nix::libc::F_PEOFPOSMODE,
                fst_offset: 0,
                fst_length: amount
                    .try_into()
                    .map_err(|_| StorageError::Integrity("Store reservation length"))?,
                fst_bytesalloc: 0,
            };
            nix::fcntl::fcntl(file, nix::fcntl::FcntlArg::F_PREALLOCATE(&mut request)).map_err(
                |error| StorageError::Io(std::io::Error::from_raw_os_error(error as i32)),
            )?;
            if request.fst_bytesalloc != request.fst_length {
                return Err(StorageError::Integrity(
                    "partial Store physical reservation",
                ));
            }
        }
        #[cfg(target_os = "linux")]
        {
            let offset = apparent
                .try_into()
                .map_err(|_| StorageError::Integrity("Store reservation offset"))?;
            let length = wanted
                .saturating_sub(apparent)
                .try_into()
                .map_err(|_| StorageError::Integrity("Store reservation length"))?;
            nix::fcntl::fallocate(
                file,
                nix::fcntl::FallocateFlags::FALLOC_FL_KEEP_SIZE,
                offset,
                length,
            )
            .map_err(|error| StorageError::Io(std::io::Error::from_raw_os_error(error as i32)))?;
        }
        if file.metadata().map_err(StorageError::Io)?.len() != apparent {
            return Err(StorageError::Integrity(
                "Store reservation changed file length",
            ));
        }
        Ok(())
    }
}
