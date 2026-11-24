//! The namespace backend's Linux lifecycle and the #512 parity checks: a
//! Sandbox's daemon runs in its own mount, PID and IPC namespaces and its own
//! cgroup, on a root that is the Sandbox image with its own writable layer,
//! commands enter them as a nonroot identity with no capabilities and
//! no-new-privs, another Sandbox's processes, paths and writes are invisible,
//! and a close leaves no process, mount, writable layer or cgroup behind.
//!
//! It needs root, `/dev/fuse`, cgroup v2 and `user.max_user_namespaces=0`; it
//! starts a stub daemon (a shell script that prints the listener line), so no
//! Store or real daemon is involved. The image is this host's own userland in
//! a docker-archive, so the stub and the tools the checks run exist in it and
//! nowhere else in the Sandbox. Without those conditions it reports why and
//! returns, and the run is not proof.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
mod support {
    pub mod image;
}
use layerfs_sandbox_backend::{
    namespaces::{NamespaceProvider, Sandbox},
    stop::{cancel, Cancelled, ExecSlot, Stop},
    Bind, CommandIdentity, ExecRequest, Image, LaunchRequest, ProviderLaunchOptions, RuntimeError,
    SandboxRecord,
};
use nix::sys::signal::Signal;
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use support::image::{archive, hex, unpacked_at, Tar, DIGEST};

/// The port the stub daemon claims; nothing listens on it.
const PORT: u16 = 40404;
/// The port every Sandbox here closes to its commands, as a worker closes
/// its own listening port.
const CLOSED: u16 = 7070;

/// The programs the checks below run: each is copied into the image from this
/// host, with the libraries `ldd` names for it.
const PROGRAMS: [&str; 13] = [
    "/bin/sh",
    "/bin/ls",
    "/bin/cat",
    "/bin/grep",
    "/bin/test",
    "/bin/echo",
    "/bin/sleep",
    "/bin/setsid",
    "/bin/mkdir",
    "/bin/mount",
    "/bin/umount",
    "/bin/unshare",
    "/bin/id",
];

/// One test's private directories: the runtime root, the Store's directory,
/// the stub daemon, a copy of the helper beside it, and the image archive.
struct Fixture {
    base: PathBuf,
    root: PathBuf,
    store: PathBuf,
    daemon: PathBuf,
    archive: PathBuf,
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
        let archive = image(&base);
        Self {
            base,
            root,
            store,
            daemon,
            archive,
        }
    }

    fn provider(&self) -> Result<NamespaceProvider, RuntimeError> {
        NamespaceProvider::new(&self.root)
    }

    fn image(&self) -> Image {
        Image::parse(DIGEST).unwrap()
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
                environment: Vec::new(),
                image: self.image(),
                archive: self.archive.clone(),
                // the machine's one table is rewritten at every start: the
                // Sandboxes of one process all close the same port, as a
                // worker's do (`CLOSED`)
                closed_ports: vec![CLOSED],
            },
        }
    }

    /// The bind of the fixture's sqlite Store file (#535 §6.3).
    fn store_bind(&self) -> Bind {
        Bind::store(&self.store.join("sealed.sqlite")).unwrap()
    }

    /// Launches `name` with the Store's bind.
    fn launch(
        &self,
        provider: &NamespaceProvider,
        name: &str,
    ) -> Result<Sandbox, Box<layerfs_sandbox_backend::LaunchFailure>> {
        provider.launch_with_binds(self.request(name), vec![self.store_bind()])
    }

    /// The Sandbox directory a launch names.
    fn directory(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("LAYERFS_H38_KEEP").is_some() {
            eprintln!("kept {}", self.base.display());
            return;
        }
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
        "build the helper first: cargo build -p layerfs-sandbox-backend --bin layerfs-sandbox-helper"
    );
    helper
}

/// One docker-archive holding this host's own userland for the programs above,
/// plus a marker file a command can read and `/var/tmp`, the one directory a
/// command may write to.
fn image(base: &Path) -> PathBuf {
    let mut tar = Tar::new()
        .directory("etc/", 0o755)
        .file("etc/layerfs-image", 0o644, b"layerfs image\n")
        .directory("var/", 0o755)
        .directory("var/tmp/", 0o1777);
    let mut copied = BTreeSet::new();
    for program in PROGRAMS {
        let Ok(source) = std::fs::canonicalize(program) else {
            continue;
        };
        tar = copy(tar, &mut copied, &source, Path::new(program));
        for library in libraries(&source) {
            tar = copy(tar, &mut copied, &library, &library);
        }
    }
    assert!(
        copied.contains(Path::new("/bin/sh")) && copied.contains(Path::new("/bin/ls")),
        "the host has no userland to build an image from"
    );
    archive(base, "image", DIGEST, vec![tar.finish()])
}

/// The same bytes at `path` in the image, with the source's mode.
fn copy(mut tar: Tar, copied: &mut BTreeSet<PathBuf>, source: &Path, path: &Path) -> Tar {
    if !copied.insert(path.to_path_buf()) {
        return tar;
    }
    let Ok(body) = fs::read(source) else {
        return tar;
    };
    let mode = fs::metadata(source).map_or(0o755, |metadata| metadata.permissions().mode() & 0o777);
    // inside the image the path is relative: the tar carries `/bin/sh` as
    // `bin/sh`, and the root's own `/` is the image itself
    let text = path.to_str().expect("a UTF-8 host path");
    tar = tar.file(text.trim_start_matches('/'), mode, &body);
    tar
}

/// The libraries `ldd` names for `program`: every absolute path it prints.
fn libraries(program: &Path) -> Vec<PathBuf> {
    let Ok(output) = std::process::Command::new("ldd").arg(program).output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut libraries = Vec::new();
    for line in text.lines() {
        if let Some(path) = (line.split_whitespace()).find(|token| token.starts_with('/')) {
            libraries.push(PathBuf::from(path));
        }
    }
    libraries
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
        stdin: None,
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
    let mut sandbox = fixture.launch(&provider, "sandbox-1").unwrap();
    assert_eq!(sandbox.port(), PORT);
    assert!(sandbox.endpoint().is_ok());
    assert!(sandbox.process().unwrap().alive());
    assert!(matches!(
        sandbox.record(),
        Some(SandboxRecord::Namespaces(_))
    ));
    let directory = fixture.directory("sandbox-1");
    assert!(directory.is_dir());
    // its root is the image, with this Sandbox's own empty writable layer
    assert!(directory.join("overlay/upper").is_dir());
    assert_eq!(
        fs::read_to_string(
            fixture
                .root
                .join("images")
                .join(format!("{}.2", hex(DIGEST)))
                .join("etc/layerfs-image")
        )
        .unwrap(),
        "layerfs image\n"
    );

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

    // the root is the image: its own /proc, /dev/fuse, userland and marker
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
    let (code, out, _) = run(&sandbox, &["/bin/cat", "/etc/layerfs-image"], &[]);
    assert_eq!((code, out.as_str()), (0, "layerfs image\n"));
    // and it is read-only: a command cannot write the image, only its own
    // writable layer above it
    let (code, _, _) = run(
        &sandbox,
        &["/bin/sh", "-c", "echo changed > /etc/layerfs-image"],
        &[],
    );
    assert_ne!(code, 0, "a command wrote the image");
    let (code, _, _) = run(&sandbox, &["/bin/test", "-w", "/etc"], &[]);
    assert_ne!(code, 0, "a command may write the image's directory");
    let (code, _, _) = run(&sandbox, &["/bin/test", "-w", "/var/tmp"], &[]);
    assert_eq!(code, 0, "a command may not write its own writable layer");
    let (code, _, _) = run(
        &sandbox,
        &[
            "/bin/sh",
            "-c",
            "/bin/mkdir /var/tmp/written && echo x > /var/tmp/written/x",
        ],
        &[],
    );
    assert_eq!(code, 0, "a command could not write its own layer");

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
    // the image stays: it is the cache every other Sandbox of it reuses
    assert!(fixture
        .root
        .join("images")
        .join(format!("{}.2", hex(DIGEST)))
        .join("etc/layerfs-image")
        .is_file());
}

#[test]
fn two_sandboxes_on_one_store_cannot_see_each_other() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces isolation: {reason}");
        return;
    }
    let fixture = Fixture::new("isolation");
    let provider = fixture.provider().unwrap();
    let first = Arc::new(fixture.launch(&provider, "sandbox-1").unwrap());
    let second = Arc::new(fixture.launch(&provider, "sandbox-2").unwrap());
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

    // one image, two writable layers: what one Sandbox writes the other never
    // sees, and each keeps its own
    let (code, _, _) = run(
        &first,
        &["/bin/sh", "-c", "echo first > /var/tmp/mine"],
        &[],
    );
    assert_eq!(code, 0);
    let (code, _, _) = run(
        &second,
        &["/bin/sh", "-c", "echo second > /var/tmp/mine"],
        &[],
    );
    assert_eq!(code, 0);
    let (code, out, _) = run(&first, &["/bin/cat", "/var/tmp/mine"], &[]);
    assert_eq!((code, out.as_str()), (0, "first\n"));
    let (code, out, _) = run(&second, &["/bin/cat", "/var/tmp/mine"], &[]);
    assert_eq!((code, out.as_str()), (0, "second\n"));
    let (code, _, _) = run(&first, &["/bin/test", "-e", "/var/tmp/only-second"], &[]);
    assert_ne!(code, 0);
    // both still see the image itself, unchanged
    let (code, out, _) = run(&first, &["/bin/cat", "/etc/layerfs-image"], &[]);
    assert_eq!((code, out.as_str()), (0, "layerfs image\n"));
    // and the writes live in each Sandbox's own upper layer, never in the
    // shared image
    let image_root = unpacked_at(&fixture.root.join("images"), DIGEST);
    assert!(
        fs::read_to_string(image_root.join("var/tmp/mine")).is_err(),
        "a Sandbox's write reached the shared image"
    );
    assert_eq!(
        fs::read_to_string(
            fixture
                .directory("sandbox-1")
                .join("overlay/upper/var/tmp/mine")
        )
        .unwrap(),
        "first\n"
    );
    assert_eq!(
        fs::read_to_string(
            fixture
                .directory("sandbox-2")
                .join("overlay/upper/var/tmp/mine")
        )
        .unwrap(),
        "second\n"
    );

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
    let sandbox = fixture.launch(&provider, "sandbox-1").unwrap();
    let id = sandbox.identity();
    let record = sandbox.record().unwrap();
    // the record names the image its root is an overlay of
    assert_eq!(
        record.namespaces().map(|record| record.image().as_str()),
        Some(DIGEST)
    );
    assert_eq!(
        record
            .namespaces()
            .map(|record| record.local().name.as_str()),
        Some("sandbox-1")
    );
    // the Sandbox keeps its own copy of the helper beside its daemon, which a
    // reopening process runs when it has no LAYERFS_SANDBOX_HELPER and its
    // executable has no helper beside it (an in-process SDK's `node`)
    assert_eq!(
        fs::read(
            fixture
                .directory("sandbox-1")
                .join("bin/layerfs-sandbox-helper")
        )
        .unwrap(),
        fs::read(helper()).unwrap()
    );
    // what a new process reads: the record alone, with the provider's root,
    // and it finds the same image under the digest the record keeps
    let reopened_provider = fixture.provider().unwrap();
    let mut reopened = reopened_provider.reopen(id, &record).unwrap();
    assert_eq!(reopened.identity(), id);
    assert_eq!(reopened.port(), PORT);
    assert_eq!(reopened.record().unwrap(), record);
    assert_eq!(reopened.image().image().as_str(), DIGEST);
    // on the root its create unpacked and its spec names: the `.2` layout
    assert_eq!(
        reopened.image().directory(),
        unpacked_at(&fixture.root.join("images"), DIGEST).as_path()
    );
    let (code, out, _) = run(&reopened, &["/bin/echo", "again"], &[]);
    assert_eq!((code, out.as_str()), (0, "again\n"));
    assert!(!reopened.log_tail(10, 4096).unwrap().is_empty());

    // a record whose image is gone is refused, not given another root
    let mut without_image = record.clone();
    if let SandboxRecord::Namespaces(record) = &mut without_image {
        record.image = Image::parse(&format!("sha256:{}", "e".repeat(64))).unwrap();
    }
    let refused = reopened_provider
        .reopen(id, &without_image)
        .expect_err("a record without its image was reopened");
    assert!(
        format!("{:?}", refused.cause).contains("cannot be found"),
        "{:?}",
        refused.cause
    );

    // a native record of the same facts is refused rather than reinterpreted
    let SandboxRecord::Namespaces(namespace) = &record else {
        panic!("a namespaces Sandbox wrote {record:?}");
    };
    let foreign = SandboxRecord::Local(namespace.local.clone());
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
        image: fixture.image(),
        archive: fixture.archive.clone(),
        closed_ports: Vec::new(),
    };
    let failure = provider
        .launch_with_binds(request, vec![fixture.store_bind()])
        .unwrap_err();
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
    let failure = provider
        .launch_with_binds(request, vec![fixture.store_bind()])
        .unwrap_err();
    assert!(failure.instance.is_none());
    // a Store directory that would expose every Sandbox
    let mut request = fixture.request("sandbox-3");
    request.options = ProviderLaunchOptions::Namespaces {
        command_identity: CommandIdentity { uid: 501, gid: 20 },
        environment: Vec::new(),
        image: fixture.image(),
        archive: fixture.archive.clone(),
        closed_ports: Vec::new(),
    };
    let exposed = Bind::store(&fixture.root.join("sealed.sqlite")).unwrap();
    let failure = provider
        .launch_with_binds(request, vec![exposed])
        .unwrap_err();
    assert!(matches!(failure.cause, RuntimeError::Protocol(_)));
    assert!(!fixture.directory("sandbox-3").exists());
    // a bundle without the image archive
    let mut request = fixture.request("sandbox-4");
    request.options = ProviderLaunchOptions::Namespaces {
        command_identity: CommandIdentity { uid: 501, gid: 20 },
        environment: Vec::new(),
        image: fixture.image(),
        archive: fixture.base.join("no-such-image.tar"),
        closed_ports: Vec::new(),
    };
    let failure = provider
        .launch_with_binds(request, vec![fixture.store_bind()])
        .unwrap_err();
    assert!(failure.instance.is_none(), "{:?}", failure.cause);
    assert!(format!("{:?}", failure.cause).contains("no-such-image.tar"));
    assert!(!fixture.directory("sandbox-4").exists());
    // an image archive that holds another image than the deployment named
    let mut request = fixture.request("sandbox-5");
    let other = Image::parse(&format!("sha256:{}", "d".repeat(64))).unwrap();
    request.options = ProviderLaunchOptions::Namespaces {
        command_identity: CommandIdentity { uid: 501, gid: 20 },
        environment: Vec::new(),
        image: other.clone(),
        archive: fixture.archive.clone(),
        closed_ports: Vec::new(),
    };
    let failure = provider
        .launch_with_binds(request, vec![fixture.store_bind()])
        .unwrap_err();
    assert!(failure.instance.is_none(), "{:?}", failure.cause);
    let message = format!("{:?}", failure.cause);
    assert!(message.contains(other.hex()), "{message}");
    assert!(message.contains(hex(DIGEST)), "{message}");
    assert!(!fixture.directory("sandbox-5").exists());
}

/// #535 §6.3: binds are all a namespaces root takes. A PostgreSQL Store's
/// Sandbox binds nothing (its Store's directory is not there); another
/// bind is seen at its guest path with its mode.
#[test]
fn binds_replace_the_store_file() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces binds: {reason}");
        return;
    }
    let fixture = Fixture::new("binds");
    let provider = fixture.provider().unwrap();
    // no bind: the Store's directory is not in the Sandbox
    let mut sandbox = provider
        .launch_with_binds(fixture.request("sandbox-1"), Vec::new())
        .unwrap();
    let store = fixture.store.to_str().unwrap();
    let (code, _, _) = run(&sandbox, &["/bin/test", "-e", store], &[]);
    assert_ne!(code, 0, "an unbound Store directory is visible");
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
    // a read-only bind at another guest path
    let bind = Bind {
        host: fixture.store.clone(),
        guest: PathBuf::from("/srv/reference"),
        read_write: false,
    };
    let mut sandbox = provider
        .launch_with_binds(fixture.request("sandbox-2"), vec![bind])
        .unwrap();
    let (code, out, _) = run(&sandbox, &["/bin/cat", "/srv/reference/sealed.sqlite"], &[]);
    assert_eq!((code, out.as_str()), (0, "stub"));
    let (code, _, _) = run(
        &sandbox,
        &["/bin/sh", "-c", "echo x > /srv/reference/written"],
        &[],
    );
    assert_ne!(code, 0, "a read-only bind was written");
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
}

#[test]
fn a_running_command_ends_with_its_sandbox() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces teardown: {reason}");
        return;
    }
    let fixture = Fixture::new("teardown");
    let provider = fixture.provider().unwrap();
    let mut sandbox = fixture.launch(&provider, "sandbox-1").unwrap();
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
    // closing it removes the writable layer and the directory, and the shared
    // image stays for the next Sandbox of it
    assert!(fixture
        .directory("sandbox-1")
        .join("overlay/upper")
        .is_dir());
    sandbox.delete().unwrap();
    assert!(!fixture.directory("sandbox-1").exists());
    assert!(fixture
        .root
        .join("images")
        .join(format!("{}.2", hex(DIGEST)))
        .join("etc/layerfs-image")
        .is_file());
}

/// Runs `script` in `sandbox` as exec `exec` of Workspace `ws1`, in a
/// thread, and answers its stop handle and the thread.
fn started(
    sandbox: &Arc<Sandbox>,
    exec: &'static str,
    script: &'static str,
) -> (Stop, std::thread::JoinHandle<Result<i32, String>>) {
    let (sent, told) = std::sync::mpsc::channel();
    let sandbox = sandbox.clone();
    let thread = std::thread::spawn(move || {
        let request = ExecRequest {
            identity: CommandIdentity { uid: 501, gid: 20 },
            arguments: vec!["/bin/sh".into(), "-c".into(), script.into()],
            environment: Vec::new(),
            directory: sandbox.tmp().display().to_string(),
            stdin: None,
        };
        let mut spawned = |stop: &Stop| {
            let _ = sent.send(stop.clone());
            Ok(())
        };
        let slot = ExecSlot {
            workspace: "ws1",
            exec,
            spawned: &mut spawned,
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        sandbox
            .run_in(&request, Some(slot), &mut out, &mut err)
            .map(|exit| exit.code)
            .map_err(|failure| format!("{:?}", failure.cause))
    });
    let stop = told.recv_timeout(Duration::from_secs(10)).unwrap();
    (stop, thread)
}

#[test]
fn a_cancelled_command_and_what_it_started_leave_its_own_group_empty() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces exec cancel: {reason}");
        return;
    }
    let fixture = Fixture::new("cancel");
    let provider = fixture.provider().unwrap();
    let sandbox = Arc::new(fixture.launch(&provider, "sandbox-1").unwrap());
    let sandbox_group = Path::new("/sys/fs/cgroup/layerfs").join(sandbox.identity().as_str());
    // a background child that left its process group, and a foreground one
    let script = "setsid /bin/sleep 600 >/dev/null 2>&1 & /bin/sleep 600";
    let (stop, thread) = started(&sandbox, "e1", script);
    let group = sandbox_group.join("ws-ws1").join("exec-e1");
    assert_eq!(
        stop,
        Stop::Cgroup {
            path: group.clone()
        }
    );
    assert_eq!(sandbox.exec_stop("ws1", "e1"), Some(stop.clone()));
    std::thread::sleep(Duration::from_millis(300));
    // the same exec id is refused while it runs
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut never = |_: &Stop| -> std::io::Result<()> { panic!("a duplicate never spawns") };
    let duplicate = ExecRequest {
        identity: CommandIdentity { uid: 501, gid: 20 },
        arguments: vec!["/bin/true".into()],
        environment: Vec::new(),
        directory: sandbox.tmp().display().to_string(),
        stdin: None,
    };
    let slot = ExecSlot {
        workspace: "ws1",
        exec: "e1",
        spawned: &mut never,
    };
    assert!(sandbox
        .run_in(&duplicate, Some(slot), &mut out, &mut err)
        .is_err());
    // the helper is in the Sandbox's leaf: only the command's processes
    // are in its own group, and none in the Sandbox's
    let procs = fs::read_to_string(group.join("cgroup.procs")).unwrap();
    assert!(procs.lines().count() >= 2, "{procs}");
    let helper = fs::read_to_string(sandbox_group.join("init").join("cgroup.procs")).unwrap();
    assert!(!helper.is_empty(), "the helper is in the Sandbox's leaf");
    let own = fs::read_to_string(sandbox_group.join("cgroup.procs")).unwrap();
    assert!(own.is_empty(), "{own}");
    let outcome = cancel(&stop, Duration::from_secs(5)).unwrap();
    assert_eq!(outcome, Cancelled::Signalled(Signal::SIGTERM));
    // the command ended by that signal, reported by the helper
    assert_eq!(thread.join().unwrap(), Ok(143));
    // its group is empty and gone, and so is its Workspace's
    assert!(!group.exists());
    assert!(!sandbox_group.join("ws-ws1").exists());
    assert_eq!(
        cancel(&stop, Duration::from_secs(1)).unwrap(),
        Cancelled::Ended
    );
    let mut sandbox = Arc::try_unwrap(sandbox).unwrap();
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
    assert!(!sandbox_group.exists());
}

#[test]
fn a_command_cancelled_before_its_handle_was_told_ends_as_it_starts() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces early cancel: {reason}");
        return;
    }
    let fixture = Fixture::new("early");
    let provider = fixture.provider().unwrap();
    let mut sandbox = fixture.launch(&provider, "sandbox-1").unwrap();
    let sandbox_group = Path::new("/sys/fs/cgroup/layerfs").join(sandbox.identity().as_str());
    let group = sandbox_group.join("ws-ws1").join("exec-e3");
    let request = ExecRequest {
        identity: CommandIdentity { uid: 501, gid: 20 },
        arguments: vec![
            "/bin/sh".into(),
            "-c".into(),
            "setsid /bin/sleep 600 >/dev/null 2>&1 & /bin/sleep 600".into(),
        ],
        environment: Vec::new(),
        directory: sandbox.tmp().display().to_string(),
        stdin: None,
    };
    let (sent, ran) = std::sync::mpsc::channel();
    let mut procs = String::new();
    std::thread::scope(|scope| {
        let (sandbox, procs) = (&sandbox, &mut procs);
        scope.spawn(move || {
            // the handle is told once the command is in its group; a
            // cancel came first
            let mut told = |stop: &Stop| {
                let Stop::Cgroup { path } = stop else {
                    panic!("{stop:?}")
                };
                *procs = fs::read_to_string(path.join("cgroup.procs")).unwrap_or_default();
                Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled as it started",
                ))
            };
            let slot = ExecSlot {
                workspace: "ws1",
                exec: "e3",
                spawned: &mut told,
            };
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let exit = sandbox.run_in(&request, Some(slot), &mut out, &mut err);
            let _ = sent.send(exit.map(|exit| exit.code).map_err(|f| f.cause));
        });
        // it ends at once, as SIGKILL ended it, not after its 600 s
        let exit = ran.recv_timeout(Duration::from_secs(20));
        assert_eq!(format!("{exit:?}"), "Ok(Ok(137))");
    });
    assert!(!procs.trim().is_empty(), "nothing ran in its group yet");
    // what it started ended with it: its group is empty and gone
    assert!(!group.exists());
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
    assert!(!sandbox_group.exists());
}

#[test]
fn a_command_group_left_running_is_removed_with_its_sandbox() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces exec group teardown: {reason}");
        return;
    }
    let fixture = Fixture::new("leftgroup");
    let provider = fixture.provider().unwrap();
    let sandbox = Arc::new(fixture.launch(&provider, "sandbox-1").unwrap());
    let sandbox_group = Path::new("/sys/fs/cgroup/layerfs").join(sandbox.identity().as_str());
    let (stop, thread) = started(
        &sandbox,
        "e2",
        "setsid /bin/sleep 600 >/dev/null 2>&1 & echo started",
    );
    assert_eq!(thread.join().unwrap(), Ok(0));
    // what it left still runs in its group, which stays until it ends
    let Stop::Cgroup { path } = &stop else {
        unreachable!()
    };
    assert!(path.is_dir());
    let mut sandbox = Arc::try_unwrap(sandbox).unwrap();
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
    assert!(!sandbox_group.exists());
}

/// The `layerfs` directory gives Sandbox groups the memory and pids
/// controllers it has and does not give yet, in one write, and nothing
/// else (layerfs-project#182 L4); a write the kernel refuses (here a
/// `cgroup.subtree_control` that cannot be opened for writing, as EBUSY or
/// ENOENT are on a real hierarchy) is an error the caller only reports.
/// Stand-in files in a scratch folder: needs neither root nor cgroup v2.
#[test]
fn the_layerfs_group_gives_only_the_controllers_it_has() {
    let base = std::env::temp_dir().join(format!("lfs-give-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    let give = |controllers: &str, given: &str| -> (std::io::Result<()>, String) {
        fs::write(base.join("cgroup.controllers"), controllers).unwrap();
        fs::write(base.join("cgroup.subtree_control"), given).unwrap();
        let outcome = layerfs_sandbox_backend::namespaces::give_controllers(&base);
        (
            outcome,
            fs::read_to_string(base.join("cgroup.subtree_control")).unwrap(),
        )
    };
    // both listed, neither given: one write of both
    let (outcome, written) = give("cpuset cpu io memory hugetlb pids rdma\n", "");
    assert!(outcome.is_ok());
    assert_eq!(written, "+memory +pids");
    // only what is listed, and never what is given already
    assert_eq!(give("cpu memory\n", "").1, "+memory");
    assert_eq!(give("cpu memory pids\n", "pids\n").1, "+memory");
    // nothing to give: nothing written
    assert_eq!(give("cpu io\n", "cpu\n").1, "cpu\n");
    assert_eq!(give("memory pids\n", "memory pids\n").1, "memory pids\n");
    // a refused write is an error, and the files are as they were
    fs::write(base.join("cgroup.controllers"), "memory pids\n").unwrap();
    fs::remove_file(base.join("cgroup.subtree_control")).unwrap();
    fs::create_dir(base.join("cgroup.subtree_control")).unwrap();
    assert!(layerfs_sandbox_backend::namespaces::give_controllers(&base).is_err());
    // a group with no controller list (no cgroup v2 there) is an error too
    fs::remove_file(base.join("cgroup.controllers")).unwrap();
    assert!(layerfs_sandbox_backend::namespaces::give_controllers(&base).is_err());
    fs::remove_dir_all(&base).unwrap();
}

/// A new Sandbox group is split before any process joins it: its leaf
/// `init` is made, then the groups below it are given the controllers it
/// has (cgroup v2 allows that only of a group with no process). Stand-in
/// files in a scratch folder:
/// needs neither root nor cgroup v2.
#[test]
fn a_sandbox_group_is_split_into_its_leaf_before_any_process() {
    use layerfs_sandbox_backend::namespaces::{leaf, split};
    let base = std::env::temp_dir().join(format!("lfs-split-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("cgroup.controllers"), "cpu memory pids\n").unwrap();
    fs::write(base.join("cgroup.subtree_control"), "").unwrap();
    split(&base).unwrap();
    assert_eq!(leaf(&base), base.join("init"));
    assert!(base.join("init").is_dir());
    assert_eq!(
        fs::read_to_string(base.join("cgroup.subtree_control")).unwrap(),
        "+memory +pids"
    );
    // one split per group: a leaf already there is refused
    let again = split(&base).unwrap_err();
    assert_eq!(again.kind(), std::io::ErrorKind::AlreadyExists);
    // a give the kernel refuses still leaves a leaf and a Sandbox
    fs::remove_dir(base.join("init")).unwrap();
    fs::remove_file(base.join("cgroup.subtree_control")).unwrap();
    fs::create_dir(base.join("cgroup.subtree_control")).unwrap();
    split(&base).unwrap();
    assert!(base.join("init").is_dir());
    fs::remove_dir_all(&base).unwrap();
}

/// On a real kernel: the Sandbox's own group holds no process, its leaf
/// holds its init, and it gives its leaf and its Workspace groups the
/// memory and pids controllers it has, so a running command's Workspace
/// group has `memory.current` and `pids.current`; the Sandbox's own files
/// count its Workspaces' processes.
#[test]
fn a_sandbox_group_gives_its_workspaces_memory_and_pids() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces Workspace controllers: {reason}");
        return;
    }
    let fixture = Fixture::new("wsmemory");
    let provider = fixture.provider().unwrap();
    let sandbox = Arc::new(fixture.launch(&provider, "sandbox-1").unwrap());
    let sandbox_group = Path::new("/sys/fs/cgroup/layerfs").join(sandbox.identity().as_str());
    let read = |path: &Path| fs::read_to_string(path).unwrap();
    assert_eq!(read(&sandbox_group.join("cgroup.procs")), "");
    assert!(!read(&sandbox_group.join("init").join("cgroup.procs")).is_empty());
    let has = read(&sandbox_group.join("cgroup.controllers"));
    let given = read(&sandbox_group.join("cgroup.subtree_control"));
    for name in ["memory", "pids"] {
        let listed = |text: &str| text.split_whitespace().any(|c| c == name);
        assert_eq!(listed(&given), listed(&has), "{name}: {given} of {has}");
    }
    let (stop, thread) = started(&sandbox, "e5", "exec /bin/sleep 600");
    let workspace = sandbox_group.join("ws-ws1");
    for (name, file) in [("memory", "memory.current"), ("pids", "pids.current")] {
        if has.split_whitespace().any(|c| c == name) {
            let value: u64 = read(&workspace.join(file)).trim().parse().unwrap();
            assert!(value > 0, "{file} of the Workspace is {value}");
        }
    }
    if has.split_whitespace().any(|c| c == "pids") {
        let total: u64 = read(&sandbox_group.join("pids.current"))
            .trim()
            .parse()
            .unwrap();
        let ours: u64 = read(&workspace.join("pids.current"))
            .trim()
            .parse()
            .unwrap();
        assert!(
            total > ours,
            "the Sandbox counts {total}, its Workspace {ours}"
        );
    }
    assert_eq!(
        cancel(&stop, Duration::from_secs(5)).unwrap(),
        Cancelled::Signalled(Signal::SIGTERM)
    );
    assert_eq!(thread.join().unwrap(), Ok(143));
    let mut sandbox = Arc::try_unwrap(sandbox).unwrap();
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
    assert!(!sandbox_group.exists());
}

/// The parent process id of `pid`, from its `/proc` status.
fn parent(pid: &str) -> String {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let line = status.lines().find(|line| line.starts_with("PPid:"));
    line.unwrap()["PPid:".len()..].trim().to_owned()
}

#[test]
fn a_command_and_its_helper_are_created_in_their_groups() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces exec groups: {reason}");
        return;
    }
    let fixture = Fixture::new("groups");
    let provider = fixture.provider().unwrap();
    let sandbox = Arc::new(fixture.launch(&provider, "sandbox-1").unwrap());
    let id = sandbox.identity().as_str().to_owned();
    // a command with no group of its own is in the Sandbox's leaf
    let (code, out, _) = run(&sandbox, &["/bin/cat", "/proc/self/cgroup"], &[]);
    assert_eq!(code, 0);
    assert!(
        out.trim_end().ends_with(&format!("/layerfs/{id}/init")),
        "{out}"
    );
    // one process, the shell replaced by sleep: a shell that forks while
    // the cancel's one SIGTERM pass reads the group would leave its new
    // child to the kill after the grace
    let (stop, thread) = started(&sandbox, "e4", "exec /bin/sleep 600");
    let Stop::Cgroup { path } = &stop else {
        unreachable!()
    };
    let procs = fs::read_to_string(path.join("cgroup.procs")).unwrap();
    let command = procs.lines().next().unwrap().to_owned();
    // the helper that waits for the command is in the Sandbox's leaf, so
    // the Sandbox's cgroup.kill ends it; its own parent, which only waits
    // for it, stays where the backend started it
    let helper = parent(&command);
    let cgroup = |pid: &str| fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap();
    assert!(
        cgroup(&helper)
            .trim_end()
            .ends_with(&format!("/layerfs/{id}/init")),
        "{}",
        cgroup(&helper)
    );
    let outer = parent(&helper);
    assert_eq!(cgroup(&outer), cgroup("self"));
    assert!(
        !procs.lines().any(|pid| pid == helper || pid == outer),
        "{procs}"
    );
    let outcome = cancel(&stop, Duration::from_secs(5)).unwrap();
    assert_eq!(outcome, Cancelled::Signalled(Signal::SIGTERM));
    assert_eq!(thread.join().unwrap(), Ok(143));
    let mut sandbox = Arc::try_unwrap(sandbox).unwrap();
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
}

#[test]
fn a_command_running_when_its_sandbox_is_killed_ends_without_a_status() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces kill under a command: {reason}");
        return;
    }
    let fixture = Fixture::new("killed");
    let provider = fixture.provider().unwrap();
    let sandbox = Arc::new(fixture.launch(&provider, "sandbox-1").unwrap());
    let sandbox_group = Path::new("/sys/fs/cgroup/layerfs").join(sandbox.identity().as_str());
    let (_, thread) = started(&sandbox, "e5", "/bin/sleep 600");
    // what a stop writes first: the helper is in the group, so it ends with
    // the command and its exec is a failure, never the command's exit code
    fs::write(sandbox_group.join("cgroup.kill"), "1").unwrap();
    let failure = thread.join().unwrap().unwrap_err();
    assert!(failure.contains("without a status"), "{failure}");
    let mut sandbox = Arc::try_unwrap(sandbox).unwrap();
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
    assert!(!sandbox_group.exists());
}

/// The network namespace identity `/proc/<pid>/ns/net` names.
fn netns(pid: &str) -> String {
    fs::read_link(format!("/proc/{pid}/ns/net"))
        .unwrap()
        .display()
        .to_string()
}

/// Waits (bounded) until the machine has no link named `link`: a network
/// namespace is torn down by the kernel after its last process ends.
fn link_gone(link: &str) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let links = layerfs_sandbox_backend::network::inspect().unwrap().links;
        if !links.iter().any(|(name, _)| name == link) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Every namespaces Sandbox's commands run in a network namespace of their
/// own, held by its init stage, joined to the machine by its own host end
/// `lfs<id12>` with one /31 of the pool; the init's inner stage (and the
/// daemon) stay on the machine's network; the firewall table is there; the
/// host end goes with the Sandbox, and the record keeps the network across
/// a reopen.
#[test]
fn each_sandbox_s_commands_have_a_network_of_their_own() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces network: {reason}");
        return;
    }
    let fixture = Fixture::new("network");
    let provider = fixture.provider().unwrap();
    let mut first = fixture.launch(&provider, "sandbox-1").unwrap();
    let mut second = fixture.launch(&provider, "sandbox-2").unwrap();
    let host = netns("self");
    let mut seen = Vec::new();
    for sandbox in [&first, &second] {
        let id = sandbox.identity().as_str().to_owned();
        let link = format!("lfs{}", &id[..12]);
        let network = sandbox
            .network()
            .expect("a started Sandbox names its network");
        let (named, guest) = network.split_once(' ').unwrap();
        assert_eq!(named, link, "{network}");
        assert!(guest.ends_with("/31"), "{network}");
        // the record keeps it
        let record = sandbox.record().unwrap();
        assert_eq!(record.namespaces().unwrap().network(), Some(network));
        // the init stage holds another network than the machine's; its
        // inner stage, and so the daemon, keeps the machine's
        let init = sandbox.process().unwrap().pid.to_string();
        assert_ne!(netns(&init), host);
        let children = fs::read_to_string(format!("/proc/{init}/task/{init}/children")).unwrap();
        let inner = children.split_whitespace().next().unwrap().to_owned();
        assert_eq!(
            netns(&inner),
            host,
            "the daemon's side left the machine's network"
        );
        // a command sees only its own loopback and guest end
        let (code, out, _) = run(sandbox, &["/bin/cat", "/proc/net/dev"], &[]);
        assert_eq!(code, 0);
        let names: Vec<&str> = (out.lines().skip(2))
            .filter_map(|line| line.split(':').next())
            .map(str::trim)
            .collect();
        assert_eq!(
            names.iter().copied().collect::<BTreeSet<_>>(),
            BTreeSet::from(["eth0", "lo"]),
            "{out}"
        );
        let (code, out, _) = run(sandbox, &["/bin/ls", "-l", "/proc/self/ns/net"], &[]);
        assert_eq!(code, 0);
        assert!(out.trim_end().ends_with(&netns(&init)), "{out}");
        // the host end is on the machine, with the other address of the /31
        let links = layerfs_sandbox_backend::network::inspect().unwrap();
        assert!(links.table, "the firewall table is missing");
        let (_, addresses) = links.links.iter().find(|(name, _)| *name == link).unwrap();
        let guest = layerfs_sandbox_backend::network::plan::Ipv4Net::parse(guest).unwrap();
        assert!(
            addresses
                .iter()
                .any(|a| a.base() == guest.base() && a.address != guest.address),
            "{addresses:?} {guest}"
        );
        seen.push((netns(&init), guest));
    }
    assert_ne!(seen[0].0, seen[1].0, "two Sandboxes share a network");
    assert_ne!(seen[0].1, seen[1].1, "two Sandboxes share an address");
    // a reopen keeps the network, as the kernel does
    let record = first.record().unwrap();
    let reopened = provider.reopen(first.identity(), &record).unwrap();
    assert_eq!(reopened.network(), first.network());
    drop(reopened);
    // the host end ends with its Sandbox; the other's stays
    let first_link = format!("lfs{}", &first.identity().as_str()[..12]);
    let second_link = format!("lfs{}", &second.identity().as_str()[..12]);
    first.stop(5).unwrap();
    first.delete().unwrap();
    assert!(link_gone(&first_link), "{first_link} outlived its Sandbox");
    let links = layerfs_sandbox_backend::network::inspect().unwrap().links;
    assert!(links.iter().any(|(name, _)| *name == second_link));
    second.stop(5).unwrap();
    second.delete().unwrap();
    assert!(
        link_gone(&second_link),
        "{second_link} outlived its Sandbox"
    );
}

/// The Sandbox's resolver files: where this machine's resolver is one a
/// Sandbox's network cannot reach (systemd-resolved's `127.0.0.53`, a lima
/// guest), its commands get the stub's upstream servers instead, with the
/// search and options lines kept, while its daemon, on the machine's
/// network, keeps this machine's own file; otherwise both get this
/// machine's file. A mount the daemon's side makes later, as a Workspace's,
/// reaches commands, and its unmount does too.
#[test]
fn a_sandbox_resolves_through_a_server_its_network_reaches() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces resolver: {reason}");
        return;
    }
    let fixture = Fixture::new("resolver");
    let provider = fixture.provider().unwrap();
    let mut sandbox = fixture.launch(&provider, "sandbox-1").unwrap();
    let (code, inside, _) = run(&sandbox, &["/bin/cat", "/etc/resolv.conf"], &[]);
    assert_eq!(code, 0);
    // the daemon's side: the init's inner stage, in the daemon's mount
    // namespace
    let init = sandbox.process().unwrap().pid.to_string();
    let children = fs::read_to_string(format!("/proc/{init}/task/{init}/children")).unwrap();
    let inner = children.split_whitespace().next().unwrap().to_owned();
    let daemon_side =
        fs::read_to_string(format!("/proc/{inner}/root/etc/resolv.conf")).unwrap_or_default();
    let launcher = fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    let systemd = fs::read_to_string("/run/systemd/resolve/resolv.conf").ok();
    match layerfs_sandbox_backend::network::resolver::resolver_file(&launcher, systemd.as_deref()) {
        layerfs_sandbox_backend::network::resolver::Resolver::Bind => assert_eq!(inside, launcher),
        layerfs_sandbox_backend::network::resolver::Resolver::Write(text) => {
            assert_eq!(inside, text);
            assert!(!inside.contains("nameserver 127."), "{inside}");
            eprintln!("RESOLVER written: {inside}");
        }
        layerfs_sandbox_backend::network::resolver::Resolver::Refuse(why) => {
            panic!("this host's resolver is refused yet a Sandbox started: {why}")
        }
    }
    assert_eq!(
        daemon_side, launcher,
        "the daemon does not resolve as this machine does"
    );
    // a mount made on the daemon's side after the start reaches commands,
    // and so does its unmount
    let point = fixture
        .directory("sandbox-1")
        .join("workspaces")
        .join("probe");
    fs::create_dir(&point).unwrap();
    let on_daemon_side = |script: &str| {
        std::process::Command::new("nsenter")
            .args(["--target", &inner, "--mount", "/bin/sh", "-c", script])
            .status()
            .is_ok_and(|status| status.success())
    };
    let seen = point.join("seen");
    assert!(on_daemon_side(&format!(
        "/bin/mount -t tmpfs -o mode=0755 probe {p} && /bin/echo seen > {s}",
        p = point.display(),
        s = seen.display()
    )));
    assert!(!seen.exists(), "the probe mount reached the machine");
    let probe = seen.display().to_string();
    assert_eq!(
        run(&sandbox, &["/bin/cat", &probe], &[]),
        (0, "seen\n".to_owned(), String::new()),
        "a mount made on the daemon's side did not reach commands"
    );
    assert!(on_daemon_side(&format!("/bin/umount {}", point.display())));
    let (code, out, _) = run(&sandbox, &["/bin/cat", &probe], &[]);
    assert!(
        code != 0 && out.is_empty(),
        "an unmount on the daemon's side did not reach commands"
    );
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
}

/// The table, as `nft` lists it, when the machine has `nft`: the rules the
/// design names, with a closed port.
#[test]
fn the_firewall_table_holds_the_design_s_rules() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces table: {reason}");
        return;
    }
    let fixture = Fixture::new("table");
    let provider = fixture.provider().unwrap();
    let request = fixture.request("sandbox-1");
    let mut sandbox = provider
        .launch_with_binds(request, vec![fixture.store_bind()])
        .unwrap();
    match std::process::Command::new("nft")
        .args(["list", "table", "inet", "layerfs"])
        .output()
    {
        Ok(listed) if listed.status.success() => {
            let text = String::from_utf8_lossy(&listed.stdout);
            for rule in [
                "type filter hook forward priority filter; policy accept;",
                "iifname \"lfs*\" oifname \"lfs*\" drop",
                "iifname \"lfs*\" accept",
                "oifname \"lfs*\" ct state established,related accept",
                "oifname \"lfs*\" drop",
                "iifname \"lfs*\" tcp dport 7070 drop",
                "type nat hook postrouting priority srcnat; policy accept;",
                "oifname != \"lfs*\" masquerade",
            ] {
                assert!(text.contains(rule), "missing {rule}: {text}");
            }
            eprintln!("TABLE {text}");
        }
        other => eprintln!("NOT_RUN table listing: no usable nft ({other:?})"),
    }
    sandbox.stop(5).unwrap();
    sandbox.delete().unwrap();
}

/// A command whose Sandbox's network cannot be joined never runs: an init
/// stage on the machine's network (here a process with its own mount, PID
/// and IPC namespaces but the machine's network, as an earlier build's init
/// stage was) makes the helper end with a signal before the command starts,
/// saying the network is why. Every other join succeeds here, so the
/// network is the only cause; and the command, had it run, would have
/// written its marker: the holder's mount namespace is a copy of this
/// host's, where `writable` is world-writable (checked as the command's own
/// uid before the refusal is tried).
#[test]
fn a_command_never_runs_where_its_network_cannot_be_joined() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces join failure: {reason}");
        return;
    }
    let fixture = Fixture::new("join");
    let mut holder = std::process::Command::new("unshare")
        .args(["--mount", "--pid", "--ipc", "--fork", "/bin/sleep", "30"])
        .spawn()
        .unwrap();
    // wait (bounded) until the holder's own namespaces exist
    let pid = holder.id().to_string();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fs::read_link(format!("/proc/{pid}/ns/mnt")).ok()
        == fs::read_link("/proc/self/ns/mnt").ok()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "unshare made no namespaces"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let group =
        Path::new("/sys/fs/cgroup").join(format!("layerfs-join-test-{}", std::process::id()));
    fs::create_dir(&group).unwrap();
    let writable = fixture.base.join("w");
    fs::create_dir(&writable).unwrap();
    fs::set_permissions(&writable, fs::Permissions::from_mode(0o777)).unwrap();
    let marker = writable.join("ran");
    // the control: the command's uid can write the marker in the holder's
    // mount namespace, so a command that ran would leave it
    let probe = writable.join("probe");
    let wrote = std::process::Command::new("nsenter")
        .args([
            "--target", &pid, "--mount", "--setuid", "501", "--setgid", "20",
        ])
        .args(["/bin/sh", "-c"])
        .arg(format!("echo probe > {}", probe.display()))
        .status();
    let exec = std::process::Command::new(helper())
        .args(["exec", "--init", &pid, "--cgroup"])
        .arg(&group)
        .args([
            "--uid", "501", "--gid", "20", "--cwd", "/", "--", "/bin/sh", "-c",
        ])
        .arg(format!("echo ran > {}", marker.display()))
        .output()
        .unwrap();
    let _ = holder.kill();
    let _ = holder.wait();
    let _ = fs::remove_dir(&group);
    assert!(
        wrote.as_ref().is_ok_and(|status| status.success()) && probe.is_file(),
        "the command's uid cannot write {}: {wrote:?}",
        writable.display()
    );
    use std::os::unix::process::ExitStatusExt;
    let said = String::from_utf8_lossy(&exec.stderr);
    assert_eq!(exec.status.signal(), Some(9), "{:?} {said}", exec.status);
    assert!(
        said.contains("holds no network namespace of its own"),
        "the stage ended for another reason than the network: {said}"
    );
    assert!(!marker.exists(), "the command ran on the machine's network");
}

/// An init stage that finds the machine's pool is not the one the launcher
/// checked (another start, or a route, changed it in between) ends before
/// it wires anything, with the `UNAVAILABLE` line the launcher answers the
/// start `Unavailable` from (spec §3.2), not as a stage that failed.
#[test]
fn an_init_stage_whose_pool_changed_says_unavailable() {
    use layerfs_sandbox_backend::network::plan::{unavailable_in, Ipv4Net, CANDIDATES};
    use layerfs_sandbox_backend::{HelperSpec, SpecNetwork};
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces init pool change: {reason}");
        return;
    }
    let fixture = Fixture::new("pool-changed");
    let now = layerfs_sandbox_backend::network::pool().unwrap();
    let other = (CANDIDATES.iter())
        .filter_map(|(address, prefix)| Ipv4Net::new(*address, *prefix))
        .find(|pool| *pool != now)
        .unwrap();
    let group =
        Path::new("/sys/fs/cgroup").join(format!("layerfs-pool-test-{}", std::process::id()));
    fs::create_dir(&group).unwrap();
    let directory = fixture.directory("sandbox-1");
    fs::create_dir(&directory).unwrap();
    let spec = HelperSpec {
        root: fixture.root.join("sandbox-1-root"),
        sandbox: directory.clone(),
        daemon: directory.join("bin/layerfs-daemon"),
        config: directory.join("config/daemon.setup"),
        cgroup: group.clone(),
        lower: fixture.base.join("lower"),
        upper: directory.join("overlay/upper"),
        work: directory.join("overlay/work"),
        binds: Vec::new(),
        runtime: None,
        network: Some(SpecNetwork {
            pool: other,
            link: "lfs0000poolchg".to_owned(),
        }),
        resolver: None,
    };
    let path = fixture.base.join("helper.spec");
    fs::write(&path, spec.encode().unwrap()).unwrap();
    let ran = std::process::Command::new(helper())
        .arg("init")
        .arg("--spec")
        .arg(&path)
        .output()
        .unwrap();
    let _ = fs::remove_dir(&group);
    let said = String::from_utf8_lossy(&ran.stderr);
    assert_eq!(ran.status.code(), Some(125), "{said}");
    let why = unavailable_in(&said).unwrap_or_else(|| panic!("no UNAVAILABLE line: {said}"));
    assert!(
        why.contains(&now.to_string()) && why.contains(&other.to_string()),
        "{why}"
    );
    assert!(!fs::exists(directory.join("network")).unwrap());
    let links = layerfs_sandbox_backend::network::inspect().unwrap().links;
    assert!(!links.iter().any(|(name, _)| name == "lfs0000poolchg"));
}

/// A machine that forwards no IPv4, or whose forward chain drops what it
/// does not accept, refuses a start before any effect, naming the fix; the
/// rules the refusal names admit the Sandboxes. Changes this machine's
/// `net.ipv4.ip_forward` and iptables FORWARD policy for its duration and
/// restores them: run it alone (`--test-threads=1`).
#[test]
fn a_machine_that_would_drop_sandbox_traffic_refuses_the_start() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces forwarding refusals: {reason}");
        return;
    }
    if std::env::var_os("LAYERFS_TEST_NETWORK_ALONE").is_none() {
        eprintln!("NOT_RUN: set LAYERFS_TEST_NETWORK_ALONE=1 and run with --test-threads=1");
        return;
    }
    let fixture = Fixture::new("refusals");
    let provider = fixture.provider().unwrap();
    let forward = "/proc/sys/net/ipv4/ip_forward";
    let before = fs::read_to_string(forward).unwrap();
    fs::write(forward, "0").unwrap();
    let refused = fixture.launch(&provider, "sandbox-1");
    fs::write(forward, before.trim()).unwrap();
    let failure = refused.expect_err("a machine that forwards nothing started a Sandbox");
    assert!(failure.instance.is_none(), "a refused start made something");
    let said = failure.cause.to_string();
    assert!(said.contains("net.ipv4.ip_forward=0"), "{said}");
    assert!(
        matches!(failure.cause, RuntimeError::Unavailable(_)),
        "{said}"
    );
    let iptables = |args: &[&str]| {
        std::process::Command::new("iptables")
            .args(args)
            .status()
            .is_ok_and(|status| status.success())
    };
    if !iptables(&["-P", "FORWARD", "DROP"]) {
        eprintln!("NOT_RUN forward policy half: no usable iptables");
        return;
    }
    let dropped = fixture.launch(&provider, "sandbox-2");
    let admit = [
        vec!["FORWARD", "-i", "lfs+", "-j", "ACCEPT"],
        vec![
            "FORWARD",
            "-o",
            "lfs+",
            "-m",
            "conntrack",
            "--ctstate",
            "RELATED,ESTABLISHED",
            "-j",
            "ACCEPT",
        ],
    ];
    let mut admitted_ok = true;
    for rule in &admit {
        let mut args = vec!["-I"];
        args.extend(rule.iter().copied());
        admitted_ok &= iptables(&args);
    }
    let admitted = fixture.launch(&provider, "sandbox-3");
    for rule in &admit {
        let mut args = vec!["-D"];
        args.extend(rule.iter().copied());
        iptables(&args);
    }
    iptables(&["-P", "FORWARD", "ACCEPT"]);
    let failure = dropped.expect_err("a dropping forward chain started a Sandbox");
    assert!(failure.instance.is_none());
    let said = failure.cause.to_string();
    assert!(
        said.contains("forward chain FORWARD") && said.contains("lfs+"),
        "{said}"
    );
    assert!(admitted_ok);
    let mut admitted = admitted.expect("the admitted machine refused a Sandbox");
    admitted.stop(5).unwrap();
    admitted.delete().unwrap();
}

/// A Sandbox directory of `fixture` named `name`, laid out as a launch
/// lays one out over the image a launch unpacked, and its spec with host
/// end `link` and the resolver file `resolver` names (given the directory):
/// the init stage then holds the commands' own view whatever this host's
/// resolver is (`resolver_file` answers `Bind` on many hosts, and the
/// launcher then names none). Returns the spec's path, the directory and
/// the cgroup the spec names.
fn gated(
    fixture: &Fixture,
    name: &str,
    link: &str,
    resolver: &dyn Fn(&Path) -> PathBuf,
) -> (PathBuf, PathBuf, PathBuf) {
    use layerfs_sandbox_backend::{HelperSpec, SpecNetwork};
    // the image's unpacked root, as a launch's own spec names it
    let provider = fixture.provider().unwrap();
    let mut launched = fixture.launch(&provider, "unpack").unwrap();
    let written = fs::read(fixture.directory("unpack").join("config/helper.spec")).unwrap();
    let lower = HelperSpec::parse(&written).unwrap().lower;
    launched.stop(5).unwrap();
    launched.delete().unwrap();
    let directory = fixture.directory(name);
    let root = fixture.root.join(format!("{name}-root"));
    for (path, mode) in [
        (directory.clone(), 0o755),
        (directory.join("bin"), 0o755),
        (directory.join("config"), 0o700),
        (directory.join("overlay"), 0o700),
        (directory.join("overlay/upper"), 0o700),
        (directory.join("overlay/work"), 0o700),
        (root.clone(), 0o755),
    ] {
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    }
    fs::copy(&fixture.daemon, directory.join("bin/layerfs-daemon")).unwrap();
    fs::write(directory.join("config/daemon.setup"), b"stub setup").unwrap();
    let group =
        Path::new("/sys/fs/cgroup").join(format!("layerfs-{name}-test-{}", std::process::id()));
    fs::create_dir(&group).unwrap();
    let spec = HelperSpec {
        root,
        sandbox: directory.clone(),
        daemon: directory.join("bin/layerfs-daemon"),
        config: directory.join("config/daemon.setup"),
        cgroup: group.clone(),
        lower,
        upper: directory.join("overlay/upper"),
        work: directory.join("overlay/work"),
        binds: Vec::new(),
        runtime: None,
        network: Some(SpecNetwork {
            pool: layerfs_sandbox_backend::network::pool().unwrap(),
            link: link.to_owned(),
        }),
        resolver: Some(resolver(&directory)),
    };
    let path = fixture.base.join(format!("{name}.spec"));
    fs::write(&path, spec.encode().unwrap()).unwrap();
    (path, directory, group)
}

/// `helper init` on `spec`, its standard output read line by line on a
/// thread of its own.
fn init_stage(
    spec: &Path,
) -> (
    std::process::Child,
    std::sync::mpsc::Receiver<String>,
    std::thread::JoinHandle<String>,
) {
    use std::io::{BufRead, Read};
    let mut child = std::process::Command::new(helper())
        .arg("init")
        .arg("--spec")
        .arg(spec)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let out = child.stdout.take().unwrap();
    let (lines, read) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let mut err = child.stderr.take().unwrap();
    let said = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = err.read_to_string(&mut text);
        text
    });
    (child, read, said)
}

/// Ends every process of cgroup `group` and removes it, once it is empty
/// (a bounded wait: the kernel empties it after the processes end).
fn end_group(group: &Path) {
    let _ = fs::write(group.join("cgroup.kill"), "1");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while fs::read_to_string(group.join("cgroup.procs")).is_ok_and(|procs| !procs.is_empty())
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    fs::remove_dir(group).unwrap();
}

/// The commands' view and its gate, from a spec that names a written
/// resolver file (review of 210f032bc4, finding 2), on any host: once the
/// daemon listens, the commands' view (the outer stage's mount namespace,
/// which commands join) has the written file at `/etc/resolv.conf` and the
/// daemon's has this machine's; a mount the daemon's side makes, and its
/// unmount, reach the commands' view, and nothing reaches the machine.
#[test]
fn a_spec_naming_a_written_resolver_gives_commands_their_own_view() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces commands' view: {reason}");
        return;
    }
    let fixture = Fixture::new("view");
    let text = "# written for this test\nnameserver 192.0.2.53\n";
    let link = "lfs0000view001";
    let (spec, directory, group) = gated(&fixture, "sandbox-1", link, &|directory| {
        let file = directory.join("resolv.conf");
        fs::write(&file, text).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        file
    });
    let (mut child, lines, said) = init_stage(&spec);
    let listening = lines
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_default();
    let outer = child.id().to_string();
    let children =
        fs::read_to_string(format!("/proc/{outer}/task/{outer}/children")).unwrap_or_default();
    let inner = children.split_whitespace().next().unwrap_or("").to_owned();
    let read = |pid: &str, path: &str| {
        fs::read_to_string(format!("/proc/{pid}/root{path}")).unwrap_or_default()
    };
    let commands = read(&outer, "/etc/resolv.conf");
    let daemon = read(&inner, "/etc/resolv.conf");
    let launcher = fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    // a mount on the daemon's side, then its unmount
    let point = directory.join("probe");
    fs::create_dir(&point).unwrap();
    let on_daemon_side = |script: &str| {
        std::process::Command::new("nsenter")
            .args(["--target", &inner, "--mount", "/bin/sh", "-c", script])
            .status()
            .is_ok_and(|status| status.success())
    };
    let seen = point.join("seen");
    let mounted = on_daemon_side(&format!(
        "/bin/mount -t tmpfs -o mode=0755 probe {p} && /bin/echo seen > {s}",
        p = point.display(),
        s = seen.display()
    ));
    let machine_saw = seen.exists();
    let probe = seen.display().to_string();
    let commands_saw = read(&outer, &probe);
    let unmounted = on_daemon_side(&format!("/bin/umount {}", point.display()));
    let commands_still = fs::exists(format!("/proc/{outer}/root{probe}")).unwrap_or(true);
    end_group(&group);
    let _ = child.wait();
    let said = said.join().unwrap();
    assert!(
        listening.starts_with("LAYERFS_DAEMON_LISTEN"),
        "the daemon did not start: {listening:?} {said}"
    );
    assert!(!inner.is_empty(), "no inner stage: {said}");
    assert_eq!(commands, text, "the commands' view lacks the written file");
    assert_eq!(
        daemon, launcher,
        "the daemon does not resolve as this machine does"
    );
    assert!(mounted, "the daemon's side could not mount: {said}");
    assert!(!machine_saw, "the probe mount reached the machine");
    assert_eq!(
        commands_saw, "seen\n",
        "a daemon-side mount did not reach commands"
    );
    assert!(unmounted);
    assert!(
        !commands_still,
        "a daemon-side unmount did not reach commands"
    );
    assert!(link_gone(link), "{link} outlived its Sandbox");
}

/// A commands' view that cannot be made (the written resolver file is not
/// there inside the root) ends the Sandbox before its daemon starts: the
/// stage fails, naming why, the daemon never says it listens, and no
/// process of the Sandbox is left.
#[test]
fn a_commands_view_that_cannot_be_made_ends_the_sandbox_before_its_daemon() {
    if let Some(reason) = unavailable() {
        eprintln!("SKIPPED namespaces failed commands' view: {reason}");
        return;
    }
    let fixture = Fixture::new("no-view");
    let link = "lfs0000noview1";
    let (spec, _, group) = gated(&fixture, "sandbox-1", link, &|directory| {
        directory.join("absent-resolv.conf")
    });
    let (mut child, lines, said) = init_stage(&spec);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait().unwrap() {
            Some(status) => break Some(status),
            None if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20))
            }
            None => break None,
        }
    };
    let left = fs::read_to_string(group.join("cgroup.procs")).unwrap_or_default();
    end_group(&group);
    if status.is_none() {
        let _ = child.wait();
    }
    let said = said.join().unwrap();
    let printed: Vec<String> = lines.try_iter().collect();
    assert_eq!(
        status.and_then(|status| status.code()),
        Some(125),
        "the stage did not end on its own: {said}"
    );
    assert!(
        !printed
            .iter()
            .any(|line| line.contains("LAYERFS_DAEMON_LISTEN")),
        "the daemon started without the commands' view: {printed:?}"
    );
    assert!(said.contains("absent-resolv.conf"), "{said}");
    assert!(
        left.trim().is_empty(),
        "processes outlived the stage: {left}"
    );
    assert!(link_gone(link), "{link} outlived its Sandbox");
}
