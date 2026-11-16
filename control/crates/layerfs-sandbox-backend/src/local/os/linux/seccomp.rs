//! The system-call filter every namespaces Sandbox command runs under (D-8,
//! layerfs-project#111): it refuses creation of a user namespace and allows
//! everything else. An unprivileged command can make a mount namespace only
//! inside a user namespace of its own, so under this filter no command can
//! hold its own copy of a Workspace mount outside the daemon's unmount. It
//! is what lets a namespaces Sandbox run where `user.max_user_namespaces`
//! cannot be set to 0 (the docker machine's worker shares Docker's kernel).
//!
//! The semantics are those of the per-container filter the retired docker
//! backend gave the Engine: `unshare` and `clone` with `CLONE_NEWUSER` in
//! their first argument answer `EPERM`; `clone3`, whose flags live in memory
//! a filter cannot read, answers `ENOSYS`, which the C library follows with
//! `clone`. A process of a foreign ABI (x86_64's i386, aarch64's arm) is
//! filtered by its own numbers; an ABI the filter does not know is killed.
//!
//! The only `unsafe` of this crate on Linux: the two `prctl` calls in the
//! command's process, between fork and exec, and the helper's `clone3`
//! ([`OneThread::fork_into`]), which creates the helper inside its
//! Sandbox's cgroup and each command inside its own (layerfs-project#113)
//! without moving a process. No seccomp crate is used.
use nix::{
    fcntl::OFlag,
    libc::{self, sock_filter, sock_fprog},
    sys::stat::Mode,
};
use std::{io, os::unix::process::CommandExt, process::Command};

/// Offsets into the kernel's `struct seccomp_data`: the call's number, its
/// ABI and the low 32 bits of its first argument (both ABIs here are
/// little-endian).
const NR: u32 = 0;
const ARCH: u32 = 4;
const ARG0_LOW: u32 = 16;
/// `CLONE_NEWUSER`, which fits in the first argument's low word.
const NEWUSER: u32 = libc::CLONE_NEWUSER as u32;
/// x86_64's x32 ABI: the native numbers with this bit set.
#[cfg(target_arch = "x86_64")]
const X32: u32 = 0x4000_0000;

/// One ABI's audit architecture and its numbers for the three calls.
struct Abi {
    arch: u32,
    unshare: u32,
    clone: u32,
    clone3: u32,
}

#[cfg(target_arch = "x86_64")]
const ABIS: [Abi; 2] = [
    // AUDIT_ARCH_X86_64
    Abi {
        arch: 0xC000_003E,
        unshare: libc::SYS_unshare as u32,
        clone: libc::SYS_clone as u32,
        clone3: libc::SYS_clone3 as u32,
    },
    // AUDIT_ARCH_I386 (arch/x86/entry/syscalls/syscall_32.tbl)
    Abi {
        arch: 0x4000_0003,
        unshare: 310,
        clone: 120,
        clone3: 435,
    },
];

#[cfg(target_arch = "aarch64")]
const ABIS: [Abi; 2] = [
    // AUDIT_ARCH_AARCH64
    Abi {
        arch: 0xC000_00B7,
        unshare: libc::SYS_unshare as u32,
        clone: libc::SYS_clone as u32,
        clone3: libc::SYS_clone3 as u32,
    },
    // AUDIT_ARCH_ARM, the EABI compat table (arch/arm/tools/syscall.tbl)
    Abi {
        arch: 0x4000_0028,
        unshare: 337,
        clone: 120,
        clone3: 435,
    },
];

/// Another architecture: no ABI is known, so [`deny_user_namespaces`] fails
/// every command rather than run it unfiltered.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const ABIS: [Abi; 0] = [];

fn statement(code: u32, k: u32) -> sock_filter {
    jump(code, k, 0, 0)
}

fn jump(code: u32, k: u32, jt: u8, jf: u8) -> sock_filter {
    sock_filter {
        code: code as u16,
        jt,
        jf,
        k,
    }
}

const LOAD: u32 = libc::BPF_LD | libc::BPF_W | libc::BPF_ABS;
const EQUALS: u32 = libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K;
const RETURN: u32 = libc::BPF_RET | libc::BPF_K;

/// The checks for one ABI, entered with its number loaded; every path ends
/// in a return.
fn checks(abi: &Abi, native: bool) -> Vec<sock_filter> {
    let mut block = vec![statement(LOAD, NR)];
    #[cfg(target_arch = "x86_64")]
    if native {
        // x32 calls carry the native numbers with the x32 bit set
        block.push(statement(libc::BPF_ALU | libc::BPF_AND | libc::BPF_K, !X32));
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = native;
    block.extend([
        jump(EQUALS, abi.clone3, 0, 1),
        statement(RETURN, libc::SECCOMP_RET_ERRNO | libc::ENOSYS as u32),
        jump(EQUALS, abi.unshare, 1, 0),
        jump(EQUALS, abi.clone, 0, 3),
        statement(LOAD, ARG0_LOW),
        jump(libc::BPF_JMP | libc::BPF_JSET | libc::BPF_K, NEWUSER, 0, 1),
        statement(RETURN, libc::SECCOMP_RET_ERRNO | libc::EPERM as u32),
        statement(RETURN, libc::SECCOMP_RET_ALLOW),
    ]);
    block
}

/// The whole filter: each known ABI's checks behind a test of the call's
/// architecture, then a kill for any other.
fn program() -> Vec<sock_filter> {
    let mut program = vec![statement(LOAD, ARCH)];
    for (index, abi) in ABIS.iter().enumerate() {
        let block = checks(abi, index == 0);
        // the architecture stays loaded when a block is jumped over
        program.push(jump(EQUALS, abi.arch, 0, block.len() as u8));
        program.extend(block);
    }
    program.push(statement(RETURN, libc::SECCOMP_RET_KILL_PROCESS));
    program
}

/// `CLONE_INTO_CGROUP` (linux/sched.h, Linux 5.7): `libc` declares it with
/// a type it overflows.
const CLONE_INTO_CGROUP: u64 = 0x2_0000_0000;

/// A process with one thread, which may therefore fork without exec: the
/// namespaces helper (layerfs-project#113). It keeps its PID namespace's
/// `/proc` open and checks `<pid>/task` there before each fork, so the
/// check reads the calling process's own threads in a forked child too, and
/// after it entered a Sandbox's mount namespace, where `/proc/self` no
/// longer resolves.
pub struct OneThread(std::os::fd::OwnedFd);

impl OneThread {
    /// This process, refused unless it has one thread.
    pub fn open() -> io::Result<Self> {
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC;
        let process = Self(nix::fcntl::open("/proc", flags, Mode::empty())?);
        process.check()?;
        Ok(process)
    }

    fn check(&self) -> io::Result<()> {
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC;
        let task = format!("{}/task", nix::unistd::getpid());
        let mut task = nix::dir::Dir::openat(&self.0, task.as_str(), flags, Mode::empty())?;
        let mut threads = 0;
        for entry in task.iter() {
            if !matches!(entry?.file_name().to_bytes(), b"." | b"..") {
                threads += 1;
            }
        }
        match threads {
            1 => Ok(()),
            _ => Err(io::Error::other("forking needs a process with one thread")),
        }
    }

    /// Forks this process, its child created inside the cgroup whose
    /// directory `group` is open (`clone3` with `CLONE_INTO_CGROUP`), or
    /// in this process's own with none. Nothing moves between groups, so
    /// the kernel takes its process-migration lock only as a reader and
    /// waits for no RCU grace period, which a write of a pid to
    /// `cgroup.procs` does (5–28 ms measured per write after an idle spell).
    /// The caller gets the child's pid; the child gets `None`.
    pub fn fork_into(&self, group: Option<std::os::fd::BorrowedFd<'_>>) -> io::Result<Option<u32>> {
        use std::os::fd::AsRawFd;
        self.check()?;
        let (flags, cgroup) = match group {
            Some(group) => (CLONE_INTO_CGROUP, group.as_raw_fd() as u64),
            None => (0, 0),
        };
        let mut arguments = libc::clone_args {
            flags,
            pidfd: 0,
            child_tid: 0,
            parent_tid: 0,
            exit_signal: libc::SIGCHLD as u64,
            stack: 0,
            stack_size: 0,
            tls: 0,
            set_tid: 0,
            set_tid_size: 0,
            cgroup,
        };
        let size = std::mem::size_of::<libc::clone_args>();
        // SAFETY: with no CLONE_VM and no stack this is a fork: the child
        // gets its own copy of the address space and continues here. The
        // process has one thread (checked just above, and only that thread
        // could have started another since), so no lock or allocator state
        // is held by a thread the child lacks. No fork handler runs; the C
        // library's guard locks against other threads, of which there are
        // none. `arguments` outlives the call, which reads it once.
        #[allow(unsafe_code)]
        let pid = unsafe { libc::syscall(libc::SYS_clone3, &mut arguments, size) };
        match pid {
            -1 => Err(io::Error::last_os_error()),
            0 => Ok(None),
            pid => u32::try_from(pid)
                .map(Some)
                .map_err(|_| io::Error::other("clone3 child process id width")),
        }
    }
}

/// Makes `command` run under the filter: in its process, after everything
/// `Command` sets up there (identity, directory, process group) and just
/// before exec, it empties the signal mask (a forked child inherits its
/// parent's), sets no-new-privs (the kernel's condition for an
/// unprivileged filter) and installs the filter. A failure of any fails
/// the spawn, so the command never runs without it.
pub fn deny_user_namespaces(command: &mut Command) {
    let filter = program();
    let known = cfg!(any(target_arch = "x86_64", target_arch = "aarch64"));
    // SAFETY: the hook runs in the forked child before exec. It allocates
    // nothing and takes no lock: it reads the filter built here and makes
    // one `sigprocmask` and two `prctl` system calls, which are
    // async-signal-safe.
    #[allow(unsafe_code)]
    unsafe {
        command.pre_exec(move || match known {
            true => unblocked().and_then(|()| install(&filter)),
            false => Err(io::ErrorKind::Unsupported.into()),
        });
    }
}

/// Empties this thread's signal mask.
#[allow(unsafe_code)]
fn unblocked() -> io::Result<()> {
    let mut set = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
    // SAFETY: `sigemptyset` initializes `set` before it is read, and
    // `pthread_sigmask` only reads it.
    let masked = unsafe {
        libc::sigemptyset(set.as_mut_ptr());
        libc::pthread_sigmask(libc::SIG_SETMASK, set.as_ptr(), std::ptr::null_mut())
    };
    match masked {
        0 => Ok(()),
        error => Err(io::Error::from_raw_os_error(error)),
    }
}

/// `PR_SET_NO_NEW_PRIVS`, then `PR_SET_SECCOMP` with `filter`, in this process.
#[allow(unsafe_code)]
fn install(filter: &[sock_filter]) -> io::Result<()> {
    let program = sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_ptr().cast_mut(),
    };
    let one: libc::c_ulong = 1;
    let zero: libc::c_ulong = 0;
    // SAFETY: plain system calls; `program` and the filter it points at
    // outlive the second call, which copies the filter into the kernel.
    let set = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, one, zero, zero, zero) };
    if set != 0 {
        return Err(io::Error::last_os_error());
    }
    let mode = libc::SECCOMP_MODE_FILTER as libc::c_ulong;
    let installed =
        unsafe { libc::prctl(libc::PR_SET_SECCOMP, mode, &program as *const sock_fprog) };
    if installed != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
