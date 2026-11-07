//! A command inside a machine's guest, as root, and what setup builds on it:
//! exec, small files through stdin, and streamed copies in both directions
//! (a `tar` stream in, `cat` out), never buffered whole.
use super::tool::{self, shell_escape, Output};
use crate::error::{Code, MachineError};
use crate::platforms::firecracker::FIRECRACKER_GUEST;
use crate::spec::Tools;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub(crate) const EXEC: Duration = Duration::from_secs(60);
pub(crate) const COPY: Duration = Duration::from_secs(600);

/// How the host reaches one guest's shell as root.
#[derive(Clone, Debug)]
pub(crate) enum Guest {
    /// ssh with Lima's own `ssh.config` for the instance, then `sudo -n`.
    Lima { dir: PathBuf, vm: String },
    /// `container exec`.
    AppleContainer { vm: String },
    /// Through its Lima host, then ssh from there to the microVM.
    Firecracker {
        host: Box<Guest>,
        root: String,
        vm: String,
    },
    /// `docker exec` in the docker machine's worker container (macOS, Linux).
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Docker { socket: PathBuf, container: String },
}

impl Guest {
    /// The host command that runs `argv` in the guest as root; `stdin` keeps
    /// the guest command's stdin open for the caller's input.
    pub fn command(
        &self,
        tools: &Tools,
        argv: &[&str],
        stdin: bool,
    ) -> Result<Command, MachineError> {
        Ok(match self {
            Self::Lima { dir, vm } => {
                let mut c = tool::command(&tool::find(tools, "ssh")?, ["-F"]);
                c.arg(dir.join("ssh.config"))
                    .arg(format!("lima-{vm}"))
                    .arg("--");
                c.args(
                    ["sudo", "-n", "--"]
                        .iter()
                        .chain(argv)
                        .map(|a| shell_escape(a)),
                );
                c
            }
            Self::AppleContainer { vm } => {
                let mut c = tool::command(&tool::find(tools, "container")?, ["exec"]);
                if stdin {
                    c.arg("-i");
                }
                c.arg(vm).args(argv);
                c
            }
            Self::Firecracker { host, root, vm } => {
                let mut inner = vec!["sh", "-c", FIRECRACKER_GUEST, "sh", root, vm];
                let escaped: Vec<String> = argv.iter().map(|a| shell_escape(a)).collect();
                inner.extend(escaped.iter().map(String::as_str));
                return host.command(tools, &inner, stdin);
            }
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Docker { socket, container } => {
                let mut c = tool::command(&tool::find(tools, "docker")?, ["-H"]);
                c.arg(format!("unix://{}", socket.display())).arg("exec");
                if stdin {
                    c.arg("-i");
                }
                c.arg(container).args(argv);
                c
            }
        })
    }

    /// One command in the guest, its output captured.
    pub fn exec(
        &self,
        tools: &Tools,
        argv: &[&str],
        timeout: Duration,
    ) -> Result<Output, MachineError> {
        let command = self.command(tools, argv, false)?;
        tool::run(command, None, timeout, &format!("{} in the guest", argv[0]))
    }

    /// A shell script in the guest with positional arguments; its stdout on success.
    pub fn script(
        &self,
        tools: &Tools,
        what: &str,
        body: &str,
        args: &[&str],
    ) -> Result<String, MachineError> {
        let mut argv = vec!["sh", "-c", body, "sh"];
        argv.extend_from_slice(args);
        let command = self.command(tools, &argv, false)?;
        tool::run(command, None, EXEC, what)?.answer(what)
    }

    /// Writes a 0600 guest file from stdin, so its text is never in a command line.
    pub fn write_file(&self, tools: &Tools, path: &str, text: &str) -> Result<(), MachineError> {
        let argv = [
            "sh",
            "-c",
            "umask 077 && mkdir -p \"$(dirname \"$1\")\" && cat > \"$1.part\" && mv \"$1.part\" \"$1\"",
            "sh",
            path,
        ];
        let command = self.command(tools, &argv, true)?;
        let what = format!("writing {path} in the guest");
        tool::run(command, Some(text.as_bytes()), EXEC, &what)?.answer(&what)?;
        Ok(())
    }

    /// A guest file's text, or `None` when it does not exist.
    pub fn read_file(&self, tools: &Tools, path: &str) -> Result<Option<String>, MachineError> {
        let out = self.exec(
            tools,
            &[
                "sh",
                "-c",
                "test -f \"$1\" || exit 3; cat \"$1\"",
                "sh",
                path,
            ],
            EXEC,
        )?;
        match out.code {
            Some(0) => Ok(Some(out.stdout)),
            Some(3) => Ok(None),
            _ => Err(MachineError::new(
                Code::Failed,
                format!(
                    "reading {path} in the guest failed ({}): {}",
                    out.exit(),
                    out.said()
                ),
            )),
        }
    }

    /// Copies `entry` of the host folder `dir` (or all of it but its top
    /// folder `skip`) into the guest folder `destination` as one `tar`
    /// stream piped straight into the guest; `COPYFILE_DISABLE` keeps macOS
    /// `tar` from adding `._` files.
    pub fn copy_in(
        &self,
        tools: &Tools,
        dir: &Path,
        entry: Option<&str>,
        skip: Option<&str>,
        destination: &str,
    ) -> Result<(), MachineError> {
        let what = format!("copying {} into the guest's {destination}", dir.display());
        let mut pack = tool::command(&tool::find(tools, "tar")?, ["-C"]);
        pack.arg(dir);
        if let Some(skip) = skip {
            pack.arg(format!("--exclude=./{skip}"));
        }
        pack.args(["-cf", "-", entry.unwrap_or(".")])
            .env("COPYFILE_DISABLE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut packer = pack.spawn().map_err(|e| tool::spawn_failed(&what, &e))?;
        let stream = packer.stdout.take().expect("piped stdout");
        let unpack = [
            "sh",
            "-c",
            "mkdir -p \"$1\" && tar -C \"$1\" -xf -",
            "sh",
            destination,
        ];
        let mut command = self.command(tools, &unpack, true)?;
        command
            .stdin(Stdio::from(stream))
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let unpacker = command.spawn();
        let deadline = Instant::now() + COPY;
        let Ok(mut unpacker) = unpacker else {
            let _ = packer.kill();
            let _ = packer.wait();
            return Err(MachineError::new(
                Code::Failed,
                format!("{what}: the guest side did not start"),
            ));
        };
        let err = unpacker.stderr.take().map(|mut e| {
            std::thread::spawn(move || {
                let mut text = String::new();
                let _ = std::io::Read::read_to_string(&mut e, &mut text);
                text
            })
        });
        let unpacked = tool::wait(&mut unpacker, deadline);
        let packed = tool::wait(&mut packer, deadline);
        let said = err.and_then(|e| e.join().ok()).unwrap_or_default();
        match (unpacked, packed) {
            (Ok(Some(u)), Ok(Some(p))) if u.success() && p.success() => Ok(()),
            (Ok(Some(u)), Ok(Some(p))) => Err(MachineError::new(
                Code::Failed,
                format!("{what} failed (guest {u}, host tar {p}): {}", said.trim()),
            )),
            _ => Err(MachineError::new(
                Code::Unknown,
                format!(
                    "{what} did not end within {} s and was stopped",
                    COPY.as_secs()
                ),
            )),
        }
    }

    /// Copies one guest file to a new 0600 host file, streamed through `cat`.
    pub fn copy_out(&self, tools: &Tools, source: &str, file: &Path) -> Result<(), MachineError> {
        let what = format!("copying the guest's {source} to {}", file.display());
        let out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(file)
            .map_err(|e| MachineError::new(Code::Failed, format!("{what}: {e}")))?;
        let mut command = self.command(tools, &["cat", source], false)?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::null());
        let mut child = command.spawn().map_err(|e| tool::spawn_failed(&what, &e))?;
        match tool::wait(&mut child, Instant::now() + COPY) {
            Ok(Some(status)) if status.success() => Ok(()),
            Ok(Some(status)) => Err(MachineError::new(
                Code::Failed,
                format!("{what} failed ({status})"),
            )),
            _ => Err(MachineError::new(
                Code::Unknown,
                format!("{what} did not end in time"),
            )),
        }
    }
}
