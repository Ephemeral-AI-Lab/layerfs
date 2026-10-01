//! Linux supervisor/command trust boundary; privileged container operators are trusted.
use std::{
    io,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::Path,
    process::Command,
};
pub const UID: u32 = 65534;
pub const GID: u32 = 65534;
#[repr(C)]
struct Header {
    version: u32,
    pid: i32,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct Capabilities {
    effective: u32,
    permitted: u32,
    inheritable: u32,
}
// Linux UAPI capability.h v3: two 32-bit capability sets, current-process pid0.
const CAPABILITY_VERSION: u32 = 0x2008_0522;
pub fn supervisor() -> io::Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(io::Error::from_raw_os_error(libc::ENOTSUP));
    }
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let path = Path::new("/layerfs/backing");
    std::fs::create_dir_all(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}
pub fn configure(command: &mut Command) {
    command
        .env_clear()
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("HOME", "/tmp")
        .env("TMPDIR", "/tmp");
    // Only async-signal-safe syscalls between fork and exec; no allocations/locks.
    unsafe {
        command.pre_exec(|| {
            if libc::setgroups(0, std::ptr::null()) != 0
                || libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::prctl(
                    libc::PR_CAP_AMBIENT,
                    libc::PR_CAP_AMBIENT_CLEAR_ALL,
                    0,
                    0,
                    0,
                ) != 0
                || libc::setresgid(GID, GID, GID) != 0
                || libc::setresuid(UID, UID, UID) != 0
            {
                return Err(io::Error::last_os_error());
            }
            let header = Header {
                version: CAPABILITY_VERSION,
                pid: 0,
            };
            let data = [Capabilities {
                effective: 0,
                permitted: 0,
                inheritable: 0,
            }; 2];
            if libc::syscall(libc::SYS_capset, &header as *const Header, data.as_ptr()) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}
