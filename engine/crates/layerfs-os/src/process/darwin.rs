//! macOS process facts, which Linux reads from `/proc`: a process's state, group
//! and start time (`proc_pidinfo`, a system call: `ps` is setuid root, which a
//! Seatbelt-sandboxed caller may not execute) and this boot's identity
//! (`sysctlbyname`, the system call behind the `sysctl` tool, so reading it
//! starts no process). Each fact is read once; nothing is retried. The mount table and a forced unmount stay with
//! the Sandbox driver (layerfs-project#112 split them from this file).
use super::Stat;
use nix::libc;
use std::{ffi::CString, io, os::unix::ffi::OsStrExt, path::Path, sync::OnceLock};

/// Wait for volume writeback completion (`SYNC_VOLUME_WAIT = 0x02` in macOS
/// SDK `<unistd.h>`; checked against `<sys/mount.h>`, where `MNT_WAIT = 1` is
/// for `getfsstat(2)`/`vfs_sync` while `sync_volume_np(3)` and its flags live
/// in `<unistd.h>`).
const SYNC_VOLUME_WAIT: libc::c_int = 0x02;

/// The value of kernel variable `name` into `buffer` (`sysctlbyname`), or
/// its failure; answers how many bytes the kernel wrote.
#[allow(unsafe_code)]
fn sysctl(name: &str, buffer: &mut [u8]) -> io::Result<usize> {
    let name = CString::new(name).map_err(|_| invalid("sysctl name"))?;
    let mut size: libc::size_t = buffer.len();
    // SAFETY: `name` is NUL-terminated and alive for the call; the kernel
    // writes at most `size` bytes into `buffer` and stores the count in
    // `size`; no new value is set (null pointer, length 0).
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(size.min(buffer.len()))
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

/// State, process group and start time of `pid` from the kernel
/// (`proc_pidinfo`, `PROC_PIDTBSDINFO`); `NotFound` when no such process
/// exists. Not `ps`: it is setuid root, and a sandboxed caller (dsh's
/// Seatbelt profile for agents' bash) may not execute it.
#[allow(unsafe_code)]
pub fn stat(pid: u32) -> io::Result<Stat> {
    let id = i32::try_from(pid).map_err(|_| invalid("process id width"))?;
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: the buffer is a proc_bsdinfo of exactly `size` bytes.
    let got = unsafe {
        libc::proc_pidinfo(
            id,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if got <= 0 {
        let error = io::Error::last_os_error();
        return Err(match error.raw_os_error() {
            Some(libc::ESRCH) | Some(0) | None => io::Error::from(io::ErrorKind::NotFound),
            _ => error,
        });
    }
    if got != size {
        return Err(invalid("proc_bsdinfo size"));
    }
    // SAFETY: the kernel filled all `size` bytes.
    let info = unsafe { info.assume_init() };
    if info.pbi_pid != pid {
        return Err(invalid("proc_bsdinfo pid"));
    }
    Ok(from_info(&info))
}

/// `ps`'s state letters for `p_stat`, and the start time in microseconds:
/// equal for the same process, different for another under the same id.
fn from_info(info: &libc::proc_bsdinfo) -> Stat {
    let state = match info.pbi_status {
        1 => b'I',
        2 => b'R',
        3 => b'S',
        4 => b'T',
        libc::SZOMB => b'Z',
        _ => b'?',
    };
    Stat {
        state,
        group: info.pbi_pgid,
        start: info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec,
    }
}

/// Every process now, with its stat (`proc_listallpids`); a process that
/// ends or is not readable between the listing and its read is left out.
#[allow(unsafe_code)]
pub fn processes() -> io::Result<Vec<(u32, Stat)>> {
    // SAFETY: a null buffer asks only for the count.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut pids = vec![0 as libc::pid_t; count as usize + 64];
    let bytes = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
    // SAFETY: `pids` holds `bytes` bytes of pid_t.
    let listed = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    if listed < 0 {
        return Err(io::Error::last_os_error());
    }
    pids.truncate((listed as usize).min(pids.len()));
    Ok(pids
        .into_iter()
        .filter(|pid| *pid > 0)
        .filter_map(|pid| {
            let pid = pid as u32;
            stat(pid).ok().map(|stat| (pid, stat))
        })
        .collect())
}

/// This boot's identity as a number, read once per process:
/// `kern.bootsessionuuid`, else `kern.boottime` (seconds and microseconds).
pub fn boot() -> io::Result<u128> {
    static BOOT: OnceLock<u128> = OnceLock::new();
    if let Some(boot) = BOOT.get() {
        return Ok(*boot);
    }
    let mut text = [0u8; 64];
    let uuid = sysctl("kern.bootsessionuuid", &mut text)
        .ok()
        .and_then(|len| std::str::from_utf8(&text[..len]).ok().and_then(uuid));
    let boot = match uuid {
        Some(boot) => boot,
        None => boottime()?,
    };
    Ok(*BOOT.get_or_init(|| boot))
}

fn uuid(text: &str) -> Option<u128> {
    let text = text.trim_end_matches('\0').trim();
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    (hex.len() == 32)
        .then(|| u128::from_str_radix(&hex, 16).ok())
        .flatten()
}

/// `kern.boottime` (a `timeval`) as seconds * 10^6 + microseconds: the
/// number `sysctl -n kern.boottime`'s `{ sec, usec }` gave before.
fn boottime() -> io::Result<u128> {
    let mut raw = [0u8; std::mem::size_of::<libc::timeval>()];
    let bad = || invalid("kern.boottime");
    if sysctl("kern.boottime", &mut raw)? != raw.len() {
        return Err(bad());
    }
    // `timeval` is `tv_sec` (`time_t`) then `tv_usec` (`suseconds_t`)
    let (sec, rest) = raw.split_at(std::mem::size_of::<libc::time_t>());
    let usec = &rest[..std::mem::size_of::<libc::suseconds_t>()];
    let sec = libc::time_t::from_ne_bytes(sec.try_into().map_err(|_| bad())?);
    let usec = libc::suseconds_t::from_ne_bytes(usec.try_into().map_err(|_| bad())?);
    let sec = u128::try_from(sec).map_err(|_| bad())?;
    Ok(sec * 1_000_000 + u128::try_from(usec).map_err(|_| bad())?)
}

/// Flush buffered writes on the volume containing `path` and wait for
/// completion (`sync_volume_np(path, SYNC_VOLUME_WAIT)`).
#[allow(unsafe_code)]
pub fn sync_volume_wait(path: &Path) -> io::Result<()> {
    extern "C" {
        fn sync_volume_np(path: *const libc::c_char, flags: libc::c_int) -> libc::c_int;
    }
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains interior NUL"))?;
    // SAFETY: `c_path` is a valid NUL-terminated C string alive for the call,
    // and `SYNC_VOLUME_WAIT` is the documented wait flag for `sync_volume_np`.
    let rc = unsafe { sync_volume_np(c_path.as_ptr(), SYNC_VOLUME_WAIT) };
    match rc {
        0 => Ok(()),
        1.. => Err(io::Error::from_raw_os_error(rc)),
        _ => Err(io::Error::last_os_error()),
    }
}
