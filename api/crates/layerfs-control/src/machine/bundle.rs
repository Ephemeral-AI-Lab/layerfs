//! The sealed Linux bundle (O16): the guest `layerfs` worker, the daemon,
//! the Sandbox helper and the Sandbox image archive, which every VM, ssh
//! host and `docker` on macOS needs. It lives in a per-build user cache
//! outside every home, `<cache>/layerfs/bundle/<version>/linux-<arch>/`,
//! with its hash in `bundle.json`; `tools/linux_bundle.py` builds and places
//! it. `LAYERFS_BUNDLE` (internal) names another folder.
use crate::common::outcome::refused;
use layerfs_bridge::control::SdkRefusal;
use layerfs_machine::Bundle;

/// The bundle of this build, or `Unsupported` naming where it is expected.
pub fn bundle() -> Result<Bundle, SdkRefusal> {
    let dir = crate::sandbox::deploy::bundle_dir("linux")?;
    let hash = super::worker::sealed_hash(&dir)?.ok_or_else(|| {
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
