use crate::{CliError, CliResult};
use std::path::{Path, PathBuf};

pub fn default_context_location() -> PathBuf {
    if let Some(location) = std::env::var_os("LAYERFS_CONTEXT") {
        return location.into();
    }
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(std::env::temp_dir);
    state.join("layerfs/context")
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SavedContext {
    pub layer: Option<PathBuf>,
    pub stacks: Vec<PathBuf>,
    pub branches: Vec<SavedBranch>,
    pub active_stack: Option<PathBuf>,
    pub active_branch: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SavedBranch {
    pub location: PathBuf,
    pub parent_stack: Option<PathBuf>,
}

#[derive(Clone)]
pub(crate) struct ContextPaths {
    pub context: PathBuf,
    pub runtime: PathBuf,
    pub socket: PathBuf,
    pub lock: PathBuf,
    pub pid: PathBuf,
}

impl ContextPaths {
    pub(crate) fn new(context: impl AsRef<Path>) -> CliResult<Self> {
        let context = absolute(context.as_ref())?;
        use std::os::unix::ffi::OsStrExt;
        let hash = format!("{:x}", stable_hash(context.as_os_str().as_bytes()));
        let runtime = std::env::temp_dir().join("layerfs").join(hash);
        Ok(Self {
            context,
            socket: runtime.join("control.sock"),
            lock: runtime.join("host.lock"),
            pid: runtime.join("host.pid"),
            runtime,
        })
    }

    pub(crate) fn prepare(&self) -> CliResult<()> {
        std::fs::create_dir_all(&self.runtime).map_err(io)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&self.runtime, std::fs::Permissions::from_mode(0o700)).map_err(io)
    }
}

impl SavedContext {
    pub(crate) fn load(path: &Path) -> CliResult<Self> {
        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(io(error)),
        };
        let mut context = Self::default();
        for line in contents.lines() {
            let fields = line.split(' ').collect::<Vec<_>>();
            match fields.as_slice() {
                ["version", "1"] => {}
                ["layer", path] => context.layer = Some(decode_path(path)?),
                ["stack", path] => context.stacks.push(decode_path(path)?),
                ["branch", path, "-"] => context.branches.push(SavedBranch {
                    location: decode_path(path)?,
                    parent_stack: None,
                }),
                ["branch", path, parent] => context.branches.push(SavedBranch {
                    location: decode_path(path)?,
                    parent_stack: Some(decode_path(parent)?),
                }),
                ["active-stack", path] => context.active_stack = Some(decode_path(path)?),
                ["active-branch", path] => context.active_branch = Some(decode_path(path)?),
                _ => return Err(CliError::Context("context format".to_owned())),
            }
        }
        Ok(context)
    }

    pub(crate) fn save(&self, path: &Path) -> CliResult<()> {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(io)?;
        writeln!(output, "version 1").map_err(io)?;
        if let Some(layer) = &self.layer {
            writeln!(output, "layer {}", encode_path(layer)).map_err(io)?;
        }
        for stack in &self.stacks {
            writeln!(output, "stack {}", encode_path(stack)).map_err(io)?;
        }
        for branch in &self.branches {
            writeln!(
                output,
                "branch {} {}",
                encode_path(&branch.location),
                branch
                    .parent_stack
                    .as_ref()
                    .map(|path| encode_path(path))
                    .unwrap_or_else(|| "-".to_owned())
            )
            .map_err(io)?;
        }
        if let Some(stack) = &self.active_stack {
            writeln!(output, "active-stack {}", encode_path(stack)).map_err(io)?;
        }
        if let Some(branch) = &self.active_branch {
            writeln!(output, "active-branch {}", encode_path(branch)).map_err(io)?;
        }
        output.sync_all().map_err(io)?;
        std::fs::rename(&temporary, path).map_err(io)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(io)
    }
}

fn absolute(path: &Path) -> CliResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(std::env::current_dir().map_err(io)?.join(path))
    }
}

fn encode_path(path: &Path) -> String {
    use std::fmt::Write as _;
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str()
        .as_bytes()
        .iter()
        .fold(String::new(), |mut value, byte| {
            let _ = write!(value, "{byte:02x}");
            value
        })
}

fn decode_path(value: &str) -> CliResult<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    if value.len() % 2 != 0 {
        return Err(CliError::Context("path encoding".to_owned()));
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        bytes.push((hex(pair[0])? << 4) | hex(pair[1])?);
    }
    Ok(std::ffi::OsString::from_vec(bytes).into())
}

fn hex(value: u8) -> CliResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(CliError::Context("path encoding".to_owned())),
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |value, byte| {
        (value ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn io(error: std::io::Error) -> CliError {
    CliError::Io(error.to_string())
}
