//! The Sandbox runtime layer (R5): the dynamic loader and libraries the
//! daemon was linked against, a static `bash` and `busybox`, and glibc's
//! `getent`, sealed into the Linux bundle as `runtime/` and bound read-only
//! into every namespaces Sandbox at [`GUEST`]. The daemon runs inside the
//! image's root, so without it the daemon loads the image's own `ld.so` and
//! libraries: an image with an older glibc (Debian bullseye), musl (alpine)
//! or no libc at all (distroless) could not start it, and an image with no
//! `bash` could run no command.
//!
//! The daemon is started through the bundled loader ([`command`]), which
//! reads no `/etc/ld.so.cache` (`--inhibit-cache`) and searches only the
//! bundled `lib/` (`--library-path`); it tells the daemon the runtime it runs
//! under in [`ENVIRONMENT`], and the daemon's name resolver runs the bundled
//! `getent` the same way ([`from_environment`]). A command sees `bin/` only
//! at the end of its `PATH`, so the image's own tools win.
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Command,
};

/// Where the init stage binds a Sandbox's runtime, inside its root.
pub const GUEST: &str = "/.layerfs/runtime";

/// Set (to [`GUEST`]) in the environment of a daemon started through the
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

/// `path`, a command's search path, with the runtime's `bin` at its end,
/// once: the image's own programs come first, and a bare `bash` that the
/// image lacks is the runtime's.
pub fn search_path(path: &OsStr, runtime: &Path) -> OsString {
    let bin = runtime.join("bin");
    if path.is_empty() {
        return bin.into_os_string();
    }
    if std::env::split_paths(path).any(|entry| entry == bin) {
        return path.to_owned();
    }
    let mut joined = path.to_owned();
    joined.push(":");
    joined.push(bin);
    joined
}
