//! Stand-in provider tools for the machine tests: small shell scripts in a
//! scratch folder that log their arguments and answer as a test needs. The
//! machine layer finds them through `Tools::path`, so no real VM, ssh host
//! or Docker Engine is touched.
#![allow(dead_code)]
use layerfs_platform::{Bundle, Lasting, Platform, Spec, SshSpec, Tools};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A scratch folder with `bin/` for stand-ins and `work/` for the machine.
pub struct Scratch {
    pub dir: PathBuf,
}

impl Scratch {
    pub fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "lfs-machine-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::create_dir_all(dir.join("work")).unwrap();
        Self { dir }
    }

    pub fn bin(&self) -> PathBuf {
        self.dir.join("bin")
    }

    pub fn log_file(&self) -> PathBuf {
        self.dir.join("calls.log")
    }

    /// Every stand-in call so far, one line each: the tool's name and arguments.
    pub fn log(&self) -> String {
        fs::read_to_string(self.log_file()).unwrap_or_default()
    }

    /// A stand-in `name` whose body runs after it logs `name args...`.
    /// `$SCRATCH` is the scratch folder.
    pub fn tool(&self, name: &str, body: &str) {
        let path = self.bin().join(name);
        let script = format!(
            "#!/bin/sh\nSCRATCH='{}'\necho \"{name} $*\" >> \"$SCRATCH/calls.log\"\n{body}\n",
            self.dir.display()
        );
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Tools that find only the stand-ins.
    pub fn tools(&self) -> Tools {
        Tools {
            path: self.bin().into_os_string(),
            docker_socket: PathBuf::from("/var/run/docker.sock"),
        }
    }

    /// A bundle folder with the three Linux binaries (empty stand-ins).
    pub fn bundle(&self) -> Bundle {
        let dir = self.dir.join("bundle");
        fs::create_dir_all(&dir).unwrap();
        for name in ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"] {
            fs::write(dir.join(name), b"").unwrap();
        }
        Bundle {
            dir,
            hash: "0123abcd".into(),
        }
    }

    /// A 2-CPU, 2G-memory, 10G-disk machine of `kind` named `dev`, VM `lfs-dev`.
    pub fn spec(&self, kind: Platform) -> Spec {
        Spec {
            name: "dev".into(),
            kind,
            vm_name: "lfs-dev".into(),
            cpus: 2,
            memory: 2 << 30,
            disk: 10 << 30,
            image: None,
            os_image: None,
            nested: false,
            host: None,
            ssh: (kind == Platform::Ssh)
                .then(|| SshSpec::new("example.test", self.dir.join("known_hosts"))),
            state: Lasting {
                port: Some(45678),
                network: None,
                gateway: None,
            },
            worker: None,
            work_dir: self.dir.join("work"),
            tools: self.tools(),
        }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The text of `path`, or empty.
pub fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}
