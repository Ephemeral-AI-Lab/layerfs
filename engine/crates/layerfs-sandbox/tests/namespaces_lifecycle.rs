//! The namespace backend's Linux lifecycle and the #512 parity checks: a
//! Sandbox's daemon runs in its own mount, PID and IPC namespaces and its own
//! cgroup, commands enter them as a nonroot identity with no capabilities and
//! no-new-privs, another Sandbox's processes and paths are invisible, and a
//! close leaves no process, mount or cgroup behind.
//!
//! It needs root, `/dev/fuse`, cgroup v2 and `user.max_user_namespaces=0`; it
//! starts a stub daemon (a shell script that prints the listener line), so no
//! Store or real daemon is involved. Without those conditions it reports why
//! and returns, and the run is not proof.
#![cfg(target_os = "linux")]
use layerfs_sandbox::{
    backend::namespaces::{NamespaceProvider, Sandbox},
    provider::{LaunchRequest, ProviderLaunchOptions, SandboxRecord},
    CommandIdentity, ExecRequest, RuntimeError,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// The port the stub daemon claims; nothing listens on it.
const PORT: u16 = 40404;

/// One test's private directories: the runtime root, the Store's directory,
/// the stub daemon and a copy of the helper beside it.
struct Fixture {
    base: PathBuf,
    root: PathBuf,
    store: PathBuf,
    daemon: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("layerfs-namespaces-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o755)).unwrap();
        let root = base.join("local");
        let store = base.join("store");
        for directory in [&root, &store] {
            fs::create_dir(directory).unwrap();
            fs::set_permissions(directory, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::write(store.join("sealed.sqlite"), b"stub").unwrap();
        let daemon = base.join("stub-daemon");
        fs::write(
            &daemon,
            format!(
                "#!/bin/sh\necho LAYERFS_DAEMON_LISTEN 127.0.0.1:{PORT}\nexec /bin/sleep 300\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&daemon, fs::Permissions::from_mode(0o755)).unwrap();
        let helper = helper();
        fs::copy(&helper, base.join("layerfs-sandbox-helper")).unwrap();
        Self {
            base,
            root,
            store,
            daemon,
        }
    }

    fn provider(&self) -> Result<NamespaceProvider, RuntimeError> {
        NamespaceProvider::new(&self.root)
    }

    fn request(&self, name: &str) -> LaunchRequest {
        LaunchRequest {
            name: name.to_owned(),
            daemon_executable: self.daemon.clone(),
            daemon_config: b"stub setup".to_vec(),
            labels: vec![("layerfs.role".to_owned(), "sandbox".to_owned())],
            startup_timeout: Duration::from_secs(10),
            options: ProviderLaunchOptions::Namespaces {
                command_identity: CommandIdentity { uid: 501, gid: 20 },
                environment: vec![("LAYERFS_KERNEL".to_owned(), "fuse".to_owned())],
                store: self.store.join("sealed.sqlite"),
            },
        }
    }

    /// The Sandbox directory a launch names.
    fn directory(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

/// The helper this build produced, beside the test binary's own directory.
fn helper() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let target = exe.parent().unwrap().parent().unwrap();
    let helper = target.join("layerfs-sandbox-helper");
    assert!(
        helper.is_file(),
        "build the helper first: cargo build -p layerfs-sandbox --bin layerfs-sandbox-helper"
    );
    helper
}

/// One command in `sandbox`, its streams captured.
fn run(sandbox: &Sandbox, arguments: &[&str], environment: &[&str]) -> (i32, String, String) {
    let directory = sandbox.tmp().display().to_string();
    let request = ExecRequest {
        identity: CommandIdentity { uid: 501, gid: 20 },
        arguments: arguments.iter().map(|value| (*value).to_owned()).collect(),
        environment: environment
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        directory: directory.clone(),
        stdin: false,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let exit = sandbox.run(&request, &mut out, &mut err).unwrap();
    let text = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned();
    (exit.code, text(out), text(err))
}

/// Why this host cannot run the lifecycle test, if it cannot.
fn unavailable() -> Option<String> {
    if !nix::unistd::Uid::effective().is_root() {
        return Some("not root".into());
    }
    match fs::read_to_string("/proc/sys/user/max_user_namespaces") {
        Ok(value) if value.trim() == "0" => {}
        Ok(value) => return Some(format!("user.max_user_namespaces is {}", value.trim())),
        Err(error) => return Some(format!("user.max_user_namespaces: {error}")),
    }
    if !Path::new("/dev/fuse").exists() {
        return Some("no /dev/fuse".into());
    }
    if !Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        return Some("no cgroup v2".into());
    }
    None
}

#[test]
fn a_sandbox_is_its_own_namespaces_and_nothing_of_it_is_left_behind() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces lifecycle: {reason}");
        return;
    }
    let fixture = Fixture::new("lifecycle");
    let provider = fixture.provider().unwrap();
    let mut sandbox = provider.launch(fixture.request("sandbox-1")).unwrap();
    assert_eq!(sandbox.port(), PORT);
    assert!(sandbox.endpoint().is_ok());
    assert!(sandbox.process().unwrap().alive());
    assert!(matches!(
        sandbox.record(),
        Some(SandboxRecord::Namespaces(_))
    ));
    let directory = fixture.directory("sandbox-1");
    assert!(directory.is_dir());

    // identity, capabilities and no-new-privs: a command is uid 501, gid 20,
    // with no effective capability and no way to gain one
    let (code, out, _) = run(&sandbox, &["/bin/sh", "-c", "id -u; id -g"], &[]);
    assert_eq!((code, out.as_str()), (0, "501\n20\n"));
    let (code, out, _err) = run(
        &sandbox,
        &[
            "/bin/grep",
            "-E",
            "^(CapEff|NoNewPrivs|CapPrm):",
            "/proc/self/status",
        ],
        &[],
    );
    assert_eq!(code, 0);
    assert!(out.contains("CapEff:\t0000000000000000"), "{out}");
    assert!(out.contains("CapPrm:\t0000000000000000"), "{out}");
    assert!(out.contains("NoNewPrivs:\t1"), "{out}");

    // user namespaces are blocked, so a command cannot copy the mount
    let (code, _, _) = run(&sandbox, &["/bin/sh", "-c", "unshare -U /bin/true"], &[]);
    assert_ne!(code, 0, "unshare -U succeeded");
    // and without capabilities it cannot mount either
    let (code, _, _) = run(
        &sandbox,
        &["/bin/sh", "-c", "mount -t tmpfs tmpfs /tmp/escape"],
        &[],
    );
    assert_ne!(code, 0, "a command mounted a filesystem");

    // the private root: its own /proc, /dev/fuse and the VM's userland
    let (code, out, _) = run(&sandbox, &["/bin/sh", "-c", "pwd"], &[]);
    assert_eq!(
        (code, out.trim_end().to_owned()),
        (0, sandbox.tmp().display().to_string())
    );
    let (code, out, _) = run(&sandbox, &["/bin/ls", "/dev"], &[]);
    assert_eq!(code, 0);
    assert!(out.lines().any(|line| line == "fuse"), "{out}");
    let (code, out, _) = run(&sandbox, &["/bin/cat", "/proc/1/comm"], &[]);
    assert_eq!(code, 0);
    assert!(!out.trim().is_empty());

    // streams, environment and exit status
    let (code, out, err) = run(
        &sandbox,
        &[
            "/bin/sh",
            "-c",
            "printf out; printf err >&2; printf %s \"$MARK\"; exit 7",
        ],
        &["MARK=value"],
    );
    assert_eq!((code, out.as_str(), err.as_str()), (7, "outvalue", "err"));

    // the Store's directory is bound, the VM's own tree is not
    let (code, _, _) = run(
        &sandbox,
        &["/bin/test", "-d", fixture.store.to_str().unwrap()],
        &[],
    );
    assert_eq!(code, 0);
    let (code, _, _) = run(&sandbox, &["/bin/sh", "-c", "test -e /var/lib"], &[]);
    assert_ne!(code, 0, "the VM's own tree is visible");

    // close: one write ends every process, and nothing is left behind
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
    assert!(!fixture.directory("sandbox-1").exists());
    assert!(!fixture.root.join("sandbox-1.root").exists());
    assert!(!Path::new("/sys/fs/cgroup/layerfs")
        .join(sandbox.identity().as_str())
        .exists());
    let mounts = fs::read_to_string("/proc/self/mountinfo").unwrap();
    assert!(
        !mounts.contains("sandbox-1"),
        "a mount of the Sandbox remains"
    );
}

#[test]
fn two_sandboxes_on_one_store_cannot_see_each_other() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces isolation: {reason}");
        return;
    }
    let fixture = Fixture::new("isolation");
    let provider = fixture.provider().unwrap();
    let first = Arc::new(provider.launch(fixture.request("sandbox-1")).unwrap());
    let second = Arc::new(provider.launch(fixture.request("sandbox-2")).unwrap());
    let other = fixture.directory("sandbox-2").display().to_string();

    // the other Sandbox's directory is not in this mount namespace
    let (code, _, _) = run(&first, &["/bin/test", "-e", &other], &[]);
    assert_ne!(code, 0, "the other Sandbox's directory is visible");
    // and none of its processes is in this PID namespace: the other Sandbox's
    // host PID does not exist here, and neither PID 1 nor PID 2 names it
    let second_pid = second.process().unwrap().pid;
    let (code, _, _) = run(
        &first,
        &["/bin/test", "-e", &format!("/proc/{second_pid}")],
        &[],
    );
    assert_ne!(
        code, 0,
        "the other Sandbox's PID is visible in this PID namespace"
    );
    let (code, out, _) = run(
        &first,
        &[
            "/bin/sh",
            "-c",
            "grep -F -- \"$OTHER\" /proc/1/cmdline /proc/2/cmdline 2>/dev/null || true",
        ],
        &[&format!("OTHER={other}")],
    );
    assert_eq!(code, 0);
    assert!(
        out.is_empty(),
        "the other Sandbox's process is visible: {out}"
    );
    // each cgroup holds only its own Sandbox
    let (code, out, _) = run(&first, &["/bin/cat", "/proc/self/cgroup"], &[]);
    assert_eq!(code, 0);
    assert!(out.contains(first.identity().as_str()), "{out}");

    // both run at the same time, neither disturbed by the other
    let handles = [first.clone(), second.clone()];
    let codes = std::thread::scope(|scope| {
        let started = handles
            .iter()
            .map(|sandbox| {
                let sandbox = sandbox.clone();
                scope.spawn(move || {
                    let (code, out, _) = run(
                        &sandbox,
                        &["/bin/sh", "-c", "/bin/sleep 0.2; printf done"],
                        &[],
                    );
                    (code, out)
                })
            })
            .collect::<Vec<_>>();
        started
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(codes, vec![(0, "done".to_owned()), (0, "done".to_owned())]);
    drop(handles);

    for (mut sandbox, name) in [
        (Arc::try_unwrap(first).ok().unwrap(), "sandbox-1"),
        (Arc::try_unwrap(second).ok().unwrap(), "sandbox-2"),
    ] {
        sandbox.stop(5).unwrap();
        sandbox.delete().unwrap();
        assert!(!fixture.directory(name).exists());
    }
}

#[test]
fn a_record_reopens_the_exact_sandbox_and_nothing_else() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces reopen: {reason}");
        return;
    }
    let fixture = Fixture::new("reopen");
    let provider = fixture.provider().unwrap();
    let sandbox = provider.launch(fixture.request("sandbox-1")).unwrap();
    let id = sandbox.identity();
    let record = sandbox.record().unwrap();
    // what a new process reads: the record alone, with the provider's root
    let reopened_provider = fixture.provider().unwrap();
    let mut reopened = reopened_provider.reopen(id, &record).unwrap();
    assert_eq!(reopened.identity(), id);
    assert_eq!(reopened.port(), PORT);
    let (code, out, _) = run(&reopened, &["/bin/echo", "again"], &[]);
    assert_eq!((code, out.as_str()), (0, "again\n"));
    assert!(!reopened.log_tail(10, 4096).unwrap().is_empty());

    // a Docker record is refused rather than reinterpreting it
    let foreign = SandboxRecord::Docker(layerfs_sandbox::provider::DockerRecord {
        image: "sha256:0".to_owned(),
        store_volume: "volume".to_owned(),
        port: 30421,
        labels: Vec::new(),
    });
    assert!(reopened_provider.reopen(id, &foreign).is_err());

    // a stopped Sandbox admits only its one Delete
    reopened.stop(5).unwrap();
    drop(sandbox);
    let stopped = reopened_provider.reopen_stopped(id, &record).unwrap();
    assert!(stopped.endpoint().is_err());
    let mut stopped = stopped;
    stopped.delete().unwrap();
    assert!(!fixture.directory("sandbox-1").exists());
}

#[test]
fn a_request_is_refused_before_any_effect() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces refusals: {reason}");
        return;
    }
    let fixture = Fixture::new("refusals");
    let provider = fixture.provider().unwrap();
    // a root command identity
    let mut request = fixture.request("sandbox-1");
    request.options = ProviderLaunchOptions::Namespaces {
        command_identity: CommandIdentity { uid: 0, gid: 0 },
        environment: Vec::new(),
        store: fixture.store.join("sealed.sqlite"),
    };
    let failure = provider.launch(request).unwrap_err();
    assert!(matches!(
        failure.cause,
        RuntimeError::Protocol("nonroot command uid/gid")
    ));
    assert!(failure.instance.is_none());
    assert!(!fixture.directory("sandbox-1").exists());
    // the other runtime's options
    let mut request = fixture.request("sandbox-2");
    request.options = ProviderLaunchOptions::Local {
        command_identity: CommandIdentity { uid: 501, gid: 20 },
        environment: Vec::new(),
    };
    let failure = provider.launch(request).unwrap_err();
    assert!(failure.instance.is_none());
    // a Store directory that would expose every Sandbox
    let mut request = fixture.request("sandbox-3");
    request.options = ProviderLaunchOptions::Namespaces {
        command_identity: CommandIdentity { uid: 501, gid: 20 },
        environment: Vec::new(),
        store: fixture.root.join("sealed.sqlite"),
    };
    let failure = provider.launch(request).unwrap_err();
    assert!(matches!(failure.cause, RuntimeError::Protocol(_)));
    assert!(!fixture.directory("sandbox-3").exists());
}

#[test]
fn a_running_command_ends_with_its_sandbox() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces teardown: {reason}");
        return;
    }
    let fixture = Fixture::new("teardown");
    let provider = fixture.provider().unwrap();
    let mut sandbox = provider.launch(fixture.request("sandbox-1")).unwrap();
    // a command that outlives its own process group and keeps running
    let (code, out, _) = run(
        &sandbox,
        &[
            "/bin/sh",
            "-c",
            "setsid /bin/sleep 300 >/dev/null 2>&1 & echo started",
        ],
        &[],
    );
    assert_eq!((code, out.as_str()), (0, "started\n"));
    sandbox.stop(5).unwrap();
    // cgroup.kill ended it: nothing of the Sandbox is left in its group
    let events = fs::read_to_string(
        Path::new("/sys/fs/cgroup/layerfs")
            .join(sandbox.identity().as_str())
            .join("cgroup.events"),
    )
    .unwrap();
    assert!(events.contains("populated 0"), "{events}");
    sandbox.delete().unwrap();
}
