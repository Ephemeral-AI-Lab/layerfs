//! Refuse directory removal only in the calling unmount thread.
use nix::libc;
use std::io;

pub fn deny() {
    let instruction = |code, jt, jf, k| libc::sock_filter { code, jt, jf, k };
    let load = (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16;
    let equal = (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16;
    let ret = (libc::BPF_RET | libc::BPF_K) as u16;
    let mut instructions = vec![instruction(
        load,
        0,
        0,
        std::mem::offset_of!(libc::seccomp_data, nr) as u32,
    )];
    #[cfg(target_arch = "x86_64")]
    instructions.extend([
        instruction(equal, 0, 1, libc::SYS_rmdir as u32),
        instruction(ret, 0, 0, libc::SECCOMP_RET_ERRNO | libc::EACCES as u32),
    ]);
    instructions.extend([
        instruction(equal, 0, 3, libc::SYS_unlinkat as u32),
        instruction(
            load,
            0,
            0,
            (std::mem::offset_of!(libc::seccomp_data, args) + 16) as u32,
        ),
        instruction(equal, 0, 1, libc::AT_REMOVEDIR as u32),
        instruction(ret, 0, 0, libc::SECCOMP_RET_ERRNO | libc::EACCES as u32),
        instruction(ret, 0, 0, libc::SECCOMP_RET_ALLOW),
    ]);
    let program = libc::sock_fprog {
        len: instructions.len() as u16,
        filter: instructions.as_mut_ptr(),
    };
    // SAFETY: live filter storage, calling-thread-only filter, no TSYNC.
    unsafe {
        assert_eq!(libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), 0);
        assert_eq!(
            libc::syscall(
                libc::SYS_seccomp,
                libc::SECCOMP_SET_MODE_FILTER,
                0,
                &program
            ),
            0,
            "install removal denial: {}",
            io::Error::last_os_error()
        );
    }
}
