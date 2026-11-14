//! The host's Docker, as the image copies read it (D3): the `docker` CLI on
//! this process's search path, pointed at the `docker` machine record's
//! socket (`machine update docker --docker`), else Docker's default context,
//! whatever machine an image is going to. Reads have a deadline; nothing
//! here pulls or builds an image.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::refused;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How long one Docker read may take.
pub const READ: Duration = Duration::from_secs(10);

/// The host's Docker CLI.
#[derive(Clone, Debug)]
pub struct Docker {
    /// The `docker` executable, or `None` when it is not installed.
    program: Option<PathBuf>,
    /// The `-H` argument, when the docker machine records a socket.
    host: Option<String>,
}

/// Why a Docker read failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// No `docker` on the search path.
    NotInstalled,
    /// The Engine did not answer (not running, or past the deadline).
    NotRunning(String),
    /// Docker answered that the object does not exist.
    Missing(String),
    /// Any other failure.
    Error(String),
}

impl Failure {
    /// Its `reason` as `machine.images` names it.
    pub fn reason(&self) -> &'static str {
        match self {
            Failure::NotInstalled => "not-installed",
            Failure::NotRunning(_) => "not-running",
            _ => "error",
        }
    }

    /// Its message for people.
    pub fn message(&self) -> String {
        match self {
            Failure::NotInstalled => "docker is not on the search path".to_owned(),
            Failure::NotRunning(m) | Failure::Missing(m) | Failure::Error(m) => m.clone(),
        }
    }

    /// The refusal of a call that needed Docker: `Unavailable` `{what:
    /// 'docker'}`, or `InvalidInput` when Docker has no such image.
    pub fn refusal(&self, reference: &str) -> SdkRefusal {
        match self {
            Failure::Missing(_) => refused(
                "InvalidInput",
                format!("Docker has no image {reference} (docker image ls lists them)"),
            ),
            other => crate::images::refused_with(
                "Unavailable",
                format!("Docker cannot be read: {}", other.message()),
                SdkValue::map([("what", SdkValue::text("docker"))]),
            ),
        }
    }
}

impl Docker {
    /// The Docker the home's `docker` machine names.
    pub fn of(home: &Path) -> Result<Self, SdkRefusal> {
        let socket = layerfs_machine::record::MachineRecord::read(home, "docker")?
            .and_then(|r| r.docker().map(str::to_owned));
        let path = std::env::var_os("PATH").unwrap_or_default();
        let program = std::env::split_paths(&path)
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(|dir| dir.join("docker"))
            .find(|p| p.is_file());
        Ok(Self::at(program, socket.map(|s| format!("unix://{s}"))))
    }

    /// A Docker CLI at `program`, pointed at `host` (tests use a stand-in).
    pub fn at(program: Option<PathBuf>, host: Option<String>) -> Self {
        Self { program, host }
    }

    /// `docker [-H host] args...`.
    pub fn command(&self, args: &[&str]) -> Result<Command, Failure> {
        let program = self.program.as_ref().ok_or(Failure::NotInstalled)?;
        let mut command = Command::new(program);
        if let Some(host) = &self.host {
            command.arg("-H").arg(host);
        }
        command.args(args).stdin(Stdio::null());
        Ok(command)
    }

    /// Runs `docker args...` to its end within `deadline`: its stdout.
    pub fn read(&self, args: &[&str], deadline: Duration) -> Result<String, Failure> {
        let mut command = self.command(args)?;
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|e| Failure::Error(format!("docker: {e}")))?;
        let out = drain(child.stdout.take());
        let err = drain(child.stderr.take());
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if start.elapsed() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Failure::NotRunning(format!(
                        "docker {} did not answer within {} s",
                        args.first().copied().unwrap_or(""),
                        deadline.as_secs()
                    )));
                }
                Err(e) => return Err(Failure::Error(format!("docker: {e}"))),
            }
        };
        let (out, err) = (
            out.join().unwrap_or_default(),
            err.join().unwrap_or_default(),
        );
        if status.success() {
            return Ok(out);
        }
        let message = err.trim().to_owned();
        let lower = message.to_lowercase();
        Err(
            if lower.contains("no such image") || lower.contains("no such object") {
                // a batched inspect answers the images it found
                if !out.trim().is_empty() {
                    return Ok(out);
                }
                Failure::Missing(message)
            } else if lower.contains("cannot connect")
                || lower.contains("is the docker daemon running")
            {
                Failure::NotRunning(message)
            } else {
                Failure::Error(message)
            },
        )
    }

    /// `docker image inspect` of each of `references`, in one call.
    pub fn inspect(&self, references: &[&str]) -> Result<Vec<Inspected>, Failure> {
        let mut args = vec!["image", "inspect", "--format", "{{json .}}"];
        args.extend_from_slice(references);
        let out = self.read(&args, READ)?;
        Ok(out
            .lines()
            .filter_map(|line| layerfs_home::json::parse(line).ok())
            .filter_map(|value| Inspected::of(&value))
            .collect())
    }

    /// The tagged images this Docker holds, inspected: one `docker image ls`
    /// and one batched `docker image inspect`. Untagged images are left out.
    pub fn images(&self) -> Result<Vec<Inspected>, Failure> {
        let listed = self.read(
            &["image", "ls", "--format", "{{.Repository}}:{{.Tag}}"],
            READ,
        )?;
        let mut refs: Vec<&str> = listed
            .lines()
            .map(str::trim)
            .filter(|r| !r.is_empty() && !r.contains("<none>"))
            .collect();
        refs.sort();
        refs.dedup();
        if refs.is_empty() {
            return Ok(Vec::new());
        }
        let mut images: Vec<Inspected> = Vec::new();
        for image in self.inspect(&refs)? {
            match images.iter_mut().find(|i| i.id == image.id) {
                Some(known) => {
                    known.refs.extend(image.refs);
                    known.refs.sort();
                    known.refs.dedup();
                }
                None => images.push(image),
            }
        }
        Ok(images)
    }

    /// Starts `docker args...` with its stdout to read as it comes.
    pub fn stream(&self, args: &[&str]) -> Result<Child, Failure> {
        let mut command = self.command(args)?;
        command.stdout(Stdio::piped()).stderr(Stdio::null());
        command
            .spawn()
            .map_err(|e| Failure::Error(format!("docker: {e}")))
    }
}

/// Reads a pipe to its end on a thread of its own.
fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    })
}

/// What `docker image inspect` says of one image.
#[derive(Clone, Debug, PartialEq)]
pub struct Inspected {
    /// Its Docker image Id, `sha256:<64 hex>`.
    pub id: String,
    /// Its tags.
    pub refs: Vec<String>,
    /// `linux`, ...
    pub os: String,
    /// `arm64`, `amd64`, ...
    pub architecture: String,
    /// Its size as Docker reports it.
    pub size: u64,
    /// `Env`, `WorkingDir`, `User`, `Entrypoint`, `Cmd`.
    pub config: SdkValue,
}

impl Inspected {
    /// Reads one `{{json .}}` line.
    pub fn of(value: &SdkValue) -> Option<Self> {
        let id = value
            .get_text("Id")
            .filter(|id| crate::image_artifact::digest(id))?;
        let config = value.get("Config");
        let field = |name: &str| {
            config
                .and_then(|c| c.get(name))
                .cloned()
                .unwrap_or(SdkValue::Null)
        };
        Some(Self {
            id: id.to_owned(),
            refs: crate::image_artifact::texts(value.get("RepoTags"))
                .into_iter()
                .filter(|r| !r.contains("<none>"))
                .collect(),
            os: value.get_text("Os").unwrap_or("").to_owned(),
            architecture: value.get_text("Architecture").unwrap_or("").to_owned(),
            size: value.get_int("Size").unwrap_or(0).max(0) as u64,
            config: SdkValue::map([
                ("Env", field("Env")),
                ("WorkingDir", field("WorkingDir")),
                ("User", field("User")),
                ("Entrypoint", field("Entrypoint")),
                ("Cmd", field("Cmd")),
            ]),
        })
    }
}

/// The bytes a copy of an image of `size` needs on its machine (D8): the
/// staged tar and the unpacked tree, with 5% to spare.
pub fn need(size: u64) -> u64 {
    (size as u128 * 2 * 105).div_ceil(100) as u64
}

/// `machine.images`' Docker part for a machine of `architecture` with
/// `free` bytes: `(docker, available)`. Images already in `held` (by id, or
/// by a ref one of them lists) are left out; a Docker failure fills
/// `docker.available: false` and never fails the call.
pub fn candidates(
    docker: &Docker,
    architecture: &str,
    free: Option<u64>,
    held: &[(String, Vec<String>)],
) -> (SdkValue, Vec<SdkValue>) {
    let images = match docker.images() {
        Ok(images) => images,
        Err(failure) => {
            return (
                SdkValue::map([
                    ("available", SdkValue::Bool(false)),
                    ("reason", SdkValue::text(failure.reason())),
                    ("message", SdkValue::text(failure.message())),
                ]),
                Vec::new(),
            )
        }
    };
    let known = |image: &Inspected| {
        held.iter().any(|(id, refs)| {
            *id == image.id
                || image.refs.iter().any(|r| refs.contains(r))
                // the bundle image loaded into Docker by the docker machine
                || image.refs.iter().any(|r| {
                    r.strip_prefix("layerfs-cache/sandbox:")
                        .is_some_and(|hex| id.strip_prefix("sha256:") == Some(hex))
                })
        })
    };
    let rows = images
        .into_iter()
        .filter(|image| !image.refs.is_empty() && !known(image))
        .map(|image| {
            let need = need(image.size);
            let reason = if image.os != "linux" {
                Some(("os", image.os.clone()))
            } else if image.architecture != architecture {
                Some(("arch", image.architecture.clone()))
            } else {
                None
            };
            let mut fields = vec![
                ("id".to_owned(), SdkValue::text(&image.id)),
                (
                    "refs".to_owned(),
                    SdkValue::List(image.refs.into_iter().map(SdkValue::Text).collect()),
                ),
                ("size".to_owned(), SdkValue::Int(image.size as i64)),
                ("os".to_owned(), SdkValue::text(&image.os)),
                (
                    "architecture".to_owned(),
                    SdkValue::text(&image.architecture),
                ),
                ("usable".to_owned(), SdkValue::Bool(reason.is_none())),
                ("need".to_owned(), SdkValue::Int(need as i64)),
                (
                    "fits".to_owned(),
                    SdkValue::Bool(free.is_none_or(|free| need <= free)),
                ),
            ];
            if let Some((kind, value)) = reason {
                fields.push((
                    "reason".to_owned(),
                    SdkValue::map([
                        ("kind", SdkValue::text(kind)),
                        ("value", SdkValue::Text(value)),
                    ]),
                ));
            }
            SdkValue::Map(fields)
        })
        .collect();
    (SdkValue::map([("available", SdkValue::Bool(true))]), rows)
}
