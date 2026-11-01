//! The sealed Linux bundle (O16): the guest `layerfs` worker, the daemon,
//! the Sandbox helper and the Sandbox image, which every VM, ssh host and
//! `docker` on macOS needs. It lives in a per-build user cache outside every
//! home, `<cache>/layerfs/bundle/<version>/`, with its hash in
//! `bundle.json`. `LAYERFS_BUNDLE` (internal) names another folder.
//! Fetching it into the cache is not built here: a build places it.
use crate::host::outcome::refused;
use layerfs_bridge::control::SdkRefusal;
use layerfs_sandbox::machine::Bundle;
use std::path::PathBuf;

/// The bundle of this build, or `Unsupported` naming where it is expected.
pub fn bundle() -> Result<Bundle, SdkRefusal> {
    let dir = match std::env::var_os("LAYERFS_BUNDLE") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => cache()?
            .join("layerfs")
            .join("bundle")
            .join(env!("CARGO_PKG_VERSION")),
    };
    let manifest = crate::home::read_record(&dir.join("bundle.json"))?;
    let hash = manifest
        .as_ref()
        .and_then(|m| m.get_text("hash"))
        .map(str::to_owned)
        .ok_or_else(|| {
            let mut refusal = refused(
                "Unsupported",
                format!(
                    "no sealed Linux bundle at {} (bundle.json with its hash); this build expects it in its user cache (O16)",
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

fn cache() -> Result<PathBuf, SdkRefusal> {
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".cache"))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "HOME is unset, so the user cache is unknown",
            )
        })
}
