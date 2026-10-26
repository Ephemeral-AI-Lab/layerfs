//! The Windows lane's dead-holder path, in its own binary so it never runs
//! beside the foreign-process test's re-exec'd children.
#![cfg(windows)]
mod support;
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{port::*, StoragePolicy};
use std::path::{Path, PathBuf};

fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}

fn reserve() -> Reserve {
    Reserve {
        packs: 1,
        ordinals: 1,
    }
}

fn authority(name: &[u8]) -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: name.to_vec(),
        cursor_key: [67; 32],
        incarnation: 1,
    }
}

/// The lane's dead-holder path: a thread that ends while owning the named
/// mutex leaves it abandoned, and the kernel hands ownership to the next
/// waiter. The abandoned state is produced deterministically here - a
/// thread waits on the product's own mutex name and returns without
/// releasing - and the Store must still open and write through that
/// `WAIT_ABANDONED` acquisition.
#[test]
fn a_dead_lane_holder_does_not_block_the_next_writer() {
    use sha2::{Digest, Sha256};
    let temp = support::Temp::new("windows-store-abandoned");
    let path = temp.join("store.sqlite");
    let catalog = authority(b"windows-store-abandoned");
    let seed = Handles::create(config(&path), StoragePolicy::frozen_default(), &catalog).unwrap();
    seed.storage.reserve(reserve()).unwrap();
    drop(seed);

    let directory = temp.path().to_path_buf();
    let holder = std::thread::spawn(move || {
        // The product's own lane name: SHA-256 of the resolved directory's
        // encoded bytes, truncated to 64 bits (windows_lane::lane_name).
        let resolved = std::fs::canonicalize(&directory).unwrap_or_else(|_| directory.clone());
        let digest = Sha256::digest(resolved.as_os_str().as_encoded_bytes());
        let mut tag = [0u8; 8];
        tag.copy_from_slice(&digest[..8]);
        let name: Vec<u16> = format!("Local\\layerfs-writer-lane-{:016x}", u64::from_be_bytes(tag))
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            windows_sys::Win32::System::Threading::CreateMutexW(
                std::ptr::null(),
                0,
                name.as_ptr(),
            )
        };
        assert!(!handle.is_null());
        let waited = unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(handle, windows_sys::Win32::System::Threading::INFINITE) };
        assert_eq!(waited, windows_sys::Win32::Foundation::WAIT_OBJECT_0);
        // The thread owns the mutex and now ends without ReleaseMutex: the
        // kernel marks it abandoned for the next waiter.
    });
    holder.join().unwrap();

    let reopened = Handles::open_writable(config(&path), &catalog.binding_key, catalog.cursor_key)
        .expect("an abandoned lane is taken by the next writer");
    reopened.storage.reserve(reserve()).unwrap();
    reopened.shutdown();
}

