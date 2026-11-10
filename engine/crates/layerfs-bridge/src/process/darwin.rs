//! macOS process facts, which Linux reads from `/proc`: a process's state, group
//! and start time (`proc_pidinfo`, a system call: `ps` is setuid root, which a
//! Seatbelt-sandboxed caller may not execute) and this boot's identity
//! (`sysctl`). Each tool is one run with a fixed path and the C locale, read
//! once; nothing is retried. The mount table and a forced unmount stay with
//! the Sandbox driver (layerfs-project#112 split them from this file).
use super::Stat;
use nix::libc;
use std::{
    ffi::CString,
    io,
    os::unix::ffi::OsStrExt,
    path::Path,
    process::{Command, Stdio},
    sync::OnceLock,
};

const SYSCTL: &str = "/usr/sbin/sysctl";
/// Wait for volume writeback completion (`SYNC_VOLUME_WAIT = 0x02` in macOS
/// SDK `<unistd.h>`; checked against `<sys/mount.h>`, where `MNT_WAIT = 1` is
/// for `getfsstat(2)`/`vfs_sync` while `sync_volume_np(3)` and its flags live
/// in `<unistd.h>`).
const SYNC_VOLUME_WAIT: libc::c_int = 0x02;

/// One tool's standard output, or its failure with what it said.
fn output(program: &str, args: &[&str]) -> io::Result<(bool, String)> {
    let out = Command::new(program)
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .output()?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
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
    let uuid = output(SYSCTL, &["-n", "kern.bootsessionuuid"])
        .ok()
        .and_then(|(ran, text)| if ran { uuid(&text) } else { None });
    let boot = match uuid {
        Some(boot) => boot,
        None => {
            let (_, text) = output(SYSCTL, &["-n", "kern.boottime"])?;
            boottime(&text).ok_or_else(|| invalid("kern.boottime"))?
        }
    };
    Ok(*BOOT.get_or_init(|| boot))
}

fn uuid(text: &str) -> Option<u128> {
    let hex: String = text.trim().chars().filter(|c| *c != '-').collect();
    (hex.len() == 32)
        .then(|| u128::from_str_radix(&hex, 16).ok())
        .flatten()
}

/// `{ sec = 1729500000, usec = 123456 } Tue Oct 21 ...` as sec * 10^6 + usec.
fn boottime(text: &str) -> Option<u128> {
    let field = |name: &str| -> Option<u128> {
        let after = text.split(name).nth(1)?;
        let digits: String = after
            .trim_start_matches([' ', '='])
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    };
    Some(field("sec")? * 1_000_000 + field("usec")?)
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
