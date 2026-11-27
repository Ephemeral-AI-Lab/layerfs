//! The Sandbox runtime layer as the process running under it sees it (R5):
//! a daemon started through the bundled loader is told the runtime it runs
//! under in [`ENVIRONMENT`], and what it runs itself, such as the name
//! resolver's `getent` (`crate::network`), goes through the same loader
//! ([`command`], [`from_environment`]). The layer itself and where a Sandbox
//! binds it are the Sandbox backend's (`layerfs_sandbox`
//! `backend/namespaces/linux/runtime.rs`).
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

/// Set (to the Sandbox's `GUEST` path) in the environment of a daemon started through the
/// runtime's loader, so what it runs itself goes through the same loader.
pub const ENVIRONMENT: &str = "LAYERFS_SANDBOX_RUNTIME";

/// The dynamic loader's file name in the runtime's `lib/` on this
/// architecture: the `PT_INTERP` glibc gives its binaries here.
#[cfg(target_arch = "aarch64")]
pub const LOADER: Option<&str> = Some("ld-linux-aarch64.so.1");
/// The dynamic loader's file name in the runtime's `lib/` on this
/// architecture: the `PT_INTERP` glibc gives its binaries here.
#[cfg(target_arch = "x86_64")]
pub const LOADER: Option<&str> = Some("ld-linux-x86-64.so.2");
/// No runtime layer is built for this architecture.
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
pub const LOADER: Option<&str> = None;

/// The loader of the runtime at `runtime`, `<runtime>/lib/<LOADER>`; `None`
/// on an architecture with no runtime layer.
pub fn loader(runtime: &Path) -> Option<PathBuf> {
    LOADER.map(|name| runtime.join("lib").join(name))
}

/// Whether `runtime` holds a runtime layer: its loader is a file.
pub fn holds(runtime: &Path) -> bool {
    loader(runtime).is_some_and(|loader| loader.is_file())
}

/// `program` run by the loader of the runtime at `runtime`, with only the
/// runtime's `lib/` searched and no loader cache read; its own arguments
/// follow. `None` on an architecture with no runtime layer.
pub fn command(runtime: &Path, program: impl AsRef<OsStr>) -> Option<Command> {
    let mut command = Command::new(loader(runtime)?);
    command
        .arg("--inhibit-cache")
        .arg("--library-path")
        .arg(runtime.join("lib"))
        .arg(program);
    Some(command)
}

/// The runtime this process was started under ([`ENVIRONMENT`]), when it
/// is an absolute path that holds one.
pub fn from_environment() -> Option<PathBuf> {
    let runtime = PathBuf::from(std::env::var_os(ENVIRONMENT)?);
    (runtime.is_absolute() && holds(&runtime)).then_some(runtime)
}
