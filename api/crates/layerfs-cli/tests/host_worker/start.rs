#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

use crate::fixture;
use crate::proof;

use fixture::Fixture;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CLI: &str = env!("CARGO_BIN_EXE_layerfs");

/// Waits for a worker this test did not spawn to finish its drain.
fn drained(home: &Fixture) {
    home.drain();
    proof::wait_for("the drained worker to release its lock", || {
        home.paths.try_hold().unwrap()
    });
}

/// The environment of a process of this user, `NAME=value` per entry.
fn environment(pid: i64) -> String {
    #[cfg(target_os = "linux")]
    {
        String::from_utf8_lossy(&std::fs::read(format!("/proc/{pid}/environ")).unwrap())
            .replace('\0', "\n")
    }
    #[cfg(target_os = "macos")]
    {
        // `ps -E` prints a process's environment after its arguments.
        let out = Command::new("/bin/ps")
            .args(["-E", "-ww", "-o", "command=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).replace(' ', "\n")
    }
}

/// The working directory of a process of this user.
fn directory(pid: i64) -> std::path::PathBuf {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/cwd")).unwrap()
    }
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("/usr/sbin/lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().find_map(|line| line.strip_prefix('n'));
        std::path::PathBuf::from(line.expect("lsof reports the working directory"))
    }
}

#[test]
fn aliased_homes_have_one_owner_and_keep_their_mode() {
    let mut home = Fixture::new();
    let alias = home.alias();
    let mut worker = home.start();
    let ready = home.ping();
    assert_eq!(ready.get_int("pid"), Some(worker.pid()));
    assert!(ready.get_text("pid_start").is_some());
    assert!(ready.get_text("pid_boot").is_some());
    let compiled = concat!(
        env!("CARGO_PKG_VERSION"),
        "/",
        env!("LAYERFS_BUILD_COMMIT"),
        "/",
        env!("LAYERFS_BUILD_DIRTY"),
        "@"
    );
    assert!(ready.get_text("build").unwrap().starts_with(compiled));
    let generation = ready.get_text("generation");

    let mut second = Fixture::spawn(&alias);
    assert_eq!(second.wait(Duration::from_secs(5)).code(), Some(125));
    let next = home.ping();
    assert_eq!(next.get_text("generation"), generation);
    assert_eq!(next.get_int("pid"), Some(worker.pid()));
    assert_eq!(
        home.paths.home.metadata().unwrap().permissions().mode() & 0o777,
        0o700
    );
    for endpoint in [&home.paths.socket, &home.paths.control] {
        assert_eq!(
            endpoint.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(!home.paths.home.join("worker-incarnation.json").exists());
    assert!(!home.paths.home.join("log/sweep.jsonl").exists());
    home.drain();
    assert!(worker.wait(Duration::from_secs(5)).success());
    assert!(!home.paths.socket.exists());
    assert!(!home.paths.control.exists());
}

#[test]
fn a_new_owner_recovers_socket_paths_after_a_worker_dies() {
    let home = Fixture::new();
    let mut first = home.start();
    let first_ping = home.ping();
    first.kill();
    assert!(home.paths.socket.exists());
    assert!(home.paths.control.exists());

    let mut second = home.start();
    let second_ping = home.ping();
    assert_ne!(
        first_ping.get_text("generation"),
        second_ping.get_text("generation")
    );
    assert_eq!(second_ping.get_int("pid"), Some(second.pid()));
    home.drain();
    assert!(second.wait(Duration::from_secs(5)).success());
}

#[test]
fn a_socket_left_without_its_private_mode_is_replaced_by_the_next_owner() {
    // A worker killed between bind and chmod leaves both endpoints with the
    // umask mode and no lifetime lock holder.
    let home = proof::create();
    for endpoint in [&home.paths.socket, &home.paths.control] {
        drop(UnixListener::bind(endpoint).unwrap());
        std::fs::set_permissions(endpoint, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let began = Instant::now();
    proof::assert_reads(proof::spawn_reads(&home, 1, false));
    assert!(
        began.elapsed() < Duration::from_secs(8),
        "the stale endpoint delayed the call to {:?}",
        began.elapsed()
    );
    for endpoint in [&home.paths.socket, &home.paths.control] {
        assert_eq!(
            endpoint.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    drained(&home);
}

#[test]
fn a_spawned_worker_keeps_no_caller_secret_or_directory() {
    let home = proof::create();
    let caller = home.paths.home.join("proof-input");
    let out = Command::new(CLI)
        .args(["file", "read", "shop/api", "main", "/hello"])
        .env("LAYERFS_ROOT", crate::root::root_of(&home.paths.home))
        .env("PGPASSWORD", "caller-secret-819")
        .env("DSH_TOKEN", "caller-token-819")
        .env("LAYERFS_LOGS_TAG", "caller-tag-819")
        .current_dir(&caller)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let pid = home.ping().get_int("pid").unwrap();
    let environment = environment(pid);
    for secret in ["caller-secret-819", "caller-token-819", "caller-tag-819"] {
        assert!(
            !environment.contains(secret),
            "worker inherited {secret}: {environment}"
        );
    }
    assert!(environment.contains("LAYERFS_ROOT="), "{environment}");
    assert_eq!(directory(pid), std::path::PathBuf::from("/"));
    drained(&home);
}
