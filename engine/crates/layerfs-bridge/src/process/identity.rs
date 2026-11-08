//! A local process named by its id, kernel start time and boot, so a reused
//! id, or the same id and start time after a reboot or snapshot restore, is
//! never taken for the process that was started. The three facts come from
//! `/proc` on Linux and from `proc_pidinfo`/`sysctl` on macOS (see
//! [`super::macos`]).
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) use super::os::{boot, stat};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use nix::{sys::signal::Signal, unistd::Pid};
use std::io;

/// One started process: its id, its start time in clock ticks since boot
/// (`/proc/<pid>/stat` field 22) and the boot it started in
/// (`/proc/sys/kernel/random/boot_id`). All must match for it to count as
/// alive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalProcess {
    pub pid: u32,
    pub start: u64,
    /// The boot id as a number. `None` only for a record written before boot
    /// ids were kept: it is matched by id and start time alone, as it was
    /// then, and never signalled.
    pub boot: Option<u128>,
}

impl LocalProcess {
    /// The process running now under `pid`, read once from this kernel, in
    /// this boot.
    #[cfg(any(target_os = "linux", target_os = "macos"))]    pub fn observe(pid: u32) -> io::Result<Self> {
        let stat = stat(pid)?;
        Ok(Self {
            pid,
            start: stat.start,
            boot: Some(boot()?),
        })
    }
    /// True while this exact process exists and has not exited: a zombie
    /// (or a dead entry) is not alive, and another process under the same id
    /// has another start time or another boot.
    #[cfg(any(target_os = "linux", target_os = "macos"))]    pub fn alive(self) -> bool {
        let this_boot = self
            .boot
            .is_none_or(|recorded| boot().ok() == Some(recorded));
        match stat(self.pid) {
            Ok(stat) => this_boot && stat.start == self.start && !stat.ended(),
            Err(_) => false,
        }
    }
    /// Sends `signal` once, only after this exact process was observed alive.
    #[cfg(any(target_os = "linux", target_os = "macos"))]    pub(crate) fn signal(self, signal: Signal) -> io::Result<()> {
        if self.boot.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a local process recorded without its boot is never signalled",
            ));
        }
        if !self.alive() {
            return Ok(());
        }
        match nix::sys::signal::kill(pid(self.pid)?, signal) {
            Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
            Err(errno) => Err(errno.into()),
        }
    }
}

/// What this kernel says of one process.
#[derive(Clone, Copy, Debug)]
    #[cfg(any(target_os = "linux", target_os = "macos"))]pub(crate) struct Stat {
    pub(crate) state: u8,
    pub(crate) group: u32,
    pub(crate) start: u64,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Stat {
    /// A zombie or a dead entry: it runs nothing any more.
    pub(crate) fn ended(self) -> bool {
        matches!(self.state, b'Z' | b'X' | b'x')
    }
}

/// A process id as the kernel's signed type.
    #[cfg(any(target_os = "linux", target_os = "macos"))]pub(crate) fn pid(id: u32) -> io::Result<Pid> {
    i32::try_from(id)
        .map(Pid::from_raw)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process id width"))
}
/// The Windows observation: the crate's one audited Win32 process-query
/// surface (the control crate's lock and lease surfaces keep the same
/// shape). A process is named by its id and its absolute creation time, so
/// the boot field stays None - the id+start match the struct itself
/// documents as the pre-boot-record mode - and a reused id never passes
/// for the process that was recorded.
#[cfg(windows)]
pub mod windows {
    #![allow(unsafe_code)]
    use super::LocalProcess;
    use std::io;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct FILETIME {
        dwLowDateTime: u32,
        dwHighDateTime: u32,
    }

    extern "system" {
        #[link_name = "OpenProcess"]
        fn OpenProcess(access: u32, inherit: u32, id: u32) -> isize;
        #[link_name = "GetProcessTimes"]
        fn GetProcessTimes(
            process: isize,
            creation: *mut FILETIME,
            exit: *mut FILETIME,
            kernel: *mut FILETIME,
            user: *mut FILETIME,
        ) -> u32;
        #[link_name = "GetExitCodeProcess"]
        fn GetExitCodeProcess(process: isize, code: *mut u32) -> u32;
        #[link_name = "CloseHandle"]
        fn CloseHandle(process: isize) -> u32;
    }

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    const ERROR_ACCESS_DENIED: i32 = 5;

    /// The process behind `pid`: its handle (closed at once) and its
    /// absolute creation time. None names no such process.
    fn open(pid: u32) -> io::Result<Option<u64>> {
        if pid == 0 {
            return Ok(None);
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process == 0 {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) {
                Ok(None)
            } else {
                Err(error)
            };
        }
        let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut exit = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut kernel = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut user = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let read = unsafe {
            GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user)
        };
        let start = u64::from(creation.dwHighDateTime) << 32 | u64::from(creation.dwLowDateTime);
        unsafe { CloseHandle(process) };
        if read == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(start))
    }

    impl LocalProcess {
        /// The process running now under `pid`: its creation time is the
        /// start; boot stays None (id+start match, the documented mode).
        pub fn observe(pid: u32) -> io::Result<Self> {
            let Some(start) = open(pid)? else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "no such process, or its times cannot be read",
                ));
            };
            Ok(Self { pid, start, boot: None })
        }
        /// True while this exact process exists and has not exited.
        pub fn alive(self) -> bool {
            let Ok(Some(start)) = open(self.pid) else {
                return false;
            };
            let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, self.pid) };
            if process == 0 {
                return false;
            }
            let mut code: u32 = 0;
            let read = unsafe { GetExitCodeProcess(process, &mut code) };
            unsafe { CloseHandle(process) };
            read != 0 && code == STILL_ACTIVE && start == self.start
        }
    }
}
