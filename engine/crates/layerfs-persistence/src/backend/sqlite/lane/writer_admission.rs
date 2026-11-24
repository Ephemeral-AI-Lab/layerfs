//! Cooperative local-volume ownership before one SQLite write attempt.
use crate::backend::records::BackendError;
use std::path::Path;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use {
    nix::{
        errno::Errno,
        fcntl::{Flock, FlockArg},
    },
    std::fs::File,
};

pub(crate) struct Writer {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    lock: Flock<File>,
}

impl Writer {
    pub(crate) fn acquire(path: &Path) -> Result<Self, BackendError> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            // Never open another FD to SQLite's main/WAL/SHM files here:
            // closing any main-file FD cancels every process POSIX lock on
            // that inode, including SQLite's live SHARED locks, and Darwin
            // flock on that file conflicts with SQLite's own fcntl locks. An
            // independent description of the existing parent directory
            // coordinates short writer jobs without interfering with SQLite's
            // descriptors. Multiple Store files in one directory share this
            // coarse lane.
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
            // One blocking kernel wait. A Session's writers wait here without
            // its FIFO turn (`Session::admit_writer`), so its reads are served
            // meanwhile; several of its threads may wait, and they resume in
            // the kernel's order. Its exits: the lock (a holder process that
            // exits releases it); `Cancelled` when a signal interrupts the
            // wait. No BEGIN/body has been attempted.
            match Flock::lock(file, FlockArg::LockExclusive) {
                Ok(lock) => Ok(Self { lock }),
                Err((_, Errno::EINTR)) => Err(BackendError::Cancelled),
                Err(_) => Err(BackendError::Integrity),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = path;
            Err(BackendError::Integrity)
        }
    }

    /// One non-blocking attempt at the same lane, for work that is skipped
    /// rather than waited for (#86: the idle WAL checkpoint). `Ok(None)`:
    /// another holder has it now (EWOULDBLOCK) and the caller skips its work.
    /// This is a single attempt, never a retry: no loop, sleep or busy handler
    /// follows it here, and a caller must not wrap it in one.
    pub(crate) fn try_acquire(path: &Path) -> Result<Option<Self>, BackendError> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            // The same independent directory description as `acquire`.
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
            match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                Ok(lock) => Ok(Some(Self { lock })),
                Err((_, Errno::EWOULDBLOCK)) => Ok(None),
                Err((_, Errno::EINTR)) => Err(BackendError::Cancelled),
                Err(_) => Err(BackendError::Integrity),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            // As `acquire`: no advisory lane on these platforms.
            Self::acquire(path).map(Some)
        }
    }

    pub(crate) fn release(self) -> Result<(), Self> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            match self.lock.unlock() {
                Ok(file) => {
                    drop(file);
                    Ok(())
                }
                Err((lock, _)) => Err(Self { lock }),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Ok(())
        }
    }
}

impl crate::backend::sqlite::session::connection::State {
    /// Releases the writer this State holds for one job. A quarantined
    /// State first closes its connection under that writer, so SQLite
    /// teardown (the rollback of any transaction the uncertain attempt left
    /// open, and a last close's checkpoint) still precedes advisory release,
    /// and the lane is not kept until the Session drops (finding #2).
    pub(crate) fn finish_writer(
        &mut self,
        background: Option<&crate::backend::sqlite::session::background::Checkpointer>,
    ) -> Result<(), BackendError> {
        let Some(writer) = self.writer_hold.take() else {
            return Ok(());
        };
        if self.quarantined && !self.close_quarantined(background) {
            // SQLite may still hold a write transaction: the writer stays
            // with the connection, declared before it in State.
            self.writer_hold = Some(writer);
            return Ok(());
        }
        if let Err(retained) = writer.release() {
            self.quarantined = true;
            self.writer_hold = Some(retained);
            return Err(BackendError::Unknown);
        }
        Ok(())
    }
}

impl crate::backend::sqlite::session::connection::Session {
    /// The cross-process writer lane for a writer that holds this Session's
    /// FIFO turn, without the turn ever waiting on it. `refuse` checks the
    /// state under the turn (quarantine, a writer this Session retains) before
    /// the lane is sought. One non-blocking attempt is a readiness probe, not
    /// a retry: when another Session or process holds the lane, the turn is
    /// given up, the lane is waited for in the kernel, and the turn is entered
    /// again, so the Session's reads are served while a writer waits on a
    /// foreign lane (a compaction's mark can hold it for tens of seconds).
    /// `refuse` runs again under the new turn, since another of the Session's
    /// callers may have quarantined it meanwhile; an exit after the lane is
    /// held drops the writer, which releases it. Nothing is attempted under
    /// the lane before this returns.
    pub(crate) fn admit_writer<'a>(
        &'a self,
        turn: crate::backend::admission::Turn<'a>,
        refuse: impl Fn(
            &mut crate::backend::sqlite::session::connection::State,
        ) -> Result<(), BackendError>,
    ) -> Result<(crate::backend::admission::Turn<'a>, Writer), BackendError> {
        refuse(&mut self.state())?;
        if let Some(writer) = Writer::try_acquire(&self.path)? {
            return Ok((turn, writer));
        }
        drop(turn);
        let writer = Writer::acquire(&self.path)?;
        let turn = self
            .admission
            .enter(layerfs_telemetry::timer::waits::Wait::Writer)?;
        refuse(&mut self.state())?;
        Ok((turn, writer))
    }
}
