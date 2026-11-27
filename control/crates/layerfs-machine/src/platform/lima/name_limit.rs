//! Lima's own limit on an instance's name (layerfs-project#180). Lima keeps
//! an instance's sockets in its folder, `<Lima home>/<instance>/`, and
//! `limactl create` refuses a name whose longest socket path there,
//! `ssh.sock.<16 digits>`, is not shorter than its `UNIX_PATH_MAX`: 104 on
//! macOS, the only host `lima` runs on ("must be less than
//! UNIX_PATH_MAX=104 characters", Lima 2.2.1). The Lima home is `LIMA_HOME`,
//! else `~/.lima`, with its symbolic links resolved when the folder exists
//! (`/var/...` counts as `/private/var/...`); `limactl` inherits both
//! settings from this process, so the same ones are read here.
use crate::setup::spec::NameLimit;
use std::path::PathBuf;

/// The longest socket name Lima makes in an instance's folder.
const LONGEST_SOCK: &str = "ssh.sock.1234567890123456";
/// Lima's `UNIX_PATH_MAX` on macOS: a socket path must be shorter.
const UNIX_PATH_MAX: usize = 104;

/// The Lima home as `limactl` resolves it; `None` when neither setting
/// names one (`limactl` then fails by itself).
fn home() -> Option<PathBuf> {
    let set = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    let dir = match set("LIMA_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(set("HOME")?).join(".lima"),
    };
    let dir: PathBuf = dir.components().collect();
    Some(std::fs::canonicalize(&dir).unwrap_or(dir))
}

/// The longest instance name whose `<Lima home>/<name>/ssh.sock.<16 digits>`
/// stays shorter than [`UNIX_PATH_MAX`].
pub(super) fn name_limit() -> Option<NameLimit> {
    let home = home()?;
    let taken = home.as_os_str().len() + 2 + LONGEST_SOCK.len();
    Some(NameLimit {
        longest: (UNIX_PATH_MAX - 1).saturating_sub(taken),
        why: format!(
            "Lima refuses an instance whose socket path `{}/<VM name>/ssh.sock.<16 digits>` \
             is not shorter than {UNIX_PATH_MAX} characters",
            home.display()
        ),
    })
}
