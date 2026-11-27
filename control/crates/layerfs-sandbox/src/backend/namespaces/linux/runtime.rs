//! The Sandbox runtime layer (R5): the dynamic loader and libraries the
//! daemon was linked against, a static `bash` and `busybox`, and glibc's
//! `getent`, sealed into the Linux bundle as `runtime/` and bound read-only
//! into every namespaces Sandbox at [`GUEST`]. The daemon runs inside the
//! image's root, so without it the daemon loads the image's own `ld.so` and
//! libraries: an image with an older glibc (Debian bullseye), musl (alpine)
//! or no libc at all (distroless) could not start it, and an image with no
//! `bash` could run no command.
//!
//! The daemon is started through the bundled loader (`layerfs_os::runtime`:
//! `command`), which reads no `/etc/ld.so.cache` (`--inhibit-cache`) and
//! searches only the bundled `lib/` (`--library-path`); it tells the daemon
//! the runtime it runs under, and the daemon's name resolver runs the bundled
//! `getent` the same way. A command sees `bin/` only at the end of its
//! `PATH`, so the image's own tools win.
use std::{
    ffi::{OsStr, OsString},
    path::Path,
};

/// Where the init stage binds a Sandbox's runtime, inside its root.
pub const GUEST: &str = "/.layerfs/runtime";

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
