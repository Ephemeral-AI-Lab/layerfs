//! The LayerFS root and the home found in it. A caller names a root, never
//! a home: the root it names, else `LAYERFS_ROOT`, else `$HOME/.layerfs`.
//! The root holds the home in `home/` and each client's folder beside it
//! (`dsh/`, `connect/`); a root that holds `layerfs.json` itself is the
//! home (a legacy `~/.layerfs`, or a home named directly), which `layerfs
//! home move <root>/home` moves into place.
use crate::reply::outcome::refused;
use layerfs_protocol::sdk::SdkRefusal;
use std::path::{Path, PathBuf};

/// The setting that names the root.
pub const ROOT_ENV: &str = "LAYERFS_ROOT";

/// The root a call uses: `explicit` when given, else `LAYERFS_ROOT` (not
/// empty), else `$HOME/.layerfs`. It must be an absolute path.
pub fn root(explicit: Option<&Path>) -> Result<PathBuf, SdkRefusal> {
    let root = match explicit {
        Some(path) => path.to_owned(),
        None => match std::env::var_os(ROOT_ENV) {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".layerfs"))
                .ok_or_else(|| refused("InvalidInput", "LAYERFS_ROOT is unset and so is HOME"))?,
        },
    };
    if !root.is_absolute() {
        return Err(refused(
            "InvalidInput",
            format!(
                "the LayerFS root must be an absolute path, not {}",
                root.display()
            ),
        ));
    }
    Ok(root)
}

/// The home in `root`: `root` itself while it holds a `layerfs.json` (one
/// `lstat`; an unreadable or broken one still names it, so its own refusal
/// is what the caller sees), else `root/home`.
pub fn home_in(root: &Path) -> PathBuf {
    match std::fs::symlink_metadata(root.join("layerfs.json")) {
        Ok(_) => root.to_owned(),
        Err(_) => root.join("home"),
    }
}

/// The home a call uses: the one found in its [`root`].
pub fn home(explicit: Option<&Path>) -> Result<PathBuf, SdkRefusal> {
    root(explicit).map(|root| home_in(&root))
}
