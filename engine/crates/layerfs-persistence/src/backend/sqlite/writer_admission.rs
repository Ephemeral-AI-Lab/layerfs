//! Linux local-volume cooperative ownership before one SQLite write attempt.
use crate::backend::records::BackendError;
use std::path::Path;
#[cfg(target_os = "linux")]
use {
    nix::{
        errno::Errno,
        fcntl::{Flock, FlockArg},
    },
    std::fs::File,
};

pub(crate) struct Writer {
    #[cfg(target_os = "linux")]
    lock: Flock<File>,
}

impl Writer {
    pub(crate) fn acquire(path: &Path) -> Result<Self, BackendError> {
        #[cfg(target_os = "linux")]
        {
            // Never open another FD to SQLite's main/WAL/SHM files here:
            // closing any main-file FD cancels every process POSIX lock on
            // that inode, including SQLite's live SHARED locks. An independent
            // description of the existing parent directory coordinates short
            // writer jobs without interfering with SQLite's descriptors.
            // Multiple Store files in one directory share this coarse lane.
            let directory = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let file = File::open(directory).map_err(|_| BackendError::Integrity)?;
            if !file
                .metadata()
                .map_err(|_| BackendError::Integrity)?
                .is_dir()
            {
                return Err(BackendError::Integrity);
            }
            // One blocking kernel wait, taken after the Session's FIFO turn,
            // so at most one thread per Session waits here. Its exits: the lock
            // (a holder process that exits releases it); `Cancelled` when a
            // signal interrupts the wait. No BEGIN/body has been attempted.
            match Flock::lock(file, FlockArg::LockExclusive) {
                Ok(lock) => Ok(Self { lock }),
                Err((_, Errno::EINTR)) => Err(BackendError::Cancelled),
                Err(_) => Err(BackendError::Integrity),
            }
        }
        #[cfg(target_os = "macos")]
        {
            // Darwin's flock conflicts with SQLite's open-file locks. Host Init
            // is sole-owner; cross-session contention retains exact SQLite Busy.
            let _ = path;
            Ok(Self {})
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = path;
            Err(BackendError::Integrity)
        }
    }

    pub(crate) fn release(self) -> Result<(), Self> {
        #[cfg(target_os = "linux")]
        {
            match self.lock.unlock() {
                Ok(file) => {
                    drop(file);
                    Ok(())
                }
                Err((lock, _)) => Err(Self { lock }),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(())
        }
    }
}

impl super::connection::State {
    pub(crate) fn finish_writer(&mut self) -> Result<(), BackendError> {
        if self.quarantined {
            // Connection is declared before this retained owner in State, so
            // SQLite teardown precedes advisory release at last-session drop.
            return Ok(());
        }
        if let Some(writer) = self.writer_hold.take() {
            if let Err(retained) = writer.release() {
                self.quarantined = true;
                self.writer_hold = Some(retained);
                return Err(BackendError::Unknown);
            }
        }
        Ok(())
    }
}
