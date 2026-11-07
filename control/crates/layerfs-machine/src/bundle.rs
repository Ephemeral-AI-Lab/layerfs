//! The sealed Linux bundle (O16): the guest `layerfs` worker, the daemon,
//! the Sandbox helper and the Sandbox image archive, which every VM, ssh
//! host and `docker` on macOS needs. It lives in a per-build user cache
//! outside every home, `<cache>/layerfs/bundle/<version>/linux-<arch>/`,
//! with its hash in `bundle.json`; `tools/linux_bundle.py` builds and places
//! it. `LAYERFS_BUNDLE` (internal) names another folder.
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::refused;
#[cfg(feature = "machine")]
use layerfs_platform::Bundle;
use std::path::PathBuf;

/// The Sandbox image archive (a docker-archive) a Linux bundle carries.
pub const IMAGE_ARCHIVE: &str = "sandbox-image.tar";

/// The sealed bundle's folder for `os` on this host's architecture (O16):
/// `<cache>/layerfs/bundle/<version>/<os>-<arch>/`, the cache being
/// `XDG_CACHE_HOME` or `~/.cache`; for Linux, `LAYERFS_BUNDLE` (internal)
/// names another folder.
pub fn bundle_dir(os: &str) -> Result<PathBuf, SdkRefusal> {
    if os == "linux" {
        if let Some(dir) = std::env::var_os("LAYERFS_BUNDLE").filter(|d| !d.is_empty()) {
            return Ok(PathBuf::from(dir));
        }
    }
    let cache = match std::env::var_os("XDG_CACHE_HOME").filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".cache"))
            .ok_or_else(|| {
                refused(
                    "InvalidInput",
                    "HOME is unset, so the user cache is unknown",
                )
            })?,
    };
    Ok(cache
        .join("layerfs")
        .join("bundle")
        .join(env!("CARGO_PKG_VERSION"))
        .join(format!("{os}-{}", std::env::consts::ARCH)))
}

/// The bundle of this build, or `Unsupported` naming where it is expected.
#[cfg(feature = "machine")]
pub fn bundle() -> Result<Bundle, SdkRefusal> {
    let dir = bundle_dir("linux")?;
    let hash = crate::worker::sealed_hash(&dir)?.ok_or_else(|| {
            let mut refusal = refused(
                "Unsupported",
                format!(
                    "no sealed Linux bundle at {} (bundle.json with its hash); `python3 tools/linux_bundle.py` builds and places it (O16)",
                    dir.display()
                ),
            );
            refusal.detail = layerfs_bridge::control::SdkValue::map([(
                "missing",
                layerfs_bridge::control::SdkValue::List(vec![layerfs_bridge::control::SdkValue::text(
                    format!("bundle:{}", dir.display()),
                )]),
            )]);
            refusal
        })?;
    Ok(Bundle { dir, hash })
}
