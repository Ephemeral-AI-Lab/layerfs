//! The user-namespace block of a namespaces Sandbox's commands (D-8,
//! layerfs-project#111): every command runs under the filter
//! `local::deny_user_namespaces` installs, so it cannot create a user
//! namespace and keep its own copy of a Workspace mount, whatever the host's
//! `user.max_user_namespaces` says. Nothing here needs root or a Sandbox:
//! the test runs its own binary again as the filtered command (whose test
//! harness starts threads, which `clone3`'s `ENOSYS` sends to `clone`), and
//! that command reports how many filters it runs under and what each call
//! answered.
#[cfg(target_os = "linux")]
mod linux {
    use nix::libc;
    use std::process::Command;

    /// Set in the child: run [`probe`] instead of the test.
    const CHILD: &str = "LAYERFS_TEST_SECCOMP_CHILD";

    /// `clone3` with no argument structure, and the errno it answered.
    fn bare_clone3() -> (libc::c_long, Option<i32>) {
        // SAFETY: a null, zero-sized argument the kernel refuses (EFAULT or
        // EINVAL) or the filter refuses first (ENOSYS); nothing is created
        #[allow(unsafe_code)]
        let answered = unsafe { libc::syscall(libc::SYS_clone3, 0usize, 0usize) };
        (answered, std::io::Error::last_os_error().raw_os_error())
    }

    /// How many seccomp filters this process runs under (Linux 5.9 and
    /// later report it).
    fn filters() -> u32 {
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        (status.lines())
            .find_map(|line| line.strip_prefix("Seccomp_filters:"))
            .and_then(|count| count.trim().parse().ok())
            .expect("this kernel reports Seccomp_filters")
    }

    /// The child's part: the filters it runs under, then each call's errno,
    /// `0` for a call that succeeded.
    fn probe() {
        let newuser = match nix::sched::unshare(nix::sched::CloneFlags::CLONE_NEWUSER) {
            Ok(()) => 0,
            Err(errno) => errno as i32,
        };
        let clone3 = match bare_clone3() {
            (0, _) => 0,
            (_, errno) => errno.unwrap_or(-1),
        };
        println!("filters={} newuser={newuser} clone3={clone3}", filters());
    }

    #[test]
    fn a_filtered_command_cannot_create_a_user_namespace() {
        if std::env::var_os(CHILD).is_some() {
            probe();
            return;
        }
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "linux::a_filtered_command_cannot_create_a_user_namespace",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1");
        layerfs_sandbox_backend::local::deny_user_namespaces(&mut command);
        let out = command.output().unwrap();
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{out:?}");
        // the harness may print the test's name on the same line first
        let line = said
            .lines()
            .find_map(|line| line.find("filters=").map(|at| &line[at..]))
            .unwrap_or_else(|| panic!("the child printed no probe: {said}"));
        // one filter more than this process runs under (a container's own
        // filter, if any, gives the same answers; this one is proven there)
        assert_eq!(
            line,
            format!(
                "filters={} newuser={} clone3={}",
                filters() + 1,
                libc::EPERM,
                libc::ENOSYS
            ),
            "{said}"
        );
    }
}

#[cfg(not(target_os = "linux"))]
#[test]
fn the_user_namespace_filter_is_linux_only() {
    eprintln!("SKIPPED namespaces user-namespace filter: seccomp is Linux only");
}
